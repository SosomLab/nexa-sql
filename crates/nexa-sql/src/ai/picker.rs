//! ★ 큰 선택 팝업(사용자 11-11 "선택 가능한 대상은 선택 버튼을 누르면 크게 보여주며 선택" · 참고 = VS Code Claude 모델 선택):
//! 제목 줄 + **두 줄 행**(이름 · 설명 흐리게) + 현재 ✓ + 휠 스크롤(보이는 행 상한) + 꼬리 행(노력도 점 5개 · Auto 스위치).
//! 순수 상태 기계 — 호스트(패널)가 사건을 넘기고 [`PickEvent`]를 받는다. 바깥 클릭은 `Close`로 알리고 사건은 호스트가 이어 쓴다.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{InputEvent, Key as CtlKey};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickItem {
    pub id: String,
    pub title: String,
    pub desc: String,
}

/// 목록 아래 꼬리 행.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Extra {
    /// 노력도 — 점 `n`개 중 `idx`번째가 현재.
    Effort { label: String, idx: usize, n: usize },
    /// 스위치.
    Toggle { label: String, on: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PickEvent {
    None,
    Redraw,
    Pick(String),
    Effort(usize),
    Toggle(bool),
    /// 닫기(Esc · 바깥 클릭 — 바깥 클릭 사건은 호스트가 이어서 처리).
    Close,
}

/// 보이는 행 상한(그 이상은 스크롤).
const MAX_ROWS: usize = 8;

pub(crate) struct Picker {
    title: String,
    items: Vec<PickItem>,
    selected: Option<usize>,
    hover: Option<usize>,
    extra_hover: Option<usize>,
    scroll: i32,
    rect: Rect,
    lh: i32,
    scale: f32,
    extras: Vec<Extra>,
}

impl Picker {
    /// `anchor` 아래(안 들어가면 위)에 `host` 안으로 놓는다 · `lh` = 글줄 높이(패널이 마지막 그리기에서 잰 값).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        title: &str,
        items: Vec<PickItem>,
        selected: Option<usize>,
        extras: Vec<Extra>,
        anchor: Rect,
        host: Rect,
        lh: i32,
        scale: f32,
    ) -> Picker {
        let mut p = Picker {
            title: title.to_string(),
            items,
            selected,
            hover: selected,
            extra_hover: None,
            scroll: 0,
            rect: Rect::default(),
            lh,
            scale,
            extras,
        };
        let w = anchor.w.max(p.s(320.0)).min(host.w.max(p.s(120.0)));
        let rows = p.items.len().clamp(1, MAX_ROWS) as i32;
        let h = p.head_h() + rows * p.row_h() + p.extras.len() as i32 * p.extra_h() + p.s(8.0);
        let at = nexa_ctl::geom::place_popup(
            Point {
                x: anchor.x,
                y: anchor.bottom(),
            },
            (w, h),
            host,
        );
        p.rect = Rect::new(at.x, at.y, w, h);
        p.ensure_visible(selected);
        p
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }
    fn head_h(&self) -> i32 {
        self.lh + self.s(10.0)
    }
    pub(crate) fn row_h(&self) -> i32 {
        self.lh * 2 + self.s(10.0)
    }
    fn extra_h(&self) -> i32 {
        self.lh + self.s(14.0)
    }
    pub(crate) fn bounds(&self) -> Rect {
        self.rect
    }

    fn list_rect(&self) -> Rect {
        let rows = self.items.len().clamp(1, MAX_ROWS) as i32;
        Rect::new(
            self.rect.x,
            self.rect.y + self.head_h(),
            self.rect.w,
            rows * self.row_h(),
        )
    }
    fn extra_rect(&self, i: usize) -> Rect {
        let lr = self.list_rect();
        Rect::new(
            self.rect.x,
            lr.bottom() + self.s(4.0) + i as i32 * self.extra_h(),
            self.rect.w,
            self.extra_h(),
        )
    }
    /// 노력도 점 `k`의 자리(행 오른쪽 끝에서부터).
    fn dot_rect(&self, row: Rect, k: usize, n: usize) -> Rect {
        let d = self.s(10.0);
        let gap = self.s(8.0);
        let right = row.right() - self.s(12.0);
        let x = right - (n - k) as i32 * (d + gap) + gap;
        Rect::new(x, row.y + (row.h - d) / 2, d, d)
    }
    fn max_scroll(&self) -> i32 {
        (self.items.len() as i32 * self.row_h() - self.list_rect().h).max(0)
    }
    fn ensure_visible(&mut self, i: Option<usize>) {
        let Some(i) = i else { return };
        let rh = self.row_h();
        let top = i as i32 * rh;
        let lh = self.list_rect().h;
        if top < self.scroll {
            self.scroll = top;
        } else if top + rh > self.scroll + lh {
            self.scroll = top + rh - lh;
        }
        self.scroll = self.scroll.clamp(0, self.max_scroll());
    }
    fn row_at(&self, p: Point) -> Option<usize> {
        let lr = self.list_rect();
        if !lr.contains(p) {
            return None;
        }
        let i = ((p.y - lr.y + self.scroll) / self.row_h().max(1)) as usize;
        (i < self.items.len()).then_some(i)
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> PickEvent {
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let p = Point { x, y };
                let h = self.row_at(p);
                let eh = (0..self.extras.len()).find(|&i| self.extra_rect(i).contains(p));
                if h != self.hover || eh != self.extra_hover {
                    self.hover = h;
                    self.extra_hover = eh;
                    PickEvent::Redraw
                } else {
                    PickEvent::None
                }
            }
            InputEvent::Wheel { delta } => {
                let step = self.row_h();
                let ns = (self.scroll - delta.signum() * step).clamp(0, self.max_scroll());
                if ns != self.scroll {
                    self.scroll = ns;
                    PickEvent::Redraw
                } else {
                    PickEvent::None
                }
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                if !self.rect.contains(p) {
                    return PickEvent::Close;
                }
                if let Some(i) = self.row_at(p) {
                    return PickEvent::Pick(self.items[i].id.clone());
                }
                for (i, e) in self.extras.iter().enumerate() {
                    let r = self.extra_rect(i);
                    if !r.contains(p) {
                        continue;
                    }
                    match e {
                        Extra::Effort { n, idx, .. } => {
                            // 점 사이를 눌러도 가장 가까운 점.
                            let mut best = *idx;
                            let mut bd = i32::MAX;
                            for k in 0..*n {
                                let dr = self.dot_rect(r, k, *n);
                                let cx = dr.x + dr.w / 2;
                                let d = (cx - p.x).abs();
                                if d < bd {
                                    bd = d;
                                    best = k;
                                }
                            }
                            return PickEvent::Effort(best);
                        }
                        Extra::Toggle { on, .. } => return PickEvent::Toggle(!*on),
                    }
                }
                PickEvent::Redraw
            }
            InputEvent::Key {
                key: CtlKey::Escape,
                ..
            } => PickEvent::Close,
            InputEvent::Key {
                key: CtlKey::Enter, ..
            } => match self.hover {
                Some(i) => PickEvent::Pick(self.items[i].id.clone()),
                None => PickEvent::None,
            },
            InputEvent::Key {
                key: CtlKey::Down, ..
            } => {
                if self.items.is_empty() {
                    return PickEvent::None;
                }
                let n = self.hover.map_or(0, |h| (h + 1).min(self.items.len() - 1));
                self.hover = Some(n);
                self.ensure_visible(Some(n));
                PickEvent::Redraw
            }
            InputEvent::Key {
                key: CtlKey::Up, ..
            } => {
                let n = self.hover.map_or(0, |h| h.saturating_sub(1));
                self.hover = Some(n);
                self.ensure_visible(Some(n));
                PickEvent::Redraw
            }
            _ => PickEvent::None,
        }
    }

    /// 호스트가 값을 바꾼 뒤(노력도 · Auto) 꼬리 행 표시를 맞춘다.
    pub(crate) fn set_extra(&mut self, i: usize, e: Extra) {
        if let Some(slot) = self.extras.get_mut(i) {
            *slot = e;
        }
    }

    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        let r = self.rect;
        let rad = self.s(6.0);
        dc.shadow(r, th.text, nexa_ctl::tokens::Elevation::Mid, self.s(6.0));
        dc.fill_round_rect(r, rad, th.panel_bg);
        dc.stroke_round_rect(r, rad, th.border, 1.0);
        dc.select_font(FontSlot::Base, false);
        let pad = self.s(12.0);
        // 제목
        {
            let hr = Rect::new(r.x, r.y, r.w, self.head_h());
            let ty = dc.text_center_y(hr.y, hr.h);
            dc.text(r.x + pad, ty, hr, &self.title, th.text_dim);
        }
        // 행
        let lr = self.list_rect();
        let rh = self.row_h();
        let first = (self.scroll / rh.max(1)) as usize;
        let mut y = lr.y - self.scroll % rh.max(1);
        let check_w = dc.text_width("✓") + pad;
        for i in first..self.items.len() {
            if y >= lr.bottom() {
                break;
            }
            let row = Rect::new(lr.x + 1, y, lr.w - 2, rh);
            let clip = Rect::new(
                row.x,
                row.y.max(lr.y),
                row.w,
                (row.bottom().min(lr.bottom()) - row.y.max(lr.y)).max(0),
            );
            if self.hover == Some(i) {
                dc.fill_rect(clip, th.sel_bg);
            }
            let it = &self.items[i];
            let tw = row.w - pad * 2 - check_w;
            dc.select_font(FontSlot::Base, true);
            let title = nexa_ctl::draw::ellipsize_middle(dc, &it.title, tw.max(10));
            dc.text(row.x + pad, y + self.s(5.0), clip, &title, th.text);
            dc.select_font(FontSlot::Base, false);
            if !it.desc.is_empty() {
                let desc = nexa_ctl::draw::ellipsize_middle(dc, &it.desc, tw.max(10));
                dc.text(
                    row.x + pad,
                    y + self.s(5.0) + self.lh,
                    clip,
                    &desc,
                    th.text_dim,
                );
            }
            if self.selected == Some(i) {
                let cw = dc.text_width("✓");
                let ty = dc.text_center_y(y, rh);
                dc.text(row.right() - pad - cw, ty, clip, "✓", th.accent);
            }
            y += rh;
        }
        if self.items.len() > MAX_ROWS {
            // 스크롤 흔적(오른쪽 가는 띠).
            let total = self.items.len() as i32 * rh;
            let th_h = (lr.h * lr.h / total.max(1)).max(self.s(16.0));
            let ty = lr.y + (lr.h - th_h) * self.scroll / self.max_scroll().max(1);
            dc.fill_round_rect_alpha(
                Rect::new(lr.right() - self.s(5.0), ty, self.s(3.0), th_h),
                self.s(1.5),
                th.text,
                0.3,
            );
        }
        // 꼬리 행
        for (i, e) in self.extras.iter().enumerate() {
            let er = self.extra_rect(i);
            if i == 0 {
                dc.fill_rect(
                    Rect::new(r.x + pad, er.y - self.s(2.0), r.w - pad * 2, 1),
                    th.border,
                );
            }
            if self.extra_hover == Some(i) {
                dc.fill_rect_alpha(er, th.text, 0.05);
            }
            let ty = dc.text_center_y(er.y, er.h);
            match e {
                Extra::Effort { label, idx, n } => {
                    dc.text(er.x + pad, ty, er, label, th.text);
                    for k in 0..*n {
                        let dr = self.dot_rect(er, k, *n);
                        if k == *idx {
                            let big = Rect::new(
                                dr.x - self.s(2.0),
                                dr.y - self.s(2.0),
                                dr.w + self.s(4.0),
                                dr.h + self.s(4.0),
                            );
                            dc.fill_ellipse(big, th.accent);
                        } else if k < *idx {
                            dc.fill_ellipse(dr, th.text_dim);
                        } else {
                            dc.fill_ellipse(dr, th.border);
                        }
                    }
                }
                Extra::Toggle { label, on } => {
                    dc.text(er.x + pad, ty, er, label, th.text);
                    let tw = self.s(30.0);
                    let thh = self.s(16.0);
                    let tr = Rect::new(er.right() - pad - tw, er.y + (er.h - thh) / 2, tw, thh);
                    dc.fill_round_rect(tr, thh / 2, if *on { th.accent } else { th.border });
                    let kn = thh - self.s(4.0);
                    let kx = if *on {
                        tr.right() - kn - self.s(2.0)
                    } else {
                        tr.x + self.s(2.0)
                    };
                    dc.fill_ellipse(
                        Rect::new(kx, tr.y + self.s(2.0), kn, kn),
                        nexa_ctl::Color(0x00FF_FFFF),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(n: usize) -> Vec<PickItem> {
        (0..n)
            .map(|i| PickItem {
                id: format!("m{i}"),
                title: format!("Model {i}"),
                desc: "desc".into(),
            })
            .collect()
    }

    #[test]
    fn rows_scroll_and_extras() {
        let host = Rect::new(0, 0, 800, 600);
        let mut p = Picker::new(
            "Select a model",
            items(12),
            Some(1),
            vec![
                Extra::Effort {
                    label: "Effort".into(),
                    idx: 2,
                    n: 5,
                },
                Extra::Toggle {
                    label: "Auto".into(),
                    on: false,
                },
            ],
            Rect::new(10, 10, 200, 24),
            host,
            16,
            1.0,
        );
        assert_eq!(p.row_h(), 42);
        assert_eq!(p.bounds().w, 320, "폭은 최소 320");
        assert_eq!(p.list_rect().h, 8 * 42, "보이는 행 8개");
        // 둘째 행 클릭 = 선택.
        let lr = p.list_rect();
        let y = lr.y + 42 + 5;
        assert_eq!(
            p.on_event(&InputEvent::MouseDown {
                x: lr.x + 20,
                y,
                shift: false,
                primary: false
            }),
            PickEvent::Pick("m1".into())
        );
        // 휠 두 번 = 두 행 내림 · 그 뒤 같은 자리 = m3.
        assert_eq!(
            p.on_event(&InputEvent::Wheel { delta: -120 }),
            PickEvent::Redraw
        );
        assert_eq!(
            p.on_event(&InputEvent::Wheel { delta: -120 }),
            PickEvent::Redraw
        );
        assert_eq!(p.scroll, 84);
        assert_eq!(
            p.on_event(&InputEvent::MouseDown {
                x: lr.x + 20,
                y,
                shift: false,
                primary: false
            }),
            PickEvent::Pick("m3".into())
        );
        // 노력도 넷째 점 · Auto 켜기 · 바깥 = 닫기.
        let er = p.extra_rect(0);
        let dr = p.dot_rect(er, 3, 5);
        assert_eq!(
            p.on_event(&InputEvent::MouseDown {
                x: dr.x + dr.w / 2,
                y: er.y + er.h / 2,
                shift: false,
                primary: false
            }),
            PickEvent::Effort(3)
        );
        let tr = p.extra_rect(1);
        assert_eq!(
            p.on_event(&InputEvent::MouseDown {
                x: tr.x + 10,
                y: tr.y + tr.h / 2,
                shift: false,
                primary: false
            }),
            PickEvent::Toggle(true)
        );
        assert_eq!(
            p.on_event(&InputEvent::MouseDown {
                x: 790,
                y: 590,
                shift: false,
                primary: false
            }),
            PickEvent::Close
        );
        assert_eq!(
            p.on_event(&InputEvent::Key {
                key: CtlKey::Escape,
                shift: false,
                primary: false
            }),
            PickEvent::Close
        );
    }
}
