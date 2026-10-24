//! ODBC 어댑터 본문(Windows 전용 · 문서는 `lib.rs`).
use nsql_core::{
    Caps, CatalogColumn, CatalogTable, Column, CursorId, DbError, Dialect, ExecRequest, ExecResult,
    ResultSet, Session, Value,
};
use nsql_script::ConnectSpec;
use odbc_api::buffers::{AnyColumnBufferSlice, BufferDesc, ColumnarDynBuffer};
use odbc_api::handles::{AsStatementRef, Statement};
use odbc_api::parameter::InputParameter;
use odbc_api::{Connection, ConnectionOptions, Cursor, DataType, IntoParameter, Nullable};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

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
    /// 드라이버가 트랜잭션을 지원하는가(접속 때 자동 커밋 끄기 탐침 성공) — 아니면 commit = no-op · rollback = 안내 오류.
    transactions: bool,
    /// ★ 수동 트랜잭션 열림(10-24): 러너의 `BEGIN TRANSACTION`을 받아 `SQL_ATTR_AUTOCOMMIT=OFF` · COMMIT/ROLLBACK(SQLEndTran) 뒤 다시 ON.
    ///   종전엔 접속 때부터 늘 OFF라 **탐색기 메타 세션의 읽기까지 트랜잭션을 열어 둬**(SQLite 공유 잠금) 실행 세션의 커밋이
    ///   `SQLEndTran` → sqlite3odbc busy_handler에서 영원히 기다렸다(맥 GUI 실기 · CLI는 연결 1이라 안 걸림).
    in_tx: bool,
    /// ★ 실행 중인 문장 핸들(T-383 2차 취소): `execute` 동안만 non-null · [`OdbcCancel`]이 다른 스레드에서 `SQLCancel`.
    active: Arc<AtomicPtr<std::ffi::c_void>>,
    /// 진행 중인 `SQLCancel` 수(해제 경합 가드 · 10-24).
    cancel_inflight: Arc<std::sync::atomic::AtomicUsize>,
}

/// ★ 취소 방식(10-24 · 138차 Linux · 설정 `odbc.cancel` = auto|on|off · 호스트가 넣는다): 0 = auto · 1 = on · 2 = off.
///   auto = **unix의 Microsoft SQL Server ODBC 드라이버는 끔** — msodbcsql18(Linux)에서 다른 스레드의 `SQLCancel`이 실행 중 문장과 경합해
///   드라이버 안에서 SIGSEGV(5회 중 2회 · gdb = libmsodbcsql `SQLCancel` 프레임 · 통합 시험 `odbc_select_bind_dml_and_errors`) → 취소 핸들 없음
///   = 호스트는 배치 경계 정지 + 두 번째 ■ 강제 중지(44 §6 · Oracle Thin과 같은 꼴). Windows 인박스 드라이버는 그대로 `SQLCancel`.
static CANCEL_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn set_cancel_mode(mode: &str) {
    let v = match mode.trim().to_ascii_lowercase().as_str() {
        "on" | "true" | "1" => 1,
        "off" | "false" | "0" => 2,
        _ => 0,
    };
    CANCEL_MODE.store(v, Ordering::Relaxed);
}

/// 이 세션(드라이버 이름 · OS)에서 `SQLCancel`을 쓸 것인가 — 순수 판정(시험).
pub(crate) fn cancel_allowed(mode: u8, dbms: &str, unix: bool) -> bool {
    match mode {
        1 => true,
        2 => false,
        _ => !(unix && dbms.to_ascii_lowercase().contains("sql server")),
    }
}

fn cancel_mode() -> u8 {
    CANCEL_MODE.load(Ordering::Relaxed)
}

/// ★ 실행 취소(ODBC `SQLCancel` · 다른 스레드에서 실행 중 문장 핸들에 · 44 §6): 실행 중이 아니면 조용히 Ok.
///   `inflight` = SQLCancel이 도는 동안 1 이상 — 실행 쪽 [`ActiveGuard`]가 핸들을 비우고 문장을 해제하기 전에 0을 기다린다(해제된 HSTMT에
///   SQLCancel이 닿는 TOCTOU 방지 · 10-24).
struct OdbcCancel {
    active: Arc<AtomicPtr<std::ffi::c_void>>,
    inflight: Arc<std::sync::atomic::AtomicUsize>,
}

impl nsql_core::CancelHandle for OdbcCancel {
    fn cancel(&self) -> Result<(), DbError> {
        self.inflight.fetch_add(1, Ordering::AcqRel);
        let r = self.cancel_inner();
        self.inflight.fetch_sub(1, Ordering::AcqRel);
        r
    }
}

impl OdbcCancel {
    fn cancel_inner(&self) -> Result<(), DbError> {
        let p = self.active.load(Ordering::Acquire);
        if p.is_null() {
            return Ok(());
        }
        // SAFETY: `p`는 `execute_inner`가 실행 중인 문장의 HSTMT · 문장이 끝나면 null로 되돌리되 `inflight`가 0이 될 때까지 해제를 미룬다 ·
        //   SQLCancel은 다른 스레드에서 실행 중 문장에 부르도록 정의된 호출이다(ODBC 3.x).
        let rc = unsafe { odbc_api::sys::SQLCancel(odbc_api::sys::HStmt(p.cast())) };
        if matches!(
            rc,
            odbc_api::sys::SqlReturn::SUCCESS | odbc_api::sys::SqlReturn::SUCCESS_WITH_INFO
        ) {
            Ok(())
        } else {
            Err(DbError {
                code: None,
                message: format!("SQLCancel returned {rc:?}"),
                position: None,
            })
        }
    }
}

/// 실행 동안 핸들을 공개하고 끝나면(패닉·오류 포함) null로 되돌린다 — 그 뒤 진행 중인 `SQLCancel`이 있으면 끝날 때까지(상한 2 s) 기다린 뒤
/// 돌아간다(이 가드가 문장 핸들보다 먼저 떨어지므로 해제 전에 취소가 끝난다).
struct ActiveGuard<'a> {
    active: &'a AtomicPtr<std::ffi::c_void>,
    inflight: &'a std::sync::atomic::AtomicUsize,
}

impl Drop for ActiveGuard<'_> {
    fn drop(&mut self) {
        self.active.store(std::ptr::null_mut(), Ordering::Release);
        let t0 = std::time::Instant::now();
        while self.inflight.load(Ordering::Acquire) != 0
            && t0.elapsed() < std::time::Duration::from_secs(2)
        {
            std::thread::yield_now();
        }
    }
}

/// `VarCharArray` → 문자열(UTF-8이 아니면 손실 변환).
fn vc<const N: usize>(v: &odbc_api::parameter::VarCharArray<N>) -> String {
    match v.as_str() {
        Ok(Some(s)) => s.to_string(),
        Ok(None) => String::new(),
        Err(_) => v
            .as_bytes()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default(),
    }
}

/// ★ 프로세스 `LC_CTYPE`을 UTF-8 로캘로(유닉스 · 1회 · 10-24 Intel 맥 실기): Rust는 `setlocale`을 부르지 않아 C 로캘이고, 그러면
///   Microsoft ODBC 드라이버 같은 벤더 드라이버가 좁은(UTF-8) API로 주는 글을 ASCII 기준으로 잘라 **한글 메시지가 깨진다**
///   (실측 = `사용자` → `ac a9 90` = UTF-16 하위 바이트 · `isql`은 `setlocale(LC_ALL, "")`라 정상). 순서 = 지금 값 → `""`(환경) →
///   `C.UTF-8`(glibc) → `en_US.UTF-8` → `UTF-8`(macOS) 중 처음 UTF-8인 것. `LC_CTYPE`만 바꾸므로 숫자·날짜 형식에는 영향 없다.
///   (10-17 맥 클립보드 pbcopy C 로캘 결함과 같은 부류.)
#[cfg(unix)]
fn ensure_utf8_ctype() {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int};
    use std::sync::Once;
    extern "C" {
        fn setlocale(category: c_int, locale: *const c_char) -> *mut c_char;
    }
    #[cfg(target_os = "macos")]
    const LC_CTYPE: c_int = 2;
    #[cfg(not(target_os = "macos"))]
    const LC_CTYPE: c_int = 0;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: setlocale는 NUL 종료 문자열 또는 null을 돌려준다 · 인자는 CString/null.
        let name_of = |p: *mut c_char| -> String {
            if p.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
            }
        };
        if is_utf8_locale(&name_of(unsafe { setlocale(LC_CTYPE, std::ptr::null()) })) {
            return;
        }
        for cand in ["", "C.UTF-8", "en_US.UTF-8", "UTF-8"] {
            let c = CString::new(cand).expect("locale name");
            if is_utf8_locale(&name_of(unsafe { setlocale(LC_CTYPE, c.as_ptr()) })) {
                return;
            }
        }
    });
}

#[cfg(not(unix))]
fn ensure_utf8_ctype() {}

/// 로캘 이름이 UTF-8 코드셋인가(순수): `ko_KR.UTF-8` · `C.UTF-8` · `en_US.utf8` · `UTF-8` = true · `C` · `POSIX` · `ko_KR.eucKR` · 빈 = false.
// Windows 비시험 빌드 = 호출 0(`ensure_utf8_ctype`가 빈 함수) → dead_code(CI windows check 39996810603) · 시험은 쓴다.
#[cfg(any(unix, test))]
#[must_use]
pub(crate) fn is_utf8_locale(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("utf-8") || n.contains("utf8")
}

/// 설치된(등록된) ODBC 드라이버 이름 — 오류 안내용 · 조회 실패 = 빈 목록(실패 경로에서만 부른다).
fn installed_drivers(env: &odbc_api::Environment) -> Vec<String> {
    env.drivers()
        .map(|v| v.into_iter().map(|d| d.description).collect())
        .unwrap_or_default()
}

/// ★ 드라이버/DSN을 못 찾은 접속 오류에 덧붙이는 안내(순수 · 사용자 10-23 맥 실기): unixODBC `Can't open lib '…' : file not found` ·
///   IM002 `Data source name not found and no default driver specified` → "Installed ODBC drivers: a, b"(없으면 none) + 등록 방법
///   (Windows = ODBC Data Sources(odbcad32) · unix = 벤더 드라이버 설치 뒤 odbcinst.ini 등록 · `odbcinst -q -d`). 다른 오류 = None.
pub(crate) fn connect_hint(message: &str, drivers: &[String]) -> Option<String> {
    let m = message.to_ascii_lowercase();
    let missing = m.contains("can't open lib")
        || m.contains("data source name not found")
        || m.contains("no default driver")
        || m.contains("im002");
    if !missing {
        return None;
    }
    let list = if drivers.is_empty() {
        "none".to_string()
    } else {
        drivers.join(", ")
    };
    let how = if cfg!(windows) {
        "register it in ODBC Data Sources (odbcad32) or use Driver={<installed name>}"
    } else {
        "install the vendor ODBC driver and register it in odbcinst.ini (unixODBC: `odbcinst -q -d` lists the names), then use Driver={<registered name>}"
    };
    Some(format!(
        "\nInstalled ODBC drivers: {list}. The driver or DSN in the connection string is not registered on this computer: {how}."
    ))
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

/// `COMMIT [WORK]` = `Some(true)` · `ROLLBACK [WORK]`/`ABORT` = `Some(false)` · 그 밖(`ROLLBACK TO sp` 포함) = `None`(순수).
/// 수동 트랜잭션 시작문인가(순수 · 10-24): `BEGIN` · `BEGIN TRANSACTION|TRAN|WORK` · `START TRANSACTION`(옵션 낱말 뒤 허용) —
/// PL/SQL 블록 `BEGIN … END`는 여러 낱말이라 걸리지 않는다(한 줄 `BEGIN NULL; END;`도 `NULL`이 뒤따라 제외).
fn tx_begin_sql(sql: &str) -> bool {
    let words: Vec<String> = sql
        .trim()
        .trim_end_matches(';')
        .split_whitespace()
        .map(|w| w.to_ascii_uppercase())
        .collect();
    match words.as_slice() {
        [w] => w == "BEGIN",
        [w, x, ..] if w == "BEGIN" => matches!(x.as_str(), "TRANSACTION" | "TRAN" | "WORK"),
        [w, x, ..] if w == "START" => x == "TRANSACTION",
        _ => false,
    }
}

impl OdbcSession {
    /// 수동 트랜잭션 끝(커밋/롤백 뒤): 자동 커밋 ON 복귀.
    fn end_manual_tx(&mut self) -> Result<(), DbError> {
        if self.in_tx {
            self.in_tx = false;
            self.conn.set_autocommit(true).map_err(err)?;
        }
        Ok(())
    }
}

fn tx_control(sql: &str) -> Option<bool> {
    let words: Vec<String> = sql
        .trim()
        .trim_end_matches(';')
        .split_whitespace()
        .map(|w| w.to_ascii_uppercase())
        .collect();
    match words.as_slice() {
        [w] | [w, _] if w == "COMMIT" => (words.len() == 1 || words[1] == "WORK").then_some(true),
        [w] | [w, _] if w == "ROLLBACK" || w == "ABORT" => {
            (words.len() == 1 || words[1] == "WORK").then_some(false)
        }
        _ => None,
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

/// ★ 긴 열이 있는가(T-383 2차): 문자열 상한에 닿거나 이진 상한에 닿는 열 = 블록 버퍼 대신 **행 단위 스트리밍**(`SQLGetData` ·
/// 잘림 없음 · 느리지만 LOB 안전).
fn has_long_column(kinds: &[Kind], descs: &[BufferDesc]) -> bool {
    kinds.iter().zip(descs).any(|(k, d)| match (k, d) {
        (Kind::Text, BufferDesc::WText { max_str_len }) => *max_str_len >= MAX_TEXT_CHARS,
        (Kind::Bin, BufferDesc::Binary { max_bytes }) => *max_bytes >= MAX_BIN_BYTES,
        _ => false,
    })
}

/// 행 단위 경로(LOB 안전): 셀마다 `SQLGetData` — 숫자는 `Nullable` · 문자열은 wide 텍스트 · 이진은 바이트.
fn fetch_rows_streaming<C: Cursor>(
    cur: &mut C,
    kinds: &[Kind],
    max_rows: usize,
) -> Result<Vec<Vec<Value>>, DbError> {
    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut wbuf: Vec<u16> = Vec::new();
    let mut bbuf: Vec<u8> = Vec::new();
    while let Some(mut row) = cur.next_row().map_err(err)? {
        let mut out = Vec::with_capacity(kinds.len());
        for (i, kind) in kinds.iter().enumerate() {
            let col = (i + 1) as u16;
            let v = match kind {
                Kind::I64 => {
                    let mut t = Nullable::<i64>::null();
                    row.get_data(col, &mut t).map_err(err)?;
                    t.into_opt().map_or(Value::Null, Value::Int)
                }
                Kind::F64 => {
                    let mut t = Nullable::<f64>::null();
                    row.get_data(col, &mut t).map_err(err)?;
                    t.into_opt().map_or(Value::Null, Value::Float)
                }
                Kind::Bit => {
                    let mut t = Nullable::<odbc_api::Bit>::null();
                    row.get_data(col, &mut t).map_err(err)?;
                    t.into_opt()
                        .map_or(Value::Null, |b| Value::Bool(b.as_bool()))
                }
                Kind::Bin => {
                    bbuf.clear();
                    if row.get_binary(col, &mut bbuf).map_err(err)? {
                        Value::Bytes(bbuf.clone())
                    } else {
                        Value::Null
                    }
                }
                Kind::Text | Kind::Decimal => {
                    wbuf.clear();
                    if row.get_wide_text(col, &mut wbuf).map_err(err)? {
                        let t = String::from_utf16_lossy(&wbuf);
                        if *kind == Kind::Decimal {
                            Value::Decimal(t.trim().to_string())
                        } else {
                            Value::Str(t)
                        }
                    } else {
                        Value::Null
                    }
                }
            };
            out.push(v);
        }
        rows.push(out);
        if max_rows > 0 && rows.len() >= max_rows {
            break;
        }
    }
    Ok(rows)
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
    if has_long_column(&kinds, &descs) {
        let rows = fetch_rows_streaming(&mut cur, &kinds, max_rows)?;
        return Ok((cur, ResultSet { columns, rows }));
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
        ensure_utf8_ctype();
        let env = odbc_api::environment().map_err(err)?;
        let conn = env
            .connect_with_connection_string(
                &cs,
                ConnectionOptions {
                    login_timeout_sec: Some(LOGIN_TIMEOUT_SECS),
                    ..ConnectionOptions::default()
                },
            )
            .map_err(|e| {
                let mut e = err(e);
                // ★ 드라이버/DSN 없음 = 설치된 드라이버 목록 + 등록 안내(사용자 10-23 맥 "Can't open lib 'SQL Server'").
                if let Some(h) = connect_hint(&e.message, &installed_drivers(env)) {
                    e.message.push_str(&h);
                }
                e
            })?;
        // ★ 자동 커밋 끄기 = best-effort(협업 V1 10-22): Excel·Text·Access(Jet/ACE)처럼 트랜잭션이 없는 드라이버는
        //   `SQL_ATTR_AUTOCOMMIT=OFF`에 HYC00 "Optional feature not implemented" → 접속을 살리고 "트랜잭션 없음"으로 표시.
        let transactions = match conn.set_autocommit(false) {
            Ok(()) => true,
            Err(odbc_api::Error::Diagnostics { record, .. })
                if record.state.as_str() == "HYC00" =>
            {
                false
            }
            Err(e) => return Err(err(e)),
        };
        // ★ 탐침만 하고 **자동 커밋 ON으로 되돌린다**(10-24): 수동 트랜잭션은 러너의 `BEGIN TRANSACTION`(Caps `tx_begin`)이
        //   오는 순간부터만 OFF — 읽기 전용 세션(탐색기 메타)이 잠금을 쥐지 않게.
        if transactions {
            conn.set_autocommit(true).map_err(err)?;
        }
        let dbms = conn.database_management_system_name().unwrap_or_default();
        Ok(OdbcSession {
            conn,
            description: spec.redacted(),
            transactions,
            in_tx: false,
            dbms,
            max_rows: 0,
            active: Arc::new(AtomicPtr::new(std::ptr::null_mut())),
            cancel_inflight: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    fn execute_inner(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        // ★ 스크립트의 `COMMIT`/`ROLLBACK` 문 = 세션 API로(협업 V1 10-22: autocommit-only 드라이버(Excel/Access)에 SQL로 보내면
        //   42000 · 트랜잭션 있는 드라이버도 ODBC 표준은 SQLEndTran). `ROLLBACK TO <sp>`는 그대로 서버로.
        if let Some(commit) = tx_control(&req.sql) {
            if commit {
                self.commit()?;
            } else {
                self.rollback()?;
            }
            return Ok(ExecResult::default());
        }
        // ★ 수동 트랜잭션 시작(러너 `tx_begin` = `BEGIN TRANSACTION` · 사용자 `BEGIN`): 드라이버 자동 커밋 OFF(10-24).
        //   트랜잭션 없는 드라이버(Excel/Access) = 조용히 무시(자동 커밋만).
        if tx_begin_sql(&req.sql) {
            if self.transactions && !self.in_tx {
                self.conn.set_autocommit(false).map_err(err)?;
                self.in_tx = true;
            }
            return Ok(ExecResult::default());
        }
        let params: Vec<Box<dyn InputParameter>> =
            req.params.iter().map(|p| to_param(&p.value)).collect();
        let mut stmt = self.conn.preallocate().map_err(err)?;
        // ★ 취소 창: 실행 동안 HSTMT를 공개(가드가 끝날 때 null) → 다른 스레드의 `SQLCancel`이 이 문장을 끊는다.
        let handle = stmt.as_stmt_ref().as_sys();
        self.active.store(handle.0.cast(), Ordering::Release);
        let _guard = ActiveGuard {
            active: &self.active,
            inflight: &self.cancel_inflight,
        };
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
        let mut s = if self.dbms.is_empty() {
            "odbc-api 29".to_string()
        } else {
            format!("odbc-api 29 · {}", self.dbms)
        };
        if !self.transactions {
            s.push_str(" · autocommit only");
        }
        s
    }

    fn cancel_handle(&self) -> Option<Arc<dyn nsql_core::CancelHandle>> {
        // ★ 10-24: unix MS 드라이버는 auto에서 끔(SQLCancel 경합 SIGSEGV) → None = 배치 경계 정지 + 두 번째 ■ 강제.
        if !cancel_allowed(cancel_mode(), &self.dbms, cfg!(unix)) {
            return None;
        }
        Some(Arc::new(OdbcCancel {
            active: self.active.clone(),
            inflight: self.cancel_inflight.clone(),
        }))
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
        if !self.transactions {
            return Ok(());
        }
        self.conn.commit().map_err(err)?;
        self.end_manual_tx()
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        if !self.transactions {
            return Err(DbError {
                code: None,
                message:
                    "ODBC: this driver has no transactions (autocommit only) - nothing to roll back"
                        .into(),
                position: None,
            });
        }
        self.conn.rollback().map_err(err)?;
        self.end_manual_tx()
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

    /// ★ `SQLTables`(T-383 2차): 스키마 없으면 전부 · 종류 = 드라이버 표기(`TABLE` · `VIEW` · `SYSTEM TABLE`).
    fn catalog_tables(
        &mut self,
        schema: Option<&str>,
    ) -> Result<Option<Vec<CatalogTable>>, DbError> {
        let it = self
            .conn
            .tables("", schema.unwrap_or(""), "", "")
            .map_err(err)?;
        let mut v = Vec::new();
        for row in it {
            let r = row.map_err(err)?;
            v.push(CatalogTable {
                catalog: vc(&r.catalog),
                schema: vc(&r.schema),
                name: vc(&r.table),
                kind: vc(&r.table_type),
            });
        }
        Ok(Some(v))
    }

    /// ★ `SQLColumns`: 위치 순 · nullable = 0 아니오 · 1 예 · 2 모름.
    fn catalog_columns(
        &mut self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<Option<Vec<CatalogColumn>>, DbError> {
        let it = self
            .conn
            .columns("", schema.unwrap_or(""), table, "")
            .map_err(err)?;
        let mut v = Vec::new();
        for row in it {
            let r = row.map_err(err)?;
            v.push(CatalogColumn {
                name: vc(&r.column_name),
                type_name: vc(&r.type_name),
                size: r.column_size.into_opt().map(i64::from),
                decimal_digits: r.decimal_digits.into_opt().map(i64::from),
                nullable: match r.nullable {
                    0 => Some(false),
                    1 => Some(true),
                    _ => None,
                },
                position: i64::from(r.ordinal_position),
                default: vc(&r.column_default),
            });
        }
        v.sort_by_key(|c| c.position);
        Ok(Some(v))
    }

    fn is_alive(&self) -> bool {
        self.conn.is_dead().map(|dead| !dead).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {

    /// MC/DC(10-24): 모드(auto/on/off) × unix × 드라이버 이름 — 각 조건이 독립으로 결과를 뒤집는다.
    #[test]
    fn cancel_allowed_matrix() {
        use super::cancel_allowed;
        assert!(
            !cancel_allowed(0, "Microsoft SQL Server", true),
            "auto · unix · MS = 끔"
        );
        assert!(
            cancel_allowed(0, "Microsoft SQL Server", false),
            "auto · Windows · MS = 켬"
        );
        assert!(
            cancel_allowed(0, "PostgreSQL", true),
            "auto · unix · 다른 드라이버 = 켬"
        );
        assert!(
            cancel_allowed(1, "Microsoft SQL Server", true),
            "on = 늘 켬"
        );
        assert!(!cancel_allowed(2, "PostgreSQL", false), "off = 늘 끔");
        assert!(
            !cancel_allowed(0, "microsoft sql server", true),
            "대소문자 무관"
        );
    }
    use super::*;

    /// UTF-8 로캘 판정(10-24 Intel 맥 한글 깨짐): UTF-8/utf8 코드셋만 true.
    #[test]
    fn utf8_locale_names() {
        for ok in ["ko_KR.UTF-8", "C.UTF-8", "en_US.utf8", "UTF-8"] {
            assert!(is_utf8_locale(ok), "{ok}");
        }
        for no in ["C", "POSIX", "ko_KR.eucKR", ""] {
            assert!(!is_utf8_locale(no), "{no}");
        }
    }

    /// 드라이버/DSN 없음 안내(순수 · 10-23 맥): unixODBC `Can't open lib` · IM002 = 설치 목록(없으면 none) + 등록 방법 · 그 밖 = None.
    #[test]
    fn connect_hint_for_missing_driver_or_dsn() {
        let m = "State: 01000, Native error: 0, Message: [unixODBC][Driver Manager]Can't open lib 'SQL Server' : file not found";
        let h = connect_hint(m, &[]).expect("hint");
        assert!(h.contains("Installed ODBC drivers: none"), "{h}");
        assert!(h.contains("Driver={"), "{h}");
        let h2 = connect_hint(
            "State: IM002, Native error: 0, Message: [Microsoft][ODBC Driver Manager] Data source name not found and no default driver specified",
            &["ODBC Driver 18 for SQL Server".into(), "PostgreSQL Unicode".into()],
        )
        .expect("hint");
        assert!(
            h2.contains("ODBC Driver 18 for SQL Server, PostgreSQL Unicode"),
            "{h2}"
        );
        assert_eq!(
            connect_hint(
                "State: 28000, Native error: 18456, Message: Login failed for user",
                &[]
            ),
            None
        );
    }

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

    /// ★ COMMIT/ROLLBACK 문 판정(autocommit-only 드라이버 보호): WORK 허용 · 세이브포인트·그 밖은 서버로.
    #[test]
    /// 수동 트랜잭션 시작문 판정(10-24): BEGIN · BEGIN TRANSACTION/TRAN/WORK · START TRANSACTION = true · PL/SQL 블록·그 밖 = false.
    fn tx_begin_sql_recognizes_transaction_starts_only() {
        for ok in [
            "BEGIN",
            "begin;",
            "BEGIN TRANSACTION",
            "BEGIN TRAN t1",
            "begin work",
            "START TRANSACTION READ WRITE",
        ] {
            assert!(tx_begin_sql(ok), "{ok}");
        }
        for no in [
            "BEGIN NULL; END;",
            "BEGIN\n  x := 1;\nEND;",
            "SELECT 1",
            "COMMIT",
            "START",
        ] {
            assert!(!tx_begin_sql(no), "{no}");
        }
    }

    #[test]
    fn tx_control_recognizes_bare_commit_and_rollback() {
        assert_eq!(tx_control("COMMIT"), Some(true));
        assert_eq!(tx_control(" commit work ; "), Some(true));
        assert_eq!(tx_control("ROLLBACK;"), Some(false));
        assert_eq!(tx_control("abort"), Some(false));
        assert_eq!(tx_control("ROLLBACK TO sp1"), None);
        assert_eq!(tx_control("COMMIT TRANSACTION t1"), None);
        assert_eq!(tx_control("SELECT 1"), None);
    }

    /// ★ 긴 열 판정(T-383 2차): 상한에 닿은 문자열/이진 열이 하나라도 있으면 행 단위 경로.
    #[test]
    fn long_columns_switch_to_streaming() {
        let (d1, k1) = buffer_for(&DataType::WLongVarchar { length: None }, None);
        let (d2, k2) = buffer_for(&DataType::Integer, None);
        assert!(has_long_column(&[k2, k1], &[d2, d1]));
        let (d3, k3) = buffer_for(&DataType::Varchar { length: None }, Some(100));
        assert!(!has_long_column(&[k2, k3], &[d2, d3]));
        let (d4, k4) = buffer_for(&DataType::LongVarbinary { length: None }, None);
        assert!(has_long_column(&[k4], &[d4]));
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
