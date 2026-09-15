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
mod exp_icons;
mod explorer;
mod findbar;
mod grid;
mod icon;
mod input;
mod keymap;
mod keys_win;
mod log_win;
mod palette;
mod prefs_win;
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
use explorer::{Explorer, ExplorerAction, LiveReq};
use findbar::{FindAction, FindBar};
use keymap::{Chord, Keymap};
use keys_win::{KeysAction, KeysWin};
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
use nsql_script::ConnectSpec;
use nsql_settings::{Settings, ThemeMode};
use nsql_vault::Vault;
use palette::{Palette, PaletteAction};
use prefs_win::{PrefsAction, PrefsWin};
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
    Explorer,
    Find,
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
    /// ★ 단축키 표(사용자 09-15 · Sublime 기본 + `key.*` 설정) · 캡처 창.
    keymap: Keymap,
    open_keys: bool,
    keys_win: KeysWin,
    /// 환경 설정 창(T-39 · 사용자 09-15).
    open_prefs: bool,
    prefs_win: PrefsWin,
    // 컨트롤
    menubar: MenuBar,
    toolbar: Toolbar,
    /// 접속 창(별도 창 · 폼 + 로그인 목록). 폼 상태의 단일 원천 = `conn_win.panel`.
    conn_win: ConnWin,
    /// 메뉴/툴바에서 접속 창 열기 요청(창 생성은 이벤트 루프 핸들에서).
    open_conn: bool,
    run_btn: Button,
    /// 찾기/바꾸기 바(편집기 위 · T-73).
    find: FindBar,
    editors: Editors,
    grid: grid::Grid,
    /// ★ 오브젝트 탐색기(사용자 09-15 · docs/28) — 메타 세션은 자기 스레드.
    explorer: Explorer,
    /// 마지막으로 접속을 시도한 스펙(접속 성공 시 탐색기 메타 세션을 같은 스펙으로 연다).
    last_spec: Option<ConnectSpec>,
    /// 현재 접속 방언(Explain · INSERT 복사 · 상태줄).
    dialect: Dialect,
    /// 수동 커밋 모드에서 커밋되지 않은 변경이 있는가(상태줄 ● · 사용자 09-15).
    tx_dirty: bool,
    /// ★ Oracle 라이브 로그 모니터(T-71 · docs/32 §2): 편집기 세션 SID · 다음 폴링 시각 · 로그 테이블 기준 시각 · 마지막 세션 줄(중복 억제) · 실행 끝 뒤 마지막 1회.
    live_sid: Option<String>,
    live_next: Instant,
    live_since: Option<String>,
    live_last: String,
    live_final: bool,
    /// settings.json 감시(경로 · 마지막 수정 시각 · 다음 확인 시각) — JSON 편집을 연 뒤부터 1초 폴링(사용자 09-15).
    json_watch: Option<(std::path::PathBuf, Option<std::time::SystemTime>)>,
    /// 접속 창이 열려 메인 창을 모달로 막고 있는가(사용자 09-15) — 열림/닫힘 전환 때 OS 활성 상태를 맞춘다.
    conn_modal: bool,
    json_next: Instant,
    focus: Focus,
    // 워커
    worker: worker::Handle,
    events: mpsc::Receiver<RunEvent>,
    /// 접속 테스트 결과(요청당 스레드 · 워커와 별개).
    tests_tx: mpsc::Sender<worker::TestResult>,
    tests_rx: mpsc::Receiver<worker::TestResult>,
    /// 테스트 스레드가 UI를 깨우는 프록시(clone해서 스레드에 넘긴다).
    wake_proxy: EventLoopProxy<Wake>,
    /// ★ Test·Connect 시도 큐(사용자 09-14) — 동시 `attempts_max`(설정 `connect.max_concurrent` · 기본 4)까지 진행,
    /// 초과분은 FIFO로 대기했다가 앞의 시도가 끝나면 시작. 진행 중 수는 결과(테스트 결과 · 접속 성공/실패)가 올 때 줄어든다.
    attempt_queue: std::collections::VecDeque<Attempt>,
    attempts_inflight: usize,
    attempts_max: usize,
    busy: bool,
    status: String,
    log: Vec<String>,
    // 입력 상태
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    alt: bool,
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
        let menu_px = self.settings.int("ui.menu_font_size") as f32;
        let menu_h = px(menu_px + 11.0, s);
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
        // 왼쪽 오브젝트 탐색기(보이면 본문을 그만큼 오른쪽으로).
        let exp_w = if self.explorer.is_visible() {
            px(self.settings.int("explorer.width") as f32, s)
        } else {
            0
        };
        self.explorer
            .set_bounds(Rect::new(0, body_top, exp_w, body_h), s);
        let rx = rx + exp_w;
        let rw = rw - exp_w;
        let editor_h = (body_h as f32 * 0.5) as i32;
        let fb_h = self.find.height(s);
        self.find.set_bounds(Rect::new(rx, body_top, rw, fb_h), s);
        self.editors
            .set_bounds(Rect::new(rx, body_top + fb_h, rw, editor_h - pad - fb_h), s);
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
        self.explorer.set_focused(f == Focus::Explorer);
        self.find.set_focused(f == Focus::Find);
        if let Some(w) = &self.window {
            w.set_ime_allowed(f == Focus::Editor || f == Focus::Find);
        }
    }

    /// 포커스 텍스트 박스(IME·편집 컨텍스트 라우팅).
    fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        match self.focus {
            Focus::Editor => Some(self.editors.cur_mut()),
            Focus::Find => self.find.focused_textbox(),
            Focus::Grid | Focus::Explorer => None,
        }
    }

    /// 접속 패널의 요청 → 워커/저장소.
    fn handle_panel_action(&mut self, a: PanelAction) {
        match a {
            PanelAction::Connect(spec) => {
                let name = self.conn_win.panel.profile_name();
                // 테스트 중인 프로필은 끝날 때까지 접속도 막는다(사용자 09-14).
                if self.conn_win.is_testing(&name) {
                    self.status = t(Msg::StTesting).into();
                    self.conn_win.panel.set_state(ConnState::Testing);
                    return;
                }
                if let Some(pw) = spec.password.as_deref() {
                    self.conn_win.remember_pw(&name, pw);
                }
                self.conn_win
                    .set_connect_mark(&name, Some(ConnectMark::Connecting));
                self.panel_op = Some((name.clone(), ConnState::Connecting));
                // 같은 서버면 기존 세션 유지 · 설정 `connect.reconnect_same`이면 닫고 다시 접속(사용자 09-14).
                let reconnect_same = self.settings.flag("connect.reconnect_same");
                // 동시 상한·큐를 거친다(초과분은 앞의 시도가 끝나면 시작).
                self.attempt_queue.push_back(Attempt::Connect {
                    name,
                    spec,
                    reconnect_same,
                });
                self.dispatch_attempts();
            }
            PanelAction::Test(spec) => {
                let name = self.conn_win.panel.profile_name();
                if let Some(pw) = spec.password.as_deref() {
                    self.conn_win.remember_pw(&name, pw);
                }
                self.start_test(&name, spec);
            }
            PanelAction::Disconnect => {
                self.busy = true;
                self.worker.send(worker::Cmd::Disconnect);
            }
            PanelAction::Save { name, spec } => {
                // 저장하지 않더라도 입력된 비밀번호는 세션에 보관(사용자 09-14).
                let typed = self.conn_win.panel.password_text();
                self.conn_win.remember_pw(&name, &typed);
                // ★ 저장은 파일 쓰기뿐 — 워커(순차 · Test/Connect 뒤에 줄 섬)를 거치지 않고 즉시(사용자 09-14
                //   "Save에서 접속 테스트를 하지 않도록": 실제로는 앞선 Test의 20초 타임아웃을 기다리던 것). 접속 검증 없음 · 포트가 틀려도 저장.
                match Vault::open_default().and_then(|v| v.save(&name, &spec)) {
                    Ok(()) => {
                        self.status = tf(Msg::WkProfileSaved, &[&name, ""]);
                        self.conn_win.refresh_profiles(Some(&name));
                    }
                    Err(e) => {
                        let e = e.to_string();
                        self.status = tf(Msg::WkProfileSaveFailed, &[&e]);
                        self.conn_win.panel.set_state(ConnState::Failed(e));
                    }
                }
            }
            PanelAction::Edit(_) => {} // 접속 창이 자체 처리(클립보드)
            PanelAction::LoadProfile(name) => {
                match Vault::open_default().and_then(|v| v.get(&name)) {
                    Ok(Some(spec)) => {
                        let spec = self.conn_win.with_session_pw(&name, spec);
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
        if self.conn_win.panel.profile_name().trim() == name.trim() {
            self.conn_win.panel.set_state(st.clone());
        }
        self.panel_op = Some((name.to_string(), st));
    }

    fn drain_conn(&mut self) -> bool {
        let mut changed = false;
        // 접속 테스트 결과(스레드별) — 이름이 함께 오므로 여러 테스트가 섞여도 각자 자리에.
        while let Ok(r) = self.tests_rx.try_recv() {
            changed = true;
            self.attempt_done();
            let name = r.name;
            match r.outcome {
                Ok((description, elapsed_s)) => {
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        format!("test ok: {description} ({elapsed_s}s)"),
                    ));
                    let msg = tf(Msg::StTestOk, &[&description, &elapsed_s]);
                    self.status = msg.clone();
                    self.conn_win.set_test_mark(&name, TestMark::Ok);
                    self.set_panel_result(&name, ConnState::TestOk(msg));
                }
                Err(e) => {
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("test: {e}")));
                    self.status = tf(Msg::StTestFailed, &[&e]);
                    self.conn_win.set_test_mark(&name, TestMark::Failed);
                    self.set_panel_result(&name, ConnState::Failed(e));
                    self.conn_win.note_failure(&name);
                }
            }
        }
        while let Ok(o) = self.worker.conn.try_recv() {
            changed = true;
            match o {
                ConnOutcome::Connected(d) => {
                    self.attempt_done();
                    let name = self.panel_op_name();
                    self.conn_win.mark_connected(&name);
                    // 접속 버튼 초록 = 지금 접속된 프로필 하나만 → 잠시 보여 준 뒤 창 닫힘(사용자 09-14).
                    self.conn_win.clear_connect_marks();
                    self.conn_win
                        .set_connect_mark(&name, Some(ConnectMark::Connected));
                    self.conn_win.close_after_connect();
                    // 활성 탭에 접속 정보 적용 — 탭이 없으면 새 탭(사용자 09-14).
                    self.editors.ensure_tab();
                    self.editors.set_conn_desc(d.clone());
                    self.set_panel_result(&name, ConnState::Connected(d));
                }
                ConnOutcome::ConnectFailed(e) => {
                    self.attempt_done();
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("connect: {e}")));
                    self.status = tf(Msg::StConnectFailed, &[&e]);
                    let name = self.panel_op_name();
                    self.conn_win.set_connect_mark(&name, None);
                    self.set_panel_result(&name, ConnState::Failed(e));
                    // 접속 실패 확인 → 그 서버 신호등 즉시 갱신(사용자 09-14).
                    self.conn_win.note_failure(&name);
                }
                ConnOutcome::SessionId(sid) => {
                    self.live_sid = Some(sid);
                }
                ConnOutcome::Disconnected => {
                    self.live_sid = None;
                    self.editors.set_conn_desc("");
                    self.conn_win.clear_connect_marks();
                    self.conn_win.clear_active();
                    self.panel_op = None;
                    self.conn_win.panel.set_state(ConnState::Idle);
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

    /// 편집기 본문에서 질의 일치 위치(문자 인덱스 · 대소문자 옵션) 전부.
    fn find_matches(&mut self) -> (Vec<(usize, usize)>, Vec<char>) {
        let text: Vec<char> = self.ed_mut().text().chars().collect();
        let q: Vec<char> = self.find.query().chars().collect();
        if q.is_empty() || q.len() > text.len() {
            return (Vec::new(), text);
        }
        let cs = self.find.case_sensitive();
        let eq = |a: char, b: char| {
            if cs {
                a == b
            } else {
                a.to_lowercase().eq(b.to_lowercase())
            }
        };
        let mut out = Vec::new();
        let mut i = 0;
        while i + q.len() <= text.len() {
            if text[i..i + q.len()].iter().zip(&q).all(|(a, b)| eq(*a, *b)) {
                out.push((i, i + q.len()));
                i += q.len();
            } else {
                i += 1;
            }
        }
        (out, text)
    }

    /// 다음/이전 일치로 이동(순환) — `advance`면 현재 선택을 지나서, 아니면 캐럿부터.
    fn find_step(&mut self, forward: bool, advance: bool) {
        if !self.find.is_visible() {
            return;
        }
        let (matches, _) = self.find_matches();
        if matches.is_empty() {
            self.find.set_status(t(Msg::StFindNone));
            self.redraw();
            return;
        }
        let sel = self.ed_mut().selection();
        let caret = self.ed_mut().caret();
        let idx = if forward {
            let from = match sel {
                Some((a, b)) if advance => {
                    if b > a {
                        a + 1
                    } else {
                        caret
                    }
                }
                Some((a, _)) => a,
                None => caret,
            };
            matches.iter().position(|(s, _)| *s >= from).unwrap_or(0)
        } else {
            let from = sel.map_or(caret, |(a, _)| a);
            matches
                .iter()
                .rposition(|(s, _)| *s < from)
                .unwrap_or(matches.len() - 1)
        };
        let (s, e) = matches[idx];
        let mut inv = Invalidations::default();
        self.ed_mut().select_range(s, e, &mut inv);
        self.find.set_status(tf(
            Msg::StFindCount,
            &[&(idx + 1).to_string(), &matches.len().to_string()],
        ));
        self.redraw();
    }

    /// 현재 선택이 일치면 바꾸고 다음으로.
    fn find_replace_one(&mut self) {
        let (matches, _) = self.find_matches();
        if let Some((a, b)) = self.ed_mut().selection() {
            if matches.contains(&(a, b)) {
                let repl = self.find.replacement();
                let mut inv = Invalidations::default();
                self.ed_mut().replace_range(a, b, &repl, &mut inv);
            }
        }
        self.find_step(true, false);
    }

    fn find_replace_all(&mut self) {
        let (matches, _) = self.find_matches();
        if matches.is_empty() {
            self.find.set_status(t(Msg::StFindNone));
            self.redraw();
            return;
        }
        let repl = self.find.replacement();
        let mut inv = Invalidations::default();
        for (a, b) in matches.iter().rev() {
            self.ed_mut().replace_range(*a, *b, &repl, &mut inv);
        }
        self.find
            .set_status(tf(Msg::StReplacedN, &[&matches.len().to_string()]));
        self.redraw();
    }

    fn find_action(&mut self, a: FindAction) {
        match a {
            FindAction::None => {}
            FindAction::Next => self.find_step(true, true),
            FindAction::Prev => self.find_step(false, true),
            FindAction::Changed => self.find_step(true, false),
            FindAction::Replace => self.find_replace_one(),
            FindAction::ReplaceAll => self.find_replace_all(),
            FindAction::Close => {
                self.find.close();
                self.layout();
                self.set_focus(Focus::Editor);
                self.redraw();
            }
        }
    }

    /// settings.json으로 편집(사용자 09-15) — 내보내고 외부 프로그램(.json 연결)으로 연 뒤 저장을 감시한다.
    /// `settings.json_editor = builtin`은 편집기 파일 저장(T-74)이 생기면 탭으로(T-76) · 지금은 외부로 대체.
    fn edit_settings_json(&mut self) {
        let path = match self.settings.export_json() {
            Ok(p) => p,
            Err(e) => {
                self.status = tf(Msg::StJsonError, &[&e.to_string()]);
                return;
            }
        };
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        self.json_watch = Some((path.clone(), mtime));
        self.json_next = Instant::now() + Duration::from_millis(1000);
        let builtin = self.settings.get("settings.json_editor") == Some("builtin");
        match open_external(&path) {
            Ok(()) => {
                self.status = if builtin {
                    t(Msg::StJsonBuiltinTodo).to_string()
                } else {
                    tf(Msg::StJsonOpened, &[&path.display().to_string()])
                };
            }
            Err(e) => self.status = tf(Msg::StJsonError, &[&e]),
        }
        self.redraw();
    }

    /// settings.json 저장 감시 — 바뀌었으면 다시 읽어 바뀐 키만 반영·저장(1초 폴링 · 열어 둔 뒤에만).
    fn json_tick(&mut self, now: Instant) -> Option<Instant> {
        let (path, last) = self.json_watch.clone()?;
        if now < self.json_next {
            return Some(self.json_next);
        }
        self.json_next = now + Duration::from_millis(1000);
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if mtime.is_some() && mtime != last {
            self.json_watch = Some((path.clone(), mtime));
            match std::fs::read_to_string(&path) {
                Ok(text) => match self.settings.import_json(&text) {
                    Ok(r) => {
                        self.persist_settings();
                        let mut restart = false;
                        for k in &r.changed {
                            if !self.apply_setting(k) {
                                restart = true;
                            }
                        }
                        let mut note = String::new();
                        if !r.unknown.is_empty() {
                            note.push_str(&format!(" · unknown {}", r.unknown.join(",")));
                        }
                        if !r.invalid.is_empty() {
                            note.push_str(&format!(
                                " · invalid {}",
                                r.invalid
                                    .iter()
                                    .map(|(k, _)| k.as_str())
                                    .collect::<Vec<_>>()
                                    .join(",")
                            ));
                        }
                        if restart {
                            note.push_str(" · ");
                            note.push_str(t(Msg::StNeedsRestart));
                        }
                        self.status =
                            tf(Msg::StJsonReloaded, &[&r.changed.len().to_string(), &note]);
                        self.prefs_win.refresh(&self.settings);
                        self.prefs_win.redraw();
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.status.clone()));
                    }
                    Err(e) => {
                        self.status = tf(Msg::StJsonError, &[&e]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, self.status.clone()));
                    }
                },
                Err(e) => self.status = tf(Msg::StJsonError, &[&e.to_string()]),
            }
            self.redraw();
        }
        Some(self.json_next)
    }

    /// 접속 창 열림/닫힘 전환 → 메인 창 활성 상태 동기화(모달 · 닫히면 메인으로 포커스).
    fn sync_conn_modal(&mut self) {
        let open = self.conn_win.is_open();
        if open == self.conn_modal && !open {
            return;
        }
        self.conn_modal = open;
        // 메인 창 + 그 일부로 보는 보조 창 전부(로그 · 색 · 단축키 · 설정).
        let others: Vec<&Window> = [
            self.window.as_deref(),
            self.log_win.window(),
            self.colors_win.window(),
            self.keys_win.window(),
            self.prefs_win.window(),
        ]
        .into_iter()
        .flatten()
        .collect();
        for w in &others {
            winfocus::set_enabled(w, !open);
        }
        if !open {
            if let Some(w) = &self.window {
                w.focus_window();
            }
        }
    }

    /// 툴바 접속 해제 버튼 = 접속돼 있을 때만 활성(사용자 09-15).
    fn sync_disconnect_btn(&mut self, connected: bool) {
        let mut inv = Invalidations::default();
        self.toolbar
            .set_item_enabled("conn.disconnect", connected, &mut inv);
    }

    /// 환경 설정 창에서 바뀐 값을 **즉시** 반영(가능한 것만 · 나머지는 다음 시작).
    fn apply_setting(&mut self, key: &str) -> bool {
        let i = |s: &Settings, k: &str| s.int(k);
        match key {
            "ui.theme" => self.apply_theme(),
            "ui.lang" => {
                nsql_i18n::set_lang(self.settings.lang());
                self.relabel();
            }
            "ui.hover_color" | "ui.pressed_color" => {
                for tg in ColorTarget::ALL {
                    if tg.key() == key {
                        apply_color(tg, color_setting(&self.settings, key).as_deref());
                    }
                }
            }
            "ui.fade_fast" => nexa_ctl::tokens::set_fade_ms(
                nexa_ctl::tokens::FadeSpeed::Fast,
                i(&self.settings, key).clamp(0, 5000) as u32,
            ),
            "ui.fade_slow" => nexa_ctl::tokens::set_fade_ms(
                nexa_ctl::tokens::FadeSpeed::Slow,
                i(&self.settings, key).clamp(0, 5000) as u32,
            ),
            "ui.hover_intent_ms" => {
                nexa_ctl::tokens::set_intent_ms(i(&self.settings, key).clamp(0, 500) as u64)
            }
            "ui.fade_out_ms" => {
                nexa_ctl::tokens::set_fade_out_ms(i(&self.settings, key).clamp(0, 2000) as u32)
            }
            "input.scroll_natural" => {
                input::set_natural_scroll(self.settings.flag(key));
            }
            "explorer.visible" => {
                self.explorer.set_visible(self.settings.flag(key));
                self.layout();
            }
            "explorer.icons" => self.explorer.set_icons(self.settings.flag(key)),
            "grid.row_numbers" => self.grid.set_row_numbers(self.settings.flag(key)),
            "grid.scroll" => self
                .grid
                .set_row_snap(self.settings.get(key) == Some("row")),
            "editor.line_numbers" => self.editors.set_line_numbers(self.settings.flag(key)),
            "editor.rulers" => self
                .editors
                .set_rulers(parse_rulers(self.settings.get(key).unwrap_or("80"))),
            "tabs.tooltip" => self.editors.set_tooltip(self.settings.flag(key)),
            "log.format" => self
                .log_win
                .set_format(self.settings.get(key).unwrap_or("raw")),
            k if k.starts_with("key.") => {
                self.keymap = Keymap::from_settings(&self.settings);
                self.keys_win.refresh(&self.keymap);
            }
            k if k.starts_with("editor.whitespace") || k.starts_with("editor.show_") => {
                self.editors
                    .set_whitespace(whitespace_style(&self.settings));
            }
            "ui.font_size" | "ui.menu_font_size" | "editor.font_size" | "grid.font_size"
            | "explorer.width" | "explorer.font_size" => {
                self.layout();
            }
            _ => return false,
        }
        self.layout();
        self.redraw();
        self.conn_win.redraw();
        true
    }

    /// 그리드가 메뉴로 만든 복사 텍스트를 OS 클립보드로.
    fn after_grid_event(&mut self) {
        if let Some((text, n)) = self.grid.take_copy() {
            if clipboard::write_text(&text) {
                self.status = tf(Msg::StCopied, &[&n.to_string()]);
            } else {
                self.status = t(Msg::ErrClipboard).into();
            }
        }
    }

    /// 단축키로 온 명령 — 팔레트 토글·로그 창·접속 창처럼 이벤트 루프 핸들이 필요한 것만 여기서, 나머지는 [`Self::menu_action`].
    fn key_command(&mut self, id: &str, el: &ActiveEventLoop) {
        match id {
            "view.palette" => {
                if self.palette.is_open() {
                    self.palette.close();
                    self.redraw();
                } else {
                    self.open_palette("");
                }
            }
            "view.log" => self.toggle_log_window(el),
            "conn.toggle" => self.open_conn_window(el),
            _ => self.menu_action(id),
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
            "edit.select_all" => {
                if self.focus == Focus::Grid {
                    self.grid.select_all();
                } else {
                    self.route(InputEvent::SelectAll);
                }
            }
            "edit.find" | "edit.replace" => {
                let seed = self.editors.cur().copy_selection();
                self.find.open(id == "edit.replace", seed);
                self.layout();
                self.set_focus(Focus::Find);
                self.find_step(true, false);
            }
            "edit.find_next" => self.find_step(true, true),
            "edit.find_prev" => self.find_step(false, true),
            "edit.undo" => self.route(InputEvent::Undo),
            "edit.redo" => self.route(InputEvent::Redo),
            "view.log" => self.toggle_log = true,
            "view.colors" => self.open_colors = true,
            "view.keys" => self.open_keys = true,
            "view.explorer" => {
                let on = !self.explorer.is_visible();
                self.explorer.set_visible(on);
                let _ = self
                    .settings
                    .set("explorer.visible", if on { "on" } else { "off" });
                let _ = self.settings.save();
                if !on && self.focus == Focus::Explorer {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
            }
            "file.close_tab" => {
                let i = self.editors.active();
                self.editors.close_tab(i);
                self.set_focus(Focus::Editor);
            }
            "tab.next" | "tab.prev" => {
                let n = self.editors.len();
                if n > 1 {
                    let i = self.editors.active();
                    let j = if id == "tab.next" {
                        (i + 1) % n
                    } else {
                        (i + n - 1) % n
                    };
                    self.editors.switch(j);
                }
            }
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
            "run.explain" => self.run_explain(),
            "run.commit" => self.worker.send(worker::Cmd::Commit),
            "run.rollback" => self.worker.send(worker::Cmd::Rollback),
            // 접속 창 열기 — 연결 중이어도 끊지 않고 그냥 연다(사용자 09-14). 끊기는 폼의 Disconnect 버튼.
            "conn.toggle" => self.open_conn = true,
            "conn.disconnect" => {
                self.busy = true;
                self.worker.send(worker::Cmd::Disconnect);
            }
            "edit.prefs" => self.open_prefs = true,
            "edit.settings_json" => self.edit_settings_json(),
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
                    item("file.close_tab", Msg::MnCloseTab),
                    MenuEntry::Separator,
                    item("file.exit", Msg::MnExit),
                ],
            ),
            MenuDef::new(
                t(Msg::MnEdit),
                vec![
                    item("edit.undo", Msg::MnUndo),
                    item("edit.redo", Msg::MnRedo),
                    MenuEntry::Separator,
                    item("edit.find", Msg::MnFind),
                    item("edit.replace", Msg::MnReplace),
                    item("edit.find_next", Msg::MnFindNext),
                    item("edit.find_prev", Msg::MnFindPrev),
                    MenuEntry::Separator,
                    item("edit.cut", Msg::MnCut),
                    item("edit.copy", Msg::MnCopy),
                    item("edit.paste", Msg::MnPaste),
                    MenuEntry::Separator,
                    item("edit.select_all", Msg::MnSelectAll),
                    MenuEntry::Separator,
                    item("edit.prefs", Msg::MnPreferences),
                    item("edit.settings_json", Msg::MnSettingsJson),
                ],
            ),
            MenuDef::new(
                t(Msg::MnView),
                vec![
                    item("view.palette", Msg::MnCommandPalette),
                    item("view.explorer", Msg::MnExplorer),
                    item("view.log", Msg::MnLogWindow),
                    MenuEntry::Separator,
                    item("view.colors", Msg::MnColors),
                    item("view.keys", Msg::MnKeys),
                    item("view.theme", Msg::MnTheme),
                    item("view.lang", Msg::MnLanguage),
                ],
            ),
            MenuDef::new(
                t(Msg::MnRun),
                vec![
                    item("run.statement", Msg::MnRunStatement),
                    item("run.all", Msg::MnRunAll),
                    item("run.explain", Msg::MnExplain),
                    MenuEntry::Separator,
                    item("run.commit", Msg::MnCommit),
                    item("run.rollback", Msg::MnRollback),
                    MenuEntry::Separator,
                    item("conn.toggle", Msg::MnConnect),
                    item("conn.disconnect", Msg::MnDisconnect),
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
            ToolItem::new("conn.disconnect", toolicons::disconnect())
                .tip(t(Msg::TipDisconnect))
                .disabled(),
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
            EditCtxAction::Copy if self.focus == Focus::Grid => {
                if let Some((text, n)) = self.grid.copy_selection(grid::CopyKind::Tsv) {
                    failed = !clipboard::write_text(&text);
                    if !failed {
                        self.status = tf(Msg::StCopied, &[&n.to_string()]);
                    }
                }
            }
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
        cmds.push(m("edit.undo", Msg::MnEdit, Msg::MnUndo));
        cmds.push(m("edit.find", Msg::MnEdit, Msg::MnFind));
        cmds.push(m("edit.replace", Msg::MnEdit, Msg::MnReplace));
        cmds.push(m("edit.find_next", Msg::MnEdit, Msg::MnFindNext));
        cmds.push(m("edit.find_prev", Msg::MnEdit, Msg::MnFindPrev));
        cmds.push(m("edit.redo", Msg::MnEdit, Msg::MnRedo));
        cmds.push(m("view.log", Msg::MnView, Msg::MnLogWindow));
        cmds.push(m("view.colors", Msg::MnView, Msg::MnColors));
        cmds.push(m("view.keys", Msg::MnView, Msg::MnKeys));
        cmds.push(m("view.explorer", Msg::MnView, Msg::MnExplorer));
        cmds.push(m("file.close_tab", Msg::MnFile, Msg::MnCloseTab));
        cmds.push(m("tab.next", Msg::MnView, Msg::MnNextTab));
        cmds.push(m("tab.prev", Msg::MnView, Msg::MnPrevTab));
        cmds.push(m("view.theme", Msg::MnView, Msg::MnTheme));
        cmds.push(m("view.lang", Msg::MnView, Msg::MnLanguage));
        cmds.push(m("run.statement", Msg::MnRun, Msg::MnRunStatement));
        cmds.push(m("run.all", Msg::MnRun, Msg::MnRunAll));
        cmds.push(m("run.explain", Msg::MnRun, Msg::MnExplain));
        cmds.push(m("run.commit", Msg::MnRun, Msg::MnCommit));
        cmds.push(m("run.rollback", Msg::MnRun, Msg::MnRollback));
        cmds.push(m("conn.toggle", Msg::MnRun, Msg::MnConnect));
        cmds.push(m("conn.disconnect", Msg::MnRun, Msg::MnDisconnect));
        cmds.push(m("edit.prefs", Msg::MnEdit, Msg::MnPreferences));
        cmds.push(m("edit.settings_json", Msg::MnEdit, Msg::MnSettingsJson));
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
    /// ★ 접속 테스트 시작 — 요청당 스레드(순차 워커·`busy`와 무관 · 실패 서버 타임아웃이 다른 테스트를 막지 않는다 · 사용자 09-14).
    /// 같은 프로필이 이미 테스트 중이면 무시.
    fn start_test(&mut self, name: &str, spec: ConnectSpec) {
        if self.conn_win.test_mark(name) == Some(TestMark::Testing) {
            return;
        }
        self.status = t(Msg::StTesting).into();
        self.conn_win.set_test_mark(name, TestMark::Testing);
        self.panel_op = Some((name.to_string(), ConnState::Testing));
        if self.conn_win.panel.profile_name() == name {
            self.conn_win.panel.set_state(ConnState::Testing);
        }
        // 동시 상한·큐를 거친다(초과분은 대기 · 표시는 진행 중과 같은 노란 고리).
        self.attempt_queue.push_back(Attempt::Test {
            name: name.to_string(),
            spec,
        });
        self.dispatch_attempts();
        self.conn_win.redraw();
    }

    /// 큐에서 시도를 꺼내 상한까지 시작한다 — 결과가 올 때마다 다시 부른다.
    fn dispatch_attempts(&mut self) {
        while self.attempts_inflight < self.attempts_max {
            let Some(a) = self.attempt_queue.pop_front() else {
                break;
            };
            self.attempts_inflight += 1;
            match a {
                Attempt::Test { name, spec } => {
                    let proxy = self.wake_proxy.clone();
                    worker::spawn_test(
                        name,
                        spec,
                        DEFAULT_DIALECT,
                        self.tests_tx.clone(),
                        Box::new(move || {
                            let _ = proxy.send_event(Wake);
                        }),
                    );
                }
                Attempt::Connect {
                    spec,
                    reconnect_same,
                    ..
                } => {
                    self.busy = true;
                    self.status = tf(Msg::StConnecting, &[&spec.redacted()]);
                    self.last_spec = Some(spec.clone());
                    self.worker.send(worker::Cmd::ConnectSpec {
                        spec,
                        reconnect_same,
                    });
                }
            }
        }
    }

    /// 시도 하나가 끝났다(테스트 결과 · 접속 성공/실패) — 슬롯을 비우고 큐를 이어 간다.
    fn attempt_done(&mut self) {
        self.attempts_inflight = self.attempts_inflight.saturating_sub(1);
        self.dispatch_attempts();
    }

    fn test_profile(&mut self, name: &str) {
        match Vault::open_default().and_then(|v| v.get(name)) {
            Ok(Some(spec)) => {
                let spec = self.conn_win.with_session_pw(name, spec);
                // 행 Test = 행 선택 + (폼이 펼쳐져 있으면) 폼에 채움 + 폼 Test와 동일 경로(사용자 09-14 "두 행위 동일").
                self.conn_win.select_by_name(name);
                if self.conn_win.is_detail_open() && self.conn_win.panel.profile_name() != name {
                    self.handle_panel_action(PanelAction::LoadProfile(name.to_string()));
                }
                self.start_test(name, spec);
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
        if self.conn_win.is_testing(name) {
            self.status = t(Msg::StTesting).into();
            return;
        }
        match Vault::open_default().and_then(|v| v.get(name)) {
            Ok(Some(spec)) => {
                let spec = self.conn_win.with_session_pw(name, spec);
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
        // 인접 항목 자동 선택 · 폼이 펼쳐져 있으면 그 항목으로 갱신(비면 New 상태).
        if let Some(next) = self.conn_win.after_delete() {
            self.handle_panel_action(PanelAction::LoadProfile(next));
        }
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
        self.live_start();
        self.redraw();
    }

    /// 라이브 로그 설정(끔이면 None) — 매번 읽는다(설정 창에서 바꾸면 다음 실행부터).
    fn live_req(&self) -> Option<LiveReq> {
        if self.dialect != Dialect::Oracle {
            return None;
        }
        let source = self.settings.get("oracle.live.source").unwrap_or("off");
        if source == "off" {
            return None;
        }
        let sid = self.live_sid.clone()?;
        let table = self
            .settings
            .get("oracle.live.table")
            .unwrap_or("")
            .trim()
            .to_string();
        if source == "table" && table.is_empty() {
            return None;
        }
        Some(LiveReq {
            sid,
            source: source.to_string(),
            table,
            ts_col: self
                .settings
                .get("oracle.live.ts_col")
                .unwrap_or("LOG_TIME")
                .trim()
                .to_string(),
            text_col: self
                .settings
                .get("oracle.live.text_col")
                .unwrap_or("LOG_TEXT")
                .trim()
                .to_string(),
            since: self.live_since.clone(),
        })
    }

    fn live_interval(&self) -> Duration {
        Duration::from_millis(
            self.settings
                .int("oracle.live.interval_ms")
                .clamp(250, 60_000) as u64,
        )
    }

    /// 실행 시작 — 기준 시각 초기화 · 첫 폴링 즉시(로그 테이블은 서버 현재 시각을 기준점으로 받는다).
    fn live_start(&mut self) {
        self.live_since = None;
        self.live_last.clear();
        self.live_final = true;
        if let Some(req) = self.live_req() {
            self.explorer.live_poll(req);
            self.live_next = Instant::now() + self.live_interval();
        }
    }

    /// 주기 폴링(실행 중) · 실행이 끝나면 마지막 1회.
    fn live_tick(&mut self, now: Instant) -> Option<Instant> {
        if self.busy {
            if now >= self.live_next {
                if let Some(req) = self.live_req() {
                    self.explorer.live_poll(req);
                }
                self.live_next = now + self.live_interval();
            }
            Some(self.live_next)
        } else if self.live_final {
            self.live_final = false;
            if let Some(req) = self.live_req() {
                self.explorer.live_poll(req);
            }
            None
        } else {
            None
        }
    }

    /// 라이브 응답 → 로그 창(세션 소스는 바뀐 줄만).
    fn live_drain(&mut self) -> bool {
        let mut changed = false;
        for r in self.explorer.take_live() {
            match r {
                Ok((lines, last_ts)) => {
                    if last_ts.is_some() {
                        self.live_since = last_ts;
                    }
                    for line in lines {
                        if line == self.live_last {
                            continue;
                        }
                        self.live_last = line.clone();
                        let text = format!("[live] {line}");
                        self.log_win
                            .push(LogEntry::new(LogKind::Output, text.clone()));
                        self.log.push(text);
                        changed = true;
                    }
                    if changed {
                        self.grid.set_messages(self.log.clone());
                    }
                }
                Err(e) => {
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("[live] {e}")));
                    changed = true;
                }
            }
        }
        changed
    }

    /// 실행 계획(사용자 09-15 기본 기능) — 캐럿 문장(또는 선택)을 방언별 EXPLAIN 관용으로 감싸 실행.
    fn run_explain(&mut self) {
        if self.busy {
            self.status = t(Msg::StRunning).into();
            return;
        }
        let text = self
            .ed_mut()
            .copy_selection()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                let full = self.ed_mut().text();
                let byte_pos = full
                    .char_indices()
                    .nth(self.ed_mut().caret())
                    .map_or(full.len(), |(b, _)| b);
                nsql_script::statement_at(&full, byte_pos).map(|it| it.text)
            });
        let Some(stmt) = text.filter(|s| !s.trim().is_empty()) else {
            self.status = t(Msg::ErrNoSql).into();
            return;
        };
        let src = nsql_script::explain_script(self.dialect, &stmt);
        self.busy = true;
        self.status = t(Msg::StRunning).into();
        self.log.clear();
        self.worker.send(worker::Cmd::Run {
            src,
            preflight: None,
        });
        self.live_start();
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
                RunEvent::ResultSet {
                    rs, elapsed, more, ..
                } => {
                    self.last_rows = Some(rs.rows.len());
                    self.last_secs = Some(elapsed.as_secs_f64());
                    let n = rs.rows.len().to_string();
                    let secs = format!("{:.3}", elapsed.as_secs_f64());
                    // 페치 상한에서 잘렸으면 "더 있음"을 알린다(DBeaver식 · 사용자 09-15).
                    self.status = if more {
                        tf(Msg::StRowsMore, &[&n, &secs])
                    } else {
                        tf(Msg::StRows, &[&n, &secs])
                    };
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
                    if rows_affected.is_some_and(|n| n > 0)
                        && !self.settings.flag("session.autocommit")
                    {
                        self.tx_dirty = true;
                    }
                }
                RunEvent::Print { pairs } => {
                    for (n, v) in pairs {
                        self.log.push(format!("{n} = {}", v.display()));
                    }
                    self.grid.set_messages(self.log.clone());
                }
                RunEvent::Message(m) => {
                    if m == t(Msg::StCommitted) || m == t(Msg::StRolledBack) {
                        self.tx_dirty = false;
                        self.status = m.clone();
                    }
                    self.log.push(m);
                    self.grid.set_messages(self.log.clone());
                }
                RunEvent::Connected {
                    description,
                    dialect,
                } => {
                    self.status = tf(Msg::StConnected, &[&description, &dialect.to_string()]);
                    self.busy = false;
                    self.dialect = dialect;
                    self.grid.set_dialect(dialect);
                    self.tx_dirty = false;
                    self.sync_disconnect_btn(true);
                    // 탐색기 메타 세션(별도) — 같은 스펙으로.
                    if let Some(spec) = self.last_spec.clone() {
                        let name = self.conn_win.active_name().to_string();
                        self.explorer.connect(&spec, &name);
                    }
                }
                RunEvent::Disconnected => {
                    self.status = t(Msg::StDisconnected).into();
                    self.explorer.disconnect();
                    self.tx_dirty = false;
                    self.sync_disconnect_btn(false);
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
        if self.explorer.drain() {
            changed = true;
        }
        if self.live_drain() {
            changed = true;
        }
        for a in self.explorer.take_actions() {
            changed = true;
            match a {
                ExplorerAction::OpenSql { title, text } => {
                    self.editors.new_tab(Some(title));
                    self.editors.cur_mut().set_text(&text);
                    self.set_focus(Focus::Editor);
                }
                ExplorerAction::Status(s) => self.status = s,
                ExplorerAction::Copy(s) => {
                    if !clipboard::write_text(&s) {
                        self.status = t(Msg::ErrClipboard).into();
                    }
                }
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
            let grid_px = self.settings.int("grid.font_size") as f32;
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
                // 메뉴바(드롭다운 포함)는 편집기·그리드·탐색기 뒤인 최상위 패스에서 그린다(09-15 사용자 캡처: 풀다운이 뒤로 가림).
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
                // 트랜잭션 모드(자동/수동 · 수동에 미커밋 변경이 있으면 ●).
                let tx = if self.settings.flag("session.autocommit") {
                    t(Msg::StTxAuto).to_string()
                } else if self.tx_dirty {
                    format!("● {}", t(Msg::StTxManual))
                } else {
                    t(Msg::StTxManual).to_string()
                };
                segs.push((tx, false));
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
            }
            // ── 결과 그리드(고정폭 · 자체 글꼴 크기 `grid.font_size`)
            {
                let prefs = FontPrefs {
                    base: SlotFont {
                        size: grid_px,
                        bold: false,
                        italic: false,
                    },
                    ..FontPrefs::default()
                };
                let mut dc = RasterCtx::new(&mut gfx, &self.mono_font, s).with_fonts(prefs);
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
                self.find.paint(&mut dc, &th);
                self.palette.paint(&mut dc, &th);
                self.editors.paint_tooltip(&mut dc, &th, wi);
            }
            // ── 오브젝트 탐색기(자체 글꼴 크기 `explorer.font_size` · 기본 = 메뉴 글꼴 · 사용자 09-15)
            {
                let exp_px = match self.settings.int("explorer.font_size") {
                    0 => self.settings.int("ui.menu_font_size"),
                    n => n,
                } as f32;
                let prefs = FontPrefs {
                    base: SlotFont {
                        size: exp_px,
                        bold: false,
                        italic: false,
                    },
                    ..FontPrefs::default()
                };
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s).with_fonts(prefs);
                self.explorer.paint(&mut dc, &th);
            }
            // ── 메뉴바 + 열린 드롭다운(별도 글꼴 크기 `ui.menu_font_size` · 팝업 규칙대로 맨 마지막 층 · 사용자 09-15)
            {
                let prefs = FontPrefs {
                    base: SlotFont {
                        size: self.settings.int("ui.menu_font_size") as f32,
                        bold: false,
                        italic: false,
                    },
                    ..FontPrefs::default()
                };
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s).with_fonts(prefs);
                self.menubar.paint(&mut dc, &th);
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
        // 찾기 바 — 마우스는 바 안일 때 · 키/문자는 포커스일 때.
        if self.find.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let in_bar = self.find.bounds().contains(cur);
            if (is_mouse && in_bar) || (self.focus == Focus::Find && !is_mouse) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) {
                    self.set_focus(Focus::Find);
                }
                let a = self.find.on_event(&ev);
                self.find_action(a);
                self.redraw();
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return;
                }
            }
        }
        // 결과 그리드 우클릭 메뉴가 열려 있으면 그리드가 먼저(바깥 클릭 = 닫고 통과).
        if self.grid.menu_open() {
            self.grid.on_event(&ev, self.scale);
            self.after_grid_event();
            if self.grid.menu_open() || !matches!(ev, InputEvent::MouseDown { .. }) {
                self.redraw();
                return;
            }
        }
        // ★ 오브젝트 탐색기 — 열린 메뉴는 먼저 · 마우스는 커서 아래 · 키는 포커스일 때.
        if self.explorer.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let in_exp = self.explorer.bounds().contains(cur);
            if self.explorer.menu_open() || (is_mouse && in_exp) || (is_wheel_ev(&ev) && in_exp) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) && !self.explorer.menu_open()
                {
                    self.set_focus(Focus::Explorer);
                }
                if self.explorer.on_event(&ev) {
                    self.redraw();
                }
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return;
                }
            } else if self.focus == Focus::Explorer && matches!(ev, InputEvent::Key { .. }) {
                if self.explorer.on_event(&ev) {
                    self.redraw();
                }
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
                    self.after_grid_event();
                    inv.push(self.grid.bounds);
                }
                Focus::Explorer | Focus::Find => {}
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
        if self.keys_win.tick(now_ms) {
            self.keys_win.redraw();
        }
        if self.prefs_win.tick(now_ms) {
            self.prefs_win.redraw();
        }
        if self.explorer.tick(now_ms) {
            self.redraw();
        }
        if self.find.tick(now_ms) {
            self.redraw();
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
            || self.keys_win.animating()
            || self.prefs_win.animating()
            || self.explorer.bars_visible()
            || self.find.animating()
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
        self.sync_conn_modal();
        // settings.json 감시(열어 둔 뒤 1초 폴링 · 저장 즉시 반영).
        if let Some(t) = self.json_tick(now) {
            next = next.min(t);
        }
        // Oracle 라이브 로그 폴링(실행 중에만 · 끝나면 마지막 1회).
        if let Some(t) = self.live_tick(now) {
            next = next.min(t);
        }
        el.set_control_flow(ControlFlow::WaitUntil(next));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::Focused(true)) {
            self.on_window_focused(id);
        }
        // ★ 접속 창 = 모달: 열려 있는 동안 **메인 창과 그 일부인 로그·색·단축키·설정 창**의 입력은 버리고
        //   (OS 수준은 `winfocus::set_enabled`) 접속 창을 앞으로(사용자 09-15 "로그 창도 메인의 일부").
        if self.conn_win.is_open()
            && !self.conn_win.is(id)
            && matches!(
                event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::Ime(_)
            )
        {
            if let Some(w) = self.conn_win.window() {
                w.focus_window();
            }
            return;
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
        if self.prefs_win.is(id) {
            let ui_px = self.settings.int("ui.font_size") as f32;
            match self.prefs_win.handle(&event) {
                PrefsAction::Paint => self.prefs_win.paint(&self.ui_font, &self.theme, ui_px),
                PrefsAction::Changed { key, value } => {
                    let ok = if value.is_empty()
                        && nsql_settings::entry(&key).is_some_and(|e| e.default.is_empty())
                    {
                        self.settings
                            .reset(&key)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    } else {
                        self.settings
                            .set(&key, &value)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    };
                    match ok {
                        Ok(()) => {
                            self.persist_settings();
                            if !self.apply_setting(&key) {
                                self.status = t(Msg::StNeedsRestart).into();
                            }
                            self.prefs_win.refresh(&self.settings);
                        }
                        Err(e) => self.prefs_win.set_error(&key, e),
                    }
                    self.prefs_win.redraw();
                }
                PrefsAction::Reset(key) => {
                    let _ = self.settings.reset(&key);
                    self.persist_settings();
                    if !self.apply_setting(&key) {
                        self.status = t(Msg::StNeedsRestart).into();
                    }
                    self.prefs_win.refresh(&self.settings);
                    self.prefs_win.redraw();
                }
                PrefsAction::OpenColors => self.open_colors = true,
                PrefsAction::OpenKeys => self.open_keys = true,
                PrefsAction::EditJson => self.edit_settings_json(),
                PrefsAction::None => {}
            }
            return;
        }
        if self.keys_win.is(id) {
            let ui_px = self.settings.int("ui.font_size") as f32;
            match self.keys_win.handle(&event) {
                KeysAction::Paint => self.keys_win.paint(&self.ui_font, &self.theme, ui_px),
                KeysAction::Changed { id, code } => {
                    let _ = self.settings.set(&keymap::setting_key(&id), &code);
                    let _ = self.settings.save();
                    self.keymap = Keymap::from_settings(&self.settings);
                    self.keys_win.refresh(&self.keymap);
                    self.keys_win.redraw();
                }
                KeysAction::ResetAll => {
                    for c in keymap::COMMANDS {
                        let _ = self.settings.reset(&keymap::setting_key(c.id));
                    }
                    let _ = self.settings.save();
                    self.keymap = Keymap::from_settings(&self.settings);
                    self.keys_win.refresh(&self.keymap);
                    self.keys_win.redraw();
                }
                KeysAction::None => {}
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
                self.alt = m.state().alt_key();
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
                // ★ 단축키 = 키맵 표 조회(Sublime 기본 · `key.*` 설정 · 사용자 09-15). 조합키 없는 글자는 타이핑이므로
                // 표에 있어도 가로채지 않는다(F-키·Enter 같은 이름 키는 예외).
                if let Some(ch) =
                    Chord::from_winit(&kev.logical_key, self.primary, self.shift, self.alt)
                {
                    let plain_char = !ch.primary && !ch.alt && ch.key.chars().count() == 1;
                    if !plain_char {
                        if let Some(id) = self.keymap.lookup(&ch) {
                            self.key_command(id, el);
                            return;
                        }
                    }
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
        if std::mem::take(&mut self.open_prefs) {
            let over = self.window.as_ref().and_then(|w| {
                let p = w.outer_position().ok()?;
                let sz = w.outer_size();
                Some((p.x, p.y, sz.width, sz.height))
            });
            let owner = self.window.clone();
            self.prefs_win.refresh(&self.settings);
            self.prefs_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                over,
                owner.as_deref(),
            );
        }
        if std::mem::take(&mut self.open_keys) {
            let over = self.window.as_ref().and_then(|w| {
                let p = w.outer_position().ok()?;
                let sz = w.outer_size();
                Some((p.x, p.y, sz.width, sz.height))
            });
            let owner = self.window.clone();
            self.keys_win.refresh(&self.keymap);
            self.keys_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                over,
                owner.as_deref(),
            );
        }
        if std::mem::take(&mut self.open_conn) {
            self.open_conn_window(el);
            self.sync_conn_modal();
        }
        if self.exit_requested {
            self.worker.send(worker::Cmd::Quit);
            el.exit();
        }
    }
}

/// OS 연결 프로그램으로 파일 열기(외부 crate 0 · 3-OS).
fn open_external(path: &std::path::Path) -> Result<(), String> {
    let p = path.display().to_string();
    let r = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &p])
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(&p).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(&p).spawn()
    };
    r.map(|_| ()).map_err(|e| e.to_string())
}

fn is_wheel_ev(ev: &InputEvent) -> bool {
    matches!(ev, InputEvent::Wheel { .. } | InputEvent::HWheel { .. })
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

/// 큐에 대기하는 시도(Test 또는 Connect).
enum Attempt {
    Test {
        name: String,
        spec: ConnectSpec,
    },
    Connect {
        #[allow(dead_code)]
        name: String,
        spec: ConnectSpec,
        reconnect_same: bool,
    },
}

/// 접속 문자열에 방언이 없을 때의 기본(워커 · 테스트 스레드 공통).
const DEFAULT_DIALECT: Dialect = Dialect::Oracle;

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
    let max_rows = settings.int("grid.max_rows").max(0) as usize;
    let autocommit = settings.flag("session.autocommit");
    let auto_reconnect = settings.flag("connect.auto_reconnect");
    let (worker, events) = worker::spawn(
        DEFAULT_DIALECT,
        max_rows,
        autocommit,
        auto_reconnect,
        Box::new(move || {
            let _ = proxy.send_event(Wake);
        }),
    );
    let probe_proxy: EventLoopProxy<Wake> = el.create_proxy();
    let probe_hub = probe::ProbeHub::spawn(
        Box::new(move || {
            let _ = probe_proxy.send_event(Wake);
        }),
        settings.int("probe.max_inflight").clamp(1, 64) as usize,
    );
    let (tests_tx, tests_rx) = mpsc::channel::<worker::TestResult>();
    let wake_proxy: EventLoopProxy<Wake> = el.create_proxy();
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
    let attempts_max = settings.int("connect.max_concurrent").clamp(1, 16) as usize;
    let keymap = Keymap::from_settings(&settings);
    let explorer = {
        let proxy = wake_proxy.clone();
        let mut e = Explorer::new(
            Box::new(move || {
                let _ = proxy.send_event(Wake);
            }),
            settings.flag("explorer.visible"),
        );
        e.set_icons(settings.flag("explorer.icons"));
        e
    };
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
        keymap,
        open_keys: false,
        keys_win: KeysWin::new(),
        open_prefs: false,
        prefs_win: PrefsWin::new(),
        menubar: MenuBar::new(App::build_menus()),
        toolbar: App::build_toolbar(),
        conn_win: ConnWin::new(panel),
        open_conn: true,
        run_btn: Button::new(t(Msg::BtnRun)),
        find: FindBar::new(),
        editors: Editors::new(ed_line_numbers, ed_multi, ed_tooltip, syntax_reg),
        grid: grid::Grid::default(),
        explorer,
        last_spec: None,
        dialect: DEFAULT_DIALECT,
        tx_dirty: false,
        live_sid: None,
        live_next: Instant::now(),
        live_since: None,
        live_last: String::new(),
        live_final: false,
        json_watch: None,
        conn_modal: false,
        json_next: Instant::now(),
        focus: Focus::Editor,
        worker,
        events,
        tests_tx,
        tests_rx,
        wake_proxy,
        attempt_queue: std::collections::VecDeque::new(),
        attempts_inflight: 0,
        attempts_max,
        busy: false,
        status,
        log: Vec::new(),
        cursor: (0, 0),
        shift: false,
        primary: false,
        alt: false,
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
    // 접속 창 조정값(비노출 설정 · 사용자 09-14 "구현 값은 설정으로").
    {
        let s = &app.settings;
        let i = |k: &str| s.int(k);
        app.conn_win.set_tuning(conn_win::ConnTuning {
            delete_confirm_ms: i("conn.delete_confirm_ms").max(0) as u64,
            close_after_ms: i("conn.close_after_connect_ms").max(0) as u64,
            tooltip_ms: i("ui.tooltip_delay_ms").max(0) as u128,
            dblclick_ms: i("ui.dblclick_ms").max(0) as u128,
            slide_ms: i("ui.slide_ms").max(0) as f32,
            window_w: i("conn.window_w").max(400) as f32,
            window_h: i("conn.window_h").max(300) as f32,
            panel_w: i("conn.panel_w").max(200) as f32,
            button_scale: i("conn.button_scale_pct").clamp(100, 250) as f32 / 100.0,
            port_w: i("conn.port_w").max(40) as f32,
        });
        nexa_ctl::tokens::set_intent_ms(i("ui.hover_intent_ms").clamp(0, 500) as u64);
        nexa_ctl::tokens::set_fade_out_ms(i("ui.fade_out_ms").clamp(0, 2000) as u32);
    }
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
