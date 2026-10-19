// Launch-at-login. Linux writes a freedesktop `.desktop` entry into the
// user's autostart directory; Windows uses HKCU Run and macOS LaunchAgents.

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
    let executable = launcher_executable()?;
    let exec = desktop_exec_argument(&executable.to_string_lossy());
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=OpenDictate\nExec={exec}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
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
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE | KEY_QUERY_VALUE,
        )
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

    let exe =
        std::env::current_exe().map_err(|e| format!("failed to locate the application: {e}"))?;
    run.set_value("OpenDictate", &format!("\"{}\"", exe.display()))
        .map_err(|e| format!("failed to write the autostart entry: {e}"))
}

#[cfg(target_os = "macos")]
pub fn set_enabled(_app: &tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use std::fs;

    let home =
        std::env::var("HOME").map_err(|e| format!("failed to locate HOME directory: {e}"))?;
    let launch_agents = std::path::PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents");
    let plist_path = launch_agents.join("com.opendictate.app.plist");

    if !enabled {
        if plist_path.exists() {
            fs::remove_file(&plist_path)
                .map_err(|e| format!("failed to disable autostart: {e}"))?;
        }
        return Ok(());
    }

    fs::create_dir_all(&launch_agents)
        .map_err(|e| format!("failed to create LaunchAgents directory: {e}"))?;
    let executable = std::env::current_exe()
        .map_err(|e| format!("failed to locate application executable: {e}"))?;
    let plist = launch_agent_plist(&executable.to_string_lossy());
    fs::write(plist_path, plist).map_err(|e| format!("failed to write LaunchAgent plist: {e}"))
}

#[cfg(any(target_os = "macos", test))]
fn launch_agent_plist(executable: &str) -> String {
    let exe_str = xml_escape(executable);
    format!(
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
    )
}

#[cfg(target_os = "linux")]
pub fn launcher_executable() -> Result<std::path::PathBuf, String> {
    let current =
        std::env::current_exe().map_err(|e| format!("failed to locate application: {e}"))?;
    let image = std::env::var_os("APPIMAGE").map(std::path::PathBuf::from);
    let executable = persistent_executable(current, image)?;
    if !executable.is_file() {
        return Err(
            "application launcher no longer exists; start OpenDictate from its installed location"
                .into(),
        );
    }
    Ok(executable)
}

#[cfg(any(target_os = "linux", test))]
fn persistent_executable(
    current: std::path::PathBuf,
    appimage: Option<std::path::PathBuf>,
) -> Result<std::path::PathBuf, String> {
    match appimage {
        Some(image) if image.is_absolute() => Ok(image),
        Some(_) => Err("APPIMAGE must be an absolute path".into()),
        None => Ok(current),
    }
}

#[cfg(any(target_os = "linux", test))]
fn desktop_exec_argument(executable: &str) -> String {
    // Exec has both key-file escaping and command-line quoting layers.
    let mut quoted = String::from("\"");
    for ch in executable.chars() {
        if matches!(ch, '"' | '\\' | '$' | '`') {
            quoted.push('\\');
        }
        if ch == '%' {
            quoted.push('%');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    quoted
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

#[cfg(any(target_os = "macos", test))]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appimage_autostart_uses_original_file_not_temporary_mount() {
        let path = persistent_executable(
            "/tmp/.mount_Open/app/usr/bin/opendictate".into(),
            Some("/home/user/Apps/Open Dictate.AppImage".into()),
        )
        .unwrap();
        assert_eq!(
            path,
            std::path::Path::new("/home/user/Apps/Open Dictate.AppImage")
        );
        assert!(persistent_executable(
            "/usr/bin/opendictate".into(),
            Some("relative.AppImage".into())
        )
        .is_err());
        assert_eq!(
            persistent_executable("/usr/bin/opendictate".into(), None).unwrap(),
            std::path::Path::new("/usr/bin/opendictate")
        );
    }

    #[test]
    fn mac_launcher_paths_are_xml_text() {
        let plist = launch_agent_plist("/Users/A&B/<tools>/Open\"Dictate");
        assert!(plist.contains("<string>/Users/A&amp;B/&lt;tools&gt;/Open&quot;Dictate</string>"));
        assert!(!plist.contains("/Users/A&B/"));
    }

    #[test]
    fn desktop_launcher_preserves_spaces_and_literal_percent() {
        assert_eq!(
            desktop_exec_argument("/home/me/Open Dictate%20.AppImage"),
            "\"/home/me/Open Dictate%%20.AppImage\""
        );
    }
}
