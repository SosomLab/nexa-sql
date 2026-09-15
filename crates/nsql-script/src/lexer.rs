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

/// 바이트 오프셋별 [`Class`]를 한 번에 계산한다 — 여러 소비자가 재사용.
pub fn classify(text: &str) -> Vec<Class> {
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
                let mut j = i + 2;
                while j + 1 < n && !(b[j] == b'*' && b[j + 1] == b'/') {
                    j += 1;
                }
                let end = (j + 2).min(n);
                out[i..end].fill(Class::BlockComment);
                i = end;
            }
            b'\'' => {
                let end = scan_single_quoted(b, i);
                out[i..end].fill(Class::Str);
                i = end;
            }
            b'q' | b'Q' if i + 2 < n && b[i + 1] == b'\'' && !prev_is_ident(b, i) => {
                let end = scan_q_quoted(b, i);
                out[i..end].fill(Class::Str);
                i = end;
            }
            // PostgreSQL 달러 인용(`$$ … $$` · `$body$ … $body$`) — 함수/프로시저 본문(사용자 09-15 PG 지원).
            b'$' if !prev_is_ident(b, i) => {
                if let Some(end) = scan_dollar_quoted(b, i) {
                    out[i..end].fill(Class::Str);
                    i = end;
                } else {
                    i += 1;
                }
            }
            b'"' => {
                let mut j = i + 1;
                while j < n && b[j] != b'"' {
                    j += 1;
                }
                let end = (j + 1).min(n);
                out[i..end].fill(Class::Ident);
                i = end;
            }
            b'[' => {
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

fn prev_is_ident(b: &[u8], i: usize) -> bool {
    i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')
}

fn memchr_nl(b: &[u8], from: usize) -> usize {
    b[from..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |p| from + p)
}

/// `'…'` — `''`는 이스케이프. 닫히지 않으면 끝까지.
fn scan_single_quoted(b: &[u8], start: usize) -> usize {
    let n = b.len();
    let mut j = start + 1;
    while j < n {
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

/// PostgreSQL 달러 인용 — 여는 태그 `$[ident]$`를 읽고 같은 태그가 닫을 때까지. 태그가 아니면 None.
fn scan_dollar_quoted(b: &[u8], start: usize) -> Option<usize> {
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
