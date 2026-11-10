//! The user's config file (`config.json` in the app config directory).

use crate::hub::Transport;
use serde::{Deserialize, Serialize};
use std::path::Path;

const DEFAULT_CONFIG: &str = include_str!("../default-config.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// Menu bar text on macOS. `{keyboard}`, `{layer}` and `{battery}` are replaced.
    pub title_format: String,
    /// `{layer}` is cut or padded to this many characters, so the menu bar item keeps its
    /// width when the layer changes. 0 leaves the name as it is.
    pub layer_width: usize,
    pub keyboards: Vec<KeyboardConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct KeyboardConfig {
    /// Name shown in the app.
    pub name: String,
    /// Case-insensitive part of the USB product name or Bluetooth name.
    #[serde(rename = "match")]
    pub matches: String,
    /// "usb" or "ble" when two keyboards share a name; empty matches both.
    pub transport: String,
    /// keymap-drawer SVG: an http(s) URL or a file path (`~` allowed).
    pub keymap: String,
    /// Names of the batteries in the order the firmware reports them.
    pub battery_labels: Vec<String>,
    /// Layer names; when empty they are read from the keymap SVG.
    pub layer_names: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config { title_format: "{layer} {battery}".into(), layer_width: 6, keyboards: Vec::new() }
    }
}

impl Config {
    /// Reads the config file, writing the default one first if there is none. A file that
    /// does not parse gives the default config and the error.
    pub fn load_or_create(path: &Path) -> (Config, Option<String>) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Err(e) = std::fs::write(path, DEFAULT_CONFIG) {
                    eprintln!("keylay: cannot write {}: {e}", path.display());
                }
                DEFAULT_CONFIG.to_string()
            }
        };
        match serde_json::from_str(&text) {
            Ok(config) => (config, None),
            Err(e) => (
                serde_json::from_str(DEFAULT_CONFIG).unwrap_or_default(),
                Some(format!("{}: {e}", path.display())),
            ),
        }
    }

    pub fn keyboard_for(&self, device_name: &str, transport: Transport) -> Option<&KeyboardConfig> {
        let name = device_name.to_lowercase();
        self.keyboards.iter().find(|k| {
            !k.matches.is_empty()
                && name.contains(&k.matches.to_lowercase())
                && (k.transport.is_empty() || k.transport.eq_ignore_ascii_case(transport.as_str()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let config: Config = serde_json::from_str(DEFAULT_CONFIG).unwrap();
        let name = |device, transport| config.keyboard_for(device, transport).map(|k| k.name.as_str());
        assert_eq!(config.layer_width, 6);
        assert_eq!(name("Sweep Dongle", Transport::Ble), Some("Sweep"));
        assert_eq!(name("moNa2", Transport::Ble), Some("moNa2"));
        // The QMK and ZMK Charybdis share a name; the link tells them apart.
        assert_eq!(name("Charybdis Nano (QMK)", Transport::Usb), Some("Charybdis QMK"));
        assert_eq!(name("Charybdis Nano", Transport::Ble), Some("Charybdis ZMK"));
        assert_eq!(name("MX Master 3", Transport::Ble), None);
    }
}
