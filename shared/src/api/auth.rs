//! `ganUser` account endpoints: SMS captcha login and token -> user info.
//!
//! The watch only needs a one-time login to learn `appUserId` (the `timer`
//! service attributes records by the `user_id` body field, not by token).
//!
//! Verified against UAT: no `signature` header is required, but the
//! `clientEventTime` header is mandatory and the phone must be sent as
//! `+86 13800000000` (plus sign and space percent-encoded in the query).

use anyhow::Result;
use serde_json::Value;

use super::{ApiClient, ApiReply};

pub const PATH_SEND_CAPTCHA: &str = "ganUser/api/v2/user/sms/sendLoginCaptcha";
pub const PATH_LOGIN_BY_CAPTCHA: &str = "ganUser/api/v2/user/login/phoneLoginByCaptcha";
pub const PATH_USER_INFO: &str = "ganUser/api/v2/user/info/getUserInfoByTokenLogin";

/// Successful login payload.
#[derive(Clone, Debug, Default)]
pub struct LoginResult {
    pub token: String,
    pub expire_timestamp: i64,
    pub is_new_user: bool,
}

/// Public profile fields the watch cares about.
#[derive(Clone, Debug, Default)]
pub struct UserProfile {
    pub app_user_id: i64,
    pub union_user_id: i64,
    pub nick_name: String,
    pub avatar: String,
}

/// Format a phone the way the gateway expects: `+86 13800000000`.
pub fn format_phone(area_code: &str, phone: &str) -> String {
    let area = if area_code.is_empty() { "+86" } else { area_code };
    let digits: String = phone.chars().filter(|c| !c.is_whitespace()).collect();
    format!("{area} {digits}")
}

/// Percent-encode a query value (RFC3986 unreserved characters pass through).
fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for b in value.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

impl ApiClient {
    /// Ask the gateway to send a login SMS.
    pub fn send_login_captcha(&self, area_code: &str, phone: &str) -> Result<ApiReply> {
        let full = format_phone(area_code, phone);
        let path = format!("{PATH_SEND_CAPTCHA}?phone={}", encode_query(&full));
        self.post_json(&path, "{}")
    }

    /// Exchange phone + captcha for a token.
    pub fn login_by_captcha(
        &self,
        area_code: &str,
        phone: &str,
        captcha: &str,
        client_event_time_ms: u64,
    ) -> Result<ApiReply> {
        let body = format!(
            "{{\"appId\":{},\"captcha\":\"{}\",\"clientEventTime\":{},\"phone\":\"{}\"}}",
            self.cfg.app_id,
            escape(captcha),
            client_event_time_ms,
            escape(&format_phone(area_code, phone)),
        );
        self.post_json(PATH_LOGIN_BY_CAPTCHA, &body)
    }

    /// Fetch the logged-in profile. Requires `self.token` to be set.
    pub fn user_info(&self, client_event_time_ms: u64) -> Result<ApiReply> {
        let path = format!("{PATH_USER_INFO}?clientEventTime={client_event_time_ms}");
        self.get(&path)
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    crate::timer::record::push_escaped(&mut out, s);
    // push_escaped adds the surrounding quotes; strip them for inline use.
    out.trim_matches('"').to_string()
}

pub fn parse_login(reply: &ApiReply) -> Option<LoginResult> {
    let data = reply.data()?;
    Some(LoginResult {
        token: data.get("token")?.as_str()?.to_string(),
        expire_timestamp: data
            .get("expireTimestamp")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        is_new_user: data
            .get("isNewUser")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub fn parse_user(reply: &ApiReply) -> Option<UserProfile> {
    let data = reply.data()?;
    let as_i64 = |k: &str| -> i64 {
        data.get(k)
            .and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
            .unwrap_or(0)
    };
    Some(UserProfile {
        app_user_id: as_i64("appUserID"),
        union_user_id: as_i64("unionUserID"),
        nick_name: data
            .get("nickName")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        avatar: data
            .get("avatar")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}
