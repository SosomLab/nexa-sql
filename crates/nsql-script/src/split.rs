//! 스크립트 → 항목 분리(docs/08 §2). SQL*Plus 규칙에 GUI 관용을 더했다.
//!
//! - 줄 첫머리의 스크립트 명령([`crate::command::is_command_start`])은 **한 줄 항목**.
//!   단 `EXEC`가 홀로 있는 줄은 **블록 EXEC** — 다음 줄부터 `;` 끝·`/` 줄·빈 줄까지 본문(Golden 관용).
//! - SQL은 문자열·주석 밖의 `;`로 끝난다. PL/SQL 블록(`BEGIN`·`DECLARE`·`CREATE … PROCEDURE|FUNCTION|PACKAGE|TRIGGER|TYPE`)은
//!   `;`로 끝나지 않고 **단독 `/` 줄**로 끝난다.
//! - T-SQL 방언에선 단독 `GO` 줄이 배치를 끝낸다(방언 무관하게 항상 인식 — Oracle 스크립트에 `GO`가 나올 일은 없다).
//! - 항목마다 원문 바이트 범위(`span`)와 시작 줄(1-base)을 남긴다 — 편집기가 오류 위치를 되찾는다.

use crate::command::{is_command_start, parse_command, Command};
use crate::lexer::{classify_in, Class};
use nsql_core::Dialect;
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SqlKind {
    Query,
    Dml,
    Ddl,
    /// PL/SQL 익명 블록·저장 단위 — `/`로 끝나며 바인드는 InOut.
    Block,
    Other,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ItemKind {
    Command(Command),
    Sql(SqlKind),
    /// 명령 해석 실패 — 문장 위치와 메시지를 남긴다.
    Invalid(String),
}

#[derive(Clone, PartialEq, Debug)]
pub struct Item {
    pub kind: ItemKind,
    /// 실행 텍스트(끝 `;`·`/` 제거, 앞뒤 공백 제거).
    pub text: String,
    pub span: Range<usize>,
    pub line: usize,
}

/// 캐럿(바이트 오프셋) 위치의 **한 문장** — 편집기 `Ctrl+Enter`(사용자 09-14 · `;`가 끝을 나타낸다는 기본 규칙 =
/// [`split_script`]와 같은 분리기 · `/` 단독 줄 · PL/SQL 블록도 같은 규칙). GUI와 CLI 셸이 같은 함수를 쓴다.
///
/// 규칙: 캐럿이 문장 안이면 그 문장 · 문장 사이(공백·주석)면 **줄 거리가 가까운 쪽**(동률 = 바로 앞 문장 · 사용자 10-13
/// "빈 줄에서 Ctrl+Enter가 14줄 위의 첫 블록을 실행" → 바로 아래 문장) · 첫 문장보다 앞이면 첫 문장 · 문장이 없으면 `None`.
/// 캐럿이 `;` 바로 뒤(span 끝)도 그 문장으로 본다. 같은 줄의 꼬리 주석(`SELECT 1; -- note`)은 거리 0 = 앞 문장.
pub fn statement_at(src: &str, byte_pos: usize) -> Option<Item> {
    statement_at_in(src, byte_pos, None)
}

/// [`statement_at`]의 방언 지정판 — 분할 규칙은 [`split_script_in`].
pub fn statement_at_in(src: &str, byte_pos: usize, dialect: Option<Dialect>) -> Option<Item> {
    let items = split_script_in(src, dialect);
    let pos = byte_pos.min(src.len());
    let mut chosen: Option<usize> = None;
    let mut inside = false;
    for (i, it) in items.iter().enumerate() {
        if it.span.start <= pos && pos <= it.span.end {
            chosen = Some(i);
            inside = true;
            break;
        }
        if it.span.end < pos {
            chosen = Some(i);
        } else {
            break;
        }
    }
    // ★ 문장 사이: 앞 문장 끝 ↔ 캐럿 ↔ 다음 문장 시작의 **줄 거리**를 재어 가까운 쪽(동률 = 앞).
    if !inside {
        if let Some(i) = chosen {
            if let Some(next) = items.get(i + 1) {
                let prev_end = items[i].span.end.min(pos);
                let up = src[prev_end..pos].matches('\n').count();
                let down = src[pos..next.span.start.max(pos)].matches('\n').count();
                if down < up {
                    chosen = Some(i + 1);
                }
            }
        }
    }
    let i = chosen.or_else(|| (!items.is_empty()).then_some(0))?;
    items.into_iter().nth(i)
}

pub fn split_script(src: &str) -> Vec<Item> {
    split_script_in(src, None)
}

/// ★ 방언을 아는 분할(09-20 · 사용자 "모든 객체 유형이 편집기에서 실행되는가" 점검에서 드러난 결함):
/// 종전 분할은 방언을 몰라 `CREATE TRIGGER`·`BEGIN`을 늘 PL/SQL 블록(단독 `/`로 끝남)으로 봤다 → SQLite에서
/// `CREATE TRIGGER … BEGIN …; END;` 뒤의 문장들이 한 덩어리로 묶여 **오류도 없이 실행되지 않았고**(드라이버는 첫 문장만 실행),
/// SQLite·PostgreSQL의 `BEGIN;`(트랜잭션 시작)도 뒤 문장을 전부 삼켰다.
///
/// 규칙: `None`·Oracle·ODBC·SQL Server = 종전 그대로(블록은 `/`·`GO`) — 단 어느 방언이든 **`BEGIN;` · `BEGIN TRANSACTION|TRAN|WORK|
/// DEFERRED|IMMEDIATE|EXCLUSIVE|ISOLATION|READ …`는 트랜잭션 문장**(PL/SQL의 `BEGIN` 뒤에는 이 낱말들이 오지 않는다).
/// SQLite·MySQL·PostgreSQL = `/` 블록이 없다: 본문에 `BEGIN`이 있으면 짝이 맞는 `END` 뒤의 `;`에서 끝나고(`CASE … END` 중첩 계산),
/// 없으면 첫 `;`에서 끝난다(PG 함수 본문 `$$…$$`은 문자열이라 안의 `;`를 보지 않는다).
pub fn split_script_in(src: &str, dialect: Option<Dialect>) -> Vec<Item> {
    let mut classes = classify_in(src, dialect);
    // ★ MySQL `DELIMITER //`(135차): 그 줄부터 문장 끝 = 그 글자열(코드 영역) · `DELIMITER ;`로 복귀 · 줄 자체는 항목이 아니다.
    let mut delim: Option<String> = None;
    let b = src.as_bytes();
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(
            b.iter()
                .enumerate()
                .filter(|(_, c)| **c == b'\n')
                .map(|(i, _)| i + 1),
        )
        .collect();
    let line_of = |off: usize| -> usize { line_starts.partition_point(|&s| s <= off) };
    let line_end = |off: usize| -> usize {
        b[off..]
            .iter()
            .position(|&c| c == b'\n')
            .map_or(b.len(), |p| off + p)
    };
    let mut items = Vec::new();
    let mut pos = 0usize;
    while pos < b.len() {
        // 공백·개행 건너뛰기
        while pos < b.len() && (b[pos] as char).is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= b.len() {
            break;
        }
        let le = line_end(pos);
        let rest = src[pos..le].trim_end_matches('\r').trim();
        // ★ Db2 CLP `--#SET TERMINATOR @`(10-26 · 협업 조사 (C)④): 주석 꼴 지시자 — 그 줄부터 문장 끝 = 그 글자(서버엔 종결자를 떼고 보낸다 ·
        //   `--#SET TERMINATOR ;` = 복귀). 주석 건너뛰기보다 **먼저** 본다.
        if dialect == Some(Dialect::Db2) {
            if let Some(d) = db2_terminator_line(rest) {
                delim = (d != ";").then(|| d.to_string());
                pos = le;
                continue;
            }
        }
        if rest.starts_with("--")
            || classes[pos] == Class::BlockComment && is_comment_only_line(src, &classes, pos, le)
        {
            pos = le;
            continue;
        }
        if rest == "/" || rest.eq_ignore_ascii_case("GO") {
            pos = le;
            continue;
        }
        if matches!(dialect, Some(Dialect::Mysql | Dialect::Db2)) {
            if let Some(d) = delimiter_line(rest) {
                delim = (d != ";").then(|| d.to_string());
                pos = le;
                continue;
            }
            if let Some(d) = delim.as_deref() {
                let (text, end_off, next) = collect_delim_stmt(src, &classes, pos, d);
                if !text.is_empty() {
                    let kind = ItemKind::Sql(classify_sql(&text));
                    items.push(Item {
                        kind,
                        text,
                        span: pos..end_off,
                        line: line_of(pos),
                    });
                }
                pos = next;
                continue;
            }
        }
        if is_command_start(rest) {
            let word = rest
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect::<String>()
                .to_ascii_uppercase();
            let is_bare_exec = matches!(word.as_str(), "EXEC" | "EXECUTE")
                && rest[word.len()..]
                    .trim()
                    .trim_end_matches(';')
                    .trim()
                    .is_empty();
            let start = pos;
            if is_bare_exec {
                let (body, end_off, next) =
                    collect_block_exec(src, &classes, (le + 1).min(b.len()));
                let text = format!("EXEC {}", body.trim());
                let kind = match parse_command(&text) {
                    Ok(Some(c)) => ItemKind::Command(c),
                    Ok(None) => ItemKind::Invalid(tr!("EXEC: body is missing", "EXEC 본문 없음")),
                    Err(e) => ItemKind::Invalid(e),
                };
                items.push(Item {
                    kind,
                    text,
                    span: start..end_off,
                    line: line_of(start),
                });
                pos = next;
                continue;
            }
            // 한 줄 명령 — 코드 영역 `;`가 있으면 거기서 끊는다(뒤 문장은 다음 항목). PROMPT·REM·@는 줄 전체.
            let whole_line =
                matches!(word.as_str(), "PROMPT" | "REM" | "REMARK") || rest.starts_with('@');
            let semi = if whole_line {
                None
            } else {
                (pos..le).find(|&i| b[i] == b';' && classes[i] == Class::Code)
            };
            if let Some(semi) = semi {
                let text = src[pos..semi].trim().to_string();
                let kind = command_or_sql(&text, dialect);
                items.push(Item {
                    kind,
                    text,
                    span: start..semi + 1,
                    line: line_of(start),
                });
                pos = semi + 1;
                continue;
            }
            let mut text = rest.to_string();
            let mut end_off = le;
            while text.ends_with('-') && end_off < b.len() {
                text.pop();
                let t = text.trim_end().len();
                text.truncate(t);
                let ns = end_off + 1;
                let ne = line_end(ns);
                text.push(' ');
                text.push_str(src[ns..ne].trim_end_matches('\r').trim());
                end_off = ne;
            }
            let kind = command_or_sql(&text, dialect);
            items.push(Item {
                kind,
                text: text.trim_end_matches(';').trim().to_string(),
                span: start..end_off,
                line: line_of(start),
            });
            // 한 줄 명령은 줄 끝에서 끝난다 — 짝 없는 따옴표(`DEFINE v = O'Neil`)가 뒤 문장들을 글자 상수로
            // 삼키지 않게, 따옴표가 줄을 넘어갔으면 다음 줄부터 다시 분류한다(T-162 ④ · 드문 길이라 비용은 그때만).
            if end_off < b.len() && matches!(classes[end_off], Class::Str | Class::Ident) {
                let tail = classify_in(&src[end_off..], dialect);
                classes[end_off..].copy_from_slice(&tail);
            }
            pos = end_off;
            continue;
        }
        let (text, end_off, next) = collect_sql(src, &classes, pos, rest, dialect);
        let kind = ItemKind::Sql(classify_sql(&text));
        items.push(Item {
            kind,
            text,
            span: pos..end_off,
            line: line_of(pos),
        });
        pos = next;
    }
    items
}

/// 줄 전체가 블록 주석 안인가(`/* … */`만 있는 줄 건너뛰기).
fn is_comment_only_line(src: &str, classes: &[Class], from: usize, to: usize) -> bool {
    src[from..to]
        .bytes()
        .enumerate()
        .all(|(i, c)| c.is_ascii_whitespace() || classes[from + i] == Class::BlockComment)
}

/// `EXEC`(홀로) 다음 줄부터 본문을 모은다: 코드 영역 `;` · 단독 `/` · 빈 줄 · 다음 명령 시작에서 멈춘다.
/// 반환 = (본문, 항목 끝 오프셋, 다음 스캔 위치).
fn collect_block_exec(src: &str, classes: &[Class], from: usize) -> (String, usize, usize) {
    let b = src.as_bytes();
    let mut pos = from;
    let mut body_start: Option<usize> = None;
    while pos < b.len() {
        let ls = pos;
        let le = b[ls..]
            .iter()
            .position(|&c| c == b'\n')
            .map_or(b.len(), |p| ls + p);
        let t = src[ls..le].trim_end_matches('\r').trim();
        // 본문 안의 `CONNECT BY`뿐인 줄(포맷터가 절 키워드를 제 줄에 둔다)은 접속 명령이 아니라 계층 질의의 절이다.
        if t.is_empty()
            || t == "/"
            || (body_start.is_some() && is_command_start(t) && !is_bare_connect_by(t))
        {
            let end = body_start.map_or(ls, |_| ls);
            let next = if t == "/" { le } else { ls };
            let body = body_start
                .map(|s| src[s..end].trim().to_string())
                .unwrap_or_default();
            return (body, end, next);
        }
        if body_start.is_none() {
            body_start = Some(ls + (src[ls..le].len() - src[ls..le].trim_start().len()));
        }
        // 이 줄 안의 코드 `;`
        if let Some(semi) = (ls..le).find(|&i| b[i] == b';' && classes[i] == Class::Code) {
            let body = src[body_start.unwrap_or(ls)..semi].trim().to_string();
            return (body, semi + 1, semi + 1);
        }
        pos = le + 1;
    }
    let body = body_start
        .map(|s| src[s..].trim().to_string())
        .unwrap_or_default();
    (body, b.len(), b.len())
}

/// `CONNECT BY` 두 단어뿐인 줄인가(`EXEC` 블록 본문의 절 줄).
fn is_bare_connect_by(line: &str) -> bool {
    let mut w = line.split_whitespace();
    matches!(
        (w.next(), w.next(), w.next()),
        (Some(a), Some(b), None) if a.eq_ignore_ascii_case("CONNECT") && b.eq_ignore_ascii_case("BY")
    )
}

/// SQL 문 하나. 블록이면 단독 `/`·`GO` 줄까지, 아니면 코드 영역 `;`까지(같은 줄 뒤 문장은 다음 항목).
/// 반환 = (텍스트, 항목 끝 오프셋, 다음 스캔 위치).
/// `/` 줄로 끝나는 블록이 없는 방언인가(문장은 늘 `;`로 끝난다 — 본문의 `BEGIN … END`만 짝을 맞춘다).
fn semicolon_only(dialect: Option<Dialect>) -> bool {
    matches!(
        dialect,
        Some(Dialect::Sqlite | Dialect::Mysql | Dialect::Postgres)
    )
}

/// `BEGIN`으로 시작하지만 블록이 아니라 **트랜잭션 문장**인가(`BEGIN;` · `BEGIN TRANSACTION` …) — 방언 무관.
fn is_begin_transaction(first_line: &str) -> bool {
    let up = first_line.to_ascii_uppercase();
    let mut w = up.split_whitespace();
    let Some(first) = w.next() else { return false };
    if first == "BEGIN;" {
        return true;
    }
    if first != "BEGIN" {
        return false;
    }
    match w.next() {
        None => false, // `BEGIN`만 있는 줄 = PL/SQL·T-SQL 블록의 시작
        Some(n) => {
            let n = n.trim_end_matches(';');
            n.is_empty()
                || matches!(
                    n,
                    "TRANSACTION"
                        | "TRAN"
                        | "WORK"
                        | "DEFERRED"
                        | "IMMEDIATE"
                        | "EXCLUSIVE"
                        | "ISOLATION"
                        | "READ"
                        | "DISTRIBUTED"
                )
        }
    }
}

/// `;`만으로 끝나는 방언의 문장 끝: 본문에 `BEGIN`이 있으면 짝이 맞는 `END` 뒤의 코드 `;` · 없으면 첫 코드 `;`.
/// Db2 CLP `--#SET TERMINATOR x` 줄이면 새 종결자(한 토큰 · `;` = 복귀 · 대소문자 무관 · `--#SET` 뒤 공백 자유).
fn db2_terminator_line(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("--#")?.trim_start();
    let rest = rest
        .get(..3)
        .filter(|w| w.eq_ignore_ascii_case("SET"))
        .map(|_| rest[3..].trim_start())?;
    let rest = rest
        .get(..10)
        .filter(|w| w.eq_ignore_ascii_case("TERMINATOR"))
        .map(|_| &rest[10..])?;
    let tok = rest.split_whitespace().next()?;
    (!tok.is_empty()).then_some(tok)
}

/// `DELIMITER xx` 줄이면 새 구분자(공백 없는 한 토큰 · `;` 포함).
fn delimiter_line(line: &str) -> Option<&str> {
    let rest = line
        .get(..9)
        .filter(|w| w.eq_ignore_ascii_case("DELIMITER"))
        .map(|_| &line[9..])?;
    if !rest.starts_with(|c: char| c.is_ascii_whitespace()) {
        return None;
    }
    let tok = rest.split_whitespace().next()?;
    (!tok.is_empty()).then_some(tok)
}

/// 사용자 구분자(`//` · `$$`)까지 — 코드 영역에서만 찾는다(주석·문자열 안은 아니다). 못 찾으면 끝까지.
fn collect_delim_stmt(
    src: &str,
    classes: &[Class],
    start: usize,
    delim: &str,
) -> (String, usize, usize) {
    let b = src.as_bytes();
    let d = delim.as_bytes();
    let mut i = start;
    while i + d.len() <= b.len() {
        if classes[i] == Class::Code && &b[i..i + d.len()] == d {
            return (src[start..i].trim().to_string(), i, i + d.len());
        }
        i += 1;
    }
    (src[start..].trim().to_string(), b.len(), b.len())
}

fn collect_semicolon_stmt(src: &str, classes: &[Class], start: usize) -> (String, usize, usize) {
    let b = src.as_bytes();
    let mut depth = 0i32;
    let mut seen_begin = false;
    let mut i = start;
    while i < b.len() {
        if classes[i] != Class::Code {
            i += 1;
            continue;
        }
        let c = b[i];
        if c == b';' && (!seen_begin || depth <= 0) {
            return (src[start..i].trim().to_string(), i + 1, i + 1);
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let ws = i;
            while i < b.len()
                && classes[i] == Class::Code
                && (b[i].is_ascii_alphanumeric() || b[i] == b'_')
            {
                i += 1;
            }
            let w = &src[ws..i];
            if w.eq_ignore_ascii_case("BEGIN") {
                // `BEGIN ATOMIC`(PG) · 트리거 본문 — 첫 낱말이 BEGIN인 트랜잭션 문장은 여기 오지 않는다.
                seen_begin = true;
                depth += 1;
            } else if w.eq_ignore_ascii_case("CASE") && seen_begin {
                depth += 1;
            } else if w.eq_ignore_ascii_case("END") && seen_begin {
                // MySQL/MariaDB `END IF` · `END LOOP` · `END WHILE` · `END REPEAT`는 여는 낱말을 세지 않았다 → 짝에서 뺀다(135차).
                // (`END CASE`는 `CASE`를 셌으니 그대로 -1.)
                let next = src[i..]
                    .trim_start()
                    .split(|c: char| !c.is_ascii_alphabetic())
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase();
                if !matches!(next.as_str(), "IF" | "LOOP" | "WHILE" | "REPEAT") {
                    depth -= 1;
                }
            }
            continue;
        }
        i += 1;
    }
    (src[start..].trim().to_string(), b.len(), b.len())
}

fn collect_sql(
    src: &str,
    classes: &[Class],
    start: usize,
    first_line: &str,
    dialect: Option<Dialect>,
) -> (String, usize, usize) {
    let b = src.as_bytes();
    if semicolon_only(dialect) && !is_begin_transaction(first_line) {
        return collect_semicolon_stmt(src, classes, start);
    }
    let is_block = starts_block(first_line) && !is_begin_transaction(first_line);
    // PG 함수/프로시저 본문은 `$$ … $$`(문자열로 분류) — 본문이 닫힌 뒤의 코드 `;`가 문장 끝(`/` 줄 불필요).
    let mut saw_dollar = false;
    let mut i = start;
    while i < b.len() {
        if b[i] == b'$' && classes[i] == Class::Str {
            saw_dollar = true;
        }
        if (!is_block || saw_dollar) && b[i] == b';' && classes[i] == Class::Code {
            return (src[start..i].trim().to_string(), i + 1, i + 1);
        }
        if b[i] == b'\n' {
            // 다음 줄이 단독 `/` 또는 `GO`면 종결.
            let ns = i + 1;
            let ne = b[ns.min(b.len())..]
                .iter()
                .position(|&c| c == b'\n')
                .map_or(b.len(), |p| ns + p);
            let t = src[ns.min(b.len())..ne].trim_end_matches('\r').trim();
            if t == "/" || t.eq_ignore_ascii_case("GO") {
                return (src[start..i].trim().to_string(), i, ne);
            }
        }
        i += 1;
    }
    (src[start..].trim().to_string(), b.len(), b.len())
}

fn starts_block(first_line: &str) -> bool {
    let up = first_line.to_ascii_uppercase();
    let w: Vec<&str> = up.split_whitespace().collect();
    match w.first().copied() {
        Some("BEGIN") | Some("DECLARE") => true,
        Some("CREATE") => {
            let rest: Vec<&str> = w
                .iter()
                .skip(1)
                .copied()
                .filter(|x| {
                    // Oracle `OR REPLACE` · T-SQL `OR ALTER`(2016 SP1+) · 편집 가능 수식어.
                    !matches!(
                        *x,
                        "OR" | "REPLACE" | "ALTER" | "EDITIONABLE" | "NONEDITIONABLE"
                    )
                })
                .collect();
            matches!(
                rest.first().copied(),
                Some("PROCEDURE")
                    | Some("PROC")
                    | Some("FUNCTION")
                    | Some("PACKAGE")
                    | Some("TRIGGER")
                    | Some("TYPE")
                    | Some("LIBRARY")
            )
        }
        _ => false,
    }
}

pub fn classify_sql(text: &str) -> SqlKind {
    let up = text.trim_start().to_ascii_uppercase();
    let first = up.split_whitespace().next().unwrap_or("");
    match first {
        "SELECT" | "WITH" | "EXPLAIN" => SqlKind::Query,
        "INSERT" | "UPDATE" | "DELETE" | "MERGE" | "TRUNCATE" => SqlKind::Dml,
        "CREATE" | "ALTER" | "DROP" | "GRANT" | "REVOKE" | "COMMENT" => {
            if starts_block(text.trim_start()) {
                SqlKind::Block
            } else {
                SqlKind::Ddl
            }
        }
        "BEGIN" | "DECLARE" => SqlKind::Block,
        _ => SqlKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;

    const GOLDEN: &str = "EXEC\t:V_PRG_NM\t\t\t:=\t'SP_DMP_MPO_DME_CREATE_BSY';\n\nEXEC DBMS_STATS.GATHER_TABLE_STATS(USER, 'DME_I301080', CASCADE=>TRUE, NO_INVALIDATE=>FALSE);\n\nEXEC SP_DMP_MP_VERSION_CREATE(:PC_RET, 'ACME', 'DP_202604_W16_V01', 'W', '20260415', '20260705', '', 'user01');\n\n\n--\tMP 버전에 기반한 변수 설정\nEXEC\nSELECT\n\tA.PROJECT_CD\n,\tA.MP_VRSN_ID\nINTO\n\t:V_PROJECT_CD\n,\t:V_MP_VRSN_ID\nFROM\n\tDMS_O301010 A\nWHERE 1=1\nAND\tA.PROJECT_CD\t\t=\t'ACME'\n\nSELECT\n\t:V_PROJECT_CD\n,\t:V_PRG_NM\nFROM\n\tDUAL\n;\n";

    #[test]
    fn tsql_create_or_alter_is_a_block_until_go() {
        let src = "CREATE OR ALTER PROCEDURE dbo.p @a INT AS
BEGIN
  SET NOCOUNT ON;
  SELECT @a;
END
GO
PRINT 'hello';
PRINT V
";
        let items = split_script(src);
        assert_eq!(items.len(), 3, "{items:#?}");
        assert!(matches!(items[0].kind, ItemKind::Sql(SqlKind::Block)));
        assert!(items[0].text.contains("SET NOCOUNT ON;"));
        assert!(
            matches!(items[1].kind, ItemKind::Sql(_)),
            "T-SQL PRINT 'x'는 SQL"
        );
        assert!(
            matches!(items[2].kind, ItemKind::Command(_)),
            "PRINT V는 SQL*Plus PRINT"
        );
    }

    #[test]
    fn pg_dollar_quoted_function_ends_at_semicolon() {
        let src = "CREATE OR REPLACE PROCEDURE p(IN a INT, OUT b INT) LANGUAGE plpgsql AS $$ BEGIN b := a * 2; END $$;
CALL p(21, NULL);
SELECT 1;
";
        let items = split_script(src);
        assert_eq!(items.len(), 3, "{items:#?}");
        assert!(items[0].text.ends_with("END $$"));
        assert!(matches!(items[0].kind, ItemKind::Sql(SqlKind::Block)));
        assert_eq!(items[1].text, "CALL p(21, NULL)");
    }

    #[test]
    fn golden_script_splits_into_five_items() {
        let items = split_script(GOLDEN);
        let kinds: Vec<String> = items
            .iter()
            .map(|i| format!("{:?}", i.kind).chars().take(14).collect())
            .collect();
        assert_eq!(items.len(), 5, "{kinds:?}");
        assert!(
            matches!(&items[0].kind, ItemKind::Command(Command::Exec { body }) if body.starts_with(":V_PRG_NM"))
        );
        assert!(
            matches!(&items[1].kind, ItemKind::Command(Command::Exec { body }) if body.starts_with("DBMS_STATS"))
        );
        assert!(
            matches!(&items[2].kind, ItemKind::Command(Command::Exec { body }) if body.starts_with("SP_DMP_MP_VERSION_CREATE(:PC_RET"))
        );
        match &items[3].kind {
            ItemKind::Command(Command::Exec { body }) => {
                assert!(body.starts_with("SELECT"), "{body}");
                assert!(body.contains("INTO"));
                assert!(body.ends_with("'ACME'"), "{body}");
            }
            k => panic!("{k:?}"),
        }
        assert_eq!(items[3].line, 9);
        assert!(matches!(items[4].kind, ItemKind::Sql(SqlKind::Query)));
        assert!(items[4].text.ends_with("DUAL"));
        assert_eq!(items[4].line, 21);
    }

    #[test]
    fn plsql_block_ends_with_slash_not_semicolon() {
        let s = "BEGIN\n  :x := 1;\n  DBMS_OUTPUT.PUT_LINE('a;b');\nEND;\n/\nSELECT 1 FROM dual;\n";
        let items = split_script(s);
        assert_eq!(items.len(), 2);
        assert!(matches!(items[0].kind, ItemKind::Sql(SqlKind::Block)));
        assert!(items[0].text.ends_with("END;"));
        assert!(matches!(items[1].kind, ItemKind::Sql(SqlKind::Query)));
    }

    #[test]
    fn create_procedure_is_block_and_go_separates() {
        let s =
            "CREATE OR REPLACE PROCEDURE p IS\nBEGIN\n NULL;\nEND p;\n/\nSELECT 1\nGO\nSELECT 2;\n";
        let items = split_script(s);
        assert_eq!(items.len(), 3);
        assert!(matches!(items[0].kind, ItemKind::Sql(SqlKind::Block)));
        assert_eq!(items[1].text, "SELECT 1");
        assert_eq!(items[2].text, "SELECT 2");
    }

    #[test]
    fn semicolon_inside_string_and_trailing_comment() {
        let s = "SELECT 'a;b' FROM t; -- done\nSELECT 2 -- ;\nFROM t;\n";
        let items = split_script(s);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "SELECT 'a;b' FROM t");
        assert!(items[1].text.starts_with("SELECT 2"));
    }

    #[test]
    fn multiline_command_continuation() {
        let s = "EXEC pkg.run(:a, -\n  'x')\nPRINT a\n";
        let items = split_script(s);
        assert_eq!(items.len(), 2);
        assert!(
            matches!(&items[0].kind, ItemKind::Command(Command::Exec { body }) if body == "pkg.run(:a, 'x')")
        );
    }

    #[test]
    fn multiple_statements_on_one_line() {
        let s = "CREATE TABLE t(a INTEGER); INSERT INTO t VALUES (1); SELECT * FROM t;";
        let items = split_script(s);
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].text, "INSERT INTO t VALUES (1)");
        assert_eq!(items[2].line, 1);
        let s = "EXEC :a := 1; SELECT :a FROM dual;\n";
        let items = split_script(s);
        assert_eq!(items.len(), 2, "{items:?}");
        assert!(
            matches!(&items[0].kind, ItemKind::Command(Command::Exec { body }) if body == ":a := 1")
        );
    }

    #[test]
    fn spans_point_back_into_source() {
        let s = "SELECT 1\nFROM dual;\n\nEXEC :v := 1;\n";
        let items = split_script(s);
        assert_eq!(&s[items[0].span.clone()], "SELECT 1\nFROM dual;");
        assert_eq!(&s[items[1].span.clone()], "EXEC :v := 1;");
        assert_eq!(items[1].line, 4);
    }
}

#[cfg(test)]
mod statement_at_tests {
    use super::*;

    const SRC: &str = "SELECT 1 FROM dual;

SELECT 2
  FROM dual;
-- tail comment
EXEC :V := 'x';";

    fn text_at(pos: usize) -> String {
        statement_at(SRC, pos).map(|i| i.text).unwrap_or_default()
    }

    #[test]
    fn caret_inside_statement_picks_it() {
        assert_eq!(text_at(3), "SELECT 1 FROM dual");
        let second = SRC.find("SELECT 2").unwrap();
        assert_eq!(
            text_at(second + 10),
            "SELECT 2
  FROM dual"
        );
    }

    /// ★ 문장 사이 = 줄 거리가 가까운 쪽(사용자 10-13): 블록 뒤 빈 줄 여러 개 아래의 빈 줄에서 = 바로 아래 문장 ·
    /// 동률(앞뒤 한 줄씩)·같은 줄 꼬리 주석 = 앞 문장(종전).
    #[test]
    fn caret_in_gap_picks_nearer_statement_by_lines() {
        let src = "BEGIN NULL; END;\n/\n\n\n\n-- head\nSELECT 9 FROM dual;";
        let blank = src.find("\n\n\n\n").unwrap() + 3; // 셋째 빈 줄(위로 3줄 · 아래로 1줄)
        let it = statement_at_in(src, blank, Some(Dialect::Oracle)).unwrap();
        assert!(
            it.text.starts_with("SELECT 9"),
            "가까운 아래 문장: {}",
            it.text
        );
        let just_after = src.find("/\n").unwrap() + 2; // `/` 바로 다음 줄(위 0 · 아래 4)
        let it = statement_at_in(src, just_after, Some(Dialect::Oracle)).unwrap();
        assert!(it.text.starts_with("BEGIN"), "앞이 더 가까움: {}", it.text);
        let tail = "SELECT 1 FROM dual; -- note\nSELECT 2 FROM dual;";
        let note = tail.find("note").unwrap();
        assert_eq!(
            statement_at(tail, note).unwrap().text,
            "SELECT 1 FROM dual",
            "꼬리 주석 = 앞 문장"
        );
    }

    #[test]
    fn caret_between_statements_picks_previous() {
        let gap = SRC
            .find(
                "

",
            )
            .unwrap()
            + 1;
        assert_eq!(text_at(gap), "SELECT 1 FROM dual");
        let comment = SRC.find("-- tail").unwrap() + 3;
        assert_eq!(
            text_at(comment),
            "SELECT 2
  FROM dual"
        );
    }

    #[test]
    fn caret_right_after_semicolon_and_at_end() {
        let semi = SRC.find(';').unwrap();
        assert_eq!(text_at(semi + 1), "SELECT 1 FROM dual");
        assert_eq!(text_at(SRC.len()), "EXEC :V := 'x'");
        assert_eq!(text_at(usize::MAX), "EXEC :V := 'x'");
    }

    #[test]
    fn empty_source_is_none() {
        assert!(statement_at(
            "   
", 1
        )
        .is_none());
    }
}

#[cfg(test)]
mod dialect_split_tests {
    use super::*;

    fn texts(src: &str, d: Option<Dialect>) -> Vec<String> {
        split_script_in(src, d)
            .into_iter()
            .map(|i| i.text)
            .collect()
    }

    /// 재현(09-20): SQLite 트리거 뒤의 문장들이 한 덩어리로 묶여 실행되지 않던 것 — 트리거는 `END;`에서 끝난다.
    #[test]
    fn sqlite_trigger_ends_at_end_semicolon() {
        let src = "CREATE TABLE t(a);\nCREATE TRIGGER trg AFTER INSERT ON t\nBEGIN\n  INSERT INTO log VALUES (CASE WHEN NEW.a > 0 THEN 'p' ELSE 'n' END);\n  UPDATE k SET v = 1;\nEND;\nINSERT INTO t VALUES (1);\nSELECT json_extract('{}', '$.a');\nSELECT 2;\n";
        let v = texts(src, Some(Dialect::Sqlite));
        assert_eq!(v.len(), 5, "{v:#?}");
        assert!(v[1].starts_with("CREATE TRIGGER") && v[1].ends_with("END"));
        assert_eq!(v[2], "INSERT INTO t VALUES (1)");
        assert_eq!(v[4], "SELECT 2");
    }

    /// `BEGIN;` · `BEGIN TRANSACTION;` = 트랜잭션 문장(방언 무관) — 뒤 문장을 삼키지 않는다.
    #[test]
    fn begin_transaction_is_a_plain_statement() {
        for d in [
            None,
            Some(Dialect::Sqlite),
            Some(Dialect::Postgres),
            Some(Dialect::Mssql),
        ] {
            let v = texts("BEGIN;\nINSERT INTO t VALUES (1);\nCOMMIT;\n", d);
            assert_eq!(
                v,
                vec!["BEGIN", "INSERT INTO t VALUES (1)", "COMMIT"],
                "{d:?}"
            );
            let v = texts("BEGIN TRANSACTION;\nSELECT 1;\n", d);
            assert_eq!(v.len(), 2, "{d:?}");
        }
    }

    /// Oracle·방언 없음 = 종전 그대로: PL/SQL 블록과 트리거는 단독 `/`에서 끝난다(본문의 `;`는 끝이 아니다).
    #[test]
    fn oracle_blocks_still_end_at_slash() {
        let src = "CREATE OR REPLACE TRIGGER trg BEFORE INSERT ON t FOR EACH ROW\nBEGIN\n  :NEW.a := 1;\nEND;\n/\nBEGIN\n  NULL;\nEND;\n/\nSELECT 1 FROM DUAL;\n";
        for d in [None, Some(Dialect::Oracle)] {
            let v = texts(src, d);
            assert_eq!(v.len(), 3, "{d:?} {v:#?}");
            assert!(v[0].contains(":NEW.a := 1;") && v[0].trim_end().ends_with("END;"));
            assert_eq!(v[2], "SELECT 1 FROM DUAL");
        }
    }

    /// PostgreSQL: `$$` 본문 안의 `;`는 보지 않고 · 트리거는 평문장 · `BEGIN ATOMIC … END;`은 짝을 맞춘다.
    #[test]
    fn postgres_bodies() {
        let src = "CREATE FUNCTION f() RETURNS int AS $$ BEGIN RETURN 1; END; $$ LANGUAGE plpgsql;\nCREATE TRIGGER trg BEFORE INSERT ON t FOR EACH ROW EXECUTE FUNCTION f();\nCREATE PROCEDURE p() BEGIN ATOMIC INSERT INTO t VALUES (1); INSERT INTO t VALUES (2); END;\nSELECT 1;\n";
        let v = texts(src, Some(Dialect::Postgres));
        assert_eq!(v.len(), 4, "{v:#?}");
        assert!(v[2].ends_with("END"));
    }

    /// 캐럿 문장도 같은 규칙(`statement_at_in`).
    #[test]
    fn statement_at_follows_dialect() {
        let src =
            "CREATE TRIGGER trg AFTER INSERT ON t BEGIN UPDATE k SET v = 1; END;\nSELECT 9;\n";
        let it = statement_at_in(src, src.len() - 2, Some(Dialect::Sqlite)).expect("stmt");
        assert_eq!(it.text, "SELECT 9");
    }
}

/// ★ 왕복 불변식(mac 09-21): 항목의 **원문 조각 `src[span]`을 다시 분할하면 같은 종류·같은 실행 텍스트 하나**가 된다 —
/// 편집기 "현재 문 실행"·Explain이 캐럿의 문장을 원문 조각으로 넘기는 근거. (`text`는 정규화본이라 이 성질이 없다:
/// 홀로 선 `EXEC` 줄 + 본문은 `EXEC 본문`이 되고, 그것을 다시 나누면 한 줄 명령 `EXEC SELECT` + SQL 둘로 갈라진다.)
#[cfg(test)]
mod span_roundtrip_tests {
    use super::*;

    const SRC: &str = "EXEC :V := 1;

EXEC
SELECT
	COUNT(*)
,	MAX(name)
INTO
	:V_CNT
,	:V_NAME
FROM
	sys.objects
WHERE 1=1
;

SELECT :V_CNT, :V_NAME
FROM DUAL;

BEGIN
  NULL;
END;
/
DEFINE x = 1
";

    #[test]
    fn resplitting_the_source_slice_gives_the_same_item() {
        for dialect in [None, Some(Dialect::Mssql), Some(Dialect::Oracle)] {
            let items = split_script_in(SRC, dialect);
            assert!(items.len() >= 5, "{dialect:?}: {}", items.len());
            for it in &items {
                let piece = &SRC[it.span.clone()];
                let again = split_script_in(piece, dialect);
                assert_eq!(
                    again.len(),
                    1,
                    "{dialect:?} · 조각이 하나로 남아야: {piece:?} → {again:#?}"
                );
                assert_eq!(again[0].kind, it.kind, "{dialect:?} · {piece:?}");
                assert_eq!(again[0].text, it.text, "{dialect:?} · {piece:?}");
            }
        }
    }

    /// 정규화된 `text`는 그 성질이 없다(문서화 — 이 사실 때문에 호출부가 `span`을 쓴다).
    #[test]
    fn normalized_text_of_a_block_exec_does_not_roundtrip() {
        let items = split_script_in(SRC, Some(Dialect::Mssql));
        let block = items
            .iter()
            .find(|it| it.text.starts_with("EXEC SELECT"))
            .expect("블록 EXEC");
        assert_eq!(split_script_in(&block.text, Some(Dialect::Mssql)).len(), 2);
    }

    /// `EXEC` 블록 본문(포맷터가 절·항목을 제 줄에 둔다 · 10-03): 명령 단어로 **시작하는 이름**(`COLUMN_NAME` · `SET_ID`)과
    /// `CONNECT BY`뿐인 줄은 본문을 끊지 않는다 · 진짜 명령 줄(`PRINT`)은 끊는다.
    #[test]
    fn exec_block_body_keeps_identifier_and_connect_by_lines() {
        let src = "EXEC\nSELECT\n\tCOLUMN_NAME\n,\tSET_ID\nINTO\n\t:A\n,\t:B\nFROM\n\tT\nSTART WITH\n\tA = 1\nCONNECT BY\n\tPRIOR A = B\nPRINT A\n";
        let items = split_script_in(src, Some(Dialect::Oracle));
        assert_eq!(items.len(), 2, "{items:#?}");
        assert!(
            items[0].text.starts_with("EXEC SELECT"),
            "{}",
            items[0].text
        );
        assert!(items[0].text.ends_with("PRIOR A = B"), "{}", items[0].text);
        assert!(matches!(items[0].kind, ItemKind::Command(_)));
        assert!(crate::command::is_command_start("COLUMN name"));
        assert!(!crate::command::is_command_start("COLUMN_NAME"));
        assert!(!crate::command::is_command_start("DESC$X"));
    }

    /// T-162 ④ — 한 줄 명령의 짝 없는 따옴표가 뒤 문장을 삼키지 않는다.
    #[test]
    fn unpaired_quote_in_a_line_command_stops_at_line_end() {
        let src = "DEFINE who = O'Neil
SELECT 1 FROM dual;
SELECT 'a;b' FROM dual;
PROMPT it's done
SELECT 3;
";
        for dialect in [None, Some(Dialect::Oracle), Some(Dialect::Sqlite)] {
            let items = split_script_in(src, dialect);
            let texts: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
            assert_eq!(
                texts,
                [
                    "DEFINE who = O'Neil",
                    "SELECT 1 FROM dual",
                    "SELECT 'a;b' FROM dual",
                    "PROMPT it's done",
                    "SELECT 3"
                ],
                "{dialect:?}"
            );
            assert!(matches!(items[0].kind, ItemKind::Command(_)), "{dialect:?}");
            assert!(matches!(items[1].kind, ItemKind::Sql(_)), "{dialect:?}");
        }
    }
}

#[cfg(test)]
mod mysql_tests {
    use super::*;

    fn texts(src: &str) -> Vec<String> {
        split_script_in(src, Some(Dialect::Mysql))
            .into_iter()
            .map(|i| i.text)
            .collect()
    }

    /// 10-26 · Db2 `--#SET TERMINATOR @`: SQL PL 본문 안의 `;`를 넘기고 `@`에서 끝난다 · `--#SET TERMINATOR ;`로 복귀 · 지시자 줄은 항목 아님 ·
    /// 보내는 문장에는 `@`가 없다 · MySQL이 아닌 다른 방언에서는 보통 주석.
    #[test]
    fn db2_set_terminator_switches_terminator() {
        let src = "--#SET TERMINATOR @\nCREATE PROCEDURE p()\nLANGUAGE SQL\nBEGIN\n  DECLARE v INT;\n  SET v = 1;\nEND@\n--#set terminator ;\nSELECT 3;\nCALL p()";
        let items = split_script_in(src, Some(Dialect::Db2));
        let texts: Vec<&str> = items.iter().map(|i| i.text.as_str()).collect();
        assert_eq!(texts.len(), 3, "{texts:#?}");
        assert!(
            texts[0].starts_with("CREATE PROCEDURE") && texts[0].ends_with("END"),
            "{}",
            texts[0]
        );
        assert!(!texts[0].contains('@'));
        assert_eq!(texts[1], "SELECT 3");
        assert_eq!(texts[2], "CALL p()");
        assert_eq!(db2_terminator_line("--#SET TERMINATOR @"), Some("@"));
        assert_eq!(db2_terminator_line("--# set   terminator   ;"), Some(";"));
        assert_eq!(db2_terminator_line("-- SET TERMINATOR @"), None);
        // 다른 방언(Oracle)에서는 그냥 주석 → `END@`가 한 문장에 남는다(동작 변경 없음).
        let ora = split_script_in("--#SET TERMINATOR @\nSELECT 1@", Some(Dialect::Oracle));
        assert_eq!(ora.len(), 1);
        assert!(ora[0].text.contains('@'));
    }

    /// 135차 · MySQL `DELIMITER //`: 프로시저 본문 안의 `;`를 넘기고 `//`에서 끝난다 · `DELIMITER ;`로 복귀 · 줄 자체는 항목 아님.
    #[test]
    fn delimiter_switches_terminator() {
        let src = "DELIMITER //\nCREATE PROCEDURE p()\nBEGIN\n  SELECT 1;\n  SELECT 2;\nEND//\nDELIMITER ;\nSELECT 3;\nCALL p();";
        let v = texts(src);
        assert_eq!(v.len(), 3, "{v:?}");
        assert!(
            v[0].starts_with("CREATE PROCEDURE p()") && v[0].ends_with("END"),
            "{}",
            v[0]
        );
        assert_eq!(v[1], "SELECT 3");
        assert_eq!(v[2], "CALL p()");
        // 구분자 `$$` · 주석·문자열 안의 구분자는 아니다.
        let src = "DELIMITER $$\nSELECT '$$' -- $$\n$$\nSELECT 2$$";
        assert_eq!(texts(src), vec!["SELECT '$$' -- $$", "SELECT 2"]);
        assert_eq!(delimiter_line("delimiter //"), Some("//"));
        assert_eq!(delimiter_line("DELIMITER"), None);
        assert_eq!(delimiter_line("DELIMITERS x"), None);
    }

    /// `END IF` · `END WHILE` · `END LOOP` · `END REPEAT`는 `BEGIN … END` 짝에서 빠진다(종전엔 depth가 0이 돼 첫 `;`에서 끊겼다).
    #[test]
    fn end_if_does_not_close_begin() {
        let src = "CREATE PROCEDURE p(IN x INT)\nBEGIN\n  IF x > 0 THEN\n    SELECT 1;\n  END IF;\n  WHILE x > 0 DO SET x = x - 1; END WHILE;\n  SELECT CASE WHEN x = 0 THEN 'z' ELSE 'n' END;\nEND;\nSELECT 9;";
        let v = texts(src);
        assert_eq!(v.len(), 2, "{v:?}");
        assert!(v[0].ends_with("END"), "{}", v[0]);
        assert_eq!(v[1], "SELECT 9");
        // `#` 주석 안의 `;`는 문장을 끝내지 않는다 · 백틱 식별자 안의 `;`도.
        assert_eq!(
            texts("SELECT 1 # a;b\n;\nSELECT `x;y`;"),
            vec!["SELECT 1 # a;b", "SELECT `x;y`"]
        );
    }
}

/// 명령 또는 SQL — ★ MySQL/MariaDB(T-387 후속 · 10-22 실측 "SHOW STATUS … 미지원"): `SHOW …`는 그 방언의 **서버 문장**
/// (SHOW STATUS · TABLES · CREATE TABLE · VARIABLES LIKE …)이라 클라이언트 꼴([`client_show`])만 명령으로 남기고 나머지는 서버로.
fn command_or_sql(text: &str, dialect: Option<Dialect>) -> ItemKind {
    match parse_command(text) {
        Ok(Some(crate::command::Command::Show { what }))
            if dialect == Some(Dialect::Mysql) && !client_show(&what) =>
        {
            ItemKind::Sql(SqlKind::Query)
        }
        Ok(Some(c)) => ItemKind::Command(c),
        Ok(None) => ItemKind::Sql(classify_sql(text)),
        Err(e) => ItemKind::Invalid(e),
    }
}

/// 클라이언트 `SHOW` 꼴(SQL*Plus 관용 · docs/63): `CONN[ECTION]`·`CONNINFO`·`ERR[ORS] [이름]` 늘 · `USER`·`VAR[IABLE]S`는
/// **단독일 때만**(MySQL `SHOW VARIABLES LIKE …`·`SHOW USER …`는 서버 문장).
pub fn client_show(what: &str) -> bool {
    let w = what
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_ascii_uppercase();
    let mut words = w.split_whitespace();
    let head = words.next().unwrap_or("");
    match head {
        "CONN" | "CONNECTION" | "CONNINFO" | "ERR" | "ERRORS" => true,
        // ★ 10-29: `SHOW AUTOCOMMIT`·`SHOW VERSION`(단독 · MySQL 서버에는 없는 꼴).
        "USER" | "VARIABLES" | "VAR" | "VARS" | "AUTOCOMMIT" | "AUTO" | "VERSION" | "VER"
        | "SERVER" => words.next().is_none(),
        _ => false,
    }
}

#[cfg(test)]
mod show_passthrough_tests {
    use super::*;

    /// ★ MySQL 방언의 `SHOW`(10-22): 서버 문장은 SQL(Query) · 클라이언트 꼴은 명령 · 다른 방언은 종전처럼 전부 명령.
    #[test]
    fn mysql_show_goes_to_server_except_client_forms() {
        let src = "SHOW SESSION STATUS LIKE 'Ssl_cipher'; SHOW VARIABLES; SHOW VARIABLES LIKE 'x'; SHOW TABLES; SHOW CONN; SHOW ERRORS; SHOW USER;";
        let kinds: Vec<bool> = split_script_in(src, Some(Dialect::Mysql))
            .iter()
            .map(|it| matches!(it.kind, ItemKind::Sql(SqlKind::Query)))
            .collect();
        assert_eq!(
            kinds,
            vec![true, false, true, true, false, false, false],
            "{kinds:?}"
        );
        let oracle: Vec<bool> = split_script_in(src, Some(Dialect::Oracle))
            .iter()
            .map(|it| matches!(it.kind, ItemKind::Command(_)))
            .collect();
        assert!(
            oracle.iter().all(|c| *c),
            "다른 방언 = 전부 클라이언트 명령: {oracle:?}"
        );
        assert!(client_show("err pkg_x") && client_show("CONNECTION") && client_show("vars"));
        assert!(
            client_show("autocommit")
                && client_show("VERSION;")
                && !client_show("version like 'x'")
        );
        assert!(!client_show("STATUS LIKE 'x'") && !client_show("variables like 'x'"));
    }
}
