use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Size, WebviewWindow};

pub const DOCK_SIZE: f64 = 29.0;
const MARGIN: f64 = 16.0;
const TOLERANCE: i32 = 2;

static LAST_ASSERT_MS: AtomicU64 = AtomicU64::new(0);

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("dock")
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
    if let Some(win) = window(app) {
        let _ = win.set_always_on_top(true);
        let _ = win.set_min_size(Some(PhysicalSize {
            width: DOCK_SIZE as u32,
            height: DOCK_SIZE as u32,
        }));
        let _ = win.set_size(Size::Physical(PhysicalSize {
            width: DOCK_SIZE as u32,
            height: DOCK_SIZE as u32,
        }));
        let size = win.outer_size().ok().unwrap_or(PhysicalSize {
            width: DOCK_SIZE as u32,
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

pub fn ensure_on_main(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    shrink_to_min(app);
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
        webview.set_size_request(DOCK_SIZE as i32, DOCK_SIZE as i32);
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
}