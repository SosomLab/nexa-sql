//! 터미널 — 비밀번호 숨김 입력(외부 crate 0). Windows는 콘솔 모드 FFI, unix는 `stty`.

use std::io::{self, BufRead, IsTerminal, Write};

/// ★ 비밀번호가 비어 있으면 채운다(사용자 09-16 "CLI에서 암호를 물어보지 않고 접속 테스트 오류") — 우선순위:
/// ① 스펙에 이미 있음 ② 환경변수 `NSQL_PASSWORD`(docs/27 · 배치) ③ 터미널이면 숨김 프롬프트(sqlplus/psql 관례).
/// `no_prompt`이거나 터미널이 아니면 묻지 않는다(배치는 빈 비밀번호로 시도해 서버 오류를 그대로 보여 준다). SQLite는 비밀번호가 없다.
pub(crate) fn ensure_password(spec: &mut nsql_script::ConnectSpec, no_prompt: bool, label: &str) {
    if spec.password.as_deref().is_some_and(|p| !p.is_empty())
        || spec.dialect == Some(nsql_core::Dialect::Sqlite)
    {
        return;
    }
    if let Ok(p) = std::env::var("NSQL_PASSWORD") {
        if !p.is_empty() {
            spec.password = Some(p);
            return;
        }
    }
    if no_prompt || !io::stdin().is_terminal() {
        return;
    }
    if let Some(p) = read_password(&format!("Password for {label}: ")) {
        if !p.is_empty() {
            spec.password = Some(p);
        }
    }
}

/// 표준 출력 터미널의 열 수(터미널이 아니거나 알 수 없으면 `None`) — Windows 콘솔 API · unix `COLUMNS`/`stty size`.
pub(crate) fn columns() -> Option<usize> {
    if !io::stdout().is_terminal() {
        return None;
    }
    #[cfg(windows)]
    {
        win::columns()
    }
    #[cfg(not(windows))]
    {
        if let Some(c) = std::env::var("COLUMNS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
        {
            if c > 0 {
                return Some(c);
            }
        }
        let out = std::process::Command::new("stty")
            .arg("size")
            .stdin(std::process::Stdio::inherit())
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout);
        let cols: usize = s.split_whitespace().nth(1)?.parse().ok()?;
        (cols > 0).then_some(cols)
    }
}

/// stderr에 프롬프트를 찍고 stdin 한 줄을 **에코 없이** 읽는다. 터미널이 아니면 그냥 읽는다.
pub(crate) fn read_password(prompt: &str) -> Option<String> {
    eprint!("{prompt}");
    let _ = io::stderr().flush();
    let hide = io::stdin().is_terminal();
    let _guard = if hide { echo_off() } else { None };
    let mut s = String::new();
    let ok = io::stdin().lock().read_line(&mut s).is_ok();
    drop(_guard);
    if hide {
        eprintln!();
    }
    if !ok {
        return None;
    }
    Some(s.trim_end_matches(['\r', '\n']).to_string())
}

/// 에코 복원 가드.
struct EchoGuard(#[allow(dead_code)] u32);

#[cfg(windows)]
fn echo_off() -> Option<EchoGuard> {
    win::echo_off().map(EchoGuard)
}

#[cfg(windows)]
impl Drop for EchoGuard {
    fn drop(&mut self) {
        win::restore(self.0);
    }
}

#[cfg(not(windows))]
fn echo_off() -> Option<EchoGuard> {
    std::process::Command::new("stty")
        .arg("-echo")
        .stdin(std::process::Stdio::inherit())
        .status()
        .ok()
        .filter(|s| s.success())
        .map(|_| EchoGuard(0))
}

#[cfg(not(windows))]
impl Drop for EchoGuard {
    fn drop(&mut self) {
        let _ = std::process::Command::new("stty")
            .arg("echo")
            .stdin(std::process::Stdio::inherit())
            .status();
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n: u32) -> *mut c_void;
        fn GetConsoleMode(h: *mut c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(h: *mut c_void, mode: u32) -> i32;
    }
    const STD_INPUT_HANDLE: u32 = 0xFFFF_FFF6; // (DWORD)-10
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5; // (DWORD)-11
    const ENABLE_ECHO_INPUT: u32 = 0x4;

    #[repr(C)]
    struct Coord {
        x: i16,
        y: i16,
    }
    #[repr(C)]
    struct SmallRect {
        left: i16,
        top: i16,
        right: i16,
        bottom: i16,
    }
    #[repr(C)]
    struct ScreenBufferInfo {
        size: Coord,
        cursor: Coord,
        attrs: u16,
        window: SmallRect,
        max_window: Coord,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleScreenBufferInfo(h: *mut c_void, info: *mut ScreenBufferInfo) -> i32;
    }

    /// 콘솔 창의 열 수(보이는 창 폭 · 버퍼 폭이 아님).
    pub(super) fn columns() -> Option<usize> {
        // SAFETY: 표준 출력 핸들 · 출력 구조체는 지역 변수.
        unsafe {
            let h = GetStdHandle(STD_OUTPUT_HANDLE);
            if h.is_null() {
                return None;
            }
            let mut info: ScreenBufferInfo = std::mem::zeroed();
            if GetConsoleScreenBufferInfo(h, &mut info) == 0 {
                return None;
            }
            let w = i32::from(info.window.right) - i32::from(info.window.left) + 1;
            usize::try_from(w).ok().filter(|w| *w > 0)
        }
    }

    /// 에코를 끄고 이전 모드를 돌려준다(콘솔이 아니면 None).
    pub(super) fn echo_off() -> Option<u32> {
        let mut mode = 0u32;
        // SAFETY: 표준 입력 핸들 조회·모드 읽기/쓰기 — 포인터는 지역 변수.
        unsafe {
            let h = GetStdHandle(STD_INPUT_HANDLE);
            if h.is_null() || GetConsoleMode(h, &mut mode) == 0 {
                return None;
            }
            if SetConsoleMode(h, mode & !ENABLE_ECHO_INPUT) == 0 {
                return None;
            }
        }
        Some(mode)
    }

    pub(super) fn restore(mode: u32) {
        // SAFETY: 위와 동일.
        unsafe {
            let h = GetStdHandle(STD_INPUT_HANDLE);
            if !h.is_null() {
                SetConsoleMode(h, mode);
            }
        }
    }
}
