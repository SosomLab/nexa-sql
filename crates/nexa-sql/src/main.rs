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

mod activity;
mod clipboard;
mod colors_win;
mod conn_win;
mod connect;
mod editors;
mod eol;
mod exp_icons;
mod explorer;
mod file_win;
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
mod toast;
mod toolicons;
mod winfocus;
mod worker;

use activity::ActivityBar;
use colors_win::{ColorTarget, ColorsAction, ColorsWin};
use conn_win::{ConnWin, ConnWinAction, ConnectMark, TestMark};
use connect::{ConnState, ConnectPanel, PanelAction};
use editors::Editors;
use explorer::{Explorer, ExplorerAction, LiveReq};
use file_win::{FileWin, FileWinAction};
use findbar::{FindAction, FindBar};
use keymap::{Chord, Keymap};
use keys_win::{KeysAction, KeysWin};
use log_win::{LogWin, LogWinAction};
use nexa_ctl::controls::{SplitAxis, SplitEvent, Splitter};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{
    ComboItem, Control, EditCtxAction, InputEvent, Invalidations, Key as CtlKey, MenuBar, MenuDef,
    MenuEntry, TextBox, ToolItem, ToolTone, Toolbar, Widget,
};
use nexa_dlg::PickerMode;
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
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
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
    /// 우측 하단 토스트(오류 분류 · docs/42).
    toasts: toast::Toasts,
    /// 파일 싱크 허브(설정 `log.file` · 배경 스레드 · 비면 None).
    log_hub: Option<nsql_log::LogHub>,
    /// 파일 대화상자의 용도(편집기 열기/저장 · 로그 내보내기).
    file_purpose: FilePurpose,
    /// 마지막 실행의 문장 목록(오류 index → 문장 · 테이블 추정).
    last_run_items: Vec<String>,
    /// 메뉴에서 요청한 종료·로그 창 토글(이벤트 루프 핸들이 필요해 window_event 끝에서 처리).
    exit_requested: bool,
    palette: Palette,
    syntax: Rc<SyntaxRegistry>,
    /// 마지막 조회 결과(상태줄: 행 수 · 소요).
    last_rows: Option<usize>,
    last_secs: Option<f64>,
    /// 상태줄 구문 이름 영역(클릭 → 팔레트 `Set Syntax`).
    status_syntax_rect: Rect,
    /// 상태줄 들여쓰기 세그먼트(`Tab Size: 4`/`Spaces: 4` · 클릭 = 팝업 · T-69 1차).
    status_tab_rect: Rect,
    /// 상태줄 줄끝 세그먼트(`LF`/`CRLF` · 클릭 = 팝업 · docs/38).
    status_eol_rect: Rect,
    status_menu: nexa_ctl::controls::ctxmenu::ContextMenu,
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
    /// 좌측 활동 막대(VS Code식 · 사용자 09-15) — 패널 토글 + 동작 버튼.
    act_bar: ActivityBar,
    /// 파일 열기/저장 창(T-74 · 모달) + 열 요청(모드).
    file_win: FileWin,
    open_file_dlg: Option<PickerMode>,
    // 컨트롤
    menubar: MenuBar,
    toolbar: Toolbar,
    /// 접속 창(별도 창 · 폼 + 로그인 목록). 폼 상태의 단일 원천 = `conn_win.panel`.
    conn_win: ConnWin,
    /// 메뉴/툴바에서 접속 창 열기 요청(창 생성은 이벤트 루프 핸들에서).
    open_conn: bool,
    /// 찾기/바꾸기 바(편집기 위 · T-73).
    find: FindBar,
    editors: Editors,
    /// 활성 편집기 탭의 결과 그리드. 다른 탭의 그리드는 `grid_stash`에 잠들어 있다가 탭을 고르면 교체된다(사용자 09-16 "편집기와 결과는 쌍").
    grid: grid::Grid,
    /// 비활성 탭의 결과 그리드(탭 id → 그리드 · 탭이 닫히면 버림).
    grid_stash: HashMap<u64, grid::Grid>,
    /// 프레임 계측(`NSQL_TRACE_FRAMES=1` · docs/39 §6 `--trace-frames`) — 60프레임마다 stderr에 구간별 평균/최대(ms).
    frame_trace: Option<FrameTrace>,
    /// 테이블 키 캐시(접속당 · 표기 그대로 키) — Copy SQL의 키 조회 왕복을 테이블당 1회로(docs/41).
    key_cache: HashMap<String, Option<nsql_core::KeyInfo>>,
    /// 키 조회를 기다리는 SQL 복사 종류.
    sql_wait: Option<nsql_io::SqlKind>,
    /// `grid`가 속한 탭 id.
    grid_tab: u64,
    /// 마지막 실행을 시작한 탭 id — 결과는 실행 중 탭을 바꿔도 그 탭의 그리드로 간다.
    run_tab: u64,
    /// ★ 오브젝트 탐색기(사용자 09-15 · docs/28) — 메타 세션은 자기 스레드.
    explorer: Explorer,
    /// 스플리터 ① 탐색기|편집기(세로선) · ② 편집기|결과(가로선) — 사용자 09-16.
    split_v: Splitter,
    split_h: Splitter,
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

/// 스플리터 잡히는 띠 두께(논리 px).
const SPLIT_GRIP: f32 = 6.0;

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
        // `preferred_height`는 논리 px(nexa-ctl 규약) → 물리 px로. 그대로 쓰면 HiDPI에서 툴바가 1/배율로 납작해진다
        // (맥 2x 실기 09-16: Windows 100% 32px vs 맥 16px).
        let tool_h = px(self.toolbar.preferred_height() as f32, s);
        self.toolbar
            .set_bounds(Rect::new(0, menu_h, w, tool_h), &mut inv);
        let chrome_h = menu_h + tool_h;
        // Run 버튼 줄은 제거(사용자 09-15 — 툴바·단축키로 충분) · 접속 패널은 별도 창(conn_win) — 본문은 창 전폭.
        let _ = (panel_w, btn_w);
        let rx = pad;
        let rw = w - rx - pad;
        let body_top = chrome_h + px(4.0, s);
        let body_h = h - body_top - status_h;
        let _ = chrome_h;
        // 좌측 활동 막대(VS Code 48px) → 그 오른쪽에 패널(오브젝트 탐색기 · 보이면 본문을 그만큼 오른쪽으로).
        let act_w = px(activity::BAR_W, s);
        self.act_bar
            .set_bounds(Rect::new(0, body_top, act_w, body_h), s);
        self.act_bar.set_active(if self.explorer.is_visible() {
            Some("view.explorer")
        } else {
            None
        });
        let exp_w = if self.explorer.is_visible() {
            px(self.settings.int("explorer.width") as f32, s)
        } else {
            0
        };
        self.explorer
            .set_bounds(Rect::new(act_w, body_top, exp_w, body_h), s);
        let rx = rx + act_w + exp_w;
        let rw = rw - act_w - exp_w;
        // 스플리터 ① 탐색기|편집기 — 잡히는 띠 = 탐색기 오른쪽 경계 ±3 논리 px(탐색기 보일 때만 · 사용자 09-16).
        let grip = px(SPLIT_GRIP, s);
        self.split_v.set_rect(if exp_w > 0 {
            Rect::new(act_w + exp_w - grip / 2, body_top, grip, body_h)
        } else {
            Rect::new(0, 0, 0, 0)
        });
        // 편집기/결과 상하 비율 = `layout.editor_split_pct`(스플리터 ② 드래그가 갱신 · 자동 기억).
        let pct = self.settings.int("layout.editor_split_pct").clamp(10, 90) as f32 / 100.0;
        let editor_h = (body_h as f32 * pct) as i32;
        self.editors
            .set_bounds(Rect::new(rx, body_top, rw, editor_h - pad), s);
        // 찾기/바꾸기는 편집기 위에 떠 있는 패널(VS Code식 · 사용자 09-15) — 본문 배치 뒤에 그 위치를 잡는다.
        self.find.set_bounds(self.editors.editor_bounds(), s);
        // 스플리터 ② 편집기|결과 — 띠 = 편집기 아래 여백(pad) 자리.
        self.split_h
            .set_rect(Rect::new(rx, body_top + editor_h - pad, rw, pad.max(grip)));
        let gb = Rect::new(rx, body_top + editor_h, rw, body_h - editor_h - pad);
        self.all_grids().for_each(|g| g.set_bounds(gb));
        self.palette.set_bounds(w, chrome_h, s);
    }

    /// 스플리터 두 개에 마우스 사건을 준다. 소비(드래그 시작·중·끝)했으면 `true` — hover만 바뀐 경우는 다시 그리되 통과.
    fn route_splitters(&mut self, ev: &InputEvent) -> bool {
        let s = self.scale;
        match self.split_v.on_event(ev) {
            SplitEvent::None => {}
            SplitEvent::Hover => self.redraw(),
            SplitEvent::Start => return true,
            SplitEvent::Drag(x) => {
                // 띠 시작 x → 탐색기 폭(논리 px · 설정 범위로 클램프) → `explorer.width`(자동 기억).
                let grip = px(SPLIT_GRIP, s);
                let act_w = px(activity::BAR_W, s);
                let w = ((x + grip / 2 - act_w) as f32 / s).round() as i64;
                let _ = self
                    .settings
                    .set("explorer.width", &w.clamp(160, 800).to_string());
                self.layout();
                self.redraw();
                return true;
            }
            SplitEvent::End => {
                self.persist_settings();
                self.redraw();
                return true;
            }
        }
        match self.split_h.on_event(ev) {
            SplitEvent::None => false,
            SplitEvent::Hover => {
                self.redraw();
                false
            }
            SplitEvent::Start => true,
            SplitEvent::Drag(y) => {
                // 띠 시작 y = body_top + editor_h − pad → 편집기 비율(%) → `layout.editor_split_pct`.
                let pad = px(8.0, s);
                let body = self.act_bar.bounds();
                if body.h > 0 {
                    let editor_h = y + pad - body.y;
                    let pct = (editor_h as f32 / body.h as f32 * 100.0).round() as i64;
                    let _ = self
                        .settings
                        .set("layout.editor_split_pct", &pct.clamp(10, 90).to_string());
                    self.layout();
                    self.redraw();
                }
                true
            }
            SplitEvent::End => {
                self.persist_settings();
                self.redraw();
                true
            }
        }
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
            PanelAction::Disconnect => self.disconnect_now(),
            PanelAction::Save {
                name,
                spec,
                rename_from,
            } => {
                // 저장하지 않더라도 입력된 비밀번호는 세션에 보관(사용자 09-14).
                let typed = self.conn_win.panel.password_text();
                self.conn_win.remember_pw(&name, &typed);
                // ★ 저장은 파일 쓰기뿐 — 워커(순차 · Test/Connect 뒤에 줄 섬)를 거치지 않고 즉시(사용자 09-14
                //   "Save에서 접속 테스트를 하지 않도록": 실제로는 앞선 Test의 20초 타임아웃을 기다리던 것). 접속 검증 없음 · 포트가 틀려도 저장.
                // 이름 변경(사용자 09-16): 새 이름으로 저장이 성공한 뒤에만 옛 항목을 지운다(실패 시 옛 프로필 보존).
                let res = Vault::open_default().and_then(|v| {
                    v.save(&name, &spec)?;
                    if let Some(old) = &rename_from {
                        v.remove(old)?;
                    }
                    Ok(())
                });
                match res {
                    Ok(()) => {
                        self.status = tf(Msg::WkProfileSaved, &[&name, ""]);
                        // 폼은 이제 새 이름의 프로필을 "불러온" 상태 — 또 바꿔 저장하면 다시 이름 변경.
                        self.conn_win.panel.fill(&name, &spec);
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
        // 상세 패널이 닫혀 있어도 목록 오른쪽 아래에 같은 안내(사용자 09-16).
        self.conn_win.set_note(name, st.clone());
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
                    // 상태줄·패널은 프로필 이름으로 간략하게(사용자 09-16) · 접속 문자열 상세는 위 로그 창에.
                    let label = if name.is_empty() { &description } else { &name };
                    let msg = tf(Msg::StTestOk, &[label, &elapsed_s]);
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
                    self.key_cache.clear();
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
                ConnOutcome::Keys(table, info) => {
                    self.key_cache.insert(table, info.clone());
                    if let Some(kind) = self.sql_wait.take() {
                        self.finish_sql_copy(kind, info.as_ref());
                    }
                }
                ConnOutcome::Disconnected => self.on_conn_disconnected(),
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
        let ww = self.find.whole_word();
        let eq = |a: char, b: char| {
            if cs {
                a == b
            } else {
                a.to_lowercase().eq(b.to_lowercase())
            }
        };
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let mut out = Vec::new();
        let mut i = 0;
        while i + q.len() <= text.len() {
            if text[i..i + q.len()].iter().zip(&q).all(|(a, b)| eq(*a, *b)) {
                // 단어 단위(ab 토글): 앞뒤가 단어 문자면 일치가 아니다.
                let boundary_ok = !ww
                    || ((i == 0 || !is_word(text[i - 1]))
                        && (i + q.len() >= text.len() || !is_word(text[i + q.len()])));
                if boundary_ok {
                    out.push((i, i + q.len()));
                    i += q.len();
                    continue;
                }
            }
            i += 1;
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
            FindAction::Changed => {
                // 펼침 토글은 패널 높이가 바뀐다 — 다시 배치.
                self.layout();
                self.find_step(true, false);
            }
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
    /// `settings.json_editor`: external = OS 연결 프로그램 · builtin = 편집기 탭(T-76 1차 · 09-16). 둘 다 저장 감시로 반영.
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
        if builtin {
            // T-76 1차(09-16): 편집기 탭으로 연다 — Ctrl+S 저장은 파일 쓰기(T-74) → 위의 1초 감시가 바뀐 키를 반영한다
            // (외부 편집기와 같은 경로 · 별도 훅 없음).
            self.open_file_enc(&path, "utf8");
            self.status = tf(Msg::StJsonOpened, &[&path.display().to_string()]);
            self.redraw();
            return;
        }
        match open_external(&path) {
            Ok(()) => self.status = tf(Msg::StJsonOpened, &[&path.display().to_string()]),
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

    /// 모달 창(접속 · 파일) 열림/닫힘 전환 → 메인 창 활성 상태 동기화(닫히면 메인으로 포커스).
    fn sync_modal(&mut self) {
        let open = self.conn_win.is_open() || self.file_win.is_open();
        if open == self.conn_modal && !open {
            return;
        }
        self.conn_modal = open;
        if !open {
            // 닫힌 모달 창의 WindowId는 z-order 목록에서 걷어낸다(열 때마다 새 id → 남겨 두면 한 칸씩 자란다 · 09-15 누수 점검).
            let live: Vec<WindowId> = [
                self.window.as_deref(),
                self.log_win.window(),
                self.colors_win.window(),
                self.keys_win.window(),
                self.prefs_win.window(),
                self.conn_win.window(),
                self.file_win.window(),
            ]
            .into_iter()
            .flatten()
            .map(Window::id)
            .collect();
            self.z_order.retain(|id| live.contains(id));
        }
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

    /// 들여쓰기 팝업(Sublime 상태바 클릭과 같은 항목 · docs/31 §2 1차): 공백/탭 · 탭 폭 1~8 · 변환.
    /// ★ 고른 값은 **활성 탭에만** 적용한다(탭마다 다를 수 있다 · 설정은 새 탭의 기본값 · 사용자 09-15).
    fn open_indent_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let (tsz, spaces) = self.editors.indent();
        let ts = i64::from(tsz);
        let mark = |on: bool, s: String| {
            if on {
                format!("✓ {s}")
            } else {
                format!("   {s}")
            }
        };
        let mut items = vec![
            CtxItem::item(
                "indent.spaces",
                mark(spaces, t(Msg::MnIndentSpaces).to_string()),
            ),
            CtxItem::item(
                "indent.tabs",
                mark(!spaces, t(Msg::MnIndentTabs).to_string()),
            ),
        ];
        for n in 1..=8 {
            items.push(CtxItem::item(
                format!("indent.size:{n}"),
                mark(n == ts, tf(Msg::MnTabWidth, &[&n.to_string()])),
            ));
        }
        items.push(CtxItem::item(
            "indent.to_spaces",
            format!("   {}", t(Msg::MnConvertToSpaces)),
        ));
        items.push(CtxItem::item(
            "indent.to_tabs",
            format!("   {}", t(Msg::MnConvertToTabs)),
        ));
        let r = self.status_tab_rect;
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(r);
        // 상태줄 위로 열리도록 세그먼트 상단 기준(팝업 부품이 화면 안에 맞춘다).
        // 배율 전달(빠져 있어 맥 2x에서 항목 간격이 절반이었다 · 09-16).
        self.status_menu.set_scale(self.scale);
        self.status_menu
            .open_at(r.x, r.y, items, host, px(240.0, self.scale));
    }

    /// 상태줄 줄끝 팝업(LF/CRLF · 현재 = ✓) — 고르면 활성 탭 줄끝 변경(저장 때 반영 · docs/38).
    fn open_eol_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let crlf = self.editors.active_crlf();
        let mark = |on: bool, s: &str| {
            if on {
                format!("✓ {s}")
            } else {
                format!("   {s}")
            }
        };
        let items = vec![
            CtxItem::item("eol.lf", mark(!crlf, "LF")),
            CtxItem::item("eol.crlf", mark(crlf, "CRLF")),
        ];
        let r = self.status_eol_rect;
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(r);
        self.status_menu.set_scale(self.scale);
        self.status_menu
            .open_at(r.x, r.y, items, host, px(120.0, self.scale));
    }

    fn indent_pick(&mut self, id: &str) {
        let (ts, spaces) = self.editors.indent();
        match id {
            "eol.lf" => self.editors.set_active_crlf(false),
            "eol.crlf" => self.editors.set_active_crlf(true),
            "indent.spaces" | "indent.tabs" => {
                self.editors.set_tab_indent(ts, id == "indent.spaces");
            }
            "indent.to_spaces" => self.editors.convert_indent(true),
            "indent.to_tabs" => self.editors.convert_indent(false),
            s if s.starts_with("indent.size:") => {
                let n = s["indent.size:".len()..]
                    .parse::<u8>()
                    .unwrap_or(4)
                    .clamp(1, 8);
                self.editors.set_tab_indent(n, spaces);
            }
            _ => return,
        }
        self.redraw();
    }

    /// 설정 → nexa-gfx 탭 폭 + 편집기 들여쓰기.
    fn apply_indent(&mut self) {
        let ts = self.settings.int("editor.tab_size").clamp(1, 8);
        nexa_gfx::text::set_tab_cols(ts as u32);
        let stops = self.settings.get("editor.tab_stops") != Some("fixed");
        nexa_gfx::text::set_tab_stops(stops);
        self.editors.set_tab_stops(stops);
        self.editors
            .set_indent(ts as u8, self.settings.flag("editor.indent_spaces"));
        self.redraw();
    }

    /// DB 워커 하나 시작(설정 현재값 · 시작 때와 같은 인자).
    fn spawn_worker(&self) -> (worker::Handle, mpsc::Receiver<RunEvent>) {
        let proxy = self.wake_proxy.clone();
        worker::spawn(
            DEFAULT_DIALECT,
            self.settings.int("grid.max_rows").max(0) as usize,
            self.settings.flag("session.autocommit"),
            self.settings.flag("connect.auto_reconnect"),
            Box::new(move || {
                let _ = proxy.send_event(Wake);
            }),
        )
    }

    /// ★ 접속 해제 — **서버 상태와 무관하게 즉시**(사용자 09-16: VPN 끊긴 채 조회가 "Loading…"에 멈추면 Disconnect가
    /// 무반응이었다). 워커는 순차라 앞 명령(실행 · 세션 commit/close)이 응답 없는 서버에서 TCP 타임아웃까지 갇힌다 →
    /// 기다리지 않는다: 옛 워커에는 Disconnect를 남기고 손잡이를 버린다(갇힌 호출이 풀리면 세션을 닫고 스스로 끝난다 ·
    /// 늦게 오는 이벤트는 버려진 채널로 사라진다) · 새 워커를 만들어 다음 접속을 받는다 · UI는 지금 해제 상태로.
    /// 탐색기 메타 스레드도 같은 방식(`Explorer::disconnect`).
    fn disconnect_now(&mut self) {
        let stuck = self.busy;
        self.worker.send(worker::Cmd::Disconnect);
        let (w, ev) = self.spawn_worker();
        self.worker = w;
        self.events = ev;
        self.busy = false;
        self.status = t(if stuck {
            Msg::StDisconnectedAbandon
        } else {
            Msg::StDisconnected
        })
        .into();
        if stuck {
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.status.clone()));
        }
        self.explorer.disconnect();
        self.tx_dirty = false;
        self.sync_disconnect_btn(false);
        self.on_conn_disconnected();
        self.redraw();
    }

    /// 접속 계열 결과 `Disconnected`의 UI 반영(워커 이벤트 · 즉시 해제 공용).
    fn on_conn_disconnected(&mut self) {
        self.live_sid = None;
        self.key_cache.clear();
        self.sql_wait = None;
        self.editors.set_conn_desc("");
        self.conn_win.clear_connect_marks();
        self.conn_win.clear_active();
        self.panel_op = None;
        self.conn_win.panel.set_state(ConnState::Idle);
    }

    /// 파일 싱크 허브(설정 `log.file` · 형식 `log.file_format`(same = 창과 같게) · 회전 `log.file_max_kb`) — 설정이 바뀌면 새로.
    fn rebuild_log_hub(&mut self) {
        self.log_hub = None;
        let file = self
            .settings
            .get("log.file")
            .unwrap_or("")
            .trim()
            .to_string();
        if file.is_empty() {
            return;
        }
        let name = self.settings.get("log.format").unwrap_or("raw").to_string();
        let ff = self
            .settings
            .get("log.file_format")
            .unwrap_or("same")
            .to_string();
        let ff = if ff == "same" { name } else { ff };
        let tpl = self.settings.get("log.template").unwrap_or("").to_string();
        let cols = nsql_log::Columns::parse(self.settings.get("log.columns").unwrap_or(""));
        let max = self.settings.int("log.file_max_kb").max(64) as u64 * 1024;
        match nsql_log::FileSink::open(&file, nsql_log::formatter_with(&ff, &tpl, cols), max) {
            Ok(sink) => self.log_hub = Some(nsql_log::LogHub::spawn(vec![Box::new(sink)], 4096)),
            Err(e) => {
                self.status = tf(Msg::ErrLogFile, &[&e.to_string()]);
                self.log_win.push(LogEntry::new(
                    LogKind::Error,
                    tf(Msg::ErrLogFile, &[&e.to_string()]),
                ));
            }
        }
    }

    /// 메인 창 항상 위(설정 `window.always_on_top`). 로그 창은 메인 창의 소유 창이라 둘 다 켜도 로그가 위(Windows/mac 소유 규칙 · 사용자 09-16).
    fn apply_on_top(&self) {
        if let Some(w) = &self.window {
            w.set_window_level(if self.settings.flag("window.always_on_top") {
                winit::window::WindowLevel::AlwaysOnTop
            } else {
                winit::window::WindowLevel::Normal
            });
        }
    }

    /// 툴바 접속 해제 버튼 = 접속돼 있을 때만 활성(사용자 09-15).
    fn sync_disconnect_btn(&mut self, connected: bool) {
        let mut inv = Invalidations::default();
        self.toolbar
            .set_item_enabled("conn.disconnect", connected, &mut inv);
        // 연결이 하나라도 있으면 Connect 아이콘 = 밝은 녹색(사용자 09-16).
        self.toolbar.set_item_tone(
            "conn.toggle",
            if connected {
                ToolTone::Ok
            } else {
                ToolTone::Default
            },
            &mut inv,
        );
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
            "ui.toast_secs" | "ui.toast_alpha" => self.toasts.configure(
                self.settings.int("ui.toast_secs"),
                self.settings.int("ui.toast_alpha"),
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
            "grid.row_numbers" => {
                let on = self.settings.flag(key);
                self.all_grids().for_each(|g| g.set_row_numbers(on));
            }
            "grid.copy_null" => {
                let on = self.settings.flag(key);
                self.all_grids().for_each(|g| g.set_copy_null(on));
            }
            "window.always_on_top" => self.apply_on_top(),
            "log.always_on_top" => self.log_win.set_on_top(self.settings.flag(key)),
            "grid.scroll" => self
                .grid
                .set_row_snap(self.settings.get(key) == Some("row")),
            "editor.line_numbers" => self.editors.set_line_numbers(self.settings.flag(key)),
            "editor.tab_size" | "editor.indent_spaces" | "editor.tab_stops" => self.apply_indent(),
            "file.eol_new" => self
                .editors
                .set_default_crlf(eol::default_crlf(self.settings.get(key).unwrap_or("auto"))),
            "editor.rulers" => self
                .editors
                .set_rulers(parse_rulers(self.settings.get(key).unwrap_or("80"))),
            "tabs.tooltip" => self.editors.set_tooltip(self.settings.flag(key)),
            "log.template" => self
                .log_win
                .set_template(self.settings.get(key).unwrap_or("")),
            "log.columns" => self
                .log_win
                .set_columns(self.settings.get(key).unwrap_or("")),
            "log.kinds" => self.log_win.set_kinds(self.settings.get(key).unwrap_or("")),
            "log.file" | "log.file_format" | "log.file_max_kb" => self.rebuild_log_hub(),
            "log.switch_scale" => self.log_win.set_switch_scale(self.settings.int(key)),
            "log.wrap" => self.log_win.set_wrap(self.settings.flag(key)),
            "log.newest_first" => self.log_win.set_newest_first(self.settings.flag(key)),
            "log.autoscroll" => self.log_win.set_autoscroll(self.settings.flag(key)),
            "log.format" => self
                .log_win
                .set_format(self.settings.get(key).unwrap_or("raw")),
            k if k.starts_with("key.") => {
                self.keymap = Keymap::from_settings(&self.settings);
                self.apply_menu_decor();
                self.keys_win.refresh(&self.keymap);
            }
            k if k.starts_with("editor.whitespace") || k.starts_with("editor.show_") => {
                self.editors
                    .set_whitespace(whitespace_style(&self.settings));
            }
            "ui.font_size"
            | "ui.menu_font_size"
            | "editor.font_size"
            | "grid.font_size"
            | "explorer.width"
            | "explorer.font_size"
            | "layout.editor_split_pct" => {
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
        if let Some(kind) = self.grid.take_pending_sql() {
            self.begin_sql_copy(kind);
        }
    }

    /// 설정 `sql.key_mode`.
    fn key_mode(&self) -> nsql_io::KeyMode {
        self.settings
            .get("sql.key_mode")
            .and_then(nsql_io::KeyMode::parse)
            .unwrap_or(nsql_io::KeyMode::Pk)
    }

    /// ★ Copy SQL(docs/41): 키 = 설정(pk: 카탈로그 PK → 유니크 → 앞 3컬럼 + 경고 1회 · all: 전체 컬럼). 카탈로그는 워커에
    ///   1회 묻고(테이블당 캐시) 답이 오면 완성한다 — UI 스레드는 디스크·네트워크를 만지지 않는다.
    fn begin_sql_copy(&mut self, kind: nsql_io::SqlKind) {
        if self.key_mode() == nsql_io::KeyMode::All {
            self.finish_sql_copy(kind, None);
            return;
        }
        let Some(guess) = self.grid.source_table() else {
            self.finish_sql_copy(kind, None);
            return;
        };
        if let Some(info) = self.key_cache.get(&guess) {
            let info = info.clone();
            self.finish_sql_copy(kind, info.as_ref());
            return;
        }
        let (schema, table) = nsql_io::split_table(self.dialect, &guess);
        self.sql_wait = Some(kind);
        self.worker.send(worker::Cmd::Keys {
            key: guess,
            schema,
            table,
        });
    }

    /// 키 정보로 문장을 만들어 클립보드에 · 대체 키/테이블 미추정은 상태줄 + 로그에 1회 경고.
    fn finish_sql_copy(&mut self, kind: nsql_io::SqlKind, info: Option<&nsql_core::KeyInfo>) {
        let names = self.grid.selected_col_names();
        let key = nsql_io::choose_key(self.key_mode(), info, &names);
        let Some((text, n)) = self.grid.copy_sql(kind, &key) else {
            return;
        };
        if !clipboard::write_text(&text) {
            self.status = t(Msg::ErrClipboard).into();
            return;
        }
        self.status = tf(Msg::StCopied, &[&n.to_string()]);
        let table = self.grid.source_table();
        let mut warns: Vec<String> = Vec::new();
        if table.is_none() {
            warns.push(t(Msg::SqlKeyWarnNoTable).to_string());
        }
        if key.needs_warning() && kind != nsql_io::SqlKind::Insert {
            warns.push(tf(
                Msg::SqlKeyWarnFirstN,
                &[
                    table.as_deref().unwrap_or("T"),
                    &key.cols.len().to_string(),
                    &key.cols.join(", "),
                ],
            ));
        }
        for w in warns {
            self.status = w.clone();
            self.log_win.push(LogEntry::new(LogKind::Error, w));
        }
        self.redraw();
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
            "view.on_top" => {
                let on = !self.settings.flag("window.always_on_top");
                let _ = self
                    .settings
                    .set("window.always_on_top", if on { "on" } else { "off" });
                let _ = self.settings.save();
                self.apply_on_top();
                self.status = tf(Msg::StOnTop, &[if on { "on" } else { "off" }]);
                self.redraw();
            }
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
            // ★ 파일 열기/저장(T-74) — 자체 대화상자(nexa-dlg) · 네이티브 0.
            "file.open" => self.open_file_dlg = Some(PickerMode::Open),
            "file.save" => match self.editors.active_path() {
                Some(p) => self.save_to(&p),
                None => self.open_file_dlg = Some(PickerMode::Save),
            },
            "file.save_as" => self.open_file_dlg = Some(PickerMode::Save),
            id if id.starts_with("file.recent:") => {
                let i: usize = id["file.recent:".len()..].parse().unwrap_or(usize::MAX);
                if let Some(p) = self.recent_files().get(i).cloned() {
                    self.open_file(&p);
                }
            }
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
                let _ = self.find.with_replace();
                self.layout();
                self.set_focus(Focus::Find);
                self.find_step(true, false);
            }
            // ★ Sublime Ctrl+D — 캐럿 밑 단어 → 다음 출현을 추가 선택(다중 커서 · 09-15).
            "edit.expand_selection" => {
                if self.editors.cur_mut().select_next_occurrence() {
                    let n = self.editors.selection_count();
                    if n > 1 {
                        self.status = tf(Msg::StSelections, &[&n.to_string()]);
                    }
                }
                self.set_focus(Focus::Editor);
            }
            // 같은 문자열 전부 선택(Sublime Ctrl+⇧D 계열 · 상한 = 더 못 찾을 때까지).
            "edit.select_all_occurrences" => {
                let ed = self.editors.cur_mut();
                if ed.select_next_occurrence() {
                    while ed.select_next_occurrence() {}
                }
                let n = self.editors.selection_count();
                self.status = tf(Msg::StSelections, &[&n.to_string()]);
                self.set_focus(Focus::Editor);
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
            "conn.disconnect" => self.disconnect_now(),
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
        Self::build_menus_with(&[])
    }

    /// 메뉴 정의 — File 메뉴 아래쪽에 최근 파일(최대 8 · Eclipse/DBeaver 관례).
    fn build_menus_with(recent: &[PathBuf]) -> Vec<MenuDef> {
        let item = |id: &str, m: Msg| MenuEntry::Item(ComboItem::new(id, t(m)));
        let mut file = vec![
            item("file.new", Msg::MnNew),
            item("file.open", Msg::MnOpen),
            item("file.save", Msg::MnSave),
            item("file.save_as", Msg::MnSaveAs),
            MenuEntry::Separator,
            item("file.close_tab", Msg::MnCloseTab),
        ];
        if !recent.is_empty() {
            file.push(MenuEntry::Separator);
            for (i, p) in recent.iter().take(8).enumerate() {
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let label = format!("{}  {}", name, nexa_fs::path::display(p));
                file.push(MenuEntry::Item(ComboItem::new(
                    format!("file.recent:{i}"),
                    label,
                )));
            }
        }
        file.push(MenuEntry::Separator);
        file.push(item("file.exit", Msg::MnExit));
        vec![
            MenuDef::new(t(Msg::MnFile), file),
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
                    item("edit.expand_selection", Msg::MnExpandSelection),
                    item("edit.select_all_occurrences", Msg::MnSelectAllOccurrences),
                    MenuEntry::Separator,
                    item("edit.prefs", Msg::MnPreferences),
                ],
            ),
            MenuDef::new(
                t(Msg::MnView),
                vec![
                    item("view.palette", Msg::MnCommandPalette),
                    item("view.explorer", Msg::MnExplorer),
                    item("view.log", Msg::MnLogWindow),
                    item("view.on_top", Msg::MnAlwaysOnTop),
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
            ToolItem::new("file.open", toolicons::open_file()).tip(t(Msg::TipOpen)),
            ToolItem::new("file.save", toolicons::save_file()).tip(t(Msg::TipSave)),
            // 다른 이름으로 저장 — 사용자가 Material `save_as` 아이콘을 준 09-16(툴바에 없던 항목 · 명령 id는 메뉴와 동일).
            ToolItem::new("file.save_as", toolicons::save_as()).tip(t(Msg::TipSaveAs)),
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
        cmds.push(m("file.open", Msg::MnFile, Msg::MnOpen));
        cmds.push(m("file.save", Msg::MnFile, Msg::MnSave));
        cmds.push(m("file.save_as", Msg::MnFile, Msg::MnSaveAs));
        cmds.push(m("file.exit", Msg::MnFile, Msg::MnExit));
        cmds.push(m("edit.cut", Msg::MnEdit, Msg::MnCut));
        cmds.push(m("edit.copy", Msg::MnEdit, Msg::MnCopy));
        cmds.push(m("edit.paste", Msg::MnEdit, Msg::MnPaste));
        cmds.push(m("edit.select_all", Msg::MnEdit, Msg::MnSelectAll));
        cmds.push(m(
            "edit.expand_selection",
            Msg::MnEdit,
            Msg::MnExpandSelection,
        ));
        cmds.push(m(
            "edit.select_all_occurrences",
            Msg::MnEdit,
            Msg::MnSelectAllOccurrences,
        ));
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

    // ───────────────────────── 파일 열기/저장(T-74) ─────────────────────────

    /// 최근 파일(설정 `file.recent` · `|` 구분 · 최신 먼저 · 존재하는 것만).
    fn recent_files(&self) -> Vec<PathBuf> {
        self.settings
            .get("file.recent")
            .unwrap_or("")
            .split('|')
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .take(8)
            .collect()
    }

    fn push_recent(&mut self, path: &Path) {
        let mut v = self.recent_files();
        v.retain(|p| p != path);
        v.insert(0, path.to_path_buf());
        v.truncate(8);
        let joined = v
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("|");
        let _ = self.settings.set("file.recent", &joined);
        self.persist_settings();
        self.menubar.set_menus(App::build_menus_with(&v));
    }

    /// 대화상자를 닫을 때 마지막 폴더·숨김 표시를 기억한다.
    fn remember_file_dialog(&mut self, dir: Option<&Path>) {
        let dir = dir
            .map(Path::to_path_buf)
            .or_else(|| self.file_win.current_dir());
        if let Some(d) = dir {
            let _ = self.settings.set("file.last_dir", &d.to_string_lossy());
        }
        if let Some(h) = self.file_win.show_hidden() {
            let _ = self
                .settings
                .set("file.show_hidden", if h { "on" } else { "off" });
        }
        if let Some(d) = self.file_win.show_dot() {
            let _ = self
                .settings
                .set("file.show_dot", if d { "on" } else { "off" });
        }
        self.persist_settings();
    }

    fn open_file_window(&mut self, el: &ActiveEventLoop, mode: PickerMode) {
        let over = self.window.as_ref().and_then(|w| {
            let p = w.outer_position().ok()?;
            let sz = w.outer_size();
            Some((p.x, p.y, sz.width, sz.height))
        });
        let owner = self.window.clone();
        // 시작 폴더 = 활성 탭 파일의 폴더 → 마지막 폴더 → 홈.
        let start = self
            .editors
            .active_path()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| {
                self.settings
                    .get("file.last_dir")
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
            });
        let default_name = match (mode, self.file_purpose) {
            (PickerMode::Save, FilePurpose::LogExport) => {
                let ext = match self.settings.get("log.format").unwrap_or("raw") {
                    "markdown" => "md",
                    "jsonl" => "jsonl",
                    "csv" => "csv",
                    "tsv" => "tsv",
                    _ => "log",
                };
                format!("nexa-sql.{ext}")
            }
            (PickerMode::Save, FilePurpose::Editor) => {
                let t = self.editors.active_title();
                if t.contains('.') {
                    t
                } else {
                    format!("{t}.sql")
                }
            }
            (PickerMode::Open, _) => String::new(),
        };
        let recent_dirs: Vec<PathBuf> = self
            .recent_files()
            .iter()
            .filter_map(|p| p.parent().map(Path::to_path_buf))
            .fold(Vec::new(), |mut acc, d| {
                if !acc.contains(&d) {
                    acc.push(d);
                }
                acc
            });
        let show_hidden = self.settings.flag("file.show_hidden");
        let show_dot = self.settings.flag("file.show_dot");
        let encoding = match mode {
            PickerMode::Open => "auto".to_string(),
            PickerMode::Save => self.editors.active_encoding(),
        };
        self.file_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            over,
            owner.as_deref(),
            mode,
            start.as_deref(),
            &default_name,
            recent_dirs,
            show_hidden,
            show_dot,
            &encoding,
        );
    }

    /// 파일 → 탭(자동 감지: BOM으로 UTF-8/UTF-16 판별 · 없으면 UTF-8).
    fn open_file(&mut self, path: &Path) {
        self.open_file_enc(path, "auto");
    }

    /// 바이트 → 문자열(인코딩 지정 · `auto` = BOM 감지). 돌려주는 값 = (본문, 대체 문자 발생, 실제 인코딩).
    fn decode_bytes(bytes: &[u8], enc: &str) -> (String, bool, &'static str) {
        let enc = if enc == "auto" {
            if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
                "utf8bom"
            } else if bytes.starts_with(&[0xFF, 0xFE]) {
                "utf16le"
            } else if bytes.starts_with(&[0xFE, 0xFF]) {
                "utf16be"
            } else {
                "utf8"
            }
        } else {
            enc
        };
        match enc {
            "utf16le" | "utf16be" => {
                let le = enc == "utf16le";
                let body = if (le && bytes.starts_with(&[0xFF, 0xFE]))
                    || (!le && bytes.starts_with(&[0xFE, 0xFF]))
                {
                    &bytes[2..]
                } else {
                    bytes
                };
                let units: Vec<u16> = body
                    .chunks(2)
                    .map(|c| {
                        let (a, b) = (c[0], c.get(1).copied().unwrap_or(0));
                        if le {
                            u16::from_le_bytes([a, b])
                        } else {
                            u16::from_be_bytes([a, b])
                        }
                    })
                    .collect();
                let text = String::from_utf16_lossy(&units);
                let lossy = text.contains('\u{FFFD}');
                (text, lossy, if le { "utf16le" } else { "utf16be" })
            }
            "utf8bom" => {
                let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
                match std::str::from_utf8(body) {
                    Ok(s) => (s.to_string(), false, "utf8bom"),
                    Err(_) => (String::from_utf8_lossy(body).into_owned(), true, "utf8bom"),
                }
            }
            _ => match std::str::from_utf8(bytes) {
                Ok(s) => (s.to_string(), false, "utf8"),
                Err(_) => (String::from_utf8_lossy(bytes).into_owned(), true, "utf8"),
            },
        }
    }

    /// 문자열 → 바이트(탭 인코딩).
    fn encode_text(text: &str, enc: &str) -> Vec<u8> {
        match enc {
            "utf8bom" => {
                let mut v = vec![0xEF, 0xBB, 0xBF];
                v.extend_from_slice(text.as_bytes());
                v
            }
            "utf16le" => {
                let mut v = vec![0xFF, 0xFE];
                for u in text.encode_utf16() {
                    v.extend_from_slice(&u.to_le_bytes());
                }
                v
            }
            "utf16be" => {
                let mut v = vec![0xFE, 0xFF];
                for u in text.encode_utf16() {
                    v.extend_from_slice(&u.to_be_bytes());
                }
                v
            }
            _ => text.as_bytes().to_vec(),
        }
    }

    /// 파일 → 탭(인코딩 지정). 깨진 바이트는 대체 문자 + 안내 · `\r\n`은 `\n`으로(저장 때 되돌린다).
    fn open_file_enc(&mut self, path: &Path, enc: &str) {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.status = tf(
                    Msg::StFileReadError,
                    &[&path.display().to_string(), &e.to_string()],
                );
                self.redraw();
                return;
            }
        };
        let (text, lossy, used) = Self::decode_bytes(&bytes, enc);
        // 줄끝 다수결 판정 + `\n` 정규화(docs/38).
        let (crlf, text) = eol::detect(&text);
        self.editors.open_file(path, &text, crlf);
        self.editors.set_active_encoding(used);
        self.set_focus(Focus::Editor);
        self.push_recent(path);
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.status = if lossy {
            tf(Msg::StFileDecodedLossy, &[&name])
        } else {
            tf(Msg::StFileOpened, &[&name])
        };
        self.layout();
        self.redraw();
    }

    /// 활성 탭 → 파일(UTF-8 · BOM 없음 · 원래 줄끝 유지).
    fn save_to(&mut self, path: &Path) {
        // 저장 줄끝 = 설정 `file.eol_save`(keep = 탭 줄끝) · docs/38.
        let crlf = eol::save_crlf(
            self.settings.get("file.eol_save").unwrap_or("keep"),
            self.editors.active_crlf(),
        );
        let text = eol::apply(&self.editors.cur().text(), crlf);
        let enc = self.editors.active_encoding();
        let text = Self::encode_text(&text, &enc);
        let tmp = path.with_extension(format!(
            "{}.nsql-tmp",
            path.extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        let res = std::fs::write(&tmp, &text).and_then(|()| std::fs::rename(&tmp, path));
        match res {
            Ok(()) => {
                self.editors.mark_saved(path);
                self.push_recent(path);
                let name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.status = tf(Msg::StFileSaved, &[&name]);
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.status = tf(
                    Msg::StFileWriteError,
                    &[&path.display().to_string(), &e.to_string()],
                );
            }
        }
        self.redraw();
    }

    /// 접속 창 열기(메인 창 위 가운데) — 이미 열려 있으면 앞으로.
    /// 우클릭 편집 메뉴(nexa-ctl 내장)의 아이콘·단축키 + 그리드 메뉴 단축키 — 부팅·키맵 변경 때(사용자 09-15 "기본 기능에도 이미지").
    fn apply_menu_decor(&mut self) {
        nexa_ctl::controls::set_edit_menu_decor(nexa_ctl::controls::EditMenuDecor {
            icons: [
                Some(toolicons::mi_copy()),
                Some(toolicons::mi_cut()),
                Some(toolicons::mi_paste()),
                Some(toolicons::mi_select_all()),
            ],
            shortcuts: [
                self.keymap.display_of("edit.copy"),
                self.keymap.display_of("edit.cut"),
                self.keymap.display_of("edit.paste"),
                self.keymap.display_of("edit.select_all"),
            ],
        });
        let (sc_copy, sc_all) = (
            self.keymap.display_of("edit.copy"),
            self.keymap.display_of("edit.select_all"),
        );
        self.all_grids()
            .for_each(|g| g.set_shortcuts(sc_copy.clone(), sc_all.clone()));
    }

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
            // 시도 자체를 신호등 대상으로 기록(성공 여부 무관 · 사용자 09-16).
            match &a {
                Attempt::Test { name, .. } | Attempt::Connect { name, .. } => {
                    self.conn_win.mark_attempted(name);
                }
            }
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
        self.menubar
            .set_menus(App::build_menus_with(&self.recent_files()));
        self.toolbar = App::build_toolbar();
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
        self.grid.set_source_sql(&src);
        self.run_tab = self.editors.active_id();
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
        self.last_run_items = split_items(&src);
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
        self.last_run_items = split_items(&src);
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
                if let Some(h) = &self.log_hub {
                    h.push(e.clone());
                }
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
                    // 결과는 실행을 시작한 탭의 그리드로(탭이 이미 닫혔으면 버림).
                    if let Some(g) = self.run_grid() {
                        g.set_result(rs);
                    }
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
                    self.all_grids().for_each(|g| g.set_dialect(dialect));
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
                RunEvent::Error { index, line, error } => {
                    // 공통 분류 + 코드 부각(docs/42): 상태줄 · 결과 메시지 · 로그 창 · 토스트(분류된 오류만).
                    let stmt = self.last_run_items.get(index).cloned().unwrap_or_default();
                    let (cls, summary) =
                        toast::summarize(self.dialect, error.code, &error.message, &stmt);
                    self.status = tf(Msg::StErrorLine, &[&line.to_string(), &summary]);
                    self.log.push(self.status.clone());
                    if let Some(label) = toast::class_label(cls.class) {
                        let title = match &cls.code {
                            Some(c) => format!("{c} · {label}"),
                            None => label.to_string(),
                        };
                        let body = cls.object.clone().unwrap_or_else(|| {
                            error.message.lines().next().unwrap_or("").to_string()
                        });
                        self.toasts.push(toast::ToastKind::Error, title, body);
                        self.redraw();
                    }
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

    /// 활성 그리드 + 잠든 그리드 전부(설정 전파용).
    fn all_grids(&mut self) -> impl Iterator<Item = &mut grid::Grid> {
        std::iter::once(&mut self.grid).chain(self.grid_stash.values_mut())
    }

    /// 마지막 실행 탭의 그리드(활성이면 `grid` · 아니면 잠든 것 · 닫혔으면 `None`).
    fn run_grid(&mut self) -> Option<&mut grid::Grid> {
        if self.run_tab == self.grid_tab {
            Some(&mut self.grid)
        } else {
            self.grid_stash.get_mut(&self.run_tab)
        }
    }

    /// ★ 편집기 탭 ↔ 결과 그리드 쌍 동기화(사용자 09-16): 활성 탭이 바뀌었으면 그 탭의 그리드를 꺼내 오고(없으면 설정만
    /// 물려받은 빈 그리드) 지금 것은 잠재운다 · 닫힌 탭의 그리드는 버린다. 페인트 직전과 이벤트 뒤에 부른다.
    fn sync_grid_tab(&mut self) {
        let cur = self.editors.active_id();
        if cur != self.grid_tab {
            let b = self.grid.bounds;
            let next = self
                .grid_stash
                .remove(&cur)
                .unwrap_or_else(|| self.grid.fresh_like());
            let old = std::mem::replace(&mut self.grid, next);
            self.grid_stash.insert(self.grid_tab, old);
            self.grid_tab = cur;
            self.grid.set_bounds(b);
        }
        let alive = self.editors.tab_ids();
        self.grid_stash.retain(|id, _| alive.contains(id));
    }

    fn paint(&mut self) {
        let t_frame = Instant::now();
        let mut marks: [u32; 6] = [0; 6];
        let mut mark_i = 0usize;
        let mut mark = |t: &mut Instant, marks: &mut [u32; 6]| {
            if mark_i < marks.len() {
                marks[mark_i] = t.elapsed().as_micros() as u32;
                mark_i += 1;
            }
            *t = Instant::now();
        };
        let mut t_sec = Instant::now();
        self.sync_grid_tab();
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
                self.editors.paint_tabs(&mut dc, &th);
                // 메뉴바·툴바(창 전폭) — 메뉴 드롭다운은 최상위라 맨 뒤에.
                dc.fill_rect(self.toolbar.bounds(), th.chrome_bg);
                self.toolbar.paint(&mut dc, &th);
                // 툴바 툴팁은 탐색기·편집기가 덮지 못하게 최상위 층(메뉴바 직전)에서 그린다(09-16 사용자 캡처: 툴바 아래 검은 띠).
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
                let ty = dc.text_center_y(sy, px(24.0, s));
                dc.text(
                    px(8.0, s),
                    ty,
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
                // 접속 세그먼트 = 프로필 이름만(URL은 툴팁 카드·접속 창에 · 사용자 09-15).
                let conn = match self.conn_win.panel.state_ref() {
                    ConnState::Connected(d) => {
                        let name = self.conn_win.active_name();
                        if name.is_empty() {
                            d.clone()
                        } else {
                            name.to_string()
                        }
                    }
                    _ => "—".to_string(),
                };
                segs.push((conn, false));
                let nsel = self.editors.selection_count();
                if self.focus == Focus::Grid
                    && self
                        .grid
                        .selection_summary()
                        .is_some_and(|(r, c, _)| r * c > 1)
                {
                    let (r, c, _) = self.grid.selection_summary().unwrap_or((0, 0, 0));
                    segs.push((tf(Msg::StGridSel, &[&r.to_string(), &c.to_string()]), false));
                } else if nsel > 1 {
                    segs.push((tf(Msg::StSelections, &[&nsel.to_string()]), false));
                } else {
                    segs.push((tf(Msg::StPos, &[&ln.to_string(), &col.to_string()]), false));
                }
                if let Some(n) = self.last_rows {
                    segs.push((tf(Msg::StRowsShort, &[&n.to_string()]), false));
                }
                if let Some(secs) = self.last_secs {
                    segs.push((format!("{secs:.3}s"), false));
                }
                // 들여쓰기 세그먼트(Sublime "Tab Size: 4"/"Spaces: 4" · 구문 왼쪽 · 클릭 = 팝업 · 사용자 09-15).
                let (tsz, ispaces) = self.editors.indent();
                let ts = tsz.to_string();
                let indent_seg = if ispaces {
                    tf(Msg::StSpaces, &[&ts])
                } else {
                    tf(Msg::StTabSize, &[&ts])
                };
                // 줄끝 세그먼트(VS Code/Sublime식 · 클릭 = LF/CRLF 팝업 · docs/38 · 사용자 09-16).
                segs.push((
                    if self.editors.active_crlf() {
                        "CRLF"
                    } else {
                        "LF"
                    }
                    .to_string(),
                    true,
                ));
                segs.push((indent_seg, true));
                segs.push((self.editors.syntax_name(), true));
                let gap = px(12.0, s);
                let mut xr = wi - px(8.0, s);
                self.status_syntax_rect = Rect::new(0, 0, 0, 0);
                self.status_tab_rect = Rect::new(0, 0, 0, 0);
                self.status_eol_rect = Rect::new(0, 0, 0, 0);
                let last = segs.len() - 1;
                for (idx, (text, is_syntax)) in segs.iter().enumerate().rev() {
                    let tw = dc.text_width(text);
                    xr -= tw;
                    let r = Rect::new(xr - gap / 2, sy, tw + gap, px(24.0, s));
                    let ty = dc.text_center_y(sy, px(24.0, s));
                    dc.text(
                        xr,
                        ty,
                        r,
                        text,
                        if *is_syntax { th.text } else { th.text_dim },
                    );
                    if *is_syntax && idx == last {
                        self.status_syntax_rect = r;
                    } else if *is_syntax && idx + 1 == last {
                        self.status_tab_rect = r;
                    } else if *is_syntax {
                        self.status_eol_rect = r;
                    }
                    xr -= gap;
                    dc.fill_rect(
                        Rect::new(xr + gap / 2, sy + px(5.0, s), 1, px(14.0, s)),
                        th.border,
                    );
                }
            }
            mark(&mut t_sec, &mut marks); // 0 = 크롬(탭·툴바·상태줄)
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
            mark(&mut t_sec, &mut marks); // 1 = 편집기
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
            mark(&mut t_sec, &mut marks); // 2 = 그리드
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
                self.status_menu.paint(&mut dc, &th);
                self.grid.paint_menu(&mut dc, &th);
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
                self.act_bar.paint(&mut dc, &th);
                self.explorer.set_font_px(exp_px);
                self.explorer.paint(&mut dc, &th);
            }
            mark(&mut t_sec, &mut marks); // 3 = 탐색기(+카드)
                                          // ── 스플리터(탐색기|편집기 · 편집기|결과) — 본문 위 · hover 시 1초에 걸쳐 진해지는 손잡이(사용자 09-16)
            {
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s);
                self.split_v.paint(&mut dc, &th);
                self.split_h.paint(&mut dc, &th);
            }
            // ── 툴바 툴팁(UI 글꼴) — 툴바 패스에서 그리면 그 뒤에 칠하는 탐색기·편집기가 덮어 툴바 아래 2~3px 띠만
            //    남았다(09-16 Windows 캡처). nexa-ctl `Toolbar::paint_tooltip` 규약대로 팝업 층에서.
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
                self.toolbar.paint_tooltip(&mut dc, &th);
                // 토스트(우측 하단 · 상태줄 위 · 반투명 · docs/42).
                let status_y = hi - px(24.0, s);
                self.toasts.paint(&mut dc, &th, wi, status_y, s);
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
        mark(&mut t_sec, &mut marks); // 4 = 스플리터·툴팁·메뉴바
        let _ = buf.present();
        mark(&mut t_sec, &mut marks); // 5 = present
        if let Some(tr) = &mut self.frame_trace {
            tr.add(t_frame.elapsed().as_micros() as u32, &marks);
        }
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
        if let InputEvent::MouseDown { x, y, .. } = ev {
            if self.toasts.click(Point { x, y }) {
                self.redraw();
                return;
            }
        }
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
        // ★ MouseUp은 커서가 어디에 있든 **편집기에도** 전달한다 — 탭 바·탐색기 위에서 놓으면 편집기가 드래그 끝을
        //   못 받아 다음 MouseMove가 선택을 바꾸던 결함(사용자 09-15). 중복 전달은 무해(dragging=false 멱등).
        if matches!(ev, InputEvent::MouseUp { .. }) {
            self.ed_mut().on_event(&ev, &mut inv);
        }
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
        // 상태줄 들여쓰기 팝업(열려 있으면 모달 · 바깥 클릭은 닫고 통과).
        if self.status_menu.is_open() {
            let consumed = self.status_menu.on_event(&ev);
            if let Some(id) = self.status_menu.take_picked() {
                self.indent_pick(&id);
                self.redraw();
                return;
            }
            if consumed || !matches!(ev, InputEvent::MouseDown { .. }) {
                self.redraw();
                return;
            }
        }
        // ★ 스플리터(탐색기|편집기 · 편집기|결과) — 마우스만 · 드래그 중이면 다른 컨트롤보다 먼저(사용자 09-16).
        if is_mouse && self.route_splitters(&ev) {
            return;
        }
        // 상태줄 구문 이름 클릭 → 팔레트(Set Syntax) · 들여쓰기 세그먼트 클릭 → 팝업.
        if let InputEvent::MouseDown { x, y, .. } = ev {
            if self.status_syntax_rect.contains(Point { x, y }) {
                let prefill = format!("{}: ", t(Msg::PalSetSyntax));
                self.open_palette(&prefill);
                return;
            }
            if self.status_tab_rect.contains(Point { x, y }) {
                self.open_indent_menu();
                self.redraw();
                return;
            }
            if self.status_eol_rect.contains(Point { x, y }) {
                self.open_eol_menu();
                self.redraw();
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
            // 항목을 골랐거나 메뉴 안을 눌렀으면 그 클릭은 끝(아래 셀 선택으로 전파 금지 · 사용자 09-15) · 바깥 클릭만 통과.
            if self.grid.menu_open()
                || self.grid.take_menu_click()
                || !matches!(ev, InputEvent::MouseDown { .. })
            {
                self.redraw();
                return;
            }
        }
        // 활동 막대 — 마우스는 커서 아래일 때만(클릭 = 패널 토글/동작).
        if is_mouse {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            if self.act_bar.bounds().contains(cur) {
                if self.act_bar.on_event(&ev) {
                    self.redraw();
                }
                if let Some(id) = self.act_bar.take_picked() {
                    self.menu_action(id);
                }
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return;
                }
            } else if self.act_bar.clear_hover() {
                self.redraw();
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
        // ★ 좌클릭·우클릭 모두 커서 아래 컨트롤에 포커스(마우스 라우팅 규칙 · CLAUDE.md §3) — 우클릭이 빠져 있어
        //   편집기에 포커스가 있으면 그리드 우클릭이 편집기로 가서 메뉴가 안 떴다(사용자 09-16 · 좌클릭 뒤에야 동작).
        if let InputEvent::MouseDown { x, y, .. } | InputEvent::RightDown { x, y } = ev {
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
        // macOS Dock 아이콘 — 이벤트 루프 생성 직후에 넣으면 winit의 applicationDidFinishLaunching(활성화 정책 Regular)이
        // Dock 타일을 다시 만들며 덮는다(09-16 실기: 호출은 되나 `exec` 그대로) → 기동이 끝난 첫 resumed에서. 다른 OS no-op.
        icon::set_dock_icon();
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
        self.apply_on_top();
        self.layout();
        self.set_focus(Focus::Editor);
        self.apply_indent();
        self.editors.set_default_crlf(eol::default_crlf(
            self.settings.get("file.eol_new").unwrap_or("auto"),
        ));
        self.apply_menu_decor();
        // 로그 창은 설정 `log.open_at_start`(기본 off · 사용자 09-15)일 때만 메인 옆에 함께 연다(F10으로 언제든).
        if self.settings.flag("log.open_at_start") {
            let owner = self.window.clone();
            self.log_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                near,
                owner.as_deref(),
            );
        }
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
        redraw |= self.split_v.tick(now_ms);
        redraw |= self.split_h.tick(now_ms);
        // 더러움 표시(`*`) 갱신 · 닫기 2단 안내.
        redraw |= self.editors.refresh_dirty();
        if let Some(m) = self.editors.take_notice() {
            self.status = t(m).into();
            redraw = true;
        }
        if self.editors.relayout_if_needed() {
            redraw = true;
        }
        if redraw {
            self.redraw();
        }
        if self.toasts.tick(Instant::now()) {
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
        if self.file_win.tick(now_ms) {
            self.file_win.redraw();
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
            || self.log_win.tooltip_pending()
            || self.log_win.drag_active()
            || self.conn_win.bars_visible()
            || self.conn_win.tooltip_pending()
            || self.grid.hover_animating()
            || self.conn_win.hover_animating()
            || self.colors_win.animating()
            || self.keys_win.animating()
            || self.prefs_win.animating()
            || self.file_win.animating()
            || self.explorer.bars_visible()
            || self.find.animating()
            || self.editors.tooltip_pending()
            || self.toasts.animating();
        let mut next = if bars_live {
            self.next_blink.min(now + Duration::from_millis(33))
        } else {
            self.next_blink
        };
        // 서버 신호등 재시도 예약(접속 창이 열려 있을 때만).
        if let Some(t) = self.conn_win.tick(now) {
            next = next.min(t);
        }
        self.sync_modal();
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
        if let WindowEvent::DroppedFile(p) = &event {
            // OS에서 창으로 끌어다 놓기(3-OS 공통 winit 경로) = 열기.
            let p = p.clone();
            self.open_file(&p);
            return;
        }
        let modal_open = self.conn_win.is_open() || self.file_win.is_open();
        let is_modal_win = self.conn_win.is(id) || self.file_win.is(id);
        if modal_open
            && !is_modal_win
            && matches!(
                event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::Ime(_)
            )
        {
            if let Some(w) = self.file_win.window().or_else(|| self.conn_win.window()) {
                w.focus_window();
            }
            return;
        }
        if self.file_win.is(id) {
            let ui_px = self.settings.int("ui.font_size") as f32;
            match self.file_win.handle(&event) {
                FileWinAction::Paint => self.file_win.paint(&self.ui_font, &self.theme, ui_px),
                FileWinAction::Confirm(mode, path, enc) => {
                    self.remember_file_dialog(path.parent());
                    match (
                        mode,
                        std::mem::replace(&mut self.file_purpose, FilePurpose::Editor),
                    ) {
                        (PickerMode::Save, FilePurpose::LogExport) => {
                            // 로그 내보내기(현재 형식 · 보이는 줄 · UTF-8).
                            let text = self.log_win.export_text();
                            self.status = match std::fs::write(&path, text) {
                                Ok(()) => tf(
                                    Msg::StLogSaved,
                                    &[
                                        &path.to_string_lossy(),
                                        &self.log_win.visible_len().to_string(),
                                    ],
                                ),
                                Err(e) => tf(Msg::ErrLogFile, &[&e.to_string()]),
                            };
                        }
                        (PickerMode::Open, _) => self.open_file_enc(&path, &enc),
                        (PickerMode::Save, _) => {
                            self.editors.set_active_encoding(&enc);
                            self.save_to(&path);
                        }
                    }
                    self.sync_modal();
                }
                FileWinAction::Cancel => {
                    self.file_purpose = FilePurpose::Editor;
                    self.remember_file_dialog(None);
                    self.sync_modal();
                }
                FileWinAction::CopyText(text) => {
                    let _ = clipboard::write_text(&text);
                }
                FileWinAction::None => {
                    if !self.file_win.is_open() {
                        self.sync_modal();
                    }
                }
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
                    self.apply_menu_decor();
                    self.keys_win.refresh(&self.keymap);
                    self.keys_win.redraw();
                }
                KeysAction::ResetAll => {
                    for c in keymap::COMMANDS {
                        let _ = self.settings.reset(&keymap::setting_key(c.id));
                    }
                    let _ = self.settings.save();
                    self.keymap = Keymap::from_settings(&self.settings);
                    self.apply_menu_decor();
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
            match self.log_win.handle(&event) {
                LogWinAction::Paint => {
                    // 시스템 UI 글꼴 · 본문 = 편집기 기본 크기 · 푸터 = 메인 상태줄 크기(사용자 09-16).
                    let body_px = self.settings.int("editor.font_size") as f32;
                    let footer_px = nexa_ctl::theme::FontPrefs::default().status.size;
                    self.log_win
                        .paint(&self.ui_font, &self.theme, body_px, footer_px);
                }
                LogWinAction::Toggled(key, on) => {
                    // 스위치 = 설정과 같은 값(자동 기억 · 설정 창에도 반영).
                    let _ = self.settings.set(key, if on { "on" } else { "off" });
                    let _ = self.settings.save();
                }
                LogWinAction::Setting(key, value) => {
                    let _ = self.settings.set(key, &value);
                    let _ = self.settings.save();
                }
                LogWinAction::SaveAs => {
                    self.file_purpose = FilePurpose::LogExport;
                    self.open_file_window(el, PickerMode::Save);
                    self.sync_modal();
                }
                LogWinAction::CopyText(text) => {
                    let _ = clipboard::write_text(&text);
                }
                LogWinAction::None => {}
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
                // ★ Alt+Shift = 열(블록) 선택 모드(Sublime · 사용자 09-15) — 드래그 시작 판정에 쓴다.
                let col = self.alt && self.shift;
                self.editors.set_column_mode(col);
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                // 그리드 헤더 경계 위 = 폭 조절 커서.
                if let Some(w) = &self.window {
                    let cur = Point {
                        x: self.cursor.0,
                        y: self.cursor.1,
                    };
                    let over_edge = self.grid.header_edge_hover(self.cursor.0, self.cursor.1);
                    // 스플리터 위/드래그 중 = ↔ · ↕ (그리드 헤더 경계보다 우선).
                    w.set_cursor(
                        if self.split_v.is_dragging() || self.split_v.rect().contains(cur) {
                            winit::window::CursorIcon::ColResize
                        } else if self.split_h.is_dragging() || self.split_h.rect().contains(cur) {
                            winit::window::CursorIcon::RowResize
                        } else if over_edge {
                            winit::window::CursorIcon::ColResize
                        } else {
                            winit::window::CursorIcon::Default
                        },
                    );
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
                if let Some(ch) = Chord::from_winit(
                    &kev.logical_key,
                    &kev.physical_key,
                    self.primary,
                    self.shift,
                    self.alt,
                ) {
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
            // ★ 설정 창에서 열면 **설정 창을 소유자**로(그 위에 뜬다 · 메인 소유면 설정 창 뒤로 숨어 "안 열린 것처럼" 보이던 결함 · 사용자 09-15).
            let parent: Option<&Window> = self.prefs_win.window().or(self.window.as_deref());
            let over = parent.and_then(|w| {
                let p = w.outer_position().ok()?;
                let sz = w.outer_size();
                Some((p.x, p.y, sz.width, sz.height))
            });
            self.colors_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                over,
                parent,
            );
            if let Some(w) = self.colors_win.window() {
                w.focus_window();
            }
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
            // 설정 창에서 열면 설정 창을 소유자로(색 창과 같은 이유).
            let parent: Option<&Window> = self.prefs_win.window().or(self.window.as_deref());
            let over = parent.and_then(|w| {
                let p = w.outer_position().ok()?;
                let sz = w.outer_size();
                Some((p.x, p.y, sz.width, sz.height))
            });
            self.keys_win.refresh(&self.keymap);
            self.keys_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                over,
                parent,
            );
            if let Some(w) = self.keys_win.window() {
                w.focus_window();
            }
        }
        if std::mem::take(&mut self.open_conn) {
            self.open_conn_window(el);
            self.sync_modal();
        }
        if let Some(mode) = self.open_file_dlg.take() {
            self.open_file_window(el, mode);
            self.sync_modal();
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
        toasts: toast::Toasts::new(),
        log_hub: None,
        file_purpose: FilePurpose::Editor,
        last_run_items: Vec::new(),
        exit_requested: false,
        z_order: Vec::new(),
        palette: Palette::new(),
        syntax: syntax_reg.clone(),
        last_rows: None,
        last_secs: None,
        status_syntax_rect: Rect::new(0, 0, 0, 0),
        status_eol_rect: Rect::new(0, 0, 0, 0),
        status_tab_rect: Rect::new(0, 0, 0, 0),
        status_menu: nexa_ctl::controls::ctxmenu::ContextMenu::new(),
        toggle_log: false,
        open_colors: false,
        colors_win,
        keymap,
        open_keys: false,
        keys_win: KeysWin::new(),
        open_prefs: false,
        prefs_win: PrefsWin::new(),
        act_bar: ActivityBar::new(),
        file_win: FileWin::new(),
        open_file_dlg: None,
        menubar: MenuBar::new(App::build_menus()),
        toolbar: App::build_toolbar(),
        conn_win: ConnWin::new(panel),
        open_conn: true,
        find: FindBar::new(),
        editors: Editors::new(ed_line_numbers, ed_multi, ed_tooltip, syntax_reg),
        grid: grid::Grid::default(),
        grid_stash: HashMap::new(),
        key_cache: HashMap::new(),
        sql_wait: None,
        frame_trace: std::env::var_os("NSQL_TRACE_FRAMES").map(|_| FrameTrace::default()),
        grid_tab: 0,
        run_tab: 0,
        explorer,
        split_v: Splitter::new(SplitAxis::Vertical),
        split_h: Splitter::new(SplitAxis::Horizontal),
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
    app.grid.set_copy_null(app.settings.flag("grid.copy_null"));
    app.log_win
        .set_on_top(app.settings.flag("log.always_on_top"));
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
    app.toasts.configure(
        app.settings.int("ui.toast_secs"),
        app.settings.int("ui.toast_alpha"),
    );
    app.log_win.set_wrap(app.settings.flag("log.wrap"));
    app.log_win
        .set_switch_scale(app.settings.int("log.switch_scale"));
    app.log_win
        .set_template(app.settings.get("log.template").unwrap_or(""));
    app.log_win
        .set_columns(app.settings.get("log.columns").unwrap_or(""));
    app.log_win
        .set_kinds(app.settings.get("log.kinds").unwrap_or(""));
    app.rebuild_log_hub();
    app.log_win
        .set_newest_first(app.settings.flag("log.newest_first"));
    app.log_win
        .set_autoscroll(app.settings.flag("log.autoscroll"));
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

/// 프레임 계측 누적(`NSQL_TRACE_FRAMES=1`) — 60프레임마다 한 줄: 평균/최대 총 ms · 구간별 평균 ms.
#[derive(Default)]
struct FrameTrace {
    announced: bool,
    n: u32,
    total_us: u64,
    max_us: u32,
    secs_us: [u64; 6],
}

impl FrameTrace {
    fn add(&mut self, total: u32, secs: &[u32; 6]) {
        self.n += 1;
        if !self.announced {
            self.announced = true;
            eprintln!(
                "[frames] tracing on · first paint {:.2}ms",
                f64::from(total) / 1000.0
            );
        }
        self.total_us += u64::from(total);
        self.max_us = self.max_us.max(total);
        for (acc, v) in self.secs_us.iter_mut().zip(secs) {
            *acc += u64::from(*v);
        }
        if self.n >= 60 {
            let n = f64::from(self.n);
            let ms = |us: u64| us as f64 / n / 1000.0;
            eprintln!(
                "[frames] n={} avg {:.2}ms max {:.2}ms · chrome {:.2} editor {:.2} grid {:.2} explorer {:.2} top {:.2} present {:.2}",
                self.n,
                ms(self.total_us),
                f64::from(self.max_us) / 1000.0,
                ms(self.secs_us[0]),
                ms(self.secs_us[1]),
                ms(self.secs_us[2]),
                ms(self.secs_us[3]),
                ms(self.secs_us[4]),
                ms(self.secs_us[5]),
            );
            *self = Self {
                announced: true,
                ..Self::default()
            };
        }
    }
}

/// 파일 대화상자의 용도 — 같은 대화상자를 편집기 열기/저장과 로그 내보내기가 나눠 쓴다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilePurpose {
    Editor,
    LogExport,
}

/// 실행 스크립트의 문장 본문 목록(오류 이벤트의 index로 찾는다).
fn split_items(src: &str) -> Vec<String> {
    nsql_script::split_script(src)
        .into_iter()
        .map(|i| i.text)
        .collect()
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
