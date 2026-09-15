//! 입력 정책(사용자 09-14) — 휠·트랙패드 스크롤 방향 반전("자연스러운 스크롤" · 맥 관례).
//!
//! 프로세스 전역 플래그 하나(설정 `input.scroll_natural` · 기본 off): 창 세 개(메인·접속·로그)가 같은 변환
//! [`wheel_event`]를 쓰므로 방향 규칙이 한 곳에 있다. macOS는 OS가 이미 트랙패드 방향을 반전해 주므로 기본 off가
//! 그대로 맞고, Windows/Linux에서 맥처럼 쓰고 싶을 때 켠다.

use nexa_ctl::InputEvent;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::event::MouseScrollDelta;

static NATURAL: AtomicBool = AtomicBool::new(false);

/// 설정 반영(부팅 · 설정 변경 시).
pub(crate) fn set_natural_scroll(on: bool) {
    NATURAL.store(on, Ordering::Relaxed);
}

#[must_use]
pub(crate) fn natural_scroll() -> bool {
    NATURAL.load(Ordering::Relaxed)
}

/// 휠 사건 → 컨트롤 입력. 가로 성분(틸트 휠·트랙패드)이 있으면 `HWheel` · Shift+세로 = 가로(관례) · 아니면 세로.
/// `natural_scroll`이면 두 축 모두 부호를 뒤집는다(내용이 손가락을 따라간다).
pub(crate) fn wheel_event(delta: &MouseScrollDelta, shift: bool) -> InputEvent {
    let (mut dx, mut dy) = match delta {
        MouseScrollDelta::LineDelta(dx, dy) => ((*dx * 120.0) as i32, (*dy * 120.0) as i32),
        MouseScrollDelta::PixelDelta(p) => (p.x as i32, p.y as i32),
    };
    // ★ macOS 가로 축 부호는 Windows와 반대(사용자 09-16 "가로 스크롤이 반대로"): AppKit `scrollingDeltaX`는
    //   **양수 = 왼쪽으로 스크롤**(세로의 "양수 = 위"와 같은 규약)인데 Windows `WM_MOUSEHWHEEL`은 양수 = 오른쪽.
    //   winit은 부호를 손대지 않으므로 여기서 Windows 규약(`HWheel` 양수 = 오른쪽)으로 맞춘다. 세로는 두 OS가 같다.
    if cfg!(target_os = "macos") {
        dx = -dx;
    }
    if natural_scroll() {
        dx = -dx;
        dy = -dy;
    }
    if dx != 0 {
        InputEvent::HWheel { delta: dx }
    } else if shift {
        InputEvent::HWheel { delta: -dy }
    } else {
        InputEvent::Wheel { delta: dy }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_flips_both_axes_and_shift_makes_horizontal() {
        set_natural_scroll(false);
        assert!(matches!(
            wheel_event(&MouseScrollDelta::LineDelta(0.0, -1.0), false),
            InputEvent::Wheel { delta: -120 }
        ));
        assert!(matches!(
            wheel_event(&MouseScrollDelta::LineDelta(0.0, -1.0), true),
            InputEvent::HWheel { delta: 120 }
        ));
        set_natural_scroll(true);
        assert!(matches!(
            wheel_event(&MouseScrollDelta::LineDelta(0.0, -1.0), false),
            InputEvent::Wheel { delta: 120 }
        ));
        // 가로: Windows/Linux 양수 = 오른쪽(natural이면 반전) · macOS는 AppKit 부호가 반대라 한 번 더 뒤집힌다.
        let expect = if cfg!(target_os = "macos") { 120 } else { -120 };
        assert!(matches!(
            wheel_event(&MouseScrollDelta::LineDelta(1.0, 0.0), false),
            InputEvent::HWheel { delta } if delta == expect
        ));
        set_natural_scroll(false);
    }

    /// macOS 가로 부호 보정(09-16): natural off에서 AppKit 양수(왼쪽)가 `HWheel` 음수(왼쪽)로.
    #[test]
    fn horizontal_sign_matches_windows_convention() {
        set_natural_scroll(false);
        let expect = if cfg!(target_os = "macos") { -120 } else { 120 };
        assert!(matches!(
            wheel_event(&MouseScrollDelta::LineDelta(1.0, 0.0), false),
            InputEvent::HWheel { delta } if delta == expect
        ));
    }
}
