//! 파일 인코딩(사용자 09-16 · 상태줄 세그먼트 + 파일 대화상자 공용) — **목록 한 곳**.
//!
//! 범위: 영어권(UTF-8/16 · Windows-1252)과 한·중·일에 필요한 것만(Sublime 전체 목록 대신). 변환은 `encoding_rs`(WHATWG ·
//! 순수 Rust · Firefox 코덱 · DR-3 예외 원장 [30 §2](../../../docs/30-architecture-patterns.md)). UTF-16/BOM은 직접 처리.
//!
//! - `Set Encoding`(저장 인코딩만 바꿈 · 본문 그대로) vs `Reopen with Encoding`(파일을 그 인코딩으로 다시 디코드) — Sublime의
//!   두 메뉴를 **한 팝업**에 둔다: 위에 "다시 열기 ▸"(파일 탭일 때만) · 아래 목록 = 저장 인코딩(VS Code는 한 목록 뒤에 Reopen/Save를
//!   묻는 2단 · 우리는 1단에서 둘 다 보이게).
//! - Hexadecimal 보기는 두지 않는다(SQL 클라이언트에 이진 보기 수요가 낮고 편집 불가 · 필요하면 외부 도구).

use nsql_i18n::{t, Msg};

/// (id · 상태줄 짧은 표기 · 메뉴 라벨 · encoding_rs 라벨(None = 직접 처리)).
pub(crate) struct EncSpec {
    pub id: &'static str,
    pub short: &'static str,
    pub label: Msg,
    pub whatwg: Option<&'static str>,
}

/// 인코딩 목록(메뉴 순서). 구분선은 `utf16be` 뒤.
pub(crate) const LIST: &[EncSpec] = &[
    EncSpec {
        id: "utf8",
        short: "UTF-8",
        label: Msg::EncUtf8,
        whatwg: None,
    },
    EncSpec {
        id: "utf8bom",
        short: "UTF-8 BOM",
        label: Msg::EncUtf8Bom,
        whatwg: None,
    },
    EncSpec {
        id: "utf16le",
        short: "UTF-16 LE",
        label: Msg::EncUtf16Le,
        whatwg: None,
    },
    EncSpec {
        id: "utf16be",
        short: "UTF-16 BE",
        label: Msg::EncUtf16Be,
        whatwg: None,
    },
    EncSpec {
        id: "cp1252",
        short: "CP1252",
        label: Msg::EncCp1252,
        whatwg: Some("windows-1252"),
    },
    EncSpec {
        id: "euckr",
        short: "EUC-KR",
        label: Msg::EncEucKr,
        whatwg: Some("euc-kr"),
    },
    EncSpec {
        id: "sjis",
        short: "Shift_JIS",
        label: Msg::EncSjis,
        whatwg: Some("shift_jis"),
    },
    EncSpec {
        id: "eucjp",
        short: "EUC-JP",
        label: Msg::EncEucJp,
        whatwg: Some("euc-jp"),
    },
    EncSpec {
        id: "gb18030",
        short: "GB18030",
        label: Msg::EncGb18030,
        whatwg: Some("gb18030"),
    },
    EncSpec {
        id: "big5",
        short: "Big5",
        label: Msg::EncBig5,
        whatwg: Some("big5"),
    },
];

/// 유니코드 계열(목록 앞부분 · 구분선 위) 수.
pub(crate) const UNICODE_COUNT: usize = 4;

pub(crate) fn spec(id: &str) -> Option<&'static EncSpec> {
    LIST.iter().find(|e| e.id == id)
}

/// 상태줄 표기(모르는 id는 그대로).
pub(crate) fn short(id: &str) -> String {
    spec(id).map_or_else(|| id.to_string(), |e| e.short.to_string())
}

/// 메뉴 라벨.
pub(crate) fn label(id: &str) -> String {
    spec(id).map_or_else(|| id.to_string(), |e| t(e.label).to_string())
}

/// 바이트 → 본문. `auto` = BOM으로만 판정(없으면 UTF-8). 반환 = (본문 · 깨진 바이트 있었나 · 실제 쓴 id).
pub(crate) fn decode(bytes: &[u8], enc: &str) -> (String, bool, &'static str) {
    let enc = if enc == "auto" {
        if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            "utf8bom"
        } else if bytes.starts_with(&[0xFF, 0xFE]) {
            "utf16le"
        } else if bytes.starts_with(&[0xFE, 0xFF]) {
            "utf16be"
        } else {
            "utf8"
        }
    } else {
        enc
    };
    match enc {
        "utf16le" | "utf16be" => {
            let le = enc == "utf16le";
            let body = if (le && bytes.starts_with(&[0xFF, 0xFE]))
                || (!le && bytes.starts_with(&[0xFE, 0xFF]))
            {
                &bytes[2..]
            } else {
                bytes
            };
            let units: Vec<u16> = body
                .chunks(2)
                .map(|c| {
                    let (a, b) = (c[0], c.get(1).copied().unwrap_or(0));
                    if le {
                        u16::from_le_bytes([a, b])
                    } else {
                        u16::from_be_bytes([a, b])
                    }
                })
                .collect();
            let text = String::from_utf16_lossy(&units);
            let lossy = text.contains('\u{FFFD}');
            (text, lossy, if le { "utf16le" } else { "utf16be" })
        }
        "utf8bom" => {
            let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
            let text = String::from_utf8_lossy(body);
            let lossy = text.contains('\u{FFFD}');
            (text.into_owned(), lossy, "utf8bom")
        }
        other => match spec(other).and_then(|e| e.whatwg) {
            Some(w) => {
                let Some(codec) = encoding_rs::Encoding::for_label(w.as_bytes()) else {
                    let text = String::from_utf8_lossy(bytes);
                    let lossy = text.contains('\u{FFFD}');
                    return (text.into_owned(), lossy, "utf8");
                };
                let (text, _, had_errors) = codec.decode(bytes);
                (
                    text.into_owned(),
                    had_errors,
                    spec(other).map_or("utf8", |e| e.id),
                )
            }
            None => {
                let text = String::from_utf8_lossy(bytes);
                let lossy = text.contains('\u{FFFD}');
                (text.into_owned(), lossy, "utf8")
            }
        },
    }
}

/// 본문 → 바이트(저장). 인코딩에 없는 글자는 `?`(encoding_rs 규약)로 대체된다.
pub(crate) fn encode(text: &str, enc: &str) -> Vec<u8> {
    match enc {
        "utf8bom" => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            v
        }
        "utf16le" => {
            let mut v = vec![0xFF, 0xFE];
            for u in text.encode_utf16() {
                v.extend_from_slice(&u.to_le_bytes());
            }
            v
        }
        "utf16be" => {
            let mut v = vec![0xFE, 0xFF];
            for u in text.encode_utf16() {
                v.extend_from_slice(&u.to_be_bytes());
            }
            v
        }
        other => match spec(other).and_then(|e| e.whatwg) {
            Some(w) => match encoding_rs::Encoding::for_label(w.as_bytes()) {
                Some(codec) => codec.encode(text).0.into_owned(),
                None => text.as_bytes().to_vec(),
            },
            None => text.as_bytes().to_vec(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_round_trip() {
        let s = "한글 テスト 中文";
        for id in [
            "euckr", "sjis", "gb18030", "big5", "utf8bom", "utf16le", "utf16be", "utf8",
        ] {
            let bytes = encode(s, id);
            let (back, lossy, used) = decode(&bytes, id);
            // EUC-KR·Shift_JIS·Big5는 다른 문자권 글자를 못 담는다(`?`) — 담을 수 있는 인코딩만 왕복 검사.
            if matches!(id, "gb18030" | "utf8bom" | "utf16le" | "utf16be" | "utf8") {
                assert_eq!(back, s, "{id}");
                assert!(!lossy, "{id}");
            }
            assert_eq!(used, id);
        }
        let ko = encode("한글", "euckr");
        assert_eq!(ko, vec![0xC7, 0xD1, 0xB1, 0xDB]);
        assert_eq!(decode(&ko, "euckr").0, "한글");
    }

    #[test]
    fn auto_detects_bom_only() {
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'a'], "auto").2, "utf8bom");
        assert_eq!(decode(&[0xFF, 0xFE, b'a', 0], "auto").2, "utf16le");
        assert_eq!(decode(b"abc", "auto").2, "utf8");
    }
}
