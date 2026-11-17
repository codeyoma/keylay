//! Launch at login: one place that changes it and tells both places that show it, the tray
//! menu's check mark and the settings window's switch (a `login` event).

use std::sync::Mutex;
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Emitter, Wry};
use tauri_plugin_autostart::ManagerExt;

/// The tray menu's check item, kept to update its check mark
static MENU_ITEM: Mutex<Option<CheckMenuItem<Wry>>> = Mutex::new(None);

pub fn enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// The tray menu's "Launch at Login" item, checked as it is now.
pub fn menu_item(app: &AppHandle) -> tauri::Result<CheckMenuItem<Wry>> {
    let item = CheckMenuItem::with_id(app, "login", "Launch at Login", true, enabled(app), None::<&str>)?;
    *MENU_ITEM.lock().unwrap() = Some(item.clone());
    Ok(item)
}

/// Turns launch at login on or off, then shows the result (the actual state, also when the
/// change failed) in the tray menu and the settings window.
pub fn set(app: &AppHandle, enable: bool) -> Result<(), String> {
    let autolaunch = app.autolaunch();
    let result = if enable { autolaunch.enable() } else { autolaunch.disable() };
    let now = enabled(app);
    if let Some(item) = MENU_ITEM.lock().unwrap().as_ref() {
        let _ = item.set_checked(now);
    }
    let _ = app.emit("login", now);
    result.map_err(|e| e.to_string())
}

pub mod commands {
    use tauri::AppHandle;

    #[tauri::command]
    pub fn get_login(app: AppHandle) -> bool {
        super::enabled(&app)
    }

    #[tauri::command]
    pub fn set_login(app: AppHandle, enabled: bool) -> Result<(), String> {
        super::set(&app, enabled)
    }
}
