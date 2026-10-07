//! proxy firmware: ESP32-C3 GATT Central.
//!
//! Stage 2: scan + connect a physical GAN cube, establish link encryption,
//! subscribe to the state characteristic, then **decrypt** each notification
//! (GAN Gen2/V1 AES-CBC, key/IV derived from the cube MAC) and decode it into
//! moves / facelets.
//!
//! Build from this directory (its .cargo/config.toml selects the proxy sdkconfig):
//!   cd proxy && cargo build

use esp_idf_svc::log::EspLogger;
use esp32_nimble::utilities::BleUuid;
use esp32_nimble::{BLEDevice, BLEScan};

use shared::crypto;
use shared::device_config::{self, Protocol};
use shared::protocol::{self, Gen2Message};

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

        log::info!("Scanning 15s — advertising every device seen (wake the cube!):");

        let addr = scan
            .start(ble_device, 15000, |device, data| {
                let name = name_of(&data);
                let svc: Vec<String> = data.service_uuids().map(|u| u.to_string()).collect();
                log::info!(
                    "  seen addr={} rssi={} name='{}' svcs={:?}",
                    device.addr(),
                    device.rssi(),
                    name,
                    svc
                );
                if matches_gan(&data) {
                    log::info!("  => GAN cube candidate! name='{}'", name);
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

/// Decrypt a GAN Gen2/V1 notification and log the decoded event.
fn handle_notify(data: &[u8], key: &[u8; 16], iv: &[u8; 16]) {
    if data.len() < 16 {
        log::warn!("short notify ({} bytes): {:02X?}", data.len(), data);
        return;
    }
    let mut buf = data.to_vec();
    crypto::decrypt_msg(&mut buf, key, iv);
    log::debug!("raw={:02X?} dec={:02X?}", data, buf);
    match protocol::decode_gen2(&buf) {
        Gen2Message::Move { serial, moves } => {
            let names = ["U", "R", "F", "D", "L", "B"];
            // Slot 0 is the newest move; following slots are history. Only report
            // slots with a valid face.
            let mut s = String::new();
            for m in moves.iter().take(7) {
                if m.face >= 6 {
                    break;
                }
                if !s.is_empty() {
                    s.push(' ');
                }
                s.push_str(names[m.face as usize]);
                if m.dir != 0 {
                    s.push('\'');
                }
            }
            if let Some(m0) = moves.first() {
                log::info!("MOVE serial={} [{}] newest_elapsed={}ms", serial, s, m0.elapsed);
            }
        }
        Gen2Message::Facelets { serial, .. } => {
            log::info!("FACELETS serial={} (state snapshot)", serial);
        }
        Gen2Message::Battery { serial, level } => {
            log::info!("BATTERY serial={} level={}%", serial, level);
        }
        Gen2Message::Gyro { qw, qx, qy, qz } => {
            log::debug!("GYRO qw={} qx={} qy={} qz={}", qw, qx, qy, qz);
        }
        Gen2Message::Other { kind, raw } => {
            log::debug!("OTHER kind=0x{:02X} raw={:02X?}", kind, raw);
        }
    }
}

fn main() {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();
    log::info!("=== gan-proxy: ESP32-C3 BLE Central (GATT client) — decrypt + decode ===");

    esp_idf_svc::hal::task::block_on(async {
        let ble_device = BLEDevice::take();

        let addr = match find_cube(ble_device).await {
            Ok(a) => a,
            Err(e) => {
                log::error!("scan failed: {e}");
                return;
            }
        };

        // Derive the GAN Gen2 key/IV from the cube's MAC. `derive_key_iv`
        // expects the MAC in **display order** (e.g. AA:BB:CC:DD:EE:FF) and
        // reverses it internally as the salt.
        let mac = addr.as_be_bytes();
        let cfg = device_config::get_config(Protocol::Gen2);
        let (key, iv) = crypto::derive_key_iv(&mac, cfg.base_key, cfg.base_iv);
        log::info!("Cube MAC {:02X?} -> key {:02X?} iv {:02X?}", mac, key, iv);

        log::info!("Connecting to {addr}...");
        let mut client = ble_device.new_client();
        if let Err(e) = client.connect(&addr).await {
            log::error!("connect failed: {e:?}");
            return;
        }
        log::info!("Connected to {addr}");

        log::info!("Initiating security (link encryption)...");
        match client.secure_connection().await {
            Ok(()) => log::info!("Security established (encrypted link)"),
            Err(e) => log::warn!("secure_connection failed: {e:?} (continuing anyway)"),
        }

        // Decrypt + decode each notification and log the decoded event.
        // Only subscribe to the protocol's STATE characteristic (the OTA notify
        // char in the OTA service must be ignored).
        let state_uuid = BleUuid::from_uuid128_string(device_config::get_config(Protocol::Gen2).state_uuid).unwrap();
        match client.get_services().await {
            Ok(services) => {
                for svc in services {
                    log::info!("Service {}", svc.uuid());
                    match svc.get_characteristics().await {
                        Ok(chars) => {
                            for chr in chars {
                                log::info!("  Char {} props={:?}", chr.uuid(), chr.properties());
                                if chr.can_notify() && chr.uuid() == state_uuid {
                                    chr.on_notify(move |data: &[u8]| {
                                        handle_notify(data, &key, &iv)
                                    });
                                    match chr.subscribe_notify(false).await {
                                        Ok(()) => log::info!("    subscribed (state notify)"),
                                        Err(e) => log::warn!("    subscribe failed: {e:?}"),
                                    }
                                }
                            }
                        }
                        Err(e) => log::warn!("  char discovery failed: {e:?}"),
                    }
                }
            }
            Err(e) => log::error!("service discovery failed: {e:?}"),
        }

        log::info!("Running; rotate the cube to see decoded moves.");

        loop {
            embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
        }
    });
}
