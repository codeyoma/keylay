//! Window background: glass (a native material the desktop shows through) or solid.

use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{AppHandle, Manager};

/// "glass" or "solid": solid when chosen, or while Reduce transparency is on (macOS).
pub fn effective(choice: &str) -> &'static str {
    if choice == "solid" || reduce_transparency() {
        "solid"
    } else {
        "glass"
    }
}

#[cfg(target_os = "macos")]
fn reduce_transparency() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
}

#[cfg(not(target_os = "macos"))]
fn reduce_transparency() -> bool {
    false
}

/// Puts the material behind (or takes it away from) the popover and the settings window.
/// The pages paint their own solid background in "solid".
pub fn apply(app: &AppHandle, mode: &str) {
    for label in [crate::popover::LABEL, crate::settings::LABEL] {
        let Some(window) = app.get_webview_window(label) else { continue };
        let _ = window.set_effects(material(mode));
    }
}

pub fn material(mode: &str) -> Option<tauri::utils::config::WindowEffectsConfig> {
    if mode == "solid" {
        return None;
    }
    #[cfg(target_os = "macos")]
    let effect = Effect::Popover;
    #[cfg(target_os = "windows")]
    let effect = Effect::Acrylic;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return None;
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    Some(EffectsBuilder::new().effect(effect).state(EffectState::Active).radius(24.0).build())
}
