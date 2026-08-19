use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

pub fn inject_text(app: &AppHandle, text: &str, mode: &str) -> Result<(), String> {
    if mode == "clipboard" {
        let clipboard = app.clipboard();
        clipboard
            .write_text(text.to_string())
            .map_err(|e| format!("failed to write clipboard: {e}"))
    } else if mode == "type" {
        let result = type_via_ydotool(app, text);
        log::info!("inject: mode=type result={result:?}");
        result
    } else {
        let x11_active = has_active_x11_window();
        log::info!(
            "inject: mode=auto active_x11={x11_active} text={} chars",
            text.chars().count()
        );
        let result = if x11_active {
            paste_via_clipboard(app, text)
        } else {
            type_via_ydotool(app, text)
        };
        log::info!("inject: result={result:?}");
        result
    }
}

pub fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    let mut capitalize_next = true;
    for ch in text.trim().chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            prev_space = false;
            if ch == ',' || ch == '.' || ch == ';' || ch == ':' || ch == '?' || ch == '!' {
                if out.ends_with(' ') {
                    out.pop();
                }
                out.push(ch);
                capitalize_next = matches!(ch, '.' | '?' | '!');
                continue;
            }
            if capitalize_next && ch.is_alphabetic() {
                out.extend(ch.to_uppercase());
                capitalize_next = false;
            } else {
                out.push(ch);
                if ch.is_alphabetic() {
                    capitalize_next = false;
                }
            }
        }
    }
    let trimmed = out.trim_end().to_string();
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => {
            let mut cap = first.to_uppercase().collect::<String>();
            cap.push_str(chars.as_str());
            cap
        }
        None => trimmed,
    }
}

pub fn undo_last_insert() -> Result<(), String> {
    run("xdotool", &["key", "--clearmodifiers", "ctrl+z"]).or_else(|first| {
        run("ydotool", &["key", "29+44"])
            .map_err(|second| format!("{first}; {second}"))
    })
}

#[cfg(test)]
mod tests {
    use super::clean_text;

    #[test]
    fn capitalizes_sentences_and_removes_punctuation_spaces() {
        assert_eq!(clean_text("hello. world! how are you?"), "Hello. World! How are you?");
    }
}

fn paste_via_clipboard(app: &AppHandle, text: &str) -> Result<(), String> {
    let clipboard = app.clipboard();
    let previous = clipboard.read_text().unwrap_or_default();
    clipboard
        .write_text(text.to_string())
        .map_err(|e| format!("failed to write clipboard: {e}"))?;

    run("xdotool", &["key", "--clearmodifiers", "ctrl+v"]).or_else(|e1| {
        run("ydotool", &["key", "29+47"]).map_err(|e2| format!("{e1}; {e2}"))
    })?;

    if !previous.is_empty() {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            let _ = app.clipboard().write_text(previous);
        });
    }
    Ok(())
}

fn type_via_ydotool(app: &AppHandle, text: &str) -> Result<(), String> {
    match run("ydotool", &["type", text]) {
        Ok(()) => Ok(()),
        Err(e) => {
            log::info!("inject: ydotool type failed ({e}); falling back to clipboard paste");
            paste_via_clipboard(app, text)
        }
    }
}

fn has_active_x11_window() -> bool {
    let out = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output();
    match out {
        Ok(o) => {
            let ok = o.status.success() && !o.stdout.is_empty();
            let name = if ok {
                std::process::Command::new("xdotool")
                    .args(["getwindowname"])
                    .arg(std::str::from_utf8(&o.stdout).unwrap_or("").trim())
                    .output()
                    .ok()
                    .and_then(|n| {
                        String::from_utf8(n.stdout).ok().map(|s| s.trim().to_string())
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            log::info!("inject: active x11 window={ok} name={name:?}");
            ok
        }
        Err(e) => {
            log::info!("inject: xdotool getactivewindow failed: {e}");
            false
        }
    }
}

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    match std::process::Command::new(program).args(args).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{program} exited with {status}")),
        Err(e) => Err(format!("failed to run {program}: {e}")),
    }
}
