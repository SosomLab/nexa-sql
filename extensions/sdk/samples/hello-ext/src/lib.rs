//! 최소 확장 — 명령 `hello.say`를 실행하면 앱 로그에 한 줄을 남긴다. 새 확장은 이 파일을 복사해 시작한다.
//!
//! 빌드: 이 작업 공간(`extensions/sdk`)에서 `cargo build --release` → `target/wasm32-unknown-unknown/release/hello_ext.wasm`.
//! 패키지: `extension.json`(kind = wasm · files[] = .wasm + sha256) + `index.json` 한 줄 → 저장소 폴더/URL.

use nexa_ext_sdk::{Command, Editor, Effect, Extension, Label, Menu, Meta, Settings};

struct Hello;

impl Extension for Hello {
    fn meta() -> Meta {
        Meta {
            id: "hello-ext".into(),
            name: "Hello Extension".into(),
            settings_prefix: "hello.".into(),
            commands: vec![Command {
                id: "hello.say".into(),
                label: Label::new("Hello: Say Hi", "Hello: 인사"),
            }],
            menus: vec![Menu {
                id: "hello".into(),
                label: Label::new("Hello", "헬로"),
                items: vec!["hello.say".into()],
            }],
        }
    }

    fn on_settings(_s: &Settings) -> Effect {
        // 이 확장은 편집기 옵션을 건드리지 않는다.
        Effect::default()
    }

    fn run(cmd: &str, _ed: &mut Editor) -> bool {
        if cmd == "hello.say" {
            nexa_ext_sdk::log("hello from wasm");
            return true;
        }
        false
    }
}

nexa_ext_sdk::export_extension!(Hello);
