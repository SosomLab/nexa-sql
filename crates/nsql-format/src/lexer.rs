//! SQL 렉서 — 공백만 버리고 **모든 토큰을 순서대로** 돌려준다(포맷터의 불변식 = 토큰 보존 · 공백만 바꾼다).
//!
//! 주석·문자열·인용 식별자·힌트·바인드는 한 토큰. 줄 바꿈 수(`nl_before`)와 원문 위치(`span`)를 들고 있어
//! 주석 배치(같은 줄/독립 줄)와 원문 유지(§19 한 줄 문장 · PL/SQL 통과)가 가능하다.

/// 토큰 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// 식별자 · 키워드(구분은 [`crate::is_keyword`]).
    Word,
    Number,
    /// `'...'`(`''` 이스케이프 포함 · 접두 `N'` 등은 Word + Str).
    Str,
    /// `"..."` · `[...]` · `` `...` ``.
    Quoted,
    /// `-- …`(줄 끝 제외).
    LineComment,
    /// `/* … */`.
    BlockComment,
    /// `/*+ … */`(Oracle 힌트).
    Hint,
    /// `( ) , ; .`
    Punct,
    /// 연산자(`=` `<>` `||` `+` …).
    Op,
    /// `:name` · `@name` · `?` · `$1`.
    Bind,
}

/// 토큰 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub text: String,
    /// 이 토큰 앞 공백에 든 줄 바꿈 수(0 = 앞 토큰과 같은 줄).
    pub nl_before: u32,
    /// 원문 바이트 범위.
    pub span: (usize, usize),
}

impl Token {
    /// 대문자 비교용(Word만 의미 있음).
    #[must_use]
    pub fn up(&self) -> String {
        self.text.to_ascii_uppercase()
    }
    /// 이 Word가 `w`(대문자)인가.
    #[must_use]
    pub fn is(&self, w: &str) -> bool {
        self.kind == Kind::Word && self.text.eq_ignore_ascii_case(w)
    }
    #[must_use]
    pub fn is_punct(&self, p: &str) -> bool {
        self.kind == Kind::Punct && self.text == p
    }
    #[must_use]
    pub fn is_comment(&self) -> bool {
        matches!(self.kind, Kind::LineComment | Kind::BlockComment)
    }
}

fn is_word_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$' | '#')
}

/// 원문 → 토큰 목록.
#[must_use]
pub fn lex(src: &str) -> Vec<Token> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut nl: u32 = 0;
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    // 바이트 위치 → 문자 인덱스 조회는 필요 없다 — 문자 단위로 걷되 바이트 위치를 함께 든다.
    let mut ci = 0usize;
    while ci < chars.len() {
        let (pos, c) = chars[ci];
        let i = pos;
        // 공백.
        if c.is_whitespace() {
            if c == '\n' {
                nl += 1;
            }
            ci += 1;
            continue;
        }
        let start = i;
        let mut push =
            |kind: Kind, end: usize, ci_next: usize, nl_v: &mut u32, ci_ref: &mut usize| {
                out.push(Token {
                    kind,
                    text: src[start..end].to_string(),
                    nl_before: *nl_v,
                    span: (start, end),
                });
                *nl_v = 0;
                *ci_ref = ci_next;
            };
        let rest = &src[i..];
        // 주석.
        if rest.starts_with("--") {
            let end = rest.find('\n').map_or(n, |k| i + k);
            let end = trim_cr(src, end);
            let next = chars.partition_point(|(p, _)| *p < end);
            push(Kind::LineComment, end, next, &mut nl, &mut ci);
            continue;
        }
        if rest.starts_with("/*") {
            let end = rest.find("*/").map_or(n, |k| i + k + 2);
            let kind = if rest.starts_with("/*+") {
                Kind::Hint
            } else {
                Kind::BlockComment
            };
            let next = chars.partition_point(|(p, _)| *p < end);
            // 블록 주석 안의 줄 바꿈은 배치에 영향 없음(원문 그대로 실린다).
            push(kind, end, next, &mut nl, &mut ci);
            continue;
        }
        // 문자열 `'…'` (`''` 이스케이프).
        if c == '\'' {
            let mut j = ci + 1;
            loop {
                if j >= chars.len() {
                    break;
                }
                if chars[j].1 == '\'' {
                    if j + 1 < chars.len() && chars[j + 1].1 == '\'' {
                        j += 2;
                        continue;
                    }
                    j += 1;
                    break;
                }
                j += 1;
            }
            let end = if j < chars.len() { chars[j].0 } else { n };
            push(Kind::Str, end, j, &mut nl, &mut ci);
            continue;
        }
        // 인용 식별자.
        if c == '"' || c == '`' || c == '[' {
            let close = match c {
                '"' => '"',
                '`' => '`',
                _ => ']',
            };
            let mut j = ci + 1;
            while j < chars.len() && chars[j].1 != close && chars[j].1 != '\n' {
                j += 1;
            }
            if j < chars.len() && chars[j].1 == close {
                j += 1;
            }
            let end = if j < chars.len() { chars[j].0 } else { n };
            push(Kind::Quoted, end, j, &mut nl, &mut ci);
            continue;
        }
        // 바인드.
        if (c == ':' || c == '@') && ci + 1 < chars.len() && is_word_start(chars[ci + 1].1) {
            let mut j = ci + 1;
            while j < chars.len() && is_word_char(chars[j].1) {
                j += 1;
            }
            let end = if j < chars.len() { chars[j].0 } else { n };
            push(Kind::Bind, end, j, &mut nl, &mut ci);
            continue;
        }
        if c == '?' {
            push(Kind::Bind, i + 1, ci + 1, &mut nl, &mut ci);
            continue;
        }
        if c == '$' && ci + 1 < chars.len() && chars[ci + 1].1.is_ascii_digit() {
            let mut j = ci + 1;
            while j < chars.len() && chars[j].1.is_ascii_digit() {
                j += 1;
            }
            let end = if j < chars.len() { chars[j].0 } else { n };
            push(Kind::Bind, end, j, &mut nl, &mut ci);
            continue;
        }
        // 숫자.
        if c.is_ascii_digit()
            || (c == '.' && ci + 1 < chars.len() && chars[ci + 1].1.is_ascii_digit())
        {
            let mut j = ci + 1;
            let mut seen_e = false;
            while j < chars.len() {
                let d = chars[j].1;
                if d.is_ascii_digit() || d == '.' {
                    j += 1;
                } else if (d == 'e' || d == 'E') && !seen_e {
                    seen_e = true;
                    j += 1;
                    if j < chars.len() && (chars[j].1 == '+' || chars[j].1 == '-') {
                        j += 1;
                    }
                } else {
                    break;
                }
            }
            let end = if j < chars.len() { chars[j].0 } else { n };
            push(Kind::Number, end, j, &mut nl, &mut ci);
            continue;
        }
        // 단어.
        if is_word_start(c) {
            let mut j = ci + 1;
            while j < chars.len() && is_word_char(chars[j].1) {
                j += 1;
            }
            let end = if j < chars.len() { chars[j].0 } else { n };
            push(Kind::Word, end, j, &mut nl, &mut ci);
            continue;
        }
        // 구두점.
        if matches!(c, '(' | ')' | ',' | ';' | '.') {
            push(Kind::Punct, i + c.len_utf8(), ci + 1, &mut nl, &mut ci);
            continue;
        }
        // 연산자(긴 것 먼저).
        const OPS: [&str; 13] = [
            "->>", "<>", "!=", "<=", ">=", "||", ":=", "=>", "**", "->", "::", "<<", ">>",
        ];
        let mut took = false;
        for op in OPS {
            if rest.starts_with(op) {
                let end = i + op.len();
                let next = chars.partition_point(|(p, _)| *p < end);
                push(Kind::Op, end, next, &mut nl, &mut ci);
                took = true;
                break;
            }
        }
        if took {
            continue;
        }
        push(Kind::Op, i + c.len_utf8(), ci + 1, &mut nl, &mut ci);
    }
    out
}

fn trim_cr(src: &str, end: usize) -> usize {
    if end > 0 && src.as_bytes()[end - 1] == b'\r' {
        end - 1
    } else {
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(Kind, String)> {
        lex(src).into_iter().map(|t| (t.kind, t.text)).collect()
    }

    #[test]
    fn lexes_comments_strings_binds_ops() {
        let v =
            kinds("SELECT 'it''s', \"Q\", :V_X, @p, $1, ? -- c\n/*+ h */ /* b */ A.B <> 1.5e3;");
        let texts: Vec<&str> = v.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(
            texts,
            [
                "SELECT", "'it''s'", ",", "\"Q\"", ",", ":V_X", ",", "@p", ",", "$1", ",", "?",
                "-- c", "/*+ h */", "/* b */", "A", ".", "B", "<>", "1.5e3", ";"
            ]
        );
        assert_eq!(v[1].0, Kind::Str);
        assert_eq!(v[3].0, Kind::Quoted);
        assert_eq!(v[5].0, Kind::Bind);
        assert_eq!(v[12].0, Kind::LineComment);
        assert_eq!(v[13].0, Kind::Hint);
        assert_eq!(v[14].0, Kind::BlockComment);
        assert_eq!(v[18].0, Kind::Op);
        assert_eq!(v[19].0, Kind::Number);
    }

    #[test]
    fn tracks_newlines_and_spans() {
        let t = lex("A\n\n  B");
        assert_eq!(t[0].nl_before, 0);
        assert_eq!(t[1].nl_before, 2);
        assert_eq!(t[1].span, (5, 6));
        let t = lex("한글_컬럼 = 1");
        assert_eq!(t[0].text, "한글_컬럼");
        assert_eq!(t[0].kind, Kind::Word);
    }

    #[test]
    fn line_comment_excludes_cr() {
        let t = lex("A -- x\r\nB");
        assert_eq!(t[1].text, "-- x");
        assert_eq!(t[2].nl_before, 1);
    }
}
