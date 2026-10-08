//! 세션 변수 저장소 — SQL*Plus 바인드 테이블의 클라이언트 측 복제(docs/04 §8.2).
//! 이름은 대소문자 무관(대문자 정규화 · 처음 쓴 표기는 `label`로 보존 · D-142). 값은 실행 사이에 **여기에만** 산다.
//!
//! ★ **네 층**(D-135 · docs/63 §7 · §11 글로벌 09-25): 찾는 순서 = **탭(local) → 연결 공유(shared) → 글로벌(global · 앱 전역 ·
//! 디스크 보존) → 프로필(fixed · 읽기 전용)**. 글로벌은 "기본값" 성격 — 탭/연결의 대입은 글로벌을 고치지 않고 앞 층에 값을 만든다 ·
//! `VAR x GLOBAL`·변수 창으로 올린 이름만 글로벌에 산다.
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
    /// ★ 타입이 **값을 따라가는** 변수인가(선언 없이 생겼거나 타입 없이 선언 `VAR x` · `VAR x GLOBAL`) — 대입마다 지금 값에서 다시 추론
    /// (사용자 10-07 "`'Number to String'`이 NUMBER처럼 보임" = 첫 값에서 한 번만 추론하던 결함). 타입을 적어 선언한 변수는 고정.
    pub auto_ty: bool,
}

/// 변수가 사는 층.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    /// 탭(기본).
    Local,
    /// 연결 공유.
    Shared,
    /// 앱 전역(모든 서버·세션·탭 · 디스크 보존 · docs/63 §11).
    Global,
    /// 프로필(읽기 전용).
    Fixed,
}

impl Layer {
    /// 표시·로그용 낱말(`tab` · `shared` · `global` · `profile`).
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Layer::Local => "tab",
            Layer::Shared => "shared",
            Layer::Global => "global",
            Layer::Fixed => "profile",
        }
    }
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
    global: BTreeMap<String, Var>,
    fixed: BTreeMap<String, Var>,
    /// 마지막 [`VarStore::take_dirty`] 뒤로 바뀐(생김·값·타입·층·사라짐) 이름(대문자 키).
    dirty: BTreeSet<String>,
    /// ★ 바인드 식의 **사용 시 재계산**(docs/63 §9 D-184 · 09-23): 이름 → 수식(`:V1 + 5`) · 의존 이름 · stale(의존이 바뀐 뒤 아직 재계산 전).
    formulas: BTreeMap<String, Formula>,
}

/// 사용 시 재계산할 바인드 식.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Formula {
    pub text: String,
    pub deps: Vec<String>,
    pub stale: bool,
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
            // 선언 없이 생긴 변수(값에서 추론한 타입으로 태어나도) · 타입 없이 선언한 변수 = 값을 따라간다(협업 bin54 ② = `!declared` 빠져 있었음).
            auto_ty: !declared || ty == VarType::Auto,
            ty,
            value,
            declared,
            label: label_of(name),
            secret: looks_secret(name),
        }
    }

    /// 고칠 수 있는 자리(탭 → 공유 → **글로벌**) — 프로필 층은 고치지 않는다.
    /// ★ D-264(사용자 10-07 두 번 확인 "`VAR x GLOBAL` 뒤 `SELECT … INTO :x` = 글로벌에 값"): 대입은 그 이름이 **사는 층**에 쓴다 · 어디에도
    /// 없으면 탭. 글로벌은 명시 선언(`VAR x GLOBAL` · `VAR x 타입 = 값 GLOBAL`)으로만 생기므로 서버 간 오염 방지(63 §11-3)는 그대로 ·
    /// 탭에서 **가리고** 싶으면 명시 선언(`VAR x 타입 [= 값]` = 늘 탭에 만든다). 종전(09-25) "대입 = 늘 탭 가림"은 폐기.
    fn slot_mut(&mut self, key: &str) -> Option<&mut Var> {
        if self.vars.contains_key(key) {
            return self.vars.get_mut(key);
        }
        if self.shared.contains_key(key) {
            return self.shared.get_mut(key);
        }
        self.global.get_mut(key)
    }

    /// `VARIABLE name type [= value]` — 공유 층에 있는 이름이면 그 자리에서 다시 선언한다.
    pub fn declare(&mut self, name: &str, ty: VarType, init: Option<Value>) {
        let key = norm(name);
        let mut var = Self::fresh(name, ty, init.unwrap_or(Value::Null), true);
        if let Some(old) = self.slot_mut(&key) {
            // 라벨은 **복제**(종전 `mem::take`는 가려지는 글로벌 줄의 이름을 비웠다 · 협업 bin54 "Name 빈칸").
            var.label = old.label.clone();
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
                // ★ 자동 타입 변수는 **지금 값**을 따른다(NULL은 타입을 안 바꾼다) · 선언 타입은 고정(사용자 10-07).
                if (v.auto_ty && value != Value::Null) || v.ty == VarType::Auto {
                    v.ty = VarType::infer(&value);
                }
                // ★ 값이 **실제로 바뀔 때만** 의존 수식을 stale로(사용자 09-27 Linux: 재계산 EXEC의 OUT 흡수가 안 바뀐 `:A`까지
                //   대입해 `:B` 수식을 매번 다시 stale로 만들어 재계획이 64회 돌았다 = "Substitution variable loop").
                let changed = v.value != value;
                if changed {
                    v.value = value;
                    self.dirty.insert(key.clone());
                }
                self.note_assigned(&key, changed);
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
                self.dirty.insert(key.clone());
                self.note_assigned(&key, true);
            }
        }
    }

    /// 대입 뒤: 이 이름 자신의 수식은 방금 계산됐으니 fresh · 값이 바뀌었으면 이 이름을 의존하는 수식은 stale.
    fn note_assigned(&mut self, key: &str, changed: bool) {
        for (name, f) in self.formulas.iter_mut() {
            if name == key {
                f.stale = false;
            } else if changed && f.deps.iter().any(|d| d == key) {
                f.stale = true;
            }
        }
    }

    /// stale인 수식 **전부**(`SHOW VARIABLES`처럼 모든 값을 보는 자리 = 사용 시점 · 사용자 09-27). 돌려주면서 stale을 내린다.
    pub fn take_all_stale_formulas(&mut self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for (name, f) in self.formulas.iter_mut() {
            if f.stale {
                f.stale = false;
                out.push((name.clone(), f.text.clone()));
            }
        }
        out
    }

    /// 수식 등록(`EXEC :V := 식` · 사용 시 모드) — 같은 수식이면 그대로.
    pub fn set_formula(&mut self, name: &str, text: &str, deps: Vec<String>) {
        let key = norm(name);
        let deps: Vec<String> = deps
            .into_iter()
            .map(|d| norm(&d))
            .filter(|d| *d != key)
            .collect();
        self.formulas.insert(
            key,
            Formula {
                text: text.to_string(),
                deps,
                stale: false,
            },
        );
    }

    /// 참조 이름들 중 stale인 수식 — 재계산할 (이름 · 수식). 돌려주면서 stale을 내린다(재계산 실패 시 무한 재계획 방지).
    pub fn take_stale_formulas(&mut self, refs: &[String]) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for r in refs {
            let key = norm(r);
            if let Some(f) = self.formulas.get_mut(&key) {
                if f.stale {
                    f.stale = false;
                    out.push((key, f.text.clone()));
                }
            }
        }
        out
    }

    /// 수식 보기(변수 창 표시용).
    pub fn formula_of(&self, name: &str) -> Option<&Formula> {
        self.formulas.get(&norm(name))
    }

    /// 타입 힌트(루틴 서명에서 읽은 타입 · `in_out` = 서명의 방향 `IN` | `OUT` | `IN/OUT`) — **선언하지 않은** 변수에만.
    /// · 타입이 아직 없으면(`Auto`) 그 타입으로 · 없는 이름은 `NULL` 값으로 만든다(선언 아님 = `declared: false`)
    /// · ★ 자동 변수가 이미 **다른 타입**을 가졌고 자리가 **OUT/IN OUT**이면 서명 타입으로 다시 맞춘다(사용자 10-08 "같은 변수를 다른
    ///   프로시저에 쓰면 기존 변수를 버리고 덮어쓰기" = `:RET`이 첫 호출에서 NUMBER 23이 된 뒤 REF CURSOR OUT에 묶여 PLS-00306).
    ///   값은 **OUT**(서버가 덮어쓴다)이거나 타입 부류가 다를 때(커서 ↔ 스칼라 · 그대로는 호출이 실패) NULL로 · IN OUT 같은 부류는 값 유지.
    /// · IN 자리·타입을 적어 선언한 변수는 건드리지 않는다. 돌려주는 값 = 적용했는가.
    pub fn hint_type(&mut self, name: &str, ty: VarType, in_out: &str) -> bool {
        let key = norm(name);
        let dir = in_out.to_ascii_uppercase();
        let out = dir.contains("OUT");
        let pure_out = dir == "OUT";
        match self.slot_mut(&key) {
            Some(v) if v.declared => false,
            Some(v) if v.ty == VarType::Auto => {
                v.ty = ty;
                self.dirty.insert(key);
                true
            }
            Some(v) if out && v.auto_ty && v.ty != ty => {
                let class_differs =
                    matches!(v.ty, VarType::RefCursor) != matches!(ty, VarType::RefCursor);
                if pure_out || class_differs {
                    v.value = Value::Null;
                }
                v.ty = ty;
                self.dirty.insert(key);
                true
            }
            Some(_) => false,
            None => {
                self.vars
                    .insert(key.clone(), Self::fresh(name, ty, Value::Null, false));
                self.dirty.insert(key);
                true
            }
        }
    }

    /// 서명 추론이 필요한가 — 없는 이름이거나 **선언하지 않은 자동 변수**(값·타입이 있어도 OUT 자리면 다시 맞춰야 하므로 ·
    /// 서명은 루틴당 1회 캐시라 비용 없음 · 10-08).
    #[must_use]
    pub fn needs_type(&self, name: &str) -> bool {
        match self.get(name) {
            None => true,
            Some(v) => !v.declared && v.auto_ty,
        }
    }

    /// 탭 → 공유 → 프로필 순으로 찾는다.
    pub fn get(&self, name: &str) -> Option<&Var> {
        let key = norm(name);
        self.vars
            .get(&key)
            .or_else(|| self.shared.get(&key))
            .or_else(|| self.global.get(&key))
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
        } else if self.global.contains_key(&key) {
            Some(Layer::Global)
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
        let gone = self
            .vars
            .remove(&key)
            .or_else(|| self.shared.remove(&key))
            .or_else(|| self.global.remove(&key));
        if gone.is_some() {
            self.dirty.insert(key);
        }
        gone
    }

    /// ★ `VAR x DROP [GLOBAL]`(사용자 10-07 · docs/63 §11-4): 탭 층에 있으면 탭 층의 그 이름을(가리던 글로벌 값이 다시 보인다) ·
    /// 없으면 공유 층을 지운다 · `global`이면 글로벌 층도 — 프로필 층은 남는다. 수식 등록도 함께 지운다. 돌려주는 값 = 하나라도 지웠는가.
    pub fn drop_var(&mut self, name: &str, global: bool) -> bool {
        let key = norm(name);
        let mut gone = self.vars.remove(&key).is_some();
        if !gone {
            gone = self.shared.remove(&key).is_some();
        }
        if global {
            gone |= self.global.remove(&key).is_some();
        }
        if gone {
            self.formulas.remove(&key);
            self.dirty.insert(key);
        }
        gone
    }

    /// ★ `VAR CLEAR [GLOBAL|ALL]`: 탭 층 전부(기본) · `global` = 글로벌 층도 · `shared` = 공유 층도 — 프로필 층은 늘 남는다.
    /// 돌려주는 값 = 지운 이름 수(층이 겹친 같은 이름은 하나로).
    pub fn clear_layers(&mut self, shared: bool, global: bool) -> usize {
        let mut keys: Vec<String> = self.vars.keys().cloned().collect();
        self.vars.clear();
        if shared {
            keys.extend(self.shared.keys().cloned());
            self.shared.clear();
        }
        if global {
            keys.extend(self.global.keys().cloned());
            self.global.clear();
        }
        for k in &keys {
            self.formulas.remove(k);
            self.dirty.insert(k.clone());
        }
        keys.sort();
        keys.dedup();
        keys.len()
    }

    /// 보이는 변수 전부(앞선 층이 가린다 · 이름순).
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Var)> {
        let mut seen: BTreeMap<&String, &Var> = BTreeMap::new();
        for (k, v) in self
            .fixed
            .iter()
            .chain(&self.global)
            .chain(&self.shared)
            .chain(&self.vars)
        {
            seen.insert(k, v);
        }
        seen.into_iter()
    }

    /// ★ **모든 층의** 변수(사용자 10-07 "같은 이름의 글로벌·탭을 함께 · 가려진 것은 흐리게"): 이름순 · 같은 이름은 앞 층(tab → shared →
    /// global → profile)부터 · 마지막 값 = 가려졌는가(앞 층에 같은 이름이 있다). `SHOW VARIABLES` 표·변수 창용.
    pub fn iter_all(&self) -> Vec<(&String, &Var, Layer, bool)> {
        let mut out: Vec<(&String, &Var, Layer, bool)> = Vec::new();
        let layers: [(&BTreeMap<String, Var>, Layer); 4] = [
            (&self.vars, Layer::Local),
            (&self.shared, Layer::Shared),
            (&self.global, Layer::Global),
            (&self.fixed, Layer::Fixed),
        ];
        for (map, layer) in layers {
            for (k, v) in map {
                let shadowed = self.layer_of(k) != Some(layer);
                out.push((k, v, layer, shadowed));
            }
        }
        let rank = |l: Layer| match l {
            Layer::Local => 0,
            Layer::Shared => 1,
            Layer::Global => 2,
            Layer::Fixed => 3,
        };
        out.sort_by(|a, b| a.0.cmp(b.0).then(rank(a.2).cmp(&rank(b.2))));
        out
    }

    /// 보이는 변수 수.
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
            && self.shared.is_empty()
            && self.global.is_empty()
            && self.fixed.is_empty()
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
        self.set_layer(name, Layer::Shared)
    }

    /// 공유 층의 이름을 탭 층으로 내린다.
    pub fn unshare(&mut self, name: &str) -> bool {
        self.set_layer(name, Layer::Local)
    }

    /// ★ 이름을 다른 층(탭 · 공유 · 글로벌)으로 옮긴다(`VAR x SHARE|LOCAL|GLOBAL` · 변수 창) — 이미 그 층이면 참 · 없는 이름·프로필 층 = 거짓.
    pub fn set_layer(&mut self, name: &str, to: Layer) -> bool {
        let key = norm(name);
        let from = match self.layer_of(name) {
            Some(Layer::Fixed) | None => return false,
            Some(l) => l,
        };
        if from == to {
            return true;
        }
        if to == Layer::Fixed {
            return false;
        }
        let src = match from {
            Layer::Local => &mut self.vars,
            Layer::Shared => &mut self.shared,
            _ => &mut self.global,
        };
        let Some(v) = src.remove(&key) else {
            return false;
        };
        let dst = match to {
            Layer::Local => &mut self.vars,
            Layer::Shared => &mut self.shared,
            Layer::Global => &mut self.global,
            Layer::Fixed => return false,
        };
        dst.insert(key.clone(), v);
        self.dirty.insert(key);
        true
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
                        auto_ty: s.ty == VarType::Auto || !s.declared,
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

    /// 공유 층을 통째로 정한다(호스트의 변수 창이 고쳤을 때).
    pub fn set_shared(&mut self, states: Vec<VarState>) {
        self.shared = Self::from_states(states);
    }

    /// 프로필 층을 정한다(접속 때).
    pub fn set_fixed(&mut self, states: Vec<VarState>) {
        self.fixed = Self::from_states(states);
    }

    /// 글로벌 층을 통째로 정한다(호스트가 시작·변경 때 · docs/63 §11).
    pub fn set_global(&mut self, states: Vec<VarState>) {
        self.global = Self::from_states(states);
    }

    /// 글로벌 층의 지금 상태.
    #[must_use]
    pub fn global_states(&self) -> Vec<VarState> {
        self.global
            .iter()
            .map(|e| Self::state(e, Layer::Global))
            .collect()
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

    /// 마지막으로 걷은 뒤 바뀐 이름(보여 줄 표기 · 사라진 이름은 대문자 키)을 걷는다.
    pub fn take_dirty(&mut self) -> Vec<String> {
        let keys = std::mem::take(&mut self.dirty);
        keys.into_iter()
            .map(|k| self.get(&k).map_or(k.clone(), |v| v.label.clone()))
            .collect()
    }

    /// 세션에 묶인 값(REF CURSOR 핸들)을 전부 비운다 — 재접속·유휴 닫기 뒤에는 죽은 핸들이다(docs/63 §4).
    pub fn invalidate_cursors(&mut self) {
        for (k, v) in self
            .vars
            .iter_mut()
            .chain(self.shared.iter_mut())
            .chain(self.global.iter_mut())
        {
            if matches!(v.value, Value::Cursor(_)) {
                v.value = Value::Null;
                self.dirty.insert(k.clone());
            }
        }
    }
}

/// 타입 → `VAR` 문에 쓸 글자(`Auto`는 선언하지 않는다 = `None`).
fn type_token(ty: &VarType) -> Option<String> {
    Some(match ty {
        VarType::Number => "NUMBER".into(),
        VarType::Varchar2(n) => format!("VARCHAR2({})", (*n).max(1)),
        VarType::Char(n) => format!("CHAR({})", (*n).max(1)),
        VarType::Clob => "CLOB".into(),
        VarType::RefCursor => "REFCURSOR".into(),
        VarType::BinaryFloat => "BINARY_FLOAT".into(),
        VarType::BinaryDouble => "BINARY_DOUBLE".into(),
        VarType::Date => "DATE".into(),
        VarType::Timestamp => "TIMESTAMP".into(),
        VarType::Boolean => "BOOLEAN".into(),
        VarType::Blob => "BLOB".into(),
        VarType::Auto => return None,
    })
}

/// 변수 표 → **실행할 수 있는 스크립트**(`VAR 이름 타입` + `EXEC :이름 := 리터럴`) — 보존(D-136)과 내보내기가 같이 쓴다.
/// 비밀 값·커서·바이트는 값을 쓰지 않는다(선언만 남는다). 다시 읽기 = [`vars_from_script`](그냥 이 스크립트를 실행해도 같다).
#[must_use]
pub fn vars_to_script(states: &[VarState]) -> String {
    let mut out = String::new();
    for s in states {
        if s.declared {
            if let Some(ty) = type_token(&s.ty) {
                out.push_str(&format!("VAR {} {ty}\n", s.name));
            }
        }
        if s.secret {
            continue;
        }
        let lit = match &s.value {
            Value::Null | Value::Cursor(_) | Value::Bytes(_) => continue,
            Value::Int(n) => n.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Decimal(d) => d.clone(),
            Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
            Value::Str(t) => format!("'{}'", t.replace('\'', "''")),
        };
        // 여러 줄 글은 한 줄 명령(`EXEC`)에 못 담는다 → 건너뛴다(값은 다음 실행에서 다시 생긴다).
        if lit.contains('\n') || lit.contains('\r') {
            continue;
        }
        out.push_str(&format!("EXEC :{} := {lit}\n", s.name));
    }
    out
}

/// [`vars_to_script`]가 만든(또는 사람이 쓴 `VAR`/`EXEC :x := 리터럴`만 있는) 스크립트 → 탭 층 변수. DB로 가는 것은 없다 —
/// 리터럴 대입과 선언만 읽고 나머지 줄은 무시한다.
#[must_use]
pub fn vars_from_script(text: &str) -> Vec<VarState> {
    let mut e = crate::engine::Engine::new(nsql_core::Dialect::Oracle);
    // 치환은 끈다(값에 `&`가 들어 있을 수 있다).
    e.settings.define_char = None;
    for item in crate::split::split_script(text) {
        if matches!(
            &item.kind,
            crate::split::ItemKind::Command(
                crate::command::Command::Variable { .. }
                    | crate::command::Command::VarDeclareIn { .. }
                    | crate::command::Command::Exec { .. }
            )
        ) {
            // 리터럴 대입·선언은 `plan` 안에서 표에 반영된다 · 서버로 가야 하는 식은 실행하지 않으므로 버려진다.
            let _ = e.plan(&item);
        }
    }
    e.vars.local_states()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 글로벌 층(docs/63 §11): 우선순위 tab > shared > global > fixed · `set_layer`로 세 층을 오간다 · 대입은 글로벌을 고치지 않고
    /// 탭 층에 가리는 값을 만든다 · 프로필 층은 옮길 수 없다.
    #[test]
    fn global_layer_precedence_and_moves() {
        let mut s = VarStore::new();
        s.set_global(vec![VarState {
            name: "PROJECT".into(),
            ty: VarType::Auto,
            value: Value::Str("g".into()),
            declared: false,
            secret: false,
            layer: Layer::Global,
        }]);
        assert_eq!(s.layer_of("project"), Some(Layer::Global));
        assert_eq!(
            s.get("PROJECT").map(|v| v.value.clone()),
            Some(Value::Str("g".into()))
        );
        // ★ D-264(10-07): 대입 = 그 이름이 사는 층 — 글로벌 값이 바뀌고 탭 층은 생기지 않는다.
        s.assign("project", Value::Str("t".into()));
        assert_eq!(s.layer_of("project"), Some(Layer::Global));
        assert_eq!(s.global_states()[0].value, Value::Str("t".into()));
        assert!(s.local_states().is_empty());
        // 탭에서 가리려면 명시 선언 — 글로벌은 그대로.
        s.declare("project", VarType::Auto, Some(Value::Str("l".into())));
        assert_eq!(s.layer_of("project"), Some(Layer::Local));
        assert_eq!(s.global_states()[0].value, Value::Str("t".into()));
        assert!(s.drop_var("project", false));
        assert_eq!(s.layer_of("project"), Some(Layer::Global));
        assert!(s.set_layer("project", Layer::Shared));
        assert_eq!(s.layer_of("project"), Some(Layer::Shared));
        assert!(!s.set_layer("nope", Layer::Global), "없는 이름");
        assert!(
            !s.set_layer("project", Layer::Fixed),
            "프로필 층으로는 못 옮긴다"
        );
        assert!(s.remove("project").is_some());
    }

    /// ★ OUT 자리의 자동 변수는 서명 타입으로 다시 맞춘다(사용자 10-08 `:RET` NUMBER → 다음 프로시저 REF CURSOR OUT): 값은 NULL ·
    /// IN 자리·선언한 변수는 그대로 · IN OUT 같은 부류는 값 유지.
    #[test]
    fn out_bind_retypes_auto_variable() {
        let mut s = VarStore::new();
        s.assign("RET", Value::Int(23));
        assert_eq!(s.get("RET").map(|v| v.ty.clone()), Some(VarType::Number));
        assert!(s.needs_type("RET"), "자동 변수는 늘 서명을 본다");
        assert!(s.hint_type("RET", VarType::RefCursor, "OUT"));
        let v = s.get("RET").unwrap();
        assert_eq!(
            (v.ty.clone(), v.value.clone()),
            (VarType::RefCursor, Value::Null)
        );
        // IN 자리 = 값 입력 → 건드리지 않는다.
        s.assign("P", Value::Str("SSS".into()));
        assert!(!s.hint_type("P", VarType::Number, "IN"));
        assert_eq!(s.get("P").unwrap().value, Value::Str("SSS".into()));
        // IN OUT · 같은 부류(스칼라 ↔ 스칼라) = 타입만 맞추고 값 유지.
        s.assign("Q", Value::Int(7));
        assert!(s.hint_type("Q", VarType::Varchar2(4000), "IN/OUT"));
        assert_eq!(s.get("Q").unwrap().value, Value::Int(7));
        // 타입을 적어 선언한 변수 = 고정.
        s.declare("D", VarType::Number, Some(Value::Int(1)));
        assert!(!s.needs_type("D"));
        assert!(!s.hint_type("D", VarType::RefCursor, "OUT"));
        assert_eq!(s.get("D").unwrap().ty, VarType::Number);
        // 없는 이름 = NULL 값으로 생김(종전).
        assert!(s.hint_type("NEW", VarType::RefCursor, "OUT"));
        assert_eq!(s.get("NEW").unwrap().ty, VarType::RefCursor);
    }

    /// ★ 자동 타입 변수(사용자 10-07 "`'Number to String'`이 NUMBER처럼"): 선언 없는 변수·타입 없는 선언은 **값마다** 타입을 다시 추론 ·
    /// NULL은 타입 유지 · 타입을 적어 선언한 변수는 고정 · 가려진 글로벌 줄의 라벨은 비지 않는다(`declare`가 라벨을 복제).
    #[test]
    fn auto_typed_variables_follow_the_value() {
        let mut s = VarStore::new();
        s.assign("E1", Value::Int(1999));
        assert_eq!(s.get("E1").map(|v| v.ty.clone()), Some(VarType::Number));
        s.assign("E1", Value::Str("Number to String".into()));
        assert_eq!(
            s.get("E1").map(|v| v.ty.clone()),
            Some(VarType::Varchar2(16)),
            "자동 변수 = 지금 값의 타입"
        );
        s.assign("E1", Value::Null);
        assert_eq!(
            s.get("E1").map(|v| v.ty.clone()),
            Some(VarType::Varchar2(16)),
            "NULL = 유지"
        );
        s.declare("T", VarType::Auto, None);
        s.assign("T", Value::Int(1));
        s.assign("T", Value::Str("x".into()));
        assert_eq!(
            s.get("T").map(|v| v.ty.clone()),
            Some(VarType::Varchar2(1)),
            "타입 없는 선언도 따라간다"
        );
        s.declare("G", VarType::Number, None);
        s.assign("G", Value::Str("abc".into()));
        assert_eq!(
            s.get("G").map(|v| v.ty.clone()),
            Some(VarType::Number),
            "선언 타입은 고정"
        );
        // 가려진 글로벌의 라벨 보존.
        s.declare("Proj", VarType::Auto, Some(Value::Int(1)));
        assert!(s.set_layer("Proj", Layer::Global));
        s.declare("Proj", VarType::Auto, Some(Value::Int(2)));
        let all = s.iter_all();
        let g = all
            .iter()
            .find(|(_, _, l, _)| *l == Layer::Global)
            .expect("global row");
        assert_eq!(g.1.label, "Proj");
        assert!(g.3, "글로벌 줄 = 가려짐");
        let t = all
            .iter()
            .find(|(_, _, l, _)| *l == Layer::Local)
            .expect("tab row");
        assert!(!t.3);
    }

    /// ★ 범위 규칙(사용자 10-07 시나리오 · D-264): `VAR A GLOBAL` + `:A := 10` = 글로벌 10(대입 = 사는 층) → 로컬 변수는 명시 선언
    /// `VAR A NUMBER = 5`(탭 가림) → DROP = 탭만 지워 10 복귀 → DROP GLOBAL = 없는 변수 · CLEAR = 탭 층만 · CLEAR GLOBAL = 글로벌도(공유는 ALL).
    #[test]
    fn drop_and_clear_follow_scope_rules() {
        let mut s = VarStore::new();
        s.declare("A", VarType::Auto, None);
        assert!(s.set_layer("A", Layer::Global));
        s.assign("A", Value::Int(10));
        assert_eq!(
            s.layer_of("A"),
            Some(Layer::Global),
            "대입 = 사는 층(글로벌)"
        );
        assert_eq!(s.get("A").map(|v| v.value.clone()), Some(Value::Int(10)));
        s.declare("A", VarType::Number, Some(Value::Int(5)));
        assert_eq!(
            s.layer_of("A"),
            Some(Layer::Local),
            "명시 선언 = 탭에 만들어 가린다(글로벌은 그대로)"
        );
        assert_eq!(s.get("A").map(|v| v.value.clone()), Some(Value::Int(5)));
        assert_eq!(s.global_states()[0].value, Value::Int(10));
        s.assign("A", Value::Int(6));
        assert_eq!(s.layer_of("A"), Some(Layer::Local), "가린 뒤 대입 = 탭");
        assert!(s.drop_var("A", false));
        assert_eq!(
            s.get("A").map(|v| v.value.clone()),
            Some(Value::Int(10)),
            "탭을 지우면 글로벌이 다시 보인다"
        );
        assert_eq!(s.layer_of("A"), Some(Layer::Global));
        assert!(!s.drop_var("NOPE", false));
        assert!(s.drop_var("A", true));
        assert!(s.get("A").is_none(), "글로벌까지 지우면 없는 변수");
        let _ = s.take_dirty();
        s.assign("B", Value::Int(1));
        s.assign("C", Value::Int(2));
        assert!(s.set_layer("C", Layer::Global));
        s.assign("D", Value::Int(3));
        assert!(s.share("D"));
        assert_eq!(s.clear_layers(false, false), 1, "탭 층만(B)");
        assert!(s.get("B").is_none());
        assert_eq!(s.layer_of("C"), Some(Layer::Global));
        assert_eq!(s.layer_of("D"), Some(Layer::Shared));
        assert_eq!(s.clear_layers(false, true), 1, "글로벌도(C)");
        assert!(s.get("C").is_none() && s.get("D").is_some());
        assert_eq!(s.clear_layers(true, true), 1, "ALL = 공유도(D)");
        assert!(s.is_empty());
        let dirty = s.take_dirty();
        assert!(dirty.iter().any(|n| n == "C") && dirty.iter().any(|n| n == "D"));
    }

    /// 보존·내보내기 왕복: 선언·타입·값이 돌아오고 비밀·커서·여러 줄은 값이 빠진다 · 결과는 실행 가능한 스크립트다.
    #[test]
    fn script_round_trip() {
        let mut s = VarStore::new();
        s.declare("n", VarType::Number, Some(Value::Int(42)));
        s.assign("V_CD", Value::Str("O'Brien & co".into()));
        s.assign("v_password", Value::Str("hunter2".into()));
        s.assign("rc", Value::Cursor(nsql_core::CursorId(1)));
        s.assign("multi", Value::Str("a\nb".into()));
        let text = vars_to_script(&s.local_states());
        assert!(text.contains("VAR n NUMBER"), "{text}");
        assert!(text.contains("EXEC :V_CD := 'O''Brien & co'"), "{text}");
        assert!(!text.contains("hunter2"), "비밀 값은 쓰지 않는다");
        let back = vars_from_script(&text);
        let get = |n: &str| back.iter().find(|v| v.name.eq_ignore_ascii_case(n));
        assert_eq!(get("n").unwrap().value, Value::Int(42));
        assert!(get("n").unwrap().declared);
        assert_eq!(
            get("V_CD").unwrap().value,
            Value::Str("O'Brien & co".into())
        );
        assert!(get("v_password").is_none() && get("multi").is_none());
    }

    /// 선언한 이름의 표기는 친 그대로 남고(D-142 · T-151) 찾기는 대소문자를 가리지 않는다.
    #[test]
    fn declared_name_keeps_its_casing() {
        let mut s = VarStore::new();
        s.declare("v_OrderCnt", VarType::Number, None);
        assert_eq!(
            s.get("V_ORDERCNT").map(|v| v.label.as_str()),
            Some("v_OrderCnt")
        );
        assert_eq!(
            s.get("v_ordercnt").map(|v| v.label.as_str()),
            Some("v_OrderCnt")
        );
        s.assign("V_ORDERCNT", Value::Int(3));
        assert_eq!(
            s.get("v_OrderCnt").map(|v| v.label.as_str()),
            Some("v_OrderCnt"),
            "대입은 표기를 바꾸지 않는다"
        );
    }

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
