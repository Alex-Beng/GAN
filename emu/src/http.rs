use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs};

use shared::config_http::{self, parse_json_str, parse_json_u8, parse_json_u64, tune_socket_timeouts, ConfigHttpDeps};
use shared::device_config::Protocol;
use shared::protocol;
use shared::wifi::SharedWifi;

const NVS_NS_HTTP: &str = "http_cfg";
const NVS_KEY_DELAY: &str = "move_delay";
const DEFAULT_DELAY_MS: u64 = 50;

/// Shared "queued but not yet played" counter + condition variable. The BLE
/// move loop decrements it as it consumes each queue entry; `/api/moves?wait=1`
/// waits on it until the batch it just enqueued has been fully played.
pub type MoveProgress = Arc<(Mutex<usize>, Condvar)>;

pub const MOVE_FACE_DELAY: u8 = 99;
pub const MOVE_FACE_TIME: u8 = 100;
pub const MOVE_FACE_DISCONNECT: u8 = 101;

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
    if let Some(v) = parse_json_u64(s, "time") {
        if v > 0 {
            let d = ((v + 5) / 10).min(255) as u8;
            return Some((MOVE_FACE_TIME, d));
        }
    }
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
    protocol: Arc<Mutex<Protocol>>,
    move_progress: MoveProgress,
) -> anyhow::Result<EspHttpServer<'static>> {
    // Shared web UI + WiFi + protocol + device/state endpoints.
    let nvs_cb = nvs.clone();
    let on_change: Box<dyn Fn(Protocol) + Send + Sync> = Box::new(move |p| {
        // Persist the protocol so it survives a reboot.
        crate::save_protocol(&nvs_cb, p);
        // Pre-set the BT MAC for the upcoming BLE restart (protocol switch).
        let cfg = shared::device_config::get_config(p);
        unsafe {
            esp_idf_svc::sys::esp_iface_mac_addr_set(
                cfg.mac.as_ptr(),
                esp_idf_svc::sys::esp_mac_type_t_ESP_MAC_BT,
            );
        }
    });
    let mut server = config_http::start(ConfigHttpDeps {
        wifi: wf,
        nvs: nvs.clone(),
        cube_state: cube_state.clone(),
        protocol,
        on_protocol_change: Some(on_change),
    })?;

    // --- emu-specific: move injection ---

    let queue1 = queue.clone();
    server.fn_handler("/api/move", Method::Post, move |mut req| {
        tune_socket_timeouts(req.connection());
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

        let _ = req.into_ok_response()?
            .write_all(resp_text.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    server.fn_handler("/api/moves", Method::Options, move |mut req| {
        tune_socket_timeouts(req.connection());
        req.into_response(200, Some("OK"), &[
            ("Access-Control-Allow-Origin", "*"),
            ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
            ("Access-Control-Allow-Headers", "Content-Type"),
        ])?;
        Ok::<(), anyhow::Error>(())
    })?;

    let queue2 = queue.clone();
    let progress2 = move_progress.clone();
    let delay_ref_moves = move_delay_ms.clone();
    server.fn_handler("/api/moves", Method::Post, move |mut req| {
        tune_socket_timeouts(req.connection());
        // Optional `?wait=1`: block until the whole batch has been played.
        let want_wait = req.uri().contains("wait=1");
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
        log::info!("/api/moves body {} bytes (wait={})", body.len(), want_wait);

        // Hold the progress lock across the whole "record baseline + enqueue +
        // wait" sequence. This is essential: if the lock were released between
        // the last enqueue and entering the wait, the BLE move loop could
        // finish the batch and call notify_all() before we start waiting, and
        // the wakeup would be lost (the request would then only return on the
        // timeout). Condvar::wait_timeout releases the mutex while waiting.
        let mut pending = progress2
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let baseline = *pending;
        let mut enqueued: usize = 0;

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
                    let time_val = parse_json_u64(obj, "time");
                    let move_name = parse_json_str(obj, "move");

                    if let (Some(f), Some(d)) = (face, dir_val) {
                        if f < 6 && d < 3 {
                            let names = ["U", "R", "F", "D", "L", "B"];
                            if d == 2 {
                                queue2.lock().unwrap().push_back((f, 0));
                                queue2.lock().unwrap().push_back((f, 0));
                                enqueued += 2;
                                *pending += 2;
                                results.push(std::format!("{}2", names[f as usize]));
                            } else {
                                queue2.lock().unwrap().push_back((f, d));
                                enqueued += 1;
                                *pending += 1;
                                let dir_str = if d == 0 { "" } else { "'" };
                                results.push(std::format!("{}{}", names[f as usize], dir_str));
                            }
                        } else {
                            results.push("invalid".to_string());
                        }
                    } else if let Some(ms) = time_val {
                        if ms > 0 {
                            let ticks = (ms as u16 + 5) / 10;
                            let d = ticks.min(255) as u8;
                            queue2.lock().unwrap().push_back((MOVE_FACE_TIME, d));
                            enqueued += 1;
                                *pending += 1;
                            results.push(std::format!("time={}", ms));
                        } else {
                            results.push("bad request".to_string());
                        }
                    } else if let Some(ms) = delay {
                        if ms > 0 {
                            queue2.lock().unwrap().push_back((MOVE_FACE_DELAY, ms));
                            enqueued += 1;
                                *pending += 1;
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
                                    enqueued += 2;
                                *pending += 2;
                                    results.push(std::format!("{}2", names[f as usize]));
                                } else {
                                    queue2.lock().unwrap().push_back((f, d));
                                    enqueued += 1;
                                *pending += 1;
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

        // `pending` (the progress lock) has been held since before the batch
        // was enqueued, so no wakeup can be lost. Wait for the BLE move loop to
        // play the whole batch (counter back to `baseline`).
        if enqueued > 0 && want_wait {
            let per_move_ms = (*delay_ref_moves.lock().unwrap()).max(1);
            // Upper bound: batch play time + generous margin.
            let timeout =
                std::time::Duration::from_millis(enqueued as u64 * per_move_ms + 5000);
            let deadline = std::time::Instant::now() + timeout;
            while *pending > baseline {
                let now = std::time::Instant::now();
                if now >= deadline {
                    log::warn!(
                        "/api/moves?wait=1 timed out: pending={} baseline={}",
                        *pending, baseline
                    );
                    break;
                }
                let (guard, _) = progress2
                    .1
                    .wait_timeout(pending, deadline - now)
                    .unwrap_or_else(|e| e.into_inner());
                pending = guard;
            }
        }
        drop(pending);

        log::info!("/api/moves resp: {}", resp_text);

        let _ = req.into_response(200, Some("OK"), &[("Access-Control-Allow-Origin", "*")])?
            .write_all(resp_text.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let cs_ref3 = cube_state.clone();
    let queue_reset = queue.clone();
    server.fn_handler("/api/reset", Method::Post, move |mut req| {
        tune_socket_timeouts(req.connection());
        let mut cs = cs_ref3.lock().unwrap();
        *cs = protocol::CubeState::solved();
        drop(cs);
        // Ask the BLE loop to emit a MoYu32 160 (device-initiated disconnect)
        // notification, mirroring a real cube resetting.
        queue_reset.lock().unwrap().push_back((MOVE_FACE_DISCONNECT, 0));
        let _ = req.into_ok_response()?
            .write_all(b"reset");
        Ok::<(), anyhow::Error>(())
    })?;

    let delay_ref = move_delay_ms.clone();
    server.fn_handler("/api/delay", Method::Get, move |mut req| {
        tune_socket_timeouts(req.connection());
        let d = *delay_ref.lock().unwrap();
        let _ = req.into_ok_response()?
            .write_all(std::format!("{{\"delay\":{}}}", d).as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let delay_ref2 = move_delay_ms.clone();
    let nvs_delay = nvs.clone();
    server.fn_handler("/api/delay", Method::Post, move |mut req| {
        tune_socket_timeouts(req.connection());
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
        let _ = req.into_ok_response()?
            .write_all(resp.as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    let queue3 = queue.clone();
    server.fn_handler("/api/moves/clear", Method::Post, move |mut req| {
        tune_socket_timeouts(req.connection());
        let mut q = queue3.lock().unwrap();
        let count = q.len();
        q.clear();
        drop(q);
        log::info!("/api/moves/clear: cleared {} moves", count);
        let _ = req.into_ok_response()?
            .write_all(std::format!("{{\"cleared\":{}}}", count).as_bytes());
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(server)
}
