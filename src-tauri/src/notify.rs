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
