//! Face-to-face room protocol and game logic (transport-agnostic).
//!
//! Transport is BLE GATT: the host advertises a custom service and broadcasts
//! room state via notifications; guests connect and write commands. Because the
//! cubebox only needs to agree on a **scramble seed** (WCA scramble generation is
//! deterministic), messages stay tiny.
//!
//! Match format: best-of-5 (first to 3 round wins). Each round:
//!   1. host picks a seed and broadcasts `State` (phase = Scramble)
//!   2. every device builds the same scramble locally and reports `Ready`
//!   3. when all are ready the host broadcasts `RoundStart` (3-2-1 prompt)
//!   4. each device times its own solve (starts on its own first turn)
//!   5. `Result` messages are tallied; the host broadcasts the next `State`
//!   6. first player to `wins_needed` wins the match (`MatchEnd`)

pub const PROTO_VER: u8 = 1;
pub const MAX_NAME: usize = 8;
pub const MAX_PLAYERS: usize = 5;

/// BLE service/characteristic UUIDs (16-bit, custom range).
pub const SERVICE_UUID: u16 = 0xFFA0;
pub const TX_UUID: u16 = 0xFFA1; // host -> guests (notify)
pub const RX_UUID: u16 = 0xFFA2; // guests -> host (write)
/// Advertising name prefix; the rest is the room id in hex (`GANW-1A2B3C4D`).
pub const ADV_PREFIX: &str = "GANW-";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RoomPhase {
    Lobby = 0,
    Scramble = 1,
    Racing = 2,
    RoundDone = 3,
    MatchDone = 4,
}

impl RoomPhase {
    fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => RoomPhase::Lobby,
            1 => RoomPhase::Scramble,
            2 => RoomPhase::Racing,
            3 => RoomPhase::RoundDone,
            4 => RoomPhase::MatchDone,
            _ => return None,
        })
    }
}

/// Bluetooth address of a player (6 bytes, little-endian as NimBLE reports it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerId(pub [u8; 6]);

impl PlayerId {
    pub fn hex(&self) -> String {
        let mut s = String::with_capacity(12);
        for b in self.0.iter() {
            s.push_str(&format!("{b:02X}"));
        }
        s
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Penalty {
    None = 0,
    PlusTwo = 1,
    Dnf = 2,
}

impl Penalty {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Penalty::PlusTwo,
            2 => Penalty::Dnf,
            _ => Penalty::None,
        }
    }

    /// Result in milliseconds with the penalty applied (`None` for DNF).
    pub fn apply(self, raw_ms: u32) -> Option<u32> {
        match self {
            Penalty::None => Some(raw_ms),
            Penalty::PlusTwo => Some(raw_ms.saturating_add(2000)),
            Penalty::Dnf => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RoomConfig {
    pub room_id: u32,
    pub name: String,
    pub max_players: u8,
    /// Total rounds (5 for bo5).
    pub rounds: u8,
    /// Round wins needed to take the match (3 for bo5).
    pub wins_needed: u8,
    pub scramble_len: u8,
    /// 3 = 三阶, 2 = 二阶.
    pub cube_type: u8,
}

impl RoomConfig {
    pub fn bo5(room_id: u32, name: &str) -> Self {
        Self {
            room_id,
            name: name.to_string(),
            max_players: MAX_PLAYERS as u8,
            rounds: 5,
            wins_needed: 3,
            scramble_len: 20,
            cube_type: 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub ready: bool,
    pub reported: bool,
    pub wins: u8,
    pub last_ms: u32,
    pub last_penalty: Penalty,
    /// False when the host lost the BLE link to this player.
    pub connected: bool,
}

impl Player {
    fn new(id: PlayerId, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            ready: false,
            reported: false,
            wins: 0,
            last_ms: 0,
            last_penalty: Penalty::None,
            connected: true,
        }
    }

    /// Effective result for ranking (`None` = DNF / not reported).
    pub fn effective_ms(&self) -> Option<u32> {
        if self.last_ms == 0 {
            return None;
        }
        self.last_penalty.apply(self.last_ms)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultEntry {
    pub id: PlayerId,
    pub round: u8,
    pub time_ms: u32,
    pub penalty: Penalty,
    pub step: u8,
    pub down_color: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomMsg {
    /// guest -> host: request to join.
    Join { id: PlayerId, name: String },
    /// host -> all: authoritative room snapshot.
    State {
        phase: RoomPhase,
        round: u8,
        seed: u64,
        players: Vec<(PlayerId, String, bool, bool, u8, u32, Penalty)>,
    },
    /// guest -> host: scramble applied for `round`.
    Ready { id: PlayerId, round: u8 },
    /// host -> all: start racing now (cosmetic countdown already shown).
    RoundStart { round: u8, seed: u64 },
    /// guest -> host: finished attempt.
    Result(ResultEntry),
    /// host -> all: match finished (winner, or None on a tie).
    MatchEnd { winner: Option<PlayerId>, wins: Vec<(PlayerId, u8)> },
    /// guest -> host: leaving the room.
    Leave { id: PlayerId },
}

// --------------------------------------------------------------- encoding

fn put_name(out: &mut Vec<u8>, name: &str) {
    let bytes: Vec<u8> = name
        .char_indices()
        .take_while(|(i, _)| *i < MAX_NAME)
        .map(|(_, c)| c as u8)
        .collect();
    out.push(bytes.len() as u8);
    out.extend_from_slice(&bytes);
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn u8(&mut self) -> Option<u8> {
        let v = *self.buf.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }
    fn u32(&mut self) -> Option<u32> {
        let mut v = 0u32;
        for i in 0..4 {
            v |= (self.u8()? as u32) << (8 * i);
        }
        Some(v)
    }
    fn u64(&mut self) -> Option<u64> {
        let mut v = 0u64;
        for i in 0..8 {
            v |= (self.u8()? as u64) << (8 * i);
        }
        Some(v)
    }
    fn id(&mut self) -> Option<PlayerId> {
        let mut id = [0u8; 6];
        for b in id.iter_mut() {
            *b = self.u8()?;
        }
        Some(PlayerId(id))
    }
    fn name(&mut self) -> Option<String> {
        let len = self.u8()? as usize;
        if len > MAX_NAME || self.pos + len > self.buf.len() {
            return None;
        }
        let s = String::from_utf8_lossy(&self.buf[self.pos..self.pos + len]).to_string();
        self.pos += len;
        Some(s)
    }
    fn done(&self) -> bool {
        self.pos == self.buf.len()
    }
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

impl RoomMsg {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(96);
        out.push(PROTO_VER);
        match self {
            RoomMsg::Join { id, name } => {
                out.push(1);
                out.extend_from_slice(&id.0);
                put_name(&mut out, name);
            }
            RoomMsg::State {
                phase,
                round,
                seed,
                players,
            } => {
                out.push(2);
                out.push(*phase as u8);
                out.push(*round);
                put_u64(&mut out, *seed);
                out.push(players.len() as u8);
                for (id, name, ready, reported, wins, last_ms, penalty) in players {
                    out.extend_from_slice(&id.0);
                    put_name(&mut out, name);
                    out.push((*ready as u8) | ((*reported as u8) << 1));
                    out.push(*wins);
                    put_u32(&mut out, *last_ms);
                    out.push(*penalty as u8);
                }
            }
            RoomMsg::Ready { id, round } => {
                out.push(3);
                out.extend_from_slice(&id.0);
                out.push(*round);
            }
            RoomMsg::RoundStart { round, seed } => {
                out.push(4);
                out.push(*round);
                put_u64(&mut out, *seed);
            }
            RoomMsg::Result(r) => {
                out.push(5);
                out.extend_from_slice(&r.id.0);
                out.push(r.round);
                put_u32(&mut out, r.time_ms);
                out.push(r.penalty as u8);
                out.push(r.step);
                out.push(r.down_color);
            }
            RoomMsg::MatchEnd { winner, wins } => {
                out.push(6);
                match winner {
                    Some(id) => {
                        out.push(1);
                        out.extend_from_slice(&id.0);
                    }
                    None => {
                        out.push(0);
                        out.extend_from_slice(&[0u8; 6]);
                    }
                }
                out.push(wins.len() as u8);
                for (id, w) in wins {
                    out.extend_from_slice(&id.0);
                    out.push(*w);
                }
            }
            RoomMsg::Leave { id } => {
                out.push(7);
                out.extend_from_slice(&id.0);
            }
        }
        out
    }

    pub fn decode(buf: &[u8]) -> Option<RoomMsg> {
        let mut r = Reader::new(buf);
        if r.u8()? != PROTO_VER {
            return None;
        }
        let msg = match r.u8()? {
            1 => {
                let id = r.id()?;
                let name = r.name()?;
                RoomMsg::Join { id, name }
            }
            2 => {
                let phase = RoomPhase::from_u8(r.u8()?)?;
                let round = r.u8()?;
                let seed = r.u64()?;
                let n = r.u8()? as usize;
                if n > MAX_PLAYERS {
                    return None;
                }
                let mut players = Vec::with_capacity(n);
                for _ in 0..n {
                    let id = r.id()?;
                    let name = r.name()?;
                    let flags = r.u8()?;
                    let wins = r.u8()?;
                    let last_ms = r.u32()?;
                    let penalty = Penalty::from_u8(r.u8()?);
                    players.push((id, name, flags & 1 != 0, flags & 2 != 0, wins, last_ms, penalty));
                }
                RoomMsg::State {
                    phase,
                    round,
                    seed,
                    players,
                }
            }
            3 => RoomMsg::Ready {
                id: r.id()?,
                round: r.u8()?,
            },
            4 => RoomMsg::RoundStart {
                round: r.u8()?,
                seed: r.u64()?,
            },
            5 => RoomMsg::Result(ResultEntry {
                id: r.id()?,
                round: r.u8()?,
                time_ms: r.u32()?,
                penalty: Penalty::from_u8(r.u8()?),
                step: r.u8()?,
                down_color: r.u8()?,
            }),
            6 => {
                let has = r.u8()? != 0;
                let wid = r.id()?;
                let n = r.u8()? as usize;
                if n > MAX_PLAYERS {
                    return None;
                }
                let mut wins = Vec::with_capacity(n);
                for _ in 0..n {
                    wins.push((r.id()?, r.u8()?));
                }
                RoomMsg::MatchEnd {
                    winner: if has { Some(wid) } else { None },
                    wins,
                }
            }
            7 => RoomMsg::Leave { id: r.id()? },
            _ => return None,
        };
        if r.done() {
            Some(msg)
        } else {
            None
        }
    }
}

// ------------------------------------------------------------ room logic

pub struct Room {
    pub cfg: RoomConfig,
    pub role: Role,
    pub phase: RoomPhase,
    pub round: u8,
    pub seed: u64,
    pub players: Vec<Player>,
    pub my_id: PlayerId,
    pub my_name: String,
    /// Winner announced by the host.
    pub winner: Option<PlayerId>,
    seed_state: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Host,
    Guest,
}

impl Room {
    pub fn host(cfg: RoomConfig, my_id: PlayerId, my_name: &str, seed: u64) -> Self {
        let mut room = Self {
            cfg,
            role: Role::Host,
            phase: RoomPhase::Lobby,
            round: 0,
            seed: 0,
            players: vec![Player::new(my_id, my_name)],
            my_id,
            my_name: my_name.to_string(),
            winner: None,
            seed_state: if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed },
        };
        room.seed_state = room.seed_state.wrapping_mul(6364136223846793005).wrapping_add(1);
        room
    }

    pub fn guest(cfg: RoomConfig, my_id: PlayerId, my_name: &str) -> Self {
        Self {
            cfg,
            role: Role::Guest,
            phase: RoomPhase::Lobby,
            round: 0,
            seed: 0,
            players: Vec::new(),
            my_id,
            my_name: my_name.to_string(),
            winner: None,
            seed_state: 0,
        }
    }

    pub fn is_host(&self) -> bool {
        self.role == Role::Host
    }

    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.players.iter().find(|p| p.id == id)
    }

    fn player_mut(&mut self, id: PlayerId) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.id == id)
    }

    fn next_seed(&mut self) -> u64 {
        self.seed_state = self
            .seed_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.seed_state
    }

    pub fn state_msg(&self) -> RoomMsg {
        RoomMsg::State {
            phase: self.phase,
            round: self.round,
            seed: self.seed,
            players: self
                .players
                .iter()
                .map(|p| {
                    (
                        p.id,
                        p.name.clone(),
                        p.ready,
                        p.reported,
                        p.wins,
                        p.last_ms,
                        p.last_penalty,
                    )
                })
                .collect(),
        }
    }

    // ---------------------------------------------------------------- host

    /// Host: begin the match (lobby -> round 1 scramble).
    pub fn host_begin(&mut self) -> Vec<RoomMsg> {
        if self.role != Role::Host || self.phase != RoomPhase::Lobby || self.players.len() < 2 {
            return Vec::new();
        }
        self.round = 1;
        self.seed = self.next_seed();
        self.begin_scramble();
        vec![self.state_msg()]
    }

    fn begin_scramble(&mut self) {
        self.phase = RoomPhase::Scramble;
        for p in self.players.iter_mut() {
            p.ready = false;
            p.reported = false;
            p.last_ms = 0;
            p.last_penalty = Penalty::None;
        }
    }

    /// Host: handle an inbound message, returning broadcasts to send.
    pub fn on_msg(&mut self, msg: RoomMsg) -> Vec<RoomMsg> {
        match self.role {
            Role::Host => self.host_on_msg(msg),
            Role::Guest => {
                self.guest_on_msg(msg);
                Vec::new()
            }
        }
    }

    fn host_on_msg(&mut self, msg: RoomMsg) -> Vec<RoomMsg> {
        match msg {
            RoomMsg::Join { id, name } => {
                if self.phase != RoomPhase::Lobby {
                    return Vec::new();
                }
                if self.players.len() as u8 >= self.cfg.max_players {
                    return Vec::new();
                }
                if self.player(id).is_none() {
                    self.players.push(Player::new(id, &name));
                } else if let Some(p) = self.player_mut(id) {
                    p.connected = true;
                    p.name = name;
                }
                vec![self.state_msg()]
            }
            RoomMsg::Leave { id } => {
                if self.phase == RoomPhase::Lobby {
                    self.players.retain(|p| p.id != id);
                    return vec![self.state_msg()];
                }
                // Mid-match: treat the player as offline and DNF this round, so
                // the match can still advance.
                if let Some(p) = self.player_mut(id) {
                    p.connected = false;
                    p.ready = true;
                    p.reported = true;
                    p.last_ms = 0;
                    p.last_penalty = Penalty::Dnf;
                }
                self.check_advance()
            }
            RoomMsg::Ready { id, round } => {
                if self.phase != RoomPhase::Scramble || round != self.round {
                    return Vec::new();
                }
                if let Some(p) = self.player_mut(id) {
                    p.ready = true;
                }
                self.check_advance()
            }
            RoomMsg::Result(r) => {
                if self.phase != RoomPhase::Racing || r.round != self.round {
                    return Vec::new();
                }
                if let Some(p) = self.player_mut(r.id) {
                    p.reported = true;
                    p.last_ms = r.time_ms;
                    p.last_penalty = r.penalty;
                    p.ready = false;
                }
                self.check_advance()
            }
            _ => Vec::new(),
        }
    }

    /// Advance the phase when every player has reported / is ready.
    fn check_advance(&mut self) -> Vec<RoomMsg> {
        match self.phase {
            RoomPhase::Scramble if self.players.iter().all(|p| p.ready) => {
                self.phase = RoomPhase::Racing;
                vec![
                    self.state_msg(),
                    RoomMsg::RoundStart {
                        round: self.round,
                        seed: self.seed,
                    },
                ]
            }
            RoomPhase::Racing if self.players.iter().all(|p| p.reported) => self.tally_round(),
            _ => vec![self.state_msg()],
        }
    }

    fn tally_round(&mut self) -> Vec<RoomMsg> {
        // Fastest effective time wins the round.
        let mut best: Option<(PlayerId, u32)> = None;
        for p in self.players.iter() {
            if let Some(ms) = p.effective_ms() {
                if best.map_or(true, |(_, b)| ms < b) {
                    best = Some((p.id, ms));
                }
            }
        }
        // A DNF can still win if everyone else DNF'd; handled by effective_ms None.
        let mut winner = None;
        if let Some((id, best_ms)) = best {
            let ties = self
                .players
                .iter()
                .filter(|p| p.effective_ms() == Some(best_ms))
                .count();
            if ties == 1 {
                winner = Some(id);
            }
        }
        if let Some(id) = winner {
            if let Some(p) = self.player_mut(id) {
                p.wins = p.wins.saturating_add(1);
            }
        }

        self.phase = RoomPhase::RoundDone;

        let match_over = self.players.iter().any(|p| p.wins >= self.cfg.wins_needed)
            || self.round >= self.cfg.rounds;

        if match_over {
            self.phase = RoomPhase::MatchDone;
            let winner = self
                .players
                .iter()
                .max_by_key(|p| (p.wins, std::cmp::Reverse(p.id)))
                .map(|p| p.id);
            // A tie on wins means no winner.
            let top = self.players.iter().map(|p| p.wins).max().unwrap_or(0);
            let leaders = self.players.iter().filter(|p| p.wins == top).count();
            self.winner = if leaders == 1 { winner } else { None };
            let wins = self.players.iter().map(|p| (p.id, p.wins)).collect();
            return vec![self.state_msg(), RoomMsg::MatchEnd { winner: self.winner, wins }];
        }

        self.round = self.round.saturating_add(1);
        self.seed = self.next_seed();
        // Keep the just-finished results visible in the RoundDone snapshot.
        let finished = self.state_msg();
        self.begin_scramble();
        let next = self.state_msg();
        vec![finished, next]
    }

    /// Host: mark itself ready (host pressed start on the scramble).
    pub fn host_ready(&mut self) -> Vec<RoomMsg> {
        if self.role != Role::Host {
            return Vec::new();
        }
        self.host_on_msg(RoomMsg::Ready {
            id: self.my_id,
            round: self.round,
        })
    }

    /// Host: submit its own result.
    pub fn host_result(&mut self, r: ResultEntry) -> Vec<RoomMsg> {
        if self.role != Role::Host {
            return Vec::new();
        }
        self.host_on_msg(RoomMsg::Result(r))
    }

    /// Host: a player's BLE link dropped.
    pub fn host_disconnected(&mut self, id: PlayerId) -> Vec<RoomMsg> {
        if self.role != Role::Host || id == self.my_id {
            return Vec::new();
        }
        // Same handling as an explicit leave: lobby -> removed, mid-match -> DNF
        // this round so the match can still advance.
        self.host_on_msg(RoomMsg::Leave { id })
    }

    // --------------------------------------------------------------- guest

    pub fn join_msg(&self) -> RoomMsg {
        RoomMsg::Join {
            id: self.my_id,
            name: self.my_name.clone(),
        }
    }

    pub fn ready_msg(&self) -> RoomMsg {
        RoomMsg::Ready {
            id: self.my_id,
            round: self.round,
        }
    }

    pub fn result_msg(&self, raw_ms: u32, penalty: Penalty, step: u8, down_color: u8) -> RoomMsg {
        RoomMsg::Result(ResultEntry {
            id: self.my_id,
            round: self.round,
            time_ms: raw_ms,
            penalty,
            step,
            down_color,
        })
    }

    pub fn leave_msg(&self) -> RoomMsg {
        RoomMsg::Leave { id: self.my_id }
    }

    fn guest_on_msg(&mut self, msg: RoomMsg) {
        match msg {
            RoomMsg::State {
                phase,
                round,
                seed,
                players,
            } => {
                self.phase = phase;
                self.round = round;
                if seed != 0 {
                    self.seed = seed;
                }
                self.players = players
                    .into_iter()
                    .map(|(id, name, ready, reported, wins, last_ms, penalty)| Player {
                        id,
                        name,
                        ready,
                        reported,
                        wins,
                        last_ms,
                        last_penalty: penalty,
                        connected: true,
                    })
                    .collect();
            }
            RoomMsg::RoundStart { round, seed } => {
                self.round = round;
                if seed != 0 {
                    self.seed = seed;
                }
                self.phase = RoomPhase::Racing;
            }
            RoomMsg::MatchEnd { winner, wins } => {
                self.phase = RoomPhase::MatchDone;
                self.winner = winner;
                for (id, w) in wins {
                    if let Some(p) = self.player_mut(id) {
                        p.wins = w;
                    }
                }
            }
            _ => {}
        }
    }
}


