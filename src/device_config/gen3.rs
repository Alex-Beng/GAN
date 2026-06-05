pub const DEVICE_NAME: &str = "GAN12UI";
pub const MAC: [u8; 6] = [0xDC, 0x56, 0x78, 0x12, 0x34, 0xAB];
pub const MANUFACTURER_CIC: u16 = 0x0001;
pub const MANUFACTURER_EXTRA: [u8; 3] = [0x00, 0x00, 0x00];

pub const SERVICE_UUID: &str    = "8653000a-43e6-47b7-9cb0-5fc21d4ae340";
pub const CMD_CHAR_UUID: &str   = "8653000c-43e6-47b7-9cb0-5fc21d4ae340";
pub const STATE_CHAR_UUID: &str = "8653000b-43e6-47b7-9cb0-5fc21d4ae340";

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
