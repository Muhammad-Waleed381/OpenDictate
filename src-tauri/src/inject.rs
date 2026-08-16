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
    if has_active_x11_window() {
        run("xdotool", &["key", "--clearmodifiers", "ctrl+v"]).or_else(|e1| {
            run("ydotool", &["key", "29+47"]).map_err(|e2| format!("{e1}; {e2}"))
        })
    } else {
        run("ydotool", &["key", "29+47"]).or_else(|e1| {
            run("xdotool", &["key", "--clearmodifiers", "ctrl+v"])
                .map_err(|e2| format!("{e1}; {e2}"))
        })
    }
}

fn has_active_x11_window() -> bool {
    std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    match std::process::Command::new(program).args(args).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{program} exited with {status}")),
        Err(e) => Err(format!("failed to run {program}: {e}")),
    }
}