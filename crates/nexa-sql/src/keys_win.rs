//! 단축키 설정 창(사용자 09-15 · T-53) — 명령 목록(라벨 · 현재 단축키 · 충돌 배지) + **캡처**(지정 → 다음 키 조합).
//!
//! 메인 창의 소유 창. 값은 호스트가 [`KeysAction::Changed`]를 받아 설정 `key.<id>`에 쓰고 키맵을 다시 만든다(이 창은 고르기만).
//! - [지정] = 캡처 모드(행이 "키를 누르세요…" · Esc = 취소) · [비우기] = 단축키 없음(`none`) · [초기화] = 전부 플랫폼 기본.
//! - 충돌 = 같은 조합을 쓰는 다른 명령 이름을 행 끝에 경고색으로.

use crate::keymap::{self, Chord, Keymap, COMMANDS};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 창이 호스트에 요청하는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeysAction {
    None,
    Paint,
    /// 명령 `id`의 단축키를 `code`로(설정 문법 · `none` = 없음).
    Changed {
        id: String,
        code: String,
    },
    /// 전부 플랫폼 기본으로.
    ResetAll,
}

const PAD: f32 = 12.0;
const ROW_H: f32 = 26.0;
const HEAD_H: f32 = 24.0;
const BTN_H: f32 = 28.0;
const LABEL_W: f32 = 230.0;
const CHORD_W: f32 = 170.0;

/// 표시 행(호스트가 키맵으로 채운다).
#[derive(Clone, Debug)]
struct RowView {
    id: &'static str,
    label: String,
    chord: String,
    conflict: Option<String>,
}

pub(crate) struct KeysWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    alt: bool,
    /// macOS Control 눌림(⌘와 별개 · 키맵 `ctrl+cmd+…`).
    ctrl_mac: bool,
    rows: Vec<RowView>,
    selected: Option<usize>,
    /// 캡처 중(선택 행에 다음 키 조합을 넣는다).
    capturing: bool,
    scroll: i32,
    list: Rect,
    assign_btn: Button,
    clear_btn: Button,
    reset_btn: Button,
    close_btn: Button,
}

impl KeysWin {
    pub(crate) fn new() -> Self {
        KeysWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            alt: false,
            ctrl_mac: false,
            rows: Vec::new(),
            selected: None,
            capturing: false,
            scroll: 0,
            list: Rect::default(),
            assign_btn: Button::new(t(Msg::BtnAssign)),
            clear_btn: Button::new(t(Msg::BtnClear)),
            reset_btn: Button::new(t(Msg::BtnReset)),
            close_btn: Button::new(t(Msg::BtnClose)),
        }
    }

    /// 키맵으로 표시 행을 다시 만든다(열 때 · 값이 바뀔 때).
    pub(crate) fn refresh(&mut self, km: &Keymap) {
        self.rows = COMMANDS
            .iter()
            .map(|c| {
                let code = km.code_of(c.id);
                let conflict = code
                    .split('|')
                    .filter_map(Chord::parse)
                    .find_map(|ch| km.conflict(&ch, c.id))
                    .map(keymap::label_of);
                RowView {
                    id: c.id,
                    label: t(c.label).to_string(),
                    chord: km.display_of(c.id),
                    conflict,
                }
            })
            .collect();
        self.sync_enabled();
    }

    fn sync_enabled(&mut self) {
        let on = self.selected.is_some() && !self.capturing;
        self.assign_btn.set_enabled(on);
        self.clear_btn.set_enabled(on);
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

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if self.window.is_none() {
            return false;
        }
        let mut any = false;
        for b in self.buttons() {
            any |= b.tick(now_ms);
        }
        any
    }

    pub(crate) fn animating(&self) -> bool {
        self.window.is_some()
            && [
                &self.assign_btn,
                &self.clear_btn,
                &self.reset_btn,
                &self.close_btn,
            ]
            .iter()
            .any(|b| b.is_animating())
    }

    fn buttons(&mut self) -> [&mut Button; 4] {
        [
            &mut self.assign_btn,
            &mut self.clear_btn,
            &mut self.reset_btn,
            &mut self.close_btn,
        ]
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
        let (lw, lh) = (560.0, 620.0);
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinKeys)))
            .with_theme(theme)
            .with_resizable(true)
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
        self.capturing = false;
        self.sync_enabled();
        self.layout();
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
        self.capturing = false;
    }

    fn layout(&mut self) {
        let Some(win) = &self.window else { return };
        let size = win.inner_size();
        let (w, h) = (size.width as i32, size.height as i32);
        let s = self.scale;
        let pad = self.s(PAD);
        let mut inv = Invalidations::default();
        let hint_h = self.s(40.0);
        let top = pad + hint_h + self.s(HEAD_H);
        let by = h - pad - self.s(BTN_H);
        self.list = Rect::new(pad, top, w - pad * 2, (by - pad) - top);
        let bw = self.s(96.0);
        let gap = self.s(8.0);
        let bh = self.s(BTN_H);
        let mut x = pad;
        for b in [&mut self.assign_btn, &mut self.clear_btn] {
            b.set_scale(s);
            b.set_bounds(Rect::new(x, by, bw, bh), &mut inv);
            x += bw + gap;
        }
        self.close_btn.set_scale(s);
        self.close_btn
            .set_bounds(Rect::new(w - pad - bw, by, bw, bh), &mut inv);
        self.reset_btn.set_scale(s);
        self.reset_btn
            .set_bounds(Rect::new(w - pad - bw * 2 - gap, by, bw, bh), &mut inv);
        self.clamp_scroll();
    }

    fn row_h(&self) -> i32 {
        self.s(ROW_H)
    }

    fn content_h(&self) -> i32 {
        self.rows.len() as i32 * self.row_h()
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h() - self.list.h).max(0);
        self.scroll = self.scroll.clamp(0, max);
    }

    fn row_at(&self, p: Point) -> Option<usize> {
        if !self.list.contains(p) {
            return None;
        }
        let i = (p.y - self.list.y + self.scroll) / self.row_h();
        (i >= 0 && (i as usize) < self.rows.len()).then_some(i as usize)
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> KeysAction {
        match ev {
            WindowEvent::CloseRequested => {
                self.close();
                return KeysAction::None;
            }
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
                return KeysAction::None;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
                self.redraw();
                return KeysAction::None;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
                // macOS Control(⌘와 별개 · Sublime `ctrl+cmd+g`) — 다른 OS에선 늘 false.
                self.ctrl_mac = cfg!(target_os = "macos") && m.state().control_key();
                self.alt = m.state().alt_key();
                return KeysAction::None;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                let mut inv = Invalidations::default();
                for b in self.buttons() {
                    b.on_event(&mv, &mut inv);
                }
                self.redraw();
                return KeysAction::None;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y * ROW_H * 3.0 * self.scale,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                self.scroll -= dy.round() as i32;
                self.clamp_scroll();
                self.redraw();
                return KeysAction::None;
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                let is_esc = matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape));
                if self.capturing {
                    if is_esc {
                        self.capturing = false;
                        self.sync_enabled();
                        self.redraw();
                        return KeysAction::None;
                    }
                    // 조합키만 누른 경우는 None → 계속 기다린다.
                    let Some(ch) = Chord::from_winit(
                        &kev.logical_key,
                        &kev.physical_key,
                        self.primary,
                        self.shift,
                        self.alt,
                        self.ctrl_mac,
                    ) else {
                        return KeysAction::None;
                    };
                    self.capturing = false;
                    self.sync_enabled();
                    if let Some(i) = self.selected {
                        let id = self.rows[i].id.to_string();
                        return KeysAction::Changed {
                            id,
                            code: ch.code(),
                        };
                    }
                    return KeysAction::None;
                }
                if is_esc {
                    self.close();
                    return KeysAction::None;
                }
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::ArrowDown) => self.move_sel(1),
                    Key::Named(NamedKey::ArrowUp) => self.move_sel(-1),
                    Key::Named(NamedKey::Enter) if self.selected.is_some() => {
                        self.capturing = true;
                        self.sync_enabled();
                    }
                    _ => {}
                }
                self.redraw();
                return KeysAction::None;
            }
            WindowEvent::RedrawRequested => return KeysAction::Paint,
            _ => {}
        }
        let (x, y) = self.cursor;
        let ie = match ev {
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => InputEvent::MouseDown {
                    x,
                    y,
                    shift: self.shift,
                    primary: false,
                },
                (ElementState::Released, MouseButton::Left) => InputEvent::MouseUp { x, y },
                _ => return KeysAction::None,
            },
            _ => return KeysAction::None,
        };
        let mut inv = Invalidations::default();
        if let InputEvent::MouseDown { x, y, .. } = ie {
            let p = Point { x, y };
            if let Some(i) = self.row_at(p) {
                self.capturing = false;
                self.selected = Some(i);
                self.sync_enabled();
                for b in self.buttons() {
                    b.set_focused(false);
                }
                self.redraw();
                return KeysAction::None;
            }
        }
        for b in self.buttons() {
            b.on_event(&ie, &mut inv);
        }
        // 포커스 규칙(CLAUDE.md §3): 누른 버튼 하나만 링.
        if let InputEvent::MouseDown { x, y, .. } = ie {
            let p = Point { x, y };
            for b in self.buttons() {
                let hit = b.bounds().contains(p);
                b.set_focused(hit);
            }
        }
        if self.assign_btn.take_clicked() {
            if self.selected.is_some() {
                self.capturing = true;
                self.sync_enabled();
            }
            self.redraw();
            return KeysAction::None;
        }
        if self.clear_btn.take_clicked() {
            self.redraw();
            if let Some(i) = self.selected {
                return KeysAction::Changed {
                    id: self.rows[i].id.to_string(),
                    code: "none".into(),
                };
            }
            return KeysAction::None;
        }
        if self.reset_btn.take_clicked() {
            self.capturing = false;
            self.sync_enabled();
            self.redraw();
            return KeysAction::ResetAll;
        }
        if self.close_btn.take_clicked() {
            self.close();
            return KeysAction::None;
        }
        self.redraw();
        KeysAction::None
    }

    fn move_sel(&mut self, d: i32) {
        if self.rows.is_empty() {
            return;
        }
        let n = self.rows.len() as i32;
        let cur = self.selected.map_or(-1, |i| i as i32);
        let next = (cur + d).clamp(0, n - 1);
        self.selected = Some(next as usize);
        self.sync_enabled();
        // 보이게 스크롤.
        let top = next * self.row_h();
        if top < self.scroll {
            self.scroll = top;
        } else if top + self.row_h() > self.scroll + self.list.h {
            self.scroll = top + self.row_h() - self.list.h;
        }
        self.clamp_scroll();
    }

    pub(crate) fn paint(&mut self, ui: &Font, th: &Theme, font_px: f32) {
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
        let list = self.list;
        let row_h = (ROW_H * s).round() as i32;
        let label_w = (LABEL_W * s).round() as i32;
        let chord_w = (CHORD_W * s).round() as i32;
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
            let th_txt = dc.text_height();
            // 안내 2줄.
            for (i, line) in t(Msg::KeysHint).split('\n').enumerate() {
                dc.text(
                    pad,
                    pad + i as i32 * th_txt,
                    Rect::new(0, 0, wi, hi),
                    line,
                    th.text_dim,
                );
            }
            // 머리글.
            let hy = list.y - (HEAD_H * s).round() as i32;
            let head = Rect::new(list.x, hy, list.w, (HEAD_H * s).round() as i32);
            dc.fill_rect(head, th.panel_bg);
            let ty = hy + (head.h - th_txt) / 2;
            dc.text(
                list.x + pad / 2,
                ty,
                head,
                t(Msg::LblKeyCommand),
                th.text_dim,
            );
            dc.text(
                list.x + label_w,
                ty,
                head,
                t(Msg::LblKeyShortcut),
                th.text_dim,
            );
            dc.stroke_round_rect(
                Rect::new(list.x, hy, list.w, list.h + head.h),
                0,
                th.border,
                1.0,
            );
            // 행.
            let first = (self.scroll / row_h).max(0) as usize;
            for (i, r) in self.rows.iter().enumerate().skip(first) {
                let y = list.y + i as i32 * row_h - self.scroll;
                if y >= list.bottom() {
                    break;
                }
                let rr = Rect::new(list.x, y, list.w, row_h);
                let clip = rr.intersection(&list);
                if clip.h <= 0 {
                    continue;
                }
                let sel = self.selected == Some(i);
                if sel {
                    dc.fill_rect(clip, th.sel_bg);
                } else if i % 2 == 1 {
                    dc.fill_rect(clip, th.panel_bg);
                }
                let ty = y + (row_h - th_txt) / 2;
                dc.text(list.x + pad / 2, ty, clip, &r.label, th.text);
                let (chord, color) = if sel && self.capturing {
                    (t(Msg::KeysCapturing).to_string(), th.accent)
                } else {
                    (r.chord.clone(), th.text)
                };
                dc.text(list.x + label_w, ty, clip, &chord, color);
                if let Some(c) = &r.conflict {
                    dc.text(
                        list.x + label_w + chord_w,
                        ty,
                        clip,
                        &tf(Msg::KeysConflict, &[c]),
                        th.warn,
                    );
                }
            }
            for b in [
                &mut self.assign_btn,
                &mut self.clear_btn,
                &mut self.reset_btn,
                &mut self.close_btn,
            ] {
                b.paint(&mut dc, th);
            }
        }
        let _ = buf.present();
    }
}
