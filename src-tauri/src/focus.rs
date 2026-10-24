//! Keeps OpenDictate from holding the keyboard focus or the application
//! activation while the user is working in another application.
//!
//! The dock — the on-screen microphone overlay — is a persistent HUD and has
//! to behave like one:
//!
//! * **macOS.** Showing a window calls `makeKeyAndOrderFront:`, which
//!   *activates* the owning application. The dock cannot become the key
//!   window because it is not focusable, so macOS leaves the previously
//!   focused application holding the key window while OpenDictate becomes
//!   the frontmost application. Switching spaces only moves the key window
//!   when the frontmost application changes, so the key window stayed on the
//!   space the user had left: `Ctrl+W` and `Alt+F4` then closed the
//!   application on the *other* space.
//! * **Linux.** The dock lives on every workspace (`gtk_window_stick()`).
//!   A sticky window that holds the keyboard focus keeps it across workspace
//!   switches, so the focus would not follow the user to the new workspace.
//!   The dock is therefore configured never to accept the focus
//!   (see [`crate::dock`]), and this module reports it if that ever stops
//!   being true.
use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{AppHandle, Manager};

/// Whether the OpenDictate main window currently holds the keyboard focus.
///
/// The main window is the only OpenDictate window that can take the focus;
/// the dock is deliberately not focusable. When the main window holds the
/// focus the user is deliberately working inside OpenDictate and the
/// activation is theirs to keep.
#[cfg(target_os = "macos")]
fn main_window_has_focus(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|win| win.is_focused())
        .unwrap_or(false)
}

/// Gives the application activation back to the user's application when the
/// dock has taken it.
pub fn ensure_external_focus(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    {
        if main_window_has_focus(app) {
            return;
        }
        // AppKit calls must run on the main thread.
        let _ = app.run_on_main_thread(move || {
            use objc2::MainThreadMarker;
            use objc2_app_kit::NSApplication;

            let Some(marker) = MainThreadMarker::new() else {
                return;
            };
            let application = NSApplication::sharedApplication(marker);
            if application.isActive() {
                log::info!(
                    "focus: releasing the activation the dock took from the frontmost application"
                );
                // Resigning the active status returns the key window to the
                // application the user is working in. The dock stays visible:
                // it is a floating window of an inactive application.
                application.deactivate();
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        // On Linux the dock is configured never to accept the keyboard focus.
        // A focused dock is a sticky window holding the focus, which keeps
        // the focus on the previous workspace, so hand the focus back.
        if let Some(win) = app.get_webview_window("dock") {
            if win.is_focused().unwrap_or(false) {
                log::warn!(
                    "focus: the dock holds the keyboard focus; handing it back to the window manager"
                );
                release_dock_focus(&win);
            }
        }
    }
}

/// Hides and re-shows the dock so the window manager restores the focus to
/// the application the user is working in.
///
/// Hiding the focused window makes the window manager focus another one,
/// and the dock is configured never to take the focus back when it is
/// re-shown. This is the last-resort path: the dock is normally prevented
/// from taking the focus in the first place.
#[cfg(not(target_os = "macos"))]
fn release_dock_focus(win: &tauri::WebviewWindow) {
    use std::sync::Mutex;
    use std::time::Instant;

    static LAST_RELEASE: Mutex<Option<Instant>> = Mutex::new(None);

    let now = Instant::now();
    {
        let Ok(mut last) = LAST_RELEASE.lock() else { return };
        // Do not fight the window manager in a loop.
        if last.is_some_and(|last| now.duration_since(last) < Duration::from_secs(3)) {
            return;
        }
        *last = Some(now);
    }
    let _ = win.hide();
    std::thread::sleep(Duration::from_millis(60));
    let _ = win.show();
}

/// Watches for OpenDictate taking the activation or the keyboard focus and
/// hands it straight back.
///
/// Only active while a dictation session is running: that is when the dock
/// is updated on every caption frame and can steal the activation.
pub fn start_watchdog(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("opendictate-focus".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_millis(500));
            let session_active = app
                .try_state::<crate::state::AppState>()
                .map(|state| {
                    state.user_dictation_active.load(Ordering::SeqCst)
                        || state.handsfree_active.load(Ordering::SeqCst)
                })
                .unwrap_or(false);
            if session_active {
                ensure_external_focus(&app);
            }
        })
        .ok();
}
