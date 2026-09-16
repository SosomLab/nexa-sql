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
