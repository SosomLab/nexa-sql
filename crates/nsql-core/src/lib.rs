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
use std::sync::Arc;

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

/// 로그인 항목(접속 폼의 칸과 1:1 · `Dialect::login_required`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginField {
    Host,
    Port,
    Database,
    Schema,
    User,
    Password,
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

    /// ★ 접속에 **필수인 로그인 항목**(docs/22 §10 · 09-19) — 접속 폼(라벨 `*` · 빠진 칸 경고)·CLI 인자 검사·확장 드라이버
    /// 매니페스트가 같은 표를 본다. Save는 여기서 Password를 뺀 것(+ 프로필 이름). Port·Schema는 늘 선택(기본값/없음).
    pub fn login_required(self) -> &'static [LoginField] {
        match self {
            // 서비스/SID 없이는 접속 불가 · OS 인증(`/`)은 미지원.
            Dialect::Oracle => &[
                LoginField::Host,
                LoginField::Database,
                LoginField::User,
                LoginField::Password,
            ],
            // Database를 비우면 libpq 규칙(= 사용자 이름) · 로그인 기본 DB — 선택.
            Dialect::Postgres | Dialect::Mssql | Dialect::Mysql => {
                &[LoginField::Host, LoginField::User, LoginField::Password]
            }
            // 파일 경로 / DSN 하나(자격 없음 · ODBC 자격은 DSN 쪽).
            Dialect::Sqlite | Dialect::Odbc => &[LoginField::Database],
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

/// 결과 집합 한 덩어리 — 드라이버·러너·CLI가 주고받는 **행 단위 `Vec<Value>`**(DR-33 확정 · 컬럼형/아레나는 T-102 후보).
/// GUI는 페치 단위로 받은 덩어리를 [`ResultData`]에 세그먼트로 덧붙여 한 세트로 든다. 렌더러는 [`RowSource`]로만 읽는다.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ResultSet {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
}

/// 열린 서버 커서(추가 페치용 · docs/43 §3-2) — 세션당 **1개**. 드라이버가 실행마다 새 번호를 매기므로
/// 앞 커서가 닫힌 뒤 옛 핸들로 `fetch_next`하면 오류가 난다(호스트는 OFFSET 재질의로 폴백).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct CursorHandle(pub u32);

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
    /// **마지막 결과 집합이 페치 상한에서 잘렸고** 드라이버가 서버 커서를 열어 둔 경우만 `Some` —
    /// 호스트는 [`Session::fetch_next`]로 이어 받는다(docs/43 D-70). 커서를 못 여는 드라이버는 항상 `None`.
    pub pending: Option<CursorHandle>,
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
        rows_approx_bytes(&self.rows)
    }
}

fn rows_approx_bytes(rows: &[Vec<Value>]) -> u64 {
    let rows_overhead = std::mem::size_of_val(rows) as u64;
    rows_overhead
        + rows
            .iter()
            .map(|r| r.iter().map(Value::approx_bytes).sum::<u64>())
            .sum::<u64>()
}

// ────────────────────────────────────────────── 트랜잭션 상태 분류(docs/44 §5 · 사용자 09-17)

/// 문장이 트랜잭션에 남기는 흔적의 종류 — 툴바 트랜잭션 버튼 색·배지, 트랜잭션 로그 Tx 열의 근거.
/// 심각도(잃는 것의 크기) 순: `DdlDrop` > `Delete`/`Truncate` > `Update` > `Insert` > `DdlCreate`/`DdlAlter` > `Read` > `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TxClass {
    /// 아무것도 없음.
    None,
    /// 조회만(SELECT/WITH/SHOW/DESC/EXPLAIN) — 수동 모드에서는 스냅샷·잠금을 쥔 읽기 트랜잭션.
    Read,
    /// 오브젝트 추가·변경(CREATE/ALTER/COMMENT/GRANT …) — 트랜잭션 DDL 방언(PG·SQL Server·SQLite)에서만 대기 상태가 된다.
    DdlCreate,
    /// 행 추가.
    Insert,
    /// 열 값 변경(UPDATE · MERGE).
    Update,
    /// 행 삭제.
    Delete,
    /// TRUNCATE — 롤백 가능 방언(PG·SQL Server)에서만 트랜잭션 대상 · Oracle/MySQL은 암묵 커밋(대상 아님).
    Truncate,
    /// 오브젝트 삭제(DROP).
    DdlDrop,
    /// 분류 밖(CALL·EXEC·PL 블록·SET …) — 영향 행이 있으면 갱신으로 취급.
    Other,
}

impl TxClass {
    /// 문장 첫 키워드로 분류(주석·공백 무시 · 대소문자 무시). CTE(`WITH`)는 조회로 본다.
    #[must_use]
    pub fn of_sql(sql: &str) -> TxClass {
        let w = first_keyword(sql);
        match w.as_str() {
            "" => TxClass::None,
            "SELECT" | "WITH" | "SHOW" | "DESC" | "DESCRIBE" | "EXPLAIN" | "VALUES" | "TABLE" => {
                TxClass::Read
            }
            "INSERT" | "REPLACE" | "COPY" | "IMPORT" => TxClass::Insert,
            "UPDATE" | "MERGE" | "UPSERT" => TxClass::Update,
            "DELETE" => TxClass::Delete,
            "TRUNCATE" => TxClass::Truncate,
            "DROP" => TxClass::DdlDrop,
            "CREATE" | "ALTER" | "COMMENT" | "GRANT" | "REVOKE" | "RENAME" => TxClass::DdlCreate,
            _ => TxClass::Other,
        }
    }

    /// 심각도(잃는 것의 크기 · 버튼 색은 가장 큰 것을 따른다).
    #[must_use]
    pub const fn severity(self) -> u8 {
        match self {
            TxClass::None => 0,
            TxClass::Read => 1,
            TxClass::DdlCreate => 2,
            TxClass::Insert => 3,
            TxClass::Other => 3,
            TxClass::Update => 4,
            TxClass::Delete | TxClass::Truncate => 5,
            TxClass::DdlDrop => 6,
        }
    }

    /// DDL(오브젝트) 종류인가.
    #[must_use]
    pub const fn is_ddl(self) -> bool {
        matches!(
            self,
            TxClass::DdlCreate | TxClass::DdlDrop | TxClass::Truncate
        )
    }
}

/// 사용자가 **직접** 트랜잭션을 통제한 문장인가(docs/56 L1 — 읽기 트랜잭션 자동 종료의 예외·초기화 근거).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TxControl {
    /// 통제 문장 아님.
    None,
    /// 트랜잭션을 열거나 붙잡는다: `BEGIN [TRANSACTION|WORK|TRAN]` · `START TRANSACTION` · `SET TRANSACTION` · `SAVEPOINT` · `LOCK …`.
    Begin,
    /// 트랜잭션을 끝낸다: `COMMIT` · `ROLLBACK`(`ROLLBACK TO …` 제외) · `END` · `ABORT`.
    End,
}

impl TxControl {
    /// 문장 앞 두 단어로 판정(주석·대소문자 무시). PL/SQL 익명 블록 `BEGIN … END;`은 통제 문장이 아니다(둘째 단어로 구분).
    #[must_use]
    pub fn of_sql(sql: &str) -> TxControl {
        let up = strip_leading_comments(sql).to_ascii_uppercase();
        let mut words = up
            .split(|c: char| c.is_whitespace() || c == ';')
            .filter(|w| !w.is_empty());
        let (w1, w2) = (words.next().unwrap_or(""), words.next().unwrap_or(""));
        match (w1, w2) {
            (
                "BEGIN",
                "" | "TRANSACTION" | "TRAN" | "WORK" | "ISOLATION" | "READ" | "DEFERRED"
                | "IMMEDIATE" | "EXCLUSIVE",
            )
            | ("START", "TRANSACTION")
            | ("SET", "TRANSACTION")
            | ("SAVEPOINT", _)
            | ("SAVE", "TRANSACTION" | "TRAN")
            | ("LOCK", _) => TxControl::Begin,
            ("ROLLBACK", "TO") => TxControl::None,
            ("COMMIT" | "ROLLBACK" | "ABORT", _) | ("END", "" | "TRANSACTION" | "WORK") => {
                TxControl::End
            }
            _ => TxControl::None,
        }
    }
}

/// 잠금이 목적인 조회인가 — `FOR UPDATE/SHARE` · `LOCK IN SHARE MODE` · SQL Server 잠금 힌트. 트랜잭션을 끝내면 뜻이 사라지므로
/// 읽기 트랜잭션 자동 종료에서 제외한다(오탐은 "안 끝냄"이라 안전 쪽).
#[must_use]
pub fn is_locking_read(sql: &str) -> bool {
    let up: String = sql
        .to_ascii_uppercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    [
        " FOR UPDATE",
        " FOR SHARE",
        " FOR NO KEY UPDATE",
        " FOR KEY SHARE",
        "LOCK IN SHARE MODE",
        "UPDLOCK",
        "HOLDLOCK",
        "XLOCK",
        "TABLOCKX",
    ]
    .iter()
    .any(|k| up.contains(k))
}

fn strip_leading_comments(sql: &str) -> &str {
    let mut rest = sql.trim_start();
    loop {
        if let Some(r) = rest.strip_prefix("--") {
            rest = r.split_once('\n').map_or("", |(_, t)| t).trim_start();
        } else if let Some(r) = rest.strip_prefix("/*") {
            rest = r.split_once("*/").map_or("", |(_, t)| t).trim_start();
        } else {
            return rest;
        }
    }
}

/// 문장 첫 키워드(앞 주석 `--`·`/* */`·공백 건너뜀 · 대문자).
#[must_use]
pub fn first_keyword(sql: &str) -> String {
    let mut rest = sql.trim_start();
    loop {
        if let Some(r) = rest.strip_prefix("--") {
            rest = r.split_once('\n').map_or("", |(_, t)| t).trim_start();
        } else if let Some(r) = rest.strip_prefix("/*") {
            rest = r.split_once("*/").map_or("", |(_, t)| t).trim_start();
        } else {
            break;
        }
    }
    rest.split(|c: char| !c.is_alphanumeric() && c != '_')
        .find(|w| !w.is_empty())
        .map(|w| w.to_ascii_uppercase())
        .unwrap_or_default()
}

impl Dialect {
    /// DDL이 트랜잭션 안에 남는가(롤백 가능) — PG·SQL Server·SQLite = 예 · Oracle·MySQL = 암묵 커밋 · ODBC = 모름(보수적으로 예).
    #[must_use]
    pub const fn ddl_transactional(self) -> bool {
        !matches!(self, Dialect::Oracle | Dialect::Mysql)
    }

    /// TRUNCATE가 롤백 가능한가(docs/44 §5 · 사용자 09-17 "Truncate는 대상이 아니지?") — Oracle·MySQL = 아니오(DDL 암묵 커밋) ·
    /// PG·SQL Server = 예 · SQLite = 문장 없음(DELETE 최적화 · 트랜잭션) · ODBC = 모름(보수적으로 아니오).
    #[must_use]
    pub const fn truncate_transactional(self) -> bool {
        matches!(self, Dialect::Postgres | Dialect::Mssql | Dialect::Sqlite)
    }

    /// 이 문장이 이 방언에서 **암묵 커밋**을 일으키는가(Oracle/MySQL DDL · TRUNCATE 포함).
    #[must_use]
    pub fn implicit_commit(self, sql: &str) -> bool {
        if self.ddl_transactional() {
            return false;
        }
        matches!(
            first_keyword(sql).as_str(),
            "CREATE" | "ALTER" | "DROP" | "TRUNCATE" | "GRANT" | "REVOKE" | "RENAME" | "COMMENT"
        )
    }
}

// ────────────────────────────────────────────── ★ 결과 데이터 1세트 + 뷰 투영(DR-33 · 사용자 09-17)
//
// 원칙: 조회 결과는 **한 세트**만 들고(형식별 사본 0), 그리드·텍스트 보기·복사·CLI는 전부 `RowSource` 포트 하나로 읽는다.
// - `ResultSet`  = 한 덩어리(드라이버·러너·CLI).
// - `ResultData` = GUI의 한 세트: 페치 단위 세그먼트를 `Arc`로 들어 **덧붙이기 복사 0 · 변환 스레드 공유 복사 0**.
// - `View`       = 행 순서(정렬·선택)·열 순서/부분집합의 **인덱스 투영**(복사 0). 뷰 위의 뷰도 된다.

static NULL_VALUE: Value = Value::Null;

/// 행 원천 포트 — 렌더러(nsql-io) · 그리드 페인트 · 복사 · 정렬이 이 포트로만 값을 읽는다.
pub trait RowSource {
    fn ncols(&self) -> usize;
    fn column(&self, c: usize) -> &Column;
    fn len(&self) -> usize;
    /// 행 `r`의 **저장 행**(열은 [`Self::col_index`]로 푼다 — 투영 뷰는 열 순서를 바꾸므로 직접 인덱싱하지 않는다).
    fn raw_row(&self, r: usize) -> &[Value];
    /// 뷰 열 `c` → 저장 열.
    fn col_index(&self, c: usize) -> usize {
        c
    }
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// 셀(없으면 NULL).
    fn cell(&self, r: usize, c: usize) -> &Value {
        self.raw_row(r)
            .get(self.col_index(c))
            .unwrap_or(&NULL_VALUE)
    }
    /// 행 `r`의 셀을 뷰 열 순서로.
    fn cells(&self, r: usize) -> Cells<'_, Self> {
        Cells {
            src: self,
            row: self.raw_row(r),
            c: 0,
            n: self.ncols(),
        }
    }
    /// 컬럼 이름(뷰 순서).
    fn col_names(&self) -> Vec<String> {
        (0..self.ncols())
            .map(|c| self.column(c).name.clone())
            .collect()
    }
}

/// [`RowSource::cells`] 반복자.
pub struct Cells<'a, S: RowSource + ?Sized> {
    src: &'a S,
    row: &'a [Value],
    c: usize,
    n: usize,
}

impl<'a, S: RowSource + ?Sized> Iterator for Cells<'a, S> {
    type Item = &'a Value;
    fn next(&mut self) -> Option<&'a Value> {
        if self.c >= self.n {
            return None;
        }
        let v = self
            .row
            .get(self.src.col_index(self.c))
            .unwrap_or(&NULL_VALUE);
        self.c += 1;
        Some(v)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.n - self.c;
        (left, Some(left))
    }
}

impl<S: RowSource + ?Sized> ExactSizeIterator for Cells<'_, S> {}

impl<S: RowSource + ?Sized> fmt::Debug for Cells<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cells({}/{})", self.c, self.n)
    }
}

impl RowSource for ResultSet {
    fn ncols(&self) -> usize {
        self.columns.len()
    }
    fn column(&self, c: usize) -> &Column {
        &self.columns[c]
    }
    fn len(&self) -> usize {
        self.rows.len()
    }
    fn raw_row(&self, r: usize) -> &[Value] {
        &self.rows[r]
    }
}

impl<S: RowSource + ?Sized> RowSource for &S {
    fn ncols(&self) -> usize {
        (**self).ncols()
    }
    fn column(&self, c: usize) -> &Column {
        (**self).column(c)
    }
    fn len(&self) -> usize {
        (**self).len()
    }
    fn raw_row(&self, r: usize) -> &[Value] {
        (**self).raw_row(r)
    }
    fn col_index(&self, c: usize) -> usize {
        (**self).col_index(c)
    }
}

/// GUI의 결과 **한 세트** — 페치 단위 세그먼트(`Arc`) 목록. `clone`은 Arc 몇 개 복사(셀 복사 0)라 변환 스레드에 그대로 준다.
/// 행 번호 → 세그먼트는 `starts` 이분 탐색(세그먼트 수 = 페치 횟수 · 수십 개).
#[derive(Clone, Debug, Default)]
pub struct ResultData {
    columns: Arc<Vec<Column>>,
    segs: Vec<Arc<Vec<Vec<Value>>>>,
    /// 세그먼트 i의 첫 행 번호.
    starts: Vec<usize>,
    len: usize,
    bytes: u64,
}

impl ResultData {
    /// 첫 덩어리에서(이동 · 복사 0).
    pub fn new(rs: ResultSet) -> Self {
        let mut d = ResultData {
            columns: Arc::new(rs.columns),
            ..Default::default()
        };
        d.push_rows(rs.rows);
        d
    }

    /// 다음 세그먼트 덧붙이기(이동 · 복사 0). 컬럼은 첫 덩어리 것을 쓴다(빈 세트였으면 이 덩어리 것).
    pub fn push(&mut self, page: ResultSet) {
        if self.columns.is_empty() && !page.columns.is_empty() {
            self.columns = Arc::new(page.columns);
        }
        self.push_rows(page.rows);
    }

    fn push_rows(&mut self, rows: Vec<Vec<Value>>) {
        if rows.is_empty() {
            return;
        }
        self.bytes += rows_approx_bytes(&rows);
        self.starts.push(self.len);
        self.len += rows.len();
        self.segs.push(Arc::new(rows));
    }

    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn segments(&self) -> usize {
        self.segs.len()
    }

    /// 행·셀의 메모리 추정(덧붙일 때 누적 · 매번 훑지 않는다).
    pub fn approx_bytes(&self) -> u64 {
        self.bytes
    }

    fn locate(&self, r: usize) -> Option<(usize, usize)> {
        if r >= self.len {
            return None;
        }
        let i = self.starts.partition_point(|&s| s <= r).saturating_sub(1);
        Some((i, r - self.starts[i]))
    }

    /// 행 `r`(없으면 None).
    pub fn row(&self, r: usize) -> Option<&[Value]> {
        let (i, off) = self.locate(r)?;
        self.segs[i].get(off).map(Vec::as_slice)
    }

    /// 전 행 순서대로(세그먼트 경계 없이).
    pub fn rows(&self) -> impl Iterator<Item = &[Value]> + '_ {
        self.segs.iter().flat_map(|s| s.iter().map(Vec::as_slice))
    }
}

impl RowSource for ResultData {
    fn ncols(&self) -> usize {
        self.columns.len()
    }
    fn column(&self, c: usize) -> &Column {
        &self.columns[c]
    }
    fn len(&self) -> usize {
        self.len
    }
    fn raw_row(&self, r: usize) -> &[Value] {
        self.row(r).unwrap_or(&[])
    }
}

/// 인덱스 투영 — 행 순서(정렬·선택 행) · 열 순서/부분집합. 원본을 건드리지 않고 복사도 없다. `None` = 원본 그대로.
#[derive(Clone, Copy, Debug)]
pub struct View<'a, S: RowSource + ?Sized> {
    src: &'a S,
    rows: Option<&'a [usize]>,
    cols: Option<&'a [usize]>,
}

impl<'a, S: RowSource + ?Sized> View<'a, S> {
    pub fn new(src: &'a S) -> Self {
        View {
            src,
            rows: None,
            cols: None,
        }
    }
    /// 행 순서(원본 행 번호의 목록 · 부분집합 가능).
    #[must_use]
    pub fn rows(mut self, order: &'a [usize]) -> Self {
        self.rows = Some(order);
        self
    }
    /// 열 순서(원본 열 번호의 목록 · 부분집합 가능).
    #[must_use]
    pub fn cols(mut self, order: &'a [usize]) -> Self {
        self.cols = Some(order);
        self
    }
    /// 뷰 행 `r`의 원본 행 번호.
    pub fn source_row(&self, r: usize) -> usize {
        self.rows.map_or(r, |o| o[r])
    }
}

impl<S: RowSource + ?Sized> RowSource for View<'_, S> {
    fn ncols(&self) -> usize {
        self.cols.map_or_else(|| self.src.ncols(), <[usize]>::len)
    }
    fn column(&self, c: usize) -> &Column {
        self.src.column(self.cols.map_or(c, |o| o[c]))
    }
    fn len(&self) -> usize {
        self.rows.map_or_else(|| self.src.len(), <[usize]>::len)
    }
    fn raw_row(&self, r: usize) -> &[Value] {
        self.src.raw_row(self.source_row(r))
    }
    fn col_index(&self, c: usize) -> usize {
        self.src.col_index(self.cols.map_or(c, |o| o[c]))
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

/// ★ 실행 취소 핸들(T-108 · docs/44 §4) — 워커가 실행에 갇혀 있는 동안 **다른 스레드**(UI)에서 부른다.
/// 드라이버가 지원할 때만 있다: SQLite `InterruptHandle` · PostgreSQL `CancelToken` · Oracle `break_execution` · SQL Server 없음.
pub trait CancelHandle: Send + Sync {
    fn cancel(&self) -> Result<(), DbError>;
    /// 취소가 **세션을 끊는** 방식인가(SQL Server = 소켓 종료 · 열린 트랜잭션은 서버가 롤백 · 다음 실행 때 재접속).
    fn drops_session(&self) -> bool {
        false
    }
}

pub trait Session {
    fn dialect(&self) -> Dialect;
    /// 실행 취소 핸들 — 지원하지 않으면 `None`(호스트는 전체 조회 배치 경계에서만 멈춘다).
    fn cancel_handle(&self) -> Option<std::sync::Arc<dyn CancelHandle>> {
        None
    }
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
    /// 서버 커서 유지가 되는가(docs/43 §3-3). `false`면 호스트가 OFFSET 재질의로 폴백하고 [`ExecResult::pending`]은 늘 `None`.
    fn cursor_supported(&self) -> bool {
        false
    }
    /// 열린 커서에서 다음 `max`행(0 = 끝까지) — `(결과, 더 있음)`. `더 있음 = false`면 드라이버가 커서를 닫은 뒤다.
    /// 옛 핸들(닫힘·교체)이면 `Err`.
    fn fetch_next(&mut self, h: CursorHandle, max: usize) -> Result<(ResultSet, bool), DbError> {
        let _ = max;
        Err(DbError {
            code: None,
            message: format!("cursor #{} is not open", h.0),
            position: None,
        })
    }
    /// 커서를 닫는다(이미 닫혔으면 무시). 새 실행·커밋·롤백·접속 해제 때 호스트/드라이버가 부른다.
    fn close_cursor(&mut self, h: CursorHandle) -> Result<(), DbError> {
        let _ = h;
        Ok(())
    }
    /// 세션이 살아 있다고 **드라이버가 아는가** — 서버 왕복 없이(docs/53 §2): 마지막 네트워크 결과 · 소켓 FIN/RST 인지.
    /// `false`면 끊긴 것이 확실하다 · `true`는 "아직 모른다"일 수 있다(반쯤 열린 소켓). 기본 = 모른다(true).
    fn is_alive(&self) -> bool {
        true
    }
}

pub mod dberr;
pub use dberr::{classify, native_code, Classified, ErrorClass};

#[cfg(test)]
mod tests {
    use super::*;

    /// docs/44 §5: 분류 · 주석 건너뜀 · 심각도 순서 · TRUNCATE/DDL 방언 규칙.
    #[test]
    fn tx_control_and_locking_read() {
        assert_eq!(TxControl::of_sql("begin"), TxControl::Begin);
        assert_eq!(
            TxControl::of_sql("-- c\nBEGIN TRANSACTION;"),
            TxControl::Begin
        );
        assert_eq!(TxControl::of_sql("START TRANSACTION"), TxControl::Begin);
        assert_eq!(
            TxControl::of_sql("set transaction isolation level serializable"),
            TxControl::Begin
        );
        assert_eq!(TxControl::of_sql("SAVEPOINT a"), TxControl::Begin);
        assert_eq!(
            TxControl::of_sql("LOCK TABLE t IN EXCLUSIVE MODE"),
            TxControl::Begin
        );
        assert_eq!(
            TxControl::of_sql("BEGIN dbms_output.put_line('x'); END;"),
            TxControl::None,
            "PL/SQL 블록"
        );
        assert_eq!(TxControl::of_sql("commit"), TxControl::End);
        assert_eq!(TxControl::of_sql("ROLLBACK;"), TxControl::End);
        assert_eq!(TxControl::of_sql("rollback to a"), TxControl::None);
        assert_eq!(TxControl::of_sql("END"), TxControl::End);
        assert_eq!(TxControl::of_sql("select 1"), TxControl::None);
        assert!(is_locking_read("select * from t\n  for update nowait"));
        assert!(is_locking_read("SELECT * FROM t WITH (UPDLOCK, ROWLOCK)"));
        assert!(is_locking_read("select 1 from t lock in share mode"));
        assert!(!is_locking_read("select * from t where a = 1"));
    }

    #[test]
    fn tx_class_and_dialect_rules() {
        assert_eq!(
            TxClass::of_sql("  -- 주석\n/* c */ select 1"),
            TxClass::Read
        );
        assert_eq!(
            TxClass::of_sql("WITH x AS (SELECT 1) SELECT * FROM x"),
            TxClass::Read
        );
        assert_eq!(TxClass::of_sql("insert into t values (1)"), TxClass::Insert);
        assert_eq!(
            TxClass::of_sql("MERGE INTO t USING s ON (1=1)"),
            TxClass::Update
        );
        assert_eq!(TxClass::of_sql("delete from t"), TxClass::Delete);
        assert_eq!(TxClass::of_sql("TRUNCATE TABLE t"), TxClass::Truncate);
        assert_eq!(TxClass::of_sql("drop view v"), TxClass::DdlDrop);
        assert_eq!(
            TxClass::of_sql("create or replace package p as end;"),
            TxClass::DdlCreate
        );
        assert_eq!(TxClass::of_sql("BEGIN NULL; END;"), TxClass::Other);
        assert_eq!(TxClass::of_sql(""), TxClass::None);
        assert!(TxClass::DdlDrop.severity() > TxClass::Delete.severity());
        assert!(TxClass::Delete.severity() > TxClass::Update.severity());
        assert!(TxClass::Update.severity() > TxClass::Insert.severity());
        assert!(TxClass::Insert.severity() > TxClass::DdlCreate.severity());
        assert!(TxClass::DdlCreate.severity() > TxClass::Read.severity());
        assert!(
            !Dialect::Oracle.truncate_transactional() && !Dialect::Mysql.truncate_transactional()
        );
        assert!(
            Dialect::Postgres.truncate_transactional() && Dialect::Mssql.truncate_transactional()
        );
        assert!(Dialect::Oracle.implicit_commit("truncate table t"));
        assert!(!Dialect::Postgres.implicit_commit("truncate table t"));
        assert!(Dialect::Mysql.implicit_commit("CREATE TABLE t(a int)"));
        assert!(!Dialect::Sqlite.implicit_commit("CREATE TABLE t(a int)"));
    }

    fn col(n: &str) -> Column {
        Column {
            name: n.into(),
            type_name: String::new(),
        }
    }

    /// 세그먼트 덧붙이기 = 복사 0(Arc 공유) · 행 번호는 세그먼트 경계를 넘어 이어진다 · 바이트는 누적.
    #[test]
    fn result_data_segments_index_across_pages() {
        let mut d = ResultData::new(ResultSet {
            columns: vec![col("a"), col("b")],
            rows: vec![vec![Value::Int(1), Value::Str("x".into())]],
        });
        d.push(ResultSet {
            columns: vec![],
            rows: vec![
                vec![Value::Int(2), Value::Null],
                vec![Value::Int(3), Value::Str("z".into())],
            ],
        });
        d.push(ResultSet::default()); // 빈 페이지는 세그먼트를 만들지 않는다.
        assert_eq!((d.len(), d.segments(), d.ncols()), (3, 2, 2));
        assert_eq!(d.row(0).unwrap()[0], Value::Int(1));
        assert_eq!(d.row(1).unwrap()[0], Value::Int(2));
        assert_eq!(d.row(2).unwrap()[1], Value::Str("z".into()));
        assert!(d.row(3).is_none());
        assert_eq!(d.rows().count(), 3);
        assert!(d.approx_bytes() > 0);
        let shared = d.clone();
        assert_eq!(shared.len(), 3);
        // 세그먼트 Arc가 공유된다(복사 0).
        assert!(Arc::ptr_eq(&d.segs[0], &shared.segs[0]));
    }

    /// 뷰 = 행·열 인덱스 투영 — 정렬(행 순서)·열 이동·부분집합을 복사 없이, 뷰 위의 뷰도 합성된다.
    #[test]
    fn view_projects_rows_and_cols_without_copy() {
        let rs = ResultSet {
            columns: vec![col("a"), col("b"), col("c")],
            rows: vec![
                vec![Value::Int(1), Value::Int(10), Value::Int(100)],
                vec![Value::Int(2), Value::Int(20), Value::Int(200)],
                vec![Value::Int(3), Value::Int(30), Value::Int(300)],
            ],
        };
        let order = [2usize, 0];
        let cols = [2usize, 0];
        let v = View::new(&rs).rows(&order).cols(&cols);
        assert_eq!((v.len(), v.ncols()), (2, 2));
        assert_eq!(v.column(0).name, "c");
        assert_eq!(v.cell(0, 0), &Value::Int(300));
        assert_eq!(v.cell(1, 1), &Value::Int(1));
        let got: Vec<&Value> = v.cells(0).collect();
        assert_eq!(got, vec![&Value::Int(300), &Value::Int(3)]);
        assert_eq!(v.col_names(), vec!["c", "a"]);
        // 뷰 위의 뷰: 열을 다시 뒤집으면 원래 (a, c) 순서.
        let flip = [1usize, 0];
        let vv = View::new(&v).cols(&flip);
        assert_eq!(vv.col_names(), vec!["a", "c"]);
        assert_eq!(vv.cell(0, 0), &Value::Int(3));
        // 없는 열은 NULL(패닉 없음).
        let wide = [5usize];
        let bad = View::new(&rs).cols(&wide);
        assert_eq!(bad.cell(0, 0), &Value::Null);
    }

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
