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
            let payload = build_move_qiyi(serial, history);
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
    w.write_bits(2, 8); // sw major
    w.write_bits(0, 8); // sw minor
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

const QIYI_FACE_ORDER: [usize; 6] = [3, 1, 4, 0, 2, 5];

pub fn build_facelets_qiyi(state: &CubeState) -> Vec<u8> {
    let color_chars = [0u8, 1, 2, 3, 4, 5];
    let mut facelets = [0u8; 54];
    for i in 0..6 { facelets[CENTER_FACELET[i]] = color_chars[i]; }
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
    //   [0x02][ts:4 BE][facelets:27][battery][pad]  == 34 content bytes
    // The App parses: role=data[2], ts=data[3..7], facelets=data[7..34], battery=data[34].
    let mut content: Vec<u8> = Vec::with_capacity(34);
    content.push(0x02);
    content.extend_from_slice(&(serial as u32).to_be_bytes());
    for i in 0..27 {
        content.push(facelets.get(i).copied().unwrap_or(0));
    }
    content.push(battery.min(100));
    content.push(0);
    content
}

pub fn build_move_qiyi(serial: u16, history: &[(u8, u8, u16)]) -> Vec<u8> {
    // State-change record (role 0x03), csTimer-compatible, 92 content bytes -> frame len 0x60.
    //   [0x03][ts:4 BE][facelets:27(unused)][firstMove] ... history entries (ts:4 BE + mv)
    let axis_map = [4u8, 1, 3, 0, 2, 5];
    let encode = |face: u8, dir: u8| -> u8 { ((axis_map[(face as usize) & 0x7] << 1) | (dir & 1)) + 1 };

    let mut c = vec![0u8; 92];
    c[0] = 0x03;
    c[1..5].copy_from_slice(&(serial as u32).to_be_bytes());
    if let Some(&(face, dir, _)) = history.first() {
        c[32] = encode(face, dir);
    }
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
    if data.is_empty() { return IncomingCommand::Unknown; }
    match data[0] {
        // Smart Player Pro bind handshake: `FC 2A <flag> <ts4> <userId> <mac6> <crc>`.
        // Replying with the current cube state lets the App observe the device.
        0xFC => IncomingCommand::Facelets,
        // Standard QiYi frame: `FE <len> <role> <payload...> <crc16>`.
        0xFE => {
            if data.len() < 3 { return IncomingCommand::Unknown; }
            match data[2] {
                0x02 | 0x05 => IncomingCommand::Facelets, // 配对后首条 / 查询
                0x03 => IncomingCommand::MoveHistory {
                    start_serial: data.get(3).copied().unwrap_or(0),
                    count: 0,
                },
                _ => IncomingCommand::Unknown,
            }
        }
        _ => IncomingCommand::Unknown,
    }
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
