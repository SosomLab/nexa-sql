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
}

impl Command {
    pub(crate) fn msg(id: &str, label: nsql_i18n::Msg) -> Command {
        Command {
            id: id.to_string(),
            label: Label::Msg(label),
        }
    }
}

/// 메뉴 기여 — 우클릭 편집 메뉴에 붙는 서브메뉴(라벨 + 항목 id/라벨/단축키 표시).
#[derive(Clone, Debug)]
pub(crate) struct MenuContribution {
    pub(crate) id: String,
    pub(crate) label: Label,
    pub(crate) items: Vec<Command>,
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
    /// 내려받은 WASM 모듈인가(내장 = false).
    fn is_wasm(&self) -> bool {
        false
    }
    /// WASM 확장이면 자신(로그 수거용 다운캐스트).
    fn as_wasm(&self) -> Option<&wasm::WasmExtension> {
        None
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
    /// 내장 확장(첫 = Rainbow Pairs).
    #[must_use]
    pub(crate) fn builtin() -> Self {
        Registry {
            extensions: vec![Box::new(rainbow_pairs::RainbowPairs)],
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

    /// 로드된 WASM 확장 id 목록.
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
                            "wasm extension {}: module says id {} — not loaded",
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
                    "wasm extension {id}: load failed — {e} (builtin fallback if any)"
                )),
            }
        }
        notes
    }

    /// 우클릭 편집 메뉴 추가 항목(켜진 확장만 · 서브메뉴로 · `display` = 단축키 표시 조회).
    pub(crate) fn menu_extras(
        &self,
        disabled: &[String],
        display: &dyn Fn(&str) -> String,
    ) -> Vec<CtxItem> {
        let mut out = Vec::new();
        for i in self.active_indices() {
            let p = &self.extensions[i];
            if disabled.iter().any(|d| d == p.id()) {
                continue;
            }
            for m in p.menus() {
                let items: Vec<CtxItem> = m
                    .items
                    .iter()
                    .map(|c| {
                        CtxItem::item(c.id.clone(), c.label.text()).with_shortcut(display(&c.id))
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
            if !changed_key.is_none_or(|k| k.starts_with(p.settings_prefix())) {
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
    pub(crate) fn run(&mut self, cmd: &str, disabled: &[String], ed: &mut dyn EditorOps) -> bool {
        let Some(i) = self.owner_of(cmd) else {
            return false;
        };
        if disabled.iter().any(|d| d == self.extensions[i].id()) {
            return false;
        }
        self.extensions[i].run(cmd, ed)
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

        let mut reg = Registry::builtin();
        assert_eq!(reg.ids().len(), 1);
        let notes = reg.sync_wasm(&[("rainbow-pairs".into(), path.clone())]);
        assert!(notes.iter().any(|n| n.contains("loaded")), "{notes:?}");
        // 같은 id 하나 · 명령 소유자 = WASM 쪽.
        assert_eq!(
            reg.ids(),
            vec![("rainbow-pairs".to_string(), "Rainbow Pairs".to_string())]
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
}
