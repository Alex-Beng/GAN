//! proxy firmware: self-contained ESP32-C3 cube gateway.
//!
//! Dual BLE role + WiFi configuration in one firmware:
//!   - **Central**: scan + connect a physical cube (GAN Gen2/Gen3/Gen4, MoYu
//!     WCU/MHC, QiYi), establish link encryption, subscribe and decode its
//!     notifications into protocol-agnostic [`CubeEvent`]s.
//!   - **Peripheral**: advertise as a virtual cube of the configured output
//!     protocol and serve the decoded moves to a phone app.
//!   - **WiFi + HTTP**: shared config UI (AP/STA + `/api/protocol`). Changing
//!     the output protocol persists to NVS and reboots.
//!
//! Build from this directory (its .cargo/config.toml selects the proxy sdkconfig):
//!   cd proxy && cargo build

mod central;
mod config;
mod server;

use std::sync::{Arc, Mutex};

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp32_nimble::BLEDevice;

use shared::config_http::{self, ConfigHttpDeps};
use shared::device_config::Protocol;
use shared::protocol::{CubeEvent, CubeState};

use crate::central::{find_cube, subscribe_and_decode, EventSink, InputProto};
use crate::server::VirtualCube;

fn main() {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    // Dynamic frequency scaling (like emu).
    unsafe {
        esp_idf_svc::sys::esp_pm_configure(
            &esp_idf_svc::sys::esp_pm_config_t {
                max_freq_mhz: 160,
                min_freq_mhz: 40,
                light_sleep_enable: false,
            } as *const _ as *mut _,
        );
    }

    let peripherals = Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    // Configured output protocol (reboot-selected via /api/protocol).
    let output_proto: Arc<Mutex<Protocol>> =
        Arc::new(Mutex::new(config::load_output_protocol(&nvs)));
    let out = *output_proto.lock().unwrap();
    log::info!("=== gan-proxy gateway === output protocol: {:?}", out);

    // WiFi (AP/STA) + shared HTTP config server.
    let wifi = shared::wifi::start_wifi(peripherals.modem, sys_loop, nvs.clone()).unwrap();
    shared::wifi::sta_reconnect_loop(wifi.clone(), nvs.clone());

    let cube_state: Arc<Mutex<CubeState>> = Arc::new(Mutex::new(CubeState::solved()));
    let nvs_switch = nvs.clone();
    let on_change: Box<dyn Fn(Protocol) + Send + Sync> = Box::new(move |p| {
        log::info!("output protocol -> {:?}, persisting and restarting", p);
        config::save_output_protocol(&nvs_switch, p);
        // Give the HTTP response a moment to flush, then reboot into the new
        // protocol (same approach as emu: rebuild the BLE session on restart).
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            unsafe { esp_idf_svc::sys::esp_restart() };
        });
    });
    let _server = config_http::start(ConfigHttpDeps {
        wifi: wifi.clone(),
        nvs: nvs.clone(),
        cube_state: cube_state.clone(),
        protocol: output_proto.clone(),
        on_protocol_change: Some(on_change),
    })
    .unwrap();

    esp_idf_svc::hal::task::block_on(async move {
        let ble_device = BLEDevice::take();

        // Peripheral: advertise the virtual cube in the configured protocol.
        // The own-address type MUST be Public so the advertised BLE address
        // equals the MAC we advertise in manufacturer data (and use for crypto
        // key derivation). The app derives its key from that MAC, so all three
        // must match (same as emu).
        let mac = *shared::device_config::get_config(out).mac;
        ble_device.set_own_addr_type(esp32_nimble::enums::OwnAddrType::Public);
        // Set the BT MAC to the protocol's advertised MAC (real GAN cubes are
        // identified by it; matches emu behaviour).
        unsafe {
            esp_idf_svc::sys::esp_iface_mac_addr_set(
                mac.as_ptr(),
                esp_idf_svc::sys::esp_mac_type_t_ESP_MAC_BT,
            );
        }
        let vcube = VirtualCube::setup(ble_device, out, mac);
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

        let proto: InputProto = match central::detect_protocol(&mut client).await {
            Some(p) => p,
            None => {
                log::error!("could not identify cube protocol (no known service UUID)");
                return;
            }
        };
        log::info!("Detected input protocol: {}", proto.label());

        if !matches!(proto, InputProto::MoyuMhc) {
            if let Err(e) = client.secure_connection().await {
                log::warn!("secure_connection failed: {e:?} (continuing)");
            }
        }

        let mac_in = addr.as_be_bytes();

        let sink: EventSink = Box::new(move |ev: CubeEvent| {
            let names = ["U", "R", "F", "D", "L", "B"];
            match ev {
                CubeEvent::Move { serial, moves } => {
                    // Re-emit the newest move only.
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

        if let Err(e) = subscribe_and_decode(&mut client, proto, mac_in, sink).await {
            log::error!("subscribe/decode failed: {e}");
            return;
        }

        log::info!("Gateway running: rotate the cube; connect your phone to the virtual cube.");

        loop {
            embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
        }
    });
}
