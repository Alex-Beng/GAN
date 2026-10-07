//! Virtual GAN Gen2 cube GATT server (BLE Peripheral).
//!
//! Advertises as a physical GAN cube (V1 / Gen2) and serves the state / command
//! characteristics to a phone app. Moves received from the real cube over the
//! Central link are re-encoded here and pushed as notifications.

use std::sync::{Arc, Mutex};

use esp32_nimble::enums::ConnMode;
use esp32_nimble::utilities::BleUuid;
use esp32_nimble::utilities::mutex::Mutex as NimbleMutex;
use esp32_nimble::{BLECharacteristic, BLEDevice, NimbleProperties, NimbleSub};

use shared::crypto;
use shared::device_config::{self, Protocol};
use shared::protocol::{self, CubeState};

/// Shared handle used by the Central side to push updates to the phone.
#[derive(Clone)]
pub struct VirtualCube {
    inner: Arc<VirtualCubeInner>,
}

struct VirtualCubeInner {
    state_char: Arc<NimbleMutex<BLECharacteristic>>,
    serial: Mutex<u16>,
    connected: Arc<Mutex<bool>>,
    key: [u8; 16],
    iv: [u8; 16],
    state: Arc<Mutex<CubeState>>,
}

impl VirtualCube {
    /// Build the GATT server and start advertising. Returns a handle for pushing
    /// moves. `mac` is the virtual cube's MAC in display order (used for both the
    /// advertised manufacturer data and the crypto key derivation).
    pub fn setup(ble_device: &mut BLEDevice, mac: [u8; 6]) -> Self {
        let cfg = device_config::get_config(Protocol::Gen2);
        let (key, iv) = crypto::derive_key_iv(&mac, cfg.base_key, cfg.base_iv);

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

        let state_char = service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.state_uuid).unwrap(),
            NimbleProperties::NOTIFY | NimbleProperties::READ,
        );

        // On subscribe, send the initial FACELETS snapshot.
        let state_char_sub = state_char.clone();
        let key_sub = key;
        let iv_sub = iv;
        state_char.lock().on_subscribe(move |_c, _d, cur_notify| {
            log::info!("[server] subscribe cur_notify={:?}", cur_notify);
            if cur_notify.contains(NimbleSub::NOTIFY) {
                let mut msg = protocol::build_facelets(Protocol::Gen2, 0, &CubeState::solved());
                crypto::encrypt_msg(&mut msg, &key_sub, &iv_sub);
                state_char_sub.lock().set_value(&msg).notify();
            }
        });

        // Command characteristic (Facelets/Hardware/Battery/Reset requests).
        let cmd_char = service.lock().create_characteristic(
            BleUuid::from_uuid128_string(cfg.cmd_uuid).unwrap(),
            NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
        );
        let state_char_cmd = state_char.clone();
        let state_cmd = Arc::new(Mutex::new(CubeState::solved()));
        let state_char_cmd_state = state_cmd.clone();
        cmd_char.lock().on_write(move |args| {
            let data = args.recv_data();
            if data.len() < 16 {
                return;
            }
            let mut buf = data[..16.min(data.len())].to_vec();
            crypto::decrypt_msg(&mut buf, &key, &iv);
            let cmd = protocol::parse_command_type(Protocol::Gen2, &buf);
            log::info!("[server] CMD {:?}", cmd);
            match cmd {
                protocol::IncomingCommand::Facelets => {
                    let cs = state_char_cmd_state.lock().unwrap();
                    let mut msg = protocol::build_facelets(Protocol::Gen2, 1, &cs);
                    drop(cs);
                    crypto::encrypt_msg(&mut msg, &key, &iv);
                    state_char_cmd.lock().set_value(&msg).notify();
                }
                protocol::IncomingCommand::Battery => {
                    let mut msg = protocol::build_battery(Protocol::Gen2, 80);
                    crypto::encrypt_msg(&mut msg, &key, &iv);
                    state_char_cmd.lock().set_value(&msg).notify();
                }
                _ => {}
            }
        });

        // Advertise: service UUID + manufacturer data (MAC reversed) + name.
        let mut mfr_payload = [0u8; 11];
        mfr_payload[0] = (cfg.mfr_cic & 0xFF) as u8;
        mfr_payload[1] = (cfg.mfr_cic >> 8) as u8;
        mfr_payload[2..5].copy_from_slice(cfg.mfr_extra);
        for i in 0..6 {
            mfr_payload[5 + i] = mac[5 - i];
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
        log::info!("[server] advertising as '{}'", cfg.device_name);

        Self {
            inner: Arc::new(VirtualCubeInner {
                state_char,
                serial: Mutex::new(0),
                connected,
                key,
                iv,
                state: state_cmd,
            }),
        }
    }

    pub fn is_connected(&self) -> bool {
        *self.inner.connected.lock().unwrap()
    }

    /// Apply one move (face 0..5, dir 0/1) from the physical cube, update the
    /// virtual state and notify the phone with a MOVE frame plus a facelets
    /// snapshot (mirrors emu's Gen2 behaviour).
    pub fn push_move(&self, face: u8, dir: u8) {
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
            log::info!("[server] move {} {} (no phone connected)", face, dir);
            return;
        }

        let history = [(face, dir, 0u16)];
        let mut move_msg = protocol::build_move(Protocol::Gen2, serial, &history, 0);
        crypto::encrypt_msg(&mut move_msg, &self.inner.key, &self.inner.iv);
        self.inner.state_char.lock().set_value(&move_msg).notify();

        let cs = self.inner.state.lock().unwrap();
        let mut facelets = protocol::build_facelets(Protocol::Gen2, serial, &cs);
        drop(cs);
        crypto::encrypt_msg(&mut facelets, &self.inner.key, &self.inner.iv);
        self.inner.state_char.lock().set_value(&facelets).notify();
    }
}
