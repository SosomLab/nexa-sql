//! DES(FIPS 46-3) 블록 암호 + CBC + PKCS#5 — DRDA EUSRIDPWD(SECMEC 9)의 사용자·비밀번호 암호화에만 쓴다(외부 crate 0 · 자체 구현).
//!
//! 표(IP · FP · E · P · S1~S8 · PC-1 · PC-2 · 회전)는 FIPS 46-3 그대로. 검증 = 알려진 벡터 2개(`tests`).

const IP: [u8; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6,
    64, 56, 48, 40, 32, 24, 16, 8, 57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61,
    53, 45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7,
];
const FP: [u8; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30,
    37, 5, 45, 13, 53, 21, 61, 29, 36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27,
    34, 2, 42, 10, 50, 18, 58, 26, 33, 1, 41, 9, 49, 17, 57, 25,
];
const E: [u8; 48] = [
    32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9, 8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17, 16, 17, 18,
    19, 20, 21, 20, 21, 22, 23, 24, 25, 24, 25, 26, 27, 28, 29, 28, 29, 30, 31, 32, 1,
];
const P: [u8; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9, 19,
    13, 30, 6, 22, 11, 4, 25,
];
const PC1: [u8; 56] = [
    57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59, 51, 43, 35, 27, 19, 11, 3, 60,
    52, 44, 36, 63, 55, 47, 39, 31, 23, 15, 7, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29,
    21, 13, 5, 28, 20, 12, 4,
];
const PC2: [u8; 48] = [
    14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10, 23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2, 41, 52,
    31, 37, 47, 55, 30, 40, 51, 45, 33, 48, 44, 49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32,
];
const SHIFTS: [u8; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];
const S: [[u8; 64]; 8] = [
    [
        14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7, 0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12,
        11, 9, 5, 3, 8, 4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0, 15, 12, 8, 2, 4, 9,
        1, 7, 5, 11, 3, 14, 10, 0, 6, 13,
    ],
    [
        15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10, 3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1,
        10, 6, 9, 11, 5, 0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15, 13, 8, 10, 1, 3, 15,
        4, 2, 11, 6, 7, 12, 0, 5, 14, 9,
    ],
    [
        10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8, 13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5,
        14, 12, 11, 15, 1, 13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7, 1, 10, 13, 0, 6,
        9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12,
    ],
    [
        7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15, 13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2,
        12, 1, 10, 14, 9, 10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4, 3, 15, 0, 6, 10, 1,
        13, 8, 9, 4, 5, 11, 12, 7, 2, 14,
    ],
    [
        2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9, 14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15,
        10, 3, 9, 8, 6, 4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14, 11, 8, 12, 7, 1, 14,
        2, 13, 6, 15, 0, 9, 10, 4, 5, 3,
    ],
    [
        12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11, 10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13,
        14, 0, 11, 3, 8, 9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6, 4, 3, 2, 12, 9, 5,
        15, 10, 11, 14, 1, 7, 6, 0, 8, 13,
    ],
    [
        4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1, 13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5,
        12, 2, 15, 8, 6, 1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2, 6, 11, 13, 8, 1, 4,
        10, 7, 9, 5, 0, 15, 14, 2, 3, 12,
    ],
    [
        13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7, 1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6,
        11, 0, 14, 9, 2, 7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8, 2, 1, 14, 7, 4, 10,
        8, 13, 15, 12, 9, 0, 3, 5, 6, 11,
    ],
];

/// `table[i]` = 출력 비트 i(1부터 · MSB 먼저)가 입력의 몇 번째 비트(1부터)인가.
fn permute(input: u64, in_bits: u32, table: &[u8]) -> u64 {
    let mut out = 0u64;
    for &pos in table {
        out <<= 1;
        out |= (input >> (in_bits - u32::from(pos))) & 1;
    }
    out
}

fn subkeys(key: u64) -> [u64; 16] {
    let k56 = permute(key, 64, &PC1);
    let mut c = (k56 >> 28) & 0x0FFF_FFFF;
    let mut d = k56 & 0x0FFF_FFFF;
    let mut ks = [0u64; 16];
    for (i, &sh) in SHIFTS.iter().enumerate() {
        c = ((c << sh) | (c >> (28 - sh))) & 0x0FFF_FFFF;
        d = ((d << sh) | (d >> (28 - sh))) & 0x0FFF_FFFF;
        ks[i] = permute((c << 28) | d, 56, &PC2);
    }
    ks
}

fn feistel(r: u32, k: u64) -> u32 {
    let x = permute(u64::from(r), 32, &E) ^ k;
    let mut out = 0u32;
    for (i, sbox) in S.iter().enumerate() {
        let six = ((x >> (42 - 6 * i)) & 0x3F) as u8;
        let row = ((six & 0x20) >> 4) | (six & 1);
        let col = (six >> 1) & 0x0F;
        out = (out << 4) | u32::from(sbox[(row * 16 + col) as usize]);
    }
    permute(u64::from(out), 32, &P) as u32
}

fn block(input: u64, ks: &[u64; 16], decrypt: bool) -> u64 {
    let x = permute(input, 64, &IP);
    let mut l = (x >> 32) as u32;
    let mut r = x as u32;
    for i in 0..16 {
        let k = if decrypt { ks[15 - i] } else { ks[i] };
        let nl = r;
        r = l ^ feistel(r, k);
        l = nl;
    }
    permute((u64::from(r) << 32) | u64::from(l), 64, &FP)
}

/// DES/CBC/PKCS#5 암호화 — `key`·`iv` 8바이트.
#[must_use]
pub fn encrypt_cbc_pkcs5(key: [u8; 8], iv: [u8; 8], data: &[u8]) -> Vec<u8> {
    let ks = subkeys(u64::from_be_bytes(key));
    let pad = 8 - data.len() % 8;
    let mut buf = data.to_vec();
    buf.extend(std::iter::repeat_n(pad as u8, pad));
    let mut prev = u64::from_be_bytes(iv);
    let mut out = Vec::with_capacity(buf.len());
    for chunk in buf.as_chunks::<8>().0 {
        let x = u64::from_be_bytes(*chunk) ^ prev;
        let c = block(x, &ks, false);
        out.extend_from_slice(&c.to_be_bytes());
        prev = c;
    }
    out
}

/// DES/CBC/PKCS#5 복호화(시험·대칭 확인용).
#[must_use]
pub fn decrypt_cbc_pkcs5(key: [u8; 8], iv: [u8; 8], data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || !data.len().is_multiple_of(8) {
        return None;
    }
    let ks = subkeys(u64::from_be_bytes(key));
    let mut prev = u64::from_be_bytes(iv);
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.as_chunks::<8>().0 {
        let c = u64::from_be_bytes(*chunk);
        let p = block(c, &ks, true) ^ prev;
        out.extend_from_slice(&p.to_be_bytes());
        prev = c;
    }
    let pad = *out.last()? as usize;
    if pad == 0 || pad > 8 || pad > out.len() {
        return None;
    }
    out.truncate(out.len() - pad);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FIPS 46-3 보기(키 133457799BBCDFF1 · 평문 0123456789ABCDEF → 85E813540F0AB405) · 전부 0 → 8CA64DE9C1B123A7 · 복호 대칭.
    #[test]
    fn des_known_vectors_and_cbc_round_trip() {
        let ks = subkeys(0x1334_5779_9BBC_DFF1);
        assert_eq!(
            block(0x0123_4567_89AB_CDEF, &ks, false),
            0x85E8_1354_0F0A_B405
        );
        assert_eq!(
            block(0x85E8_1354_0F0A_B405, &ks, true),
            0x0123_4567_89AB_CDEF
        );
        let z = subkeys(0);
        assert_eq!(block(0, &z, false), 0x8CA6_4DE9_C1B1_23A7);
        let key = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];
        let iv = [0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54, 0x32, 0x10];
        for msg in [&b""[..], b"A", b"ABCDEFGH", "db2inst1 비밀번호".as_bytes()] {
            let c = encrypt_cbc_pkcs5(key, iv, msg);
            assert_eq!(c.len() % 8, 0);
            assert!(c.len() > msg.len());
            assert_eq!(decrypt_cbc_pkcs5(key, iv, &c).unwrap(), msg);
        }
    }
}
