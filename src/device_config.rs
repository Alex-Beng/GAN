pub const DEVICE_NAME: &str = "GANi3SB";
pub const WIFI_MODE_AP: bool = false;
pub const WIFI_AP_SSID: &str = "GanCubeEmu";
pub const WIFI_AP_PASS: &str = "";
pub const WIFI_STA_SSID: &str = "BigTalent";
pub const WIFI_STA_PASS: &str = "88882727";
pub const MAC: [u8; 6] = [0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xCA];
pub const MANUFACTURER_CIC: u16 = 0x0001;
pub const MANUFACTURER_EXTRA: [u8; 3] = [0x00, 0x00, 0x00];

pub const SERVICE_UUID: &str    = "6e400001-b5a3-f393-e0a9-e50e24dc4179";
pub const CMD_CHAR_UUID: &str   = "28be4a4a-cd67-11e9-a32f-2a2ae2dbcce4";
pub const STATE_CHAR_UUID: &str = "28be4cb6-cd67-11e9-a32f-2a2ae2dbcce4";

pub const OTA_SERVICE_UUID: &str      = "f95a48e6-a721-11e9-a2a3-022ae2dbcce4";
pub const OTA_VERSION_CHAR_UUID: &str = "f95a4b66-a721-11e9-a2a3-022ae2dbcce4";
pub const OTA_WRITE_CHAR_UUID: &str   = "f95a5034-a721-11e9-a2a3-022ae2dbcce4";
pub const OTA_NOTIFY_CHAR_UUID: &str  = "ec4cff6d-81fc-4e5b-91e0-8103885c9ae3";
pub const OTA_VERSION: &[u8; 16]      = b"1.1.4-pre\x00\x00\x00\x00\x00\x00\x00";

pub const GAN_BASE_KEY: [u8; 16] = [
    0x01, 0x02, 0x42, 0x28, 0x31, 0x91, 0x16, 0x07,
    0x20, 0x05, 0x18, 0x54, 0x42, 0x11, 0x12, 0x53,
];
pub const GAN_BASE_IV: [u8; 16] = [
    0x11, 0x03, 0x32, 0x28, 0x21, 0x01, 0x76, 0x27,
    0x20, 0x95, 0x78, 0x14, 0x32, 0x12, 0x02, 0x43,
];
pub const AICUBE_BASE_KEY: [u8; 16] = [
    0x05, 0x12, 0x02, 0x45, 0x02, 0x01, 0x29, 0x56,
    0x12, 0x78, 0x12, 0x76, 0x81, 0x01, 0x08, 0x03,
];
pub const AICUBE_BASE_IV: [u8; 16] = [
    0x01, 0x44, 0x28, 0x06, 0x86, 0x21, 0x22, 0x28,
    0x51, 0x05, 0x08, 0x31, 0x82, 0x02, 0x21, 0x06,
];
