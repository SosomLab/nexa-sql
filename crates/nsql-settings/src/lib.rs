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
const OLD_DEFAULTS: &[(&str, &str)] = &[
    ("editor.minimap_width", "80"),
    // 09-28(사용자): 서버 노드 연결 해제 = 항상 고르기 · 파일 검색 상한 = SSD 기준 8 MB(docs/72 §4-1).
    ("explorer.disconnect_pick", "auto"),
    ("search.max_file_kb", "1024"),
];

/// ★ **키 이름 바꿈 표**(09-28 · docs/94 §6): `(옛 키, 새 키)`. 파일의 옛 줄은 읽을 때 새 키로 옮기고(새 키를 이미 정했으면 옛 값은
/// 버린다) 다음 저장 때 사라진다. `nsql config get/set 옛키`도 새 키로 통한다([`canonical_key`]). 새 키의 접두 = 설정 창 카테고리
/// (키 이름만 보고 어디 있는지 알게 · 사용자 09-28 "변수 이름이 설정 위치와 연결되지 않아 찾기 어렵다").
pub const RENAMED: &[(&str, &str)] = &[
    // 3차(09-30 · 94 §6-4): 확장 설정 키 = `ext.<확장>.<키>`(사용자 "모든 확장 프로그램에 규칙 적용").
    ("grid.fast_scroll", "scroll.fast"),
    ("sqlfmt.strict", "ext.sqlfmt_kiros33.strict"),
    ("sqlfmt.align_as", "ext.sqlfmt_kiros33.align_as"),
    ("sqlfmt.align_ops", "ext.sqlfmt_kiros33.align_ops"),
    ("sqlfmt.align_order", "ext.sqlfmt_kiros33.align_order"),
    ("sqlfmt.outlier_chars", "ext.sqlfmt_kiros33.outlier_chars"),
    ("sqlfmt.force_as", "ext.sqlfmt_kiros33.force_as"),
    ("sqlfmt.and_same_level", "ext.sqlfmt_kiros33.and_same_level"),
    ("sqlfmt.set_op_dashes", "ext.sqlfmt_kiros33.set_op_dashes"),
    ("rainbowpair.enabled", "ext.rainbow_pairs.enabled"),
    ("rainbowpair.colors", "ext.rainbow_pairs.colors"),
    (
        "rainbowpair.contrast_order",
        "ext.rainbow_pairs.contrast_order",
    ),
    ("rainbowpair.unmatched", "ext.rainbow_pairs.unmatched"),
    ("rainbowpair.max_kb", "ext.rainbow_pairs.max_kb"),
    ("license.gates_dev", "license.gates"),
    ("ui.ime_hint", "input.ime_hint"),
    ("ui.ime_hint_watch", "input.ime_hint_watch"),
    ("editor.tab_line_scratch", "editor.tab_line_unsaved"),
    // 2차(09-29 · T-250 · 94 §6-2): 로그인 창 UI = login.* · 그리드 키 규칙은 grid. · 단위/색 접미.
    ("conn.delete_confirm_ms", "login.delete_confirm_ms"),
    (
        "conn.close_after_connect_ms",
        "login.close_after_connect_ms",
    ),
    ("conn.window_w", "login.window_w"),
    ("conn.window_h", "login.window_h"),
    ("conn.panel_w", "login.panel_w"),
    ("conn.port_w", "login.port_w"),
    ("conn.button_scale_pct", "login.button_scale_pct"),
    ("sql.key_mode", "grid.key_mode"),
    ("ui.fade_fast", "ui.fade_fast_ms"),
    ("ui.fade_slow", "ui.fade_slow_ms"),
    ("editor.tab_accent", "editor.tab_accent_color"),
    // 4차(09-30 · 94 §6-5): 이미 ms인데 단위 접미가 없던 키 — 값은 그대로.
    (
        "explorer.typeahead_timeout",
        "explorer.typeahead_timeout_ms",
    ),
];

/// ★ **단위 변환 표**(09-30 · 사용자 "10초를 넘는 시간 설정은 초, 그 이하는 ms" · docs/94 §6-5): `(옛 키, 새 키, 배수)` —
/// **새 값 = 옛 값 × 배수**. 읽을 때 옛 줄을 새 키로 옮기며 곱하고(새 키를 이미 정했으면 옛 값은 버림) · `nsql config get/set 옛키`는
/// **옛 단위 그대로** 통한다([`Settings::get_as`] = 나누기 · [`Settings::set`] = 곱하기 · 소수 허용 `set ui.toast_secs 2.5`).
/// 기준 = 기본값이 10초 이하인 시간 설정(0 = 끔인 타임아웃류는 제외 · 보통 값이 수십 초 이상).
pub const RESCALED: &[(&str, &str, i64)] = &[
    ("meta.refresh_idle_secs", "meta.refresh_idle_ms", 1000),
    ("probe.timeout", "probe.timeout_ms", 1000),
    ("probe.retry_delay", "probe.retry_delay_ms", 1000),
    ("ui.toast_secs", "ui.toast_ms", 1000),
    ("run.toast_hide_secs", "run.toast_hide_ms", 1000),
    (
        "project.autosave_change_secs",
        "project.autosave_change_ms",
        1000,
    ),
];

/// 옛 키면 새 키를, 아니면 그대로([`RENAMED`] + [`RESCALED`]).
#[must_use]
pub fn canonical_key(key: &str) -> &str {
    if let Some((_, new)) = RENAMED.iter().find(|(old, _)| *old == key) {
        return new;
    }
    RESCALED
        .iter()
        .find(|(old, _, _)| *old == key)
        .map_or(key, |(_, new, _)| *new)
}

/// 단위가 바뀐 옛 키면 `(새 키, 배수)` — 옛 단위 값 × 배수 = 새 단위 값.
#[must_use]
pub fn alias_scale(key: &str) -> Option<(&'static str, i64)> {
    RESCALED
        .iter()
        .find(|(old, _, _)| *old == key)
        .map(|(_, new, k)| (*new, *k))
}

/// 옛 단위의 문자열(소수 허용 · `"2.5"`)을 새 단위 정수 문자열로(`"2500"`). 숫자가 아니면 그대로(검증은 `normalize`가 한다).
fn scale_raw(raw: &str, k: i64) -> String {
    let t = raw.trim();
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() => format!("{}", (v * k as f64).round() as i64),
        _ => t.to_string(),
    }
}

/// 새 단위 정수 문자열을 옛 단위로(`"2500"` → `"2.5"` · 나누어떨어지면 `"3"`).
fn unscale_value(v: &str, k: i64) -> String {
    match v.trim().parse::<i64>() {
        Ok(n) if k > 0 => {
            if n % k == 0 {
                (n / k).to_string()
            } else {
                let s = format!("{:.3}", n as f64 / k as f64);
                s.trim_end_matches('0').trim_end_matches('.').to_string()
            }
        }
        _ => v.to_string(),
    }
}

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
/// 플래시 메시지 상자 모양(사용자 10-07): 직사각형 · 둥근 모서리(기본) · 없음(글만).
const FLASH_SHAPE_OPTS: &[(&str, Msg)] = &[
    ("rect", Msg::ValFlashShapeRect),
    ("rounded", Msg::ValFlashShapeRounded),
    ("none", Msg::ValFlashShapeNone),
];
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
/// 부분 결과에 필터를 걸 때(T-285 · D-257): auto = 서버에서 걸러 같은 탭 재조회 · local = 가져온 행만(채움 상한) · ask = 안내.
const FILTER_SERVER_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValFilterServerAuto),
    ("local", Msg::ValFilterServerLocal),
    ("ask", Msg::ValFilterServerAsk),
];
/// 조건 바 열 이름 인용(사용자 10-07): needed = 필요할 때만(특수문자·숫자 시작·예약어·대소문자 접힘 규칙 위반) · always = 늘.
const QUOTE_IDENTS_OPTS: &[(&str, Msg)] = &[
    ("needed", Msg::ValQuoteNeeded),
    ("always", Msg::ValQuoteAlways),
];
const CLOSE_UNSAVED_OPTS: &[(&str, Msg)] =
    &[("ask", Msg::ValCloseAsk), ("twice", Msg::ValCloseTwice)];
const SELECT_ALL_OPTS: &[(&str, Msg)] = &[
    ("keep", Msg::ValSelectAllKeep),
    ("end", Msg::ValSelectAllEnd),
];
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
const BULK_MODE_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::OptBulkAuto),
    ("driver", Msg::OptBulkDriver),
    ("multirow", Msg::OptBulkMultiRow),
    ("single", Msg::OptBulkSingle),
];
const GE_CONC_OPTS: &[(&str, Msg)] = &[
    ("key", Msg::OptGeConcKey),
    ("key_old", Msg::OptGeConcKeyOld),
    ("all_old", Msg::OptGeConcAllOld),
];
/// 값 목록 범위(`grid.filter_values_scope` · 사용자 10-06).
const VALUES_SCOPE_OPTS: &[(&str, Msg)] = &[
    ("others", Msg::OptValuesScopeOthers),
    ("all", Msg::OptValuesScopeAll),
];
/// 열 머리 깔때기 표시 방법(`grid.filter_funnel` · 사용자 10-06).
const FUNNEL_OPTS: &[(&str, Msg)] = &[
    ("always", Msg::OptFunnelAlways),
    ("hover", Msg::OptFunnelHover),
    ("none", Msg::OptFunnelNone),
];
const GE_REFRESH_OPTS: &[(&str, Msg)] = &[
    ("rows", Msg::OptGeRows),
    ("requery", Msg::OptGeRequery),
    ("local", Msg::OptGeLocal),
];
const REFETCH_OPTS: &[(&str, Msg)] = &[
    ("strict", Msg::ValRefetchStrict),
    ("strict_all", Msg::ValRefetchStrictAll),
    ("offset", Msg::ValRefetchOffset),
];

const FILTER_SCOPE_OPTS: &[(&str, Msg)] =
    &[("all", Msg::ValFilterAll), ("shown", Msg::ValFilterShown)];

/// 탭 닫기 상자 표시(`editor.tab_close_show`).
/// 탭 닫힘 뒤 자동 선택된 탭이 탐색기에 미치는 동작(사용자 10-07): keep = 선택만(지금 자리 유지) · move = 탭을 클릭한 것처럼 이동.
const TAB_CLOSE_FOCUS_OPTS: &[(&str, Msg)] = &[
    ("keep", Msg::ValTabFocusKeep),
    ("move", Msg::ValTabFocusMove),
];
/// 닫힘 뒤 다음 활성(기록을 안 쓸 때): next = 닫힌 자리(종전 동작) · prev = 왼쪽.
const TAB_CLOSE_SELECT_OPTS: &[(&str, Msg)] = &[
    ("next", Msg::ValTabCloseSelectNext),
    ("prev", Msg::ValTabCloseSelectPrev),
];
const TAB_CLOSE_SHOW_OPTS: &[(&str, Msg)] = &[
    ("always", Msg::ValTabCloseAlways),
    ("hover", Msg::ValTabCloseHover),
];
// ★ SQL 포맷 공통 옵션(docs/95 · nsql-format `Options::from_pairs`와 같은 값 어휘).
/// 고속 스크롤 속도(사용자 09-30): (연속 N번마다 +1, 상한) = slow (6, ×4) · normal (5, ×8) · fast (3, ×16) · turbo (2, ×32).
pub const SCROLL_SPEED_OPTS: &[(&str, Msg)] = &[
    ("slow", Msg::ValScrollSpeedSlow),
    ("normal", Msg::ValScrollSpeedNormal),
    ("fast", Msg::ValScrollSpeedFast),
    ("turbo", Msg::ValScrollSpeedTurbo),
];

/// 속도 이름 → (연속 N번마다 배수 +1, 배수 상한). 순수.
#[must_use]
pub fn scroll_speed_params(name: &str) -> (u32, i32) {
    match name.trim() {
        "slow" => (6, 4),
        "normal" => (5, 8),
        "turbo" => (2, 32),
        _ => (3, 16),
    }
}

/// kiros33 중복 별칭 처리(사용자 09-30).
const SQLFMT_DUP_OPTS: &[(&str, Msg)] = &[
    ("numbered", Msg::ValSqlfmtDupNumbered),
    ("qualified", Msg::ValSqlfmtDupQualified),
    ("keep", Msg::ValSqlfmtDupKeep),
];
const FMT_INDENT_OPTS: &[(&str, Msg)] = &[
    ("editor", Msg::ValActiveTab),
    ("tab", Msg::ValFmtTab),
    ("space", Msg::ValFmtSpace),
];
/// 들여쓰기 폭 = 활성 탭 설정 또는 고정 칸 수(사용자 09-29 "두 설정 모두 활성 탭 설정이 기본").
const FMT_WIDTH_OPTS: &[(&str, Msg)] = &[
    ("editor", Msg::ValActiveTab),
    ("2", Msg::ValNum2),
    ("3", Msg::ValNum3),
    ("4", Msg::ValNum4),
    ("6", Msg::ValNum6),
    ("8", Msg::ValNum8),
];
const FMT_CASE_OPTS: &[(&str, Msg)] = &[
    ("keep", Msg::ValFmtKeep),
    ("upper", Msg::ValFmtUpper),
    ("lower", Msg::ValFmtLower),
];
const FMT_COMMA_OPTS: &[(&str, Msg)] = &[
    ("leading", Msg::ValFmtLeading),
    ("trailing", Msg::ValFmtTrailing),
];
const FMT_GAP_OPTS: &[(&str, Msg)] = &[("space", Msg::ValFmtSpace), ("tab", Msg::ValFmtTab)];
/// 간격 3택 = 활성 탭 설정 / 공백 / 탭(사용자 09-30 · 인라인 주석 앞).
const FMT_GAP3_OPTS: &[(&str, Msg)] = &[
    ("editor", Msg::ValActiveTab),
    ("space", Msg::ValFmtSpace),
    ("tab", Msg::ValFmtTab),
];
/// 조건 줄(AND/OR) 위치(사용자 09-29): 한 단계 안 / WHERE와 같은 열.
const FMT_COND_INDENT_OPTS: &[(&str, Msg)] = &[
    ("indent", Msg::ValFmtCondIndent),
    ("same", Msg::ValFmtCondSame),
];
/// 포맷 방언 치환 대상(T-255 · 스킬 §22).
const FMT_DIALECT_OPTS: &[(&str, Msg)] = &[
    ("none", Msg::ValFmtDialectNone),
    ("ansi", Msg::ValFmtDialectAnsi),
    ("oracle", Msg::ValFmtDialectOracle),
    ("tsql", Msg::ValFmtDialectTsql),
];
/// Ctrl 객체 링크 밑줄 표시 방식(사용자 09-29 2차): 전부 · 마우스 아래만(기본) · 표시 안 함(동작만).
const OBJLINK_DISPLAY_OPTS: &[(&str, Msg)] = &[
    ("all", Msg::ValObjLinkAll),
    ("hover", Msg::ValObjLinkHover),
    ("none", Msg::ValObjLinkNone),
];
/// 밑줄 모양(정상/미확인 공용).
const OBJLINK_LINE_OPTS: &[(&str, Msg)] = &[
    ("solid", Msg::ValLineSolid),
    ("dashed", Msg::ValLineDashed),
    ("dotted", Msg::ValLineDotted),
    ("wavy", Msg::ValLineWavy),
    ("wavy_dashed", Msg::ValLineWavyDashed),
];
/// Ctrl 객체 링크 툴팁 위치(T-256 · 기본 = 대상의 우상단).
/// Ctrl+클릭 동작(`objlink.click` · T-180 ② — 77의 `intel.ctrl_click` 자리 · 키는 링크 설정 묶음에 둔다).
const OBJLINK_CLICK_OPTS: &[(&str, Msg)] = &[
    ("copy", Msg::ValObjLinkClickCopy),
    ("reveal", Msg::ValObjLinkClickReveal),
];
const OBJLINK_POS_OPTS: &[(&str, Msg)] = &[
    ("top_right", Msg::ValPosTopRight),
    ("top_left", Msg::ValPosTopLeft),
    ("bottom_right", Msg::ValPosBottomRight),
    ("bottom_left", Msg::ValPosBottomLeft),
];
const FMT_LOGICAL_OPTS: &[(&str, Msg)] =
    &[("before", Msg::ValFmtBefore), ("after", Msg::ValFmtAfter)];
const FMT_LIST_OPTS: &[(&str, Msg)] = &[
    ("multi", Msg::ValFmtMulti),
    ("single", Msg::ValFmtSingle),
    ("auto", Msg::ValFmtAuto),
];
const FMT_ALIAS_AS_OPTS: &[(&str, Msg)] = &[
    ("keep", Msg::ValFmtKeep),
    ("add", Msg::ValFmtAdd),
    ("remove", Msg::ValFmtRemove),
];
const FMT_NEWLINE_OPTS: &[(&str, Msg)] = &[
    ("keep", Msg::ValFmtKeep),
    ("lf", Msg::ValFmtLf),
    ("crlf", Msg::ValFmtCrLf),
];
/// 생성 SQL 바인드 목록 주석 위치(`gen.bind_note`).
const BIND_NOTE_OPTS: &[(&str, Msg)] = &[
    ("both", Msg::ValBindNoteBoth),
    ("header", Msg::ValBindNoteHeader),
    ("tail", Msg::ValBindNoteTail),
    ("off", Msg::ValBindNoteOff),
];
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
/// 테이블 alias 삽입 방식(사용자 09-29): 약어(단어 첫 글자 · `sales_customer` → `sc`) 또는 A, B, C 순서.
const INTEL_ALIAS_STYLE: &[(&str, Msg)] = &[
    ("abbr", Msg::ValIntelAliasAbbr),
    ("letters", Msg::ValIntelAliasLetters),
];
const INTEL_STAR_SPACE: &[(&str, Msg)] = &[
    ("editor", Msg::ValIntelStarEditor),
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

/// 화면 언어 값 "OS 표시 언어를 따른다"(사용자 10-04 · nexa-dir3과 같은 값) — 기본값. 실제 언어는 [`Settings::lang`]이 푼다.
pub const LANG_SYSTEM: &str = "system";

/// ★ 설정 레지스트리 — 단일 원천. 새 설정 = 여기 한 줄 + `Msg` 라벨/설명 2줄.
pub const REGISTRY: &[Entry] = &[
    Entry {
        key: "ui.lang",
        cat: Msg::CatAppearance,
        label: Msg::LblLang,
        desc: Msg::DescLang,
        kind: SettingKind::Lang,
        default: LANG_SYSTEM,
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
    //   `ext.rainbow_pairs.auto_close`는 없앴다(그 분류는 확장이 꺼지면 숨고, 꺼진 상태 기본값이 켜짐이라 끌 방법이 없었다).
    Entry {
        key: "editor.auto_close_pairs",
        cat: Msg::CatEditor,
        label: Msg::LblAutoClosePairs,
        desc: Msg::DescAutoClosePairs,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ── 쌍 강조(편집 코어 · 사용자 09-23 "Rainbow 확장이 아니라 기본 기능 설정으로 · 강조 대상을 지정해서"): 종류 목록 ·
    //    문자열 안 · 현재 쌍 강조. Rainbow Pairs 확장은 색만 든다(`ext.rainbow_pairs.*`). 자동 닫기도 같은 종류 목록을 따른다.
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
        key: "grid.key_mode",
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
        key: "editor.tab_accent_color",
        cat: Msg::CatTabs,
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
    // ★ 확장 뷰 탭의 세션 표식(사용자 10-07 "확장은 서버 연결이 필요 없다") — 기본 숨김 · 고급.
    Entry {
        key: "extensions.tab_badge",
        cat: Msg::CatExtManager,
        label: Msg::LblExtTabBadge,
        desc: Msg::DescExtTabBadge,
        kind: SettingKind::Bool,
        default: "off",
    },
    // ★ SQL 포맷(docs/95 · 사용자 09-28): 기본 포맷터 + Basic·확장 공용 옵션(`format.*`).
    Entry {
        key: "format.default",
        cat: Msg::CatFormat,
        label: Msg::LblFmtDefault,
        desc: Msg::DescFmtDefault,
        kind: SettingKind::Text,
        default: "basic",
    },
    // 문서 전체 포맷 상한(사용자 09-30 · 72 §2): 넘거나 큰 파일 모드 = 선택·캐럿 문장(앞뒤 256 KB 창)만 · 0 = 무제한.
    Entry {
        key: "format.max_kb",
        cat: Msg::CatFormat,
        label: Msg::LblFmtMaxKb,
        desc: Msg::DescFmtMaxKb,
        kind: SettingKind::Int {
            min: 0,
            max: 1_048_576,
        },
        default: "1024",
    },
    Entry {
        key: "format.indent",
        cat: Msg::CatFormat,
        label: Msg::LblFmtIndent,
        desc: Msg::DescFmtIndent,
        kind: SettingKind::Choice(FMT_INDENT_OPTS),
        // ★ 기본 = 활성 탭 설정(상태줄 탭 크기/공백 · 사용자 09-29 · 옛 `format.indent_from_tab`은 이주로 흡수).
        default: "editor",
    },
    Entry {
        key: "format.indent_width",
        cat: Msg::CatFormat,
        label: Msg::LblFmtIndentWidth,
        desc: Msg::DescFmtIndentWidth,
        kind: SettingKind::Choice(FMT_WIDTH_OPTS),
        default: "editor",
    },
    Entry {
        key: "format.keyword_case",
        cat: Msg::CatFormat,
        label: Msg::LblFmtKeywordCase,
        desc: Msg::DescFmtKeywordCase,
        kind: SettingKind::Choice(FMT_CASE_OPTS),
        default: "upper",
    },
    Entry {
        key: "format.identifier_case",
        cat: Msg::CatFormat,
        label: Msg::LblFmtIdentifierCase,
        desc: Msg::DescFmtIdentifierCase,
        kind: SettingKind::Choice(FMT_CASE_OPTS),
        default: "keep",
    },
    Entry {
        key: "format.function_case",
        cat: Msg::CatFormat,
        label: Msg::LblFmtFunctionCase,
        desc: Msg::DescFmtFunctionCase,
        kind: SettingKind::Choice(FMT_CASE_OPTS),
        default: "keep",
    },
    Entry {
        key: "format.comma",
        cat: Msg::CatFormat,
        label: Msg::LblFmtComma,
        desc: Msg::DescFmtComma,
        kind: SettingKind::Choice(FMT_COMMA_OPTS),
        // ★ Basic 기본 = DBeaver 일반형(콤마 뒤 · 사용자 09-29 "내 기준은 kiros33 포맷터에서").
        default: "trailing",
    },
    Entry {
        key: "format.comma_gap",
        cat: Msg::CatFormat,
        label: Msg::LblFmtCommaGap,
        desc: Msg::DescFmtCommaGap,
        kind: SettingKind::Choice(FMT_GAP_OPTS),
        default: "space",
    },
    Entry {
        key: "format.logical_newline",
        cat: Msg::CatFormat,
        label: Msg::LblFmtLogicalNewline,
        desc: Msg::DescFmtLogicalNewline,
        kind: SettingKind::Choice(FMT_LOGICAL_OPTS),
        default: "before",
    },
    Entry {
        key: "format.logical_gap",
        cat: Msg::CatFormat,
        label: Msg::LblFmtLogicalGap,
        desc: Msg::DescFmtLogicalGap,
        kind: SettingKind::Choice(FMT_GAP_OPTS),
        default: "space",
    },
    Entry {
        key: "format.where_seed",
        cat: Msg::CatFormat,
        label: Msg::LblFmtWhereSeed,
        desc: Msg::DescFmtWhereSeed,
        kind: SettingKind::Bool,
        default: "off",
    },
    // 사용자 09-29: 시드 구분(공백/탭) · 조건 줄 위치(한 단계 안 / 같은 열).
    Entry {
        key: "format.seed_gap",
        cat: Msg::CatFormat,
        label: Msg::LblFmtSeedGap,
        desc: Msg::DescFmtSeedGap,
        kind: SettingKind::Choice(FMT_GAP_OPTS),
        default: "space",
    },
    Entry {
        key: "format.cond_indent",
        cat: Msg::CatFormat,
        label: Msg::LblFmtCondIndent,
        desc: Msg::DescFmtCondIndent,
        kind: SettingKind::Choice(FMT_COND_INDENT_OPTS),
        default: "indent",
    },
    Entry {
        key: "format.list_style",
        cat: Msg::CatFormat,
        label: Msg::LblFmtListStyle,
        desc: Msg::DescFmtListStyle,
        kind: SettingKind::Choice(FMT_LIST_OPTS),
        default: "multi",
    },
    Entry {
        key: "format.line_width",
        cat: Msg::CatFormat,
        label: Msg::LblFmtLineWidth,
        desc: Msg::DescFmtLineWidth,
        kind: SettingKind::Int { min: 40, max: 400 },
        default: "120",
    },
    Entry {
        key: "format.case_inline_max",
        cat: Msg::CatFormat,
        label: Msg::LblFmtCaseInlineMax,
        desc: Msg::DescFmtCaseInlineMax,
        kind: SettingKind::Int { min: 20, max: 400 },
        default: "120",
    },
    Entry {
        key: "format.operator_spaces",
        cat: Msg::CatFormat,
        label: Msg::LblFmtOperatorSpaces,
        desc: Msg::DescFmtOperatorSpaces,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "format.operator_gap",
        cat: Msg::CatFormat,
        label: Msg::LblFmtOperatorGap,
        desc: Msg::DescFmtOperatorGap,
        kind: SettingKind::Choice(FMT_GAP_OPTS),
        default: "space",
    },
    Entry {
        key: "format.operator_long_space",
        cat: Msg::CatFormat,
        label: Msg::LblFmtOperatorLongSpace,
        desc: Msg::DescFmtOperatorLongSpace,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "format.column_alias_all",
        cat: Msg::CatFormat,
        label: Msg::LblFmtColumnAliasAll,
        desc: Msg::DescFmtColumnAliasAll,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "format.as_gap",
        cat: Msg::CatFormat,
        label: Msg::LblFmtAsGap,
        desc: Msg::DescFmtAsGap,
        kind: SettingKind::Choice(FMT_GAP_OPTS),
        default: "space",
    },
    Entry {
        key: "format.window_break",
        cat: Msg::CatFormat,
        label: Msg::LblFmtWindowBreak,
        desc: Msg::DescFmtWindowBreak,
        kind: SettingKind::Int { min: 0, max: 400 },
        default: "0",
    },
    Entry {
        key: "format.comment_space",
        cat: Msg::CatFormat,
        label: Msg::LblFmtCommentSpace,
        desc: Msg::DescFmtCommentSpace,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "format.comment_gap",
        cat: Msg::CatFormat,
        label: Msg::LblFmtCommentGap,
        desc: Msg::DescFmtCommentGap,
        kind: SettingKind::Choice(FMT_GAP3_OPTS),
        default: "editor",
    },
    Entry {
        key: "format.column_as",
        cat: Msg::CatFormat,
        label: Msg::LblFmtColumnAs,
        desc: Msg::DescFmtColumnAs,
        kind: SettingKind::Choice(FMT_ALIAS_AS_OPTS),
        default: "keep",
    },
    Entry {
        key: "format.table_as",
        cat: Msg::CatFormat,
        label: Msg::LblFmtTableAs,
        desc: Msg::DescFmtTableAs,
        kind: SettingKind::Choice(FMT_ALIAS_AS_OPTS),
        default: "keep",
    },
    Entry {
        key: "format.join_indent",
        cat: Msg::CatFormat,
        label: Msg::LblFmtJoinIndent,
        desc: Msg::DescFmtJoinIndent,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "format.keep_oneliners",
        cat: Msg::CatFormat,
        label: Msg::LblFmtKeepOneliners,
        desc: Msg::DescFmtKeepOneliners,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "format.stmt_blank_lines",
        cat: Msg::CatFormat,
        label: Msg::LblFmtStmtBlankLines,
        desc: Msg::DescFmtStmtBlankLines,
        kind: SettingKind::Int { min: 0, max: 5 },
        default: "1",
    },
    Entry {
        key: "format.max_blank_lines",
        cat: Msg::CatFormat,
        label: Msg::LblFmtMaxBlankLines,
        desc: Msg::DescFmtMaxBlankLines,
        kind: SettingKind::Int { min: 0, max: 10 },
        default: "2",
    },
    Entry {
        key: "format.newline",
        cat: Msg::CatFormat,
        label: Msg::LblFmtNewline,
        desc: Msg::DescFmtNewline,
        kind: SettingKind::Choice(FMT_NEWLINE_OPTS),
        default: "keep",
    },
    Entry {
        key: "format.final_newline",
        cat: Msg::CatFormat,
        label: Msg::LblFmtFinalNewline,
        desc: Msg::DescFmtFinalNewline,
        kind: SettingKind::Bool,
        default: "on",
    },
    // T-255(09-29): 괄호 그룹 시드 · 별칭 자동 부여 · 방언 치환.
    Entry {
        key: "format.paren_seed",
        cat: Msg::CatFormat,
        label: Msg::LblFmtParenSeed,
        desc: Msg::DescFmtParenSeed,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "format.auto_alias",
        cat: Msg::CatFormat,
        label: Msg::LblFmtAutoAlias,
        desc: Msg::DescFmtAutoAlias,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "format.dialect_target",
        cat: Msg::CatFormat,
        label: Msg::LblFmtDialectTarget,
        desc: Msg::DescFmtDialectTarget,
        kind: SettingKind::Choice(FMT_DIALECT_OPTS),
        default: "none",
    },
    Entry {
        key: "format.semicolon_newline",
        cat: Msg::CatFormat,
        label: Msg::LblFmtSemicolonNewline,
        desc: Msg::DescFmtSemicolonNewline,
        kind: SettingKind::Bool,
        default: "off",
    },
    // ★ Ctrl 객체 하이퍼링크 + 설명 툴팁(`objlink.*` · docs/96 · T-256 · 사용자 09-29): Ctrl(⌘)을 누르는 동안 SQL의
    //   테이블·컬럼·루틴 참조를 링크로 · 커서 아래 링크의 코멘트 툴팁 · 좌클릭 = 설명 복사 · 우클릭 = 메뉴.
    //   향상 모드(`perf::BOOST`)는 `objlink.enabled`를 끈다 · 큰 파일 모드(L1+)와 `objlink.max_kb` 초과는 자동 끔.
    Entry {
        key: "objlink.enabled",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkEnabled,
        desc: Msg::DescObjLinkEnabled,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "objlink.display",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkDisplay,
        desc: Msg::DescObjLinkDisplay,
        kind: SettingKind::Choice(OBJLINK_DISPLAY_OPTS),
        default: "hover",
    },
    Entry {
        key: "objlink.tooltip",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkTooltip,
        desc: Msg::DescObjLinkTooltip,
        kind: SettingKind::Bool,
        default: "on",
    },
    // hover 카드 동작 버튼(T-179 ③ · 77 §1-3 "요약 + 동작 버튼 두어 개").
    Entry {
        key: "objlink.card_buttons",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkCardButtons,
        desc: Msg::DescObjLinkCardButtons,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 머무름 툴팁(T-179 ③ 첫 걸음 · 77의 `intel.hover_ms` 자리 · 10초 이하 = ms 규칙 94 §6-5).
    Entry {
        key: "objlink.hover_ms",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkHoverMs,
        desc: Msg::DescObjLinkHoverMs,
        kind: SettingKind::Int { min: 0, max: 5000 },
        default: "700",
    },
    Entry {
        key: "objlink.tooltip_pos",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkTooltipPos,
        desc: Msg::DescObjLinkTooltipPos,
        kind: SettingKind::Choice(OBJLINK_POS_OPTS),
        default: "top_right",
    },
    Entry {
        key: "objlink.click",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkClick,
        desc: Msg::DescObjLinkClick,
        kind: SettingKind::Choice(OBJLINK_CLICK_OPTS),
        default: "copy",
    },
    Entry {
        key: "objlink.show_schema",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkShowSchema,
        desc: Msg::DescObjLinkShowSchema,
        kind: SettingKind::Bool,
        default: "off",
    },
    // 밑줄 스타일(정상 = 카탈로그에서 확인된 객체 · 미확인 = 현재 연결 메타에 없는 객체 · 사용자 09-29 "밝은 벽돌색").
    Entry {
        key: "objlink.line_color",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkLineColor,
        desc: Msg::DescObjLinkLineColor,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "objlink.line_width",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkLineWidth,
        desc: Msg::DescObjLinkLineWidth,
        kind: SettingKind::Int { min: 0, max: 4 },
        default: "1",
    },
    Entry {
        key: "objlink.line_style",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkLineStyle,
        desc: Msg::DescObjLinkLineStyle,
        kind: SettingKind::Choice(OBJLINK_LINE_OPTS),
        default: "solid",
    },
    Entry {
        key: "objlink.bad_color",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkBadColor,
        desc: Msg::DescObjLinkBadColor,
        kind: SettingKind::Text,
        default: "#B7472A",
    },
    Entry {
        key: "objlink.bad_width",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkBadWidth,
        desc: Msg::DescObjLinkBadWidth,
        kind: SettingKind::Int { min: 0, max: 4 },
        default: "1",
    },
    Entry {
        key: "objlink.bad_style",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkBadStyle,
        desc: Msg::DescObjLinkBadStyle,
        kind: SettingKind::Choice(OBJLINK_LINE_OPTS),
        default: "wavy",
    },
    Entry {
        key: "objlink.max_kb",
        cat: Msg::CatObjLink,
        label: Msg::LblObjLinkMaxKb,
        desc: Msg::DescObjLinkMaxKb,
        kind: SettingKind::Int { min: 0, max: 65536 },
        default: "512",
    },
    // ★ 확장 "SQL Formatter for kiros33"(`ext.sqlfmt_kiros33.*` · docs/95 §4): Basic이 못 다루는 규칙만.
    Entry {
        key: "ext.sqlfmt_kiros33.strict",
        cat: Msg::CatExtSqlFormatter,
        label: Msg::LblSqlfmtStrict,
        desc: Msg::DescSqlfmtStrict,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ext.sqlfmt_kiros33.align_as",
        cat: Msg::CatExtSqlFormatter,
        label: Msg::LblSqlfmtAlignAs,
        desc: Msg::DescSqlfmtAlignAs,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ext.sqlfmt_kiros33.align_ops",
        cat: Msg::CatExtSqlFormatter,
        label: Msg::LblSqlfmtAlignOps,
        desc: Msg::DescSqlfmtAlignOps,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ext.sqlfmt_kiros33.align_order",
        cat: Msg::CatExtSqlFormatter,
        label: Msg::LblSqlfmtAlignOrder,
        desc: Msg::DescSqlfmtAlignOrder,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ext.sqlfmt_kiros33.outlier_chars",
        cat: Msg::CatExtSqlFormatter,
        label: Msg::LblSqlfmtOutlierChars,
        desc: Msg::DescSqlfmtOutlierChars,
        kind: SettingKind::Int { min: 16, max: 400 },
        default: "40",
    },
    Entry {
        key: "ext.sqlfmt_kiros33.force_as",
        cat: Msg::CatExtSqlFormatter,
        label: Msg::LblSqlfmtForceAs,
        desc: Msg::DescSqlfmtForceAs,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ext.sqlfmt_kiros33.window_break",
        cat: Msg::CatExtSqlFormatter,
        label: Msg::LblSqlfmtWindowBreak,
        desc: Msg::DescSqlfmtWindowBreak,
        kind: SettingKind::Int { min: 0, max: 400 },
        default: "40",
    },
    Entry {
        key: "ext.sqlfmt_kiros33.dup_alias",
        cat: Msg::CatExtSqlFormatter,
        label: Msg::LblSqlfmtDupAlias,
        desc: Msg::DescSqlfmtDupAlias,
        kind: SettingKind::Choice(SQLFMT_DUP_OPTS),
        default: "numbered",
    },
    Entry {
        key: "ext.sqlfmt_kiros33.and_same_level",
        cat: Msg::CatExtSqlFormatter,
        label: Msg::LblSqlfmtAndSameLevel,
        desc: Msg::DescSqlfmtAndSameLevel,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ext.sqlfmt_kiros33.set_op_dashes",
        cat: Msg::CatExtSqlFormatter,
        label: Msg::LblSqlfmtSetOpDashes,
        desc: Msg::DescSqlfmtSetOpDashes,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 레인보우 괄호 플러그인(사용자 09-17 · docs/51 · D-92~95): 첫 in-process 확장 `extensions/rainbow_pairs.rs`(Rainbow Pairs)가 읽는다. 향상 모드는 색을 끈다.
    Entry {
        key: "ext.rainbow_pairs.enabled",
        cat: Msg::CatExtRainbowPairs,
        label: Msg::LblRainbow,
        desc: Msg::DescRainbow,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ext.rainbow_pairs.unmatched",
        cat: Msg::CatExtRainbowPairs,
        label: Msg::LblRainbowUnmatched,
        desc: Msg::DescRainbowUnmatched,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ext.rainbow_pairs.colors",
        cat: Msg::CatExtRainbowPairs,
        label: Msg::LblRainbowColors,
        desc: Msg::DescRainbowColors,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "ext.rainbow_pairs.contrast_order",
        cat: Msg::CatExtRainbowPairs,
        label: Msg::LblRainbowContrast,
        desc: Msg::DescRainbowContrast,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ext.rainbow_pairs.max_kb",
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
        default: "rows",
    },
    Entry {
        key: "grid.edit_hidden_keys",
        cat: Msg::CatGrid,
        label: Msg::LblGridEditHiddenKeys,
        desc: Msg::DescGridEditHiddenKeys,
        kind: SettingKind::Bool,
        // 09-27 사용자 결정(D-225 · 성능 우선): 재조회는 기본 안 함 — 편집 툴바 ⚿ 행 식별 열 가져오기(`grid.edit.identify`)로 그 결과에만.
        default: "off",
    },
    Entry {
        key: "grid.edit_rowid",
        cat: Msg::CatGrid,
        label: Msg::LblGridEditRowid,
        desc: Msg::DescGridEditRowid,
        kind: SettingKind::Bool,
        // 09-27 사용자 결정(D-225 · 성능 우선): 재조회는 기본 안 함 — 편집 툴바 ⚿ 행 식별 열 가져오기(`grid.edit.identify`)로 그 결과에만.
        default: "off",
    },
    Entry {
        key: "grid.edit_all_cols",
        cat: Msg::CatGrid,
        label: Msg::LblGridEditAllCols,
        desc: Msg::DescGridEditAllCols,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "grid.edit_concurrency",
        cat: Msg::CatGrid,
        label: Msg::LblGridEditConcurrency,
        desc: Msg::DescGridEditConcurrency,
        kind: SettingKind::Choice(GE_CONC_OPTS),
        default: "key",
    },
    Entry {
        key: "grid.lob_view_max_mb",
        cat: Msg::CatGrid,
        label: Msg::LblGridLobMax,
        desc: Msg::DescGridLobMax,
        kind: SettingKind::Int { min: 1, max: 1024 },
        default: "16",
    },
    Entry {
        key: "grid.lob_image_preview",
        cat: Msg::CatGrid,
        label: Msg::LblGridLobImage,
        desc: Msg::DescGridLobImage,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bulk.batch_rows",
        cat: Msg::CatCli,
        label: Msg::LblBulkBatch,
        desc: Msg::DescBulkBatch,
        kind: SettingKind::Int {
            min: 1,
            max: 100_000,
        },
        default: "1000",
    },
    Entry {
        key: "bulk.commit_every",
        cat: Msg::CatCli,
        label: Msg::LblBulkCommit,
        desc: Msg::DescBulkCommit,
        kind: SettingKind::Int {
            min: 0,
            max: 10_000_000,
        },
        default: "10000",
    },
    Entry {
        key: "bulk.mode",
        cat: Msg::CatCli,
        label: Msg::LblBulkMode,
        desc: Msg::DescBulkMode,
        kind: SettingKind::Choice(BULK_MODE_OPTS),
        default: "auto",
    },
    Entry {
        key: "bulk.empty_null",
        cat: Msg::CatCli,
        label: Msg::LblBulkEmptyNull,
        desc: Msg::DescBulkEmptyNull,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bulk.check_constraints",
        cat: Msg::CatCli,
        label: Msg::LblBulkCheck,
        desc: Msg::DescBulkCheck,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "bulk.fire_triggers",
        cat: Msg::CatCli,
        label: Msg::LblBulkTriggers,
        desc: Msg::DescBulkTriggers,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "bulk.append_hint",
        cat: Msg::CatCli,
        label: Msg::LblBulkAppend,
        desc: Msg::DescBulkAppend,
        kind: SettingKind::Bool,
        default: "off",
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
    // 외래 키 따라가기(T-180 ⑥) — 결과가 오면 그 테이블의 제약을 한 번 읽는다(끄면 읽지 않는다 · 39 §3).
    Entry {
        key: "grid.fk_follow",
        cat: Msg::CatGrid,
        label: Msg::LblGridFkFollow,
        desc: Msg::DescGridFkFollow,
        kind: SettingKind::Bool,
        default: "on",
    },
    // 필터 줄(칩 · × · 77 §2-2 · T-181) — 필터가 있을 때만 한 줄을 차지한다.
    // ★ 필터 중 자동 페치(T-285 · 사용자 결정 D-257/D-258 10-07 · docs/103 §4).
    Entry {
        key: "grid.filter_server",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridFilterServer,
        desc: Msg::DescGridFilterServer,
        kind: SettingKind::Choice(FILTER_SERVER_OPTS),
        default: "auto",
    },
    Entry {
        key: "grid.filter_fill_pages",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridFilterFillPages,
        desc: Msg::DescGridFilterFillPages,
        kind: SettingKind::Int { min: 0, max: 20 },
        default: "3",
    },
    Entry {
        key: "grid.filter_strip",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridFilterStrip,
        desc: Msg::DescGridFilterStrip,
        kind: SettingKind::Bool,
        // D-255(사용자 10-06): 기본 숨김 — 조건 바가 기본이고 개별 제거는 열 머리 메뉴·값 팝업·깔때기로 대체된다 · 켜면 조건 바 아래 둘째 줄.
        default: "off",
    },
    // 값 고르기 하위 메뉴(T-181 · 77 §2-2 "열 머리 깔때기 → 값 목록") — 메뉴 항목 수(72 §3 상한 원장).
    Entry {
        key: "grid.filter_pick_max",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridFilterPickMax,
        desc: Msg::DescGridFilterPickMax,
        kind: SettingKind::Int { min: 5, max: 200 },
        default: "30",
    },
    // ★ 결과 필터 사용 여부(사용자 10-06) — 끄면 필터 메뉴·깔때기·값 목록이 없고 걸려 있던 필터도 푼다.
    Entry {
        key: "grid.filter_enabled",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridFilterEnabled,
        desc: Msg::DescGridFilterEnabled,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 열 머리 깔때기 표시 방법(T-181 후속 · 10-06 · 77 §2-2): 항상 · 머리 위 마우스 오버 · 없음(우클릭 메뉴로만).
    // ★ 열 머리 유형 아이콘(사용자 10-07): 글/숫자/날짜/참거짓 · 깔때기 크기 · 성능 향상 모드 = 끔(헤더 그리기 비용).
    Entry {
        key: "grid.col_type_icons",
        cat: Msg::CatGrid,
        label: Msg::LblGridColTypeIcons,
        desc: Msg::DescGridColTypeIcons,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "grid.filter_funnel",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridFilterFunnel,
        desc: Msg::DescGridFilterFunnel,
        kind: SettingKind::Choice(FUNNEL_OPTS),
        default: "always",
    },
    // 값 목록 팝업의 고유값 상한(72 §3 상한 원장 · 넘치면 "값이 더 있음").
    Entry {
        key: "grid.filter_values_max",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridFilterValuesMax,
        desc: Msg::DescGridFilterValuesMax,
        kind: SettingKind::Int { min: 20, max: 5000 },
        default: "500",
    },
    // ★ 인라인 조건 입력란(DBeaver 조건 바 · 사용자 10-06): 그리드 위 한 줄 · Enter = 출처를 감싸 같은 탭 재실행 · 기본 보임.
    Entry {
        key: "grid.condition_bar",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridConditionBar,
        desc: Msg::DescGridConditionBar,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 글꼴 크기 HUD(사용자 10-07 "고속 스크롤처럼 글꼴 이름·크기를 정해진 시간 보이고 서서히") — 항목·값 = scroll.fast_hud_* 그대로.
    Entry {
        key: "zoom.hud",
        cat: Msg::CatAppearance,
        label: Msg::LblZoomHud,
        desc: Msg::DescZoomHud,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "zoom.hud_pos",
        cat: Msg::CatAppearance,
        label: Msg::LblZoomHudPos,
        desc: Msg::DescZoomHudPos,
        kind: SettingKind::Position,
        default: "top_right",
    },
    Entry {
        key: "zoom.hud_hold_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblZoomHudHoldMs,
        desc: Msg::DescZoomHudHoldMs,
        kind: SettingKind::Int { min: 0, max: 5000 },
        default: "250",
    },
    Entry {
        key: "zoom.hud_fade_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblZoomHudFadeMs,
        desc: Msg::DescZoomHudFadeMs,
        kind: SettingKind::Int { min: 50, max: 5000 },
        default: "600",
    },
    Entry {
        key: "zoom.hud_bg",
        cat: Msg::CatAppearance,
        label: Msg::LblZoomHudBg,
        desc: Msg::DescZoomHudBg,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "zoom.hud_bg_alpha",
        cat: Msg::CatAppearance,
        label: Msg::LblZoomHudBgAlpha,
        desc: Msg::DescZoomHudBgAlpha,
        kind: SettingKind::Int { min: 0, max: 100 },
        default: "40",
    },
    Entry {
        key: "zoom.hud_fg",
        cat: Msg::CatAppearance,
        label: Msg::LblZoomHudFg,
        desc: Msg::DescZoomHudFg,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "zoom.hud_fg_alpha",
        cat: Msg::CatAppearance,
        label: Msg::LblZoomHudFgAlpha,
        desc: Msg::DescZoomHudFgAlpha,
        kind: SettingKind::Int { min: 0, max: 100 },
        default: "100",
    },
    // ★ CREATE 문 ↔ 실제 객체 비교(T-283 · 19 §6-3): 줄 안 공백 차이 무시.
    Entry {
        key: "compare.ignore_ws",
        cat: Msg::CatEditor,
        label: Msg::LblCompareIgnoreWs,
        desc: Msg::DescCompareIgnoreWs,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 조건 바 펼침 최대 줄 수(사용자 10-06 "SHIFT+ENTER를 누르면 최대 3줄(설정)까지 확장").
    Entry {
        key: "grid.cond_max_lines",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridCondMaxLines,
        desc: Msg::DescGridCondMaxLines,
        kind: SettingKind::Int { min: 2, max: 12 },
        default: "3",
    },
    // ★ 열 머리 DnD → 조건 바(사용자 10-06): 연결어 AND + 타입별 기본값(`= ''`/`= 0`)을 붙일지 · 기본 켬.
    Entry {
        key: "grid.cond_drop_template",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridCondDropTemplate,
        desc: Msg::DescGridCondDropTemplate,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 값 목록 범위(사용자 10-06): 다른 열의 필터를 통과한 행의 값만(종속 · 기본) · 받은 행 전체.
    Entry {
        key: "grid.filter_values_scope",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridFilterValuesScope,
        desc: Msg::DescGridFilterValuesScope,
        kind: SettingKind::Choice(VALUES_SCOPE_OPTS),
        default: "others",
    },
    // ★ 값 목록 팝업의 표시 행 수(사용자 10-06 "표시 데이터 행수를 고정") — 팝업 크기는 이 수로 늘 같다(값이 적어도 · 검색으로 줄어도).
    Entry {
        key: "grid.filter_popup_rows",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridFilterPopupRows,
        desc: Msg::DescGridFilterPopupRows,
        kind: SettingKind::Int { min: 3, max: 40 },
        default: "12",
    },
    // 정규식 필터 조회 SQL ③단계 값 목록 상한(사용자 09-30 · 72 §3 · 고유값 cap+1개에서 모으기 멈춤).
    Entry {
        key: "grid.filter_list_max",
        cat: Msg::CatGridFilter,
        label: Msg::LblGridFilterListMax,
        desc: Msg::DescGridFilterListMax,
        kind: SettingKind::Int {
            min: 1,
            max: 100_000,
        },
        default: "1000",
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
        key: "ui.fade_fast_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblFadeFast,
        desc: Msg::DescFadeFast,
        kind: SettingKind::Int { min: 0, max: 5000 },
        default: "500",
    },
    // ★ 플래시 메시지(클릭 복사 "복사됨" 등 · nexa-ctl `Flash`): 유지 시간 뒤 페이드아웃(사용자 09-27 · 기본 유지 2초 · 페이드 3초).
    Entry {
        key: "ui.flash_hold_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblFlashHoldMs,
        desc: Msg::DescFlashHoldMs,
        kind: SettingKind::Int {
            min: 0,
            max: 30_000,
        },
        default: "2000",
    },
    // ★ 설정 창 "고급" 스위치의 상태(사용자 09-29 "고급 보기 여부도 설정으로") — 창의 스위치가 바꾸고 다음에 열 때 복원 · HIDDEN.
    Entry {
        key: "ui.prefs_advanced",
        cat: Msg::CatAppearance,
        label: Msg::LblPrefsAdvanced,
        desc: Msg::DescPrefsAdvanced,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "ui.flash_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblFlashMs,
        desc: Msg::DescFlashMs,
        kind: SettingKind::Int {
            min: 200,
            max: 30_000,
        },
        default: "3000",
    },
    // ★ 플래시 메시지 글꼴(사용자 10-07): 얼굴 비면 시스템 기본(UI 글꼴) · 크기 10pt(프로젝트 탐색기 "색인 다시 읽기" 알림 등).
    Entry {
        key: "ui.flash_font_face",
        cat: Msg::CatAppearance,
        label: Msg::LblFlashFontFace,
        desc: Msg::DescFlashFontFace,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "ui.flash_font_size",
        cat: Msg::CatAppearance,
        label: Msg::LblFlashFontSize,
        desc: Msg::DescFlashFontSize,
        kind: SettingKind::Size { min: 6, max: 40 },
        // 12pt → 9pt → 10pt(사용자 10-07 실기 "9pt로" → "1pt 키워줘").
        default: "10pt",
    },
    Entry {
        key: "ui.flash_shape",
        cat: Msg::CatAppearance,
        label: Msg::LblFlashShape,
        desc: Msg::DescFlashShape,
        kind: SettingKind::Choice(FLASH_SHAPE_OPTS),
        default: "rounded",
    },
    Entry {
        key: "ui.fade_slow_ms",
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
    // ★ 진행 표시 링(혜성 · 사용자 10-07 "모든 검색에 · 컨트롤 속성 · 사용/두께/색/유지시간 설정 · 성능 향상 모드에서 끔"): 전역 스타일 → nexa-ctl.
    // ★ 툴팁 자리(사용자 10-07): 기본 = 항목 위(읽는 방향 · 다음 줄을 가리지 않게) · 끄면 아래.
    Entry {
        key: "ui.tooltip_above",
        cat: Msg::CatAppearance,
        label: Msg::LblTooltipAbove,
        desc: Msg::DescTooltipAbove,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ui.busy_ring",
        cat: Msg::CatAppearance,
        label: Msg::LblBusyRing,
        desc: Msg::DescBusyRing,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "ui.busy_ring_width",
        cat: Msg::CatAppearance,
        label: Msg::LblBusyRingWidth,
        desc: Msg::DescBusyRingWidth,
        kind: SettingKind::Int { min: 1, max: 8 },
        default: "3",
    },
    Entry {
        key: "ui.busy_ring_color",
        cat: Msg::CatAppearance,
        label: Msg::LblBusyRingColor,
        desc: Msg::DescBusyRingColor,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "ui.busy_ring_lap_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblBusyRingLapMs,
        desc: Msg::DescBusyRingLapMs,
        kind: SettingKind::Int {
            min: 300,
            max: 5000,
        },
        default: "1200",
    },
    Entry {
        key: "ui.busy_ring_hold_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblBusyRingHoldMs,
        desc: Msg::DescBusyRingHoldMs,
        kind: SettingKind::Int { min: 0, max: 5000 },
        default: "600",
    },
    Entry {
        key: "ui.busy_ring_done_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblBusyRingDoneMs,
        desc: Msg::DescBusyRingDoneMs,
        kind: SettingKind::Int { min: 0, max: 10000 },
        default: "0",
    },
    Entry {
        key: "tabs.rows",
        cat: Msg::CatTabs,
        label: Msg::LblTabsRows,
        desc: Msg::DescTabsRows,
        kind: SettingKind::Choice(TAB_ROWS_OPTS),
        default: "multi",
    },
    Entry {
        key: "tabs.tooltip",
        cat: Msg::CatTabs,
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
        key: "meta.refresh_idle_ms",
        cat: Msg::CatExplorer,
        label: Msg::LblMetaRefreshIdle,
        desc: Msg::DescMetaRefreshIdle,
        kind: SettingKind::Int {
            min: 0,
            max: 600_000,
        },
        default: "5000",
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
    // ★ 용량 표시(사용자 09-30 · DBeaver식): 테이블·MV·인덱스 폴더가 읽힐 때 스키마·종류당 질의 1 · 향상 모드 끔(perf::BOOST).
    Entry {
        key: "explorer.sizes",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerSizes,
        desc: Msg::DescExplorerSizes,
        kind: SettingKind::Bool,
        default: "on",
    },
    // T-249(09-29 · 93 §6에서 뺐던 4키 중 둘 — 기능과 함께 재등록): 노드 툴팁 · 노드 로드 타임아웃.
    Entry {
        key: "explorer.tooltip",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerTooltip,
        desc: Msg::DescExplorerTooltip,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "explorer.timeout",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerTimeout,
        desc: Msg::DescExplorerTimeout,
        kind: SettingKind::Int { min: 0, max: 600 },
        default: "15",
    },
    Entry {
        key: "explorer.disconnect_pick",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerDisconnectPick,
        desc: Msg::DescExplorerDisconnectPick,
        kind: SettingKind::Choice(DISC_PICK_OPTS),
        // 기본 = 항상 고르기(사용자 09-28 · D-144) — 연결이 하나여도 "모두/이 연결"을 확인하고 끊는다.
        default: "always",
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
    // ★ 객체 삭제 전 DDL 파일 백업(10-01 · 사용자 "기본은 파일로 백업 후 삭제 켬 · 끄면 백업 로직 사용 안 함").
    Entry {
        key: "explorer.drop_backup",
        cat: Msg::CatExplorer,
        label: Msg::LblDropBackup,
        desc: Msg::DescDropBackup,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 소스 열기 머리 줄에 소유 스키마(09-30 · 사용자 "원본의 스키마를 붙여서 생성하는 걸 기본값으로").
    Entry {
        key: "explorer.source_schema",
        cat: Msg::CatExplorer,
        label: Msg::LblSourceSchema,
        desc: Msg::DescSourceSchema,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 101(10-01): SQL Server 탐색기 골격 = SSMS(Databases 층) · 시스템 DB 폴더.
    Entry {
        key: "explorer.mssql_tree",
        cat: Msg::CatExplorer,
        label: Msg::LblMssqlTree,
        desc: Msg::DescMssqlTree,
        kind: SettingKind::Choice(MSSQL_TREE_OPTS),
        default: "ssms",
    },
    Entry {
        key: "explorer.mssql_system_dbs",
        cat: Msg::CatExplorer,
        label: Msg::LblMssqlSystemDbs,
        desc: Msg::DescMssqlSystemDbs,
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
    // ★ 생성 SQL의 바인드 변수 목록을 머리/꼬리 주석으로(사용자 09-28 "변수들을 직관적으로 식별 가능하게 header/tail에 표시").
    Entry {
        key: "gen.bind_note",
        cat: Msg::CatExplorer,
        label: Msg::LblGenBindNote,
        desc: Msg::DescGenBindNote,
        kind: SettingKind::Choice(BIND_NOTE_OPTS),
        default: "both",
    },
    Entry {
        key: "explorer.keep_offline",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerKeepOffline,
        desc: Msg::DescExplorerKeepOffline,
        kind: SettingKind::Bool,
        default: "off",
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
        key: "explorer.typeahead_timeout_ms",
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
    // ★ 고속 스크롤(사용자 09-30 · 8영역 공통 = 편집기·결과·객체 탐색기·검색 결과·프로젝트·북마크·아웃라인·확장): 같은 방향의
    //   ↑/↓ 자동 반복·휠 틱이 짧은 간격으로 이어지면 이동량 배수(nexa-ctl `FastScroll` 전역 + `ScrollAccel`/`SpeedHud` · 관성 없음).
    Entry {
        key: "scroll.fast",
        cat: Msg::CatScroll,
        label: Msg::LblScrollFast,
        desc: Msg::DescScrollFast,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "scroll.fast_speed",
        cat: Msg::CatScroll,
        label: Msg::LblScrollFastSpeed,
        desc: Msg::DescScrollFastSpeed,
        kind: SettingKind::Choice(SCROLL_SPEED_OPTS),
        default: "fast",
    },
    Entry {
        key: "scroll.fast_grid_extra",
        cat: Msg::CatScroll,
        label: Msg::LblScrollFastGridExtra,
        desc: Msg::DescScrollFastGridExtra,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "scroll.fast_hud",
        cat: Msg::CatScroll,
        label: Msg::LblScrollFastHud,
        desc: Msg::DescScrollFastHud,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "scroll.fast_hud_pos",
        cat: Msg::CatScroll,
        label: Msg::LblScrollFastHudPos,
        desc: Msg::DescScrollFastHudPos,
        kind: SettingKind::Position,
        default: "top_right",
    },
    Entry {
        key: "scroll.fast_hud_fade_ms",
        cat: Msg::CatScroll,
        label: Msg::LblScrollFastHudFadeMs,
        desc: Msg::DescScrollFastHudFadeMs,
        kind: SettingKind::Int {
            min: 50,
            max: 10_000,
        },
        default: "600",
    },
    // 비노출(HIDDEN · 구현 상수): HUD 유지 시간 · 연속 판정 간격.
    Entry {
        key: "scroll.fast_hud_hold_ms",
        cat: Msg::CatScroll,
        label: Msg::LblScrollFastHudHoldMs,
        desc: Msg::DescScrollFastHudHoldMs,
        kind: SettingKind::Int {
            min: 0,
            max: 10_000,
        },
        default: "250",
    },
    Entry {
        key: "scroll.fast_window_ms",
        cat: Msg::CatScroll,
        label: Msg::LblScrollFastWindowMs,
        desc: Msg::DescScrollFastWindowMs,
        kind: SettingKind::Int { min: 20, max: 2000 },
        default: "160",
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
    // 탭 구분별 활성 상단 줄 색(09-22 · 09-28 사용자 "저장된/미저장/미리보기 구분별 식별선" · 빈 값 = 기본: 미저장 warn · 저장된 파일
    //   accent · 미리보기 text_dim). 미저장 = 파일이 아닌 스크립트 탭 + 저장 뒤 바뀐 파일 탭.
    Entry {
        key: "editor.tab_line_unsaved",
        cat: Msg::CatTabs,
        label: Msg::LblEditorTabLineUnsaved,
        desc: Msg::DescEditorTabLineUnsaved,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.tab_line_file",
        cat: Msg::CatTabs,
        label: Msg::LblEditorTabLineFile,
        desc: Msg::DescEditorTabLineFile,
        kind: SettingKind::Text,
        default: "",
    },
    Entry {
        key: "editor.tab_line_preview",
        cat: Msg::CatTabs,
        label: Msg::LblEditorTabLinePreview,
        desc: Msg::DescEditorTabLinePreview,
        kind: SettingKind::Text,
        default: "",
    },
    // ★ 미저장 탭 = 탭 **이름 글자**에도 색(사용자 09-28 "포커스가 없을 때는 상단 줄로 구분이 안 된다") · 기본 켬 · 색은 빈 값 = 줄 색.
    Entry {
        key: "editor.tab_unsaved_text",
        cat: Msg::CatTabs,
        label: Msg::LblEditorTabUnsavedText,
        desc: Msg::DescEditorTabUnsavedText,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 탭 닫기 상자 표시(사용자 09-28): 기본 = 늘 보임 · hover = 탭 클릭(활성)·마우스 오버·미저장 탭에서만. 편집기 탭·결과 탭 공통.
    Entry {
        key: "editor.tab_close_show",
        cat: Msg::CatTabs,
        label: Msg::LblEditorTabCloseShow,
        desc: Msg::DescEditorTabCloseShow,
        kind: SettingKind::Choice(TAB_CLOSE_SHOW_OPTS),
        default: "always",
    },
    // ★ 탭 닫힘 뒤 포커스(사용자 10-07): 미리보기/확장(뷰) 탭 닫기 = 유지 기본(탐색을 이어 가는 뜻) · 편집기 탭 닫기 = 이동 기본.
    Entry {
        key: "tabs.close_preview_focus",
        cat: Msg::CatTabs,
        label: Msg::LblTabsClosePreviewFocus,
        desc: Msg::DescTabsClosePreviewFocus,
        kind: SettingKind::Choice(TAB_CLOSE_FOCUS_OPTS),
        default: "keep",
    },
    Entry {
        key: "tabs.close_editor_focus",
        cat: Msg::CatTabs,
        label: Msg::LblTabsCloseEditorFocus,
        desc: Msg::DescTabsCloseEditorFocus,
        kind: SettingKind::Choice(TAB_CLOSE_FOCUS_OPTS),
        default: "move",
    },
    // ★ 탭 이동 기록(사용자 10-07 · 메모리만): 켬 · 상한 · 닫힘 뒤 선택에 적용 · (안 쓸 때) 이전/다음 — `DEPENDS`로 잠금.
    Entry {
        key: "tabs.history",
        cat: Msg::CatTabs,
        label: Msg::LblTabsHistory,
        desc: Msg::DescTabsHistory,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "tabs.history_max",
        cat: Msg::CatTabs,
        label: Msg::LblTabsHistoryMax,
        desc: Msg::DescTabsHistoryMax,
        kind: SettingKind::Int { min: 2, max: 200 },
        default: "20",
    },
    Entry {
        key: "tabs.close_use_history",
        cat: Msg::CatTabs,
        label: Msg::LblTabsCloseUseHistory,
        desc: Msg::DescTabsCloseUseHistory,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "tabs.close_select",
        cat: Msg::CatTabs,
        label: Msg::LblTabsCloseSelect,
        desc: Msg::DescTabsCloseSelect,
        kind: SettingKind::Choice(TAB_CLOSE_SELECT_OPTS),
        default: "next",
    },
    Entry {
        key: "editor.tab_unsaved_color",
        cat: Msg::CatTabs,
        label: Msg::LblEditorTabUnsavedColor,
        desc: Msg::DescEditorTabUnsavedColor,
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
    // ★ 감시된 변경(탭·본문·활성 탭·폴더·선택)은 최초 변경 + 이 시간에 1회 저장(사용자 09-28 · 70 §7 · 09-30 ms 단위).
    Entry {
        key: "project.autosave_change_ms",
        cat: Msg::CatProject,
        label: Msg::LblProjectAutosaveChangeMs,
        desc: Msg::DescProjectAutosaveChangeMs,
        kind: SettingKind::Int {
            min: 100,
            max: 3_600_000,
        },
        default: "10000",
    },
    // 미저장 본문 보관 상한 = 탭 하나 기준(사용자 09-30 · 프로젝트 파일 본문 + 백업 스냅숏 공통 · 종전 상수 1 MB/8 MB) · 0 = 무제한.
    Entry {
        key: "project.unsaved_max_mb",
        cat: Msg::CatProject,
        label: Msg::LblProjectUnsavedMaxMb,
        desc: Msg::DescProjectUnsavedMaxMb,
        kind: SettingKind::Int { min: 0, max: 4096 },
        default: "8",
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
    // ★ D-259/260(10-07 · 105 §3): 색인·검색에서 건너뛸 폴더 이름(쉼표) — 빌드 산출물·VCS가 열거의 대부분(협업 실측 target/ 수십만).
    // ★ 파일 색인 TTL(T-299 ③ · 10-07): 이 시간 안에 다시 열면 재열거 생략 · 0 = 열 때마다 · 앱이 아는 변경(새 파일 저장 · 제외 변경)은 즉시 낡음.
    Entry {
        key: "project.index_ttl_secs",
        cat: Msg::CatProject,
        label: Msg::LblProjectIndexTtl,
        desc: Msg::DescProjectIndexTtl,
        kind: SettingKind::Int { min: 0, max: 3600 },
        default: "60",
    },
    // ★ 폴더 변경 감시(T-293 · D-263 · 10-07): 켬 · 묶음 시간 · 성능 프로필 low = 끔(39 §3 부하원 = 스레드 루트+1 · 유휴 CPU 0).
    Entry {
        key: "project.watch",
        cat: Msg::CatProject,
        label: Msg::LblProjectWatch,
        desc: Msg::DescProjectWatch,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "project.watch_debounce_ms",
        cat: Msg::CatProject,
        label: Msg::LblProjectWatchDebounce,
        desc: Msg::DescProjectWatchDebounce,
        kind: SettingKind::Int { min: 50, max: 5000 },
        default: "400",
    },
    Entry {
        key: "project.exclude",
        cat: Msg::CatProject,
        label: Msg::LblProjectExclude,
        desc: Msg::DescProjectExclude,
        kind: SettingKind::Text,
        default: ".git,.svn,.hg,node_modules,target,.nsql",
    },
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
        cat: Msg::CatLargeFiles,
        label: Msg::LblLargeL1Mb,
        desc: Msg::DescLargeL1,
        kind: SettingKind::Int { min: 0, max: 4096 },
        default: "5",
    },
    Entry {
        key: "file.large_l1_lines",
        cat: Msg::CatLargeFiles,
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
        cat: Msg::CatLargeFiles,
        label: Msg::LblLargeL2Mb,
        desc: Msg::DescLargeL2,
        kind: SettingKind::Int { min: 0, max: 4096 },
        default: "20",
    },
    Entry {
        key: "file.large_l2_lines",
        cat: Msg::CatLargeFiles,
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
        cat: Msg::CatLargeFiles,
        label: Msg::LblLargeExtLevel,
        desc: Msg::DescLargeExtLevel,
        kind: SettingKind::Choice(LARGE_LEVEL_OPTS),
        default: "l1",
    },
    Entry {
        key: "file.large_syntax_level",
        cat: Msg::CatLargeFiles,
        label: Msg::LblLargeSyntaxLevel,
        desc: Msg::DescLargeSyntaxLevel,
        kind: SettingKind::Choice(LARGE_LEVEL_OPTS),
        default: "l2",
    },
    Entry {
        key: "file.large_ask_mb",
        cat: Msg::CatLargeFiles,
        label: Msg::LblLargeAskMb,
        desc: Msg::DescLargeAskMb,
        kind: SettingKind::Int { min: 0, max: 65536 },
        default: "50",
    },
    Entry {
        key: "file.large_head_mb",
        cat: Msg::CatLargeFiles,
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
        cat: Msg::CatLargeFiles,
        label: Msg::LblAsyncLoadMb,
        desc: Msg::DescAsyncLoadMb,
        kind: SettingKind::Int { min: 1, max: 4096 },
        default: "8",
    },
    Entry {
        key: "file.load_progress_ms",
        cat: Msg::CatLargeFiles,
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
    // ★ T-300(사용자 10-07): 외부 변경이 본문에 반영될 때 토스트로 알린다(변경 없는 탭 자동 채택 · 다시 읽기 · 자동 병합).
    Entry {
        key: "file.external_notify",
        cat: Msg::CatFiles,
        label: Msg::LblExtNotify,
        desc: Msg::DescExtNotify,
        kind: SettingKind::Bool,
        default: "on",
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
    // ★ 가린 입력란(비밀번호) 입력 언어 안내(3-OS · `imehint.rs`) + Linux 전용 종속 = ibus 패널 구독(`imewatch.rs` · 09-27).
    //   Windows/macOS는 OS 키보드 레이아웃 API로 바로 판정하므로 구독 설정이 없다(사용자 09-28 "이름과 설명 확인").
    Entry {
        key: "input.ime_hint",
        cat: Msg::CatInput,
        label: Msg::LblImeHint,
        desc: Msg::DescImeHint,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "input.ime_hint_watch",
        cat: Msg::CatInput,
        label: Msg::LblImeHintWatch,
        desc: Msg::DescImeHintWatch,
        kind: SettingKind::Bool,
        default: "on",
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
    // ★ 프로젝트 자동 저장 표식(사용자 09-28): 켜짐/꺼짐 · 클릭 = 자동 저장 폴더(프로젝트 파일 폴더 · 폴더 모드 = `.nsql`)를 OS 탐색기로.
    Entry {
        key: "statusbar.autosave",
        cat: Msg::CatWindow,
        label: Msg::LblStatusAutosave,
        desc: Msg::DescStatusAutosave,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ★ 상태바 오른쪽 항목의 순서·표시(사용자 10-04 · nexa-ctl `order` 문법 `tx:1|mem:0|…` · 빈 값 = 기본 · 편집 = 설정 창 [편집…] → 조정 창).
    Entry {
        key: "statusbar.layout",
        cat: Msg::CatWindow,
        label: Msg::LblStatusLayout,
        desc: Msg::DescStatusLayout,
        kind: SettingKind::Text,
        default: "",
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
        cat: Msg::CatServerStatus,
        label: Msg::LblProbeEnabled,
        desc: Msg::DescProbeEnabled,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "probe.max_retries",
        cat: Msg::CatServerStatus,
        label: Msg::LblProbeMaxRetries,
        desc: Msg::DescProbeMaxRetries,
        kind: SettingKind::Int { min: 0, max: 20 },
        default: "5",
    },
    Entry {
        key: "probe.timeout_ms",
        cat: Msg::CatServerStatus,
        label: Msg::LblProbeTimeout,
        desc: Msg::DescProbeTimeout,
        kind: SettingKind::Int {
            min: 100,
            max: 30_000,
        },
        default: "2000",
    },
    // T-249: 이름 풀이 캐시(HIDDEN · 39 §DNS 재풀이).
    Entry {
        key: "probe.dns_cache_secs",
        cat: Msg::CatServerStatus,
        label: Msg::LblProbeDnsCache,
        desc: Msg::DescProbeDnsCache,
        kind: SettingKind::Int {
            min: 0,
            max: 86_400,
        },
        default: "300",
    },
    Entry {
        key: "probe.interval",
        cat: Msg::CatServerStatus,
        label: Msg::LblProbeInterval,
        desc: Msg::DescProbeInterval,
        kind: SettingKind::Int { min: 5, max: 3600 },
        default: "60",
    },
    Entry {
        key: "probe.retry_delay_ms",
        cat: Msg::CatServerStatus,
        label: Msg::LblProbeRetryDelay,
        desc: Msg::DescProbeRetryDelay,
        // 하한 5초 유지 = 네트워크 부하 규칙(26 §8 · 빠른 재시도 없음).
        kind: SettingKind::Int {
            min: 5000,
            max: 3_600_000,
        },
        default: "10000",
    },
    // ── 비노출 설정(사용자 09-14 "자주 바꾸지 않을 값은 비노출 설정으로") — 구현 상수의 설정화. `nsql config list all`로만 보인다.
    Entry {
        key: "login.delete_confirm_ms",
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
        key: "login.close_after_connect_ms",
        cat: Msg::CatConnection,
        label: Msg::LblCloseAfterConnectMs,
        desc: Msg::DescCloseAfterConnectMs,
        kind: SettingKind::Int { min: 0, max: 5000 },
        default: "450",
    },
    Entry {
        key: "login.window_w",
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
        key: "login.window_h",
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
        key: "login.panel_w",
        cat: Msg::CatConnection,
        label: Msg::LblConnPanelW,
        desc: Msg::DescConnPanelW,
        kind: SettingKind::Int { min: 200, max: 800 },
        default: "292",
    },
    Entry {
        key: "login.port_w",
        cat: Msg::CatConnection,
        label: Msg::LblConnPortW,
        desc: Msg::DescConnPortW,
        kind: SettingKind::Int { min: 40, max: 160 },
        default: "72",
    },
    Entry {
        key: "login.button_scale_pct",
        cat: Msg::CatConnection,
        label: Msg::LblConnButtonScale,
        desc: Msg::DescConnButtonScale,
        kind: SettingKind::Int { min: 100, max: 250 },
        default: "132",
    },
    Entry {
        key: "ui.toast_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblToastMs,
        desc: Msg::DescToastMs,
        kind: SettingKind::Int {
            min: 100,
            max: 60_000,
        },
        default: "3000",
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
    // ★ 버튼 빠른 두 번 누름 차단(09-30 · 사용자): 같은 버튼·툴바 항목이 이 시간 안에 두 번 눌리면 한 번만. 0 = 끔.
    Entry {
        key: "ui.click_guard_ms",
        cat: Msg::CatAppearance,
        label: Msg::LblClickGuardMs,
        desc: Msg::DescClickGuardMs,
        kind: SettingKind::Int { min: 0, max: 2000 },
        default: "350",
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
        cat: Msg::CatServerStatus,
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
        cat: Msg::CatRunCards,
        label: Msg::LblRunToast,
        desc: Msg::DescRunToast,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "run.toast_hide_ms",
        cat: Msg::CatRunCards,
        label: Msg::LblRunToastHide,
        desc: Msg::DescRunToastHide,
        kind: SettingKind::Int {
            min: 0,
            max: 600_000,
        },
        default: "5000",
    },
    // ★ 실행 카드 갱신 주기(사용자 09-22): 경과 시간 `HH:MM:SS.mmm`·카운트다운을 이 주기로만 다시 그린다(향상 모드 = 1000).
    Entry {
        key: "run.toast_tick_ms",
        cat: Msg::CatRunCards,
        label: Msg::LblRunToastTick,
        desc: Msg::DescRunToastTick,
        kind: SettingKind::Int { min: 30, max: 5000 },
        default: "100",
    },
    Entry {
        key: "run.toast_follow",
        cat: Msg::CatRunCards,
        label: Msg::LblRunToastFollow,
        desc: Msg::DescRunToastFollow,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "run.toast_max",
        cat: Msg::CatRunCards,
        label: Msg::LblRunToastMax,
        desc: Msg::DescRunToastMax,
        kind: SettingKind::Int { min: 1, max: 500 },
        default: "30",
    },
    // ── ★ Output 탭(09-30 · 사용자 "Output 탭 개념 · 처음엔 없고 · 보기 설정 · 메시지 오면 자동"): 결과 영역의 메시지 탭.
    //    서버 메시지(DBMS_OUTPUT · T-SQL PRINT · RAISE NOTICE) · SQL*Plus PRINT · 컴파일 결과 · 경고 · 오류 · 문장 완료 줄.
    Entry {
        key: "output.show",
        cat: Msg::CatOutput,
        label: Msg::LblOutputShow,
        desc: Msg::DescOutputShow,
        kind: SettingKind::Choice(OUTPUT_SHOW_OPTS),
        default: "auto",
    },
    Entry {
        key: "output.activate",
        cat: Msg::CatOutput,
        label: Msg::LblOutputActivate,
        desc: Msg::DescOutputActivate,
        kind: SettingKind::Choice(OUTPUT_ACT_OPTS),
        default: "no_results",
    },
    Entry {
        key: "output.done_lines",
        cat: Msg::CatOutput,
        label: Msg::LblOutputDoneLines,
        desc: Msg::DescOutputDoneLines,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "output.timestamps",
        cat: Msg::CatOutput,
        label: Msg::LblOutputTimestamps,
        desc: Msg::DescOutputTimestamps,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "output.max_lines",
        cat: Msg::CatOutput,
        label: Msg::LblOutputMaxLines,
        desc: Msg::DescOutputMaxLines,
        kind: SettingKind::Int {
            min: 100,
            max: 100_000,
        },
        default: "5000",
    },
    // D-239(09-30 · 권장안 적용 · 한 줄 고지): Oracle DBMS_OUTPUT은 접속 때 켠다(실행마다 GET_LINES 1회 · 39 §3 부하원 · 끌 수 있다).
    Entry {
        key: "output.serveroutput",
        cat: Msg::CatOutput,
        label: Msg::LblOutputServerOutput,
        desc: Msg::DescOutputServerOutput,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "run.after_statement",
        cat: Msg::CatRunCards,
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
        cat: Msg::CatServerStatus,
        label: Msg::LblProbeStale,
        desc: Msg::DescProbeStale,
        kind: SettingKind::Int { min: 0, max: 86400 },
        default: "60",
    },
    Entry {
        key: "net.keepalive_secs",
        cat: Msg::CatServerStatus,
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
        // 기본 = SSD 기준 8 MB(한 파일 읽기 ≈ 16 ms @ 500 MB/s) · 향상 모드 = HDD 기준 2 MB(≈ 17 ms @ 120 MB/s + 탐색) — docs/72 §4-1(09-28).
        default: "8192",
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
        cat: Msg::CatTxSafety,
        label: Msg::LblTxStaleMin,
        desc: Msg::DescTxStaleMin,
        kind: SettingKind::Int { min: 1, max: 1440 },
        default: "10",
    },
    // 수동 커밋 잠금 방지(docs/56 · 사용자 09-19 "자동 처리의 값·조건은 전부 설정으로"): L1 읽기 트랜잭션 자동 종료 ·
    //   L2 유휴 미커밋 경고/재알림/자동 동작/카운트다운 · L3 막힘 감지 주기 · L4 서버 안전망 세션 파라미터.
    Entry {
        key: "tx.read_end",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxReadEnd,
        desc: Msg::DescTxReadEnd,
        kind: SettingKind::Choice(TX_READ_END_OPTS),
        default: "auto",
    },
    Entry {
        key: "tx.remind_min",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxRemind,
        desc: Msg::DescTxRemind,
        kind: SettingKind::Int { min: 0, max: 1440 },
        default: "10",
    },
    Entry {
        key: "tx.idle_action",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxIdleAction,
        desc: Msg::DescTxIdleAction,
        kind: SettingKind::Choice(TX_IDLE_ACTION_OPTS),
        default: "rollback",
    },
    Entry {
        key: "tx.idle_limit_min",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxIdleLimit,
        desc: Msg::DescTxIdleLimit,
        kind: SettingKind::Int { min: 1, max: 1440 },
        default: "30",
    },
    Entry {
        key: "tx.idle_countdown_secs",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxIdleCountdown,
        desc: Msg::DescTxIdleCountdown,
        kind: SettingKind::Int { min: 5, max: 600 },
        default: "60",
    },
    // ── 접속 유형(운영) 기준(docs/56 §4 2차) — 전역 값과 비교해 더 엄격한 쪽이 적용된다.
    Entry {
        key: "tx.prod_stale_min",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxProdStale,
        desc: Msg::DescTxProdStale,
        kind: SettingKind::Int { min: 1, max: 240 },
        default: "5",
    },
    Entry {
        key: "tx.prod_idle_limit_min",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxProdLimit,
        desc: Msg::DescTxProdLimit,
        kind: SettingKind::Int { min: 1, max: 600 },
        default: "10",
    },
    Entry {
        key: "run.prod_confirm",
        cat: Msg::CatTxSafety,
        label: Msg::LblRunProdConfirm,
        desc: Msg::DescRunProdConfirm,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "tx.block_poll_secs",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxBlockPoll,
        desc: Msg::DescTxBlockPoll,
        kind: SettingKind::Int { min: 0, max: 3600 },
        default: "30",
    },
    Entry {
        key: "tx.server_idle_timeout_secs",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxServerIdle,
        desc: Msg::DescTxServerIdle,
        kind: SettingKind::Int { min: 0, max: 86400 },
        default: "0",
    },
    Entry {
        key: "tx.lock_wait_timeout_secs",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxLockWait,
        desc: Msg::DescTxLockWait,
        kind: SettingKind::Int { min: 0, max: 3600 },
        default: "0",
    },
    // ★ 식별자 인용 정책(사용자 10-07 "항상 감쌀지 필요할 때만인지 편집기 설정에 · 두 경로 동일 기준 · DBMS별 표현").
    Entry {
        key: "editor.quote_idents",
        cat: Msg::CatEditor,
        label: Msg::LblQuoteIdents,
        desc: Msg::DescQuoteIdents,
        kind: SettingKind::Choice(QUOTE_IDENTS_OPTS),
        default: "needed",
    },
    Entry {
        key: "editor.close_unsaved",
        cat: Msg::CatTabs,
        label: Msg::LblCloseUnsaved,
        desc: Msg::DescCloseUnsaved,
        kind: SettingKind::Choice(CLOSE_UNSAVED_OPTS),
        default: "ask",
    },
    // ★ 전체 선택 뒤 화면 위치(09-30 · 사용자 "기본은 현재 위치 유지").
    Entry {
        key: "editor.select_all_view",
        cat: Msg::CatEditor,
        label: Msg::LblSelectAllView,
        desc: Msg::DescSelectAllView,
        kind: SettingKind::Choice(SELECT_ALL_OPTS),
        default: "keep",
    },
    Entry {
        key: "tx.close_action",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxCloseAction,
        desc: Msg::DescTxCloseAction,
        kind: SettingKind::Choice(TX_CLOSE_OPTS),
        default: "ask",
    },
    Entry {
        key: "tx.badge",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxBadge,
        desc: Msg::DescTxBadge,
        kind: SettingKind::Choice(TX_BADGE_OPTS),
        default: "count",
    },
    Entry {
        key: "tx.smart_commit",
        cat: Msg::CatTxSafety,
        label: Msg::LblTxSmartCommit,
        desc: Msg::DescTxSmartCommit,
        kind: SettingKind::Bool,
        default: "off",
    },
    // D-139: OUT 바인드가 없는 DBMS의 `EXEC SELECT … INTO` — 0행·여러 행 = 오류(oracle) / 첫 행(first).
    Entry {
        key: "vars.into_policy",
        cat: Msg::CatScriptVars,
        label: Msg::LblVarsIntoPolicy,
        desc: Msg::DescVarsIntoPolicy,
        kind: SettingKind::Choice(VARS_INTO_OPTS),
        default: "oracle",
    },
    // 부하원 스위치(39 §3 · 09-21): 호출 서명 조회(루틴당 카탈로그 질의 1회) · PG 커서 이름 풀기(결과마다 확인 왕복 1회).
    // 둘 다 **결과에 영향을 주므로** 향상 모드(`perf::BOOST`)에는 넣지 않는다 — 끄는 것은 사용자의 선택.
    Entry {
        key: "vars.signature_lookup",
        cat: Msg::CatScriptVars,
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
        cat: Msg::CatScriptVars,
        label: Msg::LblVarsBraceSubst,
        desc: Msg::DescVarsBraceSubst,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "vars.env_subst",
        cat: Msg::CatScriptVars,
        label: Msg::LblVarsEnvSubst,
        desc: Msg::DescVarsEnvSubst,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "vars.intrinsic",
        cat: Msg::CatScriptVars,
        label: Msg::LblVarsIntrinsic,
        desc: Msg::DescVarsIntrinsic,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "vars.expand_at",
        cat: Msg::CatScriptVars,
        label: Msg::LblVarsExpandAt,
        desc: Msg::DescVarsExpandAt,
        kind: SettingKind::Choice(VARS_EXPAND_OPTS),
        default: "assign",
    },
    Entry {
        key: "vars.max_value_kb",
        cat: Msg::CatScriptVars,
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
        cat: Msg::CatScriptVars,
        label: Msg::LblVarsGlobalPersist,
        desc: Msg::DescVarsGlobalPersist,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "vars.persist",
        cat: Msg::CatScriptVars,
        label: Msg::LblVarsPersist,
        desc: Msg::DescVarsPersist,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "vars.persist_days",
        cat: Msg::CatScriptVars,
        label: Msg::LblVarsPersistDays,
        desc: Msg::DescVarsPersistDays,
        kind: SettingKind::Int { min: 0, max: 3650 },
        default: "90",
    },
    // D-137: 값이 없는 채로 읽히는 바인드·미정의 `&` = 실행당 한 번 묻기(prompt) / 말없이 NULL·빈 글(auto · 종전) / 오류(error).
    Entry {
        key: "vars.undeclared",
        cat: Msg::CatScriptVars,
        label: Msg::LblVarsUndeclared,
        desc: Msg::DescVarsUndeclared,
        kind: SettingKind::Choice(VARS_UNDECLARED_OPTS),
        default: "prompt",
    },
    // REF CURSOR 자동 표시(09-21): `VAR rc REFCURSOR` + `EXEC proc(:rc)` 뒤 커서를 바로 결과 탭으로(둘 이상이면 각각) · 끄면 `PRINT rc`.
    Entry {
        key: "run.cursor_autoshow",
        cat: Msg::CatScriptVars,
        label: Msg::LblCursorAutoshow,
        desc: Msg::DescCursorAutoshow,
        kind: SettingKind::Bool,
        default: "on",
    },
    // ── 스크립트 엔진 엄격 모드(T-9 · 09-16) — 배치에서 미정의 &var·암묵 :bind를 오류로. 자주 안 바꾸므로 HIDDEN(`nsql config list all`).
    // ★ 라이선스 게이트(docs/23 §4-2 · T-36 · 09-27 · ★ D-145 09-28): **기본 끔** — 개인 사용은 라이선스 없이 전 기능(사용자 09-28
    //   "개인 사용인 경우 모든 기능 오픈 · 차후 일부 제한으로 바뀔 수 있다"). 켜면 Debug·Release 모두 Pro/Org 게이트 12곳 적용(게이트 시험 =
    //   격리 홈에서 on). 종전 `license.gates_dev`(Debug 전용 · Release 늘 켬)에서 이름·뜻이 바뀌었다(RENAMED). HIDDEN.
    Entry {
        key: "license.gates",
        // ★ 고급 ▸ 내부(사용자 10-07 "개발자/사용자가 볼 수 없어야") — 설정 창에는 고급을 켜도 안 보인다(`INTERNAL_CATEGORIES`).
        cat: Msg::CatInternal,
        label: Msg::LblLicenseGatesDev,
        desc: Msg::DescLicenseGatesDev,
        kind: SettingKind::Bool,
        default: "off",
    },
    Entry {
        key: "script.strict",
        cat: Msg::CatScriptVars,
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
    // 데모(사용자 09-17): 최초 실행 1회 "샘플 데이터(Demo) 만들까요?" 팝업을 띄웠는가(자동 기억 · HIDDEN).
    Entry {
        key: "demo.prompted",
        // ★ 고급 ▸ 기억된 상태(사용자 10-07 "별도 그룹으로") — 앱이 스스로 적는 값.
        cat: Msg::CatRemembered,
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
        key: "clipboard.x11_native",
        cat: Msg::CatPerformance,
        label: Msg::LblClipX11Native,
        desc: Msg::DescClipX11Native,
        kind: SettingKind::Bool,
        // 기본 = 켬: Linux 클립보드를 X11 selection으로 **앱이 직접**(외부 wl-copy/xclip 없이 · 09-26). 끄면 종전 CLI 경로.
        default: "on",
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
        cat: Msg::CatUndo,
        label: Msg::LblUndoBudget,
        desc: Msg::DescUndoBudget,
        kind: SettingKind::Int { min: 1, max: 4096 },
        default: "64",
    },
    Entry {
        key: "editor.undo_group_ms",
        cat: Msg::CatUndo,
        label: Msg::LblUndoGroupMs,
        desc: Msg::DescUndoGroupMs,
        kind: SettingKind::Int { min: 0, max: 60000 },
        default: "1500",
    },
    Entry {
        key: "editor.undo_persist",
        cat: Msg::CatUndo,
        label: Msg::LblUndoPersist,
        desc: Msg::DescUndoPersist,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "editor.undo_persist_mb",
        cat: Msg::CatUndo,
        label: Msg::LblUndoPersistMb,
        desc: Msg::DescUndoPersistMb,
        kind: SettingKind::Int { min: 1, max: 64 },
        default: "4",
    },
    Entry {
        key: "editor.undo_persist_days",
        cat: Msg::CatUndo,
        label: Msg::LblUndoPersistDays,
        desc: Msg::DescUndoPersistDays,
        kind: SettingKind::Int { min: 0, max: 3650 },
        default: "30",
    },
    Entry {
        key: "editor.undo_giant_mb",
        cat: Msg::CatUndo,
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
        // ★ 기본 = 편집기(활성 탭)의 들여쓰기 단위를 그대로(사용자 09-29).
        default: "editor",
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
        key: "intel.alias_style",
        cat: Msg::CatIntel,
        label: Msg::LblIntelAliasStyle,
        desc: Msg::DescIntelAliasStyle,
        kind: SettingKind::Choice(INTEL_ALIAS_STYLE),
        default: "abbr",
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
        key: "intel.join_fk",
        cat: Msg::CatIntel,
        label: Msg::LblIntelJoinFk,
        desc: Msg::DescIntelJoinFk,
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
        key: "intel.signature_card",
        cat: Msg::CatIntel,
        label: Msg::LblIntelSignatureCard,
        desc: Msg::DescIntelSignatureCard,
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
        cat: Msg::CatUndo,
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
        key: "probe.icmp",
        cat: Msg::CatServerStatus,
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
    // T-255(09-29): 결과 탭 "Copy SQL"을 기본 포맷터로 정돈해 복사.
    Entry {
        key: "grid.copy_sql_format",
        cat: Msg::CatGrid,
        label: Msg::LblGridCopySqlFormat,
        desc: Msg::DescGridCopySqlFormat,
        kind: SettingKind::Bool,
        default: "off",
    },
    // T-249 / T-90b: 문장 타임아웃(D-61 기본 0 = 없음 · DBeaver 동일).
    Entry {
        key: "db.statement_timeout",
        cat: Msg::CatSession,
        label: Msg::LblStmtTimeout,
        desc: Msg::DescStmtTimeout,
        kind: SettingKind::Int {
            min: 0,
            max: 86_400,
        },
        default: "0",
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
    REGISTRY
        .iter()
        .find(|e| e.key == key)
        .or_else(|| dynamic_entries().into_iter().find(|e| e.key == key))
}

/// ★ 동적 항목(사용자 10-07 "모든 확장 = 설정에 사용 여부"): 컴파일 때 모르는 키(설치된 확장마다 `ext.<id>.use`)를 런타임에 등재한다.
/// 한 번 등재한 항목은 프로세스가 끝날 때까지 산다(같은 키 재등재 = 교체 · 누수 상한 = 서로 다른 키 수).
static DYNAMIC: std::sync::Mutex<Vec<&'static Entry>> = std::sync::Mutex::new(Vec::new());

/// 동적 항목 등재(같은 키는 교체). `Entry.key`는 [`ext_use_key`]처럼 interned `&'static str`이어야 한다.
pub fn register_dynamic(entries: Vec<Entry>) {
    let mut v = DYNAMIC
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for e in entries {
        if let Some(slot) = v.iter_mut().find(|x| x.key == e.key) {
            *slot = Box::leak(Box::new(e));
        } else {
            v.push(Box::leak(Box::new(e)));
        }
    }
}

/// 지금 등재된 동적 항목 전부(등재 순).
#[must_use]
pub fn dynamic_entries() -> Vec<&'static Entry> {
    DYNAMIC
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// 확장 id → 공통 설정 키 `ext.<id>.use`(`-` → `_` · interned · 같은 id = 같은 포인터).
#[must_use]
pub fn ext_use_key(id: &str) -> &'static str {
    static KEYS: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let want = format!("ext.{}.use", id.replace('-', "_"));
    let mut v = KEYS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(k) = v.iter().find(|k| **k == want) {
        return k;
    }
    let k: &'static str = Box::leak(want.into_boxed_str());
    v.push(k);
    k
}

/// 동적 확장 분류(확장 이름 분류 · [`Msg::Dyn`]) ↔ 확장 id — [`EXTENSION_CATEGORIES`]와 합쳐 [`extension_categories`]로 본다.
static DYN_EXT_CATS: std::sync::Mutex<Vec<(Msg, &'static str)>> = std::sync::Mutex::new(Vec::new());

/// 동적 확장 분류 등재(같은 id = 분류 교체).
pub fn register_extension_category(cat: Msg, id: &str) {
    let mut v = DYN_EXT_CATS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(slot) = v.iter_mut().find(|(_, i)| *i == id) {
        slot.0 = cat;
    } else {
        v.push((cat, Box::leak(id.to_string().into_boxed_str())));
    }
}

/// 확장이 소유한 분류 전부 = 정적([`EXTENSION_CATEGORIES`]) + 동적.
#[must_use]
pub fn extension_categories() -> Vec<(Msg, &'static str)> {
    let mut out: Vec<(Msg, &'static str)> = EXTENSION_CATEGORIES.to_vec();
    out.extend(
        DYN_EXT_CATS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .copied(),
    );
    out
}

/// ★ 설정 트리(DBeaver Preferences 차용 · 사용자 09-15) — 그룹 → 카테고리 순서. 설정 화면(T-39) 사이드바·`config list` 머리글의 단일 원천.
/// DBeaver: General / User Interface(Appearance·Navigator·Keys) / Editors(SQL Editor) / Connections / Data Editor(Result Sets).
pub const CATEGORY_TREE: &[(Msg, &[Msg])] = &[
    (
        Msg::GrpGeneral,
        &[
            Msg::CatLog,
            Msg::CatSession,
            // 실행 카드(알림) — 78 §3-4 쪼갬(T-250 · 09-29).
            Msg::CatRunCards,
            // Output 탭(09-30).
            Msg::CatOutput,
            Msg::CatPerformance,
        ],
    ),
    (
        Msg::GrpUserInterface,
        &[
            Msg::CatAppearance,
            Msg::CatInput,
            // 고속 스크롤(8영역 공통 · 결과 셋에서 분리 · 사용자 10-06).
            Msg::CatScroll,
            Msg::CatKeys,
            Msg::CatWindow,
            Msg::CatExplorer,
        ],
    ),
    (
        Msg::GrpEditors,
        &[
            Msg::CatEditor,
            // 탭 동작(사용자 10-07 "탭 동작 관련 설정을 모아") — 줄·툴팁·닫기 버튼·미저장 표시·줄 색·닫기 확인·닫힘 뒤 포커스.
            Msg::CatTabs,
            // 되돌리기(78 · T-250): 예산·묶음·기록 파일 — 종전 Performance/Editor에 흩어져 있던 `editor.undo_*`.
            Msg::CatUndo,
            Msg::CatIntel,
            Msg::CatFiles,
            // 큰 파일 처리(docs/59 · 사용자 09-28 "별도 설정 그룹으로") — 단계 L1/L2 · 열기 선택 · 부분 보기 · 비동기 적재.
            Msg::CatLargeFiles,
            Msg::CatFormat,
            Msg::CatObjLink,
            Msg::CatProject,
        ],
    ),
    // 접속 · 서버 상태(probe/net) · 트랜잭션 보호(tx) · 스크립트·변수(vars/script) · CLI — 78 §3-4(T-250 · 09-29).
    (
        Msg::GrpConnections,
        &[
            Msg::CatConnection,
            Msg::CatServerStatus,
            Msg::CatTxSafety,
            Msg::CatScriptVars,
            Msg::CatCli,
        ],
    ),
    (Msg::GrpDataEditor, &[Msg::CatGrid, Msg::CatGridFilter]),
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
        &[
            Msg::CatExtManager,
            Msg::CatExtRainbowPairs,
            Msg::CatExtSqlFormatter,
        ],
    ),
    // ★ 고급(사용자 10-07): 기억된 상태(앱이 스스로 적는 값 · 고급 켜면 보임) · 내부(설정 창에 **절대 안 보임** · `config list all`만).
    (Msg::GrpAdvanced, &[Msg::CatRemembered, Msg::CatInternal]),
];

/// ★ 설정 창에 **보이지 않는** 분류(사용자 10-07 `license.gates`) — 고급 스위치와 무관하게 트리·검색에서 뺀다. CLI `config list all`에는 나온다.
pub const INTERNAL_CATEGORIES: &[Msg] = &[Msg::CatInternal];

/// 내부 분류의 키인가([`INTERNAL_CATEGORIES`]).
#[must_use]
pub fn is_internal(key: &str) -> bool {
    entry(key).is_some_and(|e| INTERNAL_CATEGORIES.contains(&e.cat))
}

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
pub const EXTENSION_CATEGORIES: &[(Msg, &str)] = &[
    (Msg::CatExtRainbowPairs, "rainbow-pairs"),
    (Msg::CatExtSqlFormatter, "sql-formatter-kiros33"),
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
/// Output 탭 표시 시점(09-30).
const MSSQL_TREE_OPTS: &[(&str, Msg)] = &[
    ("ssms", Msg::ValMssqlTreeSsms),
    ("schema", Msg::ValMssqlTreeSchema),
];

const OUTPUT_SHOW_OPTS: &[(&str, Msg)] = &[
    ("off", Msg::ValOutputShowOff),
    ("auto", Msg::ValOutputShowAuto),
    ("errors", Msg::ValOutputShowErrors),
    ("always", Msg::ValOutputShowAlways),
];
/// 메시지가 왔을 때 Output 탭으로 전환하는 규칙(09-30).
const OUTPUT_ACT_OPTS: &[(&str, Msg)] = &[
    ("never", Msg::ValOutputActNever),
    ("no_results", Msg::ValOutputActNoResults),
    ("always", Msg::ValOutputActAlways),
];
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
    // 탭 이동 기록(10-07): 상한·적용은 기록이 켜져 있을 때 · 이전/다음은 **기록 적용이 꺼져 있을 때만**(사용자 "적용이 체크되면 설정 못 하게").
    ("ui.busy_ring_width", "ui.busy_ring", Dep::On),
    ("ui.busy_ring_color", "ui.busy_ring", Dep::On),
    ("ui.busy_ring_lap_ms", "ui.busy_ring", Dep::On),
    ("ui.busy_ring_hold_ms", "ui.busy_ring", Dep::On),
    ("ui.busy_ring_done_ms", "ui.busy_ring", Dep::On),
    ("project.watch_debounce_ms", "project.watch", Dep::On),
    ("tabs.history_max", "tabs.history", Dep::On),
    ("tabs.close_use_history", "tabs.history", Dep::On),
    (
        "tabs.close_select",
        "tabs.close_use_history",
        Dep::Eq("off"),
    ),
    ("scroll.fast_speed", "scroll.fast", Dep::On),
    ("scroll.fast_grid_extra", "scroll.fast", Dep::On),
    ("scroll.fast_hud", "scroll.fast", Dep::On),
    ("scroll.fast_hud_pos", "scroll.fast_hud", Dep::On),
    ("scroll.fast_hud_fade_ms", "scroll.fast_hud", Dep::On),
    ("meta.refresh_on_commit", "meta.refresh_on_ddl", Dep::On),
    ("input.ime_hint_watch", "input.ime_hint", Dep::On),
    (
        "editor.tab_unsaved_color",
        "editor.tab_unsaved_text",
        Dep::On,
    ),
    (
        "file.external_merge",
        "file.external_change",
        Dep::Eq("auto"),
    ),
    (
        "explorer.typeahead_timeout_ms",
        "explorer.typeahead",
        Dep::On,
    ),
    ("explorer.typeahead_space", "explorer.typeahead", Dep::On),
    ("explorer.typeahead_special", "explorer.typeahead", Dep::On),
    ("explorer.typeahead_pos", "explorer.typeahead", Dep::On),
    ("run.toast_hide_ms", "run.toast", Dep::On),
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
    ("probe.timeout_ms", "probe.enabled", Dep::On),
    ("probe.interval", "probe.enabled", Dep::On),
    ("probe.retry_delay_ms", "probe.enabled", Dep::On),
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
    ("ui.toast_alpha", "ui.toast_ms", Dep::NotEmpty),
    ("ui.toast_fade_to", "ui.toast_progress", Dep::On),
    ("ui.toast_bar_spent", "ui.toast_progress", Dep::On),
    ("grid.col_max_chars", "grid.col_max_mode", Dep::Eq("manual")),
    ("intel.detail_bg_alpha", "intel.detail_card", Dep::On),
    ("intel.detail_text_alpha", "intel.detail_card", Dep::On),
    ("intel.card_settle_ms", "intel.detail_card", Dep::On),
    ("format.seed_gap", "format.where_seed", Dep::On),
    (
        "format.logical_gap",
        "format.logical_newline",
        Dep::Eq("before"),
    ),
    ("format.operator_gap", "format.operator_spaces", Dep::On),
    ("format.comment_gap", "format.comment_space", Dep::On),
    (
        "ext.sqlfmt_kiros33.dup_alias",
        "ext.sqlfmt_kiros33.force_as",
        Dep::On,
    ),
    (
        "format.operator_long_space",
        "format.operator_spaces",
        Dep::On,
    ),
    ("objlink.display", "objlink.enabled", Dep::On),
    ("objlink.tooltip", "objlink.enabled", Dep::On),
    ("objlink.tooltip_pos", "objlink.tooltip", Dep::On),
    ("objlink.show_schema", "objlink.enabled", Dep::On),
    ("objlink.line_color", "objlink.enabled", Dep::On),
    ("objlink.line_width", "objlink.enabled", Dep::On),
    ("objlink.line_style", "objlink.enabled", Dep::On),
    ("objlink.bad_color", "objlink.enabled", Dep::On),
    ("objlink.bad_width", "objlink.enabled", Dep::On),
    ("objlink.bad_style", "objlink.enabled", Dep::On),
    ("objlink.max_kb", "objlink.enabled", Dep::On),
    ("intel.star_layout", "intel.insert_columns", Dep::On),
    ("intel.star_comma_space", "intel.insert_columns", Dep::On),
    ("intel.alias_style", "intel.insert_alias", Dep::On),
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
    // D-256(10-07): 값 목록 상한 폐지 — 키는 옛 설정 파일 호환으로만 남긴다(효과 없음).
    "grid.filter_values_max",
    "ui.prefs_advanced",
    "probe.dns_cache_secs",
    "license.gates",
    "ui.toast_fade_to",
    "ui.toast_bar_spent",
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
    "ext.rainbow_pairs.max_kb",
    "meta.refresh_idle_ms",
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
    "login.delete_confirm_ms",
    "login.close_after_connect_ms",
    "login.window_w",
    "login.window_h",
    "login.panel_w",
    "login.port_w",
    "login.button_scale_pct",
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
    "probe.icmp",
    "db.fetch_size",
    "db.cursor_idle_secs",
    "scroll.fast_hud_hold_ms",
    "scroll.fast_window_ms",
];

/// 비노출 설정인가.
#[must_use]
pub fn is_hidden(key: &str) -> bool {
    HIDDEN.contains(&key)
}

/// ★ **고급 설정**(사용자 09-28 "자주 수정할 만한 설정만 기본 표시 · DBMS 종속·드물게 바꾸는 설정은 Advanced로") — 설정 창의
/// Advanced 토글이 꺼져 있으면 숨기고, 켜면 키 이름을 다른 글자색으로 보인다. `nsql config list`에는 늘 나온다(비노출 [`HIDDEN`]과 다르다).
/// 판정 = ① [`HIDDEN`] ② DBMS 그룹의 카테고리 전부 ③ 이 표. 표의 기준 = 구현 상수(ms · 예산 · 상한 · 스레드 수 · 캐시 · 폴링 주기 · 재시도)와
/// 한 번 정하면 거의 손대지 않는 값. 글꼴·색·모드·켜기/끄기·초 단위 습관값(자동 저장 주기 · 유휴 초)은 기본 표시로 남긴다.
/// 표에 없는 키를 적으면 시험 `advanced_keys_exist`가 잡는다.
pub const ADVANCED: &[&str] = &[
    // 확장 뷰 탭 세션 표식(사용자 10-07 "고급 설정으로")
    "extensions.tab_badge",
    // 시간 상수(ms)
    "ui.fade_slow_ms",
    "ui.fade_fast_ms",
    "ui.flash_ms",
    "ui.flash_hold_ms",
    "ui.copy_feedback_ms",
    "run.toast_tick_ms",
    "run.toast_hide_ms",
    "meta.warm_idle_ms",
    "explorer.index_idle_ms",
    "explorer.typeahead_timeout_ms",
    "intel.delay_ms",
    "intel.budget_ms",
    "intel.card_settle_ms",
    "file.external_poll_ms",
    "tx.idle_countdown_secs",
    "tx.block_poll_secs",
    "probe.retry_delay_ms",
    "probe.timeout_ms",
    "probe.stale_secs",
    "session.call_timeout_secs",
    "net.keepalive_secs",
    "mem.trim_secs",
    "meta.detail_ttl_secs",
    "meta.cols_ttl_secs",
    // 예산·상한·개수
    "grid.memory_budget_mb",
    "grid.result_tabs_max",
    "grid.col_min_width",
    "editor.undo_budget_mb",
    "editor.split_max",
    "editor.copy_confirm_mb",
    "run.toast_max",
    "log.max_lines",
    "log.file_max_kb",
    "txlog.max_entries",
    "vars.max_value_kb",
    "vars.persist_days",
    "intel.max_items",
    "intel.max_doc_kb",
    "intel.popup_rows",
    "intel.popup_max_width",
    "explorer.index_max",
    "explorer.index_hits_max",
    "meta.warm_columns_max",
    "meta.detail_max",
    "search.history_max",
    "search.history_rows",
    "file.open_max",
    "file.external_backup_keep",
    "project.backup_days",
    "project.scan_max",
    "session.max_shared",
    "session.max_private",
    "probe.max_retries",
    "connect.max_concurrent",
    // 스레드·캐시·프로세스
    "search.threads",
    "project.scan_threads",
    "search.gitignore",
    "search.excludes",
    "input.ime_hint_watch",
    "ui.clipboard_probe",
    "meta.disk_cache",
    "meta.warm_comments",
    "explorer.index_prefetch",
    "intel.preload",
    "ui.text_hint",
    "ui.text_snap",
    "ui.text_weight",
    "ui.text_contrast",
    "ui.text_gdi",
    // 자체 계측·개발·드물게 손대는 동작 규칙
    "log.dev_layers",
    "log.switch_scale",
    "log.template",
    "log.file_format",
    "gen.bind_note",
    "grid.edit_concurrency",
    "grid.edit_hidden_keys",
    "grid.edit_rowid",
    "grid.edit_all_cols",
    "grid.offset_warn",
    "grid.result_tab_evict",
    "grid.row_height_pct",
    "grid.col_max_chars",
    "grid.lob_view_max_mb",
    "db.fetch_all_size",
    "vars.expand_at",
    "vars.intrinsic",
    "vars.env_subst",
    "vars.signature_lookup",
    "bookmark.anchor_context",
    "explorer.search_index",
    "meta.refresh_idle_ms",
    "meta.refresh_on_missing",
    "tx.lock_wait_timeout_secs",
    "tx.server_idle_timeout_secs",
    "file.large_ext_level",
    "file.large_syntax_level",
    "file.large_head_mb",
    "file.async_load_mb",
    "file.load_progress_ms",
    "file.external_settle_ms",
    "file.external_merge_max_kb",
    "editor.undo_group_ms",
    "editor.undo_giant_mb",
    "editor.undo_persist_mb",
    "editor.undo_persist_days",
    "editor.max_occurrences",
    "editor.minimap_width",
    "ui.glyph_cache",
    "file.icon_cache",
    "cli.width",
];

/// 고급 설정인가(설정 창 Advanced 토글 대상) — 비노출 · DBMS 종속 카테고리 · [`ADVANCED`].
#[must_use]
pub fn is_advanced(key: &str) -> bool {
    if is_hidden(key) || ADVANCED.contains(&key) {
        return true;
    }
    entry(key).is_some_and(|e| group_of(e.cat) == Some(Msg::GrpDbms))
}

/// ★ 카테고리 안 표시 순서(사용자 09-28 "그룹별 · 항목별 순서로 · 세션 관련 설정이 흩어지지 않게"): (트리 순서, 키 접두의 첫 등재 순, 등재 순).
/// 같은 카테고리에 여러 접두(`session.` `tx.` `vars.` `run.` `db.`)가 섞여 있어도 접두끼리 모인다 · 접두 묶음의 순서 = 그 접두가 처음 나온 자리.
#[must_use]
pub fn display_order(key: &str) -> (usize, usize, usize, usize) {
    let Some(idx) = REGISTRY.iter().position(|e| e.key == key) else {
        // 동적 항목(확장 "사용")은 그 분류의 **맨 앞**.
        if let Some(e) = dynamic_entries().into_iter().find(|e| e.key == key) {
            let (g, c) = tree_order(e.cat);
            return (g, c, 0, 0);
        }
        return (usize::MAX, usize::MAX, usize::MAX, usize::MAX);
    };
    let e = &REGISTRY[idx];
    let (g, c) = tree_order(e.cat);
    let prefix = key.split('.').next().unwrap_or(key);
    let first = REGISTRY
        .iter()
        .position(|x| x.cat == e.cat && x.key.split('.').next().unwrap_or(x.key) == prefix)
        .unwrap_or(idx);
    (g, c, first, idx)
}

/// 허용 값 설명(오류 메시지·`config list`용).
#[must_use]
pub fn allowed(kind: SettingKind) -> String {
    match kind {
        SettingKind::Choice(opts) => opts.iter().map(|(v, _)| *v).collect::<Vec<_>>().join(" | "),
        SettingKind::Lang => std::iter::once(LANG_SYSTEM)
            .chain(Lang::ALL.iter().map(|l| l.code()))
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
        SettingKind::Lang => {
            if v.trim().eq_ignore_ascii_case(LANG_SYSTEM) {
                Some(LANG_SYSTEM.to_string())
            } else {
                Lang::from_code(v).map(|l| l.code().to_string())
            }
        }
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
        let mut seen: Vec<String> = Vec::new();
        for (k, v) in doc.pairs {
            match entry(&k) {
                Some(e) => {
                    seen.push(k.clone());
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
        s.migrate_indent_from_tab();
        s.migrate_renamed(&seen);
        s
    }

    /// 옛 `format.indent_from_tab`(09-29 폐기): **꺼져** 있었으면 설정값을 쓰던 사용자 — 단위·폭이 저장돼 있지 않으면 옛 기본
    /// (`tab`·`4`)을 명시해 동작을 지킨다. 켜짐/없음 = 새 기본(활성 탭 설정)과 같다. 옛 줄은 다음 저장 때 사라진다.
    fn migrate_indent_from_tab(&mut self) {
        let old = self
            .unknown
            .iter()
            .find(|(k, _)| k == "format.indent_from_tab")
            .map(|(_, v)| v.clone());
        self.unknown.retain(|(k, _)| k != "format.indent_from_tab");
        if old.as_deref().is_some_and(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "off" | "false" | "0"
            )
        }) {
            self.values
                .entry("format.indent".into())
                .or_insert_with(|| "tab".into());
            self.values
                .entry("format.indent_width".into())
                .or_insert_with(|| "4".into());
        }
    }

    /// [`RENAMED`] 표대로 옛 키의 값을 새 키로 옮긴다(새 키를 이미 정했으면 옛 값은 버림 · 옛 줄은 다음 저장 때 사라진다).
    /// [`RESCALED`] 표의 키는 **배수를 곱해**(초 → ms) 옮긴다 — 옛 기본값(예 `ui.toast_secs=3`)은 새 기본값(`3000`)과 같아 줄이 남지 않는다.
    fn migrate_renamed(&mut self, seen: &[String]) {
        let renamed = RENAMED.iter().map(|(o, n)| (*o, *n, 1));
        let rescaled = RESCALED.iter().copied();
        for (old, new, k) in renamed.chain(rescaled) {
            let Some(pos) = self.unknown.iter().position(|(key, _)| key == old) else {
                continue;
            };
            let (_, v) = self.unknown.remove(pos);
            let Some(e) = entry(new) else { continue };
            // 새 키가 파일에 있으면(기본값과 같아 values에 없어도) 사용자가 이미 정한 것 — 옛 값은 버린다.
            if self.values.contains_key(new) || seen.iter().any(|key| key == new) {
                continue;
            }
            let v = if k == 1 { v } else { scale_raw(&v, k) };
            if let Some(n) = normalize(e.kind, &v) {
                let def = default_of(e.key).unwrap_or(e.default);
                if n != def {
                    self.values.insert(new.to_string(), n);
                }
            }
        }
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
        let key = canonical_key(key);
        let e = entry(key)?;
        let def = default_of(e.key).unwrap_or(e.default);
        Some(self.values.get(key).map_or(def, String::as_str))
    }

    /// 현재 값을 **물은 키의 단위로**: 단위가 바뀐 옛 키([`RESCALED`] · `ui.toast_secs`)면 새 값을 배수로 나눈 문자열(`"3000"` → `"3"` ·
    /// `"2500"` → `"2.5"`) · 그 밖에는 [`Settings::get`]과 같다. CLI `config get`이 쓴다(옛 스크립트가 옛 단위로 읽어도 뜻이 같게).
    #[must_use]
    pub fn get_as(&self, key: &str) -> Option<String> {
        let v = self.get(key)?;
        Some(match alias_scale(key) {
            Some((_, k)) => unscale_value(v, k),
            None => v.to_string(),
        })
    }

    /// 사용자가 바꾼 값인가(기본값과 다른가) — VS Code의 "Modified" 표시에 해당.
    #[must_use]
    pub fn is_modified(&self, key: &str) -> bool {
        let key = canonical_key(key);
        self.values.contains_key(key)
    }

    /// 검증 후 설정(메모리). 기본값과 같으면 사용자 값을 지운다. 단위가 바뀐 옛 키([`RESCALED`])로 오면 **옛 단위로 해석**해
    /// 배수를 곱한다(`set ui.toast_secs 2.5` = `ui.toast_ms 2500`).
    pub fn set(&mut self, key: &str, raw: &str) -> Result<String, SetError> {
        let scaled;
        let raw = match alias_scale(key) {
            Some((_, k)) => {
                scaled = scale_raw(raw, k);
                scaled.as_str()
            }
            None => raw,
        };
        let key = canonical_key(key);
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
        // 기본값과 같으면 줄을 지운다. 화면 언어는 기본값이 `system`(OS를 따름)이라 `ko`·`en`을 고르면 OS 언어와 같아도
        // 저장된다 — 나중에 OS 언어를 바꿔도 앱은 고른 언어로 남는다(`system`을 고르거나 `reset` = 다시 OS를 따름).
        if n == default_of(e.key).unwrap_or(e.default) {
            self.values.remove(key);
        } else {
            self.values.insert(key.to_string(), n.clone());
        }
        Ok(n)
    }

    /// 기본값으로(메모리).
    pub fn reset(&mut self, key: &str) -> Result<&'static str, SetError> {
        let key = canonical_key(key);
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
        // `system`(기본) = OS 표시 언어(지원 언어일 때 · 그 밖은 영어) · 그 밖 = 고른 언어.
        match self.get("ui.lang") {
            Some(v) if v != LANG_SYSTEM => Lang::from_code(v).unwrap_or_default(),
            _ => nsql_i18n::system_lang(),
        }
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
            .chain(dynamic_entries())
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

    /// 옛 `format.indent_from_tab` 폐기(09-29): 꺼져 있던 사용자는 옛 기본(tab·4)을 명시 · 켜짐/없음 = 새 기본(활성 탭 설정) · 옛 줄은 사라진다.
    #[test]
    fn indent_from_tab_is_retired_with_migration() {
        let s = Settings::from_text(tmp("ift-off"), "format.indent_from_tab=off\n");
        assert_eq!(s.get("format.indent"), Some("tab"));
        assert_eq!(s.get("format.indent_width"), Some("4"));
        assert!(s.unknown.is_empty(), "{:?}", s.unknown);
        let s = Settings::from_text(
            tmp("ift-off2"),
            "format.indent_from_tab=off\nformat.indent=space\nformat.indent_width=2\n",
        );
        assert_eq!(s.get("format.indent"), Some("space"));
        assert_eq!(s.get("format.indent_width"), Some("2"));
        let s = Settings::from_text(tmp("ift-on"), "format.indent_from_tab=on\n");
        assert_eq!(s.get("format.indent"), Some("editor"));
        assert_eq!(s.get("format.indent_width"), Some("editor"));
        assert!(s.unknown.is_empty(), "{:?}", s.unknown);
    }

    /// 키 이름 바꿈(09-28 · docs/94 §6): 옛 줄 → 새 키(기본값이면 값 없음) · 새 키가 있으면 옛 값은 버림 · 옛 줄은 unknown에 남지 않는다 · `canonical_key`.
    #[test]
    fn renamed_keys_migrate_from_old_lines() {
        let s = Settings::from_text(
            tmp("renamed"),
            "ui.ime_hint=off\nui.ime_hint_watch=off\neditor.tab_line_scratch=#112233\nlicense.gates_dev=on\n",
        );
        assert_eq!(s.get("input.ime_hint"), Some("off"));
        assert_eq!(s.get("input.ime_hint_watch"), Some("off"));
        assert_eq!(s.get("editor.tab_line_unsaved"), Some("#112233"));
        assert_eq!(s.get("license.gates"), Some("on"));
        assert!(
            s.unknown.is_empty(),
            "옛 줄은 옮기고 지운다: {:?}",
            s.unknown
        );
        // 새 키를 이미 정했으면 옛 값은 버린다.
        let s = Settings::from_text(tmp("renamed2"), "ui.ime_hint=off\ninput.ime_hint=on\n");
        assert_eq!(s.get("input.ime_hint"), Some("on"));
        assert_eq!(canonical_key("ui.ime_hint"), "input.ime_hint");
        assert_eq!(
            s.get("ui.ime_hint"),
            Some("on"),
            "옛 키로 읽어도 새 키(CLI 호환)"
        );
        assert!(
            !s.is_modified("ui.ime_hint"),
            "기본값과 같으면 변경 아님(옛 키도 새 키로 판정)"
        );
        assert_eq!(canonical_key("ui.lang"), "ui.lang");
        let pairs = RENAMED
            .iter()
            .map(|(o, n)| (*o, *n))
            .chain(RESCALED.iter().map(|(o, n, _)| (*o, *n)));
        for (old, new) in pairs {
            assert!(
                entry(old).is_none(),
                "옛 키가 레지스트리에 남아 있다: {old}"
            );
            assert!(entry(new).is_some(), "새 키가 레지스트리에 없다: {new}");
            assert_eq!(canonical_key(old), new);
        }
    }

    /// ★ 단위 변환 이주(09-30 · docs/94 §6-5): 옛 초 줄 → 새 ms 키(×배수) · 옛 기본값은 새 기본값과 같아 줄이 없어짐 · 새 키가 있으면
    /// 옛 값 버림 · 소수 옛 값 · `get_as`/`set`은 옛 키를 옛 단위로 · 표의 배수는 새 기본값 = 옛 기본값 × 배수와 맞는지(회귀 방지).
    #[test]
    fn rescaled_keys_migrate_with_unit() {
        let s = Settings::from_text(
            tmp("rescaled1"),
            "ui.toast_secs=5\nprobe.timeout=2.5\nrun.toast_hide_secs=5\nproject.autosave_change_secs=30\nmeta.refresh_idle_secs=0\nexplorer.typeahead_timeout=1500\n",
        );
        assert_eq!(s.get("ui.toast_ms"), Some("5000"));
        assert_eq!(s.get("probe.timeout_ms"), Some("2500"), "소수 초도 ms로");
        assert!(
            !s.is_modified("run.toast_hide_ms"),
            "옛 기본값 5초 = 새 기본값 5000 → 변경 아님"
        );
        assert_eq!(s.get("project.autosave_change_ms"), Some("30000"));
        assert_eq!(s.get("meta.refresh_idle_ms"), Some("0"));
        assert_eq!(
            s.get("explorer.typeahead_timeout_ms"),
            Some("1500"),
            "접미만 붙은 키는 값 그대로"
        );
        assert!(
            s.unknown.is_empty(),
            "옛 줄은 옮기고 지운다: {:?}",
            s.unknown
        );
        // 옛 키로 읽기 = 옛 단위 · 새 키로 읽기 = 새 단위.
        assert_eq!(s.get_as("ui.toast_secs").as_deref(), Some("5"));
        assert_eq!(s.get_as("probe.timeout").as_deref(), Some("2.5"));
        assert_eq!(s.get_as("ui.toast_ms").as_deref(), Some("5000"));
        assert_eq!(
            s.get("ui.toast_secs"),
            Some("5000"),
            "`get`은 늘 새 단위(옛 키 = 별칭)"
        );
        // 새 키를 이미 정했으면 옛 값은 버린다.
        let s = Settings::from_text(tmp("rescaled2"), "ui.toast_secs=5\nui.toast_ms=1200\n");
        assert_eq!(s.get("ui.toast_ms"), Some("1200"));
        // 옛 키로 set = 옛 단위 해석 · 범위 검증은 새 단위로.
        let mut s = Settings::from_text(tmp("rescaled3"), "");
        assert_eq!(s.set("ui.toast_secs", "2.5").unwrap(), "2500");
        assert_eq!(s.get("ui.toast_ms"), Some("2500"));
        assert_eq!(s.set("ui.toast_secs", "3").unwrap(), "3000");
        assert!(
            !s.is_modified("ui.toast_ms"),
            "기본값(3초)으로 돌아오면 줄 없음"
        );
        assert!(s.set("ui.toast_secs", "0.01").is_err(), "10 ms < 하한 100");
        assert_eq!(s.reset("probe.retry_delay").unwrap(), "10000");
        // 표의 배수 = 새 기본값 / 옛 기본값 검산(옛 기본값은 문서 기준: 5·2·10·3·5·10초).
        let old_defaults = [
            ("meta.refresh_idle_secs", 5),
            ("probe.timeout", 2),
            ("probe.retry_delay", 10),
            ("ui.toast_secs", 3),
            ("run.toast_hide_secs", 5),
            ("project.autosave_change_secs", 10),
        ];
        for (old, new, k) in RESCALED {
            let od = old_defaults
                .iter()
                .find(|(o, _)| o == old)
                .map(|(_, d)| *d)
                .unwrap_or_else(|| panic!("RESCALED에 옛 기본값 표가 없는 키: {old}"));
            let nd: i64 = entry(new).unwrap().default.parse().unwrap();
            assert_eq!(nd, od * k, "{new} 기본값 = 옛 기본값 × 배수");
            assert!(new.ends_with("_ms"), "새 키는 단위 접미: {new}");
        }
        assert_eq!(alias_scale("ui.toast_ms"), None);
        assert_eq!(unscale_value("2500", 1000), "2.5");
        assert_eq!(unscale_value("2", 1000), "0.002");
        assert_eq!(scale_raw(" 1.2345 ", 1000), "1235");
        assert_eq!(scale_raw("abc", 1000), "abc");
    }

    /// 고급 설정 표(09-28): 전부 존재하는 키 · 중복 없음 · DBMS 그룹은 표 없이도 고급 · 자주 쓰는 키는 기본 표시.
    #[test]
    fn advanced_keys_exist() {
        for k in ADVANCED {
            assert!(entry(k).is_some(), "ADVANCED에 없는 키: {k}");
        }
        let mut seen = std::collections::HashSet::new();
        for k in ADVANCED {
            assert!(seen.insert(*k), "ADVANCED 중복: {k}");
        }
        assert!(is_advanced("oracle.client_mode"), "DBMS 종속 = 고급");
        assert!(is_advanced("window.main_size"), "HIDDEN = 고급");
        assert!(is_advanced("search.threads"));
        for k in [
            "ui.lang",
            "ui.theme",
            "editor.font_size",
            "grid.max_rows",
            "project.autosave",
            "project.autosave_secs",
            "session.autocommit",
            "session.idle_secs",
            "explorer.typeahead",
        ] {
            assert!(!is_advanced(k), "기본 표시여야 한다: {k}");
        }
    }

    /// 표시 순서(09-28 "세션 관련 설정이 흩어지지 않게"): 같은 카테고리 안에서 키 접두끼리 모이고 · 카테고리는 트리 순 · 모르는 키는 맨 뒤.
    #[test]
    fn display_order_groups_by_category_then_prefix() {
        let mut keys: Vec<&str> = REGISTRY
            .iter()
            .filter(|e| e.cat == Msg::CatSession)
            .map(|e| e.key)
            .collect();
        keys.sort_by_key(|k| display_order(k));
        // 접두가 한 번 바뀌면 다시 돌아오지 않는다(= 접두 묶음이 연속).
        let mut seen: Vec<&str> = Vec::new();
        for k in &keys {
            let p = k.split('.').next().unwrap_or(k);
            if seen.last() != Some(&p) {
                assert!(!seen.contains(&p), "접두 {p}가 흩어졌다: {keys:?}");
                seen.push(p);
            }
        }
        assert!(
            display_order("log.max_lines") < display_order("session.autocommit"),
            "General 그룹 안 트리 순"
        );
        assert!(
            display_order("session.autocommit") < display_order("editor.font_size"),
            "그룹 순"
        );
        assert_eq!(display_order("nope").0, usize::MAX);
    }

    /// 09-28 기본값: 서버 노드 연결 해제 = 항상 고르기(D-144) · 파일 검색 상한 = SSD 8 MB · 향상 모드 = HDD 2 MB · 옛 기본값이 파일에 있으면 새 기본값.
    #[test]
    fn defaults_disconnect_pick_and_search_limit() {
        let s = Settings::from_text(
            tmp("d0928"),
            "explorer.disconnect_pick=auto\nsearch.max_file_kb=1024\n",
        );
        assert_eq!(s.get("explorer.disconnect_pick"), Some("always"));
        assert_eq!(s.get("search.max_file_kb"), Some("8192"));
        assert!(!s.is_modified("search.max_file_kb"));
        assert_eq!(perf::boost_value("search.max_file_kb"), Some("2048"));
        assert_eq!(
            default_of("license.gates"),
            Some("off"),
            "D-145 개인 사용 = 전 기능"
        );
        assert!(dependency("input.ime_hint_watch").is_some_and(|(p, _)| p == "input.ime_hint"));
        assert!(CATEGORY_TREE
            .iter()
            .any(|(_, cats)| cats.contains(&Msg::CatLargeFiles)));
        // T-250(09-29): 2차 rename은 옛 키로 읽어도 새 키로 통한다 · 새 분류에 접두가 옮겨 갔다.
        assert_eq!(canonical_key("conn.window_w"), "login.window_w");
        assert_eq!(canonical_key("sql.key_mode"), "grid.key_mode");
        assert_eq!(canonical_key("ui.fade_fast"), "ui.fade_fast_ms");
        assert_eq!(
            canonical_key("editor.tab_accent"),
            "editor.tab_accent_color"
        );
        assert!(entry("conn.window_w").is_none() && entry("login.window_w").is_some());
        assert_eq!(entry("tx.stale_min").map(|e| e.cat), Some(Msg::CatTxSafety));
        assert_eq!(
            entry("vars.persist").map(|e| e.cat),
            Some(Msg::CatScriptVars)
        );
        assert_eq!(
            entry("probe.enabled").map(|e| e.cat),
            Some(Msg::CatServerStatus)
        );
        assert_eq!(entry("run.toast").map(|e| e.cat), Some(Msg::CatRunCards));
        assert_eq!(
            entry("editor.undo_budget_mb").map(|e| e.cat),
            Some(Msg::CatUndo)
        );
        assert!(CATEGORY_TREE.iter().any(|(_, c)| c.contains(&Msg::CatUndo)));
        assert_eq!(
            entry("file.large_l1_mb").map(|e| e.cat),
            Some(Msg::CatLargeFiles)
        );
    }

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
        // 언어 기본값 = `system` → OS 표시 언어(지원 언어일 때) · 그 밖은 영어 — 시험 기기에 따라 en/ko.
        assert_eq!(s.lang(), nsql_i18n::system_lang());
        assert_eq!(s.get("ui.lang"), Some(LANG_SYSTEM));
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

    /// 언어 기본값 = `system`(OS를 따름 · 10-04) — 사용자가 언어를 고르면(OS와 같아도) 저장돼 이긴다 · `system`/reset = 다시 OS를 따름.
    #[test]
    fn lang_follows_os_until_user_picks() {
        let p = tmp("lang_os");
        let mut s = Settings::open(p.clone());
        let os = nsql_i18n::system_lang();
        assert!(
            !s.is_modified("ui.lang"),
            "처음 = OS를 따른다(저장된 값 없음)"
        );
        assert_eq!(s.set("ui.lang", os.code()).unwrap(), os.code());
        assert!(
            s.is_modified("ui.lang"),
            "OS와 같은 언어라도 고른 값은 저장"
        );
        s.save().unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.contains(&format!("ui.lang={}", os.code())), "{text}");
        assert_eq!(Settings::open(p).lang(), os, "저장된 언어를 그대로 읽는다");
        // `system`을 고르면 줄이 사라지고(기본값) 다시 OS를 따른다 · 대소문자 무관.
        assert_eq!(s.set("ui.lang", "System").unwrap(), LANG_SYSTEM);
        assert!(!s.is_modified("ui.lang"));
        assert_eq!(s.lang(), os);
        s.set("ui.lang", "ko").unwrap();
        assert_eq!(s.lang(), Lang::Ko);
        assert_eq!(s.reset("ui.lang").unwrap(), LANG_SYSTEM);
        assert!(!s.is_modified("ui.lang"), "reset = 다시 OS를 따른다");
        assert!(s.set("ui.lang", "xx").is_err());
        assert!(allowed(SettingKind::Lang).starts_with("system | "));
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
        s.set("ui.fade_fast_ms", "900").unwrap();
        s.set("statusbar.git", "on").unwrap();
        assert_eq!(s.effective("ui.fade_fast_ms"), Some("900"));
        assert!(!s.boost_locked("ui.fade_fast_ms"));
        s.set("perf.boost", "on").unwrap();
        assert_eq!(s.effective("ui.fade_fast_ms"), Some("0"), "강제값");
        assert_eq!(s.int("ui.fade_fast_ms"), 0);
        assert!(!s.flag("statusbar.git"));
        assert_eq!(s.get("ui.fade_fast_ms"), Some("900"), "저장값은 그대로");
        assert!(s.boost_locked("ui.fade_fast_ms"));
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
        assert_eq!(s.effective("ui.fade_fast_ms"), Some("900"));
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
