//! Cube primitives used by the watch firmware.
//!
//! These are thin layers on top of [`crate::protocol::CubeState`]: WCA move
//! notation parsing, scramble generation, solved detection and CFOP phase
//! segmentation. Everything here is protocol-agnostic and allocation-only (no
//! ESP-IDF types), so it can be exercised from the emu/proxy crates as well.

pub mod notation;
pub mod phase;
pub mod scramble;
pub mod solved;

pub use notation::{apply_notation, moves_to_string, parse_notation, Move};
pub use phase::{
    down_color, facelets_of, orientations, progress, slot_solved, Orientations, P_CROSS, P_F2L,
    P_NONE, P_OLL, P_SOLVED,
};
pub use scramble::{random_scramble, XorShift64};
pub use solved::{is_solved, is_solved_facelets};
