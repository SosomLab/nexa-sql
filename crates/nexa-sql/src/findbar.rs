//! 찾기/바꾸기 바(T-73 · 사용자 09-15 "일반 DBMS 클라이언트 기본 기능") — 편집기 위에 한 줄.
//!
//! `[찾기 ▢][Aa][↑][↓]  [바꾸기 ▢][Replace][All]  n of m  [×]` · Ctrl+F 열기(선택 텍스트를 씨앗으로) · Ctrl+H 바꾸기 열기 ·
//! Enter = 다음 · Shift+Enter = 이전 · Esc = 닫기(편집기로 포커스). 검색 자체(대소문자 · 순환)는 호스트([`crate::main`])가
//! 편집기 텍스트에 대해 수행하고 이 바는 입력·버튼·상태만 가진다. 정규식은 T-59.

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
    /// 질의가 바뀌었다(호스트가 개수를 다시 센다 · 첫 일치로 이동).
    Changed,
}

const BAR_H: f32 = 34.0;

pub(crate) struct FindBar {
    visible: bool,
    with_replace: bool,
    bounds: Rect,
    scale: f32,
    query: TextBox,
    repl: TextBox,
    case_btn: Button,
    prev_btn: Button,
    next_btn: Button,
    repl_btn: Button,
    all_btn: Button,
    close_btn: Button,
    case_sensitive: bool,
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
            case_btn: Button::new(t(Msg::BtnMatchCase)),
            prev_btn: Button::new("↑"),
            next_btn: Button::new("↓"),
            repl_btn: Button::new(t(Msg::BtnReplace)),
            all_btn: Button::new(t(Msg::BtnReplaceAll)),
            close_btn: Button::new("×"),
            case_sensitive: false,
            status: String::new(),
            shift: false,
        }
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }

    pub(crate) fn height(&self, scale: f32) -> i32 {
        if self.visible {
            (BAR_H * scale).round() as i32
        } else {
            0
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

    fn buttons(&mut self) -> [&mut Button; 6] {
        [
            &mut self.case_btn,
            &mut self.prev_btn,
            &mut self.next_btn,
            &mut self.repl_btn,
            &mut self.all_btn,
            &mut self.close_btn,
        ]
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        let s = scale;
        let px = |v: f32| (v * s).round() as i32;
        let pad = px(6.0);
        let h = b.h - pad * 2;
        let y = b.y + pad;
        let mut inv = Invalidations::default();
        let mut x = b.x + pad;
        let qw = px(220.0);
        self.query.set_scale(s);
        self.query.set_bounds(Rect::new(x, y, qw, h), &mut inv);
        x += qw + px(4.0);
        let small = px(30.0);
        for (btn, w) in [
            (&mut self.case_btn, px(36.0)),
            (&mut self.prev_btn, small),
            (&mut self.next_btn, small),
        ] {
            btn.set_scale(s);
            btn.set_bounds(Rect::new(x, y, w, h), &mut inv);
            x += w + px(4.0);
        }
        if self.with_replace {
            x += px(10.0);
            let rw = px(200.0);
            self.repl.set_scale(s);
            self.repl.set_bounds(Rect::new(x, y, rw, h), &mut inv);
            x += rw + px(4.0);
            for (btn, w) in [
                (&mut self.repl_btn, px(80.0)),
                (&mut self.all_btn, px(50.0)),
            ] {
                btn.set_scale(s);
                btn.set_bounds(Rect::new(x, y, w, h), &mut inv);
                x += w + px(4.0);
            }
        } else {
            self.repl.set_bounds(Rect::default(), &mut inv);
            self.repl_btn.set_bounds(Rect::default(), &mut inv);
            self.all_btn.set_bounds(Rect::default(), &mut inv);
        }
        self.close_btn.set_scale(s);
        self.close_btn
            .set_bounds(Rect::new(b.right() - pad - small, y, small, h), &mut inv);
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
                || self.case_btn.is_animating()
                || self.prev_btn.is_animating()
                || self.next_btn.is_animating()
                || self.repl_btn.is_animating()
                || self.all_btn.is_animating()
                || self.close_btn.is_animating())
    }

    /// 이벤트 → 호스트 동작. 마우스는 커서가 바 안일 때 · 키는 바에 포커스일 때 호스트가 넘긴다.
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
        if self.case_btn.take_clicked() {
            self.case_sensitive = !self.case_sensitive;
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
        dc.fill_rect(b, th.chrome_bg);
        dc.fill_rect(Rect::new(b.x, b.bottom() - 1, b.w, 1), th.border);
        dc.select_font(FontSlot::Base, false);
        // Aa 토글은 켜져 있으면 선택색 배경.
        if self.case_sensitive {
            dc.fill_round_rect(self.case_btn.bounds(), 4, th.sel_bg);
        }
        self.query.paint(dc, th);
        self.case_btn.paint(dc, th);
        self.prev_btn.paint(dc, th);
        self.next_btn.paint(dc, th);
        if self.with_replace {
            self.repl.paint(dc, th);
            self.repl_btn.paint(dc, th);
            self.all_btn.paint(dc, th);
        }
        self.close_btn.paint(dc, th);
        // 상태(n of m) — 닫기 버튼 왼쪽.
        if !self.status.is_empty() {
            let tw = dc.text_width(&self.status);
            let th_txt = dc.text_height();
            let x = self.close_btn.bounds().x - tw - (10.0 * self.scale).round() as i32;
            dc.text(x, b.y + (b.h - th_txt) / 2, b, &self.status, th.text_dim);
        }
    }
}
