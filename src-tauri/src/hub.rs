//! Shared state: the config, the keymaps and the connected keyboards (`devices`). Every change refreshes
//! the tray and sends a `snapshot` event to the popover.

use crate::config::Config;
use crate::devices::{now_ms, DeviceView, Devices, Transport};
use crate::keymap;
use crate::intake::{Sink, Source};
use crate::protocol::Report;
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
    /// The layer names shown: the keymap's, with the config's renames applied
    pub layers: Vec<String>,
    /// The keymap's own layer names, so the settings window can tell renames apart
    pub keymap_layers: Vec<String>,
    pub has_keymap: bool,
    pub keymap_error: Option<String>,
}

/// Where a config change came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigOrigin {
    /// config.json, read again (Reload)
    File,
    /// the settings window's own save
    Settings,
}

/// The `config` event: the config now in use.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigChanged {
    pub config: Config,
    pub origin: ConfigOrigin,
}

/// Something that shows the snapshot: the tray, the windows. Hub sends every new snapshot to
/// each one, in the order they subscribed.
pub trait Subscriber: Send + Sync {
    fn show(&self, snapshot: &Snapshot);
}

/// The windows get the snapshot as a `snapshot` event.
pub struct Windows(pub AppHandle);

impl Subscriber for Windows {
    fn show(&self, snapshot: &Snapshot) {
        let _ = self.0.emit("snapshot", snapshot);
    }
}

/// What the cheat sheet window adds to the snapshot (it remembers them, see popover).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WindowFacts {
    pub pinned: bool,
    /// The height the user gave the cheat sheet; "All" grows up to it
    pub max_height: Option<f64>,
}

pub struct Hub {
    app: AppHandle,
    pub config_path: PathBuf,
    config: RwLock<Config>,
    config_error: RwLock<Option<String>>,
    keymaps: keymap::Store,
    devices: Devices,
    window: RwLock<WindowFacts>,
    subscribers: RwLock<Vec<Box<dyn Subscriber>>>,
}

impl Hub {
    pub fn new(app: AppHandle, config_path: PathBuf, cache_dir: PathBuf, window: WindowFacts) -> Hub {
        let (config, config_error) = Config::load_or_create(&config_path);
        Hub {
            app,
            config_path,
            config: RwLock::new(config),
            config_error: RwLock::new(config_error),
            keymaps: keymap::Store::new(cache_dir),
            devices: Devices::default(),
            window: RwLock::new(window),
            subscribers: RwLock::new(Vec::new()),
        }
    }

    /// Re-reads the config file and checks every keymap. Blocks on the network.
    pub fn reload(&self) {
        let (config, error) = Config::load_or_create(&self.config_path);
        self.adopt(config, error, ConfigOrigin::File);
        self.refresh_keymaps();
    }

    /// Saves a config from the settings window, then checks the keymaps against it.
    pub fn save_config(self: &Arc<Self>, config: Config) -> Result<(), String> {
        config.save(&self.config_path)?;
        self.adopt(config, None, ConfigOrigin::Settings);
        let hub = self.clone();
        std::thread::spawn(move || hub.refresh_keymaps());
        Ok(())
    }

    /// Every config change goes through here: it puts the config in place, makes the windows
    /// look the way it says, and tells the windows and the tray. The settings window takes
    /// over a config read from the file, so its next save keeps the file's edits.
    fn adopt(&self, config: Config, error: Option<String>, origin: ConfigOrigin) {
        *self.config.write().unwrap() = config;
        *self.config_error.write().unwrap() = error;
        self.apply_appearance();
        let _ = self.app.emit("config", ConfigChanged { config: self.config(), origin });
        self.notify();
    }

    /// Glass or solid for the open windows, from the config (and Reduce transparency).
    pub fn apply_appearance(&self) {
        let mode = crate::appearance::effective(&self.config.read().unwrap().appearance);
        crate::appearance::apply(&self.app, mode);
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

    pub fn is_configured(&self, device_name: &str, transport: Transport) -> bool {
        self.config.read().unwrap().keyboard_for(device_name, transport).is_some()
    }

    pub fn keymap_svg(&self, keyboard: &str) -> Option<String> {
        self.keymaps.svg(keyboard)
    }

    pub fn snapshot(&self) -> Snapshot {
        let window = *self.window.read().unwrap();
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
                    layers: k.shown_layers(keymap.map_or(&[][..], |m| m.layers.as_slice())),
                    keymap_layers: keymap.map(|m| m.layers.clone()).unwrap_or_default(),
                    has_keymap: keymap.is_some_and(|m| m.svg.is_some()),
                    keymap_error: keymap.and_then(|m| m.error.clone()),
                }
            })
            .collect();

        Snapshot {
            devices: self.devices.views(&config, &keymaps),
            keyboards,
            pinned: window.pinned,
            config_path: self.config_path.display().to_string(),
            config_error: self.config_error.read().unwrap().clone(),
            title_format: config.title_format.clone(),
            layer_width: config.layer_width,
            appearance: crate::appearance::effective(&config.appearance),
            popover_max_height: window.max_height,
            layer_colors: crate::icon::LAYER_COLORS
                .iter()
                .map(|[r, g, b, _]| format!("#{r:02X}{g:02X}{b:02X}"))
                .collect(),
        }
    }

    /// Adds something that shows the snapshot; it gets the current one right away, so it does
    /// not matter what changed before it subscribed.
    pub fn subscribe(&self, subscriber: Box<dyn Subscriber>) {
        subscriber.show(&self.snapshot());
        self.subscribers.write().unwrap().push(subscriber);
    }

    /// The cheat sheet window's pin or saved height changed.
    pub fn set_window(&self, facts: WindowFacts) {
        let changed = {
            let mut window = self.window.write().unwrap();
            let changed = *window != facts;
            *window = facts;
            changed
        };
        if changed {
            self.notify();
        }
    }

    /// Sends a new snapshot to every subscriber.
    pub fn notify(&self) {
        let snapshot = self.snapshot();
        if std::env::var_os("KEYLAY_DEBUG").is_some() {
            eprintln!("keylay: {}", serde_json::to_string(&snapshot.devices).unwrap_or_default());
        }
        for subscriber in self.subscribers.read().unwrap().iter() {
            subscriber.show(&snapshot);
        }
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
