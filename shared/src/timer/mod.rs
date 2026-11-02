//! Timer domain shared by the cubebox firmware and the desktop/gateway crates.

pub mod analyse;
pub mod record;
pub mod session;
pub mod stats;

pub use analyse::{analyse, review_json, to_json_array, PhaseStat, Solution, Step};
pub use record::{
    array_json, batch_json, CubeType, GroupType, TimeStatus, TimerRecord, DNF_TIME_MS, GROUP_EG,
    GROUP_CFOP, GROUP_F2L_STD, GROUP_SOLVE_2, GROUP_SOLVE_3,
};
pub use session::{Attempt, Penalty, Phase, TimerEvent, TimerSession};
pub use stats::{average, best, centis};
