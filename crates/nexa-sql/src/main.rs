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
mod enc;
mod eol;
mod exp_icons;
mod explorer;
#[allow(dead_code)]
// 09-17 레인보우 플러그인 모듈 · 배선(설정→편집기 · 키맵 · 메뉴)은 다음 세션(T-119)
mod extensions;
mod file_win;
mod findbar;
mod gitstat;
mod grid;
mod icon;
mod input;
mod keymap;
mod keys_win;
mod log_win;
mod palette;
mod prefs_win;
mod probe;
mod results;
mod runtoast;
mod rx;
mod search_panel;
mod syntax;
mod theme;
mod toast;
mod toolfloat;
mod toolicons;
mod txlog_win;
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
use nexa_ctl::raster::{FontSet, RasterCtx};
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{
    ComboItem, Control, DockAction, DockLayout, EditCommand, EditCtxAction, InputEvent,
    Invalidations, Key as CtlKey, MenuBar, MenuDef, MenuEntry, TextBox, ToolDock, ToolGroup,
    ToolItem, ToolTone, Widget,
};
use nexa_dlg::PickerMode;
use nexa_gfx::{Font, Surface};
use nsql_core::Dialect;
use nsql_i18n::{current_lang, t, tf, Msg};
use nsql_log::{LogEntry, LogKind, LogLayer, LogLevel};
use nsql_run::txlog::{Purpose as TxPurpose, TxLog, TxOutcome};
use nsql_run::RunEvent;
use nsql_script::ConnectSpec;
use nsql_settings::{Settings, ThemeMode};
use nsql_vault::Vault;
use palette::{Palette, PaletteAction};
use prefs_win::{PrefsAction, PrefsWin};
use results::{PanelAction as ResultAction, ResultPanel, ResultTab};
use search_panel::{SearchCtx, SearchPanel};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use syntax::SyntaxRegistry;
use toolfloat::{FloatAction, ToolFloatWin};
use txlog_win::{TxLogAction, TxLogWin};
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
    /// 파일 검색 패널(T-81a).
    Search,
}

/// 잃는 순간의 확인(Commit/Rollback) 뒤 이어질 동작(DR-30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxAfter {
    CloseTab(usize),
    Disconnect,
    Exit,
    SwitchAuto,
}

/// 수동 커밋 대기 문장 하나(T-77) — 편집기 탭 · 시각 · 시:분 · 요약.
#[derive(Clone, Debug)]
struct TxItem {
    editor: u64,
    at: Instant,
    when: String,
    summary: String,
    /// 문장 종류(docs/44 §5 · 버튼 색·툴팁·로그 Tx 열).
    class: nsql_core::TxClass,
}

struct App {
    window: Option<Rc<Window>>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    ui_font: Font,
    mono_font: Font,
    /// 결과 그리드·텍스트 보기 글꼴(설정 `grid.font_face` · None = UI 글꼴 · 사용자 09-16 Golden 참고).
    grid_font: Option<Font>,
    theme: Theme,
    /// 앱 설정(언어·테마 모드·글꼴 크기) — 단축키로 바꾸면 즉시 저장.
    settings: Settings,
    scale: f32,
    /// 로그 창(별도 창 · `Ctrl/⌘+⇧G`).
    log_win: LogWin,
    /// 트랜잭션 로그(수집기 · docs/44 · T-107) + 모덜리스 창.
    txlog: TxLog,
    txlog_win: TxLogWin,
    open_txlog: bool,
    /// 우측 하단 토스트(오류 분류 · docs/42).
    toasts: toast::Toasts,
    /// 실행 상태 카드(사용자 09-17) + 다음 깨울 시각.
    run_toast: runtoast::RunToast,
    run_toast_next: Option<Instant>,
    /// 문장 실행 버튼 활성 상태 캐시(다중 커서면 비활성 · 사용자 09-17).
    run_stmt_enabled: bool,
    /// 툴바 ■(실행 중지) 활성 캐시(= busy · 시작값 true = 첫 동기화에서 비활성으로).
    run_stop_enabled: bool,
    /// 사용자가 중지를 눌렀다 — 뒤따르는 드라이버 오류(interrupted · 57014 · ORA-01013)는 오류가 아니라 "중지됨".
    run_cancel_requested: bool,
    /// 실행 텍스트의 첫 줄이 편집기에서 몇 번째 줄인가(0 기준 · 문장 실행이면 그 문장의 줄 · 오류 줄 → 편집기 줄).
    run_line_base: usize,
    /// 이번 취소가 세션을 끊는 방식(SQL Server) — 중지 뒤 트랜잭션 Lost + 재접속 안내.
    run_cancel_drops: bool,
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
    /// 상태줄 인코딩 세그먼트(클릭 = 인코딩 팝업).
    status_enc_rect: Rect,
    /// 상태줄 git 세그먼트(활성 파일 폴더 · 배경 조회).
    git: gitstat::GitWatch,
    status_menu: nexa_ctl::controls::ctxmenu::ContextMenu,
    /// 창 z-order(맨 뒤 → 맨 앞) — `window.focus = group`일 때 함께 올리는 순서.
    z_order: Vec<WindowId>,
    toggle_log: bool,
    /// 색 설정 창 열기 요청(메뉴/팔레트 → 다음 이벤트 루프 턴에 `el`로 연다).
    open_colors: bool,
    colors_win: ColorsWin,
    /// ★ 단축키 표(사용자 09-15 · Sublime 기본 + `key.*` 설정) · 캡처 창.
    keymap: Keymap,
    /// 2단 단축키의 첫 조합(`Ctrl+K` 뒤 다음 키 대기 · 09-16).
    pending_chord: Option<Chord>,
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
    /// 탭 메뉴를 마지막으로 만든 근거(id·제목·활성) — 바뀌면 메뉴를 다시 만든다.
    tabs_menu_sig: String,
    /// ★ 툴바 = 목적별 그룹 도크(nexa-ctl `ToolDock` · 사용자 09-17): 그립 드래그로 순서 이동 · 세로로 끌면 플로팅 창 ·
    ///   배치는 설정 `toolbar.layout`에 자동 저장 · 우클릭/View 메뉴에서 초기화.
    tool_dock: ToolDock,
    /// 플로팅 툴바 창(그룹당 1) — 툴바 컨트롤은 도크가 소유하고 창은 표면만.
    tool_floats: Vec<ToolFloatWin>,
    /// 떼어 내기 요청(그룹 id · 클라이언트 좌표) — 창 생성은 이벤트 루프 핸들(`about_to_wait`)에서.
    pending_float: Vec<(String, Option<(i32, i32)>)>,
    /// 배치가 바뀌어 저장할 것이 있다(플로팅 창 이동은 잦으므로 틱에서 한 번에).
    tool_layout_dirty: bool,
    /// 데모(사용자 09-17): 'Demo' 프로필 + `demo.sqlite`가 있는가(메뉴 비활성 근거 · 시작 때·생성 뒤 갱신).
    demo_ready: bool,
    /// 데모 생성 스레드의 결과 채널(Ok = 파일 경로 · Err = 메시지).
    demo_job: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    /// 최초 실행 1회 팝업 예약(창이 뜬 뒤 about_to_wait에서 연다).
    pending_demo_prompt: bool,
    /// 접속 창(별도 창 · 폼 + 로그인 목록). 폼 상태의 단일 원천 = `conn_win.panel`.
    conn_win: ConnWin,
    /// 메뉴/툴바에서 접속 창 열기 요청(창 생성은 이벤트 루프 핸들에서).
    open_conn: bool,
    /// 찾기/바꾸기 바(편집기 위 · T-73).
    find: FindBar,
    /// 선택 범위에서 찾기 — 켤 때의 선택(문자 인덱스 · 편집으로 길이가 바뀌면 그대로 둔다).
    find_scope: Option<(usize, usize)>,
    editors: Editors,
    /// **활성 결과 탭**의 그리드(활성 편집기 탭의 결과 패널 중 활성 탭). 그리기·이벤트는 이것 하나만 만진다(D-71 · T-93).
    grid: grid::Grid,
    /// 활성 편집기 탭의 결과 패널(결과 탭 여러 개 · 활성 탭 자리는 자리표시자 · `grid`가 실제).
    panel: ResultPanel,
    /// `panel`이 속한 편집기 탭 id(0 = 아직 없음).
    panel_editor: u64,
    /// 잠든 편집기 탭들의 결과 패널(편집기 탭 id → 패널 · 편집기 탭이 닫히면 통째로 drop = rows 즉시 해제).
    panels: HashMap<u64, ResultPanel>,
    /// 결과 탭 id 발급(전역 고유 · 워커 요청 키).
    next_result_id: u64,
    /// 결과 영역(탭 바 + 그리드) — 재배치 근거.
    result_area: Rect,
    /// 프레임 계측(`NSQL_TRACE_FRAMES=1` · docs/39 §6 `--trace-frames`) — 60프레임마다 stderr에 구간별 평균/최대(ms).
    frame_trace: Option<FrameTrace>,
    /// 테이블 키 캐시(접속당 · 표기 그대로 키) — Copy SQL의 키 조회 왕복을 테이블당 1회로(docs/41).
    key_cache: HashMap<String, Option<nsql_core::KeyInfo>>,
    /// 키 조회를 기다리는 SQL 복사 종류.
    sql_wait: Option<nsql_io::SqlKind>,
    /// 직전 실행이 Ctrl+Enter 한 문장(끝나면 설정 `run.after_statement`대로 캐럿 이동).
    single_run: bool,
    /// SQL 보기 모드가 키를 기다린다(결과 도구줄 ▸ 보기 ▸ SQL).
    view_wait: Option<nsql_io::SqlKind>,
    /// ORDER BY 없는 재질의 경고를 낸 결과 탭(탭당 1회).
    offset_warned: std::collections::HashSet<u64>,
    /// `grid`가 속한 **결과 탭** id.
    /// 확장 레지스트리(in-process · docs/50 §4) + 매니저 팔레트가 고른 후보(설치 목록 · 저장소 목록).
    extensions: extensions::Registry,
    ext_catalog: Vec<(extensions::manager::Source, extensions::manager::Summary)>,
    grid_tab: u64,
    /// 마지막 실행의 대상 결과 탭 id — 결과는 실행 중 탭을 바꿔도 그 탭의 그리드로 간다.
    run_tab: u64,
    /// ★ 오브젝트 탐색기(사용자 09-15 · docs/28) — 메타 세션은 자기 스레드.
    explorer: Explorer,
    /// 파일 검색 패널(활동 막대 두 번째 · T-81a · docs/36).
    search: SearchPanel,
    /// 스플리터 ① 탐색기|편집기(세로선) · ② 편집기|결과(가로선) — 사용자 09-16.
    split_v: Splitter,
    split_h: Splitter,
    /// 마지막으로 접속을 시도한 스펙(접속 성공 시 탐색기 메타 세션을 같은 스펙으로 연다).
    last_spec: Option<ConnectSpec>,
    /// 현재 접속 방언(Explain · INSERT 복사 · 상태줄).
    dialect: Dialect,
    /// 수동 커밋 모드에서 커밋되지 않은 변경이 있는가(상태줄 ● · 사용자 09-15).
    tx_dirty: bool,
    /// 수동 커밋 대기 문장(DR-30 · T-77). 세션은 공유(T-54 전)라 목록은 하나 · 탭 배지는 탭별 수.
    tx_pending: Vec<TxItem>,
    /// 수동 모드에서 마지막 커밋 뒤 조회만 있었다(읽기 트랜잭션 · 배지 없음 · 초록).
    tx_read: bool,
    /// 오래된 미커밋 경고를 로그에 남겼다(1회).
    tx_stale_logged: bool,
    /// 실행을 시작한 편집기 탭 id(대기 문장의 소속).
    run_editor: u64,
    /// 확인 뒤 이어질 동작.
    tx_after: Option<TxAfter>,
    status_tx_rect: Rect,
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
    // 입력 상태
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    alt: bool,
    /// Linux(Sublime) Shift+우클릭 드래그 열 선택이 진행 중 — 우클릭을 좌드래그로 바꿔 보내고 놓으면 끝(메뉴 안 뜸).
    col_right_drag: bool,
    /// 원시 Control 키(mac에서 ⌘=primary와 구분 · 서브워드 이동).
    ctrl_raw: bool,
    /// macOS Control 눌림(⌘와 별개 · 키맵 `ctrl+cmd+…`).
    ctrl_mac: bool,
    started: Instant,
    /// 다음 캐럿 깜빡임 시각 — about_to_wait의 재그리기 게이트.
    next_blink: Instant,
    /// 접속 패널 작업(진행/결과) — 한 번에 하나 · 프로필 이름에 묶임(사용자 09-14).
    panel_op: Option<(String, ConnState)>,
    /// ★ 프로필별 **마지막 완료 결과**(TestOk/Failed/Connected · 사용자 09-16): 상세 폼이 닫힌 채 행 Test가 실패해도,
    /// 그 뒤 다른 프로필로 접속해 `panel_op`가 바뀌어도, 나중에 Details로 그 프로필을 열면 결과가 그대로 보인다.
    /// 진행 중(Testing/Connecting)은 넣지 않는다 · 삭제/이름 변경 시 제거 · 접속 해제 시 Connected 항목 제거.
    panel_results: HashMap<String, ConnState>,
}

/// 스플리터 잡히는 띠 두께(논리 px).
const SPLIT_GRIP: f32 = 6.0;

/// 프로필을 폼에 불러올 때 보일 상태(사용자 09-16): 진행 중/직전 작업(`op`)이 그 프로필 것이면 그것 → 아니면 프로필별
/// 마지막 완료 결과(`results`) → 없으면 Idle. 다른 프로필로 접속해 `op`가 바뀌어도 앞선 테스트 실패가 사라지지 않는다.
fn resolve_panel_state(
    op: Option<&(String, ConnState)>,
    results: &HashMap<String, ConnState>,
    name: &str,
) -> ConnState {
    match op {
        Some((n, st)) if n.trim() == name.trim() => st.clone(),
        _ => results.get(name.trim()).cloned().unwrap_or(ConnState::Idle),
    }
}

/// 방언별 암묵 커밋(DDL이 트랜잭션을 끝내는 서버 · docs/34 §2-3): Oracle · MySQL. MSSQL·PG·SQLite는 DDL도 트랜잭션 안.
fn implicit_commit(dialect: Dialect, stmt: &str) -> bool {
    dialect.implicit_commit(stmt)
}

/// ★ 상세 로그(docs/48): 게이트가 꺼져 있으면 `$make`는 **평가되지 않는다**(문자열·시각 0) — 인라인 원자 load + 분기 1.
macro_rules! dlog {
    ($self:ident, $layer:expr, $level:expr, $make:expr) => {
        if nsql_log::wants($layer, $level) {
            let __e = $make;
            detail_push(
                &mut $self.log_win,
                $self.log_hub.as_ref(),
                $layer,
                $level,
                __e,
            );
        }
    };
}

/// 상세 로그 한 줄(느린 경로 · 호출 자체가 드물다 · `#[cold]`로 뜨거운 경로 코드 배치에서 떨어뜨린다).
/// 필드 둘만 받아 다른 필드가 빌려진 자리(페인트 중)에서도 부를 수 있다.
#[cold]
#[inline(never)]
fn detail_push(
    log_win: &mut LogWin,
    hub: Option<&nsql_log::LogHub>,
    layer: LogLayer,
    level: LogLevel,
    e: LogEntry,
) {
    let e = e.at(layer, level);
    if let Some(h) = hub {
        h.push(e.clone());
    }
    log_win.push(e);
}

/// 전송 속도 문구(`1.2 MB/s` · 0이면 빈 문자열).
fn speed_of(bytes: u64, dur: Duration) -> String {
    let secs = dur.as_secs_f64();
    if secs <= 0.0 || bytes == 0 {
        return String::new();
    }
    format!("{}/s", nsql_core::fmt_bytes((bytes as f64 / secs) as u64))
}

fn first_word(stmt: &str) -> String {
    stmt.split(|c: char| !c.is_alphanumeric() && c != '_')
        .find(|w| !w.is_empty())
        .map(|w| w.to_ascii_uppercase())
        .unwrap_or_default()
}

/// 한 줄 요약(공백 접기 · `max` 글자).
fn one_line(s: &str, max: usize) -> String {
    let joined: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = joined.chars().take(max).collect();
    if joined.chars().count() > max {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tx_tests {
    use super::*;

    #[test]
    fn implicit_commit_only_for_oracle_mysql_ddl() {
        assert!(implicit_commit(Dialect::Oracle, "create table t (a int)"));
        assert!(implicit_commit(Dialect::Oracle, "  TRUNCATE TABLE t"));
        assert!(!implicit_commit(Dialect::Oracle, "update t set a = 1"));
        assert!(!implicit_commit(Dialect::Sqlite, "create table t (a int)"));
        assert_eq!(one_line("update  t\n set a = 1", 8), "update t…");
    }
}

/// 대소문자 보존 치환(VS Code Preserve Case · Sublime Alt+A): 일치가 전부 대문자 → 대문자 · 첫 글자만 대문자 → 첫 글자만 ·
/// 그 밖(전부 소문자 · 혼합)은 입력 그대로.
fn preserve_case(matched: &str, repl: &str) -> String {
    let letters: Vec<char> = matched.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return repl.to_string();
    }
    if letters.iter().all(|c| c.is_uppercase()) && letters.len() > 1 {
        return repl.to_uppercase();
    }
    let first_upper = letters[0].is_uppercase();
    let rest_lower = letters.iter().skip(1).all(|c| c.is_lowercase());
    if first_upper && rest_lower {
        let mut out = String::new();
        let mut done = false;
        for c in repl.chars() {
            if !done && c.is_alphabetic() {
                out.extend(c.to_uppercase());
                done = true;
            } else {
                out.extend(c.to_lowercase());
            }
        }
        return out;
    }
    if letters.iter().all(|c| c.is_lowercase()) {
        return repl.to_lowercase();
    }
    repl.to_string()
}

#[cfg(test)]
mod find_case_tests {
    use super::preserve_case;

    #[test]
    fn preserve_case_follows_match_shape() {
        assert_eq!(preserve_case("HELLO", "world"), "WORLD");
        assert_eq!(preserve_case("Hello", "wORLD"), "World");
        assert_eq!(preserve_case("hello", "World"), "world");
        assert_eq!(preserve_case("hELLo", "World"), "World", "혼합은 그대로");
        assert_eq!(preserve_case("123", "abc"), "abc", "글자가 없으면 그대로");
    }
}

#[cfg(test)]
mod panel_state_tests {
    use super::*;

    #[test]
    fn last_result_survives_other_profile_ops() {
        let mut results = HashMap::new();
        results.insert("B".to_string(), ConnState::Failed("ORA-12541".into()));
        // 진행 중 작업이 A(다른 프로필)여도 B의 실패는 남는다.
        let op = ("A".to_string(), ConnState::Connected("A@db".into()));
        assert_eq!(
            resolve_panel_state(Some(&op), &results, "B"),
            ConnState::Failed("ORA-12541".into())
        );
        // 진행 중 작업이 B 자신이면 그것이 우선(Testing).
        let op = ("B".to_string(), ConnState::Testing);
        assert_eq!(
            resolve_panel_state(Some(&op), &results, "B"),
            ConnState::Testing
        );
        // 결과가 없는 프로필은 Idle · 이름 앞뒤 공백 무시.
        assert_eq!(resolve_panel_state(None, &results, "C"), ConnState::Idle);
        assert_eq!(
            resolve_panel_state(None, &results, " B "),
            ConnState::Failed("ORA-12541".into())
        );
    }
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
        let menu_px = self.settings.font_px("ui.menu_font_size");
        let menu_h = px(menu_px + 11.0, s);
        self.menubar.set_scale(s);
        self.tool_dock.set_scale(s);
        self.menubar
            .set_bounds(Rect::new(0, 0, w, menu_h), &mut inv);
        // `preferred_height`는 논리 px(nexa-ctl 규약) → 물리 px로. 그대로 쓰면 HiDPI에서 툴바가 1/배율로 납작해진다
        // (맥 2x 실기 09-16: Windows 100% 32px vs 맥 16px).
        let tool_h = px(self.tool_dock.preferred_height() as f32, s);
        self.tool_dock
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
        } else if self.search.is_visible() {
            Some("view.search")
        } else {
            None
        });
        let exp_w = if self.explorer.is_visible() || self.search.is_visible() {
            px(self.settings.int("explorer.width") as f32, s)
        } else {
            0
        };
        self.explorer.set_bounds(
            Rect::new(
                act_w,
                body_top,
                if self.explorer.is_visible() { exp_w } else { 0 },
                body_h,
            ),
            s,
        );
        self.search.set_bounds(
            Rect::new(
                act_w,
                body_top,
                if self.search.is_visible() { exp_w } else { 0 },
                body_h,
            ),
            s,
        );
        self.search.set_clamp_width(w);
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
        self.find.set_clamp_width(w);
        // 스플리터 ② 편집기|결과 — 띠 = 편집기 아래 여백(pad) 자리.
        self.split_h
            .set_rect(Rect::new(rx, body_top + editor_h - pad, rw, pad.max(grip)));
        let gb = Rect::new(rx, body_top + editor_h, rw, body_h - editor_h - pad);
        // 결과 영역 = 결과 탭 바(보일 때만) + 그리드(T-93).
        self.result_area = gb;
        let grid_rect = self.panel.set_bounds(gb, s);
        self.all_grids().for_each(|g| g.set_bounds(grid_rect));
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
        self.search.set_focused(f == Focus::Search);
        if let Some(w) = &self.window {
            w.set_ime_allowed(f == Focus::Editor || f == Focus::Find || f == Focus::Search);
        }
    }

    /// 포커스 텍스트 박스(IME·편집 컨텍스트 라우팅).
    fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        match self.focus {
            Focus::Editor => Some(self.editors.cur_mut()),
            Focus::Find => self.find.focused_textbox(),
            Focus::Search => self.search.focused_textbox(),
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
                        // 이름이 바뀌었으면 옛 이름의 결과는 버린다(저장 내용이 달라졌을 수 있으니 옮기지 않는다).
                        if let Some(old) = &rename_from {
                            self.panel_results.remove(old.trim());
                        }
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
                        // 상태는 한 번에 하나(사용자 09-14): 진행/결과가 이 프로필 것이면 복원, 아니면 프로필별 마지막 결과(09-16).
                        let st = self.panel_state_for(&name);
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
        // 완료 결과는 프로필별로도 남긴다(나중에 Details로 열 때 복원 · 사용자 09-16).
        if !name.trim().is_empty()
            && !matches!(
                st,
                ConnState::Idle | ConnState::Testing | ConnState::Connecting
            )
        {
            self.panel_results
                .insert(name.trim().to_string(), st.clone());
        }
        self.panel_op = Some((name.to_string(), st));
    }

    /// 프로필을 폼에 불러올 때 보일 상태 — 진행 중/직전 작업이 이 프로필 것이면 그것, 아니면 프로필별 마지막 결과, 없으면 Idle.
    fn panel_state_for(&self, name: &str) -> ConnState {
        resolve_panel_state(self.panel_op.as_ref(), &self.panel_results, name)
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
                        tf(Msg::LogTestOk, &[&description, &elapsed_s.to_string()]),
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
                    if let Some(kind) = self.view_wait.take() {
                        self.finish_view_sql(kind, info.as_ref());
                    }
                }
                ConnOutcome::FetchProgress { key, rows, bytes } => {
                    if let Some(g) = self.grid_for(key) {
                        g.set_fetch_progress(rows, bytes);
                    }
                    self.run_toast.progress(rows, bytes);
                    dlog!(self, LogLayer::Fetch, LogLevel::Progress, {
                        LogEntry::new(
                            LogKind::Fetch,
                            tf(Msg::LogDetProgress, &[&nsql_core::fmt_bytes(bytes)]),
                        )
                        .rows(rows)
                    });
                    self.status = tf(
                        Msg::StFetchingProgress,
                        &[&rows.to_string(), &nsql_core::fmt_bytes(bytes)],
                    );
                    self.redraw();
                }
                ConnOutcome::Page {
                    key,
                    offset,
                    all,
                    result,
                    stop,
                    replace,
                } => {
                    match result {
                        Ok((rs, more, elapsed)) => {
                            let n = rs.rows.len().to_string();
                            let secs = format!("{:.3}", elapsed.as_secs_f64());
                            if replace {
                                // 엄격 일관성(docs/43 §9): 처음부터 다시 받아 교체했다 — 로그 1줄.
                                self.log_win.push(LogEntry::new(
                                    LogKind::Info,
                                    tf(Msg::StRefetchReplaced, &[&n]),
                                ));
                            }
                            let total = match self.grid_for(key) {
                                Some(g) => {
                                    if replace {
                                        g.replace_rows(rs, more);
                                    } else if all {
                                        // 전체 조회 = 나머지 이어 붙이기(위치·정렬·텍스트 스크롤 유지 · 09-17).
                                        g.append_all(rs, more);
                                    } else if offset == 0 {
                                        g.set_result(rs);
                                        g.set_more(more);
                                    } else {
                                        g.append_page(rs, more);
                                    }
                                    g.row_count()
                                }
                                // (전체 조회가 예산에서 잘렸으면 아래에서 안내)
                                None => 0,
                            };
                            self.status = tf(Msg::StFetched, &[&n, &secs, &total.to_string()]);
                            if all {
                                let phase = match stop {
                                    Some(worker::FetchStop::Cancelled) => {
                                        runtoast::Phase::Stopped { rows: total as u64 }
                                    }
                                    _ => runtoast::Phase::Done {
                                        rows: Some(total as u64),
                                        secs: elapsed.as_secs_f64(),
                                        stages: String::new(),
                                    },
                                };
                                self.run_toast.finish(phase);
                                dlog!(self, LogLayer::Fetch, LogLevel::Timing, {
                                    let b = self.grid_for(key).map_or(0, |g| g.approx_bytes());
                                    LogEntry::new(
                                        LogKind::Fetch,
                                        tf(
                                            Msg::LogDetFetchAll,
                                            &[&nsql_core::fmt_bytes(b), &speed_of(b, elapsed)],
                                        ),
                                    )
                                    .rows(total as u64)
                                    .elapsed(elapsed)
                                });
                            }
                            match stop {
                                Some(worker::FetchStop::Budget) => {
                                    // 전체 조회가 메모리 예산(D-72)에서 멈췄다.
                                    self.status = tf(
                                        Msg::StBudgetExceeded,
                                        &[&self.settings.int("grid.memory_budget_mb").to_string()],
                                    );
                                    self.log_win
                                        .push(LogEntry::new(LogKind::Info, self.status.clone()));
                                }
                                Some(worker::FetchStop::Cancelled) => {
                                    self.status = tf(Msg::StFetchCancelled, &[&total.to_string()]);
                                    self.log_win
                                        .push(LogEntry::new(LogKind::Info, self.status.clone()));
                                }
                                None => {}
                            }
                        }
                        Err(e) => {
                            if let Some(g) = self.grid_for(key) {
                                g.fetch_failed();
                            }
                            self.status = tf(Msg::StFetchFailed, &[&e]);
                            self.log_win
                                .push(LogEntry::new(LogKind::Error, self.status.clone()));
                        }
                    }
                    self.redraw();
                }
                ConnOutcome::Count { key, result } => {
                    match result {
                        Ok(n) => {
                            if let Some(g) = self.grid_for(key) {
                                g.set_total(n);
                            }
                            self.status = tf(Msg::StCountResult, &[&n.to_string()]);
                            self.log_win
                                .push(LogEntry::new(LogKind::Info, self.status.clone()));
                        }
                        Err(e) => {
                            if let Some(g) = self.grid_for(key) {
                                g.fetch_failed();
                            }
                            self.status = tf(Msg::StFetchFailed, &[&e]);
                        }
                    }
                    self.redraw();
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
    /// 찾기 조건 → 정규식(D-76): 정규식 모드가 아니어도 같은 엔진(글자 그대로 이스케이프)을 쓴다 — 단어 단위·대소문자 규칙 한 곳.
    fn find_rx(&mut self) -> Option<fancy_regex::Regex> {
        let q = self.find.query();
        if q.is_empty() {
            return None;
        }
        match rx::compile(
            &q,
            self.find.regex(),
            self.find.case_sensitive(),
            self.find.whole_word(),
        ) {
            Ok(r) => Some(r),
            Err(e) => {
                self.find.set_status(tf(Msg::StFindBadRegex, &[&e]));
                self.find.set_has_matches(false);
                None
            }
        }
    }

    fn find_matches(&mut self) -> (Vec<(usize, usize)>, Vec<char>) {
        let (mut out, text) = self.find_matches_all();
        // 선택 범위에서 찾기(≡ · Alt+L): 켤 때 잡은 범위 안의 일치만.
        if let Some((a, e)) = self.find_scope {
            out.retain(|(s, t)| *s >= a && *t <= e);
        }
        (out, text)
    }

    fn find_matches_all(&mut self) -> (Vec<(usize, usize)>, Vec<char>) {
        let full = self.ed_mut().text();
        let text: Vec<char> = full.chars().collect();
        // ★ 정규식 모드 = fancy-regex(문자 인덱스로 변환) · 아니면 종전 문자 비교(빠른 경로 유지).
        if self.find.regex() {
            let Some(r) = self.find_rx() else {
                return (Vec::new(), text);
            };
            return (rx::find_all(&r, &full), text);
        }
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
        self.find.set_has_matches(!matches.is_empty());
        if matches.is_empty() {
            self.find.set_status(t(Msg::StFindNone));
            self.ed_mut().set_find_marks(Vec::new());
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
        // 일치 전부 표시(T-73).
        self.ed_mut().set_find_marks(matches);
        self.redraw();
    }

    /// 현재 선택이 일치면 바꾸고 다음으로.
    /// 일치 `(a, b)`에 넣을 치환문 — 정규식 모드면 `$1`/`${name}` 확장 · 아니면 글자 그대로.
    fn find_expansion(&mut self, a: usize) -> String {
        let repl = self.find.replacement();
        let full = self.ed_mut().text();
        let base = if self.find.regex() {
            match self.find_rx() {
                Some(r) => rx::expand_at(&r, &full, a, &repl),
                None => repl,
            }
        } else {
            repl
        };
        if !self.find.preserve_case() {
            return base;
        }
        // 대소문자 보존(AB · Alt+A): 일치가 전부 대문자면 대문자로 · 첫 글자만 대문자면 첫 글자만 · 전부 소문자면 소문자로.
        let (matches, _) = self.find_matches();
        let Some(&(s, e)) = matches.iter().find(|(s, _)| *s == a) else {
            return base;
        };
        let matched: String = full.chars().skip(s).take(e - s).collect();
        preserve_case(&matched, &base)
    }

    fn find_replace_one(&mut self) {
        let (matches, _) = self.find_matches();
        if let Some((a, b)) = self.ed_mut().selection() {
            if matches.contains(&(a, b)) {
                let repl = self.find_expansion(a);
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
        // 뒤에서 앞으로(앞 인덱스가 안 밀리게) · 확장은 원본 본문 기준으로 먼저 계산.
        let repls: Vec<String> = matches
            .iter()
            .map(|(a, _)| self.find_expansion(*a))
            .collect();
        let mut inv = Invalidations::default();
        for ((a, b), repl) in matches.iter().zip(repls.iter()).rev() {
            self.ed_mut().replace_range(*a, *b, repl, &mut inv);
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
            FindAction::ScopeChanged => {
                if self.find.in_selection() {
                    match self.ed_mut().selection() {
                        Some((a, b)) if b > a => {
                            self.find_scope = Some((a, b));
                            self.ed_mut().set_find_scope(Some((a, b)));
                            // 범위 안 첫 일치로(선택은 범위 표시가 대신한다).
                            let mut inv = Invalidations::default();
                            self.ed_mut().select_range(a, a, &mut inv);
                        }
                        _ => {
                            self.find.set_in_selection(false);
                            self.status = t(Msg::StFindNoSelection).into();
                        }
                    }
                } else {
                    self.find_scope = None;
                    self.ed_mut().set_find_scope(None);
                }
                self.find_step(true, false);
            }
            FindAction::SelectAll => {
                let (matches, _) = self.find_matches();
                if matches.is_empty() {
                    self.find.set_status(t(Msg::StFindNone));
                } else {
                    let n = matches.len();
                    self.ed_mut().set_regions_pub(&matches);
                    self.find
                        .set_status(tf(Msg::StSelections, &[&n.to_string()]));
                    self.set_focus(Focus::Editor);
                }
                self.redraw();
            }
            FindAction::Close => {
                self.find.close();
                self.find_scope = None;
                self.ed_mut().set_find_scope(None);
                self.ed_mut().set_find_marks(Vec::new());
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
                            note.push_str(&tf(Msg::StUnknownKeys, &[&r.unknown.join(",")]));
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
        // Sublime식 3종(사용자 09-16 캡처): Windows CRLF · Unix LF · Mac OS 9 CR · 현재 = ✓ + 강조색.
        let cur = self.editors.active_eol();
        let mark = |on: bool, s: &str| {
            if on {
                format!("✓ {s}")
            } else {
                format!("   {s}")
            }
        };
        let it = |id: &str, m: Msg, e: eol::Eol| {
            CtxItem::item(id, mark(cur == e, t(m))).with_active(cur == e)
        };
        let items = vec![
            it("eol.crlf", Msg::MnEolCrlf, eol::Eol::Crlf),
            it("eol.lf", Msg::MnEolLf, eol::Eol::Lf),
            it("eol.cr", Msg::MnEolCr, eol::Eol::Cr),
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
            .open_at(r.x, r.y, items, host, px(260.0, self.scale));
    }

    /// 상태줄 인코딩 팝업(사용자 09-16 · Sublime 두 메뉴를 한 팝업에): 위 = "다른 인코딩으로 다시 열기 ▸"(파일 탭일 때만 ·
    /// 파일을 그 인코딩으로 다시 디코드) · 아래 = 저장 인코딩 목록(현재 = ✓ 강조). 유니코드 4종 뒤 구분선.
    fn open_enc_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let cur = self.editors.active_encoding();
        let mark = |on: bool, s: &str| {
            if on {
                format!("✓ {s}")
            } else {
                format!("   {s}")
            }
        };
        let list = |prefix: &str, with_mark: bool| -> Vec<CtxItem> {
            let mut v: Vec<CtxItem> = Vec::new();
            for (i, e) in enc::LIST.iter().enumerate() {
                if i == enc::UNICODE_COUNT {
                    v.push(CtxItem::Separator);
                }
                let label = if with_mark {
                    mark(cur == e.id, t(e.label))
                } else {
                    t(e.label).to_string()
                };
                let item = CtxItem::item(format!("{prefix}{}", e.id), label);
                v.push(if with_mark {
                    item.with_active(cur == e.id)
                } else {
                    item
                });
            }
            v
        };
        let mut items: Vec<CtxItem> = Vec::new();
        if self.editors.active_path().is_some() {
            items.push(CtxItem::submenu(
                "enc.reopen",
                t(Msg::MnReopenEnc),
                list("enc.reopen:", false),
            ));
            items.push(CtxItem::Separator);
        }
        items.extend(list("enc.set:", true));
        let r = self.status_enc_rect;
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
            .open_at(r.x, r.y, items, host, px(280.0, self.scale));
    }

    /// 툴바 우클릭 = 그룹 띄우기/붙이기 · 버튼 표시 여부 토글(설정 `toolbar.hidden`) · 배치 초기화(사용자 09-16 · 09-17).
    fn open_toolbar_menu(&mut self, x: i32, y: i32) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let hidden = self.hidden_toolbar_ids();
        let mut items: Vec<CtxItem> = self
            .tool_dock
            .groups()
            .into_iter()
            .map(|(id, title, floating)| {
                let verb = t(if floating {
                    Msg::MnDockGroup
                } else {
                    Msg::MnFloatGroup
                });
                CtxItem::item(format!("tbg:{id}"), format!("{title} — {verb}"))
                    .with_checked(floating)
            })
            .collect();
        items.extend(TOOLBAR_ITEMS.iter().map(|(id, m)| {
            CtxItem::item(format!("tb:{id}"), t(*m)).with_checked(!hidden.contains(&id.to_string()))
        }));
        items.push(CtxItem::item("tb.reset", t(Msg::MnResetToolbar)));
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(Rect::new(x, y, 0, 0));
        self.status_menu.set_scale(self.scale);
        self.status_menu
            .open_at(x, y, items, host, px(220.0, self.scale));
    }

    fn hidden_toolbar_ids(&self) -> Vec<String> {
        self.settings
            .get("toolbar.hidden")
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// 설정 `toolbar.hidden` → 툴바 버튼 표시 여부.
    fn apply_toolbar_visibility(&mut self) {
        let hidden = self.hidden_toolbar_ids();
        let mut inv = Invalidations::default();
        for (id, _) in TOOLBAR_ITEMS {
            self.tool_dock
                .set_item_visible(id, !hidden.contains(&id.to_string()), &mut inv);
        }
        for f in &self.tool_floats {
            f.redraw();
        }
    }

    // ── 툴바 그룹 도크 · 플로팅(사용자 09-17)

    /// 설정 `toolbar.layout` → 도크 배치 + 플로팅 창(시작 시 · 설정 창에서 값을 바꿨을 때).
    fn apply_tool_layout_setting(&mut self) {
        let text = self
            .settings
            .get("toolbar.layout")
            .unwrap_or("")
            .to_string();
        let layout = DockLayout::parse(&text);
        // 열린 플로팅 창은 전부 닫고 배치대로 다시 연다(단순 · 드물다).
        for f in &mut self.tool_floats {
            f.close();
        }
        self.tool_floats.clear();
        self.tool_dock.apply_layout(&layout);
        let _ = self.tool_dock.take_actions();
        for (id, x, y) in layout.floating {
            self.pending_float.push((id, Some((x, y))));
        }
        self.layout();
        self.redraw();
    }

    /// 현재 배치를 설정에 저장(그룹 순서 · 플로팅 좌표 — 포터블 배포에서도 `NSQL_HOME` 아래 같은 파일).
    fn save_tool_layout(&mut self) {
        self.tool_layout_dirty = false;
        let text = self.tool_dock.layout().serialize();
        if self.settings.get("toolbar.layout") != Some(text.as_str()) {
            let _ = self.settings.set("toolbar.layout", &text);
            self.persist_settings();
        }
    }

    /// 도크가 보고한 일(떼어 내기 · 배치 변경)을 거둔다.
    fn drain_dock_actions(&mut self) {
        for a in self.tool_dock.take_actions() {
            match a {
                DockAction::Float { id, x, y } => self.pending_float.push((id, Some((x, y)))),
                DockAction::LayoutChanged => self.tool_layout_dirty = true,
            }
        }
    }

    /// 플로팅 창 만들기 — `at`: 도크 클라이언트 좌표(커서) 또는 저장된 화면 좌표(`screen=true`).
    fn open_float(&mut self, el: &ActiveEventLoop, gid: &str, at: Option<(i32, i32)>) {
        if self.tool_floats.iter().any(|f| f.group == gid) {
            return;
        }
        let Some(title) = self.tool_dock.title(gid).map(str::to_string) else {
            return;
        };
        let scale = self.scale.max(0.5);
        let (bw, bh) = {
            let h = self.tool_dock.preferred_height() as f32;
            let w = self
                .tool_dock
                .bar(gid)
                .map_or(120.0, |b| b.preferred_width() as f32 / scale);
            (w, h)
        };
        // 저장된 좌표(이미 플로팅으로 표시)면 그대로, 아니면 커서(클라이언트) → 화면 좌표.
        let pos = if let Some(p) = self.tool_dock.floating_pos(gid) {
            Some(p)
        } else {
            at.and_then(|(cx, cy)| {
                let w = self.window.as_ref()?;
                let o = w.inner_position().ok()?;
                Some((
                    o.x + cx - (bw * scale / 2.0) as i32,
                    o.y + cy - (bh * scale / 2.0) as i32,
                ))
            })
        };
        let owner = self.window.clone();
        let win = ToolFloatWin::open(
            el,
            gid,
            &format!("Nexa SQL — {title}"),
            theme::window_theme(self.settings.theme_mode()),
            pos,
            (bw, bh),
            owner.as_deref(),
        );
        let Some(win) = win else { return };
        let (x, y) = win.position().or(pos).unwrap_or((0, 0));
        self.tool_dock.float(gid, x, y);
        let _ = self.tool_dock.take_actions();
        self.tool_layout_dirty = true;
        self.tool_floats.push(win);
        self.layout();
        self.redraw();
    }

    /// 그룹을 도크로 되돌린다(플로팅 창 닫기 포함).
    fn dock_group(&mut self, gid: &str) {
        if let Some(i) = self.tool_floats.iter().position(|f| f.group == gid) {
            self.tool_floats[i].close();
            self.tool_floats.remove(i);
        }
        self.tool_dock.dock(gid);
        let _ = self.tool_dock.take_actions();
        self.tool_layout_dirty = true;
        self.layout();
        self.redraw();
    }

    /// 툴바 초기화(View 메뉴 · 우클릭 · 사용자 09-17) — 그룹 전부 도크 · 정의 순서 · 숨긴 버튼 복원.
    fn reset_toolbar(&mut self) {
        for f in &mut self.tool_floats {
            f.close();
        }
        self.tool_floats.clear();
        self.pending_float.clear();
        self.tool_dock.reset();
        let _ = self.tool_dock.take_actions();
        let _ = self.settings.reset("toolbar.layout");
        let _ = self.settings.reset("toolbar.hidden");
        self.persist_settings();
        self.apply_toolbar_visibility();
        self.tool_layout_dirty = false;
        self.layout();
        self.redraw();
    }

    fn indent_pick(&mut self, id: &str) {
        let (ts, spaces) = self.editors.indent();
        if let Some(e) = id.strip_prefix("enc.set:") {
            self.editors.set_active_encoding(e);
            self.status = tf(Msg::StEncSet, &[&enc::label(e)]);
            return;
        }
        if let Some(e) = id.strip_prefix("enc.reopen:") {
            if let Some(path) = self.editors.active_path() {
                let e = e.to_string();
                self.open_file_enc(&path, &e);
            }
            return;
        }
        if id == "demo.create" {
            self.start_demo_create();
            return;
        }
        if id == "demo.later" || id == "demo.ask" {
            return;
        }
        if let Some(gid) = id.strip_prefix("tbg:") {
            if self.tool_dock.is_floating(gid) {
                self.dock_group(gid);
            } else {
                // 커서 자리에 띄운다(창은 이벤트 루프 핸들에서).
                let at = Some(self.cursor);
                self.pending_float.push((gid.to_string(), at));
            }
            return;
        }
        if id == "tb.reset" {
            self.reset_toolbar();
            return;
        }
        if let Some(tid) = id.strip_prefix("tb:") {
            let mut hidden = self.hidden_toolbar_ids();
            if let Some(i) = hidden.iter().position(|h| h == tid) {
                hidden.remove(i);
            } else {
                hidden.push(tid.to_string());
            }
            let _ = self.settings.set("toolbar.hidden", &hidden.join(","));
            self.persist_settings();
            self.apply_toolbar_visibility();
            self.layout();
            return;
        }
        if let Some(rest) = id.strip_prefix("tx.") {
            self.tx_pick(rest);
            self.redraw();
            return;
        }
        match id {
            "eol.lf" => self.editors.set_active_eol(eol::Eol::Lf),
            "eol.crlf" => self.editors.set_active_eol(eol::Eol::Crlf),
            "eol.cr" => self.editors.set_active_eol(eol::Eol::Cr),
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

    /// 설정 → 편집기 안내선(표시 · 색 · 투명도) + 동일 출현 외곽선(사용자 09-16).
    /// 끈 확장 id 목록(`extensions.disabled`).
    fn ext_disabled(&self) -> Vec<String> {
        self.settings
            .get("extensions.disabled")
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    }

    /// 확장 효과 적용(시작 · 설정 변경 · 켜기/끄기): 레지스트리 → 편집기 전 탭(괄호 옵션 · 우클릭 서브메뉴).
    fn apply_extensions(&mut self, changed_key: Option<&str>) {
        // 끈 것 + **설치 기록 없는 내장 확장** = 효과 없음(설치해야 켜진다 · 사용자 09-17).
        let installed = extensions::manager::installed();
        let mut disabled = self.ext_disabled();
        for (id, _) in self.extensions.builtin_ids() {
            if !installed.iter().any(|r| r.id == id) && !disabled.iter().any(|d| d == id) {
                disabled.push(id.to_string());
            }
        }
        let effects = self
            .extensions
            .on_settings(&self.settings, changed_key, &disabled);
        for e in effects {
            if let Some(b) = e.bracket_opts {
                self.editors.set_bracket_opts(b);
            }
        }
        let km = &self.keymap;
        let extras = self
            .extensions
            .menu_extras(&disabled, &|id| km.display_of(id));
        self.editors.set_menu_extras(extras);
        // 설정 창: 끈/미설치 확장의 분류는 숨긴다(사용자 09-17 "설치되면 보이고 제거하면 사라진다").
        let hidden: Vec<Msg> = nsql_settings::EXTENSION_CATEGORIES
            .iter()
            .filter(|(_, id)| disabled.iter().any(|d| d == id))
            .map(|(c, _)| *c)
            .collect();
        self.prefs_win.set_hidden_categories(hidden);
        self.prefs_sync();
        self.redraw();
    }

    /// 코드가 설정을 바꾼 뒤(확장 켜기/끄기 · 저장소 추가 · 관리자 활성화) 열려 있는 설정 창의 스냅샷을 다시 읽는다
    /// (사용자 09-17 "팔레트에서 켜도 설정 창은 꺼진 채").
    fn prefs_sync(&mut self) {
        self.prefs_win.refresh(&self.settings);
        self.prefs_win.redraw();
    }

    /// 확장 명령(짝/형제/상위/하위 이동 등) — 소유 확장에 위임 · 켜져 있을 때만.
    fn run_extension_cmd(&mut self, id: &str) {
        let disabled = self.ext_disabled();
        if self.extensions.run(id, &disabled, self.editors.cur_mut()) {
            self.redraw();
        }
    }

    /// Extension Manager 팔레트 명령(Sublime Package Control 방식 · docs/50 §10): 텍스트 목록을 만들어 팔레트에 띄우고,
    /// 고르면 `ext.<verb>:<key>`로 다시 들어온다.
    fn ext_command(&mut self, id: &str) {
        use extensions::manager as mgr;
        let disabled = self.ext_disabled();
        let builtin: Vec<(String, String)> = self
            .extensions
            .builtin_ids()
            .into_iter()
            .map(|(i, n)| (i.to_string(), n.to_string()))
            .collect();
        let installed = mgr::installed();
        // builtin도 **설치 기록**이 있어야 설치된 것(설치 = 켜기 + 설정 분류 표시 · 삭제 = 끄기 + 숨김 · 사용자 09-17).
        let is_installed = |x: &str| installed.iter().any(|r| r.id == x);
        let _ = &builtin;
        let mut cmds: Vec<(String, String)> = Vec::new();
        // Package Control처럼 1회 활성화 — 켜기 전에는 목록/설치를 막는다(네트워크 사용을 알리는 지점).
        if id == "ext.enable_mgr" {
            let _ = self.settings.set("extensions.enabled", "on");
            let _ = self.settings.save();
            self.status = tf(
                Msg::StExtManagerEnabled,
                &[&mgr::default_source(&self.settings).display()],
            );
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.status.clone()));
            self.prefs_sync();
            self.redraw();
            return;
        }
        if !self.settings.flag("extensions.enabled") {
            self.status = t(Msg::StExtManagerOff).into();
            self.redraw();
            return;
        }
        match id {
            "ext.install" => {
                self.ext_catalog.clear();
                for src in mgr::sources(&self.settings) {
                    let mut tr = mgr::Trace::default();
                    let r = mgr::fetch_index_traced(&src, &mut tr);
                    self.ext_trace(tr);
                    match r {
                        Ok(idx) => {
                            for p in idx.packages.into_iter().filter(|p| !is_installed(&p.id)) {
                                let n = self.ext_catalog.len();
                                cmds.push((
                                    format!("ext.install:{n}"),
                                    format!(
                                        "{} {} — {} [{}] · {}",
                                        p.name,
                                        p.version,
                                        p.summary,
                                        p.kind.as_str(),
                                        src.display()
                                    ),
                                ));
                                self.ext_catalog.push((src.clone(), p));
                            }
                        }
                        Err(e) => {
                            self.status = tf(Msg::StExtIndexFailed, &[&src.display(), &e]);
                            self.log_win
                                .push(LogEntry::new(LogKind::Error, self.status.clone()));
                        }
                    }
                }
                if cmds.is_empty() {
                    self.status = t(Msg::StExtNoneAvailable).into();
                    self.redraw();
                    return;
                }
            }
            "ext.remove" | "ext.list" | "ext.enable" | "ext.disable" => {
                let verb = id.trim_start_matches("ext.");
                for r in &installed {
                    let off = disabled.iter().any(|d| d == &r.id);
                    let ok = match verb {
                        "enable" => off,
                        "disable" => !off,
                        _ => true,
                    };
                    if ok {
                        cmds.push((
                            format!("ext.{verb}:{}", r.id),
                            format!(
                                "{} {} · {} · {}",
                                r.name,
                                r.version,
                                r.kind.as_str(),
                                if off {
                                    t(Msg::StExtDisabled)
                                } else {
                                    t(Msg::StExtEnabled)
                                }
                            ),
                        ));
                    }
                }
                if cmds.is_empty() {
                    self.status = t(Msg::StExtNoneInstalled).into();
                    self.redraw();
                    return;
                }
            }
            "ext.repo_add" => {
                self.palette
                    .open_prompt("ext.repo_add", t(Msg::PhExtRepoUrl), "");
                self.redraw();
                return;
            }
            "ext.repo_list" | "ext.repo_remove" => {
                let user = mgr::user_sources(&self.settings);
                if id == "ext.repo_list" {
                    cmds.push((
                        "ext.repo:default".into(),
                        format!(
                            "{} {}",
                            mgr::default_source(&self.settings).display(),
                            t(Msg::StExtRepoDefault)
                        ),
                    ));
                }
                for (n, s) in user.iter().enumerate() {
                    cmds.push((
                        format!(
                            "{}:{n}",
                            if id == "ext.repo_list" {
                                "ext.repo"
                            } else {
                                "ext.repo_remove"
                            }
                        ),
                        s.display(),
                    ));
                }
                if cmds.is_empty() {
                    self.status = t(Msg::StExtNoneInstalled).into();
                    self.redraw();
                    return;
                }
            }
            _ => return,
        }
        self.palette.set_commands(cmds);
        self.palette.open("");
        self.redraw();
    }

    /// 팔레트에서 고른 확장 항목(`ext.<verb>:<key>`).
    fn ext_pick(&mut self, id: &str) {
        use extensions::manager as mgr;
        let (verb, key) = match id.trim_start_matches("ext.").split_once(':') {
            Some(p) => p,
            None => return,
        };
        match verb {
            "install" => {
                let Some((src, sum)) = key
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| self.ext_catalog.get(n).cloned())
                else {
                    return;
                };
                let mut tr = mgr::Trace::default();
                let r = mgr::install(&src, &sum, &mut tr);
                self.ext_trace(tr);
                match r {
                    Ok(meta) => {
                        let note = if meta.message_install.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", meta.message_install)
                        };
                        self.status = tf(Msg::StExtInstalled, &[&meta.name, &meta.version, &note]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.status.clone()));
                        self.apply_extensions(None);
                    }
                    Err(e) => {
                        self.status = e;
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, self.status.clone()));
                    }
                }
            }
            "remove" => {
                let mut tr = mgr::Trace::default();
                let r = mgr::remove(key, &mut tr);
                self.ext_trace(tr);
                match r {
                    Ok(()) => {
                        self.status = tf(Msg::StExtRemoved, &[key]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.status.clone()));
                        self.apply_extensions(None);
                    }
                    Err(e) => self.status = e,
                }
            }
            "enable" | "disable" => {
                let cur = self
                    .settings
                    .get("extensions.disabled")
                    .unwrap_or("")
                    .to_string();
                let next = mgr::list_toggle(&cur, key, verb == "disable");
                let _ = self.settings.set("extensions.disabled", &next);
                let _ = self.settings.save();
                self.status = tf(
                    if verb == "disable" {
                        Msg::StExtDisabled
                    } else {
                        Msg::StExtEnabled
                    },
                    &[key],
                );
                self.apply_extensions(None);
            }
            "list" => {
                let disabled = self.ext_disabled();
                let state = if disabled.iter().any(|d| d == key) {
                    t(Msg::StExtDisabled)
                } else {
                    t(Msg::StExtEnabled)
                };
                let (name, ver, kind) = mgr::installed()
                    .into_iter()
                    .find(|r| r.id == key)
                    .map(|r| (r.name, r.version, r.kind.as_str().to_string()))
                    .unwrap_or_default();
                self.status = tf(Msg::StExtInfo, &[&name, &ver, &kind, state]);
            }
            "repo" => {
                self.status = if key == "default" {
                    mgr::default_source(&self.settings).display()
                } else {
                    key.parse::<usize>()
                        .ok()
                        .and_then(|n| {
                            mgr::user_sources(&self.settings)
                                .get(n)
                                .map(|s| s.display())
                        })
                        .unwrap_or_default()
                };
            }
            "repo_remove" => {
                let user = mgr::user_sources(&self.settings);
                if let Some(s) = key.parse::<usize>().ok().and_then(|n| user.get(n)) {
                    let cur = self
                        .settings
                        .get("extensions.repositories")
                        .unwrap_or("")
                        .to_string();
                    let next = mgr::list_toggle(&cur, &s.display(), false);
                    let _ = self.settings.set("extensions.repositories", &next);
                    let _ = self.settings.save();
                    self.status = tf(Msg::StExtRepoRemoved, &[&s.display()]);
                    self.prefs_sync();
                }
            }
            _ => {}
        }
        self.redraw();
    }

    /// 매니저 추적 줄 → 로그 창 `ext` 층(개발자 모드 · `log.dev_layers`에 ext · 사용자 09-17 "다운로드 속도·설치 폴더까지").
    fn ext_trace(&mut self, tr: extensions::manager::Trace) {
        for line in tr.0 {
            dlog!(self, LogLayer::Ext, LogLevel::Timing, {
                LogEntry::new(LogKind::Info, line)
            });
        }
    }

    /// "Add Repository" 프롬프트 확정 — 루트에 index.json이 읽히면 설정에 더한다.
    fn ext_repo_add(&mut self, text: &str) {
        use extensions::manager as mgr;
        let src = mgr::Source::parse(text);
        if text.trim().is_empty() || mgr::fetch_index(&src).is_err() {
            self.status = tf(Msg::StExtRepoBad, &[text.trim()]);
        } else {
            let cur = self
                .settings
                .get("extensions.repositories")
                .unwrap_or("")
                .to_string();
            let next = mgr::list_toggle(&cur, &src.display(), true);
            let _ = self.settings.set("extensions.repositories", &next);
            let _ = self.settings.save();
            self.status = tf(Msg::StExtRepoAdded, &[&src.display()]);
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.status.clone()));
            self.prefs_sync();
        }
        self.redraw();
    }

    /// 파일 탭 강조색(`editor.tab_accent` · 비면 테마 accent = 결과 탭과 같음).
    fn apply_tab_accent(&mut self) {
        let c = self
            .settings
            .get("editor.tab_accent")
            .and_then(nexa_ctl::theme::color_from_hex);
        self.editors.set_tab_accent(c);
    }

    fn apply_ruler_style(&mut self) {
        let show = self.settings.flag("editor.rulers_show");
        let (color, hex_alpha) = color_alpha_setting(&self.settings, "editor.ruler_color");
        // `#RRGGBBAA`의 AA가 있으면 별도 불투명도 설정보다 우선(색 창이 알파를 함께 저장 · 09-17).
        let alpha = hex_alpha
            .unwrap_or(self.settings.int("editor.ruler_alpha").clamp(5, 100) as f32 / 100.0);
        let occ = self.settings.flag("editor.highlight_selection");
        self.editors.set_ruler_style(show, color, alpha, occ);
    }

    /// 동일 출현 상자 스타일(`editor.occurrence_*` · 사용자 09-17): 모양 · 선 색+알파 · 두께 · 배경 색+알파.
    fn apply_occurrence_style(&mut self) {
        let (line, la) = color_alpha_setting(&self.settings, "editor.occurrence_line_color");
        let (fill, fa) = color_alpha_setting(&self.settings, "editor.occurrence_fill_color");
        let st = nexa_ctl::OccurrenceStyle {
            round: self.settings.get("editor.occurrence_shape") == Some("round"),
            line,
            line_alpha: la.unwrap_or(0.7),
            width: self
                .settings
                .int("editor.occurrence_line_width")
                .clamp(0, 4) as i32,
            fill,
            fill_alpha: fa.unwrap_or(if fill.is_some() { 1.0 } else { 0.0 }),
        };
        self.editors.set_occurrence_style(st);
    }

    /// 설정 → nexa-gfx 텍스트 렌더(대비 감마 · 정수 스냅) — 전 창 공통(글리프 캐시 키에 감마가 들어 있어 비울 필요 없음).
    fn apply_text_render(&self) {
        let pct = self.settings.int("ui.text_contrast").clamp(100, 250) as f32;
        nexa_gfx::text::set_text_contrast(pct / 100.0);
        nexa_gfx::text::set_text_snap(self.settings.flag("ui.text_snap"));
        nexa_gfx::text::set_text_hint(self.settings.flag("ui.text_hint"));
        nexa_gfx::text::set_text_gdi(self.settings.flag("ui.text_gdi"));
        nexa_gfx::text::set_text_weight(
            self.settings.int("ui.text_weight").clamp(0, 60) as f32 / 100.0,
        );
    }

    /// 설정 → 미니맵(T-97).
    fn apply_minimap(&mut self) {
        let on = self.settings.flag("editor.minimap");
        let w = self.settings.int("editor.minimap_width").clamp(20, 400) as i32;
        self.editors.set_minimap(on, w);
        let (color, alpha) = color_alpha_setting(&self.settings, "editor.minimap_box_color");
        self.editors
            .set_minimap_box(color, alpha, self.settings.flag("editor.minimap_border"));
        self.editors.set_minimap_opts(
            self.settings.get("editor.minimap_viewport") == Some("hover"),
            self.settings.get("editor.minimap_click") == Some("text"),
            self.settings.flag("editor.minimap_find"),
        );
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
        // Auto indent(docs/49): 변수 4 + 규칙 세트.
        let cfg = nexa_ctl::AutoIndent {
            enabled: self.settings.flag("editor.auto_indent"),
            smart: self.settings.flag("editor.smart_indent"),
            to_bracket: self.settings.flag("editor.indent_to_bracket"),
            trim: self.settings.flag("editor.trim_auto_whitespace"),
        };
        let rules = match self.settings.get("editor.indent_rules").unwrap_or("sql") {
            "brackets" => nexa_ctl::IndentRules::brackets(),
            "none" => nexa_ctl::IndentRules::none(),
            _ => nexa_ctl::IndentRules::sql(),
        };
        self.editors.set_auto_indent(cfg, rules);
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
        // 미커밋 문장이 있으면 먼저 묻는다(잃는 순간만 모달 · DR-30).
        if !self.tx_pending.is_empty() {
            self.tx_after = Some(TxAfter::Disconnect);
            self.open_tx_guard(Msg::MnTxCommitDisconnect, Msg::MnTxRollbackDisconnect);
            self.redraw();
            return;
        }
        self.disconnect_force();
    }

    fn disconnect_force(&mut self) {
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
        self.tx_close(TxOutcome::Lost);
        self.sync_disconnect_btn(false);
        self.on_conn_disconnected();
        self.redraw();
    }

    /// 접속 계열 결과 `Disconnected`의 UI 반영(워커 이벤트 · 즉시 해제 공용).
    fn on_conn_disconnected(&mut self) {
        self.live_sid = None;
        // 해제됐으니 "접속됨" 결과는 더 이상 사실이 아니다(테스트 결과는 유지).
        self.panel_results
            .retain(|_, st| !matches!(st, ConnState::Connected(_)));
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
        self.tool_dock
            .set_item_enabled("conn.disconnect", connected, &mut inv);
        // 연결이 하나라도 있으면 Connect 아이콘 = 밝은 녹색(사용자 09-16).
        self.tool_dock.set_item_tone(
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
                fade_ms(&self.settings, key, 5000),
            ),
            "ui.fade_slow" => nexa_ctl::tokens::set_fade_ms(
                nexa_ctl::tokens::FadeSpeed::Slow,
                fade_ms(&self.settings, key, 5000),
            ),
            // 애니메이션 마스터(auto/on/off · 향상 모드 off) = 페이드·슬라이드 전부 0으로/복귀.
            "ui.animations" => {
                for k in [
                    "ui.fade_fast",
                    "ui.fade_slow",
                    "ui.fade_out_ms",
                    "ui.slide_ms",
                ] {
                    self.apply_setting(k);
                }
            }
            // 프레임 상한·캐럿 깜빡임은 about_to_wait가 매번 설정을 읽는다(즉시 반영).
            "ui.max_fps" | "editor.caret_blink" => self.redraw(),
            "ui.slide_ms" | "ui.tooltip_delay_ms" | "ui.dblclick_ms" => {
                self.conn_win.set_tuning(conn_tuning(&self.settings));
            }
            "probe.interval" | "probe.max_retries" | "probe.max_inflight" | "probe.icmp"
            | "probe.timeout" | "probe.retry_delay" | "probe.enabled" => {
                let n = self.settings.int("probe.max_inflight").clamp(1, 64) as usize;
                self.conn_win.set_policy(probe_policy(&self.settings), n);
            }
            "file.os_icons" => nexa_fs::shell::set_os_icons(self.settings.flag(key)),
            "file.probe_chevrons" => nexa_dlg::set_probe_chevrons(self.settings.flag(key)),
            "ui.toast_secs" | "ui.toast_alpha" => {
                self.toasts.configure(
                    self.settings.int("ui.toast_secs"),
                    self.settings.int("ui.toast_alpha"),
                );
                self.apply_run_toast();
            }
            "run.toast" | "run.toast_hide_secs" => self.apply_run_toast(),
            "mssql.encrypt" => {
                nsql_drivers::set_mssql_encryption(self.settings.get(key) == Some("login"))
            }
            "mssql.cancel" => {
                nsql_drivers::set_mssql_cancel_socket(self.settings.get(key) == Some("socket"))
            }
            "ui.hover_intent_ms" => {
                nexa_ctl::tokens::set_intent_ms(i(&self.settings, key).clamp(0, 500) as u64)
            }
            "ui.fade_out_ms" => {
                nexa_ctl::tokens::set_fade_out_ms(fade_ms(&self.settings, key, 2000))
            }
            "input.scroll_natural" => {
                input::set_natural_scroll(self.settings.flag(key));
            }
            "explorer.visible" => {
                self.explorer.set_visible(self.settings.flag(key));
                self.layout();
            }
            "explorer.icons" => self.explorer.set_icons(self.settings.flag(key)),
            "ui.text_contrast" | "ui.text_snap" | "ui.text_hint" | "ui.text_weight" => {
                self.apply_text_render();
                self.log_win.redraw();
            }
            "toolbar.hidden" => {
                self.apply_toolbar_visibility();
                self.layout();
            }
            "toolbar.layout" => {
                // 설정 창에서 직접 바꿨을 때(비우면 초기 배치). 앱이 저장한 값과 같으면 아무 일도 없다.
                if self.settings.get(key).unwrap_or("") != self.tool_dock.layout().serialize() {
                    self.apply_tool_layout_setting();
                }
            }
            "statusbar.git" | "statusbar.git_secs" => {
                self.git
                    .set_interval(self.settings.int("statusbar.git_secs").max(2) as u64);
                self.git.refresh(true);
            }
            "grid.font_face" => {
                let pref = self.settings.get("editor.font_face").map(str::to_string);
                self.grid_font =
                    load_grid_font(self.settings.get(key).unwrap_or(""), pref.as_deref());
            }
            "editor.font_face" => {
                // 고정폭 얼굴 교체(편집기 · 행번호 · 그리드 'mono') — 못 찾으면 사슬 fail-over라 항상 Some.
                let pref = self.settings.get(key).map(str::to_string);
                if let Some(l) = nexa_font::mono_font(pref.as_deref()) {
                    self.mono_font = l.font;
                }
                let gf = self
                    .settings
                    .get("grid.font_face")
                    .unwrap_or("")
                    .to_string();
                self.grid_font = load_grid_font(&gf, pref.as_deref());
                self.layout();
            }
            "grid.col_min_width" | "grid.col_max_mode" | "grid.col_max_chars" => {
                let (lo, chars) = (
                    self.settings.int("grid.col_min_width") as i32,
                    self.settings.grid_col_max_chars() as i32,
                );
                self.all_grids().for_each(|g| g.set_col_limits(lo, chars));
                self.redraw();
            }
            "grid.row_numbers" => {
                let on = self.settings.flag(key);
                self.all_grids().for_each(|g| g.set_row_numbers(on));
            }
            "grid.row_height_pct" => {
                let pct = self.settings.int(key).clamp(110, 300) as i32;
                self.all_grids().for_each(|g| g.set_row_pct(pct));
            }
            "grid.null_text" => {
                let text = self.settings.get(key).unwrap_or("NULL").to_string();
                self.all_grids().for_each(|g| g.set_null_text(&text));
            }
            "window.always_on_top" => self.apply_on_top(),
            "log.always_on_top" => self.log_win.set_on_top(self.settings.flag(key)),
            "grid.scroll" => {
                let on = self.settings.get(key) == Some("row");
                self.grid.set_row_snap(on);
                self.log_win.set_row_snap(on);
            }
            "editor.scroll" => self
                .editors
                .set_scroll_snap(self.settings.get(key) == Some("row")),
            "editor.line_numbers" => self.editors.set_line_numbers(self.settings.flag(key)),
            "editor.diff_marks" => self.editors.set_diff_marks(self.settings.flag(key)),
            "editor.minimap"
            | "editor.minimap_width"
            | "editor.minimap_box_color"
            | "editor.minimap_border"
            | "editor.minimap_viewport"
            | "editor.minimap_click"
            | "editor.minimap_find"
            | "editor.minimap_errors" => self.apply_minimap(),
            // 자원 거버너(T-90a/d · docs/39): 상한 세터 4종 · 모드가 바뀌면 원장 키 전부 재적용(실효 값이 바뀌므로).
            "log.max_lines" => self
                .log_win
                .set_max_lines(self.settings.int(key).max(100) as usize),
            "editor.undo_max" => self
                .editors
                .set_undo_max(self.settings.int(key).max(1) as usize),
            "ui.glyph_cache" => {
                nexa_gfx::text::set_glyph_cache_max(self.settings.int(key).max(256) as usize);
            }
            "file.icon_cache" => {
                nexa_fs::shell::set_icon_cache_max(self.settings.int(key).max(16) as usize);
            }
            "txlog.max_entries" => {
                let n = self.settings.int("txlog.max_entries").max(16) as usize;
                self.txlog.set_cap(n);
            }
            "perf.boost" => {
                // 향상 모드 켬/끔 = 강제 대상 키를 전부 다시 적용(실효 값이 바뀐다 · 저장값은 그대로).
                let keys: Vec<&'static str> =
                    nsql_settings::perf::BOOST.iter().map(|(k, _)| *k).collect();
                for k in keys {
                    self.apply_setting(k);
                }
                let on = self.settings.flag(key);
                self.status = t(if on {
                    Msg::StPerfBoostOn
                } else {
                    Msg::StPerfBoostOff
                })
                .into();
                self.layout();
            }
            "ui.menu_icons" => nexa_ctl::controls::set_menu_icons(self.settings.flag(key)),
            "ui.clipboard_probe" => {}
            "perf.mode" => {
                let keys: Vec<&'static str> = nsql_settings::PERF.iter().map(|(k, _)| *k).collect();
                for k in keys {
                    self.apply_setting(k);
                }
                self.status = tf(
                    Msg::StPerfMode,
                    &[t(self.settings.perf_mode_display().label())],
                );
            }
            "editor.tab_size"
            | "editor.indent_spaces"
            | "editor.tab_stops"
            | "editor.auto_indent"
            | "editor.smart_indent"
            | "editor.indent_to_bracket"
            | "editor.trim_auto_whitespace"
            | "editor.indent_rules" => self.apply_indent(),
            "file.eol_new" => self
                .editors
                .set_default_eol(eol::default_eol(self.settings.get(key).unwrap_or("auto"))),
            "editor.rulers" => self
                .editors
                .set_rulers(parse_rulers(self.settings.get(key).unwrap_or("80"))),
            "editor.rulers_show"
            | "editor.ruler_color"
            | "editor.ruler_alpha"
            | "editor.highlight_selection" => self.apply_ruler_style(),
            k if k.starts_with("editor.occurrence_") => self.apply_occurrence_style(),
            "editor.tab_accent" => self.apply_tab_accent(),
            "extensions.disabled" => self.apply_extensions(None),
            k if k.starts_with("rainbowpair.") => self.apply_extensions(Some(k)),
            "editor.text_pad_left" => self
                .editors
                .set_text_inset(self.settings.int(key).clamp(0, 32) as i32),
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
            "grid.max_rows" => {
                let n = self.settings.int(key).max(0) as usize;
                self.all_grids().for_each(|g| g.set_default_page_rows(n));
            }
            "grid.auto_fetch" => {
                let on = self.settings.flag(key);
                self.all_grids().for_each(|g| g.set_auto_fetch(on));
            }
            "grid.result_tabs" | "grid.result_tabbar" => self.apply_result_tab_opts(),
            "tabs.rows" => {
                let multi = self.settings.get(key) != Some("single");
                self.editors.set_multiline_tabs(multi);
                self.layout();
            }
            "log.wrap" => self.log_win.set_wrap(self.settings.flag(key)),
            "log.dev_mode" | "log.dev_layers" => self.apply_detail_mask(),
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
                let ws = whitespace_style(&self.settings);
                self.editors.set_whitespace(ws);
                // 설정 창에서 바꾼 직후 편집기에 바로 보여야 한다(사용자 09-17) · 로그 창에 적용 사실을 남긴다(진단).
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    tf(
                        Msg::StWhitespaceApplied,
                        &[
                            self.settings
                                .get("editor.whitespace")
                                .unwrap_or("selection"),
                            self.settings.get("editor.whitespace_chars").unwrap_or(""),
                        ],
                    ),
                ));
                self.redraw();
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
        // 결과 도구줄(docs/43 §4-2): 추가/전체/건수 · 새로고침 · SQL 보기.
        if let Some(req) = self.grid.take_fetch_request() {
            self.send_fetch(req);
        }
        if self.grid.take_cancel_request() {
            self.worker.cancel_fetch();
            self.status = t(Msg::StFetchCancelling).into();
        }
        if self.grid.take_refresh() {
            self.refresh_result();
        }
        if let Some(kind) = self.grid.take_pending_view() {
            self.begin_view_sql(kind);
        }
    }

    /// 캐럿을 다음/이전 문장(`;` 분리 · [`nsql_script::split_script`]) 시작으로(Alt+↓/↑ · 실행 뒤 자동 이동).
    fn goto_statement(&mut self, forward: bool) {
        let full = self.ed_mut().text();
        let caret = self.ed_mut().caret();
        let byte = full
            .char_indices()
            .nth(caret)
            .map_or(full.len(), |(b, _)| b);
        let items = nsql_script::split_script(&full);
        let cur = items
            .iter()
            .position(|it| it.span.start <= byte && byte <= it.span.end);
        let idx = if forward {
            match cur {
                Some(c) => Some(c + 1),
                None => items.iter().position(|it| it.span.start > byte),
            }
        } else {
            match cur {
                Some(c) => c.checked_sub(1),
                None => items.iter().rposition(|it| it.span.end < byte),
            }
        };
        let Some(it) = idx.and_then(|i| items.get(i)) else {
            return;
        };
        let ci = full[..it.span.start.min(full.len())].chars().count();
        let mut inv = Invalidations::default();
        self.ed_mut().select_range(ci, ci, &mut inv);
        self.redraw();
    }

    /// 결과 탭 id로 그리드 찾기 — 활성이면 `grid` · 아니면 잠든 것(활성 패널의 다른 탭 · 잠든 패널의 탭).
    fn grid_for(&mut self, key: u64) -> Option<&mut grid::Grid> {
        if key == self.grid_tab {
            return Some(&mut self.grid);
        }
        if let Some(t) = self.panel.tabs.iter_mut().find(|t| t.id == key) {
            return Some(&mut t.grid);
        }
        self.panels
            .values_mut()
            .flat_map(|p| p.tabs.iter_mut())
            .find(|t| t.id == key)
            .map(|t| &mut t.grid)
    }

    /// 잠든 그리드 전부(활성 패널의 비활성 탭 + 잠든 패널의 탭 · 활성 자리표시자 포함 — 빈 그리드라 무해).
    fn sleeping_grids(&self) -> impl Iterator<Item = &grid::Grid> {
        self.panel.tabs.iter().map(|t| &t.grid).chain(
            self.panels
                .values()
                .flat_map(|p| p.tabs.iter().map(|t| &t.grid)),
        )
    }

    fn sleeping_grids_mut(&mut self) -> impl Iterator<Item = &mut grid::Grid> {
        self.panel.tabs.iter_mut().map(|t| &mut t.grid).chain(
            self.panels
                .values_mut()
                .flat_map(|p| p.tabs.iter_mut().map(|t| &mut t.grid)),
        )
    }

    /// 설정 `grid.result_tabs`/`grid.result_tabbar` → 모든 패널. 끄면 활성 탭 외 전부 즉시 해제(D-73 "강제로 메모리 줄이기").
    fn apply_result_tab_opts(&mut self) {
        let enabled = self.settings.flag("grid.result_tabs");
        let always = self.settings.get("grid.result_tabbar") == Some("always");
        self.panel.set_options(enabled, always);
        for p in self.panels.values_mut() {
            p.set_options(enabled, always);
        }
        if !enabled {
            let keep = self.panel.active_id();
            self.panel.tabs.retain(|t| t.id == keep);
            self.panel.active = 0;
            for p in self.panels.values_mut() {
                let keep = p.active_id();
                p.tabs.retain(|t| t.id == keep);
                p.active = 0;
            }
        }
        self.panel.sync_bar();
        self.layout();
        self.redraw();
    }

    /// 결과 탭 전체의 행 바이트 합이 예산(`grid.memory_budget_mb`)을 넘는가(D-72).
    /// 활성 패널의 결과 탭 `i`를 활성으로(그리드 맞바꾸기 · D-71 "그리기는 활성 탭만").
    fn activate_result(&mut self, i: usize) {
        if i >= self.panel.tabs.len() || i == self.panel.active {
            return;
        }
        let a = self.panel.active;
        std::mem::swap(&mut self.grid, &mut self.panel.tabs[a].grid);
        self.panel.active = i;
        std::mem::swap(&mut self.grid, &mut self.panel.tabs[i].grid);
        self.grid_tab = self.panel.tabs[i].id;
        let b = self.panel.tabs[a].grid.bounds;
        self.grid.set_bounds(b);
        self.panel.sync_bar();
        self.set_focus(Focus::Grid);
    }

    /// 새 결과 탭(Ctrl+\ · D-71): 현재 설정을 물려받은 빈 그리드 · 상한을 넘으면 가장 오래된 비고정 탭 정리.
    fn new_result_tab(&mut self) {
        let a = self.panel.active;
        // 실제 그리드를 제자리에 돌려놓고 그 설정을 물려받는다.
        std::mem::swap(&mut self.grid, &mut self.panel.tabs[a].grid);
        let fresh = self.panel.tabs[a].grid.fresh_like();
        let id = self.next_result_id;
        self.next_result_id += 1;
        let tab = ResultTab {
            id,
            title: t(Msg::ResultTabDefault).to_string(),
            pinned: false,
            named: false,
            grid: fresh,
            seq: id,
        };
        self.panel.push(tab);
        let max = self.settings.int("grid.result_tabs_max").clamp(1, 64) as usize;
        if self.panel.tabs.len() > max && self.settings.flag("grid.result_tab_evict") {
            if let Some(v) = self.panel.evict_candidate() {
                let gone = self.panel.remove(v).map(|t| t.title).unwrap_or_default();
                self.status = tf(Msg::StResultTabEvicted, &[&gone]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.status.clone()));
            }
        }
        let i = self.panel.index_of(id).unwrap_or(0);
        self.panel.active = i;
        std::mem::swap(&mut self.grid, &mut self.panel.tabs[i].grid);
        self.grid_tab = id;
        self.panel.sync_bar();
        if self.panel.take_bar_changed() {
            self.layout();
        }
    }

    /// 결과가 도착한 탭의 제목(사용자가 이름 붙이거나 고정한 탭은 그대로).
    fn retitle_result(&mut self, key: u64) {
        let base = match self.grid_for(key) {
            Some(g) => results::title_from_sql(g.source_table().as_deref(), g.source_sql()),
            None => return,
        };
        let cur_editor = self.panel_editor;
        let panel = if self.panel.index_of(key).is_some() {
            Some(&mut self.panel)
        } else {
            self.panels.values_mut().find(|p| p.index_of(key).is_some())
        };
        let _ = cur_editor;
        if let Some(p) = panel {
            if let Some(i) = p.index_of(key) {
                if !p.tabs[i].named && !p.tabs[i].pinned {
                    let title = p.unique_title(&base, i);
                    p.tabs[i].title = title;
                }
            }
            p.sync_bar();
        }
    }

    /// 결과 탭 패널 동작(탭 바 클릭 · 우클릭 메뉴 · 단축키).
    fn panel_action(&mut self, a: ResultAction) {
        let n = self.panel.tabs.len();
        match a {
            ResultAction::Activate(i) => self.activate_result(i),
            ResultAction::Close(i) => self.close_result_tab(i),
            ResultAction::CloseOthers(i) => {
                self.activate_result(i);
                let keep = self.grid_tab;
                let ids: Vec<u64> = self
                    .panel
                    .tabs
                    .iter()
                    .filter(|t| t.id != keep && !t.pinned)
                    .map(|t| t.id)
                    .collect();
                for id in ids {
                    if let Some(j) = self.panel.index_of(id) {
                        self.close_result_tab(j);
                    }
                }
            }
            ResultAction::CloseRight(i) => {
                let ids: Vec<u64> = self
                    .panel
                    .tabs
                    .iter()
                    .skip(i + 1)
                    .filter(|t| !t.pinned)
                    .map(|t| t.id)
                    .collect();
                for id in ids {
                    if let Some(j) = self.panel.index_of(id) {
                        self.close_result_tab(j);
                    }
                }
            }
            ResultAction::TogglePin(i) => {
                if let Some(t) = self.panel.tabs.get_mut(i) {
                    t.pinned = !t.pinned;
                }
            }
            ResultAction::MoveFirst(i) if i < n => self.move_result_tab(i, 0),
            ResultAction::MoveLast(i) if i < n => self.move_result_tab(i, n - 1),
            ResultAction::Move { from, to } if from < n && to < n => self.move_result_tab(from, to),
            _ => {}
        }
        self.panel.sync_bar();
        if self.panel.take_bar_changed() {
            self.layout();
        }
        self.redraw();
    }

    fn move_result_tab(&mut self, from: usize, to: usize) {
        let active_id = self.grid_tab;
        let t = self.panel.tabs.remove(from);
        self.panel.tabs.insert(to, t);
        self.panel.active = self.panel.index_of(active_id).unwrap_or(0);
    }

    /// 결과 탭 닫기 = rows·커서 즉시 해제(D-72). 활성 탭이면 이웃을 활성으로 · 마지막 하나면 빈 탭으로 교체.
    fn close_result_tab(&mut self, i: usize) {
        if i >= self.panel.tabs.len() {
            return;
        }
        if i == self.panel.active {
            // 실제 그리드를 자리에 돌려놓은 뒤 제거 → 이웃 탭의 그리드를 꺼낸다.
            std::mem::swap(&mut self.grid, &mut self.panel.tabs[i].grid);
            let bounds = self.grid.bounds;
            drop(self.panel.remove(i));
            if self.panel.tabs.is_empty() {
                let id = self.next_result_id;
                self.next_result_id += 1;
                let fresh = self.grid.fresh_like();
                self.panel.push(ResultTab {
                    id,
                    title: t(Msg::ResultTabDefault).to_string(),
                    pinned: false,
                    named: false,
                    grid: fresh,
                    seq: id,
                });
                self.panel.active = 0;
            }
            let a = self.panel.active;
            std::mem::swap(&mut self.grid, &mut self.panel.tabs[a].grid);
            self.grid_tab = self.panel.tabs[a].id;
            self.grid.set_bounds(bounds);
        } else {
            drop(self.panel.remove(i));
            self.panel.active = self.panel.index_of(self.grid_tab).unwrap_or(0);
        }
        self.offset_warned.remove(&self.grid_tab);
    }

    /// 추가 페치·전체 조회·건수를 워커에(같은 세션 · docs/43 §3-4 OFFSET 폴백).
    fn send_fetch(&mut self, req: grid::FetchReq) {
        let sql = self.grid.source_sql().to_string();
        if sql.trim().is_empty() {
            self.grid.fetch_failed();
            return;
        }
        let key = self.grid_tab;
        // 메모리 예산(D-72 · 09-17 탭별 독립): **이 탭**의 행이 예산을 넘으면 추가 페치만 거부(전체 조회는 교체라 허용).
        //   다른 탭의 크기는 보지 않는다 — 사용자 09-17 "탭은 서로 영향을 미치지 않아야".
        let budget = (self.settings.int("grid.memory_budget_mb").max(1) as u64) * 1024 * 1024;
        if matches!(req, grid::FetchReq::Next { .. }) && self.grid.approx_bytes() > budget {
            self.grid.fetch_failed();
            self.status = tf(
                Msg::StBudgetExceeded,
                &[&self.settings.int("grid.memory_budget_mb").to_string()],
            );
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.status.clone()));
            self.redraw();
            return;
        }
        match req {
            grid::FetchReq::Next { offset, limit } => {
                if self.settings.flag("grid.offset_warn")
                    && !nsql_io::paging::has_order_by(&sql)
                    && self.offset_warned.insert(key)
                {
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        t(Msg::StOffsetWarn).to_string(),
                    ));
                }
                self.worker.send(worker::Cmd::FetchPage {
                    key,
                    sql,
                    offset,
                    limit,
                    budget_bytes: 0,
                    strict: self.settings.get("grid.refetch_mode") != Some("offset"),
                    strict_all: self.settings.get("grid.refetch_mode") == Some("strict_all"),
                });
            }
            grid::FetchReq::All => {
                // 전체 조회 = **나머지 이어 받기**(09-17 위치 유지): offset = 이미 든 행 수 · 예산 = 이 탭 예산에서 든 만큼을 뺀 나머지
                // (탭별 독립 · 다른 탭을 빼지 않는다 · 이미 넘었으면 1 = 첫 배치 뒤 예산 정지).
                let remain = budget.saturating_sub(self.grid.approx_bytes()).max(1);
                self.worker.send(worker::Cmd::FetchPage {
                    key,
                    sql,
                    offset: self.grid.row_count(),
                    limit: 0,
                    budget_bytes: remain,
                    strict: self.settings.get("grid.refetch_mode") != Some("offset"),
                    strict_all: self.settings.get("grid.refetch_mode") == Some("strict_all"),
                });
            }
            grid::FetchReq::Count => self.worker.send(worker::Cmd::Count { key, sql }),
        }
        self.status = t(Msg::StFetching).into();
        self.redraw();
    }

    /// 새로고침 — 같은 문장을 이 탭의 세그먼트 크기로 다시 실행.
    fn refresh_result(&mut self) {
        let src = self.grid.source_sql().to_string();
        if src.trim().is_empty() || self.busy {
            return;
        }
        self.run_tab = self.grid_tab;
        self.busy = true;
        self.status = t(Msg::StRunning).into();
        self.run_toast_start(&src);
        self.last_run_items = split_items(&src);
        let max_rows = self.grid.page_rows();
        self.worker.send(worker::Cmd::Run {
            src,
            preflight: None,
            max_rows,
        });
        self.live_start();
        self.redraw();
    }

    /// SQL 보기(결과 도구줄) — 키 규칙은 Copy SQL과 같다(docs/41 · 캐시 → 워커 1회).
    fn begin_view_sql(&mut self, kind: nsql_io::SqlKind) {
        if self.key_mode() == nsql_io::KeyMode::All {
            self.finish_view_sql(kind, None);
            return;
        }
        let Some(guess) = self.grid.source_table() else {
            self.finish_view_sql(kind, None);
            return;
        };
        if let Some(info) = self.key_cache.get(&guess) {
            let info = info.clone();
            self.finish_view_sql(kind, info.as_ref());
            return;
        }
        let (schema, table) = nsql_io::split_table(self.dialect, &guess);
        self.view_wait = Some(kind);
        self.worker.send(worker::Cmd::Keys {
            key: guess,
            schema,
            table,
        });
    }

    fn finish_view_sql(&mut self, kind: nsql_io::SqlKind, info: Option<&nsql_core::KeyInfo>) {
        let names = self.grid.all_col_names();
        let key = nsql_io::choose_key(self.key_mode(), info, &names);
        if key.needs_warning() && kind != nsql_io::SqlKind::Insert {
            let table = self.grid.source_table();
            let w = tf(
                Msg::SqlKeyWarnFirstN,
                &[
                    table.as_deref().unwrap_or("T"),
                    &key.cols.len().to_string(),
                    &key.cols.join(", "),
                ],
            );
            self.status = w.clone();
            self.log_win.push(LogEntry::new(LogKind::Error, w));
        }
        self.grid.finish_view_sql(kind, &key);
        self.redraw();
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
            "view.txlog" | "tx.log" => self.open_txlog_window(el),
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
            "file.exit" => self.request_exit(),
            // ★ 파일 열기/저장(T-74) — 자체 대화상자(nexa-dlg) · 네이티브 0.
            "file.open" => self.open_file_dlg = Some(PickerMode::Open),
            "file.save" => match self.editors.active_path() {
                Some(p) => self.save_to(&p),
                None => self.open_file_dlg = Some(PickerMode::Save),
            },
            "file.save_as" => self.open_file_dlg = Some(PickerMode::Save),
            id if id.starts_with("tab:") => {
                if let Ok(tid) = id["tab:".len()..].parse::<u64>() {
                    self.editors.switch_to_id(tid);
                    self.set_focus(Focus::Editor);
                    self.layout();
                }
            }
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
                if id == "edit.replace" && self.find.is_visible() && self.focus == Focus::Find {
                    // 열린 채 Ctrl+H = 바꾸기 줄 펼침/접기(VS Code).
                    self.find.toggle_replace();
                    self.layout();
                    self.redraw();
                } else {
                    let seed = self.editors.cur().copy_selection();
                    self.find.open(id == "edit.replace", seed);
                    self.find
                        .set_tooltip_delay(self.settings.int("ui.tooltip_delay_ms").max(0) as u128);
                    self.layout();
                    self.set_focus(Focus::Find);
                    self.find_step(true, false);
                }
            }
            // ★ Sublime Ctrl+D — 캐럿 밑 단어 → 다음 출현을 추가 선택(다중 커서 · 09-15).
            "edit.goto_bracket" => {
                if self.editors.cur_mut().goto_bracket(false) {
                    self.redraw();
                }
            }
            "edit.bracket_prev"
            | "edit.bracket_next"
            | "edit.bracket_parent"
            | "edit.bracket_child" => self.run_extension_cmd(id),
            "ext.enable_mgr" | "ext.install" | "ext.remove" | "ext.list" | "ext.enable"
            | "ext.disable" | "ext.repo_add" | "ext.repo_list" | "ext.repo_remove" => {
                self.ext_command(id)
            }
            x if x.starts_with("ext.") => self.ext_pick(x),
            "edit.expand_brackets" => {
                if self.editors.cur_mut().expand_to_brackets() {
                    self.redraw();
                }
            }
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
            // ★ Sublime 줄·선택 편집(T-98 · 09-16) — 편집기에 포커스일 때만.
            "edit.duplicate_line" => self.editor_cmd(EditCommand::DuplicateLines),
            "edit.delete_line" => self.editor_cmd(EditCommand::DeleteLines),
            "edit.join_lines" => self.editor_cmd(EditCommand::JoinLines),
            "edit.swap_line_up" => self.editor_cmd(EditCommand::SwapLinesUp),
            "edit.swap_line_down" => self.editor_cmd(EditCommand::SwapLinesDown),
            "edit.toggle_comment" => self.editor_cmd(EditCommand::ToggleComment),
            "edit.indent" => self.editor_cmd(EditCommand::Indent),
            "edit.unindent" => self.editor_cmd(EditCommand::Unindent),
            "edit.select_line" => self.editor_cmd(EditCommand::SelectLines),
            "edit.split_lines" => self.editor_cmd(EditCommand::SplitIntoLines),
            "edit.add_caret_up" => self.editor_cmd(EditCommand::AddCaretUp),
            "edit.add_caret_down" => self.editor_cmd(EditCommand::AddCaretDown),
            "edit.upper_case" => self.editor_cmd(EditCommand::UpperCase),
            "edit.lower_case" => self.editor_cmd(EditCommand::LowerCase),
            // Goto Anything(T-96): 탭 · 최근 파일 · `:줄`.
            "view.goto_anything" | "tab.find" => self.open_goto_anything(""),
            "edit.goto_line" => self.open_goto_anything(":"),
            id if id.starts_with("goto.line:") => {
                if let Ok(n) = id["goto.line:".len()..].parse::<usize>() {
                    self.ed_mut().goto_line(n);
                    self.set_focus(Focus::Editor);
                }
            }
            // 줄끝 변환 메뉴(T-89 잔여) — 상태줄 팝업과 같은 경로.
            id if id.starts_with("eol.") => self.indent_pick(id),
            "edit.find_next" => self.find_step(true, true),
            "edit.find_prev" => self.find_step(false, true),
            "edit.next_statement" => self.goto_statement(true),
            "edit.prev_statement" => self.goto_statement(false),
            "edit.undo" => self.route(InputEvent::Undo),
            "edit.redo" => self.route(InputEvent::Redo),
            "view.log" => self.toggle_log = true,
            "view.toolbar_reset" => self.reset_toolbar(),
            "view.colors" => self.open_colors = true,
            "view.keys" => self.open_keys = true,
            "view.search" => {
                let on = !self.search.is_visible();
                if on && self.explorer.is_visible() {
                    self.explorer.set_visible(false);
                    let _ = self.settings.set("explorer.visible", "off");
                    let _ = self.settings.save();
                }
                self.search.set_visible(on);
                if on {
                    let seed = self.editors.cur().copy_selection();
                    self.search.focus_query(seed);
                    self.search
                        .set_tooltip_delay(self.settings.int("ui.tooltip_delay_ms").max(0) as u128);
                    self.set_focus(Focus::Search);
                } else if self.focus == Focus::Search {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
            }
            "view.explorer" => {
                let on = !self.explorer.is_visible();
                if on && self.search.is_visible() {
                    self.search.set_visible(false);
                    if self.focus == Focus::Search {
                        self.set_focus(Focus::Editor);
                    }
                }
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
                self.close_tab_guarded(i);
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
            // Ctrl+\ = 새 결과 탭에 실행(T-93 · 끄면 Ctrl+Enter와 같다 · D-73).
            "run.statement_new_tab" => {
                if self.settings.flag("grid.result_tabs") && !self.busy {
                    self.new_result_tab();
                }
                self.run_sql(false);
            }
            "result.tab.close" => {
                let i = self.panel.active;
                self.panel_action(ResultAction::Close(i));
            }
            "result.tab.next" | "result.tab.prev" => {
                let n = self.panel.tabs.len();
                if n > 1 {
                    let i = self.panel.active;
                    let j = if id == "result.tab.next" {
                        (i + 1) % n
                    } else {
                        (i + n - 1) % n
                    };
                    self.panel_action(ResultAction::Activate(j));
                }
            }
            "run.all" => self.run_sql(true),
            "run.stop" => self.stop_run(),
            "run.explain" => self.run_explain(),
            "run.commit" => self.worker.send(worker::Cmd::Commit),
            "run.rollback" => self.worker.send(worker::Cmd::Rollback),
            // 트랜잭션 로그 창(T-107)이 오기 전까지는 상태줄 트랜잭션 팝업(모드 전환 · Commit(n) · Rollback(n) · 대기 목록).
            "tx.log" | "view.txlog" => self.open_txlog = true,
            // 접속 창 열기 — 연결 중이어도 끊지 않고 그냥 연다(사용자 09-14). 끊기는 폼의 Disconnect 버튼.
            "conn.toggle" => self.open_conn = true,
            "conn.disconnect" => self.disconnect_now(),
            "edit.prefs" => self.open_prefs = true,
            "edit.settings_json" => self.edit_settings_json(),
            "help.demo" => self.start_demo_create(),
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

    // ───────────────────────── 파일 검색(T-81a · docs/36) ─────────────────────────

    /// 검색 시작 — 열린 탭 본문 · 활성 파일 폴더 · 설정을 모아 패널에 넘긴다.
    fn start_search(&mut self) {
        let excludes: Vec<String> = self
            .settings
            .get("search.excludes")
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        let ctx = SearchCtx {
            tabs: self.editors.tab_texts(),
            current_dir: self
                .editors
                .active_path()
                .and_then(|p| p.parent().map(Path::to_path_buf)),
            max_file_kb: self.settings.int("search.max_file_kb").max(0) as usize,
            threads: self.settings.int("search.threads").clamp(0, 16) as usize,
            gitignore: self.settings.flag("search.gitignore"),
            excludes,
        };
        self.search.start(ctx);
        self.redraw();
    }

    /// 결과 행 → 그 탭/파일을 열고 줄로 이동 · 일치 구간 선택.
    fn open_search_result(&mut self, req: search_panel::OpenReq) {
        match (req.tab, &req.path) {
            (Some(id), _) => self.editors.switch_to_id(id),
            (None, Some(p)) => self.open_file(p),
            (None, None) => return,
        }
        let start = {
            let ed = self.ed_mut();
            ed.goto_line(req.line);
            ed.caret() + req.col
        };
        let mut inv = Invalidations::default();
        self.ed_mut().select_range(start, start + req.len, &mut inv);
        self.set_focus(Focus::Editor);
        self.layout();
        self.redraw();
    }

    // ───────────────────────── 트랜잭션 UX(DR-30 · T-77 · docs/34) ─────────────────────────

    /// DML/DDL 완료 → 대기 목록 갱신. 수동 모드의 DML(영향 행 > 0) = 대기 +1 · 방언별 암묵 커밋 DDL = 비움 ·
    /// 자동 모드 + `tx.smart_commit` = 첫 DML 뒤 수동으로 전환.
    fn tx_on_done(&mut self, index: usize, stmt: &str, rows_affected: Option<u64>) {
        let auto = self.settings.flag("session.autocommit");
        if implicit_commit(self.dialect, stmt) {
            if !self.tx_pending.is_empty() {
                let w = first_word(stmt);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, tf(Msg::StTxImplicit, &[&w])));
                self.tx_close(TxOutcome::ImplicitCommit(w));
            }
            return;
        }
        let class = nsql_core::TxClass::of_sql(stmt);
        // 대기 대상: 영향 행 > 0인 DML · 트랜잭션 DDL 방언(PG·SQL Server·SQLite)의 DDL/TRUNCATE(docs/44 §5).
        let ddl_pending = class.is_ddl() && self.dialect.ddl_transactional();
        if !ddl_pending && !rows_affected.is_some_and(|n| n > 0) {
            return;
        }
        if auto {
            if self.settings.flag("tx.smart_commit") {
                self.set_autocommit_now(false);
                self.status = t(Msg::StTxSmartSwitched).into();
            }
            return;
        }
        let stamp = nsql_log::now_local().stamp();
        let when = stamp.get(11..16).unwrap_or("").to_string();
        self.txlog.attach_tx(index, stamp, true);
        self.txlog_win.redraw();
        self.tx_pending.push(TxItem {
            editor: self.run_editor,
            at: Instant::now(),
            when,
            summary: one_line(stmt, 60),
            class,
        });
        self.tx_dirty = true;
        self.tx_stale_logged = false;
        self.sync_tx_ui();
    }

    /// 대기 목록 비우기(커밋 · 롤백 · 해제 · 암묵 커밋).
    fn tx_clear(&mut self) {
        self.tx_pending.clear();
        self.tx_read = false;
        self.tx_dirty = false;
        self.tx_stale_logged = false;
        self.sync_tx_ui();
    }

    /// 수동 모드의 조회 — 읽기 트랜잭션 표시(배지 없음 · 초록 · docs/44 §5).
    fn tx_on_read(&mut self, index: usize) {
        if self.settings.flag("session.autocommit") {
            return;
        }
        // 수동 모드의 조회는 열린 트랜잭션에 속한다(롤백/커밋 시점 표시 · docs/44 §3).
        self.txlog
            .attach_tx(index, nsql_log::now_local().stamp(), false);
        self.txlog_win.redraw();
        if self.tx_read {
            return;
        }
        self.tx_read = true;
        self.sync_tx_ui();
    }

    /// 트랜잭션 버튼 색·배지·툴팁(docs/44 §5): 자동 커밋 = 기본색·배지 없음 · 수동 = 가장 심각한 문장 종류의 색 + 대기 수.
    fn sync_tx_button(&mut self) {
        use nsql_core::TxClass;
        let auto = self.settings.flag("session.autocommit");
        let mut inv = Invalidations::default();
        let n = self.tx_pending.len();
        let top = self
            .tx_pending
            .iter()
            .map(|i| i.class)
            .max_by_key(|c| c.severity())
            .unwrap_or(if self.tx_read {
                TxClass::Read
            } else {
                TxClass::None
            });
        let stale = self.tx_is_stale();
        let tone = if auto || top == TxClass::None {
            ToolTone::Default
        } else if stale {
            ToolTone::Danger
        } else {
            match top {
                TxClass::Read => ToolTone::Ok,
                TxClass::DdlCreate => ToolTone::Custom(nexa_ctl::Color(0x8E5BD6FF)),
                TxClass::Insert | TxClass::Other => ToolTone::Accent,
                TxClass::Update => ToolTone::Custom(nexa_ctl::Color(0xE0A020FF)),
                TxClass::Delete | TxClass::Truncate | TxClass::DdlDrop => ToolTone::Danger,
                TxClass::None => ToolTone::Default,
            }
        };
        let badge = if auto || n == 0 {
            None
        } else if stale {
            Some(format!("!{n}"))
        } else {
            Some(n.to_string())
        };
        let tip = if auto {
            t(Msg::TipTxLogAuto).to_string()
        } else if n == 0 && !self.tx_read {
            t(Msg::TipTxLog).to_string()
        } else {
            let mut kinds: Vec<Msg> = Vec::new();
            let mut seen: Vec<TxClass> = self.tx_pending.iter().map(|i| i.class).collect();
            if self.tx_read && seen.is_empty() {
                seen.push(TxClass::Read);
            }
            seen.sort_by_key(|c| std::cmp::Reverse(c.severity()));
            seen.dedup();
            for c in seen {
                kinds.push(match c {
                    TxClass::Read => Msg::TxClsRead,
                    TxClass::Insert => Msg::TxClsInsert,
                    TxClass::Update => Msg::TxClsUpdate,
                    TxClass::Delete => Msg::TxClsDelete,
                    TxClass::Truncate => Msg::TxClsTruncate,
                    TxClass::DdlCreate => Msg::TxClsDdlCreate,
                    TxClass::DdlDrop => Msg::TxClsDdlDrop,
                    TxClass::Other | TxClass::None => Msg::TxClsOther,
                });
            }
            let list = kinds.iter().map(|m| t(*m)).collect::<Vec<_>>().join(" · ");
            let since = self
                .tx_pending
                .first()
                .map_or(String::new(), |f| f.when.clone());
            tf(Msg::TipTxState, &[&n.to_string(), &since, &list])
        };
        self.tool_dock.set_item_tone("tx.log", tone, &mut inv);
        self.tool_dock
            .set_item_badge("tx.log", badge.as_deref(), &mut inv);
        if let Some(gid) = self.tool_dock.group_of("tx.log").map(str::to_string) {
            if let Some(bar) = self.tool_dock.bar_mut(&gid) {
                bar.set_item_tip("tx.log", &tip);
            }
        }
        for f in &self.tool_floats {
            f.redraw();
        }
    }

    /// 탭 배지 · 툴바 Commit/Rollback 활성·색 — 세 층이 같은 사실을 말한다.
    fn sync_tx_ui(&mut self) {
        self.sync_tx_button();
        let stale = self.tx_is_stale();
        let mut map: HashMap<u64, (usize, bool)> = HashMap::new();
        for it in &self.tx_pending {
            let e = map.entry(it.editor).or_insert((0, stale));
            e.0 += 1;
            e.1 = stale;
        }
        let mode = self.settings.get("tx.badge").unwrap_or("count").to_string();
        self.editors.set_tx_badges(map, &mode);
        let has = !self.tx_pending.is_empty();
        let mut inv = Invalidations::default();
        for id in ["run.commit", "run.rollback"] {
            self.tool_dock.set_item_enabled(id, has, &mut inv);
            self.tool_dock.set_item_tone(
                id,
                if !has {
                    ToolTone::Default
                } else if stale {
                    ToolTone::Danger
                } else {
                    ToolTone::Accent
                },
                &mut inv,
            );
        }
        self.redraw();
    }

    fn tx_is_stale(&self) -> bool {
        let min = self.settings.int("tx.stale_min").max(1) as u64;
        self.tx_pending
            .first()
            .is_some_and(|f| f.at.elapsed().as_secs() >= min * 60)
    }

    /// 오래된 미커밋 감시(about_to_wait · 1회 로그 + 배지 ⚠).
    fn tx_tick(&mut self) {
        if self.tx_pending.is_empty() || self.tx_stale_logged || !self.tx_is_stale() {
            return;
        }
        self.tx_stale_logged = true;
        let min = self.settings.int("tx.stale_min").max(1).to_string();
        self.log_win
            .push(LogEntry::new(LogKind::Error, tf(Msg::StTxStale, &[&min])));
        self.status = tf(Msg::StTxStale, &[&min]);
        self.sync_tx_ui();
    }

    /// 상태줄 트랜잭션 팝업: 모드 전환 · Commit(n) · Rollback(n) · 대기 문장 목록.
    fn open_tx_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let auto = self.settings.flag("session.autocommit");
        let n = self.tx_pending.len();
        let mark = |on: bool, s: &str| {
            if on {
                format!("✓ {s}")
            } else {
                format!("   {s}")
            }
        };
        let mut items = vec![
            CtxItem::item("tx.auto", mark(auto, t(Msg::MnTxAuto))).with_active(auto),
            CtxItem::item("tx.manual", mark(!auto, t(Msg::MnTxManual))).with_active(!auto),
            CtxItem::Separator,
            CtxItem::item("tx.commit", tf(Msg::MnTxCommitN, &[&n.to_string()]))
                .with_shortcut(self.keymap.display_of("run.commit")),
            CtxItem::item("tx.rollback", tf(Msg::MnTxRollbackN, &[&n.to_string()]))
                .with_shortcut(self.keymap.display_of("run.rollback")),
        ];
        if n > 0 {
            items.push(CtxItem::Separator);
            for it in self.tx_pending.iter().take(12) {
                items.push(CtxItem::item(
                    "tx.noop",
                    format!("{}  {}", it.when, it.summary),
                ));
            }
        }
        self.open_status_popup(self.status_tx_rect, items);
    }

    /// 잃는 순간의 확인 팝업(Commit / Rollback / Cancel) — 상태줄 트랜잭션 세그먼트 자리(없으면 창 가운데).
    fn open_tx_guard(&mut self, commit: Msg, rollback: Msg) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let n = self.tx_pending.len().to_string();
        self.status = tf(Msg::StTxGuard, &[&n]);
        let items = vec![
            CtxItem::item("tx.commit_then", t(commit)),
            CtxItem::item("tx.rollback_then", t(rollback)),
            CtxItem::Separator,
            CtxItem::item("tx.cancel", t(Msg::MnTxCancel)),
        ];
        let mut r = self.status_tx_rect;
        if r.w == 0 {
            if let Some(w) = &self.window {
                let sz = w.inner_size();
                r = Rect::new(
                    sz.width as i32 / 2 - px(120.0, self.scale),
                    sz.height as i32 / 2,
                    0,
                    0,
                );
            }
        }
        self.open_status_popup(r, items);
    }

    fn open_status_popup(&mut self, r: Rect, items: Vec<nexa_ctl::controls::ctxmenu::CtxItem>) {
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
            .open_at(r.x, r.y, items, host, px(300.0, self.scale));
    }

    /// 팝업 항목(`tx.*`).
    fn tx_pick(&mut self, id: &str) {
        match id {
            "auto" => self.set_autocommit(true),
            "manual" => self.set_autocommit(false),
            "commit" => self.worker.send(worker::Cmd::Commit),
            "rollback" => self.worker.send(worker::Cmd::Rollback),
            "commit_then" => {
                self.worker.send(worker::Cmd::Commit);
                self.tx_close(TxOutcome::Committed);
                self.run_tx_after();
            }
            "rollback_then" => {
                self.worker.send(worker::Cmd::Rollback);
                self.tx_close(TxOutcome::RolledBack);
                self.run_tx_after();
            }
            "cancel" => self.tx_after = None,
            _ => {}
        }
    }

    fn run_tx_after(&mut self) {
        match self.tx_after.take() {
            Some(TxAfter::CloseTab(i)) => {
                self.editors.close_tab_confirmed(i);
                self.set_focus(Focus::Editor);
            }
            Some(TxAfter::Disconnect) => self.disconnect_force(),
            Some(TxAfter::Exit) => self.exit_requested = true,
            Some(TxAfter::SwitchAuto) => self.set_autocommit_now(true),
            None => {}
        }
    }

    /// 모드 전환 — 수동 → 자동인데 대기 문장이 있으면 먼저 묻는다.
    fn set_autocommit(&mut self, on: bool) {
        if on && !self.tx_pending.is_empty() {
            self.tx_after = Some(TxAfter::SwitchAuto);
            self.open_tx_guard(Msg::MnTxCommitSwitch, Msg::MnTxRollbackSwitch);
            return;
        }
        self.set_autocommit_now(on);
    }

    /// 설정 + 살아 있는 세션(스크립트 `SET AUTOCOMMIT` · 워커 큐 순서 보장).
    fn set_autocommit_now(&mut self, on: bool) {
        let _ = self
            .settings
            .set("session.autocommit", if on { "on" } else { "off" });
        self.persist_settings();
        if !self.busy {
            let src = if on {
                "SET AUTOCOMMIT ON"
            } else {
                "SET AUTOCOMMIT OFF"
            };
            self.last_run_items = split_items(src);
            self.busy = true;
            self.worker.send(worker::Cmd::Run {
                src: src.to_string(),
                preflight: None,
                max_rows: self.grid.page_rows(),
            });
        }
        if on {
            self.tx_close(TxOutcome::Switched);
        }
        self.sync_tx_ui();
    }

    /// 미커밋 탭 닫기(설정 `tx.close_action`: ask / commit / rollback).
    /// 탭 우클릭 메뉴(09-17): 이름 바꾸기(팔레트 프롬프트) · 닫기 계열(높은 index부터 · 미커밋/미저장 가드는 탭마다) · 파일 위치 열기.
    fn tab_menu_request(&mut self, req: editors::TabMenuReq) {
        use editors::TabMenuReq;
        let n = self.editors.tab_count();
        match req {
            TabMenuReq::Rename(i) => {
                let title = self.editors.title_of(i);
                self.palette
                    .open_prompt(&format!("tab.rename:{i}"), t(Msg::PhTabRename), &title);
            }
            TabMenuReq::Close(i) => self.close_tab_guarded(i),
            TabMenuReq::CloseLeft(i) => {
                for j in (0..i.min(n)).rev() {
                    self.close_tab_guarded(j);
                }
            }
            TabMenuReq::CloseRight(i) => {
                for j in ((i + 1)..n).rev() {
                    self.close_tab_guarded(j);
                }
            }
            TabMenuReq::CloseAll => {
                for j in (0..n).rev() {
                    self.close_tab_guarded(j);
                }
            }
            TabMenuReq::Reveal(i) => {
                if let Some(path) = self.editors.path_of(i) {
                    if let Err(e) = nexa_fs::shell::reveal_in_file_manager(&path) {
                        self.status = tf(Msg::StRevealFailed, &[&e.to_string()]);
                    }
                }
            }
        }
    }

    fn close_tab_guarded(&mut self, i: usize) {
        let id = self.editors.tab_id(i);
        let n = self.tx_pending.iter().filter(|t| t.editor == id).count();
        if n == 0 {
            self.editors.close_tab_confirmed(i);
            return;
        }
        match self.settings.get("tx.close_action").unwrap_or("ask") {
            "commit" => {
                self.worker.send(worker::Cmd::Commit);
                self.tx_close(TxOutcome::Committed);
                self.editors.close_tab_confirmed(i);
            }
            "rollback" => {
                self.worker.send(worker::Cmd::Rollback);
                self.tx_close(TxOutcome::RolledBack);
                self.editors.close_tab_confirmed(i);
            }
            _ => {
                self.tx_after = Some(TxAfter::CloseTab(i));
                self.open_tx_guard(Msg::MnTxCommitClose, Msg::MnTxRollbackClose);
            }
        }
    }

    /// 종료 — 미커밋이 있으면 묻는다.
    fn request_exit(&mut self) {
        if self.tx_pending.is_empty() {
            self.exit_requested = true;
            return;
        }
        self.tx_after = Some(TxAfter::Exit);
        self.open_tx_guard(Msg::MnTxCommitExit, Msg::MnTxRollbackExit);
    }

    /// 편집기 편집 명령 — 편집기 포커스일 때만 · 바뀌면 찾기 표시 갱신 + 상태줄 선택 수.
    fn editor_cmd(&mut self, cmd: EditCommand) {
        if self.focus != Focus::Editor {
            return;
        }
        if self.ed_mut().edit_command(cmd) {
            let n = self.editors.selection_count();
            if n > 1 {
                self.status = tf(Msg::StSelections, &[&n.to_string()]);
            }
        }
    }

    /// Goto Anything(T-96 · Sublime Ctrl+P): 열린 탭(제목 · 경로 · `*`) + 최근 파일 · `:숫자` = 줄 이동.
    fn open_goto_anything(&mut self, prefill: &str) {
        let mut cmds: Vec<(String, String)> = Vec::new();
        for (id, title, path, dirty, active) in self.editors.tab_entries() {
            let mark = if dirty { "*" } else { "" };
            let where_ = path
                .as_deref()
                .map(nexa_fs::path::display)
                .unwrap_or_else(|| t(Msg::PalUntitled).to_string());
            let act = if active { "✓ " } else { "" };
            cmds.push((
                format!("tab:{id}"),
                format!("{act}{title}{mark}  —  {where_}"),
            ));
        }
        for (i, p) in self.recent_files().iter().enumerate() {
            let name = p
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            cmds.push((
                format!("file.recent:{i}"),
                format!(
                    "{}: {name}  {}",
                    t(Msg::LblFileRecent),
                    nexa_fs::path::display(p)
                ),
            ));
        }
        self.palette.set_commands(cmds);
        self.palette.open(prefill);
        self.redraw();
    }

    /// 찾기 패널이 열려 있으면 일치 구간 전부를 편집기에 표시(반투명 · T-73) · 닫혀 있으면 지운다.
    fn sync_find_marks(&mut self) {
        let marks = if self.find.is_visible() {
            self.find_matches().0
        } else {
            Vec::new()
        };
        self.ed_mut().set_find_marks(marks);
    }

    fn build_menus() -> Vec<MenuDef> {
        Self::build_menus_with(&[], &[], false)
    }

    /// 메뉴 정의 — File 메뉴 아래쪽에 최근 파일(최대 8 · Eclipse/DBeaver 관례).
    fn build_menus_with(
        recent: &[PathBuf],
        tabs: &[(u64, String, bool)],
        demo_ready: bool,
    ) -> Vec<MenuDef> {
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
                    item("edit.goto_bracket", Msg::MnGotoBracket),
                    item("edit.expand_brackets", Msg::MnExpandBrackets),
                    item("edit.bracket_prev", Msg::MnBracketPrev),
                    item("edit.bracket_next", Msg::MnBracketNext),
                    item("edit.bracket_parent", Msg::MnBracketParent),
                    item("edit.bracket_child", Msg::MnBracketChild),
                    item("edit.select_all_occurrences", Msg::MnSelectAllOccurrences),
                    item("edit.select_line", Msg::MnSelectLine),
                    item("edit.split_lines", Msg::MnSplitLines),
                    item("edit.add_caret_up", Msg::MnAddCaretUp),
                    item("edit.add_caret_down", Msg::MnAddCaretDown),
                    MenuEntry::Separator,
                    item("edit.duplicate_line", Msg::MnDuplicateLine),
                    item("edit.delete_line", Msg::MnDeleteLine),
                    item("edit.join_lines", Msg::MnJoinLines),
                    item("edit.swap_line_up", Msg::MnSwapLineUp),
                    item("edit.swap_line_down", Msg::MnSwapLineDown),
                    MenuEntry::Separator,
                    item("edit.toggle_comment", Msg::MnToggleComment),
                    item("edit.indent", Msg::MnIndent),
                    item("edit.unindent", Msg::MnUnindent),
                    item("edit.upper_case", Msg::MnUpperCase),
                    item("edit.lower_case", Msg::MnLowerCase),
                    MenuEntry::Separator,
                    item("edit.goto_line", Msg::MnGotoLine),
                    MenuEntry::Separator,
                    item("eol.crlf", Msg::MnEolCrlf),
                    item("eol.lf", Msg::MnEolLf),
                    item("eol.cr", Msg::MnEolCr),
                    MenuEntry::Separator,
                    item("edit.prefs", Msg::MnPreferences),
                ],
            ),
            MenuDef::new(
                t(Msg::MnView),
                vec![
                    item("view.palette", Msg::MnCommandPalette),
                    item("view.goto_anything", Msg::MnGotoAnything),
                    item("view.explorer", Msg::MnExplorer),
                    item("view.search", Msg::MnSearchPanel),
                    item("view.log", Msg::MnLogWindow),
                    item("view.txlog", Msg::MnTxLogWindow),
                    item("view.on_top", Msg::MnAlwaysOnTop),
                    item("view.toolbar_reset", Msg::MnResetToolbar),
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
                    item("run.statement_new_tab", Msg::MnRunStatementNewTab),
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
            // ★ 탭 메뉴(Golden Tabs · 사용자 09-16): 열린 탭 순서대로 · 활성 탭은 ✓ · 고르면 전환.
            MenuDef::new(t(Msg::MnTabs), {
                let mut v: Vec<MenuEntry> = tabs
                    .iter()
                    .map(|(id, title, active)| {
                        let mut ci = ComboItem::new(format!("tab:{id}"), title.clone());
                        if *active {
                            ci.icon = Some("✓".into());
                        }
                        MenuEntry::Item(ci)
                    })
                    .collect();
                // 탭 찾기(Goto Anything · T-96 추천안 A).
                v.push(MenuEntry::Separator);
                v.push(item("tab.find", Msg::MnFindTab));
                v
            }),
            MenuDef::new(
                t(Msg::MnHelp),
                vec![
                    // 샘플 데이터(사용자 09-17): 이미 있으면 비활성.
                    if demo_ready {
                        MenuEntry::Disabled(ComboItem::new("help.demo", t(Msg::MnDemoCreate)))
                    } else {
                        item("help.demo", Msg::MnDemoCreate)
                    },
                    MenuEntry::Separator,
                    item("help.about", Msg::MnAbout),
                ],
            ),
        ]
    }

    /// 탭 목록이 바뀌었으면(열기·닫기·이름·활성) 메뉴를 다시 만든다 — 드롭다운이 열려 있는 동안은 미룬다.
    fn sync_tabs_menu(&mut self) {
        if self.menubar.is_open() {
            return;
        }
        let tabs = self.editors.tab_list();
        let sig = tabs
            .iter()
            .map(|(id, t, a)| format!("{id}:{t}:{a}"))
            .collect::<Vec<_>>()
            .join("|");
        if sig != self.tabs_menu_sig {
            self.tabs_menu_sig = sig;
            let recent = self.recent_files();
            self.menubar
                .set_menus(App::build_menus_with(&recent, &tabs, self.demo_ready));
        }
    }

    /// 툴바 = 목적별 그룹(파일 · 실행 · 접속 · 보기) — 아이콘·구분자 계층(nexa-ctl `ToolGroup`). 순서·플로팅은 도크 배치가 기억.
    fn build_tool_dock() -> ToolDock {
        // 아이콘은 글꼴 글리프가 아니라 코드로 그린 마스크(`toolicons.rs` · 사용자 09-14).
        let file = ToolGroup::new(
            "file",
            t(Msg::MnFile),
            vec![
                ToolItem::new("file.new", toolicons::new_script()).tip(t(Msg::TipNew)),
                ToolItem::new("file.open", toolicons::open_file()).tip(t(Msg::TipOpen)),
                ToolItem::separator(),
                ToolItem::new("file.save", toolicons::save_file()).tip(t(Msg::TipSave)),
                // 다른 이름으로 저장 — 사용자가 Material `save_as` 아이콘을 준 09-16(명령 id는 메뉴와 동일).
                ToolItem::new("file.save_as", toolicons::save_as()).tip(t(Msg::TipSaveAs)),
            ],
        );
        let run = ToolGroup::new(
            "run",
            t(Msg::MnRun),
            vec![
                ToolItem::new("run.statement", toolicons::run_statement())
                    .tip(t(Msg::TipRunStatement)),
                ToolItem::new("run.all", toolicons::run_all()).tip(t(Msg::TipRunAll)),
                ToolItem::new("run.stop", toolicons::fetch_stop()).tip(t(Msg::TipRunStop)),
                ToolItem::separator(),
                // 트랜잭션(DR-30): 대기 문장이 있을 때만 활성 · Material check/undo.
                ToolItem::new("run.commit", toolicons::commit())
                    .tip(t(Msg::TipCommit))
                    .disabled(),
                ToolItem::new("run.rollback", toolicons::rollback())
                    .tip(t(Msg::TipRollback))
                    .disabled(),
                // 트랜잭션 로그(docs/44 §5): 항상 활성 · 수동 모드면 배지 = 대기 수 · 색 = 가장 심각한 문장 종류.
                ToolItem::new("tx.log", toolicons::tx_log()).tip(t(Msg::TipTxLog)),
            ],
        );
        let conn = ToolGroup::new(
            "conn",
            t(Msg::TbGroupConn),
            vec![
                ToolItem::new("conn.toggle", toolicons::connect()).tip(t(Msg::TipConnect)),
                ToolItem::new("conn.disconnect", toolicons::disconnect())
                    .tip(t(Msg::TipDisconnect))
                    .disabled(),
            ],
        );
        let view = ToolGroup::new(
            "view",
            t(Msg::MnView),
            vec![ToolItem::new("view.log", toolicons::log()).tip(t(Msg::TipLog))],
        )
        .align_right();
        let mut dock = ToolDock::new(vec![file, run, conn, view]);
        dock.set_icon_size(18);
        dock
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
                            let px = self.settings.font_px("editor.font_size").round() as i32;
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
            // 플러그인 메뉴 기여(우클릭 서브메뉴 항목) → 명령 id 그대로 메뉴 경로로(09-17 · extensions).
            EditCtxAction::Custom(id) => self.menu_action(&id),
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
        cmds.push(m("edit.goto_bracket", Msg::MnEdit, Msg::MnGotoBracket));
        cmds.push(m("edit.bracket_prev", Msg::MnEdit, Msg::MnBracketPrev));
        cmds.push(m("edit.bracket_next", Msg::MnEdit, Msg::MnBracketNext));
        cmds.push(m("edit.bracket_parent", Msg::MnEdit, Msg::MnBracketParent));
        cmds.push(m("edit.bracket_child", Msg::MnEdit, Msg::MnBracketChild));
        // Extension Manager(Sublime "Package Control: …" 표기 · docs/50 §10).
        cmds.push(m(
            "ext.enable_mgr",
            Msg::MnExtensions,
            Msg::MnExtEnableManager,
        ));
        cmds.push(m("ext.install", Msg::MnExtensions, Msg::MnExtInstall));
        cmds.push(m("ext.remove", Msg::MnExtensions, Msg::MnExtRemove));
        cmds.push(m("ext.list", Msg::MnExtensions, Msg::MnExtList));
        cmds.push(m("ext.enable", Msg::MnExtensions, Msg::MnExtEnable));
        cmds.push(m("ext.disable", Msg::MnExtensions, Msg::MnExtDisable));
        cmds.push(m("ext.repo_add", Msg::MnExtensions, Msg::MnExtRepoAdd));
        cmds.push(m("ext.repo_list", Msg::MnExtensions, Msg::MnExtRepoList));
        cmds.push(m(
            "ext.repo_remove",
            Msg::MnExtensions,
            Msg::MnExtRepoRemove,
        ));
        cmds.push(m(
            "edit.expand_brackets",
            Msg::MnEdit,
            Msg::MnExpandBrackets,
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
        cmds.push(m("view.txlog", Msg::MnView, Msg::MnTxLogWindow));
        cmds.push(m("view.toolbar_reset", Msg::MnView, Msg::MnResetToolbar));
        cmds.push(m("view.colors", Msg::MnView, Msg::MnColors));
        cmds.push(m("view.keys", Msg::MnView, Msg::MnKeys));
        cmds.push(m("view.explorer", Msg::MnView, Msg::MnExplorer));
        cmds.push(m("view.search", Msg::MnView, Msg::MnSearchPanel));
        cmds.push(m("file.close_tab", Msg::MnFile, Msg::MnCloseTab));
        cmds.push(m("tab.next", Msg::MnView, Msg::MnNextTab));
        cmds.push(m("tab.prev", Msg::MnView, Msg::MnPrevTab));
        cmds.push(m("view.theme", Msg::MnView, Msg::MnTheme));
        cmds.push(m("view.lang", Msg::MnView, Msg::MnLanguage));
        cmds.push(m("run.statement", Msg::MnRun, Msg::MnRunStatement));
        cmds.push(m(
            "run.statement_new_tab",
            Msg::MnRun,
            Msg::MnRunStatementNewTab,
        ));
        cmds.push(m("result.tab.close", Msg::MnRun, Msg::MnResultCloseTab));
        cmds.push(m("result.tab.next", Msg::MnRun, Msg::MnResultNextTab));
        cmds.push(m("result.tab.prev", Msg::MnRun, Msg::MnResultPrevTab));
        cmds.push(m("run.all", Msg::MnRun, Msg::MnRunAll));
        cmds.push(m("run.explain", Msg::MnRun, Msg::MnExplain));
        cmds.push(m("run.commit", Msg::MnRun, Msg::MnCommit));
        cmds.push(m("run.rollback", Msg::MnRun, Msg::MnRollback));
        cmds.push(m("conn.toggle", Msg::MnRun, Msg::MnConnect));
        cmds.push(m("conn.disconnect", Msg::MnRun, Msg::MnDisconnect));
        for (id, msg) in [
            ("edit.duplicate_line", Msg::MnDuplicateLine),
            ("edit.delete_line", Msg::MnDeleteLine),
            ("edit.join_lines", Msg::MnJoinLines),
            ("edit.swap_line_up", Msg::MnSwapLineUp),
            ("edit.swap_line_down", Msg::MnSwapLineDown),
            ("edit.toggle_comment", Msg::MnToggleComment),
            ("edit.indent", Msg::MnIndent),
            ("edit.unindent", Msg::MnUnindent),
            ("edit.select_line", Msg::MnSelectLine),
            ("edit.split_lines", Msg::MnSplitLines),
            ("edit.add_caret_up", Msg::MnAddCaretUp),
            ("edit.add_caret_down", Msg::MnAddCaretDown),
            ("edit.upper_case", Msg::MnUpperCase),
            ("edit.lower_case", Msg::MnLowerCase),
            ("edit.goto_line", Msg::MnGotoLine),
            ("eol.crlf", Msg::MnEolCrlf),
            ("eol.lf", Msg::MnEolLf),
            ("eol.cr", Msg::MnEolCr),
        ] {
            cmds.push(m(id, Msg::MnEdit, msg));
        }
        cmds.push(m("view.goto_anything", Msg::MnView, Msg::MnGotoAnything));
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
        let tabs = self.editors.tab_list();
        self.menubar
            .set_menus(App::build_menus_with(&v, &tabs, self.demo_ready));
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
        enc::decode(bytes, enc)
    }

    fn encode_text(text: &str, enc: &str) -> Vec<u8> {
        enc::encode(text, enc)
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
        let (eol, text) = eol::detect(&text);
        self.editors.open_file(path, &text, eol);
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
        let eol = eol::save_eol(
            self.settings.get("file.eol_save").unwrap_or("keep"),
            self.editors.active_eol(),
        );
        let text = eol::apply(&self.editors.cur().text(), eol);
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
                self.git.refresh(true);
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
            Ok(_) => {
                self.status = tf(Msg::StProfileDeleted, &[name]);
                self.panel_results.remove(name.trim());
            }
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

    /// 트랜잭션 로그 창(모덜리스 · docs/44 §2 · T-107) — 이미 열려 있으면 앞으로.
    fn open_txlog_window(&mut self, el: &ActiveEventLoop) {
        let near = self.window.as_ref().and_then(|w| {
            w.outer_position()
                .ok()
                .map(|p| (p.x, p.y, w.outer_size().width))
        });
        let owner = self.window.clone();
        let ctx = self.conn_win.active_name().to_string();
        self.txlog_win.set_active_editor(self.editors.active_id());
        self.txlog_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
            &ctx,
        );
    }

    /// 열린 수동 트랜잭션을 결과와 함께 닫고 대기 목록을 비운다(커밋·롤백·암묵·전환·끊김 — docs/44 §3).
    fn tx_close(&mut self, outcome: TxOutcome) {
        let stamp = nsql_log::now_local().stamp();
        self.txlog.close_tx(outcome, stamp);
        self.tx_clear();
        self.txlog_win.redraw();
    }

    /// 개발자 모드 마스크(`log.dev_mode` × `log.dev_layers` · 향상 모드는 dev_mode를 끈다) → nsql-log 전역 게이트.
    fn apply_detail_mask(&mut self) {
        let on = self.settings.flag("log.dev_mode");
        let mask = if on {
            nsql_log::parse_detail_layers(self.settings.get("log.dev_layers").unwrap_or(""))
        } else {
            0
        };
        nsql_log::set_detail_mask(mask);
        self.log_win.set_dev(on);
        self.log_win.set_dev_mask(nsql_log::parse_detail_layers(
            self.settings.get("log.dev_layers").unwrap_or(""),
        ));
    }

    /// 실행 상태 카드 설정(`run.toast` · `run.toast_hide_secs` · 불투명도는 토스트와 공용 `ui.toast_alpha`).
    fn apply_run_toast(&mut self) {
        self.run_toast.configure(
            self.settings.flag("run.toast"),
            self.settings.int("run.toast_hide_secs"),
            self.settings.int("ui.toast_alpha"),
        );
    }

    /// 실행 시작 → 카드(문장 · 시작 시각 · 문장 수).
    fn run_toast_start(&mut self, src: &str) {
        self.run_cancel_requested = false;
        self.sync_run_stmt_button();
        self.editors.set_running(Some(self.run_editor));
        self.editors.set_error_line(self.run_editor, None);
        let n = split_items(src).len().max(1);
        self.run_toast.start(src, nsql_log::now_local().stamp(), n);
    }

    /// 중지(카드 ■ = 툴바 ■ · T-108): 실행 중 문장은 드라이버 취소 핸들로 서버에 취소 · 전체 조회는 다음 배치 경계에서.
    fn stop_run(&mut self) {
        if !self.busy && !self.grid.fetch_all_active() {
            return;
        }
        self.run_cancel_requested = self.busy;
        if self.dialect == Dialect::Mssql
            && self.settings.get("mssql.cancel") != Some("socket")
            && self.settings.get("mssql.encrypt") != Some("login")
        {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                t(Msg::StMssqlAttentionFallback),
            ));
        }
        let (sent, drops) = self.worker.cancel_run();
        self.run_cancel_drops = drops;
        self.status = t(if sent && drops {
            Msg::StRunCancellingDrop
        } else if sent {
            Msg::StRunCancelling
        } else {
            Msg::StFetchCancelling
        })
        .into();
        self.log_win
            .push(LogEntry::new(LogKind::Info, self.status.clone()));
        self.redraw();
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

    // ── 데모 프로필·샘플 데이터(사용자 09-17 · docs/21 §5)

    /// 메뉴바를 현재 상태(최근 파일 · 탭 · 데모 준비 여부)로 다시 만든다.
    fn rebuild_menus(&mut self) {
        let tabs = self.editors.tab_list();
        self.menubar.set_menus(App::build_menus_with(
            &self.recent_files(),
            &tabs,
            self.demo_ready,
        ));
    }

    /// 데모 SQLite 파일 = 사용자 설정 폴더(`NSQL_HOME`)/demo.sqlite — 설치본·포터블 규약 그대로(exe 옆 금지).
    fn demo_path() -> Option<PathBuf> {
        nsql_settings::config_dir().map(|d| d.join("demo.sqlite"))
    }

    /// 'Demo' 프로필과 파일이 둘 다 있는가.
    fn demo_exists() -> bool {
        let has_profile = Vault::open_default()
            .ok()
            .and_then(|v| v.peek("Demo").ok().flatten())
            .is_some();
        has_profile && Self::demo_path().is_some_and(|p| p.exists())
    }

    /// 최초 실행 1회 팝업(창 가운데): 만들기 / 나중에.
    fn open_demo_prompt(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let Some(w) = self.window.as_ref() else {
            return;
        };
        let sz = w.inner_size();
        let r = Rect::new(
            (sz.width as i32 / 2 - px(150.0, self.scale)).max(0),
            (sz.height as i32 / 3).max(0),
            0,
            0,
        );
        let items = vec![
            CtxItem::item("demo.ask", t(Msg::DemoAsk)),
            CtxItem::Separator,
            CtxItem::item("demo.create", t(Msg::DemoYes)),
            CtxItem::item("demo.later", t(Msg::DemoLater)),
        ];
        self.open_status_popup(r, items);
        self.redraw();
    }

    /// 데모 만들기(메뉴 · 팝업): 배경 스레드에서 `demo.sqlite`에 내장 스크립트(`examples/demo.sql`)를 실행 → 끝나면 프로필 저장.
    fn start_demo_create(&mut self) {
        if self.demo_job.is_some() || self.demo_ready {
            return;
        }
        let Some(path) = Self::demo_path() else {
            self.status = tf(Msg::StDemoFailed, &["NSQL_HOME"]);
            return;
        };
        self.status = t(Msg::StDemoCreating).into();
        let (tx, rx) = std::sync::mpsc::channel();
        self.demo_job = Some(rx);
        std::thread::Builder::new()
            .name("nsql-demo".into())
            .spawn(move || {
                let r = create_demo_db(&path);
                let _ = tx.send(r);
            })
            .ok();
        self.redraw();
    }

    /// 생성 결과: 프로필 'Demo' 저장 → 접속 창 목록 갱신 → 메뉴 비활성 → 안내.
    fn finish_demo(&mut self, r: Result<String, String>) {
        match r.and_then(|path| {
            let target = format!("sqlite:{path}");
            let spec = nsql_drivers::parse_target(&target, Dialect::Sqlite)?;
            Vault::open_default()
                .and_then(|v| v.save("Demo", &spec))
                .map_err(|e| e.to_string())?;
            Ok(path)
        }) {
            Ok(path) => {
                self.demo_ready = true;
                self.rebuild_menus();
                self.conn_win.refresh_profiles(Some("Demo"));
                self.status = tf(Msg::StDemoCreated, &[&path]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.status.clone()));
            }
            Err(e) => {
                self.status = tf(Msg::StDemoFailed, &[&e]);
                self.toasts
                    .push(toast::ToastKind::Error, t(Msg::MnDemoCreate), e);
            }
        }
        self.redraw();
    }

    fn persist_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.status = tf(Msg::CfgSaveFailed, &[&e.to_string()]);
        }
    }

    /// 언어가 바뀌면 컨트롤 문자열을 다시 만든다. TextBox는 placeholder 교체 API가 없어 본문을 보존해 재생성.
    fn relabel(&mut self) {
        let tabs = self.editors.tab_list();
        self.menubar.set_menus(App::build_menus_with(
            &self.recent_files(),
            &tabs,
            self.demo_ready,
        ));
        let layout = self.tool_dock.layout();
        self.tool_dock = App::build_tool_dock();
        self.tool_dock.apply_layout(&layout);
        let _ = self.tool_dock.take_actions();
        self.apply_toolbar_visibility();
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
        // 다중 커서/선택이면 "캐럿 문장"이 하나가 아니다 → 문장 실행은 막고 전체 실행(F5)만(사용자 09-17).
        if !all && self.editors.cur().has_multi() {
            self.status = t(Msg::StMultiCaretRun).into();
            self.redraw();
            return;
        }
        // Ctrl/⌘+Enter = 선택 영역 → 없으면 **캐럿 위치의 한 문장**(`;` 종결 · 사용자 09-14) → F5 = 전체.
        let mut line_base = 0usize;
        let text = if all {
            None
        } else {
            let sel = self
                .ed_mut()
                .copy_selection()
                .filter(|s| !s.trim().is_empty());
            if sel.is_some() {
                // 선택 실행: 선택 시작 줄.
                if let Some((a, _)) = self.ed_mut().selection() {
                    let t = self.ed_mut().text();
                    line_base = t.chars().take(a).filter(|c| *c == '\n').count();
                }
                sel
            } else {
                let full = self.ed_mut().text();
                let byte_pos = full
                    .char_indices()
                    .nth(self.ed_mut().caret())
                    .map_or(full.len(), |(b, _)| b);
                nsql_script::statement_at(&full, byte_pos).map(|it| {
                    line_base = it.line.saturating_sub(1);
                    it.text
                })
            }
        };
        self.run_line_base = line_base;
        let src = text.unwrap_or_else(|| self.ed_mut().text());
        self.grid.set_source_sql(&src);
        self.run_tab = self.grid_tab;
        self.run_editor = self.editors.active_id();
        if src.trim().is_empty() {
            self.status = t(Msg::ErrNoSql).into();
            return;
        }
        self.busy = true;
        self.status = t(Msg::StRunning).into();
        self.run_toast_start(&src);
        // 신호등이 초록이 아닌 서버(빨강·파랑·확인 중·모름)에는 실행 전 빠른 포트 판정을 건다(사용자 09-14).
        let pol = *self.conn_win.policy();
        let light = self.conn_win.status_of(self.conn_win.active_name());
        let preflight =
            (pol.enabled && light != Some(probe::ProbeStatus::Up)).then_some(pol.timeout);
        self.last_run_items = split_items(&src);
        let max_rows = self.grid.page_rows();
        self.single_run = !all;
        self.worker.send(worker::Cmd::Run {
            src,
            preflight,
            max_rows,
        });
        self.live_start();
        self.redraw();
    }

    /// 라이브 로그 설정(끔이면 None) — 매번 읽는다(설정 창에서 바꾸면 다음 실행부터).
    fn live_req(&self) -> Option<LiveReq> {
        if self.dialect != Dialect::Oracle {
            return None;
        }
        let source = self
            .settings
            .effective("oracle.live.source")
            .unwrap_or("off");
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
                        self.log_win.push(LogEntry::new(LogKind::Output, text));
                        changed = true;
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
        self.run_toast_start(&src);
        self.last_run_items = split_items(&src);
        self.worker.send(worker::Cmd::Run {
            src,
            preflight: None,
            max_rows: self.grid.page_rows(),
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
                RunEvent::Begin { index, .. } => {
                    if index == 0 {
                        self.txlog.begin_batch();
                    }
                    self.run_toast.set_phase(runtoast::Phase::Running {
                        index,
                        total: self.last_run_items.len(),
                    });
                    let stmt = self.last_run_items.get(index).cloned().unwrap_or_default();
                    dlog!(self, LogLayer::Net, LogLevel::Timing, {
                        let now = nsql_log::now_local().stamp();
                        LogEntry::new(
                            LogKind::Send,
                            tf(Msg::LogDetSent, &[&now, &stmt.len().to_string()]),
                        )
                    });
                    let purpose = if stmt
                        .trim_start()
                        .to_ascii_uppercase()
                        .starts_with("SET AUTOCOMMIT")
                    {
                        TxPurpose::Util
                    } else {
                        TxPurpose::User
                    };
                    self.txlog.begin(
                        nsql_log::now_local().stamp(),
                        self.run_editor,
                        purpose,
                        index,
                        &stmt,
                    );
                    self.txlog_win.redraw();
                }
                RunEvent::ResultSet {
                    index,
                    rs,
                    elapsed,
                    more,
                } => {
                    self.txlog.result(index, rs.rows.len() as u64, elapsed);
                    self.run_toast
                        .first_page(rs.rows.len() as u64, rs.approx_bytes(), elapsed);
                    dlog!(self, LogLayer::Net, LogLevel::Timing, {
                        let b = rs.approx_bytes();
                        LogEntry::new(
                            LogKind::Fetch,
                            tf(
                                Msg::LogDetFirstSeg,
                                &[&nsql_core::fmt_bytes(b), &speed_of(b, elapsed)],
                            ),
                        )
                        .rows(rs.rows.len() as u64)
                        .elapsed(elapsed)
                    });
                    self.run_toast.set_phase(runtoast::Phase::Done {
                        rows: Some(rs.rows.len() as u64),
                        secs: elapsed.as_secs_f64(),
                        stages: String::new(),
                    });
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
                    // 결과는 실행을 시작한 결과 탭의 그리드로(탭이 이미 닫혔으면 버림) · 탭 제목 갱신(T-93).
                    if let Some(g) = self.run_grid() {
                        g.set_result(rs);
                        g.set_more(more);
                    }
                    self.tx_on_read(index);
                    let k = self.run_tab;
                    self.retitle_result(k);
                }
                RunEvent::Done {
                    index,
                    rows_affected,
                    elapsed,
                } => {
                    let secs = format!("{:.3}", elapsed.as_secs_f64());
                    self.status = match rows_affected {
                        Some(n) => tf(Msg::StRowsAffected, &[&n.to_string(), &secs]),
                        None => tf(Msg::StOk, &[&secs]),
                    };
                    let stmt = self.last_run_items.get(index).cloned().unwrap_or_default();
                    self.txlog.done(index, rows_affected, elapsed);
                    self.run_toast.set_phase(runtoast::Phase::Done {
                        rows: rows_affected,
                        secs: elapsed.as_secs_f64(),
                        stages: String::new(),
                    });
                    self.tx_on_done(index, &stmt, rows_affected);
                }
                // PRINT · 서버 메시지는 로그 창으로만(`log_entries`) — 결과 영역은 조회 결과만(사용자 09-17).
                RunEvent::Print { .. } => {}
                RunEvent::Message(m) => {
                    if m == t(Msg::StCommitted) {
                        self.tx_close(TxOutcome::Committed);
                        self.status = m.clone();
                    } else if m == t(Msg::StRolledBack) {
                        self.tx_close(TxOutcome::RolledBack);
                        self.status = m.clone();
                    }
                }
                RunEvent::Connected {
                    description,
                    dialect,
                } => {
                    self.status = tf(Msg::StConnected, &[&description, &dialect.to_string()]);
                    self.busy = false;
                    self.dialect = dialect;
                    self.all_grids().for_each(|g| g.set_dialect(dialect));
                    self.tx_close(TxOutcome::Lost);
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
                    self.tx_close(TxOutcome::Lost);
                    self.sync_disconnect_btn(false);
                }
                RunEvent::Timing { index, timeline } => {
                    // 상태줄 = 결과 요약 + 단계별 소요(docs/26). 렌더 시간은 그리드 푸터가 자체 표시.
                    self.txlog.timing(index, timeline.total());
                    self.run_toast
                        .timing(timeline.total().as_secs_f64(), timeline.summary());
                    for sp in &timeline.spans {
                        let (layer, msg) = match sp.stage {
                            nsql_core::Stage::Send => (LogLayer::Net, Msg::LogDetStageSend),
                            nsql_core::Stage::Execute => (LogLayer::Exec, Msg::LogDetStageExec),
                            nsql_core::Stage::Fetch | nsql_core::Stage::Receive => {
                                (LogLayer::Fetch, Msg::LogDetStageFetch)
                            }
                            nsql_core::Stage::Load => (LogLayer::Load, Msg::LogDetStageLoad),
                            _ => (LogLayer::App, Msg::LogDetStageOther),
                        };
                        if layer == LogLayer::App {
                            continue;
                        }
                        dlog!(self, layer, LogLevel::Timing, {
                            let b = sp.bytes.unwrap_or(0);
                            LogEntry::new(
                                LogKind::Info,
                                tf(
                                    msg,
                                    &[
                                        sp.stage.label(),
                                        &nsql_core::fmt_dur(sp.dur),
                                        &nsql_core::fmt_bytes(b),
                                        &speed_of(b, sp.dur),
                                        sp.note.as_deref().unwrap_or(""),
                                    ],
                                ),
                            )
                            .rows(sp.rows)
                            .elapsed(sp.dur)
                        });
                    }
                    self.status = format!("{} · ⏱ {}", self.status, timeline.summary());
                }
                RunEvent::Error { index, line, error } => {
                    self.txlog.error(index, error.code, &error.message);
                    self.txlog_win.redraw();
                    if std::mem::take(&mut self.run_cancel_requested) {
                        // 사용자가 ■를 눌러 드라이버가 끊은 실행 — 오류 토스트 대신 "중지됨"(T-108).
                        if std::mem::take(&mut self.run_cancel_drops) {
                            // 소켓을 끊은 취소(SQL Server): 열린 트랜잭션은 서버가 롤백 · 다음 실행 때 자동 재접속.
                            self.tx_close(TxOutcome::Lost);
                            self.status = t(Msg::StRunCancelledDrop).into();
                            self.log_win
                                .push(LogEntry::new(LogKind::Info, self.status.clone()));
                        } else {
                            self.status = t(Msg::StRunCancelled).into();
                        }
                        self.run_toast
                            .set_phase(runtoast::Phase::Stopped { rows: 0 });
                        if let Some(g) = self.run_grid() {
                            g.clear_result();
                        }
                        self.busy = false;
                        self.sync_run_stmt_button();
                        self.editors.set_running(None);
                        self.redraw();
                        continue;
                    }
                    // 공통 분류 + 코드 부각(docs/42): 상태줄 · 결과 메시지 · 로그 창 · 토스트(분류된 오류만).
                    let stmt = self.last_run_items.get(index).cloned().unwrap_or_default();
                    let (cls, summary) =
                        toast::summarize(self.dialect, error.code, &error.message, &stmt);
                    self.status = tf(Msg::StErrorLine, &[&line.to_string(), &summary]);
                    if self.settings.flag("editor.minimap_errors") && line > 0 {
                        let ed_line = self.run_line_base + line - 1;
                        self.editors.set_error_line(self.run_editor, Some(ed_line));
                    }
                    self.run_toast
                        .set_phase(runtoast::Phase::Error(summary.clone()));
                    // 오류가 나도 결과 영역은 기본 형태(빈 그리드)로 — 본문은 로그 창·상태줄·토스트(사용자 09-17).
                    if let Some(g) = self.run_grid() {
                        g.clear_result();
                    }
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
            self.sync_run_stmt_button();
            self.editors.set_running(None);
            let failed = done.is_some();
            if std::mem::take(&mut self.run_cancel_requested) && !failed {
                // Attention 취소(SQL Server · 세션 유지): 오류 없이 부분 결과로 끝난다 → "중지됨".
                self.run_cancel_drops = false;
                // ★ 실행 중지 = 받은 행은 **보기만 유지**(사용자 09-17 결정): 이번 실행 스트림의 앞부분이라 보는 용도로는 정확하지만
                //   OFFSET 재실행은 정렬이 없으면 순서가 달라질 수 있어 이어 받기(⇊)·자동 페치는 막는다(`more=false`) · 전체는 재실행.
                //   (⇊ 나머지 이어 받기의 중지는 T-48b대로 받은 행 + 더 있음 유지 — 늘 연속된 앞부분 · 사용자 "이전 세그먼트 방식 유지".)
                let rows = self.last_rows.unwrap_or(0) as u64;
                self.status = tf(Msg::StRunCancelledPartial, &[&rows.to_string()]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.status.clone()));
                self.run_toast.finish(runtoast::Phase::Stopped { rows });
                if let Some(g) = self.run_grid() {
                    g.set_more(false);
                }
            } else {
                self.run_toast.finish_keep(done.clone());
            }
            if let Some(m) = done {
                self.status = m;
            }
            // 현재 문장 실행 뒤 캐럿(설정 `run.after_statement` · 사용자 09-16): stay / next_ok / next_always.
            if std::mem::take(&mut self.single_run) {
                let mode = self.settings.get("run.after_statement").unwrap_or("stay");
                if mode == "next_always" || (mode == "next_ok" && !failed) {
                    self.goto_statement(true);
                }
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
        let (grid, panel, panels) = (&mut self.grid, &mut self.panel, &mut self.panels);
        std::iter::once(grid)
            .chain(panel.tabs.iter_mut().map(|t| &mut t.grid))
            .chain(
                panels
                    .values_mut()
                    .flat_map(|p| p.tabs.iter_mut().map(|t| &mut t.grid)),
            )
    }

    /// 마지막 실행 대상 결과 탭의 그리드(활성이면 `grid` · 아니면 잠든 것 · 닫혔으면 `None`).
    fn run_grid(&mut self) -> Option<&mut grid::Grid> {
        let k = self.run_tab;
        self.grid_for(k)
    }

    /// ★ 편집기 탭 ↔ 결과 패널 쌍 동기화(사용자 09-16 · T-93): 활성 편집기 탭이 바뀌었으면 그 탭의 패널을 꺼내 오고
    /// (없으면 설정만 물려받은 빈 탭 하나) 지금 패널은 잠재운다 · 닫힌 편집기 탭의 패널은 통째로 버린다(rows 즉시 해제).
    /// 페인트 직전과 이벤트 뒤에 부른다.
    fn sync_grid_tab(&mut self) {
        self.sync_tabs_menu();
        let cur = self.editors.active_id();
        if cur != self.panel_editor {
            let b = self.grid.bounds;
            // 실제 그리드를 활성 자리에 돌려놓고 패널을 잠재운다.
            let a = self
                .panel
                .active
                .min(self.panel.tabs.len().saturating_sub(1));
            if let Some(slot) = self.panel.tabs.get_mut(a) {
                std::mem::swap(&mut self.grid, &mut slot.grid);
            }
            let enabled = self.settings.flag("grid.result_tabs");
            let always = self.settings.get("grid.result_tabbar") == Some("always");
            let next = self.panels.remove(&cur).unwrap_or_else(|| {
                let id = self.next_result_id;
                self.next_result_id += 1;
                let fresh = self
                    .panel
                    .tabs
                    .get(a)
                    .map_or_else(grid::Grid::default, |t| t.grid.fresh_like());
                ResultPanel::new(
                    ResultTab {
                        id,
                        title: t(Msg::ResultTabDefault).to_string(),
                        pinned: false,
                        named: false,
                        grid: fresh,
                        seq: id,
                    },
                    enabled,
                    always,
                )
            });
            let old = std::mem::replace(&mut self.panel, next);
            if self.panel_editor != 0 {
                self.panels.insert(self.panel_editor, old);
            }
            self.panel_editor = cur;
            let a = self
                .panel
                .active
                .min(self.panel.tabs.len().saturating_sub(1));
            self.panel.active = a;
            std::mem::swap(&mut self.grid, &mut self.panel.tabs[a].grid);
            self.grid_tab = self.panel.tabs[a].id;
            self.grid.set_bounds(b);
            self.panel.sync_bar();
            self.layout();
        }
        let alive = self.editors.tab_ids();
        self.panels.retain(|id, _| alive.contains(id));
    }

    fn paint(&mut self) {
        // 찾기가 열린 동안 본문이 바뀌면 일치 표시도 따라간다(전체 스캔 · 열려 있을 때만 · T-73).
        if self.find.is_visible() {
            self.sync_find_marks();
        }
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
        let caret_on = !self.settings.flag("editor.caret_blink")
            || (self.started.elapsed().as_millis() / 500) % 2 == 0;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let th = self.theme;
            let ui_px = self.settings.font_px("ui.font_size");
            let mono_px = self.settings.font_px("editor.font_size");
            let grid_px = self.settings.font_px("grid.font_size");
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
                self.panel.paint_bar(&mut dc, &th);
                // 메뉴바·툴바(창 전폭) — 메뉴 드롭다운은 최상위라 맨 뒤에.
                dc.fill_rect(self.tool_dock.bounds(), th.chrome_bg);
                self.tool_dock.paint(&mut dc, &th);
                // 툴바 툴팁은 탐색기·편집기가 덮지 못하게 최상위 층(메뉴바 직전)에서 그린다(09-16 사용자 캡처: 툴바 아래 검은 띠).
                dc.fill_rect(self.menubar.bounds(), th.chrome_bg);
                dc.fill_rect(
                    Rect::new(0, self.tool_dock.bounds().bottom() - 1, wi, 1),
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
                let left_text = format!("{busy}{}", self.status);
                // 오른쪽 세그먼트(Sublime/DBeaver/Golden 참고 · docs/29 §4): 접속 · Ln,Col · rows · time · 구문(클릭 = Set Syntax)
                let (ln, col) = self.editors.caret_line_col();
                let mut segs: Vec<(String, bool)> = Vec::new();
                // 트랜잭션 모드(자동/수동 · 수동에 미커밋 변경이 있으면 ●).
                // 트랜잭션 세그먼트(DR-30): Auto / Manual / "Manual ● n pending · since hh:mm" · 클릭 = 팝업.
                let tx = if self.settings.flag("session.autocommit") {
                    t(Msg::StTxAuto).to_string()
                } else if let Some(first) = self.tx_pending.first() {
                    tf(
                        Msg::StTxPending,
                        &[&self.tx_pending.len().to_string(), &first.when],
                    )
                } else {
                    t(Msg::StTxManual).to_string()
                };
                let tx_idx = segs.len();
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
                    // 열 모드·다중 커서 = 선택 영역 수(Sublime "5 selection regions").
                    segs.push((tf(Msg::StSelections, &[&nsel.to_string()]), false));
                } else if let Some((l, c)) = self.editors.selection_summary() {
                    // 일반 선택 = 줄 수·문자 수(Sublime "6 lines, 90 characters selected").
                    segs.push((
                        tf(Msg::StSelected, &[&l.to_string(), &c.to_string()]),
                        false,
                    ));
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
                // git 세그먼트(Sublime `main ⑥` · 활성 파일 폴더 · 설정 `statusbar.git` · 배경 조회).
                if self.settings.flag("statusbar.git") {
                    let dir = self
                        .editors
                        .active_path()
                        .and_then(|p| p.parent().map(Path::to_path_buf));
                    self.git.set_dir(dir);
                    self.git.refresh(false);
                    if let Some(info) = self.git.info() {
                        segs.push((format!("{} ({})", info.branch, info.changed), false));
                    }
                }
                // 인코딩 세그먼트(클릭 = 저장 인코딩 / 다시 열기 팝업 · 사용자 09-16).
                segs.push((enc::short(&self.editors.active_encoding()), true));
                // 줄끝 세그먼트(VS Code/Sublime식 · 클릭 = LF/CRLF 팝업 · docs/38 · 사용자 09-16).
                segs.push((self.editors.active_eol().label().to_string(), true));
                segs.push((indent_seg, true));
                segs.push((self.editors.syntax_name(), true));
                let gap = px(12.0, s);
                let mut xr = wi - px(8.0, s);
                self.status_syntax_rect = Rect::new(0, 0, 0, 0);
                self.status_tab_rect = Rect::new(0, 0, 0, 0);
                self.status_eol_rect = Rect::new(0, 0, 0, 0);
                self.status_enc_rect = Rect::new(0, 0, 0, 0);
                self.status_tx_rect = Rect::new(0, 0, 0, 0);
                let last = segs.len() - 1;
                for (idx, (text, is_syntax)) in segs.iter().enumerate().rev() {
                    let tw = dc.text_width(text);
                    xr -= tw;
                    let r = Rect::new(xr - gap / 2, sy, tw + gap, px(24.0, s));
                    if idx == tx_idx {
                        self.status_tx_rect = r;
                    }
                    let ty = dc.text_center_y(sy, px(24.0, s));
                    dc.text(
                        xr,
                        ty,
                        r,
                        text,
                        if *is_syntax { th.text } else { th.text_dim },
                    );
                    // 오른쪽 끝부터: 구문 · 들여쓰기 · 줄끝 · 인코딩(클릭 가능한 4개 · 그 앞은 표시만).
                    if *is_syntax {
                        match last - idx {
                            0 => self.status_syntax_rect = r,
                            1 => self.status_tab_rect = r,
                            2 => self.status_eol_rect = r,
                            3 => self.status_enc_rect = r,
                            _ => {}
                        }
                    }
                    xr -= gap;
                    dc.fill_rect(
                        Rect::new(xr + gap / 2, sy + px(5.0, s), 1, px(14.0, s)),
                        th.border,
                    );
                }
                // 왼쪽 상태 문구는 세그먼트 앞에서 잘라 겹치지 않게(09-16 캡처: 긴 타이밍 문구가 세그먼트 위로 지나갔다).
                dc.select_font(FontSlot::Base, false);
                let left_w = (xr - px(8.0, s) - px(4.0, s)).max(0);
                dc.text(
                    px(8.0, s),
                    ty,
                    Rect::new(0, sy, px(8.0, s) + left_w, px(24.0, s)),
                    &left_text,
                    th.text_dim,
                );
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
                let gf: &Font = self.grid_font.as_ref().unwrap_or(&self.ui_font);
                // 행번호는 고정폭 슬롯(편집기 고정폭 얼굴 · 사용자 09-17) — 자릿수 폭이 흔들리지 않게.
                let fonts = FontSet {
                    mono: Some(&self.mono_font),
                    ..FontSet::single(gf)
                };
                let mut dc = RasterCtx::with_font_set(&mut gfx, fonts, s).with_fonts(prefs);
                self.grid.paint(&mut dc, &th, s);
            }
            mark(&mut t_sec, &mut marks); // 2 = 그리드
            if let Some((lines, dur)) = self.grid.take_text_report() {
                dlog!(self, LogLayer::Load, LogLevel::Timing, {
                    LogEntry::new(
                        LogKind::Info,
                        tf(Msg::LogDetTextView, &[&lines.to_string()]),
                    )
                    .elapsed(dur)
                });
            }
            if let Some((render, load, bytes, at)) = self.grid.take_perf_report() {
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    tf(
                        Msg::LogRenderPerf,
                        &[
                            &nsql_core::fmt_dur(render),
                            &nsql_core::fmt_dur(load),
                            &nsql_core::fmt_bytes(bytes),
                        ],
                    ),
                ));
                dlog!(self, LogLayer::Load, LogLevel::Timing, {
                    LogEntry::new(
                        LogKind::Info,
                        tf(
                            Msg::LogDetLoad,
                            &[&nsql_core::fmt_dur(load), &nsql_core::fmt_bytes(bytes)],
                        ),
                    )
                    .elapsed(load)
                });
                dlog!(self, LogLayer::Render, LogLevel::Timing, {
                    let (a, b) = at.unwrap_or_default();
                    LogEntry::new(
                        LogKind::Info,
                        tf(Msg::LogDetRender, &[&a, &b, &nsql_core::fmt_dur(render)]),
                    )
                    .elapsed(render)
                });
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
                // In selection 토글 = 편집기에 선택이 있거나 이미 범위가 잡혀 있을 때만(사용자 09-17).
                let sel_ok =
                    self.find_scope.is_some() || self.editors.selection_summary().is_some();
                self.find.set_selection_available(sel_ok);
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s).with_fonts(prefs);
                self.find.paint(&mut dc, &th);
                // 결과 도구줄 상태 글자 = 상태줄과 같은 UI 글꼴·크기(사용자 09-16).
                self.grid.paint_footer_text(&mut dc, &th);
                self.editors.paint_tooltip(&mut dc, &th, wi);
            }
            // ── 오브젝트 탐색기(자체 글꼴 크기 `explorer.font_size` · 기본 = 메뉴 글꼴 · 사용자 09-15)
            {
                let exp_px = match self.settings.font_px("explorer.font_size") {
                    e if e <= 0.0 => self.settings.font_px("ui.menu_font_size"),
                    e => e,
                };
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
                self.search.paint(&mut dc, &th);
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
                self.tool_dock.paint_tooltip(&mut dc, &th);
                self.find.paint_tooltip(&mut dc, &th);
                self.search.paint_tooltip(&mut dc, &th);
                // ★ 팝업(상태줄 메뉴 · 결과 도구줄 툴팁/메뉴 · 팔레트)은 스플리터 **뒤**에 — 앞 층에서 그리면 편집기|결과
                //   구분선이 팝업 위로 지나갔다(09-16 캡처 · 팝업 = 맨 마지막 층 규칙).
                self.status_menu.paint(&mut dc, &th);
                self.grid.paint_overlays(&mut dc, &th);
                self.panel.paint_popups(&mut dc, &th);
                self.editors.paint_popups(&mut dc, &th);
                self.palette.paint(&mut dc, &th);
                // 토스트 = 편집기 영역의 우하단(결과 그리드를 가리지 않게 · 사용자 09-16 · docs/42).
                let eb = self.editors.editor_bounds();
                let (tx, ty) = if eb.w > 0 && eb.h > 0 {
                    (eb.right(), eb.bottom())
                } else {
                    (wi, hi - px(24.0, s))
                };
                let ty = self.run_toast.paint(&mut dc, &th, tx, ty, s);
                self.toasts.paint(&mut dc, &th, tx, ty, s);
            }
            // ── 메뉴바 + 열린 드롭다운(별도 글꼴 크기 `ui.menu_font_size` · 팝업 규칙대로 맨 마지막 층 · 사용자 09-15)
            {
                let prefs = FontPrefs {
                    base: SlotFont {
                        size: self.settings.font_px("ui.menu_font_size"),
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

    /// 열(블록) 선택 마우스 규칙(설정 `editor.column_select` · auto = OS별 · 사용자 09-17).
    fn column_rule(&self) -> &'static str {
        match self.settings.get("editor.column_select").unwrap_or("auto") {
            "auto" => {
                if cfg!(target_os = "windows") {
                    "alt_shift"
                } else if cfg!(target_os = "macos") {
                    "alt"
                } else {
                    "shift_right"
                }
            }
            "alt" => "alt",
            "shift_right" => "shift_right",
            _ => "alt_shift",
        }
    }

    fn ctl_event(&mut self, event: &WindowEvent) -> Option<InputEvent> {
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
                    // Linux(Sublime) 규칙: Shift+우클릭 = 열 선택 시작 — 좌드래그로 바꿔 보내고 메뉴는 열지 않는다.
                    if self.column_rule() == "shift_right"
                        && self.shift
                        && self.editors.editor_bounds().contains(Point { x, y })
                    {
                        self.col_right_drag = true;
                        self.editors.set_column_mode(true);
                        InputEvent::MouseDown {
                            x,
                            y,
                            shift: false,
                            primary: false,
                        }
                    } else {
                        InputEvent::RightDown { x, y }
                    }
                }
                (ElementState::Released, winit::event::MouseButton::Right)
                    if self.col_right_drag =>
                {
                    self.col_right_drag = false;
                    self.editors.set_column_mode(false);
                    InputEvent::MouseUp { x, y }
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
                    // ← → 수식키 번역(Sublime 기본 키맵 · 사용자 09-17): Win/Linux Ctrl = 단어 · Alt = 서브워드 ·
                    // mac ⌥ = 단어 · ⌃ = 서브워드 · ⌘ = 줄 처음/끝(primary 그대로).
                    Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowRight) => {
                        let right =
                            matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::ArrowRight));
                        let (word, subword) = if cfg!(target_os = "macos") {
                            (self.alt, self.ctrl_raw)
                        } else {
                            (self.primary && !self.alt, self.alt && !self.primary)
                        };
                        let k = match (word, subword, right) {
                            (true, _, false) => CtlKey::WordLeft,
                            (true, _, true) => CtlKey::WordRight,
                            (false, true, false) => CtlKey::SubwordLeft,
                            (false, true, true) => CtlKey::SubwordRight,
                            (false, false, false) => CtlKey::Left,
                            (false, false, true) => CtlKey::Right,
                        };
                        let primary = self.primary && !word && !subword;
                        key(k, self.shift, primary)
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

    /// 문장 실행 버튼 = 단일 커서일 때만(전체 실행은 늘 활성 · 사용자 09-17). 값이 바뀔 때만 툴바에 쓴다.
    fn sync_run_stmt_button(&mut self) {
        let stop = self.busy;
        if stop != self.run_stop_enabled {
            self.run_stop_enabled = stop;
            let mut inv = Invalidations::default();
            self.tool_dock.set_item_enabled("run.stop", stop, &mut inv);
            self.redraw();
        }
        let on = !self.editors.cur().has_multi();
        if on != self.run_stmt_enabled {
            self.run_stmt_enabled = on;
            let mut inv = Invalidations::default();
            self.tool_dock
                .set_item_enabled("run.statement", on, &mut inv);
            self.tool_dock.set_item_tip(
                "run.statement",
                t(if on {
                    Msg::TipRunStatement
                } else {
                    Msg::TipRunStatementMulti
                }),
            );
            self.redraw();
        }
    }

    fn route(&mut self, ev: InputEvent) {
        self.route_inner(ev, Invalidations::default());
        self.sync_run_stmt_button();
    }

    fn route_inner(&mut self, ev: InputEvent, mut inv: Invalidations) {
        if let InputEvent::MouseDown { x, y, .. } = ev {
            if self.toasts.click(Point { x, y }) {
                self.redraw();
                return;
            }
            match self.run_toast.click(Point { x, y }) {
                runtoast::RunToastHit::Stop => {
                    self.stop_run();
                    return;
                }
                runtoast::RunToastHit::Card => {
                    self.redraw();
                    return;
                }
                runtoast::RunToastHit::None => {}
            }
        }
        if let InputEvent::MouseMove { x, y } = ev {
            if self.run_toast.hover(Point { x, y }) {
                self.redraw();
            }
        }
        // 마우스 다운은 포커스를 옮긴다.
        // 우클릭 메뉴가 열리기 전에 "붙여넣기 가능" 여부를 넣어 준다.
        if matches!(ev, InputEvent::RightDown { .. }) {
            // 설정 `ui.clipboard_probe`(향상 모드는 끔): 끄면 클립보드를 읽지 않고 붙여넣기를 항상 활성으로.
            let has = !self.settings.flag("ui.clipboard_probe")
                || clipboard::read_text().is_some_and(|s| !s.is_empty());
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
                PaletteAction::Prompt { id, text } => {
                    self.palette.close();
                    if let Some(i) = id.strip_prefix("tab.rename:").and_then(|n| n.parse().ok()) {
                        self.editors.rename_tab(i, &text);
                    } else if id == "ext.repo_add" {
                        self.ext_repo_add(&text);
                    }
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
            if self.status_tx_rect.contains(Point { x, y }) {
                self.open_tx_menu();
                self.redraw();
                return;
            }
            if self.status_enc_rect.contains(Point { x, y }) {
                self.open_enc_menu();
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
            if let InputEvent::RightDown { x, y } = ev {
                if self.tool_dock.bounds().contains(Point { x, y }) {
                    self.open_toolbar_menu(x, y);
                    self.redraw();
                    return;
                }
            }
            self.menubar.on_event(&ev, &mut inv);
            if let Some(id) = self.menubar.take_picked() {
                self.menu_action(&id);
            }
            self.tool_dock.on_event(&ev, &mut inv);
            if let Some(id) = self.tool_dock.take_clicked() {
                self.menu_action(&id);
            }
            self.drain_dock_actions();
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
            let is_ptr = is_mouse || matches!(ev, InputEvent::RightDown { .. });
            // ★ 상자의 우클릭 메뉴가 열려 있으면 바 밖(메뉴가 펼쳐진 곳)의 마우스·키도 찾기 바로(사용자 09-17).
            let popup = self.find.popup_open();
            if popup || (is_ptr && in_bar) || (self.focus == Focus::Find && !is_ptr) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) && in_bar
                {
                    self.set_focus(Focus::Find);
                }
                let a = self.find.on_event(&ev);
                self.find_action(a);
                // 편집 메뉴의 Copy/Cut/Paste는 호스트가 OS 클립보드로 잇는다(편집기와 같은 경로).
                if let Some(act) = self.find.take_edit_ctx() {
                    self.clip_action(act);
                }
                self.redraw();
                if popup || !matches!(ev, InputEvent::MouseMove { .. }) {
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
        // 파일 검색 패널(T-81a) — 마우스는 커서 아래 · 키는 포커스일 때.
        if self.search.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let inside = self.search.bounds().contains(cur);
            if (is_mouse && inside) || (is_wheel_ev(&ev) && inside) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) {
                    self.set_focus(Focus::Search);
                }
                if self.search.on_event(&ev) {
                    self.redraw();
                }
                if self.search.take_request() {
                    self.start_search();
                }
                if let Some(req) = self.search.take_open() {
                    self.open_search_result(req);
                }
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return;
                }
            } else if self.focus == Focus::Search
                && matches!(
                    ev,
                    InputEvent::Key { .. }
                        | InputEvent::Char { .. }
                        | InputEvent::SelectAll
                        | InputEvent::Undo
                        | InputEvent::Redo
                )
            {
                if matches!(
                    ev,
                    InputEvent::Key {
                        key: CtlKey::Escape,
                        ..
                    }
                ) && !self.search.searching()
                {
                    self.set_focus(Focus::Editor);
                    self.redraw();
                    return;
                }
                if self.search.on_event(&ev) {
                    self.redraw();
                }
                if self.search.take_request() {
                    self.start_search();
                }
                if let Some(req) = self.search.take_open() {
                    self.open_search_result(req);
                }
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
            if let Some(i) = self.editors.take_tx_close_request() {
                self.close_tab_guarded(i);
            }
            if let Some(req) = self.editors.take_tab_menu_request() {
                self.tab_menu_request(req);
            }
            self.set_focus(Focus::Editor);
            self.redraw();
            return;
        }
        // 결과 탭 바·우클릭 메뉴(T-93) — 커서 아래일 때만(마우스 라우팅 규칙).
        {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            if let Some(act) = self.panel.route(&ev, cur) {
                if let Some(a) = act {
                    self.panel_action(a);
                }
                self.redraw();
                return;
            }
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
                // 포커스가 없어도 스크롤바 드래그가 끝에 닿으면 자동 페치 요청이 생긴다(09-16).
                self.after_grid_event();
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
            // ★ 휠은 포커스와 무관하게 오므로 여기서도 요청(스크롤 끝 자동 페치 · 09-16: 휠로는 안 됐다).
            self.after_grid_event();
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
                Focus::Explorer | Focus::Find | Focus::Search => {}
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
        self.editors.set_default_eol(eol::default_eol(
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
        // 툴바 배치 복원(설정 `toolbar.layout` · 플로팅 창은 about_to_wait에서 생성).
        self.apply_tool_layout_setting();
        // 데모(사용자 09-17): 'Demo' 프로필·파일이 있으면 메뉴 비활성 · 없고 아직 안 물었으면 최초 1회 팝업.
        self.demo_ready = Self::demo_exists();
        self.rebuild_menus();
        if !self.demo_ready && !self.settings.flag("demo.prompted") {
            let _ = self.settings.set("demo.prompted", "on");
            self.persist_settings();
            self.pending_demo_prompt = true;
        }
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, _ev: Wake) {
        self.drain_events();
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if std::mem::take(&mut self.pending_demo_prompt) {
            self.open_demo_prompt();
        }
        if let Some(rx) = self.demo_job.as_ref() {
            if let Ok(r) = rx.try_recv() {
                self.demo_job = None;
                self.finish_demo(r);
            }
        }
        // 툴바 떼어 내기 요청 → 플로팅 창(창 생성은 이벤트 루프 핸들이 있는 여기서) · 바뀐 배치는 한 번에 저장.
        if !self.pending_float.is_empty() {
            let reqs = std::mem::take(&mut self.pending_float);
            for (gid, at) in reqs {
                self.open_float(el, &gid, at);
            }
        }
        if self.tool_layout_dirty {
            self.save_tool_layout();
        }
        // 캐럿 깜빡임 — 0.5초 타이머가 **실제로 만료됐을 때만** 다시 그린다.
        // ★ 매 호출마다 request_redraw를 하면 그리기 → about_to_wait → 그리기의 무한 루프가 되어
        //   유휴 CPU 한 코어 100% · 키 입력이 프레임당 하나씩만 처리되는 지연(글자 14개에 3초 ·
        //   옛 결과가 화면에 남음)이 생긴다(09-13 Windows 실기 계측).
        let now = Instant::now();
        if now >= self.next_blink {
            // 캐럿 깜빡임 끔(`editor.caret_blink` · 향상 모드) = 타이머 깨움 없음(캐럿은 켜진 채).
            self.next_blink = if self.settings.flag("editor.caret_blink") {
                now + Duration::from_millis(500)
            } else {
                now + Duration::from_secs(3600)
            };
            if self.focus == Focus::Editor {
                self.redraw();
            }
        }
        // 오버레이 스크롤바 페이드(편집기·그리드·로그 창) — 보이는 동안만 ≈30ms 타이머.
        let now_ms = self.started.elapsed().as_millis() as u64;
        self.tx_tick();
        let mut redraw = self.ed_mut().tick(now_ms);
        redraw |= self.grid.tick(now_ms);
        redraw |= self.git.poll();
        // 잠든 결과 탭의 텍스트 변환도 이어서 거둔다(다른 탭에서 완성 · 09-16) — 그리지는 않는다.
        for g in self.sleeping_grids_mut() {
            let _ = g.tick(now_ms);
        }
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
        {
            let (rd, next) = self.run_toast.tick(Instant::now());
            self.run_toast_next = next;
            if rd {
                self.redraw();
            }
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
        if self.search.tick(now_ms) || self.search.poll() {
            self.redraw();
        }
        if self.search.take_request() {
            self.start_search();
        }
        if let Some(req) = self.search.take_open() {
            self.open_search_result(req);
        }
        if self.find.tick(now_ms) {
            self.redraw();
        }
        if self.conn_win.tick_bars(now_ms) {
            self.conn_win.redraw();
        }
        let bars_live = self.ed_mut().scrollbars_visible()
            || self.grid.bars_visible()
            || self.grid.text_pending()
            || self.git.pending()
            || self.sleeping_grids().any(|g| g.text_pending())
            || self.panel.menu_open()
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
            || self.search.animating()
            || self.editors.tooltip_pending()
            || self.toasts.animating();
        // 애니메이션 프레임 간격 = 1000 / `ui.max_fps`(60 = 16ms · 30 = 33ms · 15 = 66ms · 향상 모드 30).
        let frame_ms = (1000 / self.settings.int("ui.max_fps").clamp(5, 240)).max(4) as u64;
        let mut next = if bars_live {
            self.next_blink.min(now + Duration::from_millis(frame_ms))
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
        if let Some(t) = self.run_toast_next {
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
            let ui_px = self.settings.font_px("ui.font_size");
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
            let ui_px = self.settings.font_px("ui.font_size");
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
            let ui_px = self.settings.font_px("ui.font_size");
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
                PrefsAction::OpenColors(key) => {
                    // hover/pressed는 기본 모드 · 그 밖의 `*_color` 키는 그 키 하나를 고르는 모드(09-17).
                    match nsql_settings::entry(&key) {
                        Some(e) if !matches!(e.key, "ui.hover_color" | "ui.pressed_color") => {
                            let v = color_setting(&self.settings, e.key);
                            self.colors_win.set_key_mode(e.key, v, &self.theme);
                        }
                        _ => self.colors_win.set_default_mode(&self.theme),
                    }
                    self.open_colors = true;
                }
                PrefsAction::OpenKeys => self.open_keys = true,
                PrefsAction::EditJson => self.edit_settings_json(),
                PrefsAction::None => {}
            }
            return;
        }
        if self.keys_win.is(id) {
            let ui_px = self.settings.font_px("ui.font_size");
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
            let ui_px = self.settings.font_px("ui.font_size");
            match self.colors_win.handle(&event, &self.theme) {
                ColorsAction::Paint => self.colors_win.paint(&self.ui_font, &self.theme, ui_px),
                ColorsAction::Changed { target, hex } => {
                    apply_color(target, Some(&hex));
                    let _ = self.settings.set(target.key(), &hex);
                    if let ColorTarget::Key(k) = target {
                        self.apply_setting(k);
                        self.prefs_win.refresh(&self.settings);
                        self.prefs_win.redraw();
                    }
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
                    if let Some(k) = self.colors_win.key_mode_key() {
                        let _ = self.settings.reset(k);
                        self.apply_setting(k);
                        self.prefs_win.refresh(&self.settings);
                        self.prefs_win.redraw();
                    } else {
                        for tg in ColorTarget::ALL {
                            apply_color(tg, None);
                            let _ = self.settings.reset(tg.key());
                        }
                    }
                    let _ = self.settings.save();
                    self.redraw();
                    self.conn_win.redraw();
                }
                ColorsAction::None => {}
            }
            return;
        }
        if let Some(fi) = self.tool_floats.iter().position(|f| f.is(id)) {
            let gid = self.tool_floats[fi].group.clone();
            match self.tool_floats[fi].handle(&event) {
                FloatAction::Paint => {
                    if let Some(bar) = self.tool_dock.bar_mut(&gid) {
                        self.tool_floats[fi].paint(bar, &self.ui_font, &self.theme);
                    }
                }
                FloatAction::Input(ev) => {
                    let client = self.tool_floats[fi].client();
                    let sc = self.tool_floats[fi].scale();
                    let mut inv = Invalidations::default();
                    let mut clicked = None;
                    if let Some(bar) = self.tool_dock.bar_mut(&gid) {
                        bar.set_scale(sc);
                        bar.set_bounds(client, &mut inv);
                        bar.on_event(&ev, &mut inv);
                        clicked = bar.take_clicked();
                    }
                    if matches!(ev, InputEvent::RightDown { .. }) {
                        // 플로팅 창 우클릭 = 붙이기(한 번의 입력으로 · 팝업 규칙).
                        self.dock_group(&gid);
                        return;
                    }
                    if !inv.is_empty() {
                        self.tool_floats[fi].redraw();
                    }
                    if let Some(cmd) = clicked {
                        self.menu_action(&cmd);
                        self.redraw();
                    }
                }
                FloatAction::Moved(x, y) => {
                    self.tool_dock.set_floating_pos(&gid, x, y);
                    self.drain_dock_actions();
                }
                FloatAction::Close => self.dock_group(&gid),
                FloatAction::None => {}
            }
            return;
        }
        if self.txlog_win.is(id) {
            match self.txlog_win.handle(&event) {
                TxLogAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    self.txlog_win.set_active_editor(self.editors.active_id());
                    self.txlog_win
                        .paint(&self.txlog, &self.ui_font, &self.theme, ui_px);
                }
                TxLogAction::CopySql(eid) => {
                    if let Some(e) = self.txlog.entry(eid) {
                        if !clipboard::write_text(&e.text) {
                            self.status = t(Msg::ErrClipboard).into();
                        }
                    }
                }
                TxLogAction::OpenSql(eid) => {
                    if let Some(text) = self.txlog.entry(eid).map(|e| e.text.clone()) {
                        self.editors.new_tab(None);
                        self.editors.cur_mut().set_text(&text);
                        self.set_focus(Focus::Editor);
                        if let Some(w) = &self.window {
                            w.focus_window();
                        }
                        self.redraw();
                    }
                }
                TxLogAction::None => {}
            }
            return;
        }
        if self.log_win.is(id) {
            match self.log_win.handle(&event) {
                LogWinAction::Paint => {
                    // 시스템 UI 글꼴 · 본문 = 편집기 기본 크기 · 푸터 = 메인 상태줄 크기(사용자 09-16).
                    let body_px = self.settings.font_px("editor.font_size");
                    let footer_px = nexa_ctl::theme::FontPrefs::default().status.size;
                    self.log_win
                        .paint(&self.ui_font, &self.theme, body_px, footer_px);
                }
                LogWinAction::Toggled(key, on) => {
                    // 스위치 = 설정과 같은 값(자동 기억 · 설정 창에도 반영).
                    let _ = self.settings.set(key, if on { "on" } else { "off" });
                    let _ = self.settings.save();
                    if key == "log.dev_mode" {
                        self.apply_detail_mask();
                    }
                }
                LogWinAction::Setting(key, value) => {
                    let _ = self.settings.set(key, &value);
                    let _ = self.settings.save();
                    if key == "log.dev_layers" {
                        self.apply_detail_mask();
                    }
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
                if !self.tx_pending.is_empty() {
                    self.request_exit();
                    self.redraw();
                    return;
                }
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
                // macOS Control(⌘와 별개 · Sublime `ctrl+cmd+g`) — 다른 OS에선 늘 false.
                self.ctrl_mac = cfg!(target_os = "macos") && m.state().control_key();
                self.alt = m.state().alt_key();
                self.ctrl_raw = m.state().control_key();
                // ★ 열(블록) 선택 모드(Sublime · 사용자 09-15/09-17 OS별): Windows Alt+Shift · macOS Option · Linux는 우클릭 쪽에서.
                let col = match self.column_rule() {
                    "alt_shift" => self.alt && self.shift,
                    "alt" => self.alt,
                    _ => self.col_right_drag,
                };
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
                    self.ctrl_mac,
                ) {
                    // 2단 코드의 둘째 키(`Ctrl+K, Ctrl+U` · 09-16) — 없는 조합이면 안내만.
                    if let Some(first) = self.pending_chord.take() {
                        match self.keymap.lookup_seq(&first, &ch) {
                            // 자동 반복(키를 누르고 있음)은 반복해도 되는 명령만(사용자 09-17 Ctrl+T 80개).
                            Some(id) if kev.repeat && !keymap::repeatable(id) => {}
                            Some(id) => self.key_command(id, el),
                            None => {
                                self.status = tf(
                                    Msg::StChordUnbound,
                                    &[&format!("{}, {}", first.display(), ch.display())],
                                );
                                self.redraw();
                            }
                        }
                        return;
                    }
                    let plain_char =
                        !ch.primary && !ch.alt && !ch.ctrl && ch.key.chars().count() == 1;
                    if !plain_char {
                        // `find.*`는 찾기 패널에 포커스일 때만(그 밖에선 가로채지 않는다 — mac Alt+글자 입력 보존).
                        if let Some(id) = self.keymap.lookup(&ch) {
                            if id.starts_with("find.") {
                                if self.focus == Focus::Find && self.find.is_visible() {
                                    let a = self.find.command(id);
                                    self.find_action(a);
                                    self.redraw();
                                    return;
                                }
                                if !cfg!(target_os = "macos") {
                                    return;
                                }
                            }
                        }
                        if self.keymap.is_prefix(&ch) {
                            self.status = tf(Msg::StChordPending, &[&ch.display()]);
                            self.pending_chord = Some(ch);
                            self.redraw();
                            return;
                        }
                        if let Some(id) = self.keymap.lookup(&ch) {
                            // ★ 자동 반복 사건은 편집·이동 명령만 실행(사용자 09-17: Ctrl+T를 누르고 있자 탭 80개 · 릴리스 뒤에도
                            //   밀린 사건이 계속 처리돼 UI가 막혔다). 한 번짜리 명령의 반복은 여기서 즉시 버린다.
                            if kev.repeat && !keymap::repeatable(id) {
                                return;
                            }
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
        if let Some(ev) = self.ctl_event(&event) {
            self.route(ev);
        }
        if std::mem::take(&mut self.toggle_log) {
            self.toggle_log_window(el);
        }
        if std::mem::take(&mut self.open_txlog) {
            self.open_txlog_window(el);
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

/// 설정의 색 값 → (색, 알파) — `#RRGGBB` = (색, None) · `#RRGGBBAA` = (색, Some(AA/255)) · 빈 값/오류 = (None, None).
fn color_alpha_setting(settings: &Settings, key: &str) -> (Option<nexa_ctl::Color>, Option<f32>) {
    let v = settings.get(key).unwrap_or("").trim();
    let Some(rgba) = nexa_ctl::rgba_from_hex(v) else {
        return (None, None);
    };
    let color = Some(nexa_ctl::Color(rgba >> 8));
    let alpha = (v.trim_start_matches('#').len() == 8).then(|| f32::from(rgba as u8) / 255.0);
    (color, alpha)
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
        ColorTarget::Key(_) => {} // 설정 키는 호스트의 apply_setting이 반영
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
    // 고정폭 = 설정 `editor.font_face`(비면 OS 기본 사슬 · 없는 이름은 fail-over · 사용자 09-17).
    let mono_pref = settings.get("editor.font_face").map(str::to_string);
    let mono = nexa_font::mono_font(mono_pref.as_deref());
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
    if matches!(
        args.first().map(String::as_str),
        Some("--help" | "-h" | "/?")
    ) {
        println!("{}", t(Msg::GuiUsage));
        return;
    }
    // 실행 인자(사용자 09-17): `-c <대상>`/`--connect <대상>`/`<대상>` = 프로필 이름이든 접속 문자열이든 **시작하면서 접속** ·
    //   `--fill <프로필>` = 폼만 채움(종전 동작). 모르는 옵션은 무시.
    let (arg_target, arg_fill_only) = parse_gui_args(&args);
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
    let initial_target = arg_target;
    let profiles = worker::profile_names();
    let mut panel = ConnectPanel::new(nsql_drivers::available());
    // 실행 인자가 프로필 이름이면 폼도 채운다(접속은 아래에서 · `--fill`이면 채우기만).
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
    let txlog_cap = settings.int("txlog.max_entries").max(16) as usize;
    let mut app = App {
        window: None,
        ctx: None,
        surface: None,
        ui_font: ui.font,
        mono_font: mono.font,
        grid_font: load_grid_font(
            settings.get("grid.font_face").unwrap_or(""),
            settings.get("editor.font_face"),
        ),
        theme: initial_theme,
        settings,
        scale: 1.0,
        log_win: LogWin::new(&log_format),
        txlog: TxLog::new(txlog_cap),
        txlog_win: TxLogWin::new(),
        open_txlog: false,
        toasts: toast::Toasts::new(),
        run_toast: runtoast::RunToast::new(),
        run_toast_next: None,
        run_stmt_enabled: true,
        run_stop_enabled: true,
        run_cancel_requested: false,
        run_cancel_drops: false,
        run_line_base: 0,
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
        status_enc_rect: Rect::new(0, 0, 0, 0),
        git: gitstat::GitWatch::new(),
        status_tab_rect: Rect::new(0, 0, 0, 0),
        status_menu: nexa_ctl::controls::ctxmenu::ContextMenu::new(),
        toggle_log: false,
        open_colors: false,
        colors_win,
        keymap,
        pending_chord: None,
        open_keys: false,
        keys_win: KeysWin::new(),
        open_prefs: false,
        prefs_win: PrefsWin::new(),
        act_bar: ActivityBar::new(),
        file_win: FileWin::new(),
        open_file_dlg: None,
        menubar: MenuBar::new(App::build_menus()),
        tabs_menu_sig: String::new(),
        tool_dock: App::build_tool_dock(),
        tool_floats: Vec::new(),
        pending_float: Vec::new(),
        tool_layout_dirty: false,
        demo_ready: false,
        demo_job: None,
        pending_demo_prompt: false,
        conn_win: ConnWin::new(panel),
        open_conn: true,
        find: FindBar::new(),
        find_scope: None,
        editors: Editors::new(ed_line_numbers, ed_multi, ed_tooltip, syntax_reg),
        grid: grid::Grid::default(),
        panel: ResultPanel::new(
            ResultTab {
                id: 0,
                title: String::new(),
                pinned: false,
                named: false,
                grid: grid::Grid::default(),
                seq: 0,
            },
            true,
            false,
        ),
        panel_editor: 0,
        panels: HashMap::new(),
        next_result_id: 1,
        result_area: Rect::new(0, 0, 0, 0),
        key_cache: HashMap::new(),
        sql_wait: None,
        single_run: false,
        view_wait: None,
        offset_warned: std::collections::HashSet::new(),
        frame_trace: std::env::var_os("NSQL_TRACE_FRAMES").map(|_| FrameTrace::default()),
        extensions: extensions::Registry::builtin(),
        ext_catalog: Vec::new(),
        grid_tab: 0,
        run_tab: 0,
        explorer,
        search: SearchPanel::new(),
        split_v: Splitter::new(SplitAxis::Vertical),
        split_h: Splitter::new(SplitAxis::Horizontal),
        last_spec: None,
        dialect: DEFAULT_DIALECT,
        tx_dirty: false,
        tx_read: false,
        tx_pending: Vec::new(),
        tx_stale_logged: false,
        run_editor: 0,
        tx_after: None,
        status_tx_rect: Rect::new(0, 0, 0, 0),
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
        cursor: (0, 0),
        shift: false,
        primary: false,
        alt: false,
        col_right_drag: false,
        ctrl_raw: false,
        ctrl_mac: false,
        started: Instant::now(),
        next_blink: Instant::now(),
        panel_op: None,
        panel_results: HashMap::new(),
    };
    app.grid.set_row_snap(row_snap);
    app.apply_result_tab_opts();
    {
        let pct = app.settings.int("grid.row_height_pct").clamp(110, 300) as i32;
        app.grid.set_row_pct(pct);
    }
    app.editors
        .set_scroll_snap(app.settings.get("editor.scroll") == Some("row"));
    app.apply_minimap();
    // 자원 거버너 상한(T-90d · 부팅 1회) — 실효 값(`perf.mode` 반영).
    app.log_win
        .set_max_lines(app.settings.int("log.max_lines").max(100) as usize);
    app.editors
        .set_undo_max(app.settings.int("editor.undo_max").max(1) as usize);
    nexa_gfx::text::set_glyph_cache_max(app.settings.int("ui.glyph_cache").max(256) as usize);
    nexa_fs::shell::set_icon_cache_max(app.settings.int("file.icon_cache").max(16) as usize);
    // D-58: full 모드인데 배터리/원격 세션이면 1회 안내.
    if let Some(m) = app.settings.perf_hint() {
        app.status = t(m).into();
    }
    let null_text = app
        .settings
        .get("grid.null_text")
        .unwrap_or("NULL")
        .to_string();
    app.grid.set_null_text(&null_text);
    nexa_ctl::controls::set_menu_icons(app.settings.flag("ui.menu_icons"));
    app.log_win
        .set_on_top(app.settings.flag("log.always_on_top"));
    app.editors.set_rulers(rulers);
    app.conn_win
        .set_probe(probe_hub, probe_policy(&app.settings));
    app.editors.set_whitespace(ws_style);
    app.log_win.set_row_snap(row_snap);
    app.toasts.configure(
        app.settings.int("ui.toast_secs"),
        app.settings.int("ui.toast_alpha"),
    );
    app.apply_text_render();
    app.apply_toolbar_visibility();
    app.git
        .set_interval(app.settings.int("statusbar.git_secs").max(2) as u64);
    app.apply_ruler_style();
    app.apply_occurrence_style();
    app.apply_run_toast();
    app.apply_detail_mask();
    app.editors
        .set_diff_marks(app.settings.flag("editor.diff_marks"));
    app.sync_run_stmt_button();
    nsql_drivers::set_mssql_encryption(app.settings.get("mssql.encrypt") == Some("login"));
    nsql_drivers::set_mssql_cancel_socket(app.settings.get("mssql.cancel") == Some("socket"));
    app.apply_tab_accent();
    app.apply_extensions(None);
    app.editors
        .set_text_inset(app.settings.int("editor.text_pad_left").clamp(0, 32) as i32);
    app.grid.set_default_page_rows(max_rows);
    app.grid.set_col_limits(
        app.settings.int("grid.col_min_width") as i32,
        app.settings.grid_col_max_chars() as i32,
    );
    app.grid
        .set_auto_fetch(app.settings.flag("grid.auto_fetch"));
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
    app.conn_win.set_tuning(conn_tuning(&app.settings));
    nexa_ctl::tokens::set_intent_ms(app.settings.int("ui.hover_intent_ms").clamp(0, 500) as u64);
    nexa_ctl::tokens::set_fade_out_ms(fade_ms(&app.settings, "ui.fade_out_ms", 2000));
    nexa_fs::shell::set_os_icons(app.settings.flag("file.os_icons"));
    nexa_dlg::set_probe_chevrons(app.settings.flag("file.probe_chevrons"));
    // hover / 눌림 색(설정 · `#RRGGBBAA` · 비우면 테마 기본).
    for tg in ColorTarget::ALL {
        apply_color(tg, color_setting(&app.settings, tg.key()).as_deref());
    }
    // 페이드 속도 속성 두 단(Fast/Slow)의 실제 ms — 컨트롤은 속도 이름만 알고 여기서 값이 연계된다(사용자 09-14).
    nexa_ctl::tokens::set_fade_ms(
        nexa_ctl::tokens::FadeSpeed::Fast,
        fade_ms(&app.settings, "ui.fade_fast", 5000),
    );
    nexa_ctl::tokens::set_fade_ms(
        nexa_ctl::tokens::FadeSpeed::Slow,
        fade_ms(&app.settings, "ui.fade_slow", 5000),
    );
    // ★ 실행 인자 접속(사용자 09-17): 프로필 이름이든 접속 문자열이든 스펙으로 풀어 **Connect 버튼과 같은 경로**(`last_spec` →
    //   접속 뒤 탐색기도 붙는다 · T-104 해결). 스펙으로 못 풀면 종전 `Cmd::Connect(문자열)`.
    if let Some(target) = initial_target.filter(|_| !arg_fill_only) {
        let spec = if nsql_vault::is_profile_name(&target) {
            match Vault::open_default().and_then(|v| v.get(&target)) {
                Ok(Some(spec)) => Some(spec),
                _ => {
                    app.status = tf(Msg::StArgProfileMissing, &[&target]);
                    None
                }
            }
        } else {
            nsql_drivers::parse_target(&target, DEFAULT_DIALECT).ok()
        };
        match spec {
            Some(spec) => {
                app.busy = true;
                app.status = tf(Msg::StConnecting, &[&spec.redacted()]);
                app.last_spec = Some(spec.clone());
                if nsql_vault::is_profile_name(&target) {
                    app.conn_win.select_by_name(&target);
                }
                app.worker.send(worker::Cmd::ConnectSpec {
                    spec,
                    reconnect_same: false,
                });
            }
            None if !nsql_vault::is_profile_name(&target) => {
                app.busy = true;
                app.status = tf(Msg::StConnecting, &[&target]);
                app.worker.send(worker::Cmd::Connect(target));
            }
            None => {}
        }
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

/// 결과 글꼴(설정 `grid.font_face`): 비면 None(= UI 글꼴) · `mono` = 편집기 고정폭 · 그 외 = 글꼴 이름(못 찾으면 시스템 UI 본이 첫 폴백).
fn load_grid_font(face: &str, mono_pref: Option<&str>) -> Option<Font> {
    let face = face.trim();
    if face.is_empty() {
        // Windows: 시스템 UI 체인은 맑은 고딕이 먼저라(x-높이 작고 획이 가늘다) 작고 연하게 보였다(사용자 09-16) →
        // 결과 글꼴 기본 = **Calibri 10pt(13px)**(사용자 확인 09-16 · Golden 결과 그리드) · 없으면 Segoe UI · 한글은 맑은 고딕 폴백.
        if cfg!(target_os = "windows") {
            return nexa_font::ui_font(Some("Calibri"))
                .or_else(|| nexa_font::ui_font(Some("Segoe UI")))
                .map(|l| l.font);
        }
        return None;
    }
    if face.eq_ignore_ascii_case("mono") {
        return nexa_font::mono_font(mono_pref).map(|l| l.font);
    }
    nexa_font::ui_font(Some(face)).map(|l| l.font)
}

/// 툴바 버튼(id · 라벨 = 툴팁 문구) — 표시 여부 메뉴·설정 `toolbar.hidden`의 원천(순서는 `build_toolbar`와 같다).
const TOOLBAR_ITEMS: &[(&str, Msg)] = &[
    ("file.new", Msg::TipNew),
    ("file.open", Msg::TipOpen),
    ("file.save", Msg::TipSave),
    ("file.save_as", Msg::TipSaveAs),
    ("run.statement", Msg::TipRunStatement),
    ("run.stop", Msg::TipRunStop),
    ("run.all", Msg::TipRunAll),
    ("run.commit", Msg::TipCommit),
    ("run.rollback", Msg::TipRollback),
    ("tx.log", Msg::TipTxLog),
    ("conn.toggle", Msg::TipConnect),
    ("conn.disconnect", Msg::TipDisconnect),
    ("view.log", Msg::TipLog),
];

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
        .unwrap_or("·→$")
        .chars()
        .collect();
    let pick = |i: usize, d: char| match chars.get(i) {
        Some('_') | None => d,
        Some(c) => *c,
    };
    let (color, hex_alpha) = color_alpha_setting(settings, "editor.whitespace_color");
    WhitespaceStyle {
        mode,
        space: pick(0, '\0'),
        tab: pick(1, '\0'),
        eol: pick(2, '\0'),
        color,
        // `#RRGGBBAA`의 AA가 있으면 `editor.whitespace_alpha`보다 우선(09-17).
        alpha: hex_alpha
            .unwrap_or((settings.int("editor.whitespace_alpha") as f32 / 100.0).clamp(0.0, 1.0)),
    }
}

/// 페이드·슬라이드 ms — 애니메이션 마스터(`ui.animations` · auto = OS 동작 줄이기)가 꺼져 있으면 0.
fn fade_ms(settings: &Settings, key: &str, max: i64) -> u32 {
    if settings.animations_enabled() {
        settings.int(key).clamp(0, max) as u32
    } else {
        0
    }
}

/// 신호등 정책(설정 `probe.*` · 실효 값 = 향상 모드 반영).
fn probe_policy(settings: &Settings) -> probe::ProbePolicy {
    let secs = |k: &str, min: i64| Duration::from_secs(settings.int(k).max(min) as u64);
    probe::ProbePolicy {
        enabled: settings.flag("probe.enabled"),
        max_retries: settings.int("probe.max_retries").max(0) as u32,
        timeout: secs("probe.timeout", 1),
        retry_delay: secs("probe.retry_delay", 1),
        interval: secs("probe.interval", 5),
        icmp: settings.flag("probe.icmp"),
    }
}

/// 접속 창 조정값(비노출 설정 · 사용자 09-14 "구현 값은 설정으로") — 슬라이드는 애니메이션 마스터를 따른다.
fn conn_tuning(settings: &Settings) -> conn_win::ConnTuning {
    let i = |k: &str| settings.int(k);
    conn_win::ConnTuning {
        delete_confirm_ms: i("conn.delete_confirm_ms").max(0) as u64,
        close_after_ms: i("conn.close_after_connect_ms").max(0) as u64,
        tooltip_ms: i("ui.tooltip_delay_ms").max(0) as u128,
        dblclick_ms: i("ui.dblclick_ms").max(0) as u128,
        slide_ms: fade_ms(settings, "ui.slide_ms", 1000) as f32,
        window_w: i("conn.window_w").max(400) as f32,
        window_h: i("conn.window_h").max(300) as f32,
        panel_w: i("conn.panel_w").max(200) as f32,
        button_scale: i("conn.button_scale_pct").clamp(100, 250) as f32 / 100.0,
        port_w: i("conn.port_w").max(40) as f32,
    }
}

/// 내장 데모 스크립트(`examples/demo.sql` · SQLite · dept/emp + sales 5,000행) — 배포본에 파일을 따로 두지 않는다(DR-27).
const DEMO_SQL: &str = include_str!("../../../examples/demo.sql");

/// `path`에 데모 DB를 만든다(파일이 이미 있으면 스크립트를 건너뛰고 그대로 쓴다). 실패하면 만들다 만 파일을 지운다.
fn create_demo_db(path: &Path) -> Result<String, String> {
    let shown = path.display().to_string();
    if path.exists() {
        return Ok(shown);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let target = format!("sqlite:{shown}");
    let spec = nsql_drivers::parse_target(&target, Dialect::Sqlite)?;
    let session = nsql_drivers::open(&spec, Dialect::Sqlite).map_err(|e| e.message)?;
    let opener: nsql_run::Opener = Box::new(|_| {
        Err(nsql_core::DbError {
            code: None,
            message: "demo: no reconnect".into(),
            position: None,
        })
    });
    let mut runner = nsql_run::Runner::new(Dialect::Sqlite, opener).with_session(session, "demo");
    let mut errors: Vec<String> = Vec::new();
    let mut prompt = |_: &str| Some(String::new());
    let n = runner.run_script(DEMO_SQL, &mut prompt, &mut |e: nsql_run::RunEvent| {
        if let nsql_run::RunEvent::Error { error, line, .. } = e {
            errors.push(format!("line {line}: {}", error.message));
        }
    });
    drop(runner);
    if n > 0 {
        let _ = std::fs::remove_file(path);
        return Err(errors.join(" · "));
    }
    Ok(shown)
}

/// GUI 실행 인자 → (접속 대상, 폼만 채우기). `-c/--connect <대상>` · `--fill <프로필>` · 맨 앞 맨 인자 = 대상.
fn parse_gui_args(args: &[String]) -> (Option<String>, bool) {
    let mut target: Option<String> = None;
    let mut fill_only = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-c" | "--connect" => {
                target = args.get(i + 1).cloned();
                i += 1;
            }
            "--fill" => {
                target = args.get(i + 1).cloned();
                fill_only = true;
                i += 1;
            }
            a if a.starts_with('-') => {}
            a => {
                if target.is_none() {
                    target = Some(a.to_string());
                }
            }
        }
        i += 1;
    }
    (target, fill_only)
}

#[cfg(test)]
mod arg_tests {
    use super::*;

    #[test]
    fn gui_args_pick_target_and_fill_mode() {
        let a = |v: &[&str]| parse_gui_args(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(a(&[]), (None, false));
        assert_eq!(a(&["Demo"]), (Some("Demo".into()), false));
        assert_eq!(a(&["-c", "Demo"]), (Some("Demo".into()), false));
        assert_eq!(
            a(&["--connect", "sqlite:x.db"]),
            (Some("sqlite:x.db".into()), false)
        );
        assert_eq!(a(&["--fill", "Demo"]), (Some("Demo".into()), true));
        assert_eq!(a(&["--unknown", "Demo"]), (Some("Demo".into()), false));
    }

    /// 설정 `editor.whitespace = all`이 편집기 스타일 All로 풀린다(사용자 09-17 "전체로 바꿔도 안 바뀜" 진단).
    #[test]
    fn whitespace_style_follows_setting() {
        let dir = std::env::temp_dir().join(format!("nsql-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Settings::open(dir.join("settings.conf"));
        assert_eq!(
            whitespace_style(&s).mode,
            nexa_ctl::WhitespaceMode::Selection
        );
        s.set("editor.whitespace", "all").expect("set");
        let ws = whitespace_style(&s);
        assert_eq!(ws.mode, nexa_ctl::WhitespaceMode::All);
        assert_eq!((ws.space, ws.tab, ws.eol), ('·', '→', '$'));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
