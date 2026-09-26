//! ★ **라이선스 창**(docs/23 §4-4 · T-34 · 09-27): 상태줄 배지 클릭 · Help ▸ License… → 모덜리스 창.
//! 위 = 상태 표(상태 · 파일 · id · 이름 · 종류/등급 · 기능 · 기한 · 빌드일 · 기기 코드) · 가운데 = 요청 코드(이름 · 이메일 · 복사) ·
//! 아래 = [라이선스 파일 열기…] [제거] [닫기] + 안내 줄. 판정·설치는 호스트의 `nsql_license::Licensing`(단일 원천) — 창은 보기·요청만.
//!
//! 창 골격은 Import 창과 같다(winit 창 + `Presenter` · 포커스 ≤ 1 · 마우스는 커서 아래 컨트롤에만 · Esc = 닫기).

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

/// 호스트가 그릴 때 넘기는 보기(라이선스 상태의 단일 원천은 `Licensing`).
#[derive(Clone, Debug, Default)]
pub(crate) struct LicView {
    /// 상태 한 줄(배지와 같은 글) · 경고색 여부.
    pub state: String,
    pub warn: bool,
    /// (라벨, 값) 표 — 호스트가 i18n으로 만든다.
    pub rows: Vec<(String, String)>,
    /// 요청 코드(기기 ID 없으면 None).
    pub request: Option<String>,
}

pub(crate) enum LicAction {
    None,
    Paint,
    Close,
    /// 파일 창을 열어 라이선스 파일을 고른다.
    OpenFile,
    Remove,
    /// 요청 코드를 클립보드로(이름 · 이메일 = 메타).
    CopyRequest(String, String),
}

pub(crate) struct LicenseWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    tb_name: TextBox,
    tb_email: TextBox,
    btn_copy: Button,
    btn_open: Button,
    btn_remove: Button,
    btn_close: Button,
    /// 안내 줄(설치됨 · 거부 · 복사됨) · 경고색.
    note: Option<(String, bool)>,
    /// 마지막 페인트의 보기(자체 시험 덤프용).
    last: LicView,
}

impl LicenseWin {
    pub(crate) fn new() -> Self {
        LicenseWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            tb_name: TextBox::new(""),
            tb_email: TextBox::new(""),
            btn_copy: Button::new(t(Msg::LicBtnCopyReq)),
            btn_open: Button::new(t(Msg::LicBtnOpen)),
            btn_remove: Button::new(t(Msg::LicBtnRemove)),
            btn_close: Button::new(t(Msg::LicBtnClose)),
            note: None,
            last: LicView::default(),
        }
    }

    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        owner: Option<&Window>,
    ) {
        if let Some(w) = &self.window {
            crate::winfocus::focus(w);
            self.redraw();
            return;
        }
        let size = winit::dpi::LogicalSize::new(640.0, 460.0);
        let attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinLicense)))
            .with_theme(theme)
            .with_resizable(true)
            .with_inner_size(size);
        let mut attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        if let Some(o) = owner {
            if let Ok(p) = o.outer_position() {
                let s = o.outer_size();
                attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(
                    p.x + s.width as i32 / 2
                        - (size.width * f64::from(o.scale_factor() as f32) / 2.0) as i32,
                    p.y + s.height as i32 / 4,
                ));
            }
        }
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        win.set_ime_allowed(crate::input::system_ime());
        crate::winfocus::focus(&win);
        self.window = Some(win);
        self.note = None;
        self.redraw();
    }

    /// 호스트: 설치/제거/복사 결과 한 줄.
    pub(crate) fn set_note(&mut self, text: String, warn: bool) {
        self.note = Some((text, warn));
        self.redraw();
    }

    /// 자체 시험 덤프(`license.dump:`).
    pub(crate) fn dump(&self) -> String {
        let mut s = format!(
            "open={} state={} warn={} request={}\n",
            self.window.is_some(),
            self.last.state,
            self.last.warn,
            self.last.request.as_deref().map_or(0, str::len)
        );
        for (k, v) in &self.last.rows {
            s.push_str(&format!("{k}={v}\n"));
        }
        if let Some((n, w)) = &self.note {
            s.push_str(&format!("note={}:{n}\n", if *w { "warn" } else { "ok" }));
        }
        s
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
        for b in [
            &mut self.btn_copy,
            &mut self.btn_open,
            &mut self.btn_remove,
            &mut self.btn_close,
        ] {
            b.clear_transient();
        }
        self.tb_name.set_focused(false);
        self.tb_email.set_focused(false);
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

    fn focused_box(&mut self) -> Option<&mut TextBox> {
        if self.tb_name.is_focused() {
            Some(&mut self.tb_name)
        } else if self.tb_email.is_focused() {
            Some(&mut self.tb_email)
        } else {
            None
        }
    }

    /// 포커스 링은 하나(CLAUDE.md §3 포커스 규칙).
    fn own_focus(&mut self, p: Point) {
        let hit = [self.tb_name.bounds(), self.tb_email.bounds()]
            .iter()
            .position(|b| b.contains(p));
        self.tb_name.set_focused(hit == Some(0));
        self.tb_email.set_focused(hit == Some(1));
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> LicAction {
        let mut inv = Invalidations::default();
        match ev {
            WindowEvent::CloseRequested => return LicAction::Close,
            WindowEvent::RedrawRequested => return LicAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                for b in [
                    &mut self.btn_copy,
                    &mut self.btn_open,
                    &mut self.btn_remove,
                    &mut self.btn_close,
                ] {
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
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                for b in [
                    &mut self.btn_copy,
                    &mut self.btn_open,
                    &mut self.btn_remove,
                    &mut self.btn_close,
                ] {
                    b.on_event(&mv, &mut inv);
                }
                for tb in [&mut self.tb_name, &mut self.tb_email] {
                    tb.on_event(&mv, &mut inv);
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
                    self.own_focus(p);
                }
                // 마우스 라우팅 규칙: 누름은 커서 아래 컨트롤에만 · 뗌은 전부.
                for tb in [&mut self.tb_name, &mut self.tb_email] {
                    if up || tb.bounds().contains(p) {
                        tb.on_event(&e, &mut inv);
                    }
                }
                for b in [
                    &mut self.btn_copy,
                    &mut self.btn_open,
                    &mut self.btn_remove,
                    &mut self.btn_close,
                ] {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                    b.set_focused(false);
                }
                self.redraw();
                if self.btn_copy.take_clicked() {
                    return LicAction::CopyRequest(
                        self.tb_name.text().trim().to_string(),
                        self.tb_email.text().trim().to_string(),
                    );
                }
                if self.btn_open.take_clicked() {
                    return LicAction::OpenFile;
                }
                if self.btn_remove.take_clicked() {
                    return LicAction::Remove;
                }
                if self.btn_close.take_clicked() {
                    return LicAction::Close;
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if *button == MouseButton::Right && *state == ElementState::Pressed =>
            {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                for tb in [&mut self.tb_name, &mut self.tb_email] {
                    if tb.bounds().contains(p) {
                        tb.on_event(&InputEvent::RightDown { x, y }, &mut inv);
                    }
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if let Some(tb) = self.focused_box() {
                    tb.set_preedit("", &mut inv);
                    for c in text.chars().filter(|c| !c.is_control()) {
                        tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                    }
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                if let Some(tb) = self.focused_box() {
                    tb.set_preedit(text, &mut inv);
                }
                self.redraw();
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                if matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) {
                    return LicAction::Close;
                }
                if let Key::Character(_) = kev.logical_key.as_ref() {
                    if self.primary && crate::input::shortcut_letter(kev) == Some('a') {
                        if let Some(tb) = self.focused_box() {
                            tb.on_event(&InputEvent::SelectAll, &mut inv);
                        }
                        self.redraw();
                        return LicAction::None;
                    }
                }
                if let Some(e) = self.key_event(kev) {
                    if let Some(tb) = self.focused_box() {
                        tb.on_event(&e, &mut inv);
                    }
                    self.redraw();
                }
            }
            _ => {}
        }
        LicAction::None
    }

    pub(crate) fn paint(&mut self, view: LicView, font: &Font, th: &Theme, ui_px: f32) {
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
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let px = |v: f32| (v * s).round() as i32;
        let inv = &mut Invalidations::default();
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
            let pad = px(12.0);
            let clip = Rect::new(0, 0, wi, hi);
            // 상태 줄(배지와 같은 글 · 경고색).
            let color = if view.warn { th.warn } else { th.ok };
            dc.fill_rect_alpha(
                Rect::new(pad, pad, wi - pad * 2, th_txt + px(10.0)),
                color,
                0.12,
            );
            dc.text(pad + px(8.0), pad + px(5.0), clip, &view.state, color);
            let mut y = pad + th_txt + px(18.0);
            // 표.
            let label_w = view
                .rows
                .iter()
                .map(|(k, _)| dc.text_width(k))
                .max()
                .unwrap_or(0)
                + px(16.0);
            let row_h = th_txt + px(6.0);
            for (k, v) in &view.rows {
                dc.text(pad, y, clip, k, th.text_dim);
                let vr = Rect::new(pad + label_w, y, wi - pad * 2 - label_w, row_h);
                let v = nexa_ctl::draw::ellipsize_middle(&mut dc, v, vr.w);
                dc.text(vr.x, y, vr, &v, th.text);
                y += row_h;
            }
            y += px(10.0);
            dc.fill_rect(Rect::new(pad, y, wi - pad * 2, 1), th.border);
            y += px(10.0);
            // 요청 코드 구역.
            dc.text(pad, y, clip, t(Msg::LicReqTitle), th.text);
            y += th_txt + px(8.0);
            let fh = th_txt + px(12.0);
            let mut x = pad;
            for (label, tb, w) in [
                (t(Msg::LicReqName), &mut self.tb_name, px(150.0)),
                (t(Msg::LicReqEmail), &mut self.tb_email, px(200.0)),
            ] {
                dc.text(x, y + (fh - th_txt) / 2, clip, label, th.text_dim);
                x += dc.text_width(label) + px(6.0);
                tb.set_scale(s);
                tb.set_bounds(Rect::new(x, y, w, fh), inv);
                tb.paint(&mut dc, th);
                x += w + px(12.0);
            }
            self.btn_copy.set_scale(s);
            let cw = dc.text_width(t(Msg::LicBtnCopyReq)) + px(28.0);
            self.btn_copy.set_enabled(view.request.is_some());
            self.btn_copy.set_bounds(Rect::new(x, y, cw, fh), inv);
            self.btn_copy.paint(&mut dc, th);
            y += fh + px(6.0);
            let code_line = view
                .request
                .as_deref()
                .map_or_else(|| t(Msg::LicNoMachine).to_string(), str::to_string);
            let code_line = nexa_ctl::draw::ellipsize_middle(&mut dc, &code_line, wi - pad * 2);
            dc.text(pad, y, clip, &code_line, th.text_dim);
            y += th_txt + px(6.0);
            let hint = t(Msg::LicHint);
            dc.text(pad, y, clip, hint, th.text_dim);
            // 버튼 행(아래) + 안내 줄(그 위).
            let btn_h = th_txt + px(14.0);
            let btn_y = hi - pad - btn_h;
            let note_y = btn_y - px(8.0) - th_txt;
            let mut bx = pad;
            for (b, label) in [
                (&mut self.btn_open, Msg::LicBtnOpen),
                (&mut self.btn_remove, Msg::LicBtnRemove),
                (&mut self.btn_close, Msg::LicBtnClose),
            ] {
                b.set_scale(s);
                let bw = (dc.text_width(t(label)) + px(28.0)).max(px(84.0));
                b.set_bounds(Rect::new(bx, btn_y, bw, btn_h), inv);
                b.paint(&mut dc, th);
                bx += bw + px(8.0);
            }
            if let Some((n, warn)) = &self.note {
                let n = nexa_ctl::draw::ellipsize_middle(&mut dc, n, wi - pad * 2);
                dc.text(pad, note_y, clip, &n, if *warn { th.danger } else { th.ok });
            }
        }
        for tb in [&self.tb_name, &self.tb_email] {
            if tb.popup_open() {
                let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
                let prefs = FontPrefs {
                    base: SlotFont {
                        size: ui_px,
                        bold: false,
                        italic: false,
                    },
                    ..FontPrefs::default()
                };
                let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
                tb.paint_popup(&mut dc, th);
            }
        }
        let _ = buf.present();
        self.surface = Some(surface);
        self.last = view;
    }
}
