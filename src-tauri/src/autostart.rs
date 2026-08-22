// Launch-at-login. Linux writes a freedesktop `.desktop` entry into the
// user's autostart directory; other platforms are not wired up yet and
// report unsupported instead of silently doing nothing.

/// Enables or disables launch-at-login.
#[cfg(target_os = "linux")]
pub fn set_enabled(app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use std::fs;
    use tauri::Manager;

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

#[cfg(not(target_os = "linux"))]
pub fn set_enabled(_app: &tauri::AppHandle, _enabled: bool) -> Result<(), String> {
    Err("autostart is not supported on this platform yet".to_string())
}
