use std::process::Command;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::dictation;
use crate::state::AppState;

pub fn register(app: &AppHandle, state: &AppState, key: &str) -> Result<(), String> {
    let current = state
        .hotkey
        .lock()
        .map(|h| h.clone())
        .unwrap_or(None);
    if current.as_deref() == Some(key) {
        sync_gnome_keybinding(key);
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
    #[cfg(target_os = "macos")]
    crate::doubletap::disarm();

    // On GNOME (X11 or Wayland) the settings-daemon custom keybinding is the
    // single global path: the X11 grab would double-fire alongside it and
    // toggle start+stop in one press. Elsewhere (bare X11, other DEs) fall
    // back to the X11 grab.
    let is_gnome = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|d| d.to_lowercase().contains("gnome"))
        .unwrap_or(false);
    if is_gnome {
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
    sync_gnome_keybinding(key);
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

/// Serializes all dictation start/stop entry points (hold-to-talk press and
/// release, hotkey toggle). Without this, a quick press→release could run the
/// start and stop checks concurrently: the stop thread would read
/// `user_dictation_active` before the start thread sets it, drop the stop,
/// and leave dictation wedged on. With the lock, the release path waits for
/// the press path to finish and then re-checks the state.
static DICTATION_SERIALIZER: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn dictation_guard() -> std::sync::MutexGuard<'static, ()> {
    DICTATION_SERIALIZER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

pub fn start_dictation(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        let _serial = dictation_guard();
        // When handsfree is active it holds the recorder open, so
        // is_recording() is always true. Only skip start if the user already
        // has an active dictation session (user_dictation_active = true).
        let user_session_open = state.user_dictation_active.load(std::sync::atomic::Ordering::SeqCst);
        if !state.recorder.is_recording() || !user_session_open {
            log::info!("hold-to-talk: starting dictation");
            if let Err(e) = dictation::start(&app, &state, false) {
                log::error!("hold-to-talk start failed: {e}");
            }
        }
    });
}

pub fn stop_dictation(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(state) = app.try_state::<AppState>() else { return };
        // Wait for any in-flight start/stop to complete, then re-check the
        // session state — see DICTATION_SERIALIZER.
        let _serial = dictation_guard();
        // Only stop when a user session is genuinely open. Handsfree holds
        // the recorder open on its own — stopping it without a user session
        // would submit handsfree's ambient audio as a dictation result.
        let user_session_open = state.user_dictation_active.load(std::sync::atomic::Ordering::SeqCst);
        if state.recorder.is_recording() && user_session_open {
            log::info!("hold-to-talk: stopping dictation");
            if let Err(e) = dictation::stop(&app, &state) {
                log::error!("hold-to-talk stop failed: {e}");
            }
        }
    });
}


/// Serializes hotkey toggles. The global-shortcut handler runs on the MAIN
/// thread on macOS (global-hotkey delivers via NSEvent monitor), and
/// dictation::start/stop perform CoreAudio setup, notification calls and
/// first-use engine loads that can block for seconds — running them inline
/// froze the whole app. The work now happens on a worker thread; while one
/// toggle is in flight, further presses are ignored (same net effect as the
/// old main-thread serialization, minus the freeze).
static TOGGLE_INFLIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn toggle_dictation(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    if TOGGLE_INFLIGHT.swap(true, Ordering::SeqCst) {
        log::info!("toggle already in flight; ignoring");
        return;
    }
    let app = app.clone();
    let fallback = app.clone();
    let spawned = std::thread::Builder::new()
        .name("opendictate-toggle".to_string())
        .spawn(move || {
            toggle_dictation_sync(&app);
            TOGGLE_INFLIGHT.store(false, Ordering::SeqCst);
        });
    if spawned.is_err() {
        // Worker unavailable: run inline so the hotkey never goes dead.
        toggle_dictation_sync(&fallback);
        TOGGLE_INFLIGHT.store(false, Ordering::SeqCst);
    }
}

fn toggle_dictation_sync(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    // Share the hold-to-talk serializer so a hotkey toggle cannot interleave
    // with a hold-to-talk press/release mid-flight.
    let _serial = dictation_guard();
    let user_session_open = state.user_dictation_active.load(std::sync::atomic::Ordering::SeqCst);

    if user_session_open {
        log::info!("toggle: stopping dictation");
        if let Err(e) = dictation::stop(app, &state) {
            log::error!("toggle: stop failed: {e}");
            crate::notify::notify("Dictation error", &format!("Stop failed: {e}"));
            let _ = app.emit(
                "dictation-error",
                serde_json::json!({ "message": format!("stop failed: {e}") }),
            );
        }
    } else {
        log::info!("toggle: starting dictation");
        if let Err(e) = dictation::start(app, &state, false) {
            log::error!("toggle: start failed: {e}");
            crate::notify::notify("Dictation error", &format!("Start failed: {e}"));
            let _ = app.emit(
                "dictation-error",
                serde_json::json!({ "message": format!("start failed: {e}") }),
            );
        }
    }
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
pub fn sync_gnome_keybinding(key: &str) {
    let is_gnome = std::env::var("XDG_CURRENT_DESKTOP")
        .map(|d| d.to_lowercase().contains("gnome"))
        .unwrap_or(false);
    if !is_gnome {
        return;
    }
    let Some(toggle_script) = toggle_script_path() else {
        log::warn!("gnome keybinding: could not resolve toggle script path");
        return;
    };
    let Some(binding) = gnome_accelerator(key) else {
        log::warn!("gnome keybinding: could not parse hotkey '{key}'");
        return;
    };

    let schema = "org.gnome.settings-daemon.plugins.media-keys";
    let Some(existing) = gsettings_get(schema, "custom-keybindings") else {
        log::debug!("gnome keybinding: schema unavailable, skipping");
        return;
    };

    let mut slots: Vec<String> = existing
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .filter_map(|s| {
            let s = s.trim().trim_matches('\'');
            if s.is_empty() {
                None
            } else {
                Some(normalize_slot(s))
            }
        })
        .collect();
    let ours = normalize_slot(KEYBINDING_PATH);
    if !slots.iter().any(|s| s == &ours) {
        slots.push(ours);
    }
    let slot_list = format!(
        "[{}]",
        slots
            .iter()
            .map(|s| format!("'{s}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    gsettings_set(schema, "custom-keybindings", &slot_list);
    let slot_schema = format!("{schema}.custom-keybinding:{KEYBINDING_PATH}");
    gsettings_set(&slot_schema, "name", "OpenDictate: Toggle dictation");
    gsettings_set(&slot_schema, "command", &toggle_script);
    gsettings_set(&slot_schema, "binding", &binding);
    log::info!("gnome keybinding synced: {binding} -> {toggle_script}");
}

#[cfg(not(target_os = "linux"))]
pub fn sync_gnome_keybinding(_key: &str) {}

#[cfg(target_os = "linux")]
fn toggle_script_path() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent();
    for _ in 0..6 {
        let candidate = dir?.join("scripts").join("opendictate-toggle.sh");
        if candidate.exists() {
            return Some(candidate.to_string_lossy().into_owned());
        }
        dir = dir?.parent();
    }
    None
}

#[cfg(target_os = "linux")]
fn gnome_accelerator(key: &str) -> Option<String> {
    if key.trim().is_empty() {
        return None;
    }
    let mut accel = String::new();
    let mut key_part: Option<String> = None;
    for part in key.split('+') {
        match part {
            "ctrl" | "control" => accel.push_str("<Control>"),
            "alt" => accel.push_str("<Alt>"),
            "shift" => accel.push_str("<Shift>"),
            "super" | "meta" | "cmd" | "win" => accel.push_str("<Super>"),
            "space" => key_part = Some("space".to_string()),
            other => {
                let lower = other.to_lowercase();
                key_part = Some(match lower.as_str() {
                    "f1" | "f2" | "f3" | "f4" | "f5" | "f6" | "f7" | "f8" | "f9"
                    | "f10" | "f11" | "f12" => other.to_uppercase(),
                    "up" | "down" | "left" | "right" => {
                        other[..1].to_uppercase() + &other[1..]
                    }
                    _ => lower,
                });
            }
        }
    }
    key_part.map(|k| format!("{accel}{k}"))
}

#[cfg(target_os = "linux")]
fn gsettings_get(schema: &str, key: &str) -> Option<String> {
    let out = Command::new("gsettings").args(["get", schema, key]).output().ok()?;
    String::from_utf8(out.stdout).ok().map(|s| s.trim().to_string())
}

#[cfg(target_os = "linux")]
fn normalize_slot(path: &str) -> String {
    format!("{}/", path.trim_end_matches('/'))
}

#[cfg(target_os = "linux")]
fn gsettings_set(schema: &str, key: &str, value: &str) {
    let out = Command::new("gsettings")
        .args(["set", schema, key, value])
        .output();
    match out {
        Ok(o) if o.status.success() => {}
        Ok(o) => log::warn!(
            "gsettings set {schema} {key} failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => log::warn!("gsettings set {schema} {key} failed: {e}"),
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
        let _ = conn.write_all(b"show");
        true
    } else {
        false
    }
}

#[cfg(not(unix))]
pub fn is_another_instance(_path: &std::path::Path) -> bool {
    false
}
