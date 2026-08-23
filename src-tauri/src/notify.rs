/// Shows a desktop notification. This is the primary feedback channel for
/// hotkey toggles because the dock strip can be covered by maximized or
/// fullscreen apps (e.g. Electron windows on Wayland).
///
/// Linux shells out to `notify-send`/`gdbus`; Windows and macOS use
/// `notify-rust`, which wraps the native toast / NSUserNotification APIs.
#[cfg(target_os = "linux")]
pub fn notify(summary: &str, body: &str) {
    use std::process::Command;
    let ok = Command::new("notify-send")
        .args([
            "-a",
            "OpenDictate",
            "-i",
            "audio-input-microphone",
            "-t",
            "4000",
            summary,
            body,
        ])
        .status()
        .is_ok_and(|s| s.success());
    if ok {
        return;
    }
    let _ = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.Notifications",
            "--object-path",
            "/org/freedesktop/Notifications",
            "--method",
            "org.freedesktop.Notifications.Notify",
            "OpenDictate",
            "0",
            "",
            summary,
            body,
            "[]",
            "{}",
            "4000",
        ])
        .status();
}

#[cfg(not(target_os = "linux"))]
pub fn notify(summary: &str, body: &str) {
    let _ = notify_rust::Notification::new()
        .summary(summary)
        .body(body)
        .appname("OpenDictate")
        .timeout(notify_rust::Timeout::Milliseconds(4000))
        .show();
}

/// Claims the notification bundle identifier before any notification is sent.
///
/// notify-rust resolves one lazily on first use via `ensure_application_set`,
/// which runs the AppleScript `get id of application "use_default"`. No app is
/// named that, and AppleScript answers an unresolvable application name by
/// opening a blocking "Choose Application — Where is use_default?" picker over
/// the app. `set_application` is a `call_once`, so claiming the identifier here
/// wins that race and the AppleScript never runs.
///
/// Takes the identifier from tauri.conf.json so the two cannot drift. An
/// unbundled `tauri dev` binary is not registered with LaunchServices, so this
/// reports Err there and notifications stay silent — still far better than a
/// modal picker on every dictation. Bundled builds resolve normally.
#[cfg(target_os = "macos")]
pub fn init(bundle_id: &str) {
    if let Err(e) = notify_rust::set_application(bundle_id) {
        log::warn!("desktop notifications unavailable ({bundle_id}): {e}");
    }
}

#[cfg(not(target_os = "macos"))]
pub fn init(_bundle_id: &str) {}
