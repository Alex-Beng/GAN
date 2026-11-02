//! Shared runtime state between the BLE, storage, sync, input and UI threads.
//!
//! Everything is behind one `Mutex`; the loops are short and the contention is
//! negligible at UI rates. A monotonically increasing `revision` lets the UI
//! skip redraws when nothing changed.

use std::sync::{Arc, Mutex};

use shared::api::now_ms;
use shared::cube::{is_solved, Move};
use shared::protocol::CubeState;
use shared::timer::{
    Attempt, CubeType, GroupType, Penalty, Phase, TimerEvent, TimerRecord, TimerSession,
};

pub type SharedState = Arc<Mutex<AppState>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Timer,
    Records,
    Room,
    Status,
    Config,
}

impl Page {
    pub fn next(self) -> Self {
        match self {
            Page::Timer => Page::Records,
            Page::Records => Page::Room,
            Page::Room => Page::Status,
            Page::Status => Page::Config,
            Page::Config => Page::Timer,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Page::Timer => "TIMER",
            Page::Records => "RECORDS",
            Page::Room => "ROOM",
            Page::Status => "STATUS",
            Page::Config => "CONFIG",
        }
    }
}

/// A request from the UI to the room task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomAction {
    /// Host a new room.
    Create,
    /// Join the first room found on air.
    Join,
    /// Leave the current room / cancel hosting.
    Leave,
    /// Host: begin the match (needs >= 1 guest).
    Begin,
}

/// UI-facing snapshot of the room (no borrowed state).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomView {
    pub room_id: u32,
    pub name: String,
    pub hosting: bool,
    pub phase: &'static str,
    pub round: u8,
    pub rounds: u8,
    pub seed: u64,
    pub players: Vec<RoomPlayerView>,
    pub winner: Option<String>,
    /// Human-readable last action/result.
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomPlayerView {
    pub name: String,
    pub wins: u8,
    pub ready: bool,
    pub is_me: bool,
    pub last_ms: u32,
    pub dnf: bool,
}

/// A finished attempt to be reported to the room.
#[derive(Clone, Copy, Debug)]
pub struct RoomResultOut {
    pub raw_ms: u32,
    pub plus_two: bool,
    pub dnf: bool,
    pub step: u8,
    pub down_color: u8,
}

/// One finished attempt waiting to be persisted, then uploaded.
#[derive(Clone, Debug)]
pub struct CompletedAttempt {
    /// Monotonic key, also used as the file name suffix.
    pub key: u64,
    pub json: String,
}

#[derive(Clone, Debug)]
pub struct RecentRow {
    pub duration_ms: u32,
    pub penalty: Penalty,
    pub scramble: String,
}

impl RecentRow {
    /// Result in WCA-centiseconds with penalty applied.
    pub fn centis(&self) -> Option<u32> {
        shared::timer::centis(self.duration_ms, self.penalty)
    }

    /// Milliseconds after the +2 penalty (DNF stays as measured).
    pub fn display_ms(&self) -> u32 {
        match self.penalty {
            Penalty::PlusTwo => self.duration_ms.saturating_add(2000),
            _ => self.duration_ms,
        }
    }
}

pub struct AppState {
    pub page: Page,
    pub revision: u64,

    // --- cube / session ---
    pub live: CubeState,
    pub session: Option<TimerSession>,
    pub scramble_len: usize,

    // --- BLE ---
    pub ble_connected: bool,
    pub ble_name: String,
    pub battery: u8,

    // --- account / network ---
    pub user_id: i64,
    pub nick: String,
    pub logged_in: bool,
    pub wifi_ok: bool,
    pub pending_uploads: usize,
    pub status_msg: String,

    // --- results ---
    pub recent: Vec<RecentRow>,
    pub group_type: GroupType,
    pub group_id: u32,
    pub cube_type: CubeType,
    pub cube_smart: u8,

    // --- persistence handoff ---
    pub completed: Vec<CompletedAttempt>,
    next_key: u64,

    // --- face-to-face room ---
    pub room: Option<RoomView>,
    pub room_action: Option<RoomAction>,
    pub room_result: Option<RoomResultOut>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            page: Page::Timer,
            revision: 0,
            live: CubeState::solved(),
            session: None,
            scramble_len: 20,
            ble_connected: false,
            ble_name: String::new(),
            battery: 0,
            user_id: 0,
            nick: String::new(),
            logged_in: false,
            wifi_ok: false,
            pending_uploads: 0,
            status_msg: String::new(),
            recent: Vec::new(),
            // 自由训练 / 复原练习 / 三阶 is the sane default for a plain timer.
            group_type: GroupType::Free,
            group_id: shared::timer::GROUP_SOLVE_3,
            cube_type: CubeType::Three,
            cube_smart: 1,
            completed: Vec::new(),
            next_key: 1,
            room: None,
            room_action: None,
            room_result: None,
        }
    }

    pub fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    // ---------------------------------------------------------------- session

    /// Start a fresh attempt from the current live cube state.
    pub fn new_session(&mut self, seed: u64) {
        let mut rng = shared::cube::XorShift64::new(seed);
        let scramble = shared::cube::random_scramble(self.scramble_len, &mut rng);
        let base = if is_solved(&self.live) {
            CubeState::solved()
        } else {
            self.live.clone()
        };
        self.session = Some(TimerSession::new(base, scramble));
        self.bump();
    }

    /// Feed a live cube state (facelets snapshot); returns true when the UI
    /// should react.
    pub fn on_cube_state(&mut self, live: &CubeState) -> bool {
        self.live = live.clone();
        self.advance_session()
    }

    /// Feed an observed physical turn.
    ///
    /// Most protocols only stream move events (facelet snapshots are rare), so
    /// the session is advanced here too — otherwise a solve would never be
    /// detected as finished.
    pub fn on_move(&mut self, m: Move) {
        m.apply(&mut self.live);
        let now = now_ms();
        let was_armed = self.session.as_ref().map(|s| s.is_armed()).unwrap_or(false);
        // Advance first: a move that leaves the armed state must be counted as
        // a solving turn (and may even finish a one-move solve).
        self.advance_session();
        if let Some(session) = self.session.as_mut() {
            session.on_move(m, now);
        }
        if was_armed {
            self.bump();
        }
    }

    /// Run the session state machine against `self.live`.
    fn advance_session(&mut self) -> bool {
        let now = now_ms();
        let live = self.live.clone();
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let event = session.on_state(&live, now);
        let changed = event.is_some();
        if let Some(TimerEvent::Finished { .. }) = event {
            self.finish_attempt();
        }
        if changed {
            self.bump();
        }
        changed
    }

    fn finish_attempt(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let Some(attempt) = session.attempt() else {
            return;
        };
        let penalty = session.penalty;
        self.recent.insert(
            0,
            RecentRow {
                duration_ms: attempt.raw_ms,
                penalty,
                scramble: shared::cube::moves_to_string(&attempt.scramble),
            },
        );
        if self.recent.len() > 100 {
            self.recent.truncate(100);
        }
        let sol = shared::timer::analyse(&attempt.start, &attempt.steps, attempt.duration_ms);
        let record = self.record_for(&attempt, &sol);
        let key = self.next_key;
        self.next_key = self.next_key.wrapping_add(1);
        self.completed.push(CompletedAttempt {
            key,
            json: record.to_json(),
        });

        // Feed the room, if any: the room task will forward this result.
        if self.room.is_some() {
            self.room_result = Some(RoomResultOut {
                raw_ms: attempt.raw_ms,
                plus_two: penalty == Penalty::PlusTwo,
                dnf: attempt.dnf,
                step: sol.whole.turn.min(255) as u8,
                down_color: sol.down_color,
            });
        }
        self.bump();
    }

    fn record_for(&self, attempt: &Attempt, sol: &shared::timer::Solution) -> TimerRecord {
        let mut rec = TimerRecord::new(
            self.user_id,
            self.cube_type,
            self.group_type,
            self.group_id,
            self.cube_smart,
        );
        rec.disrupt = shared::cube::moves_to_string(&attempt.scramble);
        rec.time_ms = attempt.duration_ms;
        rec.time_status = if attempt.dnf {
            shared::timer::TimeStatus::Dnf
        } else {
            shared::timer::TimeStatus::Normal
        };
        rec.cube_model = self.ble_name.clone();

        // CFOP phase analysis (turn/tps/fluency per phase + review moves).
        rec.tps = sol.whole.tps_milli;
        rec.step = sol.whole.turn;
        rec.fluency = sol.whole.smooth_index;
        rec.turn = sol.whole.rotate;
        rec.down_color = sol.down_color;
        rec.msg_solution_analyse = shared::timer::to_json_array(sol);
        rec.review = shared::timer::review_json(&attempt.steps);
        rec
    }

    // ------------------------------------------------------------------ stats

    /// WCA average of `n` (best and worst removed).
    pub fn average(&self, n: usize) -> Option<i64> {
        let values: Vec<Option<u32>> = self.recent.iter().map(|r| r.centis()).collect();
        shared::timer::average(&values, n)
    }

    pub fn best(&self) -> Option<u32> {
        let values: Vec<Option<u32>> = self.recent.iter().map(|r| r.centis()).collect();
        shared::timer::best(&values)
    }

    /// Cycle the penalty on the most recent result.
    pub fn cycle_penalty(&mut self) {
        if let Some(row) = self.recent.first_mut() {
            row.penalty = match row.penalty {
                Penalty::None => Penalty::PlusTwo,
                Penalty::PlusTwo => Penalty::Dnf,
                Penalty::Dnf => Penalty::None,
            };
            self.bump();
        }
    }

    pub fn phase(&self) -> Phase {
        self.session
            .as_ref()
            .map(|s| s.phase)
            .unwrap_or(Phase::Scrambling)
    }

    pub fn scramble_text(&self) -> String {
        self.session
            .as_ref()
            .map(|s| s.scramble_text())
            .unwrap_or_default()
    }

    pub fn elapsed_ms(&self) -> u32 {
        self.session
            .as_ref()
            .map(|s| s.elapsed_ms(now_ms()))
            .unwrap_or(0)
    }

    // ----------------------------------------------------------- room glue

    /// Take the pending room request (set by the buttons).
    pub fn take_room_action(&mut self) -> Option<RoomAction> {
        self.room_action.take()
    }

    /// Take the finished attempt destined for the room.
    pub fn take_room_result(&mut self) -> Option<RoomResultOut> {
        self.room_result.take()
    }

    /// Start a new attempt from a room-provided seed, so every device gets the
    /// same scramble.
    pub fn start_room_round(&mut self, seed: u64) {
        self.new_session(seed);
    }
}

/// Create the shared state handle.
pub fn new_shared() -> SharedState {
    Arc::new(Mutex::new(AppState::new()))
}
