use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration,
    EspWifi,
};

const STA_MAX_RETRIES: u32 = 3;
const STA_RETRY_DELAY_MS: u64 = 2000;

use crate::device_config::{WIFI_AP_PASS, WIFI_AP_SSID, WIFI_MODE_AP, WIFI_STA_PASS, WIFI_STA_SSID};

pub type SharedWifi = Arc<Mutex<BlockingWifi<EspWifi<'static>>>>;

pub fn start_wifi(
    modem: Modem,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> anyhow::Result<SharedWifi> {
    let modem: Modem<'static> = unsafe { core::mem::transmute(modem) };
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sys_loop.clone(), Some(nvs))?,
        sys_loop,
    )?;

    if WIFI_MODE_AP {
        start_ap(&mut wifi)?;
    } else {
        if let Err(e) = start_sta(&mut wifi, WIFI_STA_SSID, WIFI_STA_PASS) {
            log::warn!("STA connect failed: {:?}, falling back to AP", e);
            if let Err(e2) = wifi.stop() {
                log::warn!("WiFi stop error: {:?}", e2);
            }
            start_ap(&mut wifi)?;
        }
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

fn start_ap(wifi: &mut BlockingWifi<EspWifi>) -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::AccessPoint(AccessPointConfiguration {
        ssid: WIFI_AP_SSID.try_into().unwrap(),
        password: WIFI_AP_PASS.try_into().unwrap(),
        auth_method: make_auth(WIFI_AP_PASS),
        channel: 1,
        max_connections: 10,
        ..Default::default()
    }))?;
    wifi.start()?;
    wifi.wait_netif_up()?;
    log::info!("WiFi AP started: SSID={}", WIFI_AP_SSID);
    Ok(())
}

fn start_sta(wifi: &mut BlockingWifi<EspWifi>, ssid: &str, pass: &str) -> anyhow::Result<()> {
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: ssid.try_into().unwrap(),
        password: pass.try_into().unwrap(),
        auth_method: make_auth(pass),
        ..Default::default()
    }))?;
    wifi.start()?;

    for attempt in 1..=STA_MAX_RETRIES {
        log::info!("WiFi STA connecting... attempt {}/{}", attempt, STA_MAX_RETRIES);
        if let Err(e) = wifi.connect() {
            log::warn!("connect() failed on attempt {}: {:?}", attempt, e);
            let _ = wifi.disconnect();
            if attempt < STA_MAX_RETRIES {
                thread::sleep(Duration::from_millis(STA_RETRY_DELAY_MS));
                continue;
            }
            return Err(e.into());
        }

        if let Err(e) = wifi.wait_netif_up() {
            log::warn!("wait_netif_up() failed on attempt {}: {:?}", attempt, e);
            let _ = wifi.disconnect();
            if attempt < STA_MAX_RETRIES {
                thread::sleep(Duration::from_millis(STA_RETRY_DELAY_MS));
                continue;
            }
            return Err(e.into());
        }

        let ip = wifi.wifi().sta_netif().get_ip_info().map(|info| info.ip)?;
        log::info!("WiFi STA connected: SSID={}, IP={}", ssid, ip);
        return Ok(());
    }

    anyhow::bail!("WiFi STA failed after {} attempts", STA_MAX_RETRIES)
}

pub fn switch_mode(wifi: &SharedWifi, is_ap: bool, ssid: &str, pass: &str) -> anyhow::Result<String> {
    let mut w = wifi.lock().unwrap();
    if let Err(e) = w.stop() {
        log::warn!("WiFi stop error: {:?}", e);
    }

    let result = if is_ap {
        start_ap(&mut w).map(|_| format!("AP:{}", WIFI_AP_SSID))
    } else {
        Ok(match start_sta(&mut w, ssid, pass) {
            Ok(()) => {
                let ip = w.wifi().sta_netif().get_ip_info().map(|info| info.ip);
                match ip {
                    Ok(ip) => format!("STA:{}:{}", ssid, ip),
                    Err(_) => format!("STA:{}:?", ssid),
                }
            }
            Err(e) => {
                log::warn!("STA connect failed: {:?}, falling back to AP", e);
                if let Err(e2) = w.stop() {
                    log::warn!("WiFi stop error: {:?}", e2);
                }
                start_ap(&mut w)?;
                "AP:fallback".to_string()
            }
        })
    };

    result
}
