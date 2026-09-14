//! 로그 창(별도 창 · 사용자 09-14) — 실행 단계(전송·실행/최초 응답·페치·완료·오류·접속)를 시각 첫 컬럼으로 보여 준다.
//!
//! 데이터는 [`nsql_log::LogBuffer`](링 · 상한) · 표현은 [`nsql_log::LogFormat`] 어댑터(설정 `log.format` = raw|markdown|grid).
//! 창은 메인 창과 같은 방식(winit + softbuffer + nexa-ctl 래스터) — 이 앱의 두 번째 창. `Ctrl/⌘+⇧G`로 열고 닫는다.
//! 스크롤은 **픽셀 단위**, 스크롤바는 nexa-ctl `ScrollBars`(필요할 때만 · 호버 두껍게 · 자동 숨김).
//! 파일 I/O는 없다(후속 — 파일 싱크는 같은 `LogFormat`을 쓰고 배치 flush로 속도 이슈를 피한다).

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::Rect;
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{InputEvent, ScrollBars};
use nexa_gfx::{Font, Surface};
use nsql_log::{LogBuffer, LogEntry, LogFormat, LogKind};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 창이 호스트에 요청하는 것.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogWinAction {
    None,
    /// `RedrawRequested` — 호스트가 폰트·테마를 넘겨 [`LogWin::paint`]를 부른다.
    Paint,
}

pub(crate) struct LogWin {
    window: Option<Rc<Window>>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    buf: LogBuffer,
    fmt: Box<dyn LogFormat>,
    /// 세로 스크롤(픽셀).
    scroll_y: i32,
    /// 새 줄이 오면 끝으로 따라간다(위로 굴리면 해제 · 끝으로 돌아오면 다시).
    follow: bool,
    scale: f32,
    bars: ScrollBars,
    row_h: i32,
    header_h: i32,
    /// 마지막 페인트의 창 높이(스크롤 범위 계산).
    view_h: i32,
    cursor: (i32, i32),
    /// 설정 `grid.scroll` = row 이면 줄 경계에 맞춘다(기본 pixel).
    row_snap: bool,
}

impl LogWin {
    pub(crate) fn new(format: &str) -> Self {
        LogWin {
            window: None,
            ctx: None,
            surface: None,
            buf: LogBuffer::new(10_000),
            fmt: nsql_log::formatter(format),
            scroll_y: 0,
            follow: true,
            scale: 1.0,
            bars: ScrollBars::new(),
            row_h: 0,
            header_h: 0,
            view_h: 0,
            cursor: (0, 0),
            row_snap: false,
        }
    }

    pub(crate) fn set_row_snap(&mut self, on: bool) {
        self.row_snap = on;
        let y = self.scroll_y;
        self.set_scroll(y);
    }

    /// 설정 화면(T-39)에서 `log.format`을 바꾸면 호출 — 지금은 부팅 시 설정값만.
    #[allow(dead_code)]
    pub(crate) fn set_format(&mut self, format: &str) {
        self.fmt = nsql_log::formatter(format);
        self.redraw();
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn is_open(&self) -> bool {
        self.window.is_some()
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
    }

    /// 페이드 타이머 — 다시 그려야 하면 true.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.bars.tick(now_ms)
    }

    pub(crate) fn bars_visible(&self) -> bool {
        self.bars.is_visible()
    }

    /// 메인 창 오른쪽에 연다(`near` = 메인 창 바깥 좌표·폭). 이미 열려 있으면 앞으로.
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        near: Option<(i32, i32, u32)>,
        owner: Option<&Window>,
    ) {
        if let Some(w) = &self.window {
            w.focus_window();
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title("Nexa SQL — Log")
            .with_theme(theme)
            .with_inner_size(winit::dpi::LogicalSize::new(760.0, 320.0));
        if let Some((x, y, w)) = near {
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(x + w as i32 + 8, y));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        if let Ok(ctx) = softbuffer::Context::new(win.clone()) {
            if let Ok(s) = softbuffer::Surface::new(&ctx, win.clone()) {
                self.surface = Some(s);
            }
            self.ctx = Some(ctx);
        }
        self.window = Some(win);
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.ctx = None;
        self.window = None;
    }

    pub(crate) fn push(&mut self, e: LogEntry) {
        self.buf.push(e);
        if self.follow {
            self.scroll_y = self.max_scroll();
        }
        self.redraw();
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn content_h(&self) -> i32 {
        self.header_h + self.row_h * self.buf.len() as i32
    }

    fn max_scroll(&self) -> i32 {
        (self.content_h() - self.view_h).max(0)
    }

    fn set_scroll(&mut self, y: i32) {
        let max = self.max_scroll();
        let mut y = y.clamp(0, max);
        if self.row_snap && self.row_h > 0 && y < max {
            y -= y % self.row_h;
        }
        self.scroll_y = y;
        self.follow = self.scroll_y >= max;
        self.redraw();
    }

    fn viewport(&self) -> Rect {
        let (w, h) = self
            .window
            .as_ref()
            .map(|w| (w.inner_size().width as i32, w.inner_size().height as i32))
            .unwrap_or((0, 0));
        Rect::new(0, 0, w, h)
    }

    /// 스크롤바에 마우스/휠을 먼저 준다(픽셀 스크롤). 소비되면 true.
    fn bars_event(&mut self, ev: &InputEvent) -> bool {
        if self.row_h <= 0 {
            return false;
        }
        let vp = self.viewport();
        let ch = self.content_h();
        let (_, ny, consumed) =
            self.bars
                .on_event(ev, vp, vp.w, ch.max(vp.h), 0, self.scroll_y, self.scale);
        self.set_scroll(ny);
        consumed
    }

    /// 이 창의 이벤트. `Paint`면 호스트가 [`LogWin::paint`]를 부른다.
    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> LogWinAction {
        let row = self.row_h.max(1);
        match ev {
            WindowEvent::CloseRequested => self.close(),
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let px = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (*y * 120.0) as i32,
                    MouseScrollDelta::PixelDelta(p) => p.y as i32,
                };
                if !self.bars_event(&InputEvent::Wheel { delta: px }) {
                    let y = self.scroll_y - px / 3;
                    self.set_scroll(y);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                self.bars_event(&InputEvent::MouseMove { x, y });
            }
            WindowEvent::MouseInput { state, button, .. } if *button == MouseButton::Left => {
                let (x, y) = self.cursor;
                let ev = match state {
                    ElementState::Pressed => InputEvent::MouseDown {
                        x,
                        y,
                        shift: false,
                        primary: false,
                    },
                    ElementState::Released => InputEvent::MouseUp { x, y },
                };
                self.bars_event(&ev);
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                let page = (self.view_h - self.header_h).max(row);
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => self.close(),
                    Key::Named(NamedKey::End) => self.set_scroll(i32::MAX / 2),
                    Key::Named(NamedKey::Home) => self.set_scroll(0),
                    Key::Named(NamedKey::PageUp) => self.set_scroll(self.scroll_y - page),
                    Key::Named(NamedKey::PageDown) => self.set_scroll(self.scroll_y + page),
                    Key::Named(NamedKey::ArrowUp) => self.set_scroll(self.scroll_y - row),
                    Key::Named(NamedKey::ArrowDown) => self.set_scroll(self.scroll_y + row),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => return LogWinAction::Paint,
            _ => {}
        }
        LogWinAction::None
    }

    /// 그리기 — 헤더(포맷이 주면) + 픽셀 오프셋의 보이는 줄. 고정폭 폰트 · 종류별 색.
    pub(crate) fn paint(&mut self, mono: &Font, th: &Theme, font_px: f32) {
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
            let mut dc = RasterCtx::new(&mut gfx, mono, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.panel_bg);
            dc.select_font(FontSlot::Base, false);
            let pad = (6.0 * s).round() as i32;
            let row_h = dc.text_height() + (4.0 * s).round() as i32;
            self.row_h = row_h;
            // 헤더(고정 · 스크롤과 무관)
            let mut y = pad;
            let hdr = self.fmt.header();
            if let Some(hdr) = &hdr {
                for line in hdr.lines() {
                    dc.text(pad, y, Rect::new(0, 0, wi, hi), line, th.text_dim);
                    y += row_h;
                }
                dc.fill_rect(Rect::new(0, y - 1, wi, 1), th.border);
                y += 2;
            }
            self.header_h = y;
            // 푸터(위치 표시) 한 줄은 본문에서 뺀다.
            self.view_h = hi - row_h;
            // (surface 가변 대여 중이라 self 메서드 대신 필드로 계산)
            let content_h = self.header_h + row_h * self.buf.len() as i32;
            let max = (content_h - self.view_h).max(0);
            if self.follow {
                self.scroll_y = max;
            } else {
                self.scroll_y = self.scroll_y.clamp(0, max);
            }
            let body = Rect::new(0, self.header_h, wi, (self.view_h - self.header_h).max(0));
            let first = (self.scroll_y / row_h) as usize;
            let sub = self.scroll_y % row_h;
            let mut yy = body.y - sub;
            let mut last = first;
            for i in first..self.buf.len() {
                if yy >= body.bottom() {
                    break;
                }
                let Some(e) = self.buf.get(i) else { break };
                last = i + 1;
                let color = match e.kind {
                    LogKind::Error => th.danger,
                    LogKind::Done | LogKind::Connect => th.ok,
                    LogKind::Execute | LogKind::Fetch | LogKind::Output | LogKind::Commit => {
                        th.text
                    }
                    _ => th.text_dim,
                };
                dc.text(pad, yy, body, &self.fmt.line(e), color);
                yy += row_h;
            }
            // 푸터: "120–160 / 4,321 · raw"
            let info = format!(
                "{}–{} / {} · {}",
                if self.buf.is_empty() { 0 } else { first + 1 },
                last,
                self.buf.len(),
                self.fmt.name()
            );
            let iw = dc.text_width(&info);
            dc.fill_rect(Rect::new(0, self.view_h, wi, row_h), th.panel_bg);
            dc.text(
                wi - iw - pad,
                self.view_h,
                Rect::new(0, 0, wi, hi),
                &info,
                th.text_dim,
            );
            // 오버레이 스크롤바(필요할 때만 · 호버 두껍게)
            let vp = Rect::new(0, 0, wi, self.view_h);
            let ch = content_h;
            self.bars
                .paint(&mut dc, th, vp, wi, ch.max(vp.h), 0, self.scroll_y, s);
        }
        let _ = buf.present();
    }
}
