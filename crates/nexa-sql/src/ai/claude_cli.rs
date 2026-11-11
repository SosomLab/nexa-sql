//! ★ Claude Code CLI 호스팅(docs/110 §5 · headless 프로토콜): `claude -p --output-format stream-json --input-format stream-json
//! --include-partial-messages --verbose …`의 **줄 단위 JSON**을 [`AiEvent`]로 바꾼다. 순수 함수 — 자식 프로세스 관리는 `host.rs`.

use super::{jget, jstr, AiEvent, Usage};
use nsql_settings::json::{parse as json_parse, Json};

/// 실행 인자(110 §5 · §7): `--bare`(사용자 홈 훅·MCP 무시) · 파일 도구는 주지 않는다 · 권한 모드 · MCP 설정(있으면) · 재개.
pub(crate) fn args(
    model: &str,
    effort: &str,
    permission_mode: &str,
    mcp_config: Option<&str>,
    resume: Option<&str>,
    bare: bool,
    system_prompt: &str,
) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-p".into(),
        "--output-format".into(),
        "stream-json".into(),
        "--input-format".into(),
        "stream-json".into(),
        "--include-partial-messages".into(),
        "--verbose".into(),
        "--model".into(),
        model.to_string(),
        "--permission-mode".into(),
        permission_mode.to_string(),
    ];
    if bare {
        a.push("--bare".into());
    }
    if !effort.is_empty() {
        a.push("--effort".into());
        a.push(effort.to_string());
    }
    if let Some(m) = mcp_config {
        a.push("--mcp-config".into());
        a.push(m.to_string());
        // ★ 사용자 PC의 다른 MCP 서버는 올리지 않는다(11-11 실기: 그 서버들만 올라오고 nexa가 빠져 승인 도구를 못 찾음).
        a.push("--strict-mcp-config".into());
        // 권한 프롬프트 = 우리 MCP 승인 도구(앱 확인 창).
        a.push("--permission-prompt-tool".into());
        a.push("mcp__nexa__approve".into());
    }
    if let Some(r) = resume {
        a.push("--resume".into());
        a.push(r.to_string());
    }
    if !system_prompt.is_empty() {
        a.push("--append-system-prompt".into());
        a.push(system_prompt.to_string());
    }
    a
}

/// stdin으로 보내는 사용자 턴 한 줄(stream-json 입력 · 텍스트 + 이미지 블록).
pub(crate) fn user_turn_line(text: &str, images: &[(String, Vec<u8>)]) -> String {
    let mut content: Vec<Json> = Vec::new();
    for (media, bytes) in images {
        content.push(Json::Obj(vec![
            ("type".into(), Json::Str("image".into())),
            (
                "source".into(),
                Json::Obj(vec![
                    ("type".into(), Json::Str("base64".into())),
                    ("media_type".into(), Json::Str(media.clone())),
                    ("data".into(), Json::Str(base64(bytes))),
                ]),
            ),
        ]));
    }
    content.push(Json::Obj(vec![
        ("type".into(), Json::Str("text".into())),
        ("text".into(), Json::Str(text.to_string())),
    ]));
    let msg = Json::Obj(vec![
        ("type".into(), Json::Str("user".into())),
        (
            "message".into(),
            Json::Obj(vec![
                ("role".into(), Json::Str("user".into())),
                ("content".into(), Json::Arr(content)),
            ]),
        ),
    ]);
    nsql_settings::json::dump(&msg)
}

/// stdout 한 줄 → 이벤트들(한 줄이 여러 이벤트가 될 수 있다 · 모르는 줄은 비움).
pub(crate) fn parse_line(line: &str) -> Vec<AiEvent> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }
    let Ok(v) = json_parse(line) else {
        return Vec::new();
    };
    let ty = jstr(&v, "type");
    let mut out = Vec::new();
    match ty.as_str() {
        "system" => {
            let sub = jstr(&v, "subtype");
            match sub.as_str() {
                "init" => {
                    let sid = jstr(&v, "session_id");
                    let tools = match jget(&v, "tools") {
                        Some(Json::Arr(a)) => a.len(),
                        _ => 0,
                    };
                    let mcp = match jget(&v, "mcp_servers") {
                        Some(Json::Arr(a)) => a
                            .iter()
                            .map(|s| format!("{}:{}", jstr(s, "name"), jstr(s, "status")))
                            .collect::<Vec<_>>()
                            .join(" "),
                        _ => String::new(),
                    };
                    out.push(AiEvent::Started {
                        cli_session: (!sid.is_empty()).then_some(sid),
                        note: format!("model={} tools={tools} mcp=[{mcp}]", jstr(&v, "model")),
                    });
                }
                "api_retry" => out.push(AiEvent::Note(format!(
                    "retry {}/{} in {} ms ({})",
                    jstr(&v, "attempt"),
                    jstr(&v, "max_retries"),
                    jstr(&v, "retry_delay_ms"),
                    jstr(&v, "error")
                ))),
                "permission_denied" => out.push(AiEvent::Note(format!(
                    "permission denied: {}",
                    jstr(&v, "tool_name")
                ))),
                _ => {}
            }
        }
        "stream_event" => {
            if let Some(ev) = jget(&v, "event") {
                let et = jstr(ev, "type");
                if et == "content_block_delta" {
                    if let Some(d) = jget(ev, "delta") {
                        if jstr(d, "type") == "text_delta" {
                            out.push(AiEvent::TextDelta(jstr(d, "text")));
                        }
                    }
                } else if et == "message_delta" {
                    if let Some(u) = jget(ev, "usage") {
                        out.push(AiEvent::Usage(usage_of(u)));
                    }
                }
            }
        }
        "assistant" => {
            // 완성된 블록 — 도구 호출만 꺼낸다(글은 stream_event가 이미 흘렸다).
            if let Some(Json::Arr(blocks)) = jget(&v, "message").and_then(|m| jget(m, "content")) {
                for b in blocks {
                    if jstr(b, "type") == "tool_use" {
                        out.push(AiEvent::ToolCall {
                            id: jstr(b, "id"),
                            name: jstr(b, "name"),
                            input: jget(b, "input")
                                .map(nsql_settings::json::dump)
                                .unwrap_or_default(),
                        });
                    }
                }
            }
        }
        "user" => {
            if let Some(Json::Arr(blocks)) = jget(&v, "message").and_then(|m| jget(m, "content")) {
                for b in blocks {
                    if jstr(b, "type") == "tool_result" {
                        let content = match jget(b, "content") {
                            Some(Json::Str(s)) => s.clone(),
                            Some(Json::Arr(a)) => a
                                .iter()
                                .map(|c| jstr(c, "text"))
                                .filter(|t| !t.is_empty())
                                .collect::<Vec<_>>()
                                .join("\n"),
                            _ => String::new(),
                        };
                        let is_err = matches!(jget(b, "is_error"), Some(Json::Bool(true)));
                        out.push(AiEvent::ToolResult {
                            id: jstr(b, "tool_use_id"),
                            name: String::new(),
                            output: content,
                            ok: !is_err,
                        });
                    }
                }
            }
        }
        "result" => {
            let ms = match jget(&v, "duration_ms") {
                Some(Json::Num(n)) => *n as u64,
                _ => 0,
            };
            if let Some(u) = jget(&v, "usage") {
                let mut usage = usage_of(u);
                if let Some(Json::Num(c)) = jget(&v, "total_cost_usd") {
                    usage.cost_usd = *c;
                }
                // result의 usage는 턴 합계라 델타가 아니다 — 호출자가 "합계로 바꾸기"로 다룬다(Done 앞에 Usage 하나).
                out.push(AiEvent::Usage(usage));
            }
            let sub = jstr(&v, "subtype");
            if sub.starts_with("error") || matches!(jget(&v, "is_error"), Some(Json::Bool(true))) {
                out.push(AiEvent::Error(jstr(&v, "result")));
            }
            out.push(AiEvent::Done {
                stop: if sub.is_empty() { "end".into() } else { sub },
                ms,
            });
        }
        _ => {}
    }
    out
}

pub(crate) fn usage_of(u: &Json) -> Usage {
    let n = |k: &str| match jget(u, k) {
        Some(Json::Num(n)) => *n as u64,
        _ => 0,
    };
    Usage {
        input: n("input_tokens"),
        output: n("output_tokens"),
        cache_read: n("cache_read_input_tokens"),
        cache_write: n("cache_creation_input_tokens"),
        cost_usd: 0.0,
    }
}

/// base64(표준 · 패딩) — 외부 crate 0.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_init_delta_tool_result() {
        let init = r#"{"type":"system","subtype":"init","session_id":"s1","model":"claude-opus-5-5","tools":["Read"],"mcp_servers":[{"name":"nexa","status":"connected"}]}"#;
        let ev = parse_line(init);
        assert!(
            matches!(&ev[0], AiEvent::Started { cli_session: Some(s), note } if s == "s1" && note.contains("mcp=[nexa:connected]"))
        );
        let delta = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hi"}}}"#;
        assert!(matches!(&parse_line(delta)[0], AiEvent::TextDelta(t) if t == "Hi"));
        let tool = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"mcp__nexa__run_query","input":{"sql":"select 1"}}]}}"#;
        assert!(
            matches!(&parse_line(tool)[0], AiEvent::ToolCall { id, name, input } if id == "t1" && name.ends_with("run_query") && input.contains("select 1"))
        );
        let res = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"1 row","is_error":false}]}}"#;
        assert!(
            matches!(&parse_line(res)[0], AiEvent::ToolResult { id, ok: true, output, .. } if id == "t1" && output == "1 row")
        );
        let done = r#"{"type":"result","subtype":"success","duration_ms":1234,"total_cost_usd":0.01,"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":3},"result":"ok"}"#;
        let ev = parse_line(done);
        assert!(
            matches!(&ev[0], AiEvent::Usage(u) if u.input == 10 && u.cache_read == 3 && (u.cost_usd - 0.01).abs() < 1e-9)
        );
        assert!(matches!(&ev[1], AiEvent::Done { stop, ms: 1234 } if stop == "success"));
        assert!(parse_line("garbage").is_empty());
    }

    #[test]
    fn args_and_turn_line() {
        let a = args(
            "claude-opus-5-5",
            "high",
            "default",
            Some("{}"),
            Some("s1"),
            true,
            "You help with SQL",
        );
        let s = a.join(" ");
        assert!(s.contains("-p --output-format stream-json --input-format stream-json"));
        assert!(
            s.contains("--bare")
                && s.contains("--resume s1")
                && s.contains("--permission-prompt-tool mcp__nexa__approve")
        );
        let line = user_turn_line("hello", &[("image/png".into(), vec![1, 2, 3])]);
        assert!(
            line.contains("\"type\":\"user\"")
                && line.contains("\"image\"")
                && line.contains("AQID")
        );
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
    }
}
