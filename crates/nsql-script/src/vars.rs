//! 세션 변수 저장소 — SQL*Plus 바인드 테이블의 클라이언트 측 복제(docs/04 §8.2).
//! 이름은 대소문자 무관(대문자 정규화 · 처음 쓴 표기는 `label`로 보존 · D-142). 값은 실행 사이에 **여기에만** 산다.
//!
//! ★ **세 층**(D-135 · docs/63 §7): 찾는 순서 = **탭(local) → 연결 공유(shared) → 프로필(fixed · 읽기 전용)**.
//! - 새 변수는 늘 탭 층에 생긴다(다른 탭의 실행이 내 값을 바꾸지 않는다 — Golden의 탭별 목록).
//! - 일부러 올린 이름(`VAR x SHARE` · 패널)만 공유 층에 살고, 그 이름에 대한 대입은 공유 층을 고친다(같은 연결의 탭들이 같이 본다).
//! - 프로필 층은 고치지 않는다 — 같은 이름에 대입하면 탭 층에 가리는 값이 생긴다.
//!
//! 호스트는 실행마다 탭 층만 갈아 끼운다([`VarStore::set_local`]/[`VarStore::local_states`]) · 공유 층은 러너(= 세션)와 함께 산다.
//! 바뀐 이름은 [`VarStore::take_dirty`]로 한 번에 걷는다(패널·로그는 사건이 올 때만 그린다 · docs/63 §4).

use nsql_core::{Value, VarType};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, PartialEq, Debug)]
pub struct Var {
    pub ty: VarType,
    pub value: Value,
    /// `VARIABLE`로 명시 선언했는가(아니면 대입으로 암묵 생성).
    pub declared: bool,
    /// 처음 쓴 표기(콜론 없음) — 보여 줄 때만 쓴다(찾기는 대문자 키).
    pub label: String,
    /// 비밀 값(D-140) — 패널·로그에서 가리고 저장하지 않는다.
    pub secret: bool,
}

/// 변수가 사는 층.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    /// 탭(기본).
    Local,
    /// 연결 공유.
    Shared,
    /// 프로필(읽기 전용).
    Fixed,
}

/// 층을 넘나드는 변수 한 줄(호스트 ↔ 러너 · 패널 · 보존).
#[derive(Clone, PartialEq, Debug)]
pub struct VarState {
    /// 보여 줄 이름(처음 쓴 표기).
    pub name: String,
    pub ty: VarType,
    pub value: Value,
    pub declared: bool,
    pub secret: bool,
    pub layer: Layer,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct VarStore {
    vars: BTreeMap<String, Var>,
    shared: BTreeMap<String, Var>,
    fixed: BTreeMap<String, Var>,
    /// 마지막 [`VarStore::take_dirty`] 뒤로 바뀐(생김·값·타입·층·사라짐) 이름(대문자 키).
    dirty: BTreeSet<String>,
}

pub fn norm(name: &str) -> String {
    name.trim().trim_start_matches(':').to_ascii_uppercase()
}

fn label_of(name: &str) -> String {
    name.trim().trim_start_matches(':').to_string()
}

/// 이름만 보고 비밀로 볼 것인가(D-140 · `*PASS*` `*PWD*` `*SECRET*` `*TOKEN*`).
#[must_use]
pub fn looks_secret(name: &str) -> bool {
    let up = name.to_ascii_uppercase();
    ["PASS", "PWD", "SECRET", "TOKEN"]
        .iter()
        .any(|k| up.contains(k))
}

impl VarStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn fresh(name: &str, ty: VarType, value: Value, declared: bool) -> Var {
        Var {
            ty,
            value,
            declared,
            label: label_of(name),
            secret: looks_secret(name),
        }
    }

    /// 고칠 수 있는 자리(탭 → 공유) — 프로필 층은 고치지 않는다.
    fn slot_mut(&mut self, key: &str) -> Option<&mut Var> {
        if self.vars.contains_key(key) {
            return self.vars.get_mut(key);
        }
        self.shared.get_mut(key)
    }

    /// `VARIABLE name type [= value]` — 공유 층에 있는 이름이면 그 자리에서 다시 선언한다.
    pub fn declare(&mut self, name: &str, ty: VarType, init: Option<Value>) {
        let key = norm(name);
        let mut var = Self::fresh(name, ty, init.unwrap_or(Value::Null), true);
        if let Some(old) = self.slot_mut(&key) {
            var.label = std::mem::take(&mut old.label);
            var.secret = old.secret;
        }
        if self.shared.contains_key(&key) && !self.vars.contains_key(&key) {
            self.shared.insert(key.clone(), var);
        } else {
            self.vars.insert(key.clone(), var);
        }
        self.dirty.insert(key);
    }

    /// 대입 — 없으면 [`VarType::Auto`]로 암묵 생성(Golden 관용 · docs/08 §3-2).
    /// 선언된 타입이 있으면 유지하고, `Auto`면 값에서 따라간다.
    pub fn assign(&mut self, name: &str, value: Value) {
        let key = norm(name);
        match self.slot_mut(&key) {
            Some(v) => {
                if v.ty == VarType::Auto {
                    v.ty = VarType::infer(&value);
                }
                if v.value != value {
                    v.value = value;
                    self.dirty.insert(key);
                }
            }
            None => {
                let ty = VarType::infer(&value);
                // 프로필 층의 이름이면 타입·비밀 표시를 물려받아 탭 층에 가리는 값을 만든다.
                let mut var = Self::fresh(name, ty, value, false);
                if let Some(f) = self.fixed.get(&key) {
                    var.secret = f.secret;
                    if f.ty != VarType::Auto {
                        var.ty = f.ty.clone();
                    }
                }
                self.vars.insert(key.clone(), var);
                self.dirty.insert(key);
            }
        }
    }

    /// 타입 힌트(루틴 서명에서 읽은 타입) — **선언하지 않았고 아직 타입이 정해지지 않은**(`Auto`) 변수에만 적용하고,
    /// 없으면 `NULL` 값으로 만든다(선언 아님 = `declared: false`). 돌려주는 값 = 적용했는가.
    pub fn hint_type(&mut self, name: &str, ty: VarType) -> bool {
        let key = norm(name);
        match self.slot_mut(&key) {
            Some(v) if v.declared || v.ty != VarType::Auto => false,
            Some(v) => {
                v.ty = ty;
                self.dirty.insert(key);
                true
            }
            None => {
                self.vars
                    .insert(key.clone(), Self::fresh(name, ty, Value::Null, false));
                self.dirty.insert(key);
                true
            }
        }
    }

    /// 서명 추론이 필요한가 — 없는 이름이거나, 선언하지 않았고 타입이 `Auto`이며 값이 없는 변수.
    #[must_use]
    pub fn needs_type(&self, name: &str) -> bool {
        match self.get(name) {
            None => true,
            Some(v) => !v.declared && v.ty == VarType::Auto && v.value == Value::Null,
        }
    }

    /// 탭 → 공유 → 프로필 순으로 찾는다.
    pub fn get(&self, name: &str) -> Option<&Var> {
        let key = norm(name);
        self.vars
            .get(&key)
            .or_else(|| self.shared.get(&key))
            .or_else(|| self.fixed.get(&key))
    }

    /// 그 이름이 사는 층(가장 앞선 것).
    #[must_use]
    pub fn layer_of(&self, name: &str) -> Option<Layer> {
        let key = norm(name);
        if self.vars.contains_key(&key) {
            Some(Layer::Local)
        } else if self.shared.contains_key(&key) {
            Some(Layer::Shared)
        } else if self.fixed.contains_key(&key) {
            Some(Layer::Fixed)
        } else {
            None
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }

    /// 지운다(탭 → 공유 · 프로필 층은 남는다).
    pub fn remove(&mut self, name: &str) -> Option<Var> {
        let key = norm(name);
        let gone = self.vars.remove(&key).or_else(|| self.shared.remove(&key));
        if gone.is_some() {
            self.dirty.insert(key);
        }
        gone
    }

    /// 보이는 변수 전부(앞선 층이 가린다 · 이름순).
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Var)> {
        let mut seen: BTreeMap<&String, &Var> = BTreeMap::new();
        for (k, v) in self.fixed.iter().chain(&self.shared).chain(&self.vars) {
            seen.insert(k, v);
        }
        seen.into_iter()
    }

    /// 보이는 변수 수.
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty() && self.shared.is_empty() && self.fixed.is_empty()
    }

    /// 실행 결과의 OUT 값을 되돌려 받는다. 미지의 이름은 무시하지 않고 생성한다
    /// (드라이버가 `SELECT … INTO :NEW`로 새 변수를 만들 수 있게).
    pub fn absorb(&mut self, outs: &[(String, Value)]) {
        for (name, v) in outs {
            self.assign(name, v.clone());
        }
    }

    // ───────────── 층(D-135)

    /// 탭 층의 이름을 연결 공유 층으로 올린다(이미 공유면 그대로 참). 없는 이름 = 거짓.
    pub fn share(&mut self, name: &str) -> bool {
        let key = norm(name);
        if let Some(v) = self.vars.remove(&key) {
            self.shared.insert(key.clone(), v);
            self.dirty.insert(key);
            return true;
        }
        self.shared.contains_key(&key)
    }

    /// 공유 층의 이름을 탭 층으로 내린다.
    pub fn unshare(&mut self, name: &str) -> bool {
        let key = norm(name);
        if let Some(v) = self.shared.remove(&key) {
            self.vars.insert(key.clone(), v);
            self.dirty.insert(key);
            return true;
        }
        self.vars.contains_key(&key)
    }

    fn state((_, v): (&String, &Var), layer: Layer) -> VarState {
        VarState {
            name: v.label.clone(),
            ty: v.ty.clone(),
            value: v.value.clone(),
            declared: v.declared,
            secret: v.secret,
            layer,
        }
    }

    fn from_states(states: Vec<VarState>) -> BTreeMap<String, Var> {
        states
            .into_iter()
            .map(|s| {
                (
                    norm(&s.name),
                    Var {
                        ty: s.ty,
                        value: s.value,
                        declared: s.declared,
                        label: label_of(&s.name),
                        secret: s.secret,
                    },
                )
            })
            .collect()
    }

    /// 탭 층을 통째로 갈아 끼운다(호스트가 실행 앞에 · 바뀜 표시는 건드리지 않는다).
    pub fn set_local(&mut self, states: Vec<VarState>) {
        self.vars = Self::from_states(states);
    }

    /// 프로필 층을 정한다(접속 때).
    pub fn set_fixed(&mut self, states: Vec<VarState>) {
        self.fixed = Self::from_states(states);
    }

    /// 탭 층의 지금 상태.
    #[must_use]
    pub fn local_states(&self) -> Vec<VarState> {
        self.vars
            .iter()
            .map(|e| Self::state(e, Layer::Local))
            .collect()
    }

    /// 공유 층의 지금 상태.
    #[must_use]
    pub fn shared_states(&self) -> Vec<VarState> {
        self.shared
            .iter()
            .map(|e| Self::state(e, Layer::Shared))
            .collect()
    }

    /// 공유 층의 값 하나를 호스트가 고친다(패널 편집) — 없으면 만든다.
    pub fn put_shared(&mut self, s: VarState) {
        let key = norm(&s.name);
        self.shared.extend(Self::from_states(vec![s]));
        self.dirty.insert(key);
    }

    /// 마지막으로 걷은 뒤 바뀐 이름(보여 줄 표기 · 사라진 이름은 대문자 키)을 걷는다.
    pub fn take_dirty(&mut self) -> Vec<String> {
        let keys = std::mem::take(&mut self.dirty);
        keys.into_iter()
            .map(|k| self.get(&k).map_or(k.clone(), |v| v.label.clone()))
            .collect()
    }

    /// 세션에 묶인 값(REF CURSOR 핸들)을 전부 비운다 — 재접속·유휴 닫기 뒤에는 죽은 핸들이다(docs/63 §4).
    pub fn invalidate_cursors(&mut self) {
        for (k, v) in self.vars.iter_mut().chain(self.shared.iter_mut()) {
            if matches!(v.value, Value::Cursor(_)) {
                v.value = Value::Null;
                self.dirty.insert(k.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declare_assign_infer() {
        let mut s = VarStore::new();
        s.declare("v_num", VarType::Number, None);
        assert_eq!(s.get(":V_NUM").unwrap().value, Value::Null);
        s.assign("V_NUM", Value::Int(3));
        assert_eq!(s.get("v_num").unwrap().ty, VarType::Number);
        s.assign(":v_prg_nm", Value::Str("SP_X".into()));
        let v = s.get("V_PRG_NM").unwrap();
        assert!(!v.declared);
        assert_eq!(v.ty, VarType::Varchar2(4));
        assert_eq!(v.label, "v_prg_nm", "처음 쓴 표기 보존(D-142)");
        assert_eq!(s.len(), 2);
    }

    /// D-135 세 층: 찾기 = 탭 → 공유 → 프로필 · 새 변수 = 탭 · 공유 이름에 대한 대입 = 공유 층 · 프로필 이름 = 가리는 값.
    #[test]
    fn layers_lookup_assign_share() {
        let mut s = VarStore::new();
        s.set_fixed(vec![VarState {
            name: "ENV".into(),
            ty: VarType::Varchar2(4),
            value: Value::Str("PROD".into()),
            declared: true,
            secret: false,
            layer: Layer::Fixed,
        }]);
        s.assign("a", Value::Int(1));
        assert_eq!(s.layer_of("A"), Some(Layer::Local));
        assert!(s.share("a"));
        assert_eq!(s.layer_of("a"), Some(Layer::Shared));
        // 탭 층을 갈아 끼워도(다른 탭의 실행) 공유 값은 보인다 · 대입은 공유 층을 고친다.
        s.set_local(Vec::new());
        assert_eq!(s.get("A").unwrap().value, Value::Int(1));
        s.assign("A", Value::Int(2));
        assert_eq!(s.layer_of("A"), Some(Layer::Shared));
        assert_eq!(s.shared_states()[0].value, Value::Int(2));
        assert!(s.local_states().is_empty());
        // 프로필 값은 보이고, 대입하면 탭 층에 가리는 값이 생긴다(프로필은 그대로).
        assert_eq!(s.get("env").unwrap().value, Value::Str("PROD".into()));
        s.assign("env", Value::Str("DEV".into()));
        assert_eq!(s.layer_of("ENV"), Some(Layer::Local));
        assert_eq!(s.get("ENV").unwrap().value, Value::Str("DEV".into()));
        s.remove("ENV");
        assert_eq!(s.get("ENV").unwrap().value, Value::Str("PROD".into()));
        assert!(s.unshare("A"));
        assert_eq!(s.layer_of("A"), Some(Layer::Local));
        assert_eq!(s.len(), 2);
    }

    /// 바뀐 이름만 걷힌다(같은 값 대입 = 바뀜 아님) · 커서 무효화 · 비밀 이름 규칙.
    #[test]
    fn dirty_cursor_secret() {
        let mut s = VarStore::new();
        s.assign("x", Value::Int(1));
        s.assign("rc", Value::Cursor(nsql_core::CursorId(7)));
        assert_eq!(s.take_dirty().len(), 2);
        s.assign("x", Value::Int(1));
        assert!(s.take_dirty().is_empty(), "같은 값 = 바뀜 아님");
        s.invalidate_cursors();
        assert_eq!(s.get("RC").unwrap().value, Value::Null);
        assert_eq!(s.take_dirty(), vec!["rc".to_string()]);
        s.assign("v_db_password", Value::Str("p".into()));
        assert!(s.get("V_DB_PASSWORD").unwrap().secret);
        assert!(!s.get("X").unwrap().secret);
    }
}
