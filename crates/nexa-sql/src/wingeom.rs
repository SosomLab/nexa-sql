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
