//! ★ 확장 드라이버 로더(DR-29 · T-27 ② · docs/22 §0 · 64 §3-4): `nsql-abi` 함수표를 내보내는 **cdylib**를 첫 접속 때
//! `LoadLibraryW`/`dlopen`으로 올리고(외부 crate 0 · 기동 비용 0) [`DynSession`]이 [`nsql_core::Session`]으로 감싼다.
//!
//! - 라이브러리는 **내리지 않는다**(64 §5-2: Rust cdylib 언로드는 위험 · 메모리 회수는 프로세스 격리 모드의 몫).
//! - 세션 호출은 한 번에 하나(워커 스레드) · 취소만 [`DynCancel`]이 다른 스레드에서 `cancel`을 부른다(ABI 계약).
//! - 결과·오류 버퍼는 드라이버의 `free_buf`로 돌려준다(할당자 경계).
use nsql_abi::{codec, Buf, VTable, ABI_VERSION, STATUS_DB_ERROR, STATUS_OK};
use nsql_core::{Caps, DbError, Dialect, DriverLimit, ExecRequest, ExecResult, ResultSet, Session};
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
    pub fn load(path: &Path) -> Result<Arc<DynDriver>, DbError> {
        let h = os::load(path).map_err(|e| abi_err(format!("{}: {e}", path.display())))?;
        let sym = os::symbol(h, nsql_abi::ENTRY_SYMBOL_C).ok_or_else(|| {
            abi_err(format!(
                "{}: symbol {} not found",
                path.display(),
                nsql_abi::ENTRY_SYMBOL
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
        Ok(DynSession {
            drv: Arc::clone(self),
            handle,
            description,
            limits: Box::leak(nsql_abi::bits_to_limits(limits).into_boxed_slice()),
        })
    }
}

/// 확장 드라이버 세션(프록시). 드라이버 핸들은 이 값이 drop될 때 `close`.
pub struct DynSession {
    drv: Arc<DynDriver>,
    handle: *mut c_void,
    description: String,
    limits: &'static [DriverLimit],
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
        Caps::of(self.drv.dialect)
    }

    fn driver_name(&self) -> String {
        format!("ext:{} ({})", self.drv.id, self.drv.path.display())
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

    fn fetch_cursor(&mut self, _cursor: nsql_core::CursorId) -> Result<ResultSet, DbError> {
        Err(abi_err(
            "extension driver: server cursors are not supported over the ABI (v1)",
        ))
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

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn is_alive(&self) -> bool {
        unsafe { (self.drv.vt.is_alive)(self.handle) != 0 }
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
        let e = DynDriver::load(Path::new("no-such-dir/no-such-driver.dll"))
            .expect_err("missing library");
        assert!(e.message.contains("no-such-driver"), "{e:?}");
    }
}
