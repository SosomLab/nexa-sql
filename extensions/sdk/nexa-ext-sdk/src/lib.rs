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
//! - **프롬프트 변환(ABI v1.2 · 148차 · docs/124 §7)**: 메타에 `"prompts":[{"id","label","placeholder","placeholder_key","hint","key",
//!   "help":[{"expr","label"}]}]`를 내면 호스트가 그 id를 명령으로 등록하고(팔레트·키맵·메뉴) 고르면 **라이브 프롬프트 세션**
//!   (다중 캐럿 구간 읽기 · 값 교체 · Enter 확정 · Esc 복원 · 되돌리기 한 단계)을 **호스트가** 연다. 게스트는 순수 함수 export
//!   `nx_ext_transform(ptr) -> ptr`만 낸다: 입력 JSON `{"id","text":표현식,"n":구간 수,"regions":[{"line","text"}…(상한)],
//!   "lang","doc_path","settings":{접두 키:값}}` → 출력 JSON `{"values":[…n개]}` 또는 `{"error":"…"}`(+ 선택 `"rows":[{"text","warn"}]`
//!   = 미리보기 행 덮어쓰기). 게스트에 본문 쓰기 권한은 없다(값만 제안 · 적용·복원은 호스트). 호출마다 새 인스턴스 그대로.
//!   요청의 `phase`(`live`·`commit`·`cancel`)·`selection`, 응답의 `apply`(`regions`·`text`·`none`)·`text`는 다른 플러그인이 재사용하는 축.
//! - **기본 설정 읽기·덮어쓰기(ABI v1.2 · 148차)**: 메타 `settings_read:["editor.*",…]`에 선언한 기본 키는 전용 키와 함께 설정 JSON에
//!   실리고 바뀌면 `on_settings`가 다시 불린다 · import `nx_setting_get(key, ptr, cap)`(그 키들만) · `nx_setting_set(key, val)`은 메타
//!   `settings_write`에 선언한 키만 호스트가 검증해 **런타임 overlay 층**에 넣는다(파일 저장 없음 · 확장이 꺼지면 자동 원복 = 본 프로그램에
//!   영향을 남기지 않는다).
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
    pub fn text(key: &str, label: Label, desc: Label, default: &str) -> SettingDef {
        SettingDef {
            key: key.into(),
            label,
            desc,
            kind: SettingKind::Text,
            default: default.into(),
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

/// ★ 프롬프트 선언(ABI v1.2 · 148차 · docs/124 §7): 호스트가 `id`를 명령으로 등록하고, 고르면 라이브 프롬프트 세션을 연다 —
/// 키 입력마다 [`Extension::transform`]이 불리고 그 `values`가 다중 캐럿 자리에 들어간다(적용·복원·되돌리기는 호스트).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Prompt {
    /// 명령 id(팔레트·키맵·메뉴 · `commands`에 없으면 호스트가 같은 라벨·키로 등록한다).
    pub id: String,
    pub label: Label,
    /// 자리표시자 = 빈 입력의 뜻(예 `1+1`).
    pub placeholder: String,
    /// 설정 키 — 값이 비어 있지 않으면 그 값이 자리표시자를 대신한다(예 `ext.insert_sequence.default`).
    pub placeholder_key: Option<String>,
    /// 입력란 아래 안내(`{0}` = 자리표시자).
    pub hint: Label,
    /// 기본 단축키(Windows/Linux 코드 · 비면 없음).
    pub key: Option<String>,
    /// `?` 치트 시트 = (표현식, 설명).
    pub help: Vec<(String, Label)>,
}

impl Prompt {
    pub fn new(id: &str, label: Label, placeholder: &str, hint: Label) -> Prompt {
        Prompt {
            id: id.into(),
            label,
            placeholder: placeholder.into(),
            placeholder_key: None,
            hint,
            key: None,
            help: Vec::new(),
        }
    }
    pub fn with_key(mut self, key: &str) -> Prompt {
        self.key = Some(key.into());
        self
    }
    pub fn with_placeholder_key(mut self, key: &str) -> Prompt {
        self.placeholder_key = Some(key.into());
        self
    }
    pub fn with_help(mut self, help: Vec<(&str, Label)>) -> Prompt {
        self.help = help.into_iter().map(|(e, l)| (e.to_string(), l)).collect();
        self
    }
    fn to_json(&self) -> Json {
        let mut o = Json::obj()
            .with("id", self.id.as_str())
            .with("label", self.label.to_json())
            .with("placeholder", self.placeholder.as_str())
            .with("hint", self.hint.to_json());
        if let Some(k) = &self.placeholder_key {
            o = o.with("placeholder_key", k.as_str());
        }
        if let Some(k) = &self.key {
            o = o.with("key", k.as_str());
        }
        o.with(
            "help",
            self.help
                .iter()
                .map(|(e, l)| {
                    Json::obj()
                        .with("expr", e.as_str())
                        .with("label", l.to_json())
                })
                .collect::<Vec<_>>(),
        )
    }
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
    /// ★ 프롬프트 선언(ABI v1.2 · 148차) — 라이브 프롬프트 + `transform`.
    pub prompts: Vec<Prompt>,
    /// ★ 읽는 기본 설정 키 패턴(`editor.*` · `ui.lang` · 148차): 설정 JSON에 함께 실리고 바뀌면 `on_settings` 재호출 · `host::setting_get` 허용 범위.
    pub settings_read: Vec<String>,
    /// ★ 덮어쓸 수 있다고 선언한 기본 설정 키 패턴 — `host::setting_set`은 이 안에서만(런타임 overlay · 확장이 꺼지면 원복).
    pub settings_write: Vec<String>,
    /// ★ 길 A(ABI v1.3 · 154차): 호출 사이에 인스턴스를 유지한다(`init`이 (재)인스턴스화 때 1회 · 상태는 `storage`로 보존 권장).
    pub stateful: bool,
    /// 능력 선언 — `editor.read`(본문·구간 읽기) · `editor.write`(교체·구간 요청 · 설치 때 표시 + 첫 사용 재확인) · `events`.
    pub capabilities: Vec<String>,
    /// 구독 사건(`on_event`) — `edit` · `caret` · `doc`.
    pub events: Vec<String>,
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
            .with(
                "prompts",
                self.prompts.iter().map(Prompt::to_json).collect::<Vec<_>>(),
            )
            .with(
                "settings_read",
                self.settings_read
                    .iter()
                    .map(|k| Json::from(k.as_str()))
                    .collect::<Vec<_>>(),
            )
            .with(
                "settings_write",
                self.settings_write
                    .iter()
                    .map(|k| Json::from(k.as_str()))
                    .collect::<Vec<_>>(),
            )
            .with("stateful", self.stateful)
            .with(
                "capabilities",
                self.capabilities
                    .iter()
                    .map(|k| Json::from(k.as_str()))
                    .collect::<Vec<_>>(),
            )
            .with(
                "events",
                self.events
                    .iter()
                    .map(|k| Json::from(k.as_str()))
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
    /// ★ 길 A(ABI v1.3): 본문 글자 수(`editor.read` 선언 없으면 0).
    pub fn len(&self) -> usize {
        host::editor_len().max(0) as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// 구간 글(글자 인덱스 · 1 MB 상한 · 선언 없으면 빈 글).
    pub fn text(&self, from: usize, to: usize) -> String {
        host::editor_text(from, to).unwrap_or_default()
    }
    /// 구간 목록(캐럿 = `(a, a)`).
    pub fn regions(&self) -> Vec<(usize, usize)> {
        host::editor_regions()
    }
    /// 교체 **요청**(큐 · 좌표 = 호출 시작 때 본문 · 겹치면 전부 거절 · 호출이 끝난 뒤 앱이 한 되돌리기 묶음으로 적용) · `editor.write`.
    pub fn replace(&mut self, from: usize, to: usize, text: &str) -> bool {
        host::editor_replace(from, to, text)
    }
    /// 구간/캐럿 설정 요청(교체 적용 뒤 좌표).
    pub fn set_regions(&mut self, regions: &[(usize, usize)]) -> bool {
        host::editor_set_regions(regions)
    }
    /// 상태줄 글(호출당 마지막 것).
    pub fn status(&self, text: &str) {
        host::ui_status(text)
    }
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

/// ★ 프롬프트 변환 요청(`nx_ext_transform` 입력 · ABI v1.2 · 148차).
#[derive(Clone, Debug, Default)]
pub struct TransformRequest {
    /// 프롬프트 id(메타 `prompts[].id`).
    pub id: String,
    /// 지금 입력란의 글(빈 입력은 호스트가 자리표시자로 바꿔 준다).
    pub text: String,
    /// 캐럿/구간 수 = 돌려줄 값의 수.
    pub n: usize,
    /// 구간(줄 번호 1부터, 원문) — 호스트 상한까지만(길면 앞쪽만 · `n`이 진짜 수).
    pub regions: Vec<(usize, String)>,
    /// UI 언어 코드(`en` · `ko`).
    pub lang: String,
    /// 활성 문서 경로(저장 안 된 탭 = 빈 글).
    pub doc_path: String,
    /// 이 확장의 설정(접두 키 + 메타 `settings_read` 키).
    pub settings: Settings,
    /// 단계 = `live`(키 입력마다) · `commit`(Enter) · `cancel`(Esc).
    pub phase: String,
    /// 활성 선택의 글(구간이 여럿이면 첫 구간 시작 ~ 마지막 구간 끝).
    pub selection: String,
    /// ★ 확정 때 사용자가 고른 행의 id(`with_choices`로 낸 행 · T-447 · 없으면 None).
    pub pick: Option<String>,
}

impl TransformRequest {
    pub fn from_json(text: &str) -> TransformRequest {
        let mut r = TransformRequest::default();
        let Ok(Json::Obj(items)) = json::parse(text) else {
            return r;
        };
        for (k, v) in items {
            match k.as_str() {
                "id" => r.id = v.as_str().unwrap_or("").to_string(),
                "text" => r.text = v.as_str().unwrap_or("").to_string(),
                "lang" => r.lang = v.as_str().unwrap_or("").to_string(),
                "doc_path" => r.doc_path = v.as_str().unwrap_or("").to_string(),
                "phase" => r.phase = v.as_str().unwrap_or("").to_string(),
                "selection" => r.selection = v.as_str().unwrap_or("").to_string(),
                "pick" => r.pick = v.as_str().map(str::to_string),
                "n" => r.n = v.as_f64().unwrap_or(0.0).max(0.0) as usize,
                "regions" => {
                    for it in v.as_arr().unwrap_or(&[]) {
                        let line = it.get("line").and_then(Json::as_f64).unwrap_or(0.0) as usize;
                        let t = it
                            .get("text")
                            .and_then(Json::as_str)
                            .unwrap_or("")
                            .to_string();
                        r.regions.push((line, t));
                    }
                }
                "settings" => {
                    let mut out = Vec::new();
                    if let Json::Obj(inner) = v {
                        for (ik, iv) in inner {
                            if let Some(sv) = iv.as_str() {
                                out.push((ik, sv.to_string()));
                            }
                        }
                    }
                    r.settings = Settings(out);
                }
                _ => {}
            }
        }
        r
    }
    /// 입력이 그 언어인가(`ko`).
    pub fn is_ko(&self) -> bool {
        self.lang == "ko"
    }
    /// 확정(Enter) 단계인가.
    pub fn is_commit(&self) -> bool {
        self.phase == "commit"
    }
    /// 취소(Esc) 단계인가.
    pub fn is_cancel(&self) -> bool {
        self.phase == "cancel"
    }
}

/// ★ 프롬프트 변환 응답(ABI v1.2): 값 n개 **또는** 오류(호스트 = 본문 공백 + 경고 행) · `rows`를 내면 미리보기 행을 그대로 쓴다.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransformResponse {
    pub values: Vec<String>,
    pub error: Option<String>,
    /// 미리보기 행 — 비면 호스트가 `L줄: 값`으로 만든다 · `id`가 있는 행은 사용자가 ↑↓로 골라 Enter = 확정 요청의 `pick`(T-447).
    pub rows: Vec<Row>,
    /// 적용 방식(기본 [`Apply::Regions`]).
    pub apply: Apply,
    /// [`Apply::Text`]일 때 선택 전체를 대신할 글.
    pub text: String,
}

/// 프롬프트 미리보기 행(T-447): 글 · 경고색 · 고를 수 있으면 id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub text: String,
    pub warn: bool,
    pub id: Option<String>,
}

/// 변환 결과를 본문에 어떻게 적용하는가: 구간마다 값 · 선택 전체를 글 하나로 · 본문은 두고 행만(정보 프롬프트).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Apply {
    #[default]
    Regions,
    Text,
    None,
}

impl TransformResponse {
    pub fn ok(values: Vec<String>) -> TransformResponse {
        TransformResponse {
            values,
            ..TransformResponse::default()
        }
    }
    pub fn err(msg: &str) -> TransformResponse {
        TransformResponse {
            error: Some(msg.to_string()),
            ..TransformResponse::default()
        }
    }
    /// 선택 전체를 글 하나로 바꾼다(`apply = text`).
    pub fn text(text: &str) -> TransformResponse {
        TransformResponse {
            apply: Apply::Text,
            text: text.to_string(),
            ..TransformResponse::default()
        }
    }
    /// 본문은 두고 행만 보여 준다(`apply = none` · 계산기·조회 프롬프트).
    pub fn none(rows: Vec<(String, bool)>) -> TransformResponse {
        TransformResponse {
            apply: Apply::None,
            ..TransformResponse::default()
        }
        .with_rows(rows)
    }
    /// ★ 고를 수 있는 행(T-447): (id, 글) — 사용자가 고른 id가 commit 요청의 `pick`으로 온다.
    pub fn with_choices(mut self, choices: Vec<(&str, &str)>) -> TransformResponse {
        self.rows = choices
            .into_iter()
            .map(|(id, t)| Row {
                text: t.to_string(),
                warn: false,
                id: Some(id.to_string()),
            })
            .collect();
        self
    }
    pub fn with_rows(mut self, rows: Vec<(String, bool)>) -> TransformResponse {
        self.rows = rows
            .into_iter()
            .map(|(text, warn)| Row {
                text,
                warn,
                id: None,
            })
            .collect();
        self
    }
    pub fn to_json(&self) -> String {
        let mut o = Json::obj();
        match self.apply {
            Apply::Regions => {}
            Apply::Text => o = o.with("apply", "text").with("text", self.text.as_str()),
            Apply::None => o = o.with("apply", "none"),
        }
        match &self.error {
            Some(e) => o = o.with("error", e.as_str()),
            None if self.apply != Apply::Regions => {}
            None => {
                o = o.with(
                    "values",
                    self.values
                        .iter()
                        .map(|v| Json::from(v.as_str()))
                        .collect::<Vec<_>>(),
                )
            }
        }
        if !self.rows.is_empty() {
            o = o.with(
                "rows",
                self.rows
                    .iter()
                    .map(|r| {
                        let mut o = Json::obj()
                            .with("text", r.text.as_str())
                            .with("warn", r.warn);
                        if let Some(id) = &r.id {
                            o = o.with("id", id.as_str());
                        }
                        o
                    })
                    .collect::<Vec<_>>(),
            );
        }
        json::dump(&o)
    }
}

/// 확장이 구현하는 것 — 앱의 `Extension` 트레이트와 같은 다섯 가지 + 포맷터(선택) + 프롬프트 변환(선택).
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
    /// ★ 프롬프트 변환(메타에 `prompts`를 냈을 때만 불린다 · ABI v1.2): 표현식 → 값 n개. 기본 = 없음.
    fn transform(_req: &TransformRequest) -> TransformResponse {
        TransformResponse::err("no transform")
    }
    /// ★ 길 A(ABI v1.3): (재)인스턴스화 뒤 1회(`stateful`만) — 상태 복원은 `storage::get`.
    fn init() {}
    /// ★ 구독한 사건(메타 `events`) — 처리했으면 true.
    fn on_event(_e: &Event) -> bool {
        false
    }
}

/// 사건(`nx_ext_event` 입력): `edit`(doc · version) · `caret`(regions) · `doc`(what = open|close|activate · path).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Event {
    pub kind: String,
    pub doc: String,
    pub version: u64,
    pub regions: Vec<(usize, usize)>,
    pub what: String,
    pub path: String,
}

impl Event {
    pub fn from_json(text: &str) -> Event {
        let mut e = Event::default();
        let Ok(Json::Obj(items)) = json::parse(text) else {
            return e;
        };
        for (k, v) in items {
            match k.as_str() {
                "kind" => e.kind = v.as_str().unwrap_or("").to_string(),
                "doc" => e.doc = v.as_str().unwrap_or("").to_string(),
                "what" => e.what = v.as_str().unwrap_or("").to_string(),
                "path" => e.path = v.as_str().unwrap_or("").to_string(),
                "version" => e.version = v.as_f64().unwrap_or(0.0).max(0.0) as u64,
                "regions" => {
                    for it in v.as_arr().unwrap_or(&[]) {
                        if let Some(ab) = it.as_arr() {
                            if ab.len() == 2 {
                                let a = ab[0].as_f64().unwrap_or(0.0) as usize;
                                let b = ab[1].as_f64().unwrap_or(0.0) as usize;
                                e.regions.push((a, b));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        e
    }
}

/// ★ 확장 전용 상태(길 A · `stateful`): JSON 글 하나(64 KB) · 앱이 `<root>/<id>/state.json`에 보관.
pub mod storage {
    pub fn get() -> Option<String> {
        super::host::storage_get()
    }
    pub fn set(text: &str) -> bool {
        super::host::storage_set(text)
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
        /// ★ 설정 읽기(148차): `key` 버퍼 → 값을 `ptr`(cap)에 길이 접두로 · 반환 = 길이(-1 = 없음·선언 밖).
        fn nx_setting_get(key: i32, ptr: i32, cap: i32) -> i32;
        /// ★ 설정 덮어쓰기 요청(148차): `key`·`val` 버퍼 · 반환 1 = 접수(호스트가 호출 뒤 선언·검증 후 overlay 층에).
        fn nx_setting_set(key: i32, val: i32) -> i32;
        /// ★ 길 A(ABI v1.3 · 154차): 편집기 읽기(`editor.read`) · 쓰기 요청(`editor.write` · 큐) · 상태줄 · 전용 상태.
        fn nx_editor_len() -> i32;
        fn nx_editor_text(from: i32, to: i32, ptr: i32, cap: i32) -> i32;
        fn nx_editor_regions(ptr: i32, cap: i32) -> i32;
        fn nx_editor_replace(from: i32, to: i32, ptr: i32) -> i32;
        fn nx_editor_set_regions(ptr: i32) -> i32;
        fn nx_ui_status(ptr: i32);
        fn nx_storage_get(ptr: i32, cap: i32) -> i32;
        fn nx_storage_set(ptr: i32) -> i32;
    }

    /// 호스트가 길이 접두로 채워 주는 버퍼를 받아 글로(`cap` 바이트까지).
    #[cfg(target_arch = "wasm32")]
    fn recv(cap: usize, call: impl FnOnce(i32, i32) -> i32) -> Option<String> {
        let p = super::buf::alloc(cap);
        let n = call(p as i32, cap as i32);
        // SAFETY: 호스트가 길이 접두 + 본문을 썼다(n ≤ cap) · 버퍼는 우리가 마련했고 아래에서 회수한다.
        let out = if n >= 0 {
            Some(unsafe { super::buf::read(p) })
        } else {
            None
        };
        super::buf::free_cap(p, cap + 4);
        out
    }

    #[cfg(target_arch = "wasm32")]
    pub fn editor_len() -> i32 {
        // SAFETY: 호스트가 링크한 import.
        unsafe { nx_editor_len() }
    }
    #[cfg(target_arch = "wasm32")]
    pub fn editor_text(from: usize, to: usize) -> Option<String> {
        let cap = (to.saturating_sub(from) * 4 + 16).min(1 << 20);
        // SAFETY: 호스트가 링크한 import.
        recv(cap, |p, c| unsafe {
            nx_editor_text(from as i32, to as i32, p, c)
        })
    }
    #[cfg(target_arch = "wasm32")]
    pub fn editor_regions() -> Vec<(usize, usize)> {
        // SAFETY: 호스트가 링크한 import.
        let Some(text) = recv(64 * 1024, |p, c| unsafe { nx_editor_regions(p, c) }) else {
            return Vec::new();
        };
        parse_regions(&text)
    }
    #[cfg(target_arch = "wasm32")]
    pub fn editor_replace(from: usize, to: usize, text: &str) -> bool {
        let b = super::buf::make(text.as_bytes());
        // SAFETY: 호스트가 링크한 import · 버퍼는 호출이 끝날 때까지 산다.
        let r = unsafe { nx_editor_replace(from as i32, to as i32, b as i32) };
        super::buf::free(b);
        r != 0
    }
    #[cfg(target_arch = "wasm32")]
    pub fn editor_set_regions(regions: &[(usize, usize)]) -> bool {
        let text = format!(
            "[{}]",
            regions
                .iter()
                .map(|(a, b)| format!("[{a},{b}]"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let b = super::buf::make(text.as_bytes());
        // SAFETY: 위와 같다.
        let r = unsafe { nx_editor_set_regions(b as i32) };
        super::buf::free(b);
        r != 0
    }
    #[cfg(target_arch = "wasm32")]
    pub fn ui_status(text: &str) {
        let b = super::buf::make(text.as_bytes());
        // SAFETY: 위와 같다.
        unsafe { nx_ui_status(b as i32) };
        super::buf::free(b);
    }
    #[cfg(target_arch = "wasm32")]
    pub fn storage_get() -> Option<String> {
        // SAFETY: 호스트가 링크한 import.
        recv(64 * 1024, |p, c| unsafe { nx_storage_get(p, c) })
    }
    #[cfg(target_arch = "wasm32")]
    pub fn storage_set(text: &str) -> bool {
        let b = super::buf::make(text.as_bytes());
        // SAFETY: 위와 같다.
        let r = unsafe { nx_storage_set(b as i32) };
        super::buf::free(b);
        r != 0
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn editor_len() -> i32 {
        0
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn editor_text(_from: usize, _to: usize) -> Option<String> {
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn editor_regions() -> Vec<(usize, usize)> {
        Vec::new()
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn editor_replace(_from: usize, _to: usize, _text: &str) -> bool {
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn editor_set_regions(_regions: &[(usize, usize)]) -> bool {
        false
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn ui_status(_text: &str) {}
    #[cfg(not(target_arch = "wasm32"))]
    pub fn storage_get() -> Option<String> {
        None
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn storage_set(_text: &str) -> bool {
        false
    }

    /// `[[a,b],…]` → 구간 목록(모양이 다르면 빈 목록).
    pub fn parse_regions(text: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        if let Ok(super::Json::Arr(items)) = super::json::parse(text) {
            for it in items {
                if let Some(ab) = it.as_arr() {
                    if ab.len() == 2 {
                        let a = ab[0].as_f64().unwrap_or(0.0) as usize;
                        let b = ab[1].as_f64().unwrap_or(0.0) as usize;
                        out.push((a, b));
                    }
                }
            }
        }
        out
    }

    /// 설정 값 하나(전용 키 + 메타 `settings_read`에 선언한 기본 키만 · 그 밖은 None).
    #[cfg(target_arch = "wasm32")]
    pub fn setting_get(key: &str) -> Option<String> {
        const CAP: usize = 4096;
        let k = super::buf::make(key.as_bytes());
        let p = super::buf::alloc(CAP);
        // SAFETY: 호스트가 링크한 import · 두 버퍼는 우리가 마련했고 아래에서 회수한다.
        let n = unsafe { nx_setting_get(k as i32, p as i32, CAP as i32) };
        let out = if n >= 0 {
            // SAFETY: 호스트가 길이 접두 + 본문을 썼다(n ≤ CAP).
            Some(unsafe { super::buf::read(p) })
        } else {
            None
        };
        super::buf::free(k);
        super::buf::free_cap(p, CAP + 4);
        out
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn setting_get(_key: &str) -> Option<String> {
        None
    }

    /// 설정 덮어쓰기 요청(메타 `settings_write`에 선언한 키만 · 런타임 overlay · 확장이 꺼지면 원복) — 반환 = 접수 여부(적용 결과는 앱 로그).
    #[cfg(target_arch = "wasm32")]
    pub fn setting_set(key: &str, value: &str) -> bool {
        let k = super::buf::make(key.as_bytes());
        let v = super::buf::make(value.as_bytes());
        // SAFETY: 호스트가 링크한 import · 버퍼는 호출이 끝날 때까지 산다(아래에서 회수).
        let r = unsafe { nx_setting_set(k as i32, v as i32) };
        super::buf::free(k);
        super::buf::free(v);
        r != 0
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn setting_set(_key: &str, _value: &str) -> bool {
        false
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

/// export 아홉(기본 5 + 포맷 + 변환 + init + 사건)을 만든다 — `export_extension!(MyExt);`
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
        #[no_mangle]
        pub extern "C" fn nx_ext_init() {
            <$t as $crate::Extension>::init();
        }
        #[no_mangle]
        pub extern "C" fn nx_ext_event(ptr: i32) -> i32 {
            // SAFETY: 위와 같다.
            let text = unsafe { $crate::buf::read(ptr as *const u8) };
            i32::from(<$t as $crate::Extension>::on_event(
                &$crate::Event::from_json(&text),
            ))
        }
        #[no_mangle]
        pub extern "C" fn nx_ext_transform(ptr: i32) -> i32 {
            // SAFETY: 위와 같다.
            let text = unsafe { $crate::buf::read(ptr as *const u8) };
            let req = $crate::TransformRequest::from_json(&text);
            let out = <$t as $crate::Extension>::transform(&req).to_json();
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
            prompts: vec![],
            id: "x".into(),
            name: "X".into(),
            settings_prefix: "x.".into(),
            settings_read: vec![],
            settings_write: vec![],
            stateful: false,
            capabilities: vec![],
            events: vec![],
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

    /// ★ ABI v1.2(148차): 프롬프트 메타 JSON · 변환 요청 읽기 · 응답(값/오류/행) 쓰기.
    #[test]
    fn prompt_meta_and_transform_json() {
        let p = Prompt::new(
            "edit.seq",
            Label::en("Seq"),
            "1+1",
            Label::new("hint {0}", "안내 {0}"),
        )
        .with_key("ctrl+alt+n")
        .with_placeholder_key("ext.seq.default")
        .with_help(vec![("1+1", Label::en("count"))]);
        let m = Meta {
            id: "seq".into(),
            name: "Seq".into(),
            settings_prefix: "ext.seq.".into(),
            prompts: vec![p],
            ..Meta::default()
        };
        let j = json::dump(&m.to_json());
        assert!(j.contains("\"prompts\":[{\"id\":\"edit.seq\""), "{j}");
        assert!(
            j.contains("\"placeholder_key\":\"ext.seq.default\"")
                && j.contains("\"key\":\"ctrl+alt+n\"")
        );
        assert!(j.contains("\"help\":[{\"expr\":\"1+1\""));
        let r = TransformRequest::from_json(
            r#"{"id":"edit.seq","text":"a+1","n":3,"regions":[{"line":2,"text":"x"},{"line":5,"text":""}],"lang":"ko","doc_path":"","settings":{"ext.seq.default":"1+1"}}"#,
        );
        assert_eq!(
            (r.id.as_str(), r.text.as_str(), r.n),
            ("edit.seq", "a+1", 3)
        );
        assert_eq!(r.regions, vec![(2, "x".to_string()), (5, String::new())]);
        assert!(r.is_ko() && r.settings.get("ext.seq.default") == Some("1+1"));
        assert_eq!(
            TransformResponse::ok(vec!["a".into(), "b".into()]).to_json(),
            r#"{"values":["a","b"]}"#
        );
        let e = TransformResponse::err("bad").with_rows(vec![("L1: ?".into(), true)]);
        assert_eq!(
            e.to_json(),
            r#"{"error":"bad","rows":[{"text":"L1: ?","warn":true}]}"#
        );
        assert_eq!(
            TransformResponse::text("T").to_json(),
            r#"{"apply":"text","text":"T"}"#
        );
        assert_eq!(
            TransformResponse::none(vec![("42".into(), false)]).to_json(),
            r#"{"apply":"none","rows":[{"text":"42","warn":false}]}"#
        );
        let r2 = TransformRequest::from_json(r#"{"phase":"commit","selection":"abc","pick":"ko"}"#);
        assert!(r2.is_commit() && !r2.is_cancel() && r2.selection == "abc");
        assert_eq!(r2.pick.as_deref(), Some("ko"));
        assert_eq!(
            TransformResponse::none(vec![])
                .with_choices(vec![("hi", "Hi")])
                .to_json(),
            r#"{"apply":"none","rows":[{"text":"Hi","warn":false,"id":"hi"}]}"#
        );
        let m2 = Meta {
            settings_read: vec!["ui.lang".into()],
            settings_write: vec!["editor.*".into()],
            ..Meta::default()
        };
        let j2 = json::dump(&m2.to_json());
        assert!(
            j2.contains("\"settings_read\":[\"ui.lang\"]")
                && j2.contains("\"settings_write\":[\"editor.*\"]"),
            "{j2}"
        );
        // ★ ABI v1.3(154차): stateful · capabilities · events · 사건 JSON · 구간 JSON.
        let m3 = Meta {
            stateful: true,
            capabilities: vec!["editor.read".into(), "editor.write".into()],
            events: vec!["caret".into()],
            ..Meta::default()
        };
        let j3 = json::dump(&m3.to_json());
        assert!(
            j3.contains("\"stateful\":true")
                && j3.contains("\"capabilities\":[\"editor.read\",\"editor.write\"]")
                && j3.contains("\"events\":[\"caret\"]"),
            "{j3}"
        );
        let e = Event::from_json(r#"{"kind":"caret","regions":[[1,2],[5,5]],"version":7}"#);
        assert_eq!(
            (e.kind.as_str(), e.version, e.regions.len()),
            ("caret", 7, 2)
        );
        assert_eq!(host::parse_regions("[[0,3],[4,7]]"), vec![(0, 3), (4, 7)]);
        assert!(host::parse_regions("nope").is_empty());
    }

    #[test]
    fn buffers_round_trip() {
        let p = buf::make("한글 ok".as_bytes());
        // SAFETY: make이 준 포인터.
        assert_eq!(unsafe { buf::read(p) }, "한글 ok");
        buf::free(p);
    }
}
