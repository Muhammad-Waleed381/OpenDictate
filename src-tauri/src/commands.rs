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
pub async fn set_mic(name: String, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _settings_update = state
            .sessions
            .settings_update
            .lock()
            .map_err(|e| e.to_string())?;
        let mut settings = state.settings.lock().map_err(|e| e.to_string())?.clone();
        settings.mic = if name.is_empty() || name == "default" {
            None
        } else {
            Some(name)
        };
        apply_settings(&app, &state, settings)
    })
    .await
    .map_err(|e| format!("microphone update worker failed: {e}"))?
}

#[tauri::command]
pub fn models_status(state: State<AppState>) -> ModelsStatus {
    ModelsStatus {
        stt_ready: state
            .settings
            .lock()
            .map(|s| {
                models::is_model_installed(if s.stt_model.is_empty() {
                    models::STT_MODEL_ID
                } else {
                    &s.stt_model
                })
            })
            .unwrap_or(false),
        vad_ready: models::is_vad_ready(),
        caption_ready: models::is_caption_model_ready(),
        kws_ready: models::is_kws_ready(),
        streaming_rtf_x100: state
            .streaming_rtf_x100
            .load(std::sync::atomic::Ordering::SeqCst),
        gpu_mode: current_gpu_mode(&state),
        gpu_active: state.gpu_active.load(std::sync::atomic::Ordering::SeqCst),
    }
}

#[tauri::command]
pub fn models_catalog() -> Vec<models::ModelInfo> {
    models::catalog()
}

#[tauri::command]
pub async fn ensure_model(
    id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let downloads = Arc::clone(&state.active_downloads);
    {
        let mut active = downloads.lock().map_err(|e| e.to_string())?;
        if active.contains_key(&id) {
            return Err(format!("a download for '{id}' is already in progress"));
        }
        active.insert(id.clone(), Arc::clone(&cancel));
    }
    let worker_app = app.clone();
    let worker_id = id.clone();
    let worker_cancel = Arc::clone(&cancel);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let cancelled = || worker_cancel.load(Ordering::SeqCst);
        let mut progress = |file: &str, received, total| {
            let _ = worker_app.emit(
                "model-progress",
                serde_json::json!({ "file": file, "received": received, "total": total }),
            );
        };
        models::ensure_model_with_cancel(&worker_id, &mut progress, &cancelled)
            .map_err(|e| e.to_string())?;
        if cancelled() {
            return Err("model download cancelled".to_string());
        }
        if worker_id != models::VAD_MODEL_ID {
            models::ensure_model_with_cancel(models::VAD_MODEL_ID, &mut progress, &cancelled)
                .map_err(|e| e.to_string())?;
        }
        if cancelled() {
            return Err("model download cancelled".to_string());
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("model download worker failed: {e}"))
    .and_then(|result| result);
    // Keep ownership through worker exit (including panic/join failure). A
    // cancelled operation cannot delete another operation's files or slot.
    {
        let mut active = downloads.lock().map_err(|e| e.to_string())?;
        crate::session::release_download_owner(&mut active, &id, &cancel);
    }
    match &result {
        Ok(()) => {
            let _ = app.emit("models-ready", serde_json::json!({ "file": id }));
        }
        Err(error) if cancel.load(Ordering::SeqCst) => {
            let _ = app.emit("model-cancelled", serde_json::json!({ "file": id }));
            log::info!("model download cancelled: {id}: {error}");
        }
        Err(error) => {
            let _ = app.emit(
                "model-error",
                serde_json::json!({ "file": id, "error": error }),
            );
        }
    }
    result
}

#[tauri::command]
pub fn cancel_model_download(id: String, state: State<AppState>) -> Result<(), String> {
    let active = state.active_downloads.lock().map_err(|e| e.to_string())?;
    crate::session::request_download_cancel(&active, &id);
    Ok(())
}

#[tauri::command]
pub fn play_test_sound(
    event: String,
    volume: Option<f32>,
    state: State<AppState>,
) -> Result<(), String> {
    let vol = volume.unwrap_or_else(|| {
        state
            .settings
            .lock()
            .map(|s| s.audio_feedback_volume)
            .unwrap_or(0.5)
    });
    let evt = match event.to_lowercase().as_str() {
        "listening" | "start" => crate::audio::SoundEvent::Listening,
        "inserted" | "insert" => crate::audio::SoundEvent::Inserted,
        "error" => crate::audio::SoundEvent::Error,
        other => return Err(format!("unknown sound event '{other}'")),
    };
    crate::audio::play_event(vol, evt);
    Ok(())
}

#[tauri::command]
pub async fn reset_settings(app: AppHandle) -> Result<crate::state::Settings, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _settings_update = state
            .sessions
            .settings_update
            .lock()
            .map_err(|e| e.to_string())?;
        let settings = crate::state::Settings::default();
        apply_settings(&app, &state, settings.clone())?;
        Ok(settings)
    })
    .await
    .map_err(|e| format!("reset settings worker failed: {e}"))?
}

#[tauri::command]
pub async fn remove_model(id: String, app: AppHandle) -> Result<(), String> {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let downloads = Arc::clone(&app.state::<AppState>().active_downloads);
    let owner = Arc::new(AtomicBool::new(false));
    {
        let mut active = downloads.lock().map_err(|e| e.to_string())?;
        if active.contains_key(&id) || (id == models::VAD_MODEL_ID && !active.is_empty()) {
            return Err(format!("model '{id}' has an active installation operation"));
        }
        // Reserve this path while removing it, without holding the global
        // registry mutex across filesystem work or a core path lock.
        active.insert(id.clone(), Arc::clone(&owner));
    }
    let worker_id = id.clone();
    let worker_cancel = Arc::clone(&owner);
    let result = tauri::async_runtime::spawn_blocking(move || {
        // The removal waits for any in-flight operation on the same model.
        // A cancellation must abort that wait instead of blocking forever.
        let cancelled = || worker_cancel.load(Ordering::SeqCst);
        models::remove_model_with_cancel(&worker_id, &cancelled).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("remove model worker failed: {e}"))
    .and_then(|result| result);
    let mut active = downloads.lock().map_err(|e| e.to_string())?;
    crate::session::release_download_owner(&mut active, &id, &owner);
    result
}

#[tauri::command]
pub fn warmup_model(_engine_key: String, app: AppHandle) -> Result<(), String> {
    std::thread::Builder::new()
        .name("opendictate-warmup".to_string())
        .spawn(move || {
            let state = dictation::state_from_app(&app);
            dictation::warmup_current_model(&state);
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
    state
        .user_dictation_active
        .load(std::sync::atomic::Ordering::SeqCst)
}

fn current_gpu_mode(state: &State<AppState>) -> String {
    state
        .settings
        .lock()
        .map(|s| s.gpu.clone())
        .unwrap_or_else(|_| "off".to_string())
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
pub async fn set_settings(
    settings: crate::state::SettingsPatch,
    app: AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _settings_update = state
            .sessions
            .settings_update
            .lock()
            .map_err(|e| e.to_string())?;
        let current = state.settings.lock().map_err(|e| e.to_string())?.clone();
        let next = patched_settings(current, settings)?;
        apply_settings(&app, &state, next)
    })
    .await
    .map_err(|e| format!("settings worker failed: {e}"))?
}

fn patched_settings(
    mut current: crate::state::Settings,
    settings: crate::state::SettingsPatch,
) -> Result<crate::state::Settings, String> {
    if let Some(hotkey) = settings.hotkey {
        if hotkey.trim().is_empty() {
            return Err("hotkey cannot be empty".into());
        }
        current.hotkey = hotkey;
    }
    if let Some(gpu) = settings.gpu {
        let gpu = gpu.trim().to_lowercase();
        if !matches!(gpu.as_str(), "" | "off" | "auto" | "cuda" | "coreml") {
            return Err("gpu must be one of: off, auto, cuda, coreml".into());
        }
        if !gpu.is_empty() {
            current.gpu = gpu;
        }
    }
    if let Some(engine) = settings.engine {
        if !engine.is_empty() {
            current.engine = engine;
        }
    }
    if let Some(language) = settings.language {
        if !language.is_empty() {
            current.language = language;
        }
    }
    if let Some(model) = settings.stt_model {
        if !model.is_empty() {
            current.stt_model = model;
        }
    }
    if let Some(mode) = settings.insert_mode {
        if matches!(mode.as_str(), "auto" | "type" | "clipboard") {
            current.insert_mode = mode;
        }
    }
    if let Some(target) = settings.input_target {
        if !matches!(target.as_str(), "text" | "terminal") {
            return Err("input target must be text or terminal".into());
        }
        current.input_target = target;
    }
    if let Some(color) = settings.heatmap_color {
        if color.starts_with('#')
            && matches!(color.len(), 4 | 7 | 9)
            && color[1..].chars().all(|c| c.is_ascii_hexdigit())
        {
            current.heatmap_color = color;
        }
    }
    if let Some(value) = settings.vad_sensitivity {
        if (0.0..=1.0).contains(&value) {
            current.vad_sensitivity = value;
        }
    }
    if let Some(value) = settings.continuous {
        current.continuous = value;
    }
    if let Some(value) = settings.hold_to_talk {
        current.hold_to_talk = value;
    }
    if let Some(value) = settings.autostart {
        current.autostart = value;
    }
    if let Some(value) = settings.spoken_punctuation {
        current.spoken_punctuation = value;
    }
    if let Some(value) = settings.audio_feedback {
        current.audio_feedback = value;
    }
    if let Some(value) = settings.audio_feedback_volume {
        if (0.0..=1.0).contains(&value) {
            current.audio_feedback_volume = value;
        }
    }
    if let Some(value) = settings.handsfree_mode {
        current.handsfree_mode = value;
    }
    if let Some(words) = settings.wake_words {
        if !words.trim().is_empty() {
            current.wake_words = words.trim().to_string();
        }
    }
    if let Some(timeout) = settings.handsfree_silence_timeout_sec {
        current.handsfree_silence_timeout_sec = timeout.clamp(5, 300);
    }
    if let Some(value) = settings.voice_actions_enabled {
        current.voice_actions_enabled = value;
    }
    if let Some(provider) = settings.polish_provider {
        if provider == "local_slm" {
            return Err("Local SLM polish is not available yet".into());
        }
        if !matches!(provider.as_str(), "off" | "groq") {
            return Err("unsupported polish provider".into());
        }
        current.polish_provider = provider;
    }
    if let Some(mode) = settings.polish_mode {
        if matches!(mode.as_str(), "clean" | "bullets") {
            current.polish_mode = mode;
        }
    }
    if let Some(key) = settings.groq_api_key {
        current.groq_api_key = if key.trim().is_empty() {
            None
        } else {
            Some(key.trim().to_string())
        };
    }
    if let Some(model) = settings.groq_model {
        if !model.trim().is_empty() {
            current.groq_model = Some(model.trim().to_string());
        }
    }
    if let Some(position) = settings.dock_position {
        if matches!(
            position.as_str(),
            "bottom_right" | "bottom_center" | "bottom_left" | "top_right" | "top_left"
        ) {
            current.dock_position = position;
        }
    }
    Ok(current)
}

/// All preference entry points use the same runtime transition and rollback.
/// Locks never span settings reads plus KWS/engine-cache access.
fn apply_settings(
    app: &AppHandle,
    state: &AppState,
    next: crate::state::Settings,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let _transition = state
        .sessions
        .transition
        .lock()
        .map_err(|e| e.to_string())?;
    let previous = state.settings.lock().map_err(|e| e.to_string())?.clone();
    if next.polish_provider == "local_slm" {
        return Err("Local SLM polish is not available yet".into());
    }
    let mic_changed = previous.mic != next.mic;
    if mic_changed && state.user_dictation_active.load(Ordering::SeqCst) {
        return Err("stop the active recording before changing microphones".into());
    }
    let handsfree_running = state.handsfree_active.load(Ordering::SeqCst);
    let restart_handsfree = handsfree_running
        && (mic_changed
            || previous.wake_words != next.wake_words
            || previous.vad_sensitivity != next.vad_sensitivity
            || previous.handsfree_silence_timeout_sec != next.handsfree_silence_timeout_sec);
    let hotkey_changed = previous.hotkey != next.hotkey;
    let autostart_changed = previous.autostart != next.autostart;
    *state.settings.lock().map_err(|e| e.to_string())? = next.clone();
    let result = (|| {
        if hotkey_changed {
            crate::hotkey::register(app, state, &next.hotkey)?;
        }
        if autostart_changed {
            crate::autostart::set_enabled(app, next.autostart)?;
        }
        if handsfree_running && (!next.handsfree_mode || restart_handsfree) {
            dictation::stop_handsfree_locked(app, state)?;
        }
        if next.handsfree_mode && !state.handsfree_active.load(Ordering::SeqCst) {
            dictation::start_handsfree_locked(app, state)?;
        }
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::save_settings(&conn, &next).map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        let mut rollback_errors = Vec::new();
        if state.handsfree_active.load(Ordering::SeqCst)
            && (!handsfree_running || restart_handsfree)
        {
            if let Err(e) = dictation::stop_handsfree_locked(app, state) {
                rollback_errors.push(e);
            }
        }
        *state.settings.lock().map_err(|e| e.to_string())? = previous.clone();
        if hotkey_changed {
            if let Err(e) = crate::hotkey::register(app, state, &previous.hotkey) {
                rollback_errors.push(e);
            }
        }
        if autostart_changed {
            if let Err(e) = crate::autostart::set_enabled(app, previous.autostart) {
                rollback_errors.push(e);
            }
        }
        if handsfree_running && !state.handsfree_active.load(Ordering::SeqCst) {
            if let Err(e) = dictation::start_handsfree_locked(app, state) {
                rollback_errors.push(e);
            }
        }
        let _ = app.emit("settings-changed", serde_json::json!({}));
        return Err(if rollback_errors.is_empty() {
            error
        } else {
            format!("{error}; rollback errors: {}", rollback_errors.join("; "))
        });
    }
    if previous.gpu != next.gpu {
        *state.stt_engine.lock().map_err(|e| e.to_string())? = None;
        *state.streaming_engine.lock().map_err(|e| e.to_string())? = None;
        state.gpu_active.store(false, Ordering::SeqCst);
    }
    if previous.wake_words != next.wake_words && !next.handsfree_mode {
        *state.kws_engine.lock().map_err(|e| e.to_string())? = None;
    }
    if previous.vad_sensitivity != next.vad_sensitivity {
        *state.vad.lock().map_err(|e| e.to_string())? = None;
    }
    if previous.dock_position != next.dock_position {
        let _ = app.emit("dock-position", next.dock_position.clone());
        crate::dock::reposition(app);
    }
    let _ = app.emit("settings-changed", serde_json::json!({}));
    Ok(())
}

#[tauri::command]
pub async fn toggle_handsfree(enabled: bool, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _settings_update = state
            .sessions
            .settings_update
            .lock()
            .map_err(|e| e.to_string())?;
        let mut next = state.settings.lock().map_err(|e| e.to_string())?.clone();
        next.handsfree_mode = enabled;
        apply_settings(&app, &state, next)
    })
    .await
    .map_err(|e| format!("handsfree settings worker failed: {e}"))?
}

#[cfg(test)]
mod settings_session_tests {
    use super::*;

    #[test]
    fn unavailable_polish_provider_rejects_entire_patch() {
        let original = crate::state::Settings::default();
        let patch = crate::state::SettingsPatch {
            handsfree_mode: Some(true),
            polish_provider: Some("local_slm".into()),
            ..Default::default()
        };
        assert!(patched_settings(original.clone(), patch).is_err());
        assert!(!original.handsfree_mode);
    }

    #[test]
    fn input_target_defaults_to_text_and_accepts_only_supported_profiles() {
        let defaults = crate::state::Settings::default();
        assert_eq!(defaults.input_target, "text");
        let terminal = patched_settings(
            defaults.clone(),
            crate::state::SettingsPatch {
                input_target: Some("terminal".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(terminal.input_target, "terminal");
        assert!(patched_settings(
            defaults,
            crate::state::SettingsPatch {
                input_target: Some("unknown".into()),
                ..Default::default()
            }
        )
        .is_err());
    }

    #[test]
    fn patch_keeps_unrelated_runtime_preferences() {
        let original = crate::state::Settings {
            mic: Some("USB".into()),
            handsfree_mode: true,
            ..Default::default()
        };
        let patched = patched_settings(
            original,
            crate::state::SettingsPatch {
                audio_feedback: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(patched.mic.as_deref(), Some("USB"));
        assert!(patched.handsfree_mode && patched.audio_feedback);
    }
}

#[tauri::command]
pub async fn test_groq_api_key(api_key: String, model: Option<String>) -> Result<String, String> {
    // Network request: run on a blocking thread so the main thread (and the
    // whole UI) is not frozen for the duration of the API round-trip.
    tauri::async_runtime::spawn_blocking(move || {
        let config = opendictate_core::text::PolishConfig {
            provider: opendictate_core::text::PolishProvider::Groq,
            mode: opendictate_core::text::PolishMode::Clean,
            groq_api_key: Some(api_key),
            groq_model: model,
        };
        opendictate_core::text::polish_text("um hello world this is a test like you know", &config)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("api key test task failed: {e}"))?
}

#[tauri::command]
pub async fn complete_onboarding(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _update = state
            .sessions
            .settings_update
            .lock()
            .map_err(|e| e.to_string())?;
        let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
        let mut next = settings.clone();
        next.onboarded = true;
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::save_settings(&conn, &next).map_err(|e| e.to_string())?;
        *settings = next;
        let _ = app.emit("settings-changed", serde_json::json!({}));
        Ok(())
    })
    .await
    .map_err(|e| format!("onboarding completion failed: {e}"))?
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
pub fn add_snippet(trigger: String, text: String, state: State<AppState>) -> Result<(), String> {
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
    let entries: Vec<SnippetImport> =
        serde_json::from_str(&contents).map_err(|e| format!("invalid snippets JSON: {e}"))?;
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
pub async fn paste_clipboard(text: String, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || insert_from_ui(&app, &text))
        .await
        .map_err(|e| format!("insertion task failed: {e}"))?
}

fn insert_from_ui(app: &AppHandle, text: &str) -> Result<(), String> {
    crate::inject::with_external_focus(app, || crate::inject::inject_text(app, text, "auto"))?;
    let state = app.state::<AppState>();
    *state.last_inserted.lock().map_err(|e| e.to_string())? = Some(text.to_owned());
    Ok(())
}

#[tauri::command]
pub async fn test_snippet(text: String, app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        // Read before insertion overwrites the clipboard, exactly like a
        // spoken snippet. Copy remains a literal-template operation.
        let clipboard = app.clipboard().read_text().ok();
        let expanded = opendictate_core::text::expand_snippet_template(&text, clipboard.as_deref());
        insert_from_ui(&app, &expanded)
    })
    .await
    .map_err(|e| format!("snippet test task failed: {e}"))?
}

#[tauri::command]
pub fn copy_text(text: String, app: AppHandle) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .write_text(text)
        .map_err(|e| format!("failed to write clipboard: {e}"))
}

#[tauri::command]
pub async fn undo_last_insert(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut last = state.last_inserted.lock().map_err(|e| e.to_string())?;
        undo_when_available(&mut last, || {
            crate::inject::with_external_focus(&app, crate::inject::undo_last_insert)
        })
    })
    .await
    .map_err(|e| format!("undo task failed: {e}"))?
}

fn undo_when_available(
    last: &mut Option<String>,
    undo: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if last.is_none() {
        return Err("nothing to undo".into());
    }
    undo()?;
    *last = None;
    Ok(())
}

#[tauri::command]
pub fn word_stats(state: State<AppState>) -> Result<crate::state::WordStats, String> {
    use crate::state::{DayWords, WordStats};
    use std::collections::HashMap;

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let rows = db::word_stats(&conn).map_err(|e| e.to_string())?;
    let total_sessions: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(sessions), 0) FROM daily_stats",
            [],
            |r| r.get(0),
        )
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

    let streak_days = current_streak(&by_day, chrono_day_offset);

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
pub async fn export_history(
    kind: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // Disk IO + serialization: keep off the main thread via spawn_blocking.
    // The DB guard is confined to this scope (it is not Send) so the future
    // stays Send across the await below.
    let entries = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::get_all_history(&conn).map_err(|e| e.to_string())?
    };
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn_blocking(move || {
        let exports_dir = data_dir.join("exports");
        std::fs::create_dir_all(&exports_dir).map_err(|e| e.to_string())?;

        let ext = match kind.as_str() {
            "json" => "json",
            "csv" => "csv",
            other => return Err(format!("unsupported export format '{other}'")),
        };
        let path = exports_dir.join(format!("history-{}.{ext}", db::now_timestamp()));

        let contents = match ext {
            "json" => serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?,
            _ => csv_contents(&entries),
        };

        std::fs::write(&path, contents).map_err(|e| e.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("export task failed: {e}"))?
}

fn csv_contents(entries: &[crate::state::HistoryEntry]) -> String {
    let mut out = String::from("id,text,created_at,duration_ms,source\n");
    for e in entries {
        let text = e.text.replace('"', "\"\"");
        out.push_str(&format!(
            "{},\"{}\",{},{},\"{}\"\n",
            e.id,
            text.replace(['\n', '\r'], " "),
            e.created_at,
            e.duration_ms,
            e.source.replace('"', "\"\"").replace(['\n', '\r'], " ")
        ));
    }
    out
}

fn current_streak(
    by_day: &std::collections::HashMap<String, u64>,
    day_offset: impl Fn(i64) -> String,
) -> u64 {
    let mut offset = if by_day.get(&day_offset(0)).copied().unwrap_or(0) > 0 {
        0
    } else {
        1
    };
    let mut days = 0;
    while by_day.get(&day_offset(-offset)).copied().unwrap_or(0) > 0 {
        days += 1;
        offset += 1;
    }
    days
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::HistoryEntry;

    #[test]
    fn failed_undo_can_be_retried() {
        let mut last = Some("inserted text".into());
        assert!(undo_when_available(&mut last, || Err("focus blocked".into())).is_err());
        assert_eq!(last.as_deref(), Some("inserted text"));
        undo_when_available(&mut last, || Ok(())).unwrap();
        assert!(last.is_none());
        assert!(undo_when_available(&mut last, || panic!("must not issue another undo")).is_err());
    }

    #[test]
    fn an_empty_today_does_not_break_yesterdays_streak() {
        let mut days = std::collections::HashMap::from([("-1".into(), 5), ("-2".into(), 2)]);
        assert_eq!(current_streak(&days, |n| n.to_string()), 2);
        days.insert("0".into(), 0);
        assert_eq!(current_streak(&days, |n| n.to_string()), 2);
        days.insert("0".into(), 3);
        assert_eq!(current_streak(&days, |n| n.to_string()), 3);
    }

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
        assert_eq!(lines[1], "1,\"say \"\"hi\"\"\",1724000000,1200,\"hotkey\"");
        assert_eq!(
            lines[2],
            "2,\"line1 line2, with comma\",1724000001,800,\"continuous\""
        );
    }
}

/// Local calendar date `days` from today (`days` negative → past). Uses the
/// OS timezone so "today" matches the day bucketing in `daily_stats`
/// (`date(created_at, 'unixepoch', 'localtime')`) and the frontend heatmap
/// grid, which both build keys in local time. The previous epoch-days/UTC
/// civil-date math disagreed with local dates for anyone not in UTC.
fn chrono_day_offset(days: i64) -> String {
    use chrono::Datelike;
    let local_date = chrono::Local::now().date_naive() + chrono::Duration::days(days);
    format!(
        "{:04}-{:02}-{:02}",
        local_date.year(),
        local_date.month(),
        local_date.day()
    )
}
