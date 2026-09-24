//! **메모리 맵 창**(docs/80 · 사용자 09-24): 모델리스 · 메인 소유 · 최상위 스위치(D-207). 그리는 것은 호스트가 준 `Sample` 하나 —
//! 총량 막대(데이터 카테고리 색 + 기타) · 데이터 표 · 시스템 표. 갱신 주기는 호스트(`about_to_wait` · `mem.refresh_ms`)가 맡고,
//! 이 창은 표본을 받아 그리기만 한다(닫혀 있으면 아무 비용도 없다).

use crate::memstat::{fmt, Cat, Sample};
use nexa_ctl::controls::{LabelSide, Switch};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{Color, FontPrefs, SlotFont, Theme};
use nexa_ctl::{InputEvent, Invalidations, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub(crate) enum MemWinAction {
    Paint,
    /// 최상위 스위치가 바뀌었다(설정 `mem.always_on_top`에 반영은 호스트).
    Toggled(bool),
    Close,
    None,
}

pub(crate) struct MemWin {
    window: Option<Rc<Window>>,
    memo: crate::wingeom::Memo,
    last: Option<((i32, i32), (f64, f64))>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    on_top: bool,
    top_switch: Switch,
    sample: Option<Sample>,
    /// 갱신 주기(ms · 바닥 안내 글).
    every_ms: u64,
}

impl MemWin {
    pub(crate) fn new() -> Self {
        MemWin {
            window: None,
            memo: crate::wingeom::Memo::default(),
            last: None,
            surface: None,
            scale: 1.0,
            cursor: (-1, -1),
            on_top: false,
            top_switch: Switch::new(t(Msg::LblLogSwTop), false).with_label_side(LabelSide::Right),
            sample: None,
            every_ms: 1000,
        }
    }

    /// 열기(모델리스 · 메인 소유 창) — 세션 창과 같은 규칙 + 최상위 옵션.
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
        let same = self.memo.on_same_monitor(owner);
        let (lw, lh) = same.and_then(|(_, s)| s).unwrap_or((620.0, 560.0));
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinMemory)))
            .with_theme(theme)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
        if let Some(((x, y), _)) = same {
            attrs = attrs.with_position(crate::wingeom::logical(x, y));
        } else if let Some((x, y, w)) = near {
            attrs =
                attrs.with_position(winit::dpi::PhysicalPosition::new(x + w as i32 + 8, y + 40));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        if let Some(((x, y), _)) = same {
            crate::wingeom::place_outer(&win, Some((x, y)));
        }
        crate::wingeom::keep_on_screen(&win, owner);
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        self.window = Some(win);
        self.apply_level();
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        if let Some(w) = &self.window {
            if let Some(p) = crate::wingeom::outer_pos(w) {
                self.last = Some((p, crate::wingeom::logical_size(w)));
            }
        }
        self.surface = None;
        self.window = None;
        // 표본은 창과 함께 버린다(닫힌 뒤 상주 0 · docs/80 §5).
        self.sample = None;
    }

    pub(crate) fn set_memo(&mut self, m: crate::wingeom::Memo) {
        self.memo = m;
    }

    pub(crate) fn take_last(&mut self) -> Option<((i32, i32), (f64, f64))> {
        self.last.take()
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

    /// 항상 위 켬/끔(설정 `mem.always_on_top`) — 열려 있으면 즉시 적용.
    pub(crate) fn set_on_top(&mut self, on: bool) {
        self.on_top = on;
        self.top_switch.set_on(on);
        self.apply_level();
    }

    fn apply_level(&self) {
        if let Some(w) = &self.window {
            w.set_window_level(if self.on_top {
                winit::window::WindowLevel::AlwaysOnTop
            } else {
                winit::window::WindowLevel::Normal
            });
        }
    }

    /// 새 표본(호스트가 `mem.refresh_ms`마다) — 그리기 요청까지.
    pub(crate) fn set_sample(&mut self, s: Sample, every_ms: u64) {
        self.sample = Some(s);
        self.every_ms = every_ms;
        self.redraw();
    }

    /// 창 표면(프레임버퍼) 바이트 — 호스트가 `Cat::Surfaces`에 더한다.
    pub(crate) fn surface_bytes(&self) -> u64 {
        self.window.as_ref().map_or(0, |w| {
            let s = w.inner_size();
            u64::from(s.width) * u64::from(s.height) * 4
        })
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> MemWinAction {
        match ev {
            WindowEvent::RedrawRequested => MemWinAction::Paint,
            WindowEvent::CloseRequested => MemWinAction::Close,
            WindowEvent::Resized(_) => {
                self.redraw();
                MemWinAction::None
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
                MemWinAction::None
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let mut inv = Invalidations::default();
                self.top_switch.on_event(
                    &InputEvent::MouseMove {
                        x: self.cursor.0,
                        y: self.cursor.1,
                    },
                    &mut inv,
                );
                if !inv.is_empty() {
                    self.redraw();
                }
                MemWinAction::None
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                let up = *state == ElementState::Released;
                let ev = if up {
                    InputEvent::MouseUp { x, y }
                } else {
                    InputEvent::MouseDown {
                        x,
                        y,
                        shift: false,
                        primary: false,
                    }
                };
                let mut inv = Invalidations::default();
                // 마우스 라우팅 규칙: 눌림은 커서 아래 컨트롤에만 · 뗌은 늘(눌린 상태를 풀게).
                if up || self.top_switch.bounds().contains(p) {
                    self.top_switch.on_event(&ev, &mut inv);
                }
                if let Some(on) = self.top_switch.take_toggled() {
                    self.on_top = on;
                    self.apply_level();
                    self.redraw();
                    return MemWinAction::Toggled(on);
                }
                if !inv.is_empty() {
                    self.redraw();
                }
                MemWinAction::None
            }
            WindowEvent::KeyboardInput { event: kev, .. }
                if kev.state == ElementState::Pressed
                    && matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) =>
            {
                MemWinAction::Close
            }
            _ => MemWinAction::None,
        }
    }

    pub(crate) fn paint(&mut self, font: &Font, th: &Theme, ui_px: f32) {
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
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let slot = |size: f32, bold: bool| SlotFont {
                size,
                bold,
                italic: false,
            };
            let prefs = FontPrefs {
                base: slot(ui_px, false),
                status: slot(ui_px, false),
                ..FontPrefs::default()
            };
            let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.panel_bg);
            let pad = px(12.0);
            let sample = self.sample;
            let (foot, sys) =
                sample.map_or((0, Default::default()), |sm| (sm.sys.footprint, sm.sys));
            let scale_max = foot.max(sys.resident).max(1);

            // ── 머리: 제목 + 최상위 스위치 ─────────────────────────────────────────────
            dc.select_font(FontSlot::Base, true);
            let th_txt = dc.text_height();
            let mut y = pad;
            let ty = dc.text_center_y(y, th_txt);
            dc.text(
                pad,
                ty,
                Rect::new(pad, y, wi - pad * 2, th_txt),
                t(Msg::WinMemory),
                th.text,
            );
            let sw_w = px(96.0);
            let sw_h = th_txt + px(4.0);
            let mut inv = Invalidations::default();
            self.top_switch.set_bounds(
                Rect::new(wi - pad - sw_w, y - px(2.0), sw_w, sw_h),
                &mut inv,
            );
            self.top_switch.paint(&mut dc, th);
            y += th_txt + px(10.0);

            // ── 총량 ───────────────────────────────────────────────────────────────
            dc.select_font(FontSlot::Base, true);
            let total_txt = fmt(foot);
            let ty = dc.text_center_y(y, th_txt);
            dc.text(pad, ty, Rect::new(pad, y, wi, th_txt), &total_txt, th.text);
            let tw = dc.text_width(&total_txt);
            dc.select_font(FontSlot::Base, false);
            let sub = tf(Msg::MemSubtitle, &[&fmt(sys.resident), &fmt(sys.heap_held)]);
            dc.text(
                pad + tw + px(10.0),
                ty,
                Rect::new(pad + tw + px(10.0), y, wi - pad * 2 - tw - px(10.0), th_txt),
                &sub,
                th.text_dim,
            );
            y += th_txt + px(12.0);

            // ── 총량 막대(데이터 카테고리 색 + 기타 회색) ────────────────────────────────
            let bar = Rect::new(pad, y, wi - pad * 2, px(18.0));
            dc.fill_rect(bar, th.field_bg);
            if let Some(sm) = sample {
                let mut x = bar.x;
                let mut acc_w = 0i64;
                let seg_w = |b: u64| -> i32 {
                    ((b as f64 / foot.max(1) as f64) * bar.w as f64).round() as i32
                };
                for c in Cat::ALL {
                    let b = sm.data.get(c);
                    if b == 0 {
                        continue;
                    }
                    let w = seg_w(b).min(bar.right() - x).max(0);
                    if w > 0 {
                        let (r, g, bl) = c.color();
                        dc.fill_rect(Rect::new(x, bar.y, w, bar.h), Color::from_rgb(r, g, bl));
                        x += w;
                        acc_w += w as i64;
                    }
                }
                let rest = bar.right() - x;
                if rest > 0 && acc_w >= 0 {
                    let (r, g, bl) = Cat::OTHER_COLOR;
                    dc.fill_rect(Rect::new(x, bar.y, rest, bar.h), Color::from_rgb(r, g, bl));
                }
            }
            dc.stroke_round_rect(bar, 0, th.border, 1.0);
            y += bar.h + px(14.0);

            // ── 표 두 개 ───────────────────────────────────────────────────────────
            let row_h = th_txt + px(6.0);
            let label_x = pad + px(18.0);
            let pct_w = px(54.0);
            let bytes_w = px(84.0);
            let bar_x0 = pad + px(270.0);
            let bar_x1 = wi - pad - bytes_w - pct_w - px(8.0);
            let row = |dc: &mut dyn DrawCtx,
                       y: i32,
                       chip: Option<(u8, u8, u8)>,
                       label: &str,
                       bytes: u64,
                       denom: u64| {
                if let Some((r, g, b)) = chip {
                    let c = px(10.0);
                    dc.fill_round_rect(
                        Rect::new(pad, y + (row_h - c) / 2, c, c),
                        px(2.0),
                        Color::from_rgb(r, g, b),
                    );
                }
                let ty = dc.text_center_y(y, row_h);
                dc.text(
                    label_x,
                    ty,
                    Rect::new(label_x, y, bar_x0 - label_x - px(6.0), row_h),
                    label,
                    th.text,
                );
                if bar_x1 > bar_x0 + px(20.0) {
                    let full = bar_x1 - bar_x0;
                    let track = Rect::new(bar_x0, y + row_h / 2 - px(3.0), full, px(6.0));
                    dc.fill_rect(track, th.field_bg);
                    let w = ((bytes as f64 / denom.max(1) as f64).min(1.0) * full as f64).round()
                        as i32;
                    if w > 0 {
                        let (r, g, b) = chip.unwrap_or(Cat::OTHER_COLOR);
                        dc.fill_rect(
                            Rect::new(track.x, track.y, w, track.h),
                            Color::from_rgb(r, g, b),
                        );
                    }
                }
                let bt = fmt(bytes);
                let bw = dc.text_width(&bt);
                let bx = wi - pad - pct_w - px(8.0) - bw;
                dc.text(bx, ty, Rect::new(bx, y, bw, row_h), &bt, th.text);
                let pct = if denom == 0 {
                    String::new()
                } else {
                    format!("{:.1}%", bytes as f64 * 100.0 / denom as f64)
                };
                let pw = dc.text_width(&pct);
                dc.text(
                    wi - pad - pw,
                    ty,
                    Rect::new(wi - pad - pw, y, pw, row_h),
                    &pct,
                    th.text_dim,
                );
            };
            let section = |dc: &mut dyn DrawCtx, y: i32, label: &str| {
                dc.select_font(FontSlot::Base, true);
                let ty = dc.text_center_y(y, row_h);
                dc.text(
                    pad,
                    ty,
                    Rect::new(pad, y, wi - pad * 2, row_h),
                    label,
                    th.text_dim,
                );
                dc.fill_rect(Rect::new(pad, y + row_h - 1, wi - pad * 2, 1), th.border);
                dc.select_font(FontSlot::Base, false);
            };

            section(&mut dc, y, t(Msg::MemGrpData));
            y += row_h + px(2.0);
            if let Some(sm) = sample {
                for c in Cat::ALL {
                    row(
                        &mut dc,
                        y,
                        Some(c.color()),
                        t(c.label()),
                        sm.data.get(c),
                        foot,
                    );
                    y += row_h;
                }
                row(
                    &mut dc,
                    y,
                    Some(Cat::OTHER_COLOR),
                    t(Msg::MemCatOther),
                    sm.other(),
                    foot,
                );
                y += row_h;
            }
            y += px(10.0);
            section(&mut dc, y, t(Msg::MemGrpSystem));
            y += row_h + px(2.0);
            for (label, v) in [
                (Msg::MemSysFootprint, sys.footprint),
                (Msg::MemSysResident, sys.resident),
                (Msg::MemSysAnon, sys.anon),
                (Msg::MemSysFile, sys.file_backed),
                (Msg::MemSysCompressed, sys.compressed),
                (Msg::MemHeapUsed, sys.heap_used),
                (Msg::MemHeapHeld, sys.heap_held),
            ] {
                row(&mut dc, y, None, t(label), v, scale_max);
                y += row_h;
            }

            // ── 바닥: 갱신 안내 ─────────────────────────────────────────────────────
            let foot_txt = match sample {
                Some(sm) => tf(
                    Msg::MemUpdated,
                    &[
                        &format!("{:.1}", sm.at.elapsed().as_secs_f32()),
                        &format!("{:.1}", self.every_ms as f32 / 1000.0),
                    ],
                ),
                None => t(Msg::StIntelLoading).to_string(),
            };
            let fy = hi - pad - th_txt;
            let ty = dc.text_center_y(fy, th_txt);
            dc.text(
                pad,
                ty,
                Rect::new(pad, fy, wi - pad * 2, th_txt),
                &foot_txt,
                th.text_dim,
            );
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
