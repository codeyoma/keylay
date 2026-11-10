//! The macOS menu bar title, in the menu bar's size but a monospaced font, so the item keeps
//! its width when the layer changes (with the layer name padded to a fixed length).

use objc2::runtime::AnyObject;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSColor, NSFont, NSFontAttributeName, NSForegroundColorAttributeName};
use objc2_foundation::{NSAttributedString, NSDictionary, NSString};
use tauri::tray::TrayIcon;

pub fn set_title(tray: &TrayIcon, text: String) {
    let _ = tray.with_inner_tray_icon(move |inner| {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let Some(item) = inner.ns_status_item() else { return };
        let Some(button) = item.button(mtm) else { return };

        let size = NSFont::menuBarFontOfSize(0.0).pointSize();
        // Weight 0.0 is NSFontWeightRegular
        let font = NSFont::monospacedSystemFontOfSize_weight(size, 0.0);
        // A dynamic color, so the text follows the menu bar's light or dark appearance
        let color = NSColor::labelColor();
        let objects: [&AnyObject; 2] = [&font, &color];
        let attributes = unsafe {
            NSDictionary::from_slices(&[NSFontAttributeName, NSForegroundColorAttributeName], &objects)
        };
        let title = unsafe { NSAttributedString::new_with_attributes(&NSString::from_str(&text), &attributes) };
        button.setAttributedTitle(&title);
    });
}
