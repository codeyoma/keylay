//! Bluetooth: ZMK keyboards with the KeyLay GATT service (`firmware/zmk`), and the standard
//! Battery Service of keyboards in the config that do not have it.
//!
//! Keyboards are already connected to the OS, so they do not advertise. They are found with
//! `connected_devices_with_services` and the app opens its own logical connection.

use crate::hub::{Hub, Transport};
use crate::protocol::{self, BLE_SERVICE, BLE_STATE_CHARACTERISTIC};
use bluest::btuuid::{characteristics::BATTERY_LEVEL, services::BATTERY};
use bluest::{Adapter, Characteristic, Device, DeviceId};
use futures_lite::StreamExt;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const SCAN_INTERVAL: Duration = Duration::from_secs(5);
const CONNECTION_CHECK_INTERVAL: Duration = Duration::from_secs(10);
const BATTERY_POLL_INTERVAL: Duration = Duration::from_secs(60);

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn start(hub: Arc<Hub>) {
    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(e) = scan(hub.clone()).await {
                eprintln!("keylay: Bluetooth: {e}");
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });
}

async fn scan(hub: Arc<Hub>) -> Result<()> {
    let adapter = Adapter::default().await.ok_or("no Bluetooth adapter")?;
    adapter.wait_available().await?;
    let watched: Arc<Mutex<HashSet<DeviceId>>> = Arc::default();
    loop {
        for device in adapter.connected_devices_with_services(&[BLE_SERVICE, BATTERY]).await? {
            if !watched.lock().unwrap().insert(device.id()) {
                continue;
            }
            let (adapter, hub, watched) = (adapter.clone(), hub.clone(), watched.clone());
            tauri::async_runtime::spawn(async move {
                let id = format!("ble:{}", device.id());
                if let Err(e) = watch(&adapter, &device, &id, &hub).await {
                    eprintln!("keylay: {id}: {e}");
                }
                hub.remove(&id);
                watched.lock().unwrap().remove(&device.id());
            });
        }
        tokio::time::sleep(SCAN_INTERVAL).await;
    }
}

/// Follows one device until it disconnects.
async fn watch(adapter: &Adapter, device: &Device, id: &str, hub: &Hub) -> Result<()> {
    adapter.connect_device(device).await?;
    let name = device.name().unwrap_or_else(|_| "Bluetooth keyboard".into());

    if let Some(state) = find_characteristic(device, BLE_SERVICE, BLE_STATE_CHARACTERISTIC).await? {
        return follow_state(device, &state, id, &name, hub).await;
    }

    // No KeyLay service: show the battery levels, but only for keyboards in the config, so
    // mice and headphones stay out.
    if !hub.is_configured(&name, Transport::Ble) {
        return wait_for_disconnect(device).await;
    }
    follow_batteries(device, id, &name, hub).await
}

async fn follow_state(device: &Device, state: &Characteristic, id: &str, name: &str, hub: &Hub) -> Result<()> {
    if let Some(report) = protocol::parse(&state.read().await?) {
        hub.report(id, name, Transport::Ble, report);
    }
    let mut updates = state.notify().await?;
    let mut check = tokio::time::interval(CONNECTION_CHECK_INTERVAL);
    loop {
        tokio::select! {
            update = updates.next() => match update {
                Some(Ok(value)) => {
                    if let Some(report) = protocol::parse(&value) {
                        hub.report(id, name, Transport::Ble, report);
                    }
                }
                Some(Err(e)) => return Err(e.into()),
                None => return Ok(()),
            },
            _ = check.tick() => {
                if !device.is_connected().await {
                    return Ok(());
                }
            }
        }
    }
}

/// With CONFIG_ZMK_SPLIT_BLE_CENTRAL_BATTERY_LEVEL_PROXY a split keyboard has one Battery
/// Level characteristic per half, central first.
async fn follow_batteries(device: &Device, id: &str, name: &str, hub: &Hub) -> Result<()> {
    let mut levels: Vec<Characteristic> = Vec::new();
    for service in device.discover_services_with_uuid(BATTERY).await? {
        levels.extend(service.discover_characteristics_with_uuid(BATTERY_LEVEL).await?);
    }
    if levels.is_empty() {
        return wait_for_disconnect(device).await;
    }
    loop {
        let mut batteries = Vec::with_capacity(levels.len());
        for level in &levels {
            batteries.push(level.read().await.ok().and_then(|v| v.first().copied()).filter(|&l| l <= 100));
        }
        hub.batteries(id, name, Transport::Ble, batteries);
        tokio::time::sleep(BATTERY_POLL_INTERVAL).await;
        if !device.is_connected().await {
            return Ok(());
        }
    }
}

async fn wait_for_disconnect(device: &Device) -> Result<()> {
    while device.is_connected().await {
        tokio::time::sleep(CONNECTION_CHECK_INTERVAL).await;
    }
    Ok(())
}

async fn find_characteristic(
    device: &Device,
    service: bluest::Uuid,
    characteristic: bluest::Uuid,
) -> Result<Option<Characteristic>> {
    for service in device.discover_services_with_uuid(service).await? {
        if let Some(c) = service.discover_characteristics_with_uuid(characteristic).await?.into_iter().next() {
            return Ok(Some(c));
        }
    }
    Ok(None)
}
