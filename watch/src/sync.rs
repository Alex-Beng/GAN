//! Persistence + upload loop.
//!
//! One worker thread:
//!   1. drains `AppState.completed` into LittleFS (never blocks the BLE thread),
//!   2. pushes queued files to `timer/time/practice/batch/report` whenever the
//!      station is online and a `user_id` is known,
//!   3. deletes a file only after the gateway answers `code == 200`.

use std::thread;
use std::time::Duration;

use esp_idf_svc::nvs::EspDefaultNvsPartition;
use shared::api::timer::{parse_aggregate, PATH_BATCH_REPORT};
use shared::api::{ApiClient, ClientConfig};
use shared::wifi::SharedWifi;

use crate::config::Settings;
use crate::state::SharedState;
use crate::store::Store;

/// Records per upload request. Keeps the JSON body small (a few KB) so it fits
/// in the TLS tx buffer on constrained builds.
const BATCH_LIMIT: usize = 3;

pub struct SyncCtx {
    pub app: SharedState,
    pub wifi: SharedWifi,
    pub store: Store,
    pub nvs: EspDefaultNvsPartition,
    /// Set by `POST /api/watch/upload` to request an immediate attempt.
    pub force_upload: std::sync::Arc<std::sync::Mutex<bool>>,
}

pub fn spawn(ctx: SyncCtx) {
    thread::Builder::new()
        .name("sync".into())
        .stack_size(8192)
        .spawn(move || run(ctx))
        .expect("spawn sync thread");
}

fn run(ctx: SyncCtx) {
    let mut settings = crate::config::load(&ctx.nvs);
    let mut last_settings_poll = 0u64;
    let mut backoff_ms = 2000u64;

    loop {
        let now = shared::api::now_ms();

        // Reload settings periodically: the web UI can change the token/user id.
        if now.saturating_sub(last_settings_poll) > 5000 {
            settings = crate::config::load(&ctx.nvs);
            last_settings_poll = now;
        }

        // 1. Flush finished attempts to disk.
        flush_to_disk(&ctx);

        // 2. Upload when possible.
        let forced = {
            let mut f = ctx.force_upload.lock().unwrap();
            let v = *f;
            *f = false;
            v
        };
        let wifi_up = shared::wifi::get_wifi_status(&ctx.wifi).2;
        let online = wifi_up || forced;
        let queued = ctx.store.files();
        {
            let mut st = ctx.app.lock().unwrap();
            let pending = queued.len();
            let logged_in = settings.user_id != 0 && !settings.token.is_empty();
            let mut changed = false;
            if st.pending_uploads != pending {
                st.pending_uploads = pending;
                changed = true;
            }
            if st.wifi_ok != wifi_up {
                st.wifi_ok = wifi_up;
                changed = true;
            }
            if st.user_id != settings.user_id {
                st.user_id = settings.user_id;
                changed = true;
            }
            if st.logged_in != logged_in {
                st.logged_in = logged_in;
                changed = true;
            }
            if st.nick != settings.nick {
                st.nick = settings.nick.clone();
                changed = true;
            }
            if changed {
                st.bump();
            }
        }

        if online && settings.user_id != 0 && !queued.is_empty() {
            match upload_batch(&ctx, &settings, &queued) {
                Ok(uploaded) => {
                    backoff_ms = 2000;
                    if uploaded > 0 {
                        log::info!("[sync] uploaded {uploaded} record(s)");
                    }
                }
                Err(e) => {
                    log::warn!("[sync] upload failed: {e}");
                    {
                        let mut st = ctx.app.lock().unwrap();
                        st.status_msg = format!("upload failed: {e}");
                        st.bump();
                    }
                    thread::sleep(Duration::from_millis(backoff_ms));
                    backoff_ms = (backoff_ms * 2).min(60_000);
                    continue;
                }
            }
        }

        thread::sleep(Duration::from_millis(1000));
    }
}

fn flush_to_disk(ctx: &SyncCtx) {
    let batch = {
        let mut st = ctx.app.lock().unwrap();
        if st.completed.is_empty() {
            return;
        }
        std::mem::take(&mut st.completed)
    };
    for item in batch {
        if let Err(e) = ctx.store.write(item.key, &item.json) {
            log::error!("[store] write failed: {e}");
        }
    }
}

fn upload_batch(ctx: &SyncCtx, settings: &Settings, queued: &[(u64, std::path::PathBuf)]) -> anyhow::Result<usize> {
    let take = queued.iter().take(BATCH_LIMIT);
    let mut bodies: Vec<(std::path::PathBuf, String)> = Vec::new();
    for (_, path) in take {
        match ctx.store.read(path) {
            Ok(json) => bodies.push((path.clone(), json)),
            Err(e) => log::warn!("[store] skipping unreadable {}: {e}", path.display()),
        }
    }
    if bodies.is_empty() {
        return Ok(0);
    }

    // Build `{"list":[<record>,...]}` straight from the stored JSON so no
    // re-serialization is needed.
    let mut body = String::with_capacity(32 + bodies.iter().map(|(_, j)| j.len()).sum::<usize>());
    body.push_str("{\"list\":[");
    for (i, (_, json)) in bodies.iter().enumerate() {
        if i > 0 {
            body.push(',');
        }
        body.push_str(json);
    }
    body.push_str("]}");

    let mut client = ApiClient::new(ClientConfig {
        base: settings.gateway.clone(),
        device_id: settings.device_id.clone(),
        ..Default::default()
    });
    client.token = Some(settings.token.clone());
    client.user_id = Some(settings.user_id);

    let reply = client.post_json(PATH_BATCH_REPORT, &body)?;
    if !reply.ok() {
        anyhow::bail!("code {:?} {}", reply.code, reply.message.unwrap_or_default());
    }

    for (path, _) in &bodies {
        if let Err(e) = ctx.store.remove(path) {
            log::warn!("[store] could not delete {}: {e}", path.display());
        }
    }

    let mut st = ctx.app.lock().unwrap();
    st.pending_uploads = ctx.store.count();
    if let Some(agg) = parse_aggregate(&reply) {
        st.status_msg = format!(
            "uploaded ok | ao5 {} ao12 {} | total {}",
            agg.recent_ao5, agg.recent_ao12, agg.play_count_valid
        );
    } else {
        st.status_msg = "uploaded ok".to_string();
    }
    st.bump();

    Ok(bodies.len())
}

