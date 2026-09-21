//! **변수 창**(docs/63 V2 · 사용자 09-21) — 지금 편집기 탭이 보는 변수 표(탭 층 + 연결 공유 층)를 보고 고치는 모덜리스 창.
//! PL/SQL Developer의 변수 격자 · SQL Workbench/J의 `WbVarList`(편집 가능한 목록)에 해당한다.
//!
//! - 표: 이름 · 타입 · 값 · 층(tab/shared). 이번 실행에서 **바뀐 줄은 강조**(바뀐 값이 눈에 들어오게) · 비밀 값은 `******`.
//! - 줄을 고르면 아래 입력란에 값이 들어온다 → **Set**(Enter) = 값 넣기(빈 칸·`NULL` = NULL · 수 · `'글'` · 그 밖 = 글 그대로) ·
//!   **NULL** · **Share/Local**(연결 공유 층으로 올리기/내리기 · D-135) · **Delete** · **Script**(실행 가능한 스크립트로 새 편집기 탭에).
//! - 입력란에 `이름 = 값`을 쓰면 **새 변수**를 만든다.
//!
//! 데이터는 호스트가 갖고(`App.tab_vars` · `Sess.shared_vars`) 창은 그릴 때 받은 줄만 본다. 다시 그리기는 변수 사건·탭 전환·
//! 이 창의 입력이 있을 때만(매 프레임 0 · docs/63 §4).

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 표의 한 줄(호스트가 만든다).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VarRow {
    pub name: String,
    pub ty: String,
    /// 보여 줄 값(비밀이면 가린 글).
    pub value: String,
    /// 입력란에 넣을 값(비밀이면 빈 글).
    pub edit: String,
    pub shared: bool,
    pub changed: bool,
}

pub(crate) enum VarsWinAction {
    None,
    Paint,
    /// 값 넣기(이름, 친 글) — 없는 이름이면 새 변수.
    Set(String, String),
    SetNull(String),
    /// 층 바꾸기(이름, 공유로 올리는가).
    Share(String, bool),
    Delete(String),
    Script,
}

const COLS: [(Msg, i32); 4] = [
    (Msg::VarsColName, 170),
    (Msg::VarsColType, 120),
    (Msg::VarsColValue, 0),
    (Msg::VarsColLayer, 70),
];

pub(crate) struct VarsWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    edit: TextBox,
    set_btn: Button,
    null_btn: Button,
    share_btn: Button,
    del_btn: Button,
    script_btn: Button,
    /// 마지막 페인트의 줄(히트 테스트·선택) · 고른 이름.
    rows: Vec<VarRow>,
    selected: Option<String>,
    hover: Option<usize>,
    scroll: i32,
    row_h: i32,
    table: Rect,
    title_ctx: String,
}

impl VarsWin {
    pub(crate) fn new() -> Self {
        VarsWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            edit: TextBox::new(t(Msg::VarsEditHint)),
            set_btn: Button::new(t(Msg::BtnVarsSet)),
            null_btn: Button::new("NULL"),
            share_btn: Button::new(t(Msg::BtnVarsShare)),
            del_btn: Button::new(t(Msg::BtnDelete)),
            script_btn: Button::new(t(Msg::BtnVarsScript)),
            rows: Vec::new(),
            selected: None,
            hover: None,
            scroll: 0,
            row_h: 0,
            table: Rect::default(),
            title_ctx: String::new(),
        }
    }

    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        near: Option<(i32, i32, u32)>,
        owner: Option<&Window>,
    ) {
        if let Some(w) = &self.window {
            w.focus_window();
            self.redraw();
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title(self.title())
            .with_theme(theme)
            .with_inner_size(winit::dpi::LogicalSize::new(560.0, 360.0));
        if let Some((x, y, w)) = near {
            attrs =
                attrs.with_position(winit::dpi::PhysicalPosition::new(x + w as i32 + 8, y + 120));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        // 화면 밖으로 나가지 않게(메인 창 오른쪽 기본 위치 · 해상도가 바뀐 뒤의 기억 위치).
        crate::wingeom::keep_on_screen(&win, owner);
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        win.set_ime_allowed(crate::input::system_ime());
        self.window = Some(win);
        self.edit.set_focused(true);
        self.redraw();
    }

    fn title(&self) -> String {
        if self.title_ctx.is_empty() {
            format!("Nexa SQL — {}", t(Msg::MnVariables))
        } else {
            format!("Nexa SQL — {} [{}]", t(Msg::MnVariables), self.title_ctx)
        }
    }

    /// 제목의 탭 이름(호스트가 탭 전환 때).
    pub(crate) fn set_context(&mut self, ctx: &str) {
        if self.title_ctx != ctx {
            self.title_ctx = ctx.to_string();
            self.selected = None;
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
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
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

    fn buttons(&mut self) -> [&mut Button; 5] {
        [
            &mut self.set_btn,
            &mut self.null_btn,
            &mut self.share_btn,
            &mut self.del_btn,
            &mut self.script_btn,
        ]
    }

    fn selected_row(&self) -> Option<&VarRow> {
        let name = self.selected.as_deref()?;
        self.rows.iter().find(|r| r.name.eq_ignore_ascii_case(name))
    }

    /// 입력란의 글 → (이름, 값): `이름 = 값`이면 그 이름 · 아니면 고른 줄의 이름.
    fn edit_target(&self) -> Option<(String, String)> {
        let text = self.edit.text();
        if let Some((l, r)) = text.split_once('=') {
            let name = l.trim().trim_start_matches(':');
            let ident = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '#'))
                && !name.starts_with(|c: char| c.is_ascii_digit());
            // 고른 줄이 없을 때만 `이름 = 값`으로 읽는다(값에 `=`가 들어갈 수 있다).
            if ident && self.selected.is_none() {
                return Some((name.to_string(), r.trim().to_string()));
            }
        }
        self.selected.clone().map(|n| (n, text))
    }

    fn key_event(&self, kev: &winit::event::KeyEvent) -> Option<InputEvent> {
        let key = |k: CtlKey| InputEvent::Key {
            key: k,
            shift: self.shift,
            primary: self.primary,
        };
        Some(match kev.logical_key.as_ref() {
            Key::Named(NamedKey::ArrowLeft) => key(CtlKey::Left),
            Key::Named(NamedKey::ArrowRight) => key(CtlKey::Right),
            Key::Named(NamedKey::Home) => key(CtlKey::Home),
            Key::Named(NamedKey::End) => key(CtlKey::End),
            Key::Named(NamedKey::Delete) => key(CtlKey::Delete),
            Key::Named(NamedKey::Backspace) => InputEvent::Char {
                c: '\u{8}',
                now_ms: 0,
            },
            Key::Named(NamedKey::Space) => InputEvent::Char { c: ' ', now_ms: 0 },
            Key::Character(t) => {
                if self.primary {
                    return None;
                }
                let c = t.chars().next()?;
                if c.is_control() {
                    return None;
                }
                InputEvent::Char { c, now_ms: 0 }
            }
            _ => return None,
        })
    }

    fn select(&mut self, i: usize) {
        let Some(r) = self.rows.get(i) else {
            return;
        };
        self.selected = Some(r.name.clone());
        let text = r.edit.clone();
        self.edit.set_text(&text);
        self.edit.set_focused(true);
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> VarsWinAction {
        let mut inv = Invalidations::default();
        match ev {
            WindowEvent::CloseRequested => self.close(),
            WindowEvent::RedrawRequested => return VarsWinAction::Paint,
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
                if up || self.edit.bounds().contains(p) {
                    self.edit.on_event(&e, &mut inv);
                }
                for b in self.buttons() {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                    // 포커스 링은 입력란 하나에만(버튼은 누르면 스스로 켠다).
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
                        self.close();
                        return VarsWinAction::None;
                    }
                    Key::Named(NamedKey::Enter) => {
                        return match self.edit_target() {
                            Some((n, v)) => VarsWinAction::Set(n, v),
                            None => VarsWinAction::None,
                        };
                    }
                    Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp)
                        if !self.rows.is_empty() =>
                    {
                        let n = self.rows.len();
                        let cur = self
                            .selected
                            .as_deref()
                            .and_then(|s| self.rows.iter().position(|r| r.name == s));
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
                        return VarsWinAction::None;
                    }
                    Key::Character(c) if self.primary && c.eq_ignore_ascii_case("v") => {
                        if let Some(text) = crate::clipboard::read_text() {
                            let line = text.lines().next().unwrap_or("").to_string();
                            self.edit.paste(&line, &mut inv);
                        }
                        self.redraw();
                        return VarsWinAction::None;
                    }
                    _ => {}
                }
                if let Some(e) = self.key_event(kev) {
                    self.edit.on_event(&e, &mut inv);
                    self.redraw();
                }
            }
            _ => {}
        }
        VarsWinAction::None
    }

    fn button_action(&mut self) -> VarsWinAction {
        if self.set_btn.take_clicked() {
            if let Some((n, v)) = self.edit_target() {
                return VarsWinAction::Set(n, v);
            }
        }
        let sel = self.selected_row().cloned();
        if self.null_btn.take_clicked() {
            if let Some(r) = &sel {
                return VarsWinAction::SetNull(r.name.clone());
            }
        }
        if self.share_btn.take_clicked() {
            if let Some(r) = &sel {
                return VarsWinAction::Share(r.name.clone(), !r.shared);
            }
        }
        if self.del_btn.take_clicked() {
            if let Some(r) = &sel {
                self.selected = None;
                self.edit.set_text("");
                return VarsWinAction::Delete(r.name.clone());
            }
        }
        if self.script_btn.take_clicked() {
            return VarsWinAction::Script;
        }
        VarsWinAction::None
    }

    pub(crate) fn paint(&mut self, rows_in: &[VarRow], font: &Font, th: &Theme, ui_px: f32) {
        let Some(win) = self.window.clone() else {
            return;
        };
        let Some(mut surface) = self.surface.take() else {
            return;
        };
        let size = win.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            self.surface = Some(surface);
            return;
        };
        if surface.resize(w, h).is_err() {
            self.surface = Some(surface);
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            self.surface = Some(surface);
            return;
        };
        if self.rows != rows_in {
            self.rows = rows_in.to_vec();
        }
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let px = |v: f32| (v * s).round() as i32;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let slot = |size: f32| SlotFont {
                size,
                bold: false,
                italic: false,
            };
            let prefs = FontPrefs {
                base: slot(ui_px),
                status: slot(ui_px),
                ..FontPrefs::default()
            };
            let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.panel_bg);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            let pad = px(8.0);
            let edit_h = th_txt + px(10.0);
            let btn_h = th_txt + px(12.0);
            let foot = edit_h + pad + btn_h;
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
            let fixed: i32 = COLS.iter().map(|(_, w)| px(*w as f32)).sum();
            let value_w = (table.w - fixed - px(2.0)).max(px(120.0));
            let widths: Vec<i32> = COLS
                .iter()
                .map(|(_, w)| if *w == 0 { value_w } else { px(*w as f32) })
                .collect();
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
                    t(Msg::VarsEmpty),
                    th.text_dim,
                );
            }
            for (ri, r) in self.rows.iter().enumerate().skip(first) {
                if y >= body.bottom() {
                    break;
                }
                let top = y.max(body.y);
                let clip = Rect::new(body.x, top, body.w, (y + row_h).min(body.bottom()) - top);
                let selected = self
                    .selected
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(&r.name));
                if selected {
                    dc.fill_rect_alpha(clip, th.accent, 0.16);
                } else if self.hover == Some(ri) {
                    dc.fill_rect_alpha(clip, th.text, 0.06);
                }
                if r.changed {
                    // 이번 실행에서 바뀐 줄 — 왼쪽 강조 띠.
                    dc.fill_rect(Rect::new(clip.x, clip.y, px(3.0), clip.h), th.accent);
                }
                let ty = y + (row_h - th_txt) / 2;
                let cells = [
                    format!(":{}", r.name),
                    r.ty.clone(),
                    r.value.clone(),
                    if r.shared { "shared" } else { "tab" }.to_string(),
                ];
                let mut cx = body.x;
                for (i, w) in widths.iter().enumerate() {
                    let cell = Rect::new(cx, clip.y, *w - px(4.0), clip.h);
                    let color = if i == 0 || i == 2 {
                        th.text
                    } else {
                        th.text_dim
                    };
                    dc.text(cx + px(6.0), ty, cell, &cells[i], color);
                    cx += w;
                }
                y += row_h;
            }
            // 입력란 + 버튼.
            let inv = &mut Invalidations::default();
            let ey = table.bottom() + pad;
            self.edit.set_scale(s);
            self.edit
                .set_bounds(Rect::new(pad, ey, wi - pad * 2, edit_h), inv);
            self.edit.paint(&mut dc, th);
            let shared_sel = self.selected_row().is_some_and(|r| r.shared);
            self.share_btn.set_label(if shared_sel {
                t(Msg::BtnVarsLocal)
            } else {
                t(Msg::BtnVarsShare)
            });
            let by = ey + edit_h + pad;
            let bw = ((wi - pad * 2 - px(8.0) * 4) / 5).max(px(60.0));
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
