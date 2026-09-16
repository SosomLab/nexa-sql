//! 로그 창(별도 창 · 사용자 09-14) — 실행 단계(전송·실행/최초 응답·페치·완료·오류·접속)를 시각 첫 컬럼으로 보여 준다.
//!
//! 데이터는 [`nsql_log::LogBuffer`](링 · 상한) · 표현은 [`nsql_log::LogFormat`] 어댑터(설정 `log.format` = raw|markdown|grid).
//! 창은 메인 창과 같은 방식(winit + softbuffer + nexa-ctl 래스터) — 이 앱의 두 번째 창. `Ctrl/⌘+⇧G`로 열고 닫는다.
//! 스크롤은 **픽셀 단위 · 세로/가로**(휠 · Shift+휠/가로 휠 · 스크롤바 드래그 · 키보드), 스크롤바는 nexa-ctl `ScrollBars`
//! (필요할 때만 · 호버 두껍게 · 자동 숨김). **줄바꿈 스위치**(설정 `log.wrap` · 사용자 09-16)를 켜면 창 폭에 접고 가로 스크롤은 없다.
//! 파일 I/O는 없다(후속 — 파일 싱크는 같은 `LogFormat`을 쓰고 배치 flush로 속도 이슈를 피한다).

use nexa_ctl::controls::ctxmenu::{ContextMenu, CtxItem};
use nexa_ctl::controls::{LabelSide, Switch};
use nexa_ctl::draw::{draw_tooltip, DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{Control, InputEvent, Invalidations, ScrollBars, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use nsql_log::{Columns, LogBuffer, LogEntry, LogFormat, LogKind};
use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 창이 호스트에 요청하는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LogWinAction {
    None,
    /// `RedrawRequested` — 호스트가 폰트·테마를 넘겨 [`LogWin::paint`]를 부른다.
    Paint,
    /// 푸터 스위치를 눌렀다(설정 키 · 값 — 호스트가 설정에 기억).
    Toggled(&'static str, bool),
    /// 메뉴로 바꾼 문자열 설정(`log.kinds` · `log.columns`).
    Setting(&'static str, String),
    /// 메뉴 ▸ 로그를 파일로 저장(호스트가 파일 대화상자).
    SaveAs,
    /// 메뉴 ▸ 보이는 줄 복사(클립보드는 호스트 몫).
    CopyText(String),
}

/// 항목 하나의 배치(형식 문자열 · 폭 · 줄바꿈 위치) — 페인트에서 지연 계산 · 링 버퍼와 나란히.
struct LineMeta {
    text: String,
    width: i32,
    /// 이어지는 행이 시작하는 문자 index(첫 행은 0부터 · 비면 한 행).
    breaks: Vec<usize>,
}

/// 스위치 툴팁까지 머무는 시간(ms).
const TIP_MS: u128 = 600;

pub(crate) struct LogWin {
    window: Option<Rc<Window>>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    buf: LogBuffer,
    fmt: Box<dyn LogFormat + Send>,
    /// 형식 이름 · 템플릿 · 컬럼(설정 `log.format`/`log.template`/`log.columns`).
    fmt_name: String,
    template: String,
    cols: Columns,
    /// 보이는 종류(설정 `log.kinds` · 비면 전부).
    kinds: Vec<LogKind>,
    /// 필터를 통과한 버퍼 index(표시 순서의 원천).
    vis: Vec<usize>,
    /// 우클릭 메뉴(종류·컬럼 토글 · 저장 · 복사 · 지우기).
    menu: ContextMenu,
    /// 푸터 스위치 위 hover(index · 시작 시각) — 머물면 툴팁.
    sw_hover: Option<(usize, Instant)>,
    /// 툴팁을 이미 그렸다(다음 tick에 다시 깨우지 않게).
    tip_shown: bool,
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
            fmt: nsql_log::formatter_with(format, nsql_log::DEFAULT_TEMPLATE, Columns::default()),
            fmt_name: format.to_string(),
            template: nsql_log::DEFAULT_TEMPLATE.to_string(),
            cols: Columns::default(),
            kinds: Vec::new(),
            vis: Vec::new(),
            menu: ContextMenu::new(),
            sw_hover: None,
            tip_shown: false,
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
            wrap_switch: Switch::new(t(Msg::LblLogSwWrap), false).with_label_side(LabelSide::Right),
            newest_first: false,
            newest_switch: Switch::new(t(Msg::LblLogSwSort), false)
                .with_label_side(LabelSide::Right),
            autoscroll: true,
            auto_switch: Switch::new(t(Msg::LblLogSwScroll), true)
                .with_label_side(LabelSide::Right),
            jump: true,
            on_top: false,
            top_switch: Switch::new(t(Msg::LblLogSwTop), false).with_label_side(LabelSide::Right),
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
        self.fmt_name = format.to_string();
        self.rebuild_fmt();
    }

    /// 템플릿(설정 `log.template` · 형식이 template일 때).
    pub(crate) fn set_template(&mut self, tpl: &str) {
        self.template = tpl.to_string();
        self.rebuild_fmt();
    }

    /// 보이는 컬럼(설정 `log.columns`).
    pub(crate) fn set_columns(&mut self, cols: &str) {
        self.cols = Columns::parse(cols);
        self.rebuild_fmt();
    }

    /// 보이는 종류(설정 `log.kinds` · 쉼표 · 비면 전부).
    pub(crate) fn set_kinds(&mut self, kinds: &str) {
        self.kinds = kinds.split(',').filter_map(LogKind::parse).collect();
        self.rebuild_vis();
        self.row_start.clear();
        self.jump = self.autoscroll;
        self.redraw();
    }

    fn rebuild_fmt(&mut self) {
        self.fmt = nsql_log::formatter_with(&self.fmt_name, &self.template, self.cols);
        self.invalidate_layout();
        self.redraw();
    }

    fn shown(&self, k: LogKind) -> bool {
        self.kinds.is_empty() || self.kinds.contains(&k)
    }

    fn rebuild_vis(&mut self) {
        self.vis = (0..self.buf.len())
            .filter(|&i| self.buf.get(i).is_some_and(|e| self.shown(e.kind)))
            .collect();
    }

    /// 보이는 줄 전부를 현재 형식으로(헤더 포함 · 저장/복사).
    pub(crate) fn export_text(&self) -> String {
        let mut out = String::new();
        if let Some(h) = self.fmt.header() {
            out.push_str(&h);
            out.push('\n');
        }
        for i in 0..self.vis.len() {
            let bi = self.disp(i);
            if let Some(e) = self.buf.get(bi) {
                out.push_str(&self.fmt.line(e));
                out.push('\n');
            }
        }
        out
    }

    /// 보이는 줄 수.
    pub(crate) fn visible_len(&self) -> usize {
        self.vis.len()
    }

    fn open_menu(&mut self, x: i32, y: i32) {
        self.menu.set_scale(self.scale);
        let kinds: Vec<CtxItem> = LogKind::ALL
            .iter()
            .map(|k| {
                CtxItem::item(format!("kind:{}", k.label()), k.label()).with_checked(self.shown(*k))
            })
            .collect();
        let cols: Vec<CtxItem> = Columns::NAMES
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let on = match i {
                    0 => self.cols.ts,
                    1 => self.cols.kind,
                    2 => self.cols.rows,
                    _ => self.cols.elapsed,
                };
                CtxItem::item(format!("col:{n}"), *n).with_checked(on)
            })
            .collect();
        let items = vec![
            CtxItem::submenu("kinds", t(Msg::MnLogKinds), kinds),
            CtxItem::submenu("cols", t(Msg::MnLogColumns), cols),
            CtxItem::Separator,
            CtxItem::item("save", t(Msg::MnLogSaveAs)),
            CtxItem::item("copy", t(Msg::MnLogCopyAll)),
            CtxItem::Separator,
            CtxItem::item("clear", t(Msg::MnLogClear)),
        ];
        let host = self.viewport();
        self.menu
            .open_at(x, y, items, host, (self.row_h * 10).max(160));
        self.redraw();
    }

    /// 메뉴 항목 → 동작(설정에 남길 것은 액션으로 돌려준다).
    fn menu_pick(&mut self, id: &str) -> LogWinAction {
        if let Some(label) = id.strip_prefix("kind:") {
            let Some(k) = LogKind::parse(label) else {
                return LogWinAction::None;
            };
            let mut set: Vec<LogKind> = if self.kinds.is_empty() {
                LogKind::ALL.to_vec()
            } else {
                self.kinds.clone()
            };
            if let Some(i) = set.iter().position(|x| *x == k) {
                set.remove(i);
            } else {
                set.push(k);
            }
            let text = if set.len() == LogKind::ALL.len() {
                String::new()
            } else {
                set.iter().map(|k| k.label()).collect::<Vec<_>>().join(",")
            };
            self.set_kinds(&text);
            return LogWinAction::Setting("log.kinds", text);
        }
        if let Some(name) = id.strip_prefix("col:") {
            let mut c = self.cols;
            c.toggle(name);
            let text = c.to_setting();
            self.set_columns(&text);
            return LogWinAction::Setting("log.columns", text);
        }
        match id {
            "save" => LogWinAction::SaveAs,
            "copy" => LogWinAction::CopyText(self.export_text()),
            "clear" => {
                self.buf.clear();
                self.meta.clear();
                self.vis.clear();
                self.row_start.clear();
                self.scroll_y = 0;
                self.scroll_x = 0;
                self.redraw();
                LogWinAction::None
            }
            _ => LogWinAction::None,
        }
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

    /// 표시 순서 → 버퍼 index(필터 통과 목록 위에서 · 최신 먼저면 뒤집음).
    fn disp(&self, i: usize) -> usize {
        let n = self.vis.len();
        let vi = if self.newest_first {
            n.saturating_sub(1 + i)
        } else {
            i
        };
        self.vis.get(vi).copied().unwrap_or(0)
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
        self.bars.tick(now_ms) || self.tooltip_due()
    }

    /// 스위치 위에 머문 지 600ms가 됐고 아직 안 그렸다.
    fn tooltip_due(&self) -> bool {
        !self.tip_shown
            && self
                .sw_hover
                .is_some_and(|(_, since)| since.elapsed().as_millis() >= TIP_MS)
    }

    /// 호스트의 타이머 유지 조건 — 툴팁을 기다리는 중.
    pub(crate) fn tooltip_pending(&self) -> bool {
        self.sw_hover.is_some() && !self.tip_shown
    }

    fn switch_at(&self, p: Point) -> Option<usize> {
        [
            &self.wrap_switch,
            &self.newest_switch,
            &self.auto_switch,
            &self.top_switch,
        ]
        .iter()
        .position(|sw| sw.bounds().contains(p))
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
            // 링이 앞을 버렸다 — 배치·필터 목록도 같이.
            self.meta.pop_front();
            self.row_start.clear();
            self.rebuild_vis();
        } else if self
            .buf
            .get(self.buf.len() - 1)
            .is_some_and(|e| self.shown(e.kind))
        {
            self.vis.push(self.buf.len() - 1);
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
            self.vis.len() as i32
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
                if self.menu.is_open() {
                    if self.menu.on_event(&InputEvent::MouseMove { x, y }) {
                        self.redraw();
                    }
                } else if !self.bars_event(&InputEvent::MouseMove { x, y }) {
                    let over = self.switch_at(Point { x, y });
                    match (over, self.sw_hover) {
                        (Some(i), Some((j, _))) if i == j => {}
                        (Some(i), _) => {
                            self.sw_hover = Some((i, Instant::now()));
                            if std::mem::take(&mut self.tip_shown) {
                                self.redraw();
                            }
                        }
                        (None, Some(_)) => {
                            self.sw_hover = None;
                            if std::mem::take(&mut self.tip_shown) {
                                self.redraw();
                            }
                        }
                        (None, None) => {}
                    }
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
            WindowEvent::MouseInput { state, button, .. } if *button == MouseButton::Right => {
                if *state == ElementState::Pressed {
                    let (x, y) = self.cursor;
                    if self.menu.is_open() {
                        self.menu.on_event(&InputEvent::RightDown { x, y });
                    }
                    if !self.menu.is_open() {
                        self.open_menu(x, y);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if *button == MouseButton::Left && self.menu.is_open() =>
            {
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
                self.menu.on_event(&ev);
                self.redraw();
                if let Some(id) = self.menu.take_picked() {
                    return self.menu_pick(&id);
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
                if self.menu.is_open() {
                    self.menu.close();
                    self.redraw();
                    return LogWinAction::None;
                }
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
        let nv = self.vis.len();
        if self.wrap && (changed || self.row_start.len() != nv + 1) {
            self.row_start.clear();
            self.row_start.reserve(nv + 1);
            let mut acc = 0u32;
            self.row_start.push(0);
            for di in 0..nv {
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
                    Ok(i) => i.min(self.vis.len().saturating_sub(1)),
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
            'outer: for i in first..self.vis.len() {
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
            let sep_gap = pad * 2;
            for (i, (sw, msg)) in [
                (&mut self.wrap_switch, Msg::LblLogSwWrap),
                (&mut self.newest_switch, Msg::LblLogSwSort),
                (&mut self.auto_switch, Msg::LblLogSwScroll),
                (&mut self.top_switch, Msg::LblLogSwTop),
            ]
            .into_iter()
            .enumerate()
            {
                if i > 0 {
                    // 구분선(사용자 09-16: 설정 사이를 눈으로 가르게).
                    dc.fill_rect(
                        Rect::new(x + sep_gap / 2, fy + 4, 1, hi - fy - 8),
                        th.border,
                    );
                    x += sep_gap;
                }
                sw.set_scale(s);
                // 트랙(44) + 라벨 폭 + 여백 — Switch는 폭을 스스로 재지 않는다.
                let w = dc.text_width(t(msg)) + (56.0 * s).round() as i32;
                sw.set_bounds(Rect::new(x, fy + 2, w, row_h + pad - 2), &mut inv);
                sw.paint(&mut dc, th);
                x += w;
            }
            let info = format!(
                "{}–{} / {} · {}",
                if self.vis.is_empty() { 0 } else { first + 1 },
                last,
                self.vis.len(),
                self.fmt.name()
            );
            let iw = dc.text_width(&info);
            let ity = dc.text_center_y(fy, hi - fy);
            dc.text(
                wi - iw - pad,
                ity,
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
            // 스위치 툴팁(600ms 머문 뒤 · 푸터 위쪽에 — 아래는 창 밖).
            if self.tooltip_due() {
                if let Some((i, _)) = self.sw_hover {
                    let (b, msg) = match i {
                        0 => (self.wrap_switch.bounds(), Msg::TipLogWrap),
                        1 => (self.newest_switch.bounds(), Msg::TipLogSort),
                        2 => (self.auto_switch.bounds(), Msg::TipLogScroll),
                        _ => (self.top_switch.bounds(), Msg::TipLogTop),
                    };
                    let text = t(msg);
                    dc.select_font(FontSlot::Status, false);
                    let lines = text.split('\n').count() as i32;
                    let tip_h = dc.text_height() * lines + (8.0 * s).round() as i32;
                    let gap = (6.0 * s).round() as i32;
                    // draw_tooltip은 anchor 아래 gap에 그린다 → 아래가 푸터 위에 닿는 가짜 anchor.
                    let anchor = Rect::new(b.x, fy - gap - tip_h - gap - 1, b.w, 1);
                    draw_tooltip(&mut dc, th, anchor, wi, text, s);
                    self.tip_shown = true;
                }
            }
            // 우클릭 메뉴(맨 위 층).
            self.menu.paint(&mut dc, th);
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
