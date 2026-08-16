use std::sync::Arc;
use std::time::Duration;

use opendictate_core::audio::vad::{apply_vad, SileroVad};
use opendictate_core::stt::engine::{ModelKind, SttEngine};
use opendictate_core::stt::models;
use tauri::{AppHandle, Emitter};

use crate::db;
use crate::inject;
use crate::dock;
use crate::state::{AppState, HistoryEntry, TranscriptResult};

pub fn start(app: &AppHandle, state: &AppState, test: bool) -> Result<(), String> {
    if state.recorder.is_recording() {
        log::warn!("recording already in progress; cancelling stale recording");
        state.recorder.cancel().map_err(|e| e.to_string())?;
    }

    state.set_test_mode(test);
    let mic = state.settings.lock().map(|s| s.mic.clone()).unwrap_or(None);
    match mic {
        Some(name) if !name.is_empty() => state.recorder.start_with_name(&name),
        _ => state.recorder.start(),
    }
    .map_err(|e| e.to_string())?;

    dock::set_state(app, "listening", None);

    spawn_level_emitter(app, state);
    Ok(())
}

pub fn stop(app: &AppHandle, state: &AppState) -> Result<TranscriptResult, String> {
    if !state.recorder.is_recording() {
        return Err("no recording in progress".to_string());
    }

    let audio = state.recorder.stop().map_err(|e| e.to_string())?;
    let test = state.is_test_mode();

    dock::set_state(app, "transcribing", None);

    let speech = run_vad(&audio).map_err(|e| e.to_string())?;
    if !speech.has_speech {
        dock::set_state(app, "hidden", None);
        return Ok(TranscriptResult {
            text: String::new(),
            duration_ms: 0,
        });
    }

    let engine = load_engine(state)?;
    let text = engine
        .transcribe(&speech.trimmed_audio)
        .map_err(|e| e.to_string())?;
    let duration_ms = speech.speech_duration_ms;

    if test {
        dock::set_state(app, "hidden", None);
        return Ok(TranscriptResult {
            text,
            duration_ms,
        });
    }

    let _ = app.emit("transcript", serde_json::json!({ "text": text, "injected": true }));
    let _ = app.emit("overlay-state", serde_json::json!({ "state": "inserted", "message": null }));

    if !text.is_empty() {
        let entry = HistoryEntry {
            id: 0,
            text: text.clone(),
            created_at: db::now_timestamp(),
            duration_ms,
            source: "hotkey".to_string(),
        };
        if let Ok(conn) = state.db.lock() {
            if db::insert_history(&conn, &entry).is_ok() {
                let _ = app.emit("history-updated", serde_json::json!({}));
            }
        }

        if let Err(e) = inject::inject_text(app, &text) {
            dock::set_state(app, "error", Some(&format!("failed to paste: {e}")));
            return Ok(TranscriptResult { text, duration_ms });
        }
    }

    dock::set_state(app, "inserted", None);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1200));
        dock::set_state(&app, "hidden", None);
    });

    Ok(TranscriptResult { text, duration_ms })
}

pub fn cancel(app: &AppHandle, state: &AppState) -> Result<(), String> {
    if !state.recorder.is_recording() {
        return Ok(());
    }
    state.recorder.cancel().map_err(|e| e.to_string())?;
    state.set_test_mode(false);
    dock::set_state(app, "hidden", None);
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
) -> Result<opendictate_core::audio::vad::VadResult, opendictate_core::CoreError> {
    let vad_path = models::vad_model_path();
    let silero = if models::is_vad_ready() {
        SileroVad::new(&vad_path).ok()
    } else {
        None
    };
    Ok(apply_vad(audio, silero.as_ref()))
}

fn load_engine(state: &AppState) -> Result<SttEngine, String> {
    let model_id = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        if settings.stt_model.is_empty() {
            models::STT_MODEL_ID.to_string()
        } else {
            settings.stt_model.clone()
        }
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
    SttEngine::new(&dir, kind).map_err(|e| e.to_string())
}