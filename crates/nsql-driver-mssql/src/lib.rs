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

#[allow(missing_debug_implementations)]
pub struct MssqlSession {
    rt: Runtime,
    client: Tds,
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
        config.encryption(EncryptionLevel::Required);
        config.application_name("nexa-sql");
        let addr = config.get_addr();
        let client = rt.block_on(async move {
            let tcp = TcpStream::connect(&addr).await.map_err(io_err)?;
            tcp.set_nodelay(true).map_err(io_err)?;
            Client::connect(config, tcp.compat_write())
                .await
                .map_err(err)
        })?;
        Ok(MssqlSession {
            rt,
            client,
            description: spec.redacted(),
            sink: None,
        })
    }
}

impl Session for MssqlSession {
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

    fn fetch_cursor(&mut self, _cursor: CursorId) -> Result<ResultSet, DbError> {
        Err(DbError { code: None, message: "SQL Server: 커서 변수는 sp_executesql로 넘길 수 없습니다 — 결과 집합으로 받으세요(docs/05 §7)".into(), position: None })
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
