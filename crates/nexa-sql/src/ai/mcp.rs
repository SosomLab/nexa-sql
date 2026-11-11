//! ★ MCP 서버(docs/110 §6-3 · CLI 모드): GUI가 `127.0.0.1:<port>`에 줄 단위 JSON-RPC를 받고, `claude`는 `nsql mcp-bridge
//! --port --token`(stdio ↔ TCP 중계)로 붙는다. 도구 실행·승인은 [`AiEvent::ToolRequest`]/[`AiEvent::Approval`]로 UI 스레드에.
//! 토큰 = 첫 줄(`{"token":"…"}`) · 틀리면 즉시 닫는다 · 루프백만 듣는다.

use super::{jget, jstr, AiEvent, ToolReply};
use nsql_settings::json::{dump as json_dump, parse as json_parse, Json};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

pub(crate) struct McpServer {
    pub port: u16,
    pub token: String,
    stop: Arc<AtomicBool>,
}

impl Drop for McpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// 토큰(32 hex · 시각 + pid + 주소 엔트로피 — 외부 crate 0).
fn make_token() -> String {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let x = Box::new(0u8);
    let addr = &*x as *const u8 as usize as u128;
    let mut h = t ^ (addr << 17) ^ (u128::from(std::process::id()) << 64);
    // xorshift 두 바퀴.
    h ^= h << 13;
    h ^= h >> 7;
    h ^= h << 17;
    format!("{h:032x}")
}

/// 서버 시작 — `port_pref` 0 = 임의 포트.
pub(crate) fn start(
    tx: mpsc::Sender<AiEvent>,
    wake: super::host::Wake,
    port_pref: u16,
) -> std::io::Result<McpServer> {
    let listener = TcpListener::bind(("127.0.0.1", port_pref))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let token = make_token();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let tok2 = token.clone();
    std::thread::Builder::new()
        .name("ai-mcp".into())
        .spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let tx = tx.clone();
                        let wake = wake.clone();
                        let tok = tok2.clone();
                        let stop = stop2.clone();
                        let _ = std::thread::Builder::new()
                            .name("ai-mcp-conn".into())
                            .spawn(move || serve(stream, &tok, &tx, &wake, &stop));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(_) => break,
                }
            }
        })?;
    Ok(McpServer { port, token, stop })
}

/// `--mcp-config`에 넣을 JSON(인라인) — 브리지 = 같은 폴더의 `nsql` 실행 파일.
/// 브리지 프로그램(`nsql`): 실행 파일 옆의 `nsql[.exe]`가 있으면 절대 경로 · 없으면 PATH의 `nsql`.
pub(crate) fn bridge_program() -> String {
    let exe = std::env::current_exe().ok();
    exe.as_ref()
        .and_then(|e| e.parent())
        .map(|d| d.join(if cfg!(windows) { "nsql.exe" } else { "nsql" }))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "nsql".to_string())
}

/// 브리지가 PATH로만 잡힐 때 실제로 찾을 수 있는가(사전 점검 · 11-12 "MCP 연결 30초 초과" 오류).
pub(crate) fn bridge_available(program: &str) -> bool {
    let p = std::path::Path::new(program);
    if p.is_absolute() {
        return p.is_file();
    }
    let exe = if cfg!(windows) && !program.ends_with(".exe") {
        format!("{program}.exe")
    } else {
        program.to_string()
    };
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(&exe).is_file()))
        .unwrap_or(false)
}

pub(crate) fn config_json(port: u16, token: &str) -> String {
    let nsql = bridge_program();
    let v = Json::Obj(vec![(
        "mcpServers".into(),
        Json::Obj(vec![(
            super::bridge::MCP_SERVER.into(),
            Json::Obj(vec![
                ("type".into(), Json::Str("stdio".into())),
                ("command".into(), Json::Str(nsql)),
                (
                    "args".into(),
                    Json::Arr(vec![
                        Json::Str("mcp-bridge".into()),
                        Json::Str("--port".into()),
                        Json::Str(port.to_string()),
                        Json::Str("--token".into()),
                        Json::Str(token.into()),
                    ]),
                ),
            ]),
        )]),
    )]);
    json_dump(&v)
}

fn rpc_result(id: &Json, result: Json) -> String {
    json_dump(&Json::Obj(vec![
        ("jsonrpc".into(), Json::Str("2.0".into())),
        ("id".into(), id.clone()),
        ("result".into(), result),
    ]))
}

fn rpc_error(id: &Json, code: i64, msg: &str) -> String {
    json_dump(&Json::Obj(vec![
        ("jsonrpc".into(), Json::Str("2.0".into())),
        ("id".into(), id.clone()),
        (
            "error".into(),
            Json::Obj(vec![
                ("code".into(), Json::Num(code as f64)),
                ("message".into(), Json::Str(msg.into())),
            ]),
        ),
    ]))
}

fn text_result(text: &str, is_error: bool) -> Json {
    Json::Obj(vec![
        (
            "content".into(),
            Json::Arr(vec![Json::Obj(vec![
                ("type".into(), Json::Str("text".into())),
                ("text".into(), Json::Str(text.into())),
            ])]),
        ),
        ("isError".into(), Json::Bool(is_error)),
    ])
}

/// 요청 한 줄 → 응답 한 줄(알림이면 `None`) — 순수 함수(시험) · 도구 실행은 `exec`로 위임.
pub(crate) fn handle(line: &str, exec: &mut dyn FnMut(&str, &Json) -> ToolReply) -> Option<String> {
    let v = json_parse(line.trim()).ok()?;
    let id = jget(&v, "id").cloned().unwrap_or(Json::Null);
    let method = jstr(&v, "method");
    let params = jget(&v, "params").cloned().unwrap_or(Json::Obj(Vec::new()));
    if method.starts_with("notifications/") {
        return None;
    }
    let out = match method.as_str() {
        "initialize" => rpc_result(
            &id,
            Json::Obj(vec![
                ("protocolVersion".into(), Json::Str("2025-06-18".into())),
                (
                    "capabilities".into(),
                    Json::Obj(vec![("tools".into(), Json::Obj(Vec::new()))]),
                ),
                (
                    "serverInfo".into(),
                    Json::Obj(vec![
                        ("name".into(), Json::Str("nexa-sql".into())),
                        (
                            "version".into(),
                            Json::Str(env!("CARGO_PKG_VERSION").into()),
                        ),
                    ]),
                ),
            ]),
        ),
        "ping" => rpc_result(&id, Json::Obj(Vec::new())),
        "tools/list" => rpc_result(
            &id,
            Json::Obj(vec![("tools".into(), super::tools::mcp_tools())]),
        ),
        "tools/call" => {
            let name = jstr(&params, "name");
            let args = jget(&params, "arguments")
                .cloned()
                .unwrap_or(Json::Obj(Vec::new()));
            let r = exec(&name, &args);
            rpc_result(&id, text_result(&r.output, !r.ok))
        }
        _ => rpc_error(&id, -32601, "method not found"),
    };
    Some(out)
}

fn serve(
    stream: TcpStream,
    token: &str,
    tx: &mpsc::Sender<AiEvent>,
    wake: &super::host::Wake,
    stop: &AtomicBool,
) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    // 첫 줄 = 토큰.
    let mut first = String::new();
    let mut authed = false;
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        match reader.read_line(&mut first) {
            Ok(0) => return,
            Ok(_) => {
                if let Ok(v) = json_parse(first.trim()) {
                    authed = jstr(&v, "token") == token;
                }
                break;
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => return,
        }
    }
    if !authed {
        return;
    }
    let mut exec = |name: &str, args: &Json| -> ToolReply {
        if name == "approve" {
            let tool = jstr(args, "tool_name");
            let input = jget(args, "input")
                .cloned()
                .unwrap_or(Json::Obj(Vec::new()));
            let (rtx, rrx) = mpsc::channel();
            let _ = tx.send(AiEvent::Approval {
                id: jstr(args, "tool_use_id"),
                tool,
                summary: json_dump(&input),
                reply: rtx,
            });
            wake();
            let ok = rrx.recv().unwrap_or(false);
            let v = if ok {
                Json::Obj(vec![
                    ("behavior".into(), Json::Str("allow".into())),
                    ("updatedInput".into(), input),
                ])
            } else {
                Json::Obj(vec![
                    ("behavior".into(), Json::Str("deny".into())),
                    ("message".into(), Json::Str("denied by the user".into())),
                ])
            };
            return ToolReply {
                output: json_dump(&v),
                ok: true,
            };
        }
        let (rtx, rrx) = mpsc::channel();
        let _ = tx.send(AiEvent::ToolRequest {
            id: String::new(),
            name: name.to_string(),
            input: json_dump(args),
            reply: rtx,
        });
        wake();
        rrx.recv().unwrap_or(ToolReply {
            output: "cancelled".into(),
            ok: false,
        })
    };
    let mut line = String::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => return,
            Ok(_) => {
                if let Some(out) = handle(&line, &mut exec) {
                    if writer.write_all(out.as_bytes()).is_err() || writer.write_all(b"\n").is_err()
                    {
                        return;
                    }
                    let _ = writer.flush();
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_handle_initialize_list_call() {
        let mut calls = Vec::new();
        let mut exec = |n: &str, a: &Json| {
            calls.push((n.to_string(), json_dump(a)));
            ToolReply {
                output: "ok".into(),
                ok: true,
            }
        };
        let init = handle(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            &mut exec,
        )
        .expect("resp");
        assert!(init.contains("\"protocolVersion\"") && init.contains("\"id\":1"));
        assert!(handle(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            &mut exec
        )
        .is_none());
        let list = handle(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
            &mut exec,
        )
        .expect("resp");
        assert!(list.contains("\"run_query\"") && list.contains("\"approve\""));
        let call = handle(
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_setting","arguments":{"key":"a"}}}"#,
            &mut exec,
        )
        .expect("resp");
        assert!(call.contains("\"isError\":false") && call.contains("\"text\":\"ok\""));
        let bad = handle(r#"{"jsonrpc":"2.0","id":4,"method":"nope"}"#, &mut exec).expect("resp");
        assert!(bad.contains("-32601"));
        drop(exec);
        assert_eq!(
            calls,
            vec![("get_setting".to_string(), "{\"key\":\"a\"}".to_string())]
        );
        let cfg = config_json(4321, "tok");
        assert!(
            cfg.contains("\"mcp-bridge\"") && cfg.contains("\"4321\"") && cfg.contains("\"tok\"")
        );
        assert_eq!(make_token().len(), 32);
    }

    #[test]
    fn server_accepts_token_and_serves_tools_list() {
        let (tx, rx) = mpsc::channel();
        let srv = start(tx, Arc::new(|| {}), 0).expect("bind");
        let mut s = TcpStream::connect(("127.0.0.1", srv.port)).expect("connect");
        s.write_all(format!("{{\"token\":\"{}\"}}\n", srv.token).as_bytes())
            .expect("w");
        s.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n")
            .expect("w");
        let mut r = BufReader::new(s.try_clone().expect("clone"));
        let mut line = String::new();
        r.read_line(&mut line).expect("r");
        assert!(line.contains("\"search_objects\""), "{line}");
        // 도구 호출은 UI 스레드(여기서는 시험)가 답한다.
        s.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"session_info\",\"arguments\":{}}}\n").expect("w");
        match rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("req")
        {
            AiEvent::ToolRequest { name, reply, .. } => {
                assert_eq!(name, "session_info");
                reply
                    .send(ToolReply {
                        output: "info".into(),
                        ok: true,
                    })
                    .expect("reply");
            }
            other => panic!("unexpected {other:?}"),
        }
        line.clear();
        r.read_line(&mut line).expect("r");
        assert!(line.contains("\"text\":\"info\""), "{line}");
        // 잘못된 토큰 = 응답 없이 닫힘.
        let mut bad = TcpStream::connect(("127.0.0.1", srv.port)).expect("connect");
        bad.write_all(b"{\"token\":\"x\"}\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\n")
            .expect("w");
        let mut br = BufReader::new(bad);
        let mut l2 = String::new();
        let n = br.read_line(&mut l2).unwrap_or(0);
        assert_eq!(n, 0, "{l2}");
    }
}
