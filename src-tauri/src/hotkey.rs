use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::dictation;
use crate::state::AppState;

pub fn register(app: &AppHandle, state: &AppState, key: &str) -> Result<(), String> {
    if let Ok(mut current) = state.hotkey.lock() {
        if current.as_deref() == Some(key) {
            return Ok(());
        }
        if let Some(old) = current.take() {
            let _ = app.global_shortcut().unregister(old.as_str());
        }
    }

    app.global_shortcut()
        .on_shortcut(key, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                toggle_dictation(app);
            }
        })
        .map_err(|e| format!("failed to register hotkey '{key}': {e}"))?;

    if let Ok(mut current) = state.hotkey.lock() {
        *current = Some(key.to_string());
    }
    log::info!("hotkey registered: {key}");
    Ok(())
}

pub fn toggle_dictation(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    if state.recorder.is_recording() {
        let _ = dictation::stop(app, &state);
    } else {
        let _ = dictation::start(app, &state, false);
    }
}