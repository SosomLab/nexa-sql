//! X11 `CLIPBOARD` selection **직접 구현**(사용자 09-26 — *"클립보드 관련 프로그램이 없다고 앱에서 클립보드가 안 되는 게 정상이야?"*).
//!
//! 정상이 아니다. 클립보드는 프로그램이 아니라 **프로토콜**이고, 창을 띄운 앱은 이미 X 서버와 연결돼 있으므로 직접 참여하면 된다
//! (VS Code·Sublime·Firefox가 `xclip` 없이 되는 이유). 종전 Linux 경로는 `wl-copy`/`xclip`/`xsel` **외부 프로그램**을 불렀고,
//! 그 패키지가 없는 기기에서는 복사·붙여넣기 기능 자체가 사라졌다. Windows(`SetClipboardData`)·macOS(`pbcopy`)와 격이 달랐다.
//!
//! 추가 의존은 **0** — `x11rb`는 91차에 Linux 모달(`WM_TRANSIENT_FOR`)용으로 이미 들어와 있다(DR-3 예외 원장 · docs/10 §4).
//!
//! 구조:
//! - **쓰기** = 전용 스레드가 1×1 InputOnly 창으로 `SetSelectionOwner(CLIPBOARD)`를 잡고 살아 있는 동안 소유권을 유지한다.
//!   `SelectionRequest`가 오면 `TARGETS`·`UTF8_STRING`·`STRING`·`text/plain;charset=utf-8`에 답한다. 큰 값은 **INCR**로 나눠 보낸다.
//!   → 외부 도구 경로의 고질(넣어 준 `xclip` 프로세스가 죽으면 클립보드가 비는 것)도 같이 사라진다.
//! - **읽기** = 우리가 소유자면 보관 중인 값을 그대로, 아니면 `ConvertSelection` → `SelectionNotify` → property(필요하면 INCR 수신).
//! - 스레드는 **처음 복사할 때 한 번** 뜬다(붙여넣기만 하면 안 뜬다 · 부하원 원장 39 §3). 설정 `clipboard.x11_native`로 끄면 종전 CLI 경로.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, Property,
    SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

/// 한 번에 보내는 최대 바이트(이보다 크면 INCR). X11 요청 상한보다 넉넉히 작게.
const CHUNK: usize = 128 * 1024;
/// 붙여넣기 응답 대기 상한 — 상대 앱이 죽었거나 답이 없으면 여기서 포기한다(UI를 잡지 않는다).
const READ_TIMEOUT: Duration = Duration::from_millis(1200);

struct Atoms {
    clipboard: Atom,
    targets: Atom,
    utf8: Atom,
    text_plain: Atom,
    incr: Atom,
    prop: Atom,
}

fn atoms(conn: &RustConnection) -> Result<Atoms, Box<dyn std::error::Error>> {
    let a = |n: &[u8]| -> Result<Atom, Box<dyn std::error::Error>> {
        Ok(conn.intern_atom(false, n)?.reply()?.atom)
    };
    Ok(Atoms {
        clipboard: a(b"CLIPBOARD")?,
        targets: a(b"TARGETS")?,
        utf8: a(b"UTF8_STRING")?,
        text_plain: a(b"text/plain;charset=utf-8")?,
        incr: a(b"INCR")?,
        prop: a(b"NEXA_SQL_CLIP")?,
    })
}

/// 1×1 InputOnly 창(화면에 안 보인다 · selection 주고받기용 주소).
fn helper_window(
    conn: &RustConnection,
    screen: usize,
) -> Result<Window, Box<dyn std::error::Error>> {
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id()?;
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        x11rb::COPY_FROM_PARENT,
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?
    .check()?; // ★ 오류(BadMatch 등)를 여기서 받는다 — 흘려보내면 소유권도 응답도 조용히 실패한다(09-26 실측)
    Ok(win)
}

// ───────────────────────── 쓰기(소유권 유지 스레드) ─────────────────────────

static OWNER: OnceLock<Option<Sender<Vec<u8>>>> = OnceLock::new();
/// 지금 우리가 들고 있는 내용 — 우리가 소유자일 때의 읽기는 왕복 없이 여기서.
static MINE: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// 텍스트를 CLIPBOARD에 올린다(소유자가 된다). 실패 = `false` → 호출측이 CLI 경로로 폴백.
pub(crate) fn write(text: &str) -> bool {
    let tx = OWNER.get_or_init(|| spawn_owner().ok());
    let Some(tx) = tx.as_ref() else {
        if std::env::var_os("NSQL_TRACE_CLIP").is_some() {
            eprintln!("[clip] x11 write: owner thread unavailable → fallback");
        }
        return false;
    };
    *MINE.lock().unwrap_or_else(|e| e.into_inner()) = Some(text.as_bytes().to_vec());
    let sent = tx.send(text.as_bytes().to_vec()).is_ok();
    if std::env::var_os("NSQL_TRACE_CLIP").is_some() {
        eprintln!("[clip] x11 write: {} bytes queued={sent}", text.len());
    }
    sent
}

fn spawn_owner() -> Result<Sender<Vec<u8>>, Box<dyn std::error::Error>> {
    let (tx, rx) = channel::<Vec<u8>>();
    // 연결·창은 스레드 안에서 만든다(RustConnection은 Send가 아니어도 되게).
    let (ready_tx, ready_rx) = channel::<bool>();
    std::thread::Builder::new()
        .name("nsql-clipboard".into())
        .spawn(move || {
            let Ok((conn, screen)) = x11rb::connect(None) else {
                let _ = ready_tx.send(false);
                return;
            };
            let (Ok(at), Ok(win)) = (atoms(&conn), helper_window(&conn, screen)) else {
                let _ = ready_tx.send(false);
                return;
            };
            let _ = ready_tx.send(true);
            let mut data: Vec<u8> = Vec::new();
            // 나눠 보내는 중인 요청들(INCR): (요청자 창, property, 남은 자리)
            let mut incr: Vec<(Window, Atom, usize)> = Vec::new();
            loop {
                // 새 복사가 있으면 내용 교체 + 소유권(다시) 잡기.
                while let Ok(v) = rx.try_recv() {
                    data = v;
                    let owned = conn
                        .set_selection_owner(win, at.clipboard, CURRENT_TIME)
                        .ok()
                        .and_then(|c| c.check().ok())
                        .is_some()
                        && conn
                            .get_selection_owner(at.clipboard)
                            .ok()
                            .and_then(|c| c.reply().ok())
                            .is_some_and(|r| r.owner == win);
                    if !owned {
                        // 서버가 소유권을 안 줬다 — 보관분을 비워 `read()`가 "복사됐다"고 거짓말하지 않게 한다.
                        *MINE.lock().unwrap_or_else(|e| e.into_inner()) = None;
                        if std::env::var_os("NSQL_TRACE_CLIP").is_some() {
                            eprintln!(
                                "[clip] x11: set_selection_owner not honoured (win {win:#x})"
                            );
                        }
                    }
                }
                let Ok(ev) = conn.poll_for_event() else {
                    return;
                };
                match ev {
                    Some(Event::SelectionRequest(r)) => serve(&conn, &at, &data, &r, &mut incr),
                    Some(Event::SelectionClear(_)) => {
                        // 다른 앱이 가져갔다 — 보관분은 버린다(읽기는 그쪽에 물어본다).
                        *MINE.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    }
                    Some(Event::PropertyNotify(p)) => {
                        // INCR 계속: 상대가 앞 조각을 지웠다 = 다음 조각을 올릴 차례.
                        if p.state == Property::DELETE {
                            step_incr(&conn, &at, &data, p.window, p.atom, &mut incr);
                        }
                    }
                    Some(Event::Error(e)) => {
                        if std::env::var_os("NSQL_TRACE_CLIP").is_some() {
                            eprintln!("[clip] x11 owner: {e:?}");
                        }
                    }
                    Some(_) => {}
                    None => std::thread::sleep(Duration::from_millis(15)),
                }
            }
        })?;
    match ready_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(true) => Ok(tx),
        _ => Err("x11 clipboard thread not ready".into()),
    }
}

/// `SelectionRequest` 한 건에 답한다.
fn serve(
    conn: &RustConnection,
    at: &Atoms,
    data: &[u8],
    r: &SelectionRequestEvent,
    incr: &mut Vec<(Window, Atom, usize)>,
) {
    let prop = if r.property == NONE {
        r.target
    } else {
        r.property
    };
    let ok = if r.target == at.targets {
        let list = [
            at.targets,
            at.utf8,
            at.text_plain,
            u32::from(AtomEnum::STRING),
        ];
        conn.change_property32(PropMode::REPLACE, r.requestor, prop, AtomEnum::ATOM, &list)
            .is_ok()
    } else if r.target == at.utf8
        || r.target == at.text_plain
        || r.target == u32::from(AtomEnum::STRING)
    {
        if data.len() <= CHUNK {
            conn.change_property8(PropMode::REPLACE, r.requestor, prop, r.target, data)
                .is_ok()
        } else {
            // INCR 시작: 전체 크기를 알리고, 상대가 property를 지울 때마다 한 조각씩.
            let _ = conn.change_window_attributes(
                r.requestor,
                &x11rb::protocol::xproto::ChangeWindowAttributesAux::new()
                    .event_mask(EventMask::PROPERTY_CHANGE),
            );
            let total = [data.len() as u32];
            let ok = conn
                .change_property32(PropMode::REPLACE, r.requestor, prop, at.incr, &total)
                .is_ok();
            if ok {
                incr.push((r.requestor, prop, 0));
            }
            ok
        }
    } else {
        false
    };
    let ev = SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        sequence: 0,
        time: r.time,
        requestor: r.requestor,
        selection: r.selection,
        target: r.target,
        property: if ok { prop } else { NONE },
    };
    let _ = conn.send_event(false, r.requestor, EventMask::NO_EVENT, ev);
    let _ = conn.flush();
}

/// INCR 다음 조각(빈 조각 = 끝).
fn step_incr(
    conn: &RustConnection,
    at: &Atoms,
    data: &[u8],
    win: Window,
    prop: Atom,
    incr: &mut Vec<(Window, Atom, usize)>,
) {
    let Some(i) = incr.iter().position(|(w, p, _)| *w == win && *p == prop) else {
        return;
    };
    let sent = incr[i].2;
    let end = (sent + CHUNK).min(data.len());
    let chunk = &data[sent..end];
    let _ = conn.change_property8(PropMode::REPLACE, win, prop, at.utf8, chunk);
    let _ = conn.flush();
    if chunk.is_empty() {
        incr.remove(i); // 빈 조각을 보냈다 = 전송 끝
    } else {
        incr[i].2 = end;
    }
}

// ───────────────────────── 읽기 ─────────────────────────

/// CLIPBOARD 텍스트. 우리가 소유자면 보관분을 그대로 돌려준다(왕복 0).
pub(crate) fn read() -> Option<String> {
    if let Some(v) = MINE.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return String::from_utf8(v).ok();
    }
    read_from_owner().ok().flatten()
}

fn read_from_owner() -> Result<Option<String>, Box<dyn std::error::Error>> {
    let (conn, screen) = x11rb::connect(None)?;
    let at = atoms(&conn)?;
    if conn.get_selection_owner(at.clipboard)?.reply()?.owner == NONE {
        return Ok(None); // 아무도 안 들고 있다 = 빈 클립보드
    }
    let win = helper_window(&conn, screen)?;
    conn.convert_selection(win, at.clipboard, at.utf8, at.prop, CURRENT_TIME)?;
    conn.flush()?;
    let deadline = Instant::now() + READ_TIMEOUT;
    while Instant::now() < deadline {
        match conn.poll_for_event()? {
            Some(Event::SelectionNotify(n)) => {
                if n.property == NONE {
                    return Ok(None); // 상대가 UTF8_STRING을 못 준다
                }
                let r = conn
                    .get_property(true, win, at.prop, AtomEnum::ANY, 0, u32::MAX / 4)?
                    .reply()?;
                if r.type_ == at.incr {
                    return recv_incr(&conn, &at, win, deadline);
                }
                return Ok(Some(String::from_utf8_lossy(&r.value).into_owned()));
            }
            Some(Event::Error(e)) => {
                if std::env::var_os("NSQL_TRACE_CLIP").is_some() {
                    eprintln!("[clip] x11 read: {e:?}");
                }
            }
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    Ok(None)
}

/// INCR 수신: property를 지울 때마다 상대가 다음 조각을 올린다. 빈 조각 = 끝.
fn recv_incr(
    conn: &RustConnection,
    at: &Atoms,
    win: Window,
    deadline: Instant,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let mut buf: Vec<u8> = Vec::new();
    while Instant::now() < deadline {
        match conn.poll_for_event()? {
            Some(Event::PropertyNotify(p)) if p.window == win && p.state == Property::NEW_VALUE => {
                let r = conn
                    .get_property(true, win, at.prop, AtomEnum::ANY, 0, u32::MAX / 4)?
                    .reply()?;
                if r.value.is_empty() {
                    return Ok(Some(String::from_utf8_lossy(&buf).into_owned()));
                }
                buf.extend_from_slice(&r.value);
            }
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    Ok((!buf.is_empty()).then(|| String::from_utf8_lossy(&buf).into_owned()))
}

#[cfg(test)]
mod tests {
    //! 진짜 X 서버가 있어야 도는 왕복 시험(`cargo test -p nexa-sql clipboard_x11 -- --ignored`) — CI(헤드리스)에서는 건너뛴다.
    //! 상대편은 **다른 프로세스**(python3 Gtk)여야 selection 전송이 실제로 일어난다. 환경 변수:
    //!   NSQL_CLIP_EXPECT = 다른 프로세스가 미리 올려 둔 글(읽기 시험) · NSQL_CLIP_HOLD_MS = 쓰기 뒤 소유권을 유지할 시간.
    use super::*;

    #[test]
    #[ignore]
    fn read_what_another_process_put() {
        let want = std::env::var("NSQL_CLIP_EXPECT").expect("NSQL_CLIP_EXPECT");
        assert_eq!(read().as_deref(), Some(want.as_str()));
    }

    #[test]
    #[ignore]
    fn write_and_hold_for_another_process() {
        let text = std::env::var("NSQL_CLIP_TEXT")
            .unwrap_or_else(|_| "nexa-sql X11 clipboard ✓ 한글".into());
        assert!(write(&text), "write");
        assert_eq!(
            read().as_deref(),
            Some(text.as_str()),
            "우리가 소유자일 때 읽기 = 보관분"
        );
        let hold: u64 = std::env::var("NSQL_CLIP_HOLD_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3000);
        std::thread::sleep(Duration::from_millis(hold)); // 그동안 다른 프로세스가 읽는다
    }

    #[test]
    #[ignore]
    fn write_large_uses_incr() {
        // CHUNK보다 큰 값 = INCR 경로. 상대가 조각을 받아 이어 붙이는지는 다른 프로세스가 본다.
        let text: String = (0..(CHUNK * 3 / 40))
            .map(|i| format!("line {i} 한글 텍스트 줄\n"))
            .collect();
        assert!(text.len() > CHUNK);
        std::fs::write(
            std::env::var("NSQL_CLIP_OUT").expect("NSQL_CLIP_OUT"),
            &text,
        )
        .expect("write NSQL_CLIP_OUT");
        assert!(write(&text));
        let hold: u64 = std::env::var("NSQL_CLIP_HOLD_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4000);
        std::thread::sleep(Duration::from_millis(hold));
    }
}
