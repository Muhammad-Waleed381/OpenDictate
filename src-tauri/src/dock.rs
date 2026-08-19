use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

pub const DOCK_SIZE: f64 = 29.0;
const MARGIN: f64 = 16.0;
const TOLERANCE: i32 = 2;

static LAST_ASSERT_MS: AtomicU64 = AtomicU64::new(0);
static CAPTION_WIDTH: AtomicU32 = AtomicU32::new(0);
static LAST_SHAPED: AtomicU32 = AtomicU32::new(0);

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("dock")
}

/// Width of the dock window: caption strip while streaming, DOCK_SIZE otherwise.
fn current_width() -> u32 {
    let w = CAPTION_WIDTH.load(Ordering::Relaxed);
    if w > 0 {
        w
    } else {
        DOCK_SIZE as u32
    }
}

fn bottom_right(win: &WebviewWindow, width: u32, height: u32) -> PhysicalPosition<i32> {
    let default = PhysicalPosition { x: 100, y: 16 };
    let Some(monitor) = win.current_monitor().ok().flatten() else {
        return default;
    };
    let size = monitor.size();
    let x = (size.width.saturating_sub(width) as f64 - MARGIN).max(0.0) as i32;
    let y = (size.height.saturating_sub(height) as f64 - MARGIN).max(0.0) as i32;
    PhysicalPosition { x, y }
}

pub fn init(app: &AppHandle) {
    if let Some(win) = window(app) {
        let _ = win.set_always_on_top(true);
        let _ = win.show();
    }
    #[cfg(target_os = "linux")]
    shape_input(app);
    enforce(app);
}

/// Restricts the dock's input region to the bottom strip where the content
/// renders, so the transparent upper part of the window doesn't intercept
/// clicks. Must run on the main thread.
#[cfg(target_os = "linux")]
fn shape_input(app: &AppHandle) {
    use gtk::cairo::{RectangleInt, Region};
    use gtk::prelude::*;

    let Some(win) = window(app) else { return };
    let Ok(vbox) = win.default_vbox() else { return };

    let mut widget: Option<gtk::Widget> = Some(vbox.upcast());
    while let Some(current) = widget {
        if let Ok(gtk_window) = current.clone().downcast::<gtk::Window>() {
            let (w, h) = gtk_window.size();
            if w <= 0 || h <= 0 {
                return;
            }
            let key = ((w as u32) << 16) | (h as u32);
            if LAST_SHAPED.load(Ordering::Relaxed) == key {
                return;
            }
            let strip = DOCK_SIZE as i32;
            let rect = RectangleInt::new(0, h - strip, w, strip);
            let region = Region::create_rectangle(&rect);
            gtk_window.input_shape_combine_region(Some(&region));
            LAST_SHAPED.store(key, Ordering::Relaxed);
            log::info!("dock: input shape {w}x{h} bottom {strip}px");
            return;
        }
        widget = current.parent();
    }
}

pub fn ensure(app: &AppHandle) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    if now_ms.saturating_sub(LAST_ASSERT_MS.load(Ordering::Relaxed)) < 400 {
        return;
    }
    LAST_ASSERT_MS.store(now_ms, Ordering::Relaxed);
    apply_dock_size(app);
}

/// Parks the dock at the bottom-right corner. The window size is constrained
/// by the config (min/max 210x29) and WebKit's natural height request; the
/// pill content is bottom-aligned so it hugs the corner regardless.
fn apply_dock_size(app: &AppHandle) {
    if let Some(win) = window(app) {
        let width = current_width();
        let _ = win.set_always_on_top(true);
        let size = win.outer_size().ok().unwrap_or(PhysicalSize {
            width,
            height: DOCK_SIZE as u32,
        });
        let target = bottom_right(&win, size.width, size.height);
        let placed = win
            .outer_position()
            .ok()
            .map(|p| (p.x - target.x).abs() <= TOLERANCE && (p.y - target.y).abs() <= TOLERANCE)
            .unwrap_or(false);
        if !placed {
            let _ = win.set_position(target);
        }
    }
}

/// Fixed width of the caption strip; the pill truncates long text.
const CAPTION_STRIP_WIDTH: u32 = 210;

fn emit_dock_event(app: &AppHandle, event: &str, payload: serde_json::Value) {
    let event = event.to_string();
    let payload = serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_string());
    let app = app.clone();
    let inner = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = window(&inner) else { return };
        let script = format!(
            "window.dispatchEvent(new CustomEvent({event:?}, {{ detail: {payload} }}));",
            event = format!("opendictate:{event}"),
        );
        let _ = win.eval(&script);
    });
}

/// Shows a live caption strip in the dock while streaming; pass `None` to
/// return the dock to its idle state (the window stays visible so the user
/// always has the on-screen mic control).
pub fn set_caption(app: &AppHandle, text: Option<&str>) {
    match text.map(str::trim).filter(|t| !t.is_empty()) {
        Some(text) => {
            CAPTION_WIDTH.store(CAPTION_STRIP_WIDTH, Ordering::Relaxed);
            log::info!("dock: caption set: {text:?}");
            emit_dock_event(
                app,
                "partial",
                serde_json::json!({ "text": text, "streaming": true }),
            );
        }
        None => {
            CAPTION_WIDTH.store(0, Ordering::Relaxed);
            log::info!("dock: caption cleared");
            emit_dock_event(
                app,
                "partial",
                serde_json::json!({ "text": "", "streaming": false }),
            );
        }
    }
    apply_dock_size(app);
}

pub fn ensure_on_main(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    {
        stick_to_all_workspaces(app);
        shape_input(app);
    }
    ensure(app);
}

fn enforce(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        for _ in 0..12 {
            ensure(&app);
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
        std::thread::sleep(std::time::Duration::from_millis(2000));
        if let Some(win) = window(&app) {
            let size = win.outer_size().ok().unwrap_or(PhysicalSize {
                width: CAPTION_STRIP_WIDTH,
                height: DOCK_SIZE as u32,
            });
            let monitor = win
                .current_monitor()
                .ok()
                .flatten()
                .map(|m| *m.size());
            let pos = win.outer_position().ok();
            log::info!("dock window: size={size:?} pos={pos:?} monitor={monitor:?}");
        }
    });
}

#[cfg(target_os = "linux")]
fn stick_to_all_workspaces(app: &AppHandle) {
    use gtk::prelude::*;

    let Some(win) = window(app) else { return };
    let Ok(vbox) = win.default_vbox() else { return };

    let mut widget: Option<gtk::Widget> = Some(vbox.upcast());
    while let Some(current) = widget {
        if let Ok(gtk_window) = current.clone().downcast::<gtk::Window>() {
            gtk_window.stick();
            return;
        }
        widget = current.parent();
    }
}

pub fn set_state(app: &AppHandle, status: &str, message: Option<&str>) {
    let payload = serde_json::json!({
        "state": status,
        "message": message,
    });
    emit_dock_event(app, "overlay-state", payload.clone());
    let _ = app.emit(
        "overlay-state",
        payload,
    );
    crate::tray::apply_state_icon(app, status);
}
