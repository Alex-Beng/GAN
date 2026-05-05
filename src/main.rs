mod ble;
mod crypto;
mod device_config;
mod http;
mod protocol;
mod wifi;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    let queue: Arc<Mutex<VecDeque<(u8, u8)>>> = Arc::new(Mutex::new(VecDeque::new()));
    let cube_state: Arc<Mutex<protocol::CubeState>> = Arc::new(Mutex::new(protocol::CubeState::solved()));

    let _wifi = wifi::start_wifi(peripherals.modem, sys_loop, nvs).unwrap();
    let _server = http::start_http_server(queue.clone(), _wifi.clone(), cube_state.clone()).unwrap();

    esp_idf_svc::hal::task::block_on(ble::run(queue, cube_state));
}
