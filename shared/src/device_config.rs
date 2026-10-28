#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Protocol {
    Gen2 = 0,
    Gen3 = 1,
    Gen4 = 2,
    Gan251 = 3,
    MoYu32 = 4,
    QiYi = 5,
    Robot = 6,
    Timer = 7,
}

impl Protocol {
    pub const ALL: &[Protocol] = &[
        Protocol::Gen2,
        Protocol::Gen3,
        Protocol::Gen4,
        Protocol::Gan251,
        Protocol::MoYu32,
        Protocol::QiYi,
        Protocol::Robot,
        Protocol::Timer,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Protocol::Gen2 => "Gen2 (V1/GANi3SB)",
            Protocol::Gen3 => "Gen3 (V2/GANicV2S)",
            Protocol::Gen4 => "Gen4 (V3/GANi4)",
            Protocol::Gan251 => "Gan251 (V3-2/GAN251ui)",
            Protocol::MoYu32 => "MoYu (WCU)",
            Protocol::QiYi => "QiYi (Smart)",
            Protocol::Robot => "Robot (GAN Robot)",
            Protocol::Timer => "Timer (GAN Timer)",
        }
    }

    pub fn from_index(i: u8) -> Option<Protocol> {
        match i {
            0 => Some(Protocol::Gen2),
            1 => Some(Protocol::Gen3),
            2 => Some(Protocol::Gen4),
            3 => Some(Protocol::Gan251),
            4 => Some(Protocol::MoYu32),
            5 => Some(Protocol::QiYi),
            6 => Some(Protocol::Robot),
            7 => Some(Protocol::Timer),
            _ => None,
        }
    }
}

pub struct ProtoCfg {
    #[allow(dead_code)]
    pub proto: Protocol,
    pub device_name: &'static str,
    pub mac: &'static [u8; 6],
    pub service_uuid: &'static str,
    pub cmd_uuid: &'static str,
    pub state_uuid: &'static str,
    pub ota_service_uuid: &'static str,
    pub ota_version_uuid: &'static str,
    pub ota_write_uuid: &'static str,
    pub ota_data_uuid: &'static str,
    pub ota_notify_uuid: &'static str,
    pub ota_version: &'static [u8; 16],
    pub base_key: &'static [u8; 16],
    pub base_iv: &'static [u8; 16],
    pub mfr_cic: u16,
    pub mfr_extra: &'static [u8; 3],
}

pub const WIFI_AP_SSID: &str = "GanCubeEmu";
pub const WIFI_AP_PASS: &str = "";

const MAC_GEN2: [u8; 6] = [0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xCA];
const MAC_GEN3: [u8; 6] = [0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xCB];
const MAC_GEN4: [u8; 6] = [0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xCC];
const MAC_GAN251: [u8; 6] = [0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xCD];
// Real WCU cubes: name "WCU_MY32_XXXX" with MAC CF:30:16:00:XX:XX (XXXX = MAC suffix,
// cf. csTimer moyu32cube.js). Unbound cubes advertise manufacturer CIC 0x0000.
const MAC_MOYU32: [u8; 6] = [0xCF, 0x30, 0x16, 0x00, 0xA4, 0xCE];
const MAC_QIYI: [u8; 6] = [0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xCF];
const MAC_ROBOT: [u8; 6] = [0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xD0];
const MAC_TIMER: [u8; 6] = [0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xD1];

const C_OTA_VERSION: &[u8; 16] = b"1.0.0\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
const C_MFR_EXTRA: &[u8; 3] = &[0x00, 0x00, 0x00];
// WCU app (com.yi.aicube) discovers cubes by matching the 5th byte of the
// Manufacturer Specific Data against its ProtocolFlag table (BluetoothUtil.
// CheckProtocolVersion => packetData[4]). 0x30 selects CubeProtocol5, whose
// wire format (8-char name, 48x3b facelets, 5-move packets) matches this
// emulator. 0x00 (empty) made the app return BluetoothProtocolVersion.None and
// drop the device during scan.
const C_MFR_EXTRA_MOYU32: &[u8; 3] = &[0x00, 0x00, 0x30];

const C_KEY_GEN2: [u8; 16] = [
    0x01, 0x02, 0x42, 0x28, 0x31, 0x91, 0x16, 0x07,
    0x20, 0x05, 0x18, 0x54, 0x42, 0x11, 0x12, 0x53,
];
const C_IV_GEN2: [u8; 16] = [
    0x11, 0x03, 0x32, 0x28, 0x21, 0x01, 0x76, 0x27,
    0x20, 0x95, 0x78, 0x14, 0x32, 0x12, 0x02, 0x43,
];

const C_KEY_MOYU32: [u8; 16] = [
    21, 119, 58, 92, 103, 14, 45, 31,
    23, 103, 42, 19, 155, 103, 82, 87,
];
const C_IV_MOYU32: [u8; 16] = [
    17, 35, 38, 37, 134, 42, 44, 59,
    85, 6, 127, 49, 126, 103, 33, 87,
];

// QiYi AES-128-ECB key. App and csTimer share the SAME key: `Utils.getEncrypter`
// is stored as Dart smi words (value<<1), which decodes exactly to these bytes
// (e.g. 174>>1=87=0x57).
const C_KEY_QIYI: [u8; 16] = [
    0x57, 0xB1, 0xF9, 0xAB, 0xCD, 0x5A, 0xE8, 0xA7,
    0x9C, 0xB9, 0x8C, 0xE7, 0x57, 0x8C, 0x51, 0x08,
];
const C_KEY_GAN251: [u8; 16] = [
    0x58, 0x98, 0x61, 0xfc, 0x1f, 0xec, 0xd7, 0x60,
    0x9f, 0x85, 0xd3, 0x62, 0xbe, 0x37, 0x17, 0x2c,
];
const C_IV_GAN251: [u8; 16] = [
    0x7f, 0x61, 0xd0, 0x52, 0x75, 0xc1, 0x39, 0x52,
    0x08, 0x2e, 0x54, 0x1d, 0x8a, 0x78, 0x63, 0x4d,
];

const CFG_GEN2: ProtoCfg = ProtoCfg {
    proto: Protocol::Gen2,
    device_name: "GANi3SB",
    mac: &MAC_GEN2,
    service_uuid: "6e400001-b5a3-f393-e0a9-e50e24dc4179",
    cmd_uuid: "28be4a4a-cd67-11e9-a32f-2a2ae2dbcce4",
    state_uuid: "28be4cb6-cd67-11e9-a32f-2a2ae2dbcce4",
    ota_service_uuid: "f95a48e6-a721-11e9-a2a3-022ae2dbcce4",
    ota_version_uuid: "f95a4b66-a721-11e9-a2a3-022ae2dbcce4",
    ota_write_uuid: "f95a5034-a721-11e9-a2a3-022ae2dbcce4",
    ota_notify_uuid: "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3",
    ota_data_uuid: "f95a4cce-a721-11e9-a2a3-022ae2dbcce4",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_GEN2,
    base_iv: &C_IV_GEN2,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

const CFG_GEN3: ProtoCfg = ProtoCfg {
    proto: Protocol::Gen3,
    // V2 model name; the app maps names by prefix (`ganicv2s_` -> ProtocolV2),
    // so the advertised name must be a real V2 model.
    device_name: "GANicV2S_EMU",
    mac: &MAC_GEN3,
    service_uuid: "8653000a-43e6-47b7-9cb0-5fc21d4ae340",
    cmd_uuid: "8653000c-43e6-47b7-9cb0-5fc21d4ae340",
    state_uuid: "8653000b-43e6-47b7-9cb0-5fc21d4ae340",
    ota_service_uuid: "f95a48e6-a721-11e9-a2a3-022ae2dbcce4",
    ota_version_uuid: "f95a4b66-a721-11e9-a2a3-022ae2dbcce4",
    ota_write_uuid: "f95a5034-a721-11e9-a2a3-022ae2dbcce4",
    ota_notify_uuid: "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3",
    ota_data_uuid: "f95a4cce-a721-11e9-a2a3-022ae2dbcce4",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_GEN2,
    base_iv: &C_IV_GEN2,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

const CFG_GEN4: ProtoCfg = ProtoCfg {
    proto: Protocol::Gen4,
    // V3 model name; the app maps `gani4_` -> ProtocolV3 (service 00000010-...).
    // The previous "GAN15UI" is not a recognised V3 prefix and fell back to V1.
    device_name: "GANi4_EMU",
    mac: &MAC_GEN4,
    service_uuid: "00000010-0000-fff7-fff6-fff5fff4fff0",
    cmd_uuid: "0000fff5-0000-1000-8000-00805f9b34fb",
    state_uuid: "0000fff6-0000-1000-8000-00805f9b34fb",
    ota_service_uuid: "f95a48e6-a721-11e9-a2a3-022ae2dbcce4",
    ota_version_uuid: "f95a4b66-a721-11e9-a2a3-022ae2dbcce4",
    ota_write_uuid: "f95a5034-a721-11e9-a2a3-022ae2dbcce4",
    ota_notify_uuid: "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3",
    ota_data_uuid: "f95a4cce-a721-11e9-a2a3-022ae2dbcce4",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_GEN2,
    base_iv: &C_IV_GEN2,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

const CFG_GAN251: ProtoCfg = ProtoCfg {
    proto: Protocol::Gan251,
    // V3-2 model; the app maps `gan251ui_` -> ProtocolV3-2 (its own AES key).
    device_name: "GAN251ui_EMU",
    mac: &MAC_GAN251,
    service_uuid: "00000010-0000-fff7-fff6-fff5fff4fff0",
    cmd_uuid: "0000fff5-0000-1000-8000-00805f9b34fb",
    state_uuid: "0000fff6-0000-1000-8000-00805f9b34fb",
    ota_service_uuid: "f95a48e6-a721-11e9-a2a3-022ae2dbcce4",
    ota_version_uuid: "f95a4b66-a721-11e9-a2a3-022ae2dbcce4",
    ota_write_uuid: "f95a5034-a721-11e9-a2a3-022ae2dbcce4",
    ota_notify_uuid: "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3",
    ota_data_uuid: "f95a4cce-a721-11e9-a2a3-022ae2dbcce4",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_GAN251,
    base_iv: &C_IV_GAN251,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

const CFG_MOYU32: ProtoCfg = ProtoCfg {
    proto: Protocol::MoYu32,
    device_name: "WCU_MY32_A4CE",
    mac: &MAC_MOYU32,
    service_uuid: "0783b03e-7735-b5a0-1760-a305d2795cb0",
    cmd_uuid: "0783b03e-7735-b5a0-1760-a305d2795cb2",
    state_uuid: "0783b03e-7735-b5a0-1760-a305d2795cb1",
    ota_service_uuid: "02f00000-0000-0000-0000-00000000fe00",
    ota_version_uuid: "00000000-0000-0000-0000-000000000000",
    ota_write_uuid: "02f00000-0000-0000-0000-00000000ff01",
    ota_notify_uuid: "02f00000-0000-0000-0000-00000000ff02",
    ota_data_uuid: "00000000-0000-0000-0000-000000000000",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_MOYU32,
    base_iv: &C_IV_MOYU32,
    mfr_cic: 0x0000, // unbound cube; bound cubes use account-derived CIC
    mfr_extra: C_MFR_EXTRA_MOYU32,
};

// WCU app compares this (161 software version) against the cloud firmware.json
// (currently 2.12) to decide whether to offer an OTA upgrade.
pub const MOYU32_SW_MAJOR: u8 = 2;
pub const MOYU32_SW_MINOR: u8 = 12;
// Firmware start address reported to the WCU OTA client ("GetStartAddress").
pub const MOYU32_OTA_BASE: u32 = 0x0001_0000;

const CFG_QIYI: ProtoCfg = ProtoCfg {
    proto: Protocol::QiYi,
    device_name: "QY-QYSC-A-EMU",
    mac: &MAC_QIYI,
    service_uuid: "0000fff0-0000-1000-8000-00805f9b34fb",
    cmd_uuid: "0000fff6-0000-1000-8000-00805f9b34fb",
    state_uuid: "0000fff6-0000-1000-8000-00805f9b34fb",
    ota_service_uuid: "00000000-0000-0000-0000-000000000000",
    ota_version_uuid: "00000000-0000-0000-0000-000000000000",
    ota_write_uuid: "00000000-0000-0000-0000-000000000000",
    ota_notify_uuid: "00000000-0000-0000-0000-000000000000",
    ota_data_uuid: "00000000-0000-0000-0000-000000000000",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_QIYI,
    base_iv: &[0u8; 16],
    mfr_cic: 0x0504,
    mfr_extra: C_MFR_EXTRA,
};

const CFG_ROBOT: ProtoCfg = ProtoCfg {
    proto: Protocol::Robot,
    device_name: "GAN-ROBOT",
    mac: &MAC_ROBOT,
    // RobotV1 uses same service UUID as Gen3, but no encryption
    service_uuid: "8653000a-43e6-47b7-9cb0-5fc21d4ae340",
    cmd_uuid: "8653000c-43e6-47b7-9cb0-5fc21d4ae340",
    state_uuid: "8653000b-43e6-47b7-9cb0-5fc21d4ae340",
    ota_service_uuid: "f95a48e6-a721-11e9-a2a3-022ae2dbcce4",
    ota_version_uuid: "f95a4b66-a721-11e9-a2a3-022ae2dbcce4",
    ota_write_uuid: "f95a5034-a721-11e9-a2a3-022ae2dbcce4",
    ota_notify_uuid: "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3",
    ota_data_uuid: "f95a4cce-a721-11e9-a2a3-022ae2dbcce4",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_GEN2,
    base_iv: &C_IV_GEN2,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

const CFG_TIMER: ProtoCfg = ProtoCfg {
    proto: Protocol::Timer,
    device_name: "GAN-TIMER",
    mac: &MAC_TIMER,
    // TimerV1: service FFF0, char FFF5, no encryption, CRC16
    service_uuid: "0000fff0-0000-1000-8000-00805f9b34fb",
    cmd_uuid: "0000fff5-0000-1000-8000-00805f9b34fb",
    state_uuid: "0000fff5-0000-1000-8000-00805f9b34fb",
    ota_service_uuid: "f95a48e6-a721-11e9-a2a3-022ae2dbcce4",
    ota_version_uuid: "f95a4b66-a721-11e9-a2a3-022ae2dbcce4",
    ota_write_uuid: "f95a5034-a721-11e9-a2a3-022ae2dbcce4",
    ota_data_uuid: "f95a4cce-a721-11e9-a2a3-022ae2dbcce4",
    ota_notify_uuid: "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_GEN2,
    base_iv: &C_IV_GEN2,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

pub const PROTOCOLS: &[ProtoCfg] = &[CFG_GEN2, CFG_GEN3, CFG_GEN4, CFG_GAN251, CFG_MOYU32, CFG_QIYI, CFG_ROBOT, CFG_TIMER];

pub fn get_config(proto: Protocol) -> &'static ProtoCfg {
    &PROTOCOLS[proto as usize]
}
