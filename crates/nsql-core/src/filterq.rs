//! ★ **필터 질의 언어**(사용자 09-30 "객체 필터에서 용량 기준 검색 · `Size>1G` · 논리 조합 · 다른 탐색기·북마크에서도") — docs/98.
//!
//! 스터디한 도구(Everything · Windows 검색 AQS · GitHub 검색 · Gmail · Spotlight)의 공통 문법을 따른다:
//! - 낱말 = 이름 부분 일치(대소문자 무시 · 한글 자모) · 공백 = AND · `|`/`OR` = OR · `!`/`NOT` = NOT · 괄호 · `"구절"`.
//! - 술어 = `키:값` · `키>값` `키>=값` `키<값` `키<=값` `키=값` `키!=값` · `키:>값`(Everything/AQS 식도 허용) · 범위 `키:1M..1G`.
//! - 용량 = 숫자 + 단위 `B K M G T P`(`KB` `MB` … 도 · 소수 허용 · 단위 없으면 바이트) · `size:1G` 하나만 쓰면 `>=`.
//! - 값이 없는 사실(용량을 아직 안 읽은 객체 · 파일이 아닌 항목)에 대한 술어는 **거짓**(제외) — 호출자가 문서로 알린다.
//!
//! 의존 0 · 순수(파서·평가기 모두 값만) · 호스트는 [`Facts`]로 이름·종류·용량을 준다.

use std::cmp::Ordering;

/// 판정 대상이 내는 사실.
pub trait Facts {
    /// 이름(낱말 판정 · 원문 — 한글 자모 비교에 쓴다).
    fn text(&self) -> &str;
    /// 종류(소문자 · 예 `table` `view` `index` · 파일이면 `file` `folder`) — 없으면 None.
    fn kind(&self) -> Option<&str> {
        None
    }
    /// 용량(바이트) — 모르면 None.
    fn size(&self) -> Option<u64> {
        None
    }
    /// 그 밖의 키(`ext` `path` `schema` …) — 없으면 None.
    fn field(&self, _key: &str) -> Option<String> {
        None
    }
}

/// 이름만 있는 사실.
#[derive(Debug)]
pub struct TextFacts<'a>(pub &'a str);

impl Facts for TextFacts<'_> {
    fn text(&self) -> &str {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    /// `키:값`(용량 = `>=` · 글 = 부분 일치).
    Has,
    /// `a..b`(양끝 포함).
    Range,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// 낱말(소문자 · 자모열은 평가 때).
    Term(String),
    Pred {
        key: String,
        op: Op,
        value: String,
        /// 범위의 위쪽.
        hi: Option<String>,
    },
    And(Vec<Node>),
    Or(Vec<Node>),
    Not(Box<Node>),
}

/// 파싱된 질의.
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub root: Option<Node>,
    /// 술어 키 목록(호스트가 "용량 술어가 있다" 등을 알 때).
    pub keys: Vec<String>,
}

impl Query {
    #[must_use]
    pub fn has_key(&self, key: &str) -> bool {
        self.keys.iter().any(|k| k == key)
    }
}

/// 이 글에 구조(연산자·술어·괄호·따옴표)가 있는가 — 없으면 호출자는 종전의 낱말 판정을 그대로 쓴다(빠른 길).
#[must_use]
pub fn is_structured(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    t.contains('|')
        || t.contains('(')
        || t.contains(')')
        || t.contains('"')
        || t.split_whitespace().any(|w| {
            let up = w.to_ascii_uppercase();
            w.starts_with('!') || up == "OR" || up == "NOT" || up == "AND" || is_pred_word(w)
        })
}

fn is_pred_word(w: &str) -> bool {
    // `키:` `키>` `키<` `키=` `키!=` — 키는 글자로 시작하는 낱말.
    let Some(pos) = w.find([':', '>', '<', '=', '!']) else {
        return false;
    };
    pos > 0
        && w[..pos]
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '_')
}

// ───────────────────────── 토큰 ─────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    LParen,
    RParen,
    Or,
    And,
    Not,
    /// 낱말·구절·술어 원문(따옴표 안은 그대로).
    Word(String, bool),
}

fn lex(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let cs: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            '|' => {
                out.push(Tok::Or);
                i += 1;
            }
            '!' if i + 1 < cs.len() && cs[i + 1] != '=' => {
                out.push(Tok::Not);
                i += 1;
            }
            '"' => {
                let mut j = i + 1;
                let mut s = String::new();
                while j < cs.len() && cs[j] != '"' {
                    s.push(cs[j]);
                    j += 1;
                }
                out.push(Tok::Word(s, true));
                i = (j + 1).min(cs.len());
            }
            _ => {
                let mut j = i;
                let mut s = String::new();
                while j < cs.len() && !cs[j].is_whitespace() && !matches!(cs[j], '(' | ')' | '|') {
                    // `키:"구절"` — 따옴표 안은 그대로.
                    if cs[j] == '"' {
                        j += 1;
                        while j < cs.len() && cs[j] != '"' {
                            s.push(cs[j]);
                            j += 1;
                        }
                        j += 1;
                        continue;
                    }
                    s.push(cs[j]);
                    j += 1;
                }
                let up = s.to_ascii_uppercase();
                out.push(match up.as_str() {
                    "OR" => Tok::Or,
                    "AND" => Tok::And,
                    "NOT" => Tok::Not,
                    _ => Tok::Word(s, false),
                });
                i = j;
            }
        }
    }
    out
}

// ───────────────────────── 파서 ─────────────────────────

struct P {
    toks: Vec<Tok>,
    i: usize,
    keys: Vec<String>,
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }
    fn or(&mut self) -> Option<Node> {
        let mut parts = Vec::new();
        if let Some(a) = self.and() {
            parts.push(a);
        }
        while matches!(self.peek(), Some(Tok::Or)) {
            self.i += 1;
            if let Some(a) = self.and() {
                parts.push(a);
            }
        }
        match parts.len() {
            0 => None,
            1 => parts.pop(),
            _ => Some(Node::Or(parts)),
        }
    }
    fn and(&mut self) -> Option<Node> {
        let mut parts = Vec::new();
        loop {
            match self.peek() {
                Some(Tok::And) => {
                    self.i += 1;
                }
                Some(Tok::Or) | Some(Tok::RParen) | None => break,
                _ => {}
            }
            match self.not() {
                Some(n) => parts.push(n),
                None => break,
            }
        }
        match parts.len() {
            0 => None,
            1 => parts.pop(),
            _ => Some(Node::And(parts)),
        }
    }
    fn not(&mut self) -> Option<Node> {
        if matches!(self.peek(), Some(Tok::Not)) {
            self.i += 1;
            return self.not().map(|n| Node::Not(Box::new(n)));
        }
        self.atom()
    }
    fn atom(&mut self) -> Option<Node> {
        match self.peek().cloned() {
            Some(Tok::LParen) => {
                self.i += 1;
                let inner = self.or();
                if matches!(self.peek(), Some(Tok::RParen)) {
                    self.i += 1;
                }
                inner
            }
            Some(Tok::Word(w, quoted)) => {
                self.i += 1;
                if quoted {
                    return Some(Node::Term(w.to_lowercase()));
                }
                Some(self.word(&w))
            }
            Some(Tok::RParen) | Some(Tok::Or) | Some(Tok::And) | Some(Tok::Not) | None => None,
        }
    }
    /// 낱말 하나 → 술어 또는 낱말.
    fn word(&mut self, w: &str) -> Node {
        if let Some(pos) = w.find([':', '>', '<', '=', '!']) {
            let key = &w[..pos];
            if pos > 0 && key.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
                let rest = &w[pos..];
                let (op, val) = split_op(rest);
                if !val.is_empty() || op != Op::Has {
                    let key = key.to_ascii_lowercase();
                    if !self.keys.contains(&key) {
                        self.keys.push(key.clone());
                    }
                    // 범위 `a..b`.
                    if let Some((lo, hi)) = val.split_once("..") {
                        return Node::Pred {
                            key,
                            op: Op::Range,
                            value: lo.to_lowercase(),
                            hi: Some(hi.to_lowercase()),
                        };
                    }
                    return Node::Pred {
                        key,
                        op,
                        value: val.to_lowercase(),
                        hi: None,
                    };
                }
            }
        }
        Node::Term(w.to_lowercase())
    }
}

/// `:>1G` `>=1G` `:1G` `=x` `!=x` → (연산, 값).
fn split_op(rest: &str) -> (Op, &str) {
    let r = rest.strip_prefix(':').unwrap_or(rest);
    if let Some(v) = r.strip_prefix(">=") {
        (Op::Ge, v)
    } else if let Some(v) = r.strip_prefix("<=") {
        (Op::Le, v)
    } else if let Some(v) = r.strip_prefix("!=") {
        (Op::Ne, v)
    } else if let Some(v) = r.strip_prefix("<>") {
        (Op::Ne, v)
    } else if let Some(v) = r.strip_prefix('>') {
        (Op::Gt, v)
    } else if let Some(v) = r.strip_prefix('<') {
        (Op::Lt, v)
    } else if let Some(v) = r.strip_prefix('=') {
        (Op::Eq, v)
    } else {
        (Op::Has, r)
    }
}

/// 글 → 질의(잘못된 괄호 등은 있는 데까지 · 절대 실패하지 않는다).
#[must_use]
pub fn parse(text: &str) -> Query {
    let toks = lex(text);
    let mut p = P {
        toks,
        i: 0,
        keys: Vec::new(),
    };
    let root = p.or();
    Query { root, keys: p.keys }
}

// ───────────────────────── 값 ─────────────────────────

/// 용량 글 → 바이트(`1G` `1.5GB` `512k` `1000`).
#[must_use]
pub fn parse_size(v: &str) -> Option<u64> {
    let v = v.trim().to_ascii_uppercase();
    let num_end = v
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(v.len());
    let (num, unit) = v.split_at(num_end);
    let n: f64 = num.parse().ok()?;
    let mul: f64 = match unit.trim_end_matches('B').trim_end_matches("I") {
        "" => 1.0,
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        "T" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        "P" => 1024.0 * 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mul).round() as u64)
}

fn cmp_ok(ord: Ordering, op: Op) -> bool {
    match op {
        Op::Eq => ord == Ordering::Equal,
        Op::Ne => ord != Ordering::Equal,
        Op::Gt => ord == Ordering::Greater,
        Op::Ge => ord != Ordering::Less,
        Op::Lt => ord == Ordering::Less,
        Op::Le => ord != Ordering::Greater,
        Op::Has | Op::Range => true,
    }
}

/// 용량 술어(용량 = `키:값` 은 `>=`).
fn size_pred(size: Option<u64>, op: Op, value: &str, hi: Option<&str>) -> bool {
    let Some(s) = size else { return false };
    let Some(v) = parse_size(value) else {
        return false;
    };
    match op {
        Op::Range => hi
            .and_then(parse_size)
            .is_some_and(|h| s >= v.min(h) && s <= v.max(h)),
        Op::Has => s >= v,
        _ => cmp_ok(s.cmp(&v), op),
    }
}

/// 글 술어(`:` `=` = 부분/정확 일치 · `!=` · 순서 비교는 사전순).
fn text_pred(have: Option<&str>, op: Op, value: &str) -> bool {
    let Some(h) = have else { return false };
    let h = h.to_lowercase();
    match op {
        Op::Has => h.contains(value) || kind_alias(&h, value),
        Op::Eq => h == value || kind_alias(&h, value),
        Op::Ne => !(h == value || kind_alias(&h, value)),
        Op::Range => false,
        _ => cmp_ok(h.as_str().cmp(value), op),
    }
}

/// 종류 별칭(`type:` · 객체 탐색기 종류 이름은 `materializedview`처럼 붙어 있다).
fn kind_alias(have: &str, want: &str) -> bool {
    let aliases: &[(&str, &[&str])] = &[
        ("table", &["tab", "tbl"]),
        ("view", &["vw"]),
        ("materializedview", &["mv", "matview", "mview"]),
        ("procedure", &["proc", "sp"]),
        ("function", &["func", "fn"]),
        ("packagebody", &["body", "pkgbody"]),
        ("package", &["pkg"]),
        ("sequence", &["seq"]),
        ("trigger", &["trg"]),
        ("index", &["idx"]),
        ("synonym", &["syn"]),
        ("column", &["col"]),
        ("folder", &["dir", "directory"]),
    ];
    if have.starts_with(want) {
        return true;
    }
    aliases
        .iter()
        .any(|(k, al)| *k == have && al.contains(&want))
}

fn term_matches(f: &dyn Facts, term: &str) -> bool {
    let hay = f.text();
    if crate::hangul::has_hangul(term) {
        return crate::hangul::contains_jamo(hay, &crate::hangul::decompose(term, true), true);
    }
    hay.to_lowercase().contains(term)
}

fn eval_node(n: &Node, f: &dyn Facts) -> bool {
    match n {
        Node::Term(t) => term_matches(f, t),
        Node::And(v) => v.iter().all(|c| eval_node(c, f)),
        Node::Or(v) => v.iter().any(|c| eval_node(c, f)),
        Node::Not(c) => !eval_node(c, f),
        Node::Pred { key, op, value, hi } => match key.as_str() {
            "size" | "bytes" | "len" => size_pred(f.size(), *op, value, hi.as_deref()),
            "type" | "kind" => text_pred(f.kind(), *op, value),
            "name" => text_pred(Some(f.text()), *op, value),
            other => text_pred(f.field(other).as_deref(), *op, value),
        },
    }
}

impl Query {
    /// 이 사실이 질의에 걸리는가(빈 질의 = 전부).
    #[must_use]
    pub fn matches(&self, f: &dyn Facts) -> bool {
        match &self.root {
            None => true,
            Some(n) => eval_node(n, f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct F<'a>(&'a str, Option<&'a str>, Option<u64>);
    impl Facts for F<'_> {
        fn text(&self) -> &str {
            self.0
        }
        fn kind(&self) -> Option<&str> {
            self.1
        }
        fn size(&self) -> Option<u64> {
            self.2
        }
    }
    const G: u64 = 1 << 30;
    const M: u64 = 1 << 20;

    #[test]
    fn plain_words_are_and_substrings() {
        let q = parse("tb order");
        assert!(q.matches(&F("TB_ORDER_LINE", None, None)));
        assert!(!q.matches(&F("TB_PLAN", None, None)));
        assert!(!is_structured("tb order"));
    }

    #[test]
    fn size_predicates_all_forms() {
        for s in [
            "size>1G",
            "size:>1G",
            "Size > 1G".replace(' ', "").as_str(),
            "size>=1024M",
        ] {
            let q = parse(s);
            assert!(q.has_key("size"), "{s}");
            assert!(q.matches(&F("a", None, Some(2 * G))), "{s}");
            assert!(!q.matches(&F("a", None, Some(500 * M))), "{s}");
            assert!(!q.matches(&F("a", None, None)), "모르면 제외 {s}");
        }
        assert!(parse("size:1G").matches(&F("a", None, Some(G))), "`:` = >=");
        assert!(!parse("size>1G").matches(&F("a", None, Some(G))));
        assert!(parse("size<10K").matches(&F("a", None, Some(1000))));
        assert!(parse("size:100M..1G").matches(&F("a", None, Some(512 * M))));
        assert!(!parse("size:100M..1G").matches(&F("a", None, Some(2 * G))));
        assert!(parse("size=1.5G").matches(&F("a", None, Some(G + G / 2))));
        assert_eq!(parse_size("1kb"), Some(1024));
        assert_eq!(parse_size("2MiB"), Some(2 * M));
        assert_eq!(parse_size("100"), Some(100));
        assert_eq!(parse_size("x"), None);
    }

    #[test]
    fn logic_or_not_parens_quotes() {
        let q = parse("tb_order | tb_plan");
        assert!(q.matches(&F("tb_plan_hist", None, None)));
        assert!(!q.matches(&F("tb_item", None, None)));
        let q = parse("tb !hist");
        assert!(q.matches(&F("tb_plan", None, None)));
        assert!(!q.matches(&F("tb_plan_hist", None, None)));
        let q = parse("(order OR plan) NOT hist");
        assert!(q.matches(&F("tb_order", None, None)));
        assert!(!q.matches(&F("tb_plan_hist", None, None)));
        let q = parse("\"plan hist\"");
        assert!(q.matches(&F("tb plan hist", None, None)));
        assert!(!q.matches(&F("tb_plan_hist", None, None)));
        // 낱말과 술어 섞기 · 괄호 안 OR.
        let q = parse("tb (size>1G | type:view)");
        assert!(q.matches(&F("tb_a", Some("view"), None)));
        assert!(q.matches(&F("tb_b", Some("table"), Some(2 * G))));
        assert!(!q.matches(&F("tb_c", Some("table"), Some(M))));
        assert!(!q.matches(&F("xx", Some("view"), None)));
    }

    #[test]
    fn type_predicate_and_aliases() {
        assert!(parse("type:table").matches(&F("a", Some("table"), None)));
        assert!(parse("type:mv").matches(&F("a", Some("materializedview"), None)));
        assert!(parse("type:proc").matches(&F("a", Some("procedure"), None)));
        assert!(parse("type:idx").matches(&F("a", Some("index"), None)));
        assert!(parse("type!=view").matches(&F("a", Some("table"), None)));
        assert!(!parse("type:view").matches(&F("a", None, None)));
        assert!(parse("kind=table").matches(&F("a", Some("table"), None)));
    }

    #[test]
    fn hangul_terms_use_jamo() {
        assert!(parse("주문").matches(&F("주문테이블", None, None)));
        // 조합 중인 글자("주ㅁ")도 자모열로 걸린다(탐색기 필터와 같은 규칙).
        assert!(parse("주ㅁ").matches(&F("주문테이블", None, None)));
        assert!(!parse("계획").matches(&F("주문테이블", None, None)));
    }

    #[test]
    fn broken_input_never_fails() {
        for s in [
            "(", ")", "a (b", "size>", "size:", "|", "!", "a | | b", "\"open",
        ] {
            let q = parse(s);
            let _ = q.matches(&F("abc", None, Some(1)));
        }
        assert!(parse("").matches(&F("x", None, None)));
        assert!(is_structured("size>1G") && is_structured("a | b") && is_structured("!x"));
        assert!(!is_structured("tb-order") && !is_structured("a.b"));
    }
}
