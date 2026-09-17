//! **확장(extension) 호스트 API**(docs/50 §4·§7 · docs/51 §5·§9 — D-91 "코어 + in-process 확장 층").
//!
//! in-process 확장(이 저장소 · Rust)과 나중의 WASM/프로세스 확장이 **같은 표면**을 본다:
//! 명령 · 우클릭/편집 메뉴 기여 · 설정 → 편집기 옵션 · 편집기 이동. 편집기 내부(쌍 표 · 데코레이션 계산)는
//! nexa-ctl 코어에 있고, 확장은 "무엇을 켜고 어떤 명령/메뉴를 두는가"만 정한다.
//!
//! 등록 = [`Registry::builtin`] · 호스트는 [`Plugin`] 트레이트만 안다(30 §1 포트+레지스트리+설정).

use nexa_ctl::controls::ctxmenu::CtxItem;
use nsql_settings::Settings;

pub(crate) mod manager;
pub(crate) mod rainbow_pairs;
pub(crate) mod sha256;

/// 확장이 호스트에 내는 명령 하나(팔레트·키맵·메뉴 id는 같은 문자열).
#[derive(Clone, Debug)]
pub(crate) struct Command {
    pub(crate) id: &'static str,
    pub(crate) label: nsql_i18n::Msg,
}

/// 메뉴 기여 — 우클릭 편집 메뉴에 붙는 서브메뉴(라벨 + 항목 id/라벨/단축키 표시).
#[derive(Clone, Debug)]
pub(crate) struct MenuContribution {
    pub(crate) id: &'static str,
    pub(crate) label: nsql_i18n::Msg,
    pub(crate) items: Vec<Command>,
}

/// 확장 포트 — 호스트는 이 트레이트로만 부른다(WASM은 같은 이름의 export).
pub(crate) trait Extension {
    fn id(&self) -> &'static str;
    /// 설정 키 접두(`rainbow.`) — 이 접두의 키가 바뀌면 [`Extension::on_settings`]가 불린다.
    fn settings_prefix(&self) -> &'static str;
    fn commands(&self) -> Vec<Command>;
    fn menus(&self) -> Vec<MenuContribution>;
    /// 설정이 바뀜(또는 시작) → 호스트가 적용할 편집기 옵션을 돌려준다.
    fn on_settings(&mut self, settings: &Settings) -> ExtensionEffect;
    /// 사용자가 껐을 때(Extension Manager: Disable) 적용할 효과 — 기본 = 아무것도 안 함.
    fn disabled_effect(&self) -> ExtensionEffect {
        ExtensionEffect::default()
    }
    /// 사람이 읽는 이름(팔레트 목록).
    fn name(&self) -> &'static str;
    /// 명령 실행 — 편집기 조작은 [`EditorOps`]로.
    fn run(&mut self, id: &str, ed: &mut dyn EditorOps) -> bool;
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

/// 레지스트리 — in-process 확장 목록(나중에 WASM 인스턴스도 같은 `Box<dyn Extension>`으로).
pub(crate) struct Registry {
    pub(crate) extensions: Vec<Box<dyn Extension>>,
}

impl Registry {
    /// 내장 확장(첫 = Rainbow Pairs).
    #[must_use]
    pub(crate) fn builtin() -> Self {
        Registry {
            extensions: vec![Box::new(rainbow_pairs::RainbowPairs)],
        }
    }

    /// 어느 확장의 명령인가.
    pub(crate) fn owner_of(&self, cmd: &str) -> Option<usize> {
        self.extensions
            .iter()
            .position(|p| p.commands().iter().any(|c| c.id == cmd))
    }

    /// 내장 확장 (id, 이름) 목록 — 매니저 목록에 `builtin`으로 보인다.
    pub(crate) fn builtin_ids(&self) -> Vec<(&'static str, &'static str)> {
        self.extensions.iter().map(|p| (p.id(), p.name())).collect()
    }

    /// 우클릭 편집 메뉴 추가 항목(켜진 확장만 · 서브메뉴로 · `display` = 단축키 표시 조회).
    pub(crate) fn menu_extras(
        &self,
        disabled: &[String],
        display: &dyn Fn(&str) -> String,
    ) -> Vec<CtxItem> {
        let mut out = Vec::new();
        for p in self
            .extensions
            .iter()
            .filter(|p| !disabled.iter().any(|d| d == p.id()))
        {
            for m in p.menus() {
                let items: Vec<CtxItem> = m
                    .items
                    .iter()
                    .map(|c| {
                        CtxItem::item(c.id, nsql_i18n::t(c.label)).with_shortcut(display(c.id))
                    })
                    .collect();
                out.push(CtxItem::submenu(m.id, nsql_i18n::t(m.label), items));
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
        self.extensions
            .iter_mut()
            .filter(|p| changed_key.is_none_or(|k| k.starts_with(p.settings_prefix())))
            .map(|p| {
                if disabled.iter().any(|d| d == p.id()) {
                    p.disabled_effect()
                } else {
                    p.on_settings(settings)
                }
            })
            .collect()
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
}
