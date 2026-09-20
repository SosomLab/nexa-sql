//! 줄끝(EOL) 정책([docs/38](../../../docs/38-line-endings.md) · 사용자 09-16) — **통합 로직**: 편집기 내부는 언제나 `\n`,
//! 변환은 파일 경계(열기/저장)에서만. OS는 **새 파일의 기본값**에만 관여한다(설정 `file.eol_new`).
//!
//! - 열기: CRLF/LF 다수결로 탭의 줄끝을 정하고 본문은 `\n`으로 정규화(혼합 파일도 한 종류로 저장 · 옛 Mac CR은 LF 취급).
//! - 저장: `file.eol_save` = keep(탭 줄끝 · 기본) | lf | crlf | os.
//! - 상태줄 `LF`/`CRLF` 세그먼트 클릭 = 탭 줄끝 변경(더러움 표시 · 저장 때 반영).

/// 줄끝 3종(Sublime 상태줄 팝업 · 사용자 09-16): Windows CRLF · Unix LF · 옛 Mac CR.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Eol {
    Lf,
    Crlf,
    Cr,
}

impl Eol {
    /// 상태줄 표기.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Eol::Lf => "LF",
            Eol::Crlf => "CRLF",
            Eol::Cr => "CR",
        }
    }

    /// OS 기본(Windows CRLF · 그 외 LF).
    pub(crate) fn os() -> Eol {
        if cfg!(windows) {
            Eol::Crlf
        } else {
            Eol::Lf
        }
    }
}

/// 본문의 줄끝을 판정하고 `\n`으로 정규화한다 → (줄끝, 정규화 본문).
/// 다수결: CRLF 수 ≥ (단독 LF + 단독 CR) 이면 CRLF(동률은 CRLF — Windows 파일에 LF 몇 줄이 섞인 흔한 경우) ·
/// 아니면 단독 CR이 단독 LF보다 많으면 CR(옛 Mac) · 그 외 LF. 줄끝이 없으면 LF.
pub(crate) fn detect(text: &str) -> (Eol, String) {
    if !text.as_bytes().contains(&b'\r') {
        return (Eol::Lf, text.to_string());
    }
    normalize(text)
}

/// [`detect`]의 소유권 판 — **`\r`이 하나도 없으면(LF 파일) 받은 문자열을 그대로 돌려준다**(복사 0 · 큰 파일 적재의
/// 피크 메모리 · 사용자 09-20). 있으면 한 번 훑어 세고 한 번 훑어 새로 만든다(종전 = 세 번 세고 사본 둘).
pub(crate) fn detect_owned(text: String) -> (Eol, String) {
    if !text.as_bytes().contains(&b'\r') {
        return (Eol::Lf, text);
    }
    normalize(&text)
}

/// `\r`이 있는 본문: 종류를 세고 `\n`으로 정규화한 새 문자열을 만든다(`\r`·`\n`은 ASCII라 조각 경계가 글자 경계다).
fn normalize(text: &str) -> (Eol, String) {
    let b = text.as_bytes();
    let (mut crlf, mut lone_cr, mut lf) = (0usize, 0usize, 0usize);
    let mut out = String::with_capacity(text.len());
    let (mut start, mut i) = (0usize, 0usize);
    while i < b.len() {
        match b[i] {
            b'\r' => {
                out.push_str(&text[start..i]);
                out.push('\n');
                if b.get(i + 1) == Some(&b'\n') {
                    crlf += 1;
                    i += 1;
                } else {
                    lone_cr += 1;
                }
                start = i + 1;
            }
            b'\n' => lf += 1,
            _ => {}
        }
        i += 1;
    }
    out.push_str(&text[start..]);
    let eol = if crlf > 0 && crlf >= lf + lone_cr {
        Eol::Crlf
    } else if lone_cr > lf {
        Eol::Cr
    } else {
        Eol::Lf
    };
    (eol, out)
}

/// 정규화 본문(`\n`) → 파일 본문.
pub(crate) fn apply(text: &str, eol: Eol) -> String {
    match eol {
        Eol::Crlf => text.replace('\n', "\r\n"),
        Eol::Cr => text.replace('\n', "\r"),
        Eol::Lf => text.to_string(),
    }
}

/// `file.eol_new`(auto|lf|crlf|cr) → 새 탭의 줄끝. auto = OS 기본.
pub(crate) fn default_eol(setting: &str) -> Eol {
    match setting {
        "lf" => Eol::Lf,
        "crlf" => Eol::Crlf,
        "cr" => Eol::Cr,
        _ => Eol::os(),
    }
}

/// `file.eol_save`(keep|lf|crlf|cr|os) → 저장 줄끝.
pub(crate) fn save_eol(setting: &str, tab: Eol) -> Eol {
    match setting {
        "lf" => Eol::Lf,
        "crlf" => Eol::Crlf,
        "cr" => Eol::Cr,
        "os" => Eol::os(),
        _ => tab,
    }
}

#[cfg(test)]
mod tests {
    /// 한 번 훑기 구현 = 종전 정의(세 번 세기 + replace 두 번)와 같은 답 · LF 본문은 소유권 판에서 **같은 버퍼**를 돌려준다.
    #[test]
    fn one_pass_detect_matches_reference() {
        fn reference(text: &str) -> (Eol, String) {
            let crlf = text.matches("\r\n").count();
            let lone_lf = text.matches('\n').count() - crlf;
            let lone_cr = text.matches('\r').count() - crlf;
            let eol = if crlf > 0 && crlf >= lone_lf + lone_cr {
                Eol::Crlf
            } else if lone_cr > lone_lf {
                Eol::Cr
            } else {
                Eol::Lf
            };
            (eol, text.replace("\r\n", "\n").replace('\r', "\n"))
        }
        let cases = [
            "",
            "a",
            "a\nb\n",
            "a\r\nb\r\n",
            "a\rb\rc",
            "a\r\nb\nc\rd",
            "\r",
            "\r\n",
            "\n\r",
            "\r\r\n\n",
            "한글\r\n주석\r끝\n",
            "끝에 CR\r",
        ];
        for c in cases {
            assert_eq!(detect(c), reference(c), "{c:?}");
            assert_eq!(detect_owned(c.to_string()), reference(c), "{c:?}");
        }
        let s = String::from("select 1;\nselect 2;\n");
        let p = s.as_ptr();
        let (eol, out) = detect_owned(s);
        assert_eq!(eol, Eol::Lf);
        assert_eq!(out.as_ptr(), p, "LF 본문 = 복사 없이 그대로");
    }

    use super::*;

    #[test]
    fn detect_majority_and_normalize() {
        assert_eq!(detect("a\r\nb\r\nc"), (Eol::Crlf, "a\nb\nc".into()));
        assert_eq!(detect("a\nb\nc"), (Eol::Lf, "a\nb\nc".into()));
        // 혼합: CRLF 2 · LF 1 → CRLF · 동률(1:1)도 CRLF
        assert_eq!(detect("a\r\nb\nc\r\n").0, Eol::Crlf);
        assert_eq!(detect("a\r\nb\n").0, Eol::Crlf);
        assert_eq!(detect("a\r\nb\nc\n").0, Eol::Lf);
        // 옛 Mac CR → CR(정규화는 LF로)
        assert_eq!(detect("a\rb"), (Eol::Cr, "a\nb".into()));
        assert_eq!(detect("no newline"), (Eol::Lf, "no newline".into()));
    }

    #[test]
    fn apply_and_policies() {
        assert_eq!(apply("a\nb", Eol::Crlf), "a\r\nb");
        assert_eq!(apply("a\nb", Eol::Lf), "a\nb");
        assert_eq!(apply("a\nb", Eol::Cr), "a\rb");
        assert_eq!(default_eol("lf"), Eol::Lf);
        assert_eq!(default_eol("crlf"), Eol::Crlf);
        assert_eq!(default_eol("auto"), Eol::os());
        assert_eq!(save_eol("keep", Eol::Crlf), Eol::Crlf);
        assert_eq!(save_eol("keep", Eol::Cr), Eol::Cr);
        assert_eq!(save_eol("lf", Eol::Crlf), Eol::Lf);
        assert_eq!(save_eol("crlf", Eol::Lf), Eol::Crlf);
        assert_eq!(save_eol("os", Eol::Lf), Eol::os());
    }
}
