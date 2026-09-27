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
#[cfg(all(unix, not(target_os = "macos")))]
mod clipboard_x11;

/// 설정 `clipboard.x11_native`의 사본 — 클립보드 함수는 `&self` 없이 불리므로 전역에 둔다(input.rs `NATURAL`과 같은 꼴).
static CLIP_NATIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
// 읽는 쪽은 Linux X11 경로(`clipboard.rs`)뿐 — macOS·Windows에서는 dead_code(CI `-D warnings` · 09-27 102차).
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn settings_clip_native() -> bool {
    CLIP_NATIVE.load(std::sync::atomic::Ordering::Relaxed)
}
fn set_clip_native(on: bool) {
    CLIP_NATIVE.store(on, std::sync::atomic::Ordering::Relaxed);
}
mod about_win;
mod colors_win;
mod conn_win;
mod connect;
mod copybtn;
mod dbms_icons;
mod editable;
mod editors;
mod enc;
mod eol;
mod exp_icons;
mod explorer;
mod explorers;
mod ext_panel;
mod ext_view;
// 09-17 레인보우 플러그인 모듈 · 배선(설정→편집기 · 키맵 · 메뉴)은 다음 세션(T-119)
mod extensions;
mod extfile;
mod file_win;
mod fileload;
mod filterbar;
mod findbar;
mod gitstat;
mod grid;
mod gridedit_sql;
mod icon;
mod imehint;
mod imestate;
#[cfg(all(unix, not(target_os = "macos")))]
mod imewatch;
mod import_win;
mod input;
mod input_win;
mod intel;
mod intel_card;
mod keymap;
mod keys_win;
mod license_win;
mod log_win;
mod mem_win;
mod memstat;
mod memtrim;
mod metacache;
mod objdetail;
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

use about_win::{AboutAction, AboutWin};
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
use license_win::{LicAction, LicView, LicenseWin};
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
    /// 객체 상세 패널(docs/86 · 09-25).
    Details,
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
    /// ★ 라이선스 창(docs/23 §4-4 · T-34) + 판정 문맥(단일 원천 · `check(Feature)`는 여기만).
    license_win: LicenseWin,
    licensing: nsql_license::Licensing,
    open_license: bool,
    /// ★ About 창(Help ▸ About · 09-27).
    about_win: AboutWin,
    open_about: bool,
    status_lic_rect: Rect,
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
    /// ★ 읽기 전용 글 창 요청(그리드 편집 SQL 미리보기 · 셀 값 보기 · docs/87): (제목, 본문).
    sqlprev_plain: Option<(String, String)>,
    /// ★ Import 창(89 §3-3 · T-236 B-3): 대상 (표 표기 · 파일 · 탐색기 칸의 서버) · 열 차례 · 취소 깃발 · 요청 번호.
    import_win: import_win::ImportWin,
    import_ctx: Option<(String, PathBuf, Option<ConnectSpec>)>,
    import_pending: bool,
    import_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    import_key: u64,
    /// 값 보기 창 열기 요청(그리드 → 다음 틱에 값 모드로 · 87 §5).
    sqlprev_value: Option<grid::ValueReq>,
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
    /// 파일 창을 띄운 보조 창(라이선스 창 · 설정 창) — 닫히면 **그 창으로** 포커스를 돌린다(없으면 메인 · 09-27).
    picker_return: Option<WindowId>,
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
    /// "모두 저장"(닫기/종료 물음) 진행 중 — 이름 없는 탭의 저장 창이 끝나면 다음 미저장 탭으로 이어 간다(사용자 09-26).
    save_all_pending: bool,
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
    /// ★ 객체 상세 패널(docs/86 · T-223) — 탐색기 아래 독립 영역 · 스플리터 `split_d`.
    objdetail: objdetail::DetailPanel,
    split_d: Splitter,
    detail_key: Option<String>,
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

/// `App`의 기능별 `impl` 조각(docs/93 §4 — main.rs 거대 객체 분할).
mod app;

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
        // ★ 객체 상세(docs/86): 탐색기 아래 독립 영역 — 탐색기 높이를 그만큼 줄인다(스크롤 영역이 겹치지 않게).
        let details_on = self.explorer.is_visible() && self.settings.flag("explorer.details");
        self.objdetail.set_visible(details_on);
        // 스플리터 띠 = 확장 상태에만(축소 = 머리 줄이 바닥에 붙고 띠 없음 · 사용자 09-26) · 최소 = 머리 줄 + 4줄(`min_h`).
        let collapsed = self.objdetail.is_collapsed();
        let grip = if details_on && !collapsed {
            px(SPLIT_GRIP, s)
        } else {
            0
        };
        let dh = if details_on {
            if collapsed {
                objdetail::DetailPanel::head_h(s)
            } else {
                let min_h = self.objdetail.min_h();
                px(self.settings.int("explorer.details_h") as f32, s)
                    .clamp(min_h, (body_h * 3 / 4).max(min_h))
            }
        } else {
            0
        };
        let exp_h = if details_on {
            (body_h - dh - grip).max(0)
        } else {
            body_h
        };
        self.explorer.set_bounds(
            Rect::new(
                act_w,
                body_top,
                if self.explorer.is_visible() { exp_w } else { 0 },
                exp_h,
            ),
            s,
        );
        if details_on {
            self.split_d.set_rect(if collapsed {
                Rect::default()
            } else {
                Rect::new(act_w, body_top + exp_h, exp_w, grip)
            });
            self.objdetail
                .set_bounds(Rect::new(act_w, body_top + exp_h + grip, exp_w, dh), s);
        } else {
            self.split_d.set_rect(Rect::default());
            self.objdetail.set_bounds(Rect::default(), s);
        }
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

    // ── 툴바 그룹 도크 · 플로팅(사용자 09-17)

    // ───────────────────────── 다중 열기(사용자 09-22) ──────────

    // ───────────────────────── 프로젝트(docs/67 · T-165 · 사용자 09-22) ──────────

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

    // ───────────────────────── 세션 컨텍스트(docs/52) ─────────────────────────

    // ───────────────────────── 파일 검색(T-81a · docs/36) ─────────────────────────

    // ───────────────────────── 트랜잭션 UX(DR-30 · T-77 · docs/34) ─────────────────────────

    // ───────────────────────── 메모리 회수(사용자 09-19) ─────────────────────────

    // ───────────────────────── 외부 파일 변경(docs/58 · T-140) ─────────────────────────

    // ───────────────────────── 코드 완성 · 아웃라인(docs/76) ─────────────────────────

    // ───────────────────────── 파일 열기/저장(T-74) ─────────────────────────

    // ── ★ 라이선스 게이트(docs/23 §4-2 · 25 §13-3 · T-36 · D-41/D-43/D-46 권장안 확정 09-27)
    //   규칙: 기능당 **정확히 한 곳**(UI 행위 진입점) · 깊은 곳 중복 검사 없음 · Release = 늘 켬 · Debug = `license.gates_dev`.

    // ── 데모 프로필·샘플 데이터(사용자 09-17 · docs/21 §5)

    // ───────────── 북마크(docs/69 · T-167) ─────────────
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
        // ★ 아이콘 마스크 선굽기(09-25 §203): 첫 표시 때 UI 스레드가 래스터화로 멎지 않게(Debug 1.26 s) — 별 스레드 · 결과는 전역 캐시.
        let _ = std::thread::Builder::new()
            .name("nsql-icons".into())
            .spawn(|| {
                toolicons::prewarm();
                exp_icons::prewarm();
            });
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
        license_win: LicenseWin::new(),
        about_win: AboutWin::new(),
        open_about: false,
        licensing: nsql_license::Licensing::open_default(),
        open_license: false,
        status_lic_rect: Rect::new(0, 0, 0, 0),
        mem_win: mem_win::MemWin::new(),
        sqlprev_win: sqlprev_win::SqlPrevWin::new(),
        sqlprev_pending: None,
        sqlprev_plain: None,
        import_win: import_win::ImportWin::new(),
        import_ctx: None,
        import_pending: false,
        import_cancel: None,
        import_key: 0,
        sqlprev_value: None,
        input_win,
        input_pending: None,
        pw_pending: None,
        close_after_save: None,
        save_all_pending: false,
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
        picker_return: None,
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
        // 시작 시 로그인 창 — 인자로 접속 대상을 줬으면 띄우지 않는다.
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
        objdetail: objdetail::DetailPanel::new(),
        split_d: Splitter::new(SplitAxis::Horizontal),
        detail_key: None,
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
    app.apply_grid_edit_cfg();
    app.apply_click_policy();
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
        #[cfg(all(unix, not(target_os = "macos")))]
        imewatch::set_enabled(app.settings.flag("ui.ime_hint_watch"));
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
    set_clip_native(app.settings.flag("clipboard.x11_native"));
    // 화면 내보내기 방식(T-147) — 첫 창이 만들어지기 전에.
    present::set_mode(app.settings.get("gfx.mac_present").unwrap_or("softbuffer"));
    // 객체 상세 패널(docs/86): 편집기 탭과 같은 상자(글꼴 지표·줄 간격 일치 · 09-25 캡처의 줄 겹침·하단 미표시) + 축소 상태(설정 기억).
    let plain = app.editors.syntax_for_title("detail.txt");
    app.objdetail.set_box(app.editors.preview_box("", &plain));
    app.objdetail
        .set_collapsed(app.settings.flag("explorer.details_collapsed"));
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
        warm_comments: settings.flag("meta.warm_comments"),
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
    /// 값 보기 창: 셀 값을 파일로 저장 / 파일에서 넣기(docs/87 §5).
    CellValue,
    /// ★ Import 창의 원료 파일 고르기(docs/89 §3-3).
    Import,
    /// ★ 라이선스 파일 열기(docs/23 §1-3 · 라이선스 창).
    License,
}

/// 파일에서 셀에 넣은 결과 = (이진 바이트, 글, 바이트 수).
type CellLoad = (Option<Vec<u8>>, Option<String>, usize);

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
