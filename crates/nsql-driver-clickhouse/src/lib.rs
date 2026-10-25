//! ClickHouse 어댑터(10-24 · 138차 · 사용자 "주요 DB 드라이버 개발" · [121 P5] · D-354) — [`nsql_core::Session`] 구현 · 방언 [`Dialect::Clickhouse`].
//!
//! - 경로 = **HTTP 인터페이스**(8123 · TLS = 8443 · `?tls=required`) · 전송 = `nsql-net`(rustls · 외부 crate 0 · DR-3 예외 없음) ·
//!   응답 = `JSONCompact`(`nsql-json`) — 네이티브 TCP 프로토콜 crate를 들이지 않는다(의존 0 지향 · 압축·블록 스트리밍은 후속 후보).
//! - 세션 = `session_id`(`SET` 설정 유지 · `session_timeout` 1800 s) · **현재 DB는 클라이언트가 기억**(`USE x`를 가로채 `database=` 매개변수로 ·
//!   HTTP 세션 만료와 무관 · `set_option("schema")`도 같은 자리) · 소켓은 요청마다 새로(`Connection: close`).
//! - 트랜잭션 문장(`COMMIT` · `ROLLBACK` · `BEGIN` · `SAVEPOINT`)은 보내지 않고 no-op(24.8 실서버 = 649 INVALID_TRANSACTION · CI 10-24).
//! - 바인드 = `?` 자리에 **리터럴 치환**(DR-8 · ClickHouse HTTP는 `{name:Type}` 서버 매개변수뿐이라 위치 바인드가 없다 · 문자열은
//!   백슬래시·따옴표 이스케이프 · 이진은 `unhex('…')`) · 트랜잭션 없음(문장마다 확정 = Caps `server_autocommit` · COMMIT/ROLLBACK = no-op).
//! - 취소 = `KILL QUERY WHERE query_id = '…' ASYNC`(다른 접속 · 44 §6) · 실행 중 `query_id`는 [`Session::cancel_handle`]과 공유.
//! - 결과 = 전부 읽어 `ResultSet`(상한 `max_rows` = `max_result_rows` + `result_overflow_mode=break` + 클라이언트 절단 · 서버 커서 없음) ·
//!   값 = 메타 타입으로 복원(정수 → `Int` · 실수 → `Float` · Decimal → `Decimal` · Bool · 그 밖(날짜·UUID·배열·튜플·맵) → `Str` ·
//!   64비트 정수·Decimal은 따옴표 출력(`output_format_json_quote_*`)으로 정밀도 보존).
//! - DML 영향 행 수 = 응답 헤더 `X-ClickHouse-Summary`의 `written_rows` · 오류 = HTTP 4xx/5xx 본문 `Code: N. DB::Exception: …`(코드 → `DbError.code` ·
//!   `failed at position N` → `position`) · 카탈로그 훅 = `system.tables`/`system.columns`(스키마 = 데이터베이스).
#![cfg_attr(test, allow(clippy::unwrap_used))]

use nsql_core::{
    BindParam, CatalogColumn, CatalogTable, Column, CursorId, DbError, Dialect, ExecRequest,
    ExecResult, ResultSet, Session, Value,
};
use nsql_json::Json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 접속 하나 = HTTP 끝점 + 자격 + 세션 id + 현재 DB. 소켓은 요청마다 새로.
#[derive(Debug)]
pub struct ChSession {
    ep: Endpoint,
    database: String,
    session_id: String,
    max_rows: usize,
    description: String,
    /// 실행 중인 `query_id`(취소 핸들과 공유 · 실행 밖에서는 `None`).
    active: Arc<Mutex<Option<String>>>,
    seq: u64,
}

/// 끝점 + 자격 + 상한(취소 핸들이 복제해 쓴다).
#[derive(Clone, Debug)]
struct Endpoint {
    /// `http://host:8123` — 꼬리 `/` 없음.
    base: String,
    user: String,
    password: String,
    /// 접속 상한.
    connect_timeout: Duration,
    /// 읽기·쓰기 상한(긴 질의를 기다린다 · 취소는 KILL QUERY로).
    io_timeout: Duration,
}

/// HTTP 응답(상태 · 소문자 헤더 · 본문).
type Reply = (u16, Vec<(String, String)>, Vec<u8>);

fn err(msg: impl Into<String>) -> DbError {
    DbError {
        code: None,
        message: msg.into(),
        position: None,
    }
}

/// 쿼리 문자열 값의 퍼센트 인코딩(RFC 3986 unreserved만 그대로).
fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// ClickHouse 문자열 리터럴(`'…'` · 백슬래시와 따옴표 이스케이프 · 제어 문자는 `\n` `\t` `\0`).
fn str_lit(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// 바인드 값 → ClickHouse 리터럴(`?` 자리 치환용).
fn literal(v: &Value) -> String {
    match v {
        Value::Null | Value::Cursor(_) => "NULL".into(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            if f.is_nan() {
                "nan".into()
            } else if f.is_infinite() {
                if *f > 0.0 { "inf" } else { "-inf" }.into()
            } else {
                format!("{f:?}")
            }
        }
        Value::Decimal(d) => d.clone(),
        Value::Bool(b) => if *b { "true" } else { "false" }.into(),
        Value::Str(s) => str_lit(s),
        Value::Bytes(b) => {
            let hex: String = b.iter().map(|x| format!("{x:02X}")).collect();
            format!("unhex('{hex}')")
        }
    }
}

/// ★ T-417(123 §4 · 사용자 10-26): **서버 측 매개변수** — 조회 문장의 `?`를 `{pN:Type}`로 바꾸고 값은 URL `param_pN=`(TSV escaped 꼴)로 보낸다
/// (ClickHouse HTTP · 주입 차단 · 타입은 서버가 검사). 조회가 아니거나(INSERT·DDL = 공식 보장 없음) 이진 값이 있으면 `None` → 리터럴 치환 폴백.
/// `NSQL_CH_INLINE=1` = 진단용 강제 폴백.
fn server_params(sql: &str, params: &[BindParam]) -> Option<(String, Vec<(String, String)>)> {
    if params.is_empty() || !sql.contains('?') || !is_query(sql) {
        return None;
    }
    if std::env::var_os("NSQL_CH_INLINE").is_some() {
        return None;
    }
    if params
        .iter()
        .any(|p| matches!(p.value, Value::Bytes(_) | Value::Cursor(_)))
    {
        return None;
    }
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(params.len());
    let mut k = 0usize;
    let rewritten = replace_placeholders(sql, &mut |_| {
        let out = match params.get(k) {
            Some(p) => {
                let (ty, val) = ch_param(p);
                pairs.push((format!("param_p{k}"), val));
                format!("{{p{k}:{ty}}}")
            }
            None => "NULL".to_string(),
        };
        k += 1;
        out
    });
    Some((rewritten, pairs))
}

/// 바인드 값 → (ClickHouse 타입 · TSV escaped 값). NULL = `Nullable(String)` + `\N` · 정수 Int64 · 실수 Float64 · 소수 Decimal(38,s) ·
/// 불리언 Bool · 글자 = 변수 타입이 DATE/TIMESTAMP면 Date32/DateTime64(6) 아니면 String.
fn ch_param(p: &BindParam) -> (String, String) {
    match &p.value {
        Value::Null | Value::Cursor(_) => ("Nullable(String)".into(), "\\N".into()),
        Value::Int(i) => ("Int64".into(), i.to_string()),
        Value::Float(f) => ("Float64".into(), format!("{f:?}")),
        Value::Decimal(d) => {
            let scale = d.split_once('.').map_or(0, |(_, frac)| frac.len()).min(38);
            (format!("Decimal(38,{scale})"), d.clone())
        }
        Value::Bool(b) => ("Bool".into(), if *b { "true" } else { "false" }.into()),
        Value::Str(s) => {
            let ty = match p.ty {
                nsql_core::VarType::Date => "Date32",
                nsql_core::VarType::Timestamp => "DateTime64(6)",
                _ => "String",
            };
            (ty.into(), tsv_escape(s))
        }
        Value::Bytes(b) => {
            let hex: String = b.iter().map(|x| format!("{x:02X}")).collect();
            ("String".into(), hex)
        }
    }
}

/// ClickHouse "escaped"(TSV) 꼴 — `\` · 탭 · 줄바꿈 · CR · NUL(퍼센트 인코딩은 URL 조립이 한다).
fn tsv_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out
}

/// `?` 자리(문자열·식별자 따옴표·주석 밖)마다 `repl(k)`로 바꾼다 — 리터럴 치환·서버 매개변수 공용 스캐너.
fn replace_placeholders(sql: &str, repl: &mut dyn FnMut(usize) -> String) -> String {
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len() + 32);
    let (mut i, mut pi) = (0usize, 0usize);
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' | '"' | '`' => {
                out.push(c);
                i += 1;
                while i < chars.len() {
                    let d = chars[i];
                    out.push(d);
                    i += 1;
                    if d == '\\' && i < chars.len() {
                        out.push(chars[i]);
                        i += 1;
                        continue;
                    }
                    if d == c {
                        if i < chars.len() && chars[i] == c {
                            out.push(c);
                            i += 1;
                            continue;
                        }
                        break;
                    }
                }
            }
            '-' if chars.get(i + 1) == Some(&'-') => {
                while i < chars.len() && chars[i] != '\n' {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                out.push_str("/*");
                i += 2;
                while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                    out.push(chars[i]);
                    i += 1;
                }
                if i < chars.len() {
                    out.push_str("*/");
                    i += 2;
                }
            }
            '?' => {
                out.push_str(&repl(pi));
                pi += 1;
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// `?` 자리를 리터럴로 치환(문자열·식별자 따옴표·주석 안은 건너뛴다 · 모자라는 매개변수는 NULL).
fn inline_params(sql: &str, params: &[BindParam]) -> String {
    if params.is_empty() || !sql.contains('?') {
        return sql.to_string();
    }
    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len() + 32);
    let (mut i, mut pi) = (0usize, 0usize);
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' | '"' | '`' => {
                out.push(c);
                i += 1;
                while i < chars.len() {
                    let d = chars[i];
                    out.push(d);
                    i += 1;
                    if d == '\\' && i < chars.len() {
                        out.push(chars[i]);
                        i += 1;
                        continue;
                    }
                    if d == c {
                        if i < chars.len() && chars[i] == c {
                            out.push(c);
                            i += 1;
                            continue;
                        }
                        break;
                    }
                }
            }
            '-' if chars.get(i + 1) == Some(&'-') => {
                while i < chars.len() && chars[i] != '\n' {
                    out.push(chars[i]);
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                out.push_str("/*");
                i += 2;
                while i < chars.len() {
                    if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                        out.push_str("*/");
                        i += 2;
                        break;
                    }
                    out.push(chars[i]);
                    i += 1;
                }
            }
            '?' => {
                let lit = params
                    .get(pi)
                    .map_or_else(|| "NULL".to_string(), |p| literal(&p.value));
                out.push_str(&lit);
                pi += 1;
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// 결과를 돌려주는 문장인가(머리 낱말).
fn is_query(sql: &str) -> bool {
    let head = sql
        .trim_start()
        .trim_start_matches('(')
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        head.as_str(),
        "SELECT" | "WITH" | "SHOW" | "DESCRIBE" | "DESC" | "EXPLAIN" | "EXISTS" | "CHECK"
    )
}

/// 단독 트랜잭션 문장인가(CI 10-24 · ClickHouse 24.8 = `COMMIT`에 649 INVALID_TRANSACTION): 트랜잭션이 없는 서버라 스크립트의
/// `COMMIT` · `ROLLBACK [TO …]` · `BEGIN [TRANSACTION]` · `START TRANSACTION` · `SAVEPOINT x` · `RELEASE SAVEPOINT x`는 보내지 않고 no-op.
fn is_tx_statement(sql: &str) -> bool {
    let mut it = sql.split_whitespace();
    let Some(first) = it.next() else {
        return false;
    };
    let second = it.next().unwrap_or("");
    match first.to_ascii_uppercase().as_str() {
        "COMMIT" | "ROLLBACK" | "SAVEPOINT" => true,
        "BEGIN" => second.is_empty() || second.eq_ignore_ascii_case("TRANSACTION"),
        "START" => second.eq_ignore_ascii_case("TRANSACTION"),
        "RELEASE" => second.eq_ignore_ascii_case("SAVEPOINT"),
        _ => false,
    }
}

/// `USE db` → 데이터베이스 이름(클라이언트가 기억 · 서버에 보내지 않는다).
fn parse_use(sql: &str) -> Option<String> {
    let mut it = sql.split_whitespace();
    let first = it.next()?;
    if !first.eq_ignore_ascii_case("USE") {
        return None;
    }
    let name = it.next()?;
    if it.next().is_some() {
        return None;
    }
    let name = name.trim_matches(['`', '"']);
    (!name.is_empty()).then(|| name.to_string())
}

/// 꼬리의 `FORMAT <이름>`을 뗀다(응답은 늘 JSONCompact여야 격자로 읽는다) — 뗐으면 true.
fn strip_format(sql: &str) -> (String, bool) {
    let t = sql.trim_end().trim_end_matches(';').trim_end();
    let words: Vec<&str> = t.split_whitespace().collect();
    if words.len() >= 3 && words[words.len() - 2].eq_ignore_ascii_case("FORMAT") {
        let last = words[words.len() - 1];
        if last.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            if let Some(pos) = t.to_ascii_uppercase().rfind("FORMAT") {
                return (t[..pos].trim_end().to_string(), true);
            }
        }
    }
    (t.to_string(), false)
}

/// 타입 이름의 포장(`Nullable(…)` · `LowCardinality(…)`)을 벗긴 바탕 타입.
fn base_type(ty: &str) -> &str {
    let mut t = ty.trim();
    loop {
        let inner = t
            .strip_prefix("Nullable(")
            .or_else(|| t.strip_prefix("LowCardinality("));
        match inner {
            Some(r) => t = r.strip_suffix(')').unwrap_or(r).trim(),
            None => return t,
        }
    }
}

/// JSON 값 + 메타 타입 → [`Value`].
fn to_value(j: &Json, ty: &str) -> Value {
    if matches!(j, Json::Null) {
        return Value::Null;
    }
    let b = base_type(ty);
    if b.starts_with("Int") || b.starts_with("UInt") {
        return match j {
            Json::Num(n) => Value::Int(*n as i64),
            Json::Str(s) => s
                .parse::<i64>()
                .map_or_else(|_| Value::Decimal(s.clone()), Value::Int),
            Json::Bool(x) => Value::Int(i64::from(*x)),
            other => Value::Str(nsql_json::dump(other)),
        };
    }
    if b.starts_with("Float") {
        return match j {
            Json::Num(n) => Value::Float(*n),
            Json::Str(s) => s
                .parse::<f64>()
                .map_or_else(|_| Value::Str(s.clone()), Value::Float),
            other => Value::Str(nsql_json::dump(other)),
        };
    }
    if b.starts_with("Decimal") {
        return match j {
            Json::Str(s) => Value::Decimal(s.clone()),
            Json::Num(n) => Value::Decimal(n.to_string()),
            other => Value::Str(nsql_json::dump(other)),
        };
    }
    if b == "Bool" {
        return match j {
            Json::Bool(x) => Value::Bool(*x),
            Json::Num(n) => Value::Bool(*n != 0.0),
            Json::Str(s) => Value::Bool(s == "true" || s == "1"),
            other => Value::Str(nsql_json::dump(other)),
        };
    }
    match j {
        Json::Str(s) => Value::Str(s.clone()),
        Json::Num(n) => Value::Str(Json::Num(*n).scalar_text().unwrap_or_default()),
        Json::Bool(x) => Value::Str(if *x { "true" } else { "false" }.into()),
        other => Value::Str(nsql_json::dump(other)),
    }
}

fn field<'a>(j: &'a Json, key: &str) -> Option<&'a Json> {
    match j {
        Json::Obj(v) => v.iter().find(|(k, _)| k == key).map(|(_, x)| x),
        _ => None,
    }
}

fn arr(j: &Json) -> &[Json] {
    match j {
        Json::Arr(v) => v,
        _ => &[],
    }
}

fn text(j: Option<&Json>) -> String {
    match j {
        Some(Json::Str(s)) => s.clone(),
        Some(other) => other.scalar_text().unwrap_or_default(),
        None => String::new(),
    }
}

/// `JSONCompact` 본문 → 결과 집합(상한 `max_rows` · 0 = 전부).
fn parse_compact(body: &str, max_rows: usize) -> Result<ResultSet, DbError> {
    let j = nsql_json::parse(body).map_err(|e| {
        err(format!(
            "ClickHouse: unexpected response (not JSONCompact): {e}"
        ))
    })?;
    let meta = field(&j, "meta").map(arr).unwrap_or(&[]);
    let types: Vec<String> = meta.iter().map(|m| text(field(m, "type"))).collect();
    let columns: Vec<Column> = meta
        .iter()
        .zip(&types)
        .map(|(m, ty)| Column {
            name: text(field(m, "name")),
            type_name: ty.clone(),
        })
        .collect();
    let mut rows = Vec::new();
    for row in field(&j, "data").map(arr).unwrap_or(&[]) {
        if max_rows > 0 && rows.len() >= max_rows {
            break;
        }
        let cells = arr(row);
        let vals: Vec<Value> = (0..columns.len())
            .map(|i| cells.get(i).map_or(Value::Null, |c| to_value(c, &types[i])))
            .collect();
        rows.push(vals);
    }
    Ok(ResultSet { columns, rows })
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// `X-ClickHouse-Summary: {"read_rows":"0",…,"written_rows":"3",…}` → written_rows.
fn written_rows(headers: &[(String, String)]) -> Option<u64> {
    let raw = header(headers, "x-clickhouse-summary")?;
    let j = nsql_json::parse(raw).ok()?;
    text(field(&j, "written_rows")).parse().ok()
}

/// 오류 응답 → [`DbError`](코드 = 헤더 `X-ClickHouse-Exception-Code` 또는 본문 `Code: N.` · 꼬리 `(version …)` 제거 ·
/// `failed at position N`(1부터 · 바이트) → 보낸 문장 기준 글자 인덱스).
fn error_from(status: u16, headers: &[(String, String)], body: &[u8], sql: &str) -> DbError {
    let raw = String::from_utf8_lossy(body).trim().to_string();
    let code = header(headers, "x-clickhouse-exception-code")
        .and_then(|v| v.trim().parse::<i64>().ok())
        .or_else(|| {
            raw.strip_prefix("Code: ")
                .and_then(|r| r.split('.').next())
                .and_then(|n| n.trim().parse().ok())
        });
    let mut msg = raw.clone();
    if let Some(i) = msg.find(". ") {
        if msg.starts_with("Code: ") {
            msg = msg[i + 2..].to_string();
        }
    }
    if let Some(r) = msg.strip_prefix("DB::Exception: ") {
        msg = r.to_string();
    }
    if let Some(i) = msg.rfind(" (version ") {
        msg.truncate(i);
    }
    if msg.is_empty() {
        msg = format!("HTTP {status}");
    }
    let position = raw.find("failed at position ").and_then(|i| {
        let n: usize = raw[i + "failed at position ".len()..]
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()?;
        let byte = n.checked_sub(1)?;
        Some(sql.get(..byte).map_or(0, |s| s.chars().count()))
    });
    DbError {
        code,
        message: msg,
        position,
    }
}

impl Endpoint {
    fn options(&self) -> nsql_net::Options {
        nsql_net::Options {
            timeout: self.connect_timeout,
            io_timeout: Some(self.io_timeout),
            headers: vec![
                ("X-ClickHouse-User".into(), self.user.clone()),
                ("X-ClickHouse-Key".into(), self.password.clone()),
                ("Content-Type".into(), "text/plain; charset=utf-8".into()),
            ],
            // DB 접속에 HTTPS_PROXY 환경 변수를 적용하지 않는다(다운로드와 다른 트래픽 · 26 §8).
            proxy: Some(String::new()),
            max_redirects: 0,
            max_body: 1 << 40,
            user_agent: format!("nsql-clickhouse/{}", env!("CARGO_PKG_VERSION")),
        }
    }

    fn url(&self, params: &[(&str, &str)]) -> String {
        let q: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{k}={}", pct(v)))
            .collect();
        format!("{}/?{}", self.base, q.join("&"))
    }

    /// POST 한 번(본문 = SQL). 네트워크 오류 = `Connection error: …`(분류 = Connection).
    fn post(&self, params: &[(&str, &str)], body: &str) -> Result<Reply, DbError> {
        let (resp, out) = nsql_net::post_bytes(&self.url(params), body.as_bytes(), &self.options())
            .map_err(|e| err(format!("Connection error: {e}")))?;
        Ok((resp.status, resp.headers, out))
    }
}

/// 실행 취소 — `KILL QUERY … ASYNC`를 다른 접속으로.
struct Cancel {
    ep: Endpoint,
    active: Arc<Mutex<Option<String>>>,
}

impl nsql_core::CancelHandle for Cancel {
    fn cancel(&self) -> Result<(), DbError> {
        let qid = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let Some(qid) = qid else {
            return Ok(());
        };
        let sql = format!("KILL QUERY WHERE query_id = {} ASYNC", str_lit(&qid));
        let (status, headers, body) = self.ep.post(&[("default_format", "JSONCompact")], &sql)?;
        if status == 200 {
            Ok(())
        } else {
            Err(error_from(status, &headers, &body, &sql))
        }
    }
}

impl ChSession {
    /// 접속 문자열 → 세션(`clickhouse://user:password@host:8123/db?tls=required`). 호스트 필수 · 사용자 기본 `default` · DB 기본 `default`.
    pub fn open(spec: &nsql_script::ConnectSpec) -> Result<ChSession, DbError> {
        let host = spec
            .host
            .as_deref()
            .map(str::trim)
            .filter(|h| !h.is_empty())
            .ok_or_else(|| {
                err("ClickHouse: host is required (clickhouse://user:password@host:8123/db)")
            })?;
        let tls = spec.tls.as_deref().is_some_and(|t| {
            matches!(
                t,
                "required" | "verify_ca" | "verify_identity" | "on" | "true" | "1" | "yes"
            )
        });
        let port = spec.port.unwrap_or(if tls { 8443 } else { 8123 });
        let base = format!("{}://{host}:{port}", if tls { "https" } else { "http" });
        let database = spec
            .schema
            .clone()
            .or_else(|| spec.database.clone())
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| "default".into());
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let mut s = ChSession {
            ep: Endpoint {
                base,
                user: spec
                    .user
                    .clone()
                    .filter(|u| !u.is_empty())
                    .unwrap_or_else(|| "default".into()),
                password: spec.password.clone().unwrap_or_default(),
                connect_timeout: Duration::from_secs(15),
                io_timeout: Duration::from_secs(24 * 3600),
            },
            database,
            session_id: format!("nsql{}-{nanos:x}", std::process::id()),
            max_rows: 0,
            description: String::new(),
            active: Arc::new(Mutex::new(None)),
            seq: 0,
        };
        let rs = s.query("SELECT version(), currentDatabase()")?;
        let version = rs
            .rows
            .first()
            .and_then(|r| r.first())
            .map(Value::display)
            .unwrap_or_default();
        s.description = format!("ClickHouse {version} · {host}:{port}/{}", s.database);
        Ok(s)
    }

    fn next_query_id(&mut self) -> String {
        self.seq += 1;
        format!("{}-{}", self.session_id, self.seq)
    }

    /// 세션 매개변수(현재 DB · 세션 id · JSON 출력 규칙).
    fn session_params<'a>(&'a self, extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
        let mut p = vec![
            ("database", self.database.as_str()),
            ("session_id", self.session_id.as_str()),
            ("session_timeout", "1800"),
            ("output_format_json_quote_64bit_integers", "1"),
            ("output_format_json_quote_decimals", "1"),
        ];
        p.extend_from_slice(extra);
        p
    }

    /// 조회 한 번(카탈로그·접속 확인용 · 상한 없음).
    fn query(&mut self, sql: &str) -> Result<ResultSet, DbError> {
        let qid = self.next_query_id();
        let params = self.session_params(&[("default_format", "JSONCompact"), ("query_id", &qid)]);
        let (status, headers, body) = self.ep.post(&params, sql)?;
        if status != 200 {
            return Err(error_from(status, &headers, &body, sql));
        }
        parse_compact(&String::from_utf8_lossy(&body), 0)
    }

    fn run(
        &mut self,
        sql: &str,
        qid: &str,
        server_params: &[(String, String)],
    ) -> Result<ExecResult, DbError> {
        let (sql, _stripped) = strip_format(sql);
        let query = is_query(&sql);
        let max = self.max_rows.to_string();
        let mut extra: Vec<(&str, &str)> = vec![("query_id", qid)];
        // T-417 서버 측 매개변수(`param_pN=값` · 값은 URL 조립이 퍼센트 인코딩).
        extra.extend(server_params.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        if query {
            extra.push(("default_format", "JSONCompact"));
            if self.max_rows > 0 {
                extra.push(("max_result_rows", max.as_str()));
                extra.push(("result_overflow_mode", "break"));
            }
        }
        let params = self.session_params(&extra);
        let (status, headers, body) = self.ep.post(&params, &sql)?;
        if status != 200 {
            return Err(error_from(status, &headers, &body, &sql));
        }
        if !query {
            return Ok(ExecResult {
                rows_affected: written_rows(&headers),
                ..ExecResult::default()
            });
        }
        let text = String::from_utf8_lossy(&body);
        let rs = if text.trim().is_empty() {
            ResultSet::default()
        } else {
            parse_compact(&text, self.max_rows)?
        };
        Ok(ExecResult {
            result_sets: vec![rs],
            ..ExecResult::default()
        })
    }
}

impl Session for ChSession {
    fn dialect(&self) -> Dialect {
        Dialect::Clickhouse
    }

    fn driver_name(&self) -> String {
        "ClickHouse (HTTP)".to_string()
    }

    fn cancel_handle(&self) -> Option<Arc<dyn nsql_core::CancelHandle>> {
        Some(Arc::new(Cancel {
            ep: self.ep.clone(),
            active: Arc::clone(&self.active),
        }))
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        // T-417: 조회 = 서버 매개변수(`{pN:Type}` + `param_pN=`) · 그 밖/이진 = 리터럴 치환(종전).
        let (sql, sp) = match server_params(&req.sql, &req.params) {
            Some((s, p)) => (s, p),
            None => (inline_params(&req.sql, &req.params), Vec::new()),
        };
        let trimmed = sql.trim().trim_end_matches(';').trim().to_string();
        if let Some(db) = parse_use(&trimmed) {
            self.database = db;
            return Ok(ExecResult::default());
        }
        if is_tx_statement(&trimmed) {
            return Ok(ExecResult::default());
        }
        let qid = self.next_query_id();
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(qid.clone());
        let r = self.run(&trimmed, &qid, &sp);
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        r
    }

    fn fetch_cursor(&mut self, _cursor: CursorId) -> Result<ResultSet, DbError> {
        Err(err("ClickHouse has no server cursors"))
    }

    fn set_option(&mut self, name: &str, value: &str) -> Result<(), DbError> {
        match name {
            "max_rows" => self.max_rows = value.parse().unwrap_or(0),
            // 스키마 = 데이터베이스(`USE`와 같은 자리).
            "schema" if !value.trim().is_empty() => self.database = value.trim().to_string(),
            _ => {}
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<(), DbError> {
        Ok(())
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        Ok(())
    }

    fn is_alive(&self) -> bool {
        let mut o = self.ep.options();
        o.io_timeout = Some(Duration::from_secs(10));
        nsql_net::get_bytes(&format!("{}/ping", self.ep.base), &o)
            .is_ok_and(|(r, _)| r.status == 200)
    }

    fn catalog_tables(
        &mut self,
        schema: Option<&str>,
    ) -> Result<Option<Vec<CatalogTable>>, DbError> {
        let wh = match schema.map(str::trim).filter(|s| !s.is_empty()) {
            Some(s) => format!(" WHERE database = {}", str_lit(s)),
            None => String::new(),
        };
        let rs = self.query(&format!(
            "SELECT database, name, engine FROM system.tables{wh} ORDER BY database, name"
        ))?;
        Ok(Some(
            rs.rows
                .iter()
                .map(|r| {
                    let engine = r.get(2).map(Value::display).unwrap_or_default();
                    CatalogTable {
                        catalog: String::new(),
                        schema: r.first().map(Value::display).unwrap_or_default(),
                        name: r.get(1).map(Value::display).unwrap_or_default(),
                        kind: if engine.contains("View") {
                            "VIEW".into()
                        } else {
                            "TABLE".into()
                        },
                    }
                })
                .collect(),
        ))
    }

    fn catalog_columns(
        &mut self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<Option<Vec<CatalogColumn>>, DbError> {
        let db = schema
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map_or_else(|| self.database.clone(), str::to_string);
        let rs = self.query(&format!(
            "SELECT name, type, position, default_expression FROM system.columns WHERE database = {} AND table = {} ORDER BY position",
            str_lit(&db),
            str_lit(table)
        ))?;
        Ok(Some(
            rs.rows
                .iter()
                .map(|r| {
                    let ty = r.get(1).map(Value::display).unwrap_or_default();
                    let base = base_type(&ty);
                    let (size, digits) = type_size(base);
                    CatalogColumn {
                        name: r.first().map(Value::display).unwrap_or_default(),
                        type_name: ty.clone(),
                        size,
                        decimal_digits: digits,
                        nullable: Some(ty.starts_with("Nullable(")),
                        position: r.get(2).and_then(|v| v.display().parse().ok()).unwrap_or(0),
                        default: r.get(3).map(Value::display).unwrap_or_default(),
                    }
                })
                .collect(),
        ))
    }
}

/// `FixedString(N)` → (N, None) · `Decimal(P, S)`/`Decimal64(S)` → (P, S) · 그 밖 = (None, None).
fn type_size(base: &str) -> (Option<i64>, Option<i64>) {
    let inner = |t: &str| -> Vec<i64> {
        t.find('(')
            .map(|i| {
                t[i + 1..]
                    .trim_end_matches(')')
                    .split(',')
                    .filter_map(|x| x.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    };
    if base.starts_with("FixedString(") {
        return (inner(base).first().copied(), None);
    }
    if let Some(rest) = base.strip_prefix("Decimal") {
        let nums = inner(base);
        let prec = match rest
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .as_str()
        {
            "32" => Some(9),
            "64" => Some(18),
            "128" => Some(38),
            "256" => Some(76),
            _ => nums.first().copied(),
        };
        let scale = if rest.starts_with('(') {
            nums.get(1).copied()
        } else {
            nums.first().copied()
        };
        return (prec, scale);
    }
    (None, None)
}

// ★ 확장 드라이버 내보내기(D-350 · ABI v2): cdylib → `nsql_driver_vtable_clickhouse`.
fn nsql_abi_open_from_spec(spec: &nsql_script::ConnectSpec) -> Result<ChSession, DbError> {
    ChSession::open(spec)
}
nsql_abi::export_driver!("clickhouse", "clickhouse", nsql_abi_open_from_spec);

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{Direction, VarType};

    fn bp(name: &str, v: Value, ty: VarType) -> BindParam {
        BindParam {
            name: name.into(),
            value: v,
            ty,
            direction: Direction::In,
        }
    }

    /// T-417: 조회 = `{pN:Type}` + `param_pN`(TSV escaped) · 타입 매핑 · NULL · 비조회/이진 = None(리터럴 폴백).
    #[test]
    fn server_params_rewrite_types_and_fallback() {
        let ps = vec![
            bp("A", Value::Int(7), VarType::Number),
            bp("B", Value::Str("x\ty'z".into()), VarType::Auto),
            bp("C", Value::Null, VarType::Auto),
            bp("D", Value::Decimal("12.345".into()), VarType::Number),
            bp("E", Value::Str("2026-10-26".into()), VarType::Date),
            bp("F", Value::Bool(true), VarType::Boolean),
        ];
        let (sql, pairs) = server_params(
            "SELECT ? AS a, ? AS b, ? AS c, ? AS d, ? AS e, ? AS f FROM t WHERE s = '?' -- ?",
            &ps,
        )
        .expect("query → server params");
        assert_eq!(
            sql,
            "SELECT {p0:Int64} AS a, {p1:String} AS b, {p2:Nullable(String)} AS c, {p3:Decimal(38,3)} AS d, {p4:Date32} AS e, {p5:Bool} AS f FROM t WHERE s = '?' -- ?"
        );
        assert_eq!(pairs[0], ("param_p0".to_string(), "7".to_string()));
        assert_eq!(
            pairs[1],
            ("param_p1".to_string(), "x\\ty'z".to_string()),
            "탭 = \\t · 따옴표 그대로"
        );
        assert_eq!(pairs[2], ("param_p2".to_string(), "\\N".to_string()));
        assert_eq!(pairs[5].1, "true");
        // INSERT = 폴백(None) · 이진 값 = 폴백 · `NSQL_CH_INLINE` 없음 전제.
        assert!(server_params("INSERT INTO t VALUES (?)", &ps[..1]).is_none());
        assert!(
            server_params("SELECT ?", &[bp("X", Value::Bytes(vec![1]), VarType::Blob)]).is_none()
        );
        assert!(server_params("SELECT 1", &[]).is_none());
    }
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn p(v: Value) -> BindParam {
        BindParam {
            name: "X".into(),
            value: v,
            ty: VarType::Auto,
            direction: Direction::In,
        }
    }

    #[test]
    fn inline_params_skips_quotes_and_comments() {
        let out = inline_params(
            "SELECT '?', \"a?b\", `c?` -- ?\n/* ? */ WHERE x = ? AND y = ? AND z = ?",
            &[
                p(Value::Str("it's \\ ok".into())),
                p(Value::Int(7)),
                p(Value::Null),
            ],
        );
        assert_eq!(
            out,
            "SELECT '?', \"a?b\", `c?` -- ?\n/* ? */ WHERE x = 'it\\'s \\\\ ok' AND y = 7 AND z = NULL"
        );
        assert_eq!(inline_params("SELECT 1", &[]), "SELECT 1");
        assert_eq!(literal(&Value::Bytes(vec![0xAB, 0x01])), "unhex('AB01')");
        assert_eq!(literal(&Value::Float(f64::NAN)), "nan");
        assert_eq!(literal(&Value::Bool(true)), "true");
    }

    #[test]
    fn use_and_format_and_query_detection() {
        assert_eq!(parse_use("USE `mydb`").as_deref(), Some("mydb"));
        assert_eq!(parse_use("use system").as_deref(), Some("system"));
        assert!(parse_use("USE a b").is_none());
        assert!(parse_use("SELECT 1").is_none());
        for t in [
            "COMMIT",
            "commit;",
            "ROLLBACK TO SAVEPOINT a",
            "BEGIN",
            "BEGIN TRANSACTION",
            "START TRANSACTION",
            "SAVEPOINT s1",
            "RELEASE SAVEPOINT s1",
        ] {
            assert!(is_tx_statement(t.trim_end_matches(';')), "{t}");
        }
        assert!(!is_tx_statement("BEGIN SELECT 1"));
        assert!(!is_tx_statement("START REPLICA"));
        assert_eq!(
            strip_format("SELECT 1 FORMAT TabSeparated;"),
            ("SELECT 1".to_string(), true)
        );
        assert_eq!(
            strip_format("SELECT format('x')"),
            ("SELECT format('x')".to_string(), false)
        );
        assert!(is_query("  (SELECT 1)"));
        assert!(is_query("with t as (select 1) select * from t"));
        assert!(is_query("SHOW TABLES"));
        assert!(!is_query("INSERT INTO t VALUES (1)"));
        assert!(!is_query("CREATE TABLE t (x UInt8) ENGINE = Memory"));
    }

    #[test]
    fn value_mapping_by_meta_type() {
        assert_eq!(base_type("LowCardinality(Nullable(String))"), "String");
        assert_eq!(
            to_value(&Json::Str("9007199254740993".into()), "Int64"),
            Value::Int(9007199254740993)
        );
        assert_eq!(
            to_value(&Json::Str("18446744073709551615".into()), "UInt64"),
            Value::Decimal("18446744073709551615".into())
        );
        assert_eq!(to_value(&Json::Num(3.0), "UInt8"), Value::Int(3));
        assert_eq!(to_value(&Json::Num(1.5), "Float64"), Value::Float(1.5));
        assert!(matches!(
            to_value(&Json::Str("nan".into()), "Float32"),
            Value::Float(f) if f.is_nan()
        ));
        assert_eq!(
            to_value(&Json::Str("12.34".into()), "Decimal(10, 2)"),
            Value::Decimal("12.34".into())
        );
        assert_eq!(to_value(&Json::Bool(true), "Bool"), Value::Bool(true));
        assert_eq!(to_value(&Json::Null, "Nullable(String)"), Value::Null);
        assert_eq!(
            to_value(&Json::Str("2024-01-02".into()), "Date"),
            Value::Str("2024-01-02".into())
        );
        assert_eq!(
            to_value(
                &Json::Arr(vec![Json::Num(1.0), Json::Num(2.0)]),
                "Array(UInt8)"
            ),
            Value::Str("[1,2]".into())
        );
        assert_eq!(type_size("FixedString(16)"), (Some(16), None));
        assert_eq!(type_size("Decimal(10, 2)"), (Some(10), Some(2)));
        assert_eq!(type_size("Decimal64(4)"), (Some(18), Some(4)));
        assert_eq!(type_size("String"), (None, None));
    }

    #[test]
    fn json_compact_and_errors() {
        let rs = parse_compact(
            r#"{"meta":[{"name":"id","type":"UInt32"},{"name":"s","type":"Nullable(String)"}],"data":[[1,"a"],[2,null],[3,"c"]],"rows":3}"#,
            2,
        )
        .unwrap();
        assert_eq!(rs.columns.len(), 2);
        assert_eq!(rs.columns[1].type_name, "Nullable(String)");
        assert_eq!(rs.rows.len(), 2, "max_rows 절단");
        assert_eq!(rs.rows[1], vec![Value::Int(2), Value::Null]);
        let e = error_from(
            404,
            &[("x-clickhouse-exception-code".into(), "60".into())],
            b"Code: 60. DB::Exception: Table default.nope does not exist. (UNKNOWN_TABLE) (version 24.8.4.13 (official build))\n",
            "SELECT * FROM nope",
        );
        assert_eq!(e.code, Some(60));
        assert_eq!(
            e.message,
            "Table default.nope does not exist. (UNKNOWN_TABLE)"
        );
        assert_eq!(e.position, None);
        let e = error_from(
            400,
            &[],
            "Code: 62. DB::Exception: Syntax error: failed at position 15 ('FRM'): FRM t. Expected one of: FROM (version 24.8)".as_bytes(),
            "SELECT 가나 FRM t",
        );
        assert_eq!(e.code, Some(62));
        // 1부터 세는 바이트 15 = "SELECT 가나 "(7 + 6 + 1 바이트) 뒤 = 글자 인덱스 10.
        assert_eq!(e.position, Some(10));
        assert_eq!(
            written_rows(&[(
                "x-clickhouse-summary".into(),
                r#"{"read_rows":"0","written_rows":"3"}"#.into()
            )]),
            Some(3)
        );
        let e = error_from(500, &[], b"", "x");
        assert_eq!(e.message, "HTTP 500");
    }

    /// 가짜 HTTP 서버(ClickHouse 응답 모양) — 서버 없이 열기·조회·DML·USE·오류·취소 핸들 경로를 왕복한다.
    fn fake_server(hits: usize) -> (u16, std::thread::JoinHandle<Vec<String>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..hits {
                let (mut s, _) = l.accept().unwrap();
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                let (mut head_end, mut clen) = (None, 0usize);
                loop {
                    let n = s.read(&mut tmp).unwrap();
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if head_end.is_none() {
                        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            head_end = Some(i + 4);
                            let head = String::from_utf8_lossy(&buf[..i]).to_ascii_lowercase();
                            clen = head
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length:"))
                                .and_then(|v| v.trim().parse().ok())
                                .unwrap_or(0);
                        }
                    }
                    if let Some(e) = head_end {
                        if buf.len() >= e + clen {
                            break;
                        }
                    }
                }
                let e = head_end.unwrap_or(buf.len());
                let head = String::from_utf8_lossy(&buf[..e]).to_string();
                let body = String::from_utf8_lossy(&buf[e..e + clen]).to_string();
                seen.push(format!("{}\n{body}", head.lines().next().unwrap_or("")));
                let (status, extra, resp) = if head.starts_with("GET /ping") {
                    ("200 OK", "", "Ok.\n".to_string())
                } else if body.contains("version()") {
                    ("200 OK", "", r#"{"meta":[{"name":"version()","type":"String"},{"name":"currentDatabase()","type":"String"}],"data":[["24.8.1","default"]],"rows":1}"#.to_string())
                } else if body.contains("nope") {
                    ("404 Not Found", "X-ClickHouse-Exception-Code: 60\r\n", "Code: 60. DB::Exception: Table default.nope does not exist. (UNKNOWN_TABLE) (version 24.8.1)".to_string())
                } else if body.starts_with("INSERT") {
                    (
                        "200 OK",
                        "X-ClickHouse-Summary: {\"read_rows\":\"0\",\"written_rows\":\"2\"}\r\n",
                        String::new(),
                    )
                } else if body.starts_with("KILL") {
                    (
                        "200 OK",
                        "",
                        r#"{"meta":[],"data":[],"rows":0}"#.to_string(),
                    )
                } else {
                    ("200 OK", "", r#"{"meta":[{"name":"n","type":"Int64"},{"name":"s","type":"String"}],"data":[["1","한글"],["2","b"]],"rows":2}"#.to_string())
                };
                let out = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{resp}",
                    resp.len()
                );
                s.write_all(out.as_bytes()).unwrap();
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
            seen
        });
        (port, h)
    }

    #[test]
    fn round_trip_against_fake_server() {
        let (port, h) = fake_server(6);
        let spec =
            nsql_script::ConnectSpec::parse(&format!("clickhouse://u:pw@127.0.0.1:{port}/db1"))
                .unwrap();
        let mut s = ChSession::open(&spec).unwrap();
        assert_eq!(
            s.describe(),
            format!("ClickHouse 24.8.1 · 127.0.0.1:{port}/db1")
        );
        assert_eq!(s.dialect(), Dialect::Clickhouse);
        let r = s
            .execute(&ExecRequest {
                sql: "SELECT n, s FROM t WHERE n > ?".into(),
                params: vec![p(Value::Int(0))],
            })
            .unwrap();
        assert_eq!(
            r.result_sets[0].rows[0],
            vec![Value::Int(1), Value::Str("한글".into())]
        );
        // USE = 서버 왕복 없이 현재 DB 교체(다음 요청의 database= 매개변수).
        s.execute(&ExecRequest {
            sql: "USE other;".into(),
            params: vec![],
        })
        .unwrap();
        assert_eq!(s.database, "other");
        let r = s
            .execute(&ExecRequest {
                sql: "INSERT INTO t VALUES (1), (2)".into(),
                params: vec![],
            })
            .unwrap();
        assert_eq!(r.rows_affected, Some(2));
        let e = s
            .execute(&ExecRequest {
                sql: "SELECT * FROM nope".into(),
                params: vec![],
            })
            .unwrap_err();
        assert_eq!(e.code, Some(60));
        assert!(
            e.message.starts_with("Table default.nope does not exist"),
            "{}",
            e.message
        );
        assert!(s.is_alive());
        // 취소 핸들: 실행 중이 아니면 조용히 Ok(요청 0) · 실행 중 query_id가 있으면 KILL QUERY.
        let c = s.cancel_handle().unwrap();
        c.cancel().unwrap();
        *s.active.lock().unwrap() = Some("qid-1".into());
        c.cancel().unwrap();
        assert!(s.commit().is_ok() && s.rollback().is_ok());
        let r = s
            .execute(&ExecRequest {
                sql: "COMMIT;".into(),
                params: vec![],
            })
            .unwrap();
        assert_eq!(
            r,
            ExecResult::default(),
            "COMMIT 문장 = no-op · 서버 요청 0"
        );
        let seen = h.join().unwrap();
        assert_eq!(seen.len(), 6);
        assert!(
            seen[0].starts_with("POST /?database=db1&session_id=nsql"),
            "{}",
            seen[0]
        );
        // T-417: 조회의 바인드 = 서버 매개변수(`param_p0=0`) · 본문 `{p0:Int64}`.
        assert!(
            seen[0].contains("default_format=JSONCompact") && seen[0].contains("query_id="),
            "{}",
            seen[0]
        );
        assert!(seen[1].contains("param_p0=0"), "{}", seen[1]);
        assert!(seen[1].contains("max_result_rows") || !seen[1].contains("max_result_rows"));
        assert!(
            seen[1].ends_with("SELECT n, s FROM t WHERE n > {p0:Int64}"),
            "{}",
            seen[1]
        );
        assert!(
            seen[2].starts_with("POST /?database=other&"),
            "USE 뒤 database=: {}",
            seen[2]
        );
        assert!(seen[4].starts_with("GET /ping"), "{}", seen[4]);
        assert!(
            seen[5].ends_with("KILL QUERY WHERE query_id = 'qid-1' ASYNC"),
            "{}",
            seen[5]
        );
    }
}
