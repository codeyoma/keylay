//! The menu bar / tray icon. macOS shows the keyboard glyph plus a text title ("Lower 87%");
//! Windows has no tray text, so it shows a layer badge and puts the details in the tooltip.

use crate::hub::{Hub, Snapshot};
use crate::{icon, popover, title};
use std::sync::{Arc, Mutex};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

pub const TRAY_ID: &str = "keylay";

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Cheat Sheet", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset-position", "Move Cheat Sheet Back to the Menu Bar", true, None::<&str>)?;
    let reload = MenuItem::with_id(app, "reload", "Reload Config and Keymaps", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let config = MenuItem::with_id(app, "config", "Open Config File", true, None::<&str>)?;
    let login_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let login = CheckMenuItem::with_id(app, "login", "Launch at Login", true, login_enabled, None::<&str>)?;
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
        .map(|d| title::title(&snapshot.title_format, snapshot.layer_width, d))
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
