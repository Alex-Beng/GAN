//! Watch-specific HTTP endpoints, registered on the server returned by
//! `shared::config_http::start`.
//!
//! These exist so the phone can provision the watch without a keyboard on the
//! device: WiFi is already handled by the shared UI, and this module adds
//! account login plus read-only status.
//!
//! Network calls (HTTPS login) are **not** done on the httpd task: they run in a
//! short-lived thread with a proper stack, and the handler returns immediately.

use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use serde_json::json;

use shared::api::auth::{parse_login, parse_user};
use shared::api::{ApiClient, ClientConfig};

use crate::state::SharedState;

/// Read the request body. `Request::read` is an inherent method, so this is a
/// macro rather than a generic helper.
macro_rules! read_body {
    ($req:expr) => {{
        let mut body: Vec<u8> = Vec::new();
        let mut buf = [0u8; 256];
        loop {
            match $req.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        String::from_utf8_lossy(&body).to_string()
    }};
}

pub struct Deps {
    pub app: SharedState,
    pub nvs: EspDefaultNvsPartition,
    /// Set to request an immediate upload attempt.
    pub force_upload: Arc<Mutex<bool>>,
}

pub fn register(server: &mut EspHttpServer<'static>, deps: Deps) -> Result<()> {
    let app = deps.app.clone();
    server.fn_handler("/api/watch/state", Method::Get, move |mut req| {
        shared::config_http::tune_socket_timeouts(req.connection());
        let body = {
            let st = app.lock().unwrap();
            json!({
                "revision": st.revision,
                "page": st.page.title(),
                "ble_connected": st.ble_connected,
                "ble_name": st.ble_name,
                "battery": st.battery,
                "wifi_ok": st.wifi_ok,
                "logged_in": st.logged_in,
                "user_id": st.user_id,
                "nick": st.nick,
                "pending": st.pending_uploads,
                "phase": format!("{:?}", st.phase()),
                "scramble": st.scramble_text(),
                "status": st.status_msg,
            })
            .to_string()
        };
        let _ = req.into_ok_response()?.write_all(body.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let app = deps.app.clone();
    server.fn_handler("/api/watch/records", Method::Get, move |mut req| {
        shared::config_http::tune_socket_timeouts(req.connection());
        let body = {
            let st = app.lock().unwrap();
            let rows: Vec<_> = st
                .recent
                .iter()
                .map(|r| {
                    json!({
                        "ms": r.duration_ms,
                        "penalty": format!("{:?}", r.penalty),
                        "scramble": r.scramble,
                    })
                })
                .collect();
            json!({ "count": st.recent.len(), "rows": rows }).to_string()
        };
        let _ = req.into_ok_response()?.write_all(body.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    // --- login ---------------------------------------------------------------

    let nvs = deps.nvs.clone();
    let app = deps.app.clone();
    server.fn_handler("/api/watch/auth/send", Method::Post, move |mut req| {
        shared::config_http::tune_socket_timeouts(req.connection());
        let body = read_body!(req);
        let phone = shared::config_http::parse_json_str(&body, "phone")
            .unwrap_or("")
            .to_string();
        let area = shared::config_http::parse_json_str(&body, "area")
            .unwrap_or("+86")
            .to_string();
        if phone.is_empty() {
            let _ = req
                .into_ok_response()?
                .write_all(b"{\"error\":\"missing phone\"}");
            return Ok::<(), anyhow::Error>(());
        }
        let nvs2 = nvs.clone();
        let app2 = app.clone();
        thread::Builder::new()
            .name("login-send".into())
            .stack_size(16384)
            .spawn(move || {
                let mut settings = crate::config::load(&nvs2);
                let client = client_for(&settings);
                match client.send_login_captcha(&area, &phone) {
                    Ok(reply) => {
                        let msg = if reply.ok() {
                            "captcha sent".to_string()
                        } else {
                            format!("{:?} {}", reply.code, reply.message.unwrap_or_default())
                        };
                        log::info!("[login] send: {msg}");
                        let mut st = app2.lock().unwrap();
                        st.status_msg = msg;
                        st.bump();
                        let _ = &mut settings;
                    }
                    Err(e) => {
                        log::error!("[login] send failed: {e}");
                        let mut st = app2.lock().unwrap();
                        st.status_msg = format!("send failed: {e}");
                        st.bump();
                    }
                }
            })
            .ok();
        let _ = req
            .into_ok_response()?
            .write_all(b"{\"status\":\"pending\"}");
        Ok::<(), anyhow::Error>(())
    })?;

    let nvs = deps.nvs.clone();
    let app = deps.app.clone();
    server.fn_handler("/api/watch/auth/login", Method::Post, move |mut req| {
        shared::config_http::tune_socket_timeouts(req.connection());
        let body = read_body!(req);
        let phone = shared::config_http::parse_json_str(&body, "phone")
            .unwrap_or("")
            .to_string();
        let code = shared::config_http::parse_json_str(&body, "code")
            .unwrap_or("")
            .to_string();
        let area = shared::config_http::parse_json_str(&body, "area")
            .unwrap_or("+86")
            .to_string();
        if phone.is_empty() || code.is_empty() {
            let _ = req
                .into_ok_response()?
                .write_all(b"{\"error\":\"missing phone or code\"}");
            return Ok::<(), anyhow::Error>(());
        }
        let nvs2 = nvs.clone();
        let app2 = app.clone();
        thread::Builder::new()
            .name("login-verify".into())
            .stack_size(20480)
            .spawn(move || do_login(&nvs2, &app2, &area, &phone, &code))
            .ok();
        let _ = req
            .into_ok_response()?
            .write_all(b"{\"status\":\"pending\"}");
        Ok::<(), anyhow::Error>(())
    })?;

    // --- manual upload trigger ----------------------------------------------

    let force = deps.force_upload.clone();
    server.fn_handler("/api/watch/upload", Method::Post, move |mut req| {
        shared::config_http::tune_socket_timeouts(req.connection());
        *force.lock().unwrap() = true;
        let _ = req
            .into_ok_response()?
            .write_all(b"{\"status\":\"queued\"}");
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

fn client_for(settings: &crate::config::Settings) -> ApiClient {
    ApiClient::new(ClientConfig {
        base: settings.gateway.clone(),
        device_id: settings.device_id.clone(),
        ..Default::default()
    })
}

fn do_login(
    nvs: &EspDefaultNvsPartition,
    app: &SharedState,
    area: &str,
    phone: &str,
    code: &str,
) {
    let mut settings = crate::config::load(nvs);
    let client = client_for(&settings);
    let now = shared::api::now_ms();

    let reply = match client.login_by_captcha(area, phone, code, now) {
        Ok(r) => r,
        Err(e) => {
            set_status(app, format!("login failed: {e}"));
            return;
        }
    };
    let Some(login) = parse_login(&reply) else {
        set_status(
            app,
            format!(
                "login rejected: {:?} {}",
                reply.code,
                reply.message.unwrap_or_default()
            ),
        );
        return;
    };

    settings.token = login.token.clone();
    let mut client = client;
    client.token = Some(login.token);

    match client.user_info(now) {
        Ok(info) => {
            if let Some(user) = parse_user(&info) {
                settings.user_id = user.app_user_id;
                settings.nick = user.nick_name;
            }
        }
        Err(e) => {
            log::warn!("[login] user_info failed: {e}");
        }
    }

    if let Err(e) = crate::config::save(nvs, &settings) {
        set_status(app, format!("save failed: {e}"));
        return;
    }
    let mut st = app.lock().unwrap();
    st.user_id = settings.user_id;
    st.nick = settings.nick.clone();
    st.logged_in = settings.user_id != 0;
    st.status_msg = format!("logged in as {} ({})", settings.nick, settings.user_id);
    st.bump();
}

fn set_status(app: &SharedState, msg: String) {
    log::warn!("[login] {msg}");
    let mut st = app.lock().unwrap();
    st.status_msg = msg;
    st.bump();
}
