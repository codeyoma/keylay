//! USB: QMK keyboards with the KeyLay raw HID hook (`firmware/qmk`).

use crate::hub::{Hub, Transport};
use crate::protocol::{self, Message, RAW_HID_USAGE, RAW_HID_USAGE_PAGE};
use hidapi::{HidApi, HidDevice};
use std::collections::HashSet;
use std::ffi::CString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SCAN_INTERVAL: Duration = Duration::from_secs(3);
/// How often to ask a raw HID device that has not answered yet (e.g. a VIA keyboard
/// without the hook, or one that was still booting).
const REQUERY_INTERVAL: Duration = Duration::from_secs(30);

pub fn start(hub: Arc<Hub>) {
    std::thread::Builder::new()
        .name("keylay-hid".into())
        .spawn(move || scan(hub))
        .expect("spawn HID thread");
}

fn scan(hub: Arc<Hub>) {
    let mut api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("keylay: HID unavailable: {e}");
            return;
        }
    };
    // Share the device with other apps (VIA, Vial, QMK Toolbox).
    #[cfg(target_os = "macos")]
    api.set_open_exclusive(false);

    let open: Arc<Mutex<HashSet<CString>>> = Arc::default();
    loop {
        if api.refresh_devices().is_ok() {
            let found: Vec<(CString, String)> = api
                .device_list()
                .filter(|d| d.usage_page() == RAW_HID_USAGE_PAGE && d.usage() == RAW_HID_USAGE)
                .map(|d| (d.path().to_owned(), d.product_string().unwrap_or("USB keyboard").to_string()))
                .collect();
            for (path, name) in found {
                if open.lock().unwrap().contains(&path) {
                    continue;
                }
                match api.open_path(&path) {
                    Ok(device) => {
                        open.lock().unwrap().insert(path.clone());
                        let (hub, open) = (hub.clone(), open.clone());
                        std::thread::spawn(move || {
                            read(&device, &path, &name, &hub);
                            open.lock().unwrap().remove(&path);
                        });
                    }
                    Err(e) => eprintln!("keylay: cannot open {name}: {e}"),
                }
            }
        }
        std::thread::sleep(SCAN_INTERVAL);
    }
}

/// Reads state reports until the device goes away.
fn read(device: &HidDevice, path: &CString, name: &str, hub: &Hub) {
    let id = format!("usb:{}", path.to_string_lossy());
    let mut buf = [0u8; 64];
    let mut answered = false;
    let mut last_query: Option<Instant> = None;
    loop {
        if !answered && last_query.is_none_or(|t| t.elapsed() >= REQUERY_INTERVAL) {
            if device.write(&protocol::get_state_request()).is_err() {
                break;
            }
            last_query = Some(Instant::now());
        }
        match device.read_timeout(&mut buf, 1000) {
            Ok(0) => {}
            Ok(len) => match protocol::parse_message(&buf[..len]) {
                Some(Message::State(report)) => {
                    answered = true;
                    hub.report(&id, name, Transport::Usb, report);
                }
                Some(Message::TogglePopover) => hub.toggle_popover(&id),
                None => {}
            },
            Err(_) => break,
        }
    }
    hub.remove(&id);
}
