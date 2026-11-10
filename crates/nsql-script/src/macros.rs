//! ★ **치환 변수 저장소 `MacroStore`**(T-153 ④ · docs/63 §7 "MacroStore — 바인드와 섞지 않는다") — `&이름` · `DEFINE` · `:setvar`/`$(v)` ·
//! 스크립트 인자 `&1..&n`이 사는 곳. [`crate::vars::VarStore`](바인드)와 **별개**(이름이 같아도 서로 가로채지 않는다 · 패널에서는 "종류" 열).
//!
//! 층(바인드 변수 D-135와 같은 셋 · 10-26): **탭(local) → 연결 공유(shared) → 글로벌(global)** 순으로 찾는다 · `DEFINE`은 탭 층에 쓴다 ·
//! `UNDEFINE`은 탭 → 공유 순으로 지운다(글로벌은 호스트가 통째로 바꾼다). 시스템 변수(`_USER` …)는 여기 들어오지 않는다(`Engine::sysvars`).
//!
//! 표면은 `BTreeMap<String, String>`과 호환(`insert` · `get` · `remove` · `iter` · `len` · `contains_key` · `FromIterator`)이라 종전 호출처가 그대로 돈다.

use std::collections::{BTreeMap, BTreeSet};

/// 비밀 치환 변수의 표시값(T-132 ① · D-140) — 바인드 변수 가림과 같은 글.
pub const SECRET_MASK: &str = "******";

/// 치환 변수가 사는 층.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MacroLayer {
    Local,
    Shared,
    Global,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MacroStore {
    local: BTreeMap<String, String>,
    shared: BTreeMap<String, String>,
    global: BTreeMap<String, String>,
    /// ★ T-132 ①(11-10): `ACCEPT … HIDE`로 받은 이름(대문자) — 이름 규칙(`looks_secret`)에 안 걸려도 비밀로 본다. 층과 무관 ·
    ///   `remove`로 지우면 같이 빠진다 · `clear`(탭 층 비움)에는 남겨 둔다(이름뿐이라 무해 · 다음 실행에 호스트가 같은 이름을 되돌려 준다).
    hidden: BTreeSet<String>,
}

impl MacroStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 탭 층에 정의(종전 `defines.insert`).
    pub fn insert(&mut self, name: String, value: String) -> Option<String> {
        self.local.insert(name, value)
    }

    /// 지정한 층에 정의.
    pub fn define_in(&mut self, layer: MacroLayer, name: &str, value: &str) {
        self.layer_mut(layer)
            .insert(name.to_ascii_uppercase(), value.to_string());
    }

    /// ★ 비밀로 정의(`ACCEPT … HIDE` · T-132 ①) — 값은 보통처럼 들어가되 이름이 비밀 집합에 올라 모든 출구에서 가려진다.
    pub fn define_secret(&mut self, layer: MacroLayer, name: &str, value: &str) {
        self.hidden.insert(name.to_ascii_uppercase());
        self.define_in(layer, name, value);
    }

    /// 비밀인가 = `ACCEPT HIDE`로 받았거나 이름 규칙(`*PASS*` `*PWD*` `*SECRET*` `*TOKEN*` · D-140).
    #[must_use]
    pub fn is_secret(&self, name: &str) -> bool {
        self.hidden.contains(&name.to_ascii_uppercase()) || crate::vars::looks_secret(name)
    }

    /// 찾기 = 탭 → 공유 → 글로벌.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&String> {
        self.local
            .get(name)
            .or_else(|| self.shared.get(name))
            .or_else(|| self.global.get(name))
    }

    /// 어느 층에서 보이는가(없으면 None).
    #[must_use]
    pub fn layer_of(&self, name: &str) -> Option<MacroLayer> {
        if self.local.contains_key(name) {
            Some(MacroLayer::Local)
        } else if self.shared.contains_key(name) {
            Some(MacroLayer::Shared)
        } else if self.global.contains_key(name) {
            Some(MacroLayer::Global)
        } else {
            None
        }
    }

    #[must_use]
    pub fn contains_key(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// 지우기(`UNDEFINE`) = 탭 → 없으면 공유(글로벌은 안 지운다 · 지운 값이 있으면 돌려준다).
    pub fn remove(&mut self, name: &str) -> Option<String> {
        let r = self.local.remove(name).or_else(|| self.shared.remove(name));
        if r.is_some() {
            self.hidden.remove(&name.to_ascii_uppercase());
        }
        r
    }

    /// 탭 층만 비운다(스크립트 끝 · 탭 바꿈).
    pub fn clear(&mut self) {
        self.local.clear();
    }

    /// **보이는** 이름·값(가려진 층은 빠짐 · 이름순) — 종전 `defines.iter()`.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.merged().into_iter()
    }

    /// 보이는 이름들(이름순 · 종전 `defines.keys()`).
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.merged().into_keys()
    }

    /// 보이는 이름 수.
    #[must_use]
    pub fn len(&self) -> usize {
        self.merged().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.local.is_empty() && self.shared.is_empty() && self.global.is_empty()
    }

    /// 층별 원본(호스트 보존·표시용).
    #[must_use]
    pub fn layer(&self, layer: MacroLayer) -> &BTreeMap<String, String> {
        match layer {
            MacroLayer::Local => &self.local,
            MacroLayer::Shared => &self.shared,
            MacroLayer::Global => &self.global,
        }
    }

    /// 층을 통째로 바꾼다(호스트: 탭 전환 = Local · 접속 = Shared · 시작 = Global).
    pub fn set_layer(&mut self, layer: MacroLayer, map: BTreeMap<String, String>) {
        *self.layer_mut(layer) = map;
    }

    /// ★ 이름을 다른 층으로 옮긴다(`DEFINE 이름 SHARE|LOCAL|GLOBAL` · T-153 후속 · 10-26 139차 win) — 모든 층에서 빼고 목적 층에 넣는다
    /// (같은 이름이 여러 층에 있으면 옮기는 값 = 보이던 값 = 탭 → 공유 → 글로벌 순 첫 것). 어느 층에도 없으면 `false`.
    pub fn move_to(&mut self, name: &str, layer: MacroLayer) -> bool {
        let key = name.to_ascii_uppercase();
        let mut value: Option<String> = None;
        for l in [MacroLayer::Local, MacroLayer::Shared, MacroLayer::Global] {
            if let Some(v) = self.layer_mut(l).remove(&key) {
                value.get_or_insert(v);
            }
        }
        match value {
            Some(v) => {
                self.layer_mut(layer).insert(key, v);
                true
            }
            None => false,
        }
    }

    fn layer_mut(&mut self, layer: MacroLayer) -> &mut BTreeMap<String, String> {
        match layer {
            MacroLayer::Local => &mut self.local,
            MacroLayer::Shared => &mut self.shared,
            MacroLayer::Global => &mut self.global,
        }
    }

    fn merged(&self) -> BTreeMap<&String, &String> {
        let mut m: BTreeMap<&String, &String> = BTreeMap::new();
        for (k, v) in &self.global {
            m.insert(k, v);
        }
        for (k, v) in &self.shared {
            m.insert(k, v);
        }
        for (k, v) in &self.local {
            m.insert(k, v);
        }
        m
    }
}

/// 종전 호출처(`d.into_iter().collect()`)는 탭 층으로 들어온다.
impl FromIterator<(String, String)> for MacroStore {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> Self {
        MacroStore {
            local: iter.into_iter().collect(),
            ..MacroStore::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ T-132 ①: HIDE로 받은 이름과 이름 규칙 이름은 비밀 · 지우면 풀림 · 엔진 표시글은 `******`.
    #[test]
    fn secret_names_by_hide_and_rule_are_masked() {
        let mut m = MacroStore::new();
        m.define_secret(MacroLayer::Local, "x", "hunter2");
        m.define_in(MacroLayer::Local, "DB_PASS", "tiger");
        m.define_in(MacroLayer::Local, "OWNER", "scott");
        assert!(
            m.is_secret("X") && m.is_secret("x"),
            "HIDE = 비밀(대소문자 무관)"
        );
        assert!(m.is_secret("DB_PASS"), "이름 규칙 = 비밀");
        assert!(!m.is_secret("OWNER"));
        assert_eq!(
            m.get("X").map(String::as_str),
            Some("hunter2"),
            "값 자체는 그대로(치환에 쓴다)"
        );
        assert!(m.remove("X").is_some());
        assert!(!m.is_secret("X"), "지우면 비밀 표시도 풀린다");
        let mut e = crate::engine::Engine::new(nsql_core::Dialect::Oracle);
        e.define_secret("tok", "abc");
        e.define("DB_PWD", "p");
        e.define("OWNER", "scott");
        assert_eq!(e.define_display("TOK", "abc"), SECRET_MASK);
        assert_eq!(e.define_display("DB_PWD", "p"), SECRET_MASK);
        assert_eq!(e.define_display("OWNER", "scott"), "scott");
    }

    #[test]
    fn layers_shadow_and_remove_order() {
        let mut m = MacroStore::new();
        m.define_in(MacroLayer::Global, "env", "dev");
        m.define_in(MacroLayer::Shared, "ENV", "stage");
        assert_eq!(m.get("ENV").map(String::as_str), Some("stage"));
        assert_eq!(m.layer_of("ENV"), Some(MacroLayer::Shared));
        m.insert("ENV".into(), "prod".into());
        assert_eq!(m.get("ENV").map(String::as_str), Some("prod"));
        assert_eq!(m.len(), 1, "같은 이름은 보이는 것 하나");
        // UNDEFINE = 탭 → 공유 · 글로벌은 남는다.
        assert_eq!(m.remove("ENV").as_deref(), Some("prod"));
        assert_eq!(m.get("ENV").map(String::as_str), Some("stage"));
        assert_eq!(m.remove("ENV").as_deref(), Some("stage"));
        assert_eq!(m.get("ENV").map(String::as_str), Some("dev"));
        assert_eq!(m.remove("ENV"), None);
        assert!(m.contains_key("ENV"));
        // 탭 층만 비움 · 종전 collect 호환.
        m.insert("X".into(), "1".into());
        m.clear();
        assert!(!m.contains_key("X") && m.contains_key("ENV"));
        let c: MacroStore = vec![("A".to_string(), "1".to_string())]
            .into_iter()
            .collect();
        assert_eq!(c.layer(MacroLayer::Local).len(), 1);
        let names: Vec<&String> = c.iter().map(|(k, _)| k).collect();
        assert_eq!(names, vec!["A"]);
    }
}

#[cfg(test)]
mod move_tests {
    use super::*;

    /// T-153 후속(10-26): 층 이동 = 모든 층에서 빼고 목적 층에 · 보이던 값이 따라간다 · 없는 이름 = false.
    #[test]
    fn move_to_crosses_layers_and_keeps_visible_value() {
        let mut m = MacroStore::new();
        m.define_in(MacroLayer::Global, "x", "g");
        m.define_in(MacroLayer::Local, "x", "l");
        assert_eq!(m.get("X").map(String::as_str), Some("l"));
        assert!(m.move_to("x", MacroLayer::Shared));
        assert_eq!(m.layer(MacroLayer::Local).len(), 0);
        assert_eq!(
            m.layer(MacroLayer::Global).len(),
            0,
            "글로벌의 같은 이름도 비운다"
        );
        assert_eq!(m.layer_of("X"), Some(MacroLayer::Shared));
        assert_eq!(m.get("X").map(String::as_str), Some("l"));
        assert!(!m.move_to("nope", MacroLayer::Global));
        assert!(m.move_to("X", MacroLayer::Local));
        assert_eq!(m.layer_of("X"), Some(MacroLayer::Local));
    }
}
