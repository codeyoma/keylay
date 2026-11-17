//! The menu bar / tray icon. macOS shows the keyboard glyph plus a text title ("Lower 87%");
//! Windows has no tray text, so it shows a layer badge and puts the details in the tooltip.

use crate::hub::{Hub, Snapshot, Subscriber};
use crate::{icon, popover, title};
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub const TRAY_ID: &str = "keylay";

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Cheat Sheet", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset-position", "Move Cheat Sheet Back to the Menu Bar", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload Config and Keymaps", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let config = MenuItem::with_id(app, "config", "Open Config File", true, None::<&str>)?;
    let login = crate::login::menu_item(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit KeyLay", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &reset,
            &settings,
            &reload,
            &config,
            &login,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon::glyph())
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("KeyLay")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => popover::show(app, None, None),
            "reload" => {
                let hub = app.state::<Arc<Hub>>().inner().clone();
                std::thread::spawn(move || hub.reload());
            }
            "reset-position" => popover::reset_position(app),
            "settings" => crate::settings::open(app),
            "config" => crate::open_in_editor(&app.state::<Arc<Hub>>().config_path),
            "login" => {
                if let Err(e) = crate::login::set(app, !crate::login::enabled(app)) {
                    eprintln!("keylay: launch at login: {e}");
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                popover::toggle(tray.app_handle(), Some(rect));
            }
        })
        .build(app)?;
    Ok(())
}

/// What the tray shows.
#[derive(Debug, PartialEq, Clone)]
struct Shown {
    title: String,
    tooltip: String,
    badge: Option<(String, u8, Option<u8>)>,
}

/// The tray's title (macOS), tooltip and badge (Windows) for a snapshot.
fn shown(snapshot: &Snapshot) -> Shown {
    let active = snapshot.devices.first();
    Shown {
        title: active
            .map(|d| title::title(&snapshot.title_format, snapshot.layer_width, d))
            .unwrap_or_default(),
        tooltip: tooltip(snapshot),
        badge: active.and_then(|d| Some((d.layer_name.clone()?, d.layer?, d.lowest_battery()))),
    }
}

/// The tray as a snapshot subscriber; it changes the tray only when what it shows changes.
pub struct TrayView {
    app: AppHandle,
    last: Mutex<Option<Shown>>,
}

impl TrayView {
    /// After `build`: the tray icon must exist.
    pub fn new(app: AppHandle) -> TrayView {
        TrayView { app, last: Mutex::new(None) }
    }
}

impl Subscriber for TrayView {
    fn show(&self, snapshot: &Snapshot) {
        let Some(tray) = self.app.tray_by_id(TRAY_ID) else { return };
        let shown = shown(snapshot);
        let mut last = self.last.lock().unwrap();
        if last.as_ref() == Some(&shown) {
            return;
        }
        let _ = tray.set_tooltip(Some(&shown.tooltip));
        #[cfg(target_os = "macos")]
        crate::menubar::set_title(&tray, shown.title.clone());
        #[cfg(not(target_os = "macos"))]
        if last.as_ref().map(|l| &l.badge) != Some(&shown.badge) {
            let image = match &shown.badge {
                Some((name, layer, battery)) => icon::layer_badge(name, *layer, *battery),
                None => icon::glyph(),
            };
            let _ = tray.set_icon(Some(image));
        }
        *last = Some(shown);
    }
}

fn tooltip(snapshot: &Snapshot) -> String {
    if snapshot.devices.is_empty() {
        return "KeyLay: no keyboard".into();
    }
    snapshot
        .devices
        .iter()
        .map(|d| {
            let mut line = d.display_name().to_string();
            if let Some(layer) = &d.layer_name {
                line += &format!(": {layer}");
            }
            let batteries: Vec<String> = d
                .batteries
                .iter()
                .map(|b| match b.level {
                    Some(level) => format!("{} {level}%", b.label),
                    None => format!("{} -", b.label),
                })
                .collect();
            if !batteries.is_empty() {
                line += &format!(" ({})", batteries.join(", "));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::{BatteryView, DeviceView, Transport};

    fn snapshot(devices: Vec<DeviceView>) -> Snapshot {
        Snapshot {
            devices,
            keyboards: vec![],
            pinned: false,
            config_path: String::new(),
            config_error: None,
            title_format: "{layer} {battery}".into(),
            layer_width: 6,
            appearance: "glass",
            popover_max_height: None,
            layer_colors: vec![],
        }
    }

    fn device(key: &str, layer: Option<(&str, u8)>, battery: Option<u8>) -> DeviceView {
        DeviceView {
            id: key.into(),
            key: key.into(),
            name: key.into(),
            transport: Transport::Ble,
            keyboard: Some(key.into()),
            layer: layer.map(|l| l.1),
            layer_name: layer.map(|l| l.0.to_string()),
            layer_mask: 1,
            batteries: battery.map(|level| BatteryView { label: "Battery".into(), level: Some(level) }).into_iter().collect(),
            status: String::new(),
            link: "Bluetooth",
        }
    }

    #[test]
    fn the_tray_shows_the_first_keyboard() {
        let s = snapshot(vec![device("Sweep", Some(("Lower", 1)), Some(62)), device("Mona", None, Some(90))]);
        let shown = shown(&s);
        assert_eq!(shown.title, "Lower\u{a0} \u{a0}62%");
        assert_eq!(shown.badge, Some(("Lower".into(), 1, Some(62))));
        assert_eq!(shown.tooltip, "Sweep: Lower (Battery 62%)\nMona (Battery 90%)");
    }

    #[test]
    fn no_keyboard_shows_an_empty_title() {
        let shown = shown(&snapshot(vec![]));
        assert_eq!(shown.title, "");
        assert_eq!(shown.badge, None);
        assert_eq!(shown.tooltip, "KeyLay: no keyboard");
    }
}
