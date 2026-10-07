//! 테마 해석(T-38) — `ui.theme`(System/Light/Dark) + OS 판정 → `nexa_ctl::Theme`.
//!
//! 이식 원본: `nexa-clip/crates/nclip-plat/src/theme.rs`(← nexa-beep) **Windows·macOS 부분**.
//! Linux는 원본이 zbus(포털)를 쓰지만 이 앱은 트레이가 없어 zbus를 들이지 않는다(DR-3 외부 crate 0 지향) —
//! `gsettings color-scheme` 프로세스 호출로 대체(무선호·미설치 = `None`).
//!
//! | OS | 조회 | 변경 감시 |
//! |---|---|---|
//! | Windows | HKCU `Themes\Personalize\AppsUseLightTheme` DWORD(0 = 다크) · advapi32 직접 | winit `WindowEvent::ThemeChanged` |
//! | macOS | `defaults read -g AppleInterfaceStyle`(= "Dark" · 키 부재 = 라이트) | winit `ThemeChanged` |
//! | Linux | `gsettings get org.gnome.desktop.interface color-scheme`(`prefer-dark`/`prefer-light`) | 없음(다음 실행 · 또는 winit Wayland `ThemeChanged`) |
//!
//! 우선순위: **Windows = 레지스트리 → 창 판정**([`system_dark`] `os_first` · T-308) · 다른 OS = winit `Window::theme()` → OS 조회 → `None`(= 다크 · 제품 기본 룩).

use nexa_ctl::theme::Theme;
use nsql_settings::ThemeMode;

/// OS가 다크를 선호하면 `Some(true)`, 라이트면 `Some(false)`, 모르면 `None`.
pub(crate) fn system_prefers_dark() -> Option<bool> {
    imp::prefers_dark()
}

/// System 모드의 OS 판정 고르기(순수 · 조건 2 = MC/DC 시험): `os_first`면 OS 조회 → 창 판정 · 아니면 창 판정 → OS 조회.
///
/// ★ Windows는 `os_first`(T-308 · nexa-dir3 교차 검증 10-07): winit 0.30.13의 `Window::theme()`는 **창을 만들 때의 판정이 고정**된다 —
/// `set_theme()`는 `try_theme`만 부르고 `window_state.current_theme`을 갱신하지 않는다(`platform_impl/windows/window.rs:962` vs `:1143`).
/// Dark로 기동한 뒤 System으로 바꾸면 창 판정이 계속 Dark라 레지스트리(`AppsUseLightTheme`)를 먼저 본다. 다른 OS는 winit 판정이 더 정확하다(폴백 = 외부 명령).
pub(crate) fn system_dark(os: Option<bool>, window: Option<bool>, os_first: bool) -> Option<bool> {
    if os_first {
        os.or(window)
    } else {
        window.or(os)
    }
}

/// 모드 + (winit 창 판정 ↔ OS 조회 · 순서는 [`system_dark`]) → 팔레트.
pub(crate) fn resolve(mode: ThemeMode, window_theme: Option<winit::window::Theme>) -> Theme {
    let window = match window_theme {
        Some(winit::window::Theme::Dark) => Some(true),
        Some(winit::window::Theme::Light) => Some(false),
        None => None,
    };
    let os_first = cfg!(windows);
    // 명시 모드(Light/Dark)는 OS를 안 보므로 조회를 아낀다 · 창 판정으로 충분한 OS에서도 조회하지 않는다.
    let os = if mode == ThemeMode::System && (os_first || window.is_none()) {
        system_prefers_dark()
    } else {
        None
    };
    if mode.is_dark(system_dark(os, window, os_first)) {
        Theme::dark()
    } else {
        Theme::light()
    }
}

/// 명시 모드는 창 장식(제목줄)도 따라가게 winit에 알린다. System은 `None`(OS가 정한다).
pub(crate) fn window_theme(mode: ThemeMode) -> Option<winit::window::Theme> {
    match mode {
        ThemeMode::System => None,
        ThemeMode::Light => Some(winit::window::Theme::Light),
        ThemeMode::Dark => Some(winit::window::Theme::Dark),
    }
}

#[cfg(windows)]
mod imp {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegOpenKeyExW(
            hkey: isize,
            sub_key: *const u16,
            options: u32,
            sam_desired: u32,
            result: *mut isize,
        ) -> i32;
        fn RegQueryValueExW(
            hkey: isize,
            value_name: *const u16,
            reserved: *mut u32,
            value_type: *mut u32,
            data: *mut u8,
            data_len: *mut u32,
        ) -> i32;
        fn RegCloseKey(hkey: isize) -> i32;
    }
    const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
    const KEY_QUERY_VALUE: u32 = 0x0001;
    const REG_DWORD: u32 = 4;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub(super) fn prefers_dark() -> Option<bool> {
        let sub = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
        let name = wide("AppsUseLightTheme");
        let mut hkey: isize = 0;
        // SAFETY: 널 종료 wide 문자열 + 출력 핸들. 성공 시 아래에서 닫는다.
        if unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                sub.as_ptr(),
                0,
                KEY_QUERY_VALUE,
                &mut hkey,
            )
        } != 0
        {
            return None;
        }
        let mut ty = 0u32;
        let mut data = [0u8; 4];
        let mut len = 4u32;
        // SAFETY: 열린 핸들 · 4바이트 버퍼와 길이 포인터 유효.
        let rc = unsafe {
            RegQueryValueExW(
                hkey,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut ty,
                data.as_mut_ptr(),
                &mut len,
            )
        };
        // SAFETY: 위에서 연 핸들.
        unsafe {
            RegCloseKey(hkey);
        }
        if rc != 0 || ty != REG_DWORD || len != 4 {
            return None;
        }
        Some(u32::from_le_bytes(data) == 0)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    pub(super) fn prefers_dark() -> Option<bool> {
        let out = std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
            .ok()?;
        if !out.status.success() {
            // 키 부재 = 라이트(시스템 기본).
            return Some(false);
        }
        Some(String::from_utf8_lossy(&out.stdout).trim() == "Dark")
    }
}

#[cfg(target_os = "linux")]
mod imp {
    pub(super) fn prefers_dark() -> Option<bool> {
        let out = std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "color-scheme"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout);
        if s.contains("prefer-dark") {
            Some(true)
        } else if s.contains("prefer-light") {
            Some(false)
        } else {
            None
        }
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
mod imp {
    pub(super) fn prefers_dark() -> Option<bool> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_mode_ignores_os() {
        assert!(!resolve(ThemeMode::Light, Some(winit::window::Theme::Dark)).is_dark);
        assert!(resolve(ThemeMode::Dark, Some(winit::window::Theme::Light)).is_dark);
    }

    /// MC/DC: OS 조회·창 판정·순서 — 조건 하나씩만 바꿔 결과가 바뀜을 본다(T-308).
    #[test]
    fn system_dark_order_mcdc() {
        assert_eq!(system_dark(Some(false), Some(true), true), Some(false));
        assert_eq!(system_dark(Some(true), Some(false), true), Some(true));
        assert_eq!(system_dark(None, Some(true), true), Some(true));
        assert_eq!(system_dark(None, None, true), None);
        assert_eq!(system_dark(Some(false), Some(true), false), Some(true));
        assert_eq!(system_dark(Some(true), None, false), Some(true));
        assert_eq!(system_dark(None, None, false), None);
    }

    /// System 모드: Windows는 레지스트리가 읽히면 창 판정(고정값)을 무시한다 · 다른 OS(또는 레지스트리 못 읽음)는 창 판정을 따른다.
    #[test]
    fn system_follows_window_theme_unless_windows_registry() {
        let dark = resolve(ThemeMode::System, Some(winit::window::Theme::Dark)).is_dark;
        let light = resolve(ThemeMode::System, Some(winit::window::Theme::Light)).is_dark;
        if cfg!(windows) && system_prefers_dark().is_some() {
            assert_eq!(dark, light, "Windows = 레지스트리 판정 하나로 같아야 한다");
            assert_eq!(dark, system_prefers_dark().unwrap_or(true));
        } else {
            assert!(dark);
            assert!(!light);
        }
    }

    #[test]
    fn os_probe_does_not_panic() {
        let _ = system_prefers_dark();
    }
}
