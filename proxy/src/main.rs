//! proxy firmware: self-contained ESP32-C3 GAN cube gateway.
//!
//! Dual BLE role in one firmware:
//!   - **Central**: scan + connect a physical GAN cube, establish link
//!     encryption, subscribe to its state characteristic and decrypt the
//!     Gen2 (V1) notifications.
//!   - **Peripheral**: advertise as a virtual GAN cube and serve the decoded
//!     moves to a phone app (GAN Gen2 GATT server).
//!
//! Build from this directory (its .cargo/config.toml selects the proxy sdkconfig):
//!   cd proxy && cargo build

mod server;

use esp_idf_svc::log::EspLogger;
use esp32_nimble::utilities::BleUuid;
use esp32_nimble::{BLEDevice, BLEScan};

use shared::crypto;
use shared::device_config::{self, Protocol};
use shared::protocol::{self, Gen2Message};

use crate::server::VirtualCube;

/// Known GAN cube service UUIDs (must match shared::device_config).
const GAN_SERVICE_UUIDS: &[&str] = &[
    "6e400001-b5a3-f393-e0a9-e50e24dc4179", // Gen2 (V1)
    "8653000a-43e6-47b7-9cb0-5fc21d4ae340", // Gen3 (V2)
    "00000010-0000-fff7-fff6-fff5fff4fff0", // Gen4 (V3) / Gan251 (V3-2)
];

fn name_of(data: &esp32_nimble::BLEAdvertisedData<&[u8]>) -> String {
    data.name()
        .and_then(|n| core::str::from_utf8(n.as_ref()).ok())
        .unwrap_or("")
        .to_string()
}

fn matches_gan(data: &esp32_nimble::BLEAdvertisedData<&[u8]>) -> bool {
    for uuid_str in GAN_SERVICE_UUIDS {
        let target = BleUuid::from_uuid128_string(uuid_str).unwrap();
        if data.is_advertising_service(&target) {
            return true;
        }
    }
    let name = name_of(data).to_uppercase();
    name.starts_with("GAN") || name.starts_with("GANI")
}

async fn find_cube(ble_device: &BLEDevice) -> anyhow::Result<esp32_nimble::BLEAddress> {
    loop {
        let mut scan = BLEScan::new();
        scan.active_scan(true)
            .interval(100)
            .window(100)
            .filter_duplicates(false);

        log::info!("Scanning 15s for a physical cube (wake it up!)...");

        let addr = scan
            .start(ble_device, 15000, |device, data| {
                let name = name_of(&data);
                if matches_gan(&data) {
                    log::info!(
                        "  => GAN cube candidate! addr={} name='{}' rssi={}",
                        device.addr(),
                        name,
                        device.rssi()
                    );
                    return Some(device.addr());
                }
                None
            })
            .await?;

        if let Some(a) = addr {
            return Ok(a);
        }
        log::info!("No GAN cube yet; rescanning...");
    }
}

/// Decrypt a GAN Gen2/V1 notification, then re-emit decoded moves to the
/// virtual cube server.
fn handle_notify(data: &[u8], key: &[u8; 16], iv: &[u8; 16], vcube: &VirtualCube) {
    if data.len() < 16 {
        log::warn!("short notify ({} bytes): {:02X?}", data.len(), data);
        return;
    }
    let mut buf = data.to_vec();
    crypto::decrypt_msg(&mut buf, key, iv);
    match protocol::decode_gen2(&buf) {
        Gen2Message::Move { serial, moves } => {
            let names = ["U", "R", "F", "D", "L", "B"];
            // Slot 0 is the newest move; the rest is history. Re-emit only the
            // newest move to the phone (each notification carries one new step).
            if let Some(m0) = moves.first() {
                if m0.face < 6 {
                    log::info!(
                        "MOVE serial={} {}{} ({}ms)",
                        serial,
                        names[m0.face as usize],
                        if m0.dir == 0 { "" } else { "'" },
                        m0.elapsed
                    );
                    vcube.push_move(m0.face, m0.dir);
                }
            }
        }
        Gen2Message::Facelets { serial, .. } => {
            log::info!("FACELETS serial={} (physical cube snapshot)", serial);
        }
        Gen2Message::Battery { serial, level } => {
            log::info!("BATTERY serial={} level={}%", serial, level);
        }
        Gen2Message::Gyro { .. } => {}
        Gen2Message::Other { kind, raw } => {
            log::debug!("OTHER kind=0x{:02X} raw={:02X?}", kind, raw);
        }
    }
}

fn main() {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();
    log::info!("=== gan-proxy: self-contained GAN cube gateway (Central + Peripheral) ===");

    esp_idf_svc::hal::task::block_on(async {
        let ble_device = BLEDevice::take();

        // Peripheral first: advertise the virtual cube so a phone can connect.
        let vcube = VirtualCube::setup(ble_device, *device_config::get_config(Protocol::Gen2).mac);
        let vcube_notify = vcube.clone();

        // Central: find and connect the physical cube.
        let addr = loop {
            match find_cube(ble_device).await {
                Ok(a) => break a,
                Err(e) => log::error!("scan error: {e}"),
            }
        };

        // Key/IV for the physical cube: `derive_key_iv` expects the MAC in
        // display order and reverses it internally as the salt.
        let mac = addr.as_be_bytes();
        let cfg = device_config::get_config(Protocol::Gen2);
        let (key, iv) = crypto::derive_key_iv(&mac, cfg.base_key, cfg.base_iv);
        log::info!("Physical cube MAC {:02X?}", mac);

        log::info!("Connecting to {addr}...");
        let mut client = ble_device.new_client();
        if let Err(e) = client.connect(&addr).await {
            log::error!("connect failed: {e:?}");
            return;
        }
        log::info!("Connected to {addr}");

        log::info!("Initiating security (link encryption)...");
        if let Err(e) = client.secure_connection().await {
            log::warn!("secure_connection failed: {e:?} (continuing anyway)");
        }

        // Subscribe only to the Gen2 state characteristic.
        let state_uuid =
            BleUuid::from_uuid128_string(cfg.state_uuid).unwrap();
        match client.get_services().await {
            Ok(services) => {
                for svc in services {
                    if let Ok(chars) = svc.get_characteristics().await {
                        for chr in chars {
                            if chr.can_notify() && chr.uuid() == state_uuid {
                                let vcube_cb = vcube_notify.clone();
                                chr.on_notify(move |data: &[u8]| {
                                    handle_notify(data, &key, &iv, &vcube_cb)
                                });
                                match chr.subscribe_notify(false).await {
                                    Ok(()) => log::info!("Subscribed to physical cube state"),
                                    Err(e) => log::warn!("subscribe failed: {e:?}"),
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => log::error!("service discovery failed: {e:?}"),
        }

        log::info!("Gateway running: rotate the cube; connect your phone to the virtual cube.");

        loop {
            embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
        }
    });
}
