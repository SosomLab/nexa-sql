//! **Nexa SQL 확장 SDK(게스트 쪽 · ABI v1)** — `wasm32-unknown-unknown`으로 빌드한 `.wasm` 하나가 3-OS에서 같은 확장이 된다.
//!
//! 앱 안의 in-process 확장(`crates/nexa-sql/src/extensions/*.rs` · `Extension` 트레이트 5메서드)과 **같은 표면**을
//! WASM export/import로 옮긴 것이다(docs/50 §7·§12 · docs/75).
//!
//! ## ABI v1 계약
//! - 버퍼 = 선두 4바이트 LE 길이 + UTF-8 본문. 게스트가 [`export_extension!`]로 내는 export:
//!   `memory` · `nx_alloc(len) -> ptr`(호스트 → 게스트 입력 버퍼) · `nx_ext_meta() -> ptr`(메타 JSON) ·
//!   `nx_ext_settings(ptr) -> ptr`(설정 JSON → 효과 JSON) · `nx_ext_disabled() -> ptr`(끈 효과 JSON) ·
//!   `nx_ext_run(ptr) -> i32`(명령 id → 1 처리 / 0 아님).
//! - 호스트 import(`env`): `nx_editor_op(op, flag) -> i32`(편집기 이동 · [`Editor`]) · `nx_log(ptr)`(로그 한 줄).
//! - 호출마다 새 인스턴스(연료·메모리·시간 상한은 호스트가 건다) — 게스트는 **상태를 두지 않는다**.
//!
//! ## 메타 JSON
//! `{"abi":1,"id":"…","name":"…","settings_prefix":"foo.","commands":[{"id":"edit.x","label":{"en":"…","ko":"…"}}],
//!   "menus":[{"id":"m","label":{"en":"…"},"items":["edit.x"]}]}`
//! ## 효과 JSON
//! `{"bracket":{"rainbow":true,"unmatched":true,"colors":["#RRGGBB",…],"max_chars":0}}` — 없는 키 = 앱 기본값.
//! ## 설정 JSON
//! `{"foo.enabled":"on","foo.max_kb":"0",…}` — 접두 `settings_prefix`의 키만 · 값은 설정 파일 원문(문자열).

pub mod json;

pub use json::Json;

/// 표시 이름(영어 필수 · 한국어 선택 — 호스트가 언어에 맞춰 고른다).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Label {
    pub en: String,
    pub ko: Option<String>,
}

impl Label {
    pub fn en(en: &str) -> Label {
        Label {
            en: en.to_string(),
            ko: None,
        }
    }
    pub fn new(en: &str, ko: &str) -> Label {
        Label {
            en: en.to_string(),
            ko: Some(ko.to_string()),
        }
    }
    fn to_json(&self) -> Json {
        let mut o = Json::obj().with("en", self.en.as_str());
        if let Some(k) = &self.ko {
            o = o.with("ko", k.as_str());
        }
        o
    }
}

/// 명령 하나(팔레트·키맵·메뉴 id는 같은 문자열 · 앱의 키맵 표에 있는 id면 그 단축키가 붙는다).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub id: String,
    pub label: Label,
}

/// 우클릭 편집 메뉴에 붙는 서브메뉴.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub id: String,
    pub label: Label,
    pub items: Vec<String>,
}

/// 확장 메타(호스트가 `nx_ext_meta`로 한 번 읽는다).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Meta {
    pub id: String,
    pub name: String,
    /// 이 확장이 읽는 설정 키 접두(예 `rainbowpair.`) — 이 접두의 키가 바뀌면 `on_settings`가 불린다.
    pub settings_prefix: String,
    pub commands: Vec<Command>,
    pub menus: Vec<Menu>,
}

impl Meta {
    pub fn to_json(&self) -> Json {
        Json::obj()
            .with("abi", 1i64)
            .with("id", self.id.as_str())
            .with("name", self.name.as_str())
            .with("settings_prefix", self.settings_prefix.as_str())
            .with(
                "commands",
                self.commands
                    .iter()
                    .map(|c| Json::obj().with("id", c.id.as_str()).with("label", c.label.to_json()))
                    .collect::<Vec<_>>(),
            )
            .with(
                "menus",
                self.menus
                    .iter()
                    .map(|m| {
                        Json::obj()
                            .with("id", m.id.as_str())
                            .with("label", m.label.to_json())
                            .with(
                                "items",
                                m.items.iter().map(|s| Json::from(s.as_str())).collect::<Vec<_>>(),
                            )
                    })
                    .collect::<Vec<_>>(),
            )
    }
}

/// 괄호 옵션 효과(앱 `BracketOpts` 중 확장이 정하는 것 — 색 층만 · 쌍 종류/문자열 안/현재 쌍/자동 닫기는 편집 코어 설정).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BracketEffect {
    pub rainbow: bool,
    pub unmatched: bool,
    /// `#RRGGBB` 목록(비면 테마 팔레트).
    pub colors: Vec<String>,
    /// 0 = 상한 없음(글자 수).
    pub max_chars: u64,
}

/// 설정 반영 결과 — 호스트가 편집기 전 탭에 적용한다.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Effect {
    pub bracket: Option<BracketEffect>,
}

impl Effect {
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        if let Some(b) = &self.bracket {
            o = o.with(
                "bracket",
                Json::obj()
                    .with("rainbow", b.rainbow)
                    .with("unmatched", b.unmatched)
                    .with(
                        "colors",
                        b.colors.iter().map(|s| Json::from(s.as_str())).collect::<Vec<_>>(),
                    )
                    .with("max_chars", b.max_chars),
            );
        }
        o
    }
}

/// 확장의 설정(접두 키만 · 값 = 설정 파일 원문).
#[derive(Clone, Debug, Default)]
pub struct Settings(pub Vec<(String, String)>);

impl Settings {
    pub fn from_json(text: &str) -> Settings {
        let mut out = Vec::new();
        if let Ok(Json::Obj(items)) = json::parse(text) {
            for (k, v) in items {
                if let Some(s) = v.as_str() {
                    out.push((k, s.to_string()));
                }
            }
        }
        Settings(out)
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    /// on/true/1/yes = 켬.
    pub fn flag(&self, key: &str) -> bool {
        matches!(
            self.get(key).map(|v| v.trim().to_ascii_lowercase()).as_deref(),
            Some("on" | "true" | "1" | "yes")
        )
    }
    pub fn int(&self, key: &str) -> i64 {
        self.get(key).and_then(|v| v.trim().parse().ok()).unwrap_or(0)
    }
}

/// 편집기 조작 표면(앱 `EditorOps`와 1:1 · 호스트 import `nx_editor_op`).
#[derive(Debug, Default)]
pub struct Editor;

/// `nx_editor_op`의 op 번호(호스트와 같은 표).
pub mod op {
    pub const GOTO_BRACKET: i32 = 1;
    pub const EXPAND_TO_BRACKETS: i32 = 2;
    pub const SIBLING_PREV: i32 = 3;
    pub const SIBLING_NEXT: i32 = 4;
    pub const PARENT: i32 = 5;
    pub const CHILD: i32 = 6;
}

impl Editor {
    pub fn goto_bracket(&mut self, shift: bool) -> bool {
        host::editor_op(op::GOTO_BRACKET, shift)
    }
    pub fn expand_to_brackets(&mut self) -> bool {
        host::editor_op(op::EXPAND_TO_BRACKETS, false)
    }
    pub fn goto_bracket_sibling(&mut self, next: bool, shift: bool) -> bool {
        host::editor_op(
            if next {
                op::SIBLING_NEXT
            } else {
                op::SIBLING_PREV
            },
            shift,
        )
    }
    pub fn goto_bracket_parent(&mut self, shift: bool) -> bool {
        host::editor_op(op::PARENT, shift)
    }
    pub fn goto_bracket_child(&mut self, shift: bool) -> bool {
        host::editor_op(op::CHILD, shift)
    }
}

/// 로그 한 줄(앱 로그 창 · 개발자 모드 `ext` 층).
pub fn log(msg: &str) {
    host::log(msg);
}

/// 확장이 구현하는 것 — 앱의 `Extension` 트레이트와 같은 다섯 가지.
pub trait Extension {
    fn meta() -> Meta;
    fn on_settings(s: &Settings) -> Effect;
    fn disabled() -> Effect {
        Effect::default()
    }
    fn run(cmd: &str, ed: &mut Editor) -> bool;
}

/// 호스트 import(실제 wasm32) / 목(호스트 테스트).
pub mod host {
    #[cfg(target_arch = "wasm32")]
    #[link(wasm_import_module = "env")]
    extern "C" {
        fn nx_editor_op(op: i32, flag: i32) -> i32;
        fn nx_log(ptr: i32);
    }

    #[cfg(target_arch = "wasm32")]
    pub fn editor_op(op: i32, flag: bool) -> bool {
        // SAFETY: 호스트가 링크한 import(없으면 인스턴스화가 실패하므로 호출에 닿지 않는다).
        unsafe { nx_editor_op(op, i32::from(flag)) != 0 }
    }
    #[cfg(target_arch = "wasm32")]
    pub fn log(msg: &str) {
        let buf = super::buf::make(msg.as_bytes());
        // SAFETY: 버퍼는 호출이 끝날 때까지 산다(아래에서 회수).
        unsafe { nx_log(buf as i32) };
        super::buf::free(buf);
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn editor_op(_op: i32, _flag: bool) -> bool {
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn log(_msg: &str) {}
}

/// 버퍼 규약(4바이트 LE 길이 + 본문) — export 매크로가 쓴다.
pub mod buf {
    /// 본문 → 버퍼(힙 · 호스트가 읽을 때까지 유지 · 인스턴스 폐기와 함께 사라진다).
    pub fn make(body: &[u8]) -> *mut u8 {
        let mut v = Vec::with_capacity(body.len() + 4);
        v.extend_from_slice(&(body.len() as u32).to_le_bytes());
        v.extend_from_slice(body);
        let p = v.as_mut_ptr();
        std::mem::forget(v);
        p
    }
    /// 호스트가 채울 입력 버퍼(`nx_alloc`).
    pub fn alloc(len: usize) -> *mut u8 {
        let mut v = vec![0u8; len + 4];
        let p = v.as_mut_ptr();
        std::mem::forget(v);
        p
    }
    /// 버퍼 읽기(길이 접두 검사).
    ///
    /// # Safety
    /// `p`는 [`make`]/[`alloc`]이 준 포인터여야 한다.
    pub unsafe fn read(p: *const u8) -> String {
        let len = u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]) as usize;
        let s = std::slice::from_raw_parts(p.add(4), len);
        String::from_utf8_lossy(s).into_owned()
    }
    /// [`make`]가 만든 버퍼 회수(호스트가 읽은 뒤 · 선택).
    pub fn free(p: *mut u8) {
        // SAFETY: make/alloc이 준 포인터 — 길이 접두로 원래 길이를 복원한다.
        unsafe {
            let len = u32::from_le_bytes([*p, *p.add(1), *p.add(2), *p.add(3)]) as usize;
            drop(Vec::from_raw_parts(p, len + 4, len + 4));
        }
    }
}

/// export 다섯을 만든다 — `export_extension!(MyExt);`
#[macro_export]
macro_rules! export_extension {
    ($t:ty) => {
        #[no_mangle]
        pub extern "C" fn nx_alloc(len: i32) -> i32 {
            $crate::buf::alloc(len.max(0) as usize) as i32
        }
        #[no_mangle]
        pub extern "C" fn nx_ext_meta() -> i32 {
            let m = <$t as $crate::Extension>::meta();
            $crate::buf::make($crate::json::dump(&m.to_json()).as_bytes()) as i32
        }
        #[no_mangle]
        pub extern "C" fn nx_ext_settings(ptr: i32) -> i32 {
            // SAFETY: 호스트가 nx_alloc으로 받은 버퍼에 설정 JSON을 채워 넘긴다.
            let text = unsafe { $crate::buf::read(ptr as *const u8) };
            let s = $crate::Settings::from_json(&text);
            let e = <$t as $crate::Extension>::on_settings(&s);
            $crate::buf::make($crate::json::dump(&e.to_json()).as_bytes()) as i32
        }
        #[no_mangle]
        pub extern "C" fn nx_ext_disabled() -> i32 {
            let e = <$t as $crate::Extension>::disabled();
            $crate::buf::make($crate::json::dump(&e.to_json()).as_bytes()) as i32
        }
        #[no_mangle]
        pub extern "C" fn nx_ext_run(ptr: i32) -> i32 {
            // SAFETY: 위와 같다.
            let cmd = unsafe { $crate::buf::read(ptr as *const u8) };
            let mut ed = $crate::Editor;
            i32::from(<$t as $crate::Extension>::run(&cmd, &mut ed))
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_and_effect_json() {
        let m = Meta {
            id: "x".into(),
            name: "X".into(),
            settings_prefix: "x.".into(),
            commands: vec![Command {
                id: "edit.a".into(),
                label: Label::new("A", "가"),
            }],
            menus: vec![Menu {
                id: "m".into(),
                label: Label::en("M"),
                items: vec!["edit.a".into()],
            }],
        };
        let j = json::dump(&m.to_json());
        assert!(j.contains(r#""abi":1"#) && j.contains(r#""ko":"가""#));
        let e = Effect {
            bracket: Some(BracketEffect {
                rainbow: true,
                unmatched: false,
                colors: vec!["#FF0000".into()],
                max_chars: 0,
            }),
        };
        assert_eq!(
            json::dump(&e.to_json()),
            r##"{"bracket":{"rainbow":true,"unmatched":false,"colors":["#FF0000"],"max_chars":0}}"##
        );
        let s = Settings::from_json(r#"{"x.on":"on","x.n":"12"}"#);
        assert!(s.flag("x.on") && s.int("x.n") == 12 && !s.flag("x.none"));
    }

    #[test]
    fn buffers_round_trip() {
        let p = buf::make("한글 ok".as_bytes());
        // SAFETY: make이 준 포인터.
        assert_eq!(unsafe { buf::read(p) }, "한글 ok");
        buf::free(p);
    }
}
