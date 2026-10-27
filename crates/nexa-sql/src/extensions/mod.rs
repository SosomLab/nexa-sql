//! **확장(extension) 호스트 API**(docs/50 §4·§7 · docs/51 §5·§9 · docs/75 — D-91 "코어 + in-process 확장 층" + WASM).
//!
//! in-process 확장(이 저장소 · Rust · `builtin`)과 **WASM 확장**(SDK로 만든 `.wasm` · [`wasm::WasmExtension`])이 **같은 표면**을
//! 본다: 명령 · 우클릭/편집 메뉴 기여 · 설정 → 편집기 옵션 · 편집기 이동. 편집기 내부(쌍 표 · 데코레이션 계산)는
//! nexa-ctl 코어에 있고, 확장은 "무엇을 켜고 어떤 명령/메뉴를 두는가"만 정한다.
//!
//! 등록 = [`Registry::builtin`] + [`Registry::sync_wasm`](설치된 wasm 패키지 로드 · 같은 id의 내장은 대체) · 호스트는
//! [`Extension`] 트레이트만 안다(30 §1 포트+레지스트리+설정).

use nexa_ctl::controls::ctxmenu::CtxItem;
use nsql_settings::Settings;
use std::path::{Path, PathBuf};

pub(crate) mod insert_sequence;
pub(crate) mod manager;
pub(crate) mod rainbow_pairs;
pub(crate) mod sha256;
pub(crate) mod wasm;

/// 표시 이름 — 내장은 i18n 메시지, WASM은 메타의 글(영어 + 한국어 선택).
#[derive(Clone, Debug)]
pub(crate) enum Label {
    Msg(nsql_i18n::Msg),
    Text(String, Option<String>),
}

impl Label {
    /// 지금 언어의 글.
    pub(crate) fn text(&self) -> String {
        match self {
            Label::Msg(m) => nsql_i18n::t(*m).to_string(),
            Label::Text(en, ko) => {
                if nsql_i18n::current_lang() == nsql_i18n::Lang::Ko {
                    ko.clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| en.clone())
                } else {
                    en.clone()
                }
            }
        }
    }
}

/// 확장이 호스트에 내는 명령 하나(팔레트·키맵·메뉴 id는 같은 문자열).
#[derive(Clone, Debug)]
pub(crate) struct Command {
    pub(crate) id: String,
    pub(crate) label: Label,
    /// ★ 기본 단축키(146차 10-27 · docs/75 §9 ②): Windows/Linux 코드(`ctrl+alt+p`) · mac 코드(비면 `key`에서 ctrl → cmd).
    ///   사용자 설정 `key.<id>`가 있으면 그것이 우선(앱 명령과 같은 규칙).
    pub(crate) key: Option<String>,
    pub(crate) key_mac: Option<String>,
}

impl Command {
    pub(crate) fn msg(id: &str, label: nsql_i18n::Msg) -> Command {
        Command {
            id: id.to_string(),
            label: Label::Msg(label),
            key: None,
            key_mac: None,
        }
    }
}

/// 메뉴 기여가 붙는 자리(146차 · docs/97 §5 ①): 편집기 우클릭 · 프로젝트 탐색기 행 우클릭.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuTarget {
    Editor,
    ProjectExplorer,
}

/// 메뉴 기여 — 우클릭 메뉴에 붙는 서브메뉴(라벨 + 항목 id/라벨/단축키 표시).
#[derive(Clone, Debug)]
pub(crate) struct MenuContribution {
    pub(crate) id: String,
    pub(crate) label: Label,
    pub(crate) items: Vec<Command>,
    pub(crate) target: MenuTarget,
}

/// ★ 확장이 선언하는 설정 항목(146차 · T-258 · docs/75 §9 ①): 호스트가 런타임 레지스트리에 등재해 설정 창·`Settings::get`이 안다.
#[derive(Clone, Debug)]
pub(crate) struct SettingDef {
    pub(crate) key: String,
    pub(crate) label: Label,
    pub(crate) desc: Label,
    pub(crate) kind: SettingDefKind,
    pub(crate) default: String,
}

#[derive(Clone, Debug)]
pub(crate) enum SettingDefKind {
    Bool,
    Int { min: i64, max: i64 },
    Text,
    Choice(Vec<(String, Label)>),
}

/// ★ 확장이 요청하는 파일 동작(146차 · docs/97 §5 ②) — 명령 실행이 끝난 뒤 호스트가 적용(클립보드는 명시적 명령에서만).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FileOp {
    Open(String),
    Reveal(String),
    CopyPath(String),
}

/// ★ 프롬프트 선언(148차 · ABI v1.2 · docs/124 §7 권장안 B): 확장이 라이브 프롬프트 하나를 낸다 — 호스트가 세션(구간 읽기 · 값 교체 ·
/// 복원 · 되돌리기 · 팔레트 UI)을 전부 들고, 확장은 [`Extension::transform`]으로 **값만 제안**한다(본문 쓰기 권한 0).
#[derive(Clone, Debug)]
pub(crate) struct Prompt {
    /// 명령 id(팔레트·키맵·메뉴와 같은 문자열 · wasm은 `commands`에 없으면 호스트가 같은 라벨·키로 등록).
    pub(crate) id: String,
    pub(crate) label: Label,
    /// 자리표시자 = 빈 입력의 뜻 · `placeholder_key` 설정 값이 비어 있지 않으면 그 값이 대신한다.
    pub(crate) placeholder: String,
    pub(crate) placeholder_key: Option<String>,
    /// 입력란 아래 안내(`{0}` = 자리표시자).
    pub(crate) hint: Label,
    /// 기본 단축키(wasm 메타 · 명령 병합 때 쓴다).
    pub(crate) key: Option<String>,
    /// `?` 치트 시트 = (표현식, 설명).
    pub(crate) help: Vec<(String, Label)>,
}

/// 프롬프트 변환 요청(키 입력마다 · 148차): 표현식 · 구간 수 · 구간(줄, 원문 · 상한까지) · 언어 · 문서 경로.
#[derive(Clone, Debug, Default)]
pub(crate) struct TransformReq {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) n: usize,
    pub(crate) regions: Vec<(usize, String)>,
    pub(crate) lang: String,
    pub(crate) doc_path: String,
    /// 단계 = `live`(키 입력마다) · `commit`(Enter) · `cancel`(Esc) — 확정 때만 할 일이 있는 플러그인용.
    pub(crate) phase: String,
    /// 활성 선택의 글(구간이 여럿이면 첫 구간 시작 ~ 마지막 구간 끝 · 상한 안에서).
    pub(crate) selection: String,
}

/// 프롬프트 변환 응답: 값 n개 **또는** 오류 문구(호스트 = 본문 공백 + 경고 행) · `rows` = 미리보기 행 덮어쓰기(비면 호스트가 만든다).
#[derive(Clone, Debug, Default)]
pub(crate) struct TransformOut {
    pub(crate) values: Vec<String>,
    pub(crate) error: Option<String>,
    pub(crate) rows: Vec<(String, bool)>,
    /// 적용 방식 = `regions`(기본 · 구간마다 값) · `text`(선택 전체를 글 하나로) · `none`(본문 안 건드림 · 행만).
    pub(crate) apply: String,
    /// `apply = text`일 때의 글.
    pub(crate) text: String,
}

/// 설정 키 패턴 일치(메타 `settings_read`/`settings_write` · 148차): 정확히 같거나 `prefix*`.
pub(crate) fn key_matches(pat: &str, key: &str) -> bool {
    match pat.strip_suffix('*') {
        Some(p) => key.starts_with(p),
        None => pat == key,
    }
}

/// 확장 포트 — 호스트는 이 트레이트로만 부른다(WASM은 같은 이름의 export).
pub(crate) trait Extension {
    fn id(&self) -> &str;
    /// 설정 키 접두(`rainbow.`) — 이 접두의 키가 바뀌면 [`Extension::on_settings`]가 불린다.
    fn settings_prefix(&self) -> &str;
    fn commands(&self) -> Vec<Command>;
    fn menus(&self) -> Vec<MenuContribution>;
    /// 설정이 바뀜(또는 시작) → 호스트가 적용할 편집기 옵션을 돌려준다.
    fn on_settings(&mut self, settings: &Settings) -> ExtensionEffect;
    /// 사용자가 껐을 때(Extension Manager: Disable) 적용할 효과 — 기본 = 아무것도 안 함.
    fn disabled_effect(&self) -> ExtensionEffect {
        ExtensionEffect::default()
    }
    /// 사람이 읽는 이름(팔레트 목록).
    fn name(&self) -> &str;
    /// 명령 실행 — 편집기 조작은 [`EditorOps`]로.
    fn run(&mut self, id: &str, ed: &mut dyn EditorOps) -> bool;
    /// ★ 명령 실행 + **호스트 상황**(09-30 · docs/97): `doc_path` = 활성 문서의 파일 경로(없으면 빈 글). WASM 확장은 이 값을
    /// 게스트 import `nx_host_get(DOC_PATH)`로 돌려준다. 기본 = 상황 없이 [`Extension::run`].
    fn run_with(
        &mut self,
        id: &str,
        ed: &mut dyn EditorOps,
        doc_path: &str,
        _s: &Settings,
    ) -> bool {
        let _ = doc_path;
        self.run(id, ed)
    }
    /// 내려받은 WASM 모듈인가(내장 = false).
    fn is_wasm(&self) -> bool {
        false
    }
    /// WASM 확장이면 자신(로그 수거용 다운캐스트).
    fn as_wasm(&self) -> Option<&wasm::WasmExtension> {
        None
    }
    /// ★ SQL 포맷터 제공(ABI v1.1 · docs/95): (표시 이름, 미리보기 예시 SQL) — 없으면 None.
    fn formatter(&self) -> Option<(Label, String)> {
        None
    }
    /// ★ 포맷터 미리보기 표식(메타 `formatter.marks` · 09-30): (설정 키, 조각) — 그 확장 설정 카드의 "▶ 미리보기 n행".
    fn formatter_marks(&self) -> Vec<(String, String)> {
        Vec::new()
    }
    /// 포맷 호출(입력 JSON `{"text","dialect","options","settings","preview"}` → 출력 JSON `{"text"}`/`{"error"}`).
    fn format(&self, _input_json: &str) -> Result<String, String> {
        Err("no formatter".to_string())
    }
    /// ★ 선언한 설정 항목(메타 `settings` · 146차) — 내장은 레지스트리에 정적 등록돼 있어 비어 있다.
    fn settings_defs(&self) -> Vec<SettingDef> {
        Vec::new()
    }
    /// ★ 마지막 명령 실행이 남긴 파일 동작 요청(146차) — 거둬 가면 비운다.
    fn take_file_ops(&self) -> Vec<FileOp> {
        Vec::new()
    }

    /// ★ 프롬프트 선언(148차 · docs/124 §7) — 기본 없음.
    fn prompts(&self) -> Vec<Prompt> {
        Vec::new()
    }
    /// ★ 프롬프트 변환: 표현식 → 값 n개(또는 오류 문구). 기본 = 없음.
    fn transform(&self, _req: &TransformReq, _s: &Settings) -> Result<TransformOut, String> {
        Err("no transform".to_string())
    }
    /// 설치 기록 없이 **기본 켜지는** 내장 확장(D-368 · Insert Sequence) — 끄기/켜기만 가능.
    fn default_on(&self) -> bool {
        false
    }
    /// ★ 이 확장이 읽는 **기본 설정** 키 패턴(148차 · 사용자 10-27): 전용 키와 함께 설정 JSON에 실리고, 바뀌면 `on_settings`가 다시 불린다.
    fn settings_read(&self) -> Vec<String> {
        Vec::new()
    }
    /// ★ 이 확장이 덮어쓸 수 있다고 선언한 기본 설정 키 패턴 — 선언 밖은 호스트가 거절(로그).
    fn settings_write(&self) -> Vec<String> {
        Vec::new()
    }
    /// 마지막 호출이 남긴 설정 덮어쓰기 요청 (키, 값) — 호스트가 선언·검증 뒤 overlay 층에 넣는다.
    fn take_setting_sets(&self) -> Vec<(String, String)> {
        Vec::new()
    }
}

/// 설정 반영 결과(호스트가 편집기 전 탭에 적용).
#[derive(Clone, Debug, Default)]
pub(crate) struct ExtensionEffect {
    pub(crate) bracket_opts: Option<nexa_ctl::BracketOpts>,
}

/// 편집기 조작 표면(확장이 쓰는 것만 · 작게 유지).
pub(crate) trait EditorOps {
    fn goto_bracket(&mut self, shift: bool) -> bool;
    fn expand_to_brackets(&mut self) -> bool;
    fn goto_bracket_sibling(&mut self, next: bool, shift: bool) -> bool;
    fn goto_bracket_parent(&mut self, shift: bool) -> bool;
    fn goto_bracket_child(&mut self, shift: bool) -> bool;
}

impl EditorOps for nexa_ctl::TextBox {
    fn goto_bracket(&mut self, shift: bool) -> bool {
        nexa_ctl::TextBox::goto_bracket(self, shift)
    }
    fn expand_to_brackets(&mut self) -> bool {
        nexa_ctl::TextBox::expand_to_brackets(self)
    }
    fn goto_bracket_sibling(&mut self, next: bool, shift: bool) -> bool {
        nexa_ctl::TextBox::goto_bracket_sibling(self, next, shift)
    }
    fn goto_bracket_parent(&mut self, shift: bool) -> bool {
        nexa_ctl::TextBox::goto_bracket_parent(self, shift)
    }
    fn goto_bracket_child(&mut self, shift: bool) -> bool {
        nexa_ctl::TextBox::goto_bracket_child(self, shift)
    }
}

/// 레지스트리 — 내장 확장 + 로드된 WASM 확장(같은 `Box<dyn Extension>`).
pub(crate) struct Registry {
    pub(crate) extensions: Vec<Box<dyn Extension>>,
    /// 지금 로드된 wasm (id, 모듈 경로) — 설치 목록과 비교해 바뀐 것만 다시 로드.
    loaded_wasm: Vec<(String, PathBuf)>,
}

impl Registry {
    /// 내장 확장(Rainbow Pairs · Insert Sequence(148차 · 기본 켜짐 D-368)).
    #[must_use]
    pub(crate) fn builtin() -> Self {
        Registry {
            extensions: vec![
                Box::new(rainbow_pairs::RainbowPairs),
                Box::new(insert_sequence::InsertSequence),
            ],
            loaded_wasm: Vec::new(),
        }
    }

    /// 어느 확장의 명령인가 — **WASM이 내장과 같은 id면 WASM이 앞선다**(대체).
    pub(crate) fn owner_of(&self, cmd: &str) -> Option<usize> {
        let owns = |p: &dyn Extension| p.commands().iter().any(|c| c.id == cmd);
        self.active_indices()
            .into_iter()
            .find(|&i| owns(&*self.extensions[i]))
    }

    /// 활성 확장의 인덱스 — 같은 id가 둘(내장 + WASM)이면 WASM만.
    fn active_indices(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for (i, p) in self.extensions.iter().enumerate() {
            if !p.is_wasm()
                && self
                    .extensions
                    .iter()
                    .any(|q| q.is_wasm() && q.id() == p.id())
            {
                continue;
            }
            out.push(i);
        }
        out
    }

    /// 확장 (id, 이름, wasm?) 목록 — 매니저 목록에 `builtin`/`wasm`으로 보인다(같은 id는 하나 · WASM 우선).
    pub(crate) fn ids(&self) -> Vec<(String, String)> {
        self.active_indices()
            .into_iter()
            .map(|i| {
                let p = &self.extensions[i];
                (p.id().to_string(), p.name().to_string())
            })
            .collect()
    }

    /// ★ 프롬프트 소유자(148차): 활성(wasm이 내장을 대체) + 켜진 확장 중 그 id의 프롬프트 → (확장 id, 프롬프트 사본).
    pub(crate) fn prompt_owner(&self, id: &str, disabled: &[String]) -> Option<(String, Prompt)> {
        self.active_indices().into_iter().find_map(|i| {
            let p = &self.extensions[i];
            if disabled.iter().any(|d| d == p.id()) {
                return None;
            }
            p.prompts()
                .into_iter()
                .find(|x| x.id == id)
                .map(|x| (p.id().to_string(), x))
        })
    }

    /// 그 프롬프트를 선언한 활성 확장의 이름(꺼짐 여부 무관 · "꺼져 있다" 안내용).
    pub(crate) fn prompt_ext_name(&self, id: &str) -> Option<String> {
        self.active_indices().into_iter().find_map(|i| {
            let p = &self.extensions[i];
            p.prompts()
                .iter()
                .any(|x| x.id == id)
                .then(|| p.name().to_string())
        })
    }

    /// 어느 활성 확장이든 그 id의 프롬프트를 선언했는가(메뉴 라우팅 · 꺼짐 포함).
    pub(crate) fn has_prompt(&self, id: &str) -> bool {
        self.prompt_ext_name(id).is_some()
    }

    /// ★ 변환 호출(확장 id로 · wasm 로그·오류는 `take_wasm_notes`로 거둔다).
    pub(crate) fn transform(
        &self,
        ext_id: &str,
        req: &TransformReq,
        s: &Settings,
    ) -> Result<TransformOut, String> {
        let i = self
            .active_indices()
            .into_iter()
            .find(|&i| self.extensions[i].id() == ext_id)
            .ok_or_else(|| format!("{ext_id}: not loaded"))?;
        self.extensions[i].transform(req, s)
    }

    /// 설치 기록 없이 기본 켜진 **활성** 내장 확장 (id, 이름)(D-368 · 같은 id의 wasm이 설치되면 빠진다).
    pub(crate) fn default_on_ids(&self) -> Vec<(String, String)> {
        self.active_indices()
            .into_iter()
            .map(|i| &self.extensions[i])
            .filter(|p| p.default_on())
            .map(|p| (p.id().to_string(), p.name().to_string()))
            .collect()
    }

    /// 로드된 WASM 확장 id 목록.
    #[cfg(test)]
    pub(crate) fn wasm_ids(&self) -> Vec<String> {
        self.extensions
            .iter()
            .filter(|p| p.is_wasm())
            .map(|p| p.id().to_string())
            .collect()
    }

    /// 설치된 wasm 패키지 목록 `(id, 모듈 경로)`와 맞춘다 — 새로 설치된 것은 로드, 삭제된 것은 내림, 같은 것은 그대로.
    /// 로드 실패는 안내 문장으로 돌려주고 그 id의 내장(있으면)이 그대로 산다(폴백).
    pub(crate) fn sync_wasm(&mut self, want: &[(String, PathBuf)]) -> Vec<String> {
        let mut notes = Vec::new();
        if self.loaded_wasm == want {
            return notes;
        }
        // 내린다: 지금 로드됐지만 목록에 없는(또는 경로가 바뀐) 것.
        let stale: Vec<String> = self
            .loaded_wasm
            .iter()
            .filter(|l| !want.iter().any(|w| w == *l))
            .map(|(id, _)| id.clone())
            .collect();
        if !stale.is_empty() {
            self.extensions
                .retain(|p| !(p.is_wasm() && stale.iter().any(|s| s == p.id())));
            self.loaded_wasm.retain(|l| !stale.contains(&l.0));
            for id in &stale {
                notes.push(format!("wasm extension unloaded: {id}"));
            }
        }
        // 올린다.
        for (id, path) in want {
            if self
                .loaded_wasm
                .iter()
                .any(|l| l == &(id.clone(), path.clone()))
            {
                continue;
            }
            let t0 = std::time::Instant::now();
            match wasm::WasmExtension::load_file(path) {
                Ok(ext) => {
                    if ext.id() != id {
                        notes.push(format!(
                            "wasm extension {}: module says id {} - not loaded",
                            id,
                            ext.id()
                        ));
                        continue;
                    }
                    notes.push(format!(
                        "wasm extension loaded: {} {} ({} commands · {} ms · {})",
                        id,
                        ext.name(),
                        ext.commands().len(),
                        t0.elapsed().as_millis(),
                        nexa_fs::path::display(path)
                    ));
                    self.extensions.push(Box::new(ext));
                    self.loaded_wasm.push((id.clone(), path.clone()));
                }
                Err(e) => notes.push(format!(
                    "wasm extension {id}: load failed - {e} (builtin fallback if any)"
                )),
            }
        }
        notes
    }

    /// 우클릭 메뉴 추가 항목(켜진 확장만 · 서브메뉴로 · `display` = 단축키 표시 조회 · `target` = 편집기/프로젝트 탐색기 · 146차).
    pub(crate) fn menu_extras(
        &self,
        disabled: &[String],
        display: &dyn Fn(&str) -> String,
        target: MenuTarget,
    ) -> Vec<CtxItem> {
        let mut out = Vec::new();
        for i in self.active_indices() {
            let p = &self.extensions[i];
            if disabled.iter().any(|d| d == p.id()) {
                continue;
            }
            for m in p.menus().into_iter().filter(|m| m.target == target) {
                // 하위 메뉴 안에서는 부모(확장) 이름 접두를 벗긴다(146차 · "Project Explorer Menus: 새 탭으로 열기" → "새 탭으로 열기").
                let parent = m.label.text();
                let items: Vec<CtxItem> = m
                    .items
                    .iter()
                    .map(|c| {
                        let text = c.label.text();
                        let text = text
                            .strip_prefix(&format!("{parent}: "))
                            .map_or(text.clone(), str::to_string);
                        CtxItem::item(c.id.clone(), text).with_shortcut(display(&c.id))
                    })
                    .collect();
                out.push(CtxItem::submenu(m.id.clone(), m.label.text(), items));
            }
        }
        out
    }

    /// 설정 변경(키가 어느 접두에 속하면 그 확장만) → 효과 목록. `disabled`에 든 확장은 끈 효과를 낸다.
    pub(crate) fn on_settings(
        &mut self,
        settings: &Settings,
        changed_key: Option<&str>,
        disabled: &[String],
    ) -> Vec<ExtensionEffect> {
        let active = self.active_indices();
        let mut out = Vec::new();
        for i in active {
            let p = &mut self.extensions[i];
            // 바뀐 키가 이 확장 것(접두)이거나 **선언해서 읽는 기본 설정**이면 다시 묻는다(148차).
            if !changed_key.is_none_or(|k| {
                k.starts_with(p.settings_prefix())
                    || p.settings_read().iter().any(|pat| key_matches(pat, k))
            }) {
                continue;
            }
            out.push(if disabled.iter().any(|d| d == p.id()) {
                p.disabled_effect()
            } else {
                p.on_settings(settings)
            });
        }
        out
    }

    /// 명령 실행(소유 확장에 위임) — 켜져 있을 때만.
    /// [`Registry::run`] + 호스트 상황(활성 문서 경로 · docs/97 §2).
    pub(crate) fn run_with(
        &mut self,
        cmd: &str,
        disabled: &[String],
        ed: &mut dyn EditorOps,
        doc_path: &str,
        settings: &Settings,
    ) -> bool {
        let Some(i) = self.owner_of(cmd) else {
            return false;
        };
        if disabled.iter().any(|d| d == self.extensions[i].id()) {
            return false;
        }
        self.extensions[i].run_with(cmd, ed, doc_path, settings)
    }

    /// ★ 어느 활성 확장이든 이 기본 설정 키를 읽는다고 선언했는가(바뀌면 `apply_extensions(Some(key))` · 148차).
    pub(crate) fn reads_key(&self, key: &str) -> bool {
        self.active_indices().into_iter().any(|i| {
            self.extensions[i]
                .settings_read()
                .iter()
                .any(|pat| key_matches(pat, key))
        })
    }

    /// 확장이 그 키를 덮어써도 되는가(메타 `settings_write` 선언 · 자기 접두 키는 늘 허용).
    pub(crate) fn writable(&self, ext_id: &str, key: &str) -> bool {
        self.extensions.iter().any(|p| {
            p.id() == ext_id
                && (key.starts_with(p.settings_prefix())
                    || p.settings_write().iter().any(|pat| key_matches(pat, key)))
        })
    }

    /// 모든 확장의 설정 덮어쓰기 요청을 거둔다 → (확장 id, 키, 값).
    pub(crate) fn drain_setting_sets(&self) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for p in &self.extensions {
            for (k, v) in p.take_setting_sets() {
                out.push((p.id().to_string(), k, v));
            }
        }
        out
    }

    /// ★ 켜진 확장 중 포맷터를 내는 것들: (id, 이름, 예시 SQL).
    pub(crate) fn formatters(&self, disabled: &[String]) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for i in self.active_indices() {
            let p = &self.extensions[i];
            if disabled.iter().any(|d| d == p.id()) {
                continue;
            }
            if let Some((label, sample)) = p.formatter() {
                out.push((p.id().to_string(), label.text(), sample));
            }
        }
        out
    }

    /// 포맷터 확장 `id`의 미리보기 표식(없으면 빈 목록).
    pub(crate) fn formatter_marks(&self, id: &str) -> Vec<(String, String)> {
        self.extensions
            .iter()
            .find(|p| p.id() == id)
            .map(|p| p.formatter_marks())
            .unwrap_or_default()
    }

    /// ★ 확장 포맷터로 포맷(docs/95): 공통 옵션 `format.*` 쌍 + 그 확장의 접두 설정을 JSON으로 넘기고 본문을 받는다.
    pub(crate) fn format_with(
        &self,
        id: &str,
        text: &str,
        dialect: &str,
        options: &[(String, String)],
        settings: &Settings,
        preview: bool,
    ) -> Result<String, String> {
        use nsql_settings::json::{self, Json};
        let i = self
            .active_indices()
            .into_iter()
            .find(|&i| self.extensions[i].id() == id && self.extensions[i].formatter().is_some())
            .ok_or_else(|| format!("formatter not available: {id}"))?;
        let p = &self.extensions[i];
        let own = json::parse(&wasm::settings_json(
            settings,
            p.settings_prefix(),
            &p.settings_read(),
        ))
        .unwrap_or(Json::Obj(Vec::new()));
        let opts = Json::Obj(
            options
                .iter()
                .map(|(k, v)| (k.clone(), Json::Str(v.clone())))
                .collect(),
        );
        let req = Json::Obj(vec![
            ("text".to_string(), Json::Str(text.to_string())),
            ("dialect".to_string(), Json::Str(dialect.to_string())),
            ("options".to_string(), opts),
            ("settings".to_string(), own),
            ("preview".to_string(), Json::Bool(preview)),
        ]);
        let out = p.format(&json::dump(&req))?;
        let v = json::parse(&out).map_err(|e| format!("format json: {e}"))?;
        if let Some(e) = wasm::jget(&v, "error").and_then(wasm::jstr) {
            return Err(e.to_string());
        }
        wasm::jget(&v, "text")
            .and_then(wasm::jstr)
            .map(str::to_string)
            .ok_or_else(|| "format: no text".to_string())
    }

    /// WASM 확장이 남긴 로그·오류를 거둔다(호스트가 로그 창에).
    pub(crate) fn take_wasm_notes(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        for p in &self.extensions {
            if let Some(w) = p.as_wasm() {
                out.append(&mut w.notes.borrow_mut());
            }
        }
        out
    }

    /// ★ 마지막 명령이 남긴 파일 동작 요청 전부(146차 · 호스트가 적용).
    pub(crate) fn drain_file_ops(&self) -> Vec<FileOp> {
        let mut out = Vec::new();
        for p in &self.extensions {
            out.extend(p.take_file_ops());
        }
        out
    }

    /// ★ 켜진 확장의 명령 전부 `(확장 이름, 명령)`(팔레트 라벨 · 키맵 기본 코드 · 146차).
    pub(crate) fn commands_all(&self, disabled: &[String]) -> Vec<(String, Command)> {
        let mut out = Vec::new();
        for i in self.active_indices() {
            let p = &self.extensions[i];
            if disabled.iter().any(|d| d == p.id()) {
                continue;
            }
            for c in p.commands() {
                out.push((p.name().to_string(), c));
            }
        }
        out
    }

    /// ★ 활성 확장이 선언한 설정 항목 `(id, 이름, 항목들)`(146차 · T-258).
    pub(crate) fn settings_defs_all(&self) -> Vec<(String, String, Vec<SettingDef>)> {
        self.active_indices()
            .into_iter()
            .map(|i| {
                let p = &self.extensions[i];
                (p.id().to_string(), p.name().to_string(), p.settings_defs())
            })
            .filter(|(_, _, d)| !d.is_empty())
            .collect()
    }
}

/// 설치된 wasm 패키지의 모듈 경로(보관 사본 폴더 안 첫 `.wasm`).
pub(crate) fn wasm_module_path(root: &Path, id: &str, version: &str) -> Option<PathBuf> {
    let meta = manager::installed_meta_in(root, id, version)?;
    let f = meta.files.iter().find(|f| f.path.ends_with(".wasm"))?;
    Some(root.join(id).join(version).join(&f.path))
}

#[cfg(test)]
mod wasm_registry_tests {
    use super::*;

    /// 소스 트리 `extensions/`(공식 저장소 루트)에서 wasm 패키지를 **설치 → 로드 → 내장 대체 → 삭제 → 내장 복귀**까지.
    #[test]
    fn install_load_replace_and_unload_wasm_package() {
        let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../extensions");
        let tmp = std::env::temp_dir().join(format!("nexa-ext-wasm-{}", std::process::id()));
        let root = tmp.join("root");
        let config = tmp.join("config");
        let _ = std::fs::remove_dir_all(&tmp);
        let src = manager::Source::Dir(src_dir);
        let idx = manager::fetch_index(&src).expect("index");
        let sum = idx
            .packages
            .iter()
            .find(|p| p.id == "rainbow-pairs")
            .expect("rainbow-pairs in index");
        assert_eq!(sum.kind, manager::Kind::Wasm);
        let meta = manager::install_into(&src, sum, &root, &config).expect("install wasm");
        assert_eq!(meta.kind, manager::Kind::Wasm);
        let path = wasm_module_path(&root, "rainbow-pairs", &meta.version).expect("module path");
        assert!(path.is_file(), "보관 사본 {}", path.display());

        assert!(key_matches("editor.*", "editor.pair_kinds") && key_matches("ui.lang", "ui.lang"));
        assert!(!key_matches("editor.*", "ui.lang") && !key_matches("ui.lang", "ui.lang2"));
        let mut reg = Registry::builtin();
        // 내장 둘(Rainbow Pairs · Insert Sequence = 기본 켜짐 + 프롬프트 · 148차).
        assert_eq!(reg.ids().len(), 2);
        assert_eq!(
            reg.default_on_ids(),
            vec![("insert-sequence".to_string(), "Insert Sequence".to_string())]
        );
        assert!(reg.has_prompt("edit.insert_sequence") && !reg.has_prompt("edit.other"));
        assert!(reg
            .prompt_owner("edit.insert_sequence", &["insert-sequence".to_string()])
            .is_none());
        let (owner_id, prompt) = reg
            .prompt_owner("edit.insert_sequence", &[])
            .expect("prompt");
        assert_eq!(
            (owner_id.as_str(), prompt.placeholder.as_str()),
            ("insert-sequence", "1+1")
        );
        let notes = reg.sync_wasm(&[("rainbow-pairs".into(), path.clone())]);
        assert!(notes.iter().any(|n| n.contains("loaded")), "{notes:?}");
        // 같은 id 하나 · 명령 소유자 = WASM 쪽.
        let mut ids = reg.ids();
        ids.sort();
        assert_eq!(
            ids,
            vec![
                ("insert-sequence".to_string(), "Insert Sequence".to_string()),
                ("rainbow-pairs".to_string(), "Rainbow Pairs".to_string()),
            ]
        );
        assert_eq!(reg.wasm_ids(), vec!["rainbow-pairs".to_string()]);
        let owner = reg.owner_of("edit.bracket_next").expect("owner");
        assert!(reg.extensions[owner].is_wasm());
        // 같은 목록으로 다시 = 변화 0.
        assert!(reg.sync_wasm(&[("rainbow-pairs".into(), path)]).is_empty());
        // 삭제 → 내장 복귀.
        let notes = reg.sync_wasm(&[]);
        assert!(notes.iter().any(|n| n.contains("unloaded")));
        assert!(reg.wasm_ids().is_empty());
        let owner = reg.owner_of("edit.bracket_next").expect("builtin owner");
        assert!(!reg.extensions[owner].is_wasm());
        manager::remove_from("rainbow-pairs", &root, &config).expect("remove");
        assert!(manager::installed_in(&root).is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// ★ 포맷터 확장(ABI v1.1 · docs/95): 공식 패키지 폴더의 `sql_formatter_kiros33.wasm`을 로드 → 포맷터로 등록 →
    /// `format_with`가 공통 옵션 + 접두 설정을 넘겨 탭 정렬 결과를 받는다.
    #[test]
    fn wasm_formatter_extension_formats_through_registry() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../extensions/sql-formatter-kiros33/sql_formatter_kiros33.wasm");
        let mut reg = Registry::builtin();
        let notes = reg.sync_wasm(&[("sql-formatter-kiros33".into(), path)]);
        assert!(notes.iter().any(|n| n.contains("loaded")), "{notes:?}");
        let fmts = reg.formatters(&[]);
        assert_eq!(fmts.len(), 1, "{fmts:?}");
        assert_eq!(fmts[0].0, "sql-formatter-kiros33");
        assert!(fmts[0].1.contains("kiros33"));
        assert!(!fmts[0].2.is_empty(), "예시 SQL");
        let settings = Settings::from_text(PathBuf::from("x"), "");
        let options = vec![("format.comma".to_string(), "leading".to_string())];
        let out = reg
            .format_with(
                "sql-formatter-kiros33",
                "select a.x as x1, a.long_column_name as y from t a where a.k = 1 and a.z > 2",
                "oracle",
                &options,
                &settings,
                false,
            )
            .expect("format");
        // 정렬 열 = 가장 긴 항목(`\ta.long_column_name` = 22칸) 다음 탭 스톱(24) · 짧은 항목은 탭을 더 넣어 맞춘다.
        assert!(
            out.contains("\ta.x\t\t\t\t\tAS\tx1\n,\ta.long_column_name\tAS\ty\n"),
            "{out}"
        );
        assert!(
            out.contains("WHERE 1=1\nAND\ta.k\t=\t1\nAND\ta.z\t>\t2\n"),
            "{out}"
        );
        // 끈 확장은 목록에서 빠진다.
        assert!(reg
            .formatters(&["sql-formatter-kiros33".to_string()])
            .is_empty());
        // 포맷터가 없는 id는 오류.
        assert!(reg
            .format_with("rainbow-pairs", "select 1", "", &[], &settings, false)
            .is_err());
    }
}
