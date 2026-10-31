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
    /// ★ T-200 절 안 위치(10-30): 절 키워드 **바로 뒤(첫 항목 자리)에만** 오는 예약어(`SELECT` = `DISTINCT`·`ALL` · T-SQL `TOP`) —
    /// 항목이 하나라도 나온 뒤에는 내지 않는다.
    pub first: Vec<String>,
}

/// ★ T-200(10-30): 절 안에서 캐럿이 놓인 자리 — [`Grammar::next_at`]가 예약어 후보를 좁히는 근거.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ClausePos {
    /// 절 키워드 바로 뒤(항목 없음) — `first` 예약어를 더하고 절 전환 예약어(`FROM`·`WHERE` …)는 뺀다(`SELECT FROM`은 없다).
    First,
    /// 콤마 바로 뒤(다음 항목 자리) — `first`를 뺀다(`SELECT a, DISTINCT`는 없다). 절 전환 예약어는 남긴다(사용자가 `fr…`를
    /// 치다 FROM으로 가는 흐름 · 기존 시험 `SELECT a, fr` = FROM 제안).
    AfterComma,
    /// 그 밖(항목 중간·뒤) — `first`만 뺀다.
    #[default]
    Other,
}

/// ★ T-200 CASE 식 상태(10-31 · docs/82 §10-4-3): 짝 안 맞는 `CASE` 안에서 캐럿이 놓인 자리 — [`Grammar::case_next`]의 색인.
/// 절(`SELECT`·`WHERE` …)이 아니라 **식 내부** 상태라 절 `next`보다 우선하고, 바깥 절의 `objects`는 그대로 쓴다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CasePos {
    /// `CASE |` · `CASE 식 |`(단순형) — 다음 = `WHEN`.
    AfterOpen = 0,
    /// `CASE WHEN 조건 |` — 다음 = `THEN`(과 조건 연산자).
    AfterWhen = 1,
    /// `CASE WHEN … THEN 결과 |` — 다음 = `WHEN`·`ELSE`·`END`.
    AfterThen = 2,
    /// `CASE … ELSE 결과 |` — 다음 = `END`.
    AfterElse = 3,
}

impl CasePos {
    /// `[expression CASE]` 섹션의 키 이름 → 자리.
    fn from_key(k: &str) -> Option<Self> {
        match k {
            "after_open" => Some(Self::AfterOpen),
            "after_when" => Some(Self::AfterWhen),
            "after_then" => Some(Self::AfterThen),
            "after_else" => Some(Self::AfterElse),
            _ => None,
        }
    }
}

/// `[expression CASE]`의 네 자리(해석 전 · 색인 = [`CasePos`]).
#[derive(Clone, Debug, Default)]
struct RawCase {
    set: [Option<Vec<String>>; 4],
    add: [Vec<String>; 4],
    del: [Vec<String>; 4],
}

/// 파일 한 벌을 해석한 결과(부모와 합치기 전).
#[derive(Clone, Debug, Default)]
struct Raw {
    dialect: String,
    extends: Option<String>,
    start: Option<Vec<String>>,
    start_add: Vec<String>,
    /// ★ `next -= …`(10-30 · docs/82 §10 "상속에 빼기가 없어 방언에 없는 예약어가 제안됨"): 부모에서 뺄 것.
    start_del: Vec<String>,
    statements: HashMap<String, (Option<Vec<String>>, Vec<String>)>,
    clauses: HashMap<String, RawClause>,
    /// ★ T-200(10-31): `[expression CASE]`.
    case_rule: RawCase,
}

#[derive(Clone, Debug, Default)]
struct RawClause {
    next: Option<Vec<String>>,
    next_add: Vec<String>,
    objects: Option<Vec<String>>,
    same: Option<String>,
    first: Option<Vec<String>>,
    first_add: Vec<String>,
    next_del: Vec<String>,
    first_del: Vec<String>,
}

/// 줄의 연산자: `=`(교체) · `+=`(부모에 더함) · `-=`(부모에서 뺌 · 10-30).
#[derive(Clone, Copy, PartialEq, Eq)]
enum LineOp {
    Set,
    Add,
    Del,
}

/// 방언 문법(부모까지 합친 최종).
#[derive(Clone, Debug, Default)]
pub struct Grammar {
    pub name: String,
    start: Vec<String>,
    statements: HashMap<String, Vec<String>>,
    clauses: HashMap<String, ClauseRule>,
    aliases: HashMap<String, String>,
    /// ★ T-200(10-31): CASE 식 안 자리별 예약어(색인 = [`CasePos`] · 전부 비면 섹션 없음).
    case_rule: [Vec<String>; 4],
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

/// `-=`(10-30): 대소문자 무시로 `src`에 있는 항목을 `dst`에서 뺀다(없는 것은 무시).
fn remove_ci(dst: &mut Vec<String>, src: &[String]) {
    if src.is_empty() {
        return;
    }
    dst.retain(|d| !src.iter().any(|s| s.eq_ignore_ascii_case(d)));
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
            } else if let Some(n) = body.strip_prefix("expression ") {
                // ★ T-200(10-31): 식 내부 상태 — 지금은 `CASE`만.
                if norm(n) != "CASE" {
                    return Err(tr!(
                        "{}: unknown expression section [{body}] (only `expression CASE`)",
                        "{}: 모르는 식 섹션 [{body}](`expression CASE`만)",
                        ln + 1
                    ));
                }
                cur = Some(("expression".into(), "CASE".into()));
            } else {
                return Err(tr!(
                    "{}: unknown section header [{body}]",
                    "{}: 모르는 절 머리 [{body}]",
                    ln + 1
                ));
            }
            continue;
        }
        let (key, op, val) = if let Some((k, v)) = line.split_once("+=") {
            (k.trim(), LineOp::Add, v.trim())
        } else if let Some((k, v)) = line.split_once("-=") {
            (k.trim(), LineOp::Del, v.trim())
        } else if let Some((k, v)) = line.split_once('=') {
            (k.trim(), LineOp::Set, v.trim())
        } else {
            return Err(tr!(
                "{}: not a `key = value` line: {line}",
                "{}: `키 = 값`이 아님: {line}",
                ln + 1
            ));
        };
        match (&cur, key) {
            (None, "dialect") => raw.dialect = val.to_ascii_lowercase(),
            (None, "extends") => raw.extends = Some(val.to_ascii_lowercase()),
            (Some((kind, _)), "next") if kind == "start" => match op {
                LineOp::Add => raw.start_add.extend(list(val)),
                LineOp::Del => raw.start_del.extend(list(val)),
                LineOp::Set => raw.start = Some(list(val)),
            },
            (Some((kind, name)), "clauses") if kind == "statement" => {
                let e = raw.statements.entry(name.clone()).or_default();
                match op {
                    LineOp::Add => e.1.extend(list(val)),
                    LineOp::Set => e.0 = Some(list(val)),
                    LineOp::Del => {
                        return Err(tr!(
                            "{}: `-=` is only for `next`/`first`",
                            "{}: `-=`는 `next`/`first`에서만",
                            ln + 1
                        ))
                    }
                }
            }
            (Some((kind, name)), "next") if kind == "clause" => {
                let e = raw.clauses.entry(name.clone()).or_default();
                match op {
                    LineOp::Add => e.next_add.extend(list(val)),
                    LineOp::Del => e.next_del.extend(list(val)),
                    LineOp::Set => e.next = Some(list(val)),
                }
            }
            // ★ T-200(10-30): 첫 항목 자리에만 오는 예약어(`first = …` / `first += …`).
            (Some((kind, name)), "first") if kind == "clause" => {
                let e = raw.clauses.entry(name.clone()).or_default();
                match op {
                    LineOp::Add => e.first_add.extend(list(val)),
                    LineOp::Del => e.first_del.extend(list(val)),
                    LineOp::Set => e.first = Some(list(val)),
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
            // ★ T-200(10-31): `[expression CASE]` after_open / after_when / after_then / after_else(`=`·`+=`·`-=`).
            (Some((kind, _)), key) if kind == "expression" => {
                let Some(pos) = CasePos::from_key(key) else {
                    return Err(tr!(
                        "{}: key `{key}` is not allowed in [expression CASE] (after_open/after_when/after_then/after_else)",
                        "{}: [expression CASE]에 올 수 없는 키 `{key}`(after_open/after_when/after_then/after_else)",
                        ln + 1
                    ));
                };
                let i = pos as usize;
                match op {
                    LineOp::Add => raw.case_rule.add[i].extend(list(val)),
                    LineOp::Del => raw.case_rule.del[i].extend(list(val)),
                    LineOp::Set => raw.case_rule.set[i] = Some(list(val)),
                }
            }
            _ => {
                return Err(tr!(
                    "{}: key `{key}` is not allowed here",
                    "{}: 이 자리에 올 수 없는 키 `{key}`",
                    ln + 1
                ))
            }
        }
    }
    if raw.dialect.is_empty() {
        return Err(tr!(
            "missing `dialect = name` line",
            "`dialect = 이름` 줄이 없다"
        ));
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
    remove_ci(&mut g.start, &raw.start_del);
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
        if let Some(f) = &rc.first {
            e.first = f.clone();
        }
        add_unique(&mut e.first, &rc.first_add);
        remove_ci(&mut e.next, &rc.next_del);
        remove_ci(&mut e.first, &rc.first_del);
    }
    for (i, slot) in g.case_rule.iter_mut().enumerate() {
        if let Some(s) = &raw.case_rule.set[i] {
            *slot = s.clone();
        }
        add_unique(slot, &raw.case_rule.add[i]);
        remove_ci(slot, &raw.case_rule.del[i]);
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

    /// 절 키워드 바로 뒤(첫 항목 자리)에만 오는 예약어(`first = …`) — 모르는 절 = `None`.
    #[must_use]
    pub fn first(&self, clause: &str) -> Option<&[String]> {
        let c = self.canon(clause);
        self.clauses.get(&c).map(|r| r.first.as_slice())
    }

    /// ★ T-200 절 안 위치(10-30): 자리에 맞는 예약어 — `First` = `first` + `next`에서 절 전환 예약어(다른 절 이름)를 뺀 것 ·
    /// `AfterComma` = `next`에서 절 전환 예약어와 `first`를 뺀 것 · `Other` = `next`에서 `first`를 뺀 것(파일 순서 유지).
    /// 모르는 절 = `None`(호스트는 전체 표로 폴백).
    #[must_use]
    pub fn next_at(&self, clause: &str, pos: ClausePos) -> Option<Vec<String>> {
        let c = self.canon(clause);
        let rule = self.clauses.get(&c)?;
        let names = self.clause_names();
        let is_clause = |k: &str| names.contains(&norm(k));
        let in_first = |k: &str| rule.first.iter().any(|f| f.eq_ignore_ascii_case(k));
        let mut out: Vec<String> = Vec::new();
        if pos == ClausePos::First {
            out.extend(rule.first.iter().cloned());
        }
        for k in &rule.next {
            let drop = match pos {
                ClausePos::First => is_clause(k) || in_first(k),
                ClausePos::AfterComma => in_first(k),
                ClausePos::Other => in_first(k),
            };
            if !drop && !out.iter().any(|o| o.eq_ignore_ascii_case(k)) {
                out.push(k.clone());
            }
        }
        Some(out)
    }

    /// ★ T-200 CASE 식 상태(10-31): 짝 안 맞는 `CASE` 안의 자리별 예약어(`[expression CASE]`) — 섹션이 없는 문법이면 `None`
    /// (호스트는 절 `next`로 되돌아간다).
    #[must_use]
    pub fn case_next(&self, pos: CasePos) -> Option<Vec<String>> {
        if self.case_rule.iter().all(Vec::is_empty) {
            return None;
        }
        Some(self.case_rule[pos as usize].clone())
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
    ("mysql", include_str!("../grammar/mysql.sqlg")),
    ("clickhouse", include_str!("../grammar/clickhouse.sqlg")),
    ("db2", include_str!("../grammar/db2.sqlg")),
    ("access", include_str!("../grammar/access.sqlg")),
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
        return Err(tr!(
            "{name}: extends chain is too deep",
            "{name}: extends 사슬이 너무 깊다"
        ));
    }
    let text = source_text(reg, name)
        .ok_or_else(|| tr!("{name}: grammar file not found", "{name}: 문법 파일 없음"))?;
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
        Some(Dialect::Clickhouse) => "clickhouse",
        Some(Dialect::Db2) => "db2",
        Some(Dialect::Access) => "access",
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

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ T-200 절 안 위치(10-30): `first`(SELECT = DISTINCT·ALL · mssql += TOP) · 자리별 예약어 — First = first + 전환 제외 ·
    /// AfterComma = 전환·first 제외 · Other = first 제외 · 모르는 절 = None.
    #[test]
    fn next_at_narrows_by_clause_position() {
        let g = for_dialect(None);
        let has = |v: &[String], k: &str| v.iter().any(|s| s.eq_ignore_ascii_case(k));
        assert!(has(g.first("SELECT").unwrap(), "DISTINCT"));
        let first = g.next_at("SELECT", ClausePos::First).unwrap();
        assert!(has(&first, "DISTINCT") && has(&first, "CASE"), "{first:?}");
        assert!(
            !has(&first, "FROM"),
            "SELECT 바로 뒤에 FROM 없음: {first:?}"
        );
        let comma = g.next_at("SELECT", ClausePos::AfterComma).unwrap();
        assert!(has(&comma, "CASE") && has(&comma, "FROM") && !has(&comma, "DISTINCT"));
        let other = g.next_at("SELECT", ClausePos::Other).unwrap();
        assert!(has(&other, "FROM") && !has(&other, "DISTINCT"));
        assert!(g.next_at("NO SUCH", ClausePos::Other).is_none());
        let ms = for_dialect(Some(Dialect::Mssql));
        assert!(
            has(ms.first("SELECT").unwrap(), "TOP"),
            "mssql first += TOP"
        );
        assert!(
            has(ms.first("SELECT").unwrap(), "DISTINCT"),
            "부모 first 상속"
        );
        assert!(!has(
            &ms.next_at("SELECT", ClausePos::Other).unwrap(),
            "TOP"
        ));
    }

    /// ★ T-200 CASE 식 상태(10-31 · 82 §10-4-3): ansi `[expression CASE]` 네 자리 · 절 next에서 WHEN/THEN/ELSE/END가 빠짐(CASE는 남음) ·
    /// 자식 방언의 `+=`/`-=` · 모르는 키·섹션은 오류.
    #[test]
    fn case_expression_section_parses_and_inherits() {
        let g = for_dialect(None);
        let has = |v: &[String], k: &str| v.iter().any(|s| s.eq_ignore_ascii_case(k));
        assert!(has(&g.case_next(CasePos::AfterOpen).unwrap(), "WHEN"));
        assert!(has(&g.case_next(CasePos::AfterWhen).unwrap(), "THEN"));
        let then = g.case_next(CasePos::AfterThen).unwrap();
        assert!(
            has(&then, "WHEN") && has(&then, "ELSE") && has(&then, "END"),
            "{then:?}"
        );
        assert!(has(&g.case_next(CasePos::AfterElse).unwrap(), "END"));
        for cl in ["SELECT", "ON", "WHERE", "HAVING", "SET"] {
            let n = g.next(cl).unwrap();
            assert!(
                has(n, "CASE")
                    && !has(n, "WHEN")
                    && !has(n, "THEN")
                    && !has(n, "ELSE")
                    && !has(n, "END"),
                "{cl}: {n:?}"
            );
        }
        let name = register(
            "dialect = testcase
extends = ansi
[expression CASE]
after_then -= ELSE
after_open += EXTRA_KW
",
        )
        .unwrap();
        let t = for_name(&name);
        assert!(!has(&t.case_next(CasePos::AfterThen).unwrap(), "ELSE"));
        let open = t.case_next(CasePos::AfterOpen).unwrap();
        assert!(
            has(&open, "EXTRA_KW") && has(&open, "WHEN"),
            "부모 상속 + 더함: {open:?}"
        );
        assert!(register(
            "dialect = badcase
[expression CASE]
next = X
"
        )
        .is_err());
        assert!(register(
            "dialect = badcase2
[expression IF]
after_open = X
"
        )
        .is_err());
        let empty = parse(
            "dialect = nocase
[start]
next = SELECT
",
        )
        .unwrap();
        assert!(
            resolve(&empty, None)
                .case_next(CasePos::AfterOpen)
                .is_none(),
            "섹션 없는 문법 = None"
        );
    }

    /// ★ `-=`(10-30 · docs/82 §10): 부모에서 물려받은 예약어를 자식 방언이 뺀다(next · first · start) · 대소문자 무시 · 없는 것은 무시 ·
    /// `clauses -=`는 오류.
    #[test]
    fn minus_equals_removes_inherited_keywords() {
        let name = register(
            "dialect = testdel\nextends = ansi\n[start]\nnext -= EXPLAIN, nothing_here\n[clause SELECT]\nnext -= and, OR\nfirst -= ALL\nnext += EXTRA_KW\n",
        )
        .expect("register");
        let g = for_name(&name);
        let has = |v: &[String], k: &str| v.iter().any(|s| s.eq_ignore_ascii_case(k));
        assert!(!has(g.start(), "EXPLAIN") && has(g.start(), "SELECT"));
        let n = g.next("SELECT").unwrap();
        assert!(!has(n, "AND") && !has(n, "OR") && has(n, "CASE") && has(n, "EXTRA_KW"));
        let f = g.first("SELECT").unwrap();
        assert!(!has(f, "ALL") && has(f, "DISTINCT"));
        // 부모(ansi)는 그대로.
        assert!(has(for_dialect(None).next("SELECT").unwrap(), "AND"));
        assert!(
            register("dialect = bad\nextends = ansi\n[statement SELECT]\nclauses -= FROM\n")
                .is_err()
        );
    }

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
