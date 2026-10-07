//! proxy firmware: self-contained ESP32-C3 GAN cube gateway.
//!
//! Dual BLE role in one firmware:
//!   - **Central**: scan + connect a physical cube (GAN Gen2/Gen3/Gen4, MoYu
//!     WCU/MHC, QiYi), establish link encryption, subscribe and decode its
//!     notifications into protocol-agnostic [`CubeEvent`]s.
//!   - **Peripheral**: advertise as a virtual GAN cube and serve the decoded
//!     moves to a phone app.
//!
//! Build from this directory (its .cargo/config.toml selects the proxy sdkconfig):
//!   cd proxy && cargo build

mod central;
mod server;

use esp_idf_svc::log::EspLogger;
use esp32_nimble::BLEDevice;

use shared::device_config::{self, Protocol};
use shared::protocol::CubeEvent;

use crate::central::{find_cube, subscribe_and_decode, EventSink, InputProto};
use crate::server::VirtualCube;

fn main() {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();
    log::info!("=== gan-proxy: self-contained cube gateway (Central + Peripheral) ===");

    esp_idf_svc::hal::task::block_on(async {
        let ble_device = BLEDevice::take();

        // Peripheral first: advertise the virtual cube so a phone can connect.
        let vcube = VirtualCube::setup(ble_device, *device_config::get_config(Protocol::Gen2).mac);
        let vcube_sink = vcube.clone();

        // Central: find, connect and identify the physical cube.
        let (addr, name) = find_cube(ble_device).await;
        log::info!("Connecting to {addr} ('{name}')...");
        let mut client = ble_device.new_client();
        if let Err(e) = client.connect(&addr).await {
            log::error!("connect failed: {e:?}");
            return;
        }
        log::info!("Connected to {addr}");

        // Identify the input protocol by GATT service UUID (authoritative).
        let proto: InputProto = match central::detect_protocol(&mut client).await {
            Some(p) => p,
            None => {
                log::error!("could not identify cube protocol (no known service UUID)");
                return;
            }
        };
        log::info!("Detected input protocol: {}", proto.label());

        // GAN/MoYu cubes require link encryption before characteristic access.
        if !matches!(proto, InputProto::MoyuMhc) {
            if let Err(e) = client.secure_connection().await {
                log::warn!("secure_connection failed: {e:?} (continuing)");
            }
        }

        // MAC in display order (used for key derivation by the central module).
        let mac = addr.as_be_bytes();

        // Forward every decoded event to the virtual cube.
        let sink: EventSink = Box::new(move |ev: CubeEvent| {
            let names = ["U", "R", "F", "D", "L", "B"];
            match ev {
                CubeEvent::Move { serial, moves } => {
                    if let Some(m0) = moves.first() {
                        if m0.face < 6 {
                            log::info!(
                                "MOVE serial={} {}{} ({}ms)",
                                serial,
                                names[m0.face as usize],
                                if m0.dir == 0 { "" } else { "'" },
                                m0.elapsed
                            );
                            vcube_sink.push_move(m0.face, m0.dir);
                        }
                    }
                }
                CubeEvent::Facelets { serial, .. } => {
                    log::info!("FACELETS serial={} (physical cube snapshot)", serial);
                }
                CubeEvent::Battery { serial, level } => {
                    log::info!("BATTERY serial={} level={}%", serial, level);
                }
                CubeEvent::Gyro { .. } => {}
                CubeEvent::Other { kind, .. } => {
                    log::debug!("OTHER kind=0x{:02X}", kind);
                }
            }
        });

        if let Err(e) = subscribe_and_decode(&mut client, proto, mac, sink).await {
            log::error!("subscribe/decode failed: {e}");
            return;
        }

        log::info!("Gateway running: rotate the cube; connect your phone to the virtual cube.");

        loop {
            embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
        }
    });
}
