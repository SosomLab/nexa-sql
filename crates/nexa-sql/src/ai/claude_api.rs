//! ★ Claude Messages API 직접 호출(docs/110 §14-1 · API 모드) — 요청 본문 조립 · SSE 스트림 파싱(순수) · 턴 루프(스레드).
//! 전송은 `nsql-net`(rustls) · 도구 호출은 UI 스레드에 [`AiEvent::ToolRequest`]로 부탁하고 답을 기다린다.
//! 외부 crate 0 — JSON은 `nsql-json`.

use super::{jget, jstr, AiEvent, Attachment, ToolReply, Usage};
use nsql_settings::json::{dump as json_dump, parse as json_parse, Json};
use std::io::Write;
use std::sync::mpsc;

pub(crate) const API_VERSION: &str = "2023-06-01";

/// 인증 — API 키(`x-api-key`) 또는 WIF 단명 토큰(`Authorization: Bearer` · 110 §14-5 · §15-5).
#[derive(Clone, Debug)]
pub(crate) enum ApiAuth {
    Key(String),
    Bearer(String),
}

impl ApiAuth {
    pub(crate) fn header(&self) -> (String, String) {
        match self {
            ApiAuth::Key(k) => ("x-api-key".into(), k.clone()),
            ApiAuth::Bearer(t) => ("authorization".into(), format!("Bearer {t}")),
        }
    }
}

/// 요청 본문(스트리밍 · 적응형 사고 · 노력도 · 프롬프트 캐시 = system + 마지막 메시지).
pub(crate) fn request_body(
    model: &str,
    effort: &str,
    system: &str,
    messages: &[Json],
    tools: Json,
    max_tokens: u64,
) -> Json {
    let mut msgs = messages.to_vec();
    // 마지막 사용자 메시지의 마지막 블록에 cache_control(대화 접두가 그대로 캐시).
    if let Some(Json::Obj(last)) = msgs.last_mut() {
        if let Some((_, Json::Arr(blocks))) = last.iter_mut().find(|(k, _)| k == "content") {
            if let Some(Json::Obj(b)) = blocks.last_mut() {
                b.retain(|(k, _)| k != "cache_control");
                b.push((
                    "cache_control".into(),
                    Json::Obj(vec![("type".into(), Json::Str("ephemeral".into()))]),
                ));
            }
        }
    }
    let mut body = vec![
        ("model".into(), Json::Str(model.into())),
        ("max_tokens".into(), Json::Num(max_tokens as f64)),
        ("stream".into(), Json::Bool(true)),
        (
            "system".into(),
            Json::Arr(vec![Json::Obj(vec![
                ("type".into(), Json::Str("text".into())),
                ("text".into(), Json::Str(system.into())),
                (
                    "cache_control".into(),
                    Json::Obj(vec![("type".into(), Json::Str("ephemeral".into()))]),
                ),
            ])]),
        ),
        ("messages".into(), Json::Arr(msgs)),
        ("tools".into(), tools),
        (
            "thinking".into(),
            Json::Obj(vec![("type".into(), Json::Str("adaptive".into()))]),
        ),
    ];
    if !effort.is_empty() {
        body.push((
            "output_config".into(),
            Json::Obj(vec![("effort".into(), Json::Str(effort.into()))]),
        ));
    }
    Json::Obj(body)
}

/// 사용자 메시지(글 + 첨부 블록).
pub(crate) fn user_message(text: &str, attachments: &[Attachment]) -> Json {
    let mut content: Vec<Json> = Vec::new();
    for a in attachments {
        match a {
            Attachment::Text { label, body } => content.push(Json::Obj(vec![
                ("type".into(), Json::Str("text".into())),
                (
                    "text".into(),
                    Json::Str(format!(
                        "<attachment name=\"{label}\">\n{body}\n</attachment>"
                    )),
                ),
            ])),
            Attachment::Image { media, bytes, .. } => content.push(Json::Obj(vec![
                ("type".into(), Json::Str("image".into())),
                (
                    "source".into(),
                    Json::Obj(vec![
                        ("type".into(), Json::Str("base64".into())),
                        ("media_type".into(), Json::Str(media.clone())),
                        ("data".into(), Json::Str(super::claude_cli::base64(bytes))),
                    ]),
                ),
            ])),
            Attachment::Pdf { bytes, .. } => content.push(Json::Obj(vec![
                ("type".into(), Json::Str("document".into())),
                (
                    "source".into(),
                    Json::Obj(vec![
                        ("type".into(), Json::Str("base64".into())),
                        ("media_type".into(), Json::Str("application/pdf".into())),
                        ("data".into(), Json::Str(super::claude_cli::base64(bytes))),
                    ]),
                ),
            ])),
        }
    }
    content.push(Json::Obj(vec![
        ("type".into(), Json::Str("text".into())),
        ("text".into(), Json::Str(text.into())),
    ]));
    Json::Obj(vec![
        ("role".into(), Json::Str("user".into())),
        ("content".into(), Json::Arr(content)),
    ])
}

/// 도구 결과 메시지(한 턴의 도구 호출 전부).
pub(crate) fn tool_result_message(results: &[(String, ToolReply)]) -> Json {
    Json::Obj(vec![
        ("role".into(), Json::Str("user".into())),
        (
            "content".into(),
            Json::Arr(
                results
                    .iter()
                    .map(|(id, r)| {
                        Json::Obj(vec![
                            ("type".into(), Json::Str("tool_result".into())),
                            ("tool_use_id".into(), Json::Str(id.clone())),
                            ("content".into(), Json::Str(r.output.clone())),
                            ("is_error".into(), Json::Bool(!r.ok)),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

/// 스트림에서 조립한 응답 한 벌.
#[derive(Debug, Default, Clone)]
pub(crate) struct Assembled {
    /// 응답 content 블록(사고 블록·서명 포함 · 다음 턴에 그대로 되돌린다).
    pub blocks: Vec<Json>,
    pub stop_reason: String,
    pub usage: Usage,
    pub error: Option<String>,
}

impl Assembled {
    pub(crate) fn tool_uses(&self) -> Vec<(String, String, String)> {
        self.blocks
            .iter()
            .filter(|b| jstr(b, "type") == "tool_use")
            .map(|b| {
                (
                    jstr(b, "id"),
                    jstr(b, "name"),
                    jget(b, "input").map(json_dump).unwrap_or_default(),
                )
            })
            .collect()
    }
    pub(crate) fn assistant_message(&self) -> Json {
        Json::Obj(vec![
            ("role".into(), Json::Str("assistant".into())),
            ("content".into(), Json::Arr(self.blocks.clone())),
        ])
    }
}

/// SSE 싱크 — `nsql_net::request`의 `sink`로 꽂혀 조각마다 이벤트 경계(`\n\n`)로 자르고 블록을 조립한다.
pub(crate) struct SseSink<'a> {
    buf: Vec<u8>,
    /// 조립 중 블록(인덱스 순) · 도구 입력 JSON 조각 · 사고 서명.
    pub out: Assembled,
    partial_json: Vec<String>,
    emit: &'a mut dyn FnMut(AiEvent),
}

impl<'a> SseSink<'a> {
    pub(crate) fn new(emit: &'a mut dyn FnMut(AiEvent)) -> Self {
        SseSink {
            buf: Vec::new(),
            out: Assembled::default(),
            partial_json: Vec::new(),
            emit,
        }
    }

    fn handle_event(&mut self, data: &str) {
        let Ok(v) = json_parse(data) else { return };
        match jstr(&v, "type").as_str() {
            "message_start" => {
                if let Some(u) = jget(&v, "message").and_then(|m| jget(m, "usage")) {
                    let u = super::claude_cli::usage_of(u);
                    self.out.usage.input = u.input;
                    self.out.usage.cache_read = u.cache_read;
                    self.out.usage.cache_write = u.cache_write;
                }
            }
            "content_block_start" => {
                let idx = index_of(&v);
                let block = jget(&v, "content_block").cloned().unwrap_or(Json::Null);
                let mut block = match block {
                    Json::Obj(items) => items,
                    _ => Vec::new(),
                };
                // 도구 입력은 델타로 온다 — 비워 두고 끝에 채운다.
                if block
                    .iter()
                    .any(|(k, v)| k == "type" && matches!(v, Json::Str(s) if s == "tool_use"))
                {
                    block.retain(|(k, _)| k != "input");
                }
                while self.out.blocks.len() <= idx {
                    self.out.blocks.push(Json::Obj(Vec::new()));
                    self.partial_json.push(String::new());
                }
                self.out.blocks[idx] = Json::Obj(block);
            }
            "content_block_delta" => {
                let idx = index_of(&v);
                let Some(d) = jget(&v, "delta") else { return };
                while self.out.blocks.len() <= idx {
                    self.out.blocks.push(Json::Obj(Vec::new()));
                    self.partial_json.push(String::new());
                }
                match jstr(d, "type").as_str() {
                    "text_delta" => {
                        let t = jstr(d, "text");
                        append_str(&mut self.out.blocks[idx], "text", &t);
                        (self.emit)(AiEvent::TextDelta(t));
                    }
                    "thinking_delta" => {
                        append_str(&mut self.out.blocks[idx], "thinking", &jstr(d, "thinking"));
                    }
                    "signature_delta" => {
                        append_str(
                            &mut self.out.blocks[idx],
                            "signature",
                            &jstr(d, "signature"),
                        );
                    }
                    "input_json_delta" => {
                        self.partial_json[idx].push_str(&jstr(d, "partial_json"));
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let idx = index_of(&v);
                if let (Some(Json::Obj(b)), Some(pj)) =
                    (self.out.blocks.get_mut(idx), self.partial_json.get(idx))
                {
                    if b.iter()
                        .any(|(k, v)| k == "type" && matches!(v, Json::Str(s) if s == "tool_use"))
                    {
                        let input = if pj.trim().is_empty() {
                            Json::Obj(Vec::new())
                        } else {
                            json_parse(pj).unwrap_or(Json::Obj(Vec::new()))
                        };
                        b.retain(|(k, _)| k != "input");
                        b.push(("input".into(), input.clone()));
                        (self.emit)(AiEvent::ToolCall {
                            id: jstr(&Json::Obj(b.clone()), "id"),
                            name: jstr(&Json::Obj(b.clone()), "name"),
                            input: json_dump(&input),
                        });
                    }
                }
            }
            "message_delta" => {
                if let Some(d) = jget(&v, "delta") {
                    let s = jstr(d, "stop_reason");
                    if !s.is_empty() {
                        self.out.stop_reason = s;
                    }
                }
                if let Some(u) = jget(&v, "usage") {
                    let u = super::claude_cli::usage_of(u);
                    self.out.usage.output = u.output;
                }
            }
            "error" => {
                let msg = jget(&v, "error")
                    .map(|e| jstr(e, "message"))
                    .unwrap_or_default();
                self.out.error = Some(if msg.is_empty() {
                    data.to_string()
                } else {
                    msg
                });
            }
            _ => {}
        }
    }

    fn drain_events(&mut self) {
        while let Some((chunk, rest_at)) = find_double_newline(&self.buf) {
            let raw: Vec<u8> = self.buf[..chunk].to_vec();
            self.buf.drain(..rest_at);
            let text = String::from_utf8_lossy(&raw).into_owned();
            let mut data = String::new();
            for line in text.lines() {
                if let Some(d) = line.strip_prefix("data:") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(d.trim_start());
                }
            }
            if !data.is_empty() {
                self.handle_event(&data);
            }
        }
    }

    /// 스트림이 끝난 뒤 남은 조각 처리(마지막 사건에 빈 줄이 없을 때).
    pub(crate) fn finish(&mut self) {
        if !self.buf.is_empty() {
            self.buf.extend_from_slice(b"\n\n");
            self.drain_events();
        }
    }
}

fn find_double_newline(b: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'\n' && b[i + 1] == b'\n' {
            return Some((i, i + 2));
        }
        if i + 3 < b.len() && &b[i..i + 4] == b"\r\n\r\n" {
            return Some((i, i + 4));
        }
        i += 1;
    }
    None
}

fn index_of(v: &Json) -> usize {
    match jget(v, "index") {
        Some(Json::Num(n)) => *n as usize,
        _ => 0,
    }
}

fn append_str(block: &mut Json, key: &str, add: &str) {
    if let Json::Obj(items) = block {
        match items.iter_mut().find(|(k, _)| k == key) {
            Some((_, Json::Str(s))) => s.push_str(add),
            Some(slot) => slot.1 = Json::Str(add.to_string()),
            None => items.push((key.to_string(), Json::Str(add.to_string()))),
        }
    }
}

impl Write for SseSink<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(buf);
        self.drain_events();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 한 턴의 네트워크 호출 — 응답을 조립해 돌려준다(HTTP 오류는 본문을 오류로).
pub(crate) fn stream_once(
    base_url: &str,
    auth: &ApiAuth,
    workspace: &str,
    body: &Json,
    timeout_secs: u64,
    emit: &mut dyn FnMut(AiEvent),
) -> Result<Assembled, String> {
    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
    let mut o = nsql_net::Options {
        timeout: std::time::Duration::from_secs(timeout_secs.max(10)),
        io_timeout: Some(std::time::Duration::from_secs(timeout_secs.max(10))),
        ..nsql_net::Options::default()
    };
    o.headers.push(auth.header());
    o.headers
        .push(("anthropic-version".into(), API_VERSION.into()));
    o.headers
        .push(("content-type".into(), "application/json".into()));
    o.headers
        .push(("accept".into(), "text/event-stream".into()));
    if !workspace.is_empty() {
        o.headers
            .push(("anthropic-workspace-id".into(), workspace.to_string()));
    }
    let payload = json_dump(body).into_bytes();
    let mut sink = SseSink::new(emit);
    let r = nsql_net::request("POST", &url, Some(&payload), &o, &mut sink, &mut |_, _| {});
    sink.finish();
    match r {
        Ok(resp) if (200..300).contains(&resp.status) => {
            if let Some(e) = sink.out.error.take() {
                return Err(e);
            }
            Ok(sink.out)
        }
        Ok(resp) => {
            // 오류 본문은 SSE가 아니라 JSON 한 덩어리 — 싱크 버퍼에 남아 있다.
            let text = String::from_utf8_lossy(&sink.buf).into_owned();
            let msg = json_parse(text.trim())
                .ok()
                .and_then(|v| jget(&v, "error").map(|e| jstr(e, "message")))
                .filter(|m| !m.is_empty())
                .unwrap_or(text);
            Err(format!("HTTP {}: {}", resp.status, msg.trim()))
        }
        Err(e) => Err(e.to_string()),
    }
}

/// `/v1/models` 한 번으로 키 검증 + 모델 id 목록(프로필 관리 [검증] · 110 §4 · §15-5 Models API 조회 → 프로필 `models`).
pub(crate) fn list_models(
    base_url: &str,
    auth: &ApiAuth,
    workspace: &str,
) -> Result<Vec<String>, String> {
    let url = format!("{}/v1/models?limit=100", base_url.trim_end_matches('/'));
    let mut o = nsql_net::Options {
        timeout: std::time::Duration::from_secs(20),
        ..nsql_net::Options::default()
    };
    o.headers.push(auth.header());
    o.headers
        .push(("anthropic-version".into(), API_VERSION.into()));
    if !workspace.is_empty() {
        o.headers
            .push(("anthropic-workspace-id".into(), workspace.to_string()));
    }
    let mut out = Vec::new();
    match nsql_net::get(&url, &o, &mut out, &mut |_, _| {}) {
        Ok(resp) if (200..300).contains(&resp.status) => {
            let text = String::from_utf8_lossy(&out);
            let ids = json_parse(&text)
                .ok()
                .and_then(|v| match jget(&v, "data") {
                    Some(Json::Arr(a)) => Some(
                        a.iter()
                            .map(|m| jstr(m, "id"))
                            .filter(|id| !id.is_empty())
                            .collect::<Vec<_>>(),
                    ),
                    _ => None,
                })
                .unwrap_or_default();
            Ok(ids)
        }
        Ok(resp) => {
            let text = String::from_utf8_lossy(&out).into_owned();
            let msg = json_parse(text.trim())
                .ok()
                .and_then(|v| jget(&v, "error").map(|e| jstr(e, "message")))
                .filter(|m| !m.is_empty())
                .unwrap_or(text);
            Err(format!("HTTP {}: {}", resp.status, msg.trim()))
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 턴 루프(스레드에서) — 도구 호출이 끝날 때까지 되돌고 마지막 조립본을 돌려준다. `messages`는 호출자가 보관(다음 턴에 이어 씀).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_turn(
    base_url: &str,
    auth: &ApiAuth,
    workspace: &str,
    model: &str,
    effort: &str,
    system: &str,
    messages: &mut Vec<Json>,
    timeout_secs: u64,
    max_tokens: u64,
    prices: &str,
    tx: &mpsc::Sender<AiEvent>,
    stop: &std::sync::atomic::AtomicBool,
) {
    let started = std::time::Instant::now();
    let mut total = Usage::default();
    for round in 0..64 {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = tx.send(AiEvent::Done {
                stop: "cancelled".into(),
                ms: started.elapsed().as_millis() as u64,
            });
            return;
        }
        let body = request_body(
            model,
            effort,
            system,
            messages,
            super::tools::api_tools(),
            max_tokens,
        );
        let tx2 = tx.clone();
        let mut emit = move |e: AiEvent| {
            let _ = tx2.send(e);
        };
        let asm = match stream_once(base_url, auth, workspace, &body, timeout_secs, &mut emit) {
            Ok(a) => a,
            Err(e) => {
                let _ = tx.send(AiEvent::Error(e));
                let _ = tx.send(AiEvent::Done {
                    stop: "error".into(),
                    ms: started.elapsed().as_millis() as u64,
                });
                return;
            }
        };
        let mut u = asm.usage;
        u.cost_usd = Usage::estimate_cost_in(prices, model, u.input, u.output, u.cache_read);
        total.add(u);
        let _ = tx.send(AiEvent::Usage(u));
        messages.push(asm.assistant_message());
        let uses = asm.tool_uses();
        if asm.stop_reason != "tool_use" || uses.is_empty() {
            let _ = tx.send(AiEvent::Done {
                stop: if asm.stop_reason.is_empty() {
                    "end_turn".into()
                } else {
                    asm.stop_reason
                },
                ms: started.elapsed().as_millis() as u64,
            });
            return;
        }
        let mut results: Vec<(String, ToolReply)> = Vec::new();
        for (id, name, input) in uses {
            let (rtx, rrx) = mpsc::channel();
            let _ = tx.send(AiEvent::ToolRequest {
                id: id.clone(),
                name: name.clone(),
                input,
                reply: rtx,
            });
            let reply = rrx
                .recv_timeout(std::time::Duration::from_secs(timeout_secs.max(30) * 4))
                .unwrap_or(ToolReply {
                    output: "tool timed out or cancelled".into(),
                    ok: false,
                });
            let _ = tx.send(AiEvent::ToolResult {
                id: id.clone(),
                name,
                output: reply.output.clone(),
                ok: reply.ok,
            });
            results.push((id, reply));
        }
        messages.push(tool_result_message(&results));
        if round == 63 {
            let _ = tx.send(AiEvent::Error("too many tool rounds".into()));
        }
    }
    let _ = tx.send(AiEvent::Done {
        stop: "max_rounds".into(),
        ms: started.elapsed().as_millis() as u64,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_assembles_text_and_tool_use() {
        let mut got: Vec<AiEvent> = Vec::new();
        let mut emit = |e: AiEvent| got.push(e);
        let mut sink = SseSink::new(&mut emit);
        let stream = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":12,\"cache_read_input_tokens\":4}}}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hel\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu1\",\"name\":\"get_setting\",\"input\":{}}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"key\\\":\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"editor.word_wrap\\\"}\"}}\n\n",
            "data: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":7}}\n\n",
        );
        // 조각 경계가 사건 중간에 오도록 둘로 나눠 쓴다.
        let bytes = stream.as_bytes();
        sink.write_all(&bytes[..100]).expect("w");
        sink.write_all(&bytes[100..]).expect("w");
        sink.finish();
        let out = sink.out.clone();
        assert_eq!(out.stop_reason, "tool_use");
        assert_eq!(out.usage.input, 12);
        assert_eq!(out.usage.output, 7);
        assert_eq!(out.usage.cache_read, 4);
        let uses = out.tool_uses();
        assert_eq!(uses.len(), 1);
        assert_eq!(uses[0].1, "get_setting");
        assert!(uses[0].2.contains("editor.word_wrap"), "{}", uses[0].2);
        let text = json_dump(&out.blocks[0]);
        assert!(text.contains("\"Hello\""), "{text}");
        let deltas: Vec<&AiEvent> = got
            .iter()
            .filter(|e| matches!(e, AiEvent::TextDelta(_)))
            .collect();
        assert_eq!(deltas.len(), 2);
        assert!(got
            .iter()
            .any(|e| matches!(e, AiEvent::ToolCall { name, .. } if name == "get_setting")));
    }

    #[test]
    fn body_has_cache_and_effort() {
        let m = vec![user_message(
            "hi",
            &[Attachment::Text {
                label: "a.sql".into(),
                body: "select 1".into(),
            }],
        )];
        let b = json_dump(&request_body(
            "claude-opus-5-5",
            "high",
            "sys",
            &m,
            Json::Arr(vec![]),
            4096,
        ));
        assert!(
            b.contains("\"stream\":true")
                && b.contains("\"adaptive\"")
                && b.contains("\"effort\":\"high\"")
        );
        assert!(b.matches("ephemeral").count() == 2, "{b}");
        assert!(b.contains("<attachment name=\\\"a.sql\\\">"));
        let r = json_dump(&tool_result_message(&[(
            "t1".into(),
            ToolReply {
                output: "ok".into(),
                ok: true,
            },
        )]));
        assert!(r.contains("\"tool_use_id\":\"t1\"") && r.contains("\"is_error\":false"));
    }
}
