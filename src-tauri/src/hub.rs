//! Shared state: the connected keyboards, the config and the keymaps. Every change refreshes
//! the tray and sends a `snapshot` event to the popover.

use crate::config::{Config, KeyboardConfig};
use crate::keymap::{self, Keymap};
use crate::protocol::Report;
use crate::tray;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Usb,
    Ble,
}

impl Transport {
    pub fn as_str(self) -> &'static str {
        match self {
            Transport::Usb => "usb",
            Transport::Ble => "ble",
        }
    }
}

#[derive(Debug, Clone)]
struct Device {
    name: String,
    transport: Transport,
    /// None for a keyboard that only reports batteries (standard Battery Service).
    layer: Option<u8>,
    layer_mask: u32,
    batteries: Vec<Option<u8>>,
    /// When the layer last changed (ms since the epoch); the latest one is shown in the tray.
    last_change: u64,
}

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
pub struct DeviceView {
    pub id: String,
    /// What the windows call it: the config name, else the device name
    pub key: String,
    pub name: String,
    pub transport: Transport,
    /// Name of the matching keyboard in the config, if any.
    pub keyboard: Option<String>,
    pub layer: Option<u8>,
    pub layer_name: Option<String>,
    pub layer_mask: u32,
    pub batteries: Vec<BatteryView>,
    /// "Bluetooth · Lower", or "USB · battery only" for a keyboard without layer reports
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryView {
    pub label: String,
    pub level: Option<u8>,
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

impl DeviceView {
    /// Name used in the tray: the config name, else the device name.
    pub fn display_name(&self) -> &str {
        &self.key
    }

    pub fn lowest_battery(&self) -> Option<u8> {
        self.batteries.iter().filter_map(|b| b.level).min()
    }
}

pub struct Hub {
    app: AppHandle,
    pub config_path: PathBuf,
    cache_dir: PathBuf,
    config: RwLock<Config>,
    config_error: RwLock<Option<String>>,
    keymaps: RwLock<HashMap<String, Keymap>>,
    /// One keymap check at a time, and when the last one ran
    keymap_check: Mutex<Option<Instant>>,
    devices: Mutex<HashMap<String, Device>>,
}

impl Hub {
    pub fn new(app: AppHandle, config_path: PathBuf, cache_dir: PathBuf) -> Hub {
        migrate_old_config(&config_path);
        let (config, config_error) = Config::load_or_create(&config_path);
        Hub {
            app,
            config_path,
            cache_dir,
            config: RwLock::new(config),
            config_error: RwLock::new(config_error),
            keymaps: RwLock::new(HashMap::new()),
            keymap_check: Mutex::new(None),
            devices: Mutex::new(HashMap::new()),
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

    /// Checks every keymap against its source (a 304 from GitHub when nothing changed), then
    /// tells the tray and the popover, which also shows when the check ran. Blocks on the
    /// network.
    pub fn refresh_keymaps(&self) {
        // Holding the lock for the whole check keeps two checks from running at once
        let mut last_check = self.keymap_check.lock().unwrap();
        let keyboards = self.config.read().unwrap().keyboards.clone();
        let previous = self.keymaps.read().unwrap().clone();
        let keymaps: HashMap<String, Keymap> = keyboards
            .iter()
            .map(|k| (k.name.clone(), keymap::refresh(k, previous.get(&k.name), &self.cache_dir).0))
            .collect();
        *self.keymaps.write().unwrap() = keymaps;
        *last_check = Some(Instant::now());
        drop(last_check);
        self.notify();
    }

    /// Checks the keymaps in the background unless that was done in the last `max_age`.
    pub fn refresh_keymaps_if_older_than(self: &Arc<Self>, max_age: Duration) {
        let fresh = match self.keymap_check.try_lock() {
            Ok(last) => last.is_some_and(|t| t.elapsed() < max_age),
            Err(_) => true, // a check is running
        };
        if !fresh {
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
        self.keymaps.read().unwrap().get(keyboard).and_then(|k| k.svg.clone())
    }

    /// A state message from a keyboard with the KeyLay firmware hook.
    pub fn report(&self, id: &str, name: &str, transport: Transport, report: Report) {
        let changed = {
            let mut devices = self.devices.lock().unwrap();
            let device = devices.entry(id.to_string()).or_insert_with(|| new_device(name, transport));
            let layer_changed = device.layer != Some(report.layer) || device.layer_mask != report.layer_mask;
            if layer_changed {
                device.last_change = now_ms();
            }
            let changed = layer_changed || device.batteries != report.batteries;
            device.layer = Some(report.layer);
            device.layer_mask = report.layer_mask;
            device.batteries = report.batteries;
            changed
        };
        if changed {
            self.notify();
        }
    }

    /// Battery levels from the standard Battery Service, for a keyboard without the hook.
    pub fn batteries(&self, id: &str, name: &str, transport: Transport, batteries: Vec<Option<u8>>) {
        let changed = {
            let mut devices = self.devices.lock().unwrap();
            let device = devices.entry(id.to_string()).or_insert_with(|| new_device(name, transport));
            let changed = device.batteries != batteries;
            device.batteries = batteries;
            changed
        };
        if changed {
            self.notify();
        }
    }

    /// The KeyLay key on a keyboard: opens the cheat sheet on that keyboard, or closes it.
    pub fn toggle_popover(&self, id: &str) {
        if !self.config.read().unwrap().keylay_key {
            return;
        }
        let key = {
            let devices = self.devices.lock().unwrap();
            let config = self.config.read().unwrap();
            devices.get(id).map(|d| {
                config.keyboard_for(&d.name, d.transport).map_or_else(|| d.name.clone(), |k| k.name.clone())
            })
        };
        if std::env::var_os("KEYLAY_DEBUG").is_some() {
            eprintln!("keylay: KeyLay key on {id} ({})", key.as_deref().unwrap_or("?"));
        }
        // Called from a reader thread; Tauri runs the window calls on the main thread itself
        crate::popover::toggle_for_keyboard(&self.app, key);
    }

    pub fn remove(&self, id: &str) {
        if self.devices.lock().unwrap().remove(id).is_some() {
            self.notify();
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let config = self.config.read().unwrap();
        let keymaps = self.keymaps.read().unwrap();
        let devices = self.devices.lock().unwrap();

        let mut sorted: Vec<(&String, &Device)> = devices.iter().collect();
        // Keyboards that report layers first, then the one whose layer changed last.
        sorted.sort_by(|(a_id, a), (b_id, b)| {
            b.layer
                .is_some()
                .cmp(&a.layer.is_some())
                .then(b.last_change.cmp(&a.last_change))
                .then(a_id.cmp(b_id))
        });

        let views = sorted
            .into_iter()
            .map(|(id, d)| {
                let keyboard = config.keyboard_for(&d.name, d.transport);
                let layers = keyboard.and_then(|k| keymaps.get(&k.name)).map(|k| k.layers.as_slice());
                device_view(id, d, keyboard, layers)
            })
            .collect();

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
            devices: views,
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

/// A connected keyboard as the tray and the windows show it: matched to its keyboard in the
/// config, with layer and battery names.
fn device_view(id: &str, d: &Device, keyboard: Option<&KeyboardConfig>, layers: Option<&[String]>) -> DeviceView {
    let layer_name = d.layer.map(|l| {
        layers
            .and_then(|names| names.get(l as usize))
            .cloned()
            .unwrap_or_else(|| format!("Layer {l}"))
    });
    let labels = keyboard.map(|k| k.battery_labels.as_slice()).unwrap_or(&[]);
    let batteries = d
        .batteries
        .iter()
        .enumerate()
        .map(|(i, &level)| BatteryView {
            label: labels.get(i).cloned().unwrap_or_else(|| default_battery_label(i, d.batteries.len())),
            level,
        })
        .collect();
    let link = match d.transport {
        Transport::Usb => "USB",
        Transport::Ble => "Bluetooth",
    };
    let status = match &layer_name {
        Some(layer) => format!("{link} · {layer}"),
        None => format!("{link} · battery only"),
    };
    let keyboard = keyboard.map(|k| k.name.clone());
    DeviceView {
        id: id.to_string(),
        key: keyboard.clone().unwrap_or_else(|| d.name.clone()),
        name: d.name.clone(),
        transport: d.transport,
        keyboard,
        layer: d.layer,
        layer_name,
        layer_mask: d.layer_mask,
        batteries,
        status,
    }
}

fn new_device(name: &str, transport: Transport) -> Device {
    Device {
        name: name.to_string(),
        transport,
        layer: None,
        layer_mask: 0,
        batteries: Vec::new(),
        last_change: now_ms(),
    }
}

fn default_battery_label(index: usize, count: usize) -> String {
    match (index, count) {
        (_, 1) => "Battery".into(),
        (0, _) => "Central".into(),
        (i, _) => format!("Peripheral {i}"),
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// The app was called KeyTray before; its config folder was `dev.yoma.keytray`. Copies that
/// config over the first time KeyLay starts, so the user's settings carry over.
fn migrate_old_config(config_path: &std::path::Path) {
    if config_path.exists() {
        return;
    }
    let Some(dir) = config_path.parent() else { return };
    let Some(old) = dir.parent().map(|p| p.join("dev.yoma.keytray").join("config.json")) else { return };
    if old.exists() {
        let _ = std::fs::create_dir_all(dir);
        if let Err(e) = std::fs::copy(&old, config_path) {
            eprintln!("keylay: cannot copy {}: {e}", old.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, transport: Transport, layer: Option<u8>, batteries: &[Option<u8>]) -> Device {
        Device { layer, batteries: batteries.to_vec(), ..new_device(name, transport) }
    }

    fn keyboard(name: &str, labels: &[&str]) -> KeyboardConfig {
        KeyboardConfig {
            name: name.into(),
            battery_labels: labels.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_configured_keyboard_uses_its_names() {
        let layers = ["Base".to_string(), "Lower".to_string()];
        let d = device("Sweep Dongle", Transport::Ble, Some(1), &[Some(80), Some(40)]);
        let view = device_view("a", &d, Some(&keyboard("Sweep", &["Left", "Right"])), Some(&layers));
        assert_eq!(view.key, "Sweep");
        assert_eq!(view.layer_name.as_deref(), Some("Lower"));
        assert_eq!(view.status, "Bluetooth · Lower");
        assert_eq!(view.batteries[1].label, "Right");
        assert_eq!(view.lowest_battery(), Some(40));
    }

    #[test]
    fn an_unknown_keyboard_falls_back_to_device_names() {
        let d = device("Corne", Transport::Usb, Some(3), &[]);
        let view = device_view("b", &d, None, None);
        assert_eq!(view.key, "Corne");
        assert_eq!(view.layer_name.as_deref(), Some("Layer 3"));
        assert_eq!(view.status, "USB · Layer 3");
    }

    #[test]
    fn a_keyboard_without_layer_reports_is_battery_only() {
        let d = device("Keychron", Transport::Ble, None, &[Some(90), Some(70)]);
        let view = device_view("c", &d, None, None);
        assert_eq!(view.status, "Bluetooth · battery only");
        assert_eq!(view.batteries[0].label, "Central");
        assert_eq!(view.batteries[1].label, "Peripheral 1");
    }
}
