//! OS 클립보드(텍스트) — 편집기 복사/잘라내기/붙여넣기(사용자 09-14 *"copy/paste가 SQL 창에서 동작하지 않아"*).
//!
//! 컨트롤(nexa-ctl `TextBox`)은 OS를 모른다 — 선택 텍스트를 돌려주고 붙여넣을 텍스트를 받을 뿐(DR-2). 잇는 것은 호스트 몫.
//! 외부 crate 0:
//! | OS | 읽기 | 쓰기 |
//! |---|---|---|
//! | Windows | `OpenClipboard` + `GetClipboardData(CF_UNICODETEXT)` (user32/kernel32 직접) | `EmptyClipboard` + `SetClipboardData` |
//! | macOS | `pbpaste` | `pbcopy` |
//! | Linux | `wl-paste` → `xclip -o` → `xsel -o` | `wl-copy` → `xclip -i` → `xsel -i` |
//!
//! nexa-clip(`nclip-plat/clipboard.rs`)의 다중 표현·HWND 배타 오픈은 클립보드 **관리자**용이라 여기서는 텍스트 한 표현만.
//! 실패는 `None`/`false` — 호출측이 상태줄에 알린다(조용히 삼키지 않는다).

/// 클립보드 텍스트(없거나 텍스트가 아니면 `None`).
pub(crate) fn read_text() -> Option<String> {
    imp::read()
}

/// 텍스트 게시. 실패 = `false`.
pub(crate) fn write_text(text: &str) -> bool {
    imp::write(text)
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    #[link(name = "user32")]
    extern "system" {
        fn OpenClipboard(hwnd: isize) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn GetClipboardData(format: u32) -> Handle;
        fn SetClipboardData(format: u32, mem: Handle) -> Handle;
        fn IsClipboardFormatAvailable(format: u32) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
        fn GlobalLock(h: Handle) -> *mut c_void;
        fn GlobalUnlock(h: Handle) -> i32;
        fn GlobalFree(h: Handle) -> Handle;
        fn GlobalSize(h: Handle) -> usize;
    }
    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 0x0002;

    /// 다른 앱이 잠깐 열어 둔 경우를 위해 짧게 재시도(관리자 앱 관례 · 최대 ~50ms).
    fn open() -> bool {
        for _ in 0..5 {
            // SAFETY: 인자 없는 Win32 호출.
            if unsafe { OpenClipboard(0) } != 0 {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }

    pub(super) fn read() -> Option<String> {
        // SAFETY: 형식 질의는 부작용 없음.
        if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 || !open() {
            return None;
        }
        let out = (|| {
            // SAFETY: 클립보드를 열었다. 핸들은 시스템 소유 — 잠그고 읽고 풀 뿐 해제하지 않는다.
            unsafe {
                let h = GetClipboardData(CF_UNICODETEXT);
                if h.is_null() {
                    return None;
                }
                let p = GlobalLock(h) as *const u16;
                if p.is_null() {
                    return None;
                }
                let max = GlobalSize(h) / 2;
                let mut n = 0usize;
                while n < max && *p.add(n) != 0 {
                    n += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
                GlobalUnlock(h);
                Some(s)
            }
        })();
        // SAFETY: 위에서 열었다.
        unsafe {
            CloseClipboard();
        }
        out
    }

    pub(super) fn write(text: &str) -> bool {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        if !open() {
            return false;
        }
        let ok = (|| {
            // SAFETY: GMEM_MOVEABLE 블록을 채워 SetClipboardData에 넘기면 소유권이 시스템으로 간다(성공 시 해제 금지).
            unsafe {
                let h = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
                if h.is_null() {
                    return false;
                }
                let p = GlobalLock(h) as *mut u16;
                if p.is_null() {
                    GlobalFree(h);
                    return false;
                }
                std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                GlobalUnlock(h);
                if EmptyClipboard() == 0 || SetClipboardData(CF_UNICODETEXT, h).is_null() {
                    GlobalFree(h);
                    return false;
                }
                true
            }
        })();
        // SAFETY: 위에서 열었다.
        unsafe {
            CloseClipboard();
        }
        ok
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    #[cfg(target_os = "macos")]
    const READERS: &[(&str, &[&str])] = &[("pbpaste", &[])];
    #[cfg(target_os = "macos")]
    const WRITERS: &[(&str, &[&str])] = &[("pbcopy", &[])];
    #[cfg(not(target_os = "macos"))]
    const READERS: &[(&str, &[&str])] = &[
        ("wl-paste", &["--no-newline"]),
        ("xclip", &["-selection", "clipboard", "-o"]),
        ("xsel", &["--clipboard", "--output"]),
    ];
    #[cfg(not(target_os = "macos"))]
    const WRITERS: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard", "-i"]),
        ("xsel", &["--clipboard", "--input"]),
    ];

    pub(super) fn read() -> Option<String> {
        for (cmd, args) in READERS {
            if let Ok(out) = Command::new(cmd).args(*args).output() {
                if out.status.success() {
                    return Some(String::from_utf8_lossy(&out.stdout).into_owned());
                }
            }
        }
        None
    }

    pub(super) fn write(text: &str) -> bool {
        for (cmd, args) in WRITERS {
            let Ok(mut child) = Command::new(cmd)
                .args(*args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                continue;
            };
            let written = child
                .stdin
                .take()
                .map(|mut s| s.write_all(text.as_bytes()).is_ok())
                .unwrap_or(false);
            if written && child.wait().map(|s| s.success()).unwrap_or(false) {
                return true;
            }
        }
        false
    }
}
