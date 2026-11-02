//! Timing state machine: scramble -> ready -> solving -> finished.
//!
//! The session works against a *base* state (the state the user must return to)
//! rather than assuming the cube is solved when the session starts. That keeps
//! things correct when a cube is connected mid-solve or the user re-uses a
//! state, and it degrades gracefully to the normal "scramble a solved cube"
//! flow because `base` is then simply the solved state.

use crate::cube::{moves_to_string, Move};
use crate::timer::analyse::Step;
use crate::protocol::CubeState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for the user to apply the scramble.
    Scrambling,
    /// Scramble applied; waiting for the first solving move.
    Ready,
    /// Solving in progress.
    Solving,
    /// Solved; result available.
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimerEvent {
    /// Scramble has been applied, the timer is armed.
    Ready,
    /// First move after `Ready`; wall-clock start recorded.
    Started,
    /// Cube returned to the base state.
    Finished { duration_ms: u32 },
}

/// Snapshot of a finished attempt.
#[derive(Clone, Debug)]
pub struct Attempt {
    pub scramble: Vec<Move>,
    /// Final duration with penalties applied (what gets uploaded).
    pub duration_ms: u32,
    /// Measured duration before any penalty (what the UI lists).
    pub raw_ms: u32,
    pub move_count: u32,
    pub dnf: bool,
    /// Per-move records with measured intervals, for the phase analyser.
    pub steps: Vec<Step>,
    /// Cube state the timer started from (the scrambled state).
    pub start: CubeState,
}

pub struct TimerSession {
    /// State the user must return to.
    pub base: CubeState,
    /// State produced by applying `scramble` to `base`.
    pub target: CubeState,
    pub scramble: Vec<Move>,
    pub phase: Phase,
    pub started_ms: Option<u64>,
    pub finished_ms: Option<u64>,
    pub move_count: u32,
    /// Moves performed since the timer started, with intervals.
    pub moves: Vec<Step>,
    last_move_ms: Option<u64>,
    /// Set when the attempt was marked DNF/+2 by the UI.
    pub penalty: Penalty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Penalty {
    None,
    PlusTwo,
    Dnf,
}

impl TimerSession {
    /// Create a session that scrambles `base` with `scramble` and expects the
    /// user to return to `base`.
    pub fn new(base: CubeState, scramble: Vec<Move>) -> Self {
        let mut target = base.clone();
        for m in &scramble {
            m.apply(&mut target);
        }
        Self {
            base,
            target,
            scramble,
            phase: Phase::Scrambling,
            started_ms: None,
            finished_ms: None,
            move_count: 0,
            moves: Vec::new(),
            last_move_ms: None,
            penalty: Penalty::None,
        }
    }

    pub fn scramble_text(&self) -> String {
        moves_to_string(&self.scramble)
    }

    /// Feed the latest live cube state. Returns an event on transitions only.
    pub fn on_state(&mut self, live: &CubeState, now_ms: u64) -> Option<TimerEvent> {
        match self.phase {
            Phase::Scrambling => {
                if state_eq(live, &self.target) {
                    self.phase = Phase::Ready;
                    Some(TimerEvent::Ready)
                } else {
                    None
                }
            }
            Phase::Ready => {
                if !state_eq(live, &self.target) {
                    self.phase = Phase::Solving;
                    self.started_ms = Some(now_ms);
                    self.move_count = 0;
                    self.moves.clear();
                    self.last_move_ms = None;
                    // A one-move scramble/solve can reach the goal immediately.
                    if state_eq(live, &self.base) {
                        self.phase = Phase::Done;
                        self.finished_ms = Some(now_ms);
                        return Some(TimerEvent::Finished { duration_ms: 0 });
                    }
                    Some(TimerEvent::Started)
                } else {
                    None
                }
            }
            Phase::Solving => {
                if state_eq(live, &self.base) {
                    self.phase = Phase::Done;
                    self.finished_ms = Some(now_ms);
                    let start = self.started_ms.unwrap_or(now_ms);
                    let duration_ms = now_ms.saturating_sub(start).min(u32::MAX as u64) as u32;
                    Some(TimerEvent::Finished { duration_ms })
                } else {
                    None
                }
            }
            Phase::Done => None,
        }
    }

    /// Record an observed physical turn with the moment it was seen. Only
    /// counted while solving; the interval is used by the phase analyser.
    pub fn on_move(&mut self, m: Move, now_ms: u64) {
        if self.phase == Phase::Solving {
            let interval_ms = match self.last_move_ms {
                Some(prev) => now_ms
                    .saturating_sub(prev)
                    .min(u16::MAX as u64) as u32,
                None => 0,
            };
            self.last_move_ms = Some(now_ms);
            self.move_count = self.move_count.saturating_add(1);
            // Cap the retained move list; a runaway session must not exhaust RAM.
            if self.moves.len() < 512 {
                self.moves.push(Step { mv: m, interval_ms });
            }
        }
    }

    /// Total milliseconds including the current penalty.
    pub fn effective_duration_ms(&self) -> u32 {
        let raw = self.raw_duration_ms();
        match self.penalty {
            Penalty::Dnf => crate::timer::record::DNF_TIME_MS,
            Penalty::PlusTwo => raw.saturating_add(2000),
            Penalty::None => raw,
        }
    }

    /// Measured milliseconds with no penalty applied.
    pub fn raw_duration_ms(&self) -> u32 {
        match (self.started_ms, self.finished_ms) {
            (Some(a), Some(b)) => b.saturating_sub(a).min(u32::MAX as u64) as u32,
            (Some(a), None) => now_placeholder(a),
            _ => 0,
        }
    }

    /// Build an [`Attempt`] from a finished session.
    pub fn attempt(&self) -> Option<Attempt> {
        if self.phase != Phase::Done {
            return None;
        }
        Some(Attempt {
            scramble: self.scramble.clone(),
            duration_ms: self.effective_duration_ms(),
            raw_ms: self.raw_duration_ms(),
            move_count: self.move_count,
            dnf: self.penalty == Penalty::Dnf,
            steps: self.moves.clone(),
            start: self.target.clone(),
        })
    }

    /// True when the timer is armed and waiting for the first move.
    pub fn is_armed(&self) -> bool {
        self.phase == Phase::Ready
    }

    /// Live elapsed milliseconds (0 when not started).
    pub fn elapsed_ms(&self, now_ms: u64) -> u32 {
        match (self.phase, self.started_ms) {
            (Phase::Solving, Some(start)) => now_ms.saturating_sub(start).min(u32::MAX as u64) as u32,
            (Phase::Done, Some(start)) => self
                .finished_ms
                .unwrap_or(now_ms)
                .saturating_sub(start)
                .min(u32::MAX as u64) as u32,
            _ => 0,
        }
    }
}

fn state_eq(a: &CubeState, b: &CubeState) -> bool {
    a.cp == b.cp && a.co == b.co && a.ep == b.ep && a.eo == b.eo
}

/// Unused placeholder kept out of the hot path: a running session has no
/// `finished_ms`, and `raw_duration_ms` is only meaningful once finished.
fn now_placeholder(_started: u64) -> u32 {
    0
}


