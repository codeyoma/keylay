//! The connected keyboards: what each one last reported, and how the tray and the windows
//! show them (matched to the config, with layer and battery names).

use crate::config::{Config, KeyboardConfig};
use crate::intake::Source;
use crate::keymap::Keymap;
use crate::protocol::Report;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

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

impl DeviceView {
    /// Name used in the tray: the config name, else the device name.
    pub fn display_name(&self) -> &str {
        &self.key
    }

    pub fn lowest_battery(&self) -> Option<u8> {
        self.batteries.iter().filter_map(|b| b.level).min()
    }
}


/// The connected keyboards, by connection id.
#[derive(Default)]
pub struct Devices(Mutex<HashMap<String, Device>>);

impl Devices {
    /// A state message: the layer and batteries. `now` (ms since the epoch) dates a layer
    /// change. Returns whether anything shown changed.
    pub fn state(&self, from: &Source, report: Report, now: u64) -> bool {
        let mut devices = self.0.lock().unwrap();
        let device = devices.entry(from.id.clone()).or_insert_with(|| new_device(&from.name, from.transport, now));
        let layer_changed = device.layer != Some(report.layer) || device.layer_mask != report.layer_mask;
        if layer_changed {
            device.last_change = now;
        }
        let changed = layer_changed || device.batteries != report.batteries;
        device.layer = Some(report.layer);
        device.layer_mask = report.layer_mask;
        device.batteries = report.batteries;
        changed
    }

    /// Battery levels alone (a keyboard without the KeyLay hook). Returns whether they changed.
    pub fn batteries(&self, from: &Source, levels: Vec<Option<u8>>) -> bool {
        let mut devices = self.0.lock().unwrap();
        let device = devices.entry(from.id.clone()).or_insert_with(|| new_device(&from.name, from.transport, now_ms()));
        let changed = device.batteries != levels;
        device.batteries = levels;
        changed
    }

    /// Returns whether the keyboard was there.
    pub fn remove(&self, id: &str) -> bool {
        self.0.lock().unwrap().remove(id).is_some()
    }

    /// The keyboards as the tray and the windows show them: those that report layers first,
    /// then the one whose layer changed last (the tray shows the first).
    pub fn views(&self, config: &Config, keymaps: &HashMap<String, Keymap>) -> Vec<DeviceView> {
        let devices = self.0.lock().unwrap();
        let mut sorted: Vec<(&String, &Device)> = devices.iter().collect();
        sorted.sort_by(|(a_id, a), (b_id, b)| {
            b.layer
                .is_some()
                .cmp(&a.layer.is_some())
                .then(b.last_change.cmp(&a.last_change))
                .then(a_id.cmp(b_id))
        });
        sorted
            .into_iter()
            .map(|(id, d)| {
                let keyboard = config.keyboard_for(&d.name, d.transport);
                let layers = keyboard.and_then(|k| keymaps.get(&k.name)).map(|k| k.layers.as_slice());
                device_view(id, d, keyboard, layers)
            })
            .collect()
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

fn new_device(name: &str, transport: Transport, now: u64) -> Device {
    Device {
        name: name.to_string(),
        transport,
        layer: None,
        layer_mask: 0,
        batteries: Vec::new(),
        last_change: now,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, transport: Transport, layer: Option<u8>, batteries: &[Option<u8>]) -> Device {
        Device { layer, batteries: batteries.to_vec(), ..new_device(name, transport, 0) }
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

    fn source(id: &str, name: &str) -> Source {
        Source { id: id.into(), name: name.into(), transport: Transport::Usb }
    }

    fn report(layer: u8) -> Report {
        Report { layer, layer_mask: 1 << layer, batteries: vec![] }
    }

    #[test]
    fn the_keyboard_whose_layer_changed_last_comes_first() {
        let devices = Devices::default();
        let config = Config::default();
        let keymaps = HashMap::new();
        assert!(devices.state(&source("a", "A"), report(0), 10));
        assert!(devices.state(&source("b", "B"), report(0), 20));
        assert!(devices.batteries(&source("c", "C"), vec![Some(50)]));
        let order = |d: &Devices| d.views(&config, &keymaps).into_iter().map(|v| v.id).collect::<Vec<_>>();
        assert_eq!(order(&devices), ["b", "a", "c"]);

        // The same layer again changes nothing
        assert!(!devices.state(&source("a", "A"), report(0), 30));
        assert_eq!(order(&devices), ["b", "a", "c"]);
        // A new layer on A moves it to the front; battery-only C stays last
        assert!(devices.state(&source("a", "A"), report(2), 40));
        assert_eq!(order(&devices), ["a", "b", "c"]);

        assert!(devices.remove("a"));
        assert!(!devices.remove("a"));
        assert_eq!(order(&devices), ["b", "c"]);
    }
}
