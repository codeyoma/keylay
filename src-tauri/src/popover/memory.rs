//! What the cheat sheet window remembers between openings and launches (`window.json` in the
//! app config folder): whether it is pinned, where the user put it, and the size they gave it.
//!
//! Only the user's own moves and resizes count. The app's own (placing the window under the
//! tray icon, fitting it to a layer) are recorded in a [`Tracker`] first, and the window events
//! that report exactly those values are ignored.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Saved {
    pub pinned: bool,
    /// Where the user put the window, in points (None: under the tray icon). Points, not
    /// pixels: they mean the same on every display, whatever its scale.
    pub x: Option<f64>,
    pub y: Option<f64>,
    /// The size the user dragged it to, in logical pixels
    pub width: Option<f64>,
    pub height: Option<f64>,
}

/// `Saved`, kept in a JSON file.
pub struct Memory {
    path: Option<PathBuf>,
    saved: Saved,
}

impl Memory {
    pub const fn empty() -> Memory {
        Memory {
            path: None,
            saved: Saved { pinned: false, x: None, y: None, width: None, height: None },
        }
    }

    pub fn load(path: &Path) -> Memory {
        let saved = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Memory { path: Some(path.to_path_buf()), saved }
    }

    pub fn get(&self) -> &Saved {
        &self.saved
    }

    pub fn update(&mut self, change: impl FnOnce(&mut Saved)) {
        let before = self.saved.clone();
        change(&mut self.saved);
        if self.saved == before {
            return;
        }
        if let (Some(path), Ok(text)) = (&self.path, serde_json::to_string_pretty(&self.saved)) {
            let _ = std::fs::write(path, text + "\n");
        }
    }
}

/// Tells the user's moves and resizes from the app's own.
#[derive(Debug, Default)]
pub struct Tracker {
    /// The last position (points) and size (physical pixels) the app gave the window
    position: Option<(i32, i32)>,
    size: Option<(u32, u32)>,
    /// For moves whose result the app cannot know in advance (centering)
    quiet_until: Option<Instant>,
}

/// How long window events are ignored after a move the app cannot predict.
const QUIET: Duration = Duration::from_millis(150);

impl Tracker {
    pub const fn new() -> Tracker {
        Tracker { position: None, size: None, quiet_until: None }
    }

    pub fn app_moved(&mut self, x: i32, y: i32) {
        self.position = Some((x, y));
    }

    pub fn app_resized(&mut self, width: u32, height: u32) {
        self.size = Some((width, height));
    }

    /// A move whose result is unknown: ignore window events for a moment.
    pub fn app_moved_somewhere(&mut self, now: Instant) {
        self.quiet_until = Some(now + QUIET);
    }

    fn quiet(&self, now: Instant) -> bool {
        self.quiet_until.is_some_and(|until| now < until)
    }

    /// Whether a Moved event (points) is the user's: not the position the app just set.
    pub fn is_user_move(&self, x: i32, y: i32, now: Instant) -> bool {
        !self.quiet(now) && self.position != Some((x, y))
    }

    /// Whether a Resized event (physical pixels) is the user's: not the size the app just set
    /// (±1 px of rounding).
    pub fn is_user_resize(&self, width: u32, height: u32, now: Instant) -> bool {
        if self.quiet(now) {
            return false;
        }
        match self.size {
            Some((w, h)) => w.abs_diff(width) > 1 || h.abs_diff(height) > 1,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_apps_own_move_is_not_the_users() {
        let mut t = Tracker::new();
        let now = Instant::now();
        t.app_moved(100, 40);
        assert!(!t.is_user_move(100, 40, now));
        assert!(t.is_user_move(101, 40, now));
    }

    #[test]
    fn every_move_is_the_users_before_the_app_moves() {
        assert!(Tracker::new().is_user_move(0, 0, Instant::now()));
    }

    #[test]
    fn resizes_within_rounding_are_the_apps() {
        let mut t = Tracker::new();
        let now = Instant::now();
        t.app_resized(800, 600);
        assert!(!t.is_user_resize(801, 599, now));
        assert!(t.is_user_resize(800, 640, now));
    }

    #[test]
    fn events_right_after_centering_are_ignored() {
        let mut t = Tracker::new();
        let now = Instant::now();
        t.app_moved_somewhere(now);
        assert!(!t.is_user_move(5, 5, now + Duration::from_millis(50)));
        assert!(!t.is_user_resize(5, 5, now + Duration::from_millis(50)));
        assert!(t.is_user_move(5, 5, now + Duration::from_millis(200)));
    }

    #[test]
    fn memory_saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("keylay-memory-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("window.json");
        let mut memory = Memory::load(&path);
        assert_eq!(memory.get(), &Saved::default());
        memory.update(|s| {
            s.pinned = true;
            s.x = Some(1500.5);
        });
        let again = Memory::load(&path);
        assert!(again.get().pinned);
        assert_eq!(again.get().x, Some(1500.5));
        let _ = std::fs::remove_dir_all(dir);
    }
}
