mod ble;
mod http;
mod serial;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use shared::device_config::Protocol;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};

const NVS_NS_PROTO: &str = "proto";
const NVS_KEY_PROTO: &str = "active";

const NVS_NS_MOYU: &str = "moyu";
const NVS_KEY_ACCOUNT: &str = "account";
const NVS_KEY_NAME: &str = "name";
const NVS_KEY_GYRO: &str = "gyro_open";

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

/// Load the persisted WCU bind account (0 = unbound).
pub fn load_moyu_account(nvs: &EspDefaultNvsPartition) -> u32 {
    if let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_MOYU, false) {
        let mut buf = [0u8; 4];
        if let Ok(Some(data)) = ns.get_blob(NVS_KEY_ACCOUNT, &mut buf) {
            return u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        }
    }
    0
}

pub fn save_moyu_account(nvs: &EspDefaultNvsPartition, account: u32) {
    if let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_MOYU, true) {
        let _ = ns.set_blob(NVS_KEY_ACCOUNT, &account.to_le_bytes());
    }
}

/// Load the persisted device name. Falls back to `default_name`.
pub fn load_moyu_name(nvs: &EspDefaultNvsPartition, default_name: &str) -> String {
    if let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_MOYU, false) {
        let mut buf = [0u8; 32];
        if let Ok(Some(data)) = ns.get_blob(NVS_KEY_NAME, &mut buf) {
            if let Ok(s) = std::str::from_utf8(data) {
                let s = s.trim_end_matches('\0');
                if !s.is_empty() {
                    return s.to_string();
                }
            }
        }
    }
    default_name.to_string()
}

pub fn save_moyu_name(nvs: &EspDefaultNvsPartition, name: &str) {
    if let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_MOYU, true) {
        let _ = ns.set_blob(NVS_KEY_NAME, name.as_bytes());
    }
}

pub fn load_moyu_gyro(nvs: &EspDefaultNvsPartition) -> bool {
    if let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_MOYU, false) {
        let mut buf = [0u8; 1];
        if let Ok(Some(data)) = ns.get_blob(NVS_KEY_GYRO, &mut buf) {
            return data[0] == 1;
        }
    }
    false
}

pub fn save_moyu_gyro(nvs: &EspDefaultNvsPartition, open: bool) {
    if let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_MOYU, true) {
        let _ = ns.set_blob(NVS_KEY_GYRO, &[if open { 1 } else { 0 }]);
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

    let default_name = shared::device_config::get_config(Protocol::MoYu32).device_name;
    let account: Arc<Mutex<u32>> = Arc::new(Mutex::new(load_moyu_account(&nvs)));
    let cube_name: Arc<Mutex<String>> = Arc::new(Mutex::new(load_moyu_name(&nvs, default_name)));
    let gyro_open: Arc<Mutex<bool>> = Arc::new(Mutex::new(load_moyu_gyro(&nvs)));
    log::info!("MoYu32 account: {}, name: {}", *account.lock().unwrap(), *cube_name.lock().unwrap());

    let queue: Arc<Mutex<VecDeque<(u8, u8)>>> = Arc::new(Mutex::new(VecDeque::new()));
    let cube_state: Arc<Mutex<shared::protocol::CubeState>> = Arc::new(Mutex::new(shared::protocol::CubeState::solved()));
    let move_delay_ms: Arc<Mutex<u64>> = Arc::new(Mutex::new(http::load_move_delay(&nvs)));

    let _wifi = shared::wifi::start_wifi(peripherals.modem, sys_loop, nvs.clone()).unwrap();
    let _server = http::start_http_server(
        queue.clone(), _wifi.clone(), nvs.clone(),
        cube_state.clone(), move_delay_ms.clone(), protocol.clone(),
    ).unwrap();
    serial::run_serial(queue.clone(), cube_state.clone());
    shared::wifi::sta_reconnect_loop(_wifi.clone(), nvs.clone());

    esp_idf_svc::hal::task::block_on(ble::run(
        queue, cube_state, move_delay_ms, protocol, account, cube_name, gyro_open, nvs,
    ));
}
