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
use crate::state::{AppState, HistoryEntry, StreamingPipe, TranscriptResult};

pub fn start(app: &AppHandle, state: &AppState, test: bool) -> Result<(), String> {
    if state.recorder.is_recording() {
        log::warn!("recording already in progress; cancelling stale recording");
        state.recorder.cancel().map_err(|e| e.to_string())?;
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

    let mic = state.settings.lock().map(|s| s.mic.clone()).unwrap_or(None);
    match mic {
        Some(name) if !name.is_empty() => state.recorder.start_with_name(&name),
        _ => state.recorder.start(),
    }
    .map_err(|e| e.to_string())?;

    dock::set_state(app, "listening", None);

    if streaming {
        if let Err(e) = spawn_streaming(app, state) {
            let _ = state.recorder.cancel();
            dock::set_state(app, "hidden", None);
            return Err(e);
        }
    } else if !test && is_continuous_enabled(state) {
        state.set_continuous(true);
        spawn_continuous_loop(app, state);
    }

    spawn_level_emitter(app, state);
    Ok(())
}

pub fn stop(app: &AppHandle, state: &AppState) -> Result<TranscriptResult, String> {
    state.set_continuous(false);
    if state.is_streaming_active() {
        return stop_streaming(app, state);
    }
    if !state.recorder.is_recording() {
        return Err("no recording in progress".to_string());
    }

    let audio = state.recorder.stop().map_err(|e| e.to_string())?;
    let test = state.is_test_mode();
    dock::set_state(app, "transcribing", None);

    process_utterance(app, state, &audio, test, false)
}

pub fn cancel(app: &AppHandle, state: &AppState) -> Result<(), String> {
    state.set_continuous(false);
    state.set_streaming(false);
    if !state.recorder.is_recording() {
        return Ok(());
    }
    state.recorder.cancel().map_err(|e| e.to_string())?;
    state.set_test_mode(false);
    *state
        .stream
        .lock()
        .map_err(|e| e.to_string())? = None;
    dock::set_caption(app, None);
    dock::set_state(app, "hidden", None);
    Ok(())
}

fn is_continuous_enabled(state: &AppState) -> bool {
    state
        .settings
        .lock()
        .map(|s| s.continuous)
        .unwrap_or(false)
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
                    let _ = process_utterance(&app, &state_from_app(&app), &audio, false, true);

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
    dock::set_state(app, "listening", None);
    Ok(())
}

fn state_from_app(app: &AppHandle) -> tauri::State<'_, AppState> {
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
fn spawn_streaming(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let model_id = selected_model_id(state);
    let dir = models::model_dir_for(&model_id);
    let recognizer =
        StreamingRecognizer::new(&dir).map_err(|e| e.to_string())?;
    let hotwords = dictionary_hotwords(state);
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
                std::thread::sleep(Duration::from_millis(60));
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
            }

            if pipe.recognizer.is_ready(&pipe.session) {
                let text = pipe.recognizer.result(&pipe.session);
                if !text.is_empty() {
                    let _ = app.emit(
                        "partial",
                        serde_json::json!({ "text": text, "streaming": true }),
                    );
                    dock::set_caption(&app, Some(&text));
                }
            }

            if pipe.recognizer.is_endpoint(&pipe.session) {
                let text = pipe.recognizer.result(&pipe.session);
                let duration_ms = pipe.session.started_at.elapsed().as_millis() as u64;
                if !text.is_empty() {
                    let _ = commit_text(
                        &app,
                        &state_from_app(&app),
                        &text,
                        duration_ms,
                        true,
                        false,
                    );
                }
                pipe.recognizer.reset(&mut pipe.session);
            }

            drop(pipe_guard);
            std::thread::sleep(Duration::from_millis(60));
        }
        stream_active.store(false, Ordering::SeqCst);
        dock::set_caption(&app, None);
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
    let text = pipe.recognizer.result(&pipe.session);
    let duration_ms = pipe.session.started_at.elapsed().as_millis() as u64;
    let result = TranscriptResult {
        text: text.clone(),
        duration_ms,
    };

    drop(pipe_guard);
    *state.stream.lock().map_err(|e| e.to_string())? = None;
    dock::set_caption(app, None);

    if !test && !text.is_empty() {
        let _ = commit_text(app, state, &text, duration_ms, true, true);
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
    let speech = run_vad(audio, sensitivity(state)).map_err(|e| e.to_string())?;
    if !speech.has_speech {
        dock::set_state(app, "hidden", None);
        return Ok(TranscriptResult {
            text: String::new(),
            duration_ms: 0,
        });
    }

    let engine = load_engine(state)?;
    let hotwords = dictionary_hotwords(state);
    let raw = engine
        .transcribe(&speech.trimmed_audio, hotwords.as_deref())
        .map_err(|e| e.to_string())?;
    let text = inject::clean_text(&raw);
    let duration_ms = speech.speech_duration_ms;

    if test {
        dock::set_state(app, "hidden", None);
        return Ok(TranscriptResult {
            text,
            duration_ms,
        });
    }

    commit_text(app, state, &text, duration_ms, continuous, !continuous)?;

    Ok(TranscriptResult { text, duration_ms })
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

    if !text.is_empty() {
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
            return Err(e);
        }
    }

    if continuous {
        if finish {
            dock::set_state(app, "inserted", None);
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(1200));
                dock::set_state(&app, "hidden", None);
            });
        } else {
            dock::set_state(app, "listening", None);
        }
        return Ok(());
    }

    dock::set_state(app, "inserted", None);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1200));
        dock::set_state(&app, "hidden", None);
    });

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
    audio: &[f32],
    sensitivity: f32,
) -> Result<opendictate_core::audio::vad::VadResult, opendictate_core::CoreError> {
    let vad_path = models::vad_model_path();
    let silero = if models::is_vad_ready() {
        SileroVad::with_threshold(&vad_path, sensitivity_to_silero_threshold(sensitivity)).ok()
    } else {
        None
    };
    Ok(apply_vad(audio, silero.as_ref(), sensitivity))
}

fn dictionary_hotwords(state: &AppState) -> Option<String> {
    let words = state
        .db
        .lock()
        .ok()
        .and_then(|conn| db::get_dictionary(&conn).ok())?;
    let joined: Vec<String> = words
        .iter()
        .map(|e| format!("{} {}", e.word.replace(' ', "_"), HOTWORD_SCORE))
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

fn load_engine(state: &AppState) -> Result<SttEngine, String> {
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
    SttEngine::new(&dir, kind, Some(language)).map_err(|e| e.to_string())
}