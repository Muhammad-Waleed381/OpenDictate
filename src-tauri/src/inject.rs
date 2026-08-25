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
        let result = type_text(app, text);
        log::info!("inject: mode=type result={result:?}");
        result
    } else {
        // auto
        #[cfg(target_os = "linux")]
        let result = {
            let x11_active = has_active_x11_window();
            log::info!("inject: mode=auto active_x11={x11_active}");
            if x11_active {
                paste_via_clipboard(app, text)
            } else {
                type_text(app, text)
            }
        };
        #[cfg(not(target_os = "linux"))]
        // Outside Linux, synthetic paste is the most reliable path into
        // whatever app has focus.
        let result = paste_via_clipboard(app, text);
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

#[cfg(test)]
mod tests {
    use super::clean_text;

    #[test]
    fn capitalizes_sentences_and_removes_punctuation_spaces() {
        assert_eq!(clean_text("hello. world! how are you?"), "Hello. World! How are you?");
    }
}

/// Writes `text` to the clipboard and sends Ctrl+V / Cmd+V to the focused app.
fn paste_via_clipboard(app: &AppHandle, text: &str) -> Result<(), String> {
    let clipboard = app.clipboard();
    // Save-and-restore everywhere: paste-insert must not permanently
    // destroy whatever the user had on their clipboard.
    let previous = clipboard.read_text().unwrap_or_default();
    clipboard
        .write_text(text.to_string())
        .map_err(|e| format!("failed to write clipboard: {e}"))?;

    press_paste()?;

    if !previous.is_empty() {
        let app = app.clone();
        std::thread::spawn(move || {
            // Long enough for the focused app to consume the paste.
            std::thread::sleep(Duration::from_millis(400));
            let _ = app.clipboard().write_text(previous);
        });
    }
    Ok(())
}

/// Sends the paste shortcut to the focused window.
#[cfg(target_os = "linux")]
fn press_paste() -> Result<(), String> {
    run("xdotool", &["key", "--clearmodifiers", "ctrl+v"])
        .or_else(|e1| run("wtype", &["-M", "ctrl", "-m", "v"]).map_err(|e2| format!("{e1}; {e2}")))
        .or_else(|e2| run("ydotool", &["key", "29+47"]).map_err(|e3| format!("{e2}; {e3}")))
}

#[cfg(not(target_os = "linux"))]
fn press_paste() -> Result<(), String> {
    send_combo(Combo::Paste)
}

#[cfg(target_os = "linux")]
fn type_text(app: &AppHandle, text: &str) -> Result<(), String> {
    if run("wtype", &[text]).is_ok() {
        return Ok(());
    }
    if run("ydotool", &["type", text]).is_ok() {
        return Ok(());
    }
    if run("xdotool", &["type", text]).is_ok() {
        return Ok(());
    }
    log::info!("inject: wtype/ydotool/xdotool-type unavailable; falling back to clipboard paste");
    paste_via_clipboard(app, text)
}

#[cfg(target_os = "linux")]
pub fn undo_last_insert() -> Result<(), String> {
    run("xdotool", &["key", "--clearmodifiers", "ctrl+z"])
        .or_else(|e1| run("wtype", &["-M", "ctrl", "-m", "z"]).map_err(|e2| format!("{e1}; {e2}")))
        .or_else(|e2| run("ydotool", &["key", "29+44"]).map_err(|e3| format!("{e2}; {e3}")))
}

#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn run(program: &str, args: &[&str]) -> Result<(), String> {
    match std::process::Command::new(program).args(args).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!("{program} exited with {status}")),
        Err(e) => Err(format!("failed to run {program}: {e}")),
    }
}

// --- Windows / macOS: synthetic input via enigo (SendInput / CGEvent) ------

#[cfg(not(target_os = "linux"))]
enum Combo {
    Paste,
    Undo,
}

#[cfg(not(target_os = "linux"))]
fn type_text(_app: &AppHandle, text: &str) -> Result<(), String> {
    use enigo::{Enigo, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    // Give the focused window a beat to accept synthetic input.
    std::thread::sleep(Duration::from_millis(120));
    enigo
        .text(text)
        .map_err(|e| format!("failed to type text: {e}"))
}

#[cfg(not(target_os = "linux"))]
pub fn undo_last_insert() -> Result<(), String> {
    send_combo(Combo::Undo)
}

#[cfg(not(target_os = "linux"))]
fn send_combo(combo: Combo) -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    // macOS pastes and undoes with Command, not Control — Ctrl+V does nothing
    // in virtually every Mac app, so clipboard insert mode and undo silently
    // no-oped there. Windows keeps Control.
    //
    // The keys are raw virtual keycodes rather than Key::Unicode because enigo
    // resolves Unicode through TSMGetInputSourceProperty, and HIToolbox asserts
    // that call happens on the main dispatch queue. Injection runs on the
    // inference worker, so the assert tripped and took the whole process down
    // with SIGTRAP the moment a transcript was pasted. Raw keycodes skip the
    // lookup, keeping injection off the main thread where it belongs.
    #[cfg(target_os = "macos")]
    let (accel, paste_key, undo_key) = (
        Key::Meta,
        Key::Other(0x09), // kVK_ANSI_V
        Key::Other(0x06), // kVK_ANSI_Z
    );
    #[cfg(not(target_os = "macos"))]
    let (accel, paste_key, undo_key) = (Key::Control, Key::Unicode('v'), Key::Unicode('z'));

    let (modifier, key) = match combo {
        Combo::Paste => (accel, paste_key),
        Combo::Undo => (accel, undo_key),
    };
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    std::thread::sleep(Duration::from_millis(80));
    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| format!("failed to press modifier: {e}"))?;
    enigo
        .key(key, Direction::Click)
        .map_err(|e| format!("failed to press key: {e}"))?;
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| format!("failed to release modifier: {e}"))
}
