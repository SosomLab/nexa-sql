//! 매처 — 리터럴(SWAR 후보 스캔) · 대소문자 무시(ASCII 빠른 길 + 비ASCII `char` 경로) · 단어 단위 · 정규식(리터럴 접두 사전 필터).
//!
//! 파일 검색과 열린 탭(메모리 본문) 검색이 같은 매처를 쓴다(docs/36 §2). 바이트 오프셋 `(start, end)`만 돌려주고
//! 줄 번호·문맥은 [`crate::lines`]가 일치 파일에서만 계산한다.

use crate::bytes::{commonness, memchr, memchr2};
use std::fmt;

/// 매처 생성 오류.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// 빈 검색어.
    EmptyQuery,
    /// 정규식 문법 오류(`regex` 메시지).
    Regex(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::EmptyQuery => f.write_str("empty query"),
            Error::Regex(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for Error {}

/// 리터럴 후보 스캔 — `needle`에서 가장 드문 바이트 위치(`scan_at`)를 SWAR로 찾고 그 자리에서만 전체 비교.
#[derive(Debug, Clone)]
struct Lit {
    /// 비교 대상(대소문자 무시면 ASCII 소문자화된 형태).
    needle: Vec<u8>,
    /// ASCII 대소문자 무시.
    fold: bool,
    scan_at: usize,
    /// 스캔 바이트(대소문자 무시면 소문자·대문자 두 벌).
    scan1: u8,
    scan2: u8,
}

impl Lit {
    fn new(needle: &[u8], fold: bool) -> Lit {
        let needle: Vec<u8> = if fold {
            needle.to_ascii_lowercase()
        } else {
            needle.to_vec()
        };
        // 대소문자 무시에서는 글자가 아닌 바이트(한 벌 스캔)가 유리하므로 가산점.
        let mut best = 0;
        let mut best_score = u16::MAX;
        for (i, &b) in needle.iter().enumerate() {
            let mut score = u16::from(commonness(b));
            if fold && b.is_ascii_alphabetic() {
                score += 40;
            }
            if score < best_score {
                best_score = score;
                best = i;
            }
        }
        let b = needle[best];
        Lit {
            fold,
            scan_at: best,
            scan1: b,
            scan2: if fold { b.to_ascii_uppercase() } else { b },
            needle,
        }
    }

    #[inline]
    fn eq_at(&self, hay: &[u8], at: usize) -> bool {
        let n = self.needle.len();
        match hay.get(at..at + n) {
            Some(w) if self.fold => w.eq_ignore_ascii_case(&self.needle),
            Some(w) => w == self.needle.as_slice(),
            None => false,
        }
    }

    /// `from` 이후 첫 일치의 시작.
    fn find(&self, hay: &[u8], from: usize) -> Option<usize> {
        let n = self.needle.len();
        let mut pos = from + self.scan_at;
        while pos < hay.len() {
            let rel = if self.fold {
                memchr2(self.scan1, self.scan2, &hay[pos..])?
            } else {
                memchr(self.scan1, &hay[pos..])?
            };
            let cand = pos + rel;
            let start = cand - self.scan_at;
            if cand >= self.scan_at && start + n <= hay.len() && self.eq_at(hay, start) {
                return Some(start);
            }
            pos = cand + 1;
        }
        None
    }
}

#[derive(Debug)]
enum Kind {
    Literal(Lit),
    /// 비ASCII가 든 검색어의 대소문자 무시 — `char::to_lowercase` 접기(느린 길 · 검색어에 비ASCII 대소문자가 있을 때만).
    Unicode(Vec<char>),
    Regex {
        re: regex::bytes::Regex,
        prefilter: Option<Lit>,
    },
}

/// 컴파일된 검색어. 한 번 만들어 모든 파일·버퍼에 재사용한다(`Sync`).
#[derive(Debug)]
pub struct Matcher {
    kind: Kind,
    word: bool,
}

/// 단어 문자 — ASCII 영숫자·`_` · 비ASCII 바이트는 전부(한글 등 · 유니코드 `\b`와 같은 취급).
#[inline]
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

#[inline]
fn on_word_boundary(hay: &[u8], start: usize, end: usize) -> bool {
    let before = start == 0 || !is_word_byte(hay[start - 1]);
    let after = end >= hay.len() || !is_word_byte(hay[end]);
    before && after
}

/// 검색어에 비ASCII 글자가 있어서 유니코드 접기가 필요한가(대소문자가 있는 글자만 — 한글은 접기가 없으니 바이트 비교로 충분).
fn needs_unicode_fold(q: &str) -> bool {
    q.chars()
        .any(|c| !c.is_ascii() && (c.is_lowercase() || c.is_uppercase()))
}

/// 정규식의 필수 리터럴 접두(사전 필터용). 확신이 없으면 `None`(보수적).
fn literal_prefix(pat: &str) -> Option<String> {
    if pat.contains('|') || pat.starts_with('(') {
        return None;
    }
    let mut out = String::new();
    let mut it = pat.chars().peekable();
    while let Some(c) = it.next() {
        let lit = match c {
            '\\' => match it.next() {
                Some(e) if e.is_ascii_alphanumeric() => break,
                Some(e) => e,
                None => break,
            },
            '.' | '[' | '(' | ')' | '{' | '^' | '$' | '*' | '?' | '+' => break,
            _ => c,
        };
        // 다음이 수량자면 이 글자는 필수가 아니다.
        if matches!(it.peek(), Some('*' | '?' | '+' | '{')) {
            break;
        }
        out.push(lit);
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

impl Matcher {
    /// `case` = 대소문자 구분 · `word` = 단어 단위 · `regex` = `query`를 정규식으로.
    pub fn new(query: &str, case: bool, word: bool, regex: bool) -> Result<Matcher, Error> {
        if query.is_empty() {
            return Err(Error::EmptyQuery);
        }
        let kind = if regex {
            let re = regex::bytes::RegexBuilder::new(query)
                .case_insensitive(!case)
                .multi_line(true)
                .build()
                .map_err(|e| Error::Regex(e.to_string()))?;
            let prefilter = literal_prefix(query)
                .filter(|p| case || p.is_ascii())
                .map(|p| Lit::new(p.as_bytes(), !case));
            Kind::Regex { re, prefilter }
        } else if !case && needs_unicode_fold(query) {
            Kind::Unicode(query.chars().flat_map(char::to_lowercase).collect())
        } else {
            Kind::Literal(Lit::new(query.as_bytes(), !case))
        };
        Ok(Matcher { kind, word })
    }

    /// `hay`의 모든 일치 `(start, end)`를 오름차순으로 `out`에 덧붙인다. `keep_going`이 false를 주면 멈춘다(취소).
    pub fn find_all(
        &self,
        hay: &[u8],
        out: &mut Vec<(usize, usize)>,
        keep_going: &mut dyn FnMut() -> bool,
    ) {
        match &self.kind {
            Kind::Literal(lit) => {
                let n = lit.needle.len();
                let mut from = 0;
                let mut budget = 0u32;
                while let Some(s) = lit.find(hay, from) {
                    if !self.word || on_word_boundary(hay, s, s + n) {
                        out.push((s, s + n));
                    }
                    from = s + n;
                    budget = budget.wrapping_add(1);
                    if budget.is_multiple_of(256) && !keep_going() {
                        return;
                    }
                }
            }
            Kind::Unicode(needle) => {
                for chunk in hay.utf8_chunks() {
                    let valid = chunk.valid();
                    let base = valid.as_ptr() as usize - hay.as_ptr() as usize;
                    let mut budget = 0u32;
                    for (i, _) in valid.char_indices() {
                        if let Some(len) = fold_match_at(&valid[i..], needle) {
                            let (s, e) = (base + i, base + i + len);
                            if !self.word || on_word_boundary(hay, s, e) {
                                out.push((s, e));
                            }
                        }
                        budget = budget.wrapping_add(1);
                        if budget.is_multiple_of(4096) && !keep_going() {
                            return;
                        }
                    }
                }
            }
            Kind::Regex { re, prefilter } => {
                if let Some(p) = prefilter {
                    if p.find(hay, 0).is_none() {
                        return;
                    }
                }
                let mut budget = 0u32;
                for m in re.find_iter(hay) {
                    if !self.word || on_word_boundary(hay, m.start(), m.end()) {
                        out.push((m.start(), m.end()));
                    }
                    budget = budget.wrapping_add(1);
                    if budget.is_multiple_of(256) && !keep_going() {
                        return;
                    }
                }
            }
        }
    }

    /// `hay`에 일치가 하나라도 있는가(사전 판정용 · 전체 스캔 없이 첫 일치에서 멈춘다).
    pub fn is_match(&self, hay: &[u8]) -> bool {
        match &self.kind {
            Kind::Literal(lit) if !self.word => lit.find(hay, 0).is_some(),
            Kind::Regex { re, prefilter } if !self.word => {
                prefilter.as_ref().is_none_or(|p| p.find(hay, 0).is_some()) && re.is_match(hay)
            }
            _ => {
                let mut v = Vec::with_capacity(1);
                self.find_all(hay, &mut v, &mut || true);
                !v.is_empty()
            }
        }
    }
}

/// `s`의 시작에서 접은 글자열이 `needle`과 같으면 소비한 바이트 수.
fn fold_match_at(s: &str, needle: &[char]) -> Option<usize> {
    let mut ni = 0;
    for (i, c) in s.char_indices() {
        for lc in c.to_lowercase() {
            if needle.get(ni) != Some(&lc) {
                return None;
            }
            ni += 1;
        }
        if ni == needle.len() {
            return Some(i + c.len_utf8());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(m: &Matcher, hay: &[u8]) -> Vec<(usize, usize)> {
        let mut v = Vec::new();
        m.find_all(hay, &mut v, &mut || true);
        v
    }

    #[test]
    fn literal_case_sensitive() {
        let m = Matcher::new("SELECT", true, false, false).expect("m");
        assert_eq!(all(&m, b"select SELECT xSELECTx"), vec![(7, 13), (15, 21)]);
        assert!(m.is_match(b"..SELECT"));
        assert!(!m.is_match(b"..select"));
    }

    #[test]
    fn literal_ignore_case_ascii_fast_path() {
        let m = Matcher::new("select", false, false, false).expect("m");
        assert_eq!(all(&m, b"Select sELECT selec"), vec![(0, 6), (7, 13)]);
        // 스캔 바이트가 검색어 중간이어도 앞쪽 경계를 넘지 않는다.
        let m = Matcher::new("xqz", false, false, false).expect("m");
        assert_eq!(all(&m, b"qz XQZ"), vec![(3, 6)]);
    }

    #[test]
    fn literal_word_mode() {
        let m = Matcher::new("emp", true, true, false).expect("m");
        assert_eq!(
            all(&m, b"emp employees emp_x (emp) emp"),
            vec![(0, 3), (21, 24), (26, 29)]
        );
        // 한글 옆은 단어 경계가 아니다(유니코드 \b와 같게).
        let m = Matcher::new("SELECT", true, true, false).expect("m");
        assert!(all(&m, "SELECT한글".as_bytes()).is_empty());
        assert_eq!(all(&m, "한글 SELECT".as_bytes()).len(), 1);
    }

    #[test]
    fn unicode_fold_path() {
        let m = Matcher::new("Éclair", false, false, false).expect("m");
        let hay = "un ÉCLAIR et un éclair".as_bytes();
        let v = all(&m, hay);
        assert_eq!(v.len(), 2);
        assert_eq!(&hay[v[0].0..v[0].1], "ÉCLAIR".as_bytes());
        // 한글은 접기가 없으니 바이트 경로.
        let m = Matcher::new("프로젝트", false, false, false).expect("m");
        assert_eq!(all(&m, "이 프로젝트는".as_bytes()).len(), 1);
    }

    #[test]
    fn regex_with_prefilter_and_word() {
        let m = Matcher::new(r"emp\d+", true, false, true).expect("m");
        assert!(matches!(&m.kind, Kind::Regex { prefilter: Some(p), .. } if p.needle == b"emp"));
        assert_eq!(all(&m, b"emp1 emp emp22"), vec![(0, 4), (9, 14)]);
        let m = Matcher::new(r"emp\d+", false, true, true).expect("m");
        assert_eq!(all(&m, b"EMP1x EMP2"), vec![(6, 10)]);
        assert!(Matcher::new("(", true, false, true).is_err());
        assert!(matches!(
            Matcher::new("", true, false, false),
            Err(Error::EmptyQuery)
        ));
    }

    #[test]
    fn literal_prefix_extraction() {
        assert_eq!(literal_prefix(r"emp\d+").as_deref(), Some("emp"));
        assert_eq!(literal_prefix(r"ab*c").as_deref(), Some("a"));
        assert_eq!(literal_prefix(r"a\.b").as_deref(), Some("a.b"));
        assert_eq!(literal_prefix(r"a|b"), None);
        assert_eq!(literal_prefix(r"(?i)abc"), None);
        assert_eq!(literal_prefix(r"\d+"), None);
        assert_eq!(literal_prefix(r"^SELECT"), None);
    }
}
