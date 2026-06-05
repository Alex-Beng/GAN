mod ble;
mod crypto;
mod device_config;
mod http;
mod protocol;
mod serial;
mod wifi;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use device_config::Protocol;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};

const NVS_NS_PROTO: &str = "proto";
const NVS_KEY_PROTO: &str = "active";

fn load_protocol(nvs: &EspDefaultNvsPartition) -> Protocol {
    if let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_PROTO, false) {
        let mut buf = [0u8; 1];
        if let Ok(Some(data)) = ns.get_blob(NVS_KEY_PROTO, &mut buf) {
            return Protocol::from_index(data[0]).unwrap_or(Protocol::Gen2);
        }
    }
    Protocol::Gen2
}

pub fn save_protocol(nvs: &EspDefaultNvsPartition, proto: Protocol) {
    if let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_PROTO, true) {
        let _ = ns.set_blob(NVS_KEY_PROTO, &[proto as u8]);
    }
}

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    unsafe {
        esp_idf_svc::sys::esp_pm_configure(
            &esp_idf_svc::sys::esp_pm_config_t {
                max_freq_mhz: 160,
                min_freq_mhz: 40,
                light_sleep_enable: false,
            } as *const _ as *mut _
        );
    }
    log::info!("Power management: dynamic freq 160/40MHz enabled");

    let peripherals = Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    let protocol: Arc<Mutex<Protocol>> = Arc::new(Mutex::new(load_protocol(&nvs)));
    log::info!("Loaded protocol: {:?}", *protocol.lock().unwrap());

    let queue: Arc<Mutex<VecDeque<(u8, u8)>>> = Arc::new(Mutex::new(VecDeque::new()));
    let cube_state: Arc<Mutex<protocol::CubeState>> = Arc::new(Mutex::new(protocol::CubeState::solved()));
    let move_delay_ms: Arc<Mutex<u64>> = Arc::new(Mutex::new(http::load_move_delay(&nvs)));

    let _wifi = wifi::start_wifi(peripherals.modem, sys_loop, nvs.clone()).unwrap();
    let _server = http::start_http_server(
        queue.clone(), _wifi.clone(), nvs.clone(),
        cube_state.clone(), move_delay_ms.clone(), protocol.clone(),
    ).unwrap();
    serial::run_serial(queue.clone(), cube_state.clone());
    wifi::sta_reconnect_loop(_wifi.clone(), nvs);

    esp_idf_svc::hal::task::block_on(ble::run(queue, cube_state, move_delay_ms, protocol));
}
