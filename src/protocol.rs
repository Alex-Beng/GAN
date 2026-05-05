pub struct BitWriter {
    buf: [u8; 20],
    bit_pos: usize,
}

impl BitWriter {
    pub fn new() -> Self {
        Self { buf: [0u8; 20], bit_pos: 0 }
    }

    pub fn write_bits(&mut self, value: u32, count: usize) {
        for i in (0..count).rev() {
            let bit = (value >> i) & 1;
            let byte_idx = self.bit_pos / 8;
            let bit_idx = 7 - (self.bit_pos % 8);
            self.buf[byte_idx] |= (bit as u8) << bit_idx;
            self.bit_pos += 1;
        }
    }

    pub fn finish(self) -> [u8; 20] {
        self.buf
    }
}

pub struct BitReader<'a> {
    buf: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, bit_pos: 0 }
    }

    pub fn read_bits(&mut self, count: usize) -> u32 {
        let mut value = 0u32;
        for _ in 0..count {
            let byte_idx = self.bit_pos / 8;
            let bit_idx = 7 - (self.bit_pos % 8);
            let bit = (self.buf[byte_idx] >> bit_idx) & 1;
            value = (value << 1) | bit as u32;
            self.bit_pos += 1;
        }
        value
    }
}

#[derive(Clone)]
pub struct CubeState {
    pub cp: [u8; 8],
    pub co: [u8; 8],
    pub ep: [u8; 12],
    pub eo: [u8; 12],
}

impl CubeState {
    pub fn solved() -> Self {
        Self {
            cp: [0, 1, 2, 3, 4, 5, 6, 7],
            co: [0u8; 8],
            ep: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
            eo: [0u8; 12],
        }
    }
}

struct MoveTable {
    cp: [u8; 8],
    co: [u8; 8],
    ep: [u8; 12],
    eo: [u8; 12],
}

static MOVE_TABLES: [MoveTable; 6] = [
    MoveTable {
        cp: [3, 0, 1, 2, 4, 5, 6, 7],
        co: [0, 0, 0, 0, 0, 0, 0, 0],
        ep: [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11],
        eo: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    },
    MoveTable {
        cp: [4, 1, 2, 0, 7, 5, 6, 3],
        co: [2, 0, 0, 1, 1, 0, 0, 2],
        ep: [8, 1, 2, 3, 11, 5, 6, 7, 4, 9, 10, 0],
        eo: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    },
    MoveTable {
        cp: [1, 5, 2, 3, 0, 4, 6, 7],
        co: [1, 2, 0, 0, 2, 1, 0, 0],
        ep: [0, 9, 2, 3, 4, 8, 6, 7, 1, 5, 10, 11],
        eo: [0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0],
    },
    MoveTable {
        cp: [0, 1, 2, 3, 5, 6, 7, 4],
        co: [0, 0, 0, 0, 0, 0, 0, 0],
        ep: [0, 1, 2, 3, 5, 6, 7, 4, 8, 9, 10, 11],
        eo: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    },
    MoveTable {
        cp: [0, 2, 6, 3, 4, 1, 5, 7],
        co: [0, 1, 2, 0, 0, 2, 1, 0],
        ep: [0, 1, 10, 3, 4, 5, 9, 7, 8, 2, 6, 11],
        eo: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    },
    MoveTable {
        cp: [0, 1, 3, 7, 4, 5, 2, 6],
        co: [0, 0, 1, 2, 0, 0, 2, 1],
        ep: [0, 1, 2, 11, 4, 5, 6, 10, 8, 9, 3, 7],
        eo: [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1],
    },
];

fn apply_cw(state: &mut CubeState, face: usize) {
    let t = &MOVE_TABLES[face];
    let old_cp = state.cp;
    let old_co = state.co;
    let old_ep = state.ep;
    let old_eo = state.eo;
    for i in 0..8 {
        let src = t.cp[i] as usize;
        state.cp[i] = old_cp[src];
        state.co[i] = (old_co[src] + t.co[i]) % 3;
    }
    for i in 0..12 {
        let src = t.ep[i] as usize;
        state.ep[i] = old_ep[src];
        state.eo[i] = (old_eo[src] + t.eo[i]) % 2;
    }
}

pub fn apply_move(state: &mut CubeState, face: u8, dir: u8) {
    let face = face as usize;
    let times = if dir == 0 { 1 } else { 3 };
    for _ in 0..times {
        apply_cw(state, face);
    }
}

pub fn build_facelets(serial: u8, state: &CubeState) -> [u8; 20] {
    let mut w = BitWriter::new();
    w.write_bits(0x04, 4);
    w.write_bits(serial as u32, 8);
    for i in 0..7 { w.write_bits(state.cp[i] as u32, 3); }
    for i in 0..7 { w.write_bits(state.co[i] as u32, 2); }
    for i in 0..11 { w.write_bits(state.ep[i] as u32, 4); }
    for i in 0..11 { w.write_bits(state.eo[i] as u32, 1); }
    w.finish()
}

pub fn build_hardware(hw_major: u8, hw_minor: u8, sw_major: u8, sw_minor: u8, name: &[u8; 8], gyro: bool) -> [u8; 20] {
    let mut w = BitWriter::new();
    w.write_bits(0x05, 4);
    w.write_bits(0, 4);
    w.write_bits(hw_major as u32, 8);
    w.write_bits(hw_minor as u32, 8);
    w.write_bits(sw_major as u32, 8);
    w.write_bits(sw_minor as u32, 8);
    for b in name.iter() {
        w.write_bits(*b as u32, 8);
    }
    w.write_bits(if gyro { 1 } else { 0 }, 1);
    w.finish()
}

pub fn build_battery(level: u8) -> [u8; 20] {
    let mut w = BitWriter::new();
    w.write_bits(0x09, 4);
    w.write_bits(0, 4);
    w.write_bits(level.min(100) as u32, 8);
    w.finish()
}

pub fn build_move(serial: u8, history: &[(u8, u8, u16)]) -> [u8; 20] {
    let mut w = BitWriter::new();
    w.write_bits(0x02, 4);
    w.write_bits(serial as u32, 8);
    for i in 0..7 {
        if let Some(&(face, dir, _)) = history.get(i) {
            w.write_bits(face as u32, 4);
            w.write_bits(dir as u32, 1);
        } else {
            w.write_bits(0, 4);
            w.write_bits(0, 1);
        }
    }
    for i in 0..7 {
        if let Some(&(_, _, elapsed)) = history.get(i) {
            w.write_bits(elapsed as u32, 16);
        } else {
            w.write_bits(0, 16);
        }
    }
    w.finish()
}

pub fn parse_command_type(data: &[u8; 20]) -> u8 {
    data[0]
}

const CORNER_FACELET: [[usize; 3]; 8] = [
    [8, 9, 20],
    [6, 18, 38],
    [0, 36, 47],
    [2, 45, 11],
    [29, 26, 15],
    [27, 44, 24],
    [33, 53, 42],
    [35, 17, 51],
];

const CORNER_COLOR: [[u8; 3]; 8] = [
    [0, 1, 2],
    [0, 2, 4],
    [0, 4, 5],
    [0, 5, 1],
    [3, 2, 1],
    [3, 4, 2],
    [3, 5, 4],
    [3, 1, 5],
];

const EDGE_FACELET: [[usize; 2]; 12] = [
    [5, 10],
    [7, 19],
    [3, 37],
    [1, 46],
    [32, 16],
    [28, 25],
    [30, 43],
    [34, 52],
    [23, 12],
    [21, 41],
    [50, 39],
    [48, 14],
];

const CENTER_FACELET: [usize; 6] = [4, 13, 22, 31, 40, 49];

const EDGE_COLOR: [[u8; 2]; 12] = [
    [0, 1],
    [0, 2],
    [0, 4],
    [0, 5],
    [3, 1],
    [3, 2],
    [3, 4],
    [3, 5],
    [2, 1],
    [2, 4],
    [5, 4],
    [5, 1],
];


pub fn to_facelet_string(state: &CubeState) -> [u8; 54] {
    let color_chars = [b'U', b'R', b'F', b'D', b'L', b'B'];
    let mut f = [b'U'; 54];
    for i in 0..6 {
        f[CENTER_FACELET[i]] = color_chars[i];
    }
    for i in 0..8 {
        let j = state.cp[i] as usize;
        let ori = state.co[i] as usize;
        for k in 0..3 {
            f[CORNER_FACELET[i][(k + ori) % 3]] = color_chars[CORNER_COLOR[j][k] as usize];
        }
    }
    for i in 0..12 {
        let j = state.ep[i] as usize;
        let ori = state.eo[i] as usize;
        for k in 0..2 {
            f[EDGE_FACELET[i][(k + ori) % 2]] = color_chars[EDGE_COLOR[j][k] as usize];
        }
    }
    f
}
