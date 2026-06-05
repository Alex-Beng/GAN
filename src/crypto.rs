use esp_idf_svc::sys;

fn aes_cbc_block(key: &[u8; 16], iv: &[u8; 16], block: &mut [u8], encrypt: bool) {
    unsafe {
        let mut ctx: sys::esp_aes_context = core::mem::zeroed();
        sys::esp_aes_init(&mut ctx);
        sys::esp_aes_setkey(&mut ctx, key.as_ptr(), 128);
        let mut iv_buf = *iv;
        if encrypt {
            sys::esp_aes_crypt_cbc(
                &mut ctx, sys::ESP_AES_ENCRYPT as i32,
                block.len(), iv_buf.as_mut_ptr(),
                block.as_ptr(), block.as_mut_ptr(),
            );
        } else {
            sys::esp_aes_crypt_cbc(
                &mut ctx, sys::ESP_AES_DECRYPT as i32,
                block.len(), iv_buf.as_mut_ptr(),
                block.as_ptr(), block.as_mut_ptr(),
            );
        }
        sys::esp_aes_free(&mut ctx);
    }
}

fn aes_ecb_block(key: &[u8; 16], block: &mut [u8], encrypt: bool) {
    unsafe {
        let mut ctx: sys::esp_aes_context = core::mem::zeroed();
        sys::esp_aes_init(&mut ctx);
        sys::esp_aes_setkey(&mut ctx, key.as_ptr(), 128);
        if encrypt {
            sys::esp_aes_crypt_ecb(
                &mut ctx, sys::ESP_AES_ENCRYPT as i32,
                block.as_ptr(), block.as_mut_ptr(),
            );
        } else {
            sys::esp_aes_crypt_ecb(
                &mut ctx, sys::ESP_AES_DECRYPT as i32,
                block.as_ptr(), block.as_mut_ptr(),
            );
        }
        sys::esp_aes_free(&mut ctx);
    }
}

pub fn derive_key_iv(mac: &[u8; 6], base_key: &[u8; 16], base_iv: &[u8; 16]) -> ([u8; 16], [u8; 16]) {
    let salt = [mac[5], mac[4], mac[3], mac[2], mac[1], mac[0]];
    let mut key = *base_key;
    let mut iv = *base_iv;
    for i in 0..6 {
        key[i] = ((key[i] as u16 + salt[i] as u16) % 0xFF) as u8;
        iv[i] = ((iv[i] as u16 + salt[i] as u16) % 0xFF) as u8;
    }
    (key, iv)
}

pub fn encrypt_msg(data: &mut [u8], key: &[u8; 16], iv: &[u8; 16]) {
    let sz = 16.min(data.len());
    aes_cbc_block(key, iv, &mut data[..sz], true);
    if data.len() > 16 {
        let off = data.len() - 16;
        aes_cbc_block(key, iv, &mut data[off..], true);
    }
}

pub fn decrypt_msg(data: &mut [u8], key: &[u8; 16], iv: &[u8; 16]) {
    if data.len() > 16 {
        let off = data.len() - 16;
        aes_cbc_block(key, iv, &mut data[off..], false);
    }
    let sz = 16.min(data.len());
    aes_cbc_block(key, iv, &mut data[..sz], false);
}

// MoYu32: XOR first 16 bytes with IV, then ECB. If >16, also XOR & ECB last 16.
pub fn moyu32_encrypt(data: &mut [u8], key: &[u8; 16], iv: &[u8; 16]) {
    for i in 0..16.min(data.len()) { data[i] ^= iv[i]; }
    aes_ecb_block(key, &mut data[..16], true);
    if data.len() > 16 {
        let off = data.len() - 16;
        for i in 0..16 { data[off + i] ^= iv[i]; }
        aes_ecb_block(key, &mut data[off..], true);
    }
}

pub fn moyu32_decrypt(data: &mut [u8], key: &[u8; 16], iv: &[u8; 16]) {
    if data.len() > 16 {
        let off = data.len() - 16;
        aes_ecb_block(key, &mut data[off..], false);
        for i in 0..16 { data[off + i] ^= iv[i]; }
    }
    aes_ecb_block(key, &mut data[..16], false);
    for i in 0..16.min(data.len()) { data[i] ^= iv[i]; }
}

// QiYi: pure AES-128-ECB
pub fn aes_ecb_encrypt(data: &mut [u8], key: &[u8; 16]) {
    for chunk in data.chunks_mut(16) {
        if chunk.len() == 16 {
            aes_ecb_block(key, chunk, true);
        }
    }
}

pub fn aes_ecb_decrypt(data: &mut [u8], key: &[u8; 16]) {
    for chunk in data.chunks_mut(16) {
        if chunk.len() == 16 {
            aes_ecb_block(key, chunk, false);
        }
    }
}

// CRC16-MODBUS
pub fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= byte as u16;
        for _ in 0..8 {
            if crc & 0x0001 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

// CRC16-CCITT-FALSE (poly 0x1021, init 0xFFFF)
pub fn crc16_ccitt_false(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}
