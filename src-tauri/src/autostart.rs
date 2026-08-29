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

/// Windows writes an HKCU Run entry — no admin rights, survives updates,
/// and is the mechanism users expect when they flip "Start with system".
#[cfg(target_os = "windows")]
pub fn set_enabled(_app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let run = hkcu
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_SET_VALUE | KEY_QUERY_VALUE)
        .map_err(|e| format!("failed to open the Run registry key: {e}"))?;

    if !enabled {
        // Disabling is a no-op when never enabled, so it must succeed.
        run.delete_value("OpenDictate")
            .or_else(|e| match e.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(e),
            })
            .map_err(|e| format!("failed to remove the autostart entry: {e}"))?;
        return Ok(());
    }

    let exe = std::env::current_exe()
        .map_err(|e| format!("failed to locate the application: {e}"))?;
    run.set_value("OpenDictate", &format!("\"{}\"", exe.display()))
        .map_err(|e| format!("failed to write the autostart entry: {e}"))
}

#[cfg(target_os = "macos")]
pub fn set_enabled(_app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use std::fs;

    let home = std::env::var("HOME").map_err(|e| format!("failed to locate HOME directory: {e}"))?;
    let launch_agents = std::path::PathBuf::from(home).join("Library").join("LaunchAgents");
    let plist_path = launch_agents.join("com.opendictate.app.plist");

    if !enabled {
        if plist_path.exists() {
            fs::remove_file(&plist_path).map_err(|e| format!("failed to disable autostart: {e}"))?;
        }
        return Ok(());
    }

    fs::create_dir_all(&launch_agents).map_err(|e| format!("failed to create LaunchAgents directory: {e}"))?;
    let executable = std::env::current_exe()
        .map_err(|e| format!("failed to locate application executable: {e}"))?;
    let exe_str = executable.to_string_lossy();

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.opendictate.app</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe_str}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#
    );
    fs::write(plist_path, plist).map_err(|e| format!("failed to write LaunchAgent plist: {e}"))
}
