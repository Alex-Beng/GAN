use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::protocol;

fn serial_write(data: &[u8]) {
    let total = data.len();
    let mut written = 0usize;
    while written < total {
        unsafe {
            let n = esp_idf_svc::sys::usb_serial_jtag_write_bytes(
                data[written..].as_ptr() as *const _,
                (total - written) as usize,
                100,
            );
            if n > 0 {
                written += n as usize;
            } else {
                break;
            }
        }
    }
}

fn parse_json_u8(s: &str, key: &str) -> Option<u8> {
    let search = format!("\"{}\"", key);
    let pos = s.find(&search)?;
    let after = &s[pos + search.len()..];
    let after = after.trim_start_matches(|c: char| c == ':' || c == ' ');
    let end = after.find(|c: char| !c.is_ascii_digit()).unwrap_or(after.len());
    after[..end].parse().ok()
}

pub fn run_serial(
    queue: Arc<Mutex<VecDeque<(u8, u8)>>>,
    cube_state: Arc<Mutex<protocol::CubeState>>,
) {
    thread::spawn(move || {
        log::info!("Serial task started");
        unsafe {
            let mut cfg = esp_idf_svc::sys::usb_serial_jtag_driver_config_t {
                    tx_buffer_size: 256,
                    rx_buffer_size: 256,
                };
            esp_idf_svc::sys::usb_serial_jtag_driver_install(&mut cfg as *mut _);
        }
        serial_write(b">>> GAN Cube Emu serial ready\n");

        let mut buf = [0u8; 256];
        let mut line: Vec<u8> = Vec::with_capacity(1024);

        loop {
            unsafe {
                let n = esp_idf_svc::sys::usb_serial_jtag_read_bytes(
                    buf.as_mut_ptr() as *mut _,
                    buf.len() as u32,
                    50,
                );
                if n > 0 {
                    for &b in &buf[..n as usize] {
                        if b == b'\r' {
                            continue;
                        }
                        if b == b'\n' {
                            if !line.is_empty() {
                                let cmd = String::from_utf8_lossy(&line);
                                let resp = process_command(cmd.trim(), &queue, &cube_state);
                                serial_write(b">>> ");
                                serial_write(resp.as_bytes());
                                serial_write(b"\n");
                                line.clear();
                            }
                        } else {
                            line.push(b);
                            if line.len() > 1024 {
                                line.clear();
                                serial_write(b"err:line too long\n");
                            }
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
}

fn process_command(
    cmd: &str,
    queue: &Arc<Mutex<VecDeque<(u8, u8)>>>,
    cube_state: &Arc<Mutex<protocol::CubeState>>,
) -> String {
    if cmd.is_empty() {
        return String::new();
    }

    log::info!("Serial cmd: {}", cmd);

    if cmd == "facelets" || (cmd.starts_with('{') && cmd.contains("\"facelets\"")) {
        let cs = cube_state.lock().unwrap();
        let f = protocol::to_facelet_string(&cs);
        String::from_utf8_lossy(&f).to_string()
    } else if cmd.starts_with('[') || cmd.starts_with('{') {
        let mut results = Vec::new();
        let mut pos = 0;
        while let Some(start) = cmd[pos..].find('{') {
            let abs_start = pos + start;
            if let Some(end) = cmd[abs_start..].find('}') {
                let obj = &cmd[abs_start..abs_start + end + 1];
                let face = parse_json_u8(obj, "face");
                let dir = parse_json_u8(obj, "dir");
                if let (Some(f), Some(d)) = (face, dir) {
                    if f < 6 && d < 3 {
                        let names = ["U", "R", "F", "D", "L", "B"];
                        if d == 2 {
                            queue.lock().unwrap().push_back((f, 0));
                            queue.lock().unwrap().push_back((f, 0));
                            results.push(format!("{}2", names[f as usize]));
                        } else {
                            queue.lock().unwrap().push_back((f, d));
                            let dir_str = if d == 0 { "" } else { "'" };
                            results.push(format!("{}{}", names[f as usize], dir_str));
                        }
                    } else {
                        results.push("invalid".to_string());
                    }
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
        "unknown command".to_string()
    }
}
