//! ★ T-369(10-17 · 사용자 "엔진의 안내·오류 메시지를 영어 기본 + 화면 언어 따르기로"): 이 크레이트는 의존 0이라 `nsql-i18n`을 쓸 수 없다 →
//! **영어가 기본**이고 호스트(GUI `ui.lang` · CLI 설정 언어)가 [`set_lang`]으로 한국어를 켜면 같은 자리에서 한국어 글이 나온다.
//! 글은 만드는 자리에서 [`tr!`]로 두 언어를 나란히 적는다(키 표 없음 · 사용자 메시지만 · 로그·진단 영어 고정 글은 그대로).

use std::sync::atomic::{AtomicU8, Ordering};

/// 엔진 메시지 언어(영어 기본).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Lang {
    #[default]
    En = 0,
    Ko = 1,
}

impl Lang {
    /// 화면 언어 코드(`en` · `ko` · `ko-KR` …)로 — 모르는 코드 = 영어.
    #[must_use]
    pub fn from_code(code: &str) -> Lang {
        let c = code.trim().to_ascii_lowercase();
        if c == "ko" || c.starts_with("ko-") || c.starts_with("ko_") {
            Lang::Ko
        } else {
            Lang::En
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

/// 호스트가 화면 언어를 정할 때(기동 · 설정 변경) 부른다. 엔진 스레드가 달라도 같은 값(원자).
pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

/// 지금 언어.
#[must_use]
pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 {
        Lang::Ko
    } else {
        Lang::En
    }
}

/// 한국어인가(`tr!`가 쓴다).
#[must_use]
pub fn is_ko() -> bool {
    lang() == Lang::Ko
}

/// 두 언어를 나란히: `tr!("English {x}", "한국어 {x}")` · 위치 인자도 그대로(`tr!("port '{}'", "포트 '{}'", p)`).
#[macro_export]
macro_rules! tr {
    ($en:literal, $ko:literal $(, $arg:expr)* $(,)?) => {
        if $crate::lang::is_ko() { format!($ko $(, $arg)*) } else { format!($en $(, $arg)*) }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_english_and_code_mapping() {
        assert_eq!(Lang::from_code("ko"), Lang::Ko);
        assert_eq!(Lang::from_code("ko-KR"), Lang::Ko);
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("system"), Lang::En);
    }
}
