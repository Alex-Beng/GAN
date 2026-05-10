use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use embassy_time::{Duration, Timer};
use esp32_nimble::enums::{ConnMode, OwnAddrType};
use esp32_nimble::utilities::BleUuid;
use esp32_nimble::{BLEDevice, NimbleProperties};

use crate::crypto;
use crate::device_config::*;
use crate::protocol;

pub async fn run(queue: Arc<Mutex<VecDeque<(u8, u8)>>>, cube_state: Arc<Mutex<protocol::CubeState>>, move_delay_ms: Arc<Mutex<u64>>) {
    unsafe {
        esp_idf_svc::sys::esp_iface_mac_addr_set(
            MAC.as_ptr(),
            esp_idf_svc::sys::esp_mac_type_t_ESP_MAC_BT,
        );
    }

    let (base_key, base_iv) = if DEVICE_NAME.starts_with("AiCube") {
        (&AICUBE_BASE_KEY, &AICUBE_BASE_IV)
    } else {
        (&GAN_BASE_KEY, &GAN_BASE_IV)
    };
    let (key, iv) = crypto::derive_key_iv(&MAC, base_key, base_iv);
    let key = Arc::new(key);
    let iv = Arc::new(iv);

    let ble_device = BLEDevice::take();
    ble_device.set_own_addr_type(OwnAddrType::Public);
    BLEDevice::set_device_name(DEVICE_NAME).unwrap();

    let server = ble_device.get_server();

    let state_char_ref: Arc<Mutex<Option<Arc<esp32_nimble::utilities::mutex::Mutex<esp32_nimble::BLECharacteristic>>>>> =
        Arc::new(Mutex::new(None));
    let shared_serial = Arc::new(Mutex::new(0u8));
    let shared_serial_conn = shared_serial.clone();
    let connected = Arc::new(Mutex::new(false));
    let connected_conn = connected.clone();
    let connected_disc = connected.clone();

    server.on_connect(move |_server, _desc| {
        log::info!("Client connected");
        *connected_conn.lock().unwrap() = true;
    });

    server.on_disconnect(move |_desc, _reason| {
        *connected_disc.lock().unwrap() = false;
    });

    let service = server.create_service(BleUuid::from_uuid128_string(SERVICE_UUID).unwrap());

    let ota_service = server.create_service(BleUuid::from_uuid128_string(OTA_SERVICE_UUID).unwrap());

    let ota_version_char = ota_service.lock().create_characteristic(
        BleUuid::from_uuid128_string(OTA_VERSION_CHAR_UUID).unwrap(),
        NimbleProperties::READ,
    );
    ota_version_char.lock().set_value(OTA_VERSION);

    let ota_write_char = ota_service.lock().create_characteristic(
        BleUuid::from_uuid128_string(OTA_WRITE_CHAR_UUID).unwrap(),
        NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
    );
    ota_write_char.lock().on_write(move |args| {
        let data = args.recv_data();
        log::info!("OTA_WRITE raw ({} bytes): {:02X?}", data.len(), data);
    });

    let ota_notify_char = ota_service.lock().create_characteristic(
        BleUuid::from_uuid128_string(OTA_NOTIFY_CHAR_UUID).unwrap(),
        NimbleProperties::READ | NimbleProperties::NOTIFY,
    );
    ota_notify_char.lock().on_subscribe(move |_char, _desc, _cur_notify| {
        log::info!("OTA_NOTIFY subscribed");
    });

    let cmd_char = service.lock().create_characteristic(
        BleUuid::from_uuid128_string(CMD_CHAR_UUID).unwrap(),
        NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
    );

    let state_char = service.lock().create_characteristic(
        BleUuid::from_uuid128_string(STATE_CHAR_UUID).unwrap(),
        NimbleProperties::NOTIFY | NimbleProperties::READ,
    );

    *state_char_ref.lock().unwrap() = Some(state_char.clone());

    let key_sub = key.clone();
    let iv_sub = iv.clone();
    let shared_serial_sub = shared_serial_conn.clone();
    let state_char_sub = state_char.clone();
    let cube_state_sub = cube_state.clone();
    state_char.lock().on_subscribe(move |_char, _desc, cur_notify| {
        log::info!("Client subscribed, cur_notify={:?}", cur_notify);
        let serial = *shared_serial_sub.lock().unwrap();
        let cs = cube_state_sub.lock().unwrap();
        let mut msg = protocol::build_facelets(serial, &cs);
        drop(cs);
        crypto::encrypt_msg(&mut msg, &key_sub, &iv_sub);
        log::info!("Sending initial FACELETS: {:02X?}", &msg);
        state_char_sub.lock().set_value(&msg).notify();
    });

    let state_char_cmd = state_char.clone();
    let key_cmd = key.clone();
    let iv_cmd = iv.clone();
    let cube_state_cmd = cube_state.clone();
    let shared_serial_cmd = shared_serial.clone();

    cmd_char.lock().on_write(move |args| {
        let data = args.recv_data();
        log::info!("CMD raw ({} bytes): {:02X?}", data.len(), data);
        if data.len() < 20 {
            log::warn!("CMD too short, ignoring");
            return;
        }
        let mut buf = [0u8; 20];
        buf.copy_from_slice(&data[..20]);
        crypto::decrypt_msg(&mut buf, &key_cmd, &iv_cmd);

        let cmd_type = protocol::parse_command_type(&buf);
        log::info!("CMD type: 0x{:02X}, decrypted: {:02X?}", cmd_type, &buf);

        let reply: Option<[u8; 20]> = match cmd_type {
            0x04 => {
                let cs = cube_state_cmd.lock().unwrap();
                let mut msg = protocol::build_facelets(0, &cs);
                drop(cs);
                crypto::encrypt_msg(&mut msg, &key_cmd, &iv_cmd);
                Some(msg)
            }
            0x05 => {
                let mut msg = protocol::build_hardware(1, 0, 1, 0, b"GAN356IS", false);
                crypto::encrypt_msg(&mut msg, &key_cmd, &iv_cmd);
                Some(msg)
            }
            0x09 => {
                let mut msg = protocol::build_battery(80);
                crypto::encrypt_msg(&mut msg, &key_cmd, &iv_cmd);
                Some(msg)
            }
            0x0A => {
                let mut cs = cube_state_cmd.lock().unwrap();
                *cs = protocol::CubeState::solved();
                drop(cs);
                let serial = {
                    let mut s = shared_serial_cmd.lock().unwrap();
                    *s = s.wrapping_add(1);
                    *s
                };
                let cs = cube_state_cmd.lock().unwrap();
                let mut msg = protocol::build_facelets(serial, &cs);
                drop(cs);
                crypto::encrypt_msg(&mut msg, &key_cmd, &iv_cmd);
                Some(msg)
            }
            _ => None,
        };

        if let Some(r) = reply {
            log::info!("CMD reply: {:02X?}", &r);
            state_char_cmd.lock().set_value(&r).notify();
        } else {
            log::warn!("CMD type 0x{:02X} unhandled", cmd_type);
        }
    });

    let mut mfr_payload = [0u8; 11];
    mfr_payload[0] = (MANUFACTURER_CIC & 0xFF) as u8;
    mfr_payload[1] = (MANUFACTURER_CIC >> 8) as u8;
    mfr_payload[2..5].copy_from_slice(&MANUFACTURER_EXTRA);
    mfr_payload[5..11].copy_from_slice(&MAC);

    let mut adv_data = esp32_nimble::BLEAdvertisementData::new();
    adv_data.manufacturer_data(&mfr_payload);

    let name_bytes = DEVICE_NAME.as_bytes();
    let mut scan_resp_raw: Vec<u8> = Vec::new();
    scan_resp_raw.push(name_bytes.len() as u8 + 1);
    scan_resp_raw.push(0x09);
    scan_resp_raw.extend_from_slice(name_bytes);

    {
        let mut adv = ble_device.get_advertising().lock();
        adv.advertisement_type(ConnMode::Und);
        adv.set_data(&mut adv_data).unwrap();
        adv.set_raw_scan_response_data(&scan_resp_raw).unwrap();
        adv.start().unwrap();
    }

    log::info!("BLE advertising as '{}'", DEVICE_NAME);

    let state_char_loop = state_char.clone();
    let key_loop = key.clone();
    let iv_loop = iv.clone();
    let cube_state_loop = cube_state.clone();
    let mut move_history: std::collections::VecDeque<(u8, u8, u16)> = std::collections::VecDeque::new();
    let mut last_move_time = embassy_time::Instant::now();

    loop {
        let cmd = queue.lock().unwrap().pop_front();
        let Some((face, dir)) = cmd else {
            Timer::after(Duration::from_millis(100)).await;
            continue;
        };

        let now = embassy_time::Instant::now();
        let elapsed = now.duration_since(last_move_time).as_millis().min(u16::MAX as u64) as u16;
        last_move_time = now;

        {
            let mut cs = cube_state_loop.lock().unwrap();
            protocol::apply_move(&mut cs, face, dir);
        }

        let serial = {
            let mut s = shared_serial.lock().unwrap();
            *s = s.wrapping_add(1);
            *s
        };

        move_history.push_front((face, dir, elapsed));
        if move_history.len() > 7 {
            move_history.pop_back();
        }

        let history: Vec<(u8, u8, u16)> = move_history.iter().cloned().collect();
        let mut move_msg = protocol::build_move(serial, &history);
        crypto::encrypt_msg(&mut move_msg, &key_loop, &iv_loop);
        state_char_loop.lock().set_value(&move_msg).notify();

        let delay = {
            let base = *move_delay_ms.lock().unwrap();
            let q = queue.lock().unwrap();
            if q.len() > 20 { base.min(100) }
            else if q.len() > 5 { base }
            else { base }
        };
        Timer::after(Duration::from_millis(delay)).await;

        let cs = cube_state_loop.lock().unwrap();
        let mut facelets_msg = protocol::build_facelets(serial, &cs);
        drop(cs);
        crypto::encrypt_msg(&mut facelets_msg, &key_loop, &iv_loop);
        state_char_loop.lock().set_value(&facelets_msg).notify();
    }
}
