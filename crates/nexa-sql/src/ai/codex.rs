//! ★ Codex 어댑터(docs/110 §5 · §15-3 ② · 11-11): `codex app-server`(stdio JSONL JSON-RPC · `jsonrpc` 필드 생략)의 **요청 조립 ·
//! 알림/서버 요청 파싱**(순수 함수). 프로세스 관리는 `host.rs`(`start_codex`). 도구 = `thread/start`의 `dynamicTools`(실험 · `experimentalApi`)
//! → 서버 요청 `item/tool/call` → 우리 도구 표 실행 → `{contentItems, success}` 응답. 인증 = 프로필 API 키(`account/login/start` apiKey ·
//! ChatGPT 로그인은 상업 제품에 허용되지 않음 · 110 §13-1).

use super::{jget, jstr, AiEvent, Usage};
use nsql_settings::json::{dump as json_dump, parse as json_parse, Json};

fn obj(items: Vec<(&str, Json)>) -> Json {
    Json::Obj(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn s(v: &str) -> Json {
    Json::Str(v.to_string())
}

/// 요청 한 줄(`jsonrpc` 생략 · id 정수).
pub(crate) fn request(id: u64, method: &str, params: Json) -> String {
    json_dump(&obj(vec![
        ("id", Json::Num(id as f64)),
        ("method", s(method)),
        ("params", params),
    ]))
}

/// 알림 한 줄.
pub(crate) fn notification(method: &str, params: Json) -> String {
    json_dump(&obj(vec![("method", s(method)), ("params", params)]))
}

/// 서버 요청에 대한 응답 한 줄(id = 서버가 준 것 그대로).
pub(crate) fn response(id: &Json, result: Json) -> String {
    json_dump(&obj(vec![("id", id.clone()), ("result", result)]))
}

pub(crate) fn initialize(id: u64) -> String {
    request(
        id,
        "initialize",
        obj(vec![
            (
                "clientInfo",
                obj(vec![
                    ("name", s("nexa-sql")),
                    ("title", s("Nexa SQL")),
                    ("version", s(env!("CARGO_PKG_VERSION"))),
                ]),
            ),
            (
                "capabilities",
                obj(vec![("experimentalApi", Json::Bool(true))]),
            ),
        ]),
    )
}

pub(crate) fn initialized() -> String {
    notification("initialized", Json::Obj(Vec::new()))
}

pub(crate) fn login_api_key(id: u64, key: &str) -> String {
    request(
        id,
        "account/login/start",
        obj(vec![("type", s("apiKey")), ("apiKey", s(key))]),
    )
}

/// 우리 도구 표 → `dynamicTools`.
pub(crate) fn dynamic_tools() -> Json {
    Json::Arr(
        super::tools::TOOLS
            .iter()
            .map(|t| {
                obj(vec![
                    ("name", s(t.name)),
                    ("description", s(t.desc)),
                    ("inputSchema", t.schema()),
                ])
            })
            .collect(),
    )
}

pub(crate) fn thread_start(id: u64, model: &str, cwd: &str, with_tools: bool) -> String {
    let mut p = vec![
        ("model", s(model)),
        ("cwd", s(cwd)),
        // 파일·명령 도구는 쓰지 않는다 — 승인은 그래도 받는다(우리 도구는 자체 승인 카드).
        ("approvalPolicy", s("untrusted")),
        ("sandbox", s("read-only")),
        ("serviceName", s("nexa-sql")),
    ];
    if with_tools {
        p.push(("dynamicTools", dynamic_tools()));
    }
    request(id, "thread/start", obj(p))
}

pub(crate) fn thread_resume(id: u64, thread_id: &str) -> String {
    request(id, "thread/resume", obj(vec![("threadId", s(thread_id))]))
}

/// 턴 시작 — 글 + 이미지(파일 경로 · app-server는 base64 블록 대신 `localImage`).
pub(crate) fn turn_start(
    id: u64,
    thread_id: &str,
    text: &str,
    local_images: &[String],
    effort: &str,
    system: &str,
) -> String {
    let mut input: Vec<Json> = Vec::new();
    for p in local_images {
        input.push(obj(vec![("type", s("localImage")), ("path", s(p))]));
    }
    let body = if system.is_empty() {
        text.to_string()
    } else {
        format!("<app_context>\n{system}\n</app_context>\n{text}")
    };
    input.push(obj(vec![("type", s("text")), ("text", Json::Str(body))]));
    let mut p = vec![("threadId", s(thread_id)), ("input", Json::Arr(input))];
    if !effort.is_empty() {
        p.push(("effort", s(effort)));
    }
    request(id, "turn/start", obj(p))
}

pub(crate) fn turn_interrupt(id: u64, thread_id: &str, turn_id: &str) -> String {
    request(
        id,
        "turn/interrupt",
        obj(vec![("threadId", s(thread_id)), ("turnId", s(turn_id))]),
    )
}

/// 서버 요청 응답: 도구 호출 결과.
pub(crate) fn tool_call_response(id: &Json, output: &str, ok: bool) -> String {
    response(
        id,
        obj(vec![
            (
                "contentItems",
                Json::Arr(vec![obj(vec![
                    ("type", s("inputText")),
                    ("text", s(output)),
                ])]),
            ),
            ("success", Json::Bool(ok)),
        ]),
    )
}

/// 서버 요청 응답: 명령/파일 승인.
pub(crate) fn approval_response(id: &Json, allow: bool) -> String {
    response(
        id,
        obj(vec![(
            "decision",
            s(if allow { "accept" } else { "decline" }),
        )]),
    )
}

/// 서버 요청 응답: 권한 요청(네트워크·파일) — 전부 거절(우리 도구만 쓴다).
pub(crate) fn permissions_response(id: &Json) -> String {
    response(id, obj(vec![("permissions", Json::Obj(Vec::new()))]))
}

/// 들어온 줄의 분류.
#[derive(Debug)]
pub(crate) enum Incoming {
    /// 우리 요청에 대한 응답(id · result 또는 error 글).
    Response {
        id: u64,
        result: Option<Json>,
        error: Option<String>,
    },
    /// 서버 → 클라이언트 요청(id 원본 · 메서드 · params).
    ServerRequest {
        id: Json,
        method: String,
        params: Json,
    },
    /// 알림 → 이벤트들.
    Events(Vec<AiEvent>),
    Ignore,
}

fn find_key<'a>(v: &'a Json, key: &str) -> Option<&'a Json> {
    match v {
        Json::Obj(items) => {
            for (k, x) in items {
                if k == key {
                    return Some(x);
                }
            }
            for (_, x) in items {
                if let Some(f) = find_key(x, key) {
                    return Some(f);
                }
            }
            None
        }
        Json::Arr(a) => a.iter().find_map(|x| find_key(x, key)),
        _ => None,
    }
}

fn num(v: Option<&Json>) -> u64 {
    match v {
        Some(Json::Num(n)) => *n as u64,
        Some(Json::Str(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// 한 줄 분류(모르는 알림은 `Ignore`).
pub(crate) fn parse_line(line: &str) -> Incoming {
    let line = line.trim();
    if line.is_empty() {
        return Incoming::Ignore;
    }
    let Ok(v) = json_parse(line) else {
        return Incoming::Ignore;
    };
    let method = jstr(&v, "method");
    let id = jget(&v, "id").cloned();
    if method.is_empty() {
        // 응답.
        let Some(id) = id else {
            return Incoming::Ignore;
        };
        let idn = num(Some(&id));
        if let Some(e) = jget(&v, "error") {
            let msg = jstr(e, "message");
            return Incoming::Response {
                id: idn,
                result: None,
                error: Some(if msg.is_empty() { json_dump(e) } else { msg }),
            };
        }
        return Incoming::Response {
            id: idn,
            result: jget(&v, "result").cloned(),
            error: None,
        };
    }
    let params = jget(&v, "params").cloned().unwrap_or(Json::Obj(Vec::new()));
    if let Some(id) = id {
        return Incoming::ServerRequest { id, method, params };
    }
    let mut out = Vec::new();
    match method.as_str() {
        "thread/started" => {
            let tid = jget(&params, "thread")
                .map(|t| jstr(t, "id"))
                .unwrap_or_default();
            out.push(AiEvent::Started {
                cli_session: (!tid.is_empty()).then_some(tid),
                note: "codex thread".into(),
            });
        }
        "item/agentMessage/delta" => {
            let d = jstr(&params, "delta");
            if !d.is_empty() {
                out.push(AiEvent::TextDelta(d));
            }
        }
        "item/started" => {
            if let Some(item) = jget(&params, "item") {
                let ty = jstr(item, "type");
                if ty == "dynamicToolCall" || ty == "mcpToolCall" || ty == "commandExecution" {
                    out.push(AiEvent::ToolCall {
                        id: jstr(item, "id"),
                        name: {
                            let n = jstr(item, "tool");
                            if n.is_empty() {
                                jstr(item, "name")
                            } else {
                                n
                            }
                        },
                        input: jget(item, "arguments")
                            .or_else(|| jget(item, "input"))
                            .or_else(|| jget(item, "command"))
                            .map(json_dump)
                            .unwrap_or_default(),
                    });
                }
            }
        }
        "item/completed" => {
            if let Some(item) = jget(&params, "item") {
                let ty = jstr(item, "type");
                if ty == "dynamicToolCall" || ty == "mcpToolCall" || ty == "commandExecution" {
                    let ok = !matches!(jget(item, "success"), Some(Json::Bool(false)));
                    let output = jget(item, "contentItems")
                        .map(|c| match c {
                            Json::Arr(a) => a
                                .iter()
                                .map(|x| jstr(x, "text"))
                                .filter(|t| !t.is_empty())
                                .collect::<Vec<_>>()
                                .join("\n"),
                            other => json_dump(other),
                        })
                        .or_else(|| {
                            jget(item, "output")
                                .map(|o| o.scalar_text().unwrap_or_else(|| json_dump(o)))
                        })
                        .unwrap_or_default();
                    out.push(AiEvent::ToolResult {
                        id: jstr(item, "id"),
                        name: {
                            let n = jstr(item, "tool");
                            if n.is_empty() {
                                jstr(item, "name")
                            } else {
                                n
                            }
                        },
                        output,
                        ok,
                    });
                }
            }
        }
        "thread/tokenUsage/updated" => {
            let u = jget(&params, "tokenUsage").unwrap_or(&params);
            let total = jget(u, "total").unwrap_or(u);
            out.push(AiEvent::Usage(Usage {
                input: num(find_key(total, "inputTokens")),
                output: num(find_key(total, "outputTokens")),
                cache_read: num(find_key(total, "cachedInputTokens")),
                cache_write: 0,
                cost_usd: 0.0,
            }));
        }
        "turn/completed" => {
            let turn = jget(&params, "turn").unwrap_or(&params);
            let status = jstr(turn, "status");
            if status == "failed" {
                let msg = jget(turn, "error")
                    .map(|e| jstr(e, "message"))
                    .unwrap_or_default();
                out.push(AiEvent::Error(if msg.is_empty() {
                    "turn failed".into()
                } else {
                    msg
                }));
            }
            out.push(AiEvent::Done {
                stop: if status.is_empty() {
                    "completed".into()
                } else {
                    status
                },
                ms: 0,
            });
        }
        "error" => {
            let e = jget(&params, "error").unwrap_or(&params);
            let msg = jstr(e, "message");
            let retry = matches!(jget(&params, "willRetry"), Some(Json::Bool(true)));
            out.push(if retry {
                AiEvent::Note(format!("retrying: {msg}"))
            } else {
                AiEvent::Error(msg)
            });
        }
        "account/login/completed" | "account/updated" => {
            out.push(AiEvent::Note(format!("{method}: {}", json_dump(&params))));
        }
        _ => {}
    }
    if out.is_empty() {
        Incoming::Ignore
    } else {
        Incoming::Events(out)
    }
}

/// 서버 요청의 도구 호출 인자(`item/tool/call`) → (도구 이름 · 입력 JSON).
pub(crate) fn tool_call_args(params: &Json) -> (String, String) {
    let name = {
        let n = jstr(params, "tool");
        if n.is_empty() {
            jstr(params, "name")
        } else {
            n
        }
    };
    let input = jget(params, "arguments")
        .or_else(|| jget(params, "input"))
        .map(json_dump)
        .unwrap_or_else(|| "{}".into());
    (name, input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_notifications() {
        let init = initialize(1);
        assert!(
            init.contains("\"method\":\"initialize\"")
                && init.contains("\"experimentalApi\":true")
                && !init.contains("jsonrpc")
        );
        let ts = thread_start(2, "gpt-6.1-sol", "C:/w", true);
        assert!(
            ts.contains("\"dynamicTools\"")
                && ts.contains("\"run_query\"")
                && ts.contains("\"approvalPolicy\":\"untrusted\"")
        );
        let tu = turn_start(3, "thr_1", "hi", &["a.png".into()], "high", "sys");
        assert!(
            tu.contains("\"localImage\"")
                && tu.contains("<app_context>")
                && tu.contains("\"effort\":\"high\"")
        );
        match parse_line(r#"{"method":"thread/started","params":{"thread":{"id":"thr_1"}}}"#) {
            Incoming::Events(e) => assert!(
                matches!(&e[0], AiEvent::Started { cli_session: Some(t), .. } if t == "thr_1")
            ),
            other => panic!("{other:?}"),
        }
        match parse_line(
            r#"{"method":"item/agentMessage/delta","params":{"threadId":"t","itemId":"i","delta":"Hel"}}"#,
        ) {
            Incoming::Events(e) => assert!(matches!(&e[0], AiEvent::TextDelta(t) if t == "Hel")),
            other => panic!("{other:?}"),
        }
        match parse_line(
            r#"{"id":7,"method":"item/tool/call","params":{"threadId":"t","tool":"get_setting","arguments":{"key":"a"}}}"#,
        ) {
            Incoming::ServerRequest { id, method, params } => {
                assert_eq!(method, "item/tool/call");
                let (n, i) = tool_call_args(&params);
                assert_eq!(n, "get_setting");
                assert!(i.contains("\"key\":\"a\""));
                let r = tool_call_response(&id, "off", true);
                assert!(
                    r.contains("\"id\":7")
                        && r.contains("\"success\":true")
                        && r.contains("inputText")
                );
            }
            other => panic!("{other:?}"),
        }
        match parse_line(
            r#"{"method":"thread/tokenUsage/updated","params":{"threadId":"t","tokenUsage":{"total":{"inputTokens":12,"cachedInputTokens":3,"outputTokens":5},"last":{}}}}"#,
        ) {
            Incoming::Events(e) => assert!(
                matches!(&e[0], AiEvent::Usage(u) if u.input == 12 && u.cache_read == 3 && u.output == 5)
            ),
            other => panic!("{other:?}"),
        }
        match parse_line(
            r#"{"method":"turn/completed","params":{"turn":{"id":"tu","status":"failed","error":{"message":"boom"}}}}"#,
        ) {
            Incoming::Events(e) => {
                assert!(matches!(&e[0], AiEvent::Error(m) if m == "boom"));
                assert!(matches!(&e[1], AiEvent::Done { stop, .. } if stop == "failed"));
            }
            other => panic!("{other:?}"),
        }
        match parse_line(
            r#"{"id":2,"error":{"code":-32001,"message":"Server overloaded; retry later."}}"#,
        ) {
            Incoming::Response {
                id: 2,
                error: Some(e),
                ..
            } => assert!(e.contains("overloaded")),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            parse_line("{\"method\":\"thread/status/changed\",\"params\":{}}"),
            Incoming::Ignore
        ));
    }
}
