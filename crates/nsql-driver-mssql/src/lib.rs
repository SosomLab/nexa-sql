//! SQL Server 어댑터 — `tiberius`(순수 Rust TDS). `nsql_core::Session` 구현(동기 포트 · 내부 tokio 현재 스레드 런타임).
//!
//! ## 세션 변수 회수 방식(docs/05 §7 (a)+(b)의 절충)
//! tiberius는 위치 파라미터(`@P1…`)만 RPC로 보내고 OUTPUT 파라미터를 되돌려 받는 API가 없다. 그래서 배치를 이렇게 만든다:
//! ```text
//! DECLARE @V_CD NVARCHAR(4000) = @P1;      -- 값은 리터럴이 아니라 파라미터(타입 보존 · 플랜 캐시 오염 없음)
//! DECLARE @V_SEQ DECIMAL(38,10) = @P2;
//! <사용자 배치>                              -- 엔진이 :NAME → @NAME 으로 바꿔 둔 것
//! SELECT @V_CD AS [V_CD], @V_SEQ AS [V_SEQ];  -- InOut일 때만 트레일러 → 마지막 결과 집합 = OUT 값
//! ```
//! 배치 첫 문장 제약(CREATE PROC 등)은 엔진이 `DeclarePrepend`(리터럴)로 이미 처리했으므로 params가 비어 여기서는 그대로 보낸다.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use nsql_core::MessageSink;
use nsql_core::{
    Column, CursorId, DbError, Dialect, Direction, ExecRequest, ExecResult, ResultSet, Session,
    Value,
};
use nsql_script::ConnectSpec;
use std::sync::{Arc, Mutex};
use tiberius::{AuthMethod, Client, ColumnData, Config, EncryptionLevel, Row, ToSql};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

/// tiberius Info 토큰(PRINT · RAISERROR … WITH NOWAIT · 서버 정보 메시지) 캡처 — 실행 중 현재 스레드의 기본 구독자.
/// 싱크가 있으면 도착 즉시 전달(실시간 로그) · 없으면 버퍼(실행 뒤 `messages`).
struct InfoCapture {
    buf: Mutex<Vec<String>>,
    sink: Option<MessageSink>,
}

struct MsgVisitor(String);

impl Visit for MsgVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        }
    }
}

impl Subscriber for InfoCapture {
    fn enabled(&self, m: &Metadata<'_>) -> bool {
        *m.level() == Level::INFO && m.target().starts_with("tiberius")
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, e: &Event<'_>) {
        let mut v = MsgVisitor(String::new());
        e.record(&mut v);
        if v.0.is_empty() {
            return;
        }
        match &self.sink {
            Some(s) => s(v.0),
            None => {
                if let Ok(mut b) = self.buf.lock() {
                    b.push(v.0);
                }
            }
        }
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

type Tds = Client<Compat<TcpStream>>;

/// TCP keepalive 유휴 시간(초 · 0 = 끔) — 호스트가 설정에서 넣는다 · 다음 접속부터.
static KEEPALIVE_SECS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(60);

/// 설정 `net.keepalive_secs`(docs/53).
pub fn set_keepalive_secs(secs: u64) {
    KEEPALIVE_SECS.store(secs, std::sync::atomic::Ordering::Relaxed);
}

/// 암호화 범위(설정 `mssql.encrypt` · 접속 때 읽는다): 0 = 전 구간(`Required`) · 1 = 로그인만(`Off` — 로그인 뒤 평문 · **TDS Attention 취소 가능**).
static ENCRYPTION: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// 호스트가 설정에서 넣는다(`required` = 0 · `login` = 1). 이미 열린 세션은 그대로.
pub fn set_encryption(login_only: bool) {
    ENCRYPTION.store(u8::from(login_only), std::sync::atomic::Ordering::Relaxed);
}

/// 취소 방식(설정 `mssql.cancel`): 0 = Attention(가능할 때 · 세션 유지) · 1 = 소켓 종료(항상 · 세션 끊김).
static CANCEL_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// 호스트가 설정에서 넣는다(`attention` = false · `socket` = true). 다음 취소부터 적용.
pub fn set_cancel_socket(socket: bool) {
    CANCEL_MODE.store(u8::from(socket), std::sync::atomic::Ordering::Relaxed);
}

fn cancel_socket_mode() -> bool {
    CANCEL_MODE.load(std::sync::atomic::Ordering::Relaxed) == 1
}

fn login_only_encryption() -> bool {
    ENCRYPTION.load(std::sync::atomic::Ordering::Relaxed) == 1
}

/// TDS Attention 패킷(MS-TDS 2.2.1.6 · SSMS/SqlClient·JDBC의 취소): 헤더 8바이트만 — 타입 0x06 · EOM · 길이 8 · SPID 0 · 패킷 id 1 · 창 0.
const TDS_ATTENTION: [u8; 8] = [0x06, 0x01, 0x00, 0x08, 0x00, 0x00, 0x01, 0x00];

/// 서버 암호화 정책(PRELOGIN ENCRYPTION 응답 · MS-TDS 2.2.6.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerEncrypt {
    /// `ENCRYPT_OFF` — 로그인만 암호화 가능(Attention 가능).
    Off,
    /// `ENCRYPT_ON`.
    On,
    /// `ENCRYPT_NOT_SUP` — 암호화 없음(Attention 가능).
    NotSupported,
    /// `ENCRYPT_REQ` — 서버가 전 구간 강제(Force Encryption=Yes · Attention 불가).
    Required,
}

/// ★ 접속 **전** PRELOGIN 탐침(사용자 09-17 "접속 전에 알 수 있나"): 별도 TCP로 PRELOGIN(ENCRYPTION=OFF)을 보내고 서버의
/// ENCRYPTION 바이트만 읽는다(1 RTT · 로그인 없이 닫음). tiberius는 협상 결과를 노출하지 않으므로 이 값으로
/// "로그인만 암호화가 실제로 성립하는가"를 미리 안다.
pub fn probe_server_encryption(addr: &str, timeout: std::time::Duration) -> Option<ServerEncrypt> {
    use std::io::{Read, Write};
    let sa = std::net::ToSocketAddrs::to_socket_addrs(addr)
        .ok()?
        .next()?;
    let mut sock = std::net::TcpStream::connect_timeout(&sa, timeout).ok()?;
    sock.set_read_timeout(Some(timeout)).ok()?;
    sock.set_write_timeout(Some(timeout)).ok()?;
    // 옵션 표 5개(각 5바이트) + 종결자 = 26 → 데이터: VERSION 6 · ENCRYPTION 1 · INSTOPT 1 · THREADID 4 · MARS 1 = 13 → 39바이트.
    let mut payload: Vec<u8> = Vec::with_capacity(39);
    let mut off: u16 = 26;
    for (token, len) in [(0x00u8, 6u16), (0x01, 1), (0x02, 1), (0x03, 4), (0x04, 1)] {
        payload.push(token);
        payload.extend_from_slice(&off.to_be_bytes());
        payload.extend_from_slice(&len.to_be_bytes());
        off += len;
    }
    payload.push(0xFF);
    payload.extend_from_slice(&[0x0F, 0x00, 0x08, 0xB8, 0x00, 0x00]); // version
    payload.push(0x00); // ENCRYPT_OFF 요청
    payload.push(0x00); // instance
    payload.extend_from_slice(&[0, 0, 0, 0]); // thread id
    payload.push(0x00); // MARS off
    let len = (8 + payload.len()) as u16;
    let mut pkt = vec![0x12, 0x01];
    pkt.extend_from_slice(&len.to_be_bytes());
    pkt.extend_from_slice(&[0, 0, 1, 0]);
    pkt.extend_from_slice(&payload);
    sock.write_all(&pkt).ok()?;
    let mut hdr = [0u8; 8];
    sock.read_exact(&mut hdr).ok()?;
    let total = u16::from_be_bytes([hdr[2], hdr[3]]) as usize;
    if !(8..=4096).contains(&total) {
        return None;
    }
    let mut body = vec![0u8; total - 8];
    sock.read_exact(&mut body).ok()?;
    let _ = sock.shutdown(std::net::Shutdown::Both);
    let mut i = 0;
    while i + 5 <= body.len() && body[i] != 0xFF {
        let token = body[i];
        let o = u16::from_be_bytes([body[i + 1], body[i + 2]]) as usize;
        let l = u16::from_be_bytes([body[i + 3], body[i + 4]]) as usize;
        if token == 0x01 && l >= 1 && o < body.len() {
            return Some(match body[o] {
                0 => ServerEncrypt::Off,
                1 => ServerEncrypt::On,
                2 => ServerEncrypt::NotSupported,
                _ => ServerEncrypt::Required,
            });
        }
        i += 5;
    }
    None
}

#[allow(missing_debug_implementations)]
pub struct MssqlSession {
    rt: Runtime,
    client: Tds,
    /// 실행 취소(T-108 · docs/44 §4): tiberius에 취소(TDS Attention)가 없고 전 구간 TLS라 패킷을 직접 보낼 수도 없다 →
    /// **소켓 복제본을 닫아** 실행을 끊는다(서버가 배치를 중단·열린 트랜잭션 롤백). 세션은 죽고 다음 실행 때 자동 재접속.
    cancel_sock: std::sync::Arc<std::net::TcpStream>,
    /// 로그인만 암호화된 접속인가 → 취소 = Attention(세션 유지) · 아니면 소켓 종료(세션 끊김).
    attention_ok: bool,
    description: String,
    /// 서버 메시지 실시간 싱크(없으면 실행 뒤 `messages`).
    sink: Option<MessageSink>,
}

fn err(e: tiberius::error::Error) -> DbError {
    match &e {
        tiberius::error::Error::Server(t) => DbError {
            code: Some(i64::from(t.code())),
            message: format!("{} (줄 {})", t.message(), t.line()),
            position: None,
        },
        other => DbError {
            code: None,
            message: other.to_string(),
            position: None,
        },
    }
}

fn io_err(e: std::io::Error) -> DbError {
    DbError {
        code: None,
        message: e.to_string(),
        position: None,
    }
}

/// 실행 경로 — tiberius는 `query/execute`를 항상 `sp_executesql`(RPC)로 보낸다. 그 안에서 만든 `#temp`는 반환 시 사라지고
/// `SET`·`USE`도 원복되므로, **파라미터 없는 DDL·세션 문장은 SQL 배치**(`simple_query`)로 보낸다(docs/05 §7 (a) 단점).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Route {
    /// `sp_executesql` — 파라미터·OUT 트레일러·조회.
    Rpc,
    /// `sp_executesql` + rows affected — 파라미터 없는 DML.
    Execute,
    /// SQL 배치 — DDL · `#temp` · `SET`/`USE` · `IF`/`DECLARE`/`BEGIN` 등.
    Batch,
}

pub fn route(sql: &str, has_params: bool, has_outs: bool) -> Route {
    if has_params || has_outs {
        return Route::Rpc;
    }
    let up = sql.trim_start().to_ascii_uppercase();
    let first: String = up.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    match first.as_str() {
        "SELECT" | "WITH" => Route::Rpc,
        "INSERT" | "UPDATE" | "DELETE" | "MERGE" => Route::Execute,
        _ => Route::Batch,
    }
}

/// 배치 렌더링 — 순수 함수(테스트 대상). `(배치 텍스트, 위치 파라미터, 트레일러 컬럼 이름들)`.
pub fn render_batch(req: &ExecRequest) -> (String, Vec<Value>, Vec<String>) {
    let mut head = String::new();
    let mut params = Vec::new();
    let mut outs = Vec::new();
    for (i, p) in req.params.iter().enumerate() {
        head.push_str(&format!(
            "DECLARE @{} {} = @P{};\n",
            p.name,
            p.ty.tsql_type(),
            i + 1
        ));
        params.push(p.value.clone());
        if p.direction != Direction::In {
            outs.push(p.name.clone());
        }
    }
    let mut sql = format!("{head}{}", req.sql.trim_end_matches(';'));
    if !outs.is_empty() {
        let cols: Vec<String> = outs.iter().map(|n| format!("@{n} AS [{n}]")).collect();
        sql.push_str(&format!(";\nSELECT {};", cols.join(", ")));
    }
    (sql, params, outs)
}

/// `Value` → tiberius 파라미터(소유 값 · 문자열은 NVARCHAR).
fn to_param(v: &Value) -> Box<dyn ToSql> {
    match v {
        Value::Null => Box::new(Option::<String>::None),
        Value::Int(i) => Box::new(*i),
        Value::Float(f) => Box::new(*f),
        Value::Decimal(d) => Box::new(d.clone()),
        Value::Str(s) => Box::new(s.clone()),
        Value::Bool(b) => Box::new(*b),
        Value::Bytes(b) => Box::new(b.clone()),
        Value::Cursor(_) => Box::new(Option::<String>::None),
    }
}

fn cell(row: &Row, i: usize, data: &ColumnData<'_>) -> Value {
    match data {
        ColumnData::U8(v) => v.map_or(Value::Null, |x| Value::Int(i64::from(x))),
        ColumnData::I16(v) => v.map_or(Value::Null, |x| Value::Int(i64::from(x))),
        ColumnData::I32(v) => v.map_or(Value::Null, |x| Value::Int(i64::from(x))),
        ColumnData::I64(v) => v.map_or(Value::Null, Value::Int),
        ColumnData::F32(v) => v.map_or(Value::Null, |x| Value::Float(f64::from(x))),
        ColumnData::F64(v) => v.map_or(Value::Null, Value::Float),
        ColumnData::Bit(v) => v.map_or(Value::Null, Value::Bool),
        ColumnData::String(v) => v
            .as_ref()
            .map_or(Value::Null, |s| Value::Str(s.to_string())),
        ColumnData::Guid(v) => v.map_or(Value::Null, |g| Value::Str(g.to_string())),
        ColumnData::Binary(v) => v.as_ref().map_or(Value::Null, |b| Value::Bytes(b.to_vec())),
        ColumnData::Numeric(v) => v.map_or(Value::Null, |n| Value::Decimal(n.to_string())),
        ColumnData::Xml(v) => v
            .as_ref()
            .map_or(Value::Null, |x| Value::Str(x.to_string())),
        ColumnData::DateTime(_) | ColumnData::SmallDateTime(_) | ColumnData::DateTime2(_) => row
            .try_get::<chrono::NaiveDateTime, usize>(i)
            .ok()
            .flatten()
            .map_or(Value::Null, |d| {
                Value::Str(d.format("%Y-%m-%d %H:%M:%S%.f").to_string())
            }),
        ColumnData::Date(_) => row
            .try_get::<chrono::NaiveDate, usize>(i)
            .ok()
            .flatten()
            .map_or(Value::Null, |d| Value::Str(d.to_string())),
        ColumnData::Time(_) => row
            .try_get::<chrono::NaiveTime, usize>(i)
            .ok()
            .flatten()
            .map_or(Value::Null, |d| Value::Str(d.to_string())),
        ColumnData::DateTimeOffset(_) => row
            .try_get::<chrono::DateTime<chrono::Utc>, usize>(i)
            .ok()
            .flatten()
            .map_or(Value::Null, |d| Value::Str(d.to_rfc3339())),
    }
}

fn rows_to_result_set(rows: &[Row]) -> ResultSet {
    let columns: Vec<Column> = rows
        .first()
        .map(|r| {
            r.columns()
                .iter()
                .map(|c| Column {
                    name: c.name().to_string(),
                    type_name: format!("{:?}", c.column_type()),
                })
                .collect()
        })
        .unwrap_or_default();
    let data = rows
        .iter()
        .map(|r| {
            r.cells()
                .enumerate()
                .map(|(i, (_, d))| cell(r, i, d))
                .collect()
        })
        .collect();
    ResultSet {
        columns,
        rows: data,
    }
}

impl MssqlSession {
    /// `mssql://user:pass@host:1433/db` · `?trust=0`(기본 = 서버 인증서 신뢰 — 사내망 자체서명 관용).
    pub fn connect(spec: &ConnectSpec) -> Result<Self, DbError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(io_err)?;
        let mut config = Config::new();
        config.host(spec.host.clone().unwrap_or_else(|| "localhost".into()));
        config.port(spec.port.unwrap_or(1433));
        if let Some(db) = &spec.database {
            config.database(db);
        }
        match (&spec.user, &spec.password) {
            (Some(u), Some(p)) => config.authentication(AuthMethod::sql_server(u, p)),
            (Some(u), None) => config.authentication(AuthMethod::sql_server(u, "")),
            (None, _) => {
                #[cfg(all(windows, feature = "winauth"))]
                config.authentication(AuthMethod::Integrated);
            }
        }
        config.trust_cert();
        // 로그인만 암호화(`Off`)면 로그인 뒤 평문 → 소켓 복제본으로 Attention을 보낼 수 있다(T-108 · docs/44 §4).
        config.application_name("nexa-sql");
        let addr = config.get_addr();
        // 설정이 `login`이면 먼저 PRELOGIN 탐침으로 서버 정책을 본다: 서버가 전 구간을 강제(REQ)하면 tiberius가 조용히
        // 전 구간으로 올리므로(negotiated_encryption) Attention을 보내면 TLS 스트림이 깨진다 → 이 접속만 `required`로(임시 적용).
        let want_login = login_only_encryption();
        let policy = if want_login {
            probe_server_encryption(&addr, std::time::Duration::from_secs(3))
        } else {
            None
        };
        let login_only = want_login
            && matches!(
                policy,
                Some(ServerEncrypt::Off | ServerEncrypt::NotSupported)
            );
        config.encryption(if login_only {
            EncryptionLevel::Off
        } else {
            EncryptionLevel::Required
        });
        let encrypt_note = match (want_login, policy) {
            (false, _) => "encrypt=required",
            (true, Some(ServerEncrypt::Off | ServerEncrypt::NotSupported)) => {
                "encrypt=login(Attention)"
            }
            (true, Some(_)) => "encrypt=required(server forces · Attention off)",
            (true, None) => "encrypt=required(probe failed · Attention off)",
        };
        // 동기 소켓으로 열고 복제본을 남긴다(취소 = 복제본 shutdown) → 논블로킹으로 바꿔 tokio에 넘긴다.
        let std_tcp = std::net::TcpStream::connect(&addr).map_err(io_err)?;
        std_tcp.set_nodelay(true).map_err(io_err)?;
        // TCP keepalive(설정 `net.keepalive_secs` · docs/53 §2) — 빈 세그먼트 · 서버 유휴 정책 무관 · 끊긴 경로를 OS가 정리.
        let ka = KEEPALIVE_SECS.load(std::sync::atomic::Ordering::Relaxed);
        if ka > 0 {
            let sock = socket2::SockRef::from(&std_tcp);
            let mut opt =
                socket2::TcpKeepalive::new().with_time(std::time::Duration::from_secs(ka));
            #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
            {
                opt = opt.with_interval(std::time::Duration::from_secs(ka.clamp(5, 30)));
            }
            let _ = sock.set_tcp_keepalive(&opt);
        }
        let cancel_sock = std::sync::Arc::new(std_tcp.try_clone().map_err(io_err)?);
        std_tcp.set_nonblocking(true).map_err(io_err)?;
        let client = rt.block_on(async move {
            let tcp = TcpStream::from_std(std_tcp).map_err(io_err)?;
            Client::connect(config, tcp.compat_write())
                .await
                .map_err(err)
        })?;
        Ok(MssqlSession {
            rt,
            client,
            cancel_sock,
            attention_ok: login_only,
            description: format!("{} · {encrypt_note}", spec.redacted()),
            sink: None,
        })
    }
}

/// 취소 핸들: `attention` = TDS Attention 패킷(SSMS 방식 · 서버가 배치를 멈추고 `DONE_ATTN`으로 답한다 · 접속 유지) ·
/// 아니면 소켓 종료(전 구간 TLS라 패킷을 끼워 넣을 수 없을 때 · 세션 끊김).
struct MssqlCancel {
    sock: std::sync::Arc<std::net::TcpStream>,
    attention: bool,
}

impl nsql_core::CancelHandle for MssqlCancel {
    fn cancel(&self) -> Result<(), DbError> {
        use std::io::Write;
        if !self.attention {
            return self.sock.shutdown(std::net::Shutdown::Both).map_err(io_err);
        }
        // 복제본은 논블로킹 플래그를 공유할 수 있다 — 8바이트는 WouldBlock이 나면 잠깐 뒤 다시 쓴다(블로킹 모드는 바꾸지 않는다).
        let mut sock: &std::net::TcpStream = &self.sock;
        let mut left: &[u8] = &TDS_ATTENTION;
        let mut tries = 0;
        while !left.is_empty() {
            match sock.write(left) {
                Ok(n) => left = &left[n..],
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && tries < 200 => {
                    tries += 1;
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(e) => return Err(io_err(e)),
            }
        }
        let _ = sock.flush();
        Ok(())
    }

    fn drops_session(&self) -> bool {
        !self.attention
    }
}

impl Session for MssqlSession {
    fn cancel_handle(&self) -> Option<std::sync::Arc<dyn nsql_core::CancelHandle>> {
        // Attention은 로그인만 암호화된 접속에서만 · 설정이 소켓 종료면 항상 소켓 종료.
        Some(std::sync::Arc::new(MssqlCancel {
            sock: self.cancel_sock.clone(),
            attention: self.attention_ok && !cancel_socket_mode(),
        }))
    }

    fn dialect(&self) -> Dialect {
        Dialect::Mssql
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn set_message_sink(&mut self, sink: Option<MessageSink>) {
        self.sink = sink;
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        // PRINT/RAISERROR 캡처 — 이 스레드에서 실행되는 동안만 기본 구독자(다른 스레드·다른 크레이트 무영향).
        let capture = Arc::new(InfoCapture {
            buf: Mutex::new(Vec::new()),
            sink: self.sink.clone(),
        });
        let _guard = tracing::subscriber::set_default(capture.clone());
        let mut result = self.execute_inner(req)?;
        if let Ok(mut b) = capture.buf.lock() {
            result.messages.append(&mut b);
        }
        Ok(result)
    }

    /// 커서 유지 없음(T-48a 1차 · docs/43 §3-3) — tiberius `QueryStream`은 클라이언트를 빌려 세션 구조체에 담을 수 없다.
    /// 호스트가 OFFSET 재질의로 폴백한다. 스트림 유지(워커 태스크)는 T-48c.
    fn cursor_supported(&self) -> bool {
        false
    }

    fn fetch_cursor(&mut self, _cursor: CursorId) -> Result<ResultSet, DbError> {
        Err(DbError { code: None, message: "SQL Server: 커서 변수는 sp_executesql로 넘길 수 없습니다 — 결과 집합으로 받으세요(docs/05 §7)".into(), position: None })
    }

    /// `schema` = 기본 데이터베이스 전환(`USE [db]` · SQL Server는 세션 기본 스키마를 바꿀 수 없어 DB 전환이 그 자리 · 사용자 09-18).
    fn set_option(&mut self, name: &str, value: &str) -> Result<(), DbError> {
        if name != "schema" {
            return Ok(());
        }
        let sql = format!("USE [{}]", value.replace(']', "]]"));
        let client = &mut self.client;
        self.rt.block_on(async {
            client
                .simple_query(&sql)
                .await
                .map_err(err)?
                .into_results()
                .await
                .map_err(err)?;
            Ok(())
        })
    }

    fn commit(&mut self) -> Result<(), DbError> {
        let client = &mut self.client;
        self.rt.block_on(async {
            client
                .simple_query("IF @@TRANCOUNT > 0 COMMIT")
                .await
                .map_err(err)?
                .into_results()
                .await
                .map_err(err)
        })?;
        Ok(())
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        let client = &mut self.client;
        self.rt.block_on(async {
            client
                .simple_query("IF @@TRANCOUNT > 0 ROLLBACK")
                .await
                .map_err(err)?
                .into_results()
                .await
                .map_err(err)
        })?;
        Ok(())
    }
}

impl MssqlSession {
    fn execute_inner(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let (sql, values, outs) = render_batch(req);
        let boxed: Vec<Box<dyn ToSql>> = values.iter().map(to_param).collect();
        let refs: Vec<&dyn ToSql> = boxed.iter().map(|b| b.as_ref()).collect();
        let client = &mut self.client;
        let mut result = ExecResult::default();
        match route(&req.sql, !values.is_empty(), !outs.is_empty()) {
            Route::Rpc => {
                let sets: Vec<Vec<Row>> = self.rt.block_on(async {
                    let stream = client.query(sql.as_str(), &refs).await.map_err(err)?;
                    stream.into_results().await.map_err(err)
                })?;
                let mut result_sets: Vec<ResultSet> = sets
                    .iter()
                    .filter(|s| !s.is_empty())
                    .map(|s| rows_to_result_set(s))
                    .collect();
                if !outs.is_empty() {
                    if let Some(trailer) = result_sets.pop() {
                        if let Some(row) = trailer.rows.first() {
                            for (c, v) in trailer.columns.iter().zip(row.iter()) {
                                result
                                    .out_params
                                    .push((c.name.to_ascii_uppercase(), v.clone()));
                            }
                        }
                    }
                    if result_sets.is_empty() {
                        result.rows_affected = Some(0);
                    }
                }
                result.result_sets = result_sets;
            }
            Route::Execute => {
                let n = self
                    .rt
                    .block_on(async { client.execute(sql.as_str(), &refs).await.map_err(err) })?;
                result.rows_affected = Some(n.total());
            }
            Route::Batch => {
                // SQL 배치(sp_executesql 아님) — `#temp`·SET·USE 같은 세션 상태가 남는다. 파라미터 없음.
                let sets: Vec<Vec<Row>> = self.rt.block_on(async {
                    let stream = client.simple_query(sql.as_str()).await.map_err(err)?;
                    stream.into_results().await.map_err(err)
                })?;
                result.result_sets = sets
                    .iter()
                    .filter(|s| !s.is_empty())
                    .map(|s| rows_to_result_set(s))
                    .collect();
                if result.result_sets.is_empty() {
                    result.rows_affected = None;
                }
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{BindParam, VarType};

    #[test]
    fn batch_declares_params_and_trailer_select() {
        let req = ExecRequest {
            sql: "SELECT @V_CD = A.CD, @V_SEQ = A.SEQ FROM T A WHERE X = @V_IN".into(),
            params: vec![
                BindParam {
                    name: "V_CD".into(),
                    value: Value::Null,
                    ty: VarType::Auto,
                    direction: Direction::InOut,
                },
                BindParam {
                    name: "V_SEQ".into(),
                    value: Value::Int(3),
                    ty: VarType::Number,
                    direction: Direction::InOut,
                },
                BindParam {
                    name: "V_IN".into(),
                    value: Value::Str("x".into()),
                    ty: VarType::Varchar2(10),
                    direction: Direction::In,
                },
            ],
        };
        let (sql, params, outs) = render_batch(&req);
        assert_eq!(
            sql,
            "DECLARE @V_CD NVARCHAR(4000) = @P1;\nDECLARE @V_SEQ DECIMAL(38,10) = @P2;\nDECLARE @V_IN NVARCHAR(10) = @P3;\nSELECT @V_CD = A.CD, @V_SEQ = A.SEQ FROM T A WHERE X = @V_IN;\nSELECT @V_CD AS [V_CD], @V_SEQ AS [V_SEQ];"
        );
        assert_eq!(params.len(), 3);
        assert_eq!(outs, vec!["V_CD", "V_SEQ"]);
    }

    #[test]
    fn routing_rules() {
        assert_eq!(route("SELECT 1", false, false), Route::Rpc);
        assert_eq!(
            route("INSERT INTO t VALUES (1)", false, false),
            Route::Execute
        );
        assert_eq!(route("CREATE TABLE #t (a INT)", false, false), Route::Batch);
        assert_eq!(
            route("IF OBJECT_ID('x') IS NOT NULL DROP TABLE x", false, false),
            Route::Batch
        );
        assert_eq!(route("SET NOCOUNT ON", false, false), Route::Batch);
        assert_eq!(route("INSERT INTO t VALUES (@V)", true, false), Route::Rpc);
    }

    #[test]
    fn plain_query_has_no_trailer() {
        let req = ExecRequest {
            sql: "SELECT 1".into(),
            params: vec![],
        };
        let (sql, params, outs) = render_batch(&req);
        assert_eq!(sql, "SELECT 1");
        assert!(params.is_empty() && outs.is_empty());
    }
}
