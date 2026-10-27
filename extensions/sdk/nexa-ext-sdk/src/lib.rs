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
//! - **포맷터(ABI v1.1 · 추가분)**: 메타에 `"formatter":{"label":{…},"sample":"…"}`를 내면 호스트가 이 확장을 SQL 포맷터로
//!   등록한다(기본 포맷터로 지정 가능 · 미리보기 탭). export `nx_ext_format(ptr) -> ptr`: 입력 JSON
//!   `{"text","dialect","options":{공통 format.* 키:값},"settings":{접두 키:값},"preview":bool}` → 출력 JSON `{"text":"…"}` 또는
//!   `{"error":"…"}`. 공통 옵션은 nsql-format `Options::from_pairs`로 그대로 읽는다(Basic 준용).
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
    /// ★ 기본 단축키(146차 · docs/75 §9 ②): Windows/Linux 코드(`ctrl+alt+p`) · mac 코드(비면 호스트가 `ctrl` → `cmd`).
    pub key: Option<String>,
    pub key_mac: Option<String>,
}

impl Command {
    pub fn new(id: &str, label: Label) -> Command {
        Command {
            id: id.into(),
            label,
            key: None,
            key_mac: None,
        }
    }
    pub fn with_key(mut self, key: &str) -> Command {
        self.key = Some(key.into());
        self
    }
}

/// 메뉴가 붙는 자리(146차 · docs/97 §5 ①): 편집기 우클릭 · 프로젝트 탐색기 파일 행 우클릭(명령은 그 행의 경로를 `doc_path`로 받는다).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MenuTarget {
    #[default]
    Editor,
    ProjectExplorer,
}

/// 우클릭 메뉴에 붙는 서브메뉴.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub id: String,
    pub label: Label,
    pub items: Vec<String>,
    pub target: MenuTarget,
}

impl Menu {
    pub fn editor(id: &str, label: Label, items: Vec<String>) -> Menu {
        Menu {
            id: id.into(),
            label,
            items,
            target: MenuTarget::Editor,
        }
    }
    pub fn project_explorer(id: &str, label: Label, items: Vec<String>) -> Menu {
        Menu {
            id: id.into(),
            label,
            items,
            target: MenuTarget::ProjectExplorer,
        }
    }
}

/// ★ 확장이 선언하는 설정 항목(146차 · T-258): 호스트가 설정 창·`Settings::get`에 등재한다. 키는 `settings_prefix`로 시작해야 한다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingDef {
    pub key: String,
    pub label: Label,
    pub desc: Label,
    pub kind: SettingKind,
    pub default: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingKind {
    Bool,
    Int {
        min: i64,
        max: i64,
    },
    Text,
    /// (값, 라벨) 목록.
    Choice(Vec<(String, Label)>),
}

impl SettingDef {
    pub fn bool(key: &str, label: Label, desc: Label, default: bool) -> SettingDef {
        SettingDef {
            key: key.into(),
            label,
            desc,
            kind: SettingKind::Bool,
            default: if default { "on" } else { "off" }.into(),
        }
    }
    pub fn to_json(&self) -> Json {
        let kind = match &self.kind {
            SettingKind::Bool => Json::from("bool"),
            SettingKind::Text => Json::from("text"),
            SettingKind::Int { min, max } => {
                Json::obj().with("int", Json::obj().with("min", *min).with("max", *max))
            }
            SettingKind::Choice(items) => Json::obj().with(
                "choice",
                items
                    .iter()
                    .map(|(v, l)| {
                        Json::obj()
                            .with("value", v.as_str())
                            .with("label", l.to_json())
                    })
                    .collect::<Vec<_>>(),
            ),
        };
        Json::obj()
            .with("key", self.key.as_str())
            .with("label", self.label.to_json())
            .with("desc", self.desc.to_json())
            .with("kind", kind)
            .with("default", self.default.as_str())
    }
}

/// 포맷터 선언(메타 `formatter`): 팔레트·설정에 보일 이름과 미리보기 예시 SQL(비면 앱의 예시).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Formatter {
    pub label: Label,
    pub sample: String,
    /// ★ 설정 → 미리보기 표식(09-30 "확장 것은 확장에"): `(설정 키, 조각)` — 호스트는 그 설정 카드에 "▶ 미리보기 n행"을
    /// 붙인다(조각 = 미리보기 결과에서 공백·탭을 지우고 대문자로 바꾼 뒤 처음 포함되는 줄). 비면 표식 없음.
    pub marks: Vec<(String, String)>,
}

/// 확장 메타(호스트가 `nx_ext_meta`로 한 번 읽는다).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Meta {
    pub id: String,
    pub name: String,
    /// 이 확장이 읽는 설정 키 접두(예 `ext.rainbow_pairs.`) — 이 접두의 키가 바뀌면 `on_settings`가 불린다.
    pub settings_prefix: String,
    pub commands: Vec<Command>,
    pub menus: Vec<Menu>,
    /// 이 확장이 SQL 포맷터를 제공한다(ABI v1.1 · `nx_ext_format`).
    pub formatter: Option<Formatter>,
    /// ★ 설정 항목 선언(146차 · T-258) — 호스트 설정 창에 분류(확장 이름)로 나타난다.
    pub settings: Vec<SettingDef>,
}

impl Meta {
    pub fn to_json(&self) -> Json {
        let mut o = Json::obj();
        if let Some(f) = &self.formatter {
            let mut fo = Json::obj()
                .with("label", f.label.to_json())
                .with("sample", f.sample.as_str());
            if !f.marks.is_empty() {
                fo = fo.with(
                    "marks",
                    f.marks
                        .iter()
                        .map(|(k, m)| Json::obj().with("key", k.as_str()).with("mark", m.as_str()))
                        .collect::<Vec<_>>(),
                );
            }
            o = o.with("formatter", fo);
        }
        o.with("abi", 1i64)
            .with("id", self.id.as_str())
            .with("name", self.name.as_str())
            .with("settings_prefix", self.settings_prefix.as_str())
            .with(
                "commands",
                self.commands
                    .iter()
                    .map(|c| {
                        let mut o = Json::obj()
                            .with("id", c.id.as_str())
                            .with("label", c.label.to_json());
                        if let Some(k) = &c.key {
                            o = o.with("key", k.as_str());
                        }
                        if let Some(k) = &c.key_mac {
                            o = o.with("key_mac", k.as_str());
                        }
                        o
                    })
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
                                "target",
                                match m.target {
                                    MenuTarget::Editor => "editor",
                                    MenuTarget::ProjectExplorer => "project_explorer",
                                },
                            )
                            .with(
                                "items",
                                m.items
                                    .iter()
                                    .map(|s| Json::from(s.as_str()))
                                    .collect::<Vec<_>>(),
                            )
                    })
                    .collect::<Vec<_>>(),
            )
            .with(
                "settings",
                self.settings
                    .iter()
                    .map(SettingDef::to_json)
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
                        b.colors
                            .iter()
                            .map(|s| Json::from(s.as_str()))
                            .collect::<Vec<_>>(),
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
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
    /// on/true/1/yes = 켬.
    pub fn flag(&self, key: &str) -> bool {
        matches!(
            self.get(key)
                .map(|v| v.trim().to_ascii_lowercase())
                .as_deref(),
            Some("on" | "true" | "1" | "yes")
        )
    }
    pub fn int(&self, key: &str) -> i64 {
        self.get(key)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
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

    /// ★ 활성 문서의 파일 경로(09-30 · docs/97) — 파일이 아닌 스크립트 탭이면 `None`. 프로젝트 탐색기 메뉴에서 불렸으면 그 행의 경로.
    pub fn doc_path(&self) -> Option<String> {
        host::get(query::DOC_PATH).filter(|p| !p.is_empty())
    }
}

/// `nx_file_op`의 종류 번호(호스트와 같은 표 · 146차 · docs/97 §5 ②).
pub mod file_op {
    pub const OPEN: i32 = 1;
    pub const REVEAL: i32 = 2;
    pub const COPY_PATH: i32 = 3;
}

/// ★ 파일 동작(146차): 명령이 끝난 뒤 호스트가 적용한다 — 열기(새 탭) · 프로젝트 탐색기에서 드러내기 · 경로를 클립보드에(사용자 클립보드를
/// 덮으므로 **명시적 명령**에서만 부를 것).
#[derive(Debug, Default)]
pub struct Files;

impl Files {
    pub fn open(&self, path: &str) -> bool {
        host::file_op(file_op::OPEN, path)
    }
    pub fn reveal(&self, path: &str) -> bool {
        host::file_op(file_op::REVEAL, path)
    }
    pub fn copy_path(&self, path: &str) -> bool {
        host::file_op(file_op::COPY_PATH, path)
    }
}

/// 로그 한 줄(앱 로그 창 · 개발자 모드 `ext` 층).
pub fn log(msg: &str) {
    host::log(msg);
}

/// 포맷 요청(`nx_ext_format` 입력 · ABI v1.1).
#[derive(Clone, Debug, Default)]
pub struct FormatRequest {
    /// 포맷할 SQL(선택 또는 문서 전체 · 미리보기면 예시).
    pub text: String,
    /// 방언 힌트(`oracle` · `mssql` · `postgres` · `sqlite` · 빈 값 = 모름).
    pub dialect: String,
    /// ★ 공통 옵션(앱 설정 `format.*` · 키는 접두 없이 · 값은 원문) — nsql-format `Options::from_pairs`에 그대로.
    pub options: Settings,
    /// 이 확장의 설정(접두 키만).
    pub settings: Settings,
    /// 미리보기 호출인가(설정 창·미리보기 탭).
    pub preview: bool,
}

impl FormatRequest {
    pub fn from_json(text: &str) -> FormatRequest {
        let mut r = FormatRequest::default();
        let Ok(Json::Obj(items)) = json::parse(text) else {
            return r;
        };
        for (k, v) in items {
            match k.as_str() {
                "text" => r.text = v.as_str().unwrap_or("").to_string(),
                "dialect" => r.dialect = v.as_str().unwrap_or("").to_string(),
                "preview" => r.preview = matches!(v, Json::Bool(true)),
                "options" | "settings" => {
                    let mut out = Vec::new();
                    if let Json::Obj(inner) = v {
                        for (ik, iv) in inner {
                            if let Some(sv) = iv.as_str() {
                                out.push((ik, sv.to_string()));
                            }
                        }
                    }
                    if k == "options" {
                        r.options = Settings(out);
                    } else {
                        r.settings = Settings(out);
                    }
                }
                _ => {}
            }
        }
        r
    }
}

/// 포맷 응답 JSON.
pub fn format_response(r: Result<String, String>) -> String {
    let o = match r {
        Ok(t) => Json::obj().with("text", t.as_str()),
        Err(e) => Json::obj().with("error", e.as_str()),
    };
    json::dump(&o)
}

/// 확장이 구현하는 것 — 앱의 `Extension` 트레이트와 같은 다섯 가지 + 포맷터(선택).
pub trait Extension {
    fn meta() -> Meta;
    fn on_settings(s: &Settings) -> Effect;
    fn disabled() -> Effect {
        Effect::default()
    }
    fn run(cmd: &str, ed: &mut Editor) -> bool;
    /// SQL 포맷(메타에 `formatter`를 냈을 때만 불린다). 기본 = 없음.
    fn format(_req: &FormatRequest) -> Result<String, String> {
        Err("no formatter".to_string())
    }
}

/// 호스트 import(실제 wasm32) / 목(호스트 테스트).
pub mod host {
    #[cfg(target_arch = "wasm32")]
    #[link(wasm_import_module = "env")]
    extern "C" {
        fn nx_editor_op(op: i32, flag: i32) -> i32;
        fn nx_log(ptr: i32);
        /// ★ 호스트 상황 조회(09-30 · docs/97 §2): `kind`(`query::*`) 값을 `ptr` 버퍼(길이 접두 · `cap` 본문 상한)에 써 준다 ·
        /// 반환 = 쓴 길이(모르는 종류·안 맞음 = -1).
        fn nx_host_get(kind: i32, ptr: i32, cap: i32) -> i32;
        /// ★ 파일 동작 요청(146차): `kind`(`file_op::*`) · `ptr` = 경로 버퍼(길이 접두) · 반환 1 = 접수.
        fn nx_file_op(kind: i32, ptr: i32) -> i32;
    }

    #[cfg(target_arch = "wasm32")]
    pub fn file_op(kind: i32, path: &str) -> bool {
        let buf = super::buf::make(path.as_bytes());
        // SAFETY: 호스트가 링크한 import · 버퍼는 호출이 끝날 때까지 산다(아래에서 회수).
        let r = unsafe { nx_file_op(kind, buf as i32) };
        super::buf::free(buf);
        r != 0
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn file_op(_kind: i32, _path: &str) -> bool {
        false
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

    /// 호스트 상황 값 하나(문자열) — 없으면 None.
    #[cfg(target_arch = "wasm32")]
    pub fn get(kind: i32) -> Option<String> {
        const CAP: usize = 4096;
        let p = super::buf::alloc(CAP);
        // SAFETY: 호스트가 링크한 import · 버퍼는 우리가 방금 마련했고 아래에서 회수한다.
        let n = unsafe { nx_host_get(kind, p as i32, CAP as i32) };
        let out = if n >= 0 {
            // SAFETY: 호스트가 길이 접두 + 본문을 썼다(n ≤ CAP).
            Some(unsafe { super::buf::read(p) })
        } else {
            None
        };
        // ★ 146차 결함 수정(10-27): `free(p)`는 길이 접두로 용량을 복원하는데 이 버퍼의 접두는 호스트가 쓴 값(≤ CAP · 못 썼으면 0)이라
        //   실제 할당(CAP + 4)과 달라 해제 때 트랩(`unreachable`)했다 → 마련한 용량 그대로 돌려준다(`free_cap`).
        super::buf::free_cap(p, CAP + 4);
        out
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn editor_op(_op: i32, _flag: bool) -> bool {
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn log(_msg: &str) {}
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get(_kind: i32) -> Option<String> {
        None
    }
}

/// `nx_host_get`의 종류 번호(호스트와 같은 표 · docs/97 §2).
pub mod query {
    /// 활성 문서의 파일 경로(저장된 파일이 아니면 빈 글).
    pub const DOC_PATH: i32 = 1;
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
    /// [`alloc`]이 만든 버퍼 회수 — 길이 접두가 아니라 **마련한 용량**(`len + 4`)으로(146차 · 접두는 호스트가 바꿔 쓴다).
    pub fn free_cap(p: *mut u8, cap: usize) {
        // SAFETY: alloc(cap - 4)이 준 포인터 · 그 용량 그대로.
        unsafe {
            drop(Vec::from_raw_parts(p, cap, cap));
        }
    }
    /// [`make`]가 만든 버퍼 회수(호스트가 읽은 뒤 · 선택) — 접두 = 본문 길이 = 용량 − 4이므로 안전. `alloc` 버퍼에는 쓰지 말 것(`free_cap`).
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
        #[no_mangle]
        pub extern "C" fn nx_ext_format(ptr: i32) -> i32 {
            // SAFETY: 위와 같다.
            let text = unsafe { $crate::buf::read(ptr as *const u8) };
            let req = $crate::FormatRequest::from_json(&text);
            let out = $crate::format_response(<$t as $crate::Extension>::format(&req));
            $crate::buf::make(out.as_bytes()) as i32
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
            commands: vec![Command::new("edit.a", Label::new("A", "가")).with_key("ctrl+alt+a")],
            menus: vec![Menu::project_explorer(
                "m",
                Label::en("M"),
                vec!["edit.a".into()],
            )],
            formatter: None,
            settings: vec![SettingDef::bool(
                "x.flag",
                Label::en("Flag"),
                Label::en("A flag"),
                true,
            )],
        };
        let j = json::dump(&m.to_json());
        assert!(j.contains(r#""abi":1"#) && j.contains(r#""ko":"가""#));
        // 146차: 기본 키 · 메뉴 자리 · 설정 선언.
        assert!(j.contains(r#""key":"ctrl+alt+a""#), "{j}");
        assert!(j.contains(r#""target":"project_explorer""#), "{j}");
        assert!(
            j.contains(r#""settings":[{"key":"x.flag","label":{"en":"Flag"},"desc":{"en":"A flag"},"kind":"bool","default":"on"}]"#),
            "{j}"
        );
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
    fn format_request_and_response_json() {
        let r = FormatRequest::from_json(
            r#"{"text":"select 1","dialect":"oracle","options":{"format.comma":"leading"},"settings":{"ext.sqlfmt_kiros33.strict":"on"},"preview":true}"#,
        );
        assert_eq!(r.text, "select 1");
        assert_eq!(r.dialect, "oracle");
        assert_eq!(r.options.get("format.comma"), Some("leading"));
        assert!(r.settings.flag("ext.sqlfmt_kiros33.strict") && r.preview);
        assert_eq!(format_response(Ok("X".into())), r#"{"text":"X"}"#);
        assert_eq!(format_response(Err("bad".into())), r#"{"error":"bad"}"#);
        let m = Meta {
            id: "f".into(),
            formatter: Some(Formatter {
                label: Label::en("F"),
                sample: "select 1".into(),
                marks: Vec::new(),
            }),
            ..Meta::default()
        };
        let j = json::dump(&m.to_json());
        assert!(
            j.contains(r#""formatter":{"label":{"en":"F"},"sample":"select 1"}"#),
            "{j}"
        );
    }

    /// 포맷터 미리보기 표식(09-30): 있을 때만 `marks` 배열.
    #[test]
    fn formatter_marks_json() {
        let m = Meta {
            formatter: Some(Formatter {
                label: Label::en("F"),
                sample: String::new(),
                marks: vec![("ext.x.a".into(), "SELECT".into())],
            }),
            ..Meta::default()
        };
        let j = json::dump(&m.to_json());
        assert!(
            j.contains(r#""marks":[{"key":"ext.x.a","mark":"SELECT"}]"#),
            "{j}"
        );
    }

    #[test]
    fn buffers_round_trip() {
        let p = buf::make("한글 ok".as_bytes());
        // SAFETY: make이 준 포인터.
        assert_eq!(unsafe { buf::read(p) }, "한글 ok");
        buf::free(p);
    }
}
