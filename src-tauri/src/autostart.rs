use std::fs;

use tauri::{AppHandle, Manager};

pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let dir = app
        .path()
        .config_dir()
        .map_err(|e| format!("failed to locate config directory: {e}"))?
        .join("autostart");
    let file = dir.join("com.opendictate.app.desktop");

    if !enabled {
        if file.exists() {
            fs::remove_file(&file).map_err(|e| format!("failed to disable autostart: {e}"))?;
        }
        return Ok(());
    }

    fs::create_dir_all(&dir).map_err(|e| format!("failed to create autostart directory: {e}"))?;
    let executable = std::env::current_exe()
        .map_err(|e| format!("failed to locate application: {e}"))?
        .to_string_lossy()
        .replace('"', "\\\"");
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=OpenDictate\nExec=\"{executable}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    );
    fs::write(file, desktop).map_err(|e| format!("failed to enable autostart: {e}"))
}
