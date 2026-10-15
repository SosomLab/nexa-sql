//! ★ 이름 끌기 칩(10-15 · 30 §2 등재): 결과 열 머리 끌기(121차 `Grid::paint_drag_chip`)와 객체 탐색기 노드 끌기가 같은 모양 —
//! 커서 옆 작은 상자에 끌고 있는 이름(Shift = 전체 이름) · 팝업 층 · 창 안으로 밀어 넣음(61 §2-2 팝업 배치).
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;

/// 커서 `p` 오른쪽 아래에 `name` 칩. 오른쪽이 모자라면 왼쪽으로 · 세로는 창 안으로.
pub(crate) fn paint(dc: &mut dyn DrawCtx, th: &Theme, scale: f32, name: &str, p: Point) {
    let s = scale;
    let host = dc.surface_size().map(|(w, h)| Rect::new(0, 0, w, h));
    let pad = (8.0 * s).round() as i32;
    dc.select_font(FontSlot::Base, false);
    let tw = dc.text_width(name);
    let h = (22.0 * s).round() as i32;
    let w = tw + pad * 2;
    let mut r = Rect::new(p.x + (14.0 * s).round() as i32, p.y - h / 2, w, h);
    if let Some(host) = host {
        if r.right() > host.right() {
            r.x = (p.x - w - (6.0 * s).round() as i32).max(host.x);
        }
        r.y = r.y.clamp(host.y, (host.bottom() - h).max(host.y));
    }
    dc.fill_rect(r, th.chrome_bg);
    dc.fill_rect(Rect::new(r.x, r.y, r.w, 1), th.accent);
    dc.fill_rect(Rect::new(r.x, r.bottom() - 1, r.w, 1), th.accent);
    dc.fill_rect(Rect::new(r.x, r.y, 1, r.h), th.accent);
    dc.fill_rect(Rect::new(r.right() - 1, r.y, 1, r.h), th.accent);
    let ty = dc.text_center_y(r.y, r.h);
    dc.text(
        r.x + pad,
        ty,
        Rect::new(r.x + 1, r.y + 1, (r.w - 2).max(0), (r.h - 2).max(0)),
        name,
        th.text,
    );
}
