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

/// 캐럿(바이트 오프셋) 위치의 **한 문장** — 편집기 `Ctrl+Enter`(사용자 09-14 · `;`가 끝을 나타낸다는 기본 규칙 =
/// [`split_script`]와 같은 분리기 · `/` 단독 줄 · PL/SQL 블록도 같은 규칙). GUI와 CLI 셸이 같은 함수를 쓴다.
///
/// 규칙: 캐럿이 문장 안이면 그 문장 · 문장 사이(공백·주석)면 **바로 앞** 문장 · 첫 문장보다 앞이면 첫 문장 ·
/// 문장이 없으면 `None`. 캐럿이 `;` 바로 뒤(span 끝)도 그 문장으로 본다.
pub fn statement_at(src: &str, byte_pos: usize) -> Option<Item> {
    let items = split_script(src);
    let pos = byte_pos.min(src.len());
    let mut chosen: Option<usize> = None;
    for (i, it) in items.iter().enumerate() {
        if it.span.start <= pos && pos <= it.span.end {
            chosen = Some(i);
            break;
        }
        if it.span.end < pos {
            chosen = Some(i);
        } else {
            break;
        }
    }
    let i = chosen.or_else(|| (!items.is_empty()).then_some(0))?;
    items.into_iter().nth(i)
}

pub fn split_script(src: &str) -> Vec<Item> {
    let classes = classify(src);
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
                    Ok(None) => ItemKind::Invalid("EXEC 본문 없음".into()),
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
                let kind = match parse_command(&text) {
                    Ok(Some(c)) => ItemKind::Command(c),
                    Ok(None) => ItemKind::Sql(classify_sql(&text)),
                    Err(e) => ItemKind::Invalid(e),
                };
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
            let kind = match parse_command(&text) {
                Ok(Some(c)) => ItemKind::Command(c),
                Ok(None) => ItemKind::Sql(classify_sql(&text)),
                Err(e) => ItemKind::Invalid(e),
            };
            items.push(Item {
                kind,
                text: text.trim_end_matches(';').trim().to_string(),
                span: start..end_off,
                line: line_of(start),
            });
            pos = end_off;
            continue;
        }
        let (text, end_off, next) = collect_sql(src, &classes, pos, rest);
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
        if t.is_empty() || t == "/" || (body_start.is_some() && is_command_start(t)) {
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

/// SQL 문 하나. 블록이면 단독 `/`·`GO` 줄까지, 아니면 코드 영역 `;`까지(같은 줄 뒤 문장은 다음 항목).
/// 반환 = (텍스트, 항목 끝 오프셋, 다음 스캔 위치).
fn collect_sql(
    src: &str,
    classes: &[Class],
    start: usize,
    first_line: &str,
) -> (String, usize, usize) {
    let b = src.as_bytes();
    let is_block = starts_block(first_line);
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
