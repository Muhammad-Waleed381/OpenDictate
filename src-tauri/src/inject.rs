use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

pub fn inject_text(app: &AppHandle, text: &str) -> Result<(), String> {
    let clipboard = app.clipboard();
    let previous = clipboard.read_text().unwrap_or_default();
    clipboard
        .write_text(text.to_string())
        .map_err(|e| format!("failed to write clipboard: {e}"))?;

    paste_from_clipboard()?;

    if !previous.is_empty() {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            let _ = app.clipboard().write_text(previous);
        });
    }
    Ok(())
}

fn paste_from_clipboard() -> Result<(), String> {
    let (program, args) = if std::env::var("WAYLAND_DISPLAY").is_ok() {
        (
            "wtype".to_string(),
            vec!["-M".to_string(), "ctrl".to_string(), "-k".to_string(), "v".to_string(), "-m".to_string(), "ctrl".to_string()],
        )
    } else {
        (
            "xdotool".to_string(),
            vec!["key".to_string(), "--clearmodifiers".to_string(), "ctrl+v".to_string()],
        )
    };

    let status = std::process::Command::new(&program)
        .args(&args)
        .status()
        .map_err(|e| format!("failed to run {program}: {e}"))?;
    if !status.success() {
        return Err(format!("{program} exited with {status}"));
    }
    Ok(())
}