//! CFOP phase detection from facelet snapshots.
//!
//! This is a port of cstimer's `lib/cubeutil.js` progress model: instead of
//! solving anything, every intermediate state is scored by how many CFOP steps
//! are already complete. Replaying the user's moves and watching that score
//! drop yields the phase boundaries; the same segmentation the CubeStation
//! client derives from its solver (`_solution` / `_subSolution`).
//!
//! The masks are colour-agnostic equality groups, and a solve may use any
//! cross colour, so the score is evaluated over six whole-cube orientations
//! (one per face on D) and the best-scoring orientation is used.

use crate::protocol::CubeState;

/// A group of facelet indices that must share one colour.
type Groups<'a> = &'a [&'a [usize]];

const MASK_CROSS: Groups = &[
    &[13, 16],
    &[22, 25],
    &[28, 30, 31, 32, 34],
    &[40, 43],
    &[49, 52],
];

const MASK_F2L: Groups = &[
    &[12, 13, 14, 15, 16, 17],
    &[21, 22, 23, 24, 25, 26],
    &[27, 28, 29, 30, 31, 32, 33, 34, 35],
    &[39, 40, 41, 42, 43, 44],
    &[48, 49, 50, 51, 52, 53],
];

/// F2L slots in cstimer's order (FR, FL, BL, BR).
const MASK_F2L_SLOTS: [Groups; 4] = [
    &[
        &[12, 13, 15, 16],
        &[22, 23, 25, 26],
        &[28, 29, 30, 31, 32, 34],
        &[40, 43],
        &[49, 52],
    ],
    &[
        &[13, 16],
        &[21, 22, 24, 25],
        &[27, 28, 30, 31, 32, 34],
        &[40, 41, 43, 44],
        &[49, 52],
    ],
    &[
        &[13, 14, 16, 17],
        &[22, 25],
        &[28, 30, 31, 32, 34, 35],
        &[40, 43],
        &[48, 49, 51, 52],
    ],
    &[
        &[13, 16],
        &[22, 25],
        &[28, 30, 31, 32, 33, 34],
        &[39, 40, 42, 43],
        &[49, 50, 52, 53],
    ],
];

const MASK_OLL: Groups = &[
    &[0, 1, 2, 3, 4, 5, 6, 7, 8],
    &[12, 13, 14, 15, 16, 17],
    &[21, 22, 23, 24, 25, 26],
    &[27, 28, 29, 30, 31, 32, 33, 34, 35],
    &[39, 40, 41, 42, 43, 44],
    &[48, 49, 50, 51, 52, 53],
];

/// Every face must be uniform; orientation-independent "solved" test.
const MASK_FACES: Groups = &[
    &[0, 1, 2, 3, 4, 5, 6, 7, 8],
    &[9, 10, 11, 12, 13, 14, 15, 16, 17],
    &[18, 19, 20, 21, 22, 23, 24, 25, 26],
    &[27, 28, 29, 30, 31, 32, 33, 34, 35],
    &[36, 37, 38, 39, 40, 41, 42, 43, 44],
    &[45, 46, 47, 48, 49, 50, 51, 52, 53],
];

/// CFOP progress levels, matching cstimer (higher = less progress).
pub const P_NONE: u8 = 4;
pub const P_CROSS: u8 = 3;
pub const P_F2L: u8 = 2;
pub const P_OLL: u8 = 1;
pub const P_SOLVED: u8 = 0;

type Geom = ([i8; 3], [i8; 3]);
type Mat = [[i8; 3]; 3];

/// 3D layout of sticker `j`: (position, outward normal).
///
/// `face = j / 9`, `row = (j % 9) / 3`, `col = j % 3`, URFDLB reading order.
/// Unit-tested against `protocol`'s `CORNER_FACELET` / `CORNER_COLOR` tables.
const fn geom(j: usize) -> Geom {
    let f = (j / 9) as i8;
    let r = (((j % 9) / 3) as i8) - 1;
    let c = ((j % 3) as i8) - 1;
    match f {
        0 => ([c, 1, r], [0, 1, 0]),
        1 => ([1, -r, -c], [1, 0, 0]),
        2 => ([c, -r, 1], [0, 0, 1]),
        3 => ([c, -1, -r], [0, -1, 0]),
        4 => ([-1, -r, -c], [-1, 0, 0]),
        _ => ([-c, -r, -1], [0, 0, -1]),
    }
}

fn normal_face(n: [i8; 3]) -> u32 {
    match n {
        [0, 1, 0] => 0,
        [1, 0, 0] => 1,
        [0, 0, 1] => 2,
        [0, -1, 0] => 3,
        [-1, 0, 0] => 4,
        _ => 5,
    }
}

fn key(facelet_geom: Geom) -> u32 {
    let (p, n) = facelet_geom;
    let pos = ((p[0] as i32 + 1) * 9 + (p[1] as i32 + 1) * 3 + (p[2] as i32 + 1)) as u32;
    pos * 8 + normal_face(n)
}

fn mat_mul(m: Mat, v: [i8; 3]) -> [i8; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn mat_transpose(m: Mat) -> Mat {
    [
        [m[0][0], m[1][0], m[2][0]],
        [m[0][1], m[1][1], m[2][1]],
        [m[0][2], m[1][2], m[2][2]],
    ]
}

fn det3(m: &Mat) -> i8 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// The 24 rotational symmetries of the cube (signed permutations, det +1).
fn rotations() -> [Mat; 24] {
    let mut out = [[[0i8; 3]; 3]; 24];
    let mut n = 0;
    const PERMS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for perm in PERMS {
        for sign_bits in 0..8u8 {
            let signs = [
                if sign_bits & 1 == 0 { 1i8 } else { -1 },
                if sign_bits & 2 == 0 { 1 } else { -1 },
                if sign_bits & 4 == 0 { 1 } else { -1 },
            ];
            let mut m = [[0i8; 3]; 3];
            for a in 0..3 {
                m[a][perm[a]] = signs[a];
            }
            if det3(&m) == 1 {
                out[n] = m;
                n += 1;
            }
        }
    }
    debug_assert_eq!(n, 24);
    out
}

/// Precomputed orientation data.
pub struct Orientations {
    /// `perm[j]` is the source facelet for destination `j`:
    /// `rotated[j] = original[perm[j]]`.
    pub perms: [[u8; 54]; 6],
    /// `(down_face, front_face)` for each orientation.
    pub faces: [(u8, u8); 6],
}

/// Build the six "face X on D" permutations (cheap; cache per solve).
pub fn orientations() -> Orientations {
    let rots = rotations();
    let geom_keys: [u32; 54] = core::array::from_fn(|j| key(geom(j)));

    let mut perms = [[0u8; 54]; 6];
    let mut faces = [(3u8, 2u8); 6];

    for face in 0..6usize {
        let normal = geom(face * 9 + 4).1;
        let m = *rots
            .iter()
            .find(|r| mat_mul(**r, normal) == [0, -1, 0])
            .expect("rotation onto D");
        let mt = mat_transpose(m);

        for j in 0..54 {
            let (p, n) = geom(j);
            let want = key((mat_mul(mt, p), mat_mul(mt, n)));
            perms[face][j] = geom_keys
                .iter()
                .position(|&k| k == want)
                .unwrap_or(j) as u8;
        }

        // Which original faces end up on D and F.
        let mut down = 3u8;
        let mut front = 2u8;
        for f in 0..6u8 {
            let n = geom((f as usize) * 9 + 4).1;
            let w = mat_mul(m, n);
            if w == [0, -1, 0] {
                down = f;
            } else if w == [0, 0, 1] {
                front = f;
            }
        }
        faces[face] = (down, front);
    }

    Orientations { perms, faces }
}

fn mask_solved(facelets: &[u8; 54], perm: &[u8; 54], mask: Groups) -> bool {
    for group in mask {
        let first = facelets[perm[group[0]] as usize];
        for &idx in &group[1..] {
            if facelets[perm[idx] as usize] != first {
                return false;
            }
        }
    }
    true
}

/// cstimer's `getCFOPProgress` for one orientation.
pub fn progress(facelets: &[u8; 54], orient: usize, ori: &Orientations) -> u8 {
    let perm = &ori.perms[orient];
    if !mask_solved(facelets, perm, MASK_CROSS) {
        P_NONE
    } else if !mask_solved(facelets, perm, MASK_F2L) {
        P_CROSS
    } else if !mask_solved(facelets, perm, MASK_OLL) {
        P_F2L
    } else if !mask_solved(facelets, perm, MASK_FACES) {
        P_OLL
    } else {
        P_SOLVED
    }
}

/// True when the F2L slot `slot` (0..4) is complete under `orient`.
pub fn slot_solved(facelets: &[u8; 54], orient: usize, slot: usize, ori: &Orientations) -> bool {
    mask_solved(facelets, &ori.perms[orient], MASK_F2L_SLOTS[slot])
}

/// Cross colour pair `(down, front)` of the selected orientation.
pub fn down_color(orient: usize, ori: &Orientations) -> u8 {
    let (d, f) = ori.faces[orient];
    d * 10 + f
}

/// Facelet snapshot (0..6 = U R F D L B) for a cube state.
pub fn facelets_of(state: &CubeState) -> [u8; 54] {
    crate::protocol::to_facelet_indices(state)
}




