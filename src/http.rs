use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use esp_idf_svc::http::server::{Configuration, EspHttpServer};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;

use crate::protocol;
use crate::wifi::{self, SharedWifi};

static INDEX_HTML: &str = include_str!("index.html");

pub fn start_http_server(
    queue: Arc<Mutex<VecDeque<(u8, u8)>>>,
    wf: SharedWifi,
    cube_state: Arc<Mutex<protocol::CubeState>>,
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

        let face;
        let dir;
        if let Ok(s) = std::str::from_utf8(&body) {
            face = parse_json_u8(s, "face");
            dir = parse_json_u8(s, "dir");
        } else {
            face = None;
            dir = None;
        }

        let resp_text = if let (Some(f), Some(d)) = (face, dir) {
            if f < 6 && d < 2 {
                queue1.lock().unwrap().push_back((f, d));
                let names = ["U", "R", "F", "D", "L", "B"];
                let dir_str = if d == 0 { "" } else { "'" };
                std::format!("{}{}", names[f as usize], dir_str)
            } else {
                "invalid".to_string()
            }
        } else {
            "bad request".to_string()
        };

        req.into_ok_response()?
            .write_all(resp_text.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    let queue2 = queue.clone();
    server.fn_handler("/api/moves", Method::Post, move |mut req| {
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
            let mut results = Vec::new();
            let mut pos = 0;
            while let Some(start) = s[pos..].find("{") {
                let abs_start = pos + start;
                if let Some(end) = s[abs_start..].find("}") {
                    let obj = &s[abs_start..abs_start + end + 1];
                    let face = parse_json_u8(obj, "face");
                    let dir = parse_json_u8(obj, "dir");
                    if let (Some(f), Some(d)) = (face, dir) {
                        if f < 6 && d < 2 {
                            queue2.lock().unwrap().push_back((f, d));
                            let names = ["U", "R", "F", "D", "L", "B"];
                            let dir_str = if d == 0 { "" } else { "'" };
                            results.push(std::format!("{}{}", names[f as usize], dir_str));
                        } else {
                            results.push("invalid".to_string());
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

        req.into_ok_response()?
            .write_all(resp_text.as_bytes())?;
        Ok::<(), anyhow::Error>(())
    })?;

    let wf1 = wf.clone();
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
            let is_ap = parse_json_u8(s, "ap").map(|v| v != 0).unwrap_or(true);
            let ssid = parse_json_str(s, "ssid").unwrap_or("");
            let pass = parse_json_str(s, "pass").unwrap_or("");
            match wifi::switch_mode(&wf1, is_ap, ssid, pass) {
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
        req.into_ok_response()?
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
