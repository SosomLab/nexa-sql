//! **Uppercase Selection(WASM)** — 길 A(ABI v1.3 · 154차 · docs/75 §3-5) 데모: **상태 있는 확장**(`stateful` · 호출 사이 인스턴스 유지) +
//! **편집기 읽기/쓰기**(`editor.read` · `editor.write`) + 전용 상태 저장(`storage`) + 사건 구독(`events: ["caret","edit","doc"]` · 154-b).
//!
//! - 명령 "Upper: Uppercase selection" = 선택 구간을 읽어(`Editor::regions` · `Editor::text`) 대문자로 **교체 요청**(`Editor::replace` · 큐) —
//!   적용·되돌리기 묶음은 호스트가(한 묶음) · `editor.write`는 설치 때 표시 + 첫 사용 재확인(D-90).
//! - 세션 카운터(인스턴스 메모리)와 누적 카운터(`storage` · `<root>/<id>/state.json`)를 상태줄에: "uppercased 2 (session) · 7 (total)".
//! - 캐럿 사건은 세기만 한다(10번째마다 로그 한 줄 · 디바운스 확인용).
//!
//! 빌드: `extensions/sdk`에서 `cargo build --release` → `pwsh scripts/ext-build.ps1 -Only uppercase-selection`.

use nexa_ext_sdk::{
    storage, Command, Editor, Effect, Event, Extension, Label, Menu, Meta, Settings,
};
use std::cell::Cell;

const ID: &str = "uppercase-selection";
const CMD: &str = "ext.uppercase_selection.run";

thread_local! {
    /// 세션 카운터(인스턴스가 살아 있는 동안 · `stateful`이라 호출 사이에 남는다).
    static SESSION: Cell<u32> = const { Cell::new(0) };
    static CARETS: Cell<u32> = const { Cell::new(0) };
}

struct Upper;

/// storage JSON `{"total":n}` → n(없거나 깨지면 0).
fn total_from(s: Option<String>) -> u32 {
    s.and_then(|t| {
        let t = t.trim();
        let i = t.find("\"total\"")? + 7;
        let rest = t[i..].trim_start_matches([':', ' ']);
        rest.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()
    })
    .unwrap_or(0)
}

impl Extension for Upper {
    fn meta() -> Meta {
        Meta {
            id: ID.into(),
            name: "Uppercase Selection".into(),
            settings_prefix: "ext.uppercase_selection.".into(),
            commands: vec![Command::new(
                CMD,
                Label::new("Upper: Uppercase selection", "Upper: 선택을 대문자로"),
            )],
            menus: vec![Menu::editor(
                "upper",
                Label::new("Uppercase Selection", "선택 대문자"),
                vec![CMD.into()],
            )],
            formatter: None,
            settings: vec![],
            prompts: vec![],
            settings_read: vec![],
            settings_write: vec![],
            // ★ 길 A: 인스턴스 유지 · 편집기 읽기/쓰기 · 캐럿 사건.
            stateful: true,
            capabilities: vec!["editor.read".into(), "editor.write".into(), "events".into()],
            events: vec!["caret".into(), "edit".into(), "doc".into()],
        }
    }

    fn init() {
        SESSION.with(|c| c.set(0));
        nexa_ext_sdk::log("uppercase-selection: instance ready");
    }

    fn on_settings(_s: &Settings) -> Effect {
        Effect::default()
    }

    fn run(cmd: &str, ed: &mut Editor) -> bool {
        if cmd != CMD {
            return false;
        }
        let regions = ed.regions();
        let mut changed = 0usize;
        for (a, b) in &regions {
            if b > a {
                let t = ed.text(*a, *b);
                let up = t.to_uppercase();
                if up != t && ed.replace(*a, *b, &up) {
                    changed += 1;
                }
            }
        }
        let session = SESSION.with(|c| {
            c.set(c.get() + 1);
            c.get()
        });
        let total = total_from(storage::get()) + 1;
        let _ = storage::set(&format!("{{\"total\":{total}}}"));
        ed.status(&format!(
            "uppercased {changed} region(s) · {session} (session) · {total} (total)"
        ));
        true
    }

    fn on_event(e: &Event) -> bool {
        // 편집(디바운스된 세대) · 문서(open/close/activate)는 한 줄씩 로그(154차-b V1용).
        if e.kind == "edit" {
            nexa_ext_sdk::log(&format!("edit event: doc {} v{}", e.doc, e.version));
            return true;
        }
        if e.kind == "doc" {
            nexa_ext_sdk::log(&format!("doc event: {} {} {}", e.what, e.doc, e.path));
            return true;
        }
        if e.kind == "caret" {
            let n = CARETS.with(|c| {
                c.set(c.get() + 1);
                c.get()
            });
            if n % 10 == 0 {
                nexa_ext_sdk::log(&format!("caret events: {n}"));
            }
            return true;
        }
        false
    }
}

nexa_ext_sdk::export_extension!(Upper);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_declares_stateful_and_editor_caps() {
        let m = Upper::meta();
        assert!(m.stateful);
        assert!(m.capabilities.iter().any(|c| c == "editor.write"));
        assert_eq!(m.events.len(), 3);
        assert_eq!(total_from(Some("{\"total\": 41}".into())), 41);
        assert_eq!(total_from(None), 0);
        // 호스트 밖 = 구간 없음 → 처리했다고 답하고 카운터만.
        let mut ed = Editor;
        assert!(Upper::run(CMD, &mut ed));
        assert!(!Upper::run("ext.other", &mut ed));
        assert!(Upper::on_event(&Event {
            kind: "caret".into(),
            ..Event::default()
        }));
    }
}
