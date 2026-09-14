//! 서버 도달성 신호등(사용자 09-14) — 접속 창 로그인 목록의 첫 컬럼.
//!
//! - **무엇을**: 프로필의 호스트·포트에 TCP 연결만 시도한다(DB 로그인 없음 · SYN 1개 · 즉시 닫음). 파일 DB·호스트 없음은 대상 아님.
//! - **누구를**: **한 번 이상 접속에 성공한 프로필만**(사용자 09-14) — `<설정 폴더>/connected.list`(이름 한 줄씩)로 기억.
//! - **언제**: 접속 창을 열 때 한 번. 실패하면 2초 → 4 → 8 … **지수 증가**(상한 60초)로 재시도하되 `probe.max_retries`(기본 5)를
//!   넘기면 멈춘다(빨강 고정). 성공하면 다시 묻지 않는다(창을 다시 열 때만). 창이 닫혀 있으면 아무것도 하지 않는다.
//! - **부하**: 별도 스레드 하나가 **순차**로 처리(동시 연결 0) · 타임아웃 `probe.timeout`(기본 2초) · UI 스레드는 요청/결과 전달만.
//!
//! 색 = 초록(연결 가능) · 노랑(확인 중) · 빨강(연결 불가) · 회색(대상 아님/모름).

use std::collections::HashSet;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProbeStatus {
    Unknown,
    Checking,
    Up,
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
    pub ok: bool,
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
                    let ok = reachable(&req.host, req.port, req.timeout);
                    if tx_res.send(ProbeResult { name: req.name, ok }).is_err() {
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

/// 이름 풀이(첫 주소) + `connect_timeout`. 성공하면 바로 닫는다.
fn reachable(host: &str, port: u16, timeout: Duration) -> bool {
    let Ok(mut addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    let Some(addr) = addrs.next() else {
        return false;
    };
    TcpStream::connect_timeout(&addr, timeout).is_ok()
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

    /// 결과 반영 — 실패면 지수 백오프 예약(최대 `max_retries`번).
    pub(crate) fn apply(&mut self, ok: bool, now: Instant, max_retries: u32) {
        if ok {
            self.status = ProbeStatus::Up;
            self.attempts = 0;
            self.next_at = None;
        } else {
            self.status = ProbeStatus::Down;
            self.attempts += 1;
            self.next_at = (self.attempts <= max_retries).then(|| now + backoff(self.attempts));
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
            e.apply(false, now, 3);
            assert_eq!(e.status, ProbeStatus::Down);
            assert!(e.next_at.is_some());
        }
        e.apply(false, now, 3);
        assert!(e.next_at.is_none(), "최대 횟수 뒤엔 재시도 없음");
        e.apply(true, now, 3);
        assert_eq!(e.status, ProbeStatus::Up);
        assert_eq!(e.attempts, 0);
    }

    #[test]
    fn unreachable_port_is_down_quickly() {
        // 127.0.0.1:1 — 열려 있을 리 없는 포트. 타임아웃 안에 false.
        let t = Instant::now();
        assert!(!reachable("127.0.0.1", 1, Duration::from_millis(500)));
        assert!(t.elapsed() < Duration::from_secs(3));
    }
}
