//! The menu bar / tray icon. macOS shows the keyboard glyph plus a text title ("Lower 87%");
//! Windows has no tray text, so it shows a layer badge and puts the details in the tooltip.

use crate::hub::{Hub, Snapshot};
use crate::{icon, popover};
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

pub const TRAY_ID: &str = "keylay";

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Cheat Sheet", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload Config and Keymaps", true, None::<&str>)?;
    let config = MenuItem::with_id(app, "config", "Open Config File", true, None::<&str>)?;
    let login_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let login = CheckMenuItem::with_id(app, "login", "Launch at Login", true, login_enabled, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit KeyLay", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
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
            "config" => crate::open_in_editor(&app.state::<Arc<Hub>>().config_path),
            "login" => {
                let autolaunch = app.autolaunch();
                let enable = !autolaunch.is_enabled().unwrap_or(false);
                let result = if enable { autolaunch.enable() } else { autolaunch.disable() };
                if let Err(e) = result {
                    eprintln!("keylay: launch at login: {e}");
                }
                let _ = login.set_checked(autolaunch.is_enabled().unwrap_or(false));
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

/// What the tray shows; kept so the icon is only replaced when it changes.
#[derive(PartialEq, Clone)]
struct Shown {
    title: String,
    tooltip: String,
    badge: Option<(String, u8, Option<u8>)>,
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

pub fn refresh(app: &AppHandle, snapshot: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let active = snapshot.devices.first();

    let title = active
        .map(|d| title(&snapshot.title_format, snapshot.layer_width, d))
        .unwrap_or_default();
    let tooltip = tooltip(snapshot);
    let badge = active.and_then(|d| Some((d.layer_name.clone()?, d.layer?, d.lowest_battery())));
    let shown = Shown { title, tooltip, badge };

    let mut last = SHOWN.lock().unwrap();
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

/// Padding that survives as width at the end of the title (a plain space may not), and that
/// the space cleanup below leaves alone.
const PAD: char = '\u{00A0}';

fn title(format: &str, layer_width: usize, device: &crate::hub::DeviceView) -> String {
    let layer = device.layer_name.as_deref().map(|l| fit(l, layer_width)).unwrap_or_default();
    // Always 4 characters ("  5%", " 62%", "100%"), so the width holds as the battery drains.
    let battery = device
        .lowest_battery()
        .map(|b| format!("{b:>3}%").replace(' ', &PAD.to_string()))
        .unwrap_or_default();
    // A keyboard without layer reports shows its name, so the title is not just "87%".
    let format = if device.layer.is_none() && !format.contains("{keyboard}") {
        format!("{{keyboard}} {format}")
    } else {
        format.to_string()
    };
    format
        .replace("{keyboard}", device.display_name())
        .replace("{layer}", &layer)
        .replace("{battery}", &battery)
        .split(' ')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Cuts or pads a layer name to `width` characters: "Lower" becomes "Lower ", "Colemak"
/// becomes "Colem…". With the monospaced menu bar font that is a fixed width.
fn fit(name: &str, width: usize) -> String {
    if width == 0 {
        return name.to_string();
    }
    let len = name.chars().count();
    if len > width {
        name.chars().take(width - 1).chain(std::iter::once('…')).collect()
    } else {
        name.chars().chain(std::iter::repeat_n(PAD, width - len)).collect()
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
    use crate::hub::{BatteryView, DeviceView, Transport};

    fn device(layer: Option<(&str, u8)>, batteries: &[Option<u8>]) -> DeviceView {
        DeviceView {
            id: "x".into(),
            name: "Sweep Dongle".into(),
            transport: Transport::Ble,
            keyboard: Some("Sweep".into()),
            layer: layer.map(|l| l.1),
            layer_name: layer.map(|l| l.0.to_string()),
            layer_mask: 1,
            batteries: batteries.iter().map(|&level| BatteryView { label: "b".into(), level }).collect(),
        }
    }

    #[test]
    fn layer_names_take_six_characters() {
        assert_eq!(fit("Lower", 6), "Lower\u{a0}");
        assert_eq!(fit("Adjust", 6), "Adjust");
        assert_eq!(fit("Colemak", 6), "Colem…");
        assert_eq!(fit("Colemak", 0), "Colemak");
    }

    #[test]
    fn title_has_the_same_length_for_every_layer() {
        let fmt = "{layer} {battery}";
        let a = title(fmt, 6, &device(Some(("yoma", 0)), &[Some(100), Some(62)]));
        let b = title(fmt, 6, &device(Some(("Scroll", 8)), &[Some(5)]));
        assert_eq!(a, "yoma\u{a0}\u{a0} \u{a0}62%");
        assert_eq!(b, "Scroll \u{a0}\u{a0}5%");
        assert_eq!(a.chars().count(), b.chars().count());
    }

    #[test]
    fn title_without_battery_or_layer() {
        assert_eq!(title("{layer} {battery}", 6, &device(Some(("Base", 0)), &[])), "Base\u{a0}\u{a0}");
        assert_eq!(title("{layer} {battery}", 6, &device(None, &[Some(87)])), "Sweep \u{a0}87%");
    }
}
