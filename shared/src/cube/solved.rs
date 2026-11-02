//! Solved detection.

use crate::protocol::CubeState;

/// True when every corner/edge is in place and correctly oriented.
///
/// This is stricter than "the cube looks solved": a whole-cube rotation would
/// leave `cp`/`ep` permuted, so the cubebox treats it as unsolved. For a timer
/// that is the correct call (the user must actually undo the scramble).
pub fn is_solved(state: &CubeState) -> bool {
    for (i, &v) in state.cp.iter().enumerate() {
        if v as usize != i {
            return false;
        }
    }
    for (i, &v) in state.ep.iter().enumerate() {
        if v as usize != i {
            return false;
        }
    }
    state.co.iter().all(|&v| v % 3 == 0) && state.eo.iter().all(|&v| v % 2 == 0)
}

/// Check a 54-sticker snapshot (0..5 = U R F D L B) for a solved cube.
///
/// Cheap alternative to decoding a full `CubeState` when a protocol only sends
/// facelets: each face must be uniform and show its own colour.
pub fn is_solved_facelets(facelets: &[u8; 54]) -> bool {
    for face in 0..6usize {
        let base = face * 9;
        let colour = facelets[base];
        if colour as usize != face {
            return false;
        }
        for slot in 0..9 {
            if facelets[base + slot] != colour {
                return false;
            }
        }
    }
    true
}
