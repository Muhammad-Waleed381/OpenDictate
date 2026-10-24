use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

pub fn inject_text(app: &AppHandle, text: &str, mode: &str) -> Result<(), String> {
    copy_to_system_clipboard(app, text)?;
    if mode == "clipboard" {
        return Ok(());
    }
    let terminal = uses_terminal_shortcuts(app)?;
    std::thread::sleep(std::time::Duration::from_millis(150));
    require_external_focus(app)?;
    press_paste(terminal)
}

fn uses_terminal_shortcuts(app: &AppHandle) -> Result<bool, String> {
    let state = app.state::<crate::state::AppState>();
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.input_target == "terminal")
}

/// Whether a window must be hidden to hand the keyboard focus back to the
/// user's application.
///
/// The dock is a persistent on-screen HUD: hiding it would make the
/// microphone control disappear. It is also never focusable, so it can never
/// be the window that holds the focus in the first place.
pub fn must_hide_for_focus_handoff(label: &str) -> bool {
    label != "dock"
}

/// UI insertion first hides OpenDictate so the window manager restores the
/// previous app. Never send editing keys while any OpenDictate window is focused.
pub fn with_external_focus<T>(
    app: &AppHandle,
    action: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let mut hidden = Vec::new();
    let mut failure: Option<String> = None;
    for window in app.webview_windows().into_values() {
        // The dock is a persistent HUD: hiding it would make the on-screen
        // microphone control disappear. It is also never focusable, so it
        // cannot be the window that needs handing the focus back.
        if !must_hide_for_focus_handoff(window.label()) {
            continue;
        }
        let focused = match window.is_focused() {
            Ok(focused) => focused,
            Err(e) => {
                failure = Some(e.to_string());
                break;
            }
        };
        if focused {
            if let Err(e) = window.hide() {
                failure = Some(format!("could not return focus to the previous app: {e}"));
                break;
            }
            hidden.push(window);
        }
    }
    if let Some(error) = failure {
        // Restore every window that was already hidden before giving up.
        for window in hidden {
            let _ = window.show();
            let _ = window.set_focus();
        }
        return Err(error);
    }
    if !hidden.is_empty() {
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let result = require_external_focus(app).and_then(|_| action());
    if result.is_err() {
        for window in hidden {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
    result
}

fn require_external_focus(app: &AppHandle) -> Result<(), String> {
    for window in app.webview_windows().into_values() {
        ensure_external_focus(
            window
                .is_focused()
                .map_err(|e| format!("could not verify target focus: {e}"))?,
        )?;
    }
    Ok(())
}

fn ensure_external_focus(own_window_focused: bool) -> Result<(), String> {
    if own_window_focused {
        Err("OpenDictate still has focus. Select the destination app and try again.".into())
    } else {
        Ok(())
    }
}

pub fn copy_to_system_clipboard(app: &AppHandle, text: &str) -> Result<(), String> {
    write_clipboards(app, text)
}

fn clipboard_write_result(any_succeeded: bool) -> Result<(), String> {
    if any_succeeded {
        Ok(())
    } else {
        Err("failed to write clipboard; no paste was sent".into())
    }
}

fn write_clipboards(app: &AppHandle, text: &str) -> Result<(), String> {
    let native_ok = app.clipboard().write_text(text.to_string()).is_ok();
    #[cfg(target_os = "linux")]
    {
        // Keep CLIPBOARD synchronized across Wayland and Xwayland. The target
        // profile chooses an explicit clipboard shortcut, never stale PRIMARY.
        let mut clipboard_ok = native_ok;
        for (program, args) in [
            ("wl-copy", vec!["--type", "text/plain;charset=utf-8"]),
            ("xclip", vec!["-selection", "clipboard", "-in"]),
            ("xsel", vec!["--clipboard", "--input"]),
        ] {
            clipboard_ok |= write_clipboard_tool(program, &args, text);
        }
        clipboard_write_result(clipboard_ok)
    }
    #[cfg(not(target_os = "linux"))]
    clipboard_write_result(native_ok)
}

#[cfg(target_os = "linux")]
fn write_clipboard_tool(program: &str, args: &[&str], text: &str) -> bool {
    use std::{
        io::Write,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    // The writer must not block the calling thread if a broken tool stops
    // reading stdin. Killing it at the deadline closes that pipe as well.
    let stdin = child.stdin.take();
    let content = text.to_owned();
    let writer = std::thread::spawn(move || {
        stdin.is_some_and(|mut input| input.write_all(content.as_bytes()).is_ok())
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let succeeded = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    // Always join: the writer terminates once the pipe is closed, and
    // detaching it would leak a thread on every failed write.
    let written = writer.join().unwrap_or(false);
    succeeded && written
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
    use super::*;

    #[test]
    fn clipboard_failure_prevents_paste_and_own_focus_is_rejected() {
        assert!(clipboard_write_result(false).is_err());
        assert!(clipboard_write_result(true).is_ok());
        assert!(ensure_external_focus(true).is_err());
        assert!(ensure_external_focus(false).is_ok());
    }

    #[test]
    fn the_dock_is_never_hidden_for_the_focus_handoff() {
        // The dock is a persistent HUD. Hiding it would make the on-screen
        // microphone control disappear mid-session.
        assert!(!must_hide_for_focus_handoff("dock"));
        assert!(must_hide_for_focus_handoff("main"));
    }

    #[test]
    fn app_names_are_data_not_script_source() {
        let name = "app\"; $(touch sentinel); #";
        let mac = mac_app_command(name);
        assert_eq!(mac.get_args().collect::<Vec<_>>(), vec!["-a", name]);
        let windows = windows_app_command(name);
        assert!(!windows
            .get_args()
            .any(|arg| arg.to_string_lossy().contains(name)));
        assert!(windows
            .get_envs()
            .any(|(key, value)| key == "OPENDICTATE_TARGET_APP"
                && value == Some(std::ffi::OsStr::new(name))));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sway_app_names_stay_inside_one_literal_criterion() {
        let criterion = sway_app_criteria("App[1]\"; exec bad");
        let encoded = criterion
            .strip_prefix("[app_id=")
            .unwrap()
            .strip_suffix(']')
            .unwrap();
        let pattern: String = serde_json::from_str(encoded).unwrap();
        assert_eq!(pattern, "^App\\[1\\]\"; exec bad$");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn window_matchers_receive_a_literal_name() {
        // `wmctrl -a` and `xdotool search --class` match a regular expression.
        // A spoken name must never be able to match an unrelated window.
        assert_eq!(literal_regex("chrome"), "chrome");
        assert_eq!(literal_regex(".*"), "\\.\\*");
        assert_eq!(literal_regex("a+b"), "a\\+b");
        assert_eq!(literal_regex("app[1]"), "app\\[1\\]");
        // The escaped form still matches the intended name literally.
        let pattern = literal_regex("Terminal");
        let regex = regex_lite_matches(&pattern, "Terminal");
        assert!(regex);
        assert!(!regex_lite_matches(&pattern, "XTerm"));
    }

    /// A tiny matcher for the literal-regex subset the escaper produces:
    /// everything except the escaped metacharacters is literal.
    #[cfg(target_os = "linux")]
    fn regex_lite_matches(pattern: &str, text: &str) -> bool {
        let mut pattern_chars = pattern.chars().peekable();
        let mut text_chars = text.chars();
        while let Some(ch) = pattern_chars.next() {
            if ch == '\\' {
                let Some(escaped) = pattern_chars.next() else {
                    return false;
                };
                match text_chars.next() {
                    Some(actual) if actual == escaped => {}
                    _ => return false,
                }
            } else {
                match text_chars.next() {
                    Some(actual) if actual == ch => {}
                    _ => return false,
                }
            }
        }
        text_chars.next().is_none()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn wtype_chords_press_keys_and_release_every_modifier() {
        assert_eq!(
            wtype_args(&["ctrl"], "z"),
            ["-M", "ctrl", "-k", "z", "-m", "ctrl"]
        );
        assert_eq!(
            wtype_args(&["ctrl", "shift"], "Tab"),
            ["-M", "ctrl", "-M", "shift", "-k", "Tab", "-m", "shift", "-m", "ctrl"]
        );
        assert_eq!(wtype_args(&[], "Return"), ["-k", "Return"]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn text_fields_and_shells_use_distinct_supported_editing_shortcuts() {
        let (mods, key) = paste_chord(false);
        assert_eq!(
            wtype_args(&mods.iter().map(|m| m.0).collect::<Vec<_>>(), key.0),
            ["-M", "ctrl", "-k", "v", "-m", "ctrl"]
        );
        let (mods, key) = paste_chord(true);
        assert_eq!(
            wtype_args(&mods.iter().map(|m| m.0).collect::<Vec<_>>(), key.0),
            ["-M", "ctrl", "-M", "shift", "-k", "v", "-m", "shift", "-m", "ctrl"]
        );
        let text = delete_line_chords(false);
        assert_eq!(
            text.iter().map(|(_, key)| key.0).collect::<Vec<_>>(),
            ["Home", "End", "BackSpace"]
        );
        let shell = delete_line_chords(true);
        assert_eq!(
            shell.iter().map(|(_, key)| key.0).collect::<Vec<_>>(),
            ["a", "k"]
        );
        assert!(shell
            .iter()
            .all(|(mods, _)| mods == &[("ctrl", uinput::KEY_LEFTCTRL)]));
    }

    #[test]
    fn capitalizes_sentences_and_removes_punctuation_spaces() {
        assert_eq!(
            clean_text("hello. world! how are you?"),
            "Hello. World! How are you?"
        );
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
    pub const KEY_HOME: u16 = 102;
    pub const KEY_END: u16 = 107;
    pub const KEY_K: u16 = 37;
    pub const KEY_V: u16 = 47;
    pub const KEY_W: u16 = 17;
    pub const KEY_Z: u16 = 44;
    pub const KEY_BACKSPACE: u16 = 14;
    pub const KEY_PAGEUP: u16 = 104;
    pub const KEY_PAGEDOWN: u16 = 109;

    const ALL_KEYS: &[u16] = &[
        KEY_TAB,
        KEY_ENTER,
        KEY_LEFTCTRL,
        KEY_LEFTSHIFT,
        KEY_A,
        KEY_C,
        KEY_T,
        KEY_HOME,
        KEY_END,
        KEY_K,
        KEY_V,
        KEY_W,
        KEY_Z,
        KEY_BACKSPACE,
        KEY_PAGEUP,
        KEY_PAGEDOWN,
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
            std::thread::sleep(Duration::from_millis(250));
            *guard = Some(Device { file });
            log::info!("uinput: created persistent virtual keyboard device");
        }
        Ok(guard)
    }

    pub fn send_key_combo(keys: &[u16]) -> Result<(), String> {
        let guard = get_or_create_device()?;
        let dev = guard.as_ref().ok_or("no uinput device")?;
        let fd = dev.file.as_raw_fd();

        let emit = |type_: u16, code: u16, value: i32| -> Result<(), String> {
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
                let written = libc::write(fd, ptr, size);
                if written != size as isize {
                    return Err(format!(
                        "uinput write failed: {}",
                        std::io::Error::last_os_error()
                    ));
                }
            }
            Ok(())
        };

        // Small pre-delay to allow focus to settle
        std::thread::sleep(Duration::from_millis(30));

        // Press keys in sequence
        let pressed = keys
            .iter()
            .try_for_each(|&key| emit(EV_KEY, key, 1))
            .and_then(|_| emit(EV_SYN, SYN_REPORT, 0));
        std::thread::sleep(Duration::from_millis(45));

        // Release keys in reverse sequence
        let mut released = Ok(());
        for &key in keys.iter().rev() {
            // Release even after a partial write; do not leave modifiers held.
            if let Err(e) = emit(EV_KEY, key, 0) {
                released = Err(e);
            }
        }
        let synced = emit(EV_SYN, SYN_REPORT, 0);
        std::thread::sleep(Duration::from_millis(30));

        pressed.and(released).and(synced)
    }
}

#[cfg(target_os = "linux")]
fn wtype_args(modifiers: &[&str], key: &str) -> Vec<String> {
    let mut args = Vec::new();
    for modifier in modifiers {
        args.extend(["-M".into(), (*modifier).into()]);
    }
    args.extend(["-k".into(), key.into()]);
    for modifier in modifiers.iter().rev() {
        args.extend(["-m".into(), (*modifier).into()]);
    }
    args
}

#[cfg(target_os = "linux")]
fn linux_key(modifiers: &[(&str, u16)], key: (&str, u16)) -> Result<(), String> {
    let keys: Vec<u16> = modifiers
        .iter()
        .map(|(_, code)| *code)
        .chain([key.1])
        .collect();
    uinput::send_key_combo(&keys)
        .or_else(|_| {
            let args: Vec<String> = std::iter::once("key".into())
                .chain(keys.iter().map(|code| format!("{code}:1")))
                .chain(keys.iter().rev().map(|code| format!("{code}:0")))
                .collect();
            run(
                "ydotool",
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        })
        .or_else(|_| {
            let modifiers: Vec<&str> = modifiers.iter().map(|(name, _)| *name).collect();
            let args = wtype_args(&modifiers, key.0);
            run(
                "wtype",
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        })
        .or_else(|_| {
            let chord = modifiers
                .iter()
                .map(|(name, _)| *name)
                .chain([key.0])
                .collect::<Vec<_>>()
                .join("+");
            run("xdotool", &["key", "--clearmodifiers", &chord])
        })
}

#[cfg(target_os = "linux")]
type LinuxKey = (&'static str, u16);
#[cfg(target_os = "linux")]
type LinuxChord = (Vec<LinuxKey>, LinuxKey);

#[cfg(target_os = "linux")]
fn paste_chord(terminal: bool) -> LinuxChord {
    let mut modifiers = vec![("ctrl", uinput::KEY_LEFTCTRL)];
    if terminal {
        modifiers.push(("shift", uinput::KEY_LEFTSHIFT));
    }
    (modifiers, ("v", uinput::KEY_V))
}

#[cfg(target_os = "linux")]
fn press_paste(terminal: bool) -> Result<(), String> {
    let (modifiers, key) = paste_chord(terminal);
    linux_key(&modifiers, key)
}

#[cfg(target_os = "linux")]
pub fn undo_last_insert() -> Result<(), String> {
    linux_key(&[("ctrl", uinput::KEY_LEFTCTRL)], ("z", uinput::KEY_Z))
}

#[cfg(target_os = "linux")]
pub fn press_new_line() -> Result<(), String> {
    press_enter()
}

#[cfg(target_os = "linux")]
pub fn press_new_paragraph() -> Result<(), String> {
    press_enter()?;
    press_enter()
}

#[cfg(target_os = "linux")]
pub fn press_tab() -> Result<(), String> {
    linux_key(&[], ("Tab", uinput::KEY_TAB))
}

#[cfg(target_os = "linux")]
pub fn press_delete_word() -> Result<(), String> {
    linux_key(
        &[("ctrl", uinput::KEY_LEFTCTRL)],
        ("BackSpace", uinput::KEY_BACKSPACE),
    )
}

#[cfg(target_os = "linux")]
fn delete_line_chords(terminal: bool) -> Vec<LinuxChord> {
    if terminal {
        // Readline shells (Bash/Zsh): beginning-of-line then kill-to-end,
        // independent of the cursor's current position.
        vec![
            (vec![("ctrl", uinput::KEY_LEFTCTRL)], ("a", uinput::KEY_A)),
            (vec![("ctrl", uinput::KEY_LEFTCTRL)], ("k", uinput::KEY_K)),
        ]
    } else {
        vec![
            (vec![], ("Home", uinput::KEY_HOME)),
            (
                vec![("shift", uinput::KEY_LEFTSHIFT)],
                ("End", uinput::KEY_END),
            ),
            (vec![], ("BackSpace", uinput::KEY_BACKSPACE)),
        ]
    }
}

#[cfg(target_os = "linux")]
pub fn press_delete_line(app: &AppHandle) -> Result<(), String> {
    for (modifiers, key) in delete_line_chords(uses_terminal_shortcuts(app)?) {
        linux_key(&modifiers, key)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn press_clear_all() -> Result<(), String> {
    linux_key(&[("ctrl", uinput::KEY_LEFTCTRL)], ("a", uinput::KEY_A))?;
    linux_key(&[], ("BackSpace", uinput::KEY_BACKSPACE))
}

#[cfg(target_os = "linux")]
pub fn press_enter() -> Result<(), String> {
    linux_key(&[], ("Return", uinput::KEY_ENTER))
}

#[cfg(target_os = "linux")]
pub fn press_interrupt() -> Result<(), String> {
    linux_key(&[("ctrl", uinput::KEY_LEFTCTRL)], ("c", uinput::KEY_C))
}

#[cfg(target_os = "linux")]
pub fn press_next_tab() -> Result<(), String> {
    linux_key(&[("ctrl", uinput::KEY_LEFTCTRL)], ("Tab", uinput::KEY_TAB))
}

#[cfg(target_os = "linux")]
pub fn press_prev_tab() -> Result<(), String> {
    linux_key(
        &[
            ("ctrl", uinput::KEY_LEFTCTRL),
            ("shift", uinput::KEY_LEFTSHIFT),
        ],
        ("Tab", uinput::KEY_TAB),
    )
}

#[cfg(target_os = "linux")]
pub fn press_new_tab() -> Result<(), String> {
    linux_key(&[("ctrl", uinput::KEY_LEFTCTRL)], ("t", uinput::KEY_T))
}

#[cfg(target_os = "linux")]
pub fn press_close_tab() -> Result<(), String> {
    linux_key(&[("ctrl", uinput::KEY_LEFTCTRL)], ("w", uinput::KEY_W))
}

#[cfg(target_os = "linux")]
pub fn press_scroll_down() -> Result<(), String> {
    linux_key(&[], ("Page_Down", uinput::KEY_PAGEDOWN))
}

#[cfg(target_os = "linux")]
pub fn press_scroll_up() -> Result<(), String> {
    linux_key(&[], ("Page_Up", uinput::KEY_PAGEUP))
}

#[cfg(target_os = "linux")]
pub fn switch_to_app(app_name: &str) -> Result<(), String> {
    let lower = app_name.trim().to_lowercase();
    let candidates: Vec<&str> = match lower.as_str() {
        "cursor" => vec!["cursor", "Cursor"],
        "terminal" | "console" | "bash" | "term" => vec![
            "ptyxis",
            "gnome-terminal",
            "alacritty",
            "kitty",
            "konsole",
            "xterm",
            "terminal",
        ],
        "code" | "vscode" | "vs code" | "visual studio code" => {
            vec!["code", "visual-studio-code", "vscodium"]
        }
        "chrome" | "google chrome" => vec!["google-chrome", "google-chrome-stable", "chrome"],
        "browser" | "web" => vec!["google-chrome", "firefox", "brave-browser", "chromium"],
        "slack" => vec!["slack", "Slack"],
        "discord" => vec!["discord", "Discord"],
        "files" | "file manager" | "nautilus" => {
            vec!["nautilus", "org.gnome.Nautilus", "dolphin", "thunar"]
        }
        "spotify" => vec!["spotify", "Spotify"],
        "obsidian" => vec!["obsidian", "Obsidian"],
        _ => vec![lower.as_str()],
    };

    for target in candidates {
        // `wmctrl -a` and `xdotool search --class` match their argument as a
        // regular expression. The name comes from a spoken command, so it
        // must be escaped: an unescaped name could focus an unrelated
        // window, and the next dictated insertion would land there.
        let literal = literal_regex(target);
        // 1. Try wmctrl (standard on X11 / Xwayland)
        if run("wmctrl", &["-x", "-a", &literal]).is_ok() {
            return Ok(());
        }
        // 2. Try xdotool
        if run(
            "xdotool",
            &["search", "--class", &literal, "windowactivate"],
        )
        .is_ok()
        {
            return Ok(());
        }
        // 3. Try swaymsg (Wayland Sway/Hyprland)
        if run("swaymsg", &[&sway_app_criteria(target), "focus"]).is_ok() {
            return Ok(());
        }
        // 4. Try gtk-launch (runs the `Exec=` line of a matching .desktop
        //    entry; the name is passed as a single argument, never a shell
        //    string).
        if run("gtk-launch", &[target]).is_ok() {
            return Ok(());
        }
    }

    Err(format!("could not find or focus application '{app_name}'"))
}

/// Escapes a name for use as a literal regular expression.
#[cfg(target_os = "linux")]
fn literal_regex(target: &str) -> String {
    let mut literal = String::with_capacity(target.len());
    for ch in target.chars() {
        if ".^$[](){}*+?|\\".contains(ch) {
            literal.push('\\');
        }
        literal.push(ch);
    }
    literal
}

#[cfg(target_os = "linux")]
fn sway_app_criteria(target: &str) -> String {
    // Sway interprets criteria as a quoted regular expression. Quote both
    // layers so an app name cannot terminate the criteria or add commands.
    let mut literal = String::from("^");
    for ch in target.chars() {
        if ".^$[](){}*+?|\\".contains(ch) {
            literal.push('\\');
        }
        literal.push(ch);
    }
    literal.push('$');
    format!(
        "[app_id={}]",
        serde_json::to_string(&literal).expect("string JSON")
    )
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
fn press_paste(_terminal: bool) -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;

    #[cfg(target_os = "windows")]
    {
        // Release lingering modifiers if any physical hotkeys were held
        let _ = enigo.key(Key::Alt, Direction::Release);
        let _ = enigo.key(Key::Shift, Direction::Release);
        let _ = enigo.key(Key::Meta, Direction::Release);
    }

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    #[cfg(target_os = "windows")]
    let v_key = Key::V;
    #[cfg(not(target_os = "windows"))]
    let v_key = Key::Unicode('v');

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(25));
    // Release the modifier on every path: a failed key press would
    // otherwise leave it held and every later keystroke modified.
    let pressed = enigo.key(v_key, Direction::Press);
    std::thread::sleep(std::time::Duration::from_millis(35));
    let released_key = enigo.key(v_key, Direction::Release);
    std::thread::sleep(std::time::Duration::from_millis(25));
    let released_modifier = enigo.key(modifier, Direction::Release);
    pressed
        .and(released_key)
        .and(released_modifier)
        .map_err(|e| e.to_string())
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

    #[cfg(target_os = "windows")]
    {
        // Release lingering modifiers if any physical hotkeys were held
        let _ = enigo.key(Key::Alt, Direction::Release);
        let _ = enigo.key(Key::Shift, Direction::Release);
        let _ = enigo.key(Key::Meta, Direction::Release);
    }

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    #[cfg(target_os = "windows")]
    let z_key = Key::Z;
    #[cfg(not(target_os = "windows"))]
    let z_key = Key::Unicode('z');

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(25));
    // Release the modifier on every path: a failed key press would otherwise
    // leave it held and every later keystroke would be modified.
    let pressed = enigo.key(z_key, Direction::Press);
    std::thread::sleep(std::time::Duration::from_millis(35));
    let released_key = enigo.key(z_key, Direction::Release);
    std::thread::sleep(std::time::Duration::from_millis(25));
    let released_modifier = enigo.key(modifier, Direction::Release);
    pressed
        .and(released_key)
        .and(released_modifier)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_new_line() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo
        .key(Key::Return, Direction::Click)
        .map_err(|e| e.to_string())
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
    enigo
        .key(Key::Tab, Direction::Click)
        .map_err(|e| e.to_string())
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
    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| e.to_string())?;
    // Every exit path must release the modifier, otherwise a failed click
    // leaves it held and every later keystroke is modified.
    let clicked = enigo.key(Key::Backspace, Direction::Click);
    let released = enigo.key(modifier, Direction::Release);
    clicked.and(released).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_delete_line(app: &AppHandle) -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    if uses_terminal_shortcuts(app)? {
        #[cfg(target_os = "windows")]
        let shell_keys = [Key::A, Key::K];
        #[cfg(not(target_os = "windows"))]
        let shell_keys = [Key::Unicode('a'), Key::Unicode('k')];
        for key in shell_keys {
            enigo
                .key(Key::Control, Direction::Press)
                .map_err(|e| e.to_string())?;
            let clicked = enigo.key(key, Direction::Click);
            let released = enigo.key(Key::Control, Direction::Release);
            clicked.and(released).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        enigo
            .key(Key::Meta, Direction::Press)
            .map_err(|e| e.to_string())?;
        // Every exit path must release Meta, otherwise a failed click leaves
        // the Command key held and every later keystroke is Cmd-modified.
        let selected = enigo.key(Key::LeftArrow, Direction::Click);
        let released_meta = enigo.key(Key::Meta, Direction::Release);
        selected.and(released_meta).map_err(|e| e.to_string())?;
        enigo
            .key(Key::Shift, Direction::Press)
            .map_err(|e| e.to_string())?;
        let selected = enigo.key(Key::RightArrow, Direction::Click);
        let released_shift = enigo.key(Key::Shift, Direction::Release);
        selected.and(released_shift).map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        enigo
            .key(Key::Home, Direction::Click)
            .map_err(|e| e.to_string())?;
        enigo
            .key(Key::Shift, Direction::Press)
            .map_err(|e| e.to_string())?;
        let selected = enigo.key(Key::End, Direction::Click);
        let released = enigo.key(Key::Shift, Direction::Release);
        selected.and(released).map_err(|e| e.to_string())?;
    }
    enigo
        .key(Key::Backspace, Direction::Click)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_clear_all() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;

    #[cfg(target_os = "windows")]
    {
        // Release lingering modifiers if any physical hotkeys were held
        let _ = enigo.key(Key::Alt, Direction::Release);
        let _ = enigo.key(Key::Shift, Direction::Release);
        let _ = enigo.key(Key::Meta, Direction::Release);
    }

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    #[cfg(target_os = "windows")]
    let a_key = Key::A;
    #[cfg(not(target_os = "windows"))]
    let a_key = Key::Unicode('a');

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(25));
    // Release the modifier on every path: a failed key press would
    // otherwise leave it held and every later keystroke modified.
    let pressed = enigo.key(a_key, Direction::Press);
    std::thread::sleep(std::time::Duration::from_millis(35));
    let released_key = enigo.key(a_key, Direction::Release);
    std::thread::sleep(std::time::Duration::from_millis(25));
    let released_modifier = enigo.key(modifier, Direction::Release);
    pressed
        .and(released_key)
        .and(released_modifier)
        .map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(25));
    enigo
        .key(Key::Backspace, Direction::Click)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_enter() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo
        .key(Key::Return, Direction::Click)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_interrupt() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;

    #[cfg(target_os = "windows")]
    {
        // Release lingering modifiers if any physical hotkeys were held
        let _ = enigo.key(Key::Alt, Direction::Release);
        let _ = enigo.key(Key::Shift, Direction::Release);
        let _ = enigo.key(Key::Meta, Direction::Release);
    }

    #[cfg(target_os = "windows")]
    let c_key = Key::C;
    #[cfg(not(target_os = "windows"))]
    let c_key = Key::Unicode('c');

    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(25));
    // Every exit path must release Control, otherwise a failed key press
    // leaves it held and every later keystroke is Ctrl-modified.
    let pressed = enigo.key(c_key, Direction::Press);
    let released_key = enigo.key(c_key, Direction::Release);
    let released_control = enigo.key(Key::Control, Direction::Release);
    pressed
        .and(released_key)
        .and(released_control)
        .map_err(|e| e.to_string())
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
    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| e.to_string())?;
    // Release the modifier on every path.
    let clicked = enigo.key(Key::Tab, Direction::Click);
    let released = enigo.key(modifier, Direction::Release);
    clicked.and(released).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_prev_tab() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo
        .key(Key::Control, Direction::Press)
        .map_err(|e| e.to_string())?;
    enigo
        .key(Key::Shift, Direction::Press)
        .map_err(|e| e.to_string())?;
    // Release both modifiers on every path.
    let clicked = enigo.key(Key::Tab, Direction::Click);
    let released_shift = enigo.key(Key::Shift, Direction::Release);
    let released_control = enigo.key(Key::Control, Direction::Release);
    clicked
        .and(released_shift)
        .and(released_control)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_new_tab() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;

    #[cfg(target_os = "windows")]
    {
        // Release lingering modifiers if any physical hotkeys were held
        let _ = enigo.key(Key::Alt, Direction::Release);
        let _ = enigo.key(Key::Shift, Direction::Release);
        let _ = enigo.key(Key::Meta, Direction::Release);
    }

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    #[cfg(target_os = "windows")]
    let t_key = Key::T;
    #[cfg(not(target_os = "windows"))]
    let t_key = Key::Unicode('t');

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(25));
    // Release the modifier on every path: a failed key press would
    // otherwise leave it held and every later keystroke modified.
    let pressed = enigo.key(t_key, Direction::Press);
    std::thread::sleep(std::time::Duration::from_millis(35));
    let released_key = enigo.key(t_key, Direction::Release);
    std::thread::sleep(std::time::Duration::from_millis(25));
    let released_modifier = enigo.key(modifier, Direction::Release);
    pressed
        .and(released_key)
        .and(released_modifier)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_close_tab() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;

    #[cfg(target_os = "windows")]
    {
        // Release lingering modifiers if any physical hotkeys were held
        let _ = enigo.key(Key::Alt, Direction::Release);
        let _ = enigo.key(Key::Shift, Direction::Release);
        let _ = enigo.key(Key::Meta, Direction::Release);
    }

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    #[cfg(target_os = "windows")]
    let w_key = Key::W;
    #[cfg(not(target_os = "windows"))]
    let w_key = Key::Unicode('w');

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| e.to_string())?;
    std::thread::sleep(std::time::Duration::from_millis(25));
    // Release the modifier on every path: a failed key press would
    // otherwise leave it held and every later keystroke modified.
    let pressed = enigo.key(w_key, Direction::Press);
    std::thread::sleep(std::time::Duration::from_millis(35));
    let released_key = enigo.key(w_key, Direction::Release);
    std::thread::sleep(std::time::Duration::from_millis(25));
    let released_modifier = enigo.key(modifier, Direction::Release);
    pressed
        .and(released_key)
        .and(released_modifier)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_scroll_down() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo
        .key(Key::PageDown, Direction::Click)
        .map_err(|e| e.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn press_scroll_up() -> Result<(), String> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|e| format!("failed to init input backend: {e}"))?;
    enigo
        .key(Key::PageUp, Direction::Click)
        .map_err(|e| e.to_string())
}

#[cfg(any(target_os = "macos", test))]
fn mac_app_command(app_name: &str) -> std::process::Command {
    let mut command = std::process::Command::new("open");
    command.args(["-a", app_name]);
    command
}

#[cfg(target_os = "macos")]
pub fn switch_to_app(app_name: &str) -> Result<(), String> {
    match mac_app_command(app_name).status() {
        Ok(status) if status.success() => Ok(()),
        _ => Err(format!("could not switch to application '{app_name}'")),
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_app_command(app_name: &str) -> std::process::Command {
    let mut command = std::process::Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command",
        "if (-not (New-Object -ComObject WScript.Shell).AppActivate($env:OPENDICTATE_TARGET_APP)) { exit 1 }"])
        .env("OPENDICTATE_TARGET_APP", app_name);
    command
}

#[cfg(target_os = "windows")]
pub fn switch_to_app(app_name: &str) -> Result<(), String> {
    match windows_app_command(app_name).status() {
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

    // The pinned opener uses ShellExecute on Windows; no cmd.exe parsing of
    // URL metacharacters, percent expansion, or quoted PowerShell interpolation.
    tauri_plugin_opener::open_url(&target, None::<&str>)
        .map_err(|e| format!("failed to open browser: {e}"))
}
