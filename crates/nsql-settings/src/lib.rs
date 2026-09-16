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

pub mod json;
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
}

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
/// 결과 탭 바 표시(D-74).
const TABBAR_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValTabbarAuto),
    ("always", Msg::ValTabbarAlways),
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
        key: "editor.ruler_alpha",
        cat: Msg::CatEditor,
        label: Msg::LblRulerAlpha,
        desc: Msg::DescRulerAlpha,
        kind: SettingKind::Int { min: 5, max: 100 },
        default: "25",
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
        key: "editor.highlight_selection",
        cat: Msg::CatEditor,
        label: Msg::LblHighlightSel,
        desc: Msg::DescHighlightSel,
        kind: SettingKind::Bool,
        default: "on",
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
        default: "·→_",
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
    Entry {
        key: "grid.result_tabbar",
        cat: Msg::CatGrid,
        label: Msg::LblGridResultTabbar,
        desc: Msg::DescGridResultTabbar,
        kind: SettingKind::Choice(TABBAR_OPTS),
        default: "auto",
    },
    Entry {
        key: "grid.result_tabs_max",
        cat: Msg::CatGrid,
        label: Msg::LblGridResultTabsMax,
        desc: Msg::DescGridResultTabsMax,
        kind: SettingKind::Int { min: 1, max: 64 },
        default: "8",
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
        default: "256",
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
        key: "grid.copy_null",
        cat: Msg::CatGrid,
        label: Msg::LblCopyNull,
        desc: Msg::DescCopyNull,
        kind: SettingKind::Bool,
        default: "off",
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
        key: "grid.col_max_width",
        cat: Msg::CatGrid,
        label: Msg::LblGridColMax,
        desc: Msg::DescGridColMax,
        kind: SettingKind::Int { min: 40, max: 4000 },
        default: "420",
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
    Entry {
        key: "explorer.auto_refresh",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerAutoRefresh,
        desc: Msg::DescExplorerAutoRefresh,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "explorer.refresh_secs",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerRefreshSecs,
        desc: Msg::DescExplorerRefreshSecs,
        kind: SettingKind::Int { min: 60, max: 3600 },
        default: "300",
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
        key: "explorer.tooltip",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerTooltip,
        desc: Msg::DescExplorerTooltip,
        kind: SettingKind::Bool,
        default: "on",
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
        default: "off",
    },
    Entry {
        key: "editor.minimap_width",
        cat: Msg::CatEditor,
        label: Msg::LblEditorMinimapWidth,
        desc: Msg::DescEditorMinimapWidth,
        kind: SettingKind::Int { min: 20, max: 400 },
        default: "80",
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
        default: "60",
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
        default: "640",
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
        default: "520",
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
        default: "58",
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
    Entry {
        key: "oracle.live.source",
        cat: Msg::CatConnection,
        label: Msg::LblLiveSource,
        desc: Msg::DescLiveSource,
        kind: SettingKind::Choice(LIVE_SOURCE_OPTS),
        default: "session",
    },
    Entry {
        key: "oracle.live.interval_ms",
        cat: Msg::CatConnection,
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
        cat: Msg::CatConnection,
        label: Msg::LblLiveTable,
        desc: Msg::DescLiveTable,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "oracle.live.ts_col",
        cat: Msg::CatConnection,
        label: Msg::LblLiveTsCol,
        desc: Msg::DescLiveTsCol,
        kind: SettingKind::Text,
        default: "LOG_TIME",
    },
    Entry {
        key: "oracle.live.text_col",
        cat: Msg::CatConnection,
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
    Entry {
        key: "connect.reconnect_same",
        cat: Msg::CatConnection,
        label: Msg::LblReconnectSame,
        desc: Msg::DescReconnectSame,
        kind: SettingKind::Bool,
        default: "off",
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
    // ── 트랜잭션 UX(DR-30 · T-77 · docs/34 §2-5)
    Entry {
        key: "tx.stale_min",
        cat: Msg::CatSession,
        label: Msg::LblTxStaleMin,
        desc: Msg::DescTxStaleMin,
        kind: SettingKind::Int { min: 1, max: 1440 },
        default: "10",
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
        key: "editor.highlight_max_kb",
        cat: Msg::CatEditor,
        label: Msg::LblHighlightMaxKb,
        desc: Msg::DescHighlightMaxKb,
        kind: SettingKind::Int {
            min: 0,
            max: 1_000_000,
        },
        default: "1024",
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
    (Msg::GrpEditors, &[Msg::CatEditor, Msg::CatFiles]),
    (Msg::GrpConnections, &[Msg::CatConnection, Msg::CatCli]),
    (Msg::GrpDataEditor, &[Msg::CatGrid]),
];

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
pub const DEPENDS: &[(&str, &str, Dep)] = &[
    ("explorer.refresh_secs", "explorer.auto_refresh", Dep::On),
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

    fn from_text(path: PathBuf, text: &str) -> Settings {
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
                        if n != e.default {
                            s.values.insert(k, n);
                        }
                    }
                    // 손상 값 = 기본값(fail-soft) · 저장 시 그 줄은 사라진다.
                }
                None => s.unknown.push((k, v)),
            }
        }
        s
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
        Some(self.values.get(key).map_or(e.default, String::as_str))
    }

    /// 사용자가 바꾼 값인가(기본값과 다른가) — VS Code의 "Modified" 표시에 해당.
    #[must_use]
    pub fn is_modified(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// 검증 후 설정(메모리). 기본값과 같으면 사용자 값을 지운다.
    pub fn set(&mut self, key: &str, raw: &str) -> Result<String, SetError> {
        let e = entry(key).ok_or_else(|| SetError::UnknownKey(key.to_string()))?;
        let n = normalize(e.kind, raw).ok_or_else(|| {
            SetError::InvalidValue(key.to_string(), raw.to_string(), allowed(e.kind))
        })?;
        if n == e.default {
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
        Ok(e.default)
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
                    self.values.get(e.key).map_or(e.default, String::as_str),
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
