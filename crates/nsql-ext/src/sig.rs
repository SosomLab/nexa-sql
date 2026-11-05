//! 색인 서명 **검증**(D-373 · T-166 E8): 사이드카 `index.sig` = `ed25519 <key-id> <hex128>`(nexa-license-tool `sign-file index.json`이
//! 만드는 꼴 그대로 · `.sha256` 사이드카는 선택) · 공개키 = `.pub`(nexa-license `nxp1` key=value · base32). 서명 생성은 여기 없다.
//! 서명이 덮는 것 = `index.json` 바이트 전부 — 패키지 메타·파일은 색인 행(버전)과 `extension.json`의 sha256 사슬로 묶인다(패키지
//! `extension.json`의 sha256을 색인에 넣는 D-462 후보는 후속).

use nexa_license::ed25519::Ed25519Verifier;
use nexa_license::format::Doc;
use nexa_license::types::Alg;
use nexa_license::verify::SigVerifier;

/// 사이드카 파일 이름(루트 `index.json` 옆).
pub const SIG_FILE: &str = "index.sig";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SigError {
    /// 사이드카가 없거나 비었다.
    Missing,
    /// `ed25519 <key-id> <hex128>` 꼴이 아니다.
    Malformed,
    /// 공개키 목록에 그 key-id가 없다.
    UnknownKey(String),
    /// 서명 불일치(색인이 바뀌었거나 다른 키).
    Invalid(String),
}

/// `.pub` 본문 → (key id · 공개키 32B). nexa-license `sign::parse_public_key_file`과 같은 판정(issuer feature 없이).
#[must_use]
pub fn parse_public_key_file(text: &str) -> Option<(String, [u8; 32])> {
    let d = Doc::parse(text);
    if d.get("format") != Some("nxp1") || d.get("alg").is_some_and(|a| a != "ed25519") {
        return None;
    }
    let pk = nexa_license::base32::decode(d.get("public")?)?;
    let pk: [u8; 32] = pk.try_into().ok()?;
    Some((d.get("key").unwrap_or("").to_string(), pk))
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// 사이드카 한 줄 → (key id · 서명 64B).
pub fn parse_sig(text: &str) -> Result<(String, [u8; 64]), SigError> {
    if text.trim().is_empty() {
        return Err(SigError::Missing);
    }
    let toks: Vec<&str> = text.split_whitespace().collect();
    match toks.as_slice() {
        [alg, id, sig] if alg.eq_ignore_ascii_case("ed25519") => {
            let bytes = unhex(sig).ok_or(SigError::Malformed)?;
            let sig: [u8; 64] = bytes.try_into().map_err(|_| SigError::Malformed)?;
            Ok((id.to_string(), sig))
        }
        _ => Err(SigError::Malformed),
    }
}

/// 색인 바이트의 서명을 공개키 목록으로 검증 — 통과하면 쓰인 key id.
pub fn verify_index(
    index: &[u8],
    sig_text: &str,
    keys: &[(String, [u8; 32])],
) -> Result<String, SigError> {
    let (id, sig) = parse_sig(sig_text)?;
    let Some((_, pk)) = keys.iter().find(|(k, _)| *k == id) else {
        return Err(SigError::UnknownKey(id));
    };
    if Ed25519Verifier.verify(Alg::Ed25519, pk, index, &sig) {
        Ok(id)
    } else {
        Err(SigError::Invalid(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// 시험 전용 키쌍(발급 코드가 아니다): 서명 → 검증 ✓ · 바이트 바뀜 ✗ · 모르는 키 ✗ · 꼴 오류 ✗ · `.pub` 왕복.
    #[test]
    fn sign_verify_roundtrip_and_failures() {
        let sk = SigningKey::generate(&mut rand_core::OsRng);
        let pk = sk.verifying_key().to_bytes();
        let index = br#"{"format":1,"packages":[]}"#;
        let sig = sk.sign(index).to_bytes();
        let line = format!("ed25519 k1 {}\n", hex(&sig));
        let keys = vec![("k1".to_string(), pk)];
        assert_eq!(verify_index(index, &line, &keys), Ok("k1".into()));
        assert_eq!(
            verify_index(br#"{"format":1,"packages":[ ]}"#, &line, &keys),
            Err(SigError::Invalid("k1".into()))
        );
        assert_eq!(
            verify_index(index, &line, &[("k2".to_string(), pk)]),
            Err(SigError::UnknownKey("k1".into()))
        );
        assert_eq!(verify_index(index, "", &keys), Err(SigError::Missing));
        assert_eq!(
            verify_index(index, "rsa k1 00", &keys),
            Err(SigError::Malformed)
        );
        assert_eq!(
            verify_index(index, "ed25519 k1 zz", &keys),
            Err(SigError::Malformed)
        );
        // .pub 파일 꼴(nexa-license nxp1).
        let pub_text = format!(
            "format=nxp1\nkey=k1\nalg=ed25519\npublic={}\n",
            nexa_license::base32::encode(&pk)
        );
        let (id, parsed) = parse_public_key_file(&pub_text).expect("pub");
        assert_eq!((id.as_str(), parsed), ("k1", pk));
        assert!(parse_public_key_file("format=nxl1\n").is_none());
    }
}
