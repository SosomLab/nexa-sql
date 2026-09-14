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

/// 설정 폴더 — `NSQL_HOME`이 있으면 그것(테스트·포터블), 아니면 OS 사용자 설정 폴더. `nsql-vault`도 같은 규칙을 쓴다.
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

const SESSION_MODE_OPTS: &[(&str, Msg)] = &[
    ("shared", Msg::ValSessionShared),
    ("per-editor", Msg::ValSessionPerEditor),
];

const TAB_ROWS_OPTS: &[(&str, Msg)] =
    &[("single", Msg::ValTabsSingle), ("multi", Msg::ValTabsMulti)];

const SCROLL_OPTS: &[(&str, Msg)] = &[("pixel", Msg::ValScrollPixel), ("row", Msg::ValScrollRow)];

const WS_OPTS: &[(&str, Msg)] = &[
    ("none", Msg::ValWsNone),
    ("selection", Msg::ValWsSelection),
    ("all", Msg::ValWsAll),
];

const WINDOW_FOCUS_OPTS: &[(&str, Msg)] = &[
    ("group", Msg::ValFocusGroup),
    ("single", Msg::ValFocusSingle),
];

const LOG_FORMAT_OPTS: &[(&str, Msg)] = &[
    ("raw", Msg::ValRaw),
    ("markdown", Msg::ValMarkdown),
    ("grid", Msg::ValGrid),
];

const THEME_OPTS: &[(&str, Msg)] = &[
    ("system", Msg::ValSystem),
    ("light", Msg::ValLight),
    ("dark", Msg::ValDark),
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
        key: "ui.font_size",
        cat: Msg::CatAppearance,
        label: Msg::LblUiFontSize,
        desc: Msg::DescUiFontSize,
        kind: SettingKind::Int { min: 8, max: 40 },
        default: "14",
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
        key: "editor.rulers",
        cat: Msg::CatEditor,
        label: Msg::LblRulers,
        desc: Msg::DescRulers,
        kind: SettingKind::Text,
        default: "80",
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
        key: "grid.row_numbers",
        cat: Msg::CatAppearance,
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
        key: "explorer.tooltip",
        cat: Msg::CatExplorer,
        label: Msg::LblExplorerTooltip,
        desc: Msg::DescExplorerTooltip,
        kind: SettingKind::Bool,
        default: "on",
    },
    Entry {
        key: "grid.scroll",
        cat: Msg::CatAppearance,
        label: Msg::LblGridScroll,
        desc: Msg::DescGridScroll,
        kind: SettingKind::Choice(SCROLL_OPTS),
        default: "pixel",
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
    Entry {
        key: "connect.max_concurrent",
        cat: Msg::CatConnection,
        label: Msg::LblMaxConcurrent,
        desc: Msg::DescMaxConcurrent,
        kind: SettingKind::Int { min: 1, max: 16 },
        default: "4",
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
        key: "session.mode",
        cat: Msg::CatSession,
        label: Msg::LblSessionMode,
        desc: Msg::DescSessionMode,
        kind: SettingKind::Choice(SESSION_MODE_OPTS),
        default: "shared",
    },
    Entry {
        key: "log.format",
        cat: Msg::CatLog,
        label: Msg::LblLogFormat,
        desc: Msg::DescLogFormat,
        kind: SettingKind::Choice(LOG_FORMAT_OPTS),
        default: "raw",
    },
    Entry {
        key: "editor.font_size",
        cat: Msg::CatEditor,
        label: Msg::LblEditorFontSize,
        desc: Msg::DescEditorFontSize,
        kind: SettingKind::Int { min: 8, max: 40 },
        default: "14",
    },
];

/// 키로 항목 찾기.
#[must_use]
pub fn entry(key: &str) -> Option<&'static Entry> {
    REGISTRY.iter().find(|e| e.key == key)
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
        SettingKind::Bool => match v.to_ascii_lowercase().as_str() {
            "on" | "true" | "1" | "yes" => Some("on".into()),
            "off" | "false" | "0" | "no" => Some("off".into()),
            _ => None,
        },
        SettingKind::Text => Some(v.to_string()),
    }
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

    /// on/off 설정.
    #[must_use]
    pub fn flag(&self, key: &str) -> bool {
        self.get(key) == Some("on")
    }

    /// 정수 설정(레지스트리 기본값 보장 → 실패 없음).
    #[must_use]
    pub fn int(&self, key: &str) -> i64 {
        self.get(key)
            .and_then(|v| v.parse().ok())
            .or_else(|| entry(key).and_then(|e| e.default.parse().ok()))
            .unwrap_or(0)
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
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
        assert_eq!(s.int("ui.font_size"), 14);
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
        assert_eq!(s.int("ui.font_size"), 14);
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
}
