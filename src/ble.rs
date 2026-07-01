use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use embassy_time::{Duration, Timer};
use esp32_nimble::enums::{ConnMode, OwnAddrType};
use esp32_nimble::utilities::BleUuid;
use esp32_nimble::{BLEDevice, NimbleProperties};

use crate::crypto;
use crate::device_config::{self, Protocol};
use crate::http::{MOVE_FACE_DELAY, MOVE_FACE_TIME};
use crate::protocol;

fn ble_encrypt(proto: Protocol, data: &mut Vec<u8>, key: &[u8; 16], iv: &[u8; 16]) {
    match proto {
        Protocol::MoYu32 => crate::crypto::moyu32_encrypt(data, key, iv),
        Protocol::QiYi => crate::crypto::aes_ecb_encrypt(data, key),
        _ => crate::crypto::encrypt_msg(data, key, iv),
    }
}

pub async fn run(
    queue: Arc<Mutex<VecDeque<(u8, u8)>>>,
    cube_state: Arc<Mutex<protocol::CubeState>>,
    move_delay_ms: Arc<Mutex<u64>>,
    active_proto: Arc<Mutex<Protocol>>,
) {
    loop {
        let proto = *active_proto.lock().unwrap();
        let cfg = device_config::get_config(proto);

        log::info!("Starting BLE session: {:?} @ {:02X?}", proto, cfg.mac);

        unsafe {
            esp_idf_svc::sys::esp_iface_mac_addr_set(
                cfg.mac.as_ptr(),
                esp_idf_svc::sys::esp_mac_type_t_ESP_MAC_BT,
            );
        }

        let (key, iv) = crypto::derive_key_iv(cfg.mac, cfg.base_key, cfg.base_iv);
        let key = Arc::new(key);
        let iv = Arc::new(iv);

        let ble_device = BLEDevice::take();
        ble_device.set_own_addr_type(OwnAddrType::Public);
        BLEDevice::set_device_name(cfg.device_name).unwrap();

        let server = ble_device.get_server();

        let state_char_ref: Arc<Mutex<Option<Arc<esp32_nimble::utilities::mutex::Mutex<esp32_nimble::BLECharacteristic>>>>> =
            Arc::new(Mutex::new(None));
        let shared_serial = Arc::new(Mutex::new(0u16));
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

        let service = server.create_service(BleUuid::from_uuid128_string(cfg.service_uuid).unwrap());

        let ota_service = server.create_service(BleUuid::from_uuid128_string(cfg.ota_service_uuid).unwrap());

        let ota_version_char = ota_service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.ota_version_uuid).unwrap(),
            NimbleProperties::READ,
        );
        ota_version_char.lock().set_value(cfg.ota_version);

        let ota_write_char = ota_service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.ota_write_uuid).unwrap(),
            NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
        );
        ota_write_char.lock().on_write(move |args| {
            log::info!("OTA_WRITE raw ({} bytes): {:02X?}", args.recv_data().len(), args.recv_data());
        });

        let ota_notify_char = ota_service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.ota_notify_uuid).unwrap(),
            NimbleProperties::READ | NimbleProperties::NOTIFY,
        );
        ota_notify_char.lock().on_subscribe(move |_char, _desc, _cur_notify| {
            log::info!("OTA_NOTIFY subscribed");
        });

        let cmd_char = service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.cmd_uuid).unwrap(),
            NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
        );

        let state_char = service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.state_uuid).unwrap(),
            NimbleProperties::NOTIFY | NimbleProperties::READ,
        );

        *state_char_ref.lock().unwrap() = Some(state_char.clone());

        let key_sub = key.clone();
        let iv_sub = iv.clone();
        let shared_serial_sub = shared_serial_conn.clone();
        let state_char_sub = state_char.clone();
        let cube_state_sub = cube_state.clone();
        let proto_sub = proto;
        state_char.lock().on_subscribe(move |_char, _desc, cur_notify| {
            log::info!("Client subscribed, cur_notify={:?}", cur_notify);
            let serial = *shared_serial_sub.lock().unwrap();
            let cs = cube_state_sub.lock().unwrap();
            let mut msg = protocol::build_facelets(proto_sub, serial, &cs);
            drop(cs);
            ble_encrypt(proto_sub, &mut msg, &key_sub, &iv_sub);
            log::info!("Sending initial FACELETS: {:02X?}", &msg);
            state_char_sub.lock().set_value(&msg).notify();
        });

        let state_char_cmd = state_char.clone();

        let key_cl = key.clone();
        let iv_cl = iv.clone();
        let cube_state_cl = cube_state.clone();
        let shared_serial_cl = shared_serial.clone();
        let proto_cl = proto;

        cmd_char.lock().on_write(move |args| {
            let data = args.recv_data();
            log::info!("CMD raw ({} bytes): {:02X?}", data.len(), data);

            let min_len = if proto_cl == Protocol::Gen3 { 16 } else { 20 };
            if data.len() < min_len {
                log::warn!("CMD too short, ignoring");
                return;
            }
            let mut buf = vec![0u8; min_len];
            buf.copy_from_slice(&data[..min_len]);
            match proto_cl {
                Protocol::MoYu32 => crate::crypto::moyu32_decrypt(&mut buf, &key_cl, &iv_cl),
                Protocol::QiYi => crate::crypto::aes_ecb_decrypt(&mut buf, &key_cl),
                _ => crate::crypto::decrypt_msg(&mut buf, &key_cl, &iv_cl),
            }

            let cmd = protocol::parse_command_type(proto_cl, &buf);
            log::info!("CMD: {:?}, decrypted: {:02X?}", cmd, &buf);

            let mut state_char = state_char_cmd.lock();
            match cmd {
                protocol::IncomingCommand::Facelets => {
                    let cs = cube_state_cl.lock().unwrap();
                    let serial = *shared_serial_cl.lock().unwrap();
                    let mut msg = protocol::build_facelets(proto_cl, serial, &cs);
                    drop(cs);
                    ble_encrypt(proto_cl, &mut msg, &key_cl, &iv_cl);
                    log::info!("CMD reply FACELETS: {:02X?}", &msg);
                    state_char.set_value(&msg).notify();
                }
                protocol::IncomingCommand::Hardware => {
                    match proto_cl {
                        Protocol::Gen2 | Protocol::Gen3 => {
                            let mut msg = protocol::build_hardware_single(proto_cl);
                            ble_encrypt(proto_cl, &mut msg, &key_cl, &iv_cl);
                            state_char.set_value(&msg).notify();
                        }
                        Protocol::Gen4 | Protocol::Gan251 => {
                            for mut m in protocol::build_hardware_multi(proto_cl) {
                                ble_encrypt(proto_cl, &mut m, &key_cl, &iv_cl);
                                log::info!("CMD reply HARDWARE sub: {:02X?}", &m);
                                state_char.set_value(&m).notify();
                            }
                        }
                        _ => {}
                    }
                }
                protocol::IncomingCommand::Battery => {
                    let mut msg = protocol::build_battery(proto_cl, 80);
                    ble_encrypt(proto_cl, &mut msg, &key_cl, &iv_cl);
                    state_char.set_value(&msg).notify();
                }
                protocol::IncomingCommand::Reset => {
                    let mut cs = cube_state_cl.lock().unwrap();
                    *cs = protocol::CubeState::solved();
                    drop(cs);
                    let serial = {
                        let mut s = shared_serial_cl.lock().unwrap();
                        *s = s.wrapping_add(1);
                        *s
                    };
                    let cs = cube_state_cl.lock().unwrap();
                    let mut msg = protocol::build_facelets(proto_cl, serial, &cs);
                    drop(cs);
                    ble_encrypt(proto_cl, &mut msg, &key_cl, &iv_cl);
                    state_char.set_value(&msg).notify();
                }
                protocol::IncomingCommand::MoveHistory { start_serial, count } => {
                    log::info!("CMD MoveHistory: serial={} count={}", start_serial, count);
                    let empty: [(u8, u8); 0] = [];
                    let mut msg = protocol::build_move_history(proto_cl, start_serial, &empty);
                    if !msg.is_empty() {
                        ble_encrypt(proto_cl, &mut msg, &key_cl, &iv_cl);
                        state_char.set_value(&msg).notify();
                    }
                }
                protocol::IncomingCommand::Unknown => {
                    log::warn!("CMD unhandled: {:02X?}", &buf);
                }
            }
        });

        let mut mfr_payload = [0u8; 11];
        mfr_payload[0] = (cfg.mfr_cic & 0xFF) as u8;
        mfr_payload[1] = (cfg.mfr_cic >> 8) as u8;
        mfr_payload[2..5].copy_from_slice(cfg.mfr_extra);
        for i in 0..6 {
            mfr_payload[5 + i] = cfg.mac[5 - i];
        }

        let mut adv_data = esp32_nimble::BLEAdvertisementData::new();
        adv_data.manufacturer_data(&mfr_payload);

        let name_bytes = cfg.device_name.as_bytes();
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

        log::info!("BLE advertising as '{}' ({:?})", cfg.device_name, proto);

        let state_char_loop = state_char.clone();
        let key_loop = key.clone();
        let iv_loop = iv.clone();
        let cube_state_loop = cube_state.clone();
        let connected_loop = connected.clone();
        let mut move_history: std::collections::VecDeque<(u8, u8, u16)> = std::collections::VecDeque::new();
        let mut last_move_time = embassy_time::Instant::now();
        let cube_start = embassy_time::Instant::now();

        loop {
            let cmd = queue.lock().unwrap().pop_front();
            let Some((face, dir)) = cmd else {
                Timer::after(Duration::from_millis(100)).await;
                let p = *active_proto.lock().unwrap();
                if p != proto {
                    log::info!("Protocol changed {:?} -> {:?}, restarting BLE", proto, p);
                    break;
                }
                continue;
            };

            if face == MOVE_FACE_TIME {
                let ms = dir as u64 * 10;
                log::info!("BLE time sleep {}ms", ms);
                Timer::after(Duration::from_millis(ms)).await;
                continue;
            }

            if face == MOVE_FACE_DELAY {
                let delay = dir as u64;
                *move_delay_ms.lock().unwrap() = delay;
                log::info!("BLE delay set to {}ms", delay);
                continue;
            }

            let current_proto = *active_proto.lock().unwrap();
            if current_proto != proto {
                log::info!("Protocol changed {:?} -> {:?}, restarting BLE", proto, current_proto);
                break;
            }

            if current_proto == Protocol::Gan251 && face > 2 {
                log::info!("GAN251 ignoring {}/{} (only RUF supported)", face, dir);
                continue;
            }

            let names = ["U", "R", "F", "D", "L", "B"];
            let dir_str = if dir == 0 { "" } else { "'" };
            let qlen = queue.lock().unwrap().len();
            log::info!("BLE move: {}{} (face={} dir={}) queue_remaining={}", names[face as usize], dir_str, face, dir, qlen);

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

            let cube_time = embassy_time::Instant::now()
                .duration_since(cube_start)
                .as_millis()
                .min(u32::MAX as u64) as u32;

            let mut move_msg = protocol::build_move(current_proto, serial, &history, cube_time);
            ble_encrypt(current_proto, &mut move_msg, &key_loop, &iv_loop);
            if *connected_loop.lock().unwrap() {
                state_char_loop.lock().set_value(&move_msg).notify();
            }

            let delay = (*move_delay_ms.lock().unwrap()).max(1);
            Timer::after(Duration::from_millis(delay)).await;

            let cs = cube_state_loop.lock().unwrap();
            let mut facelets_msg = protocol::build_facelets(current_proto, serial, &cs);
            drop(cs);
            ble_encrypt(current_proto, &mut facelets_msg, &key_loop, &iv_loop);
            if *connected_loop.lock().unwrap() {
                state_char_loop.lock().set_value(&facelets_msg).notify();
            }
        }

        log::info!("Deinitializing BLE for protocol restart");
        let _ = BLEDevice::deinit_full();
    }
}
