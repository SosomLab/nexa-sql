//! `nsql-i18n` — 메시지 카탈로그(T-37 · 사용자 09-14 *"i18n 기능 · 기본은 영어"*).
//!
//! 이식 원본: `nexa-clip/crates/nclip-core/src/i18n.rs`(← nexa-beep) **형태**만 —
//! `Lang`·`Msg`·`tr`·`set_lang`·`current_lang`·`tf`. 언어는 **영어(기본)·한국어** 2열(DR-19 한글 1급).
//!
//! 원칙
//! - 전부 `&'static str` 컴파일 타임 표 — 파일 로드·힙 0 · 외부 crate 0. 새 문자열 = `Msg` 1줄 + `row` 1줄.
//! - 한국어 칸이 비면 **영어로 폴백**(빈 칸이 화면에 나가지 않는다 · `all_rows_have_english` 테스트가 영어 칸은 강제).
//! - 현재 언어는 프로세스 전역(`AtomicU8`) — UI·CLI·워커 어디서나 `t(Msg)`.
//! - 자리표시자는 `{0}` `{1}` … — [`tf`]가 치환한다(`format!` 리터럴 제약 회피).

use std::sync::atomic::{AtomicU8, Ordering};

/// 지원 언어 — 첫 항목이 기본(영어 · 사용자 09-14).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Lang {
    #[default]
    En = 0,
    Ko = 1,
}

impl Lang {
    /// 전체(설정 후보 순서).
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Ko];

    /// 설정 값·CLI 인자용 코드.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ko => "ko",
        }
    }

    /// `en`·`ko`(대소문자·`ko-KR`식 지역 접미 허용).
    #[must_use]
    pub fn from_code(s: &str) -> Option<Lang> {
        let s = s.trim();
        let base = s.split(['-', '_']).next().unwrap_or(s).to_ascii_lowercase();
        match base.as_str() {
            "en" => Some(Lang::En),
            "ko" => Some(Lang::Ko),
            _ => None,
        }
    }

    /// 자기 언어로 쓴 이름(설정 드롭다운용 — 번역하지 않는다).
    #[must_use]
    pub const fn endonym(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ko => "한국어",
        }
    }

    const fn column(self) -> usize {
        self as usize
    }

    /// 다음 언어(단축키 순환용).
    #[must_use]
    pub const fn next(self) -> Lang {
        match self {
            Lang::En => Lang::Ko,
            Lang::Ko => Lang::En,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(Lang::En as u8);

/// 프로세스 전역 언어 설정(부팅 시 설정 파일에서 · 런타임 전환).
pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

/// 현재 언어.
#[must_use]
pub fn current_lang() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Ko,
        _ => Lang::En,
    }
}

/// 메시지 키 — 화면·CLI·설정 라벨 전부. 접두: `Ph`(placeholder) `Btn` `St`(상태줄) `Err` `Wk`(워커) `Cfg`(CLI config) `Cat`/`Lbl`/`Desc`/`Val`(설정 레지스트리) `Ctx`(컨트롤 메뉴).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Msg {
    // ── GUI 컨트롤
    PhProfileName,
    PhConnect,
    PhEditor,
    BtnSave,
    BtnConnect,
    BtnRun,
    // ── GUI 상태줄
    StInitial,
    StProfiles,
    StSaving,
    StRunning,
    StConnecting,
    StRows,
    StRowsAffected,
    StOk,
    StConnected,
    StDisconnected,
    StErrorLine,
    ErrClipboard,
    StHint,
    StThemeChanged,
    StLangChanged,
    ErrProfileName,
    ErrNeedTarget,
    ErrNoSql,
    ErrEnterTarget,
    ErrNoFont,
    ErrNoWindow,
    ErrEventLoop,
    // ── 접속 패널
    LblProfile,
    ValNewProfile,
    LblDbType,
    LblHost,
    LblPort,
    LblDatabase,
    LblService,
    LblFile,
    LblDsn,
    LblUser,
    LblPassword,
    LblSavePassword,
    LblProfileName,
    PhHost,
    PhDatabase,
    PhService,
    PhSqliteFile,
    PhDsn,
    PhUser,
    PhPassword,
    BtnTest,
    BtnDisconnect,
    StIdle,
    StConnectingShort,
    StTesting,
    StConnectedShort,
    StTestOk,
    StTestFailed,
    StConnectFailed,
    // ── 워커
    WkProfileSaved,
    WkProfileSaveFailed,
    WkErrors,
    // ── 컨트롤 내장 메뉴(nexa-ctl 이음새)
    CtxSelectAll,
    CtxCopy,
    CtxCut,
    CtxPaste,
    // ── 설정 레지스트리
    CatAppearance,
    CatEditor,
    LblLang,
    DescLang,
    LblTheme,
    DescTheme,
    LblUiFontSize,
    DescUiFontSize,
    LblEditorFontSize,
    DescEditorFontSize,
    ValSystem,
    ValLight,
    ValDark,
    ValNoDriver,
    LblEditorLineNumbers,
    DescEditorLineNumbers,
    LblGridRowNumbers,
    DescGridRowNumbers,
    MnFile,
    MnEdit,
    MnView,
    MnRun,
    MnHelp,
    MnNew,
    MnExit,
    MnUndoNa,
    MnCut,
    MnCopy,
    MnPaste,
    MnSelectAll,
    MnLogWindow,
    MnTheme,
    MnLanguage,
    MnRunStatement,
    MnRunAll,
    MnConnect,
    MnDisconnect,
    MnAbout,
    TipStatements,
    TipLines,
    TipChars,
    TipConnection,
    TipNew,
    TipRunStatement,
    TipRunAll,
    TipConnect,
    TipLog,
    LblTabsRows,
    DescTabsRows,
    ValTabsSingle,
    ValTabsMulti,
    LblTabsTooltip,
    DescTabsTooltip,
    CatExplorer,
    LblExplorerAutoRefresh,
    DescExplorerAutoRefresh,
    LblExplorerRefreshSecs,
    DescExplorerRefreshSecs,
    LblExplorerTimeout,
    DescExplorerTimeout,
    LblExplorerTooltip,
    DescExplorerTooltip,
    LblGridScroll,
    DescGridScroll,
    ValScrollPixel,
    ValScrollRow,
    CatSession,
    LblSessionMode,
    DescSessionMode,
    ValSessionShared,
    ValSessionPerEditor,
    CatLog,
    LblLogFormat,
    CatConnection,
    LblProbeEnabled,
    DescProbeEnabled,
    LblProbeMaxRetries,
    DescProbeMaxRetries,
    LblProbeTimeout,
    DescProbeTimeout,
    LblProbeInterval,
    DescProbeInterval,
    LblProbeRetryDelay,
    DescProbeRetryDelay,
    ErrServerUnreachable,
    WkPanic,
    CatWindow,
    LblWindowFocus,
    DescWindowFocus,
    ValFocusGroup,
    ValFocusSingle,
    MnCommandPalette,
    PhPalette,
    PalSetSyntax,
    PalNoMatch,
    StSyntaxSet,
    StPos,
    StRowsShort,
    LblCopyRich,
    DescCopyRich,
    LblRulers,
    DescRulers,
    LblWhitespace,
    DescWhitespace,
    ValWsNone,
    ValWsSelection,
    ValWsAll,
    LblWsChars,
    DescWsChars,
    LblWsColor,
    DescWsColor,
    LblWsAlpha,
    DescWsAlpha,
    WinLogin,
    LblLoginList,
    BtnNew,
    BtnEdit,
    BtnDelete,
    BtnClose,
    PhFilter,
    ColName,
    ColType,
    ColUser,
    ColTarget,
    StNoProfiles,
    StProfileDeleted,
    DescLogFormat,
    ValRaw,
    ValMarkdown,
    ValGrid,
    // ── CLI `nsql config`
    CfgUsage,
    CfgUnknownKey,
    CfgInvalidValue,
    CfgSet,
    CfgReset,
    CfgOpenFailed,
    CfgSaveFailed,
    CfgNoConfigDir,
    CfgDefaultMark,
}

impl Msg {
    /// `[영어, 한국어]` — 한국어 칸이 비면 영어 폴백.
    const fn row(self) -> [&'static str; 2] {
        match self {
            Msg::PhProfileName => ["Profile name", "프로필 이름"],
            Msg::PhConnect => [
                "Profile name · sqlite::memory: · oracle://user:pass@host:1521/svc · mssql://user:pass@host:1433/db",
                "프로필 이름 · sqlite::memory: · oracle://user:pass@host:1521/svc · mssql://user:pass@host:1433/db",
            ],
            Msg::PhEditor => ["SELECT … ;  EXEC :V := 'x';  PRINT V", ""],
            Msg::BtnSave => ["Save", "저장"],
            Msg::BtnConnect => ["Connect", "접속"],
            Msg::BtnRun => ["Run ▶", "실행 ▶"],
            Msg::StInitial => [
                "Fill in the connection panel and press Connect (Test checks without keeping a session)",
                "접속 패널을 채우고 Connect를 누르세요(Test는 세션을 유지하지 않고 확인만)",
            ],
            Msg::StProfiles => [
                "Saved profiles: {0} — pick one in the Profile box",
                "저장된 프로필: {0} — 프로필 칸에서 고르세요",
            ],
            Msg::StSaving => ["Saving profile… {0}", "프로필 저장 중… {0}"],
            Msg::StRunning => ["Running…", "실행 중…"],
            Msg::StConnecting => ["Connecting… {0}", "접속 중… {0}"],
            Msg::StRows => ["{0} rows · {1}s", "{0}행 · {1}s"],
            Msg::StRowsAffected => ["{0} rows affected · {1}s", "{0}행 반영 · {1}s"],
            Msg::StOk => ["OK · {0}s", "OK · {0}s"],
            Msg::StConnected => ["Connected: {0} ({1})", "접속: {0} ({1})"],
            Msg::StDisconnected => ["Disconnected", "접속 해제"],
            Msg::StErrorLine => ["ERROR line {0}: {1}", "오류 {0}행: {1}"],
            Msg::ErrClipboard => [
                "Clipboard unavailable (Linux: install wl-clipboard or xclip)",
                "클립보드를 사용할 수 없습니다(Linux: wl-clipboard 또는 xclip 설치)",
            ],
            Msg::StHint => [
                "⌘/Ctrl+Enter run · F5 all · ⌘/Ctrl+L connection · ⌘/Ctrl+⇧G log · ⌘/Ctrl+⇧T theme · ⌘/Ctrl+⇧L language",
                "⌘/Ctrl+Enter 실행 · F5 전체 · ⌘/Ctrl+L 접속 · ⌘/Ctrl+⇧G 로그 · ⌘/Ctrl+⇧T 테마 · ⌘/Ctrl+⇧L 언어",
            ],
            Msg::StThemeChanged => ["Theme: {0}", "테마: {0}"],
            Msg::StLangChanged => ["Language: {0}", "언어: {0}"],
            Msg::ErrProfileName => [
                "Enter a profile name (letters, digits, `_ - .` · up to 64 chars)",
                "프로필 이름을 입력하세요(영문·숫자·`_ - .` · 64자 이내)",
            ],
            Msg::ErrNeedTarget => [
                "Enter a connection string to save (e.g. oracle://user:pass@host:1521/svc)",
                "저장할 접속 문자열을 입력하세요 (예: oracle://user:pass@host:1521/svc)",
            ],
            Msg::ErrNoSql => ["Nothing to run", "실행할 SQL이 없습니다"],
            Msg::ErrEnterTarget => [
                "Enter a connection string (e.g. sqlite::memory: · oracle://user:pass@host:1521/svc)",
                "접속 문자열을 입력하세요 (예: sqlite::memory: · oracle://user:pass@host:1521/svc)",
            ],
            Msg::ErrNoFont => [
                "No system font found (check the nexa-font candidate list)",
                "시스템 폰트를 찾지 못했습니다(nexa-font 후보 목록 확인)",
            ],
            Msg::ErrNoWindow => ["Failed to create the window", "창 생성 실패"],
            Msg::ErrEventLoop => ["Event loop error: {0}", "이벤트 루프 오류: {0}"],
            Msg::LblProfile => ["Profile", "프로필"],
            Msg::ValNewProfile => ["(new connection)", "(새 접속)"],
            Msg::LblDbType => ["Database type", "DB 종류"],
            Msg::LblHost => ["Host", "호스트"],
            Msg::LblPort => ["Port", "포트"],
            Msg::LblDatabase => ["Database", "데이터베이스"],
            Msg::LblService => ["Service name / SID", "서비스명 / SID"],
            Msg::LblFile => ["File", "파일"],
            Msg::LblDsn => ["DSN", "DSN"],
            Msg::LblUser => ["User", "사용자"],
            Msg::LblPassword => ["Password", "비밀번호"],
            Msg::LblSavePassword => ["Save password", "비밀번호 저장"],
            Msg::LblProfileName => ["Profile name", "프로필 이름"],
            Msg::PhHost => ["host or IP", "호스트 또는 IP"],
            Msg::PhDatabase => ["database name", "데이터베이스 이름"],
            Msg::PhService => ["e.g. ORCLPDB1", "예: ORCLPDB1"],
            Msg::PhSqliteFile => [":memory: or path/to/file.db", ":memory: 또는 파일 경로"],
            Msg::PhDsn => ["ODBC DSN", "ODBC DSN"],
            Msg::PhUser => ["user", "사용자"],
            Msg::PhPassword => ["password", "비밀번호"],
            Msg::BtnTest => ["Test", "테스트"],
            Msg::BtnDisconnect => ["Disconnect", "접속 해제"],
            Msg::StIdle => ["Not connected", "접속 안 됨"],
            Msg::StConnectingShort => ["Connecting…", "접속 중…"],
            Msg::StTesting => ["Testing connection…", "접속 테스트 중…"],
            Msg::StConnectedShort => ["Connected · {0}", "접속됨 · {0}"],
            Msg::StTestOk => ["Test OK · {0} · {1}s", "테스트 성공 · {0} · {1}s"],
            Msg::StTestFailed => ["Test failed: {0}", "테스트 실패: {0}"],
            Msg::StConnectFailed => ["Connection failed: {0}", "접속 실패: {0}"],
            Msg::WkProfileSaved => [
                "Profile saved: {0} = {1} — now just type '{0}' in the connection box",
                "프로필 저장: {0} = {1} — 이제 접속 칸에 '{0}'만 넣어도 됩니다",
            ],
            Msg::WkProfileSaveFailed => ["Profile save failed: {0}", "프로필 저장 실패: {0}"],
            Msg::WkErrors => ["{0} error(s)", "오류 {0}건"],
            Msg::CtxSelectAll => ["Select All", "전체 선택"],
            Msg::CtxCopy => ["Copy", "복사"],
            Msg::CtxCut => ["Cut", "잘라내기"],
            Msg::CtxPaste => ["Paste", "붙여넣기"],
            Msg::CatAppearance => ["Appearance", "모양"],
            Msg::CatEditor => ["Editor", "편집기"],
            Msg::LblLang => ["Language", "언어"],
            Msg::DescLang => [
                "Language of the user interface (applies immediately)",
                "화면 언어(즉시 적용)",
            ],
            Msg::LblTheme => ["Theme", "테마"],
            Msg::DescTheme => [
                "Color theme. System follows the OS light/dark mode",
                "색 테마. System은 OS의 라이트/다크 모드를 따릅니다",
            ],
            Msg::LblUiFontSize => ["UI font size", "UI 글꼴 크기"],
            Msg::DescUiFontSize => [
                "Font size in pixels for toolbars, fields and the status bar",
                "툴바·입력 칸·상태줄 글꼴 크기(px)",
            ],
            Msg::LblEditorFontSize => ["Editor font size", "편집기 글꼴 크기"],
            Msg::DescEditorFontSize => [
                "Font size in pixels for the SQL editor and the result grid (monospace)",
                "SQL 편집기·결과 그리드 글꼴 크기(px · 고정폭)",
            ],
            Msg::ValSystem => ["System", "시스템"],
            Msg::ValLight => ["Light", "라이트"],
            Msg::ValDark => ["Dark", "다크"],
            Msg::ValNoDriver => ["(no driver in this build)", "(이 빌드에 드라이버 없음)"],
            Msg::LblEditorLineNumbers => ["Editor line numbers", "편집기 줄번호"],
            Msg::DescEditorLineNumbers => ["Show line numbers in the SQL editor gutter", "SQL 편집기 왼쪽에 줄번호 표시"],
            Msg::LblGridRowNumbers => ["Result row numbers", "결과 행번호"],
            Msg::DescGridRowNumbers => ["Show a row-number column in the result grid", "결과 그리드에 행번호 열 표시"],
            Msg::MnFile => ["File", "파일"],
            Msg::MnEdit => ["Edit", "편집"],
            Msg::MnView => ["View", "보기"],
            Msg::MnRun => ["Run", "실행"],
            Msg::MnHelp => ["Help", "도움말"],
            Msg::MnNew => ["New editor (clear)", "새 편집(비우기)"],
            Msg::MnExit => ["Exit", "종료"],
            Msg::MnUndoNa => ["Undo (coming with editor E1)", "실행 취소(편집기 E1에서)"],
            Msg::MnCut => ["Cut", "잘라내기"],
            Msg::MnCopy => ["Copy", "복사"],
            Msg::MnPaste => ["Paste", "붙여넣기"],
            Msg::MnSelectAll => ["Select All", "전체 선택"],
            Msg::MnLogWindow => ["Log window", "로그 창"],
            Msg::MnTheme => ["Cycle theme", "테마 순환"],
            Msg::MnLanguage => ["Toggle language", "언어 전환"],
            Msg::MnRunStatement => ["Run statement", "문장 실행"],
            Msg::MnRunAll => ["Run all", "전체 실행"],
            Msg::MnConnect => ["Connect", "접속"],
            Msg::MnDisconnect => ["Disconnect", "접속 해제"],
            Msg::MnAbout => ["About Nexa SQL", "Nexa SQL 정보"],
            Msg::TipStatements => ["statements", "문장"],
            Msg::TipLines => ["lines", "줄"],
            Msg::TipChars => ["chars", "글자"],
            Msg::TipConnection => ["connection", "접속"],
            Msg::TipNew => ["New (clear editor)", "새 편집(비우기)"],
            Msg::TipRunStatement => ["Run statement (Ctrl+Enter)", "문장 실행(Ctrl+Enter)"],
            Msg::TipRunAll => ["Run all (F5)", "전체 실행(F5)"],
            Msg::TipConnect => ["Connect / Disconnect", "접속 / 해제"],
            Msg::TipLog => ["Log window (Ctrl+Shift+G)", "로그 창(Ctrl+Shift+G)"],
            Msg::LblTabsRows => ["Editor tab rows", "편집기 탭 줄"],
            Msg::DescTabsRows => [
                "single = one row with ◀ ▶ scroll buttons and drag to reorder · multi = wrap tabs into several rows",
                "single = 한 줄(◀ ▶ 스크롤 · 드래그 이동) · multi = 여러 줄로 접기",
            ],
            Msg::ValTabsSingle => ["Single row", "한 줄"],
            Msg::ValTabsMulti => ["Multiple rows", "여러 줄"],
            Msg::LblTabsTooltip => ["Tab tooltip", "탭 툴팁"],
            Msg::DescTabsTooltip => [
                "Show a tooltip when hovering a tab (script path · connection · type · URL · schema · project)",
                "탭에 마우스를 올리면 설명 표시(스크립트 경로 · 접속 · 종류 · URL · 스키마 · 프로젝트)",
            ],
            Msg::CatExplorer => ["Object explorer", "오브젝트 탐색기"],
            Msg::LblExplorerAutoRefresh => ["Auto refresh", "자동 갱신"],
            Msg::DescExplorerAutoRefresh => [
                "Re-query expanded nodes periodically (off = refresh only on F5 / context menu)",
                "펼쳐진 노드를 주기적으로 재조회(off = F5·우클릭 새로 고침만)",
            ],
            Msg::LblExplorerRefreshSecs => ["Auto refresh interval (s)", "자동 갱신 주기(초)"],
            Msg::DescExplorerRefreshSecs => ["Seconds between automatic refreshes when enabled", "자동 갱신을 켰을 때의 주기(초)"],
            Msg::LblExplorerTimeout => ["Metadata timeout (s)", "메타 조회 타임아웃(초)"],
            Msg::DescExplorerTimeout => ["A node that takes longer shows an error and can be retried", "이 시간을 넘긴 노드는 오류로 표시되고 다시 시도할 수 있습니다"],
            Msg::LblExplorerTooltip => ["Node tooltip", "노드 툴팁"],
            Msg::DescExplorerTooltip => ["Show a card (kind · schema · owner · last fetched) when hovering a node", "노드에 마우스를 올리면 카드(종류·스키마·소유자·마지막 조회) 표시"],
            Msg::LblGridScroll => ["Grid scrolling", "그리드 스크롤"],
            Msg::DescGridScroll => [
                "pixel = smooth, any offset (default) · row = snap to whole rows (result grid and log window)",
                "pixel = 픽셀 단위 부드럽게(기본) · row = 행 경계에 맞춤(결과 그리드·로그 창)",
            ],
            Msg::ValScrollPixel => ["Pixel", "픽셀"],
            Msg::ValScrollRow => ["Row", "행"],
            Msg::CatSession => ["Session", "세션"],
            Msg::LblSessionMode => ["Session per editor", "편집기별 세션"],
            Msg::DescSessionMode => [
                "shared = all editor tabs use one connection session · per-editor = each editor tab opens its own session (transactions and session variables are isolated)",
                "shared = 모든 편집기 탭이 하나의 접속 세션 공유 · per-editor = 편집기 탭마다 자기 세션(트랜잭션·세션 변수 분리)",
            ],
            Msg::ValSessionShared => ["Shared", "공유"],
            Msg::ValSessionPerEditor => ["Per editor", "편집기별"],
            Msg::CatLog => ["Log", "로그"],
            Msg::LblLogFormat => ["Log format", "로그 형식"],
            Msg::DescLogFormat => [
                "How the execution log window and `nsql --log` render entries (raw text, Markdown table, aligned grid)",
                "실행 로그 창과 `nsql --log`의 표현(원문 텍스트 · Markdown 표 · 정렬 그리드)",
            ],
            Msg::ValRaw => ["Raw", "원문"],
            Msg::ValMarkdown => ["Markdown", "Markdown"],
            Msg::ValGrid => ["Grid", "그리드"],
            Msg::CatConnection => ["Connection", "접속"],
            Msg::LblProbeEnabled => ["Server status light", "서버 상태 신호등"],
            Msg::DescProbeEnabled => [
                "In the login list, check host:port reachability (TCP only, no login) for profiles that connected at least once — green port open · yellow checking · blue host alive but port closed · red unreachable",
                "로그인 목록에서 한 번 이상 접속했던 프로필의 호스트:포트 도달 여부를 확인(TCP만 · 로그인 없음) — 초록 포트 열림 · 노랑 확인 중 · 파랑 IP는 응답하나 포트 닫힘 · 빨강 도달 불가",
            ],
            Msg::LblProbeMaxRetries => ["Status light backoff steps", "신호등 간격 증가 횟수"],
            Msg::DescProbeMaxRetries => [
                "Each consecutive failure doubles the wait before the next check (retry delay ×1, ×2, ×4…) up to this many times; after that the wait stays at that maximum. The failure count keeps accumulating until a check succeeds",
                "실패가 이어질 때마다 다음 확인까지의 대기를 2배로(재시도 대기 ×1, ×2, ×4…) 최대 이 횟수까지 늘리고, 그 뒤엔 그 최대 간격을 유지. 실패 횟수는 성공할 때까지 누적",
            ],
            Msg::LblProbeTimeout => ["Status light timeout (s)", "신호등 타임아웃(초)"],
            Msg::DescProbeTimeout => [
                "Per-attempt TCP connect timeout. Also the budget for the pre-run fast check when a query targets a server whose light is not green",
                "시도 1회의 TCP 연결 타임아웃. 신호등이 초록이 아닌 서버에 쿼리를 보낼 때 실행 전 빠른 판정에도 이 시간을 쓴다",
            ],
            Msg::LblProbeInterval => ["Status light refresh (s)", "신호등 갱신 주기(초)"],
            Msg::DescProbeInterval => [
                "While the login window is open, re-check every server this often",
                "접속 창이 열려 있는 동안 이 주기로 모든 서버를 다시 확인",
            ],
            Msg::LblProbeRetryDelay => ["Status light retry delay (s)", "신호등 재시도 대기(초)"],
            Msg::DescProbeRetryDelay => [
                "Wait before the next check after the first failure; doubles on each further failure (not a fast retry)",
                "첫 실패 후 다음 확인까지의 대기 시간. 실패가 이어지면 매번 2배(빠른 재시도 아님)",
            ],
            Msg::ErrServerUnreachable => [
                "Server unreachable: {0} (fast check, {1} ms) — query not sent",
                "서버 도달 불가: {0} (빠른 판정 {1} ms) — 쿼리를 보내지 않았습니다",
            ],
            Msg::WkPanic => [
                "Driver crashed: {0} — session dropped, reconnect to continue",
                "드라이버 비정상 종료: {0} — 세션을 버렸습니다. 다시 접속하세요",
            ],
            Msg::CatWindow => ["Window", "창 관리"],
            Msg::LblWindowFocus => ["Window focus", "창 포커스"],
            Msg::DescWindowFocus => [
                "group: selecting one window brings every window forward (selected on top, z-order kept) · single: only the selected window is raised",
                "group: 창 하나를 선택하면 모든 창이 함께 앞으로(선택 창이 맨 위 · 나머지 z-order 유지) · single: 선택한 창만 활성화",
            ],
            Msg::ValFocusGroup => ["group", "함께(group)"],
            Msg::ValFocusSingle => ["single", "개별(single)"],
            Msg::MnCommandPalette => ["Command Palette…", "명령 팔레트…"],
            Msg::PhPalette => ["Type a command…", "명령 입력…"],
            Msg::PalSetSyntax => ["Set Syntax", "구문 설정"],
            Msg::PalNoMatch => ["No matching command", "일치하는 명령 없음"],
            Msg::StSyntaxSet => ["Syntax: {0}", "구문: {0}"],
            Msg::StPos => ["Ln {0}, Col {1}", "{0}행 {1}열"],
            Msg::StRowsShort => ["{0} rows", "{0}행"],
            Msg::LblCopyRich => ["Copy with formatting", "서식 있는 복사"],
            Msg::DescCopyRich => [
                "Copy from the editor also puts HTML (syntax colors, bold keywords) on the clipboard — paste into PowerPoint/Word keeps the look",
                "편집기 복사 시 HTML(구문 색·키워드 굵게)도 클립보드에 — PowerPoint/Word에 붙여넣으면 같은 모양",
            ],
            Msg::LblRulers => ["Rulers", "세로 안내선"],
            Msg::DescRulers => [
                "Vertical guide lines at these columns, comma separated (e.g. 80, 120); empty = none",
                "지정한 글자 열에 세로 안내선 · 쉼표 목록(예 80, 120) · 비우면 없음",
            ],
            Msg::LblWhitespace => ["Show whitespace", "공백 표시"],
            Msg::DescWhitespace => [
                "Which whitespace to draw: none · inside the selection · all",
                "공백을 어디에 표시할지: 없음 · 선택 영역 안 · 전체",
            ],
            Msg::ValWsNone => ["none", "없음"],
            Msg::ValWsSelection => ["selection", "선택 영역"],
            Msg::ValWsAll => ["all", "전체"],
            Msg::LblWsChars => ["Whitespace glyphs", "공백 표시 글자"],
            Msg::DescWsChars => [
                "Three characters: space, tab, line end ('_' = don't draw). Default ·→_",
                "글자 3개: 공백 · 탭 · 줄끝('_' = 표시 안 함). 기본 ·→_",
            ],
            Msg::LblWsColor => ["Whitespace color", "공백 표시 색"],
            Msg::DescWsColor => [
                "Hex RRGGBB (e.g. 808080); empty = theme dim text color",
                "16진 RRGGBB(예 808080) · 비우면 테마의 흐린 글자색",
            ],
            Msg::LblWsAlpha => ["Whitespace opacity (%)", "공백 표시 불투명도(%)"],
            Msg::DescWsAlpha => ["0 = invisible … 100 = solid", "0 = 안 보임 … 100 = 불투명"],
            Msg::WinLogin => ["Database Login", "데이터베이스 로그인"],
            Msg::LblLoginList => ["Login List", "로그인 목록"],
            Msg::BtnNew => ["New", "새로 만들기"],
            Msg::BtnEdit => ["Details", "상세 보기"],
            Msg::BtnDelete => ["Delete", "삭제"],
            Msg::BtnClose => ["Close", "닫기"],
            Msg::PhFilter => ["Filter…", "필터…"],
            Msg::ColName => ["Name", "이름"],
            Msg::ColType => ["Type", "종류"],
            Msg::ColUser => ["User", "사용자"],
            Msg::ColTarget => ["Target", "대상"],
            Msg::StNoProfiles => ["No saved profiles — fill the form and Save", "저장된 프로필 없음 — 폼을 채우고 Save"],
            Msg::StProfileDeleted => ["Profile '{0}' deleted", "프로필 '{0}' 삭제됨"],
            Msg::CfgUsage => [
                "nsql config — app settings (shared with the GUI)\n\n  nsql config list                # all keys · current value · (default)\n  nsql config get <key>\n  nsql config set <key> <value>   # e.g. ui.lang ko · ui.theme dark\n  nsql config reset <key>\n  nsql config path",
                "nsql config — 앱 설정(GUI와 공유)\n\n  nsql config list                # 전체 키 · 현재 값 · (기본값)\n  nsql config get <key>\n  nsql config set <key> <value>   # 예: ui.lang ko · ui.theme dark\n  nsql config reset <key>\n  nsql config path",
            ],
            Msg::CfgUnknownKey => ["Unknown setting: {0}", "알 수 없는 설정: {0}"],
            Msg::CfgInvalidValue => [
                "Invalid value for {0}: '{1}' (allowed: {2})",
                "{0}의 값이 올바르지 않습니다: '{1}' (허용: {2})",
            ],
            Msg::CfgSet => ["{0} = {1}", "{0} = {1}"],
            Msg::CfgReset => ["{0} reset to default ({1})", "{0} 기본값으로 복원 ({1})"],
            Msg::CfgOpenFailed => ["Cannot open settings: {0}", "설정을 열 수 없습니다: {0}"],
            Msg::CfgSaveFailed => ["Cannot save settings: {0}", "설정을 저장할 수 없습니다: {0}"],
            Msg::CfgNoConfigDir => [
                "User config folder unknown (no APPDATA/HOME) — set NSQL_HOME",
                "사용자 설정 폴더를 알 수 없습니다(APPDATA/HOME 없음) — NSQL_HOME을 지정하세요",
            ],
            Msg::CfgDefaultMark => ["(default)", "(기본값)"],
        }
    }

    /// 전수(테스트·검색용).
    pub const ALL: &'static [Msg] = &[
        Msg::PhProfileName,
        Msg::PhConnect,
        Msg::PhEditor,
        Msg::BtnSave,
        Msg::BtnConnect,
        Msg::BtnRun,
        Msg::StInitial,
        Msg::StProfiles,
        Msg::StSaving,
        Msg::StRunning,
        Msg::StConnecting,
        Msg::StRows,
        Msg::StRowsAffected,
        Msg::StOk,
        Msg::StConnected,
        Msg::StDisconnected,
        Msg::StErrorLine,
        Msg::ErrClipboard,
        Msg::StHint,
        Msg::StThemeChanged,
        Msg::StLangChanged,
        Msg::ErrProfileName,
        Msg::ErrNeedTarget,
        Msg::ErrNoSql,
        Msg::ErrEnterTarget,
        Msg::ErrNoFont,
        Msg::ErrNoWindow,
        Msg::ErrEventLoop,
        Msg::LblProfile,
        Msg::ValNewProfile,
        Msg::LblDbType,
        Msg::LblHost,
        Msg::LblPort,
        Msg::LblDatabase,
        Msg::LblService,
        Msg::LblFile,
        Msg::LblDsn,
        Msg::LblUser,
        Msg::LblPassword,
        Msg::LblSavePassword,
        Msg::LblProfileName,
        Msg::PhHost,
        Msg::PhDatabase,
        Msg::PhService,
        Msg::PhSqliteFile,
        Msg::PhDsn,
        Msg::PhUser,
        Msg::PhPassword,
        Msg::BtnTest,
        Msg::BtnDisconnect,
        Msg::StIdle,
        Msg::StConnectingShort,
        Msg::StTesting,
        Msg::StConnectedShort,
        Msg::StTestOk,
        Msg::StTestFailed,
        Msg::StConnectFailed,
        Msg::WkProfileSaved,
        Msg::WkProfileSaveFailed,
        Msg::WkErrors,
        Msg::CtxSelectAll,
        Msg::CtxCopy,
        Msg::CtxCut,
        Msg::CtxPaste,
        Msg::CatAppearance,
        Msg::CatEditor,
        Msg::LblLang,
        Msg::DescLang,
        Msg::LblTheme,
        Msg::DescTheme,
        Msg::LblUiFontSize,
        Msg::DescUiFontSize,
        Msg::LblEditorFontSize,
        Msg::DescEditorFontSize,
        Msg::ValSystem,
        Msg::ValLight,
        Msg::ValDark,
        Msg::ValNoDriver,
        Msg::LblEditorLineNumbers,
        Msg::DescEditorLineNumbers,
        Msg::LblGridRowNumbers,
        Msg::DescGridRowNumbers,
        Msg::MnFile,
        Msg::MnEdit,
        Msg::MnView,
        Msg::MnRun,
        Msg::MnHelp,
        Msg::MnNew,
        Msg::MnExit,
        Msg::MnUndoNa,
        Msg::MnCut,
        Msg::MnCopy,
        Msg::MnPaste,
        Msg::MnSelectAll,
        Msg::MnLogWindow,
        Msg::MnTheme,
        Msg::MnLanguage,
        Msg::MnRunStatement,
        Msg::MnRunAll,
        Msg::MnConnect,
        Msg::MnDisconnect,
        Msg::MnAbout,
        Msg::TipStatements,
        Msg::TipLines,
        Msg::TipChars,
        Msg::TipConnection,
        Msg::TipNew,
        Msg::TipRunStatement,
        Msg::TipRunAll,
        Msg::TipConnect,
        Msg::TipLog,
        Msg::LblTabsRows,
        Msg::DescTabsRows,
        Msg::ValTabsSingle,
        Msg::ValTabsMulti,
        Msg::LblTabsTooltip,
        Msg::DescTabsTooltip,
        Msg::CatExplorer,
        Msg::LblExplorerAutoRefresh,
        Msg::DescExplorerAutoRefresh,
        Msg::LblExplorerRefreshSecs,
        Msg::DescExplorerRefreshSecs,
        Msg::LblExplorerTimeout,
        Msg::DescExplorerTimeout,
        Msg::LblExplorerTooltip,
        Msg::DescExplorerTooltip,
        Msg::LblGridScroll,
        Msg::DescGridScroll,
        Msg::ValScrollPixel,
        Msg::ValScrollRow,
        Msg::CatSession,
        Msg::LblSessionMode,
        Msg::DescSessionMode,
        Msg::ValSessionShared,
        Msg::ValSessionPerEditor,
        Msg::CatLog,
        Msg::LblLogFormat,
        Msg::DescLogFormat,
        Msg::ValRaw,
        Msg::ValMarkdown,
        Msg::ValGrid,
        Msg::CatConnection,
        Msg::LblProbeEnabled,
        Msg::DescProbeEnabled,
        Msg::LblProbeMaxRetries,
        Msg::DescProbeMaxRetries,
        Msg::LblProbeTimeout,
        Msg::DescProbeTimeout,
        Msg::LblProbeInterval,
        Msg::DescProbeInterval,
        Msg::LblProbeRetryDelay,
        Msg::DescProbeRetryDelay,
        Msg::ErrServerUnreachable,
        Msg::WkPanic,
        Msg::CatWindow,
        Msg::LblWindowFocus,
        Msg::DescWindowFocus,
        Msg::ValFocusGroup,
        Msg::ValFocusSingle,
        Msg::MnCommandPalette,
        Msg::PhPalette,
        Msg::PalSetSyntax,
        Msg::PalNoMatch,
        Msg::StSyntaxSet,
        Msg::StPos,
        Msg::StRowsShort,
        Msg::LblCopyRich,
        Msg::DescCopyRich,
        Msg::LblRulers,
        Msg::DescRulers,
        Msg::LblWhitespace,
        Msg::DescWhitespace,
        Msg::ValWsNone,
        Msg::ValWsSelection,
        Msg::ValWsAll,
        Msg::LblWsChars,
        Msg::DescWsChars,
        Msg::LblWsColor,
        Msg::DescWsColor,
        Msg::LblWsAlpha,
        Msg::DescWsAlpha,
        Msg::WinLogin,
        Msg::LblLoginList,
        Msg::BtnNew,
        Msg::BtnEdit,
        Msg::BtnDelete,
        Msg::BtnClose,
        Msg::PhFilter,
        Msg::ColName,
        Msg::ColType,
        Msg::ColUser,
        Msg::ColTarget,
        Msg::StNoProfiles,
        Msg::StProfileDeleted,
        Msg::CfgUsage,
        Msg::CfgUnknownKey,
        Msg::CfgInvalidValue,
        Msg::CfgSet,
        Msg::CfgReset,
        Msg::CfgOpenFailed,
        Msg::CfgSaveFailed,
        Msg::CfgNoConfigDir,
        Msg::CfgDefaultMark,
    ];
}

/// 지정 언어의 문자열(빈 칸 = 영어 폴백).
#[must_use]
pub fn tr(lang: Lang, msg: Msg) -> &'static str {
    let row = msg.row();
    let s = row[lang.column()];
    if s.is_empty() {
        row[0]
    } else {
        s
    }
}

/// 현재 언어의 문자열.
#[must_use]
pub fn t(msg: Msg) -> &'static str {
    tr(current_lang(), msg)
}

/// 현재 언어 + `{0}` `{1}` … 치환. 없는 자리는 그대로 둔다.
#[must_use]
pub fn tf(msg: Msg, args: &[&str]) -> String {
    let mut s = t(msg).to_string();
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), a);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_english() {
        assert_eq!(Lang::default(), Lang::En);
        assert_eq!(current_lang(), Lang::En);
        assert_eq!(t(Msg::BtnSave), "Save");
    }

    #[test]
    fn all_rows_have_english_and_no_stray_placeholders() {
        for m in Msg::ALL {
            let row = m.row();
            assert!(!row[0].is_empty(), "{m:?}: 영어 칸이 비었다");
            // 한국어 칸이 있으면 자리표시자 집합이 영어와 같아야 한다.
            if !row[1].is_empty() {
                for i in 0..4 {
                    let p = format!("{{{i}}}");
                    assert_eq!(
                        row[0].contains(&p),
                        row[1].contains(&p),
                        "{m:?}: {p} 자리표시자 불일치"
                    );
                }
            }
        }
    }

    #[test]
    fn korean_falls_back_to_english_when_empty() {
        assert_eq!(tr(Lang::Ko, Msg::PhEditor), tr(Lang::En, Msg::PhEditor));
        assert_eq!(tr(Lang::Ko, Msg::BtnSave), "저장");
    }

    #[test]
    fn codes_roundtrip_and_regions() {
        for l in Lang::ALL {
            assert_eq!(Lang::from_code(l.code()), Some(l));
        }
        assert_eq!(Lang::from_code("ko-KR"), Some(Lang::Ko));
        assert_eq!(Lang::from_code("EN_US"), Some(Lang::En));
        assert_eq!(Lang::from_code("fr"), None);
    }

    #[test]
    fn tf_substitutes_in_order() {
        set_lang(Lang::En);
        assert_eq!(tf(Msg::StRows, &["12", "0.003"]), "12 rows · 0.003s");
        assert_eq!(tf(Msg::StSaving, &[]), "Saving profile… {0}");
    }
}
