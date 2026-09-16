//! 정규식 공용(D-76 · 사용자 09-16): 찾기/바꾸기 · (후속) 파일 찾기/바꾸기 · 그 밖의 패턴 기능이 **같은 엔진·같은 규약**을 쓴다.
//!
//! 엔진 = `fancy-regex`(`regex` 위 · lookaround/역참조가 있는 패턴만 백트래킹 · 없으면 `regex`에 위임 = 선형 시간).
//! 치환 규약 = `$1` · `${name}` · `$$`(regex/Sublime과 같음) — 정규식이 꺼져 있으면 치환문은 **글자 그대로**.
//! 편집기는 **문자 인덱스**를 쓰므로 바이트 오프셋을 여기서 변환한다.

use fancy_regex::{Expander, Regex};

/// 패턴 → 정규식. `regex`가 꺼져 있으면 글자 그대로(이스케이프) · `whole_word` = `\b…\b` · `case_sensitive`가 꺼져 있으면 `(?i)`.
pub(crate) fn compile(
    pattern: &str,
    regex: bool,
    case_sensitive: bool,
    whole_word: bool,
) -> Result<Regex, String> {
    let body = if regex {
        pattern.to_string()
    } else {
        fancy_regex::escape(pattern).into_owned()
    };
    let body = if whole_word {
        format!("\\b(?:{body})\\b")
    } else {
        body
    };
    let full = if case_sensitive {
        body
    } else {
        format!("(?i){body}")
    };
    Regex::new(&full).map_err(|e| e.to_string())
}

/// 본문의 모든 일치 — **문자 인덱스** `(시작, 끝)`. 빈 일치는 한 글자 전진(무한 루프 방지).
pub(crate) fn find_all(rx: &Regex, text: &str) -> Vec<(usize, usize)> {
    let map = ByteToChar::new(text);
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos <= text.len() {
        let Ok(Some(m)) = rx.find_from_pos(text, pos) else {
            break;
        };
        let (s, e) = (m.start(), m.end());
        out.push((map.at(s), map.at(e)));
        pos = if e > s {
            e
        } else {
            // 빈 일치: 다음 문자 경계로.
            match text[e..].chars().next() {
                Some(c) => e + c.len_utf8(),
                None => break,
            }
        };
    }
    out
}

/// 문자 인덱스 구간의 일치에 치환 템플릿을 펼친다(`$1` 등). 그 자리에 일치가 없으면 템플릿 그대로.
pub(crate) fn expand_at(rx: &Regex, text: &str, start_char: usize, template: &str) -> String {
    let byte = char_to_byte(text, start_char);
    match rx.captures_from_pos(text, byte) {
        Ok(Some(caps)) if caps.get(0).is_some_and(|m| m.start() == byte) => {
            Expander::default().expansion(template, &caps)
        }
        _ => template.to_string(),
    }
}

/// 문자 인덱스 → 바이트 오프셋.
pub(crate) fn char_to_byte(text: &str, ci: usize) -> usize {
    text.char_indices().nth(ci).map_or(text.len(), |(b, _)| b)
}

/// 바이트 오프셋 → 문자 인덱스(정렬 표 + 이진 탐색).
struct ByteToChar {
    starts: Vec<usize>,
}

impl ByteToChar {
    fn new(text: &str) -> Self {
        let mut starts: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        starts.push(text.len());
        ByteToChar { starts }
    }

    fn at(&self, byte: usize) -> usize {
        match self.starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn literal_and_regex_char_indices() {
        let text = "한글 abc ABC abc";
        let rx = compile("abc", false, false, false).unwrap();
        assert_eq!(find_all(&rx, text), vec![(3, 6), (7, 10), (11, 14)]);
        let rx = compile("abc", false, true, true).unwrap();
        assert_eq!(find_all(&rx, text), vec![(3, 6), (11, 14)]);
        let rx = compile(r"(\w)(\w)c", true, true, false).unwrap();
        assert_eq!(find_all(&rx, text), vec![(3, 6), (11, 14)]);
        assert_eq!(expand_at(&rx, text, 3, "$2$1-c"), "ba-c");
    }

    #[test]
    fn fancy_lookaround_and_bad_pattern() {
        let rx = compile(r"(?<=a)b", true, true, false).unwrap();
        assert_eq!(find_all(&rx, "ab cb ab"), vec![(1, 2), (7, 8)]);
        assert!(compile("(", true, true, false).is_err());
        // 빈 일치는 전진한다.
        let rx = compile("x*", true, true, false).unwrap();
        assert_eq!(find_all(&rx, "ab").len(), 3);
    }
}
