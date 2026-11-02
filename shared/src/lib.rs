//! Shared protocol/crypto/config logic used by the `emu` (GATT server),
//! `proxy` (GATT client) and `watch` (standalone timer) firmwares.

pub mod api;
pub mod config_http;
pub mod crypto;
pub mod cube;
pub mod device_config;
pub mod protocol;
pub mod room;
pub mod timer;
pub mod wifi;
