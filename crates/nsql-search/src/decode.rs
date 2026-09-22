//! 파일 본문 준비 — 이진 판정(첫 8KB NUL) · UTF-8 BOM 제거 · UTF-16 BOM 변환(간단 디코더).
//!
//! EUC-KR 등 레거시 인코딩 판정은 T-79(`encoding_rs` 경로)와 함께 — 지금은 UTF-8/UTF-16만 다루고 나머지는 바이트 그대로 매칭한다.

use crate::bytes::has_nul;
use std::borrow::Cow;

/// 이진 판정 창(바이트).
pub(crate) const BINARY_PROBE: usize = 8 * 1024;

/// 첫 조각(≤ [`BINARY_PROBE`])만 보고 이진인가 — NUL이 있고 UTF-16 BOM이 아니면 이진. 나머지를 읽기 전에 거른다.
pub(crate) fn probe_is_binary(head: &[u8]) -> bool {
    !(head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]))
        && has_nul(&head[..head.len().min(BINARY_PROBE)])
}

/// 본문을 매칭 가능한 형태로. `None` = 이진 파일(건너뜀).
pub(crate) fn prepare(raw: &[u8]) -> Option<Cow<'_, [u8]>> {
    if let Some(rest) = raw.strip_prefix(&[0xFF, 0xFE]) {
        return Some(Cow::Owned(utf16_to_utf8(rest, false)));
    }
    if let Some(rest) = raw.strip_prefix(&[0xFE, 0xFF]) {
        return Some(Cow::Owned(utf16_to_utf8(rest, true)));
    }
    if has_nul(&raw[..raw.len().min(BINARY_PROBE)]) {
        return None;
    }
    let body = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
    Some(Cow::Borrowed(body))
}

/// UTF-16(BOM 뒤) → UTF-8. 짝 잃은 대리 코드·홀수 바이트는 U+FFFD.
fn utf16_to_utf8(b: &[u8], big_endian: bool) -> Vec<u8> {
    // `as_chunks`(1.88 · clippy `manual_slice_chunks` — CI stable · MSRV 1.89): 짝 단위 배열 · 나머지 1바이트는 아래서.
    let (pairs, _odd) = b.as_chunks::<2>();
    let units = pairs.iter().map(|p| {
        if big_endian {
            u16::from_be_bytes(*p)
        } else {
            u16::from_le_bytes(*p)
        }
    });
    let mut s = String::with_capacity(b.len());
    s.extend(char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)));
    if b.len() % 2 == 1 {
        s.push(char::REPLACEMENT_CHARACTER);
    }
    s.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_and_bom() {
        assert!(prepare(b"abc\0def").is_none());
        assert!(probe_is_binary(b"abc\0def"));
        assert!(
            !probe_is_binary(b"\xFF\xFEa\0b\0"),
            "UTF-16 BOM은 NUL이 있어도 텍스트"
        );
        assert!(!probe_is_binary(b"abc"));
        assert_eq!(prepare(b"\xEF\xBB\xBFabc").as_deref(), Some(&b"abc"[..]));
        assert_eq!(prepare(b"plain").as_deref(), Some(&b"plain"[..]));
        // NUL이 8KB 뒤에만 있으면 텍스트로 본다(판정 창).
        let mut big = vec![b'a'; BINARY_PROBE + 10];
        big[BINARY_PROBE + 5] = 0;
        assert!(prepare(&big).is_some());
    }

    #[test]
    fn utf16_both_endians() {
        let text = "SELECT 한글 😀";
        let mut le = vec![0xFF, 0xFE];
        let mut be = vec![0xFE, 0xFF];
        for u in text.encode_utf16() {
            le.extend_from_slice(&u.to_le_bytes());
            be.extend_from_slice(&u.to_be_bytes());
        }
        assert_eq!(prepare(&le).as_deref(), Some(text.as_bytes()));
        assert_eq!(prepare(&be).as_deref(), Some(text.as_bytes()));
        // 홀수 길이 → 대체 문자.
        le.push(0x41);
        let out = prepare(&le).expect("text");
        assert!(String::from_utf8_lossy(&out).ends_with('\u{FFFD}'));
    }
}
