//! Nexa SQL GUI — M2 최소 슬라이스(DR-18 · 사용자 "GUI 최소 기능을 병행").
//!
//! 창 하나: [프로필 이름 · 접속 문자열(또는 프로필 이름) · Save · Connect · Run] / SQL 편집기(고정폭 · nexa-ctl TextBox 다중행 · IME) / 결과 그리드(자체 가상화) / 상태줄.
//! 연결 프로필(T-16b): 접속 칸에 프로필 이름만 넣고 Connect · 이름 칸 + 접속 문자열 + Save로 저장(비밀번호는 `nsql-vault` 봉투 · CLI `nsql conn`과 같은 폴더).
//! 실행은 워커 스레드의 [`nsql_run::Runner`]가 하고, 결과는 채널 + `EventLoopProxy`로 UI에 온다(UI 스레드는 기다리지 않는다).
//! 폰트: UI = 한글 UI 본, 편집기·그리드 = 고정폭(D2Coding 우선) + 한글 폴백([docs/14](../../../docs/14-fonts-and-feature-modules.md)).
//!
//! 키: `Ctrl/⌘+Enter` = 선택 영역(없으면 전체) 실행 · `F5` = 전체 실행 · `Ctrl/⌘+L` = 접속 필드로 ·
//! `Ctrl/⌘+⇧T` = 테마 순환(System→Light→Dark) · `Ctrl/⌘+⇧L` = 언어 전환(en↔ko) — 둘 다 `settings.conf`에 저장(T-37/38).
//! 문자열은 전부 `nsql-i18n`(기본 영어) · 테마는 `ui.theme` + OS 판정([`theme`]) · 글꼴 크기는 `ui.font_size`/`editor.font_size`.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod grid;
mod theme;
mod worker;

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nexa_gfx::{Font, Surface};
use nsql_core::Dialect;
use nsql_i18n::{current_lang, t, tf, Msg};
use nsql_run::RunEvent;
use nsql_settings::{Settings, ThemeMode};
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
    Name,
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
    /// 앱 설정(언어·테마 모드·글꼴 크기) — 단축키로 바꾸면 즉시 저장.
    settings: Settings,
    scale: f32,
    // 컨트롤
    name: TextBox,
    connect: TextBox,
    save_btn: Button,
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
    /// 다음 캐럿 깜빡임 시각 — about_to_wait의 재그리기 게이트.
    next_blink: Instant,
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
        let name_w = px(120.0, s);
        let field_h = top_h - px(12.0, s);
        self.name
            .set_bounds(Rect::new(pad, px(6.0, s), name_w, field_h), &mut inv);
        self.connect.set_bounds(
            Rect::new(
                pad * 2 + name_w,
                px(6.0, s),
                w - pad * 5 - name_w - btn_w * 3,
                field_h,
            ),
            &mut inv,
        );
        self.save_btn.set_bounds(
            Rect::new(w - pad * 3 - btn_w * 3, px(6.0, s), btn_w, field_h),
            &mut inv,
        );
        self.connect_btn.set_bounds(
            Rect::new(w - pad * 2 - btn_w * 2, px(6.0, s), btn_w, field_h),
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
        for c in [&mut self.name, &mut self.connect, &mut self.editor] {
            c.set_scale(s);
        }
        self.save_btn.set_scale(s);
        self.connect_btn.set_scale(s);
        self.run_btn.set_scale(s);
    }

    fn set_focus(&mut self, f: Focus) {
        self.focus = f;
        self.name.set_focused(f == Focus::Name);
        self.connect.set_focused(f == Focus::Connect);
        self.editor.set_focused(f == Focus::Editor);
        if let Some(w) = &self.window {
            w.set_ime_allowed(matches!(f, Focus::Name | Focus::Connect | Focus::Editor));
        }
    }

    /// 포커스 텍스트 박스(IME·편집 컨텍스트 라우팅).
    fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        match self.focus {
            Focus::Name => Some(&mut self.name),
            Focus::Connect => Some(&mut self.connect),
            Focus::Editor => Some(&mut self.editor),
            Focus::Grid => None,
        }
    }

    /// 이름 칸 + 접속 칸 → 프로필 저장(워커가 파싱·봉인·기록).
    fn do_save(&mut self) {
        let name = self.name.text().trim().to_string();
        let target = self.connect.text().trim().to_string();
        if !nsql_vault::is_profile_name(&name) {
            self.status = t(Msg::ErrProfileName).into();
            self.set_focus(Focus::Name);
            self.redraw();
            return;
        }
        if target.is_empty() || nsql_vault::is_profile_name(&target) {
            self.status = t(Msg::ErrNeedTarget).into();
            self.set_focus(Focus::Connect);
            self.redraw();
            return;
        }
        self.status = tf(Msg::StSaving, &[&name]);
        self.worker.send(worker::Cmd::Save { name, target });
        self.redraw();
    }

    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// `ui.theme` + OS 판정으로 팔레트를 다시 고르고 전체를 다시 그린다.
    fn apply_theme(&mut self) {
        let wt = self.window.as_ref().and_then(|w| w.theme());
        self.theme = theme::resolve(self.settings.theme_mode(), wt);
        if let Some(w) = &self.window {
            w.set_theme(theme::window_theme(self.settings.theme_mode()));
        }
        self.redraw();
    }

    /// Ctrl/⌘+⇧T — System → Light → Dark 순환 · 저장.
    fn cycle_theme(&mut self) {
        let next = self.settings.theme_mode().next();
        let _ = self.settings.set("ui.theme", next.as_str());
        self.persist_settings();
        self.apply_theme();
        self.status = tf(Msg::StThemeChanged, &[t(next.label())]);
    }

    /// Ctrl/⌘+⇧L — 언어 전환 · 저장 · 라벨 다시 만들기.
    fn toggle_lang(&mut self) {
        let next = current_lang().next();
        let _ = self.settings.set("ui.lang", next.code());
        self.persist_settings();
        nsql_i18n::set_lang(next);
        self.relabel();
        self.status = tf(Msg::StLangChanged, &[next.endonym()]);
        self.redraw();
    }

    fn persist_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.status = tf(Msg::CfgSaveFailed, &[&e.to_string()]);
        }
    }

    /// 언어가 바뀌면 컨트롤 문자열을 다시 만든다. TextBox는 placeholder 교체 API가 없어 본문을 보존해 재생성.
    fn relabel(&mut self) {
        self.save_btn.set_label(t(Msg::BtnSave));
        self.connect_btn.set_label(t(Msg::BtnConnect));
        self.run_btn.set_label(t(Msg::BtnRun));
        let (n, c, e) = (self.name.text(), self.connect.text(), self.editor.text());
        self.name = TextBox::new(t(Msg::PhProfileName)).with_text(&n);
        self.connect = TextBox::new(t(Msg::PhConnect)).with_text(&c);
        self.editor = TextBox::new(t(Msg::PhEditor))
            .with_multiline()
            .with_text(&e);
        self.layout();
        self.set_focus(self.focus);
    }

    fn run_sql(&mut self, all: bool) {
        if self.busy {
            self.status = t(Msg::StRunning).into();
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
            self.status = t(Msg::ErrNoSql).into();
            return;
        }
        self.busy = true;
        self.status = t(Msg::StRunning).into();
        self.log.clear();
        self.worker.send(worker::Cmd::Run(src));
        self.redraw();
    }

    fn do_connect(&mut self) {
        let target = self.connect.text();
        if target.trim().is_empty() {
            self.status = t(Msg::ErrEnterTarget).into();
            return;
        }
        self.busy = true;
        self.status = tf(Msg::StConnecting, &[&target]);
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
                    self.status = tf(
                        Msg::StRows,
                        &[
                            &rs.rows.len().to_string(),
                            &format!("{:.3}", elapsed.as_secs_f64()),
                        ],
                    );
                    self.grid.set_result(rs);
                }
                RunEvent::Done {
                    rows_affected,
                    elapsed,
                    ..
                } => {
                    let secs = format!("{:.3}", elapsed.as_secs_f64());
                    self.status = match rows_affected {
                        Some(n) => tf(Msg::StRowsAffected, &[&n.to_string(), &secs]),
                        None => tf(Msg::StOk, &[&secs]),
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
                    self.status = tf(Msg::StConnected, &[&description, &dialect.to_string()]);
                    self.busy = false;
                }
                RunEvent::Disconnected => {
                    self.status = t(Msg::StDisconnected).into();
                }
                RunEvent::Error { line, error, .. } => {
                    self.status = tf(Msg::StErrorLine, &[&line.to_string(), &error.to_string()]);
                    self.log.push(self.status.clone());
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
            let ui_px = self.settings.int("ui.font_size") as f32;
            let mono_px = self.settings.int("editor.font_size") as f32;
            // ── UI 층(한글 UI 본)
            {
                let prefs = FontPrefs {
                    base: SlotFont {
                        size: ui_px,
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
                self.name.paint(&mut dc, &th);
                self.connect.paint(&mut dc, &th);
                self.save_btn.paint(&mut dc, &th);
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
                let hint = t(Msg::StHint);
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
                        size: mono_px,
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
            if self.name.bounds().contains(p) {
                self.set_focus(Focus::Name);
            } else if self.connect.bounds().contains(p) {
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
            self.save_btn.on_event(&ev, &mut inv);
            self.connect_btn.on_event(&ev, &mut inv);
            self.run_btn.on_event(&ev, &mut inv);
            if self.save_btn.take_clicked() {
                self.do_save();
            }
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
            let enter = matches!(
                ev,
                InputEvent::Key {
                    key: CtlKey::Enter,
                    ..
                }
            );
            match self.focus {
                Focus::Name => {
                    if enter {
                        self.do_save();
                    } else {
                        self.name.on_event(&ev, &mut inv);
                    }
                }
                Focus::Connect => {
                    if enter {
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
            for tb in [&mut self.name, &mut self.connect, &mut self.editor] {
                if let Some(act) = tb.take_edit_ctx() {
                    self.status = tf(Msg::StClipboardLater, &[&format!("{act:?}")]);
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
            .with_theme(theme::window_theme(self.settings.theme_mode()))
            .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 720.0));
        let Ok(win) = el.create_window(attrs) else {
            eprintln!("{}", t(Msg::ErrNoWindow));
            el.exit();
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        // 창이 생기면 OS 판정(winit)이 정확해진다 — System 모드는 여기서 확정.
        self.theme = theme::resolve(self.settings.theme_mode(), win.theme());
        match softbuffer::Context::new(win.clone()) {
            Ok(ctx) => {
                match softbuffer::Surface::new(&ctx, win.clone()) {
                    Ok(s) => self.surface = Some(s),
                    Err(e) => eprintln!("softbuffer surface failed: {e}"),
                }
                self.ctx = Some(ctx);
            }
            Err(e) => eprintln!("softbuffer context failed: {e}"),
        }
        self.window = Some(win);
        self.layout();
        self.set_focus(Focus::Editor);
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, _ev: Wake) {
        self.drain_events();
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // 캐럿 깜빡임 — 0.5초 타이머가 **실제로 만료됐을 때만** 다시 그린다.
        // ★ 매 호출마다 request_redraw를 하면 그리기 → about_to_wait → 그리기의 무한 루프가 되어
        //   유휴 CPU 한 코어 100% · 키 입력이 프레임당 하나씩만 처리되는 지연(글자 14개에 3초 ·
        //   옛 결과가 화면에 남음)이 생긴다(09-13 Windows 실기 계측).
        let now = Instant::now();
        if now >= self.next_blink {
            self.next_blink = now + Duration::from_millis(500);
            if matches!(self.focus, Focus::Editor | Focus::Connect | Focus::Name) {
                self.redraw();
            }
        }
        el.set_control_flow(ControlFlow::WaitUntil(self.next_blink));
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
            WindowEvent::ThemeChanged(_) => {
                // OS 라이트/다크 전환 — System 모드일 때만 따라간다.
                if self.settings.theme_mode() == ThemeMode::System {
                    self.apply_theme();
                }
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
                if let Some(tb) = self.focused_textbox() {
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
                    Key::Character("t" | "T") if self.primary && self.shift => {
                        self.cycle_theme();
                        return;
                    }
                    Key::Character("l" | "L") if self.primary && self.shift => {
                        self.toggle_lang();
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
    // 설정(언어 · 테마 모드 · 글꼴 크기) — 폴더를 모르면 임시 경로의 기본값(저장은 실패해도 앱은 뜬다).
    let settings = Settings::open_default().unwrap_or_else(|_| {
        Settings::open(
            std::env::temp_dir()
                .join("nexa-sql")
                .join(nsql_settings::FILE_NAME),
        )
    });
    nsql_i18n::set_lang(settings.lang());
    // nexa-ctl 내장 메뉴(우클릭 편집) 라벨을 앱 i18n에 잇는다 — 미주입 기본은 영어.
    nexa_ctl::controls::set_ctl_labels(|m| {
        use nexa_ctl::controls::CtlMsg as C;
        match m {
            C::CtxSelectAll => t(Msg::CtxSelectAll),
            C::CtxCopy => t(Msg::CtxCopy),
            C::CtxCut => t(Msg::CtxCut),
            C::CtxPaste => t(Msg::CtxPaste),
        }
    });
    let ui = nexa_font::ui_font(None);
    let mono = nexa_font::mono_font(None);
    let (Some(ui), Some(mono)) = (ui, mono) else {
        eprintln!("{}", t(Msg::ErrNoFont));
        std::process::exit(1);
    };
    println!(
        "UI font: {} · mono: {} · Hangul {} · lang {} · theme {}",
        ui.chain.join(" → "),
        mono.chain.join(" → "),
        mono.font.covers('가'),
        settings.lang().code(),
        settings.theme_mode().as_str()
    );
    if args.first().map(String::as_str) == Some("--smoke") {
        println!("smoke ok — 드라이버: {:?}", nsql_drivers::available());
        return;
    }
    let Ok(el) = EventLoop::<Wake>::with_user_event().build() else {
        eprintln!("event loop creation failed");
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
    let profiles = worker::profile_names();
    let status = if profiles.is_empty() {
        t(Msg::StInitial).to_string()
    } else {
        tf(Msg::StProfiles, &[&profiles.join(", ")])
    };
    // 창이 없는 동안의 팔레트 — OS 조회(창이 생기면 winit 판정으로 다시 고른다).
    let initial_theme = theme::resolve(settings.theme_mode(), None);
    let mut app = App {
        window: None,
        ctx: None,
        surface: None,
        ui_font: ui.font,
        mono_font: mono.font,
        theme: initial_theme,
        settings,
        scale: 1.0,
        name: TextBox::new(t(Msg::PhProfileName)),
        connect: TextBox::new(t(Msg::PhConnect)).with_text(&initial_target),
        save_btn: Button::new(t(Msg::BtnSave)),
        connect_btn: Button::new(t(Msg::BtnConnect)),
        run_btn: Button::new(t(Msg::BtnRun)),
        editor: TextBox::new(t(Msg::PhEditor)).with_multiline(),
        grid: grid::Grid::default(),
        focus: Focus::Editor,
        worker,
        events,
        busy: false,
        status,
        log: Vec::new(),
        cursor: (0, 0),
        shift: false,
        primary: false,
        started: Instant::now(),
        next_blink: Instant::now(),
    };
    if let Err(e) = el.run_app(&mut app) {
        eprintln!("{}", tf(Msg::ErrEventLoop, &[&e.to_string()]));
        std::process::exit(1);
    }
}
