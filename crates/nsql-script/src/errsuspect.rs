//! ★ T-372 오류 위치 고급 분석(132차 win · 사용자 10-20 "오류 줄의 어느 위치가 원인인지 다시 검토").
//!
//! 서버가 알려 주는 구문 오류 위치는 **파서가 포기한 지점**이라 원인(키워드 오타)보다 뒤에 있을 수 있다 —
//! `SELECT DISTINC a.x FROM t`는 `DISTINC`가 식으로, `a`가 별칭으로 읽히고 `.`에서 멈춘다. 원인은 늘 서버 위치와 같거나 그 **앞**에 있으므로
//! 구간 = [문장 시작, 서버 위치]만 다시 본다(이 범위 제한이 비용의 핵심 · 문장 하나 이하).
//!
//! 방식 = 파서가 아니라 **의심 지점 찾기**: 구간의 낱말 가운데 ① 정확한 키워드가 아니고 ② 메타(캐시)에 있는 객체·컬럼·별칭도 아니며
//! ③ 그 자리에서 기대되는 키워드(문법 표 `.sqlg` `next(절)` · 절 이름 · 문장 시작 낱말)와 편집 거리(OSA · 전위 포함)가 가까운 것.
//! 결과는 **위치 순**(호스트 = 첫 번째로 캐럿 · 사용자 10-20) · 서버 왕복 0(메타는 [`Known`] 포트로 캐시만 · 모르면 "틀림"이 아니라 그대로
//! 후보) · 예산([`Input::deadline`]) 넘으면 그때까지 찾은 것만. 적용 대상은 호스트가 **구문 오류 부류**(`ErrorClass::Syntax`)로 한정한다 —
//! 식별자 오류(ORA-00904 등)는 서버 위치가 이미 정확하다.
//!
//! 의존 0(nsql-core만) · 순수 함수 · 시험 = 아래 `tests`(구간 경계 · 임계 · 메타 거르기 · 순서 · 예산).

use crate::grammar;
use crate::intel::{alias_table, keywords_for};
use crate::lexer::classify;
use crate::outline::{words, Word};
use nsql_core::Dialect;
use std::collections::HashSet;
use std::time::Instant;

/// 메타 조회 포트 — 호스트의 메타 **스냅숏**(캐시)만 본다. 서버에 묻지 않는다.
pub trait Known {
    /// 어느 스키마든 그 이름의 객체(테이블·뷰·루틴·패키지…)가 메타에 있는가.
    fn has_object(&self, name: &str) -> bool;
    /// 그 관계에 이 컬럼이 있는가 — `None` = 컬럼 목록을 아직 모른다(판정 보류 = 후보로 남긴다).
    fn has_column(&self, schema: Option<&str>, table: &str, col: &str) -> Option<bool>;
}

/// 메타 없음(CLI · 시험).
#[derive(Debug, Default)]
pub struct NothingKnown;

impl Known for NothingKnown {
    fn has_object(&self, _name: &str) -> bool {
        false
    }
    fn has_column(&self, _schema: Option<&str>, _table: &str, _col: &str) -> Option<bool> {
        None
    }
}

/// 분석 입력.
#[derive(Clone, Copy, Debug)]
pub struct Input<'a> {
    /// 실패한 문장(드라이버에 보낸 글 · 위치 단위의 기준).
    pub sql: &'a str,
    pub dialect: Option<Dialect>,
    /// 서버가 알려 준 위치 = `sql` 기준 **글자** 인덱스(0부터 · `DbError.position`) · 없으면 `None`(구간 = 문장 전체).
    pub server_pos: Option<usize>,
    /// `server_pos`가 **줄 시작**인가(SQL Server = 줄 번호만) — 구간 끝 = 그 줄에서 `near` 토큰 · 없으면 그 줄 끝.
    pub pos_is_line: bool,
    /// 서버 메시지의 `near "X"`/`near 'X'` 토큰([`near_token`]) — 구간 끝 보정(토큰까지 포함).
    pub near: Option<&'a str>,
    /// 예산(지나면 그때까지 찾은 것만 · `None` = 없음).
    pub deadline: Option<Instant>,
}

/// 의심 지점 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suspect {
    /// `sql` 기준 글자 구간(0부터 · 끝 제외).
    pub start: usize,
    pub end: usize,
    /// 원문 낱말.
    pub text: String,
    /// 가장 가까운 기대 키워드(대문자).
    pub expect: String,
    /// 편집 거리(1~2).
    pub dist: u8,
}

/// 서버 메시지의 `near "X"`(PostgreSQL · SQLite) / `near 'X'`(SQL Server) 토큰. 없으면 `None`.
#[must_use]
pub fn near_token(message: &str) -> Option<String> {
    let low = message.to_ascii_lowercase();
    let at = low.find("near ")? + 5;
    let rest = message.get(at..)?.trim_start();
    let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let inner = &rest[q.len_utf8()..];
    let end = inner.find(q)?;
    let tok = inner[..end].trim();
    (!tok.is_empty()).then(|| tok.to_string())
}

/// 구간 안의 의심 지점(위치 순). 비어 있으면 호스트는 서버 위치를 그대로 쓴다.
#[must_use]
pub fn analyze(input: &Input<'_>, known: &dyn Known) -> Vec<Suspect> {
    let sql = input.sql;
    if sql.is_empty() || expired(input.deadline) {
        return Vec::new();
    }
    let classes = classify(sql);
    let ws = words(sql, &classes);
    if ws.is_empty() {
        return Vec::new();
    }
    let end_b = window_end(sql, &ws, input);
    let g = grammar::for_dialect(input.dialect);

    // 정확한 키워드 집합(= 오타 아님) — 넓게: 완성 키워드 표 + 문법 절 이름 + 문장 시작 + 보조 표.
    let mut exact: HashSet<String> = HashSet::new();
    for k in keywords_for(input.dialect) {
        add_words(&mut exact, k);
    }
    for c in g.clause_names() {
        add_words(&mut exact, &c);
    }
    for s in g.start() {
        add_words(&mut exact, s);
    }
    for k in EXTRA_EXACT {
        exact.insert((*k).to_string());
    }
    // 기대 키워드 후보(= 오타의 목표) — 좁게: 절 이름 + 문장 시작 + 핵심 표(4글자 이상) + 현재 절의 `next`.
    let mut base_targets: Vec<String> = Vec::new();
    for c in g.clause_names() {
        push_target_words(&mut base_targets, &c);
    }
    for s in g.start() {
        push_target_words(&mut base_targets, s);
    }
    for k in CORE_TARGETS {
        push_target_words(&mut base_targets, k);
    }

    let aliases = alias_table(&ws);
    // ★ 별칭은 **한정자로 쓰였을 때만**(`FORM.x`) 보호한다 — 테이블 뒤 키워드 오타(`FROM t WHRE a = 1`)는 별칭 표에 별칭으로 들어오므로
    //   선언만으로는 가리지 않는다(`AS x`는 `NAME_AFTER`가 따로 뺀다).
    let used_quals: HashSet<String> = ws
        .windows(2)
        .filter(|p| p[1].text == "." && !p[0].quoted)
        .map(|p| p[0].text.to_ascii_uppercase())
        .collect();
    let alias_names: HashSet<String> = aliases
        .iter()
        .map(|a| a.alias.to_ascii_uppercase())
        .filter(|a| used_quals.contains(a))
        .collect();

    let mut out: Vec<Suspect> = Vec::new();
    // 절 추적(괄호 깊이별 스택 · 서브쿼리 안은 그 안의 절).
    let mut clause: Option<String> = None;
    let mut stack: Vec<Option<String>> = Vec::new();
    let names_set = g.clause_names();
    let mut scanned = 0usize;
    for (i, w) in ws.iter().enumerate() {
        if w.start >= end_b {
            break;
        }
        scanned += 1;
        if scanned.is_multiple_of(64) && expired(input.deadline) {
            break;
        }
        if scanned > MAX_WORDS {
            break;
        }
        if w.quoted {
            continue;
        }
        match w.text {
            "(" => {
                stack.push(clause.take());
                continue;
            }
            ")" => {
                clause = stack.pop().flatten();
                continue;
            }
            _ => {}
        }
        // 절 이름(1~3 낱말 · 이 낱말에서 끝남)?
        if let Some(c) = clause_ending_at(&ws, i, &names_set) {
            clause = Some(c);
            continue;
        }
        let up = w.text.to_ascii_uppercase();
        if !is_candidate_word(&ws, i) || exact.contains(&up) {
            continue;
        }
        let len = w.text.chars().count();
        if len < MIN_WORD_LEN {
            continue;
        }
        let max_dist: u8 = if len >= 7 { 2 } else { 1 };
        // 기대 키워드 = 현재 절의 next(우선) → 기본 후보.
        let mut best: Option<(u8, u8, String)> = None; // (dist, 우선순위 0=next, 1=기본, kw)
        if let Some(next) = clause.as_deref().and_then(|c| g.next(c)) {
            for n in next {
                for t in n.split_whitespace() {
                    consider(&mut best, &up, t, max_dist, 0);
                }
            }
        }
        for t in &base_targets {
            consider(&mut best, &up, t, max_dist, 1);
        }
        let Some((dist, _, expect)) = best else {
            continue;
        };
        if before_paren(&ws, i) && !PAREN_OK.contains(&expect.as_str()) {
            continue;
        }
        // 메타·별칭에 있는 이름은 오타가 아니다(캐시만 · 모르면 후보 유지).
        if alias_names.contains(&up) || known.has_object(w.text) {
            continue;
        }
        if aliases.iter().any(|a| {
            !a.local && known.has_column(a.schema.as_deref(), &a.table, w.text) == Some(true)
        }) {
            continue;
        }
        out.push(Suspect {
            start: char_at(sql, w.start),
            end: char_at(sql, w.end),
            text: w.text.to_string(),
            expect,
            dist,
        });
    }
    out.sort_by_key(|s| s.start);
    out
}

/// 낱말 수 상한(문장 하나 이하라도 수만 낱말이면 예산 전에 끊는다).
const MAX_WORDS: usize = 20_000;
/// 의심 낱말·기대 키워드 최소 길이(3글자는 짝이 너무 많다 · `SEQ`↔`SET`).
const MIN_WORD_LEN: usize = 4;

/// 정확한 키워드 보조 표 — 완성 표에 없는 구조 낱말(오타 판정에서 제외하기 위한 것 · 넓어도 된다).
const EXTRA_EXACT: &[&str] = &[
    "ALL",
    "ANY",
    "SOME",
    "INTO",
    "BY",
    "OUTER",
    "LEFT",
    "RIGHT",
    "FULL",
    "INNER",
    "CROSS",
    "NATURAL",
    "NULLS",
    "FIRST",
    "LAST",
    "ROWS",
    "ROW",
    "ONLY",
    "NEXT",
    "FETCH",
    "TO",
    "ESCAPE",
    "CAST",
    "CURRENT",
    "OF",
    "MATCHED",
    "DELETE",
    "INSERT",
    "MERGE",
    "REPLACE",
    "RENAME",
    "TEMPORARY",
    "TEMP",
    "GLOBAL",
    "LOCAL",
    "COLUMN",
    "ADD",
    "MODIFY",
    "INTERVAL",
    "DATE",
    "TIMESTAMP",
    "TIME",
    "LEVEL",
    "ROLLUP",
    "CUBE",
    "GROUPING",
    "SETS",
    "WINDOW",
    "RANGE",
    "PRECEDING",
    "FOLLOWING",
    "UNBOUNDED",
    "ELSEIF",
    "EXIT",
    "CONTINUE",
    "RAISE",
    "RETURNING",
    "PRIOR",
    "NOCYCLE",
    "SIBLINGS",
    "AFTER",
    "BEFORE",
    "EACH",
    "REFERENCING",
    "INSTEAD",
    "APPLY",
    "PERCENT",
    "TIES",
    "EXECUTE",
    "USE",
    "CONFLICT",
    "NOTHING",
    "DO",
    "WITHOUT",
    "ROWID",
    "SAMPLE",
    "PARTITION",
    "SUBPARTITION",
    "KEEP",
    "WITHIN",
    "GROUP",
    "ORDER",
    "IS",
    "IF",
    "KEY",
    "PRIMARY",
    "FOREIGN",
    "NOLOCK",
    "OUTPUT",
    "AT",
    "ZONE",
    "YEAR",
    "MONTH",
    "DAY",
    "HOUR",
    "MINUTE",
    "SECOND",
    "CHAR",
    "VARCHAR",
    "VARCHAR2",
    "NVARCHAR",
    "NUMBER",
    "NUMERIC",
    "DECIMAL",
    "INT",
    "INTEGER",
    "BIGINT",
    "SMALLINT",
    "FLOAT",
    "REAL",
    "DOUBLE",
    "PRECISION",
    "BOOLEAN",
    "TEXT",
    "BLOB",
    "CLOB",
    "BINARY",
    "VARBINARY",
    "BIT",
    "TRUE",
    "FALSE",
    "UNKNOWN",
    "CONSTRAINT",
    "CASCADE",
    "RESTRICT",
    "NO",
    "ACTION",
    "ENABLE",
    "DISABLE",
    "VALIDATE",
    "NOVALIDATE",
    "COMPILE",
    "BODY",
    "SPECIFICATION",
    "AUTHID",
    "DEFINER",
    "CURRENT_USER",
    "SESSION",
    "SYSTEM",
    "LOCK",
    "SHARE",
    "MODE",
    "NOWAIT",
    "WAIT",
    "SKIP",
    "LOCKED",
    "FORCE",
    "EDITIONABLE",
    "NONEDITIONABLE",
    "OVERRIDING",
    "GENERATED",
    "ALWAYS",
    "IDENTITY",
    "START",
    "INCREMENT",
    "MINVALUE",
    "MAXVALUE",
    "CACHE",
    "NOCACHE",
    "CYCLE",
    "ORDER",
    "PIVOT",
    "UNPIVOT",
    "MODEL",
    "CONNECT",
    "LATERAL",
    "ARRAY",
    "UNNEST",
    "ILIKE",
    "GLOB",
    "PRAGMA",
    "VACUUM",
    "ATTACH",
    "DETACH",
    "AUTOINCREMENT",
    "TOP",
    "GO",
    "EXEC",
    "DISTINCT",
    "UNIQUE",
    "WHERE",
    "FROM",
    "SELECT",
    "HAVING",
    "UNION",
    "INTERSECT",
    "EXCEPT",
    "MINUS",
    "LIMIT",
    "OFFSET",
    "JOIN",
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
    "NULL",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "ASC",
    "DESC",
    "SET",
    "VALUES",
    "UPDATE",
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
    "DECLARE",
    "EXCEPTION",
    "RETURN",
    "ELSIF",
    "LOOP",
    "WHILE",
    "FOR",
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT",
    "GRANT",
    "REVOKE",
    "WITH",
    "RECURSIVE",
    "OVER",
    "REFERENCES",
    "DEFAULT",
    "CHECK",
    "EXPLAIN",
    "DESCRIBE",
    "COLLATE",
    "TYPE",
    "TYPES",
    "OBJECT",
    "VARRAY",
    "RECORD",
    "CURSOR",
    "OPEN",
    "CLOSE",
    "ROWTYPE",
    "SQL",
    "PLAN",
    "ANALYZE",
    "COMMENT",
    "COLUMNS",
    "STATISTICS",
    "COMPUTE",
    "ESTIMATE",
    "RENAME",
    "GOTO",
    "PRINT",
    "VARIABLE",
    "SERVEROUTPUT",
    "TRANSACTION",
    "WORK",
    "READ",
    "WRITE",
    "COMMITTED",
    "SERIALIZABLE",
    "ISOLATION",
    "ACCESS",
    "EXCLUSIVE",
    "ALL_ROWS",
    "FIRST_ROWS",
    "PARALLEL",
    "NOLOGGING",
    "LOGGING",
    "TABLESPACE",
    "STORAGE",
    "INITIAL",
    "PCTFREE",
    "PCTUSED",
    "INITRANS",
    "MAXTRANS",
    "COMPRESS",
    "NOCOMPRESS",
    "ONLINE",
    "OFFLINE",
    "REBUILD",
    "UNUSABLE",
    "USABLE",
    "MONITORING",
    "NOMONITORING",
];

/// 기대 키워드 핵심 표(절 이름·문장 시작과 함께 오타의 **목표**가 되는 것 · 4글자 이상만 쓰인다).
const CORE_TARGETS: &[&str] = &[
    "DISTINCT",
    "NULL",
    "LIKE",
    "BETWEEN",
    "EXISTS",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "DESC",
    "USING",
    "VALUES",
    "INTO",
    "JOIN",
    "INNER",
    "LEFT",
    "RIGHT",
    "FULL",
    "OUTER",
    "CROSS",
    "ORDER",
    "GROUP",
    "HAVING",
    "WHERE",
    "FROM",
    "SELECT",
    "UNION",
    "INSERT",
    "UPDATE",
    "DELETE",
    "CREATE",
    "ALTER",
    "DROP",
    "TABLE",
    "VIEW",
    "INDEX",
    "LIMIT",
    "OFFSET",
    "FETCH",
    "FIRST",
    "NEXT",
    "ROWS",
    "ONLY",
    "RETURNING",
    "WITH",
    "PARTITION",
    "OVER",
    "ASC",
    "UNIQUE",
    "PRIMARY",
    "FOREIGN",
    "REFERENCES",
    "DEFAULT",
    "CONSTRAINT",
    "CHECK",
    "BEGIN",
    "DECLARE",
    "EXCEPTION",
    "RETURN",
    "LOOP",
    "WHILE",
    "COMMIT",
    "ROLLBACK",
    "GRANT",
    "REVOKE",
    "TRUNCATE",
    "MERGE",
    "MATCHED",
    "EXCEPT",
    "INTERSECT",
    "MINUS",
    "CONNECT",
    "START",
    "PRIOR",
    "NULLS",
    "LAST",
];

fn expired(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|d| Instant::now() >= d)
}

fn add_words(set: &mut HashSet<String>, phrase: &str) {
    for t in phrase.split_whitespace() {
        let t = t.trim_matches(|c| c == '(' || c == ')' || c == ',');
        if !t.is_empty() {
            set.insert(t.to_ascii_uppercase());
        }
    }
}

fn push_target_words(v: &mut Vec<String>, phrase: &str) {
    for t in phrase.split_whitespace() {
        let t = t.trim_matches(|c| c == '(' || c == ')' || c == ',');
        if t.chars().count() >= MIN_WORD_LEN
            && t.bytes().all(|b| b.is_ascii_alphabetic() || b == b'_')
        {
            let up = t.to_ascii_uppercase();
            if !v.contains(&up) {
                v.push(up);
            }
        }
    }
}

fn consider(best: &mut Option<(u8, u8, String)>, word: &str, target: &str, max_dist: u8, prio: u8) {
    if target.chars().count() < MIN_WORD_LEN {
        return;
    }
    let t = target.to_ascii_uppercase();
    if t == word {
        return;
    }
    // 길이 차가 임계를 넘으면 거리도 넘는다(빠른 탈락).
    let (lw, lt) = (word.chars().count(), t.chars().count());
    if lw.abs_diff(lt) > usize::from(max_dist) {
        return;
    }
    let d = osa_distance(word, &t);
    if d == 0 || d > usize::from(max_dist) {
        return;
    }
    let d = d as u8;
    // 동률 = 공통 접두가 긴 쪽(`WHER` → WHERE(4) > WHEN(3)) → 현재 절의 next 우선.
    let better = match best {
        None => true,
        Some((bd, bp, bt)) => {
            d < *bd
                || (d == *bd && {
                    let (pn, po) = (common_prefix(word, &t), common_prefix(word, bt));
                    pn > po || (pn == po && prio < *bp)
                })
        }
    };
    if better {
        *best = Some((d, prio, t));
    }
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// 이 낱말이 오타 후보가 될 자리인가 — 인용·바인드·숫자·한정 멤버(`a.x`)·호출(`f(`)·`AS x`·이름 자리(`FROM x` 등)는 아니다.
fn is_candidate_word(ws: &[Word<'_>], i: usize) -> bool {
    let w = &ws[i];
    let first = w.text.as_bytes().first().copied().unwrap_or(0);
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    if let Some(p) = i.checked_sub(1).map(|k| &ws[k]) {
        if p.text == "." {
            return false;
        }
        if !p.quoted && NAME_AFTER.iter().any(|k| p.text.eq_ignore_ascii_case(k)) {
            return false;
        }
    }
    if let Some(n) = ws.get(i + 1) {
        if n.text == "." {
            return false;
        }
    }
    true
}

/// 뒤에 `(`가 와도 되는 기대 키워드(`VALUES (` · `EXISTS (` · `IN (` …) — 그 밖의 `낱말(`은 함수 호출이라 오타로 보지 않는다.
const PAREN_OK: &[&str] = &[
    "VALUES",
    "EXISTS",
    "USING",
    "OVER",
    "PARTITION",
    "WITHIN",
    "KEEP",
];

fn before_paren(ws: &[Word<'_>], i: usize) -> bool {
    ws.get(i + 1).is_some_and(|n| n.text == "(")
}

/// 뒤에 **이름**이 오는 낱말(그 자리의 낱말은 식별자이므로 키워드 오타로 보지 않는다).
const NAME_AFTER: &[&str] = &[
    "AS",
    "FROM",
    "JOIN",
    "INTO",
    "UPDATE",
    "TABLE",
    "VIEW",
    "INDEX",
    "SEQUENCE",
    "SYNONYM",
    "PROCEDURE",
    "FUNCTION",
    "PACKAGE",
    "TRIGGER",
    "TYPE",
    "SCHEMA",
    "DATABASE",
    "USE",
    "EXEC",
    "EXECUTE",
    "CALL",
    "DESC",
    "DESCRIBE",
    "TRUNCATE",
    "RENAME",
    "TO",
    "ON",
    "COLUMN",
    "CONSTRAINT",
    "REFERENCES",
    "DECLARE",
    "VARIABLE",
    "DEFINE",
    "CURSOR",
    "GO",
];

/// `ws[i]`에서 끝나는 절 이름(3→1 낱말 · 인용 아님 · 대문자) — 문법 절 집합에 있으면 그 이름.
fn clause_ending_at(ws: &[Word<'_>], i: usize, names: &HashSet<String>) -> Option<String> {
    for n in (1..=3).rev() {
        if i + 1 < n {
            continue;
        }
        let slice = &ws[i + 1 - n..=i];
        if slice.iter().any(|w| w.quoted) {
            continue;
        }
        let cand = slice
            .iter()
            .map(|w| w.text.to_ascii_uppercase())
            .collect::<Vec<_>>()
            .join(" ");
        if names.contains(&cand) {
            return Some(cand);
        }
    }
    None
}

/// 구간 끝(바이트 · 끝 제외) — 서버 위치 종류별.
fn window_end(sql: &str, ws: &[Word<'_>], input: &Input<'_>) -> usize {
    let near_word = |from: usize| {
        input.near.and_then(|n| {
            ws.iter()
                .find(|w| w.start >= from && w.text.eq_ignore_ascii_case(n))
                .map(|w| w.end)
        })
    };
    match input.server_pos {
        Some(p) => {
            let pb = byte_at(sql, p);
            if input.pos_is_line {
                near_word(pb).unwrap_or_else(|| sql[pb..].find('\n').map_or(sql.len(), |k| pb + k))
            } else {
                // 서버 위치의 낱말까지 포함(그 낱말 자체가 오타일 수 있다 · `SELEC 1`).
                ws.iter()
                    .find(|w| w.start <= pb && pb < w.end)
                    .map_or(pb, |w| w.end)
            }
        }
        None => near_word(0).unwrap_or(sql.len()),
    }
}

fn byte_at(s: &str, ch: usize) -> usize {
    s.char_indices().nth(ch).map_or(s.len(), |(b, _)| b)
}

fn char_at(s: &str, byte: usize) -> usize {
    s[..byte.min(s.len())].chars().count()
}

/// 최적 문자열 정렬 거리(OSA = Damerau–Levenshtein 제한형 · 인접 전위 = 1). 대문자 비교는 호출자가 맞춘다.
#[must_use]
pub fn osa_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, v) in d[0].iter_mut().enumerate() {
        *v = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = v;
        }
    }
    d[n][m]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn input(sql: &str, pos: Option<usize>) -> Input<'_> {
        Input {
            sql,
            dialect: Some(Dialect::Oracle),
            server_pos: pos,
            pos_is_line: false,
            near: None,
            deadline: None,
        }
    }

    fn pos_of(sql: &str, needle: &str) -> usize {
        sql[..sql.find(needle).unwrap()].chars().count()
    }

    #[test]
    fn osa_counts_transposition_as_one() {
        assert_eq!(osa_distance("FORM", "FROM"), 1);
        assert_eq!(osa_distance("DISTINC", "DISTINCT"), 1);
        assert_eq!(osa_distance("SELCT", "SELECT"), 1);
        assert_eq!(osa_distance("WHRE", "WHERE"), 1);
        assert_eq!(osa_distance("DISTNCTT", "DISTINCT"), 2);
        assert_eq!(osa_distance("ABC", "ABC"), 0);
        assert_eq!(osa_distance("", "AB"), 2);
    }

    /// 사용자 사례(10-20): `DISTINC` 오타 → Oracle은 뒤의 리터럴/별칭에서 멈춘다 → 의심 = 앞의 `DISTINC`.
    #[test]
    fn distinct_typo_before_server_position() {
        let sql = "SELECT DISTINC '가나다' AS x FROM dual";
        let out = analyze(&input(sql, Some(pos_of(sql, "'가나다'"))), &NothingKnown);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].text, "DISTINC");
        assert_eq!(out[0].expect, "DISTINCT");
        assert_eq!((out[0].start, out[0].end), (7, 14));
        assert_eq!(out[0].dist, 1);
    }

    /// 구간 = [시작, 서버 위치] — 서버 위치 뒤 낱말은 보지 않는다 · 위치의 낱말 자체는 포함.
    #[test]
    fn window_ends_at_server_position_inclusive_of_its_word() {
        let sql = "SELECT DISTINC a, b FORM t";
        // 서버 위치 = `b`(FORM 앞) → DISTINC만.
        let out = analyze(&input(sql, Some(pos_of(sql, "b FORM"))), &NothingKnown);
        assert_eq!(
            out.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            vec!["DISTINC"]
        );
        // 서버 위치 = `FORM` 시작 → FORM 포함 → 둘 · 위치 순.
        let out = analyze(&input(sql, Some(pos_of(sql, "FORM"))), &NothingKnown);
        assert_eq!(
            out.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            vec!["DISTINC", "FORM"],
            "첫 번째 = 가장 앞(사용자 10-20)"
        );
        assert_eq!(out[1].expect, "FROM");
        // 서버 위치 없음 = 문장 전체.
        let out = analyze(&input(sql, None), &NothingKnown);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn exact_keywords_and_short_words_are_not_suspects() {
        assert!(analyze(
            &input("SELECT DISTINCT a FROM t WHERE b = 1", None),
            &NothingKnown
        )
        .is_empty());
        // 3글자는 보지 않는다(`FRM`·`ADN`).
        assert!(analyze(&input("SELECT a FRM t", None), &NothingKnown).is_empty());
        // 7글자 이상은 거리 2까지.
        let out = analyze(&input("SELECT DISTNCTT a FROM t", None), &NothingKnown);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].dist, 2);
        // 6글자 이하는 거리 1까지(`SELCCT`→SELECT = 1 ✓ · `SXLCCT` = 2 ✗).
        assert_eq!(
            analyze(&input("SELCCT a FROM t", None), &NothingKnown).len(),
            1
        );
        assert!(analyze(&input("SXLCCT a FROM t", None), &NothingKnown).is_empty());
    }

    struct Meta {
        objects: Vec<&'static str>,
        cols: Vec<(&'static str, &'static str)>,
    }
    impl Known for Meta {
        fn has_object(&self, name: &str) -> bool {
            self.objects.iter().any(|o| o.eq_ignore_ascii_case(name))
        }
        fn has_column(&self, _schema: Option<&str>, table: &str, col: &str) -> Option<bool> {
            let known_table = self.cols.iter().any(|(t, _)| t.eq_ignore_ascii_case(table));
            known_table.then(|| {
                self.cols
                    .iter()
                    .any(|(t, c)| t.eq_ignore_ascii_case(table) && c.eq_ignore_ascii_case(col))
            })
        }
    }

    /// MC/DC(메타 거르기): 같은 문장에서 ① 메타 없음 = 후보 ② 컬럼으로 알려짐 = 제외 ③ 객체로 알려짐 = 제외 ④ 컬럼 목록 모름 = 후보 유지.
    #[test]
    fn known_names_from_metadata_cache_are_not_suspects() {
        let sql = "SELECT FORM FROM t WHERE x = 1";
        let none = analyze(&input(sql, None), &NothingKnown);
        assert_eq!(none.len(), 1, "① 메타 없음 = FORM 후보");
        let col = Meta {
            objects: vec![],
            cols: vec![("t", "FORM")],
        };
        assert!(
            analyze(&input(sql, None), &col).is_empty(),
            "② t의 컬럼 FORM"
        );
        let obj = Meta {
            objects: vec!["FORM"],
            cols: vec![],
        };
        assert!(analyze(&input(sql, None), &obj).is_empty(), "③ 객체 FORM");
        let other = Meta {
            objects: vec![],
            cols: vec![("t", "OTHER")],
        };
        assert_eq!(
            analyze(&input(sql, None), &other).len(),
            1,
            "컬럼 목록이 있는데 없음 = 후보"
        );
        let unknown_cols = Meta {
            objects: vec![],
            cols: vec![("u", "FORM")],
        };
        assert_eq!(
            analyze(&input(sql, None), &unknown_cols).len(),
            1,
            "④ t의 컬럼 목록 모름 = 후보 유지"
        );
    }

    #[test]
    fn aliases_qualified_members_calls_and_name_positions_are_skipped() {
        // 별칭 FORM(`FROM t FORM`) · 한정 멤버 `FORM.x` → 둘 다 제외.
        assert!(analyze(
            &input("SELECT FORM.x FROM t FORM WHERE FORM.y = 1", None),
            &NothingKnown
        )
        .is_empty());
        // 선언만 된 별칭이 키워드 오타 꼴이면 후보(`FROM t WHRE a = 1`의 WHRE는 별칭으로 읽힌다 · 한정자로 쓰인 별칭만 보호).
        let out = analyze(&input("SELECT a FROM t WHRE a = 1", None), &NothingKnown);
        assert_eq!(
            out.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            vec!["WHRE"]
        );
        // 호출 `FORM(` · `AS FORM` · `FROM FORM`(테이블 자리) → 제외.
        assert!(analyze(
            &input("SELECT FORM(1), a AS FORM FROM FORM", None),
            &NothingKnown
        )
        .is_empty());
        // 바인드·숫자 → 제외.
        assert!(analyze(&input("SELECT :FORM, 1234 FROM t", None), &NothingKnown).is_empty());
    }

    /// 절 추적: 앞선 오타로 절 인식이 깨져도 절 이름 낱말은 늘 목표에 있다(`FORM … WHER`).
    #[test]
    fn clause_keywords_stay_targets_after_an_earlier_typo() {
        let out = analyze(&input("SELECT a FORM t WHER b = 1", None), &NothingKnown);
        assert_eq!(
            out.iter()
                .map(|s| (s.text.as_str(), s.expect.as_str()))
                .collect::<Vec<_>>(),
            vec![("FORM", "FROM"), ("WHER", "WHERE")]
        );
    }

    /// 서브쿼리 안의 오타도 잡힌다(괄호 스택).
    #[test]
    fn subquery_typo_is_found() {
        let out = analyze(
            &input("SELECT a FROM (SELECT DISTINC b FROM t) q", None),
            &NothingKnown,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "DISTINC");
    }

    /// MC/DC(줄 위치 · SQL Server): 서버 위치 = 줄 시작 → 구간 = 그 줄(near 토큰까지 · 없으면 줄 끝). 다른 줄의 오타는 보지 않는다.
    #[test]
    fn line_granular_position_limits_window_to_that_line() {
        let sql = "SELECT 1\nFORM t";
        let mut inp = input(sql, Some(0));
        inp.dialect = Some(Dialect::Mssql);
        inp.pos_is_line = true;
        assert!(analyze(&inp, &NothingKnown).is_empty(), "1줄만 = 오타 없음");
        inp.server_pos = Some(pos_of(sql, "FORM"));
        let out = analyze(&inp, &NothingKnown);
        assert_eq!(out.len(), 1, "2줄 시작 = FORM 포함");
        // near 토큰이 그 줄 안에 있으면 그 토큰까지만.
        let sql2 = "SELECT DISTINC a FORM t WHER x";
        let mut inp2 = input(sql2, Some(0));
        inp2.pos_is_line = true;
        inp2.near = Some("FORM");
        let out = analyze(&inp2, &NothingKnown);
        assert_eq!(
            out.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
            vec!["DISTINC", "FORM"],
            "WHER는 near 뒤 = 제외"
        );
    }

    #[test]
    fn near_token_from_server_messages() {
        assert_eq!(
            near_token("syntax error at or near \"FORM\""),
            Some("FORM".into())
        );
        assert_eq!(
            near_token("Incorrect syntax near 'FORM'."),
            Some("FORM".into())
        );
        assert_eq!(
            near_token("near \"'가나다'\": syntax error"),
            Some("'가나다'".into())
        );
        assert_eq!(
            near_token("ORA-00923: FROM keyword not found where expected"),
            None
        );
        assert_eq!(near_token("near \"\": syntax error"), None);
    }

    /// 예산: 이미 지난 기한 = 아무것도 찾지 않는다(호스트는 서버 위치로 폴백).
    #[test]
    fn expired_deadline_returns_nothing() {
        let sql = "SELECT DISTINC a FROM t";
        let mut inp = input(sql, None);
        inp.deadline = Some(Instant::now() - Duration::from_secs(1));
        assert!(analyze(&inp, &NothingKnown).is_empty());
        inp.deadline = Some(Instant::now() + Duration::from_secs(5));
        assert_eq!(analyze(&inp, &NothingKnown).len(), 1);
    }

    /// 4-DBMS 흔한 오타 표본(방언 무관).
    #[test]
    fn common_typos_across_dialects() {
        for (d, sql, typo, want) in [
            (Dialect::Postgres, "SELCT a FROM t", "SELCT", "SELECT"),
            (
                Dialect::Sqlite,
                "SELECT a FROM t WHRE a = 1",
                "WHRE",
                "WHERE",
            ),
            (
                Dialect::Mssql,
                "SELECT a FROM t GRUOP BY a",
                "GRUOP",
                "GROUP",
            ),
            (
                Dialect::Oracle,
                "SELECT a FROM t ORDR BY a",
                "ORDR",
                "ORDER",
            ),
            (
                Dialect::Postgres,
                "INSERT INTO t VALEUS (1)",
                "VALEUS",
                "VALUES",
            ),
            (
                Dialect::Oracle,
                "UPDATE t SET a = 1 WEHRE b = 2",
                "WEHRE",
                "WHERE",
            ),
        ] {
            let mut inp = input(sql, None);
            inp.dialect = Some(d);
            let out = analyze(&inp, &NothingKnown);
            assert_eq!(out.len(), 1, "{sql} → {out:?}");
            assert_eq!(
                (out[0].text.as_str(), out[0].expect.as_str()),
                (typo, want),
                "{sql}"
            );
        }
    }
}
