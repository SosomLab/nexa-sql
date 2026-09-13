//! 로컬 봉투 — **nexa-clip `crates/nclip-store/src/sealed.rs` 이식**(계열 공통 문법 · 09-13).
//!
//! ChaCha20-Poly1305 + **도메인 분리 파생 키** — 한 프로필의 비밀번호 봉투를 다른 프로필
//! 자리로 바꿔치기해도 열리지 않는다(도메인이 키 유도와 AAD 양쪽에 들어간다).
//!
//! 형식: `"NSSE"` ‖ ver(1) ‖ salt(16) ‖ nonce(12) ‖ ct(+tag16)
//! 키: SHA-256("nexa-sql/dr-22/sealed-v1" ‖ domain ‖ salt ‖ secret)
//!
//! 실패는 전부 **None/Err = fail-closed** — 손상·바꿔치기·다른 키의 봉투는
//! 평문인 척 통과하지 않는다.

use std::io;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};

const MAGIC: [u8; 4] = *b"NSSE";
const VER: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEAD: usize = 4 + 1 + SALT_LEN;

fn derive_key(domain: &[u8], salt: &[u8; SALT_LEN], secret: &[u8; 32]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"nexa-sql/dr-22/sealed-v1");
    h.update(domain);
    h.update(salt);
    h.update(secret);
    h.finalize().into()
}

/// 봉인 여부.
#[must_use]
pub(crate) fn is_sealed(bytes: &[u8]) -> bool {
    bytes.len() > HEAD + NONCE_LEN && bytes[..4] == MAGIC
}

/// 봉인 — salt·nonce는 매 호출 새로 뽑는다(같은 평문도 매번 다른 봉투).
pub(crate) fn seal(domain: &[u8], secret: &[u8; 32], plain: &[u8]) -> io::Result<Vec<u8>> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    getrandom::getrandom(&mut salt)
        .and_then(|()| getrandom::getrandom(&mut nonce))
        .map_err(|_| io::Error::other("OS 난수 실패"))?;
    let mut out = Vec::with_capacity(HEAD + NONCE_LEN + plain.len() + 16);
    out.extend_from_slice(&MAGIC);
    out.push(VER);
    out.extend_from_slice(&salt);
    let mut aad = out.clone();
    aad.extend_from_slice(domain);
    let ct = ChaCha20Poly1305::new(&derive_key(domain, &salt, secret).into())
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plain,
                aad: &aad,
            },
        )
        .map_err(|_| io::Error::other("AEAD 봉인 실패"))?;
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// 개봉 — 형식·버전·태그 어느 하나라도 어긋나면 None(fail-closed).
#[must_use]
pub(crate) fn open(domain: &[u8], secret: &[u8; 32], bytes: &[u8]) -> Option<Vec<u8>> {
    if !is_sealed(bytes) || bytes[4] != VER {
        return None;
    }
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&bytes[5..HEAD]);
    let mut aad = bytes[..HEAD].to_vec();
    aad.extend_from_slice(domain);
    ChaCha20Poly1305::new(&derive_key(domain, &salt, secret).into())
        .decrypt(
            Nonce::from_slice(&bytes[HEAD..HEAD + NONCE_LEN]),
            Payload {
                msg: &bytes[HEAD + NONCE_LEN..],
                aad: &aad,
            },
        )
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: [u8; 32] = [7u8; 32];

    #[test]
    fn roundtrip_and_fresh_envelope_every_time() {
        let a = seal(b"profile-v1/dev", &S, b"hello").unwrap();
        let b = seal(b"profile-v1/dev", &S, b"hello").unwrap();
        assert_ne!(a, b, "salt·nonce 신선 — 같은 평문도 봉투가 다르다");
        assert!(is_sealed(&a));
        assert_eq!(open(b"profile-v1/dev", &S, &a).unwrap(), b"hello");
    }

    /// 도메인 분리 — dev 프로필의 봉투를 prod 자리로 옮겨도 열리지 않는다(바꿔치기 차단).
    #[test]
    fn wrong_domain_or_secret_fails_closed() {
        let a = seal(b"profile-v1/dev", &S, b"x").unwrap();
        assert!(open(b"profile-v1/prod", &S, &a).is_none(), "도메인 분리");
        assert!(open(b"profile-v1/dev", &[8u8; 32], &a).is_none(), "다른 키");
    }

    #[test]
    fn tamper_fails_closed() {
        let mut a = seal(b"profile-v1/dev", &S, b"secret").unwrap();
        let last = a.len() - 1;
        a[last] ^= 1;
        assert!(open(b"profile-v1/dev", &S, &a).is_none(), "태그 검증");
        assert!(!is_sealed(b"plain old bytes"), "평문 판정");
    }
}
