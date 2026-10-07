//! Persistent configuration for the proxy firmware (output protocol).

use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};

use shared::device_config::Protocol;

const NVS_NS_PROTO: &str = "proto";
const NVS_KEY_PROTO: &str = "active";

/// Load the persisted output protocol (default: Gen2).
pub fn load_output_protocol(nvs: &EspDefaultNvsPartition) -> Protocol {
    if let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_PROTO, false) {
        let mut buf = [0u8; 1];
        if let Ok(Some(data)) = ns.get_blob(NVS_KEY_PROTO, &mut buf) {
            return Protocol::from_index(data[0]).unwrap_or(Protocol::Gen2);
        }
    }
    Protocol::Gen2
}

/// Persist the output protocol.
pub fn save_output_protocol(nvs: &EspDefaultNvsPartition, proto: Protocol) {
    if let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_PROTO, true) {
        let _ = ns.set_blob(NVS_KEY_PROTO, &[proto as u8]);
    }
}
