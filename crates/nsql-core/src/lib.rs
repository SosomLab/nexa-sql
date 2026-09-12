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
            VarType::Auto => "SQL_VARIANT".into(),
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
