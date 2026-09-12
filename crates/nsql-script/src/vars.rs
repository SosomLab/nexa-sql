//! 세션 변수 저장소 — SQL*Plus 바인드 테이블의 클라이언트 측 복제(docs/04 §8.2).
//! 이름은 대소문자 무관(대문자 정규화). 값은 실행 사이에 **여기에만** 산다.

use nsql_core::{Value, VarType};
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Debug)]
pub struct Var {
    pub ty: VarType,
    pub value: Value,
    /// `VARIABLE`로 명시 선언했는가(아니면 대입으로 암묵 생성).
    pub declared: bool,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct VarStore {
    vars: BTreeMap<String, Var>,
}

pub fn norm(name: &str) -> String {
    name.trim().trim_start_matches(':').to_ascii_uppercase()
}

impl VarStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// `VARIABLE name type [= value]`.
    pub fn declare(&mut self, name: &str, ty: VarType, init: Option<Value>) {
        let key = norm(name);
        let value = init.unwrap_or(Value::Null);
        self.vars.insert(
            key,
            Var {
                ty,
                value,
                declared: true,
            },
        );
    }

    /// 대입 — 없으면 [`VarType::Auto`]로 암묵 생성(Golden 관용 · docs/08 §3-2).
    /// 선언된 타입이 있으면 유지하고, `Auto`면 값에서 따라간다.
    pub fn assign(&mut self, name: &str, value: Value) {
        let key = norm(name);
        match self.vars.get_mut(&key) {
            Some(v) => {
                if v.ty == VarType::Auto {
                    v.ty = VarType::infer(&value);
                }
                v.value = value;
            }
            None => {
                let ty = VarType::infer(&value);
                self.vars.insert(
                    key,
                    Var {
                        ty,
                        value,
                        declared: false,
                    },
                );
            }
        }
    }

    pub fn get(&self, name: &str) -> Option<&Var> {
        self.vars.get(&norm(name))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.vars.contains_key(&norm(name))
    }

    pub fn remove(&mut self, name: &str) -> Option<Var> {
        self.vars.remove(&norm(name))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Var)> {
        self.vars.iter()
    }

    pub fn len(&self) -> usize {
        self.vars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    /// 실행 결과의 OUT 값을 되돌려 받는다. 미지의 이름은 무시하지 않고 생성한다
    /// (드라이버가 `SELECT … INTO :NEW`로 새 변수를 만들 수 있게).
    pub fn absorb(&mut self, outs: &[(String, Value)]) {
        for (name, v) in outs {
            self.assign(name, v.clone());
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
        assert_eq!(s.len(), 2);
    }
}
