//! 창 포커스 정책(사용자 09-14) — 설정 `window.focus`.
//!
//! - `group`(기본): 창 하나를 선택하면 **모든 창이 함께 앞으로** — 현재 z-order는 유지하고 방금 선택된 창만 맨 위.
//! - `single`: 선택한 창만 활성화(다른 창은 그대로).
//!
//! Windows = `SetWindowPos(HWND_TOP, SWP_NOACTIVATE)`로 **포커스를 빼앗지 않고** 올린다(user32 FFI · DR-3 크레이트 0).
//! macOS는 AppKit이 앱 활성화 시 모든 창을 함께 올리므로 기본이 group과 같고, Linux는 WM 정책이라 no-op.

use winit::window::{Window, WindowAttributes};

/// ★ 보조 창(접속·로그)을 메인 창의 **소유(owned) 창**으로 — 작업표시줄 항목이 인스턴스당 **하나**(Golden과 동일 ·
/// 사용자 09-14) · 항상 메인 위 · 메인과 함께 최소화. 소유 관계는 프로세스 안에서만 맺어지므로 **다중 인스턴스**는
/// 그대로(인스턴스마다 항목 하나). Windows = `WS_EX` 없이 오너 HWND만(툴 윈도 스타일 아님 · 타이틀바 그대로).
/// macOS는 Dock 아이콘이 앱당 하나라 이미 같고, Linux는 WM 몫(no-op).
pub(crate) fn owned_by(attrs: WindowAttributes, owner: Option<&Window>) -> WindowAttributes {
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::WindowAttributesExtWindows as _;
        if let Some(h) = owner.and_then(hwnd) {
            return attrs.with_owner_window(h);
        }
        attrs
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = owner;
        attrs
    }
}

#[cfg(target_os = "windows")]
fn hwnd(w: &Window) -> Option<isize> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match w.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

/// `bottom_to_top` 순서대로(맨 뒤 → 맨 앞) 활성화 없이 맨 위로 올린다 — 마지막 창이 맨 위가 된다.
pub(crate) fn raise_group(bottom_to_top: &[&Window]) {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        extern "system" {
            fn SetWindowPos(
                hwnd: isize,
                after: isize,
                x: i32,
                y: i32,
                cx: i32,
                cy: i32,
                flags: u32,
            ) -> i32;
        }
        const HWND_TOP: isize = 0;
        const SWP_NOSIZE: u32 = 0x0001;
        const SWP_NOMOVE: u32 = 0x0002;
        const SWP_NOACTIVATE: u32 = 0x0010;
        for w in bottom_to_top {
            if let Some(h) = hwnd(w) {
                // SAFETY: 유효한 HWND · 크기/위치 변경 없음 · 활성화 없음.
                unsafe {
                    SetWindowPos(
                        h,
                        HWND_TOP,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOMOVE | SWP_NOACTIVATE,
                    );
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = bottom_to_top;
    }
}
