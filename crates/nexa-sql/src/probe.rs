//! 서버 도달성 신호등(사용자 09-14) — 접속 창 로그인 목록의 첫 컬럼.
//!
//! - **무엇을**: 프로필의 호스트·포트에 TCP 연결만 시도한다(DB 로그인 없음 · SYN 1개 · 즉시 닫음). 파일 DB·호스트 없음은 대상 아님.
//! - **누구를**: **한 번 이상 접속에 성공한 프로필만**(사용자 09-14) — `<설정 폴더>/connected.list`(이름 한 줄씩)로 기억.
//! - **언제**: 접속 창을 열 때 한 번. 실패하면 2초 → 4 → 8 … **지수 증가**(상한 60초)로 재시도하되 `probe.max_retries`(기본 5)를
//!   넘기면 멈춘다(빨강 고정). 성공하면 다시 묻지 않는다(창을 다시 열 때만). 창이 닫혀 있으면 아무것도 하지 않는다.
//! - **부하**: 별도 스레드 하나가 **순차**로 처리(동시 연결 0) · 타임아웃 `probe.timeout`(기본 2초) · UI 스레드는 요청/결과 전달만.
//!
//! 색 = 초록(포트 연결 가능) · 노랑(확인 중) · **파랑(IP는 살아 있는데 포트가 안 열림** — 연결 거부 또는 ICMP 응답) · 빨강(도달 불가) · 회색(대상 아님/모름).
//! 접속 성공은 신호등을 바꾸지 않는다 — 목적은 **접속 전** 확인이라 다음에 창을 열 때 실제로 다시 묻는다(사용자 09-14).

use std::collections::HashSet;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc;
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

/// 프로브 스레드 핸들 — 요청은 큐에 넣고, 결과는 `wake` 뒤 [`ProbeHub::try_recv`]로 받는다.
pub(crate) struct ProbeHub {
    tx: mpsc::Sender<ProbeReq>,
    rx: mpsc::Receiver<ProbeResult>,
}

impl ProbeHub {
    pub(crate) fn spawn(wake: Box<dyn Fn() + Send>) -> Self {
        let (tx, rx_req) = mpsc::channel::<ProbeReq>();
        let (tx_res, rx) = mpsc::channel::<ProbeResult>();
        let _ = std::thread::Builder::new()
            .name("nsql-probe".into())
            .spawn(move || {
                // 순차 처리 — 한 번에 연결 하나(네트워크 부하 상한).
                for req in rx_req {
                    let outcome = probe_once(&req.host, req.port, req.timeout);
                    if tx_res
                        .send(ProbeResult {
                            name: req.name,
                            outcome,
                        })
                        .is_err()
                    {
                        break;
                    }
                    wake();
                }
            });
        ProbeHub { tx, rx }
    }

    pub(crate) fn request(&self, req: ProbeReq) {
        let _ = self.tx.send(req);
    }

    pub(crate) fn try_recv(&self) -> Option<ProbeResult> {
        self.rx.try_recv().ok()
    }
}

/// 이름 풀이(첫 주소) + `connect_timeout`. 성공 = Up(바로 닫음) · **연결 거부** = 호스트 살아 있음(PortClosed) ·
/// 타임아웃/불가 = ICMP 에코 1회(Windows `IcmpSendEcho` · 관리자 권한 불필요)로 호스트 생존을 한 번 더 본다 → 응답이면 PortClosed, 아니면 Down.
fn probe_once(host: &str, port: u16, timeout: Duration) -> Outcome {
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
        let secs = timeout.as_secs().max(1).to_string();
        std::process::Command::new("ping")
            .args(["-c", "1", "-W", &secs, &v4.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

/// 재시도 간격 — 2초 × 2^(attempt-1) · 상한 60초.
pub(crate) fn backoff(attempt: u32) -> Duration {
    let secs = 2u64.saturating_mul(1u64 << attempt.saturating_sub(1).min(6));
    Duration::from_secs(secs.min(60))
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

    /// 결과 반영 — Up이 아니면(포트 닫힘·도달 불가) 지수 백오프 예약(최대 `max_retries`번).
    pub(crate) fn apply(&mut self, outcome: Outcome, now: Instant, max_retries: u32) {
        match outcome {
            Outcome::Up => {
                self.status = ProbeStatus::Up;
                self.attempts = 0;
                self.next_at = None;
            }
            Outcome::PortClosed | Outcome::Down => {
                self.status = if outcome == Outcome::Down {
                    ProbeStatus::Down
                } else {
                    ProbeStatus::PortClosed
                };
                self.attempts += 1;
                self.next_at = (self.attempts <= max_retries).then(|| now + backoff(self.attempts));
            }
        }
    }
}

// ── 한 번 이상 접속 성공한 프로필 기억(`connected.list`)

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

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(backoff(1).as_secs(), 2);
        assert_eq!(backoff(2).as_secs(), 4);
        assert_eq!(backoff(4).as_secs(), 16);
        assert_eq!(backoff(20).as_secs(), 60);
    }

    #[test]
    fn entry_stops_after_max_retries() {
        let now = Instant::now();
        let mut e = ProbeEntry::fresh(now);
        for _ in 0..3 {
            e.apply(Outcome::Down, now, 3);
            assert_eq!(e.status, ProbeStatus::Down);
            assert!(e.next_at.is_some());
        }
        e.apply(Outcome::PortClosed, now, 3);
        assert_eq!(e.status, ProbeStatus::PortClosed);
        assert!(e.next_at.is_none(), "최대 횟수 뒤엔 재시도 없음");
        e.apply(Outcome::Up, now, 3);
        assert_eq!(e.status, ProbeStatus::Up);
        assert_eq!(e.attempts, 0);
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
