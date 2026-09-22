//! 성능 거버너(docs/39 §4 · T-90a) — `perf.mode` 마스터 1개 ▸ 도메인 프리셋([`PerfBinding`]) ▸ 개별 키.
//!
//! ```text
//! perf.mode  (auto | full | balanced | low | custom)      ← 마스터 · 상태줄 ⚡ · `nsql config set perf.mode low`
//!    └ 도메인 프리셋(§3 표의 full/balanced/low 열)          ← [`PERF`] 표(= 원장 · `Entry::perf()`)
//!         └ 개별 키                                       ← 사용자가 직접 바꾸면 그 키만 우선 · 모드 표시 = custom(D-59)
//! ```
//!
//! - **우선순위**([`Settings::effective`]): 사용자가 명시한 개별 값 > 모드 프리셋 > 레지스트리 기본. 개별 값을 지우면(`reset`) 다시 모드를 따른다.
//! - **auto**: OS 신호([`nexa_sys::Signals`] · 60초 캐시)로 고른다 — 배터리 전원·원격 세션 → balanced · 그 외 full.
//!   OS "동작 줄이기"는 모드가 아니라 `ui.animations = auto`가 따른다([`Settings::animations_enabled`]).
//! - **기본 = full**(D-58 · 지금 동작 그대로) + 배터리 전원이면 상태줄 1회 안내([`Settings::perf_hint`]).
//! - `PERF`에 있는 키가 곧 부하원 원장이다 — `nsql config list perf`는 이 표만 찍는다(docs/39 §3 표는 "왜"만).
//!   `Entry`에 필드를 두지 않고 별도 표로 둔 이유: 레지스트리에 항목을 append하는 다른 작업과 컴파일 충돌이 없다(09-16 병행 작업).

use crate::{entry, Entry, Settings, REGISTRY};
use nsql_i18n::Msg;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 부하 도메인(docs/39 §2 · 여섯).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Domain {
    Db,
    Net,
    Cpu,
    Gfx,
    Ui,
    Mem,
}

impl Domain {
    /// 표시 순서.
    pub const ALL: [Domain; 6] = [
        Domain::Db,
        Domain::Net,
        Domain::Cpu,
        Domain::Gfx,
        Domain::Ui,
        Domain::Mem,
    ];

    /// 짧은 식별자(`list perf` 인자·로그).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Domain::Db => "db",
            Domain::Net => "net",
            Domain::Cpu => "cpu",
            Domain::Gfx => "gfx",
            Domain::Ui => "ui",
            Domain::Mem => "mem",
        }
    }

    /// 표시 라벨.
    #[must_use]
    pub const fn label(self) -> Msg {
        match self {
            Domain::Db => Msg::CfgPerfDomDb,
            Domain::Net => Msg::CfgPerfDomNet,
            Domain::Cpu => Msg::CfgPerfDomCpu,
            Domain::Gfx => Msg::CfgPerfDomGfx,
            Domain::Ui => Msg::CfgPerfDomUi,
            Domain::Mem => Msg::CfgPerfDomMem,
        }
    }
}

/// 부하원 등재 — 이 키가 모드별로 갖는 값(docs/39 §3 표의 full/balanced/low 열).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerfBinding {
    pub domain: Domain,
    pub full: &'static str,
    pub balanced: &'static str,
    pub low: &'static str,
}

impl PerfBinding {
    /// 프리셋 모드의 값(auto·custom은 프리셋이 아니므로 `None`).
    #[must_use]
    pub const fn value(&self, mode: PerfMode) -> Option<&'static str> {
        match mode {
            PerfMode::Full => Some(self.full),
            PerfMode::Balanced => Some(self.balanced),
            PerfMode::Low => Some(self.low),
            PerfMode::Auto | PerfMode::Custom => None,
        }
    }
}

/// `perf.mode` 값.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PerfMode {
    /// OS 신호로 고른다(배터리·원격 세션 → balanced · 그 외 full).
    Auto,
    /// 제한 없음 = 지금 동작 그대로(기본 · D-58).
    #[default]
    Full,
    Balanced,
    Low,
    /// 프리셋을 적용하지 않는다(개별 키만) — 개별 키를 바꾼 상태의 표시값이기도 하다(D-59).
    Custom,
}

impl PerfMode {
    pub const ALL: [PerfMode; 5] = [
        PerfMode::Auto,
        PerfMode::Full,
        PerfMode::Balanced,
        PerfMode::Low,
        PerfMode::Custom,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            PerfMode::Auto => "auto",
            PerfMode::Full => "full",
            PerfMode::Balanced => "balanced",
            PerfMode::Low => "low",
            PerfMode::Custom => "custom",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<PerfMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(PerfMode::Auto),
            "full" => Some(PerfMode::Full),
            "balanced" => Some(PerfMode::Balanced),
            "low" => Some(PerfMode::Low),
            "custom" => Some(PerfMode::Custom),
            _ => None,
        }
    }

    #[must_use]
    pub const fn label(self) -> Msg {
        match self {
            PerfMode::Auto => Msg::ValPerfAuto,
            PerfMode::Full => Msg::ValPerfFull,
            PerfMode::Balanced => Msg::ValPerfBalanced,
            PerfMode::Low => Msg::ValPerfLow,
            PerfMode::Custom => Msg::ValPerfCustom,
        }
    }

    /// full/balanced/low(프리셋 값이 있는 모드)인가.
    #[must_use]
    pub const fn is_preset(self) -> bool {
        matches!(self, PerfMode::Full | PerfMode::Balanced | PerfMode::Low)
    }
}

/// `perf.mode` 후보(레지스트리 `Choice`).
pub(crate) const PERF_MODE_OPTS: &[(&str, Msg)] = &[
    ("auto", Msg::ValPerfAuto),
    ("full", Msg::ValPerfFull),
    ("balanced", Msg::ValPerfBalanced),
    ("low", Msg::ValPerfLow),
    ("custom", Msg::ValPerfCustom),
];

const fn b(
    domain: Domain,
    full: &'static str,
    balanced: &'static str,
    low: &'static str,
) -> PerfBinding {
    PerfBinding {
        domain,
        full,
        balanced,
        low,
    }
}

/// ★ 부하원 원장(docs/39 §3 · 등재 = 설정 키 = 모드별 값). 새 부하원 = [`REGISTRY`] 한 줄 + 여기 한 줄.
/// `full` 열은 레지스트리 기본값과 같아야 한다(기본 모드 full = 지금 동작 그대로 · 테스트가 강제).
pub const PERF: &[(&str, PerfBinding)] = &[
    // ── DB(§3-1)
    ("grid.max_rows", b(Domain::Db, "200", "200", "100")),
    ("db.statement_timeout", b(Domain::Db, "0", "0", "60")),
    (
        "oracle.live.source",
        b(Domain::Db, "session", "session", "off"),
    ),
    (
        "oracle.live.interval_ms",
        b(Domain::Db, "1000", "2000", "5000"),
    ),
    ("explorer.visible", b(Domain::Db, "on", "on", "off")),
    // 탐색기 유휴 워터마크 주기(docs/57 T2) — low = 끔(내가 실행한 DDL 반영 T1은 사용자 동작에 딸린 1질의라 유지).
    ("meta.refresh_secs", b(Domain::Db, "300", "600", "0")),
    ("connect.auto_reconnect", b(Domain::Db, "on", "on", "off")),
    // ── NET(§3-2 · 26 §8 흡수)
    ("probe.enabled", b(Domain::Net, "on", "on", "off")),
    ("probe.interval", b(Domain::Net, "60", "120", "300")),
    ("probe.max_inflight", b(Domain::Net, "16", "8", "2")),
    ("probe.icmp", b(Domain::Net, "on", "on", "off")),
    ("connect.max_concurrent", b(Domain::Net, "4", "2", "1")),
    ("probe.dns_cache_secs", b(Domain::Net, "0", "300", "3600")),
    // ── CPU(§3-3)
    (
        "editor.highlight_max_kb",
        b(Domain::Cpu, "1024", "512", "128"),
    ),
    (
        "editor.max_occurrences",
        b(Domain::Cpu, "10000", "5000", "1000"),
    ),
    ("file.probe_chevrons", b(Domain::Cpu, "on", "on", "off")),
    ("file.os_icons", b(Domain::Cpu, "on", "on", "off")),
    // ── GFX(§3-4)
    ("ui.max_fps", b(Domain::Gfx, "60", "30", "15")),
    ("ui.animations", b(Domain::Gfx, "auto", "auto", "off")),
    ("editor.caret_blink", b(Domain::Gfx, "on", "on", "off")),
    // ── MEM(§3-6)
    ("log.max_lines", b(Domain::Mem, "10000", "5000", "1000")),
    ("editor.undo_max", b(Domain::Mem, "1000", "500", "100")),
    ("ui.glyph_cache", b(Domain::Mem, "8192", "4096", "2048")),
    ("file.icon_cache", b(Domain::Mem, "512", "512", "128")),
];

/// ★ **실행 속도 향상**(`perf.boost` · 사용자 09-17) — 켜면 이 표의 키는 **사용자 값과 무관하게 이 값으로 강제**되고 설정 창에서 잠긴다.
/// 목표(사용자 09-17): ① 처음 실행 속도 ② 쿼리·네트워크 실행 속도 ③ 백그라운드·편의 기능 스레드 최소화 ④ 메모리 최소화·빠른 회수 ⑤ 체감 속도.
/// 원칙: 실제 동작(결과·트랜잭션·접속)에는 영향이 없고 **UI 구성·부가 표시·폴링·I/O에만** 영향을 주는 키만 넣는다. 저장값은 건드리지 않으므로
/// 끄면 그대로 돌아온다. 분류·제외 근거 = docs/39 §4-6. 등재 키는 전부 레지스트리에 있고 **배선이 있어야** 한다(강제해도 효과 0인 키는
/// 넣지 않는다: `explorer.tooltip`·`probe.dns_cache_secs`·`settings.watch_ms`는 기능 미구현이라 제외 · 테스트).
pub const BOOST: &[(&str, &str)] = &[
    // ── 렌더링·애니메이션(GFX): 프레임·페이드·깜빡임 = 다시 그리기 횟수
    ("ui.animations", "off"),
    ("ui.max_fps", "30"),
    ("editor.caret_blink", "off"),
    ("ui.fade_fast", "0"),
    ("ui.fade_slow", "0"),
    ("ui.fade_out_ms", "0"),
    ("ui.slide_ms", "0"),
    ("ui.hover_intent_ms", "120"),
    // 토스트 남은 시간 막대 + 진척 페이드(09-22) — 카드가 떠 있는 동안 30ms마다 다시 그린다 · 끄면 종전(마지막 300ms만).
    ("ui.toast_progress", "off"),
    ("run.toast_tick_ms", "1000"),
    ("run.toast_max", "8"),
    // ── 아이콘·부가 표시(메모리·래스터): 트리 아이콘 · OS 파일 아이콘 · 우클릭 메뉴 아이콘 · 툴팁 · 미니맵 · 선택어 강조
    ("explorer.icons", "off"),
    ("file.os_icons", "off"),
    ("file.probe_chevrons", "off"),
    ("ui.menu_icons", "off"),
    ("tabs.tooltip", "off"),
    ("editor.minimap", "off"),
    ("editor.highlight_selection", "off"),
    ("editor.diff_marks", "off"),
    ("rainbowpair.enabled", "off"),
    // 탐색기 갱신 뒤 새 객체 강조(docs/57) — 강조가 사라질 때까지 다시 그리기가 이어진다.
    ("meta.refresh_highlight_ms", "0"),
    // ── I/O·기동(파일·프로세스·클립보드·둘째 창): git 프로세스 · 복사 시 HTML 생성 · 우클릭 클립보드 읽기 · 시작 시 로그 창
    ("statusbar.git", "off"),
    ("editor.copy_rich", "off"),
    ("ui.clipboard_probe", "off"),
    // 되돌리기 기록 파일(docs/60 §7 · 저장·닫기 때 쓰기 + 열 때 읽기·검증 = 디스크 I/O와 기동 비용) — 세션 안의 되돌리기는 그대로.
    ("editor.undo_persist", "off"),
    ("log.open_at_start", "off"),
    ("log.dev_mode", "off"),
    // 막힘 감지 폴링(docs/56 L3) — 네트워크 부하원이라 향상 모드는 끈다(L1·L2는 데이터 안전 기능이라 건드리지 않는다).
    ("tx.block_poll_secs", "0"),
    // 탐색기 유휴 워터마크 폴링(docs/57 T2) — 같은 이유. 실행한 DDL 반영(T1)·못 찾음(T4)은 그대로.
    ("meta.refresh_secs", "0"),
    // 보이는 탭의 외부 변경 폴링(docs/58) — 창 활성화·탭 전환·저장 직전 확인은 그대로.
    ("file.external_poll_ms", "0"),
    // ── 메모리: 결과 다중 탭(사용자 09-21 "다중 탭을 끄는 것은 의도적으로 메모리 사용을 억제하기 위한 설정 — 향상 모드에 포함").
    //    끄면 편집기 탭당 결과 하나만 들고 다른 결과 탭은 즉시 해제된다(D-73) · 결과 자체는 그대로 조회·표시된다.
    ("grid.result_tabs", "off"),
    // ── 메모리(캐시 상한 · 아이콘은 위에서 껐으므로 캐시도 최소)
    ("file.icon_cache", "128"),
    // ── 폴링·시도 횟수·시간·스레드(NET/DB 표시용): 신호등(스레드 1) · 탐색기 자동 갱신 · Oracle 라이브 로그
    ("probe.interval", "300"),
    ("probe.max_inflight", "1"),
    ("probe.max_retries", "1"),
    ("probe.icmp", "off"),
    ("oracle.live.source", "off"),
    ("oracle.live.interval_ms", "5000"),
];

/// 향상 모드가 이 키를 강제하는 값(등재되지 않은 키 = `None`).
#[must_use]
pub fn boost_value(key: &str) -> Option<&'static str> {
    BOOST.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// 키의 부하원 등재(없으면 부하원이 아니다).
#[must_use]
pub fn binding(key: &str) -> Option<&'static PerfBinding> {
    PERF.iter().find(|(k, _)| *k == key).map(|(_, b)| b)
}

impl Entry {
    /// 이 키가 부하원이면 모드별 값(docs/39 §4-2 `Entry.perf`).
    #[must_use]
    pub fn perf(&self) -> Option<&'static PerfBinding> {
        binding(self.key)
    }
}

/// `auto`가 OS 신호로 고르는 모드 — 배터리 전원·원격 세션 → balanced · 그 외(모름 포함) full.
#[must_use]
pub const fn resolve_auto(sig: &nexa_sys::Signals) -> PerfMode {
    match (sig.on_battery, sig.remote_session) {
        (Some(true), _) | (_, Some(true)) => PerfMode::Balanced,
        _ => PerfMode::Full,
    }
}

// ────────────────────────────────────────────── OS 신호 캐시(기동 1회 + 60초 · docs/39 §4-4 · 부하원: 60초마다 syscall 몇 개)

/// 신호 재조회 간격.
pub const SIGNAL_TTL: Duration = Duration::from_secs(60);

static SIGNALS: Mutex<Option<(Instant, nexa_sys::Signals)>> = Mutex::new(None);
static OVERRIDE: Mutex<Option<nexa_sys::Signals>> = Mutex::new(None);

/// 현재 OS 신호(60초 캐시 · 재지정이 있으면 그것).
#[must_use]
pub fn signals() -> nexa_sys::Signals {
    if let Some(o) = OVERRIDE.lock().ok().and_then(|g| *g) {
        return o;
    }
    let mut g = match SIGNALS.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    match *g {
        Some((t, s)) if t.elapsed() < SIGNAL_TTL => s,
        _ => {
            let s = nexa_sys::Signals::read();
            *g = Some((Instant::now(), s));
            s
        }
    }
}

/// 신호 재지정(테스트 · `nsql --perf` 류 진단) — `None`이면 다시 OS를 읽는다.
pub fn set_signals_override(sig: Option<nexa_sys::Signals>) {
    if let Ok(mut g) = OVERRIDE.lock() {
        *g = sig;
    }
}

/// 값의 출처(`nsql config list perf` 세 번째 열).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerfSource {
    /// 실행 속도 향상 모드가 강제한 값(`perf.boost` · 사용자 값·모드보다 우선).
    Boost,
    /// 사용자가 개별 키를 직접 바꿨다.
    User,
    /// 모드 프리셋(full/balanced/low · auto는 풀린 모드).
    Mode(PerfMode),
    /// 레지스트리 기본(custom 모드 · 또는 프리셋 값 = 기본값).
    Default,
}

impl PerfSource {
    #[must_use]
    pub const fn label(self) -> Msg {
        match self {
            PerfSource::Boost => Msg::CfgSrcBoost,
            PerfSource::User => Msg::CfgSrcUser,
            PerfSource::Mode(_) => Msg::CfgSrcMode,
            PerfSource::Default => Msg::CfgSrcDefault,
        }
    }
}

/// `list perf` 한 줄.
#[derive(Clone, Debug)]
pub struct PerfRow {
    pub entry: &'static Entry,
    pub binding: &'static PerfBinding,
    /// 실효 값([`Settings::effective`]).
    pub value: String,
    pub source: PerfSource,
}

impl Settings {
    /// 설정된 `perf.mode`(파일 값 · auto는 풀지 않는다).
    #[must_use]
    pub fn perf_mode(&self) -> PerfMode {
        self.get("perf.mode")
            .and_then(PerfMode::parse)
            .unwrap_or_default()
    }

    /// 프리셋이 실제로 적용되는 모드 — auto는 OS 신호로 푼다 · custom은 그대로(프리셋 없음).
    #[must_use]
    pub fn perf_mode_resolved(&self) -> PerfMode {
        match self.perf_mode() {
            PerfMode::Auto => resolve_auto(&signals()),
            m => m,
        }
    }

    /// 상태줄·설정 창에 보일 모드(D-59) — 부하원 키 하나라도 개별 값이 있으면 `custom`, 아니면 설정된 모드.
    #[must_use]
    pub fn perf_mode_display(&self) -> PerfMode {
        if self.perf_has_overrides() {
            PerfMode::Custom
        } else {
            self.perf_mode()
        }
    }

    /// 부하원 키 중 사용자가 직접 바꾼 것이 있는가.
    #[must_use]
    pub fn perf_has_overrides(&self) -> bool {
        PERF.iter().any(|(k, _)| self.is_modified(k))
    }

    /// 실행 속도 향상 모드가 켜져 있는가(`perf.boost`).
    #[must_use]
    pub fn boost_on(&self) -> bool {
        self.get("perf.boost") == Some("on")
    }

    /// 향상 모드가 지금 이 키를 강제·잠금 중인가(설정 창 잠금 · 값은 [`boost_value`]).
    #[must_use]
    pub fn boost_locked(&self, key: &str) -> bool {
        self.boost_on() && boost_value(key).is_some()
    }

    /// 실효 값 — **향상 모드 강제값** > 개별 값 > 모드 프리셋 > 기본. 모르는 키는 `None`. 부하원이 아닌 키는 [`Settings::get`]과 같다.
    #[must_use]
    pub fn effective(&self, key: &str) -> Option<&str> {
        let e = entry(key)?;
        if self.boost_on() {
            if let Some(v) = boost_value(key) {
                return Some(v);
            }
        }
        if self.is_modified(key) {
            return self.get(key);
        }
        if let Some(b) = e.perf() {
            if let Some(v) = b.value(self.perf_mode_resolved()) {
                return Some(v);
            }
        }
        Some(e.default)
    }

    /// 실효 값의 출처(부하원이 아닌 키는 `None`).
    #[must_use]
    pub fn perf_source(&self, key: &str) -> Option<PerfSource> {
        let e = entry(key)?;
        let b = e.perf()?;
        if self.boost_locked(key) {
            return Some(PerfSource::Boost);
        }
        if self.is_modified(key) {
            return Some(PerfSource::User);
        }
        let m = self.perf_mode_resolved();
        match b.value(m) {
            Some(v) if v != e.default => Some(PerfSource::Mode(m)),
            _ => Some(PerfSource::Default),
        }
    }

    /// 실효 정수(레지스트리 기본값 보장).
    #[must_use]
    pub fn effective_int(&self, key: &str) -> i64 {
        self.effective(key)
            .and_then(|v| v.parse().ok())
            .or_else(|| entry(key).and_then(|e| e.default.parse().ok()))
            .unwrap_or(0)
    }

    /// 실효 on/off.
    #[must_use]
    pub fn effective_flag(&self, key: &str) -> bool {
        self.effective(key) == Some("on")
    }

    /// `ui.animations` 실효 — auto는 OS "동작 줄이기"의 반대.
    #[must_use]
    pub fn animations_enabled(&self) -> bool {
        match self.effective("ui.animations") {
            Some("on") => true,
            Some("off") => false,
            _ => !signals().reduce_motion.unwrap_or(false),
        }
    }

    /// 부하원 전부(도메인 → 원장 순서) — `nsql config list perf` · 설정 창 Performance 카드.
    #[must_use]
    pub fn perf_rows(&self) -> Vec<PerfRow> {
        let mut rows: Vec<PerfRow> = PERF
            .iter()
            .filter_map(|(k, b)| {
                let e = REGISTRY.iter().find(|e| e.key == *k)?;
                Some(PerfRow {
                    entry: e,
                    binding: b,
                    value: self.effective(k).unwrap_or(e.default).to_string(),
                    source: self.perf_source(k)?,
                })
            })
            .collect();
        rows.sort_by_key(|r| r.binding.domain);
        rows
    }

    /// 배터리·원격 세션인데 모드가 full이면 안내 메시지(D-58 · 상태줄 1회 · 호스트가 표시·기억).
    #[must_use]
    pub fn perf_hint(&self) -> Option<Msg> {
        if self.perf_mode() != PerfMode::Full {
            return None;
        }
        let s = signals();
        if s.on_battery == Some(true) {
            Some(Msg::StPerfBatteryHint)
        } else if s.remote_session == Some(true) {
            Some(Msg::StPerfRemoteHint)
        } else {
            None
        }
    }
}
