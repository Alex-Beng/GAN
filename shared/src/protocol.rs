use crate::device_config::Protocol;

pub struct BitWriter {
    buf: Vec<u8>,
    bit_pos: usize,
}

impl BitWriter {
    pub fn new(size: usize) -> Self {
        Self { buf: vec![0u8; size], bit_pos: 0 }
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

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

#[allow(dead_code)]
pub struct BitReader<'a> {
    buf: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    #[allow(dead_code)]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, bit_pos: 0 }
    }

    #[allow(dead_code)]
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
    MoveTable { cp: [3, 0, 1, 2, 4, 5, 6, 7], co: [0,0,0,0,0,0,0,0], ep: [3,0,1,2,4,5,6,7,8,9,10,11], eo: [0,0,0,0,0,0,0,0,0,0,0,0] },
    MoveTable { cp: [4,1,2,0,7,5,6,3], co: [2,0,0,1,1,0,0,2], ep: [8,1,2,3,11,5,6,7,4,9,10,0], eo: [0,0,0,0,0,0,0,0,0,0,0,0] },
    MoveTable { cp: [1,5,2,3,0,4,6,7], co: [1,2,0,0,2,1,0,0], ep: [0,9,2,3,4,8,6,7,1,5,10,11], eo: [0,1,0,0,0,1,0,0,1,1,0,0] },
    MoveTable { cp: [0,1,2,3,5,6,7,4], co: [0,0,0,0,0,0,0,0], ep: [0,1,2,3,5,6,7,4,8,9,10,11], eo: [0,0,0,0,0,0,0,0,0,0,0,0] },
    MoveTable { cp: [0,2,6,3,4,1,5,7], co: [0,1,2,0,0,2,1,0], ep: [0,1,10,3,4,5,9,7,8,2,6,11], eo: [0,0,0,0,0,0,0,0,0,0,0,0] },
    MoveTable { cp: [0,1,3,7,4,5,2,6], co: [0,0,1,2,0,0,2,1], ep: [0,1,2,11,4,5,6,10,8,9,3,7], eo: [0,0,0,1,0,0,0,1,0,0,1,1] },
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

// === build_facelets ===

pub fn build_facelets(proto: Protocol, serial: u16, state: &CubeState) -> Vec<u8> {
    match proto {
        Protocol::Gen2 => {
            let mut w = BitWriter::new(20);
            w.write_bits(0x04, 4);
            w.write_bits(serial as u32, 8);
            for i in 0..7 { w.write_bits(state.cp[i] as u32, 3); }
            for i in 0..7 { w.write_bits(state.co[i] as u32, 2); }
            for i in 0..11 { w.write_bits(state.ep[i] as u32, 4); }
            for i in 0..11 { w.write_bits(state.eo[i] as u32, 1); }
            w.finish()
        }
        Protocol::Gen3 => {
            let mut w = BitWriter::new(17);
            w.write_bits(0x55, 8);
            w.write_bits(0x02, 8);
            w.write_bits(12, 8);
            w.write_bits(serial as u32, 16);
            for i in 0..7 { w.write_bits(state.cp[i] as u32, 3); }
            for i in 0..7 { w.write_bits(state.co[i] as u32, 2); }
            w.write_bits(0, 2);
            for i in 0..11 { w.write_bits(state.ep[i] as u32, 4); }
            for i in 0..11 { w.write_bits(state.eo[i] as u32, 1); }
            w.finish()
        }
        Protocol::Gen4 => {
            let mut w = BitWriter::new(20);
            w.write_bits(0xED, 8);
            w.write_bits(12, 8);
            w.write_bits(serial as u32, 16);
            for i in 0..7 { w.write_bits(state.cp[i] as u32, 3); }
            for i in 0..7 { w.write_bits(state.co[i] as u32, 2); }
            w.write_bits(0, 2);
            for i in 0..11 { w.write_bits(state.ep[i] as u32, 4); }
            for i in 0..11 { w.write_bits(state.eo[i] as u32, 1); }
            w.finish()
        }
        Protocol::Gan251 => {
            let mut buf = [0u8; 20];
            buf[0] = 0xED;
            buf[1] = 0x0E;
            buf[2..4].copy_from_slice(&(serial as u16).to_le_bytes());
            let mut w = BitWriter::new(12);
            for i in 0..7  { w.write_bits(state.cp[i] as u32, 3); }
            for i in 0..8  { w.write_bits(state.co[i] as u32, 2); }
            for i in 0..11 { w.write_bits(state.ep[i] as u32, 4); }
            for i in 0..12 { w.write_bits(state.eo[i] as u32, 1); }
            buf[4..16].copy_from_slice(&w.finish());
            let crc = crate::crypto::crc16_ccitt_false(&buf[..18]);
            buf[18] = (crc & 0xFF) as u8;
            buf[19] = (crc >> 8) as u8;
            buf.to_vec()
        }
        Protocol::MoYu32 => build_facelets_moyu32(serial, state),
        Protocol::QiYi => {
            let fb = &build_facelets_qiyi(state);
            let fb_packed = &pack_facelets_nibble(fb);
            let mac = crate::device_config::get_config(Protocol::QiYi).mac;
            let payload = build_full_qiyi(fb_packed, serial, 80, mac);
            qiyi_frame(&payload)
        }
        Protocol::Timer => Vec::new(),
        Protocol::Robot => Vec::new(),  // Robot 不使用 facelets
    }
}

// === build_hardware ===

pub fn build_hardware_single(proto: Protocol) -> Vec<u8> {
    match proto {
        Protocol::Gen2 => {
            let mut w = BitWriter::new(20);
            w.write_bits(0x05, 4);
            w.write_bits(0, 4);
            w.write_bits(1, 8);
            w.write_bits(0, 8);
            w.write_bits(1, 8);
            w.write_bits(0, 8);
            for b in b"GAN356IS" { w.write_bits(*b as u32, 8); }
            w.write_bits(0, 1);
            w.finish()
        }
        Protocol::Gen3 => {
            let mut w = BitWriter::new(16);
            w.write_bits(0x55, 8);
            w.write_bits(0x07, 8);
            w.write_bits(9, 8);
            w.write_bits(0, 8);
            // V2 `ble=7` model field is 5 bytes (GANicV2S -> "GANic").
            for b in b"GANic" { w.write_bits(*b as u32, 8); }
            w.write_bits(1, 4);
            w.write_bits(0, 4);
            w.write_bits(1, 4);
            w.write_bits(0, 4);
            w.finish()
        }
        Protocol::Gen4 | Protocol::Gan251 => {
            let mut w = BitWriter::new(20);
            w.write_bits(0xFC, 8);
            w.write_bits(7, 8);
            for b in b"GAN12UI" { w.write_bits(*b as u32, 8); }
            w.finish()
        }
        // MoYu32 is handled directly in ble.rs (needs the mutable device name).
        Protocol::MoYu32 => Vec::new(),
        Protocol::QiYi => {
            let dummy_fb = vec![0u8; 54];
            let mac = crate::device_config::get_config(Protocol::QiYi).mac;
            let payload = build_full_qiyi(&dummy_fb, 0, 80, mac);
            qiyi_frame(&payload)
        }
        Protocol::Timer => Vec::new(),
        Protocol::Robot => Vec::new(),
    }
}

pub fn build_hardware_multi(proto: Protocol) -> Vec<Vec<u8>> {
    let mut msgs = Vec::new();

    // V3 info responses. Field layouts verified against the decrypted GAN SDK
    // protocol definition (`GanSDK_ProtocolV3.json`, GAN SDK v3.18):
    //   252 model          index:8 + model:8*11
    //   253 version        index:8 + version:4*2
    //   254 deviceVersion  index:8 + deviceVersion:16
    //   250 productDate    index:8 + (16+8+8)
    //   245 buildTime      index:8 + (16+8+8+8+8)
    //   246 restartReason  index:8 + restartReason:16
    //   255 macAddress     index:8 + macAddress:8*7
    let model: &[u8] = match proto {
        Protocol::Gen4 => b"GANi4",
        Protocol::Gan251 => b"GAN251ui",
        _ => b"UNKNOWN",
    };

    // hardware name (0xFC / 252), 11-byte model field, zero padded.
    let mut w = BitWriter::new(20);
    w.write_bits(0xFC, 8);
    w.write_bits(1, 8);
    for i in 0..11 {
        w.write_bits(*model.get(i).unwrap_or(&0) as u32, 8);
    }
    msgs.push(w.finish());

    // hardware version (0xFE / 254), 16-bit deviceVersion.
    let mut w = BitWriter::new(20);
    w.write_bits(0xFE, 8);
    w.write_bits(2, 8);
    w.write_bits(0x0100, 16);
    msgs.push(w.finish());

    // software version (0xFD / 253), two 4-bit fields.
    let mut w = BitWriter::new(20);
    w.write_bits(0xFD, 8);
    w.write_bits(3, 8);
    w.write_bits(1, 4);
    w.write_bits(0, 4);
    msgs.push(w.finish());

    // product date (0xFA / 250), year:16 month:8 day:8.
    let mut w = BitWriter::new(20);
    w.write_bits(0xFA, 8);
    w.write_bits(4, 8);
    w.write_bits(2024u32, 16);
    w.write_bits(6u32, 8);
    w.write_bits(1u32, 8);
    msgs.push(w.finish());

    // build time (0xF5 / 245), year:16 month:8 day:8 hour:8 minute:8.
    let mut w = BitWriter::new(20);
    w.write_bits(0xF5, 8);
    w.write_bits(5, 8);
    w.write_bits(2024u32, 16);
    w.write_bits(6u32, 8);
    w.write_bits(1u32, 8);
    w.write_bits(12u32, 8);
    w.write_bits(0u32, 8);
    msgs.push(w.finish());

    // restart reason (0xF6 / 246), 16-bit.
    let mut w = BitWriter::new(20);
    w.write_bits(0xF6, 8);
    w.write_bits(6, 8);
    w.write_bits(0u32, 16);
    msgs.push(w.finish());

    // mac address (0xFF / 255), reversed MAC + 1 reserved byte.
    let mac = crate::device_config::get_config(proto).mac;
    let mut w = BitWriter::new(20);
    w.write_bits(0xFF, 8);
    w.write_bits(7, 8);
    for i in 0..6 {
        w.write_bits(mac[5 - i] as u32, 8);
    }
    w.write_bits(0, 8);
    msgs.push(w.finish());

    msgs
}

// === build_battery ===

pub fn build_battery(proto: Protocol, level: u8) -> Vec<u8> {
    match proto {
        Protocol::Gen2 => {
            let mut w = BitWriter::new(20);
            w.write_bits(0x09, 4);
            w.write_bits(0, 4);
            w.write_bits(level.min(100) as u32, 8);
            w.finish()
        }
        Protocol::Gen3 => {
            let mut w = BitWriter::new(16);
            w.write_bits(0x55, 8);
            w.write_bits(0x10, 8);
            w.write_bits(1, 8);
            w.write_bits(level.min(100) as u32, 8);
            w.finish()
        }
        Protocol::Gen4 | Protocol::Gan251 => {
            let mut buf = [0u8; 20];
            buf[0] = 0xEF;
            buf[1] = 0x01;
            buf[2] = level.min(100);
            let crc = crate::crypto::crc16_ccitt_false(&buf[..18]);
            buf[18] = (crc & 0xFF) as u8;
            buf[19] = (crc >> 8) as u8;
            buf.to_vec()
        }
        Protocol::MoYu32 => build_battery_moyu32(level),
        Protocol::QiYi => {
            let dummy_fb = vec![0u8; 54];
            let mac = crate::device_config::get_config(Protocol::QiYi).mac;
            let payload = build_full_qiyi(&dummy_fb, 0, level, mac);
            qiyi_frame(&payload)
        }
        Protocol::Timer => Vec::new(),
        Protocol::Robot => Vec::new(),
    }
}

// === build_move ===

const G3_FACE_BITMAP: [u8; 6] = [2, 32, 8, 1, 16, 4];
const H3_FACE_BITMAP: [u8; 6] = [1, 5, 3, 0, 4, 2];

fn face_to_bitmap(face: u8) -> u8 {
    G3_FACE_BITMAP.get(face as usize).copied().unwrap_or(0)
}

pub fn build_move(proto: Protocol, serial: u16, history: &[(u8, u8, u16)], cube_time: u32) -> Vec<u8> {
    match proto {
        Protocol::Gen2 => {
            let mut w = BitWriter::new(20);
            w.write_bits(0x02, 4);
            w.write_bits(serial as u32, 8);
            for i in 0..7 {
                if let Some(&(face, dir, _)) = history.get(i) {
                    w.write_bits(face as u32, 4);
                    w.write_bits(dir as u32, 1);
                } else { w.write_bits(0, 5); }
            }
            for i in 0..7 {
                if let Some(&(_, _, elapsed)) = history.get(i) {
                    w.write_bits(elapsed as u32, 16);
                } else { w.write_bits(0, 16); }
            }
            w.finish()
        }
        Protocol::Gen3 => {
            let mut w = BitWriter::new(16);
            w.write_bits(0x55, 8);
            w.write_bits(0x01, 8);
            w.write_bits(9, 8);
            w.write_bits(cube_time, 32);
            w.write_bits(serial as u32, 16);
            if let Some(&(face, dir, _)) = history.first() {
                w.write_bits(dir as u32, 2);
                w.write_bits(face_to_bitmap(face) as u32, 6);
            } else { w.write_bits(0, 8); }
            w.finish()
        }
        Protocol::Gen4 => {
            let mut w = BitWriter::new(20);
            w.write_bits(0x01, 8);
            w.write_bits(9, 8);
            w.write_bits(cube_time, 32);
            w.write_bits(serial as u32, 16);
            if let Some(&(face, dir, _)) = history.first() {
                w.write_bits(dir as u32, 2);
                w.write_bits(face_to_bitmap(face) as u32, 6);
            } else { w.write_bits(0, 8); }
            w.finish()
        }
        Protocol::Gan251 => {
            let mut buf = [0u8; 20];
            buf[0] = 0x01;
            buf[1] = 0x07;
            buf[2..6].copy_from_slice(&cube_time.to_le_bytes());
            buf[6..8].copy_from_slice(&(serial as u16).to_le_bytes());
            if let Some(&(face, dir, _)) = history.first() {
                buf[8] = ((dir as u8 & 0x3) << 6) | (face_to_bitmap(face) & 0x3F);
            }
            let crc = crate::crypto::crc16_ccitt_false(&buf[..18]);
            buf[18] = (crc & 0xFF) as u8;
            buf[19] = (crc >> 8) as u8;
            buf.to_vec()
        }
        Protocol::MoYu32 => build_move_moyu32(serial, history),
        Protocol::QiYi => {
            // NOTE: the emu move loop builds the QiYi 0x03 frame itself (with the
            // real cube state) via `build_move_qiyi`. This path keeps the generic
            // signature happy with a solved placeholder.
            let payload = build_move_qiyi(serial, &CubeState::solved(), history);
            qiyi_frame(&payload)
        }
        Protocol::Timer => Vec::new(),
        Protocol::Robot => Vec::new(),
    }
}

// === build_move_history ===

fn face_to_hist_bitmap(face: u8) -> u32 {
    H3_FACE_BITMAP.get(face as usize).copied().unwrap_or(0) as u32
}

pub fn build_move_history(proto: Protocol, start_serial: u8, moves: &[(u8, u8)]) -> Vec<u8> {
    match proto {
        Protocol::Gen3 => {
            let count = moves.len().min(0xFF) as u8;
            let mut w = BitWriter::new(16);
            w.write_bits(0x55, 8);
            w.write_bits(0x06, 8);
            w.write_bits(1 + ((count as usize + 1) / 2) as u32, 8);
            w.write_bits(start_serial as u32, 8);
            for chunk in moves.chunks(2) {
                for (j, &(face, dir)) in chunk.iter().enumerate() {
                    w.write_bits(face_to_hist_bitmap(face), 3);
                    w.write_bits(dir as u32, 1);
                    if j == 0 && chunk.len() == 1 { w.write_bits(0, 4); }
                }
            }
            w.finish()
        }
        Protocol::Gen4 | Protocol::Gan251 => {
            let count = moves.len().min(0xFF) as u8;
            let mut w = BitWriter::new(20);
            w.write_bits(0xD1, 8);
            w.write_bits(1 + ((count as usize + 1) / 2) as u32, 8);
            w.write_bits(start_serial as u32, 8);
            w.write_bits(count as u32, 8);
            for chunk in moves.chunks(2) {
                for (j, &(face, dir)) in chunk.iter().enumerate() {
                    w.write_bits(face_to_hist_bitmap(face), 3);
                    w.write_bits(dir as u32, 1);
                    if j == 0 && chunk.len() == 1 { w.write_bits(0, 4); }
                }
            }
            w.finish()
        }
        Protocol::Gen2 => Vec::new(),
        Protocol::MoYu32 | Protocol::QiYi => Vec::new(),
        Protocol::Timer => Vec::new(),
        Protocol::Robot => Vec::new(),
    }
}

// === build_gyro ===

#[allow(dead_code)]
pub fn build_gyro(proto: Protocol, qw: i16, qx: i16, qy: i16, qz: i16, vx: i8, vy: i8, vz: i8) -> Option<Vec<u8>> {
    match proto {
        Protocol::Gen2 => {
            let mut w = BitWriter::new(20);
            w.write_bits(0x01, 4);
            w.write_bits(qw as u16 as u32, 16);
            w.write_bits(qx as u16 as u32, 16);
            w.write_bits(qy as u16 as u32, 16);
            w.write_bits(qz as u16 as u32, 16);
            w.write_bits((vx & 0xF) as u32, 4);
            w.write_bits((vy & 0xF) as u32, 4);
            w.write_bits((vz & 0xF) as u32, 4);
            Some(w.finish())
        }
        Protocol::Gen4 | Protocol::Gan251 => {
            let mut w = BitWriter::new(20);
            w.write_bits(0xEC, 8);
            w.write_bits(10, 8);
            w.write_bits(qw as u16 as u32, 16);
            w.write_bits(qx as u16 as u32, 16);
            w.write_bits(qy as u16 as u32, 16);
            w.write_bits(qz as u16 as u32, 16);
            w.write_bits((vx & 0xF) as u32, 4);
            w.write_bits((vy & 0xF) as u32, 4);
            w.write_bits((vz & 0xF) as u32, 4);
            Some(w.finish())
        }
        Protocol::Gen3 => None,
        Protocol::MoYu32 => {
            let scale = 1i32 << 30;
            let qwi = (qw as f64 / 32767.0 * scale as f64) as i32;
            let qxi = (qx as f64 / 32767.0 * scale as f64) as i32;
            let qyi = (qy as f64 / 32767.0 * scale as f64) as i32;
            let qzi = (qz as f64 / 32767.0 * scale as f64) as i32;
            Some(build_gyro_moyu32(qwi, qxi, qyi, qzi))
        }
        Protocol::QiYi => {
            let qwf = qw as f32 / 32767.0;
            let qxf = qx as f32 / 32767.0;
            let qyf = qy as f32 / 32767.0;
            let qzf = qz as f32 / 32767.0;
            Some(qiyi_frame(&build_gyro_qiyi(qwf, qxf, qyf, qzf)))
        }
        Protocol::Timer => None,
        Protocol::Robot => None,
    }
}

// === parse_command_type ===

#[derive(Debug, Clone, Copy)]
pub enum IncomingCommand {
    Facelets,
    Hardware,
    Battery,
    Reset,
    MoveHistory { start_serial: u8, count: u8 },
    /// QiYi (Smart Player Pro) "set state / reset": `FC 4C 08 <ts:4 LE> <facelets:27> <crc>`.
    /// The payload is read by the caller straight from the decrypted buffer.
    QiYiSetState,
    Unknown,
}

pub fn parse_command_type(proto: Protocol, data: &[u8]) -> IncomingCommand {
    match proto {
        Protocol::Gen2 => match data.get(0) {
            Some(0x04) => IncomingCommand::Facelets,
            Some(0x05) => IncomingCommand::Hardware,
            Some(0x09) => IncomingCommand::Battery,
            Some(0x0A) => IncomingCommand::Reset,
            _ => IncomingCommand::Unknown,
        },
        Protocol::Gen3 => {
            if data.len() < 16 || data[0] != 0x68 { return IncomingCommand::Unknown; }
            match data[1] {
                0x01 => IncomingCommand::Facelets,
                0x04 => IncomingCommand::Hardware,
                0x07 => IncomingCommand::Battery,
                0x05 => IncomingCommand::Reset,
                0x03 => IncomingCommand::MoveHistory { start_serial: data[2], count: data[3] },
                _ => IncomingCommand::Unknown,
            }
        }
        Protocol::Gen4 | Protocol::Gan251 => {
            if data.len() < 20 { return IncomingCommand::Unknown; }
            match (data[0], data[1], data.get(3)) {
                (0xDD, 0x04, Some(0xED)) => IncomingCommand::Facelets,
                (0xDD, 0x04, Some(0xEF)) => IncomingCommand::Battery,
                (0xDF, 0x03, _) => IncomingCommand::Hardware,
                (0xD2, 0x0D, _) => IncomingCommand::Reset,
                (0xD1, 0x04, _) => IncomingCommand::MoveHistory { start_serial: data[2], count: data[3] },
                _ => IncomingCommand::Unknown,
            }
        }
        // MoYu32 uses its own decoder (parse_command_moyu32 -> MoYuCommand) in ble.rs.
        Protocol::MoYu32 => IncomingCommand::Unknown,
        Protocol::QiYi => parse_command_qiyi(data),
        Protocol::Timer => IncomingCommand::Unknown,
        Protocol::Robot => IncomingCommand::Unknown,  // 使用 parse_robot_command
    }
}

// === to_facelet_string ===

const CORNER_FACELET: [[usize; 3]; 8] = [
    [8, 9, 20], [6, 18, 38], [0, 36, 47], [2, 45, 11],
    [29, 26, 15], [27, 44, 24], [33, 53, 42], [35, 17, 51],
];
const CORNER_COLOR: [[u8; 3]; 8] = [
    [0, 1, 2], [0, 2, 4], [0, 4, 5], [0, 5, 1],
    [3, 2, 1], [3, 4, 2], [3, 5, 4], [3, 1, 5],
];
const EDGE_FACELET: [[usize; 2]; 12] = [
    [5, 10], [7, 19], [3, 37], [1, 46], [32, 16], [28, 25],
    [30, 43], [34, 52], [23, 12], [21, 41], [50, 39], [48, 14],
];
const CENTER_FACELET: [usize; 6] = [4, 13, 22, 31, 40, 49];
const EDGE_COLOR: [[u8; 2]; 12] = [
    [0, 1], [0, 2], [0, 4], [0, 5], [3, 1], [3, 2],
    [3, 4], [3, 5], [2, 1], [2, 4], [5, 4], [5, 1],
];

pub fn to_facelet_string(state: &CubeState) -> [u8; 54] {
    let color_chars = [b'U', b'R', b'F', b'D', b'L', b'B'];
    let mut f = [b'U'; 54];
    for i in 0..6 { f[CENTER_FACELET[i]] = color_chars[i]; }
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

// ==================== MoYu32 (WCU) ====================

// The WCU wire format uses FBUDLR order for both faces and colors (F=0 B=1 U=2 D=3
// L=4 R=5). This maps our internal URFDLB index (U=0 R=1 F=2 D=3 L=4 B=5) onto it;
// read as a list it is also the URFDLB faces in WCU storage order (F, B, U, D, L, R).
// Reference: csTimer src/js/hardware/moyu32cube.js
const URFDLB_TO_FBUDLR: [u32; 6] = [2, 5, 0, 3, 4, 1];

fn facelet_colors(state: &CubeState) -> [u8; 54] {
    let mut facelets = [0u8; 54];
    for i in 0..6 { facelets[CENTER_FACELET[i]] = i as u8; }
    for i in 0..8 {
        let j = state.cp[i] as usize;
        let ori = state.co[i] as usize;
        for k in 0..3 {
            facelets[CORNER_FACELET[i][(k + ori) % 3]] = CORNER_COLOR[j][k];
        }
    }
    for i in 0..12 {
        let j = state.ep[i] as usize;
        let ori = state.eo[i] as usize;
        for k in 0..2 {
            facelets[EDGE_FACELET[i][(k + ori) % 2]] = EDGE_COLOR[j][k];
        }
    }
    facelets
}

pub fn build_facelets_moyu32(serial: u16, state: &CubeState) -> Vec<u8> {
    // WCU state message: [163 (8b)][6 faces x 8 stickers x 3b, FBUDLR order, centers
    // implied][moveCnt (8b)] == 160 bits == 20 bytes
    let mut w = BitWriter::new(20);
    w.write_bits(163, 8);
    let facelets = facelet_colors(state);
    for &face in URFDLB_TO_FBUDLR.iter() {
        let base = face as usize * 9;
        for j in [0usize, 1, 2, 3, 5, 6, 7, 8] { // center (pos 4) is implied
            w.write_bits(URFDLB_TO_FBUDLR[facelets[base + j] as usize], 3);
        }
    }
    w.write_bits(serial as u32 & 0xFF, 8);
    w.finish()
}

pub fn build_battery_moyu32(level: u8) -> Vec<u8> {
    let mut w = BitWriter::new(20);
    w.write_bits(164, 8);
    w.write_bits(level.min(100) as u32, 8);
    w.finish()
}

pub fn build_hardware_moyu32(name: &str) -> Vec<u8> {
    // WCU info message (matches CubeProcess5 opcode 161 bit layout):
    // [161][name 8 bytes ASCII][hw major][hw minor][sw major][sw minor]
    // [flag1..flag5 (1b each)][battery 8b][5b][9b][9b] == 160 bits == 20 bytes.
    // Only the first bytes are meaningful to the app; the trailing fields are
    // filled with sane defaults.
    let mut w = BitWriter::new(20);
    w.write_bits(161, 8);
    let mut nbuf = [b' '; 8];
    for (i, b) in name.bytes().take(8).enumerate() { nbuf[i] = b; }
    for b in nbuf { w.write_bits(b as u32, 8); }
    w.write_bits(1, 8); // hw major
    w.write_bits(0, 8); // hw minor
    w.write_bits(crate::device_config::MOYU32_SW_MAJOR as u32, 8); // sw major
    w.write_bits(crate::device_config::MOYU32_SW_MINOR as u32, 8); // sw minor
    // 5 flags: isOutage, isGyroEnabled, isGyroCorrect, isMagWorking, isMagCalibrated
    for _ in 0..5 { w.write_bits(0, 1); }
    w.write_bits(80, 8); // battery level
    w.write_bits(0, 5);
    w.write_bits(0, 9);
    w.write_bits(0, 9);
    w.finish()
}

pub fn build_bind_account_moyu32(account: u32) -> Vec<u8> {
    // [170][account 32b] == CubeSender5.RequestBindAccount / CubeProcess5 170.
    let mut w = BitWriter::new(20);
    w.write_bits(170, 8);
    w.write_bits(account, 32);
    w.finish()
}

pub fn build_gyro_operation_moyu32(reset_correction: bool, open: bool) -> Vec<u8> {
    // [172][resetCorrection 8b][open 8b] == CubeSender5.RequestCubeGyroOperation.
    let mut w = BitWriter::new(20);
    w.write_bits(172, 8);
    w.write_bits(if reset_correction { 1 } else { 0 }, 8);
    w.write_bits(if open { 1 } else { 0 }, 8);
    w.finish()
}

pub fn build_change_name_moyu32(name: &str) -> Vec<u8> {
    // [173][len 8b][name bytes 8b each] == CubeSender5.RequestCubeChangeName.
    let bytes = name.as_bytes();
    let len = bytes.len().min(16);
    let mut w = BitWriter::new(20);
    w.write_bits(173, 8);
    w.write_bits(len as u32, 8);
    for b in &bytes[..len] { w.write_bits(*b as u32, 8); }
    w.finish()
}

pub fn build_solving_moyu32(state: &[u8; 48], flag: bool) -> Vec<u8> {
    // [174][48 x 3b facelets][1b flag] == CubeProcess5 174.
    let mut w = BitWriter::new(20);
    w.write_bits(174, 8);
    for &s in state.iter() { w.write_bits(s as u32 & 0x7, 3); }
    w.write_bits(if flag { 1 } else { 0 }, 1);
    w.finish()
}

pub fn build_move_history_moyu32(serial: u8, moves: &[(u8, u8, u16)]) -> Vec<u8> {
    // [168][6 x elapsed 16b][moveCnt 8b][5 x move code 5b] == CubeProcess5 168.
    let mut w = BitWriter::new(20);
    w.write_bits(168, 8);
    let n = moves.len().min(5);
    for i in 0..6 {
        let elapsed = if i < n { moves[i].2 as u32 } else { 0 };
        w.write_bits(elapsed, 16);
    }
    w.write_bits(serial as u32, 8);
    for i in 0..5 {
        if i < n {
            let (face, dir, _) = moves[i];
            let code = (URFDLB_TO_FBUDLR[face as usize] << 1) | dir as u32;
            w.write_bits(code, 5);
        } else {
            w.write_bits(0b11111, 5);
        }
    }
    w.finish()
}

pub fn build_disconnect_moyu32() -> Vec<u8> {
    // [160] == CubeProcess5 160 -> CubeDisconnect.
    let mut w = BitWriter::new(20);
    w.write_bits(160, 8);
    w.finish()
}

// ==================== WCU OTA (02f00000-...fe00, plaintext) ====================

/// Decoded OTA request written by the WCU app to characteristic `...ff01`.
/// Formats follow BluetoothProcessor.OTASender.
#[derive(Debug, Clone, Copy)]
pub enum OtaRequest {
    StartAddress,
    Version,
    Erase { addr: u32 },
    SenderData { base: u32, len: u16 },
    UpdateEnd,
    Unknown,
}

fn le_u32(data: &[u8], off: usize) -> u32 {
    let mut b = [0u8; 4];
    for i in 0..4 { b[i] = *data.get(off + i).unwrap_or(&0); }
    u32::from_le_bytes(b)
}

pub fn parse_ota_request(data: &[u8]) -> OtaRequest {
    match data.get(0) {
        Some(0x01) => OtaRequest::StartAddress,
        Some(0x02) => OtaRequest::Version,
        Some(0x03) => OtaRequest::Erase { addr: le_u32(data, 3) },
        Some(0x05) => OtaRequest::SenderData {
            base: le_u32(data, 3),
            len: u16::from_le_bytes([*data.get(7).unwrap_or(&0), *data.get(8).unwrap_or(&0)]),
        },
        Some(0x09) => OtaRequest::UpdateEnd,
        _ => OtaRequest::Unknown,
    }
}

/// Build an OTA reply as expected by BluetoothProcessor.OTAProcess:
/// `[flag(0=OK)][cmd][len16 LE][payload...]`.
pub fn build_ota_reply(cmd: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(4 + payload.len());
    v.push(0x00);
    v.push(cmd);
    v.push((payload.len() & 0xFF) as u8);
    v.push((payload.len() >> 8) as u8);
    v.extend_from_slice(payload);
    v
}

pub fn build_move_moyu32(serial: u16, history: &[(u8, u8, u16)]) -> Vec<u8> {
    // WCU move message: [165 (8b)][5 x elapsed 16b][moveCnt 8b][5 x move code 5b].
    // Move code = (face << 1) | dir in FBUDLR order; slot 0 = newest; code >= 12
    // marks an empty slot. Padded to 20 bytes.
    let mut w = BitWriter::new(20);
    w.write_bits(165, 8);
    let n = history.len().min(5);
    for i in 0..5 {
        let elapsed = if i < n { history[i].2 as u32 } else { 0 };
        w.write_bits(elapsed, 16);
    }
    w.write_bits(serial as u32 & 0xFF, 8);
    for i in 0..5 {
        if i < n {
            let (face, dir, _) = history[i];
            let code = (URFDLB_TO_FBUDLR[face as usize] << 1) | dir as u32;
            w.write_bits(code, 5);
        } else {
            w.write_bits(0b11111, 5);
        }
    }
    w.finish()
}

#[allow(dead_code)]
pub fn build_gyro_moyu32(qw: i32, qx: i32, qy: i32, qz: i32) -> Vec<u8> {
    let mut w = BitWriter::new(20);
    w.write_bits(171, 8);
    let mut le = |v: i32| {
        let b = v.to_le_bytes();
        for &byte in &b { w.write_bits(byte as u32, 8); }
    };
    le(qw); le(qx); le(qy); le(qz);
    w.finish()
}

#[allow(dead_code)]
pub fn build_gyro_enable_moyu32() -> Vec<u8> {
    let mut v = vec![172u8];
    v.extend_from_slice(&[0u8; 19]);
    v
}

/// Decoded MoYu32 (WCU) inbound command. Bit layout per the WCU app's
/// CubeProcess5 / CubeSender5 (MSB-first, encrypted with the salted key).
#[derive(Debug, Clone, Copy)]
pub enum MoYuCommand {
    Facelets,
    Hardware,
    Battery,
    BindAccount(u32),
    GyroOperation { reset_correction: bool, open: bool },
    ChangeName { len: u8 },
    Solving { flag: bool },
    ResetState,
    MoveHistory { step: u8 },
    Disconnect,
    Unknown,
}

/// Read `count` bits (MSB-first) starting at bit offset `start` from `data`.
fn read_bits(data: &[u8], start: usize, count: usize) -> u32 {
    let mut value = 0u32;
    for i in 0..count {
        let pos = start + i;
        let byte_idx = pos / 8;
        if byte_idx >= data.len() { break; }
        let bit_idx = 7 - (pos % 8);
        let bit = (data[byte_idx] >> bit_idx) & 1;
        value = (value << 1) | bit as u32;
    }
    value
}

pub fn parse_command_moyu32(data: &[u8]) -> MoYuCommand {
    match data.get(0) {
        Some(160) => MoYuCommand::Disconnect,
        Some(161) => MoYuCommand::Hardware,
        Some(162) => MoYuCommand::ResetState,
        Some(163) => MoYuCommand::Facelets,
        Some(164) => MoYuCommand::Battery,
        Some(168) => MoYuCommand::MoveHistory { step: read_bits(data, 8, 8) as u8 },
        Some(170) => MoYuCommand::BindAccount(read_bits(data, 8, 32)),
        Some(172) => MoYuCommand::GyroOperation {
            reset_correction: read_bits(data, 8, 8) == 1,
            open: read_bits(data, 16, 8) == 1,
        },
        Some(173) => MoYuCommand::ChangeName { len: read_bits(data, 8, 8) as u8 },
        Some(174) => MoYuCommand::Solving { flag: read_bits(data, 8 + 48 * 3, 1) == 1 },
        _ => MoYuCommand::Unknown,
    }
}

/// Extract the requested name bytes from a 173 ChangeName request.
pub fn parse_change_name_moyu32(data: &[u8]) -> String {
    let len = read_bits(data, 8, 8) as usize;
    let mut s = String::new();
    for i in 0..len.min(16) {
        let c = read_bits(data, 16 + i * 8, 8) as u8;
        if c != 0 { s.push(c as char); }
    }
    s
}

/// Extract the 48 requested facelets (3b each) from a 162 Reset request or a
/// 174 Solving message.
pub fn parse_state48_moyu32(data: &[u8], header_bits: usize) -> [u8; 48] {
    let mut out = [0u8; 48];
    for i in 0..48 { out[i] = read_bits(data, header_bits + i * 3, 3) as u8; }
    out
}

/// Convert the WCU wire facelet array (48 stickers: 6 faces x 8, FBUDLR face
/// order, centers omitted; color indices also FBUDLR) into a cubie CubeState.
/// Inverse of `build_facelets_moyu32`.
pub fn facelets48_to_state_moyu32(facelets: &[u8; 48]) -> CubeState {
    let positions = [0usize, 1, 2, 3, 5, 6, 7, 8];
    let mut f = [0u8; 54];
    for i in 0..6 {
        let b = URFDLB_TO_FBUDLR[i] as usize; // block index for our face i
        f[i * 9 + 4] = i as u8; // center
        for (p, &slot) in positions.iter().enumerate() {
            let color_fbudlr = (facelets[b * 8 + p] & 0x7) as usize;
            f[i * 9 + slot] = FBUDLR_TO_URFDLB[color_fbudlr] as u8;
        }
    }
    decode_facelets(f)
}

/// WCU FBUDLR color/face index -> our URFDLB index.
const FBUDLR_TO_URFDLB: [u32; 6] = [2, 5, 0, 3, 4, 1];

/// Reverse of `to_facelet_string`: recover a CubeState from 54 sticker colors
/// (URFDLB color indices, centers implicit by position).
fn decode_facelets(f: [u8; 54]) -> CubeState {
    let mut state = CubeState::solved();
    for i in 0..8 {
        'find: for j in 0..8 {
            for ori in 0..3 {
                let mut ok = true;
                for k in 0..3 {
                    if f[CORNER_FACELET[i][(k + ori) % 3]] != CORNER_COLOR[j][k] { ok = false; break; }
                }
                if ok { state.cp[i] = j as u8; state.co[i] = ori as u8; break 'find; }
            }
        }
    }
    for i in 0..12 {
        'find: for j in 0..12 {
            for ori in 0..2 {
                let mut ok = true;
                for k in 0..2 {
                    if f[EDGE_FACELET[i][(k + ori) % 2]] != EDGE_COLOR[j][k] { ok = false; break; }
                }
                if ok { state.ep[i] = j as u8; state.eo[i] = ori as u8; break 'find; }
            }
        }
    }
    state
}

// ==================== QiYi ====================

// Face emission order for QiYi. The App's canonical position order is URFDLB
// (verified from `getFacelets` + the solved facelet constant
// "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB"): positions 0..5 are
// U,R,F,D,L,B. Internal indices are the same (U=0 R=1 F=2 D=3 L=4 B=5).
// The previous [4,1,3,0,2,5] emitted LRDUFB, which left only the fixed points
// R(1)/B(5) correct -- matching the observed "D/L faces swapped" symptom.
const QIYI_FACE_ORDER: [usize; 6] = [0, 1, 2, 3, 4, 5];

pub fn build_facelets_qiyi(state: &CubeState) -> Vec<u8> {
    // Internal color codes are URFDLB (U=0 R=1 F=2 D=3 L=4 B=5). The QiYi App
    // (and csTimer) expect LRDUFB (L=0 R=1 D=2 U=3 F=4 B=5), so remap before packing;
    // otherwise opposite pairs (e.g. U<->D / L<->B) get swapped and the App shows an
    // illegal state (centers look fine, corners/edges are wrong).
    const REMAP: [u8; 6] = [3, 1, 4, 2, 0, 5]; // U->3 R->1 F->4 D->2 L->0 B->5
    let mut facelets = [0u8; 54];
    for i in 0..6 { facelets[CENTER_FACELET[i]] = REMAP[i]; }
    for i in 0..8 {
        let j = state.cp[i] as usize;
        let ori = state.co[i] as usize;
        for k in 0..3 {
            facelets[CORNER_FACELET[i][(k + ori) % 3]] = REMAP[CORNER_COLOR[j][k] as usize];
        }
    }
    for i in 0..12 {
        let j = state.ep[i] as usize;
        let ori = state.eo[i] as usize;
        for k in 0..2 {
            facelets[EDGE_FACELET[i][(k + ori) % 2]] = REMAP[EDGE_COLOR[j][k] as usize];
        }
    }
    let mut fb = Vec::new();
    for &m in QIYI_FACE_ORDER.iter() {
        let base = m * 9;
        for s in 0..9 {
            fb.push(facelets[base + s] & 0xF);
        }
    }
    fb
}

pub fn pack_facelets_nibble(fb: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for i in (0..fb.len()).step_by(2) {
        let a = fb[i] & 0xF;
        let b = if i + 1 < fb.len() { fb[i + 1] & 0xF } else { 0 };
        out.push(a | (b << 4));
    }
    out
}

#[allow(dead_code)]
pub fn reverse_slice(data: &[u8]) -> Vec<u8> {
    let mut v = data.to_vec();
    v.reverse();
    v
}

pub fn build_full_qiyi(facelets: &[u8], serial: u16, battery: u8, _mac: &[u8; 6]) -> Vec<u8> {
    // QiYi "hello"/state record, csTimer-compatible:
    //   [0x02][ts:4 BE][facelets:27][bat][bat]  == 34 content bytes -> frame len 0x26
    // App offsets: role=data[2], ts=data[3..7], facelets=data[7..34],
    // battery is read from data[35] (`field_33` -> BlueDeviceModel.setBattery);
    // data[34] is unused by csTimer, we mirror the battery there as well.
    let mut content: Vec<u8> = Vec::with_capacity(34);
    content.push(0x02);
    content.extend_from_slice(&(serial as u32).to_be_bytes());
    for i in 0..27 {
        content.push(facelets.get(i).copied().unwrap_or(0));
    }
    let b = battery.min(100);
    content.push(b); // data[34]
    content.push(b); // data[35]
    content
}

pub fn build_move_qiyi(serial: u16, state: &CubeState, history: &[(u8, u8, u16)]) -> Vec<u8> {
    // State-change record (role 0x03), csTimer-compatible, 92 content bytes -> frame len 0x60.
    //   [0x03][ts:4 BE][facelets:27][firstMove][battery] ... history entries (ts:4 BE + mv)
    let axis_map = [4u8, 1, 3, 0, 2, 5];
    let encode = |face: u8, dir: u8| -> u8 { ((axis_map[(face as usize) & 0x7] << 1) | (dir & 1)) + 1 };

    let mut c = vec![0u8; 92];
    c[0] = 0x03;
    c[1..5].copy_from_slice(&(serial as u32).to_be_bytes());
    // Current facelets (27 bytes, wire order/encoding) at content[5..32).
    let packed = pack_facelets_nibble(&build_facelets_qiyi(state));
    for (i, b) in packed.iter().take(27).enumerate() {
        c[5 + i] = *b;
    }
    if let Some(&(face, dir, _)) = history.first() {
        c[32] = encode(face, dir);
    }
    c[33] = 80; // battery (data[35])
    // history entries: newest first at content[84], then 79, 74, ...
    let mut off: i32 = 84;
    let mut t: u32 = 0;
    for &(face, dir, elapsed) in history.iter() {
        if off < 34 { break; }
        t = t.wrapping_add(elapsed as u32);
        let o = off as usize;
        c[o..o + 4].copy_from_slice(&t.to_be_bytes());
        c[o + 4] = encode(face, dir);
        off -= 5;
    }
    c
}

pub fn qiyi_frame(payload: &[u8]) -> Vec<u8> {
    // csTimer frame: [0xFE][len][content...][crc16 MODBUS lo][hi], zero-padded to 16.
    // `len` is the WHOLE frame length (FE + len + content + crc) == content.len() + 4.
    let len = payload.len() + 4;
    let mut framed = vec![0xFEu8, len as u8];
    framed.extend_from_slice(payload);
    let crc = crate::crypto::crc16_modbus(&framed);
    framed.push((crc & 0xFF) as u8);
    framed.push((crc >> 8) as u8);
    while framed.len() % 16 != 0 { framed.push(0); }
    framed
}

#[allow(dead_code)]
pub fn build_gyro_qiyi(qw: f32, qx: f32, qy: f32, qz: f32) -> Vec<u8> {
    let mut p = vec![0xCCu8, 0x10];
    for v in &[qw, qx, qy, qz] {
        p.extend_from_slice(&v.to_le_bytes());
    }
    while p.len() < 18 { p.push(0); }
    p
}

pub fn parse_command_qiyi(data: &[u8]) -> IncomingCommand {
    // QiYi downlink is plain (already AES-ECB encrypted on the wire). Frames:
    //   * `FE 09 <role> <ts:4> <crc>`  -> the App's ACK for a received frame; IGNORE.
    //   * `FC 2A ...`                  -> bind handshake; ignore for now.
    //   * `BA 12 ...`                  -> query; ignore for now.
    //   * `FC 4C 08 <ts:4 LE> <facelets:27> <crc>` -> set state / reset.
    if data.len() < 3 { return IncomingCommand::Unknown; }
    if data[0] == 0xFC && data[1] == 0x4C && data[2] == 0x08 {
        return IncomingCommand::QiYiSetState;
    }
    IncomingCommand::Unknown
}

/// Decode a QiYi wire facelet block (27 bytes) into a `CubeState`.
/// Wire format: each byte low-nibble first; face order URFDLB; color nibble
/// `{U:3, R:1, F:4, D:2, L:0, B:5}` (inverse of `build_facelets_qiyi`'s REMAP).
pub fn decode_facelets_qiyi(wire: &[u8]) -> CubeState {
    const INVMAP: [u8; 6] = [4, 1, 3, 0, 2, 5]; // wire nibble -> internal URFDLB code
    let mut f = [0u8; 54];
    for i in 0..27 {
        let b = wire.get(i).copied().unwrap_or(0);
        let lo = (b & 0x0F) as usize;
        let hi = ((b >> 4) & 0x0F) as usize;
        f[i * 2] = INVMAP.get(lo).copied().unwrap_or(0);
        f[i * 2 + 1] = INVMAP.get(hi).copied().unwrap_or(0);
    }
    decode_facelets(f)
}

// ==================== Robot ====================

/// Robot 信息类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RobotInfoType {
    RemainFormulas,   // appProtoId=2
    MacAddress,       // appProtoId=5
    Model,            // appProtoId=6
    SerialNumber,     // appProtoId=7
    Version,          // appProtoId=8
    DeviceVersion,    // appProtoId=9
    VersionDate,      // appProtoId=10
    Manufacturer,     // appProtoId=11
}

/// Robot 指令
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RobotCommand {
    Order(u8),                    // appProtoId=1
    QueryRemainFormulas,          // appProtoId=2
    SetFormulaList,               // appProtoId=3
    SingleMove(u8, u8, u16),      // appProtoId=4: direction, surface, turns
    QueryInfo(RobotInfoType),     // appProtoId=5..11
    SetLed(u8, u8, u8, u8),       // appProtoId=12: op, r, g, b
}

/// 解析 Robot 写入指令
pub fn parse_robot_command(data: &[u8]) -> RobotCommand {
    if data.is_empty() {
        return RobotCommand::QueryInfo(RobotInfoType::RemainFormulas);
    }
    let cmd = data[0];
    match cmd {
        1 => RobotCommand::Order(data.get(1).copied().unwrap_or(0)),
        2 => RobotCommand::QueryRemainFormulas,
        3 => RobotCommand::SetFormulaList,
        4 => {
            let dir = data.get(1).copied().unwrap_or(0);
            let surface = data.get(2).copied().unwrap_or(0);
            let turns = u16::from_le_bytes([data.get(3).copied().unwrap_or(0), data.get(4).copied().unwrap_or(0)]);
            RobotCommand::SingleMove(dir, surface, turns)
        }
        5 => RobotCommand::QueryInfo(RobotInfoType::MacAddress),
        6 => RobotCommand::QueryInfo(RobotInfoType::Model),
        7 => RobotCommand::QueryInfo(RobotInfoType::SerialNumber),
        8 => RobotCommand::QueryInfo(RobotInfoType::Version),
        9 => RobotCommand::QueryInfo(RobotInfoType::DeviceVersion),
        10 => RobotCommand::QueryInfo(RobotInfoType::VersionDate),
        11 => RobotCommand::QueryInfo(RobotInfoType::Manufacturer),
        12 => {
            let op = data.get(1).copied().unwrap_or(0);
            let r = data.get(2).copied().unwrap_or(0);
            let g = data.get(3).copied().unwrap_or(0);
            let b = data.get(4).copied().unwrap_or(0);
            RobotCommand::SetLed(op, r, g, b)
        }
        _ => RobotCommand::Order(cmd),
    }
}

/// 构建 Robot 信息响应包
/// 格式: [appProtoId, data...]
pub fn build_robot_info(info_type: RobotInfoType) -> Vec<u8> {
    let mut buf = Vec::with_capacity(20);
    match info_type {
        RobotInfoType::RemainFormulas => {
            buf.push(2);  // appProtoId=2
            buf.push(0);  // 0 formulas remaining
        }
        RobotInfoType::MacAddress => {
            buf.push(5);  // appProtoId=5
            buf.extend_from_slice(&[0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xD0]);
        }
        RobotInfoType::Model => {
            buf.push(6);  // appProtoId=6
            let model = b"GAN Robot Emu  \x00";  // 16 bytes
            buf.extend_from_slice(&model[..16]);
        }
        RobotInfoType::SerialNumber => {
            buf.push(7);  // appProtoId=7
            buf.extend_from_slice(b"EMU0001");  // 7 bytes
        }
        RobotInfoType::Version => {
            buf.push(8);  // appProtoId=8
            buf.extend_from_slice(b"1.0.0\x00\x00\x00");  // 7 bytes
        }
        RobotInfoType::DeviceVersion => {
            buf.push(9);  // appProtoId=9
            buf.extend_from_slice(&[1, 0, 0]);  // 3 bytes
        }
        RobotInfoType::VersionDate => {
            buf.push(10);  // appProtoId=10
            buf.extend_from_slice(&[1, 0, 0]);  // version: 3 bytes
            buf.extend_from_slice(&[0xE8, 0x07, 0x01]);  // date: 2024-07-01
        }
        RobotInfoType::Manufacturer => {
            buf.push(11);  // appProtoId=11
            let mfr = b"GAN ROBOT       ";  // 16 bytes
            buf.extend_from_slice(&mfr[..16]);
        }
    }
    buf
}

/// 构建 Robot 状态响应 (appProtoId=13/14)
/// status: 0=idle, 1=solving, 2=done
pub fn build_robot_status(status: u8, progress: u8) -> Vec<u8> {
    // appProtoId=14: [len, robotStatus, robotTime(16bit)]
    let buf = vec![14u8, 3, status, progress, 0, 0];
    buf
}

// ==================== Timer ====================

/// Timer 命令类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerCommand {
    GetStatus,
    GetTimerData,
    BindAccount(u32),
    SetTime(u8,u8,u8,u8,u8,u8),
    SetLed(u8,u8,u8,u8,u8,u8),
    SetColorType(u8),
    QueryBattery,
    Reset(u8),
    Calibrate(u8),
    QueryInfo(u8),
}

/// Timer 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerStatus {
    Idle = 0,
    Timing = 1,
    Stopped = 2,
}

/// 解析 Timer 写入指令
pub fn parse_timer_command(data: &[u8]) -> TimerCommand {
    if data.len() < 3 {
        return TimerCommand::GetStatus;
    }
    let proto_id = (data[0] as u32) << 16 | (data[1] as u32) << 8 | data[2] as u32;
    // 24-bit protoId
    if proto_id >= 0x010000 && proto_id < 0x020000 {
        return TimerCommand::GetStatus;
    }
    if proto_id >= 0x020000 && proto_id < 0x030000 {
        return TimerCommand::GetTimerData;
    }
    if proto_id >= 0x030000 && proto_id < 0x040000 {
        if data.len() >= 9 {
            let account = u32::from_le_bytes([data[3], data[4], data[5], data[6]]);
            return TimerCommand::BindAccount(account);
        }
        return TimerCommand::GetStatus;
    }
    // 8-bit protoId
    match data[0] {
        4 => TimerCommand::SetTime(
            data.get(1).copied().unwrap_or(0),
            data.get(2).copied().unwrap_or(0),
            data.get(3).copied().unwrap_or(0),
            data.get(4).copied().unwrap_or(0),
            data.get(5).copied().unwrap_or(0),
            data.get(6).copied().unwrap_or(0),
        ),
        5 => TimerCommand::SetLed(
            data.get(1).copied().unwrap_or(0),
            data.get(2).copied().unwrap_or(0),
            data.get(3).copied().unwrap_or(0),
            data.get(4).copied().unwrap_or(0),
            data.get(5).copied().unwrap_or(0),
            data.get(6).copied().unwrap_or(0),
        ),
        6 => TimerCommand::SetColorType(data.get(1).copied().unwrap_or(0)),
        7 => TimerCommand::QueryBattery,
        8 => TimerCommand::Reset(data.get(1).copied().unwrap_or(0)),
        9 => TimerCommand::Calibrate(data.get(1).copied().unwrap_or(0)),
        10..=16 => TimerCommand::QueryInfo(data[0]),
        _ => TimerCommand::GetStatus,
    }
}

/// 构建 Timer 状态响应 [0x01, 0x04, status, crc16]
pub fn build_timer_status(status: TimerStatus) -> Vec<u8> {
    let mut buf = vec![0x01, 0x04, status as u8];
    let crc = crate::crypto::crc16_ccitt_false(&buf);
    buf.push((crc & 0xFF) as u8);
    buf.push((crc >> 8) as u8);
    buf
}

/// 构建 Timer 计时数据 [0x01, 0x08, status, minute, second, ms(16bit), crc16]
pub fn build_timer_data(status: TimerStatus, minute: u8, second: u8, ms: u16) -> Vec<u8> {
    let mut buf = vec![0x01, 0x08, status as u8, minute, second];
    buf.push((ms & 0xFF) as u8);
    buf.push((ms >> 8) as u8);
    let crc = crate::crypto::crc16_ccitt_false(&buf);
    buf.push((crc & 0xFF) as u8);
    buf.push((crc >> 8) as u8);
    buf
}

/// 构建 Timer 信息响应 (appProtoId=10..16)
pub fn build_timer_info(info_type: u8) -> Vec<u8> {
    let mut buf = vec![info_type];
    match info_type {
        10 => buf.extend_from_slice(&[0xAB, 0x12, 0x34, 0x5F, 0xA4, 0xD1]),
        11 => buf.extend_from_slice(b"GAN TIMER EMU  "),
        12 => buf.extend_from_slice(b"TM00001"),
        13 => buf.extend_from_slice(b"1.0.0\x00\x00\x00"),
        14 => buf.extend_from_slice(&[1, 0, 0]),
        15 => {
            buf.extend_from_slice(&[1, 0, 0]);
            buf.extend_from_slice(&[0xE8, 0x07, 0x01]);
        }
        16 => buf.extend_from_slice(b"GAN TIMER"),
        _ => {}
    }
    let crc = crate::crypto::crc16_ccitt_false(&buf);
    buf.push((crc & 0xFF) as u8);
    buf.push((crc >> 8) as u8);
    buf
}

// === GAN Gen2 (V1) incoming decoder (proxy / GATT client side) ===

/// One decoded move from any physical cube: `face` 0..5 = U R F D L B,
/// `dir` 0=CW 1=CCW. `elapsed` is the cube-reported interval in ms (0 if N/A).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CubeMove {
    pub face: u8,
    pub dir: u8,
    pub elapsed: u16,
}

/// Protocol-agnostic event decoded from a physical cube's notification stream.
///
/// All input protocols (GAN Gen2/Gen3/Gen4, MoYu MHC/WCU, QiYi) decode into
/// this single type so the gateway's forwarding logic is protocol-independent.
#[derive(Debug)]
pub enum CubeEvent {
    /// A MOVE event with up to N moves (newest first).
    Move { serial: u8, moves: Vec<CubeMove> },
    /// A FACELETS snapshot: 54 facelet indices (0..5 = U R F D L B).
    Facelets { serial: u8, facelets: [u8; 54] },
    /// Battery level event.
    Battery { serial: u8, level: u8 },
    /// Gyroscope quaternion event (raw signed fixed-point values).
    Gyro { qw: i16, qx: i16, qy: i16, qz: i16 },
    /// Anything else (hardware/reset/unknown), with the raw decrypted bytes.
    Other { kind: u8, raw: Vec<u8> },
}

/// Decode a 54-char URFDLB facelet string into 0..5 facelet indices.
fn facelets_to_indices(facelets: &[u8; 54]) -> [u8; 54] {
    let mut idx = [0u8; 54];
    for i in 0..54 {
        idx[i] = match facelets[i] {
            b'U' => 0, b'R' => 1, b'F' => 2, b'D' => 3, b'L' => 4, _ => 5,
        };
    }
    idx
}

/// Reconstruct a `CubeState` from 7 corner perms (3b each), 7 corner orients
/// (2b), 11 edge perms (4b), 11 edge orients (1b), computing the 8th/12th.
fn read_cube_state(r: &mut BitReader) -> CubeState {
    let mut cp = [0u8; 8];
    let mut co = [0u8; 8];
    let mut ep = [0u8; 12];
    let mut eo = [0u8; 12];
    for i in 0..7 { cp[i] = r.read_bits(3) as u8; }
    cp[7] = (28u32.wrapping_sub(cp[0..7].iter().map(|&x| x as u32).sum::<u32>()) & 0x7) as u8;
    for i in 0..7 { co[i] = r.read_bits(2) as u8; }
    co[7] = (0u32.wrapping_sub(co[0..7].iter().map(|&x| x as u32).sum::<u32>()) & 0x3) as u8;
    for i in 0..11 { ep[i] = r.read_bits(4) as u8; }
    ep[11] = (66u32.wrapping_sub(ep[0..11].iter().map(|&x| x as u32).sum::<u32>()) & 0xF) as u8;
    for i in 0..11 { eo[i] = r.read_bits(1) as u8; }
    eo[11] = (eo[0..11].iter().map(|&x| x as u32).sum::<u32>() & 0x1) as u8;
    CubeState { cp, co, ep, eo }
}

/// Parse a decrypted 20-byte GAN Gen2 (V1) message (see gen2.md §3.3/§3.4).
pub fn decode_gen2(data: &[u8]) -> CubeEvent {
    let mut r = BitReader::new(data);
    let kind = r.read_bits(4) as u8;
    let serial = r.read_bits(8) as u8;
    match kind {
        0x01 => {
            // GYRO: qw/qx/qy/qz each 16 bits (signed fixed-point).
            let qw = r.read_bits(16) as u16 as i16;
            let qx = r.read_bits(16) as u16 as i16;
            let qy = r.read_bits(16) as u16 as i16;
            let qz = r.read_bits(16) as u16 as i16;
            CubeEvent::Gyro { qw, qx, qy, qz }
        }
        0x02 => {
            let mut faces = [(0u8, 0u8); 7];
            for slot in faces.iter_mut() {
                let f = r.read_bits(4) as u8;
                let d = r.read_bits(1) as u8;
                *slot = (f, d);
            }
            let mut moves = Vec::new();
            for &(f, d) in faces.iter() {
                let elapsed = r.read_bits(16) as u16;
                // Stop at padding: face 0 dir 0 with zero elapsed, or face >= 6.
                if f >= 6 { break; }
                moves.push(CubeMove { face: f, dir: d, elapsed });
            }
            CubeEvent::Move { serial, moves }
        }
        0x04 => {
            let state = read_cube_state(&mut r);
            let facelets = to_facelet_string(&state);
            CubeEvent::Facelets { serial, facelets: facelets_to_indices(&facelets) }
        }
        0x09 => {
            let level = r.read_bits(8).min(100) as u8;
            CubeEvent::Battery { serial, level }
        }
        _ => CubeEvent::Other { kind, raw: data.to_vec() },
    }
}

// === GAN Gen3 (V2) decoder (cf. scripts/proxy/gan.py) ===

const GEN3_AXIS_MASKS: [u8; 6] = [2, 32, 8, 1, 16, 4];

fn decode_gen_axis(mask: u8, masks: &[u8; 6]) -> Option<u8> {
    for (i, &m) in masks.iter().enumerate() {
        if mask == m {
            return Some(i as u8);
        }
    }
    None
}

/// Parse a decrypted GAN Gen3 (V2) 16-byte message.
pub fn decode_gen3(data: &[u8]) -> CubeEvent {
    let mut r = BitReader::new(data);
    let magic = r.read_bits(8) as u8;
    if magic != 0x55 {
        return CubeEvent::Other { kind: magic, raw: data.to_vec() };
    }
    let mode = r.read_bits(8) as u8;
    let _length = r.read_bits(8) as u8;
    match mode {
        0x01 => {
            // MOVE: time(32) is split across bytes 3..7 in a scrambled order.
            let t0 = r.read_bits(8);
            let t1 = r.read_bits(8);
            let t2 = r.read_bits(8);
            let t3 = r.read_bits(8);
            let _ts = (t3 << 24) | (t2 << 16) | (t1 << 8) | t0;
            let serial_lo = r.read_bits(8);
            let serial_hi = r.read_bits(8);
            let serial = (serial_hi << 8) | serial_lo;
            let _pad = r.read_bits(6) as u8;
            let pow_ = r.read_bits(2) as u8;
            let mask = r.read_bits(6) as u8;
            let face = decode_gen_axis(mask, &GEN3_AXIS_MASKS).unwrap_or(6);
            let dir = if pow_ == 2 { 1 } else { 0 };
            let moves = if face < 6 {
                vec![CubeMove { face, dir, elapsed: 0 }]
            } else {
                Vec::new()
            };
            CubeEvent::Move { serial: serial as u8, moves }
        }
        0x02 => {
            // FACELETS: cubie layout identical to Gen2 §3.4 body.
            let _pad = r.read_bits(6);
            let state = read_cube_state(&mut r);
            let facelets = to_facelet_string(&state);
            CubeEvent::Facelets { serial: 0, facelets: facelets_to_indices(&facelets) }
        }
        0x10 => {
            let level = r.read_bits(8).min(100) as u8;
            CubeEvent::Battery { serial: 0, level }
        }
        _ => CubeEvent::Other { kind: mode, raw: data.to_vec() },
    }
}

// === GAN Gen4 (V3) decoder (cf. scripts/proxy/gan.py) ===

/// Parse a decrypted GAN Gen4 (V3) message (repeated 72-bit move records).
pub fn decode_gen4(data: &[u8]) -> CubeEvent {
    let mut r = BitReader::new(data);
    let mode = r.read_bits(8) as u8;
    let length = r.read_bits(8) as u8;
    match mode {
        0x01 => {
            // One or more 72-bit records; each starts with 0x01.
            let mut moves = Vec::new();
            let mut serial = 0u8;
            for _ in 0..length.max(1) {
                let rec_kind = r.read_bits(8) as u8;
                if rec_kind != 0x01 {
                    break;
                }
                let t0 = r.read_bits(8);
                let t1 = r.read_bits(8);
                let t2 = r.read_bits(8);
                let t3 = r.read_bits(8);
                let _ts = (t3 << 24) | (t2 << 16) | (t1 << 8) | t0;
                let s_lo = r.read_bits(8);
                let s_hi = r.read_bits(8);
                serial = ((s_hi << 8) | s_lo) as u8;
                let _pad = r.read_bits(6) as u8;
                let pow_ = r.read_bits(2) as u8;
                let mask = r.read_bits(6) as u8;
                let face = decode_gen_axis(mask, &GEN3_AXIS_MASKS).unwrap_or(6);
                if face < 6 {
                    moves.push(CubeMove { face, dir: if pow_ == 2 { 1 } else { 0 }, elapsed: 0 });
                }
            }
            CubeEvent::Move { serial, moves }
        }
        0xED => {
            let state = read_cube_state(&mut r);
            let facelets = to_facelet_string(&state);
            CubeEvent::Facelets { serial: 0, facelets: facelets_to_indices(&facelets) }
        }
        0xEF => {
            let level = r.read_bits(8).min(100) as u8;
            CubeEvent::Battery { serial: 0, level }
        }
        0xEC => CubeEvent::Other { kind: 0xEC, raw: data.to_vec() }, // gyro
        _ => CubeEvent::Other { kind: mode, raw: data.to_vec() },
    }
}

// === MoYu WCU_MY3 decoder (cf. scripts/proxy/moyu32.py) ===

/// Decode a MY3 163 facelet snapshot. Facelets occupy bits 8..152 as 6 faces ×
/// 8 stickers × 3 bits, in wire face order `[2,5,0,3,4,1]`; the center is
/// implied at sticker j==3. Colours are indexed into "FBUDLR".
fn decode_moyu_facelets(data: &[u8]) -> [u8; 54] {
    const FACES: [usize; 6] = [2, 5, 0, 3, 4, 1];
    const COLS: &[u8; 6] = b"FBUDLR";
    let mut r = BitReader::new(data);
    let _ = r.read_bits(8); // skip opcode
    // Pull the 144 facelet bits (6×24) into a flat array first.
    let mut bits = [0u8; 144];
    for b in bits.iter_mut() {
        *b = r.read_bits(1) as u8;
    }
    let read3 = |bits: &[u8; 144], off: usize| -> u8 {
        (bits[off] << 2) | (bits[off + 1] << 1) | bits[off + 2]
    };
    let mut facelets = [b'U'; 54];
    let mut pos = 0usize;
    for i in 0..6 {
        let wire_face = FACES[i];
        for j in 0..8 {
            let v = read3(&bits, wire_face * 24 + j * 3) as usize;
            facelets[pos] = COLS[v.min(5)];
            pos += 1;
            if j == 3 {
                facelets[pos] = COLS[wire_face];
                pos += 1;
            }
        }
    }
    facelets
}

/// Parse a decrypted 20-byte MoYu WCU_MY3 frame.
pub fn decode_moyu_my3(data: &[u8]) -> CubeEvent {
    if data.is_empty() {
        return CubeEvent::Other { kind: 0, raw: data.to_vec() };
    }
    match data[0] {
        163 => {
            let facelets = decode_moyu_facelets(data);
            let mut r = BitReader::new(data);
            let _ = r.read_bits(152);
            let serial = r.read_bits(8) as u8;
            CubeEvent::Facelets { serial, facelets: facelets_to_indices(&facelets) }
        }
        164 => CubeEvent::Battery { serial: 0, level: data[1].min(100) },
        165 => {
            // Move: serial at bits 88..96; 5 slots (elapsed 16b at 8+i*16, code
            // 5b at 96+i*5). code >= 12 invalid.
            let mut r = BitReader::new(data);
            let _ = r.read_bits(88);
            let serial = r.read_bits(8) as u8;
            let mut codes = [(0u8, 0u16); 5];
            for slot in codes.iter_mut() {
                let elapsed = r.read_bits(16) as u16;
                let code = r.read_bits(5) as u8;
                *slot = (code, elapsed);
            }
            let mut moves = Vec::new();
            for &(code, elapsed) in codes.iter() {
                if code >= 12 {
                    break;
                }
                let dev_face = (code >> 1) as usize;
                let dir = code & 1;
                // 'FBUDLR'[dev_face] -> URFDLB index.
                let ch = *b"FBUDLR".get(dev_face).unwrap_or(&b'U');
                let face = match ch {
                    b'U' => 0, b'R' => 1, b'F' => 2, b'D' => 3, b'L' => 4, _ => 5,
                };
                moves.push(CubeMove { face, dir, elapsed });
            }
            CubeEvent::Move { serial, moves }
        }
        171 => CubeEvent::Other { kind: 171, raw: data.to_vec() }, // gyro
        other => CubeEvent::Other { kind: other, raw: data.to_vec() },
    }
}

// === QiYi decoder (cf. scripts/proxy/qiyi.py + cubebridge) ===

const QIYI_AXIS_MAP: [u8; 6] = [4, 1, 3, 0, 2, 5];

/// Parse a decrypted (AES-ECB) QiYi frame; the caller must have validated the
/// `0xFE` magic and CRC16-Modbus.
pub fn decode_qiyi(data: &[u8]) -> CubeEvent {
    if data.len() < 3 || data[0] != 0xFE {
        return CubeEvent::Other { kind: 0, raw: data.to_vec() };
    }
    let opcode = data[2];
    match opcode {
        0x02 | 0x04 => {
            // State frame: 54 nibbles at offset 7, battery at offset 35.
            const CENTER_FACELET: [usize; 6] = [4, 13, 22, 31, 40, 49];
            let mut facelets = [b'U'; 54];
            let std_colors = [b'U', b'R', b'F', b'D', b'L', b'B'];
            for i in 0..6 {
                facelets[CENTER_FACELET[i]] = std_colors[i];
            }
            if data.len() >= 38 {
                // Nibble order is low-nibble-first; colours indexed by "LRDUFB".
                const QIYI_COLORS: [u8; 6] = [b'L', b'R', b'D', b'U', b'F', b'B'];
                for i in 0..54 {
                    let byte = data[7 + (i >> 1)];
                    let nib = if i % 2 == 0 { byte & 0x0F } else { byte >> 4 };
                    facelets[i] = QIYI_COLORS.get(nib as usize).copied().unwrap_or(b'U');
                }
            }
            CubeEvent::Facelets { serial: 0, facelets: facelets_to_indices(&facelets) }
        }
        0x03 => {
            // State-change: 11 history slots at offset 36, 5 bytes each.
            let mut moves = Vec::new();
            let mut last_ts = 0u32;
            for i in 0..11 {
                let base = 36 + i * 5;
                if base + 4 >= data.len() {
                    break;
                }
                if data[base] == 0xFF && data[base + 1] == 0xFF {
                    continue;
                }
                let ts = u32::from_be_bytes([
                    data[base],
                    data[base + 1],
                    data[base + 2],
                    data[base + 3],
                ]);
                let code = data[base + 4];
                if code < 1 || code > 12 {
                    continue;
                }
                let axis = QIYI_AXIS_MAP[((code - 1) >> 1) as usize];
                let dir = code & 1;
                let elapsed = ts.saturating_sub(last_ts).min(u16::MAX as u32) as u16;
                last_ts = ts;
                moves.push(CubeMove { face: axis, dir, elapsed });
            }
            CubeEvent::Move { serial: 0, moves }
        }
        0xCC => CubeEvent::Other { kind: 0xCC, raw: data.to_vec() }, // gyro
        other => CubeEvent::Other { kind: other, raw: data.to_vec() },
    }
}

impl CubeEvent {
    /// Convenience: battery level if this is a Battery event.
    pub fn battery_level(&self) -> Option<u8> {
        match self {
            CubeEvent::Battery { level, .. } => Some(*level),
            _ => None,
        }
    }
}

// === MoYu MHC decoder (cf. cubebridge/main/cube_client.c) ===

const MHC_AXIS_MAP: [u8; 6] = [3, 4, 5, 1, 2, 0];

/// Parse a plaintext MoYu MHC notification: `d[0]` = count, then `count`
/// 6-byte records of {ts[4], face, angular-delta}.
pub fn decode_mhc(data: &[u8]) -> CubeEvent {
    if data.is_empty() {
        return CubeEvent::Other { kind: 0, raw: data.to_vec() };
    }
    let count = data[0] as usize;
    let mut moves = Vec::new();
    for i in 0..count {
        let m = 1 + i * 6;
        if m + 5 >= data.len() {
            break;
        }
        let face_dev = data[m + 4];
        if face_dev >= 6 {
            break;
        }
        let delta = data[m + 5] as i8;
        let dir = if delta >= 0 { 0 } else { 1 };
        let face = MHC_AXIS_MAP[face_dev as usize];
        moves.push(CubeMove { face, dir, elapsed: 0 });
    }
    CubeEvent::Move { serial: 0, moves }
}

/// Decode a notification for the given input protocol (undecrypted input must
/// be decrypted by the caller first — except MHC which is plaintext).
pub fn decode_event(proto: Protocol, data: &[u8]) -> CubeEvent {
    match proto {
        Protocol::Gen2 => decode_gen2(data),
        Protocol::Gen3 => decode_gen3(data),
        Protocol::Gen4 => decode_gen4(data),
        Protocol::MoYu32 => decode_moyu_my3(data),
        Protocol::QiYi => decode_qiyi(data),
        // MHC shares the MoYu slot in the enum for now; chosen by the caller.
        Protocol::Gan251 => decode_gen4(data),
        _ => CubeEvent::Other { kind: 0, raw: data.to_vec() },
    }
}
