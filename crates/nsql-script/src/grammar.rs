//! **문법 참조(.sqlg) — 절(clause) 기반 완성**(사용자 09-24 "SQLite `SELECT avg` 자리에 `HAVING`이 나온다 · 각 DBMS 문법 구조를
//! 참조 파일로 두고 파일을 추가하면 그 DB를 지원하는 플러그인 구조").
//!
//! - 파일 하나 = 방언 하나. `dialect = 이름` · `extends = 부모`(기본 `ansi`) · `[start] next = …`(문장 시작에 올 수 있는 것) ·
//!   `[statement 이름] clauses = …`(감지용 절 목록) · `[clause 이름] next = …`(그 절 뒤에 올 수 있는 예약어 · 파일 순서 = 자주 쓰는 것) ·
//!   `next += …`(부모에 더함) · `objects = column, table, view, schema, function, subquery`(놓을 수 있는 객체 종류 · `subquery` = 재귀) ·
//!   `same = 다른 절`(별칭 · `LEFT JOIN` → `JOIN`).
//! - 내장 = `grammar/ansi.sqlg` + `oracle`/`mssql`/`postgres`/`sqlite`(컴파일에 포함). **덧입힘** = `NSQL_HOME/grammar/*.sqlg`
//!   (호스트가 기동 때 [`load_dir`] · `dialect` 이름이 같으면 내장을 대신 · 새 이름이면 새 방언).
//! - 소비: [`for_dialect`] → [`Grammar::next`]/[`Grammar::objects`]/[`Grammar::start`] · 절 감지는 완성 문맥(`intel::context_at`)이
//!   [`Grammar::clause_names`]로 한다(괄호 깊이 0의 마지막 절 · 서브쿼리 안이면 그 안의 절).

use nsql_core::Dialect;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

/// 절 규칙(해석 뒤).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClauseRule {
    pub next: Vec<String>,
    pub objects: Vec<String>,
}

/// 파일 한 벌을 해석한 결과(부모와 합치기 전).
#[derive(Clone, Debug, Default)]
struct Raw {
    dialect: String,
    extends: Option<String>,
    start: Option<Vec<String>>,
    start_add: Vec<String>,
    statements: HashMap<String, (Option<Vec<String>>, Vec<String>)>,
    clauses: HashMap<String, RawClause>,
}

#[derive(Clone, Debug, Default)]
struct RawClause {
    next: Option<Vec<String>>,
    next_add: Vec<String>,
    objects: Option<Vec<String>>,
    same: Option<String>,
}

/// 방언 문법(부모까지 합친 최종).
#[derive(Clone, Debug, Default)]
pub struct Grammar {
    pub name: String,
    start: Vec<String>,
    statements: HashMap<String, Vec<String>>,
    clauses: HashMap<String, ClauseRule>,
    aliases: HashMap<String, String>,
}

fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

fn list(v: &str) -> Vec<String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

fn add_unique(dst: &mut Vec<String>, src: &[String]) {
    for s in src {
        if !dst.iter().any(|d| d.eq_ignore_ascii_case(s)) {
            dst.push(s.clone());
        }
    }
}

/// 파일 해석 — 형식 오류는 줄 번호와 함께.
fn parse(text: &str) -> Result<Raw, String> {
    let mut raw = Raw::default();
    // 현재 절: None = 머리 · Some(("start"|"statement"|"clause", 이름)).
    let mut cur: Option<(String, String)> = None;
    for (ln, line0) in text.lines().enumerate() {
        let line = line0.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if let Some(body) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let body = body.trim();
            if body.eq_ignore_ascii_case("start") {
                cur = Some(("start".into(), String::new()));
            } else if let Some(n) = body.strip_prefix("statement ") {
                cur = Some(("statement".into(), norm(n)));
                raw.statements.entry(norm(n)).or_default();
            } else if let Some(n) = body.strip_prefix("clause ") {
                cur = Some(("clause".into(), norm(n)));
                raw.clauses.entry(norm(n)).or_default();
            } else {
                return Err(format!("{}: 모르는 절 머리 [{body}]", ln + 1));
            }
            continue;
        }
        let (key, add, val) = if let Some((k, v)) = line.split_once("+=") {
            (k.trim(), true, v.trim())
        } else if let Some((k, v)) = line.split_once('=') {
            (k.trim(), false, v.trim())
        } else {
            return Err(format!("{}: `키 = 값`이 아님: {line}", ln + 1));
        };
        match (&cur, key) {
            (None, "dialect") => raw.dialect = val.to_ascii_lowercase(),
            (None, "extends") => raw.extends = Some(val.to_ascii_lowercase()),
            (Some((kind, _)), "next") if kind == "start" => {
                if add {
                    raw.start_add.extend(list(val));
                } else {
                    raw.start = Some(list(val));
                }
            }
            (Some((kind, name)), "clauses") if kind == "statement" => {
                let e = raw.statements.entry(name.clone()).or_default();
                if add {
                    e.1.extend(list(val));
                } else {
                    e.0 = Some(list(val));
                }
            }
            (Some((kind, name)), "next") if kind == "clause" => {
                let e = raw.clauses.entry(name.clone()).or_default();
                if add {
                    e.next_add.extend(list(val));
                } else {
                    e.next = Some(list(val));
                }
            }
            (Some((kind, name)), "objects") if kind == "clause" => {
                let e = raw.clauses.entry(name.clone()).or_default();
                e.objects = Some(
                    list(val)
                        .into_iter()
                        .map(|s| s.to_ascii_lowercase())
                        .collect(),
                );
            }
            (Some((kind, name)), "same") if kind == "clause" => {
                let e = raw.clauses.entry(name.clone()).or_default();
                e.same = Some(norm(val));
            }
            _ => return Err(format!("{}: 이 자리에 올 수 없는 키 `{key}`", ln + 1)),
        }
    }
    if raw.dialect.is_empty() {
        return Err("`dialect = 이름` 줄이 없다".into());
    }
    Ok(raw)
}

/// 부모 위에 덧입힌다(`=`는 바꿈 · `+=`는 더함 · 없는 것은 부모 그대로).
fn resolve(raw: &Raw, base: Option<&Grammar>) -> Grammar {
    let mut g = base.cloned().unwrap_or_default();
    g.name = raw.dialect.clone();
    if let Some(s) = &raw.start {
        g.start = s.clone();
    }
    add_unique(&mut g.start, &raw.start_add);
    for (name, (set, add)) in &raw.statements {
        let e = g.statements.entry(name.clone()).or_default();
        if let Some(s) = set {
            *e = s.clone();
        }
        add_unique(e, add);
    }
    for (name, rc) in &raw.clauses {
        if let Some(target) = &rc.same {
            g.aliases.insert(name.clone(), target.clone());
            continue;
        }
        let e = g.clauses.entry(name.clone()).or_default();
        if let Some(n) = &rc.next {
            e.next = n.clone();
        }
        add_unique(&mut e.next, &rc.next_add);
        if let Some(o) = &rc.objects {
            e.objects = o.clone();
        }
    }
    g
}

impl Grammar {
    /// 문장 시작에 올 수 있는 것.
    #[must_use]
    pub fn start(&self) -> &[String] {
        &self.start
    }

    fn canon<'a>(&'a self, clause: &'a str) -> String {
        let up = norm(clause);
        let mut cur = up;
        for _ in 0..4 {
            match self.aliases.get(&cur) {
                Some(t) => cur = t.clone(),
                None => break,
            }
        }
        cur
    }

    /// 절 뒤에 올 수 있는 예약어(파일 순서). 모르는 절 = `None`(호스트는 전체 표로 폴백).
    #[must_use]
    pub fn next(&self, clause: &str) -> Option<&[String]> {
        let c = self.canon(clause);
        self.clauses.get(&c).map(|r| r.next.as_slice())
    }

    /// 절에 놓을 수 있는 객체 종류(`column`·`table`·`view`·`schema`·`function`·`subquery`).
    #[must_use]
    pub fn objects(&self, clause: &str) -> Option<&[String]> {
        let c = self.canon(clause);
        self.clauses.get(&c).map(|r| r.objects.as_slice())
    }

    /// 감지용 절 이름 전부(절 + 별칭 + 문장 절 목록 · 대문자 · 낱말은 한 칸).
    #[must_use]
    pub fn clause_names(&self) -> HashSet<String> {
        let mut s: HashSet<String> = self.clauses.keys().cloned().collect();
        s.extend(self.aliases.keys().cloned());
        for v in self.statements.values() {
            s.extend(v.iter().map(|c| norm(c)));
        }
        s
    }
}

const BUILTIN: &[(&str, &str)] = &[
    ("ansi", include_str!("../grammar/ansi.sqlg")),
    ("oracle", include_str!("../grammar/oracle.sqlg")),
    ("mssql", include_str!("../grammar/mssql.sqlg")),
    ("postgres", include_str!("../grammar/postgres.sqlg")),
    ("sqlite", include_str!("../grammar/sqlite.sqlg")),
];

struct Registry {
    /// 덧입힌 원문(이름 → 파일 글) — 내장보다 먼저.
    overrides: HashMap<String, String>,
    cache: HashMap<String, Arc<Grammar>>,
}

fn registry() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(|| {
        Mutex::new(Registry {
            overrides: HashMap::new(),
            cache: HashMap::new(),
        })
    })
}

fn source_text(reg: &Registry, name: &str) -> Option<String> {
    reg.overrides.get(name).cloned().or_else(|| {
        BUILTIN
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, t)| (*t).to_string())
    })
}

fn build(reg: &mut Registry, name: &str, depth: usize) -> Result<Arc<Grammar>, String> {
    if let Some(g) = reg.cache.get(name) {
        return Ok(g.clone());
    }
    if depth > 4 {
        return Err(format!("{name}: extends 사슬이 너무 깊다"));
    }
    let text = source_text(reg, name).ok_or_else(|| format!("{name}: 문법 파일 없음"))?;
    let raw = parse(&text).map_err(|e| format!("{name}: {e}"))?;
    let base = match &raw.extends {
        Some(b) if b != name => Some(build(reg, b, depth + 1)?),
        Some(_) => None,
        // 기본 부모 = ansi(자기 자신이 ansi가 아닐 때).
        None if name != "ansi" => Some(build(reg, "ansi", depth + 1)?),
        None => None,
    };
    let g = Arc::new(resolve(&raw, base.as_deref()));
    reg.cache.insert(name.to_string(), g.clone());
    Ok(g)
}

fn dialect_name(d: Option<Dialect>) -> &'static str {
    match d {
        Some(Dialect::Oracle) => "oracle",
        Some(Dialect::Mssql) => "mssql",
        Some(Dialect::Postgres) => "postgres",
        Some(Dialect::Sqlite) => "sqlite",
        Some(Dialect::Mysql) => "mysql",
        _ => "ansi",
    }
}

/// 방언 문법(덧입힘 → 내장 → ansi). 실패는 ansi로.
#[must_use]
pub fn for_dialect(d: Option<Dialect>) -> Arc<Grammar> {
    for_name(dialect_name(d))
}

/// 이름으로(플러그인이 새 방언을 더한 경우 · 없으면 ansi).
#[must_use]
pub fn for_name(name: &str) -> Arc<Grammar> {
    let mut reg = registry().lock().unwrap_or_else(|e| e.into_inner());
    let name = name.to_ascii_lowercase();
    match build(&mut reg, &name, 0) {
        Ok(g) => g,
        Err(_) => build(&mut reg, "ansi", 0).unwrap_or_default(),
    }
}

/// 문법 파일 글을 등록(같은 방언 이름 = 내장을 대신 · 새 이름 = 새 방언). 캐시는 비운다.
pub fn register(text: &str) -> Result<String, String> {
    let raw = parse(text)?;
    let mut reg = registry().lock().unwrap_or_else(|e| e.into_inner());
    reg.overrides.insert(raw.dialect.clone(), text.to_string());
    reg.cache.clear();
    Ok(raw.dialect)
}

/// 폴더의 `*.sqlg` 전부 등록 — (파일 이름, 결과). 폴더가 없으면 빈 목록.
pub fn load_dir(dir: &std::path::Path) -> Vec<(String, Result<String, String>)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut files: Vec<_> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "sqlg"))
        .collect();
    files.sort();
    for p in files {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let r = std::fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|t| register(&t));
        out.push((name, r));
    }
    out
}

/// 시험·진단: 내장 방언 이름들.
#[must_use]
pub fn builtin_names() -> Vec<&'static str> {
    BUILTIN.iter().map(|(n, _)| *n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_files_parse_and_extend_ansi() {
        for (n, t) in BUILTIN {
            parse(t).unwrap_or_else(|e| panic!("{n}: {e}"));
        }
        let lite = for_dialect(Some(Dialect::Sqlite));
        assert_eq!(lite.name, "sqlite");
        let sel = lite.next("SELECT").expect("select");
        assert!(sel.iter().any(|k| k == "FROM") && sel.iter().any(|k| k == "GLOB"));
        assert!(
            !sel.iter().any(|k| k == "HAVING"),
            "SELECT 목록 뒤에 HAVING은 없다"
        );
        let gb = lite.next("group by").expect("group by");
        assert!(gb.iter().any(|k| k == "HAVING") && gb.iter().any(|k| k == "LIMIT"));
        // 별칭 · objects · 부모 상속.
        assert_eq!(lite.next("LEFT JOIN"), lite.next("JOIN"));
        assert!(lite
            .objects("FROM")
            .expect("from")
            .contains(&"subquery".to_string()));
        assert!(
            lite.start().iter().any(|k| k == "PRAGMA")
                && lite.start().iter().any(|k| k == "SELECT")
        );
        let ora = for_dialect(Some(Dialect::Oracle));
        assert!(ora
            .next("WHERE")
            .expect("where")
            .iter()
            .any(|k| k == "CONNECT BY"));
        assert!(ora.clause_names().contains("CONNECT BY"));
        let ms = for_dialect(Some(Dialect::Mssql));
        assert!(ms
            .next("SELECT")
            .expect("select")
            .iter()
            .any(|k| k == "TOP"));
        assert!(!ms
            .next("SELECT")
            .expect("select")
            .iter()
            .any(|k| k == "ROWNUM"));
        assert_eq!(for_dialect(None).name, "ansi");
    }

    #[test]
    fn override_replaces_builtin_and_new_dialect_extends_ansi() {
        let txt = "dialect = tibero\nextends = oracle\n[clause SELECT]\nnext += PSM_ONLY\n";
        assert_eq!(register(txt).expect("register"), "tibero");
        let g = for_name("tibero");
        assert_eq!(g.name, "tibero");
        let sel = g.next("SELECT").expect("select");
        assert!(sel.iter().any(|k| k == "PSM_ONLY") && sel.iter().any(|k| k == "ROWNUM"));
        assert!(parse("[clause X]\nnext = A").is_err(), "dialect 줄 없음");
        assert!(parse("dialect = x\nfoo\n").is_err(), "키=값 아님");
    }
}
