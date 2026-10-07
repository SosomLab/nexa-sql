//! **메모리 맵 창**(docs/80 · 사용자 09-24): 모델리스 · 메인 소유 · 최상위 스위치(D-207). 그리는 것은 호스트가 준 `Sample` 하나 —
//! 총량 막대(데이터 카테고리 색 + 기타) · 데이터 표 · 시스템 표. 갱신 주기는 호스트(`about_to_wait` · `mem.refresh_ms`)가 맡고,
//! 이 창은 표본을 받아 그리기만 한다(닫혀 있으면 아무 비용도 없다).

use crate::memstat::{fmt, fmt_delta, Cat, Group, Sample, Trend};
use nexa_ctl::controls::{Button, LabelSide, Switch};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{Color, FontPrefs, Theme};
use nexa_ctl::{Control, InputEvent, Invalidations, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use std::collections::VecDeque;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub(crate) enum MemWinAction {
    Paint,
    /// 최상위 스위치가 바뀌었다(설정 `mem.always_on_top`에 반영은 호스트).
    Toggled(bool),
    /// "힙 정리" 버튼(호스트가 `memtrim::trim` + 즉시 표본).
    Trim,
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
    /// 힙 정리 버튼(80 §7 · T-197 잔여).
    trim_btn: Button,
    /// 바닥 [닫기](사용자 10-07 · Esc와 같음).
    close_btn: Button,
    /// 풋프린트 이력(최근 60 표본 · 스파크라인 · 480 B).
    hist: VecDeque<u64>,
    sample: Option<Sample>,
    /// 갱신 주기(ms · 바닥 안내 글).
    every_ms: u64,
    /// ★ 줄별 변화량(▲/▼ · 사용자 10-07 · nexa-dir3 이식).
    trend: Trend,
    /// [힙 정리] 뒤 버튼을 잠가 두는 남은 표본 수(0 = 평소).
    trim_hold: u8,
    /// 마지막 정리 결과 안내(바닥 줄 · 남은 표시 표본 수).
    trim_note: Option<(String, u8)>,
    /// 처음 열 때 한 번 — 내용 전체가 보이도록 창 높이를 맞춘다(사용자 10-07).
    fit: bool,
    /// 마지막으로 **값이 바뀐** 표본의 벽시계(바닥 글 "갱신 hh:mm:ss") — T-310: 안 바뀐 표본은 다시 그리지 않는다.
    updated_at: String,
    /// 마지막으로 그린 표본의 **표시 서명**(`Sample::display_sig` · 바이트 흔들림은 같은 서명).
    last_sig: Option<u64>,
}

/// [힙 정리] 뒤 버튼을 잠가 두는 표본 수(즉시 표본 1 + 다음 주기 1) · 결과 안내가 남는 표본 수.
const TRIM_HOLD: u8 = 2;
const TRIM_NOTE_HOLD: u8 = 8;

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
            trim_btn: Button::new(t(Msg::MemTrim)),
            close_btn: Button::new(t(Msg::BtnClose)),
            hist: VecDeque::with_capacity(60),
            sample: None,
            every_ms: 1000,
            trend: Trend::default(),
            trim_hold: 0,
            trim_note: None,
            fit: false,
            updated_at: String::new(),
            last_sig: None,
        }
    }

    /// [힙 정리]를 누른 직후 — 버튼을 잠그고 글을 "정리 중…"으로(호스트가 정리를 마치고 [`Self::set_trim_result`]를 부른다).
    fn begin_trim(&mut self) {
        self.trim_hold = TRIM_HOLD;
        self.trim_btn.set_enabled(false);
        self.trim_btn.set_label(t(Msg::MemTrimming));
        self.redraw();
    }

    /// 정리 결과(전 · 후 풋프린트 · 걸린 µs) → 바닥 안내 글(줄었으면 반환량 · 아니면 "반환할 것이 없음").
    pub(crate) fn set_trim_result(&mut self, before: u64, after: u64, us: u128) {
        let ms = format!("{:.1}", us as f64 / 1000.0);
        let text = if before > after {
            tf(Msg::MemTrimmed, &[&fmt(before - after), &ms])
        } else {
            tf(Msg::MemTrimmedNone, &[&ms])
        };
        self.trim_note = Some((text, TRIM_NOTE_HOLD));
        self.redraw();
    }

    #[cfg(test)]
    pub(crate) fn trim_note(&self) -> Option<&str> {
        self.trim_note.as_ref().map(|n| n.0.as_str())
    }

    #[cfg(test)]
    pub(crate) fn trimming(&self) -> bool {
        self.trim_hold > 0
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
        // 창 열기 꼬리 = 공통 호스트(T-247 · winhost).
        let Some(o) = crate::winhost::open_window(
            el,
            crate::winhost::OpenSpec {
                title: format!("Nexa SQL - {}", t(Msg::WinMemory)),
                theme,
                near,
                dy: 40,
                owner,
                memo: Some(&self.memo),
                // 표 전체(묶음 5 + 시스템 8줄)가 들어가는 높이 — 글꼴·배율이 다르면 첫 그리기에서 `fit`이 한 번 더 맞춘다.
                default_size: (640.0, 780.0),
                ime: false,
            },
        ) else {
            return;
        };
        self.scale = o.scale;
        self.surface = o.surface;
        self.window = Some(o.window);
        self.fit = true;
        self.apply_level();
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.last = self.window.as_deref().and_then(crate::winhost::last_geom);
        self.surface = None;
        self.window = None;
        // 표본·이력은 창과 함께 버린다(닫힌 뒤 상주 0 · docs/80 §5).
        self.sample = None;
        self.hist = VecDeque::new();
        self.last_sig = None;
        self.trend = Trend::default();
        self.trim_hold = 0;
        self.trim_note = None;
        self.trim_btn.set_enabled(true);
        self.trim_btn.set_label(t(Msg::MemTrim));
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
        if self.hist.len() >= 60 {
            self.hist.pop_front();
        }
        self.hist.push_back(s.sys.footprint);
        self.trend.update(&s);
        // 정리 뒤 잠금·결과 안내는 표본 수로 센다(갱신 주기에 맞춰 자연히 풀린다).
        if self.trim_hold > 0 {
            self.trim_hold -= 1;
            if self.trim_hold == 0 {
                self.trim_btn.set_enabled(true);
                self.trim_btn.set_label(t(Msg::MemTrim));
            }
        }
        if let Some((_, left)) = &mut self.trim_note {
            *left = left.saturating_sub(1);
            if *left == 0 {
                self.trim_note = None;
            }
        }
        // ★ T-310(사용자 10-07 "바뀐 줄만"): 값이 하나도 안 바뀐 표본은 **다시 그리지 않는다**(1초마다 전체 표를 그리던 비용 → 0) ·
        //   바뀌었거나 ▲/▼·정리 잠금·결과 안내가 진행 중일 때만 그린다. 바닥 글은 경과 초 대신 마지막 갱신 시각.
        //   비교 = 원값이 아니라 **화면에 보이는 글의 서명**(원값은 매초 바이트 단위로 흔들려 늘 "바뀜"이었다 · 협업 재측정 10-08).
        let sig = s.display_sig();
        let changed = self.last_sig != Some(sig);
        if changed {
            self.last_sig = Some(sig);
            self.updated_at = nsql_log::local_at(std::time::SystemTime::now()).time_only();
            self.updated_at.truncate(8);
        }
        self.sample = Some(s);
        self.every_ms = every_ms;
        if changed || self.trend.any_shown() || self.trim_hold > 0 || self.trim_note.is_some() {
            self.redraw();
        }
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
                let mv = InputEvent::MouseMove {
                    x: self.cursor.0,
                    y: self.cursor.1,
                };
                self.top_switch.on_event(&mv, &mut inv);
                self.trim_btn.on_event(&mv, &mut inv);
                self.close_btn.on_event(&mv, &mut inv);
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
                if up || self.trim_btn.bounds().contains(p) {
                    self.trim_btn.on_event(&ev, &mut inv);
                }
                if up || self.close_btn.bounds().contains(p) {
                    self.close_btn.on_event(&ev, &mut inv);
                }
                if self.close_btn.take_clicked() {
                    return MemWinAction::Close;
                }
                if self.trim_btn.take_clicked() {
                    self.begin_trim();
                    return MemWinAction::Trim;
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
        let Some(mut buf) = surface.frame(size) else {
            self.surface = Some(surface);
            return;
        };
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let px = |v: f32| (v * s).round() as i32;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs::with_base_status(ui_px);
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
            // 힙 정리 버튼(스위치 왼쪽) — 할당자가 들고 있는 빈 조각을 OS에 돌려준다(`memtrim`).
            let btn_w = px(96.0);
            let bx = wi - pad - sw_w - px(12.0) - btn_w;
            self.trim_btn
                .set_bounds(Rect::new(bx, y - px(2.0), btn_w, sw_h), &mut inv);
            self.trim_btn.paint(&mut dc, th);
            // 스파크라인(최근 60 표본 · 풋프린트 · 버튼 왼쪽) — 막대로(백엔드 무관).
            if self.hist.len() >= 2 {
                let sp_w = px(150.0);
                let area = Rect::new(bx - px(12.0) - sp_w, y - px(2.0), sp_w, sw_h);
                dc.fill_rect(area, th.field_bg);
                let max = self.hist.iter().copied().max().unwrap_or(1).max(1);
                let step = (area.w as f32 / 60.0).max(1.0);
                for (i, v) in self.hist.iter().enumerate() {
                    let h = ((*v as f64 / max as f64) * f64::from(area.h - 2)).round() as i32;
                    let x0 = area.x + (i as f32 * step).round() as i32;
                    let w = (step - 1.0).max(1.0) as i32;
                    dc.fill_rect(Rect::new(x0, area.bottom() - 1 - h, w, h.max(1)), th.accent);
                }
            }
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

            // ── 표: 묶음(소계) + 시스템 ─────────────────────────────────────────────
            let row_h = th_txt + px(6.0);
            let label_x = pad + px(18.0);
            let pct_w = dc.text_width("100.0%") + px(6.0);
            let bytes_w = dc.text_width("999.9 MB") + px(8.0);
            let delta_w = dc.text_width("▲ 999.9 MB") + px(8.0);
            let label_w = px(230.0).min((wi - pad * 2) / 2);
            let bar_x0 = label_x + label_w;
            let bar_x1 = wi - pad - pct_w - bytes_w - delta_w - px(6.0);
            let trend = self.trend;
            let row = |dc: &mut dyn DrawCtx,
                       y: i32,
                       chip: Option<(u8, u8, u8)>,
                       label: &str,
                       bytes: u64,
                       denom: u64,
                       delta: Option<i64>| {
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
                // 변화(▲ 늘었다 = 위험색 · ▼ 줄었다 = 강조색) — 바뀐 뒤 몇 표본 동안만.
                if let Some(d) = delta {
                    let dt = fmt_delta(d);
                    let dw = dc.text_width(&dt);
                    let dx = wi - pad - pct_w - bytes_w - dw - px(4.0);
                    let col = if d >= 0 { th.danger } else { th.accent };
                    dc.text(dx, ty, Rect::new(dx, y, dw, row_h), &dt, col);
                }
                let bt = fmt(bytes);
                let bw = dc.text_width(&bt);
                let bx = wi - pad - pct_w - bw;
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
            // 구획 머리: 묶음 이름(굵게) + 소계(오른쪽 · 바이트 열 자리) + 밑줄.
            let section = |dc: &mut dyn DrawCtx, y: i32, label: &str, sum: Option<u64>| {
                dc.select_font(FontSlot::Base, true);
                let ty = dc.text_center_y(y, row_h);
                dc.text(
                    pad,
                    ty,
                    Rect::new(pad, y, wi - pad * 2, row_h),
                    label,
                    th.text_dim,
                );
                dc.select_font(FontSlot::Base, false);
                if let Some(v) = sum {
                    let st = fmt(v);
                    let sw = dc.text_width(&st);
                    let sx = wi - pad - pct_w - sw;
                    dc.text(sx, ty, Rect::new(sx, y, sw, row_h), &st, th.text_dim);
                }
                dc.fill_rect(Rect::new(pad, y + row_h - 1, wi - pad * 2, 1), th.border);
            };

            if let Some(sm) = sample {
                for g in Group::ALL {
                    let other = sm.other();
                    let sum = if g == Group::Runtime {
                        other
                    } else {
                        sm.data.group_sum(g)
                    };
                    section(&mut dc, y, t(g.label()), Some(sum));
                    y += row_h + px(2.0);
                    for c in Cat::ALL.into_iter().filter(|c| c.group() == g) {
                        row(
                            &mut dc,
                            y,
                            Some(c.color()),
                            t(c.label()),
                            sm.data.get(c),
                            foot,
                            trend.shown(c.idx()),
                        );
                        y += row_h;
                    }
                    if g == Group::Runtime {
                        row(
                            &mut dc,
                            y,
                            Some(Cat::OTHER_COLOR),
                            t(Msg::MemCatOther),
                            other,
                            foot,
                            trend.shown(Trend::OTHER),
                        );
                        y += row_h;
                    }
                    y += px(6.0);
                }
            } else {
                for g in Group::ALL {
                    section(&mut dc, y, t(g.label()), None);
                    y += row_h + px(2.0);
                    y += row_h * Cat::ALL.iter().filter(|c| c.group() == g).count() as i32
                        + if g == Group::Runtime { row_h } else { 0 }
                        + px(6.0);
                }
            }
            y += px(4.0);
            section(&mut dc, y, t(Msg::MemGrpSystem), None);
            y += row_h + px(2.0);
            // (라벨, 값, 변화 칸 · None = 안 보임, 0이어도 보임)
            for (label, v, tk, always) in [
                (Msg::MemSysFootprint, sys.footprint, Some(0), true),
                (Msg::MemSysPrivateWs, sys.private_ws, Some(4), false),
                (Msg::MemSysResident, sys.resident, Some(1), true),
                (Msg::MemSysAnon, sys.anon, None, true),
                (Msg::MemSysFile, sys.file_backed, None, true),
                (Msg::MemSysCompressed, sys.compressed, None, true),
                (Msg::MemHeapUsed, sys.heap_used, Some(2), true),
                (Msg::MemHeapHeld, sys.heap_held, Some(3), true),
            ] {
                if !always && v == 0 {
                    continue;
                }
                row(
                    &mut dc,
                    y,
                    None,
                    t(label),
                    v,
                    scale_max,
                    tk.and_then(|k| trend.shown_sys(k)),
                );
                y += row_h;
            }

            // 처음 열 때 한 번: 내용 끝 + 바닥 줄이 들어가도록 창 높이를 맞춘다(사용자 10-07 "열릴 때 전체 내용이 보이게").
            let btn_h = th_txt + px(12.0);
            if self.fit {
                self.fit = false;
                let need = y + px(10.0) + btn_h + pad;
                if need > hi {
                    let _ = win
                        .request_inner_size(winit::dpi::PhysicalSize::new(size.width, need as u32));
                }
            }

            // ── 바닥: 정리 결과(방금 눌렀으면 · 몇 표본 뒤 평소 안내로) 또는 갱신 안내 ───────────
            let foot_txt = match (&self.trim_note, sample) {
                (Some((note, _)), _) => note.clone(),
                (None, Some(_)) => tf(
                    Msg::MemUpdated,
                    &[
                        &self.updated_at,
                        &format!("{:.1}", self.every_ms as f32 / 1000.0),
                    ],
                ),
                (None, None) => t(Msg::StIntelLoading).to_string(),
            };
            // 바닥 줄 = 안내 글(왼쪽) + [닫기](오른쪽 · 사용자 10-07).
            let by = hi - pad - btn_h;
            self.close_btn.set_scale(s);
            let bw = dc.text_width(t(Msg::BtnClose)) + px(28.0);
            self.close_btn
                .set_bounds(Rect::new(wi - pad - bw, by, bw, btn_h), &mut inv);
            self.close_btn.paint(&mut dc, th);
            let ty = dc.text_center_y(by, btn_h);
            dc.text(
                pad,
                ty,
                Rect::new(pad, by, (wi - pad * 2 - bw - px(8.0)).max(1), btn_h),
                &foot_txt,
                th.text_dim,
            );
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 정리 결과 글: 줄었으면 반환량 · 아니면 "없음" · 표본 TRIM_NOTE_HOLD번 뒤 사라짐 · 버튼 잠금은 TRIM_HOLD 표본.
    #[test]
    fn trim_note_and_hold_follow_samples() {
        let mut w = MemWin::new();
        assert!(!w.trimming());
        w.begin_trim();
        assert!(w.trimming());
        w.set_trim_result(10 * 1024 * 1024, 7 * 1024 * 1024, 1500);
        let n = w.trim_note().unwrap_or_default().to_string();
        assert!(n.contains("3.00 MB") && n.contains("1.5"), "{n}");
        w.set_trim_result(5, 5, 200);
        assert!(w.trim_note().unwrap_or_default().contains("0.2"));
        let s = crate::memstat::sample(&[], |_| {});
        for i in 0..TRIM_NOTE_HOLD {
            assert_eq!(w.trimming(), i < TRIM_HOLD);
            assert!(w.trim_note().is_some());
            w.set_sample(s, 1000);
        }
        assert!(w.trim_note().is_none());
        assert!(!w.trimming());
    }
}
