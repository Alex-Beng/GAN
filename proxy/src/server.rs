//! Virtual cube GATT server (BLE Peripheral).
//!
//! Advertises as a physical cube of the **configured output protocol** and
//! serves its state / command characteristics to a phone app. Moves received
//! from the real cube over the Central link are re-encoded here (per output
//! protocol) and pushed as notifications.

use std::sync::{Arc, Mutex};

use esp32_nimble::enums::ConnMode;
use esp32_nimble::utilities::BleUuid;
use esp32_nimble::utilities::mutex::Mutex as NimbleMutex;
use esp32_nimble::{BLECharacteristic, BLEDevice, NimbleProperties, NimbleSub};

use shared::crypto;
use shared::device_config::{self, Protocol};
use shared::protocol::{self, CubeState};

/// Encrypt a message with the output protocol's scheme.
fn encrypt(proto: Protocol, data: &mut [u8], key: &[u8; 16], iv: &[u8; 16]) {
    match proto {
        Protocol::MoYu32 => crypto::moyu32_encrypt(data, key, iv),
        Protocol::QiYi => crypto::aes_ecb_encrypt(data, key),
        Protocol::Robot | Protocol::Timer => {}
        _ => crypto::encrypt_msg(data, key, iv),
    }
}

fn decrypt(proto: Protocol, data: &mut [u8], key: &[u8; 16], iv: &[u8; 16]) {
    match proto {
        Protocol::MoYu32 => crypto::moyu32_decrypt(data, key, iv),
        Protocol::QiYi => crypto::aes_ecb_decrypt(data, key),
        Protocol::Robot | Protocol::Timer => {}
        _ => crypto::decrypt_msg(data, key, iv),
    }
}

/// Shared handle used by the Central side to push updates to the phone.
#[derive(Clone)]
pub struct VirtualCube {
    inner: Arc<VirtualCubeInner>,
}

struct VirtualCubeInner {
    proto: Protocol,
    state_char: Arc<NimbleMutex<BLECharacteristic>>,
    serial: Mutex<u16>,
    connected: Arc<Mutex<bool>>,
    key: [u8; 16],
    iv: [u8; 16],
    state: Arc<Mutex<CubeState>>,
}

impl VirtualCube {
    /// Build the GATT server for output `proto` and start advertising.
    ///
    /// `mac` is the virtual cube MAC in display order (advertised manufacturer
    /// data + crypto key derivation).
    pub fn setup(ble_device: &mut BLEDevice, proto: Protocol, mac: [u8; 6]) -> Self {
        let cfg = device_config::get_config(proto);
        // QiYi uses a fixed AES-ECB key (no MAC salt); others derive from MAC.
        let (key, iv) = if proto == Protocol::QiYi {
            (*cfg.base_key, *cfg.base_iv)
        } else {
            crypto::derive_key_iv(&mac, cfg.base_key, cfg.base_iv)
        };

        BLEDevice::set_device_name(cfg.device_name).unwrap();
        let server = ble_device.get_server();

        let connected = Arc::new(Mutex::new(false));
        let connected_sub = connected.clone();
        server.on_connect(move |_s, _d| {
            *connected_sub.lock().unwrap() = true;
            log::info!("[server] phone connected");
        });
        let connected_disc = connected.clone();
        server.on_disconnect(move |_d, _r| {
            *connected_disc.lock().unwrap() = false;
            log::info!("[server] phone disconnected");
        });

        let service = server.create_service(
            BleUuid::from_uuid128_string(cfg.service_uuid).unwrap(),
        );

        // The state characteristic: for QiYi the same char is used for
        // write+notify; here we use NOTIFY|READ for all cube protocols.
        let state_char = service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.state_uuid).unwrap(),
            NimbleProperties::NOTIFY | NimbleProperties::READ,
        );

        let state = Arc::new(Mutex::new(CubeState::solved()));

        // On subscribe, send the initial FACELETS snapshot.
        let state_char_sub = state_char.clone();
        let state_sub = state.clone();
        state_char.lock().on_subscribe(move |_c, _d, cur_notify| {
            log::info!("[server] subscribe cur_notify={:?}", cur_notify);
            if cur_notify.contains(NimbleSub::NOTIFY) {
                let cs = state_sub.lock().unwrap();
                let mut msg = protocol::build_facelets(proto, 0, &cs);
                drop(cs);
                encrypt(proto, &mut msg, &key, &iv);
                state_char_sub.lock().set_value(&msg).notify();
            }
        });

        // Command characteristic (Facelets/Hardware/Battery/Reset requests).
        if cfg.cmd_uuid != "00000000-0000-0000-0000-000000000000"
            && cfg.cmd_uuid != cfg.state_uuid
        {
            let cmd_char = service.lock().create_characteristic(
                BleUuid::from_uuid128_string(cfg.cmd_uuid).unwrap(),
                NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
            );
            let state_char_cmd = state_char.clone();
            let state_cmd = state.clone();
            let key_c = key;
            let iv_c = iv;
            cmd_char.lock().on_write(move |args| {
                let data = args.recv_data();
                let min = if proto == Protocol::Gen3 || proto == Protocol::QiYi { 16 } else { 20 };
                if data.len() < min {
                    return;
                }
                let mut buf = data[..min].to_vec();
                decrypt(proto, &mut buf, &key_c, &iv_c);
                let cmd = protocol::parse_command_type(proto, &buf);
                log::info!("[server] CMD {:?}, decrypted: {:02X?}", cmd, &buf);
                match cmd {
                    protocol::IncomingCommand::Facelets => {
                        let cs = state_cmd.lock().unwrap();
                        let mut msg = protocol::build_facelets(proto, 1, &cs);
                        drop(cs);
                        encrypt(proto, &mut msg, &key_c, &iv_c);
                        state_char_cmd.lock().set_value(&msg).notify();
                    }
                    protocol::IncomingCommand::Hardware => {
                        for mut m in protocol::build_hardware_multi(proto) {
                            encrypt(proto, &mut m, &key_c, &iv_c);
                            state_char_cmd.lock().set_value(&m).notify();
                        }
                    }
                    protocol::IncomingCommand::Battery => {
                        let mut msg = protocol::build_battery(proto, 80);
                        encrypt(proto, &mut msg, &key_c, &iv_c);
                        state_char_cmd.lock().set_value(&msg).notify();
                    }
                    _ => {}
                }
            });
        }

        // Advertise: service UUID + manufacturer data (MAC reversed) + name.
        Self::advertise(ble_device, cfg, mac);

        Self {
            inner: Arc::new(VirtualCubeInner {
                proto,
                state_char,
                serial: Mutex::new(0),
                connected,
                key,
                iv,
                state,
            }),
        }
    }

    fn advertise(ble_device: &BLEDevice, cfg: &device_config::ProtoCfg, mac: [u8; 6]) {
        let mut mfr_payload = [0u8; 11];
        if cfg.proto == Protocol::MoYu32 {
            // WCU: [account BE 4B][model flag][MAC reversed 6B].
            mfr_payload[4] = cfg.mfr_extra[2];
            for i in 0..6 {
                mfr_payload[5 + i] = mac[5 - i];
            }
        } else if cfg.proto == Protocol::QiYi {
            mfr_payload[0] = (cfg.mfr_cic & 0xFF) as u8;
            mfr_payload[1] = (cfg.mfr_cic >> 8) as u8;
            for i in 0..6 {
                mfr_payload[2 + i] = mac[5 - i];
            }
            mfr_payload[8..11].copy_from_slice(cfg.mfr_extra);
        } else {
            mfr_payload[0] = (cfg.mfr_cic & 0xFF) as u8;
            mfr_payload[1] = (cfg.mfr_cic >> 8) as u8;
            mfr_payload[2..5].copy_from_slice(cfg.mfr_extra);
            for i in 0..6 {
                mfr_payload[5 + i] = mac[5 - i];
            }
        }

        let mut adv_data = esp32_nimble::BLEAdvertisementData::new();
        adv_data.add_service_uuid(BleUuid::from_uuid128_string(cfg.service_uuid).unwrap());

        let name_bytes = cfg.device_name.as_bytes();
        let mut scan_resp: Vec<u8> = Vec::new();
        scan_resp.push(12);
        scan_resp.push(0xFF);
        scan_resp.extend_from_slice(&mfr_payload);
        scan_resp.push(name_bytes.len() as u8 + 1);
        scan_resp.push(0x09);
        scan_resp.extend_from_slice(name_bytes);

        {
            let mut adv = ble_device.get_advertising().lock();
            adv.advertisement_type(ConnMode::Und);
            adv.set_data(&mut adv_data).unwrap();
            adv.set_raw_scan_response_data(&scan_resp).unwrap();
            adv.start().unwrap();
        }
        log::info!("[server] advertising as '{}' ({:?})", cfg.device_name, cfg.proto);
    }

    pub fn is_connected(&self) -> bool {
        *self.inner.connected.lock().unwrap()
    }

    /// Apply one move from the physical cube and notify the phone with a MOVE
    /// frame plus a facelets snapshot, encoded for the output protocol.
    pub fn push_move(&self, face: u8, dir: u8) {
        let proto = self.inner.proto;
        let serial = {
            let mut s = self.inner.serial.lock().unwrap();
            *s = s.wrapping_add(1);
            *s
        };
        {
            let mut cs = self.inner.state.lock().unwrap();
            protocol::apply_move(&mut cs, face, dir);
        }

        if !self.is_connected() {
            return;
        }

        let history = [(face, dir, 0u16)];
        let mut move_msg = protocol::build_move(proto, serial, &history, 0);
        encrypt(proto, &mut move_msg, &self.inner.key, &self.inner.iv);
        self.inner.state_char.lock().set_value(&move_msg).notify();

        let cs = self.inner.state.lock().unwrap();
        let mut facelets = protocol::build_facelets(proto, serial, &cs);
        drop(cs);
        encrypt(proto, &mut facelets, &self.inner.key, &self.inner.iv);
        self.inner.state_char.lock().set_value(&facelets).notify();
    }
}
