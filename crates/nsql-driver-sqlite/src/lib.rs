//! SQLite 어댑터 — `nsql_core::Session` 구현. 방언 `Dialect::Sqlite`(`?` 위치 바인드).
//! OUT 파라미터가 없는 방언이므로 `EXEC :V := expr`는 엔진이 `SELECT expr AS "V"`로 만들고,
//! 호스트(`nsql-run`)가 1행 결과의 컬럼 이름을 변수로 흡수한다.

#![cfg_attr(test, allow(clippy::unwrap_used))]

use nsql_core::{
    Column, CursorId, DbError, Dialect, ExecRequest, ExecResult, ResultSet, Session, Value,
};
use rusqlite::types::{ToSqlOutput, Value as SqlValue, ValueRef};
use rusqlite::{params_from_iter, Connection};

#[allow(missing_debug_implementations)]
pub struct SqliteSession {
    conn: Connection,
}

impl SqliteSession {
    /// `":memory:"` 또는 파일 경로.
    pub fn open(target: &str) -> Result<Self, DbError> {
        let conn = if target.is_empty() || target == ":memory:" {
            Connection::open_in_memory()
        } else {
            Connection::open(target)
        }
        .map_err(err)?;
        Ok(SqliteSession { conn })
    }
}

fn err(e: rusqlite::Error) -> DbError {
    let code = match &e {
        rusqlite::Error::SqliteFailure(f, _) => Some(f.extended_code as i64),
        _ => None,
    };
    DbError {
        code,
        message: e.to_string(),
        position: None,
    }
}

fn to_sql(v: &Value) -> ToSqlOutput<'_> {
    ToSqlOutput::Owned(match v {
        Value::Null => SqlValue::Null,
        Value::Int(i) => SqlValue::Integer(*i),
        Value::Float(f) => SqlValue::Real(*f),
        Value::Decimal(d) => d
            .parse::<f64>()
            .map(SqlValue::Real)
            .unwrap_or_else(|_| SqlValue::Text(d.clone())),
        Value::Str(s) => SqlValue::Text(s.clone()),
        Value::Bool(b) => SqlValue::Integer(i64::from(*b)),
        Value::Bytes(b) => SqlValue::Blob(b.clone()),
        Value::Cursor(_) => SqlValue::Null,
    })
}

fn from_sql(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Int(i),
        ValueRef::Real(f) => Value::Float(f),
        ValueRef::Text(t) => Value::Str(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::Bytes(b.to_vec()),
    }
}

impl Session for SqliteSession {
    fn dialect(&self) -> Dialect {
        Dialect::Sqlite
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let mut stmt = self.conn.prepare(&req.sql).map_err(err)?;
        let params: Vec<ToSqlOutput<'_>> = req.params.iter().map(|p| to_sql(&p.value)).collect();
        let mut result = ExecResult::default();
        if stmt.column_count() > 0 {
            let columns: Vec<Column> = (0..stmt.column_count())
                .map(|i| Column {
                    name: stmt.column_name(i).unwrap_or("?").to_string(),
                    type_name: String::new(),
                })
                .collect();
            let mut rows = stmt.query(params_from_iter(params.iter())).map_err(err)?;
            let mut out_rows = Vec::new();
            while let Some(row) = rows.next().map_err(err)? {
                let mut cells = Vec::with_capacity(columns.len());
                for i in 0..columns.len() {
                    cells.push(from_sql(row.get_ref(i).map_err(err)?));
                }
                out_rows.push(cells);
            }
            result.result_sets.push(ResultSet {
                columns,
                rows: out_rows,
            });
        } else {
            let n = stmt.execute(params_from_iter(params.iter())).map_err(err)?;
            result.rows_affected = Some(n as u64);
        }
        Ok(result)
    }

    fn fetch_cursor(&mut self, _cursor: CursorId) -> Result<ResultSet, DbError> {
        Err(DbError {
            code: None,
            message: "SQLite에는 REF CURSOR가 없습니다".into(),
            position: None,
        })
    }

    fn commit(&mut self) -> Result<(), DbError> {
        if !self.conn.is_autocommit() {
            self.conn.execute_batch("COMMIT").map_err(err)?;
        }
        Ok(())
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        if !self.conn.is_autocommit() {
            self.conn.execute_batch("ROLLBACK").map_err(err)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{BindParam, Direction, VarType};

    #[test]
    fn query_and_dml_with_positional_params() {
        let mut s = SqliteSession::open(":memory:").unwrap();
        s.execute(&ExecRequest {
            sql: "CREATE TABLE t(id INTEGER, name TEXT)".into(),
            params: vec![],
        })
        .unwrap();
        let r = s
            .execute(&ExecRequest {
                sql: "INSERT INTO t VALUES (?, ?)".into(),
                params: vec![
                    BindParam {
                        name: "A".into(),
                        value: Value::Int(1),
                        ty: VarType::Number,
                        direction: Direction::In,
                    },
                    BindParam {
                        name: "B".into(),
                        value: Value::Str("홍길동".into()),
                        ty: VarType::Auto,
                        direction: Direction::In,
                    },
                ],
            })
            .unwrap();
        assert_eq!(r.rows_affected, Some(1));
        let r = s
            .execute(&ExecRequest {
                sql: "SELECT id, name FROM t".into(),
                params: vec![],
            })
            .unwrap();
        let rs = &r.result_sets[0];
        assert_eq!(rs.columns[1].name, "name");
        assert_eq!(rs.rows[0], vec![Value::Int(1), Value::Str("홍길동".into())]);
    }

    #[test]
    fn errors_are_reported() {
        let mut s = SqliteSession::open(":memory:").unwrap();
        let e = s
            .execute(&ExecRequest {
                sql: "SELEC 1".into(),
                params: vec![],
            })
            .unwrap_err();
        assert!(e.message.contains("syntax error"), "{e}");
    }
}
