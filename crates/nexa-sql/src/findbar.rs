//! 찾기/바꾸기 **플로팅 위젯**(T-73 · 사용자 09-15 "VS Code의 Find/Replace 구조를 차용해 편집기 상단 중앙에 Floating") —
//! 편집기 위에 떠 있는 작은 패널(레이아웃 흐름 밖 · 본문을 밀지 않는다).
//!
//! ```text
//! [›] [찾기 ▢ Aa ab] n of m  ↑ ↓ ×
//!     [바꾸기 ▢]  [바꾸기][모두]          ← 왼쪽 › 를 누르면 펼침(⌄) · Ctrl+H는 펼친 채 연다
//! ```
//! Ctrl+F 열기(선택 텍스트를 씨앗으로) · Ctrl+H 바꾸기 열기 · Enter = 다음 · Shift+Enter = 이전 · Esc = 닫기(편집기로 포커스).
//! 검색 자체(대소문자 · 단어 단위 · 순환)는 호스트([`crate::main`])가 편집기 텍스트에 대해 수행하고 이 위젯은 입력·토글·상태만 가진다. 정규식은 T-59.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{Button, Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nsql_i18n::{t, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FindAction {
    None,
    Next,
    Prev,
    Replace,
    ReplaceAll,
    Close,
    /// 질의·토글이 바뀌었다(호스트가 개수를 다시 센다 · 첫 일치로 이동).
    Changed,
}

/// 패널 폭(논리 px) · 행 높이 · 여백.
const PANEL_W: f32 = 440.0;
const ROW_H: f32 = 30.0;
const PAD: f32 = 6.0;
const MARGIN_RIGHT: f32 = 20.0;

pub(crate) struct FindBar {
    visible: bool,
    with_replace: bool,
    bounds: Rect,
    scale: f32,
    query: TextBox,
    repl: TextBox,
    toggle_btn: Button,
    case_btn: Button,
    word_btn: Button,
    prev_btn: Button,
    next_btn: Button,
    repl_btn: Button,
    all_btn: Button,
    close_btn: Button,
    case_sensitive: bool,
    whole_word: bool,
    status: String,
    shift: bool,
}

impl FindBar {
    pub(crate) fn new() -> Self {
        FindBar {
            visible: false,
            with_replace: false,
            bounds: Rect::default(),
            scale: 1.0,
            query: TextBox::new(t(Msg::PhFind)),
            repl: TextBox::new(t(Msg::PhReplace)),
            toggle_btn: Button::new("›"),
            case_btn: Button::new(t(Msg::BtnMatchCase)),
            word_btn: Button::new(t(Msg::BtnWholeWord)),
            prev_btn: Button::new("↑"),
            next_btn: Button::new("↓"),
            repl_btn: Button::new(t(Msg::BtnReplace)),
            all_btn: Button::new(t(Msg::BtnReplaceAll)),
            close_btn: Button::new("×"),
            case_sensitive: false,
            whole_word: false,
            status: String::new(),
            shift: false,
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn bounds(&self) -> Rect {
        if self.visible {
            self.bounds
        } else {
            Rect::default()
        }
    }

    pub(crate) fn query(&self) -> String {
        self.query.text()
    }

    pub(crate) fn replacement(&self) -> String {
        self.repl.text()
    }

    pub(crate) fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub(crate) fn whole_word(&self) -> bool {
        self.whole_word
    }

    pub(crate) fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
    }

    /// 열기(이미 열려 있으면 질의 상자로 포커스) — `seed` = 편집기 선택 텍스트(한 줄일 때).
    pub(crate) fn open(&mut self, with_replace: bool, seed: Option<String>) {
        self.visible = true;
        self.with_replace |= with_replace;
        if let Some(s) = seed.filter(|s| !s.is_empty() && !s.contains('\n')) {
            self.query.set_text(&s);
        }
        self.sync_toggle_label();
        self.focus_query();
    }

    pub(crate) fn close(&mut self) {
        self.visible = false;
        self.with_replace = false;
        self.query.set_focused(false);
        self.repl.set_focused(false);
        for b in self.buttons() {
            b.set_focused(false);
        }
    }

    fn sync_toggle_label(&mut self) {
        self.toggle_btn
            .set_label(if self.with_replace { "⌄" } else { "›" });
    }

    fn focus_query(&mut self) {
        self.query.set_focused(true);
        self.repl.set_focused(false);
        for b in self.buttons() {
            b.set_focused(false);
        }
    }

    pub(crate) fn set_focused(&mut self, on: bool) {
        if on {
            if !self.repl.is_focused() {
                self.query.set_focused(true);
            }
        } else {
            self.query.set_focused(false);
            self.repl.set_focused(false);
            for b in self.buttons() {
                b.set_focused(false);
            }
        }
    }

    /// 포커스 텍스트박스(IME · 클립보드 라우팅).
    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if self.repl.is_focused() {
            Some(&mut self.repl)
        } else if self.query.is_focused() {
            Some(&mut self.query)
        } else {
            None
        }
    }

    fn buttons(&mut self) -> [&mut Button; 8] {
        [
            &mut self.toggle_btn,
            &mut self.case_btn,
            &mut self.word_btn,
            &mut self.prev_btn,
            &mut self.next_btn,
            &mut self.repl_btn,
            &mut self.all_btn,
            &mut self.close_btn,
        ]
    }

    /// 편집기 사각형 기준으로 **오른쪽 위**에 떠 있게 놓는다(VS Code 관례 · 본문을 밀지 않는다).
    pub(crate) fn set_bounds(&mut self, editor: Rect, scale: f32) {
        self.scale = scale;
        let s = scale;
        let px = |v: f32| (v * s).round() as i32;
        let pad = px(PAD);
        let row = px(ROW_H);
        let rows = if self.with_replace { 2 } else { 1 };
        let w = px(PANEL_W).min((editor.w - px(MARGIN_RIGHT) * 2).max(px(200.0)));
        let h = pad * 2 + row * rows + if rows == 2 { pad } else { 0 };
        let x = editor.right() - px(MARGIN_RIGHT) - w;
        let y = editor.y + px(4.0);
        self.bounds = Rect::new(x.max(editor.x), y, w, h);
        let b = self.bounds;
        let mut inv = Invalidations::default();
        let small = px(26.0);
        let y1 = b.y + pad;
        // 왼쪽 펼침 토글(두 행 높이 걸침).
        self.toggle_btn.set_scale(s);
        self.toggle_btn
            .set_bounds(Rect::new(b.x + pad, y1, px(18.0), h - pad * 2), &mut inv);
        let x0 = b.x + pad + px(18.0) + px(4.0);
        // 1행: [찾기 ▢][Aa][ab] 상태 [↑][↓][×]
        let close_x = b.right() - pad - small;
        let next_x = close_x - px(2.0) - small;
        let prev_x = next_x - px(2.0) - small;
        let status_w = px(84.0);
        let toggles_w = small * 2 + px(4.0);
        let qw = (prev_x - px(6.0) - status_w - toggles_w - px(4.0) - x0).max(px(80.0));
        self.query.set_scale(s);
        self.query.set_bounds(Rect::new(x0, y1, qw, row), &mut inv);
        let mut tx = x0 + qw + px(4.0);
        for btn in [&mut self.case_btn, &mut self.word_btn] {
            btn.set_scale(s);
            btn.set_bounds(Rect::new(tx, y1, small, row), &mut inv);
            tx += small + px(2.0);
        }
        for (btn, x) in [
            (&mut self.prev_btn, prev_x),
            (&mut self.next_btn, next_x),
            (&mut self.close_btn, close_x),
        ] {
            btn.set_scale(s);
            btn.set_bounds(Rect::new(x, y1, small, row), &mut inv);
        }
        // 2행: [바꾸기 ▢][바꾸기][모두]
        if self.with_replace {
            let y2 = y1 + row + pad;
            let all_w = px(52.0);
            let rep_w = px(76.0);
            let all_x = b.right() - pad - all_w;
            let rep_x = all_x - px(4.0) - rep_w;
            let rw = (rep_x - px(6.0) - x0).max(px(80.0));
            self.repl.set_scale(s);
            self.repl.set_bounds(Rect::new(x0, y2, rw, row), &mut inv);
            self.repl_btn.set_scale(s);
            self.repl_btn
                .set_bounds(Rect::new(rep_x, y2, rep_w, row), &mut inv);
            self.all_btn.set_scale(s);
            self.all_btn
                .set_bounds(Rect::new(all_x, y2, all_w, row), &mut inv);
        } else {
            self.repl.set_bounds(Rect::default(), &mut inv);
            self.repl_btn.set_bounds(Rect::default(), &mut inv);
            self.all_btn.set_bounds(Rect::default(), &mut inv);
        }
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if !self.visible {
            return false;
        }
        let mut any = self.query.tick(now_ms) | self.repl.tick(now_ms);
        for b in self.buttons() {
            any |= b.tick(now_ms);
        }
        any
    }

    pub(crate) fn animating(&self) -> bool {
        self.visible
            && (self.query.is_animating()
                || self.repl.is_animating()
                || self.toggle_btn.is_animating()
                || self.case_btn.is_animating()
                || self.word_btn.is_animating()
                || self.prev_btn.is_animating()
                || self.next_btn.is_animating()
                || self.repl_btn.is_animating()
                || self.all_btn.is_animating()
                || self.close_btn.is_animating())
    }

    /// 펼침 상태가 바뀌었는가(호스트가 다시 배치) — 1회성.
    pub(crate) fn with_replace(&self) -> bool {
        self.with_replace
    }

    /// 이벤트 → 호스트 동작. 마우스는 커서가 패널 안일 때 · 키는 패널에 포커스일 때 호스트가 넘긴다.
    pub(crate) fn on_event(&mut self, ev: &InputEvent) -> FindAction {
        if !self.visible {
            return FindAction::None;
        }
        let mut inv = Invalidations::default();
        if let InputEvent::Key { shift, .. } = ev {
            self.shift = *shift;
        }
        // Enter/Esc는 텍스트박스보다 먼저(단일행 박스의 Enter = commit이라 무해하지만 명확히).
        if let InputEvent::Key { key, shift, .. } = ev {
            match key {
                CtlKey::Enter => {
                    return if *shift {
                        FindAction::Prev
                    } else if self.repl.is_focused() {
                        FindAction::Replace
                    } else {
                        FindAction::Next
                    };
                }
                CtlKey::Escape => return FindAction::Close,
                _ => {}
            }
        }
        // 클릭 = 포커스 이동(링 ≤ 1 · CLAUDE.md §3).
        if let InputEvent::MouseDown { x, y, .. } = *ev {
            let p = Point { x, y };
            let in_q = self.query.bounds().contains(p);
            let in_r = self.with_replace && self.repl.bounds().contains(p);
            self.query.set_focused(in_q);
            self.repl.set_focused(in_r);
            let hit: Vec<bool> = self
                .buttons()
                .iter()
                .map(|b| b.bounds().contains(p))
                .collect();
            for (b, h) in self.buttons().iter_mut().zip(hit) {
                b.set_focused(h);
            }
        }
        self.query.on_event(ev, &mut inv);
        if self.with_replace {
            self.repl.on_event(ev, &mut inv);
        }
        for b in self.buttons() {
            b.on_event(ev, &mut inv);
        }
        if self.query.take_changed().is_some() {
            return FindAction::Changed;
        }
        let _ = self.repl.take_changed();
        if self.toggle_btn.take_clicked() {
            self.with_replace = !self.with_replace;
            self.sync_toggle_label();
            if !self.with_replace {
                self.repl.set_focused(false);
            }
            return FindAction::Changed;
        }
        if self.case_btn.take_clicked() {
            self.case_sensitive = !self.case_sensitive;
            return FindAction::Changed;
        }
        if self.word_btn.take_clicked() {
            self.whole_word = !self.whole_word;
            return FindAction::Changed;
        }
        if self.prev_btn.take_clicked() {
            return FindAction::Prev;
        }
        if self.next_btn.take_clicked() {
            return FindAction::Next;
        }
        if self.repl_btn.take_clicked() {
            return FindAction::Replace;
        }
        if self.all_btn.take_clicked() {
            return FindAction::ReplaceAll;
        }
        if self.close_btn.take_clicked() {
            return FindAction::Close;
        }
        FindAction::None
    }

    pub(crate) fn paint(&mut self, dc: &mut dyn DrawCtx, th: &Theme) {
        if !self.visible {
            return;
        }
        let b = self.bounds;
        let r = (4.0 * self.scale).round() as i32;
        // 떠 있는 패널 — 테두리 + 아래쪽 한 줄 더 진하게(그림자 대신).
        dc.fill_round_rect(b, r, th.chrome_bg);
        dc.stroke_round_rect(b, r, th.border, 1.0);
        dc.fill_rect(Rect::new(b.x + 1, b.bottom(), b.w - 2, 1), th.border);
        dc.select_font(FontSlot::Base, false);
        // 켜진 토글은 선택색 배경.
        if self.case_sensitive {
            dc.fill_round_rect(self.case_btn.bounds(), 4, th.sel_bg);
        }
        if self.whole_word {
            dc.fill_round_rect(self.word_btn.bounds(), 4, th.sel_bg);
        }
        self.toggle_btn.paint(dc, th);
        self.query.paint(dc, th);
        self.case_btn.paint(dc, th);
        self.word_btn.paint(dc, th);
        self.prev_btn.paint(dc, th);
        self.next_btn.paint(dc, th);
        if self.with_replace {
            self.repl.paint(dc, th);
            self.repl_btn.paint(dc, th);
            self.all_btn.paint(dc, th);
        }
        self.close_btn.paint(dc, th);
        // 상태(n of m · No results) — 토글 오른쪽 · ↑ 왼쪽에 우측 정렬.
        if !self.status.is_empty() {
            let tw = dc.text_width(&self.status);
            let th_txt = dc.text_height();
            let q = self.query.bounds();
            let x = self.prev_btn.bounds().x - tw - (8.0 * self.scale).round() as i32;
            dc.text(x, q.y + (q.h - th_txt) / 2, b, &self.status, th.text_dim);
        }
        self.query.paint_popup(dc, th);
        if self.with_replace {
            self.repl.paint_popup(dc, th);
        }
    }
}
