//! OS 화면 언어 → 앱 언어(사용자 09-28 "언어가 있는 경우 설치 시에 해당 언어로 기본 설정, 없는 경우는 영어").
//!
//! 설치 경로가 여럿이라(MSI · pkg · dmg · deb · rpm · brew) 설치기마다 따로 쓰지 않고, **설정 `ui.lang`의 기본값**이
//! 이 판정을 따른다(`nsql_settings::default_of`) — 사용자가 언어를 고른 적이 없으면 OS 언어, 지원하지 않는 언어면 영어.
//! 사용자가 고르면 그 값이 이긴다(`reset` = 다시 OS 언어).
//!
//! 원천(외부 crate 0):
//! - Windows = `GetUserDefaultUILanguage`(kernel32 · 표시 언어) — 시작 메뉴에서 띄운 GUI에는 `LANG`이 없다.
//! - macOS = `CFLocaleCopyPreferredLanguages`(CoreFoundation · 시스템 설정 ▸ 언어 목록의 첫째) — Finder에서 띄운 앱도 같다.
//! - 그 밖(Linux 등) = `LANGUAGE`(콜론 목록의 첫째) → `LC_ALL` → `LC_MESSAGES` → `LANG`(gettext 우선순위 · `C`/`POSIX`는 없음으로).

use crate::Lang;
use std::sync::OnceLock;

/// 이 프로세스의 OS 언어에 맞는 앱 언어 — 지원하지 않거나 모르면 영어. 한 번만 묻는다(이후 캐시).
#[must_use]
pub fn system_lang() -> Lang {
    static CACHE: OnceLock<Lang> = OnceLock::new();
    *CACHE.get_or_init(|| detect().unwrap_or(Lang::En))
}

/// 로캘 문자열(`ko_KR.UTF-8` · `ko-KR` · `en-US` · `ko`) → 지원 언어. 모르는 언어·`C`·`POSIX`·빈 값 = `None`.
#[must_use]
pub fn lang_from_locale(s: &str) -> Option<Lang> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("C") || s.eq_ignore_ascii_case("POSIX") {
        return None;
    }
    // `ko_KR.UTF-8@euro` → `ko_KR` → Lang::from_code가 지역 접미를 떼어 판정.
    let base = s.split(['.', '@']).next().unwrap_or(s);
    Lang::from_code(base)
}

/// Windows LANGID → 지원 언어(주 언어 = 하위 10비트 · 0x12 = 한국어 · 0x09 = 영어).
#[must_use]
pub fn lang_from_langid(id: u16) -> Option<Lang> {
    match id & 0x3ff {
        0x12 => Some(Lang::Ko),
        0x09 => Some(Lang::En),
        _ => None,
    }
}

/// 유닉스 계열 환경 변수 순서(gettext): `LANGUAGE`(콜론 목록) → `LC_ALL` → `LC_MESSAGES` → `LANG`. 첫 "값 있는" 변수로 판정.
#[must_use]
pub fn lang_from_env(get: impl Fn(&str) -> Option<String>) -> Option<Lang> {
    for key in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        let Some(v) = get(key).filter(|v| !v.trim().is_empty()) else {
            continue;
        };
        let first = if key == "LANGUAGE" {
            v.split(':')
                .find(|p| !p.trim().is_empty())
                .unwrap_or("")
                .to_string()
        } else {
            v
        };
        // 첫 "값 있는" 변수가 답이다 — C/POSIX·지원 밖 언어면 다음 변수로 넘어가지 않고 "모름"(→ 영어).
        return lang_from_locale(&first);
    }
    None
}

#[cfg(windows)]
fn detect() -> Option<Lang> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // SAFETY: 인자 없는 조회 함수 · 전역 상태를 바꾸지 않는다.
    let id = unsafe { GetUserDefaultUILanguage() };
    lang_from_langid(id)
}

#[cfg(target_os = "macos")]
fn detect() -> Option<Lang> {
    use std::ffi::{c_char, c_void, CStr};
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> *const c_void;
        fn CFArrayGetCount(a: *const c_void) -> isize;
        fn CFArrayGetValueAtIndex(a: *const c_void, i: isize) -> *const c_void;
        fn CFStringGetCString(s: *const c_void, buf: *mut c_char, len: isize, enc: u32) -> u8;
        fn CFRelease(p: *const c_void);
    }
    const UTF8: u32 = 0x0800_0100;
    // SAFETY: Copy 규칙 = 받은 배열은 우리가 CFRelease · 원소는 빌린 참조(배열이 살아 있는 동안만 읽는다) · 버퍼 길이를 넘겨 준다.
    unsafe {
        let arr = CFLocaleCopyPreferredLanguages();
        if arr.is_null() {
            return None;
        }
        let mut out = None;
        if CFArrayGetCount(arr) > 0 {
            let s = CFArrayGetValueAtIndex(arr, 0);
            let mut buf = [0 as c_char; 64];
            if !s.is_null()
                && CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as isize, UTF8) != 0
            {
                let code = CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned();
                out = lang_from_locale(&code);
            }
        }
        CFRelease(arr);
        out
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn detect() -> Option<Lang> {
    lang_from_env(|k| std::env::var(k).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_strings_map_to_supported_or_none() {
        assert_eq!(lang_from_locale("ko_KR.UTF-8"), Some(Lang::Ko));
        assert_eq!(lang_from_locale("ko-KR"), Some(Lang::Ko));
        assert_eq!(lang_from_locale("ko"), Some(Lang::Ko));
        assert_eq!(lang_from_locale("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(lang_from_locale("en-GB"), Some(Lang::En));
        assert_eq!(
            lang_from_locale("ja_JP.UTF-8"),
            None,
            "지원하지 않는 언어 = 모름(→ 영어)"
        );
        assert_eq!(lang_from_locale("C"), None);
        assert_eq!(lang_from_locale("POSIX"), None);
        assert_eq!(lang_from_locale(""), None);
    }

    #[test]
    fn windows_langids() {
        assert_eq!(lang_from_langid(0x0412), Some(Lang::Ko), "ko-KR");
        assert_eq!(lang_from_langid(0x0409), Some(Lang::En), "en-US");
        assert_eq!(lang_from_langid(0x0809), Some(Lang::En), "en-GB");
        assert_eq!(lang_from_langid(0x0411), None, "ja-JP");
    }

    #[test]
    fn env_priority_follows_gettext() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| (*v).to_string())
            }
        };
        assert_eq!(
            lang_from_env(env(&[("LANG", "ko_KR.UTF-8")])),
            Some(Lang::Ko)
        );
        assert_eq!(
            lang_from_env(env(&[("LANG", "en_US.UTF-8"), ("LC_ALL", "ko_KR.UTF-8")])),
            Some(Lang::Ko),
            "LC_ALL이 LANG보다 앞"
        );
        assert_eq!(
            lang_from_env(env(&[("LANGUAGE", "ko:en"), ("LANG", "en_US.UTF-8")])),
            Some(Lang::Ko),
            "LANGUAGE 목록의 첫째"
        );
        assert_eq!(
            lang_from_env(env(&[("LANG", "fr_FR.UTF-8")])),
            None,
            "지원 밖 = 모름"
        );
        assert_eq!(lang_from_env(env(&[("LANG", "C")])), None);
        assert_eq!(lang_from_env(env(&[])), None);
    }

    #[test]
    fn system_lang_is_supported_and_stable() {
        let a = system_lang();
        assert!(Lang::ALL.contains(&a));
        assert_eq!(a, system_lang(), "캐시 = 같은 값");
    }
}
