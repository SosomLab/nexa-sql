//! ★ **Linux IME 상태 감시**(사용자 09-27 "상단 바는 바로 바뀌는데 그 정보를 그대로 쓸 수 없나"): GNOME 상단 바의 한/A는 ibus 데몬이
//! 패널에 보내는 `org.freedesktop.IBus.Panel.UpdateProperty`(속성 `InputMode` · symbol `한`/`EN`)로 그려진다. ibus의 자체 버스는 매치
//! 규칙으로 그 메시지를 엿듣게 해 주므로(실측 09-27 · `dbus-monitor --address $(ibus address)`) 같은 값을 앱이 구독한다.
//!
//! 제한 운용(39 §3 부하원): **가린 칸(비밀번호)에 포커스가 있는 동안만** `dbus-monitor` 자식 하나 + 읽기 스레드 하나 · 포커스가
//! 떠나면 즉시 죽인다 · `ui.ime_hint` 끔 = 아무것도 안 띄움 · `dbus-monitor`/ibus가 없으면 조용히 폴백(엔진 이름 + 토글 키 + 입력 종류).
//! 앱이 갑자기 죽어도 자식은 파이프가 닫혀 다음 메시지에서 SIGPIPE로 끝난다. 스레드는 EOF에서 끝난다.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

static CHILD: Mutex<Option<Child>> = Mutex::new(None);
/// 설정 `ui.ime_hint_watch`(향상 모드 = off) — 끄면 `start`가 아무것도 안 한다(돌던 것은 `stop`).
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub(crate) fn set_enabled(on: bool) {
    ENABLED.store(on, std::sync::atomic::Ordering::Relaxed);
    if !on {
        stop();
    }
}

/// 감시 시작(이미 돌고 있으면 그대로) — 실패 = 조용히 무시.
pub(crate) fn start() {
    if !ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let Ok(mut g) = CHILD.lock() else { return };
    if let Some(c) = g.as_mut() {
        if matches!(c.try_wait(), Ok(None)) {
            return;
        }
        *g = None;
    }
    let Some(addr) = ibus_address() else { return };
    let mut cmd = Command::new("dbus-monitor");
    cmd.arg("--address")
        .arg(&addr)
        .arg("interface='org.freedesktop.IBus.Panel'")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let Ok(mut child) = cmd.spawn() else { return };
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
                    crate::imestate::note_input(!native);
                }
            }
        })
        .ok();
    *g = Some(child);
}

/// 감시 종료(가린 칸 포커스가 떠남 · 창 닫힘).
pub(crate) fn stop() {
    let Ok(mut g) = CHILD.lock() else { return };
    if let Some(mut c) = g.take() {
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
