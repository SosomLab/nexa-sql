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
use oracle::sql_type::{OracleType, RefCursor, Timestamp};
use oracle::{Connection, Connector, InitParams, Privilege, Row};
use std::collections::HashMap;

/// Instant Client 위치를 지정하는 환경변수(선택). 없으면 ODPI-C 기본 탐색(`ORACLE_HOME/lib` · macOS `~/lib`·`/usr/local/lib` · Linux `LD_LIBRARY_PATH` · Windows `PATH`).
pub const CLIENT_DIR_ENV: &str = "NSQL_ORACLE_CLIENT_DIR";

pub mod client;

/// 호스트가 정한 클라이언트 위치(설정 `oracle.client_mode = manual`일 때만 값이 있다 · 자동이면 빈 값).
static CLIENT_CONFIG: std::sync::Mutex<Option<client::ClientConfig>> = std::sync::Mutex::new(None);

/// 설정 `oracle.client_*` → 드라이버(사용자 09-21). ODPI-C는 프로세스에서 한 번만 초기화되므로 **첫 Oracle 접속 전**에 부른 값만
/// 이번 실행에 쓰인다(그 뒤의 변경은 다음 실행부터 — [`loaded_version`]으로 이미 로드됐는지 안다).
pub fn set_client_config(cfg: client::ClientConfig) {
    if let Ok(mut g) = CLIENT_CONFIG.lock() {
        *g = Some(cfg);
    }
}

/// 지금 설정(없으면 자동).
#[must_use]
pub fn client_config() -> client::ClientConfig {
    CLIENT_CONFIG
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// 이미 로드된 클라이언트의 버전(`19.20.0.0.0`) — 아직 Oracle에 접속한 적이 없으면 `None`(물어보려고 로드하지 않는다).
#[must_use]
pub fn loaded_version() -> Option<String> {
    if !InitParams::is_initialized() {
        return None;
    }
    oracle::Version::client().ok().map(|v| v.to_string())
}

/// ODPI-C 초기화 — 첫 접속 전에 1회. **찾은 폴더를 그대로 넘긴다**([`client::detect`] — 설정 창에 보인 것 = 실제로 로드되는 것):
/// 직접 지정한 폴더 · `NSQL_ORACLE_CLIENT_DIR` · `ORACLE_HOME` · 잘 알려진 자리. OS 검색 경로에서 찾은 경우와 못 찾은 경우는
/// 종전처럼 ODPI-C의 기본 탐색에 맡긴다. `TNS_ADMIN`은 직접 지정했거나 클라이언트 폴더에서 찾았을 때만 넘긴다(환경 변수는 OCI가 읽는다).
fn init_client() -> Result<(), DbError> {
    if InitParams::is_initialized() {
        return Ok(());
    }
    let cfg = client_config();
    let report = client::detect(&cfg);
    let mut p = InitParams::new();
    if let Some(dir) = &cfg.client_dir {
        // ★ 직접 지정 = **그 폴더만**: 라이브러리가 없어도 그대로 넘겨 ODPI-C가 그 폴더에서만 찾고 실패하게 한다(DPI-1047) —
        //   넘기지 않으면 기본 탐색(PATH 등)의 다른 클라이언트로 조용히 접속된다(09-21 실서버 테스트가 잡은 결함).
        p.oracle_client_lib_dir(dir.as_os_str())
            .map_err(|e| err(&e))?;
    } else if let (Some(dir), true) = (
        &report.dir,
        matches!(
            report.source,
            client::Source::Setting
                | client::Source::EnvVar
                | client::Source::OracleHome
                | client::Source::WellKnown
        ),
    ) {
        p.oracle_client_lib_dir(dir.as_os_str())
            .map_err(|e| err(&e))?;
    }
    if let (Some(adm), true) = (
        &report.tns_admin,
        matches!(
            report.tns_source,
            client::TnsSource::Setting | client::TnsSource::ClientDir
        ),
    ) {
        p.oracle_client_config_dir(adm.as_os_str())
            .map_err(|e| err(&e))?;
    }
    p.init().map(|_| ()).map_err(|e| err(&e))
}

/// 클라이언트 로드 실패(DPI-1047)에 설치 안내를 덧붙인다.
fn with_client_hint(mut e: DbError) -> DbError {
    if e.message.contains("DPI-1047") || e.message.to_ascii_lowercase().contains("cannot locate") {
        e.message.push_str(&format!(
            "\n→ Oracle Instant Client가 필요합니다. 설치 후 설정 ▸ DBMS ▸ Oracle에서 폴더를 직접 지정하거나({CLIENT_DIR_ENV}=<instantclient 폴더>도 된다) \
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

/// OCI 호출 상한(초 · 0 = 없음) — 호스트가 설정에서 넣는다 · 다음 접속부터.
static CALL_TIMEOUT_SECS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 설정 `session.call_timeout_secs`(docs/53).
pub fn set_call_timeout_secs(secs: u64) {
    CALL_TIMEOUT_SECS.store(secs, std::sync::atomic::Ordering::Relaxed);
}

#[allow(missing_debug_implementations)]
pub struct OracleSession {
    /// `Arc` = 실행 취소 핸들(T-108 · `break_execution`)이 다른 스레드에서 같은 접속을 가리킨다.
    conn: std::sync::Arc<Connection>,
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
        // 비밀번호는 **빌려 쓴다**(사본 없음 — 일회성 비밀번호를 호출자가 접속 직후 덮어써 지운다 · nsql-core `secret`).
        let pass: &str = spec.password.as_deref().unwrap_or("");
        let mut connector = Connector::new(user.as_str(), pass, target.as_str());
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
        // 호출 상한(설정 `session.call_timeout_secs` · docs/53 §2): 죽은 소켓에 보낸 호출이 OS 재전송 한도(분)까지 막히지 않게.
        let secs = CALL_TIMEOUT_SECS.load(std::sync::atomic::Ordering::Relaxed);
        if secs > 0 {
            let _ = conn.set_call_timeout(Some(std::time::Duration::from_secs(secs)));
        }
        Ok(OracleSession {
            conn: std::sync::Arc::new(conn),
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

    /// 값 → 바인드할 시각. `Some(None)` = NULL · `None` = ISO 꼴이 아니다(호출자는 글자 바인드로 물러난다).
    fn timestamp_of(v: &Value) -> Option<Option<Timestamp>> {
        match v {
            Value::Null => Some(None),
            Value::Str(s) => parse_iso_timestamp(s).map(Some),
            _ => None,
        }
    }

    /// 값 → 바인드할 불리언(`Some(None)` = NULL · `None` = 불리언으로 읽을 수 없다).
    fn bool_of(v: &Value) -> Option<Option<bool>> {
        match v {
            Value::Null => Some(None),
            Value::Bool(b) => Some(Some(*b)),
            Value::Int(i) => Some(Some(*i != 0)),
            Value::Str(s) => parse_bool_text(s).map(Some),
            _ => None,
        }
    }

    /// 돌아온 시각의 표시 글자 — 결과 그리드의 DATE/TIMESTAMP 열과 같은 ISO 꼴(`2026-09-21 13:45:10[.123456]`).
    fn timestamp_text(t: &Timestamp, date_only_precision: bool) -> String {
        let base = format!(
            "{}-{:02}-{:02} {:02}:{:02}:{:02}",
            t.year(),
            t.month(),
            t.day(),
            t.hour(),
            t.minute(),
            t.second()
        );
        iso_with_fraction(
            base,
            if date_only_precision {
                0
            } else {
                t.nanosecond()
            },
        )
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

/// OCIBreak — 진행 중 호출은 `ORA-01013`으로 끝난다.
struct OracleCancel(std::sync::Weak<Connection>);

impl nsql_core::CancelHandle for OracleCancel {
    fn cancel(&self) -> Result<(), DbError> {
        match self.0.upgrade() {
            Some(c) => c.break_execution().map_err(|e| err(&e)),
            None => Ok(()), // 접속이 이미 닫혔다
        }
    }
}

impl Session for OracleSession {
    /// OCI 서버 핸들 상태(왕복 0 · `OCI_ATTR_SERVER_STATUS`): 마지막 네트워크 결과·FIN/RST를 반영한다.
    fn is_alive(&self) -> bool {
        !matches!(self.conn.status(), Ok(oracle::ConnStatus::NotConnected))
    }

    fn cancel_handle(&self) -> Option<std::sync::Arc<dyn nsql_core::CancelHandle>> {
        Some(std::sync::Arc::new(OracleCancel(
            std::sync::Arc::downgrade(&self.conn),
        )))
    }

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
            // 기본 스키마 — 세션의 미수식 이름 해석 대상(`?schema=` · 사용자 09-18). 식별자는 따옴표로 감싸 대소문자 그대로.
            "schema" => {
                let sql = format!(
                    "ALTER SESSION SET CURRENT_SCHEMA = \"{}\"",
                    value.replace('"', "")
                );
                self.conn.execute(&sql, &[]).map_err(|e| err(&e))?;
            }
            "max_rows" => self.max_rows = value.parse().unwrap_or(0),
            "autocommit" => {
                // `Arc<Connection>`(취소 핸들은 `Weak`만 쥔다) — 취소가 진행 중인 찰나가 아니면 유일 소유자다.
                let on = value == "true";
                let mut tries = 0;
                loop {
                    if let Some(c) = std::sync::Arc::get_mut(&mut self.conn) {
                        c.set_autocommit(on);
                        break;
                    }
                    tries += 1;
                    if tries > 50 {
                        return Err(DbError {
                            code: None,
                            message: "connection busy (cancel in progress)".into(),
                            position: None,
                        });
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
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
            // ★ 날짜·불리언은 **진짜 타입으로**(T-151 · 09-21 실서버): 글자로 바인드하면 DATE는 세션 NLS 형식으로 바뀌어 시각이
            // 잘리고(`26/09/21`) 다시 쓰면 ORA-01722 · PL/SQL BOOLEAN은 PLS-00306이다. 값이 ISO 꼴이 아니면(사용자가 NLS 형식으로
            // 적은 글) 종전처럼 글자로 보내 서버의 암묵 변환에 맡긴다.
            let native = match &p.ty {
                VarType::Date | VarType::Timestamp => Self::timestamp_of(&p.value).map(|ts| {
                    let oty = if p.ty == VarType::Date {
                        OracleType::Date
                    } else {
                        OracleType::Timestamp(9)
                    };
                    stmt.bind(name, &(&ts, &oty))
                }),
                VarType::Boolean => {
                    Self::bool_of(&p.value).map(|b| stmt.bind(name, &(&b, &OracleType::Boolean)))
                }
                _ => None,
            };
            if let Some(bound) = native {
                bound.map_err(|e| err(&e))?;
                continue;
            }
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
                } else if matches!(p.ty, VarType::Date | VarType::Timestamp)
                    && Self::timestamp_of(&p.value).is_some()
                {
                    let ts: Option<Timestamp> =
                        stmt.bind_value(p.name.as_str()).map_err(|e| err(&e))?;
                    let v = ts.map_or(Value::Null, |t| {
                        Value::Str(Self::timestamp_text(&t, p.ty == VarType::Date))
                    });
                    result.out_params.push((p.name.clone(), v));
                } else if p.ty == VarType::Boolean && Self::bool_of(&p.value).is_some() {
                    let b: Option<bool> = stmt.bind_value(p.name.as_str()).map_err(|e| err(&e))?;
                    result
                        .out_params
                        .push((p.name.clone(), b.map_or(Value::Null, Value::Bool)));
                } else {
                    let s: Option<String> =
                        stmt.bind_value(p.name.as_str()).map_err(|e| err(&e))?;
                    result
                        .out_params
                        .push((p.name.clone(), Self::coerce(&p.ty, s)));
                }
            }
            // 암묵 결과(DBMS_SQL.RETURN_RESULT) — **전부**(프로시저가 커서를 둘 이상 돌려줄 수 있다 · 종전은 첫 장만 · 09-21).
            while let Ok(Some(mut rc)) = stmt.implicit_result() {
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

/// 소수 초를 붙인다(뒤의 0은 뗀다 · 0이면 붙이지 않는다) — 순수.
fn iso_with_fraction(base: String, nanos: u32) -> String {
    if nanos == 0 {
        return base;
    }
    let frac = format!("{nanos:09}");
    format!("{base}.{}", frac.trim_end_matches('0'))
}

/// ISO 꼴 시각(`YYYY-MM-DD[ |T]HH:MM[:SS[.fff…]]` · 날짜만도 허용)을 읽는다 — 그 밖의 꼴(NLS 형식 글)은 `None`.
/// 자리 검사를 먼저 해서 `26/09/21` 같은 글이 엉뚱한 해로 읽히지 않게 한다.
fn parse_iso_timestamp(s: &str) -> Option<Timestamp> {
    let t = s.trim();
    let b = t.as_bytes();
    let digits = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = t.get(r)?;
        (!part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    };
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let (y, mo, d) = (digits(0..4)?, digits(5..7)?, digits(8..10)?);
    let (mut h, mut mi, mut sec, mut ns) = (0, 0, 0, 0);
    if b.len() > 10 {
        if !(b[10] == b' ' || b[10] == b'T') || b.len() < 16 || b[13] != b':' {
            return None;
        }
        h = digits(11..13)?;
        mi = digits(14..16)?;
        if b.len() > 16 {
            if b[16] != b':' || b.len() < 19 {
                return None;
            }
            sec = digits(17..19)?;
            if b.len() > 19 {
                if b[19] != b'.' || b.len() == 20 || b.len() > 29 {
                    return None;
                }
                let frac = digits(20..b.len())?;
                ns = frac * 10u32.pow((29 - b.len()) as u32);
            }
        }
    }
    Timestamp::new(y as i32, mo, d, h, mi, sec, ns).ok()
}

/// 불리언 글자(`true`/`false` · `t`/`f` · `y`/`n` · `yes`/`no` · `on`/`off` · `1`/`0`).
fn parse_bool_text(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "y" | "yes" | "on" | "1" => Some(true),
        "false" | "f" | "n" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    /// T-151 — 시각 글자: ISO 꼴만 읽는다(날짜만 · 분까지 · 초 · 소수 초 1~9자리 · `T`) · NLS 꼴·엉뚱한 글은 물러난다 ·
    /// 표시 글자는 뒤의 0을 뗀다 · 불리언 글자.
    #[test]
    fn iso_timestamp_and_bool_text() {
        let f = |s: &str| {
            super::parse_iso_timestamp(s).map(|t| {
                (
                    t.year(),
                    t.month(),
                    t.day(),
                    t.hour(),
                    t.minute(),
                    t.second(),
                    t.nanosecond(),
                )
            })
        };
        assert_eq!(f("2026-09-21"), Some((2026, 9, 21, 0, 0, 0, 0)));
        assert_eq!(f("2026-09-21 13:45"), Some((2026, 9, 21, 13, 45, 0, 0)));
        assert_eq!(
            f(" 2026-09-21T13:45:10 "),
            Some((2026, 9, 21, 13, 45, 10, 0))
        );
        assert_eq!(
            f("2026-09-21 13:45:10.123456"),
            Some((2026, 9, 21, 13, 45, 10, 123_456_000))
        );
        assert_eq!(
            f("2026-09-21 13:45:10.5"),
            Some((2026, 9, 21, 13, 45, 10, 500_000_000))
        );
        for bad in [
            "26/09/21",
            "21-SEP-26",
            "2026-13-01",
            "2026-09-21 25:00",
            "2026-09-21 13:45:10.",
            "x",
            "",
        ] {
            assert_eq!(f(bad), None, "{bad}");
        }
        assert_eq!(super::iso_with_fraction("a".into(), 0), "a");
        assert_eq!(
            super::iso_with_fraction("a".into(), 123_456_000),
            "a.123456"
        );
        assert_eq!(super::parse_bool_text(" TRUE "), Some(true));
        assert_eq!(super::parse_bool_text("off"), Some(false));
        assert_eq!(super::parse_bool_text("maybe"), None);
    }
}
