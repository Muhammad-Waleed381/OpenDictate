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

use crate::session::{submit_after_insert, SpeechBuffer, WorkToken};
use crate::state::{AppState, HistoryEntry, StreamingPipe, TranscriptResult};
use crate::{db, dock, inject, notify};

const MAX_OFFLINE_SAMPLES: usize = 110 * 16_000;
const CONTINUOUS_BACKLOG: usize = 2;

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
    }
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
    let engine = load_engine(state)?;
    let raw = engine
        .transcribe(
            &final_audio,
            dictionary_hotwords(&dictionary_terms(state)).as_deref(),
        )
        .map_err(|e| e.to_string())?;
    finalize_transcript(app, state, &raw, duration_ms, token, continuous, finish)
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
    let name = parts.next().unwrap_or("");
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
        &["insert snippet ", "paste snippet ", "snippet "][..]
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
    let (punctuation, actions_enabled) = state
        .settings
        .lock()
        .map(|s| (s.spoken_punctuation, s.voice_actions_enabled))
        .unwrap_or((false, true));
    let prepared = prepare_transcript(raw, punctuation, &dictionary, actions_enabled);
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
        return handle_snippet_command_locked(app, state, snippet, token);
    }
    let action = prepared.action.clone();
    let label = match action {
        Some(VoiceAction::Undo) => {
            inject::undo_last_insert()?;
            "Undo".to_string()
        }
        Some(VoiceAction::DeleteWord) => {
            inject::press_delete_word()?;
            "Delete Word".to_string()
        }
        Some(VoiceAction::DeleteLine) => {
            inject::press_delete_line(app)?;
            "Delete Line".to_string()
        }
        Some(VoiceAction::ClearAll) => {
            inject::press_clear_all()?;
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
            inject::switch_to_app(&name)?;
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
            return handle_snippet_command_locked(
                app,
                state,
                &split_snippet_payload(&name)
                    .ok_or_else(|| "snippet name is missing".to_string())?,
                token,
            );
        }
        Some(VoiceAction::Sleep) => {
            state.handsfree_awake.store(false, Ordering::SeqCst);
            restore_dock(app, state);
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
    let snippet = opendictate_core::text::fuzzy_match_trigger(name, &triggers, 0.6)
        .and_then(|(trigger, _)| list.into_iter().find(|s| s.trigger == trigger))
        .ok_or_else(|| format!("Snippet not found: {name}"))?;
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
            serde_json::json!({ "text": text, "injected": false, "error": error }),
        );
        return Err(format!("Failed to insert snippet: {error}"));
    }
    let inserted = mode != "clipboard";
    if inserted {
        *state.last_inserted.lock().map_err(|e| e.to_string())? = Some(text.clone());
    }
    let _ = app.emit(
        "transcript",
        serde_json::json!({ "text": text, "injected": inserted, "copied": !inserted }),
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
    if state.user_dictation_active.load(Ordering::SeqCst)
        || state.handsfree_awake.load(Ordering::SeqCst)
    {
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
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        let state = state_from_app(&app);
        if let Ok(_commit) = state.sessions.commit_guard(token) {
            restore_dock(&app, &state);
        };
    });
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
            serde_json::json!({ "text": text, "injected": false, "error": error }),
        );
        return Err(format!("Failed to insert text (saved in History): {error}"));
    }
    let inserted = mode != "clipboard";
    if inserted {
        *state.last_inserted.lock().map_err(|e| e.to_string())? = Some(text.to_string());
    }
    let _ = app.emit(
        "transcript",
        serde_json::json!({ "text": text, "injected": inserted, "copied": !inserted }),
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
            let mut noise_floor: f32 = 0.012;
            let silero = if models::is_vad_ready() {
                SileroVad::with_threshold(
                    &models::vad_model_path(),
                    sensitivity_to_silero_threshold(settings.vad_sensitivity),
                )
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
                    state.handsfree_awake.store(false, Ordering::SeqCst);
                    watermark = state.recorder.sample_position();
                    boundary = current_boundary;
                    was_explicit = explicit;
                    if explicit {
                        continue;
                    }
                }
                if !state.recorder.is_recording() {
                    if let Err(error) = start_capture(&state) {
                        state.handsfree_active.store(false, Ordering::SeqCst);
                        drop(transition);
                        notify::notify("Handsfree stopped", &error);
                        break;
                    }
                    watermark = state.recorder.sample_position();
                }
                let token = state.sessions.handsfree_token();
                let samples = state.recorder.take_since(&mut watermark);
                drop(transition);
                if samples.is_empty() {
                    continue;
                }
                let rms =
                    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
                if !state.handsfree_awake.load(Ordering::SeqCst) {
                    speech.take();
                    frames.clear();
                    noise_floor = (noise_floor * 0.9 + rms * 0.1).clamp(0.005, 0.12);
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
                let rate = if speech.active { 0.05 } else { 0.15 };
                noise_floor = (noise_floor * (1.0 - rate) + rms * rate).clamp(0.005, 0.15);
                let base = sensitivity_to_energy_threshold(settings.vad_sensitivity);
                let threshold = if speech.active {
                    (noise_floor * 1.4).max(base * 0.6)
                } else {
                    (noise_floor * 2.0).max(base * 0.9)
                };
                let is_speech = if let Some(ref detector) = silero {
                    detector.accept_streaming(&samples)
                } else {
                    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                    rms >= threshold && (peak / rms.max(1e-5) >= 2.0 || rms >= noise_floor * 2.0)
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
                    dock::set_state(&app, "recording", None);
                }
                if speech.active
                    && silence_confirmed
                    && silent_since.elapsed() >= Duration::from_millis(500)
                {
                    let audio = speech.take();
                    if audio.len() >= 6400 {
                        completed.push(audio);
                    }
                    frames.clear();
                }
                for audio in completed {
                    if !work_current(&state, token) {
                        break;
                    }
                    let result = process_utterance(&app, &state, &audio, token, true, false);
                    if let Err(error) = result {
                        report_error(&app, &state, token, &error);
                    }
                    last_awake_activity = Instant::now();
                }
                if !speech.active
                    && last_awake_activity.elapsed()
                        > Duration::from_secs(settings.handsfree_silence_timeout_sec as u64)
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
        let prepared = prepare_transcript(&format!("ASK {payload}"), true, &[], true);
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
}
