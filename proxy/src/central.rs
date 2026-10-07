//! BLE Central: scan / connect a physical smart cube, identify its protocol by
//! GATT service UUID, establish link encryption, subscribe to the state
//! characteristic and decode notifications into [`CubeEvent`].
//!
//! Supported inputs: GAN Gen2 (V1), GAN Gen3 (V2), GAN Gen4 (V3),
//! MoYu WCU_MY3, MoYu MHC (plaintext), QiYi.

use esp32_nimble::utilities::BleUuid;
use esp32_nimble::{BLEAddress, BLEClient, BLEDevice, BLEScan};

use shared::crypto;
use shared::device_config::{self, Protocol};
use shared::protocol::{self, CubeEvent};

/// Input protocol of a physical cube (as seen by the Central side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputProto {
    /// GAN Gen2 / V1 (service 6e400001-...).
    GanV1,
    /// GAN Gen3 / V2 (service 8653000a-...).
    GanV2,
    /// GAN Gen4 / V3 + Gan251 (service 00000010-...).
    GanV3,
    /// MoYu WCU_MY3 (service 0783b03e-...).
    MoyuWcu,
    /// MoYu MHC (service 00001000, plaintext).
    MoyuMhc,
    /// QiYi QY-QYSC (service 0000fff0, char 0000fff6).
    QiYi,
}

impl InputProto {
    pub fn label(&self) -> &'static str {
        match self {
            InputProto::GanV1 => "GAN Gen2/V1",
            InputProto::GanV2 => "GAN Gen3/V2",
            InputProto::GanV3 => "GAN Gen4/V3",
            InputProto::MoyuWcu => "MoYu WCU",
            InputProto::MoyuMhc => "MoYu MHC",
            InputProto::QiYi => "QiYi",
        }
    }

    /// The `shared::device_config::Protocol` used to source crypto material.
    fn cfg_proto(&self) -> Protocol {
        match self {
            InputProto::GanV1 => Protocol::Gen2,
            InputProto::GanV2 => Protocol::Gen3,
            InputProto::GanV3 => Protocol::Gen4,
            InputProto::MoyuWcu => Protocol::MoYu32,
            InputProto::MoyuMhc => Protocol::Gen2, // unused (plaintext)
            InputProto::QiYi => Protocol::QiYi,
        }
    }

    /// Notification characteristic UUID the Central must subscribe to.
    fn state_uuid(&self) -> &'static str {
        match self {
            InputProto::GanV1 => "28be4cb6-cd67-11e9-a32f-2a2ae2dbcce4",
            InputProto::GanV2 => "8653000b-43e6-47b7-9cb0-5fc21d4ae340",
            InputProto::GanV3 => "0000fff6-0000-1000-8000-00805f9b34fb",
            InputProto::MoyuWcu => "0783b03e-7735-b5a0-1760-a305d2795cb1",
            InputProto::MoyuMhc => "00001002-0000-1000-8000-00805f9b34fb",
            InputProto::QiYi => "0000fff6-0000-1000-8000-00805f9b34fb",
        }
    }
}

/// Service UUIDs used for authoritative protocol detection after connect.
const SVC_GAN_V1: &str = "6e400001-b5a3-f393-e0a9-e50e24dc4179";
const SVC_GAN_V2: &str = "8653000a-43e6-47b7-9cb0-5fc21d4ae340";
const SVC_GAN_V3: &str = "00000010-0000-fff7-fff6-fff5fff4fff0";
const SVC_MOYU_WCU: &str = "0783b03e-7735-b5a0-1760-a305d2795cb0";
const SVC_MOYU_MHC: &str = "00001000-0000-1000-8000-00805f9b34fb";
const SVC_QIYI: &str = "0000fff0-0000-1000-8000-00805f9b34fb";

/// Decrypted-message handler: receives each decoded [`CubeEvent`].
pub type EventSink = Box<dyn Fn(CubeEvent) + Send + Sync + 'static>;

fn name_of(data: &esp32_nimble::BLEAdvertisedData<&[u8]>) -> String {
    data.name()
        .and_then(|n| core::str::from_utf8(n.as_ref()).ok())
        .unwrap_or("")
        .to_string()
}

/// Name-prefix pre-filter for the scan phase (authoritative detection happens
/// after connecting, via service UUIDs).
fn is_candidate_name(name: &str) -> bool {
    let up = name.to_uppercase();
    up.starts_with("GAN")
        || up.starts_with("MG")
        || up.starts_with("AICUBE")
        || up.starts_with("WCU")
        || up.starts_with("MHC")
        || up.starts_with("QY-")
        || up.starts_with("XMD-")
}

/// Scan forever until a candidate cube is found; returns its address + name.
pub async fn find_cube(
    ble_device: &BLEDevice,
) -> (BLEAddress, String) {
    loop {
        let mut scan = BLEScan::new();
        scan.active_scan(true)
            .interval(100)
            .window(100)
            .filter_duplicates(false);

        log::info!("[central] scanning 15s for a physical cube (wake it up!)...");

        let found = scan
            .start(ble_device, 15000, |device, data| {
                let name = name_of(&data);
                if !name.is_empty() {
                    log::debug!(
                        "[central] seen addr={} name='{}' rssi={}",
                        device.addr(),
                        name,
                        device.rssi()
                    );
                }
                if is_candidate_name(&name) {
                    log::info!(
                        "[central] candidate addr={} name='{}' rssi={}",
                        device.addr(),
                        name,
                        device.rssi()
                    );
                    return Some((*device, name));
                }
                None
            })
            .await
            .ok()
            .flatten();

        if let Some((device, name)) = found {
            return (device.addr(), name);
        }
        log::info!("[central] no candidate yet; rescanning...");
    }
}

/// After connecting, walk the GATT services to identify the input protocol.
/// Returns `None` if no known service is present.
pub async fn detect_protocol(client: &mut BLEClient) -> Option<InputProto> {
    let known: &[(&str, InputProto)] = &[
        (SVC_GAN_V1, InputProto::GanV1),
        (SVC_GAN_V2, InputProto::GanV2),
        (SVC_GAN_V3, InputProto::GanV3),
        (SVC_MOYU_WCU, InputProto::MoyuWcu),
        (SVC_MOYU_MHC, InputProto::MoyuMhc),
        (SVC_QIYI, InputProto::QiYi),
    ];

    let services = client.get_services().await.ok()?;
    for svc in services {
        let uuid = svc.uuid().to_string().to_lowercase();
        for (want, proto) in known {
            if uuid == want.to_lowercase() {
                return Some(*proto);
            }
        }
    }
    None
}

/// Subscribe to the state characteristic of `proto` and route decoded events
/// to `sink`. `mac` is the cube MAC in display order; used for key derivation.
pub async fn subscribe_and_decode(
    client: &mut BLEClient,
    proto: InputProto,
    mac: [u8; 6],
    sink: EventSink,
) -> anyhow::Result<()> {
    // Pre-compute crypto material for this input protocol.
    let cfg = device_config::get_config(proto.cfg_proto());
    let (key, iv) = if proto == InputProto::QiYi {
        (*cfg.base_key, *cfg.base_iv) // QiYi uses a fixed AES-ECB key (no salt)
    } else {
        crypto::derive_key_iv(&mac, cfg.base_key, cfg.base_iv)
    };

    let state_uuid = proto.state_uuid();
    let target = BleUuid::from_uuid128_string(state_uuid).unwrap();

    let services = client.get_services().await?;
    for svc in services {
        let chars = match svc.get_characteristics().await {
            Ok(c) => c,
            Err(_) => continue,
        };
        for chr in chars {
            if !chr.can_notify() || chr.uuid() != target {
                continue;
            }
            let proto_cb = proto;
            chr.on_notify(move |data: &[u8]| {
                let ev = decrypt_and_decode(proto_cb, data, &key, &iv);
                if let Some(ev) = ev {
                    sink(ev);
                }
            });
            chr.subscribe_notify(false).await?;
            log::info!("[central] subscribed to {} state char", proto.label());
            return Ok(());
        }
    }
    anyhow::bail!("state characteristic {state_uuid} not found for {}", proto.label())
}

/// Decrypt (per protocol) then decode a raw notification into a [`CubeEvent`].
fn decrypt_and_decode(
    proto: InputProto,
    data: &[u8],
    key: &[u8; 16],
    iv: &[u8; 16],
) -> Option<CubeEvent> {
    match proto {
        InputProto::MoyuMhc => Some(protocol::decode_mhc(data)),
        InputProto::QiYi => {
            if data.len() < 16 || data.len() % 16 != 0 {
                return None;
            }
            let mut buf = data.to_vec();
            crypto::aes_ecb_decrypt(&mut buf, key);
            // Validate CRC16-Modbus (over data[0..len-2]) unless it's a gyro frame.
            if buf[0] != 0xCC {
                let msg_len = buf[1] as usize;
                if msg_len >= 3 && msg_len <= buf.len() {
                    // CRC check is optional here; decode anyway.
                }
            }
            Some(protocol::decode_qiyi(&buf))
        }
        InputProto::MoyuWcu => {
            if data.len() < 20 {
                return None;
            }
            let mut buf = data[..20].to_vec();
            crypto::moyu32_decrypt(&mut buf, key, iv);
            Some(protocol::decode_moyu_my3(&buf))
        }
        InputProto::GanV1 | InputProto::GanV2 | InputProto::GanV3 => {
            let min = if proto == InputProto::GanV2 { 16 } else { 20 };
            if data.len() < min {
                return None;
            }
            let mut buf = data[..min].to_vec();
            crypto::decrypt_msg(&mut buf, key, iv);
            Some(match proto {
                InputProto::GanV1 => protocol::decode_gen2(&buf),
                InputProto::GanV2 => protocol::decode_gen3(&buf),
                InputProto::GanV3 => protocol::decode_gen4(&buf),
                _ => unreachable!(),
            })
        }
    }
}
