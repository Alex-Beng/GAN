use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};

use crate::protocol;
use crate::wifi::{self, SharedWifi};

static INDEX_HTML: &str = include_str!("index.html");

const NVS_NS_HTTP: &str = "http_cfg";
const NVS_KEY_DELAY: &str = "move_delay";
const DEFAULT_DELAY_MS: u64 = 50;

pub const MOVE_FACE_DELAY: u8 = 99;

pub fn parse_move_notation(s: &str) -> Option<(u8, u8)> {
    let s = s.trim();
    if s.is_empty() { return None; }
    let face_char = s.chars().next()?;
    let face = match face_char {
        'U' | 'u' => 0, 'R' | 'r' => 1, 'F' | 'f' => 2,
        'D' | 'd' => 3, 'L' | 'l' => 4, 'B' | 'b' => 5,
        _ => return None,
    };
    let rest = &s[face_char.len_utf8()..];
    if rest.is_empty() {
        Some((face, 0))
    } else if rest == "'" || rest == "\u{2019}" {
        Some((face, 1))
    } else if rest == "2" {
        Some((face, 2))
    } else {
        None
    }
}

pub fn parse_move_item(s: &str) -> Option<(u8, u8)> {
    if let Some(v) = parse_json_u8(s, "delay") {
        if v > 0 { return Some((MOVE_FACE_DELAY, v)); }
    }
    if let Some(name) = parse_json_str(s, "move") {
        return parse_move_notation(name);
    }
    let face = parse_json_u8(s, "face");
    let dir = parse_json_u8(s, "dir");
    match (face, dir) {
        (Some(f), Some(d)) => Some((f, d)),
        _ => None,
    }
}

pub fn push_move_or_delay(queue: &mut VecDeque<(u8, u8)>, f: u8, d: u8) -> String {
    if f == MOVE_FACE_DELAY {
        queue.push_back((f, d));
        std::format!("delay={}", d)
    } else if f < 6 && d < 3 {
        let names = ["U", "R", "F", "D", "L", "B"];
        if d == 2 {
            queue.push_back((f, 0));
            queue.push_back((f, 0));
            std::format!("{}2", names[f as usize])
        } else {
            queue.push_back((f, d));
            let dir_str = if d == 0 { "" } else { "'" };
            std::format!("{}{}", names[f as usize], dir_str)
        }
    } else {
        "invalid".to_string()
    }
}

pub fn load_move_delay(nvs: &EspDefaultNvsPartition) -> u64 {
    let Ok(ns) = EspNvs::new(nvs.clone(), NVS_NS_HTTP, false) else { return DEFAULT_DELAY_MS; };
    let mut buf = [0u8; 8];
    if ns.get_blob(NVS_KEY_DELAY, &mut buf).ok().flatten().is_some() {
        u64::from_le_bytes(buf)
    } else {
        DEFAULT_DELAY_MS
    }
}

fn save_move_delay(nvs: &EspDefaultNvsPartition, delay: u64) -> anyhow::Result<()> {
    let ns = EspNvs::new(nvs.clone(), NVS_NS_HTTP, true)?;
    ns.set_blob(NVS_KEY_DELAY, &delay.to_le_bytes())?;
    Ok(())
}

pub fn start_http_server(
    queue: Arc<Mutex<VecDeque<(u8, u8)>>>,
    wf: SharedWifi,
    nvs: EspDefaultNvsPartition,
    cube_state: Arc<Mutex<protocol::CubeState>>,
    move_delay_ms: Arc<Mutex<u64>>,
) -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&Configuration::default())?;

    server.fn_handler("/", Method::Get, |req| {
        req.into_ok_response()?
            .write_all(INDEX_HTML.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    let queue1 = queue.clone();
    server.fn_handler("/api/move", Method::Post, move |mut req| {
        let mut body = Vec::new();
        let mut buf = [0u8; 64];
        loop {
            match req.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }

        let resp_text = if let Ok(s) = std::str::from_utf8(&body) {
            log::info!("/api/move body ({} bytes): {}", s.len(), s);
            if let Some(name) = parse_json_str(s, "move") {
                if let Some((f, d)) = parse_move_notation(name) {
                    push_move_or_delay(&mut queue1.lock().unwrap(), f, d)
                } else {
                    "bad request".to_string()
                }
            } else {
                let face = parse_json_u8(s, "face");
                let dir = parse_json_u8(s, "dir");
                if let (Some(f), Some(d)) = (face, dir) {
                    if f < 6 && d < 3 {
                        let names = ["U", "R", "F", "D", "L", "B"];
                        if d == 2 {
                            queue1.lock().unwrap().push_back((f, 0));
                            queue1.lock().unwrap().push_back((f, 0));
                            std::format!("{}2", names[f as usize])
                        } else {
                            queue1.lock().unwrap().push_back((f, d));
                            let dir_str = if d == 0 { "" } else { "'" };
                            std::format!("{}{}", names[f as usize], dir_str)
                        }
                    } else {
                        "invalid".to_string()
                    }
                } else {
                    "bad request".to_string()
                }
            }
        } else {
            "bad request".to_string()
        };

        req.into_ok_response()?
            .write_all(resp_text.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    server.fn_handler("/api/moves", Method::Options, move |req| {
        req.into_response(200, Some("OK"), &[
            ("Access-Control-Allow-Origin", "*"),
            ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
            ("Access-Control-Allow-Headers", "Content-Type"),
        ])?;
        Ok::<(), anyhow::Error>(())
    })?;

    let queue2 = queue.clone();
    server.fn_handler("/api/moves", Method::Post, move |mut req| {
        let mut body = Vec::new();
        let mut buf = [0u8; 512];
        loop {
            match req.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => body.extend_from_slice(&buf[..n]),
                Err(_) => {
                    log::warn!("/api/moves read error after {} bytes", body.len());
                    break;
                }
            }
        }
        log::info!("/api/moves body {} bytes", body.len());

        let resp_text = if let Ok(s) = std::str::from_utf8(&body) {
            let mut results = Vec::new();
            let mut pos = 0;
            while let Some(start) = s[pos..].find("{") {
                let abs_start = pos + start;
                if let Some(end) = s[abs_start..].find("}") {
                    let obj = &s[abs_start..abs_start + end + 1];
                    let face = parse_json_u8(obj, "face");
                    let dir_val = parse_json_u8(obj, "dir");
                    let delay = parse_json_u8(obj, "delay");
                    let move_name = parse_json_str(obj, "move");

                    if let (Some(f), Some(d)) = (face, dir_val) {
                        if f < 6 && d < 3 {
                            let names = ["U", "R", "F", "D", "L", "B"];
                            if d == 2 {
                                queue2.lock().unwrap().push_back((f, 0));
                                queue2.lock().unwrap().push_back((f, 0));
                                results.push(std::format!("{}2", names[f as usize]));
                            } else {
                                queue2.lock().unwrap().push_back((f, d));
                                let dir_str = if d == 0 { "" } else { "'" };
                                results.push(std::format!("{}{}", names[f as usize], dir_str));
                            }
                        } else {
                            results.push("invalid".to_string());
                        }
                    } else if let Some(ms) = delay {
                        if ms > 0 {
                            queue2.lock().unwrap().push_back((MOVE_FACE_DELAY, ms));
                            results.push(std::format!("delay={}", ms));
                        } else {
                            results.push("bad request".to_string());
                        }
                    } else if let Some(name) = move_name {
                        if let Some((f, d)) = parse_move_notation(name) {
                            if f < 6 && d < 3 {
                                let names = ["U", "R", "F", "D", "L", "B"];
                                if d == 2 {
                                    queue2.lock().unwrap().push_back((f, 0));
                                    queue2.lock().unwrap().push_back((f, 0));
                                    results.push(std::format!("{}2", names[f as usize]));
                                } else {
                                    queue2.lock().unwrap().push_back((f, d));
                                    let dir_str = if d == 0 { "" } else { "'" };
                                    results.push(std::format!("{}{}", names[f as usize], dir_str));
                                }
                            } else {
                                results.push("invalid".to_string());
                            }
                        } else {
                            results.push("bad request".to_string());
                        }
                    } else {
                        results.push("bad request".to_string());
                    }
                    pos = abs_start + end + 1;
                } else {
                    break;
                }
            }
            if results.is_empty() {
                "bad request".to_string()
            } else {
                results.join(",")
            }
        } else {
            "bad request".to_string()
        };

        log::info!("/api/moves resp: {}", resp_text);

        req.into_response(200, Some("OK"), &[("Access-Control-Allow-Origin", "*")])?
            .write_all(resp_text.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    let wf1 = wf.clone();
    let nvs1 = nvs.clone();
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

        req.into_ok_response()?
            .write_all(resp_text.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    let wf_ap_off = wf.clone();
    let nvs_ap_off = nvs.clone();
    server.fn_handler("/api/wifi/ap/off", Method::Post, move |req| {
        match wifi::disable_ap(&wf_ap_off, &nvs_ap_off) {
            Ok(()) => {
                req.into_ok_response()?
                    .write_all(b"AP disabled")?;
            }
            Err(e) => {
                req.into_response(500, Some("Error"), &[])?
                    .write_all(format!("error:{}", e).as_bytes())?;
            }
        }
        Ok::<(), anyhow::Error>(())
    })?;

    let wf_status = wf.clone();
    server.fn_handler("/api/wifi/status", Method::Get, move |req| {
        let (mode, ap_active, sta_connected, ip) = wifi::get_wifi_status(&wf_status);
        let json = std::format!(
            "{{\"mode\":\"{}\",\"ap_active\":{},\"sta_connected\":{},\"ip\":\"{}\"}}",
            mode,
            ap_active,
            sta_connected,
            ip.unwrap_or_default()
        );
        req.into_ok_response()?
            .write_all(json.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    let cs_ref = cube_state.clone();
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
        req.into_ok_response()?
            .write_all(json.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    let cs_ref2 = cube_state.clone();
    server.fn_handler("/api/facelets", Method::Get, move |req| {
        let cs = cs_ref2.lock().unwrap();
        let f = protocol::to_facelet_string(&cs);
        drop(cs);
        let s = std::str::from_utf8(&f).unwrap_or("UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB");
        req.into_response(200, Some("OK"), &[("Access-Control-Allow-Origin", "*")])?
            .write_all(s.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    let cs_ref3 = cube_state.clone();
    server.fn_handler("/api/reset", Method::Post, move |req| {
        let mut cs = cs_ref3.lock().unwrap();
        *cs = protocol::CubeState::solved();
        drop(cs);
        req.into_ok_response()?
            .write_all(b"reset")?;
        Ok::<(), anyhow::Error>(())
    })?;

    let delay_ref = move_delay_ms.clone();
    server.fn_handler("/api/delay", Method::Get, move |req| {
        let d = *delay_ref.lock().unwrap();
        req.into_ok_response()?
            .write_all(std::format!("{{\"delay\":{}}}", d).as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    let delay_ref2 = move_delay_ms.clone();
    let nvs_delay = nvs.clone();
    server.fn_handler("/api/delay", Method::Post, move |mut req| {
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
            if let Some(d) = parse_json_u64(s, "delay") {
                *delay_ref2.lock().unwrap() = d;
                let _ = save_move_delay(&nvs_delay, d);
                std::format!("{{\"delay\":{}}}", d)
            } else {
                "bad request".to_string()
            }
        } else {
            "bad request".to_string()
        };
        req.into_ok_response()?
            .write_all(resp.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(server)
}

fn parse_json_str<'a>(s: &'a str, key: &str) -> Option<&'a str> {
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

fn parse_json_u8(s: &str, key: &str) -> Option<u8> {
    let search = std::format!("\"{}\"", key);
    let pos = s.find(&search)?;
    let after = &s[pos + search.len()..];
    let after = after.trim_start_matches(|c: char| c == ':' || c == ' ');
    let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
    after[..end].parse().ok()
}

fn parse_json_u64(s: &str, key: &str) -> Option<u64> {
    let search = std::format!("\"{}\"", key);
    let pos = s.find(&search)?;
    let after = &s[pos + search.len()..];
    let after = after.trim_start_matches(|c: char| c == ':' || c == ' ');
    let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
    after[..end].parse().ok()
}
