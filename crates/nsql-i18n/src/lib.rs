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
