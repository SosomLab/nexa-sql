//! 바이트 스캔 원시 함수 — memchr 계열을 std만으로(`u64` SWAR · docs/36 §2 "매칭" 행).
//!
//! 8바이트 단어를 한 번에 보고, 찾는 바이트가 있는 단어에서만 바이트 단위로 확인한다.
//! 외부 crate 0(DR-3) · 3-OS 동일 코드(SIMD 내장 함수 없음).

const LO: u64 = 0x0101_0101_0101_0101;
const HI: u64 = 0x8080_8080_8080_8080;

/// `x`에 0 바이트가 하나라도 있으면 0이 아닌 값(그 바이트의 최상위 비트가 선다).
#[inline(always)]
const fn zero_byte_mask(x: u64) -> u64 {
    x.wrapping_sub(LO) & !x & HI
}

/// 첫 `b`의 위치(SWAR).
#[inline]
pub(crate) fn memchr(b: u8, hay: &[u8]) -> Option<usize> {
    let pat = LO.wrapping_mul(u64::from(b));
    let mut i = 0;
    let n = hay.len();
    while i + 8 <= n {
        // 정렬을 가정하지 않는다 — 8바이트 복사가 정렬 접근보다 싸다.
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&hay[i..i + 8]);
        let w = u64::from_le_bytes(buf);
        let m = zero_byte_mask(w ^ pat);
        if m != 0 {
            // LE로 읽었으므로 가장 낮은 바이트 = 가장 앞 바이트.
            return Some(i + (m.trailing_zeros() / 8) as usize);
        }
        i += 8;
    }
    hay[i..].iter().position(|&c| c == b).map(|p| i + p)
}

/// 첫 `b1` 또는 `b2`의 위치(대소문자 무시 후보 스캔용).
#[inline]
pub(crate) fn memchr2(b1: u8, b2: u8, hay: &[u8]) -> Option<usize> {
    if b1 == b2 {
        return memchr(b1, hay);
    }
    let p1 = LO.wrapping_mul(u64::from(b1));
    let p2 = LO.wrapping_mul(u64::from(b2));
    let mut i = 0;
    let n = hay.len();
    while i + 8 <= n {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&hay[i..i + 8]);
        let w = u64::from_le_bytes(buf);
        let m = zero_byte_mask(w ^ p1) | zero_byte_mask(w ^ p2);
        if m != 0 {
            return Some(i + (m.trailing_zeros() / 8) as usize);
        }
        i += 8;
    }
    hay[i..]
        .iter()
        .position(|&c| c == b1 || c == b2)
        .map(|p| i + p)
}

/// 마지막 `b`의 위치(줄 시작 찾기 — 일치 지점에서만 쓰므로 단순 역순).
#[inline]
pub(crate) fn memrchr(b: u8, hay: &[u8]) -> Option<usize> {
    hay.iter().rposition(|&c| c == b)
}

/// `hay`에 NUL이 있는가(이진 판정 · 첫 8KB에만 적용).
#[inline]
pub(crate) fn has_nul(hay: &[u8]) -> bool {
    memchr(0, hay).is_some()
}

/// 바이트의 "흔함" 등급(낮을수록 드묾) — 리터럴 검색이 스캔할 바이트를 고를 때 쓴다.
/// 영문 텍스트 빈도 + 한글 UTF-8 선두 바이트(0xEA~0xED)가 몹시 흔하다는 점을 반영한다.
pub(crate) fn commonness(b: u8) -> u8 {
    match b {
        b' ' | b'\n' | b'\t' | b'\r' => 255,
        b'e' | b't' | b'a' | b'o' | b'i' | b'n' => 240,
        b's' | b'r' | b'h' | b'l' | b'd' | b'c' | b'u' => 220,
        b'm' | b'p' | b'f' | b'g' | b'w' | b'y' | b'b' => 190,
        b'v' | b'k' => 150,
        b'x' | b'j' | b'q' | b'z' => 60,
        b'E' | b'T' | b'A' | b'O' | b'I' | b'N' | b'S' | b'R' => 170,
        b'A'..=b'Z' => 120,
        b'0'..=b'9' => 130,
        b',' | b'.' | b'_' | b'(' | b')' | b'=' | b'\'' | b'"' | b'-' | b'/' => 160,
        0xEA..=0xED => 230,
        0x80..=0xBF => 200,
        0xC0..=0xFF => 140,
        _ => 90,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memchr_matches_iter_position() {
        let hay: Vec<u8> = (0..300u32).map(|i| (i * 7 % 251) as u8).collect();
        for b in 0..=255u8 {
            assert_eq!(memchr(b, &hay), hay.iter().position(|&c| c == b), "{b}");
        }
        assert_eq!(memchr(b'x', b""), None);
        assert_eq!(memchr(b'x', b"x"), Some(0));
        assert_eq!(memchr(b'x', b"1234567x"), Some(7));
        assert_eq!(memchr(b'x', b"12345678x"), Some(8));
    }

    #[test]
    fn memchr2_matches_iter_position() {
        let hay = b"Hello World, hello again HELLO".to_vec();
        assert_eq!(memchr2(b'h', b'H', &hay), Some(0));
        assert_eq!(memchr2(b'w', b'W', &hay), Some(6));
        assert_eq!(memchr2(b'z', b'Z', &hay), None);
        let hay: Vec<u8> = (0..300u32).map(|i| (i * 13 % 251) as u8).collect();
        for b in 0..=255u8 {
            let b2 = b.wrapping_add(97);
            assert_eq!(
                memchr2(b, b2, &hay),
                hay.iter().position(|&c| c == b || c == b2),
                "{b}"
            );
        }
    }

    #[test]
    fn memrchr_and_nul() {
        assert_eq!(memrchr(b'\n', b"a\nb\nc"), Some(3));
        assert_eq!(memrchr(b'\n', b"abc"), None);
        assert!(has_nul(b"ab\0c"));
        assert!(!has_nul(b"abc"));
    }
}
