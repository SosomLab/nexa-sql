//! **실행 전에 빠진 입력 찾기**(D-137 · docs/63 V3) — 스크립트를 실행하기 전에 한 번 훑어, 값이 없는 채로 **읽히는** 바인드와
//! 정의되지 않은 치환 변수(`&v`)를 모은다. 호스트는 이것으로 **실행당 한 번** 입력 격자를 띄운다(문장마다 묻지 않는다).
//!
//! 판정은 보수적이다 — "묻지 않아도 되는 것을 묻는" 쪽보다 "물어야 할 것을 놓치는" 쪽이 덜 성가시다:
//! - 스크립트 안에서 **먼저 대입되는** 이름은 묻지 않는다(`EXEC :V := …` · `SELECT … INTO :V` · `VAR V 타입` · `RETURNING … INTO :V`).
//! - 프로시저 호출의 **인자 전체가 바인드 하나**(`EXEC p(:OUT, …)`)면 OUT일 수 있어 묻지 않는다.
//! - 익명 블록(`BEGIN … END;`)의 바인드는 블록이 채울 수 있어 묻지 않는다.
//! - 저장 코드 DDL의 `:NEW`/`:x`는 바인드가 아니다.
//!
//! 남는 것 = 보통 SQL(조회·DML)이 **읽기만** 하는, 표에도 없고 앞에서 대입되지도 않은 이름 — 대개 오타나 빠진 입력이다.

use crate::bind::{extract_binds, unique_names};
use crate::call::call_shape;
use crate::command::Command;
use crate::lexer::{classify, Class};
use crate::split::{split_script_in, ItemKind, SqlKind};
use crate::vars::{norm, VarStore};
use nsql_core::Dialect;
use std::collections::{BTreeMap, BTreeSet};

/// 빠진 입력의 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputKind {
    /// 바인드 변수(`:NAME`) — 값은 타입 있는 값으로 표에 들어간다.
    Bind,
    /// 치환 변수(`&NAME`) — 값은 글자 그대로 끼워진다.
    Macro,
}

/// 빠진 입력 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputNeed {
    pub kind: InputKind,
    /// 보여 줄 이름(스크립트에 쓴 표기 · 접두 없음).
    pub name: String,
    /// 처음 나온 줄(1부터).
    pub line: usize,
}

/// 글에서 치환 변수 참조 이름(대문자)을 나온 순서대로 — [`crate::Engine::substitute`]와 같은 규칙(주석 안 제외).
#[must_use]
pub fn macro_refs(text: &str, define_char: char) -> Vec<String> {
    let classes = classify(text);
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == define_char as u8
            && !matches!(classes[i], Class::LineComment | Class::BlockComment)
        {
            let double = i + 1 < b.len() && b[i + 1] == define_char as u8;
            let start = i + if double { 2 } else { 1 };
            let mut j = start;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                j += 1;
            }
            if j > start {
                out.push(text[start..j].to_ascii_uppercase());
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// `SELECT … INTO :A, :B FROM …` · `… RETURNING x INTO :A`의 받는 바인드(대문자).
pub(crate) fn into_targets(sql: &str) -> Vec<String> {
    let up = sql.to_ascii_uppercase();
    let classes = classify(sql);
    // 코드 상태의 마지막 ` INTO ` 뒤부터 다음 ` FROM `(없으면 끝)까지의 바인드.
    let mut at = None;
    let mut from = 0;
    while let Some(i) = up[from..].find("INTO") {
        let p = from + i;
        let before_ok = p == 0 || !up.as_bytes()[p - 1].is_ascii_alphanumeric();
        let after_ok = up
            .as_bytes()
            .get(p + 4)
            .is_none_or(|c| !c.is_ascii_alphanumeric() && *c != b'_');
        if before_ok && after_ok && classes.get(p) == Some(&Class::Code) {
            at = Some(p + 4);
        }
        from = p + 4;
    }
    let Some(start) = at else {
        return Vec::new();
    };
    let tail = &sql[start..];
    let tail_up = &up[start..];
    let end = tail_up.find("FROM").unwrap_or(tail.len());
    extract_binds(&tail[..end])
        .into_iter()
        .map(|b| b.name)
        .collect()
}

/// 실행 전에 빠진 입력을 찾는다. `vars` = 지금 표(탭 + 공유 + 프로필) · `defines` = 지금 치환 변수 · `define_char` = `SET DEFINE`
/// (`None` = 치환 끔) · `args` = `&1..`로 넘어온 인자 수.
#[must_use]
pub fn missing_inputs(
    src: &str,
    dialect: Option<Dialect>,
    vars: &VarStore,
    defines: &BTreeMap<String, String>,
    define_char: Option<char>,
) -> Vec<InputNeed> {
    let mut assigned: BTreeSet<String> = BTreeSet::new();
    let mut defined: BTreeSet<String> = defines.keys().cloned().collect();
    let mut seen: BTreeSet<(bool, String)> = BTreeSet::new();
    let mut out = Vec::new();
    let mut define = define_char;
    for item in split_script_in(src, dialect) {
        // ① 치환 변수 — 명령이든 SQL이든 글 전체에서(치환이 가장 먼저 돈다).
        if let Some(ch) = define {
            // `DEFINE x = …`·`UNDEFINE x`·`ACCEPT` 줄의 이름 자리는 참조가 아니다.
            let is_def_cmd = matches!(
                &item.kind,
                ItemKind::Command(Command::Define { .. } | Command::Undefine { .. })
            );
            if !is_def_cmd {
                for name in macro_refs(&item.text, ch) {
                    if !defined.contains(&name) && seen.insert((true, name.clone())) {
                        out.push(InputNeed {
                            kind: InputKind::Macro,
                            name,
                            line: item.line,
                        });
                    }
                }
            }
        }
        // ② 이 문장이 만드는 것 · 읽는 것.
        let mut reads: Vec<String> = Vec::new();
        match &item.kind {
            ItemKind::Invalid(_) => {}
            ItemKind::Command(cmd) => match cmd {
                Command::Define {
                    name: Some(name), ..
                }
                | Command::SetVar { name, .. } => {
                    defined.insert(name.to_ascii_uppercase());
                }
                Command::Undefine { names } => {
                    for n in names {
                        defined.remove(&n.to_ascii_uppercase());
                    }
                }
                Command::Set(crate::command::SetOption::Define(c)) => define = *c,
                Command::Variable {
                    name: Some(n),
                    ty: Some(_),
                    ..
                } => {
                    assigned.insert(norm(n));
                }
                Command::Exec { body } => {
                    let binds = unique_names(&extract_binds(body));
                    let mut targets: BTreeSet<String> = BTreeSet::new();
                    if let Some((lhs, _)) = body.split_once(":=") {
                        let l = lhs.trim();
                        if l.starts_with(':') && extract_binds(l).len() == 1 {
                            targets.insert(norm(l));
                        }
                    }
                    targets.extend(into_targets(body));
                    if let Some(shape) = call_shape(body) {
                        targets.extend(shape.ret);
                        targets.extend(shape.args.into_iter().filter_map(|a| a.bind));
                    }
                    // `OPEN :rc FOR …` — 첫 바인드가 받는 쪽.
                    if body.trim_start().to_ascii_uppercase().starts_with("OPEN ") {
                        if let Some(first) = binds.first() {
                            targets.insert(first.clone());
                        }
                    }
                    reads.extend(binds.iter().filter(|b| !targets.contains(*b)).cloned());
                    assigned.extend(targets);
                }
                _ => {}
            },
            // 익명 블록·저장 코드 DDL = 묻지 않는다(블록이 채울 수 있다 · DDL 본문은 바인드가 아니다).
            ItemKind::Sql(SqlKind::Block | SqlKind::Ddl) => {
                if !item
                    .text
                    .trim_start()
                    .get(..6)
                    .is_some_and(|w| w.eq_ignore_ascii_case("CREATE"))
                {
                    assigned.extend(unique_names(&extract_binds(&item.text)));
                }
            }
            ItemKind::Sql(_) => {
                let targets: BTreeSet<String> = into_targets(&item.text).into_iter().collect();
                for b in unique_names(&extract_binds(&item.text)) {
                    if targets.contains(&b) {
                        assigned.insert(b);
                    } else {
                        reads.push(b);
                    }
                }
            }
        }
        for name in reads {
            // 자리 바인드(`:1`)는 변수가 아니다.
            if name.starts_with(|c: char| c.is_ascii_digit()) {
                continue;
            }
            if assigned.contains(&name) || vars.contains(&name) {
                continue;
            }
            if seen.insert((false, name.clone())) {
                out.push(InputNeed {
                    kind: InputKind::Bind,
                    name,
                    line: item.line,
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_core::Value;

    fn needs(src: &str, vars: &VarStore) -> Vec<(InputKind, String)> {
        missing_inputs(
            src,
            Some(Dialect::Oracle),
            vars,
            &BTreeMap::new(),
            Some('&'),
        )
        .into_iter()
        .map(|n| (n.kind, n.name))
        .collect()
    }

    /// 사용자 관용 스크립트(Golden) — 앞에서 대입한 것 · OUT일 수 있는 호출 인자 · INTO 대상은 묻지 않는다.
    #[test]
    fn assigned_before_use_is_not_asked() {
        let src = "EXEC :V_CD := 'SEBANG';\nEXEC\nSELECT a, b INTO :V_A, :V_B FROM t WHERE cd = :V_CD\n;\n\
                   SELECT :V_A, :V_B, :V_CD FROM DUAL;\nVAR PC REFCURSOR;\nEXEC p(:PC, :V_CD, :PC2);\n";
        assert!(needs(src, &VarStore::new()).is_empty());
    }

    /// 읽기만 하는 미지의 바인드 · 미정의 치환 변수는 한 번씩(처음 나온 순서).
    #[test]
    fn unknown_reads_and_macros_are_asked_once() {
        let src = "SELECT * FROM emp WHERE deptno = :DEPT AND job = '&job' -- &not_this\n;\n\
                   SELECT :DEPT, :KNOWN, '&job', &&limit FROM DUAL;\nDEFINE later = 1\nSELECT &later FROM DUAL;\n";
        let mut vars = VarStore::new();
        vars.assign("known", Value::Int(1));
        assert_eq!(
            needs(src, &vars),
            [
                (InputKind::Macro, "JOB".to_string()),
                (InputKind::Bind, "DEPT".to_string()),
                (InputKind::Macro, "LIMIT".to_string()),
            ]
        );
    }

    /// 블록·저장 코드·캐스트·자리 바인드·`SET DEFINE OFF`는 묻지 않는다.
    #[test]
    fn blocks_casts_positionals_and_define_off() {
        let src = "BEGIN :OUT1 := 1; END;\n/\nSELECT :OUT1 FROM DUAL;\n\
                   CREATE OR REPLACE TRIGGER tr BEFORE INSERT ON t FOR EACH ROW\nBEGIN\n  :NEW.a := 1;\nEND;\n/\n\
                   SET DEFINE OFF\nSELECT 'a&b', x::text FROM t;\n";
        assert!(
            needs(src, &VarStore::new()).is_empty(),
            "{:?}",
            needs(src, &VarStore::new())
        );
        let got = missing_inputs(
            "UPDATE t SET a = :A WHERE id = :ID RETURNING b INTO :B;\n",
            Some(Dialect::Oracle),
            &VarStore::new(),
            &BTreeMap::new(),
            None,
        );
        let names: Vec<&str> = got.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["A", "ID"], "RETURNING INTO 대상은 받는 쪽");
    }
}
