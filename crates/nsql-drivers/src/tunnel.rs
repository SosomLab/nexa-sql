//! ★ SSH 터널(T-3 · D-382 · 사용자 10-28 "시스템 ssh 프로세스 포워딩 · 외부 crate 0" · 159차): 접속 스펙의 `?ssh=user@host[:port]`
//! (`ssh_key=` = `-i`)가 있으면 허브가 **시스템 `ssh`** 를 `-N -L 127.0.0.1:<로컬>:<DB 호스트>:<DB 포트>`로 띄우고 드라이버는
//! `127.0.0.1:<로컬>`로 붙는다. 드라이버·ABI는 터널을 모른다(스펙의 host/port만 바뀜).
//!
//! - 인증 = 사용자의 ssh 환경 그대로(`~/.ssh/config` · 에이전트 · 기본 키 · `ssh_key=`) · `BatchMode=yes`라 **묻지 않는다** —
//!   모르는 호스트 키·비밀번호 인증은 실패로 끝나고 안내문이 붙는다(터미널에서 한 번 `ssh user@host`로 받아 두기).
//! - 26 §8: 사용자 접속 동작에서만 · 재시도 0 · 세션당 프로세스 1 · 세션이 닫히면 프로세스 종료([`Tunnel`]의 Drop).
//! - 39 §3: 설정 `ssh.command`(빈 값 = 터널 비활성 · 기본 `ssh`) · `ssh.connect_timeout_secs`(포워딩 포트가 열릴 때까지 상한).
//! - TLS와 함께 쓰면 드라이버가 보는 호스트 이름은 `127.0.0.1`이라 `verify_identity`는 맞지 않는다 → `verify_ca`를 쓴다.
//! - ★ 162차(사용자 10-28 "까만 명령창 여러 개 · 좀비 걱정"): ① Windows는 `CREATE_NO_WINDOW`로 콘솔 없이 띄운다 ② **같은 대상(ssh 인자가
//!   같은) 터널은 세션 N개가 프로세스 1개를 공유**(`Arc` · 마지막 세션이 닫힐 때 종료 · 객체 탐색기·편집기·추가 접속이 각각 ssh를 띄우던 것)
//!   ③ 앱이 비정상 종료해도 ssh가 남지 않게 — Windows = Job Object(`KILL_ON_JOB_CLOSE` · 부모가 죽으면 커널이 자식을 끝냄) ·
//!   3-OS 공통 = **PID 원장**(`<설정 폴더>/run/ssh-tunnels.pids` · `소유자 pid \t ssh pid \t 로컬 포트`) → 다음 기동 때 소유자가
//!   죽은 줄의 ssh를 확인(프로세스 이름 ssh 또는 그 포트가 열려 있음)하고 끝낸다(`cleanup_stale`) · 다중 인스턴스는 소유자 pid로 구분.

use nsql_core::DbError;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

/// 설정(`ssh.command` · `ssh.connect_timeout_secs`) — 호스트가 기동·설정 변경 때 넣는다. 없으면 `ssh` · 15 s.
static SSH_CFG: Mutex<Option<(String, u64)>> = Mutex::new(None);

pub fn set_config(command: &str, timeout_secs: u64) {
    *SSH_CFG.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((command.trim().to_string(), timeout_secs.max(1)));
}

fn config() -> (String, u64) {
    SSH_CFG
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| ("ssh".to_string(), 15))
}

fn err(msg: impl Into<String>) -> DbError {
    DbError {
        code: None,
        message: msg.into(),
        position: None,
    }
}

// ── 살아 있는 터널 등록부(대상별 공유) · PID 원장(기동 정리)

/// 지금 살아 있는 터널(키 = 명령 + 인자 전체 · 포트 제외). `Weak`라 세션이 다 닫히면 자연히 빠진다.
static LIVE: Mutex<Vec<(String, Weak<Tunnel>)>> = Mutex::new(Vec::new());

/// 살아 있는 터널 프로세스 수(시험·진단).
pub fn live_tunnels() -> usize {
    LIVE.lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(_, w)| w.upgrade().is_some_and(|t| t.alive()))
        .count()
}

/// PID 원장이 놓일 폴더(`<설정 폴더>/run`) — 호스트가 기동 때 넣는다 · 없으면 원장 없음(기능은 그대로).
static STATE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_state_dir(dir: Option<PathBuf>) {
    *STATE_DIR.lock().unwrap_or_else(|e| e.into_inner()) = dir;
}

const LEDGER_FILE: &str = "ssh-tunnels.pids";

fn ledger_path() -> Option<PathBuf> {
    STATE_DIR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|d| d.join(LEDGER_FILE))
}

/// 원장 한 줄 = `소유자 pid \t ssh pid \t 로컬 포트`.
fn ledger_lines(path: &std::path::Path) -> Vec<(u32, u32, u16)> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            Some((
                it.next()?.trim().parse().ok()?,
                it.next()?.trim().parse().ok()?,
                it.next()?.trim().parse().ok()?,
            ))
        })
        .collect()
}

fn ledger_write(path: &std::path::Path, lines: &[(u32, u32, u16)]) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let body: String = lines
        .iter()
        .map(|(o, p, port)| format!("{o}\t{p}\t{port}\n"))
        .collect();
    let _ = std::fs::write(path, body);
}

fn ledger_add(pid: u32, port: u16) {
    if let Some(path) = ledger_path() {
        let mut lines = ledger_lines(&path);
        lines.retain(|(_, p, _)| *p != pid);
        lines.push((std::process::id(), pid, port));
        ledger_write(&path, &lines);
    }
}

fn ledger_remove(pid: u32) {
    if let Some(path) = ledger_path() {
        let mut lines = ledger_lines(&path);
        let n = lines.len();
        lines.retain(|(_, p, _)| *p != pid);
        if lines.len() != n {
            ledger_write(&path, &lines);
        }
    }
}

/// 그 로컬 포트가 지금 받는가(원장의 ssh가 아직 우리 포워딩을 듣고 있다는 표식).
fn port_open(port: u16) -> bool {
    TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(150),
    )
    .is_ok()
}

/// ★ 기동 정리(호스트가 `set_state_dir` 뒤 한 번): 소유자가 죽은 원장 줄의 ssh를 끝낸다 — **이름이 ssh이거나 그 포트가 열려 있을 때만**
/// (pid 재사용으로 남의 프로세스를 죽이지 않게) · 소유자가 살아 있는 줄(다른 인스턴스)은 그대로. 반환 = 끝낸 수.
pub fn cleanup_stale() -> usize {
    ledger_path()
        .map(|p| cleanup_stale_in(&p, std::process::id()))
        .unwrap_or(0)
}

/// 폴더를 인자로 받는 본체(단위 시험 = `temp_dir()` 아래 · 실제 설정 폴더에 쓰지 않는다).
pub fn cleanup_stale_in(path: &std::path::Path, me: u32) -> usize {
    let lines = ledger_lines(path);
    if lines.is_empty() {
        return 0;
    }
    let mut keep = Vec::new();
    let mut killed = 0;
    for (owner, pid, port) in lines {
        if owner == me || os::process_alive(owner) {
            keep.push((owner, pid, port));
            continue;
        }
        if os::process_alive(pid)
            && (os::process_is_ssh(pid) || port_open(port))
            && os::kill_pid(pid)
        {
            killed += 1;
        }
    }
    ledger_write(path, &keep);
    killed
}

/// `user@host[:port]` → (`user@host` 또는 `host`, 포트). IPv6는 `[::1]:22` 꼴만.
pub fn parse_target(ssh: &str) -> Result<(String, Option<u16>), DbError> {
    let s = ssh.trim();
    if s.is_empty() {
        return Err(err("SSH: empty tunnel target (expected user@host[:port])"));
    }
    // 끝의 `:숫자`만 포트 — IPv6는 `[…]:포트` 꼴(대괄호 없는 `::1`은 포트 없음으로 본다).
    let (dest, port) = match s.rfind(':') {
        Some(i)
            if !s[i + 1..].is_empty()
                && s[i + 1..].chars().all(|c| c.is_ascii_digit())
                && (!s[..i].contains(':') || s[..i].ends_with(']')) =>
        {
            (s[..i].to_string(), s[i + 1..].parse::<u16>().ok())
        }
        _ => (s.to_string(), None),
    };
    if dest.is_empty() || dest.ends_with('@') {
        return Err(err(format!(
            "SSH: bad tunnel target '{ssh}' (expected user@host[:port])"
        )));
    }
    Ok((dest, port))
}

/// 빈 로컬 포트 하나(127.0.0.1 · OS가 고른 뒤 바로 놓는다 — ssh가 같은 번호로 듣는다).
pub fn pick_local_port() -> Result<u16, DbError> {
    let l = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| err(format!("SSH: cannot reserve a local port: {e}")))?;
    let p = l
        .local_addr()
        .map_err(|e| err(format!("SSH: local port: {e}")))?
        .port();
    drop(l);
    Ok(p)
}

/// ssh 인자(프로그램 이름 제외) — 시험이 본다.
pub fn ssh_args(
    dest: &str,
    ssh_port: Option<u16>,
    key: Option<&str>,
    local_port: u16,
    host: &str,
    port: u16,
    timeout_secs: u64,
) -> Vec<String> {
    let mut a = vec![
        "-N".to_string(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ExitOnForwardFailure=yes".into(),
        // ★ 처음 보는 호스트 키 = 자동 등록(사용자 10-29 결정 · D-392) · 등록된 키와 다르면 종전처럼 거절(중간자 방어는 유지).
        "-o".into(),
        "StrictHostKeyChecking=accept-new".into(),
        "-o".into(),
        format!("ConnectTimeout={timeout_secs}"),
        "-o".into(),
        "ServerAliveInterval=30".into(),
    ];
    if let Some(p) = ssh_port {
        a.push("-p".into());
        a.push(p.to_string());
    }
    if let Some(k) = key.map(str::trim).filter(|k| !k.is_empty()) {
        a.push("-i".into());
        a.push(k.to_string());
    }
    a.push("-L".into());
    a.push(format!("127.0.0.1:{local_port}:{host}:{port}"));
    a.push(dest.to_string());
    a
}

/// 살아 있는 터널 — 같은 대상의 세션들이 `Arc`로 공유 · 마지막 세션이 놓을 때 ssh 종료(Drop) + 원장에서 제거.
#[derive(Debug)]
pub struct Tunnel {
    child: Mutex<Child>,
    pid: u32,
    pub local_port: u16,
    /// 표시용 `user@host[:port] → host:port`.
    pub label: String,
}

impl Tunnel {
    /// ssh가 아직 도는가(공유 전 확인 — 배스천이 끊었으면 새로 띄운다).
    fn alive(&self) -> bool {
        matches!(
            self.child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .try_wait(),
            Ok(None)
        )
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
        ledger_remove(self.pid);
    }
}

/// 터널을 연다 — **같은 대상(명령·인자 동일 · 포트 제외)의 터널이 살아 있으면 그것을 공유**하고, 없으면 띄워서 포워딩 포트가 받을 때까지
/// 기다린다(상한 = `ssh.connect_timeout_secs`). 실패 = ssh의 stderr 원문 + 안내.
pub fn open(ssh: &str, key: Option<&str>, host: &str, port: u16) -> Result<Arc<Tunnel>, DbError> {
    let (cmd, timeout_secs) = config();
    if cmd.is_empty() {
        return Err(err(
            "SSH: tunnels are disabled (setting ssh.command is empty)",
        ));
    }
    let (dest, ssh_port) = parse_target(ssh)?;
    // 공유 키 = 로컬 포트를 뺀 인자 전부(포트 0 자리) + 명령.
    let share_key = format!(
        "{cmd}\u{1}{}",
        ssh_args(&dest, ssh_port, key, 0, host, port, timeout_secs).join("\u{1}")
    );
    {
        let mut live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        live.retain(|(_, w)| w.strong_count() > 0);
        if let Some(t) = live
            .iter()
            .find(|(k, _)| *k == share_key)
            .and_then(|(_, w)| w.upgrade())
            .filter(|t| t.alive())
        {
            return Ok(t);
        }
    }
    let local_port = pick_local_port()?;
    let args = ssh_args(&dest, ssh_port, key, local_port, host, port, timeout_secs);
    let mut command = Command::new(&cmd);
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    os::prepare(&mut command);
    let mut child = command.spawn().map_err(|e| {
        err(format!(
            "SSH: cannot start '{cmd}': {e} (install OpenSSH or set ssh.command to the ssh executable)"
        ))
    })?;
    os::guard_child(&child);
    let pid = child.id();
    ledger_add(pid, local_port);
    let label = format!("{ssh} -> {host}:{port}");
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let mut text = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_string(&mut text);
            }
            ledger_remove(pid);
            return Err(err(exit_message(&dest, status.code(), &text)));
        }
        if port_open(local_port) {
            let t = Arc::new(Tunnel {
                child: Mutex::new(child),
                pid,
                local_port,
                label,
            });
            LIVE.lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((share_key, Arc::downgrade(&t)));
            return Ok(t);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            ledger_remove(pid);
            return Err(err(format!(
                "SSH: tunnel to {dest} did not come up within {timeout_secs}s (ssh.connect_timeout_secs)"
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// OS별 조각: 콘솔 없이 띄우기(Windows) · 부모 사망 시 동반 종료(Windows Job Object) · pid 생존/이름/종료(3-OS). 외부 crate 0.
#[cfg(windows)]
mod os {
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command};
    use std::sync::OnceLock;

    type Handle = *mut core::ffi::c_void;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;
    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;

    #[repr(C)]
    struct BasicLimit {
        per_process_user_time: i64,
        per_job_user_time: i64,
        limit_flags: u32,
        min_ws: usize,
        max_ws: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }
    #[repr(C)]
    struct ExtLimit {
        basic: BasicLimit,
        io: [u64; 6],
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory: usize,
        peak_job_memory: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attrs: *const core::ffi::c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: u32,
            info: *const core::ffi::c_void,
            len: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
        fn QueryFullProcessImageNameW(
            process: Handle,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
        fn TerminateProcess(process: Handle, code: u32) -> i32;
        fn CloseHandle(h: Handle) -> i32;
    }

    struct Job(Handle);
    // 핸들은 프로세스 수명 동안 하나 · 닫지 않는다(닫히는 순간 = 우리 프로세스 종료 = 자식 전부 종료).
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    fn job() -> Option<Handle> {
        static JOB: OnceLock<Option<Job>> = OnceLock::new();
        JOB.get_or_init(|| unsafe {
            let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if h.is_null() {
                return None;
            }
            let mut info: ExtLimit = std::mem::zeroed();
            info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                h,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                (&info as *const ExtLimit).cast(),
                std::mem::size_of::<ExtLimit>() as u32,
            );
            if ok == 0 {
                CloseHandle(h);
                return None;
            }
            Some(Job(h))
        })
        .as_ref()
        .map(|j| j.0)
    }

    /// 콘솔 창 없이(사용자 10-28 "까만 명령창") — ssh는 stdin null · stderr 파이프라 콘솔이 필요 없다.
    pub(super) fn prepare(cmd: &mut Command) {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    /// Job에 넣는다 — 우리 프로세스가 어떻게 끝나든 커널이 ssh를 함께 끝낸다. 실패(중첩 Job 금지 환경)는 무시 = 원장이 대신한다.
    pub(super) fn guard_child(child: &Child) {
        if let Some(j) = job() {
            unsafe {
                AssignProcessToJobObject(j, child.as_raw_handle() as Handle);
            }
        }
    }

    pub(super) fn process_alive(pid: u32) -> bool {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code);
            CloseHandle(h);
            ok != 0 && code == STILL_ACTIVE
        }
    }

    pub(super) fn process_is_ssh(pid: u32) -> bool {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return false;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            if ok == 0 {
                return false;
            }
            let name = String::from_utf16_lossy(&buf[..len as usize]).to_ascii_lowercase();
            name.ends_with("\\ssh.exe") || name.ends_with("/ssh.exe")
        }
    }

    pub(super) fn kill_pid(pid: u32) -> bool {
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if h.is_null() {
                return false;
            }
            let ok = TerminateProcess(h, 1);
            CloseHandle(h);
            ok != 0
        }
    }
}

#[cfg(unix)]
mod os {
    use std::process::{Child, Command};

    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    const SIGTERM: i32 = 15;
    const EPERM: i32 = 1;

    pub(super) fn prepare(_cmd: &mut Command) {}
    pub(super) fn guard_child(_child: &Child) {}

    /// `kill(pid, 0)` — 0 = 있음 · EPERM = 있지만 남의 것(살아 있음으로 본다).
    pub(super) fn process_alive(pid: u32) -> bool {
        let Ok(p) = i32::try_from(pid) else {
            return false;
        };
        unsafe { kill(p, 0) == 0 || std::io::Error::last_os_error().raw_os_error() == Some(EPERM) }
    }

    /// Linux = `/proc/<pid>/comm` · 그 밖(macOS)은 `ps -o comm=`(기동 정리 때만 · 원장에 줄이 있을 때만 돈다).
    pub(super) fn process_is_ssh(pid: u32) -> bool {
        if let Ok(c) = std::fs::read_to_string(format!("/proc/{pid}/comm")) {
            return c.trim() == "ssh";
        }
        Command::new("ps")
            .args(["-o", "comm=", "-p", &pid.to_string()])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().rsplit('/').next() == Some("ssh"))
            .unwrap_or(false)
    }

    pub(super) fn kill_pid(pid: u32) -> bool {
        let Ok(p) = i32::try_from(pid) else {
            return false;
        };
        unsafe { kill(p, SIGTERM) == 0 }
    }
}

/// ssh가 먼저 끝났을 때의 문구: stderr 원문 한 줄 + 흔한 원인 안내.
fn exit_message(dest: &str, code: Option<i32>, stderr: &str) -> String {
    let line = stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string();
    let hint = if line.contains("Host key verification failed") {
        // accept-new라 처음 보는 키는 통과한다 → 이 실패 = known_hosts의 키와 **다르다**(서버 교체 또는 중간자).
        " - the host key differs from ~/.ssh/known_hosts: verify the server, then `ssh-keygen -R <host>` and retry (ssh "
            .to_string()
            + dest
            + ")"
    } else if line.contains("Permission denied") {
        " - BatchMode cannot ask for a password: use a key (ssh_key=) or an agent".to_string()
    } else {
        String::new()
    };
    let code = code
        .map(|c| c.to_string())
        .unwrap_or_else(|| "signal".into());
    if line.is_empty() {
        format!("SSH: ssh exited ({code}) before the tunnel to {dest} came up{hint}")
    } else {
        format!("SSH: {line} (ssh exited {code}){hint}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_parse_and_args() {
        assert_eq!(
            parse_target("u@h:2222").unwrap(),
            ("u@h".to_string(), Some(2222))
        );
        assert_eq!(parse_target("h").unwrap(), ("h".to_string(), None));
        assert_eq!(parse_target("u@h").unwrap(), ("u@h".to_string(), None));
        assert_eq!(
            parse_target("[::1]:22").unwrap(),
            ("[::1]".to_string(), Some(22))
        );
        assert!(parse_target("").is_err());
        assert!(parse_target("u@").is_err());
        let a = ssh_args("u@h", Some(2222), Some(" /k/id "), 40001, "db", 5432, 7);
        assert_eq!(
            a,
            [
                "-N",
                "-o",
                "BatchMode=yes",
                "-o",
                "ExitOnForwardFailure=yes",
                "-o",
                "StrictHostKeyChecking=accept-new",
                "-o",
                "ConnectTimeout=7",
                "-o",
                "ServerAliveInterval=30",
                "-p",
                "2222",
                "-i",
                "/k/id",
                "-L",
                "127.0.0.1:40001:db:5432",
                "u@h"
            ]
        );
        let b = ssh_args("h", None, Some(""), 1, "x", 1, 1);
        assert!(!b.contains(&"-i".to_string()) && !b.contains(&"-p".to_string()));
        assert_ne!(pick_local_port().unwrap(), 0);
    }

    #[test]
    fn exit_messages_carry_stderr_and_hints() {
        let m = exit_message("u@h", Some(255), "\nHost key verification failed.\n");
        assert!(
            m.starts_with("SSH: Host key verification failed. (ssh exited 255)"),
            "{m}"
        );
        assert!(
            m.contains("ssh-keygen -R") && m.ends_with("(ssh u@h)"),
            "{m}"
        );
        let m = exit_message(
            "u@h",
            Some(255),
            "u@h: Permission denied (publickey,password).",
        );
        assert!(
            m.contains("Permission denied") && m.contains("ssh_key="),
            "{m}"
        );
        let m = exit_message("u@h", None, "");
        assert!(m.contains("exited (signal)"), "{m}");
    }

    /// ★ 162차 PID 원장 정리(`temp_dir()` 아래 · 실제 설정 폴더 X): 소유자가 죽은 줄은 지우고(ssh가 없으면 끝낼 것도 없음) ·
    /// 소유자가 나(살아 있음)인 줄은 남긴다 · 깨진 줄은 무시 · 파일 없음 = 0.
    #[test]
    fn stale_ledger_cleanup_keeps_live_owner_rows() {
        let dir = std::env::temp_dir().join(format!("nsql-tunnel-ledger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(LEDGER_FILE);
        assert_eq!(cleanup_stale_in(&path, 1), 0);
        let me = std::process::id();
        // 소유자 4_000_000_000 = 없는 pid(죽은 인스턴스) · ssh pid도 없는 pid → 줄만 지운다.
        ledger_write(
            &path,
            &[(4_000_000_000, 4_000_000_001, 1), (me, 4_000_000_002, 2)],
        );
        std::fs::write(
            &path,
            format!("{}garbage\n", std::fs::read_to_string(&path).unwrap()),
        )
        .unwrap();
        assert_eq!(cleanup_stale_in(&path, me), 0);
        assert_eq!(ledger_lines(&path), vec![(me, 4_000_000_002, 2)]);
        // add/remove 왕복(원장 폴더를 잠깐 이 temp로).
        set_state_dir(Some(dir.clone()));
        ledger_add(4_000_000_003, 3);
        assert!(ledger_lines(&path).contains(&(me, 4_000_000_003, 3)));
        ledger_remove(4_000_000_003);
        assert!(!ledger_lines(&path)
            .iter()
            .any(|(_, p, _)| *p == 4_000_000_003));
        set_state_dir(None);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(live_tunnels(), 0);
        assert!(!os::process_alive(4_000_000_000));
    }

    /// 실패 경로 둘 = 명령 없음 · 비활성(빈 명령) — 네트워크 0.
    #[test]
    fn open_fails_fast_without_command() {
        set_config("nsql-no-such-ssh-binary", 1);
        let e = open("u@h", None, "db", 1).expect_err("spawn 실패");
        assert!(e.message.contains("cannot start"), "{}", e.message);
        set_config("", 1);
        let e = open("u@h", None, "db", 1).expect_err("비활성");
        assert!(e.message.contains("disabled"), "{}", e.message);
        set_config("ssh", 15);
    }
}
