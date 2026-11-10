//! ★ `nsql mcp-bridge --port <n> --token <t>`(docs/110 §6-3 · 11-11): Claude Code CLI가 stdio MCP 서버로 띄우는 **중계기** —
//! 표준 입력 줄 → GUI 루프백 TCP · TCP 줄 → 표준 출력. 첫 줄로 토큰을 보내 GUI가 확인한다. 프로토콜 처리는 GUI(`ai/mcp.rs`).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

pub(crate) fn run(port: Option<u16>, token: Option<&str>) -> i32 {
    let (Some(port), Some(token)) = (port, token) else {
        eprintln!("usage: nsql mcp-bridge --port <port> --token <token>");
        return 2;
    };
    let stream = match TcpStream::connect(("127.0.0.1", port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("mcp-bridge: connect 127.0.0.1:{port}: {e}");
            return 1;
        }
    };
    let mut up = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("mcp-bridge: {e}");
            return 1;
        }
    };
    // 토큰은 JSON 문자열로(따옴표·역슬래시 이스케이프).
    let tok = token.replace('\\', "\\\\").replace('"', "\\\"");
    if up
        .write_all(format!("{{\"token\":\"{tok}\"}}\n").as_bytes())
        .is_err()
    {
        return 1;
    }
    // TCP → stdout.
    let down = std::thread::spawn(move || {
        let out = std::io::stdout();
        let mut out = out.lock();
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            if out.write_all(line.as_bytes()).is_err() || out.write_all(b"\n").is_err() {
                break;
            }
            let _ = out.flush();
        }
    });
    // stdin → TCP.
    let stdin = std::io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        if up.write_all(line.as_bytes()).is_err() || up.write_all(b"\n").is_err() {
            break;
        }
        let _ = up.flush();
    }
    let _ = up.shutdown(std::net::Shutdown::Write);
    let _ = down.join();
    0
}
