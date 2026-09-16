//! 로그 창(별도 창 · 사용자 09-14) — 실행 단계(전송·실행/최초 응답·페치·완료·오류·접속)를 시각 첫 컬럼으로 보여 준다.
//!
//! 데이터는 [`nsql_log::LogBuffer`](링 · 상한) · 표현은 [`nsql_log::LogFormat`] 어댑터(설정 `log.format` = raw|markdown|grid).
//! 창은 메인 창과 같은 방식(winit + softbuffer + nexa-ctl 래스터) — 이 앱의 두 번째 창. `Ctrl/⌘+⇧G`로 열고 닫는다.
//! 스크롤은 **픽셀 단위 · 세로/가로**(휠 · Shift+휠/가로 휠 · 스크롤바 드래그 · 키보드), 스크롤바는 nexa-ctl `ScrollBars`
//! (필요할 때만 · 호버 두껍게 · 자동 숨김). **줄바꿈 스위치**(설정 `log.wrap` · 사용자 09-16)를 켜면 창 폭에 접고 가로 스크롤은 없다.
//! 파일 I/O는 없다(후속 — 파일 싱크는 같은 `LogFormat`을 쓰고 배치 flush로 속도 이슈를 피한다).

use nexa_ctl::controls::{LabelSide, Switch};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{Control, InputEvent, Invalidations, ScrollBars, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use nsql_log::{LogBuffer, LogEntry, LogFormat, LogKind};
use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 창이 호스트에 요청하는 것.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogWinAction {
    None,
    /// `RedrawRequested` — 호스트가 폰트·테마를 넘겨 [`LogWin::paint`]를 부른다.
    Paint,
    /// 푸터 스위치를 눌렀다(설정 키 · 값 — 호스트가 설정에 기억).
    Toggled(&'static str, bool),
}

/// 항목 하나의 배치(형식 문자열 · 폭 · 줄바꿈 위치) — 페인트에서 지연 계산 · 링 버퍼와 나란히.
struct LineMeta {
    text: String,
    width: i32,
    /// 이어지는 행이 시작하는 문자 index(첫 행은 0부터 · 비면 한 행).
    breaks: Vec<usize>,
}

pub(crate) struct LogWin {
    window: Option<Rc<Window>>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    buf: LogBuffer,
    fmt: Box<dyn LogFormat>,
    /// 항목마다 배치(버퍼와 같은 순서 · `None` = 아직 계산 전).
    meta: VecDeque<Option<LineMeta>>,
    /// 줄바꿈 켬일 때 항목별 누적 행 시작(len+1) — 스크롤 위치 ↔ 항목 변환.
    row_start: Vec<u32>,
    /// 배치를 계산한 조건(글꼴 px 비트 · 줄바꿈 · 접는 폭) — 바뀌면 전부 다시.
    layout_key: (u32, bool, i32),
    /// 세로/가로 스크롤(픽셀).
    scroll_y: i32,
    scroll_x: i32,
    scale: f32,
    bars: ScrollBars,
    row_h: i32,
    header_h: i32,
    /// 마지막 페인트의 창 폭·본문 높이(스크롤 범위 계산).
    view_w: i32,
    view_h: i32,
    /// 가장 넓은 줄(px · 여백 포함) — 가로 스크롤 범위.
    content_w: i32,
    cursor: (i32, i32),
    shift: bool,
    /// 설정 `grid.scroll` = row 이면 줄 경계에 맞춘다(기본 pixel).
    row_snap: bool,
    /// 줄바꿈(설정 `log.wrap` · 푸터 스위치).
    wrap: bool,
    wrap_switch: Switch,
    /// 최신 먼저(설정 `log.newest_first`) — 표시 순서만 뒤집는다(버퍼는 그대로).
    newest_first: bool,
    newest_switch: Switch,
    /// 자동 스크롤(설정 `log.autoscroll`) — 새 줄이 오면 최신 줄로.
    autoscroll: bool,
    auto_switch: Switch,
    /// 다음 페인트에서 최신 줄로 이동(push가 켠다).
    jump: bool,
    /// 항상 위(설정 `log.always_on_top` · 스위치) — 소유 창이라 메인 창보다 늘 위.
    on_top: bool,
    top_switch: Switch,
}

impl LogWin {
    pub(crate) fn new(format: &str) -> Self {
        LogWin {
            window: None,
            ctx: None,
            surface: None,
            buf: LogBuffer::new(10_000),
            fmt: nsql_log::formatter(format),
            meta: VecDeque::new(),
            row_start: Vec::new(),
            layout_key: (0, false, 0),
            scroll_y: 0,
            scroll_x: 0,
            scale: 1.0,
            bars: ScrollBars::new(),
            row_h: 0,
            header_h: 0,
            view_w: 0,
            view_h: 0,
            content_w: 0,
            cursor: (0, 0),
            shift: false,
            row_snap: false,
            wrap: false,
            wrap_switch: Switch::new(t(Msg::LblLogWrap), false).with_label_side(LabelSide::Left),
            newest_first: false,
            newest_switch: Switch::new(t(Msg::LblLogNewestFirst), false)
                .with_label_side(LabelSide::Left),
            autoscroll: true,
            auto_switch: Switch::new(t(Msg::LblLogAutoscroll), true)
                .with_label_side(LabelSide::Left),
            jump: true,
            on_top: false,
            top_switch: Switch::new(t(Msg::LblLogOnTop), false).with_label_side(LabelSide::Left),
        }
    }

    /// 스크롤 단위 — `true` = 줄 경계에 맞춤.
    pub(crate) fn set_row_snap(&mut self, on: bool) {
        self.row_snap = on;
        let y = self.scroll_y;
        self.set_scroll(y);
    }

    /// 형식 어댑터 교체(설정 `log.format` 즉시 반영) — 배치는 다시 계산.
    pub(crate) fn set_format(&mut self, format: &str) {
        self.fmt = nsql_log::formatter(format);
        self.invalidate_layout();
        self.redraw();
    }

    /// 줄바꿈 켬/끔(설정 `log.wrap` · 스위치).
    pub(crate) fn set_wrap(&mut self, on: bool) {
        self.wrap = on;
        self.wrap_switch.set_on(on);
        self.scroll_x = 0;
        self.invalidate_layout();
        self.redraw();
    }

    /// 최신 먼저 켬/끔(설정 `log.newest_first` · 스위치) — 누적 행만 다시(배치는 그대로).
    pub(crate) fn set_newest_first(&mut self, on: bool) {
        self.newest_first = on;
        self.newest_switch.set_on(on);
        self.row_start.clear();
        self.jump = self.autoscroll;
        self.redraw();
    }

    /// 자동 스크롤 켬/끔(설정 `log.autoscroll` · 스위치).
    pub(crate) fn set_autoscroll(&mut self, on: bool) {
        self.autoscroll = on;
        self.auto_switch.set_on(on);
        self.jump = on;
        self.redraw();
    }

    /// 항상 위 켬/끔(설정 `log.always_on_top` · 스위치) — 열려 있으면 즉시 적용.
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

    /// 표시 순서 → 버퍼 index.
    fn disp(&self, i: usize) -> usize {
        if self.newest_first {
            self.buf.len().saturating_sub(1 + i)
        } else {
            i
        }
    }

    fn invalidate_layout(&mut self) {
        for m in &mut self.meta {
            *m = None;
        }
        self.row_start.clear();
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

    /// 스크롤바 페이드 틱 — 다시 그릴 것이 있으면 true.
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
        self.apply_level();
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.ctx = None;
        self.window = None;
    }

    pub(crate) fn push(&mut self, e: LogEntry) {
        let before = self.buf.len();
        self.buf.push(e);
        self.meta.push_back(None);
        if self.buf.len() == before {
            // 링이 앞을 버렸다 — 배치도 같이.
            self.meta.pop_front();
            self.row_start.clear();
        }
        if self.autoscroll {
            self.jump = true; // 페인트에서 최신 줄로(행 수는 배치 뒤에 안다).
        }
        self.redraw();
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// 전체 행 수(줄바꿈이면 접힌 행 합 · 아니면 항목 수).
    fn total_rows(&self) -> i32 {
        if self.wrap {
            self.row_start.last().copied().unwrap_or(0) as i32
        } else {
            self.buf.len() as i32
        }
    }

    fn content_h(&self) -> i32 {
        self.header_h + self.row_h * self.total_rows()
    }

    fn max_scroll(&self) -> i32 {
        (self.content_h() - self.view_h).max(0)
    }

    fn max_scroll_x(&self) -> i32 {
        if self.wrap {
            0
        } else {
            (self.content_w - self.view_w).max(0)
        }
    }

    fn set_scroll(&mut self, y: i32) {
        let max = self.max_scroll();
        let mut y = y.clamp(0, max);
        if self.row_snap && self.row_h > 0 && y < max {
            y -= y % self.row_h;
        }
        self.scroll_y = y;
        self.redraw();
    }

    fn set_scroll_x(&mut self, x: i32) {
        self.scroll_x = x.clamp(0, self.max_scroll_x());
        self.redraw();
    }

    fn viewport(&self) -> Rect {
        let (w, h) = self
            .window
            .as_ref()
            .map(|w| (w.inner_size().width as i32, w.inner_size().height as i32))
            .unwrap_or((0, 0));
        // 본문(푸터 제외) — 마지막 페인트가 잰 높이.
        Rect::new(0, 0, w, if self.view_h > 0 { self.view_h } else { h })
    }

    /// 스크롤바에 마우스/휠을 먼저 준다(픽셀 스크롤 · 세로·가로). 소비되면 true.
    fn bars_event(&mut self, ev: &InputEvent) -> bool {
        if self.row_h <= 0 {
            return false;
        }
        let vp = self.viewport();
        let ch = self.content_h();
        let cw = self.content_w;
        let (nx, ny, consumed) = self.bars.on_event(
            ev,
            vp,
            cw.max(vp.w),
            ch.max(vp.h),
            self.scroll_x,
            self.scroll_y,
            self.scale,
        );
        self.set_scroll(ny);
        self.set_scroll_x(nx);
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
            WindowEvent::ModifiersChanged(m) => self.shift = m.state().shift_key(),
            WindowEvent::MouseWheel { delta, .. } => {
                // 세로 휠 · Shift+휠/가로 휠 = 가로(방향 반전·macOS 부호는 공용 변환이 처리).
                let ev = crate::input::wheel_event(delta, self.shift);
                if !self.bars_event(&ev) {
                    match ev {
                        InputEvent::Wheel { delta: px } => {
                            let y = self.scroll_y - px / 3;
                            self.set_scroll(y);
                        }
                        InputEvent::HWheel { delta: px } => {
                            let x = self.scroll_x + px / 3;
                            self.set_scroll_x(x);
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                if !self.bars_event(&InputEvent::MouseMove { x, y }) {
                    let mut inv = Invalidations::default();
                    let mv = InputEvent::MouseMove { x, y };
                    self.wrap_switch.on_event(&mv, &mut inv);
                    self.newest_switch.on_event(&mv, &mut inv);
                    self.auto_switch.on_event(&mv, &mut inv);
                    self.top_switch.on_event(&mv, &mut inv);
                    if !inv.is_empty() {
                        self.redraw();
                    }
                }
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
                if !self.bars_event(&ev) {
                    // 스위치는 커서 아래일 때만(마우스 라우팅 규칙) · 놓기는 늘 전달(눌림 해제).
                    let p = Point { x, y };
                    let up = matches!(ev, InputEvent::MouseUp { .. });
                    let mut inv = Invalidations::default();
                    if up || self.wrap_switch.bounds().contains(p) {
                        self.wrap_switch.on_event(&ev, &mut inv);
                    }
                    if up || self.newest_switch.bounds().contains(p) {
                        self.newest_switch.on_event(&ev, &mut inv);
                    }
                    if up || self.auto_switch.bounds().contains(p) {
                        self.auto_switch.on_event(&ev, &mut inv);
                    }
                    if up || self.top_switch.bounds().contains(p) {
                        self.top_switch.on_event(&ev, &mut inv);
                    }
                    if !inv.is_empty() {
                        self.redraw();
                    }
                    if let Some(on) = self.wrap_switch.take_toggled() {
                        self.set_wrap(on);
                        return LogWinAction::Toggled("log.wrap", on);
                    }
                    if let Some(on) = self.newest_switch.take_toggled() {
                        self.set_newest_first(on);
                        return LogWinAction::Toggled("log.newest_first", on);
                    }
                    if let Some(on) = self.auto_switch.take_toggled() {
                        self.set_autoscroll(on);
                        return LogWinAction::Toggled("log.autoscroll", on);
                    }
                    if let Some(on) = self.top_switch.take_toggled() {
                        self.set_on_top(on);
                        return LogWinAction::Toggled("log.always_on_top", on);
                    }
                }
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                let page = (self.view_h - self.header_h).max(row);
                let step_x = row * 2;
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => self.close(),
                    Key::Named(NamedKey::End) => self.set_scroll(i32::MAX / 2),
                    Key::Named(NamedKey::Home) => self.set_scroll(0),
                    Key::Named(NamedKey::PageUp) => self.set_scroll(self.scroll_y - page),
                    Key::Named(NamedKey::PageDown) => self.set_scroll(self.scroll_y + page),
                    Key::Named(NamedKey::ArrowUp) => self.set_scroll(self.scroll_y - row),
                    Key::Named(NamedKey::ArrowDown) => self.set_scroll(self.scroll_y + row),
                    Key::Named(NamedKey::ArrowLeft) => self.set_scroll_x(self.scroll_x - step_x),
                    Key::Named(NamedKey::ArrowRight) => self.set_scroll_x(self.scroll_x + step_x),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => return LogWinAction::Paint,
            _ => {}
        }
        LogWinAction::None
    }

    /// 배치 계산 — 조건(글꼴·줄바꿈·폭)이 바뀌었으면 전부, 아니면 새 항목만. 줄바꿈이면 누적 행도 갱신.
    fn ensure_layout(&mut self, dc: &mut dyn DrawCtx, font_px: f32, avail_w: i32) {
        let key = (
            font_px.to_bits(),
            self.wrap,
            if self.wrap { avail_w } else { 0 },
        );
        if key != self.layout_key {
            self.layout_key = key;
            self.invalidate_layout();
        }
        let indent = dc.text_width("    ");
        let n = self.buf.len();
        while self.meta.len() < n {
            self.meta.push_back(None);
        }
        while self.meta.len() > n {
            self.meta.pop_front();
        }
        let mut changed = false;
        for i in 0..n {
            if self.meta[i].is_some() {
                continue;
            }
            let Some(e) = self.buf.get(i) else { break };
            let text = self.fmt.line(e);
            let width = dc.text_width(&text);
            let mut breaks = Vec::new();
            if self.wrap && width > avail_w && avail_w > indent + 20 {
                let chars: Vec<char> = text.chars().collect();
                let mut wpx = Vec::new();
                dc.text_prefix_widths(&text, &mut wpx);
                let mut start = 0usize;
                let mut limit = avail_w;
                while start < chars.len() {
                    let base = wpx[start];
                    let mut end = start + 1;
                    while end < chars.len() && wpx[end + 1] - base <= limit {
                        end += 1;
                    }
                    if end >= chars.len() {
                        break;
                    }
                    // 공백 경계 선호(줄머리 절반 이후에 공백이 있으면 거기서).
                    let mut brk = end;
                    if let Some(sp) = chars[start..end].iter().rposition(|c| *c == ' ') {
                        if sp > (end - start) / 2 {
                            brk = start + sp + 1;
                        }
                    }
                    breaks.push(brk);
                    start = brk;
                    limit = avail_w - indent;
                }
            }
            self.meta[i] = Some(LineMeta {
                text,
                width,
                breaks,
            });
            changed = true;
        }
        if self.wrap && (changed || self.row_start.len() != n + 1) {
            self.row_start.clear();
            self.row_start.reserve(n + 1);
            let mut acc = 0u32;
            self.row_start.push(0);
            for di in 0..n {
                let bi = self.disp(di);
                acc += self.meta[bi]
                    .as_ref()
                    .map_or(1, |m| m.breaks.len() as u32 + 1);
                self.row_start.push(acc);
            }
        }
        if !self.wrap {
            self.content_w = self
                .meta
                .iter()
                .filter_map(|m| m.as_ref().map(|m| m.width))
                .max()
                .unwrap_or(0);
        } else {
            self.content_w = 0;
        }
    }

    /// 그리기 — 헤더(포맷이 주면) + 픽셀 오프셋의 보이는 줄(줄바꿈이면 접힌 행). 고정폭 폰트 · 종류별 색.
    pub(crate) fn paint(&mut self, mono: &Font, th: &Theme, font_px: f32) {
        // 표면을 잠시 꺼내 둔다 — 그리는 동안 배치 계산(`&mut self`)을 해야 한다.
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
            self.view_w = wi;
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
            // 푸터(스위치 · 위치 표시) 한 줄은 본문에서 뺀다.
            self.view_h = hi - row_h - pad;
            let avail_w = wi - pad * 2;
            self.ensure_layout(&mut dc, font_px, avail_w);
            // 가로 폭에 여백을 더해 마지막 글자가 잘리지 않게.
            if !self.wrap {
                self.content_w += pad * 2;
            }
            let content_h = self.content_h();
            let max = (content_h - self.view_h).max(0);
            if std::mem::take(&mut self.jump) {
                self.scroll_y = if self.newest_first { 0 } else { max };
            }
            self.scroll_y = self.scroll_y.clamp(0, max);
            self.scroll_x = self.scroll_x.clamp(0, self.max_scroll_x());
            let body = Rect::new(0, self.header_h, wi, (self.view_h - self.header_h).max(0));
            let first_row = (self.scroll_y / row_h) as usize;
            let sub = self.scroll_y % row_h;
            let indent = dc.text_width("    ");
            // 첫 항목과 그 안의 행 오프셋.
            let (first, mut row_in) = if self.wrap {
                let i = match self.row_start.binary_search(&(first_row as u32)) {
                    Ok(i) => i.min(self.buf.len().saturating_sub(1)),
                    Err(i) => i.saturating_sub(1),
                };
                (
                    i,
                    first_row.saturating_sub(self.row_start.get(i).copied().unwrap_or(0) as usize),
                )
            } else {
                (first_row, 0usize)
            };
            let mut yy = body.y - sub;
            let mut last = first;
            let x0 = pad - self.scroll_x;
            'outer: for i in first..self.buf.len() {
                if yy >= body.bottom() {
                    break;
                }
                let bi = self.disp(i);
                let Some(e) = self.buf.get(bi) else { break };
                last = i + 1;
                let color = match e.kind {
                    LogKind::Error => th.danger,
                    LogKind::Done | LogKind::Connect => th.ok,
                    LogKind::Execute | LogKind::Fetch | LogKind::Output | LogKind::Commit => {
                        th.text
                    }
                    _ => th.text_dim,
                };
                let Some(m) = self.meta.get(bi).and_then(|m| m.as_ref()) else {
                    break;
                };
                if m.breaks.is_empty() {
                    if row_in == 0 {
                        dc.text(x0, yy, body, &m.text, color);
                        yy += row_h;
                    }
                    row_in = 0;
                    continue;
                }
                let chars: Vec<char> = m.text.chars().collect();
                let mut starts = vec![0usize];
                starts.extend(m.breaks.iter().copied());
                for (r, &st) in starts.iter().enumerate() {
                    if r < row_in {
                        continue;
                    }
                    if yy >= body.bottom() {
                        break 'outer;
                    }
                    let en = starts.get(r + 1).copied().unwrap_or(chars.len());
                    let seg: String = chars[st..en].iter().collect();
                    let x = if r == 0 { pad } else { pad + indent };
                    dc.text(x, yy, body, &seg, color);
                    yy += row_h;
                }
                row_in = 0;
            }
            // 푸터: [줄바꿈 스위치]                "120–160 / 4,321 · raw"
            let fy = self.view_h;
            dc.fill_rect(Rect::new(0, fy, wi, hi - fy), th.panel_bg);
            dc.fill_rect(Rect::new(0, fy, wi, 1), th.border);
            let mut inv = Invalidations::default();
            let mut x = pad;
            for (sw, msg) in [
                (&mut self.wrap_switch, Msg::LblLogWrap),
                (&mut self.newest_switch, Msg::LblLogNewestFirst),
                (&mut self.auto_switch, Msg::LblLogAutoscroll),
                (&mut self.top_switch, Msg::LblLogOnTop),
            ] {
                sw.set_scale(s);
                // 라벨 폭 + 트랙(44) + 여백 — Switch는 폭을 스스로 재지 않는다.
                let w = dc.text_width(t(msg)) + (60.0 * s).round() as i32;
                sw.set_bounds(Rect::new(x, fy + 2, w, row_h + pad - 2), &mut inv);
                sw.paint(&mut dc, th);
                x += w + pad;
            }
            let info = format!(
                "{}–{} / {} · {}",
                if self.buf.is_empty() { 0 } else { first + 1 },
                last,
                self.buf.len(),
                self.fmt.name()
            );
            let iw = dc.text_width(&info);
            dc.text(
                wi - iw - pad,
                fy + pad / 2,
                Rect::new(0, fy, wi, hi - fy),
                &info,
                th.text_dim,
            );
            // 오버레이 스크롤바(필요할 때만 · 호버 두껍게 · 세로 + 가로)
            let vp = Rect::new(0, 0, wi, self.view_h);
            self.bars.paint(
                &mut dc,
                th,
                vp,
                self.content_w.max(vp.w),
                content_h.max(vp.h),
                self.scroll_x,
                self.scroll_y,
                s,
            );
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
