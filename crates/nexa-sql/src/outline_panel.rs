//! **아웃라인 패널**(활동 막대 · docs/76 · 사용자 09-23 "현재 문서 기반 Outline") — 활성 탭의 심볼(문장 머리 · DEFINE/VARIABLE ·
//! CTE · CREATE 대상 · PL/SQL 서브프로그램/커서/타입/라벨 · 선언 변수)을 깊이대로 나열 · 필터 틀(`FilterBar` · 한글 자모) ·
//! 클릭/Enter = 그 자리로. 데이터는 호스트가 `set_symbols`로 준다(탭별 캐시 = `intel::Intel::outline_for` · 세대 열쇠).

use crate::filterbar::{FilterBar, FilterEvent, GAP_Y};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{InputEvent, Invalidations, Key as CtlKey, ScrollBars, TextBox};
use nsql_i18n::{t, tf, Msg};
use nsql_script::outline::{Outline, SymKind};

#[derive(Clone, Debug)]
struct Row {
    name: String,
    detail: String,
    kind: SymKind,
    line: usize,
    byte: usize,
    depth: u8,
}

pub(crate) struct OutlinePanel {
    visible: bool,
    bounds: Rect,
    scale: f32,
    filter: FilterBar,
    filter_text: String,
    all: Vec<Row>,
    rows: Vec<Row>,
    /// 지금 보이는 심볼의 열쇠(탭 id · 본문 세대) — 호스트가 바뀌었을 때만 다시 준다.
    key: Option<(u64, u64)>,
    scroll_y: i32,
    bars: ScrollBars,
    sel: Option<usize>,
    hover: Option<usize>,
    list_rect: Rect,
    header_rect: Rect,
    row_h: i32,
    focused: bool,
    clamp_w: i32,
    open: Option<usize>,
}

const ROW_H: f32 = 22.0;
const INPUT_H: f32 = 25.0;
const PAD: f32 = 8.0;
const INDENT: f32 = 14.0;

impl OutlinePanel {
    pub(crate) fn new() -> Self {
        OutlinePanel {
            visible: false,
            bounds: Rect::default(),
            scale: 1.0,
            filter: FilterBar::new(t(Msg::PhOutlineFilter), &[]),
            filter_text: String::new(),
            all: Vec::new(),
            rows: Vec::new(),
            key: None,
            scroll_y: 0,
            bars: ScrollBars::new(),
            sel: None,
            hover: None,
            list_rect: Rect::default(),
            header_rect: Rect::default(),
            row_h: 22,
            focused: false,
            clamp_w: i32::MAX / 2,
            open: None,
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    /// 필터 검색어 이력 잇기(전역 · `filter.outline` · 사용자 09-23).
    pub(crate) fn set_history(&mut self, h: crate::search_history::SharedHistory) {
        self.filter.set_history(h, "filter.outline");
    }

    /// 팝업 층(필터 편집 메뉴 · 이력 드롭다운 · 툴팁) — 창의 맨 마지막에.
    pub(crate) fn paint_popup(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        if self.visible {
            self.filter.paint_popup(dc, th);
        }
    }

    pub(crate) fn set_visible(&mut self, on: bool) {
        self.visible = on;
        if !on {
            self.set_focused(false);
        }
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        if !on {
            self.filter.set_focused(false);
        }
    }

    pub(crate) fn focus_filter(&mut self) {
        self.filter.set_focused(true);
    }

    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if self.filter.is_focused() {
            Some(self.filter.tb_mut())
        } else {
            None
        }
    }

    pub(crate) fn bounds(&self) -> Rect {
        if self.visible {
            self.bounds
        } else {
            Rect::default()
        }
    }

    pub(crate) fn set_clamp_width(&mut self, w: i32) {
        self.clamp_w = w;
        self.filter.set_clamp_width(w);
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        let px = |v: f32| (v * scale).round() as i32;
        let pad = px(PAD);
        let ih = px(INPUT_H);
        self.row_h = px(ROW_H);
        self.header_rect = Rect::new(b.x, b.y + px(4.0), b.w, self.row_h);
        let y1 = self.header_rect.bottom() + px(GAP_Y);
        self.filter.set_bounds(
            Rect::new(b.x + pad, y1, (b.w - pad * 2).max(px(80.0)), ih),
            scale,
        );
        let list_top = y1 + ih + px(GAP_Y);
        self.list_rect = Rect::new(b.x, list_top, b.w, (b.bottom() - list_top).max(0));
        self.clamp_scroll();
    }

    /// 지금 보이는 심볼의 열쇠(탭 · 세대) — 호스트가 비교해 바뀐 문서만 다시 준다.
    pub(crate) fn key(&self) -> Option<(u64, u64)> {
        self.key
    }

    /// 심볼 전체 교체(활성 탭이 바뀌거나 본문이 바뀌었을 때).
    pub(crate) fn set_symbols(&mut self, key: (u64, u64), ol: &Outline) {
        self.key = Some(key);
        self.all = ol
            .symbols
            .iter()
            .map(|s| Row {
                name: s.name.clone(),
                detail: s.detail.clone(),
                kind: s.kind,
                line: s.line,
                byte: s.byte,
                depth: s.depth,
            })
            .collect();
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let filtering = !self.filter.is_empty();
        self.rows = self
            .all
            .iter()
            .filter(|r| {
                !filtering
                    || self
                        .filter
                        .matches(&format!("{} {} {}", r.name, r.detail, r.kind.label()))
            })
            .cloned()
            .collect();
        if self.sel.is_some_and(|s| s >= self.rows.len()) {
            self.sel = None;
        }
        self.clamp_scroll();
    }

    fn content_h(&self) -> i32 {
        self.rows.len() as i32 * self.row_h
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h() - self.list_rect.h).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max);
    }

    fn row_at(&self, p: Point) -> Option<usize> {
        if !self.list_rect.contains(p) || self.row_h <= 0 {
            return None;
        }
        let r = ((p.y - self.list_rect.y + self.scroll_y) / self.row_h) as usize;
        (r < self.rows.len()).then_some(r)
    }

    fn reveal(&mut self, r: usize) {
        let top = r as i32 * self.row_h;
        if top < self.scroll_y {
            self.scroll_y = top;
        } else if top + self.row_h > self.scroll_y + self.list_rect.h {
            self.scroll_y = top + self.row_h - self.list_rect.h;
        }
        self.clamp_scroll();
    }

    /// 열기 요청(바이트 오프셋 · 1회성).
    pub(crate) fn take_open(&mut self) -> Option<usize> {
        self.open.take()
    }

    fn filter_feed(&mut self, ev: &InputEvent) -> FilterEvent {
        let mut inv = Invalidations::default();
        let evt = self.filter.on_event(ev, &mut inv);
        let now = self.filter.display_text();
        if evt == FilterEvent::Changed || now != self.filter_text {
            self.filter_text = now;
            self.rebuild();
        }
        if evt == FilterEvent::LeaveDown {
            // 이력 끝에서 ↓/Tab = 심볼 목록 첫 행으로(사용자 09-23).
            self.filter.set_focused(false);
            if !self.rows.is_empty() {
                self.sel = Some(0);
                self.reveal(0);
            }
        }
        evt
    }

    /// IME 조합·확정 뒤(호스트) — 조합 중 글자까지 바로 거른다.
    pub(crate) fn query_changed(&mut self) {
        if !self.filter.is_focused() {
            return;
        }
        self.filter.refresh();
        let now = self.filter.display_text();
        if now != self.filter_text {
            self.filter_text = now;
            self.rebuild();
        }
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.filter.tick(now_ms) | self.bars.tick(now_ms)
    }

    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> bool {
        if !self.visible {
            return false;
        }
        let mut inv = Invalidations::default();
        if matches!(
            ev,
            InputEvent::MouseMove { .. } | InputEvent::MouseUp { .. }
        ) && self.filter.on_event(ev, &mut inv) == FilterEvent::Changed
        {
            self.filter_text = self.filter.display_text();
            self.rebuild();
            return true;
        }
        let (_, ny, consumed) = self.bars.on_event(
            ev,
            self.list_rect,
            self.list_rect.w,
            self.content_h().max(self.list_rect.h),
            0,
            self.scroll_y,
            self.scale,
        );
        if ny != self.scroll_y {
            self.scroll_y = ny;
            self.clamp_scroll();
        }
        if consumed {
            return true;
        }
        match *ev {
            InputEvent::MouseMove { x, y } => {
                let h = self.row_at(Point { x, y });
                let changed = h != self.hover;
                self.hover = h;
                changed
            }
            InputEvent::MouseDown { x, y, .. } => {
                let p = Point { x, y };
                let in_f = self.filter.bounds().contains(p);
                self.filter.set_focused(in_f);
                if in_f {
                    self.filter_feed(ev);
                    return true;
                }
                if let Some(r) = self.row_at(p) {
                    self.sel = Some(r);
                    self.open = Some(self.rows[r].byte);
                } else if self.list_rect.contains(p) {
                    self.sel = None;
                }
                true
            }
            InputEvent::Wheel { delta } => {
                self.scroll_y -= delta / 120 * self.row_h * 3;
                self.clamp_scroll();
                true
            }
            InputEvent::Char { .. }
            | InputEvent::SelectAll
            | InputEvent::Undo
            | InputEvent::Redo
                if self.filter.is_focused() =>
            {
                self.filter_feed(ev);
                true
            }
            InputEvent::Key { key, .. } => {
                let n = self.rows.len();
                // 필터에 포커스면 이동 키는 이력(드롭다운/Flat)이 먼저 — 손대지 않으면(None) 종전대로 목록 이동.
                if self.filter.is_focused()
                    && matches!(
                        key,
                        CtlKey::Down
                            | CtlKey::Up
                            | CtlKey::Home
                            | CtlKey::End
                            | CtlKey::PageUp
                            | CtlKey::PageDown
                    )
                {
                    match self.filter_feed(ev) {
                        FilterEvent::None | FilterEvent::Side(_) => {}
                        _ => return true,
                    }
                }
                match key {
                    CtlKey::Down if n > 0 => {
                        let r = self.sel.map_or(0, |s| (s + 1).min(n - 1));
                        self.sel = Some(r);
                        self.reveal(r);
                        true
                    }
                    CtlKey::Up if n > 0 => {
                        let r = self.sel.map_or(0, |s| s.saturating_sub(1));
                        self.sel = Some(r);
                        self.reveal(r);
                        true
                    }
                    CtlKey::Enter => {
                        if let Some(r) = self
                            .sel
                            .or((n > 0 && self.filter.is_focused()).then_some(0))
                        {
                            self.open = Some(self.rows[r].byte);
                        }
                        true
                    }
                    CtlKey::Escape => {
                        if self.filter.is_focused() && !self.filter_text.is_empty() {
                            self.filter.set_text("");
                            self.filter_text.clear();
                            self.rebuild();
                            return true;
                        }
                        false
                    }
                    _ => {
                        if self.filter.is_focused() {
                            let _ = self.filter_feed(ev);
                            return true;
                        }
                        false
                    }
                }
            }
            _ => false,
        }
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let px = |v: f32| (v * self.scale).round() as i32;
        let b = self.bounds;
        let pad = px(PAD);
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        dc.select_font(FontSlot::Base, true);
        let hr = self.header_rect;
        let hy = dc.text_center_y(hr.y, hr.h);
        let title = format!("{} ({})", t(Msg::MnOutlinePanel), self.all.len());
        dc.text(hr.x + pad, hy, hr, &title, th.text);
        dc.select_font(FontSlot::Base, false);
        self.filter.paint(dc, th, true);
        let lr = self.list_rect;
        let rh = self.row_h;
        if self.rows.is_empty() {
            let msg = if self.all.is_empty() {
                t(Msg::OutlineEmpty).to_string()
            } else {
                tf(Msg::ProjNoMatch, &[])
            };
            let ty = dc.text_center_y(lr.y, rh);
            dc.text(lr.x + pad, ty, lr, &msg, th.text_dim);
            return;
        }
        let first = (self.scroll_y / rh) as usize;
        let mut y = lr.y - self.scroll_y % rh;
        for r in first..self.rows.len() {
            if y >= lr.bottom() {
                break;
            }
            let row = &self.rows[r];
            let row_rect = Rect::new(lr.x, y, lr.w, rh);
            if self.sel == Some(r) {
                dc.fill_rect(
                    row_rect,
                    if self.focused {
                        th.sel_bg
                    } else {
                        th.sel_bg_inactive
                    },
                );
            } else if self.hover == Some(r) {
                dc.fill_rect_alpha(row_rect, th.text, 0.06);
            }
            let ty = dc.text_center_y(y, rh);
            let indent = px(INDENT) * row.depth as i32;
            let mut tx = lr.x + pad + indent;
            // 줄 번호(흐린 작은 글) → 이름(문장은 흐리게 · 심볼은 본문색 · 서브프로그램은 굵게) → 종류/타입(오른쪽 · 흐림).
            dc.select_font(FontSlot::Status, false);
            let ln = row.line.to_string();
            let lw = dc.text_width(&ln);
            dc.text(tx, ty, row_rect, &ln, th.text_dim);
            tx += lw + px(8.0);
            let detail = if row.detail.is_empty() {
                row.kind.label().to_string()
            } else {
                row.detail.clone()
            };
            let dw = dc.text_width(&detail);
            let bold = matches!(
                row.kind,
                SymKind::Procedure
                    | SymKind::Function
                    | SymKind::Package
                    | SymKind::PackageBody
                    | SymKind::Trigger
            );
            dc.select_font(FontSlot::Base, bold);
            let fg = if row.kind == SymKind::Statement {
                th.text_dim
            } else if bold {
                th.accent
            } else {
                th.text
            };
            let clip = Rect::new(tx, y, (lr.right() - tx - dw - pad * 2).max(0), rh);
            let shown = nexa_ctl::draw::ellipsize_middle(dc, &row.name, clip.w);
            dc.text(tx, ty, clip, &shown, fg);
            dc.select_font(FontSlot::Status, false);
            dc.text(lr.right() - pad - dw, ty, row_rect, &detail, th.text_dim);
            dc.select_font(FontSlot::Base, false);
            y += rh;
        }
        self.bars.paint(
            dc,
            th,
            lr,
            lr.w,
            self.content_h().max(lr.h),
            0,
            self.scroll_y,
            self.scale,
        );
    }
}
