use std::process::Command;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::dictation;
use crate::state::AppState;

pub fn register(app: &AppHandle, state: &AppState, key: &str) -> Result<(), String> {
    let current = state.hotkey.lock().map(|h| h.clone()).unwrap_or(None);
    if current.as_deref() == Some(key) {
        return sync_gnome_keybinding(key);
    }

    // Double-tap gestures bypass the OS hotkey API entirely — see doubletap.rs
    // for why Carbon cannot express them.
    #[cfg(target_os = "macos")]
    if let Some(modifier) = crate::doubletap::parse(key) {
        if let Some(old) = current {
            if crate::doubletap::parse(&old).is_none() {
                let _ = app.global_shortcut().unregister(old.as_str());
            }
        }
        crate::doubletap::arm(app, modifier);
        if let Ok(mut current) = state.hotkey.lock() {
            *current = Some(key.to_string());
        }
        log::info!("hotkey registered: {key} (double-tap gesture)");
        return Ok(());
    }
    // On GNOME (X11 or Wayland) the settings-daemon custom keybinding is the
    // single global path: the X11 grab would double-fire alongside it and
    // toggle start+stop in one press. Elsewhere (bare X11, other DEs) fall
    // back to the X11 grab.
    let is_gnome = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|d| d.to_lowercase().contains("gnome"))
        .unwrap_or(false);
    if is_gnome {
        sync_gnome_keybinding(key)?;
        log::info!("GNOME session: relying on settings-daemon keybinding (no X11 grab)");
    } else {
        app.global_shortcut()
            .on_shortcut(key, move |app, _shortcut, event| {
                let hold = app
                    .try_state::<AppState>()
                    .and_then(|s| s.settings.lock().ok().map(|st| st.hold_to_talk))
                    .unwrap_or(false);
                if hold {
                    if event.state() == ShortcutState::Pressed {
                        start_dictation(app);
                    } else if event.state() == ShortcutState::Released {
                        stop_dictation(app);
                    }
                } else if event.state() == ShortcutState::Pressed {
                    toggle_dictation(app);
                }
            })
            .map_err(|e| format!("failed to register hotkey '{key}': {e}"))?;

        // Keep the old gesture armed until the new registration succeeds.
        #[cfg(target_os = "macos")]
        crate::doubletap::disarm();

        if let Some(old) = current {
            #[cfg(target_os = "macos")]
            let stale_chord = crate::doubletap::parse(&old).is_none();
            #[cfg(not(target_os = "macos"))]
            let stale_chord = true;
            if stale_chord {
                let _ = app.global_shortcut().unregister(old.as_str());
            }
        }
    }

    if let Ok(mut current) = state.hotkey.lock() {
        *current = Some(key.to_string());
    }
    log::info!("hotkey registered: {key}");
    if is_gnome {
        // The settings-daemon custom keybinding runs a toggle script: the
        // shell consumes the key and only a toggle action reaches the app.
        // There is no press/release event stream, so hold-to-talk cannot
        // work on the GNOME path — surface that instead of failing silently.
        let hold = state
            .settings
            .lock()
            .map(|s| s.hold_to_talk)
            .unwrap_or(false);
        if hold {
            log::warn!(
                "GNOME session: hold-to-talk is not supported by the settings-daemon \
                 keybinding (press/release events are unavailable); the hotkey toggles \
                 dictation instead"
            );
        }
    }
    Ok(())
}

/// A single receiver preserves callback order. A mutex acquired by freshly
/// spawned threads does not: release can acquire it before press.
fn ordered_worker<T: Send + 'static>(
    mut consume: impl FnMut(T) + Send + 'static,
) -> Result<std::sync::mpsc::Sender<T>, String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("opendictate-hotkeys".into())
        .spawn(move || {
            for request in receiver {
                consume(request);
            }
        })
        .map_err(|e| format!("could not start hotkey worker: {e}"))?;
    Ok(sender)
}

#[derive(Clone, Copy)]
enum DictationRequest {
    Start,
    Stop,
    Toggle,
}

type RequestSender = std::sync::mpsc::Sender<(AppHandle, DictationRequest)>;
static DICTATION_QUEUE: std::sync::OnceLock<Result<RequestSender, String>> =
    std::sync::OnceLock::new();

fn enqueue(app: &AppHandle, request: DictationRequest) {
    let queue = DICTATION_QUEUE
        .get_or_init(|| ordered_worker(|(app, request)| handle_request(&app, request)));
    let result = match queue {
        Ok(sender) => sender
            .send((app.clone(), request))
            .map_err(|e| e.to_string()),
        Err(error) => Err(error.clone()),
    };
    if let Err(error) = result {
        report_error(app, &error);
    }
}

pub fn start_dictation(app: &AppHandle) {
    enqueue(app, DictationRequest::Start);
}
pub fn stop_dictation(app: &AppHandle) {
    enqueue(app, DictationRequest::Stop);
}
pub fn toggle_dictation(app: &AppHandle) {
    enqueue(app, DictationRequest::Toggle);
}

fn handle_request(app: &AppHandle, request: DictationRequest) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let active = state
        .user_dictation_active
        .load(std::sync::atomic::Ordering::SeqCst);
    let result = match request {
        DictationRequest::Start if !active => dictation::start(app, &state, false),
        DictationRequest::Stop if active => dictation::stop(app, &state).map(|_| ()),
        DictationRequest::Toggle if active => dictation::stop(app, &state).map(|_| ()),
        DictationRequest::Toggle => dictation::start(app, &state, false),
        _ => Ok(()),
    };
    if let Err(error) = result {
        report_error(app, &error);
    }
}

fn report_error(app: &AppHandle, error: &str) {
    log::error!("hotkey dictation failed: {error}");
    crate::notify::notify("Dictation error", error);
    let _ = app.emit("dictation-error", serde_json::json!({"message": error}));
}

// ---------------------------------------------------------------------------
// GNOME fallback: on Wayland the X11 grab never sees keys while a native
// Wayland window has focus, so we mirror the hotkey into a settings-daemon
// custom keybinding; the shell consumes the key globally and runs
// scripts/opendictate-toggle.sh, which toggles over the app's unix socket.
// ---------------------------------------------------------------------------

const KEYBINDING_PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/opendictate/";

#[cfg(target_os = "linux")]
pub fn sync_gnome_keybinding(key: &str) -> Result<(), String> {
    let is_gnome = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|d| d.to_lowercase().contains("gnome"))
        .unwrap_or(false);
    if !is_gnome {
        return Ok(());
    }
    let executable = crate::autostart::launcher_executable()?;
    let command = format!(
        "'{}' --toggle",
        executable.to_string_lossy().replace('\'', "'\\''")
    );
    let binding =
        gnome_accelerator(key).ok_or_else(|| format!("could not parse hotkey '{key}'"))?;
    let schema = "org.gnome.settings-daemon.plugins.media-keys";
    let existing = gsettings_get(schema, "custom-keybindings")?;
    let mut slots = parse_binding_slots(&existing)?;
    let ours = normalize_slot(KEYBINDING_PATH);
    if !slots.contains(&ours) {
        slots.push(ours);
    }
    let slot_list = format!(
        "[{}]",
        slots
            .iter()
            .map(|s| variant_string(s))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let slot_schema = format!("{schema}.custom-keybinding:{KEYBINDING_PATH}");
    gsettings_set(
        &slot_schema,
        "name",
        &variant_string("OpenDictate: Toggle dictation"),
    )?;
    gsettings_set(&slot_schema, "command", &variant_string(&command))?;
    gsettings_set(&slot_schema, "binding", &variant_string(&binding))?;
    gsettings_set(schema, "custom-keybindings", &slot_list)?;
    log::info!("gnome keybinding synced: {binding} -> {command}");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn sync_gnome_keybinding(_key: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "linux")]
fn variant_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(target_os = "linux")]
fn parse_binding_slots(value: &str) -> Result<Vec<String>, String> {
    let value = value
        .trim()
        .strip_prefix("@as")
        .unwrap_or(value.trim())
        .trim();
    let body = value
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or("invalid GNOME custom-keybindings list")?;
    if body.trim().is_empty() {
        return Ok(Vec::new());
    }
    body.split(',')
        .map(|item| {
            let item = item.trim();
            let path = item
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
                .or_else(|| item.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                .ok_or("invalid GNOME keybinding path")?;
            if !path.starts_with('/')
                || !path
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-'))
            {
                return Err("invalid GNOME keybinding path".into());
            }
            Ok(normalize_slot(path))
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn gnome_accelerator(key: &str) -> Option<String> {
    if key.trim().is_empty() {
        return None;
    }
    let mut accel = String::new();
    let mut key_part: Option<String> = None;
    for part in key.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => accel.push_str("<Control>"),
            "alt" => accel.push_str("<Alt>"),
            "shift" => accel.push_str("<Shift>"),
            "super" | "meta" | "cmd" | "win" => accel.push_str("<Super>"),
            "space" => key_part = Some("space".to_string()),
            other => {
                let lower = other.to_lowercase();
                key_part = Some(match lower.as_str() {
                    "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10"
                    | "f11" | "f12" => other.to_uppercase(),
                    "up" | "down" | "left" | "right" => other[..1].to_uppercase() + &other[1..],
                    _ => lower,
                });
            }
        }
    }
    key_part.map(|k| format!("{accel}{k}"))
}

#[cfg(target_os = "linux")]
fn gsettings_get(schema: &str, key: &str) -> Result<String, String> {
    let out = Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .map_err(|e| format!("gsettings unavailable: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    String::from_utf8(out.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
fn normalize_slot(path: &str) -> String {
    format!("{}/", path.trim_end_matches('/'))
}

#[cfg(target_os = "linux")]
fn gsettings_set(schema: &str, key: &str, value: &str) -> Result<(), String> {
    let out = Command::new("gsettings")
        .args(["set", schema, key, value])
        .output()
        .map_err(|e| format!("gsettings set {key} failed: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "gsettings set {key} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

#[cfg(unix)]
pub fn install_socket_toggle(app: AppHandle, path: std::path::PathBuf) {
    let _ = std::fs::remove_file(&path);
    let listener = match std::os::unix::net::UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("toggle socket: failed to bind {}: {e}", path.display());
            return;
        }
    };
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { continue };
            use std::io::Read;
            let mut buf = [0u8; 16];
            let n = conn.read(&mut buf).unwrap_or(0);
            match &buf[..n] {
                b"toggle" => toggle_dictation(&app),
                b"show" => crate::tray::show_main(&app),
                _ => {}
            }
        }
    });
    log::info!("toggle socket armed: {}", path.display());
}

/// Single-instance guard over the toggle socket. Unix only; other platforms
/// have no socket yet, so a second instance is allowed to proceed.
#[cfg(unix)]
pub fn is_another_instance(path: &std::path::Path) -> bool {
    use std::io::Write;
    if let Ok(mut conn) = std::os::unix::net::UnixStream::connect(path) {
        let message: &[u8] = if std::env::args().any(|arg| arg == "--toggle") {
            b"toggle"
        } else {
            b"show"
        };
        let _ = conn.write_all(message);
        true
    } else {
        false
    }
}

#[cfg(not(unix))]
pub fn is_another_instance(_path: &std::path::Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_press_release_requests_stay_in_callback_order() {
        let (completed, received) = std::sync::mpsc::channel();
        let worker = ordered_worker(move |value| {
            if value == 0 {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            completed.send(value).unwrap();
        })
        .unwrap();
        for n in 0..100 {
            worker.send(n).unwrap();
        }
        drop(worker);
        let events: Vec<_> = received.iter().collect();
        assert_eq!(events, (0..100).collect::<Vec<_>>());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gnome_empty_typed_array_does_not_create_a_bogus_slot() {
        assert!(parse_binding_slots("@as []").unwrap().is_empty());
        assert!(parse_binding_slots("[]").unwrap().is_empty());
        assert_eq!(
            parse_binding_slots("['/org/other/binding/', '/org/another/']").unwrap(),
            ["/org/other/binding/", "/org/another/"]
        );
        assert!(parse_binding_slots("[not-a-path]").is_err());
    }
}
