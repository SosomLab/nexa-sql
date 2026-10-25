//! ★ **치환 변수 저장소 `MacroStore`**(T-153 ④ · docs/63 §7 "MacroStore — 바인드와 섞지 않는다") — `&이름` · `DEFINE` · `:setvar`/`$(v)` ·
//! 스크립트 인자 `&1..&n`이 사는 곳. [`crate::vars::VarStore`](바인드)와 **별개**(이름이 같아도 서로 가로채지 않는다 · 패널에서는 "종류" 열).
//!
//! 층(바인드 변수 D-135와 같은 셋 · 10-26): **탭(local) → 연결 공유(shared) → 글로벌(global)** 순으로 찾는다 · `DEFINE`은 탭 층에 쓴다 ·
//! `UNDEFINE`은 탭 → 공유 순으로 지운다(글로벌은 호스트가 통째로 바꾼다). 시스템 변수(`_USER` …)는 여기 들어오지 않는다(`Engine::sysvars`).
//!
//! 표면은 `BTreeMap<String, String>`과 호환(`insert` · `get` · `remove` · `iter` · `len` · `contains_key` · `FromIterator`)이라 종전 호출처가 그대로 돈다.

use std::collections::BTreeMap;

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
        self.local.remove(name).or_else(|| self.shared.remove(name))
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
