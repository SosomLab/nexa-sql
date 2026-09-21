//! `EXEC` 본문의 **호출 모양** — `[:RET :=] [schema.][pkg.]name[(인자, …)]`에서 이름과 "인자 자리 ↔ 바인드 이름"을 읽는다.
//!
//! 쓰임: 선언 없이 쓴 바인드(`EXEC proc(:PC_RET, 'a')` — Golden 관용)의 타입을 **루틴 서명에서** 정하려고(REF CURSOR OUT을
//! 문자열로 바인드하면 PLS-00306). 서명 조회는 호스트(러너 + 카탈로그)의 일이고 여기는 순수 파싱만 한다.

use crate::lexer::{classify, Class};

/// 인자 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallArg {
    /// 이름 표기(`P_NAME => …`)의 형식 인자 이름(대문자).
    pub named: Option<String>,
    /// 인자 전체가 바인드 하나(`:NAME`)면 그 이름(대문자 · 콜론 없음) — 식(`:A + 1`)이면 `None`.
    pub bind: Option<String>,
    /// T-SQL 호출에서 이 인자 뒤에 `OUTPUT`/`OUT`이 이미 붙어 있다(괄호 꼴 호출에서는 늘 거짓).
    pub output: bool,
}

/// 호출 모양.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallShape {
    /// `:RET := f(…)`의 받는 바인드(대문자).
    pub ret: Option<String>,
    /// 호출 이름(원문 그대로 · 점 포함).
    pub name: String,
    pub args: Vec<CallArg>,
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$' | '#')
}

/// 글 전체가 `:IDENT` 하나인가.
fn whole_bind(s: &str) -> Option<String> {
    let t = s.trim();
    let rest = t.strip_prefix(':')?;
    let mut it = rest.chars();
    let first = it.next()?;
    if !(first.is_alphabetic() || first == '_') || !it.all(is_ident_char) {
        return None;
    }
    Some(rest.to_ascii_uppercase())
}

/// 호출로 볼 수 없는 첫 낱말(문장·블록).
const NOT_A_CALL: &[&str] = &[
    "SELECT", "WITH", "INSERT", "UPDATE", "DELETE", "MERGE", "OPEN", "BEGIN", "DECLARE", "IF",
    "FOR", "WHILE", "LOOP", "NULL", "RETURN", "CALL", "SET",
];

/// `EXEC` 뒤 본문을 읽는다. 호출 모양이 아니면(대입 · `SELECT … INTO` · `OPEN :rc FOR …`) `None`.
#[must_use]
pub fn call_shape(body: &str) -> Option<CallShape> {
    let mut text = body.trim().trim_end_matches(';').trim();
    let mut ret = None;
    if let Some((lhs, rhs)) = text.split_once(":=") {
        ret = Some(whole_bind(lhs)?);
        text = rhs.trim();
    }
    let name_end = text
        .char_indices()
        .find(|(_, c)| !(is_ident_char(*c) || matches!(c, '.' | '"')))
        .map_or(text.len(), |(i, _)| i);
    let name = &text[..name_end];
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let first = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    if NOT_A_CALL.contains(&first.as_str()) {
        return None;
    }
    let rest = text[name_end..].trim();
    if rest.is_empty() {
        // 대입의 오른쪽이 맨 이름이면(`:V := X`) 호출이 아니다 — 인자 없는 프로시저 호출은 받는 바인드가 없을 때만.
        return ret.is_none().then(|| CallShape {
            ret: None,
            name: name.to_string(),
            args: Vec::new(),
        });
    }
    let inner = rest.strip_prefix('(')?.trim_end();
    let inner = inner.strip_suffix(')')?;
    // 최상위 쉼표로 나눈다(문자열·주석·괄호 안은 건너뜀).
    let classes = classify(inner);
    let mut args = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    let cut = |from: usize, to: usize, args: &mut Vec<CallArg>| {
        let piece = &inner[from..to];
        if piece.trim().is_empty() {
            return;
        }
        // 이름 표기: 최상위 `=>`(문자열 밖) — 조각 안에서 다시 찾는다.
        let pc = classify(piece);
        let arrow = piece
            .char_indices()
            .zip(pc.iter())
            .find(|((i, c), cls)| {
                **cls == Class::Code && *c == '=' && piece[*i..].starts_with("=>")
            })
            .map(|((i, _), _)| i);
        match arrow {
            Some(i) => args.push(CallArg {
                named: Some(piece[..i].trim().trim_matches('"').to_ascii_uppercase()),
                bind: whole_bind(&piece[i + 2..]),
                output: false,
            }),
            None => args.push(CallArg {
                named: None,
                bind: whole_bind(piece),
                output: false,
            }),
        }
    };
    for ((i, c), cls) in inner.char_indices().zip(classes.iter()) {
        if *cls != Class::Code {
            continue;
        }
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                cut(start, i, &mut args);
                start = i + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    cut(start, inner.len(), &mut args);
    Some(CallShape {
        ret,
        name: name.to_string(),
        args,
    })
}

/// T-SQL 호출 모양 — `[스키마.]이름 [인자, …]`(괄호 없음 · `@이름 = 값` 이름 표기 · 뒤의 `OUTPUT`/`OUT`). 쓰임 = 서명으로
/// 타입을 정하고 **빠진 `OUTPUT`을 보충**하려고(T-151). 반환값 받기(`@r = proc`) · 문자열 실행(`EXEC('…')`)은 다루지 않는다.
#[must_use]
pub fn tsql_call_shape(body: &str) -> Option<CallShape> {
    let text = body.trim().trim_end_matches(';').trim();
    if text.contains(":=") {
        return None;
    }
    let name_end = text
        .char_indices()
        .find(|(_, c)| !(is_ident_char(*c) || matches!(c, '.' | '[' | ']' | '"')))
        .map_or(text.len(), |(i, _)| i);
    let name = &text[..name_end];
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let first = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_uppercase();
    if NOT_A_CALL.contains(&first.as_str()) {
        return None;
    }
    let inner = &text[name_end..];
    if inner.trim_start().starts_with('(') {
        return None;
    }
    let classes = classify(inner);
    let mut args = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    let cut = |from: usize, to: usize, args: &mut Vec<CallArg>| {
        let mut piece = inner[from..to].trim();
        if piece.is_empty() {
            return;
        }
        let mut output = false;
        for kw in ["OUTPUT", "OUT"] {
            if piece.len() > kw.len()
                && piece.is_char_boundary(piece.len() - kw.len())
                && piece[piece.len() - kw.len()..].eq_ignore_ascii_case(kw)
                && piece[..piece.len() - kw.len()].ends_with(char::is_whitespace)
            {
                output = true;
                piece = piece[..piece.len() - kw.len()].trim_end();
                break;
            }
        }
        // 이름 표기 `@p = 값` — `@이름` 뒤의 첫 `=`.
        let named = piece.strip_prefix('@').and_then(|r| {
            let (n, v) = r.split_once('=')?;
            let n = n.trim();
            (!n.is_empty() && n.chars().all(is_ident_char)).then(|| (n.to_ascii_uppercase(), v))
        });
        match named {
            Some((n, val)) => args.push(CallArg {
                named: Some(n),
                bind: whole_bind(val),
                output,
            }),
            None => args.push(CallArg {
                named: None,
                bind: whole_bind(piece),
                output,
            }),
        }
    };
    for ((i, c), cls) in inner.char_indices().zip(classes.iter()) {
        if *cls != Class::Code {
            continue;
        }
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                cut(start, i, &mut args);
                start = i + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    cut(start, inner.len(), &mut args);
    Some(CallShape {
        ret: None,
        name: name.to_string(),
        args,
    })
}

/// 글의 `:NAME`(코드 상태 · 낱말 경계) 뒤에 ` OUTPUT`을 붙인다 — 이미 `OUTPUT`/`OUT`이 뒤따르면 그대로. `names` = 대문자.
#[must_use]
pub fn mark_output(sql: &str, names: &[String]) -> String {
    if names.is_empty() {
        return sql.to_string();
    }
    let classes = classify(sql);
    let b = sql.as_bytes();
    let mut out = String::with_capacity(sql.len() + names.len() * 7);
    let mut i = 0;
    let mut last = 0;
    while i < b.len() {
        if b[i] == b':' && classes[i] == Class::Code {
            let mut j = i + 1;
            while j < b.len()
                && (b[j].is_ascii_alphanumeric() || matches!(b[j], b'_' | b'$' | b'#'))
            {
                j += 1;
            }
            let name = sql[i + 1..j].to_ascii_uppercase();
            if j > i + 1 && names.contains(&name) {
                let tail = sql[j..].trim_start();
                let word: String = tail
                    .chars()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .collect();
                if !word.eq_ignore_ascii_case("OUTPUT") && !word.eq_ignore_ascii_case("OUT") {
                    out.push_str(&sql[last..j]);
                    out.push_str(" OUTPUT");
                    last = j;
                }
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out.push_str(&sql[last..]);
    out
}

#[cfg(test)]
mod tests {
    /// T-151 — T-SQL 호출 모양: 자리·이름 표기 · 이미 붙은 OUTPUT/OUT · 식·상수 · 임시 프로시저(`#`)·대괄호 이름 · 호출 아님.
    #[test]
    fn tsql_shape_and_output_marks() {
        use super::{mark_output, tsql_call_shape};
        let s = tsql_call_shape("#p 3, :X, :Y OUTPUT").expect("call");
        assert_eq!(s.name, "#p");
        let got: Vec<(Option<&str>, Option<&str>, bool)> = s
            .args
            .iter()
            .map(|a| (a.named.as_deref(), a.bind.as_deref(), a.output))
            .collect();
        assert_eq!(
            got,
            vec![
                (None, None, false),
                (None, Some("X"), false),
                (None, Some("Y"), true)
            ]
        );
        let s = tsql_call_shape("[dbo].[sp_x] @msg = :M out, @a = 1, @b = :N + 1;").expect("call");
        assert_eq!(s.name, "[dbo].[sp_x]");
        assert_eq!(s.args[0].named.as_deref(), Some("MSG"));
        assert_eq!(
            (s.args[0].bind.as_deref(), s.args[0].output),
            (Some("M"), true)
        );
        assert_eq!(
            (s.args[1].bind.as_deref(), s.args[2].bind.as_deref()),
            (None, None)
        );
        assert_eq!(tsql_call_shape("sp_who").map(|s| s.args.len()), Some(0));
        assert!(tsql_call_shape("SELECT 1").is_none());
        assert!(tsql_call_shape(":V := 1").is_none());
        assert!(
            tsql_call_shape("sp_x(1)").is_none(),
            "괄호 꼴은 call_shape의 몫"
        );

        let names = vec!["X".to_string(), "Y".to_string()];
        assert_eq!(
            mark_output("EXEC #p 3, :X, :Y OUTPUT, ':X', :XY -- :X", &names),
            "EXEC #p 3, :X OUTPUT, :Y OUTPUT, ':X', :XY -- :X"
        );
        assert_eq!(mark_output("EXEC p :x out", &names), "EXEC p :x out");
        assert_eq!(mark_output("EXEC p :Z", &names), "EXEC p :Z");
    }

    use super::*;

    #[test]
    fn positional_named_and_return() {
        let s = call_shape("SP_X(:PC_RET, 'SEBANG', f(1, 2), :V_USER);").unwrap();
        assert_eq!(s.name, "SP_X");
        let binds: Vec<Option<&str>> = s.args.iter().map(|a| a.bind.as_deref()).collect();
        assert_eq!(binds, [Some("PC_RET"), None, None, Some("V_USER")]);
        let s = call_shape("pkg.find(p_name => :v_name, p_rc => :rc)").unwrap();
        assert_eq!(s.name, "pkg.find");
        assert_eq!(s.args[1].named.as_deref(), Some("P_RC"));
        assert_eq!(s.args[1].bind.as_deref(), Some("RC"));
        let s = call_shape(":rc := pkg.objects_like('A,B', :n + 1)").unwrap();
        assert_eq!(s.ret.as_deref(), Some("RC"));
        assert_eq!(s.args.len(), 2, "문자열 안 쉼표는 나누지 않는다");
        assert_eq!(s.args[1].bind, None, "식은 바인드 하나가 아니다");
        assert_eq!(call_shape("NSQLT_V_IMPL").unwrap().args.len(), 0);
    }

    #[test]
    fn non_calls() {
        assert!(call_shape(":V := 'x'").is_none());
        assert!(call_shape(":V := 1").is_none());
        assert!(call_shape("SELECT a INTO :V FROM t").is_none());
        assert!(call_shape("OPEN :rc FOR SELECT 1 FROM dual").is_none());
        assert!(call_shape("p(:a").is_none(), "괄호 불일치");
    }
}
