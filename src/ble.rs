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
        Protocol::Robot | Protocol::Timer => {},  // 无加密
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

        // OTA_ERR notify characteristic (must be created before FW_INFO for closure capture)
        let ota_notify_char = ota_service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.ota_notify_uuid).unwrap(),
            NimbleProperties::READ | NimbleProperties::NOTIFY,
        );
        let ota_notify_char_sub = ota_notify_char.clone();
        ota_notify_char.lock().on_subscribe(move |_char, _desc, _cur_notify| {
            log::info!("OTA_NOTIFY subscribed");
        });

        // FW_DATA: receive firmware binary data
        let ota_data_char = ota_service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.ota_data_uuid).unwrap(),
            NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
        );
        let ota_fw_bytes = Arc::new(Mutex::new(Vec::new()));
        let ota_fw_bytes_cl = ota_fw_bytes.clone();
        ota_data_char.lock().on_write(move |args| {
            let data = args.recv_data();
            log::info!("OTA_DATA ({} bytes): {:02X?}", data.len(), data);
            ota_fw_bytes_cl.lock().unwrap().extend_from_slice(data);
        });

        // FW_INFO (f95a5034-...): OTA control commands
        let ota_write_char = ota_service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.ota_write_uuid).unwrap(),
            NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
        );
        let ota_fw_bytes_cl2 = ota_fw_bytes.clone();
        let ota_notify_char_cl = ota_notify_char_sub.clone();
        ota_write_char.lock().on_write(move |args| {
            let data = args.recv_data();
            log::info!("OTA_CMD ({} bytes): {:02X?}", data.len(), data);
            if data.len() >= 1 {
                match data[0] {
                    0x01 => {
                        log::info!("OTA: start update");
                        // Notify progress
                        let resp = vec![0x01u8, 0x00]; // status=1, progress=0
                        ota_notify_char_cl.lock().set_value(&resp).notify();
                    }
                    0x02 => log::info!("OTA: check version"),
                    0x03 => {
                        let sz = ota_fw_bytes_cl2.lock().unwrap().len();
                        log::info!("OTA: verify firmware ({} bytes)", sz);
                    }
                    0x04 => {
                        log::info!("OTA: reboot");
                    }
                    _ => log::info!("OTA: unknown cmd 0x{:02X}", data[0]),
                }
            }
        });

        // Nordic DFU Service (for Android OtaBridge compatibility)
        let dfu_service = server.create_service(
            BleUuid::from_uuid128_string("0000fe59-0000-1000-8000-00805f9b34fb").unwrap()
        );
        // Secure DFU Control Point
        let _dfu_cp = dfu_service.lock().create_characteristic(
            BleUuid::from_uuid128_string("8ec90001-f315-4f60-9fb8-838830daea50").unwrap(),
            NimbleProperties::WRITE | NimbleProperties::NOTIFY,
        );
        // Secure DFU Packet
        let _dfu_packet = dfu_service.lock().create_characteristic(
            BleUuid::from_uuid128_string("8ec90002-f315-4f60-9fb8-838830daea50").unwrap(),
            NimbleProperties::WRITE_NO_RSP,
        );
        // Buttonless DFU
        let _dfu_btnless = dfu_service.lock().create_characteristic(
            BleUuid::from_uuid128_string("8ec90003-f315-4f60-9fb8-838830daea50").unwrap(),
            NimbleProperties::WRITE | NimbleProperties::NOTIFY,
        );

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
            if proto_sub == Protocol::Robot {
                let msg = protocol::build_robot_info(protocol::RobotInfoType::Model);
                log::info!("Sending initial ROBOT MODEL: {:02X?}", &msg);
                state_char_sub.lock().set_value(&msg).notify();
                let msg2 = protocol::build_robot_info(protocol::RobotInfoType::Version);
                log::info!("Sending initial ROBOT VERSION: {:02X?}", &msg2);
                state_char_sub.lock().set_value(&msg2).notify();
            } else if proto_sub == Protocol::Timer {
                let msg = protocol::build_timer_status(protocol::TimerStatus::Idle);
                log::info!("Sending initial TIMER STATUS: {:02X?}", &msg);
                state_char_sub.lock().set_value(&msg).notify();
            } else {
                let serial = *shared_serial_sub.lock().unwrap();
                let cs = cube_state_sub.lock().unwrap();
                let mut msg = protocol::build_facelets(proto_sub, serial, &cs);
                drop(cs);
                ble_encrypt(proto_sub, &mut msg, &key_sub, &iv_sub);
                log::info!("Sending initial FACELETS: {:02X?}", &msg);
                state_char_sub.lock().set_value(&msg).notify();
            }
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

            // Timer 协议: 无加密, CRC16, 命令/响应
            if proto_cl == Protocol::Timer {
                let cmd = protocol::parse_timer_command(data);
                log::info!("TIMER CMD: {:?}", cmd);
                let mut state_char = state_char_cmd.lock();
                match cmd {
                    protocol::TimerCommand::GetStatus => {
                        let msg = protocol::build_timer_status(protocol::TimerStatus::Idle);
                        state_char.set_value(&msg).notify();
                    }
                    protocol::TimerCommand::GetTimerData => {
                        let msg = protocol::build_timer_data(protocol::TimerStatus::Idle, 0, 0, 0);
                        state_char.set_value(&msg).notify();
                    }
                    protocol::TimerCommand::BindAccount(account) => {
                        log::info!("Timer bind account: {}", account);
                        let msg = protocol::build_timer_status(protocol::TimerStatus::Idle);
                        state_char.set_value(&msg).notify();
                    }
                    protocol::TimerCommand::SetTime(y, mo, d, h, mi, s) => {
                        log::info!("Timer set time: {}-{:02}-{:02} {:02}:{:02}:{:02}", y, mo, d, h, mi, s);
                    }
                    protocol::TimerCommand::SetLed(ct, r1, g1, b1, r2, g2) => {
                        log::info!("Timer LED: ct={}, RGB1=({},{},{}), RGB2=({},{})", ct, r1, g1, b1, r2, g2);
                    }
                    protocol::TimerCommand::SetColorType(ct) => {
                        log::info!("Timer set color type: {}", ct);
                    }
                    protocol::TimerCommand::QueryBattery => {
                        let mut buf = vec![0x07u8, 64, 1, 80];
                        let crc = crate::crypto::crc16_ccitt_false(&buf);
                        buf.push((crc & 0xFF) as u8);
                        buf.push((crc >> 8) as u8);
                        state_char.set_value(&buf).notify();
                    }
                    protocol::TimerCommand::Reset(typ) => {
                        log::info!("Timer reset type: {}", typ);
                    }
                    protocol::TimerCommand::Calibrate(cmd) => {
                        log::info!("Timer calibrate: {}", cmd);
                    }
                    protocol::TimerCommand::QueryInfo(info_type) => {
                        let msg = protocol::build_timer_info(info_type);
                        state_char.set_value(&msg).notify();
                    }
                }
                return;
            }

            // Robot 协议: 无加密, 直接命令/响应
            if proto_cl == Protocol::Robot {
                let cmd = protocol::parse_robot_command(data);
                log::info!("ROBOT CMD: {:?}", cmd);
                let mut state_char = state_char_cmd.lock();
                match cmd {
                    protocol::RobotCommand::Order(v) => {
                        log::info!("Robot order: {}", v);
                    }
                    protocol::RobotCommand::QueryRemainFormulas => {
                        let msg = protocol::build_robot_info(protocol::RobotInfoType::RemainFormulas);
                        state_char.set_value(&msg).notify();
                    }
                    protocol::RobotCommand::SetFormulaList => {
                        log::info!("Robot received formula list ({} bytes)", data.len());
                        // 模拟执行: 转动魔方 + 发送状态
                        let mut cs = cube_state_cl.lock().unwrap();
                        // 简单模拟: 打乱魔方表示正在执行
                        *cs = protocol::CubeState::solved();
                        *cs = protocol::CubeState {
                            cp: [3, 0, 1, 2, 4, 5, 6, 7],
                            co: [0, 0, 0, 0, 0, 0, 0, 0],
                            ep: [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11],
                            eo: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                        };
                        drop(cs);
                        // 发送执行中状态
                        let status = protocol::build_robot_status(1, 50);
                        state_char.set_value(&status).notify();
                        // 稍后发送完成状态
                        let done = protocol::build_robot_status(2, 100);
                        state_char.set_value(&done).notify();
                        // 发送剩余公式数
                        let remain = protocol::build_robot_info(protocol::RobotInfoType::RemainFormulas);
                        state_char.set_value(&remain).notify();
                    }
                    protocol::RobotCommand::SingleMove(dir, surface, turns) => {
                        log::info!("Robot single move: dir={}, surface={}, turns={}", dir, surface, turns);
                        // 应用移动到魔方状态
                        if surface < 6 {
                            let mut cs = cube_state_cl.lock().unwrap();
                            protocol::apply_move(&mut cs, surface, if dir == 0 { 0 } else { 1 });
                            drop(cs);
                        }
                        // 发送完成状态
                        let done = protocol::build_robot_status(2, 100);
                        state_char.set_value(&done).notify();
                    }
                    protocol::RobotCommand::QueryInfo(info_type) => {
                        let msg = protocol::build_robot_info(info_type);
                        state_char.set_value(&msg).notify();
                    }
                    protocol::RobotCommand::SetLed(op, r, g, b) => {
                        log::info!("Robot LED: op={}, R={}, G={}, B={}", op, r, g, b);
                    }
                }
                return;
            }

            // 以下为魔方协议处理
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
            // Robot/Timer 协议: 不发送转动通知, 仅命令/响应
            if proto == Protocol::Robot || proto == Protocol::Timer {
                Timer::after(Duration::from_millis(500)).await;
                let p = *active_proto.lock().unwrap();
                if p != proto {
                    log::info!("Protocol changed {:?} -> {:?}, restarting BLE", proto, p);
                    break;
                }
                continue;
            }

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
