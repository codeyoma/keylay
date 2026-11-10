//! Remembers the cheat sheet window between openings and launches (`window.json` in the app
//! config folder): whether it is pinned, where the user put it, and the size they gave it.
//!
//! Only the user's own moves and resizes count. The app's own (placing the window under the
//! tray icon, fitting it to a layer) are recorded with `app_moved` / `app_resized`, and the
//! window events that report exactly those values are ignored.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowState {
    pub pinned: bool,
    /// Where the user put the window, in points (None: under the tray icon). Points, not
    /// pixels: they mean the same on every display, whatever its scale.
    pub x: Option<f64>,
    pub y: Option<f64>,
    /// The size the user dragged it to, in logical pixels
    pub width: Option<f64>,
    pub height: Option<f64>,
}

static STATE: Mutex<Option<(PathBuf, WindowState)>> = Mutex::new(None);
static PROGRAMMATIC_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);
/// The last position (points) and size (physical pixels) the app gave the window
static APP_POSITION: Mutex<Option<(i32, i32)>> = Mutex::new(None);
static APP_SIZE: Mutex<Option<(u32, u32)>> = Mutex::new(None);

pub fn init(path: PathBuf) {
    let state = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    *STATE.lock().unwrap() = Some((path, state));
}

pub fn get() -> WindowState {
    STATE.lock().unwrap().as_ref().map(|(_, s)| s.clone()).unwrap_or_default()
}

pub fn update(change: impl FnOnce(&mut WindowState)) {
    let mut guard = STATE.lock().unwrap();
    let Some((path, state)) = guard.as_mut() else { return };
    change(state);
    if let Ok(text) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, text + "\n");
    }
}

/// For moves whose result the app cannot know in advance (centering): ignores window events
/// for a moment.
pub fn mark_programmatic() {
    *PROGRAMMATIC_UNTIL.lock().unwrap() = Some(Instant::now() + Duration::from_millis(150));
}

pub fn app_moved(x: i32, y: i32) {
    *APP_POSITION.lock().unwrap() = Some((x, y));
}

pub fn app_resized(width: u32, height: u32) {
    *APP_SIZE.lock().unwrap() = Some((width, height));
}

fn just_marked() -> bool {
    PROGRAMMATIC_UNTIL.lock().unwrap().is_some_and(|until| Instant::now() < until)
}

/// Whether a Moved event is the user's: not the position the app just set.
pub fn user_moved(x: i32, y: i32) -> bool {
    !just_marked() && *APP_POSITION.lock().unwrap() != Some((x, y))
}

/// Whether a Resized event is the user's: not the size the app just set (±1 px of rounding).
pub fn user_resized(width: u32, height: u32) -> bool {
    if just_marked() {
        return false;
    }
    match *APP_SIZE.lock().unwrap() {
        Some((w, h)) => w.abs_diff(width) > 1 || h.abs_diff(height) > 1,
        None => true,
    }
}
