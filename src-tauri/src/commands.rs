use tauri::{AppHandle, Emitter, Manager, State};

use opendictate_core::audio::capture::AudioRecorder;
use opendictate_core::stt::models;

use crate::db;
use crate::dictation;
use crate::state::{AppState, ModelsStatus, TranscriptResult};

#[tauri::command]
pub fn list_mics() -> Vec<opendictate_core::audio::MicDevice> {
    use opendictate_core::audio::MicDevice;

    let mut mics = vec![MicDevice {
        id: "default".to_string(),
        label: "System default".to_string(),
    }];

    // Prefer PulseAudio sources when the server is reachable: they cover every
    // mic the OS can access (built-in, USB, Bluetooth), which cpal/ALSA cannot.
    #[cfg(target_os = "linux")]
    if let Some(sources) = opendictate_core::audio::pulse::list_sources() {
        if !sources.is_empty() {
            for s in sources {
                mics.push(MicDevice {
                    id: format!("{}{}", opendictate_core::audio::pulse::PULSE_PREFIX, s.name),
                    label: s.description,
                });
            }
            return mics;
        }
    }

    for name in AudioRecorder::list_input_devices() {
        mics.push(MicDevice {
            id: name.clone(),
            label: name,
        });
    }
    mics
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
pub fn models_status(state: State<AppState>) -> ModelsStatus {
    ModelsStatus {
        stt_ready: models::is_stt_model_ready(),
        vad_ready: models::is_vad_ready(),
        caption_ready: models::is_caption_model_ready(),
        streaming_rtf_x100: state
            .streaming_rtf_x100
            .load(std::sync::atomic::Ordering::SeqCst),
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
        // VAD is an internal default, never user-selectable: install it
        // alongside any requested model so speech detection just works.
        let result = match result {
            Ok(()) if id != models::VAD_MODEL_ID => {
                models::ensure_model(models::VAD_MODEL_ID, &mut |file, received, total| {
                    let _ = app.emit(
                        "model-progress",
                        serde_json::json!({ "file": file, "received": received, "total": total }),
                    );
                })
            }
            other => other,
        };
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
pub fn warmup_model(
    _id: String,
    engine_key: String,
    app: AppHandle,
    _state: State<AppState>,
) -> Result<(), String> {
    let is_streaming = engine_key.contains("streaming");
    std::thread::Builder::new()
        .name("opendictate-warmup".to_string())
        .spawn(move || {
            let state = dictation::state_from_app(&app);
            if is_streaming {
                let _ = dictation::spawn_streaming(&app, &state);
            } else {
                let _ = dictation::load_engine(&state);
            }
        })
        .map_err(|e| format!("failed to start warm-up thread: {e}"))?;
    Ok(())
}

// These three must be `async` so Tauri runs them on the async runtime. A
// synchronous command body executes on the MAIN thread, and each of these
// blocks for a long time: `dictation::stop` waits on the inference worker's
// channel until transcription finishes, and start/cancel drive CoreAudio
// setup and teardown. Running that on the main thread beachballs the whole
// window until it returns. The hotkey path already hops to a worker thread
// (see hotkey::toggle_dictation); this is the same fix for the UI path.
#[tauri::command]
pub async fn start_recording(mode: String, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        dictation::start(&app, &state, mode == "test")
    })
    .await
    .map_err(|e| format!("start_recording worker failed: {e}"))?
}

#[tauri::command]
pub async fn stop_recording(app: AppHandle) -> Result<TranscriptResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        dictation::stop(&app, &state)
    })
    .await
    .map_err(|e| format!("stop_recording worker failed: {e}"))?
}

#[tauri::command]
pub async fn cancel_recording(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        dictation::cancel(&app, &state)
    })
    .await
    .map_err(|e| format!("cancel_recording worker failed: {e}"))?
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
    if let Some(vad_sensitivity) = settings.vad_sensitivity {
        if (0.0..=1.0).contains(&vad_sensitivity) {
            current.vad_sensitivity = vad_sensitivity;
        }
    }
    if let Some(continuous) = settings.continuous {
        current.continuous = continuous;
    }
    if let Some(autostart) = settings.autostart {
        current.autostart = autostart;
    }
    if let Some(spoken_punctuation) = settings.spoken_punctuation {
        current.spoken_punctuation = spoken_punctuation;
    }
    if let Some(audio_feedback) = settings.audio_feedback {
        current.audio_feedback = audio_feedback;
    }
    if let Some(audio_feedback_volume) = settings.audio_feedback_volume {
        if (0.0..=1.0).contains(&audio_feedback_volume) {
            current.audio_feedback_volume = audio_feedback_volume;
        }
    }
    let settings = current.clone();
    drop(current);

    if hotkey_changed {
        crate::hotkey::register(&app, &state, &settings.hotkey)?;
    }

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::save_settings(&conn, &settings).map_err(|e| e.to_string())?;
    // Autostart is best-effort: on platforms without support (or on fs
    // errors) the save itself must still succeed, otherwise every settings
    // write rejects and the UI appears broken.
    if let Err(e) = crate::autostart::set_enabled(&app, settings.autostart) {
        log::warn!("autostart update failed: {e}");
    }
    Ok(())
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
pub fn update_history(id: i64, text: String, state: State<AppState>) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("history text cannot be empty".to_string());
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::update_history(&conn, id, text).map_err(|e| e.to_string())
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
    let word = word.trim().to_string();
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
pub fn list_snippets(state: State<AppState>) -> Result<Vec<crate::state::SnippetEntry>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_snippets(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_snippet(
    trigger: String,
    text: String,
    state: State<AppState>,
) -> Result<(), String> {
    let trigger = trigger.trim().to_string();
    let text = text.trim().to_string();
    if trigger.is_empty() || text.is_empty() {
        return Err("snippet trigger and text are required".to_string());
    }
    if !opendictate_core::text::is_single_word(&trigger) {
        return Err("snippet trigger must be a single word".to_string());
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if !db::add_snippet(&conn, &trigger, &text).map_err(|e| e.to_string())? {
        return Err(format!("a snippet named \"{trigger}\" already exists"));
    }
    Ok(())
}

#[tauri::command]
pub fn update_snippet(
    id: i64,
    trigger: String,
    text: String,
    state: State<AppState>,
) -> Result<(), String> {
    let trigger = trigger.trim().to_string();
    let text = text.trim().to_string();
    if trigger.is_empty() || text.is_empty() {
        return Err("snippet trigger and text are required".to_string());
    }
    if !opendictate_core::text::is_single_word(&trigger) {
        return Err("snippet trigger must be a single word".to_string());
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if !db::update_snippet(&conn, id, &trigger, &text).map_err(|e| e.to_string())? {
        return Err(format!("a snippet named \"{trigger}\" already exists"));
    }
    Ok(())
}

#[tauri::command]
pub fn remove_snippet(id: i64, state: State<AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::remove_snippet(&conn, id).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct SnippetImport {
    trigger: String,
    text: String,
}

#[tauri::command]
pub fn import_snippets(contents: String, state: State<AppState>) -> Result<usize, String> {
    let entries: Vec<SnippetImport> = serde_json::from_str(&contents)
        .map_err(|e| format!("invalid snippets JSON: {e}"))?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut imported = 0;
    for entry in entries {
        let trigger = entry.trigger.trim().to_string();
        let text = entry.text.trim().to_string();
        if trigger.is_empty() || text.is_empty() {
            continue;
        }
        if !opendictate_core::text::is_single_word(&trigger) {
            continue;
        }
        if db::add_snippet(&conn, &trigger, &text).map_err(|e| e.to_string())? {
            imported += 1;
        }
    }
    Ok(imported)
}

#[tauri::command]
pub fn export_snippets(app: AppHandle, state: State<AppState>) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let entries = db::list_snippets(&conn).map_err(|e| e.to_string())?;
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let exports_dir = data_dir.join("exports");
    std::fs::create_dir_all(&exports_dir).map_err(|e| e.to_string())?;
    let path = exports_dir.join(format!("snippets-{}.json", db::now_timestamp()));
    let contents = serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?;
    std::fs::write(&path, contents).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
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
pub fn undo_last_insert(state: State<AppState>) -> Result<(), String> {
    let mut last = state.last_inserted.lock().map_err(|e| e.to_string())?;
    if last.take().is_none() {
        return Err("nothing to undo".to_string());
    }
    crate::inject::undo_last_insert()
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

#[tauri::command]
pub fn export_history(
    kind: String,
    app: AppHandle,
    state: State<AppState>,
) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let entries = db::get_history(&conn).map_err(|e| e.to_string())?;

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let exports_dir = data_dir.join("exports");
    std::fs::create_dir_all(&exports_dir).map_err(|e| e.to_string())?;

    let ext = match kind.as_str() {
        "json" => "json",
        "csv" => "csv",
        other => return Err(format!("unsupported export format '{other}'")),
    };
    let path = exports_dir.join(format!("history-{}.{ext}", db::now_timestamp()));

    let contents = match ext {
        "json" => serde_json::to_string_pretty(&entries)
            .map_err(|e| e.to_string())?,
        _ => csv_contents(&entries),
    };

    std::fs::write(&path, contents).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

fn csv_contents(entries: &[crate::state::HistoryEntry]) -> String {
    let mut out = String::from("id,text,created_at,duration_ms,source\n");
    for e in entries {
        let text = e.text.replace('"', "\"\"");
        out.push_str(&format!(
            "{},{},\"{}\",{},\"{}\"\n",
            e.id,
            e.created_at,
            text.replace('\n', " "),
            e.duration_ms,
            e.source
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::HistoryEntry;

    #[test]
    fn csv_escapes_quotes_and_newlines() {
        let entries = vec![
            HistoryEntry {
                id: 1,
                text: "say \"hi\"".to_string(),
                created_at: "1724000000".to_string(),
                duration_ms: 1200,
                source: "hotkey".to_string(),
            },
            HistoryEntry {
                id: 2,
                text: "line1\nline2, with comma".to_string(),
                created_at: "1724000001".to_string(),
                duration_ms: 800,
                source: "continuous".to_string(),
            },
        ];
        let csv = csv_contents(&entries);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "id,text,created_at,duration_ms,source");
        assert!(lines[1].contains("\"say \"\"hi\"\"\""));
        assert!(lines[2].contains("line1 line2, with comma"));
        assert!(lines[2].contains("continuous"));
    }
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
