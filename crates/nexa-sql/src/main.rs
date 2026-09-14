//! Nexa SQL GUI — M2 최소 슬라이스(DR-18 · 사용자 "GUI 최소 기능을 병행").
//!
//! 창 하나: 왼쪽 **접속 패널**([`connect`] — DB 종류·호스트·포트·DB·사용자·비밀번호·프로필 · Test/Connect/Save · 상태) / 오른쪽 SQL 편집기(고정폭 · nexa-ctl TextBox 다중행 · IME) + 결과 그리드(자체 가상화) / 하단 상태줄.
//! 연결 프로필(T-16b): 접속 칸에 프로필 이름만 넣고 Connect · 이름 칸 + 접속 문자열 + Save로 저장(비밀번호는 `nsql-vault` 봉투 · CLI `nsql conn`과 같은 폴더).
//! 실행은 워커 스레드의 [`nsql_run::Runner`]가 하고, 결과는 채널 + `EventLoopProxy`로 UI에 온다(UI 스레드는 기다리지 않는다).
//! 폰트: UI = 한글 UI 본, 편집기·그리드 = 고정폭(D2Coding 우선) + 한글 폴백([docs/14](../../../docs/14-fonts-and-feature-modules.md)).
//!
//! 키: `Ctrl/⌘+Enter` = 선택 영역(없으면 전체) 실행 · `F5` = 전체 실행 · `Ctrl/⌘+L` = 접속 필드로 ·
//! `Ctrl/⌘+⇧T` = 테마 순환(System→Light→Dark) · `Ctrl/⌘+⇧L` = 언어 전환(en↔ko) — 둘 다 `settings.conf`에 저장(T-37/38).
//! 문자열은 전부 `nsql-i18n`(기본 영어) · 테마는 `ui.theme` + OS 판정([`theme`]) · 글꼴 크기는 `ui.font_size`/`editor.font_size`.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod clipboard;
mod colors_win;
mod conn_win;
mod connect;
mod editors;
mod grid;
mod icon;
mod input;
mod log_win;
mod palette;
mod probe;
mod syntax;
mod theme;
mod toolicons;
mod winfocus;
mod worker;

use colors_win::{ColorTarget, ColorsAction, ColorsWin};
use conn_win::{ConnWin, ConnWinAction, ConnectMark, TestMark};
use connect::{ConnState, ConnectPanel, PanelAction};
use editors::Editors;
use log_win::{LogWin, LogWinAction};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{
    Button, ComboItem, Control, EditCtxAction, InputEvent, Invalidations, Key as CtlKey, MenuBar,
    MenuDef, MenuEntry, TextBox, ToolItem, Toolbar, Widget,
};
use nexa_gfx::{Font, Surface};
use nsql_core::Dialect;
use nsql_i18n::{current_lang, t, tf, Msg};
use nsql_log::{LogEntry, LogKind};
use nsql_run::RunEvent;
use nsql_settings::{Settings, ThemeMode};
use nsql_vault::Vault;
use palette::{Palette, PaletteAction};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use syntax::SyntaxRegistry;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};
use worker::ConnOutcome;

/// UI 스레드를 깨우는 사용자 이벤트(워커가 보냄).
#[derive(Debug)]
struct Wake;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
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
    /// 로그 창(별도 창 · `Ctrl/⌘+⇧G`).
    log_win: LogWin,
    /// 메뉴에서 요청한 종료·로그 창 토글(이벤트 루프 핸들이 필요해 window_event 끝에서 처리).
    exit_requested: bool,
    palette: Palette,
    syntax: Rc<SyntaxRegistry>,
    /// 마지막 조회 결과(상태줄: 행 수 · 소요).
    last_rows: Option<usize>,
    last_secs: Option<f64>,
    /// 상태줄 구문 이름 영역(클릭 → 팔레트 `Set Syntax`).
    status_syntax_rect: Rect,
    /// 창 z-order(맨 뒤 → 맨 앞) — `window.focus = group`일 때 함께 올리는 순서.
    z_order: Vec<WindowId>,
    toggle_log: bool,
    /// 색 설정 창 열기 요청(메뉴/팔레트 → 다음 이벤트 루프 턴에 `el`로 연다).
    open_colors: bool,
    colors_win: ColorsWin,
    // 컨트롤
    menubar: MenuBar,
    toolbar: Toolbar,
    /// 접속 창(별도 창 · 폼 + 로그인 목록). 폼 상태의 단일 원천 = `conn_win.panel`.
    conn_win: ConnWin,
    /// 메뉴/툴바에서 접속 창 열기 요청(창 생성은 이벤트 루프 핸들에서).
    open_conn: bool,
    run_btn: Button,
    editors: Editors,
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
    /// 접속 패널 작업(진행/결과) — 한 번에 하나 · 프로필 이름에 묶임(사용자 09-14).
    panel_op: Option<(String, ConnState)>,
}

fn px(v: f32, s: f32) -> i32 {
    (v * s).round() as i32
}

impl App {
    fn ed_mut(&mut self) -> &mut TextBox {
        self.editors.cur_mut()
    }

    fn layout(&mut self) {
        let Some(win) = &self.window else { return };
        let size = win.inner_size();
        let (w, h) = (size.width as i32, size.height as i32);
        let s = self.scale;
        let mut inv = Invalidations::default();
        let pad = px(8.0, s);
        let status_h = px(24.0, s);
        let panel_w = px(300.0, s);
        let btn_w = px(84.0, s);
        // 메뉴바 · 툴바(창 전폭)
        let menu_h = px(26.0, s);
        self.menubar.set_scale(s);
        self.toolbar.set_scale(s);
        self.menubar
            .set_bounds(Rect::new(0, 0, w, menu_h), &mut inv);
        let tool_h = self.toolbar.preferred_height();
        self.toolbar
            .set_bounds(Rect::new(0, menu_h, w, tool_h), &mut inv);
        let chrome_h = menu_h + tool_h;
        let top_h = px(36.0, s);
        // 접속 패널은 별도 창(conn_win) — 본문은 창 전폭.
        let _ = panel_w;
        let rx = pad;
        let rw = w - rx - pad;
        self.run_btn.set_bounds(
            Rect::new(
                w - pad - btn_w,
                chrome_h + px(6.0, s),
                btn_w,
                top_h - px(12.0, s),
            ),
            &mut inv,
        );
        let body_top = chrome_h + top_h;
        let body_h = h - body_top - status_h;
        let _ = chrome_h;
        let editor_h = (body_h as f32 * 0.5) as i32;
        self.editors
            .set_bounds(Rect::new(rx, body_top, rw, editor_h - pad), s);
        self.grid.set_bounds(Rect::new(
            rx,
            body_top + editor_h,
            rw,
            body_h - editor_h - pad,
        ));
        self.run_btn.set_scale(s);
        self.palette.set_bounds(w, chrome_h, s);
    }

    fn set_focus(&mut self, f: Focus) {
        self.focus = f;
        self.ed_mut().set_focused(f == Focus::Editor);
        if let Some(w) = &self.window {
            w.set_ime_allowed(f == Focus::Editor);
        }
    }

    /// 포커스 텍스트 박스(IME·편집 컨텍스트 라우팅).
    fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        match self.focus {
            Focus::Editor => Some(self.editors.cur_mut()),
            Focus::Grid => None,
        }
    }

    /// 접속 패널의 요청 → 워커/저장소.
    fn handle_panel_action(&mut self, a: PanelAction) {
        match a {
            PanelAction::Connect(spec) => {
                self.busy = true;
                self.status = tf(Msg::StConnecting, &[&spec.redacted()]);
                let name = self.conn_win.panel.profile_name();
                self.conn_win
                    .set_connect_mark(&name, Some(ConnectMark::Connecting));
                self.panel_op = Some((name, ConnState::Connecting));
                // 같은 서버면 기존 세션 유지 · 설정 `connect.reconnect_same`이면 닫고 다시 접속(사용자 09-14).
                let reconnect_same = self.settings.flag("connect.reconnect_same");
                self.worker.send(worker::Cmd::ConnectSpec {
                    spec,
                    reconnect_same,
                });
            }
            PanelAction::Test(spec) => {
                self.busy = true;
                self.status = t(Msg::StTesting).into();
                let name = self.conn_win.panel.profile_name();
                self.conn_win.set_test_mark(&name, TestMark::Testing);
                self.panel_op = Some((name, ConnState::Testing));
                self.worker.send(worker::Cmd::Test(spec));
            }
            PanelAction::Disconnect => {
                self.busy = true;
                self.worker.send(worker::Cmd::Disconnect);
            }
            PanelAction::Save { name, spec } => {
                self.status = tf(Msg::StSaving, &[&name]);
                self.worker.send(worker::Cmd::SaveSpec { name, spec });
            }
            PanelAction::Edit(_) => {} // 접속 창이 자체 처리(클립보드)
            PanelAction::LoadProfile(name) => {
                match Vault::open_default().and_then(|v| v.get(&name)) {
                    Ok(Some(spec)) => {
                        self.conn_win.panel.fill(&name, &spec);
                        // 상태는 한 번에 하나(사용자 09-14): 진행/결과가 이 프로필 것이면 복원, 아니면 Idle.
                        let st = match &self.panel_op {
                            Some((n, st)) if *n == name => st.clone(),
                            _ => ConnState::Idle,
                        };
                        self.conn_win.panel.set_state(st);
                    }
                    Ok(None) => {}
                    Err(e) => self
                        .conn_win
                        .panel
                        .set_state(ConnState::Failed(e.to_string())),
                }
            }
        }
        self.redraw();
    }

    /// 진행 중/마지막 패널 작업의 프로필 이름(없으면 지금 패널의 이름).
    fn panel_op_name(&self) -> String {
        self.panel_op
            .as_ref()
            .map_or_else(|| self.conn_win.panel.profile_name(), |(n, _)| n.clone())
    }

    /// 패널 작업 결과를 기록하고, 지금 패널에 그 프로필이 떠 있을 때만 상태줄에 보인다(한 번에 상태 하나 · 사용자 09-14).
    fn set_panel_result(&mut self, name: &str, st: ConnState) {
        if self.conn_win.panel.profile_name() == name {
            self.conn_win.panel.set_state(st.clone());
        }
        self.panel_op = Some((name.to_string(), st));
    }

    fn drain_conn(&mut self) -> bool {
        let mut changed = false;
        while let Ok(o) = self.worker.conn.try_recv() {
            changed = true;
            match o {
                ConnOutcome::Connected(d) => {
                    let name = self.panel_op_name();
                    self.conn_win.mark_connected(&name);
                    // 접속 버튼 초록 = 지금 접속된 프로필 하나만 → 잠시 보여 준 뒤 창 닫힘(사용자 09-14).
                    self.conn_win.clear_connect_marks();
                    self.conn_win
                        .set_connect_mark(&name, Some(ConnectMark::Connected));
                    self.conn_win.close_soon(Duration::from_millis(450));
                    // 활성 탭에 접속 정보 적용 — 탭이 없으면 새 탭(사용자 09-14).
                    self.editors.ensure_tab();
                    self.editors.set_conn_desc(d.clone());
                    self.set_panel_result(&name, ConnState::Connected(d));
                }
                ConnOutcome::ConnectFailed(e) => {
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("connect: {e}")));
                    self.status = tf(Msg::StConnectFailed, &[&e]);
                    let name = self.panel_op_name();
                    self.conn_win.set_connect_mark(&name, None);
                    self.set_panel_result(&name, ConnState::Failed(e));
                    // 접속 실패 확인 → 그 서버 신호등 즉시 갱신(사용자 09-14).
                    self.conn_win.note_failure(&name);
                }
                ConnOutcome::TestOk {
                    description,
                    elapsed_s,
                } => {
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        format!("test ok: {description} ({elapsed_s}s)"),
                    ));
                    let msg = tf(Msg::StTestOk, &[&description, &elapsed_s]);
                    self.status = msg.clone();
                    let name = self.panel_op_name();
                    self.conn_win.set_test_mark(&name, TestMark::Ok);
                    self.set_panel_result(&name, ConnState::TestOk(msg));
                }
                ConnOutcome::TestFailed(e) => {
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("test: {e}")));
                    self.status = tf(Msg::StTestFailed, &[&e]);
                    let name = self.panel_op_name();
                    self.conn_win.set_test_mark(&name, TestMark::Failed);
                    self.set_panel_result(&name, ConnState::Failed(e));
                    self.conn_win.note_failure(&name);
                }
                ConnOutcome::Disconnected => {
                    self.editors.set_conn_desc("");
                    self.conn_win.clear_connect_marks();
                    self.conn_win.clear_active();
                    self.panel_op = None;
                    self.conn_win.panel.set_state(ConnState::Idle);
                }
                ConnOutcome::Saved(name) => {
                    self.status = tf(Msg::WkProfileSaved, &[&name, ""]);
                    self.conn_win.refresh_profiles(Some(&name));
                }
                ConnOutcome::SaveFailed(e) => {
                    self.status = tf(Msg::WkProfileSaveFailed, &[&e]);
                    self.conn_win.panel.set_state(ConnState::Failed(e));
                }
            }
        }
        changed
    }

    fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// 메뉴·툴바 액션(id = 메뉴 항목 값 · 툴바 항목 id — 같은 어휘).
    fn menu_action(&mut self, id: &str) {
        match id {
            "file.new" => {
                self.editors.new_tab(None);
                self.set_focus(Focus::Editor);
            }
            "file.exit" => self.exit_requested = true,
            "edit.cut" => self.clip_action(EditCtxAction::Cut),
            "edit.copy" => self.clip_action(EditCtxAction::Copy),
            "edit.paste" => self.clip_action(EditCtxAction::Paste),
            "edit.select_all" => self.route(InputEvent::SelectAll),
            "view.log" => self.toggle_log = true,
            "view.colors" => self.open_colors = true,
            "view.theme" => self.cycle_theme(),
            "view.lang" => self.toggle_lang(),
            "view.palette" => self.open_palette(""),
            id if id.starts_with("syntax.set:") => {
                let name = &id["syntax.set:".len()..];
                if self.editors.set_syntax(name) {
                    self.status = tf(Msg::StSyntaxSet, &[name]);
                }
            }
            "run.statement" => self.run_sql(false),
            "run.all" => self.run_sql(true),
            // 접속 창 열기 — 연결 중이어도 끊지 않고 그냥 연다(사용자 09-14). 끊기는 폼의 Disconnect 버튼.
            "conn.toggle" => self.open_conn = true,
            "help.about" => {
                self.status = format!(
                    "Nexa SQL {} · SosomLab · PolyForm NC 1.0.0",
                    env!("CARGO_PKG_VERSION")
                );
            }
            _ => {}
        }
        self.redraw();
    }

    fn build_menus() -> Vec<MenuDef> {
        let item = |id: &str, m: Msg| MenuEntry::Item(ComboItem::new(id, t(m)));
        vec![
            MenuDef::new(
                t(Msg::MnFile),
                vec![
                    item("file.new", Msg::MnNew),
                    MenuEntry::Separator,
                    item("file.exit", Msg::MnExit),
                ],
            ),
            MenuDef::new(
                t(Msg::MnEdit),
                vec![
                    item("edit.cut", Msg::MnCut),
                    item("edit.copy", Msg::MnCopy),
                    item("edit.paste", Msg::MnPaste),
                    MenuEntry::Separator,
                    item("edit.select_all", Msg::MnSelectAll),
                ],
            ),
            MenuDef::new(
                t(Msg::MnView),
                vec![
                    item("view.palette", Msg::MnCommandPalette),
                    item("view.log", Msg::MnLogWindow),
                    MenuEntry::Separator,
                    item("view.colors", Msg::MnColors),
                    item("view.theme", Msg::MnTheme),
                    item("view.lang", Msg::MnLanguage),
                ],
            ),
            MenuDef::new(
                t(Msg::MnRun),
                vec![
                    item("run.statement", Msg::MnRunStatement),
                    item("run.all", Msg::MnRunAll),
                    MenuEntry::Separator,
                    item("conn.toggle", Msg::MnConnect),
                ],
            ),
            MenuDef::new(t(Msg::MnHelp), vec![item("help.about", Msg::MnAbout)]),
        ]
    }

    fn build_toolbar() -> Toolbar {
        let mut tb = Toolbar::new(vec![
            // 아이콘은 글꼴 글리프가 아니라 코드로 그린 마스크(`toolicons.rs` · 사용자 09-14).
            ToolItem::new("file.new", toolicons::new_script()).tip(t(Msg::TipNew)),
            ToolItem::new("run.statement", toolicons::run_statement()).tip(t(Msg::TipRunStatement)),
            ToolItem::new("run.all", toolicons::run_all()).tip(t(Msg::TipRunAll)),
            ToolItem::new("conn.toggle", toolicons::connect()).tip(t(Msg::TipConnect)),
            ToolItem::new("view.log", toolicons::log())
                .tip(t(Msg::TipLog))
                .align_right(),
        ]);
        tb.set_icon_size(18);
        tb
    }

    /// 복사·잘라내기·붙여넣기·전체 선택 — 포커스 텍스트박스 ↔ OS 클립보드([`clipboard`]). 실패는 상태줄에.
    fn clip_action(&mut self, act: EditCtxAction) {
        let mut inv = Invalidations::default();
        let mut failed = false;
        match act {
            EditCtxAction::Copy => {
                if let Some(text) = self.focused_textbox().and_then(|tb| tb.copy_selection()) {
                    let rich =
                        self.focus == Focus::Editor && self.settings.flag("editor.copy_rich");
                    let hl = self.editors.cur().highlighter().cloned();
                    failed = match (rich, hl) {
                        (true, Some(h)) => {
                            let px = self.settings.int("editor.font_size") as i32;
                            let html = nexa_ctl::to_html(
                                &text,
                                h.as_ref(),
                                &self.theme,
                                "Consolas, 'Cascadia Mono', 'D2Coding', Menlo",
                                px,
                            );
                            !clipboard::write_rich(&text, &html)
                        }
                        _ => !clipboard::write_text(&text),
                    };
                }
            }
            EditCtxAction::Cut => {
                if let Some(text) = self
                    .focused_textbox()
                    .and_then(|tb| tb.cut_selection(&mut inv))
                {
                    failed = !clipboard::write_text(&text);
                }
            }
            EditCtxAction::Paste => match clipboard::read_text() {
                Some(text) => {
                    if let Some(tb) = self.focused_textbox() {
                        tb.paste(&text, &mut inv);
                    }
                }
                None => failed = true,
            },
        }
        if failed {
            self.status = t(Msg::ErrClipboard).into();
        }
        self.redraw();
    }

    /// 명령 팔레트 열기(prefill = 초기 질의 · 예 "Set Syntax: ").
    fn open_palette(&mut self, prefill: &str) {
        let mut cmds: Vec<(String, String)> = Vec::new();
        let m =
            |id: &str, menu: Msg, item: Msg| (id.to_string(), format!("{}: {}", t(menu), t(item)));
        cmds.push(m("file.new", Msg::MnFile, Msg::MnNew));
        cmds.push(m("file.exit", Msg::MnFile, Msg::MnExit));
        cmds.push(m("edit.cut", Msg::MnEdit, Msg::MnCut));
        cmds.push(m("edit.copy", Msg::MnEdit, Msg::MnCopy));
        cmds.push(m("edit.paste", Msg::MnEdit, Msg::MnPaste));
        cmds.push(m("edit.select_all", Msg::MnEdit, Msg::MnSelectAll));
        cmds.push(m("view.log", Msg::MnView, Msg::MnLogWindow));
        cmds.push(m("view.colors", Msg::MnView, Msg::MnColors));
        cmds.push(m("view.theme", Msg::MnView, Msg::MnTheme));
        cmds.push(m("view.lang", Msg::MnView, Msg::MnLanguage));
        cmds.push(m("run.statement", Msg::MnRun, Msg::MnRunStatement));
        cmds.push(m("run.all", Msg::MnRun, Msg::MnRunAll));
        cmds.push(m("conn.toggle", Msg::MnRun, Msg::MnConnect));
        cmds.push(m("help.about", Msg::MnHelp, Msg::MnAbout));
        for name in self.syntax.names() {
            cmds.push((
                format!("syntax.set:{name}"),
                format!("{}: {name}", t(Msg::PalSetSyntax)),
            ));
        }
        self.palette.set_commands(cmds);
        self.palette.open(prefill);
        self.redraw();
    }

    /// 창이 포커스를 받았다 — z-order 갱신 · `window.focus = group`이면 나머지 창을 활성화 없이 같이 올린다.
    fn on_window_focused(&mut self, id: WindowId) {
        self.z_order.retain(|w| *w != id);
        self.z_order.push(id);
        if self.settings.get("window.focus") == Some("single") {
            return;
        }
        let mut wins: Vec<&Window> = Vec::new();
        for wid in &self.z_order {
            if let Some(w) = self.window.as_deref().filter(|w| w.id() == *wid) {
                wins.push(w);
            } else if let Some(w) = self.log_win.window().filter(|w| w.id() == *wid) {
                wins.push(w);
            } else if let Some(w) = self.conn_win.window().filter(|w| w.id() == *wid) {
                wins.push(w);
            }
        }
        winfocus::raise_group(&wins);
    }

    /// 접속 창 열기(메인 창 위 가운데) — 이미 열려 있으면 앞으로.
    fn open_conn_window(&mut self, el: &ActiveEventLoop) {
        let over = self.window.as_ref().and_then(|w| {
            let p = w.outer_position().ok()?;
            let sz = w.outer_size();
            Some((p.x, p.y, sz.width, sz.height))
        });
        let owner = self.window.clone();
        self.conn_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            over,
            owner.as_deref(),
        );
    }

    /// 로그인 목록 더블클릭/Enter — 저장소에서 읽어 폼에 채우고 바로 접속.
    /// 행 테스트 버튼 — 폼을 건드리지 않고 저장소 스펙으로 접속만 해 본다. 결과는 행 버튼 표시로.
    fn test_profile(&mut self, name: &str) {
        if self.busy {
            self.status = t(Msg::StRunning).into();
            return;
        }
        match Vault::open_default().and_then(|v| v.get(name)) {
            Ok(Some(spec)) => {
                self.busy = true;
                self.status = t(Msg::StTesting).into();
                self.conn_win.set_test_mark(name, TestMark::Testing);
                self.panel_op = Some((name.to_string(), ConnState::Testing));
                if self.conn_win.panel.profile_name() == name {
                    self.conn_win.panel.set_state(ConnState::Testing);
                }
                self.worker.send(worker::Cmd::Test(spec));
            }
            Ok(None) => self.status = tf(Msg::StTestFailed, &[name]),
            Err(e) => self.status = tf(Msg::StTestFailed, &[&e.to_string()]),
        }
        self.conn_win.redraw();
    }

    fn login_profile(&mut self, name: &str) {
        if self.busy {
            self.status = t(Msg::StRunning).into();
            return;
        }
        match Vault::open_default().and_then(|v| v.get(name)) {
            Ok(Some(spec)) => {
                self.conn_win.panel.fill(name, &spec);
                self.conn_win.panel.set_state(ConnState::Idle);
                if let Some(a) = self.conn_win.panel.connect_action() {
                    self.handle_panel_action(a);
                }
            }
            Ok(None) => self
                .conn_win
                .panel
                .set_state(ConnState::Failed(name.to_string())),
            Err(e) => self
                .conn_win
                .panel
                .set_state(ConnState::Failed(e.to_string())),
        }
        self.conn_win.redraw();
    }

    /// 우클릭 Duplicate — `<이름>_Copied`(있으면 `_Copied2`…)로 저장(비밀번호 봉투 포함).
    fn duplicate_profile(&mut self, name: &str) {
        let r = Vault::open_default().and_then(|v| {
            let Some(spec) = v.get(name)? else {
                return Ok(None);
            };
            let names: Vec<String> = v.list()?.into_iter().map(|p| p.name).collect();
            let mut new = format!("{name}_Copied");
            let mut n = 2;
            while names.contains(&new) {
                new = format!("{name}_Copied{n}");
                n += 1;
            }
            if !nsql_vault::is_profile_name(&new) {
                return Ok(None);
            }
            v.save(&new, &spec)?;
            Ok(Some(new))
        });
        match r {
            Ok(Some(new)) => {
                self.status = tf(Msg::WkProfileSaved, &[&new, ""]);
                self.conn_win.refresh_profiles(Some(&new));
            }
            Ok(None) => self.status = t(Msg::ErrProfileName).into(),
            Err(e) => self.status = e.to_string(),
        }
        self.conn_win.redraw();
    }

    fn delete_profile(&mut self, name: &str) {
        match Vault::open_default().and_then(|v| v.remove(name)) {
            Ok(_) => self.status = tf(Msg::StProfileDeleted, &[name]),
            Err(e) => self.status = e.to_string(),
        }
        self.conn_win.refresh_profiles(None);
        self.redraw();
    }

    fn toggle_log_window(&mut self, el: &ActiveEventLoop) {
        if self.log_win.is_open() {
            self.log_win.close();
        } else {
            let near = self.window.as_ref().and_then(|w| {
                w.outer_position()
                    .ok()
                    .map(|p| (p.x, p.y, w.outer_size().width))
            });
            let owner = self.window.clone();
            self.log_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                near,
                owner.as_deref(),
            );
        }
    }

    /// `ui.theme` + OS 판정으로 팔레트를 다시 고르고 전체를 다시 그린다.
    fn apply_theme(&mut self) {
        let wt = self.window.as_ref().and_then(|w| w.theme());
        self.theme = theme::resolve(self.settings.theme_mode(), wt);
        self.log_win.redraw();
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
        self.menubar.set_menus(App::build_menus());
        self.toolbar = App::build_toolbar();
        self.run_btn.set_label(t(Msg::BtnRun));
        self.conn_win.relabel();
        self.editors.rebuild_boxes();
        self.layout();
        self.set_focus(self.focus);
    }

    fn run_sql(&mut self, all: bool) {
        if self.busy {
            self.status = t(Msg::StRunning).into();
            return;
        }
        // Ctrl/⌘+Enter = 선택 영역 → 없으면 **캐럿 위치의 한 문장**(`;` 종결 · 사용자 09-14) → F5 = 전체.
        let text = if all {
            None
        } else {
            self.ed_mut()
                .copy_selection()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| {
                    let full = self.ed_mut().text();
                    let byte_pos = full
                        .char_indices()
                        .nth(self.ed_mut().caret())
                        .map_or(full.len(), |(b, _)| b);
                    nsql_script::statement_at(&full, byte_pos).map(|it| it.text)
                })
        };
        let src = text.unwrap_or_else(|| self.ed_mut().text());
        if src.trim().is_empty() {
            self.status = t(Msg::ErrNoSql).into();
            return;
        }
        self.busy = true;
        self.status = t(Msg::StRunning).into();
        self.log.clear();
        // 신호등이 초록이 아닌 서버(빨강·파랑·확인 중·모름)에는 실행 전 빠른 포트 판정을 건다(사용자 09-14).
        let pol = *self.conn_win.policy();
        let light = self.conn_win.status_of(self.conn_win.active_name());
        let preflight =
            (pol.enabled && light != Some(probe::ProbeStatus::Up)).then_some(pol.timeout);
        self.worker.send(worker::Cmd::Run { src, preflight });
        self.redraw();
    }

    fn drain_events(&mut self) {
        let mut changed = self.drain_conn();
        self.conn_win.drain_probes();
        while let Ok(ev) = self.events.try_recv() {
            changed = true;
            for e in nsql_run::log_entries(&ev) {
                self.log_win.push(e);
            }
            match ev {
                RunEvent::Begin { .. } => {}
                RunEvent::ResultSet { rs, elapsed, .. } => {
                    self.last_rows = Some(rs.rows.len());
                    self.last_secs = Some(elapsed.as_secs_f64());
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
                RunEvent::Timing { timeline, .. } => {
                    // 상태줄 = 결과 요약 + 단계별 소요(docs/26). 렌더 시간은 그리드 푸터가 자체 표시.
                    self.status = format!("{} · ⏱ {}", self.status, timeline.summary());
                }
                RunEvent::Error { line, error, .. } => {
                    self.status = tf(Msg::StErrorLine, &[&line.to_string(), &error.to_string()]);
                    self.log.push(self.status.clone());
                    self.grid.set_messages(self.log.clone());
                    self.busy = false;
                    // 실행 중 접속성 오류 확인 → 활성 서버 신호등 즉시 갱신(사용자 09-14).
                    if probe::is_connection_error(error.code, &error.message) {
                        let name = self.conn_win.active_name().to_string();
                        self.conn_win.note_failure(&name);
                    }
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
                let chrome_top = self.toolbar.bounds().bottom();
                dc.fill_rect(Rect::new(0, chrome_top, wi, px(36.0, s)), th.chrome_bg);
                dc.fill_rect(Rect::new(0, chrome_top + px(36.0, s) - 1, wi, 1), th.border);
                self.run_btn.paint(&mut dc, &th);
                self.editors.paint_tabs(&mut dc, &th);
                // 메뉴바·툴바(창 전폭) — 메뉴 드롭다운은 최상위라 맨 뒤에.
                dc.fill_rect(self.toolbar.bounds(), th.chrome_bg);
                self.toolbar.paint(&mut dc, &th);
                self.toolbar.paint_tooltip(&mut dc, &th);
                dc.fill_rect(self.menubar.bounds(), th.chrome_bg);
                dc.fill_rect(
                    Rect::new(0, self.toolbar.bounds().bottom() - 1, wi, 1),
                    th.border,
                );
                self.menubar.paint(&mut dc, &th);
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
                // 오른쪽 세그먼트(Sublime/DBeaver/Golden 참고 · docs/29 §4): 접속 · Ln,Col · rows · time · 구문(클릭 = Set Syntax)
                let (ln, col) = self.editors.caret_line_col();
                let mut segs: Vec<(String, bool)> = Vec::new();
                let conn = match self.conn_win.panel.state_ref() {
                    ConnState::Connected(d) => d.clone(),
                    _ => "—".to_string(),
                };
                segs.push((conn, false));
                segs.push((tf(Msg::StPos, &[&ln.to_string(), &col.to_string()]), false));
                if let Some(n) = self.last_rows {
                    segs.push((tf(Msg::StRowsShort, &[&n.to_string()]), false));
                }
                if let Some(secs) = self.last_secs {
                    segs.push((format!("{secs:.3}s"), false));
                }
                segs.push((self.editors.syntax_name(), true));
                let gap = px(12.0, s);
                let mut xr = wi - px(8.0, s);
                self.status_syntax_rect = Rect::new(0, 0, 0, 0);
                for (text, is_syntax) in segs.iter().rev() {
                    let tw = dc.text_width(text);
                    xr -= tw;
                    let r = Rect::new(xr - gap / 2, sy, tw + gap, px(24.0, s));
                    dc.text(
                        xr,
                        sy + px(5.0, s),
                        r,
                        text,
                        if *is_syntax { th.text } else { th.text_dim },
                    );
                    if *is_syntax {
                        self.status_syntax_rect = r;
                    }
                    xr -= gap;
                    dc.fill_rect(
                        Rect::new(xr + gap / 2, sy + px(5.0, s), 1, px(14.0, s)),
                        th.border,
                    );
                }
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
                self.editors.cur_mut().paint(&mut dc, &th);
                self.grid.paint(&mut dc, &th, s);
            }
            // ── 최상위 카드(탭 툴팁 · UI 글꼴)
            {
                let prefs = FontPrefs {
                    base: SlotFont {
                        size: ui_px,
                        bold: false,
                        italic: false,
                    },
                    ..FontPrefs::default()
                };
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s).with_fonts(prefs);
                self.palette.paint(&mut dc, &th);
                self.editors.paint_tooltip(&mut dc, &th, wi);
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
            // 휠: 가로 성분(틸트 휠·트랙패드)이 있으면 HWheel · Shift+세로 휠 = 가로(관례) · 아니면 세로.
            // 휠 변환은 한 곳(`input::wheel_event` · 방향 반전 설정 포함).
            WindowEvent::MouseWheel { delta, .. } => crate::input::wheel_event(delta, self.shift),
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
        // 우클릭 메뉴가 열리기 전에 "붙여넣기 가능" 여부를 넣어 준다.
        if matches!(ev, InputEvent::RightDown { .. }) {
            let has = clipboard::read_text().is_some_and(|s| !s.is_empty());
            if let Some(tb) = self.focused_textbox() {
                tb.set_clipboard_has_text(has);
            }
        }
        let is_mouse = matches!(
            ev,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
        );
        // 열린 명령 팔레트는 모달.
        if self.palette.is_open() {
            match self.palette.on_event(&ev, &mut inv) {
                PaletteAction::None => {}
                PaletteAction::Close => self.palette.close(),
                PaletteAction::Pick(id) => {
                    self.palette.close();
                    self.menu_action(&id);
                }
            }
            self.redraw();
            return;
        }
        // 상태줄 구문 이름 클릭 → 팔레트(Set Syntax).
        if let InputEvent::MouseDown { x, y, .. } = ev {
            if self.status_syntax_rect.contains(Point { x, y }) {
                let prefill = format!("{}: ", t(Msg::PalSetSyntax));
                self.open_palette(&prefill);
                return;
            }
        }
        // 열린 메뉴는 모달 — 어디를 눌러도 메뉴바가 먼저 받는다.
        if self.menubar.is_open() {
            self.menubar.on_event(&ev, &mut inv);
            if let Some(id) = self.menubar.take_picked() {
                self.menu_action(&id);
            }
            self.redraw();
            return;
        }
        if is_mouse {
            self.menubar.on_event(&ev, &mut inv);
            if let Some(id) = self.menubar.take_picked() {
                self.menu_action(&id);
            }
            self.toolbar.on_event(&ev, &mut inv);
            if let Some(id) = self.toolbar.take_clicked() {
                self.menu_action(&id);
            }
            if self.menubar.is_open() {
                self.redraw();
                return;
            }
        }
        // 편집기 탭 바(클릭·드래그·휠 · 툴팁 호버).
        if self.editors.route_tabs(&ev, &mut inv) {
            self.set_focus(Focus::Editor);
            self.redraw();
            return;
        }
        if let InputEvent::MouseDown { x, y, .. } = ev {
            let p = Point { x, y };
            if self.editors.editor_bounds().contains(p) {
                self.set_focus(Focus::Editor);
            } else if self.grid.bounds.contains(p) {
                self.set_focus(Focus::Grid);
            }
        }
        // 스크롤바 호버·드래그: 포커스와 무관하게 **커서 아래** 편집기/그리드가 마우스 사건을 받는다.
        if is_mouse {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            if self.grid.bounds.contains(cur) && self.focus != Focus::Grid {
                self.grid.on_event(&ev, self.scale);
                if self.grid.bars_visible() {
                    inv.push(self.grid.bounds);
                }
            }
            if self.editors.editor_bounds().contains(cur)
                && self.focus != Focus::Editor
                && matches!(ev, InputEvent::MouseMove { .. })
            {
                self.ed_mut().on_event(&ev, &mut inv);
            }
        }
        // 버튼·패널은 항상 마우스 사건을 받는다.
        if is_mouse {
            self.run_btn.on_event(&ev, &mut inv);
            if self.run_btn.take_clicked() {
                self.run_sql(true);
            }
        }
        // 휠은 포커스가 아니라 **커서 아래 영역**으로 간다(편집기·그리드·패널).
        let is_wheel = matches!(ev, InputEvent::Wheel { .. } | InputEvent::HWheel { .. });
        let cur = Point {
            x: self.cursor.0,
            y: self.cursor.1,
        };
        if is_wheel && self.grid.bounds.contains(cur) {
            self.grid.on_event(&ev, self.scale);
            inv.push(self.grid.bounds);
        } else if is_wheel && self.editors.editor_bounds().contains(cur) {
            self.ed_mut().on_event(&ev, &mut inv);
        } else {
            let enter = matches!(
                ev,
                InputEvent::Key {
                    key: CtlKey::Enter,
                    ..
                }
            );
            let _ = enter;
            match self.focus {
                Focus::Editor => self.ed_mut().on_event(&ev, &mut inv),
                Focus::Grid => {
                    self.grid.on_event(&ev, self.scale);
                    inv.push(self.grid.bounds);
                }
            }
            // 편집 컨텍스트 요청(우클릭 메뉴 복사·붙여넣기) — 호스트가 OS 클립보드를 잇는다.
            let pending = self.ed_mut().take_edit_ctx();
            if let Some(act) = pending {
                self.clip_action(act);
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
        let attrs = icon::with_icon(
            Window::default_attributes()
                .with_title("Nexa SQL")
                .with_theme(theme::window_theme(self.settings.theme_mode()))
                .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 720.0)),
        );
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
        let near = win
            .outer_position()
            .ok()
            .map(|p| (p.x, p.y, win.outer_size().width));
        self.window = Some(win);
        self.layout();
        self.set_focus(Focus::Editor);
        // 로그 창은 메인 창 오른쪽에 함께 연다(사용자 09-14 "별도 창") — 메인의 소유 창.
        let owner = self.window.clone();
        self.log_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
        );
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
            if self.focus == Focus::Editor {
                self.redraw();
            }
        }
        // 오버레이 스크롤바 페이드(편집기·그리드·로그 창) — 보이는 동안만 ≈30ms 타이머.
        let now_ms = self.started.elapsed().as_millis() as u64;
        let mut redraw = self.ed_mut().tick(now_ms);
        redraw |= self.grid.tick(now_ms);
        redraw |= self.editors.tick();
        if self.editors.relayout_if_needed() {
            redraw = true;
        }
        if redraw {
            self.redraw();
        }
        if self.log_win.tick(now_ms) {
            self.log_win.redraw();
        }
        if self.colors_win.tick(now_ms) {
            self.colors_win.redraw();
        }
        if self.conn_win.tick_bars(now_ms) {
            self.conn_win.redraw();
        }
        let bars_live = self.ed_mut().scrollbars_visible()
            || self.grid.bars_visible()
            || self.log_win.bars_visible()
            || self.conn_win.bars_visible()
            || self.conn_win.tooltip_pending()
            || self.grid.hover_animating()
            || self.conn_win.hover_animating()
            || self.colors_win.animating()
            || self.editors.tooltip_pending();
        let mut next = if bars_live {
            self.next_blink.min(now + Duration::from_millis(33))
        } else {
            self.next_blink
        };
        // 서버 신호등 재시도 예약(접속 창이 열려 있을 때만).
        if let Some(t) = self.conn_win.tick(now) {
            next = next.min(t);
        }
        el.set_control_flow(ControlFlow::WaitUntil(next));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::Focused(true)) {
            self.on_window_focused(id);
        }
        if self.conn_win.is(id) {
            let ui_px = self.settings.int("ui.font_size") as f32;
            for a in self.conn_win.handle(&event) {
                match a {
                    ConnWinAction::Paint => {
                        self.conn_win.paint(&self.ui_font, &self.theme, ui_px);
                    }
                    ConnWinAction::Panel(a) => self.handle_panel_action(a),
                    ConnWinAction::Login(name) => self.login_profile(&name),
                    ConnWinAction::TestProfile(name) => self.test_profile(&name),
                    ConnWinAction::Delete(name) => self.delete_profile(&name),
                    ConnWinAction::Duplicate(name) => self.duplicate_profile(&name),
                }
            }
            return;
        }
        if self.colors_win.is(id) {
            let ui_px = self.settings.int("ui.font_size") as f32;
            match self.colors_win.handle(&event, &self.theme) {
                ColorsAction::Paint => self.colors_win.paint(&self.ui_font, &self.theme, ui_px),
                ColorsAction::Changed { target, hex } => {
                    apply_color(target, Some(&hex));
                    let _ = self.settings.set(target.key(), &hex);
                    let recent = self
                        .colors_win
                        .recent()
                        .iter()
                        .map(|c| format!("#{c:08X}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    let _ = self.settings.set("ui.color_recent", &recent);
                    let _ = self.settings.save();
                    self.redraw();
                    self.conn_win.redraw();
                }
                ColorsAction::Reset => {
                    for tg in ColorTarget::ALL {
                        apply_color(tg, None);
                        let _ = self.settings.reset(tg.key());
                    }
                    let _ = self.settings.save();
                    self.redraw();
                    self.conn_win.redraw();
                }
                ColorsAction::None => {}
            }
            return;
        }
        if self.log_win.is(id) {
            if self.log_win.handle(&event) == LogWinAction::Paint {
                let px = self.settings.int("editor.font_size") as f32;
                self.log_win.paint(&self.mono_font, &self.theme, px);
            }
            return;
        }
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
                // 그리드 헤더 경계 위 = 폭 조절 커서.
                if let Some(w) = &self.window {
                    let over_edge = self.grid.header_edge_hover(self.cursor.0, self.cursor.1);
                    w.set_cursor(if over_edge {
                        winit::window::CursorIcon::ColResize
                    } else {
                        winit::window::CursorIcon::Default
                    });
                }
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
                    Key::Character("p" | "P") if self.primary && self.shift => {
                        if self.palette.is_open() {
                            self.palette.close();
                            self.redraw();
                        } else {
                            self.open_palette("");
                        }
                        return;
                    }
                    Key::Character("g" | "G") if self.primary && self.shift => {
                        self.toggle_log_window(el);
                        return;
                    }
                    Key::Character("l" | "L") if self.primary && self.shift => {
                        self.toggle_lang();
                        return;
                    }
                    Key::Character("l" | "L") if self.primary => {
                        self.open_conn_window(el);
                        return;
                    }
                    Key::Character("a" | "A") if self.primary => {
                        self.route(InputEvent::SelectAll);
                        return;
                    }
                    Key::Character("c" | "C") if self.primary => {
                        self.clip_action(EditCtxAction::Copy);
                        return;
                    }
                    Key::Character("x" | "X") if self.primary => {
                        self.clip_action(EditCtxAction::Cut);
                        return;
                    }
                    Key::Character("v" | "V") if self.primary => {
                        self.clip_action(EditCtxAction::Paste);
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
        if std::mem::take(&mut self.toggle_log) {
            self.toggle_log_window(el);
        }
        if std::mem::take(&mut self.open_colors) {
            let over = self.window.as_ref().and_then(|w| {
                let p = w.outer_position().ok()?;
                let sz = w.outer_size();
                Some((p.x, p.y, sz.width, sz.height))
            });
            let owner = self.window.clone();
            self.colors_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                over,
                owner.as_deref(),
            );
        }
        if std::mem::take(&mut self.open_conn) {
            self.open_conn_window(el);
        }
        if self.exit_requested {
            self.worker.send(worker::Cmd::Quit);
            el.exit();
        }
    }
}

/// 설정의 색 값(`#RRGGBB[AA]` · 형식 오류/빈 값 = None).
fn color_setting(settings: &Settings, key: &str) -> Option<String> {
    let v = settings.get(key)?.trim().to_string();
    (!v.is_empty() && nexa_ctl::rgba_from_hex(&v).is_some()).then_some(v)
}

/// 설정의 최근 색 목록(쉼표 구분 `#RRGGBBAA`).
fn recent_colors(settings: &Settings) -> Vec<u32> {
    settings
        .get("ui.color_recent")
        .unwrap_or("")
        .split(',')
        .filter_map(|s| nexa_ctl::rgba_from_hex(s.trim()))
        .take(8)
        .collect()
}

/// nexa-ctl 토큰에 색 적용(전 컨트롤 즉시).
fn apply_color(target: ColorTarget, hex: Option<&str>) {
    let rgba = hex.and_then(nexa_ctl::rgba_from_hex);
    match target {
        ColorTarget::Hover => nexa_ctl::tokens::set_hover_color(rgba),
        ColorTarget::Pressed => nexa_ctl::tokens::set_pressed_color(rgba),
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
    let probe_proxy: EventLoopProxy<Wake> = el.create_proxy();
    let probe_hub = probe::ProbeHub::spawn(Box::new(move || {
        let _ = probe_proxy.send_event(Wake);
    }));
    let initial_target = args.first().cloned();
    let profiles = worker::profile_names();
    let mut panel = ConnectPanel::new(nsql_drivers::available());
    // 실행 인자로 프로필 이름이 오면 폼을 채운다(접속은 Connect 버튼).
    if let Some(name) = initial_target
        .as_deref()
        .filter(|n| nsql_vault::is_profile_name(n))
    {
        if let Ok(Some(spec)) = Vault::open_default().and_then(|v| v.get(name)) {
            panel.fill(name, &spec);
        }
    }
    let status = if profiles.is_empty() {
        t(Msg::StInitial).to_string()
    } else {
        tf(Msg::StProfiles, &[&profiles.join(", ")])
    };
    // 창이 없는 동안의 팔레트 — OS 조회(창이 생기면 winit 판정으로 다시 고른다).
    let initial_theme = theme::resolve(settings.theme_mode(), None);
    let log_format = settings.get("log.format").unwrap_or("raw").to_string();
    let ed_line_numbers = settings.flag("editor.line_numbers");
    let ed_multi = settings.get("tabs.rows") != Some("single");
    let ed_tooltip = settings.flag("tabs.tooltip");
    let row_snap = settings.get("grid.scroll") == Some("row");
    let syntax_reg = Rc::new(SyntaxRegistry::load());
    let rulers = parse_rulers(settings.get("editor.rulers").unwrap_or("80"));
    let ws_style = whitespace_style(&settings);
    let colors_win = ColorsWin::new(
        color_setting(&settings, "ui.hover_color"),
        color_setting(&settings, "ui.pressed_color"),
        &recent_colors(&settings),
    );
    let mut app = App {
        window: None,
        ctx: None,
        surface: None,
        ui_font: ui.font,
        mono_font: mono.font,
        theme: initial_theme,
        settings,
        scale: 1.0,
        log_win: LogWin::new(&log_format),
        exit_requested: false,
        z_order: Vec::new(),
        palette: Palette::new(),
        syntax: syntax_reg.clone(),
        last_rows: None,
        last_secs: None,
        status_syntax_rect: Rect::new(0, 0, 0, 0),
        toggle_log: false,
        open_colors: false,
        colors_win,
        menubar: MenuBar::new(App::build_menus()),
        toolbar: App::build_toolbar(),
        conn_win: ConnWin::new(panel),
        open_conn: true,
        run_btn: Button::new(t(Msg::BtnRun)),
        editors: Editors::new(ed_line_numbers, ed_multi, ed_tooltip, syntax_reg),
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
        panel_op: None,
    };
    app.grid.set_row_snap(row_snap);
    app.editors.set_rulers(rulers);
    {
        let secs = |k: &str, min: i64| Duration::from_secs(app.settings.int(k).max(min) as u64);
        let policy = probe::ProbePolicy {
            enabled: app.settings.flag("probe.enabled"),
            max_retries: app.settings.int("probe.max_retries").max(0) as u32,
            timeout: secs("probe.timeout", 1),
            retry_delay: secs("probe.retry_delay", 1),
            interval: secs("probe.interval", 5),
        };
        app.conn_win.set_probe(probe_hub, policy);
    }
    app.editors.set_whitespace(ws_style);
    app.log_win.set_row_snap(row_snap);
    app.grid
        .set_row_numbers(app.settings.flag("grid.row_numbers"));
    // 호버 행 페이드 진입 시간(ms) — 전역이라 버튼·콤보·그리드·목록에 함께 적용(사용자 09-14).
    // 스크롤 방향(맥식 자연스러운 스크롤) — 창 세 개 공통.
    input::set_natural_scroll(app.settings.flag("input.scroll_natural"));
    // hover / 눌림 색(설정 · `#RRGGBBAA` · 비우면 테마 기본).
    for tg in ColorTarget::ALL {
        apply_color(tg, color_setting(&app.settings, tg.key()).as_deref());
    }
    // 페이드 속도 속성 두 단(Fast/Slow)의 실제 ms — 컨트롤은 속도 이름만 알고 여기서 값이 연계된다(사용자 09-14).
    nexa_ctl::tokens::set_fade_ms(
        nexa_ctl::tokens::FadeSpeed::Fast,
        app.settings.int("ui.fade_fast").clamp(0, 5000) as u32,
    );
    nexa_ctl::tokens::set_fade_ms(
        nexa_ctl::tokens::FadeSpeed::Slow,
        app.settings.int("ui.fade_slow").clamp(0, 5000) as u32,
    );
    // 접속 문자열(URL)로 실행하면 종전처럼 즉시 접속.
    if let Some(t) = initial_target.filter(|t| !nsql_vault::is_profile_name(t)) {
        app.busy = true;
        app.status = tf(Msg::StConnecting, &[&t]);
        app.worker.send(worker::Cmd::Connect(t));
    }
    if let Err(e) = el.run_app(&mut app) {
        eprintln!("{}", tf(Msg::ErrEventLoop, &[&e.to_string()]));
        std::process::exit(1);
    }
}

/// `editor.rulers` = "80, 120" → 열 목록(0·비정상 값 제외).
fn parse_rulers(v: &str) -> Vec<usize> {
    v.split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|t| t.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .collect()
}

/// `editor.whitespace*` 4키 → [`nexa_ctl::WhitespaceStyle`].
fn whitespace_style(settings: &Settings) -> nexa_ctl::WhitespaceStyle {
    use nexa_ctl::{WhitespaceMode, WhitespaceStyle};
    let mode = match settings.get("editor.whitespace") {
        Some("none") => WhitespaceMode::None,
        Some("all") => WhitespaceMode::All,
        _ => WhitespaceMode::Selection,
    };
    let chars: Vec<char> = settings
        .get("editor.whitespace_chars")
        .unwrap_or("·→_")
        .chars()
        .collect();
    let pick = |i: usize, d: char| match chars.get(i) {
        Some('_') | None => d,
        Some(c) => *c,
    };
    let color = settings
        .get("editor.whitespace_color")
        .filter(|h| h.len() == 6)
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .map(nexa_ctl::Color);
    WhitespaceStyle {
        mode,
        space: pick(0, '\0'),
        tab: pick(1, '\0'),
        eol: pick(2, '\0'),
        color,
        alpha: (settings.int("editor.whitespace_alpha") as f32 / 100.0).clamp(0.0, 1.0),
    }
}
