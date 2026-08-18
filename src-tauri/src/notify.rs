use std::process::Command;

/// Shows a GNOME notification. This is the primary feedback channel for
/// hotkey toggles because the dock strip can be covered by maximized or
/// fullscreen apps (e.g. Electron windows on Wayland).
pub fn notify(summary: &str, body: &str) {
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
