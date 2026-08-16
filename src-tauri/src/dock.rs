use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

pub const DOCK_SIZE: f64 = 30.0;
const MARGIN: f64 = 16.0;

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("dock")
}

fn top_right(app: &AppHandle) -> PhysicalPosition<i32> {
    let default = PhysicalPosition { x: 100, y: 16 };
    let Some(win) = window(app) else {
        return default;
    };
    let Some(monitor) = win.current_monitor().ok().flatten() else {
        return default;
    };
    let size = monitor.size();
    let x = (size.width as f64 - DOCK_SIZE - MARGIN).max(0.0) as i32;
    PhysicalPosition { x, y: MARGIN as i32 }
}

pub fn init(app: &AppHandle) {
    if let Some(win) = window(app) {
        let _ = win.set_position(top_right(app));
        let _ = win.set_always_on_top(true);
        let _ = win.show();
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