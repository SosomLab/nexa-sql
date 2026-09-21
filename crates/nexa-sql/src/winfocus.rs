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
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Linux(09-22): 창 종류 = 대화상자(X11 `_NET_WM_WINDOW_TYPE_DIALOG` · WM이 부모 위에 두고 작업 표시줄에 따로 안 낸다).
        // 부모 관계 자체는 창이 생긴 뒤 `attach_child`가 `WM_TRANSIENT_FOR`로 건다(winit의 `with_parent_window`는 X11에서 **삽입**이라 못 쓴다).
        use winit::platform::x11::{WindowAttributesExtX11 as _, WindowType};
        let _ = owner;
        attrs.with_x11_window_type(vec![WindowType::Dialog])
    }
    #[cfg(target_os = "macos")]
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

/// ★ 모달(사용자 09-15 "연결 창은 모달로"): 소유 창이 열려 있는 동안 **메인 창 입력을 OS 수준에서 막는다**.
/// Windows = `EnableWindow(FALSE)`(클릭하면 모달 창이 깜빡이며 앞으로 · 표준 모달 대화상자 동작). macOS/Linux는
/// 호스트가 이벤트를 걸러 같은 효과(`App::window_event` 모달 가드).
pub(crate) fn set_enabled(w: &Window, on: bool) {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        extern "system" {
            fn EnableWindow(hwnd: isize, enable: i32) -> i32;
        }
        if let Some(h) = hwnd(w) {
            // SAFETY: 유효한 HWND · 단순 상태 전환.
            unsafe {
                EnableWindow(h, i32::from(on));
            }
        }
    }
    // macOS: 소유 창이 열려 있는 동안 메인 창 **이동 잠금**(`setMovable:`) — 입력은 호스트 가드가 막고, 창 이동은 OS 몫이라
    //   여기서(사용자 09-17 "로그인 창이 떠 있는데 메인 창 옮기기가 동작"). Windows의 EnableWindow와 짝.
    #[cfg(target_os = "macos")]
    {
        if let Some(nsw) = ns_window(w) {
            nsw.setMovable(on);
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (w, on);
    }
}

/// macOS NSWindow 핸들(winit raw handle → NSView → window).
#[cfg(target_os = "macos")]
fn ns_window(w: &Window) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match w.window_handle().ok()?.as_raw() {
        RawWindowHandle::AppKit(h) => {
            // SAFETY: winit이 준 유효한 NSView 포인터(창이 살아 있는 동안).
            let view: &objc2_app_kit::NSView = unsafe { h.ns_view.cast().as_ref() };
            view.window()
        }
        _ => None,
    }
}

/// ★ macOS 소유 관계(사용자 09-17 "Windows처럼 모달"): 보조 창을 메인의 **자식 창**으로 — 항상 메인 위 · 메인과 함께 이동/최소화.
/// Windows는 `owned_by`(오너 HWND)가 같은 일을 창 생성 때 한다 · Linux no-op.
pub(crate) fn attach_child(owner: &Window, child: &Window) {
    #[cfg(target_os = "macos")]
    {
        if let (Some(o), Some(c)) = (ns_window(owner), ns_window(child)) {
            // SAFETY: 두 창 모두 살아 있는 winit 창의 NSWindow · 메인 스레드.
            unsafe {
                o.addChildWindow_ordered(&c, objc2_app_kit::NSWindowOrderingMode::NSWindowAbove);
            }
        }
    }
    // Linux(09-22 · 사용자 "메인을 눌러도 모달이 위에 · 두 창이 하나의 포커스"): X11(XWayland)에서 `WM_TRANSIENT_FOR` = 메인 +
    // `_NET_WM_STATE_MODAL` → GNOME/KDE가 **부모에 붙은 모달**로 다룬다(부모를 누르면 둘이 함께 올라오고 모달이 위 · 부모와 함께 최소화).
    // Wayland 네이티브(설정 `gfx.linux_backend = wayland`)에서는 winit 0.30이 부모 창 API가 없어 no-op(호스트 가드만).
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let (Some(o), Some(c)) = (x11_window(owner), x11_window(child)) {
            // 실패(연결 불가 등)는 조용히 — 호스트 가드가 모달을 지킨다.
            let _ = x11_set_transient_modal(o, c);
        }
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (owner, child);
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

/// 창을 앞으로 — 자체 시험(`NSQL_NO_ACTIVATE`)에서는 **하지 않는다**(시험 인스턴스가 사용자의 전경 창·키 입력을 빼앗지 않게 ·
/// docs/61 §2). 새로 만드는 창의 활성화는 `icon::with_icon`이 같은 변수로 막는다.
pub(crate) fn focus(w: &winit::window::Window) {
    if std::env::var_os("NSQL_NO_ACTIVATE").is_none() {
        w.focus_window();
    }
}

/// X11 창 id(XWayland 포함) — Wayland 네이티브 창이면 `None`.
#[cfg(all(unix, not(target_os = "macos")))]
fn x11_window(w: &Window) -> Option<u32> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match w.window_handle().ok()?.as_raw() {
        RawWindowHandle::Xlib(h) => u32::try_from(h.window).ok(),
        RawWindowHandle::Xcb(h) => Some(h.window.get()),
        _ => None,
    }
}

/// `WM_TRANSIENT_FOR` + `_NET_WM_STATE_MODAL`(EWMH 클라이언트 메시지 · 이미 매핑된 창에도 유효). 자체 연결(`DISPLAY`) 한 번 · 짧게 쓰고 닫는다.
#[cfg(all(unix, not(target_os = "macos")))]
fn x11_set_transient_modal(owner: u32, child: u32) -> Result<(), Box<dyn std::error::Error>> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, PropMode,
    };
    use x11rb::wrapper::ConnectionExt as _;
    let (conn, screen) = x11rb::connect(None)?;
    conn.change_property32(
        PropMode::REPLACE,
        child,
        AtomEnum::WM_TRANSIENT_FOR,
        AtomEnum::WINDOW,
        &[owner],
    )?;
    let state = conn.intern_atom(false, b"_NET_WM_STATE")?.reply()?.atom;
    let modal = conn
        .intern_atom(false, b"_NET_WM_STATE_MODAL")?
        .reply()?
        .atom;
    // 아직 매핑되지 않은 창은 WM이 매핑 때 **속성**을 읽는다(클라이언트 메시지는 버린다) → 속성에도 덧붙인다.
    conn.change_property32(PropMode::APPEND, child, state, AtomEnum::ATOM, &[modal])?;
    let root = conn.setup().roots[screen].root;
    // 이미 매핑된 창(재사용)은 EWMH 클라이언트 메시지로: data = [_NET_WM_STATE_ADD(1), 상태 원자, 0, 출처 = 응용(1), 0]
    let ev = ClientMessageEvent::new(32, child, state, [1u32, modal, 0, 1, 0]);
    conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        ev,
    )?;
    conn.flush()?;
    Ok(())
}
