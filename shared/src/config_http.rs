//! Shared HTTP configuration endpoints for both `emu` and `proxy` firmwares:
//! the web UI, WiFi provisioning, output-protocol selection, and read-only
//! device/state queries.
//!
//! Firmware-specific endpoints (e.g. emu's move injection) are registered
//! separately by the caller on the same [`EspHttpServer`].

use std::sync::{Arc, Mutex};

use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

use crate::device_config::{self, Protocol};
use crate::protocol::{self, CubeState};
use crate::wifi::{self, SharedWifi};

static INDEX_HTML: &str = include_str!("index.html");

/// CPU temperature sensor wrapper (RAII).
struct CpuTemp {
    handle: esp_idf_svc::sys::temperature_sensor_handle_t,
}

unsafe impl Send for CpuTemp {}
unsafe impl Sync for CpuTemp {}

impl CpuTemp {
    fn new() -> anyhow::Result<Self> {
        let config = esp_idf_svc::sys::temperature_sensor_config_t {
            range_min: -10,
            range_max: 80,
            clk_src: esp_idf_svc::sys::soc_periph_temperature_sensor_clk_src_t_TEMPERATURE_SENSOR_CLK_SRC_DEFAULT,
            flags: esp_idf_svc::sys::temperature_sensor_config_t__bindgen_ty_1 { allow_pd: 0 },
        };
        let mut handle: esp_idf_svc::sys::temperature_sensor_handle_t = core::ptr::null_mut();
        let ret = unsafe { esp_idf_svc::sys::temperature_sensor_install(&config, &mut handle) };
        if ret != 0 {
            anyhow::bail!("temperature_sensor_install failed: {}", ret);
        }
        let ret = unsafe { esp_idf_svc::sys::temperature_sensor_enable(handle) };
        if ret != 0 {
            anyhow::bail!("temperature_sensor_enable failed: {}", ret);
        }
        Ok(Self { handle })
    }

    fn get_celsius(&self) -> anyhow::Result<f32> {
        let mut temp: f32 = 0.0;
        let ret = unsafe { esp_idf_svc::sys::temperature_sensor_get_celsius(self.handle, &mut temp) };
        if ret != 0 {
            anyhow::bail!("get_celsius failed: {}", ret);
        }
        Ok(temp)
    }
}

impl Drop for CpuTemp {
    fn drop(&mut self) {
        unsafe {
            esp_idf_svc::sys::temperature_sensor_disable(self.handle);
            esp_idf_svc::sys::temperature_sensor_uninstall(self.handle);
        }
    }
}

/// Dependencies the shared config endpoints need from the firmware.
pub struct ConfigHttpDeps {
    pub wifi: SharedWifi,
    pub nvs: EspDefaultNvsPartition,
    pub cube_state: Arc<Mutex<CubeState>>,
    pub protocol: Arc<Mutex<Protocol>>,
    /// Called when the output protocol is changed (before restart). Firmware can
    /// persist and/or pre-set the BT MAC here.
    pub on_protocol_change: Option<Box<dyn Fn(Protocol) + Send + Sync>>,
}

/// Create the shared HTTP server (web UI + config endpoints) and return it.
///
/// Callers may register additional firmware-specific handlers on the returned
/// server before it goes out of scope, but must keep it alive.
pub fn start(deps: ConfigHttpDeps) -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&Configuration {
        max_open_sockets: 7,
        ..Default::default()
    })?;

    let temp_sensor = match CpuTemp::new() {
        Ok(t) => Some(Arc::new(Mutex::new(t))),
        Err(e) => {
            log::warn!("Temperature sensor init failed: {}, temp API disabled", e);
            None
        }
    };

    server.fn_handler("/", Method::Get, |req| {
        let _ = req.into_ok_response()?.write_all(INDEX_HTML.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let wf1 = deps.wifi.clone();
    let nvs1 = deps.nvs.clone();
    server.fn_handler("/api/wifi/mode", Method::Post, move |mut req| {
        let mut body = Vec::new();
        let mut buf = [0u8; 128];
        loop {
            match req.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        let resp_text = if let Ok(s) = std::str::from_utf8(&body) {
            let is_sta = parse_json_u8(s, "sta").map(|v| v != 0).unwrap_or(false);
            let ssid = parse_json_str(s, "ssid").unwrap_or("");
            let pass = parse_json_str(s, "pass").unwrap_or("");
            match wifi::switch_mode(&wf1, &nvs1, is_sta, ssid, pass) {
                Ok(msg) => msg,
                Err(e) => std::format!("error:{}", e),
            }
        } else {
            "bad request".to_string()
        };
        let _ = req.into_ok_response()?.write_all(resp_text.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let wf_ap_off = deps.wifi.clone();
    let nvs_ap_off = deps.nvs.clone();
    server.fn_handler("/api/wifi/ap/off", Method::Post, move |req| {
        match wifi::disable_ap(&wf_ap_off, &nvs_ap_off) {
            Ok(()) => {
                let _ = req.into_ok_response()?.write_all(b"AP disabled");
            }
            Err(e) => {
                let _ = req
                    .into_response(500, Some("Error"), &[])?
                    .write_all(format!("error:{}", e).as_bytes());
            }
        }
        Ok::<(), anyhow::Error>(())
    })?;

    let wf_status = deps.wifi.clone();
    server.fn_handler("/api/wifi/status", Method::Get, move |req| {
        let (mode, ap_active, sta_connected, ip) = wifi::get_wifi_status(&wf_status);
        let json = std::format!(
            "{{\"mode\":\"{}\",\"ap_active\":{},\"sta_connected\":{},\"ip\":\"{}\"}}",
            mode,
            ap_active,
            sta_connected,
            ip.unwrap_or_default()
        );
        let _ = req.into_ok_response()?.write_all(json.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let wf_scan = deps.wifi.clone();
    server.fn_handler("/api/wifi/scan", Method::Get, move |req| {
        let json = wifi::scan_and_format_json(&wf_scan)
            .unwrap_or_else(|e| std::format!("{{\"error\":\"{}\"}}", e));
        let _ = req.into_ok_response()?.write_all(json.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let nvs_saved = deps.nvs.clone();
    server.fn_handler("/api/wifi/saved", Method::Get, move |req| {
        let saved = wifi::load_all_sta_configs(&nvs_saved);
        let mut json = String::from("{\"saved\":[");
        for (i, net) in saved.iter().enumerate() {
            if i > 0 {
                json.push(',');
            }
            json.push_str(&std::format!("{{\"ssid\":\"{}\"}}", net.ssid));
        }
        json.push_str("]}");
        let _ = req.into_ok_response()?.write_all(json.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let nvs_saved2 = deps.nvs.clone();
    server.fn_handler("/api/wifi/saved", Method::Post, move |mut req| {
        let mut body = Vec::new();
        let mut buf = [0u8; 256];
        loop {
            match req.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        let resp = if let Ok(s) = std::str::from_utf8(&body) {
            let ssid = parse_json_str(s, "ssid").unwrap_or("");
            let pass = parse_json_str(s, "pass").unwrap_or("");
            if ssid.is_empty() {
                "{\"error\":\"missing ssid\"}".to_string()
            } else {
                match wifi::save_sta_network(&nvs_saved2, ssid, pass) {
                    Ok(()) => std::format!("{{\"ssid\":\"{}\",\"saved\":true}}", ssid),
                    Err(e) => std::format!("{{\"error\":\"{}\"}}", e),
                }
            }
        } else {
            "{\"error\":\"bad request\"}".to_string()
        };
        let _ = req.into_ok_response()?.write_all(resp.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let nvs_forget = deps.nvs.clone();
    server.fn_handler("/api/wifi/saved/forget", Method::Post, move |mut req| {
        let mut body = Vec::new();
        let mut buf = [0u8; 128];
        loop {
            match req.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        let resp = if let Ok(s) = std::str::from_utf8(&body) {
            let ssid = parse_json_str(s, "ssid").unwrap_or("");
            if ssid.is_empty() {
                "{\"error\":\"missing ssid\"}".to_string()
            } else {
                match wifi::forget_sta_network(&nvs_forget, ssid) {
                    Ok(true) => std::format!("{{\"ssid\":\"{}\",\"forgotten\":true}}", ssid),
                    Ok(false) => std::format!("{{\"ssid\":\"{}\",\"forgotten\":false}}", ssid),
                    Err(e) => std::format!("{{\"error\":\"{}\"}}", e),
                }
            }
        } else {
            "{\"error\":\"bad request\"}".to_string()
        };
        let _ = req.into_ok_response()?.write_all(resp.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let cs_ref = deps.cube_state.clone();
    server.fn_handler("/api/state", Method::Get, move |req| {
        let cs = cs_ref.lock().unwrap();
        let json = std::format!(
            "{{\"cp\":[{},{},{},{},{},{},{},{}],\"co\":[{},{},{},{},{},{},{},{}],\"ep\":[{},{},{},{},{},{},{},{},{},{},{},{}],\"eo\":[{},{},{},{},{},{},{},{},{},{},{},{}]}}",
            cs.cp[0], cs.cp[1], cs.cp[2], cs.cp[3], cs.cp[4], cs.cp[5], cs.cp[6], cs.cp[7],
            cs.co[0], cs.co[1], cs.co[2], cs.co[3], cs.co[4], cs.co[5], cs.co[6], cs.co[7],
            cs.ep[0], cs.ep[1], cs.ep[2], cs.ep[3], cs.ep[4], cs.ep[5], cs.ep[6], cs.ep[7], cs.ep[8], cs.ep[9], cs.ep[10], cs.ep[11],
            cs.eo[0], cs.eo[1], cs.eo[2], cs.eo[3], cs.eo[4], cs.eo[5], cs.eo[6], cs.eo[7], cs.eo[8], cs.eo[9], cs.eo[10], cs.eo[11],
        );
        drop(cs);
        let _ = req.into_ok_response()?.write_all(json.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let cs_ref2 = deps.cube_state.clone();
    server.fn_handler("/api/facelets", Method::Get, move |req| {
        let cs = cs_ref2.lock().unwrap();
        let f = protocol::to_facelet_string(&cs);
        drop(cs);
        let s = std::str::from_utf8(&f)
            .unwrap_or("UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB");
        let _ = req
            .into_response(200, Some("OK"), &[("Access-Control-Allow-Origin", "*")])?
            .write_all(s.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    server.fn_handler("/api/freq", Method::Get, move |req| {
        let mhz = unsafe { esp_idf_svc::sys::ets_get_cpu_frequency() };
        let json = std::format!("{{\"mhz\":{}}}", mhz);
        let _ = req.into_ok_response()?.write_all(json.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let temp_ref = temp_sensor.clone();
    server.fn_handler("/api/temp", Method::Get, move |req| {
        let json = match temp_ref.as_ref() {
            Some(arc) => match arc.lock() {
                Ok(guard) => match guard.get_celsius() {
                    Ok(c) => std::format!("{{\"celsius\":{:.1}}}", c),
                    Err(_) => "{\"error\":\"read failed\"}".to_string(),
                },
                Err(_) => "{\"error\":\"lock failed\"}".to_string(),
            },
            None => "{\"error\":\"sensor not available\"}".to_string(),
        };
        let _ = req.into_ok_response()?.write_all(json.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let proto_get = deps.protocol.clone();
    server.fn_handler("/api/protocol", Method::Get, move |req| {
        let p = *proto_get.lock().unwrap();
        let json = std::format!("{{\"protocol\":\"{:?}\",\"index\":{}}}", p, p as u8);
        let _ = req.into_ok_response()?.write_all(json.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let proto_set = deps.protocol.clone();
    let on_change = deps.on_protocol_change;
    server.fn_handler("/api/protocol", Method::Post, move |mut req| {
        let mut body = Vec::new();
        let mut buf = [0u8; 64];
        loop {
            match req.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        let resp = if let Ok(s) = std::str::from_utf8(&body) {
            if let Some(idx) = parse_json_u8(s, "index") {
                if let Some(p) = Protocol::from_index(idx) {
                    *proto_set.lock().unwrap() = p;
                    if let Some(cb) = &on_change {
                        cb(p);
                    }
                    std::format!("{{\"protocol\":\"{:?}\"}}", p)
                } else {
                    "{\"error\":\"invalid index\"}".to_string()
                }
            } else {
                "{\"error\":\"bad request\"}".to_string()
            }
        } else {
            "{\"error\":\"bad request\"}".to_string()
        };
        let _ = req.into_ok_response()?.write_all(resp.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let proto_gatt = deps.protocol.clone();
    server.fn_handler("/api/gatt", Method::Get, move |req| {
        let p = *proto_gatt.lock().unwrap();
        let cfg = device_config::get_config(p);
        let json = std::format!(
            r#"{{"protocol":"{:?}","device":"{}","mac":"{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}","services":["{}","{}"],"version":"{}"}}"#,
            p,
            cfg.device_name,
            cfg.mac[0], cfg.mac[1], cfg.mac[2], cfg.mac[3], cfg.mac[4], cfg.mac[5],
            cfg.service_uuid,
            cfg.ota_service_uuid,
            core::str::from_utf8(cfg.ota_version).unwrap_or("?.?.?"),
        );
        let _ = req
            .into_response(200, Some("OK"), &[("Access-Control-Allow-Origin", "*")])?
            .write_all(json.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(server)
}

pub fn parse_json_str<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let search = std::format!("\"{}\"", key);
    let pos = s.find(&search)?;
    let after = &s[pos + search.len()..];
    let after = after.trim_start_matches(|c: char| c == ':' || c == ' ');
    if after.starts_with('"') {
        let val = &after[1..];
        let end = val.find('"')?;
        Some(&val[..end])
    } else {
        None
    }
}

pub fn parse_json_u8(s: &str, key: &str) -> Option<u8> {
    let search = std::format!("\"{}\"", key);
    let pos = s.find(&search)?;
    let after = &s[pos + search.len()..];
    let after = after.trim_start_matches(|c: char| c == ':' || c == ' ');
    let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
    after[..end].parse().ok()
}

pub fn parse_json_u64(s: &str, key: &str) -> Option<u64> {
    let search = std::format!("\"{}\"", key);
    let pos = s.find(&search)?;
    let after = &s[pos + search.len()..];
    let after = after.trim_start_matches(|c: char| c == ':' || c == ' ');
    let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
    after[..end].parse().ok()
}
