//! Timer practice records: the exact payload shape the `timer` gateway service
//! expects (`timer/time/practice/report` and `.../batch/report`).
//!
//! Field names are snake_case because the server deserializes
//! `GanTimerPart.ReportTimerDataReq` directly. Version constants mirror the
//! decompiled client (`version = 4`, `solution_type = 1`), while `down_color`
//! and the per-phase fields come from [`crate::timer::analyse`].

use crate::cube::Move;

/// Training type, mirrors `GanTimerPart.ExerciseType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupType {
    /// 1 鈥?鍩虹璁粌 (CFOP / ZB / ZZ / VRLS / EG / TCLL).
    Basic = 1,
    /// 2 = 专项训练 (F2L 标态).
    Special = 2,
    /// 3 鈥?鑷敱璁粌 (澶嶅師缁冧範).
    Free = 3,
}

impl GroupType {
    pub fn as_str(&self) -> &'static str {
        match self {
            GroupType::Basic => "1",
            GroupType::Special => "2",
            GroupType::Free => "3",
        }
    }
}

/// Cube type, mirrors the `cube_type` string in the timer group table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubeType {
    Three = 3,
    Two = 2,
}

impl CubeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CubeType::Three => "3",
            CubeType::Two => "2",
        }
    }
}

/// Default group ids per exercise/cube type (from `timer_groupdata`).
pub const GROUP_CFOP: u32 = 1002;
pub const GROUP_EG: u32 = 1101;
pub const GROUP_F2L_STD: u32 = 2001;
pub const GROUP_SOLVE_3: u32 = 3001;
pub const GROUP_SOLVE_2: u32 = 3002;

/// `time_status` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeStatus {
    Normal,
    Add2s,
    Dnf,
}

impl TimeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimeStatus::Normal => "Normal",
            TimeStatus::Add2s => "Add2s",
            TimeStatus::Dnf => "DNF",
        }
    }
}

/// DNF placeholder duration, same constant as the client (`GanTimerPart.DNFTIME`).
pub const DNF_TIME_MS: u32 = 300_000;

/// One completed practice attempt, ready to be JSON-encoded.
#[derive(Clone, Debug)]
pub struct TimerRecord {
    pub user_id: i64,
    pub cube_type: CubeType,
    pub group_type: GroupType,
    pub group_id: u32,
    pub group_disrupt: u32,
    pub library_formula_id: String,
    /// 1 = 鏅鸿兘榄旀柟, 2 = 闈炴櫤鑳?
    pub cube_smart: u8,
    /// Duration in milliseconds (or [`DNF_TIME_MS`] for DNF).
    pub time_ms: u32,
    pub time_status: TimeStatus,
    /// TPS in milli-units (`(int)(tps * 1000)` in the client).
    pub tps: u32,
    pub step: u32,
    pub fluency: u32,
    pub turn: u32,
    /// Scramble that was solved (WCA notation).
    pub disrupt: String,
    /// Per-move review blob (`MsgCubeReview` JSON, `""` when unrecorded).
    pub review: String,
    /// `MsgSolutionAnalyse` list JSON (`"[]"` when unrecorded).
    pub msg_solution_analyse: String,
    /// Packed cross orientation (`down * 10 + front`), from the analyser.
    pub down_color: u8,
    pub cube_model: String,
}

impl TimerRecord {
    pub fn version() -> u8 {
        4
    }
    pub fn solution_type() -> u8 {
        1
    }

    /// Convenience constructor with sensible defaults for a smart cube.
    pub fn new(
        user_id: i64,
        cube_type: CubeType,
        group_type: GroupType,
        group_id: u32,
        cube_smart: u8,
    ) -> Self {
        Self {
            user_id,
            cube_type,
            group_type,
            group_id,
            group_disrupt: 0,
            library_formula_id: String::new(),
            cube_smart,
            time_ms: 0,
            time_status: TimeStatus::Normal,
            tps: 0,
            step: 0,
            fluency: 0,
            turn: 0,
            disrupt: String::new(),
            review: String::new(),
            msg_solution_analyse: "[]".to_string(),
            down_color: 0,
            cube_model: String::new(),
        }
    }

    /// Fill `disrupt` from a generated scramble.
    pub fn with_scramble(mut self, scramble: &[Move]) -> Self {
        self.disrupt = crate::cube::moves_to_string(scramble);
        self
    }

    /// Serialize to the server's snake_case object.
    pub fn to_json(&self) -> String {
        let mut s = String::with_capacity(320 + self.review.len() + self.disrupt.len());
        s.push('{');
        field_i64(&mut s, "user_id", self.user_id, true);
        field_str(&mut s, "cube_type", self.cube_type.as_str(), false);
        field_str(&mut s, "group_type", self.group_type.as_str(), false);
        field_str(&mut s, "group_id", &self.group_id.to_string(), false);
        field_str(
            &mut s,
            "group_disrupt",
            &self.group_disrupt.to_string(),
            false,
        );
        field_str(&mut s, "library_formula_id", &self.library_formula_id, false);
        field_u32(&mut s, "cube_smart", self.cube_smart as u32, false);
        field_u32(&mut s, "time", self.time_ms, false);
        field_str(&mut s, "time_status", self.time_status.as_str(), false);
        field_u32(&mut s, "tps", self.tps, false);
        field_u32(&mut s, "step", self.step, false);
        field_u32(&mut s, "fluency", self.fluency, false);
        field_u32(&mut s, "turn", self.turn, false);
        field_str(&mut s, "disrupt", &self.disrupt, false);
        field_str(&mut s, "review", &self.review, false);
        // Analysis list is empty until a CFOP analyser exists on-device.
        let analysis: &str = if self.msg_solution_analyse.trim().is_empty() {
            "[]"
        } else {
            &self.msg_solution_analyse
        };
        field_raw(&mut s, "msg_solution_analyse", analysis, false);
        field_str(&mut s, "cube_model", &self.cube_model, false);
        field_u32(&mut s, "version", Self::version() as u32, false);
        field_u32(&mut s, "solution_type", Self::solution_type() as u32, false);
        field_u32(&mut s, "down_color", self.down_color as u32, false);
        s.push('}');
        s
    }
}

/// Encode `{"list":[record, ...]}` for `timer/time/practice/batch/report`.
pub fn batch_json(records: &[TimerRecord]) -> String {
    let mut s = String::with_capacity(16 + records.len() * 360);
    s.push_str("{\"list\":[");
    for (i, rec) in records.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&rec.to_json());
    }
    s.push_str("]}");
    s
}

/// Encode a bare array of records, for services that want `[...]`.
pub fn array_json(records: &[TimerRecord]) -> String {
    let mut s = String::with_capacity(8 + records.len() * 360);
    s.push('[');
    for (i, rec) in records.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&rec.to_json());
    }
    s.push(']');
    s
}

fn sep(out: &mut String, first: bool) {
    if !first {
        out.push(',');
    }
}

fn field_raw(out: &mut String, key: &str, raw: &str, first: bool) {
    sep(out, first);
    push_escaped(out, key);
    out.push(':');
    out.push_str(raw);
}

fn field_str(out: &mut String, key: &str, value: &str, first: bool) {
    sep(out, first);
    push_escaped(out, key);
    out.push(':');
    push_escaped(out, value);
}

fn field_u32(out: &mut String, key: &str, value: u32, first: bool) {
    sep(out, first);
    push_escaped(out, key);
    out.push(':');
    out.push_str(&value.to_string());
}

fn field_i64(out: &mut String, key: &str, value: i64, first: bool) {
    sep(out, first);
    push_escaped(out, key);
    out.push(':');
    out.push_str(&value.to_string());
}

/// Minimal JSON string escaping (`"` control chars and backslash).
pub fn push_escaped(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}



