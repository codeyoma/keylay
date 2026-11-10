//! The settings window: keyboards and general options, instead of editing config.json.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "settings";

pub fn open(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("settings.html".into()))
        .title("KeyLay Settings")
        .inner_size(880.0, 640.0)
        .min_inner_size(720.0, 520.0)
        .transparent(true)
        .focused(true);
    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    match builder.build() {
        Ok(window) => {
            // Glass or solid, from the setting; set after the webview exists, so the
            // material goes behind it
            let mode = crate::appearance::effective(&app.state::<std::sync::Arc<crate::hub::Hub>>().config().appearance);
            let _ = window.set_effects(crate::appearance::material(mode));
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("keylay: cannot open settings: {e}"),
    }
}
