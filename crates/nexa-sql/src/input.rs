//! 입력 정책(사용자 09-14) — 휠·트랙패드 스크롤 방향 반전("자연스러운 스크롤" · 맥 관례).
//!
//! 프로세스 전역 플래그 하나(설정 `input.scroll_natural` · 기본 off): 창 세 개(메인·접속·로그)가 같은 변환
//! [`wheel_event`]를 쓰므로 방향 규칙이 한 곳에 있다. macOS는 OS가 이미 트랙패드 방향을 반전해 주므로 기본 off가
//! 그대로 맞고, Windows/Linux에서 맥처럼 쓰고 싶을 때 켠다.

use nexa_ctl::InputEvent;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use winit::event::MouseScrollDelta;

static NATURAL: AtomicBool = AtomicBool::new(false);

/// ★ 휠 잔여 누적(사용자 09-16 "트랙패드로 천천히 스크롤하면 미동작"): 소비자(스크롤바·로그 창·트리)가 전부 `delta/3`
/// px로 쓰므로 |delta| < 3인 사건(맥 트랙패드 느린 이동 = 1~2px · 소수 px)은 통째로 버려졌다. 여기서 축별로 누적해
/// **3의 배수만** 내보내고 나머지는 다음 사건으로 이월한다 — 손실 0 · 방향이 바뀌면 잔여는 버린다. 창 3개가 공유해도
/// 잔여는 3px 미만이라 무해.
static WHEEL_REM: Mutex<(f32, f32)> = Mutex::new((0.0, 0.0));

/// 누적 후 3의 배수로 양자화(부호 반전 시 잔여 초기화).
fn quantize3(v: f32, rem: &mut f32) -> i32 {
    if (v > 0.0 && *rem < 0.0) || (v < 0.0 && *rem > 0.0) {
        *rem = 0.0;
    }
    let total = *rem + v;
    let q = (total / 3.0).trunc() * 3.0;
    *rem = total - q;
    q as i32
}

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
    let (fx, fy) = match delta {
        MouseScrollDelta::LineDelta(dx, dy) => (*dx * 120.0, *dy * 120.0),
        MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
    };
    let (mut dx, mut dy) = {
        let mut rem = WHEEL_REM.lock().unwrap_or_else(|e| e.into_inner());
        (quantize3(fx, &mut rem.0), quantize3(fy, &mut rem.1))
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

    /// 느린 트랙패드(1~2px 사건)도 누적돼 잃지 않는다 · 부호가 바뀌면 잔여를 버린다(09-16).
    #[test]
    fn small_deltas_accumulate_to_multiples_of_three() {
        let mut rem = 0.0f32;
        assert_eq!(quantize3(1.0, &mut rem), 0);
        assert_eq!(quantize3(1.0, &mut rem), 0);
        assert_eq!(quantize3(1.0, &mut rem), 3, "1+1+1 = 3 → 한 번에 나간다");
        assert_eq!(quantize3(4.5, &mut rem), 3);
        assert_eq!(quantize3(1.5, &mut rem), 3, "잔여 1.5 + 1.5");
        assert_eq!(quantize3(-2.0, &mut rem), 0, "방향 반전 = 잔여 초기화 후 -2");
        assert_eq!(quantize3(-1.0, &mut rem), -3);
        // 마우스 휠 한 노치(120)는 그대로.
        let mut rem = 0.0f32;
        assert_eq!(quantize3(120.0, &mut rem), 120);
        assert_eq!(rem, 0.0);
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
