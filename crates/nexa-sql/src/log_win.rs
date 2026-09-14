//! 로그 창(별도 창 · 사용자 09-14) — 실행 단계(전송·실행/최초 응답·페치·완료·오류·접속)를 시각 첫 컬럼으로 보여 준다.
//!
//! 데이터는 [`nsql_log::LogBuffer`](링 · 상한) · 표현은 [`nsql_log::LogFormat`] 어댑터(설정 `log.format` = raw|markdown|grid).
//! 창은 메인 창과 같은 방식(winit + softbuffer + nexa-ctl 래스터) — 이 앱의 두 번째 창. `Ctrl/⌘+⇧G`로 열고 닫는다.
//! 파일 I/O는 없다(후속 — 파일 싱크는 같은 `LogFormat`을 쓰고 배치 flush로 속도 이슈를 피한다).

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::Rect;
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_gfx::{Font, Surface};
use nsql_log::{LogBuffer, LogEntry, LogFormat, LogKind};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
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
    /// 첫 표시 줄(엔트리 인덱스).
    scroll: usize,
    /// 새 줄이 오면 끝으로 따라간다(사용자가 위로 굴리면 해제 · 끝으로 돌아오면 다시).
    follow: bool,
    scale: f32,
    rows_visible: usize,
}

impl LogWin {
    pub(crate) fn new(format: &str) -> Self {
        LogWin {
            window: None,
            ctx: None,
            surface: None,
            buf: LogBuffer::new(10_000),
            fmt: nsql_log::formatter(format),
            scroll: 0,
            follow: true,
            scale: 1.0,
            rows_visible: 1,
        }
    }

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

    /// 메인 창 오른쪽에 연다(`near` = 메인 창 바깥 좌표·폭). 이미 열려 있으면 앞으로.
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        near: Option<(i32, i32, u32)>,
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
            self.scroll = self.buf.len().saturating_sub(self.rows_visible);
        }
        self.redraw();
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn scroll_by(&mut self, delta_rows: i32) {
        let max = self.buf.len().saturating_sub(self.rows_visible);
        let next = (self.scroll as i64 + i64::from(delta_rows)).clamp(0, max as i64) as usize;
        self.scroll = next;
        self.follow = next >= max;
        self.redraw();
    }

    /// 이 창의 이벤트. `Paint`면 호스트가 [`LogWin::paint`]를 부른다.
    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> LogWinAction {
        match ev {
            WindowEvent::CloseRequested => self.close(),
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 20.0) as f32,
                };
                self.scroll_by(if dy > 0.0 { -3 } else { 3 });
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => self.close(),
                    Key::Named(NamedKey::End) => {
                        self.follow = true;
                        self.scroll = self.buf.len().saturating_sub(self.rows_visible);
                        self.redraw();
                    }
                    Key::Named(NamedKey::Home) => self.scroll_by(i32::MIN / 2),
                    Key::Named(NamedKey::PageUp) => {
                        self.scroll_by(-(self.rows_visible as i32).max(1))
                    }
                    Key::Named(NamedKey::PageDown) => {
                        self.scroll_by((self.rows_visible as i32).max(1))
                    }
                    Key::Named(NamedKey::ArrowUp) => self.scroll_by(-1),
                    Key::Named(NamedKey::ArrowDown) => self.scroll_by(1),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => return LogWinAction::Paint,
            _ => {}
        }
        LogWinAction::None
    }

    /// 그리기 — 헤더(포맷이 주면) + 보이는 줄. 고정폭 폰트 · 종류별 색.
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
            let mut y = pad;
            if let Some(hdr) = self.fmt.header() {
                for line in hdr.lines() {
                    dc.text(pad, y, Rect::new(0, 0, wi, hi), line, th.text_dim);
                    y += row_h;
                }
                dc.fill_rect(Rect::new(0, y - 1, wi, 1), th.border);
                y += 2;
            }
            self.rows_visible = (((hi - y - pad) / row_h).max(1)) as usize;
            if self.follow {
                self.scroll = self.buf.len().saturating_sub(self.rows_visible);
            }
            let clip = Rect::new(0, 0, wi, hi);
            for i in self.scroll..self.buf.len() {
                if y + row_h > hi {
                    break;
                }
                let Some(e) = self.buf.get(i) else { break };
                let color = match e.kind {
                    LogKind::Error => th.danger,
                    LogKind::Done | LogKind::Connect => th.ok,
                    LogKind::Execute | LogKind::Fetch | LogKind::Output | LogKind::Commit => {
                        th.text
                    }
                    _ => th.text_dim,
                };
                dc.text(pad, y, clip, &self.fmt.line(e), color);
                y += row_h;
            }
            // 위치 표시(우하단): "120–160 / 4,321 · raw"
            let info = format!(
                "{}–{} / {} · {}",
                if self.buf.is_empty() {
                    0
                } else {
                    self.scroll + 1
                },
                (self.scroll + self.rows_visible).min(self.buf.len()),
                self.buf.len(),
                self.fmt.name()
            );
            let iw = dc.text_width(&info);
            dc.text(wi - iw - pad, hi - row_h, clip, &info, th.text_dim);
        }
        let _ = buf.present();
    }
}
