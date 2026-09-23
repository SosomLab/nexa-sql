//! **자동 완성 문맥·랭킹**(docs/47 §6 · docs/76 · 사용자 09-23 "가장 보편적이고 친숙도가 높은 방식") — 순수 함수.
//!
//! - [`context_at`]: 캐럿 문장([`crate::statement_at_in`])만 보고 **문맥**을 정한다 — `alias.`/`table.`/`schema.` 뒤 = 멤버 ·
//!   `FROM/JOIN/UPDATE/INTO/DELETE FROM` 뒤 = 관계 · 문장 시작 = 키워드 · 그 밖 = 식(컬럼·함수·키워드·문서 심볼).
//!   문자열/주석 안 = 없음. alias 표(`[schema.]table [AS] alias` · CTE)도 여기서.
//! - [`score`]/[`rank`]: 정확 > 접두 > 단어 경계(`_`·camel) > 포함 > 약어 퍼지(`s_c` → `sales_customer`) · 같은 등급은 MRU > 짧은 것 > 이름.
//!   한글 질의는 자모열([`nsql_core::hangul`]).
//! - 키워드 표 [`KEYWORDS`](ANSI + 방언 공통 · 대문자).

use crate::lexer::{classify, is_ident_char, Class};
use crate::outline::{words, Word};
use crate::split::statement_at_in;
use nsql_core::Dialect;
use std::ops::Range;

/// 캐럿 문맥 종류.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CtxKind {
    /// 문자열·주석 안 — 팝업 없음.
    None,
    /// `qualifier.` 뒤 — qualifier = alias·테이블·스키마·패키지(점 앞 마지막 이름 · `a.b.` 는 전체 `a.b`).
    Member { qualifier: String },
    /// 관계 자리(FROM/JOIN/UPDATE/INTO/DELETE FROM/DESC 뒤) — 테이블·뷰·시노님·스키마·CTE.
    Relation,
    /// 문장 시작(앞에 낱말 없음) — 키워드·명령.
    Start,
    /// 식(SELECT 목록·WHERE·ON·SET …) — 컬럼(alias 표)·함수·키워드·문서 심볼.
    Expr,
}

/// alias 한 줄.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alias {
    pub alias: String,
    pub schema: Option<String>,
    pub table: String,
    /// CTE/서브쿼리(카탈로그에 없음).
    pub local: bool,
}

/// 문맥.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub kind: CtxKind,
    /// 캐럿 앞 식별자 조각(바꿀 글자).
    pub prefix: String,
    /// 바꿀 바이트 구간(`prefix` 자리 · 확정 때 이 구간을 후보로 교체).
    pub replace: Range<usize>,
    pub aliases: Vec<Alias>,
    /// 캐럿 문장 구간(없으면 빈 구간).
    pub statement: Range<usize>,
}

const RELATION_AFTER: &[&str] = &[
    "FROM", "JOIN", "UPDATE", "INTO", "TABLE", "DESC", "DESCRIBE", "TRUNCATE", "USING", "ON",
];
const JOIN_WORDS: &[&str] = &[
    "INNER", "LEFT", "RIGHT", "FULL", "CROSS", "OUTER", "NATURAL", "LATERAL",
];
const NOT_ALIAS: &[&str] = &[
    "WHERE",
    "ON",
    "AND",
    "OR",
    "GROUP",
    "ORDER",
    "HAVING",
    "UNION",
    "INTERSECT",
    "MINUS",
    "EXCEPT",
    "SET",
    "VALUES",
    "SELECT",
    "LIMIT",
    "FETCH",
    "OFFSET",
    "JOIN",
    "INNER",
    "LEFT",
    "RIGHT",
    "FULL",
    "CROSS",
    "OUTER",
    "NATURAL",
    "USING",
    "AS",
    "WITH",
    "START",
    "CONNECT",
    "RETURNING",
    "FOR",
    "INTO",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "CASE",
    "PARTITION",
];

/// ANSI + 방언 공통 키워드(대문자) — `Start`/`Expr` 문맥의 후보(설정 `intel.keywords`).
pub const KEYWORDS: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "GROUP BY",
    "ORDER BY",
    "HAVING",
    "JOIN",
    "INNER JOIN",
    "LEFT JOIN",
    "RIGHT JOIN",
    "FULL JOIN",
    "CROSS JOIN",
    "ON",
    "USING",
    "AS",
    "AND",
    "OR",
    "NOT",
    "IN",
    "EXISTS",
    "BETWEEN",
    "LIKE",
    "IS NULL",
    "IS NOT NULL",
    "DISTINCT",
    "UNION",
    "UNION ALL",
    "INTERSECT",
    "MINUS",
    "EXCEPT",
    "INSERT INTO",
    "VALUES",
    "UPDATE",
    "SET",
    "DELETE FROM",
    "MERGE INTO",
    "WHEN MATCHED",
    "WHEN NOT MATCHED",
    "CREATE",
    "ALTER",
    "DROP",
    "TRUNCATE",
    "TABLE",
    "VIEW",
    "INDEX",
    "SEQUENCE",
    "SYNONYM",
    "PROCEDURE",
    "FUNCTION",
    "PACKAGE",
    "TRIGGER",
    "BEGIN",
    "END",
    "DECLARE",
    "EXCEPTION",
    "RETURN",
    "IF",
    "ELSIF",
    "ELSE",
    "END IF",
    "LOOP",
    "END LOOP",
    "WHILE",
    "FOR",
    "CASE",
    "WHEN",
    "THEN",
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT",
    "GRANT",
    "REVOKE",
    "WITH",
    "RECURSIVE",
    "LIMIT",
    "OFFSET",
    "FETCH FIRST",
    "ROWS ONLY",
    "OVER",
    "PARTITION BY",
    "ROW_NUMBER()",
    "RANK()",
    "DENSE_RANK()",
    "COUNT(*)",
    "SUM(",
    "AVG(",
    "MIN(",
    "MAX(",
    "COALESCE(",
    "NULLIF(",
    "CAST(",
    "CURRENT_DATE",
    "CURRENT_TIMESTAMP",
    "NULL",
    "TRUE",
    "FALSE",
    "ASC",
    "DESC",
    "PRIMARY KEY",
    "FOREIGN KEY",
    "REFERENCES",
    "DEFAULT",
    "CONSTRAINT",
    "UNIQUE",
    "CHECK",
    "EXPLAIN",
    "DESCRIBE",
];

fn is_word(w: &Word<'_>, kw: &str) -> bool {
    !w.quoted && w.text.eq_ignore_ascii_case(kw)
}

fn is_name(w: &Word<'_>) -> bool {
    w.quoted
        || w.text
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
}

/// 캐럿 문맥.
#[must_use]
pub fn context_at(src: &str, caret: usize, dialect: Option<Dialect>) -> Context {
    let caret = caret.min(src.len());
    let b = src.as_bytes();
    // 접두 = 캐럿 앞 식별자 글자들(`:`·`&`·`@` 접두 포함).
    let mut s = caret;
    while s > 0 && (is_ident_char(b[s - 1]) || b[s - 1] == b'$' || b[s - 1] == b'#') {
        s -= 1;
    }
    if s > 0 && matches!(b[s - 1], b':' | b'&' | b'@') && s < caret {
        s -= 1;
    }
    let prefix = src[s..caret].to_string();
    let replace = s..caret;
    let none = Context {
        kind: CtxKind::None,
        prefix: prefix.clone(),
        replace: replace.clone(),
        aliases: Vec::new(),
        statement: 0..0,
    };
    let Some(item) = statement_at_in(src, caret, dialect) else {
        // 문장이 없으면(빈 문서) 시작 문맥.
        return Context {
            kind: CtxKind::Start,
            ..none
        };
    };
    let stmt = item.span;
    // 캐럿이 문장 밖(다음 문장 앞 빈 자리)이면 시작 문맥.
    if caret < stmt.start || caret > stmt.end + 1 {
        return Context {
            kind: CtxKind::Start,
            ..none
        };
    }
    let text = &src[stmt.start..stmt.end.min(src.len())];
    let classes = classify(text);
    let rel = (s.max(stmt.start) - stmt.start).min(classes.len());
    // 접두 바로 앞 글자의 부류 — 문자열/주석이면 팝업 없음.
    if rel > 0 && !matches!(classes[rel - 1], Class::Code | Class::Ident) {
        return none;
    }
    if rel < classes.len()
        && matches!(
            classes[rel],
            Class::Str | Class::LineComment | Class::BlockComment
        )
    {
        return none;
    }
    let ws = words(text, &classes);
    let aliases = alias_table(&ws);
    // 접두 앞의 낱말들(접두 자체는 뺀다).
    let before: Vec<&Word<'_>> = ws.iter().filter(|w| w.end <= rel).collect();
    if before.is_empty() {
        return Context {
            kind: CtxKind::Start,
            prefix,
            replace,
            aliases,
            statement: stmt,
        };
    }
    let last = before[before.len() - 1];
    // `qualifier.` — 점이 접두 바로 앞에 붙어 있을 때만.
    if last.text == "." && last.end == rel {
        let mut q = String::new();
        let mut k = before.len() - 1;
        // `a.b.` 사슬을 거꾸로 모은다.
        while k > 0 {
            let name = before[k - 1];
            if !is_name(name) {
                break;
            }
            if q.is_empty() {
                q = name.text.to_string();
            } else {
                q = format!("{}.{}", name.text, q);
            }
            if k >= 2 && before[k - 2].text == "." {
                k -= 2;
            } else {
                break;
            }
        }
        return Context {
            kind: if q.is_empty() {
                CtxKind::Expr
            } else {
                CtxKind::Member { qualifier: q }
            },
            prefix,
            replace,
            aliases,
            statement: stmt,
        };
    }
    // 관계 자리: 바로 앞 낱말이 FROM/JOIN/… 이거나, 콤마 앞 관계 목록이 이어지는 중.
    let relation = RELATION_AFTER.iter().any(|k| is_word(last, k))
        || (last.text == "," && in_from_list(&before))
        || (JOIN_WORDS.iter().any(|k| is_word(last, k)) && false);
    if relation {
        return Context {
            kind: CtxKind::Relation,
            prefix,
            replace,
            aliases,
            statement: stmt,
        };
    }
    Context {
        kind: CtxKind::Expr,
        prefix,
        replace,
        aliases,
        statement: stmt,
    }
}

/// 콤마 앞이 FROM 목록인가 — 마지막 절 키워드가 FROM/JOIN 계열이면.
fn in_from_list(before: &[&Word<'_>]) -> bool {
    let mut depth = 0i32;
    for w in before.iter().rev() {
        match w.text {
            ")" => depth += 1,
            "(" => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
            }
            _ if depth == 0 => {
                if is_word(w, "FROM") || is_word(w, "JOIN") || is_word(w, "UPDATE") {
                    return true;
                }
                if [
                    "SELECT", "WHERE", "SET", "VALUES", "GROUP", "ORDER", "HAVING", "ON", "INTO",
                ]
                .iter()
                .any(|k| is_word(w, k))
                {
                    return false;
                }
            }
            _ => {}
        }
    }
    false
}

/// alias 표 — `FROM/JOIN/UPDATE/INTO/DELETE FROM` 뒤 `[schema.]table [AS] alias`(콤마·JOIN 반복) + CTE 이름.
#[must_use]
pub fn alias_table(ws: &[Word<'_>]) -> Vec<Alias> {
    let mut out: Vec<Alias> = Vec::new();
    // CTE.
    let mut i = 0;
    while i < ws.len() {
        if is_word(&ws[i], "WITH") || (ws[i].text == "," && out.iter().any(|a| a.local)) {
            let mut k = i + 1;
            if k < ws.len() && is_word(&ws[k], "RECURSIVE") {
                k += 1;
            }
            if let Some(nw) = ws.get(k).filter(|w| is_name(w)) {
                let mut m = k + 1;
                if ws.get(m).is_some_and(|w| w.text == "(") {
                    m = skip_parens(ws, m);
                }
                if ws.get(m).is_some_and(|w| is_word(w, "AS"))
                    && ws.get(m + 1).is_some_and(|w| w.text == "(")
                {
                    out.push(Alias {
                        alias: nw.text.to_string(),
                        schema: None,
                        table: nw.text.to_string(),
                        local: true,
                    });
                }
            }
        }
        i += 1;
    }
    // FROM 목록.
    let mut i = 0;
    while i < ws.len() {
        let w = &ws[i];
        let starts =
            is_word(w, "FROM") || is_word(w, "JOIN") || is_word(w, "UPDATE") || is_word(w, "INTO");
        if !starts {
            i += 1;
            continue;
        }
        let mut k = i + 1;
        loop {
            // 서브쿼리 `( … ) alias`.
            if ws.get(k).is_some_and(|w| w.text == "(") {
                let end = skip_parens(ws, k);
                let mut m = end;
                if ws.get(m).is_some_and(|w| is_word(w, "AS")) {
                    m += 1;
                }
                if let Some(a) = ws
                    .get(m)
                    .filter(|w| is_name(w) && !NOT_ALIAS.iter().any(|x| is_word(w, x)))
                {
                    out.push(Alias {
                        alias: a.text.to_string(),
                        schema: None,
                        table: String::new(),
                        local: true,
                    });
                    m += 1;
                }
                k = m;
            } else {
                let Some(first) = ws
                    .get(k)
                    .filter(|w| is_name(w) && !NOT_ALIAS.iter().any(|x| is_word(w, x)))
                else {
                    break;
                };
                // [schema.]table
                let (schema, table, mut m) = if ws.get(k + 1).is_some_and(|w| w.text == ".")
                    && ws.get(k + 2).is_some_and(is_name)
                {
                    (
                        Some(first.text.to_string()),
                        ws[k + 2].text.to_string(),
                        k + 3,
                    )
                } else {
                    (None, first.text.to_string(), k + 1)
                };
                if ws.get(m).is_some_and(|w| is_word(w, "AS")) {
                    m += 1;
                }
                let alias = match ws.get(m) {
                    Some(a) if is_name(a) && !NOT_ALIAS.iter().any(|x| is_word(a, x)) => {
                        m += 1;
                        a.text.to_string()
                    }
                    _ => table.clone(),
                };
                let local = out
                    .iter()
                    .any(|c| c.local && c.alias.eq_ignore_ascii_case(&table) && c.table == c.alias);
                out.push(Alias {
                    alias,
                    schema,
                    table,
                    local,
                });
                k = m;
            }
            if ws.get(k).is_some_and(|w| w.text == ",") {
                k += 1;
                continue;
            }
            break;
        }
        i = k.max(i + 1);
    }
    out
}

fn skip_parens(ws: &[Word<'_>], open: usize) -> usize {
    let mut d = 0i32;
    let mut p = open;
    while p < ws.len() {
        if ws[p].text == "(" {
            d += 1;
        } else if ws[p].text == ")" {
            d -= 1;
            if d == 0 {
                return p + 1;
            }
        }
        p += 1;
    }
    ws.len()
}

/// alias → (schema, table) 조회(대소문자 무시 · alias 없으면 테이블 이름 자체).
#[must_use]
pub fn resolve_alias<'a>(aliases: &'a [Alias], name: &str) -> Option<&'a Alias> {
    aliases
        .iter()
        .find(|a| a.alias.eq_ignore_ascii_case(name))
        .or_else(|| aliases.iter().find(|a| a.table.eq_ignore_ascii_case(name)))
}

/// 후보 일치 규칙(설정 `intel.match`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchMode {
    Prefix,
    Contains,
    Fuzzy,
}

impl MatchMode {
    #[must_use]
    pub fn parse(s: &str) -> MatchMode {
        match s {
            "prefix" => MatchMode::Prefix,
            "contains" => MatchMode::Contains,
            _ => MatchMode::Fuzzy,
        }
    }
}

/// 후보 점수(높을수록 앞) — 없으면 `None`. 빈 질의 = 전부 같은 점수.
#[must_use]
pub fn score(candidate: &str, query: &str, mode: MatchMode) -> Option<u32> {
    if query.is_empty() {
        return Some(100);
    }
    let c = candidate.to_lowercase();
    let q = query.to_lowercase();
    if c == q {
        return Some(1000);
    }
    if c.starts_with(&q) {
        return Some(900_u32.saturating_sub((c.len() - q.len()).min(100) as u32));
    }
    if mode == MatchMode::Prefix {
        return None;
    }
    // 한글 = 자모열(조합 중 글자도).
    if nsql_core::hangul::has_hangul(&q) {
        let qj = nsql_core::hangul::decompose(&q, true);
        return nsql_core::hangul::contains_jamo(&c, &qj, true).then_some(500);
    }
    // 단어 경계(`_` 뒤 · 숫자→글자 · camel 대문자 앞) 시작.
    let bounds = word_starts(candidate);
    if bounds.iter().any(|&i| c[i..].starts_with(&q)) {
        return Some(700);
    }
    if let Some(p) = c.find(&q) {
        return Some(500_u32.saturating_sub(p.min(100) as u32));
    }
    if mode == MatchMode::Contains {
        return None;
    }
    // 약어 퍼지: 질의 글자들이 단어 시작들에 순서대로 걸리면 높게, 아니면 부분열이면 낮게.
    let starts_text: String = bounds
        .iter()
        .filter_map(|&i| c[i..].chars().next())
        .collect();
    if is_subsequence(&q.replace('_', ""), &starts_text) {
        return Some(400);
    }
    if is_subsequence(&q, &c) {
        return Some(200_u32.saturating_sub((c.len() - q.len()).min(100) as u32));
    }
    None
}

fn word_starts(s: &str) -> Vec<usize> {
    let mut out = vec![0usize];
    let cs: Vec<(usize, char)> = s.char_indices().collect();
    for k in 1..cs.len() {
        let (i, c) = cs[k];
        let (_, p) = cs[k - 1];
        if p == '_' || p == '$' || p == '#' || p == '.' {
            if c != '_' {
                out.push(i);
            }
        } else if (c.is_uppercase() && p.is_lowercase())
            || (c.is_ascii_digit() != p.is_ascii_digit()
                && c.is_alphanumeric()
                && p.is_alphanumeric())
        {
            out.push(i);
        }
    }
    out
}

fn is_subsequence(q: &str, hay: &str) -> bool {
    let mut it = hay.chars();
    'outer: for qc in q.chars() {
        for hc in it.by_ref() {
            if hc == qc {
                continue 'outer;
            }
        }
        return false;
    }
    true
}

/// 후보 종류(아이콘·정렬 그룹).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CandKind {
    Column,
    Alias,
    Table,
    View,
    Schema,
    Routine,
    Package,
    Sequence,
    Synonym,
    Keyword,
    Variable,
    Symbol,
    Word,
}

/// 후보 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cand {
    pub text: String,
    pub kind: CandKind,
    /// 오른쪽 설명(타입 · 스키마 · 종류).
    pub detail: String,
    /// 출처 순위(낮을수록 앞 · docs/29 §6-2: alias 컬럼 1 · 문장 테이블 2 · 스키마 객체 3 · 시스템 4 · 키워드 5 · 문서 6).
    pub source: u8,
}

/// 랭킹 — 점수(일치 등급) → MRU → 출처 순위 → 길이 → 이름. `max`로 자른다.
#[must_use]
pub fn rank(
    cands: Vec<Cand>,
    query: &str,
    mode: MatchMode,
    mru: &[String],
    max: usize,
) -> Vec<Cand> {
    let mut scored: Vec<(u32, usize, Cand)> = cands
        .into_iter()
        .filter_map(|c| {
            let s = score(&c.text, query, mode)?;
            let m = mru
                .iter()
                .position(|x| x.eq_ignore_ascii_case(&c.text))
                .unwrap_or(usize::MAX);
            Some((s, m, c))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.source.cmp(&b.2.source))
            .then(a.2.text.len().cmp(&b.2.text.len()))
            .then(a.2.text.to_lowercase().cmp(&b.2.text.to_lowercase()))
    });
    // 같은 글(대소문자 무시)은 앞 것만(출처가 달라도 — 테이블 `EMP`가 있으면 문서 단어 `emp`는 숨긴다).
    let mut out: Vec<Cand> = Vec::new();
    for (_, _, c) in scored {
        if out.iter().any(|o| o.text.eq_ignore_ascii_case(&c.text)) {
            continue;
        }
        out.push(c);
        if out.len() >= max {
            break;
        }
    }
    out
}

/// 확정 글자의 대소문자(설정 `intel.insert_case`): default = 후보 그대로 · upper/lower · match = 접두가 대문자면 대문자.
#[must_use]
pub fn apply_case(text: &str, prefix: &str, mode: &str) -> String {
    match mode {
        "upper" => text.to_uppercase(),
        "lower" => text.to_lowercase(),
        "match" => {
            if prefix.chars().any(|c| c.is_alphabetic())
                && prefix
                    .chars()
                    .filter(|c| c.is_alphabetic())
                    .all(|c| c.is_uppercase())
            {
                text.to_uppercase()
            } else if prefix.chars().any(|c| c.is_alphabetic())
                && prefix
                    .chars()
                    .filter(|c| c.is_alphabetic())
                    .all(|c| c.is_lowercase())
            {
                text.to_lowercase()
            } else {
                text.to_string()
            }
        }
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(src: &str) -> Context {
        // `|` = 캐럿.
        let caret = src.find('|').expect("caret");
        let text = src.replacen('|', "", 1);
        context_at(&text, caret, Some(Dialect::Oracle))
    }

    #[test]
    fn member_relation_expr_start_and_none() {
        let c = ctx("SELECT a.| FROM sch.emp a, dept d WHERE 1=1");
        assert_eq!(
            c.kind,
            CtxKind::Member {
                qualifier: "a".into()
            }
        );
        assert_eq!(c.prefix, "");
        let a = resolve_alias(&c.aliases, "a").expect("alias a");
        assert_eq!(
            (a.schema.as_deref(), a.table.as_str()),
            (Some("sch"), "emp")
        );
        assert!(resolve_alias(&c.aliases, "d").is_some());
        assert!(
            resolve_alias(&c.aliases, "DEPT").is_some(),
            "테이블 이름으로도"
        );

        let c = ctx("SELECT a.na| FROM emp a");
        assert_eq!(
            c.kind,
            CtxKind::Member {
                qualifier: "a".into()
            }
        );
        assert_eq!(c.prefix, "na");
        assert_eq!(c.replace.len(), 2);

        let c = ctx("SELECT * FROM em|");
        assert_eq!(c.kind, CtxKind::Relation);
        assert_eq!(c.prefix, "em");
        let c = ctx("SELECT * FROM emp e, de|");
        assert_eq!(c.kind, CtxKind::Relation);
        let c = ctx("SELECT * FROM emp e JOIN |");
        assert_eq!(c.kind, CtxKind::Relation);
        let c = ctx("UPDATE |");
        assert_eq!(c.kind, CtxKind::Relation);
        let c = ctx("INSERT INTO |");
        assert_eq!(c.kind, CtxKind::Relation);

        let c = ctx("SELECT id, na| FROM emp");
        assert_eq!(c.kind, CtxKind::Expr);
        let c = ctx("SELECT * FROM emp WHERE de| = 1");
        assert_eq!(c.kind, CtxKind::Expr);

        let c = ctx("|");
        assert_eq!(c.kind, CtxKind::Start);
        let c = ctx("SELECT 1 FROM dual;\nSEL|");
        assert_eq!(c.kind, CtxKind::Start);
        assert_eq!(c.prefix, "SEL");

        let c = ctx("SELECT 'a.|' FROM dual");
        assert_eq!(c.kind, CtxKind::None);
        let c = ctx("SELECT 1 -- from x.|\nFROM dual");
        assert_eq!(c.kind, CtxKind::None);

        let c = ctx("SELECT sch.tab.| FROM dual");
        assert_eq!(
            c.kind,
            CtxKind::Member {
                qualifier: "sch.tab".into()
            }
        );
        let c = ctx("SELECT :v| FROM dual");
        assert_eq!(c.prefix, ":v");
    }

    #[test]
    fn aliases_with_cte_subquery_and_joins() {
        let src = "WITH t AS (SELECT 1 x FROM dual) SELECT * FROM t, (SELECT 2 y FROM dual) s LEFT JOIN sch.emp AS e ON e.id = s.y WHERE |";
        let c = ctx(src);
        let names: Vec<(&str, &str, bool)> = c
            .aliases
            .iter()
            .map(|a| (a.alias.as_str(), a.table.as_str(), a.local))
            .collect();
        assert!(names.contains(&("t", "t", true)), "{names:?}");
        assert!(names.contains(&("s", "", true)), "{names:?}");
        assert!(names.contains(&("e", "emp", false)), "{names:?}");
        assert!(!names.iter().any(|(a, _, _)| *a == "ON" || *a == "WHERE"));
    }

    #[test]
    fn scoring_and_ranking() {
        assert_eq!(score("emp", "emp", MatchMode::Fuzzy), Some(1000));
        assert!(
            score("employee", "emp", MatchMode::Prefix).unwrap()
                > score("sales_emp", "emp", MatchMode::Fuzzy).unwrap()
        );
        assert_eq!(score("sales_emp", "emp", MatchMode::Prefix), None);
        assert_eq!(
            score("sales_emp", "emp", MatchMode::Contains),
            Some(700),
            "단어 경계"
        );
        assert_eq!(score("resample", "amp", MatchMode::Contains), Some(497));
        assert_eq!(
            score("sales_customer", "sc", MatchMode::Fuzzy),
            Some(400),
            "약어"
        );
        assert_eq!(score("sales_customer", "sc", MatchMode::Contains), None);
        assert_eq!(
            score("sales_customer", "s_c", MatchMode::Contains),
            Some(496),
            "부분 문자열"
        );
        assert!(
            score("MyTableName", "mtn", MatchMode::Fuzzy).is_some(),
            "camel 약어"
        );
        assert_eq!(score("abc", "zz", MatchMode::Fuzzy), None);
        assert_eq!(score("고객_테이블", "ㄱ", MatchMode::Contains), Some(500));
        let cands = vec![
            Cand {
                text: "emp_no".into(),
                kind: CandKind::Column,
                detail: "".into(),
                source: 1,
            },
            Cand {
                text: "EMP".into(),
                kind: CandKind::Table,
                detail: "".into(),
                source: 3,
            },
            Cand {
                text: "employees".into(),
                kind: CandKind::Table,
                detail: "".into(),
                source: 3,
            },
            Cand {
                text: "temp".into(),
                kind: CandKind::Word,
                detail: "".into(),
                source: 6,
            },
            Cand {
                text: "emp".into(),
                kind: CandKind::Word,
                detail: "".into(),
                source: 6,
            },
        ];
        let r = rank(cands, "emp", MatchMode::Fuzzy, &["employees".into()], 10);
        let texts: Vec<&str> = r.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts[0], "EMP", "정확 일치 · 같은 글의 Word는 제거");
        assert_eq!(texts[1], "emp_no");
        assert_eq!(texts[2], "employees");
        assert_eq!(texts[3], "temp");
        assert_eq!(apply_case("employees", "EMP", "match"), "EMPLOYEES");
        assert_eq!(apply_case("employees", "Emp", "match"), "employees");
        assert_eq!(apply_case("employees", "x", "upper"), "EMPLOYEES");
    }
}
