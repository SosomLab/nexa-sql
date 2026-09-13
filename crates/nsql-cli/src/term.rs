//! 터미널 — 비밀번호 숨김 입력(외부 crate 0). Windows는 콘솔 모드 FFI, unix는 `stty`.

use std::io::{self, BufRead, IsTerminal, Write};

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
    const ENABLE_ECHO_INPUT: u32 = 0x4;

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
