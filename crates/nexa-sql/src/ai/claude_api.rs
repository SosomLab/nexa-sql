//! ★ Claude Messages API 직접 호출(docs/110 §14-1 · API 모드) — 요청 본문 조립 · SSE 스트림 파싱(순수) · 턴 루프(스레드).
//! 전송은 `nsql-net`(rustls) · 도구 호출은 UI 스레드에 [`AiEvent::ToolRequest`]로 부탁하고 답을 기다린다.
//! 외부 crate 0 — JSON은 `nsql-json`.

use super::{jget, jstr, AiEvent, Attachment, ToolReply, Usage};
use nsql_settings::json::{dump as json_dump, parse as json_parse, Json};
use std::io::Write;
use std::sync::mpsc;

pub(crate) const API_VERSION: &str = "2023-06-01";

/// 인증 — API 키(`x-api-key`) 또는 WIF 단명 토큰(`Authorization: Bearer` · 110 §14-5 · §15-5).
#[derive(Clone)]
pub(crate) enum ApiAuth {
    Key(String),
    Bearer(String),
}

/// ★ `{:?}`에 키·토큰 글자를 싣지 않는다(11-13 · 비밀은 로그·덤프·Debug 출력 금지) — 종류만 보인다.
impl std::fmt::Debug for ApiAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ApiAuth::Key(_) => "ApiAuth::Key(***)",
            ApiAuth::Bearer(_) => "ApiAuth::Bearer(***)",
        })
    }
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
/// 메시지의 content 블록 배열 끝에 블록을 덧붙인다(배열이 아니면 그대로 둔다).
pub(crate) fn append_content(msg: &mut Json, blocks: Vec<Json>) {
    if blocks.is_empty() {
        return;
    }
    if let Json::Obj(fields) = msg {
        if let Some((_, Json::Arr(content))) = fields.iter_mut().find(|(k, _)| k == "content") {
            content.extend(blocks);
        }
    }
}

/// 진행 중 턴에 덧붙일 사용자 요청의 content 블록(첨부 블록 + 글) — [`user_message`]와 같은 모양.
pub(crate) fn steer_blocks(text: &str, attachments: &[super::Attachment]) -> Vec<Json> {
    match user_message(text, attachments) {
        Json::Obj(fields) => fields
            .into_iter()
            .find_map(|(k, v)| match v {
                Json::Arr(a) if k == "content" => Some(a),
                _ => None,
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// 대화 재구성: user 메시지가 연달아 오면(진행 중 턴에 반영한 요청 · 답 없이 끝난 요청) 앞 메시지에 합친다.
pub(crate) fn push_merged(msgs: &mut Vec<Json>, m: Json) {
    let is_user = |j: &Json| super::jstr(j, "role") == "user";
    if is_user(&m) && msgs.last().is_some_and(is_user) {
        if let (Some(last), Json::Obj(fields)) = (msgs.last_mut(), m) {
            let blocks = fields
                .into_iter()
                .find_map(|(k, v)| match v {
                    Json::Arr(a) if k == "content" => Some(a),
                    _ => None,
                })
                .unwrap_or_default();
            append_content(last, blocks);
        }
        return;
    }
    msgs.push(m);
}

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
    /// ★ 받은 본문의 앞부분 원문(상한 [`RAW_KEEP`] · 사용자 11-13 "HTTP 503: " 뒤가 비어 있었다): HTTP 오류의 본문은 SSE가 아니라
    /// JSON 한 덩어리인데, `finish`가 남은 조각을 사건으로 풀다가(`data:` 줄이 없어) 버려 **오류 글이 늘 비었다** → 원문을 따로 든다.
    raw: Vec<u8>,
    /// 조립 중 블록(인덱스 순) · 도구 입력 JSON 조각 · 사고 서명.
    pub out: Assembled,
    partial_json: Vec<String>,
    emit: &'a mut dyn FnMut(AiEvent),
    /// ■ 깃발(11-15): 서 있으면 다음 조각에서 받기를 끊는다 — 종전에는 멈춘 뒤에도 응답을 끝까지 받았다.
    stop: Option<&'a std::sync::atomic::AtomicBool>,
}

impl<'a> SseSink<'a> {
    pub(crate) fn new(emit: &'a mut dyn FnMut(AiEvent)) -> Self {
        SseSink {
            buf: Vec::new(),
            raw: Vec::new(),
            out: Assembled::default(),
            partial_json: Vec::new(),
            emit,
            stop: None,
        }
    }

    /// ■ 깃발을 건다 — 서면 `write`가 오류로 돌아가 요청이 끊긴다.
    pub(crate) fn with_stop(mut self, stop: Option<&'a std::sync::atomic::AtomicBool>) -> Self {
        self.stop = stop;
        self
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
        if self
            .stop
            .is_some_and(|s| s.load(std::sync::atomic::Ordering::Relaxed))
        {
            return Err(std::io::Error::other("stopped"));
        }
        if self.raw.len() < RAW_KEEP {
            let n = buf.len().min(RAW_KEEP - self.raw.len());
            self.raw.extend_from_slice(&buf[..n]);
        }
        self.buf.extend_from_slice(buf);
        self.drain_events();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// 오류 글에 쓰려고 드는 본문 원문의 상한(바이트).
const RAW_KEEP: usize = 16 * 1024;

/// ★ HTTP 오류 글(순수 · 11-13): `HTTP <상태>: <서버 메시지>` + 요청 id(문의·대조용) + 상태별 안내 한 줄. 본문이 비면 그렇다고 적는다
/// (앞단 프록시·게이트웨이가 본문 없이 5xx를 내는 경우). 서버 메시지는 오류 JSON의 `error.message` · 아니면 본문 그대로(앞 400자).
pub(crate) fn http_error_text(
    status: u16,
    body: &str,
    request_id: Option<&str>,
    retry_after: Option<&str>,
) -> String {
    use nsql_i18n::{t, tf, Msg};
    let body = body.trim();
    let msg = json_parse(body)
        .ok()
        .and_then(|v| jget(&v, "error").map(|e| jstr(e, "message")))
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| body.chars().take(400).collect());
    let mut out = if msg.trim().is_empty() {
        format!("HTTP {status}: {}", t(Msg::AiHttpEmptyBody))
    } else {
        format!("HTTP {status}: {}", msg.trim())
    };
    if let Some(id) = request_id.filter(|s| !s.is_empty()) {
        out.push_str(&format!(" (request-id {id})"));
    }
    let hint = match status {
        429 => Some(tf(
            Msg::AiHttpRateLimited,
            &[retry_after.filter(|s| !s.is_empty()).unwrap_or("?")],
        )),
        500 | 502 | 503 | 504 | 529 => Some(t(Msg::AiHttpUnavailable).to_string()),
        401 | 403 => Some(t(Msg::AiHttpAuth).to_string()),
        _ => None,
    };
    if let Some(h) = hint {
        out.push('\n');
        out.push_str(&h);
    }
    out
}

/// 한 턴의 네트워크 호출 — 응답을 조립해 돌려준다(HTTP 오류는 본문을 오류로).
pub(crate) fn stream_once(
    base_url: &str,
    auth: &ApiAuth,
    workspace: &str,
    body: &Json,
    timeout_secs: u64,
    emit: &mut dyn FnMut(AiEvent),
    stop: Option<&std::sync::atomic::AtomicBool>,
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
    let mut sink = SseSink::new(emit).with_stop(stop);
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
            // 오류 본문은 SSE가 아니라 JSON 한 덩어리 — 싱크가 따로 든 원문에서 읽는다(사건 버퍼는 `finish`가 이미 비웠다).
            let text = String::from_utf8_lossy(&sink.raw).into_owned();
            Err(http_error_text(
                resp.status,
                &text,
                resp.header("request-id"),
                resp.header("retry-after"),
            ))
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

/// ★ 대화 이력 보정(사용자 11-14 "HTTP 400: messages.N.content.M.tool_use.input: Field required" — 그 세션의 모든 요청이 거절됨):
/// 응답이 도구 호출 블록의 끝(`content_block_stop`) 전에 끊기면(■ 강제 종료 · 출력 한도 · 연결 끊김) 입력 없는 `tool_use`가
/// 이력에 남고, 결과(`tool_result`)가 없는 `tool_use`도 같은 400을 만든다. 보내기 전에 고친다 —
/// 바로 다음 사용자 메시지에 결과가 **없는** 도구 호출 = 실행되지 않은 것 → 그 자리를 글 블록("중단됨")으로 바꾼다 ·
/// 결과는 있는데 입력만 없는 것 = 빈 입력 `{}`을 채운다. 돌려주는 값 = 고친 블록 수. 순수.
pub(crate) fn repair_history(messages: &mut [Json]) -> usize {
    let mut fixed = 0;
    for i in 0..messages.len() {
        if jstr(&messages[i], "role") != "assistant" {
            continue;
        }
        let answered: Vec<String> = messages
            .get(i + 1)
            .filter(|m| jstr(m, "role") == "user")
            .and_then(|m| jget(m, "content"))
            .map(|c| match c {
                Json::Arr(a) => a
                    .iter()
                    .filter(|b| jstr(b, "type") == "tool_result")
                    .map(|b| jstr(b, "tool_use_id"))
                    .collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default();
        let Json::Obj(items) = &mut messages[i] else {
            continue;
        };
        let Some((_, Json::Arr(blocks))) = items.iter_mut().find(|(k, _)| k == "content") else {
            continue;
        };
        for b in blocks.iter_mut() {
            if jstr(b, "type") != "tool_use" {
                continue;
            }
            if !answered.contains(&jstr(b, "id")) {
                let name = jstr(b, "name");
                *b = Json::Obj(vec![
                    ("type".into(), Json::Str("text".into())),
                    (
                        "text".into(),
                        Json::Str(format!(
                            "[tool call `{name}` was interrupted before it ran]"
                        )),
                    ),
                ]);
                fixed += 1;
            } else if jget(b, "input").is_none() {
                if let Json::Obj(f) = b {
                    f.push(("input".into(), Json::Obj(Vec::new())));
                    fixed += 1;
                }
            }
        }
    }
    fixed
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
    let stopped = || stop.load(std::sync::atomic::Ordering::Relaxed);
    for round in 0..64 {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = tx.send(AiEvent::Done {
                stop: "cancelled".into(),
                ms: started.elapsed().as_millis() as u64,
            });
            return;
        }
        // 끊긴 도구 호출이 이력에 남아 있으면 보내기 전에 고친다(없으면 0 · 이미 400이 난 세션도 다음 요청에서 낫는다).
        repair_history(messages);
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
        let asm = match stream_once(
            base_url,
            auth,
            workspace,
            &body,
            timeout_secs,
            &mut emit,
            Some(stop),
        ) {
            Ok(a) => a,
            // ■로 끊은 요청은 오류가 아니다.
            Err(_) if stopped() => {
                let _ = tx.send(AiEvent::Done {
                    stop: "cancelled".into(),
                    ms: started.elapsed().as_millis() as u64,
                });
                return;
            }
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
        // ★ 출력 한도에서 잘린 도구 호출(11-14 · 사용자 "AI가 프로시저를 고쳤다는데 반영이 없다"): 긴 본문(프로시저 전체)을 쓰다
        //   `max_tokens`에 닿으면 호출은 대본에 남지만 **실행되지 않는다**(입력 JSON이 끝나지 않았다). 종전에는 상태줄 한 줄만
        //   바뀌어 실행된 것처럼 보였다 → 실행하지 않았음을 오류 줄로 알린다.
        if asm.stop_reason == "max_tokens" && !uses.is_empty() {
            let names: Vec<&str> = uses.iter().map(|(_, n, _)| n.as_str()).collect();
            let _ = tx.send(AiEvent::Error(nsql_i18n::tf(
                nsql_i18n::Msg::AiErrMaxTokensTool,
                &[&names.join(", ")],
            )));
        }
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
        // ★ ■ 뒤에는 도구를 부르지 않는다(협업 V1 11-15 · 데이터 보호): 종전에는 멈춤을 라운드 시작에서만 봐, 멈춘 뒤에도 받던
        //   응답의 도구 호출(조회는 그대로 실행 · 쓰기는 승인 카드)이 UI로 갔다. 끊긴 호출은 다음 요청 전에 `repair_history`가 고친다.
        if stopped() {
            let _ = tx.send(AiEvent::Done {
                stop: "cancelled".into(),
                ms: started.elapsed().as_millis() as u64,
            });
            return;
        }
        let mut results: Vec<(String, ToolReply)> = Vec::new();
        for (id, name, input) in uses {
            if stopped() {
                let _ = tx.send(AiEvent::Done {
                    stop: "cancelled".into(),
                    ms: started.elapsed().as_millis() as u64,
                });
                return;
            }
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
        let mut tr = tool_result_message(&results);
        // ★ 진행 중 턴에 대기 요청 반영(사용자 11-15 "대기 풀의 요청이 진행 중에도 순차 반영되게"): 도구 결과를 돌려보내는 이
        //   user 메시지 **끝에**(API 규칙 = tool_result 블록이 먼저) 대기 중인 사용자 글을 덧붙인다 → 모델이 다음 단계부터 읽는다.
        //   UI가 답하지 않고 사건을 버리면(창 닫힘 등) 보낸 쪽이 사라져 바로 풀린다 — 시간 제한을 두지 않는 까닭 = 늦게 온 답을
        //   놓치면 UI는 반영했다고 알고 요청은 사라진다.
        if !stop.load(std::sync::atomic::Ordering::Relaxed) {
            let (stx, srx) = mpsc::channel();
            if tx.send(AiEvent::SteerRequest { reply: stx }).is_ok() {
                if let Ok(blocks) = srx.recv() {
                    append_content(&mut tr, blocks);
                }
            }
        }
        messages.push(tr);
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
    /// 진행 중 턴 반영(11-15): 덧붙인 사용자 글은 tool_result 블록 **뒤에** 오고(API 규칙) · 끊긴 호출 보정과 함께 쓸 수 있으며 ·
    /// 대화 재구성에서 연달아 온 user는 한 메시지로 합쳐진다.
    #[test]
    fn steer_blocks_follow_tool_results_and_users_merge() {
        let mut tr = tool_result_message(&[(
            "t1".to_string(),
            ToolReply {
                output: "ok".into(),
                ok: true,
            },
        )]);
        append_content(&mut tr, steer_blocks("also check indexes", &[]));
        append_content(&mut tr, Vec::new());
        let dump = json_dump(&tr);
        let (a, b) = (
            dump.find("tool_result").expect("tool_result"),
            dump.find("also check indexes").expect("steer text"),
        );
        assert!(a < b, "{dump}");
        assert_eq!(dump.matches("\"type\"").count(), 2, "{dump}");
        let mut msgs: Vec<Json> = Vec::new();
        push_merged(&mut msgs, user_message("first", &[]));
        push_merged(&mut msgs, user_message("second", &[]));
        assert_eq!(msgs.len(), 1);
        let d = json_dump(&msgs[0]);
        assert!(
            d.find("first").expect("first") < d.find("second").expect("second"),
            "{d}"
        );
        push_merged(
            &mut msgs,
            Json::Obj(vec![
                ("role".into(), Json::Str("assistant".into())),
                ("content".into(), Json::Arr(Vec::new())),
            ]),
        );
        push_merged(&mut msgs, user_message("third", &[]));
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn interrupted_tool_calls_are_repaired_before_sending() {
        use super::*;
        let parse = |s: &str| json_parse(s).expect("test json");
        let mut msgs = vec![
            parse(r#"{"role":"user","content":[{"type":"text","text":"q"}]}"#),
            // 결과는 있는데 입력이 없는 호출(t1) + 정상 호출(t2).
            parse(
                r#"{"role":"assistant","content":[{"type":"text","text":"a"},{"type":"tool_use","id":"t1","name":"search_files"},{"type":"tool_use","id":"t2","name":"get_setting","input":{"key":"x"}}]}"#,
            ),
            parse(
                r#"{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"r1"},{"type":"tool_result","tool_use_id":"t2","content":"r2"}]}"#,
            ),
            // 끊긴 호출(t3 · 입력도 결과도 없음) 뒤에 사용자의 새 글.
            parse(
                r#"{"role":"assistant","content":[{"type":"tool_use","id":"t3","name":"object_source"}]}"#,
            ),
            parse(r#"{"role":"user","content":[{"type":"text","text":"again"}]}"#),
            // 마지막이 끊긴 호출(다음 메시지 없음).
            parse(
                r#"{"role":"assistant","content":[{"type":"tool_use","id":"t4","name":"run_query","input":{"sql":"select 1"}}]}"#,
            ),
        ];
        assert_eq!(repair_history(&mut msgs), 3);
        let dump = |m: &Json| json_dump(m);
        let a1 = dump(&msgs[1]);
        assert!(
            a1.contains(r#""id":"t1""#) && a1.contains(r#""input":{}"#),
            "{a1}"
        );
        assert!(a1.contains(r#""input":{"key":"x"}"#), "{a1}");
        let a3 = dump(&msgs[3]);
        assert!(
            !a3.contains("tool_use") && a3.contains("object_source"),
            "{a3}"
        );
        let a5 = dump(&msgs[5]);
        assert!(
            !a5.contains("tool_use") && a5.contains("interrupted"),
            "{a5}"
        );
        // 고칠 것이 없으면 그대로.
        let before: Vec<String> = msgs.iter().map(dump).collect();
        assert_eq!(repair_history(&mut msgs), 0);
        assert_eq!(before, msgs.iter().map(dump).collect::<Vec<_>>());
    }

    /// ■ 깃발이 서면 싱크가 다음 조각에서 받기를 끊는다(11-15) — 서기 전 조각은 그대로 처리.
    #[test]
    fn sse_sink_stops_reading_once_the_stop_flag_is_set() {
        use super::*;
        use std::io::Write;
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut seen = 0usize;
        let mut emit = |_: AiEvent| seen += 1;
        let mut sink = SseSink::new(&mut emit).with_stop(Some(&stop));
        sink.write_all(b"data: {\"type\":\"ping\"}\n\n")
            .expect("before stop");
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(sink.write_all(b"data: {\"type\":\"ping\"}\n\n").is_err());
    }

    #[test]
    fn http_error_body_survives_the_sse_sink_and_reads_well() {
        use super::*;
        use std::io::Write;
        // 오류 본문(JSON 한 덩어리 · SSE 아님)을 받아도 원문이 남는다 — 종전에는 `finish`가 버려 "HTTP 503: "만 보였다.
        let mut emit = |_: AiEvent| {};
        let mut sink = SseSink::new(&mut emit);
        let body =
            br#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        sink.write_all(body).expect("write to the sink");
        sink.finish();
        let text = String::from_utf8_lossy(&sink.raw).into_owned();
        let e = http_error_text(503, &text, Some("req_123"), None);
        assert!(
            e.starts_with("HTTP 503: Overloaded (request-id req_123)"),
            "{e}"
        );
        assert!(e.lines().count() == 2, "상태별 안내 한 줄: {e}");
        // 본문이 비면 그렇다고 적는다(프록시·게이트웨이).
        let e = http_error_text(503, "  ", None, None);
        assert!(!e.trim_end().ends_with(':'), "{e}");
        // JSON이 아닌 본문은 그대로(앞부분만) · 안내 없는 상태는 한 줄.
        let e = http_error_text(418, "<html>teapot</html>", None, None);
        assert_eq!(e, "HTTP 418: <html>teapot</html>");
        // 429 = retry-after를 안내에 싣는다.
        let e = http_error_text(
            429,
            r#"{"error":{"message":"rate limited"}}"#,
            None,
            Some("12"),
        );
        assert!(
            e.starts_with("HTTP 429: rate limited") && e.contains("12"),
            "{e}"
        );
    }

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
