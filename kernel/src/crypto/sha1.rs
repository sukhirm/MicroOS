//! SHA-1, HMAC-SHA1, PBKDF2, and PRF-512 Implementation (RFC 3174 / IEEE 802.11i).
//!
//! Pure no_std implementation optimized for deterministic WPA2 key derivation.

#![allow(dead_code)]

/// Computes the 20-byte SHA-1 digest for an arbitrary data slice.
pub fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h0 = 0x67452301u32;
    let mut h1 = 0xEFCDAB89u32;
    let mut h2 = 0x98BADCFEu32;
    let mut h3 = 0x10325476u32;
    let mut h4 = 0xC3D2E1F0u32;

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = alloc::vec::Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);

    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for i in 0..80 {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999u32),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1u32),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDCu32),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut result = [0u8; 20];
    result[0..4].copy_from_slice(&h0.to_be_bytes());
    result[4..8].copy_from_slice(&h1.to_be_bytes());
    result[8..12].copy_from_slice(&h2.to_be_bytes());
    result[12..16].copy_from_slice(&h3.to_be_bytes());
    result[16..20].copy_from_slice(&h4.to_be_bytes());
    result
}

/// Computes HMAC-SHA1 for arbitrary key and data.
pub fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    let mut key_block = [0u8; 64];
    if key.len() > 64 {
        let digest = sha1_digest(key);
        key_block[..20].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut o_key_pad = [0x5cu8; 64];
    let mut i_key_pad = [0x36u8; 64];
    for i in 0..64 {
        o_key_pad[i] ^= key_block[i];
        i_key_pad[i] ^= key_block[i];
    }

    let mut inner_data = alloc::vec::Vec::with_capacity(64 + data.len());
    inner_data.extend_from_slice(&i_key_pad);
    inner_data.extend_from_slice(data);
    let inner_hash = sha1_digest(&inner_data);

    let mut outer_data = alloc::vec::Vec::with_capacity(64 + 20);
    outer_data.extend_from_slice(&o_key_pad);
    outer_data.extend_from_slice(&inner_hash);
    sha1_digest(&outer_data)
}

/// Derives the 32-byte WPA2 Pairwise Master Key (PMK) using PBKDF2-HMAC-SHA1 (4096 rounds).
pub fn pbkdf2_sha1(passphrase: &[u8], ssid: &[u8], iterations: u32) -> [u8; 32] {
    let mut pmk = [0u8; 32];

    // Block 1 (bytes 0..20)
    let mut salt1 = alloc::vec::Vec::with_capacity(ssid.len() + 4);
    salt1.extend_from_slice(ssid);
    salt1.extend_from_slice(&1u32.to_be_bytes());

    let mut u = hmac_sha1(passphrase, &salt1);
    let mut f1 = u;

    for _ in 1..iterations {
        u = hmac_sha1(passphrase, &u);
        for i in 0..20 {
            f1[i] ^= u[i];
        }
    }

    // Block 2 (bytes 20..32)
    let mut salt2 = alloc::vec::Vec::with_capacity(ssid.len() + 4);
    salt2.extend_from_slice(ssid);
    salt2.extend_from_slice(&2u32.to_be_bytes());

    let mut u2 = hmac_sha1(passphrase, &salt2);
    let mut f2 = u2;

    for _ in 1..iterations {
        u2 = hmac_sha1(passphrase, &u2);
        for i in 0..20 {
            f2[i] ^= u2[i];
        }
    }

    pmk[0..20].copy_from_slice(&f1);
    pmk[20..32].copy_from_slice(&f2[0..12]);
    pmk
}

/// IEEE 802.11i PRF-512 (Pseudo-Random Function using HMAC-SHA1).
pub fn prf_512(key: &[u8], label: &[u8], context: &[u8]) -> [u8; 64] {
    let mut result = [0u8; 64];
    let total_len = 64usize;
    let mut offset = 0usize;
    let mut counter = 0u8;

    while offset < total_len {
        let mut data = alloc::vec::Vec::new();
        data.extend_from_slice(label);
        data.push(0x00);
        data.extend_from_slice(context);
        data.push(counter);

        let hash = hmac_sha1(key, &data);
        let copy_len = (total_len - offset).min(20);
        result[offset..offset + copy_len].copy_from_slice(&hash[0..copy_len]);

        offset += copy_len;
        counter += 1;
    }

    result
}
