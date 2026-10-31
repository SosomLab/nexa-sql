//! ★ T-255 2단계 PL/SQL 블록 **재들여쓰기**(10-31 · docs/95 §11 `format.plsql = indent`): 통과(Raw) 블록의 원문 줄바꿈은 그대로 두고
//! 줄 앞 공백만 구조 깊이로 다시 매긴다(§11-3 표). `off`(기본) = 원문 통과 · `full`(단위 SQL 펼치기)은 v2.
//!
//! 안전장치: 깊이가 음수가 되거나 블록 끝에 0으로 돌아오지 않으면 **원문 그대로**(`None`) · 여러 줄 문자열/주석 **안**의 줄은 손대지
//! 않는다(토큰 경계로 판정) · 결과는 줄 앞 공백만 다르므로 토큰열은 그대로.

use crate::lexer::{Kind, Token};
use crate::Options;

/// 여는 것의 종류(스택).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Open {
    /// 선언부(`DECLARE` · 루틴 머리의 `IS`/`AS`) — `BEGIN`이 닫고 본문을 연다 · 패키지 명세는 `END`가 닫는다.
    Decl,
    /// `BEGIN` 본문(`EXCEPTION`이 같은 깊이로 이어진다).
    Begin,
    /// `IF … THEN`(`ELSIF`/`ELSE`는 같은 깊이 · `END IF`가 닫음).
    If,
    /// `… LOOP`(`END LOOP`).
    Loop,
    /// `CASE`(문장 · `WHEN`/`ELSE` 가지 · `END CASE`).
    Case,
    /// `WHEN … THEN`/`ELSE` 가지(EXCEPTION·CASE 안 · 다음 가지나 `END`가 닫음).
    When,
}

/// 줄 하나의 토큰 정보 — 첫 낱말(대문자) · 둘째 낱말 · 마지막 낱말(주석 제외) · 줄 안에서 시작하는 토큰이 있는가.
struct LineInfo {
    first: String,
    second: String,
    last: String,
    has_tokens: bool,
    /// 이 줄이 여러 줄 토큰(블록 주석·문자열)의 **이어지는 줄**이다 → 손대지 않는다.
    inside_multiline: bool,
}

fn line_infos(text: &str, toks: &[Token], base: usize) -> Vec<(usize, usize, LineInfo)> {
    // 줄 바이트 범위(원문 `text` 기준 · 토큰 span은 `base`만큼 밀려 있다).
    let mut out = Vec::new();
    let mut pos = 0usize;
    for raw in text.split_inclusive('\n') {
        let start = pos;
        let end = pos + raw.len();
        pos = end;
        let a = start + base;
        let b = end + base;
        let mut first = String::new();
        let mut second = String::new();
        let mut last = String::new();
        let mut has = false;
        let mut inside = false;
        for t in toks {
            if t.span.1 <= a {
                continue;
            }
            if t.span.0 >= b {
                break;
            }
            if t.span.0 < a {
                // 앞 줄에서 시작한 토큰이 이 줄까지 이어진다.
                if !t.is_comment() || t.kind == Kind::BlockComment {
                    inside = true;
                }
                continue;
            }
            if t.is_comment() {
                continue;
            }
            has = true;
            let up = t.up();
            if first.is_empty() {
                first = up.clone();
            } else if second.is_empty() {
                second = up.clone();
            }
            last = up;
        }
        out.push((
            start,
            end,
            LineInfo {
                first,
                second,
                last,
                has_tokens: has,
                // 앞 줄에서 시작한 토큰(문자열·블록 주석)이 이 줄까지 이어지면 그 줄은 통째로 손대지 않는다(뒤에 다른 토큰이 있어도).
                inside_multiline: inside,
            },
        ));
    }
    out
}

fn is_routine_head(first: &str) -> bool {
    matches!(
        first,
        "CREATE" | "PROCEDURE" | "FUNCTION" | "PACKAGE" | "TRIGGER" | "TYPE"
    )
}

/// 블록 재들여쓰기(`indent`) — `text` = 통과 블록 원문 · `toks` = 그 블록의 토큰(span은 `base`부터) · 실패 = `None`.
pub fn reindent(text: &str, toks: &[Token], base: usize, opts: &Options) -> Option<String> {
    let unit = opts.indent_unit();
    let infos = line_infos(text, toks, base);
    let mut stack: Vec<Open> = Vec::new();
    let mut in_header = false;
    let mut out = String::with_capacity(text.len() + 64);
    for (start, end, info) in &infos {
        let raw = &text[*start..*end];
        let (body, nl) = match raw.strip_suffix("\r\n") {
            Some(b) => (b, "\r\n"),
            None => match raw.strip_suffix('\n') {
                Some(b) => (b, "\n"),
                None => (raw, ""),
            },
        };
        if info.inside_multiline || body.trim().is_empty() {
            // 여러 줄 토큰의 이어지는 줄 · 빈 줄 = 그대로(빈 줄은 공백 없이).
            if body.trim().is_empty() {
                out.push_str(nl);
            } else {
                out.push_str(body);
                out.push_str(nl);
            }
            continue;
        }
        let depth = stack.len();
        let f = info.first.as_str();
        let s = info.second.as_str();
        let l = info.last.as_str();
        // 이 줄 깊이 · 뒤 스택 변화.
        let mut this = depth;
        if !info.has_tokens {
            // 주석만 있는 줄 = 지금 깊이.
        } else if body.trim() == "/" {
            this = 0;
        } else if f == "END" {
            // 가지(WHEN/ELSE)가 열려 있으면 먼저 닫고 블록을 닫는다.
            if stack.last() == Some(&Open::When) {
                stack.pop();
            }
            stack.pop()?; // 짝 없는 END = 실패
            this = stack.len();
        } else if f == "BEGIN" {
            if stack.last() == Some(&Open::Decl) {
                stack.pop();
                this = stack.len();
            }
            in_header = false;
            stack.push(Open::Begin);
        } else if f == "DECLARE" {
            in_header = false;
            stack.push(Open::Decl);
        } else if f == "EXCEPTION" && stack.last() == Some(&Open::Begin) {
            this = depth - 1;
        } else if matches!(f, "ELSIF" | "ELSE") {
            match stack.last() {
                Some(Open::When) => {
                    // CASE의 ELSE 가지 = 앞 가지를 닫고 새 가지.
                    stack.pop();
                    this = stack.len();
                    stack.push(Open::When);
                }
                Some(Open::If) => this = depth - 1,
                Some(Open::Case) => {
                    this = depth;
                    stack.push(Open::When);
                }
                _ => return None,
            }
        } else if f == "WHEN" {
            // EXCEPTION/CASE 가지: 앞 가지가 열려 있으면 닫고, 이 줄은 그 자리에.
            if stack.last() == Some(&Open::When) {
                stack.pop();
            }
            this = stack.len();
            if l == "THEN" {
                stack.push(Open::When);
            } else {
                return None; // `WHEN … THEN`이 한 줄에 없음 = v1 범위 밖 → 원문
            }
        } else if f == "IF" {
            if l == "THEN" {
                stack.push(Open::If);
            } else {
                return None;
            }
        } else if f == "CASE" && s != "WHEN" || (f == "CASE" && l != "END") {
            // CASE 문장(줄 끝 END가 아니면 여러 줄 CASE)
            if l == "END" {
                // `CASE … END;` 한 줄
            } else {
                stack.push(Open::Case);
            }
        } else if matches!(f, "FOR" | "WHILE" | "LOOP") && l == "LOOP" {
            stack.push(Open::Loop);
        } else if is_routine_head(f) && !matches!(l, "IS" | "AS") && f != "TYPE" {
            // 루틴 머리 첫 줄(IS/AS가 아직 안 나옴) — 이어지는 줄은 +1.
            in_header = true;
        } else if in_header {
            this = depth + 1;
            if matches!(l, "IS" | "AS") {
                in_header = false;
                stack.push(Open::Decl);
            }
        } else if matches!(l, "IS" | "AS") && is_routine_head(f) {
            // 한 줄 머리 + IS/AS = 선언부 시작.
            stack.push(Open::Decl);
        } else if l == "THEN" && f != "WHEN" {
            // `ELSIF cond THEN`은 위에서 · 그 밖(`IF`가 앞 줄에 걸친 조건의 끝 줄) = 열기.
            stack.push(Open::If);
        } else if l == "LOOP" {
            stack.push(Open::Loop);
        }
        if is_routine_head(f) && matches!(l, "IS" | "AS") && !stack.contains(&Open::Decl) {
            // 위 가지에서 못 넣은 경우 보강(머리 한 줄).
            if stack.last() != Some(&Open::Decl) {
                stack.push(Open::Decl);
            }
        }
        for _ in 0..this {
            out.push_str(&unit);
        }
        out.push_str(body.trim());
        out.push_str(nl);
    }
    // 블록 끝 = 스택 0(또는 머리만 남은 패키지 명세는 END가 닫았어야 한다).
    if !stack.is_empty() {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::{Indent, PlsqlStyle};

    fn opts() -> Options {
        Options {
            indent: Indent::Spaces(2),
            tab_width: 2,
            plsql: PlsqlStyle::Indent,
            ..Options::default()
        }
    }

    fn run(src: &str) -> Option<String> {
        let toks = lex(src);
        reindent(src, &toks, 0, &opts())
    }

    /// 95 §11-3 예시 그대로(머리 · 선언 · IF/ELSIF · FOR LOOP · EXCEPTION/WHEN · END 이름 · `/`).
    #[test]
    fn reindent_matches_design_example() {
        let src = "CREATE OR REPLACE PROCEDURE p (a NUMBER) IS\nv NUMBER;\n    BEGIN\nIF a > 0 THEN\nUPDATE t SET x = 1 WHERE id = a;\nELSIF a < 0 THEN\nNULL;\nEND IF;\nFOR r IN c LOOP\nNULL;\nEND LOOP;\nEXCEPTION\nWHEN OTHERS THEN\nRAISE;\nEND p;\n/\n";
        let want = "CREATE OR REPLACE PROCEDURE p (a NUMBER) IS\n  v NUMBER;\nBEGIN\n  IF a > 0 THEN\n    UPDATE t SET x = 1 WHERE id = a;\n  ELSIF a < 0 THEN\n    NULL;\n  END IF;\n  FOR r IN c LOOP\n    NULL;\n  END LOOP;\nEXCEPTION\n  WHEN OTHERS THEN\n    RAISE;\nEND p;\n/\n";
        assert_eq!(run(src).as_deref(), Some(want));
        // 멱등.
        assert_eq!(run(want).as_deref(), Some(want));
    }

    /// 익명 블록 · 여러 줄 머리(+1) · CASE 문 · 여러 줄 문자열 안은 그대로 · 짝 없는 END = 원문(None).
    #[test]
    fn anonymous_header_case_string_and_failure() {
        let src =
            "DECLARE\nx NUMBER;\nBEGIN\nCASE x\nWHEN 1 THEN\nNULL;\nELSE\nNULL;\nEND CASE;\nEND;\n";
        let want = "DECLARE\n  x NUMBER;\nBEGIN\n  CASE x\n    WHEN 1 THEN\n      NULL;\n    ELSE\n      NULL;\n  END CASE;\nEND;\n";
        assert_eq!(run(src).as_deref(), Some(want));
        let head = "CREATE OR REPLACE FUNCTION f (\na NUMBER,\nb NUMBER)\nRETURN NUMBER IS\nBEGIN\nRETURN a + b;\nEND f;\n";
        let want2 = "CREATE OR REPLACE FUNCTION f (\n  a NUMBER,\n  b NUMBER)\n  RETURN NUMBER IS\nBEGIN\n  RETURN a + b;\nEND f;\n";
        assert_eq!(run(head).as_deref(), Some(want2));
        let s = "BEGIN\nx := 'a\n   b';\nEND;\n";
        let want3 = "BEGIN\n  x := 'a\n   b';\nEND;\n";
        assert_eq!(run(s).as_deref(), Some(want3), "여러 줄 문자열 안은 그대로");
        assert_eq!(
            run("BEGIN\nNULL;\nEND;\nEND;\n"),
            None,
            "짝 없는 END = 원문"
        );
        assert_eq!(run("BEGIN\nNULL;\n"), None, "안 닫힘 = 원문");
    }
}
