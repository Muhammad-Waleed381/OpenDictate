use tauri::{AppHandle, Emitter, State};

use opendictate_core::audio::capture::AudioRecorder;
use opendictate_core::stt::models;

use crate::db;
use crate::dictation;
use crate::state::{AppState, ModelsStatus, TranscriptResult};

#[tauri::command]
pub fn list_mics() -> Vec<String> {
    AudioRecorder::list_input_devices()
}

#[tauri::command]
pub fn get_mic(state: State<AppState>) -> Option<String> {
    state.settings.lock().ok()?.mic.clone()
}

#[tauri::command]
pub fn set_mic(name: String, state: State<AppState>) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.mic = Some(name);
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::save_settings(&conn, &settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn models_status() -> ModelsStatus {
    ModelsStatus {
        stt_ready: models::is_stt_model_ready(),
        vad_ready: models::is_vad_ready(),
    }
}

#[tauri::command]
pub fn models_catalog() -> Vec<models::ModelInfo> {
    models::catalog()
}

#[tauri::command]
pub fn ensure_model(id: String, app: AppHandle) -> Result<(), String> {
    std::thread::spawn(move || {
        let result = models::ensure_model(&id, &mut |file, received, total| {
            let _ = app.emit(
                "model-progress",
                serde_json::json!({ "file": file, "received": received, "total": total }),
            );
        });
        match result {
            Ok(()) => {
                let _ = app.emit("models-ready", serde_json::json!({}));
            }
            Err(e) => {
                let _ = app.emit(
                    "overlay-state",
                    serde_json::json!({ "state": "error", "message": format!("model download failed: {e}") }),
                );
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn remove_model(id: String) -> Result<(), String> {
    models::remove_model(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn start_recording(mode: String, app: AppHandle, state: State<AppState>) -> Result<(), String> {
    dictation::start(&app, &state, mode == "test")
}

#[tauri::command]
pub fn stop_recording(app: AppHandle, state: State<AppState>) -> Result<TranscriptResult, String> {
    dictation::stop(&app, &state)
}

#[tauri::command]
pub fn cancel_recording(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    dictation::cancel(&app, &state)
}

#[tauri::command]
pub fn is_recording(state: State<AppState>) -> bool {
    state.recorder.is_recording()
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<crate::state::Settings, String> {
    state
        .settings
        .lock()
        .map(|s| s.clone())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_settings(
    settings: crate::state::SettingsPatch,
    app: AppHandle,
    state: State<AppState>,
) -> Result<(), String> {
    let mut current = state.settings.lock().map_err(|e| e.to_string())?;
    if let Some(hotkey) = &settings.hotkey {
        if hotkey.trim().is_empty() {
            return Err("hotkey cannot be empty".to_string());
        }
    }
    let hotkey_changed = settings
        .hotkey
        .as_ref()
        .is_some_and(|h| h != &current.hotkey);
    if let Some(hotkey) = &settings.hotkey {
        current.hotkey = hotkey.clone();
    }
    if let Some(engine) = &settings.engine {
        if !engine.is_empty() {
            current.engine = engine.clone();
        }
    }
    if let Some(language) = &settings.language {
        if !language.is_empty() {
            current.language = language.clone();
        }
    }
    if let Some(stt_model) = &settings.stt_model {
        if !stt_model.is_empty() {
            current.stt_model = stt_model.clone();
        }
    }
    if let Some(insert_mode) = &settings.insert_mode {
        if matches!(insert_mode.as_str(), "auto" | "type" | "clipboard") {
            current.insert_mode = insert_mode.clone();
        }
    }
    let settings = current.clone();
    drop(current);

    if hotkey_changed {
        crate::hotkey::register(&app, &state, &settings.hotkey)?;
    }

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::save_settings(&conn, &settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn complete_onboarding(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    {
        let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.onboarded = true;
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::save_settings(&conn, &settings).map_err(|e| e.to_string())?;
    }
    let _ = app.emit("settings-changed", serde_json::json!({}));
    Ok(())
}

#[tauri::command]
pub fn get_history(state: State<AppState>) -> Result<Vec<crate::state::HistoryEntry>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::get_history(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_history(id: i64, state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::delete_history(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_history(state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::clear_history(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_dictionary(state: State<AppState>) -> Result<Vec<crate::state::DictEntry>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::get_dictionary(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_dictionary_word(word: String, state: State<AppState>) -> Result<(), String> {
    let word = word.trim().to_lowercase();
    if word.is_empty() {
        return Err("word cannot be empty".to_string());
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::add_dictionary_word(&conn, &word).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_dictionary_word(word: String, state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::remove_dictionary_word(&conn, &word).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn paste_clipboard(text: String, app: AppHandle) -> Result<(), String> {
    crate::inject::inject_text(&app, &text, "auto")
}

#[tauri::command]
pub fn copy_text(text: String, app: AppHandle) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("failed to write clipboard: {e}"))
}