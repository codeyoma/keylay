//! The cheat sheet window, shown next to the tray icon like a popover.
//!
//! This module owns everything about the window: opening and closing it, placing it at the
//! tray icon, fitting it to its content, pinning it, and remembering where the user put it
//! and the size they gave it (see `memory`). `main.rs` forwards the window's events to
//! [`on_window_event`].

mod memory;

use crate::hub::Hub;
use memory::{Memory, Saved, Tracker};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalSize, Rect, WebviewWindow};
use tauri::{Window, WindowEvent};

pub const LABEL: &str = "popover";

struct State {
    memory: Memory,
    tracker: Tracker,
    /// The tray icon the popover was last placed against, in physical pixels (x, y, w, h).
    anchor: Option<(f64, f64, f64, f64)>,
    /// When the popover was last hidden. A click on the tray icon first takes the focus away
    /// from the popover, which hides it, and then toggles it; this keeps it from reopening.
    last_hidden: Option<Instant>,
    /// Pinned without saving it (the KEYLAY_SHOW_POPOVER debugging aid)
    pinned_this_run: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    memory: Memory::empty(),
    tracker: Tracker::new(),
    anchor: None,
    last_hidden: None,
    pinned_this_run: false,
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap()
}

fn saved() -> Saved {
    state().memory.get().clone()
}

/// Loads what the window remembers (`window.json`).
pub fn init(path: &Path) {
    state().memory = Memory::load(path);
}

pub fn pinned() -> bool {
    let state = state();
    state.memory.get().pinned || state.pinned_this_run
}

/// Pins the popover until the app quits, without saving it.
pub fn pin_for_this_run() {
    state().pinned_this_run = true;
}

/// The height the user gave the window; "All" grows up to it.
pub fn max_height() -> Option<f64> {
    state().memory.get().height
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

fn visible(window: &WebviewWindow) -> bool {
    window.is_visible().unwrap_or(false)
}

/// A pinned popover stays open when another app has the focus.
pub fn set_pinned(app: &AppHandle, pinned: bool) {
    state().memory.update(|s| s.pinned = pinned);
    app.state::<Arc<Hub>>().notify();
}

/// A click on the tray icon.
pub fn toggle(app: &AppHandle, anchor: Option<Rect>) {
    let Some(window) = window(app) else { return };
    if visible(&window) {
        dismiss(app);
        return;
    }
    if state().last_hidden.is_some_and(|t| t.elapsed() < Duration::from_millis(300)) {
        return;
    }
    show(app, anchor, None);
}

/// The KeyLay key on a keyboard: opens the popover at the tray icon on that keyboard (its
/// config name, or its device name), or closes it.
pub fn toggle_for_keyboard(app: &AppHandle, keyboard: Option<String>) {
    let Some(window) = window(app) else { return };
    if visible(&window) {
        dismiss(app);
        return;
    }
    let anchor = app.tray_by_id(crate::tray::TRAY_ID).and_then(|t| t.rect().ok().flatten());
    show(app, anchor, keyboard);
}

/// Shows the popover: where the user last put it, else at the tray icon when its position is
/// known, else centered. `keyboard` picks the keyboard to show; otherwise the popover follows
/// the one whose layer changed last.
pub fn show(app: &AppHandle, anchor: Option<Rect>, keyboard: Option<String>) {
    let Some(window) = window(app) else { return };
    let scale = window.scale_factor().unwrap_or(1.0);
    if let Some(rect) = anchor {
        let pos = rect.position.to_physical::<f64>(scale);
        let size = rect.size.to_physical::<f64>(scale);
        state().anchor = Some((pos.x, pos.y, size.width, size.height));
    }
    // The width the user last gave it (the height follows the content)
    if let (Some(width), Ok(inner)) = (saved().width, window.inner_size()) {
        let inner = inner.to_logical::<f64>(scale);
        if (inner.width - width).abs() >= 1.0 {
            resize_to(&window, width, inner.height);
        }
    }
    if let Some(position) = saved_position(&window) {
        move_to(&window, position.x, position.y);
    } else if state().anchor.is_some() {
        place(&window);
    } else {
        state().tracker.app_moved_somewhere(Instant::now());
        let _ = window.center();
    }
    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit_to(LABEL, "popover-shown", keyboard);
    app.state::<Arc<Hub>>().cheat_sheet_opened();
}

/// Hides the popover (another window takes over, or it lost the focus).
pub fn hide(app: &AppHandle) {
    if let Some(window) = window(app) {
        let _ = window.hide();
        state().last_hidden = Some(Instant::now());
    }
}

/// Closes the popover on purpose (keyboard key, Esc, close button, tray click). On macOS the
/// app is hidden too, so the app you were typing in gets the focus back.
pub fn dismiss(app: &AppHandle) {
    hide(app);
    #[cfg(target_os = "macos")]
    let _ = app.hide();
}

/// Forgets where the user put the window: it opens under the tray icon again.
pub fn reset_position(app: &AppHandle) {
    state().memory.update(|s| {
        s.x = None;
        s.y = None;
    });
    if let Some(window) = window(app) {
        if visible(&window) && state().anchor.is_some() {
            place(&window);
        }
    }
}

/// Sets the popover height (logical pixels) to fit its content. A window the user put
/// somewhere keeps its top left; otherwise it stays at the tray icon.
pub fn fit_height(app: &AppHandle, height: f64) {
    let Some(window) = window(app) else { return };
    let scale = window.scale_factor().unwrap_or(1.0);
    let Ok(inner) = window.inner_size() else { return };
    let inner = inner.to_logical::<f64>(scale);
    let height = height.clamp(160.0, 900.0).round();
    if (inner.height - height).abs() < 2.0 {
        return;
    }
    resize_to(&window, inner.width, height);
    if saved_position(&window).is_none() && state().anchor.is_some() {
        place(&window);
    }
}

/// The popover's window events: it hides when another app is clicked unless pinned, closing
/// only hides it, and the user's own moves and resizes are remembered.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != LABEL {
        return;
    }
    let scale = window.scale_factor().unwrap_or(1.0);
    match event {
        WindowEvent::Focused(false) if !pinned() => hide(window.app_handle()),
        WindowEvent::Moved(position) => {
            let point = position.to_logical::<f64>(scale);
            let mut state = state();
            if state.tracker.is_user_move(point.x.round() as i32, point.y.round() as i32, Instant::now()) {
                state.memory.update(|s| {
                    s.x = Some(point.x);
                    s.y = Some(point.y);
                });
            }
        }
        WindowEvent::Resized(size) if window.is_visible().unwrap_or(false) => {
            let mut state = state();
            if state.tracker.is_user_resize(size.width, size.height, Instant::now()) {
                let size = size.to_logical::<f64>(scale);
                state.memory.update(|s| {
                    s.width = Some(size.width);
                    s.height = Some(size.height);
                });
            }
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(window.app_handle());
        }
        _ => {}
    }
}

/// Moves the window (points), recording the position so the Moved event is not taken for the
/// user's.
fn move_to(window: &WebviewWindow, x: f64, y: f64) {
    state().tracker.app_moved(x.round() as i32, y.round() as i32);
    let _ = window.set_position(LogicalPosition::new(x, y));
}

/// Resizes the window (logical size), recording the result for the Resized event.
fn resize_to(window: &WebviewWindow, width: f64, height: f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    let physical: PhysicalSize<u32> = LogicalSize::new(width, height).to_physical(scale);
    state().tracker.app_resized(physical.width, physical.height);
    let _ = window.set_size(LogicalSize::new(width, height));
}

/// Where the user last put the window (points), if that is still on a screen.
fn saved_position(window: &WebviewWindow) -> Option<LogicalPosition<f64>> {
    let saved = saved();
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
    let Some((ax, ay, aw, ah)) = state().anchor else { return };
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

pub mod commands {
    use tauri::AppHandle;

    #[tauri::command]
    pub fn set_pinned(app: AppHandle, pinned: bool) {
        super::set_pinned(&app, pinned);
    }

    #[tauri::command]
    pub fn hide_popover(app: AppHandle) {
        super::dismiss(&app);
    }

    #[tauri::command]
    pub fn fit_popover(app: AppHandle, height: f64) {
        super::fit_height(&app, height);
    }
}
