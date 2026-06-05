pub const DEVICE_NAME: &str = "GAN15UI";
pub const MAC: [u8; 6] = [0xEF, 0x67, 0x89, 0x23, 0x45, 0xBC];
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
    0x01, 0x02, 0x42, 0x28, 0x31, 0x91, 0x16, 0x07,
    0x20, 0x05, 0x18, 0x54, 0x42, 0x11, 0x12, 0x53,
];
pub const BASE_IV: [u8; 16] = [
    0x11, 0x03, 0x32, 0x28, 0x21, 0x01, 0x76, 0x27,
    0x20, 0x95, 0x78, 0x14, 0x32, 0x12, 0x02, 0x43,
];
