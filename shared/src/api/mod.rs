//! Minimal CubeStation gateway client for on-device use.
//!
//! Only what the cubebox firmware needs: build the common header set, POST/GET
//! JSON over HTTPS (verified against the ESP-IDF CA bundle), and expose the
//! service `code`/`message` pair.
//!
//! Verified behaviour (UAT, 2026-11): the `timer/*` and `academy/*` services do
//! **not** require the Android `signature` header, and `timer/*` does not even
//! require a token — records are attributed by the `user_id` field in the body.

pub mod auth;
pub mod timer;

use std::time::Duration;

use anyhow::{anyhow, Result};
use esp_idf_svc::http::client::{Configuration as HttpConfig, EspHttpConnection};
use esp_idf_svc::http::Method;
use esp_idf_svc::sys;

pub const GATEWAY_PROD: &str = "https://prod-api.cubestation.cc/gw/";
pub const GATEWAY_PROD_ALT: &str = "https://prod-api.gancube.cc/gw/";
pub const GATEWAY_UAT: &str = "https://uat-api.cubestation.cc/gw/";

/// Header values that identify the client to the gateway. These mirror
/// `BaseInterceptor` on the Android side; none of them are secret.
#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub base: String,
    pub device_id: String,
    pub app_id: u32,
    pub sub_app_id: u32,
    pub channel: String,
    pub client_version: String,
    pub os_version: String,
    pub device_model: String,
    pub brand: String,
    pub language: String,
    pub origin_language: String,
    pub user_type: u32,
    pub client_zone: i32,
    pub os_type: u32,
    pub timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base: GATEWAY_PROD.to_string(),
            device_id: "00000000-0000-0000-0000-000000000000".to_string(),
            app_id: 1,
            sub_app_id: 6,
            channel: "10003".to_string(),
            client_version: "6.8".to_string(),
            // These headers are informational telemetry: `timer/*` and
            // `ganUser/*` do not validate `deviceModel`, `brand` or
            // `osVersion` (verified against the gateway). Report the real
            // hardware instead of pretending to be a phone.
            os_version: "5.5.3".to_string(),
            device_model: "ESP32-S3".to_string(),
            brand: "Espressif".to_string(),
            language: "zh_cn".to_string(),
            origin_language: "zh".to_string(),
            user_type: 3,
            client_zone: 28800,
            // The gateway only knows Android(1)/iOS(2); keep the Android value
            // so analytics buckets stay meaningful.
            os_type: 1,
            timeout: Duration::from_secs(12),
        }
    }
}

/// Parsed gateway envelope. `code` is normalized to a string because some
/// services answer `{"code":200}` and others `{"code":"200"}`.
#[derive(Clone, Debug)]
pub struct ApiReply {
    pub status: u16,
    pub body: String,
    pub code: Option<String>,
    pub message: Option<String>,
}

impl ApiReply {
    pub fn ok(&self) -> bool {
        self.code.as_deref() == Some("200")
    }

    pub fn data(&self) -> Option<serde_json::Value> {
        let v: serde_json::Value = serde_json::from_str(&self.body).ok()?;
        v.get("data").cloned()
    }

    /// Error suitable for logging.
    pub fn err(&self) -> anyhow::Error {
        anyhow!(
            "gateway code={:?} status={} msg={:?}",
            self.code,
            self.status,
            self.message
        )
    }
}

pub struct ApiClient {
    pub cfg: ClientConfig,
    /// Session token, when logged in. Unused by `timer/*` but kept for account
    /// endpoints.
    pub token: Option<String>,
    pub user_id: Option<i64>,
}

impl ApiClient {
    pub fn new(cfg: ClientConfig) -> Self {
        Self {
            cfg,
            token: None,
            user_id: None,
        }
    }

    pub fn url(&self, path: &str) -> String {
        let mut base = self.cfg.base.clone();
        if !base.ends_with('/') {
            base.push('/');
        }
        format!("{base}{}", path.trim_start_matches('/'))
    }

    /// Build the common header set plus extras. `body_len` adds Content-Length.
    fn headers(&self, extra: &[(&str, String)], body_len: Option<usize>) -> Vec<(String, String)> {
        let now = now_ms().to_string();
        let mut h: Vec<(String, String)> = vec![
            ("Content-Type".into(), "application/json".into()),
            ("User-Agent".into(), "okhttp/4.9.0".into()),
            ("deviceId".into(), self.cfg.device_id.clone()),
            ("appId".into(), self.cfg.app_id.to_string()),
            ("subAppId".into(), self.cfg.sub_app_id.to_string()),
            ("channel".into(), self.cfg.channel.clone()),
            ("clientEventTime".into(), now),
            ("clientVersion".into(), self.cfg.client_version.clone()),
            ("osVersion".into(), self.cfg.os_version.clone()),
            ("deviceModel".into(), self.cfg.device_model.clone()),
            ("brand".into(), self.cfg.brand.clone()),
            ("language".into(), self.cfg.language.clone()),
            ("originLanguage".into(), self.cfg.origin_language.clone()),
            ("userType".into(), self.cfg.user_type.to_string()),
            ("clientZone".into(), self.cfg.client_zone.to_string()),
            ("osType".into(), self.cfg.os_type.to_string()),
            ("responseBodyNoEncryption".into(), "yes".into()),
        ];
        if let Some(len) = body_len {
            h.push(("Content-Length".into(), len.to_string()));
        }
        if let Some(token) = &self.token {
            h.push(("token".into(), token.clone()));
        }
        if let Some(uid) = self.user_id {
            h.push(("appUserId".into(), uid.to_string()));
            h.push(("UserId".into(), uid.to_string()));
        }
        for (k, v) in extra {
            h.push(((*k).to_string(), v.clone()));
        }
        h
    }

    fn perform(
        &self,
        method: Method,
        path_or_url: &str,
        body: Option<&str>,
        extra: &[(&str, String)],
    ) -> Result<ApiReply> {
        let url = if path_or_url.starts_with("http") {
            path_or_url.to_string()
        } else {
            self.url(path_or_url)
        };

        let conf = HttpConfig {
            buffer_size: Some(4096),
            buffer_size_tx: Some(2048),
            timeout: Some(self.cfg.timeout),
            crt_bundle_attach: Some(sys::esp_crt_bundle_attach),
            keep_alive_enable: true,
            ..Default::default()
        };

        let mut conn = EspHttpConnection::new(&conf)
            .map_err(|e| anyhow!("http connection init failed: {e:?}"))?;

        let owned = self.headers(extra, body.map(|b| b.len()));
        let hdrs: Vec<(&str, &str)> = owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        conn.initiate_request(method, &url, &hdrs)
            .map_err(|e| anyhow!("initiate_request failed: {e:?}"))?;

        if let Some(b) = body {
            conn.write_all(b.as_bytes())
                .map_err(|e| anyhow!("request write failed: {e:?}"))?;
        }

        conn.initiate_response()
            .map_err(|e| anyhow!("initiate_response failed: {e:?}"))?;

        let status = conn.status();
        let mut buf = [0u8; 1024];
        let mut out: Vec<u8> = Vec::with_capacity(512);
        loop {
            match conn.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    out.extend_from_slice(&buf[..n]);
                    // Hard cap: never buffer an unbounded response.
                    if out.len() > 32 * 1024 {
                        break;
                    }
                }
                Err(e) => return Err(anyhow!("response read failed: {e:?}")),
            }
        }
        let body_text = String::from_utf8_lossy(&out).to_string();
        Ok(parse_reply(status, body_text))
    }

    pub fn post_json(&self, path: &str, body: &str) -> Result<ApiReply> {
        self.perform(Method::Post, path, Some(body), &[])
    }

    pub fn post_json_with(&self, path: &str, body: &str, extra: &[(&str, String)]) -> Result<ApiReply> {
        self.perform(Method::Post, path, Some(body), extra)
    }

    pub fn get(&self, path: &str) -> Result<ApiReply> {
        self.perform(Method::Get, path, None, &[])
    }
}

fn parse_reply(status: u16, body: String) -> ApiReply {
    let mut code = None;
    let mut message = None;
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
        if let Some(c) = v.get("code") {
            code = Some(match c {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                other => other.to_string(),
            });
        }
        message = v
            .get("message")
            .or_else(|| v.get("msg"))
            .and_then(|m| m.as_str())
            .map(|s| s.to_string());
    }
    ApiReply {
        status,
        body,
        code,
        message,
    }
}

/// Milliseconds since boot. Good enough for `clientEventTime` (no signature is
/// verified by the gateway); wall-clock values come from SNTP when needed.
pub fn now_ms() -> u64 {
    unsafe { (sys::esp_timer_get_time() / 1000) as u64 }
}

/// 64-bit hardware random value, used to seed the scramble PRNG.
pub fn hw_random() -> u64 {
    unsafe { ((sys::esp_random() as u64) << 32) | (sys::esp_random() as u64) }
}
