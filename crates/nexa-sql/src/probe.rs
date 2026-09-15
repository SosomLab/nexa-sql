//! 서버 도달성 신호등(사용자 09-14) — 접속 창 로그인 목록의 첫 컬럼.
//!
//! - **무엇을**: 프로필의 호스트·포트에 TCP 연결만 시도한다(DB 로그인 없음 · SYN 1개 · 즉시 닫음). 파일 DB·호스트 없음은 대상 아님.
//! - **누구를**: **한 번 이상 접속을 시도(Test/Connect)한 프로필**(사용자 09-14 "접속한 적 있는 것만" → 09-16 "시도한 적 있으면" —
//!   서버는 살아 있는데 포트가 안 열리는 대상도 점검) — `<설정 폴더>/connected.list`(이름 한 줄씩)로 기억.
//! - **언제**: 접속 창을 열 때 한 번, 그 뒤 창이 열려 있는 동안 `probe.interval`(기본 60초)마다 **주기 갱신**(사용자 09-14).
//!   실패하면 그 시점부터 **횟수를 누적**하고, 횟수가 쌓일수록 다음 확인까지의 간격을 **지수로 늘린다**(사용자 09-14 — 빠른 재시도 아님):
//!   1회 실패 = `probe.retry_delay`(기본 60초) · 2회 = ×2 · 3회 = ×4 … `probe.max_retries`(기본 5)번 늘어난 뒤엔 그 간격(기본 60×2⁵ = 32분)을 유지.
//!   성공하면 횟수 0 · 다음은 주기. 창이 닫혀 있으면 주기 갱신은 멈춘다.
//! - **즉시 갱신**: 접속 창의 Connect/Test 실패 · SQL 실행 중 **접속성 오류**(`is_connection_error`)가 확인되면 그 서버만 바로 다시 묻는다
//!   (창이 닫혀 있어도 — 다음에 열 때 최신 상태가 보인다 · 사용자 09-14).
//! - **실행 전 빠른 판정**: 워커는 신호등이 초록이 아니거나 직전 실행이 접속성 오류였으면 쿼리를 보내기 전에 같은 `probe_once`로
//!   포트를 먼저 본다 — 죽은 서버에 드라이버 타임아웃(수십 초)을 기다리지 않고 `probe.timeout` 안에 실패를 알린다(사용자 09-14).
//! - **부하·격리**(사용자 09-14): 틱은 **확인이 필요한 대상만**(예약 시각 지남 · 확인 중 아님) 골라 요청하고, 요청마다 **별도 스레드**가
//!   TCP 연결 1개로 판정한다(병렬 · 동시 최대 `MAX_INFLIGHT` · 초과분은 다음 틱). DB 워커 스레드·세션은 전혀 건드리지 않는다 —
//!   실행 중인 쿼리·기존 접속에 영향 0. 타임아웃 `probe.timeout`(기본 2초) · UI 스레드는 요청/결과 전달만.
//!
//! 색 = 초록(포트 연결 가능) · 노랑(확인 중) · **파랑(IP는 살아 있는데 포트가 안 열림** — 연결 거부 또는 ICMP 응답) · 빨강(도달 불가) · 회색(대상 아님/모름).
//! 접속 성공은 신호등을 바꾸지 않는다 — 목적은 **접속 전** 확인이라 다음에 창을 열 때 실제로 다시 묻는다(사용자 09-14).

use std::collections::HashSet;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProbeStatus {
    Unknown,
    Checking,
    /// 포트까지 열림.
    Up,
    /// 호스트는 응답하는데(연결 거부 RST · ICMP 에코 응답) 포트가 닫혀 있다.
    PortClosed,
    /// 도달 불가(타임아웃 · 이름 풀이 실패 · 네트워크 불가).
    Down,
}

/// 한 번의 프로브 결과.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Up,
    PortClosed,
    Down,
}

pub(crate) struct ProbeReq {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
}

pub(crate) struct ProbeResult {
    pub name: String,
    pub outcome: Outcome,
}

/// 동시에 도는 프로브 스레드 상한(프로필 수십 개까지 여유 · 초과분은 다음 틱에).
/// 기본값 — 설정 `probe.max_inflight`가 없을 때(레지스트리 기본과 같다).
pub(crate) const MAX_INFLIGHT: usize = 16;

/// 프로브 허브 — 요청마다 **짧은 스레드 하나**(병렬) · 결과는 `wake` 뒤 [`ProbeHub::try_recv`]로 받는다.
/// DB 워커와 채널·스레드를 공유하지 않는다(실행 중 쿼리·세션에 영향 0).
pub(crate) struct ProbeHub {
    /// 프로브 스레드 → 수집 스레드(결과 전달 + UI 깨우기 · `wake`는 Sync가 아니어도 된다).
    tx_res: mpsc::Sender<ProbeResult>,
    rx: mpsc::Receiver<ProbeResult>,
    inflight: Arc<AtomicUsize>,
    /// 동시 프로브 스레드 상한(설정 `probe.max_inflight` · 기본 [`MAX_INFLIGHT`]).
    max_inflight: usize,
}

impl ProbeHub {
    pub(crate) fn spawn(wake: Box<dyn Fn() + Send>, max_inflight: usize) -> Self {
        let (tx_res, rx_internal) = mpsc::channel::<ProbeResult>();
        let (tx_ui, rx) = mpsc::channel::<ProbeResult>();
        // 수집 스레드 — 결과를 UI 채널로 옮기고 깨운다(대기만 하므로 비용 0 · 잠금 없음).
        let _ = std::thread::Builder::new()
            .name("nsql-probe-collect".into())
            .spawn(move || {
                for r in rx_internal {
                    if tx_ui.send(r).is_err() {
                        break;
                    }
                    wake();
                }
            });
        ProbeHub {
            tx_res,
            rx,
            inflight: Arc::new(AtomicUsize::new(0)),
            max_inflight: if max_inflight == 0 {
                MAX_INFLIGHT
            } else {
                max_inflight
            },
        }
    }

    /// 요청 하나 = 스레드 하나. 상한을 넘으면 `false`(호출자가 예약을 유지해 다음 틱에 다시 보낸다).
    pub(crate) fn request(&self, req: ProbeReq) -> bool {
        if self.inflight.load(Ordering::Relaxed) >= self.max_inflight {
            return false;
        }
        self.inflight.fetch_add(1, Ordering::Relaxed);
        let tx = self.tx_res.clone();
        let inflight = Arc::clone(&self.inflight);
        let spawned = std::thread::Builder::new()
            .name(format!("nsql-probe:{}", req.name))
            .spawn(move || {
                let outcome = probe_once(&req.host, req.port, req.timeout);
                inflight.fetch_sub(1, Ordering::Relaxed);
                let _ = tx.send(ProbeResult {
                    name: req.name,
                    outcome,
                });
            });
        if spawned.is_err() {
            self.inflight.fetch_sub(1, Ordering::Relaxed);
            return false;
        }
        true
    }

    pub(crate) fn try_recv(&self) -> Option<ProbeResult> {
        self.rx.try_recv().ok()
    }
}

/// 이름 풀이(첫 주소) + `connect_timeout`. 성공 = Up(바로 닫음) · **연결 거부** = 호스트 살아 있음(PortClosed) ·
/// 타임아웃/불가 = ICMP 에코 1회(Windows `IcmpSendEcho` · 관리자 권한 불필요)로 호스트 생존을 한 번 더 본다 → 응답이면 PortClosed, 아니면 Down.
pub(crate) fn probe_once(host: &str, port: u16, timeout: Duration) -> Outcome {
    let Ok(mut addrs) = (host, port).to_socket_addrs() else {
        return Outcome::Down;
    };
    let Some(addr) = addrs.next() else {
        return Outcome::Down;
    };
    match TcpStream::connect_timeout(&addr, timeout) {
        Ok(_) => Outcome::Up,
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => Outcome::PortClosed,
        Err(_) => {
            if icmp_alive(addr.ip(), timeout) {
                Outcome::PortClosed
            } else {
                Outcome::Down
            }
        }
    }
}

/// ICMP 에코(IPv4) — Windows는 iphlpapi `IcmpSendEcho`(raw 소켓·관리자 불필요). 다른 OS는 `ping` 1회(없으면 false).
fn icmp_alive(ip: std::net::IpAddr, timeout: Duration) -> bool {
    let std::net::IpAddr::V4(v4) = ip else {
        return false;
    };
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        type Handle = *mut c_void;
        #[link(name = "iphlpapi")]
        extern "system" {
            fn IcmpCreateFile() -> Handle;
            fn IcmpCloseHandle(h: Handle) -> i32;
            fn IcmpSendEcho(
                h: Handle,
                dest: u32,
                req: *const c_void,
                req_size: u16,
                opts: *const c_void,
                reply: *mut c_void,
                reply_size: u32,
                timeout_ms: u32,
            ) -> u32;
        }
        // IPAddr = 네트워크 바이트 순서 u32.
        let dest = u32::from_ne_bytes(v4.octets());
        let data = [0x6Eu8; 8];
        let mut reply = [0u8; 256];
        // SAFETY: 유효한 버퍼 · 핸들은 닫는다 · 호출은 동기.
        unsafe {
            let h = IcmpCreateFile();
            if h.is_null() || h as isize == -1 {
                return false;
            }
            let n = IcmpSendEcho(
                h,
                dest,
                data.as_ptr().cast(),
                data.len() as u16,
                std::ptr::null(),
                reply.as_mut_ptr().cast(),
                reply.len() as u32,
                timeout.as_millis().clamp(100, 10_000) as u32,
            );
            IcmpCloseHandle(h);
            // 응답 1개 이상 + Status(IP_SUCCESS = 0)
            n > 0 && u32::from_ne_bytes([reply[4], reply[5], reply[6], reply[7]]) == 0
        }
    }
    #[cfg(not(windows))]
    {
        let mut args = ping_args(timeout);
        args.push(v4.to_string());
        std::process::Command::new("ping")
            .args(&args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

/// unix `ping` 인자 — ★ OS마다 `-W`의 단위가 다르다(09-16 mac 실기: 파란색이 안 나오던 원인).
/// macOS/BSD: `-W waittime` = **밀리초** · `-t timeout` = 초(전체 상한). Linux(iputils): `-W timeout` = **초**.
/// 리눅스 값(초)을 맥에 그대로 주면 2ms 대기 → 항상 실패 → "서버 살아 있음" 판정이 불가능했다.
#[cfg(not(windows))]
fn ping_args(timeout: Duration) -> Vec<String> {
    let secs = timeout.as_secs().max(1).to_string();
    if cfg!(target_os = "macos") {
        let ms = timeout.as_millis().clamp(100, 60_000).to_string();
        vec!["-c".into(), "1".into(), "-W".into(), ms, "-t".into(), secs]
    } else {
        vec!["-c".into(), "1".into(), "-W".into(), secs]
    }
}

/// 프로브 정책(설정 `probe.*` — 부팅 시 한 번 주입).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ProbePolicy {
    pub enabled: bool,
    /// 실패 간격이 2배씩 늘어나는 최대 횟수(`probe.max_retries`) — 그 뒤엔 `retry_delay × 2^max_retries`로 고정.
    pub max_retries: u32,
    /// 시도 1회 TCP 타임아웃(`probe.timeout`).
    pub timeout: Duration,
    /// 첫 실패 뒤 다음 확인까지의 대기(`probe.retry_delay`) — 실패마다 ×2.
    pub retry_delay: Duration,
    /// 정상(초록)일 때의 주기 갱신 간격(`probe.interval`).
    pub interval: Duration,
}

impl Default for ProbePolicy {
    fn default() -> Self {
        ProbePolicy {
            enabled: true,
            max_retries: 5,
            timeout: Duration::from_secs(2),
            retry_delay: Duration::from_secs(60),
            interval: Duration::from_secs(60),
        }
    }
}

impl ProbePolicy {
    /// 실패 `attempts`회째의 다음 확인 대기 — `retry_delay × 2^(attempts-1)` · `max_retries`번까지만 늘어난다.
    pub(crate) fn failure_wait(&self, attempts: u32) -> Duration {
        let cap = self
            .retry_delay
            .saturating_mul(1u32 << self.max_retries.min(16));
        backoff(attempts, self.retry_delay, cap)
    }
}

/// 재시도 간격 — `base` × 2^(attempt-1) · 상한 `cap`(최소 1초).
pub(crate) fn backoff(attempt: u32, base: Duration, cap: Duration) -> Duration {
    let mul = 1u32 << attempt.saturating_sub(1).min(16);
    base.saturating_mul(mul)
        .min(cap)
        .max(Duration::from_secs(1))
}

/// 실행/접속 오류가 **서버 도달성** 문제로 보이는가(사용자 09-14 — 이때만 신호등을 즉시 갱신).
/// 문법·권한 오류에 프로브를 낭비하지 않도록 보수적으로 고른다: Oracle ORA-코드(네트워크·세션 단절) · TDS/소켓 계열 문구.
pub(crate) fn is_connection_error(code: Option<i64>, message: &str) -> bool {
    // Oracle: 00028/01041 세션 종료 · 03113 EOF · 03114 not connected · 03135 lost contact · 03150 통신 채널 ·
    // 12170 timeout · 12514 service unknown(리스너는 산 상태 → 프로브 가치 있음) · 12541 no listener · 12543/12545 host · 12560 protocol adapter.
    if let Some(c) = code {
        if matches!(
            c,
            28 | 1041 | 3113 | 3114 | 3135 | 3150 | 12170 | 12514 | 12541 | 12543 | 12545 | 12560
        ) {
            return true;
        }
    }
    let m = message.to_ascii_lowercase();
    [
        "connection refused",
        "connection reset",
        "connection closed",
        "connection timed out",
        "connect timeout",
        "timed out",
        "not connected",
        "no listener",
        "tns:",
        "unreachable",
        "broken pipe",
        "end-of-file on communication channel",
        "lost contact",
        "forcibly closed",
        "tds",
        "socket",
        "os error 10060",
        "os error 10061",
        "os error 10054",
        "os error 111",
        "os error 110",
        "os error 104",
        "failed to lookup",
    ]
    .iter()
    .any(|k| m.contains(k))
}

/// 프로필별 신호등 상태 + 재시도 스케줄.
#[derive(Clone, Debug)]
pub(crate) struct ProbeEntry {
    pub status: ProbeStatus,
    pub attempts: u32,
    pub next_at: Option<Instant>,
}

impl ProbeEntry {
    pub(crate) fn fresh(now: Instant) -> Self {
        ProbeEntry {
            status: ProbeStatus::Unknown,
            attempts: 0,
            next_at: Some(now),
        }
    }

    /// 결과 반영 — Up이면 횟수 0 · 다음은 주기. 아니면(포트 닫힘·도달 불가) 횟수 누적 · 간격은 횟수만큼 지수 증가
    /// (`ProbePolicy::failure_wait` · 사용자 09-14 "횟수가 쌓이면 시도 간격을 지수 증가 · 빠르게 시도하는 게 아님").
    pub(crate) fn apply(&mut self, outcome: Outcome, now: Instant, pol: &ProbePolicy) {
        match outcome {
            Outcome::Up => {
                self.status = ProbeStatus::Up;
                self.attempts = 0;
                self.next_at = Some(now + pol.interval);
            }
            Outcome::PortClosed | Outcome::Down => {
                self.status = if outcome == Outcome::Down {
                    ProbeStatus::Down
                } else {
                    ProbeStatus::PortClosed
                };
                self.attempts = self.attempts.saturating_add(1);
                self.next_at = Some(now + pol.failure_wait(self.attempts));
            }
        }
    }

    /// 지금 바로 다시 묻도록 당긴다(누적 횟수는 보존). 이미 확인 중이면 그대로.
    /// (창 재오픈 시 전부 당기던 용도는 09-14에 제거 — 지금은 테스트·수동 갱신 후보용.)
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn poke(&mut self, now: Instant) {
        if self.status != ProbeStatus::Checking {
            self.next_at = Some(now);
        }
    }
}

// ── 한 번 이상 접속을 **시도**한 프로필 기억(`connected.list` · 이름은 09-14 그대로 · 의미는 09-16 확장)

fn list_path() -> Option<std::path::PathBuf> {
    nsql_settings::config_dir().map(|d| d.join("connected.list"))
}

pub(crate) fn connected_names() -> HashSet<String> {
    list_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn mark_connected(name: &str) {
    if name.is_empty() {
        return;
    }
    let mut set = connected_names();
    if !set.insert(name.to_string()) {
        return;
    }
    if let Some(p) = list_path() {
        let mut v: Vec<String> = set.into_iter().collect();
        v.sort();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, v.join("\n") + "\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// macOS는 `-W`가 밀리초(+ `-t` 초) · Linux는 `-W` 초 — 단위를 섞으면 맥에서 ICMP 판정이 항상 실패한다(09-16).
    #[cfg(not(windows))]
    #[test]
    fn ping_args_use_os_units() {
        let a = ping_args(Duration::from_secs(2));
        if cfg!(target_os = "macos") {
            assert_eq!(a, ["-c", "1", "-W", "2000", "-t", "2"]);
        } else {
            assert_eq!(a, ["-c", "1", "-W", "2"]);
        }
    }

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(backoff(1, S(2), S(60)).as_secs(), 2);
        assert_eq!(backoff(2, S(2), S(60)).as_secs(), 4);
        assert_eq!(backoff(4, S(2), S(60)).as_secs(), 16);
        assert_eq!(backoff(20, S(2), S(60)).as_secs(), 60, "상한 = 주기");
        assert_eq!(backoff(1, S(5), S(60)).as_secs(), 5, "base 설정");
        assert_eq!(backoff(3, S(10), S(30)).as_secs(), 30);
    }

    #[test]
    fn entry_accumulates_and_interval_grows_exponentially() {
        let now = Instant::now();
        let pol = ProbePolicy {
            max_retries: 3,
            retry_delay: S(60),
            interval: S(60),
            ..ProbePolicy::default()
        };
        let mut e = ProbeEntry::fresh(now);
        // 1회 60s · 2회 120s · 3회 240s · 4회 480s(= 60×2³ 상한) · 5회도 480s.
        for (i, want) in [60u64, 120, 240, 480, 480].iter().enumerate() {
            e.apply(Outcome::Down, now, &pol);
            assert_eq!(e.status, ProbeStatus::Down);
            assert_eq!(e.attempts, i as u32 + 1, "실패마다 누적");
            assert_eq!(
                e.next_at,
                Some(now + S(*want)),
                "간격 지수 증가 · 상한 유지"
            );
        }
        e.apply(Outcome::PortClosed, now, &pol);
        assert_eq!(e.status, ProbeStatus::PortClosed);
        assert_eq!(e.attempts, 6, "상한 뒤에도 누적");
        assert_eq!(
            e.next_at,
            Some(now + S(480)),
            "상한 간격으로 계속 확인(회복 감지)"
        );
        e.apply(Outcome::Up, now, &pol);
        assert_eq!(e.status, ProbeStatus::Up);
        assert_eq!(e.attempts, 0);
        assert_eq!(e.next_at, Some(now + S(60)), "성공해도 주기 갱신");
        e.poke(now + S(5));
        assert_eq!(e.next_at, Some(now + S(5)), "즉시 갱신 당김");
        e.status = ProbeStatus::Checking;
        e.next_at = None;
        e.poke(now);
        assert_eq!(e.next_at, None, "확인 중이면 중복 요청 없음");
    }

    #[test]
    fn connection_error_classifier() {
        assert!(is_connection_error(
            Some(12541),
            "ORA-12541: TNS:no listener"
        ));
        assert!(is_connection_error(
            Some(3113),
            "end-of-file on communication channel"
        ));
        assert!(is_connection_error(
            None,
            "An existing connection was forcibly closed (os error 10054)"
        ));
        assert!(is_connection_error(None, "connection refused"));
        assert!(!is_connection_error(
            Some(942),
            "ORA-00942: table or view does not exist"
        ));
        assert!(!is_connection_error(
            Some(1017),
            "invalid username/password; logon denied"
        ));
        assert!(!is_connection_error(None, "Incorrect syntax near 'SELEC'."));
    }

    #[test]
    fn closed_port_on_localhost_is_port_closed() {
        // 127.0.0.1:1 — 열려 있을 리 없는 포트 → 연결 거부(호스트는 살아 있다) = PortClosed.
        let t = Instant::now();
        assert_eq!(
            probe_once("127.0.0.1", 1, Duration::from_millis(800)),
            Outcome::PortClosed
        );
        assert!(t.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn open_port_is_up() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = l.local_addr().expect("addr").port();
        assert_eq!(
            probe_once("127.0.0.1", port, Duration::from_millis(800)),
            Outcome::Up
        );
    }
}
