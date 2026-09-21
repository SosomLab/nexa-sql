//! 창 크기 기억(사용자 09-17 "최종 종료된 창의 크기가 기억되도록 — 메인·로그인·로그·트랜잭션 로그 등"):
//! 닫힐 때 논리 px 크기를 설정(`window.<name>_size` · 숨김 · `"w,h"`)에 남기고 다음 열기 때 그 크기로 만든다.
//! 위치는 창마다 이미 있는 규칙(메인 창 기준 · `window.monitor`)을 따르고 크기만 기억한다.

use winit::window::Window;

/// `"w,h"` → (w, h) 논리 px — 너무 작은 값(< 200)은 무시(깨진 설정 fail-soft).
pub(crate) fn parse_size(v: &str) -> Option<(f64, f64)> {
    let (w, h) = v.trim().split_once(',')?;
    let (w, h) = (w.trim().parse::<f64>().ok()?, h.trim().parse::<f64>().ok()?);
    (w >= 200.0 && h >= 150.0 && w <= 10000.0 && h <= 10000.0).then_some((w, h))
}

/// 창의 현재 안쪽 크기(논리 px · 반올림).
pub(crate) fn logical_size(w: &Window) -> (f64, f64) {
    let s = w.scale_factor().max(0.25);
    let p = w.inner_size();
    (
        (f64::from(p.width) / s).round(),
        (f64::from(p.height) / s).round(),
    )
}

pub(crate) fn format_size(w: f64, h: f64) -> String {
    format!("{},{}", w.round() as i64, h.round() as i64)
}

/// `"x,y"` → 바깥 위치(**논리 좌표(points)** · 전역 — macOS는 모니터마다 배율이 달라 물리 px는 섞이면 틀린다 · 09-17).
pub(crate) fn parse_pos(v: &str) -> Option<(i32, i32)> {
    let (x, y) = v.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

pub(crate) fn format_pos(x: i32, y: i32) -> String {
    format!("{x},{y}")
}

/// (위치, 크기) — 같은 모니터 판정 결과.
pub(crate) type Placement = ((i32, i32), Option<(f64, f64)>);

/// 기억된 기하(창마다 · 설정 `window.<name>_pos`/`_size`) — 규칙(사용자 09-17): **같은 모니터면 기록 위치·크기**,
/// 다른 모니터면 기본 크기·기본 위치(메인 창 가운데/근처).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Memo {
    pub(crate) pos: Option<(i32, i32)>,
    pub(crate) size: Option<(f64, f64)>,
}

impl Memo {
    /// 기록 위치가 `owner`(메인 창)와 같은 모니터 안이면 `(위치, 크기)` — 아니면 None(= 기본 규칙).
    pub(crate) fn on_same_monitor(&self, owner: Option<&Window>) -> Option<Placement> {
        let pos = self.pos?;
        let m = owner.and_then(Window::current_monitor)?;
        rect_contains(monitor_rect(&m), pos).then_some((pos, self.size))
    }
}

/// 모니터 사각형(논리 좌표): winit의 물리 위치·크기를 그 모니터의 배율로 나눈다.
pub(crate) fn monitor_rect(m: &winit::monitor::MonitorHandle) -> (i32, i32, i32, i32) {
    let s = m.scale_factor().max(0.25);
    let (p, z) = (m.position(), m.size());
    (
        (f64::from(p.x) / s).round() as i32,
        (f64::from(p.y) / s).round() as i32,
        (f64::from(z.width) / s).round() as i32,
        (f64::from(z.height) / s).round() as i32,
    )
}

fn rect_contains(r: (i32, i32, i32, i32), p: (i32, i32)) -> bool {
    p.0 >= r.0 && p.0 < r.0 + r.2 && p.1 >= r.1 && p.1 < r.1 + r.3
}

/// 위치(논리)가 어느 모니터 안에라도 있는가(메인 창 복원 · 없어진 모니터의 좌표는 버린다).
pub(crate) fn on_any_monitor(
    pos: (i32, i32),
    mons: impl Iterator<Item = winit::monitor::MonitorHandle>,
) -> bool {
    mons.into_iter()
        .any(|m| rect_contains(monitor_rect(&m), pos))
}

/// 창의 바깥 위치(논리 좌표) — 실패하면 None.
pub(crate) fn outer_pos(w: &Window) -> Option<(i32, i32)> {
    let s = w.scale_factor().max(0.25);
    w.outer_position().ok().map(|p| {
        (
            (f64::from(p.x) / s).round() as i32,
            (f64::from(p.y) / s).round() as i32,
        )
    })
}

/// ★ 기억한 바깥 위치를 **생성 뒤 다시** 놓는다(사용자 09-19 "토글할수록 창이 위로 올라감"): macOS winit의 생성 인자
/// `with_position`은 내용 영역 기준으로 놓는데 저장은 `outer_position`(제목 표시줄 포함 프레임) 기준이라 열고 닫을 때마다
/// 제목 표시줄 높이만큼 위로 어긋났다. `set_outer_position`은 프레임 기준이라 정확하다(메인 창과 같은 방법).
pub(crate) fn place_outer(w: &Window, pos: Option<(i32, i32)>) {
    if let Some((x, y)) = pos {
        w.set_outer_position(logical(x, y));
    }
}

/// 창(물리 좌표의 위치·크기)을 모니터 사각형 **안으로** 옮긴 위치 — 오른쪽/아래로 넘치면 당기고, 그래도 크면 왼쪽/위에 맞춘다.
/// (창이 모니터보다 크면 제목 표시줄이 보이는 쪽 = 왼쪽 위를 지킨다.)
pub(crate) fn clamp_into(
    mon: (i32, i32, i32, i32),
    pos: (i32, i32),
    size: (i32, i32),
) -> (i32, i32) {
    let fit = |p: i32, len: i32, lo: i32, span: i32| -> i32 { p.min(lo + span - len).max(lo) };
    (
        fit(pos.0, size.0, mon.0, mon.2),
        fit(pos.1, size.1, mon.1, mon.3),
    )
}

/// ★ **보조 창이 화면 밖으로 나가지 않게**(Windows 종합 점검 09-21): "메인 창 오른쪽에 연다"는 기본 위치는 메인 창이 넓으면
/// 화면 밖으로 수백 px 나갔다(1920 폭 화면 · 메인 1391 → 로그 창 608 중 191px · 트랜잭션 로그 976 중 559px이 안 보였다).
/// 만든 직후에 부른다 — 창이 놓인 모니터(없으면 메인 창의 모니터) 안으로 당긴다. 기억한 위치로 연 창에도 같은 규칙
/// (모니터 해상도가 바뀌었을 수 있다). 옮겼으면 true.
pub(crate) fn keep_on_screen(w: &Window, owner: Option<&Window>) -> bool {
    let Some(m) = w
        .current_monitor()
        .or_else(|| owner.and_then(Window::current_monitor))
    else {
        return false;
    };
    let (Ok(p), z) = (w.outer_position(), w.outer_size()) else {
        return false;
    };
    let (mp, mz) = (m.position(), m.size());
    let mon = (mp.x, mp.y, mz.width as i32, mz.height as i32);
    let to = clamp_into(mon, (p.x, p.y), (z.width as i32, z.height as i32));
    if to == (p.x, p.y) {
        return false;
    }
    w.set_outer_position(winit::dpi::PhysicalPosition::new(to.0, to.1));
    true
}

/// 논리 위치 → winit 위치 인자.
pub(crate) fn logical(x: i32, y: i32) -> winit::dpi::LogicalPosition<f64> {
    winit::dpi::LogicalPosition::new(f64::from(x), f64::from(y))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clamp_keeps_windows_inside_the_monitor() {
        use super::clamp_into as f;
        let mon = (0, 0, 1920, 1080);
        assert_eq!(
            f(mon, (100, 100), (600, 400)),
            (100, 100),
            "안에 있으면 그대로"
        );
        assert_eq!(
            f(mon, (1503, 104), (608, 359)),
            (1312, 104),
            "오른쪽으로 넘침 → 당김"
        );
        assert_eq!(f(mon, (1503, 464), (976, 459)), (944, 464));
        assert_eq!(f(mon, (100, 900), (600, 400)), (100, 680), "아래로 넘침");
        assert_eq!(f(mon, (-50, -20), (600, 400)), (0, 0), "왼쪽·위로 넘침");
        assert_eq!(
            f(mon, (500, 300), (2400, 1300)),
            (0, 0),
            "모니터보다 크면 왼쪽 위"
        );
        // 둘째 모니터(오른쪽 · 음수 y 포함)도 자기 사각형 기준.
        let right = (1920, -200, 2560, 1440);
        assert_eq!(f(right, (4300, 1100), (600, 400)), (3880, 840));
        assert_eq!(f(right, (2000, -300), (600, 400)), (2000, -200));
    }

    #[test]
    fn parse_and_format() {
        assert_eq!(parse_size("1100,720"), Some((1100.0, 720.0)));
        assert_eq!(parse_size(" 640 , 552 "), Some((640.0, 552.0)));
        assert_eq!(parse_size("10,10"), None);
        assert_eq!(parse_size(""), None);
        assert_eq!(format_size(959.6, 420.2), "960,420");
    }
}
