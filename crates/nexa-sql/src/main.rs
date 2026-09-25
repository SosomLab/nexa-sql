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
mod backups;
mod bookmarks;
mod bookmarks_panel;
mod clipboard;
mod colors_win;
mod conn_win;
mod connect;
mod copybtn;
mod dbms_icons;
mod editors;
mod enc;
mod eol;
mod exp_icons;
mod explorer;
mod explorers;
mod ext_panel;
mod ext_view;
#[allow(dead_code)]
// 09-17 레인보우 플러그인 모듈 · 배선(설정→편집기 · 키맵 · 메뉴)은 다음 세션(T-119)
mod extensions;
mod extfile;
mod file_win;
mod fileload;
mod filterbar;
mod findbar;
mod gitstat;
mod grid;
mod icon;
mod imehint;
mod imestate;
mod input;
mod input_win;
mod intel;
mod intel_card;
mod keymap;
mod keys_win;
mod log_win;
mod mem_win;
mod memstat;
mod memtrim;
mod metacache;
mod outline_panel;
mod palette;
mod parwalk;
mod prefs_win;
mod present;
mod probe;
mod project;
mod project_panel;
mod results;
mod runtoast;
mod rx;
mod search_history;
mod search_panel;
mod sessions;
mod sessions_win;
mod sqlprev_win;
mod syntax;
mod theme;
mod toast;
mod toolfloat;
mod toolicons;
mod txlog_win;
mod txwarn;
mod undofile;
mod vars_win;
mod varsfile;
mod winfocus;
mod wingeom;
mod worker;

use activity::ActivityBar;
use colors_win::{ColorTarget, ColorsAction, ColorsWin};
use conn_win::{ConnWin, ConnWinAction, ConnectMark, TestMark};
use connect::{ConnState, ConnectPanel, PanelAction};
use editors::Editors;
use explorer::{ExplorerAction, LiveReq};
use ext_panel::{ExtPanel, ExtPanelAction, ExtRow};
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
use nsql_run::txlog::{Purpose as TxPurpose, TxOutcome};
use nsql_run::RunEvent;
use nsql_script::ConnectSpec;
use nsql_settings::{Settings, ThemeMode};
use nsql_vault::Vault;
use palette::{Palette, PaletteAction};
use prefs_win::{PrefsAction, PrefsWin};
use results::{PanelAction as ResultAction, ResultPanel, ResultTab};
use search_panel::{SearchCtx, SearchPanel};
use sessions_win::{SessRow, SessWinAction, SessionsWin};
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
use winit::event::{ElementState, Ime, KeyEvent, WindowEvent};
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
    /// 확장 패널(검색 상자 · 사용자 09-19).
    Ext,
    /// 프로젝트 탐색기(docs/67 · 사용자 09-22).
    Project,
    /// 북마크 패널(docs/69 · 사용자 09-22).
    Bookmarks,
    /// 아웃라인 패널(docs/76 · 사용자 09-23).
    Outline,
}

/// 저장소 읽기 스레드의 결과 한 벌(원천 · index · 추적 줄).
type ExtFetch = Vec<(
    extensions::manager::Source,
    Result<extensions::manager::Index, String>,
    extensions::manager::Trace,
)>;

/// 잃는 순간의 확인(Commit/Rollback) 뒤 이어질 동작(DR-30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TxAfter {
    CloseTab(usize),
    Disconnect,
    Exit,
    SwitchAuto,
}

use sessions::{ConnectIntent, Sess, SessionMode, TxItem, SHARED};

struct App {
    window: Option<Rc<Window>>,
    surface: Option<present::Presenter>,
    /// ★ 편집기 탭별 변수 표(탭 층 · D-135 · docs/63) — 실행마다 워커에 넘기고 `RunEvent::Vars`로 돌려받는다. 탭을 닫으면 버린다.
    tab_vars: std::collections::HashMap<u64, Vec<nsql_script::VarState>>,
    /// ★ 글로벌 변수 층(docs/63 §11 · 앱 전역 · `vars/global.sql` 보존 · 모든 세션에 전파) — 단일 원천.
    global_vars: Vec<nsql_script::VarState>,
    /// 오래된 변수 보존 파일 정리를 이번 실행에서 했는가(처음 쓸 때 1회).
    vars_pruned: bool,
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
    txlog_win: TxLogWin,
    /// 세션 창(서버별 전체 세션 · 사용자 09-18) — 트랜잭션 로그 창과 같은 골격.
    sessions_win: SessionsWin,
    /// 메모리 맵 창(docs/80 · 모델리스 · 닫혀 있으면 비용 0).
    mem_win: mem_win::MemWin,
    /// 변수 값 입력 창(D-137) · 열기를 기다리는 요청(세션 id, 빠진 입력) — 창은 이벤트 루프가 있을 때(`about_to_wait`) 만든다.
    input_win: input_win::InputWin,
    /// SQL Preview 모달(탐색기 ▸ Generate SQL · docs/83 §4).
    sqlprev_win: sqlprev_win::SqlPrevWin,
    /// 열 차례인 미리보기(사건 루프 밖에서 온 탐색기 응답 → `el`이 있는 자리에서 연다).
    sqlprev_pending: Option<(
        nsql_catalog::GenSpec,
        Result<String, String>,
        Option<ConnectSpec>,
    )>,
    input_pending: Option<(u64, Vec<nsql_script::InputNeed>)>,
    /// 워커가 비밀번호를 묻는다(세션 id · 가린 접속 문자열) — 입력 창은 이벤트 루프에서 연다.
    pw_pending: Option<(u64, String, bool)>,
    /// "저장하고 닫기"를 고른 탭(id) — 저장이 끝나(저장 창 포함) 더는 바뀐 것이 없으면 닫는다 · 저장을 취소·실패하면 닫지 않는다.
    close_after_save: Option<u64>,
    /// ★ **닫힌 창의 키가 메인 창으로 새지 않게**(사용자 09-21 — 비밀번호 창의 Enter가 편집기에 줄바꿈을 넣었다): 입력 창을
    /// 키로 닫은 시각. 그 키가 **떼어질 때까지** 메인 창에 오는 자동 반복 누름을 버린다([`key_guard_step`]).
    key_guard: Option<Instant>,
    /// **일회성 비밀번호의 둘째 사본**(세션 id · 값 · 받은 시각): 같은 서버의 탐색기 메타 세션을 붙일 때 한 번 쓰고 지운다.
    /// 접속이 끝나 탐색기를 붙이는 순간 소비되고, 그러지 못했으면 20초 뒤에 버린다(버려지면서 0으로 덮어쓴다).
    pw_once: Option<(u64, nsql_core::Secret, Instant)>,
    /// 변수 창(docs/63 V2) · 마지막 실행에서 바뀐 이름(탭 id, 대문자 이름들 — 그 탭의 줄을 강조).
    vars_win: vars_win::VarsWin,
    vars_changed: (u64, std::collections::HashSet<String>),
    open_sessions: bool,
    open_mem: bool,
    /// 변수 창을 다음 틱에 연다(메뉴·팔레트·기동 명령은 이벤트 루프 핸들이 없다).
    open_vars: bool,
    open_txlog: bool,
    /// 우측 하단 토스트(오류 분류 · docs/42).
    toasts: toast::Toasts,
    /// 유휴 미커밋 경고 카드(docs/56 L2) — 세션 하나를 가리킨다(가장 급한 것).
    tx_warn: txwarn::TxWarn,
    /// 다음 미커밋 점검 시각(about_to_wait 깨움).
    tx_guard_next: Instant,
    run_toast_next: Option<Instant>,
    /// 문장 실행 버튼 활성 상태 캐시(다중 커서면 비활성 · 사용자 09-17).
    run_stmt_enabled: bool,
    /// 툴바 ■(실행 중지) 활성 캐시(= busy · 시작값 true = 첫 동기화에서 비활성으로).
    run_stop_enabled: bool,
    /// 파일 싱크 허브(설정 `log.file` · 배경 스레드 · 비면 None).
    log_hub: Option<nsql_log::LogHub>,
    /// 파일 대화상자의 용도(편집기 열기/저장 · 로그 내보내기).
    file_purpose: FilePurpose,
    /// 메뉴에서 요청한 종료·로그 창 토글(이벤트 루프 핸들이 필요해 window_event 끝에서 처리).
    exit_requested: bool,
    palette: Palette,
    syntax: Rc<SyntaxRegistry>,
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
    /// 거터(북마크/니모닉 영역) 우클릭 메뉴의 대상 — (탭 index, 논리 줄, 그 줄의 북마크 id) · `status_menu`를 빌려 쓴다(사용자 09-23).
    bm_gutter: Option<(usize, usize, Option<u64>)>,
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
    /// Windows 탐색기 타입어헤드 한/영 상태(앱 소유 · nexa-beep docs/27 §8): 탐색기는 IME를 끊어 OS 한/영이 무력하므로
    /// 한/영 키를 앱이 받아 토글하고 라틴 키를 두벌식 자모로 번역한다. mac은 레이아웃이 자모를 주므로 안 쓴다.
    hangul_mode: bool,
    open_keys: bool,
    keys_win: KeysWin,
    /// 환경 설정 창(T-39 · 사용자 09-15).
    open_prefs: bool,
    /// 기동 명령 `edit.prefs:<검색어>` — 설정 창을 열면서 넣을 검색어(자체 캡처용).
    prefs_query: Option<String>,
    prefs_win: PrefsWin,
    /// 좌측 활동 막대(VS Code식 · 사용자 09-15) — 패널 토글 + 동작 버튼.
    act_bar: ActivityBar,
    /// 파일 열기/저장 창(T-74 · 모달) + 열 요청(모드).
    file_win: FileWin,
    open_file_dlg: Option<PickerMode>,
    /// 탭별 치환 변수(`DEFINE` · 이름 · 원문 · 표시값) — 변수 창 행 + 다음 실행에 전달(09-23).
    tab_defines: HashMap<u64, Vec<(String, String, String)>>,
    /// 프로젝트 자동 저장(사용자 09-23): 마지막 저장 시각 · 마지막으로 쓴 JSON(같으면 안 쓴다).
    project_autosave_at: Instant,
    /// 마지막으로 쓴/읽은 프로젝트 **문서 전체**(헤더 JSON + 탭 payload 블록 · `Project::to_document`) — 바뀜 비교용.
    project_last_json: Vec<u8>,
    /// 사건(탭 열기/닫기/전환 · 폴더 변경) 뒤 2초 디바운스 저장(09-23).
    project_touch_at: Option<Instant>,
    /// 종료 흐름(사용자 09-23): 프로젝트 저장 물음 → 미저장 파일 탭마다 물음 → 종료.
    exit_pending: bool,
    exit_project_asked: bool,
    /// 마지막으로 탐색기와 맞춘 활성 탭(바뀌면 `project_sync_active` · 사용자 09-22).
    last_synced_tab: u64,
    /// ★ 다중 열기(사용자 09-22): 확인 팝업이 기다리는 (파일들 · 인코딩) · 진행 중인 순차 적재.
    multi_pending: Option<(Vec<PathBuf>, String)>,
    multi_load: Option<MultiLoad>,
    /// 시작 인자(사용자 09-22): 프로젝트 파일 · 열 파일들 · 첫 인스턴스인가 · 인스턴스 잠금(살아 있는 동안 쥔다).
    arg_project: Option<PathBuf>,
    arg_files: Vec<PathBuf>,
    /// 실행 인자의 작업 폴더(`nexa-sql .` · 사용자 09-23) — 파일 모드의 로컬 상태(북마크)를 `<폴더>/.nsql/`에.
    arg_folder: Option<PathBuf>,
    first_instance: bool,
    _instance_lock: Option<std::fs::File>,
    /// 폴더 고르기의 시작 폴더(설정의 지금 값).
    folder_start: Option<PathBuf>,
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
    /// ★ 큰 선택 복사/잘라내기 확인(`editor.copy_confirm_mb` · docs/72 ②-3): 첫 누름 안내 뒤 3초 안 되풀이 = 실행.
    copy_armed_until: Option<Instant>,
    /// "새 프로젝트 저장"으로 연 저장 창인가 — 지금 프로젝트를 복사하지 않고 **빈 프로젝트 + 저장 폴더**로(사용자 09-22).
    project_new_fresh: bool,
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
    /// `NSQL_TRACE_IME=1` — 키·IME 사건을 stderr로(T-139 진단).
    trace_ime: bool,
    /// 지금 한글을 앱이 조합하는가(`sync_hangul_mode` · None = 아직 안 맞춤).
    hangul_app: Option<bool>,
    /// 계측용: 마지막 입력(키·글자·클릭)이 들어온 시각 — 다음 present 뒤 "입력→화면" 지연을 찍는다.
    input_at: Option<Instant>,
    /// 계측용: 입력 뒤 경로의 지점들(이름 · 시각) — present 뒤 한 줄로 찍는다(출력 자체가 지연을 만들지 않게).
    trace_marks: std::cell::RefCell<Vec<(&'static str, Instant)>>,
    /// 캐럿 깜빡임 위상 원점 — 입력마다 지금으로 되돌려 캐럿이 **움직인 직후엔 항상 켜져** 보이게(Sublime·VS Code · 09-19).
    blink_origin: Instant,
    /// ORDER BY 없는 재질의 경고를 낸 결과 탭(탭당 1회).
    offset_warned: std::collections::HashSet<u64>,
    /// `grid`가 속한 **결과 탭** id.
    /// 확장 레지스트리(in-process · docs/50 §4) + 매니저 팔레트가 고른 후보(설치 목록 · 저장소 목록).
    extensions: extensions::Registry,
    ext_catalog: Vec<(extensions::manager::Source, extensions::manager::Summary)>,
    grid_tab: u64,
    /// ★ 오브젝트 탐색기(사용자 09-15 · docs/28) — 메타 세션은 자기 스레드.
    explorer: explorers::ExplorerSet,
    /// 파일 검색 패널(활동 막대 두 번째 · T-81a · docs/36).
    search: SearchPanel,
    /// 프로젝트 탐색기(docs/67 §4) · 열린 프로젝트(없으면 기본 워크스페이스).
    project_panel: project_panel::ProjectPanel,
    /// 북마크(docs/69 · T-167).
    bookmarks: bookmarks::Bookmarks,
    bm_panel: bookmarks_panel::BookmarksPanel,
    /// 북마크 패널에 마지막으로 준 탭 제목 세대(`Editors::titles_rev`) — 바뀐 틱에만 id → 제목 표를 다시 준다(사용자 09-23).
    bm_titles_rev: u64,
    /// 검색어 이력(전역 한 파일 · 상자 이름별 · `search.history_max` · 사용자 09-23) — 찾기/파일 검색/필터/설정 검색이 공유.
    search_history: search_history::SharedHistory,
    /// 코드 완성(docs/76 · 팝업 · 문서 아웃라인 캐시 · MRU).
    intel: intel::Intel,
    /// 아웃라인 패널(docs/76).
    outline_panel: outline_panel::OutlinePanel,
    project: project::Project,
    /// 확장 패널(활동 막대 "확장" · 확장 관리자가 켜져 있을 때만 · 사용자 09-19).
    ext_panel: ExtPanel,
    /// 저장소 읽기 스레드의 결과(패널을 열거나 ⟳ · 사용자 동작으로만 · 26 §8).
    ext_fetch_rx: Option<std::sync::mpsc::Receiver<ExtFetch>>,
    /// 확장 상세 뷰(편집기 자리에 그리는 전용 페이지) · 뷰 열쇠(`ext:<id>`)별 내용 · 마지막으로 그린 열쇠(바뀌면 스크롤 초기화).
    ext_view: ext_view::ExtView,
    ext_details: HashMap<String, ext_view::ExtDetail>,
    ext_view_key: String,
    /// 탐색기 유휴 워터마크의 다음 시각(docs/57 T2).
    meta_refresh_next: Option<Instant>,
    /// 외부 파일 변경(docs/58): 감시 스레드(처음 쓸 때 만든다) · 탭별 상태 · 확인 띠 · 다음 폴링 · 메인 창 활성 여부 ·
    /// 마지막으로 확인한 활성 탭 · 저장 2단 확인(탭, 시각).
    ext_watch: Option<nexa_fs::watch::StatWatch>,
    ext_files: HashMap<u64, extfile::ExtInfo>,
    ext_banner: extfile::Banner,
    ext_poll_next: Option<Instant>,
    main_active: bool,
    ext_last_tab: u64,
    ext_save_armed: Option<(u64, Instant)>,
    /// 자체 캡처용: 첫 접속 뒤에 실행할 기동 명령(`NSQL_STARTUP_CMD`의 `@connected:` 항목).
    startup_after_connect: Vec<String>,
    startup_connected: bool,
    /// 자체 캡처·측정용: 시각이 되면 실행할 기동 명령(`@after:<ms>:<명령>`).
    startup_timed: Vec<(Instant, String)>,
    /// 파일 적재 스레드의 결과(큰 파일 = 비동기 · docs/59) · 열기 선택을 기다리는 큰 파일(경로, 인코딩, 크기).
    /// 스레드 적재 중인 파일들(앞 = 막에 보이는 것) — 하나라도 있으면 메인 창 입력을 받지 않는다(`fileload.rs`).
    file_loads: Vec<LoadJob>,
    big_pending: Option<(PathBuf, String, u64)>,
    /// 이 실행에서 오래된 되돌리기 기록 파일을 이미 치웠는가(처음 저장할 때 한 번).
    undo_pruned: bool,
    /// 메모리 회수(`memtrim.rs`): 큰 것을 놓은 뒤의 1회 회수 예정 시각 · 다음 주기 회수 · 직전 틱의 탭 수(닫힘 감지).
    mem_trim_due: Option<Instant>,
    mem_trim_next: Option<Instant>,
    mem_last_tabs: usize,
    /// 스플리터 ① 탐색기|편집기(세로선) · ② 편집기|결과(가로선) — 사용자 09-16.
    split_v: Splitter,
    split_h: Splitter,
    /// 확인 뒤 이어질 동작.
    tx_after: Option<TxAfter>,
    status_tx_rect: Rect,
    /// 상태줄 메모리 세그먼트(클릭 = 메모리 맵 창 토글 · docs/80).
    status_mem_rect: Rect,
    /// 상태줄 총량(바이트 · 마지막 조회 시각) — 그릴 때 `mem.status_refresh_ms`보다 오래됐으면 OS 한 번.
    mem_status: (u64, Option<Instant>),
    /// 세그먼트 눌림 표시(MouseDown~MouseUp).
    mem_pressed: bool,
    /// 창이 열려 있을 때 다음 표본 시각.
    mem_next: Instant,
    /// settings.json 감시(경로 · 마지막 수정 시각 · 다음 확인 시각) — JSON 편집을 연 뒤부터 1초 폴링(사용자 09-15).
    json_watch: Option<(std::path::PathBuf, Option<std::time::SystemTime>)>,
    /// 접속 창이 열려 메인 창을 모달로 막고 있는가(사용자 09-15) — 열림/닫힘 전환 때 OS 활성 상태를 맞춘다.
    conn_modal: bool,
    json_next: Instant,
    focus: Focus,
    /// 트랜잭션 로그 — **전 세션 합본**(세션 열 · docs/52 D-105): 기록할 때마다 `select_session(지금 세션)`으로 고른다.
    txlog: nsql_run::txlog::TxLog,
    // ★ 세션 컨텍스트(docs/52): `sess` = **활성 편집기 탭이 쓰는 세션**(워커 + 실행·트랜잭션 상태 전부) · `parked` = 나머지.
    //   활성 탭이 바뀌면 `sync_sess`가 통째로 맞바꾼다. 통제 판정은 `Sess::blocked` 하나.
    sess: Sess,
    parked: Vec<Sess>,
    /// ★ 실행 카드 스택(앱에 하나 · 전 탭·세션 공유 · 사용자 09-22).
    run_toast: runtoast::RunToast,
    next_sess_id: u64,
    /// 탭 → 공유 세션 선택(없으면 `default_shared`). 전용 세션은 `Sess::owner`가 우선한다.
    tab_bind: HashMap<u64, u64>,
    /// 묶이지 않은 탭이 쓰는 공유 세션.
    default_shared: u64,
    /// 개별 모드에서 새 탭이 붙을 기본 접속 정보(접속 창에서 마지막으로 성공한 스펙 = 1 인스턴스 · 1 서버 · 1 계정).
    default_spec: Option<ConnectSpec>,
    /// 탐색기·접속 창 표시가 따라가는 세션(접속 창으로 마지막에 붙은 세션).
    primary_sess: u64,
    /// 통제 상태를 마지막으로 툴바에 쓴 값(바뀔 때만 쓴다).
    gate_shown: Option<bool>,
    /// 탭 표식 메뉴가 가리키는 탭 id.
    badge_menu_tab: Option<u64>,
    /// 마지막으로 표식을 맞춘 시점의 탭 목록(바뀌면 즉시 다시 맞춘다).
    badge_tabs: Vec<u64>,
    /// 탭↔세션 묶임이 바뀌었으니 표식·해제 버튼 배지를 다시 맞춰야 함(`sync_sess`가 새 탭을 묶는 순간 · 09-19).
    sess_ui_dirty: bool,
    /// 유휴 세션 점검 다음 시각(§6 · 30초 간격).
    idle_next: Instant,
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
    // 입력 상태
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    alt: bool,
    /// 마지막 마우스 위치(창 좌표 · 휠의 안/밖 판정 — 휠 사건에는 좌표가 없다 · 09-24).
    pointer: Option<Point>,
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
/// 변수 타입의 표시 글자(변수 창).
fn var_type_text(ty: &nsql_core::VarType) -> String {
    ty.sql_name()
}

/// 바뀐 변수의 로그 한 줄(`A = 1, B = 'x'`) — 비밀 값은 가리고(D-140) 긴 값은 앞부분만 · 사라진 이름은 `(removed)`.
fn vars_log_line(
    changed: &[String],
    local: &[nsql_script::VarState],
    shared: &[nsql_script::VarState],
    global: &[nsql_script::VarState],
) -> String {
    const MAX: usize = 80;
    let mut parts = Vec::new();
    for name in changed {
        let found = local
            .iter()
            .chain(shared)
            .chain(global)
            .find(|v| v.name.eq_ignore_ascii_case(name));
        let text = match found {
            None => "(removed)".to_string(),
            Some(v) if v.secret => "******".to_string(),
            Some(v) => {
                let d = match &v.value {
                    nsql_core::Value::Str(s) => format!("'{s}'"),
                    other => other.display(),
                };
                if d.chars().count() > MAX {
                    format!("{}…", d.chars().take(MAX).collect::<String>())
                } else {
                    d
                }
            }
        };
        parts.push(format!(":{name} = {text}"));
    }
    parts.join(", ")
}

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
mod key_guard_tests {
    use super::key_guard_step;
    use std::time::Duration;

    /// 닫힌 창의 키 문지기 — 조건마다 하나씩 뒤집어 본다(문지기 있음 · 누름 · 반복 · 3초 안).
    #[test]
    fn key_guard_drops_only_repeats_until_release() {
        let fresh = Some(Duration::from_millis(50));
        assert_eq!(
            key_guard_step(fresh, true, true),
            (true, true),
            "자동 반복 = 버림"
        );
        assert_eq!(
            key_guard_step(None, true, true),
            (false, false),
            "문지기 없음"
        );
        assert_eq!(
            key_guard_step(fresh, false, false),
            (false, false),
            "뗌 = 통과 · 끝"
        );
        assert_eq!(
            key_guard_step(fresh, true, false),
            (false, false),
            "새로 누름 = 통과 · 끝"
        );
        let old = Some(Duration::from_secs(4));
        assert_eq!(
            key_guard_step(old, true, true),
            (false, false),
            "3초 뒤에는 끝"
        );
    }
}

#[cfg(test)]
mod find_case_tests {
    use super::{find_in_chars, preserve_case};

    /// 줄마다 찾기(T-142 — 본문을 통째로 뜨지 않는다) = 본문 전체에서 찾기와 **같은 답**(질의에 줄바꿈이 없을 때):
    /// 대소문자 · 단어 단위 · 줄 처음/끝의 경계 · 한글 · 겹치는 후보 · 빈 줄.
    #[test]
    fn per_line_find_equals_whole_text_find() {
        let text = "select a, A_b from tab;\nSELECT a\n\n값 = a; aaa a\ntab tabtab tab\nab";
        let all: Vec<char> = text.chars().collect();
        for q in ["a", "A", "tab", "aa", "select", "값", "b", "ab"] {
            let q: Vec<char> = q.chars().collect();
            for cs in [false, true] {
                for ww in [false, true] {
                    let mut whole = Vec::new();
                    find_in_chars(&all, 0, &q, cs, ww, &mut whole);
                    let mut per_line = Vec::new();
                    let mut base = 0usize;
                    for line in text.split('\n') {
                        let lc: Vec<char> = line.chars().collect();
                        find_in_chars(&lc, base, &q, cs, ww, &mut per_line);
                        base += lc.len() + 1;
                    }
                    assert_eq!(per_line, whole, "{q:?} cs={cs} ww={ww}");
                }
            }
        }
        // 단어 단위: `tab`은 `tabtab` 안에서는 일치가 아니다.
        let mut out = Vec::new();
        find_in_chars(&all, 0, &['t', 'a', 'b'], true, true, &mut out);
        assert_eq!(out.len(), 3);
    }

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
        // 확장 아이콘·패널 = 확장 관리자가 켜져 있을 때만(꺼지면 아이콘이 사라지고 패널도 닫힌다 · 사용자 09-19).
        let ext_on = self.settings.flag("extensions.enabled");
        self.act_bar.set_item_visible("view.extensions", ext_on);
        if !ext_on && self.ext_panel.is_visible() {
            self.ext_panel.set_visible(false);
            if self.focus == Focus::Ext {
                self.set_focus(Focus::Editor);
            }
        }
        self.act_bar.set_active(if self.explorer.is_visible() {
            Some("view.explorer")
        } else if self.search.is_visible() {
            Some("view.search")
        } else if self.ext_panel.is_visible() {
            Some("view.extensions")
        } else if self.project_panel.is_visible() {
            Some("view.project")
        } else if self.bm_panel.is_visible() {
            Some("view.bookmarks")
        } else if self.outline_panel.is_visible() {
            Some("view.outline")
        } else {
            None
        });
        let exp_w = if self.explorer.is_visible()
            || self.search.is_visible()
            || self.ext_panel.is_visible()
            || self.project_panel.is_visible()
            || self.bm_panel.is_visible()
            || self.outline_panel.is_visible()
        {
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
        self.explorer.set_menu_area(Rect::new(0, 0, w, h));
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
        self.project_panel.set_bounds(
            Rect::new(
                act_w,
                body_top,
                if self.project_panel.is_visible() {
                    exp_w
                } else {
                    0
                },
                body_h,
            ),
            s,
        );
        self.project_panel.set_clamp_width(w);
        self.bm_panel.set_bounds(
            Rect::new(
                act_w,
                body_top,
                if self.bm_panel.is_visible() { exp_w } else { 0 },
                body_h,
            ),
            s,
        );
        self.bm_panel.set_clamp_width(w);
        self.outline_panel.set_bounds(
            Rect::new(
                act_w,
                body_top,
                if self.outline_panel.is_visible() {
                    exp_w
                } else {
                    0
                },
                body_h,
            ),
            s,
        );
        self.outline_panel.set_clamp_width(w);
        self.ext_panel.set_bounds(
            Rect::new(
                act_w,
                body_top,
                if self.ext_panel.is_visible() {
                    exp_w
                } else {
                    0
                },
                body_h,
            ),
            s,
        );
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
        self.palette.set_window(w, h);
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
        self.project_panel.set_focused(f == Focus::Project);
        self.bm_panel.set_focused(f == Focus::Bookmarks);
        self.outline_panel.set_focused(f == Focus::Outline);
        self.ext_panel.set_focused(f == Focus::Ext);
        self.ime_refresh();
    }

    /// 메인 창 IME 허용 = 글 입력 포커스이거나 **팔레트가 열려 있을 때**(사용자 09-22 "팔레트에 한글 입력이 안 됨" — 포커스가
    /// 그리드/탐색기인 채 팔레트를 열면 IME가 꺼져 한글이 새고 있었다). 앱 조합 모드(T-139)면 늘 끔.
    fn ime_refresh(&mut self) {
        let f = self.focus;
        let palette = self.palette.is_open();
        if let Some(w) = &self.window {
            // 앱 조합 모드(T-139)면 어느 포커스든 IME를 끊는다(raw 자모 → 상자가 조합) · 아니면 글 입력 포커스에서만 붙인다.
            w.set_ime_allowed(
                input::system_ime()
                    && (palette
                        || f == Focus::Editor
                        || f == Focus::Find
                        || f == Focus::Search
                        || f == Focus::Ext
                        || f == Focus::Project
                        || f == Focus::Bookmarks
                        || f == Focus::Outline),
            );
        }
    }

    /// 한글 조합 방식 맞춤(T-139): 설정 × OS × 입력 소스 → nexa-ctl 앱 조합 스위치 + 모든 창의 IME 허용.
    /// 부르는 때 = 기동 · 창 활성화 · 입력 소스 바뀜 알림 · 설정 변경(값이 바뀔 때만 창에 쓴다).
    fn sync_hangul_mode(&mut self) {
        let setting = self
            .settings
            .get("input.hangul_compose")
            .unwrap_or("auto")
            .to_string();
        let korean = if setting == "auto" && cfg!(target_os = "macos") {
            nexa_sys::input_source::is_korean()
        } else {
            None
        };
        let app = input::hangul_app_mode(&setting, cfg!(target_os = "macos"), korean);
        if self.hangul_app == Some(app) {
            return;
        }
        self.hangul_app = Some(app);
        nexa_ctl::controls::set_hangul_app_compose(app);
        input::set_system_ime(!app);
        let f = self.focus;
        self.set_focus(f);
        for w in [
            self.conn_win.window(),
            self.file_win.window(),
            self.prefs_win.window(),
            self.txlog_win.window(),
            self.input_win.window(),
            self.sqlprev_win.window(),
            self.vars_win.window(),
        ]
        .into_iter()
        .flatten()
        {
            w.set_ime_allowed(!app);
        }
    }

    /// 포커스 텍스트 박스(IME·편집 컨텍스트 라우팅).
    fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        match self.focus {
            Focus::Editor => Some(self.editors.cur_mut()),
            Focus::Find => self.find.focused_textbox(),
            Focus::Search => self.search.focused_textbox(),
            Focus::Ext => self.ext_panel.focused_textbox(),
            Focus::Project => self.project_panel.focused_textbox(),
            Focus::Bookmarks => self.bm_panel.focused_textbox(),
            Focus::Outline => self.outline_panel.focused_textbox(),
            Focus::Explorer => self.explorer.focused_textbox(),
            Focus::Grid => None,
        }
    }

    /// 접속 패널의 요청 → 워커/저장소.
    fn handle_conn_win_action(&mut self, a: ConnWinAction) {
        match a {
            ConnWinAction::Paint => {
                let ui_px = self.settings.font_px("ui.font_size");
                self.conn_win.paint(&self.ui_font, &self.theme, ui_px);
            }
            ConnWinAction::Panel(a) => self.handle_panel_action(a),
            ConnWinAction::Login(name) => self.login_profile(&name),
            ConnWinAction::TestProfile(name) => self.test_profile(&name),
            ConnWinAction::Delete(name) => self.delete_profile(&name),
            ConnWinAction::Duplicate(name) => self.duplicate_profile(&name),
            ConnWinAction::SetEnv(name, env) => self.set_profile_env(&name, env),
            ConnWinAction::CopyText(text) => {
                if !clipboard::write_text(&text) {
                    self.sess.status = t(Msg::ErrClipboard).into();
                }
            }
        }
    }

    fn handle_panel_action(&mut self, a: PanelAction) {
        match a {
            PanelAction::Connect(spec) => {
                let name = self.conn_win.panel.profile_name();
                // 테스트 중인 프로필은 끝날 때까지 접속도 막는다(사용자 09-14).
                if self.conn_win.is_testing(&name) {
                    self.sess.status = t(Msg::StTesting).into();
                    self.conn_win.panel.set_state(ConnState::Testing);
                    return;
                }
                if let Some(pw) = spec.password.as_deref() {
                    self.conn_win.remember_pw(&name, pw);
                }
                self.conn_win
                    .set_connect_mark(&name, Some(ConnectMark::Connecting));
                self.panel_op = Some((name.clone(), ConnState::Connecting));
                // ★ 접속 중 막(사용자 09-19): 창 전체를 덮고 대상·단계·경과를 보인다 — 큐 대기면 그 단계부터.
                let phase = if self.attempts_inflight >= self.attempts_max {
                    Msg::VeilQueued
                } else {
                    Msg::VeilConnecting
                };
                self.conn_win
                    .veil_begin(&name, &spec.connection_string(), t(phase));
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
                self.sess.disc_path = Some(sessions::DiscPath::ConnWin);
                self.disconnect_now();
            }
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
                        self.sess.status = tf(Msg::WkProfileSaved, &[&name, ""]);
                        // 저장본 = 지금 값(바뀜 표시 전부 꺼짐 · T-131).
                        self.conn_win.panel.mark_saved();
                        // 이름이 바뀌었으면 옛 이름의 결과는 버린다(저장 내용이 달라졌을 수 있으니 옮기지 않는다).
                        if let Some(old) = &rename_from {
                            self.panel_results.remove(old.trim());
                        }
                        // 폼은 이제 새 이름의 프로필을 "불러온" 상태 — 또 바꿔 저장하면 다시 이름 변경.
                        self.conn_win.panel.fill(&name, &spec);
                        self.conn_win.refresh_profiles(Some(&name));
                        // 지킴이가 미뤄 둔 동작(다른 프로필 불러오기 · New · 닫기)을 이어간다.
                        for a in self.conn_win.after_save(true) {
                            self.handle_conn_win_action(a);
                        }
                    }
                    Err(e) => {
                        let e = e.to_string();
                        self.sess.status = tf(Msg::WkProfileSaveFailed, &[&e]);
                        self.conn_win.panel.set_state(ConnState::Failed(e));
                        self.conn_win.after_save(false);
                    }
                }
            }
            PanelAction::Edit(_) => {}     // 접속 창이 자체 처리(클립보드)
            PanelAction::CopyFile(_) => {} // 접속 창이 자체 처리(`copy_profile_file` → CopyText)
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
        // ★ 진행 중은 **행의 표식이 원천**(사용자 09-19: 테스트 도중 Details를 펼치면 가끔 "Testing"이 안 보였다 — `panel_op`가 다른
        //   프로필의 결과·프로브에 덮여 있었다). 행 Test/Connect와 폼 Test/Connect는 같은 표식을 본다.
        if self.conn_win.is_testing(name) {
            return ConnState::Testing;
        }
        if self.conn_win.connect_mark(name) == Some(ConnectMark::Connecting) {
            return ConnState::Connecting;
        }
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
                    self.sess.status = msg.clone();
                    self.conn_win.set_test_mark(&name, TestMark::Ok);
                    self.set_panel_result(&name, ConnState::TestOk(msg));
                }
                Err(e) => {
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("test: {e}")));
                    self.sess.status = tf(Msg::StTestFailed, &[&e]);
                    self.conn_win.set_test_mark(&name, TestMark::Failed);
                    self.set_panel_result(&name, ConnState::Failed(e));
                    self.conn_win.note_failure(&name);
                }
            }
        }
        while let Ok(o) = self.sess.worker.conn.try_recv() {
            changed = true;
            match o {
                ConnOutcome::Connected(d) if !std::mem::take(&mut self.sess.attempt_inflight) => {
                    // 접속 창을 거치지 않은 접속(개별 모드 자동 접속 · 유휴 뒤 재접속) — 시도 큐·접속 창 표시는 그대로.
                    self.sess.key_cache.clear();
                    self.sess.connected = true;
                    self.sess.desc = d;
                }
                ConnOutcome::ConnectFailed(e)
                    if !std::mem::take(&mut self.sess.attempt_inflight) =>
                {
                    self.sess.connected = false;
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("connect: {e}")));
                    self.sess.status = tf(Msg::StConnectFailed, &[&e]);
                }
                ConnOutcome::Connected(d) => {
                    self.attempt_done();
                    self.sess.key_cache.clear();
                    self.sess.connected = true;
                    self.sess.desc = d.clone();
                    // 접속 창으로 붙은 세션 = 탐색기·접속 창 표시가 따라가는 세션 · 개별 모드의 새 탭이 쓸 기본 접속 정보.
                    //   공유 모드면 이 연결이 **활성 공유 연결**이 된다(묶이지 않은 탭이 따른다 · 기존 연결은 그대로 유지).
                    let relinked = self.primary_sess != self.sess.id;
                    self.primary_sess = self.sess.id;
                    if !self.sess.is_private() {
                        self.default_shared = self.sess.id;
                    }
                    self.default_spec = self.sess.last_spec.clone();
                    self.sess.spec = self.sess.last_spec.clone();
                    // 이미 붙어 있던 연결을 다시 고른 경우(워커가 세션을 유지 = Connected 이벤트 없음) 탐색기를 이쪽으로 돌린다.
                    // ★ 또한 `RunEvent::Connected`가 이 결과보다 **먼저** 처리되면(두 채널의 경주 · 시작 인자/`dev.start_demo`
                    //   접속에서 재현 · 사용자 09-19 "sqlite는 접속이 안 되었다" = 탐색기에 Demo 루트가 없음) 그때는 `spec`이
                    //   비어 있어 탐색기를 못 붙였다 → 지금 spec이 채워졌으니 이 서버의 탐색기가 없으면 여기서 붙인다.
                    let missing = !self.explorer.has_server(self.sess.spec.as_ref());
                    if relinked || missing {
                        self.explorer_attach();
                    }
                    let name = self.panel_op_name();
                    self.conn_win.mark_connected(&name);
                    // 접속 버튼 초록 = 지금 접속된 프로필 하나만 → 잠시 보여 준 뒤 창 닫힘(사용자 09-14).
                    self.conn_win.clear_connect_marks();
                    self.conn_win
                        .set_connect_mark(&name, Some(ConnectMark::Connected));
                    self.conn_win.veil_phase(&name, t(Msg::VeilConnected));
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
                    self.sess.status = tf(Msg::StConnectFailed, &[&e]);
                    let name = self.panel_op_name();
                    self.conn_win.set_connect_mark(&name, None);
                    self.conn_win.veil_end(&name);
                    self.set_panel_result(&name, ConnState::Failed(e));
                    // 접속 실패 확인 → 그 서버 신호등 즉시 갱신(사용자 09-14).
                    self.conn_win.note_failure(&name);
                }
                // 접속 문자열에 비밀번호 자리가 없다 → 한 번 묻는다(입력 창은 `about_to_wait`에서 · 워커는 답을 기다린다).
                ConnOutcome::PasswordNeeded { target, rejected } => {
                    self.sess.status = t(Msg::StPasswordPrompt).into();
                    if rejected {
                        // 들고 있던 비밀번호가 거부돼 폐기했다 — 로그에 한 줄(값은 없다 · 대상은 가린 표시).
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::PasswordHintRejected, &[&target]),
                        ));
                    }
                    self.pw_pending = Some((self.sess.id, target, rejected));
                }
                ConnOutcome::SessionId(sid) => {
                    self.sess.live_sid = Some(sid);
                }
                ConnOutcome::Keys(table, info) => {
                    self.sess.aux_done();
                    self.sess.key_cache.insert(table, info.clone());
                    if let Some(kind) = self.sess.sql_wait.take() {
                        self.finish_sql_copy(kind, info.as_ref());
                    }
                    if let Some(kind) = self.sess.view_wait.take() {
                        self.finish_view_sql(kind, info.as_ref());
                    }
                }
                ConnOutcome::Requery { key, offset, limit } => {
                    // 커서가 없어 서버에 새 SQL(OFFSET 재질의/재실행)이 간다 — 직접 실행처럼 카드(이미 켜져 있으면 로그만).
                    let line = tf(Msg::StRequery, &[&offset.to_string(), &limit.to_string()]);
                    self.log_win
                        .push(LogEntry::new(LogKind::Info, line.clone()));
                    if !self.sess.fetch_card.is_some_and(|(k, _)| k == key) {
                        let sql = self
                            .grid_for(key)
                            .map(|g| g.source_sql().to_string())
                            .unwrap_or_default();
                        self.fetch_card_start(key, Msg::CardRequery, &sql);
                    }
                    self.sess.status = line;
                    self.redraw();
                }
                ConnOutcome::FetchProgress { key, rows, bytes } => {
                    if let Some(g) = self.grid_for(key) {
                        g.set_fetch_progress(rows, bytes);
                    }
                    self.run_toast.progress(self.sess.run_card, rows, bytes);
                    dlog!(self, LogLayer::Fetch, LogLevel::Progress, {
                        LogEntry::new(
                            LogKind::Fetch,
                            tf(Msg::LogDetProgress, &[&nsql_core::fmt_bytes(bytes)]),
                        )
                        .rows(rows)
                    });
                    self.sess.status = tf(
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
                    via_cursor,
                } => {
                    self.sess.aux_done();
                    // 실행 Facade(docs/43 §11): 이 페치의 카드가 켜져 있으면 결과로 끝낸다(실패 = Error).
                    let card = self.sess.fetch_card.is_some_and(|(k, _)| k == key);
                    if card {
                        if let Err(e) = &result {
                            self.run_toast
                                .finish(self.sess.run_card, runtoast::Phase::Error(e.clone()));
                            self.sess.fetch_card = None;
                        }
                    }
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
                            self.sess.status =
                                if !sessions::fetch_card_policy(all, false, via_cursor) {
                                    tf(Msg::StFetchedCursor, &[&n, &secs, &total.to_string()])
                                } else {
                                    tf(Msg::StFetched, &[&n, &secs, &total.to_string()])
                                };
                            if !sessions::fetch_card_policy(all, false, via_cursor) {
                                // 커서 이어 읽기 = 카드 없이 로그 한 줄(서버에 새 SQL 없음).
                                self.log_win
                                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                            }
                            if all || card {
                                self.sess.fetch_card = None;
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
                                self.run_toast.finish(self.sess.run_card, phase);
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
                                Some(worker::FetchStop::CursorGone) => {
                                    // 커서 결과의 커서가 닫혔다(T-202) — 그리드는 빈 페이지로 `more=false`가 됐다 · 안내 한 줄.
                                    self.sess.status = t(Msg::StCursorGone).into();
                                    self.log_win.push(LogEntry::new(
                                        LogKind::Info,
                                        self.sess.status.clone(),
                                    ));
                                }
                                Some(worker::FetchStop::Budget) => {
                                    // 전체 조회가 메모리 예산(D-72)에서 멈췄다.
                                    self.sess.status = tf(
                                        Msg::StBudgetExceeded,
                                        &[&self.settings.int("grid.memory_budget_mb").to_string()],
                                    );
                                    self.log_win.push(LogEntry::new(
                                        LogKind::Info,
                                        self.sess.status.clone(),
                                    ));
                                }
                                Some(worker::FetchStop::Cancelled) => {
                                    self.sess.status =
                                        tf(Msg::StFetchCancelled, &[&total.to_string()]);
                                    self.log_win.push(LogEntry::new(
                                        LogKind::Info,
                                        self.sess.status.clone(),
                                    ));
                                }
                                None => {}
                            }
                        }
                        Err(e) => {
                            if let Some(g) = self.grid_for(key) {
                                g.fetch_failed();
                            }
                            self.sess.status = tf(Msg::StFetchFailed, &[&e]);
                            self.log_win
                                .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                        }
                    }
                    self.redraw();
                }
                ConnOutcome::Count { key, result } => {
                    self.sess.aux_done();
                    if let Some((k, t0)) = self.sess.fetch_card {
                        if k == key {
                            self.sess.fetch_card = None;
                            let phase = match &result {
                                Ok(n) => runtoast::Phase::Done {
                                    rows: Some(*n),
                                    secs: t0.elapsed().as_secs_f64(),
                                    stages: String::new(),
                                },
                                Err(e) => runtoast::Phase::Error(e.clone()),
                            };
                            self.run_toast.finish(self.sess.run_card, phase);
                        }
                    }
                    match result {
                        Ok(n) => {
                            if let Some(g) = self.grid_for(key) {
                                g.set_total(n);
                            }
                            self.sess.status = tf(Msg::StCountResult, &[&n.to_string()]);
                            self.log_win
                                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                        }
                        Err(e) => {
                            if let Some(g) = self.grid_for(key) {
                                g.fetch_failed();
                            }
                            self.sess.status = tf(Msg::StFetchFailed, &[&e]);
                        }
                    }
                    self.redraw();
                }
                ConnOutcome::Disconnected => self.on_conn_disconnected(),
                // ★ 끊김 확인(docs/53 §3): 세션은 남기고 상태만 — 표식·플러그·목록이 "끊김"을 보인다.
                ConnOutcome::Broken(m) => {
                    if !self.sess.broken {
                        self.sess.broken = true;
                        let line = if m.is_empty() {
                            tf(Msg::StSessBroken, &[&self.sess.desc])
                        } else {
                            format!("{} — {m}", tf(Msg::StSessBroken, &[&self.sess.desc]))
                        };
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, line.clone()));
                        self.sess.status = line;
                        self.sync_sess_ui();
                    }
                }
                ConnOutcome::Alive => {
                    if self.sess.broken {
                        self.sess.broken = false;
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::StSessAlive, &[&self.sess.desc]),
                        ));
                        self.sync_sess_ui();
                    }
                }
            }
        }
        changed
    }

    /// 계측 지점(`NSQL_TRACE_FRAMES=1`일 때만 · 메모리에 쌓고 present 뒤 한 줄).
    fn tmark(&self, what: &'static str) {
        if self.frame_trace.is_some() && self.input_at.is_some() {
            self.trace_marks.borrow_mut().push((what, Instant::now()));
        }
    }

    fn redraw(&self) {
        if let Some(w) = &self.window {
            self.tmark("request_redraw");
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

    fn find_matches(&mut self) -> Vec<(usize, usize)> {
        let mut out = self.find_matches_all();
        // 선택 범위에서 찾기(≡ · Alt+L): 켤 때 잡은 범위 안의 일치만.
        if let Some((a, e)) = self.find_scope {
            out.retain(|(s, t)| *s >= a && *t <= e);
        }
        out
    }

    /// 본문 전체의 일치 구간(글자 인덱스). 보통 찾기(정규식 아님 · 질의에 줄바꿈 없음)는 **버퍼의 줄을 하나씩** 본다 —
    /// 종전에는 찾기 입력마다 본문 전체를 문자열 + 글자 배열로 다시 만들었다(65 MB 파일 = 임시 325 MB · T-142).
    fn find_matches_all(&mut self) -> Vec<(usize, usize)> {
        // ★ 정규식 모드 = fancy-regex(문자 인덱스로 변환) — 엔진이 문자열 하나를 요구한다.
        if self.find.regex() {
            let Some(r) = self.find_rx() else {
                return Vec::new();
            };
            let full = self.ed_mut().text();
            return rx::find_all(&r, &full);
        }
        let q: Vec<char> = self.find.query().chars().collect();
        let (cs, ww) = (self.find.case_sensitive(), self.find.whole_word());
        let buf = self.editors.cur().buf();
        if q.is_empty() || q.len() > buf.len() {
            return Vec::new();
        }
        let mut out = Vec::new();
        if q.contains(&'\n') {
            // 여러 줄 질의(드묾): 본문을 글자 배열로 떠서 종전 방식 그대로.
            let text: Vec<char> = buf.iter_from(0).collect();
            find_in_chars(&text, 0, &q, cs, ww, &mut out);
            return out;
        }
        let mut line: Vec<char> = Vec::new();
        for l in 0..buf.line_count() {
            let s = buf.line_text(l);
            if s.len() < q.len() {
                continue; // 바이트 수가 글자 수보다 작을 수는 없다 — 이 줄에는 들어갈 자리가 없다.
            }
            line.clear();
            line.extend(s.chars());
            find_in_chars(&line, buf.line_start(l), &q, cs, ww, &mut out);
        }
        out
    }

    /// 다음/이전 일치로 이동(순환) — `advance`면 현재 선택을 지나서, 아니면 캐럿부터.
    fn find_step(&mut self, forward: bool, advance: bool) {
        if !self.find.is_visible() {
            return;
        }
        let matches = self.find_matches();
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
        let matches = self.find_matches();
        let Some(&(s, e)) = matches.iter().find(|(s, _)| *s == a) else {
            return base;
        };
        let matched: String = full.chars().skip(s).take(e - s).collect();
        preserve_case(&matched, &base)
    }

    fn find_replace_one(&mut self) {
        let matches = self.find_matches();
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
        let matches = self.find_matches();
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
        // ★ 한 번 훑어 전부 바꾼다 + 되돌리기 **한 단계**(종전 = 일치마다 `replace_range` — 3 MB 본문 2,000건이 15초 ·
        //   되돌리기도 2,000번 눌러야 했다 · docs/60).
        let mut inv = Invalidations::default();
        let edits: Vec<(usize, usize, &str)> = matches
            .iter()
            .zip(repls.iter())
            .map(|((a, b), r)| (*a, *b, r.as_str()))
            .collect();
        self.ed_mut().replace_many(&edits, &mut inv);
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
                            self.sess.status = t(Msg::StFindNoSelection).into();
                        }
                    }
                } else {
                    self.find_scope = None;
                    self.ed_mut().set_find_scope(None);
                }
                self.find_step(true, false);
            }
            FindAction::SelectAll => {
                let matches = self.find_matches();
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
                self.sess.status = tf(Msg::StJsonError, &[&e.to_string()]);
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
            self.sess.status = tf(Msg::StJsonOpened, &[&path.display().to_string()]);
            self.redraw();
            return;
        }
        match open_external(&path) {
            Ok(()) => self.sess.status = tf(Msg::StJsonOpened, &[&path.display().to_string()]),
            Err(e) => self.sess.status = tf(Msg::StJsonError, &[&e]),
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
                        self.sess.status =
                            tf(Msg::StJsonReloaded, &[&r.changed.len().to_string(), &note]);
                        self.prefs_win.refresh(&self.settings);
                        self.prefs_win.redraw();
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                    }
                    Err(e) => {
                        self.sess.status = tf(Msg::StJsonError, &[&e]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                    }
                },
                Err(e) => self.sess.status = tf(Msg::StJsonError, &[&e.to_string()]),
            }
            self.redraw();
        }
        Some(self.json_next)
    }

    /// 지금 열린 모달 창(접속 · 파일 · 비밀번호 입력) — 사건 가드·`sync_modal`이 같은 판정을 쓴다.
    fn modal_window(&self) -> Option<&Window> {
        if self.input_win.is_modal() {
            return self.input_win.window();
        }
        self.file_win
            .window()
            .or_else(|| self.conn_win.window())
            .or_else(|| self.sqlprev_win.window())
    }

    fn modal_open(&self) -> bool {
        self.conn_win.is_open()
            || self.file_win.is_open()
            || self.input_win.is_modal()
            || self.sqlprev_win.is_open()
    }

    /// 모달 창(접속 · 파일 · 비밀번호 입력) 열림/닫힘 전환 → 메인 창 활성 상태 동기화(닫히면 메인으로 포커스).
    fn sync_modal(&mut self) {
        let open = self.modal_open();
        if open == self.conn_modal && !open {
            return;
        }
        self.conn_modal = open;
        if !open {
            // 닫힌 모달 창의 WindowId는 z-order 목록에서 걷어낸다(열 때마다 새 id → 남겨 두면 한 칸씩 자란다 · 09-15 누수 점검).
            let live: Vec<WindowId> = [
                self.window.as_deref(),
                self.log_win.window(),
                self.txlog_win.window(),
                self.sessions_win.window(),
                self.vars_win.window(),
                self.mem_win.window(),
                self.colors_win.window(),
                self.keys_win.window(),
                self.prefs_win.window(),
                self.conn_win.window(),
                self.file_win.window(),
                self.input_win.window(),
                self.sqlprev_win.window(),
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
                // 행 수가 바뀌었다(다중 행 도크 · 09-19) — 크롬 높이가 달라지니 창 전체를 다시 배치.
                DockAction::Resized => self.layout(),
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

    /// 저장하지 않은 탭을 닫으려 한다(X · Ctrl+W · 탭 메뉴 · 모두 닫기) — 설정 `editor.close_unsaved`: `ask` = 탭 옆 메뉴로 묻는다 ·
    /// `twice` = 종전의 2단 닫기.
    fn ask_save_close(&mut self, i: usize) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        if self.settings.get("editor.close_unsaved") == Some("twice") {
            self.editors.close_tab_two_step(i);
            return;
        }
        // 저장은 **활성 탭**에 하는 동작이다 → 닫으려는 탭을 앞으로.
        self.editors.switch(i);
        self.sync_grid_tab();
        let title = self
            .editors
            .tab_list()
            .into_iter()
            .nth(i)
            .map(|(_, t, _)| t)
            .unwrap_or_default();
        self.sess.status = tf(Msg::StUnsavedAsk, &[&title]);
        let items = vec![
            CtxItem::item("close.save", t(Msg::MnCloseSave)),
            CtxItem::item("close.discard", t(Msg::MnCloseDiscard)),
            CtxItem::Separator,
            CtxItem::item("close.cancel", t(Msg::MnCloseCancel)),
        ];
        let r = self
            .editors
            .tab_rect(i)
            .map_or(self.status_tx_rect, |r| Rect::new(r.x, r.bottom(), 0, 0));
        self.open_status_popup(r, items);
        self.redraw();
    }

    /// 닫기 확인 메뉴의 답.
    fn close_pick(&mut self, id: &str) {
        let tab = self.editors.active_id();
        match id {
            "close.discard" => {
                if let Some(i) = self.editors.index_of_id(tab) {
                    if let Some(p) = self.editors.path_of(i) {
                        backups::remove(&p);
                    }
                    self.editors.close_tab_forced(i);
                    self.sync_grid_tab();
                }
                if self.exit_pending {
                    self.request_exit();
                }
            }
            "close.cancel" => {
                self.exit_pending = false;
                self.exit_project_asked = false;
            }
            "close.save" => {
                self.close_after_save = Some(tab);
                match self.editors.active_path() {
                    Some(p) => {
                        self.save_to(&p);
                        self.finish_close_after_save();
                    }
                    // 이름 없는 스크립트 = 저장 창(이미 있는 이름은 그 창이 타임아웃 버튼으로 다시 확인한다).
                    None => self.open_file_dlg = Some(PickerMode::Save),
                }
            }
            _ => {}
        }
        self.redraw();
    }

    /// "저장하고 닫기"의 뒷부분 — 저장이 실제로 끝나 그 탭에 바뀐 것이 없을 때만 닫는다(저장 실패 · 외부 변경 확인 대기 = 닫지 않는다).
    fn finish_close_after_save(&mut self) {
        let Some(tab) = self.close_after_save.take() else {
            return;
        };
        if let Some(i) = self.editors.index_of_id(tab) {
            if !self.editors.is_dirty(i) {
                self.editors.close_tab_forced(i);
                self.sync_grid_tab();
            }
        }
        if self.exit_pending {
            self.request_exit();
        }
    }

    fn indent_pick(&mut self, id: &str) {
        if id.starts_with("close.") {
            self.close_pick(id);
            return;
        }
        // 거터(북마크/니모닉 영역) 우클릭 메뉴의 답(사용자 09-23).
        if let Some(rest) = id.strip_prefix("bmg.") {
            self.bm_gutter_pick(rest);
            return;
        }
        // 종료 전 프로젝트 저장 물음의 답(사용자 09-23).
        if id == "project.exit_save" || id == "project.exit_skip" {
            if id == "project.exit_save" {
                if let Err(e) = self.project_save() {
                    self.sess.status = tf(Msg::ErrProjectFile, &[&e]);
                }
            }
            self.exit_project_asked = true;
            self.request_exit();
            self.redraw();
            return;
        }
        // 툴바 Disconnect 드롭다운(공유 연결 목록 · docs/52 §7).
        if id.starts_with("conn.drop")
            || id.starts_with("conn.use:")
            || id.starts_with("conn.again:")
        {
            self.disconnect_pick(id);
            return;
        }
        let (ts, spaces) = self.editors.indent();
        if let Some(e) = id.strip_prefix("enc.set:") {
            self.editors.set_active_encoding(e);
            self.sess.status = tf(Msg::StEncSet, &[&enc::label(e)]);
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
        if let Some(rest) = id.strip_prefix("multi.") {
            self.multi_pick(rest);
            self.redraw();
            return;
        }
        if let Some(rest) = id.strip_prefix("tx.") {
            self.tx_pick(rest);
            self.redraw();
            return;
        }
        if let Some(rest) = id.strip_prefix("disc.") {
            self.disc_pick(rest);
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
        // 확장 관리자가 꺼져 있으면 **모든 확장이 꺼진 것**(설치 기록·개별 켬/끔과 무관 · 사용자 09-19).
        if !self.settings.flag("extensions.enabled") {
            return self
                .extensions
                .ids()
                .into_iter()
                .map(|(id, _)| id)
                .collect();
        }
        self.ext_disabled_list()
    }

    /// 설정 `extensions.disabled` 그대로(관리자 상태와 무관 — 패널·목록의 켬/끔 표시용).
    fn ext_disabled_list(&self) -> Vec<String> {
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
        // ★ 설치된 WASM 패키지 = 모듈을 로드해 레지스트리에(같은 id의 내장은 대체 · 실패면 내장 폴백 · docs/75).
        let want: Vec<(String, PathBuf)> = extensions::manager::root_dir()
            .map(|root| {
                installed
                    .iter()
                    .filter(|r| r.kind == extensions::manager::Kind::Wasm)
                    .filter_map(|r| {
                        extensions::wasm_module_path(&root, &r.id, &r.version)
                            .map(|p| (r.id.clone(), p))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for note in self.extensions.sync_wasm(&want) {
            self.log_win.push(LogEntry::new(LogKind::Info, note));
        }
        let mut disabled = self.ext_disabled();
        for (id, _) in self.extensions.ids() {
            if !installed.iter().any(|r| r.id == id) && !disabled.contains(&id) {
                disabled.push(id);
            }
        }
        let effects = self
            .extensions
            .on_settings(&self.settings, changed_key, &disabled);
        for e in effects {
            if let Some(mut b) = e.bracket_opts {
                // 자동 닫기 = 편집 코어 설정(`editor.auto_close_pairs`) — Rainbow Pairs 기능이 아니다(확장 유무와 무관 · 사용자 09-19).
                b.auto_close = self.settings.flag("editor.auto_close_pairs");
                // 쌍 종류 · 문자열 안 · 현재 쌍 강조도 편집 코어 설정(`editor.pair_*` · 사용자 09-23 "Rainbow 확장이 아니라 기본 기능 설정으로").
                b.pairs = nexa_ctl::PairOpts {
                    kinds: nexa_ctl::PairOpts::kinds_from_spec(
                        self.settings.get("editor.pair_kinds").unwrap_or(""),
                    ),
                    in_strings: self.settings.flag("editor.pair_in_strings"),
                };
                b.match_mode = match self.settings.get("editor.pair_match").unwrap_or("near") {
                    "off" => 0,
                    "always" => 2,
                    _ => 1,
                };
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
        // WASM 확장의 `nx_log` 줄·오류를 로그 창에(docs/75 §6).
        for note in self.extensions.take_wasm_notes() {
            self.log_win.push(LogEntry::new(LogKind::Info, note));
        }
    }

    /// Extension Manager 팔레트 명령(Sublime Package Control 방식 · docs/50 §10): 텍스트 목록을 만들어 팔레트에 띄우고,
    /// 고르면 `ext.<verb>:<key>`로 다시 들어온다.
    fn ext_command(&mut self, id: &str) {
        use extensions::manager as mgr;
        let disabled = self.ext_disabled();
        let builtin: Vec<(String, String)> = self.extensions.ids();
        let installed = mgr::installed();
        // builtin도 **설치 기록**이 있어야 설치된 것(설치 = 켜기 + 설정 분류 표시 · 삭제 = 끄기 + 숨김 · 사용자 09-17).
        let is_installed = |x: &str| installed.iter().any(|r| r.id == x);
        let _ = &builtin;
        let mut cmds: Vec<(String, String)> = Vec::new();
        // Package Control처럼 1회 활성화 — 켜기 전에는 목록/설치를 막는다(네트워크 사용을 알리는 지점).
        if id == "ext.enable_mgr" || id == "ext.disable_mgr" {
            let on = id == "ext.enable_mgr";
            let _ = self
                .settings
                .set("extensions.enabled", if on { "on" } else { "off" });
            let _ = self.settings.save();
            self.sess.status = if on {
                tf(
                    Msg::StExtManagerEnabled,
                    &[&mgr::default_source(&self.settings).display()],
                )
            } else {
                t(Msg::StExtManagerDisabled).into()
            };
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            // 켬/끔 = 확장 효과 전체 재적용(끄면 전부 정지) + 활동 막대 아이콘·패널(layout).
            self.apply_extensions(None);
            self.layout();
            self.redraw();
            return;
        }
        if !self.settings.flag("extensions.enabled") {
            self.sess.status = t(Msg::StExtManagerOff).into();
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
                            for p in idx.packages {
                                let n = self.ext_catalog.len();
                                if is_installed(&p.id) {
                                    self.ext_catalog.push((src.clone(), p));
                                    continue;
                                }
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
                            self.sess.status = tf(Msg::StExtIndexFailed, &[&src.display(), &e]);
                            self.log_win
                                .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                        }
                    }
                }
                // 비어도 **목록 창은 연다**(안내 한 줄 · 상태줄 글자만으로는 못 본다 · 사용자 09-19).
                if cmds.is_empty() {
                    cmds.push(("ext.noop".into(), t(Msg::StExtNoneAvailable).into()));
                }
                self.ext_panel_sync();
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
                                    t(Msg::ExtStDisabled)
                                } else {
                                    t(Msg::ExtStEnabled)
                                }
                            ),
                        ));
                    }
                }
                // 설치된 확장이 없어도 **목록 창을 열고 거기서** 알린다(사용자 09-19 · 종전 = 상태줄 "No extensions match").
                if cmds.is_empty() {
                    cmds.push((
                        "ext.noop".into(),
                        t(if installed.is_empty() {
                            Msg::ExtPanelNoneInstalled
                        } else {
                            Msg::StExtNoneInstalled
                        })
                        .into(),
                    ));
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
                    self.sess.status = t(Msg::StExtNoneInstalled).into();
                    self.redraw();
                    return;
                }
            }
            _ => return,
        }
        self.palette.set_commands(cmds);
        self.palette.open("");
        self.ime_refresh();
        self.redraw();
    }

    /// 팔레트에서 고른 확장 항목(`ext.<verb>:<key>`).
    fn ext_pick(&mut self, id: &str) {
        use extensions::manager as mgr;
        let (verb, key) = match id.trim_start_matches("ext.").split_once(':') {
            Some(p) => p,
            None => return,
        };
        // 확장 관리자가 꺼져 있으면 고르기도 막는다(명령 쪽 `ext_command`와 같은 문).
        if !self.settings.flag("extensions.enabled") {
            self.sess.status = t(Msg::StExtManagerOff).into();
            self.redraw();
            return;
        }
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
                        self.sess.status =
                            tf(Msg::StExtInstalled, &[&meta.name, &meta.version, &note]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                        self.apply_extensions(None);
                    }
                    Err(e) => {
                        self.sess.status = e;
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                    }
                }
            }
            "remove" => {
                let mut tr = mgr::Trace::default();
                let r = mgr::remove(key, &mut tr);
                self.ext_trace(tr);
                match r {
                    Ok(()) => {
                        self.sess.status = tf(Msg::StExtRemoved, &[key]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                        self.apply_extensions(None);
                    }
                    Err(e) => self.sess.status = e,
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
                self.sess.status = tf(
                    if verb == "disable" {
                        Msg::StExtDisabled
                    } else {
                        Msg::StExtEnabled
                    },
                    &[key],
                );
                self.apply_extensions(None);
            }
            // 목록에서 고르면 패널의 행 클릭과 같은 **상세 안내 탭**(종전 = 상태줄 한 줄).
            "list" => {
                let off = self.ext_disabled_list();
                if let Some(r) = mgr::installed().into_iter().find(|r| r.id == key) {
                    let cat = self.ext_catalog.iter().position(|(_, p)| p.id == r.id);
                    let row = ExtRow {
                        summary: cat
                            .map(|n| self.ext_catalog[n].1.summary.clone())
                            .unwrap_or_default(),
                        source: cat
                            .map(|n| self.ext_catalog[n].0.display())
                            .unwrap_or_default(),
                        enabled: !off.iter().any(|d| d == &r.id),
                        kind: r.kind.as_str().to_string(),
                        id: r.id,
                        name: r.name,
                        version: r.version,
                        installed: true,
                        catalog: cat,
                    };
                    self.ext_open_detail(&row);
                }
            }
            "repo" => {
                self.sess.status = if key == "default" {
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
                    self.sess.status = tf(Msg::StExtRepoRemoved, &[&s.display()]);
                    self.prefs_sync();
                }
            }
            _ => {}
        }
        if self.ext_panel.is_visible() {
            self.ext_panel_sync();
        }
        self.ext_details_sync();
        self.redraw();
    }

    /// 열려 있는 확장 상세 탭의 상태(설치됨·켜짐)를 지금 값으로 맞춘다 — 상세의 버튼을 누른 직후 버튼이 바뀌어야 한다.
    fn ext_details_sync(&mut self) {
        if self.ext_details.is_empty() {
            return;
        }
        let installed = extensions::manager::installed();
        let off = self.ext_disabled_list();
        for d in self.ext_details.values_mut() {
            let inst = installed.iter().any(|r| r.id == d.row.id);
            d.row.installed = inst;
            d.row.enabled = inst && !off.iter().any(|x| x == &d.row.id);
            d.row.catalog = self.ext_catalog.iter().position(|(_, p)| p.id == d.row.id);
            let state = t(if !inst {
                Msg::ExtStNotInstalled
            } else if d.row.enabled {
                Msg::ExtStEnabled
            } else {
                Msg::ExtStDisabled
            });
            if let Some(f) = d.fields.first_mut() {
                f.1 = state.to_string();
            }
        }
    }

    /// 확장 상세 뷰가 낸 동작 — 확장 패널과 같은 경로.
    fn ext_view_actions(&mut self) {
        for a in self.ext_view.take_actions() {
            match a {
                ext_view::ExtViewAction::Install(n) => self.ext_pick(&format!("ext.install:{n}")),
                ext_view::ExtViewAction::Remove(id) => self.ext_pick(&format!("ext.remove:{id}")),
                ext_view::ExtViewAction::Enable(id) => self.ext_pick(&format!("ext.enable:{id}")),
                ext_view::ExtViewAction::Disable(id) => self.ext_pick(&format!("ext.disable:{id}")),
            }
        }
    }

    // ───────────────────────── 다중 열기(사용자 09-22) ──────────

    /// 확인 팝업 — 개수·합계 크기 · 상한(`file.open_max`)까지의 목록 · 초과분 안내 · 열기/취소.
    fn multi_open_ask(&mut self, paths: Vec<PathBuf>, enc: String) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let info = |label: String| CtxItem::Item {
            id: String::new(),
            label,
            enabled: false,
            icon: None,
            shortcut: None,
            sub: None,
            children: Vec::new(),
            checked: None,
            active: false,
            mark: None,
            emph: false,
            marks: Vec::new(),
        };
        let max = self.settings.int("file.open_max").max(1) as usize;
        let take: Vec<PathBuf> = paths.iter().take(max).cloned().collect();
        let excluded = paths.len().saturating_sub(take.len());
        if take.len() == 1 {
            // 상한이 1이거나 하나만 남았다 = 바로.
            self.open_file_enc(&take[0], &enc);
            return;
        }
        let sizes: Vec<u64> = take
            .iter()
            .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
            .collect();
        let total: u64 = sizes.iter().sum();
        let mut items = vec![info(tf(
            Msg::MultiOpenHeader,
            &[&take.len().to_string(), &nsql_core::fmt_bytes(total)],
        ))];
        items.push(CtxItem::Separator);
        for (p, sz) in take.iter().zip(&sizes) {
            items.push(info(format!(
                "{}  ({})",
                editors::file_title(p),
                nsql_core::fmt_bytes(*sz)
            )));
        }
        if excluded > 0 {
            items.push(CtxItem::Separator);
            items.push(info(tf(
                Msg::MultiOpenExcluded,
                &[&excluded.to_string(), &max.to_string()],
            )));
        }
        items.push(CtxItem::Separator);
        items.push(CtxItem::item(
            "multi.open",
            tf(Msg::MultiOpenGo, &[&take.len().to_string()]),
        ));
        items.push(CtxItem::item("multi.cancel", t(Msg::BtnCancel)));
        self.multi_pending = Some((take, enc));
        // ★ 화면 **중앙** 모달(사용자 09-22): 한 번 열어 크기를 재고 가운데에 다시 연다 · Enter = 첫 활성 항목(모두 열기).
        let again = items.clone();
        self.open_status_popup(Rect::new(0, 0, 0, 0), items);
        let mb = self.status_menu.bounds();
        let r = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(
                    (sz.width as i32 - mb.w) / 2,
                    (sz.height as i32 - mb.h) / 2,
                    0,
                    0,
                )
            })
            .unwrap_or_default();
        let open_idx = again.len().saturating_sub(2); // [정보들 · 구분선 · 열기 · 취소]
        self.open_status_popup(r, again);
        self.status_menu.set_default(open_idx); // Enter = 모두 열기(기본 항목 · 테두리 표시)
    }

    /// 팝업 항목(`multi.*`).
    fn multi_pick(&mut self, id: &str) {
        match id {
            "open" => self.multi_open_start(),
            _ => {
                self.multi_pending = None;
            }
        }
    }

    /// 자리 탭을 전부 만들고 스레드 하나가 순서대로 읽는다. 이미 연 파일은 건너뛴다.
    fn multi_open_start(&mut self) {
        let Some((paths, enc)) = self.multi_pending.take() else {
            return;
        };
        let origin = self.editors.active_id();
        let mut jobs: Vec<(PathBuf, u64)> = Vec::new();
        for p in paths {
            if self.editors.path_tab(&p).is_some() {
                continue;
            }
            let id = self.editors.begin_load_tab(&editors::file_title(&p));
            jobs.push((p, id));
        }
        if jobs.is_empty() {
            self.redraw();
            return;
        }
        // 첫 파일의 탭을 보인다(자리 탭은 마지막 것이 활성이 되므로 되돌린다).
        self.editors.switch_to_id(jobs[0].1);
        let n = jobs.len();
        let (tx, rx) = std::sync::mpsc::channel();
        let proxy = std::sync::Mutex::new(self.wake_proxy.clone());
        let spawned = std::thread::Builder::new()
            .name("multi-open".into())
            .spawn(move || {
                for (p, id) in jobs {
                    let r = fileload::load(&p, &enc, None, true, None);
                    if tx.send((p, id, r)).is_err() {
                        break;
                    }
                    if let Ok(px) = proxy.lock() {
                        let _ = px.send_event(Wake);
                    }
                }
            });
        if spawned.is_err() {
            self.sess.status = tf(Msg::StFileReadError, &["multi-open", "thread"]);
            return;
        }
        self.multi_load = Some(MultiLoad {
            rx,
            remaining: n,
            total: n,
            origin,
            started: Instant::now(),
        });
        self.sess.status = tf(Msg::StMultiOpenStart, &[&n.to_string()]);
        self.set_focus(Focus::Editor);
        self.layout();
        self.redraw();
    }

    /// 순차 적재 결과 수거(틱) — 온 것부터 그 자리 탭에 옮겨 넣는다(활성 탭은 바꾸지 않는다).
    fn multi_load_poll(&mut self) {
        let Some(ml) = self.multi_load.as_mut() else {
            return;
        };
        let mut got = Vec::new();
        while let Ok(item) = ml.rx.try_recv() {
            got.push(item);
        }
        if got.is_empty() {
            return;
        }
        let origin = ml.origin;
        for (path, tab, result) in got {
            self.file_loaded(FileLoaded {
                path,
                mode: LoadMode::Open,
                tab: Some(tab),
                origin,
                result,
            });
            if let Some(ml) = self.multi_load.as_mut() {
                ml.remaining = ml.remaining.saturating_sub(1);
            }
        }
        let done = self.multi_load.as_ref().is_some_and(|m| m.remaining == 0);
        if done {
            let m = self.multi_load.take().unwrap_or_else(|| unreachable!());
            self.sess.status = tf(
                Msg::StMultiOpenDone,
                &[
                    &m.total.to_string(),
                    &format!("{:.1}", m.started.elapsed().as_secs_f32()),
                ],
            );
        }
        self.redraw();
    }

    // ───────────────────────── 프로젝트(docs/67 · T-165 · 사용자 09-22) ──────────

    /// Project 메뉴 항목 — 명령 · 폴더 제거(폴더마다) · 최근 프로젝트 · 프로젝트 없음.
    fn project_menu_entries(&self) -> Vec<MenuEntry> {
        let item = |id: &str, m: Msg| MenuEntry::Item(ComboItem::new(id, t(m)));
        let open = self.project.is_open();
        let gated = |id: &str, m: Msg| {
            if open {
                MenuEntry::Item(ComboItem::new(id, t(m)))
            } else {
                MenuEntry::Disabled(ComboItem::new(id, t(m)))
            }
        };
        let mut v = vec![
            item("project.new", Msg::MnProjectNew),
            item("project.open", Msg::MnProjectOpen),
            item("project.switch", Msg::MnProjectSwitch),
            MenuEntry::Separator,
            gated("project.save", Msg::MnProjectSave),
            gated("project.save_as", Msg::MnProjectSaveAs),
            gated("project.close", Msg::MnProjectClose),
            MenuEntry::Separator,
            gated("project.add_folder", Msg::MnProjectAddFolder),
        ];
        // 폴더 제거는 탐색기 루트 우클릭에서만(풀다운의 폴더별 항목은 뺐다 · 사용자 09-23).
        let recent = project::recent_list(self.settings.get("project.recent").unwrap_or(""));
        if !recent.is_empty() {
            v.push(MenuEntry::Separator);
            for (i, p) in recent.iter().enumerate() {
                let name = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                v.push(MenuEntry::Item(ComboItem::new(
                    format!("project.recent:{i}"),
                    format!("{}  {}", name, nexa_fs::path::display(p)),
                )));
            }
        }
        v
    }

    /// `project.*` 명령 하나로(메뉴 · 팔레트 · 탐색기 링크 · 기동 명령).
    fn project_cmd(&mut self, id: &str) {
        if let Some(rest) = id.strip_prefix("project.recent:") {
            let recent = project::recent_list(self.settings.get("project.recent").unwrap_or(""));
            if let Some(p) = rest
                .parse::<usize>()
                .ok()
                .and_then(|i| recent.get(i).cloned())
            {
                self.project_load_path(&p);
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("project.remove_folder:") {
            if let Some(f) = rest
                .parse::<usize>()
                .ok()
                .and_then(|i| self.project.remove_folder(i))
            {
                self.sess.status = tf(Msg::StProjectFolderRemoved, &[&nexa_fs::path::display(&f)]);
                self.project_changed(true);
            }
            return;
        }
        // 자체 캡처·자동화: 대화상자 없이 바로 연다.
        if let Some(p) = id.strip_prefix("project.load:") {
            self.project_load_path(Path::new(p.trim()));
            return;
        }
        // OPEN FILES × = 그 탭 닫기(미저장 확인은 기존 흐름).
        if let Some(id) = id
            .strip_prefix("editor.close:")
            .and_then(|s| s.trim().parse::<u64>().ok())
        {
            if let Some(i) = self.editors.index_of_id(id) {
                self.close_tab_guarded(i);
            }
            self.redraw();
            return;
        }
        // OPEN FILES 항목 클릭 = 그 탭으로(사용자 09-23).
        if let Some(id) = id
            .strip_prefix("editor.switch:")
            .and_then(|s| s.trim().parse::<u64>().ok())
        {
            self.editors.switch_to_id(id);
            self.sync_grid_tab();
            self.set_focus(Focus::Editor);
            self.redraw();
            return;
        }
        // 자체 시험: 탐색기 필터에 글 넣기(`project.filter:<글>` · 키 주입 없이 필터 결과를 캡처).
        if let Some(q) = id.strip_prefix("project.filter:") {
            // `project.filter:[cwrp]:<글>` = 옵션(Case·Word·Regex·Path)을 먼저 켠다.
            let (flags, text) = match q.split_once(':') {
                Some((f, t)) if !f.is_empty() && f.chars().all(|c| "cwrp".contains(c)) => (f, t),
                _ => ("", q),
            };
            self.project_panel.set_filter_opts(
                flags.contains('c'),
                flags.contains('w'),
                flags.contains('r'),
                flags.contains('p'),
            );
            self.project_panel.set_filter_text(text.trim());
            self.redraw();
            return;
        }
        match id {
            // "새 프로젝트 저장"(파일 모드에서도 늘 활성) = 빈 프로젝트 · "다른 이름으로" = 지금 프로젝트 복사 — 둘 다 저장 뒤 그 프로젝트로 전환.
            "project.new" | "project.save_as" => {
                self.project_new_fresh = id == "project.new";
                self.file_purpose = FilePurpose::Project;
                self.open_file_dlg = Some(PickerMode::Save);
            }
            "project.open" => {
                self.file_purpose = FilePurpose::Project;
                self.open_file_dlg = Some(PickerMode::Open);
            }
            "project.save" => {
                if let Some(p) = self.project.path.clone() {
                    self.project_save_to(&p);
                } else {
                    self.sess.status = t(Msg::StProjectNoProject).into();
                }
            }
            "project.close" | "project.none" => {
                self.palette.close();
                if self.project.is_open() {
                    let _ = self.project_save();
                    self.project_set(project::Project::default(), false);
                    // ★ 닫은 뒤 = 처음 실행 상태(사용자 09-23): 전체 상태는 위 저장으로 프로젝트 파일·스냅숏에 담겼다.
                    self.reset_workspace_to_initial();
                    self.sess.status = t(Msg::StProjectClosed).into();
                }
            }
            "project.add_folder" => {
                if self.project.is_open() {
                    self.file_purpose = FilePurpose::Project;
                    self.folder_start = self
                        .project
                        .path
                        .as_ref()
                        .and_then(|p| p.parent().map(Path::to_path_buf));
                    self.open_file_dlg = Some(PickerMode::Folder);
                } else {
                    self.sess.status = t(Msg::StProjectNoProject).into();
                }
            }
            // 탐색기 필터 옆 토글(사용자 09-22) — 파일 대화상자와 같은 설정 키를 뒤집는다 → 변경 훅이 패널에 되돌려 준다.
            "project.toggle_hidden" | "project.toggle_dot" => {
                let key = if id == "project.toggle_hidden" {
                    "file.show_hidden"
                } else {
                    "file.show_dot"
                };
                let on = !self.settings.flag(key);
                let _ = self.settings.set(key, if on { "on" } else { "off" });
                let _ = self.apply_setting(key);
                self.redraw();
            }
            "project.switch" => {
                // 팔레트로 고른다: (프로젝트 없음) + 최근 목록.
                let recent =
                    project::recent_list(self.settings.get("project.recent").unwrap_or(""));
                let mut cmds: Vec<(String, String)> =
                    vec![("project.none".into(), t(Msg::MnProjectNone).into())];
                for (i, p) in recent.iter().enumerate() {
                    let name = p
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    cmds.push((
                        format!("project.recent:{i}"),
                        format!("{}  {}", name, nexa_fs::path::display(p)),
                    ));
                }
                cmds.push(("project.open".into(), t(Msg::MnProjectOpen).into()));
                self.palette.set_commands(cmds);
                self.palette.open("");
                self.ime_refresh();
            }
            _ => {}
        }
        self.redraw();
    }

    /// ★ 프로젝트를 닫은 뒤 = **처음 실행 상태**(사용자 09-23 "기본 편집 탭 1개만 남기고 모두 닫기 · 초기 상태"): 편집기는
    /// 빈 `Script_1` 하나(전체 상태는 직전 `project_save`가 프로젝트 파일·스냅숏에 담았다) · 좌측은 객체 탐색기만 · 북마크는
    /// 로컬 세트(`bind_project(None)`이 이미 바꿈) · 접속은 그대로(접속은 사용자 몫 · 70 §2).
    fn reset_workspace_to_initial(&mut self) {
        self.editors.reset_to_initial();
        self.side_panel_close_others("view.explorer");
        self.sync_grid_tab();
        self.sync_gate();
        self.apply_tab_line_colors();
        self.sync_open_files();
        self.layout();
        self.redraw();
    }

    /// 프로젝트 파일을 읽어 현재 프로젝트로(현재 것은 먼저 저장).
    fn project_load_path(&mut self, path: &Path) {
        self.palette.close();
        match project::Project::load(path) {
            Ok(p) => {
                let switching = self.project.path.as_deref() != Some(path);
                if self.project.is_open() && switching {
                    let _ = self.project_save();
                }
                // 같은 파일을 다시 열면(전환 목록에서 지금 프로젝트) 복원하지 않는다 — 탭이 겹쳐 늘지 않게.
                self.project_set(p, switching);
            }
            Err(e) => {
                self.sess.status = tf(Msg::ErrProjectFile, &[&e]);
                self.log_win
                    .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
            }
        }
        self.redraw();
    }

    /// 새 프로젝트/다른 이름으로 — 지금 프로젝트(없으면 빈 것)를 그 경로에 쓴다.
    fn project_save_to(&mut self, path: &Path) {
        let path = if path.extension().is_some_and(|e| e == project::EXT) {
            path.to_path_buf()
        } else {
            path.with_extension(project::EXT)
        };
        let fresh = std::mem::take(&mut self.project_new_fresh);
        let mut p = if fresh {
            project::Project::default()
        } else {
            self.project.clone()
        }
        .with_path(&path);
        // 프로젝트 파일이 놓인 폴더 = 기본 폴더(새 프로젝트는 늘 · 복사본은 폴더가 없을 때만 · 사용자 09-22).
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            if fresh || p.folders.is_empty() {
                p.folders.insert(0, dir.to_path_buf());
                p.folders.dedup();
            }
        }
        // ★ 지금 작업 환경(탭 경로 · 캐럿 · 북마크 · 패널 상태)을 담아서 쓴다 — 담지 않고 써서 "저장했는데 탭 경로가 없다"
        //   (사용자 09-23). 새 프로젝트도 지금 열린 탭이 그 프로젝트의 첫 작업 환경이다.
        let prev = std::mem::replace(&mut self.project, p);
        self.project_capture_state();
        match self.project.save() {
            Ok(()) => {
                self.sess.status = tf(Msg::StProjectSaved, &[&nexa_fs::path::display(&path)]);
                // 파일 모드 → 새 프로젝트: 로컬 북마크는 프로젝트로 **이관**(로컬 파일 비움 · 사용자 09-23 "닫으면 북마크도 초기화").
                if !prev.is_open() {
                    self.bookmarks.mark_migrate_local();
                }
                let p = self.project.clone();
                self.project_set(p, false);
            }
            Err(e) => {
                self.project = prev;
                self.sess.status = tf(Msg::ErrProjectFile, &[&e]);
            }
        }
        self.redraw();
    }

    fn project_add_folder(&mut self, dir: &Path) {
        if self.project.add_folder(dir) {
            self.sess.status = tf(Msg::StProjectFolderAdded, &[&nexa_fs::path::display(dir)]);
            self.project_changed(true);
        }
    }

    /// 프로젝트 교체 = 상태 · 설정(`project.last` · 최근) · 탐색기 · 메뉴 · `restore` = 작업 환경 복원까지
    /// (**다른** 프로젝트를 열 때만 true — 지금 작업 환경을 그 파일에 저장하는 길(저장 · 새 프로젝트 · 닫기)은 false.
    ///  저장 때도 복원이 돌아 누를 때마다 스크립트 탭이 하나씩 늘던 결함 · 사용자 09-23).
    fn project_set(&mut self, p: project::Project, restore: bool) {
        self.project = p;
        let last = self
            .project
            .path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let _ = self.settings.set("project.last", &last);
        if let Some(p) = self.project.path.clone() {
            let raw = project::push_recent(self.settings.get("project.recent").unwrap_or(""), &p);
            let _ = self.settings.set("project.recent", &raw);
            self.sess.status = tf(
                Msg::StProjectOpened,
                &[
                    &self.project.name().unwrap_or_default(),
                    &self.project.folders.len().to_string(),
                ],
            );
        }
        self.persist_settings();
        self.project_changed(false);
        // 프로젝트가 바뀌면 북마크 저장소도(워크스페이스 파일 · 69 C-17).
        self.bookmarks.bind_project(self.project.path.as_deref());
        if restore {
            self.bm_sync_ui();
            self.project_restore();
        } else {
            // 🔧 저장·새 프로젝트·닫기 길: `bind_project`가 저장소를 비우고 워크스페이스 파일(옛 것)을 읽어 **방금 담은 북마크
            //   (니모닉 포함)가 사라지던 결함**(사용자 09-23) — 프로젝트 파일에 담긴 북마크를 그대로 되돌린다(복원 길은 `project_restore`가 한다).
            if let Some(js) = self.project.bookmarks.clone() {
                self.bookmarks.load_json(&js);
            }
            self.bm_sync_ui();
            self.project_last_json = self.project.to_document();
        }
    }

    /// 프로젝트 파일 저장 — 탐색기의 마지막 선택 위치를 담아서(닫기·전환·폴더 변경·종료 = 전부 이 길).
    fn project_save(&mut self) -> Result<(), String> {
        self.project_capture_state();
        let r = self.project.save();
        if r.is_ok() {
            self.project_last_json = self.project.to_document();
        }
        r
    }

    /// 작업 환경을 프로젝트에 담는다(사용자 09-23): 탐색기 선택 · 탭 순서(파일 = 경로만 · 스크립트 = 본문 ≤ 1 MB) ·
    /// 캐럿 + fuzzy 앵커(북마크 `make_anchor` · 10만 줄 넘는 탭은 줄 번호만) · 활성 탭 · 북마크(JSON).
    fn project_capture_state(&mut self) {
        if let Some(p) = self.project_panel.selected_path() {
            self.project.last_selected = Some(p);
        }
        let opts = nsql_bookmarks::RelocateOpts::default();
        let mut tabs = Vec::new();
        for i in 0..self.editors.tab_count() {
            let Some(tb) = self.editors.tab_box(i) else {
                continue;
            };
            let path = self.editors.path_of(i);
            let buf = tb.buf();
            let caret = tb.caret();
            let line = buf.line_of(caret);
            let col = caret.saturating_sub(buf.line_start(line));
            let mut t = project::TabState {
                path: path.clone(),
                title: self.editors.title_of(i),
                line,
                col,
                preview: self.editors.preview_id() == Some(self.editors.tab_id(i)),
                id: self.editors.tab_id(i),
                ..project::TabState::default()
            };
            if buf.line_count() <= 100_000 {
                let lines: Vec<String> = (0..buf.line_count())
                    .map(|l| buf.line_text(l).into_owned())
                    .collect();
                let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
                let a = nsql_bookmarks::make_anchor(
                    &refs,
                    line.min(refs.len().saturating_sub(1)),
                    col as u32,
                    &opts,
                );
                t.anchor_text = a.text;
                t.before = a.before;
                t.after = a.after;
            }
            // 본문을 담는 탭 = 이름 없는 스크립트 **+ 미저장 편집이 있는 파일 탭**(사용자 09-23 "탭별로 프로젝트 파일에 · 로드 때 자동 복구 ·
            // 백업은 백업용으로만") — 파일 탭은 그때의 디스크 해시도 같이(로드 때 밖에서 바뀌었는지 알리려고). 1 MB 상한은 같다.
            let dirty_file = path.is_some() && self.editors.is_dirty(i);
            if path.is_none() || dirty_file {
                let text = tb.text();
                if text.len() <= 1 << 20 {
                    t.text = Some(text);
                    if let Some(p) = &path {
                        t.disk_hash = backups::disk_hash(p);
                    }
                }
            }
            tabs.push(t);
        }
        // 파일 탭의 미저장 본문 = 스냅숏(docs/70 §5 · 원본은 안 만진다) · clean이면 스냅숏 삭제.
        for i in 0..self.editors.tab_count() {
            let Some(p) = self.editors.path_of(i) else {
                continue;
            };
            if self.editors.is_dirty(i) {
                if let Some(tb) = self.editors.tab_box(i) {
                    let text = tb.text();
                    if text.len() <= 8 << 20 {
                        backups::write(&p, &text);
                    }
                }
            } else {
                backups::remove(&p);
            }
        }
        self.project.tabs = tabs;
        self.project.active = self.editors.active();
        self.project.bookmarks = Some(self.bookmarks.store.to_json());
        // ★ 좌측 패널 상태(사용자 09-23 "각 좌측 기능별로 복원"): 탐색기 펼침 · 보이던 패널 · 검색어 · 북마크 접힘 · 접속 표식.
        self.project.expanded = self.project_panel.expanded_dirs();
        self.project.panel = if self.project_panel.is_visible() {
            Some("project".into())
        } else if self.bm_panel.is_visible() {
            Some("bookmarks".into())
        } else if self.search.is_visible() {
            Some("search".into())
        } else if self.ext_panel.is_visible() {
            Some("ext".into())
        } else {
            None
        };
        self.project.search = Some(self.search.query_text()).filter(|s| !s.is_empty());
        self.project.bm_collapsed = self.bm_panel.collapsed_groups();
        // 접속은 **표식만**(docs/70 §2 · 26 §8 자동 재접속 금지): 붙어 있는 세션의 프로필 이름(중복 제거 · 접속 순서).
        let mut profiles: Vec<String> = Vec::new();
        for s in std::iter::once(&self.sess).chain(self.parked.iter()) {
            if s.connected && !s.profile.is_empty() && !profiles.contains(&s.profile) {
                profiles.push(s.profile.clone());
            }
        }
        self.project.profiles = profiles;
    }

    /// 프로젝트를 열었을 때 작업 환경 복원(사용자 09-23): 탭 순서대로 파일은 다시 읽고(원본 = 최신) 스크립트는 본문 그대로 ·
    /// 캐럿은 앵커로 fuzzy 재탐색(외부 수정 대응 · 북마크 `relocate`) · 활성 탭 · 북마크.
    fn project_restore(&mut self) {
        let tabs = self.project.tabs.clone();
        // ★ 작업 환경은 **교체**된다(사용자 09-23 "마지막 작업 상태 그대로"): 열려 있던 탭 중 복원 목록에 없는 것은 뒤에서 닫는다
        //   — 이전 프로젝트가 있었으면 그 파일(스크립트 본문)과 스냅숏(파일 탭 미저장분)에 이미 담겼고, 없었으면(파일 모드에서
        //   열기) 미저장 탭은 남긴다. 닫기는 복원 뒤에(마지막 탭을 닫으면 빈 탭이 새로 생기는 일을 피한다).
        let prev_saved = {
            let n = b"\"tabs\"";
            self.project_last_json.windows(n.len()).any(|w| w == n)
        };
        let old_ids: Vec<u64> = (0..self.editors.tab_count())
            .map(|i| self.editors.tab_id(i))
            .collect();
        if tabs.is_empty() {
            self.project_restore_panels();
            return;
        }
        let opts = nsql_bookmarks::RelocateOpts::default();
        let mut ids: Vec<Option<u64>> = Vec::new();
        let mut restored_dirty = 0usize;
        let mut scratch_remap: Vec<(u64, u64)> = Vec::new();
        for t in &tabs {
            let id = match &t.path {
                Some(p) => {
                    if !p.is_file() {
                        None
                    } else {
                        if t.preview {
                            // 미리보기 탭은 미리보기로(북마크·탐색기 한 번 클릭으로 연 것 · 편집하면 승격).
                            self.project_open_req(project_panel::OpenReq {
                                path: p.clone(),
                                permanent: false,
                            });
                        } else {
                            self.open_file(p);
                        }
                        let i = self.editors.active();
                        if self.editors.active_path().as_deref() == Some(p.as_path()) {
                            // ★ 미저장 편집분은 **프로젝트 파일**에서 그대로 올린다(사용자 09-23 "탭별로 저장해 자동 복구" · `backups/`는
                            //   백업용으로만 · 복원에 쓰지 않는다). 디스크가 그 사이 바뀌었으면(해시 다름) 올리되 로그로 알린다.
                            if let Some(body) = &t.text {
                                if let Some(tb) = self.editors.tab_box_mut(i) {
                                    tb.set_text(body);
                                }
                                restored_dirty += 1;
                                if t.disk_hash != 0 && backups::disk_hash(p) != t.disk_hash {
                                    self.log_win.push(LogEntry::new(
                                        LogKind::Info,
                                        tf(Msg::StBackupExternal, &[&p.to_string_lossy()]),
                                    ));
                                }
                            }
                            self.restore_caret(i, t, &opts);
                            Some(self.editors.tab_id(i))
                        } else {
                            None
                        }
                    }
                }
                None => {
                    self.editors.new_tab(Some(t.title.clone()));
                    let i = self.editors.active();
                    if let Some(tb) = self.editors.tab_box_mut(i) {
                        if let Some(txt) = &t.text {
                            tb.set_text(txt);
                        }
                    }
                    self.restore_caret(i, t, &opts);
                    let new_id = self.editors.tab_id(i);
                    // 이름 없는 탭의 북마크(`Scratch { tab }`)는 옛 id → 새 id로 재매핑(사용자 09-23 검토 · 본문이 그대로라 줄이 맞는다).
                    if t.id != 0 {
                        scratch_remap.push((t.id, new_id));
                    }
                    Some(new_id)
                }
            };
            ids.push(id);
        }
        if let Some(Some(id)) = ids.get(self.project.active) {
            self.editors.switch_to_id(*id);
            self.sync_grid_tab();
        }
        if let Some(js) = self.project.bookmarks.clone() {
            if self.bookmarks.load_json(&js) {
                self.bookmarks.remap_scratch(&scratch_remap);
                self.bm_sync_ui();
            }
        }
        // 복원 목록에 없던 옛 탭 닫기(위 주석) — 복원으로 재사용된 탭(같은 파일)은 남는다.
        let restored: Vec<u64> = ids.iter().flatten().copied().collect();
        for id in old_ids.into_iter().rev() {
            if restored.contains(&id) {
                continue;
            }
            let Some(i) = self.editors.index_of_id(id) else {
                continue;
            };
            if !self.editors.is_dirty(i) || prev_saved {
                self.editors.close_tab_forced(i);
            }
        }
        if let Some(Some(id)) = ids.get(self.project.active) {
            self.editors.switch_to_id(*id);
            self.sync_grid_tab();
        }
        self.project_restore_panels();
        let n = restored.len();
        self.sess.status = if restored_dirty > 0 {
            tf(
                Msg::StProjectRestoredDirty,
                &[&n.to_string(), &restored_dirty.to_string()],
            )
        } else {
            tf(Msg::StProjectRestored, &[&n.to_string()])
        };
        if !self.project.profiles.is_empty() {
            // 접속은 표식만(docs/70 §2) — 어디에 붙어 있었는지 알려 주고 접속은 사용자가.
            let msg = tf(
                Msg::StProjectPrevProfiles,
                &[&self.project.profiles.join(", ")],
            );
            self.log_win.push(LogEntry::new(LogKind::Info, msg.clone()));
            self.toasts
                .push(toast::ToastKind::Info, t(Msg::MnProject).to_string(), msg);
        }
        self.project_last_json = self.project.to_document();
        self.sync_open_files();
        self.layout();
        self.redraw();
    }

    /// 좌측 패널 상태 복원(사용자 09-23 "각 좌측 기능별"): 탐색기 펼침(선택은 `set_project`가) · 검색어 · 북마크 접힘 · 보이던 패널.
    fn project_restore_panels(&mut self) {
        let expanded = self.project.expanded.clone();
        if !expanded.is_empty() {
            self.project_panel.expand_dirs(&expanded);
            if let Some(sel) = self.project.last_selected.clone() {
                self.project_panel.reveal(&sel);
            }
        }
        if let Some(q) = self.project.search.clone() {
            self.search.set_query_text(&q);
        }
        let folded = self.project.bm_collapsed.clone();
        if !folded.is_empty() {
            self.bm_panel.set_collapsed_groups(folded);
        }
        let want = match self.project.panel.as_deref() {
            Some("project") => Some(("view.project", self.project_panel.is_visible())),
            Some("bookmarks") => Some(("view.bookmarks", self.bm_panel.is_visible())),
            Some("outline") => Some(("view.outline", self.outline_panel.is_visible())),
            Some("search") => Some(("view.search", self.search.is_visible())),
            Some("ext") => Some(("view.extensions", self.ext_panel.is_visible())),
            _ => None,
        };
        if let Some((cmd, visible)) = want {
            if !visible {
                self.menu_action(cmd);
            }
        }
    }

    /// 저장된 캐럿을 지금 본문에 맞춘다 — 앵커가 있으면 북마크와 같은 fuzzy 재탐색(줄 이동 · 못 찾으면 저장된 줄).
    fn restore_caret(
        &mut self,
        i: usize,
        t: &project::TabState,
        opts: &nsql_bookmarks::RelocateOpts,
    ) {
        let Some(tb) = self.editors.tab_box_mut(i) else {
            return;
        };
        let mut line = t.line;
        if !t.anchor_text.is_empty() {
            let buf = tb.buf();
            let lines: Vec<String> = (0..buf.line_count())
                .map(|l| buf.line_text(l).into_owned())
                .collect();
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            let a = nsql_bookmarks::Anchor {
                line: t.line as u32,
                col: t.col as u32,
                text: t.anchor_text.clone(),
                before: t.before.clone(),
                after: t.after.clone(),
                doc_hash: 0,
                doc_lines: 0,
            };
            if let nsql_bookmarks::Relocated::Moved(l) =
                nsql_bookmarks::relocate(&a, &refs, None, opts)
            {
                line = l;
            }
        }
        tb.goto_line(line + 1);
    }

    /// 자동 저장의 다음 마감 시각(틱 스케줄러 깨움용 · 사용자 09-23): 사건 디바운스면 `touch + 2초` · 아니면 `마지막 저장 + 주기`.
    fn project_autosave_next(&self, now: Instant) -> Option<Instant> {
        if !self.project.is_open() || !self.settings.flag("project.autosave") {
            return None;
        }
        let secs = self.settings.int("project.autosave_secs").max(5) as u64;
        let periodic = self.project_autosave_at + Duration::from_secs(secs);
        let touched = self.project_touch_at.map(|t| t + Duration::from_secs(2));
        Some(touched.map_or(periodic, |t| t.min(periodic)).max(now))
    }

    /// ★ 종료 직전 마지막 저장(사용자 09-23 "프로그램 종료 시 꼭 저장"): 프로젝트(자동 저장이 켜져 있을 때 · 꺼져 있으면
    /// `request_exit`의 물음이 이미 처리) + **북마크 워크스페이스**(디바운스를 기다리지 않고 지금). 창 닫기·File ▸ Exit 두 길 모두.
    fn flush_on_exit(&mut self) {
        if self.project.is_open() && self.settings.flag("project.autosave") {
            let _ = self.project_save();
        }
        self.bookmarks.save_now();
    }

    /// 주기 자동 저장(설정 `project.autosave` · `project.autosave_secs`) — 바뀐 것이 있을 때만 쓴다.
    fn project_autosave_tick(&mut self) {
        if !self.project.is_open() || !self.settings.flag("project.autosave") {
            return;
        }
        let secs = self.settings.int("project.autosave_secs").max(5) as u64;
        let touched = self
            .project_touch_at
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(2));
        if !touched && self.project_autosave_at.elapsed().as_secs() < secs {
            return;
        }
        self.project_touch_at = None;
        self.project_autosave_at = Instant::now();
        self.project_capture_state();
        let js = self.project.to_document();
        if js != self.project_last_json && self.project.save().is_ok() {
            self.project_last_json = js;
        }
    }

    /// OPEN FILES(프로젝트 패널) 동기 — 탭 순서 · 제목 · 미저장 · 활성 · 동시 편집 칸.
    fn sync_open_files(&mut self) {
        if !self.project_panel.is_visible() {
            return;
        }
        let split = self.editors.split_tabs().to_vec();
        let v: Vec<project_panel::OpenFile> = self
            .editors
            .tab_list()
            .into_iter()
            .enumerate()
            .map(|(i, (id, title, active))| project_panel::OpenFile {
                id,
                title,
                dirty: self.editors.is_dirty(i),
                active,
                grouped: split.len() > 1 && split.contains(&i),
            })
            .collect();
        if self.project_panel.set_open_files(v) {
            self.project_touch();
        }
    }

    /// 작업 환경이 바뀌었다(탭 · 폴더) → 2초 뒤 저장(자동 저장 켬 · 프로젝트 열림).
    fn project_touch(&mut self) {
        if self.project.is_open() {
            self.project_touch_at = Some(Instant::now());
        }
    }

    /// 종료 전 프로젝트 저장 물음(자동 저장이 꺼져 있을 때).
    fn ask_project_exit(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        self.sess.status = t(Msg::StProjectAsk).into();
        let items = vec![
            CtxItem::item("project.exit_save", t(Msg::MnProjectExitSave)),
            CtxItem::item("project.exit_skip", t(Msg::MnProjectExitSkip)),
            CtxItem::Separator,
            CtxItem::item("close.cancel", t(Msg::MnCloseCancel)),
        ];
        let r = self.status_tx_rect;
        self.open_status_popup(r, items);
        self.redraw();
    }

    /// 폴더 목록이 바뀌었다 — 탐색기·메뉴 갱신(`save`면 파일에도).
    fn project_changed(&mut self, save: bool) {
        self.project_touch();
        if let Some(p) = self.project_panel.selected_path() {
            self.project.last_selected = Some(p);
        }
        if save {
            // 폴더 추가/제거 때도 작업 환경을 담아 쓴다(탭 목록이 비거나 묵던 결함 · 사용자 09-23).
            if let Err(e) = self.project_save() {
                self.sess.status = tf(Msg::ErrProjectFile, &[&e]);
            }
        }
        self.sync_project_panel_opts();
        let has_recent =
            !project::recent_list(self.settings.get("project.recent").unwrap_or("")).is_empty();
        self.project_panel.set_has_recent(has_recent);
        self.project_panel.set_project(
            self.project.name(),
            &self.project.folders,
            self.project.last_selected.clone().as_deref(),
        );
        self.rebuild_menus();
        self.layout();
        self.redraw();
    }

    fn sync_project_panel_opts(&mut self) {
        self.project_panel.set_list_opts(
            self.settings.flag("file.show_hidden"),
            self.settings.flag("file.show_dot"),
            self.settings.int("project.scan_max").max(0) as usize,
            self.settings.int("project.scan_threads").max(0) as usize,
        );
        self.project_panel
            .set_icons(self.settings.flag("project.icons"));
        self.editors
            .set_project_folders(self.project.folders.clone());
        self.project_panel
            .set_tooltip_delay(self.settings.int("ui.tooltip_delay_ms").max(0) as u128);
        self.project_panel
            .set_dblclick_ms(self.settings.int("ui.dblclick_ms").max(0) as u128);
        self.editors
            .set_dblclick_ms(self.settings.int("ui.dblclick_ms").max(0) as u128);
        self.bm_panel
            .set_dblclick_ms(self.settings.int("ui.dblclick_ms").max(0) as u128);
        self.search
            .set_dblclick_ms(self.settings.int("ui.dblclick_ms").max(0) as u128);
    }

    /// 기동 시작 모드(사용자 09-22 · [`startup_project_plan`]): 기본 = **파일 모드**(프로젝트 없음) · 인자 `.nsql-project` =
    /// 프로젝트 모드 · `project.restore_last`가 켜져 있고 **첫 인스턴스**이며 파일 인자가 없을 때만 마지막 프로젝트 복원.
    fn project_startup(&mut self) {
        self.sync_project_panel_opts();
        let plan = startup_project_plan(
            self.arg_project.as_deref(),
            !self.arg_files.is_empty(),
            self.first_instance,
            self.settings.flag("project.restore_last"),
            self.settings.get("project.last").unwrap_or(""),
        );
        let Some((path, from_arg)) = plan else {
            return;
        };
        match project::Project::load(&path) {
            Ok(proj) => {
                if from_arg {
                    // 복원은 기동 마지막(`project_restore` 호출부)에서 한 번 — 여기서 하면 두 번 열린다.
                    self.project_set(proj, false);
                } else {
                    self.project = proj;
                    self.project_panel.set_project(
                        self.project.name(),
                        &self.project.folders,
                        self.project.last_selected.clone().as_deref(),
                    );
                }
            }
            Err(e) => {
                if from_arg {
                    self.sess.status = tf(Msg::ErrProjectFile, &[&e]);
                } else {
                    let _ = self.settings.set("project.last", "");
                    self.persist_settings();
                }
            }
        }
    }

    /// 탐색기가 낸 요청 거두기(열기 · 링크 명령).
    fn project_pump(&mut self) {
        if let Some(id) = self.project_panel.take_command() {
            self.project_cmd(&id);
        }
        if let Some(req) = self.project_panel.take_open() {
            self.project_open_req(req);
        }
    }

    /// 탐색기 클릭 = 미리보기 탭(설정 `project.preview_tab`) · 더블클릭/Enter = 정식 탭.
    /// 큰 파일(`file.async_load_mb` 이상)은 미리보기 없이 보통 열기(자리 탭 + 스레드 · 큰 파일 확인).
    fn project_open_req(&mut self, req: project_panel::OpenReq) {
        let preview = self.settings.flag("project.preview_tab") && !req.permanent;
        if !preview {
            if !self.editors.promote_path(&req.path) {
                self.open_file(&req.path);
            }
            self.set_focus(Focus::Editor);
            self.layout();
            self.redraw();
            return;
        }
        if let Some(i) = self.editors.path_tab(&req.path) {
            self.editors.switch(i);
            self.layout();
            self.redraw();
            return;
        }
        let size = std::fs::metadata(&req.path).map(|m| m.len()).unwrap_or(0);
        let async_at = (self.settings.int("file.async_load_mb").max(1) as u64) << 20;
        if size >= async_at {
            self.open_file(&req.path);
            return;
        }
        match fileload::load(&req.path, "auto", None, true, None) {
            Ok(l) => {
                if let fileload::Body::Prepared(prep) = l.body {
                    let i = self.editors.open_preview(&req.path, prep, l.eol);
                    self.editors.set_encoding(i, l.used);
                    let id = self.editors.tab_id(i);
                    self.ext_track(id);
                    self.sync_grid_tab();
                    self.sync_gate();
                }
            }
            Err(e) => {
                self.sess.status = tf(Msg::StFileReadError, &[&req.path.display().to_string(), &e]);
            }
        }
        self.layout();
        self.redraw();
    }

    /// 우클릭 메뉴 전부 닫기(풀다운과 배타 · 사용자 09-22).
    fn close_context_menus(&mut self) {
        self.editors.close_menus();
        self.status_menu.close();
        self.explorer.close_menu();
        self.grid.close_menu();
        self.panel.close_menu();
    }

    /// 옆 패널은 한 번에 하나 — `keep`만 남기고 닫는다(탐색기·파일 검색·확장·프로젝트).
    fn side_panel_close_others(&mut self, keep: &str) {
        if keep != "view.explorer" && self.explorer.is_visible() {
            self.explorer.set_visible(false);
            let _ = self.settings.set("explorer.visible", "off");
            let _ = self.settings.save();
        }
        if keep != "view.search" && self.search.is_visible() {
            self.search.set_visible(false);
        }
        if keep != "view.extensions" && self.ext_panel.is_visible() {
            self.ext_panel.set_visible(false);
        }
        if keep != "view.project" && self.project_panel.is_visible() {
            self.project_panel.set_visible(false);
        }
        if keep != "view.bookmarks" && self.bm_panel.is_visible() {
            self.bm_panel.set_visible(false);
        }
        if keep != "view.outline" && self.outline_panel.is_visible() {
            self.outline_panel.set_visible(false);
        }
        if matches!(
            self.focus,
            Focus::Explorer | Focus::Search | Focus::Ext | Focus::Project | Focus::Outline
        ) {
            self.set_focus(Focus::Editor);
        }
    }

    /// 확장 패널 목록 다시 만들기 — 설치 기록 + 마지막으로 읽은 카탈로그(네트워크 0).
    fn ext_panel_sync(&mut self) {
        use extensions::manager as mgr;
        let installed = mgr::installed();
        let off = self.ext_disabled_list();
        let mut rows: Vec<ExtRow> = installed
            .iter()
            .map(|r| {
                let cat = self.ext_catalog.iter().find(|(_, p)| p.id == r.id);
                ExtRow {
                    id: r.id.clone(),
                    name: r.name.clone(),
                    version: r.version.clone(),
                    kind: r.kind.as_str().to_string(),
                    summary: cat.map(|(_, p)| p.summary.clone()).unwrap_or_default(),
                    installed: true,
                    enabled: !off.iter().any(|d| d == &r.id),
                    catalog: self.ext_catalog.iter().position(|(_, p)| p.id == r.id),
                    source: cat.map(|(s, _)| s.display()).unwrap_or_default(),
                }
            })
            .collect();
        for (n, (src, p)) in self.ext_catalog.iter().enumerate() {
            if installed.iter().any(|r| r.id == p.id) || rows.iter().any(|r| r.id == p.id) {
                continue;
            }
            rows.push(ExtRow {
                id: p.id.clone(),
                name: p.name.clone(),
                version: p.version.clone(),
                kind: p.kind.as_str().to_string(),
                summary: p.summary.clone(),
                installed: false,
                enabled: false,
                catalog: Some(n),
                source: src.display(),
            });
        }
        let note = if self.ext_fetch_rx.is_some() {
            t(Msg::ExtPanelLoading).to_string()
        } else {
            String::new()
        };
        self.ext_panel.set_rows(rows, note);
        self.redraw();
    }

    /// 저장소 `index.json` 읽기를 **스레드로** 시작(원격은 curl 최대 30초 — UI를 막지 않는다). 패널을 열 때와 ⟳에서만.
    fn ext_fetch_start(&mut self) {
        use extensions::manager as mgr;
        if self.ext_fetch_rx.is_some() {
            return;
        }
        let sources = mgr::sources(&self.settings);
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("ext-index".into())
            .spawn(move || {
                let out: ExtFetch = sources
                    .into_iter()
                    .map(|src| {
                        let mut tr = mgr::Trace::default();
                        let r = mgr::fetch_index_traced(&src, &mut tr);
                        (src, r, tr)
                    })
                    .collect();
                let _ = tx.send(out);
            });
        if spawned.is_ok() {
            self.ext_fetch_rx = Some(rx);
        }
        self.ext_panel_sync();
    }

    /// 읽기 결과 수거(틱) — 카탈로그 교체 · 추적 줄 · 실패는 로그.
    fn ext_fetch_poll(&mut self) {
        let Some(rx) = &self.ext_fetch_rx else {
            return;
        };
        let got = match rx.try_recv() {
            Ok(v) => v,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Vec::new(),
        };
        self.ext_fetch_rx = None;
        self.ext_catalog.clear();
        for (src, r, tr) in got {
            self.ext_trace(tr);
            match r {
                Ok(idx) => {
                    for p in idx.packages {
                        self.ext_catalog.push((src.clone(), p));
                    }
                }
                Err(e) => {
                    let line = tf(Msg::StExtIndexFailed, &[&src.display(), &e]);
                    self.log_win.push(LogEntry::new(LogKind::Error, line));
                }
            }
        }
        self.ext_panel_sync();
    }

    /// 확장 패널이 낸 동작 처리.
    fn ext_panel_actions(&mut self) {
        for a in self.ext_panel.take_actions() {
            match a {
                ExtPanelAction::Refresh => self.ext_fetch_start(),
                ExtPanelAction::Install(n) => self.ext_pick(&format!("ext.install:{n}")),
                ExtPanelAction::Remove(id) => self.ext_pick(&format!("ext.remove:{id}")),
                ExtPanelAction::Enable(id) => self.ext_pick(&format!("ext.enable:{id}")),
                ExtPanelAction::Disable(id) => self.ext_pick(&format!("ext.disable:{id}")),
                ExtPanelAction::Open(row) => self.ext_open_detail(&row),
            }
        }
    }

    /// 확장 상세 = 읽기용 안내 탭(설치본의 메타 사본 → 없으면 저장소에서 · 없으면 목록 정보만).
    fn ext_open_detail(&mut self, row: &ExtRow) {
        use extensions::manager as mgr;
        let mut tr = mgr::Trace::default();
        let meta = mgr::installed_meta(&row.id, &row.version).or_else(|| {
            let (src, sum) = row.catalog.and_then(|n| self.ext_catalog.get(n))?;
            mgr::fetch_meta(src, &sum.dir, &mut tr).ok()
        });
        self.ext_trace(tr);
        let state = t(if !row.installed {
            Msg::ExtStNotInstalled
        } else if row.enabled {
            Msg::ExtStEnabled
        } else {
            Msg::ExtStDisabled
        });
        // 한 줄 설명 = 목록의 것 → 없으면(저장소를 읽기 전에 연 설치본) 메타의 것.
        let summary = if row.summary.is_empty() {
            meta.as_ref().map(|m| m.summary.clone()).unwrap_or_default()
        } else {
            row.summary.clone()
        };
        // ★ 편집기 탭이 아니라 **확장 탭**(전용 뷰 · VS Code식 · 사용자 09-19) — 글 본문은 비어 있고 호스트가 뷰를 그린다.
        let mut fields: Vec<(String, String)> = Vec::new();
        let mut field = |label: Msg, v: &str| {
            if !v.is_empty() {
                fields.push((t(label).to_string(), v.to_string()));
            }
        };
        field(Msg::ExtDetState, state);
        field(Msg::ExtDetVersion, &row.version);
        field(Msg::ExtDetKind, &row.kind);
        field(Msg::ExtDetSource, &row.source);
        let mut description = String::new();
        if let Some(m) = &meta {
            field(Msg::ExtDetAuthor, &m.author);
            field(Msg::ExtDetLicense, &m.license);
            field(Msg::ExtDetHomepage, &m.homepage);
            field(Msg::ExtDetRequires, &m.requires.join(", "));
            field(Msg::ExtDetSettings, &m.settings_prefix);
            let files: Vec<String> = m.files.iter().map(|f| f.path.clone()).collect();
            field(Msg::ExtDetFiles, &files.join(", "));
            description = m.description.clone();
        }
        let mut shown = row.clone();
        shown.summary = summary;
        let key = format!("ext:{}", row.id);
        self.ext_details.insert(
            key.clone(),
            ext_view::ExtDetail {
                row: shown,
                fields,
                description,
            },
        );
        let created = self
            .editors
            .open_view_tab(&key, &format!("Extension: {}", row.name));
        self.set_focus(Focus::Editor);
        if created {
            // 읽을거리 탭 = DB 연결이 필요 없다(No connection).
            let tab = self.editors.active_id();
            self.make_unconnected(tab);
        }
        self.sync_sess();
        self.sync_sess_ui();
        self.layout();
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
            self.sess.status = tf(Msg::StExtRepoBad, &[text.trim()]);
        } else {
            let cur = self
                .settings
                .get("extensions.repositories")
                .unwrap_or("")
                .to_string();
            let next = mgr::list_toggle(&cur, &src.display(), true);
            let _ = self.settings.set("extensions.repositories", &next);
            let _ = self.settings.save();
            self.sess.status = tf(Msg::StExtRepoAdded, &[&src.display()]);
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            self.prefs_sync();
        }
        self.redraw();
    }

    /// 창 기하 기억(사용자 09-17 규칙): 닫힌 보조 창의 마지막 (위치, 크기)를 설정에 · `main`이면 메인 창도(종료 직전).
    fn persist_window_sizes(&mut self, main: bool) {
        let mut changed = false;
        let mut put = |settings: &mut Settings, name: &str, g: Option<((i32, i32), (f64, f64))>| {
            if let Some(((x, y), (w, h))) = g {
                for (key, v) in [
                    (format!("window.{name}_pos"), wingeom::format_pos(x, y)),
                    (format!("window.{name}_size"), wingeom::format_size(w, h)),
                ] {
                    if settings.get(&key) != Some(v.as_str()) {
                        let _ = settings.set(&key, &v);
                        changed = true;
                    }
                }
            }
        };
        let g1 = self.conn_win.take_last();
        let g2 = self.log_win.take_last();
        let g3 = self.txlog_win.take_last();
        let g4 = self.prefs_win.take_last();
        let g5 = self.sessions_win.take_last();
        let g6 = self.mem_win.take_last();
        put(&mut self.settings, "login", g1);
        put(&mut self.settings, "log", g2);
        put(&mut self.settings, "txlog", g3);
        put(&mut self.settings, "prefs", g4);
        put(&mut self.settings, "sessions", g5);
        put(&mut self.settings, "mem", g6);
        if main {
            let g = self
                .window
                .as_ref()
                .and_then(|w| wingeom::outer_pos(w).map(|p| (p, wingeom::logical_size(w))));
            put(&mut self.settings, "main", g);
        }
        if changed {
            let _ = self.settings.save();
            self.apply_window_sizes();
        }
    }

    /// 설정의 기억된 기하를 보조 창의 메모로(열 때 같은 모니터면 그대로 · 아니면 기본 규칙).
    fn apply_window_sizes(&mut self) {
        let memo = |s: &Settings, name: &str| wingeom::Memo {
            pos: s
                .get(&format!("window.{name}_pos"))
                .and_then(wingeom::parse_pos),
            size: s
                .get(&format!("window.{name}_size"))
                .and_then(wingeom::parse_size),
        };
        let (a, b, c, d) = (
            memo(&self.settings, "login"),
            memo(&self.settings, "log"),
            memo(&self.settings, "txlog"),
            memo(&self.settings, "prefs"),
        );
        self.conn_win.set_memo(a);
        self.log_win.set_memo(b);
        self.txlog_win.set_memo(c);
        self.prefs_win.set_memo(d);
        let e = memo(&self.settings, "sessions");
        self.sessions_win.set_memo(e);
        let f = memo(&self.settings, "mem");
        self.mem_win.set_memo(f);
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
        self.editors
            .set_split_max(self.settings.int("editor.split_max"));
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

    // ───────────────────────── 세션 컨텍스트(docs/52) ─────────────────────────

    fn session_mode(&self) -> SessionMode {
        SessionMode::parse(self.settings.get("session.mode"))
    }

    /// 접속 창(로그인)이 붙이는 세션 — 개별 모드 = 활성 탭의 세션 · 공유 모드 = `login_plan`
    /// (같은 서버 = 중복 없이 그 세션 · 놀고 있는 세션 재활용 · 아니면 **추가** · 상한). None = 지금은 못 한다(안내는 여기서).
    fn login_place(&mut self, spec: &ConnectSpec) -> Option<u64> {
        if self.session_mode() == SessionMode::PerEditor {
            return Some(self.sess_id_for_tab(self.editors.active_id()));
        }
        let views: Vec<sessions::SharedView> = self
            .all_sess()
            .filter(|s| !s.is_private() && !s.closing)
            .map(|s| sessions::SharedView {
                id: s.id,
                same_server: s
                    .spec
                    .as_ref()
                    .or(s.last_spec.as_ref())
                    .is_some_and(|have| worker::same_server(have, spec)),
                connected: s.connected,
                blocked: s.blocked(),
                idle_closed: s.idle_closed,
                bound_tabs: self.bound_tabs(s.id),
            })
            .collect();
        let max = self.settings.int("session.max_shared").max(1) as usize;
        match sessions::login_plan(&views, max) {
            sessions::LoginPlan::Use(id) | sessions::LoginPlan::Recycle(id) => Some(id),
            sessions::LoginPlan::New => Some(self.new_shared()),
            sessions::LoginPlan::Busy(_) => {
                self.sess.status = t(Msg::StRunning).into();
                self.fail_login_attempt(t(Msg::StRunning));
                None
            }
            sessions::LoginPlan::Limit => {
                let m = tf(Msg::StSessSharedLimit, &[&max.to_string()]);
                self.log_win.push(LogEntry::new(LogKind::Error, m.clone()));
                self.fail_login_attempt(&m);
                self.sess.status = m;
                None
            }
        }
    }

    /// 접속 시도를 워커에 보내지 못했다 — 접속 창의 "접속 중" 표시를 실패로 되돌린다.
    fn fail_login_attempt(&mut self, why: &str) {
        let name = self.panel_op_name();
        self.conn_win.set_connect_mark(&name, None);
        self.set_panel_result(&name, ConnState::Failed(why.to_string()));
        self.conn_win.redraw();
    }

    /// 모든 세션(지금 것 + 잠든 것).
    fn all_sess(&self) -> impl Iterator<Item = &Sess> {
        std::iter::once(&self.sess).chain(self.parked.iter())
    }

    fn sess_by_id(&self, id: u64) -> Option<&Sess> {
        self.all_sess().find(|s| s.id == id)
    }

    /// 탭이 쓰는 세션 id — ① 그 탭의 전용 세션(닫는 중이 아닌) ② 탭이 고른 공유 세션 ③ 기본 공유 세션.
    fn sess_id_for_tab(&self, tab: u64) -> u64 {
        let private = self
            .all_sess()
            .find(|s| s.owner == Some(tab) && !s.closing)
            .map(|s| s.id);
        let bound = self.tab_bind.get(&tab).copied();
        let alive = bound.is_some_and(|id| {
            self.all_sess()
                .any(|s| s.id == id && !s.is_private() && !s.closing)
        });
        sessions::route_tab(private, bound, alive, self.default_shared)
    }

    /// 공유 세션에 묶인 탭 수.
    fn bound_tabs(&self, id: u64) -> usize {
        self.tab_bind.values().filter(|v| **v == id).count()
    }

    /// 공유 세션 하나 추가(워커 하나) — 기존 연결은 그대로 둔다(docs/52 §2-1).
    fn new_shared(&mut self) -> u64 {
        let (w, ev) = self.spawn_worker();
        let id = self.next_sess_id;
        self.next_sess_id += 1;
        let s = Sess::new(id, None, w, ev, DEFAULT_DIALECT);
        s.worker
            .send(worker::Cmd::GlobalVars(self.global_vars.clone()));
        self.parked.push(s);
        id
    }

    /// 공유 연결 활성화 — 묶이지 않은 탭과 탐색기·접속 창 표시가 이 연결을 따른다.
    fn activate_shared(&mut self, id: u64) {
        let Some((spec, profile, desc, connected)) =
            self.sess_by_id(id).filter(|s| !s.is_private()).map(|s| {
                (
                    s.spec.clone(),
                    s.profile.clone(),
                    s.desc.clone(),
                    s.connected,
                )
            })
        else {
            return;
        };
        self.default_shared = id;
        self.primary_sess = id;
        self.default_spec = spec.clone();
        self.editors
            .set_conn_desc(if connected { desc } else { String::new() });
        if connected {
            if let Some(spec) = spec {
                self.conn_win.mark_connected(&profile);
                self.conn_win.clear_connect_marks();
                self.conn_win
                    .set_connect_mark(&profile, Some(ConnectMark::Connected));
                self.explorer.connect(&spec, &profile, true, false);
            }
        }
        self.sync_sess();
        self.sync_sess_ui();
        self.redraw();
    }

    /// 공유 세션 해제(툴바 드롭다운 · 메뉴) — 탐색기가 이 연결을 따르고 있었으면 함께 닫고, 묶인 탭은 끊김 표식으로 남는다.
    fn disconnect_shared(&mut self, id: u64) {
        if self.sess.id == id {
            // 세션 창·탐색기에서 고른 해제 = 이미 연결 단위로 고른 것이라 "다른 탭도 씀" 확인을 다시 묻지 않는다.
            self.disconnect_current();
            return;
        }
        self.with_sess(id, |a| {
            if !a.sess.tx_pending.is_empty() {
                // 다른 탭의 세션 — 확인 팝업은 그 탭을 앞에 두고 답해야 한다.
                a.sess.status = t(Msg::StSessTxPending).into();
                a.log_win
                    .push(LogEntry::new(LogKind::Error, a.sess.status.clone()));
                return;
            }
            a.disconnect_force();
        });
        self.sess.status = t(Msg::StDisconnected).into();
        self.reap_sessions();
        self.sync_sess_ui();
        self.redraw();
    }

    /// 모든 연결 해제(툴바 Disconnect 본체): 지금 세션에 미커밋이 있으면 묻고(DR-30) · 다른 세션은 미커밋이 있으면 남긴다(로그) ·
    /// 나머지는 전부 끊는다 → 전 탭이 미연결(공유 탭은 끊긴 공유 연결에 묶인 채 · 전용 탭은 세션 규칙대로).
    fn disconnect_all(&mut self) {
        if !self.sess.tx_pending.is_empty() {
            self.tx_after = Some(TxAfter::Disconnect);
            self.open_tx_guard(Msg::MnTxCommitDisconnect, Msg::MnTxRollbackDisconnect);
            self.redraw();
            return;
        }
        let ids: Vec<u64> = self
            .all_sess()
            .filter(|s| !s.closing && (s.connected || s.busy || s.idle_closed))
            .map(|s| s.id)
            .collect();
        let mut skipped = 0usize;
        for sid in ids {
            if self
                .sess_by_id(sid)
                .is_some_and(|s| !s.tx_pending.is_empty())
            {
                skipped += 1;
                continue;
            }
            self.disconnect_session(sid);
        }
        let note = if skipped > 0 {
            tf(Msg::StSessSkippedPending, &[&skipped.to_string()])
        } else {
            String::new()
        };
        self.sess.status = tf(Msg::StSessAllDropped, &[&note]);
        self.log_win
            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
        self.sync_sess_ui();
        self.redraw();
    }

    /// 세션 하나 해제 — 전용/개별 탭 세션이면 그 탭의 규칙(공유 복귀 · 개별 모드 = 끊김)으로, 공유 연결이면 공유 해제로.
    fn disconnect_session(&mut self, id: u64) {
        match self.sess_by_id(id).map(|s| s.owner) {
            Some(Some(tab)) => self.disconnect_private(tab),
            Some(None) => self.disconnect_shared(id),
            None => {}
        }
    }

    fn disconnect_pick(&mut self, id: &str) {
        if id == "conn.drop_all" {
            let ids: Vec<u64> = self.all_sess().map(|s| s.id).collect();
            for sid in ids {
                self.mark_disc(sid, sessions::DiscPath::SessionsWin);
            }
            self.disconnect_all();
        } else if let Some(sid) = id.strip_prefix("conn.use:").and_then(|v| v.parse().ok()) {
            self.activate_shared(sid);
        } else if let Some(sid) = id.strip_prefix("conn.again:").and_then(|v| v.parse().ok()) {
            // 세션 관리자의 "다시 접속" = 명시적 요청(설정 `connect.reconnect_same`).
            let again = self.settings.flag("connect.reconnect_same");
            let fallback = self.default_spec.clone();
            self.with_sess(sid, |a| {
                if let Some(spec) = a.sess.spec.clone().or(fallback) {
                    a.connect_quietly(spec, again);
                }
            });
            self.sync_sess_ui();
        } else if let Some(sid) = id.strip_prefix("conn.drop:").and_then(|v| v.parse().ok()) {
            self.mark_disc(sid, sessions::DiscPath::SessionsWin);
            self.disconnect_session(sid);
        }
    }

    /// `id` 세션을 잠시 `self.sess` 자리에 놓고 `f`를 돈다(끝나면 되돌린다). 기존 코드는 늘 `self.sess`만 보므로
    /// 잠든 세션의 이벤트 처리·접속 창의 공유 세션 조작을 같은 코드로 한다.
    fn with_sess<R>(&mut self, id: u64, f: impl FnOnce(&mut Self) -> R) -> Option<R> {
        if self.sess.id == id {
            return Some(f(self));
        }
        let i = self.parked.iter().position(|s| s.id == id)?;
        let home = self.sess.id;
        std::mem::swap(&mut self.sess, &mut self.parked[i]);
        let r = f(self);
        if let Some(j) = self.parked.iter().position(|s| s.id == home) {
            std::mem::swap(&mut self.sess, &mut self.parked[j]);
        }
        Some(r)
    }

    /// 활성 편집기 탭의 세션을 `self.sess`로 — 탭 전환·세션 생성/해제 뒤에 부른다(이벤트 뒤 · 페인트 전 · 실행 직전).
    /// 개별 모드면 처음 활성화된 탭에 전용 세션을 만들고 기본 접속 정보로 붙인다(열기만 하고 안 본 탭은 접속하지 않는다 = 부하 0).
    fn sync_sess(&mut self) {
        let tab = self.editors.active_id();
        if tab == 0 {
            return;
        }
        // 상한에 닿았으면 조용히 공유 세션을 쓴다(표식 없음 = 공유) — 페인트마다 불리므로 여기서 경고를 내지 않는다.
        let room = self
            .all_sess()
            .filter(|s| s.is_private() && !s.closing)
            .count()
            < self.settings.int("session.max_private").max(0) as usize;
        if room
            && self.session_mode() == SessionMode::PerEditor
            && !self.all_sess().any(|s| s.owner == Some(tab) && !s.closing)
        {
            let spec = self.default_spec.clone();
            if let Some(id) = self.new_private(tab) {
                if let Some(spec) = spec {
                    self.with_sess(id, |a| a.connect_quietly(spec, false));
                }
            }
        }
        let want = self.sess_id_for_tab(tab);
        // 공유 모드의 새 탭 = **그때의 활성 공유 연결**에 바로 묶인다(사용자 09-18) — 뒤에 활성 연결을 바꿔도 이 탭은 그대로.
        if !self.all_sess().any(|s| s.owner == Some(tab) && !s.closing)
            && !self.tab_bind.contains_key(&tab)
        {
            self.tab_bind.insert(tab, want);
            // 함께 쓰는 탭 수가 바뀌었다 → 해제 버튼 배지·툴팁을 바로(사용자 09-19 "탭이 추가돼도 숫자가 안 는다").
            self.sess_ui_dirty = true;
        }
        if self.sess.id != want {
            if let Some(i) = self.parked.iter().position(|s| s.id == want) {
                std::mem::swap(&mut self.sess, &mut self.parked[i]);
                self.sync_sess_ui();
                self.redraw();
            }
        }
    }

    /// 탭을 **미연결**로(사용자 09-18 · 표식 메뉴 "미연결"): 전용 세션이 있으면 그 세션을 끊어 미연결로 남기고 ·
    /// 공유 탭이면 접속 없는 전용 자리(세션 객체만)를 만들어 어떤 연결에도 묶이지 않게 한다.
    fn make_unconnected(&mut self, tab: u64) {
        let private = self
            .all_sess()
            .find(|s| s.owner == Some(tab) && !s.closing)
            .map(|s| s.id);
        if let Some(id) = private {
            if self.sess_by_id(id).is_some_and(|s| s.disc_path.is_none()) {
                self.mark_disc(id, sessions::DiscPath::Badge);
            }
            self.disconnect_private_as(tab, true);
            return;
        }
        if let Some(id) = self.new_private(tab) {
            self.with_sess(id, |a| {
                a.sess.user_disconnected = true;
                a.sess.status = t(Msg::StSessUnconnected).into();
            });
            self.freeze_results_of(tab);
            self.sync_sess();
            self.sync_sess_ui();
            self.redraw();
        }
    }

    /// 새 탭 규칙(사용자 09-18): 공유 모드 = 활성 공유 연결(`sync_sess`가 묶는다) · 개별 모드 = **미연결 자리 + 접속 창을 바로**
    /// (접속하지 않고 닫으면 미연결 그대로).
    fn on_new_tab(&mut self) {
        if self.session_mode() != SessionMode::PerEditor {
            return;
        }
        let tab = self.editors.active_id();
        if let Some(id) = self.new_private(tab) {
            self.with_sess(id, |a| a.sess.user_disconnected = true);
            self.sync_sess();
            self.sync_sess_ui();
            self.open_conn = true;
        }
    }

    /// 탭 전용 세션 하나(워커 스레드 하나) — 상한 `session.max_private`(기본 사상: 1 인스턴스 · 1 서버 · 1 계정이라 예외는 아껴 쓴다).
    fn new_private(&mut self, tab: u64) -> Option<u64> {
        let max = self.settings.int("session.max_private").max(0) as usize;
        let n = self
            .all_sess()
            .filter(|s| s.is_private() && !s.closing)
            .count();
        if n >= max {
            self.sess.status = tf(Msg::StSessLimit, &[&max.to_string()]);
            self.log_win
                .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
            return None;
        }
        let (w, ev) = self.spawn_worker();
        let id = self.next_sess_id;
        self.next_sess_id += 1;
        let s = Sess::new(id, Some(tab), w, ev, DEFAULT_DIALECT);
        s.worker
            .send(worker::Cmd::GlobalVars(self.global_vars.clone()));
        self.parked.push(s);
        Some(id)
    }

    /// 접속 창을 거치지 않는 접속(개별 모드의 탭 자동 접속 · 유휴 해제 뒤 재접속) — 시도 큐·접속 창 표시는 건드리지 않는다.
    /// `reconnect_same` = 같은 서버에 이미 붙어 있어도 끊고 다시(사용자의 **명시적** 재접속 = 설정 `connect.reconnect_same`).
    fn connect_quietly(&mut self, spec: ConnectSpec, reconnect_same: bool) {
        self.sess.busy = true;
        self.sess.user_disconnected = false;
        self.sess.idle_closed = false;
        self.sess.status = tf(Msg::StConnecting, &[&spec.redacted()]);
        self.sess.spec = Some(spec.clone());
        self.sess.touch();
        self.sess.worker.send(worker::Cmd::ConnectSpec {
            spec,
            reconnect_same,
        });
    }

    /// 모든 세션의 워커 응답을 처리한다 — 잠든 세션은 잠시 앞으로 꺼내 같은 코드로.
    fn drain_all(&mut self) {
        self.drain_events();
        let ids: Vec<u64> = self.parked.iter().map(|s| s.id).collect();
        for id in ids {
            self.with_sess(id, |a| a.drain_events());
        }
        self.reap_sessions();
        self.sync_sess();
        self.sync_sess_ui();
    }

    /// 닫는 중인 세션 · 주인 탭이 닫힌 전용 세션을 거둔다(워커에 Quit = 커밋 없이 닫지 않는다: 미커밋은 닫기 전에 이미 물었다).
    fn reap_sessions(&mut self) {
        let alive = self.editors.tab_ids();
        for s in &mut self.parked {
            if s.owner.is_some_and(|t| !alive.contains(&t)) {
                s.closing = true;
            }
        }
        // 지금 세션이 닫는 중이면 먼저 자리를 비킨다(공유 세션으로).
        if self.sess.closing || self.sess.owner.is_some_and(|t| !alive.contains(&t)) {
            self.sess.closing = true;
            let to = self.default_shared;
            if let Some(i) = self.parked.iter().position(|s| s.id == to) {
                std::mem::swap(&mut self.sess, &mut self.parked[i]);
            }
        }
        let before = self.parked.len();
        // 거두는 세션의 해제 로그(탭 닫기 · 다른 연결로 전환 · 스크립트 DISCONNECT 뒤 거둠) — 아직 접속돼 있던 것만.
        let closing: Vec<u64> = self
            .parked
            .iter()
            .filter(|s| s.closing && s.id != SHARED && (s.connected || s.busy))
            .map(|s| s.id)
            .collect();
        for id in closing {
            let fallback =
                if alive.contains(&self.sess_by_id(id).and_then(|s| s.owner).unwrap_or(0)) {
                    sessions::DiscPath::Switch
                } else {
                    sessions::DiscPath::TabClose
                };
            self.with_sess(id, |a| a.log_disconnect(fallback));
        }
        self.parked.retain(|s| {
            if s.closing && s.id != SHARED {
                // 실행 중이면 먼저 취소를 보낸다(닫힌 탭의 질의가 서버에 남아 돌지 않게).
                if s.blocked() {
                    let _ = s.worker.cancel_run();
                }
                s.worker.send(worker::Cmd::Disconnect);
                s.worker.send(worker::Cmd::Quit);
                false
            } else {
                true
            }
        });
        self.tab_bind.retain(|t, _| alive.contains(t));
        // 끊긴 채 아무도 안 쓰는 공유 세션 객체(추가 접속 실패 · 해제 뒤)는 거둔다 — 활성 연결·묶인 탭이 있는 것은 남긴다.
        let default = self.default_shared;
        let binds: Vec<u64> = self.tab_bind.values().copied().collect();
        self.parked.retain(|s| {
            let n = binds.iter().filter(|b| **b == s.id).count();
            if !s.is_private()
                && sessions::reap_shared(
                    s.connected,
                    s.blocked(),
                    s.idle_closed,
                    s.id == default,
                    n,
                )
            {
                s.worker.send(worker::Cmd::Quit);
                false
            } else {
                true
            }
        });
        if self.parked.len() != before {
            self.sync_sess_ui();
        }
    }

    /// 지금 세션이 붙은 서버의 탐색기를 확보한다 — 스펙이 프로필 이름뿐이면 저장소에서 완성해 둔다(메타 세션도 같은 자격으로
    /// 붙는다 · 같은 서버 판정·재접속에도 같은 스펙을 쓴다). 인라인 스펙은 이미 자격을 갖고 있다(비밀번호 필수 · 09-19).
    fn explorer_attach(&mut self) {
        let Some(spec) = self.sess.spec.clone() else {
            return;
        };
        let (full, name) = match sessions::bare_profile_name(&spec) {
            Some(n) => match Vault::open_default().and_then(|v| v.resolve(n)) {
                Ok(Some(f)) => (f, n.to_string()),
                _ => return,
            },
            None => (spec, self.sess.profile.clone()),
        };
        self.sess.spec = Some(full.clone());
        let show = self.sess_id_for_tab(self.editors.active_id()) == self.sess.id;
        // ★ 일회성 비밀번호(입력 창으로 받은 것): 세션 스펙에는 **넣지 않는다**. 탐색기 메타 세션이 같은 자격으로 붙도록 이 호출에만
        //   빌려주고 바로 지운다(메타 스레드도 접속 뒤 지운다 · 유휴 회수 없음).
        let once = match self.pw_once.take() {
            Some((sid, secret, _)) if sid == self.sess.id && full.password.is_none() => {
                Some(secret)
            }
            other => {
                self.pw_once = other;
                None
            }
        };
        match once {
            Some(secret) => {
                let mut lend = full.clone();
                lend.password = Some(secret.expose().to_string());
                drop(secret);
                self.explorer.connect(&lend, &name, show, true);
                nsql_core::secret::wipe_opt(&mut lend.password);
            }
            None => self.explorer.connect(&full, &name, show, false),
        }
    }

    /// 세션 상태가 바뀌었을 때 화면의 세 층을 한 번에 맞춘다: 탭 표식·설명 · 통제(툴바) · 트랜잭션 · 해제 버튼.
    fn sync_sess_ui(&mut self) {
        let mut info: HashMap<u64, (nexa_ctl::TabBadge, String)> = HashMap::new();
        // 공유 연결이 둘 이상이면 공유 탭에도 표식(어느 서버인지 · 표식 메뉴로 고른다).
        let multi = self
            .all_sess()
            .filter(|s| !s.is_private() && !s.closing)
            .count()
            > 1;
        for tab in self.editors.tab_ids() {
            let Some(s) = self.sess_by_id(self.sess_id_for_tab(tab)) else {
                continue;
            };
            let badge = match sessions::badge_kind(s.is_private(), multi, s.connected && !s.broken)
            {
                sessions::BadgeKind::Private => nexa_ctl::TabBadge::Link,
                sessions::BadgeKind::Shared => nexa_ctl::TabBadge::Shared,
                sessions::BadgeKind::Off => nexa_ctl::TabBadge::LinkOff,
            };
            info.insert(tab, (badge, s.desc.clone()));
        }
        self.editors.set_sess_info(info);
        // 탐색기 참조 수 = 지금 붙어 있는 세션들의 서버(0이 된 서버는 메타 접속만 닫고 트리는 남긴다) · 활성 탭의 서버를 앞으로.
        let live: Vec<ConnectSpec> = self
            .all_sess()
            .filter(|s| (s.connected || s.idle_closed) && !s.closing)
            .filter_map(|s| s.spec.clone())
            .collect();
        self.explorer.sync_refs(&live);
        let cur = self.sess.spec.clone();
        self.explorer.show_for(cur.as_ref());
        self.sync_gate();
        self.sync_tx_ui();
        self.sessions_win.redraw();
        // Disconnect = **지금 탭의 연결**이 있을 때만(종전 규칙).
        let cur = self.sess.connected || self.sess.busy;
        self.sync_disconnect_btn(cur);
    }

    /// ★ 통제의 단일 출구(§3): 지금 세션이 막혔으면 실행 계열 진입점을 한꺼번에 끄고, 풀리면 한꺼번에 켠다.
    /// 툴바 · 결과 도구줄(추가 페치·전체 조회·건수)이 같은 판정을 본다 · 값이 바뀔 때만 쓴다.
    fn sync_gate(&mut self) {
        let blocked = !self.gate().run_other;
        // 활성 그리드는 탭 전환으로 바뀌므로 매번 알린다(그리드가 바뀔 때만 도구줄을 다시 맞춘다).
        // 연결 전에는 서버로 나가는 결과 도구줄 버튼(새로고침·전체 조회·건수)을 전부 끈다(사용자 09-19) — 유휴 닫힘은 조용히 재접속하므로 연결로.
        self.grid
            .set_session_connected(self.sess.connected || self.sess.idle_closed);
        self.grid.set_session_blocked(blocked);
        if self.gate_shown == Some(blocked) {
            return;
        }
        self.gate_shown = Some(blocked);
        let mut inv = Invalidations::default();
        for id in ["run.all", "run.explain"] {
            self.tool_dock.set_item_enabled(id, !blocked, &mut inv);
        }
        // 메뉴바 Run 메뉴도 같은 판정(비활성 항목 · 09-19 검토).
        self.rebuild_menus();
        // 문장 실행(단일 커서 조건과 겹침) · Commit/Rollback(대기 문장 조건과 겹침)은 각자의 동기화가 통제 상태를 함께 본다.
        self.sync_run_stmt_button();
        self.sync_tx_ui();
        self.redraw();
    }

    /// 지금 세션의 통제 상태가 화면에 내는 값(순수 판정 `sessions::gate_view` · MC/DC 표 D6).
    fn gate(&self) -> sessions::GateView {
        sessions::gate_view(
            self.sess.busy,
            self.sess.aux,
            self.editors.cur().has_multi(),
            !self.sess.tx_pending.is_empty(),
        )
    }

    /// 실행 계열 진입점의 공통 문지기 — 막혔으면 상태줄에 알리고 false.
    fn gate_open(&mut self) -> bool {
        self.sync_sess();
        if self.sess.blocked() {
            self.sess.status = t(Msg::StRunning).into();
            self.redraw();
            return false;
        }
        true
    }

    /// 유휴 세션 점검(§6 · 30초 간격) — 닫아도 안전한 세션만 닫고 스펙은 남긴다(다음 실행 때 조용히 재접속).
    fn idle_tick(&mut self, now: Instant) {
        if now < self.idle_next {
            return;
        }
        self.idle_next = now + Duration::from_secs(30);
        let limit = self.settings.int("session.idle_secs").max(0) as u64;
        if limit == 0 {
            return;
        }
        // 탐색기 메타 세션도 같은 한도로 유휴 회수(트리는 그대로 · 다음 펼침 때 메타 스레드가 다시 연다).
        self.explorer.idle_tick(limit);
        let include_shared = self.settings.flag("session.idle_shared");
        let manual = !self.settings.flag("session.autocommit");
        let due: Vec<u64> = self
            .all_sess()
            .filter(|s| {
                sessions::idle_action(sessions::IdleInput {
                    dialect: s.dialect,
                    private: s.is_private(),
                    connected: s.connected && s.spec.is_some(),
                    blocked: s.blocked(),
                    // 수동 커밋이면 조회만 했어도 트랜잭션이 열려 있을 수 있다 → 닫지 않는다.
                    tx_open: !s.tx_pending.is_empty() || s.tx_dirty || (manual && s.tx_read),
                    stateful: s.stateful,
                    idle: now.saturating_duration_since(s.last_used),
                    limit_secs: limit,
                    include_shared,
                }) == sessions::IdleAction::Close
            })
            .map(|s| s.id)
            .collect();
        for id in due {
            self.with_sess(id, |a| {
                // 닫기(commit + logoff)가 죽은 소켓에 갇혀도 세션의 큐가 막히지 않게 — 옛 워커에 Disconnect를 남기고 새 워커로(docs/53 §4-8).
                a.sess.disc_path = Some(sessions::DiscPath::Idle);
                a.abandon_worker();
                let m = tf(Msg::StSessIdleClosed, &[&a.sess.desc]);
                a.log_win.push(LogEntry::new(LogKind::Info, m.clone()));
                a.sess.status = m;
            });
        }
        self.sync_sess_ui();
    }

    /// 실행 직전: 유휴로 닫힌 세션이면 같은 스펙으로 먼저 다시 붙는다(워커는 순차라 뒤따르는 실행은 접속 뒤에 돈다).
    fn wake_if_idle(&mut self) {
        if self.sess.idle_closed {
            if let Some(spec) = self.sess.spec.clone() {
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    tf(Msg::StReconnecting, &[&spec.redacted()]),
                ));
                self.connect_quietly(spec, false);
                // 이 접속의 완료 신호는 뒤따르는 작업의 busy를 풀면 안 된다.
                self.sess.skip_done += 1;
            }
        }
    }

    /// 실행할 스크립트를 보고 세션 배치를 정한다(§4). 반환 false = 실행하지 않는다(이미 처리했거나 거부).
    ///  - `CONNECT 대상`이 먼저 나오면: 이 탭의 전용 세션으로(없으면 만든다) — 스크립트 전체가 그 세션에서 돈다.
    ///  - `DISCONNECT`가 먼저 나오고 이 탭이 전용 세션이면: 그 세션을 닫는다(공유 모드 = 공유 세션 복귀 · 개별 모드 = 실행 불가 상태).
    ///    공유 세션 탭의 `DISCONNECT`는 종전대로 워커가 처리한다(공유 세션 해제).
    fn place_run(&mut self, src: &mut String) -> bool {
        let tab = self.editors.active_id();
        let intent = sessions::connect_intent(src);
        let plan = sessions::placement(
            intent.as_ref(),
            self.sess.is_private(),
            self.settings.flag("session.private_connect"),
            intent.is_some() && sessions::statements_before_connect(src),
        );
        match plan {
            sessions::Placement::Refuse => {
                // D-99: 새 전용 세션에는 아직 접속이 없다 — 앞 문장이 조용히 "not connected"로 실패하게 두지 않는다.
                self.sess.status = t(Msg::StSessConnectFirst).into();
                self.log_win
                    .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                self.toasts.push(
                    toast::ToastKind::Error,
                    "CONNECT".to_string(),
                    self.sess.status.clone(),
                );
                self.redraw();
                false
            }
            sessions::Placement::NewPrivate | sessions::Placement::Retarget => {
                if plan == sessions::Placement::NewPrivate {
                    let Some(id) = self.new_private(tab) else {
                        self.redraw();
                        return false;
                    };
                    // 공유 세션의 결과는 새 세션에서 이어 받을 수 없다 → 이 탭의 옛 결과는 "더 있음"을 내린다.
                    self.freeze_tab_results();
                    self.sync_sess();
                    if self.sess.id != id {
                        return false;
                    }
                }
                if let Some(ConnectIntent::Connect(spec)) = intent {
                    // 전용 탭의 CONNECT가 **같은 서버·계정**이면(동일성 = `same_server`) 설정 `connect.reconnect_same`에 따라:
                    //   끔 = 기존 접속 유지(CONNECT 명령만 지우고 나머지 실행) · 켬 = 명시적 재접속(끊고 다시).
                    // ★ 프로필 이름(`CONNECT M4PLAN`)은 저장소에서 **완성한 스펙으로 비교**한다 — 이름뿐인 스펙은 어떤 접속과도
                    //   같지 않아서 되풀이할 때마다 다시 접속했다(사용자 09-21). 세션의 스펙은 접속 뒤 이미 완성본이다(`explorer_attach`).
                    let target = sessions::bare_profile_name(&spec)
                        .and_then(|n| {
                            Vault::open_default()
                                .and_then(|v| v.resolve(n))
                                .ok()
                                .flatten()
                        })
                        .unwrap_or_else(|| spec.clone());
                    let same = plan == sessions::Placement::Retarget
                        && self.sess.connected
                        && !self.sess.broken
                        && self
                            .sess
                            .spec
                            .as_ref()
                            .is_some_and(|have| worker::same_server(have, &target));
                    //   ★ 단, **자격이 바뀐** CONNECT(`user:@host` · 다른 비밀번호)는 유지가 아니라 다시 접속이다(사용자 09-21).
                    let reconnect_setting = self.settings.flag("connect.reconnect_same");
                    let cred_changed = same && worker::credential_changed(&target, DEFAULT_DIALECT);
                    let keep = sessions::keep_same_session(same, reconnect_setting, cred_changed);
                    if keep {
                        *src = sessions::strip_first_connect(src);
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::StSessSameKept, &[&self.sess.desc]),
                        ));
                    } else {
                        // ★ 왜 다시 접속하는지 한 줄(사용자 09-25 "이미 접속됐는데 다시 접속" 진단): 계정·서버 다름 / 자격 변경 /
                        //   설정 / 접속 안 됨.
                        let why = if !self.sess.connected || self.sess.broken {
                            Msg::RsnNotConnected
                        } else if !same {
                            Msg::RsnAccountDiffers
                        } else if cred_changed {
                            Msg::RsnCredChanged
                        } else {
                            Msg::RsnReconnectSetting
                        };
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::StSessRetarget, &[&target.redacted(), t(why)]),
                        ));
                        self.sess.spec = Some(spec);
                    }
                }
                self.sess.user_disconnected = false;
                self.sess.idle_closed = false;
                true
            }
            sessions::Placement::ClosePrivate => {
                self.disconnect_private(tab);
                false
            }
            sessions::Placement::Run => {
                // 공유 연결이 여럿일 수 있다 → 탭은 **처음 실행한 연결에 묶인다**(활성 연결을 바꿔도 이 탭은 엉뚱한 서버로 가지 않는다).
                if !self.sess.is_private() && !self.tab_bind.contains_key(&tab) {
                    self.tab_bind.insert(tab, self.sess.id);
                    self.sess_ui_dirty = true;
                }
                // `session.private_connect = off`의 CONNECT = 이 세션이 대상을 바꾼다 → 탐색기·재접속이 새 대상을 알게.
                if let Some(ConnectIntent::Connect(spec)) = intent {
                    self.sess.spec = Some(spec);
                    self.sess.profile.clear();
                }
                true
            }
        }
    }

    /// 활성 편집기 탭의 결과 그리드 전부에서 "더 있음"을 내린다(세션이 바뀌어 이어 받기가 뜻을 잃을 때).
    fn freeze_tab_results(&mut self) {
        let tab = self.editors.active_id();
        self.freeze_results_of(tab);
    }

    fn freeze_results_of(&mut self, tab: u64) {
        if tab == self.panel_editor {
            self.grid.set_more(false);
            for t in &mut self.panel.tabs {
                t.grid.set_more(false);
            }
        } else if let Some(p) = self.panels.get_mut(&tab) {
            for t in &mut p.tabs {
                t.grid.set_more(false);
            }
        }
    }

    /// 탭의 전용 세션 해제 — 미커밋이 있으면 먼저 묻는다. 공유 모드 = 세션을 거두고 공유 세션으로 복귀 ·
    /// 개별 모드 = 세션은 남기되 끊긴 상태(그 탭은 다시 접속할 때까지 실행 불가).
    fn disconnect_private(&mut self, tab: u64) {
        let keep = self.session_mode() == SessionMode::PerEditor;
        self.disconnect_private_as(tab, keep);
    }

    /// `keep` = 세션 객체를 **미연결**로 남긴다(개별 모드 · 표식 메뉴 "미연결") · 아니면 거두고 공유 세션으로 복귀(공유 모드).
    fn disconnect_private_as(&mut self, tab: u64, keep: bool) {
        let Some(id) = self
            .all_sess()
            .find(|s| s.owner == Some(tab) && !s.closing)
            .map(|s| s.id)
        else {
            return;
        };
        let per_editor = keep;
        // 미커밋이 있으면 먼저 묻는다(잃는 순간만 모달 · DR-30) — 답한 뒤 `disconnect_force`가 다시 여기로 온다.
        if self.sess.id == id && !self.sess.tx_pending.is_empty() {
            self.tx_after = Some(TxAfter::Disconnect);
            self.open_tx_guard(Msg::MnTxCommitDisconnect, Msg::MnTxRollbackDisconnect);
            self.redraw();
            return;
        }
        self.with_sess(id, |a| {
            if !a.sess.tx_pending.is_empty() {
                a.sess.status = t(Msg::StSessTxPending).into();
                a.log_win
                    .push(LogEntry::new(LogKind::Error, a.sess.status.clone()));
                return;
            }
            let stuck = a.sess.busy;
            a.log_disconnect(sessions::DiscPath::Badge);
            a.sess.worker.send(worker::Cmd::Disconnect);
            a.editors.set_running(a.sess.run_editor, false);
            if per_editor {
                // 갇힌 워커는 버리고 새 워커로(즉시 해제 규약 · 09-16) — 세션 객체는 남는다.
                if stuck {
                    let (w, ev) = a.spawn_worker();
                    a.sess.worker = w;
                    a.sess.events = ev;
                }
                a.sess.busy = false;
                a.sess.aux = 0;
                a.sess.connected = false;
                a.sess.user_disconnected = true;
                a.sess.idle_closed = false;
                a.sess.status = t(Msg::StSessDisconnected).into();
            } else {
                a.sess.closing = true;
            }
            a.tx_close(TxOutcome::Lost);
            let m = tf(Msg::StSessClosed, &[&a.sess.desc]);
            a.log_win.push(LogEntry::new(LogKind::Info, m));
        });
        if !per_editor {
            self.freeze_tab_results();
        }
        self.reap_sessions();
        self.sync_sess();
        if !per_editor {
            self.sess.status = t(Msg::StSessBackToShared).into();
        }
        self.sync_sess_ui();
        self.redraw();
    }

    /// 탭 표식 메뉴(§7): 세션 설명 · 해제(공유 복귀) · 다시 접속 · 공유 연결 고르기(공유 연결이 둘 이상일 때).
    /// 탭 표식 메뉴(사용자 09-18 확정 — 꼭 필요한 것만): `No connection` · 연결할 수 있는 공유 연결 목록 · (전용 연결이 있을 때만)
    /// 구분자 + 전용 연결 한 줄. **지금 이 탭이 쓰는 것 앞에만 ✓**(1탭 1연결 · 배타) · 체크 유무와 무관하게 글자는 같은 열.
    /// `No connection` = 이 탭은 어떤 서버에도 연결되지 않은 상태(전용 연결이 있으면 그 연결을 해제).
    fn open_badge_menu(&mut self, i: usize) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let tab = self.editors.tab_id(i);
        let Some(s) = self.sess_by_id(self.sess_id_for_tab(tab)) else {
            return;
        };
        let cur = s.id;
        let private = s.is_private();
        // 이 탭의 세션이 접속돼 있지 않으면(시작 직후 · 해제 뒤 · 미연결 자리) `No connection`에 ✓ — 목록의 다른 줄은 접속된 것만이라 배타.
        let unconnected = !s.connected && !s.busy && !s.idle_closed;
        let mut items =
            vec![CtxItem::item("sess.none", t(Msg::MnSessNoConnection)).with_mark(unconnected)];
        for sh in self
            .all_sess()
            .filter(|s| !s.is_private() && !s.closing && (s.connected || s.busy))
        {
            let label = if sh.broken {
                format!("{} · {}", sh.desc, t(Msg::StSessBrokenTag))
            } else {
                sh.desc.clone()
            };
            items.push(
                CtxItem::item(format!("sess.use:{}", sh.id), label)
                    .with_mark(!private && sh.id == cur),
            );
        }
        if private && !unconnected {
            items.push(CtxItem::Separator);
            let label = if s.broken {
                format!("{} · {}", s.desc, t(Msg::StSessBrokenTag))
            } else if !s.connected {
                format!("{} · {}", s.desc, t(Msg::ExpOffline))
            } else {
                s.desc.clone()
            };
            items.push(CtxItem::item("sess.private", label).with_mark(true));
        }
        self.badge_menu_tab = Some(tab);
        self.editors.open_badge_menu(i, items);
        self.redraw();
    }

    fn badge_pick(&mut self, tab: u64, id: &str) {
        match id {
            "sess.disconnect" => self.disconnect_private(tab),
            "sess.none" => self.make_unconnected(tab),
            // 전용 연결 줄 = 이미 이 탭의 것 — 끊겨 있으면 다시 접속, 아니면 아무것도 안 함.
            "sess.private" => {
                if self
                    .sess_by_id(self.sess_id_for_tab(tab))
                    .is_some_and(|s| !s.connected)
                {
                    self.badge_pick(tab, "sess.reconnect");
                }
            }
            "sess.reconnect" => {
                let Some(sid) = self
                    .all_sess()
                    .find(|s| s.owner == Some(tab) && !s.closing)
                    .map(|s| s.id)
                else {
                    return;
                };
                let fallback = self.default_spec.clone();
                // 표식 메뉴의 "다시 접속" = 사용자의 명시적 요청 → 설정 `connect.reconnect_same`이 켜져 있으면 끊고 다시.
                let again = self.settings.flag("connect.reconnect_same");
                self.with_sess(sid, |a| {
                    if let Some(spec) = a.sess.spec.clone().or(fallback) {
                        a.connect_quietly(spec, again);
                    }
                });
                self.sync_sess_ui();
            }
            id if id.starts_with("sess.use:") => {
                if let Ok(sid) = id["sess.use:".len()..].parse::<u64>() {
                    // 전용 탭이면 먼저 그 세션을 닫는다(미커밋이 있으면 확인 팝업이 뜨고 여기서는 묶지 않는다).
                    if self.all_sess().any(|s| s.owner == Some(tab) && !s.closing) {
                        self.disconnect_private(tab);
                    }
                    if !self.all_sess().any(|s| s.owner == Some(tab) && !s.closing) {
                        self.tab_bind.insert(tab, sid);
                        // 다른 서버의 결과를 이어 받지 않게.
                        self.freeze_results_of(tab);
                        self.sync_sess();
                        self.sync_sess_ui();
                    }
                }
            }
            _ => {}
        }
        self.redraw();
    }

    /// DB 워커 하나 시작(설정 현재값 · 시작 때와 같은 인자).
    fn spawn_worker(&self) -> (worker::Handle, mpsc::Receiver<RunEvent>) {
        let proxy = self.wake_proxy.clone();
        worker::spawn(
            DEFAULT_DIALECT,
            self.settings.int("grid.max_rows").max(0) as usize,
            self.settings.flag("session.autocommit"),
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
        // ★ 툴바 Disconnect의 뜻은 지금 탭이 쥔 연결로 정한다(docs/54 · `sessions::disconnect_plan` D16):
        //   전용 = 이 탭만 · 공유 혼자 = 바로 · 공유를 다른 탭도 쓰면 "모두 해제 / 이 탭만 떼기 / 취소".
        let bound = self.bound_tabs(self.sess.id);
        if sessions::disconnect_plan(self.sess.is_private(), bound)
            == sessions::DisconnectPlan::SharedAsk
        {
            self.open_disc_guard(bound);
            return;
        }
        self.disconnect_current();
    }

    /// 접속 해제 로그 한 줄(사용자 09-19 "어느 경로로 어떤 서버를") — 지금 세션 기준: 종류(공유/전용[탭]) · 대상 · 경로.
    /// 경로는 `Sess.disc_path`(진입점이 미리 표시) · 없으면 `fallback`. 워커를 바꾸는 해제는 `Disconnected` 이벤트가 오지
    /// 않으므로 여기서 바로 남기고, 이벤트로 오는 해제(스크립트·서버)는 `drain_events`가 같은 문장으로 남긴다.
    fn log_disconnect(&mut self, fallback: sessions::DiscPath) {
        let path = self.sess.disc_path.unwrap_or(fallback);
        self.sess.disc_path = Some(path);
        let kind = match self.sess.owner {
            Some(tab) => {
                let title = self
                    .editors
                    .tab_list()
                    .into_iter()
                    .find(|(id, _, _)| *id == tab)
                    .map(|(_, t, _)| t)
                    .unwrap_or_default();
                tf(Msg::LogSessPrivate, &[&title])
            }
            None => t(Msg::LogSessShared).to_string(),
        };
        let target = self
            .sess
            .spec
            .as_ref()
            .map(nsql_script::ConnectSpec::redacted)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.sess.desc.clone());
        let line = tf(Msg::LogDisconnected, &[&kind, &target, t(path.msg())]);
        let e = LogEntry::new(LogKind::Disconnect, line);
        if let Some(h) = &self.log_hub {
            h.push(e.clone());
        }
        self.log_win.push(e);
    }

    /// 세션 `id`에 해제 경로를 표시(진입점이 부른다 · 그 뒤의 해제가 로그에 경로를 남긴다).
    fn mark_disc(&mut self, id: u64, path: sessions::DiscPath) {
        self.with_sess(id, |a| a.sess.disc_path = Some(path));
    }

    /// 공유 연결을 다른 탭도 쓸 때의 확인 팝업(잃는 순간 규칙과 같은 부품).
    fn open_disc_guard(&mut self, bound: usize) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let others = bound.saturating_sub(1).to_string();
        self.sess.status = tf(Msg::StDiscGuard, &[&others]);
        let items = vec![
            CtxItem::item("disc.all", tf(Msg::MnDiscAll, &[&bound.to_string()])),
            CtxItem::item("disc.detach", t(Msg::MnDiscDetach)),
            CtxItem::Separator,
            CtxItem::item("disc.cancel", t(Msg::MnTxCancel)),
        ];
        let r = self
            .tool_dock
            .item_rect("conn.disconnect")
            .unwrap_or(self.status_tx_rect);
        self.open_status_popup(Rect::new(r.x, r.bottom(), 0, 0), items);
        self.redraw();
    }

    fn disc_pick(&mut self, id: &str) {
        match id {
            "all" => self.disconnect_current(),
            // 이 탭만 떼기 = 미연결 자리(다른 탭들은 그대로 공유 연결을 쓴다).
            "detach" => self.make_unconnected(self.editors.active_id()),
            _ => {}
        }
    }

    /// 이 서버에 붙은 세션 전부 해제(탐색기 루트 메뉴 · docs/54): 공유 세션 = 해제(묶인 탭은 미연결로 보임) ·
    /// 전용 세션 = 그 탭을 미연결로 · 메타 세션은 참조 수 0이 되면 `sync_sess_ui`가 닫는다(트리는 오프라인으로 남는다).
    fn disconnect_server(&mut self, spec: &ConnectSpec) {
        let ids: Vec<(u64, Option<u64>)> = self
            .all_sess()
            .filter(|s| {
                !s.closing
                    && s.spec
                        .as_ref()
                        .is_some_and(|have| worker::same_server(have, spec))
            })
            .map(|s| (s.id, s.owner))
            .collect();
        for (id, owner) in ids {
            self.mark_disc(id, sessions::DiscPath::Explorer);
            match owner {
                Some(tab) => self.disconnect_private_as(tab, true),
                None => self.disconnect_shared(id),
            }
        }
        self.sync_sess();
        self.sync_sess_ui();
        self.redraw();
    }

    /// 오프라인 루트를 다시 연결(탐색기 루트 메뉴): 이 서버의 공유 세션이 남아 있으면 그 세션으로 · 없으면 공유 세션을 새로.
    fn connect_server(&mut self, spec: ConnectSpec) {
        let existing = self
            .all_sess()
            .find(|s| {
                !s.closing
                    && !s.is_private()
                    && s.spec
                        .as_ref()
                        .is_some_and(|have| worker::same_server(have, &spec))
            })
            .map(|s| s.id);
        let id = match existing {
            Some(id) => id,
            None => self.new_shared(),
        };
        let again = self.settings.flag("connect.reconnect_same");
        self.with_sess(id, |a| a.connect_quietly(spec, again));
        self.activate_shared(id);
        self.sync_sess_ui();
        self.redraw();
    }

    /// 이 연결로 새 탭(탐색기 루트 메뉴): 새 탭을 만들고 이 서버의 공유 세션에 묶는다(없으면 그냥 새 탭).
    fn new_tab_on(&mut self, spec: &ConnectSpec) {
        let shared = self
            .all_sess()
            .find(|s| {
                !s.closing
                    && !s.is_private()
                    && s.spec
                        .as_ref()
                        .is_some_and(|have| worker::same_server(have, spec))
            })
            .map(|s| s.id);
        // ★ 이 서버에 공유 연결이 없고 **전용 세션만** 있다(편집기 `CONNECT`로 붙은 서버 · 사용자 09-21): 새 탭도 같은 스펙으로
        //   전용 세션을 하나 연다 — 종전에는 묶을 공유 연결이 없어 새 탭이 "연결 없음"으로 남았다. 스펙에 비밀번호가 없으면
        //   워커가 세션 자격 금고에서 꺼내 쓰고, 금고에도 없으면 한 번 묻는다.
        let private_spec = if shared.is_none() {
            self.all_sess()
                .filter(|s| !s.closing && s.is_private())
                .filter_map(|s| s.spec.clone())
                .find(|have| worker::same_server(have, spec))
        } else {
            None
        };
        self.editors.new_tab(None);
        self.set_focus(Focus::Editor);
        let tab = self.editors.active_id();
        if let Some(sid) = shared {
            self.tab_bind.insert(tab, sid);
        } else if let Some(pspec) = private_spec {
            if let Some(id) = self.new_private(tab) {
                self.with_sess(id, |a| a.connect_quietly(pspec, false));
            }
        }
        self.sync_sess();
        self.sync_sess_ui();
        self.redraw();
    }

    /// 지금 탭의 연결 해제(전용 = 그 세션 · 공유 = 그 공유 연결 전체).
    fn disconnect_current(&mut self) {
        // 미커밋 문장이 있으면 먼저 묻는다(잃는 순간만 모달 · DR-30).
        if !self.sess.tx_pending.is_empty() {
            self.tx_after = Some(TxAfter::Disconnect);
            self.open_tx_guard(Msg::MnTxCommitDisconnect, Msg::MnTxRollbackDisconnect);
            self.redraw();
            return;
        }
        self.disconnect_force();
    }

    fn disconnect_force(&mut self) {
        // 이 세션이 비밀번호를 묻는 중이면 그 물음부터 거둔다(입력 창을 닫고 워커에 취소 — 늦은 답이 새 워커로 가지 않게).
        if self.input_win.is_open()
            && self.input_win.is_password()
            && self.input_win.sess == self.sess.id
        {
            self.password_reply(worker::PwReply::Cancel);
        }
        // 전용 세션 탭에서의 해제 = 그 탭의 세션만(공유 모드면 공유 세션으로 복귀 · docs/52 §4).
        if let Some(tab) = self.sess.owner {
            self.disconnect_private(tab);
            return;
        }
        let stuck = self.sess.busy;
        self.log_disconnect(sessions::DiscPath::Toolbar);
        self.sess.worker.send(worker::Cmd::Disconnect);
        let (w, ev) = self.spawn_worker();
        self.sess.worker = w;
        self.sess.events = ev;
        self.sess.busy = false;
        self.sess.aux = 0;
        self.sess.user_disconnected = true;
        self.sess.idle_closed = false;
        self.editors.set_running(self.sess.run_editor, false);
        self.sess.status = t(if stuck {
            Msg::StDisconnectedAbandon
        } else {
            Msg::StDisconnected
        })
        .into();
        if stuck {
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
        }
        self.tx_close(TxOutcome::Lost);
        self.on_conn_disconnected();
        self.sync_sess_ui();
        self.redraw();
    }

    /// 드라이버 네트워크 옵션(docs/53): keepalive · Oracle 호출 상한 — 다음 접속부터.
    fn apply_net_options(&self) {
        nsql_drivers::set_net_options(
            self.settings.int("net.keepalive_secs").max(0) as u64,
            self.settings.int("session.call_timeout_secs").max(0) as u64,
        );
    }

    /// 접속 계열 결과 `Disconnected`의 UI 반영(워커 이벤트 · 즉시 해제 공용).
    fn on_conn_disconnected(&mut self) {
        self.sess.live_sid = None;
        self.sess.connected = false;
        self.sess.key_cache.clear();
        self.sess.sql_wait = None;
        // 접속 창·공유 접속 설명은 **접속 창으로 붙은 세션**이 끊겼을 때만 되돌린다(전용 세션의 해제는 그 탭의 일 · docs/52).
        if self.sess.id != self.primary_sess {
            return;
        }
        // 해제됐으니 "접속됨" 결과는 더 이상 사실이 아니다(테스트 결과는 유지).
        self.panel_results
            .retain(|_, st| !matches!(st, ConnState::Connected(_)));
        self.sess.key_cache.clear();
        self.sess.sql_wait = None;
        self.editors.set_conn_desc("");
        self.conn_win.clear_connect_marks();
        self.conn_win.clear_active();
        self.panel_op = None;
        self.conn_win.panel.set_state(ConnState::Idle);
    }

    /// 파일 싱크 허브(설정 `log.file` · 형식 `log.file_format`(same = 창과 같게) · 회전 `log.file_max_kb`) — 설정이 바뀌면 새로.
    fn rebuild_log_hub(&mut self) {
        self.log_hub = None;
        // 경로 설정도 내장 변수를 받는다(`${nsqlHome}/log.txt` · `${workspaceFolder}/…` · 사용자 09-23).
        let file = nsql_script::intrinsic::expand(
            self.settings.get("log.file").unwrap_or("").trim(),
            &self.run_intrinsic(),
        );
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
                self.sess.status = tf(Msg::ErrLogFile, &[&e.to_string()]);
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
    /// 툴바 두 버튼(docs/52 §7-2): Disconnect = 끊을 연결이 하나라도 있으면 활성 · Connect(플러그) 색 = **지금 탭의 연결** —
    /// 초록(연결됨) · 빨강(끊김 확인) · 기본(미연결). 탭마다 연결이 다를 수 있으므로 "어딘가 연결됨"이 아니라 이 탭의 사실을 보인다.
    fn sync_disconnect_btn(&mut self, cur_connected: bool) {
        let mut inv = Invalidations::default();
        self.tool_dock
            .set_item_enabled("conn.disconnect", cur_connected, &mut inv);
        // 툴팁·배지 = 누르면 무슨 일이 나는지(docs/54): 전용 / 공유 / 공유 + 함께 쓰는 탭 수(배지).
        let bound = self.bound_tabs(self.sess.id);
        let desc = self.sess.desc.clone();
        let (tip, badge) = match sessions::disconnect_plan(self.sess.is_private(), bound) {
            sessions::DisconnectPlan::Private => (t(Msg::TipDisconnectPrivate).to_string(), None),
            sessions::DisconnectPlan::SharedAlone if cur_connected => {
                (tf(Msg::TipDisconnectShared, &[&desc]), None)
            }
            sessions::DisconnectPlan::SharedAlone => (t(Msg::TipDisconnect).to_string(), None),
            sessions::DisconnectPlan::SharedAsk => (
                tf(Msg::TipDisconnectSharedN, &[&desc, &bound.to_string()]),
                Some(bound.to_string()),
            ),
        };
        self.tool_dock.set_item_tip("conn.disconnect", &tip);
        self.tool_dock
            .set_item_badge("conn.disconnect", badge.as_deref(), &mut inv);
        let tone = if self.sess.connected && self.sess.broken {
            ToolTone::Danger
        } else if self.sess.connected {
            ToolTone::Ok
        } else {
            ToolTone::Default
        };
        self.tool_dock.set_item_tone("conn.toggle", tone, &mut inv);
    }

    /// 환경 설정 창에서 바뀐 값을 **즉시** 반영(가능한 것만 · 나머지는 다음 시작).
    fn apply_setting(&mut self, key: &str) -> bool {
        let i = |s: &Settings, k: &str| s.int(k);
        match key {
            "ui.theme" => self.apply_theme(),
            // 끄는 순간 들고 있던 비밀번호 봉투를 전부 버린다(세션 자격 금고 · 켜는 것은 다음 입력부터).
            "connect.remember_session_password" => {
                if !self.settings.flag("connect.remember_session_password") {
                    nsql_vault::session::clear();
                }
            }
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
                self.sync_project_panel_opts();
            }
            // 복사 버튼 체크 표시 시간 — 접속 창(튜닝) + 실행 카드 둘 다.
            "ui.copy_feedback_ms" => {
                self.conn_win.set_tuning(conn_tuning(&self.settings));
                self.apply_run_toast();
            }
            "ui.menu_max_width" => self.rebuild_menus(),
            "ui.ime_hint" => {
                let on = self.settings.flag("ui.ime_hint");
                self.conn_win.set_ime_hint(on);
                self.input_win.set_ime_hint(on);
            }
            "probe.interval" | "probe.max_retries" | "probe.max_inflight" | "probe.icmp"
            | "probe.timeout" | "probe.retry_delay" | "probe.enabled" => {
                let n = self.settings.int("probe.max_inflight").clamp(1, 64) as usize;
                self.conn_win.set_policy(probe_policy(&self.settings), n);
            }
            "file.os_icons" => nexa_fs::shell::set_os_icons(self.settings.flag(key)),
            "project.icons" => self.project_panel.set_icons(self.settings.flag(key)),
            "editor.tab_line_scratch" | "editor.tab_line_file" | "editor.tab_line_preview" => {
                self.apply_tab_line_colors()
            }
            "file.probe_chevrons" => nexa_dlg::set_probe_chevrons(self.settings.flag(key)),
            "ui.toast_secs" | "ui.toast_alpha" => {
                self.toasts.configure(
                    self.settings.int("ui.toast_secs"),
                    self.settings.int("ui.toast_alpha"),
                );
                self.apply_run_toast();
            }
            "ui.toast_progress" | "ui.toast_fade_to" | "ui.toast_bar_spent" => {
                self.toasts.configure_progress(
                    self.settings.flag("ui.toast_progress"),
                    self.settings.int("ui.toast_fade_to"),
                    self.settings.int("ui.toast_bar_spent"),
                );
                self.apply_run_toast();
            }
            "run.toast"
            | "run.toast_hide_secs"
            | "run.toast_tick_ms"
            | "run.toast_follow"
            | "run.toast_max" => self.apply_run_toast(),
            "net.keepalive_secs" | "session.call_timeout_secs" => self.apply_net_options(),
            "mssql.encrypt" => {
                nsql_drivers::set_mssql_encryption(self.settings.get(key) == Some("login"))
            }
            // Oracle 클라이언트(자동/직접 지정) — 드라이버에 넘기고 설정 창의 읽기 전용 탐지 결과를 다시 계산한다.
            "oracle.client_mode" | "oracle.client_dir" | "oracle.tns_admin" => {
                apply_oracle_client(&self.settings);
                self.prefs_win.set_info(dbms_info_values());
                self.prefs_win.refresh(&self.settings);
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
            "input.hangul_compose" => {
                self.hangul_app = None;
                self.sync_hangul_mode();
            }
            // 새로 여는 창부터(메인 창은 재시작 뒤) — 열려 있는 창의 뒷단은 바꾸지 않는다.
            "gfx.mac_present" => {
                present::set_mode(self.settings.get(key).unwrap_or("softbuffer"));
            }
            "explorer.visible" => {
                self.explorer.set_visible(self.settings.flag(key));
                self.layout();
            }
            "explorer.icons" => self.explorer.set_icons(self.settings.flag(key)),
            "explorer.disconnect_pick" => self
                .explorer
                .set_disconnect_pick(self.settings.get(key).unwrap_or("auto")),
            "explorer.keep_offline" => self.explorer.set_keep_offline(self.settings.flag(key)),
            "explorer.filter_scope" => self
                .explorer
                .set_filter_scope(self.settings.get(key).unwrap_or("all")),
            "explorer.share_catalog" => self.explorer.set_share_catalog(self.settings.flag(key)),
            "explorer.show_system_schemas" | "explorer.hide_empty_schemas" => {
                self.explorer
                    .set_schema_opts(schema_opts_from(&self.settings));
            }
            "explorer.search_index"
            | "explorer.index_max"
            | "explorer.index_hits_max"
            | "explorer.index_prefetch"
            | "explorer.index_idle_ms"
            | "meta.warm_columns_max"
            | "meta.warm_idle_ms"
            | "meta.detail_max"
            | "meta.detail_ttl_secs"
            | "meta.cols_ttl_secs"
            | "meta.disk_cache" => {
                self.explorer.set_index_cfg(index_cfg_from(&self.settings));
            }
            "gen.qualified" | "gen.compact" | "gen.full_ddl" | "gen.separate_fk" => {
                let o = gen_opts_from(&self.settings);
                self.explorer.set_gen_opts(o);
            }
            "meta.refresh_highlight_ms" => self
                .explorer
                .set_highlight_ms(self.settings.int(key).max(0) as u64),
            "meta.refresh_secs" => self.meta_refresh_next = None,
            "file.external_poll_ms" | "file.external_check" | "file.external_change" => {
                self.ext_poll_next = None;
            }
            // 안정 대기·크기 상한은 감시 스레드를 만들 때 넣는 값 → 다음 확인 때 새로 만든다.
            "file.external_settle_ms" | "file.external_merge_max_kb" => self.ext_watch = None,
            "explorer.typeahead"
            | "explorer.typeahead_timeout"
            | "explorer.typeahead_space"
            | "explorer.typeahead_special"
            | "explorer.typeahead_pos" => {
                self.explorer.set_typeahead(typeahead_cfg(&self.settings))
            }
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
            "grid.row_focus" | "grid.row_focus_color" => self.apply_grid_row_focus(),
            k if k.starts_with("bookmark.") => {
                self.bookmarks.apply_settings(&self.settings);
                let on = self.bookmarks.enabled;
                self.act_bar.set_item_visible("view.bookmarks", on);
                if !on && self.bm_panel.is_visible() {
                    self.bm_panel.set_visible(false);
                    self.layout();
                }
                self.bm_sync_ui();
            }
            "window.always_on_top" => self.apply_on_top(),
            "log.always_on_top" => self.log_win.set_on_top(self.settings.flag(key)),
            "mem.always_on_top" => self.mem_win.set_on_top(self.settings.flag(key)),
            "mem.statusbar" | "mem.status_refresh_ms" => {
                self.mem_status = (0, None);
                self.redraw();
            }
            "grid.scroll" => {
                let on = self.settings.get(key) == Some("row");
                self.grid.set_row_snap(on);
                self.log_win.set_row_snap(on);
            }
            "editor.scroll" => self
                .editors
                .set_scroll_snap(self.settings.get(key) == Some("row")),
            "editor.line_numbers" => self.editors.set_line_numbers(self.settings.flag(key)),
            k if k.starts_with("intel.") => {
                self.intel
                    .set_cfg(intel::IntelCfg::from_settings(&self.settings));
                self.explorer
                    .set_preload(self.settings.flag("intel.preload"));
                self.explorer
                    .set_routines(self.settings.flag("intel.from_routines"));
            }
            "editor.diff_marks" => self.editors.set_diff_marks(self.settings.flag(key)),
            // 자동 닫기(코어 설정) = 편집기 옵션 한 벌을 다시 계산해 적용(키 접두가 확장 것이 아니라 None으로).
            "editor.auto_close_pairs"
            | "editor.pair_kinds"
            | "editor.pair_in_strings"
            | "editor.pair_match" => self.apply_extensions(None),
            "file.large_l1_mb"
            | "file.large_l1_lines"
            | "file.large_l2_mb"
            | "file.large_l2_lines" => {
                self.editors.set_large_cfg(large_cfg(&self.settings));
            }
            "file.large_ext_level" | "file.large_syntax_level" => {
                let (ext, syn) = large_feature_levels(&self.settings);
                self.editors.set_large_feature_levels(ext, syn);
                self.redraw();
            }
            "editor.undo_group_ms" | "editor.undo_giant_mb" => {
                let (ms, giant) = undo_rules(&self.settings);
                self.editors.set_undo_rules(ms, giant);
            }
            "editor.max_occurrences" => {
                let cap = self.occurrence_cap();
                self.editors.set_max_regions(cap);
            }
            "editor.undo_budget_mb" => self
                .editors
                .set_undo_budget(self.settings.int(key).max(1) as usize * 1024 * 1024),
            // 확장 관리자 켬/끔(설정 창에서 바꿔도) = 확장 효과 전체 재적용 + 활동 막대 아이콘.
            "project.preview_tab"
            | "project.scan_max"
            | "project.scan_threads"
            | "file.show_hidden"
            | "file.show_dot" => self.sync_project_panel_opts(),
            "search.history_max" => self
                .search_history
                .borrow_mut()
                .set_max(self.settings.int(key).max(0) as usize),
            "search.history_view" => {
                self.search_history
                    .borrow_mut()
                    .set_view(search_history::HistoryView::parse(
                        self.settings.get(key).unwrap_or("dropdown"),
                    ))
            }
            "search.history_rows" => self
                .search_history
                .borrow_mut()
                .set_rows(self.settings.int(key).max(1) as usize),
            "extensions.enabled" => {
                self.apply_extensions(None);
                self.layout();
            }
            "editor.minimap"
            | "editor.split_max"
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
                self.sess.status = t(if on {
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
                self.sess.status = tf(
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
            "ui.font_face" => {
                let pref = self.settings.get(key).map(str::to_string);
                if let Some(l) =
                    nexa_font::ui_font(pref.as_deref()).or_else(|| nexa_font::ui_font(None))
                {
                    self.ui_font = l.font;
                }
                self.layout();
            }
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
            "grid.result_tabs" | "grid.result_tabbar_single" | "grid.result_tab_title" => {
                self.apply_result_tab_opts();
            }
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
                self.sess.status = tf(Msg::StCopied, &[&n.to_string()]);
            } else {
                self.sess.status = t(Msg::ErrClipboard).into();
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
            self.sess.worker.cancel_fetch();
            self.sess.status = t(Msg::StFetchCancelling).into();
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
        let items = nsql_script::split_script_in(&full, Some(self.sess.dialect));
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

    /// 설정 `grid.result_tabs`/`grid.result_tabbar_single` → 모든 패널. 끄면 활성 탭 외 전부 즉시 해제(D-73 "강제로 메모리 줄이기").
    fn apply_result_tab_opts(&mut self) {
        let enabled = self.settings.flag("grid.result_tabs");
        let always = self.settings.flag("grid.result_tabbar_single");
        let numbered = self.settings.get("grid.result_tab_title") != Some("table");
        self.panel.set_options(enabled, always);
        self.panel.set_numbered(numbered);
        for p in self.panels.values_mut() {
            p.set_options(enabled, always);
            p.set_numbered(numbered);
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
            sql: String::new(),
            grid: fresh,
            seq: id,
            child_of: None,
        };
        self.panel.push(tab);
        let max = self.settings.int("grid.result_tabs_max").clamp(1, 64) as usize;
        if self.panel.tabs.len() > max && self.settings.flag("grid.result_tab_evict") {
            if let Some(v) = self.panel.evict_candidate() {
                let gone = self.panel.remove(v).map(|t| t.title).unwrap_or_default();
                self.sess.status = tf(Msg::StResultTabEvicted, &[&gone]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
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

    /// `parent` 탭에 딸린 `ord`번째 결과 탭의 id — 있으면 재사용, 없으면 같은 패널 끝에 만든다(활성 탭은 바꾸지 않는다 ·
    /// 상한 `grid.result_tabs_max`를 넘으면 가장 오래된 비고정 탭을 걷는다). 패널을 못 찾으면 `parent`(덮어쓰기 = 종전).
    fn child_result_tab(&mut self, parent: u64, ord: u32) -> u64 {
        let max = self.settings.int("grid.result_tabs_max").clamp(1, 64) as usize;
        let evict = self.settings.flag("grid.result_tab_evict");
        let Some(fresh) = self.grid_for(parent).map(|g| g.fresh_like()) else {
            return parent;
        };
        let in_active = self.panel.index_of(parent).is_some();
        let panel = if in_active {
            &mut self.panel
        } else {
            match self
                .panels
                .values_mut()
                .find(|p| p.index_of(parent).is_some())
            {
                Some(p) => p,
                None => return parent,
            }
        };
        if let Some(t) = panel
            .tabs
            .iter()
            .find(|t| t.child_of == Some((parent, ord)))
        {
            return t.id;
        }
        let id = self.next_result_id;
        self.next_result_id += 1;
        panel.push(ResultTab {
            id,
            title: t(Msg::ResultTabDefault).to_string(),
            pinned: false,
            named: false,
            sql: String::new(),
            grid: fresh,
            seq: id,
            child_of: Some((parent, ord)),
        });
        if panel.tabs.len() > max && evict {
            // 방금 만든 탭·부모·활성 탭은 걷지 않는다.
            let active = panel.active;
            let victim = panel
                .tabs
                .iter()
                .enumerate()
                .filter(|(i, t)| !t.pinned && *i != active && t.id != id && t.id != parent)
                .min_by_key(|(_, t)| t.seq)
                .map(|(i, _)| i);
            if let Some(v) = victim {
                panel.remove(v);
            }
        }
        panel.sync_bar();
        if in_active && self.panel.take_bar_changed() {
            self.layout();
        }
        id
    }

    /// 실행이 끝났다 — `parent`에 딸린 탭 가운데 이번 실행에서 쓰이지 않은 것(`ord >= used`)을 걷는다(고정·활성 탭은 둔다).
    fn prune_child_results(&mut self, parent: u64, used: u32) {
        let in_active = self.panel.index_of(parent).is_some();
        let panel = if in_active {
            &mut self.panel
        } else {
            match self
                .panels
                .values_mut()
                .find(|p| p.index_of(parent).is_some())
            {
                Some(p) => p,
                None => return,
            }
        };
        let active_id = panel.tabs.get(panel.active).map(|t| t.id);
        let gone: Vec<u64> = panel
            .tabs
            .iter()
            .filter(|t| {
                matches!(t.child_of, Some((p, o)) if p == parent && o >= used)
                    && !t.pinned
                    && Some(t.id) != active_id
            })
            .map(|t| t.id)
            .collect();
        if gone.is_empty() {
            return;
        }
        for id in gone {
            if let Some(i) = panel.index_of(id) {
                panel.remove(i);
            }
        }
        panel.sync_bar();
        if in_active && self.panel.take_bar_changed() {
            self.layout();
        }
        self.mem_released();
    }

    /// 결과 탭의 지금 제목.
    fn result_title(&self, key: u64) -> String {
        self.panel
            .tabs
            .iter()
            .chain(self.panels.values().flat_map(|p| p.tabs.iter()))
            .find(|t| t.id == key)
            .map(|t| t.title.clone())
            .unwrap_or_default()
    }

    /// 결과 탭 제목을 정해 준다(커서 변수 이름 · 딸린 결과 번호) — 사용자가 이름 붙였거나 고정한 탭은 그대로.
    fn title_result_as(&mut self, key: u64, title: &str) {
        let panel = if self.panel.index_of(key).is_some() {
            Some(&mut self.panel)
        } else {
            self.panels.values_mut().find(|p| p.index_of(key).is_some())
        };
        if let Some(p) = panel {
            if let Some(i) = p.index_of(key) {
                if !p.tabs[i].named && !p.tabs[i].pinned {
                    p.tabs[i].title = p.unique_title(title, i);
                }
            }
            p.sync_bar();
        }
    }

    /// 결과가 도착한 탭의 제목(사용자가 이름 붙이거나 고정한 탭은 그대로).
    fn retitle_result(&mut self, key: u64) {
        let (base, sql) = match self.grid_for(key) {
            Some(g) => (
                results::title_from_sql(g.source_table().as_deref(), g.source_sql()),
                g.source_sql().to_string(),
            ),
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
                // 이 결과를 만든 실행 쿼리를 탭에 보관한다(우클릭 ▸ 실행 쿼리 복사).
                p.tabs[i].sql.clone_from(&sql);
                if p.numbered() {
                    // 번호 규칙(`결과N` · 기본): 번호가 있으면 그대로 · 없으면 새 번호(이름 붙인·고정한 탭은 그대로).
                    p.ensure_numbered(i);
                } else if !p.tabs[i].named && !p.tabs[i].pinned {
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
            ResultAction::CopySql(i) => {
                // 탭에 보관한 실행 쿼리(없으면 그 탭 그리드의 출처 문장)를 클립보드에.
                let sql = self
                    .panel
                    .tabs
                    .get(i)
                    .map(|t| t.sql.clone())
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| {
                        let id = self.panel.tabs.get(i)?.id;
                        self.grid_for(id).map(|g| g.source_sql().to_string())
                    })
                    .unwrap_or_default();
                if sql.trim().is_empty() {
                    self.sess.status = t(Msg::StResultNoSql).to_string();
                } else if clipboard::write_text(&sql) {
                    self.sess.status =
                        tf(Msg::StResultSqlCopied, &[&sql.lines().count().to_string()]);
                }
            }
            ResultAction::Rename(i) => {
                if let Some(tab) = self.panel.tabs.get(i) {
                    let (id, title) = (tab.id, tab.title.clone());
                    let anchor = self.panel.tab_rect(i);
                    self.palette.open_prompt_at(
                        &format!("result.rename:{id}"),
                        t(Msg::PhResultRename),
                        &title,
                        anchor,
                    );
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
                    sql: String::new(),
                    grid: fresh,
                    seq: id,
                    child_of: None,
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
        // ★ 통제(docs/52 §3): 이 탭의 세션이 다른 작업 중이면 추가 페치·전체 조회·건수도 보내지 않는다(요청 상태만 푼다 —
        //   풀린 뒤 스크롤·버튼으로 다시 요청된다). 워커는 순차라 보내도 안전하지만, 언제 끝날지 모르는 대기를 만들지 않는다.
        if !self.gate_open() {
            self.grid.fetch_failed();
            return;
        }
        self.wake_if_idle();
        let key = self.grid_tab;
        // 메모리 예산(D-72 · 09-17 탭별 독립): **이 탭**의 행이 예산을 넘으면 추가 페치만 거부(전체 조회는 교체라 허용).
        //   다른 탭의 크기는 보지 않는다 — 사용자 09-17 "탭은 서로 영향을 미치지 않아야".
        let budget = (self.settings.int("grid.memory_budget_mb").max(1) as u64) * 1024 * 1024;
        if matches!(req, grid::FetchReq::Next { .. }) && self.grid.approx_bytes() > budget {
            self.grid.fetch_failed();
            self.sess.status = tf(
                Msg::StBudgetExceeded,
                &[&self.settings.int("grid.memory_budget_mb").to_string()],
            );
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
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
                self.sess.worker.send(worker::Cmd::FetchPage {
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
                // 실행 Facade(docs/43 §11): 전체 조회는 길 수 있어 커서 경로여도 카드.
                self.fetch_card_start(key, Msg::CardFetchAll, &sql);
                // 전체 조회 = **나머지 이어 받기**(09-17 위치 유지): offset = 이미 든 행 수 · 예산 = 이 탭 예산에서 든 만큼을 뺀 나머지
                // (탭별 독립 · 다른 탭을 빼지 않는다 · 이미 넘었으면 1 = 첫 배치 뒤 예산 정지).
                let remain = budget.saturating_sub(self.grid.approx_bytes()).max(1);
                self.sess.worker.send(worker::Cmd::FetchPage {
                    key,
                    sql,
                    offset: self.grid.row_count(),
                    limit: 0,
                    budget_bytes: remain,
                    strict: self.settings.get("grid.refetch_mode") != Some("offset"),
                    strict_all: self.settings.get("grid.refetch_mode") == Some("strict_all"),
                });
            }
            grid::FetchReq::Count => {
                // 건수 = 새 SQL(`SELECT COUNT(*) …`) → 카드.
                self.fetch_card_start(key, Msg::CardCount, &sql);
                self.sess.worker.send(worker::Cmd::Count { key, sql });
            }
        }
        self.sess.aux += 1;
        self.sess.touch();
        self.sync_gate();
        self.sess.status = t(Msg::StFetching).into();
        self.redraw();
    }

    /// ★ 페치/건수의 실행 상태 카드 시작(docs/43 §11 실행 Facade): 직접 실행과 같은 카드 · 결과(`Page`/`Count`)가 오면 끝난다.
    fn fetch_card_start(&mut self, key: u64, kind: Msg, sql: &str) {
        let text = format!("{} — {}", t(kind), nsql_run::txlog::one_line(sql, 200));
        self.sess.fetch_card = Some((key, Instant::now()));
        self.sess.run_card = self
            .run_toast
            .start(&text, nsql_log::now_local().stamp(), 1);
        self.run_toast
            .set_phase(self.sess.run_card, runtoast::Phase::Fetching);
    }

    /// 새로고침 — 같은 문장을 이 탭의 세그먼트 크기로 다시 실행.
    fn refresh_result(&mut self) {
        let src = self.grid.source_sql().to_string();
        if src.trim().is_empty() || !self.gate_open() {
            return;
        }
        self.wake_if_idle();
        self.sess.touch();
        self.sess.run_tab = self.grid_tab;
        self.sess.run_set_stmt = None;
        self.sess.run_children = 0;
        // 결과 새로고침은 **그 탭의 문장 하나**만 다시 돈다 → 딸린 결과 탭을 걷지 않는다(첫 결과 탭을 새로 고쳤다고
        // 다른 문장의 결과 탭이 닫히면 안 된다) · 변수 표는 지금 편집기 탭의 것.
        self.sess.run_tracking = false;
        self.sess.run_editor = self.editors.active_id();
        self.sess.busy = true;
        self.sess.status = t(Msg::StRunning).into();
        self.run_toast_start(&src);
        self.sess.last_run_items = split_items(&src, self.sess.dialect);
        let max_rows = self.grid.page_rows();
        self.sess.worker.send(worker::Cmd::Run {
            src,
            preflight: None,
            max_rows,
            vars: self.run_vars(),
            defines: self.run_defines(),
            intrinsic: Some(self.run_intrinsic()),
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
        if let Some(info) = self.sess.key_cache.get(&guess) {
            let info = info.clone();
            self.finish_view_sql(kind, info.as_ref());
            return;
        }
        if !self.gate_open() {
            return;
        }
        let (schema, table) = nsql_io::split_table(self.sess.dialect, &guess);
        self.sess.view_wait = Some(kind);
        self.sess.aux += 1;
        self.sess.worker.send(worker::Cmd::Keys {
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
            self.sess.status = w.clone();
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
        if let Some(info) = self.sess.key_cache.get(&guess) {
            let info = info.clone();
            self.finish_sql_copy(kind, info.as_ref());
            return;
        }
        if !self.gate_open() {
            return;
        }
        let (schema, table) = nsql_io::split_table(self.sess.dialect, &guess);
        self.sess.sql_wait = Some(kind);
        self.sess.aux += 1;
        self.sess.worker.send(worker::Cmd::Keys {
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
            self.sess.status = t(Msg::ErrClipboard).into();
            return;
        }
        self.sess.status = tf(Msg::StCopied, &[&n.to_string()]);
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
            self.sess.status = w.clone();
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
            "view.sessions" => self.open_sessions_window(el),
            "view.memory" => {
                if self.mem_win.is_open() {
                    self.mem_win.close();
                    self.persist_window_sizes(false);
                } else {
                    self.open_mem_window(el);
                }
            }
            "view.variables" => self.open_vars_window(el),
            "view.on_top" => {
                let on = !self.settings.flag("window.always_on_top");
                let _ = self
                    .settings
                    .set("window.always_on_top", if on { "on" } else { "off" });
                let _ = self.settings.save();
                self.apply_on_top();
                self.sess.status = tf(Msg::StOnTop, &[if on { "on" } else { "off" }]);
                self.redraw();
            }
            "conn.toggle" => self.open_conn_window(el),
            _ => self.menu_action(id),
        }
    }

    /// 메뉴·툴바 액션(id = 메뉴 항목 값 · 툴바 항목 id — 같은 어휘).
    fn menu_action(&mut self, id: &str) {
        // 적재 취소(Esc와 같은 길 · 팔레트/자동화용) — 활성 탭의 적재만.
        if id == "file.load_cancel" {
            self.file_load_cancel_active();
            return;
        }
        // ★ 적재 중인 탭(사용자 09-20): 그 탭의 본문을 쓰는 명령(편집·찾기·실행·저장)만 막는다 — 새 탭·열기·보기·접속은 그대로.
        if self.editors.active_loading() && fileload::blocked_while_loading(id) {
            self.sess.status = t(Msg::StLoadTabBusy).into();
            self.redraw();
            return;
        }
        // ★ 프로젝트 명령(docs/67 §2-2 · `project.*` 전부 — 접미 인자가 붙는 것도) — 메뉴바·팔레트·툴바·패널 링크·기동 명령이
        //   전부 이 한 길로(🔧 09-22 사용자: 풀다운 "새 프로젝트 저장…"이 무반응 — 패널 링크만 `project_cmd`로 갔다).
        if id.starts_with("project.") {
            self.project_cmd(id);
            return;
        }
        match id {
            "file.new" => {
                self.editors.new_tab(None);
                self.set_focus(Focus::Editor);
                self.on_new_tab();
            }
            "file.exit" => self.request_exit(),
            // ★ 파일 열기/저장(T-74) — 자체 대화상자(nexa-dlg) · 네이티브 0.
            "file.open" => self.open_file_dlg = Some(PickerMode::Open),
            "file.save" => match self.editors.active_path() {
                Some(p) => self.save_to(&p),
                None => self.open_file_dlg = Some(PickerMode::Save),
            },
            "file.save_as" => self.open_file_dlg = Some(PickerMode::Save),
            "file.follow" => self.ext_toggle_follow(),
            // 큰 파일 열기 선택(팔레트 목록 · docs/59 §4 3단계).
            "bigfile.open" | "bigfile.readonly" | "bigfile.head" | "bigfile.run" => {
                // 팔레트 밖(기동 명령 · 단축키)에서 와도 선택 목록은 닫는다.
                self.palette.close();
                if let Some((path, enc, _)) = self.big_pending.take() {
                    let mode = match id {
                        "bigfile.readonly" => LoadMode::ReadOnly,
                        "bigfile.head" => LoadMode::Head(
                            (self.settings.int("file.large_head_mb").max(1) as u64) << 20,
                        ),
                        "bigfile.run" => LoadMode::Run,
                        _ => LoadMode::Open,
                    };
                    self.load_file(&path, &enc, mode);
                }
            }
            // 디스크에서 바로 실행 — 파일 대화상자로 고른 뒤 편집기에 싣지 않고 돌린다.
            "file.run_file" => {
                self.file_purpose = FilePurpose::RunFile;
                self.open_file_dlg = Some(PickerMode::Open);
            }
            // 큰 파일 모드의 기능 축소를 이 탭에서만 풀거나 다시 건다.
            "file.large_force" => {
                self.sess.status = match self.editors.toggle_large_force() {
                    Some(true) => t(Msg::StLargeForced).into(),
                    Some(false) => t(Msg::StLargeRestored).into(),
                    None => t(Msg::StLargeNotLarge).into(),
                };
                self.layout();
                self.redraw();
            }
            // 메모리 지금 정리(팔레트) — 보이지 않는 탭의 그리기 캐시 + 힙 → OS. 결과·본문은 건드리지 않는다.
            "mem.trim_now" => {
                let (before, _) = memtrim::usage();
                self.mem_trim("manual");
                let (after, _) = memtrim::usage();
                self.sess.status = tf(
                    Msg::StMemTrimmed,
                    &[&nsql_core::fmt_bytes(before), &nsql_core::fmt_bytes(after)],
                );
                self.redraw();
            }
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
            // ★ 탐색기에 포커스가 있으면 ⌘F/Ctrl+F = 객체 필터 상자(docs/28 §7 · 사용자 09-25).
            "edit.find" if self.focus == Focus::Explorer => {
                self.explorer.focus_filter();
                self.redraw();
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
            // 코드 완성 수동 트리거(Ctrl+Space · docs/76) · Goto Symbol(Ctrl+R) · 심볼 이동.
            "edit.complete" => self.intel_request(true),
            "goto.symbol" => self.open_goto_symbol(),
            id if id.starts_with("sym:") => {
                if let Ok(b) = id[4..].parse::<usize>() {
                    self.goto_byte(b);
                }
            }
            "edit.goto_bracket" => {
                if self.editors.cur_mut().goto_bracket(false) {
                    self.redraw();
                }
            }
            "edit.bracket_prev"
            | "edit.bracket_next"
            | "edit.bracket_parent"
            | "edit.bracket_child" => self.run_extension_cmd(id),
            "ext.enable_mgr" | "ext.disable_mgr" | "ext.install" | "ext.remove" | "ext.list"
            | "ext.enable" | "ext.disable" | "ext.repo_add" | "ext.repo_list"
            | "ext.repo_remove" => self.ext_command(id),
            x if x.starts_with("ext.") => self.ext_pick(x),
            "edit.expand_brackets" => {
                if self.editors.cur_mut().expand_to_brackets() {
                    self.redraw();
                }
            }
            "edit.expand_selection" => {
                // ★ 다중 선택 구간 수 상한 `editor.max_occurrences`(docs/72 §2 · 09-22 전까지 키만 있고 미배선).
                let cap = self.occurrence_cap();
                let n0 = self.editors.selection_count();
                if n0 >= cap {
                    self.sess.status = tf(Msg::StOccurrenceCap, &[&n0.to_string()]);
                } else if self.editors.cur_mut().select_next_occurrence() {
                    let n = self.editors.selection_count();
                    if n > 1 {
                        self.sess.status = tf(Msg::StSelections, &[&n.to_string()]);
                    }
                }
                self.set_focus(Focus::Editor);
            }
            // ★ Quick Skip Next(Ctrl+K,Ctrl+D · 사용자 09-22): 마지막 출현을 버리고 다음 출현으로.
            "edit.skip_occurrence" => {
                if self.editors.cur_mut().skip_next_occurrence() {
                    let n = self.editors.selection_count();
                    self.sess.status = tf(Msg::StSelections, &[&n.to_string()]);
                }
                self.set_focus(Focus::Editor);
            }
            // 같은 문자열 전부 선택(Sublime Ctrl+⇧D 계열 · 상한 = 더 못 찾을 때까지).
            "edit.select_all_occurrences" => {
                let cap = self.occurrence_cap();
                let ed = self.editors.cur_mut();
                let mut capped = false;
                if ed.select_next_occurrence() {
                    // 상한(`editor.max_occurrences`)에서 멈춘다 — 구간마다 캐럿·편집이 곱해진다(docs/72 §2).
                    while ed.selection_count() < cap && ed.select_next_occurrence() {}
                    capped = ed.selection_count() >= cap;
                }
                let n = self.editors.selection_count();
                self.sess.status = if capped {
                    tf(Msg::StOccurrenceCap, &[&n.to_string()])
                } else {
                    tf(Msg::StSelections, &[&n.to_string()])
                };
                self.set_focus(Focus::Editor);
            }
            // ★ Sublime 줄·선택 편집(T-98 · 09-16) — 편집기에 포커스일 때만.
            "edit.duplicate_line" => self.editor_cmd(EditCommand::DuplicateLines),
            "edit.delete_line" => self.editor_cmd(EditCommand::DeleteLines),
            "edit.join_lines" => self.editor_cmd(EditCommand::JoinLines),
            "edit.swap_line_up" => self.editor_cmd(EditCommand::SwapLinesUp),
            "edit.swap_line_down" => self.editor_cmd(EditCommand::SwapLinesDown),
            "edit.toggle_comment" => self.editor_cmd(EditCommand::ToggleComment),
            "edit.toggle_block_comment" => self.editor_cmd(EditCommand::ToggleBlockComment),
            "intel.refresh" | "intel.refresh_server" | "intel.refresh_all" => {
                self.intel_refresh(id)
            }
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
            // ★ 선택 되돌리기(Sublime soft undo · 사용자 09-22) — 편집기에서만.
            "edit.soft_undo" | "edit.soft_redo" => {
                let ed = self.editors.cur_mut();
                let ok = if id == "edit.soft_undo" {
                    ed.soft_undo()
                } else {
                    ed.soft_redo()
                };
                if ok {
                    let n = self.editors.selection_count();
                    if n > 1 {
                        self.sess.status = tf(Msg::StSelections, &[&n.to_string()]);
                    }
                }
                self.set_focus(Focus::Editor);
            }
            "view.log" => self.toggle_log = true,
            "view.toolbar_reset" => self.reset_toolbar(),
            "view.colors" => self.open_colors = true,
            "view.keys" => self.open_keys = true,
            "view.extensions" => {
                if !self.settings.flag("extensions.enabled") {
                    self.sess.status = t(Msg::StExtManagerOff).into();
                    self.redraw();
                    return;
                }
                let on = !self.ext_panel.is_visible();
                if on {
                    self.side_panel_close_others("view.extensions");
                }
                self.ext_panel.set_visible(on);
                if on {
                    self.ext_panel.focus_query();
                    self.set_focus(Focus::Ext);
                    self.ext_panel_sync();
                    self.ext_fetch_start();
                } else if self.focus == Focus::Ext {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
            }
            "view.bookmarks" => {
                let on = !self.bm_panel.is_visible();
                if on {
                    self.side_panel_close_others("view.bookmarks");
                    self.bm_panel.sync(&self.bookmarks.store);
                }
                self.bm_panel.set_visible(on);
                if on {
                    self.bm_panel.focus_filter();
                    self.set_focus(Focus::Bookmarks);
                } else if self.focus == Focus::Bookmarks {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
                self.redraw();
            }
            x if x.starts_with("bookmark.") => self.bookmark_cmd(x),
            // 아웃라인 패널(docs/76): 켜면 활성 탭 심볼 동기 + 필터 포커스 · 다른 좌측 패널은 닫힌다.
            "view.outline" => {
                let on = !self.outline_panel.is_visible();
                if on {
                    self.side_panel_close_others("view.outline");
                }
                self.outline_panel.set_visible(on);
                if on {
                    self.outline_sync();
                    self.outline_panel.focus_filter();
                    self.set_focus(Focus::Outline);
                } else if self.focus == Focus::Outline {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
                self.redraw();
            }
            "view.project" => {
                let on = !self.project_panel.is_visible();
                if on {
                    self.side_panel_close_others("view.project");
                    self.sync_project_panel_opts();
                }
                self.project_panel.set_visible(on);
                if on {
                    self.project_panel.focus_filter();
                    self.set_focus(Focus::Project);
                } else if self.focus == Focus::Project {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
            }
            "view.search" => {
                let on = !self.search.is_visible();
                if on {
                    self.side_panel_close_others("view.search");
                }
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
                if on {
                    self.side_panel_close_others("view.explorer");
                }
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
                // 마지막 탭은 닫히지 않고 비워진다(탭 수가 그대로라 틱이 못 본다) → 여기서 회수를 예약.
                self.mem_released();
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
                    self.sess.status = tf(Msg::StSyntaxSet, &[name]);
                }
            }
            "run.statement" => self.run_sql(false),
            // Ctrl+\ = 새 결과 탭에 실행(T-93 · 끄면 Ctrl+Enter와 같다 · D-73).
            // 이 탭의 변수(탭 층 + 연결 공유 층)를 **실행할 수 있는 스크립트**로 새 편집기 탭에(내보내기 = 저장 · 가져오기 = 실행 · D-136).
            "vars.script" => {
                let mut all = self
                    .tab_vars
                    .get(&self.editors.active_id())
                    .cloned()
                    .unwrap_or_default();
                all.extend(self.sess.shared_vars.iter().cloned());
                all.extend(self.global_vars.iter().cloned());
                let text = nsql_script::vars_to_script(&all);
                self.editors.new_tab(None);
                self.editors.cur_mut().set_text(&text);
                self.set_focus(Focus::Editor);
                self.redraw();
            }
            // 이 탭의 변수 표를 새 결과 탭으로(`SHOW VARIABLES` — 러너가 탭 층 + 공유 층 + 프로필 층을 한 표로 낸다 · docs/63 V2).
            "vars.show" => {
                if !self.sess.busy && self.gate_open() {
                    if self.settings.flag("grid.result_tabs") {
                        self.new_result_tab();
                    }
                    self.run_text("SHOW VARIABLES".to_string(), 0, true);
                }
            }
            "run.statement_new_tab" => {
                if self.settings.flag("grid.result_tabs") && !self.sess.busy {
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
            "run.commit" | "run.rollback" => {
                if self.gate_open() {
                    self.sess.busy = true;
                    self.sess.touch();
                    self.sess.worker.send(if id == "run.commit" {
                        worker::Cmd::Commit
                    } else {
                        worker::Cmd::Rollback
                    });
                    self.sync_gate();
                }
            }
            // 트랜잭션 로그 창(T-107)이 오기 전까지는 상태줄 트랜잭션 팝업(모드 전환 · Commit(n) · Rollback(n) · 대기 목록).
            "tx.log" | "view.txlog" => self.open_txlog = true,
            // 접속 창 열기 — 연결 중이어도 끊지 않고 그냥 연다(사용자 09-14). 끊기는 폼의 Disconnect 버튼.
            "conn.toggle" => self.open_conn = true,
            // 툴바 Disconnect = **지금 탭의 연결 해제**(종전과 같음 · 사용자 09-18 원복) · 세션 목록 버튼/View = 세션 창.
            "conn.disconnect" => {
                self.sess.disc_path = Some(sessions::DiscPath::Toolbar);
                self.disconnect_now();
            }
            // 세션 목록 = 토글(사용자 09-19): 열려 있으면 닫고 · 아니면 연다.
            "view.variables" => {
                if self.vars_win.is_open() {
                    self.vars_win.close();
                } else {
                    self.open_vars = true;
                    // 창은 다음 `about_to_wait`에서 만든다 — 이벤트가 없으면 그 틱이 오지 않으므로 깨운다.
                    self.redraw();
                }
            }
            "view.memory" => self.toggle_mem_window(),
            "conn.sessions" | "view.sessions" => {
                if self.sessions_win.is_open() {
                    self.sessions_win.close();
                    self.persist_window_sizes(false);
                } else {
                    self.open_sessions = true;
                }
            }
            id if id.starts_with("conn.drop")
                || id.starts_with("conn.use:")
                || id.starts_with("conn.again:") =>
            {
                self.disconnect_pick(id);
            }
            "edit.prefs" => self.open_prefs = true,
            "edit.settings_json" => self.edit_settings_json(),
            "help.demo" => self.start_demo_create(),
            "help.about" => {
                self.sess.status = format!(
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
        // 범위 상자가 비었을 때의 기본 = 작업 모드별(사용자 09-23): 파일 모드 = 열린 파일만 · 폴더 모드 = + 그 폴더 이하 ·
        //   프로젝트 모드 = + 프로젝트 폴더(파일이 있는 곳) + 프로젝트에 추가한 폴더.
        let mut default_roots: Vec<PathBuf> = Vec::new();
        match project::WorkMode::of(self.project.path.as_deref(), self.arg_folder.as_deref()) {
            project::WorkMode::File => {}
            project::WorkMode::Folder(d) => default_roots.push(d),
            project::WorkMode::Project(p) => {
                if let Some(d) = p.parent() {
                    default_roots.push(d.to_path_buf());
                }
                for f in &self.project.folders {
                    if !default_roots.iter().any(|r| r == f) {
                        default_roots.push(f.clone());
                    }
                }
            }
        }
        let ctx = SearchCtx {
            tabs: self.editors.tab_texts(),
            current_dir: self
                .editors
                .active_path()
                .and_then(|p| p.parent().map(Path::to_path_buf)),
            default_roots,
            max_file_kb: self.settings.int("search.max_file_kb").max(0) as usize,
            threads: self.settings.int("search.threads").clamp(0, 16) as usize,
            gitignore: self.settings.flag("search.gitignore"),
            excludes,
        };
        self.search.start(ctx);
        self.redraw();
    }

    /// 결과 행 → 그 탭/파일을 열고 줄로 이동 · 일치 구간 선택.
    /// 파일은 **프로젝트 탐색기와 같은 규칙**(사용자 09-23): 한 번 클릭 = 미리보기 탭(포커스는 패널에) · 더블클릭/Enter = 정식 탭 + 편집기 포커스.
    fn open_search_result(&mut self, req: search_panel::OpenReq) {
        match (req.tab, &req.path) {
            (Some(id), _) => self.editors.switch_to_id(id),
            (None, Some(p)) => self.project_open_req(project_panel::OpenReq {
                path: p.clone(),
                permanent: req.permanent,
            }),
            (None, None) => return,
        }
        let start = {
            let ed = self.ed_mut();
            ed.goto_line(req.line);
            ed.caret() + req.col
        };
        let mut inv = Invalidations::default();
        self.ed_mut().select_range(start, start + req.len, &mut inv);
        if req.permanent {
            self.set_focus(Focus::Editor);
        }
        self.layout();
        self.redraw();
    }

    // ───────────────────────── 트랜잭션 UX(DR-30 · T-77 · docs/34) ─────────────────────────

    /// DML/DDL 완료 → 대기 목록 갱신. 수동 모드의 DML(영향 행 > 0) = 대기 +1 · 방언별 암묵 커밋 DDL = 비움 ·
    /// 자동 모드 + `tx.smart_commit` = 첫 DML 뒤 수동으로 전환.
    fn tx_on_done(&mut self, index: usize, stmt: &str, rows_affected: Option<u64>) {
        let auto = self.settings.flag("session.autocommit");
        if implicit_commit(self.sess.dialect, stmt) {
            if !self.sess.tx_pending.is_empty() {
                let w = first_word(stmt);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, tf(Msg::StTxImplicit, &[&w])));
                self.tx_close(TxOutcome::ImplicitCommit(w));
            }
            return;
        }
        let class = nsql_core::TxClass::of_sql(stmt);
        // 대기 대상: 영향 행 > 0인 DML · 트랜잭션 DDL 방언(PG·SQL Server·SQLite)의 DDL/TRUNCATE(docs/44 §5).
        let ddl_pending = class.is_ddl() && self.sess.dialect.ddl_transactional();
        if !ddl_pending && !rows_affected.is_some_and(|n| n > 0) {
            return;
        }
        if auto {
            if self.settings.flag("tx.smart_commit") {
                self.set_autocommit_now(false);
                self.sess.status = t(Msg::StTxSmartSwitched).into();
            }
            return;
        }
        let stamp = nsql_log::now_local().stamp();
        let when = stamp.get(11..16).unwrap_or("").to_string();
        self.txlog
            .select_session(self.sess.id)
            .attach_tx(index, stamp, true);
        self.txlog_win.redraw();
        self.sess.tx_pending.push(TxItem {
            editor: self.sess.run_editor,
            at: Instant::now(),
            when,
            summary: one_line(stmt, 60),
            class,
        });
        self.sess.tx_dirty = true;
        self.sess.tx_stale_logged = false;
        self.sync_tx_ui();
    }

    // ───────────────────────── 메모리 회수(사용자 09-19) ─────────────────────────

    /// "방금 큰 것을 놓았다" — 1초 뒤(해제가 실제로 끝나고 · 연달아 닫아도 한 번만) 힙을 OS에 돌려준다.
    fn mem_released(&mut self) {
        if self.settings.flag("mem.trim_on_release") {
            self.mem_trim_due = Some(Instant::now() + Duration::from_secs(1));
        }
    }

    /// 틱: ① 탭이 줄었으면 회수 예약 ② 예약된 1회 회수 ③ 유휴 주기 회수(`mem.trim_secs` · 입력 없음 5초 + 실행 중 세션 없음).
    fn mem_tick(&mut self, now: Instant) -> Option<Instant> {
        let tabs = self.editors.tab_count();
        if tabs < self.mem_last_tabs {
            self.mem_released();
        }
        self.mem_last_tabs = tabs;
        let mut next: Option<Instant> = None;
        if let Some(due) = self.mem_trim_due {
            if now >= due {
                self.mem_trim_due = None;
                self.mem_trim("release");
            } else {
                next = Some(due);
            }
        }
        let secs = self.settings.int("mem.trim_secs").max(0) as u64;
        if secs > 0 {
            let at = *self
                .mem_trim_next
                .get_or_insert(now + Duration::from_secs(secs));
            if now >= at {
                let idle = now.duration_since(self.blink_origin) >= Duration::from_secs(5)
                    && !self.all_sess().any(|s| s.busy);
                if idle {
                    self.mem_trim("periodic");
                    self.mem_trim_next = Some(now + Duration::from_secs(secs));
                } else {
                    // 바쁘면 10초 뒤에 다시 본다.
                    self.mem_trim_next = Some(now + Duration::from_secs(10));
                }
            }
            next = Some(next.map_or(self.mem_trim_next.unwrap_or(at), |n| {
                n.min(self.mem_trim_next.unwrap_or(at))
            }));
        }
        next
    }

    /// 회수 한 번: ① 보이지 않는 탭의 그리기 캐시를 놓는다(다시 보면 다시 만든다) ② 힙을 정리해 OS에 돌려준다.
    /// 개발자 모드(`load` 층)에는 전후 Private·걸린 시간을 남긴다.
    fn mem_trim(&mut self, why: &str) {
        let (before, _) = memtrim::usage();
        let released = self.editors.release_inactive_caches();
        let us = memtrim::trim();
        let (after, ws) = memtrim::usage();
        dlog!(self, LogLayer::Load, LogLevel::Timing, {
            LogEntry::new(
                LogKind::Info,
                format!(
                    "memory trim ({why}): private {} → {} · working set {} · {released} tab cache(s) released · {:.1} ms",
                    nsql_core::fmt_bytes(before),
                    nsql_core::fmt_bytes(after),
                    nsql_core::fmt_bytes(ws),
                    us as f64 / 1000.0
                ),
            )
        });
        if std::env::var_os("NSQL_TRACE_MEM").is_some() {
            eprintln!(
                "[mem] trim ({why}) private {before} -> {after} · {released} caches · {us} us"
            );
        }
    }

    // ───────────────────────── 외부 파일 변경(docs/58 · T-140) ─────────────────────────

    /// 활성 탭을 방금 읽었거나 저장했다 = 디스크와 기준이 맞다 → 서명을 기록하고 대기·유지·삭제 상태를 지운다(따라가기는 유지).
    fn ext_track_active(&mut self) {
        self.ext_track(self.editors.active_id());
    }

    /// 탭 하나의 파일 서명을 지금 디스크로 다시 잡는다(읽은 직후 · 저장 직후 — 활성 탭이 아닐 수도 있다).
    fn ext_track(&mut self, id: u64) {
        let path = self
            .editors
            .index_of_id(id)
            .and_then(|i| self.editors.path_of(i));
        let Some(path) = path else {
            self.ext_files.remove(&id);
            return;
        };
        let follow = self.ext_files.get(&id).is_some_and(|e| e.follow);
        self.ext_files.insert(
            id,
            extfile::ExtInfo {
                sig: nexa_fs::watch::file_sig(&path),
                follow,
                ..extfile::ExtInfo::default()
            },
        );
        self.ext_save_armed = None;
        self.ext_banner_sync();
    }

    /// 확인 요청(비동기) — `all` = 열린 파일 전부(창 활성화) · 아니면 활성 탭 하나(탭 전환·폴링). 실행 중인 탭은 건너뛴다.
    fn ext_check(&mut self, all: bool) {
        if self.settings.get("file.external_change") == Some("off") {
            return;
        }
        let active = self.editors.active_id();
        let reqs: Vec<nexa_fs::watch::WatchReq> = self
            .editors
            .files()
            .into_iter()
            .filter(|(id, _)| all || *id == active)
            .filter(|(id, _)| !self.editors.is_running(*id))
            .map(|(id, path)| nexa_fs::watch::WatchReq {
                key: id,
                path,
                known: self.ext_files.get(&id).and_then(extfile::ExtInfo::known),
            })
            .collect();
        self.ext_send(reqs);
    }

    fn ext_send(&mut self, reqs: Vec<nexa_fs::watch::WatchReq>) {
        if reqs.is_empty() {
            return;
        }
        if self.ext_watch.is_none() {
            let proxy = std::sync::Mutex::new(self.wake_proxy.clone());
            let settle = self.settings.int("file.external_settle_ms").max(0) as u64;
            // 읽기 상한 = 병합 상한의 8배(그보다 큰 파일은 편집기가 열 대상이 아니다 — 읽지 않고 "못 읽음").
            let max = (self.settings.int("file.external_merge_max_kb").max(16) as u64) * 1024 * 8;
            self.ext_watch = nexa_fs::watch::StatWatch::spawn(
                Box::new(move || {
                    if let Ok(p) = proxy.lock() {
                        let _ = p.send_event(Wake);
                    }
                }),
                Duration::from_millis(settle),
                max,
            );
        }
        if let Some(w) = &self.ext_watch {
            w.check(reqs);
        }
    }

    /// 틱: 사건 수거 · 닫힌 탭 정리 · 폴링 예약(활성 창 = 보이는 탭 · 따라가기 탭 = 비활성에서도 간격 ×2).
    fn ext_tick(&mut self, now: Instant) -> Option<Instant> {
        let events = self
            .ext_watch
            .as_ref()
            .map(|w| w.poll())
            .unwrap_or_default();
        if !events.is_empty() {
            let mut changed = 0;
            for ev in events {
                changed += usize::from(self.ext_on_event(ev));
            }
            if changed > 1 {
                self.sess.status = tf(Msg::StExtMany, &[&changed.to_string()]);
            }
            self.ext_banner_sync();
            self.redraw();
        }
        let alive = self.editors.tab_ids();
        self.ext_files.retain(|id, _| alive.contains(id));
        self.editors.reap_views();
        // 저장 2단 확인은 3초 창 — 지나고도 남은 "저장 막힘" 띠는 10초 뒤 걷는다(다시 저장하면 다시 묻는다).
        if self
            .ext_save_armed
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(10))
        {
            self.ext_save_armed = None;
            self.ext_banner_sync();
        }
        let poll = self.settings.int("file.external_poll_ms").max(0) as u64;
        if poll == 0
            || self.settings.get("file.external_check") == Some("focus")
            || self.settings.get("file.external_change") == Some("off")
        {
            return None;
        }
        let following = self.ext_files.values().any(|e| e.follow);
        if !self.main_active && !following {
            self.ext_poll_next = None;
            return None;
        }
        let step = Duration::from_millis(if self.main_active { poll } else { poll * 2 });
        let next = *self.ext_poll_next.get_or_insert(now + step);
        if now < next {
            return Some(next);
        }
        if self.main_active {
            self.ext_check(false);
        }
        if following {
            let reqs: Vec<nexa_fs::watch::WatchReq> = self
                .editors
                .files()
                .into_iter()
                .filter(|(id, _)| self.ext_files.get(id).is_some_and(|e| e.follow))
                .map(|(id, path)| nexa_fs::watch::WatchReq {
                    key: id,
                    path,
                    known: self.ext_files.get(&id).and_then(extfile::ExtInfo::known),
                })
                .collect();
            self.ext_send(reqs);
        }
        self.ext_poll_next = Some(now + step);
        self.ext_poll_next
    }

    /// 사건 하나 처리 — 돌려주는 값 = 사용자가 알아야 할 변경이었는가.
    fn ext_on_event(&mut self, ev: nexa_fs::watch::WatchEvent) -> bool {
        use nexa_fs::watch::WatchEvent as E;
        match ev {
            E::Missing { key, path } => {
                let Some(_) = self.editors.index_of_id(key) else {
                    return false;
                };
                let info = self.ext_files.entry(key).or_default();
                if info.deleted {
                    return false;
                }
                info.deleted = true;
                info.pending = None;
                let name = nexa_fs::path::display(&path);
                self.sess.status = tf(Msg::StExtDeleted, &[&name]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                true
            }
            E::Unreadable { key, path, error } => {
                dlog!(self, LogLayer::Load, LogLevel::Timing, {
                    LogEntry::new(
                        LogKind::Info,
                        format!(
                            "external change: {} unreadable — {error} (tab {key})",
                            nexa_fs::path::display(&path)
                        ),
                    )
                });
                false
            }
            E::Changed {
                key,
                path,
                sig,
                bytes,
                ..
            } => self.ext_on_changed(key, &path, sig, &bytes),
        }
    }

    fn ext_on_changed(
        &mut self,
        id: u64,
        path: &Path,
        sig: nexa_fs::watch::FileSig,
        bytes: &[u8],
    ) -> bool {
        let Some(i) = self.editors.index_of_id(id) else {
            return false;
        };
        // 실행 중인 탭 = 줄 번호가 밀리면 실행 범위·오류 줄이 어긋난다 → 서명을 갱신하지 않고 둔다(다음 확인 때 다시 온다).
        if self.editors.is_running(id) {
            return false;
        }
        let Some((buf, base, enc, eol)) = self.editors.snapshot(i) else {
            return false;
        };
        let (mut base, enc) = (base.to_string(), enc.to_string());
        let (text, _, used) = Self::decode_bytes(bytes, &enc);
        let (disk_eol, disk) = eol::detect(&text);
        // 큰 파일 탭은 저장본 사본을 들고 있지 않다(docs/59) — 고치지 않은 탭이면 "기준 = 버퍼"로 보고 다시 읽기,
        //   고친 탭이면 병합 없이 묻는다(기준이 없으니 3-way가 성립하지 않는다).
        let large = self.editors.is_large(i);
        if large && !self.editors.is_dirty(i) {
            base = buf.clone();
        }
        let max = (self.settings.int("file.external_merge_max_kb").max(16) as usize) * 1024;
        let decision = extfile::decide(&extfile::DecideIn {
            base: &base,
            buf: &buf,
            disk: &disk,
            ask_always: self.settings.get("file.external_change") == Some("ask"),
            merge_on: self.settings.flag("file.external_merge") && !large,
            size_ok: bytes.len() <= max && buf.len() <= max,
            format_changed: disk_eol != eol || used != enc,
        });
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let now = Instant::now();
        let info = self.ext_files.entry(id).or_default();
        info.deleted = false;
        let suggest = info.note_change(now);
        let follow = info.follow;
        let mut notable = false;
        match decision {
            extfile::Decision::Ignore => {
                info.sig = Some(sig);
            }
            extfile::Decision::AdoptBase => {
                info.sig = Some(sig);
                info.pending = None;
                info.ignored = None;
                info.diverged = false;
                self.editors
                    .apply_external(i, None, &disk, Some(disk_eol), Some(used));
            }
            extfile::Decision::Reload => {
                info.sig = Some(sig);
                info.pending = None;
                info.ignored = None;
                info.diverged = false;
                self.editors
                    .apply_external(i, Some(&disk), &disk, Some(disk_eol), Some(used));
                self.sess.status = tf(Msg::StExtReloaded, &[&name]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                notable = true;
            }
            extfile::Decision::Merge(merged) => {
                info.sig = Some(sig);
                info.pending = None;
                info.ignored = None;
                info.diverged = false;
                self.ext_backup(&name, &buf);
                // 기준 = 디스크 본문 → 탭은 "디스크 대비 내 변경"만큼 dirty로 남는다.
                self.editors
                    .apply_external(i, Some(&merged.text), &disk, None, None);
                self.sess.status = tf(Msg::StExtMerged, &[&name, &merged.applied.to_string()]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                notable = true;
            }
            extfile::Decision::Ask(conflicts) => {
                if follow {
                    // 조용히 따라가기 = 겹치면 내 내용을 지킨다(띠 없음 · 저장은 2단 확인).
                    info.ignored = Some(sig);
                    info.diverged = true;
                } else {
                    info.pending = Some(extfile::Pending {
                        sig,
                        text: disk,
                        eol: disk_eol,
                        enc: used.to_string(),
                        conflicts,
                    });
                    info.ignored = None;
                    self.sess.status = tf(Msg::StExtConflict, &[&name]);
                    self.log_win
                        .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                    notable = true;
                }
            }
        }
        // 자주 바뀌는 파일 — 띠가 떠 있을 때 "조용히 따라가기"를 함께 내놓는다(`ext_banner_sync`가 본다).
        let _ = suggest;
        notable
    }

    /// 병합 전 본문 백업(`<설정 폴더>/backup/` · 최근 `file.external_backup_keep`개).
    fn ext_backup(&mut self, name: &str, text: &str) {
        let keep = self.settings.int("file.external_backup_keep").max(0) as usize;
        if keep == 0 {
            return;
        }
        let Some(dir) = nsql_settings::config_dir().map(|d| d.join("backup")) else {
            return;
        };
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let stamp: String = nsql_log::now_local()
            .stamp()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let _ = std::fs::write(dir.join(format!("{stamp}-{name}")), text);
        // 오래된 것부터 지운다(이름 = 시각 접두라 사전순 = 시간순).
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
            files.sort();
            let extra = files.len().saturating_sub(keep);
            for old in files.into_iter().take(extra) {
                let _ = std::fs::remove_file(old);
            }
        }
    }

    /// 확인 띠를 활성 탭의 상태로 맞춘다(띠 높이가 바뀌면 본문을 다시 배치).
    fn ext_banner_sync(&mut self) {
        let id = self.editors.active_id();
        let armed = self.ext_save_armed.is_some_and(|(t, _)| t == id);
        let view = self.ext_files.get(&id).and_then(|info| {
            let follow_btn = info.recent.len() >= 3 && !info.follow;
            if let Some(p) = &info.pending {
                let mut buttons = vec![
                    (extfile::BannerHit::Diff, t(Msg::BtnExtDiff).to_string()),
                    (extfile::BannerHit::Reload, t(Msg::BtnExtReload).to_string()),
                    (extfile::BannerHit::Keep, t(Msg::BtnExtKeep).to_string()),
                ];
                if follow_btn {
                    buttons.push((extfile::BannerHit::Follow, t(Msg::BtnExtFollow).to_string()));
                }
                Some(extfile::BannerView {
                    text: if p.conflicts > 0 {
                        tf(Msg::ExtBanConflict, &[&p.conflicts.to_string()])
                    } else {
                        t(Msg::ExtBanChanged).to_string()
                    },
                    buttons,
                    danger: false,
                })
            } else if info.deleted {
                Some(extfile::BannerView {
                    text: t(Msg::ExtBanDeleted).to_string(),
                    buttons: vec![(
                        extfile::BannerHit::Dismiss,
                        t(Msg::BtnExtDismiss).to_string(),
                    )],
                    danger: true,
                })
            } else if armed {
                Some(extfile::BannerView {
                    text: t(Msg::ExtBanSave).to_string(),
                    buttons: vec![
                        (extfile::BannerHit::Diff, t(Msg::BtnExtDiff).to_string()),
                        (
                            extfile::BannerHit::Overwrite,
                            t(Msg::BtnExtOverwrite).to_string(),
                        ),
                    ],
                    danger: true,
                })
            } else {
                None
            }
        });
        let changed = self.ext_banner.set(view);
        let inset = px(self.ext_banner.height(), self.scale);
        if self.editors.set_top_inset(inset) || changed {
            self.layout();
            self.redraw();
        }
    }

    fn ext_banner_pick(&mut self, hit: extfile::BannerHit) {
        let id = self.editors.active_id();
        let Some(i) = self.editors.index_of_id(id) else {
            return;
        };
        let name = self.editors.active_title();
        match hit {
            extfile::BannerHit::None | extfile::BannerHit::Body => {}
            extfile::BannerHit::Diff => {
                // 1차 = 디스크 내용을 읽기용 안내 탭으로(원래 파일의 구문 · No connection) — 좌우 비교 뷰는 docs/19가 들어오면.
                let text = self
                    .ext_files
                    .get(&id)
                    .and_then(|e| e.pending.as_ref().map(|p| p.text.clone()))
                    .or_else(|| {
                        let path = self.editors.active_path()?;
                        let bytes = std::fs::read(path).ok()?;
                        let (text, _, _) =
                            Self::decode_bytes(&bytes, &self.editors.active_encoding());
                        Some(eol::detect(&text).1)
                    });
                if let Some(text) = text {
                    let title = tf(Msg::ExtDiskTitle, &[&name]);
                    let created = self.editors.open_info_tab_like(&title, &text, Some(&name));
                    if created {
                        let tab = self.editors.active_id();
                        self.make_unconnected(tab);
                    }
                    self.set_focus(Focus::Editor);
                }
            }
            extfile::BannerHit::Reload => {
                let pending = self.ext_files.get_mut(&id).and_then(|e| e.pending.take());
                if let Some(p) = pending {
                    self.editors.apply_external(
                        i,
                        Some(&p.text),
                        &p.text,
                        Some(p.eol),
                        Some(&p.enc),
                    );
                    if let Some(e) = self.ext_files.get_mut(&id) {
                        e.sig = Some(p.sig);
                        e.ignored = None;
                        e.diverged = false;
                    }
                    self.sess.status = tf(Msg::StExtReloaded, &[&name]);
                }
            }
            extfile::BannerHit::Keep | extfile::BannerHit::Follow => {
                if let Some(e) = self.ext_files.get_mut(&id) {
                    if let Some(p) = e.pending.take() {
                        e.ignored = Some(p.sig);
                        e.diverged = true;
                    }
                    if hit == extfile::BannerHit::Follow {
                        e.follow = true;
                    }
                }
                self.sess.status = tf(
                    if hit == extfile::BannerHit::Follow {
                        Msg::StExtFollowOn
                    } else {
                        Msg::StExtKept
                    },
                    &[&name],
                );
            }
            extfile::BannerHit::Overwrite => {
                if let Some(path) = self.editors.active_path() {
                    self.ext_save_armed = Some((id, Instant::now()));
                    self.save_to(&path);
                }
            }
            extfile::BannerHit::Dismiss => {
                // 삭제 안내를 닫는다 — 상태(`deleted`)는 남겨 같은 삭제로 다시 띄우지 않는다.
                if let Some(e) = self.ext_files.get_mut(&id) {
                    e.deleted = false;
                    e.sig = None;
                }
            }
        }
        self.ext_banner_sync();
    }

    /// 팔레트 "외부 변경 조용히 따라가기(이 탭)".
    fn ext_toggle_follow(&mut self) {
        let id = self.editors.active_id();
        if self.editors.active_path().is_none() {
            return;
        }
        let name = self.editors.active_title();
        let e = self.ext_files.entry(id).or_default();
        e.follow = !e.follow;
        self.sess.status = tf(
            if e.follow {
                Msg::StExtFollowOn
            } else {
                Msg::StExtFollowOff
            },
            &[&name],
        );
        self.ext_poll_next = None;
        self.ext_banner_sync();
        self.redraw();
    }

    /// **저장 직전 확인**(동기 · stat 1 + 달라졌을 때만 읽기): true = 저장해도 된다. 디스크가 기준과 다르고 버퍼와도 다르면
    /// 첫 저장은 막고 띠 + 상태줄로 알린다 — 3초 안에 다시 저장하면(또는 띠의 [덮어쓰기]) 덮어쓴다(앱의 2단 확인 관례).
    fn ext_save_guard(&mut self, path: &Path) -> bool {
        let id = self.editors.active_id();
        // 다른 이름으로 저장 = 이 탭의 파일이 아니다(덮어쓰기 확인은 파일 대화상자의 몫).
        if self.editors.active_path().as_deref() != Some(path) {
            return true;
        }
        let armed = self
            .ext_save_armed
            .is_some_and(|(t, at)| t == id && at.elapsed() <= Duration::from_secs(3));
        if armed {
            return true;
        }
        let info = self.ext_files.get(&id).cloned().unwrap_or_default();
        let now_sig = nexa_fs::watch::file_sig(path);
        let mut differs = info.diverged || info.pending.is_some();
        if !differs && now_sig != info.sig && now_sig.is_some() {
            // 서명이 다르다 → 내용으로 확정(내용이 기준이나 버퍼와 같으면 통과 — VS Code와 같은 규칙).
            if let (Ok(bytes), Some(i)) = (std::fs::read(path), self.editors.index_of_id(id)) {
                if let Some((buf, base, enc, _)) = self.editors.snapshot(i) {
                    let (text, _, _) = Self::decode_bytes(&bytes, enc);
                    let disk = eol::detect(&text).1;
                    differs = disk != base && disk != buf;
                }
            }
        }
        if !differs {
            return true;
        }
        self.ext_save_armed = Some((id, Instant::now()));
        let name = self.editors.active_title();
        self.sess.status = tf(Msg::StExtSaveBlocked, &[&name]);
        self.ext_banner_sync();
        self.redraw();
        false
    }

    /// 자체 캡처용 기동 명령 하나 — `open:<경로>` = 파일을 탭으로 · 나머지 = 명령 id.
    fn startup_cmd(&mut self, id: &str) {
        // `conn.edit:<프로필>` = 로그인 창의 상세 폼을 그 프로필로 열고 두 칸을 바꾼 상태로(필수·바뀜 표식 캡처).
        // 자체 캡처용: 이름 바꾸기 입력 상자를 키 없이 연다(`tab.rename` = 편집기 탭 · `result.rename` = 결과 탭).
        // 자체 캡처용: 설정의 "찾아보기…"를 키·마우스 없이 누른다(`prefs.browse:<폴더 설정 키>`).
        if let Some(key) = id.strip_prefix("prefs.browse:") {
            if let Some(k) = nsql_settings::entry(key).map(|e| e.key) {
                self.file_purpose = FilePurpose::SettingFolder(k);
                self.folder_start = self
                    .settings
                    .get(k)
                    .map(PathBuf::from)
                    .filter(|p| p.is_dir());
                self.open_file_dlg = Some(PickerMode::Folder);
            }
            return;
        }
        // 자체 캡처용: 탐색기 우클릭 메뉴(`explorer.menu[:n]` = n번째 보이는 줄) · 항목 고르기(`explorer.pick:<id>`).
        // 자체 시험용: **앱 안에서** 마우스 사건을 만든다(`ui.move:x/y` · `ui.click:x/y` · `ui.rclick:x/y` — 창 좌표 · 장치 픽셀 · 쉼표는 명령 구분자라 못 쓴다).
        //   OS 입력 주입이 아니다 — 사용자의 커서·포커스·전경 창을 건드리지 않고, 실제 입력과 같은 `route` 경로를 그대로 탄다.
        if let Some((kind, xy)) = id
            .strip_prefix("ui.")
            .and_then(|r| r.split_once(':'))
            .filter(|(k, _)| {
                matches!(
                    *k,
                    "move"
                        | "click"
                        | "dclick"
                        | "sclick"
                        | "cclick"
                        | "rclick"
                        | "wheel"
                        | "hwheel"
                )
            })
        {
            let mut it = xy
                .split(['/', 'x'])
                .map(|v| v.trim().parse::<i32>().unwrap_or(0));
            let (x, y) = (it.next().unwrap_or(0), it.next().unwrap_or(0));
            // `ui.wheel:x/y/delta` · `ui.hwheel:x/y/delta` = 커서 아래로 휠 사건(가로 스크롤 결함 재현 · 09-22).
            let delta = it.next().unwrap_or(120);
            self.cursor = (x, y);
            self.route(InputEvent::MouseMove { x, y });
            match kind {
                "wheel" => self.route(InputEvent::Wheel { delta }),
                "hwheel" => self.route(InputEvent::HWheel { delta }),
                "click" | "dclick" | "sclick" | "cclick" => {
                    // `sclick` = Shift+클릭 · `cclick` = Ctrl/⌘+클릭(동시 편집 탭 선택 캡처 · 09-22) ·
                    // `dclick` = 더블클릭(같은 자리 두 번 · 프로젝트 탐색기의 정식 탭 열기 캡처).
                    let times = if kind == "dclick" { 2 } else { 1 };
                    for _ in 0..times {
                        self.route(InputEvent::MouseDown {
                            x,
                            y,
                            shift: kind == "sclick",
                            primary: kind == "cclick",
                        });
                        self.route(InputEvent::MouseUp { x, y });
                    }
                }
                "rclick" => self.route(InputEvent::RightDown { x, y }),
                _ => {}
            }
            self.redraw();
            return;
        }
        // 자체 시험용(83 · 09-25): 줄 펼치기 · 트리 덤프 · SQL Preview 덤프(키 주입 없이 구조·생성 결과를 파일로 확인).
        if let Some(rest) = id.strip_prefix("explorer.expand") {
            let row = rest.trim_start_matches(':').parse().unwrap_or(0);
            let ok = self.explorer.capture_expand(row);
            self.sess.status = format!("explorer.expand row={row} ok={ok}");
            self.redraw();
            return;
        }
        if let Some(q) = id.strip_prefix("explorer.filter:") {
            self.explorer.set_filter_text(q);
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("explorer.dump:") {
            let _ = std::fs::write(path, self.explorer.dump_rows());
            return;
        }
        if let Some(path) = id.strip_prefix("sqlprev.dump:") {
            let text = if self.sqlprev_win.is_open() {
                format!(
                    "{}\n---\n{}",
                    self.sqlprev_win.note_text(),
                    self.sqlprev_win.text()
                )
            } else {
                "closed".to_string()
            };
            let _ = std::fs::write(path, text);
            return;
        }
        if let Some(rest) = id.strip_prefix("explorer.menu") {
            let row = rest.trim_start_matches(':').parse().unwrap_or(0);
            let ok = self.explorer.capture_menu(row);
            self.sess.status = format!("explorer.menu row={row} opened={ok}");
            self.redraw();
            return;
        }
        if let Some(pick) = id.strip_prefix("explorer.pick:") {
            let ok = self.explorer.capture_pick(pick);
            if !ok {
                self.sess.status = format!("explorer.pick {pick}: menu not open");
            }
            self.redraw();
            return;
        }
        // 자체 시험용: 파일 검색 패널에 검색어를 넣고 실행(`search.run:<글>` · 제외 로그 캡처 09-23).
        if let Some(q) = id.strip_prefix("search.run:") {
            // `검색어|범위` — 범위 상자 글까지(쉼표는 명령 구분자라 범위 안에서는 `;`로 적고 여기서 콤마로 바꾼다).
            let (q, w) = match q.split_once('|') {
                Some((q, w)) => (q, Some(w.replace(';', ","))),
                None => (q, None),
            };
            self.search.run_query(q, w.as_deref());
            self.redraw();
            return;
        }
        // 자체 시험용: 활성 탭 이름을 바로 바꾼다(`tab.rename_to:<이름>` — 팔레트 입력 없이 · 북마크 패널 문서 이름 추종 캡처 09-23).
        if let Some(name) = id.strip_prefix("tab.rename_to:") {
            let i = self.editors.active();
            self.editors.rename_tab(i, name);
            self.redraw();
            return;
        }
        if id == "tab.drag_demo" {
            self.editors.capture_drag_demo();
            self.redraw();
            return;
        }
        if id == "tab.rename" {
            self.tab_menu_request(editors::TabMenuReq::Rename(self.editors.active()));
            self.redraw();
            return;
        }
        if id == "result.rename" {
            self.panel_action(ResultAction::Rename(self.panel.active));
            return;
        }
        if id == "result.menu" {
            self.panel.open_menu_for_capture();
            self.redraw();
            return;
        }
        if let Some(q) = id.strip_prefix("edit.prefs:") {
            self.prefs_query = Some(q.to_string());
            self.open_prefs = true;
            return;
        }
        // 자체 캡처용: 로그인 목록의 우클릭 메뉴를 `row`번째 줄에서 연다(`conn.menu[:n]` · 창 밖으로 안 잘리는지 모서리 캡처 · 61 §2-2 ④).
        if let Some(xy) = id.strip_prefix("conn.ime_hint:") {
            let mut it = xy.split('/').map(|v| v.trim().parse::<i32>().unwrap_or(0));
            let at = (it.next().unwrap_or(0), it.next().unwrap_or(0));
            self.conn_win.capture_ime_hint(at);
            return;
        }
        // (프로젝트 명령 `project.*`는 `menu_action` 앞머리에서 `project_cmd`로 — 메뉴바·팔레트와 같은 길.)
        // 자체 시험: 상태 팝업의 선택(`multi.open`/`multi.cancel` · `tx.*` · `disc.*` · `close.*`)은 팝업 픽 경로로(메뉴 동작이 아니다 ·
        //   09-22 기능 점검 S03 — `multi.open`이 `menu_action`으로 떨어져 아무 일도 안 했다).
        if ["multi.", "tx.", "disc.", "close."]
            .iter()
            .any(|p| id.starts_with(p))
        {
            self.indent_pick(id);
            self.redraw();
            return;
        }
        // 자체 캡처: 파일 대화상자 없이 다중 열기 확인 팝업(`file.open_many:<a>;<b>;…` · 쉼표는 기동 명령 구분자라 `;`).
        if let Some(rest) = id.strip_prefix("file.open_many:") {
            let paths: Vec<PathBuf> = rest
                .split(';')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .collect();
            self.multi_open_ask(paths, "auto".into());
            self.redraw();
            return;
        }
        if let Some(rest) = id.strip_prefix("conn.menu") {
            let row = rest.trim_start_matches(':').parse().unwrap_or(0);
            let ok = self.conn_win.capture_menu(row);
            self.sess.status = format!("conn.menu row={row} opened={ok}");
            self.conn_win.redraw();
            return;
        }
        if let Some(name) = id.strip_prefix("conn.edit:") {
            if let Ok(Some(spec)) = Vault::open_default().and_then(|v| v.get(name)) {
                self.conn_win.capture_open_detail(name);
                self.conn_win.panel.fill(name, &spec);
                self.conn_win.panel.capture_touch();
                self.conn_win.redraw();
            }
            return;
        }
        // `tx.manual`/`tx.auto` = 상태줄 Auto-commit 팝업의 선택과 같은 길(수동 커밋 실기를 입력 주입 없이 · `tx.log`는 창이라 아래로).
        if let Some(rest) = id
            .strip_prefix("tx.")
            .filter(|r| matches!(*r, "manual" | "auto"))
        {
            self.tx_pick(rest);
            self.redraw();
            return;
        }
        match id.strip_prefix("open:") {
            Some(path) => self.open_file(Path::new(path)),
            None => self.menu_action(id),
        }
    }

    /// 문장에 스키마가 없을 때 쓸 그 세션의 기본 스키마(`?schema=` · SQL Server는 그 값이 DB라 제외 → 탐색기가 `dbo`로).
    fn meta_default_schema(&self) -> Option<String> {
        if self.sess.dialect == Dialect::Mssql {
            return None;
        }
        self.sess.spec.as_ref().and_then(|s| s.schema.clone())
    }

    /// 성공한 문장이 DDL이면 대상을 모은다(docs/57 T1) — 반영은 실행이 끝난 뒤(또는 커밋 때) 한 번에.
    fn meta_on_done(&mut self, stmt: &str) {
        if !self.settings.flag("meta.refresh_on_ddl") {
            return;
        }
        let Some(t) = nsql_core::ddl_target(stmt, self.sess.dialect) else {
            return;
        };
        let wait = sessions::ddl_waits_for_commit(
            self.sess.dialect.ddl_transactional(),
            self.settings.flag("session.autocommit"),
            self.settings.flag("meta.refresh_on_commit"),
        );
        if wait {
            self.sess.ddl_wait.push(t);
        } else {
            self.sess.ddl_now.push(t);
        }
    }

    /// 모아 둔 DDL 대상을 탐색기에 반영 — 같은 (동작 종류·객체 종류·스키마)는 폴더가 같으므로 탐색기가 겹친 요청을 하나로 접는다.
    fn meta_flush(&mut self, targets: Vec<nsql_core::DdlTarget>) {
        if targets.is_empty() {
            return;
        }
        let schema = self.meta_default_schema();
        let mut sent = 0;
        for t in &targets {
            sent += self
                .explorer
                .apply_ddl(self.sess.spec.as_ref(), t, schema.as_deref());
        }
        dlog!(self, LogLayer::Load, LogLevel::Timing, {
            LogEntry::new(
                LogKind::Info,
                format!(
                    "explorer refresh after DDL: {} target(s) → {sent} folder request(s)",
                    targets.len()
                ),
            )
        });
        if sent > 0 {
            self.redraw();
        }
    }

    /// 유휴 워터마크(docs/57 T2): `meta.refresh_secs`마다 · 입력이 `meta.refresh_idle_secs` 동안 없고 실행 중인 세션이 없을 때만.
    fn meta_refresh_tick(&mut self, now: Instant) -> Option<Instant> {
        let secs = self.settings.int("meta.refresh_secs").max(0) as u64;
        if secs == 0 || !self.explorer.is_visible() {
            return None;
        }
        let next = *self
            .meta_refresh_next
            .get_or_insert(now + Duration::from_secs(secs));
        if now < next {
            return Some(next);
        }
        let idle = Duration::from_secs(self.settings.int("meta.refresh_idle_secs").max(0) as u64);
        let busy = self.all_sess().any(|s| s.busy);
        if busy || now.duration_since(self.blink_origin) < idle {
            // 바쁘면 조금 뒤에 다시 본다(주기를 통째로 미루지 않는다).
            let retry = now + idle.max(Duration::from_secs(5));
            self.meta_refresh_next = Some(retry);
            return Some(retry);
        }
        let all = self.settings.get("meta.refresh_scope") == Some("all");
        self.explorer.watermark_poll(all);
        let next = now + Duration::from_secs(secs);
        self.meta_refresh_next = Some(next);
        Some(next)
    }

    /// 대기 목록 비우기(커밋 · 롤백 · 해제 · 암묵 커밋).
    fn tx_clear(&mut self) {
        self.tx_guard_reset();
        self.sess.tx_blockers = 0;
        self.sess.tx_blocker_who.clear();
        self.sess.tx_pending.clear();
        self.sess.tx_read = false;
        self.sess.tx_dirty = false;
        self.sess.tx_stale_logged = false;
        self.sync_tx_ui();
    }

    /// 수동 모드의 조회 — 읽기 트랜잭션 표시(배지 없음 · 초록 · docs/44 §5).
    fn tx_on_read(&mut self, index: usize) {
        if self.settings.flag("session.autocommit") {
            return;
        }
        // 수동 모드의 조회는 열린 트랜잭션에 속한다(롤백/커밋 시점 표시 · docs/44 §3).
        self.txlog.select_session(self.sess.id).attach_tx(
            index,
            nsql_log::now_local().stamp(),
            false,
        );
        self.txlog_win.redraw();
        if self.sess.tx_read {
            return;
        }
        self.sess.tx_read = true;
        self.sync_tx_ui();
    }

    /// docs/56 L3 — 미커밋 변경이 있는 세션마다 주기적으로 "나 때문에 기다리는 세션"을 메타 세션에 묻는다.
    /// 조건: 설정 주기 > 0 · 세션 식별자를 앎 · 그 서버의 탐색기(메타 세션)가 온라인 · 이 서버에서 꺼지지 않음 · 한가함.
    fn tx_block_tick(&mut self, now: Instant) -> Option<Instant> {
        let poll = self.settings.int("tx.block_poll_secs").max(0) as u64;
        if poll == 0 {
            return None;
        }
        let due: Vec<(u64, String, Option<ConnectSpec>)> = self
            .all_sess()
            .filter(|s| {
                !s.tx_pending.is_empty()
                    && !s.closing
                    && !s.tx_block_off
                    && s.connected
                    && !s.blocked()
                    && now >= s.tx_block_next
            })
            .filter_map(|s| s.live_sid.clone().map(|sid| (s.id, sid, s.spec.clone())))
            .collect();
        for (id, sid, spec) in due {
            if self.explorer.blockers_poll(spec.as_ref(), &sid) {
                self.with_sess(id, |a| {
                    a.sess.tx_block_next = now + Duration::from_secs(poll)
                });
            }
        }
        self.all_sess()
            .filter(|s| !s.tx_pending.is_empty() && !s.tx_block_off && s.live_sid.is_some())
            .map(|s| s.tx_block_next)
            .min()
    }

    /// 트랜잭션 로그 창의 "차단 중" 띠를 전 세션의 현재 상태로 맞춘다(세션마다 "연결 — 기다리는 세션").
    fn tx_block_band_sync(&mut self) {
        let lines: Vec<String> = self
            .all_sess()
            .filter(|s| !s.tx_pending.is_empty())
            .flat_map(|s| {
                let name = if s.profile.is_empty() {
                    s.desc.clone()
                } else {
                    s.profile.clone()
                };
                s.tx_blocker_who
                    .iter()
                    .map(move |w| format!("{name} — {w}"))
                    .collect::<Vec<_>>()
            })
            .collect();
        if self.txlog_win.set_blocking(lines) {
            self.txlog_win.redraw();
        }
    }

    /// 막힘 감지 응답 반영 — 0 → n이면 위험 토스트·로그·상태줄 · n → 0이면 해소 로그 · 오류(권한 없음)면 그 세션에서 기능을 끈다.
    fn tx_block_drain(&mut self) -> bool {
        let mut changed = false;
        for (sid, r) in self.explorer.take_blockers() {
            let Some(id) = self
                .all_sess()
                .find(|s| s.live_sid.as_deref() == Some(sid.as_str()))
                .map(|s| s.id)
            else {
                continue;
            };
            match r {
                Ok(list) => {
                    let prev = self.sess_by_id(id).map_or(0, |s| s.tx_blockers);
                    let pending = self
                        .sess_by_id(id)
                        .is_some_and(|s| !s.tx_pending.is_empty());
                    let n = if pending { list.len() } else { 0 };
                    // 띠의 내용(누가 기다리는가)은 개수가 같아도 바뀔 수 있다 → 먼저 저장.
                    let who: Vec<String> = if pending { list.clone() } else { Vec::new() };
                    self.with_sess(id, |a| a.sess.tx_blocker_who = who);
                    self.tx_block_band_sync();
                    if n == prev {
                        continue;
                    }
                    self.with_sess(id, |a| a.sess.tx_blockers = n);
                    changed = true;
                    if n > 0 {
                        let who = list.iter().take(5).cloned().collect::<Vec<_>>().join(" | ");
                        let text = tf(Msg::StTxBlocking, &[&n.to_string(), &who]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, text.clone()));
                        self.toasts.push(
                            toast::ToastKind::Error,
                            t(Msg::TxWarnToastTitle),
                            text.clone(),
                        );
                        self.sess.status = text;
                        if let Some(w) = &self.window {
                            w.request_user_attention(Some(
                                winit::window::UserAttentionType::Informational,
                            ));
                        }
                    } else {
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            t(Msg::StTxBlockingCleared).to_string(),
                        ));
                    }
                }
                Err(e) => {
                    self.with_sess(id, |a| {
                        a.sess.tx_block_off = true;
                        a.sess.tx_blockers = 0;
                    });
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        tf(Msg::StTxBlockPollOff, &[&e]),
                    ));
                    changed = true;
                }
            }
        }
        changed
    }

    /// 활동·정리 시 유휴 미커밋 상태를 되돌린다(이 세션 · 카드가 이 세션 것이면 걷는다).
    fn tx_guard_reset(&mut self) {
        self.sess.last_exec = Instant::now();
        self.sess.tx_warned_at = None;
        self.sess.tx_snooze_until = None;
        self.sess.tx_countdown = None;
        if self.tx_warn.sess_id() == Some(self.sess.id) {
            self.tx_warn.hide();
        }
    }

    /// docs/56 L2 — 모든 세션의 유휴 미커밋을 점검한다(판정 = `sessions::tx_guard_step`). 돌려주는 값 = 다음에 깰 시각.
    fn tx_guard_tick(&mut self, now: Instant) -> Option<Instant> {
        if now < self.tx_guard_next {
            return Some(self.tx_guard_next);
        }
        let ids: Vec<u64> = self
            .all_sess()
            .filter(|s| !s.tx_pending.is_empty() && !s.closing)
            .map(|s| s.id)
            .collect();
        if ids.is_empty() {
            if self.tx_warn.sess_id().is_some() {
                self.tx_warn.hide();
                self.redraw();
            }
            self.tx_guard_next = now + Duration::from_secs(3600);
            return None;
        }
        let stale_min = self.settings.int("tx.stale_min").max(1) as u64;
        let remind_min = self.settings.int("tx.remind_min").max(0) as u64;
        let limit_min = self.settings.int("tx.idle_limit_min").max(1) as u64;
        let countdown =
            Duration::from_secs(self.settings.int("tx.idle_countdown_secs").clamp(5, 600) as u64);
        let action = sessions::TxIdleAction::parse(
            self.settings.get("tx.idle_action").unwrap_or("rollback"),
        );
        for id in ids {
            let Some(s) = self.sess_by_id(id) else {
                continue;
            };
            // 운영 접속 = 더 엄격한 기준(`tx.prod_*` · 전역 값보다 느슨해지지는 않는다 · docs/56 §4).
            let prod = s.spec.as_ref().and_then(|sp| sp.env) == Some(nsql_script::ConnEnv::Prod);
            let (stale_min, limit_min) = sessions::tx_limits_for(
                prod,
                (stale_min, limit_min),
                (
                    self.settings.int("tx.prod_stale_min").max(1) as u64,
                    self.settings.int("tx.prod_idle_limit_min").max(1) as u64,
                ),
            );
            let input = sessions::TxGuardIn {
                pending: true,
                blocked: s.blocked(),
                idle: now.saturating_duration_since(s.last_exec),
                stale_min,
                remind_min,
                action,
                limit_min,
                since_warn: s.tx_warned_at.map(|t| now.saturating_duration_since(t)),
                snoozed: s.tx_snooze_until.is_some_and(|t| now < t),
                counting: s.tx_countdown.map(|t| t.saturating_duration_since(now)),
            };
            match sessions::tx_guard_step(input) {
                sessions::TxGuardStep::None => {}
                sessions::TxGuardStep::Counting => self.redraw(),
                sessions::TxGuardStep::Warn => {
                    self.with_sess(id, |a| a.sess.tx_warned_at = Some(now));
                    // 카운트다운 카드가 떠 있으면 그것이 우선.
                    if !self.tx_warn.counting() {
                        self.tx_warn_show(id, None);
                    }
                }
                sessions::TxGuardStep::StartCountdown => {
                    let deadline = now + countdown;
                    self.with_sess(id, |a| {
                        a.sess.tx_countdown = Some(deadline);
                        a.sess.tx_warned_at = Some(now);
                    });
                    self.tx_warn_show(id, Some((deadline, action)));
                }
                sessions::TxGuardStep::Fire => self.tx_guard_fire(id, action),
            }
        }
        self.tx_guard_next = now
            + if self.tx_warn.counting() {
                Duration::from_secs(1)
            } else {
                Duration::from_secs(5)
            };
        Some(self.tx_guard_next)
    }

    /// 경고/카운트다운 카드를 띄운다(+ 창이 비활성이면 작업 표시줄 깜빡임 · 로그 1줄).
    fn tx_warn_show(&mut self, id: u64, countdown: Option<(Instant, sessions::TxIdleAction)>) {
        let Some(s) = self.sess_by_id(id) else { return };
        let n = s.tx_pending.len();
        let idle_min = s.last_exec.elapsed().as_secs() / 60;
        let title = tf(Msg::TxWarnTitle, &[&n.to_string(), &idle_min.to_string()]);
        let first = s
            .tx_pending
            .first()
            .map(|i| i.summary.clone())
            .unwrap_or_default();
        let detail = if s.desc.is_empty() {
            first
        } else {
            format!("{} · {}", s.desc, first)
        };
        let (countdown, buttons) = match countdown {
            Some((deadline, act)) => {
                let commit = act == sessions::TxIdleAction::Commit;
                let word = t(if commit {
                    Msg::TxWarnCommitWord
                } else {
                    Msg::TxWarnRollbackWord
                })
                .to_string();
                (
                    Some((deadline, word)),
                    vec![
                        (
                            txwarn::TxWarnHit::Rollback,
                            t(Msg::BtnTxRollbackNow).to_string(),
                        ),
                        (txwarn::TxWarnHit::Commit, t(Msg::BtnTxCommit).to_string()),
                        (txwarn::TxWarnHit::Later, t(Msg::BtnTxExtend).to_string()),
                    ],
                )
            }
            None => (
                None,
                vec![
                    (txwarn::TxWarnHit::Commit, t(Msg::BtnTxCommit).to_string()),
                    (
                        txwarn::TxWarnHit::Rollback,
                        t(Msg::BtnTxRollback).to_string(),
                    ),
                    (txwarn::TxWarnHit::Later, t(Msg::BtnTxLater).to_string()),
                ],
            ),
        };
        self.log_win
            .push(LogEntry::new(LogKind::Error, format!("{title} — {detail}")));
        self.tx_warn.show(txwarn::TxWarnView {
            sess_id: id,
            title,
            detail,
            countdown,
            buttons,
        });
        if let Some(w) = &self.window {
            w.request_user_attention(Some(winit::window::UserAttentionType::Informational));
        }
        self.redraw();
    }

    /// 카드 버튼 — 커밋/롤백은 그 세션에(작업 중이면 무시) · 나중에/연장 = 재알림 간격(0이면 경고 간격)만큼 미룸.
    fn tx_warn_pick(&mut self, hit: txwarn::TxWarnHit) {
        let Some(id) = self.tx_warn.sess_id() else {
            return;
        };
        match hit {
            txwarn::TxWarnHit::Commit | txwarn::TxWarnHit::Rollback => {
                let commit = hit == txwarn::TxWarnHit::Commit;
                self.with_sess(id, |a| {
                    if a.sess.blocked() || a.sess.tx_pending.is_empty() {
                        return;
                    }
                    a.sess.worker.send(if commit {
                        worker::Cmd::Commit
                    } else {
                        worker::Cmd::Rollback
                    });
                    a.tx_close(if commit {
                        TxOutcome::Committed
                    } else {
                        TxOutcome::RolledBack
                    });
                });
                self.tx_warn.hide();
            }
            txwarn::TxWarnHit::Later => {
                let mins = match self.settings.int("tx.remind_min") {
                    m if m > 0 => m,
                    _ => self.settings.int("tx.stale_min").max(1),
                } as u64;
                let until = Instant::now() + Duration::from_secs(mins * 60);
                self.with_sess(id, |a| {
                    a.sess.tx_snooze_until = Some(until);
                    a.sess.tx_countdown = None;
                });
                self.tx_warn.hide();
            }
            txwarn::TxWarnHit::Card | txwarn::TxWarnHit::None => {}
        }
        self.tx_guard_next = Instant::now();
    }

    /// 카운트다운 만료 — 자동 처리(롤백/커밋) + 트랜잭션 로그·로그 창·토스트·상태줄 기록.
    fn tx_guard_fire(&mut self, id: u64, action: sessions::TxIdleAction) {
        let commit = action == sessions::TxIdleAction::Commit;
        let done = self.with_sess(id, |a| {
            if a.sess.blocked() || a.sess.tx_pending.is_empty() {
                a.sess.tx_countdown = None;
                return None;
            }
            let n = a.sess.tx_pending.len();
            let idle_min = a.sess.last_exec.elapsed().as_secs() / 60;
            a.sess.worker.send(if commit {
                worker::Cmd::Commit
            } else {
                worker::Cmd::Rollback
            });
            a.tx_close(if commit {
                TxOutcome::AutoCommitted(idle_min)
            } else {
                TxOutcome::AutoRolledBack(idle_min)
            });
            Some((n, idle_min))
        });
        if self.tx_warn.sess_id() == Some(id) {
            self.tx_warn.hide();
        }
        if let Some(Some((n, idle_min))) = done {
            let text = tf(
                if commit {
                    Msg::StTxAutoCommitted
                } else {
                    Msg::StTxAutoRolledBack
                },
                &[&n.to_string(), &idle_min.to_string()],
            );
            self.log_win
                .push(LogEntry::new(LogKind::Error, text.clone()));
            self.toasts.push(
                toast::ToastKind::Warn,
                t(Msg::TxWarnToastTitle),
                text.clone(),
            );
            self.sess.status = text;
        }
        self.redraw();
    }

    /// 러너가 변경 없는 읽기 트랜잭션을 끝냈다(docs/56 L1) — 읽기 표시를 걷고 트랜잭션 로그의 열린 기록을 닫는다.
    /// `deferred` = 열린 커서가 닫힐 때 끝난다(표시는 지금 걷는다 — 잠금은 커서 유휴 상한·다음 실행에서 풀린다).
    fn tx_on_read_ended(&mut self, index: usize, deferred: bool) {
        let _ = index;
        dlog!(self, LogLayer::Tx, LogLevel::Basic, {
            LogEntry::new(
                LogKind::Info,
                tf(
                    Msg::LogTxReadEnded,
                    &[if deferred {
                        " · deferred to cursor close"
                    } else {
                        ""
                    }],
                ),
            )
        });
        if !self.sess.tx_pending.is_empty() {
            return;
        }
        self.sess.tx_read = false;
        self.sess.tx_dirty = false;
        let stamp = nsql_log::now_local().stamp();
        self.txlog
            .select_session(self.sess.id)
            .close_tx(TxOutcome::ReadEnded, stamp);
        self.txlog_win.redraw();
        self.sync_tx_ui();
    }

    /// 트랜잭션 버튼 색·배지·툴팁(docs/44 §5): 자동 커밋 = 기본색·배지 없음 · 수동 = 가장 심각한 문장 종류의 색 + 대기 수.
    fn sync_tx_button(&mut self) {
        use nsql_core::TxClass;
        let auto = self.settings.flag("session.autocommit");
        let mut inv = Invalidations::default();
        let n = self.sess.tx_pending.len();
        let top = self
            .sess
            .tx_pending
            .iter()
            .map(|i| i.class)
            .max_by_key(|c| c.severity())
            .unwrap_or(if self.sess.tx_read {
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
        } else if n == 0 && !self.sess.tx_read {
            t(Msg::TipTxLog).to_string()
        } else {
            let mut kinds: Vec<Msg> = Vec::new();
            let mut seen: Vec<TxClass> = self.sess.tx_pending.iter().map(|i| i.class).collect();
            if self.sess.tx_read && seen.is_empty() {
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
                .sess
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
        // 탭 배지는 **모든 세션**의 대기 문장을 모은다(전용 세션 탭도 자기 배지) · 오래됨은 세션별 첫 문장 기준.
        let min = self.settings.int("tx.stale_min").max(1) as u64;
        let mut map: HashMap<u64, (usize, bool)> = HashMap::new();
        for s in self.all_sess() {
            let st = s
                .tx_pending
                .first()
                .is_some_and(|f| f.at.elapsed().as_secs() >= min * 60);
            for it in &s.tx_pending {
                let e = map.entry(it.editor).or_insert((0, st));
                e.0 += 1;
                e.1 = st;
            }
        }
        let mode = self.settings.get("tx.badge").unwrap_or("count").to_string();
        self.editors.set_tx_badges(map, &mode);
        let has = !self.sess.tx_pending.is_empty();
        // 툴바 Commit/Rollback = 지금 세션에 대기 문장이 있고 **세션이 한가할 때만**(통제 · docs/52 §3) · 색은 대기 여부 그대로.
        let open = self.gate().commit;
        let mut inv = Invalidations::default();
        for id in ["run.commit", "run.rollback"] {
            self.tool_dock.set_item_enabled(id, open, &mut inv);
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
        self.sess
            .tx_pending
            .first()
            .is_some_and(|f| f.at.elapsed().as_secs() >= min * 60)
    }

    /// 오래된 미커밋 감시(about_to_wait · 1회 로그 + 배지 ⚠).
    fn tx_tick(&mut self) {
        if self.sess.tx_pending.is_empty() || self.sess.tx_stale_logged || !self.tx_is_stale() {
            return;
        }
        self.sess.tx_stale_logged = true;
        let min = self.settings.int("tx.stale_min").max(1).to_string();
        self.log_win
            .push(LogEntry::new(LogKind::Error, tf(Msg::StTxStale, &[&min])));
        self.sess.status = tf(Msg::StTxStale, &[&min]);
        self.sync_tx_ui();
    }

    /// 상태줄 트랜잭션 팝업: 모드 전환 · Commit(n) · Rollback(n) · 대기 문장 목록.
    fn open_tx_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let auto = self.settings.flag("session.autocommit");
        let n = self.sess.tx_pending.len();
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
            for it in self.sess.tx_pending.iter().take(12) {
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
        let n = self.sess.tx_pending.len().to_string();
        self.sess.status = tf(Msg::StTxGuard, &[&n]);
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
        self.open_status_popup_w(r, items, 300.0);
    }

    /// 폭을 지정하는 판 — 거터 북마크 메뉴처럼 짧은 항목만 있는 팝업은 좁게(사용자 09-23 "메뉴 폭이 너무 넓어").
    fn open_status_popup_w(
        &mut self,
        r: Rect,
        items: Vec<nexa_ctl::controls::ctxmenu::CtxItem>,
        width: f32,
    ) {
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
            .open_at(r.x, r.y, items, host, px(width, self.scale));
    }

    /// 팝업 항목(`tx.*`).
    fn tx_pick(&mut self, id: &str) {
        match id {
            "auto" => self.set_autocommit(true),
            "manual" => self.set_autocommit(false),
            // 상태줄 팝업의 Commit/Rollback도 메뉴·툴바와 같은 문지기를 지난다(docs/52 §3).
            "commit" => self.menu_action("run.commit"),
            "rollback" => self.menu_action("run.rollback"),
            "commit_then" => {
                self.sess.worker.send(worker::Cmd::Commit);
                self.tx_close(TxOutcome::Committed);
                self.run_tx_after();
            }
            "rollback_then" => {
                self.sess.worker.send(worker::Cmd::Rollback);
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
            // 다른 세션에도 미커밋이 남아 있을 수 있다 → 다시 점검(전부 답해야 종료).
            Some(TxAfter::Exit) => self.request_exit(),
            Some(TxAfter::SwitchAuto) => self.set_autocommit_now(true),
            None => {}
        }
    }

    /// 모드 전환 — 수동 → 자동인데 대기 문장이 있으면 먼저 묻는다.
    fn set_autocommit(&mut self, on: bool) {
        if on && !self.sess.tx_pending.is_empty() {
            self.tx_after = Some(TxAfter::SwitchAuto);
            self.open_tx_guard(Msg::MnTxCommitSwitch, Msg::MnTxRollbackSwitch);
            return;
        }
        self.set_autocommit_now(on);
    }

    /// 설정 + 살아 있는 세션(스크립트 `SET AUTOCOMMIT` · 워커 큐 순서 보장). 세션이 작업 중이면 문지기가 거부(설정도 그대로 —
    /// 설정과 세션이 어긋나지 않게 · 09-19 검토).
    fn set_autocommit_now(&mut self, on: bool) {
        if !self.gate_open() {
            return;
        }
        let _ = self
            .settings
            .set("session.autocommit", if on { "on" } else { "off" });
        self.persist_settings();
        if !self.sess.busy {
            let src = if on {
                "SET AUTOCOMMIT ON"
            } else {
                "SET AUTOCOMMIT OFF"
            };
            self.sess.last_run_items = split_items(src, self.sess.dialect);
            self.sess.busy = true;
            self.sess.worker.send(worker::Cmd::Run {
                src: src.to_string(),
                preflight: None,
                max_rows: self.grid.page_rows(),
                vars: self.run_vars(),
                defines: self.run_defines(),
                intrinsic: Some(self.run_intrinsic()),
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
                // 이름을 바꾸는 탭 바로 아래에 붙이고 그 탭을 강조한다(사용자 09-21).
                let anchor = self.editors.tab_rect(i);
                self.palette.open_prompt_at(
                    &format!("tab.rename:{i}"),
                    t(Msg::PhTabRename),
                    &title,
                    anchor,
                );
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
                        self.sess.status = tf(Msg::StRevealFailed, &[&e.to_string()]);
                    }
                }
            }
            TabMenuReq::KeepOpen(i) => {
                self.editors.promote_tab(i);
            }
            TabMenuReq::RevealProject(i) => {
                if let Some(path) = self.editors.path_of(i) {
                    self.reveal_in_project(&path);
                }
            }
        }
    }

    /// 편집기 탭 유형별 활성 줄 색(사용자 09-22): 설정 `editor.tab_line_{scratch,file,preview}`(`#RRGGBB` · 빈 값 = 기본) —
    /// 기본 = 스크립트 warn(미저장 주의) · 파일 accent · 미리보기 text_dim(임시). 테마·설정이 바뀔 때 다시 계산.
    fn apply_tab_line_colors(&mut self) {
        let pick = |s: &Settings, key: &str, dflt: Option<nexa_ctl::Color>| {
            color_alpha_setting(s, key).0.or(dflt)
        };
        let c = [
            pick(
                &self.settings,
                "editor.tab_line_scratch",
                Some(self.theme.warn),
            ),
            // ★ 파일 탭은 테마 강조색을 **명시**한다 — `None`은 탭 바에서 "바 공통 accent"(= 활성 탭의 유형 색)로 떨어져
            //   미저장 탭이 활성인 채 파일 탭을 묶으면 파일 탭 줄까지 주황이 됐다(사용자 09-23 "각 탭의 색을 유지").
            pick(
                &self.settings,
                "editor.tab_line_file",
                Some(self.theme.accent),
            ),
            pick(
                &self.settings,
                "editor.tab_line_preview",
                Some(self.theme.text_dim),
            ),
        ];
        self.editors.set_tab_line_colors(c);
    }

    /// 프로젝트 탐색기에서 파일 보기(탭 메뉴 · 사용자 09-22): 패널이 닫혀 있으면 열고 · 자동 확장 설정과 무관하게
    /// 조상을 펼쳐 선택 · 보이게 스크롤 · 포커스를 패널로.
    fn reveal_in_project(&mut self, path: &Path) {
        if !self.project_panel.is_visible() {
            self.menu_action("view.project");
        }
        if self.project_panel.reveal(path) {
            self.set_focus(Focus::Project);
        }
        self.redraw();
    }

    /// 활성 탭이 바뀌면 탐색기의 선택을 그 파일에 맞춘다(프로젝트 폴더 안 파일만) — 기본은 펼치지 않고 표시만(`mark_path`) ·
    /// `project.auto_reveal`이면 조상을 펼치고 스크롤(`reveal` · 포커스는 안 옮긴다).
    fn project_sync_active(&mut self) {
        let id = self.editors.active_id();
        if id == self.last_synced_tab {
            return;
        }
        self.last_synced_tab = id;
        if !self.project.is_open() {
            return;
        }
        let Some(p) = self.editors.active_path() else {
            return;
        };
        if !self.editors.in_project(&p) {
            return;
        }
        if self.settings.flag("project.auto_reveal") {
            self.project_panel.reveal(&p);
        } else {
            self.project_panel.mark_path(&p);
        }
        self.redraw();
    }

    fn close_tab_guarded(&mut self, i: usize) {
        let id = self.editors.tab_id(i);
        // 실행 중인 탭은 닫지 않는다(결과가 갈 곳이 사라진다) — ■로 중지한 뒤(09-19 검토).
        if self.editors.is_running(id) {
            self.sess.status = t(Msg::StTabRunningClose).into();
            self.redraw();
            return;
        }
        // 그 탭이 쓰는 세션의 대기 문장(전용 세션이면 그 세션 전부 = 탭을 닫으면 세션도 닫힌다 · docs/52 §8).
        let sid = self.sess_id_for_tab(id);
        let n = self.sess_by_id(sid).map_or(0, |s| {
            if s.is_private() {
                s.tx_pending.len()
            } else {
                s.tx_pending.iter().filter(|t| t.editor == id).count()
            }
        });
        if n == 0 {
            self.editors.close_tab_confirmed(i);
            return;
        }
        match self.settings.get("tx.close_action").unwrap_or("ask") {
            act @ ("commit" | "rollback") => {
                let commit = act == "commit";
                self.with_sess(sid, |a| {
                    a.sess.worker.send(if commit {
                        worker::Cmd::Commit
                    } else {
                        worker::Cmd::Rollback
                    });
                    a.tx_close(if commit {
                        TxOutcome::Committed
                    } else {
                        TxOutcome::RolledBack
                    });
                });
                self.editors.close_tab_confirmed(i);
            }
            _ => {
                // 묻는 팝업은 지금 세션에 답을 보낸다 → 그 탭을 먼저 앞으로.
                if self.sess.id != sid {
                    self.editors.switch(i);
                    self.sync_grid_tab();
                }
                self.tx_after = Some(TxAfter::CloseTab(i));
                self.open_tx_guard(Msg::MnTxCommitClose, Msg::MnTxRollbackClose);
            }
        }
    }

    /// 종료 — 미커밋이 있으면 묻는다.
    /// 종료 마무리(저장 · 창 크기 · 워커 종료 · 루프 종료) — 창 이벤트 끝과 유휴 틱 **둘 다**에서 부른다(09-24: 기동 명령·타이머로
    /// 요청한 종료가 창 이벤트가 없으면 영영 처리되지 않았다 — `scripts/func-block-comment.sh`가 잡음).
    fn finish_exit(&mut self, el: &ActiveEventLoop) {
        self.flush_on_exit();
        self.persist_window_sizes(true);
        for s in self.all_sess() {
            s.worker.send(worker::Cmd::Quit);
        }
        el.exit();
    }

    fn request_exit(&mut self) {
        // ★ 종료 흐름(사용자 09-23): ① 프로젝트 — 자동 저장이면 저장 · 아니면 묻기 ② 미저장 **파일** 탭마다 묻기(스크립트 탭은
        //   프로젝트에 본문이 보존되므로 프로젝트가 있으면 묻지 않는다 · 없으면 전부 묻는다) ③ 트랜잭션 확인 → 종료.
        if self.project.is_open() {
            if self.settings.flag("project.autosave") {
                let _ = self.project_save();
            } else if !self.exit_project_asked {
                self.exit_pending = true;
                self.ask_project_exit();
                return;
            }
        }
        let keep_scratch = self.project.is_open();
        let dirty = (0..self.editors.tab_count()).find(|&i| {
            self.editors.is_dirty(i) && !(keep_scratch && self.editors.path_of(i).is_none())
        });
        if let Some(i) = dirty {
            self.exit_pending = true;
            self.ask_save_close(i);
            return;
        }
        self.exit_pending = false;
        self.exit_project_asked = false;
        if self.sess.tx_pending.is_empty() {
            // 잠든 세션에 미커밋이 있으면 그 탭을 앞으로 꺼내 거기서 묻는다(세션마다 한 번씩 · docs/52 §8).
            let tab = self
                .parked
                .iter()
                .find(|s| !s.tx_pending.is_empty())
                .map(|s| s.owner.unwrap_or(s.tx_pending[0].editor));
            if let Some(tab) = tab {
                self.editors.switch_to_id(tab);
                self.sync_grid_tab();
            }
            if self.sess.tx_pending.is_empty() {
                self.exit_requested = true;
                return;
            }
        }
        self.tx_after = Some(TxAfter::Exit);
        self.open_tx_guard(Msg::MnTxCommitExit, Msg::MnTxRollbackExit);
        self.redraw();
    }

    /// 편집기 편집 명령 — 편집기 포커스일 때만 · 바뀌면 찾기 표시 갱신 + 상태줄 선택 수.
    fn editor_cmd(&mut self, cmd: EditCommand) {
        if self.focus != Focus::Editor {
            return;
        }
        if self.ed_mut().edit_command(cmd) {
            let n = self.editors.selection_count();
            if n > 1 {
                self.sess.status = tf(Msg::StSelections, &[&n.to_string()]);
            }
        }
    }

    // ───────────────────────── 코드 완성 · 아웃라인(docs/76) ─────────────────────────

    /// 캐럿 앞 식별자 길이(줄 안 · 트리거 판정용 · 본문 복사 0).
    fn intel_prefix_len(&self) -> usize {
        let ed = self.editors.cur();
        let buf = ed.buf();
        let caret = ed.caret();
        let l = buf.line_of(caret);
        let start = buf.line_start(l);
        let line = buf.line_text(l);
        let col = caret.saturating_sub(start);
        line.chars()
            .take(col)
            .collect::<Vec<char>>()
            .iter()
            .rev()
            .take_while(|c| c.is_alphanumeric() || **c == '_' || **c == '$' || **c == '#')
            .count()
    }

    /// 편집기가 사건을 처리한 뒤 — 글자면 트리거 규칙 · 열린 팝업은 이동/클릭에 닫힌다.
    fn intel_after_event(&mut self, ev: &InputEvent) {
        if !self.intel.cfg().enabled {
            return;
        }
        match ev {
            // Backspace = `Char('\u{8}')`: 열린 팝업은 접두가 남아 있으면 다시 거르고 없으면 닫는다.
            InputEvent::Char { c: '\u{8}', .. } => {
                if self.intel.is_open() {
                    if self.intel_prefix_len() == 0 {
                        self.intel.close();
                    } else {
                        self.intel_request(false);
                    }
                }
            }
            InputEvent::Char { c, .. } => {
                let n = self.intel_prefix_len();
                if self.intel.after_char(*c, n) {
                    self.intel_request(false);
                }
                // `(` = 시그니처 도움(내장 함수·DBMS_* 멤버 · 상태줄 · T-178).
                if *c == '(' {
                    self.intel_signature_help();
                }
            }
            InputEvent::Key { .. }
            | InputEvent::MouseDown { .. }
            | InputEvent::RightDown { .. }
                if self.intel.is_open() =>
            {
                self.intel.close();
            }
            _ => {}
        }
    }

    /// 코드 기능(아웃라인 · 완성 · Goto Symbol)을 이 탭에 써도 되는가 — 큰 파일 단계가 아니고 · **구문이 SQL**이고 · 본문이
    /// 이진스럽지 않아야(첫 8 KB에 NUL 없음 · U+FFFD 8개 미만). 아니면 `Some(안내)`를 돌려준다(사용자 09-23 "`.o` 파일 아웃라인 = 앱 종료 ·
    /// 적합하지 않은 파일은 아웃라인 무시"). 분석기 자체도 패닉하지 않게 고쳤지만(nsql-script fuzz 시험) 뜻 없는 심볼을 만들지 않는다.
    /// `light` = 가벼운 요청(수동 완성 · 시그니처 도움 — 창 방식이라 큰 파일 1단계에서도 된다) · 아니면(자동 팝업 · 아웃라인 · Goto Symbol)
    /// 1단계부터 끈다 · 2단계는 전부 끈다(09-24 §187 · 72).
    fn intel_unsuitable(&self, i: usize, light: bool) -> Option<Msg> {
        if self.editors.is_large(i) {
            let lvl = self.editors.large_level(i);
            if lvl >= 2 || !light {
                return Some(Msg::StLargeFileFeatureOff);
            }
        }
        if self.editors.syntax_name() != "SQL" {
            return Some(Msg::StIntelUnsuitable);
        }
        let buf = self.editors.cur().buf();
        let n = buf.line_count().min(64);
        let mut fffd = 0usize;
        for l in 0..n {
            let line = buf.line_text(l);
            if line.contains('\0') {
                return Some(Msg::StIntelUnsuitable);
            }
            fffd += line.matches('\u{fffd}').count();
            if fffd >= 8 {
                return Some(Msg::StIntelUnsuitable);
            }
        }
        None
    }

    /// 후보 조립 + 팝업(`manual` = Ctrl+Space · 자동 설정과 무관). 큰 파일 단계(L1+)·비SQL·이진 파일에서는 하지 않는다(72 §2).
    fn intel_request(&mut self, manual: bool) {
        if !self.intel.cfg().enabled || self.focus != Focus::Editor {
            return;
        }
        let i = self.editors.active();
        if let Some(why) = self.intel_unsuitable(i, manual) {
            if manual {
                self.sess.status = t(why).into();
            }
            return;
        }
        let tab = self.editors.tab_id(i);
        let (rev, text, caret_c, anchor) = {
            let ed = self.editors.cur();
            (ed.rev(), ed.text(), ed.caret(), ed.caret_point())
        };
        let caret_b = text
            .char_indices()
            .nth(caret_c)
            .map_or(text.len(), |(b, _)| b);
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or_default();
        let spec = self.sess.spec.clone();
        let dialect = Some(self.sess.dialect);
        let (names, snap) = self.explorer.meta_view(spec.as_ref());
        let view = intel::MetaView { names, snap };
        // 접두 시작 글자의 좌표(바이트 → 글자 인덱스 → 마지막 그리기의 줄 배치) — 팝업이 타이핑 중 제자리에 있게(09-23).
        let ed = self.editors.cur();
        let point_of = |b: usize| ed.point_at(text[..b.min(text.len())].chars().count());
        let opened = self.intel.request(
            tab,
            rev,
            &text,
            caret_b,
            dialect,
            Some(&view),
            anchor,
            host,
            self.scale,
            &point_of,
        );
        let needs = self.intel.take_needs();
        for n in needs {
            self.explorer
                .request_columns(spec.as_ref(), n.schema.as_deref(), &n.table, n.urgent);
        }
        // ★ 객체 목록 즉시 채움(`스키마.` · 현재 스키마 · 사전 — 09-23): 탐색기 메타 세션 1건씩 · 오면 아래 drain이 팝업을 다시 그린다.
        for s in self.intel.take_need_objects() {
            self.explorer.request_objects(spec.as_ref(), &s);
        }
        self.intel_card_settle();
        // 예산 초과 = 로그 창 한 줄(개발자 상세 · D-202의 근거).
        if let Some((n, ms)) = self.intel.take_over_budget() {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!(
                    "[intel] candidates={n} took {ms} ms (> intel.budget_ms {})",
                    self.intel.cfg().budget_ms
                ),
            ));
        }
        // 자기 감속(§187): 연속 초과로 이 탭의 문서 낱말이 꺼졌다 — 상태줄 한 번.
        if self.intel.take_degraded().is_some() {
            self.sess.status = t(Msg::StIntelDegraded).into();
        }
        if opened || manual {
            self.redraw();
        }
    }

    /// 상세 카드가 쓸 메타(테이블 상세·컬럼)를 미리 요청(강조 행이 바뀔 때 · 이미 있으면 0 · 09-24).
    /// ★ 인텔리센스 캐시 명시 갱신(79 §4 · T-188): 현재 스키마 / 이 서버 / 전 서버 — 버킷 `Stale`·컬럼 `Unknown` + 그 스키마(전부면
    /// 현재 스키마·사전) 즉시 다시 읽기 · 상태줄 · 열린 팝업은 닫는다(다음 요청이 새 것으로).
    fn intel_refresh(&mut self, id: &str) {
        let spec = self.sess.spec.clone();
        let (schema, all) = match id {
            "intel.refresh_all" => (None, true),
            "intel.refresh_server" => (None, false),
            _ => (self.explorer.current_schema(spec.as_ref()), false),
        };
        let (b, o) = self
            .explorer
            .refresh_meta(spec.as_ref(), schema.as_deref(), all);
        self.intel.close();
        self.sess.status = tf(Msg::StIntelRefreshed, &[&b.to_string(), &o.to_string()]);
        self.redraw();
    }

    /// 강조 행이 바뀐 뒤: 목표만 적고, 머문 대상이 넘어왔을 때만 선조회(빠른 스크롤 중 서버 상세 질의 0 · 09-24).
    fn intel_card_settle(&mut self) {
        let now = Instant::now();
        self.intel.note_hover(now);
        if self.intel.card_tick(now) {
            self.intel_card_prefetch();
        }
    }

    fn intel_card_prefetch(&mut self) {
        if !self.intel.cfg().detail_card {
            return;
        }
        if let Some(id) = self.intel.card_target().and_then(|t| t.needs()) {
            let spec = self.sess.spec.clone();
            self.explorer.request_detail(spec.as_ref(), id);
        }
    }

    /// 확정 글자를 편집기에(접두 구간 교체 · `caret_back` = `NAME()` 안으로).
    fn intel_apply(&mut self) {
        let Some(a) = self.intel.take_accept() else {
            return;
        };
        let text = self.editors.cur().text();
        let from = text[..a.replace.start.min(text.len())].chars().count();
        let to = text[..a.replace.end.min(text.len())].chars().count();
        let mut inv = Invalidations::default();
        self.ed_mut().replace_range(from, to, &a.text, &mut inv);
        if a.caret_back > 0 {
            let at = (from + a.text.chars().count()).saturating_sub(a.caret_back);
            self.ed_mut().select_range(at, at, &mut inv);
            // 괄호 안에서 바로 시그니처(확정한 함수의).
            self.intel_signature_help();
        }
        self.redraw();
    }

    /// 시그니처 도움(`intel.signature_help`): 캐럿을 감싸는 `(`의 주인이 내장 함수면 상태줄에 시그니처 한 줄.
    fn intel_signature_help(&mut self) {
        if !self.intel.cfg().signature_help || self.focus != Focus::Editor {
            return;
        }
        let i = self.editors.active();
        if self.intel_unsuitable(i, true).is_some() {
            return;
        }
        let (text, caret_c) = {
            let ed = self.editors.cur();
            (ed.text(), ed.caret())
        };
        let caret_b = text
            .char_indices()
            .nth(caret_c)
            .map_or(text.len(), |(b, _)| b);
        if let Some(sig) = self
            .intel
            .signature_at(&text, caret_b, Some(self.sess.dialect))
        {
            self.sess.status = tf(Msg::StIntelSignature, &[&sig]);
            self.redraw();
        }
    }

    /// Goto Symbol(Ctrl+R · Sublime): 문서 아웃라인 심볼 목록을 팔레트에 — 고르면 `sym:<byte>`.
    fn open_goto_symbol(&mut self) {
        let i = self.editors.active();
        if let Some(why) = self.intel_unsuitable(i, false) {
            self.sess.status = t(why).into();
            self.redraw();
            return;
        }
        let tab = self.editors.tab_id(i);
        let (rev, text) = {
            let ed = self.editors.cur();
            (ed.rev(), ed.text())
        };
        let dialect = Some(self.sess.dialect);
        let ol = self
            .intel
            .outline_for(tab, rev, &|| text.clone(), dialect)
            .clone();
        let mut cmds: Vec<(String, String)> = Vec::new();
        for s in &ol.symbols {
            let indent = "  ".repeat(s.depth as usize);
            let detail = if s.detail.is_empty() {
                s.kind.label().to_string()
            } else {
                format!("{} · {}", s.kind.label(), s.detail)
            };
            cmds.push((
                format!("sym:{}", s.byte),
                format!("{indent}{}  —  {detail}  :{}", s.name, s.line),
            ));
        }
        if cmds.is_empty() {
            self.sess.status = t(Msg::OutlineEmpty).into();
            self.redraw();
            return;
        }
        self.palette.set_commands(cmds);
        self.palette.open("");
        self.ime_refresh();
        self.redraw();
    }

    /// 아웃라인 패널 동기 — 활성 탭·본문 세대가 바뀌었을 때만 심볼을 다시 준다(큰 파일 단계는 비움).
    fn outline_sync(&mut self) {
        if !self.outline_panel.is_visible() {
            return;
        }
        let i = self.editors.active();
        let tab = self.editors.tab_id(i);
        let rev = self.editors.cur().rev();
        if self.outline_panel.key() == Some((tab, rev)) {
            return;
        }
        if !self.intel.cfg().enabled || self.intel_unsuitable(i, false).is_some() {
            // 적합하지 않은 파일(큰 파일 · SQL 아님 · 이진) = 빈 아웃라인(사용자 09-23).
            self.outline_panel
                .set_symbols((tab, rev), &nsql_script::outline::Outline::default());
            return;
        }
        let text = self.editors.cur().text();
        let dialect = Some(self.sess.dialect);
        let ol = self
            .intel
            .outline_for(tab, rev, &|| text.clone(), dialect)
            .clone();
        self.outline_panel.set_symbols((tab, rev), &ol);
    }

    /// 아웃라인 패널의 열기 요청 → 그 자리로.
    fn outline_pump(&mut self) {
        if let Some(b) = self.outline_panel.take_open() {
            self.goto_byte(b);
        }
    }

    /// 바이트 오프셋으로 캐럿 이동(심볼 이동).
    fn goto_byte(&mut self, byte: usize) {
        let text = self.editors.cur().text();
        let idx = text[..byte.min(text.len())].chars().count();
        let mut inv = Invalidations::default();
        self.ed_mut().select_range(idx, idx, &mut inv);
        self.set_focus(Focus::Editor);
        self.redraw();
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
        self.ime_refresh();
        self.redraw();
    }

    /// 찾기 패널이 열려 있으면 일치 구간 전부를 편집기에 표시(반투명 · T-73) · 닫혀 있으면 지운다.
    fn sync_find_marks(&mut self) {
        let marks = if self.find.is_visible() {
            self.find_matches()
        } else {
            Vec::new()
        };
        self.ed_mut().set_find_marks(marks);
    }

    fn build_menus() -> Vec<MenuDef> {
        Self::build_menus_with(&[], &[], false, false, Vec::new())
    }

    /// 메뉴 정의 — File 메뉴 아래쪽에 최근 파일(최대 8 · Eclipse/DBeaver 관례).
    /// `blocked` = 지금 탭의 세션이 작업 중(docs/52 §3 통제) — Run 메뉴의 실행 계열은 비활성으로(툴바와 같은 판정 · 09-19).
    fn build_menus_with(
        recent: &[PathBuf],
        tabs: &[(u64, String, bool)],
        demo_ready: bool,
        blocked: bool,
        project: Vec<MenuEntry>,
    ) -> Vec<MenuDef> {
        let item = |id: &str, m: Msg| MenuEntry::Item(ComboItem::new(id, t(m)));
        let gated = |id: &str, m: Msg| {
            if blocked {
                MenuEntry::Disabled(ComboItem::new(id, t(m)))
            } else {
                MenuEntry::Item(ComboItem::new(id, t(m)))
            }
        };
        let mut file = vec![
            item("file.new", Msg::MnNew),
            item("file.open", Msg::MnOpen),
            item("file.save", Msg::MnSave),
            item("file.save_as", Msg::MnSaveAs),
            MenuEntry::Separator,
            item("file.run_file", Msg::MnRunFile),
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
            MenuDef::new(t(Msg::MnProject), project),
            // ★ Edit 메뉴 = 그룹(하위 메뉴 · 사용자 09-22 "1레벨로 너무 길다 — Sublime/VS Code/IntelliJ 참고"):
            //   Sublime의 Edit(Line ▸ · Comment ▸ · Convert Case ▸) + Selection 메뉴 + Find 메뉴를 한 메뉴 안 그룹으로 접었다 ·
            //   자주 쓰는 Undo/Redo · Cut/Copy/Paste · Select All · Toggle Comment · Go to Line · Preferences는 1레벨 유지
            //   (VS Code Edit 메뉴와 같은 자리) · 설정 = 맨 아래(IntelliJ/Sublime Preferences 자리).
            MenuDef::new(
                t(Msg::MnEdit),
                vec![
                    item("edit.undo", Msg::MnUndo),
                    item("edit.redo", Msg::MnRedo),
                    // Sublime Edit ▸ Undo Selection ▸ Soft Undo / Soft Redo.
                    MenuEntry::sub(
                        t(Msg::MnGrpUndoSelection),
                        vec![
                            item("edit.soft_undo", Msg::MnSoftUndo),
                            item("edit.soft_redo", Msg::MnSoftRedo),
                        ],
                    ),
                    MenuEntry::Separator,
                    item("edit.cut", Msg::MnCut),
                    item("edit.copy", Msg::MnCopy),
                    item("edit.paste", Msg::MnPaste),
                    MenuEntry::Separator,
                    item("edit.select_all", Msg::MnSelectAll),
                    MenuEntry::sub(
                        t(Msg::MnGrpSelection),
                        vec![
                            item("edit.expand_selection", Msg::MnExpandSelection),
                            item("edit.skip_occurrence", Msg::MnSkipOccurrence),
                            item("edit.select_all_occurrences", Msg::MnSelectAllOccurrences),
                            MenuEntry::Separator,
                            item("edit.select_line", Msg::MnSelectLine),
                            item("edit.split_lines", Msg::MnSplitLines),
                            item("edit.add_caret_up", Msg::MnAddCaretUp),
                            item("edit.add_caret_down", Msg::MnAddCaretDown),
                            MenuEntry::Separator,
                            item("edit.expand_brackets", Msg::MnExpandBrackets),
                        ],
                    ),
                    MenuEntry::sub(
                        t(Msg::MnGrpBrackets),
                        vec![
                            item("edit.complete", Msg::MnComplete),
                            item("goto.symbol", Msg::MnGotoSymbol),
                            MenuEntry::Separator,
                            item("edit.goto_bracket", Msg::MnGotoBracket),
                            item("edit.bracket_prev", Msg::MnBracketPrev),
                            item("edit.bracket_next", Msg::MnBracketNext),
                            item("edit.bracket_parent", Msg::MnBracketParent),
                            item("edit.bracket_child", Msg::MnBracketChild),
                        ],
                    ),
                    MenuEntry::sub(
                        t(Msg::MnGrpLine),
                        vec![
                            item("edit.indent", Msg::MnIndent),
                            item("edit.unindent", Msg::MnUnindent),
                            MenuEntry::Separator,
                            item("edit.swap_line_up", Msg::MnSwapLineUp),
                            item("edit.swap_line_down", Msg::MnSwapLineDown),
                            MenuEntry::Separator,
                            item("edit.duplicate_line", Msg::MnDuplicateLine),
                            item("edit.delete_line", Msg::MnDeleteLine),
                            item("edit.join_lines", Msg::MnJoinLines),
                        ],
                    ),
                    MenuEntry::sub(
                        t(Msg::MnGrpConvertCase),
                        vec![
                            item("edit.upper_case", Msg::MnUpperCase),
                            item("edit.lower_case", Msg::MnLowerCase),
                        ],
                    ),
                    item("edit.toggle_comment", Msg::MnToggleComment),
                    item("edit.toggle_block_comment", Msg::MnToggleBlockComment),
                    MenuEntry::Separator,
                    // 인텔리센스 캐시 새로 고침(79 §4 · 현재 스키마 · 이 서버 · 전 서버).
                    item("intel.refresh", Msg::MnIntelRefresh),
                    item("intel.refresh_server", Msg::MnIntelRefreshServer),
                    item("intel.refresh_all", Msg::MnIntelRefreshAll),
                    MenuEntry::Separator,
                    // 북마크(docs/69 §6-1).
                    MenuEntry::sub(
                        t(Msg::MnBookmarks),
                        vec![
                            item("bookmark.toggle", Msg::MnBmToggle),
                            item("bookmark.next", Msg::MnBmNext),
                            item("bookmark.prev", Msg::MnBmPrev),
                            MenuEntry::Separator,
                            item("bookmark.select_all", Msg::MnBmSelectAll),
                            item("bookmark.label", Msg::MnBmLabel),
                            item("bookmark.clear_doc", Msg::MnBmClearDoc),
                            MenuEntry::Separator,
                            item("view.bookmarks", Msg::MnBookmarksPanel),
                        ],
                    ),
                    MenuEntry::sub(
                        t(Msg::MnGrpFind),
                        vec![
                            item("edit.find", Msg::MnFind),
                            item("edit.replace", Msg::MnReplace),
                            MenuEntry::Separator,
                            item("edit.find_next", Msg::MnFindNext),
                            item("edit.find_prev", Msg::MnFindPrev),
                        ],
                    ),
                    item("edit.goto_line", Msg::MnGotoLine),
                    MenuEntry::Separator,
                    MenuEntry::sub(
                        t(Msg::MnGrpLineEndings),
                        vec![
                            item("eol.crlf", Msg::MnEolCrlf),
                            item("eol.lf", Msg::MnEolLf),
                            item("eol.cr", Msg::MnEolCr),
                        ],
                    ),
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
                    item("view.project", Msg::MnProjectPanel),
                    item("view.log", Msg::MnLogWindow),
                    item("view.txlog", Msg::MnTxLogWindow),
                    item("view.sessions", Msg::MnSessManager),
                    item("view.memory", Msg::MnMemoryWindow),
                    item("view.variables", Msg::MnVariables),
                    item("vars.show", Msg::MnShowVariables),
                    item("vars.script", Msg::MnVariablesScript),
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
                    gated("run.statement", Msg::MnRunStatement),
                    gated("run.statement_new_tab", Msg::MnRunStatementNewTab),
                    gated("run.all", Msg::MnRunAll),
                    gated("run.explain", Msg::MnExplain),
                    MenuEntry::Separator,
                    gated("run.commit", Msg::MnCommit),
                    gated("run.rollback", Msg::MnRollback),
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
            self.menubar.set_menus(App::build_menus_with(
                &recent,
                &tabs,
                self.demo_ready,
                self.gate_shown.unwrap_or(false),
                self.project_menu_entries(),
            ));
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
                ToolItem::new("conn.sessions", toolicons::sessions()).tip(t(Msg::TipSessions)),
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
                        self.sess.status = tf(Msg::StCopied, &[&n.to_string()]);
                    }
                }
            }
            // ★ 큰 선택 복사/잘라내기 확인(사용자 09-22 · docs/72 §2): 문자열을 만들기 전에 바이트 수로 판정 — 첫 누름은 안내,
            //   3초 안에 같은 동작 = 실행(거대 편집 확인과 같은 꼴).
            EditCtxAction::Copy | EditCtxAction::Cut
                if self.focus == Focus::Editor && self.copy_confirm_pending() => {}
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
            self.sess.status = t(Msg::ErrClipboard).into();
        }
        self.redraw();
    }

    /// 큰 선택 복사/잘라내기 확인이 필요한가 — `editor.copy_confirm_mb`(0 = 안 물음) 이상이면 첫 누름은 안내(3초 무장) · 무장 중 되풀이 = 통과.
    fn copy_confirm_pending(&mut self) -> bool {
        let limit = (self.settings.int("editor.copy_confirm_mb").max(0) as usize) << 20;
        if limit == 0 {
            return false;
        }
        let bytes = self.ed_mut().selected_bytes();
        if bytes < limit {
            return false;
        }
        let now = Instant::now();
        if self.copy_armed_until.is_some_and(|t| now <= t) {
            self.copy_armed_until = None;
            return false;
        }
        self.copy_armed_until = Some(now + Duration::from_secs(3));
        self.sess.status = tf(Msg::StCopyConfirm, &[&nsql_core::fmt_bytes(bytes as u64)]);
        true
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
        cmds.push(m("file.follow", Msg::MnFile, Msg::MnFileFollow));
        cmds.push(m("file.run_file", Msg::MnFile, Msg::MnRunFile));
        cmds.push(m("file.large_force", Msg::MnFile, Msg::MnLargeForce));
        cmds.push(m("mem.trim_now", Msg::MnView, Msg::MnMemTrimNow));
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
        cmds.push(m("edit.complete", Msg::MnEdit, Msg::MnComplete));
        cmds.push(m("goto.symbol", Msg::MnEdit, Msg::MnGotoSymbol));
        // Extension Manager(Sublime "Package Control: …" 표기 · docs/50 §10).
        // 관리자 상태에 맞는 명령만(사용자 09-19): 꺼짐 = "켜기" 하나 · 켜짐 = "끄기" + 나머지(두 번째 "켜기"는 없다).
        if self.settings.flag("extensions.enabled") {
            cmds.push(m(
                "ext.disable_mgr",
                Msg::MnExtensions,
                Msg::MnExtDisableManager,
            ));
            cmds.push(m("view.extensions", Msg::MnView, Msg::MnExtensionsPanel));
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
        } else {
            cmds.push(m(
                "ext.enable_mgr",
                Msg::MnExtensions,
                Msg::MnExtEnableManager,
            ));
        }
        cmds.push(m(
            "edit.expand_brackets",
            Msg::MnEdit,
            Msg::MnExpandBrackets,
        ));
        cmds.push(m(
            "edit.skip_occurrence",
            Msg::MnEdit,
            Msg::MnSkipOccurrence,
        ));
        for (id, msg) in [
            ("bookmark.toggle", Msg::MnBmToggle),
            ("bookmark.next", Msg::MnBmNext),
            ("bookmark.prev", Msg::MnBmPrev),
            ("bookmark.select_all", Msg::MnBmSelectAll),
            ("bookmark.label", Msg::MnBmLabel),
            ("bookmark.clear_doc", Msg::MnBmClearDoc),
            ("view.bookmarks", Msg::MnBookmarksPanel),
        ] {
            cmds.push(m(id, Msg::MnBookmarks, msg));
        }
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
        cmds.push(m("edit.soft_undo", Msg::MnEdit, Msg::MnSoftUndo));
        cmds.push(m("edit.soft_redo", Msg::MnEdit, Msg::MnSoftRedo));
        cmds.push(m("view.log", Msg::MnView, Msg::MnLogWindow));
        cmds.push(m("view.txlog", Msg::MnView, Msg::MnTxLogWindow));
        cmds.push(m("view.sessions", Msg::MnView, Msg::MnSessManager));
        cmds.push(m("view.memory", Msg::MnView, Msg::MnMemoryWindow));
        cmds.push(m("view.toolbar_reset", Msg::MnView, Msg::MnResetToolbar));
        cmds.push(m("view.colors", Msg::MnView, Msg::MnColors));
        cmds.push(m("view.keys", Msg::MnView, Msg::MnKeys));
        cmds.push(m("view.explorer", Msg::MnView, Msg::MnExplorer));
        cmds.push(m("view.search", Msg::MnView, Msg::MnSearchPanel));
        cmds.push(m("view.project", Msg::MnView, Msg::MnProjectPanel));
        cmds.push(m("project.new", Msg::MnProject, Msg::MnProjectNew));
        cmds.push(m("project.open", Msg::MnProject, Msg::MnProjectOpen));
        cmds.push(m("project.switch", Msg::MnProject, Msg::MnProjectSwitch));
        cmds.push(m("project.save", Msg::MnProject, Msg::MnProjectSave));
        cmds.push(m("project.save_as", Msg::MnProject, Msg::MnProjectSaveAs));
        cmds.push(m("project.close", Msg::MnProject, Msg::MnProjectClose));
        cmds.push(m(
            "project.add_folder",
            Msg::MnProject,
            Msg::MnProjectAddFolder,
        ));
        cmds.push(m("view.variables", Msg::MnView, Msg::MnVariables));
        cmds.push(m("vars.script", Msg::MnView, Msg::MnVariablesScript));
        cmds.push(m("vars.show", Msg::MnView, Msg::MnShowVariables));
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
            ("edit.toggle_block_comment", Msg::MnToggleBlockComment),
            ("intel.refresh", Msg::MnIntelRefresh),
            ("intel.refresh_server", Msg::MnIntelRefreshServer),
            ("intel.refresh_all", Msg::MnIntelRefreshAll),
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
        self.ime_refresh();
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
            } else if let Some(w) = self.txlog_win.window().filter(|w| w.id() == *wid) {
                wins.push(w);
            } else if let Some(w) = self.sessions_win.window().filter(|w| w.id() == *wid) {
                wins.push(w);
            } else if let Some(w) = self.vars_win.window().filter(|w| w.id() == *wid) {
                wins.push(w);
            } else if let Some(w) = self.mem_win.window().filter(|w| w.id() == *wid) {
                wins.push(w);
            } else if let Some(w) = self.prefs_win.window().filter(|w| w.id() == *wid) {
                wins.push(w);
            } else if let Some(w) = self.colors_win.window().filter(|w| w.id() == *wid) {
                wins.push(w);
            } else if let Some(w) = self.keys_win.window().filter(|w| w.id() == *wid) {
                wins.push(w);
            } else if let Some(w) = self.sqlprev_win.window().filter(|w| w.id() == *wid) {
                wins.push(w);
            }
        }
        // ★ 보조 창(메모리 창 등)을 골라도 메인·다른 창이 함께 앞으로(사용자 09-24) — 맥은 `orderFront:` · 고른 창이 맨 위.
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
        self.menubar.set_menus(App::build_menus_with(
            &v,
            &tabs,
            self.demo_ready,
            self.gate_shown.unwrap_or(false),
            self.project_menu_entries(),
        ));
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
            (
                PickerMode::Save,
                FilePurpose::Editor | FilePurpose::RunFile | FilePurpose::SettingFolder(_),
            ) => {
                let t = self.editors.active_title();
                if t.contains('.') {
                    t
                } else {
                    format!("{t}.sql")
                }
            }
            (PickerMode::Save, FilePurpose::Project) => format!(
                "{}.{}",
                self.project.name().unwrap_or_else(|| "untitled".into()),
                project::EXT
            ),
            (PickerMode::Save, FilePurpose::SqlPreview) => self.sqlprev_win.file_name(),
            (PickerMode::Open | PickerMode::Folder, _) => String::new(),
        };
        // 폴더 고르기: 시작 = 그 설정의 지금 값 · 주인 = 설정 창(그 위에 뜬다).
        let (start, owner, over) = if mode == PickerMode::Folder {
            let pw = self.prefs_win.window_rc();
            let over2 = pw.as_ref().and_then(|w| {
                let p = w.outer_position().ok()?;
                let sz = w.outer_size();
                Some((p.x, p.y, sz.width, sz.height))
            });
            (
                self.folder_start.take().or(start),
                pw.or(owner),
                over2.or(over),
            )
        } else {
            (start, owner, over)
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
            PickerMode::Open | PickerMode::Folder => "auto".to_string(),
            PickerMode::Save => self.editors.active_encoding(),
        };
        self.file_win
            .set_overwrite_confirm_ms(self.settings.int("file.overwrite_confirm_ms").max(0) as u64);
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
            // 편집기 파일 열기만 다중 · 프로젝트/실행 파일/내보내기/설정 폴더는 하나(사용자 09-22).
            !matches!(self.file_purpose, FilePurpose::Editor),
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
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let ask = (self.settings.int("file.large_ask_mb").max(0) as u64) << 20;
        // ★ 큰 파일(docs/59 §4 3단계): 기준을 넘으면 **먼저 묻는다** — 열기 / 읽기 전용 / 앞부분만 / 열지 않고 실행.
        if ask > 0 && size >= ask {
            self.big_pending = Some((path.to_path_buf(), enc.to_string(), size));
            let mb = format!("{:.0}", size as f64 / 1048576.0);
            let head = self.settings.int("file.large_head_mb").max(1).to_string();
            self.palette.set_commands(vec![
                ("bigfile.open".into(), tf(Msg::BigOpen, &[&mb])),
                ("bigfile.readonly".into(), t(Msg::BigReadOnly).into()),
                ("bigfile.head".into(), tf(Msg::BigHead, &[&head])),
                ("bigfile.run".into(), t(Msg::BigRun).into()),
            ]);
            self.palette.open("");
            self.ime_refresh();
            self.redraw();
            return;
        }
        self.load_file(path, enc, LoadMode::Open);
    }

    /// 파일을 읽어 `mode`대로 쓴다. 큰 파일(`file.async_load_mb` 이상)은 **자리 탭**을 먼저 만들고 작업 스레드가 읽기·풀기·
    /// 편집기 본문 준비까지 끝낸다 — 그동안 진행 막은 그 탭 안에만 보이고 다른 탭은 그대로 쓴다(`fileload.rs`).
    fn load_file(&mut self, path: &Path, enc: &str, mode: LoadMode) {
        let to_tab = mode != LoadMode::Run;
        if matches!(mode, LoadMode::Open | LoadMode::ReadOnly) {
            // 이미 열려 있으면 읽지 않고 그 탭으로 · 같은 파일을 읽는 중이면 그 자리 탭으로.
            if let Some(i) = self.editors.path_tab(path) {
                self.editors.switch(i);
                self.set_focus(Focus::Editor);
                self.layout();
                self.redraw();
                return;
            }
            let dup = self
                .file_loads
                .iter()
                .find(|j| j.path == path)
                .and_then(|j| j.tab);
            if let Some(i) = dup.and_then(|id| self.editors.index_of_id(id)) {
                self.editors.switch(i);
                self.redraw();
                return;
            }
        }
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let limit = match mode {
            LoadMode::Head(n) => Some(n),
            _ => None,
        };
        let origin = self.editors.active_id();
        let async_at = (self.settings.int("file.async_load_mb").max(1) as u64) << 20;
        if size < async_at {
            let result = fileload::load(path, enc, limit, to_tab, None);
            self.file_loaded(FileLoaded {
                path: path.to_path_buf(),
                mode,
                tab: None,
                origin,
                result,
            });
            return;
        }
        let name = editors::file_title(path);
        let (tx, rx) = std::sync::mpsc::channel();
        let (p, e) = (path.to_path_buf(), enc.to_string());
        let proxy = std::sync::Mutex::new(self.wake_proxy.clone());
        let prog = std::sync::Arc::new(fileload::Progress::default());
        let prog_t = prog.clone();
        let spawned = std::thread::Builder::new()
            .name("file-load".into())
            .spawn(move || {
                let result = fileload::load(&p, &e, limit, to_tab, Some(&prog_t));
                let _ = tx.send(result);
                if let Ok(px) = proxy.lock() {
                    let _ = px.send_event(Wake);
                }
            });
        if spawned.is_err() {
            let result = fileload::load(path, enc, limit, to_tab, None);
            self.file_loaded(FileLoaded {
                path: path.to_path_buf(),
                mode,
                tab: None,
                origin,
                result,
            });
            return;
        }
        // 자리 탭(편집기에 실을 때만) — 비어 있고 읽기 전용 · 경로 없음. "앞부분만"은 읽을거리라 No connection.
        let tab = to_tab.then(|| {
            let id = self.editors.begin_load_tab(&name);
            if matches!(mode, LoadMode::Head(_)) {
                self.make_unconnected(id);
            }
            id
        });
        self.file_loads.push(LoadJob {
            rx,
            path: path.to_path_buf(),
            mode,
            tab,
            origin,
            name,
            total: limit.map_or(size, |n| n.min(size)),
            started: Instant::now(),
            prog,
            ready: None,
            shown_full: false,
        });
        self.sess.status = tf(
            Msg::StFileLoading,
            &[&nexa_fs::path::display(path), &nsql_core::fmt_bytes(size)],
        );
        if to_tab {
            self.set_focus(Focus::Editor);
        }
        self.layout();
        self.redraw();
    }

    /// 적재 스레드의 결과 수거(틱). 자리 탭이 닫혔으면 그 적재를 취소한다. 진행 막이 보이는 중(자리 탭이 활성 + 지연 지남)이면
    /// **100% 프레임을 한 번 그린 뒤에** 옮겨 넣는다 — 사용자가 본 마지막 값이 100%가 되게.
    fn file_loads_poll(&mut self) {
        if self.file_loads.is_empty() {
            return;
        }
        let delay = Duration::from_millis(self.settings.int("file.load_progress_ms").max(0) as u64);
        let active = self.editors.active_id();
        let mut done = Vec::new();
        let mut k = 0;
        while k < self.file_loads.len() {
            // 자리 탭을 닫았다 = 그 적재 취소(스레드는 다음 덩어리에서 멈춘다).
            let closed = self.file_loads[k]
                .tab
                .is_some_and(|id| self.editors.index_of_id(id).is_none());
            if closed {
                let job = self.file_loads.remove(k);
                job.prog.cancel();
                self.sess.status = tf(Msg::StLoadCancelled, &[&job.name]);
                self.mem_released();
                continue;
            }
            let job = &mut self.file_loads[k];
            if job.ready.is_none() {
                match job.rx.try_recv() {
                    Ok(r) => job.ready = Some(r),
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        job.ready = Some(Err("file-load thread ended".into()));
                    }
                }
            }
            let visible =
                job.tab == Some(active) && fileload::overlay_due(job.started.elapsed(), delay);
            if job.ready.is_some() && (!visible || job.shown_full) {
                let job = self.file_loads.remove(k);
                if let Some(result) = job.ready {
                    done.push(FileLoaded {
                        path: job.path,
                        mode: job.mode,
                        tab: job.tab,
                        origin: job.origin,
                        result,
                    });
                }
                continue;
            }
            k += 1;
        }
        for l in done {
            self.file_loaded(l);
        }
        // 막이 보이는 동안만 계속 다시 그린다(다른 탭에서 일하는 중에는 프레임을 만들지 않는다).
        if self.file_loads.iter().any(|j| j.tab == Some(active)) {
            self.redraw();
        }
    }

    /// 활성 탭의 적재 취소(Esc · `file.load_cancel`): 스레드에 알리고 자리 탭을 닫는다. 돌려주는 값 = 취소했는가.
    fn file_load_cancel_active(&mut self) -> bool {
        let active = self.editors.active_id();
        let Some(k) = self.file_loads.iter().position(|j| j.tab == Some(active)) else {
            return false;
        };
        let job = self.file_loads.remove(k);
        job.prog.cancel();
        if let Some(i) = self.editors.index_of_id(active) {
            self.editors.close_tab_confirmed(i);
            self.editors.reap_views();
        }
        self.sess.status = tf(Msg::StLoadCancelled, &[&job.name]);
        self.mem_released();
        self.layout();
        self.redraw();
        true
    }

    /// 진행 막 그리기(편집 영역 가운데) — **활성 탭이 적재 중인 탭일 때만** · 지연이 지난 뒤. 100%를 그렸으면 표시해 둔다
    /// (수거가 그걸 보고 옮겨 넣는다). 그리는 동안 `surface`가 `self`를 잡고 있어 필드만 받는 연관 함수로 둔다.
    fn paint_file_load(
        jobs: &mut [LoadJob],
        active: u64,
        delay: Duration,
        area: Rect,
        dc: &mut dyn nexa_ctl::draw::DrawCtx,
        th: &nexa_ctl::theme::Theme,
        s: f32,
    ) {
        let more = jobs.len().saturating_sub(1);
        let Some(job) = jobs.iter_mut().find(|j| j.tab == Some(active)) else {
            return;
        };
        if !fileload::overlay_due(job.started.elapsed(), delay) {
            return;
        }
        let stage = if job.ready.is_some() {
            fileload::Stage::Opening
        } else {
            // 스레드는 끝났어도 아직 받지 않았으면 그 앞 단계에 둔다 — 100%는 "받았다"는 뜻으로만 쓴다.
            match job.prog.stage() {
                fileload::Stage::Opening => fileload::Stage::Preparing,
                st => st,
            }
        };
        fileload::paint(
            dc,
            th,
            area,
            s,
            &fileload::View {
                name: &job.name,
                stage,
                read: job.prog.read(),
                total: job.total,
                started: job.started,
                more,
            },
        );
        if stage == fileload::Stage::Opening {
            job.shown_full = true;
        }
    }

    /// 읽은 결과를 쓴다 — 편집기에 실을 것은 자리 탭(없으면 지금 만든다 = 작은 파일의 동기 경로)에 **옮겨 넣고**,
    /// 실행만 할 것은 돌린다. 활성 탭은 바꾸지 않는다(사용자가 다른 탭에서 일하고 있을 수 있다).
    fn file_loaded(&mut self, l: FileLoaded) {
        let path = l.path;
        let name = editors::file_title(&path);
        let loaded = match l.result {
            Ok(v) => v,
            Err(e) => {
                // 자리 탭은 걷는다(빈 탭이 남지 않게).
                if let Some(i) = l.tab.and_then(|id| self.editors.index_of_id(id)) {
                    self.editors.close_tab_confirmed(i);
                    self.editors.reap_views();
                }
                self.sess.status = if e == fileload::CANCELLED {
                    tf(Msg::StLoadCancelled, &[&name])
                } else {
                    tf(Msg::StFileReadError, &[&path.display().to_string(), &e])
                };
                self.layout();
                self.redraw();
                return;
            }
        };
        let (eol, used, lossy, truncated) =
            (loaded.eol, loaded.used, loaded.lossy, loaded.truncated);
        match (l.mode, loaded.body) {
            (LoadMode::Run, fileload::Body::Text(text)) => {
                // ★ 읽는 사이 활성 탭이 바뀌었으면 실행하지 않는다 — 실행은 활성 탭의 세션으로 가므로, 사용자가 고른 것과
                //   **다른 접속**(운영일 수도 있다)에서 큰 스크립트가 돌 뻔한 경우다.
                if self.editors.active_id() != l.origin {
                    let msg = tf(Msg::StRunFileTabChanged, &[&name]);
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, msg.clone()));
                    self.sess.status = msg;
                } else if self.gate_open() {
                    let n = nsql_script::split_script(&text).len();
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        tf(Msg::StRunFile, &[&name, &n.to_string()]),
                    ));
                    self.run_text(text, 0, true);
                }
            }
            (mode, fileload::Body::Prepared(prep)) => {
                let head = matches!(mode, LoadMode::Head(_));
                let tab = match l.tab {
                    Some(id) => id,
                    None => {
                        let id = self.editors.begin_load_tab(&name);
                        if head {
                            self.make_unconnected(id);
                        }
                        id
                    }
                };
                // "앞부분만" = 경로 없는 읽기 전용 읽을거리(저장해도 원본을 덮지 않는다) · 원래 파일의 구문.
                let title = if head && truncated {
                    tf(Msg::BigHeadTitle, &[&name])
                } else {
                    name.clone()
                };
                let keep_path = (!head).then_some(path.as_path());
                let t_fill = Instant::now();
                let filled = self
                    .editors
                    .fill_loaded(tab, keep_path, &title, &name, prep, eol);
                if self.frame_trace.is_some() {
                    // 계측(`NSQL_TRACE_FRAMES`): UI 스레드가 옮겨 넣느라 멎은 시간 — 다른 탭에서 타이핑 중이면 이만큼 끊긴다.
                    eprintln!(
                        "[load] fill {name}: {:.1} ms on the UI thread",
                        t_fill.elapsed().as_secs_f64() * 1000.0
                    );
                }
                let Some(i) = filled else {
                    self.mem_released();
                    return; // 자리 탭이 그사이 닫혔다 — 버린다.
                };
                self.editors.set_encoding(i, used);
                if head || mode == LoadMode::ReadOnly {
                    self.editors.set_read_only(i, true);
                }
                let mut restored = 0;
                if !head {
                    self.ext_track(tab);
                    self.push_recent(&path);
                    if mode == LoadMode::Open {
                        restored = self.undo_persist_load(i, &path);
                        self.vars_persist_load(tab, &path);
                        self.bookmarks.on_opened(&self.editors, i);
                        self.bm_refresh_tab(i);
                    }
                }
                if self.editors.active_id() == tab {
                    self.set_focus(Focus::Editor);
                }
                let level = self.editors.large_level(i);
                self.sess.status = if lossy {
                    tf(Msg::StFileDecodedLossy, &[&name])
                } else if level > 0 {
                    tf(Msg::StLargeMode, &[&name, &level.to_string()])
                } else if restored > 0 {
                    tf(Msg::StUndoRestored, &[&title, &restored.to_string()])
                } else {
                    tf(Msg::StFileOpened, &[&title])
                };
            }
            // 짝이 맞지 않는 조합은 만들지 않는다(실행 = 문자열 · 그 밖 = 준비본).
            (_, fileload::Body::Text(_)) => {}
        }
        // 읽느라 쓴 임시 버퍼를 놓았다 → 힙 정리 예약.
        self.mem_released();
        self.layout();
        self.redraw();
    }

    /// 변수 표 보존(D-136 · `varsfile`) — 실행이 그 탭의 변수를 바꿨을 때 · 파일이 있는 탭만.
    fn vars_persist_save(&mut self, tab: u64, vars: &[nsql_script::VarState]) {
        if !self.settings.flag("vars.persist") {
            return;
        }
        let Some(path) = self
            .editors
            .index_of_id(tab)
            .and_then(|i| self.editors.path_of(i))
        else {
            return;
        };
        let Some(dir) = varsfile::dir() else { return };
        if !self.vars_pruned {
            self.vars_pruned = true;
            varsfile::prune_in(&dir, self.settings.int("vars.persist_days").max(0) as u64);
        }
        varsfile::store_in(&dir, &path, vars);
    }

    /// 방금 연 파일의 변수 표를 되살린다(그 탭에 아직 표가 없을 때만).
    fn vars_persist_load(&mut self, tab: u64, path: &Path) {
        if !self.settings.flag("vars.persist") || self.tab_vars.contains_key(&tab) {
            return;
        }
        let Some(dir) = varsfile::dir() else { return };
        let vars = varsfile::load_in(&dir, path);
        if !vars.is_empty() {
            self.tab_vars.insert(tab, vars);
        }
    }

    /// 되돌리기 기록 파일 쓰기(docs/60 D-129 · 저장 직후 · 활성 탭) — 기록이 없으면 옛 파일을 지운다. 큰 파일 탭은 건너뛴다.
    fn undo_persist_save(&mut self, path: &Path) {
        if !self.settings.flag("editor.undo_persist") {
            return;
        }
        let Some(dir) = undofile::dir() else { return };
        if !self.undo_pruned {
            self.undo_pruned = true;
            undofile::prune_in(
                &dir,
                self.settings.int("editor.undo_persist_days").max(0) as u64,
            );
        }
        let i = self.editors.active();
        if self.editors.large_level(i) > 0 {
            undofile::store_in(&dir, path, None);
            return;
        }
        let cap = (self.settings.int("editor.undo_persist_mb").max(1) as usize) << 20;
        let bytes = self.editors.cur().export_history(cap);
        undofile::store_in(&dir, path, bytes.as_deref());
    }

    /// 방금 읽은 탭에 되돌리기 기록을 되살린다(본문이 기록의 것과 같을 때만) — 되살린 단계 수.
    fn undo_persist_load(&mut self, i: usize, path: &Path) -> usize {
        if !self.settings.flag("editor.undo_persist") || self.editors.large_level(i) > 0 {
            return 0;
        }
        let cap = (self.settings.int("editor.undo_persist_mb").max(1) as u64) << 20;
        let Some(bytes) = undofile::dir().and_then(|d| undofile::load_in(&d, path, cap)) else {
            return 0;
        };
        self.editors.import_history(i, &bytes)
    }

    /// 활성 탭 → 파일(UTF-8 · BOM 없음 · 원래 줄끝 유지).
    fn save_to(&mut self, path: &Path) {
        // 저장 직전 확인(docs/58 — 어떤 감지도 놓칠 수 있다 · 마지막 안전망): 디스크가 읽어 온 것과 다르면 2단 저장.
        if !self.ext_save_guard(path) {
            return;
        }
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
                backups::remove(path);
                self.undo_persist_save(path);
                self.ext_track_active();
                self.git.refresh(true);
                self.push_recent(path);
                // 새 파일이 생겼을 수 있다 — 프로젝트 탐색기의 펼친 폴더를 다시 열거.
                if self.project_panel.is_visible() {
                    self.project_panel.refresh();
                }
                let name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.sess.status = tf(Msg::StFileSaved, &[&name]);
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.sess.status = tf(
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
        if let (Some(o), Some(c)) = (owner.as_deref(), self.conn_win.window()) {
            winfocus::attach_child(o, c);
        }
    }

    /// 로그인 목록 더블클릭/Enter — 저장소에서 읽어 폼에 채우고 바로 접속.
    /// 행 테스트 버튼 — 폼을 건드리지 않고 저장소 스펙으로 접속만 해 본다. 결과는 행 버튼 표시로.
    /// ★ 접속 테스트 시작 — 요청당 스레드(순차 워커·`busy`와 무관 · 실패 서버 타임아웃이 다른 테스트를 막지 않는다 · 사용자 09-14).
    /// 같은 프로필이 이미 테스트 중이면 무시.
    fn start_test(&mut self, name: &str, spec: ConnectSpec) {
        if self.conn_win.test_mark(name) == Some(TestMark::Testing) {
            return;
        }
        self.sess.status = t(Msg::StTesting).into();
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
                    name: a_name,
                    spec,
                    reconnect_same,
                } => {
                    // 접속 창의 대상 세션(docs/52 §2): 공유 모드 = 기본 공유 세션(활성 탭이 전용이어도) · 개별 모드 = 활성 탭의 세션.
                    let Some(target) = self.login_place(&spec) else {
                        self.attempts_inflight = self.attempts_inflight.saturating_sub(1);
                        // 자리를 못 잡았다(세션 바쁨·상한) — 막을 걷는다(사유는 login_place가 상태줄에).
                        self.conn_win.veil_end(&a_name);
                        continue;
                    };
                    self.conn_win.veil_phase(&a_name, t(Msg::VeilConnecting));
                    let profile = a_name.clone();
                    self.with_sess(target, |a| {
                        a.sess.profile = profile;
                        a.sess.busy = true;
                        a.sess.attempt_inflight = true;
                        a.sess.user_disconnected = false;
                        a.sess.idle_closed = false;
                        a.sess.status = tf(Msg::StConnecting, &[&spec.redacted()]);
                        a.sess.last_spec = Some(spec.clone());
                        a.sess.spec = Some(spec.clone());
                        a.sess.touch();
                        a.sess.worker.send(worker::Cmd::ConnectSpec {
                            spec,
                            reconnect_same,
                        });
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
            Ok(None) => self.sess.status = tf(Msg::StTestFailed, &[name]),
            Err(e) => self.sess.status = tf(Msg::StTestFailed, &[&e.to_string()]),
        }
        self.conn_win.redraw();
    }

    fn login_profile(&mut self, name: &str) {
        // (바쁜 세션 판정은 시도를 보낼 때 `login_place`가 한다 — 공유 연결이 여럿이라 대상은 스펙을 봐야 정해진다.)
        if self.conn_win.is_testing(name) {
            self.sess.status = t(Msg::StTesting).into();
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
    /// 목록 우클릭 메뉴의 접속 유형 — 저장소의 그 프로필만 고친다(비밀번호 봉투 포함 그대로 다시 쓴다). 지금 붙어 있는 세션이
    /// 그 프로필이면 세션의 표식도 바로 바꾼다(다시 접속할 필요 없음).
    fn set_profile_env(&mut self, name: &str, env: Option<nsql_script::ConnEnv>) {
        let r = Vault::open_default().and_then(|v| {
            let Some(mut spec) = v.get(name)? else {
                return Ok(false);
            };
            spec.env = env;
            v.save(name, &spec)?;
            Ok(true)
        });
        match r {
            Ok(true) => {
                let label = match env {
                    Some(nsql_script::ConnEnv::Prod) => t(Msg::MnEnvProd),
                    Some(nsql_script::ConnEnv::Test) => t(Msg::MnEnvTest),
                    Some(nsql_script::ConnEnv::Dev) => t(Msg::MnEnvDev),
                    None => t(Msg::MnEnvNone),
                };
                self.sess.status = tf(Msg::StEnvSet, &[name, label]);
                self.conn_win.refresh_profiles(Some(name));
                let ids: Vec<u64> = self
                    .all_sess()
                    .filter(|s| s.profile == name)
                    .map(|s| s.id)
                    .collect();
                for id in ids {
                    self.with_sess(id, |a| {
                        if let Some(sp) = a.sess.spec.as_mut() {
                            sp.env = env;
                        }
                    });
                }
            }
            Ok(false) => {}
            Err(e) => self.sess.status = e.to_string(),
        }
        self.conn_win.redraw();
        self.redraw();
    }

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
                self.sess.status = tf(Msg::WkProfileSaved, &[&new, ""]);
                self.conn_win.refresh_profiles(Some(&new));
            }
            Ok(None) => self.sess.status = t(Msg::ErrProfileName).into(),
            Err(e) => self.sess.status = e.to_string(),
        }
        self.conn_win.redraw();
    }

    fn delete_profile(&mut self, name: &str) {
        match Vault::open_default().and_then(|v| v.remove(name)) {
            Ok(_) => {
                self.sess.status = tf(Msg::StProfileDeleted, &[name]);
                self.panel_results.remove(name.trim());
            }
            Err(e) => self.sess.status = e.to_string(),
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
    /// 트랜잭션 로그 창 — **토글**(사용자 09-19: 열려 있으면 닫는다 · 로그 창·세션 창과 같은 규칙).
    fn open_txlog_window(&mut self, el: &ActiveEventLoop) {
        if self.txlog_win.is_open() {
            self.txlog_win.close();
            self.persist_window_sizes(false);
            return;
        }
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

    /// 변수 창 열기/닫기(View ▸ Variables).
    fn open_vars_window(&mut self, el: &ActiveEventLoop) {
        if self.vars_win.is_open() {
            self.vars_win.close();
            return;
        }
        let near = self.window.as_ref().and_then(|w| {
            w.outer_position()
                .ok()
                .map(|p| (p.x, p.y, w.outer_size().width))
        });
        let owner = self.window.clone();
        self.vars_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
        );
        self.vars_win_context();
    }

    /// 변수 창의 제목 = 지금 편집기 탭.
    fn vars_win_context(&mut self) {
        if !self.vars_win.is_open() {
            return;
        }
        let id = self.editors.active_id();
        let title = self
            .editors
            .tab_list()
            .into_iter()
            .find(|(t, _, _)| *t == id)
            .map(|(_, title, _)| title)
            .unwrap_or_default();
        self.vars_win.set_context(&title);
    }

    /// 변수 창의 줄 — 지금 편집기 탭의 표(탭 층) + 이 세션의 공유 층. 비밀 값은 가린다 · 긴 값은 앞부분만.
    /// 다음 실행에 넘길 치환 변수(활성 탭 · 이름 · 원문).
    /// 내장 변수 스냅숏(`${workspaceFolder}` · `${file}` · `${config:키}` … · [nsql_script::intrinsic] · 사용자 09-23) — 실행마다
    /// 그 순간의 프로젝트·활성 탭·접속·설정으로 만든다(수십 항목 + 설정 키 · 복사 0 = `Arc`). 설정 `vars.intrinsic` 끔 = 빈 표.
    fn run_intrinsic(&self) -> std::sync::Arc<std::collections::BTreeMap<String, String>> {
        if !self.settings.flag("vars.intrinsic") {
            return std::sync::Arc::default();
        }
        std::sync::Arc::new(nsql_script::intrinsic::build(&self.intrinsic_context()))
    }

    fn intrinsic_context(&self) -> nsql_script::intrinsic::Context {
        let tb = self.editors.cur();
        let caret = tb.caret();
        let line = tb.buf().line_of(caret);
        let col = caret.saturating_sub(tb.buf().line_start(line));
        let leaf = |p: &Path| {
            p.file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        nsql_script::intrinsic::Context {
            project_file: self.project.path.clone(),
            workspace_dir: self.arg_folder.clone(),
            folders: self
                .project
                .folders
                .iter()
                .map(|f| (leaf(f), f.clone()))
                .collect(),
            file: self.editors.active_path(),
            line: Some(line + 1),
            column: Some(col + 1),
            user_home: std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(PathBuf::from),
            app_home: nsql_settings::config_dir(),
            exec: std::env::current_exe().ok(),
            cwd: std::env::current_dir().ok(),
            profile: Some(self.sess.profile.clone()).filter(|p| !p.is_empty()),
            dialect: self
                .sess
                .spec
                .as_ref()
                .and_then(|s| s.dialect)
                .map(|d| d.to_string()),
            config: nsql_settings::REGISTRY
                .iter()
                .filter_map(|e| {
                    self.settings
                        .get(e.key)
                        .map(|v| (e.key.to_string(), v.to_string()))
                })
                .collect(),
        }
    }

    fn run_defines(&self) -> Option<Vec<(String, String)>> {
        self.tab_defines.get(&self.editors.active_id()).map(|v| {
            v.iter()
                .map(|(n, raw, _)| (n.clone(), raw.clone()))
                .collect()
        })
    }

    fn vars_rows(&self) -> Vec<vars_win::VarRow> {
        const MAX: usize = 200;
        let tab = self.editors.active_id();
        let changed = (self.vars_changed.0 == tab).then_some(&self.vars_changed.1);
        let local = self.tab_vars.get(&tab).map(Vec::as_slice).unwrap_or(&[]);
        local
            .iter()
            .map(|v| (v, nsql_script::Layer::Local))
            .chain(
                self.sess
                    .shared_vars
                    .iter()
                    .map(|v| (v, nsql_script::Layer::Shared)),
            )
            .chain(
                self.global_vars
                    .iter()
                    .map(|v| (v, nsql_script::Layer::Global)),
            )
            .map(|(v, layer)| {
                let full = match &v.value {
                    nsql_core::Value::Null => String::new(),
                    other => other.display(),
                };
                let shown = if v.secret {
                    "******".to_string()
                } else if matches!(v.value, nsql_core::Value::Null) {
                    "NULL".to_string()
                } else if full.chars().count() > MAX {
                    format!("{}…", full.chars().take(MAX).collect::<String>())
                } else {
                    full.clone()
                };
                vars_win::VarRow {
                    name: v.name.clone(),
                    ty: var_type_text(&v.ty),
                    value: shown,
                    edit: if v.secret { String::new() } else { full },
                    layer,
                    changed: changed.is_some_and(|c| c.contains(&v.name.to_ascii_uppercase())),
                }
            })
            // 치환 변수(`&이름` · DEFINE) — 표시값 = 사용 시 모드면 `원문 → 현재 값`(63 §9) · 편집 = 원문.
            .chain(
                self.tab_defines
                    .get(&tab)
                    .into_iter()
                    .flatten()
                    .map(|(n, raw, shown)| vars_win::VarRow {
                        name: format!("&{n}"),
                        ty: "DEFINE".into(),
                        value: shown.clone(),
                        edit: raw.clone(),
                        layer: nsql_script::Layer::Local,
                        changed: false,
                    }),
            )
            .collect()
    }

    /// 변수 창의 동작을 표에 반영한다 — 탭 층은 App이 주인(바로 고친다 · 보존 파일도) · 공유 층은 워커에 통째로 알린다.
    fn vars_apply(&mut self, action: vars_win::VarsWinAction) {
        use vars_win::VarsWinAction as A;
        let tab = self.editors.active_id();
        let is = |v: &nsql_script::VarState, n: &str| v.name.eq_ignore_ascii_case(n);
        let mut shared_dirty = false;
        let mut global_dirty = false;
        match action {
            A::None | A::Paint => return,
            A::Script => {
                self.menu_action("vars.script");
                return;
            }
            A::Set(name, text) if name.starts_with('&') => {
                // 치환 변수 편집 = 원문을 바꾼다(다음 실행 때 엔진에 전달 · 표시값도 원문으로).
                let key = name[1..].to_ascii_uppercase();
                let list = self.tab_defines.entry(tab).or_default();
                match list.iter_mut().find(|(n, _, _)| *n == key) {
                    Some(slot) => {
                        slot.1 = text.clone();
                        slot.2 = text;
                    }
                    None => list.push((key.clone(), text.clone(), text)),
                }
                self.vars_changed = (tab, std::iter::once(name.to_ascii_uppercase()).collect());
            }
            A::SetNull(name) if name.starts_with('&') => {
                let key = name[1..].to_ascii_uppercase();
                if let Some(list) = self.tab_defines.get_mut(&tab) {
                    list.retain(|(n, _, _)| *n != key);
                }
            }
            A::Set(name, text) => {
                let value = nsql_run::input_value(&text);
                let set = |v: &mut nsql_script::VarState| {
                    if v.ty == nsql_core::VarType::Auto || !v.declared {
                        v.ty = nsql_core::VarType::infer(&value);
                    }
                    v.value = value.clone();
                };
                if let Some(v) = self.sess.shared_vars.iter_mut().find(|v| is(v, &name)) {
                    set(v);
                    shared_dirty = true;
                } else if let Some(v) = self.global_vars.iter_mut().find(|v| is(v, &name)) {
                    set(v);
                    global_dirty = true;
                } else {
                    let list = self.tab_vars.entry(tab).or_default();
                    match list.iter_mut().find(|v| is(v, &name)) {
                        Some(v) => set(v),
                        None => list.push(nsql_script::VarState {
                            secret: nsql_script::looks_secret(&name),
                            name: name.clone(),
                            ty: nsql_core::VarType::infer(&value),
                            value: value.clone(),
                            declared: false,
                            layer: nsql_script::Layer::Local,
                        }),
                    }
                }
                self.vars_changed = (tab, std::iter::once(name.to_ascii_uppercase()).collect());
            }
            A::SetNull(name) => {
                if let Some(v) = self.sess.shared_vars.iter_mut().find(|v| is(v, &name)) {
                    v.value = nsql_core::Value::Null;
                    shared_dirty = true;
                } else if let Some(v) = self.global_vars.iter_mut().find(|v| is(v, &name)) {
                    v.value = nsql_core::Value::Null;
                    global_dirty = true;
                } else if let Some(v) = self
                    .tab_vars
                    .get_mut(&tab)
                    .and_then(|l| l.iter_mut().find(|v| is(v, &name)))
                {
                    v.value = nsql_core::Value::Null;
                }
            }
            A::Delete(name) => {
                let before = self.sess.shared_vars.len();
                self.sess.shared_vars.retain(|v| !is(v, &name));
                shared_dirty = self.sess.shared_vars.len() != before;
                let gb = self.global_vars.len();
                self.global_vars.retain(|v| !is(v, &name));
                global_dirty = self.global_vars.len() != gb;
                if let Some(l) = self.tab_vars.get_mut(&tab) {
                    l.retain(|v| !is(v, &name));
                }
            }
            A::Layer(name, to) => {
                // 세 층(탭 · 공유 · 글로벌) 사이 이동 — 단일 원천에서 빼서 목적 층에 넣는다(docs/63 §11).
                let mut taken: Option<nsql_script::VarState> = None;
                if let Some(l) = self.tab_vars.get_mut(&tab) {
                    if let Some(i) = l.iter().position(|v| is(v, &name)) {
                        taken = Some(l.remove(i));
                    }
                }
                if taken.is_none() {
                    if let Some(i) = self.sess.shared_vars.iter().position(|v| is(v, &name)) {
                        taken = Some(self.sess.shared_vars.remove(i));
                        shared_dirty = true;
                    }
                }
                if taken.is_none() {
                    if let Some(i) = self.global_vars.iter().position(|v| is(v, &name)) {
                        taken = Some(self.global_vars.remove(i));
                        global_dirty = true;
                    }
                }
                if let Some(mut v) = taken {
                    v.layer = to;
                    match to {
                        nsql_script::Layer::Shared => {
                            self.sess.shared_vars.push(v);
                            shared_dirty = true;
                        }
                        nsql_script::Layer::Global => {
                            self.global_vars.push(v);
                            global_dirty = true;
                        }
                        _ => self.tab_vars.entry(tab).or_default().push(v),
                    }
                }
            }
        }
        if shared_dirty {
            self.sess
                .worker
                .send(worker::Cmd::SharedVars(self.sess.shared_vars.clone()));
        }
        if global_dirty {
            self.global_vars_changed();
        }
        let local = self.tab_vars.get(&tab).cloned().unwrap_or_default();
        self.vars_persist_save(tab, &local);
        self.vars_win.redraw();
    }

    /// 글로벌 층이 바뀌었다(변수 창 · 스크립트 `VAR x GLOBAL`) — 파일에 남기고(`vars.global_persist`) 모든 세션에 전파.
    fn global_vars_changed(&mut self) {
        if self.settings.flag("vars.global_persist") {
            if let Some(dir) = varsfile::dir() {
                varsfile::store_global(&dir, &self.global_vars);
            }
        }
        let v = self.global_vars.clone();
        for s in self.all_sess() {
            s.worker.send(worker::Cmd::GlobalVars(v.clone()));
        }
    }

    /// 메모리 맵 창 열기(docs/80 · 모델리스 · 최상위는 설정 `mem.always_on_top`) — 첫 표본은 다음 유휴 틱에.
    fn open_mem_window(&mut self, el: &ActiveEventLoop) {
        let near = self.window.as_ref().and_then(|w| {
            w.outer_position()
                .ok()
                .map(|p| (p.x, p.y, w.outer_size().width))
        });
        let owner = self.window.clone();
        self.mem_win
            .set_on_top(self.settings.flag("mem.always_on_top"));
        self.mem_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
        );
        self.mem_next = Instant::now();
    }

    /// 상태줄 세그먼트·메뉴에서 토글(열기는 깃발 → `about_to_wait`가 이벤트 루프로 연다).
    fn toggle_mem_window(&mut self) {
        if self.mem_win.is_open() {
            self.mem_win.close();
            self.persist_window_sizes(false);
        } else {
            self.open_mem = true;
        }
    }

    fn mem_every(&self) -> Duration {
        Duration::from_millis(self.settings.int("mem.refresh_ms").clamp(250, 10_000) as u64)
    }

    /// 전체 표본(창이 열려 있을 때만 불린다) — 부품 보고(`MemSource`) + 결과 탭 + 창 표면 + 로그.
    fn mem_sample(&mut self) -> memstat::Sample {
        use memstat::Cat;
        let grids: Vec<(u64, u64)> = self.all_grids().map(|g| g.mem_parts()).collect();
        let main_surface = self.window.as_ref().map_or(0, |w| {
            let s = w.inner_size();
            u64::from(s.width) * u64::from(s.height) * 4
        });
        let surfaces = main_surface + self.mem_win.surface_bytes();
        let logs = self.log_win.approx_bytes() + self.txlog.len() as u64 * 256;
        memstat::sample(&[&self.editors, &self.explorer, &self.intel], |acc| {
            for (d, tx) in grids {
                acc.add(Cat::ResultData, d);
                acc.add(Cat::ResultText, tx);
            }
            acc.add(Cat::Surfaces, surfaces);
            acc.add(Cat::Logs, logs);
        })
    }

    /// 상태줄 총량 글(`mem.statusbar` 꺼짐 = None) — 마지막 조회가 `mem.status_refresh_ms`보다 오래됐을 때만 OS 한 번(그릴 때만 · 깨우지 않음).
    fn mem_status_text(&mut self) -> Option<String> {
        if !self.settings.flag("mem.statusbar") {
            return None;
        }
        let now = Instant::now();
        let every = Duration::from_millis(
            self.settings
                .int("mem.status_refresh_ms")
                .clamp(1000, 60_000) as u64,
        );
        if self
            .mem_status
            .1
            .is_none_or(|t| now.duration_since(t) >= every)
        {
            self.mem_status = (memstat::sys_total(), Some(now));
        }
        Some(memstat::fmt(self.mem_status.0))
    }

    fn open_sessions_window(&mut self, el: &ActiveEventLoop) {
        let near = self.window.as_ref().and_then(|w| {
            w.outer_position()
                .ok()
                .map(|p| (p.x, p.y, w.outer_size().width))
        });
        let owner = self.window.clone();
        self.sessions_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            near,
            owner.as_deref(),
        );
    }

    /// 세션 창의 표 줄 — 세션 상태의 단일 원천(`Sess`)에서 그릴 때마다 만든다(복사는 줄 수만큼 · 세션은 몇 개뿐).
    fn session_rows(&self) -> Vec<SessRow> {
        let titles: HashMap<u64, String> = self
            .editors
            .tab_list()
            .into_iter()
            .map(|(id, title, _)| (id, title))
            .collect();
        let now = Instant::now();
        let mut rows: Vec<SessRow> = self
            .all_sess()
            .filter(|s| !s.closing && (s.connected || s.busy || s.idle_closed || s.is_private()))
            .map(|s| SessRow {
                id: s.id,
                server: if s.desc.is_empty() {
                    t(Msg::MnSessNoneConnected).to_string()
                } else {
                    s.desc.clone()
                },
                shared: !s.is_private(),
                active: s.id == self.default_shared && !s.is_private(),
                tab: s
                    .owner
                    .map(|tab| (tab, titles.get(&tab).cloned().unwrap_or_default())),
                connected: s.connected,
                broken: s.broken,
                busy: s.blocked(),
                idle_secs: now.saturating_duration_since(s.last_used).as_secs(),
                pending: s.tx_pending.len(),
            })
            .collect();
        // 접속 순(세션 id 순) · 같은 서버끼리는 창이 모은다.
        rows.sort_by_key(|r| r.id);
        rows
    }

    /// 열린 수동 트랜잭션을 결과와 함께 닫고 대기 목록을 비운다(커밋·롤백·암묵·전환·끊김 — docs/44 §3).
    fn tx_close(&mut self, outcome: TxOutcome) {
        // (세션이 끊기는 경로도 여기를 지난다 — Lost) 큰 결과·메타를 놓았을 수 있다.
        if matches!(outcome, TxOutcome::Lost) {
            self.mem_released();
        }
        // 커밋을 기다리던 DDL(docs/57 D-107): 커밋 계열 = 이제 다른 세션(메타 세션)도 본다 → 반영 · 롤백·소실 = 버림 ·
        // 읽기 트랜잭션 종료는 변경이 없던 것이라 대기열과 무관.
        match &outcome {
            TxOutcome::Committed
            | TxOutcome::ImplicitCommit(_)
            | TxOutcome::Switched
            | TxOutcome::AutoCommitted(_) => {
                let ddl = std::mem::take(&mut self.sess.ddl_wait);
                self.meta_flush(ddl);
            }
            TxOutcome::RolledBack | TxOutcome::Lost | TxOutcome::AutoRolledBack(_) => {
                self.sess.ddl_wait.clear();
            }
            _ => {}
        }
        let stamp = nsql_log::now_local().stamp();
        self.txlog
            .select_session(self.sess.id)
            .close_tx(outcome, stamp);
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
        let (on, hide, alpha) = (
            self.settings.flag("run.toast"),
            self.settings.int("run.toast_hide_secs"),
            self.settings.int("ui.toast_alpha"),
        );
        let (prog, fade_to, spent) = (
            self.settings.flag("ui.toast_progress"),
            self.settings.int("ui.toast_fade_to"),
            self.settings.int("ui.toast_bar_spent"),
        );
        let tick = self.settings.int("run.toast_tick_ms");
        self.run_toast.configure(on, hide, alpha);
        self.run_toast
            .configure_copy(self.settings.int("ui.copy_feedback_ms"));
        self.run_toast.configure_progress(prog, fade_to, spent);
        self.run_toast.configure_tick(tick);
        self.run_toast.configure_stack(
            self.settings.flag("run.toast_follow"),
            self.settings.int("run.toast_max"),
        );
    }

    /// 실행 시작 → 카드(문장 · 시작 시각 · 문장 수).
    fn run_toast_start(&mut self, src: &str) {
        // 이 세션의 문장 실행 = 활동(docs/56 L2) — 유휴 시계·경고·카운트다운을 되돌린다.
        self.tx_guard_reset();
        self.sess.run_cancel_requested = false;
        self.sync_run_stmt_button();
        self.editors.set_running(self.sess.run_editor, true);
        self.editors.set_error_line(self.sess.run_editor, None);
        let n = split_items(src, self.sess.dialect).len().max(1);
        self.sess.run_card = self.run_toast.start(src, nsql_log::now_local().stamp(), n);
    }

    /// 중지(카드 ■ = 툴바 ■ · T-108): 실행 중 문장은 드라이버 취소 핸들로 서버에 취소 · 전체 조회는 다음 배치 경계에서.
    /// 지금 세션의 워커를 버리고 새 워커로(즉시 해제 규약 09-16 — 죽은 소켓에 갇힌 호출을 기다리지 않는다).
    /// 세션은 **끊김(Broken)으로 남기고 스펙을 지킨다** → 다음 동작 때 판정 뒤 조용히 재접속(`wake_if_idle`).
    fn abandon_worker(&mut self) {
        let (w, ev) = self.spawn_worker();
        self.log_disconnect(sessions::DiscPath::Stop);
        self.sess.worker.send(worker::Cmd::Disconnect);
        self.sess.worker = w;
        self.sess.events = ev;
        self.sess.busy = false;
        self.sess.aux = 0;
        self.sess.skip_done = 0;
        self.sess.connected = false;
        self.sess.idle_closed = true;
        self.sess.run_cancel_requested = false;
        self.editors.set_running(self.sess.run_editor, false);
        self.run_toast
            .finish(self.sess.run_card, runtoast::Phase::Stopped { rows: 0 });
        self.grid.fetch_failed();
        self.tx_close(TxOutcome::Lost);
    }

    fn stop_run(&mut self) {
        if !self.sess.busy && !self.grid.fetch_all_active() {
            return;
        }
        // 입력 창이 답을 기다리는 중이면 ■ = 입력 취소(워커는 입력을 기다리며 멈춰 있다 — DB로 간 것이 없다).
        if self.input_win.is_open() && self.input_win.sess == self.sess.id {
            if self.input_win.is_password() {
                self.password_reply(worker::PwReply::Cancel);
            } else {
                self.input_reply(worker::InputReply::Cancel);
            }
            return;
        }
        // ★ 끊긴 서버(docs/53 §3): 취소(OCIBreak)도 같은 소켓으로 가 함께 막힌다 → 워커를 버리는 것이 유일한 즉시 중지.
        if self.sess.broken {
            self.abandon_worker();
            self.sess.status = tf(Msg::StSessBrokenStopped, &[&self.sess.desc]);
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            self.sync_sess_ui();
            self.redraw();
            return;
        }
        self.sess.run_cancel_requested = self.sess.busy;
        if self.sess.dialect == Dialect::Mssql
            && self.settings.get("mssql.cancel") != Some("socket")
            && self.settings.get("mssql.encrypt") != Some("login")
        {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                t(Msg::StMssqlAttentionFallback),
            ));
        }
        let (sent, drops) = self.sess.worker.cancel_run();
        self.sess.run_cancel_drops = drops;
        self.sess.status = t(if sent && drops {
            Msg::StRunCancellingDrop
        } else if sent {
            Msg::StRunCancelling
        } else {
            Msg::StFetchCancelling
        })
        .into();
        self.log_win
            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
        self.redraw();
    }

    /// `ui.theme` + OS 판정으로 팔레트를 다시 고르고 전체를 다시 그린다.
    fn apply_theme(&mut self) {
        let wt = self.window.as_ref().and_then(|w| w.theme());
        self.theme = theme::resolve(self.settings.theme_mode(), wt);
        self.apply_tab_line_colors();
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
        self.sess.status = tf(Msg::StThemeChanged, &[t(next.label())]);
    }

    /// Ctrl/⌘+⇧L — 언어 전환 · 저장 · 라벨 다시 만들기.
    fn toggle_lang(&mut self) {
        let next = current_lang().next();
        let _ = self.settings.set("ui.lang", next.code());
        self.persist_settings();
        nsql_i18n::set_lang(next);
        self.relabel();
        self.sess.status = tf(Msg::StLangChanged, &[next.endonym()]);
        self.redraw();
    }

    // ── 데모 프로필·샘플 데이터(사용자 09-17 · docs/21 §5)

    /// 메뉴바를 현재 상태(최근 파일 · 탭 · 데모 준비 여부)로 다시 만든다.
    fn rebuild_menus(&mut self) {
        self.menubar
            .set_max_label_width(self.settings.int("ui.menu_max_width") as i32);
        let tabs = self.editors.tab_list();
        self.menubar.set_menus(App::build_menus_with(
            &self.recent_files(),
            &tabs,
            self.demo_ready,
            self.gate_shown.unwrap_or(false),
            self.project_menu_entries(),
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
            self.sess.status = tf(Msg::StDemoFailed, &["NSQL_HOME"]);
            return;
        };
        self.sess.status = t(Msg::StDemoCreating).into();
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
                self.sess.status = tf(Msg::StDemoCreated, &[&path]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            }
            Err(e) => {
                self.sess.status = tf(Msg::StDemoFailed, &[&e]);
                self.toasts
                    .push(toast::ToastKind::Error, t(Msg::MnDemoCreate), e);
            }
        }
        self.redraw();
    }

    fn persist_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.sess.status = tf(Msg::CfgSaveFailed, &[&e.to_string()]);
        }
    }

    /// 언어가 바뀌면 컨트롤 문자열을 다시 만든다. TextBox는 placeholder 교체 API가 없어 본문을 보존해 재생성.
    fn relabel(&mut self) {
        let tabs = self.editors.tab_list();
        self.menubar.set_menus(App::build_menus_with(
            &self.recent_files(),
            &tabs,
            self.demo_ready,
            self.gate_shown.unwrap_or(false),
            self.project_menu_entries(),
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
        if !self.gate_open() {
            return;
        }
        // 다중 커서/선택이면 "캐럿 문장"이 하나가 아니다 → 문장 실행은 막고 전체 실행(F5)만(사용자 09-17).
        if !all && self.editors.cur().has_multi() {
            self.sess.status = t(Msg::StMultiCaretRun).into();
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
                // ★ 원문 조각(`span`)을 넘긴다 — `it.text`는 정규화된 실행 텍스트라(홀로 선 `EXEC` 줄 + 본문 = `EXEC 본문`)
                //   다시 스크립트로 분할하면 다른 항목이 된다(mac 09-21: `EXEC` 블록 + `SELECT … INTO :V`가 현재 문 실행에서만
                //   Msg 102 · 선택 실행과 같은 길 = 원문 그대로).
                nsql_script::statement_at_in(&full, byte_pos, Some(self.sess.dialect)).map(|it| {
                    line_base = it.line.saturating_sub(1);
                    full[it.span].to_string()
                })
            }
        };
        let src = text.unwrap_or_else(|| self.ed_mut().text());
        self.run_text(src, line_base, all);
    }

    /// 본문 실행의 공통 경로 — 편집기 실행(`run_sql`)과 **디스크에서 바로 실행**(docs/59 §4 3단계 · 편집기에 싣지 않는다)이 같이 쓴다.
    fn run_text(&mut self, mut src: String, line_base: usize, all: bool) {
        // ★ 세션 배치(docs/52 §4): `CONNECT`면 이 탭의 전용 세션으로 · 전용 탭의 `DISCONNECT`면 해제하고 끝.
        if !self.place_run(&mut src) {
            return;
        }
        self.wake_if_idle();
        self.sess.touch();
        self.sess.run_line_base = line_base;
        self.grid.set_source_sql(&src);
        self.sess.run_tab = self.grid_tab;
        self.sess.run_set_stmt = None;
        self.sess.run_children = 0;
        self.sess.run_tracking = true;
        self.sess.run_editor = self.editors.active_id();
        if src.trim().is_empty() {
            self.sess.status = t(Msg::ErrNoSql).into();
            return;
        }
        // 운영 접속 + 변경 문장 = 2단 실행(같은 본문을 3초 안에 다시 실행하면 진행 · 앱의 2단 확인 관례 · docs/56 §4).
        let prod =
            self.sess.spec.as_ref().and_then(|sp| sp.env) == Some(nsql_script::ConnEnv::Prod);
        if sessions::prod_confirm_needed(
            prod,
            self.settings.flag("run.prod_confirm"),
            &split_items(&src, self.sess.dialect),
        ) {
            let key = nexa_fs::watch::content_hash(src.as_bytes());
            let armed = self
                .sess
                .prod_armed
                .is_some_and(|(k, at)| k == key && at.elapsed() <= Duration::from_secs(3));
            if !armed {
                self.sess.prod_armed = Some((key, Instant::now()));
                self.sess.status = t(Msg::StProdConfirm).into();
                self.toasts.push(
                    toast::ToastKind::Error,
                    t(Msg::StProdConfirmTitle),
                    t(Msg::StProdConfirm).to_string(),
                );
                self.redraw();
                return;
            }
            self.sess.prod_armed = None;
        }
        self.sess.busy = true;
        self.sess.status = t(Msg::StRunning).into();
        self.run_toast_start(&src);
        // 신호등이 초록이 아닌 서버(빨강·파랑·확인 중·모름)에는 실행 전 빠른 포트 판정을 건다(사용자 09-14).
        let pol = *self.conn_win.policy();
        let light = self.conn_win.status_of(self.conn_win.active_name());
        let preflight =
            (pol.enabled && light != Some(probe::ProbeStatus::Up)).then_some(pol.timeout);
        self.sess.last_run_items = split_items(&src, self.sess.dialect);
        // 세션 상태를 바꾸는 문장이 나가면 이 세션은 유휴로 닫지 않는다(닫으면 그 설정·임시 데이터를 잃는다 · docs/52 §6-4).
        if self
            .sess
            .last_run_items
            .iter()
            .any(|s| sessions::alters_session_state(s))
        {
            self.sess.stateful = true;
        }
        let max_rows = self.grid.page_rows();
        self.sess.single_run = !all;
        self.sess.worker.send(worker::Cmd::Run {
            src,
            preflight,
            max_rows,
            vars: self.run_vars(),
            defines: self.run_defines(),
            intrinsic: Some(self.run_intrinsic()),
        });
        self.live_start();
        self.redraw();
    }

    /// 라이브 로그 설정(끔이면 None) — 매번 읽는다(설정 창에서 바꾸면 다음 실행부터).
    fn live_req(&self) -> Option<LiveReq> {
        // 라이브 모니터는 탐색기 메타 세션(= 접속 창으로 붙은 서버)으로 본다 → 그 세션의 실행만 대상(docs/52 §9).
        if self.sess.dialect != Dialect::Oracle
            || !self.explorer.has_server(self.sess.spec.as_ref())
        {
            return None;
        }
        let source = self
            .settings
            .effective("oracle.live.source")
            .unwrap_or("off");
        if source == "off" {
            return None;
        }
        let sid = self.sess.live_sid.clone()?;
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
            since: self.sess.live_since.clone(),
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
        self.sess.live_since = None;
        self.sess.live_last.clear();
        self.sess.live_final = true;
        if let Some(req) = self.live_req() {
            self.explorer.live_poll(self.sess.spec.as_ref(), req);
            self.sess.live_next = Instant::now() + self.live_interval();
        }
    }

    /// 주기 폴링(실행 중) · 실행이 끝나면 마지막 1회.
    fn live_tick(&mut self, now: Instant) -> Option<Instant> {
        if self.sess.busy {
            if now >= self.sess.live_next {
                if let Some(req) = self.live_req() {
                    self.explorer.live_poll(self.sess.spec.as_ref(), req);
                }
                self.sess.live_next = now + self.live_interval();
            }
            Some(self.sess.live_next)
        } else if self.sess.live_final {
            self.sess.live_final = false;
            if let Some(req) = self.live_req() {
                self.explorer.live_poll(self.sess.spec.as_ref(), req);
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
                        self.sess.live_since = last_ts;
                    }
                    for line in lines {
                        if line == self.sess.live_last {
                            continue;
                        }
                        self.sess.live_last = line.clone();
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
        if !self.gate_open() {
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
                // 원문 조각(`span`) — 위 `run_sql`과 같은 이유(정규화된 `text`는 재분할하면 달라진다).
                nsql_script::statement_at_in(&full, byte_pos, Some(self.sess.dialect))
                    .map(|it| full[it.span].to_string())
            });
        let Some(stmt) = text.filter(|s| !s.trim().is_empty()) else {
            self.sess.status = t(Msg::ErrNoSql).into();
            return;
        };
        let src = nsql_script::explain_script(self.sess.dialect, &stmt);
        self.wake_if_idle();
        self.sess.touch();
        self.sess.run_tab = self.grid_tab;
        self.sess.run_set_stmt = None;
        self.sess.run_children = 0;
        self.sess.run_tracking = true;
        self.sess.run_editor = self.editors.active_id();
        self.sess.busy = true;
        self.sess.status = t(Msg::StRunning).into();
        self.run_toast_start(&src);
        self.sess.last_run_items = split_items(&src, self.sess.dialect);
        self.sess.worker.send(worker::Cmd::Run {
            src,
            preflight: None,
            max_rows: self.grid.page_rows(),
            vars: self.run_vars(),
            defines: self.run_defines(),
            intrinsic: Some(self.run_intrinsic()),
        });
        self.live_start();
        self.redraw();
    }

    /// 탐색기가 부탁한 동작(우클릭 메뉴 · 더블클릭 — SQL 열기 · 이름 복사 · 서버 연결/해제 · 새 탭)을 **바로** 처리한다.
    /// ★ 종전에는 워커 응답을 걷는 `drain_events` 안에서만 걷어서, 메뉴로 고른 연결 해제·이름 복사가 **다음 워커 응답이 올 때까지**
    ///   실행되지 않았다(응답이 없으면 끝내 안 됨 · 사용자 09-21). 입력을 탐색기에 준 직후에도 부른다.
    fn explorer_actions(&mut self) -> bool {
        let mut changed = false;
        for a in self.explorer.take_actions() {
            changed = true;
            match a {
                ExplorerAction::OpenSql { title, text } => {
                    self.editors.new_tab(Some(title));
                    self.editors.cur_mut().set_text(&text);
                    self.set_focus(Focus::Editor);
                }
                ExplorerAction::Status(s) => self.sess.status = s,
                // 상태줄은 놓치기 쉽다 → 경고 토스트도(예: 연결이 해제된 서버에서 새로 고침).
                ExplorerAction::Notice(s) => {
                    self.toasts.push(
                        toast::ToastKind::Warn,
                        t(Msg::ExpNotConnected).to_string(),
                        s.clone(),
                    );
                    self.sess.status = s;
                }
                // (서버 제거는 `ExplorerSet::take_actions`가 안에서 처리한다.)
                ExplorerAction::RemoveServer => {}
                ExplorerAction::DisconnectServer(spec) => {
                    if let Some(spec) = spec {
                        self.disconnect_server(&spec);
                    }
                }
                ExplorerAction::ConnectServer(spec) => {
                    if let Some(spec) = spec {
                        self.connect_server(spec);
                    }
                }
                ExplorerAction::NewTabHere(spec) => {
                    if let Some(spec) = spec {
                        self.new_tab_on(&spec);
                    }
                }
                ExplorerAction::Copy(s) => {
                    if !clipboard::write_text(&s) {
                        self.sess.status = t(Msg::ErrClipboard).into();
                    }
                }
                // Generate SQL 결과(83 §3) — 창은 `el`이 있는 자리에서 연다(이미 열려 있으면 바로 본문 교체).
                ExplorerAction::Preview { spec, r, server } => {
                    if let Err(e) = &r {
                        self.sess.status = tf(Msg::StGenFailed, &[e]);
                    } else {
                        self.sess.status = spec.title();
                    }
                    if self.sqlprev_win.is_open() {
                        self.sqlprev_win.spec = Some(spec);
                        self.sqlprev_win.server = server;
                        self.sqlprev_win.set_result(r);
                    } else {
                        self.sqlprev_pending = Some((spec, r, server));
                    }
                    changed = true;
                }
            }
        }
        changed
    }

    fn drain_events(&mut self) {
        let mut changed = self.drain_conn();
        self.conn_win.drain_probes();
        while let Ok(ev) = self.sess.events.try_recv() {
            changed = true;
            // 다시 접속하려다 기존 접속을 잃었다(`ConnectionClosed`) — 해제는 같지만 **세션은 거두지 않는다**(아래).
            let lost_by_connect = matches!(ev, RunEvent::ConnectionClosed);
            if matches!(ev, RunEvent::Disconnected | RunEvent::ConnectionClosed) {
                // 호스트가 시작한 해제는 이미 경로와 함께 남겼다(disc_path) · 스크립트/서버 쪽 해제만 여기서 남긴다.
                if self.sess.disc_path.is_none() {
                    self.log_disconnect(if lost_by_connect {
                        sessions::DiscPath::Reconnect
                    } else {
                        sessions::DiscPath::Script
                    });
                }
            } else {
                for e in nsql_run::log_entries(&ev) {
                    if let Some(h) = &self.log_hub {
                        h.push(e.clone());
                    }
                    self.log_win.push(e);
                }
            }
            match ev {
                RunEvent::Begin { index, server, .. } => {
                    if index == 0 {
                        self.txlog.select_session(self.sess.id).begin_batch();
                    }
                    self.run_toast.set_phase(
                        self.sess.run_card,
                        runtoast::Phase::Running {
                            index,
                            total: self.sess.last_run_items.len(),
                        },
                    );
                    let stmt = self
                        .sess
                        .last_run_items
                        .get(index)
                        .cloned()
                        .unwrap_or_default();
                    // 전송 로그는 **서버로 가는 항목에만**(클라이언트 명령은 네트워크 0 · mac 09-21 `PRINT`/`VARIABLE`에 찍히던 것).
                    if server {
                        dlog!(self, LogLayer::Net, LogLevel::Timing, {
                            let now = nsql_log::now_local().stamp();
                            LogEntry::new(
                                LogKind::Send,
                                tf(Msg::LogDetSent, &[&now, &stmt.len().to_string()]),
                            )
                        });
                    }
                    let purpose = if stmt
                        .trim_start()
                        .to_ascii_uppercase()
                        .starts_with("SET AUTOCOMMIT")
                    {
                        TxPurpose::Util
                    } else {
                        TxPurpose::User
                    };
                    self.txlog.select_session(self.sess.id).begin(
                        nsql_log::now_local().stamp(),
                        self.sess.run_editor,
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
                    label,
                } => {
                    self.txlog.select_session(self.sess.id).result(
                        index,
                        rs.rows.len() as u64,
                        elapsed,
                    );
                    self.run_toast.first_page(
                        self.sess.run_card,
                        rs.rows.len() as u64,
                        rs.approx_bytes(),
                        elapsed,
                    );
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
                    self.run_toast.set_phase(
                        self.sess.run_card,
                        runtoast::Phase::Done {
                            rows: Some(rs.rows.len() as u64),
                            secs: elapsed.as_secs_f64(),
                            stages: String::new(),
                        },
                    );
                    self.sess.last_rows = Some(rs.rows.len());
                    self.sess.last_secs = Some(elapsed.as_secs_f64());
                    let n = rs.rows.len().to_string();
                    let secs = format!("{:.3}", elapsed.as_secs_f64());
                    // 페치 상한에서 잘렸으면 "더 있음"을 알린다(DBeaver식 · 사용자 09-15).
                    self.sess.status = if more {
                        tf(Msg::StRowsMore, &[&n, &secs])
                    } else {
                        tf(Msg::StRows, &[&n, &secs])
                    };
                    // 결과는 실행을 시작한 결과 탭의 그리드로(탭이 이미 닫혔으면 버림) · 탭 제목 갱신(T-93).
                    // 이 결과를 만든 문장 하나를 그리드에 알린다 — 건수(Σ)·OFFSET 재질의·새로고침·SQL 복사가 스크립트 전체가
                    // 아니라 **그 문장**을 쓴다 · 조회 문장이 아니면(EXEC/PRINT의 REF CURSOR 등) 건수 불가(09-19 규정).
                    let stmt = self.sess.last_run_items.get(index).cloned();
                    let is_query = stmt.as_deref().is_some_and(|s| {
                        nsql_script::split_script(s).first().is_some_and(|it| {
                            matches!(
                                it.kind,
                                nsql_script::ItemKind::Sql(nsql_script::SqlKind::Query)
                            )
                        })
                    });
                    // ★ 같은 문장이 결과를 둘 이상 냈으면(REF CURSOR 여러 개 · 암묵 결과 · 다중 결과 집합) 두 번째부터는
                    //   딸린 결과 탭으로 — 종전에는 같은 그리드를 덮어써 마지막 것만 남았다(09-21).
                    let same_stmt = self.sess.run_set_stmt == Some(index);
                    let slot = sessions::extra_result_slot(
                        self.sess.run_set_stmt,
                        index,
                        self.sess.run_children,
                        self.settings.flag("grid.result_per_statement"),
                    );
                    self.sess.run_set_stmt = Some(index);
                    let k = match slot {
                        Some(ord) => {
                            self.sess.run_children = ord + 1;
                            self.child_result_tab(self.sess.run_tab, ord)
                        }
                        None => self.sess.run_tab,
                    };
                    // 이름 있는 결과(커서 변수)나 같은 문장의 추가 결과는 조회 문장 하나로 다시 만들 수 없다 → 건수·재질의 불가.
                    let is_query = is_query && !(same_stmt && slot.is_some()) && label.is_none();
                    if let Some(g) = self.grid_for(k) {
                        g.set_result(rs);
                        g.set_more(more);
                        if let Some(s) = stmt.as_deref() {
                            g.set_result_origin(s, is_query);
                        }
                    }
                    self.tx_on_read(index);
                    self.retitle_result(k);
                    if let Some(name) = label {
                        self.title_result_as(k, &name);
                    } else if let (Some(ord), true) = (slot, same_stmt) {
                        // 같은 문장의 이름 없는 추가 결과(암묵 결과 · 다중 결과 집합) = "제목 (n)" · 다른 문장의 결과는 제 SQL에서 제목을 얻었다.
                        let base = self.result_title(self.sess.run_tab);
                        self.title_result_as(k, &format!("{base} ({})", ord + 2));
                    }
                }
                RunEvent::Done {
                    index,
                    rows_affected,
                    elapsed,
                } => {
                    let secs = format!("{:.3}", elapsed.as_secs_f64());
                    self.sess.status = match rows_affected {
                        Some(n) => tf(Msg::StRowsAffected, &[&n.to_string(), &secs]),
                        None => tf(Msg::StOk, &[&secs]),
                    };
                    let stmt = self
                        .sess
                        .last_run_items
                        .get(index)
                        .cloned()
                        .unwrap_or_default();
                    self.txlog
                        .select_session(self.sess.id)
                        .done(index, rows_affected, elapsed);
                    self.run_toast.set_phase(
                        self.sess.run_card,
                        runtoast::Phase::Done {
                            rows: rows_affected,
                            secs: elapsed.as_secs_f64(),
                            stages: String::new(),
                        },
                    );
                    self.tx_on_done(index, &stmt, rows_affected);
                    self.meta_on_done(&stmt);
                }
                // PRINT · VARIABLE 목록 · 서버 메시지는 로그 창으로만(`log_entries`) — 결과 영역은 조회 결과만(사용자 09-17).
                RunEvent::Print { .. } | RunEvent::VarList { .. } => {}
                // 실행 전에 값이 필요하다(D-137) — 입력 창은 이벤트 루프에서 연다(`about_to_wait`). 워커는 답을 기다린다.
                RunEvent::InputNeeded { needs } => {
                    self.sess.status = t(Msg::WinInputs).into();
                    self.input_pending = Some((self.sess.id, needs));
                }
                // ★ 변수 표가 바뀌었다(실행당 한 번 · D-135): 탭 층 = 실행한 탭의 표 · 공유 층 = 이 세션의 표 · 로그에 바뀐 값(비밀은 가림).
                RunEvent::Vars {
                    local,
                    shared,
                    global,
                    changed,
                    defines,
                } => {
                    self.tab_defines.insert(self.sess.run_editor, defines);
                    let line = vars_log_line(&changed, &local, &shared, &global);
                    // 스크립트가 글로벌 층을 바꿨으면(`VAR x GLOBAL` · 대입) 단일 원천 갱신 + 저장 + 다른 세션에 전파.
                    if global != self.global_vars {
                        self.global_vars = global;
                        self.global_vars_changed();
                    }
                    self.vars_changed = (
                        self.sess.run_editor,
                        changed.iter().map(|n| n.to_ascii_uppercase()).collect(),
                    );
                    self.vars_win.redraw();
                    self.vars_persist_save(self.sess.run_editor, &local);
                    self.tab_vars.insert(self.sess.run_editor, local);
                    self.sess.shared_vars = shared;
                    if !line.is_empty() {
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::LogVarsChanged, &[&line]),
                        ));
                    }
                }
                RunEvent::Warning(m) => {
                    // 눈에 띄게(T-202): 상태줄 + 로그(`log_entries`가 이미 넣었다).
                    self.sess.status = m;
                }
                RunEvent::Message(m) => {
                    if m == t(Msg::StCommitted) {
                        self.tx_close(TxOutcome::Committed);
                        self.sess.status = m.clone();
                    } else if m == t(Msg::StRolledBack) {
                        self.tx_close(TxOutcome::RolledBack);
                        self.sess.status = m.clone();
                    }
                }
                RunEvent::Connected {
                    description,
                    dialect,
                } => {
                    self.sess.disc_path = None;
                    self.sess.status = tf(Msg::StConnected, &[&description, &dialect.to_string()]);
                    self.startup_connected = true;
                    if self.sess.skip_done == 0 {
                        self.sess.busy = false;
                    }
                    self.sess.dialect = dialect;
                    self.sess.connected = true;
                    self.sess.broken = false;
                    self.sess.user_disconnected = false;
                    self.sess.idle_closed = false;
                    self.sess.desc = description.clone();
                    self.sess.key_cache.clear();
                    self.sess.touch();
                    self.txlog.set_session_label(self.sess.id, &description);
                    // 새 접속 = 서버의 세션 상태는 처음부터 — 앞 세션에서 세션 설정·임시 데이터를 만들었으면 한 번 알린다.
                    if std::mem::take(&mut self.sess.stateful) {
                        let m = t(Msg::StSessStateLost).to_string();
                        self.log_win.push(LogEntry::new(LogKind::Info, m.clone()));
                        self.toasts
                            .push(toast::ToastKind::Error, description.clone(), m);
                    }
                    // 방언은 **이 세션의 결과 그리드**에만(Copy SQL 방언): 공유 세션 = 전용 탭을 뺀 전부 · 전용 세션 = 주인 탭의 것.
                    self.set_dialect_for_sess_grids(dialect);
                    self.tx_close(TxOutcome::Lost);
                    if self.sess.is_private() {
                        self.log_win.push(LogEntry::new(
                            LogKind::Connect,
                            tf(Msg::StSessPrivateOpened, &[&description]),
                        ));
                    }
                    if self.sess.attempt_inflight {
                        self.primary_sess = self.sess.id;
                    }
                    // ★ 탐색기 = 서버별(docs/52 §2-2): **어떤 세션이든** 붙은 서버의 탐색기를 확보한다(이미 있으면 그대로 · 메타가
                    //   인텔리센스·툴팁의 단일 원천이라 그 서버에 붙은 세션이 하나라도 있는 동안 유지된다). 활성 탭의 세션이면 앞으로.
                    self.explorer_attach();
                }
                RunEvent::Disconnected | RunEvent::ConnectionClosed => {
                    // 접속이 끊겼다: 결과는 기본으로 **보기용으로 남긴다**(09-18 결정) — `mem.release_results_on_disconnect`를
                    //   켜면 그 세션으로 받은 결과 그리드를 비워 메모리를 바로 돌려준다.
                    if self.settings.flag("mem.release_results_on_disconnect") {
                        self.grid.clear_result();
                        for g in self.sleeping_grids_mut() {
                            g.clear_result();
                        }
                    }
                    self.mem_released();
                    self.sess.status = t(Msg::StDisconnected).into();
                    self.sess.connected = false;
                    // (탐색기는 `sync_sess_ui`의 참조 수 맞춤이 처리한다 — 이 서버에 붙은 세션이 남아 있으면 유지.)
                    self.tx_close(TxOutcome::Lost);
                    // 공유 모드의 전용 세션이 스크립트 안의 DISCONNECT로 끊겼다 → 세션을 거두고 공유 세션으로 복귀(유휴 닫기는 제외).
                    // ★ 사용자가 배지 메뉴 "No connection"으로 **명시적으로** 끊은 세션(`user_disconnected`)은 거두지 않는다 —
                    //   공유로 되돌리면 사용자의 선택을 무시하는 것(09-19). 툴바 Disconnect(keep=false)는 종전대로 공유 복귀.
                    // ★ `CONNECT`가 실패해 접속을 잃은 경우(`lost_by_connect`)는 **거두지 않는다**(사용자 09-21 "첫 번째에는 오류
                    //   토스트가 안 뜬다"): 거두면 ① 이 세션의 실행 상태 카드·상태줄에 실릴 접속 오류가 세션과 함께 사라지고
                    //   ② 탭이 공유 연결(다른 서버일 수 있다)로 조용히 돌아간다. 탭은 "연결 없음"인 전용 세션으로 남는다 —
                    //   처음부터 접속에 실패한 전용 세션과 같은 모습이다.
                    if sessions::reap_on_disconnect(
                        self.sess.is_private(),
                        self.sess.idle_closed,
                        self.sess.user_disconnected,
                        self.session_mode() == SessionMode::Shared,
                        lost_by_connect,
                    ) {
                        self.sess.closing = true;
                    }
                }
                RunEvent::ReadTxEnded { index, deferred } => self.tx_on_read_ended(index, deferred),
                RunEvent::Timing { index, timeline } => {
                    // 상태줄 = 결과 요약 + 단계별 소요(docs/26). 렌더 시간은 그리드 푸터가 자체 표시.
                    self.txlog
                        .select_session(self.sess.id)
                        .timing(index, timeline.total());
                    self.run_toast.timing(
                        self.sess.run_card,
                        timeline.total().as_secs_f64(),
                        timeline.summary(),
                    );
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
                    self.sess.status = format!("{} · ⏱ {}", self.sess.status, timeline.summary());
                }
                RunEvent::Error { index, line, error } => {
                    self.txlog.select_session(self.sess.id).error(
                        index,
                        error.code,
                        &error.message,
                    );
                    self.txlog_win.redraw();
                    if std::mem::take(&mut self.sess.run_cancel_requested) {
                        // 사용자가 ■를 눌러 드라이버가 끊은 실행 — 오류 토스트 대신 "중지됨"(T-108).
                        if std::mem::take(&mut self.sess.run_cancel_drops) {
                            // 소켓을 끊은 취소(SQL Server): 열린 트랜잭션은 서버가 롤백 · 다음 실행 때 자동 재접속.
                            self.tx_close(TxOutcome::Lost);
                            self.sess.status = t(Msg::StRunCancelledDrop).into();
                            self.log_win
                                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                        } else {
                            self.sess.status = t(Msg::StRunCancelled).into();
                        }
                        self.run_toast
                            .set_phase(self.sess.run_card, runtoast::Phase::Stopped { rows: 0 });
                        if let Some(g) = self.run_grid() {
                            g.clear_result();
                        }
                        self.sess.busy = false;
                        self.sync_run_stmt_button();
                        self.editors.set_running(self.sess.run_editor, false);
                        self.redraw();
                        continue;
                    }
                    // 공통 분류 + 코드 부각(docs/42): 상태줄 · 결과 메시지 · 로그 창 · 토스트(분류된 오류만).
                    let stmt = self
                        .sess
                        .last_run_items
                        .get(index)
                        .cloned()
                        .unwrap_or_default();
                    let err_dialect = sessions::error_dialect(
                        self.sess.connected,
                        self.sess.dialect,
                        self.sess.last_spec.as_ref().and_then(|s| s.dialect),
                    );
                    let (cls, summary) =
                        toast::summarize(err_dialect, error.code, &error.message, &stmt);
                    self.sess.status = tf(Msg::StErrorLine, &[&line.to_string(), &summary]);
                    // "테이블/뷰 없음"인데 탐색기에는 그 이름이 있다 = 트리가 낡았다 → 그 폴더만 다시(docs/57 T4).
                    if cls.class == nsql_core::ErrorClass::NoTable
                        && self.settings.flag("meta.refresh_on_missing")
                    {
                        let name = explorer::missing_name(&error.message);
                        let schema = self.meta_default_schema();
                        self.explorer.note_missing(
                            self.sess.spec.as_ref(),
                            name.as_deref(),
                            schema.as_deref(),
                        );
                    }
                    if self.settings.flag("editor.minimap_errors") && line > 0 {
                        let ed_line = self.sess.run_line_base + line - 1;
                        self.editors
                            .set_error_line(self.sess.run_editor, Some(ed_line));
                    }
                    self.run_toast
                        .set_phase(self.sess.run_card, runtoast::Phase::Error(summary.clone()));
                    // 오류가 나도 결과 영역은 기본 형태(빈 그리드)로 — 본문은 로그 창·상태줄·토스트(사용자 09-17).
                    if let Some(g) = self.run_grid() {
                        g.clear_result();
                    }
                    // ★ 같은 오류를 두 번 보이지 않는다(사용자 09-19 "오류가 왜 2번 출력되나"): 실행 상태 카드(`run.toast`)가
                    //   이미 분류된 오류 요약을 빨간 카드로 보여 주므로, 카드가 켜져 있으면 오류 토스트는 띄우지 않는다
                    //   (카드를 끈 사용자에게만 토스트 · 로그 창에는 종전대로 한 줄).
                    let card_shows = self.settings.flag("run.toast");
                    if let Some(label) = toast::class_label(cls.class).filter(|_| !card_shows) {
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
                    self.sess.busy = false;
                    // 실행 중 접속성 오류 확인 → 활성 서버 신호등 즉시 갱신(사용자 09-14).
                    if probe::is_connection_error(error.code, &error.message) {
                        if self.sess.id == self.primary_sess {
                            let name = self.conn_win.active_name().to_string();
                            self.conn_win.note_failure(&name);
                        }
                        if !self.sess.broken {
                            self.sess.broken = true;
                            self.sync_sess_ui();
                        }
                    }
                }
            }
        }
        while let Ok(done) = self.sess.worker.done.try_recv() {
            changed = true;
            self.sess.touch();
            if self.sess.skip_done > 0 {
                // 실행 앞에 끼운 재접속의 완료 — 실패했으면 알리기만 하고 뒤따르는 작업의 완료를 기다린다.
                self.sess.skip_done -= 1;
                if let Some(m) = done {
                    self.sess.status = m;
                }
                continue;
            }
            self.sess.busy = false;
            self.sync_run_stmt_button();
            self.editors.set_running(self.sess.run_editor, false);
            // 실행이 끝났다 = 이번 실행에서 쓰이지 않은 딸린 결과 탭(앞선 실행의 커서 탭)을 걷는다.
            if std::mem::take(&mut self.sess.run_tracking) {
                self.prune_child_results(self.sess.run_tab, self.sess.run_children);
            }
            // 실행이 끝났다 = 이번 실행의 DDL을 폴더별로 한 번만 탐색기에 반영(스크립트 디바운스 · docs/57 T1).
            let ddl = std::mem::take(&mut self.sess.ddl_now);
            self.meta_flush(ddl);
            // 실행이 끝났다 = 앞선 결과(그리드·텍스트 보기·페치 버퍼)를 놓았다 → 1초 뒤 힙을 한 번 정리.
            self.mem_released();
            let failed = done.is_some();
            // 뒤에서 끝난 실행(D-104): 그 탭 제목 앞에 ✓/✗ — 탭을 보면 지워진다.
            if self.editors.active_id() != self.sess.run_editor {
                self.editors
                    .set_done_mark(self.sess.run_editor, Some(!failed));
            }
            if std::mem::take(&mut self.sess.run_cancel_requested) && !failed {
                // Attention 취소(SQL Server · 세션 유지): 오류 없이 부분 결과로 끝난다 → "중지됨".
                self.sess.run_cancel_drops = false;
                // ★ 실행 중지 = 받은 행은 **보기만 유지**(사용자 09-17 결정): 이번 실행 스트림의 앞부분이라 보는 용도로는 정확하지만
                //   OFFSET 재실행은 정렬이 없으면 순서가 달라질 수 있어 이어 받기(⇊)·자동 페치는 막는다(`more=false`) · 전체는 재실행.
                //   (⇊ 나머지 이어 받기의 중지는 T-48b대로 받은 행 + 더 있음 유지 — 늘 연속된 앞부분 · 사용자 "이전 세그먼트 방식 유지".)
                let rows = self.sess.last_rows.unwrap_or(0) as u64;
                self.sess.status = tf(Msg::StRunCancelledPartial, &[&rows.to_string()]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                self.run_toast
                    .finish(self.sess.run_card, runtoast::Phase::Stopped { rows });
                if let Some(g) = self.run_grid() {
                    g.set_more(false);
                }
            } else {
                self.run_toast.finish_keep(self.sess.run_card, done.clone());
            }
            if let Some(m) = done {
                self.sess.status = m;
            }
            // 현재 문장 실행 뒤 캐럿(설정 `run.after_statement` · 사용자 09-16): stay / next_ok / next_always.
            if std::mem::take(&mut self.sess.single_run)
                && self.editors.active_id() == self.sess.run_editor
            {
                let mode = self.settings.get("run.after_statement").unwrap_or("stay");
                if mode == "next_always" || (mode == "next_ok" && !failed) {
                    self.goto_statement(true);
                }
            }
        }
        if self.explorer.drain() {
            changed = true;
            // "불러오는 중"인 완성 팝업은 메타가 도착한 세대에 다시 조립한다(09-23).
            if self.intel.is_loading() {
                self.intel_request(false);
            }
        }
        if self.live_drain() {
            changed = true;
        }
        if self.tx_block_drain() {
            changed = true;
        }
        if self.explorer_actions() {
            changed = true;
        }
        if changed {
            self.redraw();
        }
    }

    /// 지금 세션(`self.sess`)의 결과 그리드에 방언을 알린다 — 전용 세션 = 주인 탭의 패널 · 공유 세션 = 전용 탭이 아닌 패널 전부.
    fn set_dialect_for_sess_grids(&mut self, dialect: Dialect) {
        let owner = self.sess.owner;
        let private_tabs: Vec<u64> = self.all_sess().filter_map(|s| s.owner).collect();
        let mine = |tab: u64| match owner {
            Some(o) => tab == o,
            None => !private_tabs.contains(&tab),
        };
        if mine(self.panel_editor) {
            self.grid.set_dialect(dialect);
            for t in &mut self.panel.tabs {
                t.grid.set_dialect(dialect);
            }
        }
        for (tab, p) in &mut self.panels {
            if mine(*tab) {
                for t in &mut p.tabs {
                    t.grid.set_dialect(dialect);
                }
            }
        }
    }

    // ───────────── 북마크(docs/69 · T-167) ─────────────

    /// 탭 하나의 거터 마크를 저장소에서 다시 만든다.
    fn bm_refresh_tab(&mut self, i: usize) {
        let (c, d) = (self.theme.accent, self.theme.text_dim);
        let marks = if self.bookmarks.enabled && self.settings.flag("bookmark.gutter") {
            self.bookmarks.marks_for(&self.editors, i, c, d)
        } else {
            Vec::new()
        };
        let labels = if self.bookmarks.enabled && self.settings.flag("bookmark.gutter") {
            self.bookmarks.labels_for(&self.editors, i)
        } else {
            Vec::new()
        };
        let on = self.bookmarks.enabled;
        let mm = if on && self.settings.flag("bookmark.minimap") {
            self.bookmarks.minimap_for(&self.editors, i, c)
        } else {
            Vec::new()
        };
        let inline = if on && self.settings.flag("bookmark.inline_label") {
            let cap = self.settings.int("bookmark.inline_label_chars").max(8) as usize;
            self.bookmarks.inline_for(&self.editors, i, cap)
        } else {
            Vec::new()
        };
        if let Some(tb) = self.editors.tab_box_mut(i) {
            tb.set_line_marks(marks);
            tb.set_gutter_labels(labels);
            tb.set_minimap_marks(mm);
            tb.set_inline_labels(inline);
        }
    }

    /// 표시 전부(거터 · 패널 · 상태줄) 갱신.
    fn bm_sync_ui(&mut self) {
        for i in 0..self.editors.len() {
            self.bm_refresh_tab(i);
        }
        self.bm_titles_rev = self.editors.titles_rev();
        self.bm_panel.set_tab_titles(self.editors.tab_titles());
        self.bm_panel.sync(&self.bookmarks.store);
        self.redraw();
    }

    /// 문서 열쇠의 표시 이름(토스트·상태줄) — 이름 없는 탭은 id로 지금 탭 제목(패널과 같은 규칙).
    fn bm_doc_name(&self, doc: &nsql_bookmarks::DocKey) -> String {
        self.bm_panel.doc_name(doc)
    }

    /// 틱: 활성 탭의 줄 변경 기록 소비(L0) · 바뀐 표시 갱신 · 디바운스 저장 · **탭 이름이 바뀌면 패널의 문서 이름**(id → 제목).
    fn bm_tick(&mut self) {
        if !self.bookmarks.enabled {
            return;
        }
        let i = self.editors.active();
        let moved = self.bookmarks.sync_tab(&self.editors, i);
        if self.bookmarks.take_changed() || moved {
            self.bm_sync_ui();
        } else if self.bm_titles_rev != self.editors.titles_rev() {
            // 탭 제목 세대가 바뀐 때만(이름 바꾸기 · 열기/닫기 · 저장) — 매 틱 제목 비교는 하지 않는다.
            self.bm_titles_rev = self.editors.titles_rev();
            if self.bm_panel.set_tab_titles(self.editors.tab_titles()) {
                self.redraw();
            }
        }
        self.bookmarks.tick_save();
    }

    /// 패널이 낸 요청 거두기.
    fn bm_pump(&mut self) {
        while let Some(a) = self.bm_panel.take_action() {
            match a {
                bookmarks_panel::BmAction::Goto(id, permanent) => self.bm_goto(id, permanent),
                bookmarks_panel::BmAction::Remove(id) => {
                    if let Some(b) = self.bookmarks.remove(id) {
                        self.bm_removed_toast(1, &b.display());
                    }
                }
                bookmarks_panel::BmAction::Rename(id, text) => {
                    self.bookmarks.set_label(id, Some(text))
                }
                bookmarks_panel::BmAction::Mnemonic(id, n) => self.bookmarks.set_mnemonic(id, n),
                bookmarks_panel::BmAction::MoveGroup(id, g) => self.bookmarks.move_group(id, g),
                bookmarks_panel::BmAction::RemoveDoc(doc) => {
                    let n = self.bookmarks.remove_doc(&doc);
                    if n > 0 {
                        let name = self.bm_doc_name(&doc);
                        self.bm_removed_toast(n, &name);
                    }
                }
                bookmarks_panel::BmAction::NewGroup => {
                    let base = t(Msg::BmNewGroupName).to_string();
                    let gid = self.bookmarks.new_group(&base);
                    self.bm_panel.sync(&self.bookmarks.store);
                    self.sess.status = tf(Msg::StBookmarkGroupNew, &[&gid.to_string()]);
                }
                bookmarks_panel::BmAction::RenameGroup(g, name) => {
                    self.bookmarks.rename_group(g, &name)
                }
                bookmarks_panel::BmAction::ToggleGroup(g) => self.bookmarks.toggle_group(g),
                bookmarks_panel::BmAction::DefaultGroup(g) => self.bookmarks.set_default_group(g),
                bookmarks_panel::BmAction::DeleteGroup(g, keep) => {
                    if !self.bookmarks.delete_group(g, keep) {
                        self.sess.status = t(Msg::StBookmarkGroupDefault).into();
                    } else if !keep {
                        self.bm_removed_toast(0, "");
                    }
                }
                bookmarks_panel::BmAction::RemoveInvalid => {
                    let n = self.bookmarks.remove_invalid();
                    if n > 0 {
                        self.bm_removed_toast(n, "");
                    }
                }
                bookmarks_panel::BmAction::OpenSettings => self.menu_action("edit.prefs"),
                bookmarks_panel::BmAction::SelectAllInDoc(doc) => {
                    if let Some(i) = self.bm_tab_of(&doc) {
                        self.editors.switch(i);
                        self.bm_select_all_in(i);
                        self.set_focus(Focus::Editor);
                    }
                }
            }
        }
        if self.bookmarks.take_changed() {
            self.bm_sync_ui();
        }
    }

    /// 제거 뒤 5초 [되돌리기] 토스트(69 C-28 · Ctrl+Z는 본문만) — 클릭 = `bookmark.undo_remove`.
    fn bm_removed_toast(&mut self, n: usize, what: &str) {
        let body = if n == 1 {
            tf(Msg::StBookmarkRemoved, &[what])
        } else {
            tf(Msg::StBookmarkCleared, &[&n.to_string()])
        };
        self.sess.status = body.clone();
        self.toasts.push_action(
            toast::ToastKind::Info,
            body,
            t(Msg::StBookmarkUndo).to_string(),
            "bookmark.undo_remove",
        );
    }

    fn bm_tab_of(&self, doc: &nsql_bookmarks::DocKey) -> Option<usize> {
        let ci = cfg!(any(windows, target_os = "macos"));
        (0..self.editors.len())
            .find(|&i| bookmarks::Bookmarks::doc_key(&self.editors, i).same(doc, ci))
    }

    /// 북마크로 이동 — 열린 탭이면 전환 · 파일이면 열고 · 그 줄로.
    /// 북마크로 이동 — `permanent` = 정식 탭(더블클릭·Enter·메뉴 Open · 이미 미리보기로 열려 있으면 승격) ·
    /// false = 미리보기 탭(한 번 클릭 · 69 B4b · 프로젝트 탐색기와 같은 규칙 `project.preview_tab` · 편집하면 승격).
    fn bm_goto(&mut self, id: u64, permanent: bool) {
        let Some(b) = self.bookmarks.store.get(id).cloned() else {
            return;
        };
        let mut idx = self.bm_tab_of(&b.doc);
        if let nsql_bookmarks::DocKey::File { path } = &b.doc {
            if idx.is_none() || permanent {
                self.project_open_req(project_panel::OpenReq {
                    path: PathBuf::from(path),
                    permanent,
                });
                idx = self.bm_tab_of(&b.doc);
            }
        }
        let Some(i) = idx else {
            self.sess.status = t(Msg::StBookmarkNoDoc).into();
            return;
        };
        self.editors.switch(i);
        self.editors.cur_mut().goto_line(b.anchor.line as usize + 1);
        if let Some(bm) = self.bookmarks.store.get_mut(id) {
            bm.visited = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
        }
        self.set_focus(Focus::Editor);
        self.redraw();
    }

    /// 문서의 북마크 줄 전부에 캐럿(Sublime Select All Bookmarks · 69 §6-1).
    fn bm_select_all_in(&mut self, i: usize) {
        let lines = self.bookmarks.doc_lines(&self.editors, i);
        if lines.is_empty() {
            self.sess.status = t(Msg::StBookmarkNone).into();
            return;
        }
        if let Some(tb) = self.editors.tab_box_mut(i) {
            let regions: Vec<(usize, usize)> = lines
                .iter()
                .map(|&l| {
                    let a = tb
                        .buf()
                        .line_start(l.min(tb.buf().line_count().saturating_sub(1)));
                    (a, a)
                })
                .collect();
            tb.set_regions_pub(&regions);
        }
        let n = lines.len();
        self.sess.status = tf(Msg::StSelections, &[&n.to_string()]);
    }

    /// `bookmark.*` 명령.
    /// 거터 우클릭(사용자 09-23): 활성 편집기의 거터(북마크/니모닉 영역 + 줄번호) 안이면 그 줄로 캐럿을 옮기고 상태에 맞는 메뉴를 연다.
    /// 열었으면 true(사건 소비) · 거터 밖이면 false(본문 우클릭 = 종전 편집 메뉴).
    fn open_bm_gutter_menu(&mut self, p: Point) -> bool {
        if !self.bookmarks.enabled {
            return false;
        }
        let i = self.editors.active();
        let Some(tb) = self.editors.tab_box(i) else {
            return false;
        };
        if !tb.in_gutter(p) {
            return false;
        }
        // 마지막 줄 아래 빈 영역 = 줄 단위 항목(토글·니모닉) 없이 "북마크 보기"만(사용자 09-23).
        let on_text = !tb.below_text(p);
        let line = on_text.then(|| tb.line_at_point(p)).flatten();
        let state = line.map(|line| {
            self.editors.cur_mut().goto_line(line + 1);
            let (bm, mn) = self.bookmarks.line_state(&self.editors, i, line);
            (line, bm, mn)
        });
        self.bm_gutter = state.map(|(line, bm, _)| (i, line, bm));
        self.close_context_menus();
        let items = bm_gutter_items(state.map(|(_, bm, mn)| (bm.is_some(), mn)));
        self.open_status_popup_w(Rect::new(p.x, p.y, 1, 1), items, 170.0);
        true
    }

    /// 거터 메뉴의 답 — `view` = 좌측 북마크 패널 열기 · `toggle` = 토글(캐럿은 이미 그 줄) · `mn:<n>` = 니모닉 지정(없으면 만들어서) · `mn:clear` = 해제.
    fn bm_gutter_pick(&mut self, rest: &str) {
        if rest == "view" {
            self.bm_gutter = None;
            if !self.bm_panel.is_visible() {
                self.menu_action("view.bookmarks");
            }
            self.redraw();
            return;
        }
        let Some((i, line, bm)) = self.bm_gutter.take() else {
            return;
        };
        if self.editors.active() != i {
            self.editors.switch(i);
        }
        self.editors.cur_mut().goto_line(line + 1);
        match rest {
            "toggle" => self.bookmark_cmd("bookmark.toggle"),
            "mn:clear" => {
                if let Some(id) = bm {
                    self.bookmarks.set_mnemonic(id, None);
                    self.bm_sync_ui();
                }
            }
            other => {
                if let Some(n) = other.strip_prefix("mn:") {
                    self.bookmark_cmd(&format!("bookmark.set_{n}"));
                }
            }
        }
        self.redraw();
    }

    fn bookmark_cmd(&mut self, id: &str) {
        if !self.bookmarks.enabled {
            self.sess.status = t(Msg::StBookmarkOff).into();
            return;
        }
        let i = self.editors.active();
        if let Some(n) = id
            .strip_prefix("bookmark.set_")
            .and_then(|n| n.parse::<u8>().ok())
        {
            match self.bookmarks.set_mnemonic_at_caret(&self.editors, i, n) {
                Ok(_) => {
                    let line = self.editors.caret_line_col().0;
                    self.sess.status = tf(
                        Msg::StBookmarkMnemonic,
                        &[&n.to_string(), &line.to_string()],
                    );
                }
                Err(k) => self.sess.status = tf(Msg::StBookmarkCap, &[&format!("bookmark.{k}")]),
            }
        } else if let Some(n) = id
            .strip_prefix("bookmark.goto_")
            .and_then(|n| n.parse::<u8>().ok())
        {
            match self.bookmarks.mnemonic_line(&self.editors, i, n) {
                Some(l) => {
                    self.editors.cur_mut().goto_line(l + 1);
                    self.set_focus(Focus::Editor);
                }
                None => self.sess.status = tf(Msg::StBookmarkNoMnemonic, &[&n.to_string()]),
            }
        } else {
            match id {
                "bookmark.toggle" => match self.bookmarks.toggle_caret(&self.editors, i) {
                    Ok(true) => {
                        let line = self.editors.caret_line_col().0;
                        self.sess.status = tf(Msg::StBookmarkAdded, &[&line.to_string()]);
                    }
                    Ok(false) => self.sess.status = tf(Msg::StBookmarkRemoved, &[""]),
                    Err(k) => {
                        self.sess.status = tf(Msg::StBookmarkCap, &[&format!("bookmark.{k}")])
                    }
                },
                "bookmark.next" | "bookmark.prev" => {
                    match self
                        .bookmarks
                        .next_line(&self.editors, i, id == "bookmark.next")
                    {
                        Some(l) => {
                            self.editors.cur_mut().goto_line(l + 1);
                            self.set_focus(Focus::Editor);
                        }
                        None => self.sess.status = t(Msg::StBookmarkNone).into(),
                    }
                }
                "bookmark.clear_doc" => {
                    let n = self.bookmarks.clear_doc(&self.editors, i);
                    self.sess.status = tf(Msg::StBookmarkCleared, &[&n.to_string()]);
                }
                "bookmark.select_all" => self.bm_select_all_in(i),
                "bookmark.undo_remove" => {
                    let n = self.bookmarks.undo_remove();
                    self.sess.status = tf(Msg::StBookmarkRestored, &[&n.to_string()]);
                }
                "bookmark.label" => {
                    if !self.bm_panel.is_visible() {
                        self.menu_action("view.bookmarks");
                    }
                    if !self.bm_panel.begin_rename() {
                        self.set_focus(Focus::Bookmarks);
                    }
                }
                _ => {}
            }
        }
        if self.bookmarks.take_changed() {
            self.bm_sync_ui();
        }
        self.redraw();
    }

    /// 다중 선택 구간 수 상한(`editor.max_occurrences` · 최소 100 · docs/72 §2).
    fn occurrence_cap(&self) -> usize {
        self.settings.int("editor.max_occurrences").max(100) as usize
    }

    /// 행 포커스 배경 설정 → 전 그리드(사용자 09-22).
    fn apply_grid_row_focus(&mut self) {
        let on = self.settings.flag("grid.row_focus");
        let (c, a) = color_alpha_setting(&self.settings, "grid.row_focus_color");
        self.all_grids().for_each(|g| g.set_row_focus(on, c, a));
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
    /// 입력 창의 답을 그 세션의 워커에 보내고 창을 닫는다(다중 세션: 물은 세션에게).
    /// 비밀번호 입력 창의 답을 **물은 세션의 워커**에 보내고 창을 닫는다. 값이면 둘째 사본을 잠깐 쥔다 — 같은 서버의 탐색기
    /// 메타 세션이 같은 자격으로 붙어야 트리가 보인다(`explorer_attach`가 소비 · 못 쓰면 20초 뒤 폐기).
    fn password_reply(&mut self, reply: worker::PwReply) {
        let sid = self.input_win.sess;
        self.input_win.close();
        self.key_guard = Some(Instant::now());
        self.pw_pending = None;
        // 세션 자격 금고가 켜져 있으면 탐색기 메타 세션은 금고에서 빌린다 → 둘째 사본을 쥘 필요가 없다.
        let lend = !worker::remember_session_password();
        self.pw_once = match &reply {
            worker::PwReply::Value(s) if lend => Some((
                sid,
                nsql_core::Secret::new(s.expose().to_string()),
                Instant::now(),
            )),
            _ => None,
        };
        if sid == self.sess.id {
            self.sess.worker.password(reply);
        } else {
            let mut reply = Some(reply);
            self.with_sess(sid, |a| {
                if let Some(r) = reply.take() {
                    a.sess.worker.password(r);
                }
            });
        }
        if let Some(w) = &self.window {
            w.focus_window();
        }
        self.redraw();
    }

    fn input_reply(&mut self, reply: worker::InputReply) {
        let sid = self.input_win.sess;
        self.input_win.close();
        self.key_guard = Some(Instant::now());
        self.input_pending = None;
        if sid == self.sess.id {
            self.sess.worker.input(reply);
        } else {
            let mut reply = Some(reply);
            self.with_sess(sid, |a| {
                if let Some(r) = reply.take() {
                    a.sess.worker.input(r);
                }
            });
        }
        if let Some(w) = &self.window {
            w.focus_window();
        }
        self.redraw();
    }

    /// 지금 실행하려는 탭의 변수 표(탭 층) — 없으면 빈 표. 실행 명령에 실어 보낸다(D-135).
    fn run_vars(&self) -> Option<Vec<nsql_script::VarState>> {
        Some(
            self.tab_vars
                .get(&self.editors.active_id())
                .cloned()
                .unwrap_or_default(),
        )
    }

    fn run_grid(&mut self) -> Option<&mut grid::Grid> {
        let k = self.sess.run_tab;
        self.grid_for(k)
    }

    /// ★ 편집기 탭 ↔ 결과 패널 쌍 동기화(사용자 09-16 · T-93): 활성 편집기 탭이 바뀌었으면 그 탭의 패널을 꺼내 오고
    /// (없으면 설정만 물려받은 빈 탭 하나) 지금 패널은 잠재운다 · 닫힌 편집기 탭의 패널은 통째로 버린다(rows 즉시 해제).
    /// 페인트 직전과 이벤트 뒤에 부른다.
    fn sync_grid_tab(&mut self) {
        self.sync_tabs_menu();
        // 탭을 바꿨다 = 그 파일을 확인하고(오래 안 본 탭) 확인 띠를 그 탭의 것으로.
        let tab = self.editors.active_id();
        if tab != self.ext_last_tab {
            self.ext_last_tab = tab;
            self.vars_win_context();
            self.ext_check(false);
            self.ext_banner_sync();
        }
        // 활성 탭의 세션도 같은 시점에 맞춘다(docs/52) — 표식 클릭·메뉴 선택도 여기서 거둔다.
        if let Some(i) = self.editors.take_badge_request() {
            self.open_badge_menu(i);
        }
        if let Some((tab, id)) = self.editors.take_badge_pick() {
            self.badge_pick(tab, &id);
        }
        // 탭 바 [+]로 만든 새 탭(메뉴 New와 같은 규칙).
        if self.editors.take_new_tab_created() {
            self.on_new_tab();
        }
        // ★ 순서(09-19): 거두기 → 활성 탭 세션 맞추기(새 탭을 공유 연결에 **묶는다**) → 표식·배지. 종전엔 표식을 먼저 맞춰
        //   새 탭이 아직 안 묶인 상태로 배지(함께 쓰는 탭 수)를 계산했고, 다음 탭 전환 때에야 숫자가 늘었다.
        self.reap_sessions();
        self.sync_sess();
        // 탭 목록(새 탭·닫기)이나 묶임이 바뀌었으면 표식·해제 버튼 배지를 바로 맞춘다 — 이벤트를 기다리지 않는다(바뀔 때만 · 페인트마다 아님).
        let ids = self.editors.tab_ids();
        if ids != self.badge_tabs || self.sess_ui_dirty {
            self.badge_tabs = ids;
            self.sess_ui_dirty = false;
            self.sync_sess_ui();
        }
        self.sync_gate();
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
            let always = self.settings.flag("grid.result_tabbar_single");
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
                        sql: String::new(),
                        grid: fresh,
                        seq: id,
                        child_of: None,
                    },
                    enabled,
                    always,
                )
            });
            let mut next = next;
            next.set_numbered(self.settings.get("grid.result_tab_title") != Some("table"));
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
        self.tab_vars.retain(|id, _| alive.contains(id));
    }

    fn paint(&mut self) {
        // 상태줄 메모리 글은 그리기 빌림 전에(5 s에 한 번 OS 조회 · docs/80).
        let mem_txt = self.mem_status_text();
        // 찾기가 열린 동안 본문이 바뀌면 일치 표시도 따라간다(전체 스캔 · 열려 있을 때만 · T-73).
        if self.find.is_visible() {
            self.sync_find_marks();
        }
        let t_frame = Instant::now();
        self.tmark("paint begin");
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
            || !self.main_active
            || !nexa_sys::layer_present::app_active().unwrap_or(true)
            || (self.blink_origin.elapsed().as_millis() / 500).is_multiple_of(2);
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
                let busy = if self.sess.blocked() { "⏳ " } else { "" };
                let ty = dc.text_center_y(sy, px(24.0, s));
                // 전용 세션 탭 = 상태줄 앞에 "전용: 접속 설명"(Golden의 Private Session 줄 · docs/52 §7).
                let broken = if self.sess.broken {
                    format!("[{}] ", t(Msg::StSessBrokenTag))
                } else {
                    String::new()
                };
                let left_text = if self.sess.is_private() {
                    format!(
                        "{busy}{broken}[{}: {}] {}",
                        t(Msg::StSessPrivate),
                        if self.sess.desc.is_empty() {
                            "—"
                        } else {
                            self.sess.desc.as_str()
                        },
                        self.sess.status
                    )
                } else {
                    format!("{busy}{broken}{}", self.sess.status)
                };
                // 오른쪽 세그먼트(Sublime/DBeaver/Golden 참고 · docs/29 §4): 접속 · Ln,Col · rows · time · 구문(클릭 = Set Syntax)
                let (ln, col) = self.editors.caret_line_col();
                let mut segs: Vec<(String, bool)> = Vec::new();
                // 트랜잭션 모드(자동/수동 · 수동에 미커밋 변경이 있으면 ●).
                // 트랜잭션 세그먼트(DR-30): Auto / Manual / "Manual ● n pending · since hh:mm" · 클릭 = 팝업.
                let tx = if self.settings.flag("session.autocommit") {
                    t(Msg::StTxAuto).to_string()
                } else if let Some(first) = self.sess.tx_pending.first() {
                    let base = tf(
                        Msg::StTxPending,
                        &[&self.sess.tx_pending.len().to_string(), &first.when],
                    );
                    // 내 미커밋이 남을 막고 있으면 상태줄에도(docs/56 L3).
                    if self.sess.tx_blockers > 0 {
                        format!(
                            "{base} · {}",
                            tf(Msg::StatusTxBlocking, &[&self.sess.tx_blockers.to_string()])
                        )
                    } else {
                        base
                    }
                } else {
                    t(Msg::StTxManual).to_string()
                };
                let tx_idx = segs.len();
                segs.push((tx, false));
                // 메모리 총량 세그먼트(docs/80 · `mem.statusbar` · 클릭 = 메모리 맵 창) — 조회는 5 s에 한 번, 그릴 때만.
                let mem_idx = mem_txt.map(|txt| {
                    segs.push((txt, false));
                    segs.len() - 1
                });
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
                // 큰 파일 모드 · 읽기 전용 표식(docs/59) — 이 탭에서 일부 기능이 꺼져 있음을 늘 보이게.
                let (large_level, large_forced) = self.editors.active_large();
                if large_level > 0 && !large_forced {
                    segs.push((tf(Msg::StLargeSeg, &[&large_level.to_string()]), false));
                }
                if self.editors.active_read_only() {
                    segs.push((t(Msg::StReadOnlySeg).to_string(), false));
                }
                // 북마크 `이 문서/전체`(docs/69 §6-2 · `bookmark.statusbar`).
                if self.bookmarks.enabled && self.settings.flag("bookmark.statusbar") {
                    let (here, all) = self
                        .bookmarks
                        .counts_for(&self.editors, self.editors.active());
                    if all > 0 {
                        segs.push((
                            tf(Msg::StBookmarkSeg, &[&here.to_string(), &all.to_string()]),
                            false,
                        ));
                    }
                }
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
                if let Some(n) = self.sess.last_rows {
                    segs.push((tf(Msg::StRowsShort, &[&n.to_string()]), false));
                }
                if let Some(secs) = self.sess.last_secs {
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
                self.status_mem_rect = Rect::new(0, 0, 0, 0);
                let last = segs.len() - 1;
                for (idx, (text, is_syntax)) in segs.iter().enumerate().rev() {
                    let tw = dc.text_width(text);
                    xr -= tw;
                    let r = Rect::new(xr - gap / 2, sy, tw + gap, px(24.0, s));
                    if idx == tx_idx {
                        self.status_tx_rect = r;
                    }
                    if Some(idx) == mem_idx {
                        self.status_mem_rect = r;
                        // 클릭 효과 = 상태 레이어(hover · pressed · 버튼과 같은 부품).
                        let st = if self.mem_pressed {
                            nexa_ctl::tokens::State::Pressed
                        } else if self.pointer.is_some_and(|p| r.contains(p)) {
                            nexa_ctl::tokens::State::Hover
                        } else {
                            nexa_ctl::tokens::State::Rest
                        };
                        dc.state_layer(r, th.text, st);
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
                // 접속 유형 표식(운영 = 위험색 · 시험 = 경고색 칩 · 상태줄 맨 앞) — 지금 탭의 세션이 붙어 있을 때만.
                let env_chip = self
                    .sess
                    .connected
                    .then(|| self.sess.spec.as_ref().and_then(|sp| sp.env))
                    .flatten()
                    .and_then(|e| match e {
                        nsql_script::ConnEnv::Prod => Some(("PROD", th.danger)),
                        nsql_script::ConnEnv::Test => Some(("TEST", th.warn)),
                        nsql_script::ConnEnv::Dev => None,
                    });
                let mut lx = px(8.0, s);
                if let Some((label, color)) = env_chip {
                    let cw = dc.text_width(label) + px(12.0, s);
                    let chip = Rect::new(lx, sy + px(4.0, s), cw, px(16.0, s));
                    dc.fill_round_rect(chip, px(3.0, s), color);
                    let cy = dc.text_center_y(chip.y, chip.h);
                    dc.text(chip.x + px(6.0, s), cy, chip, label, th.panel_bg);
                    lx += cw + px(6.0, s);
                }
                dc.text(
                    lx,
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
                let view_key = self.editors.active_view().map(str::to_string);
                match view_key
                    .as_ref()
                    .and_then(|k| self.ext_details.get(k).cloned())
                {
                    // ★ 뷰 탭(확장 상세) = 편집기 자리에 전용 페이지를 UI 글꼴로 그린다(글 편집기는 그리지 않는다).
                    Some(detail) => {
                        let vprefs = FontPrefs {
                            base: SlotFont {
                                size: ui_px,
                                bold: false,
                                italic: false,
                            },
                            message: SlotFont {
                                size: ui_px * 1.7,
                                bold: true,
                                italic: false,
                            },
                            ..FontPrefs::default()
                        };
                        let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s).with_fonts(vprefs);
                        if view_key.as_deref() != Some(self.ext_view_key.as_str()) {
                            self.ext_view_key = view_key.unwrap_or_default();
                            self.ext_view.reset();
                        }
                        self.ext_view.set_bounds(self.editors.editor_bounds(), s);
                        self.ext_view.paint(&mut dc, &th, &detail);
                    }
                    None => {
                        let mut dc = RasterCtx::new(&mut gfx, &self.mono_font, s)
                            .with_fonts(prefs)
                            .with_caret_on(caret_on);
                        self.editors.paint_bodies(&mut dc, &th);
                    }
                }
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
                // 결과 탭 줄이 바로 위면 그리드의 위 경계선을 끈다(탭 줄 아래선 1px만 · 사용자 09-22).
                let bar = self.panel.bar_visible();
                self.grid.set_top_border(!bar);
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
                self.project_panel.paint(&mut dc, &th);
                self.bm_panel.paint(&mut dc, &th);
                self.outline_panel.paint(&mut dc, &th);
                self.ext_panel.paint(&mut dc, &th);
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
                // 툴바 그룹 드래그 고스트 — 편집기 위까지 나가므로 팝업 층에.
                self.tool_dock.paint_drag_overlay(&mut dc, &th);
                self.find.paint_tooltip(&mut dc, &th);
                self.search.paint_tooltip(&mut dc, &th);
                self.project_panel.paint_tooltip(&mut dc, &th);
                self.project_panel.paint_popup(&mut dc, &th);
                self.bm_panel.paint_popup(&mut dc, &th);
                self.outline_panel.paint_popup(&mut dc, &th);
                self.ext_panel.paint_popup(&mut dc, &th);
                // ★ 팝업(상태줄 메뉴 · 결과 도구줄 툴팁/메뉴 · 팔레트)은 스플리터 **뒤**에 — 앞 층에서 그리면 편집기|결과
                //   구분선이 팝업 위로 지나갔다(09-16 캡처 · 팝업 = 맨 마지막 층 규칙).
                self.status_menu.paint(&mut dc, &th);
                // 자동 완성 팝업(캐럿 아래 · 팝업 층 · docs/76) + 상세 카드(옆 · 같은 높이 · 09-24).
                self.intel.menu.paint(&mut dc, &th);
                if self.intel.is_open() && self.intel.cfg().detail_card {
                    if let Some(target) = self.intel.card_target() {
                        let spec = self.sess.spec.clone();
                        let (names, snap) = self.explorer.meta_view(spec.as_ref());
                        if let Some(card) =
                            intel_card::build(&target, names, &snap, Some(self.sess.dialect))
                        {
                            intel_card::paint(
                                &mut dc,
                                &th,
                                self.intel.menu.bounds(),
                                &card,
                                self.intel.cfg(),
                                s,
                                (wi, hi),
                            );
                        }
                    }
                }
                // ★ 토스트·실행 카드는 팝업(우클릭 메뉴·팔레트) **아래 층**(팝업 = 맨 마지막 규칙 · 사용자 09-22 "우클릭 메뉴가 뒤로 숨음").
                // 토스트 = 편집기 영역의 우하단(결과 그리드를 가리지 않게 · 사용자 09-16 · docs/42).
                let eb = self.editors.editor_bounds();
                let (top, tx, ty) = if eb.w > 0 && eb.h > 0 {
                    (eb.y, eb.right(), eb.bottom())
                } else {
                    (0, wi, hi - px(24.0, s))
                };
                // 실행 카드 누적(아래→위 · 편집기 영역을 넘으면 휠 스크롤 · 사용자 09-22).
                let ty = self.run_toast.paint(&mut dc, &th, top, tx, ty, s);
                let ty = self.tx_warn.paint(&mut dc, &th, tx, ty, s);
                self.ext_banner.set_bounds(self.editors.banner_rect());
                self.ext_banner.paint(&mut dc, &th, s);
                self.toasts.paint(&mut dc, &th, tx, ty, s);
                // 토스트·카드가 바꾼 글꼴 슬롯(Status·굵게)을 되돌린다 — 팝업은 호출자의 글꼴을 쓴다(09-22 메뉴 글자 커짐).
                dc.select_font(FontSlot::Base, false);
                self.grid.paint_overlays(&mut dc, &th);
                self.panel.paint_popups(&mut dc, &th);
                self.editors.paint_popups(&mut dc, &th);
                self.explorer.paint_popups(&mut dc, &th);
                self.palette.paint(&mut dc, &th);
                let eb = self.editors.editor_bounds();
                // ★ 파일 적재 진행 막 — 편집 영역 가운데 · 맨 위 층(적재 중에는 입력도 받지 않는다).
                let delay =
                    Duration::from_millis(self.settings.int("file.load_progress_ms").max(0) as u64);
                let active_tab = self.editors.active_id();
                Self::paint_file_load(&mut self.file_loads, active_tab, delay, eb, &mut dc, &th, s);
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
            if let Some(t0) = self.input_at.take() {
                let path: Vec<String> = self
                    .trace_marks
                    .borrow_mut()
                    .drain(..)
                    .map(|(w, t)| format!("{w} +{:.2}", (t - t0).as_secs_f64() * 1000.0))
                    .collect();
                eprintln!(
                    "[frames] input→present {:.2}ms (paint {:.2}ms) · {}",
                    t0.elapsed().as_secs_f64() * 1000.0,
                    t_frame.elapsed().as_secs_f64() * 1000.0,
                    path.join(" · ")
                );
            }
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
                    // 수식키(⌘/Ctrl · Control)와 함께 누른 Space는 단축키 후보지 글자가 아니다 — 키맵에 없으면 버린다
                    //   (사용자 09-23 mac: ⌃Space가 표에 없어 공백이 들어갔다). Shift+Space는 글자 그대로.
                    Key::Named(NamedKey::Space) if self.primary || self.ctrl_raw => return None,
                    Key::Named(NamedKey::Space) => InputEvent::Char { c: ' ', now_ms: 0 },
                    Key::Character(t) if !self.primary => {
                        let c = t.chars().next()?;
                        if c.is_control() {
                            return None;
                        }
                        // Windows 탐색기 한글 모드: IME가 없어 라틴이 온다 → 두벌식 자모로(대문자 = 시프트 · 숫자·기호는 그대로).
                        let c =
                            if cfg!(windows) && self.focus == Focus::Explorer && self.hangul_mode {
                                nexa_ctl::hangul::jamo_from_qwerty(c, c.is_ascii_uppercase())
                                    .unwrap_or(c)
                            } else {
                                c
                            };
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
        // ■ = 막힌 상태를 푸는 유일한 버튼 — 실행 중이거나 보조 요청(전체 조회 등)이 진행 중일 때 켠다.
        let gate = self.gate();
        let stop = gate.stop;
        if stop != self.run_stop_enabled {
            self.run_stop_enabled = stop;
            let mut inv = Invalidations::default();
            self.tool_dock.set_item_enabled("run.stop", stop, &mut inv);
            self.redraw();
        }
        let on = gate.run_statement;
        if on != self.run_stmt_enabled {
            self.run_stmt_enabled = on;
            let mut inv = Invalidations::default();
            self.tool_dock
                .set_item_enabled("run.statement", on, &mut inv);
            self.tool_dock.set_item_tip(
                "run.statement",
                t(if on || !self.editors.cur().has_multi() {
                    Msg::TipRunStatement
                } else {
                    Msg::TipRunStatementMulti
                }),
            );
            self.redraw();
        }
    }

    /// 열린 팝업 메뉴의 비트 집합(배타 규칙의 입력 · 09-22): 1 풀다운 · 2 편집기 탭 메뉴 · 4 편집기 본문 편집 메뉴 · 8 상태줄/툴바 팝업 ·
    /// 16 오브젝트 탐색기 · 32 결과 그리드 · 64 결과 탭 줄. 팔레트·툴팁·하위 메뉴(같은 메뉴 안)는 메뉴가 아니다.
    fn open_menus(&self) -> u32 {
        let mut m = 0;
        if self.menubar.is_open() {
            m |= 1;
        }
        if self.editors.tab_menu_open() {
            m |= 2;
        }
        if self.editors.edit_menu_open() {
            m |= 4;
        }
        if self.status_menu.is_open() {
            m |= 8;
        }
        if self.explorer.menu_open() {
            m |= 16;
        }
        if self.intel.is_open() {
            m |= 1024;
        }
        if self.grid.menu_open() {
            m |= 32;
        }
        if self.panel.menu_open() {
            m |= 64;
        }
        if self.bm_panel.menu_open() {
            m |= 128;
        }
        if self.project_panel.menu_open() {
            m |= 256;
        }
        m
    }

    fn close_menu_bits(&mut self, bits: u32) {
        if bits & 1 != 0 {
            self.menubar.dismiss();
        }
        if bits & 2 != 0 {
            self.editors.close_tab_menu();
        }
        if bits & 4 != 0 {
            self.editors.close_edit_menus();
        }
        if bits & 8 != 0 {
            self.status_menu.close();
        }
        if bits & 16 != 0 {
            self.explorer.close_menu();
        }
        if bits & 32 != 0 {
            self.grid.close_menu();
        }
        if bits & 128 != 0 {
            self.bm_panel.close_menu();
        }
        if bits & 256 != 0 {
            self.project_panel.close_menu();
        }
        if bits & 64 != 0 {
            self.panel.close_menu();
        }
    }

    /// ★ 팝업 메뉴 배타 규칙(사용자 09-22 "다른 메뉴들도 배타적 배치가 기본"): 사건 전후로 열린 메뉴 집합을 비교해
    /// **새로 열린 메뉴가 있으면 나머지를 전부 닫는다**(마지막에 연 것이 이긴다). 같은 메뉴의 하위 메뉴·툴팁·팔레트는 대상이 아니다.
    fn route(&mut self, ev: InputEvent) {
        let before = self.open_menus();
        self.route_dispatch(ev);
        let after = self.open_menus();
        let fresh = after & !before;
        if fresh != 0 && after != fresh {
            self.close_menu_bits(after & !fresh);
            self.redraw();
        }
    }

    fn route_dispatch(&mut self, ev: InputEvent) {
        if self.frame_trace.is_some() {
            if let InputEvent::Key { .. } = ev {
                self.tmark("route");
            }
        }
        if let InputEvent::MouseMove { x, y } = ev {
            self.pointer = Some(Point { x, y });
        }
        if matches!(ev, InputEvent::MouseUp { .. }) && self.mem_pressed {
            self.mem_pressed = false;
            self.redraw();
        }
        self.route_inner(ev, Invalidations::default());
        self.sync_run_stmt_button();
        self.giant_notice();
        self.regions_cap_notice();
    }

    /// 막힌 거대 편집(docs/60 D-130)을 알린다 — 편집기가 막았다는 표시를 꺼내 상태줄 + 토스트로.
    /// 다중 선택 구간 상한에 걸렸으면 상태줄 안내(`editor.max_occurrences` · 줄 나누기·열 선택·Ctrl+클릭 포함 · docs/72).
    fn regions_cap_notice(&mut self) {
        if self.ed_mut().take_regions_capped() {
            let n = self.editors.selection_count();
            self.sess.status = tf(Msg::StOccurrenceCap, &[&n.to_string()]);
        }
    }

    fn giant_notice(&mut self) {
        let Some((bytes, repeat)) = self.ed_mut().take_giant_blocked() else {
            return;
        };
        let size = nsql_core::fmt_bytes(bytes as u64);
        let msg = tf(
            if repeat {
                Msg::StGiantEditRepeat
            } else {
                Msg::StGiantEditTyping
            },
            &[&size],
        );
        self.toasts.push(
            toast::ToastKind::Error,
            t(Msg::StGiantEditTitle),
            msg.clone(),
        );
        self.sess.status = msg;
        self.redraw();
    }

    fn route_inner(&mut self, ev: InputEvent, mut inv: Invalidations) {
        if let InputEvent::MouseDown { x, y, .. } = ev {
            if self.toasts.click(Point { x, y }) {
                if let Some(a) = self.toasts.take_action() {
                    self.menu_action(&a);
                }
                self.redraw();
                return;
            }
            match self.ext_banner.click(Point { x, y }) {
                extfile::BannerHit::None => {}
                hit => {
                    self.ext_banner_pick(hit);
                    self.redraw();
                    return;
                }
            }
            match self.tx_warn.click(Point { x, y }) {
                txwarn::TxWarnHit::None => {}
                hit => {
                    self.tx_warn_pick(hit);
                    self.redraw();
                    return;
                }
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
                runtoast::RunToastHit::Copy(sql) => {
                    // 실행 카드의 복사 버튼(사용자 09-23) — 결과 탭 "SQL 복사"와 같은 상태줄 문구.
                    if clipboard::write_text(&sql) {
                        self.sess.status =
                            tf(Msg::StResultSqlCopied, &[&sql.lines().count().to_string()]);
                    } else {
                        self.sess.status = t(Msg::ErrClipboard).into();
                    }
                    self.redraw();
                    return;
                }
                runtoast::RunToastHit::None => {}
            }
        }
        if let InputEvent::Wheel { delta } = ev {
            let p = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            if self.run_toast.wheel(p, delta) {
                self.redraw();
                return;
            }
        }
        if let InputEvent::MouseMove { x, y } = ev {
            if self.run_toast.hover(Point { x, y }) {
                self.redraw();
            }
            if self.tx_warn.hover(Point { x, y }) {
                self.redraw();
            }
            if self.ext_banner.hover(Point { x, y }) {
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
        // ★ 우클릭도 **포인터 사건**이다(사용자 09-21 "탐색기 우클릭 새로 고침이 안 된다"): `is_mouse`에는 우클릭이 없어서
        //   탐색기·툴바의 우클릭 분기가 처음부터 닿지 않는 코드였다(메뉴 코드는 있었지만 실제 입력으로는 열리지 않았다).
        //   우클릭 메뉴가 있는 영역은 `is_ptr`로 판정한다.
        let is_ptr = is_mouse || matches!(ev, InputEvent::RightDown { .. });
        // ★ 탐색기 우클릭 메뉴는 **창 위에** 뜬다(탐색기 폭에 갇히지 않는다 · 09-21) → 열려 있는 동안은 다른 영역(분할선·탭 바·
        //   편집기)보다 **먼저** 사건을 받는다. 메뉴 안 = 메뉴가 먹는다 · 키·휠 = 메뉴가 먹는다 · 바깥 클릭 = 메뉴를 닫고 그 클릭은
        //   그대로 아래로 흘려 보낸다(CLAUDE.md §3 팝업 규칙 — 다음 동작이 한 번의 입력으로 이어지게).
        if self.explorer.is_visible() && self.explorer.menu_open() {
            let at = match ev {
                InputEvent::MouseDown { x, y, .. }
                | InputEvent::RightDown { x, y }
                | InputEvent::MouseUp { x, y }
                | InputEvent::MouseMove { x, y } => Some(Point { x, y }),
                _ => None,
            };
            let inside = at.is_none_or(|p| self.explorer.menu_bounds().contains(p));
            if self.explorer.on_event(&ev) {
                self.redraw();
            }
            if self.explorer_actions() {
                self.redraw();
            }
            // ★ 바깥 **클릭**만 아래로 흘린다(메뉴를 닫고 그 클릭을 그대로 진행). 마우스 **이동**은 메뉴 밖이어도 여기서 끝낸다 —
            //   아래로 흘리면 다른 영역(열려 있던 탭 표식 메뉴 · 탭 툴팁)이 그 이동을 받아 포커스를 편집기로 옮기고, 탐색기는
            //   포커스를 잃으면서 메뉴를 닫는다(사용자 09-21 "항목으로 가는 사이 메뉴가 사라져 누를 수 없다").
            let outside_click = !inside
                && matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                );
            if !outside_click {
                return;
            }
        }
        // ★ MouseUp은 커서가 어디에 있든 **편집기에도** 전달한다 — 탭 바·탐색기 위에서 놓으면 편집기가 드래그 끝을
        //   못 받아 다음 MouseMove가 선택을 바꾸던 결함(사용자 09-15). 중복 전달은 무해(dragging=false 멱등).
        if matches!(ev, InputEvent::MouseUp { .. }) {
            self.ed_mut().on_event(&ev, &mut inv);
        }
        // ★ 결과 그리드 컬럼 이동 중(09-19): Esc = 취소(포커스와 무관하게).
        if self.grid.col_dragging()
            && matches!(
                ev,
                InputEvent::Key {
                    key: CtlKey::Escape,
                    ..
                }
            )
        {
            self.grid.cancel_col_drag();
            self.redraw();
            return;
        }
        // ★ 툴바 그룹 드래그 중(09-19): 마우스는 도크가 잡고 있고(고스트가 도크 밖까지 따라간다) · Esc = 취소.
        if self.tool_dock.is_dragging() {
            if matches!(
                ev,
                InputEvent::Key {
                    key: CtlKey::Escape,
                    ..
                }
            ) {
                self.tool_dock.cancel_drag(&mut inv);
                self.drain_dock_actions();
                self.redraw();
                return;
            }
            if is_mouse {
                self.tool_dock.on_event(&ev, &mut inv);
                self.drain_dock_actions();
                self.redraw();
                return;
            }
        }
        // 열린 명령 팔레트는 모달.
        if self.palette.is_open() {
            match self.palette.on_event(&ev, &mut inv) {
                PaletteAction::None => {}
                PaletteAction::Close => {
                    self.palette.close();
                    self.ime_refresh();
                }
                PaletteAction::Pick(id) => {
                    self.palette.close();
                    self.ime_refresh();
                    self.menu_action(&id);
                }
                PaletteAction::Prompt { id, text } => {
                    self.palette.close();
                    if let Some(i) = id.strip_prefix("tab.rename:").and_then(|n| n.parse().ok()) {
                        self.editors.rename_tab(i, &text);
                    } else if let Some(rid) = id
                        .strip_prefix("result.rename:")
                        .and_then(|n| n.parse::<u64>().ok())
                    {
                        // 그 사이 편집기 탭을 바꿨어도 결과 탭 id로 찾는다(지금 패널 → 잠든 패널).
                        if !self.panel.rename(rid, &text) {
                            for p in self.panels.values_mut() {
                                if p.rename(rid, &text) {
                                    break;
                                }
                            }
                        }
                    } else if id == "ext.repo_add" {
                        self.ext_repo_add(&text);
                    }
                }
            }
            self.redraw();
            return;
        }
        // ★ 자동 완성 팝업(docs/76): ↑↓/Enter/Tab/Esc/휠/클릭은 팝업이 먼저 · 글자·Backspace는 편집기로 가서 다시 거른다 ·
        //   바깥 클릭은 닫고 통과 · 다른 탭이면 닫는다.
        if self.intel.is_open() {
            let cur_tab = self.editors.tab_id(self.editors.active());
            if self.intel.tab() != Some(cur_tab) || self.focus != Focus::Editor {
                self.intel.close();
            } else {
                let outside = self.intel.menu.is_outside_click(&ev);
                // Tab은 `Char('\t')`로 온다 — 팝업이 열려 있으면 Enter와 같다(확정).
                let is_tab = matches!(ev, InputEvent::Char { c: '\t', .. });
                // ←/→는 라벨이 넘쳐 가로 스크롤이 있을 때만 팝업이(아니면 편집기로 · 09-24).
                let lr = self.intel.menu.label_overflow() > 0
                    && matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Left | CtlKey::Right,
                            ..
                        }
                    );
                let nav = is_tab
                    || lr
                    || matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Up
                                | CtlKey::Down
                                | CtlKey::Enter
                                | CtlKey::Escape
                                | CtlKey::PageUp
                                | CtlKey::PageDown
                                | CtlKey::Home
                                | CtlKey::End,
                            ..
                        }
                    );
                // ★ 키 관통(`intel.key_passthrough` · 사용자 09-24 "Enter를 눌러도 창만 사라진다"): 고른 항목이 없으면 Enter/Tab은
                //   팝업만 닫고 **그 키는 편집기로 그대로** 간다(줄 바꿈·탭 입력) · 끄면 종전처럼 팝업이 삼킨다(두 번 입력).
                let accept_key = is_tab
                    || matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Enter,
                            ..
                        }
                    );
                if nav
                    && accept_key
                    && self.intel.menu.hovered().is_none()
                    && self.intel.cfg().key_passthrough
                {
                    self.intel.close();
                } else if nav {
                    let ev2 = if is_tab {
                        InputEvent::Key {
                            key: CtlKey::Enter,
                            shift: false,
                            primary: false,
                        }
                    } else {
                        ev
                    };
                    // End = 남은 페이지 전부 붙인 뒤 마지막으로(76 §13 D-206).
                    if matches!(
                        ev2,
                        InputEvent::Key {
                            key: CtlKey::End,
                            ..
                        }
                    ) {
                        self.intel.extend_all();
                    }
                    self.intel.menu.on_event(&ev2);
                    // 끝에 닿았으면 다음 페이지(T-196).
                    if self.intel.menu.take_reached_end() {
                        self.intel.extend();
                    }
                    if let Some(id) = self.intel.menu.take_picked() {
                        // Alt를 누른 채 확정 = 한정자 규칙 반대(`A.컬럼` ↔ 컬럼만 · 09-24).
                        self.intel.pick_with(&id, self.alt);
                        self.intel_apply();
                    } else if let Some(full) = self.intel.hovered_full() {
                        // 강조 행의 전체 이름을 상태줄에(폭 상한으로 가운데 …가 된 긴 이름 · 09-23).
                        self.sess.status = full;
                        self.intel_card_settle();
                    }
                    if matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Escape,
                            ..
                        }
                    ) {
                        self.intel.close();
                    }
                    self.redraw();
                    return;
                }
                // ★ 팝업 밖 휠(사용자 09-24 "영역 밖 스크롤이 안으로 전달") = 팝업을 닫고 편집기로 흘린다(팝업이 본문과 어긋난 채 남지 않게).
                if is_wheel_ev(&ev)
                    && self
                        .pointer
                        .is_some_and(|p| !self.intel.menu.bounds().contains(p))
                {
                    self.intel.close();
                    self.redraw();
                } else if is_mouse || is_wheel_ev(&ev) {
                    let consumed = self.intel.menu.on_event(&ev);
                    if self.intel.menu.take_reached_end() {
                        self.intel.extend();
                    }
                    if let Some(id) = self.intel.menu.take_picked() {
                        // Alt를 누른 채 확정 = 한정자 규칙 반대(`A.컬럼` ↔ 컬럼만 · 09-24).
                        self.intel.pick_with(&id, self.alt);
                        self.intel_apply();
                        self.redraw();
                        return;
                    }
                    if let Some(full) = self.intel.hovered_full() {
                        self.sess.status = full;
                        self.intel_card_settle();
                    }
                    if outside {
                        self.intel.close();
                    } else if consumed || !matches!(ev, InputEvent::MouseMove { .. }) {
                        self.redraw();
                        return;
                    }
                }
            }
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
            if self.status_mem_rect.contains(Point { x, y }) {
                self.mem_pressed = true;
                self.toggle_mem_window();
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
        // 열린 메뉴는 모달 — 어디를 눌러도 메뉴바가 먼저 받는다. 단 **우클릭**은 풀다운을 닫고 그대로 진행(그 자리의 우클릭 메뉴가 열린다 · 배타).
        if self.menubar.is_open() && !matches!(ev, InputEvent::RightDown { .. }) {
            self.menubar.on_event(&ev, &mut inv);
            if let Some(id) = self.menubar.take_picked() {
                self.menu_action(&id);
            }
            self.redraw();
            return;
        }
        if matches!(ev, InputEvent::RightDown { .. }) && self.menubar.is_open() {
            // 우클릭 메뉴가 열리는 길 = 풀다운은 닫는다(배타).
            self.menubar.dismiss();
        }
        if let InputEvent::RightDown { x, y } = ev {
            if self.tool_dock.bounds().contains(Point { x, y }) {
                self.open_toolbar_menu(x, y);
                self.redraw();
                return;
            }
            // ★ 편집기 거터(북마크/니모닉 영역 + 줄번호) 우클릭 = 북마크 메뉴(사용자 09-23): 그 줄에 캐럿을 두고 상태에 맞춰
            //   "추가"(없을 때) / "제거"·"니모닉 해제"(있을 때) · "니모닉 지정 ▸ 1~9"(늘). 본문 우클릭(편집 메뉴)은 그대로.
            if self.open_bm_gutter_menu(Point { x, y }) {
                self.redraw();
                return;
            }
        }
        if is_mouse {
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
                // ★ 풀다운이 열리면 다른 팝업(탭·편집·상태줄·탐색기·그리드·결과 메뉴)은 닫는다 — 동시 표시 금지(사용자 09-22).
                self.close_context_menus();
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
            // 항목을 골랐거나 메뉴 안을 눌렀으면 그 클릭은 끝(아래 셀 선택으로 전파 금지 · 사용자 09-15) · 바깥 **좌/우** 클릭만 통과
            //   (우클릭도 — 09-22: 그리드 메뉴가 열린 채 편집기/탭을 우클릭하면 닫히기만 했다).
            if self.grid.menu_open()
                || self.grid.take_menu_click()
                || !matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                )
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
        // 확장 패널 — 마우스는 커서 아래 · 키는 포커스일 때(마우스 라우팅 규칙).
        if self.ext_panel.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let inside = self.ext_panel.bounds().contains(cur);
            if (is_mouse && inside) || (is_wheel_ev(&ev) && inside) {
                if matches!(ev, InputEvent::MouseDown { .. }) {
                    self.set_focus(Focus::Ext);
                }
                if self.ext_panel.on_event(&ev) {
                    self.redraw();
                }
                self.ext_panel_actions();
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return;
                }
            } else if self.focus == Focus::Ext
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
                ) && !self.ext_panel.has_query()
                {
                    self.set_focus(Focus::Editor);
                    self.redraw();
                    return;
                }
                if self.ext_panel.on_event(&ev) {
                    self.redraw();
                }
                return;
            }
        }
        // 북마크 패널의 우클릭 메뉴는 창 위에 뜬다 → 열린 동안 먼저 받는다(탐색기 메뉴와 같은 규칙 · 바깥 클릭은 닫고 통과).
        if self.bm_panel.is_visible() && self.bm_panel.menu_open() {
            let outside_click = matches!(
                ev,
                InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
            ) && !self.bm_panel.bounds().contains(Point {
                x: self.cursor.0,
                y: self.cursor.1,
            });
            if self.bm_panel.on_event(&ev) {
                self.redraw();
            }
            self.bm_pump();
            if (!outside_click || self.bm_panel.menu_open())
                && !matches!(ev, InputEvent::MouseMove { .. })
            {
                return;
            }
        }
        // 북마크 패널(docs/69 §6) — 프로젝트 탐색기와 같은 규칙.
        if self.bm_panel.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let inside = self.bm_panel.bounds().contains(cur);
            // 우클릭(메뉴)도 포인터 사건 — `is_ptr`(탐색기와 같은 판정 · 09-22 S35: `is_mouse`만 보면 RightDown이 안 닿는다).
            if (is_ptr && inside) || (is_wheel_ev(&ev) && inside) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) {
                    self.set_focus(Focus::Bookmarks);
                }
                if self.bm_panel.on_event(&ev) {
                    self.redraw();
                }
                self.bm_pump();
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return;
                }
            } else if self.focus == Focus::Bookmarks
                && matches!(
                    ev,
                    InputEvent::Key { .. }
                        | InputEvent::Char { .. }
                        | InputEvent::SelectAll
                        | InputEvent::Undo
                        | InputEvent::Redo
                )
            {
                let handled = self.bm_panel.on_event(&ev);
                if !handled
                    && matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Escape,
                            ..
                        }
                    )
                {
                    self.set_focus(Focus::Editor);
                    self.redraw();
                    return;
                }
                if handled {
                    self.redraw();
                }
                self.bm_pump();
                return;
            }
        }
        // 아웃라인 패널(docs/76) — 북마크 패널과 같은 규칙(포인터는 안일 때 · 키는 포커스일 때 · 열기 요청은 pump).
        if self.outline_panel.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let inside = self.outline_panel.bounds().contains(cur);
            if (is_ptr && inside) || (is_wheel_ev(&ev) && inside) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) {
                    self.set_focus(Focus::Outline);
                }
                if self.outline_panel.on_event(&ev) {
                    self.redraw();
                }
                self.outline_pump();
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return;
                }
            } else if self.focus == Focus::Outline
                && matches!(
                    ev,
                    InputEvent::Key { .. }
                        | InputEvent::Char { .. }
                        | InputEvent::SelectAll
                        | InputEvent::Undo
                        | InputEvent::Redo
                )
            {
                let handled = self.outline_panel.on_event(&ev);
                if !handled
                    && matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Escape,
                            ..
                        }
                    )
                {
                    self.set_focus(Focus::Editor);
                    self.redraw();
                    return;
                }
                if handled {
                    self.redraw();
                }
                self.outline_pump();
                return;
            }
        }
        // 프로젝트 탐색기의 우클릭 메뉴는 창 위에 뜬다 → 열린 동안 먼저 받는다(북마크 패널과 같은 규칙 · 바깥 클릭은 닫고 통과).
        if self.project_panel.is_visible() && self.project_panel.menu_open() {
            let outside_click = matches!(
                ev,
                InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
            ) && !self.project_panel.bounds().contains(Point {
                x: self.cursor.0,
                y: self.cursor.1,
            });
            if self.project_panel.on_event(&ev) {
                self.redraw();
            }
            self.project_pump();
            if (!outside_click || self.project_panel.menu_open())
                && !matches!(ev, InputEvent::MouseMove { .. })
            {
                return;
            }
        }
        // 프로젝트 탐색기(docs/67 §4) — 마우스는 커서 아래 · 키는 포커스일 때.
        if self.project_panel.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let inside = self.project_panel.bounds().contains(cur);
            if (is_ptr && inside) || (is_wheel_ev(&ev) && inside) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) {
                    self.set_focus(Focus::Project);
                }
                if self.project_panel.on_event(&ev) {
                    self.redraw();
                }
                self.project_pump();
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return;
                }
            } else if self.focus == Focus::Project
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
                ) {
                    self.set_focus(Focus::Editor);
                    self.redraw();
                    return;
                }
                if self.project_panel.on_event(&ev) {
                    self.redraw();
                }
                self.project_pump();
                return;
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
            if self.explorer.menu_open() || (is_ptr && in_exp) || (is_wheel_ev(&ev) && in_exp) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) && !self.explorer.menu_open()
                {
                    // 한 창에 열린 메뉴는 하나 — 탭 표식 메뉴를 열어 둔 채 탐색기에서 우클릭하면 두 메뉴가 함께 떠 있었다.
                    if self.editors.tab_menu_open() {
                        self.editors.close_tab_menu();
                        self.redraw();
                    }
                    self.set_focus(Focus::Explorer);
                }
                if self.explorer.on_event(&ev) {
                    self.redraw();
                }
                if self.explorer_actions() {
                    self.redraw();
                }
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return;
                }
            } else if self.focus == Focus::Explorer
                && matches!(ev, InputEvent::Key { .. } | InputEvent::Char { .. })
            {
                if self.explorer.on_event(&ev) {
                    self.redraw();
                }
                if self.explorer_actions() {
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
            // 포커스는 **누를 때만** 옮긴다 — 이동(hover·툴팁)으로 옮기면 다른 영역의 포커스에 딸린 것(탐색기 메뉴 · 타입어헤드)이 꺼진다.
            if !matches!(ev, InputEvent::MouseMove { .. }) {
                self.set_focus(Focus::Editor);
            }
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
        // ★ 뷰 탭(확장 상세): 편집기 자리의 마우스·휠은 뷰가 받고, 편집기 포커스의 글자·키 입력은 버린다(편집 대상이 아니다).
        if self.editors.active_view().is_some() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let in_view = self.editors.editor_bounds().contains(cur);
            let pointer =
                is_mouse || matches!(ev, InputEvent::Wheel { .. } | InputEvent::HWheel { .. });
            if pointer && in_view {
                if matches!(ev, InputEvent::MouseDown { .. }) {
                    self.set_focus(Focus::Editor);
                }
                if self.ext_view.on_event(&ev) {
                    self.redraw();
                }
                self.ext_view_actions();
                return;
            }
            if !pointer && self.focus == Focus::Editor {
                return;
            }
        }
        // ★ 좌클릭·우클릭 모두 커서 아래 컨트롤에 포커스(마우스 라우팅 규칙 · CLAUDE.md §3) — 우클릭이 빠져 있어
        //   편집기에 포커스가 있으면 그리드 우클릭이 편집기로 가서 메뉴가 안 떴다(사용자 09-16 · 좌클릭 뒤에야 동작).
        if let InputEvent::MouseDown { x, y, .. } | InputEvent::RightDown { x, y } = ev {
            let p = Point { x, y };
            if self.editors.editor_bounds().contains(p) {
                self.set_focus(Focus::Editor);
                // 동시 편집: 누른 칸의 탭이 활성(키·실행 대상)이 된다.
                if self.editors.activate_pane_at(p) {
                    self.sync_gate();
                }
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
                self.editors.box_at_or_cur_mut(cur).on_event(&ev, &mut inv);
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
            self.editors.box_at_or_cur_mut(cur).on_event(&ev, &mut inv);
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
                Focus::Editor => {
                    self.ed_mut().on_event(&ev, &mut inv);
                    self.tmark("editor on_event");
                    self.intel_after_event(&ev);
                }
                Focus::Grid => {
                    self.grid.on_event(&ev, self.scale);
                    self.after_grid_event();
                    inv.push(self.grid.bounds);
                }
                Focus::Explorer
                | Focus::Find
                | Focus::Search
                | Focus::Ext
                | Focus::Project
                | Focus::Bookmarks
                | Focus::Outline => {}
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
        // 메인 창 규칙(사용자 09-17): **마지막 위치에서 종료 시점 크기로** 시작 · `window.monitor`(1-기준 · 왼쪽→오른쪽)가
        //   있으면 그 모니터 왼쪽 위 안쪽. 모니터 목록은 창을 만든 뒤 창 핸들로 얻는다(`el.available_monitors()`는 macOS
        //   `resumed` 시점에 비어 있었다 · 09-17) → 위치는 첫 창 이벤트 때 적용(직후 호출은 캐스케이드 배치가 덮는다).
        // ★ 숨긴 채 만들고 → 위치를 정한 뒤 → 보인다: 보이는 창을 다른 배율 모니터로 옮기면 winit(macOS)이 생성 시점 배율
        //   (2x)을 유지해 1x 모니터에서 반 크기로 그려졌다(09-17 ASCII 캡처). 숨긴 창은 보이는 순간 그 모니터 배율을 받는다.
        let attrs = icon::with_icon(
            Window::default_attributes()
                .with_title("Nexa SQL")
                .with_visible(false)
                .with_theme(theme::window_theme(self.settings.theme_mode()))
                .with_inner_size({
                    // 마지막으로 닫힌 크기(`window.main_size` · 사용자 09-17) · 없으면 기본.
                    let (w, h) = self
                        .settings
                        .get("window.main_size")
                        .and_then(wingeom::parse_size)
                        .unwrap_or((1375.0, 945.0));
                    winit::dpi::LogicalSize::new(w, h)
                }),
        );
        // ★ 기동 구간 계측 2부(09-24 mac 전수 · 창까지 592 ms 가운데 `App` 뒤 320 ms가 어디인가): resumed 안의 단계별 누적 ms.
        let boot = self.frame_trace.as_ref().map(|f| f.boot);
        let mut rmarks: Vec<(&str, u128)> = Vec::new();
        let rmark = |what: &'static str, v: &mut Vec<(&str, u128)>| {
            if let Some(b) = boot {
                v.push((what, b.elapsed().as_millis()));
            }
        };
        rmark("resumed", &mut rmarks);
        let Ok(win) = el.create_window(attrs) else {
            eprintln!("{}", t(Msg::ErrNoWindow));
            el.exit();
            return;
        };
        let win = Rc::new(win);
        {
            let mut mons: Vec<_> = win.available_monitors().collect();
            mons.sort_by_key(|m| (m.position().x, m.position().y));
            if std::env::var_os("NSQL_TRACE_WINDOW").is_some() {
                for (i, m) in mons.iter().enumerate() {
                    eprintln!(
                        "monitor {}: pos={:?} size={:?} scale={} name={:?}",
                        i + 1,
                        m.position(),
                        m.size(),
                        m.scale_factor(),
                        m.name()
                    );
                }
            }
            let mon = self.settings.int("window.monitor");
            // 논리 좌표: 모니터 위치는 그 모니터 배율로 나눈다(2x 메인 + 1x 보조에서 물리 px는 섞인다 · 09-17).
            let place = if mon > 0 {
                mons.get((mon - 1) as usize).map(|m| {
                    let r = wingeom::monitor_rect(m);
                    (r.0 + 20, r.1 + 30)
                })
            } else {
                self.settings
                    .get("window.main_pos")
                    .and_then(wingeom::parse_pos)
                    .filter(|p| wingeom::on_any_monitor(*p, mons.iter().cloned()))
            };
            if let Some((x, y)) = place {
                win.set_outer_position(wingeom::logical(x, y));
            }
            // 화면 밖으로 나가지 않게(OS가 계단식으로 놓은 기본 위치 · 해상도가 바뀐 뒤의 기억 위치 — 09-21 점검: 1080 높이에서 아래 60px이 잘렸다).
            wingeom::keep_on_screen(&win, None);
            win.set_visible(true);
            rmark("visible", &mut rmarks);
        }
        self.scale = win.scale_factor() as f32;
        // 창이 생기면 OS 판정(winit)이 정확해진다 — System 모드는 여기서 확정.
        self.theme = theme::resolve(self.settings.theme_mode(), win.theme());
        self.apply_tab_line_colors();
        rmark("window", &mut rmarks);
        match present::Presenter::new(win.clone()) {
            Ok(p) => {
                if self.frame_trace.is_some() {
                    eprintln!("[frames] present backend = {}", p.backend());
                }
                self.surface = Some(p);
            }
            Err(e) => eprintln!("{e}"),
        }
        let near = win
            .outer_position()
            .ok()
            .map(|p| (p.x, p.y, win.outer_size().width));
        rmark("present", &mut rmarks);
        if !rmarks.is_empty() {
            let line: Vec<String> = rmarks.iter().map(|(w, ms)| format!("{w} {ms}")).collect();
            eprintln!("[startup:resumed] {} (ms since main)", line.join(" · "));
        }
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
        // 한글 조합 방식(T-139): 입력 소스 바뀜 알림을 구독하고 지금 상태로 맞춘다.
        nexa_sys::input_source::watch();
        self.sync_hangul_mode();
        // 툴바 배치 복원(설정 `toolbar.layout` · 플로팅 창은 about_to_wait에서 생성).
        self.apply_tool_layout_setting();
        // 데모(사용자 09-17): 'Demo' 프로필·파일이 있으면 메뉴 비활성 · 없고 아직 안 물었으면 최초 1회 팝업.
        self.demo_ready = Self::demo_exists();
        // 오래된 미저장 스냅숏 정리(docs/70 §5 · `project.backup_days`).
        let pruned = backups::prune(self.settings.int("project.backup_days").max(1) as u64);
        if pruned > 0 {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                tf(Msg::LogBackupPruned, &[&pruned.to_string()]),
            ));
        }
        self.project_startup();
        // ★ 작업 모드 셋(사용자 09-23 · [project::WorkMode]): 파일 모드 = 전역(`%APPDATA%`) · **폴더 모드**(`nexa-sql .` · `nexa-sql <폴더>`) =
        //   `<폴더>/.nsql/` · 프로젝트 모드 = 프로젝트 파일. 지금 폴더 모드가 나누는 것은 북마크뿐 — 설정·다른 기능도 `WorkMode::local_dir`
        //   한 자리에서 나누도록 설계만(67 §6).
        if let Some(dir) = self.arg_folder.clone() {
            self.bookmarks.set_folder(Some(dir.clone()));
            if self.folder_start.is_none() {
                self.folder_start = Some(dir);
            }
        }
        self.bookmarks.bind_project(self.project.path.as_deref());
        self.bm_sync_ui();
        self.project_restore();
        self.rebuild_menus();
        if !self.demo_ready && !self.settings.flag("demo.prompted") {
            let _ = self.settings.set("demo.prompted", "on");
            self.persist_settings();
            self.pending_demo_prompt = true;
        } // 자체 캡처용 기동 명령(`NSQL_STARTUP_CMD=open:<파일>,view.extensions,…` · 쉼표 구분): 키 주입(SendKeys) 없이 특정 화면을
          //   띄워 PrintWindow로 확인하려는 것(사용자가 쓰는 중에 키를 쏘면 다른 창으로 간다 · 09-19 사고). 평소엔 변수 없음 = 비용 0.
          // 인자로 받은 파일(프로젝트 파일 제외) = 파일 모드로 연다(사용자 09-22).
        for f in std::mem::take(&mut self.arg_files) {
            self.open_file(&f);
        }
        if let Ok(cmds) = std::env::var("NSQL_STARTUP_CMD") {
            for id in cmds.split(',').map(str::trim).filter(|c| !c.is_empty()) {
                // `@connected:<명령>` = 첫 접속이 된 뒤에 실행(시작 인자로 접속하는 프로필 + 실행 시험 · 메모리 측정).
                // `@after:<ms>:<명령>` = 기동 뒤 그 시간이 지나면 실행(닫기 전후 메모리 비교 같은 시차 시험).
                if let Some(rest) = id.strip_prefix("@after:") {
                    if let Some((ms, cmd)) = rest.split_once(':') {
                        let at = Instant::now() + Duration::from_millis(ms.parse().unwrap_or(0));
                        self.startup_timed.push((at, cmd.to_string()));
                    }
                    continue;
                }
                match id.strip_prefix("@connected:") {
                    Some(later) => self.startup_after_connect.push(later.to_string()),
                    None => self.startup_cmd(id),
                }
            }
        }
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, _ev: Wake) {
        self.drain_all();
    }

    /// 이벤트 루프가 끝난다 = 프로그램 종료: 들고 있던 임시 비밀번호(세션 자격 금고)의 봉투와 키를 덮어써 버린다.
    fn exiting(&mut self, _el: &ActiveEventLoop) {
        self.pw_once = None;
        nsql_vault::session::shutdown();
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.exit_requested {
            self.finish_exit(el);
            return;
        }
        // 입력 소스가 바뀌었다(한/영 · 다른 입력기 — macOS 분산 알림) → 한글 조합 방식을 다시 맞춘다.
        if nexa_sys::input_source::take_changed() {
            self.sync_hangul_mode();
        }
        self.persist_window_sizes(false);
        // 기동 명령·타이머가 부탁한 변수 창(창 이벤트가 없어도 열리게).
        // 창 열기 깃발은 이벤트가 없을 때도 본다(메뉴·기동 명령이 부탁한 창 — `window_event` 끝에서만 보면 늦게 열린다 · docs/61 §6 흠 ⑤).
        if std::mem::take(&mut self.open_txlog) {
            self.open_txlog_window(el);
        }
        if std::mem::take(&mut self.open_sessions) {
            self.open_sessions_window(el);
        }
        if std::mem::take(&mut self.open_mem) {
            self.open_mem_window(el);
        }
        if std::mem::take(&mut self.open_vars) {
            self.open_vars_window(el);
        }
        // 파일·폴더 대화상자도 같다(설정 창의 "찾아보기…" · 기동 명령 — 메인 창에 사건이 없으면 열리지 않았다).
        if let Some(mode) = self.open_file_dlg.take() {
            self.open_file_window(el, mode);
            self.sync_modal();
        }
        // Generate SQL 결과 → SQL Preview 모달(docs/83 §4).
        if let Some((spec, r, server)) = self.sqlprev_pending.take() {
            let owner = self.window.clone();
            let syntax = self.editors.syntax_for_title("preview.sql");
            let tb = self.editors.preview_box("", &syntax);
            let was_open = self.sqlprev_win.is_open();
            self.sqlprev_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                owner.as_deref(),
                spec,
                server,
                r,
                tb,
            );
            // ★ 맥 자식 창(비밀번호 창과 같은 길 · 사용자 09-25 "DDL 팝업이 메인 뒤로 숨는다"): 메인의 자식으로 붙여 늘 위에.
            if !was_open {
                if let (Some(o), Some(c)) = (owner.as_deref(), self.sqlprev_win.window()) {
                    winfocus::attach_child(o, c);
                }
            }
            self.sync_modal();
        }
        // 워커가 실행 전에 값을 묻는다(D-137) → 입력 창.
        if let Some((sid, needs)) = self.input_pending.take() {
            let owner = self.window.clone();
            self.input_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                owner.as_deref(),
                needs,
                sid,
            );
        }
        // 저장하지 않은 탭을 닫으려 했다(X · 단축키 · 탭 메뉴 · 모두 닫기 — 어느 길이든 여기서 걷는다).
        if let Some(i) = self.editors.take_save_close_request() {
            self.ask_save_close(i);
        }
        // 워커가 비밀번호를 묻는다 → 같은 입력 창의 비밀번호 모드(다른 물음이 떠 있으면 그 뒤에).
        if self.pw_pending.is_some() && !self.input_win.is_open() {
            if let Some((sid, target, rejected)) = self.pw_pending.take() {
                let owner = self.window.clone();
                self.input_win.open_password(
                    el,
                    theme::window_theme(self.settings.theme_mode()),
                    owner.as_deref(),
                    input_win::PasswordAsk {
                        target,
                        rejected,
                        remember: self.settings.flag("connect.remember_session_password"),
                    },
                    sid,
                );
                // ★ 최상위 모달(사용자 mac 09-21): 맥은 자식 창(항상 메인 위 · 함께 이동) · 메인·보조 창 입력은 가드가 막고
                //   Windows는 `EnableWindow(FALSE)`(`sync_modal`). 접속 창과 같은 길.
                if let (Some(o), Some(c)) = (owner.as_deref(), self.input_win.window()) {
                    winfocus::attach_child(o, c);
                }
                self.sync_modal();
            }
        }
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
            // ★ 메인 창이 키 창이 아니면 깜빡이지 않는다(캐럿은 켜진 채 · 09-24 맥 전수: 깜빡임 = 전체 프레임 다시 그리기 ×2/초 ·
            //   맥 softbuffer present 36 ms/프레임이라 뒤에 있어도 유휴 CPU 19 % · footprint +20 MB였다 → 0).
            //   맥은 창이 키 창이어도 **앱이 비활성**(뒤에 있음)이면 깜빡이지 않는다(`NSApplication.isActive`).
            let app_active = nexa_sys::layer_present::app_active().unwrap_or(true);
            if self.focus == Focus::Editor && self.main_active && app_active {
                self.redraw();
            }
        }
        // 오버레이 스크롤바 페이드(편집기·그리드·로그 창) — 보이는 동안만 ≈30ms 타이머.
        let now_ms = self.started.elapsed().as_millis() as u64;
        self.tx_tick();
        self.idle_tick(now);
        // ★ 유휴 인덱스 선적재(docs/84 §7) — 칸마다 간격 판정은 안에서.
        self.explorer.prefetch_tick(now);
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
            self.sess.status = t(m).into();
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
        if self.project_panel.tick(now_ms) {
            self.redraw();
        }
        // 프로젝트 필터 열거의 실패 폴더 = 로그 창(도착 순 병합 · 사용자 09-23).
        for (path, why) in self.project_panel.take_scan_log() {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!("project filter: skipped {}: {why}", path.display()),
            ));
        }
        if self.bm_panel.tick(now_ms) {
            self.redraw();
        }
        if self.outline_panel.is_visible() && self.outline_panel.tick(now_ms) {
            self.redraw();
        }
        self.project_sync_active();
        self.project_autosave_tick();
        self.sync_open_files();
        self.bm_tick();
        self.project_pump();
        if self.editors.poll_preview() {
            self.redraw();
        }
        if self.search.take_request() {
            self.start_search();
        }
        if let Some(req) = self.search.take_open() {
            self.open_search_result(req);
        }
        if self.ext_panel.tick(now_ms) {
            self.redraw();
        }
        self.ext_fetch_poll();
        if self.find.tick(now_ms) {
            self.redraw();
        }
        if self.conn_win.tick_bars(now_ms) {
            self.conn_win.redraw();
        }
        let bars_live = self.ed_mut().scrollbars_visible()
            // 드래그 선택 중 포인터가 편집기 위/아래 밖에 멈춰 있다 → 틱이 자동 스크롤을 이어 간다(T-158 · 그동안만).
            || self.ed_mut().drag_autoscroll_active()
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
            || self.project_panel.animating()
            || self.bm_panel.animating()
            || self.ext_fetch_rx.is_some()
            || !self.file_loads.is_empty()
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
        // 입력 창의 IME 안내 만료.
        if let Some(t) = self.input_win.tick(now) {
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
        // 막힘 감지(docs/56 L3) — 미커밋 세션이 있을 때만 `tx.block_poll_secs` 간격으로 메타 세션에 한 문장.
        if let Some(t) = self.tx_block_tick(now) {
            next = next.min(t);
        }
        // 자체 캡처용 지연 기동 명령 — 접속이 끝나 세션이 한가해진 뒤 한 번.
        if self.startup_connected && !self.startup_after_connect.is_empty() && !self.sess.blocked()
        {
            for id in std::mem::take(&mut self.startup_after_connect) {
                self.startup_cmd(&id);
            }
        }
        // 쓰이지 못한 일회성 비밀번호는 20초 뒤에 버린다(버려지면서 0으로 덮어쓴다).
        if let Some((_, _, at)) = &self.pw_once {
            let end = *at + Duration::from_secs(20);
            if end <= now {
                self.pw_once = None;
            } else {
                next = next.min(end);
            }
        }
        if !self.startup_timed.is_empty() {
            let due: Vec<String> = self
                .startup_timed
                .iter()
                .filter(|(at, _)| *at <= now)
                .map(|(_, c)| c.clone())
                .collect();
            self.startup_timed.retain(|(at, _)| *at > now);
            for id in due {
                self.startup_cmd(&id);
            }
            if let Some(t) = self.startup_timed.iter().map(|(at, _)| *at).min() {
                next = next.min(t);
            }
        }
        self.file_loads_poll();
        self.multi_load_poll();
        // 명령·IME로 온 편집이 거대 편집 확인에 막혔으면 알린다(키 입력은 `route`가 바로 알린다).
        self.giant_notice();
        self.regions_cap_notice();
        // 메모리 회수 — 큰 것을 놓은 직후 1회 + 유휴 주기(`memtrim.rs`).
        if let Some(t) = self.mem_tick(now) {
            next = next.min(t);
        }
        // 외부 파일 변경(docs/58) — 사건 수거 + 활성 창에서 보이는 탭 폴링.
        if let Some(t) = self.ext_tick(now) {
            next = next.min(t);
        }
        // 탐색기 유휴 워터마크(docs/57 T2) — `meta.refresh_secs` 간격(기본 300초 · 0 = 끔 · 유휴일 때만 1행 질의).
        if let Some(t) = self.meta_refresh_tick(now) {
            next = next.min(t);
        }
        // 유휴 미커밋 점검(docs/56 L2) — 미커밋 세션이 있을 때만 5초(카운트다운 중 1초) 간격으로 깬다.
        if let Some(t) = self.tx_guard_tick(now) {
            next = next.min(t);
        }
        // 🔧 북마크 디바운스 저장 · 프로젝트 자동 저장의 **깨움**(사용자 09-23 "값이 바뀌어도 저장 안 됨"): 종전에는 다른 사건이
        //   루프를 깨울 때만 `tick_save`/`project_autosave_tick`이 돌아 앱이 가만히 있으면 저장이 미뤄졌다 → 마감 시각에 스스로 깬다.
        if let Some(t) = self.bookmarks.next_save_at(now) {
            next = next.min(t);
        }
        if let Some(t) = self.project_autosave_next(now) {
            next = next.min(t);
        }
        // 자동 완성 디바운스(docs/76 · `intel.delay_ms`) — 마감이 지나면 요청 · 아니면 그 시각에 깬다.
        if self.intel.due(now) {
            self.intel_request(false);
        }
        // 아웃라인 패널 = 활성 탭·본문 세대가 바뀌었을 때만 다시(캐시 · 유휴 틱 · D-203).
        self.outline_sync();
        // ★ 미사용 판정 즉시 회수(사용자 09-23): `스키마.`로 읽어 온 메타 버킷은 열린 문서 어디에도 그 이름이 없으면 바로 버린다
        //   (문서 낱말 캐시로 판정 · 현재 스키마·사전·트리가 펼친 것은 대상 아님).
        {
            let intel = &self.intel;
            let n = self
                .explorer
                .reclaim_intel_buckets(&|s| intel.doc_mentions(s));
            if n > 0 && self.settings.flag("log.dev_mode") {
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    format!("[intel] reclaimed {n} unused schema bucket(s)"),
                ));
            }
        }
        // 상세 카드 머무름 마감(사용자 09-24): 머문 마지막 대상을 카드에 넘기고 그때만 선조회·다시 그리기.
        if self.intel.card_tick(now) {
            self.intel_card_prefetch();
            self.redraw();
        }
        if let Some(t) = self.intel.next_wake() {
            next = next.min(t);
        }
        // ★ 메모리 맵 창(docs/80): 열려 있을 때만 `mem.refresh_ms`마다 표본 → 창·상태줄 갱신. 닫혀 있으면 깨우지도 않는다.
        if self.mem_win.is_open() {
            if now >= self.mem_next {
                let every = self.mem_every();
                self.mem_next = now + every;
                let s = self.mem_sample();
                self.mem_status = (s.sys.footprint, Some(now));
                self.mem_win.set_sample(s, every.as_millis() as u64);
                self.redraw();
            }
            next = next.min(self.mem_next);
        }
        el.set_control_flow(ControlFlow::WaitUntil(next));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // 한글 입력 진단(`NSQL_TRACE_IME=1` · T-139 · 평소 비용 = 환경 변수 조회 없음 — 기동 때 한 번 읽어 둔 깃발):
        //   창·키(논리/물리/text)·IME 사건을 온 순서대로 stderr에. 판정 로직은 건드리지 않는다.
        if self.trace_ime {
            match &event {
                WindowEvent::KeyboardInput { event: k, is_synthetic, .. } => eprintln!(
                    "[ime] {:?} key state={:?} logical={:?} physical={:?} text={:?} repeat={} synth={}",
                    id, k.state, k.logical_key, k.physical_key, k.text, k.repeat, is_synthetic
                ),
                WindowEvent::Ime(i) => eprintln!("[ime] {id:?} ime {i:?}"),
                WindowEvent::Focused(f) => eprintln!("[ime] {id:?} focused={f}"),
                WindowEvent::ModifiersChanged(m) => eprintln!("[ime] {id:?} mods={:?}", m.state()),
                _ => {}
            }
        }
        // ★ 닫힌 창에서 누른 키가 새 포커스 창으로 새는 두 길을 막는다(사용자 09-21 — 비밀번호 창 Enter → 편집기 줄바꿈):
        //   ① winit은 창이 포커스를 얻을 때 **이미 눌려 있는 키**를 합성 누름(`is_synthetic`)으로 보낸다 → 어느 창이든 버린다
        //   (사용자가 새로 누른 것이 아니다 · 수식키 상태는 `ModifiersChanged`가 따로 알린다) ② 계속 누르고 있으면 OS 자동 반복이
        //   새 창으로 온다 → 입력 창을 닫은 뒤 그 키가 떼어질 때까지의 반복 누름을 버린다(`key_guard_step`).
        if let WindowEvent::KeyboardInput {
            event: k,
            is_synthetic,
            ..
        } = &event
        {
            let pressed = k.state == ElementState::Pressed;
            if *is_synthetic && pressed {
                return;
            }
            let age = self.key_guard.map(|at| at.elapsed());
            let (drop_it, keep) = key_guard_step(age, pressed, k.repeat);
            if !keep {
                self.key_guard = None;
            }
            if drop_it {
                return;
            }
        }
        // 입력(키 누름·IME·마우스 버튼) = 캐럿 깜빡임 위상을 "켜짐"으로 되돌리고(움직인 캐럿이 최대 0.5초 안 보이던 것 · 09-19)
        //   계측이 켜져 있으면 입력→화면 지연의 시작점을 남긴다.
        let is_input = matches!(
            event,
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    state: ElementState::Pressed,
                    ..
                },
                ..
            } | WindowEvent::Ime(_)
                | WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                }
        );
        if is_input {
            let now = Instant::now();
            self.blink_origin = now;
            self.next_blink = now + Duration::from_millis(500);
            if self.frame_trace.is_some() {
                self.input_at = Some(now);
            }
        }
        if matches!(event, WindowEvent::Focused(true)) {
            self.on_window_focused(id);
            // 다른 앱에서 입력 소스를 바꾸고 돌아왔을 수 있다.
            self.sync_hangul_mode();
        }
        // 메인 창 활성/비활성(docs/58 §2-5): 돌아오는 순간 열린 파일을 한 번에 확인 · 뒤에 있을 때는 아무것도 안 한다.
        if let WindowEvent::Focused(on) = event {
            if self.window.as_ref().is_some_and(|w| w.id() == id) {
                self.main_active = on;
                // 위상 초기화: 돌아오면 켜진 채로 깜빡임 재개 · 나가면 켜진 채 한 번 그리고 멈춘다(09-24).
                self.blink_origin = Instant::now();
                self.next_blink = self.blink_origin + Duration::from_millis(500);
                self.redraw();
                if on {
                    self.ext_check(true);
                }
            }
        }
        // ★ 접속 창 = 모달: 열려 있는 동안 **메인 창과 그 일부인 로그·색·단축키·설정 창**의 입력은 버리고
        //   (OS 수준은 `winfocus::set_enabled`) 접속 창을 앞으로(사용자 09-15 "로그 창도 메인의 일부").
        if let WindowEvent::DroppedFile(p) = &event {
            // OS에서 창으로 끌어다 놓기(3-OS 공통 winit 경로) = 열기.
            let p = p.clone();
            self.open_file(&p);
            return;
        }
        let modal_open = self.modal_open();
        let is_modal_win = self.conn_win.is(id)
            || self.file_win.is(id)
            || self.sqlprev_win.is(id)
            || (self.input_win.is_modal() && self.input_win.is(id));
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
            if let Some(w) = self.modal_window() {
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
                            self.sess.status = match std::fs::write(&path, text) {
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
                        (PickerMode::Folder, FilePurpose::SettingFolder(key)) => {
                            // 고른 폴더 → 설정(직접 입력한 것과 같은 길: 저장 · 반영 · 설정 창 갱신).
                            let value = path.to_string_lossy().into_owned();
                            match self.settings.set(key, &value) {
                                Ok(_) => {
                                    self.persist_settings();
                                    if !self.apply_setting(key) {
                                        self.sess.status = t(Msg::StNeedsRestart).into();
                                    }
                                    self.prefs_win.refresh(&self.settings);
                                }
                                Err(e) => self.prefs_win.set_error(key, e.to_string()),
                            }
                            self.prefs_win.redraw();
                        }
                        (PickerMode::Save, FilePurpose::SqlPreview) => {
                            let text = self.sqlprev_win.text();
                            self.sess.status = match std::fs::write(&path, text) {
                                Ok(()) => tf(Msg::StSqlPreviewSaved, &[&path.to_string_lossy()]),
                                Err(e) => tf(Msg::ErrLogFile, &[&e.to_string()]),
                            };
                            self.sqlprev_win.set_note(self.sess.status.clone());
                        }
                        (PickerMode::Open, FilePurpose::Project) => self.project_load_path(&path),
                        (PickerMode::Save, FilePurpose::Project) => self.project_save_to(&path),
                        (PickerMode::Folder, FilePurpose::Project) => {
                            self.project_add_folder(&path)
                        }
                        (PickerMode::Folder, _) => {}
                        (PickerMode::Open, FilePurpose::RunFile) => {
                            self.load_file(&path, &enc, LoadMode::Run);
                        }
                        (PickerMode::Open, _) => self.open_file_enc(&path, &enc),
                        (PickerMode::Save, _) => {
                            self.editors.set_active_encoding(&enc);
                            self.save_to(&path);
                            self.finish_close_after_save();
                        }
                    }
                    self.sync_modal();
                }
                FileWinAction::ConfirmMany(paths, enc) => {
                    self.remember_file_dialog(paths.first().and_then(|p| p.parent()));
                    self.file_purpose = FilePurpose::Editor;
                    self.multi_open_ask(paths, enc);
                    self.sync_modal();
                }
                FileWinAction::Cancel => {
                    // 저장 창을 취소했다 = "저장하고 닫기"도 없던 일(탭은 그대로).
                    self.close_after_save = None;
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
            for a in self.conn_win.handle(&event) {
                self.handle_conn_win_action(a);
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
                                self.sess.status = t(Msg::StNeedsRestart).into();
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
                        self.sess.status = t(Msg::StNeedsRestart).into();
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
                // 설정 창이 다시 활성화됐다 → 파일 있음/없음·별칭 수를 다시 본다(파일 시스템만 · 네트워크 0 · 라이브러리 로드 0).
                PrefsAction::RefreshInfo => {
                    self.prefs_win.set_info(dbms_info_values());
                    self.prefs_win.refresh(&self.settings);
                    self.prefs_win.redraw();
                }
                PrefsAction::BrowseFolder { key, current } => {
                    // 폴더 전용 대화상자(파일은 보이지 않는다) — 시작 = 지금 값(있고 폴더면) · 고르면 그 설정에 넣는다.
                    if let Some(k) = nsql_settings::entry(&key).map(|e| e.key) {
                        self.file_purpose = FilePurpose::SettingFolder(k);
                        self.folder_start =
                            Some(PathBuf::from(current.trim())).filter(|p| p.is_dir());
                        self.open_file_dlg = Some(PickerMode::Folder);
                    }
                }
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
        if self.vars_win.is(id) {
            match self.vars_win.handle(&event) {
                vars_win::VarsWinAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    let rows = self.vars_rows();
                    self.vars_win
                        .paint(&rows, &self.ui_font, &self.theme, ui_px);
                }
                other => self.vars_apply(other),
            }
            return;
        }
        if self.sqlprev_win.is(id) {
            match self.sqlprev_win.handle(&event) {
                sqlprev_win::SqlPrevAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    let mono_px = self.settings.font_px("editor.font_size");
                    self.sqlprev_win.paint(
                        &self.ui_font,
                        &self.mono_font,
                        &self.theme,
                        ui_px,
                        mono_px,
                    );
                }
                sqlprev_win::SqlPrevAction::Refresh => {
                    // 체크박스로 바꾼 옵션은 설정(`gen.*`)에도 남기고 탐색기의 다음 생성에도 쓴다.
                    let o = self.sqlprev_win.opts();
                    for (k, v) in [
                        ("gen.qualified", o.qualified),
                        ("gen.compact", o.compact),
                        ("gen.full_ddl", o.full_ddl),
                        ("gen.separate_fk", o.separate_fk),
                    ] {
                        let cur = self.settings.flag(k);
                        if cur != v {
                            let _ = self.settings.set(k, if v { "on" } else { "off" });
                        }
                    }
                    self.persist_settings();
                    self.explorer.set_gen_opts(o);
                    if let Some(spec) = self.sqlprev_win.spec.clone() {
                        let server = self.sqlprev_win.server.clone();
                        self.sqlprev_win
                            .set_note(tf(Msg::StGenerating, &[&spec.title()]));
                        self.explorer.gen_sql(server.as_ref(), spec);
                    }
                }
                sqlprev_win::SqlPrevAction::Save => {
                    self.file_purpose = FilePurpose::SqlPreview;
                    self.open_file_dlg = Some(PickerMode::Save);
                }
                sqlprev_win::SqlPrevAction::OpenEditor => {
                    let title = self.sqlprev_win.file_name();
                    let text = self.sqlprev_win.text();
                    self.sqlprev_win.close();
                    self.editors.new_tab(Some(title));
                    self.editors.cur_mut().set_text(&text);
                    self.set_focus(Focus::Editor);
                    self.sync_modal();
                }
                sqlprev_win::SqlPrevAction::Copy => {
                    let text = self.sqlprev_win.copy_text();
                    let n = text.chars().count();
                    if clipboard::write_text(&text) {
                        self.sqlprev_win
                            .set_note(tf(Msg::SpCopied, &[&n.to_string()]));
                    } else {
                        self.sqlprev_win.set_note(t(Msg::ErrClipboard).to_string());
                    }
                }
                sqlprev_win::SqlPrevAction::Close => {
                    self.sqlprev_win.close();
                    self.sync_modal();
                }
                sqlprev_win::SqlPrevAction::None => {
                    if self.conn_modal && !self.modal_open() {
                        self.sync_modal();
                    }
                }
            }
            return;
        }
        if self.input_win.is(id) {
            match self.input_win.handle(&event) {
                input_win::InputWinAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    self.input_win.paint(&self.ui_font, &self.theme, ui_px);
                }
                input_win::InputWinAction::Run(values) => {
                    self.input_reply(worker::InputReply::Values(values));
                }
                input_win::InputWinAction::Password(secret) => {
                    self.password_reply(worker::PwReply::Value(secret));
                    self.sync_modal();
                }
                input_win::InputWinAction::Cancel if self.input_win.is_password() => {
                    self.password_reply(worker::PwReply::Cancel);
                    self.sync_modal();
                }
                input_win::InputWinAction::Skip => {
                    self.input_reply(worker::InputReply::Values(Vec::new()));
                }
                input_win::InputWinAction::Cancel => {
                    self.input_reply(worker::InputReply::Cancel);
                }
                input_win::InputWinAction::None => {
                    // 모달이었다가 닫혔으면(창 닫기 등) 메인을 다시 살린다.
                    if self.conn_modal && !self.modal_open() {
                        self.sync_modal();
                    }
                }
            }
            return;
        }
        if self.mem_win.is(id) {
            match self.mem_win.handle(&event) {
                mem_win::MemWinAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    self.mem_win.paint(&self.ui_font, &self.theme, ui_px);
                }
                mem_win::MemWinAction::Toggled(on) => {
                    let _ = self
                        .settings
                        .set("mem.always_on_top", if on { "on" } else { "off" });
                }
                mem_win::MemWinAction::Trim => {
                    // 힙 정리(80 §7): 할당자 빈 조각 → OS · 곧바로 표본을 다시 떠서 전후를 보인다.
                    let before = memstat::sys_total();
                    let us = memtrim::trim();
                    let s = self.mem_sample();
                    self.mem_status = (s.sys.footprint, Some(Instant::now()));
                    let every = self.mem_every();
                    self.mem_win.set_sample(s, every.as_millis() as u64);
                    self.sess.status = tf(
                        Msg::StMemTrimResult,
                        &[
                            &memstat::fmt(before),
                            &memstat::fmt(s.sys.footprint),
                            &(us / 1000).to_string(),
                        ],
                    );
                    self.redraw();
                }
                mem_win::MemWinAction::Close => {
                    self.mem_win.close();
                    self.persist_window_sizes(false);
                    self.redraw();
                }
                mem_win::MemWinAction::None => {}
            }
            return;
        }
        if self.sessions_win.is(id) {
            // ★ 그린 직후에 다시 그리기를 요청하면 끝없이 돈다(세션 창을 열어 두면 유휴 CPU 90% · 09-19 메모리 점검에서 발견).
            let action = self.sessions_win.handle(&event);
            let painted = matches!(action, SessWinAction::Paint);
            match action {
                SessWinAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    let rows = self.session_rows();
                    self.sessions_win
                        .paint(&rows, &self.ui_font, &self.theme, ui_px);
                }
                SessWinAction::Activate(sid) => self.activate_shared(sid),
                SessWinAction::Reconnect(sid) => self.disconnect_pick(&format!("conn.again:{sid}")),
                SessWinAction::Disconnect(sid) => self.disconnect_session(sid),
                SessWinAction::DisconnectAll => self.disconnect_all(),
                SessWinAction::GoTab(tab) => {
                    self.editors.switch_to_id(tab);
                    self.sync_grid_tab();
                    if let Some(w) = &self.window {
                        w.focus_window();
                    }
                    self.redraw();
                }
                SessWinAction::None => {}
            }
            if !painted {
                self.sessions_win.redraw();
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
                            self.sess.status = t(Msg::ErrClipboard).into();
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
                // 🔧 창 닫기(X)도 **늘** 종료 흐름(`request_exit` = 프로젝트 저장/물음 · 미저장 파일 탭 물음 · 트랜잭션 확인)을
                //   지난다 — 종전에는 미커밋이 없으면 흐름을 건너뛰어 프로젝트·북마크가 저장되지 않았다(사용자 09-23 "종료 시 꼭 저장").
                self.request_exit();
                self.redraw();
                if !self.exit_requested {
                    return;
                }
                self.flush_on_exit();
                self.persist_window_sizes(true);
                for s in self.all_sess() {
                    s.worker.send(worker::Cmd::Quit);
                }
                el.exit();
                return;
            }
            WindowEvent::Moved(_) => {
                // 다른 배율의 모니터로 옮겨진 뒤 ScaleFactorChanged가 안 오는 경우(프로그램 이동 · 09-17) 배율을 다시 읽는다.
                if let Some(w) = &self.window {
                    let s = w.scale_factor() as f32;
                    if (s - self.scale).abs() > 0.01 {
                        self.scale = s;
                        self.layout();
                    }
                }
                self.redraw();
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
                // ★ Alt를 누르는 동안 = 전체 경로 보기(메뉴·팔레트·검색 결과의 가운데 … 축약 해제 · 사용자 09-22).
                if nexa_ctl::draw::set_show_full(self.alt) {
                    self.redraw();
                }
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
                    // 편집기 본문(거터 제외) 위 = **I-빔**(Sublime·VS Code · 사용자 09-19) — 메뉴·팔레트·팝업이 덮으면 화살표.
                    let ed = self.editors.editor_bounds();
                    let gutter = self.editors.cur().gutter_width();
                    let text_area = Rect::new(ed.x + gutter, ed.y, (ed.w - gutter).max(0), ed.h);
                    let covered = self.menubar.is_open()
                        || self.palette.is_open()
                        || self.tool_dock.is_dragging()
                        || self.status_menu.is_open()
                        || self.intel.is_open()
                        || self.editors.cur().popup_open();
                    let over_text = !covered && text_area.contains(cur);
                    // 스플리터 위/드래그 중 = ↔ · ↕ (그리드 헤더 경계보다 우선).
                    w.set_cursor(
                        if self.split_v.is_dragging() || self.split_v.rect().contains(cur) {
                            winit::window::CursorIcon::ColResize
                        } else if self.split_h.is_dragging() || self.split_h.rect().contains(cur) {
                            winit::window::CursorIcon::RowResize
                        } else if over_edge {
                            winit::window::CursorIcon::ColResize
                        } else if over_text {
                            winit::window::CursorIcon::Text
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
                    // ★ 검색에 쓰이는 입력은 전부 조합 중 글자까지 바로 거른다(설정 창 검색과 같은 규칙 · 09-19 확장 패널 →
                    //   09-23 프로젝트 필터·북마크 필터·찾기 막대로 일반화 · 사용자 "자모 완성과 상관없이 한글 검색").
                    match self.focus {
                        Focus::Ext => self.ext_panel.query_changed(),
                        Focus::Project => self.project_panel.query_changed(),
                        Focus::Bookmarks => self.bm_panel.query_changed(),
                        Focus::Outline => self.outline_panel.query_changed(),
                        Focus::Find => self.find_step(true, false),
                        _ => {}
                    }
                    self.redraw();
                }
                return;
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                // 한/영 키(Windows · 탐색기 포커스 전용 — nexa-beep docs/27 §8): 탐색기는 IME를 끊어 OS 전환이 무력하므로
                //   앱이 모드를 토글한다. VK_HANGUL은 키보드 드라이버 수준이라 IME 없이도 온다(논리 HangulMode · 물리 Lang1).
                if cfg!(windows)
                    && self.focus == Focus::Explorer
                    && (kev.logical_key == Key::Named(NamedKey::HangulMode)
                        || kev.physical_key
                            == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Lang1))
                {
                    self.hangul_mode = !self.hangul_mode;
                    self.sess.status = t(if self.hangul_mode {
                        Msg::StHangulOn
                    } else {
                        Msg::StHangulOff
                    })
                    .into();
                    self.redraw();
                    return;
                }
                // ★ 적재 중인 탭에서 Esc = 그 적재 취소(자리 탭을 닫는다 · 사용자 09-20). 팔레트·찾기 막대가 열려 있으면 그쪽의 Esc.
                if kev.logical_key == Key::Named(NamedKey::Escape)
                    && self.focus == Focus::Editor
                    && !self.palette.is_open()
                    && self.editors.active_loading()
                    && self.file_load_cancel_active()
                {
                    return;
                }
                // 탐색기 포커스의 F5 = 선택 노드 하위를 조용히 다시 읽기 · Shift+F5 = 캐시를 버리고 새로(docs/57 T3).
                //   편집기 포커스의 F5(전체 실행)와 겹치지 않게 키맵보다 먼저 본다.
                if self.focus == Focus::Explorer && kev.logical_key == Key::Named(NamedKey::F5) {
                    self.explorer.refresh_selected(self.shift);
                    self.redraw();
                    return;
                }
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
                                self.sess.status = tf(
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
                    self.tmark("key");
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
                            self.sess.status = tf(Msg::StChordPending, &[&ch.display()]);
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
                self.tmark("RedrawRequested");
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
        if std::mem::take(&mut self.open_vars) {
            self.open_vars_window(el);
        }
        if std::mem::take(&mut self.open_sessions) {
            self.open_sessions_window(el);
        }
        if std::mem::take(&mut self.open_mem) {
            self.open_mem_window(el);
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
            self.prefs_win.set_info(dbms_info_values());
            self.prefs_win.refresh(&self.settings);
            if let Some(q) = self.prefs_query.take() {
                self.prefs_win.preset_query(&q);
            }
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
            self.finish_exit(el);
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

/// 닫힌 창의 키 문지기 — `(이 사건을 버리는가, 문지기를 유지하는가)`. `age` = 창을 닫은 뒤 지난 시간(`None` = 문지기 없음).
/// 자동 반복 누름 = 버림 · 뗌 = 통과시키고 끝 · **새로 누른 키**(반복 아님) = 사용자의 다음 입력이므로 통과시키고 끝 ·
/// 3초가 지나면(뗌 사건을 다른 창이 받아 놓친 경우) 스스로 끝난다.
fn key_guard_step(age: Option<Duration>, pressed: bool, repeat: bool) -> (bool, bool) {
    match age {
        None => (false, false),
        Some(a) if a > Duration::from_secs(3) => (false, false),
        Some(_) if pressed && repeat => (true, true),
        Some(_) => (false, false),
    }
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
    // 기동 구간 계측(`NSQL_TRACE_FRAMES=1` · docs/39 S-14 · Linux 09-22): 표시는 `[startup]` 한 줄(누적 ms) — 창이 보이기까지 어디에 쓰였는가.
    let boot = Instant::now();
    let mut marks: Vec<(&'static str, Duration)> = Vec::new();
    let mark =
        |m: &mut Vec<(&'static str, Duration)>, name: &'static str| m.push((name, boot.elapsed()));
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
    // UI 글꼴 = 설정 `ui.font_face`(비면 OS 사슬 · 못 찾으면 사슬로 fail-over · 사용자 09-17 비레티나 모니터 비교용).
    mark(&mut marks, "settings");
    let ui_pref = settings.get("ui.font_face").map(str::to_string);
    let ui = nexa_font::ui_font(ui_pref.as_deref()).or_else(|| nexa_font::ui_font(None));
    // 고정폭 = 설정 `editor.font_face`(비면 OS 기본 사슬 · 없는 이름은 fail-over · 사용자 09-17).
    let mono_pref = settings.get("editor.font_face").map(str::to_string);
    let mono = nexa_font::mono_font(mono_pref.as_deref());
    mark(&mut marks, "fonts");
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
    // ★ 시작 모드(사용자 09-22 · 다중 인스턴스): 인자 중 파일은 갈라낸다 — `.nsql-project` = 프로젝트 모드로 진입 ·
    //   그 밖 존재하는 파일 = 파일 모드로 연다 · 나머지 = 종전 접속 인자. 인스턴스 잠금(설정 폴더 `instance.lock`)은
    //   "이미 열린 인스턴스가 있는가"만 판단한다(마지막 프로젝트 복원은 첫 인스턴스 + 설정이 켜졌을 때만).
    let (arg_project, arg_files, arg_folder, args) = split_file_args(&args);
    let instance_lock = instance_lock();
    let first_instance = instance_lock.is_some();
    let (arg_target, arg_fill_only) = parse_gui_args(&args);
    // ★ 임시(사용자 09-19 · 추후 제거): `dev.start_demo`가 켜져 있고 인자로 대상을 주지 않았으면 Demo 프로필에 자동 접속 +
    //   로그인 창 생략(`-c Demo`와 같은 경로).
    //   ★ 릴리즈에는 절대 들어가지 않는다(사용자 09-19): Debug 빌드(`debug_assertions`)에서만 유효 — Release는 설정이 켜져 있어도 무시.
    let dev_demo =
        cfg!(debug_assertions) && settings.flag("dev.start_demo") && arg_target.is_none();
    let arg_target = if dev_demo {
        Some("Demo".to_string())
    } else {
        arg_target
    };
    // Linux(09-22): 창 백엔드 = 설정 `gfx.linux_backend`(기본 X11 — 모달 창을 메인의 transient로 붙이려면 · Wayland 경로는 winit 0.30이
    // 부모 창·활성화를 지원하지 않는다). 고른 백엔드로 못 만들면(예: XWayland 없음) winit 기본으로 한 번 더.
    let built = {
        let mut b = EventLoop::<Wake>::with_user_event();
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            use winit::platform::wayland::EventLoopBuilderExtWayland as _;
            use winit::platform::x11::EventLoopBuilderExtX11 as _;
            match settings.get("gfx.linux_backend").unwrap_or("x11") {
                "x11" if std::env::var_os("DISPLAY").is_some() => {
                    b.with_x11();
                }
                "wayland" if std::env::var_os("WAYLAND_DISPLAY").is_some() => {
                    b.with_wayland();
                }
                _ => {}
            }
        }
        b.build()
            .or_else(|_| EventLoop::<Wake>::with_user_event().build())
    };
    let Ok(el) = built else {
        eprintln!("event loop creation failed");
        std::process::exit(1);
    };
    mark(&mut marks, "event_loop");
    let proxy: EventLoopProxy<Wake> = el.create_proxy();
    let max_rows = settings.int("grid.max_rows").max(0) as usize;
    let autocommit = settings.flag("session.autocommit");
    let (worker, events) = worker::spawn(
        DEFAULT_DIALECT,
        max_rows,
        autocommit,
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
    mark(&mut marks, "worker+probe");
    let profiles = worker::profile_names();
    mark(&mut marks, "profiles");
    let mut panel = ConnectPanel::new(nsql_drivers::available());
    mark(&mut marks, "connect_panel");
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
    mark(&mut marks, "theme");
    let syntax_reg = Rc::new(SyntaxRegistry::load());
    mark(&mut marks, "syntax");
    let rulers = parse_rulers(settings.get("editor.rulers").unwrap_or("80"));
    let ws_style = whitespace_style(&settings);
    let attempts_max = settings.int("connect.max_concurrent").clamp(1, 16) as usize;
    let keymap = Keymap::from_settings(&settings);
    mark(&mut marks, "keymap");
    let explorer = {
        let proxy = std::sync::Mutex::new(wake_proxy.clone());
        let mut e = explorers::ExplorerSet::new(
            std::sync::Arc::new(move || {
                if let Ok(p) = proxy.lock() {
                    let _ = p.send_event(Wake);
                }
            }),
            settings.flag("explorer.visible"),
        );
        e.set_icons(settings.flag("explorer.icons"));
        e.set_typeahead(typeahead_cfg(&settings));
        e.set_preload(settings.flag("intel.preload"));
        e.set_routines(settings.flag("intel.from_routines"));
        e.set_disconnect_pick(settings.get("explorer.disconnect_pick").unwrap_or("auto"));
        e.set_keep_offline(settings.flag("explorer.keep_offline"));
        e.set_filter_scope(settings.get("explorer.filter_scope").unwrap_or("all"));
        e.set_gen_opts(gen_opts_from(&settings));
        e.set_schema_opts(schema_opts_from(&settings));
        e.set_index_cfg(index_cfg_from(&settings));
        e.set_share_catalog(settings.flag("explorer.share_catalog"));
        e.set_tooltip_delay(settings.int("ui.tooltip_delay_ms").max(0) as u128);
        // ★ 문법 참조 플러그인(nsql-script `grammar` · 09-24): 설정 폴더 `grammar/*.sqlg`가 내장 방언을 대신하거나 새 방언을 더한다.
        if let Some(dir) = nsql_settings::config_dir() {
            for (f, r) in nsql_script::grammar::load_dir(&dir.join("grammar")) {
                if let Err(err) = r {
                    eprintln!("[grammar] {f}: {err}");
                }
            }
        }
        e
    };
    mark(&mut marks, "explorer");
    let colors_win = ColorsWin::new(
        color_setting(&settings, "ui.hover_color"),
        color_setting(&settings, "ui.pressed_color"),
        &recent_colors(&settings),
    );
    let txlog_cap = settings.int("txlog.max_entries").max(16) as usize;
    mark(&mut marks, "colors_win");
    let mut sess = Sess::new(SHARED, None, worker, events, DEFAULT_DIALECT);
    sess.status = status;
    mark(&mut marks, "sess");
    // 창 부품의 생성 비용을 `[startup]`에 낱개로 보이게(09-22 Linux: `App { … }` 한 덩어리가 300 ms였다).
    let log_win = LogWin::new(&log_format);
    mark(&mut marks, "log_win");
    let txlog_win = TxLogWin::new();
    mark(&mut marks, "txlog_win");
    let sessions_win = SessionsWin::new();
    mark(&mut marks, "sessions_win");
    let input_win = input_win::InputWin::new();
    mark(&mut marks, "input_win");
    let vars_win = vars_win::VarsWin::new();
    mark(&mut marks, "vars_win");
    let git = gitstat::GitWatch::new();
    mark(&mut marks, "git");
    let keys_win = KeysWin::new();
    mark(&mut marks, "keys_win");
    let prefs_win = PrefsWin::new();
    mark(&mut marks, "prefs_win");
    let act_bar = ActivityBar::new();
    mark(&mut marks, "act_bar");
    let file_win = FileWin::new();
    mark(&mut marks, "file_win");
    let menubar = MenuBar::new(App::build_menus());
    mark(&mut marks, "menubar");
    let conn_win = ConnWin::new(panel);
    mark(&mut marks, "conn_win");
    let find = FindBar::new();
    mark(&mut marks, "find");
    let editors = Editors::new(ed_line_numbers, ed_multi, ed_tooltip, syntax_reg.clone());
    mark(&mut marks, "editors");
    let search = SearchPanel::new();
    mark(&mut marks, "search");
    let project_panel = project_panel::ProjectPanel::new();
    let bm_panel = bookmarks_panel::BookmarksPanel::new();
    let intel = intel::Intel::new(intel::IntelCfg::from_settings(&settings));
    let outline_panel = outline_panel::OutlinePanel::new();
    let ext_panel = ExtPanel::new();
    mark(&mut marks, "ext_panel");
    let palette = Palette::new();
    mark(&mut marks, "palette");
    let toasts = toast::Toasts::new();
    mark(&mut marks, "toasts");
    let search_history = {
        let mut h = search_history::SearchHistory::load_in(
            nsql_settings::config_dir().as_deref(),
            settings.int("search.history_max").max(0) as usize,
        );
        h.set_view(search_history::HistoryView::parse(
            settings.get("search.history_view").unwrap_or("dropdown"),
        ));
        h.set_rows(settings.int("search.history_rows").max(1) as usize);
        h.shared()
    };
    let mut app = App {
        window: None,
        surface: None,
        tab_vars: std::collections::HashMap::new(),
        global_vars: varsfile::dir()
            .map(|d| varsfile::load_global(&d))
            .unwrap_or_default(),
        vars_pruned: false,
        ui_font: ui.font,
        mono_font: mono.font,
        grid_font: load_grid_font(
            settings.get("grid.font_face").unwrap_or(""),
            settings.get("editor.font_face"),
        ),
        theme: initial_theme,
        settings,
        scale: 1.0,
        log_win,
        txlog_win,
        sessions_win,
        mem_win: mem_win::MemWin::new(),
        sqlprev_win: sqlprev_win::SqlPrevWin::new(),
        sqlprev_pending: None,
        input_win,
        input_pending: None,
        pw_pending: None,
        close_after_save: None,
        key_guard: None,
        pw_once: None,
        vars_win,
        vars_changed: (0, std::collections::HashSet::new()),
        open_sessions: false,
        open_mem: false,
        open_vars: false,
        open_txlog: false,
        toasts,
        tx_warn: txwarn::TxWarn::default(),
        tx_guard_next: Instant::now(),
        run_toast_next: None,
        run_stmt_enabled: true,
        run_stop_enabled: true,
        log_hub: None,
        file_purpose: FilePurpose::Editor,
        exit_requested: false,
        z_order: Vec::new(),
        palette,
        syntax: syntax_reg,
        status_syntax_rect: Rect::new(0, 0, 0, 0),
        status_eol_rect: Rect::new(0, 0, 0, 0),
        status_enc_rect: Rect::new(0, 0, 0, 0),
        git,
        status_tab_rect: Rect::new(0, 0, 0, 0),
        status_menu: nexa_ctl::controls::ctxmenu::ContextMenu::new(),
        bm_gutter: None,
        toggle_log: false,
        open_colors: false,
        colors_win,
        keymap,
        pending_chord: None,
        hangul_mode: false,
        open_keys: false,
        keys_win,
        open_prefs: false,
        prefs_query: None,
        prefs_win,
        act_bar,
        file_win,
        open_file_dlg: None,
        tab_defines: HashMap::new(),
        project_autosave_at: Instant::now(),
        project_last_json: Vec::new(),
        project_touch_at: None,
        exit_pending: false,
        exit_project_asked: false,
        last_synced_tab: u64::MAX,
        multi_pending: None,
        multi_load: None,
        arg_project,
        arg_files,
        arg_folder,
        first_instance,
        _instance_lock: instance_lock,
        folder_start: None,
        menubar,
        tabs_menu_sig: String::new(),
        tool_dock: App::build_tool_dock(),
        tool_floats: Vec::new(),
        pending_float: Vec::new(),
        tool_layout_dirty: false,
        demo_ready: false,
        demo_job: None,
        pending_demo_prompt: false,
        copy_armed_until: None,
        project_new_fresh: false,
        conn_win,
        // 시작 시 로그인 창 — 인자로 접속 대상을 줬거나 임시 Demo 자동 접속이면 띄우지 않는다.
        open_conn: initial_target.is_none() || arg_fill_only,
        find,
        find_scope: None,
        editors,
        grid: grid::Grid::default(),
        panel: ResultPanel::new(
            ResultTab {
                id: 0,
                title: String::new(),
                pinned: false,
                named: false,
                sql: String::new(),
                grid: grid::Grid::default(),
                seq: 0,
                child_of: None,
            },
            true,
            false,
        ),
        panel_editor: 0,
        panels: HashMap::new(),
        next_result_id: 1,
        result_area: Rect::new(0, 0, 0, 0),
        offset_warned: std::collections::HashSet::new(),
        frame_trace: std::env::var_os("NSQL_TRACE_FRAMES").map(|_| FrameTrace::new(boot)),
        trace_ime: std::env::var_os("NSQL_TRACE_IME").is_some(),
        hangul_app: None,
        input_at: None,
        trace_marks: std::cell::RefCell::new(Vec::new()),
        blink_origin: Instant::now(),
        extensions: extensions::Registry::builtin(),
        ext_catalog: Vec::new(),
        grid_tab: 0,
        explorer,
        search,
        project_panel,
        bookmarks: bookmarks::Bookmarks::new(),
        bm_panel,
        bm_titles_rev: u64::MAX,
        search_history,
        intel,
        outline_panel,
        project: project::Project::default(),
        ext_panel,
        ext_fetch_rx: None,
        ext_view: ext_view::ExtView::default(),
        ext_details: HashMap::new(),
        ext_view_key: String::new(),
        meta_refresh_next: None,
        ext_watch: None,
        ext_files: HashMap::new(),
        ext_banner: extfile::Banner::default(),
        ext_poll_next: None,
        // 포커스 사건이 오기 전에는 비활성(09-24: 한 번도 활성화되지 않은 창(`NSQL_NO_ACTIVATE` 시험 · 뒤에서 띄운 창)이
        //   계속 깜빡이며 다시 그렸다) — 정상 기동은 첫 `Focused(true)`로 바로 활성이 된다.
        main_active: false,
        ext_last_tab: 0,
        ext_save_armed: None,
        startup_after_connect: Vec::new(),
        startup_connected: false,
        startup_timed: Vec::new(),
        file_loads: Vec::new(),
        big_pending: None,
        undo_pruned: false,
        mem_trim_due: None,
        mem_trim_next: None,
        mem_last_tabs: 0,
        split_v: Splitter::new(SplitAxis::Vertical),
        split_h: Splitter::new(SplitAxis::Horizontal),
        tx_after: None,
        status_tx_rect: Rect::new(0, 0, 0, 0),
        json_watch: None,
        conn_modal: false,
        json_next: Instant::now(),
        focus: Focus::Editor,
        txlog: nsql_run::txlog::TxLog::new(txlog_cap),
        sess,
        run_toast: runtoast::RunToast::new(),
        parked: Vec::new(),
        next_sess_id: 1,
        tab_bind: HashMap::new(),
        default_shared: SHARED,
        default_spec: None,
        primary_sess: SHARED,
        gate_shown: None,
        badge_menu_tab: None,
        badge_tabs: Vec::new(),
        sess_ui_dirty: false,
        idle_next: Instant::now(),
        tests_tx,
        tests_rx,
        wake_proxy,
        attempt_queue: std::collections::VecDeque::new(),
        attempts_inflight: 0,
        attempts_max,
        cursor: (0, 0),
        shift: false,
        primary: false,
        alt: false,
        pointer: None,
        status_mem_rect: Rect::new(0, 0, 0, 0),
        mem_status: (0, None),
        mem_pressed: false,
        mem_next: Instant::now(),
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
    // 검색어 이력(전역 한 벌)을 상자를 가진 곳마다 건넨다(찾기/바꾸기 · 파일 검색 · 프로젝트/북마크/아웃라인/확장 필터 · 설정 검색 · 사용자 09-23).
    {
        let h = app.search_history.clone();
        app.find.set_history(h.clone());
        app.search.set_history(h.clone());
        app.project_panel.set_history(h.clone());
        app.explorer.set_history(h.clone());
        // 글로벌 변수 층을 첫 세션에(docs/63 §11).
        app.sess
            .worker
            .send(worker::Cmd::GlobalVars(app.global_vars.clone()));
        app.bm_panel.set_history(h.clone());
        app.outline_panel.set_history(h.clone());
        app.ext_panel.set_history(h.clone());
        app.prefs_win.set_history(h);
    }
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
    app.editors.set_large_cfg(large_cfg(&app.settings));
    {
        let (ext, syn) = large_feature_levels(&app.settings);
        app.editors.set_large_feature_levels(ext, syn);
    }
    app.editors
        .set_undo_budget(app.settings.int("editor.undo_budget_mb").max(1) as usize * 1024 * 1024);
    let (ms, giant) = undo_rules(&app.settings);
    app.editors.set_undo_rules(ms, giant);
    let cap = app.occurrence_cap();
    app.editors.set_max_regions(cap);
    nexa_gfx::text::set_glyph_cache_max(app.settings.int("ui.glyph_cache").max(256) as usize);
    nexa_fs::shell::set_icon_cache_max(app.settings.int("file.icon_cache").max(16) as usize);
    // D-58: full 모드인데 배터리/원격 세션이면 1회 안내.
    if let Some(m) = app.settings.perf_hint() {
        app.sess.status = t(m).into();
    }
    let null_text = app
        .settings
        .get("grid.null_text")
        .unwrap_or("NULL")
        .to_string();
    app.grid.set_null_text(&null_text);
    app.apply_grid_row_focus();
    app.bookmarks.apply_settings(&app.settings);
    let bm_on = app.bookmarks.enabled;
    app.act_bar.set_item_visible("view.bookmarks", bm_on);
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
    app.toasts.configure_progress(
        app.settings.flag("ui.toast_progress"),
        app.settings.int("ui.toast_fade_to"),
        app.settings.int("ui.toast_bar_spent"),
    );
    {
        let on = app.settings.flag("ui.ime_hint");
        app.conn_win.set_ime_hint(on);
        app.input_win.set_ime_hint(on);
    }
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
    apply_oracle_client(&app.settings);
    app.apply_net_options();
    // 첫 화면부터 탭 표식(미연결 사선)이 보이게 — 세션 상태 → 화면 3층 동기화 1회.
    app.sync_sess_ui();
    nsql_drivers::set_mssql_cancel_socket(app.settings.get("mssql.cancel") == Some("socket"));
    app.apply_tab_accent();
    app.apply_extensions(None);
    app.apply_window_sizes();
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
    // 화면 내보내기 방식(T-147) — 첫 창이 만들어지기 전에.
    present::set_mode(app.settings.get("gfx.mac_present").unwrap_or("softbuffer"));
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
                    app.sess.status = tf(Msg::StArgProfileMissing, &[&target]);
                    None
                }
            }
        } else {
            nsql_drivers::parse_target(&target, DEFAULT_DIALECT).ok()
        };
        match spec {
            Some(spec) => {
                app.sess.busy = true;
                app.sess.status = tf(Msg::StConnecting, &[&spec.redacted()]);
                app.sess.last_spec = Some(spec.clone());
                // 스펙을 미리 둔다 — `RunEvent::Connected`의 탐색기 붙이기가 `spec`을 본다(`connect_quietly`와 같게).
                app.sess.spec = Some(spec.clone());
                if nsql_vault::is_profile_name(&target) {
                    app.conn_win.select_by_name(&target);
                    // 접속 창 Connect와 같게 프로필 이름을 세션에 — 탐색기 루트·공유 연결 목록이 `host:port`가 아니라
                    // 프로필 이름으로 보인다(86차 mac 점검 · T-148).
                    app.sess.profile = target.clone();
                }
                app.sess.worker.send(worker::Cmd::ConnectSpec {
                    spec,
                    reconnect_same: false,
                });
            }
            None if !nsql_vault::is_profile_name(&target) => {
                app.sess.busy = true;
                app.sess.status = tf(Msg::StConnecting, &[&target]);
                app.sess.worker.send(worker::Cmd::Connect(target));
            }
            None => {}
        }
    }
    mark(&mut marks, "app");
    if app.frame_trace.is_some() {
        // 누적 ms — 구간 = 앞 표시와의 차. 창 자체는 `resumed`에서 만들어지므로 `[frames] … first paint … at +N ms`가 그 다음 표시다.
        let line: Vec<String> = marks
            .iter()
            .map(|(n, d)| format!("{n} {:.0}", d.as_secs_f64() * 1000.0))
            .collect();
        eprintln!("[startup] {} (ms, cumulative)", line.join(" · "));
    }
    if let Err(e) = el.run_app(&mut app) {
        eprintln!("{}", tf(Msg::ErrEventLoop, &[&e.to_string()]));
        std::process::exit(1);
    }
}

/// 프레임 계측 누적(`NSQL_TRACE_FRAMES=1`) — 60프레임마다 한 줄: 평균/최대 총 ms · 구간별 평균 ms.
struct FrameTrace {
    /// `main()` 시작 시각 — 첫 페인트가 기동 뒤 몇 ms인지(`[startup]`과 같은 기준).
    boot: Instant,
    announced: bool,
    n: u32,
    total_us: u64,
    max_us: u32,
    secs_us: [u64; 6],
}

impl FrameTrace {
    fn new(boot: Instant) -> Self {
        FrameTrace {
            boot,
            announced: false,
            n: 0,
            total_us: 0,
            max_us: 0,
            secs_us: [0; 6],
        }
    }

    fn add(&mut self, total: u32, secs: &[u32; 6]) {
        self.n += 1;
        if !self.announced {
            self.announced = true;
            eprintln!(
                "[frames] tracing on · first paint {:.2}ms at +{:.0}ms",
                f64::from(total) / 1000.0,
                self.boot.elapsed().as_secs_f64() * 1000.0
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
                ..Self::new(self.boot)
            };
        }
    }
}

/// 탐색기 타입어헤드 설정 한 벌(`explorer.typeahead*`).
/// 스키마 목록 옵션(설정 → `SchemaOpts` · 시스템 스키마 기본 숨김 · DBeaver 대조 09-25).
fn schema_opts_from(settings: &Settings) -> nsql_catalog::SchemaOpts {
    nsql_catalog::SchemaOpts {
        show_system: settings.flag("explorer.show_system_schemas"),
        hide_empty: settings.flag("explorer.hide_empty_schemas"),
    }
}

/// 검색 인덱스 설정(`explorer.search_index` · `index_max` · `index_hits_max` · `index_prefetch` · `index_idle_ms` · docs/84 §5).
fn index_cfg_from(settings: &Settings) -> explorer::IndexCfg {
    explorer::IndexCfg {
        on: settings.flag("explorer.search_index"),
        max: settings.int("explorer.index_max").max(0) as usize,
        hits_max: settings.int("explorer.index_hits_max").max(1) as usize,
        prefetch: settings.flag("explorer.index_prefetch"),
        idle_ms: settings.int("explorer.index_idle_ms").max(0) as u64,
        warm_columns_max: settings.int("meta.warm_columns_max").max(0) as usize,
        warm_idle_ms: settings.int("meta.warm_idle_ms").max(0) as u64,
        detail_max: settings.int("meta.detail_max").max(0) as usize,
        detail_ttl_secs: settings.int("meta.detail_ttl_secs").max(0) as u64,
        cols_ttl_secs: settings.int("meta.cols_ttl_secs").max(0) as u64,
        disk_cache: settings.flag("meta.disk_cache"),
    }
}

/// Generate SQL 옵션(설정 `gen.*` → `GenOpts` · docs/83 §3-1).
fn gen_opts_from(settings: &Settings) -> nsql_catalog::GenOpts {
    nsql_catalog::GenOpts {
        qualified: settings.flag("gen.qualified"),
        compact: settings.flag("gen.compact"),
        full_ddl: settings.flag("gen.full_ddl"),
        separate_fk: settings.flag("gen.separate_fk"),
    }
}

fn typeahead_cfg(s: &Settings) -> explorer::TypeAheadCfg {
    explorer::TypeAheadCfg {
        enabled: s.flag("explorer.typeahead"),
        timeout_ms: s.int("explorer.typeahead_timeout").clamp(200, 60_000) as u64,
        filter: nexa_ctl::TypeAheadFilter {
            space: s.flag("explorer.typeahead_space"),
            special: s.flag("explorer.typeahead_special"),
        },
        pos: nexa_ctl::HudPos::parse(s.get("explorer.typeahead_pos").unwrap_or("bottom_left")),
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
    ("conn.sessions", Msg::TipSessions),
    ("view.log", Msg::TipLog),
];

/// 파일 대화상자의 용도 — 같은 대화상자를 편집기 열기/저장과 로그 내보내기가 나눠 쓴다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilePurpose {
    Editor,
    LogExport,
    /// 설정의 폴더 경로 고르기(설정 키) — 고른 폴더를 그 설정에 넣는다.
    SettingFolder(&'static str),
    /// 디스크에서 바로 실행할 SQL 파일 고르기(docs/59 §4 3단계).
    RunFile,
    /// 프로젝트 파일 열기/저장 · 프로젝트 폴더 추가(docs/67).
    Project,
    /// SQL Preview 본문을 파일로(docs/83 §4).
    SqlPreview,
}

/// 읽은 파일을 어떻게 쓸 것인가.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoadMode {
    Open,
    ReadOnly,
    /// 앞부분 이만큼(바이트)만 — 읽기 전용 안내 탭.
    Head(u64),
    /// 편집기에 싣지 않고 실행.
    Run,
}

struct FileLoaded {
    path: PathBuf,
    mode: LoadMode,
    /// 자리 탭(스레드 적재) · None = 동기 경로(지금 만든다) 또는 실행.
    tab: Option<u64>,
    /// 적재를 시작할 때의 활성 탭(실행 = 그 탭의 세션으로만).
    origin: u64,
    result: Result<fileload::Loaded, String>,
}

/// 스레드 적재 한 건(진행 상태 + 받을 곳).
/// ★ **다중 열기 순차 적재**(사용자 09-22): 파일 대화상자에서 여러 파일을 고르면 자리 탭을 **먼저 전부** 만들고,
/// 작업 스레드 **하나**가 고른 순서대로 읽어 탭마다 결과를 보낸다(파일별 독립 · 동시 스레드 N개의 경합 없음).
/// 그동안 이미 채워진 탭은 바로 편집할 수 있다(자리 탭만 읽기 전용 · 탭 격리 적재 규칙 docs/59 §5).
struct MultiLoad {
    rx: std::sync::mpsc::Receiver<(PathBuf, u64, Result<fileload::Loaded, String>)>,
    /// 아직 안 온 결과 수.
    remaining: usize,
    total: usize,
    origin: u64,
    started: Instant,
}

struct LoadJob {
    rx: std::sync::mpsc::Receiver<Result<fileload::Loaded, String>>,
    path: PathBuf,
    mode: LoadMode,
    tab: Option<u64>,
    origin: u64,
    name: String,
    /// 읽을 양(바이트 · "앞부분만"이면 그만큼).
    total: u64,
    started: Instant,
    prog: std::sync::Arc<fileload::Progress>,
    /// 받았지만 아직 옮겨 넣지 않은 결과(100% 프레임을 기다린다).
    ready: Option<Result<fileload::Loaded, String>>,
    /// 100% 프레임을 그렸는가.
    shown_full: bool,
}

/// 되돌리기 규칙(설정 → (쉬었다 치면 새 묶음 ms, 거대 편집 확인 바이트)).
fn undo_rules(s: &Settings) -> (u64, usize) {
    (
        s.int("editor.undo_group_ms").max(0) as u64,
        (s.int("editor.undo_giant_mb").max(0) as usize) << 20,
    )
}

/// 큰 파일 단계 기준(설정 → [(바이트, 줄 수); L1, L2]).
/// 설정 `oracle.client_*` → Oracle 드라이버(첫 Oracle 접속 전에 넘긴 값이 이번 실행에 쓰인다).
fn apply_oracle_client(s: &Settings) {
    // 경로 설정의 내장 변수(`${nsqlHome}` · `${userHome}` · `${env:…}` — 프로젝트·파일은 앱 문맥이라 여기서는 없음 · 사용자 09-23).
    let m = nsql_script::intrinsic::build(&nsql_script::intrinsic::Context {
        user_home: std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from),
        app_home: nsql_settings::config_dir(),
        exec: std::env::current_exe().ok(),
        cwd: std::env::current_dir().ok(),
        ..Default::default()
    });
    nsql_drivers::set_oracle_client(
        s.get("oracle.client_mode") == Some("manual"),
        &nsql_script::intrinsic::expand(s.get("oracle.client_dir").unwrap_or(""), &m),
        &nsql_script::intrinsic::expand(s.get("oracle.tns_admin").unwrap_or(""), &m),
    );
}

/// 설정 창 DBMS 그룹의 **읽기 전용 정보 값**(`nsql_settings::INFO_KEYS`) — 파일 시스템만 읽는다(네트워크 0 · 라이브러리 로드 0).
fn dbms_info_values() -> Vec<(String, String)> {
    let o = nsql_drivers::oracle_client_info();
    // 없는 경로 = **공백**(사용자 09-21).
    let none = String::new;
    let path =
        |p: &Option<std::path::PathBuf>| p.as_ref().map_or_else(none, |x| x.display().to_string());
    let search_var = if cfg!(target_os = "windows") {
        "PATH"
    } else if cfg!(target_os = "macos") {
        "DYLD_LIBRARY_PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    let source = if !o.included {
        t(Msg::ValDbmsNotIncluded).to_string()
    } else {
        match o.source {
            "setting" => t(Msg::ValOraSrcSetting).to_string(),
            "env" => t(Msg::ValOraSrcEnv).to_string(),
            "oracle_home" => t(Msg::ValOraSrcHome).to_string(),
            "search_path" => tf(Msg::ValOraSrcPath, &[search_var]),
            "well_known" => t(Msg::ValOraSrcWellKnown).to_string(),
            _ => t(Msg::ValOraSrcNotFound).to_string(),
        }
    };
    // TNS_ADMIN이 정해진 방법 — 경로 자체는 위의 TNS_ADMIN 칸에 나온다(중복 표시 제거 · 사용자 09-21).
    let tns_how = match o.tns_source {
        "setting" => t(Msg::ValOraTnsSetting),
        "env" => t(Msg::ValOraTnsEnv),
        "client_dir" => t(Msg::ValOraTnsClientDir),
        "oracle_home" => t(Msg::ValOraTnsHome),
        _ => t(Msg::ValOraTnsNone),
    };
    // ★ 정해진 자리에 **있는가**만 말한다(사용자 09-21 — 자동이든 직접 지정이든 같다): 있음 — 경로 / 없음 — 어디에 무엇이 없는지.
    let found = |p: &std::path::PathBuf| tf(Msg::ValOraFound, &[&p.display().to_string()]);
    let missing_in = |file: &str, dir: &Option<std::path::PathBuf>| match dir {
        Some(d) => tf(Msg::ValOraMissingIn, &[file, &d.display().to_string()]),
        None => t(Msg::ValOraMissing).to_string(),
    };
    let tnsnames = match &o.tnsnames {
        Some(p) => {
            let head: Vec<&str> = o.aliases.iter().take(8).map(String::as_str).collect();
            let more = if o.aliases.len() > head.len() {
                ", …"
            } else {
                ""
            };
            tf(
                Msg::ValOraAliases,
                &[
                    &p.display().to_string(),
                    &o.aliases.len().to_string(),
                    &format!("{}{more}", head.join(", ")),
                ],
            )
        }
        None => missing_in("tnsnames.ora", &o.tns_admin),
    };
    let included = |d: Dialect, yes: Msg| {
        if nsql_drivers::available().contains(&d) {
            t(yes).to_string()
        } else {
            t(Msg::ValDbmsNotIncluded).to_string()
        }
    };
    vec![
        // 자동 탐지 방식에서 **잠긴 입력 칸**에 보여 줄 값(탐지된 폴더 · 없으면 공백) — 설정 창이 종속 조건이 안 맞을 때만 쓴다.
        ("oracle.client_dir".into(), path(&o.dir)),
        ("oracle.tns_admin".into(), path(&o.tns_admin)),
        // 판단 근거 한 줄 — 각 폴더 항목의 설명 아래에(별도 항목 "폴더가 정해진 방법"은 없앴다 · 사용자 09-21).
        (
            "oracle.client_dir#note".into(),
            tf(Msg::ValOraBasis, &[&source]),
        ),
        (
            "oracle.tns_admin#note".into(),
            tf(Msg::ValOraBasis, &[tns_how]),
        ),
        (
            "oracle.info_library".into(),
            match &o.library {
                Some(p) => found(p),
                None if o.included => missing_in(o.library_name, &o.dir),
                None => String::new(),
            },
        ),
        (
            "oracle.info_version".into(),
            o.version
                .clone()
                .unwrap_or_else(|| t(Msg::ValOraNotLoaded).to_string()),
        ),
        ("oracle.info_tnsnames".into(), tnsnames),
        (
            "oracle.info_sqlnet".into(),
            match &o.sqlnet {
                Some(p) => found(p),
                None => missing_in("sqlnet.ora", &o.tns_admin),
            },
        ),
        (
            "mssql.info_driver".into(),
            included(Dialect::Mssql, Msg::ValDbmsBuiltinMssql),
        ),
        (
            "pg.info_driver".into(),
            included(Dialect::Postgres, Msg::ValDbmsBuiltinPg),
        ),
        (
            "sqlite.info_driver".into(),
            included(Dialect::Sqlite, Msg::ValDbmsBuiltinSqlite),
        ),
    ]
}

/// 큰 파일 단계별 기능 제한 기준(설정 `file.large_ext_level` · `file.large_syntax_level` — off = 0 · l1 = 1 · l2 = 2).
fn large_feature_levels(s: &Settings) -> (u8, u8) {
    let lv = |k: &str, d: u8| match s.get(k) {
        Some("off") => 0,
        Some("l1") => 1,
        Some("l2") => 2,
        _ => d,
    };
    (
        lv("file.large_ext_level", 1),
        lv("file.large_syntax_level", 2),
    )
}

fn large_cfg(s: &Settings) -> [(usize, usize); 2] {
    let mb = |k: &str| (s.int(k).max(0) as usize) << 20;
    let n = |k: &str| s.int(k).max(0) as usize;
    [
        (mb("file.large_l1_mb"), n("file.large_l1_lines")),
        (mb("file.large_l2_mb"), n("file.large_l2_lines")),
    ]
}

/// `text` 안의 `q` 일치를 `out`에 더한다(겹치지 않게 · `base` = `text[0]`의 본문 글자 인덱스). 대소문자·단어 단위 옵션.
/// 단어 단위: 일치의 앞뒤가 단어 글자면 일치가 아니다(줄의 처음·끝은 경계다 — 그 밖은 줄바꿈이니까).
fn find_in_chars(
    text: &[char],
    base: usize,
    q: &[char],
    case_sensitive: bool,
    whole_word: bool,
    out: &mut Vec<(usize, usize)>,
) {
    let eq = |a: char, b: char| {
        if case_sensitive {
            a == b
        } else {
            a.to_lowercase().eq(b.to_lowercase())
        }
    };
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    // 한글이 든 질의 = 자모열 검색(조합 중 "ㄴ"·"내"도 "내용"에 걸린다 · nsql-core `hangul` · 사용자 09-23).
    if nsql_core::hangul::has_hangul_chars(q) {
        let qj = nsql_core::hangul::decompose_chars(q, !case_sensitive);
        let mut found = Vec::new();
        nsql_core::hangul::find_jamo(text, &qj, !case_sensitive, &mut found);
        for (s, e) in found {
            let boundary_ok = !whole_word
                || ((s == 0 || !is_word(text[s - 1])) && (e >= text.len() || !is_word(text[e])));
            if boundary_ok {
                out.push((base + s, base + e));
            }
        }
        return;
    }
    let mut i = 0;
    while i + q.len() <= text.len() {
        if text[i..i + q.len()].iter().zip(q).all(|(a, b)| eq(*a, *b)) {
            let boundary_ok = !whole_word
                || ((i == 0 || !is_word(text[i - 1]))
                    && (i + q.len() >= text.len() || !is_word(text[i + q.len()])));
            if boundary_ok {
                out.push((base + i, base + i + q.len()));
                i += q.len();
                continue;
            }
        }
        i += 1;
    }
}

/// 실행 스크립트의 문장 본문 목록(오류 이벤트의 index로 찾는다).
/// 분할은 **러너와 같은 방언 규칙**이어야 한다(index가 어긋나면 오류·결과가 다른 문장에 붙는다) — `split_script_in`.
fn split_items(src: &str, dialect: Dialect) -> Vec<String> {
    nsql_script::split_script_in(src, Some(dialect))
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

/// 거터 우클릭 메뉴 항목(순수 함수 · 사용자 09-23 정정 둘):
/// 맨 위 `bmg.view`(늘 · 좌측 북마크 패널 열기) → 줄 위에서만(`line = Some((북마크 있음, 니모닉))`)
/// `bmg.toggle`(늘 활성 · 있으면 ✓ · "제거" 항목 없음) · `bmg.mn:1..9`(없으면 만들어 지정 · 지금 것은 ✓) · `bmg.mn:clear`(니모닉이 있을 때만).
/// 마지막 줄 아래 빈 영역(`line = None`)에서는 "북마크 보기"만.
fn bm_gutter_items(line: Option<(bool, Option<u8>)>) -> Vec<nexa_ctl::controls::ctxmenu::CtxItem> {
    use nexa_ctl::controls::ctxmenu::CtxItem;
    let mut out = vec![CtxItem::item("bmg.view", t(Msg::MnBookmarksPanel))];
    let Some((has_bm, mnemonic)) = line else {
        return out;
    };
    let mn: Vec<CtxItem> = (1..=9u8)
        .map(|n| {
            CtxItem::item(format!("bmg.mn:{n}"), format!("{n}  (Ctrl+{n})"))
                .with_checked(mnemonic == Some(n))
        })
        .chain(std::iter::once(CtxItem::Separator))
        .chain(std::iter::once(CtxItem::maybe(
            "bmg.mn:clear",
            t(Msg::MnBmMnemonicClear),
            mnemonic.is_some(),
        )))
        .collect();
    // 토글 하나가 추가/제거를 다 하므로 "제거" 항목은 두지 않는다(사용자 09-23 정정) — 상태는 ✓로만 보인다.
    out.extend([
        CtxItem::Separator,
        CtxItem::item("bmg.toggle", t(Msg::MnBmToggle)).with_checked(has_bm),
        CtxItem::submenu("bmg.mn", t(Msg::MnBmMnemonic), mn),
    ]);
    out
}

#[cfg(test)]
mod bm_gutter_tests {
    use super::*;
    use nexa_ctl::controls::ctxmenu::CtxItem;

    fn enabled(items: &[CtxItem], id: &str) -> Option<bool> {
        items.iter().find_map(|it| match it {
            CtxItem::Item { id: i, enabled, .. } if i == id => Some(*enabled),
            _ => None,
        })
    }

    /// 맨 위 "북마크 보기"는 늘 · 토글 하나(늘 활성 · 있으면 ✓ · "제거" 항목 없음 · 사용자 09-23 정정) ·
    /// 니모닉 해제는 니모닉이 있을 때만 · 빈 영역(None) = 보기만.
    #[test]
    fn gutter_menu_enables_by_line_state() {
        let checked = |items: &[CtxItem], id: &str| -> Option<bool> {
            items.iter().find_map(|it| match it {
                CtxItem::Item { id: i, checked, .. } if i == id => *checked,
                _ => None,
            })
        };
        let blank = bm_gutter_items(None);
        assert_eq!(blank.len(), 1, "below text: view only");
        assert_eq!(enabled(&blank, "bmg.view"), Some(true));
        let none = bm_gutter_items(Some((false, None)));
        assert_eq!(enabled(&none, "bmg.view"), Some(true));
        assert_eq!(enabled(&none, "bmg.toggle"), Some(true));
        assert_eq!(checked(&none, "bmg.toggle"), Some(false));
        assert_eq!(enabled(&none, "bmg.remove"), None, "no remove item");
        let bm = bm_gutter_items(Some((true, None)));
        assert_eq!(enabled(&bm, "bmg.toggle"), Some(true));
        assert_eq!(checked(&bm, "bmg.toggle"), Some(true));
        let sub = |items: &[CtxItem]| -> Vec<CtxItem> {
            items
                .iter()
                .find_map(|it| match it {
                    CtxItem::Item { children, .. } if !children.is_empty() => {
                        Some(children.clone())
                    }
                    _ => None,
                })
                .expect("mnemonic submenu")
        };
        assert_eq!(enabled(&sub(&bm), "bmg.mn:clear"), Some(false));
        let with_mn = bm_gutter_items(Some((true, Some(3))));
        assert_eq!(enabled(&sub(&with_mn), "bmg.mn:clear"), Some(true));
        assert_eq!(sub(&with_mn).len(), 11, "1..9 + separator + clear");
    }
}

fn conn_tuning(settings: &Settings) -> conn_win::ConnTuning {
    let i = |k: &str| settings.int(k);
    conn_win::ConnTuning {
        delete_confirm_ms: i("conn.delete_confirm_ms").max(0) as u64,
        close_after_ms: i("conn.close_after_connect_ms").max(0) as u64,
        tooltip_ms: i("ui.tooltip_delay_ms").max(0) as u128,
        dblclick_ms: i("ui.dblclick_ms").max(0) as u128,
        copy_feedback_ms: i("ui.copy_feedback_ms"),
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
/// 인자 중 **파일**을 갈라낸다(사용자 09-22): `.nsql-project` = 프로젝트(첫 것) · 존재하는 파일 = 열 파일 ·
/// 나머지(옵션과 그 값 · 프로필 이름 · 접속 문자열)는 그대로 돌려준다. `-c`/`--connect`/`--fill` 다음 값은 파일이어도
/// 접속 대상으로 남긴다(SQLite 파일 경로일 수 있다).
/// 인자 갈라내기 → (프로젝트 파일, 파일들, **폴더**, 나머지). 폴더(`nexa-sql .` · `nexa-sql c:\…\project` · 사용자 09-23) =
/// 파일 모드의 **작업 폴더**: 북마크 등 로컬 상태를 `<폴더>/.nsql/`에 둔다(없으면 `%APPDATA%` 전역). 첫 폴더만 · 절대 경로로.
fn split_file_args(
    args: &[String],
) -> (Option<PathBuf>, Vec<PathBuf>, Option<PathBuf>, Vec<String>) {
    let mut project = None;
    let mut files = Vec::new();
    let mut folder: Option<PathBuf> = None;
    let mut rest = Vec::new();
    let mut keep_next = false;
    for a in args {
        if keep_next {
            rest.push(a.clone());
            keep_next = false;
            continue;
        }
        if matches!(a.as_str(), "-c" | "--connect" | "--fill") {
            keep_next = true;
            rest.push(a.clone());
            continue;
        }
        if a.starts_with('-') {
            rest.push(a.clone());
            continue;
        }
        let p = PathBuf::from(a);
        if p.extension().is_some_and(|e| e == project::EXT) {
            if project.is_none() {
                project = Some(p);
            }
        } else if p.is_file() {
            files.push(p);
        } else if p.is_dir() {
            if folder.is_none() {
                folder = Some(std::path::absolute(&p).unwrap_or(p));
            }
        } else {
            rest.push(a.clone());
        }
    }
    (project, files, folder, rest)
}

/// 시작 때 열 프로젝트 — `(경로, 인자로 받았는가)`. 규칙(사용자 09-22): ① 인자 프로젝트가 있으면 그것 ② 아니면
/// `restore_last`가 켜져 있고 **첫 인스턴스**이고 **파일 인자가 없을 때만** `last` ③ 그 밖 = 파일 모드(None).
fn startup_project_plan(
    arg_project: Option<&Path>,
    has_files: bool,
    first_instance: bool,
    restore_last: bool,
    last: &str,
) -> Option<(PathBuf, bool)> {
    if let Some(p) = arg_project {
        return Some((p.to_path_buf(), true));
    }
    if restore_last && first_instance && !has_files && !last.trim().is_empty() {
        return Some((PathBuf::from(last.trim()), false));
    }
    None
}

/// 인스턴스 잠금 — 설정 폴더의 `instance.lock`을 배타 잠금(`File::try_lock` · 3-OS 표준 라이브러리). 잡히면 첫 인스턴스
/// (파일을 살아 있는 동안 쥔다 · 프로세스가 끝나면 OS가 푼다) · 못 잡으면 이미 다른 인스턴스가 있다. 폴더를 모르면 첫 것으로.
fn instance_lock() -> Option<std::fs::File> {
    let dir = nsql_settings::config_dir()?;
    let _ = std::fs::create_dir_all(&dir);
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("instance.lock"))
        .ok()?;
    match f.try_lock() {
        Ok(()) => Some(f),
        Err(_) => None,
    }
}

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

    /// 파일 인자 분류(09-22): 프로젝트 파일 · 존재하는 파일 · 접속 인자(`-c` 뒤는 파일이어도 접속 대상).
    #[test]
    fn file_args_are_split_from_connect_args() {
        let dir = std::env::temp_dir().join(format!("nsql-args-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("a.sql");
        std::fs::write(&f, "x").unwrap_or(());
        let fs = f.to_string_lossy().into_owned();
        let pj = dir.join("p.nsql-project").to_string_lossy().into_owned();
        let v: Vec<String> = vec![
            fs.clone(),
            "-c".into(),
            fs.clone(),
            pj.clone(),
            "Demo".into(),
            "--x".into(),
        ];
        let (p, files, folder, rest) = split_file_args(&v);
        assert_eq!(p, Some(PathBuf::from(&pj)));
        assert_eq!(files, vec![f]);
        assert_eq!(folder, None, "파일·프로젝트 인자만 = 폴더 없음");
        // 폴더 인자(사용자 09-23 `nexa-sql .`) = 첫 폴더 · 절대 경로 · 나머지 인자에는 안 남는다.
        let (_, _, folder2, rest2) =
            split_file_args(&[dir.to_string_lossy().into_owned(), "Demo".into()]);
        assert_eq!(folder2.as_deref(), Some(dir.as_path()));
        assert_eq!(rest2, vec!["Demo".to_string()]);
        assert_eq!(
            rest,
            vec!["-c".to_string(), fs, "Demo".into(), "--x".into()]
        );
        let (p2, files2, folder3, rest3) = split_file_args(&[]);
        assert!(p2.is_none() && files2.is_empty() && folder3.is_none() && rest3.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 시작 모드 MC/DC(사용자 09-22): 인자 프로젝트 > (복원 켬 · 첫 인스턴스 · 파일 인자 없음 · last 있음) > 파일 모드.
    #[test]
    fn startup_project_plan_rules() {
        let pj = Path::new("x.nsql-project");
        // 인자 프로젝트 = 다른 조건과 무관하게 그것.
        assert_eq!(
            startup_project_plan(Some(pj), true, false, false, ""),
            Some((pj.to_path_buf(), true))
        );
        // 복원 조건 전부 참 = last.
        assert_eq!(
            startup_project_plan(None, false, true, true, "last.nsql-project"),
            Some((PathBuf::from("last.nsql-project"), false))
        );
        // 조건 하나씩 거짓 = 파일 모드.
        assert_eq!(
            startup_project_plan(None, true, true, true, "last.nsql-project"),
            None,
            "파일 인자"
        );
        assert_eq!(
            startup_project_plan(None, false, false, true, "last.nsql-project"),
            None,
            "이후 인스턴스"
        );
        assert_eq!(
            startup_project_plan(None, false, true, false, "last.nsql-project"),
            None,
            "복원 끔(기본)"
        );
        assert_eq!(
            startup_project_plan(None, false, true, true, "  "),
            None,
            "last 없음"
        );
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
