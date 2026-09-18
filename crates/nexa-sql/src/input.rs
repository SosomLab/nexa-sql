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
/// 단축키용 **영문 글자**(소문자) — 논리 키가 ASCII면 그것, 아니면(한글·일본어 입력 소스에서 ⌘C가 "ㅊ"으로 온다 · 사용자 09-19
/// "복사 단축키가 동작하지 않아") **물리 키**(`KeyCode::KeyA..Z`)로. 메인 창 키맵(`Chord::from_winit`)과 같은 규칙.
pub(crate) fn shortcut_letter(kev: &winit::event::KeyEvent) -> Option<char> {
    use winit::keyboard::{Key, KeyCode, PhysicalKey};
    if let Key::Character(t) = kev.logical_key.as_ref() {
        if let Some(c) = t.chars().next().filter(char::is_ascii_alphabetic) {
            return Some(c.to_ascii_lowercase());
        }
    }
    let PhysicalKey::Code(code) = kev.physical_key else {
        return None;
    };
    let c = match code {
        KeyCode::KeyA => 'a',
        KeyCode::KeyB => 'b',
        KeyCode::KeyC => 'c',
        KeyCode::KeyD => 'd',
        KeyCode::KeyE => 'e',
        KeyCode::KeyF => 'f',
        KeyCode::KeyG => 'g',
        KeyCode::KeyH => 'h',
        KeyCode::KeyI => 'i',
        KeyCode::KeyJ => 'j',
        KeyCode::KeyK => 'k',
        KeyCode::KeyL => 'l',
        KeyCode::KeyM => 'm',
        KeyCode::KeyN => 'n',
        KeyCode::KeyO => 'o',
        KeyCode::KeyP => 'p',
        KeyCode::KeyQ => 'q',
        KeyCode::KeyR => 'r',
        KeyCode::KeyS => 's',
        KeyCode::KeyT => 't',
        KeyCode::KeyU => 'u',
        KeyCode::KeyV => 'v',
        KeyCode::KeyW => 'w',
        KeyCode::KeyX => 'x',
        KeyCode::KeyY => 'y',
        KeyCode::KeyZ => 'z',
        _ => return None,
    };
    Some(c)
}

pub(crate) fn wheel_event(delta: &MouseScrollDelta, shift: bool) -> InputEvent {
    // ★ 픽셀 delta(macOS 트랙패드 · Linux libinput)는 **1:1**로(사용자 09-16 "DBeaver처럼 부드럽게 · 점진 가속"):
    //   소비자가 전부 `delta/3` px로 쓰므로 ×3 해 둔다. OS가 이미 가속·관성(손을 뗀 뒤 감쇠하는 사건)을 넣어 주므로
    //   손실 없이 그대로 흘리면 DBeaver와 같은 느낌이 된다. 휠 노치(LineDelta)는 종전대로 노치당 40px.
    let (fx, fy) = match delta {
        MouseScrollDelta::LineDelta(dx, dy) => (*dx * 120.0, *dy * 120.0),
        MouseScrollDelta::PixelDelta(p) => (p.x as f32 * 3.0, p.y as f32 * 3.0),
    };
    // ★ 축 잠금(사용자 09-16 "위로 올릴 때 흔들리거나 안 움직임"): 두 손가락 스와이프에는 미세한 가로 성분이 섞이는데,
    //   누적기 때문에 그 가로 잔여가 3px에 이를 때마다 세로 delta를 버리고 `HWheel`로 나가 세로 스크롤이 끊겼다.
    //   사건마다 **큰 축만** 쓰고 작은 축의 잔여는 버린다(트랙패드 관례 · 사선 드리프트 무시).
    let (mut dx, mut dy) = {
        let mut rem = WHEEL_REM.lock().unwrap_or_else(|e| e.into_inner());
        if fx.abs() > fy.abs() {
            rem.1 = 0.0;
            (quantize3(fx, &mut rem.0), 0)
        } else {
            rem.0 = 0.0;
            (0, quantize3(fy, &mut rem.1))
        }
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
        assert_eq!(
            quantize3(-2.0, &mut rem),
            0,
            "방향 반전 = 잔여 초기화 후 -2"
        );
        assert_eq!(quantize3(-1.0, &mut rem), -3);
        // 마우스 휠 한 노치(120)는 그대로.
        let mut rem = 0.0f32;
        assert_eq!(quantize3(120.0, &mut rem), 120);
        assert_eq!(rem, 0.0);
    }

    /// 픽셀 delta는 1:1(소비자의 /3에 맞춰 ×3) — 트랙패드 2.5px → 7.5 → 6 나가고 1.5 이월(09-16).
    #[test]
    fn pixel_delta_maps_one_to_one() {
        set_natural_scroll(false);
        {
            let mut rem = WHEEL_REM.lock().unwrap_or_else(|e| e.into_inner());
            *rem = (0.0, 0.0);
        }
        let p = winit::dpi::PhysicalPosition::new(0.0, -10.0);
        assert!(matches!(
            wheel_event(&MouseScrollDelta::PixelDelta(p), false),
            InputEvent::Wheel { delta: -30 }
        ));
        // 사선 드리프트(가로 0.4px)는 세로 사건을 빼앗지 않는다(축 잠금) · 여러 번 와도 가로로 새지 않는다.
        for _ in 0..10 {
            let p = winit::dpi::PhysicalPosition::new(0.4, -1.0);
            assert!(matches!(
                wheel_event(&MouseScrollDelta::PixelDelta(p), false),
                InputEvent::Wheel { .. }
            ));
        }
        // 가로가 큰 사건은 가로로.
        let p = winit::dpi::PhysicalPosition::new(5.0, 0.5);
        assert!(matches!(
            wheel_event(&MouseScrollDelta::PixelDelta(p), false),
            InputEvent::HWheel { .. }
        ));
        {
            let mut rem = WHEEL_REM.lock().unwrap_or_else(|e| e.into_inner());
            *rem = (0.0, 0.0);
        }
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
