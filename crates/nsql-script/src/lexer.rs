//! 문자 단위 스캐너 — 문자열·주석·`q'…'` 리터럴을 **건너뛰며** SQL을 걷는다.
//! 문장 분리([`crate::split`])와 바인드 추출([`crate::bind`])이 같은 규칙을 써야
//! 어긋나지 않으므로 여기 한 곳에 둔다.

/// 스캔 중 현재 위치의 성격.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// 코드(식별자·연산자·공백).
    Code,
    /// `'…'` 문자열 리터럴 안(`''` 이스케이프 · Oracle `q'[…]'` 포함).
    Str,
    /// `"…"` 인용 식별자 · `[…]` T-SQL 식별자.
    Ident,
    /// `-- …` 줄 주석.
    LineComment,
    /// `/* … */` 블록 주석.
    BlockComment,
}

/// 바이트 오프셋별 [`Class`]를 한 번에 계산한다 — 여러 소비자가 재사용(방언 무관 1벌 · 백틱 인용 포함).
pub fn classify(text: &str) -> Vec<Class> {
    classify_in(text, None)
}

/// ★ 방언을 아는 분류(135차 · MySQL): `#` 줄 주석 · 문자열 안 백슬래시 이스케이프(`'it\'s'`)는 **MySQL에서만**(Oracle `#`은
/// 식별자 글자 · T-SQL `#temp` · PG 표준 문자열은 `\`가 글자). 백틱 `` `…` ``은 어느 방언에서도 다른 뜻이 없어 늘 인용 식별자.
pub fn classify_in(text: &str, dialect: Option<nsql_core::Dialect>) -> Vec<Class> {
    // T-149 ④(10-30): 방언 표(`Syntax`) — `#` 줄 주석 · 역슬래시 이스케이프 · 달러 인용(방언 모름 = ANSI = 달러 인용만 참).
    let syn = dialect.map_or(nsql_core::caps::Syntax::ansi(), |d| {
        nsql_core::Caps::of(d).syntax
    });
    let (hash_comment, backslash, dollar) =
        (syn.hash_comment, syn.backslash_escape, syn.dollar_quoting);
    // ★ T-324(11-03): 중첩 블록 주석 · `E'…'` 이스케이프 문자열 · `[…]` 식별자 여부도 방언 표에서.
    let (nested, esc_prefix, bracket) = (
        syn.nested_comments,
        syn.escape_string_prefix,
        syn.bracket_ident,
    );
    // ★ T-325(11-04): `U&"…"`·`U&'…'` 유니코드 이스케이프(PostgreSQL · Db2 · ANSI F392/F393)는 한 토큰.
    let unicode_esc = syn.unicode_escapes;
    let b = text.as_bytes();
    let n = b.len();
    let mut out = vec![Class::Code; n];
    let mut i = 0;
    while i < n {
        let c = b[i];
        match c {
            b'-' if i + 1 < n && b[i + 1] == b'-' => {
                let end = memchr_nl(b, i);
                out[i..end].fill(Class::LineComment);
                i = end;
            }
            b'/' if i + 1 < n && b[i + 1] == b'*' => {
                let end = scan_block_comment(b, i, nested);
                out[i..end].fill(Class::BlockComment);
                i = end;
            }
            // PostgreSQL `E'…'`(T-324): 접두가 있는 문자열에서만 `\'`·`\\`가 한 글자(표준 문자열은 `\`가 글자).
            b'E' | b'e' if esc_prefix && i + 1 < n && b[i + 1] == b'\'' && !prev_is_ident(b, i) => {
                let end = scan_single_quoted_in(b, i + 1, true);
                out[i..end].fill(Class::Str);
                i = end;
            }
            b'#' if hash_comment => {
                let end = memchr_nl(b, i);
                out[i..end].fill(Class::LineComment);
                i = end;
            }
            b'\'' => {
                let end = scan_single_quoted_in(b, i, backslash);
                out[i..end].fill(Class::Str);
                i = end;
            }
            b'q' | b'Q' if i + 2 < n && b[i + 1] == b'\'' && !prev_is_ident(b, i) => {
                let end = scan_q_quoted(b, i);
                out[i..end].fill(Class::Str);
                i = end;
            }
            // PostgreSQL 달러 인용(`$$ … $$` · `$body$ … $body$`) — 함수/프로시저 본문(사용자 09-15 PG 지원) · MySQL에는 없다(`DELIMITER $$`가 구분자).
            b'$' if !prev_is_ident(b, i) && dollar => {
                if let Some(end) = scan_dollar_quoted(b, i) {
                    out[i..end].fill(Class::Str);
                    i = end;
                } else {
                    i += 1;
                }
            }
            // 유니코드 이스케이프 접두(T-325): `U&"…"` = 인용 식별자 하나 · `U&'…'` = 문자열 하나(안의 `\XXXX`는 글자 · 표준 문자열 규칙).
            b'U' | b'u'
                if unicode_esc
                    && i + 2 < n
                    && b[i + 1] == b'&'
                    && (b[i + 2] == b'"' || b[i + 2] == b'\'')
                    && !prev_is_ident(b, i) =>
            {
                if b[i + 2] == b'"' {
                    let end = scan_double_quoted(b, i + 2);
                    out[i..end].fill(Class::Ident);
                    i = end;
                } else {
                    let end = scan_single_quoted_in(b, i + 2, false);
                    out[i..end].fill(Class::Str);
                    i = end;
                }
            }
            b'"' => {
                let end = scan_double_quoted(b, i);
                out[i..end].fill(Class::Ident);
                i = end;
            }
            b'`' => {
                let mut j = i + 1;
                while j < n && b[j] != b'`' && b[j] != b'\n' {
                    j += 1;
                }
                let end = (j + 1).min(n);
                out[i..end].fill(Class::Ident);
                i = end;
            }
            // `[…]` 인용 식별자(T-SQL · SQLite) — 배열 첨자 방언(PG `a[1]` · DuckDB · ClickHouse)은 코드로 둔다(T-324).
            b'[' if bracket => {
                let mut j = i + 1;
                while j < n && b[j] != b']' && b[j] != b'\n' {
                    j += 1;
                }
                let end = (j + 1).min(n);
                out[i..end].fill(Class::Ident);
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

/// `/* … */` — `nested`면 안의 `/*`마다 깊이를 세어 짝이 맞는 `*/`에서 닫는다(PostgreSQL · T-SQL · T-324). 닫히지 않으면 끝까지.
fn scan_block_comment(b: &[u8], start: usize, nested: bool) -> usize {
    let n = b.len();
    let mut depth = 1usize;
    let mut j = start + 2;
    while j + 1 < n {
        if b[j] == b'*' && b[j + 1] == b'/' {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return j;
            }
            continue;
        }
        if nested && b[j] == b'/' && b[j + 1] == b'*' {
            depth += 1;
            j += 2;
            continue;
        }
        j += 1;
    }
    n
}

fn prev_is_ident(b: &[u8], i: usize) -> bool {
    i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')
}

fn memchr_nl(b: &[u8], from: usize) -> usize {
    b[from..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |p| from + p)
}

/// `'…'` — `''`는 이스케이프. 닫히지 않으면 끝까지. `backslash` = MySQL 기본(`NO_BACKSLASH_ESCAPES` 꺼짐)처럼 `\'`·`\\`를 한 글자로 건너뛴다.
fn scan_single_quoted_in(b: &[u8], start: usize, backslash: bool) -> usize {
    let n = b.len();
    let mut j = start + 1;
    while j < n {
        if backslash && b[j] == b'\\' {
            j += 2;
            continue;
        }
        if b[j] == b'\'' {
            if j + 1 < n && b[j + 1] == b'\'' {
                j += 2;
                continue;
            }
            return j + 1;
        }
        j += 1;
    }
    n
}

/// `"…"` 인용 식별자 — 닫는 `"`까지(없으면 끝까지 · 종전 인라인 규칙 그대로 · T-325에서 `U&"…"`와 공유).
fn scan_double_quoted(b: &[u8], start: usize) -> usize {
    let n = b.len();
    let mut j = start + 1;
    while j < n && b[j] != b'"' {
        j += 1;
    }
    (j + 1).min(n)
}

/// Oracle `q'[…]'` · `q'{…}'` · `q'(…)'` · `q'<…>'` · `q'X…X'`.
fn scan_q_quoted(b: &[u8], start: usize) -> usize {
    let n = b.len();
    let open = b[start + 2];
    let close = match open {
        b'[' => b']',
        b'{' => b'}',
        b'(' => b')',
        b'<' => b'>',
        c => c,
    };
    let mut j = start + 3;
    while j + 1 < n {
        if b[j] == close && b[j + 1] == b'\'' {
            return j + 2;
        }
        j += 1;
    }
    n
}

/// ★ 위치 `pos`가 든 **달러 인용 본문**의 안쪽 구간(태그를 뺀 `본문 시작..본문 끝`) — 완성이 PL/pgSQL 본문 안에서도 동작하게
/// (사용자 10-09 "PG `$$ … $$` 본문 안 선언 판정"): `classes`의 Str 구간 가운데 `$`로 시작하는 것만 본다. 닫는 태그가 없으면 끝까지.
#[must_use]
pub fn dollar_body_at(text: &str, classes: &[Class], pos: usize) -> Option<(usize, usize)> {
    let b = text.as_bytes();
    let n = b.len().min(classes.len());
    let mut i = 0;
    while i < n {
        if classes[i] != Class::Str {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && classes[i] == Class::Str {
            i += 1;
        }
        if b[start] != b'$' {
            continue;
        }
        let Some(end) = scan_dollar_quoted(b, start) else {
            continue;
        };
        // 태그 = `$ident$`.
        let mut t = start + 1;
        while t < n && b[t] != b'$' {
            t += 1;
        }
        let tag_len = t + 1 - start;
        let inner_start = start + tag_len;
        let closed =
            end >= inner_start + tag_len && b[end - tag_len..end] == b[start..start + tag_len];
        let inner_end = if closed { end - tag_len } else { end.min(n) };
        if pos >= inner_start && pos <= inner_end {
            return Some((inner_start, inner_end));
        }
    }
    None
}

/// PostgreSQL 달러 인용 — 여는 태그 `$[ident]$`를 읽고 같은 태그가 닫을 때까지. 태그가 아니면 None.
pub(crate) fn scan_dollar_quoted(b: &[u8], start: usize) -> Option<usize> {
    let n = b.len();
    let mut j = start + 1;
    while j < n && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
        j += 1;
    }
    if j >= n || b[j] != b'$' {
        return None;
    }
    let tag = &b[start..=j];
    let mut k = j + 1;
    while k + tag.len() <= n {
        if &b[k..k + tag.len()] == tag {
            return Some(k + tag.len());
        }
        k += 1;
    }
    Some(n)
}

/// 식별자 문자(바인드 이름 · 키워드) — Oracle은 `$` `#`도 허용.
pub fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c == b'#'
}

/// 코드 영역에서 `needle`(대소문자 무관 단어)이 나오는 첫 위치.
pub fn find_word_ci(text: &str, classes: &[Class], needle: &str) -> Option<usize> {
    let b = text.as_bytes();
    let nb = needle.as_bytes();
    if nb.is_empty() || b.len() < nb.len() {
        return None;
    }
    (0..=b.len() - nb.len()).find(|&i| {
        classes[i] == Class::Code
            && b[i..i + nb.len()].eq_ignore_ascii_case(nb)
            && (i == 0 || !is_ident_char(b[i - 1]))
            && (i + nb.len() == b.len() || !is_ident_char(b[i + nb.len()]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_comments_and_q_quotes_are_masked() {
        let s = "SELECT 'a;''b' -- ;x\n, q'[;]' /* ; */ , \"x;\" , [y;] FROM t;";
        let c = classify(s);
        let semis: Vec<usize> = s
            .bytes()
            .enumerate()
            .filter(|(i, ch)| *ch == b';' && c[*i] == Class::Code)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(semis, vec![s.len() - 1]);
    }

    #[test]
    fn find_word_skips_strings_and_prefixes() {
        let s = "SELECT 'INTO' , xINTO, x INTO :v FROM d";
        let c = classify(s);
        let p = find_word_ci(s, &c, "into").unwrap();
        assert_eq!(&s[p..p + 4], "INTO");
        assert_eq!(p, s.find(" INTO :v").unwrap() + 1);
    }
}

#[cfg(test)]
mod mysql_tests {
    use super::*;
    use nsql_core::Dialect;

    fn classes_of(text: &str, d: Option<Dialect>) -> String {
        classify_in(text, d)
            .iter()
            .map(|c| match c {
                Class::Code => '.',
                Class::Str => 's',
                Class::Ident => 'i',
                Class::LineComment => '-',
                Class::BlockComment => '*',
            })
            .collect()
    }

    /// 135차 · MySQL: `#` 줄 주석 · 백슬래시 이스케이프는 MySQL만 · 백틱은 공통.
    #[test]
    fn mysql_lexical_rules() {
        assert_eq!(
            classes_of("a # c\nb", Some(Dialect::Mysql)),
            "..---\n.".replace('\n', ".")
        );
        assert_eq!(
            classes_of("a # c", None),
            ".....",
            "Oracle/MSSQL에서 `#`은 식별자 글자"
        );
        // `'it\'s'` = 한 문자열(MySQL) · 다른 방언에서는 `'it\'`로 닫히고 `s'`부터 새 문자열.
        assert_eq!(classes_of("'it\\'s' x", Some(Dialect::Mysql)), "sssssss..");
        assert_eq!(classes_of("'it\\'s' x", None), "sssss.sss");
        assert_eq!(classes_of("`a;b`;", None), "iiiii.");
        assert_eq!(classes_of("`a;b`;", Some(Dialect::Mysql)), "iiiii.");
    }

    /// ★ T-324(11-03 · 108 §9 #1~#6): 중첩 블록 주석(PG·T-SQL = 하나 · Oracle = 첫 `*/`) · PG `E'…'`만 역슬래시 이스케이프 ·
    /// `[…]`는 T-SQL·SQLite에서만 식별자(PG·DuckDB = 첨자 = 코드) · 닫히지 않은 중첩 주석은 끝까지.
    #[test]
    fn t324_nested_comments_escape_strings_and_brackets() {
        let nested = "/* a /* b */ c */ x";
        assert_eq!(
            classes_of(nested, Some(Dialect::Postgres)),
            format!("{}..", "*".repeat(17))
        );
        assert_eq!(
            classes_of(nested, Some(Dialect::Mssql)),
            format!("{}..", "*".repeat(17))
        );
        assert_eq!(
            classes_of(nested, Some(Dialect::Oracle)),
            format!("{}{}", "*".repeat(12), ".".repeat(7)),
            "Oracle = 첫 */에서 닫힘"
        );
        assert_eq!(
            classes_of("/* a /* b */", Some(Dialect::Postgres)),
            "*".repeat(12),
            "미완 중첩 = 끝까지"
        );
        assert_eq!(
            classes_of("E'a\\'b' x", Some(Dialect::Postgres)),
            "sssssss..",
            "PG E'' = 역슬래시 이스케이프"
        );
        assert_eq!(
            classes_of("E'a\\'b' x", Some(Dialect::Oracle)),
            ".ssss.sss",
            "Oracle = E는 코드 · 표준 문자열"
        );
        assert_eq!(
            classes_of("'a\\'b' x", Some(Dialect::Postgres)),
            "ssss.sss",
            "PG 표준 문자열 = 역슬래시는 글자"
        );
        assert_eq!(
            classes_of("xE'a' y", Some(Dialect::Postgres)),
            "..sss..",
            "식별자 꼬리 E는 접두가 아님"
        );
        assert_eq!(
            classes_of("a[1]", Some(Dialect::Postgres)),
            "....",
            "PG 첨자 = 코드"
        );
        // ★ T-325(11-04 · 108 §9 #7 · 109 F392/F393): `U&"…"` = 식별자 하나 · `U&'…'` = 문자열 하나(PG · Db2) · 다른 방언 = `U` 코드 + 인용.
        assert_eq!(
            classes_of("U&\"d\\0061t\" x", Some(Dialect::Postgres)),
            "iiiiiiiiiii..",
            "PG U&\"…\" = 식별자 한 토큰"
        );
        assert_eq!(
            classes_of("U&'a;b' x", Some(Dialect::Db2)),
            "sssssss..",
            "Db2 U&'…' = 문자열 한 토큰(안의 ;는 글자)"
        );
        assert_eq!(
            classes_of("U&\"ab\" x", Some(Dialect::Oracle)),
            "..iiii..",
            "Oracle = U는 코드 · 인용만 식별자"
        );
        assert_eq!(
            classes_of("xU&\"a\" y", Some(Dialect::Postgres)),
            "...iii..",
            "식별자 꼬리 U는 접두가 아님"
        );
        assert_eq!(classes_of("a[1]", Some(Dialect::Duckdb)), "....");
        assert_eq!(
            classes_of("a[1]", Some(Dialect::Mssql)),
            ".iii",
            "T-SQL = 식별자"
        );
        assert_eq!(classes_of("[y;]", Some(Dialect::Sqlite)), "iiii");
    }
}
