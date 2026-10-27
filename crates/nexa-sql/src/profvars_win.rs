//! ★ 프로필 변수 창(T-153 후속 · 10-27 141차 win · docs/63 §11 프로필 층 · docs/21 프로필 변수): 접속 폼의 [프로필 변수 (n)…] 버튼으로 열린다.
//!
//! - 표 = Name | Value(폼 `ConnectPanel::vars`가 단일 원천 — 창은 그릴 때 호스트에게 받는다 · 변수 창과 같은 꼴).
//! - 입력란 = 고른 줄의 값 · 고른 줄이 없으면 `이름 = 값`으로 새 변수 · 값이 비면 삭제(CLI `nsql conn var NAME=`와 같은 규칙).
//! - 창의 편집은 **폼에만** 돌아간다(`ProfVarsAction::Set/Delete` → `ConnectPanel::set_var/remove_var` = 바뀜 표식) · 파일 쓰기는 폼의 [저장]에서
//!   `Vault::set_vars`(스펙 저장과 한 트랜잭션 · 잃는 순간 지킴이 그대로).
//! - 모달이 아니다(접속 창의 딸린 창 · 접속 창이 닫히면 함께 닫힌다).

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, TextBox, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use std::rc::Rc;
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub(crate) enum ProfVarsAction {
    None,
    Paint,
    /// 값 넣기(이름, 값) — 없는 이름이면 새 변수 · 빈 값 = 삭제(호스트 = `ConnectPanel::set_var`).
    Set(String, String),
    Delete(String),
}

/// 열 폭(논리 px · 0 = 남는 폭 전부).
const COLS: [(Msg, i32); 2] = [(Msg::VarsColName, 180), (Msg::VarsColValue, 0)];

pub(crate) struct ProfVarsWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    edit: TextBox,
    set_btn: Button,
    del_btn: Button,
    close_btn: Button,
    /// 마지막 페인트의 줄(히트 테스트·선택) · 고른 이름.
    rows: Vec<(String, String)>,
    selected: Option<String>,
    hover: Option<usize>,
    scroll: i32,
    row_h: i32,
    table: Rect,
    /// 제목의 프로필 이름(호스트가 그릴 때 준다).
    profile: String,
    /// 안내 줄(이름 오류 등 · 비면 기본 안내).
    note: String,
    /// 창을 닫아 달라는 요청(닫기 버튼 · 호스트가 `close`).
    close_requested: bool,
}

impl ProfVarsWin {
    pub(crate) fn new() -> Self {
        ProfVarsWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            edit: TextBox::new(t(Msg::ProfVarsEditHint)),
            set_btn: Button::new(t(Msg::BtnVarsSet)),
            del_btn: Button::new(t(Msg::BtnDelete)),
            close_btn: Button::new(t(Msg::BtnClose)),
            rows: Vec::new(),
            selected: None,
            hover: None,
            scroll: 0,
            row_h: 0,
            table: Rect::default(),
            profile: String::new(),
            note: String::new(),
            close_requested: false,
        }
    }

    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        near: Option<(i32, i32, u32)>,
        owner: Option<&Window>,
        profile: &str,
    ) {
        self.profile = profile.to_string();
        if let Some(w) = &self.window {
            w.set_title(&self.title());
            crate::winfocus::focus(w);
            self.redraw();
            return;
        }
        let Some(o) = crate::winhost::open_window(
            el,
            crate::winhost::OpenSpec {
                title: self.title(),
                theme,
                near,
                dy: 80,
                owner,
                memo: None,
                default_size: (520.0, 320.0),
                ime: true,
            },
        ) else {
            return;
        };
        self.scale = o.scale;
        self.surface = o.surface;
        self.window = Some(o.window);
        self.selected = None;
        self.note.clear();
        self.edit.set_text("");
        self.edit.set_focused(true);
        self.redraw();
    }

    fn title(&self) -> String {
        if self.profile.is_empty() {
            format!("Nexa SQL - {}", t(Msg::WinProfileVars))
        } else {
            format!("Nexa SQL - {} [{}]", t(Msg::WinProfileVars), self.profile)
        }
    }

    /// 폼의 프로필이 바뀌었다(다른 프로필 불러오기 · New) — 제목·선택을 맞춘다.
    pub(crate) fn set_profile(&mut self, profile: &str) {
        if self.profile != profile {
            self.profile = profile.to_string();
            self.selected = None;
            self.edit.set_text("");
            if let Some(w) = &self.window {
                w.set_title(&self.title());
            }
            self.redraw();
        }
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
        self.edit.set_focused(false);
        for b in self.buttons() {
            b.clear_transient();
        }
        self.close_requested = false;
    }

    pub(crate) fn take_close_requested(&mut self) -> bool {
        std::mem::take(&mut self.close_requested)
    }

    pub(crate) fn is_open(&self) -> bool {
        self.window.is_some()
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// 안내 줄(호스트 = 이름 오류).
    pub(crate) fn set_note(&mut self, note: String) {
        self.note = note;
        self.redraw();
    }

    fn buttons(&mut self) -> [&mut Button; 3] {
        [&mut self.set_btn, &mut self.del_btn, &mut self.close_btn]
    }

    /// 입력란의 글 → (이름, 값): 고른 줄이 없으면 `이름 = 값` · 있으면 그 줄의 이름 + 글 전부가 값.
    fn edit_target(&self) -> Option<(String, String)> {
        let text = self.edit.text();
        if let Some(sel) = &self.selected {
            return Some((sel.clone(), text.trim().to_string()));
        }
        let (l, r) = text.split_once('=')?;
        let name = l.trim();
        (!name.is_empty()).then(|| (name.to_string(), r.trim().to_string()))
    }

    fn select(&mut self, i: usize) {
        let Some((n, v)) = self.rows.get(i) else {
            return;
        };
        self.selected = Some(n.clone());
        let v = v.clone();
        self.edit.set_text(&v);
        self.edit.set_focused(true);
        self.note.clear();
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> ProfVarsAction {
        let mut inv = Invalidations::default();
        match ev {
            WindowEvent::CloseRequested => self.close_requested = true,
            WindowEvent::RedrawRequested => return ProfVarsAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                for b in self.buttons() {
                    b.clear_transient();
                }
                self.redraw();
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let e = crate::input::wheel_event(delta, self.shift);
                if let InputEvent::Wheel { delta: px } = e {
                    self.scroll = (self.scroll - px / 3).max(0);
                    self.redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                self.edit.on_event(&mv, &mut inv);
                for b in self.buttons() {
                    b.on_event(&mv, &mut inv);
                }
                let over = if self.table.contains(Point { x, y }) && self.row_h > 0 {
                    let r = ((y - self.table.y + self.scroll) / self.row_h) as usize;
                    (r < self.rows.len()).then_some(r)
                } else {
                    None
                };
                if over != self.hover {
                    self.hover = over;
                    self.redraw();
                }
                if !inv.is_empty() {
                    self.redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } if *button == MouseButton::Left => {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                let e = match state {
                    ElementState::Pressed => InputEvent::MouseDown {
                        x,
                        y,
                        shift: self.shift,
                        primary: self.primary,
                    },
                    ElementState::Released => InputEvent::MouseUp { x, y },
                };
                let up = matches!(e, InputEvent::MouseUp { .. });
                if !up {
                    if let Some(i) = self.hover.filter(|_| self.table.contains(p)) {
                        self.select(i);
                    } else if self.table.contains(p) {
                        // 빈 자리 = 선택 풀기(새 변수 입력 `이름 = 값`).
                        self.selected = None;
                        self.edit.set_text("");
                    }
                }
                if up || self.edit.bounds().contains(p) || self.edit.popup_open() {
                    self.edit.on_event(&e, &mut inv);
                }
                for b in self.buttons() {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                    // 포커스 링은 입력란 하나에만.
                    b.set_focused(false);
                }
                self.redraw();
                return self.button_action();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.edit.set_preedit("", &mut inv);
                for c in text.chars().filter(|c| !c.is_control()) {
                    self.edit
                        .on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                self.edit.set_preedit(text, &mut inv);
                self.redraw();
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => {
                        self.close_requested = true;
                        return ProfVarsAction::None;
                    }
                    Key::Named(NamedKey::Enter) => {
                        return self.set_action();
                    }
                    Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp)
                        if !self.rows.is_empty() =>
                    {
                        let n = self.rows.len();
                        let cur = self
                            .selected
                            .as_deref()
                            .and_then(|s| self.rows.iter().position(|(r, _)| r == s));
                        let down =
                            matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::ArrowDown));
                        let next = match (cur, down) {
                            (None, true) => 0,
                            (None, false) => n - 1,
                            (Some(i), true) => (i + 1) % n,
                            (Some(i), false) => (i + n - 1) % n,
                        };
                        self.select(next);
                        self.redraw();
                        return ProfVarsAction::None;
                    }
                    Key::Character(_)
                        if self.primary && crate::input::shortcut_letter(kev) == Some('v') =>
                    {
                        if let Some(text) = crate::clipboard::read_text() {
                            let line = text.lines().next().unwrap_or("").to_string();
                            self.edit.paste(&line, &mut inv);
                        }
                        self.redraw();
                        return ProfVarsAction::None;
                    }
                    _ => {}
                }
                if let Some(e) = crate::input::text_key_event(
                    kev,
                    self.shift,
                    self.primary,
                    crate::input::TextKeys::Line,
                ) {
                    self.edit.on_event(&e, &mut inv);
                    self.redraw();
                }
            }
            _ => {}
        }
        ProfVarsAction::None
    }

    /// [값 넣기]·Enter: 입력란 → Set(이름, 값) · 넣은 뒤 입력란은 비우고 선택을 푼다(다음 변수 입력).
    fn set_action(&mut self) -> ProfVarsAction {
        match self.edit_target() {
            Some((n, v)) => {
                self.selected = None;
                self.edit.set_text("");
                self.note.clear();
                ProfVarsAction::Set(n, v)
            }
            None => ProfVarsAction::None,
        }
    }

    fn button_action(&mut self) -> ProfVarsAction {
        if self.set_btn.take_clicked() {
            return self.set_action();
        }
        if self.del_btn.take_clicked() {
            if let Some(n) = self.selected.take() {
                self.edit.set_text("");
                return ProfVarsAction::Delete(n);
            }
        }
        if self.close_btn.take_clicked() {
            self.close_requested = true;
        }
        ProfVarsAction::None
    }

    /// 그린다 — 줄은 호스트(폼)가 준다(단일 원천).
    pub(crate) fn paint(
        &mut self,
        rows_in: &[(String, String)],
        font: &Font,
        th: &Theme,
        ui_px: f32,
    ) {
        let Some(win) = self.window.clone() else {
            return;
        };
        let Some(mut surface) = self.surface.take() else {
            return;
        };
        let size = win.inner_size();
        let Some(mut buf) = surface.frame(size) else {
            self.surface = Some(surface);
            return;
        };
        if self.rows != rows_in {
            self.rows = rows_in.to_vec();
            if let Some(sel) = &self.selected {
                if !self.rows.iter().any(|(n, _)| n == sel) {
                    self.selected = None;
                }
            }
        }
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let px = |v: f32| (v * s).round() as i32;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs::with_base_status(ui_px);
            let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.panel_bg);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            let pad = px(8.0);
            let edit_h = th_txt + px(10.0);
            let note_h = th_txt + px(4.0);
            let btn_h = th_txt + px(12.0);
            let foot = edit_h + pad + note_h + pad + btn_h;
            let table = Rect::new(pad, pad, wi - pad * 2, (hi - pad * 3 - foot).max(0));
            dc.fill_rect(table, th.field_bg);
            dc.fill_rect(Rect::new(table.x, table.y, table.w, 1), th.border);
            dc.fill_rect(
                Rect::new(table.x, table.bottom() - 1, table.w, 1),
                th.border,
            );
            dc.fill_rect(Rect::new(table.x, table.y, 1, table.h), th.border);
            dc.fill_rect(Rect::new(table.right() - 1, table.y, 1, table.h), th.border);
            self.row_h = th_txt + px(6.0);
            let row_h = self.row_h;
            let name_w = px(COLS[0].1 as f32);
            let widths = [name_w, (table.w - px(2.0) - name_w).max(px(80.0))];
            let hdr = Rect::new(table.x + 1, table.y + 1, table.w - 2, row_h);
            dc.fill_rect(hdr, th.chrome_bg);
            let mut cx = hdr.x;
            for ((m, _), w) in COLS.iter().zip(widths.iter()) {
                let cell = Rect::new(cx, hdr.y, *w, row_h);
                dc.text(
                    cx + px(6.0),
                    hdr.y + (row_h - th_txt) / 2,
                    cell,
                    t(*m),
                    th.text,
                );
                dc.fill_rect(Rect::new(cx + w - 1, hdr.y, 1, row_h), th.border);
                cx += w;
            }
            let body = Rect::new(
                table.x + 1,
                hdr.bottom() + 1,
                table.w - 2,
                (table.bottom() - 1 - hdr.bottom() - 1).max(0),
            );
            self.table = body;
            let max_scroll = (self.rows.len() as i32 * row_h - body.h).max(0);
            self.scroll = self.scroll.clamp(0, max_scroll);
            let first = (self.scroll / row_h.max(1)) as usize;
            let mut y = body.y - (self.scroll % row_h.max(1));
            if self.rows.is_empty() {
                dc.text(
                    body.x + px(8.0),
                    body.y + px(8.0),
                    body,
                    t(Msg::ProfVarsEmpty),
                    th.text_dim,
                );
            }
            for (ri, (name, value)) in self.rows.iter().enumerate().skip(first) {
                if y >= body.bottom() {
                    break;
                }
                let top = y.max(body.y);
                let clip = Rect::new(body.x, top, body.w, (y + row_h).min(body.bottom()) - top);
                let selected = self.selected.as_deref() == Some(name.as_str());
                if selected {
                    dc.fill_rect_alpha(clip, th.accent, 0.16);
                } else if self.hover == Some(ri) {
                    dc.fill_rect_alpha(clip, th.text, 0.06);
                }
                let ty = y + (row_h - th_txt) / 2;
                let cells = [format!(":{name}"), value.clone()];
                let mut cx = body.x;
                for (i, w) in widths.iter().enumerate() {
                    let cell = Rect::new(cx, clip.y, *w - px(4.0), clip.h);
                    dc.text(cx + px(6.0), ty, cell, &cells[i], th.text);
                    cx += w;
                }
                y += row_h;
            }
            // 입력란 · 안내 줄 · 버튼.
            let inv = &mut Invalidations::default();
            let ey = table.bottom() + pad;
            self.edit.set_scale(s);
            self.edit
                .set_bounds(Rect::new(pad, ey, wi - pad * 2, edit_h), inv);
            self.edit.paint(&mut dc, th);
            let ny = ey + edit_h + pad;
            let (note, color) = if self.note.is_empty() {
                (t(Msg::ProfVarsNote).to_string(), th.text_dim)
            } else {
                (self.note.clone(), th.warn)
            };
            dc.text(
                pad,
                ny + (note_h - th_txt) / 2,
                Rect::new(pad, ny, wi - pad * 2, note_h),
                &note,
                color,
            );
            self.del_btn.set_enabled(self.selected.is_some());
            let by = ny + note_h + pad;
            let bw = ((wi - pad * 2 - px(8.0) * 2) / 3).max(px(60.0));
            let mut bx = pad;
            for b in self.buttons() {
                b.set_scale(s);
                b.set_bounds(Rect::new(bx, by, bw, btn_h), inv);
                b.paint(&mut dc, th);
                bx += bw + px(8.0);
            }
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}

/// 창이 폼에 적용할 Set 요청의 결과 안내(호스트가 `set_var` 실패 때 쓴다).
pub(crate) fn bad_name_note(name: &str) -> String {
    tf(Msg::ProfVarsBadName, &[name])
}
