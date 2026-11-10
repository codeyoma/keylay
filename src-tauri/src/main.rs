// No console window on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
mod tray;

use hub::{Hub, Snapshot};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager, State, WindowEvent};

const KEYMAP_CHECK_INTERVAL: Duration = Duration::from_secs(15 * 60);

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

#[tauri::command]
fn set_pinned(hub: State<'_, Arc<Hub>>, pinned: bool) {
    hub.set_pinned(pinned);
}

#[tauri::command]
fn hide_popover(app: AppHandle) {
    if let Some(window) = app.get_webview_window(popover::LABEL) {
        popover::hide(&window);
    }
}

#[tauri::command]
fn fit_popover(app: AppHandle, height: f64) {
    if let Some(window) = app.get_webview_window(popover::LABEL) {
        popover::fit_height(&window, height);
    }
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
            get_keymap_svg,
            reload,
            open_config,
            set_pinned,
            hide_popover,
            fit_popover
        ])
        .setup(|app| {
            // A menu bar app: no Dock icon
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let config_path = app.path().app_config_dir()?.join("config.json");
            let cache_dir = app.path().app_cache_dir()?.join("keymaps");
            let hub = Arc::new(Hub::new(handle.clone(), config_path, cache_dir));
            app.manage(hub.clone());

            tray::build(&handle)?;
            tray::refresh(&handle, &hub.snapshot());

            // Keymaps: the cached copies first, then a check with GitHub every 15 minutes
            // (and when the popover opens, see popover::show)
            {
                let hub = hub.clone();
                std::thread::spawn(move || loop {
                    hub.refresh_keymaps();
                    std::thread::sleep(KEYMAP_CHECK_INTERVAL);
                });
            }
            hid::start(hub.clone());
            ble::start(hub.clone());
            // Debugging aid: open the popover right away, centered and pinned (so it does not
            // hide when another app has the focus)
            if std::env::var_os("KEYLAY_SHOW_POPOVER").is_some() {
                hub.set_pinned(true);
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(1500));
                    let app = handle.clone();
                    let _ = handle.run_on_main_thread(move || popover::show(&app, None));
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Behave like a popover: hide when another app is clicked, unless pinned.
            WindowEvent::Focused(false) if window.label() == popover::LABEL => {
                let pinned = window.state::<Arc<Hub>>().pinned();
                if !pinned {
                    if let Some(w) = window.app_handle().get_webview_window(popover::LABEL) {
                        popover::hide(&w);
                    }
                }
            }
            // Closing the window only hides it; the app lives in the tray.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
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
