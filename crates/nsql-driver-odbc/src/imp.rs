//! ODBC 어댑터 본문(Windows 전용 · 문서는 `lib.rs`).
use nsql_core::{
    Caps, Column, CursorId, DbError, Dialect, DriverLimit, ExecRequest, ExecResult, ResultSet,
    Session, Value,
};
use nsql_script::ConnectSpec;
use odbc_api::buffers::{AnyColumnBufferSlice, BufferDesc, ColumnarDynBuffer};
use odbc_api::parameter::InputParameter;
use odbc_api::{Connection, ConnectionOptions, Cursor, DataType, IntoParameter};

/// 블록 페치 행 수(열 버퍼 크기 = 행 수 × 열 폭).
const FETCH_ROWS: usize = 256;
/// 문자열 열 한 칸의 상한(UTF-16 단위 · 드라이버가 폭을 모르거나 더 크다고 할 때 = 이 값 · 넘치면 잘림 오류 → 1차 제한).
const MAX_TEXT_CHARS: usize = 32 * 1024;
/// 이진 열 한 칸의 상한(바이트).
const MAX_BIN_BYTES: usize = 64 * 1024;
/// 로그인 상한(초 · `SQL_ATTR_LOGIN_TIMEOUT`).
const LOGIN_TIMEOUT_SECS: u32 = 15;

/// 스펙 → ODBC 접속 문자열(순수 · 비밀번호 포함 · 로그에 적지 말 것).
#[must_use]
pub fn connection_string(spec: &ConnectSpec) -> String {
    let db = spec.database.as_deref().unwrap_or("").trim();
    let mut s = if db.contains('=') {
        db.trim_end_matches(';').to_string()
    } else {
        format!("DSN={}", odbc_api::escape_attribute_value(db))
    };
    let has = |key: &str| -> bool {
        s.split(';')
            .any(|kv| kv.trim_start().to_ascii_uppercase().starts_with(key))
    };
    let (has_uid, has_pwd) = (has("UID="), has("PWD="));
    if let Some(u) = spec.user.as_deref().filter(|u| !u.is_empty()) {
        if !has_uid {
            s.push_str(&format!(";UID={}", odbc_api::escape_attribute_value(u)));
        }
    }
    if let Some(p) = spec.password.as_deref() {
        if !has_pwd {
            s.push_str(&format!(";PWD={}", odbc_api::escape_attribute_value(p)));
        }
    }
    s
}

#[allow(missing_debug_implementations)]
pub struct OdbcSession {
    conn: Connection<'static>,
    description: String,
    dbms: String,
    max_rows: usize,
}

fn err(e: odbc_api::Error) -> DbError {
    match e {
        odbc_api::Error::Diagnostics { record, .. } => DbError {
            code: Some(i64::from(record.native_error)),
            message: record.to_string(),
            position: None,
        },
        other => DbError {
            code: None,
            message: other.to_string(),
            position: None,
        },
    }
}

fn to_param(v: &Value) -> Box<dyn InputParameter> {
    match v {
        Value::Null | Value::Cursor(_) => Box::new(Option::<String>::None.into_parameter()),
        Value::Int(i) => Box::new(*i),
        Value::Float(f) => Box::new(*f),
        Value::Bool(b) => Box::new(odbc_api::Bit(u8::from(*b))),
        Value::Bytes(b) => Box::new(b.clone().into_parameter()),
        Value::Decimal(s) | Value::Str(s) => Box::new(s.clone().into_parameter()),
    }
}

/// 열의 C 버퍼 종류(값 복원 규칙).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    I64,
    F64,
    Bit,
    Bin,
    Text,
    Decimal,
}

/// 열 타입 → (버퍼 서술, 복원 종류). `display` = 드라이버가 알려 준 표시 폭(모르면 상한).
fn buffer_for(dt: &DataType, display: Option<usize>) -> (BufferDesc, Kind) {
    let text = |n: Option<usize>| BufferDesc::WText {
        max_str_len: n.unwrap_or(MAX_TEXT_CHARS).clamp(1, MAX_TEXT_CHARS),
    };
    match dt {
        DataType::Integer | DataType::SmallInt | DataType::TinyInt | DataType::BigInt => {
            (BufferDesc::I64 { nullable: true }, Kind::I64)
        }
        DataType::Real | DataType::Double | DataType::Float { .. } => {
            (BufferDesc::F64 { nullable: true }, Kind::F64)
        }
        DataType::Bit => (BufferDesc::Bit { nullable: true }, Kind::Bit),
        DataType::Binary { length }
        | DataType::Varbinary { length }
        | DataType::LongVarbinary { length } => (
            BufferDesc::Binary {
                max_bytes: length
                    .map_or(MAX_BIN_BYTES, |n| n.get())
                    .clamp(1, MAX_BIN_BYTES),
            },
            Kind::Bin,
        ),
        DataType::Decimal { precision, .. } | DataType::Numeric { precision, .. } => (
            BufferDesc::WText {
                max_str_len: (*precision + 3).clamp(8, 128),
            },
            Kind::Decimal,
        ),
        _ => (
            text(dt.display_size().map(|n| n.get()).or(display)),
            Kind::Text,
        ),
    }
}

/// 드라이버 원 타입 이름(표시·복사용).
fn type_name(dt: &DataType) -> String {
    let len = |n: &Option<std::num::NonZeroUsize>| n.map_or(String::new(), |l| format!("({l})"));
    match dt {
        DataType::Char { length } => format!("CHAR{}", len(length)),
        DataType::WChar { length } => format!("NCHAR{}", len(length)),
        DataType::Varchar { length } => format!("VARCHAR{}", len(length)),
        DataType::WVarchar { length } => format!("NVARCHAR{}", len(length)),
        DataType::LongVarchar { .. } => "TEXT".into(),
        DataType::WLongVarchar { .. } => "NTEXT".into(),
        DataType::Numeric { precision, scale } => format!("NUMERIC({precision},{scale})"),
        DataType::Decimal { precision, scale } => format!("DECIMAL({precision},{scale})"),
        DataType::Integer => "INTEGER".into(),
        DataType::SmallInt => "SMALLINT".into(),
        DataType::TinyInt => "TINYINT".into(),
        DataType::BigInt => "BIGINT".into(),
        DataType::Float { precision } => format!("FLOAT({precision})"),
        DataType::Real => "REAL".into(),
        DataType::Double => "DOUBLE".into(),
        DataType::Date => "DATE".into(),
        DataType::Time { .. } => "TIME".into(),
        DataType::Timestamp { .. } => "TIMESTAMP".into(),
        DataType::Bit => "BIT".into(),
        DataType::Varbinary { length } => format!("VARBINARY{}", len(length)),
        DataType::Binary { length } => format!("BINARY{}", len(length)),
        DataType::LongVarbinary { .. } => "LONGVARBINARY".into(),
        DataType::Other { data_type, .. } => format!("{data_type:?}"),
        _ => "UNKNOWN".into(),
    }
}

/// 블록 한 칸 → 값.
fn cell(col: AnyColumnBufferSlice<'_>, kind: Kind, r: usize) -> Value {
    match kind {
        Kind::I64 => col
            .as_nullable_slice::<i64>()
            .and_then(|s| s.get(r).copied())
            .map_or(Value::Null, Value::Int),
        Kind::F64 => col
            .as_nullable_slice::<f64>()
            .and_then(|s| s.get(r).copied())
            .map_or(Value::Null, Value::Float),
        Kind::Bit => col
            .as_nullable_slice::<odbc_api::Bit>()
            .and_then(|s| s.get(r).map(|b| b.as_bool()))
            .map_or(Value::Null, Value::Bool),
        Kind::Bin => col
            .as_binary()
            .and_then(|s| s.get(r).map(<[u8]>::to_vec))
            .map_or(Value::Null, Value::Bytes),
        Kind::Text | Kind::Decimal => col
            .as_wide_text()
            .and_then(|s| s.get(r).map(String::from_utf16_lossy))
            .map_or(Value::Null, |t| {
                if kind == Kind::Decimal {
                    Value::Decimal(t.trim().to_string())
                } else {
                    Value::Str(t)
                }
            }),
    }
}

/// 커서 하나를 끝까지(또는 `max_rows`까지) 읽어 결과 집합으로 · 커서는 돌려준다(`more_results`용).
fn fetch_all<C: Cursor>(mut cur: C, max_rows: usize) -> Result<(C, ResultSet), DbError> {
    let n = cur.num_result_cols().map_err(err)?.max(0) as u16;
    let mut columns = Vec::with_capacity(n as usize);
    let mut descs = Vec::with_capacity(n as usize);
    let mut kinds = Vec::with_capacity(n as usize);
    for i in 1..=n {
        let name = cur.col_name(i).map_err(err)?;
        let dt = cur.col_data_type(i).map_err(err)?;
        let display = cur.col_display_size(i).ok().flatten().map(|x| x.get());
        let (desc, kind) = buffer_for(&dt, display);
        columns.push(Column {
            name,
            type_name: type_name(&dt),
        });
        descs.push(desc);
        kinds.push(kind);
    }
    let buf = ColumnarDynBuffer::from_descs(FETCH_ROWS, descs);
    let mut block = cur.bind_buffer(buf).map_err(err)?;
    let mut rows: Vec<Vec<Value>> = Vec::new();
    'outer: while let Some(batch) = block.fetch_with_truncation_check(true).map_err(err)? {
        for r in 0..batch.num_rows() {
            let mut row = Vec::with_capacity(kinds.len());
            for (c, kind) in kinds.iter().enumerate() {
                row.push(cell(batch.column(c), *kind, r));
            }
            rows.push(row);
            if max_rows > 0 && rows.len() >= max_rows {
                break 'outer;
            }
        }
    }
    let (cur, _) = block.unbind().map_err(err)?;
    Ok((cur, ResultSet { columns, rows }))
}

impl OdbcSession {
    /// `odbc:DSN=…` / `odbc:Driver={…};…` — [`connection_string`] 규칙 · 로그인 상한 15 s · 자동 커밋 끔.
    pub fn connect(spec: &ConnectSpec) -> Result<Self, DbError> {
        let cs = connection_string(spec);
        let env = odbc_api::environment().map_err(err)?;
        let conn = env
            .connect_with_connection_string(
                &cs,
                ConnectionOptions {
                    login_timeout_sec: Some(LOGIN_TIMEOUT_SECS),
                    ..ConnectionOptions::default()
                },
            )
            .map_err(err)?;
        conn.set_autocommit(false).map_err(err)?;
        let dbms = conn.database_management_system_name().unwrap_or_default();
        Ok(OdbcSession {
            conn,
            description: spec.redacted(),
            dbms,
            max_rows: 0,
        })
    }

    fn execute_inner(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let params: Vec<Box<dyn InputParameter>> =
            req.params.iter().map(|p| to_param(&p.value)).collect();
        let mut stmt = self.conn.preallocate().map_err(err)?;
        let mut result = ExecResult::default();
        let mut first = stmt.execute(&req.sql, &params[..]).map_err(err)?;
        if first.is_none() {
            drop(first);
            result.rows_affected = stmt.row_count().map_err(err)?.map(|n| n as u64);
            return Ok(result);
        }
        let mut cur = first.take().expect("checked above");
        loop {
            let (c, rs) = fetch_all(cur, self.max_rows)?;
            result.result_sets.push(rs);
            match c.more_results().map_err(err)? {
                Some(next) => cur = next,
                None => break,
            }
        }
        Ok(result)
    }
}

impl Session for OdbcSession {
    fn dialect(&self) -> Dialect {
        Dialect::Odbc
    }

    fn caps(&self) -> Caps {
        Caps::of(Dialect::Odbc)
    }

    fn driver_name(&self) -> String {
        if self.dbms.is_empty() {
            "odbc-api 29".to_string()
        } else {
            format!("odbc-api 29 · {}", self.dbms)
        }
    }

    fn limits(&self) -> &'static [DriverLimit] {
        &[DriverLimit::Cancel]
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        self.execute_inner(req)
    }

    fn fetch_cursor(&mut self, _cursor: CursorId) -> Result<ResultSet, DbError> {
        Err(DbError {
            code: None,
            message: "ODBC: server cursors are not supported".into(),
            position: None,
        })
    }

    fn commit(&mut self) -> Result<(), DbError> {
        self.conn.commit().map_err(err)
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        self.conn.rollback().map_err(err)
    }

    fn set_option(&mut self, name: &str, value: &str) -> Result<(), DbError> {
        if name == "max_rows" {
            self.max_rows = value.parse().unwrap_or(0);
        }
        Ok(())
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn is_alive(&self) -> bool {
        self.conn.is_dead().map(|dead| !dead).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 접속 문자열(순수): DSN 이름 → `DSN=` + UID/PWD · 전체 문자열(`=` 포함)은 그대로 + 빠진 UID/PWD만 · 특수 문자는 `{}`로.
    #[test]
    fn connection_string_from_dsn_or_full_string() {
        let dsn = ConnectSpec {
            dialect: Some(Dialect::Odbc),
            database: Some("Sales".into()),
            user: Some("scott".into()),
            password: Some("ti;ger".into()),
            ..ConnectSpec::default()
        };
        assert_eq!(connection_string(&dsn), "DSN=Sales;UID=scott;PWD={ti;ger}");
        let full = ConnectSpec {
            dialect: Some(Dialect::Odbc),
            database: Some(
                "Driver={PostgreSQL Unicode};Server=h;Port=5432;Database=d;Uid=u;Pwd=p;".into(),
            ),
            user: Some("ignored".into()),
            password: Some("ignored".into()),
            ..ConnectSpec::default()
        };
        assert_eq!(
            connection_string(&full),
            "Driver={PostgreSQL Unicode};Server=h;Port=5432;Database=d;Uid=u;Pwd=p"
        );
        let no_pw = ConnectSpec {
            dialect: Some(Dialect::Odbc),
            database: Some("Driver={SQL Server};Server=h".into()),
            user: Some("sa".into()),
            ..ConnectSpec::default()
        };
        assert_eq!(
            connection_string(&no_pw),
            "Driver={SQL Server};Server=h;UID=sa"
        );
    }

    /// 열 타입 → 버퍼 종류·이름.
    #[test]
    fn buffer_kinds_and_type_names() {
        let (_, k) = buffer_for(&DataType::BigInt, None);
        assert_eq!(k, Kind::I64);
        let (_, k) = buffer_for(&DataType::Double, None);
        assert_eq!(k, Kind::F64);
        let (d, k) = buffer_for(
            &DataType::Decimal {
                precision: 10,
                scale: 2,
            },
            None,
        );
        assert_eq!(k, Kind::Decimal);
        assert!(matches!(d, BufferDesc::WText { max_str_len: 13 }));
        let (d, k) = buffer_for(&DataType::Varchar { length: None }, Some(40));
        assert_eq!(k, Kind::Text);
        assert!(matches!(d, BufferDesc::WText { max_str_len: 40 }));
        let (d, _) = buffer_for(&DataType::WLongVarchar { length: None }, None);
        assert!(matches!(
            d,
            BufferDesc::WText {
                max_str_len: MAX_TEXT_CHARS
            }
        ));
        assert_eq!(type_name(&DataType::Integer), "INTEGER");
        assert_eq!(
            type_name(&DataType::WVarchar {
                length: std::num::NonZeroUsize::new(50)
            }),
            "NVARCHAR(50)"
        );
    }
}
