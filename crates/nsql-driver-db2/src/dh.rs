//! 256비트 Diffie-Hellman(DRDA EUSRIDPWD · SECMEC 9) — p·g = Derby/pydrda/JCC가 쓰는 고정값. 큰 수 산술 = `[u64; 8]`(512비트) 자체 구현.
//!
//! 공개키 = g^x mod p(32바이트 큰 끝 · SECTKN) · 공유 비밀 = Y^x mod p → DES 키 = 비밀[12..20) · IV = 서버 공개키[12..20)(pydrda `secmec9`).

/// p(256비트).
pub const P_HEX: &str = "C62112D73EE613F0947AB31F0F6846A1BFF5B3A4CA0D60BC1E4C7A0D8C16B3E3";
/// g.
pub const G_HEX: &str = "4690FA1F7B9E1D4442C86C9114603FDECF071EDCEC5F626E21E256AED9EA34E4";

/// 512비트 부호 없는 정수(작은 쪽 limb부터).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U512([u64; 8]);

impl U512 {
    pub const ZERO: U512 = U512([0; 8]);

    #[must_use]
    pub fn from_be_bytes(b: &[u8]) -> U512 {
        let mut v = [0u64; 8];
        for (i, &x) in b.iter().rev().enumerate() {
            if i / 8 < 8 {
                v[i / 8] |= u64::from(x) << (8 * (i % 8));
            }
        }
        U512(v)
    }

    #[must_use]
    pub fn from_hex(h: &str) -> U512 {
        let bytes: Vec<u8> = (0..h.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0))
            .collect();
        U512::from_be_bytes(&bytes)
    }

    /// 하위 32바이트(256비트) 큰 끝.
    #[must_use]
    pub fn to_be_bytes_32(self) -> [u8; 32] {
        let mut out = [0u8; 32];
        for i in 0..32 {
            out[31 - i] = (self.0[i / 8] >> (8 * (i % 8))) as u8;
        }
        out
    }

    fn is_zero(&self) -> bool {
        self.0.iter().all(|&x| x == 0)
    }

    fn bit(&self, i: usize) -> bool {
        (self.0[i / 64] >> (i % 64)) & 1 == 1
    }

    fn bits(&self) -> usize {
        for i in (0..8).rev() {
            if self.0[i] != 0 {
                return i * 64 + (64 - self.0[i].leading_zeros() as usize);
            }
        }
        0
    }

    fn cmp_u(&self, o: &U512) -> std::cmp::Ordering {
        for i in (0..8).rev() {
            if self.0[i] != o.0[i] {
                return self.0[i].cmp(&o.0[i]);
            }
        }
        std::cmp::Ordering::Equal
    }

    fn shl1(&mut self) -> bool {
        let mut carry = 0u64;
        for x in &mut self.0 {
            let nc = *x >> 63;
            *x = (*x << 1) | carry;
            carry = nc;
        }
        carry == 1
    }

    fn add_assign(&mut self, o: &U512) -> bool {
        let mut carry = 0u128;
        for i in 0..8 {
            let s = u128::from(self.0[i]) + u128::from(o.0[i]) + carry;
            self.0[i] = s as u64;
            carry = s >> 64;
        }
        carry != 0
    }

    fn sub_assign(&mut self, o: &U512) {
        let mut borrow = 0i128;
        for i in 0..8 {
            let d = i128::from(self.0[i]) - i128::from(o.0[i]) - borrow;
            if d < 0 {
                self.0[i] = (d + (1i128 << 64)) as u64;
                borrow = 1;
            } else {
                self.0[i] = d as u64;
                borrow = 0;
            }
        }
    }

    /// (a + b) mod m — a·b < m 전제.
    fn add_mod(a: &U512, b: &U512, m: &U512) -> U512 {
        let mut r = *a;
        let overflow = r.add_assign(b);
        if overflow || r.cmp_u(m) != std::cmp::Ordering::Less {
            r.sub_assign(m);
        }
        r
    }

    /// a·b mod m(이진 곱 · a·b < m 전제 · m < 2^511).
    fn mul_mod(a: &U512, b: &U512, m: &U512) -> U512 {
        let mut r = U512::ZERO;
        let n = b.bits();
        for i in (0..n).rev() {
            let mut d = r;
            let c = d.shl1();
            if c || d.cmp_u(m) != std::cmp::Ordering::Less {
                d.sub_assign(m);
            }
            r = d;
            if b.bit(i) {
                r = U512::add_mod(&r, a, m);
            }
        }
        r
    }

    /// base^exp mod m.
    #[must_use]
    pub fn pow_mod(base: &U512, exp: &U512, m: &U512) -> U512 {
        let mut result = U512([1, 0, 0, 0, 0, 0, 0, 0]);
        let mut b = *base;
        // base mod m
        while b.cmp_u(m) != std::cmp::Ordering::Less {
            b.sub_assign(m);
        }
        for i in 0..exp.bits() {
            if exp.bit(i) {
                result = U512::mul_mod(&result, &b, m);
            }
            b = U512::mul_mod(&b, &b, m);
        }
        result
    }
}

/// 비밀키(2 ≤ x < p) — OS 난수(`/dev/urandom`) · 없으면 시간·주소·pid를 SipHash(OS 시드)로 섞는다.
#[must_use]
pub fn random_private() -> U512 {
    let mut bytes = [0u8; 32];
    let mut ok = false;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        ok = f.read_exact(&mut bytes).is_ok();
    }
    if !ok {
        use std::hash::{BuildHasher, Hasher};
        let st = std::collections::hash_map::RandomState::new();
        for (i, chunk) in bytes.chunks_mut(8).enumerate() {
            let mut h = st.build_hasher();
            h.write_u64(i as u64);
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
            );
            h.write_u32(std::process::id());
            chunk.copy_from_slice(&h.finish().to_le_bytes());
        }
    }
    bytes[0] &= 0x3F; // < p 보장(p 최상위 바이트 0xC6)
    let mut x = U512::from_be_bytes(&bytes);
    if x.is_zero() || x.0[0] < 2 {
        x.0[0] |= 2;
    }
    x
}

/// 공개키(32바이트 큰 끝 · SECTKN).
#[must_use]
pub fn public_key(private: &U512) -> [u8; 32] {
    let p = U512::from_hex(P_HEX);
    let g = U512::from_hex(G_HEX);
    U512::pow_mod(&g, private, &p).to_be_bytes_32()
}

/// 공유 비밀(32바이트 큰 끝) — DES 키 = `[12..20)`.
#[must_use]
pub fn shared_secret(server_public: &[u8], private: &U512) -> [u8; 32] {
    let p = U512::from_hex(P_HEX);
    let y = U512::from_be_bytes(server_public);
    U512::pow_mod(&y, private, &p).to_be_bytes_32()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modpow_small_and_dh_agreement() {
        let m = U512([1_000_000_007, 0, 0, 0, 0, 0, 0, 0]);
        let b = U512([3, 0, 0, 0, 0, 0, 0, 0]);
        let e = U512([200, 0, 0, 0, 0, 0, 0, 0]);
        // 3^200 mod 1e9+7 = 파이썬 pow(3, 200, 10**9+7) = 674 232 097 (검증값 · 아래 교차 확인 식과 함께)
        let r = U512::pow_mod(&b, &e, &m);
        let mut chk = 1u64;
        for _ in 0..200 {
            chk = chk * 3 % 1_000_000_007;
        }
        assert_eq!(r.0[0], chk);
        // DH: g^a, g^b → (g^b)^a == (g^a)^b
        let a = random_private();
        let bb = random_private();
        assert_ne!(a, bb);
        let pa = public_key(&a);
        let pb = public_key(&bb);
        assert_eq!(shared_secret(&pb, &a), shared_secret(&pa, &bb));
        assert_eq!(U512::from_hex(P_HEX).bits(), 256);
    }
}
