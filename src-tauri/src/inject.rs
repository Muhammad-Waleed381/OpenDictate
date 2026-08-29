use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

pub fn inject_text(app: &AppHandle, text: &str, mode: &str) -> Result<(), String> {
    copy_to_system_clipboard(app, text)?;

    if mode == "clipboard" {
        return Ok(());
    }

    // mode == "type" or mode == "auto"
    #[cfg(target_os = "linux")]
    {
        // Settle delay to ensure Wayland compositor and target window receive clipboard update
        std::thread::sleep(std::time::Duration::from_millis(50));
        let res = press_paste();
        log::info!("inject: press_paste result={res:?}");
        res
    }

    #[cfg(not(target_os = "linux"))]
    {
        let res = press_paste();
        log::info!("inject: press_paste result={res:?}");
        res
    }
}

pub fn copy_to_system_clipboard(app: &AppHandle, text: &str) -> Result<(), String> {
    // 1. Write via Tauri clipboard manager (GTK / OS native). The error is
    // kept: if every backend fails, the caller must know — otherwise a paste
    // injects whatever stale content was in the clipboard before.
    let clipboard = app.clipboard();
    let native_result = clipboard.write_text(text.to_string());

    // 2. On Linux, also write directly to wl-copy (Wayland) and xclip (X11)
    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut any_succeeded = native_result.is_ok();

        // Try wl-copy on Wayland
        if let Ok(mut child) = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let mut wrote = false;
            if let Some(mut stdin) = child.stdin.take() {
                wrote = stdin.write_all(text.as_bytes()).is_ok();
            }
            if let Ok(status) = child.wait() {
                if wrote && status.success() {
                    any_succeeded = true;
                }
            }
        }

        // Try xclip for X11 / Xwayland
        if let Ok(mut child) = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let mut wrote = false;
            if let Some(mut stdin) = child.stdin.take() {
                wrote = stdin.write_all(text.as_bytes()).is_ok();
            }
            if let Ok(status) = child.wait() {
                if wrote && status.success() {
                    any_succeeded = true;
                }
            }
        }

        if any_succeeded {
            Ok(())
        } else {
            Err("failed to write to clipboard (all backends failed)".to_string())
        }
    }

    #[cfg(not(target_os = "linux"))]
    native_result.map_err(|e| format!("failed to write to clipboard: {e}"))
}

pub fn clean_text(text: &str) -> String {
    let text = opendictate_core::text::strip_sound_effects(text);
    let text = opendictate_core::text::deduplicate_repeated_phrases(&text);
    let trimmed_input = text.trim();
    if trimmed_input.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(text.len());
    let mut prev_space = false;
    let mut capitalize_next = true;
    for ch in trimmed_input.chars() {
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

// --- Linux uinput & fallback keystroke injection ---------------------------

#[cfg(target_os = "linux")]
mod uinput {
    use std::fs::{File, OpenOptions};
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::io::AsRawFd;
    use std::sync::Mutex;
    use std::time::Duration;

    const UI_SET_EVBIT: libc::c_ulong = 0x40045564;
    const UI_SET_KEYBIT: libc::c_ulong = 0x40045565;
    const UI_DEV_SETUP: libc::c_ulong = 0x405c5503;
    const UI_DEV_CREATE: libc::c_ulong = 0x5501;
    const UI_DEV_DESTROY: libc::c_ulong = 0x5502;

    const EV_SYN: u16 = 0x00;
    const EV_KEY: u16 = 0x01;
    const SYN_REPORT: u16 = 0x00;

    pub const KEY_TAB: u16 = 15;
    pub const KEY_ENTER: u16 = 28;
    pub const KEY_LEFTCTRL: u16 = 29;
    pub const KEY_LEFTSHIFT: u16 = 42;
    pub const KEY_A: u16 = 30;
    pub const KEY_C: u16 = 46;
    pub const KEY_T: u16 = 20;
    pub const KEY_U: u16 = 22;
    pub const KEY_V: u16 = 47;
    pub const KEY_W: u16 = 17;
    pub const KEY_Z: u16 = 44;
    pub const KEY_BACKSPACE: u16 = 14;
    pub const KEY_PAGEUP: u16 = 104;
    pub const KEY_PAGEDOWN: u16 = 109;

    const ALL_KEYS: &[u16] = &[
        KEY_TAB, KEY_ENTER, KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_A, KEY_C,
        KEY_T, KEY_U, KEY_V, KEY_W, KEY_Z, KEY_BACKSPACE, KEY_PAGEUP, KEY_PAGEDOWN,
    ];

    #[repr(C)]
    struct InputId {
        bustype: u16,
        vendor: u16,
        product: u16,
        version: u16,
    }

    #[repr(C)]
    struct UInputSetup {
        id: InputId,
        name: [u8; 80],
        ff_effects_max: u32,
    }

    #[repr(C)]
    struct InputEvent {
        time_sec: usize,
        time_usec: usize,
        type_: u16,
        code: u16,
        value: i32,
    }

    pub struct Device {
        file: File,
    }

    impl Drop for Device {
        fn drop(&mut self) {
            let fd = self.file.as_raw_fd();
            unsafe {
                let _ = libc::ioctl(fd, UI_DEV_DESTROY);
            }
        }
    }

    static GLOBAL_DEVICE: Mutex<Option<Device>> = Mutex::new(None);

    fn get_or_create_device() -> Result<std::sync::MutexGuard<'static, Option<Device>>, String> {
        let mut guard = GLOBAL_DEVICE.lock().map_err(|e| e.to_string())?;
        if guard.is_none() {
            let file = OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open("/dev/uinput")
                .map_err(|e| format!("cannot open /dev/uinput: {e}"))?;

            let fd = file.as_raw_fd();
            unsafe {
                if libc::ioctl(fd, UI_SET_EVBIT, EV_KEY as libc::c_ulong) < 0 {
                    return Err("ioctl UI_SET_EVBIT failed".to_string());
                }

                for &key in ALL_KEYS {
                    if libc::ioctl(fd, UI_SET_KEYBIT, key as libc::c_ulong) < 0 {
                        return Err(format!("ioctl UI_SET_KEYBIT for {key} failed"));
                    }
                }

                let mut setup = UInputSetup {
                    id: InputId {
                        bustype: 0x03, // BUS_USB
                        vendor: 0x1234,
                        product: 0x5678,
                        version: 1,
                    },
                    name: [0; 80],
                    ff_effects_max: 0,
                };
                let name_bytes = b"OpenDictate Virtual Keyboard";
                setup.name[..name_bytes.len()].copy_from_slice(name_bytes);

                if libc::ioctl(fd, UI_DEV_SETUP, &setup) < 0 {
                    return Err("ioctl UI_DEV_SETUP failed".to_string());
                }

                if libc::ioctl(fd, UI_DEV_CREATE) < 0 {
                    return Err("ioctl UI_DEV_CREATE failed".to_string());
                }
            }

            // Initial settle time for compositor to register the virtual keyboard
            std::thread::sleep(Duration::from_millis(150));
            *guard = Some(Device { file });
            log::info!("uinput: created persistent virtual keyboard device");
        }
        Ok(guard)
    }

    pub fn send_key_combo(keys: &[u16]) -> Result<(), String> {
        let guard = get_or_create_device()?;
        let dev = guard.as_ref().ok_or("no uinput device")?;
        let fd = dev.file.as_raw_fd();

        let emit = |type_: u16, code: u16, value: i32| {
            let ev = InputEvent {
                time_sec: 0,
                time_usec: 0,
                type_,
                code,
                value,
            };
            unsafe {
                let ptr = &ev as *const InputEvent as *const libc::c_void;
                let size = std::mem::size_of::<InputEvent>();
                libc::write(fd, ptr, size);
            }
        };

        // Small pre-delay to allow focus to settle
        std::thread::sleep(Duration::from_millis(20));

        // Press keys in sequence
        for &key in keys {
            emit(EV_KEY, key, 1);
        }
        emit(EV_SYN, SYN_REPORT, 0);
        std::thread::sleep(Duration::from_millis(20));

        // Release keys in reverse sequence
        for &key in keys.iter().rev() {
            emit(EV_KEY, key, 0);
        }
        emit(EV_SYN, SYN_REPORT, 0);
        std::thread::sleep(Duration::from_millis(20));

        Ok(())
    }
}

/// Sends the paste shortcut to the focused window.
#[cfg(target_os = "linux")]
fn press_paste() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_LEFTCTRL, uinput::KEY_V])
        .or_else(|_| run("ydotool", &["key", "29+47"]))
        .or_else(|_| run("wtype", &["-M", "ctrl", "-m", "v"]))
        .or_else(|_| run("xdotool", &["key", "--clearmodifiers", "ctrl+v"]))
}

#[cfg(target_os = "linux")]
pub fn undo_last_insert() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_LEFTCTRL, uinput::KEY_Z])
        .or_else(|_| run("ydotool", &["key", "29+44"]))
        .or_else(|_| run("wtype", &["-M", "ctrl", "-m", "z"]))
        .or_else(|_| run("xdotool", &["key", "--clearmodifiers", "ctrl+z"]))
}

#[cfg(target_os = "linux")]
pub fn press_new_line() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_ENTER])
        .or_else(|_| run("ydotool", &["key", "28"]))
        .or_else(|_| run("wtype", &["-k", "Return"]))
        .or_else(|_| run("xdotool", &["key", "Return"]))
}

#[cfg(target_os = "linux")]
pub fn press_new_paragraph() -> Result<(), String> {
    press_new_line()?;
    press_new_line()
}

#[cfg(target_os = "linux")]
pub fn press_tab() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_TAB])
        .or_else(|_| run("ydotool", &["key", "15"]))
        .or_else(|_| run("wtype", &["-k", "Tab"]))
        .or_else(|_| run("xdotool", &["key", "Tab"]))
}

#[cfg(target_os = "linux")]
pub fn press_delete_word() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_LEFTCTRL, uinput::KEY_BACKSPACE])
        .or_else(|_| run("ydotool", &["key", "29+14"]))
        .or_else(|_| run("wtype", &["-M", "ctrl", "-k", "BackSpace"]))
        .or_else(|_| run("xdotool", &["key", "--clearmodifiers", "ctrl+BackSpace"]))
}

#[cfg(target_os = "linux")]
pub fn press_delete_line() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_LEFTCTRL, uinput::KEY_U])
        .or_else(|_| run("ydotool", &["key", "29+22"]))
        .or_else(|_| run("wtype", &["-M", "ctrl", "-m", "u"]))
        .or_else(|_| run("xdotool", &["key", "--clearmodifiers", "ctrl+u"]))
}

#[cfg(target_os = "linux")]
pub fn press_clear_all() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_LEFTCTRL, uinput::KEY_A, uinput::KEY_BACKSPACE])
        .or_else(|_| run("wtype", &["-M", "ctrl", "-m", "a", "-k", "BackSpace"]))
        .or_else(|_| run("xdotool", &["key", "--clearmodifiers", "ctrl+a", "BackSpace"]))
}

#[cfg(target_os = "linux")]
pub fn press_enter() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_ENTER])
        .or_else(|_| run("ydotool", &["key", "28"]))
        .or_else(|_| run("wtype", &["-k", "Return"]))
        .or_else(|_| run("xdotool", &["key", "Return"]))
}

#[cfg(target_os = "linux")]
pub fn press_interrupt() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_LEFTCTRL, uinput::KEY_C])
        .or_else(|_| run("ydotool", &["key", "29+46"]))
        .or_else(|_| run("wtype", &["-M", "ctrl", "-m", "c"]))
        .or_else(|_| run("xdotool", &["key", "--clearmodifiers", "ctrl+c"]))
}

#[cfg(target_os = "linux")]
pub fn press_next_tab() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_LEFTCTRL, uinput::KEY_TAB])
        .or_else(|_| run("wtype", &["-M", "ctrl", "-k", "Tab"]))
        .or_else(|_| run("xdotool", &["key", "--clearmodifiers", "ctrl+Tab"]))
}

#[cfg(target_os = "linux")]
pub fn press_prev_tab() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_LEFTCTRL, uinput::KEY_LEFTSHIFT, uinput::KEY_TAB])
        .or_else(|_| run("wtype", &["-M", "ctrl", "-M", "shift", "-k", "Tab"]))
        .or_else(|_| run("xdotool", &["key", "--clearmodifiers", "ctrl+shift+Tab"]))
}

#[cfg(target_os = "linux")]
pub fn press_new_tab() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_LEFTCTRL, uinput::KEY_T])
        .or_else(|_| run("wtype", &["-M", "ctrl", "-m", "t"]))
        .or_else(|_| run("xdotool", &["key", "--clearmodifiers", "ctrl+t"]))
}

#[cfg(target_os = "linux")]
pub fn press_close_tab() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_LEFTCTRL, uinput::KEY_W])
        .or_else(|_| run("wtype", &["-M", "ctrl", "-m", "w"]))
        .or_else(|_| run("xdotool", &["key", "--clearmodifiers", "ctrl+w"]))
}

#[cfg(target_os = "linux")]
pub fn press_scroll_down() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_PAGEDOWN])
        .or_else(|_| run("wtype", &["-k", "Page_Down"]))
        .or_else(|_| run("xdotool", &["key", "Page_Down"]))
}

#[cfg(target_os = "linux")]
pub fn press_scroll_up() -> Result<(), String> {
    uinput::send_key_combo(&[uinput::KEY_PAGEUP])
        .or_else(|_| run("wtype", &["-k", "Page_Up"]))
        .or_else(|_| run("xdotool", &["key", "Page_Up"]))
}

#[cfg(target_os = "linux")]
pub fn switch_to_app(app_name: &str) -> Result<(), String> {
    let lower = app_name.trim().to_lowercase();
    let candidates: Vec<&str> = match lower.as_str() {
        "cursor" => vec!["cursor", "Cursor"],
        "terminal" | "console" | "bash" | "term" => vec![
            "ptyxis", "gnome-terminal", "alacritty", "kitty", "konsole", "xterm", "terminal",
        ],
        "code" | "vscode" | "vs code" | "visual studio code" => vec!["code", "visual-studio-code", "vscodium"],
        "chrome" | "google chrome" => vec!["google-chrome", "google-chrome-stable", "chrome"],
        "browser" | "web" => vec!["google-chrome", "firefox", "brave-browser", "chromium"],
        "slack" => vec!["slack", "Slack"],
        "discord" => vec!["discord", "Discord"],
        "files" | "file manager" | "nautilus" => vec!["nautilus", "org.gnome.Nautilus", "dolphin", "thunar"],
        "spotify" => vec!["spotify", "Spotify"],
        "obsidian" => vec!["obsidian", "Obsidian"],
        _ => vec![lower.as_str()],
    };

    for target in candidates {
        // 1. Try wmctrl (standard on X11 / Xwayland)
        if run("wmctrl", &["-x", "-a", target]).is_ok() {
            return Ok(());
        }
        // 2. Try xdotool
        if run("xdotool", &["search", "--class", target, "windowactivate"]).is_ok() {
            return Ok(());
        }
        // 3. Try swaymsg (Wayland Sway/Hyprland)
        if run("swaymsg", &[&format!("[app_id=\"{target}\"]"), "focus"]).is_ok() {
            return Ok(());
        }
        // 4. Try gtk-launch
        if run("gtk-launch", &[target]).is_ok() {
            return Ok(());
        }
        // 5. Try direct spawn
        if std::process::Command::new(target).spawn().is_ok() {
            return Ok(());
        }
    }

    Err(format!("could not find or focus application '{app_name}'"))
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
fn press_paste() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Unicode('v'), Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(modifier, Direction::Release).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
fn type_text(_app: &AppHandle, text: &str) -> Result<(), String> {
    use enigo::{Enigo, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    // Give the focused window a beat to accept synthetic input.
    std::thread::sleep(std::time::Duration::from_millis(120));
    enigo
        .text(text)
        .map_err(|e| format!("failed to type text: {e}"))
}

#[cfg(not(target_os = "linux"))]
pub fn undo_last_insert() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Unicode('z'), Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(modifier, Direction::Release).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_new_line() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo.key(Key::Return, Direction::Click).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_new_paragraph() -> Result<(), String> {
    press_new_line()?;
    press_new_line()
}

#[cfg(not(target_os = "linux"))]
pub fn press_tab() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo.key(Key::Tab, Direction::Click).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_delete_word() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Alt;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Backspace, Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(modifier, Direction::Release).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_delete_line() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Backspace, Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(modifier, Direction::Release).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_clear_all() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Unicode('a'), Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(modifier, Direction::Release).map_err(|e| e.to_string())?;
    enigo.key(Key::Backspace, Direction::Click).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_enter() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo.key(Key::Return, Direction::Click).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_interrupt() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo.key(Key::Control, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Unicode('c'), Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(Key::Control, Direction::Release).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_next_tab() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Control;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Tab, Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(modifier, Direction::Release).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_prev_tab() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo.key(Key::Control, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Shift, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Tab, Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(Key::Shift, Direction::Release).map_err(|e| e.to_string())?;
    enigo.key(Key::Control, Direction::Release).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_new_tab() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Unicode('t'), Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(modifier, Direction::Release).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_close_tab() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;
    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Unicode('w'), Direction::Click).map_err(|e| e.to_string())?;
    enigo.key(modifier, Direction::Release).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_scroll_down() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo.key(Key::PageDown, Direction::Click).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_scroll_up() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo.key(Key::PageUp, Direction::Click).map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
pub fn switch_to_app(app_name: &str) -> Result<(), String> {
    let script = format!("tell application \"{}\" to activate", app_name);
    match std::process::Command::new("osascript").args(["-e", &script]).status() {
        Ok(status) if status.success() => Ok(()),
        _ => match std::process::Command::new("open").args(["-a", app_name]).status() {
            Ok(status) if status.success() => Ok(()),
            _ => Err(format!("could not switch to application '{app_name}'")),
        },
    }
}

#[cfg(target_os = "windows")]
pub fn switch_to_app(app_name: &str) -> Result<(), String> {
    let script = format!("(New-Object -ComObject WScript.Shell).AppActivate(\"{}\")", app_name);
    match std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]).status() {
        Ok(status) if status.success() => Ok(()),
        _ => Err(format!("could not switch to application '{app_name}'")),
    }
}

/// Helper to open a URL or perform a web search across operating systems.
pub fn open_browser_search(query: &str) -> Result<(), String> {
    // Encode UTF-8 *bytes* (proper percent-encoding). The previous char-based
    // version emitted `%XX` per code point, producing mojibake for any
    // non-ASCII query (e.g. "é" → `%E9` instead of `%C3%A9`).
    let encoded: String = query
        .bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    let url = format!("https://www.google.com/search?q={encoded}");
    open_browser_url(&url)
}

/// Opens a URL in the user's default browser.
pub fn open_browser_url(url: &str) -> Result<(), String> {
    let target = if !url.starts_with("http://") && !url.starts_with("https://") {
        format!("https://{url}")
    } else {
        url.to_string()
    };

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&target)
            .spawn()
            .map_err(|e| format!("failed to open browser with xdg-open: {e}"))?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&target)
            .spawn()
            .map_err(|e| format!("failed to open browser: {e}"))?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &target])
            .spawn()
            .map_err(|e| format!("failed to open browser: {e}"))?;
        Ok(())
    }
}
