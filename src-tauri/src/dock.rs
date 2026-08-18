use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Size, WebviewWindow};

pub const DOCK_SIZE: f64 = 29.0;
const MARGIN: f64 = 16.0;
const TOLERANCE: i32 = 2;

static LAST_ASSERT_MS: AtomicU64 = AtomicU64::new(0);
static CAPTION_WIDTH: AtomicU32 = AtomicU32::new(0);

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

fn bottom_right(app: &AppHandle, width: u32, height: u32) -> PhysicalPosition<i32> {
    let default = PhysicalPosition { x: 100, y: 16 };
    let Some(win) = window(app) else {
        return default;
    };
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
    enforce(app);
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

/// Resizes the dock to the caption strip width (or back to the round button)
/// and parks it at the bottom-right corner. Tauri window calls only — GTK
/// level adjustments happen via `shrink_to_min` on the main thread.
fn apply_dock_size(app: &AppHandle) {
    if let Some(win) = window(app) {
        let width = current_width();
        let _ = win.set_always_on_top(true);
        let _ = win.set_min_size(Some(PhysicalSize {
            width: DOCK_SIZE as u32,
            height: DOCK_SIZE as u32,
        }));
        let _ = win.set_size(Size::Physical(PhysicalSize {
            width,
            height: DOCK_SIZE as u32,
        }));
        let size = win.outer_size().ok().unwrap_or(PhysicalSize {
            width,
            height: DOCK_SIZE as u32,
        });
        let target = bottom_right(app, size.width, size.height);
        let placed = win
            .outer_position()
            .ok()
            .map(|p| (p.x - target.x).abs() <= TOLERANCE && (p.y - target.y).abs() <= TOLERANCE)
            .unwrap_or(false);
        if !placed {
            let _ = win.set_position(target);
        }
        let _ = win.show();
    }
}

/// Fixed width of the caption strip; the pill truncates long text.
const CAPTION_STRIP_WIDTH: u32 = 210;

/// Shows a live caption strip in the dock while streaming; pass `None` to
/// collapse back to the round button.
pub fn set_caption(app: &AppHandle, text: Option<&str>) {
    match text.map(str::trim).filter(|t| !t.is_empty()) {
        Some(text) => {
            CAPTION_WIDTH.store(CAPTION_STRIP_WIDTH, Ordering::Relaxed);
            let _ = app.emit(
                "partial",
                serde_json::json!({ "text": text, "streaming": true }),
            );
        }
        None => {
            CAPTION_WIDTH.store(0, Ordering::Relaxed);
            let _ = app.emit(
                "partial",
                serde_json::json!({ "text": "", "streaming": false }),
            );
        }
    }
    apply_dock_size(app);
    #[cfg(target_os = "linux")]
    {
        let app = app.clone();
        let inner = app.clone();
        let _ = app.run_on_main_thread(move || shrink_to_min(&inner));
    }
}

pub fn ensure_on_main(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    {
        shrink_to_min(app);
        stick_to_all_workspaces(app);
    }
    ensure(app);
}

fn enforce(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        for i in 0..12 {
            let app_for_main = app.clone();
            let _ = app.run_on_main_thread(move || {
                #[cfg(target_os = "linux")]
                shrink_to_min(&app_for_main);
            });
            ensure(&app);
            std::thread::sleep(std::time::Duration::from_millis(400 + i * 400));
        }
        std::thread::sleep(std::time::Duration::from_millis(2000));
        if let Some(win) = window(&app) {
            let size = win.outer_size().ok().unwrap_or(PhysicalSize {
                width: DOCK_SIZE as u32,
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

#[cfg(target_os = "linux")]
fn shrink_to_min(app: &AppHandle) {
    use gtk::prelude::*;

    let Some(win) = window(app) else { return };
    let Ok(vbox) = win.default_vbox() else { return };

    fn find_webview(widget: &gtk::Widget) -> Option<gtk::Widget> {
        if widget.type_().name().contains("WebKitWebView") {
            return Some(widget.clone());
        }
        let Ok(container) = widget.clone().downcast::<gtk::Container>() else {
            return None;
        };
        container
            .children()
            .iter()
            .find_map(find_webview)
    }

    let vbox_widget: gtk::Widget = vbox.clone().upcast();
    if let Some(webview) = find_webview(&vbox_widget) {
        let width = current_width();
        webview.set_size_request(width as i32, DOCK_SIZE as i32);
    }
}

pub fn set_state(app: &AppHandle, status: &str, message: Option<&str>) {
    let _ = app.emit(
        "overlay-state",
        serde_json::json!({
            "state": status,
            "message": message,
        }),
    );
    crate::tray::apply_state_icon(app, status);
}