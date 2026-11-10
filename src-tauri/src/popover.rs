//! The cheat sheet window, shown next to the tray icon like a popover.

use crate::hub::Hub;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Rect, WebviewWindow};

pub const LABEL: &str = "popover";

/// When the popover was last hidden. A click on the tray icon first takes the focus away
/// from the popover, which hides it, and then toggles it; this keeps it from reopening.
static LAST_HIDDEN: Mutex<Option<Instant>> = Mutex::new(None);
/// The tray icon the popover was last placed against, in physical pixels (x, y, w, h).
static ANCHOR: Mutex<Option<(f64, f64, f64, f64)>> = Mutex::new(None);

pub fn toggle(app: &AppHandle, anchor: Option<Rect>) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    if window.is_visible().unwrap_or(false) {
        hide(&window);
        return;
    }
    if LAST_HIDDEN.lock().unwrap().is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
        return;
    }
    show(app, anchor);
}

pub fn show(app: &AppHandle, anchor: Option<Rect>) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    if let Some(rect) = anchor {
        let scale = window.scale_factor().unwrap_or(1.0);
        let pos = rect.position.to_physical::<f64>(scale);
        let size = rect.size.to_physical::<f64>(scale);
        *ANCHOR.lock().unwrap() = Some((pos.x, pos.y, size.width, size.height));
    }
    if ANCHOR.lock().unwrap().is_some() {
        place(&window);
    } else {
        let _ = window.center();
    }
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to(LABEL, "popover-shown", ());
    // Opening the cheat sheet is when an outdated keymap matters; check unless just done
    app.state::<Arc<Hub>>().refresh_keymaps_if_older_than(Duration::from_secs(60));
}

pub fn hide(window: &WebviewWindow) {
    let _ = window.hide();
    *LAST_HIDDEN.lock().unwrap() = Some(Instant::now());
}

/// Sets the popover height (logical pixels) to fit its content, keeping it at the tray icon.
pub fn fit_height(window: &WebviewWindow, height: f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let Ok(inner) = window.inner_size() else { return };
    let inner = inner.to_logical::<f64>(scale);
    let height = height.clamp(160.0, 900.0).round();
    if (inner.height - height).abs() < 2.0 {
        return;
    }
    let _ = window.set_size(LogicalSize::new(inner.width, height));
    if ANCHOR.lock().unwrap().is_some() {
        place(window);
    }
}

/// Below the icon when the tray is at the top of the screen (macOS), above it otherwise
/// (Windows taskbar), centered on the icon and kept on screen.
fn place(window: &WebviewWindow) {
    let Some((ax, ay, aw, ah)) = *ANCHOR.lock().unwrap() else { return };
    let Ok(size) = window.outer_size() else { return };
    let (w, h) = (size.width as f64, size.height as f64);
    let (cx, cy) = (ax + aw / 2.0, ay + ah / 2.0);

    let monitor = window
        .monitor_from_point(cx, cy)
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let (mx, my, mw, mh) = match &monitor {
        Some(m) => {
            let area = m.work_area();
            (area.position.x as f64, area.position.y as f64, area.size.width as f64, area.size.height as f64)
        }
        None => (0.0, 0.0, f64::MAX / 4.0, f64::MAX / 4.0),
    };
    let margin = 8.0;
    let x = (cx - w / 2.0).clamp(mx + margin, (mx + mw - w - margin).max(mx + margin));
    let y = if cy < my + mh / 2.0 { ay + ah + 6.0 } else { ay - h - 6.0 };
    let y = y.clamp(my, (my + mh - h).max(my));
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}
