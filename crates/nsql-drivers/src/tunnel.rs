//! ★ SSH 터널(T-3 · D-382 · 사용자 10-28 "시스템 ssh 프로세스 포워딩 · 외부 crate 0" · 159차): 접속 스펙의 `?ssh=user@host[:port]`
//! (`ssh_key=` = `-i`)가 있으면 허브가 **시스템 `ssh`** 를 `-N -L 127.0.0.1:<로컬>:<DB 호스트>:<DB 포트>`로 띄우고 드라이버는
//! `127.0.0.1:<로컬>`로 붙는다. 드라이버·ABI는 터널을 모른다(스펙의 host/port만 바뀜).
//!
//! - 인증 = 사용자의 ssh 환경 그대로(`~/.ssh/config` · 에이전트 · 기본 키 · `ssh_key=`) · `BatchMode=yes`라 **묻지 않는다** —
//!   모르는 호스트 키·비밀번호 인증은 실패로 끝나고 안내문이 붙는다(터미널에서 한 번 `ssh user@host`로 받아 두기).
//! - 26 §8: 사용자 접속 동작에서만 · 재시도 0 · 세션당 프로세스 1 · 세션이 닫히면 프로세스 종료([`Tunnel`]의 Drop).
//! - 39 §3: 설정 `ssh.command`(빈 값 = 터널 비활성 · 기본 `ssh`) · `ssh.connect_timeout_secs`(포워딩 포트가 열릴 때까지 상한).
//! - TLS와 함께 쓰면 드라이버가 보는 호스트 이름은 `127.0.0.1`이라 `verify_identity`는 맞지 않는다 → `verify_ca`를 쓴다.

use nsql_core::DbError;
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
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

/// 살아 있는 터널 — 세션과 함께 산다(Drop = ssh 종료).
#[derive(Debug)]
pub struct Tunnel {
    child: Child,
    pub local_port: u16,
    /// 표시용 `user@host[:port] → host:port`.
    pub label: String,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// 터널을 열고 **포워딩 포트가 받을 때까지** 기다린다(상한 = `ssh.connect_timeout_secs`). 실패 = ssh의 stderr 원문 + 안내.
pub fn open(ssh: &str, key: Option<&str>, host: &str, port: u16) -> Result<Tunnel, DbError> {
    let (cmd, timeout_secs) = config();
    if cmd.is_empty() {
        return Err(err(
            "SSH: tunnels are disabled (setting ssh.command is empty)",
        ));
    }
    let (dest, ssh_port) = parse_target(ssh)?;
    let local_port = pick_local_port()?;
    let args = ssh_args(&dest, ssh_port, key, local_port, host, port, timeout_secs);
    let mut child = Command::new(&cmd)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            err(format!(
                "SSH: cannot start '{cmd}': {e} (install OpenSSH or set ssh.command to the ssh executable)"
            ))
        })?;
    let label = format!("{ssh} -> {host}:{port}");
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let mut text = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_string(&mut text);
            }
            return Err(err(exit_message(&dest, status.code(), &text)));
        }
        if TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], local_port)),
            Duration::from_millis(200),
        )
        .is_ok()
        {
            return Ok(Tunnel {
                child,
                local_port,
                label,
            });
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err(format!(
                "SSH: tunnel to {dest} did not come up within {timeout_secs}s (ssh.connect_timeout_secs)"
            )));
        }
        std::thread::sleep(Duration::from_millis(100));
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
        " - accept the host key once from a terminal: ssh ".to_string() + dest
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
        assert!(m.ends_with("ssh u@h"), "{m}");
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
