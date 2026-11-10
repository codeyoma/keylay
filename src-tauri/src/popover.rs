//! The cheat sheet window, shown next to the tray icon like a popover.

use crate::hub::Hub;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use crate::window_state::{self, mark_programmatic};
use tauri::{LogicalPosition, PhysicalSize};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, Rect, WebviewWindow};

pub const LABEL: &str = "popover";

/// When the popover was last hidden. A click on the tray icon first takes the focus away
/// from the popover, which hides it, and then toggles it; this keeps it from reopening.
static LAST_HIDDEN: Mutex<Option<Instant>> = Mutex::new(None);
/// The tray icon the popover was last placed against, in physical pixels (x, y, w, h).
static ANCHOR: Mutex<Option<(f64, f64, f64, f64)>> = Mutex::new(None);

/// Moves the window (points), recording the position so the Moved event is not taken for the
/// user's.
fn move_to(window: &WebviewWindow, x: f64, y: f64) {
    window_state::app_moved(x.round() as i32, y.round() as i32);
    let _ = window.set_position(LogicalPosition::new(x, y));
}

/// Resizes the window (logical size), recording the result for the Resized event.
fn resize_to(window: &WebviewWindow, width: f64, height: f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let physical: PhysicalSize<u32> = LogicalSize::new(width, height).to_physical(scale);
    window_state::app_resized(physical.width, physical.height);
    let _ = window.set_size(LogicalSize::new(width, height));
}

/// Forgets where the user put the window: it opens under the tray icon again.
pub fn reset_position(app: &AppHandle) {
    window_state::update(|s| {
        s.x = None;
        s.y = None;
    });
    if let Some(window) = app.get_webview_window(LABEL) {
        if window.is_visible().unwrap_or(false) && ANCHOR.lock().unwrap().is_some() {
            place(&window);
        }
    }
}

/// A click on the tray icon.
pub fn toggle(app: &AppHandle, anchor: Option<Rect>) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    if window.is_visible().unwrap_or(false) {
        dismiss(app, &window);
        return;
    }
    if LAST_HIDDEN.lock().unwrap().is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
        return;
    }
    show(app, anchor, None);
}

/// The KeyLay key on a keyboard: opens the popover at the tray icon on that keyboard (its
/// config name, or its device name), or closes it.
pub fn toggle_for_keyboard(app: &AppHandle, keyboard: Option<String>) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    if window.is_visible().unwrap_or(false) {
        dismiss(app, &window);
        return;
    }
    let anchor = app.tray_by_id(crate::tray::TRAY_ID).and_then(|t| t.rect().ok().flatten());
    show(app, anchor, keyboard);
}

/// Shows the popover, at the tray icon when its position is known. `keyboard` picks the
/// keyboard to show; otherwise the popover follows the one whose layer changed last.
pub fn show(app: &AppHandle, anchor: Option<Rect>, keyboard: Option<String>) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    if let Some(rect) = anchor {
        let scale = window.scale_factor().unwrap_or(1.0);
        let pos = rect.position.to_physical::<f64>(scale);
        let size = rect.size.to_physical::<f64>(scale);
        *ANCHOR.lock().unwrap() = Some((pos.x, pos.y, size.width, size.height));
    }
    let saved = window_state::get();
    // The width the user last gave it (the height follows the content)
    if let (Some(width), Ok(inner)) = (saved.width, window.inner_size()) {
        let scale = window.scale_factor().unwrap_or(1.0);
        let inner = inner.to_logical::<f64>(scale);
        if (inner.width - width).abs() >= 1.0 {
            resize_to(&window, width, inner.height);
        }
    }
    if let Some(position) = saved_position(&window) {
        // Where the user last put it
        move_to(&window, position.x, position.y);
    } else if ANCHOR.lock().unwrap().is_some() {
        place(&window);
    } else {
        mark_programmatic();
        let _ = window.center();
    }
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to(LABEL, "popover-shown", keyboard);
    // Opening the cheat sheet is when an outdated keymap matters; check unless just done
    app.state::<Arc<Hub>>().refresh_keymaps_if_older_than(Duration::from_secs(60));
}

pub fn hide(window: &WebviewWindow) {
    let _ = window.hide();
    *LAST_HIDDEN.lock().unwrap() = Some(Instant::now());
}

/// Closes the popover on purpose (keyboard key, Esc, close button, tray click). On macOS the
/// app is hidden too, so the app you were typing in gets the focus back.
pub fn dismiss(app: &AppHandle, window: &WebviewWindow) {
    hide(window);
    #[cfg(target_os = "macos")]
    let _ = app.hide();
    #[cfg(not(target_os = "macos"))]
    let _ = app;
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
    resize_to(window, inner.width, height);
    // A window the user put somewhere stays there (its top left stays put); otherwise it
    // stays at the tray icon
    if saved_position(window).is_none() && ANCHOR.lock().unwrap().is_some() {
        place(window);
    }
}

/// Where the user last put the window (points), if that is still on a screen.
fn saved_position(window: &WebviewWindow) -> Option<LogicalPosition<f64>> {
    let saved = window_state::get();
    let (x, y) = (saved.x?, saved.y?);
    // A monitor that was unplugged since: fall back to the tray icon
    let on_screen = window.available_monitors().ok()?.iter().any(|m| {
        let scale = m.scale_factor();
        let p = m.position().to_logical::<f64>(scale);
        let s = m.size().to_logical::<f64>(scale);
        x + 20.0 >= p.x && x + 20.0 < p.x + s.width && y + 10.0 >= p.y && y + 10.0 < p.y + s.height
    });
    on_screen.then_some(LogicalPosition::new(x, y))
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
    let scale = window.scale_factor().unwrap_or(1.0);
    move_to(window, x / scale, y / scale);
}
