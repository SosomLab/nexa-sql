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

/// 기억된 기하(창마다 · 설정 `window.<name>_pos`/`_size`) — 규칙(사용자 09-17): **같은 모니터면 기록 위치·크기**,
/// 다른 모니터면 기본 크기·기본 위치(메인 창 가운데/근처).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Memo {
    pub(crate) pos: Option<(i32, i32)>,
    pub(crate) size: Option<(f64, f64)>,
}

impl Memo {
    /// 기록 위치가 `owner`(메인 창)와 같은 모니터 안이면 `(위치, 크기)` — 아니면 None(= 기본 규칙).
    pub(crate) fn on_same_monitor(
        &self,
        owner: Option<&Window>,
    ) -> Option<((i32, i32), Option<(f64, f64)>)> {
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

/// 논리 위치 → winit 위치 인자.
pub(crate) fn logical(x: i32, y: i32) -> winit::dpi::LogicalPosition<f64> {
    winit::dpi::LogicalPosition::new(f64::from(x), f64::from(y))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_and_format() {
        assert_eq!(parse_size("1100,720"), Some((1100.0, 720.0)));
        assert_eq!(parse_size(" 640 , 552 "), Some((640.0, 552.0)));
        assert_eq!(parse_size("10,10"), None);
        assert_eq!(parse_size(""), None);
        assert_eq!(format_size(959.6, 420.2), "960,420");
    }
}
