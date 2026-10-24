#[cfg(target_os = "linux")]
use std::process::Command;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::dictation;
use crate::state::AppState;

pub fn register(app: &AppHandle, state: &AppState, key: &str) -> Result<(), String> {
    let current = state.hotkey.lock().map(|h| h.clone()).unwrap_or(None);
    if current.as_deref() == Some(key) {
        // The key is already bound. On GNOME that binding is the
        // settings-daemon keybinding, so re-syncing it is all that is needed.
        // Elsewhere the OS shortcut must still be asserted, because a failed
        // unregister can leave the platform state out of sync with ours.
        if is_gnome_session() && gnome_schema_available() {
            // A failed re-sync leaves the previous binding in place, which is
            // the same key, so the hotkey keeps working.
            if let Err(error) = sync_gnome_keybinding(key) {
                log::warn!("gnome keybinding re-sync failed: {error}");
            }
            return Ok(());
        }
        register_os_shortcut(app, key)?;
        return Ok(());
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
    let is_gnome = is_gnome_session();
    if is_gnome {
        // The GNOME binding is only the hotkey when the settings-daemon
        // schema is actually usable. Otherwise fall back to the OS grab
        // instead of leaving the user with no hotkey at all.
        if gnome_schema_available() {
            match sync_gnome_keybinding(key) {
                Ok(()) => {
                    log::info!(
                        "GNOME session: relying on settings-daemon keybinding (no X11 grab)"
                    );
                }
                Err(error) => {
                    // A failed sync must not leave the user with no hotkey.
                    // Our settings-daemon slot is removed first so the two
                    // mechanisms cannot both fire for the same key.
                    log::warn!(
                        "GNOME keybinding sync failed ({error}); falling back to the OS hotkey"
                    );
                    remove_gnome_keybinding();
                    register_os_shortcut(app, key)?;
                }
            }
        } else {
            log::warn!(
                "GNOME session: settings-daemon keybindings are unavailable; \
                 falling back to the OS hotkey"
            );
            register_os_shortcut(app, key)?;
        }
    } else {
        register_os_shortcut(app, key)?;

        // Keep the old gesture armed until the new registration succeeds.
        #[cfg(target_os = "macos")]
        crate::doubletap::disarm();

        if let Some(old) = current {
            #[cfg(target_os = "macos")]
            let stale_chord = crate::doubletap::parse(&old).is_none();
            #[cfg(not(target_os = "macos"))]
            let stale_chord = true;
            if stale_chord {
                // A failed unregister leaves both chords live: one press would
                // fire two actions.
                if let Err(e) = app.global_shortcut().unregister(old.as_str()) {
                    log::warn!("could not unregister previous hotkey '{old}': {e}");
                }
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

/// Whether the session is a GNOME one. The settings-daemon keybinding is the
/// only global path there, because an X11 grab cannot see keys while a native
/// Wayland window has focus.
fn is_gnome_session() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|d| d.to_lowercase().contains("gnome"))
        .unwrap_or(false)
}

/// Whether the GNOME settings-daemon keybinding schema can actually be used.
/// A missing `gsettings` binary or schema must not disable the hotkey.
fn gnome_schema_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        gsettings_get(
            "org.gnome.settings-daemon.plugins.media-keys",
            "custom-keybindings",
        )
        .is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Binds the hotkey through the OS global-shortcut API.
fn register_os_shortcut(app: &AppHandle, key: &str) -> Result<(), String> {
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
        .map_err(|e| format!("failed to register hotkey '{key}': {e}"))
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
    if !is_gnome_session() {
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
    // A missing `gsettings` binary or schema must never disable the hotkey.
    let existing = match gsettings_get(schema, "custom-keybindings") {
        Ok(existing) => existing,
        Err(error) => {
            log::warn!("gnome keybinding: settings-daemon unavailable ({error}); skipping");
            return Ok(());
        }
    };
    let mut slots = parse_binding_slots(&existing);
    let ours = normalize_slot(KEYBINDING_PATH);
    if !slots.contains(&ours) {
        slots.push(ours.clone());
    }
    let slot_schema = format!("{schema}.custom-keybinding:{KEYBINDING_PATH}");
    gsettings_set(
        &slot_schema,
        "name",
        &variant_string("OpenDictate: Toggle dictation"),
    )?;
    gsettings_set(&slot_schema, "command", &variant_string(&command))?;
    gsettings_set(&slot_schema, "binding", &variant_string(&binding))?;
    // Re-read immediately before writing: another application may have
    // registered a binding since the first read, and replacing the whole
    // list would silently drop it.
    let current = gsettings_get(schema, "custom-keybindings").unwrap_or(existing);
    let mut merged = parse_binding_slots(&current);
    if !merged.contains(&ours) {
        merged.push(ours);
    }
    for slot in slots {
        if !merged.contains(&slot) {
            merged.push(slot);
        }
    }
    let slot_list = format!(
        "[{}]",
        merged
            .iter()
            .map(|s| variant_string(s))
            .collect::<Vec<_>>()
            .join(", ")
    );
    gsettings_set(schema, "custom-keybindings", &slot_list)?;
    log::info!("gnome keybinding synced: {binding} -> {command}");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn sync_gnome_keybinding(_key: &str) -> Result<(), String> {
    Ok(())
}

/// Removes our settings-daemon keybinding slot. Used when the OS hotkey takes
/// over, so the two mechanisms cannot both fire for the same key.
#[cfg(target_os = "linux")]
fn remove_gnome_keybinding() {
    let schema = "org.gnome.settings-daemon.plugins.media-keys";
    let Ok(existing) = gsettings_get(schema, "custom-keybindings") else {
        return;
    };
    let ours = normalize_slot(KEYBINDING_PATH);
    let slots: Vec<String> = parse_binding_slots(&existing)
        .into_iter()
        .filter(|slot| *slot != ours)
        .collect();
    let slot_list = format!(
        "[{}]",
        slots
            .iter()
            .map(|s| variant_string(s))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if let Err(error) = gsettings_set(schema, "custom-keybindings", &slot_list) {
        log::warn!("could not remove the GNOME keybinding slot: {error}");
    }
}

#[cfg(not(target_os = "linux"))]
fn remove_gnome_keybinding() {}

#[cfg(target_os = "linux")]
fn variant_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(target_os = "linux")]
/// Parses the `custom-keybindings` array.
///
/// A path that cannot be used is skipped rather than fatal: another
/// application may legitimately register a binding whose name contains
/// characters this implementation does not accept, and that must never
/// disable the OpenDictate hotkey.
fn parse_binding_slots(value: &str) -> Vec<String> {
    let value = value
        .trim()
        .strip_prefix("@as")
        .unwrap_or(value.trim())
        .trim();
    let Some(body) = value.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        log::warn!("ignoring unparsable GNOME custom-keybindings list");
        return Vec::new();
    };
    if body.trim().is_empty() {
        return Vec::new();
    }
    body.split(',')
        .filter_map(|item| {
            let item = item.trim();
            let path = item
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
                .or_else(|| item.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                // `gsettings` escapes quotes and backslashes in GVariant
                // string literals.
                .map(|path| path.replace("\\'", "'").replace("\\\\", "\\"))?;
            if !path.starts_with('/')
                || !path
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-'))
            {
                log::warn!("skipping unusable GNOME keybinding path: {path}");
                return None;
            }
            Some(normalize_slot(&path))
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
    // The socket drives microphone capture and text injection, so it must
    // not be reachable by other local users. `bind` uses the process umask,
    // which is typically world-accessible.
    if let Err(e) = restrict_socket_permissions(&path) {
        log::warn!(
            "toggle socket: could not restrict permissions on {}: {e}",
            path.display()
        );
    }
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { continue };
            if let Err(e) = handle_socket_request(&app, &mut conn) {
                log::warn!("toggle socket: request failed: {e}");
            }
        }
    });
    log::info!("toggle socket armed: {}", path.display());
}

/// Restricts the toggle socket to its owner.
#[cfg(unix)]
fn restrict_socket_permissions(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

/// Reads one request from the socket and acts on it. A stream read may return
/// a short count, so the payload is read until EOF or the buffer is full.
#[cfg(unix)]
fn handle_socket_request(
    app: &AppHandle,
    conn: &mut std::os::unix::net::UnixStream,
) -> std::io::Result<()> {
    use std::io::Read;
    let mut buf = Vec::with_capacity(16);
    let mut chunk = [0u8; 16];
    loop {
        let n = conn.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() >= 16 {
            break;
        }
    }
    match buf.as_slice() {
        b"toggle" => toggle_dictation(app),
        b"show" => crate::tray::show_main(app),
        other => {
            log::warn!(
                "toggle socket: ignoring unknown request {:?}",
                String::from_utf8_lossy(other)
            );
        }
    }
    Ok(())
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
        if let Err(e) = conn.write_all(message) {
            log::warn!("toggle socket: failed to send request: {e}");
            return false;
        }
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
        assert!(parse_binding_slots("@as []").is_empty());
        assert!(parse_binding_slots("[]").is_empty());
        assert_eq!(
            parse_binding_slots("['/org/other/binding/', '/org/another/']"),
            ["/org/other/binding/", "/org/another/"]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gnome_unusable_foreign_binding_never_disables_the_hotkey() {
        // A third-party binding whose name contains characters this
        // implementation does not accept must be skipped, not fatal.
        assert_eq!(
            parse_binding_slots(
                "['/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/com.example.App/']"
            ),
            Vec::<String>::new()
        );
        // The usable bindings around it are still discovered.
        assert_eq!(
            parse_binding_slots("['/org/ok/one/', '/org/bad/with.dots/', '/org/ok/two/']"),
            ["/org/ok/one/", "/org/ok/two/"]
        );
        // A malformed list is skipped instead of aborting the whole sync.
        assert!(parse_binding_slots("[not-a-path]").is_empty());
    }
}
