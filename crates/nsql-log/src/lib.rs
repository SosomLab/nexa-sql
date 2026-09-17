//! `nsql-log` — 실행 로그(사용자 09-14 *"전송·실행·최초 응답·처리 rows·완료 시간이 로그에 · 내부 로그 창 · 년월일시분초밀리초 첫 컬럼 ·
//! Raw 기본, 설정에 따라 Markdown/Grid — 어댑터 기법으로 입출력 구조 동일 · 교체 가능한 확장 설계"*).
//!
//! 구조(어댑터):
//! ```text
//! 생산자(러너 이벤트 · 접속 결과 · 앱 정보) ──▶ LogEntry{ts, kind, rows, elapsed, message} ──▶ LogBuffer(링)
//!                                                         │
//!                                                         ▼  trait LogFormat (교체 지점)
//!                                            RawFormat · MarkdownFormat · GridFormat · (파일 · JSON · 원격 … 후속)
//!                                                         ▼
//!                                            소비자(GUI 로그 창 · CLI stderr · 파일 싱크(후속 · I/O 속도 이슈는 배치/스레드로))
//! ```
//! - **입력 구조는 하나**([`LogEntry`]) · **출력 구조도 하나**(`String` 줄 + 선택적 헤더) — 포맷은 [`LogFormat`] 구현을 바꿔 끼운다.
//! - 타임스탬프는 **엔트리 생성 시각(로컬)** — [`now_local`](외부 crate 0 · Windows `GetLocalTime` · Unix `localtime_r`).
//! - 의존 0 · UI 0 · I/O 0(파일 싱크는 이 트레이트 위에 후속 — 배치 flush로 속도 이슈 회피).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::collections::VecDeque;
use std::time::Duration;

/// 로그 구분 — 사용자 관점 실행 단계 + 접속·정보·오류.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogKind {
    Connect,
    Disconnect,
    /// 요청 전송(문장 요약).
    Send,
    /// 서버 실행 → **최초 응답**까지.
    Execute,
    /// 행 페치(처리 rows).
    Fetch,
    /// 서버 메시지 플러시(DBMS_OUTPUT 등 · 줄 수).
    Output,
    Commit,
    /// 완료(총 소요 · rows).
    Done,
    Error,
    Info,
}

impl LogKind {
    /// 전체(표시 필터 메뉴 순서).
    pub const ALL: [LogKind; 10] = [
        LogKind::Connect,
        LogKind::Disconnect,
        LogKind::Send,
        LogKind::Execute,
        LogKind::Fetch,
        LogKind::Output,
        LogKind::Commit,
        LogKind::Done,
        LogKind::Error,
        LogKind::Info,
    ];

    /// 라벨 → 종류(설정 `log.kinds` 파싱 · 대소문자 무관).
    pub fn parse(label: &str) -> Option<LogKind> {
        let l = label.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|k| k.label().eq_ignore_ascii_case(&l))
    }

    /// 고정 폭 라벨(Grid/Raw 정렬용 · 번역하지 않는다).
    pub fn label(self) -> &'static str {
        match self {
            LogKind::Connect => "connect",
            LogKind::Disconnect => "disconn",
            LogKind::Send => "send",
            LogKind::Execute => "execute",
            LogKind::Fetch => "fetch",
            LogKind::Output => "output",
            LogKind::Commit => "commit",
            LogKind::Done => "done",
            LogKind::Error => "ERROR",
            LogKind::Info => "info",
        }
    }
}

/// 로컬 시각(밀리초).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LocalTime {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
    pub ms: u32,
}

impl LocalTime {
    /// `HH:MM:SS.mmm`(12자 · compact/템플릿 `{time}`).
    pub fn time_only(&self) -> String {
        format!(
            "{:02}:{:02}:{:02}.{:03}",
            self.hour, self.min, self.sec, self.ms
        )
    }

    /// `YYYY-MM-DD`.
    pub fn date_only(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// `YYYY-MM-DD HH:MM:SS.mmm`(23자 고정 — 첫 컬럼 정렬).
    pub fn stamp(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
            self.year, self.month, self.day, self.hour, self.min, self.sec, self.ms
        )
    }
}

/// 지금(로컬). 실패하면 UTC.
pub fn now_local() -> LocalTime {
    imp::now_local().unwrap_or_else(now_utc)
}

/// UTC(폴백 · 시간대 조회 실패 시). Howard Hinnant civil_from_days.
pub fn now_utc() -> LocalTime {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    LocalTime {
        year: (if m <= 2 { y + 1 } else { y }) as i32,
        month: m as u32,
        day: day as u32,
        hour: rem / 3600,
        min: (rem % 3600) / 60,
        sec: rem % 60,
        ms: d.subsec_millis(),
    }
}

#[cfg(windows)]
mod imp {
    use super::LocalTime;
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        millis: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(out: *mut SystemTime);
    }
    pub(super) fn now_local() -> Option<LocalTime> {
        let mut st = SystemTime::default();
        // SAFETY: 출력 구조체 포인터만 넘긴다.
        unsafe { GetLocalTime(&mut st) };
        Some(LocalTime {
            year: i32::from(st.year),
            month: u32::from(st.month),
            day: u32::from(st.day),
            hour: u32::from(st.hour),
            min: u32::from(st.minute),
            sec: u32::from(st.second),
            ms: u32::from(st.millis),
        })
    }
}

#[cfg(unix)]
mod imp {
    use super::LocalTime;
    /// `struct tm`의 앞 9개 int는 glibc·musl·macOS 공통 배치(뒤의 gmtoff/zone은 읽지 않는다).
    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        _gmtoff: i64,
        _zone: *const u8,
    }
    extern "C" {
        fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
    }
    pub(super) fn now_local() -> Option<LocalTime> {
        let d = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        let t = d.as_secs() as i64;
        let mut tm = Tm {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 0,
            tm_mon: 0,
            tm_year: 0,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            _gmtoff: 0,
            _zone: std::ptr::null(),
        };
        // SAFETY: time_t 포인터와 충분히 큰 출력 구조체(선두 9 int + gmtoff + zone 포인터).
        let r = unsafe { localtime_r(&t, &mut tm) };
        if r.is_null() {
            return None;
        }
        Some(LocalTime {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u32,
            day: tm.tm_mday as u32,
            hour: tm.tm_hour as u32,
            min: tm.tm_min as u32,
            sec: tm.tm_sec as u32,
            ms: d.subsec_millis(),
        })
    }
}

#[cfg(not(any(windows, unix)))]
mod imp {
    pub(super) fn now_local() -> Option<super::LocalTime> {
        None
    }
}

/// 로그 한 줄 — **입력 구조의 단일 원천**(모든 포맷·싱크가 이것만 받는다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub ts: LocalTime,
    pub kind: LogKind,
    /// 처리 rows(페치·완료) · 줄 수(output).
    pub rows: Option<u64>,
    /// 구간 소요.
    pub elapsed: Option<Duration>,
    pub message: String,
    /// 처리 층(개발자 모드 상세 로그 · docs/48) — 기본 메시지는 `App`.
    pub layer: LogLayer,
    /// 상세 수준 — `Basic`은 늘 보이는 기본 메시지 · 나머지는 개발자 모드 + 마스크가 켜져 있을 때만 **생성**된다.
    pub level: LogLevel,
}

/// 처리 층(docs/48 §2) — 상세 로그 마스크의 축. 순서 = 마스크 비트 그룹.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LogLayer {
    App = 0,
    /// 네트워크(전송 시각·전송량·속도·첫 응답 도착).
    Net = 1,
    /// 서버 실행(첫 응답까지).
    Exec = 2,
    /// 행 페치(시작~완료 · 행·바이트·속도).
    Fetch = 3,
    /// 수신 뒤 데이터 처리(ResultData 구성 · 텍스트 변환 · 렌더 준비).
    Load = 4,
    /// 화면 렌더(시작·종료 시각 · 소요).
    Render = 5,
    Tx = 6,
    /// 메타(탐색기·카탈로그).
    Meta = 7,
}

impl LogLayer {
    pub const ALL: [LogLayer; 8] = [
        LogLayer::App,
        LogLayer::Net,
        LogLayer::Exec,
        LogLayer::Fetch,
        LogLayer::Load,
        LogLayer::Render,
        LogLayer::Tx,
        LogLayer::Meta,
    ];
    pub fn label(self) -> &'static str {
        match self {
            LogLayer::App => "app",
            LogLayer::Net => "net",
            LogLayer::Exec => "exec",
            LogLayer::Fetch => "fetch",
            LogLayer::Load => "load",
            LogLayer::Render => "render",
            LogLayer::Tx => "tx",
            LogLayer::Meta => "meta",
        }
    }
    pub fn parse(s: &str) -> Option<LogLayer> {
        let l = s.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|k| k.label() == l)
    }
}

/// 상세 수준(docs/48 §2) — 층마다 3비트(timing · progress · trace).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LogLevel {
    Basic = 0,
    /// 단계 시각·소요·전송량(실행당 몇 줄).
    Timing = 1,
    /// 진행(페치 배치 · 100ms 간격).
    Progress = 2,
    /// 추적(호출 단위 · 많음).
    Trace = 3,
}

impl LogLevel {
    pub const DETAIL: [LogLevel; 3] = [LogLevel::Timing, LogLevel::Progress, LogLevel::Trace];
    pub fn label(self) -> &'static str {
        match self {
            LogLevel::Basic => "basic",
            LogLevel::Timing => "timing",
            LogLevel::Progress => "progress",
            LogLevel::Trace => "trace",
        }
    }
    pub fn parse(s: &str) -> Option<LogLevel> {
        let l = s.trim().to_ascii_lowercase();
        Self::DETAIL.into_iter().find(|k| k.label() == l)
    }
}

/// ★ 상세 로그 게이트(docs/48 §3) — 층×수준 비트 하나의 원자 정수. 꺼져 있으면(0) 상세 로그 코드는 **load 1회 + 예측 가능한
/// 분기 1개**만 남고 문자열·시각·할당은 전혀 일어나지 않는다. 쓰기는 설정 변경 때만(`Relaxed`로 충분 — 순서 보장 불필요).
static DETAIL_MASK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[inline(always)]
const fn detail_bit(layer: LogLayer, level: LogLevel) -> u32 {
    1u32 << ((layer as u32) * 4 + (level as u32))
}

/// 이 층·수준의 상세 로그를 만들어야 하는가 — **항상 인라인**(호출 0 · 분기 1).
#[inline(always)]
pub fn wants(layer: LogLayer, level: LogLevel) -> bool {
    #[cfg(feature = "devlog")]
    {
        DETAIL_MASK.load(std::sync::atomic::Ordering::Relaxed) & detail_bit(layer, level) != 0
    }
    #[cfg(not(feature = "devlog"))]
    {
        let _ = (layer, level);
        false
    }
}

pub fn set_detail_mask(mask: u32) {
    DETAIL_MASK.store(mask, std::sync::atomic::Ordering::Relaxed);
}

pub fn detail_mask() -> u32 {
    DETAIL_MASK.load(std::sync::atomic::Ordering::Relaxed)
}

/// 설정 `log.dev_layers` → 마스크. 문법: `layer[:level+level]`을 쉼표로 · 수준 생략 = timing+progress+trace ·
/// `*` = 전 층 전 수준 · 빈 문자열 = 0. 예 `net,fetch:timing+progress,render:timing`.
pub fn parse_detail_layers(spec: &str) -> u32 {
    let mut m = 0u32;
    for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (layer_s, levels_s) = match part.split_once(':') {
            Some((a, b)) => (a, Some(b)),
            None => (part, None),
        };
        let layers: Vec<LogLayer> = if layer_s.trim() == "*" {
            LogLayer::ALL.to_vec()
        } else {
            LogLayer::parse(layer_s).into_iter().collect()
        };
        let levels: Vec<LogLevel> = match levels_s {
            Some(ls) => ls.split('+').filter_map(LogLevel::parse).collect(),
            None => LogLevel::DETAIL.to_vec(),
        };
        for l in &layers {
            for v in &levels {
                m |= detail_bit(*l, *v);
            }
        }
    }
    m
}

/// 마스크에 층이 하나라도 켜져 있는가(메뉴 체크 표시).
pub fn layer_in_mask(mask: u32, layer: LogLayer) -> bool {
    LogLevel::DETAIL
        .iter()
        .any(|v| mask & detail_bit(layer, *v) != 0)
}

impl LogEntry {
    pub fn new(kind: LogKind, message: impl Into<String>) -> Self {
        LogEntry {
            ts: now_local(),
            kind,
            rows: None,
            elapsed: None,
            message: message.into(),
            layer: LogLayer::App,
            level: LogLevel::Basic,
        }
    }
    pub fn rows(mut self, n: impl Into<Option<u64>>) -> Self {
        self.rows = n.into();
        self
    }
    pub fn elapsed(mut self, d: impl Into<Option<Duration>>) -> Self {
        self.elapsed = d.into();
        self
    }
    /// 상세 로그 표식(층 · 수준) — 메시지 앞에 `⟨net⟩`.
    pub fn at(mut self, layer: LogLayer, level: LogLevel) -> Self {
        self.layer = layer;
        self.level = level;
        if layer != LogLayer::App {
            self.message = format!("⟨{}⟩ {}", layer.label(), self.message);
        }
        self
    }
}

/// `1.234s` / `12.3ms` / `456µs`.
pub fn fmt_dur(d: Duration) -> String {
    let us = d.as_micros();
    if us >= 1_000_000 {
        format!("{:.3}s", d.as_secs_f64())
    } else if us >= 1_000 {
        format!("{:.1}ms", us as f64 / 1000.0)
    } else {
        format!("{us}µs")
    }
}

/// 출력 어댑터 — **교체 지점**. 새 포맷 = 이 트레이트 구현 1개 + [`formatter`] 표 1줄.
pub trait LogFormat {
    /// 설정 값·표시 이름(`raw` · `markdown` · `grid` · `compact` · `jsonl` · `csv` · `tsv` · `template`).
    fn name(&self) -> &'static str;
    /// 맨 위 헤더 줄(없으면 `None`).
    fn header(&self) -> Option<String> {
        None
    }
    /// 엔트리 한 줄.
    fn line(&self, e: &LogEntry) -> String;
    /// 표시 컬럼 마스크(텍스트 계열만 반영 · 구조형은 무시). 기본 no-op.
    fn set_columns(&mut self, _c: Columns) {}
}

/// 표시 컬럼 마스크(설정 `log.columns` · 로그 창 메뉴) — `message`는 늘 보인다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Columns {
    pub ts: bool,
    pub kind: bool,
    pub rows: bool,
    pub elapsed: bool,
}

impl Default for Columns {
    fn default() -> Self {
        Columns {
            ts: true,
            kind: true,
            rows: true,
            elapsed: true,
        }
    }
}

impl Columns {
    pub const NAMES: [&'static str; 4] = ["time", "kind", "rows", "elapsed"];

    /// `time,kind,rows,elapsed` 중 보일 것(빈 문자열 = 전부).
    pub fn parse(s: &str) -> Columns {
        let s = s.trim();
        if s.is_empty() {
            return Columns::default();
        }
        let has = |n: &str| s.split(',').any(|p| p.trim().eq_ignore_ascii_case(n));
        Columns {
            ts: has("time"),
            kind: has("kind"),
            rows: has("rows"),
            elapsed: has("elapsed"),
        }
    }

    /// 설정 문자열(전부면 빈 문자열).
    pub fn to_setting(self) -> String {
        if self == Columns::default() {
            return String::new();
        }
        let mut v = Vec::new();
        if self.ts {
            v.push("time");
        }
        if self.kind {
            v.push("kind");
        }
        if self.rows {
            v.push("rows");
        }
        if self.elapsed {
            v.push("elapsed");
        }
        v.join(",")
    }

    fn get(self, i: usize) -> bool {
        match i {
            0 => self.ts,
            1 => self.kind,
            2 => self.rows,
            3 => self.elapsed,
            _ => true,
        }
    }

    pub fn toggle(&mut self, name: &str) {
        match name {
            "time" => self.ts = !self.ts,
            "kind" => self.kind = !self.kind,
            "rows" => self.rows = !self.rows,
            "elapsed" => self.elapsed = !self.elapsed,
            _ => {}
        }
    }
}

/// 컬럼 값(모든 포맷이 같은 순서로 쓴다 — ts · kind · rows · elapsed · message).
fn cells(e: &LogEntry) -> [String; 5] {
    [
        e.ts.stamp(),
        e.kind.label().to_string(),
        e.rows.map(|r| r.to_string()).unwrap_or_default(),
        e.elapsed.map(fmt_dur).unwrap_or_default(),
        e.message.replace('\n', " "),
    ]
}

/// 마스크 적용(숨긴 컬럼은 빈 문자열).
fn masked(e: &LogEntry, c: Columns) -> [String; 5] {
    let mut v = cells(e);
    for (i, cell) in v.iter_mut().enumerate().take(4) {
        if !c.get(i) {
            cell.clear();
        }
    }
    v
}

/// Raw(기본): `2026-09-14 15:41:22.123  execute  500 rows  340.1ms  SELECT …`.
#[derive(Debug, Default, Clone, Copy)]
pub struct RawFormat {
    pub cols: Columns,
}

impl LogFormat for RawFormat {
    fn name(&self) -> &'static str {
        "raw"
    }
    fn set_columns(&mut self, c: Columns) {
        self.cols = c;
    }
    fn line(&self, e: &LogEntry) -> String {
        let [ts, kind, rows, el, msg] = masked(e, self.cols);
        let mut s = String::new();
        if self.cols.ts {
            s.push_str(&ts);
        }
        if self.cols.kind {
            if !s.is_empty() {
                s.push_str("  ");
            }
            s.push_str(&format!("{kind:<8}"));
        }
        if !rows.is_empty() {
            s.push_str(&format!(
                "  {rows} {}",
                if e.kind == LogKind::Output {
                    "lines"
                } else {
                    "rows"
                }
            ));
        }
        if !el.is_empty() {
            s.push_str(&format!("  {el}"));
        }
        if !msg.is_empty() {
            s.push_str("  ");
            s.push_str(&msg);
        }
        s
    }
}

/// Markdown 표.
#[derive(Debug, Default, Clone, Copy)]
pub struct MarkdownFormat {
    pub cols: Columns,
}

impl LogFormat for MarkdownFormat {
    fn name(&self) -> &'static str {
        "markdown"
    }
    fn set_columns(&mut self, c: Columns) {
        self.cols = c;
    }
    fn header(&self) -> Option<String> {
        let mut h = String::from("|");
        let mut r = String::from("|");
        for (i, n) in Columns::NAMES.iter().enumerate() {
            if self.cols.get(i) {
                h.push_str(&format!(" {n} |"));
                r.push_str(if i >= 2 { "--:|" } else { "---|" });
            }
        }
        h.push_str(" message |");
        r.push_str("---|");
        Some(format!("{h}\n{r}"))
    }
    fn line(&self, e: &LogEntry) -> String {
        let v = cells(e);
        let mut s = String::from("|");
        for (i, cell) in v.iter().enumerate().take(4) {
            if self.cols.get(i) {
                s.push_str(&format!(" {cell} |"));
            }
        }
        s.push_str(&format!(" {} |", v[4].replace('|', "\\|")));
        s
    }
}

/// Grid: 고정 폭 컬럼(그리드 컨트롤이 오기 전의 텍스트 정렬판 — 컬럼 경계 `│`).
#[derive(Debug, Default, Clone, Copy)]
pub struct GridFormat {
    pub cols: Columns,
}

impl GridFormat {
    pub const WIDTHS: [usize; 4] = [23, 8, 8, 9];

    fn row(&self, v: [String; 5]) -> String {
        let w = Self::WIDTHS;
        let mut s = String::new();
        for i in 0..4 {
            if !self.cols.get(i) {
                continue;
            }
            let cell = &v[i];
            if i >= 2 {
                s.push_str(&format!("{cell:>w$}│", w = w[i]));
            } else {
                s.push_str(&format!("{cell:<w$}│", w = w[i]));
            }
        }
        s.push_str(&v[4]);
        s
    }
}

impl LogFormat for GridFormat {
    fn name(&self) -> &'static str {
        "grid"
    }
    fn set_columns(&mut self, c: Columns) {
        self.cols = c;
    }
    fn header(&self) -> Option<String> {
        Some(self.row([
            "time".into(),
            "kind".into(),
            "rows".into(),
            "elapsed".into(),
            "message".into(),
        ]))
    }
    fn line(&self, e: &LogEntry) -> String {
        self.row(cells(e))
    }
}

/// Compact: `15:41:22.123 execute  SELECT …`(시각만 · rows/elapsed는 뒤에 괄호).
#[derive(Debug, Default, Clone, Copy)]
pub struct CompactFormat {
    pub cols: Columns,
}

impl LogFormat for CompactFormat {
    fn name(&self) -> &'static str {
        "compact"
    }
    fn set_columns(&mut self, c: Columns) {
        self.cols = c;
    }
    fn line(&self, e: &LogEntry) -> String {
        let [_, kind, rows, el, msg] = cells(e);
        let mut s = String::new();
        if self.cols.ts {
            s.push_str(&e.ts.time_only());
            s.push(' ');
        }
        if self.cols.kind {
            s.push_str(&format!("{kind:<7} "));
        }
        s.push_str(&msg);
        let mut tail = Vec::new();
        if self.cols.rows && !rows.is_empty() {
            tail.push(format!("{rows} rows"));
        }
        if self.cols.elapsed && !el.is_empty() {
            tail.push(el);
        }
        if !tail.is_empty() {
            s.push_str(&format!(" ({})", tail.join(" · ")));
        }
        s
    }
}

/// JSON Lines: 줄마다 객체(`ts` · `kind` · `rows` · `elapsed_ms` · `message`) — 도구 연계용.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonlFormat;

fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

impl LogFormat for JsonlFormat {
    fn name(&self) -> &'static str {
        "jsonl"
    }
    fn line(&self, e: &LogEntry) -> String {
        format!(
            "{{\"ts\":{},\"kind\":{},\"rows\":{},\"elapsed_ms\":{},\"message\":{}}}",
            json_str(&e.ts.stamp()),
            json_str(e.kind.label()),
            e.rows.map_or("null".to_string(), |r| r.to_string()),
            e.elapsed.map_or("null".to_string(), |d| format!(
                "{:.1}",
                d.as_secs_f64() * 1000.0
            )),
            json_str(&e.message)
        )
    }
}

/// CSV/TSV(RFC 4180 따옴표 · 헤더 1줄).
#[derive(Debug, Clone, Copy)]
pub struct DelimitedFormat {
    pub delim: char,
}

impl DelimitedFormat {
    fn q(&self, s: &str) -> String {
        if s.contains(self.delim) || s.contains('"') || s.contains('\n') {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    }
}

impl LogFormat for DelimitedFormat {
    fn name(&self) -> &'static str {
        if self.delim == '\t' {
            "tsv"
        } else {
            "csv"
        }
    }
    fn header(&self) -> Option<String> {
        Some(["time", "kind", "rows", "elapsed", "message"].join(&self.delim.to_string()))
    }
    fn line(&self, e: &LogEntry) -> String {
        cells(e)
            .iter()
            .map(|c| self.q(c))
            .collect::<Vec<_>>()
            .join(&self.delim.to_string())
    }
}

/// 사용자 템플릿: `{ts}` · `{time}` · `{date}` · `{kind}` · `{rows}` · `{elapsed}` · `{msg}` + 폭 `{kind:<8}`/`{rows:>6}`.
#[derive(Debug, Clone)]
pub struct TemplateFormat {
    pub template: String,
}

/// 기본 템플릿(설정 `log.template`).
pub const DEFAULT_TEMPLATE: &str = "{time} {kind:<8} {msg}";

impl TemplateFormat {
    fn value(e: &LogEntry, key: &str) -> Option<String> {
        Some(match key {
            "ts" => e.ts.stamp(),
            "time" => e.ts.time_only(),
            "date" => e.ts.date_only(),
            "kind" => e.kind.label().to_string(),
            "rows" => e.rows.map(|r| r.to_string()).unwrap_or_default(),
            "elapsed" => e.elapsed.map(fmt_dur).unwrap_or_default(),
            "msg" | "message" => e.message.replace('\n', " "),
            _ => return None,
        })
    }
}

impl LogFormat for TemplateFormat {
    fn name(&self) -> &'static str {
        "template"
    }
    fn line(&self, e: &LogEntry) -> String {
        let t = &self.template;
        let mut out = String::with_capacity(t.len() + 64);
        let mut rest = t.as_str();
        while let Some(i) = rest.find('{') {
            out.push_str(&rest[..i]);
            let Some(j) = rest[i..].find('}') else {
                out.push_str(&rest[i..]);
                rest = "";
                break;
            };
            let spec = &rest[i + 1..i + j];
            let (key, pad) = spec.split_once(':').unwrap_or((spec, ""));
            match Self::value(e, key.trim()) {
                Some(v) => {
                    let (align, n) = if let Some(n) = pad.strip_prefix('<') {
                        ('<', n.parse::<usize>().unwrap_or(0))
                    } else if let Some(n) = pad.strip_prefix('>') {
                        ('>', n.parse::<usize>().unwrap_or(0))
                    } else {
                        ('<', 0)
                    };
                    let w = v.chars().count();
                    if align == '>' && w < n {
                        out.push_str(&" ".repeat(n - w));
                    }
                    out.push_str(&v);
                    if align == '<' && w < n {
                        out.push_str(&" ".repeat(n - w));
                    }
                }
                None => {
                    out.push('{');
                    out.push_str(spec);
                    out.push('}');
                }
            }
            rest = &rest[i + j + 1..];
        }
        out.push_str(rest);
        out
    }
}

/// 이름 → 포맷(설정 `log.format`). 모르면 Raw.
pub fn formatter(name: &str) -> Box<dyn LogFormat> {
    formatter_with(name, DEFAULT_TEMPLATE, Columns::default())
}

/// 이름 + 템플릿 + 컬럼 마스크 → 포맷(`Send` — 싱크 스레드에도 쓴다).
pub fn formatter_with(name: &str, template: &str, cols: Columns) -> Box<dyn LogFormat + Send> {
    let mut f: Box<dyn LogFormat + Send> = match name.trim().to_ascii_lowercase().as_str() {
        "markdown" | "md" => Box::new(MarkdownFormat::default()),
        "grid" => Box::new(GridFormat::default()),
        "compact" => Box::new(CompactFormat::default()),
        "jsonl" | "json" => Box::new(JsonlFormat),
        "csv" => Box::new(DelimitedFormat { delim: ',' }),
        "tsv" => Box::new(DelimitedFormat { delim: '\t' }),
        "template" | "custom" => Box::new(TemplateFormat {
            template: if template.trim().is_empty() {
                DEFAULT_TEMPLATE.to_string()
            } else {
                template.to_string()
            },
        }),
        _ => Box::new(RawFormat::default()),
    };
    f.set_columns(cols);
    f
}

/// 포맷 이름 목록(설정 후보).
pub const FORMAT_NAMES: [&str; 8] = [
    "raw", "markdown", "grid", "compact", "jsonl", "csv", "tsv", "template",
];

/// 링 버퍼(상한 초과 시 오래된 것부터 버림 — 메모리 상시 상한).
#[derive(Debug)]
pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
    cap: usize,
}

impl LogBuffer {
    pub fn new(cap: usize) -> Self {
        LogBuffer {
            entries: VecDeque::with_capacity(cap.min(1024)),
            cap: cap.max(1),
        }
    }
    pub fn push(&mut self, e: LogEntry) {
        if self.entries.len() >= self.cap {
            self.entries.pop_front();
        }
        self.entries.push_back(e);
    }
    /// 상한(줄 수).
    pub fn cap(&self) -> usize {
        self.cap
    }
    /// 상한을 바꾼다(0은 1로 · 설정 `log.max_lines` · docs/39 T-90d) — 넘치는 만큼 **앞(오래된 것)에서 즉시 버리고** 버린 수를 돌려준다.
    pub fn set_cap(&mut self, cap: usize) -> usize {
        self.cap = cap.max(1);
        let drop_n = self.entries.len().saturating_sub(self.cap);
        for _ in 0..drop_n {
            self.entries.pop_front();
        }
        drop_n
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &LogEntry> {
        self.entries.iter()
    }
    pub fn get(&self, i: usize) -> Option<&LogEntry> {
        self.entries.get(i)
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

// ────────────────────────────────────────────── 독립 I/O(사용자 09-14 "로그가 성능·메인 프로세스에 영향 없게")

/// 로그 싱크(파일 · stderr · 원격 …) — **로그 스레드에서만** 호출된다. I/O는 여기서만.
pub trait LogSink: Send {
    fn name(&self) -> &'static str;
    /// 한 줄 기록. 오류는 반환만(허브가 세고 계속 간다 — 메인 경로에 전파하지 않는다).
    fn write(&mut self, e: &LogEntry) -> Result<(), String>;
    /// 배치 끝(파일 flush 등). 기본 no-op.
    fn flush(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// stderr 싱크(CLI `--log`) — 포맷터를 품는다.
pub struct StderrSink {
    fmt: Box<dyn LogFormat + Send>,
}

impl std::fmt::Debug for StderrSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StderrSink")
            .field("fmt", &self.fmt.name())
            .finish()
    }
}

impl StderrSink {
    pub fn new(fmt: Box<dyn LogFormat + Send>) -> Self {
        if let Some(h) = fmt.header() {
            eprintln!("{h}");
        }
        StderrSink { fmt }
    }
}

impl LogSink for StderrSink {
    fn name(&self) -> &'static str {
        "stderr"
    }
    fn write(&mut self, e: &LogEntry) -> Result<(), String> {
        use std::io::Write as _;
        let mut err = std::io::stderr().lock();
        writeln!(err, "{}", self.fmt.line(e)).map_err(|e| e.to_string())
    }
}

/// 파일 싱크(배치 flush · 단순 롤링: 크기를 넘으면 `<path>.1`로 밀고 새로 시작 · 설정 `log.file`/`log.file_max_kb`).
pub struct FileSink {
    path: std::path::PathBuf,
    fmt: Box<dyn LogFormat + Send>,
    max_bytes: u64,
    file: std::io::BufWriter<std::fs::File>,
    written: u64,
}

impl std::fmt::Debug for FileSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileSink")
            .field("path", &self.path)
            .field("fmt", &self.fmt.name())
            .finish()
    }
}

impl FileSink {
    /// 열기(이어 쓰기 · 새 파일이면 헤더).
    pub fn open(
        path: impl Into<std::path::PathBuf>,
        fmt: Box<dyn LogFormat + Send>,
        max_bytes: u64,
    ) -> std::io::Result<Self> {
        let path = path.into();
        let (file, written) = Self::open_file(&path, fmt.as_ref())?;
        Ok(FileSink {
            path,
            fmt,
            max_bytes,
            file,
            written,
        })
    }

    fn open_file(
        path: &std::path::Path,
        fmt: &dyn LogFormat,
    ) -> std::io::Result<(std::io::BufWriter<std::fs::File>, u64)> {
        use std::io::Write as _;
        let existed = path.exists();
        let f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        let mut w = std::io::BufWriter::new(f);
        if !existed || len == 0 {
            if let Some(h) = fmt.header() {
                writeln!(w, "{h}")?;
            }
        }
        Ok((w, len))
    }

    fn rotate(&mut self) -> std::io::Result<()> {
        use std::io::Write as _;
        self.file.flush()?;
        let bak = self.path.with_extension(match self.path.extension() {
            Some(e) => format!("{}.1", e.to_string_lossy()),
            None => "1".into(),
        });
        let _ = std::fs::remove_file(&bak);
        std::fs::rename(&self.path, &bak)?;
        let (file, written) = Self::open_file(&self.path, self.fmt.as_ref())?;
        self.file = file;
        self.written = written;
        Ok(())
    }
}

impl LogSink for FileSink {
    fn name(&self) -> &'static str {
        "file"
    }
    fn write(&mut self, e: &LogEntry) -> Result<(), String> {
        use std::io::Write as _;
        if self.max_bytes > 0 && self.written > self.max_bytes {
            self.rotate().map_err(|e| e.to_string())?;
        }
        let line = self.fmt.line(e);
        writeln!(self.file, "{line}").map_err(|e| e.to_string())?;
        self.written += line.len() as u64 + 1;
        Ok(())
    }
    fn flush(&mut self) -> Result<(), String> {
        use std::io::Write as _;
        self.file.flush().map_err(|e| e.to_string())
    }
}

/// 허브 통계(드롭·싱크 오류 — 상태줄·진단용).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HubStats {
    pub sent: u64,
    pub dropped: u64,
    pub sink_errors: u64,
}

/// ★ 로그 허브 — **생산자는 절대 막히지 않는다**(bounded 채널 `try_send` · 가득 차면 버리고 센다) ·
/// 싱크는 **별도 스레드**에서 돌고 패닉은 `catch_unwind`로 가둔다 · 스레드가 죽어도 생산자는 드롭만 늘 뿐 계속 산다.
/// 메모리 상한 = 채널 용량 × 엔트리. 파일 싱크는 이 위에 배치 flush로 붙인다(후속).
pub struct LogHub {
    tx: std::sync::mpsc::SyncSender<LogEntry>,
    stats: std::sync::Arc<std::sync::Mutex<HubStats>>,
    _thread: Option<std::thread::JoinHandle<()>>,
}

impl std::fmt::Debug for LogHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogHub")
            .field("stats", &self.stats())
            .finish()
    }
}

impl LogHub {
    /// 싱크 목록으로 허브를 띄운다(싱크 0개면 스레드는 곧바로 소비만 한다).
    pub fn spawn(sinks: Vec<Box<dyn LogSink>>, capacity: usize) -> Self {
        let (tx, rx) = std::sync::mpsc::sync_channel::<LogEntry>(capacity.max(16));
        let stats = std::sync::Arc::new(std::sync::Mutex::new(HubStats::default()));
        let st = stats.clone();
        let thread = std::thread::Builder::new()
            .name("nsql-log".into())
            .spawn(move || {
                let mut sinks = sinks;
                while let Ok(e) = rx.recv() {
                    for s in sinks.iter_mut() {
                        let r =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.write(&e)));
                        if !matches!(r, Ok(Ok(()))) {
                            if let Ok(mut g) = st.lock() {
                                g.sink_errors += 1;
                            }
                        }
                    }
                    // 큐가 비면 flush(배치 경계) — 파일 싱크의 fsync 비용을 묶는다.
                    if rx
                        .try_recv()
                        .map(|next| {
                            for s in sinks.iter_mut() {
                                let _ =
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        s.write(&next)
                                    }));
                            }
                        })
                        .is_err()
                    {
                        for s in sinks.iter_mut() {
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                s.flush()
                            }));
                        }
                    }
                }
            })
            .ok();
        LogHub {
            tx,
            stats,
            _thread: thread,
        }
    }

    /// 비차단 전송. 가득 차거나 스레드가 죽었으면 버리고 통계만 올린다.
    pub fn push(&self, e: LogEntry) {
        let ok = self.tx.try_send(e).is_ok();
        if let Ok(mut g) = self.stats.lock() {
            if ok {
                g.sent += 1;
            } else {
                g.dropped += 1;
            }
        }
    }

    pub fn stats(&self) -> HubStats {
        self.stats.lock().map(|g| *g).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 마스크 0 = 아무 층도 원하지 않음 · 문법 파싱 · 층 체크.
    #[test]
    fn detail_mask_parse_and_gate() {
        set_detail_mask(0);
        assert!(!wants(LogLayer::Net, LogLevel::Timing));
        let m = parse_detail_layers("net, fetch:timing+progress ,render:timing,bogus");
        set_detail_mask(m);
        assert!(wants(LogLayer::Net, LogLevel::Trace));
        assert!(wants(LogLayer::Fetch, LogLevel::Progress));
        assert!(!wants(LogLayer::Fetch, LogLevel::Trace));
        assert!(wants(LogLayer::Render, LogLevel::Timing));
        assert!(!wants(LogLayer::Load, LogLevel::Timing));
        assert!(layer_in_mask(m, LogLayer::Render) && !layer_in_mask(m, LogLayer::Load));
        assert_eq!(
            parse_detail_layers("*"),
            parse_detail_layers("app,net,exec,fetch,load,render,tx,meta")
        );
        set_detail_mask(0);
        let e = LogEntry::new(LogKind::Info, "x").at(LogLayer::Net, LogLevel::Timing);
        assert_eq!(e.message, "⟨net⟩ x");
        assert_eq!(e.level, LogLevel::Timing);
    }

    fn e() -> LogEntry {
        LogEntry {
            ts: LocalTime {
                year: 2026,
                month: 9,
                day: 14,
                hour: 15,
                min: 41,
                sec: 22,
                ms: 123,
            },
            kind: LogKind::Fetch,
            rows: Some(500),
            elapsed: Some(Duration::from_micros(340_100)),
            message: "SELECT * FROM t".into(),
            layer: LogLayer::App,
            level: LogLevel::Basic,
        }
    }

    #[test]
    fn stamp_is_fixed_width() {
        assert_eq!(e().ts.stamp(), "2026-09-14 15:41:22.123");
        assert_eq!(e().ts.stamp().len(), 23);
    }

    #[test]
    fn raw_markdown_grid_lines() {
        assert_eq!(
            RawFormat::default().line(&e()),
            "2026-09-14 15:41:22.123  fetch     500 rows  340.1ms  SELECT * FROM t"
        );
        assert_eq!(
            MarkdownFormat::default().line(&e()),
            "| 2026-09-14 15:41:22.123 | fetch | 500 | 340.1ms | SELECT * FROM t |"
        );
        let g = GridFormat::default().line(&e());
        assert!(
            g.starts_with("2026-09-14 15:41:22.123│fetch   │     500│  340.1ms│SELECT"),
            "{g}"
        );
        assert!(GridFormat::default().header().unwrap().starts_with("time"));
        assert!(RawFormat::default().header().is_none());
    }

    #[test]
    fn factory_and_names() {
        for n in FORMAT_NAMES {
            assert_eq!(formatter(n).name(), n);
        }
        assert_eq!(formatter("??").name(), "raw");
        assert_eq!(formatter("MD").name(), "markdown");
    }

    #[test]
    fn ring_buffer_caps() {
        let mut b = LogBuffer::new(3);
        for i in 0..5 {
            b.push(LogEntry::new(LogKind::Info, i.to_string()));
        }
        assert_eq!(b.len(), 3);
        assert_eq!(b.get(0).unwrap().message, "2");
    }

    /// T-90d(docs/39 §3-6 `log.max_lines`): 상한을 줄이면 앞(오래된 것)에서 즉시 버린다 · 늘리면 그대로 · 0은 1로.
    #[test]
    fn ring_buffer_set_cap_trims_front() {
        let mut b = LogBuffer::new(10);
        for i in 0..8 {
            b.push(LogEntry::new(LogKind::Info, i.to_string()));
        }
        assert_eq!(b.set_cap(3), 5, "5줄을 버렸다");
        assert_eq!(b.len(), 3);
        assert_eq!(b.get(0).unwrap().message, "5");
        assert_eq!(b.cap(), 3);
        b.push(LogEntry::new(LogKind::Info, "x"));
        assert_eq!(b.len(), 3);
        assert_eq!(b.get(2).unwrap().message, "x");
        assert_eq!(b.set_cap(100), 0);
        assert_eq!(b.len(), 3);
        assert_eq!(b.set_cap(0), 2);
        assert_eq!(b.cap(), 1);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn now_local_is_sane() {
        let t = now_local();
        assert!(t.year >= 2026 && (1..=12).contains(&t.month) && (1..=31).contains(&t.day));
        assert!(t.hour < 24 && t.min < 60 && t.sec < 60 && t.ms < 1000);
        let u = now_utc();
        assert!(u.year >= 2026);
    }

    struct Failing(u32);
    impl LogSink for Failing {
        fn name(&self) -> &'static str {
            "failing"
        }
        fn write(&mut self, _: &LogEntry) -> Result<(), String> {
            self.0 += 1;
            if self.0 % 2 == 0 {
                panic!("sink panic must not escape");
            }
            Err("boom".into())
        }
    }

    #[test]
    fn hub_never_blocks_and_contains_sink_failures() {
        let hub = LogHub::spawn(vec![Box::new(Failing(0))], 16);
        for i in 0..200 {
            hub.push(LogEntry::new(LogKind::Info, i.to_string()));
        }
        std::thread::sleep(Duration::from_millis(50));
        let st = hub.stats();
        assert_eq!(st.sent + st.dropped, 200);
        assert!(st.sink_errors > 0, "{st:?}");
    }
}

#[cfg(test)]
mod format_ext_tests {
    use super::*;

    fn entry() -> LogEntry {
        let mut e = LogEntry::new(LogKind::Fetch, "SELECT 1\nFROM dual");
        e.ts = LocalTime {
            year: 2026,
            month: 9,
            day: 16,
            hour: 14,
            min: 15,
            sec: 19,
            ms: 907,
        };
        e.rows(201u64).elapsed(Duration::from_millis(33))
    }

    #[test]
    fn new_formats_and_template() {
        let e = entry();
        assert_eq!(
            formatter("compact").line(&e),
            "14:15:19.907 fetch   SELECT 1 FROM dual (201 rows · 33.0ms)"
        );
        let j = formatter("jsonl").line(&e);
        assert!(j.starts_with("{\"ts\":\"2026-09-16 14:15:19.907\",\"kind\":\"fetch\",\"rows\":201,\"elapsed_ms\":33.0,"), "{j}");
        assert!(j.contains("\"message\":\"SELECT 1\\nFROM dual\""), "{j}");
        let c = formatter("csv");
        assert_eq!(
            c.header().as_deref(),
            Some("time,kind,rows,elapsed,message")
        );
        assert!(
            c.line(&e).ends_with(",\"SELECT 1 FROM dual\"")
                || c.line(&e).ends_with(",SELECT 1 FROM dual"),
            "{}",
            c.line(&e)
        );
        let t = formatter_with(
            "template",
            "[{kind:>7}] {time} {msg} r={rows}",
            Columns::default(),
        );
        assert_eq!(
            t.line(&e),
            "[  fetch] 14:15:19.907 SELECT 1 FROM dual r=201"
        );
        // 컬럼 마스크: raw에서 시각·rows 숨김
        let m = formatter_with("raw", "", Columns::parse("kind,elapsed"));
        assert_eq!(m.line(&e), "fetch     33.0ms  SELECT 1 FROM dual");
        assert_eq!(Columns::parse("").to_setting(), "");
        assert_eq!(Columns::parse("time,kind").to_setting(), "time,kind");
        assert_eq!(LogKind::parse("ERROR"), Some(LogKind::Error));
    }

    #[test]
    fn file_sink_appends_and_rotates() {
        let dir = std::env::temp_dir().join(format!("nsql-log-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("app.log");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(dir.join("app.log.1"));
        let mut sink =
            FileSink::open(&path, formatter_with("csv", "", Columns::default()), 120).unwrap();
        for _ in 0..6 {
            sink.write(&entry()).unwrap();
        }
        sink.flush().unwrap();
        let cur = std::fs::read_to_string(&path).unwrap();
        assert!(
            cur.starts_with("time,kind,rows,elapsed,message\n"),
            "새 파일마다 헤더: {cur}"
        );
        assert!(dir.join("app.log.1").exists(), "상한을 넘으면 .1로 회전");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
