//! Persistent settings in NVS.
//!
//! Stored as one JSON blob so adding fields never needs a migration.

use anyhow::{anyhow, Result};
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use serde_json::{json, Value};

const NAMESPACE: &str = "watch_cfg";
const KEY: &str = "settings";
const BUF_LEN: usize = 1024;

#[derive(Clone, Debug)]
pub struct Settings {
    /// CubeStation account id. Records are attributed by this field.
    pub user_id: i64,
    pub nick: String,
    pub token: String,
    /// Stable pseudo-UUID reported as `deviceId`.
    pub device_id: String,
    /// Gateway base URL.
    pub gateway: String,
    /// 3 = 三阶, 2 = 二阶.
    pub cube_type: u8,
    /// 1 = 基础, 2 = 专项, 3 = 自由.
    pub group_type: u8,
    pub group_id: u32,
    /// 1 = smart cube, 2 = non-smart.
    pub cube_smart: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            user_id: 0,
            nick: String::new(),
            token: String::new(),
            device_id: String::new(),
            gateway: shared::api::GATEWAY_PROD.to_string(),
            cube_type: 3,
            group_type: 3,
            group_id: shared::timer::GROUP_SOLVE_3,
            cube_smart: 1,
        }
    }
}

impl Settings {
    pub fn to_json(&self) -> String {
        json!({
            "user_id": self.user_id,
            "nick": self.nick,
            "token": self.token,
            "device_id": self.device_id,
            "gateway": self.gateway,
            "cube_type": self.cube_type,
            "group_type": self.group_type,
            "group_id": self.group_id,
            "cube_smart": self.cube_smart,
        })
        .to_string()
    }

    pub fn from_json(v: &Value) -> Self {
        let mut s = Self::default();
        s.user_id = v.get("user_id").and_then(Value::as_i64).unwrap_or(0);
        s.nick = v
            .get("nick")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        s.token = v
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        s.device_id = v
            .get("device_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        s.gateway = v
            .get("gateway")
            .and_then(Value::as_str)
            .unwrap_or(shared::api::GATEWAY_PROD)
            .to_string();
        s.cube_type = v.get("cube_type").and_then(Value::as_u64).unwrap_or(3) as u8;
        s.group_type = v.get("group_type").and_then(Value::as_u64).unwrap_or(3) as u8;
        s.group_id = v.get("group_id").and_then(Value::as_u64).unwrap_or(3001) as u32;
        s.cube_smart = v.get("cube_smart").and_then(Value::as_u64).unwrap_or(1) as u8;
        s
    }
}

fn open(nvs: &EspDefaultNvsPartition, read_write: bool) -> Result<EspNvs<NvsDefault>> {
    EspNvs::new(nvs.clone(), NAMESPACE, read_write)
        .map_err(|e| anyhow!("nvs open {NAMESPACE}: {e:?}"))
}

pub fn load(nvs: &EspDefaultNvsPartition) -> Settings {
    let Ok(handle) = open(nvs, false) else {
        return Settings::default();
    };
    let mut buf = [0u8; BUF_LEN];
    match handle.get_str(KEY, &mut buf) {
        Ok(Some(s)) => serde_json::from_str::<Value>(s)
            .map(|v| Settings::from_json(&v))
            .unwrap_or_default(),
        _ => Settings::default(),
    }
}

pub fn save(nvs: &EspDefaultNvsPartition, settings: &Settings) -> Result<()> {
    let handle = open(nvs, true)?;
    handle
        .set_str(KEY, &settings.to_json())
        .map_err(|e| anyhow!("nvs set: {e:?}"))
}

/// Build a stable device id from the factory MAC on first boot.
pub fn ensure_device_id(nvs: &EspDefaultNvsPartition, settings: &mut Settings) -> Result<()> {
    if !settings.device_id.is_empty() {
        return Ok(());
    }
    let mac = unsafe { esp_idf_svc::sys::esp_random() };
    let mac2 = unsafe { esp_idf_svc::sys::esp_random() };
    settings.device_id = format!(
        "{:08x}-{:04x}-4{:03x}-a{:03x}-{:012x}",
        mac,
        (mac2 >> 16) & 0xFFFF,
        mac2 & 0x0FFF,
        (mac >> 4) & 0x0FFF,
        ((mac as u64) << 16 | (mac2 as u64 & 0xFFFF)) & 0xFFFF_FFFF_FFFF,
    );
    save(nvs, settings)
}
