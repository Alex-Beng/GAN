//! Random scramble generation with WCA-style constraints.
//!
//! Deterministic PRNG on purpose: the firmware seeds it from the hardware RNG /
//! session counter and the test suite seeds it with a constant.

use super::notation::Move;

/// Small xorshift64* generator. Good enough for scrambles, needs no `rand`.
#[derive(Clone, Debug)]
pub struct XorShift64(u64);

impl XorShift64 {
    pub fn new(seed: u64) -> Self {
        // xorshift must not start at zero.
        Self(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform-ish value in `0..n` (`n` must be > 0).
    pub fn below(&mut self, n: u32) -> u32 {
        (self.next_u64() % n as u64) as u32
    }
}

/// Axis a face belongs to: U/D = 0, R/L = 1, F/B = 2.
fn axis(face: u8) -> u8 {
    match face {
        0 | 3 => 0,
        1 | 4 => 1,
        _ => 2,
    }
}

/// Generate a random scramble of `len` half-turns.
///
/// Constraints (same spirit as WCA random-move scrambles):
/// - a face never repeats back-to-back;
/// - three consecutive turns never share an axis (avoids `R L R`, which
///   collapses into a single-axis rotation and produces dull scrambles).
pub fn random_scramble(len: usize, rng: &mut XorShift64) -> Vec<Move> {
    let mut out: Vec<Move> = Vec::with_capacity(len);
    let mut guard = 0usize;
    while out.len() < len {
        guard += 1;
        // Bail out of the constraint loop rather than spinning forever.
        let relaxed = guard > len * 64;
        let face = rng.below(6) as u8;
        if !relaxed {
            if let Some(last) = out.last() {
                if last.face == face {
                    continue;
                }
                if out.len() >= 2 {
                    let prev = out[out.len() - 2];
                    if axis(prev.face) == axis(last.face)
                        && axis(last.face) == axis(face)
                    {
                        continue;
                    }
                }
            }
        }
        // turns: 1 = CW, 2 = half turn, 3 = CCW.
        let turns = 1 + rng.below(3) as u8;
        out.push(Move { face, turns });
    }
    out
}
