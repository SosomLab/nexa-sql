//! 스크립트 → 항목 분리(docs/08 §2). SQL*Plus 규칙에 GUI 관용을 더했다.
//!
//! - 줄 첫머리의 스크립트 명령([`crate::command::is_command_start`])은 **한 줄 항목**.
//!   단 `EXEC`가 홀로 있는 줄은 **블록 EXEC** — 다음 줄부터 `;` 끝·`/` 줄·빈 줄까지 본문(Golden 관용).
//! - SQL은 문자열·주석 밖의 `;`로 끝난다. PL/SQL 블록(`BEGIN`·`DECLARE`·`CREATE … PROCEDURE|FUNCTION|PACKAGE|TRIGGER|TYPE`)은
//!   `;`로 끝나지 않고 **단독 `/` 줄**로 끝난다.
//! - T-SQL 방언에선 단독 `GO` 줄이 배치를 끝낸다(방언 무관하게 항상 인식 — Oracle 스크립트에 `GO`가 나올 일은 없다).
//! - 항목마다 원문 바이트 범위(`span`)와 시작 줄(1-base)을 남긴다 — 편집기가 오류 위치를 되찾는다.

use crate::command::{is_command_start, parse_command, Command};
use crate::lexer::{classify, Class};
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

pub fn split_script(src: &str) -> Vec<Item> {
    let mut items = Vec::new();
    let lines: Vec<(usize, &str)> = line_offsets(src);
    let mut i = 0;
    while i < lines.len() {
        let (off, raw) = lines[i];
        let line = raw.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        // 줄 주석만 있는 줄 — 건너뛴다(항목 아님).
        if trimmed.starts_with("--") {
            i += 1;
            continue;
        }
        if trimmed == "/" {
            i += 1;
            continue;
        }
        if is_command_start(trimmed) {
            let word = trimmed
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect::<String>()
                .to_ascii_uppercase();
            let is_bare_exec = matches!(word.as_str(), "EXEC" | "EXECUTE")
                && trimmed[word.len()..]
                    .trim()
                    .trim_end_matches(';')
                    .trim()
                    .is_empty();
            if is_bare_exec {
                // 블록 EXEC: 다음 줄부터 본문.
                let start_line = i + 1;
                let (body, end_i, end_off) = collect_block_exec(src, &lines, start_line);
                let text = format!("EXEC {}", body.trim());
                let kind = match parse_command(&text) {
                    Ok(Some(c)) => ItemKind::Command(c),
                    Ok(None) => ItemKind::Invalid("EXEC 본문 없음".into()),
                    Err(e) => ItemKind::Invalid(e),
                };
                items.push(Item {
                    kind,
                    text,
                    span: off..end_off,
                    line: i + 1,
                });
                i = end_i;
                continue;
            }
            // 줄 연속(`-`로 끝나는 줄) — SQL*Plus 관용.
            let mut text = trimmed.to_string();
            let mut end_off = off + raw.len();
            let mut j = i;
            while text.ends_with('-') && j + 1 < lines.len() {
                text.pop();
                let trimmed_len = text.trim_end().len();
                text.truncate(trimmed_len);
                j += 1;
                let (o2, r2) = lines[j];
                text.push(' ');
                text.push_str(r2.trim_end_matches('\r').trim());
                end_off = o2 + r2.len();
            }
            let kind = match parse_command(&text) {
                Ok(Some(c)) => ItemKind::Command(c),
                Ok(None) => ItemKind::Sql(classify_sql(&text)),
                Err(e) => ItemKind::Invalid(e),
            };
            items.push(Item {
                kind,
                text: text.trim_end_matches(';').trim().to_string(),
                span: off..end_off,
                line: i + 1,
            });
            i = j + 1;
            continue;
        }
        // SQL 또는 PL/SQL 블록.
        let (text, end_i, end_off) = collect_sql(src, &lines, i);
        let kind = ItemKind::Sql(classify_sql(&text));
        items.push(Item {
            kind,
            text,
            span: off..end_off,
            line: i + 1,
        });
        i = end_i;
    }
    items
}

fn line_offsets(src: &str) -> Vec<(usize, &str)> {
    let mut v = Vec::new();
    let mut off = 0;
    for l in src.split_inclusive('\n') {
        let body = l.strip_suffix('\n').unwrap_or(l);
        v.push((off, body));
        off += l.len();
    }
    v
}

/// `EXEC`(홀로) 다음 줄부터 본문을 모은다: `;`로 끝나는 줄 · 단독 `/` · 빈 줄 · 다음 명령 시작에서 멈춘다.
fn collect_block_exec<'a>(
    src: &'a str,
    lines: &[(usize, &'a str)],
    start: usize,
) -> (String, usize, usize) {
    let mut j = start;
    let mut end_off = lines
        .get(start.saturating_sub(1))
        .map_or(0, |(o, r)| o + r.len());
    let mut body_start: Option<usize> = None;
    let mut body_end: Option<usize> = None;
    while j < lines.len() {
        let (o, r) = lines[j];
        let t = r.trim_end_matches('\r').trim();
        if t.is_empty() || t == "/" || (body_start.is_some() && is_command_start(t)) {
            if t == "/" {
                end_off = o + r.len();
                j += 1;
            }
            break;
        }
        let bs = *body_start.get_or_insert(o);
        end_off = o + r.len();
        j += 1;
        if let Some(semi) = terminated_by_semicolon(&src[bs..end_off]) {
            body_end = Some(bs + semi);
            break;
        }
    }
    let body = body_start
        .map(|s| src[s..body_end.unwrap_or(end_off)].trim().to_string())
        .unwrap_or_default();
    (body, j, end_off)
}

/// SQL 문 하나를 모은다. 블록이면 `/`까지, 아니면 코드 영역의 `;`까지. `GO` 줄도 종결.
fn collect_sql<'a>(
    src: &'a str,
    lines: &[(usize, &'a str)],
    start: usize,
) -> (String, usize, usize) {
    let (start_off, _) = lines[start];
    let is_block = starts_block(lines[start].1.trim());
    let mut j = start;
    let mut end_off = start_off;
    let mut text_end: Option<usize> = None;
    while j < lines.len() {
        let (o, r) = lines[j];
        let t = r.trim_end_matches('\r').trim();
        if j > start && (t == "/" || t.eq_ignore_ascii_case("GO")) {
            j += 1;
            break;
        }
        end_off = o + r.len();
        j += 1;
        if !is_block {
            if let Some(semi) = terminated_by_semicolon(&src[start_off..end_off]) {
                text_end = Some(start_off + semi);
                break;
            }
        }
    }
    let text = src[start_off..text_end.unwrap_or(end_off)]
        .trim()
        .to_string();
    (text, j, end_off)
}

/// 코드 영역의 마지막 비공백 문자가 `;`이면 그 위치(주석 꼬리 `… ; -- x`는 건너뛴다).
fn terminated_by_semicolon(chunk: &str) -> Option<usize> {
    let classes = classify(chunk);
    let b = chunk.as_bytes();
    let mut k = b.len();
    while k > 0 {
        k -= 1;
        if classes[k] != Class::Code {
            if classes[k] == Class::LineComment || classes[k] == Class::BlockComment {
                continue;
            }
            return None;
        }
        if b[k].is_ascii_whitespace() {
            continue;
        }
        return if b[k] == b';' { Some(k) } else { None };
    }
    None
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
                    *x != "OR" && *x != "REPLACE" && *x != "EDITIONABLE" && *x != "NONEDITIONABLE"
                })
                .collect();
            matches!(
                rest.first().copied(),
                Some("PROCEDURE")
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

    const GOLDEN: &str = "EXEC\t:V_PRG_NM\t\t\t:=\t'SP_M4P_MPO_M4E_CREATE_BSY';\n\nEXEC DBMS_STATS.GATHER_TABLE_STATS(USER, 'M4E_I301080', CASCADE=>TRUE, NO_INVALIDATE=>FALSE);\n\nEXEC SP_M4P_MP_VERSION_CREATE(:PC_RET, 'SEBANG', 'DP_202604_W16_V01', 'W', '20260415', '20260705', '', 'sybae0057');\n\n\n--\tMP 버전에 기반한 변수 설정\nEXEC\nSELECT\n\tA.PROJECT_CD\n,\tA.MP_VRSN_ID\nINTO\n\t:V_PROJECT_CD\n,\t:V_MP_VRSN_ID\nFROM\n\tM4S_O301010 A\nWHERE 1=1\nAND\tA.PROJECT_CD\t\t=\t'SEBANG'\n\nSELECT\n\t:V_PROJECT_CD\n,\t:V_PRG_NM\nFROM\n\tDUAL\n;\n";

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
            matches!(&items[2].kind, ItemKind::Command(Command::Exec { body }) if body.starts_with("SP_M4P_MP_VERSION_CREATE(:PC_RET"))
        );
        match &items[3].kind {
            ItemKind::Command(Command::Exec { body }) => {
                assert!(body.starts_with("SELECT"), "{body}");
                assert!(body.contains("INTO"));
                assert!(body.ends_with("'SEBANG'"), "{body}");
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
    fn spans_point_back_into_source() {
        let s = "SELECT 1\nFROM dual;\n\nEXEC :v := 1;\n";
        let items = split_script(s);
        assert_eq!(&s[items[0].span.clone()], "SELECT 1\nFROM dual;");
        assert_eq!(&s[items[1].span.clone()], "EXEC :v := 1;");
        assert_eq!(items[1].line, 4);
    }
}
