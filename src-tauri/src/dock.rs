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

pub(crate) fn compute_coordinates(
    ox: i32,
    oy: i32,
    w: u32,
    h: u32,
    win_w: u32,
    win_h: u32,
    margin: f64,
    position: &str,
) -> (i32, i32) {
    match position {
        "bottom_center" => (
            ox + ((w.saturating_sub(win_w) as f64) / 2.0).max(0.0) as i32,
            oy + (h.saturating_sub(win_h) as f64 - margin).max(0.0) as i32,
        ),
        "bottom_left" => (
            ox + margin.max(0.0) as i32,
            oy + (h.saturating_sub(win_h) as f64 - margin).max(0.0) as i32,
        ),
        "top_right" => (
            ox + (w.saturating_sub(win_w) as f64 - margin).max(0.0) as i32,
            oy + margin.max(0.0) as i32,
        ),
        "top_left" => (
            ox + margin.max(0.0) as i32,
            oy + margin.max(0.0) as i32,
        ),
        _ => (
            ox + (w.saturating_sub(win_w) as f64 - margin).max(0.0) as i32,
            oy + (h.saturating_sub(win_h) as f64 - margin).max(0.0) as i32,
        ),
    }
}

fn calculate_position(win: &WebviewWindow, width: u32, height: u32, position: &str) -> PhysicalPosition<i32> {
    let default = PhysicalPosition { x: 100, y: 16 };
    let Some(monitor) = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten()) else {
        return default;
    };
    let scale = monitor.scale_factor();
    let margin = (MARGIN * scale).round();
    let (ox, oy, w, h) = {
        let wa = monitor.work_area();
        if wa.size.width > 0 && wa.size.height > 0 {
            (wa.position.x, wa.position.y, wa.size.width, wa.size.height)
        } else {
            let s = monitor.size();
            let p = monitor.position();
            (p.x, p.y, s.width, s.height)
        }
    };
    let (x, y) = compute_coordinates(ox, oy, w, h, width, height, margin, position);
    PhysicalPosition { x, y }
}

pub fn reposition(app: &AppHandle) {
    LAST_ASSERT_MS.store(0, Ordering::Relaxed);
    apply_dock_size(app);
    enforce(app);
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

/// Parks the dock at the configured anchor position. The window size is constrained
/// by the config (min/max 210x29) and WebKit's natural height request; the
/// pill content is aligned so it hugs the corner regardless.
fn apply_dock_size(app: &AppHandle) {
    if let Some(win) = window(app) {
        let width = current_width();
        let _ = win.set_always_on_top(true);
        let _ = win.show();
        let size = win.outer_size().ok().unwrap_or(PhysicalSize {
            width,
            height: DOCK_SIZE as u32,
        });
        let dock_position = app
            .try_state::<crate::state::AppState>()
            .and_then(|s| s.settings.lock().ok().map(|settings| settings.dock_position.clone()))
            .unwrap_or_else(|| "bottom_right".to_string());
        let target = calculate_position(&win, size.width, size.height, &dock_position);
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
static LAST_CAPTION: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

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
    let text = text.map(str::trim).filter(|t| !t.is_empty());
    
    // Deduplicate consecutive identical caption calls to avoid flooding WebKit with eval calls
    if let Ok(mut last) = LAST_CAPTION.lock() {
        let text_owned = text.map(|s| s.to_string());
        if *last == text_owned {
            return;
        }
        *last = text_owned;
    }

    match text {
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

    let app_inner = app.clone();
    let _ = app.run_on_main_thread(move || {
        apply_dock_size(&app_inner);
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bottom_right_standard_1080p_with_taskbar() {
        // 1920x1080 monitor, but work_area is 1920x1032 due to 48px Windows bottom taskbar
        let (ox, oy, w, h) = (0, 0, 1920, 1032);
        let (win_w, win_h) = (210, 29);
        let margin = 16.0;

        let (x, y) = compute_coordinates(ox, oy, w, h, win_w, win_h, margin, "bottom_right");
        assert_eq!(x, 1920 - 210 - 16);
        assert_eq!(y, 1032 - 29 - 16); // 987, well above 1032 taskbar
    }

    #[test]
    fn test_bottom_left_with_taskbar() {
        let (ox, oy, w, h) = (0, 0, 1920, 1032);
        let (win_w, win_h) = (210, 29);
        let margin = 16.0;

        let (x, y) = compute_coordinates(ox, oy, w, h, win_w, win_h, margin, "bottom_left");
        assert_eq!(x, 16);
        assert_eq!(y, 987);
    }

    #[test]
    fn test_bottom_center_with_taskbar() {
        let (ox, oy, w, h) = (0, 0, 1920, 1032);
        let (win_w, win_h) = (210, 29);
        let margin = 16.0;

        let (x, y) = compute_coordinates(ox, oy, w, h, win_w, win_h, margin, "bottom_center");
        assert_eq!(x, ((1920 - 210) as f64 / 2.0) as i32);
        assert_eq!(y, 987);
    }

    #[test]
    fn test_top_right_and_top_left() {
        let (ox, oy, w, h) = (0, 0, 1920, 1032);
        let (win_w, win_h) = (210, 29);
        let margin = 16.0;

        let (tr_x, tr_y) = compute_coordinates(ox, oy, w, h, win_w, win_h, margin, "top_right");
        assert_eq!(tr_x, 1920 - 210 - 16);
        assert_eq!(tr_y, 16);

        let (tl_x, tl_y) = compute_coordinates(ox, oy, w, h, win_w, win_h, margin, "top_left");
        assert_eq!(tl_x, 16);
        assert_eq!(tl_y, 16);
    }

    #[test]
    fn test_secondary_monitor_offset() {
        // Secondary monitor placed to the right at ox=1920, oy=0
        let (ox, oy, w, h) = (1920, 0, 1920, 1032);
        let (win_w, win_h) = (210, 29);
        let margin = 16.0;

        let (x, y) = compute_coordinates(ox, oy, w, h, win_w, win_h, margin, "bottom_right");
        assert_eq!(x, 1920 + (1920 - 210 - 16));
        assert_eq!(y, 987);

        // Secondary monitor placed to the left at ox=-1920, oy=0
        let (ox_l, oy_l, w_l, h_l) = (-1920, 0, 1920, 1032);
        let (xl, yl) = compute_coordinates(ox_l, oy_l, w_l, h_l, win_w, win_h, margin, "bottom_right");
        assert_eq!(xl, -1920 + (1920 - 210 - 16));
        assert_eq!(yl, 987);
    }

    #[test]
    fn test_top_and_left_taskbar_offsets() {
        // Taskbar at top of screen: oy=48, h=1032
        let (ox, oy, w, h) = (0, 48, 1920, 1032);
        let (win_w, win_h) = (210, 29);
        let margin = 16.0;

        let (x, y) = compute_coordinates(ox, oy, w, h, win_w, win_h, margin, "top_left");
        assert_eq!(x, 16);
        assert_eq!(y, 48 + 16);

        // Taskbar at left of screen: ox=60, w=1860
        let (ox2, oy2, w2, h2) = (60, 0, 1860, 1080);
        let (x2, y2) = compute_coordinates(ox2, oy2, w2, h2, win_w, win_h, margin, "bottom_left");
        assert_eq!(x2, 60 + 16);
        assert_eq!(y2, 1080 - 29 - 16);
    }
}

