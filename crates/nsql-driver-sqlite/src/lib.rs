//! SQLite 어댑터 — `nsql_core::Session` 구현. 방언 `Dialect::Sqlite`(`?` 위치 바인드).
//! OUT 파라미터가 없는 방언이므로 `EXEC :V := expr`는 엔진이 `SELECT expr AS "V"`로 만들고,
//! 호스트(`nsql-run`)가 1행 결과의 컬럼 이름을 변수로 흡수한다.
//!
//! ## 커서 유지(T-48a · docs/43 §3-3) — 세션 스레드
//! rusqlite의 `Statement<'conn>`·`Rows<'stmt>`는 `Connection`을 **빌린다** → 셋을 한 구조체에 담을 수 없다(자기 참조 ·
//! unsafe 없이는 불가). 그래서 **세션 전용 스레드가 `Connection`을 소유**하고, 열린 커서(문장 + 행 이터레이터)는 그 스레드의
//! **스택 프레임**([`serve_execute`])에 산다 — `OwnedStatement` 대신 "문장 수명 = 프레임 수명"으로 푼 부품이다.
//! [`SqliteSession`]은 채널로 요청하고 답을 기다린다(동기 포트 그대로 · 왕복 ≈ 수 µs).
//!
//! 규칙(docs/43 D-70): 세션당 커서 **1개** · 커서가 열린 동안 다른 실행(COUNT·카탈로그)은 같은 프레임 안에서 **중첩 문장**으로
//! 처리한다(rusqlite는 한 연결에 여러 문장 허용) · 중첩 실행이 새 커서를 열면 앞 커서는 닫힌다 · 커밋/롤백은 커서를 닫고 처리한다.
//! 상한(`max_rows`)까지 읽은 뒤 **한 행을 더 엿봐**(peek) 실제로 남은 행이 있을 때만 커서를 연다(정확히 N행이면 커서 0).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use nsql_core::{
    Column, CursorHandle, CursorId, DbError, Dialect, ExecRequest, ExecResult, ResultSet, Session,
    Value,
};
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{params_from_iter, Connection, Row, Rows};
use std::sync::mpsc::{channel, Receiver, Sender};

/// 세션 스레드로 가는 요청 — 답은 요청 안의 채널로.
enum Req {
    Execute {
        sql: String,
        params: Vec<SqlValue>,
        max_rows: usize,
        reply: Sender<Result<ExecResult, DbError>>,
    },
    FetchNext {
        h: u32,
        max: usize,
        reply: Sender<Result<(ResultSet, bool), DbError>>,
    },
    CloseCursor {
        h: u32,
        reply: Sender<()>,
    },
    Commit {
        reply: Sender<Result<(), DbError>>,
    },
    Rollback {
        reply: Sender<Result<(), DbError>>,
    },
}

#[allow(missing_debug_implementations)]
pub struct SqliteSession {
    tx: Sender<Req>,
    /// 실행 취소(T-108) — 접속 스레드의 `sqlite3_interrupt`.
    interrupt: std::sync::Arc<rusqlite::InterruptHandle>,
    /// 페치 상한(0 = 무제한 · 세션 옵션 `max_rows`).
    max_rows: usize,
    description: String,
}

impl SqliteSession {
    /// `":memory:"` 또는 파일 경로. 연결은 세션 스레드에서 열리고 그 스레드가 끝날 때(세션 drop) 닫힌다.
    pub fn open(target: &str) -> Result<Self, DbError> {
        let conn = if target.is_empty() || target == ":memory:" {
            Connection::open_in_memory()
        } else {
            Connection::open(target)
        }
        .map_err(err)?;
        let interrupt = std::sync::Arc::new(conn.get_interrupt_handle());
        let (tx, rx) = channel::<Req>();
        std::thread::Builder::new()
            .name("nsql-sqlite".into())
            .spawn(move || serve(conn, &rx))
            .map_err(|e| DbError {
                code: None,
                message: e.to_string(),
                position: None,
            })?;
        Ok(SqliteSession {
            tx,
            interrupt,
            max_rows: 0,
            description: if target.is_empty() {
                ":memory:".into()
            } else {
                target.to_string()
            },
        })
    }

    /// 요청 1회 · 답 대기. 스레드가 죽었으면(패닉) 세션 소실 오류.
    fn call<T>(&self, make: impl FnOnce(Sender<T>) -> Req) -> Result<T, DbError> {
        let (tx, rx) = channel::<T>();
        self.tx.send(make(tx)).map_err(|_| dead())?;
        rx.recv().map_err(|_| dead())
    }
}

fn dead() -> DbError {
    DbError {
        code: None,
        message: "SQLite session thread has ended".into(),
        position: None,
    }
}

fn closed(h: u32) -> DbError {
    DbError {
        code: None,
        message: format!("cursor #{h} is not open"),
        position: None,
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

fn to_sql(v: &Value) -> SqlValue {
    match v {
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
    }
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

fn row_values(row: &Row<'_>, ncols: usize) -> Result<Vec<Value>, DbError> {
    let mut cells = Vec::with_capacity(ncols);
    for i in 0..ncols {
        cells.push(from_sql(row.get_ref(i).map_err(err)?));
    }
    Ok(cells)
}

/// 한 묶음 = (읽은 행, 엿본 행 — `Some`이면 뒤에 더 있다).
type Batch = (Vec<Vec<Value>>, Option<Vec<Value>>);

/// 행 이터레이터에서 `max`행(0 = 끝까지)을 읽고, 상한에 닿았으면 한 행을 더 엿본다.
fn read_batch(
    rows: &mut Rows<'_>,
    ncols: usize,
    max: usize,
    carry: Option<Vec<Value>>,
) -> Result<Batch, DbError> {
    let mut out: Vec<Vec<Value>> = Vec::new();
    if let Some(c) = carry {
        out.push(c);
    }
    loop {
        if max > 0 && out.len() >= max {
            let peek = match rows.next().map_err(err)? {
                Some(r) => Some(row_values(r, ncols)?),
                None => None,
            };
            return Ok((out, peek));
        }
        match rows.next().map_err(err)? {
            Some(r) => out.push(row_values(r, ncols)?),
            None => return Ok((out, None)),
        }
    }
}

/// 세션 스레드 본체 — 연결을 소유하고 요청을 순서대로 처리한다. `rx`가 닫히면(세션 drop) 끝난다.
fn serve(conn: Connection, rx: &Receiver<Req>) {
    let mut next_id: u32 = 1;
    let mut pending: Option<Req> = None;
    loop {
        let req = match pending.take() {
            Some(r) => r,
            None => match rx.recv() {
                Ok(r) => r,
                Err(_) => break,
            },
        };
        match req {
            Req::Execute {
                sql,
                params,
                max_rows,
                reply,
            } => {
                let (next, _) =
                    serve_execute(&conn, rx, &mut next_id, &sql, &params, max_rows, reply);
                pending = next;
            }
            Req::FetchNext { h, reply, .. } => {
                let _ = reply.send(Err(closed(h)));
            }
            Req::CloseCursor { reply, .. } => {
                let _ = reply.send(());
            }
            Req::Commit { reply } => {
                let r = if conn.is_autocommit() {
                    Ok(())
                } else {
                    conn.execute_batch("COMMIT").map_err(err)
                };
                let _ = reply.send(r);
            }
            Req::Rollback { reply } => {
                let r = if conn.is_autocommit() {
                    Ok(())
                } else {
                    conn.execute_batch("ROLLBACK").map_err(err)
                };
                let _ = reply.send(r);
            }
        }
    }
}

/// 문장 하나를 실행한다. 조회가 상한에서 잘리고 남은 행이 있으면 **이 프레임에서 커서를 열어 둔 채**
/// 다음 요청을 받는다(`FetchNext`/`CloseCursor` · 중첩 `Execute`). 커서가 닫히면 돌아온다.
/// 반환 = (이 프레임이 처리하지 못해 바깥으로 넘기는 요청(커밋/롤백), 커서를 열었는가).
fn serve_execute(
    conn: &Connection,
    rx: &Receiver<Req>,
    next_id: &mut u32,
    sql: &str,
    params: &[SqlValue],
    max_rows: usize,
    reply: Sender<Result<ExecResult, DbError>>,
) -> (Option<Req>, bool) {
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => {
            let _ = reply.send(Err(err(e)));
            return (None, false);
        }
    };
    let ncols = stmt.column_count();
    if ncols == 0 {
        let r = stmt
            .execute(params_from_iter(params.iter()))
            .map(|n| ExecResult {
                rows_affected: Some(n as u64),
                ..ExecResult::default()
            })
            .map_err(err);
        let _ = reply.send(r);
        return (None, false);
    }
    let columns: Vec<Column> = (0..ncols)
        .map(|i| Column {
            name: stmt.column_name(i).unwrap_or("?").to_string(),
            type_name: String::new(),
        })
        .collect();
    let mut rows = match stmt.query(params_from_iter(params.iter())) {
        Ok(r) => r,
        Err(e) => {
            let _ = reply.send(Err(err(e)));
            return (None, false);
        }
    };
    let (batch, peek) = match read_batch(&mut rows, ncols, max_rows, None) {
        Ok(x) => x,
        Err(e) => {
            let _ = reply.send(Err(e));
            return (None, false);
        }
    };
    let mut result = ExecResult::default();
    result.result_sets.push(ResultSet {
        columns: columns.clone(),
        rows: batch,
    });
    let Some(first) = peek else {
        let _ = reply.send(Ok(result));
        return (None, false);
    };
    // 남은 행이 있다 → 커서 열기(이 프레임 유지).
    let id = *next_id;
    *next_id += 1;
    result.pending = Some(CursorHandle(id));
    let _ = reply.send(Ok(result));
    let mut carry = Some(first);
    loop {
        let req = match rx.recv() {
            Ok(r) => r,
            Err(_) => return (None, true),
        };
        match req {
            Req::FetchNext { h, max, reply } if h == id => {
                match read_batch(&mut rows, ncols, max, carry.take()) {
                    Ok((batch, peek)) => {
                        let more = peek.is_some();
                        carry = peek;
                        let _ = reply.send(Ok((
                            ResultSet {
                                columns: columns.clone(),
                                rows: batch,
                            },
                            more,
                        )));
                        if !more {
                            return (None, true);
                        }
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                        return (None, true);
                    }
                }
            }
            Req::FetchNext { h, reply, .. } => {
                let _ = reply.send(Err(closed(h)));
            }
            Req::CloseCursor { h, reply } if h == id => {
                let _ = reply.send(());
                return (None, true);
            }
            Req::CloseCursor { reply, .. } => {
                let _ = reply.send(());
            }
            Req::Execute {
                sql,
                params,
                max_rows,
                reply,
            } => {
                // 중첩 실행(COUNT·카탈로그) — 이 커서는 살아 있다. 안쪽이 새 커서를 열었으면 세션당 1개 규칙으로 이 커서를 닫는다.
                let (next, opened) =
                    serve_execute(conn, rx, next_id, &sql, &params, max_rows, reply);
                if opened {
                    return (next, true);
                }
            }
            other @ (Req::Commit { .. } | Req::Rollback { .. }) => {
                // 커밋/롤백 = 커서 닫고 바깥 루프가 처리.
                return (Some(other), true);
            }
        }
    }
}

/// `sqlite3_interrupt` — 진행 중 문장은 `SQLITE_INTERRUPT`로 끝난다.
struct SqliteCancel(std::sync::Arc<rusqlite::InterruptHandle>);

impl nsql_core::CancelHandle for SqliteCancel {
    fn cancel(&self) -> Result<(), DbError> {
        self.0.interrupt();
        Ok(())
    }
}

impl Session for SqliteSession {
    fn cancel_handle(&self) -> Option<std::sync::Arc<dyn nsql_core::CancelHandle>> {
        Some(std::sync::Arc::new(SqliteCancel(self.interrupt.clone())))
    }

    fn dialect(&self) -> Dialect {
        Dialect::Sqlite
    }

    fn describe(&self) -> String {
        format!("sqlite {}", self.description)
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let sql = req.sql.clone();
        let params: Vec<SqlValue> = req.params.iter().map(|p| to_sql(&p.value)).collect();
        let max_rows = self.max_rows;
        self.call(|reply| Req::Execute {
            sql,
            params,
            max_rows,
            reply,
        })?
    }

    fn cursor_supported(&self) -> bool {
        true
    }

    fn fetch_next(&mut self, h: CursorHandle, max: usize) -> Result<(ResultSet, bool), DbError> {
        self.call(|reply| Req::FetchNext { h: h.0, max, reply })?
    }

    fn close_cursor(&mut self, h: CursorHandle) -> Result<(), DbError> {
        self.call(|reply| Req::CloseCursor { h: h.0, reply })
    }

    fn fetch_cursor(&mut self, _cursor: CursorId) -> Result<ResultSet, DbError> {
        Err(DbError {
            code: None,
            message: "SQLite에는 REF CURSOR가 없습니다".into(),
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
        self.call(|reply| Req::Commit { reply })?
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        self.call(|reply| Req::Rollback { reply })?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::{BindParam, Direction, VarType};

    fn run(s: &mut SqliteSession, sql: &str) -> ExecResult {
        s.execute(&ExecRequest {
            sql: sql.into(),
            params: vec![],
        })
        .unwrap()
    }

    fn seed(n: i64) -> SqliteSession {
        let mut s = SqliteSession::open(":memory:").unwrap();
        run(&mut s, "CREATE TABLE t(id INTEGER)");
        run(
            &mut s,
            &format!(
                "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < {n}) INSERT INTO t SELECT x FROM c"
            ),
        );
        s
    }

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
        assert!(r.pending.is_none());
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

    /// 커서 유지: 상한 10으로 25행을 3번에 나눠 받는다 · 끝에서 커서가 닫힌다 · 옛 핸들은 오류.
    #[test]
    fn cursor_pages_through_rows_and_closes_at_end() {
        let mut s = seed(25);
        assert!(s.cursor_supported());
        s.set_option("max_rows", "10").unwrap();
        let r = run(&mut s, "SELECT id FROM t ORDER BY id");
        assert_eq!(r.result_sets[0].rows.len(), 10);
        let h = r.pending.expect("커서 열림");
        let (rs, more) = s.fetch_next(h, 10).unwrap();
        assert_eq!(rs.rows.len(), 10);
        assert_eq!(rs.rows[0][0], Value::Int(11));
        assert!(more);
        let (rs, more) = s.fetch_next(h, 10).unwrap();
        assert_eq!(rs.rows.len(), 5);
        assert_eq!(rs.rows[4][0], Value::Int(25));
        assert!(!more, "끝 = 커서 닫힘");
        assert!(s.fetch_next(h, 10).is_err(), "닫힌 핸들");
    }

    /// 정확히 상한만큼이면 커서를 열지 않는다(peek) · 커서가 열린 동안 중첩 실행(COUNT)은 커서를 살려 둔다 ·
    /// 커밋은 커서를 닫는다.
    #[test]
    fn exact_limit_opens_no_cursor_and_nested_execute_keeps_it() {
        let mut s = seed(10);
        s.set_option("max_rows", "10").unwrap();
        let r = run(&mut s, "SELECT id FROM t");
        assert_eq!(r.result_sets[0].rows.len(), 10);
        assert!(r.pending.is_none(), "정확히 10행 = 더 없음");

        s.set_option("max_rows", "3").unwrap();
        let r = run(&mut s, "SELECT id FROM t ORDER BY id");
        let h = r.pending.unwrap();
        // 중첩: COUNT(*)는 상한 0으로.
        s.set_option("max_rows", "0").unwrap();
        let c = run(&mut s, "SELECT COUNT(*) FROM t");
        assert_eq!(c.result_sets[0].rows[0][0], Value::Int(10));
        assert!(c.pending.is_none());
        let (rs, more) = s.fetch_next(h, 3).unwrap();
        assert_eq!(rs.rows[0][0], Value::Int(4), "커서 위치 유지");
        assert!(more);
        // 커밋 = 커서 닫힘.
        s.commit().unwrap();
        assert!(s.fetch_next(h, 3).is_err());
        // 이후 보통 실행은 정상.
        let r = run(&mut s, "SELECT COUNT(*) FROM t");
        assert_eq!(r.result_sets[0].rows[0][0], Value::Int(10));
    }

    /// 새 커서가 열리면 앞 커서는 닫힌다(세션당 1개) · 명시 close 뒤 옛 핸들은 오류.
    #[test]
    fn one_cursor_per_session() {
        let mut s = seed(20);
        s.set_option("max_rows", "5").unwrap();
        let a = run(&mut s, "SELECT id FROM t ORDER BY id").pending.unwrap();
        let b = run(&mut s, "SELECT id FROM t ORDER BY id DESC")
            .pending
            .unwrap();
        assert_ne!(a, b);
        assert!(s.fetch_next(a, 5).is_err(), "앞 커서는 대체됨");
        let (rs, _) = s.fetch_next(b, 5).unwrap();
        assert_eq!(rs.rows[0][0], Value::Int(15));
        s.close_cursor(b).unwrap();
        assert!(s.fetch_next(b, 5).is_err());
        s.close_cursor(b).unwrap(); // 두 번 닫아도 조용히
    }
}
