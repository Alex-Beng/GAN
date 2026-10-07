use std::collections::HashSet;
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

use shared::device_config::{WIFI_AP_PASS, WIFI_AP_SSID};

pub type SharedWifi = Arc<Mutex<BlockingWifi<EspWifi<'static>>>>;

// --- Constants ---
const NVS_NS: &str = "wifi_cfg";
const NVS_KEY_LIST: &str = "sta_list";
const NVS_KEY_SSID_OLD: &str = "sta_ssid";
const NVS_KEY_PASS_OLD: &str = "sta_pass";
const STA_MAX_RETRIES: u32 = 3;
const STA_RETRY_DELAY_MS: u64 = 2000;
const MAX_SAVED_NETWORKS: usize = 10;

// --- Saved Network ---
#[derive(Clone, Debug)]
pub struct SavedStaConfig {
    pub ssid: String,
    pub pass: String,
}

// --- NVS helpers ---
fn open_nvs(nvs: &EspDefaultNvsPartition, read_write: bool) -> Option<EspNvs<NvsDefault>> {
    EspNvs::new(nvs.clone(), NVS_NS, read_write)
        .map_err(|e| log::warn!("NVS open error: {:?}", e))
        .ok()
}

fn serialize_networks(networks: &[SavedStaConfig]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + networks.len() * 104);
    buf.extend_from_slice(&(networks.len() as u32).to_le_bytes());
    for net in networks {
        buf.extend_from_slice(&(net.ssid.len() as u32).to_le_bytes());
        buf.extend_from_slice(net.ssid.as_bytes());
        buf.extend_from_slice(&(net.pass.len() as u32).to_le_bytes());
        buf.extend_from_slice(net.pass.as_bytes());
    }
    buf
}

fn deserialize_networks(data: &[u8]) -> Vec<SavedStaConfig> {
    if data.len() < 4 {
        return Vec::new();
    }
    let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let mut pos = 4;
    let mut result = Vec::with_capacity(count.min(MAX_SAVED_NETWORKS));
    for _ in 0..count {
        if pos + 4 > data.len() {
            break;
        }
        let ssid_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + ssid_len > data.len() {
            break;
        }
        let ssid = String::from_utf8_lossy(&data[pos..pos + ssid_len]).to_string();
        pos += ssid_len;
        if pos + 4 > data.len() {
            break;
        }
        let pass_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if pos + pass_len > data.len() {
            break;
        }
        let pass = String::from_utf8_lossy(&data[pos..pos + pass_len]).to_string();
        pos += pass_len;
        if !ssid.is_empty() {
            result.push(SavedStaConfig { ssid, pass });
        }
    }
    result
}

fn migrate_old_config(nvs: &EspDefaultNvsPartition) {
    let ns = match open_nvs(nvs, false) {
        Some(ns) => ns,
        None => return,
    };
    let has_old = ns.find_key(NVS_KEY_SSID_OLD).ok().flatten().is_some();
    if !has_old {
        return;
    }
    let ssid_len = match ns.blob_len(NVS_KEY_SSID_OLD).ok().flatten() {
        Some(len) => len,
        None => return,
    };
    let mut ssid_buf = vec![0u8; ssid_len];
    if ns.get_blob(NVS_KEY_SSID_OLD, &mut ssid_buf)
        .ok()
        .flatten()
        .is_none()
    {
        return;
    }
    let ssid = String::from_utf8_lossy(&ssid_buf).to_string();
    if ssid.is_empty() {
        return;
    }
    let pass = if let Ok(Some(pass_len)) = ns.blob_len(NVS_KEY_PASS_OLD) {
        let mut pass_buf = vec![0u8; pass_len];
        if ns.get_blob(NVS_KEY_PASS_OLD, &mut pass_buf)
            .ok()
            .flatten()
            .is_some()
        {
            String::from_utf8_lossy(&pass_buf).to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    drop(ns);
    let _ = save_sta_network(nvs, &ssid, &pass);
    if let Ok(ns_w) = EspNvs::new(nvs.clone(), NVS_NS, true) {
        let _ = ns_w.remove(NVS_KEY_SSID_OLD);
        let _ = ns_w.remove(NVS_KEY_PASS_OLD);
    }
    log::info!("Migrated old STA config: SSID={}", ssid);
}

// --- Public NVS functions ---

pub fn load_all_sta_configs(nvs: &EspDefaultNvsPartition) -> Vec<SavedStaConfig> {
    let ns = match open_nvs(nvs, false) {
        Some(ns) => ns,
        None => return Vec::new(),
    };
    let len = match ns.blob_len(NVS_KEY_LIST).ok().flatten() {
        Some(len) => len,
        None => return Vec::new(),
    };
    let mut buf = vec![0u8; len];
    match ns.get_blob(NVS_KEY_LIST, &mut buf).ok().flatten() {
        Some(_) => deserialize_networks(&buf),
        None => Vec::new(),
    }
}

#[allow(dead_code)]
pub fn load_sta_config(nvs: &EspDefaultNvsPartition) -> Option<SavedStaConfig> {
    load_all_sta_configs(nvs).into_iter().last()
}

pub fn save_sta_network(
    nvs: &EspDefaultNvsPartition,
    ssid: &str,
    pass: &str,
) -> anyhow::Result<()> {
    let mut networks = load_all_sta_configs(nvs);
    networks.retain(|n| n.ssid != ssid);
    networks.push(SavedStaConfig {
        ssid: ssid.to_string(),
        pass: pass.to_string(),
    });
    while networks.len() > MAX_SAVED_NETWORKS {
        networks.remove(0);
    }
    let data = serialize_networks(&networks);
    let ns = EspNvs::new(nvs.clone(), NVS_NS, true)?;
    ns.set_blob(NVS_KEY_LIST, &data)?;
    log::info!("Saved STA network: SSID={}", ssid);
    Ok(())
}

pub fn forget_sta_network(nvs: &EspDefaultNvsPartition, ssid: &str) -> anyhow::Result<bool> {
    let mut networks = load_all_sta_configs(nvs);
    let before = networks.len();
    networks.retain(|n| n.ssid != ssid);
    if networks.len() == before {
        return Ok(false);
    }
    let data = serialize_networks(&networks);
    let ns = EspNvs::new(nvs.clone(), NVS_NS, true)?;
    ns.set_blob(NVS_KEY_LIST, &data)?;
    log::info!("Forgot STA network: SSID={}", ssid);
    Ok(true)
}

// --- Hardware helpers ---

pub fn ap_station_count() -> u16 {
    unsafe {
        let mut list = core::mem::zeroed::<esp_idf_svc::sys::wifi_sta_list_t>();
        if esp_idf_svc::sys::esp_wifi_ap_get_sta_list(&mut list as *mut _) == 0 {
            list.num as u16
        } else {
            0
        }
    }
}

// --- WiFi config helpers ---

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

fn wait_netif_up_poll(
    up_fn: impl Fn() -> Result<bool, esp_idf_svc::sys::EspError>,
    timeout_ms: u64,
) -> bool {
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

/// Wait for DHCP to assign a non-zero STA IP; netif-up alone is not enough.
fn wait_sta_ip(wifi: &mut BlockingWifi<EspWifi>, timeout_ms: u64) -> Option<String> {
    let step = 200u64;
    let mut elapsed = 0u64;
    while elapsed < timeout_ms {
        if let Ok(info) = wifi.wifi().sta_netif().get_ip_info() {
            let o = info.ip.octets();
            if o != [0, 0, 0, 0] {
                return Some(format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3]));
            }
        }
        thread::sleep(Duration::from_millis(step));
        elapsed += step;
    }
    None
}

// --- Mode start helpers ---

fn start_ap(wifi: &mut BlockingWifi<EspWifi>) -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::AccessPoint(ap_config()))?;
    wifi.start()?;
    wait_netif_up_poll(|| wifi.wifi().ap_netif().is_netif_up(), 4000);
    log::info!("WiFi AP started: SSID={}", WIFI_AP_SSID);
    Ok(())
}

fn start_sta_only(wifi: &mut BlockingWifi<EspWifi>, ssid: &str, pass: &str) -> bool {
    if wifi
        .set_configuration(&Configuration::Client(sta_config(ssid, pass)))
        .is_err()
    {
        return false;
    }
    if wifi.start().is_err() {
        return false;
    }
    for attempt in 1..=STA_MAX_RETRIES {
        log::info!(
            "STA-only connecting... attempt {}/{} to {}",
            attempt,
            STA_MAX_RETRIES,
            ssid
        );
        if let Err(e) = wifi.connect() {
            log::warn!("connect() failed on attempt {}: {:?}", attempt, e);
            let _ = wifi.disconnect();
            if attempt < STA_MAX_RETRIES {
                thread::sleep(Duration::from_millis(STA_RETRY_DELAY_MS));
            }
            continue;
        }
        let up = wait_netif_up_poll(|| wifi.wifi().sta_netif().is_netif_up(), 10000);
        if up {
            let ip = wait_sta_ip(wifi, 8000).unwrap_or_else(|| "0.0.0.0".to_string());
            log::info!("STA-only connected: SSID={}, IP={}", ssid, ip);
            return true;
        }
        log::warn!("STA netif not up on attempt {}", attempt);
        let _ = wifi.disconnect();
        if attempt < STA_MAX_RETRIES {
            thread::sleep(Duration::from_millis(STA_RETRY_DELAY_MS));
        }
    }
    false
}

fn start_mixed(
    wifi: &mut BlockingWifi<EspWifi>,
    ssid: &str,
    pass: &str,
) -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::Mixed(
        sta_config(ssid, pass),
        ap_config(),
    ))?;
    wifi.start()?;
    wait_netif_up_poll(|| wifi.wifi().ap_netif().is_netif_up(), 4000);
    log::info!("WiFi Mixed mode started, AP SSID={}", WIFI_AP_SSID);
    thread::sleep(Duration::from_millis(2000));
    for attempt in 1..=STA_MAX_RETRIES {
        log::info!(
            "WiFi STA connecting... attempt {}/{} to {}",
            attempt,
            STA_MAX_RETRIES,
            ssid
        );
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
            let ip = wait_sta_ip(wifi, 8000).unwrap_or_else(|| "0.0.0.0".to_string());
            log::info!("WiFi STA connected: SSID={}, IP={}", ssid, ip);
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

// --- Main entry points ---

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

    migrate_old_config(&nvs);
    let saved = load_all_sta_configs(&nvs);

    if saved.is_empty() {
        log::info!("No saved STA config, starting AP only");
        start_ap(&mut wifi)?;
        return Ok(Arc::new(Mutex::new(wifi)));
    }

    // Start WiFi (STA-only) minimally to enable scanning
    log::info!("Scanning for known networks ({} saved)...", saved.len());
    if wifi
        .set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: "".try_into().unwrap(),
            password: "".try_into().unwrap(),
            auth_method: AuthMethod::None,
            ..Default::default()
        }))
        .is_err()
        || wifi.start().is_err()
    {
        log::warn!("Could not start WiFi for scanning, starting AP");
        start_ap(&mut wifi)?;
        return Ok(Arc::new(Mutex::new(wifi)));
    }
    thread::sleep(Duration::from_millis(500));

    let mut scan_results = match wifi.scan() {
        Ok(r) => r,
        Err(e) => {
            log::warn!("WiFi scan failed: {:?}, starting AP", e);
            wifi.stop().ok();
            start_ap(&mut wifi)?;
            return Ok(Arc::new(Mutex::new(wifi)));
        }
    };

    // A single scan can miss in-range networks in weak RF environments;
    // retry once before giving up and falling back to AP.
    if !scan_results
        .iter()
        .any(|ap| saved.iter().any(|n| n.ssid == ap.ssid.as_str()))
    {
        log::warn!("No known network found in scan, retrying scan...");
        wifi.stop().ok();
        thread::sleep(Duration::from_millis(500));
        if wifi
            .set_configuration(&Configuration::Client(ClientConfiguration {
                ssid: "".try_into().unwrap(),
                password: "".try_into().unwrap(),
                auth_method: AuthMethod::None,
                ..Default::default()
            }))
            .is_ok()
            && wifi.start().is_ok()
        {
            thread::sleep(Duration::from_millis(500));
            match wifi.scan() {
                Ok(r) => scan_results = r,
                Err(e) => log::warn!("Retry scan failed: {:?}", e),
            }
        }
    }

    wifi.stop().ok();
    thread::sleep(Duration::from_millis(300));

    // Collect in-range saved SSIDs
    let in_range: HashSet<&str> = scan_results
        .iter()
        .filter(|ap| !ap.ssid.is_empty())
        .map(|ap| ap.ssid.as_str())
        .collect();

    // Try each saved network, most-recent first
    for network in saved.iter().rev() {
        if !in_range.contains(network.ssid.as_str()) {
            log::debug!("Known network {} not in range", network.ssid);
            continue;
        }

        log::info!("Trying known network: {}", network.ssid);
        if start_sta_only(&mut wifi, &network.ssid, &network.pass) {
            log::info!("Connected to known network: {}", network.ssid);
            let _ = save_sta_network(&nvs, &network.ssid, &network.pass);
            return Ok(Arc::new(Mutex::new(wifi)));
        }

        wifi.stop().ok();
        thread::sleep(Duration::from_millis(300));
    }

    log::info!("No known network could be connected, starting AP");
    start_ap(&mut wifi)?;
    Ok(Arc::new(Mutex::new(wifi)))
}

pub fn get_wifi_status(wifi: &SharedWifi) -> (&str, bool, bool, Option<String>) {
    let w = wifi.lock().unwrap();
    let cfg = w.get_configuration();
    let (mode, ap_active) = match cfg {
        Ok(Configuration::Mixed(_, _)) => ("mixed", true),
        Ok(Configuration::AccessPoint(_)) => ("ap", true),
        Ok(Configuration::Client(_)) => ("sta", false),
        _ => ("unknown", false),
    };
    let sta_connected = w.wifi().is_connected().unwrap_or(false);
    let ip = if sta_connected {
        w.wifi()
            .sta_netif()
            .get_ip_info()
            .ok()
            .map(|info| format!("{:?}", info.ip))
    } else {
        None
    };
    (mode, ap_active, sta_connected, ip)
}

pub fn disable_ap(wifi: &SharedWifi, nvs: &EspDefaultNvsPartition) -> anyhow::Result<()> {
    let saved = load_all_sta_configs(nvs);
    if saved.is_empty() {
        return Ok(());
    }
    for network in saved.iter().rev() {
        let mut w = wifi.lock().unwrap();
        w.stop().ok();
        thread::sleep(Duration::from_millis(500));
        if w.set_configuration(&Configuration::Client(sta_config(
            &network.ssid,
            &network.pass,
        )))
        .is_err()
        {
            continue;
        }
        if w.start().is_err() {
            continue;
        }
        thread::sleep(Duration::from_millis(500));
        for attempt in 1..=2 {
            if let Err(e) = w.connect() {
                log::warn!(
                    "disable_ap: connect {} attempt {}: {:?}",
                    network.ssid,
                    attempt,
                    e
                );
                let _ = w.disconnect();
                if attempt < 2 {
                    thread::sleep(Duration::from_millis(1000));
                }
                continue;
            }
            let up = wait_netif_up_poll(|| w.wifi().sta_netif().is_netif_up(), 10000);
            if up {
                let ip = wait_sta_ip(&mut w, 8000).unwrap_or_else(|| "0.0.0.0".to_string());
                log::info!("AP disabled, STA-only connected to {}, IP={}", network.ssid, ip);
                return Ok(());
            }
            let _ = w.disconnect();
        }
        drop(w);
    }
    log::warn!("disable_ap: all networks failed, falling back to AP");
    let mut w = wifi.lock().unwrap();
    w.stop().ok();
    thread::sleep(Duration::from_millis(500));
    start_ap(&mut w)?;
    Ok(())
}

pub fn sta_reconnect_loop(wifi: SharedWifi, nvs: EspDefaultNvsPartition) {
    let wifi_ap = wifi.clone();
    let nvs_ap = nvs.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(60));
        let mut zero_count = 0u8;
        loop {
            thread::sleep(Duration::from_secs(10));
            let is_mixed = {
                let w = wifi_ap.lock().unwrap();
                matches!(w.get_configuration(), Ok(Configuration::Mixed(_, _)))
            };
            if !is_mixed {
                break;
            }
            if ap_station_count() == 0 {
                zero_count += 1;
                if zero_count >= 6 {
                    log::info!("AP idle 60s, disabling AP");
                    let _ = disable_ap(&wifi_ap, &nvs_ap);
                    break;
                }
            } else {
                zero_count = 0;
            }
        }
    });

    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(10));
            let is_sta = {
                let w = wifi.lock().unwrap();
                matches!(
                    w.get_configuration(),
                    Ok(Configuration::Mixed(_, _)) | Ok(Configuration::Client(_))
                )
            };
            if !is_sta {
                continue;
            }
            let already_connected = {
                let w = wifi.lock().unwrap();
                w.wifi().is_connected().unwrap_or(false)
            };
            if already_connected {
                continue;
            }
            let saved = load_all_sta_configs(&nvs);
            if saved.is_empty() {
                continue;
            }
            let mut w = wifi.lock().unwrap();
            let scan_results = match w.scan() {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("reconnect scan failed: {:?}", e);
                    continue;
                }
            };
            let in_range: HashSet<&str> = scan_results
                .iter()
                .filter(|ap| !ap.ssid.is_empty())
                .map(|ap| ap.ssid.as_str())
                .collect();
            for network in saved.iter().rev() {
                if !in_range.contains(network.ssid.as_str()) {
                    continue;
                }
                log::info!("Reconnecting to known network: {}", network.ssid);
                w.stop().ok();
                thread::sleep(Duration::from_millis(500));
                if start_sta_only(&mut w, &network.ssid, &network.pass) {
                    log::info!("Reconnected to {}", network.ssid);
                    let _ = save_sta_network(&nvs, &network.ssid, &network.pass);
                    break;
                }
                w.stop().ok();
                thread::sleep(Duration::from_millis(500));
            }
        }
    });
}

pub fn scan_and_format_json(wifi: &SharedWifi) -> Result<String, String> {
    let mut w = wifi.lock().map_err(|e| format!("lock: {}", e))?;
    let results = w.scan().map_err(|e| format!("scan: {}", e))?;
    let mut json = String::from("[");
    let mut first = true;
    for ap in &results {
        if ap.ssid.is_empty() {
            continue;
        }
        if !first {
            json.push(',');
        }
        first = false;
        let auth = match ap.auth_method {
            Some(AuthMethod::None) => "open",
            Some(AuthMethod::WEP) => "wep",
            Some(AuthMethod::WPA) => "wpa",
            Some(AuthMethod::WPA2Personal) => "wpa2",
            Some(AuthMethod::WPAWPA2Personal) => "wpa_wpa2",
            Some(AuthMethod::WPA2Enterprise) => "wpa2_ent",
            Some(AuthMethod::WPA3Personal) => "wpa3",
            Some(AuthMethod::WPA2WPA3Personal) => "wpa2_wpa3",
            Some(AuthMethod::WAPIPersonal) => "wapi",
            _ => "unknown",
        };
        let escaped: String = ap
            .ssid
            .chars()
            .flat_map(|c| match c {
                '"' => vec!['\\', '"'],
                '\\' => vec!['\\', '\\'],
                '\n' => vec!['\\', 'n'],
                '\r' => vec!['\\', 'r'],
                '\t' => vec!['\\', 't'],
                c if (c as u32) < 0x20 => vec![],
                c => vec![c],
            })
            .collect();
        json.push_str(&format!(
            "{{\"ssid\":\"{}\",\"rssi\":{},\"auth\":\"{}\"}}",
            escaped, ap.signal_strength, auth
        ));
    }
    json.push(']');
    Ok(json)
}

pub fn switch_mode(
    wifi: &SharedWifi,
    nvs: &EspDefaultNvsPartition,
    sta: bool,
    ssid: &str,
    pass: &str,
) -> anyhow::Result<String> {
    let mut w = wifi.lock().unwrap();
    if let Err(e) = w.stop() {
        log::warn!("WiFi stop error: {:?}", e);
    }
    if sta && !ssid.is_empty() {
        let actual_pass = if pass.is_empty() {
            load_all_sta_configs(nvs)
                .into_iter()
                .rev()
                .find(|n| n.ssid == ssid)
                .map(|n| n.pass)
                .unwrap_or_default()
        } else {
            pass.to_string()
        };
        let _ = save_sta_network(nvs, ssid, &actual_pass);
        start_mixed(&mut w, ssid, &actual_pass)?;
        match w.wifi().sta_netif().get_ip_info() {
            Ok(info) => {
                let o = info.ip.octets();
                Ok(format!(
                    "Mixed:AP+STA:{}:{}.{}.{}.{}",
                    ssid, o[0], o[1], o[2], o[3]
                ))
            }
            Err(_) => Ok(format!("Mixed:AP only (STA {} failed)", ssid)),
        }
    } else {
        start_ap(&mut w)?;
        Ok(format!("AP:{}", WIFI_AP_SSID))
    }
}
