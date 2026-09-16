//! # nsql-core — 도메인 허브 (의존 0)
//!
//! 모든 크레이트가 이 타입으로 말한다. 드라이버·UI·스크립트 엔진은 여기에 의존하고,
//! 여기는 아무것도 모른다(계열 규칙: 허브 크레이트는 앱 의존 0).
//!
//! - [`Dialect`] — DBMS 방언. 스크립트 엔진이 바인드 문법·배치 규칙을 고르는 축.
//! - [`Value`] / [`VarType`] — 세션 변수와 결과 셀의 공통 값 모델.
//! - [`ExecRequest`] / [`ExecResult`] — 엔진 ↔ 드라이버 사이의 계약(`Session` 포트).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use std::fmt;

/// DBMS 방언 — 값이 아니라 **문법 규칙의 축**이다(docs/08 §4).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Dialect {
    Oracle,
    Mssql,
    Postgres,
    Mysql,
    Sqlite,
    /// ODBC 폴백(Tibero · Altibase · CUBRID …) — `?` 위치 바인드.
    Odbc,
}

impl Dialect {
    /// 접속 문자열 스킴·설정 값에서 방언을 고른다(대소문자 무관).
    pub fn from_name(s: &str) -> Option<Dialect> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "oracle" | "ora" | "oci" => Dialect::Oracle,
            "mssql" | "sqlserver" | "tds" => Dialect::Mssql,
            "postgres" | "postgresql" | "pg" => Dialect::Postgres,
            "mysql" | "mariadb" => Dialect::Mysql,
            "sqlite" | "sqlite3" => Dialect::Sqlite,
            "odbc" | "tibero" | "altibase" | "cubrid" => Dialect::Odbc,
            _ => return None,
        })
    }

    /// 전체(접속 폼의 DB 종류 목록 순서).
    pub const ALL: [Dialect; 6] = [
        Dialect::Oracle,
        Dialect::Mssql,
        Dialect::Postgres,
        Dialect::Mysql,
        Dialect::Sqlite,
        Dialect::Odbc,
    ];

    /// 기본 포트(접속 폼·CLI에서 포트를 비우면 이 값). 파일·DSN 기반 방언은 `None`.
    pub fn default_port(self) -> Option<u16> {
        match self {
            Dialect::Oracle => Some(1521),
            Dialect::Mssql => Some(1433),
            Dialect::Postgres => Some(5432),
            Dialect::Mysql => Some(3306),
            Dialect::Sqlite | Dialect::Odbc => None,
        }
    }

    /// 표시 이름(제품명 — 번역하지 않는다).
    pub fn display_name(self) -> &'static str {
        match self {
            Dialect::Oracle => "Oracle",
            Dialect::Mssql => "SQL Server",
            Dialect::Postgres => "PostgreSQL",
            Dialect::Mysql => "MySQL",
            Dialect::Sqlite => "SQLite",
            Dialect::Odbc => "ODBC",
        }
    }

    /// 호스트·포트 대신 **파일 경로/DSN** 하나로 접속하는 방언인가(폼이 필드를 바꾸는 근거).
    pub fn is_file_based(self) -> bool {
        matches!(self, Dialect::Sqlite | Dialect::Odbc)
    }

    /// 방언의 바인드 플레이스홀더 문법.
    pub fn bind_style(self) -> BindStyle {
        match self {
            Dialect::Oracle => BindStyle::NamedColon,
            Dialect::Mssql => BindStyle::NamedAt,
            Dialect::Postgres => BindStyle::Dollar,
            Dialect::Mysql | Dialect::Sqlite | Dialect::Odbc => BindStyle::Question,
        }
    }

    /// 배치 구분자(`GO`)가 있는 방언인가 — T-SQL만.
    pub fn has_batch_separator(self) -> bool {
        matches!(self, Dialect::Mssql)
    }
}

impl fmt::Display for Dialect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Dialect::Oracle => "oracle",
            Dialect::Mssql => "mssql",
            Dialect::Postgres => "postgres",
            Dialect::Mysql => "mysql",
            Dialect::Sqlite => "sqlite",
            Dialect::Odbc => "odbc",
        })
    }
}

/// 드라이버가 받는 플레이스홀더 모양.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BindStyle {
    /// `:NAME` — Oracle(이름 바인드).
    NamedColon,
    /// `@NAME` — SQL Server(`sp_executesql` 파라미터).
    NamedAt,
    /// `$1 $2` — PostgreSQL(위치).
    Dollar,
    /// `?` — MySQL · SQLite · ODBC(위치).
    Question,
}

/// 세션 변수 타입 — SQL*Plus `VARIABLE` 타입의 부분집합 + [`VarType::Auto`](docs/04 §8.1).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VarType {
    Number,
    Varchar2(u32),
    Char(u32),
    Clob,
    RefCursor,
    BinaryFloat,
    BinaryDouble,
    Date,
    Timestamp,
    /// 선언 없이 대입으로 생긴 변수 — 값에서 타입을 따라간다(Golden 관용 · docs/08 §3-2).
    Auto,
}

impl VarType {
    /// `VARIABLE` 문의 타입 토큰을 해석한다. 예: `NUMBER`, `VARCHAR2(30)`, `REFCURSOR`.
    pub fn parse(s: &str) -> Option<VarType> {
        let s = s.trim();
        let up = s.to_ascii_uppercase();
        let (head, arg) = match up.find('(') {
            Some(i) => (&up[..i], up[i + 1..].trim_end_matches(')').trim()),
            None => (up.as_str(), ""),
        };
        let len = || -> u32 {
            arg.split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(4000)
        };
        Some(match head.trim() {
            "NUMBER" | "NUM" | "INT" | "INTEGER" | "DECIMAL" | "NUMERIC" => VarType::Number,
            "VARCHAR2" | "VARCHAR" | "NVARCHAR2" | "NVARCHAR" | "STRING" => {
                VarType::Varchar2(len())
            }
            "CHAR" | "NCHAR" => VarType::Char(if arg.is_empty() { 1 } else { len() }),
            "CLOB" | "NCLOB" | "TEXT" => VarType::Clob,
            "REFCURSOR" | "REF_CURSOR" | "CURSOR" | "SYS_REFCURSOR" => VarType::RefCursor,
            "BINARY_FLOAT" | "FLOAT" | "REAL" => VarType::BinaryFloat,
            "BINARY_DOUBLE" | "DOUBLE" => VarType::BinaryDouble,
            "DATE" => VarType::Date,
            "TIMESTAMP" | "DATETIME" | "DATETIME2" => VarType::Timestamp,
            _ => return None,
        })
    }

    /// 값에서 타입을 추론한다([`VarType::Auto`] 변수용).
    pub fn infer(v: &Value) -> VarType {
        match v {
            Value::Int(_) | Value::Decimal(_) => VarType::Number,
            Value::Float(_) => VarType::BinaryDouble,
            Value::Str(s) => VarType::Varchar2((s.chars().count().max(1)) as u32),
            Value::Bool(_) => VarType::Number,
            Value::Bytes(_) => VarType::Clob,
            Value::Cursor(_) => VarType::RefCursor,
            Value::Null => VarType::Auto,
        }
    }

    /// T-SQL 선언 타입 — `DECLARE @v <type>` · `sp_executesql @params` 양쪽에 쓴다.
    pub fn tsql_type(&self) -> String {
        match self {
            VarType::Number => "DECIMAL(38,10)".into(),
            VarType::Varchar2(n) => format!(
                "NVARCHAR({})",
                if *n > 4000 {
                    "MAX".into()
                } else {
                    n.to_string()
                }
            ),
            VarType::Char(n) => format!("NCHAR({n})"),
            VarType::Clob => "NVARCHAR(MAX)".into(),
            // T-SQL 커서 파라미터는 sp_executesql로 못 넘긴다(docs/05 §7) — 자리표시.
            VarType::RefCursor => "CURSOR".into(),
            VarType::BinaryFloat => "REAL".into(),
            VarType::BinaryDouble => "FLOAT".into(),
            VarType::Date => "DATE".into(),
            VarType::Timestamp => "DATETIME2".into(),
            // Auto = 값에서 추론 전 — NVARCHAR로 받고 클라이언트가 다시 추론한다(SQL_VARIANT는 드라이버 회수가 불안정).
            VarType::Auto => "NVARCHAR(4000)".into(),
        }
    }
}

/// 서버 측 커서 핸들 — 드라이버가 발급하고 `PRINT`가 1회 소비한다(docs/04 §8.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct CursorId(pub u64);

/// 세션 변수 값 · 결과 셀 값의 공통 모델.
/// `Decimal`은 **문자 그대로** 보존한다 — Oracle `NUMBER(38)`을 f64로 깎지 않는다(docs/06 A-5 주의 (2)).
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Decimal(String),
    Str(String),
    Bool(bool),
    Bytes(Vec<u8>),
    Cursor(CursorId),
}

impl Value {
    /// SQL 리터럴로 직렬화 — `DECLARE @v = <리터럴>` 프리펜드(docs/05 §7 (b))와 `PRINT` 표시용.
    /// 문자열은 `'` 이스케이프. `Bytes`는 16진 리터럴. 커서는 리터럴이 없다(NULL).
    pub fn to_sql_literal(&self, dialect: Dialect) -> String {
        match self {
            Value::Null => "NULL".into(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => {
                if f.is_finite() {
                    format!("{f:?}")
                } else {
                    "NULL".into()
                }
            }
            Value::Decimal(d) => d.clone(),
            Value::Str(s) => {
                let esc = s.replace('\'', "''");
                match dialect {
                    Dialect::Mssql => format!("N'{esc}'"),
                    _ => format!("'{esc}'"),
                }
            }
            Value::Bool(b) => if *b { "1" } else { "0" }.into(),
            Value::Bytes(b) => {
                let hex: String = b.iter().map(|x| format!("{x:02X}")).collect();
                match dialect {
                    Dialect::Mssql => format!("0x{hex}"),
                    Dialect::Postgres => format!("'\\x{hex}'"),
                    _ => format!("'{hex}'"),
                }
            }
            Value::Cursor(_) => "NULL".into(),
        }
    }

    /// 사람이 읽는 표시 문자열(`PRINT`).
    pub fn display(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Decimal(d) => d.clone(),
            Value::Str(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Bytes(b) => format!("<{} bytes>", b.len()),
            Value::Cursor(c) => format!("<refcursor #{}>", c.0),
        }
    }
}

/// 바인드 방향 — PL/SQL 블록·`EXEC`은 InOut, 조회는 In(docs/08 §4-1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    In,
    InOut,
    Out,
}

/// 드라이버로 넘기는 바인드 하나.
#[derive(Clone, PartialEq, Debug)]
pub struct BindParam {
    /// 저장소 키(대문자). 위치 바인드 방언에서도 이름을 유지해 OUT 값을 되돌려 받는다.
    pub name: String,
    pub value: Value,
    pub ty: VarType,
    pub direction: Direction,
}

/// 드라이버가 실행할 요청 — 엔진이 만든다(`nsql-script::Prepared`에서 변환).
#[derive(Clone, PartialEq, Debug)]
pub struct ExecRequest {
    pub sql: String,
    pub params: Vec<BindParam>,
}

/// 테이블 키 정보(카탈로그 → SQL 생성의 키 선택 · docs/41): `pk` = 기본 키 컬럼(순서) · `unique` = 유니크 제약/인덱스(이름, 컬럼).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct KeyInfo {
    pub pk: Vec<String>,
    pub unique: Vec<(String, Vec<String>)>,
}

/// 결과 컬럼 메타.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Column {
    pub name: String,
    /// 드라이버 원 타입 이름(`NUMBER`, `nvarchar`, `int4` …) — 표시·복사용.
    pub type_name: String,
}

/// 결과 집합 — M0 임시 모델. 컬럼형(Arrow 여부)은 D-2에서 결정.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ResultSet {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
}

/// 드라이버 실행 결과.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ExecResult {
    pub rows_affected: Option<u64>,
    /// OUT/InOut 바인드의 실행 후 값 — 엔진이 저장소에 흡수한다.
    pub out_params: Vec<(String, Value)>,
    pub result_sets: Vec<ResultSet>,
    /// `DBMS_OUTPUT` · T-SQL `PRINT` 등 서버 메시지.
    pub messages: Vec<String>,
    /// 단계별 소요([`Timeline`] · docs/26). 드라이버가 채운 만큼만 — 비면 러너가 전체 시간을 `Execute`로 넣는다.
    pub timing: Timeline,
}

// ────────────────────────────────────────────── 성능 계측(docs/26 — 사용자 09-14 "어디가 느린지 단계별로")

/// 한 번의 실행이 지나는 단계. **사용자 관점** 9단계 + 서버 메시지 플러시 + 커밋.
/// 순서는 시간 순이며 어느 층이 재는지는 docs/26 §2 표가 SSOT.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Stage {
    /// 쿼리 작성(편집기 — 키 입력부터 실행 요청까지 · GUI만).
    Compose,
    /// 요청 송신(파싱·바인드·네트워크 왕복 시작 전 준비).
    Send,
    /// 서버 실행(옵티마이저·실행 — 첫 행/완료 응답까지).
    Execute,
    /// 행 페치(배열 페치 왕복 · 부분/전체).
    Fetch,
    /// 수신 디코딩(와이어 → `Value` — 드라이버 변환).
    Receive,
    /// 결과 탑재(호스트 메모리 구조 · 그리드 모델).
    Load,
    /// 화면 렌더(그리드 첫 그리기 · 이후 프레임).
    Render,
    /// 탐색(스크롤 · 추가 페치 · 페이징).
    Navigate,
    /// 중간 산출물 정리(수신 버퍼 · 렌더 캐시 · 메모리 회수).
    Cleanup,
    /// 서버 메시지 플러시(`DBMS_OUTPUT.GET_LINE(S)` 폴링 · T-SQL PRINT 수집).
    OutputFlush,
    /// 커밋/롤백.
    Commit,
}

impl Stage {
    /// 짧은 표시 이름(로그·상태줄 — 번역하지 않는다).
    pub fn label(self) -> &'static str {
        match self {
            Stage::Compose => "compose",
            Stage::Send => "send",
            Stage::Execute => "execute",
            Stage::Fetch => "fetch",
            Stage::Receive => "receive",
            Stage::Load => "load",
            Stage::Render => "render",
            Stage::Navigate => "navigate",
            Stage::Cleanup => "cleanup",
            Stage::OutputFlush => "output",
            Stage::Commit => "commit",
        }
    }
}

/// 한 단계의 측정값.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Span {
    pub stage: Stage,
    pub dur: std::time::Duration,
    /// 행 수(페치·탑재·렌더) · 줄 수(OutputFlush).
    pub rows: Option<u64>,
    /// 바이트 추정(수신·탑재).
    pub bytes: Option<u64>,
    /// 부가 설명(예: "execute+fetch (TDS stream)").
    pub note: Option<String>,
}

/// 단계별 소요의 순서 있는 목록 — 각 층이 자기 단계를 **덧붙이고**, 누구도 남의 단계를 고치지 않는다.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Timeline {
    pub spans: Vec<Span>,
}

impl Timeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, stage: Stage, dur: std::time::Duration) -> &mut Span {
        self.spans.push(Span {
            stage,
            dur,
            rows: None,
            bytes: None,
            note: None,
        });
        self.spans.last_mut().expect("just pushed")
    }

    pub fn add(&mut self, span: Span) {
        self.spans.push(span);
    }

    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// 같은 단계가 여러 번이면 합산.
    pub fn get(&self, stage: Stage) -> std::time::Duration {
        self.spans
            .iter()
            .filter(|s| s.stage == stage)
            .map(|s| s.dur)
            .sum()
    }

    pub fn total(&self) -> std::time::Duration {
        self.spans.iter().map(|s| s.dur).sum()
    }

    /// 한 줄 요약 — `execute 12.3ms · fetch 340.1ms (500 rows) · output 3.2ms (12 lines) · total 357ms`.
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = Vec::with_capacity(self.spans.len() + 1);
        for s in &self.spans {
            let mut p = format!("{} {}", s.stage.label(), fmt_dur(s.dur));
            if let Some(r) = s.rows {
                p.push_str(&format!(
                    " ({r} {})",
                    if s.stage == Stage::OutputFlush {
                        "lines"
                    } else {
                        "rows"
                    }
                ));
            }
            if let Some(b) = s.bytes {
                p.push_str(&format!(" {}", fmt_bytes(b)));
            }
            parts.push(p);
        }
        if self.spans.len() > 1 {
            parts.push(format!("total {}", fmt_dur(self.total())));
        }
        parts.join(" · ")
    }
}

/// `1.234s` / `12.3ms` / `456µs`.
pub fn fmt_dur(d: std::time::Duration) -> String {
    let us = d.as_micros();
    if us >= 1_000_000 {
        format!("{:.3}s", d.as_secs_f64())
    } else if us >= 1_000 {
        format!("{:.1}ms", us as f64 / 1000.0)
    } else {
        format!("{us}µs")
    }
}

/// `1.2 MB` / `345 KB` / `12 B`.
pub fn fmt_bytes(b: u64) -> String {
    if b >= 1 << 20 {
        format!("{:.1} MB", b as f64 / (1u64 << 20) as f64)
    } else if b >= 1 << 10 {
        format!("{} KB", b >> 10)
    } else {
        format!("{b} B")
    }
}

impl Value {
    /// 호스트 메모리 추정(바이트) — 결과 탑재 예산·클렌징 판정용(정밀 회계가 아니라 자릿수).
    pub fn approx_bytes(&self) -> u64 {
        let inline = std::mem::size_of::<Value>() as u64;
        inline
            + match self {
                Value::Decimal(s) | Value::Str(s) => s.capacity() as u64,
                Value::Bytes(b) => b.capacity() as u64,
                _ => 0,
            }
    }
}

impl ResultSet {
    /// 행·셀의 메모리 추정(바이트).
    pub fn approx_bytes(&self) -> u64 {
        let rows_overhead = (self.rows.capacity() * std::mem::size_of::<Vec<Value>>()) as u64;
        rows_overhead
            + self
                .rows
                .iter()
                .map(|r| r.iter().map(Value::approx_bytes).sum::<u64>())
                .sum::<u64>()
    }
}

/// 드라이버 오류.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DbError {
    pub code: Option<i64>,
    pub message: String,
    /// 문장 내 오류 위치(바이트) — 있으면 편집기가 표시.
    pub position: Option<usize>,
}

impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            Some(c) => write!(f, "[{c}] {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for DbError {}

/// 세션 포트 — 드라이버 크레이트가 구현한다. 엔진은 이 트레이트만 안다.
/// 서버 메시지 실시간 싱크(드라이버 스레드에서 불린다 — 호스트는 채널로 넘겨 UI/stdout에).
pub type MessageSink = std::sync::Arc<dyn Fn(String) + Send + Sync>;

pub trait Session {
    fn dialect(&self) -> Dialect;
    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError>;
    /// 서버 커서를 1회 소비해 결과 집합으로(REFCURSOR `PRINT`).
    fn fetch_cursor(&mut self, cursor: CursorId) -> Result<ResultSet, DbError>;
    fn commit(&mut self) -> Result<(), DbError>;
    fn rollback(&mut self) -> Result<(), DbError>;
    /// 세션 옵션(`serveroutput` = on/off · `fetch_size` = n …). 모르는 옵션은 무시한다.
    fn set_option(&mut self, name: &str, value: &str) -> Result<(), DbError> {
        let _ = (name, value);
        Ok(())
    }
    /// ★ 서버 메시지 실시간 싱크(사용자 09-15 "실행 중 로그") — 설정되면 드라이버는 `PRINT`/`RAISERROR … WITH NOWAIT`(SQL Server) ·
    /// `RAISE NOTICE`(PostgreSQL)를 **도착 즉시** 싱크로 보내고 `ExecResult::messages`에는 넣지 않는다. 없으면 실행 끝에 messages로.
    /// Oracle `DBMS_OUTPUT`은 호출이 끝나야 읽히므로(서버 제약) 항상 실행 뒤 messages — 실시간은 로그 테이블/V$SESSION 폴링(docs/32).
    fn set_message_sink(&mut self, sink: Option<MessageSink>) {
        let _ = sink;
    }
    /// 접속 설명(상태줄).
    fn describe(&self) -> String {
        self.dialect().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_names_round_trip() {
        for d in [
            Dialect::Oracle,
            Dialect::Mssql,
            Dialect::Postgres,
            Dialect::Mysql,
            Dialect::Sqlite,
            Dialect::Odbc,
        ] {
            assert_eq!(Dialect::from_name(&d.to_string()), Some(d));
        }
        assert_eq!(Dialect::from_name("Tibero"), Some(Dialect::Odbc));
        assert_eq!(Dialect::from_name("nope"), None);
    }

    #[test]
    fn var_type_parse_variants() {
        assert_eq!(VarType::parse("NUMBER"), Some(VarType::Number));
        assert_eq!(VarType::parse("varchar2(30)"), Some(VarType::Varchar2(30)));
        assert_eq!(
            VarType::parse("VARCHAR2(30 CHAR)"),
            Some(VarType::Varchar2(30))
        );
        assert_eq!(VarType::parse("VARCHAR2"), Some(VarType::Varchar2(4000)));
        assert_eq!(VarType::parse("REFCURSOR"), Some(VarType::RefCursor));
        assert_eq!(VarType::parse("CHAR"), Some(VarType::Char(1)));
        assert_eq!(VarType::parse("BLOB"), None);
    }

    #[test]
    fn literals_escape_and_prefix() {
        assert_eq!(
            Value::Str("it's".into()).to_sql_literal(Dialect::Oracle),
            "'it''s'"
        );
        assert_eq!(
            Value::Str("x".into()).to_sql_literal(Dialect::Mssql),
            "N'x'"
        );
        assert_eq!(
            Value::Bytes(vec![0xAB, 1]).to_sql_literal(Dialect::Mssql),
            "0xAB01"
        );
        assert_eq!(Value::Null.to_sql_literal(Dialect::Postgres), "NULL");
        assert_eq!(
            Value::Decimal("12345678901234567890.5".into()).to_sql_literal(Dialect::Oracle),
            "12345678901234567890.5"
        );
    }

    #[test]
    fn tsql_types() {
        assert_eq!(VarType::Varchar2(30).tsql_type(), "NVARCHAR(30)");
        assert_eq!(VarType::Varchar2(8000).tsql_type(), "NVARCHAR(MAX)");
        assert_eq!(VarType::Number.tsql_type(), "DECIMAL(38,10)");
    }
}
