//! PostgreSQL 어댑터 — rust-postgres 동기 클라이언트. `nsql_core::Session` 구현. 방언 `Dialect::Postgres`(`$n` 위치 바인드).
//!
//! - 파라미터 없는 문장은 **단순 질의 프로토콜**(`simple_query`) — 모든 셀이 텍스트로 오므로 어떤 타입이든 표시된다
//!   · `CommandComplete`의 행 수를 `rows_affected`로.
//! - 파라미터 있는 문장은 확장 프로토콜(바이너리) — 자주 쓰는 타입은 [`Any`] 디코더가 값으로, 모르는 타입은 16진 문자열.
//! - OUT 파라미터가 없는 방언(함수·프로시저의 OUT은 결과 행으로 온다) — `EXEC :V := expr`는 엔진이 `SELECT expr AS "V"`로
//!   만들고 호스트가 1행 결과의 컬럼 이름을 변수로 흡수한다. `CALL p(...)`의 OUT도 1행 결과.
//! - TLS: 현재 `NoTls`(서버가 `hostssl`만 허용하면 접속 오류에 그대로 드러난다) — rustls 연결은 T-70.
//! - ★ 커서 유지(T-48a · docs/43 §3-3): 동기 `postgres` 크레이트의 Portal은 `Transaction<'_>` 수명에 묶여 세션 구조체에 담을 수
//!   없다(자기 참조). 그래서 psql `FETCH_COUNT`와 같은 **SQL 커서** — `DECLARE nsql_cur NO SCROLL CURSOR WITHOUT HOLD FOR <질의>` +
//!   `FETCH FORWARD n FROM nsql_cur` + `CLOSE`. 커서는 트랜잭션 안에서만 살므로 명시 트랜잭션이 없으면 드라이버가 `BEGIN`을
//!   열고([`PgCursor::own_txn`]) 닫을 때 `COMMIT`한다(docs/43 "자동 커밋 모드에선 결과를 다 읽기 전까지 암묵 트랜잭션 유지").
//!   파라미터 없는 SELECT/WITH/VALUES/TABLE만(단순 질의 프로토콜) · 실서버 검증 전(단위 테스트 = SQL 조립·판정 규칙).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use nsql_core::{
    Column, CursorHandle, CursorId, DbError, Dialect, ExecRequest, ExecResult, ResultSet, Session,
    Value,
};
use nsql_script::ConnectSpec;
use postgres::error::SqlState;
use postgres::types::{FromSql, IsNull, ToSql, Type};
use postgres::{Client, NoTls, SimpleQueryMessage};

/// `RAISE NOTICE`/WARNING 수신부 — 접속 시 `notice_callback`에 심는다. 싱크가 있으면 즉시 전달, 없으면 버퍼(실행 뒤 `messages`).
#[derive(Default)]
struct Notices {
    buf: Vec<String>,
    sink: Option<nsql_core::MessageSink>,
}

/// 서버 커서 이름(세션당 1개).
const CURSOR_NAME: &str = "nsql_cur";

/// 열린 SQL 커서(docs/43 D-70).
#[derive(Debug)]
struct PgCursor {
    id: u32,
    /// 드라이버가 `BEGIN`으로 연 트랜잭션인가(닫을 때 `COMMIT`).
    own_txn: bool,
    columns: Vec<Column>,
    /// 상한에서 엿본 다음 행.
    carry: Option<Vec<Value>>,
}

#[allow(missing_debug_implementations)]
pub struct PgSession {
    client: Client,
    description: String,
    notices: std::sync::Arc<std::sync::Mutex<Notices>>,
    /// 페치 상한(0 = 무제한 · 세션 옵션 `max_rows`).
    max_rows: usize,
    /// 사용자가 `BEGIN`으로 연 트랜잭션 안인가(문장 머리로 추적 · 커서 트랜잭션 소유 판정용).
    in_txn: bool,
    cursor: Option<PgCursor>,
    next_cursor: u32,
}

fn err(e: postgres::Error) -> DbError {
    let (code, message, position) = match e.as_db_error() {
        Some(db) => {
            let pos = match db.position() {
                Some(postgres::error::ErrorPosition::Original(p)) => Some(*p as usize),
                _ => None,
            };
            let mut m = format!("{}: {}", db.code().code(), db.message());
            if let Some(d) = db.detail() {
                m.push_str("\n  ");
                m.push_str(d);
            }
            if let Some(h) = db.hint() {
                m.push_str("\n  HINT: ");
                m.push_str(h);
            }
            (sqlstate_code(db.code()), m, pos)
        }
        None => (None, e.to_string(), None),
    };
    DbError {
        code,
        message,
        position,
    }
}

/// SQLSTATE 5자(예 `42P01`)를 숫자 코드로 — 상위 2자를 36진으로 읽어 분류만 남긴다(표시는 메시지 앞머리에 문자열 그대로).
fn sqlstate_code(s: &SqlState) -> Option<i64> {
    i64::from_str_radix(s.code(), 36).ok()
}

impl PgSession {
    /// `postgres://user:pass@host:5432/db` — 데이터베이스가 없으면 사용자 이름.
    pub fn connect(spec: &ConnectSpec) -> Result<Self, DbError> {
        let mut cfg = postgres::Config::new();
        cfg.host(spec.host.as_deref().unwrap_or("localhost"));
        cfg.port(spec.port.unwrap_or(5432));
        if let Some(u) = &spec.user {
            cfg.user(u);
        }
        if let Some(p) = &spec.password {
            cfg.password(p);
        }
        match (&spec.database, &spec.user) {
            (Some(db), _) => {
                cfg.dbname(db);
            }
            (None, Some(u)) => {
                cfg.dbname(u);
            }
            _ => {}
        }
        cfg.application_name("nexa-sql");
        cfg.connect_timeout(std::time::Duration::from_secs(15));
        let notices = std::sync::Arc::new(std::sync::Mutex::new(Notices::default()));
        let n2 = notices.clone();
        cfg.notice_callback(move |e: postgres::error::DbError| {
            let text = format!("{}: {}", e.severity(), e.message());
            if let Ok(mut n) = n2.lock() {
                match &n.sink {
                    Some(s) => s(text),
                    None => n.buf.push(text),
                }
            }
        });
        let client = cfg.connect(NoTls).map_err(err)?;
        Ok(PgSession {
            client,
            description: spec.redacted(),
            notices,
            max_rows: 0,
            in_txn: false,
            cursor: None,
            next_cursor: 1,
        })
    }
}

// ───────────────────────────────────────────── 값 변환

#[derive(Debug)]
struct Param<'a>(&'a Value);

impl ToSql for Param<'_> {
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut postgres::types::private::BytesMut,
    ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self.0 {
            Value::Null => Ok(IsNull::Yes),
            Value::Int(i) => match *ty {
                Type::INT2 => (*i as i16).to_sql(ty, out),
                Type::INT4 => (*i as i32).to_sql(ty, out),
                Type::FLOAT4 => (*i as f32).to_sql(ty, out),
                Type::FLOAT8 => (*i as f64).to_sql(ty, out),
                Type::INT8 => i.to_sql(ty, out),
                _ => i.to_string().to_sql(ty, out),
            },
            Value::Float(f) => match *ty {
                Type::FLOAT4 => (*f as f32).to_sql(ty, out),
                Type::FLOAT8 => f.to_sql(ty, out),
                _ => f.to_string().to_sql(ty, out),
            },
            Value::Decimal(d) | Value::Str(d) => d.to_sql(ty, out),
            Value::Bool(b) => match *ty {
                Type::BOOL => b.to_sql(ty, out),
                _ => b.to_string().to_sql(ty, out),
            },
            Value::Bytes(b) => b.as_slice().to_sql(ty, out),
            Value::Cursor(_) => Ok(IsNull::Yes),
        }
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }

    postgres::types::to_sql_checked!();
}

/// 어떤 컬럼이든 받는 디코더 — 자주 쓰는 타입은 값으로 · 모르는 타입은 16진 문자열(타입 이름과 함께 표시).
struct Any(Value);

impl<'a> FromSql<'a> for Any {
    fn from_sql(
        ty: &Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let v = match *ty {
            Type::BOOL => Value::Bool(bool::from_sql(ty, raw)?),
            Type::INT2 => Value::Int(i64::from(i16::from_sql(ty, raw)?)),
            Type::INT4 => Value::Int(i64::from(i32::from_sql(ty, raw)?)),
            Type::INT8 => Value::Int(i64::from_sql(ty, raw)?),
            Type::OID => Value::Int(i64::from(u32::from_sql(ty, raw)?)),
            Type::FLOAT4 => Value::Float(f64::from(f32::from_sql(ty, raw)?)),
            Type::FLOAT8 => Value::Float(f64::from_sql(ty, raw)?),
            Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::CHAR | Type::UNKNOWN => {
                Value::Str(String::from_sql(ty, raw)?)
            }
            Type::JSON => Value::Str(String::from_utf8_lossy(raw).into_owned()),
            Type::JSONB => {
                Value::Str(String::from_utf8_lossy(raw.get(1..).unwrap_or(raw)).into_owned())
            }
            Type::BYTEA => Value::Bytes(raw.to_vec()),
            Type::NUMERIC => Value::Decimal(decode_numeric(raw)),
            Type::DATE => Value::Str(decode_date(raw)),
            Type::TIMESTAMP => Value::Str(decode_timestamp(raw, false)),
            Type::TIMESTAMPTZ => Value::Str(decode_timestamp(raw, true)),
            Type::TIME => Value::Str(decode_time(raw)),
            Type::UUID => Value::Str(decode_uuid(raw)),
            _ => Value::Str(format!(
                "{}:{}",
                ty.name(),
                raw.iter().map(|b| format!("{b:02x}")).collect::<String>()
            )),
        };
        Ok(Any(v))
    }

    fn from_sql_null(_ty: &Type) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        Ok(Any(Value::Null))
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }
}

fn be_i16(b: &[u8], at: usize) -> i16 {
    i16::from_be_bytes([b[at], b[at + 1]])
}
fn be_i32(b: &[u8], at: usize) -> i32 {
    i32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn be_i64(b: &[u8], at: usize) -> i64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[at..at + 8]);
    i64::from_be_bytes(a)
}

/// NUMERIC 바이너리(ndigits · weight · sign · dscale · base-10000 자리) → 십진 문자열.
fn decode_numeric(raw: &[u8]) -> String {
    if raw.len() < 8 {
        return String::new();
    }
    let ndigits = be_i16(raw, 0) as usize;
    let weight = be_i16(raw, 2) as i32;
    let sign = be_i16(raw, 4) as u16;
    let dscale = be_i16(raw, 6) as usize;
    if sign == 0xC000 {
        return "NaN".into();
    }
    if raw.len() < 8 + ndigits * 2 {
        return String::new();
    }
    let digits: Vec<i16> = (0..ndigits).map(|i| be_i16(raw, 8 + i * 2)).collect();
    let mut int_part = String::new();
    let mut frac_part = String::new();
    // weight = 최상위 자리(base 10000)의 지수. weight >= 0 이면 정수부에 weight+1 자리.
    let int_groups = if weight >= 0 { weight as usize + 1 } else { 0 };
    for i in 0..int_groups {
        let d = digits.get(i).copied().unwrap_or(0);
        if i == 0 {
            int_part.push_str(&d.to_string());
        } else {
            int_part.push_str(&format!("{d:04}"));
        }
    }
    if int_part.is_empty() {
        int_part.push('0');
    }
    // 소수부: weight < 0 이면 앞에 0000 그룹이 (-weight-1)개.
    let mut idx = int_groups;
    if weight < 0 {
        for _ in 0..(-weight - 1) {
            frac_part.push_str("0000");
        }
    }
    while idx < ndigits {
        frac_part.push_str(&format!("{:04}", digits[idx]));
        idx += 1;
    }
    let mut s = String::new();
    if sign == 0x4000 {
        s.push('-');
    }
    s.push_str(int_part.trim_start_matches('0'));
    if s.is_empty() || s == "-" {
        s.push('0');
    }
    if dscale > 0 {
        frac_part.truncate(dscale);
        while frac_part.len() < dscale {
            frac_part.push('0');
        }
        s.push('.');
        s.push_str(&frac_part);
    }
    s
}

/// 2000-01-01 기준 일수 → `YYYY-MM-DD`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Howard Hinnant의 days_from_civil 역함수(1970 기준) — PG 기준일(2000-01-01 = 10957일)을 더한다.
    let z = days + 10957 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn decode_date(raw: &[u8]) -> String {
    if raw.len() < 4 {
        return String::new();
    }
    let d = be_i32(raw, 0);
    match d {
        i32::MAX => "infinity".into(),
        i32::MIN => "-infinity".into(),
        _ => {
            let (y, m, dd) = civil_from_days(i64::from(d));
            format!("{y:04}-{m:02}-{dd:02}")
        }
    }
}

fn decode_timestamp(raw: &[u8], tz: bool) -> String {
    if raw.len() < 8 {
        return String::new();
    }
    let us = be_i64(raw, 0);
    match us {
        i64::MAX => return "infinity".into(),
        i64::MIN => return "-infinity".into(),
        _ => {}
    }
    let days = us.div_euclid(86_400_000_000);
    let rem = us.rem_euclid(86_400_000_000);
    let (y, m, d) = civil_from_days(days);
    let mut s = format!("{y:04}-{m:02}-{d:02} {}", fmt_time_us(rem));
    if tz {
        s.push_str("+00");
    }
    s
}

fn fmt_time_us(us: i64) -> String {
    let h = us / 3_600_000_000;
    let mi = (us / 60_000_000) % 60;
    let sec = (us / 1_000_000) % 60;
    let frac = us % 1_000_000;
    if frac == 0 {
        format!("{h:02}:{mi:02}:{sec:02}")
    } else {
        let f = format!("{frac:06}");
        format!("{h:02}:{mi:02}:{sec:02}.{}", f.trim_end_matches('0'))
    }
}

fn decode_time(raw: &[u8]) -> String {
    if raw.len() < 8 {
        return String::new();
    }
    fmt_time_us(be_i64(raw, 0))
}

fn decode_uuid(raw: &[u8]) -> String {
    if raw.len() != 16 {
        return String::new();
    }
    let h: Vec<String> = raw.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        h[0..4].concat(),
        h[4..6].concat(),
        h[6..8].concat(),
        h[8..10].concat(),
        h[10..16].concat()
    )
}

// ───────────────────────────────────────────── 실행

/// 결과 행이 있을 법한 문장(확장 프로토콜에서 `query` vs `execute` 선택).
fn returns_rows(sql: &str) -> bool {
    let up = sql.trim_start().to_ascii_uppercase();
    let head = up.split_whitespace().next().unwrap_or("");
    matches!(
        head,
        "SELECT" | "WITH" | "SHOW" | "EXPLAIN" | "VALUES" | "TABLE" | "CALL" | "FETCH"
    ) || up.contains(" RETURNING ")
}

/// 서버 커서로 감쌀 수 있는 문장인가 — 파라미터 없는 SELECT/WITH/VALUES/TABLE(`DECLARE … FOR`가 받는 형태).
fn cursorable(sql: &str) -> bool {
    let up = sql.trim_start().to_ascii_uppercase();
    let head = up.split_whitespace().next().unwrap_or("");
    matches!(head, "SELECT" | "WITH" | "VALUES" | "TABLE") && !up.contains(" INTO ")
}

/// 트랜잭션 경계 문장의 머리 → `Some(true)` = 열림 · `Some(false)` = 닫힘 · `None` = 무관.
fn txn_boundary(sql: &str) -> Option<bool> {
    let up = strip_semicolon(sql).to_ascii_uppercase();
    let mut it = up.split_whitespace();
    let head = it.next().unwrap_or("");
    match head {
        "BEGIN" => Some(true),
        "START" if it.next() == Some("TRANSACTION") => Some(true),
        "COMMIT" | "END" | "ROLLBACK" | "ABORT" => Some(false),
        _ => None,
    }
}

/// 뒤의 `;`를 뗀 본문.
fn strip_semicolon(sql: &str) -> &str {
    let mut s = sql.trim();
    while let Some(rest) = s.strip_suffix(';') {
        s = rest.trim_end();
    }
    s
}

fn declare_sql(sql: &str) -> String {
    format!(
        "DECLARE {CURSOR_NAME} NO SCROLL CURSOR WITHOUT HOLD FOR {}",
        strip_semicolon(sql)
    )
}

/// `n` 0 = 남은 전부.
fn fetch_sql(n: usize) -> String {
    if n == 0 {
        format!("FETCH FORWARD ALL FROM {CURSOR_NAME}")
    } else {
        format!("FETCH FORWARD {n} FROM {CURSOR_NAME}")
    }
}

fn close_sql() -> String {
    format!("CLOSE {CURSOR_NAME}")
}

/// `DECLARE … FOR ` 접두 길이(오류 위치를 원문 기준으로 되돌릴 때).
fn declare_prefix_len() -> usize {
    declare_sql("").len()
}

fn rows_to_result_set(rows: &[postgres::Row]) -> ResultSet {
    let Some(first) = rows.first() else {
        return ResultSet::default();
    };
    let columns: Vec<Column> = first
        .columns()
        .iter()
        .map(|c| Column {
            name: c.name().to_string(),
            type_name: c.type_().name().to_string(),
        })
        .collect();
    let data = rows
        .iter()
        .map(|r| {
            (0..columns.len())
                .map(|i| r.try_get::<_, Any>(i).map_or(Value::Null, |a| a.0))
                .collect()
        })
        .collect();
    ResultSet {
        columns,
        rows: data,
    }
}

impl Session for PgSession {
    fn dialect(&self) -> Dialect {
        Dialect::Postgres
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn set_message_sink(&mut self, sink: Option<nsql_core::MessageSink>) {
        if let Ok(mut n) = self.notices.lock() {
            n.sink = sink;
        }
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let r = self.execute_inner(req);
        if r.is_ok() {
            if let Some(open) = txn_boundary(&req.sql) {
                self.in_txn = open;
                if !open {
                    self.cursor = None; // 사용자의 COMMIT/ROLLBACK = WITHOUT HOLD 커서 소멸
                }
            }
        }
        let mut result = r?;
        if let Ok(mut n) = self.notices.lock() {
            result.messages.append(&mut n.buf);
        }
        Ok(result)
    }

    fn set_option(&mut self, name: &str, value: &str) -> Result<(), DbError> {
        if name == "max_rows" {
            self.max_rows = value.parse().unwrap_or(0);
        }
        Ok(())
    }

    fn cursor_supported(&self) -> bool {
        true
    }

    fn fetch_next(&mut self, h: CursorHandle, max: usize) -> Result<(ResultSet, bool), DbError> {
        if !self.cursor.as_ref().is_some_and(|c| c.id == h.0) {
            return Err(DbError {
                code: None,
                message: format!("cursor #{} is not open", h.0),
                position: None,
            });
        }
        let carry_n = usize::from(self.cursor.as_ref().is_some_and(|c| c.carry.is_some()));
        // 상한+1을 받아 마지막 한 행으로 "더 있음"을 판정(carry 포함).
        let want = if max == 0 {
            0
        } else {
            max + 1 - carry_n.min(max + 1)
        };
        let fetched = match self.simple_rows(&fetch_sql(want)) {
            Ok(x) => x,
            Err(e) => {
                self.close_open_cursor();
                return Err(e);
            }
        };
        let Some(cur) = self.cursor.as_mut() else {
            return Err(DbError {
                code: None,
                message: format!("cursor #{} is not open", h.0),
                position: None,
            });
        };
        let mut rows: Vec<Vec<Value>> = cur.carry.take().into_iter().collect();
        let (cols, data) = fetched;
        if cur.columns.is_empty() {
            cur.columns = cols;
        }
        rows.extend(data);
        let more = max > 0 && rows.len() > max;
        if more {
            cur.carry = rows.pop();
        }
        let columns = cur.columns.clone();
        if !more {
            self.close_open_cursor();
        }
        Ok((ResultSet { columns, rows }, more))
    }

    fn close_cursor(&mut self, h: CursorHandle) -> Result<(), DbError> {
        if self.cursor.as_ref().is_some_and(|c| c.id == h.0) {
            self.close_open_cursor();
        }
        Ok(())
    }

    fn fetch_cursor(&mut self, _cursor: CursorId) -> Result<ResultSet, DbError> {
        Err(DbError {
            code: None,
            message:
                "PostgreSQL: refcursor는 같은 트랜잭션 안에서 FETCH ALL FROM <name>으로 읽으세요"
                    .into(),
            position: None,
        })
    }

    fn commit(&mut self) -> Result<(), DbError> {
        self.close_open_cursor();
        self.in_txn = false;
        self.client.simple_query("COMMIT").map(|_| ()).map_err(err)
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        self.close_open_cursor();
        self.in_txn = false;
        self.client
            .simple_query("ROLLBACK")
            .map(|_| ())
            .map_err(err)
    }
}

impl PgSession {
    /// 단순 질의 1회 → (컬럼, 행) — 첫 결과 집합만(커서 FETCH용).
    fn simple_rows(&mut self, sql: &str) -> Result<(Vec<Column>, Vec<Vec<Value>>), DbError> {
        let msgs = self.client.simple_query(sql).map_err(err)?;
        let mut columns: Vec<Column> = Vec::new();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for m in msgs {
            match m {
                SimpleQueryMessage::RowDescription(desc) => {
                    columns = desc
                        .iter()
                        .map(|c| Column {
                            name: c.name().to_string(),
                            type_name: String::new(),
                        })
                        .collect();
                }
                SimpleQueryMessage::Row(r) => {
                    if columns.is_empty() {
                        columns = r
                            .columns()
                            .iter()
                            .map(|c| Column {
                                name: c.name().to_string(),
                                type_name: String::new(),
                            })
                            .collect();
                    }
                    rows.push(
                        (0..columns.len())
                            .map(|i| r.get(i).map_or(Value::Null, |s| Value::Str(s.to_string())))
                            .collect(),
                    );
                }
                _ => {}
            }
        }
        Ok((columns, rows))
    }

    /// 열린 커서를 닫는다(`CLOSE` · 우리가 연 트랜잭션이면 `COMMIT`) — 오류는 무시(이미 소멸했을 수 있다).
    fn close_open_cursor(&mut self) {
        if let Some(c) = self.cursor.take() {
            let _ = self.client.simple_query(&close_sql());
            if c.own_txn {
                let _ = self.client.simple_query("COMMIT");
            }
        }
    }

    /// 서버 커서로 조회(파라미터 없는 SELECT · 상한 > 0). 상한+1행을 받아 남은 행이 있으면 커서를 열어 둔다.
    fn execute_cursor(&mut self, sql: &str) -> Result<ExecResult, DbError> {
        self.close_open_cursor();
        let own_txn = !self.in_txn;
        if own_txn {
            self.client.simple_query("BEGIN").map_err(err)?;
        }
        if let Err(e) = self.client.simple_query(&declare_sql(sql)) {
            if own_txn {
                let _ = self.client.simple_query("ROLLBACK");
            }
            let mut d = err(e);
            // 오류 위치를 원문 기준으로.
            if let Some(p) = d.position.as_mut() {
                *p = p.saturating_sub(declare_prefix_len());
            }
            return Err(d);
        }
        let max = self.max_rows;
        let (columns, mut rows) = match self.simple_rows(&fetch_sql(max + 1)) {
            Ok(x) => x,
            Err(e) => {
                let _ = self.client.simple_query(&close_sql());
                if own_txn {
                    let _ = self.client.simple_query("ROLLBACK");
                }
                return Err(e);
            }
        };
        let mut result = ExecResult::default();
        if rows.len() > max {
            let carry = rows.pop();
            let id = self.next_cursor;
            self.next_cursor += 1;
            self.cursor = Some(PgCursor {
                id,
                own_txn,
                columns: columns.clone(),
                carry,
            });
            result.pending = Some(CursorHandle(id));
        } else {
            let _ = self.client.simple_query(&close_sql());
            if own_txn {
                let _ = self.client.simple_query("COMMIT");
            }
        }
        result.result_sets.push(ResultSet { columns, rows });
        Ok(result)
    }

    fn execute_inner(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let mut result = ExecResult::default();
        if req.params.is_empty() && self.max_rows > 0 && cursorable(&req.sql) {
            return self.execute_cursor(&req.sql);
        }
        if req.params.is_empty() {
            // 단순 질의 프로토콜 — 텍스트 셀 · 여러 결과 집합 · 행 수.
            let msgs = self.client.simple_query(&req.sql).map_err(err)?;
            let mut columns: Vec<Column> = Vec::new();
            let mut rows: Vec<Vec<Value>> = Vec::new();
            for m in msgs {
                match m {
                    SimpleQueryMessage::Row(r) => {
                        if columns.is_empty() {
                            columns = r
                                .columns()
                                .iter()
                                .map(|c| Column {
                                    name: c.name().to_string(),
                                    type_name: String::new(),
                                })
                                .collect();
                        }
                        rows.push(
                            (0..columns.len())
                                .map(|i| {
                                    r.get(i).map_or(Value::Null, |s| Value::Str(s.to_string()))
                                })
                                .collect(),
                        );
                    }
                    SimpleQueryMessage::CommandComplete(n) => {
                        if !columns.is_empty() {
                            result.result_sets.push(ResultSet {
                                columns: std::mem::take(&mut columns),
                                rows: std::mem::take(&mut rows),
                            });
                        } else {
                            result.rows_affected = Some(n);
                        }
                    }
                    SimpleQueryMessage::RowDescription(desc) => {
                        columns = desc
                            .iter()
                            .map(|c| Column {
                                name: c.name().to_string(),
                                type_name: String::new(),
                            })
                            .collect();
                    }
                    _ => {}
                }
            }
            if !columns.is_empty() || !rows.is_empty() {
                result.result_sets.push(ResultSet { columns, rows });
            }
            return Ok(result);
        }
        let params: Vec<Param<'_>> = req.params.iter().map(|p| Param(&p.value)).collect();
        let refs: Vec<&(dyn ToSql + Sync)> =
            params.iter().map(|p| p as &(dyn ToSql + Sync)).collect();
        if returns_rows(&req.sql) {
            let rows = self.client.query(req.sql.as_str(), &refs).map_err(err)?;
            result.result_sets.push(rows_to_result_set(&rows));
        } else {
            let n = self.client.execute(req.sql.as_str(), &refs).map_err(err)?;
            result.rows_affected = Some(n);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_binary_decodes() {
        // 1234.5 → ndigits 2, weight 0, sign 0, dscale 1, digits [1234, 5000]
        let raw = [0, 2, 0, 0, 0, 0, 0, 1, 0x04, 0xD2, 0x13, 0x88];
        assert_eq!(decode_numeric(&raw), "1234.5");
        // -0.05 → ndigits 1, weight -1, sign 0x4000, dscale 2, digits [500]
        let raw = [0, 1, 0xFF, 0xFF, 0x40, 0, 0, 2, 0x01, 0xF4];
        assert_eq!(decode_numeric(&raw), "-0.05");
        // 0 → ndigits 0 weight 0 sign 0 dscale 0
        assert_eq!(decode_numeric(&[0, 0, 0, 0, 0, 0, 0, 0]), "0");
    }

    #[test]
    fn dates_and_timestamps_decode() {
        assert_eq!(decode_date(&0i32.to_be_bytes()), "2000-01-01");
        assert_eq!(decode_date(&9700i32.to_be_bytes()), "2026-07-23");
        let us: i64 = 9700 * 86_400_000_000 + 3_723_000_500; // 01:02:03.0005
        assert_eq!(
            decode_timestamp(&us.to_be_bytes(), false),
            "2026-07-23 01:02:03.0005"
        );
        assert_eq!(decode_date(&(-1i32).to_be_bytes()), "1999-12-31");
    }

    #[test]
    fn routing_rules() {
        assert!(returns_rows("select 1"));
        assert!(returns_rows("WITH x AS (SELECT 1) SELECT * FROM x"));
        assert!(returns_rows("INSERT INTO t VALUES (1) RETURNING id"));
        assert!(!returns_rows("UPDATE t SET a = $1"));
        assert!(returns_rows("CALL p($1)"));
    }

    /// 커서 유지(T-48a): 어떤 문장을 DECLARE로 감싸는가 · 조립 SQL · 트랜잭션 경계 추적.
    #[test]
    fn cursor_sql_and_rules() {
        assert!(cursorable("select * from t"));
        assert!(cursorable("  WITH x AS (SELECT 1) SELECT * FROM x;"));
        assert!(cursorable("VALUES (1), (2)"));
        assert!(!cursorable("SELECT * INTO t2 FROM t"), "SELECT INTO는 DDL");
        assert!(!cursorable("SHOW search_path"));
        assert!(!cursorable("EXPLAIN SELECT 1"));
        assert!(!cursorable("INSERT INTO t VALUES (1) RETURNING id"));
        assert_eq!(
            declare_sql("select * from t;"),
            "DECLARE nsql_cur NO SCROLL CURSOR WITHOUT HOLD FOR select * from t"
        );
        assert_eq!(fetch_sql(201), "FETCH FORWARD 201 FROM nsql_cur");
        assert_eq!(fetch_sql(0), "FETCH FORWARD ALL FROM nsql_cur");
        assert_eq!(close_sql(), "CLOSE nsql_cur");
        assert_eq!(declare_prefix_len(), 51);
        assert_eq!(txn_boundary("begin"), Some(true));
        assert_eq!(
            txn_boundary("START TRANSACTION ISOLATION LEVEL SERIALIZABLE"),
            Some(true)
        );
        assert_eq!(txn_boundary("start something"), None);
        assert_eq!(txn_boundary("COMMIT;"), Some(false));
        assert_eq!(txn_boundary("rollback"), Some(false));
        assert_eq!(txn_boundary("SELECT 1"), None);
    }
}
