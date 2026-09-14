//! 편집기 탭(사용자 09-14) — nexa-ctl `TabBar`(dir2 이식 · U-2) + 탭마다 `TextBox` 버퍼.
//!
//! - 줄 수 = 설정 `tabs.rows`(**multi 기본** · single = ◀ ▶ 스크롤 + 드래그 이동).
//! - 툴팁 = 설정 `tabs.tooltip`(기본 켬) — 탭 위에 1초 머물면 카드(제목 · 문장 수 · 글자 수 · 접속). 내용은 호스트가 [`Editors::set_conn_desc`]로 준다.
//! - 세션 분리(`session.mode = per-editor`)는 T-54 — 지금은 모든 탭이 한 세션.

use nexa_ctl::draw::{draw_tooltip, DrawCtx};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, TabAction, TabBar, TextBox, Widget};
use nsql_i18n::{t, Msg};
use std::time::Instant;

pub(crate) struct Editors {
    tabs: TabBar,
    bufs: Vec<TextBox>,
    titles: Vec<String>,
    active: usize,
    counter: usize,
    bounds: Rect,
    scale: f32,
    line_numbers: bool,
    tooltip_on: bool,
    /// (탭 index · 머문 시작) — 1초 뒤 카드.
    hover: Option<(usize, Instant)>,
    cursor: (i32, i32),
    conn_desc: String,
}

const HOVER_MS: u128 = 900;

impl Editors {
    pub(crate) fn new(line_numbers: bool, multiline: bool, tooltip_on: bool) -> Self {
        let mut tabs = TabBar::new();
        tabs.set_multiline(multiline);
        tabs.set_show_new(true);
        let mut e = Editors {
            tabs,
            bufs: Vec::new(),
            titles: Vec::new(),
            active: 0,
            counter: 0,
            bounds: Rect::new(0, 0, 0, 0),
            scale: 1.0,
            line_numbers,
            tooltip_on,
            hover: None,
            cursor: (0, 0),
            conn_desc: String::new(),
        };
        e.new_tab(None);
        e
    }

    fn make_box(&self, text: &str) -> TextBox {
        let mut tb = TextBox::new(t(Msg::PhEditor))
            .with_multiline()
            .with_text(text);
        tb.set_line_numbers(self.line_numbers);
        tb.set_scale(self.scale);
        tb
    }

    /// 설정 화면(T-39)에서 바꿀 때 — 지금은 부팅 값만.
    #[allow(dead_code)]
    pub(crate) fn set_line_numbers(&mut self, on: bool) {
        self.line_numbers = on;
        for b in &mut self.bufs {
            b.set_line_numbers(on);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn set_tooltip(&mut self, on: bool) {
        self.tooltip_on = on;
    }

    pub(crate) fn set_conn_desc(&mut self, d: impl Into<String>) {
        self.conn_desc = d.into();
    }

    pub(crate) fn cur(&self) -> &TextBox {
        &self.bufs[self.active]
    }

    pub(crate) fn cur_mut(&mut self) -> &mut TextBox {
        &mut self.bufs[self.active]
    }

    pub(crate) fn editor_bounds(&self) -> Rect {
        self.cur().bounds()
    }

    /// 새 탭(제목 없으면 `Script_N`). 활성으로.
    pub(crate) fn new_tab(&mut self, title: Option<String>) {
        self.counter += 1;
        let title = title.unwrap_or_else(|| format!("Script_{}", self.counter));
        let tb = self.make_box("");
        self.bufs.push(tb);
        self.titles.push(title);
        self.active = self.bufs.len() - 1;
        self.sync_tabs();
    }

    pub(crate) fn close_tab(&mut self, i: usize) {
        if self.bufs.len() <= 1 || i >= self.bufs.len() {
            // 마지막 탭은 비우기만.
            if let Some(b) = self.bufs.get_mut(i) {
                b.set_text("");
            }
            return;
        }
        self.bufs.remove(i);
        self.titles.remove(i);
        if self.active >= self.bufs.len() {
            self.active = self.bufs.len() - 1;
        } else if i < self.active {
            self.active -= 1;
        }
        self.sync_tabs();
    }

    pub(crate) fn switch(&mut self, i: usize) {
        if i < self.bufs.len() {
            let focused = self.cur().is_focused();
            self.cur_mut().set_focused(false);
            self.active = i;
            self.cur_mut().set_focused(focused);
            self.sync_tabs();
        }
    }

    fn sync_tabs(&mut self) {
        let mut inv = Invalidations::default();
        self.tabs
            .set_tabs(self.titles.clone(), self.active, &mut inv);
        self.layout(&mut inv);
    }

    /// 언어 전환 등으로 placeholder를 다시 만들 때 — 본문 보존 재생성.
    pub(crate) fn rebuild_boxes(&mut self) {
        let texts: Vec<String> = self.bufs.iter().map(TextBox::text).collect();
        let focused = self.cur().is_focused();
        self.bufs = texts.iter().map(|s| self.make_box(s)).collect();
        self.cur_mut().set_focused(focused);
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        self.tabs.set_scale(scale);
        for tb in &mut self.bufs {
            tb.set_scale(scale);
        }
        let mut inv = Invalidations::default();
        self.layout(&mut inv);
    }

    fn layout(&mut self, inv: &mut Invalidations) {
        let b = self.bounds;
        let th = self.tabs.preferred_height().max(1);
        self.tabs.set_bounds(Rect::new(b.x, b.y, b.w, th), inv);
        let ed = Rect::new(b.x, b.y + th, b.w, (b.h - th).max(0));
        for tb in &mut self.bufs {
            tb.set_bounds(ed, inv);
        }
    }

    /// 탭 줄 수가 페인트에서 바뀌면(멀티라인 접힘) 다시 배치.
    pub(crate) fn relayout_if_needed(&mut self) -> bool {
        if self.tabs.take_lines_changed() {
            let mut inv = Invalidations::default();
            self.layout(&mut inv);
            return true;
        }
        false
    }

    /// 탭 바 이벤트(마우스가 탭 영역에 있거나 드래그 중). 소비했으면 true.
    pub(crate) fn route_tabs(&mut self, ev: &InputEvent, inv: &mut Invalidations) -> bool {
        let p = match *ev {
            InputEvent::MouseMove { x, y }
            | InputEvent::MouseDown { x, y, .. }
            | InputEvent::MouseUp { x, y }
            | InputEvent::RightDown { x, y } => Some(Point { x, y }),
            _ => None,
        };
        if let Some(p) = p {
            self.cursor = (p.x, p.y);
            let over = self.tabs.bounds().contains(p);
            // 툴팁 호버 추적
            let idx = if over && self.tooltip_on {
                self.tabs.tab_index_at(p.x, p.y)
            } else {
                None
            };
            match (idx, self.hover) {
                (Some(i), Some((h, _))) if h == i => {}
                (Some(i), _) => self.hover = Some((i, Instant::now())),
                (None, _) => self.hover = None,
            }
            if !over && self.tabs.dragging().is_none() {
                return false;
            }
            if !matches!(ev, InputEvent::MouseMove { .. }) {
                self.hover = None;
            }
        } else {
            let is_wheel = matches!(ev, InputEvent::Wheel { .. } | InputEvent::HWheel { .. });
            let over = self.tabs.bounds().contains(Point {
                x: self.cursor.0,
                y: self.cursor.1,
            });
            if !is_wheel || !over {
                return false;
            }
        }
        self.tabs.on_event(ev, inv);
        if let Some(a) = self.tabs.take_action() {
            match a {
                TabAction::Switch(i) => self.switch(i),
                TabAction::Close(i) => self.close_tab(i),
                TabAction::New => self.new_tab(None),
                TabAction::Move { from, to } => {
                    if from < self.bufs.len() && to < self.bufs.len() {
                        let b = self.bufs.remove(from);
                        let t = self.titles.remove(from);
                        self.bufs.insert(to, b);
                        self.titles.insert(to, t);
                        self.active = to;
                        self.sync_tabs();
                    }
                }
                TabAction::Context(_) => {}
            }
        }
        inv.push(self.bounds);
        true
    }

    /// 툴팁 타이머 — 카드가 뜰 시각이 되면 true(호스트가 다시 그린다).
    pub(crate) fn tick(&self) -> bool {
        matches!(self.hover, Some((_, t)) if t.elapsed().as_millis() >= HOVER_MS && t.elapsed().as_millis() < HOVER_MS + 40)
    }

    pub(crate) fn tooltip_pending(&self) -> bool {
        self.tooltip_on && self.hover.is_some()
    }

    pub(crate) fn paint_tabs(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        self.tabs.paint(dc, th);
    }

    /// 툴팁 카드(최상위 — 호스트가 맨 마지막에 부른다).
    pub(crate) fn paint_tooltip(&self, dc: &mut dyn DrawCtx, th: &Theme, clamp_w: i32) {
        let Some((i, since)) = self.hover else { return };
        if !self.tooltip_on || since.elapsed().as_millis() < HOVER_MS {
            return;
        }
        let Some(r) = self.tabs.tab_rect(i) else {
            return;
        };
        let text = self.bufs[i].text();
        let stmts = nsql_script::split_script(&text).len();
        let lines = text.split('\n').count();
        let mut card = format!(
            "{}\n{}: {} · {}: {} · {}: {}",
            self.titles[i],
            t(Msg::TipStatements),
            stmts,
            t(Msg::TipLines),
            lines,
            t(Msg::TipChars),
            text.chars().count()
        );
        if !self.conn_desc.is_empty() {
            card.push('\n');
            card.push_str(t(Msg::TipConnection));
            card.push_str(": ");
            card.push_str(&self.conn_desc);
        }
        draw_tooltip(dc, th, r, clamp_w, &card, self.scale);
    }
}
