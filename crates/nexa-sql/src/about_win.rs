//! ★ **About 창**(Help ▸ About · 사용자 09-27 "About 버튼이 미동작" — 종전엔 상태줄 한 줄뿐): 제품 · 버전 · 빌드일 · OS · 라이선스 상태 ·
//! 저작권/라이선스 · 저장소 · [정보 복사] [License…] [Close]. 라이선스 창과 같은 골격(winit 창 + `Presenter` · 버튼만 · Esc = 닫기).

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub(crate) enum AboutAction {
    None,
    Paint,
    Close,
    /// 정보 줄 전부를 클립보드로.
    Copy,
    /// 라이선스 창 열기.
    OpenLicense,
}

pub(crate) struct AboutWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    btn_copy: Button,
    btn_license: Button,
    btn_close: Button,
    note: Option<String>,
    last_lines: Vec<(String, String)>,
    /// 다음 페인트에서 창 높이를 내용에 맞춘다(열 때 · 사용자 09-27 "중간 공백 필요 없음").
    fit: bool,
}

impl AboutWin {
    pub(crate) fn new() -> Self {
        AboutWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            btn_copy: Button::new(t(Msg::AboutBtnCopy)),
            btn_license: Button::new(t(Msg::AboutBtnLicense)),
            btn_close: Button::new(t(Msg::LicBtnClose)),
            note: None,
            last_lines: Vec::new(),
            fit: true,
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
        let size = winit::dpi::LogicalSize::new(520.0, 300.0);
        let attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinAbout)))
            .with_theme(theme)
            .with_resizable(false)
            .with_inner_size(size);
        let mut attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        if let Some(o) = owner {
            if let Ok(p) = o.outer_position() {
                let s = o.outer_size();
                attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(
                    p.x + s.width as i32 / 2
                        - (size.width * f64::from(o.scale_factor() as f32) / 2.0) as i32,
                    p.y + s.height as i32 / 3,
                ));
            }
        }
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        crate::winfocus::focus(&win);
        self.window = Some(win);
        self.note = None;
        self.fit = true;
        self.redraw();
    }

    pub(crate) fn set_note(&mut self, text: String) {
        self.note = Some(text);
        self.redraw();
    }

    /// 복사용 본문(마지막 페인트의 줄들).
    pub(crate) fn info_text(&self) -> String {
        let mut s = String::new();
        for (k, v) in &self.last_lines {
            if k.is_empty() {
                s.push_str(v);
            } else {
                s.push_str(&format!("{k}: {v}"));
            }
            s.push('\n');
        }
        s
    }

    pub(crate) fn dump(&self) -> String {
        format!(
            "open={} lines={}\n{}",
            self.window.is_some(),
            self.last_lines.len(),
            self.info_text()
        )
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
        for b in [
            &mut self.btn_copy,
            &mut self.btn_license,
            &mut self.btn_close,
        ] {
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

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> AboutAction {
        let mut inv = Invalidations::default();
        match ev {
            WindowEvent::CloseRequested => return AboutAction::Close,
            WindowEvent::RedrawRequested => return AboutAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                for b in [
                    &mut self.btn_copy,
                    &mut self.btn_license,
                    &mut self.btn_close,
                ] {
                    b.clear_transient();
                }
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                for b in [
                    &mut self.btn_copy,
                    &mut self.btn_license,
                    &mut self.btn_close,
                ] {
                    b.on_event(&mv, &mut inv);
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
                        shift: false,
                        primary: false,
                    },
                    ElementState::Released => InputEvent::MouseUp { x, y },
                };
                let up = matches!(e, InputEvent::MouseUp { .. });
                for b in [
                    &mut self.btn_copy,
                    &mut self.btn_license,
                    &mut self.btn_close,
                ] {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                    b.set_focused(false);
                }
                self.redraw();
                if self.btn_copy.take_clicked() {
                    return AboutAction::Copy;
                }
                if self.btn_license.take_clicked() {
                    return AboutAction::OpenLicense;
                }
                if self.btn_close.take_clicked() {
                    return AboutAction::Close;
                }
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                if matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) {
                    return AboutAction::Close;
                }
            }
            _ => {}
        }
        AboutAction::None
    }

    /// `lines` = (라벨, 값) — 라벨이 비면 본문 한 줄(저작권 등).
    pub(crate) fn paint(
        &mut self,
        lines: Vec<(String, String)>,
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
            let pad = px(16.0);
            let clip = Rect::new(0, 0, wi, hi);
            let mut y = pad;
            // 제품 이름 줄(첫 줄 · 강조색).
            if let Some((_, title)) = lines.first() {
                dc.text(pad, y, clip, title, th.accent);
                y += th_txt + px(10.0);
            }
            let label_w = lines
                .iter()
                .skip(1)
                .filter(|(k, _)| !k.is_empty())
                .map(|(k, _)| dc.text_width(k))
                .max()
                .unwrap_or(0)
                + px(14.0);
            let row_h = th_txt + px(5.0);
            for (k, v) in lines.iter().skip(1) {
                if k.is_empty() {
                    let v = nexa_ctl::draw::ellipsize_middle(&mut dc, v, wi - pad * 2);
                    dc.text(pad, y, clip, &v, th.text_dim);
                } else {
                    dc.text(pad, y, clip, k, th.text_dim);
                    let vr = Rect::new(pad + label_w, y, wi - pad * 2 - label_w, row_h);
                    let v = nexa_ctl::draw::ellipsize_middle(&mut dc, v, vr.w);
                    dc.text(vr.x, y, vr, &v, th.text);
                }
                y += row_h;
            }
            // 버튼 = 창 바닥 · 창 높이 = 내용 끝 + 버튼 행(열 때 한 번 맞춘다 · 사용자 09-27).
            let btn_h = th_txt + px(14.0);
            let btn_y = hi - pad - btn_h;
            if self.fit {
                self.fit = false;
                let need = y + px(12.0) + btn_h + pad;
                if need != hi {
                    let _ = win.request_inner_size(winit::dpi::PhysicalSize::new(
                        size.width,
                        need.max(px(160.0)) as u32,
                    ));
                }
            }
            let mut bx = pad;
            for (b, label) in [
                (&mut self.btn_copy, Msg::AboutBtnCopy),
                (&mut self.btn_license, Msg::AboutBtnLicense),
                (&mut self.btn_close, Msg::LicBtnClose),
            ] {
                b.set_scale(s);
                let bw = (dc.text_width(t(label)) + px(28.0)).max(px(84.0));
                b.set_bounds(Rect::new(bx, btn_y, bw, btn_h), inv);
                b.paint(&mut dc, th);
                bx += bw + px(8.0);
            }
            if let Some(n) = &self.note {
                let nx = bx + px(4.0);
                let n = nexa_ctl::draw::ellipsize_middle(&mut dc, n, (wi - pad - nx).max(0));
                dc.text(
                    nx,
                    btn_y + (btn_h - th_txt) / 2,
                    Rect::new(nx, btn_y, (wi - pad - nx).max(0), btn_h),
                    &n,
                    th.ok,
                );
            }
        }
        let _ = buf.present();
        self.surface = Some(surface);
        self.last_lines = lines;
    }
}
