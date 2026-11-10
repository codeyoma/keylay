// No console window on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appearance;
mod ble;
mod config;
mod hid;
mod hub;
mod icon;
mod keymap;
#[cfg(target_os = "macos")]
mod menubar;
mod popover;
mod protocol;
mod settings;
mod title;
mod tray;

use hub::{Hub, Snapshot};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, State};


#[tauri::command]
fn get_config(hub: State<'_, Arc<Hub>>) -> config::Config {
    hub.config()
}

#[tauri::command]
fn save_config(hub: State<'_, Arc<Hub>>, config: config::Config) -> Result<(), String> {
    hub.inner().save_config(config)
}

/// The layer names in a keymap URL or file, for the settings window
#[tauri::command]
async fn check_keymap(source: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || keymap::probe(&source))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_login(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_login(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    let result = if enabled { autolaunch.enable() } else { autolaunch.disable() };
    result.map_err(|e| e.to_string())
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    popover::hide(&app);
    settings::open(&app);
}

/// The menu bar title for a format, for the settings preview
#[tauri::command]
fn preview_title(hub: State<'_, Arc<Hub>>, format: String, layer_width: usize) -> String {
    title::preview(&format, layer_width, hub.snapshot().devices.first())
}

#[tauri::command]
fn get_snapshot(hub: State<'_, Arc<Hub>>) -> Snapshot {
    hub.snapshot()
}

#[tauri::command]
fn get_keymap_svg(hub: State<'_, Arc<Hub>>, keyboard: String) -> Option<String> {
    hub.keymap_svg(&keyboard)
}

#[tauri::command]
async fn reload(hub: State<'_, Arc<Hub>>) -> Result<(), String> {
    let hub = hub.inner().clone();
    tauri::async_runtime::spawn_blocking(move || hub.reload()).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn open_config(hub: State<'_, Arc<Hub>>) {
    open_in_editor(&hub.config_path);
}

pub fn open_in_editor(path: &Path) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg("-t").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("notepad").arg(path).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = result {
        eprintln!("keylay: cannot open {}: {e}", path.display());
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            preview_title,
            get_keymap_svg,
            reload,
            open_config,
            get_config,
            save_config,
            check_keymap,
            get_login,
            set_login,
            open_settings,
            popover::commands::set_pinned,
            popover::commands::hide_popover,
            popover::commands::fit_popover
        ])
        .setup(|app| {
            // A menu bar app: no Dock icon
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            popover::init(&app.path().app_config_dir()?.join("window.json"));
            let config_path = app.path().app_config_dir()?.join("config.json");
            let cache_dir = app.path().app_cache_dir()?.join("keymaps");
            let hub = Arc::new(Hub::new(handle.clone(), config_path, cache_dir));
            app.manage(hub.clone());

            appearance::apply(&handle, appearance::effective(&hub.config().appearance));
            tray::build(&handle)?;
            tray::refresh(&handle, &hub.snapshot());

            hub.start_keymap_checks();
            hid::start(hub.clone());
            ble::start(hub.clone());
            // Debugging aid: open the popover right away, centered and pinned (so it does not
            // hide when another app has the focus)
            if std::env::var_os("KEYLAY_SHOW_SETTINGS").is_some() {
                let handle = handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    let app = handle.clone();
                    let _ = handle.run_on_main_thread(move || settings::open(&app));
                });
            }
            if std::env::var_os("KEYLAY_SHOW_POPOVER").is_some() {
                popover::pin_for_this_run();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    let app = handle.clone();
                    let _ = handle.run_on_main_thread(move || popover::show(&app, None, None));
                });
            }
            Ok(())
        })
        .on_window_event(popover::on_window_event)
        .build(tauri::generate_context!())
        .expect("error while building KeyLay")
        .run(|_app, event| {
            // Keep running with no window open.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
