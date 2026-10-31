use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use opendictate_core::audio::vad::{
    apply_vad, sensitivity_to_energy_threshold, sensitivity_to_silero_threshold, SileroVad,
};
use opendictate_core::stt::engine::{ModelKind, SttEngine};
use opendictate_core::stt::models;
use opendictate_core::stt::streaming::StreamingRecognizer;
use tauri::{AppHandle, Emitter, Manager};

use crate::session::{submit_after_insert, NoiseGate, SpeechBuffer, WorkToken};
use crate::state::{AppState, HistoryEntry, StreamingPipe, TranscriptResult};
use crate::{db, dock, inject, notify};

const MAX_OFFLINE_SAMPLES: usize = 110 * 16_000;
const CONTINUOUS_BACKLOG: usize = 2;
const MIC_RETRY_INTERVAL: Duration = Duration::from_secs(2);
const ERROR_DISPLAY: Duration = Duration::from_millis(2600);

pub struct QueuedAudio {
    audio: Vec<f32>,
    finish: bool,
    response: Option<mpsc::SyncSender<Result<TranscriptResult, String>>>,
}

pub fn state_from_app(app: &AppHandle) -> tauri::State<'_, AppState> {
    app.state::<AppState>()
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

fn desired_provider(state: &AppState) -> opendictate_core::stt::provider::Provider {
    let mode = state
        .settings
        .lock()
        .map(|s| s.gpu.clone())
        .unwrap_or_default();
    opendictate_core::stt::provider::resolve(&mode)
}

fn sensitivity(state: &AppState) -> f32 {
    state
        .settings
        .lock()
        .map(|s| s.vad_sensitivity)
        .unwrap_or(0.5)
}

fn play_sound(state: &AppState, event: crate::audio::SoundEvent) {
    let (enabled, volume) = state
        .settings
        .lock()
        .map(|s| (s.audio_feedback, s.audio_feedback_volume))
        .unwrap_or((false, 0.5));
    if enabled {
        crate::audio::play_event(volume, event);
    }
}

fn start_capture(state: &AppState) -> Result<(), String> {
    let mic = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .mic
        .clone();
    match mic {
        Some(name) if !name.is_empty() && name != "default" => {
            state.recorder.start_with_name(&name)
        }
        _ => state.recorder.start(),
    }
    .map_err(|e| e.to_string())
}

fn cancel_capture(state: &AppState) -> Result<(), String> {
    match state.recorder.cancel() {
        Ok(()) => Ok(()),
        // Cancelling in-flight inference is valid after capture has stopped.
        Err(opendictate_core::CoreError::Audio(message))
            if message == "no recording in progress" =>
        {
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn work_current(state: &AppState, token: WorkToken) -> bool {
    state.sessions.current(token)
        && (token.handsfree_generation.is_none()
            || (state.handsfree_active.load(Ordering::SeqCst)
                && !state.user_dictation_active.load(Ordering::SeqCst)))
}

fn ensure_current(state: &AppState, token: WorkToken) -> Result<(), String> {
    if work_current(state, token) {
        Ok(())
    } else {
        Err("dictation session was cancelled or replaced".to_string())
    }
}

fn report_error(app: &AppHandle, state: &AppState, token: WorkToken, error: &str) {
    if let Ok(_commit) = state.sessions.commit_guard(token) {
        if !work_current(state, token) {
            return;
        }
        dock::set_state(app, "error", Some(error));
        let _ = app.emit("dictation-error", serde_json::json!({ "error": error }));
        notify::notify("Dictation error", error);
        play_sound(state, crate::audio::SoundEvent::Error);
        restore_dock_after(app, token, ERROR_DISPLAY);
    }
}

/// Returns the dock to the session's resting state once a temporary state has
/// been shown. Nothing else leaves "error" or "inserted" while Handsfree idles.
fn restore_dock_after(app: &AppHandle, token: WorkToken, delay: Duration) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        let state = state_from_app(&app);
        if let Ok(_commit) = state.sessions.commit_guard(token) {
            restore_dock(&app, &state);
        };
    });
}

pub fn start(app: &AppHandle, state: &AppState, test: bool) -> Result<(), String> {
    let _transition = state
        .sessions
        .transition
        .lock()
        .map_err(|e| e.to_string())?;
    if state.user_dictation_active.load(Ordering::SeqCst) {
        return Err("a dictation session is already active".to_string());
    }
    let model_id = selected_model_id(state);
    if !test && !models::is_model_installed(&model_id) {
        return Err(format!(
            "STT model '{model_id}' is not installed — download it in Settings → Models"
        ));
    }
    let token = state.sessions.invalidate();
    *state.recording_token.lock().map_err(|e| e.to_string())? = Some(token);
    state.handsfree_awake.store(false, Ordering::SeqCst);
    state.set_test_mode(test);
    state.set_continuous(false);
    state.set_streaming(false);
    state.caption_active.store(false, Ordering::SeqCst);
    *state.caption_stream.lock().map_err(|e| e.to_string())? = None;
    *state.stream.lock().map_err(|e| e.to_string())? = None;
    if let Err(error) = crate::session::begin_explicit_capture(
        state.recorder.is_recording(),
        || cancel_capture(state),
        || state.recorder.clear_buffer(),
        || start_capture(state),
    ) {
        *state.recording_token.lock().map_err(|e| e.to_string())? = None;
        state.set_test_mode(false);
        return Err(error);
    }
    state.user_dictation_active.store(true, Ordering::SeqCst);

    let streaming = !test && models::is_streaming_model(&model_id);
    let continuous =
        !test && !streaming && state.settings.lock().map(|s| s.continuous).unwrap_or(false);
    let setup = if streaming {
        spawn_streaming(app, state, token)
    } else if continuous {
        spawn_continuous_loop(app, state, token)
    } else {
        Ok(())
    };
    if let Err(error) = setup {
        state.sessions.invalidate();
        *state.recording_token.lock().map_err(|e| e.to_string())? = None;
        state.user_dictation_active.store(false, Ordering::SeqCst);
        state.set_test_mode(false);
        state.set_streaming(false);
        state.set_continuous(false);
        *state.stream.lock().map_err(|e| e.to_string())? = None;
        *state.continuous_queue.lock().map_err(|e| e.to_string())? = None;
        if !state.handsfree_active.load(Ordering::SeqCst) {
            let _ = state.recorder.cancel();
        }
        return Err(error);
    }
    dock::set_state(app, "listening", None);
    dock::set_caption(
        app,
        Some(if test {
            "testing microphone…"
        } else {
            "listening…"
        }),
    );
    if !test {
        notify::notify("Dictation on", "Recording — use your hotkey to stop");
        play_sound(state, crate::audio::SoundEvent::Listening);
        let worker_app = app.clone();
        std::thread::Builder::new()
            .name("opendictate-eager-warmup".into())
            .spawn(move || {
                let state = state_from_app(&worker_app);
                warmup_current_model(&state);
            })
            .ok();
    }
    spawn_level_emitter(app, state, token);
    if !test && models::is_caption_model_ready() {
        spawn_caption_loop(app, state, token);
    }
    spawn_capture_guard(app, token, !streaming && !continuous);
    let _ = app.emit(
        "recording-changed",
        serde_json::json!({ "recording": true }),
    );
    // Starting from the dock's microphone button activates OpenDictate on
    // macOS. Hand the activation straight back so the application the user
    // is working in keeps the key window.
    crate::focus::ensure_external_focus(app);
    Ok(())
}

pub fn stop(app: &AppHandle, state: &AppState) -> Result<TranscriptResult, String> {
    stop_session(app, state, None)
}

fn stop_session(
    app: &AppHandle,
    state: &AppState,
    expected: Option<WorkToken>,
) -> Result<TranscriptResult, String> {
    let transition = state
        .sessions
        .transition
        .lock()
        .map_err(|e| e.to_string())?;
    let token = state
        .recording_token
        .lock()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no recording in progress".to_string())?;
    if expected.is_some_and(|t| t != token) {
        return Err("recording session changed".into());
    }
    if !state.user_dictation_active.load(Ordering::SeqCst) {
        return Err("no recording in progress".into());
    }
    let test = state.is_test_mode();
    let was_streaming = state.is_streaming_active();
    state.set_continuous(false);
    state.set_streaming(false);
    state.caption_active.store(false, Ordering::SeqCst);
    *state.caption_stream.lock().map_err(|e| e.to_string())? = None;
    let mut pipe = state.stream.lock().map_err(|e| e.to_string())?.take();
    let queue = state
        .continuous_queue
        .lock()
        .map_err(|e| e.to_string())?
        .take();
    let handsfree = state.handsfree_active.load(Ordering::SeqCst);
    let audio = if let Some(pipe) = pipe.as_mut() {
        // Join the capture producer before its final unread tail. Handsfree
        // resumes capture after this explicit boundary if it is still enabled.
        state
            .recorder
            .stop_since(&mut pipe.watermark)
            .map_err(|e| e.to_string())
    } else if handsfree && state.recorder.is_recording() {
        Ok(state.recorder.drain_buffer())
    } else {
        state.recorder.stop().map_err(|e| e.to_string())
    };
    state.user_dictation_active.store(false, Ordering::SeqCst);
    *state.recording_token.lock().map_err(|e| e.to_string())? = None;
    state.set_test_mode(false);
    if !test && state.sessions.current(token) {
        dock::set_state(app, "transcribing", None);
        dock::set_caption(app, Some("finalizing…"));
    }
    let _ = app.emit(
        "recording-changed",
        serde_json::json!({ "recording": false }),
    );
    drop(transition);
    let audio = match audio {
        Ok(audio) => audio,
        Err(error) => {
            report_error(app, state, token, &error);
            return Err(error);
        }
    };

    let result = if test {
        Ok(microphone_test_result(&audio))
    } else if was_streaming {
        let pipe = pipe
            .take()
            .ok_or_else(|| "streaming session is missing".to_string())?;
        pipe.recognizer.accept(&pipe.session, &audio);
        pipe.recognizer.finish(&pipe.session);
        let raw = pipe.recognizer.result(&pipe.session);
        let duration_ms = pipe.session.started_at.elapsed().as_millis() as u64;
        let _inference = state.sessions.inference.lock().map_err(|e| e.to_string())?;
        finalize_transcript(app, state, &raw, duration_ms, token, true, true)
    } else if let Some(queue) = queue {
        let (sender, receiver) = mpsc::sync_channel(1);
        queue
            .send(QueuedAudio {
                audio,
                finish: true,
                response: Some(sender),
            })
            .map_err(|_| "continuous inference worker exited".to_string())?;
        drop(queue);
        receiver
            .recv()
            .map_err(|_| "continuous inference worker exited".to_string())?
    } else {
        process_utterance(app, state, &audio, token, false, true)
    };
    if test && state.sessions.current(token) {
        restore_dock(app, state);
    }
    if let Err(error) = &result {
        report_error(app, state, token, error);
    }
    // The session is over: hand the activation back to the application the
    // user is working in, in case the dock took it when the session started.
    crate::focus::ensure_external_focus(app);
    result
}

pub fn cancel(app: &AppHandle, state: &AppState) -> Result<(), String> {
    // Signal cancellation before waiting for a transition that may itself be
    // waiting for an in-flight decode. Stop retains the original recording token.
    state.sessions.invalidate();
    let _transition = state
        .sessions
        .transition
        .lock()
        .map_err(|e| e.to_string())?;
    state.sessions.invalidate();
    *state.recording_token.lock().map_err(|e| e.to_string())? = None;
    state.set_continuous(false);
    state.set_streaming(false);
    state.caption_active.store(false, Ordering::SeqCst);
    state.user_dictation_active.store(false, Ordering::SeqCst);
    state.set_test_mode(false);
    *state.stream.lock().map_err(|e| e.to_string())? = None;
    *state.caption_stream.lock().map_err(|e| e.to_string())? = None;
    *state.continuous_queue.lock().map_err(|e| e.to_string())? = None;
    if state.handsfree_active.load(Ordering::SeqCst) && state.recorder.is_recording() {
        state.recorder.clear_buffer();
    } else {
        cancel_capture(state)?;
    }
    let _ = app.emit(
        "recording-changed",
        serde_json::json!({ "recording": false }),
    );
    restore_dock(app, state);
    Ok(())
}

fn microphone_test_result(audio: &[f32]) -> TranscriptResult {
    TranscriptResult {
        text: if audio.iter().any(|s| s.abs() >= 0.0005) {
            "Microphone detected audio".into()
        } else {
            String::new()
        },
        duration_ms: audio.len() as u64 * 1000 / 16_000,
    }
}

fn spawn_capture_guard(app: &AppHandle, token: WorkToken, manual: bool) {
    let app = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(60));
        let state = state_from_app(&app);
        if !state.sessions.current(token) || !state.user_dictation_active.load(Ordering::SeqCst) {
            break;
        }
        let failed = !state.recorder.is_recording();
        if failed || (manual && state.recorder.buffered_samples() >= MAX_OFFLINE_SAMPLES) {
            if failed {
                notify::notify("Microphone stopped", "Finishing the audio already captured. Check your microphone before recording again.");
            } else {
                notify::notify("Recording limit reached", "Finishing this recording before the 120-second audio buffer fills. Start a new recording to continue.");
            }
            let _ = app.emit(
                "recording-limit",
                serde_json::json!({ "reason": if failed { "capture-ended" } else { "duration" } }),
            );
            let _ = stop_session(&app, &state, Some(token));
            break;
        }
    });
}

/// Inference consumes a bounded queue while this producer keeps capture open.
/// A full queue stops capture visibly, then flushes all already-captured audio.
fn spawn_continuous_loop(
    app: &AppHandle,
    state: &AppState,
    token: WorkToken,
) -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel::<QueuedAudio>(CONTINUOUS_BACKLOG);
    let worker_app = app.clone();
    std::thread::Builder::new()
        .name("opendictate-continuous-inference".into())
        .spawn(move || {
            while let Ok(work) = receiver.recv() {
                let state = state_from_app(&worker_app);
                let result =
                    process_utterance(&worker_app, &state, &work.audio, token, true, work.finish);
                if let Err(error) = &result {
                    report_error(&worker_app, &state, token, error);
                }
                if let Some(response) = work.response {
                    let _ = response.send(result);
                }
                if work.finish {
                    break;
                }
            }
        })
        .map_err(|e| format!("failed to start continuous inference: {e}"))?;
    *state.continuous_queue.lock().map_err(|e| e.to_string())? = Some(sender.clone());
    state.set_continuous(true);
    let app = app.clone();
    let capture_worker = move || {
        let mut silent_since = Instant::now();
        let mut utterance_started = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(60));
            let state = state_from_app(&app);
            let Ok(transition) = state.sessions.transition.lock() else {
                break;
            };
            if !state.sessions.current(token)
                || !state.continuous.load(Ordering::SeqCst)
                || !state.user_dictation_active.load(Ordering::SeqCst)
            {
                break;
            }
            if !state.recorder.is_recording() {
                break;
            }
            if state.recorder.current_rms() >= sensitivity_to_energy_threshold(sensitivity(&state))
            {
                silent_since = Instant::now();
            }
            let endpoint = silent_since.elapsed() >= Duration::from_millis(1400)
                && utterance_started.elapsed() >= Duration::from_millis(600);
            if !endpoint && state.recorder.buffered_samples() < MAX_OFFLINE_SAMPLES {
                continue;
            }
            let audio = state.recorder.drain_buffer();
            let work = QueuedAudio {
                audio,
                finish: false,
                response: None,
            };
            match sender.try_send(work) {
                Ok(()) => {
                    silent_since = Instant::now();
                    utterance_started = Instant::now();
                }
                Err(mpsc::TrySendError::Full(mut work)) => {
                    let tail = if state.handsfree_active.load(Ordering::SeqCst) {
                        state.recorder.drain_buffer()
                    } else {
                        match state.recorder.stop() {
                            Ok(audio) => audio,
                            Err(error) => {
                                log::warn!("continuous capture stop failed: {error}; recovering the retained buffer");
                                state.recorder.drain_buffer()
                            }
                        }
                    };
                    work.audio.extend(tail);
                    work.finish = true;
                    state.set_continuous(false);
                    state.caption_active.store(false, Ordering::SeqCst);
                    state.user_dictation_active.store(false, Ordering::SeqCst);
                    if let Ok(mut recording) = state.recording_token.lock() {
                        *recording = None;
                    }
                    if let Ok(mut q) = state.continuous_queue.lock() {
                        *q = None;
                    }
                    let _ = app.emit(
                        "recording-changed",
                        serde_json::json!({ "recording": false }),
                    );
                    notify::notify("Recording paused", "Transcription could not keep up. Finishing all captured audio; start again when processing completes.");
                    let _ = app.emit(
                        "recording-limit",
                        serde_json::json!({ "reason": "processing-backlog" }),
                    );
                    drop(transition);
                    let _ = sender.send(work);
                    break;
                }
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    drop(transition);
                    let _ = stop_session(&app, &state, Some(token));
                    break;
                }
            }
        }
    };
    std::thread::Builder::new()
        .name("opendictate-continuous-capture".into())
        .spawn(capture_worker)
        .map_err(|e| {
            state.set_continuous(false);
            if let Ok(mut queue) = state.continuous_queue.lock() {
                *queue = None;
            }
            format!("failed to start continuous capture: {e}")
        })?;
    Ok(())
}

pub fn warmup_current_model(state: &AppState) {
    let model_id = selected_model_id(state);
    if !models::is_model_installed(&model_id) {
        return;
    }
    if models::is_streaming_model(&model_id) {
        let _ = warmup_streaming(state);
    } else {
        let _ = load_engine(state);
    }
    if models::is_vad_ready() {
        let _ = run_vad(state, &[0.0f32; 1600]);
    }
}

pub fn warmup_streaming(state: &AppState) -> Result<Arc<StreamingRecognizer>, String> {
    let model_id = selected_model_id(state);
    let provider = desired_provider(state);
    let mut cache = state.streaming_engine.lock().map_err(|e| e.to_string())?;
    if let Some(existing) = cache.as_ref().filter(|e| e.model_id == model_id) {
        return Ok(Arc::clone(&existing.recognizer));
    }
    let recognizer = Arc::new(
        StreamingRecognizer::new_with_provider(
            &models::model_dir_for(&model_id),
            Some("nemo_transducer"),
            provider,
        )
        .map_err(|e| e.to_string())?,
    );
    state
        .gpu_active
        .store(recognizer.provider != "cpu", Ordering::SeqCst);
    *cache = Some(crate::state::CachedStreamingEngine {
        model_id,
        recognizer: Arc::clone(&recognizer),
    });
    Ok(recognizer)
}

fn spawn_streaming(app: &AppHandle, state: &AppState, token: WorkToken) -> Result<(), String> {
    let recognizer = warmup_streaming(state)?;
    let session =
        recognizer.create_session(dictionary_hotwords(&dictionary_terms(state)).as_deref());
    *state.stream.lock().map_err(|e| e.to_string())? = Some(StreamingPipe {
        token,
        recognizer,
        session,
        watermark: 0,
        total_fed: 0,
    });
    state.set_streaming(true);
    let app = app.clone();
    std::thread::Builder::new()
        .name("opendictate-streaming".into())
        .spawn(move || {
            loop {
                let state = state_from_app(&app);
                if !state.sessions.current(token)
                    || !state.is_streaming_active()
                    || !state.user_dictation_active.load(Ordering::SeqCst)
                {
                    break;
                }
                let Ok(mut guard) = state.stream.lock() else {
                    break;
                };
                let Some(pipe) = guard.as_mut().filter(|p| p.token == token) else {
                    break;
                };
                if !state.is_streaming_active() || !state.sessions.current(token) {
                    break;
                }
                // Watermark advance and waveform acceptance are one ownership step.
                let samples = state.recorder.take_since(&mut pipe.watermark);
                pipe.total_fed += samples.len();
                if !samples.is_empty() {
                    pipe.recognizer.accept(&pipe.session, &samples);
                    pipe.recognizer.drain(&pipe.session);
                }
                let raw = pipe.recognizer.result(&pipe.session);
                if !raw.is_empty() && !state.caption_active.load(Ordering::SeqCst) {
                    if let Ok(_commit) = state.sessions.commit_guard(token) {
                        if state.is_streaming_active() {
                            let _ = app.emit(
                                "partial",
                                serde_json::json!({ "text": raw, "streaming": true }),
                            );
                            dock::set_caption(&app, Some(&raw));
                        }
                    }
                }
                if pipe.recognizer.is_endpoint(&pipe.session) {
                    let duration = pipe.session.started_at.elapsed().as_millis() as u64;
                    // Reserve inference order before releasing the pipe; manual
                    // finalization cannot overtake this endpoint's final transcript.
                    let inference = state.sessions.inference.lock();
                    pipe.recognizer.reset(&mut pipe.session);
                    drop(guard);
                    let result = inference.map_err(|e| e.to_string()).and_then(|_inference| {
                        finalize_transcript(&app, &state, &raw, duration, token, true, false)
                    });
                    if let Err(error) = result {
                        report_error(&app, &state, token, &error);
                    }
                } else {
                    drop(guard);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        })
        .map_err(|e| format!("failed to start streaming worker: {e}"))?;
    Ok(())
}

fn spawn_caption_loop(app: &AppHandle, state: &AppState, token: WorkToken) {
    let Ok(mut cache) = state.caption_engine.lock() else {
        return;
    };
    let recognizer = if let Some(existing) = cache.as_ref() {
        Arc::clone(&existing.recognizer)
    } else {
        let created = match StreamingRecognizer::new(&models::caption_model_dir()) {
            Ok(r) => Arc::new(r),
            Err(error) => {
                log::warn!("captions unavailable: {error}");
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
    let session = recognizer.create_session(None);
    *state
        .caption_stream
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(StreamingPipe {
        token,
        recognizer,
        session,
        watermark: 0,
        total_fed: 0,
    });
    state.caption_active.store(true, Ordering::SeqCst);
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            let state = state_from_app(&app);
            if !state.sessions.current(token)
                || !state.caption_active.load(Ordering::SeqCst)
                || !state.user_dictation_active.load(Ordering::SeqCst)
            {
                break;
            }
            let Ok(mut guard) = state.caption_stream.lock() else {
                break;
            };
            let Some(pipe) = guard.as_mut().filter(|p| p.token == token) else {
                break;
            };
            if !state.sessions.current(token) || !state.caption_active.load(Ordering::SeqCst) {
                break;
            }
            let samples = state.recorder.take_since(&mut pipe.watermark);
            pipe.total_fed += samples.len();
            if !samples.is_empty() {
                pipe.recognizer.accept(&pipe.session, &samples);
                pipe.recognizer.drain(&pipe.session);
            }
            let text = pipe.recognizer.result(&pipe.session);
            if !text.is_empty() {
                if let Ok(_commit) = state.sessions.commit_guard(token) {
                    if state.caption_active.load(Ordering::SeqCst) {
                        let _ = app.emit(
                            "partial",
                            serde_json::json!({ "text": text, "streaming": true }),
                        );
                        dock::set_caption(&app, Some(&text));
                    }
                }
            }
            drop(guard);
            std::thread::sleep(Duration::from_millis(100));
        }
        let state = state_from_app(&app);
        if let Ok(mut guard) = state.caption_stream.lock() {
            if guard.as_ref().is_some_and(|p| p.token == token) {
                *guard = None;
                if state.sessions.current(token) {
                    state.caption_active.store(false, Ordering::SeqCst);
                }
            }
        };
    });
}

fn spawn_level_emitter(app: &AppHandle, _state: &AppState, token: WorkToken) {
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            let state = state_from_app(&app);
            {
                let Ok(_commit) = state.sessions.commit_guard(token) else {
                    break;
                };
                if !state.user_dictation_active.load(Ordering::SeqCst) {
                    break;
                }
                let _ = app.emit(
                    "audio-level",
                    serde_json::json!({ "rms": state.recorder.current_rms() }),
                );
            }
            std::thread::sleep(Duration::from_millis(33));
        }
        let state = state_from_app(&app);
        if let Ok(_commit) = state.sessions.commit_guard(token) {
            let _ = app.emit("audio-level", serde_json::json!({ "rms": 0.0 }));
        };
    });
}

fn process_utterance(
    app: &AppHandle,
    state: &AppState,
    audio: &[f32],
    token: WorkToken,
    continuous: bool,
    finish: bool,
) -> Result<TranscriptResult, String> {
    ensure_current(state, token)?;
    let _inference = state.sessions.inference.lock().map_err(|e| e.to_string())?;
    ensure_current(state, token)?;
    let speech = run_vad(state, audio).map_err(|e| e.to_string())?;
    let final_audio = if speech.has_speech {
        speech.trimmed_audio
    } else if !continuous && audio.len() >= 2400 {
        let trimmed = opendictate_core::audio::vad::apply_energy_vad_with_config(
            audio,
            &opendictate_core::audio::vad::VadConfig {
                energy_threshold: 0.0005,
                frame_size: 480,
                min_speech_frames: 1,
                hangover_frames: 15,
            },
        );
        if trimmed.has_speech {
            trimmed.trimmed_audio
        } else {
            audio.to_vec()
        }
    } else {
        Vec::new()
    };
    let duration_ms = final_audio.len() as u64 * 1000 / 16_000;
    if final_audio.is_empty() {
        return finalize_transcript(app, state, "", 0, token, continuous, finish);
    }
    ensure_current(state, token)?;
    let raw = transcribe_utterance(state, &final_audio)?;
    finalize_transcript(app, state, &raw, duration_ms, token, continuous, finish)
}

/// Decodes a complete utterance with the selected model. Handsfree buffers
/// whole utterances even when that model is a streaming one, and a streaming
/// model cannot be loaded by the offline engine.
fn transcribe_utterance(state: &AppState, audio: &[f32]) -> Result<String, String> {
    let hotwords = dictionary_hotwords(&dictionary_terms(state));
    if models::is_streaming_model(&selected_model_id(state)) {
        let recognizer = warmup_streaming(state)?;
        let session = recognizer.create_session(hotwords.as_deref());
        recognizer.accept(&session, audio);
        recognizer.finish(&session);
        return Ok(recognizer.result(&session));
    }
    load_engine(state)?
        .transcribe(audio, hotwords.as_deref())
        .map_err(|e| e.to_string())
}

fn normalize_transcript(raw: &str, punctuation: bool, dictionary: &[String]) -> String {
    let mapped = if punctuation {
        opendictate_core::text::map_spoken_punctuation(raw)
    } else {
        raw.to_string()
    };
    opendictate_core::text::correct_dictionary_terms(&inject::clean_text(&mapped), dictionary)
}

#[derive(Debug, PartialEq, Eq)]
struct SnippetCommand {
    name: String,
    tail: String,
}

struct PreparedTranscript {
    text: String,
    action: Option<opendictate_core::text::VoiceAction>,
    snippet: Option<SnippetCommand>,
}

fn split_snippet_payload(payload: &str) -> Option<SnippetCommand> {
    let mut parts = payload.trim().splitn(2, char::is_whitespace);
    // The recognizer ends a sentence right after the name: "Insert snippet sig."
    let name = parts
        .next()
        .unwrap_or("")
        .trim_end_matches(['.', ',', '!', '?', ';', ':', '\u{2026}']);
    if name.is_empty() {
        return None;
    }
    Some(SnippetCommand {
        name: name.to_string(),
        tail: parts.next().unwrap_or("").trim().to_string(),
    })
}

fn parse_snippet_command(text: &str, aliases_enabled: bool) -> Option<SnippetCommand> {
    let original = text.trim();
    let prefixes = if aliases_enabled {
        &["insert snippet ", "paste snippet "][..]
    } else {
        &["insert snippet "][..]
    };
    for prefix in prefixes {
        if original
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        {
            // Prefix matching may ignore case, but the payload is always sliced
            // from the original input; URL/code/prompt suffixes are opaque data.
            return split_snippet_payload(original.get(prefix.len()..)?);
        }
    }
    None
}

fn prepare_transcript(
    raw: &str,
    punctuation: bool,
    dictionary: &[String],
    actions_enabled: bool,
) -> PreparedTranscript {
    let original = raw.trim();
    let action = if actions_enabled {
        opendictate_core::text::parse_voice_action(original)
    } else {
        None
    };
    let snippet = parse_snippet_command(original, actions_enabled);
    // Punctuation mapping, sentence capitalization and dictionary correction
    // apply only to prose. Running them before command matching can mutate a
    // case-sensitive URL/path or turn literal payload words into punctuation.
    let text = if action.is_some() || snippet.is_some() {
        original.to_string()
    } else {
        normalize_transcript(original, punctuation, dictionary)
    };
    PreparedTranscript {
        text,
        action,
        snippet,
    }
}

/// "go to sleep" only means something to the Handsfree listener. In an explicit
/// recording it is dictated text, not a command that silently discards it.
fn action_applies(action: &opendictate_core::text::VoiceAction, handsfree: bool) -> bool {
    handsfree || *action != opendictate_core::text::VoiceAction::Sleep
}

/// Whether a Handsfree transcript is only a wake phrase repeated while already
/// awake. Typing it would put the wake phrase into the user's document.
fn is_wake_phrase_echo(raw: &str, wake_words: &str) -> bool {
    let key = |value: &str| opendictate_core::text::command_key(value).replace(',', "");
    let spoken = key(raw);
    !spoken.is_empty() && wake_words.split(',').any(|phrase| key(phrase) == spoken)
}

/// "switch to" and "open app" can only be commands. "open x", "focus x" and
/// "go to x" can also be sentences, so those stay dictation when the named
/// application is not available.
fn is_explicit_app_switch(text: &str) -> bool {
    let text = text.trim_start().to_lowercase();
    text.starts_with("switch to") || text.starts_with("open app")
}

fn finalize_transcript(
    app: &AppHandle,
    state: &AppState,
    raw: &str,
    duration_ms: u64,
    token: WorkToken,
    continuous: bool,
    finish: bool,
) -> Result<TranscriptResult, String> {
    ensure_current(state, token)?;
    let dictionary = dictionary_terms(state);
    let (punctuation, actions_enabled, wake_words) = state
        .settings
        .lock()
        .map(|s| {
            (
                s.spoken_punctuation,
                s.voice_actions_enabled,
                s.wake_words.clone(),
            )
        })
        .unwrap_or((false, true, String::new()));
    let handsfree = token.handsfree_generation.is_some();
    if handsfree && is_wake_phrase_echo(raw, &wake_words) {
        return Ok(TranscriptResult {
            text: String::new(),
            duration_ms,
        });
    }
    let mut prepared = prepare_transcript(raw, punctuation, &dictionary, actions_enabled);
    if prepared
        .action
        .as_ref()
        .is_some_and(|action| !action_applies(action, handsfree))
    {
        prepared.action = None;
        prepared.text = normalize_transcript(raw.trim(), punctuation, &dictionary);
    }
    let was_command = prepared.action.is_some() || prepared.snippet.is_some();
    let text = prepared.text.clone();
    if text.is_empty() {
        let _commit = state.sessions.commit_guard(token)?;
        if finish {
            restore_dock(app, state);
            if !continuous {
                notify::notify("No speech detected", "Try again or check your microphone");
            }
        }
        return Ok(TranscriptResult { text, duration_ms });
    }
    if let Some(text) = handle_voice_action_or_snippet(app, state, &prepared, token)? {
        return Ok(TranscriptResult { text, duration_ms });
    }
    // A command that could not run falls through as the sentence it also was;
    // it needs the prose normalization that command matching skipped.
    let text = if was_command {
        normalize_transcript(raw.trim(), punctuation, &dictionary)
    } else {
        text
    };
    ensure_current(state, token)?;
    let text = opendictate_core::text::correct_dictionary_terms(
        &apply_voice_polish(state, &text),
        &dictionary,
    );
    commit_text(app, state, &text, duration_ms, token, continuous, finish)?;
    Ok(TranscriptResult { text, duration_ms })
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

fn run_vad(
    state: &AppState,
    audio: &[f32],
) -> Result<opendictate_core::audio::vad::VadResult, opendictate_core::CoreError> {
    let sensitivity = sensitivity(state);
    let vad_path = models::vad_model_path();
    let silero = if models::is_vad_ready() {
        let threshold = sensitivity_to_silero_threshold(sensitivity);
        let mut cache = state.vad.lock().map_err(|_| {
            opendictate_core::CoreError::Audio("VAD cache lock poisoned".to_string())
        })?;
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
    if terms.is_empty() {
        None
    } else {
        Some(terms.join("\n"))
    }
}

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
    } else if models::is_omnilingual_model(&model_id) {
        ModelKind::OmnilingualCtc
    } else if models::is_cohere_model(&model_id) {
        ModelKind::CohereTranscribe
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
    log::info!(
        "offline engine ready in {} ms",
        load_started.elapsed().as_millis()
    );
    Ok(engine)
}

/// Whether a spoken terminal command also presses Enter. Only read-only commands
/// run at once, and only in the Terminal input target: a misheard "git push"
/// must not change a repository, and Enter means something else in a document.
/// Otherwise the command is only typed, and saying "send" runs it.
fn terminal_command_runs_at_once(command: &str, terminal_target: bool) -> bool {
    terminal_target && matches!(command, "git status" | "git diff" | "git log" | "clear")
}

/// Actions delivered as keystrokes to whichever window holds the focus.
fn sends_editing_keys(action: &opendictate_core::text::VoiceAction) -> bool {
    use opendictate_core::text::VoiceAction::*;
    matches!(
        action,
        Undo | DeleteWord
            | DeleteLine
            | ClearAll
            | NewLine
            | NewParagraph
            | Tab
            | Submit
            | Interrupt
            | NextTab
            | PrevTab
            | NewTab
            | CloseTab
            | ScrollDown
            | ScrollUp
    )
}

fn handle_voice_action_or_snippet(
    app: &AppHandle,
    state: &AppState,
    prepared: &PreparedTranscript,
    token: WorkToken,
) -> Result<Option<String>, String> {
    use opendictate_core::text::VoiceAction;
    let _commit = state.sessions.commit_guard(token)?;
    ensure_current(state, token)?;
    if let Some(snippet) = prepared.snippet.as_ref() {
        let result = handle_snippet_command_locked(app, state, snippet, token)?;
        if result.is_none() {
            notify::notify(
                "Snippet not found",
                &format!("No snippet named \"{}\". Inserted your words as text.", snippet.name),
            );
        }
        return Ok(result);
    }
    let action = prepared.action.clone();
    if action.as_ref().is_some_and(sends_editing_keys) {
        // These keystrokes would edit OpenDictate's own fields, for example
        // "clear all" while the Settings window is focused.
        inject::require_external_focus(app)?;
    }
    let label = match action {
        Some(VoiceAction::Undo) => {
            inject::undo_in_target(app)?;
            // The insertion has been undone: the UI must not offer another
            // Undo for it, or a second press would remove an earlier
            // insertion.
            *state.last_inserted.lock().map_err(|e| e.to_string())? = None;
            "Undo".to_string()
        }
        Some(VoiceAction::DeleteWord) => {
            inject::delete_word_in_target(app)?;
            "Delete Word".to_string()
        }
        Some(VoiceAction::DeleteLine) => {
            inject::press_delete_line(app)?;
            "Delete Line".to_string()
        }
        Some(VoiceAction::ClearAll) => {
            inject::clear_all_in_target(app)?;
            "Clear All".to_string()
        }
        Some(VoiceAction::NewLine) => {
            inject::press_new_line()?;
            "New Line".to_string()
        }
        Some(VoiceAction::NewParagraph) => {
            inject::press_new_paragraph()?;
            "New Paragraph".to_string()
        }
        Some(VoiceAction::Tab) => {
            inject::press_tab()?;
            "Tab".to_string()
        }
        Some(VoiceAction::BulletPoint) => {
            commit_text_locked(app, state, "• ", 0, token, false, true)?;
            return Ok(Some("• ".to_string()));
        }
        Some(
            VoiceAction::AllCaps(phrase)
            | VoiceAction::CamelCase(phrase)
            | VoiceAction::SnakeCase(phrase)
            | VoiceAction::TitleCase(phrase),
        ) => {
            commit_text_locked(app, state, &phrase, 0, token, false, true)?;
            return Ok(Some(phrase));
        }
        Some(VoiceAction::TerminalCommand(text))
            if !terminal_command_runs_at_once(&text, inject::uses_terminal_shortcuts(app)?) =>
        {
            commit_text_locked(app, state, &text, 0, token, false, true)?;
            return Ok(Some(text));
        }
        Some(VoiceAction::PromptAndSend(text) | VoiceAction::TerminalCommand(text)) => {
            let inserted = submit_after_insert(
                || commit_text_locked(app, state, &text, 0, token, false, false),
                || {
                    std::thread::sleep(Duration::from_millis(60));
                    inject::press_enter()
                },
            )?;
            if inserted {
                show_completed(app, state, token, "✓ Sent");
            }
            return Ok(Some(text));
        }
        Some(VoiceAction::Submit) => {
            inject::press_enter()?;
            "Sent".to_string()
        }
        Some(VoiceAction::Interrupt) => {
            inject::press_interrupt()?;
            "Stopped (Ctrl+C)".to_string()
        }
        Some(VoiceAction::SwitchApp(name)) => {
            if let Err(error) = inject::switch_to_app(&name) {
                if is_explicit_app_switch(&prepared.text) {
                    return Err(error);
                }
                log::info!("treating as dictation: {error}");
                return Ok(None);
            }
            format!("Switched to {name}")
        }
        Some(VoiceAction::NextTab) => {
            inject::press_next_tab()?;
            "Next Tab".to_string()
        }
        Some(VoiceAction::PrevTab) => {
            inject::press_prev_tab()?;
            "Previous Tab".to_string()
        }
        Some(VoiceAction::NewTab) => {
            inject::press_new_tab()?;
            "New Tab".to_string()
        }
        Some(VoiceAction::CloseTab) => {
            inject::press_close_tab()?;
            "Close Tab".to_string()
        }
        Some(VoiceAction::ScrollDown) => {
            inject::press_scroll_down()?;
            "Scroll Down".to_string()
        }
        Some(VoiceAction::ScrollUp) => {
            inject::press_scroll_up()?;
            "Scroll Up".to_string()
        }
        Some(VoiceAction::WebSearch(query)) => {
            inject::open_browser_search(&query)?;
            format!("Search: {query}")
        }
        Some(VoiceAction::OpenUrl(url)) => {
            inject::open_browser_url(&url)?;
            format!("Opened {url}")
        }
        Some(VoiceAction::InsertSnippet(name)) => {
            return match split_snippet_payload(&name) {
                Some(command) => handle_snippet_command_locked(app, state, &command, token),
                None => Ok(None),
            };
        }
        Some(VoiceAction::Sleep) => {
            state.handsfree_awake.store(false, Ordering::SeqCst);
            state.handsfree_speaking.store(false, Ordering::SeqCst);
            restore_dock(app, state);
            play_sound(state, crate::audio::SoundEvent::Sleeping);
            return Ok(Some("✓ Sleeping".to_string()));
        }
        None => return Ok(None),
    };
    let label = format!("✓ {label}");
    show_completed(app, state, token, &label);
    Ok(Some(label))
}

fn handle_snippet_command_locked(
    app: &AppHandle,
    state: &AppState,
    command: &SnippetCommand,
    token: WorkToken,
) -> Result<Option<String>, String> {
    let name = command.name.as_str();
    let tail = command.tail.as_str();
    let list = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::list_snippets(&conn).map_err(|e| e.to_string())?
    };
    let triggers = list
        .iter()
        .filter(|s| opendictate_core::text::is_single_word(&s.trigger))
        .map(|s| s.trigger.clone())
        .collect::<Vec<_>>();
    let Some(snippet) = opendictate_core::text::fuzzy_match_trigger(name, &triggers, 0.6)
        .and_then(|(trigger, _)| list.into_iter().find(|s| s.trigger == trigger))
    else {
        return Ok(None);
    };
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let clipboard = app.clipboard().read_text().ok();
    let expanded =
        opendictate_core::text::expand_snippet_template(&snippet.text, clipboard.as_deref());
    let text = if tail.is_empty() {
        expanded
    } else {
        format!("{} {tail}", expanded.trim())
    };
    *state.last_inserted.lock().map_err(|e| e.to_string())? = None;
    let mode = insert_mode(state);
    if let Err(error) = inject::inject_text(app, &text, &mode) {
        let _ = app.emit(
            "transcript",
            serde_json::json!({
                "text": text,
                "injected": false,
                "error": error,
                "duration_ms": 0,
            }),
        );
        return Err(format!("Failed to insert snippet: {error}"));
    }
    let inserted = mode != "clipboard";
    if inserted {
        *state.last_inserted.lock().map_err(|e| e.to_string())? = Some(text.clone());
    }
    let _ = app.emit(
        "transcript",
        serde_json::json!({
            "text": text,
            "injected": inserted,
            "copied": !inserted,
            "duration_ms": 0,
        }),
    );
    show_completed(
        app,
        state,
        token,
        &if inserted {
            pill_text(&text)
        } else {
            "Copied to clipboard".into()
        },
    );
    Ok(Some(text))
}

fn pill_text(text: &str) -> String {
    let mut label: String = text.trim().chars().take(48).collect();
    if text.trim().chars().count() > 48 {
        label.push('…');
    }
    label
}

fn restore_dock(app: &AppHandle, state: &AppState) {
    let awake = state.handsfree_awake.load(Ordering::SeqCst);
    if state.user_dictation_active.load(Ordering::SeqCst) || awake {
        if awake && state.handsfree_speaking.load(Ordering::SeqCst) {
            // Do not show "listening" over an utterance still in progress.
            dock::set_state(app, "recording", None);
            dock::set_caption(app, Some("Recording…"));
            return;
        }
        dock::set_state(app, "listening", None);
        dock::set_caption(app, Some("Listening…"));
    } else {
        dock::set_state(app, "hidden", None);
        if state.handsfree_active.load(Ordering::SeqCst) {
            dock::set_caption(app, Some("Handsfree: Sleeping"));
        } else {
            dock::set_caption(app, None);
        }
    }
}

fn show_completed(app: &AppHandle, state: &AppState, token: WorkToken, label: &str) {
    dock::set_state(app, "inserted", None);
    dock::set_caption(app, Some(label));
    play_sound(state, crate::audio::SoundEvent::Inserted);
    restore_dock_after(app, token, Duration::from_millis(1500));
}

fn commit_text(
    app: &AppHandle,
    state: &AppState,
    text: &str,
    duration_ms: u64,
    token: WorkToken,
    continuous: bool,
    finish: bool,
) -> Result<bool, String> {
    let _commit = state.sessions.commit_guard(token)?;
    ensure_current(state, token)?;
    commit_text_locked(app, state, text, duration_ms, token, continuous, finish)
}

/// The caller holds the session's commit guard through persistence, actual
/// insertion and optional Enter. A failed insert remains recoverable in history.
fn commit_text_locked(
    app: &AppHandle,
    state: &AppState,
    text: &str,
    duration_ms: u64,
    token: WorkToken,
    continuous: bool,
    finish: bool,
) -> Result<bool, String> {
    if text.is_empty() {
        return Ok(false);
    }
    *state.last_inserted.lock().map_err(|e| e.to_string())? = None;
    let entry = HistoryEntry {
        id: 0,
        text: text.to_string(),
        created_at: db::now_timestamp(),
        duration_ms,
        source: if continuous {
            "continuous".into()
        } else {
            "hotkey".into()
        },
    };
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::insert_history(&conn, &entry).map_err(|e| format!("failed to save transcript: {e}"))?;
    }
    let _ = app.emit("history-updated", serde_json::json!({}));
    let mode = insert_mode(state);
    if let Err(error) = inject::inject_text(app, text, &mode) {
        let _ = app.emit(
            "transcript",
            serde_json::json!({
                "text": text,
                "injected": false,
                "error": error,
                "duration_ms": duration_ms,
            }),
        );
        return Err(format!("Failed to insert text (saved in History): {error}"));
    }
    let inserted = mode != "clipboard";
    if inserted {
        *state.last_inserted.lock().map_err(|e| e.to_string())? = Some(text.to_string());
    }
    let _ = app.emit(
        "transcript",
        serde_json::json!({
            "text": text,
            "injected": inserted,
            "copied": !inserted,
            "duration_ms": duration_ms,
        }),
    );
    if finish || !continuous {
        let label = if inserted {
            pill_text(text)
        } else {
            "Copied to clipboard".into()
        };
        notify::notify(if inserted { "Inserted" } else { "Copied" }, &label);
        show_completed(app, state, token, &label);
    } else {
        restore_dock(app, state);
        play_sound(state, crate::audio::SoundEvent::Inserted);
    }
    Ok(inserted)
}

pub fn ensure_kws_spotter(
    state: &AppState,
) -> Result<Arc<opendictate_core::stt::kws::Spotter>, String> {
    // Settings are read before the cache lock; settings application never nests
    // those locks in the inverse order.
    let wake_words = state
        .settings
        .lock()
        .map_err(|e| e.to_string())?
        .wake_words
        .clone();
    let mut cache = state.kws_engine.lock().map_err(|e| e.to_string())?;
    if let Some(existing) = cache.as_ref().filter(|c| c.wake_words == wake_words) {
        return Ok(Arc::clone(&existing.spotter));
    }
    if !models::is_kws_ready() {
        return Err("KWS model is not downloaded yet — download it in Settings → Models".into());
    }
    let keywords = wake_words
        .split(',')
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>();
    let spotter = Arc::new(
        opendictate_core::stt::kws::Spotter::new(&models::kws_model_dir(), &keywords, 0.25)
            .map_err(|e| e.to_string())?,
    );
    *cache = Some(crate::state::CachedKwsEngine {
        wake_words,
        spotter: Arc::clone(&spotter),
    });
    Ok(spotter)
}

pub fn start_handsfree(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let _transition = state
        .sessions
        .transition
        .lock()
        .map_err(|e| e.to_string())?;
    start_handsfree_locked(app, state)
}

pub(crate) fn start_handsfree_locked(app: &AppHandle, state: &AppState) -> Result<(), String> {
    if state.handsfree_active.load(Ordering::SeqCst) {
        return Ok(());
    }
    let spotter = ensure_kws_spotter(state)?;
    let settings = state.settings.lock().map_err(|e| e.to_string())?.clone();
    let started_capture = !state.recorder.is_recording();
    if started_capture {
        start_capture(state)?;
    }
    let generation = state.sessions.restart_handsfree();
    state.handsfree_awake.store(false, Ordering::SeqCst);
    // Keep the child behind a startup latch until spawning succeeds and the
    // service is published as active. Startup failure never leaves active=true.
    let (ready_tx, ready_rx) = mpsc::sync_channel::<()>(0);
    let worker_app = app.clone();
    let worker = std::thread::Builder::new()
        .name("opendictate-handsfree".into())
        .spawn(move || {
            if ready_rx.recv().is_err() {
                return;
            }
            let app = worker_app;
            let state = state_from_app(&app);
            let mut keyword_session = spotter.create_session();
            let mut watermark = state.recorder.sample_position();
            let mut boundary = state.sessions.token().generation;
            let mut was_explicit = false;
            let mut speech = SpeechBuffer::new(15 * 16_000, 8_000);
            let mut frames: VecDeque<bool> = VecDeque::with_capacity(8);
            let mut silent_since = Instant::now();
            let mut last_awake_activity = Instant::now();
            let mut noise_gate = NoiseGate::default();
            let mut next_mic_attempt = Instant::now();
            let mut mic_lost = false;
            let mut silero_sensitivity = settings.vad_sensitivity;
            let mut silero = if models::is_vad_ready() {
                SileroVad::with_threshold(
                    &models::vad_model_path(),
                    sensitivity_to_silero_threshold(settings.vad_sensitivity),
                )
                .inspect_err(|error| {
                    log::warn!("handsfree: Silero VAD unavailable, using energy: {error}")
                })
                .ok()
            } else {
                None
            };
            while state.sessions.handsfree_current(generation)
                && state.handsfree_active.load(Ordering::SeqCst)
            {
                std::thread::sleep(Duration::from_millis(60));
                let Ok(transition) = state.sessions.transition.lock() else {
                    break;
                };
                if !state.sessions.handsfree_current(generation)
                    || !state.handsfree_active.load(Ordering::SeqCst)
                {
                    break;
                }
                let explicit = state.user_dictation_active.load(Ordering::SeqCst);
                let current_boundary = state.sessions.token().generation;
                if explicit || was_explicit || boundary != current_boundary {
                    speech.take();
                    frames.clear();
                    keyword_session = spotter.create_session();
                    if let Some(ref detector) = silero {
                        detector.reset();
                    }
                    state.handsfree_speaking.store(false, Ordering::SeqCst);
                    let was_awake = state.handsfree_awake.swap(false, Ordering::SeqCst);
                    if was_awake && !explicit {
                        // A cancellation showed the dock for the awake state
                        // that has just ended.
                        restore_dock(&app, &state);
                    }
                    watermark = state.recorder.sample_position();
                    boundary = current_boundary;
                    was_explicit = explicit;
                    if explicit {
                        continue;
                    }
                }
                if !state.recorder.is_recording() {
                    // A microphone can disappear and come back (a USB or
                    // Bluetooth headset). Keep retrying instead of leaving
                    // Handsfree off while Settings still shows it as active.
                    if Instant::now() < next_mic_attempt {
                        continue;
                    }
                    next_mic_attempt = Instant::now() + MIC_RETRY_INTERVAL;
                    match start_capture(&state) {
                        Ok(()) => {
                            watermark = state.recorder.sample_position();
                            if mic_lost {
                                mic_lost = false;
                                notify::notify("Handsfree resumed", "The microphone is back.");
                            }
                        }
                        Err(error) => {
                            if !mic_lost {
                                mic_lost = true;
                                speech.take();
                                frames.clear();
                                state.handsfree_awake.store(false, Ordering::SeqCst);
                                restore_dock(&app, &state);
                                notify::notify(
                                    "Handsfree paused",
                                    &format!("Waiting for the microphone: {error}"),
                                );
                            }
                            continue;
                        }
                    }
                }
                let token = state.sessions.handsfree_token();
                let samples = state.recorder.take_since(&mut watermark);
                drop(transition);
                if samples.is_empty() {
                    continue;
                }
                let rms =
                    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
                let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                // Read live: moving the sensitivity slider must not restart the
                // listener or put it back to sleep.
                let current_sensitivity = sensitivity(&state);
                let base_threshold = sensitivity_to_energy_threshold(current_sensitivity);
                if !state.handsfree_awake.load(Ordering::SeqCst) {
                    state.handsfree_speaking.store(false, Ordering::SeqCst);
                    speech.take();
                    frames.clear();
                    noise_gate.classify(rms, peak, false, base_threshold);
                    if let Some(keyword) = spotter.process_samples(&keyword_session, &samples) {
                        if let Ok(_commit) = state.sessions.commit_guard(token) {
                            if !work_current(&state, token) {
                                continue;
                            }
                            state.handsfree_awake.store(true, Ordering::SeqCst);
                            last_awake_activity = Instant::now();
                            silent_since = Instant::now();
                            keyword_session = spotter.create_session();
                            if let Some(ref detector) = silero {
                                detector.reset();
                            }
                            restore_dock(&app, &state);
                            notify::notify("Handsfree awake", &format!("Detected {keyword}"));
                            play_sound(&state, crate::audio::SoundEvent::Listening);
                        }
                    }
                    continue;
                }
                // Swap detectors between utterances; a new one has no memory of
                // the speech in progress.
                if silero.is_some()
                    && !speech.active
                    && (silero_sensitivity - current_sensitivity).abs() > f32::EPSILON
                {
                    silero_sensitivity = current_sensitivity;
                    match SileroVad::with_threshold(
                        &models::vad_model_path(),
                        sensitivity_to_silero_threshold(current_sensitivity),
                    ) {
                        Ok(detector) => silero = Some(detector),
                        Err(error) => {
                            log::warn!("handsfree: keeping the previous VAD threshold: {error}")
                        }
                    }
                }
                let is_speech = if let Some(ref detector) = silero {
                    detector.accept_streaming(&samples)
                } else {
                    noise_gate.classify(rms, peak, speech.active, base_threshold)
                };
                frames.push_back(is_speech);
                if frames.len() > 8 {
                    frames.pop_front();
                }
                let silence_confirmed =
                    frames.len() == 8 && frames.iter().filter(|&&s| !s).count() >= 7;
                if is_speech {
                    last_awake_activity = Instant::now();
                }
                if is_speech || !silence_confirmed {
                    silent_since = Instant::now();
                }
                let was_active = speech.active;
                let mut completed = speech.push(&samples, is_speech);
                if !was_active && speech.active && work_current(&state, token) {
                    state.handsfree_speaking.store(true, Ordering::SeqCst);
                    restore_dock(&app, &state);
                }
                if speech.active
                    && silence_confirmed
                    && silent_since.elapsed() >= Duration::from_millis(500)
                {
                    let audio = speech.take();
                    state.handsfree_speaking.store(false, Ordering::SeqCst);
                    if audio.len() >= 6400 {
                        completed.push(audio);
                    } else if work_current(&state, token) {
                        restore_dock(&app, &state);
                    }
                    frames.clear();
                }
                for audio in completed {
                    if !work_current(&state, token) {
                        break;
                    }
                    dock::set_state(&app, "transcribing", None);
                    dock::set_caption(&app, Some("transcribing…"));
                    match process_utterance(&app, &state, &audio, token, true, false) {
                        Err(error) => report_error(&app, &state, token, &error),
                        // Silence, noise, or a repeated wake phrase: nothing
                        // was inserted that would have updated the dock.
                        Ok(result) if result.text.is_empty() => {
                            if let Ok(_commit) = state.sessions.commit_guard(token) {
                                if work_current(&state, token) {
                                    restore_dock(&app, &state);
                                }
                            }
                        }
                        Ok(_) => {}
                    }
                    last_awake_activity = Instant::now();
                }
                // Read live: moving the timeout slider must not restart the
                // listener or put it back to sleep.
                let silence_timeout = state
                    .settings
                    .lock()
                    .map(|s| s.handsfree_silence_timeout_sec)
                    .unwrap_or(settings.handsfree_silence_timeout_sec);
                if !speech.active
                    && last_awake_activity.elapsed() > Duration::from_secs(silence_timeout as u64)
                {
                    if let Ok(_commit) = state.sessions.commit_guard(token) {
                        if work_current(&state, token) {
                            state.handsfree_awake.store(false, Ordering::SeqCst);
                            speech.take();
                            frames.clear();
                            keyword_session = spotter.create_session();
                            if let Some(ref detector) = silero {
                                detector.reset();
                            }
                            restore_dock(&app, &state);
                            play_sound(&state, crate::audio::SoundEvent::Sleeping);
                        }
                    }
                }
            }
        });
    if let Err(error) = worker {
        if started_capture && !state.user_dictation_active.load(Ordering::SeqCst) {
            let _ = state.recorder.cancel();
        }
        return Err(format!("failed to start handsfree worker: {error}"));
    }
    state.handsfree_active.store(true, Ordering::SeqCst);
    if ready_tx.send(()).is_err() {
        state.handsfree_active.store(false, Ordering::SeqCst);
        if started_capture {
            let _ = state.recorder.cancel();
        }
        return Err("handsfree worker exited during startup".into());
    }
    if !state.user_dictation_active.load(Ordering::SeqCst) {
        restore_dock(app, state);
    }
    Ok(())
}

pub(crate) fn stop_handsfree_locked(app: &AppHandle, state: &AppState) -> Result<(), String> {
    state.sessions.restart_handsfree();
    state.handsfree_active.store(false, Ordering::SeqCst);
    state.handsfree_awake.store(false, Ordering::SeqCst);
    state.handsfree_speaking.store(false, Ordering::SeqCst);
    if !state.user_dictation_active.load(Ordering::SeqCst) {
        cancel_capture(state)?;
        restore_dock(app, state);
    }
    Ok(())
}

#[cfg(test)]
mod session_regressions {
    use super::*;

    #[test]
    fn microphone_test_needs_only_audio_and_does_not_return_recognized_actions() {
        let result = microphone_test_result(&vec![0.2; 16_000]);
        assert_eq!(result.text, "Microphone detected audio");
        assert_eq!(result.duration_ms, 1000);
        assert!(microphone_test_result(&[0.0; 160]).text.is_empty());
    }

    #[test]
    fn prepared_url_action_preserves_path_extension_and_query_case() {
        use opendictate_core::text::VoiceAction;
        let url = "https://Example.com/file.json?Token=AbC&key=a.b!";
        for punctuation in [false, true] {
            let prepared = prepare_transcript(
                &format!("OPEN URL {url}"),
                punctuation,
                &["JSON".into()],
                true,
            );
            assert_eq!(prepared.action, Some(VoiceAction::OpenUrl(url.into())));
        }
    }

    #[test]
    fn prepared_prompt_payload_is_not_spoken_punctuation_or_prose() {
        use opendictate_core::text::VoiceAction;
        let payload = "Explain period comma and question mark in file.json?";
        let prepared = prepare_transcript(&format!("ASK CLAUDE {payload}"), true, &[], true);
        assert_eq!(
            prepared.action,
            Some(VoiceAction::PromptAndSend(payload.into()))
        );
    }

    #[test]
    fn prepared_snippet_preserves_original_mixed_case_suffix() {
        let prepared = prepare_transcript(
            "Insert Snippet SiG KeepCase and file.json?Token=AbC",
            true,
            &[],
            true,
        );
        assert_eq!(
            prepared.snippet,
            Some(SnippetCommand {
                name: "SiG".into(),
                tail: "KeepCase and file.json?Token=AbC".into()
            })
        );
    }

    #[test]
    fn prepared_snippet_alias_keeps_unicode_and_literal_payload_words() {
        let prepared =
            prepare_transcript("PASTE SNIPPET MySig KeepCase Café period?", true, &[], true);
        assert_eq!(
            prepared.snippet,
            Some(SnippetCommand {
                name: "MySig".into(),
                tail: "KeepCase Café period?".into()
            })
        );
    }

    #[test]
    fn disabling_voice_actions_keeps_commands_out_of_the_action_dispatcher() {
        let prepared =
            prepare_transcript("OPEN URL https://example.com/file.json", true, &[], false);
        assert!(prepared.action.is_none());
        assert!(prepared.snippet.is_none());
    }

    #[test]
    fn snippet_name_drops_the_sentence_punctuation_after_it() {
        let prepared = prepare_transcript("Insert snippet sig.", true, &[], true);
        assert_eq!(
            prepared.snippet,
            Some(SnippetCommand {
                name: "sig".into(),
                tail: String::new()
            })
        );
    }

    #[test]
    fn sleep_is_a_command_only_for_the_handsfree_listener() {
        use opendictate_core::text::VoiceAction;
        assert!(action_applies(&VoiceAction::Sleep, true));
        assert!(!action_applies(&VoiceAction::Sleep, false));
        assert!(action_applies(&VoiceAction::Undo, false));
    }

    #[test]
    fn repeated_wake_phrase_is_recognized_but_sentences_containing_it_are_not() {
        let words = "hey dictate, computer";
        assert!(is_wake_phrase_echo("Hey, dictate.", words));
        assert!(is_wake_phrase_echo("COMPUTER!", words));
        assert!(!is_wake_phrase_echo("Computer science is fun.", words));
        assert!(!is_wake_phrase_echo("...", words));
    }

    #[test]
    fn only_unambiguous_app_commands_report_a_missing_application() {
        assert!(is_explicit_app_switch("Switch to Slack."));
        assert!(is_explicit_app_switch("open app chrome"));
        assert!(!is_explicit_app_switch("Open calculator"));
        assert!(!is_explicit_app_switch("Go to market"));
    }

    #[test]
    fn only_read_only_commands_run_at_once_and_only_in_a_terminal() {
        assert!(terminal_command_runs_at_once("git status", true));
        assert!(terminal_command_runs_at_once("clear", true));
        assert!(!terminal_command_runs_at_once("git push", true));
        assert!(!terminal_command_runs_at_once("git pull", true));
        assert!(!terminal_command_runs_at_once("git status", false));
    }

    #[test]
    fn keystroke_actions_are_guarded_but_app_and_browser_actions_are_not() {
        use opendictate_core::text::VoiceAction;
        assert!(sends_editing_keys(&VoiceAction::ClearAll));
        assert!(sends_editing_keys(&VoiceAction::Undo));
        assert!(!sends_editing_keys(&VoiceAction::SwitchApp("code".into())));
        assert!(!sends_editing_keys(&VoiceAction::OpenUrl("a.io".into())));
        assert!(!sends_editing_keys(&VoiceAction::Sleep));
    }

    #[test]
    fn hotwords_preserve_phrase_boundaries_without_fake_score_tokens() {
        assert_eq!(
            dictionary_hotwords(&["New York".into(), "GitHub".into()]),
            Some("New York\nGitHub".into())
        );
    }

    #[test]
    fn common_cleanup_preserves_canonical_dictionary_case() {
        assert_eq!(
            prepare_transcript("github is great", false, &["GitHub".into()], true).text,
            "GitHub is great"
        );
    }

    #[test]
    fn common_cleanup_maps_punctuation_before_dictionary() {
        assert_eq!(
            prepare_transcript("hello comma github", true, &["GitHub".into()], true).text,
            "Hello, GitHub"
        );
    }

    #[test]
    fn warmup_current_model_handles_missing_safely() {
        let state = AppState {
            sessions: Arc::new(crate::session::SessionControl::default()),
            recording_token: Arc::new(std::sync::Mutex::new(None)),
            continuous_queue: Arc::new(std::sync::Mutex::new(None)),
            recorder: Arc::new(opendictate_core::audio::capture::AudioRecorder::new()),
            test_mode: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            db: Arc::new(std::sync::Mutex::new(rusqlite::Connection::open_in_memory().unwrap())),
            settings: Arc::new(std::sync::Mutex::new(crate::state::Settings {
                stt_model: "nonexistent-model-id".into(),
                ..Default::default()
            })),
            hotkey: Arc::new(std::sync::Mutex::new(None)),
            continuous: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            stream: Arc::new(std::sync::Mutex::new(None)),
            stream_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            last_inserted: Arc::new(std::sync::Mutex::new(None)),
            stt_engine: Arc::new(std::sync::Mutex::new(None)),
            streaming_engine: Arc::new(std::sync::Mutex::new(None)),
            caption_engine: Arc::new(std::sync::Mutex::new(None)),
            caption_stream: Arc::new(std::sync::Mutex::new(None)),
            caption_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            kws_engine: Arc::new(std::sync::Mutex::new(None)),
            handsfree_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            handsfree_awake: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            handsfree_speaking: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            user_dictation_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            streaming_rtf_x100: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            gpu_active: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            vad: Arc::new(std::sync::Mutex::new(None)),
            active_downloads: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        };
        warmup_current_model(&state);
        assert!(state.stt_engine.lock().unwrap().is_none());
    }
}
