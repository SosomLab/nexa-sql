//! 앱 아이콘(사용자 선택 09-14 · "Union" — 색이 다른 타원 세 장이 한 실린더로 융합 · 타원은 크고 같은 크기).
//!
//! 원본 SSOT = `packaging/branding/icon.svg`. 런타임 창 아이콘은 **디코더 없이**(외부 crate 0)
//! 빌드 때 만든 raw RGBA(`nexa-sql-32.rgba` · `nexa-sql-64.rgba`)를 `include_bytes!`로 박아 쓴다.
//! 재생성 = `packaging/branding/README.md`.
//!
//! | OS | 창 아이콘의 효과 |
//! |---|---|
//! | Windows | 타이틀바(小 32) + 작업표시줄(大 64 — `with_taskbar_icon`) |
//! | Linux/X11 | 태스크바·창 전환기 · Wayland는 `.desktop` + hicolor PNG 몫 |
//! | macOS | 무시 — Dock 아이콘은 `.app` 번들의 `.icns` 몫 |

const SMALL: &[u8] = include_bytes!("../../../packaging/branding/nexa-sql-32.rgba");
const LARGE: &[u8] = include_bytes!("../../../packaging/branding/nexa-sql-64.rgba");

/// 창 속성에 아이콘을 붙인다 — 모든 창(메인·접속·로그)이 이 한 곳을 지난다. 변환 실패 = 아이콘 없이(fail-soft).
pub(crate) fn with_icon(attrs: winit::window::WindowAttributes) -> winit::window::WindowAttributes {
    let small = winit::window::Icon::from_rgba(SMALL.to_vec(), 32, 32).ok();
    #[cfg(windows)]
    let attrs = {
        use winit::platform::windows::WindowAttributesExtWindows as _;
        let large = winit::window::Icon::from_rgba(LARGE.to_vec(), 64, 64).ok();
        attrs.with_taskbar_icon(large)
    };
    #[cfg(not(windows))]
    let _ = LARGE;
    attrs.with_window_icon(small)
}
