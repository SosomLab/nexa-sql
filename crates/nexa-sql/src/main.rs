//! Nexa SQL GUI — M2 최소 슬라이스(DR-18 · 사용자 "GUI 최소 기능을 병행").
//!
//! 창 하나: [접속 문자열 · Connect] / SQL 편집기(고정폭 · nexa-ctl TextBox 다중행 · IME) / 결과 그리드(자체 가상화) / 상태줄.
//! 실행은 워커 스레드의 [`nsql_run::Runner`]가 하고, 결과는 채널 + `EventLoopProxy`로 UI에 온다(UI 스레드는 기다리지 않는다).
//! 폰트: UI = 한글 UI 본, 편집기·그리드 = 고정폭(D2Coding 우선) + 한글 폴백([docs/14](../../../docs/14-fonts-and-feature-modules.md)).
//!
//! 키: `Ctrl/⌘+Enter` = 선택 영역(없으면 전체) 실행 · `F5` = 전체 실행 · `Ctrl/⌘+L` = 접속 필드로.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod grid;
mod worker;

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nexa_gfx::{Font, Surface};
use nsql_core::Dialect;
use nsql_run::RunEvent;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// UI 스레드를 깨우는 사용자 이벤트(워커가 보냄).
#[derive(Debug)]
struct Wake;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    Connect,
    Editor,
    Grid,
}

struct App {
    window: Option<Rc<Window>>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    ui_font: Font,
    mono_font: Font,
    theme: Theme,
    scale: f32,
    // 컨트롤
    connect: TextBox,
    connect_btn: Button,
    run_btn: Button,
    editor: TextBox,
    grid: grid::Grid,
    focus: Focus,
    // 워커
    worker: worker::Handle,
    events: mpsc::Receiver<RunEvent>,
    busy: bool,
    status: String,
    log: Vec<String>,
    // 입력 상태
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    started: Instant,
}

fn px(v: f32, s: f32) -> i32 {
    (v * s).round() as i32
}

impl App {
    fn layout(&mut self) {
        let Some(win) = &self.window else { return };
        let size = win.inner_size();
        let (w, h) = (size.width as i32, size.height as i32);
        let s = self.scale;
        let mut inv = Invalidations::default();
        let pad = px(8.0, s);
        let top_h = px(36.0, s);
        let status_h = px(24.0, s);
        let btn_w = px(84.0, s);
        self.connect.set_bounds(
            Rect::new(
                pad,
                px(6.0, s),
                w - pad * 3 - btn_w * 2,
                top_h - px(12.0, s),
            ),
            &mut inv,
        );
        self.connect_btn.set_bounds(
            Rect::new(
                w - pad * 2 - btn_w * 2,
                px(6.0, s),
                btn_w,
                top_h - px(12.0, s),
            ),
            &mut inv,
        );
        self.run_btn.set_bounds(
            Rect::new(w - pad - btn_w, px(6.0, s), btn_w, top_h - px(12.0, s)),
            &mut inv,
        );
        let body_top = top_h;
        let body_h = h - body_top - status_h;
        let editor_h = (body_h as f32 * 0.5) as i32;
        self.editor.set_bounds(
            Rect::new(pad, body_top, w - pad * 2, editor_h - pad),
            &mut inv,
        );
        self.grid.set_bounds(Rect::new(
            pad,
            body_top + editor_h,
            w - pad * 2,
            body_h - editor_h - pad,
        ));
        for c in [&mut self.connect, &mut self.editor] {
            c.set_scale(s);
        }
        self.connect_btn.set_scale(s);
        self.run_btn.set_scale(s);
    }

    fn set_focus(&mut self, f: Focus) {
        self.focus = f;
        self.connect.set_focused(f == Focus::Connect);
        self.editor.set_focused(f == Focus::Editor);
        if let Some(w) = &self.window {
            w.set_ime_allowed(matches!(f, Focus::Connect | Focus::Editor));
        }
    }

    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn run_sql(&mut self, all: bool) {
        if self.busy {
            self.status = "실행 중…".into();
            return;
        }
        let text = if all {
            None
        } else {
            self.editor
                .copy_selection()
                .filter(|s| !s.trim().is_empty())
        };
        let src = text.unwrap_or_else(|| self.editor.text());
        if src.trim().is_empty() {
            self.status = "실행할 SQL이 없습니다".into();
            return;
        }
        self.busy = true;
        self.status = "실행 중…".into();
        self.log.clear();
        self.worker.send(worker::Cmd::Run(src));
        self.redraw();
    }

    fn do_connect(&mut self) {
        let target = self.connect.text();
        if target.trim().is_empty() {
            self.status =
                "접속 문자열을 입력하세요 (예: sqlite::memory: · oracle://user:pass@host:1521/svc)"
                    .into();
            return;
        }
        self.busy = true;
        self.status = format!("접속 중… {target}");
        self.worker.send(worker::Cmd::Connect(target));
        self.redraw();
    }

    fn drain_events(&mut self) {
        let mut changed = false;
        while let Ok(ev) = self.events.try_recv() {
            changed = true;
            match ev {
                RunEvent::Begin { .. } => {}
                RunEvent::ResultSet { rs, elapsed, .. } => {
                    self.status = format!("{} rows · {:.3}s", rs.rows.len(), elapsed.as_secs_f64());
                    self.grid.set_result(rs);
                }
                RunEvent::Done {
                    rows_affected,
                    elapsed,
                    ..
                } => {
                    self.status = match rows_affected {
                        Some(n) => format!("{n} rows affected · {:.3}s", elapsed.as_secs_f64()),
                        None => format!("OK · {:.3}s", elapsed.as_secs_f64()),
                    };
                }
                RunEvent::Print { pairs } => {
                    for (n, v) in pairs {
                        self.log.push(format!("{n} = {}", v.display()));
                    }
                    self.grid.set_messages(self.log.clone());
                }
                RunEvent::Message(m) => {
                    self.log.push(m);
                    self.grid.set_messages(self.log.clone());
                }
                RunEvent::Connected {
                    description,
                    dialect,
                } => {
                    self.status = format!("접속: {description} ({dialect})");
                    self.busy = false;
                }
                RunEvent::Disconnected => {
                    self.status = "접속 해제".into();
                }
                RunEvent::Error { line, error, .. } => {
                    self.status = format!("ERROR line {line}: {error}");
                    self.log.push(format!("ERROR line {line}: {error}"));
                    self.grid.set_messages(self.log.clone());
                    self.busy = false;
                }
            }
        }
        while let Ok(done) = self.worker.done.try_recv() {
            changed = true;
            self.busy = false;
            if let Some(m) = done {
                self.status = m;
            }
        }
        if changed {
            self.redraw();
        }
    }

    fn paint(&mut self) {
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
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let th = self.theme;
            // ── UI 층(한글 UI 본)
            {
                let prefs = FontPrefs {
                    base: SlotFont {
                        size: 14.0,
                        bold: false,
                        italic: false,
                    },
                    ..FontPrefs::default()
                };
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s)
                    .with_fonts(prefs)
                    .with_caret_on(caret_on);
                dc.fill_rect(Rect::new(0, 0, wi, hi), th.window_bg);
                dc.fill_rect(Rect::new(0, 0, wi, px(36.0, s)), th.chrome_bg);
                dc.fill_rect(Rect::new(0, px(36.0, s) - 1, wi, 1), th.border);
                self.connect.paint(&mut dc, &th);
                self.connect_btn.paint(&mut dc, &th);
                self.run_btn.paint(&mut dc, &th);
                // 상태줄
                let sy = hi - px(24.0, s);
                dc.fill_rect(Rect::new(0, sy, wi, px(24.0, s)), th.chrome_bg);
                dc.fill_rect(Rect::new(0, sy, wi, 1), th.border);
                dc.select_font(FontSlot::Base, false);
                let busy = if self.busy { "⏳ " } else { "" };
                dc.text(
                    px(8.0, s),
                    sy + px(5.0, s),
                    Rect::new(0, sy, wi, px(24.0, s)),
                    &format!("{busy}{}", self.status),
                    th.text_dim,
                );
                let hint = "⌘/Ctrl+Enter 실행 · F5 전체 · ⌘/Ctrl+L 접속";
                let hw = dc.text_width(hint);
                dc.text(
                    wi - px(8.0, s) - hw,
                    sy + px(5.0, s),
                    Rect::new(0, sy, wi, px(24.0, s)),
                    hint,
                    th.text_dim,
                );
            }
            // ── 고정폭 층(편집기·그리드)
            {
                let prefs = FontPrefs {
                    base: SlotFont {
                        size: 14.0,
                        bold: false,
                        italic: false,
                    },
                    ..FontPrefs::default()
                };
                let mut dc = RasterCtx::new(&mut gfx, &self.mono_font, s)
                    .with_fonts(prefs)
                    .with_caret_on(caret_on);
                self.editor.paint(&mut dc, &th);
                self.grid.paint(&mut dc, &th, s);
            }
        }
        let _ = buf.present();
    }

    fn to_ctl_event(&self, event: &WindowEvent) -> Option<InputEvent> {
        let (x, y) = self.cursor;
        let key = |k: CtlKey, shift: bool, primary: bool| InputEvent::Key {
            key: k,
            shift,
            primary,
        };
        Some(match event {
            WindowEvent::CursorMoved { position, .. } => InputEvent::MouseMove {
                x: position.x as i32,
                y: position.y as i32,
            },
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, winit::event::MouseButton::Left) => InputEvent::MouseDown {
                    x,
                    y,
                    shift: self.shift,
                    primary: self.primary,
                },
                (ElementState::Released, winit::event::MouseButton::Left) => {
                    InputEvent::MouseUp { x, y }
                }
                (ElementState::Pressed, winit::event::MouseButton::Right) => {
                    InputEvent::RightDown { x, y }
                }
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
                    Key::Named(NamedKey::PageUp) => key(CtlKey::PageUp, self.shift, self.primary),
                    Key::Named(NamedKey::PageDown) => {
                        key(CtlKey::PageDown, self.shift, self.primary)
                    }
                    Key::Named(NamedKey::Delete) => key(CtlKey::Delete, false, false),
                    Key::Named(NamedKey::Backspace) => InputEvent::Char {
                        c: '\u{8}',
                        now_ms: 0,
                    },
                    Key::Named(NamedKey::Tab) => InputEvent::Char { c: '\t', now_ms: 0 },
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

    fn route(&mut self, ev: InputEvent) {
        let mut inv = Invalidations::default();
        // 마우스 다운은 포커스를 옮긴다.
        if let InputEvent::MouseDown { x, y, .. } = ev {
            let p = Point { x, y };
            if self.connect.bounds().contains(p) {
                self.set_focus(Focus::Connect);
            } else if self.editor.bounds().contains(p) {
                self.set_focus(Focus::Editor);
            } else if self.grid.bounds.contains(p) {
                self.set_focus(Focus::Grid);
            }
        }
        // 버튼은 항상 마우스 사건을 받는다.
        if matches!(
            ev,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
        ) {
            self.connect_btn.on_event(&ev, &mut inv);
            self.run_btn.on_event(&ev, &mut inv);
            if self.connect_btn.take_clicked() {
                self.do_connect();
            }
            if self.run_btn.take_clicked() {
                self.run_sql(true);
            }
        }
        let over_grid = match ev {
            InputEvent::Wheel { .. } | InputEvent::HWheel { .. } => {
                self.grid.bounds.contains(Point {
                    x: self.cursor.0,
                    y: self.cursor.1,
                })
            }
            _ => false,
        };
        if over_grid {
            self.grid.on_event(&ev);
            inv.push(self.grid.bounds);
        } else {
            match self.focus {
                Focus::Connect => {
                    if matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Enter,
                            ..
                        }
                    ) {
                        self.do_connect();
                    } else {
                        self.connect.on_event(&ev, &mut inv);
                    }
                }
                Focus::Editor => self.editor.on_event(&ev, &mut inv),
                Focus::Grid => {
                    self.grid.on_event(&ev);
                    inv.push(self.grid.bounds);
                }
            }
            // 편집 컨텍스트 요청(복사·붙여넣기 — 호스트 몫)
            for tb in [&mut self.connect, &mut self.editor] {
                if let Some(act) = tb.take_edit_ctx() {
                    self.status = format!("{act:?}: 클립보드 연동은 T-16b");
                }
            }
        }
        if !inv.is_empty() {
            self.redraw();
        }
    }
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("Nexa SQL")
            .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 720.0));
        let Ok(win) = el.create_window(attrs) else {
            eprintln!("창 생성 실패");
            el.exit();
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        match softbuffer::Context::new(win.clone()) {
            Ok(ctx) => {
                match softbuffer::Surface::new(&ctx, win.clone()) {
                    Ok(s) => self.surface = Some(s),
                    Err(e) => eprintln!("softbuffer surface 실패: {e}"),
                }
                self.ctx = Some(ctx);
            }
            Err(e) => eprintln!("softbuffer context 실패: {e}"),
        }
        self.window = Some(win);
        self.layout();
        self.set_focus(Focus::Editor);
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, _ev: Wake) {
        self.drain_events();
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // 캐럿 깜빡임 — 포커스 편집기가 있을 때만 0.5초 주기로 다시 그린다.
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(500),
        ));
        if matches!(self.focus, Focus::Editor | Focus::Connect) {
            self.redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match &event {
            WindowEvent::CloseRequested => {
                self.worker.send(worker::Cmd::Quit);
                el.exit();
                return;
            }
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
                return;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
                self.redraw();
                return;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
            }
            WindowEvent::Ime(ime) => {
                let mut inv = Invalidations::default();
                let tb = match self.focus {
                    Focus::Editor => Some(&mut self.editor),
                    Focus::Connect => Some(&mut self.connect),
                    Focus::Grid => None,
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
                    self.redraw();
                }
                return;
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Enter) if self.primary => {
                        self.run_sql(false);
                        return;
                    }
                    Key::Named(NamedKey::F5) => {
                        self.run_sql(true);
                        return;
                    }
                    Key::Character("l" | "L") if self.primary => {
                        self.set_focus(Focus::Connect);
                        self.redraw();
                        return;
                    }
                    Key::Character("a" | "A") if self.primary => {
                        self.route(InputEvent::SelectAll);
                        return;
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.paint();
                return;
            }
            _ => {}
        }
        if let Some(ev) = self.to_ctl_event(&event) {
            self.route(ev);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let ui = nexa_font::ui_font(None);
    let mono = nexa_font::mono_font(None);
    let (Some(ui), Some(mono)) = (ui, mono) else {
        eprintln!("시스템 폰트를 찾지 못했습니다(nexa-font 후보 목록 확인)");
        std::process::exit(1);
    };
    println!(
        "UI 글꼴: {} · 고정폭: {} · 한글 커버 {}",
        ui.chain.join(" → "),
        mono.chain.join(" → "),
        mono.font.covers('가')
    );
    if args.first().map(String::as_str) == Some("--smoke") {
        println!("smoke ok — 드라이버: {:?}", nsql_drivers::available());
        return;
    }
    let Ok(el) = EventLoop::<Wake>::with_user_event().build() else {
        eprintln!("이벤트 루프 생성 실패");
        std::process::exit(1);
    };
    let proxy: EventLoopProxy<Wake> = el.create_proxy();
    let (worker, events) = worker::spawn(
        Dialect::Oracle,
        Box::new(move || {
            let _ = proxy.send_event(Wake);
        }),
    );
    let initial_target = args
        .first()
        .cloned()
        .unwrap_or_else(|| "sqlite::memory:".into());
    let mut app = App {
        window: None,
        ctx: None,
        surface: None,
        ui_font: ui.font,
        mono_font: mono.font,
        theme: Theme::dark(),
        scale: 1.0,
        connect: TextBox::new(
            "sqlite::memory: · oracle://user:pass@host:1521/svc · mssql://user:pass@host:1433/db",
        )
        .with_text(&initial_target),
        connect_btn: Button::new("Connect"),
        run_btn: Button::new("Run ▶"),
        editor: TextBox::new("SELECT … ;  EXEC :V := 'x';  PRINT V").with_multiline(),
        grid: grid::Grid::default(),
        focus: Focus::Editor,
        worker,
        events,
        busy: false,
        status: "Connect를 누르거나 ⌘/Ctrl+L 후 Enter".into(),
        log: Vec::new(),
        cursor: (0, 0),
        shift: false,
        primary: false,
        started: Instant::now(),
    };
    if let Err(e) = el.run_app(&mut app) {
        eprintln!("이벤트 루프 오류: {e}");
        std::process::exit(1);
    }
}
