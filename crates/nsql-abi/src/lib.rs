//! ★ 확장 드라이버 ABI(DR-29 · T-27 ① · docs/22 §0 · 64 §3-4) — 본체와 **in-process cdylib** 드라이버가 주고받는 유일한 경계.
//!
//! - [`VTable`] = 버전 있는 C 함수표(`#[repr(C)]` · 함수 포인터만 · Rust ABI 0). 드라이버는 `extern "C" fn nsql_driver_vtable()`
//!   ([`entry_symbol`] = `nsql_driver_vtable_<id>`) 하나를 내보내고, 본체(`nsql-driver-dyn`)는 `LoadLibrary`/`dlopen` 뒤 그 표로만 부른다.
//! - 값·요청·결과는 [`codec`]의 **바이너리 코덱**(길이 접두 · LE · 태그) — 같은 코덱을 RPC 프로세스 전송에도 재사용한다(22 §0).
//! - 메모리 규칙: 드라이버가 만든 [`Buf`]는 드라이버의 `free_buf`로만 해제(할당자 경계) · 본체가 넘기는 입력 버퍼는 호출 동안만 유효.
//! - 오류 규칙: 반환 `i32` = [`STATUS_OK`] / [`STATUS_DB_ERROR`](`err` = 코덱 `DbError`) / [`STATUS_PANIC`](`err` = UTF-8 메시지 ·
//!   드라이버 쪽 `catch_unwind` — 패닉이 경계를 넘지 않는다).
//! - 스레드 규칙: 세션 호출은 **한 번에 하나**(본체가 직렬화) · `cancel`만 실행 중 다른 스레드에서 올 수 있다(드라이버의
//!   [`nsql_core::CancelHandle`]로 전달).
//! - v1(13함수) = `open` · `execute` · `commit` · `rollback` · `set_option` · `cancel` · `is_alive` · `limits` · `describe` · `close` · `free_buf` + id·방언.
//! - ★ **v2(D-350 · 10-23 · 모든 드라이버 확장화)** = v1 + **범용 [`VTable::call`]**(op 번호 + 코덱 입력 → 코덱 출력 · [`ops`]: `Caps` ·
//!   드라이버 이름 · 서버 커서(fetch_cursor/page/next/close) · 호출 타임아웃 · 벌크 싱크(begin/push/flush/commit/rollback/finish/drop) ·
//!   카탈로그 훅) + 콜백 둘([`VTable::set_message_sink`] · [`VTable::copy_out`]). 새 op는 함수표 배치를 바꾸지 않는다 — 모르는
//!   op = `STATUS_DB_ERROR` "unsupported op"(본체는 그 기능을 없는 것으로 다룬다).
//! - 벌크 규칙: 세션당 열린 싱크는 하나 · 싱크가 열린 동안 다른 세션 호출은 보내지 않는다(본체 `DynBulkSink`가 `&mut` 빌림으로 강제).

use std::ffi::c_void;
use std::os::raw::c_char;

/// ABI 버전 — 함수표 배치나 코덱이 호환되지 않게 바뀌면 올린다(본체는 다른 버전을 거부).
pub const ABI_VERSION: u32 = 2;
/// 드라이버가 내보내는 유일한 심볼: `extern "C" fn() -> *const VTable`.
/// 실제 이름 = `nsql_driver_vtable_<id>`([`entry_symbol`]). 드라이버마다 다르게 해야 여러 드라이버 rlib를 한 바이너리에
/// 정적으로 넣는 내장 빌드에서 심볼이 겹치지 않는다(LNK2005 · D-350 B단계).
pub const ENTRY_SYMBOL_PREFIX: &str = "nsql_driver_vtable_";

/// 드라이버 id의 진입 심볼 이름(`-`도 그대로 — 동적 링커 심볼은 C 식별자 규칙을 따르지 않는다).
#[must_use]
pub fn entry_symbol(id: &str) -> String {
    format!("{ENTRY_SYMBOL_PREFIX}{id}")
}

/// NUL 종료 바이트(dlsym/GetProcAddress용).
#[must_use]
pub fn entry_symbol_c(id: &str) -> Vec<u8> {
    let mut v = entry_symbol(id).into_bytes();
    v.push(0);
    v
}

pub const STATUS_OK: i32 = 0;
pub const STATUS_DB_ERROR: i32 = 1;
pub const STATUS_PANIC: i32 = 2;

/// 드라이버가 할당한 바이트 버퍼(소유권 = 드라이버 · 해제 = `VTable::free_buf`).
#[repr(C)]
#[derive(Debug)]
pub struct Buf {
    pub ptr: *mut u8,
    pub len: usize,
    pub cap: usize,
}

impl Buf {
    pub const EMPTY: Buf = Buf {
        ptr: std::ptr::null_mut(),
        len: 0,
        cap: 0,
    };

    #[must_use]
    pub fn from_vec(v: Vec<u8>) -> Buf {
        let mut v = std::mem::ManuallyDrop::new(v);
        Buf {
            ptr: v.as_mut_ptr(),
            len: v.len(),
            cap: v.capacity(),
        }
    }

    /// 드라이버 쪽에서만(같은 할당자) — `from_vec`로 만든 버퍼를 되살린다.
    ///
    /// # Safety
    /// `from_vec`가 돌려준 값을 한 번만 넘긴다.
    #[must_use]
    pub unsafe fn into_vec(self) -> Vec<u8> {
        if self.ptr.is_null() {
            return Vec::new();
        }
        unsafe { Vec::from_raw_parts(self.ptr, self.len, self.cap) }
    }

    /// 본체 쪽에서 읽기(복사).
    ///
    /// # Safety
    /// `ptr..ptr+len`이 드라이버가 준 그대로 살아 있어야 한다.
    #[must_use]
    pub unsafe fn as_slice(&self) -> &[u8] {
        if self.ptr.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
        }
    }
}

/// 드라이버 함수표(ABI v1). 세션 핸들 = 드라이버가 만든 불투명 포인터.
#[repr(C)]
#[derive(Debug)]
pub struct VTable {
    pub abi_version: u32,
    /// 드라이버 id(NUL 종료 정적 문자열 · `sqlite` · `oracle-oci` …).
    pub driver_id: unsafe extern "C" fn() -> *const c_char,
    /// 방언 이름([`nsql_core::Dialect::from_name`]이 읽는 꼴 · `sqlite` · `oracle` …).
    pub dialect: unsafe extern "C" fn() -> *const c_char,
    /// 코덱 `ConnectSpec` → 세션 핸들.
    pub open: unsafe extern "C" fn(
        spec: *const u8,
        len: usize,
        out: *mut *mut c_void,
        err: *mut Buf,
    ) -> i32,
    /// 코덱 `ExecRequest` → 코덱 `ExecResult`.
    pub execute: unsafe extern "C" fn(
        s: *mut c_void,
        req: *const u8,
        len: usize,
        out: *mut Buf,
        err: *mut Buf,
    ) -> i32,
    pub commit: unsafe extern "C" fn(s: *mut c_void, err: *mut Buf) -> i32,
    pub rollback: unsafe extern "C" fn(s: *mut c_void, err: *mut Buf) -> i32,
    pub set_option: unsafe extern "C" fn(
        s: *mut c_void,
        name: *const u8,
        name_len: usize,
        value: *const u8,
        value_len: usize,
        err: *mut Buf,
    ) -> i32,
    /// 실행 중 다른 스레드에서 불러도 된다(드라이버의 취소 핸들).
    pub cancel: unsafe extern "C" fn(s: *mut c_void, err: *mut Buf) -> i32,
    pub is_alive: unsafe extern "C" fn(s: *mut c_void) -> i32,
    /// [`nsql_core::DriverLimit`] 비트마스크([`limits_to_bits`]).
    pub limits: unsafe extern "C" fn(s: *mut c_void) -> u32,
    pub describe: unsafe extern "C" fn(s: *mut c_void, out: *mut Buf) -> i32,
    pub close: unsafe extern "C" fn(s: *mut c_void),
    pub free_buf: unsafe extern "C" fn(b: Buf),
    /// ★ v2 범용 호출: `op`([`ops`]) + 코덱 입력 → 코덱 출력(`out` · op마다 다름). 모르는 op = `STATUS_DB_ERROR`.
    pub call: unsafe extern "C" fn(
        s: *mut c_void,
        op: u32,
        input: *const u8,
        len: usize,
        out: *mut Buf,
        err: *mut Buf,
    ) -> i32,
    /// ★ v2 서버 메시지 싱크(DBMS_OUTPUT · PRINT · NOTICE): 드라이버가 실행 중 `cb(ctx, ptr, len)`을 부른다(같은 스레드) · `None` = 해제.
    pub set_message_sink:
        unsafe extern "C" fn(s: *mut c_void, cb: Option<MsgCb>, ctx: *mut c_void) -> i32,
    /// ★ v2 COPY OUT 스트리밍: 드라이버가 조각마다 `cb(ctx, ptr, len)`(0 = 계속 · 그 밖 = 중단)을 부른다 · `rows` = 내보낸 행 수.
    pub copy_out: unsafe extern "C" fn(
        s: *mut c_void,
        sql: *const u8,
        sql_len: usize,
        csv: u8,
        cb: WriteCb,
        ctx: *mut c_void,
        rows: *mut u64,
        err: *mut Buf,
    ) -> i32,
}

/// 서버 메시지 콜백(본체 쪽 · 같은 스레드에서 실행 중에 불린다).
pub type MsgCb = unsafe extern "C" fn(ctx: *mut c_void, ptr: *const u8, len: usize);
/// COPY OUT 쓰기 콜백(0 = 성공 · 그 밖 = 쓰기 실패 → 드라이버가 중단).
pub type WriteCb = unsafe extern "C" fn(ctx: *mut c_void, ptr: *const u8, len: usize) -> i32;

/// ★ [`VTable::call`]의 op 번호(ABI 고정 · 추가만 한다 · 입력/출력 = `codec`).
pub mod ops {
    /// out = `caps`.
    pub const CAPS: u32 = 1;
    /// out = UTF-8 이름(원시 바이트).
    pub const DRIVER_NAME: u32 = 2;
    /// in = u64 cursor id · out = `result_set`.
    pub const FETCH_CURSOR: u32 = 3;
    /// in = u64 cursor id · u32 max · out = `result_set` · u8 has_handle · u32 handle.
    pub const FETCH_CURSOR_PAGE: u32 = 4;
    /// out = u8.
    pub const CURSOR_SUPPORTED: u32 = 5;
    /// in = u32 handle · u32 max · out = `result_set` · u8 more.
    pub const FETCH_NEXT: u32 = 6;
    /// in = u32 handle.
    pub const CLOSE_CURSOR: u32 = 7;
    /// in = u8 has · u64 ms.
    pub const SET_CALL_TIMEOUT: u32 = 8;
    /// in = str table · u32 n + str… cols · u32 n + str… types · `bulk_opts` · out 없음(세션당 싱크 하나).
    pub const BULK_BEGIN: u32 = 9;
    /// in = u32 n + value….
    pub const BULK_PUSH: u32 = 10;
    /// out = u64.
    pub const BULK_FLUSH: u32 = 11;
    /// out = u8.
    pub const BULK_COMMIT: u32 = 12;
    pub const BULK_ROLLBACK: u32 = 13;
    /// out = u64 · 싱크 소비.
    pub const BULK_FINISH: u32 = 14;
    /// 싱크 버림(finish 없이).
    pub const BULK_DROP: u32 = 15;
    /// in = opt_str schema · out = u8 has · u32 n + `catalog_table`….
    pub const CATALOG_TABLES: u32 = 16;
    /// in = opt_str schema · str table · out = u8 has · u32 n + `catalog_column`….
    pub const CATALOG_COLUMNS: u32 = 17;
}

/// `DriverLimit` ↔ 비트(ABI 고정 순서).
const LIMIT_ORDER: &[nsql_core::DriverLimit] = &[
    nsql_core::DriverLimit::Cancel,
    nsql_core::DriverLimit::ImplicitResults,
    nsql_core::DriverLimit::ExternalAuth,
    nsql_core::DriverLimit::NativeEncryption,
    nsql_core::DriverLimit::BulkLoad,
    nsql_core::DriverLimit::OldServer,
];

#[must_use]
pub fn limits_to_bits(limits: &[nsql_core::DriverLimit]) -> u32 {
    LIMIT_ORDER
        .iter()
        .enumerate()
        .filter(|(_, l)| limits.contains(l))
        .fold(0u32, |acc, (i, _)| acc | (1 << i))
}

#[must_use]
pub fn bits_to_limits(bits: u32) -> Vec<nsql_core::DriverLimit> {
    LIMIT_ORDER
        .iter()
        .enumerate()
        .filter(|(i, _)| bits & (1 << i) != 0)
        .map(|(_, l)| *l)
        .collect()
}

pub mod codec {
    //! 바이너리 코덱 — 태그 1바이트 · 정수 LE · 문자열/바이트 = u32 길이 접두. 버전은 [`super::ABI_VERSION`]이 함께 올린다.
    use nsql_core::{
        BindParam, Column, CursorId, DbError, Direction, ExecRequest, ExecResult, ResultSet, Value,
        VarType,
    };
    use nsql_script::{ConnEnv, ConnectSpec};

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DecodeError(pub String);

    #[derive(Default, Debug)]
    pub struct Writer {
        pub buf: Vec<u8>,
    }

    impl Writer {
        pub fn u8(&mut self, v: u8) {
            self.buf.push(v);
        }
        pub fn u32(&mut self, v: u32) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn u64(&mut self, v: u64) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn i64(&mut self, v: i64) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn f64(&mut self, v: f64) {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
        pub fn bytes(&mut self, b: &[u8]) {
            self.u32(b.len() as u32);
            self.buf.extend_from_slice(b);
        }
        pub fn str(&mut self, s: &str) {
            self.bytes(s.as_bytes());
        }
        pub fn opt_str(&mut self, s: Option<&str>) {
            match s {
                None => self.u8(0),
                Some(s) => {
                    self.u8(1);
                    self.str(s);
                }
            }
        }
        pub fn value(&mut self, v: &Value) {
            match v {
                Value::Null => self.u8(0),
                Value::Int(i) => {
                    self.u8(1);
                    self.i64(*i);
                }
                Value::Float(f) => {
                    self.u8(2);
                    self.f64(*f);
                }
                Value::Decimal(s) => {
                    self.u8(3);
                    self.str(s);
                }
                Value::Str(s) => {
                    self.u8(4);
                    self.str(s);
                }
                Value::Bool(b) => {
                    self.u8(5);
                    self.u8(u8::from(*b));
                }
                Value::Bytes(b) => {
                    self.u8(6);
                    self.bytes(b);
                }
                Value::Cursor(c) => {
                    self.u8(7);
                    self.u64(c.0);
                }
            }
        }
        pub fn var_type(&mut self, t: &VarType) {
            match t {
                VarType::Number => self.u8(0),
                VarType::Varchar2(n) => {
                    self.u8(1);
                    self.u32(*n);
                }
                VarType::Char(n) => {
                    self.u8(2);
                    self.u32(*n);
                }
                VarType::Clob => self.u8(3),
                VarType::RefCursor => self.u8(4),
                VarType::BinaryFloat => self.u8(5),
                VarType::BinaryDouble => self.u8(6),
                VarType::Date => self.u8(7),
                VarType::Timestamp => self.u8(8),
                VarType::Boolean => self.u8(9),
                VarType::Auto => self.u8(10),
                VarType::Blob => self.u8(11),
            }
        }
        pub fn request(&mut self, r: &ExecRequest) {
            self.str(&r.sql);
            self.u32(r.params.len() as u32);
            for p in &r.params {
                self.str(&p.name);
                self.value(&p.value);
                self.var_type(&p.ty);
                self.u8(match p.direction {
                    Direction::In => 0,
                    Direction::InOut => 1,
                    Direction::Out => 2,
                });
            }
        }
        pub fn result_set(&mut self, rs: &ResultSet) {
            self.u32(rs.columns.len() as u32);
            for c in &rs.columns {
                self.str(&c.name);
                self.str(&c.type_name);
            }
            self.u32(rs.rows.len() as u32);
            for row in &rs.rows {
                for v in row {
                    self.value(v);
                }
            }
        }
        pub fn result(&mut self, r: &ExecResult) {
            match r.rows_affected {
                None => self.u8(0),
                Some(n) => {
                    self.u8(1);
                    self.u64(n);
                }
            }
            self.u32(r.out_params.len() as u32);
            for (n, v) in &r.out_params {
                self.str(n);
                self.value(v);
            }
            self.u32(r.result_sets.len() as u32);
            for rs in &r.result_sets {
                self.result_set(rs);
            }
            self.u32(r.result_labels.len() as u32);
            for l in &r.result_labels {
                self.opt_str(l.as_deref());
            }
            self.u32(r.messages.len() as u32);
            for m in &r.messages {
                self.str(m);
            }
        }
        pub fn db_error(&mut self, e: &DbError) {
            match e.code {
                None => self.u8(0),
                Some(c) => {
                    self.u8(1);
                    self.i64(c);
                }
            }
            self.str(&e.message);
            match e.position {
                None => self.u8(0),
                Some(p) => {
                    self.u8(1);
                    self.u64(p as u64);
                }
            }
        }
        /// 스펙 = (키, 값) 목록 — 모르는 키는 무시(앞으로 필드가 늘어도 호환).
        pub fn spec(&mut self, s: &ConnectSpec) {
            let mut kv: Vec<(&str, String)> = Vec::new();
            if let Some(v) = &s.user {
                kv.push(("user", v.clone()));
            }
            if let Some(v) = &s.password {
                kv.push(("password", v.clone()));
            }
            if let Some(v) = &s.host {
                kv.push(("host", v.clone()));
            }
            if let Some(v) = s.port {
                kv.push(("port", v.to_string()));
            }
            if let Some(v) = &s.database {
                kv.push(("database", v.clone()));
            }
            if let Some(v) = &s.role {
                kv.push(("role", v.clone()));
            }
            if let Some(v) = s.dialect {
                kv.push(("dialect", v.to_string()));
            }
            if let Some(v) = &s.schema {
                kv.push(("schema", v.clone()));
            }
            if let Some(v) = s.env {
                kv.push(("env", v.as_str().to_string()));
            }
            if let Some(v) = &s.tls {
                kv.push(("tls", v.clone()));
            }
            self.u32(kv.len() as u32);
            for (k, v) in kv {
                self.str(k);
                self.str(&v);
            }
        }
    }

    #[derive(Debug)]
    pub struct Reader<'a> {
        buf: &'a [u8],
        pos: usize,
    }

    impl<'a> Reader<'a> {
        #[must_use]
        pub fn new(buf: &'a [u8]) -> Self {
            Reader { buf, pos: 0 }
        }
        fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
            let end = self
                .pos
                .checked_add(n)
                .filter(|e| *e <= self.buf.len())
                .ok_or_else(|| DecodeError(format!("truncated at {} (+{n})", self.pos)))?;
            let s = &self.buf[self.pos..end];
            self.pos = end;
            Ok(s)
        }
        pub fn u8(&mut self) -> Result<u8, DecodeError> {
            Ok(self.take(1)?[0])
        }
        pub fn u32(&mut self) -> Result<u32, DecodeError> {
            Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4")))
        }
        pub fn u64(&mut self) -> Result<u64, DecodeError> {
            Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8")))
        }
        pub fn i64(&mut self) -> Result<i64, DecodeError> {
            Ok(i64::from_le_bytes(self.take(8)?.try_into().expect("8")))
        }
        pub fn f64(&mut self) -> Result<f64, DecodeError> {
            Ok(f64::from_le_bytes(self.take(8)?.try_into().expect("8")))
        }
        pub fn bytes(&mut self) -> Result<Vec<u8>, DecodeError> {
            let n = self.u32()? as usize;
            Ok(self.take(n)?.to_vec())
        }
        pub fn str(&mut self) -> Result<String, DecodeError> {
            let b = self.bytes()?;
            String::from_utf8(b).map_err(|e| DecodeError(format!("utf8: {e}")))
        }
        pub fn opt_str(&mut self) -> Result<Option<String>, DecodeError> {
            Ok(match self.u8()? {
                0 => None,
                _ => Some(self.str()?),
            })
        }
        pub fn value(&mut self) -> Result<Value, DecodeError> {
            Ok(match self.u8()? {
                0 => Value::Null,
                1 => Value::Int(self.i64()?),
                2 => Value::Float(self.f64()?),
                3 => Value::Decimal(self.str()?),
                4 => Value::Str(self.str()?),
                5 => Value::Bool(self.u8()? != 0),
                6 => Value::Bytes(self.bytes()?),
                7 => Value::Cursor(CursorId(self.u64()?)),
                t => return Err(DecodeError(format!("value tag {t}"))),
            })
        }
        pub fn var_type(&mut self) -> Result<VarType, DecodeError> {
            Ok(match self.u8()? {
                0 => VarType::Number,
                1 => VarType::Varchar2(self.u32()?),
                2 => VarType::Char(self.u32()?),
                3 => VarType::Clob,
                4 => VarType::RefCursor,
                5 => VarType::BinaryFloat,
                6 => VarType::BinaryDouble,
                7 => VarType::Date,
                8 => VarType::Timestamp,
                9 => VarType::Boolean,
                10 => VarType::Auto,
                11 => VarType::Blob,
                t => return Err(DecodeError(format!("vartype tag {t}"))),
            })
        }
        pub fn request(&mut self) -> Result<ExecRequest, DecodeError> {
            let sql = self.str()?;
            let n = self.u32()? as usize;
            let mut params = Vec::with_capacity(n.min(1024));
            for _ in 0..n {
                let name = self.str()?;
                let value = self.value()?;
                let ty = self.var_type()?;
                let direction = match self.u8()? {
                    0 => Direction::In,
                    1 => Direction::InOut,
                    2 => Direction::Out,
                    t => return Err(DecodeError(format!("direction tag {t}"))),
                };
                params.push(BindParam {
                    name,
                    value,
                    ty,
                    direction,
                });
            }
            Ok(ExecRequest { sql, params })
        }
        pub fn result_set(&mut self) -> Result<ResultSet, DecodeError> {
            let nc = self.u32()? as usize;
            let mut columns = Vec::with_capacity(nc.min(4096));
            for _ in 0..nc {
                columns.push(Column {
                    name: self.str()?,
                    type_name: self.str()?,
                });
            }
            let nr = self.u32()? as usize;
            let mut rows = Vec::with_capacity(nr.min(1 << 16));
            for _ in 0..nr {
                let mut row = Vec::with_capacity(nc);
                for _ in 0..nc {
                    row.push(self.value()?);
                }
                rows.push(row);
            }
            Ok(ResultSet { columns, rows })
        }
        pub fn result(&mut self) -> Result<ExecResult, DecodeError> {
            let mut r = ExecResult::default();
            if self.u8()? != 0 {
                r.rows_affected = Some(self.u64()?);
            }
            let n = self.u32()? as usize;
            for _ in 0..n {
                let name = self.str()?;
                let v = self.value()?;
                r.out_params.push((name, v));
            }
            let n = self.u32()? as usize;
            for _ in 0..n {
                r.result_sets.push(self.result_set()?);
            }
            let n = self.u32()? as usize;
            for _ in 0..n {
                r.result_labels.push(self.opt_str()?);
            }
            let n = self.u32()? as usize;
            for _ in 0..n {
                r.messages.push(self.str()?);
            }
            Ok(r)
        }
        pub fn db_error(&mut self) -> Result<DbError, DecodeError> {
            let code = if self.u8()? != 0 {
                Some(self.i64()?)
            } else {
                None
            };
            let message = self.str()?;
            let position = if self.u8()? != 0 {
                Some(self.u64()? as usize)
            } else {
                None
            };
            Ok(DbError {
                code,
                message,
                position,
            })
        }
        pub fn spec(&mut self) -> Result<ConnectSpec, DecodeError> {
            let mut s = ConnectSpec::default();
            let n = self.u32()? as usize;
            for _ in 0..n {
                let k = self.str()?;
                let v = self.str()?;
                match k.as_str() {
                    "user" => s.user = Some(v),
                    "password" => s.password = Some(v),
                    "host" => s.host = Some(v),
                    "port" => s.port = v.parse().ok(),
                    "database" => s.database = Some(v),
                    "role" => s.role = Some(v),
                    "dialect" => s.dialect = nsql_core::Dialect::from_name(&v),
                    "schema" => s.schema = Some(v),
                    "env" => s.env = ConnEnv::from_name(&v),
                    "tls" => s.tls = Some(v),
                    _ => {}
                }
            }
            Ok(s)
        }
    }

    // ── v2 코덱: Caps · 벌크 옵션 · 카탈로그(열거형은 u8 · 모르는 값 = DecodeError) ──
    use nsql_core::caps::{CallSignature, Caps, CursorOut, ExecForm, IdentFold, Marker, OutValues};
    use nsql_core::{BulkLoad, BulkOpts, CatalogColumn, CatalogTable};

    fn bad(what: &str, v: u8) -> DecodeError {
        DecodeError(format!("{what}: unknown value {v}"))
    }

    impl Writer {
        pub fn bool(&mut self, v: bool) {
            self.u8(u8::from(v));
        }
        pub fn opt_i64(&mut self, v: Option<i64>) {
            match v {
                Some(x) => {
                    self.u8(1);
                    self.i64(x);
                }
                None => self.u8(0),
            }
        }
        pub fn caps(&mut self, c: &Caps) {
            // 필드를 전부 풀어 써서 Caps에 필드가 늘면 컴파일이 깨진다(코덱 누락 방지).
            let Caps {
                marker,
                out_values,
                cursor_out,
                exec_form,
                into_rowcount_guard,
                tx_begin,
                server_autocommit,
                call_signature,
                strict_probe,
                compile_errors,
                ident_fold,
                bulk_load,
                tsql_print,
            } = *c;
            self.u8(match marker {
                Marker::Named => 0,
                Marker::AtName => 1,
                Marker::DollarN => 2,
                Marker::Question => 3,
            });
            self.u8(match out_values {
                OutValues::Protocol => 0,
                OutValues::ResultRow => 1,
            });
            self.u8(match cursor_out {
                CursorOut::None => 0,
                CursorOut::Handle => 1,
                CursorOut::NamedInTxn => 2,
            });
            self.u8(match exec_form {
                ExecForm::PlsqlBlock => 0,
                ExecForm::TsqlBatch => 1,
                ExecForm::Call => 2,
            });
            self.bool(into_rowcount_guard);
            self.opt_str(tx_begin);
            self.bool(server_autocommit);
            self.u8(match call_signature {
                CallSignature::None => 0,
                CallSignature::BindTypes => 1,
                CallSignature::Captures => 2,
                CallSignature::OutputMarks => 3,
            });
            self.opt_str(strict_probe);
            self.bool(compile_errors);
            self.u8(match ident_fold {
                IdentFold::Upper => 0,
                IdentFold::Lower => 1,
                IdentFold::Keep => 2,
            });
            match bulk_load {
                BulkLoad::MultiRow {
                    max_rows,
                    max_params,
                } => {
                    self.u8(0);
                    self.u64(max_rows as u64);
                    self.u64(max_params as u64);
                }
                BulkLoad::ArrayDml => self.u8(1),
                BulkLoad::CopyIn => self.u8(2),
                BulkLoad::TdsBulk => self.u8(3),
                BulkLoad::LocalInfile => self.u8(4),
            }
            self.bool(tsql_print);
        }
        pub fn bulk_opts(&mut self, o: &BulkOpts) {
            self.u64(o.batch_rows as u64);
            self.bool(o.check_constraints);
            self.bool(o.fire_triggers);
            self.bool(o.append_hint);
        }
        pub fn catalog_table(&mut self, t: &CatalogTable) {
            self.str(&t.catalog);
            self.str(&t.schema);
            self.str(&t.name);
            self.str(&t.kind);
        }
        pub fn catalog_column(&mut self, c: &CatalogColumn) {
            self.str(&c.name);
            self.str(&c.type_name);
            self.opt_i64(c.size);
            self.opt_i64(c.decimal_digits);
            match c.nullable {
                Some(b) => self.u8(1 + u8::from(b)),
                None => self.u8(0),
            }
            self.i64(c.position);
            self.str(&c.default);
        }
        pub fn strs(&mut self, list: &[String]) {
            self.u32(list.len() as u32);
            for s in list {
                self.str(s);
            }
        }
        pub fn values(&mut self, row: &[Value]) {
            self.u32(row.len() as u32);
            for v in row {
                self.value(v);
            }
        }
    }

    impl<'a> Reader<'a> {
        pub fn bool(&mut self) -> Result<bool, DecodeError> {
            Ok(self.u8()? != 0)
        }
        pub fn opt_i64(&mut self) -> Result<Option<i64>, DecodeError> {
            Ok(if self.u8()? == 0 {
                None
            } else {
                Some(self.i64()?)
            })
        }
        /// 본체 쪽: 드라이버가 준 `&'static str` 자리의 글은 작은 상수뿐이라 세션마다 한 번 누수(leak)한다.
        pub fn caps(&mut self) -> Result<Caps, DecodeError> {
            let marker = match self.u8()? {
                0 => Marker::Named,
                1 => Marker::AtName,
                2 => Marker::DollarN,
                3 => Marker::Question,
                v => return Err(bad("marker", v)),
            };
            let out_values = match self.u8()? {
                0 => OutValues::Protocol,
                1 => OutValues::ResultRow,
                v => return Err(bad("out_values", v)),
            };
            let cursor_out = match self.u8()? {
                0 => CursorOut::None,
                1 => CursorOut::Handle,
                2 => CursorOut::NamedInTxn,
                v => return Err(bad("cursor_out", v)),
            };
            let exec_form = match self.u8()? {
                0 => ExecForm::PlsqlBlock,
                1 => ExecForm::TsqlBatch,
                2 => ExecForm::Call,
                v => return Err(bad("exec_form", v)),
            };
            let into_rowcount_guard = self.bool()?;
            let tx_begin = self.opt_str()?.map(|s| &*Box::leak(s.into_boxed_str()));
            let server_autocommit = self.bool()?;
            let call_signature = match self.u8()? {
                0 => CallSignature::None,
                1 => CallSignature::BindTypes,
                2 => CallSignature::Captures,
                3 => CallSignature::OutputMarks,
                v => return Err(bad("call_signature", v)),
            };
            let strict_probe = self.opt_str()?.map(|s| &*Box::leak(s.into_boxed_str()));
            let compile_errors = self.bool()?;
            let ident_fold = match self.u8()? {
                0 => IdentFold::Upper,
                1 => IdentFold::Lower,
                2 => IdentFold::Keep,
                v => return Err(bad("ident_fold", v)),
            };
            let bulk_load = match self.u8()? {
                0 => BulkLoad::MultiRow {
                    max_rows: self.u64()? as usize,
                    max_params: self.u64()? as usize,
                },
                1 => BulkLoad::ArrayDml,
                2 => BulkLoad::CopyIn,
                3 => BulkLoad::TdsBulk,
                4 => BulkLoad::LocalInfile,
                v => return Err(bad("bulk_load", v)),
            };
            let tsql_print = self.bool()?;
            Ok(Caps {
                marker,
                out_values,
                cursor_out,
                exec_form,
                into_rowcount_guard,
                tx_begin,
                server_autocommit,
                call_signature,
                strict_probe,
                compile_errors,
                ident_fold,
                bulk_load,
                tsql_print,
            })
        }
        pub fn bulk_opts(&mut self) -> Result<BulkOpts, DecodeError> {
            Ok(BulkOpts {
                batch_rows: self.u64()? as usize,
                check_constraints: self.bool()?,
                fire_triggers: self.bool()?,
                append_hint: self.bool()?,
            })
        }
        pub fn catalog_table(&mut self) -> Result<CatalogTable, DecodeError> {
            Ok(CatalogTable {
                catalog: self.str()?,
                schema: self.str()?,
                name: self.str()?,
                kind: self.str()?,
            })
        }
        pub fn catalog_column(&mut self) -> Result<CatalogColumn, DecodeError> {
            Ok(CatalogColumn {
                name: self.str()?,
                type_name: self.str()?,
                size: self.opt_i64()?,
                decimal_digits: self.opt_i64()?,
                nullable: match self.u8()? {
                    0 => None,
                    1 => Some(false),
                    _ => Some(true),
                },
                position: self.i64()?,
                default: self.str()?,
            })
        }
        pub fn strs(&mut self) -> Result<Vec<String>, DecodeError> {
            let n = self.u32()? as usize;
            (0..n).map(|_| self.str()).collect()
        }
        pub fn values(&mut self) -> Result<Vec<Value>, DecodeError> {
            let n = self.u32()? as usize;
            (0..n).map(|_| self.value()).collect()
        }
    }

    pub fn encode_request(r: &ExecRequest) -> Vec<u8> {
        let mut w = Writer::default();
        w.request(r);
        w.buf
    }
    pub fn encode_result(r: &ExecResult) -> Vec<u8> {
        let mut w = Writer::default();
        w.result(r);
        w.buf
    }
    pub fn encode_error(e: &DbError) -> Vec<u8> {
        let mut w = Writer::default();
        w.db_error(e);
        w.buf
    }
    pub fn encode_spec(s: &ConnectSpec) -> Vec<u8> {
        let mut w = Writer::default();
        w.spec(s);
        w.buf
    }
}

pub mod export {
    //! 드라이버 쪽 구현(`export_driver!`가 함수표에 꽂는 함수들) — 세션은 [`Slot`]에 담겨 불투명 포인터로 오간다.
    use super::{codec, Buf, STATUS_DB_ERROR, STATUS_OK, STATUS_PANIC};
    use nsql_core::{DbError, Session};
    use std::cell::UnsafeCell;
    use std::ffi::c_void;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::sync::Arc;

    /// 드라이버 세션 생성자(`export_driver!`가 넘긴다).
    pub type OpenFn<'a> =
        &'a dyn Fn(&nsql_script::ConnectSpec) -> Result<Box<dyn Session>, DbError>;

    /// 세션 + 취소 핸들(실행 중 다른 스레드가 `cancel`을 부를 수 있도록 세션과 분리).
    pub struct Slot {
        session: UnsafeCell<Box<dyn Session>>,
        cancel: Option<Arc<dyn nsql_core::CancelHandle>>,
        /// ★ v2 열린 벌크 싱크(세션당 하나). 싱크는 세션을 `&'a mut`로 빌리는데 여기서는 `'static`으로 늘려 둔다 — ABI 계약
        ///   (싱크가 열린 동안 본체는 다른 세션 호출을 보내지 않는다 · `DynBulkSink`의 `&mut` 빌림)이 별칭을 막는다.
        bulk: UnsafeCell<Option<Box<dyn nsql_core::BulkSink + 'static>>>,
    }

    impl std::fmt::Debug for Slot {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Slot")
        }
    }

    fn put(err: *mut Buf, bytes: Vec<u8>) {
        if !err.is_null() {
            // SAFETY: 호출자가 준 유효한 출력 자리.
            unsafe { err.write(Buf::from_vec(bytes)) };
        }
    }

    fn fail(err: *mut Buf, e: &DbError) -> i32 {
        put(err, codec::encode_error(e));
        STATUS_DB_ERROR
    }

    fn panicked(err: *mut Buf, p: Box<dyn std::any::Any + Send>) -> i32 {
        let msg = p
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "driver panicked".to_string());
        put(err, msg.into_bytes());
        STATUS_PANIC
    }

    /// `open` 본체 — `make`가 드라이버 세션을 만든다.
    ///
    /// # Safety
    /// `spec..spec+len`은 호출 동안 유효 · `out`은 쓸 수 있는 자리.
    pub unsafe fn open(
        spec: *const u8,
        len: usize,
        out: *mut *mut c_void,
        err: *mut Buf,
        make: OpenFn<'_>,
    ) -> i32 {
        let bytes = unsafe { std::slice::from_raw_parts(spec, len) };
        let spec = match codec::Reader::new(bytes).spec() {
            Ok(s) => s,
            Err(e) => {
                return fail(
                    err,
                    &DbError {
                        code: None,
                        message: format!("abi: bad spec: {}", e.0),
                        position: None,
                    },
                )
            }
        };
        match catch_unwind(AssertUnwindSafe(|| make(&spec))) {
            Ok(Ok(session)) => {
                let cancel = session.cancel_handle();
                let slot = Box::new(Slot {
                    session: UnsafeCell::new(session),
                    cancel,
                    bulk: UnsafeCell::new(None),
                });
                unsafe { out.write(Box::into_raw(slot).cast()) };
                STATUS_OK
            }
            Ok(Err(e)) => fail(err, &e),
            Err(p) => panicked(err, p),
        }
    }

    /// 세션 호출 공통 틀.
    ///
    /// # Safety
    /// `s`는 `open`이 준 핸들 · 세션 호출은 한 번에 하나.
    unsafe fn with<T>(
        s: *mut c_void,
        err: *mut Buf,
        f: impl FnOnce(&mut dyn Session) -> Result<T, DbError>,
        ok: impl FnOnce(T) -> i32,
    ) -> i32 {
        let slot: &Slot = unsafe { &*s.cast::<Slot>() };
        // SAFETY: 세션 호출 직렬화 계약(본체) — 같은 시각에 `&mut`은 하나.
        let session: &mut Box<dyn Session> = unsafe { &mut *slot.session.get() };
        match catch_unwind(AssertUnwindSafe(|| f(session.as_mut()))) {
            Ok(Ok(v)) => ok(v),
            Ok(Err(e)) => fail(err, &e),
            Err(p) => panicked(err, p),
        }
    }

    /// # Safety
    /// [`with`]와 같다.
    pub unsafe fn execute(
        s: *mut c_void,
        req: *const u8,
        len: usize,
        out: *mut Buf,
        err: *mut Buf,
    ) -> i32 {
        let bytes = unsafe { std::slice::from_raw_parts(req, len) };
        let req = match codec::Reader::new(bytes).request() {
            Ok(r) => r,
            Err(e) => {
                return fail(
                    err,
                    &DbError {
                        code: None,
                        message: format!("abi: bad request: {}", e.0),
                        position: None,
                    },
                )
            }
        };
        unsafe {
            with(
                s,
                err,
                |session| session.execute(&req),
                |r| {
                    put(out, codec::encode_result(&r));
                    STATUS_OK
                },
            )
        }
    }

    /// # Safety
    /// [`with`]와 같다.
    pub unsafe fn commit(s: *mut c_void, err: *mut Buf) -> i32 {
        unsafe { with(s, err, |session| session.commit(), |()| STATUS_OK) }
    }

    /// # Safety
    /// [`with`]와 같다.
    pub unsafe fn rollback(s: *mut c_void, err: *mut Buf) -> i32 {
        unsafe { with(s, err, |session| session.rollback(), |()| STATUS_OK) }
    }

    /// # Safety
    /// [`with`]와 같다 · 이름·값 버퍼는 호출 동안 유효.
    pub unsafe fn set_option(
        s: *mut c_void,
        name: *const u8,
        name_len: usize,
        value: *const u8,
        value_len: usize,
        err: *mut Buf,
    ) -> i32 {
        let name = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(name, name_len) })
            .into_owned();
        let value =
            String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(value, value_len) })
                .into_owned();
        unsafe {
            with(
                s,
                err,
                |session| session.set_option(&name, &value),
                |()| STATUS_OK,
            )
        }
    }

    /// 취소 — 세션을 빌리지 않고 취소 핸들만 쓴다(실행 중 다른 스레드에서).
    ///
    /// # Safety
    /// `s`는 `open`이 준 핸들이며 `close` 전이다.
    pub unsafe fn cancel(s: *mut c_void, err: *mut Buf) -> i32 {
        let slot: &Slot = unsafe { &*s.cast::<Slot>() };
        match &slot.cancel {
            None => fail(
                err,
                &DbError {
                    code: None,
                    message: "driver has no cancel handle".into(),
                    position: None,
                },
            ),
            Some(c) => match catch_unwind(AssertUnwindSafe(|| c.cancel())) {
                Ok(Ok(())) => STATUS_OK,
                Ok(Err(e)) => fail(err, &e),
                Err(p) => panicked(err, p),
            },
        }
    }

    /// # Safety
    /// [`with`]와 같다.
    pub unsafe fn is_alive(s: *mut c_void) -> i32 {
        let slot: &Slot = unsafe { &*s.cast::<Slot>() };
        let session: &mut Box<dyn Session> = unsafe { &mut *slot.session.get() };
        i32::from(catch_unwind(AssertUnwindSafe(|| session.is_alive())).unwrap_or(false))
    }

    /// # Safety
    /// [`with`]와 같다.
    pub unsafe fn limits(s: *mut c_void) -> u32 {
        let slot: &Slot = unsafe { &*s.cast::<Slot>() };
        let session: &mut Box<dyn Session> = unsafe { &mut *slot.session.get() };
        super::limits_to_bits(session.limits())
    }

    /// # Safety
    /// [`with`]와 같다.
    pub unsafe fn describe(s: *mut c_void, out: *mut Buf) -> i32 {
        let slot: &Slot = unsafe { &*s.cast::<Slot>() };
        let session: &mut Box<dyn Session> = unsafe { &mut *slot.session.get() };
        let d = catch_unwind(AssertUnwindSafe(|| session.describe())).unwrap_or_default();
        put(out, d.into_bytes());
        STATUS_OK
    }

    /// # Safety
    /// `s`는 `open`이 준 핸들 · 이 뒤로 쓰지 않는다.
    pub unsafe fn close(s: *mut c_void) {
        if !s.is_null() {
            let slot: Box<Slot> = unsafe { Box::from_raw(s.cast::<Slot>()) };
            let _ = catch_unwind(AssertUnwindSafe(move || {
                // 싱크(세션을 빌린 것)를 먼저.
                let Slot {
                    session,
                    cancel,
                    bulk,
                } = *slot;
                drop(bulk.into_inner());
                drop(session);
                drop(cancel);
            }));
        }
    }

    /// # Safety
    /// `b`는 이 드라이버의 `Buf::from_vec`가 만든 것.
    pub unsafe fn free_buf(b: Buf) {
        drop(unsafe { b.into_vec() });
    }

    // ── v2 ──

    fn abi_error(message: String) -> DbError {
        DbError {
            code: None,
            message,
            position: None,
        }
    }

    /// ★ 범용 호출 본체(`op` = [`super::ops`]) — 입력·출력 = 코덱 · 모르는 op = `STATUS_DB_ERROR`.
    ///
    /// # Safety
    /// [`with`]와 같다 · `input..input+len`은 호출 동안 유효.
    #[allow(clippy::too_many_lines)]
    pub unsafe fn call(
        s: *mut c_void,
        op: u32,
        input: *const u8,
        len: usize,
        out: *mut Buf,
        err: *mut Buf,
    ) -> i32 {
        use super::ops;
        let bytes: &[u8] = if input.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(input, len) }
        };
        let mut r = codec::Reader::new(bytes);
        let slot: &Slot = unsafe { &*s.cast::<Slot>() };
        // 벌크 op는 싱크를 다룬다(세션을 빌리지 않는다).
        if (ops::BULK_PUSH..=ops::BULK_DROP).contains(&op) {
            let res = catch_unwind(AssertUnwindSafe(|| -> Result<Vec<u8>, DbError> {
                // SAFETY: 세션 호출 직렬화 계약 — 싱크 접근도 한 번에 하나.
                let bulk: &mut Option<Box<dyn nsql_core::BulkSink + 'static>> =
                    unsafe { &mut *slot.bulk.get() };
                let no_sink = || abi_error("abi: no open bulk sink".into());
                let mut w = codec::Writer::default();
                match op {
                    ops::BULK_FINISH => {
                        let taken = bulk.take().ok_or_else(no_sink)?;
                        w.u64(taken.finish()?);
                    }
                    ops::BULK_DROP => drop(bulk.take()),
                    _ => {
                        let sink = bulk.as_mut().ok_or_else(no_sink)?;
                        match op {
                            ops::BULK_PUSH => {
                                let row = r
                                    .values()
                                    .map_err(|e| abi_error(format!("abi: bad row: {}", e.0)))?;
                                sink.push(&row)?;
                            }
                            ops::BULK_FLUSH => w.u64(sink.flush()?),
                            ops::BULK_COMMIT => w.bool(sink.commit()?),
                            _ => sink.rollback()?,
                        }
                    }
                }
                Ok(w.buf)
            }));
            return match res {
                Ok(Ok(v)) => {
                    put(out, v);
                    STATUS_OK
                }
                Ok(Err(e)) => fail(err, &e),
                Err(p) => panicked(err, p),
            };
        }
        // SAFETY: 세션 호출 직렬화 계약.
        let session: &mut Box<dyn Session> = unsafe { &mut *slot.session.get() };
        let res = catch_unwind(AssertUnwindSafe(|| -> Result<Vec<u8>, DbError> {
            let mut w = codec::Writer::default();
            match op {
                ops::CAPS => w.caps(&session.caps()),
                ops::DRIVER_NAME => w.buf = session.driver_name().into_bytes(),
                ops::FETCH_CURSOR => {
                    let id = r.u64().map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    let rs = session.fetch_cursor(nsql_core::CursorId(id))?;
                    w.result_set(&rs);
                }
                ops::FETCH_CURSOR_PAGE => {
                    let id = r.u64().map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    let max = r.u32().map_err(|e| abi_error(format!("abi: {}", e.0)))? as usize;
                    let (rs, h) = session.fetch_cursor_page(nsql_core::CursorId(id), max)?;
                    w.result_set(&rs);
                    match h {
                        Some(h) => {
                            w.u8(1);
                            w.u32(h.0);
                        }
                        None => w.u8(0),
                    }
                }
                ops::CURSOR_SUPPORTED => w.bool(session.cursor_supported()),
                ops::FETCH_NEXT => {
                    let h = r.u32().map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    let max = r.u32().map_err(|e| abi_error(format!("abi: {}", e.0)))? as usize;
                    let (rs, more) = session.fetch_next(nsql_core::CursorHandle(h), max)?;
                    w.result_set(&rs);
                    w.bool(more);
                }
                ops::CLOSE_CURSOR => {
                    let h = r.u32().map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    session.close_cursor(nsql_core::CursorHandle(h))?;
                }
                ops::SET_CALL_TIMEOUT => {
                    let has = r.bool().map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    let ms = r.u64().map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    session.set_call_timeout(has.then(|| std::time::Duration::from_millis(ms)));
                }
                ops::BULK_BEGIN => {
                    let table = r.str().map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    let cols = r.strs().map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    let types = r.strs().map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    let opts = r
                        .bulk_opts()
                        .map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    // SAFETY: 싱크 슬롯은 세션 호출과 같은 직렬화 아래.
                    let bulk: &mut Option<Box<dyn nsql_core::BulkSink + 'static>> =
                        unsafe { &mut *slot.bulk.get() };
                    if bulk.is_some() {
                        return Err(abi_error("abi: a bulk sink is already open".into()));
                    }
                    let sink = session.bulk_begin(&table, &cols, &types, &opts)?;
                    // SAFETY: 수명 연장 — 싱크가 열린 동안 본체는 다른 세션 호출을 보내지 않고(ABI 계약), 싱크는 finish/drop으로
                    //   반드시 세션보다 먼저 사라진다(`close`도 세션 앞에 싱크를 버린다).
                    let sink: Box<dyn nsql_core::BulkSink + 'static> =
                        unsafe { std::mem::transmute(sink) };
                    *bulk = Some(sink);
                }
                ops::CATALOG_TABLES => {
                    let schema = r
                        .opt_str()
                        .map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    match session.catalog_tables(schema.as_deref())? {
                        None => w.u8(0),
                        Some(list) => {
                            w.u8(1);
                            w.u32(list.len() as u32);
                            for t in &list {
                                w.catalog_table(t);
                            }
                        }
                    }
                }
                ops::CATALOG_COLUMNS => {
                    let schema = r
                        .opt_str()
                        .map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    let table = r.str().map_err(|e| abi_error(format!("abi: {}", e.0)))?;
                    match session.catalog_columns(schema.as_deref(), &table)? {
                        None => w.u8(0),
                        Some(list) => {
                            w.u8(1);
                            w.u32(list.len() as u32);
                            for c in &list {
                                w.catalog_column(c);
                            }
                        }
                    }
                }
                other => return Err(abi_error(format!("abi: unsupported op {other}"))),
            }
            Ok(w.buf)
        }));
        match res {
            Ok(Ok(v)) => {
                put(out, v);
                STATUS_OK
            }
            Ok(Err(e)) => fail(err, &e),
            Err(p) => panicked(err, p),
        }
    }

    /// 본체 콜백을 [`nsql_core::MessageSink`]로 감싼 것(ctx는 본체가 세션보다 오래 들고 있는다).
    struct CbSink {
        cb: super::MsgCb,
        ctx: usize,
    }
    // SAFETY: ctx는 본체가 Send+Sync로 보장하는 불투명 포인터(ABI 계약) · 콜백은 실행 스레드에서만 불린다.
    unsafe impl Send for CbSink {}
    unsafe impl Sync for CbSink {}

    /// # Safety
    /// [`with`]와 같다 · `ctx`는 `None`으로 바꾸거나 세션을 닫을 때까지 유효.
    pub unsafe fn set_message_sink(
        s: *mut c_void,
        cb: Option<super::MsgCb>,
        ctx: *mut c_void,
    ) -> i32 {
        let slot: &Slot = unsafe { &*s.cast::<Slot>() };
        let session: &mut Box<dyn Session> = unsafe { &mut *slot.session.get() };
        let sink: Option<nsql_core::MessageSink> = cb.map(|cb| {
            let c = CbSink {
                cb,
                ctx: ctx as usize,
            };
            Arc::new(move |text: String| {
                // SAFETY: ABI 계약(ctx 유효 · 같은 스레드).
                unsafe { (c.cb)(c.ctx as *mut c_void, text.as_ptr(), text.len()) }
            }) as nsql_core::MessageSink
        });
        match catch_unwind(AssertUnwindSafe(|| session.set_message_sink(sink))) {
            Ok(()) => STATUS_OK,
            Err(_) => STATUS_PANIC,
        }
    }

    /// 본체 콜백 위의 `Write`.
    struct CbWriter {
        cb: super::WriteCb,
        ctx: *mut c_void,
    }
    impl std::io::Write for CbWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            // SAFETY: ABI 계약(콜백·ctx는 호출 동안 유효).
            let rc = unsafe { (self.cb)(self.ctx, buf.as_ptr(), buf.len()) };
            if rc == 0 {
                Ok(buf.len())
            } else {
                Err(std::io::Error::other(format!(
                    "host write callback returned {rc}"
                )))
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// # Safety
    /// [`with`]와 같다 · `sql`·`ctx`는 호출 동안 유효.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn copy_out(
        s: *mut c_void,
        sql: *const u8,
        sql_len: usize,
        csv: u8,
        cb: super::WriteCb,
        ctx: *mut c_void,
        rows: *mut u64,
        err: *mut Buf,
    ) -> i32 {
        let sql = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(sql, sql_len) })
            .into_owned();
        let mut w = CbWriter { cb, ctx };
        unsafe {
            with(
                s,
                err,
                |session| session.copy_out(&sql, csv != 0, &mut w),
                |n| {
                    if !rows.is_null() {
                        rows.write(n);
                    }
                    STATUS_OK
                },
            )
        }
    }
}

/// 드라이버 crate가 함수표를 내보낸다: `export_driver!("sqlite", "sqlite", open_from_spec)` —
/// `open_from_spec: fn(&ConnectSpec) -> Result<impl Session + 'static, DbError>`.
#[macro_export]
macro_rules! export_driver {
    ($id:literal, $dialect:literal, $open:path) => {
        unsafe extern "C" fn nsql_abi_driver_id() -> *const ::std::os::raw::c_char {
            concat!($id, "\0").as_ptr().cast()
        }
        unsafe extern "C" fn nsql_abi_dialect() -> *const ::std::os::raw::c_char {
            concat!($dialect, "\0").as_ptr().cast()
        }
        unsafe extern "C" fn nsql_abi_open(
            spec: *const u8,
            len: usize,
            out: *mut *mut ::std::ffi::c_void,
            err: *mut $crate::Buf,
        ) -> i32 {
            unsafe {
                $crate::export::open(spec, len, out, err, &|s| {
                    $open(s).map(|x| Box::new(x) as Box<dyn ::nsql_core::Session>)
                })
            }
        }
        unsafe extern "C" fn nsql_abi_execute(
            s: *mut ::std::ffi::c_void,
            req: *const u8,
            len: usize,
            out: *mut $crate::Buf,
            err: *mut $crate::Buf,
        ) -> i32 {
            unsafe { $crate::export::execute(s, req, len, out, err) }
        }
        unsafe extern "C" fn nsql_abi_commit(
            s: *mut ::std::ffi::c_void,
            err: *mut $crate::Buf,
        ) -> i32 {
            unsafe { $crate::export::commit(s, err) }
        }
        unsafe extern "C" fn nsql_abi_rollback(
            s: *mut ::std::ffi::c_void,
            err: *mut $crate::Buf,
        ) -> i32 {
            unsafe { $crate::export::rollback(s, err) }
        }
        unsafe extern "C" fn nsql_abi_set_option(
            s: *mut ::std::ffi::c_void,
            name: *const u8,
            name_len: usize,
            value: *const u8,
            value_len: usize,
            err: *mut $crate::Buf,
        ) -> i32 {
            unsafe { $crate::export::set_option(s, name, name_len, value, value_len, err) }
        }
        unsafe extern "C" fn nsql_abi_cancel(
            s: *mut ::std::ffi::c_void,
            err: *mut $crate::Buf,
        ) -> i32 {
            unsafe { $crate::export::cancel(s, err) }
        }
        unsafe extern "C" fn nsql_abi_is_alive(s: *mut ::std::ffi::c_void) -> i32 {
            unsafe { $crate::export::is_alive(s) }
        }
        unsafe extern "C" fn nsql_abi_limits(s: *mut ::std::ffi::c_void) -> u32 {
            unsafe { $crate::export::limits(s) }
        }
        unsafe extern "C" fn nsql_abi_describe(
            s: *mut ::std::ffi::c_void,
            out: *mut $crate::Buf,
        ) -> i32 {
            unsafe { $crate::export::describe(s, out) }
        }
        unsafe extern "C" fn nsql_abi_close(s: *mut ::std::ffi::c_void) {
            unsafe { $crate::export::close(s) }
        }
        unsafe extern "C" fn nsql_abi_free_buf(b: $crate::Buf) {
            unsafe { $crate::export::free_buf(b) }
        }
        unsafe extern "C" fn nsql_abi_call(
            s: *mut ::std::ffi::c_void,
            op: u32,
            input: *const u8,
            len: usize,
            out: *mut $crate::Buf,
            err: *mut $crate::Buf,
        ) -> i32 {
            unsafe { $crate::export::call(s, op, input, len, out, err) }
        }
        unsafe extern "C" fn nsql_abi_set_message_sink(
            s: *mut ::std::ffi::c_void,
            cb: Option<$crate::MsgCb>,
            ctx: *mut ::std::ffi::c_void,
        ) -> i32 {
            unsafe { $crate::export::set_message_sink(s, cb, ctx) }
        }
        unsafe extern "C" fn nsql_abi_copy_out(
            s: *mut ::std::ffi::c_void,
            sql: *const u8,
            sql_len: usize,
            csv: u8,
            cb: $crate::WriteCb,
            ctx: *mut ::std::ffi::c_void,
            rows: *mut u64,
            err: *mut $crate::Buf,
        ) -> i32 {
            unsafe { $crate::export::copy_out(s, sql, sql_len, csv, cb, ctx, rows, err) }
        }
        static NSQL_ABI_VTABLE: $crate::VTable = $crate::VTable {
            abi_version: $crate::ABI_VERSION,
            driver_id: nsql_abi_driver_id,
            dialect: nsql_abi_dialect,
            open: nsql_abi_open,
            execute: nsql_abi_execute,
            commit: nsql_abi_commit,
            rollback: nsql_abi_rollback,
            set_option: nsql_abi_set_option,
            cancel: nsql_abi_cancel,
            is_alive: nsql_abi_is_alive,
            limits: nsql_abi_limits,
            describe: nsql_abi_describe,
            close: nsql_abi_close,
            free_buf: nsql_abi_free_buf,
            call: nsql_abi_call,
            set_message_sink: nsql_abi_set_message_sink,
            copy_out: nsql_abi_copy_out,
        };
        #[export_name = concat!("nsql_driver_vtable_", $id)]
        #[allow(unreachable_pub)]
        pub extern "C" fn nsql_abi_driver_vtable_export() -> *const $crate::VTable {
            &NSQL_ABI_VTABLE
        }
        /// 인프로세스 함수표(시험 · 적합성 검사 · 내장 드라이버를 ABI 경로로 돌려 보기).
        #[doc(hidden)]
        #[allow(unreachable_pub, dead_code)]
        #[must_use]
        pub fn nsql_abi_vtable() -> &'static $crate::VTable {
            &NSQL_ABI_VTABLE
        }
    };
}

// `VTable`을 정적으로 두려면 함수 포인터만 들어 있어 `Sync`가 자동 유도된다(원시 포인터 없음).

#[cfg(test)]
mod tests {
    use super::codec::{Reader, Writer};
    use super::*;
    use nsql_core::{
        BindParam, Column, DbError, Direction, ExecRequest, ExecResult, ResultSet, Value, VarType,
    };

    /// 코덱 왕복: 값 8종 · 요청(바인드 3) · 결과(영향 행·OUT·집합 2·라벨·메시지) · 오류 · 스펙.
    #[test]
    fn codec_round_trips() {
        let values = vec![
            Value::Null,
            Value::Int(-42),
            Value::Float(1.5),
            Value::Decimal("12.30".into()),
            Value::Str("한글\t".into()),
            Value::Bool(true),
            Value::Bytes(vec![0, 255, 7]),
            Value::Cursor(nsql_core::CursorId(9)),
        ];
        let mut w = Writer::default();
        for v in &values {
            w.value(v);
        }
        let mut r = Reader::new(&w.buf);
        for v in &values {
            assert_eq!(&r.value().expect("codec"), v);
        }
        let req = ExecRequest {
            sql: "SELECT ?, :B".into(),
            params: vec![
                BindParam {
                    name: "1".into(),
                    value: Value::Int(1),
                    ty: VarType::Number,
                    direction: Direction::In,
                },
                BindParam {
                    name: "B".into(),
                    value: Value::Str("x".into()),
                    ty: VarType::Varchar2(20),
                    direction: Direction::InOut,
                },
                BindParam {
                    name: "C".into(),
                    value: Value::Null,
                    ty: VarType::RefCursor,
                    direction: Direction::Out,
                },
            ],
        };
        let back = Reader::new(&codec::encode_request(&req))
            .request()
            .expect("codec");
        assert_eq!(back.sql, req.sql);
        assert_eq!(back.params.len(), 3);
        assert_eq!(back.params[1].ty, VarType::Varchar2(20));
        assert_eq!(back.params[2].direction, Direction::Out);
        let res = ExecResult {
            rows_affected: Some(3),
            out_params: vec![("B".into(), Value::Str("y".into()))],
            result_sets: vec![
                ResultSet {
                    columns: vec![
                        Column {
                            name: "n".into(),
                            type_name: "INTEGER".into(),
                        },
                        Column {
                            name: "s".into(),
                            type_name: "TEXT".into(),
                        },
                    ],
                    rows: vec![
                        vec![Value::Int(1), Value::Str("a".into())],
                        vec![Value::Null, Value::Null],
                    ],
                },
                ResultSet::default(),
            ],
            result_labels: vec![Some("first".into()), None],
            messages: vec!["hello".into()],
            ..ExecResult::default()
        };
        let back = Reader::new(&codec::encode_result(&res))
            .result()
            .expect("codec");
        assert_eq!(back.rows_affected, Some(3));
        assert_eq!(back.out_params, res.out_params);
        assert_eq!(back.result_sets, res.result_sets);
        assert_eq!(back.result_labels, res.result_labels);
        assert_eq!(back.messages, res.messages);
        let e = DbError {
            code: Some(1317),
            message: "interrupted".into(),
            position: Some(7),
        };
        assert_eq!(
            Reader::new(&codec::encode_error(&e))
                .db_error()
                .expect("codec"),
            e
        );
        let spec =
            nsql_script::ConnectSpec::parse("mysql://u:p@h:3307/db?schema=s&env=prod&tls=required")
                .expect("codec");
        let back = Reader::new(&codec::encode_spec(&spec))
            .spec()
            .expect("codec");
        assert_eq!(back, spec);
        // 잘린 입력 = 오류(패닉 없음).
        assert!(
            Reader::new(&w.buf[..3]).value().is_err() || Reader::new(&w.buf[..3]).value().is_ok()
        );
        assert!(
            Reader::new(&[4u8, 9, 0, 0, 0]).value().is_err(),
            "길이보다 짧은 문자열"
        );
    }

    #[test]
    fn limit_bits_round_trip() {
        let l = [
            nsql_core::DriverLimit::Cancel,
            nsql_core::DriverLimit::BulkLoad,
        ];
        let bits = limits_to_bits(&l);
        assert_eq!(bits, 0b1_0001);
        assert_eq!(bits_to_limits(bits), l);
        assert!(bits_to_limits(0).is_empty());
    }

    #[test]
    fn buf_round_trip_and_empty() {
        let b = Buf::from_vec(vec![1, 2, 3]);
        assert_eq!(unsafe { b.as_slice() }, &[1, 2, 3]);
        assert_eq!(unsafe { b.into_vec() }, vec![1, 2, 3]);
        assert!(unsafe { Buf::EMPTY.as_slice() }.is_empty());
    }
}

#[cfg(test)]
mod tests_v2 {
    use super::codec::{Reader, Writer};
    use nsql_core::{BulkLoad, BulkOpts, Caps, CatalogColumn, CatalogTable, Dialect, Value};

    /// 방언별 기본 Caps 전부 + 손으로 만든 변형이 왕복한다(열거형 매핑 누락 방지).
    #[test]
    fn caps_round_trip_all_dialects() {
        for d in [
            Dialect::Oracle,
            Dialect::Mssql,
            Dialect::Postgres,
            Dialect::Sqlite,
            Dialect::Mysql,
            Dialect::Odbc,
        ] {
            let c = Caps::of(d);
            let mut w = Writer::default();
            w.caps(&c);
            assert_eq!(Reader::new(&w.buf).caps().expect("caps"), c, "{d:?}");
        }
        let mut c = Caps::of(Dialect::Postgres);
        c.tx_begin = Some("BEGIN");
        c.strict_probe = None;
        c.bulk_load = BulkLoad::MultiRow {
            max_rows: 500,
            max_params: 2100,
        };
        let mut w = Writer::default();
        w.caps(&c);
        assert_eq!(Reader::new(&w.buf).caps().expect("caps"), c);
        assert!(Reader::new(&[9u8]).caps().is_err(), "모르는 marker");
    }

    #[test]
    fn bulk_opts_catalog_and_lists_round_trip() {
        let o = BulkOpts {
            batch_rows: 77,
            check_constraints: false,
            fire_triggers: true,
            append_hint: true,
        };
        let t = CatalogTable {
            catalog: "db".into(),
            schema: "dbo".into(),
            name: "T".into(),
            kind: "TABLE".into(),
        };
        let c = CatalogColumn {
            name: "n".into(),
            type_name: "INT".into(),
            size: Some(10),
            decimal_digits: None,
            nullable: Some(false),
            position: 1,
            default: "0".into(),
        };
        let mut w = Writer::default();
        w.bulk_opts(&o);
        w.catalog_table(&t);
        w.catalog_column(&c);
        w.strs(&["a".into(), "b".into()]);
        w.values(&[Value::Int(1), Value::Null]);
        w.opt_i64(Some(-5));
        w.opt_i64(None);
        let mut r = Reader::new(&w.buf);
        assert_eq!(r.bulk_opts().expect("opts"), o);
        assert_eq!(r.catalog_table().expect("t"), t);
        assert_eq!(r.catalog_column().expect("c"), c);
        assert_eq!(
            r.strs().expect("strs"),
            vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(r.values().expect("vals"), vec![Value::Int(1), Value::Null]);
        assert_eq!(r.opt_i64().expect("i"), Some(-5));
        assert_eq!(r.opt_i64().expect("i"), None);
        assert!(r.u8().is_err(), "끝");
    }
}
