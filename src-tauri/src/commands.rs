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
    if let Some(heatmap_color) = &settings.heatmap_color {
        let valid = heatmap_color.starts_with('#')
            && matches!(heatmap_color.len(), 4 | 7 | 9)
            && heatmap_color[1..].chars().all(|c| c.is_ascii_hexdigit());
        if valid {
            current.heatmap_color = heatmap_color.clone();
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

#[tauri::command]
pub fn word_stats(state: State<AppState>) -> Result<crate::state::WordStats, String> {
    use crate::state::{DayWords, WordStats};
    use std::collections::HashMap;

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let rows = db::word_stats(&conn).map_err(|e| e.to_string())?;
    let total_sessions: i64 = conn
        .query_row("SELECT COALESCE(SUM(sessions), 0) FROM daily_stats", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;

    let mut by_day: HashMap<String, u64> = HashMap::new();
    let mut total_words: u64 = 0;
    let mut best_day: Option<String> = None;
    let mut best_words: u64 = 0;
    for (day, words) in &rows {
        let words = *words as u64;
        total_words += words;
        by_day.insert(day.clone(), words);
        if words > best_words {
            best_words = words;
            best_day = Some(day.clone());
        }
    }
    let total_sessions = total_sessions as u64;

    let mut streak_days: u64 = 0;
    let today = chrono_day_offset(0);
    if let Some(first) = by_day.get(&today) {
        if *first > 0 {
            streak_days = 1;
            let mut offset = 1;
            while by_day.get(&chrono_day_offset(-(offset as i64))).is_some_and(|w| *w > 0) {
                streak_days += 1;
                offset += 1;
            }
        }
    } else {
        let mut offset = 1;
        while by_day.get(&chrono_day_offset(-(offset as i64))).is_some_and(|w| *w > 0) {
            streak_days += 1;
            offset += 1;
        }
    }

    Ok(WordStats {
        daily: by_day
            .into_iter()
            .map(|(day, words)| DayWords { day, words })
            .collect(),
        total_words,
        total_sessions,
        streak_days,
        best_day,
        best_words,
    })
}

#[tauri::command]
pub fn reset_word_stats(state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::reset_word_stats(&conn).map_err(|e| e.to_string())
}

fn chrono_day_offset(days: i64) -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let z = secs.div_euclid(86_400) + days;
    let (y, m, d) = civil_from_days(z);
    format!("{y:04}-{m:02}-{d:02}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}