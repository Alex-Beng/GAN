#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Protocol {
    Gen2 = 0,
    Gen3 = 1,
    Gen4 = 2,
    Gan251 = 3,
    MoYu32 = 4,
    QiYi = 5,
}

impl Protocol {
    pub const ALL: &[Protocol] = &[
        Protocol::Gen2,
        Protocol::Gen3,
        Protocol::Gen4,
        Protocol::Gan251,
        Protocol::MoYu32,
        Protocol::QiYi,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Protocol::Gen2 => "Gen2 (GANi3SB)",
            Protocol::Gen3 => "Gen3 (GAN12UI)",
            Protocol::Gen4 => "Gen4 (GAN15UI)",
            Protocol::Gan251 => "Gan251 (GAN251UI)",
            Protocol::MoYu32 => "MoYu (WCU)",
            Protocol::QiYi => "QiYi (Smart)",
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
const MAC_MOYU32: [u8; 6] = [0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xCE];
const MAC_QIYI: [u8; 6] = [0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xCF];

const C_OTA_VERSION: &[u8; 16] = b"1.0.0\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
const C_MFR_EXTRA: &[u8; 3] = &[0x00, 0x00, 0x00];

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
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_GEN2,
    base_iv: &C_IV_GEN2,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

const CFG_GEN3: ProtoCfg = ProtoCfg {
    proto: Protocol::Gen3,
    device_name: "GAN12UI",
    mac: &MAC_GEN3,
    service_uuid: "8653000a-43e6-47b7-9cb0-5fc21d4ae340",
    cmd_uuid: "8653000c-43e6-47b7-9cb0-5fc21d4ae340",
    state_uuid: "8653000b-43e6-47b7-9cb0-5fc21d4ae340",
    ota_service_uuid: "f95a48e6-a721-11e9-a2a3-022ae2dbcce4",
    ota_version_uuid: "f95a4b66-a721-11e9-a2a3-022ae2dbcce4",
    ota_write_uuid: "f95a5034-a721-11e9-a2a3-022ae2dbcce4",
    ota_notify_uuid: "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_GEN2,
    base_iv: &C_IV_GEN2,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

const CFG_GEN4: ProtoCfg = ProtoCfg {
    proto: Protocol::Gen4,
    device_name: "GAN15UI",
    mac: &MAC_GEN4,
    service_uuid: "00000010-0000-fff7-fff6-fff5fff4fff0",
    cmd_uuid: "0000fff5-0000-1000-8000-00805f9b34fb",
    state_uuid: "0000fff6-0000-1000-8000-00805f9b34fb",
    ota_service_uuid: "f95a48e6-a721-11e9-a2a3-022ae2dbcce4",
    ota_version_uuid: "f95a4b66-a721-11e9-a2a3-022ae2dbcce4",
    ota_write_uuid: "f95a5034-a721-11e9-a2a3-022ae2dbcce4",
    ota_notify_uuid: "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_GEN2,
    base_iv: &C_IV_GEN2,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

const CFG_GAN251: ProtoCfg = ProtoCfg {
    proto: Protocol::Gan251,
    device_name: "GAN251Ui_SB",
    mac: &MAC_GAN251,
    service_uuid: "00000010-0000-fff7-fff6-fff5fff4fff0",
    cmd_uuid: "0000fff5-0000-1000-8000-00805f9b34fb",
    state_uuid: "0000fff6-0000-1000-8000-00805f9b34fb",
    ota_service_uuid: "f95a48e6-a721-11e9-a2a3-022ae2dbcce4",
    ota_version_uuid: "f95a4b66-a721-11e9-a2a3-022ae2dbcce4",
    ota_write_uuid: "f95a5034-a721-11e9-a2a3-022ae2dbcce4",
    ota_notify_uuid: "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_GAN251,
    base_iv: &C_IV_GAN251,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

const CFG_MOYU32: ProtoCfg = ProtoCfg {
    proto: Protocol::MoYu32,
    device_name: "WCU_MY32_EMU",
    mac: &MAC_MOYU32,
    service_uuid: "0783b03e-7735-b5a0-1760-a305d2795cb0",
    cmd_uuid: "0783b03e-7735-b5a0-1760-a305d2795cb2",
    state_uuid: "0783b03e-7735-b5a0-1760-a305d2795cb1",
    ota_service_uuid: "00000000-0000-0000-0000-000000000000",
    ota_version_uuid: "00000000-0000-0000-0000-000000000000",
    ota_write_uuid: "00000000-0000-0000-0000-000000000000",
    ota_notify_uuid: "00000000-0000-0000-0000-000000000000",
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_MOYU32,
    base_iv: &C_IV_MOYU32,
    mfr_cic: 0x0001,
    mfr_extra: C_MFR_EXTRA,
};

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
    ota_version: C_OTA_VERSION,
    base_key: &C_KEY_QIYI,
    base_iv: &[0u8; 16],
    mfr_cic: 0x0504,
    mfr_extra: C_MFR_EXTRA,
};

pub const PROTOCOLS: &[ProtoCfg] = &[CFG_GEN2, CFG_GEN3, CFG_GEN4, CFG_GAN251, CFG_MOYU32, CFG_QIYI];

pub fn get_config(proto: Protocol) -> &'static ProtoCfg {
    &PROTOCOLS[proto as usize]
}
