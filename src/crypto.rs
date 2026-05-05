use esp_idf_svc::sys;

fn aes_cbc_block(key: &[u8; 16], iv: &[u8; 16], block: &mut [u8], encrypt: bool) {
    unsafe {
        let mut ctx: sys::esp_aes_context = core::mem::zeroed();
        sys::esp_aes_init(&mut ctx);
        sys::esp_aes_setkey(&mut ctx, key.as_ptr(), 128);
        let mut iv_buf = *iv;
        if encrypt {
            sys::esp_aes_crypt_cbc(
                &mut ctx,
                sys::ESP_AES_ENCRYPT as i32,
                block.len(),
                iv_buf.as_mut_ptr(),
                block.as_ptr(),
                block.as_mut_ptr(),
            );
        } else {
            sys::esp_aes_crypt_cbc(
                &mut ctx,
                sys::ESP_AES_DECRYPT as i32,
                block.len(),
                iv_buf.as_mut_ptr(),
                block.as_ptr(),
                block.as_mut_ptr(),
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

pub fn encrypt_msg(data: &mut [u8; 20], key: &[u8; 16], iv: &[u8; 16]) {
    aes_cbc_block(key, iv, &mut data[0..16], true);
    aes_cbc_block(key, iv, &mut data[4..20], true);
}

pub fn decrypt_msg(data: &mut [u8; 20], key: &[u8; 16], iv: &[u8; 16]) {
    aes_cbc_block(key, iv, &mut data[4..20], false);
    aes_cbc_block(key, iv, &mut data[0..16], false);
}
