//! 편집기 탭(사용자 09-14) — nexa-ctl `TabBar`(dir2 이식 · U-2) + 탭마다 `TextBox` 버퍼.
//!
//! - 줄 수 = 설정 `tabs.rows`(**multi 기본** · single = ◀ ▶ 스크롤 + 드래그 이동).
//! - 툴팁 = 설정 `tabs.tooltip`(기본 켬) — 탭 위에 1초 머물면 카드(제목 · 문장 수 · 글자 수 · 접속). 내용은 호스트가 [`Editors::set_conn_desc`]로 준다.
//! - 세션 분리(`session.mode = per-editor`)는 T-54 — 지금은 모든 탭이 한 세션.

use crate::syntax::SyntaxRegistry;
use nexa_ctl::draw::{draw_tooltip, DrawCtx};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{
    Control, InputEvent, Invalidations, SyntaxSpec, TabAction, TabBar, TextBox, WhitespaceStyle,
    Widget,
};
use nsql_i18n::{t, Msg};
use std::rc::Rc;
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
    /// 탭별 구문(기본 = 제목 확장자 · 사용자 변경 시 그 탭만).
    syntax: Vec<Rc<SyntaxSpec>>,
    registry: Rc<SyntaxRegistry>,
    rulers: Vec<usize>,
    whitespace: WhitespaceStyle,
}

const HOVER_MS: u128 = 900;

impl Editors {
    pub(crate) fn new(
        line_numbers: bool,
        multiline: bool,
        tooltip_on: bool,
        registry: Rc<SyntaxRegistry>,
    ) -> Self {
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
            syntax: Vec::new(),
            registry,
            rulers: Vec::new(),
            whitespace: WhitespaceStyle::default(),
        };
        e.new_tab(None);
        e
    }

    fn make_box(&self, text: &str, syntax: &Rc<SyntaxSpec>) -> TextBox {
        let mut tb = TextBox::new(t(Msg::PhEditor))
            .with_multiline()
            .with_text(text);
        tb.set_line_numbers(self.line_numbers);
        tb.set_scale(self.scale);
        tb.set_highlighter(Some(syntax.clone()));
        tb.set_rulers(self.rulers.clone());
        tb.set_whitespace(self.whitespace);
        tb
    }

    /// 세로 안내선 열 목록(설정 `editor.rulers`).
    pub(crate) fn set_rulers(&mut self, cols: Vec<usize>) {
        self.rulers = cols;
        for b in &mut self.bufs {
            b.set_rulers(self.rulers.clone());
        }
    }

    /// 공백 표시 스타일(설정 `editor.whitespace*`).
    pub(crate) fn set_whitespace(&mut self, ws: WhitespaceStyle) {
        self.whitespace = ws;
        for b in &mut self.bufs {
            b.set_whitespace(ws);
        }
    }

    /// 활성 탭의 구문 이름.
    pub(crate) fn syntax_name(&self) -> String {
        self.syntax
            .get(self.active)
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }

    /// 활성 탭 구문 변경(팔레트 `Set Syntax`). 모르는 이름이면 false.
    pub(crate) fn set_syntax(&mut self, name: &str) -> bool {
        let Some(spec) = self.registry.get(name) else {
            return false;
        };
        if let Some(slot) = self.syntax.get_mut(self.active) {
            *slot = spec.clone();
        }
        self.cur_mut().set_highlighter(Some(spec));
        true
    }

    /// 캐럿 위치(1-기준 줄 · 열).
    pub(crate) fn caret_line_col(&self) -> (usize, usize) {
        let tb = self.cur();
        let text = tb.text();
        let caret = tb.caret();
        let mut line = 1;
        let mut col = 1;
        for (i, c) in text.chars().enumerate() {
            if i >= caret {
                break;
            }
            if c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
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

    /// 탭이 하나도 없으면 새 탭을 만든다(접속 성공을 활성 탭에 적용할 때 · 사용자 09-14).
    pub(crate) fn ensure_tab(&mut self) {
        if self.bufs.is_empty() {
            self.new_tab(None);
        }
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
        let syntax = self.registry.for_title(&title);
        let tb = self.make_box("", &syntax);
        self.bufs.push(tb);
        self.syntax.push(syntax);
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
        self.syntax.remove(i);
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
        self.bufs = texts
            .iter()
            .zip(self.syntax.iter())
            .map(|(s, syn)| self.make_box(s, syn))
            .collect();
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
                        let sy = self.syntax.remove(from);
                        self.bufs.insert(to, b);
                        self.titles.insert(to, t);
                        self.syntax.insert(to, sy);
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
        card.push_str(&format!(
            "\n{}: {}",
            t(Msg::PalSetSyntax),
            self.syntax.get(i).map(|s| s.name.as_str()).unwrap_or("")
        ));
        if !self.conn_desc.is_empty() {
            card.push('\n');
            card.push_str(t(Msg::TipConnection));
            card.push_str(": ");
            card.push_str(&self.conn_desc);
        }
        draw_tooltip(dc, th, r, clamp_w, &card, self.scale);
    }
}
