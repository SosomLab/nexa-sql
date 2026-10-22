//! DuckDB 어댑터(D-350 · 사용자 10-23 "duckdb 개발/테스트") — `nsql_core::Session` 구현 · 방언 [`Dialect::Duckdb`](`?` 위치 바인드 ·
//! 파일/메모리 DB · 자동 커밋 · 트랜잭션 = `BEGIN TRANSACTION`/`COMMIT`/`ROLLBACK`).
//!
//! - 바인딩 = duckdb-rs(MIT · DR-3 예외) · libduckdb는 **동적 링크**(빌드 = `download-lib`가 공식 prebuilt를 받는다 · 런타임 = 패키지 안의
//!   `duckdb.dll`/`libduckdb.so`/`libduckdb.dylib` — 로더가 cdylib 폴더를 먼저 찾는다 · 64 §5).
//! - 결과 = 전부 읽어 `ResultSet`(상한 `max_rows` · 서버 커서 없음 · DuckDB는 결과를 통째로 만든다) · 값 = 정수 계열 → `Int` · 실수 → `Float` ·
//!   DECIMAL → `Decimal`(문자열) · 문자열/날짜/시각/리스트 등 → `Str`(DuckDB 텍스트 표현) · BLOB → `Bytes` · NULL.
//! - DML 영향 행 수 = 실행 결과의 `Count` 열(DuckDB는 INSERT/UPDATE/DELETE가 한 행짜리 결과로 돌려준다) · 조회는 그대로 결과 집합.
//! - 카탈로그 훅 = `information_schema`(`catalog_tables`/`catalog_columns`) — ODBC처럼 탐색기가 훅으로 읽는다.
//! - 취소 = `InterruptHandle`(실행 중 다른 스레드) · 확장 전용(`export_driver!("duckdb", …)`).
#![cfg_attr(test, allow(clippy::unwrap_used))]

use duckdb::types::ValueRef;
use duckdb::{params_from_iter, Connection};
use nsql_core::{
    CatalogColumn, CatalogTable, Column, CursorId, DbError, Dialect, ExecRequest, ExecResult,
    ResultSet, Session, Value,
};

#[allow(missing_debug_implementations)]
pub struct DuckSession {
    conn: Connection,
    interrupt: std::sync::Arc<duckdb::InterruptHandle>,
    max_rows: usize,
    description: String,
}

fn err(e: duckdb::Error) -> DbError {
    DbError {
        code: None,
        message: e.to_string(),
        position: None,
    }
}

impl DuckSession {
    /// 파일 경로(없으면 만든다) 또는 `:memory:`.
    pub fn open(path: &str) -> Result<DuckSession, DbError> {
        let conn = if path.is_empty() || path == ":memory:" {
            Connection::open_in_memory().map_err(err)?
        } else {
            Connection::open(path).map_err(err)?
        };
        let interrupt = conn.interrupt_handle();
        let version: String = conn
            .query_row("SELECT version()", [], |r| r.get(0))
            .unwrap_or_default();
        Ok(DuckSession {
            conn,
            interrupt,
            max_rows: 0,
            description: format!(
                "DuckDB {} · {}",
                version,
                if path.is_empty() { ":memory:" } else { path }
            ),
        })
    }

    pub fn open_from_spec(spec: &nsql_script::ConnectSpec) -> Result<DuckSession, DbError> {
        Self::open(spec.database.as_deref().unwrap_or(":memory:"))
    }

    fn to_param(v: &Value) -> duckdb::types::Value {
        use duckdb::types::Value as D;
        match v {
            Value::Null => D::Null,
            Value::Int(i) => D::BigInt(*i),
            Value::Float(f) => D::Double(*f),
            Value::Bool(b) => D::Boolean(*b),
            Value::Bytes(b) => D::Blob(b.clone()),
            Value::Decimal(s) | Value::Str(s) => D::Text(s.clone()),
            Value::Cursor(_) => D::Null,
        }
    }

    fn from_ref(v: ValueRef<'_>) -> Value {
        match v {
            ValueRef::Null => Value::Null,
            ValueRef::Boolean(b) => Value::Bool(b),
            ValueRef::TinyInt(i) => Value::Int(i64::from(i)),
            ValueRef::SmallInt(i) => Value::Int(i64::from(i)),
            ValueRef::Int(i) => Value::Int(i64::from(i)),
            ValueRef::BigInt(i) => Value::Int(i),
            ValueRef::UTinyInt(i) => Value::Int(i64::from(i)),
            ValueRef::USmallInt(i) => Value::Int(i64::from(i)),
            ValueRef::UInt(i) => Value::Int(i64::from(i)),
            ValueRef::UBigInt(i) => {
                i64::try_from(i).map_or_else(|_| Value::Decimal(i.to_string()), Value::Int)
            }
            ValueRef::HugeInt(i) => {
                i64::try_from(i).map_or_else(|_| Value::Decimal(i.to_string()), Value::Int)
            }
            ValueRef::Float(f) => Value::Float(f64::from(f)),
            ValueRef::Double(f) => Value::Float(f),
            ValueRef::Decimal(d) => Value::Decimal(d.to_string()),
            ValueRef::Text(b) => Value::Str(String::from_utf8_lossy(b).into_owned()),
            ValueRef::Blob(b) => Value::Bytes(b.to_vec()),
            ValueRef::Date32(d) => Value::Str(date_text(i64::from(d))),
            ValueRef::Timestamp(u, v) => Value::Str(timestamp_text(u, v)),
            ValueRef::Time64(u, v) => Value::Str(time_text(u, v)),
            ValueRef::Interval {
                months,
                days,
                nanos,
            } => Value::Str(format!("{months} months {days} days {nanos} ns")),
            other => Value::Str(format!("{other:?}")),
        }
    }

    fn result_set(
        stmt: &mut duckdb::Statement<'_>,
        params: Vec<duckdb::types::Value>,
        max_rows: usize,
    ) -> Result<ResultSet, DbError> {
        let mut rows = stmt.query(params_from_iter(params.iter())).map_err(err)?;
        let mut out = ResultSet::default();
        let mut columns: Option<Vec<Column>> = None;
        while let Some(row) = rows.next().map_err(err)? {
            if columns.is_none() {
                let st = row.as_ref();
                let n = st.column_count();
                let mut cols = Vec::with_capacity(n);
                for i in 0..n {
                    let name = st
                        .column_name(i)
                        .map(ToString::to_string)
                        .unwrap_or_default();
                    let ty = row
                        .get_ref(i)
                        .map(|r| format!("{:?}", r.data_type()))
                        .unwrap_or_default();
                    cols.push(Column {
                        name,
                        type_name: ty,
                    });
                }
                columns = Some(cols);
            }
            let n = columns.as_ref().map_or(0, Vec::len);
            let mut vals = Vec::with_capacity(n);
            for i in 0..n {
                vals.push(row.get_ref(i).map(Self::from_ref).unwrap_or(Value::Null));
            }
            out.rows.push(vals);
            if max_rows > 0 && out.rows.len() >= max_rows {
                break;
            }
        }
        // 0행 = 열 메타만(43 §3-1): 문장의 열 이름.
        out.columns = columns.unwrap_or_else(|| {
            (0..stmt.column_count())
                .map(|i| Column {
                    name: stmt
                        .column_name(i)
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    type_name: String::new(),
                })
                .collect()
        });
        Ok(out)
    }

    fn is_query(sql: &str) -> bool {
        let head = sql
            .trim_start()
            .trim_start_matches('(')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_uppercase();
        matches!(
            head.as_str(),
            "SELECT"
                | "WITH"
                | "SHOW"
                | "DESCRIBE"
                | "DESC"
                | "PRAGMA"
                | "EXPLAIN"
                | "VALUES"
                | "FROM"
                | "SUMMARIZE"
                | "CALL"
                | "TABLE"
        )
    }
}

/// 1970-01-01부터의 일수 → `YYYY-MM-DD`(Howard Hinnant civil_from_days · 외부 crate 0).
fn date_text(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn unit_per_sec(u: duckdb::types::TimeUnit) -> i64 {
    use duckdb::types::TimeUnit as U;
    match u {
        U::Second => 1,
        U::Millisecond => 1_000,
        U::Microsecond => 1_000_000,
        U::Nanosecond => 1_000_000_000,
    }
}

fn time_of_day(secs: i64, frac: i64, per_sec: i64) -> String {
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if frac == 0 || per_sec == 1 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        let digits = match per_sec {
            1_000 => 3,
            1_000_000 => 6,
            _ => 9,
        };
        format!("{h:02}:{m:02}:{s:02}.{:0width$}", frac, width = digits)
    }
}

fn timestamp_text(u: duckdb::types::TimeUnit, v: i64) -> String {
    let per = unit_per_sec(u);
    let total_secs = v.div_euclid(per);
    let frac = v.rem_euclid(per);
    let days = total_secs.div_euclid(86_400);
    let secs = total_secs.rem_euclid(86_400);
    format!("{} {}", date_text(days), time_of_day(secs, frac, per))
}

fn time_text(u: duckdb::types::TimeUnit, v: i64) -> String {
    let per = unit_per_sec(u);
    time_of_day(v.div_euclid(per).rem_euclid(86_400), v.rem_euclid(per), per)
}

impl Session for DuckSession {
    fn dialect(&self) -> Dialect {
        Dialect::Duckdb
    }

    fn driver_name(&self) -> String {
        "duckdb-rs".to_string()
    }

    fn cancel_handle(&self) -> Option<std::sync::Arc<dyn nsql_core::CancelHandle>> {
        struct H(std::sync::Arc<duckdb::InterruptHandle>);
        impl nsql_core::CancelHandle for H {
            fn cancel(&self) -> Result<(), DbError> {
                self.0.interrupt();
                Ok(())
            }
        }
        Some(std::sync::Arc::new(H(std::sync::Arc::clone(
            &self.interrupt,
        ))))
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let params: Vec<duckdb::types::Value> = req
            .params
            .iter()
            .map(|p| Self::to_param(&p.value))
            .collect();
        let mut stmt = self.conn.prepare(&req.sql).map_err(err)?;
        if Self::is_query(&req.sql) {
            let rs = Self::result_set(&mut stmt, params, self.max_rows)?;
            return Ok(ExecResult {
                result_sets: vec![rs],
                ..ExecResult::default()
            });
        }
        // DML/DDL: DuckDB는 영향 행 수를 한 행짜리 결과(Count)로 돌려준다 — `execute`가 그것을 읽는다.
        let n = stmt.execute(params_from_iter(params.iter())).map_err(err)?;
        Ok(ExecResult {
            rows_affected: Some(n as u64),
            ..ExecResult::default()
        })
    }

    fn fetch_cursor(&mut self, _cursor: CursorId) -> Result<ResultSet, DbError> {
        Err(DbError {
            code: None,
            message: "DuckDB has no server cursors".into(),
            position: None,
        })
    }

    fn set_option(&mut self, name: &str, value: &str) -> Result<(), DbError> {
        if name == "max_rows" {
            self.max_rows = value.parse().unwrap_or(0);
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<(), DbError> {
        match self.conn.execute_batch("COMMIT") {
            Ok(()) => Ok(()),
            // 열린 트랜잭션이 없는 자동 커밋 상태의 COMMIT = 할 일 없음(런너가 자동 커밋 모드에서도 보낼 수 있다).
            Err(e) if e.to_string().contains("no transaction is active") => Ok(()),
            Err(e) => Err(err(e)),
        }
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        match self.conn.execute_batch("ROLLBACK") {
            Ok(()) => Ok(()),
            Err(e) if e.to_string().contains("no transaction is active") => Ok(()),
            Err(e) => Err(err(e)),
        }
    }

    fn is_alive(&self) -> bool {
        self.conn
            .query_row("SELECT 1", [], |r| r.get::<_, i64>(0))
            .is_ok()
    }

    fn catalog_tables(
        &mut self,
        schema: Option<&str>,
    ) -> Result<Option<Vec<CatalogTable>>, DbError> {
        let sql = "SELECT table_catalog, table_schema, table_name, table_type FROM information_schema.tables WHERE (? IS NULL OR table_schema = ?) ORDER BY table_schema, table_name";
        let mut stmt = self.conn.prepare(sql).map_err(err)?;
        let s: Option<String> = schema.map(ToString::to_string);
        let rows = stmt
            .query_map([s.clone(), s], |r| {
                Ok(CatalogTable {
                    catalog: r.get::<_, String>(0).unwrap_or_default(),
                    schema: r.get::<_, String>(1).unwrap_or_default(),
                    name: r.get::<_, String>(2).unwrap_or_default(),
                    kind: match r.get::<_, String>(3).unwrap_or_default().as_str() {
                        "VIEW" => "VIEW".to_string(),
                        _ => "TABLE".to_string(),
                    },
                })
            })
            .map_err(err)?;
        Ok(Some(rows.filter_map(Result::ok).collect()))
    }

    fn catalog_columns(
        &mut self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<Option<Vec<CatalogColumn>>, DbError> {
        let sql = "SELECT column_name, data_type, character_maximum_length, numeric_scale, is_nullable, ordinal_position, column_default FROM information_schema.columns WHERE table_name = ? AND (? IS NULL OR table_schema = ?) ORDER BY ordinal_position";
        let mut stmt = self.conn.prepare(sql).map_err(err)?;
        let s: Option<String> = schema.map(ToString::to_string);
        let rows = stmt
            .query_map(duckdb::params![table, s.clone(), s], |r| {
                Ok(CatalogColumn {
                    name: r.get::<_, String>(0).unwrap_or_default(),
                    type_name: r.get::<_, String>(1).unwrap_or_default(),
                    size: r.get::<_, Option<i64>>(2).unwrap_or(None),
                    decimal_digits: r.get::<_, Option<i64>>(3).unwrap_or(None),
                    nullable: r
                        .get::<_, String>(4)
                        .ok()
                        .map(|v| v.eq_ignore_ascii_case("YES")),
                    position: r.get::<_, i64>(5).unwrap_or(0),
                    default: r
                        .get::<_, Option<String>>(6)
                        .unwrap_or(None)
                        .unwrap_or_default(),
                })
            })
            .map_err(err)?;
        Ok(Some(rows.filter_map(Result::ok).collect()))
    }
}

// ★ 확장 드라이버 내보내기(D-350 · ABI v2): cdylib → `nsql_driver_vtable_duckdb`.
fn nsql_abi_open_from_spec(spec: &nsql_script::ConnectSpec) -> Result<DuckSession, DbError> {
    DuckSession::open_from_spec(spec)
}
nsql_abi::export_driver!("duckdb", "duckdb", nsql_abi_open_from_spec);

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{BindParam, Direction, VarType};

    fn exec(s: &mut DuckSession, sql: &str) -> ExecResult {
        s.execute(&ExecRequest {
            sql: sql.into(),
            params: Vec::new(),
        })
        .unwrap_or_else(|e| panic!("{sql}: {}", e.message))
    }

    #[test]
    fn memory_db_query_dml_bind_types_catalog_and_tx() {
        let mut s = DuckSession::open(":memory:").expect("open");
        assert_eq!(s.dialect(), Dialect::Duckdb);
        assert!(s.describe().starts_with("DuckDB "));
        exec(&mut s, "CREATE TABLE t (id INTEGER, name VARCHAR, amount DECIMAL(10,2), big BIGINT, flag BOOLEAN, raw BLOB, d DATE)");
        let r = exec(&mut s, "INSERT INTO t VALUES (1, 'alpha', 1.50, 9000000000, true, '\\x01\\x02'::BLOB, DATE '2026-10-23'), (2, '감마', NULL, NULL, false, NULL, NULL)");
        assert_eq!(r.rows_affected, Some(2));
        let r = exec(
            &mut s,
            "SELECT id, name, amount, big, flag, raw, d FROM t ORDER BY id",
        );
        let rs = &r.result_sets[0];
        assert_eq!(
            rs.columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["id", "name", "amount", "big", "flag", "raw", "d"]
        );
        assert_eq!(rs.rows.len(), 2);
        assert_eq!(rs.rows[0][0], Value::Int(1));
        assert_eq!(rs.rows[0][1], Value::Str("alpha".into()));
        assert_eq!(rs.rows[0][2], Value::Decimal("1.50".into()));
        assert_eq!(rs.rows[0][3], Value::Int(9_000_000_000));
        assert_eq!(rs.rows[0][4], Value::Bool(true));
        assert_eq!(rs.rows[0][5], Value::Bytes(vec![1, 2]));
        assert_eq!(rs.rows[0][6], Value::Str("2026-10-23".into()));
        let r = exec(&mut s, "SELECT TIMESTAMP '2026-10-23 13:45:10.5' AS ts, TIME '01:02:03' AS t, DATE '1969-12-31' AS d0");
        assert_eq!(
            r.result_sets[0].rows[0][0],
            Value::Str("2026-10-23 13:45:10.500000".into())
        );
        assert_eq!(r.result_sets[0].rows[0][1], Value::Str("01:02:03".into()));
        assert_eq!(r.result_sets[0].rows[0][2], Value::Str("1969-12-31".into()));
        assert_eq!(rs.rows[1][1], Value::Str("감마".into()));
        assert_eq!(rs.rows[1][2], Value::Null);
        // `?` 바인드.
        let r = s
            .execute(&ExecRequest {
                sql: "SELECT COUNT(*) AS n FROM t WHERE id > ? AND name <> ?".into(),
                params: vec![
                    BindParam {
                        name: "1".into(),
                        value: Value::Int(0),
                        ty: VarType::Number,
                        direction: Direction::In,
                    },
                    BindParam {
                        name: "2".into(),
                        value: Value::Str("zzz".into()),
                        ty: VarType::Varchar2(10),
                        direction: Direction::In,
                    },
                ],
            })
            .expect("bind");
        assert_eq!(r.result_sets[0].rows[0][0], Value::Int(2));
        // 0행 = 열 메타.
        let r = exec(&mut s, "SELECT id FROM t WHERE 1 = 0");
        assert_eq!(r.result_sets[0].columns.len(), 1);
        assert!(r.result_sets[0].rows.is_empty());
        // max_rows.
        s.set_option("max_rows", "1").expect("opt");
        assert_eq!(
            exec(&mut s, "SELECT id FROM t").result_sets[0].rows.len(),
            1
        );
        s.set_option("max_rows", "0").expect("opt");
        // 트랜잭션: BEGIN → INSERT → ROLLBACK = 되돌아감 · 자동 커밋 상태의 COMMIT = no-op.
        exec(&mut s, "BEGIN TRANSACTION");
        exec(&mut s, "INSERT INTO t (id, name) VALUES (3, 'tmp')");
        s.rollback().expect("rollback");
        assert_eq!(
            exec(&mut s, "SELECT COUNT(*) FROM t").result_sets[0].rows[0][0],
            Value::Int(2)
        );
        s.commit().expect("commit without tx = ok");
        // 카탈로그 훅.
        let tables = s
            .catalog_tables(Some("main"))
            .expect("tables")
            .expect("hook");
        assert!(
            tables.iter().any(|t| t.name == "t" && t.kind == "TABLE"),
            "{tables:?}"
        );
        let cols = s
            .catalog_columns(Some("main"), "t")
            .expect("cols")
            .expect("hook");
        assert_eq!(cols.len(), 7);
        assert_eq!(cols[0].name, "id");
        assert_eq!(cols[2].decimal_digits, Some(2));
        // 오류.
        let e = s
            .execute(&ExecRequest {
                sql: "SELECT nope FROM t".into(),
                params: Vec::new(),
            })
            .expect_err("bad column");
        assert!(e.message.contains("nope"), "{}", e.message);
        assert!(s.is_alive());
        assert!(s.cancel_handle().is_some());
    }

    #[test]
    fn file_db_persists() {
        let p = std::env::temp_dir().join(format!("nsql-duck-{}.duckdb", std::process::id()));
        let _ = std::fs::remove_file(&p);
        {
            let mut s = DuckSession::open(&p.to_string_lossy()).expect("open");
            exec(&mut s, "CREATE TABLE k (v INTEGER)");
            exec(&mut s, "INSERT INTO k VALUES (42)");
        }
        let mut s = DuckSession::open(&p.to_string_lossy()).expect("reopen");
        assert_eq!(
            exec(&mut s, "SELECT v FROM k").result_sets[0].rows[0][0],
            Value::Int(42)
        );
        drop(s);
        let _ = std::fs::remove_file(&p);
    }
}

#[cfg(test)]
mod date_tests {
    use super::*;

    #[test]
    fn civil_dates_and_times() {
        assert_eq!(date_text(0), "1970-01-01");
        assert_eq!(date_text(-1), "1969-12-31");
        assert_eq!(date_text(20749), "2026-10-23");
        assert_eq!(date_text(11016), "2000-02-29");
        assert_eq!(
            timestamp_text(duckdb::types::TimeUnit::Microsecond, 1_793_000_000_000_000),
            "2026-10-26 07:33:20"
        );
        assert_eq!(
            time_text(duckdb::types::TimeUnit::Microsecond, 3_723_000_001),
            "01:02:03.000001"
        );
    }
}
