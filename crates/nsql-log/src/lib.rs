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
}

impl LogEntry {
    pub fn new(kind: LogKind, message: impl Into<String>) -> Self {
        LogEntry {
            ts: now_local(),
            kind,
            rows: None,
            elapsed: None,
            message: message.into(),
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
    /// 설정 값·표시 이름(`raw` · `markdown` · `grid`).
    fn name(&self) -> &'static str;
    /// 맨 위 헤더 줄(없으면 `None`).
    fn header(&self) -> Option<String> {
        None
    }
    /// 엔트리 한 줄.
    fn line(&self, e: &LogEntry) -> String;
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

/// Raw(기본): `2026-09-14 15:41:22.123  execute  500 rows  340.1ms  SELECT …`.
#[derive(Debug, Default, Clone, Copy)]
pub struct RawFormat;

impl LogFormat for RawFormat {
    fn name(&self) -> &'static str {
        "raw"
    }
    fn line(&self, e: &LogEntry) -> String {
        let [ts, kind, rows, el, msg] = cells(e);
        let mut s = format!("{ts}  {kind:<8}");
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
pub struct MarkdownFormat;

impl LogFormat for MarkdownFormat {
    fn name(&self) -> &'static str {
        "markdown"
    }
    fn header(&self) -> Option<String> {
        Some("| time | kind | rows | elapsed | message |\n|---|---|--:|--:|---|".into())
    }
    fn line(&self, e: &LogEntry) -> String {
        let [ts, kind, rows, el, msg] = cells(e);
        format!(
            "| {ts} | {kind} | {rows} | {el} | {} |",
            msg.replace('|', "\\|")
        )
    }
}

/// Grid: 고정 폭 컬럼(그리드 컨트롤이 오기 전의 텍스트 정렬판 — 컬럼 경계 `│`).
#[derive(Debug, Default, Clone, Copy)]
pub struct GridFormat;

impl GridFormat {
    pub const WIDTHS: [usize; 4] = [23, 8, 8, 9];
}

impl LogFormat for GridFormat {
    fn name(&self) -> &'static str {
        "grid"
    }
    fn header(&self) -> Option<String> {
        let w = Self::WIDTHS;
        Some(format!(
            "{:<w0$}│{:<w1$}│{:>w2$}│{:>w3$}│message",
            "time",
            "kind",
            "rows",
            "elapsed",
            w0 = w[0],
            w1 = w[1],
            w2 = w[2],
            w3 = w[3]
        ))
    }
    fn line(&self, e: &LogEntry) -> String {
        let [ts, kind, rows, el, msg] = cells(e);
        let w = Self::WIDTHS;
        format!(
            "{ts:<w0$}│{kind:<w1$}│{rows:>w2$}│{el:>w3$}│{msg}",
            w0 = w[0],
            w1 = w[1],
            w2 = w[2],
            w3 = w[3]
        )
    }
}

/// 이름 → 포맷(설정 `log.format`). 모르면 Raw.
pub fn formatter(name: &str) -> Box<dyn LogFormat> {
    match name.trim().to_ascii_lowercase().as_str() {
        "markdown" | "md" => Box::new(MarkdownFormat),
        "grid" => Box::new(GridFormat),
        _ => Box::new(RawFormat),
    }
}

/// 포맷 이름 목록(설정 후보).
pub const FORMAT_NAMES: [&str; 3] = ["raw", "markdown", "grid"];

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
        if self.entries.len() == self.cap {
            self.entries.pop_front();
        }
        self.entries.push_back(e);
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
            RawFormat.line(&e()),
            "2026-09-14 15:41:22.123  fetch     500 rows  340.1ms  SELECT * FROM t"
        );
        assert_eq!(
            MarkdownFormat.line(&e()),
            "| 2026-09-14 15:41:22.123 | fetch | 500 | 340.1ms | SELECT * FROM t |"
        );
        let g = GridFormat.line(&e());
        assert!(
            g.starts_with("2026-09-14 15:41:22.123│fetch   │     500│  340.1ms│SELECT"),
            "{g}"
        );
        assert!(GridFormat.header().unwrap().starts_with("time"));
        assert!(RawFormat.header().is_none());
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
