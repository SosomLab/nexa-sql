//! 색 설정 창(사용자 09-14) — 버튼 hover 색 · 눌림 색을 nexa-ctl [`ColorPanel`](투명도·프리셋·최근 색)로 고른다.
//!
//! 메인 창의 소유 창(작업표시줄 항목 1개). 왼쪽 = 대상 2개(Hover · Pressed · 현재 색 칩) · 오른쪽 = 색 패널 ·
//! 아래 = 미리보기 버튼(고른 색이 실시간으로 hover/눌림에 반영된다) · 초기화 · 닫기.
//! 값은 `#RRGGBBAA`로 설정(`ui.hover_color` · `ui.pressed_color`)에 저장되고, 최근 색은 `ui.color_recent`.
//! 이 창은 색을 **고르기만** 한다 — 적용(nexa-ctl 토큰)·저장은 호스트가 [`ColorsAction::Changed`]를 받아서.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{
    rgba_from_hex, Button, ColorPanel, Control, InputEvent, Invalidations, Key as CtlKey, Widget,
};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 고르는 대상.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColorTarget {
    Hover,
    Pressed,
    /// 설정 레지스트리의 임의 색 키(`*_color` · 설정 창 "고르기" · 09-17) — 값은 `#RRGGBBAA`로 저장.
    Key(&'static str),
}

impl ColorTarget {
    pub(crate) const ALL: [ColorTarget; 2] = [ColorTarget::Hover, ColorTarget::Pressed];

    pub(crate) fn key(self) -> &'static str {
        match self {
            ColorTarget::Hover => "ui.hover_color",
            ColorTarget::Pressed => "ui.pressed_color",
            ColorTarget::Key(k) => k,
        }
    }

    fn label(self) -> Msg {
        match self {
            ColorTarget::Hover => Msg::LblColorHover,
            ColorTarget::Pressed => Msg::LblColorPressed,
            ColorTarget::Key(k) => nsql_settings::entry(k).map_or(Msg::WinColors, |e| e.label),
        }
    }

    fn idx(self) -> usize {
        match self {
            ColorTarget::Hover | ColorTarget::Key(_) => 0,
            ColorTarget::Pressed => 1,
        }
    }
}

/// 창이 호스트에 요청하는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ColorsAction {
    None,
    Paint,
    /// 값이 바뀌었다(드래그 중에도 — 실시간 미리보기) · `hex` = `#RRGGBBAA`.
    Changed {
        target: ColorTarget,
        hex: String,
    },
    /// 두 색 모두 테마 기본으로(키 모드 = 그 키만).
    Reset,
}

const PAD: f32 = 12.0;
const LIST_W: f32 = 150.0;
const ROW_H: f32 = 32.0;
const BTN_H: f32 = 28.0;

pub(crate) struct ColorsWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    panel: ColorPanel,
    target: ColorTarget,
    /// 대상별 값(`#RRGGBBAA` · None = 테마 기본).
    values: [Option<String>; 2],
    /// 키 모드(설정 창에서 연 임의 색 키) — 목록은 이 키 하나 · 값 None = 기본(빈 문자열).
    key_mode: Option<(&'static str, Option<String>)>,
    preview: Button,
    reset_btn: Button,
    close_btn: Button,
    list_rows: [Rect; 2],
}

impl ColorsWin {
    pub(crate) fn new(hover: Option<String>, pressed: Option<String>, recent: &[u32]) -> Self {
        let mut panel = ColorPanel::new(hover.as_deref().unwrap_or("#D8E8FFFF"));
        panel.set_recent(recent);
        ColorsWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            panel,
            target: ColorTarget::Hover,
            values: [hover, pressed],
            key_mode: None,
            preview: Button::new(t(Msg::BtnPreview)),
            reset_btn: Button::new(t(Msg::BtnReset)),
            close_btn: Button::new(t(Msg::BtnClose)),
            list_rows: [Rect::default(); 2],
        }
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// 버튼 hover 페이드 틱 — 다시 그려야 하면 true.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if self.window.is_none() {
            return false;
        }
        let a = self.preview.tick(now_ms);
        let b = self.reset_btn.tick(now_ms);
        let c = self.close_btn.tick(now_ms);
        let d = self.panel.tick(now_ms);
        a || b || c || d
    }

    pub(crate) fn animating(&self) -> bool {
        self.window.is_some()
            && (self.preview.is_animating()
                || self.reset_btn.is_animating()
                || self.close_btn.is_animating()
                || self.panel.is_animating())
    }

    /// 대상의 현재 hex(없으면 테마 기본 = `sel_bg` 불투명 · 키 모드 = 강조색 70%).
    fn value_of(&self, th: &Theme, tg: ColorTarget) -> String {
        if let ColorTarget::Key(_) = tg {
            return self
                .key_mode
                .as_ref()
                .and_then(|(_, v)| v.clone())
                .unwrap_or_else(|| format!("#{:06X}B3", th.accent.0 & 0x00FF_FFFF));
        }
        self.values[tg.idx()]
            .clone()
            .unwrap_or_else(|| format!("#{:06X}FF", th.sel_bg.0 & 0x00FF_FFFF))
    }

    /// 지금 보이는 대상 목록(키 모드 = 하나 · 기본 = hover·pressed).
    fn targets(&self) -> Vec<ColorTarget> {
        match self.key_mode {
            Some((k, _)) => vec![ColorTarget::Key(k)],
            None => ColorTarget::ALL.to_vec(),
        }
    }

    /// 설정 창의 색 키 "고르기" — 이 키 하나만 고르는 모드로 전환(열려 있어도 대상만 바꾼다).
    pub(crate) fn set_key_mode(&mut self, key: &'static str, value: Option<String>, th: &Theme) {
        self.key_mode = Some((key, value));
        self.target = ColorTarget::Key(key);
        let v = self.value_of(th, self.target);
        self.panel.set_value(&v);
        self.panel.set_focused(false);
        self.redraw();
    }

    /// 기본 모드(hover·pressed).
    pub(crate) fn set_default_mode(&mut self, th: &Theme) {
        if self.key_mode.take().is_some() {
            self.target = ColorTarget::Hover;
            let v = self.value_of(th, self.target);
            self.panel.set_value(&v);
            self.redraw();
        }
    }

    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        over: Option<(i32, i32, u32, u32)>,
        owner: Option<&Window>,
    ) {
        if let Some(w) = &self.window {
            w.focus_window();
            return;
        }
        let (lw, lh) = (500.0, 340.0);
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinColors)))
            .with_theme(theme)
            .with_resizable(false)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
        if let Some((x, y, w, h)) = over {
            let cx = x + (w as i32 - lw as i32) / 2;
            let cy = y + (h as i32 - lh as i32) / 2;
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(cx.max(0), cy.max(0)));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        self.window = Some(win);
        self.layout();
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
    }

    fn layout(&mut self) {
        let Some(win) = &self.window else { return };
        let size = win.inner_size();
        let (w, h) = (size.width as i32, size.height as i32);
        let s = self.scale;
        let pad = self.s(PAD);
        let mut inv = Invalidations::default();
        // 왼쪽 대상 목록.
        let list_x = pad;
        let mut y = pad + self.s(20.0);
        for i in 0..2 {
            self.list_rows[i] = Rect::new(list_x, y, self.s(LIST_W), self.s(ROW_H));
            y += self.s(ROW_H) + self.s(4.0);
        }
        // 오른쪽 색 패널.
        self.panel.set_scale(s);
        let px = pad + self.s(LIST_W) + pad;
        let (pw, ph) = self.panel.preferred_size();
        self.panel
            .set_bounds(Rect::new(px, pad, pw.min(w - px - pad), ph), &mut inv);
        // 아래 버튼 줄.
        let by = h - pad - self.s(BTN_H);
        let bw = self.s(96.0);
        let gap = self.s(8.0);
        for b in [&mut self.preview, &mut self.reset_btn, &mut self.close_btn] {
            b.set_scale(s);
        }
        self.preview
            .set_bounds(Rect::new(pad, by, bw, self.s(BTN_H)), &mut inv);
        self.close_btn
            .set_bounds(Rect::new(w - pad - bw, by, bw, self.s(BTN_H)), &mut inv);
        self.reset_btn.set_bounds(
            Rect::new(w - pad - bw * 2 - gap, by, bw, self.s(BTN_H)),
            &mut inv,
        );
    }

    fn to_input(&self, ev: &WindowEvent) -> Option<InputEvent> {
        let (x, y) = self.cursor;
        let key = |k: CtlKey| InputEvent::Key {
            key: k,
            shift: self.shift,
            primary: false,
        };
        Some(match ev {
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => InputEvent::MouseDown {
                    x,
                    y,
                    shift: self.shift,
                    primary: false,
                },
                (ElementState::Released, MouseButton::Left) => InputEvent::MouseUp { x, y },
                _ => return None,
            },
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Enter) => key(CtlKey::Enter),
                    Key::Named(NamedKey::ArrowLeft) => key(CtlKey::Left),
                    Key::Named(NamedKey::ArrowRight) => key(CtlKey::Right),
                    Key::Named(NamedKey::Home) => key(CtlKey::Home),
                    Key::Named(NamedKey::End) => key(CtlKey::End),
                    Key::Named(NamedKey::Delete) => key(CtlKey::Delete),
                    Key::Named(NamedKey::Backspace) => InputEvent::Char {
                        c: '\u{8}',
                        now_ms: 0,
                    },
                    Key::Character(t) => {
                        let c = t.chars().next()?;
                        if c.is_control() {
                            return None;
                        }
                        InputEvent::Char { c, now_ms: 0 }
                    }
                    _ => return None,
                }
            }
            _ => return None,
        })
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent, th: &Theme) -> ColorsAction {
        match ev {
            WindowEvent::CloseRequested => {
                self.close();
                return ColorsAction::None;
            }
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
                return ColorsAction::None;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
                self.redraw();
                return ColorsAction::None;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                return ColorsAction::None;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                let mut inv = Invalidations::default();
                self.panel.on_event(&mv, &mut inv);
                for b in [&mut self.preview, &mut self.reset_btn, &mut self.close_btn] {
                    b.on_event(&mv, &mut inv);
                }
                self.redraw();
                return self.after_panel();
            }
            WindowEvent::KeyboardInput { event: kev, .. }
                if kev.state == ElementState::Pressed
                    && matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) =>
            {
                self.close();
                return ColorsAction::None;
            }
            WindowEvent::RedrawRequested => return ColorsAction::Paint,
            _ => {}
        }
        let Some(ie) = self.to_input(ev) else {
            return ColorsAction::None;
        };
        let mut inv = Invalidations::default();
        if let InputEvent::MouseDown { x, y, .. } = ie {
            let p = Point { x, y };
            let targets = self.targets();
            for (i, r) in self.list_rows.iter().enumerate().take(targets.len()) {
                if r.contains(p) {
                    let tg = targets[i];
                    if tg != self.target {
                        self.target = tg;
                        let v = self.value_of(th, tg);
                        self.panel.set_value(&v);
                    }
                    self.panel.set_focused(false);
                    self.redraw();
                    return ColorsAction::None;
                }
            }
        }
        // 색 패널 + 버튼.
        self.panel.on_event(&ie, &mut inv);
        for b in [&mut self.preview, &mut self.reset_btn, &mut self.close_btn] {
            b.on_event(&ie, &mut inv);
        }
        // 포커스 규칙(CLAUDE.md §3): 마지막으로 누른 곳만 링 — 패널 밖 클릭이면 패널 링 해제 · 버튼은 누른 하나만.
        if let InputEvent::MouseDown { x, y, .. } = ie {
            let p = Point { x, y };
            let hit = [
                self.preview.bounds().contains(p),
                self.reset_btn.bounds().contains(p),
                self.close_btn.bounds().contains(p),
            ];
            self.preview.set_focused(hit[0]);
            self.reset_btn.set_focused(hit[1]);
            self.close_btn.set_focused(hit[2]);
            if hit.iter().any(|&h| h) {
                self.panel.set_focused(false);
            }
        }
        let _ = self.preview.take_clicked(); // 미리보기 = 눌러 보는 용도(동작 없음)
        if self.reset_btn.take_clicked() {
            if let Some((_, v)) = &mut self.key_mode {
                *v = None;
            } else {
                self.values = [None, None];
            }
            let v = self.value_of(th, self.target);
            self.panel.set_value(&v);
            self.redraw();
            return ColorsAction::Reset;
        }
        if self.close_btn.take_clicked() {
            self.close();
            return ColorsAction::None;
        }
        self.redraw();
        self.after_panel()
    }

    /// 패널 변경 수거 → 호스트에 보고(실시간 적용).
    fn after_panel(&mut self) -> ColorsAction {
        if let Some(hex) = self.panel.take_changed() {
            if let Some((_, v)) = &mut self.key_mode {
                *v = Some(hex.clone());
            } else {
                self.values[self.target.idx()] = Some(hex.clone());
            }
            return ColorsAction::Changed {
                target: self.target,
                hex,
            };
        }
        ColorsAction::None
    }

    /// 키 모드의 키(기본 모드 = None).
    pub(crate) fn key_mode_key(&self) -> Option<&'static str> {
        self.key_mode.as_ref().map(|(k, _)| *k)
    }

    /// 최근 색(호스트 영속화용).
    pub(crate) fn recent(&self) -> Vec<u32> {
        self.panel.recent().to_vec()
    }

    pub(crate) fn paint(&mut self, ui: &Font, th: &Theme, font_px: f32) {
        // surface 가변 대여 전에 값 문자열을 만들어 둔다.
        let targets = self.targets();
        let hexes: Vec<String> = targets.iter().map(|&tg| self.value_of(th, tg)).collect();
        let (Some(win), Some(surface)) = (self.window.clone(), self.surface.as_mut()) else {
            return;
        };
        let size = win.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            return;
        };
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let pad = (PAD * s).round() as i32;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs {
                base: SlotFont {
                    size: font_px,
                    bold: false,
                    italic: false,
                },
                ..FontPrefs::default()
            };
            let mut dc = RasterCtx::new(&mut gfx, ui, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.window_bg);
            dc.select_font(FontSlot::Base, false);
            // 왼쪽 목록 제목 + 대상 행(칩 + 라벨).
            dc.text(
                pad,
                pad,
                Rect::new(0, 0, wi, hi),
                t(Msg::LblColorTargets),
                th.text_dim,
            );
            let th_txt = dc.text_height();
            for (i, tg) in targets.iter().enumerate() {
                let r = self.list_rows[i];
                if *tg == self.target {
                    dc.fill_round_rect(r, (5.0 * s).round() as i32, th.sel_bg);
                }
                let chip = (18.0 * s).round() as i32;
                let cr = Rect::new(
                    r.x + (6.0 * s).round() as i32,
                    r.y + (r.h - chip) / 2,
                    chip,
                    chip,
                );
                if let Some(rgba) = rgba_from_hex(&hexes[i]) {
                    dc.fill_round_rect(cr, 3, th.panel_bg);
                    dc.fill_round_rect_alpha(
                        cr,
                        3,
                        nexa_ctl::theme::Color(rgba >> 8),
                        f32::from(rgba as u8) / 255.0,
                    );
                }
                dc.stroke_round_rect(cr, 3, th.border, 1.0);
                dc.text(
                    cr.right() + (8.0 * s).round() as i32,
                    r.y + (r.h - th_txt) / 2,
                    r,
                    t(tg.label()),
                    th.text,
                );
            }
            // 안내.
            let hint_y = self.list_rows[targets.len() - 1].bottom() + (10.0 * s).round() as i32;
            let hint_clip = Rect::new(pad, hint_y, (LIST_W * s).round() as i32, hi - hint_y);
            for (i, line) in t(Msg::ColorsHint).split('\n').enumerate() {
                dc.text(
                    pad,
                    hint_y + i as i32 * th_txt,
                    hint_clip,
                    line,
                    th.text_dim,
                );
            }
            // 색 패널 + 버튼.
            self.panel.paint(&mut dc, th);
            self.preview.paint(&mut dc, th);
            self.reset_btn.paint(&mut dc, th);
            self.close_btn.paint(&mut dc, th);
        }
        let _ = buf.present();
    }
}
