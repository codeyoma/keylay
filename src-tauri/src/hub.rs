//! Shared state: the config, the keymaps and the connected keyboards (`devices`). Every change refreshes
//! the tray and sends a `snapshot` event to the popover.

use crate::config::Config;
use crate::devices::{now_ms, DeviceView, Devices, Transport};
use crate::keymap;
use crate::intake::{Sink, Source};
use crate::protocol::Report;
use crate::tray;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Connected keyboards, the one shown in the tray first.
    pub devices: Vec<DeviceView>,
    /// Keyboards from the config file.
    pub keyboards: Vec<KeyboardView>,
    pub pinned: bool,
    pub config_path: String,
    pub config_error: Option<String>,
    pub title_format: String,
    pub layer_width: usize,
    /// "glass" or "solid", as the windows should look now
    pub appearance: &'static str,
    /// The height the user gave the cheat sheet; "All" grows up to it
    pub popover_max_height: Option<f64>,
    /// The layer colours, by layer index (repeating), shared with the tray badge
    pub layer_colors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardView {
    pub name: String,
    /// Changes whenever the keymap SVG does, so the popover reloads it
    pub revision: u32,
    /// The keymap URL or path, and when it was last checked (ms since the epoch)
    pub source: String,
    pub checked_at: Option<u64>,
    pub layers: Vec<String>,
    pub has_keymap: bool,
    pub keymap_error: Option<String>,
}

pub struct Hub {
    app: AppHandle,
    pub config_path: PathBuf,
    config: RwLock<Config>,
    config_error: RwLock<Option<String>>,
    keymaps: keymap::Store,
    devices: Devices,
}

impl Hub {
    pub fn new(app: AppHandle, config_path: PathBuf, cache_dir: PathBuf) -> Hub {
        let (config, config_error) = Config::load_or_create(&config_path);
        Hub {
            app,
            config_path,
            config: RwLock::new(config),
            config_error: RwLock::new(config_error),
            keymaps: keymap::Store::new(cache_dir),
            devices: Devices::default(),
        }
    }

    /// Re-reads the config file and checks every keymap. Blocks on the network.
    pub fn reload(&self) {
        let (config, error) = Config::load_or_create(&self.config_path);
        *self.config.write().unwrap() = config;
        *self.config_error.write().unwrap() = error;
        self.refresh_keymaps();
        self.notify();
    }

    /// Checks every keymap against its source, then tells the tray and the popover (which
    /// also shows when the check ran). Blocks on the network.
    pub fn refresh_keymaps(&self) {
        let keyboards = self.config.read().unwrap().keyboards.clone();
        self.keymaps.refresh(&keyboards);
        self.notify();
    }

    /// Checks the keymaps in the background now, then every `keymap_check_minutes` (a
    /// setting, 0 = never, so it is read again every minute).
    pub fn start_keymap_checks(self: &Arc<Self>) {
        let hub = self.clone();
        std::thread::spawn(move || loop {
            hub.refresh_keymaps();
            let mut waited = 0;
            loop {
                std::thread::sleep(Duration::from_secs(60));
                waited += 1;
                let every = hub.config().keymap_check_minutes;
                if every > 0 && waited >= every {
                    break;
                }
            }
        });
    }

    /// The cheat sheet opened, which is when an outdated keymap matters: checks in the
    /// background unless that was just done.
    pub fn cheat_sheet_opened(self: &Arc<Self>) {
        if !self.keymaps.checked_within(Duration::from_secs(60)) {
            let hub = self.clone();
            std::thread::spawn(move || hub.refresh_keymaps());
        }
    }

    pub fn config(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    /// Saves a config from the settings window, then checks the keymaps against it.
    pub fn save_config(self: &Arc<Self>, config: Config) -> Result<(), String> {
        config.save(&self.config_path)?;
        let mode = crate::appearance::effective(&config.appearance);
        *self.config.write().unwrap() = config;
        *self.config_error.write().unwrap() = None;
        crate::appearance::apply(&self.app, mode);
        self.notify();
        let hub = self.clone();
        std::thread::spawn(move || hub.refresh_keymaps());
        Ok(())
    }

    pub fn is_configured(&self, device_name: &str, transport: Transport) -> bool {
        self.config.read().unwrap().keyboard_for(device_name, transport).is_some()
    }

    pub fn keymap_svg(&self, keyboard: &str) -> Option<String> {
        self.keymaps.svg(keyboard)
    }

    pub fn snapshot(&self) -> Snapshot {
        let config = self.config.read().unwrap();
        let keymaps = self.keymaps.all();
        let keyboards = config
            .keyboards
            .iter()
            .map(|k| {
                let keymap = keymaps.get(&k.name);
                KeyboardView {
                    name: k.name.clone(),
                    revision: keymap.map_or(0, |m| m.revision),
                    source: k.keymap.clone(),
                    checked_at: keymap.and_then(|m| m.checked_at),
                    layers: keymap.map(|m| m.layers.clone()).unwrap_or_default(),
                    has_keymap: keymap.is_some_and(|m| m.svg.is_some()),
                    keymap_error: keymap.and_then(|m| m.error.clone()),
                }
            })
            .collect();

        Snapshot {
            devices: self.devices.views(&config, &keymaps),
            keyboards,
            pinned: crate::popover::pinned(),
            config_path: self.config_path.display().to_string(),
            config_error: self.config_error.read().unwrap().clone(),
            title_format: config.title_format.clone(),
            layer_width: config.layer_width,
            appearance: crate::appearance::effective(&config.appearance),
            popover_max_height: crate::popover::max_height(),
            layer_colors: crate::icon::LAYER_COLORS
                .iter()
                .map(|[r, g, b, _]| format!("#{r:02X}{g:02X}{b:02X}"))
                .collect(),
        }
    }

    /// Tells the tray and the windows that something changed.
    pub fn notify(&self) {
        let snapshot = self.snapshot();
        if std::env::var_os("KEYLAY_DEBUG").is_some() {
            eprintln!("keylay: {}", serde_json::to_string(&snapshot.devices).unwrap_or_default());
        }
        tray::refresh(&self.app, &snapshot);
        let _ = self.app.emit("snapshot", &snapshot);
    }
}

impl Sink for Hub {
    fn state(&self, from: &Source, report: Report) {
        if self.devices.state(from, report, now_ms()) {
            self.notify();
        }
    }

    fn batteries(&self, from: &Source, levels: Vec<Option<u8>>) {
        if self.devices.batteries(from, levels) {
            self.notify();
        }
    }

    /// Opens the cheat sheet on that keyboard (its config name, else its device name), or
    /// closes it.
    fn keylay_key(&self, from: &Source) {
        let config = self.config.read().unwrap();
        if !config.keylay_key {
            return;
        }
        let key = config.keyboard_for(&from.name, from.transport).map_or_else(|| from.name.clone(), |k| k.name.clone());
        drop(config);
        if std::env::var_os("KEYLAY_DEBUG").is_some() {
            eprintln!("keylay: KeyLay key on {} ({key})", from.id);
        }
        // Called from a reader thread; Tauri runs the window calls on the main thread itself
        crate::popover::toggle_for_keyboard(&self.app, Some(key));
    }

    fn gone(&self, from: &Source) {
        if self.devices.remove(&from.id) {
            self.notify();
        }
    }
}
