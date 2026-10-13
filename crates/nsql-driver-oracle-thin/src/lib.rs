//! Oracle **Thin** 어댑터 — Oracle 공식 순수 Rust 드라이버 `oracledb`(TNS/TTC 프로토콜 직접 · **Instant Client 불필요**).
//! `nsql_core::Session` 구현 · docs/110.
//!
//! ★ OCI 어댑터(`nsql-driver-oracle`)와 **같은 계약**(110 §3-3)을 지킨다 — 위층(엔진·러너·카탈로그·GUI·CLI)은 두 어댑터를 구별하지
//! 못한다. 값·타입 글(`Value` · `Column.type_name`)도 OCI와 같은 꼴로 만든다(ODPI-C `OracleType` Display · `Timestamp` Display 거울).
//!
//! Thin이 **못 하는 것**(110 §6)은 [`Session::limits`]로 알리고, 실제로 요청됐을 때는 [`ExecResult::limits_hit`]에 적는다
//! (호스트가 Warning 토스트 · 사용자 10-13):
//! - 실행 중 즉시 취소(`cancel_handle` = `None` → 호스트 = 배치 경계 멈춤 + 두 번째 ■ 강제 중지)
//! - 암묵 결과 집합(`DBMS_SQL.RETURN_RESULT`) · OS/외부 인증 · NNE · 11g · 배열 DML 적재(1차 = 다중 행 INSERT 폴백)
//!
//! 어댑터 안에서 흡수하는 Thin 흠(110 §3-4): 빈 SQL(`todo!()`) · 문장 캐시 0(파싱 실패 고착) · LOB 인라인 페치(한글 깨짐 회피) ·
//! 트리거 DDL은 바인드 없이 · 오류 = 서버 ORA 번호만 `code`에(클라이언트 끊김은 `None` + 분류기 낱말).

use nsql_core::{
    BindParam, Caps, Column, CursorHandle, CursorId, DbError, Dialect, Direction, DriverLimit,
    ExecRequest, ExecResult, ResultSet, Session, Stage, Value, VarType,
};
use nsql_script::ConnectSpec;
use oracledb::{Config, Connection, Cursor, ErrorKind, Metadata, Row, Statement};
use std::cell::Cell;
use std::collections::HashMap;
use std::str::FromStr;

/// 표시용 드라이버 이름(영어 · 프로그램 내부 정보).
pub const DRIVER_NAME: &str = "Oracle Thin";

/// Thin의 제약(110 §6) — 접속 직후 호스트가 한 번 안내한다.
pub const LIMITS: &[DriverLimit] = &[
    DriverLimit::Cancel,
    DriverLimit::ImplicitResults,
    DriverLimit::ExternalAuth,
    DriverLimit::NativeEncryption,
    DriverLimit::BulkLoad,
    DriverLimit::OldServer,
];

/// 호출 상한(초 · 0 = 없음) — 호스트가 설정에서 넣는다 · 다음 접속부터(OCI 어댑터와 같은 전역 · 110 §3-2 "두 드라이버 전역에 다").
static CALL_TIMEOUT_SECS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 설정 `session.call_timeout_secs`(docs/53).
pub fn set_call_timeout_secs(secs: u64) {
    CALL_TIMEOUT_SECS.store(secs, std::sync::atomic::Ordering::Relaxed);
}

/// `TNS_ADMIN`(tnsnames.ora 폴더) — 호스트의 Oracle 클라이언트 설정(`oracle.tns_admin` · 직접 지정일 때만 값) · 비면 드라이버 기본
/// (환경 변수 `TNS_ADMIN` → `ORACLE_HOME/network/admin`).
static TNS_ADMIN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// 설정 `oracle.tns_admin` → Thin(접속마다 읽는다 · 재시작 불필요).
pub fn set_tns_admin(dir: Option<String>) {
    if let Ok(mut g) = TNS_ADMIN.lock() {
        *g = dir.filter(|d| !d.trim().is_empty());
    }
}

fn tns_admin() -> Option<String> {
    TNS_ADMIN.lock().ok().and_then(|g| g.clone())
}

/// 한 묶음 = (읽은 행, 엿본 행 — `Some`이면 뒤에 더 있다).
type Batch = (Vec<Vec<Value>>, Option<Vec<Value>>);

/// 열린 서버 커서(추가 페치용 · docs/43 D-70) — `Cursor`가 문장 핸들을 소유한다(drop = 커서 닫힘). 조회 문장의 커서와 OUT으로
/// 돌아온 REF CURSOR가 같은 꼴이다(Thin `Cursor` = 수명 매개변수 없음 · 지연 `Iterator`).
struct OpenCursor {
    id: u32,
    cur: Cursor,
    columns: Vec<Column>,
    /// 상한에서 엿본 다음 행(다음 `fetch_next`의 첫 행).
    carry: Option<Vec<Value>>,
}

#[allow(missing_debug_implementations)]
pub struct ThinSession {
    conn: Connection,
    /// OUT으로 돌아온 REF CURSOR(1회 `PRINT`).
    cursors: HashMap<u64, Cursor>,
    next_cursor: u64,
    serveroutput: bool,
    fetch_size: u32,
    /// 페치 상한(0 = 무제한 · 세션 옵션 `max_rows`). 커서 지원 세션이므로 호스트는 상한을 그대로 넘긴다(초과 판정은 peek).
    max_rows: usize,
    /// 추가 페치 커서(세션당 1개).
    open: Option<OpenCursor>,
    next_open: u32,
    description: String,
    /// 세션 시간대 오프셋(시, 분 · 접속 직후 1회 조회): `TIMESTAMP WITH LOCAL TIME ZONE`은 서버가 UTC로 보내고 Thin이 그대로 두므로
    /// OCI처럼 세션 시간대 시각 + 오프셋 라벨로 바꿀 때 쓴다(10-13 실서버 대조).
    tz: (i8, i8),
    /// 끊김을 본 적이 있는가(`is_alive`는 `&self` · 단일 스레드라 `Cell`) — 끊김 네 종류를 본 실행 경로가 내리고 정상 실행이 올린다.
    alive: Cell<bool>,
}

impl ThinSession {
    /// `user/pass@host:port/service` · `user/pass@tns` · 외부 인증(`/ as sysdba`)은 **거부**(Thin 제약 · 110 §6 #3).
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
        if user.is_empty() && pass.is_empty() {
            return Err(DbError {
                code: None,
                message: format!(
                    "{DRIVER_NAME}: OS/external authentication (\"/ as sysdba\", Kerberos, wallet) is not supported \
                     (driver limitation: {}) - enter a user name and password, or choose the Instant Client (OCI) driver \
                     in Settings > DBMS > Oracle",
                    DriverLimit::ExternalAuth.code()
                ),
                position: None,
            });
        }
        if target.is_empty() {
            return Err(DbError {
                code: None,
                message: format!("{DRIVER_NAME}: no host or TNS alias in the connect string"),
                position: None,
            });
        }
        let mut cfg = Config::default()
            .set_credentials(&user, pass)
            .set_driver_name("Nexa SQL (oracledb thin)")
            // ★ 문장 캐시 끔(110 §3-4): 파싱 실패가 캐시에 남아 되풀이되는 결함(#51) 회피 · IDE는 같은 글을 다시 실행하는 패턴.
            .set_stmtcachesize(0);
        if let Some(dir) = tns_admin() {
            cfg = cfg.set_config_dir(&dir);
        }
        match spec.role.as_deref() {
            Some("SYSDBA") => cfg = cfg.set_auth_mode(oracledb::AUTH_MODE_SYSDBA),
            Some("SYSOPER") => cfg = cfg.set_auth_mode(oracledb::AUTH_MODE_SYSOPER),
            _ => {}
        }
        let cfg = cfg.set_connect_string(&target).map_err(|e| err(&e))?;
        let trace = std::env::var_os("NSQL_TRACE_CONNECT").is_some();
        let t0 = std::time::Instant::now();
        let conn = oracledb::connect(cfg).map_err(|e| with_limit_hint(err(&e)))?;
        if trace {
            eprintln!(
                "[connect] oracle thin connect {:.1}ms",
                t0.elapsed().as_secs_f64() * 1000.0
            );
        }
        // 호출 상한(설정 `session.call_timeout_secs` · docs/53 §2).
        let secs = CALL_TIMEOUT_SECS.load(std::sync::atomic::Ordering::Relaxed);
        if secs > 0 {
            let _ = conn.set_call_timeout(Some(std::time::Duration::from_secs(secs)));
        }
        // 세션 시간대(드라이버가 접속 때 OS 지역 시간대로 `ALTER SESSION SET TIME_ZONE`을 보낸다) — 숫자 오프셋으로 1회(26 §8: 접속당 1 왕복).
        let tz = conn
            .query_row(
                "SELECT TO_CHAR(SYSTIMESTAMP AT TIME ZONE SESSIONTIMEZONE, 'TZH:TZM') FROM dual",
                &[],
            )
            .ok()
            .and_then(|r| r.get::<String>(0).ok())
            .and_then(|s| parse_tz_offset(&s))
            .unwrap_or((0, 0));
        Ok(ThinSession {
            conn,
            tz,
            cursors: HashMap::new(),
            next_cursor: 1,
            serveroutput: false,
            fetch_size: 500,
            max_rows: 0,
            open: None,
            next_open: 1,
            description: spec.redacted(),
            alive: Cell::new(true),
        })
    }

    /// 드라이버 오류 → `DbError` + 끊김이면 `alive`를 내린다(한 자리).
    fn fail(&self, e: &oracledb::Error) -> DbError {
        if is_dead(e) {
            self.alive.set(false);
        }
        err(e)
    }

    /// ★ LOB 열의 진짜 타입 글(CLOB·BLOB·NCLOB · 10-13 실서버 대조): 인라인 LOB 페치는 드라이버가 정의 메타를 `LONG`·`LONG RAW`·
    /// `LONG NVARCHAR`로 바꿔 원래 타입을 잃는다(`Metadata::define_metadata`). LOB을 `Lob`으로 받는 길은 한글 깨짐(#18)이라 쓰지 않고,
    /// **LONG 계열 열이 있을 때만** `fetch_lobs` 문장으로 **파싱만** 1회(왕복 1 · 값·로케이터 없음) 해 열별 타입 글을 얻는다.
    /// 실패 = 인라인 메타 그대로(LONG). REF CURSOR 결과는 SQL이 없어 보정하지 못한다(110 §6).
    fn lob_type_names(&self, sql: &str) -> Option<Vec<String>> {
        let mut stmt = self
            .conn
            .statement(sql)
            .ok()?
            .fetch_lobs()
            .exclude_from_cache()
            .build()
            .ok()?;
        stmt.ensure_fully_parsed().ok()?;
        Some(stmt.out_metadata().iter().map(type_name).collect())
    }

    /// 인라인 LOB 페치가 LONG 계열로 바꾼 열이 있는가.
    fn has_long_like(meta: &[Metadata]) -> bool {
        meta.iter().any(|m| {
            matches!(
                m.db_type().name(),
                "DB_TYPE_LONG" | "DB_TYPE_LONG_RAW" | "DB_TYPE_LONG_NVARCHAR"
            )
        })
    }

    /// 열 메타 → `Column`(이름 · OCI `OracleType` Display와 같은 타입 글 = [`type_name`]).
    fn columns(meta: &[Metadata]) -> Vec<Column> {
        meta.iter()
            .map(|m| Column {
                name: m.name().to_string(),
                type_name: type_name(m),
            })
            .collect()
    }

    /// 한 행 → 값들(OCI `row_to_values` 거울 · 110 §3-3 "값 표현 같은 글 꼴").
    fn row_to_values(row: &Row, tz: (i8, i8)) -> Result<Vec<Value>, DbError> {
        let meta = row.columns();
        let mut out = Vec::with_capacity(meta.len());
        for (i, m) in meta.iter().enumerate() {
            out.push(cell_value(row, i, m, tz)?);
        }
        Ok(out)
    }

    /// 소유 커서에서 `max`행(0 = 끝까지)을 읽고 상한에 닿았으면 한 행을 더 엿본다 — (읽은 행, 엿본 행).
    fn read_batch(
        cur: &mut Cursor,
        max: usize,
        carry: Option<Vec<Value>>,
        tz: (i8, i8),
    ) -> Result<Batch, DbError> {
        let mut out: Vec<Vec<Value>> = Vec::new();
        if let Some(c) = carry {
            out.push(c);
        }
        loop {
            if max > 0 && out.len() >= max {
                let peek = match cur.next() {
                    Some(r) => Some(Self::row_to_values(&r.map_err(|e| err(&e))?, tz)?),
                    None => None,
                };
                return Ok((out, peek));
            }
            match cur.next() {
                Some(r) => out.push(Self::row_to_values(&r.map_err(|e| err(&e))?, tz)?),
                None => return Ok((out, None)),
            }
        }
    }

    fn poll_dbms_output(&mut self) -> Vec<String> {
        let mut lines = Vec::new();
        if !self.serveroutput {
            return lines;
        }
        let Ok(builder) = self
            .conn
            .statement("BEGIN DBMS_OUTPUT.GET_LINE(:line, :status); END;")
        else {
            return lines;
        };
        let Ok(mut stmt) = builder.build() else {
            return lines;
        };
        for _ in 0..100_000 {
            let Ok(mut r) = stmt.execute_named(&[
                ("line", &oracledb::DB_TYPE_VARCHAR),
                ("status", &oracledb::DB_TYPE_NUMBER),
            ]) else {
                break;
            };
            let row = r.out_bind_data();
            let status = out_index(&row, "status")
                .and_then(|i| row.get::<Option<oracledb::OracleNumber>>(i).ok())
                .flatten()
                .map_or(1, |n| n.to_string().parse::<i64>().unwrap_or(1));
            if status != 0 {
                break;
            }
            let line = out_index(&row, "line")
                .and_then(|i| row.get::<Option<String>>(i).ok())
                .flatten();
            lines.push(line.unwrap_or_default());
        }
        lines
    }

    /// OUT 바인드 하나를 요청 타입대로 읽어 `Value`로(OCI 어댑터의 OUT 수거 규칙 거울).
    fn out_value(row: &mut Row, idx: usize, p: &BindParam, tz: (i8, i8)) -> Result<Value, DbError> {
        if p.ty == VarType::RefCursor {
            // 커서는 호출자가 맵에 넣는다(여기서는 자리만) — [`Session::execute`]가 처리.
            return Ok(Value::Null);
        }
        let m = &row.columns()[idx];
        if matches!(p.ty, VarType::Date | VarType::Timestamp) && timestamp_of(&p.value).is_some() {
            let ts: Option<oracledb::OracleTimestamp> = row.get(idx).map_err(|e| err(&e))?;
            return Ok(ts.map_or(Value::Null, |t| {
                Value::Str(timestamp_text(&t, if p.ty == VarType::Date { 0 } else { 9 }, false, true))
            }));
        }
        if p.ty == VarType::Boolean && bool_of(&p.value).is_some() {
            let b: Option<bool> = row.get(idx).map_err(|e| err(&e))?;
            return Ok(b.map_or(Value::Null, Value::Bool));
        }
        // 그 밖 = 값을 글로 읽어 변수 타입에 맞춘다(OCI = VARCHAR2 OUT 글 → `coerce` · 여기서는 타입 값의 글).
        let v = cell_value(row, idx, m, tz)?;
        let s = match v {
            Value::Null => None,
            Value::Str(s) => Some(s),
            other => Some(other.display()),
        };
        Ok(coerce(&p.ty, s))
    }
}

impl Session for ThinSession {
    fn dialect(&self) -> Dialect {
        Dialect::Oracle
    }

    fn driver_name(&self) -> String {
        DRIVER_NAME.into()
    }

    fn limits(&self) -> &'static [DriverLimit] {
        LIMITS
    }

    /// 능력표 = Oracle 표 그대로 · 1차는 배열 DML 적재만 뺀다(`oracledb` batch error `todo!()` → abort 위험 · 110 §3-3).
    fn caps(&self) -> Caps {
        Caps {
            bulk_load: nsql_core::BulkLoad::MultiRow {
                max_rows: 500,
                max_params: 2000,
            },
            ..Caps::of(Dialect::Oracle)
        }
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn set_call_timeout(&mut self, timeout: Option<std::time::Duration>) {
        let _ = self.conn.set_call_timeout(timeout);
    }

    fn is_alive(&self) -> bool {
        self.alive.get()
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
                self.conn.execute(sql, &[]).map_err(|e| self.fail(&e))?;
            }
            "fetch_size" => self.fetch_size = value.parse().unwrap_or(500),
            // 기본 스키마 — 세션의 미수식 이름 해석 대상(`?schema=`). 식별자는 따옴표로 감싸 대소문자 그대로.
            "schema" => {
                let sql = format!(
                    "ALTER SESSION SET CURRENT_SCHEMA = \"{}\"",
                    value.replace('"', "")
                );
                self.conn.execute(&sql, &[]).map_err(|e| self.fail(&e))?;
            }
            "max_rows" => self.max_rows = value.parse().unwrap_or(0),
            // `autocommit` = 무시(호출처 0 · Oracle `Caps.server_autocommit = false`라 러너가 COMMIT을 스스로 보낸다 · 110 §3-3).
            _ => {}
        }
        Ok(())
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let mut result = ExecResult::default();
        // ★ 빈 문장 = 드라이버에 보내지 않는다(`oracledb` `todo!()` · 110 §3-4).
        if req.sql.trim().is_empty() {
            result.rows_affected = Some(0);
            return Ok(result);
        }
        let build = |sql: &str| -> Result<Statement, DbError> {
            self.conn
                .statement(sql)
                .map_err(|e| self.fail(&e))?
                .fetch_array_size(self.fetch_size)
                .exclude_from_cache()
                .build()
                .map_err(|e| self.fail(&e))
        };
        let mut stmt = build(&req.sql)?;
        // ★ DDL에 바인드 꼴(`CREATE TRIGGER … :NEW.col` · oracledb #33 · 110 §3-4): 드라이버 파서가 `:NEW`를 바인드로 모아
        //   "missing bind value"가 난다 → PL/SQL `EXECUTE IMMEDIATE q'[…]'`로 감싸 보낸다(문자열 안은 파서가 건너뛴다 · 서버 결과 같음 ·
        //   컴파일 오류는 객체에 남아 러너의 SHOW ERRORS가 읽는다). DDL은 어차피 바인드가 없다(OCI도 ORA-01027).
        let wrapped;
        if stmt.is_ddl() && !stmt.bind_names().is_empty() {
            wrapped = wrap_ddl(&req.sql)?;
            stmt = build(&wrapped)?;
        }
        // 바인드 — 실제로 문장에 있는 이름만(엔진이 추출한 이름과 드라이버 이름은 같아야 하지만 방어). 대문자 비교.
        let present: Vec<String> = if stmt.is_ddl() || stmt.bind_names().is_empty() {
            Vec::new()
        } else {
            stmt.bind_names()
                .iter()
                .map(|n| n.to_ascii_uppercase())
                .collect()
        };
        let mut holders: Vec<(String, Bound)> = Vec::new();
        for p in &req.params {
            if !present.contains(&p.name) {
                continue;
            }
            holders.push((p.name.clone(), Bound::of(p)));
        }
        let named: Vec<(&str, &dyn oracledb::ToDbValue)> = holders
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_dyn()))
            .collect();
        // 암묵 결과 집합은 Thin이 돌려주지 못한다(110 §6 #2) — 요청됐으면 제약으로 적는다(결과 탭 없음 = 오류 없이 빠지므로 안내).
        if contains_ci(&req.sql, "RETURN_RESULT") {
            result.limits_hit.push(DriverLimit::ImplicitResults);
        }
        // 암묵 결과 집합(`DBMS_SQL.RETURN_RESULT`)이 실제로 돌아오면 Thin은 TTC 메시지를 못 읽고 내부 오류를 낸다(세션은 유지) →
        // 그 오류를 **제약 안내**로 바꾼다(호스트는 메시지의 `driver limitation: implicit-results`로 Warning 토스트를 올린다).
        let implicit = contains_ci(&req.sql, "RETURN_RESULT");
        let limit_err = |e: DbError| -> DbError {
            if implicit && e.message.to_ascii_lowercase().contains("unknown ttc message type") {
                DbError {
                    code: None,
                    message: format!(
                        "{DRIVER_NAME}: implicit result sets (DBMS_SQL.RETURN_RESULT) are not supported (driver limitation: {}) - \
                         use a REF CURSOR OUT bind and PRINT, or choose the Instant Client (OCI) driver in Settings > DBMS > Oracle",
                        DriverLimit::ImplicitResults.code()
                    ),
                    position: None,
                }
            } else {
                e
            }
        };
        if stmt.is_query() {
            let t0 = std::time::Instant::now();
            let mut cur = stmt.query_named(&named).map_err(|e| self.fail(&e))?;
            let mut columns = Self::columns(cur.columns());
            if Self::has_long_like(cur.columns()) {
                if let Some(real) = self.lob_type_names(&req.sql) {
                    for (c, t) in columns.iter_mut().zip(real) {
                        if matches!(c.type_name.as_str(), "LONG" | "LONG RAW" | "LONG NVARCHAR") {
                            c.type_name = t;
                        }
                    }
                }
            }
            result.timing.push(Stage::Execute, t0.elapsed());
            let t1 = std::time::Instant::now();
            let (rows, peek) = Self::read_batch(&mut cur, self.max_rows, None, self.tz)?;
            let rs_out = ResultSet {
                columns: columns.clone(),
                rows,
            };
            let span = result.timing.push(Stage::Fetch, t1.elapsed());
            span.rows = Some(rs_out.rows.len() as u64);
            span.bytes = Some(rs_out.approx_bytes());
            span.note = Some(format!("array {}", self.fetch_size));
            result.result_sets.push(rs_out);
            if let Some(first) = peek {
                let id = self.next_open;
                self.next_open += 1;
                self.open = Some(OpenCursor {
                    id,
                    cur,
                    columns,
                    carry: Some(first),
                });
                result.pending = Some(CursorHandle(id));
            }
        } else {
            let t0 = std::time::Instant::now();
            let is_dml = stmt.is_dml();
            let mut r = stmt
                .execute_named(&named)
                .map_err(|e| limit_err(self.fail(&e)))?;
            result.timing.push(Stage::Execute, t0.elapsed());
            result.rows_affected = Some(if is_dml { r.rows_affected() } else { 0 });
            let mut row = r.out_bind_data();
            for p in &req.params {
                if !present.contains(&p.name) || p.direction == Direction::In {
                    continue;
                }
                // ★ 서버가 OUT으로 표시하지 않은 바인드(블록이 **읽기만** 한 `:B`)는 OUT 행에 없다 → **원래 값 그대로**(OCI는 바인드
                //   버퍼가 남아 값이 유지된다 · 10-13 실서버 실험: NULL로 채우면 변수가 지워져 뒤 문장이 전부 NULL이 됐다).
                let Some(idx) = out_index(&row, &p.name) else {
                    result.out_params.push((p.name.clone(), p.value.clone()));
                    continue;
                };
                if p.ty == VarType::RefCursor {
                    match row.take::<Option<Cursor>>(idx) {
                        Ok(Some(rc)) => {
                            let id = self.next_cursor;
                            self.next_cursor += 1;
                            self.cursors.insert(id, rc);
                            result
                                .out_params
                                .push((p.name.clone(), Value::Cursor(CursorId(id))));
                        }
                        _ => result.out_params.push((p.name.clone(), Value::Null)),
                    }
                    continue;
                }
                let v = Self::out_value(&mut row, idx, p, self.tz)?;
                result.out_params.push((p.name.clone(), v));
            }
        }
        self.alive.set(true);
        // DBMS_OUTPUT 플러시 — 프로시저 로그 회수 비용은 별도 단계.
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
        self.fetch_cursor_page(cursor, 0).map(|(rs, _)| rs)
    }

    /// REF CURSOR의 첫 `max`행(0 = 전부) · 남은 행이 있으면 커서를 열린 커서 자리에 남긴다(T-202 · 앞 커서는 대체 = 세션당 1개).
    fn fetch_cursor_page(
        &mut self,
        cursor: CursorId,
        max: usize,
    ) -> Result<(ResultSet, Option<CursorHandle>), DbError> {
        let Some(mut rc) = self.cursors.remove(&cursor.0) else {
            return Err(DbError {
                code: None,
                message: "REF CURSOR can be printed only once (same as SQL*Plus)".into(),
                position: None,
            });
        };
        let columns = Self::columns(rc.columns());
        let (rows, peek) = Self::read_batch(&mut rc, max, None, self.tz)?;
        let out = ResultSet {
            columns: columns.clone(),
            rows,
        };
        let handle = peek.map(|first| {
            let id = self.next_open;
            self.next_open += 1;
            self.open = Some(OpenCursor {
                id,
                cur: rc,
                columns,
                carry: Some(first),
            });
            CursorHandle(id)
        });
        Ok((out, handle))
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
        let carry = cur.carry.take();
        let tz = self.tz;
        match Self::read_batch(&mut cur.cur, max, carry, tz) {
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
        self.conn.commit().map_err(|e| self.fail(&e))
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        self.open = None;
        self.conn.rollback().map_err(|e| self.fail(&e))
    }
}

/// 바인드 값 보관(`ToDbValue`는 외부에서 구현할 수 없어(`ToBuf` 비공개) 드라이버가 아는 타입으로 담는다).
enum Bound {
    Null,
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Bytes(Vec<u8>),
    Number(oracledb::OracleNumber),
    Timestamp(oracledb::OracleTimestamp),
    /// OUT 자리(초기값 없음) — 타입만.
    Out(&'static oracledb::DbType),
}

impl Bound {
    /// OCI 어댑터의 바인드 규칙 거울(110 §3-3): 날짜·불리언은 ISO/불리언 글이면 진짜 타입 · REF CURSOR = 커서 타입 · LOB = 글/바이트
    /// (Thin은 인라인 바인드 · 큰 값은 서버가 LOB로 받는다) · OUT/InOut = 초기값이 있으면 그 값(글) · 없으면 VARCHAR OUT 자리.
    fn of(p: &BindParam) -> Bound {
        match (&p.ty, &p.value) {
            (VarType::Date | VarType::Timestamp, v) => match timestamp_of(v) {
                Some(Some(ts)) => Bound::Timestamp(ts),
                Some(None) => Bound::Out(if p.ty == VarType::Date {
                    oracledb::DB_TYPE_DATE
                } else {
                    oracledb::DB_TYPE_TIMESTAMP
                }),
                None => Bound::text_of(v),
            },
            (VarType::Boolean, v) => match bool_of(v) {
                Some(Some(b)) => Bound::Bool(b),
                Some(None) => Bound::Out(oracledb::DB_TYPE_BOOLEAN),
                None => Bound::text_of(v),
            },
            (VarType::RefCursor, _) => Bound::Out(oracledb::DB_TYPE_CURSOR),
            (VarType::Blob, Value::Bytes(b)) => Bound::Bytes(b.clone()),
            (VarType::Blob, _) => Bound::Out(oracledb::DB_TYPE_BLOB),
            (VarType::Clob, Value::Null) => Bound::Out(oracledb::DB_TYPE_CLOB),
            (VarType::Clob, v) => Bound::Str(v.display()),
            (_, v) if p.direction == Direction::In => match v {
                Value::Null => Bound::Null,
                Value::Int(i) => Bound::Int(*i),
                Value::Float(f) => Bound::Float(*f),
                Value::Bool(b) => Bound::Int(i64::from(*b)),
                Value::Bytes(b) => Bound::Bytes(b.clone()),
                Value::Decimal(d) => oracledb::OracleNumber::from_str(d)
                    .map_or_else(|_| Bound::Str(d.clone()), Bound::Number),
                Value::Str(s) => Bound::Str(s.clone()),
                Value::Cursor(_) => Bound::Out(oracledb::DB_TYPE_CURSOR),
            },
            // InOut/Out(그 밖 타입): OCI = VARCHAR2(4000) + 초기 글. Thin = 초기 글(있으면) · 없으면 VARCHAR OUT 자리(4000).
            (_, Value::Null) => Bound::Out(oracledb::DB_TYPE_VARCHAR),
            (_, v) => Bound::Str(v.display()),
        }
    }

    fn text_of(v: &Value) -> Bound {
        match v {
            Value::Null => Bound::Null,
            other => Bound::Str(other.display()),
        }
    }

    fn as_dyn(&self) -> &dyn oracledb::ToDbValue {
        match self {
            Bound::Null => &NULL_STR,
            Bound::Str(s) => s,
            Bound::Int(i) => i,
            Bound::Float(f) => f,
            Bound::Bool(b) => b,
            Bound::Bytes(b) => b,
            Bound::Number(n) => n,
            Bound::Timestamp(t) => t,
            Bound::Out(t) => t,
        }
    }
}

/// NULL 글자 바인드(VARCHAR NULL).
static NULL_STR: Option<String> = None;

/// DDL 본문을 `BEGIN EXECUTE IMMEDIATE q'<d>…<d>'; END;`로 감싼다 — 본문에 없는 q-string 구분자를 고른다(쌍 `[] {} <> ()` →
/// 홑 `! # | ~ ^ @`). 전부 쓰였으면 오류(사실상 없다).
fn wrap_ddl(sql: &str) -> Result<String, DbError> {
    let body = sql.trim_end().trim_end_matches(';').trim_end();
    const PAIRS: &[(&str, &str)] = &[
        ("[", "]"),
        ("{", "}"),
        ("<", ">"),
        ("(", ")"),
        ("!", "!"),
        ("#", "#"),
        ("|", "|"),
        ("~", "~"),
        ("^", "^"),
        ("@", "@"),
    ];
    let (open, close) = PAIRS
        .iter()
        .find(|(_, c)| !body.contains(&format!("{c}'")))
        .ok_or_else(|| DbError {
            code: None,
            message: format!("{DRIVER_NAME}: cannot wrap DDL - every q-string delimiter appears in the text"),
            position: None,
        })?;
    Ok(format!("BEGIN EXECUTE IMMEDIATE q'{open}{body}{close}'; END;"))
}

/// OUT 바인드 행에서 이름(대소문자 무시)의 자리.
fn out_index(row: &Row, name: &str) -> Option<usize> {
    row.columns()
        .iter()
        .position(|m| m.name().eq_ignore_ascii_case(name))
}

fn contains_ci(hay: &str, needle: &str) -> bool {
    hay.to_ascii_uppercase().contains(needle)
}

/// 열 타입 글 — ODPI-C(kubo `oracle`) `OracleType` Display와 같은 꼴(110 §3-3 · 결과 그리드·편집 판정·복사가 읽는다).
/// 문자 열 크기: ODPI-C = 바이트(`dbSizeInBytes`) · Thin `max_size` = 선언 길이(BYTE 의미면 바이트 · CHAR 의미면 글자 수 — 서버가 보낸
/// 바이트 길이는 드라이버가 RAW만 남기고 버린다) → `max_size`를 쓴다 = BYTE 의미 열(기본 `NLS_LENGTH_SEMANTICS`)은 OCI와 같고,
/// `VARCHAR2(10 CHAR)`만 OCI `VARCHAR2(40)` ↔ Thin `VARCHAR2(10)`(10-13 실서버 대조 · 110 §6 기록 · `buffer_size()`(×4)는 BYTE 열까지 틀림).
fn type_name(m: &Metadata) -> String {
    let t = m.db_type();
    match t.name() {
        "DB_TYPE_VARCHAR" => format!("VARCHAR2({})", m.max_size()),
        "DB_TYPE_NVARCHAR" => format!("NVARCHAR2({})", m.max_size()),
        "DB_TYPE_CHAR" => format!("CHAR({})", m.max_size()),
        "DB_TYPE_NCHAR" => format!("NCHAR({})", m.max_size()),
        "DB_TYPE_RAW" => format!("RAW({})", m.max_size()),
        "DB_TYPE_NUMBER" | "DB_TYPE_BINARY_INTEGER" => {
            let (p, s) = (m.precision(), m.scale());
            if s == -127 {
                // FLOAT(p) — 정밀도 126 = `FLOAT`.
                if p == 0 {
                    "NUMBER".into()
                } else if p == 126 {
                    "FLOAT".into()
                } else {
                    format!("FLOAT({p})")
                }
            } else if p == 0 {
                "NUMBER".into()
            } else if s == 0 {
                format!("NUMBER({p})")
            } else {
                format!("NUMBER({p},{s})")
            }
        }
        "DB_TYPE_TIMESTAMP" => ts_type("TIMESTAMP", "", m.scale()),
        "DB_TYPE_TIMESTAMP_TZ" => ts_type("TIMESTAMP", " WITH TIME ZONE", m.scale()),
        "DB_TYPE_TIMESTAMP_LTZ" => ts_type("TIMESTAMP", " WITH LOCAL TIME ZONE", m.scale()),
        "DB_TYPE_INTERVAL_DS" => {
            let (lf, fs) = (m.precision(), m.scale());
            if lf == 2 && fs == 6 {
                "INTERVAL DAY TO SECOND".into()
            } else {
                format!("INTERVAL DAY({lf}) TO SECOND({fs})")
            }
        }
        "DB_TYPE_INTERVAL_YM" => {
            let lf = m.precision();
            if lf == 2 {
                "INTERVAL YEAR TO MONTH".into()
            } else {
                format!("INTERVAL YEAR({lf}) TO MONTH")
            }
        }
        // DATE · CLOB · BLOB · ROWID · LONG · BOOLEAN · JSON … = 이름 그대로(ODPI-C와 같다).
        _ => m.data_type(),
    }
}

fn ts_type(head: &str, tail: &str, fs: i8) -> String {
    if fs == 6 {
        format!("{head}{tail}")
    } else {
        format!("{head}({fs}){tail}")
    }
}

/// 셀 하나 → `Value`(열 타입으로 분기 · NULL = `Value::Null`).
fn cell_value(row: &Row, i: usize, m: &Metadata, tz: (i8, i8)) -> Result<Value, DbError> {
    let e = |x: oracledb::Error| err(&x);
    let t = m.db_type().name();
    Ok(match t {
        "DB_TYPE_NUMBER" | "DB_TYPE_BINARY_INTEGER" => {
            match row.get::<Option<oracledb::OracleNumber>>(i).map_err(e)? {
                None => Value::Null,
                // FLOAT(p)(= 이진 정밀도 NUMBER · scale -127 · precision > 0) = OCI `OracleType::Float` → `Value::Float`(10-13 실서버 대조:
                // `CAST(3 AS FLOAT)` OCI Float(3.0) / Thin Int(3)). 제약 없는 NUMBER(precision 0)는 글 그대로(Int/Decimal).
                Some(n) if m.scale() == -127 && m.precision() > 0 => {
                    let text = n.to_string();
                    text.parse::<f64>().map_or_else(|_| number_value(&text), Value::Float)
                }
                Some(n) => number_value(&n.to_string()),
            }
        }
        "DB_TYPE_BINARY_DOUBLE" => row
            .get::<Option<f64>>(i)
            .map_err(e)?
            .map_or(Value::Null, Value::Float),
        "DB_TYPE_BINARY_FLOAT" => row
            .get::<Option<f32>>(i)
            .map_err(e)?
            .map_or(Value::Null, |f| Value::Float(f64::from(f))),
        "DB_TYPE_RAW" | "DB_TYPE_LONG_RAW" | "DB_TYPE_BLOB" => row
            .get::<Option<Vec<u8>>>(i)
            .map_err(e)?
            .map_or(Value::Null, Value::Bytes),
        "DB_TYPE_BOOLEAN" => row
            .get::<Option<bool>>(i)
            .map_err(e)?
            .map_or(Value::Null, Value::Bool),
        "DB_TYPE_DATE" | "DB_TYPE_TIMESTAMP" | "DB_TYPE_TIMESTAMP_TZ" | "DB_TYPE_TIMESTAMP_LTZ" => {
            match row.get::<Option<oracledb::OracleTimestamp>>(i).map_err(e)? {
                None => Value::Null,
                Some(ts) => {
                    let digits = if t == "DB_TYPE_DATE" { 0 } else { m.scale().clamp(0, 9) as u32 };
                    if t == "DB_TYPE_TIMESTAMP_LTZ" {
                        // LTZ = UTC + 오프셋 없음으로 온다 → 세션 시간대로 옮기고 OCI처럼 오프셋 라벨까지.
                        let local = oracledb::OracleTimestamp::new_timestamp_tz(
                            ts.year(),
                            ts.month(),
                            ts.day(),
                            ts.hour(),
                            ts.minute(),
                            ts.second(),
                            ts.nanoseconds(),
                            tz.0,
                            tz.1,
                        );
                        Value::Str(timestamp_text(&local, digits, true, false))
                    } else {
                        Value::Str(timestamp_text(&ts, digits, t == "DB_TYPE_TIMESTAMP_TZ", false))
                    }
                }
            }
        }
        "DB_TYPE_INTERVAL_DS" => match row.get::<Option<oracledb::OracleIntervalDS>>(i).map_err(e)? {
            None => Value::Null,
            Some(v) => Value::Str(interval_ds_text(&v, m.precision(), m.scale())),
        },
        "DB_TYPE_INTERVAL_YM" => match row.get::<Option<oracledb::OracleIntervalYM>>(i).map_err(e)? {
            None => Value::Null,
            Some(v) => Value::Str(interval_ym_text(&v, m.precision())),
        },
        "DB_TYPE_CURSOR" => Value::Null,
        // VARCHAR2 · CHAR · CLOB · LONG · ROWID · JSON(글) · 그 밖 = 글.
        _ => match row.get::<Option<String>>(i) {
            Ok(v) => v.map_or(Value::Null, Value::Str),
            // 글로 못 읽는 타입(JSON · VECTOR · OBJECT …) = 타입 이름만.
            Err(_) => Value::Str(format!("<{}>", m.data_type())),
        },
    })
}

/// NUMBER 글 → 정수면 `Int` · 아니면 `Decimal`(글 그대로 · OCI 어댑터와 같은 규칙).
fn number_value(s: &str) -> Value {
    match s.parse::<i64>() {
        Ok(i) => Value::Int(i),
        Err(_) => Value::Decimal(s.to_string()),
    }
}

/// 시각 글 — ODPI-C `Timestamp` Display 거울: `YYYY-MM-DD HH:MM:SS` + 소수 `digits`자리(열 정밀도 · 0 = 없음) + TZ ` ±HH:MM`.
/// `trim` = OUT 바인드 글(OCI 어댑터 `timestamp_text` = 뒤의 0을 뗀다).
/// ★ `TIMESTAMP WITH TIME ZONE`은 서버가 **UTC 시각 + 오프셋**으로 보내고 Thin이 그대로 둔다(OCI는 지역 시각으로 바꿔 준다 ·
///   10-13 실서버 대조 `13:45:10 +09:00` ↔ Thin `04:45:10 +09:00`) → `with_tz`면 오프셋을 더해 지역 시각으로 맞춘다.
fn timestamp_text(t: &oracledb::OracleTimestamp, digits: u32, with_tz: bool, trim: bool) -> String {
    let (mut y, mut mo, mut d, mut h, mut mi) = (
        i64::from(t.year()),
        u32::from(t.month()),
        u32::from(t.day()),
        i64::from(t.hour()),
        i64::from(t.minute()),
    );
    if with_tz {
        let shift = i64::from(t.tz_hour_offset()) * 60 + i64::from(t.tz_minute_offset());
        if shift != 0 {
            let total = h * 60 + mi + shift;
            let day_shift = total.div_euclid(1440);
            let rem = total.rem_euclid(1440);
            (h, mi) = (rem / 60, rem % 60);
            if day_shift != 0 {
                (y, mo, d) = civil_from_days(days_from_civil(y, mo, d) + day_shift);
            }
        }
    }
    let mut s = format!(
        "{y}-{mo:02}-{d:02} {h:02}:{mi:02}:{:02}",
        t.second()
    );
    if trim {
        if t.nanoseconds() != 0 {
            let frac = format!("{:09}", t.nanoseconds());
            s.push('.');
            s.push_str(frac.trim_end_matches('0'));
        }
    } else if digits > 0 {
        let frac = format!("{:09}", t.nanoseconds());
        s.push('.');
        s.push_str(&frac[..digits.min(9) as usize]);
    }
    if with_tz {
        let (h, mi) = (t.tz_hour_offset(), t.tz_minute_offset());
        let sign = if h < 0 || mi < 0 { '-' } else { '+' };
        s.push_str(&format!(" {sign}{:02}:{:02}", h.abs(), mi.abs()));
    }
    s
}

/// `+09:00` · `-05:30` → (시, 분)(부호는 둘 다에 · 순수).
fn parse_tz_offset(s: &str) -> Option<(i8, i8)> {
    let t = s.trim();
    let (sign, rest) = match t.chars().next()? {
        '+' => (1i8, &t[1..]),
        '-' => (-1i8, &t[1..]),
        _ => (1i8, t),
    };
    let (h, m) = rest.split_once(':')?;
    let (h, m) = (h.trim().parse::<i8>().ok()?, m.trim().parse::<i8>().ok()?);
    Some((sign * h, sign * m))
}

/// 날짜 → 1970-01-01 기준 일수(순수 · Howard Hinnant `days_from_civil`).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 일수 → 날짜(`days_from_civil`의 역).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `INTERVAL DAY TO SECOND` 글 — ODPI-C `IntervalDS` Display 거울(`+DD HH:MM:SS.ffffff` · 일 자릿수 = 선행 정밀도 · 소수 = 초 정밀도).
fn interval_ds_text(v: &oracledb::OracleIntervalDS, lfprec: u8, fsprec: i8) -> String {
    let neg = v.days() < 0 || v.hours() < 0 || v.minutes() < 0 || v.seconds() < 0 || v.nanoseconds() < 0;
    let mut s = String::from(if neg { "-" } else { "+" });
    let days = v.days().abs();
    s.push_str(&match lfprec {
        2..=9 => format!("{:0width$}", days, width = lfprec as usize),
        _ => days.to_string(),
    });
    s.push_str(&format!(
        " {:02}:{:02}:{:02}",
        v.hours().abs(),
        v.minutes().abs(),
        v.seconds().abs()
    ));
    if (1..=9).contains(&fsprec) {
        let frac = format!("{:09}", v.nanoseconds().abs());
        s.push('.');
        s.push_str(&frac[..fsprec as usize]);
    }
    s
}

/// `INTERVAL YEAR TO MONTH` 글 — ODPI-C `IntervalYM` Display 거울(`+YY-MM`).
fn interval_ym_text(v: &oracledb::OracleIntervalYM, lfprec: u8) -> String {
    let neg = v.years() < 0 || v.months() < 0;
    let years = v.years().abs();
    let y = match lfprec {
        2..=9 => format!("{:0width$}", years, width = lfprec as usize),
        _ => years.to_string(),
    };
    format!("{}{y}-{:02}", if neg { "-" } else { "+" }, v.months().abs())
}

/// 값 → 바인드할 시각. `Some(None)` = NULL · `None` = ISO 꼴이 아니다(호출자는 글자 바인드로 물러난다).
fn timestamp_of(v: &Value) -> Option<Option<oracledb::OracleTimestamp>> {
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

/// OUT 글자 → 값(OCI 어댑터 `coerce` 거울): NUMBER 타입에 숫자 꼴은 Int/Decimal · 숫자 아닌 글은 Str · Auto는 정수면 Int 아니면 Str.
fn coerce(ty: &VarType, s: Option<String>) -> Value {
    match s {
        None => Value::Null,
        Some(s) => match ty {
            VarType::Number | VarType::BinaryFloat | VarType::BinaryDouble => match s.parse::<i64>() {
                Ok(i) => Value::Int(i),
                Err(_) if s.trim().parse::<f64>().is_ok() => Value::Decimal(s),
                Err(_) => Value::Str(s),
            },
            VarType::Auto => match s.parse::<i64>() {
                Ok(i) => Value::Int(i),
                Err(_) => Value::Str(s),
            },
            _ => Value::Str(s),
        },
    }
}

/// ISO 꼴 시각(`YYYY-MM-DD[ |T]HH:MM[:SS[.fff…]]` · 날짜만도 허용)을 읽는다 — 그 밖의 꼴(NLS 형식 글)은 `None`.
fn parse_iso_timestamp(s: &str) -> Option<oracledb::OracleTimestamp> {
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
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 59 {
        return None;
    }
    Some(oracledb::OracleTimestamp::new_timestamp(
        y as i16, mo as u8, d as u8, h as u8, mi as u8, sec as u8, ns,
    ))
}

/// 불리언 글자(`true`/`false` · `t`/`f` · `y`/`n` · `yes`/`no` · `on`/`off` · `1`/`0`).
fn parse_bool_text(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "y" | "yes" | "on" | "1" => Some(true),
        "false" | "f" | "n" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

/// 클라이언트 쪽 끊김 종류(110 §3-3) — `is_alive`를 내린다.
fn is_dead(e: &oracledb::Error) -> bool {
    matches!(
        e.kind(),
        ErrorKind::DeadConnection
            | ErrorKind::NotConnected
            | ErrorKind::UnableToRecover
            | ErrorKind::CallTimeoutExceeded
    )
}

/// 드라이버 오류 → `DbError`. **코드를 지어내지 않는다**(110 §3-3): 서버 ORA 번호가 있으면 그대로 · 클라이언트 쪽 끊김은 `None` +
/// 호스트 분류기(`is_connection_error` 문구 표)가 읽는 낱말(`connection closed` · `not connected` · `timed out`)을 메시지에.
fn err(e: &oracledb::Error) -> DbError {
    match e.kind() {
        ErrorKind::DbError(d) => DbError {
            code: Some(d.code() as i64),
            message: d.message().trim_end().to_string(),
            position: Some(d.offset()).filter(|o| *o > 0),
        },
        ErrorKind::DeadConnection => DbError {
            code: None,
            message: format!("{DRIVER_NAME}: the database or network closed the connection (connection closed)"),
            position: None,
        },
        ErrorKind::NotConnected => DbError {
            code: None,
            message: format!("{DRIVER_NAME}: not connected"),
            position: None,
        },
        ErrorKind::UnableToRecover => DbError {
            code: None,
            message: format!("{DRIVER_NAME}: unable to recover the connection after an error (connection closed)"),
            position: None,
        },
        ErrorKind::CallTimeoutExceeded => DbError {
            code: None,
            message: format!("{DRIVER_NAME}: the call timeout was exceeded (timed out)"),
            position: None,
        },
        ErrorKind::ServerVersionNotSupported => DbError {
            code: None,
            message: format!(
                "{DRIVER_NAME}: this server version is not supported by the Thin driver (12c or later only; \
                 driver limitation: {}) - choose the Instant Client (OCI) driver in Settings > DBMS > Oracle",
                DriverLimit::OldServer.code()
            ),
            position: None,
        },
        _ => DbError {
            code: None,
            message: format!("{DRIVER_NAME}: {e}"),
            position: None,
        },
    }
}

/// 접속 실패에 Thin 제약 안내를 덧붙인다 — NNE 요구(`NSI_NA_REQUIRED`) 등은 드라이버가 일반 오류로 내므로 낱말로 가린다.
fn with_limit_hint(mut e: DbError) -> DbError {
    let low = e.message.to_ascii_lowercase();
    if low.contains("native") && low.contains("encrypt") || low.contains("na_required") {
        e.message.push_str(&format!(
            "\n-> Oracle native network encryption is not supported by the Thin driver (driver limitation: {}) - \
             use TLS, or choose the Instant Client (OCI) driver in Settings > DBMS > Oracle",
            DriverLimit::NativeEncryption.code()
        ));
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerce_mirrors_oci_rules() {
        assert_eq!(coerce(&VarType::Number, Some("12".into())), Value::Int(12));
        assert_eq!(
            coerce(&VarType::Number, Some("1.5".into())),
            Value::Decimal("1.5".into())
        );
        assert_eq!(
            coerce(&VarType::Number, Some("Number to String".into())),
            Value::Str("Number to String".into())
        );
        assert_eq!(coerce(&VarType::Auto, Some("7".into())), Value::Int(7));
        assert_eq!(coerce(&VarType::Auto, Some("x".into())), Value::Str("x".into()));
        assert_eq!(coerce(&VarType::Varchar2(10), Some("5".into())), Value::Str("5".into()));
        assert_eq!(coerce(&VarType::Number, None), Value::Null);
    }

    #[test]
    fn iso_timestamp_text_mirrors_odpi_display() {
        let t = parse_iso_timestamp("2026-09-21 13:45:10.5").expect("iso");
        assert_eq!(timestamp_text(&t, 0, false, false), "2026-09-21 13:45:10");
        assert_eq!(timestamp_text(&t, 6, false, false), "2026-09-21 13:45:10.500000");
        assert_eq!(timestamp_text(&t, 9, false, true), "2026-09-21 13:45:10.5");
        let d = parse_iso_timestamp("2026-01-02").expect("date");
        assert_eq!(timestamp_text(&d, 0, false, false), "2026-01-02 00:00:00");
        assert!(parse_iso_timestamp("26/09/21").is_none());
        assert!(parse_iso_timestamp("2026-13-01").is_none());
        assert!(bool_of(&Value::Str("yes".into())).is_some());
        assert!(bool_of(&Value::Str("maybe".into())).is_none());
    }

    #[test]
    fn timestamp_tz_is_shifted_from_utc_to_local_label() {
        // 서버가 보낸 UTC 04:45 + 오프셋 +09:00 → 지역 13:45(OCI와 같은 글) · 날짜 넘김도.
        let t = oracledb::OracleTimestamp::new_timestamp_tz(2026, 9, 21, 4, 45, 10, 500_000_000, 9, 0);
        assert_eq!(timestamp_text(&t, 9, true, false), "2026-09-21 13:45:10.500000000 +09:00");
        let late = oracledb::OracleTimestamp::new_timestamp_tz(2026, 12, 31, 20, 0, 0, 0, 9, 0);
        assert_eq!(timestamp_text(&late, 0, true, false), "2027-01-01 05:00:00 +09:00");
        let neg = oracledb::OracleTimestamp::new_timestamp_tz(2026, 1, 1, 2, 30, 0, 0, -5, -30);
        assert_eq!(timestamp_text(&neg, 0, true, false), "2025-12-31 21:00:00 -05:30");
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(parse_tz_offset("+09:00"), Some((9, 0)));
        assert_eq!(parse_tz_offset("-05:30"), Some((-5, -30)));
        assert_eq!(parse_tz_offset("Asia/Seoul"), None);
    }

    #[test]
    fn number_value_keeps_decimals_as_text() {
        assert_eq!(number_value("42"), Value::Int(42));
        assert_eq!(number_value("-0.5"), Value::Decimal("-0.5".into()));
        assert_eq!(
            number_value("123456789012345678901234567890"),
            Value::Decimal("123456789012345678901234567890".into())
        );
    }

    #[test]
    fn ddl_with_bind_lookalikes_is_wrapped_in_execute_immediate() {
        let sql = "CREATE OR REPLACE TRIGGER t_bi BEFORE INSERT ON t FOR EACH ROW\nBEGIN :NEW.id := NVL(:NEW.id, 1); END;";
        let w = wrap_ddl(sql).expect("wrap");
        assert!(w.starts_with("BEGIN EXECUTE IMMEDIATE q'["), "{w}");
        assert!(w.ends_with("END]'; END;"), "{w}");
        // 본문에 `]'`가 있으면 다음 구분자.
        let w2 = wrap_ddl("CREATE TRIGGER x BEFORE INSERT ON t FOR EACH ROW BEGIN :NEW.a := q'[a]'; END;").expect("wrap");
        assert!(w2.starts_with("BEGIN EXECUTE IMMEDIATE q'{"), "{w2}");
    }

    #[test]
    fn implicit_results_are_detected_by_text() {
        assert!(contains_ci("begin dbms_sql.return_result(:c); end;", "RETURN_RESULT"));
        assert!(!contains_ci("select 1 from dual", "RETURN_RESULT"));
    }

    #[test]
    fn external_auth_is_refused_before_the_network() {
        let spec = ConnectSpec::parse("/@localhost:1521/x").unwrap_or_default();
        let e = ThinSession::connect(&spec).err().expect("refused");
        assert!(e.message.contains("external-auth"), "{}", e.message);
    }
}
