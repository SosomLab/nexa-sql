//! MySQL/MariaDB 어댑터 — `mysql`(blackbeam · 동기 · 순수 Rust) 클라이언트. `nsql_core::Session` 구현 · 방언 `Dialect::Mysql`
//! (`?` 위치 바인드 · 능력표 [`nsql_core::Caps`] `MYSQL`). 135차 win(사용자 10-22 "추가 드라이버 지원").
//!
//! - 바인드가 없는 문장은 **텍스트 프로토콜**(`COM_QUERY`) — 모든 셀이 글자로 오므로 컬럼 타입으로 값을 되살린다([`text_cell`]).
//! - 바인드가 있는 문장은 **준비문**(`COM_STMT_PREPARE/EXECUTE` · 이진 결과) — 서버가 타입을 준다([`bin_cell`]).
//! - OUT 인자가 없는 방언(`OutValues::ResultRow`): `EXEC :V := 식`은 엔진이 `SELECT 식 AS "V"`로 만들고 호스트가 1행을 흡수한다.
//!   `EXEC p(:A, :B)` = `CALL p(?, ?)` — ★ 준비문의 OUT 결과 집합(`SERVER_PS_OUT_PARAMS`)을 `mysql` 크레이트 28이 소화하지 못해
//!   **다음 문장부터 "Packets out of sync"**(탐침 10-22) → 드라이버가 **사용자 변수 길**로 바꾼다([`CallPlan`]): `SET @nsql_i = ?`
//!   (준비문) → 텍스트 `CALL p(@nsql_1, …)` → 서명(`information_schema.parameters` · 세션 캐시 · [`MysqlSession::out_positions`])의
//!   OUT/INOUT 자리만 `SELECT @nsql_i AS 이름` → **마지막 결과 집합 한 행**(서버의 준비문 동작과 같은 모양) → 러너 `capture_row`가
//!   받는다(`CallSignature::Captures` · 러너 쪽 서명 = `nsql_catalog::routine_args` · 같은 순서). 왕복 = 3(+서명 1회).
//! - 취소 = 프로토콜 취소가 없다 → **별도 짧은 접속**으로 `KILL QUERY <connection_id>`(docs/108 §6 T-14 · [`MysqlCancel`]).
//! - 경고(`warnings > 0`) = `SHOW WARNINGS` 1회 → `ExecResult::messages`(MySQL에는 PRINT가 없다 · Note/Warning 줄).
//! - 커서 유지 없음(`cursor_supported = false` · 호스트는 LIMIT/OFFSET 재질의 폴백 · `nsql_io::paging`) · `max_rows` = 상한까지만
//!   담고 나머지는 버린다(결과 집합을 끝까지 읽어야 같은 접속으로 다음 명령을 보낼 수 있다 · 크레이트가 Drop에서 비운다).
//! - TLS(T-388): 모드 = 접속 문자열 `?tls=` > 설정 `mysql.tls`([`set_tls`]) · `preferred`(기본) = TLS 시도 → 서버가 못 하면 평문
//!   (인증 실패 1045는 재시도 안 함) · `required` = 암호화(인증서 검증 없음) · `verify_ca`/`verify_identity` = CA 파일(`mysql.tls_ca` ·
//!   없으면 시스템 루트) · `disabled` = 평문. 스택 = rustls + aws-lc-rs(Oracle Thin과 공유).
//! - 트랜잭션: 서버 자동 커밋(`server_autocommit`) · 수동 커밋은 러너가 `START TRANSACTION`을 먼저 보낸다(`Caps.tx_begin`).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use mysql::consts::{ColumnFlags, ColumnType};
use mysql::prelude::Queryable;
use mysql::{Conn, Opts, OptsBuilder, Params};
use nsql_core::{
    Column, CursorId, DbError, Dialect, ExecRequest, ExecResult, ResultSet, Session, Value,
};
use nsql_script::ConnectSpec;

/// TCP keepalive 유휴 시간(초 · 0 = 끔) — 호스트가 설정 `net.keepalive_secs`에서 넣는다 · 다음 접속부터(docs/53).
static KEEPALIVE_SECS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(60);

pub fn set_keepalive_secs(secs: u64) {
    KEEPALIVE_SECS.store(secs, std::sync::atomic::Ordering::Relaxed);
}

/// ★ TLS 모드(T-388 · 설정 `mysql.tls` · 접속 문자열 `?tls=`가 이긴다).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TlsMode {
    /// 평문.
    Disabled,
    /// TLS 시도 → 서버가 못 하면 평문(기본).
    Preferred,
    /// 암호화만(인증서 검증 없음).
    Required,
    /// CA로 인증서 검증(호스트 이름은 안 봄).
    VerifyCa,
    /// CA + 호스트 이름 검증.
    VerifyIdentity,
}

impl TlsMode {
    /// 설정값·접속 문자열 값 → 모드(별칭 허용 · 모르면 None).
    #[must_use]
    pub fn parse(s: &str) -> Option<TlsMode> {
        match s.trim().to_ascii_lowercase().replace('-', "_").as_str() {
            "disabled" | "disable" | "off" | "none" | "false" => Some(TlsMode::Disabled),
            "preferred" | "prefer" => Some(TlsMode::Preferred),
            "required" | "require" | "on" | "true" => Some(TlsMode::Required),
            "verify_ca" => Some(TlsMode::VerifyCa),
            "verify_identity" | "verify_full" => Some(TlsMode::VerifyIdentity),
            _ => None,
        }
    }

    fn from_u8(v: u8) -> TlsMode {
        match v {
            0 => TlsMode::Disabled,
            2 => TlsMode::Required,
            3 => TlsMode::VerifyCa,
            4 => TlsMode::VerifyIdentity,
            _ => TlsMode::Preferred,
        }
    }
}

static TLS_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);
static TLS_CA: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

/// 전역 TLS 모드 + CA 파일(설정 · 다음 접속부터).
pub fn set_tls(mode: TlsMode, ca: Option<&std::path::Path>) {
    TLS_MODE.store(mode as u8, std::sync::atomic::Ordering::Relaxed);
    *TLS_CA.lock().unwrap_or_else(|e| e.into_inner()) = ca.map(std::path::Path::to_path_buf);
}

fn global_tls() -> (TlsMode, Option<std::path::PathBuf>) {
    (
        TlsMode::from_u8(TLS_MODE.load(std::sync::atomic::Ordering::Relaxed)),
        TLS_CA.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    )
}

/// 모드 → `SslOpts`(`None` = 평문 · 순수).
fn ssl_opts_for(mode: TlsMode, ca: Option<&std::path::Path>) -> Option<mysql::SslOpts> {
    let base = mysql::SslOpts::default();
    let with_ca = |o: mysql::SslOpts| o.with_root_cert_path(ca.map(std::path::Path::to_path_buf));
    match mode {
        TlsMode::Disabled => None,
        TlsMode::Preferred | TlsMode::Required => Some(
            base.with_danger_accept_invalid_certs(true)
                .with_danger_skip_domain_validation(true),
        ),
        TlsMode::VerifyCa => Some(with_ca(base).with_danger_skip_domain_validation(true)),
        TlsMode::VerifyIdentity => Some(with_ca(base)),
    }
}

/// 인증 실패(1045 · 자격 증명)인가 — `preferred`의 평문 재시도 대상이 아니다.
fn is_auth_error(e: &mysql::Error) -> bool {
    matches!(e, mysql::Error::MySqlError(me) if me.code == 1045)
}

/// ★ 보안 접속(TLS) 상한(초): 인증서 검증이 실패하면(CA 없음 · 호스트 이름 불일치) mysql crate + rustls가 서버 응답을
/// 기다리며 멈춘다(10-22 실측 25 s+) → 접속을 스레드로 감싸 시간 안에 못 끝나면 원인을 적은 오류로(스레드는 소켓이 닫히면 끝난다).
const TLS_CONNECT_TIMEOUT_SECS: u64 = 20;

enum TlsConnect {
    Ok(Conn),
    Mysql(mysql::Error),
    Timeout,
}

fn connect_secure(opts: Opts) -> TlsConnect {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("nsql-mysql-tls-connect".into())
        .spawn(move || {
            let _ = tx.send(Conn::new(opts));
        });
    if spawned.is_err() {
        return TlsConnect::Timeout;
    }
    match rx.recv_timeout(std::time::Duration::from_secs(TLS_CONNECT_TIMEOUT_SECS)) {
        Ok(Ok(c)) => TlsConnect::Ok(c),
        Ok(Err(e)) => TlsConnect::Mysql(e),
        Err(_) => TlsConnect::Timeout,
    }
}

fn tls_timeout_error() -> DbError {
    DbError {
        code: None,
        message: format!(
            "TLS handshake did not complete within {TLS_CONNECT_TIMEOUT_SECS}s — the server certificate may have failed verification \
             (set mysql.tls_ca to the server CA file, use tls=verify_ca when the host name does not match, or tls=required to skip verification)"
        ),
        position: None,
    }
}

#[allow(missing_debug_implementations)]
pub struct MysqlSession {
    conn: Conn,
    /// 취소용 재접속 옵션(비밀번호 포함 · 메모리에만).
    opts: Opts,
    description: String,
    /// 페치 상한(0 = 무제한 · 세션 옵션 `max_rows`).
    max_rows: usize,
    /// 소켓 오류를 본 뒤 = 끊김 확실(`is_alive` · 왕복 0).
    broken: bool,
    sink: Option<nsql_core::MessageSink>,
    /// 프로시저 이름(소문자 · `[db.]이름`) → OUT/INOUT 인자 (자리, 이름) — `CALL` 사용자 변수 길의 마지막 집합 구성용.
    sig_cache: std::collections::HashMap<String, Vec<(usize, String)>>,
}

/// `KILL QUERY <id>` — 별도 접속에서(문장은 `1317 Query execution was interrupted`로 끝난다 · 세션은 살아 있다).
struct MysqlCancel {
    opts: Opts,
    id: u32,
}

impl nsql_core::CancelHandle for MysqlCancel {
    fn cancel(&self) -> Result<(), DbError> {
        let mut c = Conn::new(self.opts.clone()).map_err(err)?;
        c.query_drop(kill_sql(self.id)).map_err(err)
    }
}

fn kill_sql(id: u32) -> String {
    format!("KILL QUERY {id}")
}

fn err(e: mysql::Error) -> DbError {
    match e {
        mysql::Error::MySqlError(m) => DbError {
            code: Some(i64::from(m.code)),
            message: format!("{} ({}): {}", m.code, m.state, m.message),
            position: None,
        },
        other => DbError {
            code: None,
            message: other.to_string(),
            position: None,
        },
    }
}

fn is_io(e: &mysql::Error) -> bool {
    matches!(e, mysql::Error::IoError(_) | mysql::Error::CodecError(_))
}

/// MySQL 파서 오류 꼬리 `… at line N`(1부터) → 보낸 문장의 그 줄 **첫 글자**의 글자 인덱스(T-371 · `DbError.position`).
/// 열 정보는 없다(서버가 주지 않는다) → 줄 머리.
fn line_position(message: &str, sql: &str) -> Option<usize> {
    let tail = message.rsplit(" at line ").next()?;
    if tail == message {
        return None;
    }
    let n: usize = tail
        .trim()
        .trim_end_matches(|c: char| !c.is_ascii_digit())
        .parse()
        .ok()?;
    if n == 0 {
        return None;
    }
    let mut line = 1usize;
    let mut idx = 0usize;
    for (i, ch) in sql.chars().enumerate() {
        if line == n {
            idx = i;
            break;
        }
        if ch == '\n' {
            line += 1;
            idx = i + 1;
        }
    }
    (line == n).then_some(idx)
}

impl MysqlSession {
    /// `mysql://user:pass@host:3306/db`(`mariadb://` 별칭) — 데이터베이스는 선택.
    pub fn connect(spec: &ConnectSpec) -> Result<Self, DbError> {
        let ka = KEEPALIVE_SECS.load(std::sync::atomic::Ordering::Relaxed);
        let mut b = OptsBuilder::new()
            .ip_or_hostname(Some(spec.host.as_deref().unwrap_or("localhost")))
            .tcp_port(spec.port.unwrap_or(3306))
            .user(spec.user.as_deref())
            .pass(spec.password.as_deref())
            .db_name(spec.database.as_deref().filter(|d| !d.is_empty()))
            // localhost라도 유닉스 소켓/명명된 파이프를 찾지 않는다(코드스페이스 포트 전달 · 결정적 동작).
            .prefer_socket(false)
            .tcp_connect_timeout(Some(std::time::Duration::from_secs(15)))
            .tcp_nodelay(true)
            .connect_attrs(Some(std::collections::HashMap::from([(
                "program_name".to_string(),
                "nexa-sql".to_string(),
            )])));
        if ka > 0 {
            b = b.tcp_keepalive_time_ms(Some(u32::try_from(ka * 1000).unwrap_or(u32::MAX)));
        }
        // ★ TLS(T-388): 접속 문자열 `?tls=` > 설정 · preferred = TLS 먼저, 서버가 못 하면(핸드셰이크 실패) 평문 한 번 더.
        let (gmode, gca) = global_tls();
        let mode = spec
            .tls
            .as_deref()
            .and_then(TlsMode::parse)
            .unwrap_or(gmode);
        let plain = Opts::from(b);
        let (conn, opts) = match ssl_opts_for(mode, gca.as_deref()) {
            None => (Conn::new(plain.clone()).map_err(err)?, plain),
            Some(ssl) => {
                let secure = Opts::from(OptsBuilder::from_opts(plain.clone()).ssl_opts(ssl));
                match connect_secure(secure.clone()) {
                    TlsConnect::Ok(c) => (c, secure),
                    TlsConnect::Mysql(e) if mode == TlsMode::Preferred && !is_auth_error(&e) => {
                        (Conn::new(plain.clone()).map_err(err)?, plain)
                    }
                    TlsConnect::Mysql(e) => return Err(err(e)),
                    TlsConnect::Timeout => return Err(tls_timeout_error()),
                }
            }
        };
        Ok(MysqlSession {
            conn,
            opts,
            description: spec.redacted(),
            max_rows: 0,
            broken: false,
            sink: None,
            sig_cache: std::collections::HashMap::new(),
        })
    }

    fn note(&mut self, e: &mysql::Error) {
        if is_io(e) {
            self.broken = true;
        }
    }

    fn execute_inner(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let mut result = ExecResult::default();
        let params: Option<Vec<mysql::Value>> = (!req.params.is_empty())
            .then(|| req.params.iter().map(|p| to_mysql(&p.value)).collect());
        let r = match (params, CallPlan::parse(&req.sql)) {
            (Some(p), Some(plan)) => self.run_call(&plan, p, &mut result),
            (params, _) => run_query(&mut self.conn, self.max_rows, &req.sql, params, &mut result),
        };
        if let Err((io, e)) = r {
            self.broken |= io;
            return Err(e);
        }
        // 결과를 다 읽은 뒤의 경고 수는 접속 쪽에 남는다(`QueryResult::warnings`는 그때 0 · 탐침 10-22).
        if self.conn.warnings() > 0 {
            // 경고 본문은 따로 물어야 한다(왕복 1회 · 경고가 있을 때만).
            if let Ok(rows) = self.conn.query::<(String, u32, String), _>("SHOW WARNINGS") {
                for (level, code, msg) in rows {
                    let text = format!("{level} {code}: {msg}");
                    match &self.sink {
                        Some(s) => s(text),
                        None => result.messages.push(text),
                    }
                }
            }
        }
        Ok(result)
    }
}

impl MysqlSession {
    /// `CALL p(…)`의 사용자 변수 길(머리 주석 참고): **모든 인자**를 `@nsql_i`로 옮긴다(`SET`은 준비문 = `?` 값 타입 보존 ·
    /// 상수 인자는 SET 식으로 그대로) → 텍스트 `CALL p(@nsql_1, …)` → OUT/INOUT 자리 전부 `SELECT` = 러너 captures와 자리가 같다.
    /// MySQL은 OUT 자리에 상수를 허용하지 않지만 이 길에서는 PG처럼 된다(값은 버려진다).
    fn run_call(
        &mut self,
        plan: &CallPlan,
        params: Vec<mysql::Value>,
        result: &mut ExecResult,
    ) -> Result<(), (bool, DbError)> {
        let n = plan.args.len();
        let set_sql = plan
            .args
            .iter()
            .enumerate()
            .map(|(i, a)| format!("@nsql_{} = {a}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let mut scratch = ExecResult::default();
        run_query(
            &mut self.conn,
            0,
            &format!("SET {set_sql}"),
            Some(params),
            &mut scratch,
        )?;
        run_query(
            &mut self.conn,
            self.max_rows,
            &plan.call_sql(),
            None,
            result,
        )?;
        let outs = self.out_positions(&plan.name)?;
        let outs: Vec<&(usize, String)> = outs
            .iter()
            .filter(|(pos, _)| *pos >= 1 && *pos <= n)
            .collect();
        if outs.is_empty() {
            return Ok(());
        }
        let cols = outs
            .iter()
            .map(|(pos, name)| format!("@nsql_{pos} AS `{}`", name.replace('`', "``")))
            .collect::<Vec<_>>()
            .join(", ");
        let mut tail = ExecResult::default();
        run_query(
            &mut self.conn,
            0,
            &format!("SELECT {cols}"),
            None,
            &mut tail,
        )?;
        result.result_sets.extend(tail.result_sets);
        Ok(())
    }

    /// 프로시저의 OUT/INOUT 인자 (자리, 이름) — 세션당 이름마다 1회(캐시). 조회 실패 = 오류 그대로(권한 없음 등).
    fn out_positions(&mut self, name: &str) -> Result<Vec<(usize, String)>, (bool, DbError)> {
        let key = name.to_ascii_lowercase();
        if let Some(v) = self.sig_cache.get(&key) {
            return Ok(v.clone());
        }
        let (schema, proc_name) = match key.split_once('.') {
            Some((db, p)) => (format!("'{}'", db.replace('\'', "''")), p.to_string()),
            None => ("DATABASE()".to_string(), key.clone()),
        };
        let sql = format!(
            "SELECT ordinal_position, parameter_name FROM information_schema.parameters \
             WHERE specific_schema = {schema} AND specific_name = '{}' AND routine_type = 'PROCEDURE' \
             AND parameter_mode IN ('OUT', 'INOUT') ORDER BY ordinal_position",
            proc_name.replace('\'', "''")
        );
        let mut r = ExecResult::default();
        run_query(&mut self.conn, 0, &sql, None, &mut r)?;
        let v: Vec<(usize, String)> = r
            .result_sets
            .first()
            .map(|rs| {
                rs.rows
                    .iter()
                    .filter_map(|row| match (row.first(), row.get(1)) {
                        (Some(Value::Int(p)), Some(Value::Str(n))) => {
                            Some((*p as usize, n.clone()))
                        }
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.sig_cache.insert(key, v.clone());
        Ok(v)
    }
}

/// `CALL 이름(인자, …)` 문장의 조각 — 프로시저 이름(백틱 벗김 · `[db.]이름`) + 최상위 인자 글(원문 그대로 · `?` 포함).
#[derive(Debug, PartialEq, Eq)]
struct CallPlan {
    name: String,
    args: Vec<String>,
}

impl CallPlan {
    /// `CALL` 문장이고 인자 목록의 코드 영역에 `?`가 있을 때만. 문자열·주석·백틱·괄호 안의 `,`·`?`는 최상위가 아니다.
    fn parse(sql: &str) -> Option<CallPlan> {
        let t = sql.trim_start();
        let rest = t
            .get(..4)
            .filter(|w| w.eq_ignore_ascii_case("CALL"))
            .map(|_| &t[4..])?;
        if !rest.starts_with(|c: char| c.is_ascii_whitespace()) {
            return None;
        }
        let head = rest.trim_start();
        let name_end = head
            .find(|c: char| c == '(' || c.is_ascii_whitespace())
            .unwrap_or(head.len());
        let name: String = head[..name_end]
            .split('.')
            .map(|p| p.trim_matches('`'))
            .collect::<Vec<_>>()
            .join(".");
        if name.is_empty() {
            return None;
        }
        let after = head[name_end..].trim_start();
        let body = after.strip_prefix('(')?;
        let b = body.as_bytes();
        let mut args: Vec<String> = Vec::new();
        let mut depth = 0i32;
        let mut start = 0usize;
        let mut has_q = false;
        let mut i = 0;
        let mut closed = None;
        while i < b.len() {
            let c = b[i];
            match c {
                b'\'' | b'"' | b'`' => {
                    let mut j = i + 1;
                    while j < b.len() {
                        if b[j] == b'\\' && c != b'`' {
                            j += 2;
                            continue;
                        }
                        if b[j] == c {
                            if j + 1 < b.len() && b[j + 1] == c {
                                j += 2;
                                continue;
                            }
                            break;
                        }
                        j += 1;
                    }
                    i = (j + 1).min(b.len());
                    continue;
                }
                b'#' => {
                    i = b[i..]
                        .iter()
                        .position(|&x| x == b'\n')
                        .map_or(b.len(), |p| i + p);
                    continue;
                }
                b'-' if b.get(i + 1) == Some(&b'-') => {
                    i = b[i..]
                        .iter()
                        .position(|&x| x == b'\n')
                        .map_or(b.len(), |p| i + p);
                    continue;
                }
                b'/' if b.get(i + 1) == Some(&b'*') => {
                    let mut j = i + 2;
                    while j + 1 < b.len() && !(b[j] == b'*' && b[j + 1] == b'/') {
                        j += 1;
                    }
                    i = (j + 2).min(b.len());
                    continue;
                }
                b'(' => depth += 1,
                b')' if depth == 0 => {
                    closed = Some(i);
                    break;
                }
                b')' => depth -= 1,
                b',' if depth == 0 => {
                    args.push(body[start..i].trim().to_string());
                    start = i + 1;
                }
                b'?' if depth == 0 => has_q = true,
                _ => {}
            }
            i += 1;
        }
        let end = closed?;
        let last = body[start..end].trim();
        if !last.is_empty() || !args.is_empty() {
            args.push(last.to_string());
        }
        (has_q && args.iter().all(|a| !a.is_empty())).then_some(CallPlan { name, args })
    }

    /// `CALL 이름(@nsql_1, …, @nsql_n)` — 이름은 원문 인용을 잃지만 백틱이 필요한 이름은 다시 인용한다.
    fn call_sql(&self) -> String {
        let name = self
            .name
            .split('.')
            .map(|p| format!("`{}`", p.replace('`', "``")))
            .collect::<Vec<_>>()
            .join(".");
        let vars = (1..=self.args.len())
            .map(|i| format!("@nsql_{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("CALL {name}({vars})")
    }
}

/// 한 문장 실행 + 결과 전부 읽기 — 바인드 없음 = 텍스트 프로토콜 · 있음 = 준비문. 오류 = (소켓 오류인가, 변환된 오류).
/// `QueryResult`는 이 함수 안에서 끝나므로 호출자가 바로 접속을 다시 쓸 수 있다(경고 수 · 다음 문장).
fn run_query(
    conn: &mut Conn,
    max: usize,
    sql: &str,
    params: Option<Vec<mysql::Value>>,
    result: &mut ExecResult,
) -> Result<(), (bool, DbError)> {
    let wrap = |e: mysql::Error| (is_io(&e), err(e));
    match params {
        None => {
            let mut qr = conn.query_iter(sql).map_err(wrap)?;
            drain(max, &mut qr, result).map_err(|e| (false, e))
        }
        Some(p) => {
            let mut qr = conn.exec_iter(sql, Params::Positional(p)).map_err(wrap)?;
            drain(max, &mut qr, result).map_err(|e| (false, e))
        }
    }
}

/// 결과 집합들을 전부 읽는다(텍스트·이진 공통) — 열 없는 집합 = 영향 행 수 · `max` > 0이면 그만큼만 담고 나머지는 버린다.
fn drain<P: mysql::prelude::Protocol>(
    max: usize,
    qr: &mut mysql::QueryResult<'_, '_, '_, P>,
    result: &mut ExecResult,
) -> Result<(), DbError> {
    {
        loop {
            let Some(set) = qr.iter() else {
                break;
            };
            let cols: Vec<(Column, ColumnType, ColumnFlags, u16)> = set
                .columns()
                .as_ref()
                .iter()
                .map(|c| {
                    (
                        Column {
                            name: c.name_str().into_owned(),
                            type_name: type_name(c.column_type(), c.flags()),
                        },
                        c.column_type(),
                        c.flags(),
                        c.character_set(),
                    )
                })
                .collect();
            if cols.is_empty() {
                result.rows_affected = Some(set.affected_rows());
                continue;
            }
            let mut rows: Vec<Vec<Value>> = Vec::new();
            for row in set {
                let row = row.map_err(err)?;
                if max > 0 && rows.len() >= max {
                    break;
                }
                let vals = row.unwrap_raw();
                rows.push(
                    vals.into_iter()
                        .zip(cols.iter())
                        .map(|(v, (_, ty, flags, cs))| match v {
                            Some(mysql::Value::Bytes(b)) => text_cell(b, *ty, *flags, *cs),
                            Some(v) => bin_cell(v, *ty),
                            None => Value::Null,
                        })
                        .collect(),
                );
            }
            result.result_sets.push(ResultSet {
                columns: cols.into_iter().map(|c| c.0).collect(),
                rows,
            });
        }
        Ok(())
    }
}

// ───────────────────────────────────────────── 값 변환

fn to_mysql(v: &Value) -> mysql::Value {
    match v {
        Value::Null | Value::Cursor(_) => mysql::Value::NULL,
        Value::Int(i) => mysql::Value::Int(*i),
        Value::Float(f) => mysql::Value::Double(*f),
        Value::Decimal(d) => mysql::Value::Bytes(d.as_bytes().to_vec()),
        Value::Str(s) => mysql::Value::Bytes(s.as_bytes().to_vec()),
        Value::Bool(b) => mysql::Value::Int(i64::from(*b)),
        Value::Bytes(b) => mysql::Value::Bytes(b.clone()),
    }
}

/// 문자 집합 63 = `binary`(BINARY/VARBINARY/BLOB) — 글자가 아니라 바이트.
const CS_BINARY: u16 = 63;

fn is_binary(ty: ColumnType, flags: ColumnFlags, cs: u16) -> bool {
    use ColumnType::*;
    cs == CS_BINARY
        && flags.contains(ColumnFlags::BINARY_FLAG)
        && matches!(
            ty,
            MYSQL_TYPE_TINY_BLOB
                | MYSQL_TYPE_MEDIUM_BLOB
                | MYSQL_TYPE_LONG_BLOB
                | MYSQL_TYPE_BLOB
                | MYSQL_TYPE_VAR_STRING
                | MYSQL_TYPE_STRING
                | MYSQL_TYPE_VARCHAR
                | MYSQL_TYPE_GEOMETRY
        )
}

/// 텍스트 프로토콜 셀(글자) → 값: 정수·실수·DECIMAL·BIT는 되살리고 날짜·시각은 글자 그대로 · 이진은 바이트.
fn text_cell(b: Vec<u8>, ty: ColumnType, flags: ColumnFlags, cs: u16) -> Value {
    use ColumnType::*;
    match ty {
        MYSQL_TYPE_TINY | MYSQL_TYPE_SHORT | MYSQL_TYPE_INT24 | MYSQL_TYPE_LONG
        | MYSQL_TYPE_LONGLONG | MYSQL_TYPE_YEAR => {
            let s = String::from_utf8_lossy(&b);
            match s.parse::<i64>() {
                Ok(i) => Value::Int(i),
                Err(_) => Value::Decimal(s.into_owned()), // BIGINT UNSIGNED > i64::MAX
            }
        }
        MYSQL_TYPE_DECIMAL | MYSQL_TYPE_NEWDECIMAL => {
            Value::Decimal(String::from_utf8_lossy(&b).into_owned())
        }
        MYSQL_TYPE_FLOAT | MYSQL_TYPE_DOUBLE => {
            let s = String::from_utf8_lossy(&b);
            s.parse::<f64>()
                .map_or_else(|_| Value::Str(s.into_owned()), Value::Float)
        }
        MYSQL_TYPE_BIT => Value::Int(bits_to_int(&b)),
        _ if is_binary(ty, flags, cs) => Value::Bytes(b),
        _ => Value::Str(String::from_utf8_lossy(&b).into_owned()),
    }
}

/// BIT(n) = 빅엔디언 바이트열 → 정수(최대 64비트).
fn bits_to_int(b: &[u8]) -> i64 {
    let mut v: u64 = 0;
    for x in b.iter().take(8) {
        v = (v << 8) | u64::from(*x);
    }
    i64::try_from(v).unwrap_or(i64::MAX)
}

/// 이진 프로토콜 셀(타입이 있는 값) → 값. `Bytes`는 [`text_cell`]이 받는다(호출자가 가른다).
fn bin_cell(v: mysql::Value, ty: ColumnType) -> Value {
    match v {
        mysql::Value::NULL => Value::Null,
        mysql::Value::Int(i) => Value::Int(i),
        mysql::Value::UInt(u) => {
            i64::try_from(u).map_or_else(|_| Value::Decimal(u.to_string()), Value::Int)
        }
        mysql::Value::Float(f) => Value::Float(f64::from(f)),
        mysql::Value::Double(d) => Value::Float(d),
        mysql::Value::Date(y, m, d, h, i, s, us) => {
            if ty == ColumnType::MYSQL_TYPE_DATE {
                Value::Str(format!("{y:04}-{m:02}-{d:02}"))
            } else {
                Value::Str(fmt_datetime(y, m, d, h, i, s, us))
            }
        }
        mysql::Value::Time(neg, days, h, i, s, us) => Value::Str(fmt_time(neg, days, h, i, s, us)),
        mysql::Value::Bytes(b) => Value::Str(String::from_utf8_lossy(&b).into_owned()),
    }
}

fn fmt_datetime(y: u16, m: u8, d: u8, h: u8, i: u8, s: u8, us: u32) -> String {
    if us > 0 {
        format!("{y:04}-{m:02}-{d:02} {h:02}:{i:02}:{s:02}.{us:06}")
    } else {
        format!("{y:04}-{m:02}-{d:02} {h:02}:{i:02}:{s:02}")
    }
}

fn fmt_time(neg: bool, days: u32, h: u8, i: u8, s: u8, us: u32) -> String {
    let hours = days * 24 + u32::from(h);
    let sign = if neg { "-" } else { "" };
    if us > 0 {
        format!("{sign}{hours:02}:{i:02}:{s:02}.{us:06}")
    } else {
        format!("{sign}{hours:02}:{i:02}:{s:02}")
    }
}

/// 드라이버 원 타입 이름(표시·복사·열 머리 배지 · MySQL 표기).
fn type_name(ty: ColumnType, flags: ColumnFlags) -> String {
    use ColumnType::*;
    let bin = flags.contains(ColumnFlags::BINARY_FLAG);
    let base = match ty {
        MYSQL_TYPE_TINY => "TINYINT",
        MYSQL_TYPE_SHORT => "SMALLINT",
        MYSQL_TYPE_INT24 => "MEDIUMINT",
        MYSQL_TYPE_LONG => "INT",
        MYSQL_TYPE_LONGLONG => "BIGINT",
        MYSQL_TYPE_DECIMAL | MYSQL_TYPE_NEWDECIMAL => "DECIMAL",
        MYSQL_TYPE_FLOAT => "FLOAT",
        MYSQL_TYPE_DOUBLE => "DOUBLE",
        MYSQL_TYPE_NULL => "NULL",
        MYSQL_TYPE_TIMESTAMP | MYSQL_TYPE_TIMESTAMP2 => "TIMESTAMP",
        MYSQL_TYPE_DATE | MYSQL_TYPE_NEWDATE => "DATE",
        MYSQL_TYPE_TIME | MYSQL_TYPE_TIME2 => "TIME",
        MYSQL_TYPE_DATETIME | MYSQL_TYPE_DATETIME2 => "DATETIME",
        MYSQL_TYPE_YEAR => "YEAR",
        MYSQL_TYPE_VARCHAR | MYSQL_TYPE_VAR_STRING => {
            if bin {
                "VARBINARY"
            } else {
                "VARCHAR"
            }
        }
        MYSQL_TYPE_STRING => {
            if flags.contains(ColumnFlags::ENUM_FLAG) {
                "ENUM"
            } else if flags.contains(ColumnFlags::SET_FLAG) {
                "SET"
            } else if bin {
                "BINARY"
            } else {
                "CHAR"
            }
        }
        MYSQL_TYPE_BIT => "BIT",
        MYSQL_TYPE_JSON => "JSON",
        MYSQL_TYPE_ENUM => "ENUM",
        MYSQL_TYPE_SET => "SET",
        MYSQL_TYPE_TINY_BLOB => {
            if bin {
                "TINYBLOB"
            } else {
                "TINYTEXT"
            }
        }
        MYSQL_TYPE_MEDIUM_BLOB => {
            if bin {
                "MEDIUMBLOB"
            } else {
                "MEDIUMTEXT"
            }
        }
        MYSQL_TYPE_LONG_BLOB => {
            if bin {
                "LONGBLOB"
            } else {
                "LONGTEXT"
            }
        }
        MYSQL_TYPE_BLOB => {
            if bin {
                "BLOB"
            } else {
                "TEXT"
            }
        }
        MYSQL_TYPE_GEOMETRY => "GEOMETRY",
        MYSQL_TYPE_VECTOR => "VECTOR",
        _ => "UNKNOWN",
    };
    let numeric = matches!(
        ty,
        MYSQL_TYPE_TINY
            | MYSQL_TYPE_SHORT
            | MYSQL_TYPE_INT24
            | MYSQL_TYPE_LONG
            | MYSQL_TYPE_LONGLONG
            | MYSQL_TYPE_DECIMAL
            | MYSQL_TYPE_NEWDECIMAL
            | MYSQL_TYPE_FLOAT
            | MYSQL_TYPE_DOUBLE
    );
    if numeric && flags.contains(ColumnFlags::UNSIGNED_FLAG) {
        format!("{base} UNSIGNED")
    } else {
        base.to_string()
    }
}

// ───────────────────────────────────────────── Session

impl Session for MysqlSession {
    fn dialect(&self) -> Dialect {
        Dialect::Mysql
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn is_alive(&self) -> bool {
        !self.broken
    }

    fn cancel_handle(&self) -> Option<std::sync::Arc<dyn nsql_core::CancelHandle>> {
        Some(std::sync::Arc::new(MysqlCancel {
            opts: self.opts.clone(),
            id: self.conn.connection_id(),
        }))
    }

    fn set_message_sink(&mut self, sink: Option<nsql_core::MessageSink>) {
        self.sink = sink;
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        self.execute_inner(req).map_err(|mut e| {
            if e.position.is_none() {
                e.position = line_position(&e.message, &req.sql);
            }
            e
        })
    }

    fn set_option(&mut self, name: &str, value: &str) -> Result<(), DbError> {
        match name {
            "max_rows" => self.max_rows = value.parse().unwrap_or(0),
            // 기본 스키마 = 데이터베이스 전환(`?schema=` · 툴바 작업 단위 `USE`와 같은 자리).
            "schema" | "database" if !value.is_empty() => {
                self.conn.select_db(value).map_err(|e| {
                    self.note(&e);
                    err(e)
                })?;
            }
            _ => {}
        }
        Ok(())
    }

    fn fetch_cursor(&mut self, _cursor: CursorId) -> Result<ResultSet, DbError> {
        Err(DbError {
            code: None,
            message: "MySQL: cursors cannot be returned to the client - use a result set".into(),
            position: None,
        })
    }

    fn commit(&mut self) -> Result<(), DbError> {
        self.conn.query_drop("COMMIT").map_err(|e| {
            self.note(&e);
            err(e)
        })
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        self.conn.query_drop("ROLLBACK").map_err(|e| {
            self.note(&e);
            err(e)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_line_to_char_position() {
        let sql = "SELECT 1\nFROM dual\nWHERE x";
        let m = "1064 (42000): You have an error in your SQL syntax; ... near 'dual' at line 2";
        assert_eq!(line_position(m, sql), Some(9));
        assert_eq!(
            line_position("1064 (42000): near 'x' at line 1", sql),
            Some(0)
        );
        assert_eq!(
            line_position("1064 (42000): near 'x' at line 3", sql),
            Some(19)
        );
        assert_eq!(
            line_position("1064 (42000): near 'x' at line 9", sql),
            None,
            "없는 줄"
        );
        assert_eq!(
            line_position("1146 (42S02): Table 'nexa.t' doesn't exist", sql),
            None
        );
        assert_eq!(kill_sql(42), "KILL QUERY 42");
    }

    /// ★ TLS 모드(T-388): 별칭·대소문자·`-` 정규화 · 모르는 값 None · 모드별 SslOpts(disabled = 평문 · 검증 수준 셋).
    #[test]
    fn tls_mode_parse_and_ssl_opts() {
        assert_eq!(TlsMode::parse("REQUIRED"), Some(TlsMode::Required));
        assert_eq!(
            TlsMode::parse("verify-identity"),
            Some(TlsMode::VerifyIdentity)
        );
        assert_eq!(TlsMode::parse("prefer"), Some(TlsMode::Preferred));
        assert_eq!(TlsMode::parse("off"), Some(TlsMode::Disabled));
        assert_eq!(TlsMode::parse("verify_ca"), Some(TlsMode::VerifyCa));
        assert_eq!(TlsMode::parse("bogus"), None);
        assert!(ssl_opts_for(TlsMode::Disabled, None).is_none());
        let ca = std::path::Path::new("C:/certs/ca.pem");
        let req = ssl_opts_for(TlsMode::Required, Some(ca)).expect("required");
        assert!(req.accept_invalid_certs() && req.skip_domain_validation());
        assert!(
            req.root_cert_path().is_none(),
            "required = CA를 보지 않는다"
        );
        let vca = ssl_opts_for(TlsMode::VerifyCa, Some(ca)).expect("verify_ca");
        assert!(!vca.accept_invalid_certs() && vca.skip_domain_validation());
        assert_eq!(
            vca.root_cert_path().map(|p| p.to_path_buf()),
            Some(ca.to_path_buf())
        );
        let vid = ssl_opts_for(TlsMode::VerifyIdentity, Some(ca)).expect("verify_identity");
        assert!(!vid.accept_invalid_certs() && !vid.skip_domain_validation());
        assert_eq!(TlsMode::from_u8(TlsMode::VerifyCa as u8), TlsMode::VerifyCa);
    }

    #[test]
    fn text_cells_restore_types() {
        use ColumnType::*;
        let f = ColumnFlags::empty();
        assert_eq!(
            text_cell(b"42".to_vec(), MYSQL_TYPE_LONG, f, 63),
            Value::Int(42)
        );
        assert_eq!(
            text_cell(
                b"18446744073709551615".to_vec(),
                MYSQL_TYPE_LONGLONG,
                ColumnFlags::UNSIGNED_FLAG,
                63
            ),
            Value::Decimal("18446744073709551615".into())
        );
        assert_eq!(
            text_cell(b"1234.50".to_vec(), MYSQL_TYPE_NEWDECIMAL, f, 63),
            Value::Decimal("1234.50".into())
        );
        assert_eq!(
            text_cell(b"1.5".to_vec(), MYSQL_TYPE_DOUBLE, f, 63),
            Value::Float(1.5)
        );
        assert_eq!(
            text_cell(vec![0x01, 0x02], MYSQL_TYPE_BIT, f, 63),
            Value::Int(258)
        );
        assert_eq!(
            text_cell(b"2026-10-22".to_vec(), MYSQL_TYPE_DATE, f, 63),
            Value::Str("2026-10-22".into())
        );
        // 이진 = 문자 집합 binary + BINARY_FLAG + 글자형 타입.
        assert_eq!(
            text_cell(
                vec![0xff, 0x00],
                MYSQL_TYPE_BLOB,
                ColumnFlags::BINARY_FLAG,
                63
            ),
            Value::Bytes(vec![0xff, 0x00])
        );
        assert_eq!(
            text_cell(
                "한글".as_bytes().to_vec(),
                MYSQL_TYPE_BLOB,
                ColumnFlags::BINARY_FLAG,
                255
            ),
            Value::Str("한글".into()),
            "utf8mb4 TEXT는 BINARY_FLAG가 있어도 글자"
        );
        assert_eq!(bits_to_int(&[]), 0);
    }

    #[test]
    fn binary_cells_format_dates_and_times() {
        assert_eq!(
            bin_cell(
                mysql::Value::Date(2026, 10, 22, 1, 2, 3, 0),
                ColumnType::MYSQL_TYPE_DATETIME
            ),
            Value::Str("2026-10-22 01:02:03".into())
        );
        assert_eq!(
            bin_cell(
                mysql::Value::Date(2026, 10, 22, 1, 2, 3, 500),
                ColumnType::MYSQL_TYPE_TIMESTAMP
            ),
            Value::Str("2026-10-22 01:02:03.000500".into())
        );
        assert_eq!(
            bin_cell(
                mysql::Value::Date(2026, 10, 22, 0, 0, 0, 0),
                ColumnType::MYSQL_TYPE_DATE
            ),
            Value::Str("2026-10-22".into())
        );
        assert_eq!(
            bin_cell(
                mysql::Value::Time(true, 1, 2, 3, 4, 0),
                ColumnType::MYSQL_TYPE_TIME
            ),
            Value::Str("-26:03:04".into())
        );
        assert_eq!(
            bin_cell(
                mysql::Value::UInt(u64::MAX),
                ColumnType::MYSQL_TYPE_LONGLONG
            ),
            Value::Decimal(u64::MAX.to_string())
        );
        assert_eq!(
            bin_cell(mysql::Value::Float(1.5), ColumnType::MYSQL_TYPE_FLOAT),
            Value::Float(1.5)
        );
    }

    #[test]
    fn type_names() {
        use ColumnType::*;
        assert_eq!(
            type_name(MYSQL_TYPE_LONG, ColumnFlags::UNSIGNED_FLAG),
            "INT UNSIGNED"
        );
        assert_eq!(
            type_name(MYSQL_TYPE_VAR_STRING, ColumnFlags::empty()),
            "VARCHAR"
        );
        assert_eq!(
            type_name(MYSQL_TYPE_VAR_STRING, ColumnFlags::BINARY_FLAG),
            "VARBINARY"
        );
        assert_eq!(type_name(MYSQL_TYPE_BLOB, ColumnFlags::empty()), "TEXT");
        assert_eq!(type_name(MYSQL_TYPE_STRING, ColumnFlags::ENUM_FLAG), "ENUM");
        assert_eq!(type_name(MYSQL_TYPE_JSON, ColumnFlags::empty()), "JSON");
        // 글자형이 아닌 타입의 UNSIGNED는 붙지 않는다.
        assert_eq!(
            type_name(MYSQL_TYPE_DATE, ColumnFlags::UNSIGNED_FLAG),
            "DATE"
        );
    }

    #[test]
    fn call_plan_splits_arguments() {
        let p = CallPlan::parse("CALL `shop`.`add_item`(3, ?, 'a,?)b', f(1, 2), ?) -- ?").unwrap();
        assert_eq!(p.name, "shop.add_item");
        assert_eq!(p.args, vec!["3", "?", "'a,?)b'", "f(1, 2)", "?"]);
        assert_eq!(
            p.call_sql(),
            "CALL `shop`.`add_item`(@nsql_1, @nsql_2, @nsql_3, @nsql_4, @nsql_5)"
        );
        let p = CallPlan::parse("  call p (?)").unwrap();
        assert_eq!(
            (p.name.as_str(), p.args.as_slice()),
            ("p", &["?".to_string()][..])
        );
        assert!(
            CallPlan::parse("CALL p(1, 2)").is_none(),
            "바인드 없음 = 텍스트 그대로"
        );
        assert!(CallPlan::parse("SELECT f(?)").is_none());
        assert!(CallPlan::parse("CALLx(?)").is_none());
        assert!(CallPlan::parse("CALL p(?").is_none(), "닫는 괄호 없음");
        assert!(CallPlan::parse("CALL p(?,)").is_none(), "빈 인자");
    }

    #[test]
    fn bind_values() {
        assert_eq!(to_mysql(&Value::Bool(true)), mysql::Value::Int(1));
        assert_eq!(
            to_mysql(&Value::Decimal("1.50".into())),
            mysql::Value::Bytes(b"1.50".to_vec())
        );
        assert_eq!(to_mysql(&Value::Cursor(CursorId(1))), mysql::Value::NULL);
    }
}
