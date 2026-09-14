//! 접속 창(별도 창 · Golden "Database Login" 차용 · DR-23 · T-31 · 사용자 09-14).
//!
//! 기본 = **로그인 목록**만(저장 프로필 · 필터 · 이름/종류/사용자/대상) + New/Edit/Delete/Close.
//! New/Edit를 누르면 오른쪽에서 **상세 폼**([`ConnectPanel`] — DB 종류·호스트/포트·DB·사용자·비밀번호·프로필 · Test/Connect/Save · 상태)이
//! 슬라이딩해 들어온다(≈200ms ease-out · Esc = 접기). 목록은 그만큼 좁아진다.
//! 행 클릭 = 폼에 채움 · 더블클릭 = 바로 접속 · 접속되면 창이 닫힌다. `Ctrl/⌘+L` · 툴바 ⇄ · Run ▸ Connect로 연다.
//! 창 골격은 로그 창과 같다(winit + softbuffer + nexa-ctl 래스터). I/O(저장소·워커)는 전부 호스트 몫 — [`ConnWinAction`]으로 요청.

use crate::connect::{ConnectPanel, PanelAction};
use crate::probe::{self, ProbeEntry, ProbeHub, ProbeReq, ProbeStatus};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use nsql_vault::{Profile, Vault};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Duration;
use std::time::Instant;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 창이 호스트에 요청하는 것.
pub(crate) enum ConnWinAction {
    /// `RedrawRequested` — 호스트가 폰트·테마를 넘겨 [`ConnWin::paint`]를 부른다.
    Paint,
    /// 폼의 요청(접속·테스트·저장·프로필 로드) — 호스트 `handle_panel_action`.
    Panel(PanelAction),
    /// 목록 더블클릭 — 저장소에서 읽어 폼에 채우고 바로 접속.
    Login(String),
    /// 목록에서 프로필 삭제.
    Delete(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WFocus {
    Panel,
    Filter,
    /// 목록(행 선택 · ↑↓ · Enter 접속 · Delete).
    List,
}

pub(crate) struct ConnWin {
    window: Option<Rc<Window>>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    started: Instant,
    pub(crate) panel: ConnectPanel,
    profiles: Vec<Profile>,
    /// 필터 통과한 profiles index.
    shown: Vec<usize>,
    /// 선택(shown 기준 위치).
    sel: Option<usize>,
    hover: Option<usize>,
    filter: TextBox,
    btn_new: Button,
    btn_edit: Button,
    btn_delete: Button,
    btn_close: Button,
    list: Rect,
    row_h: i32,
    focus: WFocus,
    last_click: Option<(usize, Instant)>,
    /// 상세 폼 열림 정도 0.0(닫힘)~1.0(열림) — 슬라이딩 애니메이션 현재값.
    detail_t: f32,
    /// (시작 시각 · from · to) — 진행 중 애니메이션.
    anim: Option<(Instant, f32, f32)>,
    /// 서버 신호등(프로필 이름 → 상태·재시도) — [`crate::probe`].
    probes: HashMap<String, ProbeEntry>,
    hub: Option<ProbeHub>,
    /// 한 번 이상 접속 성공한 프로필(대상 집합).
    connected: HashSet<String>,
    probe_enabled: bool,
    probe_max_retries: u32,
    probe_timeout: Duration,
}

const SLIDE_MS: f32 = 200.0;

const PANEL_W: f32 = 320.0;
/// 기본 창 크기(목록만 · 사용자 캡처 09-14) — New/Edit 시 오른쪽으로 PANEL_W만큼 커진다.
const BASE_W: f32 = 640.0;
const BASE_H: f32 = 520.0;
const DBLCLICK_MS: u128 = 400;

impl ConnWin {
    pub(crate) fn new(panel: ConnectPanel) -> Self {
        let mut w = ConnWin {
            window: None,
            ctx: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            started: Instant::now(),
            panel,
            profiles: Vec::new(),
            shown: Vec::new(),
            sel: None,
            hover: None,
            filter: TextBox::new(t(Msg::PhFilter)),
            btn_new: Button::new(t(Msg::BtnNew)),
            btn_edit: Button::new(t(Msg::BtnEdit)),
            btn_delete: Button::new(t(Msg::BtnDelete)),
            btn_close: Button::new(t(Msg::BtnClose)),
            list: Rect::new(0, 0, 0, 0),
            row_h: 24,
            focus: WFocus::Filter,
            last_click: None,
            detail_t: 0.0,
            anim: None,
            probes: HashMap::new(),
            hub: None,
            connected: probe::connected_names(),
            probe_enabled: true,
            probe_max_retries: 5,
            probe_timeout: Duration::from_secs(2),
        };
        w.refresh_profiles(None);
        w
    }

    /// 프로브 스레드·설정 주입(부팅 시 한 번).
    pub(crate) fn set_probe(
        &mut self,
        hub: ProbeHub,
        enabled: bool,
        max_retries: u32,
        timeout_secs: u64,
    ) {
        self.hub = Some(hub);
        self.probe_enabled = enabled;
        self.probe_max_retries = max_retries;
        self.probe_timeout = Duration::from_secs(timeout_secs.max(1));
    }

    /// 접속 성공 — 이 프로필을 신호등 대상에 올리고(영속) 바로 초록으로.
    pub(crate) fn mark_connected(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        probe::mark_connected(name);
        self.connected.insert(name.to_string());
        let mut e = ProbeEntry::fresh(Instant::now());
        e.status = ProbeStatus::Up;
        e.next_at = None;
        self.probes.insert(name.to_string(), e);
    }

    /// 대상 프로필(한 번 이상 접속 · 호스트/포트 있음)에 프로브를 예약 — 창을 열 때 한 번.
    fn schedule_probes(&mut self) {
        if !self.probe_enabled || self.window.is_none() {
            return;
        }
        let now = Instant::now();
        for p in &self.profiles {
            if !self.connected.contains(&p.name) || p.spec.host.is_none() || p.spec.port.is_none() {
                continue;
            }
            self.probes.insert(p.name.clone(), ProbeEntry::fresh(now));
        }
    }

    /// 예약된 프로브를 보낸다(순차 스레드) · 다음 예약 시각을 돌려준다(호스트 WaitUntil).
    pub(crate) fn tick(&mut self, now: Instant) -> Option<Instant> {
        if self.window.is_none() || !self.probe_enabled {
            return None;
        }
        let Some(hub) = &self.hub else { return None };
        let mut next: Option<Instant> = None;
        let mut changed = false;
        for p in &self.profiles {
            let Some(e) = self.probes.get_mut(&p.name) else {
                continue;
            };
            match e.next_at {
                Some(t) if t <= now => {
                    if let (Some(h), Some(port)) = (p.spec.host.clone(), p.spec.port) {
                        hub.request(ProbeReq {
                            name: p.name.clone(),
                            host: h,
                            port,
                            timeout: self.probe_timeout,
                        });
                        e.status = ProbeStatus::Checking;
                        e.next_at = None;
                        changed = true;
                    }
                }
                Some(t) => next = Some(next.map_or(t, |n: Instant| n.min(t))),
                None => {}
            }
        }
        if changed {
            self.redraw();
        }
        next
    }

    /// 프로브 결과 수거(호스트가 Wake마다 부른다). 바뀐 게 있으면 true.
    pub(crate) fn drain_probes(&mut self) -> bool {
        let Some(hub) = &self.hub else { return false };
        let now = Instant::now();
        let mut changed = false;
        while let Some(r) = hub.try_recv() {
            if let Some(e) = self.probes.get_mut(&r.name) {
                e.apply(r.ok, now, self.probe_max_retries);
                changed = true;
            }
        }
        if changed {
            self.redraw();
        }
        changed
    }

    fn probe_status(&self, name: &str) -> Option<ProbeStatus> {
        self.probes.get(name).map(|e| e.status)
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
    }

    /// 저장소를 다시 읽어 목록·폼 콤보를 갱신(`select` = 선택 유지할 이름).
    pub(crate) fn refresh_profiles(&mut self, select: Option<&str>) {
        self.profiles = Vault::open_default()
            .and_then(|v| v.list())
            .unwrap_or_default();
        self.refilter();
        if let Some(n) = select {
            self.sel = self.shown.iter().position(|&i| self.profiles[i].name == n);
        }
        self.redraw();
    }

    /// 언어 전환 — 라벨 재생성.
    pub(crate) fn relabel(&mut self) {
        self.panel.relabel();
        let text = self.filter.text();
        self.filter = TextBox::new(t(Msg::PhFilter)).with_text(&text);
        self.btn_new.set_label(t(Msg::BtnNew));
        self.btn_edit.set_label(t(Msg::BtnEdit));
        self.btn_delete.set_label(t(Msg::BtnDelete));
        self.btn_close.set_label(t(Msg::BtnClose));
        self.layout();
        self.redraw();
    }

    fn refilter(&mut self) {
        let q = self.filter.text().trim().to_lowercase();
        self.shown = self
            .profiles
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                q.is_empty()
                    || p.name.to_lowercase().contains(&q)
                    || target_of(p).to_lowercase().contains(&q)
                    || p.spec
                        .user
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        if self.sel.is_some_and(|s| s >= self.shown.len()) {
            self.sel = None;
        }
    }

    /// 메인 창 위 가운데에 연다(`over` = 메인 창 바깥 좌표·크기). 이미 열려 있으면 앞으로.
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        over: Option<(i32, i32, u32, u32)>,
    ) {
        if let Some(w) = &self.window {
            w.focus_window();
            return;
        }
        let (lw, lh) = (f64::from(BASE_W), f64::from(BASE_H));
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinLogin)))
            .with_theme(theme)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
        if let Some((x, y, w, h)) = over {
            let cx = x + (w as i32 - lw as i32) / 2;
            let cy = y + (h as i32 - lh as i32) / 2;
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(cx.max(0), cy.max(0)));
        }
        let Ok(win) = el.create_window(crate::icon::with_icon(attrs)) else {
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
        win.set_ime_allowed(true);
        self.window = Some(win);
        self.refresh_profiles(None);
        self.probes.clear();
        self.schedule_probes();
        self.detail_t = 0.0;
        self.anim = None;
        self.set_focus(WFocus::List);
        if self.profiles.is_empty() {
            self.open_detail(true);
        }
        self.layout();
        self.redraw();
    }

    /// 상세 폼 열기(슬라이드 인) · `clear` = New(폼 비움) / false = Edit(폼은 호스트가 채운다).
    fn open_detail(&mut self, clear: bool) {
        if clear {
            self.panel.clear();
            self.sel = None;
        }
        if self.detail_t < 1.0 {
            self.anim = Some((Instant::now(), self.detail_t, 1.0));
        }
        self.set_focus(WFocus::Panel);
        self.panel.focus_host();
        self.redraw();
    }

    /// 상세 폼 접기(슬라이드 아웃).
    fn close_detail(&mut self) {
        if self.detail_t > 0.0 {
            self.anim = Some((Instant::now(), self.detail_t, 0.0));
        }
        self.set_focus(WFocus::List);
        self.redraw();
    }

    fn detail_open(&self) -> bool {
        self.detail_t > 0.0 || matches!(self.anim, Some((_, _, to)) if to > 0.0)
    }

    /// 애니메이션 진행(페인트 시작에 호출) — 아직 진행 중이면 true.
    fn advance(&mut self) -> bool {
        let Some((t0, from, to)) = self.anim else {
            return false;
        };
        let p = (t0.elapsed().as_secs_f32() * 1000.0 / SLIDE_MS).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - p) * (1.0 - p); // ease-out
        self.detail_t = from + (to - from) * e;
        let done = p >= 1.0;
        if done {
            self.detail_t = to;
            self.anim = None;
        }
        // 창 자체가 오른쪽으로 커진다/줄어든다(목록 폭은 그대로 · 폼은 새 영역에 드러난다).
        if let Some(w) = &self.window {
            let lw = f64::from(BASE_W + PANEL_W * self.detail_t);
            let _ = w.request_inner_size(winit::dpi::LogicalSize::new(lw, f64::from(BASE_H)));
        }
        !done
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.ctx = None;
        self.window = None;
        self.panel.set_focused(false);
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn layout(&mut self) {
        let Some(win) = &self.window else { return };
        let size = win.inner_size();
        let (w, h) = (size.width as i32, size.height as i32);
        let s = self.scale;
        let pad = self.s(10.0);
        let pw = self.s(PANEL_W);
        // 상세 폼 = 오른쪽에서 detail_t만큼 들어와 있다(닫힘 = 창 밖).
        let open_px = (pw as f32 * self.detail_t).round() as i32;
        self.panel.set_bounds(Rect::new(w - open_px, 0, pw, h), s);
        let x0 = pad;
        let rw = (w - open_px - x0 - pad).max(0);
        let row = self.s(28.0);
        let btn_w = self.s(72.0);
        let mut inv = Invalidations::default();
        // 제목 줄(라벨) → 필터 + 버튼 줄 → 목록
        let y1 = pad + self.s(22.0);
        let bx = x0 + rw - btn_w * 4 - pad * 3;
        self.filter
            .set_bounds(Rect::new(x0, y1, (bx - x0 - pad).max(0), row), &mut inv);
        self.btn_new
            .set_bounds(Rect::new(bx, y1, btn_w, row), &mut inv);
        self.btn_edit
            .set_bounds(Rect::new(bx + btn_w + pad, y1, btn_w, row), &mut inv);
        self.btn_delete
            .set_bounds(Rect::new(bx + (btn_w + pad) * 2, y1, btn_w, row), &mut inv);
        self.btn_close
            .set_bounds(Rect::new(bx + (btn_w + pad) * 3, y1, btn_w, row), &mut inv);
        for c in [
            &mut self.btn_new,
            &mut self.btn_edit,
            &mut self.btn_delete,
            &mut self.btn_close,
        ] {
            c.set_scale(s);
        }
        self.filter.set_scale(s);
        let ly = y1 + row + pad;
        self.list = Rect::new(x0, ly, rw, (h - ly - pad).max(0));
        self.row_h = self.s(24.0);
    }

    fn row_at(&self, p: Point) -> Option<usize> {
        let body = Rect::new(
            self.list.x,
            self.list.y + self.row_h,
            self.list.w,
            (self.list.h - self.row_h).max(0),
        );
        if !body.contains(p) {
            return None;
        }
        let i = ((p.y - body.y) / self.row_h.max(1)) as usize;
        (i < self.shown.len()).then_some(i)
    }

    fn selected_name(&self) -> Option<String> {
        self.sel
            .and_then(|s| self.shown.get(s))
            .map(|&i| self.profiles[i].name.clone())
    }

    fn set_focus(&mut self, f: WFocus) {
        self.focus = f;
        self.panel.set_focused(f == WFocus::Panel);
        self.filter.set_focused(f == WFocus::Filter);
        // 버튼은 키보드 포커스를 갖지 않는다(클릭 뒤 링이 남던 버그 09-14).
        for b in [
            &mut self.btn_new,
            &mut self.btn_edit,
            &mut self.btn_delete,
            &mut self.btn_close,
        ] {
            b.set_focused(false);
        }
    }

    // ── 이벤트

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> Vec<ConnWinAction> {
        let mut out = Vec::new();
        match ev {
            WindowEvent::CloseRequested => self.close(),
            WindowEvent::RedrawRequested => out.push(ConnWinAction::Paint),
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
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
                let p = Point {
                    x: self.cursor.0,
                    y: self.cursor.1,
                };
                let h = self.row_at(p);
                if h != self.hover {
                    self.hover = h;
                    self.redraw();
                }
                self.route(InputEvent::MouseMove { x: p.x, y: p.y }, &mut out);
            }
            WindowEvent::Ime(ime) => {
                let mut inv = Invalidations::default();
                let tb = match self.focus {
                    WFocus::Panel => self.panel.focused_textbox(),
                    WFocus::Filter => Some(&mut self.filter),
                    WFocus::List => None,
                };
                if let Some(tb) = tb {
                    match ime {
                        Ime::Preedit(t, _) => tb.set_preedit(t, &mut inv),
                        Ime::Commit(t) => {
                            tb.set_preedit("", &mut inv);
                            for c in t.chars().filter(|c| !c.is_control()) {
                                tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                            }
                        }
                        _ => {}
                    }
                    if self.focus == WFocus::Filter {
                        self.refilter();
                    }
                    self.redraw();
                }
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) if !self.panel.popup_open() => {
                        if self.detail_open() {
                            self.close_detail();
                        } else {
                            self.close();
                        }
                    }
                    Key::Named(NamedKey::Enter)
                        if matches!(self.focus, WFocus::Filter | WFocus::List) =>
                    {
                        if let Some(n) = self.selected_name() {
                            out.push(ConnWinAction::Login(n));
                        }
                    }
                    Key::Named(NamedKey::ArrowDown)
                        if matches!(self.focus, WFocus::Filter | WFocus::List) =>
                    {
                        if !self.shown.is_empty() {
                            self.sel =
                                Some(self.sel.map_or(0, |s| (s + 1).min(self.shown.len() - 1)));
                            self.redraw();
                        }
                    }
                    Key::Named(NamedKey::ArrowUp)
                        if matches!(self.focus, WFocus::Filter | WFocus::List) =>
                    {
                        self.sel = self.sel.map(|s| s.saturating_sub(1));
                        self.redraw();
                    }
                    Key::Named(NamedKey::Delete)
                        if matches!(self.focus, WFocus::Filter | WFocus::List) =>
                    {
                        if let Some(n) = self.selected_name() {
                            out.push(ConnWinAction::Delete(n));
                        }
                    }
                    _ => {
                        if let Some(ie) = self.to_input(ev) {
                            self.route(ie, &mut out);
                        }
                    }
                }
            }
            _ => {
                if let Some(ie) = self.to_input(ev) {
                    self.route(ie, &mut out);
                }
            }
        }
        out
    }

    fn to_input(&self, event: &WindowEvent) -> Option<InputEvent> {
        let (x, y) = self.cursor;
        let key = |k: CtlKey, shift: bool, primary: bool| InputEvent::Key {
            key: k,
            shift,
            primary,
        };
        Some(match event {
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => InputEvent::MouseDown {
                    x,
                    y,
                    shift: self.shift,
                    primary: self.primary,
                },
                (ElementState::Released, MouseButton::Left) => InputEvent::MouseUp { x, y },
                (ElementState::Pressed, MouseButton::Right) => InputEvent::RightDown { x, y },
                _ => return None,
            },
            WindowEvent::MouseWheel { delta, .. } => InputEvent::Wheel {
                delta: match delta {
                    MouseScrollDelta::LineDelta(_, dy) => (*dy * 120.0) as i32,
                    MouseScrollDelta::PixelDelta(p) => p.y as i32,
                },
            },
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Enter) => key(CtlKey::Enter, self.shift, self.primary),
                    Key::Named(NamedKey::Escape) => key(CtlKey::Escape, false, false),
                    Key::Named(NamedKey::ArrowUp) => key(CtlKey::Up, self.shift, self.primary),
                    Key::Named(NamedKey::ArrowDown) => key(CtlKey::Down, self.shift, self.primary),
                    Key::Named(NamedKey::ArrowLeft) => key(CtlKey::Left, self.shift, self.primary),
                    Key::Named(NamedKey::ArrowRight) => {
                        key(CtlKey::Right, self.shift, self.primary)
                    }
                    Key::Named(NamedKey::Home) => key(CtlKey::Home, self.shift, self.primary),
                    Key::Named(NamedKey::End) => key(CtlKey::End, self.shift, self.primary),
                    Key::Named(NamedKey::Delete) => key(CtlKey::Delete, false, false),
                    Key::Named(NamedKey::Tab) => InputEvent::Char { c: '\t', now_ms: 0 },
                    Key::Named(NamedKey::Backspace) => InputEvent::Char {
                        c: '\u{8}',
                        now_ms: 0,
                    },
                    Key::Named(NamedKey::Space) => InputEvent::Char { c: ' ', now_ms: 0 },
                    Key::Character(t) if !self.primary => {
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

    fn route(&mut self, ev: InputEvent, out: &mut Vec<ConnWinAction>) {
        let mut inv = Invalidations::default();
        // 열린 콤보(폼)는 모달.
        if self.panel.popup_open() {
            if let Some(a) = self.panel.route(&ev, &mut inv) {
                out.push(ConnWinAction::Panel(a));
            }
            self.redraw();
            return;
        }
        let is_mouse = matches!(
            ev,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
        );
        if let InputEvent::MouseDown { x, y, .. } = ev {
            let p = Point { x, y };
            if self.panel.bounds().contains(p) {
                self.set_focus(WFocus::Panel);
            } else if self.filter.bounds().contains(p) {
                self.set_focus(WFocus::Filter);
            } else if let Some(row) = self.row_at(p) {
                self.set_focus(WFocus::List);
                let now = Instant::now();
                let double = matches!(self.last_click, Some((r, t)) if r == row && t.elapsed().as_millis() < DBLCLICK_MS);
                self.sel = Some(row);
                self.last_click = Some((row, now));
                if let Some(n) = self.selected_name() {
                    if double {
                        out.push(ConnWinAction::Login(n));
                    } else if self.detail_open() {
                        out.push(ConnWinAction::Panel(PanelAction::LoadProfile(n)));
                    }
                }
                self.redraw();
                return;
            }
        }
        if is_mouse {
            self.btn_new.on_event(&ev, &mut inv);
            self.btn_edit.on_event(&ev, &mut inv);
            self.btn_delete.on_event(&ev, &mut inv);
            self.btn_close.on_event(&ev, &mut inv);
            if self.btn_new.take_clicked() {
                self.open_detail(true);
            }
            if self.btn_edit.take_clicked() {
                if let Some(n) = self.selected_name() {
                    out.push(ConnWinAction::Panel(PanelAction::LoadProfile(n)));
                    self.open_detail(false);
                }
            }
            if self.btn_delete.take_clicked() {
                if let Some(n) = self.selected_name() {
                    out.push(ConnWinAction::Delete(n));
                }
            }
            if self.btn_close.take_clicked() {
                self.close();
                return;
            }
            if matches!(ev, InputEvent::MouseUp { .. }) {
                for b in [
                    &mut self.btn_new,
                    &mut self.btn_edit,
                    &mut self.btn_delete,
                    &mut self.btn_close,
                ] {
                    b.set_focused(false);
                }
            }
            if self.detail_t > 0.0 {
                if let Some(a) = self.panel.route(&ev, &mut inv) {
                    out.push(ConnWinAction::Panel(a));
                }
            }
        } else {
            match self.focus {
                WFocus::Panel => {
                    if let Some(a) = self.panel.route(&ev, &mut inv) {
                        out.push(ConnWinAction::Panel(a));
                    }
                }
                WFocus::Filter => {
                    self.filter.on_event(&ev, &mut inv);
                    self.refilter();
                }
                WFocus::List => {}
            }
        }
        self.redraw();
    }

    // ── 그리기

    pub(crate) fn paint(&mut self, ui: &Font, th: &Theme, font_px: f32) {
        let animating = self.advance();
        if animating || self.anim.is_none() {
            self.layout();
        }
        let statuses: Vec<Option<ProbeStatus>> = self
            .profiles
            .iter()
            .map(|p| self.probe_status(&p.name))
            .collect();
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
        let caret_on = (self.started.elapsed().as_millis() / 500) % 2 == 0;
        let detail_px = self.panel.bounds().x;
        let detail_visible = self.detail_t > 0.0;
        let pad = (10.0 * s).round() as i32;
        let list = self.list;
        let row_h = self.row_h;
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
            let mut dc = RasterCtx::new(&mut gfx, ui, s)
                .with_fonts(prefs)
                .with_caret_on(caret_on);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.window_bg);
            if detail_visible {
                self.panel.paint(&mut dc, th);
                dc.fill_rect(Rect::new(detail_px, 0, 1, hi), th.border);
            }
            dc.select_font(FontSlot::Base, false);
            let x0 = list.x;
            dc.text(
                x0,
                pad,
                Rect::new(x0, 0, list.w, hi),
                t(Msg::LblLoginList),
                th.text_dim,
            );
            self.filter.paint(&mut dc, th);
            self.btn_new.paint(&mut dc, th);
            self.btn_edit.paint(&mut dc, th);
            self.btn_delete.paint(&mut dc, th);
            self.btn_close.paint(&mut dc, th);
            // 목록
            let l = list;
            dc.fill_rect(l, th.panel_bg);
            dc.fill_rect(Rect::new(l.x, l.y, l.w, 1), th.border);
            dc.fill_rect(Rect::new(l.x, l.bottom() - 1, l.w, 1), th.border);
            dc.fill_rect(Rect::new(l.x, l.y, 1, l.h), th.border);
            dc.fill_rect(Rect::new(l.right() - 1, l.y, 1, l.h), th.border);
            let rh = row_h;
            // 첫 열 = 신호등(고정 폭) · 나머지는 비율.
            let sw = rh;
            let cols = [
                (t(Msg::ColName).to_string(), 0.22),
                (t(Msg::ColType).to_string(), 0.16),
                (t(Msg::ColUser).to_string(), 0.20),
                (t(Msg::ColTarget).to_string(), 0.42),
            ];
            let lw_cols = l.w - sw;
            let ty = |y: i32| y + (rh - dc_text_h(rh)) / 2;
            // 헤더
            dc.fill_rect(Rect::new(l.x, l.y, l.w, rh), th.chrome_bg);
            dc.fill_rect(Rect::new(l.x, l.y + rh - 1, l.w, 1), th.border);
            let mut cx = l.x + sw + pad;
            for (name, frac) in &cols {
                let cw = (lw_cols as f32 * frac) as i32;
                dc.text(
                    cx,
                    ty(l.y),
                    Rect::new(cx, l.y, cw - pad, rh),
                    name,
                    th.text_dim,
                );
                cx += cw;
            }
            let body = Rect::new(l.x, l.y + rh, l.w, (l.h - rh).max(0));
            for (row, &pi) in self.shown.iter().enumerate() {
                let y = body.y + row as i32 * rh;
                if y >= body.bottom() {
                    break;
                }
                let r = Rect::new(l.x + 1, y, l.w - 2, rh).intersection(&body);
                if Some(row) == self.sel {
                    dc.fill_rect(r, th.sel_bg);
                } else if Some(row) == self.hover {
                    dc.fill_rect(r, th.panel_bg_alt);
                }
                let p = &self.profiles[pi];
                let cells = [
                    p.name.clone(),
                    p.spec
                        .dialect
                        .map(|d| d.display_name())
                        .unwrap_or("")
                        .to_string(),
                    p.spec.user.clone().unwrap_or_default(),
                    target_of(p),
                ];
                // 신호등(초록 가능 · 노랑 확인 중 · 빨강 불가 · 회색 대상 아님)
                let dot = (rh / 2).max(6);
                let dot_color = match statuses.get(pi).copied().flatten() {
                    Some(ProbeStatus::Up) => th.ok,
                    Some(ProbeStatus::Checking) => th.warn,
                    Some(ProbeStatus::Down) => th.danger,
                    Some(ProbeStatus::Unknown) | None => th.border,
                };
                dc.fill_ellipse(
                    Rect::new(l.x + (sw - dot) / 2 + 1, y + (rh - dot) / 2, dot, dot),
                    dot_color,
                );
                let mut cx = l.x + sw + pad;
                for ((_, frac), text) in cols.iter().zip(cells.iter()) {
                    let cw = (lw_cols as f32 * frac) as i32;
                    dc.text(
                        cx,
                        ty(y),
                        Rect::new(cx, y, cw - pad, rh).intersection(&body),
                        text,
                        th.text,
                    );
                    cx += cw;
                }
            }
            if self.shown.is_empty() {
                dc.text(
                    body.x + pad,
                    ty(body.y),
                    body,
                    t(Msg::StNoProfiles),
                    th.text_dim,
                );
            }
        }
        let _ = buf.present();
        if animating {
            self.redraw();
        }
    }
}

/// 행 높이 안 글자 세로 위치 계산용(라스터 글꼴 높이 ≈ 행 높이 - 8px 여백).
fn dc_text_h(row_h: i32) -> i32 {
    (row_h - 8).max(8)
}

/// `host:port/database` (파일 DB는 경로).
fn target_of(p: &Profile) -> String {
    let host = p.spec.host.as_deref().unwrap_or("");
    let db = p.spec.database.as_deref().unwrap_or("");
    match p.spec.port {
        Some(port) if !host.is_empty() => format!("{host}:{port}/{db}"),
        _ if host.is_empty() => db.to_string(),
        _ => format!("{host}/{db}"),
    }
}
