pub const DEVICE_NAME: &str = "GAN251UI";
pub const MAC: [u8; 6] = [0x06, 0xFA, 0x04, 0xE5, 0x66, 0xE4];
pub const MANUFACTURER_CIC: u16 = 0x0001;
pub const MANUFACTURER_EXTRA: [u8; 3] = [0x00, 0x00, 0x00];

pub const SERVICE_UUID: &str    = "00000010-0000-fff7-fff6-fff5fff4fff0";
pub const CMD_CHAR_UUID: &str   = "0000fff5-0000-1000-8000-00805f9b34fb";
pub const STATE_CHAR_UUID: &str = "0000fff6-0000-1000-8000-00805f9b34fb";

pub const OTA_SERVICE_UUID: &str      = "f95a48e6-a721-11e9-a2a3-022ae2dbcce4";
pub const OTA_VERSION_CHAR_UUID: &str = "f95a4b66-a721-11e9-a2a3-022ae2dbcce4";
pub const OTA_WRITE_CHAR_UUID: &str   = "f95a5034-a721-11e9-a2a3-022ae2dbcce4";
pub const OTA_NOTIFY_CHAR_UUID: &str  = "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3";
pub const OTA_VERSION: &[u8; 16]      = b"1.0.0\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";

pub const BASE_KEY: [u8; 16] = [
    0x58, 0x98, 0x61, 0xfc, 0x1f, 0xec, 0xd7, 0x60,
    0x9f, 0x85, 0xd3, 0x62, 0xbe, 0x37, 0x17, 0x2c,
];
pub const BASE_IV: [u8; 16] = [
    0x7f, 0x61, 0xd0, 0x52, 0x75, 0xc1, 0x39, 0x52,
    0x08, 0x2e, 0x54, 0x1d, 0x8a, 0x78, 0x63, 0x4d,
];
