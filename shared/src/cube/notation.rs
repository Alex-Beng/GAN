//! WCA move notation for a 3x3x3.
//!
//! Face indices follow the rest of the crate: `0..6 = U R F D L B`, matching
//! `protocol::apply_move`'s `MOVE_TABLES` order.
//!
//! Only outer face turns are supported (no wide/slice moves). That is enough
//! for both scramble display and solving-time bookkeeping.

use crate::protocol::{apply_move, CubeState};

pub const FACE_CHARS: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];

/// A single face turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    /// 0..6 = U R F D L B.
    pub face: u8,
    /// 1 = clockwise, 2 = half turn, 3 = counter-clockwise.
    pub turns: u8,
}

impl Move {
    pub fn new(face: u8, turns: u8) -> Self {
        Self {
            face: face % 6,
            turns: match turns {
                0 => 1,
                t => ((t - 1) % 4) + 1,
            },
        }
    }

    pub fn suffix(&self) -> &'static str {
        match self.turns {
            2 => "2",
            3 => "'",
            _ => "",
        }
    }

    /// Append `R`, `R2` or `R'` to `out`.
    pub fn write_to(&self, out: &mut String) {
        out.push(FACE_CHARS[(self.face % 6) as usize]);
        out.push_str(self.suffix());
    }

    /// Apply this move to `state`.
    pub fn apply(&self, state: &mut CubeState) {
        match self.turns {
            2 => {
                apply_move(state, self.face, 0);
                apply_move(state, self.face, 0);
            }
            3 => apply_move(state, self.face, 1),
            _ => apply_move(state, self.face, 0),
        }
    }
}

fn face_index(c: char) -> Option<u8> {
    match c.to_ascii_uppercase() {
        'U' => Some(0),
        'R' => Some(1),
        'F' => Some(2),
        'D' => Some(3),
        'L' => Some(4),
        'B' => Some(5),
        _ => None,
    }
}

/// Parse a whitespace separated notation string such as `"R U R' U2 F2"`.
///
/// Tolerates parentheses, commas and stray whitespace; unknown tokens are
/// skipped so a slightly malformed server-side scramble cannot poison a
/// session.
pub fn parse_notation(s: &str) -> Vec<Move> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(face) = face_index(c) {
            let mut turns = 1u8;
            match chars.peek().copied() {
                Some('\'') | Some('\u{2019}') => {
                    chars.next();
                    turns = 3;
                    if chars.peek() == Some(&'2') {
                        chars.next();
                    }
                }
                Some('2') => {
                    chars.next();
                    turns = 2;
                    if chars.peek().copied() == Some('\'') {
                        chars.next();
                    }
                }
                Some('3') => {
                    chars.next();
                    turns = 3;
                }
                _ => {}
            }
            out.push(Move { face, turns });
        }
    }
    out
}

/// Render moves back into a single-space separated notation string.
pub fn moves_to_string(moves: &[Move]) -> String {
    let mut s = String::with_capacity(moves.len() * 3);
    for (i, m) in moves.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        m.write_to(&mut s);
    }
    s
}

/// Parse and apply a notation string to `state`.
pub fn apply_notation(state: &mut CubeState, s: &str) {
    for m in parse_notation(s) {
        m.apply(state);
    }
}
