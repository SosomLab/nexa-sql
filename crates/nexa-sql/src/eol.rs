//! 줄끝(EOL) 정책([docs/38](../../../docs/38-line-endings.md) · 사용자 09-16) — **통합 로직**: 편집기 내부는 언제나 `\n`,
//! 변환은 파일 경계(열기/저장)에서만. OS는 **새 파일의 기본값**에만 관여한다(설정 `file.eol_new`).
//!
//! - 열기: CRLF/LF 다수결로 탭의 줄끝을 정하고 본문은 `\n`으로 정규화(혼합 파일도 한 종류로 저장 · 옛 Mac CR은 LF 취급).
//! - 저장: `file.eol_save` = keep(탭 줄끝 · 기본) | lf | crlf | os.
//! - 상태줄 `LF`/`CRLF` 세그먼트 클릭 = 탭 줄끝 변경(더러움 표시 · 저장 때 반영).

/// 본문의 줄끝을 판정하고 `\n`으로 정규화한다 → (CRLF인가, 정규화 본문).
/// 다수결: CRLF 수 ≥ (단독 LF + 단독 CR) 이면 CRLF(동률은 CRLF — Windows 파일에 LF 몇 줄이 섞인 흔한 경우). 줄끝이 없으면 LF.
pub(crate) fn detect(text: &str) -> (bool, String) {
    let crlf = text.matches("\r\n").count();
    let lone_lf = text.matches('\n').count() - crlf;
    let lone_cr = text.matches('\r').count() - crlf;
    let is_crlf = crlf > 0 && crlf >= lone_lf + lone_cr;
    (is_crlf, text.replace("\r\n", "\n").replace('\r', "\n"))
}

/// 정규화 본문(`\n`) → 파일 본문.
pub(crate) fn apply(text: &str, crlf: bool) -> String {
    if crlf {
        text.replace('\n', "\r\n")
    } else {
        text.to_string()
    }
}

/// `file.eol_new`(auto|lf|crlf) → 새 탭의 CRLF 여부. auto = OS 기본(Windows CRLF · 그 외 LF).
pub(crate) fn default_crlf(setting: &str) -> bool {
    match setting {
        "lf" => false,
        "crlf" => true,
        _ => cfg!(windows),
    }
}

/// `file.eol_save`(keep|lf|crlf|os) → 저장 줄끝.
pub(crate) fn save_crlf(setting: &str, tab_crlf: bool) -> bool {
    match setting {
        "lf" => false,
        "crlf" => true,
        "os" => cfg!(windows),
        _ => tab_crlf,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_majority_and_normalize() {
        assert_eq!(detect("a\r\nb\r\nc"), (true, "a\nb\nc".into()));
        assert_eq!(detect("a\nb\nc"), (false, "a\nb\nc".into()));
        // 혼합: CRLF 2 · LF 1 → CRLF · 동률(1:1)도 CRLF
        assert!(detect("a\r\nb\nc\r\n").0);
        assert!(detect("a\r\nb\n").0);
        assert!(!detect("a\r\nb\nc\n").0);
        // 옛 Mac CR → LF 취급(정규화만)
        assert_eq!(detect("a\rb"), (false, "a\nb".into()));
        assert_eq!(detect("no newline"), (false, "no newline".into()));
    }

    #[test]
    fn apply_and_policies() {
        assert_eq!(apply("a\nb", true), "a\r\nb");
        assert_eq!(apply("a\nb", false), "a\nb");
        assert!(!default_crlf("lf"));
        assert!(default_crlf("crlf"));
        assert_eq!(default_crlf("auto"), cfg!(windows));
        assert!(save_crlf("keep", true));
        assert!(!save_crlf("keep", false));
        assert!(!save_crlf("lf", true));
        assert!(save_crlf("crlf", false));
        assert_eq!(save_crlf("os", false), cfg!(windows));
    }
}
