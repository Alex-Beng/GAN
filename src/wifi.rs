use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration,
    EspWifi,
};

use crate::device_config::{WIFI_AP_PASS, WIFI_AP_SSID};

pub type SharedWifi = Arc<Mutex<BlockingWifi<EspWifi<'static>>>>;

const NVS_KEY_SSID: &str = "sta_ssid";
const NVS_KEY_PASS: &str = "sta_pass";
const NVS_NS: &str = "wifi_cfg";
const STA_MAX_RETRIES: u32 = 3;
const STA_RETRY_DELAY_MS: u64 = 2000;

pub struct SavedStaConfig {
    pub ssid: String,
    pub pass: String,
}

fn open_nvs(nvs: &EspDefaultNvsPartition, read_write: bool) -> Option<EspNvs<NvsDefault>> {
    EspNvs::new(nvs.clone(), NVS_NS, read_write)
        .map_err(|e| log::warn!("NVS open error: {:?}", e))
        .ok()
}

pub fn load_sta_config(nvs: &EspDefaultNvsPartition) -> Option<SavedStaConfig> {
    let ns = open_nvs(nvs, false)?;

    let ssid_len = ns.blob_len(NVS_KEY_SSID).ok()??;
    let mut ssid_buf = vec![0u8; ssid_len];
    ns.get_blob(NVS_KEY_SSID, &mut ssid_buf).ok()?;
    let ssid = String::from_utf8_lossy(&ssid_buf).to_string();
    if ssid.is_empty() {
        return None;
    }

    let pass = if let Ok(Some(pass_len)) = ns.blob_len(NVS_KEY_PASS) {
        let mut pass_buf = vec![0u8; pass_len];
        ns.get_blob(NVS_KEY_PASS, &mut pass_buf).ok()?;
        String::from_utf8_lossy(&pass_buf).to_string()
    } else {
        String::new()
    };

    Some(SavedStaConfig { ssid, pass })
}

pub fn save_sta_config(nvs: &EspDefaultNvsPartition, ssid: &str, pass: &str) -> anyhow::Result<()> {
    let ns = EspNvs::new(nvs.clone(), NVS_NS, true)?;
    ns.set_blob(NVS_KEY_SSID, ssid.as_bytes())?;
    ns.set_blob(NVS_KEY_PASS, pass.as_bytes())?;
    log::info!("Saved STA config: SSID={}", ssid);
    Ok(())
}

pub fn start_wifi(
    modem: Modem,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<SharedWifi> {
    let modem: Modem<'static> = unsafe { core::mem::transmute(modem) };
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sys_loop.clone(), Some(nvs.clone()))?,
        sys_loop,
    )?;

    let saved_cfg = load_sta_config(&nvs);
    if let Some(ref cfg) = saved_cfg {
        log::info!("Found saved STA config: SSID={}", cfg.ssid);
        start_mixed_with_fallback(&mut wifi, &cfg.ssid, &cfg.pass)?;
    } else {
        log::info!("No saved STA config, starting AP only");
        start_ap(&mut wifi)?;
    }

    Ok(Arc::new(Mutex::new(wifi)))
}

fn make_auth(pass: &str) -> AuthMethod {
    if pass.is_empty() {
        AuthMethod::None
    } else {
        AuthMethod::WPA2Personal
    }
}

fn ap_config() -> AccessPointConfiguration {
    AccessPointConfiguration {
        ssid: WIFI_AP_SSID.try_into().unwrap(),
        password: WIFI_AP_PASS.try_into().unwrap(),
        auth_method: make_auth(WIFI_AP_PASS),
        channel: 1,
        max_connections: 10,
        ..Default::default()
    }
}

fn sta_config(ssid: &str, pass: &str) -> ClientConfiguration {
    ClientConfiguration {
        ssid: ssid.try_into().unwrap(),
        password: pass.try_into().unwrap(),
        auth_method: make_auth(pass),
        ..Default::default()
    }
}

fn wait_netif_up_poll(up_fn: impl Fn() -> Result<bool, esp_idf_svc::sys::EspError>, timeout_ms: u64) -> bool {
    let step = 200u64;
    let mut elapsed = 0u64;
    while elapsed < timeout_ms {
        if up_fn().unwrap_or(false) {
            return true;
        }
        thread::sleep(Duration::from_millis(step));
        elapsed += step;
    }
    false
}

fn start_ap(wifi: &mut BlockingWifi<EspWifi>) -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::AccessPoint(ap_config()))?;
    wifi.start()?;
    wait_netif_up_poll(|| wifi.wifi().ap_netif().is_netif_up(), 4000);
    log::info!("WiFi AP started: SSID={}", WIFI_AP_SSID);
    Ok(())
}

fn start_mixed(wifi: &mut BlockingWifi<EspWifi>, ssid: &str, pass: &str) -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::Mixed(sta_config(ssid, pass), ap_config()))?;
    wifi.start()?;

    wait_netif_up_poll(|| wifi.wifi().ap_netif().is_netif_up(), 4000);
    log::info!("WiFi Mixed mode started, AP SSID={}", WIFI_AP_SSID);
    thread::sleep(Duration::from_millis(2000));

    for attempt in 1..=STA_MAX_RETRIES {
        log::info!("WiFi STA connecting... attempt {}/{} to {}", attempt, STA_MAX_RETRIES, ssid);
        if let Err(e) = wifi.connect() {
            log::warn!("connect() failed on attempt {}: {:?}", attempt, e);
            let _ = wifi.disconnect();
            if attempt < STA_MAX_RETRIES {
                thread::sleep(Duration::from_millis(STA_RETRY_DELAY_MS));
                continue;
            }
            log::warn!("STA failed after all attempts, AP still running");
            return Ok(());
        }

        let up = wait_netif_up_poll(|| wifi.wifi().sta_netif().is_netif_up(), 6000);
        if up {
            let ip = wifi.wifi().sta_netif().get_ip_info().map(|i| i.ip);
            log::info!("WiFi STA connected: SSID={}, IP={:?}", ssid, ip);
            return Ok(());
        }

        log::warn!("STA netif not up on attempt {}", attempt);
        let _ = wifi.disconnect();
        if attempt < STA_MAX_RETRIES {
            thread::sleep(Duration::from_millis(STA_RETRY_DELAY_MS));
        }
    }

    log::warn!("STA failed after all attempts, AP still running");
    Ok(())
}

fn start_mixed_with_fallback(wifi: &mut BlockingWifi<EspWifi>, ssid: &str, pass: &str) -> anyhow::Result<()> {
    match start_mixed(wifi, ssid, pass) {
        Ok(()) => Ok(()),
        Err(e) => {
            log::warn!("Mixed mode start failed: {:?}, falling back to AP only", e);
            if let Err(e2) = wifi.stop() {
                log::warn!("WiFi stop error: {:?}", e2);
            }
            start_ap(wifi)
        }
    }
}

pub fn get_wifi_status(wifi: &SharedWifi) -> (bool, bool, Option<String>) {
    let w = wifi.lock().unwrap();
    let cfg = w.get_configuration();
    let ap_active = matches!(cfg, Ok(Configuration::AccessPoint(_)) | Ok(Configuration::Mixed(_, _)));
    let sta_connected = w.wifi().is_connected().unwrap_or(false);
    let ip = if sta_connected {
        w.wifi().sta_netif().get_ip_info().ok().map(|info| format!("{:?}", info.ip))
    } else {
        None
    };
    (ap_active, sta_connected, ip)
}

pub fn sta_reconnect_loop(wifi: SharedWifi, nvs: EspDefaultNvsPartition) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(10));

            let is_mixed = {
                let w = wifi.lock().unwrap();
                matches!(w.get_configuration(), Ok(Configuration::Mixed(_, _)))
            };
            if !is_mixed {
                continue;
            }

            let already_connected = {
                let w = wifi.lock().unwrap();
                w.wifi().is_connected().unwrap_or(false)
            };
            if already_connected {
                continue;
            }

            let Some(cfg) = load_sta_config(&nvs) else { continue };

            log::info!("STA disconnected, attempting reconnect to {}", cfg.ssid);
            let mut w = wifi.lock().unwrap();
            if let Err(e) = w.connect() {
                log::warn!("STA reconnect failed: {:?}", e);
                let _ = w.disconnect();
            } else {
                log::info!("STA reconnected to {}", cfg.ssid);
            }
        }
    });
}

pub fn switch_mode(wifi: &SharedWifi, nvs: &EspDefaultNvsPartition, sta: bool, ssid: &str, pass: &str) -> anyhow::Result<String> {
    let mut w = wifi.lock().unwrap();

    if let Err(e) = w.stop() {
        log::warn!("WiFi stop error: {:?}", e);
    }

    if sta && !ssid.is_empty() {
        let _ = save_sta_config(nvs, ssid, pass);
        start_mixed(&mut w, ssid, pass)?;
        let ip = w.wifi().sta_netif().get_ip_info();
        match ip {
            Ok(ip) => Ok(format!("Mixed:AP+STA:{}:{:?}", ssid, ip)),
            Err(_) => Ok(format!("Mixed:AP only (STA {} failed)", ssid)),
        }
    } else {
        start_ap(&mut w)?;
        Ok(format!("AP:{}", WIFI_AP_SSID))
    }
}
