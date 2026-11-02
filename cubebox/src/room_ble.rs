//! BLE transport for face-to-face rooms.
//!
//! * **Host**: advertises `GANW-<room id>` and runs a GATT server. Room state is
//!   broadcast on the TX characteristic (notify); guests write commands to RX.
//! * **Guest**: scans for the advertising name, connects, subscribes to TX and
//!   writes commands to RX.
//!
//! The GATT database is created **once per boot** (NimBLE cannot re-register
//! services after `start()`); hosting simply starts/stops advertising.
//!
//! The game logic lives in `shared::room::Room`; this module only moves bytes
//! and mirrors the state into `AppState` for the UI.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use embassy_time::{Duration as ETDuration, Timer};
use esp32_nimble::enums::ConnMode;
use esp32_nimble::utilities::mutex::Mutex as NimbleMutex;
use esp32_nimble::utilities::BleUuid;
use esp32_nimble::{
    BLEAdvertisementData, BLECharacteristic, BLEClient, BLEDevice, BLEScan, NimbleProperties,
};

use shared::room::{
    Penalty, PlayerId, ResultEntry, Room, RoomConfig, RoomMsg, RoomPhase, ADV_PREFIX, RX_UUID,
    SERVICE_UUID, TX_UUID,
};

use crate::state::{RoomAction, RoomPlayerView, RoomView, SharedState};

/// Inbound queue filled by BLE callbacks.
struct Inbox {
    q: Mutex<VecDeque<(PlayerId, RoomMsg)>>,
}

impl Inbox {
    fn new() -> Self {
        Self {
            q: Mutex::new(VecDeque::new()),
        }
    }

    fn push(&self, id: PlayerId, msg: RoomMsg) {
        if let Ok(mut q) = self.q.lock() {
            // Never let a misbehaving peer grow this without bound.
            if q.len() < 64 {
                q.push_back((id, msg));
            }
        }
    }

    fn drain(&self) -> Vec<(PlayerId, RoomMsg)> {
        match self.q.lock() {
            Ok(mut q) => q.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }
}

enum Session {
    Host {
        room: Room,
        tx: Arc<NimbleMutex<BLECharacteristic>>,
    },
    Guest {
        room: Room,
        client: BLEClient,
    },
}

impl Session {
    fn room(&self) -> &Room {
        match self {
            Session::Host { room, .. } => room,
            Session::Guest { room, .. } => room,
        }
    }
}

fn uuid16(v: u16) -> BleUuid {
    BleUuid::Uuid16(v)
}

fn local_id(ble: &BLEDevice) -> PlayerId {
    match ble.get_addr() {
        Ok(addr) => PlayerId(addr.as_le_bytes()),
        Err(_) => PlayerId([1, 2, 3, 4, 5, 6]),
    }
}

fn short_name(id: &PlayerId) -> String {
    let hex = id.hex();
    format!("P{}", &hex[hex.len().saturating_sub(4)..])
}

/// Create the room GATT service once. Safe to call once per boot.
fn setup_transport(
    ble: &'static BLEDevice,
    inbox: Arc<Inbox>,
) -> Option<Arc<NimbleMutex<BLECharacteristic>>> {
    let server = ble.get_server();
    server.advertise_on_disconnect(true);

    let inbox_disc = inbox.clone();
    server.on_disconnect(move |desc, _reason| {
        let id = PlayerId(desc.address().as_le_bytes());
        log::info!("[room] guest {} disconnected", id.hex());
        inbox_disc.push(id, RoomMsg::Leave { id });
    });

    let service = server.create_service(uuid16(SERVICE_UUID));
    let tx = service
        .lock()
        .create_characteristic(uuid16(TX_UUID), NimbleProperties::NOTIFY);
    let rx = service.lock().create_characteristic(
        uuid16(RX_UUID),
        NimbleProperties::WRITE | NimbleProperties::WRITE_NO_RSP,
    );

    let inbox_rx = inbox;
    rx.lock().on_write(move |args| {
        let Some(msg) = RoomMsg::decode(args.recv_data()) else {
            return;
        };
        let id = PlayerId(args.desc().address().as_le_bytes());
        inbox_rx.push(id, msg);
    });

    match server.start() {
        Ok(()) => Some(tx),
        Err(e) => {
            log::error!("[room] gatt server start failed: {e:?}");
            None
        }
    }
}

fn advertise(ble: &BLEDevice, room_id: u32) -> bool {
    let adv_name = format!("{ADV_PREFIX}{room_id:08X}");
    let mut adv_data = BLEAdvertisementData::new();
    adv_data
        .name(&adv_name)
        .add_service_uuid(uuid16(SERVICE_UUID));
    let mut adv = ble.get_advertising().lock();
    let _ = adv.advertisement_type(ConnMode::Und);
    if adv.set_data(&mut adv_data).is_err() || adv.start().is_err() {
        return false;
    }
    log::info!("[room] advertising '{adv_name}'");
    true
}

fn stop_advertising(ble: &BLEDevice) {
    let _ = ble.get_advertising().lock().stop();
}

/// Guest: find and connect to the first advertised room.
async fn join_guest(
    ble: &'static BLEDevice,
    inbox: Arc<Inbox>,
    my_id: PlayerId,
    my_name: &str,
) -> Option<(Room, BLEClient, u32)> {
    let found = {
        let mut scan = BLEScan::new();
        scan.active_scan(true)
            .interval(80)
            .window(80)
            .filter_duplicates(true);
        scan.start(ble, 4000, |dev, data| {
            let name = data
                .name()
                .and_then(|n| core::str::from_utf8(n.as_ref()).ok())
                .unwrap_or("");
            let hex = name.strip_prefix(ADV_PREFIX)?;
            if hex.len() != 8 {
                return None;
            }
            let room_id = u32::from_str_radix(hex, 16).ok()?;
            Some((dev.addr(), room_id))
        })
        .await
        .ok()
        .flatten()
    };
    let (addr, room_id) = found?;
    log::info!("[room] joining {addr} (id {room_id:08X})");

    let mut client = ble.new_client();
    client.connect(&addr).await.ok()?;

    {
        let svc = client.get_service(uuid16(SERVICE_UUID)).await.ok()?;
        let tx = svc.get_characteristic(uuid16(TX_UUID)).await.ok()?;
        let inbox_tx = inbox;
        tx.on_notify(move |data| {
            if let Some(msg) = RoomMsg::decode(data) {
                inbox_tx.push(PlayerId([0; 6]), msg);
            }
        });
        tx.subscribe_notify(false).await.ok()?;
    }

    Some((
        Room::guest(RoomConfig::bo5(room_id, "ROOM"), my_id, my_name),
        client,
        room_id,
    ))
}

/// Write one message from the guest to the host.
async fn guest_write(client: &mut BLEClient, bytes: &[u8]) {
    let Ok(svc) = client.get_service(uuid16(SERVICE_UUID)).await else {
        return;
    };
    let Ok(rx) = svc.get_characteristic(uuid16(RX_UUID)).await else {
        return;
    };
    if let Err(e) = rx.write_value(bytes, false).await {
        log::warn!("[room] write failed: {e:?}");
    }
}

fn host_broadcast(tx: &Arc<NimbleMutex<BLECharacteristic>>, msg: &RoomMsg) {
    let bytes = msg.encode();
    tx.lock().set_value(&bytes).notify();
}

fn view_for(room: &Room, note: &str) -> RoomView {
    let phase = match room.phase {
        RoomPhase::Lobby => "LOBBY",
        RoomPhase::Scramble => "SCRAMBLE",
        RoomPhase::Racing => "RACING",
        RoomPhase::RoundDone => "ROUND DONE",
        RoomPhase::MatchDone => "FINISHED",
    };
    RoomView {
        room_id: room.cfg.room_id,
        name: room.cfg.name.clone(),
        hosting: room.is_host(),
        phase,
        round: room.round,
        rounds: room.cfg.rounds,
        seed: room.seed,
        players: room
            .players
            .iter()
            .map(|p| RoomPlayerView {
                name: p.name.clone(),
                wins: p.wins,
                ready: p.ready,
                is_me: p.id == room.my_id,
                last_ms: p.last_ms,
                dnf: p.last_penalty == Penalty::Dnf,
            })
            .collect(),
        winner: room
            .winner
            .and_then(|id| room.player(id).map(|p| p.name.clone())),
        note: note.to_string(),
    }
}

fn publish(app: &SharedState, room: Option<&Room>, note: &str) {
    let next = room.map(|r| view_for(r, note));
    let mut st = app.lock().unwrap();
    if st.room == next {
        return;
    }
    st.room = next;
    st.bump();
}

/// Drive the room: consume UI actions, pump BLE messages, start rounds.
pub async fn room_task(ble: &'static BLEDevice, app: SharedState) {
    let my_id = local_id(ble);
    let my_name = short_name(&my_id);
    let inbox = Arc::new(Inbox::new());
    let host_tx = setup_transport(ble, inbox.clone());

    let mut session: Option<Session> = None;
    let mut started: Option<(u8, u64)> = None;
    let mut ready_sent: Option<u8> = None;
    let mut note = String::new();
    let mut racing_since: Option<u64> = None;

    loop {
        // ---- UI requests -------------------------------------------------
        let action = { app.lock().unwrap().take_room_action() };
        match action {
            Some(RoomAction::Create) => {
                if matches!(session, Some(Session::Host { .. })) {
                    // Already hosting.
                } else {
                    session = None;
                    let room_id = (shared::api::hw_random() & 0xFFFF_FFFF) as u32;
                    let cfg = RoomConfig::bo5(room_id, "TABLE");
                    match &host_tx {
                        Some(tx) if advertise(ble, room_id) => {
                            let room = Room::host(cfg, my_id, &my_name, shared::api::hw_random());
                            note = format!("hosting {room_id:08X}");
                            session = Some(Session::Host {
                                room,
                                tx: tx.clone(),
                            });
                        }
                        _ => note = "host failed".to_string(),
                    }
                }
                started = None;
                ready_sent = None;
            }
            Some(RoomAction::Join) => {
                if matches!(session, Some(Session::Host { .. })) {
                    stop_advertising(ble);
                }
                session = None;
                match join_guest(ble, inbox.clone(), my_id, &my_name).await {
                    Some((room, mut client, room_id)) => {
                        note = format!("joined {room_id:08X}");
                        let join = room.join_msg();
                        guest_write(&mut client, &join.encode()).await;
                        session = Some(Session::Guest { room, client });
                    }
                    None => note = "no room found".to_string(),
                }
                started = None;
                ready_sent = None;
            }
            Some(RoomAction::Leave) => {
                if let Some(Session::Guest { room, client }) = &mut session {
                    let msg = room.leave_msg();
                    guest_write(client, &msg.encode()).await;
                }
                stop_advertising(ble);
                session = None;
                started = None;
                ready_sent = None;
                note = "left room".to_string();
            }
            Some(RoomAction::Begin) => {
                if let Some(Session::Host { room, tx }) = &mut session {
                    for msg in room.host_begin() {
                        host_broadcast(tx, &msg);
                    }
                    note = "match started".to_string();
                }
            }
            None => {}
        }

        // ---- inbound BLE messages ----------------------------------------
        let incoming = inbox.drain();
        if !incoming.is_empty() {
            match &mut session {
                Some(Session::Host { room, tx }) => {
                    for (peer, msg) in incoming {
                        // Trust the transport for the peer identity.
                        let msg = match msg {
                            RoomMsg::Join { name, .. } => RoomMsg::Join { id: peer, name },
                            RoomMsg::Ready { round, .. } => RoomMsg::Ready { id: peer, round },
                            RoomMsg::Leave { .. } => RoomMsg::Leave { id: peer },
                            RoomMsg::Result(mut r) => {
                                r.id = peer;
                                RoomMsg::Result(r)
                            }
                            other => other,
                        };
                        for out in room.on_msg(msg) {
                            host_broadcast(tx, &out);
                        }
                    }
                }
                Some(Session::Guest { room, .. }) => {
                    for (_, msg) in incoming {
                        let is_end = matches!(msg, RoomMsg::MatchEnd { .. });
                        room.on_msg(msg);
                        if is_end {
                            note = "match finished".to_string();
                        }
                    }
                }
                None => {}
            }
        }

        // ---- start a round with the room seed ----------------------------
        if let Some(room) = session.as_ref().map(|s| s.room()) {
            if room.phase == RoomPhase::Scramble && room.seed != 0 {
                let key = (room.round, room.seed);
                if started != Some(key) {
                    started = Some(key);
                    ready_sent = None;
                    let (round, seed) = (room.round, room.seed);
                    app.lock().unwrap().start_room_round(seed);
                    note = format!("round {round} ready");
                }
            }
        }

        // ---- report local ready / result --------------------------------
        {
            let (phase, room_phase, round) = {
                let st = app.lock().unwrap();
                (
                    st.phase(),
                    session.as_ref().map(|s| s.room().phase),
                    session.as_ref().map(|s| s.room().round).unwrap_or(0),
                )
            };
            if room_phase == Some(RoomPhase::Scramble)
                && phase == shared::timer::Phase::Ready
                && ready_sent != Some(round)
            {
                ready_sent = Some(round);
                if let Some(session) = &mut session {
                    match session {
                        Session::Host { room, tx } => {
                            for m in room.host_ready() {
                                host_broadcast(tx, &m);
                            }
                        }
                        Session::Guest { room, client } => {
                            let msg = room.ready_msg();
                            guest_write(client, &msg.encode()).await;
                        }
                    }
                }
            }

            let result = app.lock().unwrap().take_room_result();
            if let Some(r) = result {
                let penalty = if r.dnf {
                    Penalty::Dnf
                } else if r.plus_two {
                    Penalty::PlusTwo
                } else {
                    Penalty::None
                };
                if let Some(session) = &mut session {
                    match session {
                        Session::Host { room, tx } => {
                            let entry = ResultEntry {
                                id: my_id,
                                round: room.round,
                                time_ms: r.raw_ms,
                                penalty,
                                step: r.step,
                                down_color: r.down_color,
                            };
                            for m in room.host_result(entry) {
                                host_broadcast(tx, &m);
                            }
                        }
                        Session::Guest { room, client } => {
                            let msg = room.result_msg(r.raw_ms, penalty, r.step, r.down_color);
                            guest_write(client, &msg.encode()).await;
                        }
                    }
                }
            }
        }

        // ---- host: force-advance a stalled round ------------------------
        {
            let now = shared::api::now_ms();
            if let Some(Session::Host { room, tx }) = &mut session {
                if room.phase == RoomPhase::Racing {
                    let since = *racing_since.get_or_insert(now);
                    if now.saturating_sub(since) > 180_000 {
                        // Give up on missing results so the match progresses.
                        let missing: Vec<PlayerId> = room
                            .players
                            .iter()
                            .filter(|p| !p.reported)
                            .map(|p| p.id)
                            .collect();
                        for id in missing {
                            let entry = ResultEntry {
                                id,
                                round: room.round,
                                time_ms: 300_000,
                                penalty: Penalty::Dnf,
                                step: 0,
                                down_color: 0,
                            };
                            for m in room.on_msg(RoomMsg::Result(entry)) {
                                host_broadcast(tx, &m);
                            }
                        }
                        racing_since = None;
                    }
                } else {
                    racing_since = None;
                }
            } else {
                racing_since = None;
            }
        }

        // ---- publish the view --------------------------------------------
        publish(&app, session.as_ref().map(|s| s.room()), &note);

        Timer::after(ETDuration::from_millis(50)).await;
    }
}

