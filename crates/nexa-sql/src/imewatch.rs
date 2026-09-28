//! ★ **Linux IME 상태 감시**(사용자 09-27 "상단 바는 바로 바뀌는데 그 정보를 그대로 쓸 수 없나"): GNOME 상단 바의 한/A는 ibus 데몬이
//! 패널에 보내는 `org.freedesktop.IBus.Panel.UpdateProperty`(속성 `InputMode` · symbol `한`/`EN`)로 그려진다. ibus의 자체 버스는 매치
//! 규칙으로 그 메시지를 엿듣게 해 주므로(실측 09-27 · `dbus-monitor --address $(ibus address)`) 같은 값을 앱이 구독한다.
//!
//! 제한 운용(39 §3 부하원): **가린 칸(비밀번호)이 있는 창(로그인 · 입력 창)이 열려 있는 동안만** `dbus-monitor` 자식 하나 + 읽기
//! 스레드 하나 · 창이 닫히면 즉시 죽인다(처음엔 칸 포커스 동안만이었으나 — 시작 순간엔 값이 없고(ibus는 포커스 변화 때만 보낸다)
//! 다른 창에서 바꾸는 동안 꺼져 있으면 돌아올 때 틀렸다 · 09-27) · `input.ime_hint`/`input.ime_hint_watch` 끔 = 안 띄움 ·
//! `dbus-monitor`/ibus가 없으면 조용히 폴백(엔진 이름 + 토글 키 + 입력 종류).
//! 실측(09-27 `scratchpad/ibus_focus_test.py`): ibus-hangul의 한/영 모드는 **전역**(다른 컨텍스트에서 바꾼 값이 돌아온 컨텍스트에도) ·
//! 컨텍스트 FocusIn마다 `RegisterProperties`로 지금 모드를 다시 보낸다 → 창보다 먼저 듣기 시작하면 초기값·재포커스 값이 생긴다.
//! 앱이 갑자기 죽어도 자식은 파이프가 닫혀 다음 메시지에서 SIGPIPE로 끝난다. 스레드는 EOF에서 끝난다.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

static CHILD: Mutex<Option<Child>> = Mutex::new(None);
/// 감시를 띄운 안내(소유자 토큰) — 다른 창의 tick(가린 칸 포커스 없음)이 남의 감시를 죽이지 않게(09-27 실기: stop/start 반복으로 메시지 유실).
static OWNER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn trace(msg: &str) {
    if crate::input::trace_ime() {
        eprintln!("[ime] watch {msg}");
    }
}
/// 설정 `input.ime_hint_watch`(향상 모드 = off) — 끄면 `start`가 아무것도 안 한다(돌던 것은 `stop`).
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub(crate) fn set_enabled(on: bool) {
    ENABLED.store(on, std::sync::atomic::Ordering::Relaxed);
    if !on {
        stop(0);
    }
}

/// 감시 시작(이미 돌고 있으면 그대로 · 소유자만 갱신) — 실패 = 조용히 무시. `owner` = 띄우는 안내의 토큰.
pub(crate) fn start(owner: usize) {
    OWNER.store(owner, std::sync::atomic::Ordering::Relaxed);
    if !ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Ok(mut g) = CHILD.lock() else { return };
    if let Some(c) = g.as_mut() {
        if matches!(c.try_wait(), Ok(None)) {
            return;
        }
        trace("child exited on its own — restarting");
        *g = None;
    }
    let Some(addr) = ibus_address() else {
        trace("no ibus address");
        return;
    };
    let mut cmd = Command::new("dbus-monitor");
    cmd.arg("--address")
        .arg(&addr)
        .arg("interface='org.freedesktop.IBus.Panel'")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            trace(&format!("spawn failed: {e}"));
            return;
        }
    };
    let mut child = child;
    trace(&format!("started pid={}", child.id()));
    crate::imestate::set_watch_active(true);
    let Some(out) = child.stdout.take() else {
        let _ = child.kill();
        return;
    };
    std::thread::Builder::new()
        .name("ime-watch".into())
        .spawn(move || {
            let mut p = Parser::default();
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Some(native) = p.feed(&line) {
                    trace(if native {
                        "symbol=한 → native"
                    } else {
                        "symbol=EN → latin"
                    });
                    crate::imestate::set_from_watch(native);
                }
            }
            // EOF = 자식이 끝났다(stop · 죽음) → 폴백으로.
            crate::imestate::set_watch_active(false);
            trace("reader ended");
        })
        .ok();
    *g = Some(child);
}

/// 감시가 살아 있는가(있으면 한/영 상태의 원천은 감시뿐 — 토글 키로 뒤집지 않는다 · 사용자 09-27 "Shift+Space가 들쭉날쭉").
pub(crate) fn running() -> bool {
    crate::imestate::watch_active()
}

/// 감시 종료(가린 칸 포커스가 떠남 · 창 닫힘) — **소유자**의 요청만 듣는다(`owner` 0 = 강제).
pub(crate) fn stop(owner: usize) {
    if owner != 0 && OWNER.load(std::sync::atomic::Ordering::Relaxed) != owner {
        return;
    }
    let Ok(mut g) = CHILD.lock() else { return };
    if let Some(mut c) = g.take() {
        trace("stop");
        crate::imestate::set_watch_active(false);
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// `ibus address`(한 번 읽어 둔다 · ibus가 없으면 None).
fn ibus_address() -> Option<String> {
    static ADDR: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    ADDR.get_or_init(|| {
        let out = Command::new("ibus")
            .arg("address")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && s.starts_with("unix:")).then_some(s)
    })
    .clone()
}

/// `dbus-monitor` 텍스트 파서 — `member=UpdateProperty|RegisterProperties` 메시지 안의 `InputMode` 속성: `string "InputMode"` 뒤
/// 세 번째 `IBusText`(label · tooltip · **symbol**) 다음 줄의 문자열이 symbol(`한` = 한글 · `EN` = 영문). 돌려주는 값 = 본연(한글)인가.
#[derive(Default)]
struct Parser {
    in_prop: bool,
    texts: u8,
    want_symbol: bool,
}

impl Parser {
    fn feed(&mut self, line: &str) -> Option<bool> {
        let t = line.trim();
        if t.starts_with("method call") || t.starts_with("signal") {
            *self = Parser::default();
            return None;
        }
        let s = t
            .strip_prefix("string \"")
            .and_then(|r| r.strip_suffix('"'))?;
        if s == "InputMode" {
            self.in_prop = true;
            self.texts = 0;
            self.want_symbol = false;
            return None;
        }
        if !self.in_prop {
            return None;
        }
        if s == "IBusProperty" {
            // 다음 속성으로 넘어감 — InputMode 블록 끝.
            self.in_prop = false;
            return None;
        }
        if self.want_symbol {
            self.want_symbol = false;
            self.in_prop = false;
            let native = s.chars().any(|c| {
                ('\u{AC00}'..='\u{D7A3}').contains(&c) || ('\u{3131}'..='\u{318E}').contains(&c)
            });
            return Some(native);
        }
        if s == "IBusText" {
            self.texts += 1;
            if self.texts == 3 {
                self.want_symbol = true;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"method call time=1 sender=org.freedesktop.DBus -> destination=(null destination) serial=1 path=/org/freedesktop/IBus/Panel; interface=org.freedesktop.IBus.Panel; member=UpdateProperty
   variant       struct {
         string "IBusProperty"
         array [
         ]
         string "InputMode"
         uint32 1
         variant             struct {
               string "IBusText"
               array [
               ]
               string "한글 상태"
               variant                   struct {
                     string "IBusAttrList"
                  }
            }
         string ""
         variant             struct {
               string "IBusText"
               array [
               ]
               string "한글 입력 모드를 선택합니다"
            }
         boolean true
         boolean true
         uint32 0
         variant             struct {
               string "IBusPropList"
               array [
               ]
            }
         variant             struct {
               string "IBusText"
               array [
               ]
               string "한"
            }
      }
"#;

    #[test]
    fn parses_input_mode_symbol() {
        let mut p = Parser::default();
        let mut got = None;
        for l in SAMPLE.lines() {
            if let Some(v) = p.feed(l) {
                got = Some(v);
            }
        }
        assert_eq!(got, Some(true), "symbol 한 = 본연");
        let mut p = Parser::default();
        let mut got = None;
        for l in SAMPLE.replace("string \"한\"", "string \"EN\"").lines() {
            if let Some(v) = p.feed(l) {
                got = Some(v);
            }
        }
        assert_eq!(got, Some(false), "symbol EN = 라틴");
        // InputMode가 없는 속성(hanja_mode)은 무시.
        let mut p = Parser::default();
        let none: Vec<bool> = SAMPLE
            .replace("\"InputMode\"", "\"hanja_mode\"")
            .lines()
            .filter_map(|l| p.feed(l))
            .collect();
        assert!(none.is_empty());
    }
}
