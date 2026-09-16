//! Oracle 어댑터 — kubo `oracle`(ODPI-C). `nsql_core::Session` 구현.
//!
//! - 이름 바인드: 엔진이 `:NAME`을 그대로 두므로 `Statement::bind(name, …)`로 IN/InOut을 붙인다.
//! - InOut 스칼라는 **VARCHAR2(4000)** 로 바인드하고 실행 후 문자열로 회수해 [`VarType`]에 맞춰 값을 만든다
//!   (SQL*Plus의 `VARIABLE … VARCHAR2` 관용 · Oracle 암시 변환이 NUMBER↔VARCHAR2를 처리). CLOB는 `Clob`.
//! - REF CURSOR는 `OracleType::RefCursor`로 바인드 → 실행 후 `bind_value::<RefCursor>` → 핸들 보관 → `PRINT`가 1회 소비.
//! - `SET SERVEROUTPUT ON`이면 실행마다 `DBMS_OUTPUT.GET_LINE`을 폴링해 메시지로.
//! - Instant Client는 ODPI-C가 런타임 dlopen — 없으면 접속 시 오류 메시지에 그대로 드러난다(경로 안내는 CLI 몫).
//! - ★ 커서 유지(T-48a · docs/43 §3-3): 조회는 `Statement::into_result_set`으로 **소유 ResultSet**(문장 수명 = 커서 수명)을 만들고,
//!   상한(`max_rows`)까지 읽은 뒤 한 행을 **엿봐**(peek) 남은 행이 있으면 세션 구조체에 보관한다([`OpenCursor`]) →
//!   [`Session::fetch_next`]가 이어 읽는다. 세션당 1개(새 커서가 앞 커서를 대체) · 커밋/롤백/접속 해제가 닫는다.
//!   `fetch_array_size` = 세션 옵션 `fetch_size`(설정 `db.fetch_size` · SQL*Plus ARRAYSIZE 격).

use nsql_core::{
    Column, CursorHandle, CursorId, DbError, Dialect, Direction, ExecRequest, ExecResult,
    ResultSet, Session, Stage, Value, VarType,
};
use nsql_script::ConnectSpec;
use oracle::sql_type::{OracleType, RefCursor};
use oracle::{Connection, Connector, InitParams, Privilege, Row};
use std::collections::HashMap;

/// Instant Client 위치를 지정하는 환경변수(선택). 없으면 ODPI-C 기본 탐색(`ORACLE_HOME/lib` · macOS `~/lib`·`/usr/local/lib` · Linux `LD_LIBRARY_PATH` · Windows `PATH`).
pub const CLIENT_DIR_ENV: &str = "NSQL_ORACLE_CLIENT_DIR";

/// ODPI-C 초기화 — 첫 접속 전에 1회. `NSQL_ORACLE_CLIENT_DIR`가 있으면 그 폴더에서만 `libclntsh`를 찾는다.
fn init_client() -> Result<(), DbError> {
    if InitParams::is_initialized() {
        return Ok(());
    }
    let mut p = InitParams::new();
    if let Some(dir) = std::env::var_os(CLIENT_DIR_ENV) {
        p.oracle_client_lib_dir(dir).map_err(|e| err(&e))?;
    }
    p.init().map(|_| ()).map_err(|e| err(&e))
}

/// 클라이언트 로드 실패(DPI-1047)에 설치 안내를 덧붙인다.
fn with_client_hint(mut e: DbError) -> DbError {
    if e.message.contains("DPI-1047") || e.message.to_ascii_lowercase().contains("cannot locate") {
        e.message.push_str(&format!(
            "\n→ Oracle Instant Client가 필요합니다. 설치 후 {CLIENT_DIR_ENV}=<instantclient 폴더> 로 위치를 알려 주거나 \
             macOS는 ~/lib 에 libclntsh.dylib 심볼릭 링크 · Linux는 LD_LIBRARY_PATH · Windows는 PATH. 자세히: docs/20 §5"
        ));
    }
    e
}

/// 한 묶음 = (읽은 행, 엿본 행 — `Some`이면 뒤에 더 있다).
type Batch = (Vec<Vec<Value>>, Option<Vec<Value>>);

/// 열린 서버 커서(추가 페치용 · docs/43 D-70) — 소유 ResultSet이 문장 핸들을 쥔다(drop = 커서 닫힘).
struct OpenCursor {
    id: u32,
    rs: oracle::ResultSet<'static, Row>,
    columns: Vec<Column>,
    /// 상한에서 엿본 다음 행(다음 `fetch_next`의 첫 행).
    carry: Option<Vec<Value>>,
}

#[allow(missing_debug_implementations)]
pub struct OracleSession {
    conn: Connection,
    cursors: HashMap<u64, RefCursor>,
    next_cursor: u64,
    serveroutput: bool,
    fetch_size: u32,
    /// 페치 상한(0 = 무제한 · 세션 옵션 `max_rows`). 커서 지원 세션이므로 호스트는 상한을 그대로 넘긴다(초과 판정은 peek).
    max_rows: usize,
    /// 추가 페치 커서(세션당 1개).
    open: Option<OpenCursor>,
    next_open: u32,
    description: String,
}

fn err(e: &oracle::Error) -> DbError {
    let (code, position) = match e.db_error() {
        Some(d) => (
            Some(i64::from(d.code())),
            Some(d.offset() as usize).filter(|o| *o > 0),
        ),
        None => (None, None),
    };
    DbError {
        code,
        message: e.to_string(),
        position,
    }
}

impl OracleSession {
    /// `user/pass@host:port/service` · `user/pass@tns` · `/ as sysdba`(외부 인증).
    pub fn connect(spec: &ConnectSpec) -> Result<Self, DbError> {
        let target = match (&spec.host, spec.port, &spec.database) {
            (Some(h), Some(p), Some(db)) => format!("{h}:{p}/{db}"),
            (Some(h), None, Some(db)) => format!("{h}/{db}"),
            (Some(h), Some(p), None) => format!("{h}:{p}"),
            (Some(h), None, None) => h.clone(),
            (None, _, _) => String::new(),
        };
        let user = spec.user.clone().unwrap_or_default();
        let pass = spec.password.clone().unwrap_or_default();
        let mut connector = Connector::new(user.as_str(), pass.as_str(), target.as_str());
        if user.is_empty() && pass.is_empty() {
            connector.external_auth(true);
        }
        match spec.role.as_deref() {
            Some("SYSDBA") => {
                connector.privilege(Privilege::Sysdba);
            }
            Some("SYSOPER") => {
                connector.privilege(Privilege::Sysoper);
            }
            _ => {}
        }
        init_client().map_err(with_client_hint)?;
        let conn = connector.connect().map_err(|e| with_client_hint(err(&e)))?;
        Ok(OracleSession {
            conn,
            cursors: HashMap::new(),
            next_cursor: 1,
            serveroutput: false,
            fetch_size: 500,
            max_rows: 0,
            open: None,
            next_open: 1,
            description: spec.redacted(),
        })
    }

    fn row_to_values(row: &Row) -> Result<Vec<Value>, DbError> {
        let mut out = Vec::with_capacity(row.sql_values().len());
        for sv in row.sql_values() {
            if sv.is_null().map_err(|e| err(&e))? {
                out.push(Value::Null);
                continue;
            }
            let ty = sv.oracle_type().map_err(|e| err(&e))?.clone();
            let v = match ty {
                OracleType::Number(_, _) | OracleType::Int64 | OracleType::UInt64 => {
                    let s: String = sv.get().map_err(|e| err(&e))?;
                    match s.parse::<i64>() {
                        Ok(i) => Value::Int(i),
                        Err(_) => Value::Decimal(s),
                    }
                }
                OracleType::Float(_) | OracleType::BinaryFloat | OracleType::BinaryDouble => {
                    Value::Float(sv.get::<f64>().map_err(|e| err(&e))?)
                }
                OracleType::Raw(_) | OracleType::LongRaw | OracleType::BLOB => {
                    Value::Bytes(sv.get::<Vec<u8>>().map_err(|e| err(&e))?)
                }
                OracleType::Boolean => Value::Bool(sv.get::<bool>().map_err(|e| err(&e))?),
                _ => Value::Str(sv.get::<String>().map_err(|e| err(&e))?),
            };
            out.push(v);
        }
        Ok(out)
    }

    fn columns(info: &[oracle::ColumnInfo]) -> Vec<Column> {
        info.iter()
            .map(|c| Column {
                name: c.name().to_string(),
                type_name: c.oracle_type().to_string(),
            })
            .collect()
    }

    fn poll_dbms_output(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        if !self.serveroutput {
            return lines;
        }
        let Ok(mut stmt) = self
            .conn
            .statement("BEGIN DBMS_OUTPUT.GET_LINE(:line, :status); END;")
            .build()
        else {
            return lines;
        };
        for _ in 0..100_000 {
            if stmt.bind("line", &OracleType::Varchar2(32767)).is_err()
                || stmt.bind("status", &OracleType::Number(0, 0)).is_err()
            {
                break;
            }
            if stmt.execute(&[]).is_err() {
                break;
            }
            let status: i64 = stmt.bind_value("status").unwrap_or(1);
            if status != 0 {
                break;
            }
            let line: Option<String> = stmt.bind_value("line").unwrap_or(None);
            lines.push(line.unwrap_or_default());
        }
        lines
    }

    fn coerce(ty: &VarType, s: Option<String>) -> Value {
        match s {
            None => Value::Null,
            Some(s) => match ty {
                VarType::Number | VarType::BinaryFloat | VarType::BinaryDouble => {
                    match s.parse::<i64>() {
                        Ok(i) => Value::Int(i),
                        Err(_) => Value::Decimal(s),
                    }
                }
                VarType::Auto => match s.parse::<i64>() {
                    Ok(i) => Value::Int(i),
                    Err(_) => Value::Str(s),
                },
                _ => Value::Str(s),
            },
        }
    }

    /// 소유 ResultSet에서 `max`행(0 = 끝까지)을 읽고 상한에 닿았으면 한 행을 더 엿본다 — (읽은 행, 엿본 행).
    fn read_batch(
        rs: &mut oracle::ResultSet<'static, Row>,
        max: usize,
        carry: Option<Vec<Value>>,
    ) -> Result<Batch, DbError> {
        let mut out: Vec<Vec<Value>> = Vec::new();
        if let Some(c) = carry {
            out.push(c);
        }
        loop {
            if max > 0 && out.len() >= max {
                let peek = match rs.next() {
                    Some(r) => Some(Self::row_to_values(&r.map_err(|e| err(&e))?)?),
                    None => None,
                };
                return Ok((out, peek));
            }
            match rs.next() {
                Some(r) => out.push(Self::row_to_values(&r.map_err(|e| err(&e))?)?),
                None => return Ok((out, None)),
            }
        }
    }
}

impl Session for OracleSession {
    fn dialect(&self) -> Dialect {
        Dialect::Oracle
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn set_option(&mut self, name: &str, value: &str) -> Result<(), DbError> {
        match name {
            "serveroutput" => {
                let on = value.eq_ignore_ascii_case("on");
                self.serveroutput = on;
                let sql = if on {
                    "BEGIN DBMS_OUTPUT.ENABLE(NULL); END;"
                } else {
                    "BEGIN DBMS_OUTPUT.DISABLE; END;"
                };
                self.conn.execute(sql, &[]).map_err(|e| err(&e))?;
            }
            "fetch_size" => self.fetch_size = value.parse().unwrap_or(500),
            "max_rows" => self.max_rows = value.parse().unwrap_or(0),
            "autocommit" => self.conn.set_autocommit(value == "true"),
            _ => {}
        }
        Ok(())
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let mut stmt = self
            .conn
            .statement(&req.sql)
            .fetch_array_size(self.fetch_size)
            .build()
            .map_err(|e| err(&e))?;
        // 바인드 — 실제로 문장에 있는 이름만(엔진이 추출한 이름과 OCI 이름은 같아야 하지만 방어).
        let present: Vec<String> = stmt
            .bind_names()
            .iter()
            .map(|n| n.to_ascii_uppercase())
            .collect();
        for p in &req.params {
            if !present.contains(&p.name) {
                continue;
            }
            let name = p.name.as_str();
            match (&p.ty, &p.direction) {
                (VarType::RefCursor, _) => stmt.bind(name, &OracleType::RefCursor),
                (VarType::Clob, _) => {
                    let init: Option<String> = match &p.value {
                        Value::Null => None,
                        v => Some(v.display()),
                    };
                    stmt.bind(name, &(&init, &OracleType::CLOB))
                }
                (_, Direction::In) => match &p.value {
                    Value::Null => stmt.bind(name, &(&None::<String>, &OracleType::Varchar2(4000))),
                    Value::Int(i) => stmt.bind(name, i),
                    Value::Float(f) => stmt.bind(name, f),
                    Value::Bool(b) => stmt.bind(name, &i64::from(*b)),
                    Value::Bytes(b) => stmt.bind(name, b),
                    Value::Decimal(d) => stmt.bind(name, &(d, &OracleType::Number(0, 0))),
                    Value::Str(s) => stmt.bind(name, s),
                    Value::Cursor(_) => stmt.bind(name, &OracleType::RefCursor),
                },
                (_, Direction::InOut | Direction::Out) => {
                    let init: Option<String> = match &p.value {
                        Value::Null => None,
                        v => Some(v.display()),
                    };
                    stmt.bind(name, &(&init, &OracleType::Varchar2(4000)))
                }
            }
            .map_err(|e| err(&e))?;
        }
        let mut result = ExecResult::default();
        if stmt.is_query() {
            // Execute = 첫 응답까지(옵티마이저·실행) · Fetch = 배열 페치 + 디코딩(행 수 기록).
            let t0 = std::time::Instant::now();
            let mut rs = stmt.into_result_set::<Row>(&[]).map_err(|e| err(&e))?;
            let columns = Self::columns(rs.column_info());
            result.timing.push(Stage::Execute, t0.elapsed());
            let t1 = std::time::Instant::now();
            let (rows, peek) = Self::read_batch(&mut rs, self.max_rows, None)?;
            let rs_out = ResultSet {
                columns: columns.clone(),
                rows,
            };
            let span = result.timing.push(Stage::Fetch, t1.elapsed());
            span.rows = Some(rs_out.rows.len() as u64);
            span.bytes = Some(rs_out.approx_bytes());
            span.note = Some(format!("array {}", self.fetch_size));
            result.result_sets.push(rs_out);
            // 남은 행이 있으면 커서 유지(앞 커서는 대체 = 세션당 1개).
            if let Some(first) = peek {
                let id = self.next_open;
                self.next_open += 1;
                self.open = Some(OpenCursor {
                    id,
                    rs,
                    columns,
                    carry: Some(first),
                });
                result.pending = Some(CursorHandle(id));
            }
        } else {
            let t0 = std::time::Instant::now();
            stmt.execute(&[]).map_err(|e| err(&e))?;
            result.timing.push(Stage::Execute, t0.elapsed());
            if stmt.is_dml() {
                result.rows_affected = stmt.row_count().ok();
            } else {
                result.rows_affected = Some(0);
            }
            for p in &req.params {
                if !present.contains(&p.name) || p.direction == Direction::In {
                    continue;
                }
                if p.ty == VarType::RefCursor {
                    match stmt.bind_value::<&str, RefCursor>(p.name.as_str()) {
                        Ok(rc) => {
                            let id = self.next_cursor;
                            self.next_cursor += 1;
                            self.cursors.insert(id, rc);
                            result
                                .out_params
                                .push((p.name.clone(), Value::Cursor(CursorId(id))));
                        }
                        Err(_) => result.out_params.push((p.name.clone(), Value::Null)),
                    }
                } else {
                    let s: Option<String> =
                        stmt.bind_value(p.name.as_str()).map_err(|e| err(&e))?;
                    result
                        .out_params
                        .push((p.name.clone(), Self::coerce(&p.ty, s)));
                }
            }
            // 암묵 결과(DBMS_SQL.RETURN_RESULT)
            if let Ok(Some(mut rc)) = stmt.implicit_result() {
                if let Ok(rs) = rc.query() {
                    let columns = Self::columns(rs.column_info());
                    let mut rows = Vec::new();
                    for row in rs.flatten() {
                        rows.push(Self::row_to_values(&row)?);
                    }
                    result.result_sets.push(ResultSet { columns, rows });
                }
            }
        }
        // DBMS_OUTPUT 플러시 — 프로시저 로그 회수 비용은 별도 단계(사용자 09-14).
        if self.serveroutput {
            let t = std::time::Instant::now();
            result.messages = self.poll_dbms_output();
            let span = result.timing.push(Stage::OutputFlush, t.elapsed());
            span.rows = Some(result.messages.len() as u64);
            span.note = Some("GET_LINE per line".into());
        }
        Ok(result)
    }

    fn fetch_cursor(&mut self, cursor: CursorId) -> Result<ResultSet, DbError> {
        let Some(mut rc) = self.cursors.remove(&cursor.0) else {
            return Err(DbError {
                code: None,
                message: "REF CURSOR는 한 번만 PRINT할 수 있습니다(SQL*Plus 동일)".into(),
                position: None,
            });
        };
        let rs = rc.query().map_err(|e| err(&e))?;
        let columns = Self::columns(rs.column_info());
        let mut rows = Vec::new();
        for row in rs {
            let row = row.map_err(|e| err(&e))?;
            rows.push(Self::row_to_values(&row)?);
        }
        Ok(ResultSet { columns, rows })
    }

    fn cursor_supported(&self) -> bool {
        true
    }

    fn fetch_next(&mut self, h: CursorHandle, max: usize) -> Result<(ResultSet, bool), DbError> {
        let Some(cur) = self.open.as_mut().filter(|c| c.id == h.0) else {
            return Err(DbError {
                code: None,
                message: format!("cursor #{} is not open", h.0),
                position: None,
            });
        };
        let r = Self::read_batch(&mut cur.rs, max, cur.carry.take());
        match r {
            Ok((rows, peek)) => {
                let more = peek.is_some();
                let columns = cur.columns.clone();
                cur.carry = peek;
                if !more {
                    self.open = None; // 끝 = 문장 핸들 해제
                }
                Ok((ResultSet { columns, rows }, more))
            }
            Err(e) => {
                self.open = None;
                Err(e)
            }
        }
    }

    fn close_cursor(&mut self, h: CursorHandle) -> Result<(), DbError> {
        if self.open.as_ref().is_some_and(|c| c.id == h.0) {
            self.open = None;
        }
        Ok(())
    }

    fn commit(&mut self) -> Result<(), DbError> {
        self.open = None; // 커밋/롤백 = 커서 닫기(docs/43 · ORA-01002 방지)
        self.conn.commit().map_err(|e| err(&e))
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        self.open = None;
        self.conn.rollback().map_err(|e| err(&e))
    }
}
