//! Solution analysis: turn statistics and CFOP phase segmentation.
//!
//! Produces the two payloads the CubeStation client attaches to a practice
//! record:
//!
//! * `msg_solution_analyse` 鈥?one `MsgSolutionAnalyse` per segment (whole solve,
//!   cross/F2L/OLL/PLL, and the four F2L pairs). The client serializes these
//!   protobuf messages with Newtonsoft.Json, so the JSON keys are PascalCase.
//! * `review` 鈥?`MsgCubeReview` with the per-move list (`MUserMoveL`).
//!
//! Segmentation follows cstimer's `cubeutil.getProgress` model: replay the
//! solve, score each intermediate state with `progress()`, and cut where the
//! score drops.

use crate::cube::phase::{
    down_color, facelets_of, orientations, progress, slot_solved, Orientations, P_CROSS, P_F2L,
    P_OLL, P_SOLVED,
};
use crate::cube::Move;
use crate::protocol::CubeState;

/// One performed turn and the measured interval before it (ms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub mv: Move,
    pub interval_ms: u32,
}

/// Statistics for one analysed segment.
#[derive(Clone, Copy, Debug, Default)]
pub struct PhaseStat {
    /// `order` field: 0 = whole, 1..4 = CFOP phases, 21..24 = F2L pairs.
    pub order: u8,
    pub total_duration: u32,
    pub duration: u32,
    pub turn: u32,
    /// Turns per second 脳1000 (the unit of `ReportTimerDataReq.tps`).
    pub tps_milli: u32,
    pub rotate: u32,
    pub smooth_index: u32,
    pub start_step: u32,
    pub over_step: u32,
    pub observe_time: u32,
    pub auf_time: u32,
}

impl PhaseStat {
    pub fn tps(&self) -> f32 {
        self.tps_milli as f32 / 1000.0
    }
}

/// Full analysis of one solve.
#[derive(Clone, Debug, Default)]
pub struct Solution {
    /// Whole-solve statistics (`order = 0`).
    pub whole: PhaseStat,
    /// Cross / F2L / OLL / PLL (`order = 1..4`), possibly empty.
    pub phases: Vec<PhaseStat>,
    /// F2L pairs in completion order (`order = 21..24`), possibly empty.
    pub f2l_pairs: Vec<PhaseStat>,
    /// `(down, front)` faces packed as `down * 10 + front`.
    pub down_color: u8,
    pub steps: Vec<Step>,
}

/// Analyse `steps` performed from `start`.
///
/// `start` is the state the timer started from (i.e. the scrambled state) and
/// `total_ms` the measured solve time. Returns a solution with at least the
/// whole-solve statistics; phase entries are omitted if the solve does not look
/// like CFOP (e.g. a colour-neutral solve on an unsupported orientation).
pub fn analyse(start: &CubeState, steps: &[Step], total_ms: u32) -> Solution {
    let ori = orientations();
    let start_facelets = facelets_of(start);

    // Replay and snapshot each intermediate state.
    let mut states: Vec<[u8; 54]> = Vec::with_capacity(steps.len());
    let mut cube = start.clone();
    for s in steps {
        s.mv.apply(&mut cube);
        states.push(facelets_of(&cube));
    }

    // Pick the orientation whose cross/F2L/OLL/solved milestones are reached
    // earliest (cstimer evaluates the best orientation per state; picking one
    // for the whole solve keeps the phase boundaries consistent).
    let Some((orient, marks)) = pick_orientation(&start_facelets, &states, &ori) else {
        return Solution {
            whole: whole_stat(steps, total_ms),
            phases: Vec::new(),
            f2l_pairs: Vec::new(),
            down_color: 0,
            steps: steps.to_vec(),
        };
    };
    let [cross_end, f2l_end, oll_end, solved_end] = marks;

    let mut solution = Solution {
        whole: whole_stat(steps, total_ms),
        phases: Vec::new(),
        f2l_pairs: Vec::new(),
        down_color: down_color(orient, &ori),
        steps: steps.to_vec(),
    };

    // Step ranges are 1-based step indices: phase k covers [start, end].
    let ranges = [
        (1usize, cross_end),
        (cross_end + 1, f2l_end),
        (f2l_end + 1, oll_end),
        (oll_end + 1, solved_end),
    ];

    // --- phase durations (last phase absorbs the rounding) -----------------
    let mut sum = 0u32;
    for (k, &(from, to)) in ranges.iter().enumerate() {
        if from > to {
            continue;
        }
        let idx = step_indices(from, to);
        let mut stat = stat_for(steps, &idx, total_ms, None);
        stat.order = (k + 1) as u8;
        stat.total_duration = total_ms;
        stat.observe_time = if from > 1 { steps[from - 1].interval_ms } else { 0 };
        if k == 3 {
            // PLL: report the trailing AUF move when it is a U turn.
            if let Some(last) = steps.get(to - 1) {
                if last.mv.face == 0 {
                    stat.auf_time = last.interval_ms;
                }
            }
            stat.duration = total_ms.saturating_sub(sum);
        } else {
            sum = sum.saturating_add(stat.duration);
        }
        solution.phases.push(stat);
    }

    // --- F2L pairs in completion order -------------------------------------
    solution.f2l_pairs = f2l_pairs(steps, &states, orient, &ori, cross_end, f2l_end);

    solution
}

fn step_indices(from: usize, to: usize) -> Vec<usize> {
    (from..=to).map(|i| i - 1).collect()
}

/// First step index (1-based) where the progress reaches `level` or better.
fn first_at_or_below(prog: &[u8], level: u8) -> Option<usize> {
    (1..prog.len()).find(|&i| prog[i] <= level)
}

/// Choose the cross orientation and the four step marks
/// `[cross, f2l, oll, solved]`.
///
/// An orientation is only considered when the cross is *not* already solved at
/// the timer start (progress `P_CROSS` or better at step 0), and when all four
/// milestones occur in order. Among the candidates the one reaching the cross
/// (then F2L, OLL, solved) earliest wins.
fn pick_orientation(
    start: &[u8; 54],
    states: &[[u8; 54]],
    ori: &Orientations,
) -> Option<(usize, [usize; 4])> {
    let mut best: Option<(usize, [usize; 4])> = None;
    for o in 0..6 {
        let mut prog = Vec::with_capacity(states.len() + 1);
        prog.push(progress(start, o, ori));
        for st in states {
            prog.push(progress(st, o, ori));
        }
        if prog[0] <= P_CROSS {
            continue;
        }
        let mut marks = [0usize; 4];
        let mut ok = true;
        for (k, level) in [P_CROSS, P_F2L, P_OLL, P_SOLVED].into_iter().enumerate() {
            match first_at_or_below(&prog, level) {
                Some(i) if k == 0 || i >= marks[k - 1] => marks[k] = i,
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        if best.map_or(true, |(_, b)| marks < b) {
            best = Some((o, marks));
        }
    }
    best
}

fn whole_stat(steps: &[Step], total_ms: u32) -> PhaseStat {
    let idx: Vec<usize> = (0..steps.len()).collect();
    let mut stat = stat_for(steps, &idx, total_ms, Some(total_ms));
    stat.order = 0;
    stat.total_duration = total_ms;
    stat.duration = total_ms;
    stat.start_step = 0;
    stat.over_step = 0;
    stat
}

/// Statistics over `moves` (indices into `steps`).
///
/// `hard_total` reproduces the client's scaling of sub-phase durations: raw
/// interval sums are scaled so they add up to the parent phase duration.
fn stat_for(
    steps: &[Step],
    idx: &[usize],
    total_ms: u32,
    duration: Option<u32>,
) -> PhaseStat {
    let mut turn = 0u32;
    let mut raw = 0u32;
    let mut samples: Vec<f32> = Vec::with_capacity(idx.len());
    for (n, &i) in idx.iter().enumerate() {
        let s = steps[i];
        turn += 1;
        raw = raw.saturating_add(s.interval_ms);
        // The client drops the first move's interval (inspection time).
        if n == 0 {
            samples.push(0.0);
        } else {
            samples.push(s.interval_ms as f32 / 1000.0);
        }
    }

    let dur = duration.unwrap_or(raw);
    let tps_milli = if dur > 0 {
        let tps10 = ((10000u64 * turn as u64) / dur as u64) as u32;
        if tps10 >= 9000 {
            888_000
        } else {
            tps10 * 100
        }
    } else {
        0
    };

    PhaseStat {
        order: 0,
        total_duration: total_ms,
        duration: dur,
        turn,
        tps_milli,
        rotate: 0,
        smooth_index: fluency(&samples),
        start_step: idx.first().map(|&i| (i + 1) as u32).unwrap_or(0),
        over_step: idx.last().map(|&i| (i + 1) as u32).unwrap_or(0),
        observe_time: 0,
        auf_time: 0,
    }
}

/// cstimer's fluency metric: `floor(100 * exp(-1.11571775657105 * stddev))`.
fn fluency(samples: &[f32]) -> u32 {
    if samples.len() <= 2 {
        return 0;
    }
    let n = samples.len() as f32;
    let mean = samples.iter().sum::<f32>() / n;
    let var = samples.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
    let sd = var.sqrt();
    let v = 100.0 * (-1.115_717_8f32 * sd).exp();
    if v.is_finite() {
        v.max(0.0) as u32
    } else {
        0
    }
}

fn f2l_pairs(
    steps: &[Step],
    states: &[[u8; 54]],
    orient: usize,
    ori: &Orientations,
    cross_end: usize,
    f2l_end: usize,
) -> Vec<PhaseStat> {
    if f2l_end <= cross_end {
        return Vec::new();
    }

    // First step index (1-based) at which each slot is complete.
    let mut done: Vec<Option<u32>> = vec![None; 4];
    for i in cross_end..=f2l_end {
        if i == 0 || i > states.len() {
            continue;
        }
        let st = &states[i - 1];
        for slot in 0..4 {
            if done[slot].is_none() && slot_solved(st, orient, slot, ori) {
                done[slot] = Some(i as u32);
            }
        }
    }

    let mut order: Vec<(u32, usize)> = done
        .iter()
        .enumerate()
        .filter_map(|(slot, at)| at.map(|at| (at, slot)))
        .collect();
    order.sort_unstable();

    // Raw durations, then scale so they sum to the F2L phase duration.
    let f2l_from = cross_end + 1;
    let f2l_ms: u32 = steps[(f2l_from - 1)..f2l_end]
        .iter()
        .map(|s| s.interval_ms)
        .sum();
    let mut raw_pairs: Vec<(usize, usize, u32)> = Vec::new();
    let mut prev = cross_end;
    for (at, _slot) in order.iter().copied() {
        let from = prev + 1;
        let to = at as usize;
        let raw: u32 = if from <= to {
            steps[(from - 1)..to].iter().map(|s| s.interval_ms).sum()
        } else {
            0
        };
        raw_pairs.push((from, to, raw));
        prev = to;
    }
    let raw_total: u32 = raw_pairs.iter().map(|(_, _, r)| *r).sum();

    let mut out = Vec::new();
    let mut acc = 0u32;
    let count = raw_pairs.len();
    for (k, (from, to, raw)) in raw_pairs.into_iter().enumerate() {
        let idx = step_indices(from, to);
        let scaled = if raw_total > 0 {
            // Scale into the parent phase, keeping the last pair exact.
            if k + 1 == count {
                f2l_ms.saturating_sub(acc)
            } else {
                ((raw as u64 * f2l_ms as u64) / raw_total as u64) as u32
            }
        } else {
            0
        };
        acc = acc.saturating_add(scaled);
        let mut stat = stat_for(steps, &idx, f2l_ms, Some(scaled));
        stat.order = 21 + k as u8;
        stat.total_duration = f2l_ms;
        out.push(stat);
    }
    out
}

// ---------------------------------------------------------------- JSON

/// Serialize the analysis list (`msg_solution_analyse`).
///
/// PascalCase keys because the client serializes protobuf messages with
/// Newtonsoft.Json, which uses the generated C# property names verbatim.
pub fn to_json_array(sol: &Solution) -> String {
    let mut s = String::with_capacity(512);
    s.push('[');
    push_stat(&mut s, &sol.whole, true);
    for p in sol.phases.iter().chain(sol.f2l_pairs.iter()) {
        s.push(',');
        push_stat(&mut s, p, false);
    }
    s.push(']');
    s
}

fn push_stat(out: &mut String, p: &PhaseStat, first: bool) {
    let _ = first;
    out.push_str(&format!(
        "{{\"Type\":1,\"Order\":{},\"TotalDuration\":{},\"Duration\":{},\"Turn\":{},\"Tps\":{:.1},\"Rotate\":{},\"SmoothIndex\":{},\"ObserveTime\":{},\"AdjustTime\":0,\"ExecuteTime\":0,\"AufTime\":{},\"StartStep\":{},\"OverStep\":{}}}",
        p.order,
        p.total_duration,
        p.duration,
        p.turn,
        p.tps(),
        p.rotate,
        p.smooth_index,
        p.observe_time,
        p.auf_time,
        p.start_step,
        p.over_step,
    ));
}

/// Serialize the review blob (`MsgCubeReview`), which only carries the user's
/// move list in the client's upload path.
pub fn review_json(steps: &[Step]) -> String {
    let mut s = String::with_capacity(64 + steps.len() * 96);
    s.push_str("{\"MUserMoveL\":[");
    for (i, st) in steps.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let mut notation = String::new();
        st.mv.write_to(&mut notation);
        let interval = if i == 0 { 0 } else { st.interval_ms };
        s.push_str(&format!(
            "{{\"MAngle\":{{\"X\":0,\"Y\":0,\"Z\":0}},\"SMove\":\"{}\",\"IInterval\":{},\"IStep\":{},\"IRotate\":0,\"IModify\":0,\"PlaneStr\":null}}",
            notation,
            interval,
            i + 1
        ));
    }
    s.push_str("],\"MMoveL\":[],\"MSolutionL\":[]}");
    s
}


