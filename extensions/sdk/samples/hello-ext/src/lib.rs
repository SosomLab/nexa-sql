//! 최소 확장 — 명령 `hello.say`를 실행하면 앱 로그에 한 줄을 남긴다. 새 확장은 이 파일을 복사해 시작한다.
//!
//! 0.2.0(148차 · 사용자 10-27): **기본 설정 읽기·덮어쓰기 데모** — `settings_read`에 선언한 `ui.lang`을 `host::setting_get`으로 읽어
//! 로그에 · `settings_write`에 선언한 `editor.auto_close_pairs`를 명령으로 토글(`host::setting_set` → 호스트 **overlay 층** · 파일 저장
//! 없음 · 확장을 끄거나 지우면 자동 원복 = 본 프로그램에 영향을 남기지 않는다). 선언 밖 키는 호스트가 거절한다(로그).
//!
//! 빌드: 이 작업 공간(`extensions/sdk`)에서 `cargo build --release` → `target/wasm32-unknown-unknown/release/hello_ext.wasm`.
//! 패키지: `extension.json`(kind = wasm · files[] = .wasm + sha256) + `index.json` 한 줄 → 저장소 폴더/URL.

use nexa_ext_sdk::{Command, Editor, Effect, Extension, Label, Menu, Meta, Settings};

struct Hello;

const CMD_SAY: &str = "ext.hello_ext.say";
const CMD_TOGGLE: &str = "ext.hello_ext.toggle_autoclose";
/// 덮어쓰기 데모 대상(메타 `settings_write`에 선언).
const KEY_AUTOCLOSE: &str = "editor.auto_close_pairs";

impl Extension for Hello {
    fn meta() -> Meta {
        Meta {
            id: "hello-ext".into(),
            name: "Hello Extension".into(),
            settings_prefix: "ext.hello_ext.".into(),
            prompts: vec![],
            commands: vec![
                Command::new(CMD_SAY, Label::new("Hello: Say Hi", "Hello: 인사")),
                Command::new(
                    CMD_TOGGLE,
                    Label::new(
                        "Hello: Toggle auto-close pairs (overlay)",
                        "Hello: 자동 닫기 토글 (덮어쓰기)",
                    ),
                ),
            ],
            menus: vec![Menu::editor(
                "hello",
                Label::new("Hello", "헬로"),
                vec![CMD_SAY.into(), CMD_TOGGLE.into()],
            )],
            formatter: None,
            settings: vec![],
            // 기본 설정 접근 선언(148차): 읽기 = 화면 언어 · 덮어쓰기 = 자동 닫기 쌍(데모 · 그 밖은 호스트가 거절).
            settings_read: vec!["ui.lang".into()],
            settings_write: vec![KEY_AUTOCLOSE.into()],
        }
    }

    fn on_settings(_s: &Settings) -> Effect {
        // 이 확장은 편집기 옵션을 건드리지 않는다.
        Effect::default()
    }

    fn run(cmd: &str, _ed: &mut Editor) -> bool {
        match cmd {
            CMD_SAY => {
                let lang = nexa_ext_sdk::host::setting_get("ui.lang").unwrap_or_default();
                nexa_ext_sdk::log(&format!("hello from wasm · ui.lang={lang}"));
                true
            }
            CMD_TOGGLE => {
                let cur = nexa_ext_sdk::host::setting_get(KEY_AUTOCLOSE);
                let next = if cur.as_deref() == Some("off") {
                    "on"
                } else {
                    "off"
                };
                let ok = nexa_ext_sdk::host::setting_set(KEY_AUTOCLOSE, next);
                nexa_ext_sdk::log(&format!(
                    "{KEY_AUTOCLOSE}: {} -> {next} (overlay request {})",
                    cur.unwrap_or_default(),
                    if ok { "accepted" } else { "rejected" }
                ));
                true
            }
            _ => false,
        }
    }
}

nexa_ext_sdk::export_extension!(Hello);

#[cfg(test)]
mod tests {
    use super::*;

    /// 메타 = 명령 둘 · 메뉴 항목 = 등록된 명령 · 기본 설정 접근 선언(읽기 ui.lang · 덮어쓰기 editor.auto_close_pairs) · 호스트 밖에서는 조회가 None이라도 명령은 처리.
    #[test]
    fn meta_declares_setting_access_and_runs() {
        let m = Hello::meta();
        assert_eq!(m.commands.len(), 2);
        assert!(m.menus[0]
            .items
            .iter()
            .all(|i| m.commands.iter().any(|c| &c.id == i)));
        assert_eq!(m.settings_read, vec!["ui.lang".to_string()]);
        assert_eq!(m.settings_write, vec![KEY_AUTOCLOSE.to_string()]);
        let mut ed = Editor;
        assert!(Hello::run(CMD_SAY, &mut ed) && Hello::run(CMD_TOGGLE, &mut ed));
        assert!(!Hello::run("ext.other", &mut ed));
    }
}
