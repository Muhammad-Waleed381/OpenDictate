use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

pub const OVERLAY_WIDTH: f64 = 360.0;

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("overlay")
}

pub fn show(app: &AppHandle) {
    if let Some(win) = window(app) {
        let _ = win.set_position(center_top(app));
        let _ = win.show();
        let _ = win.set_always_on_top(true);
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(win) = window(app) {
        let _ = win.hide();
    }
}

fn center_top(app: &AppHandle) -> PhysicalPosition<i32> {
    let default = PhysicalPosition { x: 100, y: 16 };
    let Some(win) = window(app) else {
        return default;
    };
    let Some(monitor) = win.current_monitor().ok().flatten() else {
        return default;
    };
    let size = monitor.size();
    let x = (size.width.saturating_sub(OVERLAY_WIDTH as u32) / 2) as i32;
    PhysicalPosition { x, y: 16 }
}

pub fn set_state(app: &AppHandle, status: &str, message: Option<&str>) {
    let _ = app.emit(
        "overlay-state",
        serde_json::json!({
            "state": status,
            "message": message,
        }),
    );
    match status {
        "hidden" => hide(app),
        _ => show(app),
    }
}