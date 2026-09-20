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
            }),
            None => args.push(CallArg {
                named: None,
                bind: whole_bind(piece),
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

#[cfg(test)]
mod tests {
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
