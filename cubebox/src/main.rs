//! `gan-cubebox` 鈥?standalone smart-cube timer / training recorder.
//!
//! Behaviour:
//!   * BLE Central connects a physical GAN/MoYu/QiYi cube and decodes moves.
//!   * A local session generates a scramble, arms on "scramble applied", times
//!     from the first move and stops when the cube is back to the start state.
//!   * Every finished attempt is written to LittleFS immediately.
//!   * When WiFi is up and an account is configured, queued attempts are pushed
//!     to `timer/time/practice/batch/report` and deleted on success.
//!   * A 240x320 ST7789 shows scramble / timer / records / status, driven by
//!     four buttons; provisioning and login are also available over HTTP.
//!
//! Build from this directory (its .cargo/config.toml selects ESP32-S3):
//!   cd cubebox && cargo build

#[cfg(feature = "ble")]
mod ble_central;
#[cfg(feature = "ble")]
mod room_ble;
mod board;
mod config;
mod display;
mod input;
mod state;
mod store;
mod sync;
mod ui;
mod web;

use std::sync::{Arc, Mutex};
use std::thread;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyInputPin, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config as SpiConfig, DriverConfig, MODE_0};
use esp_idf_svc::hal::spi::SpiDeviceDriver;
use esp_idf_svc::hal::units::Hertz;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
#[cfg(feature = "ble")]
use esp32_nimble::BLEDevice;

use shared::config_http::{self, ConfigHttpDeps};
use shared::device_config::Protocol;
#[cfg(feature = "ble")]
use shared::protocol::CubeEvent;
use shared::protocol::CubeState;

use crate::display::{Canvas, Display};
use crate::state::{new_shared, SharedState};
use crate::sync::SyncCtx;
use crate::ui::Ui;

const STORE_MOUNT: &str = "/storage";

fn main() {
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    // Dynamic frequency scaling, same as the other firmwares.
    unsafe {
        esp_idf_svc::sys::esp_pm_configure(&esp_idf_svc::sys::esp_pm_config_t {
            max_freq_mhz: 160,
            min_freq_mhz: 40,
            light_sleep_enable: false,
        } as *const _ as *mut _);
    }

    log::info!("=== gan-cubebox === booting");

    let peripherals = Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();

    // --- settings -----------------------------------------------------------
    let mut settings = config::load(&nvs);
    if let Err(e) = config::ensure_device_id(&nvs, &mut settings) {
        log::warn!("could not persist device id: {e}");
    }
    log::info!(
        "account: user={} nick='{}' gateway={}",
        settings.user_id,
        settings.nick,
        settings.gateway
    );

    // --- shared state -------------------------------------------------------
    let app = new_shared();
    {
        let mut st = app.lock().unwrap();
        st.user_id = settings.user_id;
        st.logged_in = settings.user_id != 0 && !settings.token.is_empty();
        st.nick = settings.nick.clone();
        st.cube_type = match settings.cube_type {
            2 => shared::timer::CubeType::Two,
            _ => shared::timer::CubeType::Three,
        };
        st.group_type = match settings.group_type {
            1 => shared::timer::GroupType::Basic,
            2 => shared::timer::GroupType::Special,
            _ => shared::timer::GroupType::Free,
        };
        st.group_id = settings.group_id;
        st.cube_smart = settings.cube_smart;
        st.bump();
    }

    // --- WiFi ---------------------------------------------------------------
    let wifi = shared::wifi::start_wifi(peripherals.modem, sys_loop.clone(), nvs.clone())
        .expect("wifi init");
    shared::wifi::sta_reconnect_loop(wifi.clone(), nvs.clone());

    // Display/buttons own SPI2 and the GPIOs; grab them before moving anything
    // else out of `peripherals`.
    let spi2 = peripherals.spi2;
    let pins = peripherals.pins;

    // --- SNTP (best effort; only used for human-facing timestamps) ----------
    let _sntp = match esp_idf_svc::sntp::EspSntp::new_default() {
        Ok(s) => Some(s),
        Err(e) => {
            log::warn!("sntp init failed: {e:?}");
            None
        }
    };

    // --- record store -------------------------------------------------------
    let store_mount = mount_store();
    let store = match &store_mount {
        Ok(_) => store::Store::new(STORE_MOUNT),
        Err(e) => {
            log::error!("LittleFS unavailable ({e}); records stay in RAM only");
            store::Store::new("/nonexistent")
        }
    };

    // --- shared config HTTP (WiFi provisioning + /api/state) ----------------
    let live: Arc<Mutex<CubeState>> = Arc::new(Mutex::new(CubeState::solved()));
    let server = config_http::start(ConfigHttpDeps {
        wifi: wifi.clone(),
        nvs: nvs.clone(),
        cube_state: live.clone(),
        protocol: Arc::new(Mutex::new(Protocol::Gen4)),
        on_protocol_change: None,
    })
    .expect("http server");

    let force_upload = Arc::new(Mutex::new(false));
    let mut server = server;
    if let Err(e) = web::register(
        &mut server,
        web::Deps {
            app: app.clone(),
            nvs: nvs.clone(),
            force_upload: force_upload.clone(),
        },
    ) {
        log::error!("cubebox http endpoints unavailable: {e}");
    }

    // --- storage + upload worker -------------------------------------------
    sync::spawn(SyncCtx {
        app: app.clone(),
        wifi: wifi.clone(),
        store,
        nvs: nvs.clone(),
        force_upload,
    });

    // --- BLE central --------------------------------------------------------
    #[cfg(feature = "ble")]
    {
        let app = app.clone();
        let live = live.clone();
        thread::Builder::new()
            .name("ble".into())
            .stack_size(32768)
            .spawn(move || {
                // One owner of `BLEDevice`, shared by reference by the cube link
                // and the face-to-face room task.
                let ble: &'static BLEDevice = BLEDevice::take();
                let cube = ble_loop(ble, app.clone(), live);
                let room = room_ble::room_task(ble, app);
                esp_idf_svc::hal::task::block_on(embassy_futures::join::join(cube, room));
            })
            .expect("spawn ble thread");
    }

    // --- display + buttons + UI --------------------------------------------
    {
        let app = app.clone();
        thread::Builder::new()
            .name("ui".into())
            .stack_size(24576)
            .spawn(move || run_ui(app, spi2, pins))
            .expect("spawn ui thread");
    }

    // Keep the HTTP server / SNTP / mount alive for the lifetime of the device.
    loop {
        FreeRtos::delay_ms(10_000);
        let _ = &server;
        let _ = &store_mount;
        let _ = &_sntp;
    }
}

fn mount_store(
) -> anyhow::Result<esp_idf_svc::io::vfs::MountedLittlefs<esp_idf_svc::fs::littlefs::Littlefs<()>>> {
    use esp_idf_svc::fs::littlefs::Littlefs;
    use esp_idf_svc::io::vfs::MountedLittlefs;

    let littlefs = unsafe { Littlefs::<()>::new_partition("storage")? };
    let mounted = MountedLittlefs::mount(littlefs, STORE_MOUNT)?;
    if let Ok(info) = mounted.info() {
        log::info!(
            "LittleFS mounted at {}: {}/{} bytes used",
            STORE_MOUNT,
            info.used_bytes,
            info.total_bytes
        );
    }
    Ok(mounted)
}

// ===========================================================================
// BLE
// ===========================================================================

#[cfg(feature = "ble")]
async fn ble_loop(
    ble_device: &'static BLEDevice,
    app: SharedState,
    live: Arc<Mutex<CubeState>>,
) {

    loop {
        let (addr, name) = crate::ble_central::find_cube(ble_device).await;
        log::info!("[ble] connecting to {addr} ('{name}')");
        {
            let mut st = app.lock().unwrap();
            st.ble_name = name.clone();
            st.bump();
        }

        let mut client = ble_device.new_client();
        if let Err(e) = client.connect(&addr).await {
            log::warn!("[ble] connect failed: {e:?}");
            continue;
        }

        let proto = match crate::ble_central::detect_protocol(&mut client).await {
            Some(p) => p,
            None => {
                log::warn!("[ble] unknown cube protocol; disconnecting");
                let _ = client.disconnect();
                continue;
            }
        };
        log::info!("[ble] protocol {}", proto.label());

        if proto != crate::ble_central::InputProto::MoyuMhc {
            if let Err(e) = client.secure_connection().await {
                log::warn!("[ble] secure_connection failed: {e:?} (continuing)");
            }
        }

        let mac = addr.as_be_bytes();

        let sink_app = app.clone();
        let sink_live = live.clone();
        let sink: crate::ble_central::EventSink = Box::new(move |ev: CubeEvent| {
            handle_event(&sink_app, &sink_live, ev);
        });

        if let Err(e) = crate::ble_central::subscribe_and_decode(&mut client, proto, mac, sink).await
        {
            log::warn!("[ble] subscribe failed: {e}");
            let _ = client.disconnect();
            continue;
        }

        {
            let mut st = app.lock().unwrap();
            st.ble_connected = true;
            st.bump();
        }

        // Wait for the link to drop, then rescan.
        while client.connected() {
            embassy_time::Timer::after(embassy_time::Duration::from_secs(1)).await;
        }
        log::info!("[ble] disconnected");
        {
            let mut st = app.lock().unwrap();
            st.ble_connected = false;
            st.bump();
        }
    }
}

#[cfg(feature = "ble")]
fn handle_event(app: &SharedState, live: &Arc<Mutex<CubeState>>, ev: CubeEvent) {
    match ev {
        CubeEvent::Move { moves, .. } => {
            // Newest first; apply oldest-first so the state stays correct.
            for m in moves.iter().rev() {
                if m.face >= 6 {
                    continue;
                }
                let mv = shared::cube::Move::new(m.face, if m.dir == 0 { 1 } else { 3 });
                {
                    let mut l = live.lock().unwrap();
                    mv.apply(&mut l);
                }
                let mut st = app.lock().unwrap();
                st.on_move(mv);
            }
        }
        CubeEvent::Facelets { facelets, .. } => {
            let state = shared::protocol::decode_facelets(facelets);
            {
                let mut l = live.lock().unwrap();
                *l = state.clone();
            }
            let mut st = app.lock().unwrap();
            st.on_cube_state(&state);
        }
        CubeEvent::Battery { level, .. } => {
            let mut st = app.lock().unwrap();
            if st.battery != level {
                st.battery = level;
                st.bump();
            }
        }
        CubeEvent::Gyro { .. } | CubeEvent::Other { .. } => {}
    }
}

// ===========================================================================
// Display / buttons
// ===========================================================================

fn run_ui(
    app: SharedState,
    spi2: esp_idf_svc::hal::spi::SPI2<'static>,
    pins: esp_idf_svc::hal::gpio::Pins,
) {
    let esp_idf_svc::hal::gpio::Pins {
        gpio4,
        gpio5,
        gpio6,
        gpio7,
        gpio8,
        gpio9,
        gpio10,
        gpio11,
        gpio12,
        gpio15,
        ..
    } = pins;

    let spi = match SpiDeviceDriver::new_single(
        spi2,
        gpio12,
        gpio11,
        None::<AnyInputPin<'static>>,
        Some(gpio10),
        &DriverConfig::new(),
        &SpiConfig::new()
            .baudrate(Hertz(board::DISPLAY_SPI_HZ))
            .data_mode(MODE_0),
    ) {
        Ok(s) => s,
        Err(e) => {
            log::error!("[ui] SPI init failed: {e:?}");
            return;
        }
    };

    let dc = match PinDriver::output(gpio9) {
        Ok(p) => p,
        Err(e) => {
            log::error!("[ui] DC pin init failed: {e:?}");
            return;
        }
    };
    let rst = match PinDriver::output(gpio8) {
        Ok(p) => p,
        Err(e) => {
            log::error!("[ui] RST pin init failed: {e:?}");
            return;
        }
    };
    let bl = match PinDriver::output(gpio7) {
        Ok(p) => p,
        Err(e) => {
            log::error!("[ui] BL pin init failed: {e:?}");
            return;
        }
    };

    let mut display = Display::new(
        spi,
        dc,
        rst,
        bl,
        board::DISPLAY_WIDTH,
        board::DISPLAY_HEIGHT,
    );
    if let Err(e) = display.init() {
        log::error!("[ui] display init failed: {e}");
        return;
    }

    let mut canvas = Canvas::new(board::DISPLAY_WIDTH, board::DISPLAY_HEIGHT);
    log::info!("[ui] canvas {} KB", canvas.bytes() / 1024);

    let buttons: [PinDriver<'static, esp_idf_svc::hal::gpio::Input>; 4] = match (|| {
        Ok::<_, esp_idf_svc::sys::EspError>([
            PinDriver::input(gpio4, Pull::Up)?,
            PinDriver::input(gpio5, Pull::Up)?,
            PinDriver::input(gpio6, Pull::Up)?,
            PinDriver::input(gpio15, Pull::Up)?,
        ])
    })() {
        Ok(b) => b,
        Err(e) => {
            log::error!("[ui] button init failed: {e:?}");
            return;
        }
    };
    let mut buttons = input::Buttons::new(buttons);
    let mut ui = Ui::new();

    // Force a first paint.
    {
        let mut st = app.lock().unwrap();
        st.bump();
        ui.render(&mut canvas, &st);
    }
    if let Err(e) = display.flush_all(&canvas) {
        log::error!("[ui] flush failed: {e}");
    }

    loop {
        let now = shared::api::now_ms();
        let events = buttons.poll(now);
        if !events.is_empty() {
            let seed = now ^ shared::api::hw_random();
            let mut st = app.lock().unwrap();
            for ev in events {
                input::apply(&mut st, ev, seed);
            }
        }

        let redraw = {
            let st = app.lock().unwrap();
            ui.render_if_needed(&mut canvas, &st)
        };
        if redraw {
            if let Err(e) = display.flush_all(&canvas) {
                log::error!("[ui] flush failed: {e}");
            }
        }

        FreeRtos::delay_ms(20);
    }
}



