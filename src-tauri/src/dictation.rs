use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use opendictate_core::audio::vad::{
    apply_vad, sensitivity_to_energy_threshold, sensitivity_to_silero_threshold, SileroVad,
};
use opendictate_core::stt::engine::{ModelKind, SttEngine};
use opendictate_core::stt::models;
use opendictate_core::stt::streaming::StreamingRecognizer;
use tauri::{AppHandle, Emitter, Manager};

use crate::db;
use crate::inject;
use crate::dock;
use crate::notify;
use crate::state::{AppState, HistoryEntry, StreamingPipe, TranscriptResult};


/// Resolves the execution-provider to request for new engines from the
/// current settings. "auto" defers to the platform policy in
/// [`opendictate_core::stt::provider::Provider::auto`]; engines always fall
/// back to CPU internally when a GPU provider cannot be created.
fn desired_provider(state: &AppState) -> opendictate_core::stt::provider::Provider {
    let mode = state
        .settings
        .lock()
        .map(|s| s.gpu.clone())
        .unwrap_or_default();
    opendictate_core::stt::provider::resolve(&mode)
}


pub fn start(app: &AppHandle, state: &AppState, test: bool) -> Result<(), String> {
    // Determine whether handsfree mode is the current owner of the recorder.
    // When handsfree is active it keeps the mic open continuously; cancelling
    // the stream here would stop handsfree's mic, causing a race with the
    // handsfree loop that will try to restart it.
    let handsfree_owns_mic = state.handsfree_active.load(Ordering::SeqCst);

    if state.recorder.is_recording() {
        if handsfree_owns_mic {
            // Handsfree holds the mic. Clear the buffer so normal dictation
            // starts with a clean slate from this moment forward, but keep the
            // hardware stream alive to avoid the cancel→restart race.
            state.recorder.clear_buffer();
            log::info!("start: handsfree owns mic; cleared buffer for fresh dictation session");
        } else {
            log::warn!("recording already in progress; cancelling stale recording");
            state.recorder.cancel().map_err(|e| e.to_string())?;
        }
    }

    state.set_test_mode(test);

    let streaming = !test && is_streaming_enabled(state);
    if streaming {
        let model_id = selected_model_id(state);
        if !models::is_model_installed(&model_id) {
            return Err(format!(
                "STT model '{model_id}' is not installed — download it in Settings → Models"
            ));
        }
    }

    // Only start the recorder if it is not already running (handsfree path
    // keeps it open; otherwise open it fresh).
    if !state.recorder.is_recording() {
        let mic = state.settings.lock().map(|s| s.mic.clone()).unwrap_or(None);
        match mic {
            Some(name) if !name.is_empty() => state.recorder.start_with_name(&name),
            _ => state.recorder.start(),
        }
        .map_err(|e| e.to_string())?;
    }


    dock::set_state(app, "listening", None);
    dock::set_caption(app, Some("listening…"));
    if !test {
        notify::notify("Dictation on", "Recording — press Ctrl+K to stop");
        play_sound(state, crate::audio::SoundEvent::Listening);
    }

    if streaming {
        if let Err(e) = spawn_streaming(app, state) {
            let _ = state.recorder.cancel();
            dock::set_caption(app, None);
            dock::set_state(app, "hidden", None);
            return Err(e);
        }
    } else if !test && is_continuous_enabled(state) {
        state.set_continuous(true);
        spawn_continuous_loop(app, state);
    }

    spawn_level_emitter(app, state);
    if models::is_caption_model_ready() {
        spawn_caption_loop(app, state);
    } else if !test {
        // Captions are core UX: fetch the small caption engine quietly so
        // the next dictation has them.
        let app2 = app.clone();
        std::thread::spawn(move || {
            let st = state_from_app(&app2);
            let _ = opendictate_core::stt::models::ensure_model(
                models::CAPTION_MODEL_ID,
                &mut |_file, _received, _total| {},
            );
            log::info!("caption model ensured for next dictation");
            drop(st);
        });
    }

    // Mark that a user-initiated dictation session is now open. The toggle
    // hotkey reads this to distinguish "user is dictating" from "handsfree is
    // just holding the mic warm" so a second press correctly stops the session.
    state.user_dictation_active.store(true, Ordering::SeqCst);
    // Tell the UI: hotkey-driven start/stop never passes through the frontend,
    // so the main window would otherwise keep showing "Idle" and the Record
    // button would start a second session instead of stopping the first.
    let _ = app.emit("recording-changed", serde_json::json!({ "recording": true }));
    Ok(())
}

pub fn stop(app: &AppHandle, state: &AppState) -> Result<TranscriptResult, String> {
    state.set_continuous(false);
    state.user_dictation_active.store(false, Ordering::SeqCst);
    let _ = app.emit("recording-changed", serde_json::json!({ "recording": false }));
    if state.is_streaming_active() {
        return stop_streaming(app, state);
    }
    if !state.recorder.is_recording() {
        return Err("no recording in progress".to_string());
    }

    let audio = state.recorder.stop().map_err(|e| e.to_string())?;
    let test = state.is_test_mode();
    dock::set_state(app, "transcribing", None);
    dock::set_caption(app, Some("transcribing…"));

    process_utterance_on_worker(app, audio, test, false)
}

fn process_utterance_on_worker(
    app: &AppHandle,
    audio: Vec<f32>,
    test: bool,
    continuous: bool,
) -> Result<TranscriptResult, String> {
    let app = app.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("opendictate-inference".to_string())
        .spawn(move || {
            let state = state_from_app(&app);
            let result = process_utterance(&app, &state, &audio, test, continuous);
            let _ = sender.send(result);
        })
        .map_err(|e| format!("failed to start inference worker: {e}"))?;
    receiver
        .recv()
        .map_err(|_| "inference worker exited unexpectedly".to_string())?
}

pub fn cancel(app: &AppHandle, state: &AppState) -> Result<(), String> {
    state.set_continuous(false);
    state.set_streaming(false);
    state.user_dictation_active.store(false, Ordering::SeqCst);
    let _ = app.emit("recording-changed", serde_json::json!({ "recording": false }));
    state.caption_active.store(false, Ordering::SeqCst);
    *state.caption_stream.lock().map_err(|e| e.to_string())? = None;
    // Clear the streaming pipe before the early return below: when cancel is
    // invoked after the recorder already stopped (e.g. a failed
    // stop_streaming) the pipe used to survive with a stale session.
    *state
        .stream
        .lock()
        .map_err(|e| e.to_string())? = None;
    if !state.recorder.is_recording() {
        dock::set_caption(app, None);
        dock::set_state(app, "hidden", None);
        return Ok(());
    }
    state.recorder.cancel().map_err(|e| e.to_string())?;
    state.set_test_mode(false);
    dock::set_caption(app, None);
    dock::set_state(app, "hidden", None);
    Ok(())
}

fn continuous_flag_from_app(app: &AppHandle) -> bool {
    app.state::<AppState>().continuous.load(Ordering::SeqCst)
}

fn is_continuous_enabled(state: &AppState) -> bool {
    state
        .settings
        .lock()
        .map(|s| s.continuous)
        .unwrap_or(false)
}

fn spoken_punctuation_enabled(state: &AppState) -> bool {
    state
        .settings
        .lock()
        .map(|s| s.spoken_punctuation)
        .unwrap_or(false)
}

fn audio_feedback(state: &AppState) -> (bool, f32) {
    state
        .settings
        .lock()
        .map(|s| (s.audio_feedback, s.audio_feedback_volume))
        .unwrap_or((false, 0.5))
}

fn play_sound(state: &AppState, event: crate::audio::SoundEvent) {
    let (enabled, volume) = audio_feedback(state);
    if enabled {
        crate::audio::play_event(volume, event);
    }
}

fn sensitivity(state: &AppState) -> f32 {
    state
        .settings
        .lock()
        .map(|s| s.vad_sensitivity)
        .unwrap_or(0.5)
}

/// Keeps the mic open: each utterance is endpointed by trailing silence,
/// transcribed, and inserted, then listening resumes. Killed when the user
/// toggles stop (clears the continuous flag) or calls cancel.
fn spawn_continuous_loop(app: &AppHandle, state: &AppState) {
    const SILENCE_TIMEOUT: Duration = Duration::from_millis(1400);
    const MIN_UTTERANCE: Duration = Duration::from_millis(600);
    // Endpoint long continuous speech before the recorder's
    // MAX_RECORDING_SAMPLES cap (120 s) starts draining the front of the
    // buffer, which would silently truncate the utterance. Slightly under
    // the cap so the forced endpoint lands while everything is still buffered.
    const MAX_UTTERANCE: Duration = Duration::from_secs(110);

    let app = app.clone();
    let recorder = Arc::clone(&state.recorder);
    let continuous = Arc::clone(&state.continuous);
    std::thread::spawn(move || {
        let mut silent_since = Instant::now();
        let mut utterance_started = Instant::now();

        while continuous.load(std::sync::atomic::Ordering::SeqCst) {
            if !recorder.is_recording() {
                break;
            }
            let rms = recorder.current_rms();
            let energy_threshold = sensitivity_to_energy_threshold(
                state_sensitivity(&app),
            );
            if rms < energy_threshold {
                if silent_since.elapsed() >= SILENCE_TIMEOUT
                    && utterance_started.elapsed() >= MIN_UTTERANCE
                {
                    let audio = match recorder.stop() {
                        Ok(a) => a,
                        Err(e) => {
                            log::warn!("continuous: stop failed: {e}");
                            break;
                        }
                    };
                    if !continuous.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    dock::set_state(&app, "transcribing", None);
                    let _ = process_utterance_on_worker(&app, audio, false, true);
                    if !continuous.load(std::sync::atomic::Ordering::SeqCst) {
                        break;
                    }
                    match restart_recorder(&app) {
                        Ok(()) => {
                            silent_since = Instant::now();
                            utterance_started = Instant::now();
                        }
                        Err(e) => {
                            log::warn!("continuous: restart failed: {e}");
                            break;
                        }
                    }
                }
            } else {
                silent_since = Instant::now();
            }
            // Max-utterance guard: forces an endpoint even mid-speech so a
            // single uninterrupted monologue cannot overflow the buffer cap.
            if utterance_started.elapsed() >= MAX_UTTERANCE {
                log::info!("continuous: max utterance length reached; endpointing early");
                let audio = match recorder.stop() {
                    Ok(a) => a,
                    Err(e) => {
                        log::warn!("continuous: stop failed: {e}");
                        break;
                    }
                };
                if !continuous.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                dock::set_state(&app, "transcribing", None);
                let _ = process_utterance_on_worker(&app, audio, false, true);
                if !continuous.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                match restart_recorder(&app) {
                    Ok(()) => {
                        silent_since = Instant::now();
                        utterance_started = Instant::now();
                    }
                    Err(e) => {
                        log::warn!("continuous: restart failed: {e}");
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(60));
        }
        continuous.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

fn restart_recorder(app: &AppHandle) -> Result<(), String> {
    let state = state_from_app(app);
    let mic = state
        .settings
        .lock()
        .map(|s| s.mic.clone())
        .unwrap_or(None);
    match mic {
        Some(name) if !name.is_empty() => state.recorder.start_with_name(&name),
        _ => state.recorder.start(),
    }
    .map_err(|e| e.to_string())?;

    if !state.handsfree_active.load(Ordering::SeqCst) || state.handsfree_awake.load(Ordering::SeqCst) || state.user_dictation_active.load(Ordering::SeqCst) {
        dock::set_state(app, "listening", None);
    }
    Ok(())
}

pub fn state_from_app(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
}

fn state_sensitivity(app: &AppHandle) -> f32 {
    let state = state_from_app(app);
    sensitivity(&state)
}

fn is_streaming_enabled(state: &AppState) -> bool {
    let model_id = selected_model_id(state);
    models::is_streaming_model(&model_id)
}

fn selected_model_id(state: &AppState) -> String {
    state
        .settings
        .lock()
        .map(|s| {
            if s.stt_model.is_empty() {
                models::STT_MODEL_ID.to_string()
            } else {
                s.stt_model.clone()
            }
        })
        .unwrap_or_else(|_| models::STT_MODEL_ID.to_string())
}

/// Builds the streaming pipe (recognizer + session) and starts the capture
/// loop. Streaming ignores the continuous toggle: it always runs until stop.
pub fn spawn_streaming(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let load_started = Instant::now();
    let model_id = selected_model_id(state);
    let dir = models::model_dir_for(&model_id);
    let recognizer = {
        let mut cache = state
            .streaming_engine
            .lock()
            .map_err(|e| e.to_string())?;
        if let Some(existing) = cache.as_ref() {
            if existing.model_id == model_id {
                log::debug!("streaming engine cache hit for {}", model_id);
                Arc::clone(&existing.recognizer)
            } else {
                let created = {
                    let prov = desired_provider(state);
                    let rec = StreamingRecognizer::new_with_provider(&dir, Some("nemo_transducer"), prov)
                        .map_err(|e| e.to_string())?;
                    if rec.provider != "cpu" {
                        state.gpu_active.store(true, Ordering::SeqCst);
                    }
                    Arc::new(rec)
                };
                *cache = Some(crate::state::CachedStreamingEngine {
                    model_id: model_id.clone(),
                    recognizer: Arc::clone(&created),
                });
                created
            }
        } else {
            let created = {
                    let prov = desired_provider(state);
                    let rec = StreamingRecognizer::new_with_provider(&dir, Some("nemo_transducer"), prov)
                        .map_err(|e| e.to_string())?;
                    if rec.provider != "cpu" {
                        state.gpu_active.store(true, Ordering::SeqCst);
                    }
                    Arc::new(rec)
                };
            *cache = Some(crate::state::CachedStreamingEngine {
                model_id: model_id.clone(),
                recognizer: Arc::clone(&created),
            });
            created
        }
    };
    log::info!("streaming engine ready in {} ms", load_started.elapsed().as_millis());
    let dictionary = dictionary_terms(state);
    let hotwords = dictionary_hotwords(&dictionary);
    let session = recognizer.create_session(hotwords.as_deref());

    let pipe = StreamingPipe {
        recognizer,
        session,
        watermark: 0,
        total_fed: 0,
    };
    *state.stream.lock().map_err(|e| e.to_string())? = Some(pipe);
    state.set_streaming(true);

    spawn_streaming_loop(app, state);
    Ok(())
}

/// Feeds captured samples into the recognizer in small chunks, emits partial
/// hypotheses as live captions, and commits each endpointed utterance.
fn spawn_streaming_loop(app: &AppHandle, state: &AppState) {
    const STREAM_POLL_INTERVAL: Duration = Duration::from_millis(100);
    let app = app.clone();
    let recorder = Arc::clone(&state.recorder);
    let stream = Arc::clone(&state.stream);
    let stream_active = Arc::clone(&state.stream_active);
    std::thread::spawn(move || {
        log::info!("stream loop: started");
        loop {
            if !stream_active.load(Ordering::SeqCst) || !recorder.is_recording() {
                log::info!("stream loop: exiting (active={}, recording={})", stream_active.load(Ordering::SeqCst), recorder.is_recording());
                break;
            }
            let mut pipe_guard = match stream.lock() {
                Ok(g) => g,
                Err(_) => {
                    log::info!("stream loop: exiting (pipe lock poisoned)");
                    break;
                }
            };
            let Some(pipe) = pipe_guard.as_mut() else {
                log::info!("stream loop: exiting (no pipe)");
                break;
            };
            if !stream_active.load(Ordering::SeqCst) {
                log::info!("stream loop: exiting (active cleared)");
                break;
            }

            let samples = recorder.take_since(&mut pipe.watermark);
            if !samples.is_empty() && pipe.total_fed % 33 == 0 {
                let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
                log::info!(
                    "stream loop: fed {} samples (rms {rms:.4}), partial so far: {}",
                    samples.len(),
                    pipe.recognizer.result(&pipe.session).len()
                );
            }
            pipe.total_fed += samples.len();
            drop(pipe_guard);
            if samples.is_empty() {
                std::thread::sleep(STREAM_POLL_INTERVAL);
                continue;
            }
            pipe_guard = match stream.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            let Some(pipe) = pipe_guard.as_mut() else {
                break;
            };
            if !samples.is_empty() {
                pipe.recognizer.accept(&pipe.session, &samples);
                // Decode everything accepted so far; without this the decode
                // queue grows unboundedly and partials lag further behind.
                pipe.recognizer.drain(&pipe.session);
            }

            if pipe.recognizer.is_ready(&pipe.session) {
                let text = pipe.recognizer.result(&pipe.session);
                // When the zipformer caption engine is live it owns partial
                // emission (it decodes in real time; this model may lag by
                // minutes). Endpoint commits below still come from here.
                let captions_owned = state_from_app(&app)
                    .caption_active
                    .load(Ordering::SeqCst);
                if !text.is_empty() && !captions_owned {
                    let _ = app.emit(
                        "partial",
                        serde_json::json!({ "text": text, "streaming": true }),
                    );
                    dock::set_caption(&app, Some(&text));
                }
            }

            if pipe.recognizer.is_endpoint(&pipe.session) {
                let mut text = pipe.recognizer.result(&pipe.session);
                if spoken_punctuation_enabled(&state_from_app(&app)) {
                    text = opendictate_core::text::map_spoken_punctuation(&text);
                }
                let duration_ms = pipe.session.started_at.elapsed().as_millis() as u64;
                // Reset while the pipe is still locked (fast, local), then
                // release the lock BEFORE the slow commit path below: voice
                // polish is a network round-trip and holding the pipe mutex
                // through it would stall partial-caption feeding for seconds.
                pipe.recognizer.reset(&mut pipe.session);
                drop(pipe_guard);
                if !text.is_empty() {
                    let handled = handle_voice_action_or_snippet(&app, &state_from_app(&app), &text);
                    if handled.is_none() {
                        let text = apply_voice_polish(&state_from_app(&app), &text);
                        let _ = commit_text(
                            &app,
                            &state_from_app(&app),
                            &text,
                            duration_ms,
                            true,
                            false,
                        );
                    }
                }
                std::thread::sleep(STREAM_POLL_INTERVAL);
                continue;
            }

            drop(pipe_guard);
            std::thread::sleep(STREAM_POLL_INTERVAL);
        }
        stream_active.store(false, Ordering::SeqCst);
        dock::set_caption(&app, None);
    });
}


/// Live captions from the internal zipformer caption engine. Runs during any
/// recording (offline or streaming accuracy path) and owns `partial`
/// emission so captions stay real-time even when the selected STT model
/// decodes slower than the mic speaks.
fn spawn_caption_loop(app: &AppHandle, state: &AppState) {
    const CAPTION_POLL_INTERVAL: Duration = Duration::from_millis(100);
    let app = app.clone();
    let recorder = Arc::clone(&state.recorder);
    let caption_stream = Arc::clone(&state.caption_stream);
    let caption_active = Arc::clone(&state.caption_active);

    let Ok(mut cache) = state.caption_engine.lock() else {
        log::warn!("captions unavailable: engine cache poisoned");
        return;
    };
    let recognizer = if let Some(existing) = cache.as_ref() {
        Arc::clone(&existing.recognizer)
    } else {
        let created = match StreamingRecognizer::new(&models::caption_model_dir()) {
            Ok(r) => Arc::new(r),
            Err(e) => {
                log::warn!("captions unavailable: {e}");
                return;
            }
        };
        *cache = Some(crate::state::CachedStreamingEngine {
            model_id: models::CAPTION_MODEL_ID.to_string(),
            recognizer: Arc::clone(&created),
        });
        created
    };
    drop(cache);
    // Captions never use dictionary hotwords; those belong to the accuracy
    // model that produces the final transcript.
    let session = recognizer.create_session(None);
    *caption_stream.lock().unwrap_or_else(|e| e.into_inner()) = Some(StreamingPipe {
        recognizer,
        session,
        watermark: 0,
        total_fed: 0,
    });
    caption_active.store(true, Ordering::SeqCst);

    std::thread::spawn(move || {
        log::info!("caption loop: started");
        let continuous = continuous_flag_from_app(&app);
        while caption_active.load(Ordering::SeqCst)
            && (recorder.is_recording() || continuous)
        {
            let mut pipe_guard = match caption_stream.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            let Some(pipe) = pipe_guard.as_mut() else {
                break;
            };
            let samples = recorder.take_since(&mut pipe.watermark);
            pipe.total_fed += samples.len();
            if !samples.is_empty() {
                pipe.recognizer.accept(&pipe.session, &samples);
                pipe.recognizer.drain(&pipe.session);
                let text = pipe.recognizer.result(&pipe.session);
                drop(pipe_guard);
                if !text.is_empty() {
                    let _ = app.emit(
                        "partial",
                        serde_json::json!({ "text": text, "streaming": true }),
                    );
                    dock::set_caption(&app, Some(&text));
                }
                std::thread::sleep(CAPTION_POLL_INTERVAL);
                continue;
            }
            drop(pipe_guard);
            std::thread::sleep(CAPTION_POLL_INTERVAL);
        }
        caption_active.store(false, Ordering::SeqCst);
        *caption_stream.lock().unwrap_or_else(|e| e.into_inner()) = None;
        log::info!("caption loop: exited");
    });
}

/// Finalizes a manual stop for streaming mode: consumes whatever remains in
/// the capture buffer, decodes it and commits the last utterance.
fn stop_streaming(app: &AppHandle, state: &AppState) -> Result<TranscriptResult, String> {
    state.set_streaming(false);
    if !state.recorder.is_recording() {
        return Err("no recording in progress".to_string());
    }

    let test = state.is_test_mode();

    // The loop checks the active flag under the pipe lock, so once we hold
    // the lock no more samples are consumed by the loop.
    let mut pipe_guard = state.stream.lock().map_err(|e| e.to_string())?;
    let pipe = pipe_guard
        .as_mut()
        .ok_or_else(|| "streaming is not active".to_string())?;

    // Capture whatever is left in the live buffer before releasing the mic.
    let tail = state.recorder.take_since(&mut pipe.watermark);
    if let Err(e) = state.recorder.stop() {
        log::warn!("streaming stop: recorder stop failed: {e}");
    }

    pipe.recognizer.accept(&pipe.session, &tail);
    // The online decoder lags behind real time; drain everything accepted so
    // the final transcript reflects the whole utterance. This can take a
    // while on slow CPUs — keep the dock caption alive while it runs.
    log::info!("streaming stop: draining decode backlog");
    let drain_started = Instant::now();
    dock::set_caption(app, Some("finalizing…"));
    pipe.recognizer.drain(&pipe.session);
    log::info!(
        "streaming stop: drained in {} ms",
        drain_started.elapsed().as_millis()
    );
    let mut text = pipe.recognizer.result(&pipe.session);
    if spoken_punctuation_enabled(state) {
        text = opendictate_core::text::map_spoken_punctuation(&text);
    }
    let duration_ms = pipe.session.started_at.elapsed().as_millis() as u64;
    let result = TranscriptResult {
        text: text.clone(),
        duration_ms,
    };

    drop(pipe_guard);
    *state.stream.lock().map_err(|e| e.to_string())? = None;
    dock::set_caption(app, None);

    if !test && !text.is_empty() {
        let snippet_text = handle_snippet_command(app, state, &text);
        if let Some(snippet_text) = snippet_text {
            if !snippet_text.is_empty() {
                return Ok(TranscriptResult {
                    text: snippet_text,
                    duration_ms,
                });
            }
        } else {
            let _ = commit_text(app, state, &text, duration_ms, true, true);
        }
    } else if !test {
        notify::notify("No speech detected", "Try again or check your microphone");
        play_sound(state, crate::audio::SoundEvent::Error);
        let app = app.clone();
        std::thread::spawn(move || {
            dock::set_caption(&app, Some("no speech detected"));
            std::thread::sleep(Duration::from_millis(2200));
            dock::set_caption(&app, None);
            dock::set_state(&app, "hidden", None);
        });
    }

    Ok(result)
}

fn process_utterance(
    app: &AppHandle,
    state: &AppState,
    audio: &[f32],
    test: bool,
    continuous: bool,
) -> Result<TranscriptResult, String> {
    let speech = run_vad(state, audio).map_err(|e| e.to_string())?;
    let (final_audio, duration_ms) = if speech.has_speech {
        (speech.trimmed_audio, speech.speech_duration_ms)
    } else if !continuous && opendictate_core::audio::vad::compute_rms(audio) > 0.002 {
        // Fallback for manual hotkey recordings: ensure quiet speech on quiet mics is never blocked.
        // Trim trailing silence using energy VAD to prevent Whisper repetition loops.
        let energy_trimmed = opendictate_core::audio::vad::apply_energy_vad_with_config(
            audio,
            &opendictate_core::audio::vad::VadConfig {
                energy_threshold: 0.002,
                frame_size: 480,
                min_speech_frames: 1,
                hangover_frames: 15,
            },
        );
        let trimmed = if energy_trimmed.has_speech {
            energy_trimmed.trimmed_audio
        } else {
            audio.to_vec()
        };
        let dur = (trimmed.len() as u64 * 1000) / 16_000;
        (trimmed, dur)
    } else {
        if continuous {
            dock::set_state(app, "hidden", None);
        } else {
            dock::set_caption(app, Some("no speech detected"));
            notify::notify("No speech detected", "Try again or check your microphone");
            play_sound(state, crate::audio::SoundEvent::Error);
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(2200));
                dock::set_caption(&app, None);
                dock::set_state(&app, "hidden", None);
            });
        }
        return Ok(TranscriptResult {
            text: String::new(),
            duration_ms: 0,
        });
    };

    let engine = load_engine(state)?;
    let dictionary = dictionary_terms(state);
    let hotwords = dictionary_hotwords(&dictionary);
    let raw = engine
        .transcribe(&final_audio, hotwords.as_deref())
        .map_err(|e| e.to_string())?;
    let mapped = if spoken_punctuation_enabled(state) {
        opendictate_core::text::map_spoken_punctuation(&raw)
    } else {
        raw
    };
    let corrected = opendictate_core::text::correct_dictionary_terms(&mapped, &dictionary);
    let text = inject::clean_text(&corrected);

    if let Some(action_text) = handle_voice_action_or_snippet(app, state, &text) {
        return Ok(TranscriptResult {
            text: action_text,
            duration_ms,
        });
    }

    let text = apply_voice_polish(state, &text);

    if test {
        dock::set_caption(app, None);
        dock::set_state(app, "hidden", None);
        return Ok(TranscriptResult {
            text,
            duration_ms,
        });
    }

    commit_text(app, state, &text, duration_ms, continuous, !continuous)?;

    Ok(TranscriptResult { text, duration_ms })
}

/// Helper to check if voice actions are enabled.
fn voice_actions_enabled(state: &AppState) -> bool {
    state.settings.lock().map(|s| s.voice_actions_enabled).unwrap_or(true)
}

/// Applies AI Voice Polish (Groq API or Local SLM) if configured in Settings.
fn apply_voice_polish(state: &AppState, text: &str) -> String {
    let settings = match state.settings.lock() {
        Ok(s) => s.clone(),
        Err(_) => return text.to_string(),
    };
    let provider = match settings.polish_provider.as_str() {
        "groq" => opendictate_core::text::PolishProvider::Groq,
        "local_slm" => opendictate_core::text::PolishProvider::LocalSlm,
        _ => return text.to_string(),
    };
    let mode = match settings.polish_mode.as_str() {
        "bullets" => opendictate_core::text::PolishMode::Bullets,
        _ => opendictate_core::text::PolishMode::Clean,
    };
    let config = opendictate_core::text::PolishConfig {
        provider,
        mode,
        groq_api_key: settings.groq_api_key,
        groq_model: settings.groq_model,
    };
    match opendictate_core::text::polish_text(text, &config) {
        Ok(polished) => polished,
        Err(e) => {
            log::warn!("voice polish failed: {e}; using raw text");
            text.to_string()
        }
    }
}

/// Handles voice action commands (e.g. "scratch that", "new line", "all caps", etc.)
/// and falls back to snippet commands. Returns Some(text) if handled, or None to proceed.
fn handle_voice_action_or_snippet(app: &AppHandle, state: &AppState, text: &str) -> Option<String> {
    use opendictate_core::text::VoiceAction;

    if voice_actions_enabled(state) {
        if let Some(action) = opendictate_core::text::parse_voice_action(text) {
            match action {
                VoiceAction::Undo => {
                    let _ = inject::undo_last_insert();
                    notify::notify("Action Executed", "Undo");
                    show_action(app, state, "✓ Undo");
                    return Some("✓ Action: Undo".to_string());
                }
                VoiceAction::DeleteWord => {
                    let _ = inject::press_delete_word();
                    show_action(app, state, "✓ Delete Word");
                    return Some("✓ Action: Delete Word".to_string());
                }
                VoiceAction::DeleteLine => {
                    let _ = inject::press_delete_line();
                    show_action(app, state, "✓ Delete Line");
                    return Some("✓ Action: Delete Line".to_string());
                }
                VoiceAction::ClearAll => {
                    let _ = inject::press_clear_all();
                    show_action(app, state, "✓ Clear All");
                    return Some("✓ Action: Clear All".to_string());
                }
                VoiceAction::NewLine => {
                    let _ = inject::press_new_line();
                    show_action(app, state, "✓ New Line");
                    return Some("✓ Action: New Line".to_string());
                }
                VoiceAction::NewParagraph => {
                    let _ = inject::press_new_paragraph();
                    show_action(app, state, "✓ New Paragraph");
                    return Some("✓ Action: New Paragraph".to_string());
                }
                VoiceAction::Tab => {
                    let _ = inject::press_tab();
                    show_action(app, state, "✓ Tab");
                    return Some("✓ Action: Tab".to_string());
                }
                VoiceAction::BulletPoint => {
                    let _ = inject::inject_text(app, "• ", &insert_mode(state));
                    show_action(app, state, "✓ Bullet");
                    return Some("• ".to_string());
                }
                VoiceAction::AllCaps(phrase) => {
                    let _ = commit_text(app, state, &phrase, 0, false, true);
                    return Some(phrase);
                }
                VoiceAction::CamelCase(phrase) => {
                    let _ = commit_text(app, state, &phrase, 0, false, true);
                    return Some(phrase);
                }
                VoiceAction::SnakeCase(phrase) => {
                    let _ = commit_text(app, state, &phrase, 0, false, true);
                    return Some(phrase);
                }
                VoiceAction::TitleCase(phrase) => {
                    let _ = commit_text(app, state, &phrase, 0, false, true);
                    return Some(phrase);
                }
                VoiceAction::PromptAndSend(prompt_text) => {
                    let _ = commit_text(app, state, &prompt_text, 0, false, false);
                    std::thread::sleep(Duration::from_millis(60));
                    let _ = inject::press_enter();
                    show_action(app, state, "✓ Prompt Sent");
                    return Some(format!("✓ Prompt Sent: {prompt_text}"));
                }
                VoiceAction::Submit => {
                    let _ = inject::press_enter();
                    show_action(app, state, "✓ Sent");
                    return Some("✓ Sent".to_string());
                }
                VoiceAction::Interrupt => {
                    let _ = inject::press_interrupt();
                    show_action(app, state, "✓ Stopped (Ctrl+C)");
                    return Some("✓ Stopped (Ctrl+C)".to_string());
                }
                VoiceAction::SwitchApp(app_name) => {
                    let res = inject::switch_to_app(&app_name);
                    let label = format!("✓ Switch: {app_name}");
                    show_action(app, state, &label);
                    return Some(match res {
                        Ok(()) => format!("✓ Switched to {app_name}"),
                        Err(e) => format!("⚠ App switch failed: {e}"),
                    });
                }
                VoiceAction::NextTab => {
                    let _ = inject::press_next_tab();
                    show_action(app, state, "✓ Next Tab");
                    return Some("✓ Next Tab".to_string());
                }
                VoiceAction::PrevTab => {
                    let _ = inject::press_prev_tab();
                    show_action(app, state, "✓ Prev Tab");
                    return Some("✓ Prev Tab".to_string());
                }
                VoiceAction::NewTab => {
                    let _ = inject::press_new_tab();
                    show_action(app, state, "✓ New Tab");
                    return Some("✓ New Tab".to_string());
                }
                VoiceAction::CloseTab => {
                    let _ = inject::press_close_tab();
                    show_action(app, state, "✓ Close Tab");
                    return Some("✓ Close Tab".to_string());
                }
                VoiceAction::ScrollDown => {
                    let _ = inject::press_scroll_down();
                    show_action(app, state, "✓ Scroll Down");
                    return Some("✓ Scroll Down".to_string());
                }
                VoiceAction::ScrollUp => {
                    let _ = inject::press_scroll_up();
                    show_action(app, state, "✓ Scroll Up");
                    return Some("✓ Scroll Up".to_string());
                }
                VoiceAction::WebSearch(query) => {
                    let _ = inject::open_browser_search(&query);
                    let label = format!("✓ Search: {query}");
                    show_action(app, state, &label);
                    return Some(format!("✓ Web Search: {query}"));
                }
                VoiceAction::OpenUrl(url) => {
                    let _ = inject::open_browser_url(&url);
                    let label = format!("✓ Open: {url}");
                    show_action(app, state, &label);
                    return Some(format!("✓ Opened {url}"));
                }
                VoiceAction::TerminalCommand(cmd) => {
                    let _ = commit_text(app, state, &cmd, 0, false, false);
                    std::thread::sleep(Duration::from_millis(60));
                    let _ = inject::press_enter();
                    let label = format!("✓ Run: {cmd}");
                    show_action(app, state, &label);
                    return Some(format!("✓ Terminal: {cmd}"));
                }
                VoiceAction::InsertSnippet(name) => {
                    return handle_snippet_command(app, state, &format!("insert snippet {name}"));
                }
                VoiceAction::Sleep => {
                    state.handsfree_awake.store(false, Ordering::SeqCst);
                    dock::set_state(app, "hidden", None);
                    dock::set_caption(app, Some("Handsfree: Sleeping (Say 'Hey Dictate')"));
                    notify::notify("Handsfree Mode", "Sleeping — say 'Hey Dictate' to wake");
                    play_sound(state, crate::audio::SoundEvent::Inserted);
                    return Some("✓ Sleeping".to_string());
                }
            }
        }
    }

    handle_snippet_command(app, state, text)
}

/// Truncates a transcript for the dock caption pill.
fn pill_text(text: &str) -> String {
    let t = text.trim();
    let mut s: String = t.chars().take(48).collect();
    if t.chars().count() > 48 {
        s.push('…');
    }
    s
}

/// Flashes an executed action in the dock pill, then restores the correct dock state.
fn show_action(app: &AppHandle, state: &AppState, label: &str) {
    dock::set_state(app, "inserted", None);
    dock::set_caption(app, Some(label));
    play_sound(state, crate::audio::SoundEvent::Inserted);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        let Some(state) = app.try_state::<AppState>() else { return };
        if state.handsfree_active.load(Ordering::SeqCst) && state.handsfree_awake.load(Ordering::SeqCst) {
            dock::set_state(&app, "listening", None);
            dock::set_caption(&app, Some("Listening…"));
        } else if state.handsfree_active.load(Ordering::SeqCst) {
            dock::set_state(&app, "hidden", None);
            dock::set_caption(&app, Some("Handsfree: Sleeping (Say 'Hey Dictate')"));
        } else {
            dock::set_caption(&app, None);
            dock::set_state(&app, "hidden", None);
        }
    });
}

/// Flashes the inserted transcript in the pill, then restores the correct dock state.
fn show_inserted(app: &AppHandle, text: &str) {
    dock::set_state(app, "inserted", None);
    dock::set_caption(app, Some(&pill_text(text)));
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        let Some(state) = app.try_state::<AppState>() else { return };
        if state.handsfree_active.load(Ordering::SeqCst) && state.handsfree_awake.load(Ordering::SeqCst) {
            dock::set_state(&app, "listening", None);
            dock::set_caption(&app, Some("Listening…"));
        } else if state.handsfree_active.load(Ordering::SeqCst) {
            dock::set_state(&app, "hidden", None);
            dock::set_caption(&app, Some("Handsfree: Sleeping (Say 'Hey Dictate')"));
        } else {
            dock::set_caption(&app, None);
            dock::set_state(&app, "hidden", None);
        }
    });
}

/// Detects an `insert snippet <name>` command. Snippet triggers are restricted
/// to a single word, so only the first word after the prefix is treated as the
/// name; any remaining words are dictated normally after the snippet text is
/// inserted. The snippet is resolved with a best-effort fuzzy match on its
/// trigger, injected alongside the tail, and dock/notification feedback is
/// driven. Returns `Some(inserted_text)` — or `Some("")` when the name could
/// not be resolved. Returns `None` when the text is not a snippet command, so
/// callers fall through to normal dictation. Snippet expansions never write
/// history entries.
fn handle_snippet_command(app: &AppHandle, state: &AppState, text: &str) -> Option<String> {
    const PREFIX: &str = "insert snippet";
    let lowered = text.trim().to_lowercase();
    let rest = lowered.strip_prefix(PREFIX)?.trim();
    if rest.is_empty() {
        return None;
    }
    let mut parts = rest.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("");
    if name.is_empty() {
        return None;
    }
    let tail = parts.next().unwrap_or("").trim();

    let snippet = match state.db.lock() {
        Ok(conn) => match db::list_snippets(&conn) {
            Ok(list) => {
                let triggers: Vec<String> = list
                    .iter()
                    .filter(|s| opendictate_core::text::is_single_word(&s.trigger))
                    .map(|s| s.trigger.clone())
                    .collect();
                let matched = opendictate_core::text::fuzzy_match_trigger(name, &triggers, 0.6);
                matched.and_then(|(trigger, _)| {
                    list.into_iter().find(|s| s.trigger == trigger)
                })
            }
            Err(e) => {
                log::warn!("snippet lookup failed: {e}");
                return Some(String::new());
            }
        },
        Err(e) => {
            log::warn!("snippet lookup failed: {e}");
            return Some(String::new());
        }
    };

    let snippet = match snippet {
        Some(snippet) => snippet,
        None => {
            let message = format!("Snippet not found: \"{name}\"");
            dock::set_state(app, "error", Some(&message));
            notify::notify("Snippet not found", &message);
            play_sound(state, crate::audio::SoundEvent::Error);
            return Some(String::new());
        }
    };

    let inserted = if tail.is_empty() {
        snippet.text.clone()
    } else {
        format!("{} {}", snippet.text.trim(), tail)
    };

    if let Err(e) = inject::inject_text(app, &inserted, &insert_mode(state)) {
        let message = format!("failed to paste: {e}");
        dock::set_state(app, "error", Some(&message));
        notify::notify("Dictation error", &format!("Failed to insert snippet: {e}"));
        play_sound(state, crate::audio::SoundEvent::Error);
        return Some(String::new());
    }
    if let Ok(mut last) = state.last_inserted.lock() {
        *last = Some(inserted.clone());
    }

    let _ = app.emit(
        "transcript",
        serde_json::json!({ "text": inserted, "injected": true }),
    );
    let _ = app.emit(
        "overlay-state",
        serde_json::json!({ "state": "inserted", "message": null }),
    );
    notify::notify("Snippet inserted", &format!("\"{}\"", snippet.trigger));
    play_sound(state, crate::audio::SoundEvent::Inserted);
    show_inserted(app, &inserted);
    Some(inserted)
}

/// Emits the transcript, persists it, injects it and drives dock feedback.
/// `continuous` keeps the dock in listening state; `finish` (end of session)
/// flashes "inserted" and then hides the dock.
fn commit_text(
    app: &AppHandle,
    state: &AppState,
    text: &str,
    duration_ms: u64,
    continuous: bool,
    finish: bool,
) -> Result<(), String> {
    let _ = app.emit(
        "transcript",
        serde_json::json!({ "text": text, "injected": true }),
    );
    let _ = app.emit(
        "overlay-state",
        serde_json::json!({ "state": "inserted", "message": null }),
    );

    if text.is_empty() {
        if state.handsfree_active.load(Ordering::SeqCst) && state.handsfree_awake.load(Ordering::SeqCst) {
            dock::set_state(app, "listening", None);
            dock::set_caption(app, Some("Listening…"));
        } else if state.handsfree_active.load(Ordering::SeqCst) {
            dock::set_state(app, "hidden", None);
            dock::set_caption(app, Some("Handsfree: Sleeping (Say 'Hey Dictate')"));
        } else {
            dock::set_caption(app, None);
            dock::set_state(app, "hidden", None);
        }
        return Ok(());
    }

    let entry = HistoryEntry {
        id: 0,
        text: text.to_string(),
        created_at: db::now_timestamp(),
        duration_ms,
        source: if continuous { "continuous".to_string() } else { "hotkey".to_string() },
    };
    if let Ok(conn) = state.db.lock() {
        if db::insert_history(&conn, &entry).is_ok() {
            let _ = app.emit("history-updated", serde_json::json!({}));
        }
    }

    if let Err(e) = inject::inject_text(app, text, &insert_mode(state)) {
        dock::set_state(app, "error", Some(&format!("failed to paste: {e}")));
        notify::notify("Dictation error", &format!("Failed to insert text: {e}"));
        play_sound(state, crate::audio::SoundEvent::Error);
        return Err(e);
    }
    if let Ok(mut last) = state.last_inserted.lock() {
        *last = Some(text.to_string());
    }
    play_sound(state, crate::audio::SoundEvent::Inserted);

    if !continuous || finish {
        notify::notify("Inserted", &pill_text(text));
    }

    if continuous {
        if finish {
            show_inserted(app, text);
        } else {
            dock::set_state(app, "listening", None);
            dock::set_caption(app, Some("listening…"));
        }
        return Ok(());
    }

    show_inserted(app, text);

    Ok(())
}

fn spawn_level_emitter(app: &AppHandle, state: &AppState) {
    let app = app.clone();
    let recorder = Arc::clone(&state.recorder);
    std::thread::spawn(move || {
        while recorder.is_recording() {
            let rms = recorder.current_rms();
            let _ = app.emit("audio-level", serde_json::json!({ "rms": rms }));
            std::thread::sleep(Duration::from_millis(33));
        }
        let _ = app.emit("audio-level", serde_json::json!({ "rms": 0.0 }));
    });
}

fn run_vad(
    state: &AppState,
    audio: &[f32],
) -> Result<opendictate_core::audio::vad::VadResult, opendictate_core::CoreError> {
    let sensitivity = sensitivity(state);
    let vad_path = models::vad_model_path();
    let silero = if models::is_vad_ready() {
        let threshold = sensitivity_to_silero_threshold(sensitivity);
        let mut cache = state
            .vad
            .lock()
            .map_err(|_| opendictate_core::CoreError::Audio("VAD cache lock poisoned".to_string()))?;
        if let Some(cached) = cache.as_ref() {
            if (cached.sensitivity - sensitivity).abs() < f32::EPSILON {
                Some(Arc::clone(&cached.detector))
            } else {
                let detector = Arc::new(SileroVad::with_threshold(&vad_path, threshold)?);
                *cache = Some(crate::state::CachedVad {
                    sensitivity,
                    detector: Arc::clone(&detector),
                });
                Some(detector)
            }
        } else {
            let detector = Arc::new(SileroVad::with_threshold(&vad_path, threshold)?);
            *cache = Some(crate::state::CachedVad {
                sensitivity,
                detector: Arc::clone(&detector),
            });
            Some(detector)
        }
    } else {
        None
    };
    Ok(apply_vad(audio, silero.as_deref(), sensitivity))
}

fn dictionary_terms(state: &AppState) -> Vec<String> {
    state
        .db
        .lock()
        .ok()
        .and_then(|conn| db::get_dictionary(&conn).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.word)
        .collect()
}

fn dictionary_hotwords(terms: &[String]) -> Option<String> {
    let joined: Vec<String> = terms
        .iter()
        .map(|word| format!("{} {}", word.replace(' ', "_"), HOTWORD_SCORE))
        .collect();
    if joined.is_empty() {
        None
    } else {
        Some(joined.join("\n"))
    }
}

const HOTWORD_SCORE: f32 = 3.0;

fn insert_mode(state: &AppState) -> String {
    state
        .settings
        .lock()
        .map(|s| s.insert_mode.clone())
        .unwrap_or_else(|_| "auto".to_string())
}

pub fn load_engine(state: &AppState) -> Result<Arc<SttEngine>, String> {
    let (model_id, language) = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        (
            if settings.stt_model.is_empty() {
                models::STT_MODEL_ID.to_string()
            } else {
                settings.stt_model.clone()
            },
            settings.language.clone(),
        )
    };
    let language_key = language.clone();
    {
        let cache = state.stt_engine.lock().map_err(|e| e.to_string())?;
        if let Some(cached) = cache.as_ref() {
            if cached.model_id == model_id && cached.language == language_key {
                log::debug!("offline engine cache hit for {}", model_id);
                return Ok(Arc::clone(&cached.engine));
            }
        }
    }
    if !models::is_model_installed(&model_id) {
        return Err(format!(
            "STT model '{model_id}' is not installed — download it in Settings → Models"
        ));
    }
    let dir = models::model_dir_for(&model_id);
    let kind = if models::is_whisper_model(&model_id) {
        ModelKind::Whisper
    } else if models::is_transducer_model(&model_id) {
        ModelKind::NemoTransducer
    } else {
        ModelKind::NemoCtc
    };
    let load_started = Instant::now();
    let provider = desired_provider(state);
    let engine = Arc::new({
        let eng = SttEngine::new_with_provider(&dir, kind, Some(language), provider)
            .map_err(|e| e.to_string())?;
        if eng.provider != "cpu" {
            state.gpu_active.store(true, Ordering::SeqCst);
        }
        eng
    });
    let mut cache = state.stt_engine.lock().map_err(|e| e.to_string())?;
    if let Some(cached) = cache.as_ref() {
        if cached.model_id == model_id && cached.language == language_key {
            return Ok(Arc::clone(&cached.engine));
        }
    }
    *cache = Some(crate::state::CachedSttEngine {
        model_id,
        language: language_key,
        engine: Arc::clone(&engine),
    });
    log::info!("offline engine ready in {} ms", load_started.elapsed().as_millis());
    Ok(engine)
}

/// Loads and caches the Sherpa-ONNX Keyword Spotter (KWS) with user-configured wake words.
pub fn ensure_kws_spotter(state: &AppState) -> Result<Arc<opendictate_core::stt::kws::Spotter>, String> {
    let mut cache = state.kws_engine.lock().map_err(|e| e.to_string())?;
    if let Some(existing) = cache.as_ref() {
        return Ok(Arc::clone(&existing.spotter));
    }

    let kws_dir = models::kws_model_dir();
    if !models::is_kws_ready() {
        return Err("KWS model is not downloaded yet — please download it in Settings → Models".to_string());
    }

    let wake_words_str = state
        .settings
        .lock()
        .map(|s| s.wake_words.clone())
        .unwrap_or_else(|_| "hey dictate, computer".to_string());
    let keywords: Vec<String> = wake_words_str
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let spotter = opendictate_core::stt::kws::Spotter::new(&kws_dir, &keywords, 0.25)
        .map_err(|e| e.to_string())?;
    let arc = Arc::new(spotter);
    *cache = Some(crate::state::CachedKwsEngine {
        spotter: Arc::clone(&arc),
    });
    log::info!("KWS spotter initialized with wake words: {:?}", keywords);
    Ok(arc)
}

/// Starts the Handsfree background listening service.
pub fn start_handsfree(app: &AppHandle, state: &AppState) -> Result<(), String> {
    if state.handsfree_active.load(Ordering::SeqCst) {
        return Ok(());
    }

    let spotter = ensure_kws_spotter(state)?;
    state.handsfree_active.store(true, Ordering::SeqCst);
    state.handsfree_awake.store(false, Ordering::SeqCst);

    if !state.recorder.is_recording() {
        let mic = state.settings.lock().map(|s| s.mic.clone()).unwrap_or(None);
        match mic {
            Some(name) if !name.is_empty() => state.recorder.start_with_name(&name),
            _ => state.recorder.start(),
        }
        .map_err(|e| e.to_string())?;
    }

    dock::set_state(app, "hidden", None);
    dock::set_caption(app, Some("Handsfree: Sleeping (Say 'Hey Dictate')"));
    notify::notify("Handsfree Mode Enabled", "Listening for wake phrase in background");

    let app = app.clone();
    std::thread::Builder::new()
        .name("opendictate-handsfree".to_string())
        .spawn(move || {
            let state = state_from_app(&app);
            let mut session = spotter.create_session();
            let mut watermark = 0;
            let mut last_awake_activity = Instant::now();
            let mut utterance_samples: Vec<f32> = Vec::new();
            let mut utterance_active = false;
            let mut silent_since = Instant::now();
            let poll_interval = Duration::from_millis(60);

            let mut noise_floor: f32 = 0.012;
            let mut utterance_started = Instant::now();
            let mut consecutive_speech_frames: u32 = 0;
            const SILENCE_TIMEOUT: Duration = Duration::from_millis(800);
            const MAX_UTTERANCE_DURATION: Duration = Duration::from_secs(15);
            const MIN_UTTERANCE_SAMPLES: usize = 6400; // 0.4s @ 16kHz
            const SPEECH_CONFIRMATION_FRAMES: u32 = 2; // require >=120ms continuous speech to transition to Recording

            let vad_path = models::vad_model_path();
            let silero_detector = if models::is_vad_ready() {
                SileroVad::with_threshold(&vad_path, 0.5).ok()
            } else {
                None
            };

            log::info!("handsfree background loop running (neural VAD: {})", silero_detector.is_some());

            while state.handsfree_active.load(Ordering::SeqCst) {
                if !state.recorder.is_recording() {
                    // Only restart if we are the sole owner (no normal dictation in progress).
                    // Do NOT call stop() here – the recorder is already idle and stop() errors
                    // on an idle recorder, potentially masking the real state.
                    let _ = restart_recorder(&app);
                    // After restarting, reset our watermark to the empty buffer start.
                    watermark = 0;
                }

                let is_awake = state.handsfree_awake.load(Ordering::SeqCst);
                let samples = state.recorder.take_since(&mut watermark);

                let rms = if samples.is_empty() {
                    0.0
                } else {
                    let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
                    (sum_sq / samples.len() as f32).sqrt()
                };
                let peak = if samples.is_empty() {
                    0.0
                } else {
                    samples.iter().map(|&s| s.abs()).fold(0.0f32, f32::max)
                };
                let crest_factor = if rms > 1e-5 { peak / rms } else { 1.0 };

                if !is_awake {
                    // Sleeping: only run low-power KWS keyword spotting
                    consecutive_speech_frames = 0;
                    if !samples.is_empty() {
                        noise_floor = (noise_floor * 0.90 + rms * 0.10).clamp(0.005, 0.12);
                        if let Some(keyword) = spotter.process_samples(&session, &samples) {
                            log::info!("wake word detected: '{}'", keyword);
                            state.handsfree_awake.store(true, Ordering::SeqCst);
                            last_awake_activity = Instant::now();
                            utterance_active = false;
                            utterance_samples.clear();
                            silent_since = Instant::now();
                            utterance_started = Instant::now();
                            session = spotter.create_session();
                            if let Some(ref silero) = silero_detector {
                                silero.reset();
                            }
                            dock::set_state(&app, "listening", None);
                            dock::set_caption(&app, Some("Listening…"));
                            notify::notify("Handsfree Awake", &format!("Detected \"{keyword}\""));
                            play_sound(&state, crate::audio::SoundEvent::Listening);
                        }
                    }
                } else {
                    // Awake: use neural Silero VAD + adaptive energy for speech detection
                    let base_threshold = sensitivity_to_energy_threshold(state_sensitivity(&app));
                    if !utterance_active {
                        noise_floor = (noise_floor * 0.85 + rms * 0.15).clamp(0.005, 0.15);
                    }

                    let vad_speech = if let Some(ref silero) = silero_detector {
                        if !samples.is_empty() {
                            silero.accept_streaming(&samples)
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    let speech_threshold = if utterance_active {
                        (noise_floor * 1.4).max(base_threshold * 0.6)
                    } else {
                        (noise_floor * 2.0).max(base_threshold * 0.9)
                    };

                    let is_speech = if silero_detector.is_some() {
                        vad_speech || (rms >= speech_threshold && (crest_factor >= 2.0 || rms >= noise_floor * 2.0))
                    } else {
                        rms >= speech_threshold && (crest_factor >= 2.0 || rms >= noise_floor * 2.0)
                    };

                    if is_speech {
                        last_awake_activity = Instant::now();
                        silent_since = Instant::now();
                        consecutive_speech_frames += 1;

                        if !utterance_active {
                            if consecutive_speech_frames >= SPEECH_CONFIRMATION_FRAMES {
                                utterance_active = true;
                                utterance_started = Instant::now();
                                dock::set_state(&app, "recording", None);
                                dock::set_caption(&app, Some("Recording…"));
                            }
                        }
                        if utterance_active {
                            utterance_samples.extend_from_slice(&samples);
                        }
                    } else {
                        consecutive_speech_frames = 0;
                        if utterance_active {
                            utterance_samples.extend_from_slice(&samples);
                            let silence_reached = silent_since.elapsed() >= SILENCE_TIMEOUT;
                            let max_reached = utterance_started.elapsed() >= MAX_UTTERANCE_DURATION;

                            if silence_reached || max_reached {
                                if utterance_samples.len() >= MIN_UTTERANCE_SAMPLES {
                                    dock::set_state(&app, "transcribing", None);
                                    dock::set_caption(&app, Some("Processing…"));
                                    let audio = std::mem::take(&mut utterance_samples);
                                    let _ = process_utterance_on_worker(&app, audio, false, true);
                                    last_awake_activity = Instant::now();
                                    if state.handsfree_awake.load(Ordering::SeqCst) {
                                        dock::set_state(&app, "listening", None);
                                        dock::set_caption(&app, Some("Listening…"));
                                    }
                                } else {
                                    // Short noise spike (under 0.4s): discard and return to listening cleanly
                                    if state.handsfree_awake.load(Ordering::SeqCst) {
                                        dock::set_state(&app, "listening", None);
                                        dock::set_caption(&app, Some("Listening…"));
                                    }
                                }
                                utterance_active = false;
                                utterance_samples.clear();
                                silent_since = Instant::now();
                            }
                        } else {
                            // Check inactivity timeout
                            let timeout_sec = state
                                .settings
                                .lock()
                                .map(|s| s.handsfree_silence_timeout_sec as u64)
                                .unwrap_or(30);

                            if last_awake_activity.elapsed() > Duration::from_secs(timeout_sec) {
                                log::info!("handsfree inactivity timeout ({}s); going to sleep", timeout_sec);
                                state.handsfree_awake.store(false, Ordering::SeqCst);
                                session = spotter.create_session();
                                dock::set_state(&app, "hidden", None);
                                dock::set_caption(&app, Some("Handsfree: Sleeping (Say 'Hey Dictate')"));
                                play_sound(&state, crate::audio::SoundEvent::Inserted);
                            }
                        }
                    }
                }

                std::thread::sleep(poll_interval);
            }
            log::info!("handsfree background loop exited");
        })
        .map_err(|e| format!("failed to spawn handsfree thread: {e}"))?;

    Ok(())
}

/// Stops the Handsfree listening service.
pub fn stop_handsfree(app: &AppHandle, state: &AppState) {
    state.handsfree_active.store(false, Ordering::SeqCst);
    state.handsfree_awake.store(false, Ordering::SeqCst);
    // Never stop the recorder out from under a live user dictation session:
    // handsfree may have been toggled off while the user is mid-dictation
    // (started via hotkey/hold-to-talk), and stopping here would submit half
    // of the user's utterance as a transcript.
    let user_session_open = state.user_dictation_active.load(Ordering::SeqCst);
    if !state.continuous.load(Ordering::SeqCst)
        && !state.is_streaming_active()
        && !user_session_open
    {
        let _ = state.recorder.stop();
    }
    dock::set_caption(app, None);
    dock::set_state(app, "hidden", None);
}
