//! ★ 확장 드라이버 로더(DR-29 · T-27 ② · docs/22 §0 · 64 §3-4): `nsql-abi` 함수표를 내보내는 **cdylib**를 첫 접속 때
//! `LoadLibraryW`/`dlopen`으로 올리고(외부 crate 0 · 기동 비용 0) [`DynSession`]이 [`nsql_core::Session`]으로 감싼다.
//!
//! - 라이브러리는 **내리지 않는다**(64 §5-2: Rust cdylib 언로드는 위험 · 메모리 회수는 프로세스 격리 모드의 몫).
//! - 세션 호출은 한 번에 하나(워커 스레드) · 취소만 [`DynCancel`]이 다른 스레드에서 `cancel`을 부른다(ABI 계약).
//! - 결과·오류 버퍼는 드라이버의 `free_buf`로 돌려준다(할당자 경계).
use nsql_abi::{codec, ops, Buf, VTable, ABI_VERSION, STATUS_DB_ERROR, STATUS_OK};
use nsql_core::{
    BulkOpts, BulkSink, Caps, CatalogColumn, CatalogTable, CursorHandle, CursorId, DbError,
    Dialect, DriverLimit, ExecRequest, ExecResult, MessageSink, ResultSet, Session,
};
use nsql_script::ConnectSpec;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn abi_err(message: impl Into<String>) -> DbError {
    DbError {
        code: None,
        message: message.into(),
        position: None,
    }
}

/// 올라온 드라이버 라이브러리 하나(함수표 + 경로 · 내리지 않음).
#[derive(Debug)]
pub struct DynDriver {
    vt: &'static VTable,
    path: PathBuf,
    id: String,
    dialect: Dialect,
}

#[cfg(windows)]
mod os {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        fn GetLastError() -> u32;
    }
    pub(crate) fn load(path: &Path) -> Result<*mut c_void, String> {
        let mut w: Vec<u16> = path.as_os_str().encode_wide().collect();
        w.push(0);
        // SAFETY: NUL 종료 UTF-16 경로.
        let h = unsafe { LoadLibraryW(w.as_ptr()) };
        if h.is_null() {
            Err(format!("LoadLibraryW failed (error {})", unsafe {
                GetLastError()
            }))
        } else {
            Ok(h)
        }
    }
    pub(crate) fn symbol(h: *mut c_void, name_c: &[u8]) -> Option<*mut c_void> {
        // SAFETY: `name_c`는 NUL 종료.
        let p = unsafe { GetProcAddress(h, name_c.as_ptr()) };
        (!p.is_null()).then_some(p)
    }
}

#[cfg(unix)]
mod os {
    use std::ffi::c_void;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    extern "C" {
        fn dlopen(filename: *const u8, flags: i32) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const u8) -> *mut c_void;
        fn dlerror() -> *const u8;
    }
    const RTLD_NOW: i32 = 2;
    #[cfg(target_os = "linux")]
    const RTLD_LOCAL: i32 = 0;
    #[cfg(not(target_os = "linux"))]
    const RTLD_LOCAL: i32 = 4;
    fn last_error() -> String {
        // SAFETY: dlerror는 NUL 종료 문자열 또는 null.
        let p = unsafe { dlerror() };
        if p.is_null() {
            return "dlopen failed".to_string();
        }
        let mut n = 0usize;
        // SAFETY: NUL까지 읽는다.
        while unsafe { *p.add(n) } != 0 {
            n += 1;
        }
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p, n) }).into_owned()
    }
    pub(crate) fn load(path: &Path) -> Result<*mut c_void, String> {
        let mut c: Vec<u8> = path.as_os_str().as_bytes().to_vec();
        c.push(0);
        // SAFETY: NUL 종료 경로.
        let h = unsafe { dlopen(c.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
        if h.is_null() {
            Err(last_error())
        } else {
            Ok(h)
        }
    }
    pub(crate) fn symbol(h: *mut c_void, name_c: &[u8]) -> Option<*mut c_void> {
        // SAFETY: NUL 종료 심볼 이름.
        let p = unsafe { dlsym(h, name_c.as_ptr()) };
        (!p.is_null()).then_some(p)
    }
}

#[cfg(not(any(windows, unix)))]
mod os {
    use std::ffi::c_void;
    use std::path::Path;
    pub fn load(_path: &Path) -> Result<*mut c_void, String> {
        Err("dynamic loading is not supported on this platform".to_string())
    }
    pub fn symbol(_h: *mut c_void, _name_c: &[u8]) -> Option<*mut c_void> {
        None
    }
}

fn cstr<'a>(p: *const std::os::raw::c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    // SAFETY: 드라이버가 준 NUL 종료 정적 문자열.
    unsafe { std::ffi::CStr::from_ptr(p) }
        .to_str()
        .unwrap_or("")
}

impl DynDriver {
    /// 라이브러리를 올리고 함수표를 확인한다(버전 불일치 = 거부).
    pub fn load(path: &Path, id: &str) -> Result<Arc<DynDriver>, DbError> {
        let h = os::load(path).map_err(|e| abi_err(format!("{}: {e}", path.display())))?;
        let sym = os::symbol(h, &nsql_abi::entry_symbol_c(id)).ok_or_else(|| {
            abi_err(format!(
                "{}: symbol {} not found",
                path.display(),
                nsql_abi::entry_symbol(id)
            ))
        })?;
        // SAFETY: 심볼은 `extern "C" fn() -> *const VTable`(ABI 계약).
        let entry: extern "C" fn() -> *const VTable = unsafe { std::mem::transmute(sym) };
        let vt = entry();
        if vt.is_null() {
            return Err(abi_err(format!("{}: null vtable", path.display())));
        }
        // SAFETY: 드라이버의 정적 함수표(프로세스 수명 동안 유효 · 라이브러리를 내리지 않는다).
        let vt: &'static VTable = unsafe { &*vt };
        Self::from_vtable(vt, path)
    }

    /// ★ 함수표에서 바로(인프로세스 · 시험·내장 드라이버를 ABI로 돌려 보는 적합성 검사용). 버전 불일치 = 거부.
    pub fn from_vtable(vt: &'static VTable, path: &Path) -> Result<Arc<DynDriver>, DbError> {
        if vt.abi_version != ABI_VERSION {
            return Err(abi_err(format!(
                "{}: ABI version {} (host {})",
                path.display(),
                vt.abi_version,
                ABI_VERSION
            )));
        }
        // SAFETY: 함수표 계약.
        let id = cstr(unsafe { (vt.driver_id)() }).to_string();
        let dialect_name = cstr(unsafe { (vt.dialect)() });
        let dialect = Dialect::from_name(dialect_name).ok_or_else(|| {
            abi_err(format!(
                "{}: unknown dialect '{dialect_name}'",
                path.display()
            ))
        })?;
        Ok(Arc::new(DynDriver {
            vt,
            path: path.to_path_buf(),
            id,
            dialect,
        }))
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    #[must_use]
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 드라이버가 준 버퍼를 읽고 돌려준다.
    fn take(&self, b: Buf) -> Vec<u8> {
        // SAFETY: 드라이버가 준 그대로 · 아래에서 드라이버의 free_buf로 해제.
        let v = unsafe { b.as_slice() }.to_vec();
        unsafe { (self.vt.free_buf)(b) };
        v
    }

    fn status(&self, st: i32, err: Buf) -> Result<(), DbError> {
        match st {
            STATUS_OK => {
                if !err.ptr.is_null() {
                    unsafe { (self.vt.free_buf)(err) };
                }
                Ok(())
            }
            STATUS_DB_ERROR => {
                let bytes = self.take(err);
                Err(codec::Reader::new(&bytes)
                    .db_error()
                    .unwrap_or_else(|e| abi_err(format!("abi: bad error payload: {}", e.0))))
            }
            _ => {
                let bytes = self.take(err);
                Err(abi_err(format!(
                    "driver {} panicked: {}",
                    self.id,
                    String::from_utf8_lossy(&bytes)
                )))
            }
        }
    }

    /// 세션을 연다.
    pub fn open(self: &Arc<Self>, spec: &ConnectSpec) -> Result<DynSession, DbError> {
        let bytes = codec::encode_spec(spec);
        let mut handle: *mut c_void = std::ptr::null_mut();
        let mut err = Buf::EMPTY;
        // SAFETY: 입력 버퍼는 호출 동안 유효 · out/err는 지역 변수.
        let st = unsafe { (self.vt.open)(bytes.as_ptr(), bytes.len(), &mut handle, &mut err) };
        self.status(st, err)?;
        if handle.is_null() {
            return Err(abi_err("driver returned null session"));
        }
        let limits = unsafe { (self.vt.limits)(handle) };
        let mut dbuf = Buf::EMPTY;
        let _ = unsafe { (self.vt.describe)(handle, &mut dbuf) };
        let description = String::from_utf8_lossy(&self.take(dbuf)).into_owned();
        let mut s = DynSession {
            drv: Arc::clone(self),
            handle,
            description,
            limits: Box::leak(nsql_abi::bits_to_limits(limits).into_boxed_slice()),
            caps: Caps::of(self.dialect),
            name: self.id.clone(),
            sink_ctx: None,
        };
        // v2: 드라이버가 말하는 Caps·이름(실패하면 방언 기본값).
        if let Ok(bytes) = s.call(ops::CAPS, &[]) {
            if let Ok(c) = codec::Reader::new(&bytes).caps() {
                s.caps = c;
            }
        }
        if let Ok(bytes) = s.call(ops::DRIVER_NAME, &[]) {
            let n = String::from_utf8_lossy(&bytes).into_owned();
            if !n.is_empty() {
                s.name = n;
            }
        }
        Ok(s)
    }
}

/// 확장 드라이버 세션(프록시). 드라이버 핸들은 이 값이 drop될 때 `close`.
pub struct DynSession {
    drv: Arc<DynDriver>,
    handle: *mut c_void,
    description: String,
    limits: &'static [DriverLimit],
    caps: Caps,
    name: String,
    /// 서버 메시지 싱크의 본체 쪽 컨텍스트(드라이버가 콜백 ctx로 받는다 · 세션보다 오래 산다).
    sink_ctx: Option<Box<SinkCtx>>,
}

/// 드라이버 콜백이 넘겨 받는 ctx — `MessageSink`를 담는다.
struct SinkCtx(MessageSink);

unsafe extern "C" fn msg_cb(ctx: *mut c_void, ptr: *const u8, len: usize) {
    if ctx.is_null() {
        return;
    }
    // SAFETY: ctx = Box<SinkCtx>의 포인터(세션이 들고 있다) · ptr..len은 호출 동안 유효.
    let sink: &SinkCtx = unsafe { &*ctx.cast::<SinkCtx>() };
    let text =
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len) }).into_owned();
    (sink.0)(text);
}

/// COPY OUT 쓰기 콜백 — ctx = `&mut dyn Write`를 담은 상자.
struct WriteCtx<'a> {
    out: &'a mut dyn std::io::Write,
    error: Option<std::io::Error>,
}

unsafe extern "C" fn write_cb(ctx: *mut c_void, ptr: *const u8, len: usize) -> i32 {
    // SAFETY: ctx = WriteCtx(호출 동안 유효).
    let w: &mut WriteCtx<'_> = unsafe { &mut *ctx.cast::<WriteCtx<'_>>() };
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    match w.out.write_all(bytes) {
        Ok(()) => 0,
        Err(e) => {
            w.error = Some(e);
            1
        }
    }
}

impl DynSession {
    /// 범용 호출(v2) — 입력 코덱 → 출력 바이트.
    fn call(&mut self, op: u32, input: &[u8]) -> Result<Vec<u8>, DbError> {
        let mut out = Buf::EMPTY;
        let mut err = Buf::EMPTY;
        // SAFETY: 입력은 호출 동안 유효 · out/err 지역 변수 · 세션 호출 직렬화.
        let st = unsafe {
            (self.drv.vt.call)(
                self.handle,
                op,
                input.as_ptr(),
                input.len(),
                &mut out,
                &mut err,
            )
        };
        self.drv.status(st, err)?;
        Ok(self.drv.take(out))
    }

    fn bad(e: codec::DecodeError) -> DbError {
        abi_err(format!("abi: bad payload: {}", e.0))
    }
}

/// 확장 드라이버의 벌크 싱크(프록시) — 세션을 `&mut`로 빌려 다른 세션 호출을 막는다(ABI 계약).
pub struct DynBulkSink<'a> {
    s: &'a mut DynSession,
    open: bool,
}

impl std::fmt::Debug for DynBulkSink<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DynBulkSink")
    }
}

impl BulkSink for DynBulkSink<'_> {
    fn push(&mut self, row: &[nsql_core::Value]) -> Result<(), DbError> {
        let mut w = codec::Writer::default();
        w.values(row);
        self.s.call(ops::BULK_PUSH, &w.buf).map(|_| ())
    }
    fn flush(&mut self) -> Result<u64, DbError> {
        let b = self.s.call(ops::BULK_FLUSH, &[])?;
        codec::Reader::new(&b).u64().map_err(DynSession::bad)
    }
    fn commit(&mut self) -> Result<bool, DbError> {
        let b = self.s.call(ops::BULK_COMMIT, &[])?;
        codec::Reader::new(&b).bool().map_err(DynSession::bad)
    }
    fn rollback(&mut self) -> Result<(), DbError> {
        self.s.call(ops::BULK_ROLLBACK, &[]).map(|_| ())
    }
    fn finish(mut self: Box<Self>) -> Result<u64, DbError> {
        self.open = false;
        let b = self.s.call(ops::BULK_FINISH, &[])?;
        codec::Reader::new(&b).u64().map_err(DynSession::bad)
    }
}

impl Drop for DynBulkSink<'_> {
    fn drop(&mut self) {
        if self.open {
            let _ = self.s.call(ops::BULK_DROP, &[]);
        }
    }
}

impl std::fmt::Debug for DynSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DynSession({})", self.drv.id)
    }
}

// SAFETY: 세션 호출은 한 스레드씩(워커가 소유) · 핸들은 드라이버 세션의 불투명 포인터 · 드라이버 세션 타입은 Send(ABI 계약).
unsafe impl Send for DynSession {}

/// 취소 핸들 — 세션과 무관하게 핸들만 들고 다른 스레드에서 `cancel`.
struct DynCancel {
    drv: Arc<DynDriver>,
    handle: usize,
}

impl nsql_core::CancelHandle for DynCancel {
    fn cancel(&self) -> Result<(), DbError> {
        let mut err = Buf::EMPTY;
        // SAFETY: ABI 계약 = cancel은 실행 중 다른 스레드에서 불러도 된다.
        let st = unsafe { (self.drv.vt.cancel)(self.handle as *mut c_void, &mut err) };
        self.drv.status(st, err)
    }
}

impl Drop for DynSession {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: open이 준 핸들 · 이 뒤로 쓰지 않는다.
            unsafe { (self.drv.vt.close)(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

impl Session for DynSession {
    fn dialect(&self) -> Dialect {
        self.drv.dialect
    }

    fn caps(&self) -> Caps {
        self.caps
    }

    fn driver_name(&self) -> String {
        format!("ext:{} · {}", self.drv.id, self.name)
    }

    fn limits(&self) -> &'static [DriverLimit] {
        self.limits
    }

    fn cancel_handle(&self) -> Option<Arc<dyn nsql_core::CancelHandle>> {
        Some(Arc::new(DynCancel {
            drv: Arc::clone(&self.drv),
            handle: self.handle as usize,
        }))
    }

    fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
        let bytes = codec::encode_request(req);
        let mut out = Buf::EMPTY;
        let mut err = Buf::EMPTY;
        // SAFETY: 입력은 호출 동안 유효 · out/err 지역 변수 · 세션 호출 직렬화.
        let st = unsafe {
            (self.drv.vt.execute)(self.handle, bytes.as_ptr(), bytes.len(), &mut out, &mut err)
        };
        self.drv.status(st, err)?;
        let payload = self.drv.take(out);
        codec::Reader::new(&payload)
            .result()
            .map_err(|e| abi_err(format!("abi: bad result payload: {}", e.0)))
    }

    fn fetch_cursor(&mut self, cursor: CursorId) -> Result<ResultSet, DbError> {
        let mut w = codec::Writer::default();
        w.u64(cursor.0);
        let b = self.call(ops::FETCH_CURSOR, &w.buf)?;
        codec::Reader::new(&b).result_set().map_err(Self::bad)
    }

    fn fetch_cursor_page(
        &mut self,
        cursor: CursorId,
        max: usize,
    ) -> Result<(ResultSet, Option<CursorHandle>), DbError> {
        let mut w = codec::Writer::default();
        w.u64(cursor.0);
        w.u32(max as u32);
        let b = self.call(ops::FETCH_CURSOR_PAGE, &w.buf)?;
        let mut r = codec::Reader::new(&b);
        let rs = r.result_set().map_err(Self::bad)?;
        let h = if r.u8().map_err(Self::bad)? != 0 {
            Some(CursorHandle(r.u32().map_err(Self::bad)?))
        } else {
            None
        };
        Ok((rs, h))
    }

    fn commit(&mut self) -> Result<(), DbError> {
        let mut err = Buf::EMPTY;
        let st = unsafe { (self.drv.vt.commit)(self.handle, &mut err) };
        self.drv.status(st, err)
    }

    fn rollback(&mut self) -> Result<(), DbError> {
        let mut err = Buf::EMPTY;
        let st = unsafe { (self.drv.vt.rollback)(self.handle, &mut err) };
        self.drv.status(st, err)
    }

    fn set_option(&mut self, name: &str, value: &str) -> Result<(), DbError> {
        let mut err = Buf::EMPTY;
        let st = unsafe {
            (self.drv.vt.set_option)(
                self.handle,
                name.as_ptr(),
                name.len(),
                value.as_ptr(),
                value.len(),
                &mut err,
            )
        };
        self.drv.status(st, err)
    }

    fn set_message_sink(&mut self, sink: Option<MessageSink>) {
        // 옛 ctx는 드라이버가 새 값을 받은 뒤에 버린다(실행 중이 아니므로 안전).
        let new_ctx = sink.map(|s| Box::new(SinkCtx(s)));
        let (cb, ctx): (Option<nsql_abi::MsgCb>, *mut c_void) = match &new_ctx {
            Some(b) => (
                Some(msg_cb),
                std::ptr::from_ref::<SinkCtx>(b.as_ref()).cast_mut().cast(),
            ),
            None => (None, std::ptr::null_mut()),
        };
        // SAFETY: ctx는 세션 필드로 들고 있어 드라이버가 쓰는 동안 살아 있다.
        let _ = unsafe { (self.drv.vt.set_message_sink)(self.handle, cb, ctx) };
        self.sink_ctx = new_ctx;
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn cursor_supported(&self) -> bool {
        // `&self`라 call을 못 쓴다 — 핸들로 직접(드라이버는 읽기만).
        let mut out = Buf::EMPTY;
        let mut err = Buf::EMPTY;
        let st = unsafe {
            (self.drv.vt.call)(
                self.handle,
                ops::CURSOR_SUPPORTED,
                std::ptr::null(),
                0,
                &mut out,
                &mut err,
            )
        };
        if self.drv.status(st, err).is_err() {
            return false;
        }
        let b = self.drv.take(out);
        codec::Reader::new(&b).bool().unwrap_or(false)
    }

    fn fetch_next(&mut self, h: CursorHandle, max: usize) -> Result<(ResultSet, bool), DbError> {
        let mut w = codec::Writer::default();
        w.u32(h.0);
        w.u32(max as u32);
        let b = self.call(ops::FETCH_NEXT, &w.buf)?;
        let mut r = codec::Reader::new(&b);
        let rs = r.result_set().map_err(Self::bad)?;
        let more = r.bool().map_err(Self::bad)?;
        Ok((rs, more))
    }

    fn close_cursor(&mut self, h: CursorHandle) -> Result<(), DbError> {
        let mut w = codec::Writer::default();
        w.u32(h.0);
        self.call(ops::CLOSE_CURSOR, &w.buf).map(|_| ())
    }

    fn is_alive(&self) -> bool {
        unsafe { (self.drv.vt.is_alive)(self.handle) != 0 }
    }

    fn set_call_timeout(&mut self, timeout: Option<std::time::Duration>) {
        let mut w = codec::Writer::default();
        w.bool(timeout.is_some());
        w.u64(timeout.map_or(0, |d| d.as_millis() as u64));
        let _ = self.call(ops::SET_CALL_TIMEOUT, &w.buf);
    }

    fn bulk_begin<'a>(
        &'a mut self,
        table: &str,
        cols: &[String],
        types: &[String],
        opts: &BulkOpts,
    ) -> Result<Box<dyn BulkSink + 'a>, DbError> {
        let mut w = codec::Writer::default();
        w.str(table);
        w.strs(cols);
        w.strs(types);
        w.bulk_opts(opts);
        self.call(ops::BULK_BEGIN, &w.buf)?;
        Ok(Box::new(DynBulkSink {
            s: self,
            open: true,
        }))
    }

    fn catalog_tables(
        &mut self,
        schema: Option<&str>,
    ) -> Result<Option<Vec<CatalogTable>>, DbError> {
        let mut w = codec::Writer::default();
        w.opt_str(schema);
        let b = self.call(ops::CATALOG_TABLES, &w.buf)?;
        let mut r = codec::Reader::new(&b);
        if r.u8().map_err(Self::bad)? == 0 {
            return Ok(None);
        }
        let n = r.u32().map_err(Self::bad)? as usize;
        let list = (0..n)
            .map(|_| r.catalog_table())
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::bad)?;
        Ok(Some(list))
    }

    fn catalog_columns(
        &mut self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<Option<Vec<CatalogColumn>>, DbError> {
        let mut w = codec::Writer::default();
        w.opt_str(schema);
        w.str(table);
        let b = self.call(ops::CATALOG_COLUMNS, &w.buf)?;
        let mut r = codec::Reader::new(&b);
        if r.u8().map_err(Self::bad)? == 0 {
            return Ok(None);
        }
        let n = r.u32().map_err(Self::bad)? as usize;
        let list = (0..n)
            .map(|_| r.catalog_column())
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::bad)?;
        Ok(Some(list))
    }

    fn copy_out(
        &mut self,
        sql: &str,
        csv: bool,
        out: &mut dyn std::io::Write,
    ) -> Result<u64, DbError> {
        let mut ctx = WriteCtx { out, error: None };
        let mut rows = 0u64;
        let mut err = Buf::EMPTY;
        // SAFETY: sql·ctx는 호출 동안 유효 · 콜백은 같은 스레드.
        let st = unsafe {
            (self.drv.vt.copy_out)(
                self.handle,
                sql.as_ptr(),
                sql.len(),
                u8::from(csv),
                write_cb,
                std::ptr::from_mut::<WriteCtx<'_>>(&mut ctx).cast(),
                &mut rows,
                &mut err,
            )
        };
        let r = self.drv.status(st, err);
        if let Some(e) = ctx.error.take() {
            return Err(abi_err(format!("copy out: write failed: {e}")));
        }
        r.map(|()| rows)
    }
}

/// `target/<profile>/` 안의 cdylib 파일 이름(OS별) — 시험·개발 편의.
#[must_use]
pub fn library_file_name(crate_name: &str) -> String {
    let stem = crate_name.replace('-', "_");
    if cfg!(windows) {
        format!("{stem}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{stem}.dylib")
    } else {
        format!("lib{stem}.so")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_names_follow_os() {
        let n = library_file_name("nsql-driver-sqlite");
        assert!(n.contains("nsql_driver_sqlite"));
        assert!(n.ends_with(".dll") || n.ends_with(".so") || n.ends_with(".dylib"));
    }

    #[test]
    fn loading_a_missing_library_is_an_error_not_a_panic() {
        let e = DynDriver::load(Path::new("no-such-dir/no-such-driver.dll"), "x")
            .expect_err("missing library");
        assert!(e.message.contains("no-such-driver"), "{e:?}");
    }
}
