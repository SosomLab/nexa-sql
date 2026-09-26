//! `nsql-settings` — 앱 설정(T-37·T-38 · 사용자 09-14 *"i18n 기본 영어 · 테마 System/Light/Dark 기본 System"*).
//!
//! ```text
//! <사용자 설정 폴더>/nexa-sql/settings.conf     config_dir() = NSQL_HOME | user_config_dir("nexa-sql")
//!   _schema=1
//!   ui.lang=en                                    ← 레지스트리 기본값과 같으면 줄을 쓰지 않는다(파일 = 사용자 변경분만)
//!   ui.theme=system
//! ```
//!
//! 설계(VS Code 설정 방식 차용 · [docs/24](../../../docs/24-settings-and-vscode-analysis.md)):
//! - **레지스트리가 단일 원천** — 키·종류·기본값·라벨/설명(i18n 키)·카테고리를 [`REGISTRY`] 한 곳에 적는다.
//!   CLI `nsql config`·GUI 설정 화면·검색이 전부 이 표를 읽으므로 "화면엔 있는데 검색 안 되는 설정"이 구조적으로 없다
//!   (nexa-clip `settings_registry.rs` 선례).
//! - **파일에는 기본값과 다른 값만** 쓴다(VS Code `settings.json`과 같은 "변경분" 모델) — 기본값을 바꿔도 사용자가 손대지 않은 항목은 따라온다.
//! - **모르는 키는 보존**(nexa-conf F-1 계약) — 구판이 저장해도 신판 키가 살아남는다.
//! - 값 검증은 저장 전에(`set`) — 손상된 파일 값은 읽을 때 기본값으로 대체하되 파일은 건드리지 않는다(fail-soft).
//! - 이 크레이트는 UI를 모른다 — 테마는 `ThemeMode`(모드)만 다루고 팔레트(`nexa_ctl::Theme`)는 GUI가 고른다.

use nsql_i18n::{Lang, Msg};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// 앱 폴더 이름(`%APPDATA%\nexa-sql` · `~/.config/nexa-sql` · `~/Library/Application Support/nexa-sql`).
pub const APP_DIR: &str = "nexa-sql";
/// 설정 파일 이름.
pub const FILE_NAME: &str = "settings.conf";

/// 기본값이 바뀐 키의 **옛 기본값** — 설정 창이 저장해 둔 옛 기본값은 사용자가 고른 값이 아니므로 새 기본값을 따른다.
/// (미니맵 폭 80 → 160 · 사용자 09-17 "지금의 2배")
const OLD_DEFAULTS: &[(&str, &str)] = &[("editor.minimap_width", "80")];

pub mod json;
pub mod projfile;
pub use json::{to_json, Import as JsonImport, Json};
pub mod perf;
pub use perf::{binding as perf_binding, Domain, PerfBinding, PerfMode, PerfRow, PerfSource, PERF};

/// 설정 폴더 — `NSQL_HOME`이 있으면 그것(테스트·개발용 재지정), 아니면 OS 사용자 설정 폴더. `nsql-vault`도 같은 규칙을 쓴다.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("NSQL_HOME") {
        return Some(PathBuf::from(h));
    }
    nexa_conf::user_config_dir(APP_DIR)
}

// ────────────────────────────────────────────────────────────── 테마 모드

/// 테마 모드 — 기본 **System**(사용자 09-14).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    /// 설정 값 순서 = 순환 순서.
    pub const ALL: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<ThemeMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "system" | "auto" => Some(ThemeMode::System),
            "light" => Some(ThemeMode::Light),
            "dark" => Some(ThemeMode::Dark),
            _ => None,
        }
    }

    /// 표시 라벨 키.
    #[must_use]
    pub const fn label(self) -> Msg {
        match self {
            ThemeMode::System => Msg::ValSystem,
            ThemeMode::Light => Msg::ValLight,
            ThemeMode::Dark => Msg::ValDark,
        }
    }

    /// 다음 모드(단축키 순환 System → Light → Dark → System).
    #[must_use]
    pub const fn next(self) -> ThemeMode {
        match self {
            ThemeMode::System => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::System,
        }
    }

    /// 모드 + OS 판정 → 다크 여부. `System`에서 OS가 무선호/판정 불가면 **다크**(제품 기본 룩 · nexa-clip 선례).
    #[must_use]
    pub const fn is_dark(self, system_dark: Option<bool>) -> bool {
        match self {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::System => match system_dark {
                Some(d) => d,
                None => true,
            },
        }
    }
}

// ────────────────────────────────────────────────────────────── 레지스트리

/// 설정 항목 종류(컨트롤 형태 + 검증 규칙). VS Code `IConfigurationPropertySchema.type`에 해당.
#[derive(Clone, Copy, Debug)]
pub enum SettingKind {
    /// 후보 중 택일(값, 라벨 키) — 드롭다운.
    Choice(&'static [(&'static str, Msg)]),
    /// 언어 — 후보는 [`Lang::ALL`], 라벨은 endonym(번역하지 않음).
    Lang,
    /// 정수 범위.
    Int { min: i64, max: i64 },
    /// 글꼴 크기 — `13` · `13px` · `10pt`(1pt = 96/72 px · 사용자 09-16). 범위는 px 기준. 저장은 입력한 단위 그대로.
    Size { min: i64, max: i64 },
    /// on/off.
    Bool,
    /// 자유 텍스트(목록·색 등 — 앱이 해석).
    Text,
    /// 화면 안 위치(3×3) — 설정 창은 이미지 드롭다운(미니 화면 타일)으로 고른다(사용자 09-19). 값 = [`POSITIONS`].
    Position,
}

/// [`SettingKind::Position`] 값(행우선 · 왼쪽 위 → 오른쪽 아래).
pub const POSITIONS: [&str; 9] = [
    "top_left",
    "top_center",
    "top_right",
    "mid_left",
    "center",
    "mid_right",
    "bottom_left",
    "bottom_center",
    "bottom_right",
];

/// 설정 항목 — 레지스트리 한 줄. VS Code `IConfigurationNode.properties[key]` + TOC 카테고리에 해당.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    /// 값 키(안정 계약 — rename 시 마이그레이션 표). `<category>.<name>` 소문자·`_`.
    pub key: &'static str,
    /// 카테고리(사이드바·검색 대상).
    pub cat: Msg,
    /// 제목(검색 대상).
    pub label: Msg,
    /// 설명 한 줄(검색 대상).
    pub desc: Msg,
    pub kind: SettingKind,
    /// 기본값(문자열 표현 — 파일 표현과 동일).
    pub default: &'static str,
}

/// 줄끝(docs/38 · DBeaver/Eclipse "New text file line delimiter" 대응 · 09-16).
const TAB_STOPS_OPTS: &[(&str, Msg)] = &[
    ("stop", Msg::ValTabStopsStop),
    ("fixed", Msg::ValTabStopsFixed),
];
const CLI_FORMAT_OPTS: &[(&str, Msg)] = &[
    ("grid", Msg::ValFmtGrid),
    ("markdown", Msg::ValFmtMarkdown),
    ("csv", Msg::ValFmtCsv),
    ("tsv", Msg::ValFmtTsv),
    ("json", Msg::ValFmtJson),
    ("jsonl", Msg::ValFmtJsonl),
];
const KEY_MODE_OPTS: &[(&str, Msg)] = &[("pk", Msg::ValKeyModePk), ("all", Msg::ValKeyModeAll)];
/// 그리드 컬럼 최대 폭 — 자동(= [`COL_MAX_AUTO_CHARS`]자) · 직접(`grid.col_max_chars`) (사용자 09-17 "폰트 기준 24자 수준").
const COL_MAX_MODE_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValColMaxAuto),
    ("manual", Msg::ValColMaxManual),
];
/// 자동 모드의 컬럼 최대 글자 수(영문 숫자 폭 기준 · 한글 등 전각은 2자로 셈). `grid.col_max_chars`의 기본값과 같다.
pub const COL_MAX_AUTO_CHARS: i64 = 24;
const OVERFLOW_OPTS: &[(&str, Msg)] = &[
    ("wrap", Msg::ValOverflowWrap),
    ("truncate", Msg::ValOverflowTruncate),
    ("expanded", Msg::ValOverflowExpanded),
    ("none", Msg::ValOverflowNone),
];
const EOL_NEW_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValEolAuto),
    ("lf", Msg::ValEolLf),
    ("crlf", Msg::ValEolCrlf),
];
const EOL_SAVE_OPTS: &[(&str, Msg)] = &[
    ("keep", Msg::ValEolKeep),
    ("lf", Msg::ValEolLf),
    ("crlf", Msg::ValEolCrlf),
    ("os", Msg::ValEolAuto),
];

const SESSION_MODE_OPTS: &[(&str, Msg)] = &[
    ("shared", Msg::ValSessionShared),
    ("per-editor", Msg::ValSessionPerEditor),
];

const TAB_ROWS_OPTS: &[(&str, Msg)] =
    &[("single", Msg::ValTabsSingle), ("multi", Msg::ValTabsMulti)];

const SCROLL_OPTS: &[(&str, Msg)] = &[("pixel", Msg::ValScrollPixel), ("row", Msg::ValScrollRow)];
/// 미커밋 탭 닫기(DR-30).
const TX_READ_END_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValTxReadEndAuto),
    ("strict", Msg::ValTxReadEndStrict),
    ("off", Msg::ValTxReadEndOff),
];
const TX_IDLE_ACTION_OPTS: &[(&str, Msg)] = &[
    ("warn", Msg::ValTxIdleWarn),
    ("rollback", Msg::ValTxIdleRollback),
    ("commit", Msg::ValTxIdleCommit),
];
/// 저장하지 않은 탭을 닫을 때.
const CLOSE_UNSAVED_OPTS: &[(&str, Msg)] =
    &[("ask", Msg::ValCloseAsk), ("twice", Msg::ValCloseTwice)];
const TX_CLOSE_OPTS: &[(&str, Msg)] = &[
    ("ask", Msg::ValTxAsk),
    ("commit", Msg::ValTxCommit),
    ("rollback", Msg::ValTxRollback),
];
/// 탭 배지 모양(DR-30).
const TX_BADGE_OPTS: &[(&str, Msg)] = &[
    ("count", Msg::ValBadgeCount),
    ("dot", Msg::ValBadgeDot),
    ("off", Msg::ValBadgeOff),
];

const MSSQL_ENCRYPT_OPTS: &[(&str, Msg)] = &[
    ("required", Msg::ValMssqlEncryptRequired),
    ("login", Msg::ValMssqlEncryptLogin),
];

const MINIMAP_VIEWPORT_OPTS: &[(&str, Msg)] = &[
    ("always", Msg::ValMinimapVpAlways),
    ("hover", Msg::ValMinimapVpHover),
];

const MINIMAP_CLICK_OPTS: &[(&str, Msg)] = &[
    ("center", Msg::ValMinimapClickCenter),
    ("text", Msg::ValMinimapClickText),
];

const CLICK_DBL_OPTS: &[(&str, Msg)] = &[
    ("word", Msg::OptClickWord),
    ("line", Msg::OptClickLine),
    ("none", Msg::OptClickNone),
];
const CLICK_TRIPLE_OPTS: &[(&str, Msg)] = &[
    ("line", Msg::OptClickLine),
    ("all", Msg::OptClickAll),
    ("none", Msg::OptClickNone),
];
const GE_EMPTY_OPTS: &[(&str, Msg)] = &[("null", Msg::OptGeNull), ("empty", Msg::OptGeEmpty)];
const GE_REFRESH_OPTS: &[(&str, Msg)] =
    &[("requery", Msg::OptGeRequery), ("local", Msg::OptGeLocal)];
const REFETCH_OPTS: &[(&str, Msg)] = &[
    ("strict", Msg::ValRefetchStrict),
    ("strict_all", Msg::ValRefetchStrictAll),
    ("offset", Msg::ValRefetchOffset),
];

const FILTER_SCOPE_OPTS: &[(&str, Msg)] =
    &[("all", Msg::ValFilterAll), ("shown", Msg::ValFilterShown)];

const DISC_PICK_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValDiscAuto),
    ("always", Msg::ValDiscAlways),
    ("all", Msg::ValDiscAll),
];

const MSSQL_CANCEL_OPTS: &[(&str, Msg)] = &[
    ("attention", Msg::ValMssqlCancelAttention),
    ("socket", Msg::ValMssqlCancelSocket),
];

const INDENT_RULES_OPTS: &[(&str, Msg)] = &[
    ("sql", Msg::ValIndentSql),
    ("brackets", Msg::ValIndentBrackets),
    ("none", Msg::ValIndentNone),
];

const OCC_SHAPES: &[(&str, Msg)] = &[("rect", Msg::ValOccRect), ("round", Msg::ValOccRound)];
/// 열(블록) 선택 마우스 조합(사용자 09-17): auto = OS 규칙(Windows Alt+Shift+좌드래그 · macOS Option+좌드래그 · Linux Sublime = Shift+우드래그).
const COLUMN_SELECT: &[(&str, Msg)] = &[
    ("auto", Msg::ValColumnAuto),
    ("alt_shift", Msg::ValColumnAltShift),
    ("alt", Msg::ValColumnAlt),
    ("shift_right", Msg::ValColumnShiftRight),
];

const INTEL_STAR_LAYOUT: &[(&str, Msg)] = &[
    ("inline", Msg::ValIntelStarInline),
    ("lines", Msg::ValIntelStarLines),
];
const INTEL_STAR_SPACE: &[(&str, Msg)] = &[
    ("space", Msg::ValIntelStarSpace),
    ("tab", Msg::ValIntelStarTab),
];
const INTEL_MATCH: &[(&str, Msg)] = &[
    ("prefix", Msg::ValIntelMatchPrefix),
    ("contains", Msg::ValIntelMatchContains),
    ("fuzzy", Msg::ValIntelMatchFuzzy),
];

const INTEL_CASE: &[(&str, Msg)] = &[
    ("default", Msg::ValIntelCaseDefault),
    ("upper", Msg::ValIntelCaseUpper),
    ("lower", Msg::ValIntelCaseLower),
    ("match", Msg::ValIntelCaseMatch),
];

/// 검색어 이력 보기 방식(사용자 09-23 "Dropdown 기본 · Flat은 상자 안 ↑/↓").
const HISTORY_VIEW: &[(&str, Msg)] = &[
    ("dropdown", Msg::ValHistoryViewDropdown),
    ("flat", Msg::ValHistoryViewFlat),
];

const RAINBOW_MATCH: &[(&str, Msg)] = &[
    ("off", Msg::ValRainbowMatchOff),
    ("near", Msg::ValRainbowMatchNear),
    ("always", Msg::ValRainbowMatchAlways),
];

const WS_OPTS: &[(&str, Msg)] = &[
    ("none", Msg::ValWsNone),
    ("selection", Msg::ValWsSelection),
    ("all", Msg::ValWsAll),
];

const WINDOW_FOCUS_OPTS: &[(&str, Msg)] = &[
    ("group", Msg::ValFocusGroup),
    ("single", Msg::ValFocusSingle),
];

const RUN_AFTER_OPTS: &[(&str, Msg)] = &[
    ("stay", Msg::ValRunStay),
    ("next_ok", Msg::ValRunNextOk),
    ("next_always", Msg::ValRunNextAlways),
];
const KEY_PRESET_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValKeyAuto),
    ("windows", Msg::ValKeyWindows),
    ("macos", Msg::ValKeyMacos),
    ("linux", Msg::ValKeyLinux),
];
const LOG_FORMAT_OPTS: &[(&str, Msg)] = &[
    ("raw", Msg::ValRaw),
    ("markdown", Msg::ValMarkdown),
    ("grid", Msg::ValGrid),
    ("compact", Msg::ValLogCompact),
    ("jsonl", Msg::ValLogJsonl),
    ("csv", Msg::ValFmtCsv),
    ("tsv", Msg::ValFmtTsv),
    ("template", Msg::ValLogTemplate),
];
const EXT_CHANGE_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValExtAuto),
    ("ask", Msg::ValExtAsk),
    ("off", Msg::ValExtOff),
];
const EXT_CHECK_OPTS: &[(&str, Msg)] = &[
    ("focus_poll", Msg::ValExtCheckPoll),
    ("focus", Msg::ValExtCheckFocus),
];
const META_SCOPE_OPTS: &[(&str, Msg)] = &[
    ("changed", Msg::ValMetaScopeChanged),
    ("all", Msg::ValMetaScopeAll),
];
const LOG_FILE_FORMAT_OPTS: &[(&str, Msg)] = &[
    ("same", Msg::ValLogSame),
    ("raw", Msg::ValRaw),
    ("markdown", Msg::ValMarkdown),
    ("grid", Msg::ValGrid),
    ("compact", Msg::ValLogCompact),
    ("jsonl", Msg::ValLogJsonl),
    ("csv", Msg::ValFmtCsv),
    ("tsv", Msg::ValFmtTsv),
    ("template", Msg::ValLogTemplate),
];

/// Oracle 실행 중 로그 소스(T-71 · docs/32 §2).
const LIVE_SOURCE_OPTS: &[(&str, Msg)] = &[
    ("off", Msg::ValLiveOff),
    ("session", Msg::ValLiveSession),
    ("table", Msg::ValLiveTable),
];

/// `settings.json` 편집기(외부 = OS의 .json 연결 프로그램 · 내장 = 편집기 탭 · T-76).
const JSON_EDITOR_OPTS: &[(&str, Msg)] = &[
    ("external", Msg::ValJsonExternal),
    ("builtin", Msg::ValJsonBuiltin),
];

/// `ui.animations`(docs/39 §3-4) — auto = OS "동작 줄이기"를 따른다.
const ANIM_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValAnimAuto),
    ("on", Msg::ValAnimOn),
    ("off", Msg::ValAnimOff),
];

/// 한글 조합 방식(T-139 · 09-20): auto = macOS + 한글 입력 소스면 앱 조합 · 그 밖은 시스템 IME.
const HANGUL_COMPOSE_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::OptHangulAuto),
    ("system", Msg::OptHangulSystem),
    ("app", Msg::OptHangulApp),
];

/// `SELECT … INTO` 행 수 정책(D-139).
/// 변수 안의 변수 확장 시점(docs/63 §9 · 09-22): 대입 시(기본 · SQL*Plus) / 사용 시(재귀 · SQL Workbench/J).
const VARS_EXPAND_OPTS: &[(&str, Msg)] =
    &[("assign", Msg::OptExpandAssign), ("use", Msg::OptExpandUse)];

const VARS_INTO_OPTS: &[(&str, Msg)] =
    &[("oracle", Msg::OptIntoOracle), ("first", Msg::OptIntoFirst)];

/// 값 없는 바인드·미정의 치환 변수(D-137 · docs/63 V3).
const VARS_UNDECLARED_OPTS: &[(&str, Msg)] = &[
    ("prompt", Msg::OptVarsPrompt),
    ("auto", Msg::OptVarsAuto),
    ("error", Msg::OptVarsError),
];

/// macOS 화면 내보내기(T-147 · docs/62 §2).
const LINUX_BACKEND_OPTS: &[(&str, Msg)] = &[
    ("x11", Msg::OptLinuxBackendX11),
    ("wayland", Msg::OptLinuxBackendWayland),
    ("auto", Msg::OptLinuxBackendAuto),
];

const MAC_PRESENT_OPTS: &[(&str, Msg)] = &[
    ("iosurface", Msg::OptMacPresentLayer),
    ("softbuffer", Msg::OptMacPresentSoft),
];

const THEME_OPTS: &[(&str, Msg)] = &[
    ("system", Msg::ValSystem),
    ("light", Msg::ValLight),
    ("dark", Msg::ValDark),
];

/// 추가 페치 방식(docs/43 D-70 · T-48a).
const FETCH_MODE_OPTS: &[(&str, Msg)] = &[
    ("cursor", Msg::ValFetchCursor),
    ("offset", Msg::ValFetchOffset),
    ("off", Msg::ValFetchOff),
];

/// ★ 설정 레지스트리 — 단일 원천. 새 설정 = 여기 한 줄 + `Msg` 라벨/설명 2줄.
pub const REGISTRY: &[Entry] = &[
    Entry {
        key: "ui.lang",
        cat: Msg::CatAppearance,
        label: Msg::LblLang,
        desc: Msg::DescLang,
        kind: SettingKind::Lang,
        default: "en",
    },
    Entry {
        key: "ui.theme",
        cat: Msg::CatAppearance,
        label: Msg::LblTheme,
        desc: Msg::DescTheme,
        kind: SettingKind::Choice(THEME_OPTS),
        default: "system",
    },
    Entry {
        key: "ui.menu_font_size",
        cat: Msg::CatAppearance,
        label: Msg::LblMenuFontSize,
        desc: Msg::DescMenuFontSize,
        kind: SettingKind::Size { min: 10, max: 32 },
        default: "17",
    },
    Entry {
        key: "ui.font_face",
        cat: Msg::CatAppearance,
        label: Msg::LblUiFontFace,
        desc: Msg::DescUiFontFace,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "ui.text_contrast",
        cat: Msg::CatAppearance,
        label: Msg::LblTextContrast,
        desc: Msg::DescTextContrast,
        kind: SettingKind::Int { min: 100, max: 250 },
        default: "140",
    },
    Entry {
        key: "ui.text_weight",
        cat: Msg::CatAppearance,
        label: Msg::LblTextWeight,
        desc: Msg::DescTextWeight,
        kind: SettingKind::Int { min: 0, max: 60 },
        default: "25",
    },
    Entry {
        key: "ui.text_gdi",
        cat: Msg::CatAppearance,
        label: Msg::LblTextGdi,
        desc: Msg::DescTextGdi,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ui.text_hint",
        cat: Msg::CatAppearance,
        label: Msg::LblTextHint,
        desc: Msg::DescTextHint,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ui.text_snap",
        cat: Msg::CatAppearance,
        label: Msg::LblTextSnap,
        desc: Msg::DescTextSnap,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ui.font_size",
        // 기본 14 → 15 · 편집기 14 → 16 · 그리드 15 신설 — Golden 기준 가독성(사용자 09-15 "폰트가 너무 작다").
        cat: Msg::CatAppearance,
        label: Msg::LblUiFontSize,
        desc: Msg::DescUiFontSize,
        kind: SettingKind::Size { min: 8, max: 40 },
        default: "15",
    },
    Entry {
        key: "editor.line_numbers",
        cat: Msg::CatEditor,
        label: Msg::LblEditorLineNumbers,
        desc: Msg::DescEditorLineNumbers,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "editor.copy_rich",
        cat: Msg::CatEditor,
        label: Msg::LblCopyRich,
        desc: Msg::DescCopyRich,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "editor.tab_size",
        cat: Msg::CatEditor,
        label: Msg::LblTabSize,
        desc: Msg::DescTabSize,
        kind: SettingKind::Int { min: 1, max: 8 },
        default: "4",
    },
    Entry {
        key: "editor.indent_spaces",
        cat: Msg::CatEditor,
        label: Msg::LblIndentSpaces,
        desc: Msg::DescIndentSpaces,
        kind: SettingKind::Bool,
        // 기본 off = Tab 키가 **탭 문자**(폭 4 = `editor.tab_size`) — 사용자 확정 09-16(이전 on = 공백).
        default: "off",
    },
    Entry {
        key: "editor.tab_stops",
        cat: Msg::CatEditor,
        label: Msg::LblTabStops,
        desc: Msg::DescTabStops,
        kind: SettingKind::Choice(TAB_STOPS_OPTS),
        // 기본 = 정지점(Golden/Sublime/VS Code 관례 · 사용자 09-16) · fixed = 종전 절대 4칸.
        default: "stop",
    },
    // ── Auto indent(docs/49 · 사용자 09-17): Sublime 변수 4 + 규칙 세트.
    Entry {
        key: "editor.auto_indent",
        cat: Msg::CatEditor,
        label: Msg::LblAutoIndent,
        desc: Msg::DescAutoIndent,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "editor.smart_indent",
        cat: Msg::CatEditor,
        label: Msg::LblSmartIndent,
        desc: Msg::DescSmartIndent,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "editor.indent_rules",
        cat: Msg::CatEditor,
        label: Msg::LblIndentRules,
        desc: Msg::DescIndentRules,
        kind: SettingKind::Choice(INDENT_RULES_OPTS),
        default: "sql",
    },
    Entry {
        key: "editor.indent_to_bracket",
        cat: Msg::CatEditor,
        label: Msg::LblIndentToBracket,
        desc: Msg::DescIndentToBracket,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "editor.trim_auto_whitespace",
        cat: Msg::CatEditor,
        label: Msg::LblTrimAutoWs,
        desc: Msg::DescTrimAutoWs,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 괄호·인용부호 자동 닫기(사용자 09-19): **편집 코어 기능**이지 Rainbow Pairs 확장 기능이 아니다 — 확장 분류의 옛 키
    //   `rainbowpair.auto_close`는 없앴다(그 분류는 확장이 꺼지면 숨고, 꺼진 상태 기본값이 켜짐이라 끌 방법이 없었다).
    Entry {
        key: "editor.auto_close_pairs",
        cat: Msg::CatEditor,
        label: Msg::LblAutoClosePairs,
        desc: Msg::DescAutoClosePairs,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ── 쌍 강조(편집 코어 · 사용자 09-23 "Rainbow 확장이 아니라 기본 기능 설정으로 · 강조 대상을 지정해서"): 종류 목록 ·
    //    문자열 안 · 현재 쌍 강조. Rainbow Pairs 확장은 색만 든다(`rainbowpair.*`). 자동 닫기도 같은 종류 목록을 따른다.
    Entry {
        key: "editor.pair_kinds",
        cat: Msg::CatEditor,
        label: Msg::LblPairKinds,
        desc: Msg::DescPairKinds,
        kind: SettingKind::Text,
        default: "() [] {} \"\" '' ``",
    },
    Entry {
        key: "editor.pair_in_strings",
        cat: Msg::CatEditor,
        label: Msg::LblPairInStrings,
        desc: Msg::DescPairInStrings,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "editor.pair_match",
        cat: Msg::CatEditor,
        label: Msg::LblRainbowMatch,
        desc: Msg::DescRainbowMatch,
        kind: SettingKind::Choice(RAINBOW_MATCH),
        default: "near",
    },
    // ── CLI 표 출력(사용자 09-16 · 터미널 폭에서 표가 접혀 깨짐) — `nsql config set cli.width 160` · 1회성은 `--width`.
    Entry {
        key: "cli.width",
        cat: Msg::CatCli,
        label: Msg::LblCliWidth,
        desc: Msg::DescCliWidth,
        kind: SettingKind::Int { min: 0, max: 10000 },
        default: "0",
    },
    Entry {
        key: "cli.max_col_width",
        cat: Msg::CatCli,
        label: Msg::LblCliMaxColWidth,
        desc: Msg::DescCliMaxColWidth,
        kind: SettingKind::Int { min: 0, max: 10000 },
        default: "60",
    },
    Entry {
        key: "cli.overflow",
        cat: Msg::CatCli,
        label: Msg::LblCliOverflow,
        desc: Msg::DescCliOverflow,
        kind: SettingKind::Choice(OVERFLOW_OPTS),
        // 기본 none(사용자 09-16 확정: 종전처럼 한 줄 · 편집기에 붙여 넣으면 정렬) · wrap은 선택.
        default: "none",
    },
    // ── 생성 SQL(UPDATE/DELETE/MERGE 등)의 유일성 기준(docs/41 · 사용자 09-16) — GUI Copy SQL · CLI -f sql:* 공통.
    Entry {
        key: "sql.key_mode",
        cat: Msg::CatGrid,
        label: Msg::LblSqlKeyMode,
        desc: Msg::DescSqlKeyMode,
        kind: SettingKind::Choice(KEY_MODE_OPTS),
        default: "pk",
    },
    Entry {
        key: "cli.format",
        cat: Msg::CatCli,
        label: Msg::LblCliFormat,
        desc: Msg::DescCliFormat,
        kind: SettingKind::Choice(CLI_FORMAT_OPTS),
        default: "grid",
    },
    Entry {
        key: "cli.null_text",
        cat: Msg::CatCli,
        label: Msg::LblCliNullText,
        desc: Msg::DescCliNullText,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.rulers",
        cat: Msg::CatEditor,
        label: Msg::LblRulers,
        desc: Msg::DescRulers,
        kind: SettingKind::Text,
        default: "80",
    },
    Entry {
        key: "editor.rulers_show",
        cat: Msg::CatEditor,
        label: Msg::LblRulersShow,
        desc: Msg::DescRulersShow,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "editor.ruler_color",
        cat: Msg::CatEditor,
        label: Msg::LblRulerColor,
        desc: Msg::DescRulerColor,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.tab_accent",
        cat: Msg::CatEditor,
        label: Msg::LblEditorTabAccent,
        desc: Msg::DescEditorTabAccent,
        kind: SettingKind::Text,
        default: "#E8962E",
    },
    Entry {
        key: "editor.ruler_alpha",
        cat: Msg::CatEditor,
        label: Msg::LblRulerAlpha,
        desc: Msg::DescRulerAlpha,
        kind: SettingKind::Int { min: 5, max: 100 },
        default: "45",
    },
    Entry {
        key: "editor.text_pad_left",
        cat: Msg::CatEditor,
        label: Msg::LblTextPadLeft,
        desc: Msg::DescTextPadLeft,
        kind: SettingKind::Int { min: 0, max: 32 },
        default: "3",
    },
    Entry {
        key: "editor.column_select",
        cat: Msg::CatEditor,
        label: Msg::LblColumnSelect,
        desc: Msg::DescColumnSelect,
        kind: SettingKind::Choice(COLUMN_SELECT),
        default: "auto",
    },
    Entry {
        key: "editor.highlight_selection",
        cat: Msg::CatEditor,
        label: Msg::LblHighlightSel,
        desc: Msg::DescHighlightSel,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 줄 변경 표시(사용자 09-17): 저장 기준선 대비 수정(warn)·추가(ok)·삭제 쐐기(danger)를 줄번호 오른쪽 띠에. 향상 모드는 끈다.
    Entry {
        key: "editor.diff_marks",
        cat: Msg::CatEditor,
        label: Msg::LblDiffMarks,
        desc: Msg::DescDiffMarks,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 확장 매니저(사용자 09-17 · docs/50 §10 · Sublime Package Control 방식): 저장소 목록 · 끈 확장 목록.
    Entry {
        key: "extensions.enabled",
        cat: Msg::CatExtManager,
        label: Msg::LblExtEnabled,
        desc: Msg::DescExtEnabled,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "extensions.default_repository",
        cat: Msg::CatExtManager,
        label: Msg::LblExtDefaultRepo,
        desc: Msg::DescExtDefaultRepo,
        kind: SettingKind::Text,
        default: "https://raw.githubusercontent.com/SosomLab/nexa-sql/main/extensions",
    },
    Entry {
        key: "extensions.repositories",
        cat: Msg::CatExtManager,
        label: Msg::LblExtRepositories,
        desc: Msg::DescExtRepositories,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "extensions.disabled",
        cat: Msg::CatExtManager,
        label: Msg::LblExtDisabled,
        desc: Msg::DescExtDisabled,
        kind: SettingKind::Text,
        default: "",
    },
    // 레인보우 괄호 플러그인(사용자 09-17 · docs/51 · D-92~95): 첫 in-process 확장 `extensions/rainbow_pairs.rs`(Rainbow Pairs)가 읽는다. 향상 모드는 색을 끈다.
    Entry {
        key: "rainbowpair.enabled",
        cat: Msg::CatExtRainbowPairs,
        label: Msg::LblRainbow,
        desc: Msg::DescRainbow,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "rainbowpair.unmatched",
        cat: Msg::CatExtRainbowPairs,
        label: Msg::LblRainbowUnmatched,
        desc: Msg::DescRainbowUnmatched,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "rainbowpair.colors",
        cat: Msg::CatExtRainbowPairs,
        label: Msg::LblRainbowColors,
        desc: Msg::DescRainbowColors,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "rainbowpair.contrast_order",
        cat: Msg::CatExtRainbowPairs,
        label: Msg::LblRainbowContrast,
        desc: Msg::DescRainbowContrast,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "rainbowpair.max_kb",
        cat: Msg::CatExtRainbowPairs,
        label: Msg::LblRainbowMaxKb,
        desc: Msg::DescRainbowMaxKb,
        kind: SettingKind::Int { min: 0, max: 65536 },
        default: "0",
    },
    // 동일 출현 상자 스타일(사용자 09-17): 모양 · 선 색(+알파) · 선 두께 · 배경 색(+알파). 색은 `#RRGGBB[AA]`.
    Entry {
        key: "editor.occurrence_shape",
        cat: Msg::CatEditor,
        label: Msg::LblOccShape,
        desc: Msg::DescOccShape,
        kind: SettingKind::Choice(OCC_SHAPES),
        default: "rect",
    },
    Entry {
        key: "editor.occurrence_line_color",
        cat: Msg::CatEditor,
        label: Msg::LblOccLineColor,
        desc: Msg::DescOccLineColor,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.occurrence_line_width",
        cat: Msg::CatEditor,
        label: Msg::LblOccLineWidth,
        desc: Msg::DescOccLineWidth,
        kind: SettingKind::Int { min: 0, max: 4 },
        default: "1",
    },
    Entry {
        key: "editor.occurrence_fill_color",
        cat: Msg::CatEditor,
        label: Msg::LblOccFillColor,
        desc: Msg::DescOccFillColor,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.whitespace",
        cat: Msg::CatEditor,
        label: Msg::LblWhitespace,
        desc: Msg::DescWhitespace,
        kind: SettingKind::Choice(WS_OPTS),
        default: "selection",
    },
    Entry {
        key: "editor.whitespace_chars",
        cat: Msg::CatEditor,
        label: Msg::LblWsChars,
        desc: Msg::DescWsChars,
        kind: SettingKind::Text,
        default: "·→$",
    },
    Entry {
        key: "editor.whitespace_color",
        cat: Msg::CatEditor,
        label: Msg::LblWsColor,
        desc: Msg::DescWsColor,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.whitespace_alpha",
        cat: Msg::CatEditor,
        label: Msg::LblWsAlpha,
        desc: Msg::DescWsAlpha,
        kind: SettingKind::Int { min: 0, max: 100 },
        default: "40",
    },
    Entry {
        key: "grid.auto_fetch",
        cat: Msg::CatGrid,
        label: Msg::LblGridAutoFetch,
        desc: Msg::DescGridAutoFetch,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "grid.offset_warn",
        cat: Msg::CatGrid,
        label: Msg::LblGridOffsetWarn,
        desc: Msg::DescGridOffsetWarn,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 데이터 일관성(사용자 09-17 · docs/43 §9): 커서가 없고 ORDER BY도 없으면 이어 붙이지 않고 처음부터 다시 받아 교체.
    Entry {
        key: "grid.refetch_mode",
        cat: Msg::CatGrid,
        label: Msg::LblRefetchMode,
        desc: Msg::DescRefetchMode,
        kind: SettingKind::Choice(REFETCH_OPTS),
        default: "strict",
    },
    Entry {
        key: "grid.max_rows",
        cat: Msg::CatGrid,
        label: Msg::LblGridMaxRows,
        desc: Msg::DescGridMaxRows,
        kind: SettingKind::Int {
            min: 0,
            max: 10_000_000,
        },
        default: "200",
    },
    // ── 결과 탭(T-93 · docs/43 §4-3a · D-71~75)
    Entry {
        key: "grid.result_tabs",
        cat: Msg::CatGrid,
        label: Msg::LblGridResultTabs,
        desc: Msg::DescGridResultTabs,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 결과 탭(다중) 바로 밑: **결과가 1개일 때도 탭 영역을 보일지**(사용자 09-21) — 결과 탭을 쓸 때만 바꿀 수 있다(`DEPENDS` ·
    // 부모가 꺼지면 설정 창에서 잠긴다). 옛 키 `grid.result_tabbar`(auto|always)는 읽을 때 옮긴다(`migrate_result_tabbar`).
    Entry {
        key: "grid.result_tabbar_single",
        cat: Msg::CatGrid,
        label: Msg::LblGridResultTabbar,
        desc: Msg::DescGridResultTabbar,
        kind: SettingKind::Bool,
        default: "off",
    },
    // 결과 탭 이름 규칙(사용자 09-21): 번호(결과1·결과2… · 가장 큰 번호 + 1) | 테이블 이름(종전).
    Entry {
        key: "grid.result_tab_title",
        cat: Msg::CatGrid,
        label: Msg::LblGridResultTabTitle,
        desc: Msg::DescGridResultTabTitle,
        kind: SettingKind::Choice(RESULT_TITLE_OPTS),
        default: "number",
    },
    Entry {
        key: "grid.result_tabs_max",
        cat: Msg::CatGrid,
        label: Msg::LblGridResultTabsMax,
        desc: Msg::DescGridResultTabsMax,
        kind: SettingKind::Int { min: 1, max: 64 },
        default: "8",
    },
    // D-138(09-21): 스크립트 전체 실행의 조회가 여럿이면 문장마다 결과 탭(상한 `grid.result_tabs_max` 안 · 재실행 = 같은 자리 재사용).
    // 끄면 종전처럼 마지막 결과가 시작 탭을 덮는다(같은 문장의 커서 여러 개는 늘 탭으로 나뉜다).
    Entry {
        key: "grid.result_per_statement",
        cat: Msg::CatGrid,
        label: Msg::LblGridResultPerStatement,
        desc: Msg::DescGridResultPerStatement,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "grid.result_tab_evict",
        cat: Msg::CatGrid,
        label: Msg::LblGridResultTabEvict,
        desc: Msg::DescGridResultTabEvict,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "grid.memory_budget_mb",
        cat: Msg::CatGrid,
        label: Msg::LblGridMemoryBudget,
        desc: Msg::DescGridMemoryBudget,
        kind: SettingKind::Int {
            min: 16,
            max: 65536,
        },
        default: "1024",
    },
    Entry {
        key: "grid.row_height_pct",
        cat: Msg::CatGrid,
        label: Msg::LblGridRowHeight,
        desc: Msg::DescGridRowHeight,
        kind: SettingKind::Int { min: 110, max: 300 },
        default: "150",
    },
    Entry {
        key: "grid.null_text",
        cat: Msg::CatGrid,
        label: Msg::LblNullText,
        desc: Msg::DescNullText,
        kind: SettingKind::Text,
        default: "NULL",
    },
    // ★ 연속 클릭 정책(사용자 09-26 · nexa-ctl `set_click_policy` · 편집기·셀 편집기·패널 공통 · CatEditor).
    Entry {
        key: "editor.dblclick",
        cat: Msg::CatEditor,
        label: Msg::LblEditorDblClick,
        desc: Msg::DescEditorDblClick,
        kind: SettingKind::Choice(CLICK_DBL_OPTS),
        default: "word",
    },
    Entry {
        key: "editor.triple_click",
        cat: Msg::CatEditor,
        label: Msg::LblEditorTripleClick,
        desc: Msg::DescEditorTripleClick,
        kind: SettingKind::Choice(CLICK_TRIPLE_OPTS),
        default: "line",
    },
    Entry {
        key: "editor.dblclick_underscore",
        cat: Msg::CatEditor,
        label: Msg::LblEditorDblUnderscore,
        desc: Msg::DescEditorDblUnderscore,
        kind: SettingKind::Bool,
        default: "off",
    },
    // ★ 그리드 데이터 편집(docs/87 · T-182 · 사용자 09-26): 편집 허용 · 빈 값 = NULL/빈 문자열(D-210) · 문장당 1행 검사 · 적용 뒤 재조회(D-212) · 붙여넣기 상한.
    Entry {
        key: "grid.edit",
        cat: Msg::CatGrid,
        label: Msg::LblGridEdit,
        desc: Msg::DescGridEdit,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "grid.edit_empty",
        cat: Msg::CatGrid,
        label: Msg::LblGridEditEmpty,
        desc: Msg::DescGridEditEmpty,
        kind: SettingKind::Choice(GE_EMPTY_OPTS),
        default: "null",
    },
    Entry {
        key: "grid.edit_refresh",
        cat: Msg::CatGrid,
        label: Msg::LblGridEditRefresh,
        desc: Msg::DescGridEditRefresh,
        kind: SettingKind::Choice(GE_REFRESH_OPTS),
        default: "requery",
    },
    Entry {
        key: "grid.paste_max_rows",
        cat: Msg::CatGrid,
        label: Msg::LblGridPasteMax,
        desc: Msg::DescGridPasteMax,
        kind: SettingKind::Int {
            min: 1,
            max: 1_000_000,
        },
        default: "10000",
    },
    // ★ 행 포커스 배경(사용자 09-22): 셀을 골라도 그 행 전체에 연한 배경 · 색은 `#RRGGBB[AA]`(비면 선택색 35 %).
    Entry {
        key: "grid.row_focus",
        cat: Msg::CatGrid,
        label: Msg::LblGridRowFocus,
        desc: Msg::DescGridRowFocus,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "grid.row_focus_color",
        cat: Msg::CatGrid,
        label: Msg::LblGridRowFocusColor,
        desc: Msg::DescGridRowFocusColor,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "grid.col_min_width",
        cat: Msg::CatGrid,
        label: Msg::LblGridColMin,
        desc: Msg::DescGridColMin,
        kind: SettingKind::Int { min: 16, max: 2000 },
        default: "40",
    },
    Entry {
        key: "grid.col_max_mode",
        cat: Msg::CatGrid,
        label: Msg::LblGridColMaxMode,
        desc: Msg::DescGridColMaxMode,
        kind: SettingKind::Choice(COL_MAX_MODE_OPTS),
        default: "auto",
    },
    Entry {
        key: "grid.col_max_chars",
        cat: Msg::CatGrid,
        label: Msg::LblGridColMax,
        desc: Msg::DescGridColMax,
        kind: SettingKind::Int { min: 4, max: 400 },
        default: "24",
    },
    Entry {
        key: "grid.row_numbers",
        cat: Msg::CatGrid,
        label: Msg::LblGridRowNumbers,
        desc: Msg::DescGridRowNumbers,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ui.hover_color",
        cat: Msg::CatAppearance,
        label: Msg::LblColorHover,
        desc: Msg::DescColorHover,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "ui.pressed_color",
        cat: Msg::CatAppearance,
        label: Msg::LblColorPressed,
        desc: Msg::DescColorPressed,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "ui.color_recent",
        cat: Msg::CatAppearance,
        label: Msg::LblColorRecent,
        desc: Msg::DescColorRecent,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "ui.fade_fast",
        cat: Msg::CatAppearance,
        label: Msg::LblFadeFast,
        desc: Msg::DescFadeFast,
        kind: SettingKind::Int { min: 0, max: 5000 },
        default: "500",
    },
    Entry {
        key: "ui.fade_slow",
        cat: Msg::CatAppearance,
        label: Msg::LblFadeSlow,
        desc: Msg::DescFadeSlow,
        kind: SettingKind::Int { min: 0, max: 5000 },
        default: "1000",
    },
    Entry {
        key: "toolbar.hidden",
        cat: Msg::CatAppearance,
        label: Msg::LblToolbarHidden,
        desc: Msg::DescToolbarHidden,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "toolbar.layout",
        cat: Msg::CatAppearance,
        label: Msg::LblToolbarLayout,
        desc: Msg::DescToolbarLayout,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "tabs.rows",
        cat: Msg::CatAppearance,
        label: Msg::LblTabsRows,
        desc: Msg::DescTabsRows,
        kind: SettingKind::Choice(TAB_ROWS_OPTS),
        default: "multi",
    },
    Entry {
        key: "tabs.tooltip",
        cat: Msg::CatAppearance,
        label: Msg::LblTabsTooltip,
        desc: Msg::DescTabsTooltip,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ── 객체 탐색기 갱신(docs/57 · T-138 · 47 §8의 `meta.*`와 한 벌 — 인텔리센스 메타 저장소가 들어와도 같은 키).
    Entry {
        key: "meta.refresh_on_ddl",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerAutoRefresh,
        desc: Msg::DescExplorerAutoRefresh,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "meta.refresh_on_commit",
        cat: Msg::CatExplorer,
        label: Msg::LblMetaRefreshOnCommit,
        desc: Msg::DescMetaRefreshOnCommit,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "meta.refresh_on_missing",
        cat: Msg::CatExplorer,
        label: Msg::LblMetaRefreshOnMissing,
        desc: Msg::DescMetaRefreshOnMissing,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "meta.refresh_secs",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerRefreshSecs,
        desc: Msg::DescExplorerRefreshSecs,
        kind: SettingKind::Int { min: 0, max: 3600 },
        default: "300",
    },
    Entry {
        key: "meta.refresh_scope",
        cat: Msg::CatExplorer,
        label: Msg::LblMetaRefreshScope,
        desc: Msg::DescMetaRefreshScope,
        kind: SettingKind::Choice(META_SCOPE_OPTS),
        default: "changed",
    },
    Entry {
        key: "meta.refresh_idle_secs",
        cat: Msg::CatExplorer,
        label: Msg::LblMetaRefreshIdle,
        desc: Msg::DescMetaRefreshIdle,
        kind: SettingKind::Int { min: 0, max: 600 },
        default: "5",
    },
    Entry {
        key: "meta.refresh_highlight_ms",
        cat: Msg::CatExplorer,
        label: Msg::LblMetaRefreshHighlight,
        desc: Msg::DescMetaRefreshHighlight,
        kind: SettingKind::Int { min: 0, max: 10000 },
        default: "2000",
    },
    Entry {
        key: "explorer.timeout",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerTimeout,
        desc: Msg::DescExplorerTimeout,
        kind: SettingKind::Int { min: 1, max: 600 },
        default: "15",
    },
    Entry {
        key: "explorer.font_size",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerFontSize,
        desc: Msg::DescExplorerFontSize,
        // 0 = 메뉴 글꼴 크기를 그대로 따른다(DBeaver처럼 풀다운·탐색기 동일 · 사용자 09-15).
        kind: SettingKind::Size { min: 0, max: 40 },
        default: "0",
    },
    Entry {
        key: "explorer.visible",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerVisible,
        desc: Msg::DescExplorerVisible,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "explorer.icons",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerIcons,
        desc: Msg::DescExplorerIcons,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "explorer.disconnect_pick",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerDisconnectPick,
        desc: Msg::DescExplorerDisconnectPick,
        kind: SettingKind::Choice(DISC_PICK_OPTS),
        default: "auto",
    },
    Entry {
        key: "explorer.filter_scope",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerFilterScope,
        desc: Msg::DescExplorerFilterScope,
        kind: SettingKind::Choice(FILTER_SCOPE_OPTS),
        default: "all",
    },
    Entry {
        key: "explorer.share_catalog",
        cat: Msg::CatExplorer,
        label: Msg::LblShareCatalog,
        desc: Msg::DescShareCatalog,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "explorer.show_system_schemas",
        cat: Msg::CatExplorer,
        label: Msg::LblShowSystemSchemas,
        desc: Msg::DescShowSystemSchemas,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "explorer.hide_empty_schemas",
        cat: Msg::CatExplorer,
        label: Msg::LblHideEmptySchemas,
        desc: Msg::DescHideEmptySchemas,
        kind: SettingKind::Bool,
        default: "off",
    },
    // ★ 검색 인덱스(docs/84 · 09-25): 필터 = 서버 전체 객체 대상(스키마 순차 인덱스 · 부분 폴더 · 순차 완성 · 유휴 선적재).
    Entry {
        key: "explorer.search_index",
        cat: Msg::CatExplorer,
        label: Msg::LblSearchIndex,
        desc: Msg::DescSearchIndex,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "explorer.index_max",
        cat: Msg::CatExplorer,
        label: Msg::LblIndexMax,
        desc: Msg::DescIndexMax,
        kind: SettingKind::Int {
            min: 0,
            max: 10_000_000,
        },
        default: "200000",
    },
    Entry {
        key: "explorer.index_hits_max",
        cat: Msg::CatExplorer,
        label: Msg::LblIndexHitsMax,
        desc: Msg::DescIndexHitsMax,
        kind: SettingKind::Int {
            min: 1,
            max: 100_000,
        },
        default: "2000",
    },
    Entry {
        key: "explorer.index_prefetch",
        cat: Msg::CatExplorer,
        label: Msg::LblIndexPrefetch,
        desc: Msg::DescIndexPrefetch,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "explorer.index_idle_ms",
        cat: Msg::CatExplorer,
        label: Msg::LblIndexIdleMs,
        desc: Msg::DescIndexIdleMs,
        kind: SettingKind::Int {
            min: 0,
            max: 600_000,
        },
        default: "250",
    },
    // ★ 메타 3층(docs/85 · 09-25): L2 워머(현재 스키마 컬럼 미리 읽기) · L3 회수(상세·컬럼 TTL/상한).
    Entry {
        key: "meta.warm_columns_max",
        cat: Msg::CatExplorer,
        label: Msg::LblWarmColumnsMax,
        desc: Msg::DescWarmColumnsMax,
        kind: SettingKind::Int {
            min: 0,
            max: 100_000,
        },
        default: "200",
    },
    Entry {
        key: "meta.warm_idle_ms",
        cat: Msg::CatExplorer,
        label: Msg::LblWarmIdleMs,
        desc: Msg::DescWarmIdleMs,
        kind: SettingKind::Int {
            min: 0,
            max: 600_000,
        },
        default: "300",
    },
    Entry {
        key: "meta.detail_max",
        cat: Msg::CatExplorer,
        label: Msg::LblDetailMax,
        desc: Msg::DescDetailMax,
        kind: SettingKind::Int {
            min: 0,
            max: 100_000,
        },
        default: "64",
    },
    Entry {
        key: "meta.detail_ttl_secs",
        cat: Msg::CatExplorer,
        label: Msg::LblDetailTtl,
        desc: Msg::DescDetailTtl,
        kind: SettingKind::Int {
            min: 0,
            max: 86_400,
        },
        default: "300",
    },
    Entry {
        key: "meta.cols_ttl_secs",
        cat: Msg::CatExplorer,
        label: Msg::LblColsTtl,
        desc: Msg::DescColsTtl,
        kind: SettingKind::Int {
            min: 0,
            max: 86_400,
        },
        default: "600",
    },
    Entry {
        key: "meta.disk_cache",
        cat: Msg::CatExplorer,
        label: Msg::LblMetaDiskCache,
        desc: Msg::DescMetaDiskCache,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "meta.warm_comments",
        cat: Msg::CatExplorer,
        label: Msg::LblWarmComments,
        desc: Msg::DescWarmComments,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "gen.qualified",
        cat: Msg::CatExplorer,
        label: Msg::GenOptQualified,
        desc: Msg::DescGenQualified,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "gen.compact",
        cat: Msg::CatExplorer,
        label: Msg::GenOptCompact,
        desc: Msg::DescGenCompact,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "gen.full_ddl",
        cat: Msg::CatExplorer,
        label: Msg::GenOptFull,
        desc: Msg::DescGenFull,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "gen.separate_fk",
        cat: Msg::CatExplorer,
        label: Msg::GenOptSepFk,
        desc: Msg::DescGenSepFk,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "explorer.keep_offline",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerKeepOffline,
        desc: Msg::DescExplorerKeepOffline,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "explorer.tooltip",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerTooltip,
        desc: Msg::DescExplorerTooltip,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 탐색기 타입어헤드(nexa-beep 이식 · 사용자 09-19): 글자를 치면 접두 항목으로 · 한글 직접 조합 · ↑/↓ 매치 순환 · HUD.
    Entry {
        key: "explorer.typeahead",
        cat: Msg::CatExplorer,
        label: Msg::LblTypeahead,
        desc: Msg::DescTypeahead,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "explorer.typeahead_timeout",
        cat: Msg::CatExplorer,
        label: Msg::LblTypeaheadTimeout,
        desc: Msg::DescTypeaheadTimeout,
        kind: SettingKind::Int {
            min: 200,
            max: 60000,
        },
        default: "2000",
    },
    Entry {
        key: "explorer.typeahead_space",
        cat: Msg::CatExplorer,
        label: Msg::LblTypeaheadSpace,
        desc: Msg::DescTypeaheadSpace,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "explorer.typeahead_special",
        cat: Msg::CatExplorer,
        label: Msg::LblTypeaheadSpecial,
        desc: Msg::DescTypeaheadSpecial,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "explorer.typeahead_pos",
        cat: Msg::CatExplorer,
        label: Msg::LblTypeaheadPos,
        desc: Msg::DescTypeaheadPos,
        kind: SettingKind::Position,
        default: "bottom_left",
    },
    Entry {
        key: "grid.scroll",
        cat: Msg::CatGrid,
        label: Msg::LblGridScroll,
        desc: Msg::DescGridScroll,
        kind: SettingKind::Choice(SCROLL_OPTS),
        default: "pixel",
    },
    // 미니맵(Sublime · T-97 · 09-16).
    Entry {
        key: "editor.minimap",
        cat: Msg::CatEditor,
        label: Msg::LblEditorMinimap,
        desc: Msg::DescEditorMinimap,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 탭 유형별 활성 상단 줄 색(09-22 · 빈 값 = 기본: 스크립트 warn · 파일 accent · 미리보기 text_dim).
    Entry {
        key: "editor.tab_line_scratch",
        cat: Msg::CatEditor,
        label: Msg::LblEditorTabLineScratch,
        desc: Msg::DescEditorTabLineScratch,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.tab_line_file",
        cat: Msg::CatEditor,
        label: Msg::LblEditorTabLineFile,
        desc: Msg::DescEditorTabLineFile,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.tab_line_preview",
        cat: Msg::CatEditor,
        label: Msg::LblEditorTabLinePreview,
        desc: Msg::DescEditorTabLinePreview,
        kind: SettingKind::Text,
        default: "",
    },
    // ★ 동시 편집(사용자 09-22): 탭 바에서 Shift/Ctrl+클릭으로 나란히 보이는 탭 수 상한.
    Entry {
        key: "editor.split_max",
        cat: Msg::CatEditor,
        label: Msg::LblEditorSplitMax,
        desc: Msg::DescEditorSplitMax,
        kind: SettingKind::Int { min: 1, max: 4 },
        default: "3",
    },
    Entry {
        key: "editor.minimap_width",
        cat: Msg::CatEditor,
        label: Msg::LblEditorMinimapWidth,
        desc: Msg::DescEditorMinimapWidth,
        kind: SettingKind::Int { min: 20, max: 400 },
        // 09-24 사용자: 160 → 120.
        default: "120",
    },
    Entry {
        key: "editor.minimap_box_color",
        cat: Msg::CatEditor,
        label: Msg::LblMinimapBoxColor,
        desc: Msg::DescMinimapBoxColor,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.minimap_border",
        cat: Msg::CatEditor,
        label: Msg::LblMinimapBorder,
        desc: Msg::DescMinimapBorder,
        kind: SettingKind::Bool,
        default: "off",
    },
    // 미니맵 1순위(docs/46 §2 · T-110): 뷰포트 표시 · 클릭 동작 · 찾기 띠 · 오류 줄.
    Entry {
        key: "editor.minimap_viewport",
        cat: Msg::CatEditor,
        label: Msg::LblMinimapViewport,
        desc: Msg::DescMinimapViewport,
        kind: SettingKind::Choice(MINIMAP_VIEWPORT_OPTS),
        default: "always",
    },
    Entry {
        key: "editor.minimap_click",
        cat: Msg::CatEditor,
        label: Msg::LblMinimapClick,
        desc: Msg::DescMinimapClick,
        kind: SettingKind::Choice(MINIMAP_CLICK_OPTS),
        default: "center",
    },
    Entry {
        key: "editor.minimap_find",
        cat: Msg::CatEditor,
        label: Msg::LblMinimapFind,
        desc: Msg::DescMinimapFind,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "editor.minimap_errors",
        cat: Msg::CatEditor,
        label: Msg::LblMinimapErrors,
        desc: Msg::DescMinimapErrors,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 편집기 휠 단위(사용자 09-16 · 픽셀 스크롤 도입과 함께 그리드와 같은 선택지).
    Entry {
        key: "editor.scroll",
        cat: Msg::CatEditor,
        label: Msg::LblEditorScroll,
        desc: Msg::DescEditorScroll,
        kind: SettingKind::Choice(SCROLL_OPTS),
        default: "pixel",
    },
    // ★ 단축키(사용자 09-15) — 값 문법은 `nexa-sql::keymap`(Sublime Text 기본 · 비우면 플랫폼 기본).
    Entry {
        key: "key.preset",
        cat: Msg::CatKeys,
        label: Msg::LblKeyPreset,
        desc: Msg::DescKeyPreset,
        kind: SettingKind::Choice(KEY_PRESET_OPTS),
        default: "auto",
    },
    Entry {
        key: "key.view.palette",
        cat: Msg::CatKeys,
        label: Msg::MnCommandPalette,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.file.new",
        cat: Msg::CatKeys,
        label: Msg::MnNew,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.file.close_tab",
        cat: Msg::CatKeys,
        label: Msg::MnCloseTab,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.tab.next",
        cat: Msg::CatKeys,
        label: Msg::MnNextTab,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.tab.prev",
        cat: Msg::CatKeys,
        label: Msg::MnPrevTab,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.run.statement",
        cat: Msg::CatKeys,
        label: Msg::MnRunStatement,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.run.all",
        cat: Msg::CatKeys,
        label: Msg::MnRunAll,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.run.commit",
        cat: Msg::CatKeys,
        label: Msg::MnCommit,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.run.rollback",
        cat: Msg::CatKeys,
        label: Msg::MnRollback,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.run.explain",
        cat: Msg::CatKeys,
        label: Msg::MnExplain,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.conn.toggle",
        cat: Msg::CatKeys,
        label: Msg::MnConnect,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.view.log",
        cat: Msg::CatKeys,
        label: Msg::MnLogWindow,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.view.theme",
        cat: Msg::CatKeys,
        label: Msg::MnTheme,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.view.lang",
        cat: Msg::CatKeys,
        label: Msg::MnLanguage,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.view.colors",
        cat: Msg::CatKeys,
        label: Msg::MnColors,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    // ★ 객체 상세 패널(docs/86 · T-223 · 09-25): View ▸ Object Details · 높이(스플리터 자동 기억) · 축소 상태.
    Entry {
        key: "explorer.details",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerDetails,
        desc: Msg::DescExplorerDetails,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "explorer.details_h",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerDetailsH,
        desc: Msg::DescExplorerDetailsH,
        kind: SettingKind::Int { min: 80, max: 1200 },
        default: "240",
    },
    Entry {
        key: "explorer.details_collapsed",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerDetailsCollapsed,
        desc: Msg::DescExplorerDetailsCollapsed,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "explorer.width",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerWidth,
        desc: Msg::DescExplorerWidth,
        kind: SettingKind::Int { min: 160, max: 800 },
        default: "260",
    },
    // 편집기/결과 상하 분할 비율 — 스플리터 드래그로 바뀌고 자동 기억(HIDDEN · 사용자 09-16).
    Entry {
        key: "layout.editor_split_pct",
        cat: Msg::CatEditor,
        label: Msg::LblEditorSplit,
        desc: Msg::DescEditorSplit,
        kind: SettingKind::Int { min: 10, max: 90 },
        default: "50",
    },
    Entry {
        key: "key.view.explorer",
        cat: Msg::CatKeys,
        label: Msg::MnExplorer,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.view.keys",
        cat: Msg::CatKeys,
        label: Msg::MnKeys,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    // ★ 파일 대화상자(T-74) — 마지막 폴더·최근 파일·숨김 표시(자동 기억 · HIDDEN).
    Entry {
        key: "file.last_dir",
        cat: Msg::CatFiles,
        label: Msg::LblFileLastDir,
        desc: Msg::DescFileLastDir,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "file.recent",
        cat: Msg::CatFiles,
        label: Msg::LblFileRecent,
        desc: Msg::DescFileRecent,
        kind: SettingKind::Text,
        default: "",
    },
    // ★ 프로젝트(docs/67 · 사용자 09-22): 마지막/최근 프로젝트 파일 · 미리보기 탭 · 필터 열거 상한.
    Entry {
        key: "project.last",
        cat: Msg::CatProject,
        label: Msg::LblProjectLast,
        desc: Msg::DescProjectLast,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "project.recent",
        cat: Msg::CatProject,
        label: Msg::LblProjectRecent,
        desc: Msg::DescProjectRecent,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "project.restore_last",
        cat: Msg::CatProject,
        label: Msg::LblProjectRestoreLast,
        desc: Msg::DescProjectRestoreLast,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "project.preview_tab",
        cat: Msg::CatProject,
        label: Msg::LblProjectPreviewTab,
        desc: Msg::DescProjectPreviewTab,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "project.icons",
        cat: Msg::CatProject,
        label: Msg::LblProjectIcons,
        desc: Msg::DescProjectIcons,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "project.auto_reveal",
        cat: Msg::CatProject,
        label: Msg::LblProjectAutoReveal,
        desc: Msg::DescProjectAutoReveal,
        kind: SettingKind::Bool,
        default: "off",
    },
    // ★ 프로젝트 자동 저장(사용자 09-23): 기본 켬 · 30초(작업 환경 JSON은 작고 바뀔 때만 쓴다 — 손실 창 ≤ 30초 · 디스크 부하 0에 가깝다).
    Entry {
        key: "project.autosave",
        cat: Msg::CatProject,
        label: Msg::LblProjectAutosave,
        desc: Msg::DescProjectAutosave,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "project.backup_days",
        cat: Msg::CatProject,
        label: Msg::LblProjectBackupDays,
        desc: Msg::DescProjectBackupDays,
        kind: SettingKind::Int { min: 1, max: 365 },
        default: "7",
    },
    Entry {
        key: "project.autosave_secs",
        cat: Msg::CatProject,
        label: Msg::LblProjectAutosaveSecs,
        desc: Msg::DescProjectAutosaveSecs,
        kind: SettingKind::Int { min: 5, max: 3600 },
        default: "30",
    },
    Entry {
        key: "project.scan_max",
        cat: Msg::CatProject,
        label: Msg::LblProjectScanMax,
        desc: Msg::DescProjectScanMax,
        kind: SettingKind::Int {
            min: 0,
            max: 2_000_000,
        },
        default: "0",
    },
    // 필터 열거 워커 스레드 수(사용자 09-23 "별도 스레드 · 분할 병렬" · 39 §3 부하원) — 0 = 코어 수/2.
    Entry {
        key: "project.scan_threads",
        cat: Msg::CatProject,
        label: Msg::LblProjectScanThreads,
        desc: Msg::DescProjectScanThreads,
        kind: SettingKind::Int { min: 0, max: 16 },
        default: "4",
    },
    Entry {
        key: "file.show_hidden",
        cat: Msg::CatFiles,
        label: Msg::LblFileShowHidden,
        desc: Msg::DescFileShowHidden,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "file.show_dot",
        cat: Msg::CatFiles,
        label: Msg::LblFileShowDot,
        desc: Msg::DescFileShowDot,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 줄끝(docs/38 · 사용자 09-16 · DBeaver ▸ General ▸ Workspace "New text file line delimiter" 분류 = Files).
    Entry {
        key: "file.eol_new",
        cat: Msg::CatFiles,
        label: Msg::LblEolNew,
        desc: Msg::DescEolNew,
        kind: SettingKind::Choice(EOL_NEW_OPTS),
        default: "auto",
    },
    // ── 큰 파일(docs/59 §4 · T-141 · T-143).
    Entry {
        key: "file.large_l1_mb",
        cat: Msg::CatFiles,
        label: Msg::LblLargeL1Mb,
        desc: Msg::DescLargeL1,
        kind: SettingKind::Int { min: 0, max: 4096 },
        default: "5",
    },
    Entry {
        key: "file.large_l1_lines",
        cat: Msg::CatFiles,
        label: Msg::LblLargeL1Lines,
        desc: Msg::DescLargeL1,
        kind: SettingKind::Int {
            min: 0,
            max: 100_000_000,
        },
        default: "100000",
    },
    Entry {
        key: "file.large_l2_mb",
        cat: Msg::CatFiles,
        label: Msg::LblLargeL2Mb,
        desc: Msg::DescLargeL2,
        kind: SettingKind::Int { min: 0, max: 4096 },
        default: "20",
    },
    Entry {
        key: "file.large_l2_lines",
        cat: Msg::CatFiles,
        label: Msg::LblLargeL2Lines,
        desc: Msg::DescLargeL2,
        kind: SettingKind::Int {
            min: 0,
            max: 100_000_000,
        },
        default: "300000",
    },
    // 큰 파일 단계별 기능 제한(사용자 09-21): 확장 효과(괄호 색·짝 표 등 — Rainbow Pairs)와 구문 강조를 끄기 시작하는 단계.
    Entry {
        key: "file.large_ext_level",
        cat: Msg::CatFiles,
        label: Msg::LblLargeExtLevel,
        desc: Msg::DescLargeExtLevel,
        kind: SettingKind::Choice(LARGE_LEVEL_OPTS),
        default: "l1",
    },
    Entry {
        key: "file.large_syntax_level",
        cat: Msg::CatFiles,
        label: Msg::LblLargeSyntaxLevel,
        desc: Msg::DescLargeSyntaxLevel,
        kind: SettingKind::Choice(LARGE_LEVEL_OPTS),
        default: "l2",
    },
    Entry {
        key: "file.large_ask_mb",
        cat: Msg::CatFiles,
        label: Msg::LblLargeAskMb,
        desc: Msg::DescLargeAskMb,
        kind: SettingKind::Int { min: 0, max: 65536 },
        default: "50",
    },
    Entry {
        key: "file.large_head_mb",
        cat: Msg::CatFiles,
        label: Msg::LblLargeHeadMb,
        desc: Msg::DescLargeHeadMb,
        kind: SettingKind::Int { min: 1, max: 1024 },
        default: "8",
    },
    // ★ 다중 열기 상한(사용자 09-22): 대화상자에서 여러 파일을 골라도 이 개수까지만(고른 순서).
    Entry {
        key: "file.open_max",
        cat: Msg::CatFiles,
        label: Msg::LblFileOpenMax,
        desc: Msg::DescFileOpenMax,
        kind: SettingKind::Int { min: 1, max: 50 },
        default: "10",
    },
    Entry {
        key: "file.async_load_mb",
        cat: Msg::CatFiles,
        label: Msg::LblAsyncLoadMb,
        desc: Msg::DescAsyncLoadMb,
        kind: SettingKind::Int { min: 1, max: 4096 },
        default: "8",
    },
    Entry {
        key: "file.load_progress_ms",
        cat: Msg::CatFiles,
        label: Msg::LblLoadProgressMs,
        desc: Msg::DescLoadProgressMs,
        kind: SettingKind::Int { min: 0, max: 10000 },
        default: "300",
    },
    // ── 외부 파일 변경(docs/58 · T-140).
    Entry {
        key: "file.external_change",
        cat: Msg::CatFiles,
        label: Msg::LblExtChange,
        desc: Msg::DescExtChange,
        kind: SettingKind::Choice(EXT_CHANGE_OPTS),
        default: "auto",
    },
    Entry {
        key: "file.external_merge",
        cat: Msg::CatFiles,
        label: Msg::LblExtMerge,
        desc: Msg::DescExtMerge,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "file.external_check",
        cat: Msg::CatFiles,
        label: Msg::LblExtCheck,
        desc: Msg::DescExtCheck,
        kind: SettingKind::Choice(EXT_CHECK_OPTS),
        default: "focus_poll",
    },
    Entry {
        key: "file.external_poll_ms",
        cat: Msg::CatFiles,
        label: Msg::LblExtPollMs,
        desc: Msg::DescExtPollMs,
        kind: SettingKind::Int { min: 0, max: 60000 },
        default: "2000",
    },
    Entry {
        key: "file.external_merge_max_kb",
        cat: Msg::CatFiles,
        label: Msg::LblExtMergeMaxKb,
        desc: Msg::DescExtMergeMaxKb,
        kind: SettingKind::Int {
            min: 16,
            max: 65536,
        },
        default: "2048",
    },
    Entry {
        key: "file.external_settle_ms",
        cat: Msg::CatFiles,
        label: Msg::LblExtSettleMs,
        desc: Msg::DescExtSettleMs,
        kind: SettingKind::Int { min: 0, max: 5000 },
        default: "300",
    },
    Entry {
        key: "file.external_backup_keep",
        cat: Msg::CatFiles,
        label: Msg::LblExtBackupKeep,
        desc: Msg::DescExtBackupKeep,
        kind: SettingKind::Int { min: 0, max: 200 },
        default: "10",
    },
    Entry {
        key: "file.overwrite_confirm_ms",
        cat: Msg::CatFiles,
        label: Msg::LblOverwriteConfirmMs,
        desc: Msg::DescOverwriteConfirmMs,
        kind: SettingKind::Int { min: 0, max: 60000 },
        default: "5000",
    },
    Entry {
        key: "file.eol_save",
        cat: Msg::CatFiles,
        label: Msg::LblEolSave,
        desc: Msg::DescEolSave,
        kind: SettingKind::Choice(EOL_SAVE_OPTS),
        default: "keep",
    },
    Entry {
        key: "key.file.open",
        cat: Msg::CatKeys,
        label: Msg::MnOpen,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.file.save",
        cat: Msg::CatKeys,
        label: Msg::MnSave,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.file.save_as",
        cat: Msg::CatKeys,
        label: Msg::MnSaveAs,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.expand_selection",
        cat: Msg::CatKeys,
        label: Msg::MnExpandSelection,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.select_all_occurrences",
        cat: Msg::CatKeys,
        label: Msg::MnSelectAllOccurrences,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.skip_occurrence",
        cat: Msg::CatKeys,
        label: Msg::MnSkipOccurrence,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.prefs",
        cat: Msg::CatKeys,
        label: Msg::MnPreferences,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.conn.disconnect",
        cat: Msg::CatKeys,
        label: Msg::TipDisconnect,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.find",
        cat: Msg::CatKeys,
        label: Msg::MnFind,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.replace",
        cat: Msg::CatKeys,
        label: Msg::MnReplace,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.find_next",
        cat: Msg::CatKeys,
        label: Msg::MnFindNext,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.find_prev",
        cat: Msg::CatKeys,
        label: Msg::MnFindPrev,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.undo",
        cat: Msg::CatKeys,
        label: Msg::MnUndo,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.redo",
        cat: Msg::CatKeys,
        label: Msg::MnRedo,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.soft_undo",
        cat: Msg::CatKeys,
        label: Msg::MnSoftUndo,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.soft_redo",
        cat: Msg::CatKeys,
        label: Msg::MnSoftRedo,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.cut",
        cat: Msg::CatKeys,
        label: Msg::MnCut,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.copy",
        cat: Msg::CatKeys,
        label: Msg::MnCopy,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.paste",
        cat: Msg::CatKeys,
        label: Msg::MnPaste,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.edit.select_all",
        cat: Msg::CatKeys,
        label: Msg::MnSelectAll,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "input.scroll_natural",
        cat: Msg::CatInput,
        label: Msg::LblScrollNatural,
        desc: Msg::DescScrollNatural,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "input.hangul_compose",
        cat: Msg::CatInput,
        label: Msg::LblHangulCompose,
        desc: Msg::DescHangulCompose,
        kind: SettingKind::Choice(HANGUL_COMPOSE_OPTS),
        default: "auto",
    },
    Entry {
        key: "statusbar.git",
        cat: Msg::CatWindow,
        label: Msg::LblStatusGit,
        desc: Msg::DescStatusGit,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "statusbar.git_secs",
        cat: Msg::CatWindow,
        label: Msg::LblStatusGitSecs,
        desc: Msg::DescStatusGitSecs,
        kind: SettingKind::Int { min: 2, max: 600 },
        default: "15",
    },
    // 창 크기 기억(사용자 09-17 · 숨김 · "w,h" 논리 px · 닫힐 때 앱이 쓴다).
    Entry {
        key: "window.main_size",
        cat: Msg::CatWindow,
        label: Msg::LblWindowSizeMemo,
        desc: Msg::DescWindowSizeMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.login_size",
        cat: Msg::CatWindow,
        label: Msg::LblWindowSizeMemo,
        desc: Msg::DescWindowSizeMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.log_size",
        cat: Msg::CatWindow,
        label: Msg::LblWindowSizeMemo,
        desc: Msg::DescWindowSizeMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.sessions_size",
        cat: Msg::CatWindow,
        label: Msg::LblWindowSizeMemo,
        desc: Msg::DescWindowSizeMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.txlog_size",
        cat: Msg::CatWindow,
        label: Msg::LblWindowSizeMemo,
        desc: Msg::DescWindowSizeMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.prefs_size",
        cat: Msg::CatWindow,
        label: Msg::LblWindowSizeMemo,
        desc: Msg::DescWindowSizeMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.main_pos",
        cat: Msg::CatWindow,
        label: Msg::LblWindowPosMemo,
        desc: Msg::DescWindowPosMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.login_pos",
        cat: Msg::CatWindow,
        label: Msg::LblWindowPosMemo,
        desc: Msg::DescWindowPosMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.log_pos",
        cat: Msg::CatWindow,
        label: Msg::LblWindowPosMemo,
        desc: Msg::DescWindowPosMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.sessions_pos",
        cat: Msg::CatWindow,
        label: Msg::LblWindowPosMemo,
        desc: Msg::DescWindowPosMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.txlog_pos",
        cat: Msg::CatWindow,
        label: Msg::LblWindowPosMemo,
        desc: Msg::DescWindowPosMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.prefs_pos",
        cat: Msg::CatWindow,
        label: Msg::LblWindowPosMemo,
        desc: Msg::DescWindowPosMemo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "window.monitor",
        cat: Msg::CatWindow,
        label: Msg::LblWindowMonitor,
        desc: Msg::DescWindowMonitor,
        kind: SettingKind::Int { min: 0, max: 8 },
        default: "0",
    },
    Entry {
        key: "window.always_on_top",
        cat: Msg::CatWindow,
        label: Msg::LblWindowOnTop,
        desc: Msg::DescWindowOnTop,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "window.focus",
        cat: Msg::CatWindow,
        label: Msg::LblWindowFocus,
        desc: Msg::DescWindowFocus,
        kind: SettingKind::Choice(WINDOW_FOCUS_OPTS),
        default: "group",
    },
    Entry {
        key: "probe.enabled",
        cat: Msg::CatConnection,
        label: Msg::LblProbeEnabled,
        desc: Msg::DescProbeEnabled,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "probe.max_retries",
        cat: Msg::CatConnection,
        label: Msg::LblProbeMaxRetries,
        desc: Msg::DescProbeMaxRetries,
        kind: SettingKind::Int { min: 0, max: 20 },
        default: "5",
    },
    Entry {
        key: "probe.timeout",
        cat: Msg::CatConnection,
        label: Msg::LblProbeTimeout,
        desc: Msg::DescProbeTimeout,
        kind: SettingKind::Int { min: 1, max: 30 },
        default: "2",
    },
    Entry {
        key: "probe.interval",
        cat: Msg::CatConnection,
        label: Msg::LblProbeInterval,
        desc: Msg::DescProbeInterval,
        kind: SettingKind::Int { min: 5, max: 3600 },
        default: "60",
    },
    Entry {
        key: "probe.retry_delay",
        cat: Msg::CatConnection,
        label: Msg::LblProbeRetryDelay,
        desc: Msg::DescProbeRetryDelay,
        kind: SettingKind::Int { min: 5, max: 3600 },
        default: "10",
    },
    // ── 비노출 설정(사용자 09-14 "자주 바꾸지 않을 값은 비노출 설정으로") — 구현 상수의 설정화. `nsql config list all`로만 보인다.
    Entry {
        key: "conn.delete_confirm_ms",
        cat: Msg::CatConnection,
        label: Msg::LblDeleteConfirmMs,
        desc: Msg::DescDeleteConfirmMs,
        kind: SettingKind::Int {
            min: 1000,
            max: 60000,
        },
        default: "5000",
    },
    Entry {
        key: "conn.close_after_connect_ms",
        cat: Msg::CatConnection,
        label: Msg::LblCloseAfterConnectMs,
        desc: Msg::DescCloseAfterConnectMs,
        kind: SettingKind::Int { min: 0, max: 5000 },
        default: "450",
    },
    Entry {
        key: "conn.window_w",
        cat: Msg::CatConnection,
        label: Msg::LblConnWindowW,
        desc: Msg::DescConnWindowW,
        kind: SettingKind::Int {
            min: 400,
            max: 2000,
        },
        default: "748",
    },
    Entry {
        key: "conn.window_h",
        cat: Msg::CatConnection,
        label: Msg::LblConnWindowH,
        desc: Msg::DescConnWindowH,
        kind: SettingKind::Int {
            min: 300,
            max: 1600,
        },
        default: "526",
    },
    Entry {
        key: "conn.panel_w",
        cat: Msg::CatConnection,
        label: Msg::LblConnPanelW,
        desc: Msg::DescConnPanelW,
        kind: SettingKind::Int { min: 200, max: 800 },
        default: "292",
    },
    Entry {
        key: "conn.port_w",
        cat: Msg::CatConnection,
        label: Msg::LblConnPortW,
        desc: Msg::DescConnPortW,
        kind: SettingKind::Int { min: 40, max: 160 },
        default: "72",
    },
    Entry {
        key: "conn.button_scale_pct",
        cat: Msg::CatConnection,
        label: Msg::LblConnButtonScale,
        desc: Msg::DescConnButtonScale,
        kind: SettingKind::Int { min: 100, max: 250 },
        default: "132",
    },
    Entry {
        key: "ui.toast_secs",
        cat: Msg::CatAppearance,
        label: Msg::LblToastSecs,
        desc: Msg::DescToastSecs,
        kind: SettingKind::Int { min: 1, max: 60 },
        default: "3",
    },
    Entry {
        key: "ui.toast_alpha",
        cat: Msg::CatAppearance,
        label: Msg::LblToastAlpha,
        desc: Msg::DescToastAlpha,
        kind: SettingKind::Int { min: 30, max: 100 },
        default: "85",
    },
    // ★ 토스트 남은 시간 표시(사용자 09-22): 오른쪽 끝 세로 막대 + 진척에 따라 투명해짐. `ui.toast_fade_to` = 수명 끝의 불투명도 비율(HIDDEN).
    Entry {
        key: "ui.toast_progress",
        cat: Msg::CatAppearance,
        label: Msg::LblToastProgress,
        desc: Msg::DescToastProgress,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ui.toast_fade_to",
        cat: Msg::CatAppearance,
        label: Msg::LblToastFadeTo,
        desc: Msg::DescToastFadeTo,
        kind: SettingKind::Int { min: 0, max: 100 },
        default: "35",
    },
    Entry {
        key: "ui.toast_bar_spent",
        cat: Msg::CatAppearance,
        label: Msg::LblToastBarSpent,
        desc: Msg::DescToastBarSpent,
        kind: SettingKind::Int { min: 0, max: 100 },
        default: "30",
    },
    // 가린 입력란 IME 안내(사용자 09-22): 비밀번호 칸에 글자가 들어올 때 입력 언어가 라틴이 아니면 마우스 옆에 N초(0 = 끔).
    // ★ 메뉴 항목 폭 상한(사용자 09-22): 최근 파일·프로젝트 경로가 길면 가운데 …로 줄인다(Alt = 전체 경로).
    Entry {
        key: "ui.menu_max_width",
        cat: Msg::CatAppearance,
        label: Msg::LblMenuMaxWidth,
        desc: Msg::DescMenuMaxWidth,
        kind: SettingKind::Int {
            min: 200,
            max: 2000,
        },
        default: "480",
    },
    Entry {
        key: "ui.ime_hint",
        cat: Msg::CatAppearance,
        label: Msg::LblImeHint,
        desc: Msg::DescImeHint,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ui.copy_feedback_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblCopyFeedbackMs,
        desc: Msg::DescCopyFeedbackMs,
        kind: SettingKind::Int {
            min: 300,
            max: 10000,
        },
        default: "2000",
    },
    Entry {
        key: "ui.tooltip_delay_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblTooltipDelayMs,
        desc: Msg::DescTooltipDelayMs,
        kind: SettingKind::Int {
            min: 100,
            max: 5000,
        },
        default: "600",
    },
    Entry {
        key: "ui.dblclick_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblDblclickMs,
        desc: Msg::DescDblclickMs,
        kind: SettingKind::Int {
            min: 100,
            max: 1000,
        },
        default: "400",
    },
    Entry {
        key: "ui.slide_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblSlideMs,
        desc: Msg::DescSlideMs,
        kind: SettingKind::Int { min: 0, max: 1000 },
        default: "200",
    },
    Entry {
        key: "ui.hover_intent_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblHoverIntentMs,
        desc: Msg::DescHoverIntentMs,
        kind: SettingKind::Int { min: 0, max: 500 },
        default: "70",
    },
    Entry {
        key: "ui.fade_out_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblFadeOutMs,
        desc: Msg::DescFadeOutMs,
        kind: SettingKind::Int { min: 0, max: 2000 },
        default: "220",
    },
    Entry {
        key: "probe.max_inflight",
        cat: Msg::CatConnection,
        label: Msg::LblProbeMaxInflight,
        desc: Msg::DescProbeMaxInflight,
        kind: SettingKind::Int { min: 1, max: 64 },
        default: "16",
    },
    Entry {
        key: "connect.max_concurrent",
        cat: Msg::CatConnection,
        label: Msg::LblMaxConcurrent,
        desc: Msg::DescMaxConcurrent,
        kind: SettingKind::Int { min: 1, max: 16 },
        default: "4",
    },
    // ★ Oracle 클라이언트(Instant Client · 사용자 09-21): 자동 = 지금 환경에서 찾아 **읽기 전용**으로 보여 준다 · 직접 지정 = 폴더와
    //   TNS_ADMIN만 바꿀 수 있고(`DEPENDS`) 거기서 나오는 파일 경로들은 읽기 전용(`INFO_KEYS` — 저장하지 않는 계산 값).
    Entry {
        key: "oracle.client_mode",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblOraClientMode,
        desc: Msg::DescOraClientMode,
        kind: SettingKind::Choice(ORA_CLIENT_OPTS),
        default: "auto",
    },
    Entry {
        key: "oracle.client_dir",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblOraClientDir,
        desc: Msg::DescOraClientDir,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "oracle.tns_admin",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblOraTnsAdmin,
        desc: Msg::DescOraTnsAdmin,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "oracle.info_library",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblOraInfoLibrary,
        desc: Msg::DescOraInfoLibrary,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "oracle.info_version",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblOraInfoVersion,
        desc: Msg::DescOraInfoVersion,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "oracle.info_tnsnames",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblOraInfoTnsnames,
        desc: Msg::DescOraInfoTnsnames,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "oracle.info_sqlnet",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblOraInfoSqlnet,
        desc: Msg::DescOraInfoSqlnet,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "mssql.info_driver",
        cat: Msg::CatDbmsMssql,
        label: Msg::LblDbmsInfoDriver,
        desc: Msg::DescDbmsInfo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "pg.info_driver",
        cat: Msg::CatDbmsPostgres,
        label: Msg::LblDbmsInfoDriver,
        desc: Msg::DescDbmsInfo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "sqlite.info_driver",
        cat: Msg::CatDbmsSqlite,
        label: Msg::LblDbmsInfoDriver,
        desc: Msg::DescDbmsInfo,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "oracle.live.source",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblLiveSource,
        desc: Msg::DescLiveSource,
        kind: SettingKind::Choice(LIVE_SOURCE_OPTS),
        default: "session",
    },
    Entry {
        key: "oracle.live.interval_ms",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblLiveInterval,
        desc: Msg::DescLiveInterval,
        kind: SettingKind::Int {
            min: 250,
            max: 60_000,
        },
        default: "1000",
    },
    Entry {
        key: "oracle.live.table",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblLiveTable,
        desc: Msg::DescLiveTable,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "oracle.live.ts_col",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblLiveTsCol,
        desc: Msg::DescLiveTsCol,
        kind: SettingKind::Text,
        default: "LOG_TIME",
    },
    Entry {
        key: "oracle.live.text_col",
        cat: Msg::CatDbmsOracle,
        label: Msg::LblLiveTextCol,
        desc: Msg::DescLiveTextCol,
        kind: SettingKind::Text,
        default: "LOG_TEXT",
    },
    Entry {
        key: "connect.auto_reconnect",
        cat: Msg::CatConnection,
        label: Msg::LblAutoReconnect,
        desc: Msg::DescAutoReconnect,
        kind: SettingKind::Bool,
        default: "on",
    },
    // SQL Server 암호화 범위(T-108 · docs/44 §4): 로그인만 암호화하면 실행 취소(TDS Attention)가 접속을 유지한 채 된다.
    Entry {
        key: "mssql.encrypt",
        cat: Msg::CatDbmsMssql,
        label: Msg::LblMssqlEncrypt,
        desc: Msg::DescMssqlEncrypt,
        kind: SettingKind::Choice(MSSQL_ENCRYPT_OPTS),
        default: "required",
    },
    Entry {
        key: "mssql.cancel",
        cat: Msg::CatDbmsMssql,
        label: Msg::LblMssqlCancel,
        desc: Msg::DescMssqlCancel,
        kind: SettingKind::Choice(MSSQL_CANCEL_OPTS),
        default: "attention",
    },
    Entry {
        key: "connect.remember_session_password",
        cat: Msg::CatConnection,
        label: Msg::LblRememberSessionPw,
        desc: Msg::DescRememberSessionPw,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "connect.reconnect_same",
        cat: Msg::CatConnection,
        label: Msg::LblReconnectSame,
        desc: Msg::DescReconnectSame,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "run.toast",
        cat: Msg::CatSession,
        label: Msg::LblRunToast,
        desc: Msg::DescRunToast,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "run.toast_hide_secs",
        cat: Msg::CatSession,
        label: Msg::LblRunToastHide,
        desc: Msg::DescRunToastHide,
        kind: SettingKind::Int { min: 0, max: 600 },
        default: "5",
    },
    // ★ 실행 카드 갱신 주기(사용자 09-22): 경과 시간 `HH:MM:SS.mmm`·카운트다운을 이 주기로만 다시 그린다(향상 모드 = 1000).
    Entry {
        key: "run.toast_tick_ms",
        cat: Msg::CatSession,
        label: Msg::LblRunToastTick,
        desc: Msg::DescRunToastTick,
        kind: SettingKind::Int { min: 30, max: 5000 },
        default: "100",
    },
    Entry {
        key: "run.toast_follow",
        cat: Msg::CatSession,
        label: Msg::LblRunToastFollow,
        desc: Msg::DescRunToastFollow,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "run.toast_max",
        cat: Msg::CatSession,
        label: Msg::LblRunToastMax,
        desc: Msg::DescRunToastMax,
        kind: SettingKind::Int { min: 1, max: 500 },
        default: "30",
    },
    Entry {
        key: "run.after_statement",
        cat: Msg::CatSession,
        label: Msg::LblRunAfter,
        desc: Msg::DescRunAfter,
        kind: SettingKind::Choice(RUN_AFTER_OPTS),
        default: "stay",
    },
    Entry {
        key: "session.autocommit",
        cat: Msg::CatSession,
        label: Msg::LblAutocommit,
        desc: Msg::DescAutocommit,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ── 세션 컨텍스트(docs/52) — 전용 세션·유휴 닫기. 기본 사상 = 1 인스턴스 · 1 서버 · 1 계정(전용 세션은 예외라 상한을 둔다).
    Entry {
        key: "session.private_connect",
        cat: Msg::CatSession,
        label: Msg::LblSessPrivateConnect,
        desc: Msg::DescSessPrivateConnect,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "session.max_shared",
        cat: Msg::CatSession,
        label: Msg::LblSessMaxShared,
        desc: Msg::DescSessMaxShared,
        kind: SettingKind::Int { min: 1, max: 32 },
        default: "8",
    },
    Entry {
        key: "session.max_private",
        cat: Msg::CatSession,
        label: Msg::LblSessMaxPrivate,
        desc: Msg::DescSessMaxPrivate,
        kind: SettingKind::Int { min: 0, max: 64 },
        default: "8",
    },
    Entry {
        key: "session.idle_secs",
        cat: Msg::CatSession,
        label: Msg::LblSessIdleSecs,
        desc: Msg::DescSessIdleSecs,
        kind: SettingKind::Int { min: 0, max: 86400 },
        default: "1800",
    },
    Entry {
        key: "session.idle_shared",
        cat: Msg::CatSession,
        label: Msg::LblSessIdleShared,
        desc: Msg::DescSessIdleShared,
        kind: SettingKind::Bool,
        default: "off",
    },
    // ── 접속 생존(docs/53): 동작 직전 빠른 판정 기준 · TCP keepalive · Oracle 호출 상한.
    Entry {
        key: "probe.stale_secs",
        cat: Msg::CatConnection,
        label: Msg::LblProbeStale,
        desc: Msg::DescProbeStale,
        kind: SettingKind::Int { min: 0, max: 86400 },
        default: "60",
    },
    Entry {
        key: "net.keepalive_secs",
        cat: Msg::CatConnection,
        label: Msg::LblNetKeepalive,
        desc: Msg::DescNetKeepalive,
        kind: SettingKind::Int { min: 0, max: 7200 },
        default: "60",
    },
    Entry {
        key: "session.call_timeout_secs",
        cat: Msg::CatSession,
        label: Msg::LblCallTimeout,
        desc: Msg::DescCallTimeout,
        kind: SettingKind::Int { min: 0, max: 86400 },
        default: "0",
    },
    // ── 파일 검색(T-81a · docs/36 §2 · D-55)
    Entry {
        key: "search.max_file_kb",
        cat: Msg::CatFiles,
        label: Msg::LblSearchMaxFileKb,
        desc: Msg::DescSearchMaxFileKb,
        kind: SettingKind::Int {
            min: 0,
            max: 1_048_576,
        },
        default: "1024",
    },
    Entry {
        key: "search.threads",
        cat: Msg::CatFiles,
        label: Msg::LblSearchThreads,
        desc: Msg::DescSearchThreads,
        kind: SettingKind::Int { min: 0, max: 16 },
        default: "0",
    },
    Entry {
        key: "search.gitignore",
        cat: Msg::CatFiles,
        label: Msg::LblSearchGitignore,
        desc: Msg::DescSearchGitignore,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "search.excludes",
        cat: Msg::CatFiles,
        label: Msg::LblSearchExcludes,
        desc: Msg::DescSearchExcludes,
        kind: SettingKind::Text,
        default: "",
    },
    // ── 검색어 이력(사용자 09-23 "검색 상자마다 최대 20개(설정) · 프로그램 전역") — 찾기/바꾸기 · 파일 검색 · 필터 · 설정 검색 공통 상한.
    Entry {
        key: "search.history_max",
        cat: Msg::CatFiles,
        label: Msg::LblSearchHistoryMax,
        desc: Msg::DescSearchHistoryMax,
        kind: SettingKind::Int { min: 0, max: 500 },
        default: "20",
    },
    Entry {
        key: "search.history_view",
        cat: Msg::CatFiles,
        label: Msg::LblSearchHistoryView,
        desc: Msg::DescSearchHistoryView,
        kind: SettingKind::Choice(HISTORY_VIEW),
        default: "dropdown",
    },
    Entry {
        key: "search.history_rows",
        cat: Msg::CatFiles,
        label: Msg::LblSearchHistoryRows,
        desc: Msg::DescSearchHistoryRows,
        kind: SettingKind::Int { min: 1, max: 50 },
        default: "5",
    },
    // ── 트랜잭션 UX(DR-30 · T-77 · docs/34 §2-5)
    Entry {
        key: "tx.stale_min",
        cat: Msg::CatSession,
        label: Msg::LblTxStaleMin,
        desc: Msg::DescTxStaleMin,
        kind: SettingKind::Int { min: 1, max: 1440 },
        default: "10",
    },
    // 수동 커밋 잠금 방지(docs/56 · 사용자 09-19 "자동 처리의 값·조건은 전부 설정으로"): L1 읽기 트랜잭션 자동 종료 ·
    //   L2 유휴 미커밋 경고/재알림/자동 동작/카운트다운 · L3 막힘 감지 주기 · L4 서버 안전망 세션 파라미터.
    Entry {
        key: "tx.read_end",
        cat: Msg::CatSession,
        label: Msg::LblTxReadEnd,
        desc: Msg::DescTxReadEnd,
        kind: SettingKind::Choice(TX_READ_END_OPTS),
        default: "auto",
    },
    Entry {
        key: "tx.remind_min",
        cat: Msg::CatSession,
        label: Msg::LblTxRemind,
        desc: Msg::DescTxRemind,
        kind: SettingKind::Int { min: 0, max: 1440 },
        default: "10",
    },
    Entry {
        key: "tx.idle_action",
        cat: Msg::CatSession,
        label: Msg::LblTxIdleAction,
        desc: Msg::DescTxIdleAction,
        kind: SettingKind::Choice(TX_IDLE_ACTION_OPTS),
        default: "rollback",
    },
    Entry {
        key: "tx.idle_limit_min",
        cat: Msg::CatSession,
        label: Msg::LblTxIdleLimit,
        desc: Msg::DescTxIdleLimit,
        kind: SettingKind::Int { min: 1, max: 1440 },
        default: "30",
    },
    Entry {
        key: "tx.idle_countdown_secs",
        cat: Msg::CatSession,
        label: Msg::LblTxIdleCountdown,
        desc: Msg::DescTxIdleCountdown,
        kind: SettingKind::Int { min: 5, max: 600 },
        default: "60",
    },
    // ── 접속 유형(운영) 기준(docs/56 §4 2차) — 전역 값과 비교해 더 엄격한 쪽이 적용된다.
    Entry {
        key: "tx.prod_stale_min",
        cat: Msg::CatSession,
        label: Msg::LblTxProdStale,
        desc: Msg::DescTxProdStale,
        kind: SettingKind::Int { min: 1, max: 240 },
        default: "5",
    },
    Entry {
        key: "tx.prod_idle_limit_min",
        cat: Msg::CatSession,
        label: Msg::LblTxProdLimit,
        desc: Msg::DescTxProdLimit,
        kind: SettingKind::Int { min: 1, max: 600 },
        default: "10",
    },
    Entry {
        key: "run.prod_confirm",
        cat: Msg::CatSession,
        label: Msg::LblRunProdConfirm,
        desc: Msg::DescRunProdConfirm,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "tx.block_poll_secs",
        cat: Msg::CatSession,
        label: Msg::LblTxBlockPoll,
        desc: Msg::DescTxBlockPoll,
        kind: SettingKind::Int { min: 0, max: 3600 },
        default: "30",
    },
    Entry {
        key: "tx.server_idle_timeout_secs",
        cat: Msg::CatSession,
        label: Msg::LblTxServerIdle,
        desc: Msg::DescTxServerIdle,
        kind: SettingKind::Int { min: 0, max: 86400 },
        default: "0",
    },
    Entry {
        key: "tx.lock_wait_timeout_secs",
        cat: Msg::CatSession,
        label: Msg::LblTxLockWait,
        desc: Msg::DescTxLockWait,
        kind: SettingKind::Int { min: 0, max: 3600 },
        default: "0",
    },
    Entry {
        key: "editor.close_unsaved",
        cat: Msg::CatEditor,
        label: Msg::LblCloseUnsaved,
        desc: Msg::DescCloseUnsaved,
        kind: SettingKind::Choice(CLOSE_UNSAVED_OPTS),
        default: "ask",
    },
    Entry {
        key: "tx.close_action",
        cat: Msg::CatSession,
        label: Msg::LblTxCloseAction,
        desc: Msg::DescTxCloseAction,
        kind: SettingKind::Choice(TX_CLOSE_OPTS),
        default: "ask",
    },
    Entry {
        key: "tx.badge",
        cat: Msg::CatSession,
        label: Msg::LblTxBadge,
        desc: Msg::DescTxBadge,
        kind: SettingKind::Choice(TX_BADGE_OPTS),
        default: "count",
    },
    Entry {
        key: "tx.smart_commit",
        cat: Msg::CatSession,
        label: Msg::LblTxSmartCommit,
        desc: Msg::DescTxSmartCommit,
        kind: SettingKind::Bool,
        default: "off",
    },
    // D-139: OUT 바인드가 없는 DBMS의 `EXEC SELECT … INTO` — 0행·여러 행 = 오류(oracle) / 첫 행(first).
    Entry {
        key: "vars.into_policy",
        cat: Msg::CatSession,
        label: Msg::LblVarsIntoPolicy,
        desc: Msg::DescVarsIntoPolicy,
        kind: SettingKind::Choice(VARS_INTO_OPTS),
        default: "oracle",
    },
    // 부하원 스위치(39 §3 · 09-21): 호출 서명 조회(루틴당 카탈로그 질의 1회) · PG 커서 이름 풀기(결과마다 확인 왕복 1회).
    // 둘 다 **결과에 영향을 주므로** 향상 모드(`perf::BOOST`)에는 넣지 않는다 — 끄는 것은 사용자의 선택.
    Entry {
        key: "vars.signature_lookup",
        cat: Msg::CatSession,
        label: Msg::LblVarsSignatureLookup,
        desc: Msg::DescVarsSignatureLookup,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "pg.refcursor_expand",
        cat: Msg::CatDbmsPostgres,
        label: Msg::LblPgRefcursorExpand,
        desc: Msg::DescPgRefcursorExpand,
        kind: SettingKind::Bool,
        default: "on",
    },
    // T-153: `${이름:형식}` 치환 · 돌아온 값의 크기 상한(기본 1 MB — SQL*Plus VARCHAR2 32 KB · CLOB는 무제한 · DBeaver 상한 없음의 사이).
    Entry {
        key: "vars.brace_subst",
        cat: Msg::CatSession,
        label: Msg::LblVarsBraceSubst,
        desc: Msg::DescVarsBraceSubst,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "vars.env_subst",
        cat: Msg::CatSession,
        label: Msg::LblVarsEnvSubst,
        desc: Msg::DescVarsEnvSubst,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "vars.intrinsic",
        cat: Msg::CatSession,
        label: Msg::LblVarsIntrinsic,
        desc: Msg::DescVarsIntrinsic,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "vars.expand_at",
        cat: Msg::CatSession,
        label: Msg::LblVarsExpandAt,
        desc: Msg::DescVarsExpandAt,
        kind: SettingKind::Choice(VARS_EXPAND_OPTS),
        default: "assign",
    },
    Entry {
        key: "vars.max_value_kb",
        cat: Msg::CatSession,
        label: Msg::LblVarsMaxValueKb,
        desc: Msg::DescVarsMaxValueKb,
        kind: SettingKind::Int {
            min: 0,
            max: 1_048_576,
        },
        default: "1024",
    },
    // D-136: 파일별 변수 보존(`<설정 폴더>/vars/<경로 해시>.sql` — 실행 가능한 VAR/EXEC 스크립트 · 비밀·커서·여러 줄 제외).
    Entry {
        key: "vars.global_persist",
        cat: Msg::CatSession,
        label: Msg::LblVarsGlobalPersist,
        desc: Msg::DescVarsGlobalPersist,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "vars.persist",
        cat: Msg::CatSession,
        label: Msg::LblVarsPersist,
        desc: Msg::DescVarsPersist,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "vars.persist_days",
        cat: Msg::CatSession,
        label: Msg::LblVarsPersistDays,
        desc: Msg::DescVarsPersistDays,
        kind: SettingKind::Int { min: 0, max: 3650 },
        default: "90",
    },
    // D-137: 값이 없는 채로 읽히는 바인드·미정의 `&` = 실행당 한 번 묻기(prompt) / 말없이 NULL·빈 글(auto · 종전) / 오류(error).
    Entry {
        key: "vars.undeclared",
        cat: Msg::CatSession,
        label: Msg::LblVarsUndeclared,
        desc: Msg::DescVarsUndeclared,
        kind: SettingKind::Choice(VARS_UNDECLARED_OPTS),
        default: "prompt",
    },
    // REF CURSOR 자동 표시(09-21): `VAR rc REFCURSOR` + `EXEC proc(:rc)` 뒤 커서를 바로 결과 탭으로(둘 이상이면 각각) · 끄면 `PRINT rc`.
    Entry {
        key: "run.cursor_autoshow",
        cat: Msg::CatSession,
        label: Msg::LblCursorAutoshow,
        desc: Msg::DescCursorAutoshow,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ── 스크립트 엔진 엄격 모드(T-9 · 09-16) — 배치에서 미정의 &var·암묵 :bind를 오류로. 자주 안 바꾸므로 HIDDEN(`nsql config list all`).
    Entry {
        key: "script.strict",
        cat: Msg::CatSession,
        label: Msg::LblScriptStrict,
        desc: Msg::DescScriptStrict,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "settings.json_editor",
        cat: Msg::CatSession,
        label: Msg::LblJsonEditor,
        desc: Msg::DescJsonEditor,
        kind: SettingKind::Choice(JSON_EDITOR_OPTS),
        default: "external",
    },
    Entry {
        key: "session.mode",
        cat: Msg::CatSession,
        label: Msg::LblSessionMode,
        desc: Msg::DescSessionMode,
        kind: SettingKind::Choice(SESSION_MODE_OPTS),
        default: "shared",
    },
    // ★ 임시(사용자 09-19 · 특정 시점에 제거): 시작하면 Demo 프로필에 자동 접속하고 로그인 창을 띄우지 않는다(개발 편의 ·
    //   `window.monitor`와 함께 · HIDDEN · `nsql config set dev.start_demo on`).
    Entry {
        key: "dev.start_demo",
        cat: Msg::CatLog,
        label: Msg::LblDevStartDemo,
        desc: Msg::DescDevStartDemo,
        kind: SettingKind::Bool,
        default: "off",
    },
    // 데모(사용자 09-17): 최초 실행 1회 "샘플 데이터(Demo) 만들까요?" 팝업을 띄웠는가(자동 기억 · HIDDEN).
    Entry {
        key: "demo.prompted",
        cat: Msg::CatLog,
        label: Msg::LblDemoPrompted,
        desc: Msg::DescDemoPrompted,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "log.open_at_start",
        cat: Msg::CatLog,
        label: Msg::LblLogOpenAtStart,
        desc: Msg::DescLogOpenAtStart,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "log.format",
        cat: Msg::CatLog,
        label: Msg::LblLogFormat,
        desc: Msg::DescLogFormat,
        kind: SettingKind::Choice(LOG_FORMAT_OPTS),
        default: "raw",
    },
    // ── 로그 형식 어댑터 확장(사용자 09-16 · 4종 전부): 템플릿 · 파일 싱크 · 필터/컬럼.
    Entry {
        key: "log.template",
        cat: Msg::CatLog,
        label: Msg::LblLogTemplate,
        desc: Msg::DescLogTemplate,
        kind: SettingKind::Text,
        default: "{time} {kind:<8} {msg}",
    },
    Entry {
        key: "log.file",
        cat: Msg::CatLog,
        label: Msg::LblLogFile,
        desc: Msg::DescLogFile,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "log.file_format",
        cat: Msg::CatLog,
        label: Msg::LblLogFileFormat,
        desc: Msg::DescLogFileFormat,
        kind: SettingKind::Choice(LOG_FILE_FORMAT_OPTS),
        default: "same",
    },
    Entry {
        key: "log.file_max_kb",
        cat: Msg::CatLog,
        label: Msg::LblLogFileMaxKb,
        desc: Msg::DescLogFileMaxKb,
        kind: SettingKind::Int {
            min: 64,
            max: 1_048_576,
        },
        default: "5120",
    },
    Entry {
        key: "log.kinds",
        cat: Msg::CatLog,
        label: Msg::LblLogKinds,
        desc: Msg::DescLogKinds,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "log.columns",
        cat: Msg::CatLog,
        label: Msg::LblLogColumns,
        desc: Msg::DescLogColumns,
        kind: SettingKind::Text,
        default: "",
    },
    // ── 로그 창 스위치 3종(사용자 09-16) — 창 아래 스위치와 같은 값(자동 기억).
    Entry {
        key: "log.wrap",
        cat: Msg::CatLog,
        label: Msg::LblLogWrap,
        desc: Msg::DescLogWrap,
        kind: SettingKind::Bool,
        default: "off",
    },
    // 개발자 모드(사용자 09-17 · docs/48): 상세 로그는 이 두 키가 만드는 마스크가 켜져 있을 때만 **생성**된다.
    Entry {
        key: "log.dev_mode",
        cat: Msg::CatLog,
        label: Msg::LblLogDevMode,
        desc: Msg::DescLogDevMode,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "log.dev_layers",
        cat: Msg::CatLog,
        label: Msg::LblLogDevLayers,
        desc: Msg::DescLogDevLayers,
        kind: SettingKind::Text,
        default: "net,exec,fetch,load,render,ext",
    },
    Entry {
        key: "log.newest_first",
        cat: Msg::CatLog,
        label: Msg::LblLogNewestFirst,
        desc: Msg::DescLogNewestFirst,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "log.switch_scale",
        cat: Msg::CatLog,
        label: Msg::LblLogSwitchScale,
        desc: Msg::DescLogSwitchScale,
        kind: SettingKind::Int { min: 50, max: 150 },
        default: "80",
    },
    Entry {
        key: "log.always_on_top",
        cat: Msg::CatLog,
        label: Msg::LblLogOnTop,
        desc: Msg::DescLogOnTop,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "log.autoscroll",
        cat: Msg::CatLog,
        label: Msg::LblLogAutoscroll,
        desc: Msg::DescLogAutoscroll,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "editor.font_size",
        cat: Msg::CatEditor,
        label: Msg::LblEditorFontSize,
        desc: Msg::DescEditorFontSize,
        kind: SettingKind::Size { min: 8, max: 40 },
        default: "16",
    },
    // 편집기 고정폭 글꼴(사용자 09-17): 비면 OS별 기본 사슬(D2Coding → Sarasa → Nanum Gothic Coding → OS 고정폭 → 한글 UI 본) ·
    // 이름을 주면 먼저 시도하고 못 찾으면 같은 사슬로 fail-over(nexa-font `mono_font`).
    Entry {
        key: "editor.font_face",
        cat: Msg::CatEditor,
        label: Msg::LblEditorFontFace,
        desc: Msg::DescEditorFontFace,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "grid.font_face",
        cat: Msg::CatGrid,
        label: Msg::LblGridFontFace,
        desc: Msg::DescGridFontFace,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "grid.font_size",
        cat: Msg::CatGrid,
        label: Msg::LblGridFontSize,
        desc: Msg::DescGridFontSize,
        kind: SettingKind::Size { min: 8, max: 40 },
        default: "13",
    },
    // ── ★ 성능 거버너(docs/39 T-90a/T-90d · 09-16) — 마스터 `perf.mode` + 신설 부하원 키(모드별 값은 `perf::PERF`).
    Entry {
        key: "perf.mode",
        cat: Msg::CatPerformance,
        label: Msg::LblPerfMode,
        desc: Msg::DescPerfMode,
        kind: SettingKind::Choice(perf::PERF_MODE_OPTS),
        // 기본 full = 지금 동작 그대로(D-58) · 배터리/원격이면 상태줄 안내만.
        default: "full",
    },
    // ★ 실행 속도 향상(사용자 09-17): UI 구성·부가 표시·폴링·I/O 키(`perf::BOOST`)를 최적값으로 강제 + 설정 창 잠금 · 동작 영향 0.
    // macOS 화면 내보내기(T-147 · D-133 ②) — IOSurface 풀(할당·복사 0 · 색 맞춤 = 합성기) / 종전 softbuffer. 다른 OS는 무시.
    Entry {
        key: "gfx.linux_backend",
        cat: Msg::CatPerformance,
        label: Msg::LblLinuxBackend,
        desc: Msg::DescLinuxBackend,
        kind: SettingKind::Choice(LINUX_BACKEND_OPTS),
        // 기본 = X11(XWayland): winit 0.30의 Wayland 경로는 부모 창·창 활성화를 지원하지 않아 모달(접속·파일·비밀번호 창)이
        // 메인 뒤로 숨는다(사용자 09-22). Wayland 네이티브는 선택.
        default: "x11",
    },
    Entry {
        key: "gfx.mac_present",
        cat: Msg::CatPerformance,
        label: Msg::LblMacPresent,
        desc: Msg::DescMacPresent,
        kind: SettingKind::Choice(MAC_PRESENT_OPTS),
        // ★ 09-24 기본 = IOSurface(D-133 ② 전환 · 100차 실측: Debug(의존 최적화) 프레임 48 → 13.9 ms · present 36 → 3.1 ms). 만들기에
        //   실패하면 `present.rs`가 조용히 softbuffer로 돌아간다 · 빈 창이 보이면 `softbuffer`로 되돌린다(86차 Intel+AMD 사례는 87차에 수정).
        default: "iosurface",
    },
    Entry {
        key: "perf.boost",
        cat: Msg::CatPerformance,
        label: Msg::LblPerfBoost,
        desc: Msg::DescPerfBoost,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "ui.menu_icons",
        cat: Msg::CatAppearance,
        label: Msg::LblMenuIcons,
        desc: Msg::DescMenuIcons,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ui.clipboard_probe",
        cat: Msg::CatAppearance,
        label: Msg::LblClipboardProbe,
        desc: Msg::DescClipboardProbe,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "db.statement_timeout",
        cat: Msg::CatSession,
        label: Msg::LblStatementTimeout,
        desc: Msg::DescStatementTimeout,
        kind: SettingKind::Int {
            min: 0,
            max: 86_400,
        },
        // 0 = 없음(DBeaver 동일 · D-61).
        default: "0",
    },
    Entry {
        key: "log.max_lines",
        cat: Msg::CatLog,
        label: Msg::LblLogMaxLines,
        desc: Msg::DescLogMaxLines,
        kind: SettingKind::Int {
            min: 100,
            max: 1_000_000,
        },
        default: "10000",
    },
    Entry {
        key: "txlog.max_entries",
        cat: Msg::CatLog,
        label: Msg::LblTxLogMax,
        desc: Msg::DescTxLogMax,
        kind: SettingKind::Int {
            min: 16,
            max: 1_000_000,
        },
        default: "10000",
    },
    Entry {
        key: "editor.undo_budget_mb",
        cat: Msg::CatPerformance,
        label: Msg::LblUndoBudget,
        desc: Msg::DescUndoBudget,
        kind: SettingKind::Int { min: 1, max: 4096 },
        default: "64",
    },
    Entry {
        key: "editor.undo_group_ms",
        cat: Msg::CatPerformance,
        label: Msg::LblUndoGroupMs,
        desc: Msg::DescUndoGroupMs,
        kind: SettingKind::Int { min: 0, max: 60000 },
        default: "1500",
    },
    Entry {
        key: "editor.undo_persist",
        cat: Msg::CatEditor,
        label: Msg::LblUndoPersist,
        desc: Msg::DescUndoPersist,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "editor.undo_persist_mb",
        cat: Msg::CatPerformance,
        label: Msg::LblUndoPersistMb,
        desc: Msg::DescUndoPersistMb,
        kind: SettingKind::Int { min: 1, max: 64 },
        default: "4",
    },
    Entry {
        key: "editor.undo_persist_days",
        cat: Msg::CatPerformance,
        label: Msg::LblUndoPersistDays,
        desc: Msg::DescUndoPersistDays,
        kind: SettingKind::Int { min: 0, max: 3650 },
        default: "30",
    },
    Entry {
        key: "editor.undo_giant_mb",
        cat: Msg::CatPerformance,
        label: Msg::LblUndoGiantMb,
        desc: Msg::DescUndoGiantMb,
        kind: SettingKind::Int { min: 0, max: 4096 },
        default: "32",
    },
    // ── 메모리 회수(`memtrim.rs` · docs/26 §7-4).
    Entry {
        key: "mem.trim_on_release",
        cat: Msg::CatPerformance,
        label: Msg::LblMemTrimOnRelease,
        desc: Msg::DescMemTrimOnRelease,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "mem.release_results_on_disconnect",
        cat: Msg::CatPerformance,
        label: Msg::LblMemReleaseOnDisc,
        desc: Msg::DescMemReleaseOnDisc,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "mem.trim_secs",
        cat: Msg::CatPerformance,
        label: Msg::LblMemTrimSecs,
        desc: Msg::DescMemTrimSecs,
        kind: SettingKind::Int { min: 0, max: 86400 },
        default: "300",
    },
    // ★ 큰 선택 복사/잘라내기 확인(사용자 09-22 · docs/72 §2): 이보다 큰 선택은 3초 안에 다시 눌러야 복사/잘라내기(0 = 안 물음).
    Entry {
        key: "editor.copy_confirm_mb",
        cat: Msg::CatEditor,
        label: Msg::LblCopyConfirmMb,
        desc: Msg::DescCopyConfirmMb,
        kind: SettingKind::Int { min: 0, max: 4096 },
        default: "32",
    },
    // ★ 코드 완성(docs/47 §8-1 · docs/76 · 사용자 09-23): 트리거 · 일치 · 표시 · 삽입 — 값은 전부 여기(하드코딩 0).
    Entry {
        key: "intel.enabled",
        cat: Msg::CatIntel,
        label: Msg::LblIntelEnabled,
        desc: Msg::DescIntelEnabled,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.auto_activation",
        cat: Msg::CatIntel,
        label: Msg::LblIntelAutoActivation,
        desc: Msg::DescIntelAutoActivation,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.delay_ms",
        cat: Msg::CatIntel,
        label: Msg::LblIntelDelayMs,
        desc: Msg::DescIntelDelayMs,
        kind: SettingKind::Int { min: 0, max: 2000 },
        default: "250",
    },
    Entry {
        key: "intel.trigger_chars",
        cat: Msg::CatIntel,
        label: Msg::LblIntelTriggerChars,
        desc: Msg::DescIntelTriggerChars,
        kind: SettingKind::Text,
        default: ".",
    },
    Entry {
        key: "intel.min_chars",
        cat: Msg::CatIntel,
        label: Msg::LblIntelMinChars,
        desc: Msg::DescIntelMinChars,
        kind: SettingKind::Int { min: 1, max: 5 },
        default: "2",
    },
    Entry {
        key: "intel.match",
        cat: Msg::CatIntel,
        label: Msg::LblIntelMatch,
        desc: Msg::DescIntelMatch,
        kind: SettingKind::Choice(INTEL_MATCH),
        default: "fuzzy",
    },
    Entry {
        key: "intel.recent_boost",
        cat: Msg::CatIntel,
        label: Msg::LblIntelRecentBoost,
        desc: Msg::DescIntelRecentBoost,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.max_items",
        cat: Msg::CatIntel,
        label: Msg::LblIntelMaxItems,
        desc: Msg::DescIntelMaxItems,
        // 09-24: 200 = **페이지**(사용자) — 끝까지 스크롤하면 이만큼씩 이어 붙는다(76 §13 · T-196) · 한 세트 상한은 `intel.max_total`.
        kind: SettingKind::Int { min: 50, max: 5000 },
        default: "200",
    },
    // 한 세트로 드는 후보 상한(HIDDEN · 그 위는 잘림 — "더 좁혀 주세요").
    Entry {
        key: "intel.max_total",
        cat: Msg::CatIntel,
        label: Msg::LblIntelMaxTotal,
        desc: Msg::DescIntelMaxTotal,
        kind: SettingKind::Int {
            min: 500,
            max: 20_000,
        },
        default: "5000",
    },
    Entry {
        key: "intel.popup_rows",
        cat: Msg::CatIntel,
        label: Msg::LblIntelPopupRows,
        desc: Msg::DescIntelPopupRows,
        kind: SettingKind::Int { min: 6, max: 30 },
        default: "10",
    },
    // ★ 09-23(사용자 "긴 이름 · 가로 스크롤"): 팝업 폭 상한 — 넘치는 이름은 가운데 …(앞뒤 보존) + Shift+휠 가로 스크롤 · 강조 행은 상태줄에 전체.
    Entry {
        key: "intel.popup_max_width",
        cat: Msg::CatIntel,
        label: Msg::LblIntelPopupMaxWidth,
        desc: Msg::DescIntelPopupMaxWidth,
        kind: SettingKind::Int {
            min: 240,
            max: 2000,
        },
        default: "560",
    },
    // ★ 09-24(사용자 "팝업 상태에서 키 입력이 관통 · 기본 = 바로 입력 · 옵션 = 두 번"): 고른 항목이 없을 때 Enter/Tab.
    // ★ 09-24(사용자 "같은 높이의 추가 설명란 · 배경 80% 투명 · 글씨 50%"): 상세 카드(컬럼·테이블·함수 …).
    Entry {
        key: "intel.detail_card",
        cat: Msg::CatIntel,
        label: Msg::LblIntelDetailCard,
        desc: Msg::DescIntelDetailCard,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.detail_bg_alpha",
        cat: Msg::CatIntel,
        label: Msg::LblIntelDetailBgAlpha,
        desc: Msg::DescIntelDetailBgAlpha,
        kind: SettingKind::Int { min: 0, max: 100 },
        // 09-24: 20 → 0(사용자 "투명도 0으로 완전하게" · 값 = **투명도 %** · 0 = 불투명).
        default: "0",
    },
    Entry {
        key: "intel.detail_text_alpha",
        cat: Msg::CatIntel,
        label: Msg::LblIntelDetailTextAlpha,
        desc: Msg::DescIntelDetailTextAlpha,
        kind: SettingKind::Int { min: 0, max: 100 },
        default: "0",
    },
    // ★ 09-24(사용자 "JOIN에서 alias 없이 완성 → 전 테이블 컬럼 · 기본 A.컬럼 · Alt = 컬럼만").
    Entry {
        key: "intel.qualify_columns",
        cat: Msg::CatIntel,
        label: Msg::LblIntelQualifyColumns,
        desc: Msg::DescIntelQualifyColumns,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 09-24(사용자 "FROM 자리에 뷰·함수 등 테이블 역할 대상 — 뷰는 테이블과 같은 레이어 · 함수·프로시저는 낮은 레이어").
    Entry {
        key: "intel.from_routines",
        cat: Msg::CatIntel,
        label: Msg::LblIntelFromRoutines,
        desc: Msg::DescIntelFromRoutines,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.icons",
        cat: Msg::CatIntel,
        label: Msg::LblIntelIcons,
        desc: Msg::DescIntelIcons,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ `*` 조각의 구분(사용자 09-24): 한 줄 `A, B, C`(기본) / 여러 줄 `A` ↵ `, B` ↵ `, C`(들여쓰기 없음) · 쉼표 뒤 Space(기본)/Tab.
    Entry {
        key: "intel.star_layout",
        cat: Msg::CatIntel,
        label: Msg::LblIntelStarLayout,
        desc: Msg::DescIntelStarLayout,
        kind: SettingKind::Choice(INTEL_STAR_LAYOUT),
        default: "inline",
    },
    Entry {
        key: "intel.star_comma_space",
        cat: Msg::CatIntel,
        label: Msg::LblIntelStarSpace,
        desc: Msg::DescIntelStarSpace,
        kind: SettingKind::Choice(INTEL_STAR_SPACE),
        default: "space",
    },
    // ★ 창 방식 문턱(09-24 §187): 이 크기(KB)를 넘는 문서는 완성 문맥을 캐럿 앞뒤 256 KB 창에서만 구하고 문서 낱말·아웃라인 캐시를 끈다.
    Entry {
        key: "intel.max_doc_kb",
        cat: Msg::CatIntel,
        label: Msg::LblIntelMaxDocKb,
        desc: Msg::DescIntelMaxDocKb,
        kind: SettingKind::Int {
            min: 64,
            max: 65_536,
        },
        default: "1024",
    },
    Entry {
        key: "intel.card_settle_ms",
        cat: Msg::CatIntel,
        label: Msg::LblIntelCardSettle,
        desc: Msg::DescIntelCardSettle,
        kind: SettingKind::Int { min: 0, max: 2000 },
        default: "150",
    },
    // ★ 메모리 모니터(docs/80 · 사용자 09-24): 상태줄 총량 · 메모리 맵 창(모델리스 · 최상위) · 창이 닫혀 있으면 비용 0.
    Entry {
        key: "mem.statusbar",
        cat: Msg::CatPerformance,
        label: Msg::LblMemStatusbar,
        desc: Msg::DescMemStatusbar,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "mem.refresh_ms",
        cat: Msg::CatPerformance,
        label: Msg::LblMemRefresh,
        desc: Msg::DescMemRefresh,
        kind: SettingKind::Int {
            min: 250,
            max: 10_000,
        },
        default: "1000",
    },
    // 상태줄 숫자의 조회 간격(HIDDEN) — 그릴 때만 OS 한 번.
    Entry {
        key: "mem.status_refresh_ms",
        cat: Msg::CatPerformance,
        label: Msg::LblMemStatusRefresh,
        desc: Msg::DescMemStatusRefresh,
        kind: SettingKind::Int {
            min: 1000,
            max: 60_000,
        },
        default: "5000",
    },
    Entry {
        key: "mem.always_on_top",
        cat: Msg::CatPerformance,
        label: Msg::LblMemOnTop,
        desc: Msg::DescMemOnTop,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "intel.key_passthrough",
        cat: Msg::CatIntel,
        label: Msg::LblIntelKeyPassthrough,
        desc: Msg::DescIntelKeyPassthrough,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.keywords",
        cat: Msg::CatIntel,
        label: Msg::LblIntelKeywords,
        desc: Msg::DescIntelKeywords,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.document_words",
        cat: Msg::CatIntel,
        label: Msg::LblIntelDocumentWords,
        desc: Msg::DescIntelDocumentWords,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.insert_case",
        cat: Msg::CatIntel,
        label: Msg::LblIntelInsertCase,
        desc: Msg::DescIntelInsertCase,
        kind: SettingKind::Choice(INTEL_CASE),
        default: "default",
    },
    Entry {
        key: "intel.show_types",
        cat: Msg::CatIntel,
        label: Msg::LblIntelShowTypes,
        desc: Msg::DescIntelShowTypes,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ T-178(사용자 09-23 인텔리센스 재검토): 내장 함수·시스템 패키지·사전 객체(정적 표 nsql-script builtins) · 삽입 옵션 · 시그니처 도움 · 예산.
    Entry {
        key: "intel.functions",
        cat: Msg::CatIntel,
        label: Msg::LblIntelFunctions,
        desc: Msg::DescIntelFunctions,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 09-23(사용자 "기본 스키마는 접속 시 바로 메모리에 · 특정 `스키마.` 입력 시 그 시점에 캐싱"): 접속 직후 현재 스키마의
    //   테이블·뷰·시노님 + 권한 반영 사전 뷰를 메타 세션으로 한 번 읽는다(26 §8 · 39 §3). 끄면 처음 완성을 요청할 때 읽는다.
    Entry {
        key: "intel.preload",
        cat: Msg::CatIntel,
        label: Msg::LblIntelPreload,
        desc: Msg::DescIntelPreload,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.insert_parens",
        cat: Msg::CatIntel,
        label: Msg::LblIntelInsertParens,
        desc: Msg::DescIntelInsertParens,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.insert_alias",
        cat: Msg::CatIntel,
        label: Msg::LblIntelInsertAlias,
        desc: Msg::DescIntelInsertAlias,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "intel.insert_space",
        cat: Msg::CatIntel,
        label: Msg::LblIntelInsertSpace,
        desc: Msg::DescIntelInsertSpace,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "intel.insert_columns",
        cat: Msg::CatIntel,
        label: Msg::LblIntelInsertColumns,
        desc: Msg::DescIntelInsertColumns,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.signature_help",
        cat: Msg::CatIntel,
        label: Msg::LblIntelSignatureHelp,
        desc: Msg::DescIntelSignatureHelp,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "intel.budget_ms",
        cat: Msg::CatIntel,
        label: Msg::LblIntelBudgetMs,
        desc: Msg::DescIntelBudgetMs,
        kind: SettingKind::Int { min: 5, max: 500 },
        default: "30",
    },
    // ★ 북마크(docs/69 §9 · 독립 그룹 `Bookmarks` · 1차 = 동작·저장/표시/위치 추적 핵심 키 · 나머지는 B4b~).
    Entry {
        key: "bookmark.enabled",
        cat: Msg::CatBookmarkGeneral,
        label: Msg::LblBmEnabled,
        desc: Msg::DescBmEnabled,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bookmark.persist",
        cat: Msg::CatBookmarkGeneral,
        label: Msg::LblBmPersist,
        desc: Msg::DescBmPersist,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bookmark.stale_days",
        cat: Msg::CatBookmarkGeneral,
        label: Msg::LblBmStaleDays,
        desc: Msg::DescBmStaleDays,
        kind: SettingKind::Int { min: 0, max: 3650 },
        default: "30",
    },
    Entry {
        key: "bookmark.max_per_doc",
        cat: Msg::CatBookmarkGeneral,
        label: Msg::LblBmMaxPerDoc,
        desc: Msg::DescBmMaxPerDoc,
        kind: SettingKind::Int {
            min: 0,
            max: 100_000,
        },
        default: "500",
    },
    Entry {
        key: "bookmark.max_total",
        cat: Msg::CatBookmarkGeneral,
        label: Msg::LblBmMaxTotal,
        desc: Msg::DescBmMaxTotal,
        kind: SettingKind::Int {
            min: 0,
            max: 1_000_000,
        },
        default: "5000",
    },
    Entry {
        key: "bookmark.gutter",
        cat: Msg::CatBookmarkDisplay,
        label: Msg::LblBmGutter,
        desc: Msg::DescBmGutter,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bookmark.minimap",
        cat: Msg::CatBookmarkDisplay,
        label: Msg::LblBmMinimap,
        desc: Msg::DescBmMinimap,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bookmark.inline_label",
        cat: Msg::CatBookmarkDisplay,
        label: Msg::LblBmInline,
        desc: Msg::DescBmInline,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bookmark.inline_label_chars",
        cat: Msg::CatBookmarkDisplay,
        label: Msg::LblBmInlineChars,
        desc: Msg::DescBmInlineChars,
        kind: SettingKind::Int { min: 8, max: 200 },
        default: "40",
    },
    Entry {
        key: "bookmark.statusbar",
        cat: Msg::CatBookmarkDisplay,
        label: Msg::LblBmStatusbar,
        desc: Msg::DescBmStatusbar,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bookmark.anchor_context",
        cat: Msg::CatBookmarkAnchor,
        label: Msg::LblBmAnchorContext,
        desc: Msg::DescBmAnchorContext,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bookmark.anchor_chars",
        cat: Msg::CatBookmarkAnchor,
        label: Msg::LblBmAnchorChars,
        desc: Msg::DescBmAnchorChars,
        kind: SettingKind::Int { min: 8, max: 4000 },
        default: "200",
    },
    Entry {
        key: "bookmark.anchor_trim",
        cat: Msg::CatBookmarkAnchor,
        label: Msg::LblBmAnchorTrim,
        desc: Msg::DescBmAnchorTrim,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bookmark.search_lines",
        cat: Msg::CatBookmarkAnchor,
        label: Msg::LblBmSearchLines,
        desc: Msg::DescBmSearchLines,
        kind: SettingKind::Int {
            min: 0,
            max: 100_000,
        },
        default: "400",
    },
    Entry {
        key: "bookmark.similarity",
        cat: Msg::CatBookmarkAnchor,
        label: Msg::LblBmSimilarity,
        desc: Msg::DescBmSimilarity,
        kind: SettingKind::Text,
        default: "0.6",
    },
    Entry {
        key: "bookmark.relocate_unique",
        cat: Msg::CatBookmarkAnchor,
        label: Msg::LblBmRelocateUnique,
        desc: Msg::DescBmRelocateUnique,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bookmark.relocate_max_lines",
        cat: Msg::CatBookmarkAnchor,
        label: Msg::LblBmRelocateMaxLines,
        desc: Msg::DescBmRelocateMaxLines,
        kind: SettingKind::Int {
            min: 0,
            max: 10_000_000,
        },
        default: "200000",
    },
    Entry {
        key: "key.bookmark.toggle",
        cat: Msg::CatKeys,
        label: Msg::MnBmToggle,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.next",
        cat: Msg::CatKeys,
        label: Msg::MnBmNext,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.prev",
        cat: Msg::CatKeys,
        label: Msg::MnBmPrev,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.clear_doc",
        cat: Msg::CatKeys,
        label: Msg::MnBmClearDoc,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.select_all",
        cat: Msg::CatKeys,
        label: Msg::MnBmSelectAll,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.label",
        cat: Msg::CatKeys,
        label: Msg::MnBmLabel,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.view.bookmarks",
        cat: Msg::CatKeys,
        label: Msg::MnBookmarksPanel,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.set_0",
        cat: Msg::CatKeys,
        label: Msg::MnBmSetMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.set_1",
        cat: Msg::CatKeys,
        label: Msg::MnBmSetMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.set_2",
        cat: Msg::CatKeys,
        label: Msg::MnBmSetMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.set_3",
        cat: Msg::CatKeys,
        label: Msg::MnBmSetMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.set_4",
        cat: Msg::CatKeys,
        label: Msg::MnBmSetMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.set_5",
        cat: Msg::CatKeys,
        label: Msg::MnBmSetMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.set_6",
        cat: Msg::CatKeys,
        label: Msg::MnBmSetMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.set_7",
        cat: Msg::CatKeys,
        label: Msg::MnBmSetMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.set_8",
        cat: Msg::CatKeys,
        label: Msg::MnBmSetMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.set_9",
        cat: Msg::CatKeys,
        label: Msg::MnBmSetMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.goto_0",
        cat: Msg::CatKeys,
        label: Msg::MnBmGotoMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.goto_1",
        cat: Msg::CatKeys,
        label: Msg::MnBmGotoMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.goto_2",
        cat: Msg::CatKeys,
        label: Msg::MnBmGotoMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.goto_3",
        cat: Msg::CatKeys,
        label: Msg::MnBmGotoMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.goto_4",
        cat: Msg::CatKeys,
        label: Msg::MnBmGotoMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.goto_5",
        cat: Msg::CatKeys,
        label: Msg::MnBmGotoMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.goto_6",
        cat: Msg::CatKeys,
        label: Msg::MnBmGotoMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.goto_7",
        cat: Msg::CatKeys,
        label: Msg::MnBmGotoMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.goto_8",
        cat: Msg::CatKeys,
        label: Msg::MnBmGotoMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "key.bookmark.goto_9",
        cat: Msg::CatKeys,
        label: Msg::MnBmGotoMnemonic,
        desc: Msg::DescKey,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.undo_max",
        cat: Msg::CatEditor,
        label: Msg::LblUndoMax,
        desc: Msg::DescUndoMax,
        kind: SettingKind::Int {
            min: 10,
            max: 100_000,
        },
        default: "1000",
    },
    Entry {
        key: "ui.glyph_cache",
        cat: Msg::CatAppearance,
        label: Msg::LblGlyphCache,
        desc: Msg::DescGlyphCache,
        kind: SettingKind::Int {
            min: 256,
            max: 1_000_000,
        },
        default: "8192",
    },
    Entry {
        key: "file.icon_cache",
        cat: Msg::CatFiles,
        label: Msg::LblIconCache,
        desc: Msg::DescIconCache,
        kind: SettingKind::Int {
            min: 16,
            max: 100_000,
        },
        default: "512",
    },
    Entry {
        key: "editor.max_occurrences",
        cat: Msg::CatEditor,
        label: Msg::LblMaxOccurrences,
        desc: Msg::DescMaxOccurrences,
        kind: SettingKind::Int {
            min: 100,
            max: 10_000_000,
        },
        default: "10000",
    },
    Entry {
        key: "ui.max_fps",
        cat: Msg::CatAppearance,
        label: Msg::LblMaxFps,
        desc: Msg::DescMaxFps,
        kind: SettingKind::Int { min: 5, max: 240 },
        default: "60",
    },
    Entry {
        key: "ui.animations",
        cat: Msg::CatAppearance,
        label: Msg::LblAnimations,
        desc: Msg::DescAnimations,
        kind: SettingKind::Choice(ANIM_OPTS),
        default: "auto",
    },
    Entry {
        key: "editor.caret_blink",
        cat: Msg::CatEditor,
        label: Msg::LblCaretBlink,
        desc: Msg::DescCaretBlink,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "file.os_icons",
        cat: Msg::CatFiles,
        label: Msg::LblOsIcons,
        desc: Msg::DescOsIcons,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "file.probe_chevrons",
        cat: Msg::CatFiles,
        label: Msg::LblProbeChevrons,
        desc: Msg::DescProbeChevrons,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "probe.dns_cache_secs",
        cat: Msg::CatConnection,
        label: Msg::LblDnsCacheSecs,
        desc: Msg::DescDnsCacheSecs,
        kind: SettingKind::Int {
            min: 0,
            max: 86_400,
        },
        default: "0",
    },
    Entry {
        key: "probe.icmp",
        cat: Msg::CatConnection,
        label: Msg::LblProbeIcmp,
        desc: Msg::DescProbeIcmp,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ── T-48a/T-48d 페치 모델(docs/43 §3·§5 · 09-17): 왕복당 행수 · 추가 페치 방식 · 커서 유휴 상한 · 셸 상한/자동 이어 보기.
    Entry {
        key: "db.fetch_size",
        cat: Msg::CatSession,
        label: Msg::LblFetchSize,
        desc: Msg::DescFetchSize,
        kind: SettingKind::Int {
            min: 1,
            max: 100_000,
        },
        default: "200",
    },
    Entry {
        key: "db.fetch_all_size",
        cat: Msg::CatSession,
        label: Msg::LblFetchAllSize,
        desc: Msg::DescFetchAllSize,
        kind: SettingKind::Int {
            min: 100,
            max: 100_000,
        },
        default: "5000",
    },
    Entry {
        key: "grid.fetch_mode",
        cat: Msg::CatGrid,
        label: Msg::LblFetchMode,
        desc: Msg::DescFetchMode,
        kind: SettingKind::Choice(FETCH_MODE_OPTS),
        default: "cursor",
    },
    Entry {
        key: "db.cursor_idle_secs",
        cat: Msg::CatSession,
        label: Msg::LblCursorIdle,
        desc: Msg::DescCursorIdle,
        kind: SettingKind::Int {
            min: 0,
            max: 86_400,
        },
        default: "0",
    },
    Entry {
        key: "cli.max_rows",
        cat: Msg::CatCli,
        label: Msg::LblCliMaxRows,
        desc: Msg::DescCliMaxRows,
        kind: SettingKind::Int {
            min: 0,
            max: 10_000_000,
        },
        default: "200",
    },
    Entry {
        key: "cli.auto_more",
        cat: Msg::CatCli,
        label: Msg::LblCliAutoMore,
        desc: Msg::DescCliAutoMore,
        kind: SettingKind::Bool,
        default: "off",
    },
];

/// 키로 항목 찾기.
#[must_use]
pub fn entry(key: &str) -> Option<&'static Entry> {
    REGISTRY.iter().find(|e| e.key == key)
}

/// ★ 설정 트리(DBeaver Preferences 차용 · 사용자 09-15) — 그룹 → 카테고리 순서. 설정 화면(T-39) 사이드바·`config list` 머리글의 단일 원천.
/// DBeaver: General / User Interface(Appearance·Navigator·Keys) / Editors(SQL Editor) / Connections / Data Editor(Result Sets).
pub const CATEGORY_TREE: &[(Msg, &[Msg])] = &[
    (
        Msg::GrpGeneral,
        &[Msg::CatLog, Msg::CatSession, Msg::CatPerformance],
    ),
    (
        Msg::GrpUserInterface,
        &[
            Msg::CatAppearance,
            Msg::CatInput,
            Msg::CatKeys,
            Msg::CatWindow,
            Msg::CatExplorer,
        ],
    ),
    (
        Msg::GrpEditors,
        &[
            Msg::CatEditor,
            Msg::CatIntel,
            Msg::CatFiles,
            Msg::CatProject,
        ],
    ),
    (Msg::GrpConnections, &[Msg::CatConnection, Msg::CatCli]),
    (Msg::GrpDataEditor, &[Msg::CatGrid]),
    // DBMS별 종속 설정(사용자 09-21): 클라이언트 자동 탐지/직접 지정 + 읽기 전용 파생 정보 · 그 DBMS에만 뜻이 있는 키.
    (
        Msg::GrpDbms,
        &[
            Msg::CatDbmsOracle,
            Msg::CatDbmsMssql,
            Msg::CatDbmsPostgres,
            Msg::CatDbmsSqlite,
        ],
    ),
    // 확장(사용자 09-17 "Extensions 설정은 별도 그룹 밑에"): 관리자 + 확장별 분류(확장 하나 = 분류 하나).
    (
        Msg::GrpBookmarks,
        &[
            Msg::CatBookmarkGeneral,
            Msg::CatBookmarkDisplay,
            Msg::CatBookmarkAnchor,
        ],
    ),
    (
        Msg::GrpExtensions,
        &[Msg::CatExtManager, Msg::CatExtRainbowPairs],
    ),
];

/// ★ OS별 기본값(사용자 09-17 "OS별 차이를 잘 분석해 기능·UI/UX를 관리"): (키, macOS, Linux). Windows = 레지스트리 `default`.
/// 텍스트 래스터 5키는 Windows GDI ClearType을 흉내내려 넣은 것(42~44차)이라 macOS에서는 **전부 끈다** — Apple 텍스트는
/// 힌팅·줄기 스냅 없이 부드러운 AA(1x 모니터에서 특히 차이 · 사용자 09-17 "왼쪽/오른쪽 모니터 차이"). Linux는 FreeType 관례(힌트)라 Windows 값.
pub const OS_DEFAULTS: &[(&str, &str, &str)] = &[
    ("ui.text_hint", "off", "on"),
    ("ui.text_snap", "off", "on"),
    ("ui.text_weight", "0", "25"),
    ("ui.text_contrast", "100", "140"),
    // macOS = CoreText 글리프(T-100 · 파인더와 같은 픽셀) · Linux = 내장 래스터.
    ("ui.text_gdi", "on", "off"),
];

/// 이 OS의 기본값 — [`OS_DEFAULTS`]에 있으면 그것 · 아니면 레지스트리 `default`.
#[must_use]
pub fn default_of(key: &str) -> Option<&'static str> {
    let e = entry(key)?;
    let os = OS_DEFAULTS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, mac, linux)| {
            if cfg!(target_os = "macos") {
                *mac
            } else if cfg!(target_os = "windows") {
                e.default
            } else {
                *linux
            }
        });
    Some(os.unwrap_or(e.default))
}

/// 확장이 소유한 설정 분류 ↔ 확장 id(사용자 09-17 "설치되면 보이고 끄거나 제거하면 사라진다") — 설정 창이 끈/미설치
/// 확장의 분류를 숨긴다. 새 확장은 여기 한 줄 + `CATEGORY_TREE`의 Extensions 그룹에 분류 하나.
pub const EXTENSION_CATEGORIES: &[(Msg, &str)] = &[(Msg::CatExtRainbowPairs, "rainbow-pairs")];

/// 카테고리의 트리 순서(그룹 index, 카테고리 index) — 없으면 맨 뒤.
#[must_use]
pub fn tree_order(cat: Msg) -> (usize, usize) {
    for (gi, (_, cats)) in CATEGORY_TREE.iter().enumerate() {
        if let Some(ci) = cats.iter().position(|c| *c == cat) {
            return (gi, ci);
        }
    }
    (usize::MAX, usize::MAX)
}

/// 카테고리가 속한 그룹(없으면 None).
#[must_use]
pub fn group_of(cat: Msg) -> Option<Msg> {
    CATEGORY_TREE
        .iter()
        .find(|(_, cats)| cats.contains(&cat))
        .map(|(g, _)| *g)
}

/// 비노출 설정(자주 바꾸지 않는 구현 값 · 사용자 09-14) — 레지스트리에는 있어 `set/get/reset`은 되지만 목록·설정 화면엔 기본 숨김.
/// 종속 조건(설정 화면 잠금용 · 사용자 09-16 "종속 메뉴는 수정이 불가능하도록").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dep {
    /// 부모가 `on`.
    On,
    /// 부모가 비어 있지 않음.
    NotEmpty,
    /// 부모가 이 값.
    Eq(&'static str),
}

impl Dep {
    /// 부모 값이 조건을 만족하는가.
    #[must_use]
    pub fn satisfied(self, parent_value: &str) -> bool {
        match self {
            Dep::On => parent_value == "on",
            Dep::NotEmpty => !parent_value.trim().is_empty(),
            Dep::Eq(v) => parent_value == v,
        }
    }
}

/// (자식, 부모, 조건) — 부모가 조건을 만족하지 않으면 자식은 설정 화면에서 잠긴다(값은 유지 · CLI `config set`은 그대로).
/// Oracle 클라이언트를 정하는 방식.
const ORA_CLIENT_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValOraClientAuto),
    ("manual", Msg::ValOraClientManual),
];

/// ★ **읽기 전용 정보 키**(사용자 09-21): 저장하지 않는 계산 값 — 호스트가 그때그때 채워 설정 창에 잠긴 칸으로 보여 준다
/// (자동 탐지 결과 · 설정에서 파생된 파일 경로 · 내장 드라이버 안내). `set`은 거부한다 · 설정 파일에 적히지 않는다.
pub const INFO_KEYS: &[&str] = &[
    "oracle.info_library",
    "oracle.info_version",
    "oracle.info_tnsnames",
    "oracle.info_sqlnet",
    "mssql.info_driver",
    "pg.info_driver",
    "sqlite.info_driver",
];

/// 읽기 전용 정보 키인가.
#[must_use]
pub fn is_info(key: &str) -> bool {
    INFO_KEYS.contains(&key)
}

/// 결과 탭 이름 규칙.
const RESULT_TITLE_OPTS: &[(&str, Msg)] = &[
    ("number", Msg::ValResultTitleNumber),
    ("table", Msg::ValResultTitleTable),
];

/// 큰 파일 모드가 어떤 기능을 끄기 시작하는 단계.
const LARGE_LEVEL_OPTS: &[(&str, Msg)] = &[
    ("off", Msg::ValLargeLevelOff),
    ("l1", Msg::ValLargeLevelL1),
    ("l2", Msg::ValLargeLevelL2),
];

pub const DEPENDS: &[(&str, &str, Dep)] = &[
    ("meta.refresh_on_commit", "meta.refresh_on_ddl", Dep::On),
    (
        "file.external_merge",
        "file.external_change",
        Dep::Eq("auto"),
    ),
    ("explorer.typeahead_timeout", "explorer.typeahead", Dep::On),
    ("explorer.typeahead_space", "explorer.typeahead", Dep::On),
    ("explorer.typeahead_special", "explorer.typeahead", Dep::On),
    ("explorer.typeahead_pos", "explorer.typeahead", Dep::On),
    ("run.toast_hide_secs", "run.toast", Dep::On),
    ("run.toast_tick_ms", "run.toast", Dep::On),
    ("run.toast_follow", "run.toast", Dep::On),
    ("run.toast_max", "run.toast", Dep::On),
    // 결과가 1개일 때도 탭 줄을 둘지 — 결과 탭(다중)을 쓸 때만 뜻이 있다(`ResultPanel::bar_visible` = enabled && …).
    ("grid.result_tabbar_single", "grid.result_tabs", Dep::On),
    ("vars.env_subst", "vars.brace_subst", Dep::On),
    // Oracle 클라이언트: 직접 지정일 때만 폴더·TNS_ADMIN을 바꿀 수 있다(자동이면 탐지 결과가 읽기 전용으로 보인다).
    ("oracle.client_dir", "oracle.client_mode", Dep::Eq("manual")),
    ("oracle.tns_admin", "oracle.client_mode", Dep::Eq("manual")),
    ("vars.persist_days", "vars.persist", Dep::On),
    ("log.dev_layers", "log.dev_mode", Dep::On),
    ("editor.smart_indent", "editor.auto_indent", Dep::On),
    ("editor.indent_rules", "editor.smart_indent", Dep::On),
    ("editor.indent_to_bracket", "editor.auto_indent", Dep::On),
    ("editor.trim_auto_whitespace", "editor.auto_indent", Dep::On),
    ("editor.minimap_width", "editor.minimap", Dep::On),
    ("editor.minimap_box_color", "editor.minimap", Dep::On),
    ("grid.row_focus_color", "grid.row_focus", Dep::On),
    ("bookmark.persist", "bookmark.enabled", Dep::On),
    ("bookmark.stale_days", "bookmark.enabled", Dep::On),
    ("bookmark.max_per_doc", "bookmark.enabled", Dep::On),
    ("bookmark.max_total", "bookmark.enabled", Dep::On),
    ("bookmark.gutter", "bookmark.enabled", Dep::On),
    ("bookmark.minimap", "bookmark.enabled", Dep::On),
    ("bookmark.inline_label", "bookmark.enabled", Dep::On),
    (
        "bookmark.inline_label_chars",
        "bookmark.inline_label",
        Dep::On,
    ),
    ("bookmark.statusbar", "bookmark.enabled", Dep::On),
    ("bookmark.anchor_context", "bookmark.enabled", Dep::On),
    ("bookmark.anchor_chars", "bookmark.enabled", Dep::On),
    ("bookmark.anchor_trim", "bookmark.enabled", Dep::On),
    ("bookmark.search_lines", "bookmark.enabled", Dep::On),
    ("bookmark.similarity", "bookmark.enabled", Dep::On),
    ("bookmark.relocate_unique", "bookmark.enabled", Dep::On),
    ("bookmark.relocate_max_lines", "bookmark.enabled", Dep::On),
    ("editor.minimap_border", "editor.minimap", Dep::On),
    ("editor.minimap_viewport", "editor.minimap", Dep::On),
    ("editor.minimap_click", "editor.minimap", Dep::On),
    ("editor.minimap_find", "editor.minimap", Dep::On),
    ("editor.minimap_errors", "editor.minimap", Dep::On),
    ("statusbar.git_secs", "statusbar.git", Dep::On),
    ("editor.rulers", "editor.rulers_show", Dep::On),
    ("editor.ruler_color", "editor.rulers_show", Dep::On),
    ("editor.ruler_alpha", "editor.rulers_show", Dep::On),
    ("probe.max_retries", "probe.enabled", Dep::On),
    ("probe.timeout", "probe.enabled", Dep::On),
    ("probe.interval", "probe.enabled", Dep::On),
    ("probe.retry_delay", "probe.enabled", Dep::On),
    ("probe.max_inflight", "probe.enabled", Dep::On),
    ("log.file_format", "log.file", Dep::NotEmpty),
    ("log.file_max_kb", "log.file", Dep::NotEmpty),
    ("log.template", "log.format", Dep::Eq("template")),
    (
        "oracle.live.interval_ms",
        "oracle.live.source",
        Dep::Eq("table"),
    ),
    ("oracle.live.table", "oracle.live.source", Dep::Eq("table")),
    ("oracle.live.ts_col", "oracle.live.source", Dep::Eq("table")),
    (
        "oracle.live.text_col",
        "oracle.live.source",
        Dep::Eq("table"),
    ),
    ("ui.toast_alpha", "ui.toast_secs", Dep::NotEmpty),
    ("ui.toast_fade_to", "ui.toast_progress", Dep::On),
    ("ui.toast_bar_spent", "ui.toast_progress", Dep::On),
    ("grid.col_max_chars", "grid.col_max_mode", Dep::Eq("manual")),
    ("intel.detail_bg_alpha", "intel.detail_card", Dep::On),
    ("intel.detail_text_alpha", "intel.detail_card", Dep::On),
    ("intel.card_settle_ms", "intel.detail_card", Dep::On),
    ("intel.star_layout", "intel.insert_columns", Dep::On),
    ("intel.star_comma_space", "intel.insert_columns", Dep::On),
];

/// 자식 키의 (부모, 조건).
#[must_use]
pub fn dependency(child: &str) -> Option<(&'static str, Dep)> {
    DEPENDS
        .iter()
        .find(|(c, _, _)| *c == child)
        .map(|(_, p, d)| (*p, *d))
}

pub const HIDDEN: &[&str] = &[
    "ui.toast_fade_to",
    "ui.toast_bar_spent",
    "dev.start_demo",
    "window.main_size",
    "window.login_size",
    "window.log_size",
    "window.txlog_size",
    "window.sessions_size",
    "window.sessions_pos",
    "window.prefs_size",
    "window.main_pos",
    "window.login_pos",
    "window.log_pos",
    "window.txlog_pos",
    "window.prefs_pos",
    "rainbowpair.max_kb",
    "meta.refresh_idle_secs",
    "file.external_merge_max_kb",
    "file.external_settle_ms",
    "file.external_backup_keep",
    "file.overwrite_confirm_ms",
    "file.async_load_mb",
    "file.load_progress_ms",
    "editor.undo_group_ms",
    "editor.undo_giant_mb",
    "editor.undo_persist_mb",
    "editor.undo_persist_days",
    "meta.refresh_highlight_ms",
    "demo.prompted",
    "log.kinds",
    "statusbar.git_secs",
    "log.switch_scale",
    "log.columns",
    "conn.delete_confirm_ms",
    "conn.close_after_connect_ms",
    "conn.window_w",
    "conn.window_h",
    "conn.panel_w",
    "conn.port_w",
    "conn.button_scale_pct",
    "ui.tooltip_delay_ms",
    "explorer.width",
    "layout.editor_split_pct",
    "ui.dblclick_ms",
    "ui.slide_ms",
    "ui.hover_intent_ms",
    "ui.fade_out_ms",
    "probe.max_inflight",
    "ui.color_recent",
    "file.last_dir",
    "file.recent",
    "project.last",
    "project.recent",
    "file.show_hidden",
    "file.show_dot",
    "script.strict",
    // ── 성능 거버너 비노출(docs/39 §3 "HIDDEN") — `nsql config list perf`에는 나온다.
    "editor.undo_max",
    "ui.glyph_cache",
    "file.icon_cache",
    "editor.max_occurrences",
    "probe.dns_cache_secs",
    "probe.icmp",
    "db.fetch_size",
    "db.cursor_idle_secs",
];

/// 비노출 설정인가.
#[must_use]
pub fn is_hidden(key: &str) -> bool {
    HIDDEN.contains(&key)
}

/// 허용 값 설명(오류 메시지·`config list`용).
#[must_use]
pub fn allowed(kind: SettingKind) -> String {
    match kind {
        SettingKind::Choice(opts) => opts.iter().map(|(v, _)| *v).collect::<Vec<_>>().join(" | "),
        SettingKind::Lang => Lang::ALL
            .iter()
            .map(|l| l.code())
            .collect::<Vec<_>>()
            .join(" | "),
        SettingKind::Int { min, max } => format!("{min}..{max}"),
        SettingKind::Size { min, max } => format!("{min}..{max}px | Npt"),
        SettingKind::Bool => "on | off".into(),
        SettingKind::Text => "text".into(),
        SettingKind::Position => POSITIONS.join(" | "),
    }
}

/// 값 검증 → 정규화된 저장 표현. 실패 = `None`.
#[must_use]
pub fn normalize(kind: SettingKind, raw: &str) -> Option<String> {
    let v = raw.trim();
    match kind {
        SettingKind::Choice(opts) => {
            let lower = v.to_ascii_lowercase();
            opts.iter()
                .find(|(o, _)| *o == lower)
                .map(|(o, _)| (*o).to_string())
        }
        SettingKind::Lang => Lang::from_code(v).map(|l| l.code().to_string()),
        SettingKind::Int { min, max } => {
            let n: i64 = v.parse().ok()?;
            (min..=max).contains(&n).then(|| n.to_string())
        }
        SettingKind::Size { min, max } => {
            let (n, unit) = parse_size(v)?;
            let px = if unit == "pt" { n * 96.0 / 72.0 } else { n };
            if !(min as f32..=max as f32).contains(&px) {
                return None;
            }
            let num = if (n.fract()).abs() < 1e-6 {
                format!("{}", n as i64)
            } else {
                format!("{n}")
            };
            Some(if unit == "pt" {
                format!("{num}pt")
            } else {
                num
            })
        }
        SettingKind::Bool => match v.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" | "yes" => Some("on".into()),
            "off" | "false" | "0" | "no" => Some("off".into()),
            _ => None,
        },
        SettingKind::Text => Some(v.to_string()),
        SettingKind::Position => {
            let lower = v.to_ascii_lowercase();
            POSITIONS
                .iter()
                .find(|p| **p == lower)
                .map(|p| (*p).to_string())
        }
    }
}

/// `13` · `13px` · `10pt` → (수치, 단위 `px`|`pt`). 모르는 형식 = None.
#[must_use]
pub fn parse_size(v: &str) -> Option<(f32, &'static str)> {
    let t = v.trim().to_ascii_lowercase();
    let (num, unit) = if let Some(n) = t.strip_suffix("pt") {
        (n.trim(), "pt")
    } else if let Some(n) = t.strip_suffix("px") {
        (n.trim(), "px")
    } else {
        (t.as_str(), "px")
    };
    let n: f32 = num.parse().ok()?;
    (n.is_finite() && n >= 0.0).then_some((n, unit))
}

/// 글꼴 크기 값 → px(`10pt` = 13.33px). 형식이 틀리면 None.
#[must_use]
pub fn size_px(v: &str) -> Option<f32> {
    let (n, unit) = parse_size(v)?;
    Some(if unit == "pt" { n * 96.0 / 72.0 } else { n })
}

// ────────────────────────────────────────────────────────────── 설정 값

/// `set` 실패 사유.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetError {
    UnknownKey(String),
    /// (키, 입력값, 허용 값 설명)
    InvalidValue(String, String, String),
}

impl std::fmt::Display for SetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetError::UnknownKey(k) => write!(f, "{}", nsql_i18n::tf(Msg::CfgUnknownKey, &[k])),
            SetError::InvalidValue(k, v, a) => {
                write!(f, "{}", nsql_i18n::tf(Msg::CfgInvalidValue, &[k, v, a]))
            }
        }
    }
}

impl std::error::Error for SetError {}

/// 설정 값 집합 + 파일. **파일에는 기본값과 다른 값만** 쓴다.
#[derive(Debug)]
pub struct Settings {
    path: PathBuf,
    /// 사용자가 바꾼 값(정규화 완료 · 기본값과 다른 것만).
    values: BTreeMap<String, String>,
    /// 레지스트리에 없는 키(보존 재방출).
    unknown: Vec<(String, String)>,
}

impl Settings {
    /// 그리드 컬럼 최대 폭(글자 수) — 자동이면 [`COL_MAX_AUTO_CHARS`] · 직접이면 `grid.col_max_chars`.
    #[must_use]
    pub fn grid_col_max_chars(&self) -> i64 {
        if self.get("grid.col_max_mode") == Some("manual") {
            self.int("grid.col_max_chars").max(1)
        } else {
            COL_MAX_AUTO_CHARS
        }
    }

    /// 기본 폴더의 `settings.conf`. 폴더를 알 수 없으면 오류(파일이 없는 것은 오류가 아니다 — 전부 기본값).
    pub fn open_default() -> io::Result<Settings> {
        let dir =
            config_dir().ok_or_else(|| io::Error::other(nsql_i18n::t(Msg::CfgNoConfigDir)))?;
        Ok(Self::open(dir.join(FILE_NAME)))
    }

    /// 지정 파일. 없거나 손상이어도 실패하지 않는다(손상 값은 기본값으로 · 파일은 건드리지 않음).
    #[must_use]
    pub fn open(path: PathBuf) -> Settings {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        Self::from_text(path, &text)
    }

    /// 파일 없이 본문으로(시험 · 격리 인스턴스 — 저장은 `path`로).
    #[must_use]
    pub fn from_text(path: PathBuf, text: &str) -> Settings {
        let doc = nexa_conf::parse(text);
        let mut s = Settings {
            path,
            values: BTreeMap::new(),
            unknown: Vec::new(),
        };
        for (k, v) in doc.pairs {
            match entry(&k) {
                Some(e) => {
                    if let Some(n) = normalize(e.kind, &v) {
                        // 옛 기본값 그대로 저장돼 있던 값은 "기본값 유지"로 본다(기본값이 바뀌면 따라간다).
                        let old_default =
                            OLD_DEFAULTS.iter().any(|(key, old)| *key == k && *old == n);
                        let def = default_of(e.key).unwrap_or(e.default);
                        if n != def && !old_default {
                            s.values.insert(k, n);
                        }
                    }
                    // 손상 값 = 기본값(fail-soft) · 저장 시 그 줄은 사라진다.
                }
                None => s.unknown.push((k, v)),
            }
        }
        s.migrate_explorer_refresh();
        s.migrate_result_tabbar();
        s
    }

    /// 옛 `grid.result_tabbar = always`(선택 상자) → `grid.result_tabbar_single = on`(스위치 · 사용자 09-21). `auto`는 새 기본값(off)과
    /// 같아 옮길 것이 없다. 옛 줄은 다음 저장 때 사라진다 · 새 키를 이미 정했으면 건드리지 않는다.
    fn migrate_result_tabbar(&mut self) {
        let old = self
            .unknown
            .iter()
            .find(|(k, _)| k == "grid.result_tabbar")
            .map(|(_, v)| v.clone());
        self.unknown.retain(|(k, _)| k != "grid.result_tabbar");
        if old
            .as_deref()
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("always"))
            && !self.values.contains_key("grid.result_tabbar_single")
        {
            self.values
                .insert("grid.result_tabbar_single".into(), "on".into());
        }
    }

    /// 옛 `explorer.auto_refresh`/`explorer.refresh_secs`(배선된 적 없는 키) → `meta.refresh_secs`(docs/57 §2-5): 자동 갱신을
    /// **켜 둔** 사용자만 그 주기를 옮긴다(꺼짐은 옛 기본값이라 저장돼 있지 않다 → 새 기본 300초). 옛 줄은 다음 저장 때 사라진다.
    fn migrate_explorer_refresh(&mut self) {
        let take = |u: &mut Vec<(String, String)>, key: &str| {
            let v = u.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
            u.retain(|(k, _)| k != key);
            v
        };
        let on = take(&mut self.unknown, "explorer.auto_refresh");
        let secs = take(&mut self.unknown, "explorer.refresh_secs");
        if on.as_deref().is_some_and(|v| v.eq_ignore_ascii_case("on"))
            && !self.values.contains_key("meta.refresh_secs")
        {
            if let Some(n) = secs.and_then(|v| v.trim().parse::<u32>().ok()) {
                if n != 300 {
                    self.values
                        .insert("meta.refresh_secs".into(), n.clamp(0, 3600).to_string());
                }
            }
        }
    }

    /// 파일 경로.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 현재 값(사용자 값 → 기본값). 모르는 키는 `None`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        let e = entry(key)?;
        let def = default_of(e.key).unwrap_or(e.default);
        Some(self.values.get(key).map_or(def, String::as_str))
    }

    /// 사용자가 바꾼 값인가(기본값과 다른가) — VS Code의 "Modified" 표시에 해당.
    #[must_use]
    pub fn is_modified(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// 검증 후 설정(메모리). 기본값과 같으면 사용자 값을 지운다.
    pub fn set(&mut self, key: &str, raw: &str) -> Result<String, SetError> {
        let e = entry(key).ok_or_else(|| SetError::UnknownKey(key.to_string()))?;
        if is_info(key) {
            // 읽기 전용 정보 — 저장하지 않는다.
            return Err(SetError::InvalidValue(
                key.to_string(),
                raw.to_string(),
                "read-only".to_string(),
            ));
        }
        let n = normalize(e.kind, raw).ok_or_else(|| {
            SetError::InvalidValue(key.to_string(), raw.to_string(), allowed(e.kind))
        })?;
        if n == default_of(e.key).unwrap_or(e.default) {
            self.values.remove(key);
        } else {
            self.values.insert(key.to_string(), n.clone());
        }
        Ok(n)
    }

    /// 기본값으로(메모리).
    pub fn reset(&mut self, key: &str) -> Result<&'static str, SetError> {
        let e = entry(key).ok_or_else(|| SetError::UnknownKey(key.to_string()))?;
        self.values.remove(key);
        Ok(default_of(e.key).unwrap_or(e.default))
    }

    /// 원자적 저장(폴더가 없으면 만든다). 내용 = 변경분 + 모르는 키.
    pub fn save(&self) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let known: Vec<(&str, &str)> = self
            .values
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        nexa_conf::write_atomic(&self.path, &nexa_conf::serialize(&known, &self.unknown))
    }

    // ── 타입 있는 접근자(자주 쓰는 키)

    #[must_use]
    pub fn lang(&self) -> Lang {
        self.get("ui.lang")
            .and_then(Lang::from_code)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn theme_mode(&self) -> ThemeMode {
        self.get("ui.theme")
            .and_then(ThemeMode::parse)
            .unwrap_or_default()
    }

    /// on/off 설정 — **실효 값**([`Settings::effective`] · 부하원 키는 `perf.mode` 프리셋을 따른다 · 09-16 T-90a).
    /// 파일 값(사용자/기본)만 보려면 [`Settings::get`].
    #[must_use]
    pub fn flag(&self, key: &str) -> bool {
        self.effective_flag(key)
    }

    /// 글꼴 크기(px) — `Size` 항목(`13` · `13px` · `10pt`) · 틀리면 레지스트리 기본.
    #[must_use]
    pub fn font_px(&self, key: &str) -> f32 {
        self.get(key)
            .and_then(size_px)
            .or_else(|| entry(key).and_then(|e| size_px(e.default)))
            .unwrap_or(0.0)
    }

    /// 정수 설정(레지스트리 기본값 보장 → 실패 없음) — **실효 값**([`Settings::effective`] · 부하원 키는 `perf.mode` 프리셋을 따른다).
    #[must_use]
    pub fn int(&self, key: &str) -> i64 {
        self.effective_int(key)
    }

    /// 전 항목 (키, 현재 값, 변경 여부) — `config list`·설정 화면.
    #[must_use]
    pub fn list(&self) -> Vec<(&'static Entry, &str, bool)> {
        REGISTRY
            .iter()
            .map(|e| {
                (
                    e,
                    self.values
                        .get(e.key)
                        .map_or(default_of(e.key).unwrap_or(e.default), String::as_str),
                    self.values.contains_key(e.key),
                )
            })
            .collect()
    }

    /// 노출 항목만(설정 화면 · `config list`) — 비노출([`is_hidden`])은 `config list all`로만.
    pub fn list_visible(&self) -> Vec<(&'static Entry, &str, bool)> {
        self.list()
            .into_iter()
            .filter(|(e, _, _)| !is_hidden(e.key))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn size_units() {
        assert_eq!(size_px("13"), Some(13.0));
        assert_eq!(size_px("13px"), Some(13.0));
        assert!((size_px("10pt").unwrap_or(0.0) - 13.333_333).abs() < 1e-3);
        assert_eq!(size_px("abc"), None);
        let k = SettingKind::Size { min: 8, max: 40 };
        assert_eq!(normalize(k, "10pt"), Some("10pt".into()));
        assert_eq!(normalize(k, "13px"), Some("13".into()));
        assert_eq!(normalize(k, "10.5pt"), Some("10.5pt".into()));
        assert_eq!(normalize(k, "99"), None);
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nsql-settings-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&d);
        d.join(FILE_NAME)
    }

    #[test]
    fn defaults_english_and_system() {
        let s = Settings::open(tmp("defaults"));
        assert_eq!(s.lang(), Lang::En);
        assert_eq!(s.theme_mode(), ThemeMode::System);
        assert_eq!(s.int("ui.font_size"), 15);
        assert!(!s.is_modified("ui.lang"));
        assert_eq!(s.get("nope"), None);
    }

    #[test]
    fn set_validates_and_saves_only_changes() {
        let p = tmp("set");
        let mut s = Settings::open(p.clone());
        assert_eq!(s.set("ui.lang", "KO").unwrap(), "ko");
        assert_eq!(s.set("ui.theme", "Dark").unwrap(), "dark");
        assert!(matches!(
            s.set("ui.theme", "blue"),
            Err(SetError::InvalidValue(..))
        ));
        assert!(matches!(s.set("x.y", "1"), Err(SetError::UnknownKey(_))));
        assert!(matches!(
            s.set("ui.font_size", "99"),
            Err(SetError::InvalidValue(..))
        ));
        // 기본값으로 되돌리면 줄이 사라진다.
        s.set("ui.theme", "system").unwrap();
        s.save().unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("ui.lang=ko"), "{text}");
        assert!(!text.contains("ui.theme"), "{text}");
        let r = Settings::open(p);
        assert_eq!(r.lang(), Lang::Ko);
        assert_eq!(r.theme_mode(), ThemeMode::System);
        assert!(r.is_modified("ui.lang"));
    }

    #[test]
    fn corrupt_value_falls_back_and_unknown_keys_survive() {
        let p = tmp("corrupt");
        let s = Settings::from_text(
            p.clone(),
            "_schema=1\nui.theme=purple\nui.font_size=abc\nfuture.key=42\n",
        );
        assert_eq!(s.theme_mode(), ThemeMode::System);
        assert_eq!(s.int("ui.font_size"), 15);
        s.save().unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains("future.key=42"), "{text}");
        assert!(!text.contains("purple"), "{text}");
    }

    #[test]
    fn theme_mode_resolution() {
        assert!(ThemeMode::System.is_dark(Some(true)));
        assert!(!ThemeMode::System.is_dark(Some(false)));
        assert!(ThemeMode::System.is_dark(None), "무선호 = 다크(기본 룩)");
        assert!(!ThemeMode::Light.is_dark(Some(true)));
        assert!(ThemeMode::Dark.is_dark(Some(false)));
        assert_eq!(ThemeMode::parse("AUTO"), Some(ThemeMode::System));
        assert_eq!(ThemeMode::System.next().next().next(), ThemeMode::System);
    }

    /// OS별 기본값도 자기 검증을 통과하고, 이 OS에서 `get`이 그 값을 돌려준다.
    #[test]
    fn os_defaults_are_valid_and_applied() {
        for (k, mac, linux) in OS_DEFAULTS {
            let e = entry(k).expect("OS 기본값 키는 레지스트리에 있어야 한다");
            for v in [*mac, *linux] {
                assert_eq!(normalize(e.kind, v).as_deref(), Some(v), "{k}: {v}");
            }
        }
        let s = Settings::open(std::env::temp_dir().join("nexa-os-defaults-none.conf"));
        assert_eq!(s.get("ui.text_hint"), default_of("ui.text_hint"));
        if cfg!(target_os = "macos") {
            assert_eq!(s.get("ui.text_hint"), Some("off"));
            assert_eq!(s.get("ui.text_contrast"), Some("100"));
        }
    }

    #[test]
    fn registry_defaults_are_valid_and_keys_unique() {
        for e in REGISTRY {
            assert_eq!(
                normalize(e.kind, e.default).as_deref(),
                Some(e.default),
                "{}: 기본값이 자기 검증을 통과해야 한다",
                e.key
            );
            assert_eq!(REGISTRY.iter().filter(|x| x.key == e.key).count(), 1);
        }
    }

    /// DBMS 그룹(사용자 09-21): 읽기 전용 정보 키는 저장되지 않고 `set`을 거부한다 · Oracle 폴더·TNS_ADMIN은 직접 지정일 때만 ·
    /// 정보 키는 전부 레지스트리에 있고 DBMS 분류에 속한다.
    #[test]
    fn dbms_info_keys_are_read_only_and_client_paths_need_manual_mode() {
        let mut s = Settings::from_text(std::path::PathBuf::from("x"), "");
        for k in INFO_KEYS {
            let e = entry(k).expect(k);
            assert!(group_of(e.cat) == Some(Msg::GrpDbms), "{k}");
            assert!(s.set(k, "anything").is_err(), "{k}");
        }
        for k in ["oracle.client_dir", "oracle.tns_admin"] {
            let (parent, dep) = dependency(k).expect(k);
            assert_eq!(parent, "oracle.client_mode");
            assert!(dep.satisfied("manual") && !dep.satisfied("auto"));
        }
        assert_eq!(s.get("oracle.client_mode"), Some("auto"));
        assert!(s.set("oracle.client_mode", "manual").is_ok());
        assert!(s
            .set("oracle.client_dir", "C:/oracle/instantclient_19_20")
            .is_ok());
    }

    /// 결과가 1개일 때의 탭 줄 설정은 결과 탭(다중) **바로 밑**에 있고 그 설정에 종속된다(사용자 09-21).
    #[test]
    fn single_result_tabbar_sits_under_result_tabs_and_depends_on_it() {
        let pos = |k: &str| REGISTRY.iter().position(|e| e.key == k).expect(k);
        assert_eq!(
            pos("grid.result_tabbar_single"),
            pos("grid.result_tabs") + 1
        );
        let (parent, dep) = dependency("grid.result_tabbar_single").expect("dep");
        assert_eq!(parent, "grid.result_tabs");
        assert!(dep.satisfied("on") && !dep.satisfied("off"));
        // 옛 선택 상자 값의 이관: always → on · auto → 기본(off) · 새 키가 이미 있으면 그대로 · 옛 줄은 남지 않는다.
        let load = |body: &str| Settings::from_text(std::path::PathBuf::from("x"), body);
        let a = load("_schema=1\ngrid.result_tabbar=always\n");
        assert_eq!(a.get("grid.result_tabbar_single"), Some("on"));
        assert!(!a.unknown.iter().any(|(k, _)| k == "grid.result_tabbar"));
        let b = load("_schema=1\ngrid.result_tabbar=auto\n");
        assert_eq!(b.get("grid.result_tabbar_single"), Some("off"));
        // 새 키를 켜 둔 파일에 옛 줄(auto)이 남아 있어도 끄지 않는다. (옛 줄은 첫 저장 때 사라지고 기본값은 파일에 적히지 않으므로,
        // 새 키의 "끔"과 옛 "always"가 함께 있는 파일은 생기지 않는다.)
        let c = load("_schema=1\ngrid.result_tabbar=auto\ngrid.result_tabbar_single=on\n");
        assert_eq!(c.get("grid.result_tabbar_single"), Some("on"));
    }

    /// 실행 속도 향상(09-17): 켜면 BOOST 키는 사용자 값·모드와 무관하게 강제값 · 잠금 · 출처 boost · 끄면 저장값 그대로 복귀.
    /// BOOST 키는 전부 레지스트리에 있고 값이 자기 검증을 통과해야 한다.
    #[test]
    fn boost_forces_values_and_locks_without_touching_stored() {
        for (k, v) in perf::BOOST {
            let e = entry(k).unwrap_or_else(|| panic!("{k}: 레지스트리에 없음"));
            assert_eq!(
                normalize(e.kind, v).as_deref(),
                Some(*v),
                "{k}: 강제값이 검증을 통과해야 한다"
            );
        }
        let mut s = Settings::open(tmp("boost"));
        s.set("ui.fade_fast", "900").unwrap();
        s.set("statusbar.git", "on").unwrap();
        assert_eq!(s.effective("ui.fade_fast"), Some("900"));
        assert!(!s.boost_locked("ui.fade_fast"));
        s.set("perf.boost", "on").unwrap();
        assert_eq!(s.effective("ui.fade_fast"), Some("0"), "강제값");
        assert_eq!(s.int("ui.fade_fast"), 0);
        assert!(!s.flag("statusbar.git"));
        assert_eq!(s.get("ui.fade_fast"), Some("900"), "저장값은 그대로");
        assert!(s.boost_locked("ui.fade_fast"));
        assert!(
            !s.boost_locked("grid.max_rows"),
            "동작에 영향 있는 키는 강제하지 않는다"
        );
        assert_eq!(s.perf_source("probe.interval"), Some(PerfSource::Boost));
        assert_eq!(
            s.perf_mode_display(),
            PerfMode::Full,
            "향상 모드는 custom 표시와 무관"
        );
        s.set("perf.boost", "off").unwrap();
        assert_eq!(s.effective("ui.fade_fast"), Some("900"));
        assert!(s.flag("statusbar.git"));
    }

    /// T-90a(docs/39 §4): 실효 값 우선순위 — 개별 > 모드 프리셋 > 기본 · full = 기본과 동일 · reset하면 다시 모드.
    #[test]
    fn perf_effective_priority() {
        let mut s = Settings::open(tmp("perf-eff"));
        assert_eq!(s.perf_mode(), PerfMode::Full);
        assert_eq!(s.effective("grid.max_rows"), Some("200"));
        assert_eq!(s.perf_source("grid.max_rows"), Some(PerfSource::Default));
        s.set("perf.mode", "low").unwrap();
        assert_eq!(s.effective("grid.max_rows"), Some("100"));
        assert_eq!(s.int("grid.max_rows"), 100, "int()는 실효 값");
        assert!(!s.flag("probe.enabled"), "flag()는 실효 값");
        assert_eq!(
            s.perf_source("grid.max_rows"),
            Some(PerfSource::Mode(PerfMode::Low))
        );
        // 개별 값이 모드보다 우선.
        s.set("grid.max_rows", "150").unwrap();
        assert_eq!(s.effective("grid.max_rows"), Some("150"));
        assert_eq!(s.perf_source("grid.max_rows"), Some(PerfSource::User));
        s.reset("grid.max_rows").unwrap();
        assert_eq!(
            s.effective("grid.max_rows"),
            Some("100"),
            "지우면 다시 모드"
        );
        // custom = 프리셋 없음.
        s.set("perf.mode", "custom").unwrap();
        assert_eq!(s.effective("grid.max_rows"), Some("200"));
        // 부하원이 아닌 키는 get과 같다.
        assert_eq!(s.effective("ui.font_size"), s.get("ui.font_size"));
        assert_eq!(s.effective("nope"), None);
    }

    /// D-59: 부하원 키를 직접 바꾸면 표시 모드 = custom(설정된 모드는 그대로 남는다).
    #[test]
    fn perf_mode_display_is_custom_when_overridden() {
        let mut s = Settings::open(tmp("perf-custom"));
        s.set("perf.mode", "balanced").unwrap();
        assert_eq!(s.perf_mode_display(), PerfMode::Balanced);
        s.set("probe.interval", "30").unwrap();
        assert_eq!(s.perf_mode_display(), PerfMode::Custom);
        assert_eq!(s.perf_mode(), PerfMode::Balanced, "설정 값은 유지");
        assert_eq!(
            s.effective("grid.max_rows"),
            Some("200"),
            "다른 키는 여전히 모드"
        );
        s.set("ui.font_size", "16").unwrap();
        s.reset("probe.interval").unwrap();
        assert_eq!(
            s.perf_mode_display(),
            PerfMode::Balanced,
            "부하원 아닌 키는 custom을 만들지 않는다"
        );
    }

    /// auto = OS 신호(배터리·원격 → balanced · 그 외 full) · 안내(D-58)는 full일 때만.
    #[test]
    fn perf_auto_follows_signals() {
        let mut s = Settings::open(tmp("perf-auto"));
        s.set("perf.mode", "auto").unwrap();
        let mut sig = nexa_sys::Signals::default();
        perf::set_signals_override(Some(sig));
        assert_eq!(s.perf_mode_resolved(), PerfMode::Full);
        sig.on_battery = Some(true);
        perf::set_signals_override(Some(sig));
        assert_eq!(s.perf_mode_resolved(), PerfMode::Balanced);
        assert_eq!(s.effective("probe.interval"), Some("120"));
        assert_eq!(s.perf_hint(), None, "auto면 안내 없음");
        s.set("perf.mode", "full").unwrap();
        assert_eq!(s.perf_hint(), Some(Msg::StPerfBatteryHint));
        sig.on_battery = Some(false);
        sig.remote_session = Some(true);
        perf::set_signals_override(Some(sig));
        assert_eq!(s.perf_hint(), Some(Msg::StPerfRemoteHint));
        // 동작 줄이기 → ui.animations=auto만 꺼진다(모드는 그대로).
        sig.remote_session = None;
        sig.reduce_motion = Some(true);
        perf::set_signals_override(Some(sig));
        assert!(!s.animations_enabled());
        s.set("ui.animations", "on").unwrap();
        assert!(s.animations_enabled());
        perf::set_signals_override(None);
    }

    /// 원장 무결성: PERF의 키는 전부 레지스트리에 있고 · full 열 = 기본값(기본 모드 = 지금 동작) · 세 값 모두 자기 검증 통과.
    #[test]
    fn perf_ledger_matches_registry() {
        for (k, b) in PERF {
            let e = entry(k).unwrap_or_else(|| panic!("{k}: 레지스트리에 없다"));
            assert_eq!(b.full, e.default, "{k}: full 열은 기본값과 같아야 한다");
            for v in [b.full, b.balanced, b.low] {
                assert_eq!(
                    normalize(e.kind, v).as_deref(),
                    Some(v),
                    "{k}: 프리셋 값 {v}가 검증을 통과해야 한다"
                );
            }
            assert_eq!(PERF.iter().filter(|(x, _)| x == k).count(), 1, "{k} 중복");
            assert_eq!(e.perf().map(|x| x.domain), Some(b.domain));
        }
        let s = Settings::open(tmp("perf-rows"));
        let rows = s.perf_rows();
        assert_eq!(rows.len(), PERF.len());
        assert!(
            rows.windows(2)
                .all(|w| w[0].binding.domain <= w[1].binding.domain),
            "도메인 순"
        );
        assert!(perf_binding("ui.font_size").is_none());
        assert_eq!(PerfMode::parse("LOW"), Some(PerfMode::Low));
        assert_eq!(
            tree_order(Msg::CatPerformance).0,
            0,
            "Performance는 General 그룹"
        );
    }
}
