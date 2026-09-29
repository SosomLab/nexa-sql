//! 레이아웃 IR + Basic 렌더 + 정렬 보조.
//!
//! 문장 → [`Line`] 목록. 줄은 들여쓰기 단계와 조각([`Part`])으로 되어 있고, 조각 경계는 **정렬이 필요한 자리**(별칭 `AS` ·
//! 비교 연산자 · ORDER BY 방향 · 인라인 주석)에만 둔다. Basic은 조각을 한 칸 띄워 붙이고, 확장(kiros33)은 같은 IR에서
//! 블록별 탭 정렬·구분행 같은 규칙을 얹어 렌더한다.

use crate::lexer::{lex, Kind, Token};
use crate::{
    is_keyword, AliasAs, Comma, DialectTarget, Indent, ListStyle, LogicalNewline, Newline, Options,
};

/// 줄의 역할(확장의 정렬 블록 판정 기준).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// 절 키워드 줄(`SELECT` · `FROM` · `WHERE 1=1` …).
    Clause,
    /// 목록 항목(SELECT 열 · FROM 테이블 · GROUP/ORDER BY 항목 · SET 대입 · VALUES 행 · CTE).
    Item,
    /// 조건(WHERE/ON/HAVING의 AND/OR 줄).
    Cond,
    /// JOIN 줄.
    Join,
    /// 집합 연산자(`UNION ALL` …).
    SetOp,
    /// `)`로 시작하는 닫는 줄.
    Close,
    /// CASE 내부(WHEN/ELSE/END).
    Case,
    /// 독립 주석 줄.
    Comment,
    /// 원문 그대로(DDL · PL/SQL · 한 줄 문장).
    Raw,
    Other,
}

/// 줄 조각.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// 이미 띄어쓰기가 된 글(토큰 나열).
    Text(String),
    /// 줄 앞 콤마(콤마 위치 옵션이 렌더에서 처리).
    Comma,
    /// 별칭 `AS`(뒤에 [`Part::Alias`]).
    As,
    Alias(String),
    /// 비교 연산자(`=` `<>` …) — 정렬 대상.
    CmpOp(String),
    /// ORDER BY 방향(`ASC` · `DESC NULLS LAST`).
    OrderDir(String),
    /// 줄 끝 인라인 주석(`-- …` · `/* … */`).
    Comment(String),
}

/// 줄 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub indent: usize,
    pub parts: Vec<Part>,
    pub role: Role,
    /// 이 줄 앞 빈 줄 수.
    pub blank_before: usize,
}

impl Line {
    fn new(indent: usize, role: Role) -> Line {
        Line {
            indent,
            parts: Vec::new(),
            role,
            blank_before: 0,
        }
    }
    /// 정렬 없이 이은 본문(들여쓰기 제외).
    #[must_use]
    pub fn plain(&self, opts: &Options) -> String {
        render_parts(&self.parts, opts, None)
    }
}

// ───────────────────────── 렌더 ─────────────────────────

/// 줄 접두 = 들여쓰기 + (줄 앞 콤마면) 콤마. 콤마 줄은 **한 단계 위 열에 `,`** 를 두고 `comma_gap` 뒤에 항목(탭 들여쓰기: 탭만 +
/// `,` + 간격 · 공백 n + 공백 간격: `" "*(n-2) + ", "` · 탭 간격: `,\t`). 확장도 이 규칙을 그대로 쓴다.
#[must_use]
pub fn line_prefix(indent: usize, leading_comma: bool, opts: &Options) -> String {
    let unit = opts.indent_unit();
    let mut s = String::new();
    if !leading_comma {
        for _ in 0..indent {
            s.push_str(&unit);
        }
        return s;
    }
    for _ in 0..indent.saturating_sub(1) {
        s.push_str(&unit);
    }
    if indent == 0 {
        s.push(',');
        s.push_str(opts.gap());
        return s;
    }
    // 콤마 뒤 간격은 늘 `comma_gap`(사용자 09-29). 탭 들여쓰기 = 콤마 앞은 탭(정지점)만 · 공백 채움 없음(사용자 09-29 정정) ·
    // 공백 들여쓰기 + 공백 간격 = (폭 - 2)칸 + `, `로 항목이 원래 열에.
    match (opts.indent, opts.comma_gap) {
        (Indent::Tab, gap) => {
            s.push(',');
            s.push_str(match gap {
                crate::Gap::Tab => "\t",
                crate::Gap::Space => " ",
            });
        }
        (Indent::Spaces(_), crate::Gap::Tab) => {
            s.push(',');
            s.push('\t');
        }
        (Indent::Spaces(n), crate::Gap::Space) => {
            for _ in 0..(n as usize).saturating_sub(2) {
                s.push(' ');
            }
            s.push_str(", ");
        }
    }
    s
}

/// 정렬 채움 함수(확장용): (조각 index, 지금까지의 줄 글) → 그 조각 앞에 넣을 글.
pub type Pad<'a> = &'a dyn Fn(usize, &str) -> String;

/// 조각 → 글(들여쓰기·줄 앞 콤마 제외 — 그 둘은 [`line_prefix`]). `pad` = 조각 index → 그 조각 **앞**에 넣을 채움 글
/// (확장의 정렬용 · Basic = None).
#[must_use]
pub fn render_parts(parts: &[Part], opts: &Options, pad: Option<Pad<'_>>) -> String {
    let mut s = String::new();
    // 연산자 공백 끔 = 연산자 **뒤** 조각도 붙여 쓴다(`a= 1`이 아니라 `a=1`).
    let mut tight_next = false;
    for (i, p) in parts.iter().enumerate() {
        let need_space = !tight_next && !s.is_empty() && !s.ends_with('\t') && !s.ends_with(' ');
        tight_next = false;
        match p {
            Part::Comma => {
                // 줄 앞 콤마는 접두가 그린다 · 줄 중간(한 줄 목록)은 `, `.
                if i > 0 {
                    s.push(',');
                    s.push_str(opts.gap());
                }
            }
            Part::Text(t) => {
                let tight = t.starts_with(';')
                    || t.starts_with(')')
                    || t.starts_with(',')
                    || t.starts_with('.');
                if need_space && !s.ends_with('(') && !tight {
                    s.push(' ');
                }
                s.push_str(t);
            }
            // 정렬 채움(확장): 비어 있지 않으면 채움 + 탭 간격 · 비어 있으면(Basic) 보통 띄어쓰기.
            Part::As => {
                // `AS` 앞뒤 = `as_gap`(공백/탭 · 사용자 09-29) · 정렬 채움(확장)이 있으면 앞은 채움.
                let g = match opts.as_gap {
                    crate::Gap::Tab => '\t',
                    crate::Gap::Space => ' ',
                };
                let fill = pad.map(|f| f(i, &s)).unwrap_or_default();
                if !fill.is_empty() {
                    s.push_str(&fill);
                } else if need_space {
                    s.push(g);
                }
                s.push_str(&opts.keyword_case.apply("AS"));
                s.push(g);
            }
            Part::Alias(a) => {
                if !s.ends_with(' ') && !s.ends_with('\t') && !s.is_empty() {
                    s.push(' ');
                }
                s.push_str(a);
            }
            Part::CmpOp(op) => {
                let fill = pad.map(|f| f(i, &s)).unwrap_or_default();
                if !fill.is_empty() {
                    // 정렬된 연산자 뒤 = `operator_gap`(4자 이상은 공백 1개 · 확장의 탭 정렬과 공통 규칙).
                    s.push_str(&fill);
                    s.push_str(op);
                    let long = opts.operator_long_space && op.chars().count() >= 4;
                    if opts.operator_gap == crate::Gap::Tab && !long {
                        s.push('\t');
                    } else {
                        s.push(' ');
                    }
                } else if opts.operator_spaces {
                    // 양쪽 공백의 글자 = `operator_gap`(공백/탭 · 사용자 09-29) · 4자 이상 연산자는 **오른쪽만** 공백 1개
                    // (`operator_long_space` · 왼쪽은 설정대로 — 사용자 09-29 정정).
                    let long = opts.operator_long_space && op.chars().count() >= 4;
                    let g = match opts.operator_gap {
                        crate::Gap::Tab => '\t',
                        crate::Gap::Space => ' ',
                    };
                    if need_space {
                        s.push(g);
                    }
                    s.push_str(op);
                    s.push(if long { ' ' } else { g });
                } else {
                    s.push_str(op);
                    tight_next = true;
                }
            }
            Part::OrderDir(d) => {
                let fill = pad.map(|f| f(i, &s)).unwrap_or_default();
                if !fill.is_empty() {
                    s.push_str(&fill);
                } else if need_space {
                    s.push(' ');
                }
                s.push_str(d);
            }
            Part::Comment(c) => {
                if !s.is_empty() {
                    if opts.comma_gap == crate::Gap::Tab {
                        s.push('\t');
                    } else {
                        s.push_str("  ");
                    }
                }
                s.push_str(c);
            }
        }
    }
    // 조각 끝의 여분 공백 제거.
    while s.ends_with(' ') || s.ends_with('\t') {
        s.pop();
    }
    s
}

/// Basic 렌더 — 줄마다 들여쓰기 + 조각 · 콤마 위치 · `;` 줄 · 빈 줄 · 개행 · 끝 개행.
#[must_use]
pub fn render(lines: &[Line], opts: &Options, src: &str) -> String {
    render_lines(lines, opts, src, &|_, _, _| String::new())
}

/// 렌더(정렬 채움 포함) — `pad(줄 index, 조각 index, 지금까지의 줄 글)` = 그 조각 앞에 넣을 채움(확장의 탭 정렬 · Basic = 빈 글).
#[must_use]
pub fn render_lines(
    lines: &[Line],
    opts: &Options,
    src: &str,
    pad: &dyn Fn(usize, usize, &str) -> String,
) -> String {
    let unit = opts.indent_unit();
    let nl = match opts.newline {
        Newline::Lf => "\n",
        Newline::CrLf => "\r\n",
        Newline::Keep => {
            if src.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            }
        }
    };
    let padded = |li: usize, parts: &[Part]| -> String {
        let f = |pi: usize, sofar: &str| pad(li, pi, sofar);
        render_parts(parts, opts, Some(&f))
    };
    let mut texts: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(li, l)| {
            if l.role == Role::Raw {
                match l.parts.first() {
                    Some(Part::Text(t)) => t.clone(),
                    _ => String::new(),
                }
            } else {
                padded(li, &l.parts)
            }
        })
        .collect();
    // 콤마 줄 끝 옵션: 다음 줄이 콤마로 시작하면 이 줄 끝으로 옮긴다(인라인 주석 앞에).
    if opts.comma == Comma::Trailing {
        for i in 0..lines.len() {
            if lines[i].role == Role::Raw {
                continue;
            }
            let next_comma = lines.get(i + 1).is_some_and(|n| {
                n.role != Role::Raw && matches!(n.parts.first(), Some(Part::Comma))
            });
            if next_comma {
                let mut without: Vec<Part> = lines[i + 1].parts.clone();
                without.remove(0);
                texts[i + 1] = padded(i + 1, &without);
                // 이 줄: 주석 앞에 콤마.
                let cut = lines[i]
                    .parts
                    .iter()
                    .position(|p| matches!(p, Part::Comment(_)));
                match cut {
                    Some(c) => {
                        let head = padded(i, &lines[i].parts[..c]);
                        let tail = render_parts(&lines[i].parts[c..], opts, None);
                        texts[i] = format!("{head},  {tail}");
                    }
                    None => texts[i].push(','),
                }
            }
        }
    }
    let _ = &unit;
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        for _ in 0..l.blank_before.min(opts.max_blank_lines) {
            out.push_str(nl);
        }
        let body = &texts[i];
        if l.role != Role::Raw {
            let leading =
                opts.comma == Comma::Leading && matches!(l.parts.first(), Some(Part::Comma));
            out.push_str(&line_prefix(l.indent, leading, opts));
        }
        out.push_str(body.trim_end());
        out.push_str(nl);
    }
    // 끝 개행 하나로.
    while out.ends_with(nl) {
        out.truncate(out.len() - nl.len());
    }
    if opts.final_newline {
        out.push_str(nl);
    }
    out
}

// ───────────────────────── 정렬 보조(확장용) ─────────────────────────

/// 글자 폭(탭은 다음 탭 스톱까지).
#[must_use]
pub fn display_width(s: &str, tab_width: usize) -> usize {
    let mut w = 0usize;
    for c in s.chars() {
        if c == '\t' {
            w = (w / tab_width + 1) * tab_width;
        } else {
            w += 1;
        }
    }
    w
}

/// `from`(지금 폭)에서 `to` 스톱까지 탭으로 채우는 문자열(최소 탭 1개 · 탭 폭 기준). `to`는 탭 스톱(배수)이어야 한다.
#[must_use]
pub fn tabs_to(from: usize, to: usize, tab_width: usize) -> String {
    let mut s = String::new();
    let mut w = from;
    loop {
        s.push('\t');
        w = (w / tab_width + 1) * tab_width;
        if w >= to {
            break;
        }
    }
    s
}

/// 연속 줄을 **정렬 블록**(같은 들여쓰기 · 같은 역할 · 사이에 다른 줄 없음)으로 나눈다 — 확장이 블록마다 정렬 열을 정한다.
#[must_use]
pub fn blocks(lines: &[Line]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let (ind, role) = (lines[i].indent, lines[i].role);
        let mut j = i + 1;
        while j < lines.len() && lines[j].indent == ind && lines[j].role == role {
            j += 1;
        }
        out.push((i, j));
        i = j;
    }
    out
}

// ───────────────────────── 레이아웃 ─────────────────────────

/// 원문 → 줄 IR(문장별 · 통과 문장은 Raw 줄).
#[must_use]
pub fn layout(src: &str, opts: &Options) -> Vec<Line> {
    let toks = substitute_dialect(lex(src), opts.dialect_target);
    let mut out: Vec<Line> = Vec::new();
    let mut i = 0usize;
    let mut first_stmt = true;
    while i < toks.len() {
        // 문장 앞 독립 주석.
        let stmt_start = i;
        // 문장 끝 = 최상위 `;`(괄호 밖) — 통과 대상(PL/SQL·CREATE 루틴)은 끝까지.
        if starts_block_passthrough(&toks[i..]) {
            let text = src[toks[i].span.0..].trim_end();
            push_raw(&mut out, text, blank_of(&toks[i], first_stmt, opts));
            break;
        }
        let mut depth = 0i32;
        let mut j = i;
        while j < toks.len() {
            let t = &toks[j];
            if t.is_punct("(") {
                depth += 1;
            } else if t.is_punct(")") {
                depth -= 1;
            } else if t.is_punct(";") && depth <= 0 {
                j += 1;
                break;
            }
            j += 1;
        }
        let stmt = &toks[stmt_start..j];
        let blank = blank_of(&stmt[0], first_stmt, opts);
        format_statement(&mut out, stmt, src, opts, blank);
        first_stmt = false;
        i = j;
    }
    out
}

fn blank_of(first: &Token, first_stmt: bool, opts: &Options) -> usize {
    if first_stmt {
        0
    } else {
        let orig = first.nl_before.saturating_sub(1) as usize;
        orig.max(opts.stmt_blank_lines).min(opts.max_blank_lines)
    }
}

fn push_raw(out: &mut Vec<Line>, text: &str, blank: usize) {
    let mut first = true;
    for raw in text.lines() {
        let mut l = Line::new(0, Role::Raw);
        l.parts.push(Part::Text(raw.trim_end().to_string()));
        if first {
            l.blank_before = blank;
            first = false;
        }
        out.push(l);
    }
}

/// 문장 첫 토큰이 PL/SQL 블록·루틴 생성이면 그 뒤 전부 통과(안의 `;`로 쪼개면 안 된다).
fn starts_block_passthrough(toks: &[Token]) -> bool {
    let words: Vec<String> = toks
        .iter()
        .filter(|t| !t.is_comment())
        .take(4)
        .map(Token::up)
        .collect();
    let Some(first) = words.first() else {
        return false;
    };
    match first.as_str() {
        "DECLARE" | "BEGIN" => true,
        "CREATE" => words.iter().any(|w| {
            matches!(
                w.as_str(),
                "PROCEDURE" | "FUNCTION" | "PACKAGE" | "TRIGGER" | "TYPE" | "BODY"
            )
        }),
        _ => false,
    }
}

fn format_statement(out: &mut Vec<Line>, stmt: &[Token], src: &str, opts: &Options, blank: usize) {
    // 앞머리 주석은 독립 줄.
    let mut k = 0;
    let mut blank = blank;
    while k < stmt.len() && stmt[k].is_comment() {
        let mut l = Line::new(0, Role::Comment);
        l.parts.push(Part::Text(stmt[k].text.clone()));
        l.blank_before = blank;
        blank = 0;
        out.push(l);
        k += 1;
    }
    let body = &stmt[k..];
    if body.is_empty() {
        return;
    }
    let first = body[0].up();
    let known = matches!(
        first.as_str(),
        "SELECT" | "WITH" | "INSERT" | "UPDATE" | "DELETE" | "MERGE"
    ) || (first == "(" && false);
    let text = src[body[0].span.0..body[body.len() - 1].span.1].trim();
    // §19 한 줄 문장 유지 · INSERT 두 줄(컬럼 한 줄 + VALUES 한 줄) 유지.
    if opts.keep_oneliners && known && is_oneliner(text, &first, opts.line_width) {
        push_raw(out, text, blank);
        return;
    }
    if !known {
        push_raw(out, text, blank);
        return;
    }
    let stmt_words: std::collections::HashSet<String> = body
        .iter()
        .filter(|t| t.kind == Kind::Word)
        .map(Token::up)
        .collect();
    let mut w = Walker {
        toks: body,
        i: 0,
        out,
        opts,
        cur: None,
        in_from: false,
        stmt_words,
        alias_next: 0,
    };
    w.statement(0);
    w.flush();
    // 첫 줄에 빈 줄 수.
    if let Some(l) = w.out.iter_mut().rev().find(|_| true) {
        let _ = l;
    }
    // (첫 줄은 statement() 시작에서 push됐다 — 찾아서 blank 적용.)
    let start = out.len();
    let _ = start;
    // blank은 out에서 이 문장의 첫 줄에 넣는다: 문장 시작 전 길이를 기억하지 못했으니 역순으로 찾는다.
    apply_blank(out, blank, body);
}

/// 이 문장의 첫 줄(문장 시작 토큰의 글을 담은 줄)에 빈 줄 수를 넣는다.
fn apply_blank(out: &mut [Line], blank: usize, body: &[Token]) {
    if blank == 0 {
        return;
    }
    let first_word = body[0].text.clone();
    // 역순으로 첫 조각이 첫 단어로 시작하는 줄 중 가장 앞 것을 찾는다(문장은 방금 추가됐다).
    let mut idx = None;
    for (n, l) in out.iter().enumerate().rev() {
        if let Some(Part::Text(t)) = l.parts.first() {
            if t.get(..first_word.len())
                .is_some_and(|h| h.eq_ignore_ascii_case(&first_word))
            {
                idx = Some(n);
            }
        }
        if l.blank_before > 0 && idx.is_some() {
            break;
        }
    }
    if let Some(n) = idx {
        out[n].blank_before = blank;
    }
}

/// §19: **짧은 DML**(INSERT … VALUES · UPDATE · DELETE)이 한 줄(INSERT는 컬럼 한 줄 + VALUES 한 줄)로 완결되면 작성자의
/// 의도로 보고 펼치지 않는다. SELECT/WITH/MERGE는 대상이 아니다(한 줄 SELECT를 포맷하는 것이 포맷터의 첫 쓰임).
fn is_oneliner(text: &str, first: &str, line_width: usize) -> bool {
    if !matches!(first, "INSERT" | "UPDATE" | "DELETE") {
        return false;
    }
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.iter().any(|l| l.len() > line_width) {
        return false;
    }
    match lines.len() {
        1 => true,
        2 if first == "INSERT" => lines[1]
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("VALUES"),
        _ => false,
    }
}

// ───────────────────────── 걷기 ─────────────────────────

struct Walker<'a> {
    toks: &'a [Token],
    i: usize,
    out: &'a mut Vec<Line>,
    opts: &'a Options,
    cur: Option<Line>,
    /// FROM/JOIN 목록을 걷는 중(별칭 자동 부여 대상 · T-255).
    in_from: bool,
    /// 이 문장에 나온 단어(대문자) + 만든 별칭 — 자동 별칭이 겹치지 않게.
    stmt_words: std::collections::HashSet<String>,
    alias_next: usize,
}

const JOIN_HEADS: &[&str] = &[
    "INNER", "LEFT", "RIGHT", "FULL", "CROSS", "NATURAL", "JOIN", "OUTER",
];

impl<'a> Walker<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.toks.get(self.i)
    }
    fn peek_at(&self, n: usize) -> Option<&'a Token> {
        // 주석을 건너뛴 n번째 토큰.
        let mut k = self.i;
        let mut seen = 0;
        while k < self.toks.len() {
            if !self.toks[k].is_comment() {
                if seen == n {
                    return Some(&self.toks[k]);
                }
                seen += 1;
            }
            k += 1;
        }
        None
    }
    fn peek_is(&self, w: &str) -> bool {
        self.peek_at(0).is_some_and(|t| t.is(w))
    }
    fn peek_phrase(&self, ws: &[&str]) -> bool {
        ws.iter()
            .enumerate()
            .all(|(n, w)| self.peek_at(n).is_some_and(|t| t.is(w)))
    }
    fn next(&mut self) -> Option<&'a Token> {
        let t = self.toks.get(self.i)?;
        self.i += 1;
        Some(t)
    }

    fn flush(&mut self) {
        if let Some(l) = self.cur.take() {
            if !l.parts.is_empty() {
                self.out.push(l);
            }
        }
    }
    fn open(&mut self, indent: usize, role: Role) {
        self.flush();
        self.cur = Some(Line::new(indent, role));
    }
    fn cur_mut(&mut self) -> &mut Line {
        if self.cur.is_none() {
            self.cur = Some(Line::new(0, Role::Other));
        }
        self.cur.as_mut().expect("line")
    }
    /// 현재 줄에 글을 덧붙인다(마지막 Text 조각에 띄어쓰기 규칙으로).
    fn word(&mut self, s: &str) {
        let line = self.cur_mut();
        match line.parts.last_mut() {
            Some(Part::Text(t)) => {
                if need_space(t, s) {
                    t.push(' ');
                }
                t.push_str(s);
            }
            _ => line.parts.push(Part::Text(s.to_string())),
        }
    }
    fn part(&mut self, p: Part) {
        self.cur_mut().parts.push(p);
    }
    /// ★ 줄 앞 AND/OR 뒤 간격(`logical_gap` · 사용자 09-29 "콤마처럼 AND/OR 뒤 공백/탭"): 탭이면 현재 글 조각 끝에 탭을 붙여 다음
    /// 토큰이 그 뒤에 온다(공백은 `word`/`need_space`가 알아서).
    fn logical_gap(&mut self) {
        if self.opts.logical_gap != crate::Gap::Tab {
            return;
        }
        if let Some(Part::Text(t)) = self.cur_mut().parts.last_mut() {
            t.push('\t');
        }
    }
    /// 토큰 하나를 대소문자 규칙대로 현재 줄에.
    fn emit(&mut self, t: &Token) {
        let s = self.cased(t);
        self.word(&s);
    }
    fn cased(&self, t: &Token) -> String {
        match t.kind {
            Kind::Word => {
                if is_keyword(&t.text) {
                    self.opts.keyword_case.apply(&t.text)
                } else if self.toks.get(self.i).is_some_and(|n| n.is_punct("(")) {
                    self.opts.function_case.apply(&t.text)
                } else {
                    self.opts.identifier_case.apply(&t.text)
                }
            }
            _ => t.text.clone(),
        }
    }
    fn kw(&self, w: &str) -> String {
        self.opts.keyword_case.apply(w)
    }
    /// 주석 토큰 처리: 같은 줄이면 인라인 조각, 아니면 독립 줄(현재 들여쓰기).
    fn comment(&mut self, t: &Token, indent: usize) {
        if t.nl_before == 0 && self.cur.as_ref().is_some_and(|l| !l.parts.is_empty()) {
            if t.kind == Kind::BlockComment || t.kind == Kind::Hint {
                // 같은 줄 블록 주석 = 글의 일부(앞뒤 한 칸).
                self.word(&t.text);
                return;
            }
            self.part(Part::Comment(t.text.clone()));
            if t.kind == Kind::LineComment {
                // 줄 주석 뒤에는 반드시 새 줄.
                let ind = self.cur.as_ref().map_or(indent, |l| l.indent);
                let role = self.cur.as_ref().map_or(Role::Other, |l| l.role);
                self.open(ind, role);
            }
        } else {
            self.flush();
            let mut l = Line::new(indent, Role::Comment);
            l.parts.push(Part::Text(t.text.clone()));
            l.blank_before =
                (t.nl_before.saturating_sub(1) as usize).min(self.opts.max_blank_lines);
            self.out.push(l);
        }
    }

    // ── 문장

    fn statement(&mut self, indent: usize) {
        let Some(first) = self.peek_at(0) else { return };
        match first.up().as_str() {
            "SELECT" | "WITH" => self.query(indent),
            "INSERT" => self.insert(indent),
            "UPDATE" => self.update(indent),
            "DELETE" => self.delete(indent),
            "MERGE" => self.merge(indent),
            _ => self.rest_inline(indent),
        }
        // 남은 토큰(`;` 등).
        self.tail(indent);
    }

    /// 문장 끝 `;`·잔여 토큰.
    fn tail(&mut self, indent: usize) {
        while let Some(t) = self.peek() {
            if t.is_comment() {
                let t = self.next().expect("tok");
                self.comment(t, indent);
                continue;
            }
            if t.is_punct(";") {
                self.next();
                if self.opts.semicolon_newline {
                    self.open(indent, Role::Other);
                }
                self.word(";");
                continue;
            }
            let t = self.next().expect("tok");
            self.emit(t);
        }
    }

    fn rest_inline(&mut self, indent: usize) {
        self.open(indent, Role::Other);
        while let Some(t) = self.peek() {
            if t.is_punct(";") {
                break;
            }
            let t = self.next().expect("tok");
            if t.is_comment() {
                self.comment(t, indent);
            } else {
                self.emit(t);
            }
        }
    }

    // ── 질의(SELECT/WITH … 집합 연산 포함) — 닫는 `)`(depth 0)나 `;`에서 멈춘다.

    fn query(&mut self, indent: usize) {
        loop {
            let Some(t) = self.peek_at(0) else { return };
            if t.is_punct(")") || t.is_punct(";") {
                return;
            }
            let up = t.up();
            match up.as_str() {
                "WITH" => self.with_clause(indent),
                "SELECT" => self.select_block(indent),
                "UNION" | "INTERSECT" | "MINUS" | "EXCEPT" => {
                    self.open(indent, Role::SetOp);
                    let t = self.next().expect("tok");
                    self.emit(t);
                    if self.peek_is("ALL") || self.peek_is("DISTINCT") {
                        let t = self.next().expect("tok");
                        self.emit(t);
                    }
                }
                "(" => {
                    // `( SELECT … ) UNION …` 같은 괄호 질의.
                    self.open(indent, Role::Other);
                    self.paren(indent);
                }
                _ => {
                    // 알 수 없는 것 = 인라인으로 흘려보낸다.
                    if self.cur.is_none() {
                        self.open(indent, Role::Other);
                    }
                    let t = self.next().expect("tok");
                    if t.is_comment() {
                        self.comment(t, indent);
                    } else {
                        self.emit(t);
                    }
                }
            }
        }
    }

    fn with_clause(&mut self, indent: usize) {
        self.open(indent, Role::Clause);
        let t = self.next().expect("WITH");
        self.emit(t);
        if self.peek_is("RECURSIVE") {
            let t = self.next().expect("tok");
            self.emit(t);
        }
        // 항목: 이름 [(cols)] AS ( 질의 ) , …
        self.list_items(indent + 1, Role::Item, |w, ind| {
            // 항목 하나: `AS (` 를 만나면 서브쿼리 블록.
            loop {
                let Some(t) = w.peek() else { return };
                if t.is_punct(",") || t.is_punct(")") || t.is_punct(";") {
                    return;
                }
                if t.is("SELECT") {
                    return;
                }
                let t = w.next().expect("tok");
                if t.is_comment() {
                    w.comment(t, ind);
                } else if t.is_punct("(") {
                    w.paren_open_at(ind);
                } else {
                    w.emit(t);
                }
            }
        });
    }

    /// 목록 항목들(콤마 분리 · 항목마다 줄 · `item`이 한 항목을 소비한다). 항목이 끝난 뒤 남은 토큰은 호출자가 본다.
    fn list_items(&mut self, indent: usize, role: Role, item: impl Fn(&mut Self, usize)) {
        let single = self.list_single(indent);
        let mut first = true;
        loop {
            if single && !first {
                self.word(",");
            } else {
                self.open(indent, role);
                if !first {
                    self.part(Part::Comma);
                }
            }
            first = false;
            item(self, indent);
            if self.peek().is_some_and(|t| t.is_punct(",")) {
                self.next();
                continue;
            }
            break;
        }
    }

    /// 목록을 한 줄에 둘지(Single · Auto) — Auto는 다음 절까지의 원문 길이로 판정(대략).
    fn list_single(&self, _indent: usize) -> bool {
        match self.opts.list_style {
            ListStyle::Multi => false,
            ListStyle::Single => true,
            ListStyle::Auto => {
                let start = self.toks.get(self.i).map_or(0, |t| t.span.0);
                let mut depth = 0i32;
                let mut end = start;
                for t in &self.toks[self.i..] {
                    if t.is_punct("(") {
                        depth += 1;
                    } else if t.is_punct(")") {
                        if depth == 0 {
                            break;
                        }
                        depth -= 1;
                    } else if depth == 0
                        && (t.is_punct(";") || (t.kind == Kind::Word && is_clause_start(&t.up())))
                    {
                        break;
                    }
                    end = t.span.1;
                }
                end.saturating_sub(start) + 8 <= self.opts.line_width
            }
        }
    }

    fn select_block(&mut self, indent: usize) {
        self.open(indent, Role::Clause);
        let t = self.next().expect("SELECT");
        self.emit(t);
        // 힌트 · DISTINCT/ALL · TOP n.
        while let Some(t) = self.peek() {
            if t.kind == Kind::Hint {
                let t = self.next().expect("tok");
                self.word(&t.text);
            } else if t.is("DISTINCT") || t.is("ALL") || t.is("UNIQUE") {
                let t = self.next().expect("tok");
                self.emit(t);
            } else if t.is("TOP") {
                let t = self.next().expect("tok");
                self.emit(t);
                if let Some(n) = self.peek() {
                    if n.kind == Kind::Number || n.is_punct("(") {
                        let n = self.next().expect("tok");
                        if n.is_punct("(") {
                            self.paren_inline_after_open();
                        } else {
                            self.emit(n);
                        }
                    }
                }
            } else {
                break;
            }
        }
        // 열 목록.
        let alias_all = self.opts.column_alias_all;
        let col_as = self.opts.column_as;
        self.list_items(indent + 1, Role::Item, move |w, ind| {
            w.expr_item(ind, ItemKind::Select { alias_all, col_as });
        });
        // 절.
        loop {
            let Some(t) = self.peek_at(0) else { return };
            let up = t.up();
            if t.is_punct(")") || t.is_punct(";") {
                return;
            }
            if t.is_comment() {
                let t = self.next().expect("tok");
                self.comment(t, indent);
                continue;
            }
            match up.as_str() {
                "INTO" => {
                    self.clause_line(indent, &["INTO"]);
                    self.list_items(indent + 1, Role::Item, |w, ind| {
                        w.expr_item(ind, ItemKind::Plain)
                    });
                }
                "FROM" => {
                    self.clause_line(indent, &["FROM"]);
                    self.join_list(indent);
                }
                "WHERE" => self.cond_clause(indent, &["WHERE"]),
                "START" if self.peek_phrase(&["START", "WITH"]) => {
                    self.cond_clause(indent, &["START", "WITH"]);
                }
                "CONNECT" if self.peek_phrase(&["CONNECT", "BY"]) => {
                    self.cond_clause(indent, &["CONNECT", "BY"]);
                }
                "GROUP" if self.peek_phrase(&["GROUP", "BY"]) => {
                    self.clause_line(indent, &["GROUP", "BY"]);
                    // ROLLUP/CUBE/GROUPING SETS ( … ) = 괄호 목록.
                    if self.peek_is("ROLLUP")
                        || self.peek_is("CUBE")
                        || self.peek_phrase(&["GROUPING", "SETS"])
                    {
                        let l = self.cur_mut();
                        let _ = l;
                        while let Some(t) = self.peek() {
                            if t.is_punct("(") {
                                break;
                            }
                            let t = self.next().expect("tok");
                            self.emit(t);
                        }
                        if self.peek().is_some_and(|t| t.is_punct("(")) {
                            self.next();
                            self.word("(");
                            self.list_items(indent + 2, Role::Item, |w, ind| {
                                w.expr_item(ind, ItemKind::Plain)
                            });
                            if self.peek().is_some_and(|t| t.is_punct(")")) {
                                self.next();
                                self.open(indent + 1, Role::Close);
                                self.word(")");
                            }
                        }
                    } else {
                        self.list_items(indent + 1, Role::Item, |w, ind| {
                            w.expr_item(ind, ItemKind::Plain)
                        });
                    }
                }
                "HAVING" => self.cond_clause(indent, &["HAVING"]),
                "QUALIFY" => self.cond_clause(indent, &["QUALIFY"]),
                "WINDOW" => {
                    self.clause_line(indent, &["WINDOW"]);
                    self.list_items(indent + 1, Role::Item, |w, ind| {
                        w.expr_item(ind, ItemKind::Plain)
                    });
                }
                "ORDER" if self.peek_phrase(&["ORDER", "BY"]) => {
                    self.clause_line(indent, &["ORDER", "BY"]);
                    if self.peek_is("SIBLINGS") {
                        let t = self.next().expect("tok");
                        self.emit(t);
                    }
                    self.list_items(indent + 1, Role::Item, |w, ind| {
                        w.expr_item(ind, ItemKind::Order)
                    });
                }
                "LIMIT" | "OFFSET" | "FETCH" | "FOR" | "RETURNING" => {
                    self.open(indent, Role::Clause);
                    // 절 끝까지 인라인(다음 절 키워드·`)`·`;` 전까지).
                    let t = self.next().expect("tok");
                    self.emit(t);
                    while let Some(t) = self.peek() {
                        if t.is_punct(")") || t.is_punct(";") || t.is_punct(",") {
                            break;
                        }
                        if t.kind == Kind::Word
                            && is_clause_start(&t.up())
                            && !t.is("FIRST")
                            && !t.is("NEXT")
                            && !t.is("ROWS")
                            && !t.is("ROW")
                            && !t.is("ONLY")
                            && !t.is("UPDATE")
                        {
                            break;
                        }
                        let t = self.next().expect("tok");
                        if t.is_comment() {
                            self.comment(t, indent);
                        } else if t.is_punct("(") {
                            self.paren_open_at(indent);
                        } else {
                            self.emit(t);
                        }
                    }
                }
                "UNION" | "INTERSECT" | "MINUS" | "EXCEPT" => return,
                _ => {
                    // 절 밖의 토큰(예: 알 수 없는 방언 절) — 현재 들여쓰기 줄에 인라인.
                    if self.cur.as_ref().is_none_or(|l| l.role != Role::Other) {
                        self.open(indent, Role::Other);
                    }
                    let t = self.next().expect("tok");
                    if t.is_punct("(") {
                        self.paren_open_at(indent);
                    } else {
                        self.emit(t);
                    }
                }
            }
        }
    }

    fn clause_line(&mut self, indent: usize, words: &[&str]) {
        self.open(indent, Role::Clause);
        for _ in words {
            let t = self.next().expect("tok");
            self.emit(t);
        }
    }

    /// FROM 목록: 콤마 항목 + JOIN 줄 + ON/USING(별칭 자동 부여 문맥 = 이 안).
    fn join_list(&mut self, indent: usize) {
        let prev = self.in_from;
        self.in_from = true;
        self.join_list_inner(indent);
        self.in_from = prev;
    }

    fn join_list_inner(&mut self, indent: usize) {
        let table_as = self.opts.table_as;
        self.list_items(indent + 1, Role::Item, move |w, ind| {
            w.expr_item(ind, ItemKind::Table { table_as });
        });
        // JOIN 들.
        loop {
            let Some(t) = self.peek_at(0) else { return };
            if t.is_comment() {
                let t = self.next().expect("tok");
                self.comment(t, indent);
                continue;
            }
            let up = t.up();
            let is_join = JOIN_HEADS.contains(&up.as_str())
                || (up == "CROSS" && self.peek_phrase(&["CROSS", "APPLY"]))
                || (up == "OUTER" && self.peek_phrase(&["OUTER", "APPLY"]))
                || up == "PIVOT"
                || up == "UNPIVOT";
            if !is_join {
                return;
            }
            let jind = if self.opts.join_indent {
                indent + 1
            } else {
                indent
            };
            self.open(jind, Role::Join);
            // 조인 키워드 묶음.
            while let Some(t) = self.peek() {
                let u = t.up();
                if JOIN_HEADS.contains(&u.as_str())
                    || u == "APPLY"
                    || u == "PIVOT"
                    || u == "UNPIVOT"
                {
                    let t = self.next().expect("tok");
                    self.emit(t);
                    if u == "JOIN" || u == "APPLY" || u == "PIVOT" || u == "UNPIVOT" {
                        break;
                    }
                } else {
                    break;
                }
            }
            // 조인 대상(서브쿼리 가능) — ON/USING/다음 JOIN/절 전까지.
            self.expr_item(jind, ItemKind::Table { table_as });
            if self.peek_is("ON") {
                self.cond_clause(jind + 1, &["ON"]);
            } else if self.peek_is("USING") {
                self.open(jind + 1, Role::Clause);
                let t = self.next().expect("tok");
                self.emit(t);
                if self.peek().is_some_and(|t| t.is_punct("(")) {
                    self.next();
                    self.paren_inline_after_open();
                }
            }
        }
    }

    /// 조건 절(WHERE/ON/HAVING/…): 시드 옵션 · AND/OR마다 줄(괄호 안·BETWEEN…AND 제외).
    /// Basic 배치 = WHERE/HAVING은 절 단독 줄 + 조건 한 단계 안 · ON은 첫 조건을 ON 줄에 잇고 나머지는 ON 열(시드가 있으면
    /// `ON 1=1` 뒤 조건은 ON 열).
    fn cond_clause(&mut self, clause_indent: usize, words: &[&str]) {
        self.open(clause_indent, Role::Clause);
        for _ in words {
            let t = self.next().expect("tok");
            self.emit(t);
        }
        let is_on = words == ["ON"];
        // ★ 조건 줄 위치(사용자 09-29): 한 단계 안(기본) / 절 키워드와 같은 열(`cond_indent = same`) · ON은 늘 같은 열.
        let indent = if is_on || !self.opts.cond_indent {
            clause_indent
        } else {
            clause_indent + 1
        };
        // 원문이 이미 `1=1`로 시작하는가.
        let has_seed = self
            .peek_at(0)
            .is_some_and(|a| a.kind == Kind::Number && a.text == "1")
            && self
                .peek_at(1)
                .is_some_and(|b| b.kind == Kind::Op && b.text == "=")
            && self
                .peek_at(2)
                .is_some_and(|c| c.kind == Kind::Number && c.text == "1");
        let mut cond_started = false;
        if has_seed {
            // 시드는 절 줄에 붙인다(`1=1` 한 단어 · 공백 없이).
            for _ in 0..3 {
                self.next();
            }
            self.seed_word();
            cond_started = true;
        } else if self.opts.where_seed {
            self.seed_word();
            cond_started = true;
        }
        // 조건들.
        let mut between = 0u32;
        let mut first = true;
        loop {
            let Some(t) = self.peek() else { return };
            if t.is_punct(")") || t.is_punct(";") || t.is_punct(",") {
                return;
            }
            if t.is_comment() {
                let t = self.next().expect("tok");
                self.comment(t, indent);
                continue;
            }
            let up = t.up();
            if t.kind == Kind::Word
                && (is_clause_start(&up)
                    || JOIN_HEADS.contains(&up.as_str())
                    || up == "PIVOT"
                    || up == "UNPIVOT")
                && !(up == "SET" || up == "WHEN")
            {
                return;
            }
            if up == "WHEN" && words.contains(&"ON") && !self.opts.where_seed {
                // MERGE ON 뒤 WHEN MATCHED — 호출자 몫.
                return;
            }
            if up == "WHEN" {
                return;
            }
            if (up == "AND" || up == "OR") && between == 0 {
                let t = self.next().expect("tok");
                match self.opts.logical_newline {
                    LogicalNewline::Before => {
                        self.open(indent, Role::Cond);
                        self.emit(t);
                        self.logical_gap();
                    }
                    LogicalNewline::After => {
                        self.emit(t);
                        self.open(indent, Role::Cond);
                    }
                }
                first = false;
                cond_started = true;
                self.cond_expr(indent, &mut between);
                continue;
            }
            // 첫 조건(시드 뒤면 AND로 시작).
            if first {
                if cond_started {
                    match self.opts.logical_newline {
                        LogicalNewline::Before => {
                            self.open(indent, Role::Cond);
                            let and = self.kw("AND");
                            self.word(&and);
                            self.logical_gap();
                        }
                        LogicalNewline::After => {
                            let and = self.kw("AND");
                            self.word(&and);
                            self.open(indent, Role::Cond);
                        }
                    }
                } else if !is_on {
                    // 시드 없음: WHERE/HAVING은 절 단독 줄 + 조건 다음 줄 · ON은 같은 줄에 잇는다.
                    self.open(indent, Role::Cond);
                }
                first = false;
                cond_started = true;
                self.cond_expr(indent, &mut between);
                continue;
            }
            // AND/OR 없이 이어지는 토큰(드물다) — 현재 줄에.
            let t = self.next().expect("tok");
            self.emit_expr_token(t, indent, &mut between, ExprCtx::Cond);
        }
    }

    /// 시드 `1=1`을 절 줄에(`WHERE 1=1` · `ON 1=1`): 사이는 `seed_gap`(공백 하나 / 탭 · 사용자 09-29).
    fn seed_word(&mut self) {
        if self.opts.seed_gap == crate::Gap::Tab {
            let line = self.cur_mut();
            match line.parts.last_mut() {
                Some(Part::Text(t)) if !t.is_empty() => {
                    t.push('\t');
                    t.push_str("1=1");
                }
                _ => line.parts.push(Part::Text("1=1".into())),
            }
        } else {
            self.word("1=1");
        }
    }

    /// 조건 하나(다음 최상위 AND/OR · 절 · `)` 전까지) — 비교 연산자를 조각으로.
    fn cond_expr(&mut self, indent: usize, between: &mut u32) {
        let mut cmp_done = false;
        loop {
            let Some(t) = self.peek() else { return };
            if t.is_punct(")") || t.is_punct(";") || t.is_punct(",") {
                return;
            }
            let up = t.up();
            if t.kind == Kind::Word {
                if (up == "AND" || up == "OR") && *between == 0 {
                    return;
                }
                if up == "AND" && *between > 0 {
                    *between -= 1;
                }
                if is_clause_start(&up)
                    || JOIN_HEADS.contains(&up.as_str())
                    || up == "WHEN"
                    || up == "PIVOT"
                    || up == "UNPIVOT"
                {
                    return;
                }
            }
            // ★ 단어 비교 연산자(IN · IS · LIKE · NOT IN · NOT LIKE · IS NOT)도 연산자 조각(간격·정렬 규칙 공유 · 사용자 09-29).
            if t.kind == Kind::Word && !cmp_done && *between == 0 {
                let (words, n): (&str, usize) = match up.as_str() {
                    "NOT" if self.peek_at(1).is_some_and(|x| x.is("IN")) => ("NOT IN", 2),
                    "NOT" if self.peek_at(1).is_some_and(|x| x.is("LIKE")) => ("NOT LIKE", 2),
                    "IS" if self.peek_at(1).is_some_and(|x| x.is("NOT")) => ("IS NOT", 2),
                    "IN" => ("IN", 1),
                    "IS" => ("IS", 1),
                    "LIKE" => ("LIKE", 1),
                    _ => ("", 0),
                };
                if n > 0 {
                    for _ in 0..n {
                        self.next();
                    }
                    cmp_done = true;
                    self.part(Part::CmpOp(self.opts.keyword_case.apply(words)));
                    continue;
                }
            }
            let t = self.next().expect("tok");
            if t.kind == Kind::Op && !cmp_done && is_cmp(&t.text) {
                cmp_done = true;
                self.part(Part::CmpOp(t.text.clone()));
                continue;
            }
            self.emit_expr_token(t, indent, between, ExprCtx::Cond);
        }
    }

    /// 표현식 토큰 하나(괄호·CASE·BETWEEN·주석 처리 공용).
    fn emit_expr_token(&mut self, t: &'a Token, indent: usize, between: &mut u32, ctx: ExprCtx) {
        if t.is_comment() {
            self.comment(t, indent);
            return;
        }
        if t.is("BETWEEN") {
            *between += 1;
            self.emit(t);
            return;
        }
        if t.is_punct("(") {
            // ★ 괄호 AND/OR 그룹(스킬 §3 · T-255): 조건 문맥 + 옵션 + 안에 최상위 AND/OR → `(1=1`/`(1=0` 시드 블록.
            if ctx == ExprCtx::Cond && self.opts.paren_seed {
                if let Some(or_group) = self.paren_is_cond_group() {
                    self.paren_group(indent, or_group);
                    return;
                }
            }
            self.paren_open_at(indent);
            return;
        }
        if t.is("CASE") {
            self.case_expr(indent);
            return;
        }
        self.emit(t);
    }

    /// `(`를 이미 소비한 상태에서 짝 `)`까지가 **조건 그룹**인가 — 서브쿼리가 아니고 최상위(괄호·CASE 밖)에 AND/OR가 있을 때
    /// `Some(첫 연산자가 OR인가)`. BETWEEN … AND의 AND는 세지 않는다.
    fn paren_is_cond_group(&self) -> Option<bool> {
        if self.peek_is("SELECT") || self.peek_is("WITH") {
            return None;
        }
        let mut depth = 0i32;
        let mut case_depth = 0i32;
        let mut between = 0u32;
        let mut j = self.i;
        while let Some(t) = self.toks.get(j) {
            j += 1;
            if t.is_punct("(") {
                depth += 1;
                continue;
            }
            if t.is_punct(")") {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                continue;
            }
            if depth != 0 || t.kind != Kind::Word {
                continue;
            }
            let up = t.up();
            if up == "CASE" {
                case_depth += 1;
            } else if up == "END" && case_depth > 0 {
                case_depth -= 1;
            } else if case_depth == 0 {
                if up == "BETWEEN" {
                    between += 1;
                } else if up == "AND" {
                    if between > 0 {
                        between -= 1;
                    } else {
                        return Some(false);
                    }
                } else if up == "OR" {
                    return Some(true);
                }
            }
        }
        None
    }

    /// 괄호 AND/OR 그룹: 여는 줄에 `(1=1`(AND) / `(1=0`(OR) — 원문에 `1=1`/`1=0`이 있으면 그것 · 하위 조건은 한 단계 더 · `)`도 그 열.
    fn paren_group(&mut self, indent: usize, or_group: bool) {
        let src_seed = self
            .peek_at(0)
            .is_some_and(|a| a.kind == Kind::Number && a.text == "1")
            && self
                .peek_at(1)
                .is_some_and(|b| b.kind == Kind::Op && b.text == "=")
            && self
                .peek_at(2)
                .is_some_and(|c| c.kind == Kind::Number && (c.text == "1" || c.text == "0"));
        let seed = if src_seed {
            let c = self.peek_at(2).map(|c| c.text.clone()).unwrap_or_default();
            for _ in 0..3 {
                self.next();
            }
            format!("1={c}")
        } else if or_group {
            "1=0".to_string()
        } else {
            "1=1".to_string()
        };
        self.word(&format!("({seed}"));
        let ind = indent + 1;
        let mut between = 0u32;
        let mut first = true;
        loop {
            let Some(t) = self.peek() else { return };
            if t.is_punct(")") {
                self.next();
                self.open(ind, Role::Close);
                self.word(")");
                return;
            }
            if t.is_punct(";") {
                return;
            }
            if t.is_comment() {
                let t = self.next().expect("tok");
                self.comment(t, ind);
                continue;
            }
            let up = t.up();
            if t.kind == Kind::Word && (up == "AND" || up == "OR") && between == 0 {
                let t = self.next().expect("tok");
                match self.opts.logical_newline {
                    LogicalNewline::Before => {
                        self.open(ind, Role::Cond);
                        self.emit(t);
                        self.logical_gap();
                    }
                    LogicalNewline::After => {
                        self.emit(t);
                        self.open(ind, Role::Cond);
                    }
                }
                first = false;
                self.cond_expr(ind, &mut between);
                continue;
            }
            if first {
                // 시드 뒤 첫 조건 = 그룹 연산자로 시작.
                let op = self.kw(if or_group { "OR" } else { "AND" });
                match self.opts.logical_newline {
                    LogicalNewline::Before => {
                        self.open(ind, Role::Cond);
                        self.word(&op);
                        self.logical_gap();
                    }
                    LogicalNewline::After => {
                        self.word(&op);
                        self.open(ind, Role::Cond);
                    }
                }
                first = false;
                self.cond_expr(ind, &mut between);
                continue;
            }
            let t = self.next().expect("tok");
            self.emit_expr_token(t, ind, &mut between, ExprCtx::Cond);
        }
    }

    /// 자동 별칭(스킬 2-1): `A`…`Z`, `AA`… 중 이 문장에 없는 첫 이름.
    fn next_auto_alias(&mut self) -> String {
        loop {
            let n = self.alias_next;
            self.alias_next += 1;
            let name = alias_name(n);
            if self.stmt_words.insert(name.clone()) {
                return name;
            }
        }
    }

    /// 테이블 설명 주석(스킬 2-2): 물리 테이블(`S.T`/`T` · 서브쿼리 제외)에 호스트가 준 설명이 있으면 줄 끝 `--\t설명`.
    fn table_comment(&mut self, toks: &[&Token]) {
        if self.opts.table_comments.is_empty() {
            return;
        }
        let Some(first) = toks.first() else { return };
        if first.kind != Kind::Word && first.kind != Kind::Quoted {
            return;
        }
        let mut name = first
            .text
            .trim_matches(|c| c == '"' || c == '[' || c == ']' || c == '`')
            .to_ascii_uppercase();
        let mut k = 1;
        while k + 1 < toks.len()
            && toks[k].is_punct(".")
            && (toks[k + 1].kind == Kind::Word || toks[k + 1].kind == Kind::Quoted)
        {
            name.push('.');
            name.push_str(
                &toks[k + 1]
                    .text
                    .trim_matches(|c| c == '"' || c == '[' || c == ']' || c == '`')
                    .to_ascii_uppercase(),
            );
            k += 2;
        }
        let last = name.rsplit('.').next().unwrap_or(&name).to_string();
        let found = self
            .opts
            .table_comments
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(&name))
            .or_else(|| {
                self.opts
                    .table_comments
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(&last))
            })
            .map(|(_, c)| c.clone());
        if let Some(c) = found {
            let c = c.trim();
            if !c.is_empty() {
                self.part(Part::Comment(format!("--\t{c}")));
            }
        }
    }

    /// `(`를 이미 소비한 상태: 서브쿼리면 블록, 아니면 인라인 괄호.
    fn paren_open_at(&mut self, indent: usize) {
        self.word("(");
        if self.peek_is("SELECT") || self.peek_is("WITH") {
            // 서브쿼리 블록.
            self.flush();
            self.query(indent + 1);
            self.flush();
            if self.peek().is_some_and(|t| t.is_punct(")")) {
                self.next();
                self.open(indent, Role::Close);
                self.word(")");
            }
        } else {
            self.paren_inline_after_open_at(indent);
        }
    }

    fn paren_inline_after_open(&mut self) {
        let ind = self.cur.as_ref().map_or(0, |l| l.indent);
        self.word("(");
        self.paren_inline_after_open_at(ind);
    }

    /// 인라인 괄호 내용(중첩 · 안의 서브쿼리는 블록으로 · 안의 CASE는 한 줄) — 닫는 `)`까지 소비.
    fn paren_inline_after_open_at(&mut self, indent: usize) {
        loop {
            let Some(t) = self.peek() else { return };
            if t.is_punct(")") {
                self.next();
                self.word(")");
                return;
            }
            let t = self.next().expect("tok");
            if t.is_comment() {
                self.comment(t, indent);
            } else if t.is_punct("(") {
                self.paren_open_at(indent);
            } else if t.is_punct(",") {
                self.word(",");
            } else {
                self.emit(t);
            }
        }
    }

    /// 괄호 질의 `( … )` at query level.
    fn paren(&mut self, indent: usize) {
        if self.peek().is_some_and(|t| t.is_punct("(")) {
            self.next();
            self.paren_open_at(indent);
        }
    }

    /// CASE … END(`CASE`는 아직 소비 전이 아니라 **소비된 뒤** 호출된다 — 여기서 CASE 글을 쓴다).
    fn case_expr(&mut self, indent: usize) {
        // 한 줄에 들어가는가: 원문 토큰으로 END까지의 길이를 잰다.
        let start_i = self.i - 1;
        let end_i = self.find_case_end(start_i);
        let raw_len = self.toks[start_i].span.0..self.toks[end_i.min(self.toks.len() - 1)].span.1;
        let inline = raw_len.len() <= self.opts.case_inline_max;
        let kw = self.kw("CASE");
        self.word(&kw);
        if inline {
            // WHEN/THEN/ELSE/END 인라인.
            let mut depth = 0i32;
            loop {
                if self.peek().is_none() {
                    return;
                }
                let t = self.next().expect("tok");
                if t.is_comment() {
                    self.comment(t, indent);
                    continue;
                }
                if t.is("CASE") {
                    depth += 1;
                } else if t.is("END") {
                    if depth == 0 {
                        self.emit(t);
                        return;
                    }
                    depth -= 1;
                }
                if t.is_punct("(") {
                    self.paren_open_at(indent);
                } else if t.is_punct(",") {
                    self.word(",");
                } else {
                    self.emit(t);
                }
            }
        }
        // 다중 줄: CASE [식] / WHEN … THEN … / ELSE … / END.
        let inner = indent + 1;
        let mut depth = 0i32;
        loop {
            let Some(t) = self.peek() else { return };
            let up = t.up();
            if depth == 0 && (up == "WHEN" || up == "ELSE") {
                self.open(inner, Role::Case);
                let t = self.next().expect("tok");
                self.emit(t);
                continue;
            }
            if depth == 0 && up == "END" {
                self.next();
                self.open(indent, Role::Case);
                let e = self.kw("END");
                self.word(&e);
                return;
            }
            let t = self.next().expect("tok");
            if t.is_comment() {
                self.comment(t, inner);
                continue;
            }
            if t.is("CASE") {
                // 중첩 CASE = 한 줄로.
                depth += 1;
                self.emit(t);
                continue;
            }
            if t.is("END") {
                depth -= 1;
                self.emit(t);
                continue;
            }
            if t.is_punct("(") {
                self.paren_open_at(inner);
            } else if t.is_punct(",") {
                self.word(",");
            } else {
                self.emit(t);
            }
        }
    }

    fn find_case_end(&self, start: usize) -> usize {
        let mut depth = 0i32;
        let mut k = start + 1;
        while k < self.toks.len() {
            let t = &self.toks[k];
            if t.is("CASE") {
                depth += 1;
            } else if t.is("END") {
                if depth == 0 {
                    return k;
                }
                depth -= 1;
            }
            k += 1;
        }
        self.toks.len() - 1
    }

    /// 목록 항목 하나(SELECT 열 · FROM 테이블 · GROUP/ORDER 항목 · 대입) — 콤마·절·`)`·`;`·JOIN 전까지. 별칭·방향을 조각으로.
    fn expr_item(&mut self, indent: usize, kind: ItemKind) {
        let mut between = 0u32;
        let item_start = self.i;
        let mut cmp_done = false;
        while let Some(t) = self.peek() {
            if t.is_punct(",") || t.is_punct(")") || t.is_punct(";") {
                break;
            }
            let up = t.up();
            if t.kind == Kind::Word {
                if is_clause_start(&up) {
                    break;
                }
                if matches!(kind, ItemKind::Table { .. })
                    && (JOIN_HEADS.contains(&up.as_str())
                        || up == "ON"
                        || up == "USING"
                        || up == "PIVOT"
                        || up == "UNPIVOT")
                {
                    break;
                }
                if up == "WHEN"
                    || up == "UNION"
                    || up == "INTERSECT"
                    || up == "MINUS"
                    || up == "EXCEPT"
                {
                    break;
                }
            }
            // ORDER BY 방향.
            if matches!(kind, ItemKind::Order) && (up == "ASC" || up == "DESC" || up == "NULLS") {
                let mut dir: Vec<String> = Vec::new();
                while let Some(t) = self.peek() {
                    let u = t.up();
                    if u == "ASC" || u == "DESC" || u == "NULLS" || u == "FIRST" || u == "LAST" {
                        let t = self.next().expect("tok");
                        dir.push(self.cased(t));
                    } else {
                        break;
                    }
                }
                self.part(Part::OrderDir(dir.join(" ")));
                continue;
            }
            // 별칭 `AS x`.
            if up == "AS" {
                self.next();
                if let Some(a) = self.peek() {
                    if a.kind == Kind::Word || a.kind == Kind::Quoted {
                        let a = self.next().expect("tok");
                        let name = if a.kind == Kind::Word {
                            self.opts.identifier_case.apply(&a.text)
                        } else {
                            a.text.clone()
                        };
                        let remove = match kind {
                            ItemKind::Select { col_as, .. } => col_as == AliasAs::Remove,
                            ItemKind::Table { table_as } => table_as == AliasAs::Remove,
                            _ => false,
                        };
                        if remove {
                            self.part(Part::Alias(name));
                        } else {
                            self.part(Part::As);
                            self.part(Part::Alias(name));
                        }
                        continue;
                    }
                }
                let as_kw = self.kw("AS");
                self.word(&as_kw);
                continue;
            }
            let t = self.next().expect("tok");
            if matches!(kind, ItemKind::Set) && t.kind == Kind::Op && !cmp_done && t.text == "=" {
                cmp_done = true;
                self.part(Part::CmpOp("=".into()));
                continue;
            }
            self.emit_expr_token(t, indent, &mut between, ExprCtx::Item);
        }
        // 암묵 별칭 감지 · 명시적 AS 추가 · 모든 열 별칭.
        let end = self.i;
        let toks: Vec<&Token> = self.toks[item_start..end]
            .iter()
            .filter(|t| !t.is_comment())
            .collect();
        let has_as_part = self.cur.as_ref().is_some_and(|l| {
            l.parts
                .iter()
                .any(|p| matches!(p, Part::As | Part::Alias(_)))
        });
        if toks.is_empty() {
            return;
        }
        if !has_as_part {
            match kind {
                ItemKind::Select { alias_all, col_as } => {
                    if let Some(alias) = implicit_alias(&toks) {
                        if col_as == AliasAs::Add {
                            // 마지막 Text 조각에서 별칭 단어를 떼어 AS + Alias로.
                            if self.strip_last_word(&alias) {
                                self.part(Part::As);
                                self.part(Part::Alias(self.opts.identifier_case.apply(&alias)));
                            }
                        }
                    } else if alias_all {
                        if let Some(col) = bare_column(&toks) {
                            self.part(Part::As);
                            self.part(Part::Alias(self.opts.identifier_case.apply(&col)));
                        }
                    }
                }
                ItemKind::Table { table_as } => {
                    if let Some(alias) = implicit_alias(&toks) {
                        if table_as == AliasAs::Add && self.strip_last_word(&alias) {
                            self.part(Part::As);
                            self.part(Part::Alias(self.opts.identifier_case.apply(&alias)));
                        }
                    } else if self.opts.auto_alias && self.in_from {
                        // ★ 별칭 자동 부여(스킬 2-1 · T-255): FROM/JOIN의 별칭 없는 테이블·인라인뷰에 `A`, `B`, ….
                        let name = self.next_auto_alias();
                        if table_as == AliasAs::Add {
                            self.part(Part::As);
                        }
                        self.part(Part::Alias(name));
                    }
                }
                _ => {}
            }
        }
        // ★ 테이블 설명 주석(스킬 2-2 · T-255) — 별칭 뒤 줄 끝. 원문 항목에 주석이 이미 있었으면(지난 포맷의 설명 포함) 안 붙인다(멱등).
        let had_comment = self.toks[item_start..end].iter().any(Token::is_comment);
        if matches!(kind, ItemKind::Table { .. }) && !had_comment {
            self.table_comment(&toks);
        }
    }

    /// 현재 줄 마지막 Text 조각의 끝 단어가 `w`면 떼어 낸다.
    fn strip_last_word(&mut self, w: &str) -> bool {
        let line = self.cur_mut();
        if let Some(Part::Text(t)) = line.parts.last_mut() {
            let trimmed = t.trim_end();
            if trimmed.len() >= w.len()
                && trimmed[trimmed.len() - w.len()..].eq_ignore_ascii_case(w)
            {
                let cut = trimmed.len() - w.len();
                let head = trimmed[..cut].trim_end().to_string();
                if head.is_empty() {
                    return false;
                }
                *t = head;
                return true;
            }
        }
        false
    }

    // ── DML

    fn insert(&mut self, indent: usize) {
        self.open(indent, Role::Clause);
        let t = self.next().expect("INSERT");
        self.emit(t);
        // INSERT [INTO] table [(cols)] · INSERT ALL … 은 인라인 통과.
        while let Some(t) = self.peek() {
            if t.is_punct("(")
                || t.is("VALUES")
                || t.is("SELECT")
                || t.is("WITH")
                || t.is_punct(";")
            {
                break;
            }
            let t = self.next().expect("tok");
            if t.is_comment() {
                self.comment(t, indent);
            } else {
                self.emit(t);
            }
        }
        if self.peek().is_some_and(|t| t.is_punct("(")) {
            self.next();
            self.word("(");
            self.list_items(indent + 1, Role::Item, |w, ind| {
                w.expr_item(ind, ItemKind::Plain)
            });
            if self.peek().is_some_and(|t| t.is_punct(")")) {
                self.next();
                self.open(indent, Role::Close);
                self.word(")");
            }
        }
        if self.peek_is("VALUES") {
            self.clause_line(indent, &["VALUES"]);
            // 행마다 `( … )`.
            self.list_items(indent + 1, Role::Item, |w, ind| {
                if w.peek().is_some_and(|t| t.is_punct("(")) {
                    w.next();
                    w.word("(");
                    w.paren_inline_after_open_at(ind);
                } else {
                    w.expr_item(ind, ItemKind::Plain);
                }
            });
        } else if self.peek_is("SELECT") || self.peek_is("WITH") {
            self.query(indent);
        }
        // RETURNING 등은 select_block 규칙과 같이 tail에서.
    }

    fn update(&mut self, indent: usize) {
        self.open(indent, Role::Clause);
        let t = self.next().expect("UPDATE");
        self.emit(t);
        // 대상 · 별칭 · SET 까지 한 줄.
        while let Some(t) = self.peek() {
            if t.is("SET") || t.is_punct(";") {
                break;
            }
            let t = self.next().expect("tok");
            if t.is_comment() {
                self.comment(t, indent);
            } else if t.is_punct("(") {
                self.paren_open_at(indent);
            } else {
                self.emit(t);
            }
        }
        if self.peek_is("SET") {
            let t = self.next().expect("SET");
            self.emit(t);
            self.list_items(indent + 1, Role::Item, |w, ind| {
                w.expr_item(ind, ItemKind::Set)
            });
        }
        self.dml_tail(indent);
    }

    fn delete(&mut self, indent: usize) {
        self.open(indent, Role::Clause);
        let t = self.next().expect("DELETE");
        self.emit(t);
        while let Some(t) = self.peek() {
            if t.is("WHERE") || t.is("USING") || t.is_punct(";") {
                break;
            }
            let t = self.next().expect("tok");
            if t.is_comment() {
                self.comment(t, indent);
            } else if t.is_punct("(") {
                self.paren_open_at(indent);
            } else {
                self.emit(t);
            }
        }
        self.dml_tail(indent);
    }

    /// UPDATE/DELETE 뒤 절(FROM · WHERE · RETURNING …).
    fn dml_tail(&mut self, indent: usize) {
        loop {
            let Some(t) = self.peek_at(0) else { return };
            if t.is_punct(";") || t.is_punct(")") {
                return;
            }
            if t.is_comment() {
                let t = self.next().expect("tok");
                self.comment(t, indent);
                continue;
            }
            match t.up().as_str() {
                "WHERE" => self.cond_clause(indent, &["WHERE"]),
                "FROM" | "USING" => {
                    let w = t.up();
                    self.clause_line(indent, &[w.as_str()]);
                    self.join_list(indent);
                }
                "RETURNING" | "OUTPUT" => {
                    let w = t.up();
                    self.clause_line(indent, &[w.as_str()]);
                    self.list_items(indent + 1, Role::Item, |w, ind| {
                        w.expr_item(ind, ItemKind::Plain)
                    });
                }
                _ => {
                    if self.cur.as_ref().is_none_or(|l| l.role != Role::Other) {
                        self.open(indent, Role::Other);
                    }
                    let t = self.next().expect("tok");
                    if t.is_punct("(") {
                        self.paren_open_at(indent);
                    } else {
                        self.emit(t);
                    }
                }
            }
        }
    }

    fn merge(&mut self, indent: usize) {
        self.open(indent, Role::Clause);
        let t = self.next().expect("MERGE");
        self.emit(t);
        loop {
            let Some(t) = self.peek_at(0) else { return };
            if t.is_punct(";") || t.is_punct(")") {
                return;
            }
            if t.is_comment() {
                let t = self.next().expect("tok");
                self.comment(t, indent);
                continue;
            }
            match t.up().as_str() {
                "USING" => {
                    self.clause_line(indent, &["USING"]);
                    self.expr_item(
                        indent,
                        ItemKind::Table {
                            table_as: self.opts.table_as,
                        },
                    );
                }
                "ON" => {
                    // ON (cond) — 괄호가 있으면 괄호 안을 조건 절처럼.
                    self.open(indent + 1, Role::Clause);
                    let t = self.next().expect("ON");
                    self.emit(t);
                    if self.peek().is_some_and(|t| t.is_punct("(")) {
                        self.next();
                        self.word("(");
                        // 조건들.
                        self.cond_body(indent + 1);
                        if self.peek().is_some_and(|t| t.is_punct(")")) {
                            self.next();
                            self.open(indent + 1, Role::Close);
                            self.word(")");
                        }
                    } else {
                        self.cond_body(indent + 1);
                    }
                }
                "WHEN" => {
                    self.open(indent, Role::Clause);
                    while self.peek().is_some() {
                        let t = self.next().expect("tok");
                        self.emit(t);
                        if t.is("THEN") {
                            break;
                        }
                    }
                    // THEN 뒤: UPDATE SET … / DELETE [WHERE] / INSERT (…) VALUES (…).
                    if self.peek_is("UPDATE") {
                        self.open(indent + 1, Role::Clause);
                        let t = self.next().expect("tok");
                        self.emit(t);
                        if self.peek_is("SET") {
                            let t = self.next().expect("tok");
                            self.emit(t);
                            self.list_items(indent + 2, Role::Item, |w, ind| {
                                w.expr_item(ind, ItemKind::Set)
                            });
                        }
                        if self.peek_is("WHERE") {
                            self.cond_clause(indent + 1, &["WHERE"]);
                        }
                        if self.peek_is("DELETE") {
                            self.open(indent + 1, Role::Clause);
                            let t = self.next().expect("tok");
                            self.emit(t);
                            if self.peek_is("WHERE") {
                                self.cond_clause(indent + 1, &["WHERE"]);
                            }
                        }
                    } else if self.peek_is("DELETE") {
                        self.open(indent + 1, Role::Clause);
                        let t = self.next().expect("tok");
                        self.emit(t);
                        if self.peek_is("WHERE") {
                            self.cond_clause(indent + 1, &["WHERE"]);
                        }
                    } else if self.peek_is("INSERT") {
                        self.open(indent + 1, Role::Clause);
                        let t = self.next().expect("tok");
                        self.emit(t);
                        if self.peek().is_some_and(|t| t.is_punct("(")) {
                            self.next();
                            self.word("(");
                            self.list_items(indent + 2, Role::Item, |w, ind| {
                                w.expr_item(ind, ItemKind::Plain)
                            });
                            if self.peek().is_some_and(|t| t.is_punct(")")) {
                                self.next();
                                self.open(indent + 1, Role::Close);
                                self.word(")");
                            }
                        }
                        if self.peek_is("VALUES") {
                            self.open(indent + 1, Role::Clause);
                            let t = self.next().expect("tok");
                            self.emit(t);
                            if self.peek().is_some_and(|t| t.is_punct("(")) {
                                self.next();
                                self.word("(");
                                self.list_items(indent + 2, Role::Item, |w, ind| {
                                    w.expr_item(ind, ItemKind::Plain)
                                });
                                if self.peek().is_some_and(|t| t.is_punct(")")) {
                                    self.next();
                                    self.open(indent + 1, Role::Close);
                                    self.word(")");
                                }
                            }
                        }
                        if self.peek_is("WHERE") {
                            self.cond_clause(indent + 1, &["WHERE"]);
                        }
                    }
                }
                _ => {
                    // MERGE INTO 대상 별칭 등 — 첫 줄에 인라인.
                    let t = self.next().expect("tok");
                    if t.is_punct("(") {
                        self.paren_open_at(indent);
                    } else {
                        self.emit(t);
                    }
                }
            }
        }
    }

    /// 조건 본문(절 키워드 없이 · MERGE ON 괄호 안 등): 시드 옵션 + AND/OR 줄.
    fn cond_body(&mut self, indent: usize) {
        let has_seed = self
            .peek_at(0)
            .is_some_and(|a| a.kind == Kind::Number && a.text == "1")
            && self
                .peek_at(1)
                .is_some_and(|b| b.kind == Kind::Op && b.text == "=")
            && self
                .peek_at(2)
                .is_some_and(|c| c.kind == Kind::Number && c.text == "1");
        let mut started = false;
        if has_seed {
            for _ in 0..3 {
                self.next();
            }
            self.seed_word();
            started = true;
        } else if self.opts.where_seed {
            self.seed_word();
            started = true;
        }
        let mut between = 0u32;
        let mut first = true;
        loop {
            let Some(t) = self.peek() else { return };
            if t.is_punct(")") || t.is_punct(";") {
                return;
            }
            if t.is_comment() {
                let t = self.next().expect("tok");
                self.comment(t, indent);
                continue;
            }
            let up = t.up();
            if up == "WHEN" || (t.kind == Kind::Word && is_clause_start(&up)) {
                return;
            }
            if (up == "AND" || up == "OR") && between == 0 {
                let t = self.next().expect("tok");
                self.open(indent, Role::Cond);
                self.emit(t);
                self.logical_gap();
                first = false;
                self.cond_expr(indent, &mut between);
                continue;
            }
            if first {
                if started {
                    self.open(indent, Role::Cond);
                    let and = self.kw("AND");
                    self.word(&and);
                    self.logical_gap();
                } else {
                    self.open(indent, Role::Cond);
                }
                first = false;
                self.cond_expr(indent, &mut between);
                continue;
            }
            let t = self.next().expect("tok");
            self.emit_expr_token(t, indent, &mut between, ExprCtx::Cond);
        }
    }
}

#[derive(Clone, Copy)]
enum ItemKind {
    Plain,
    Select { alias_all: bool, col_as: AliasAs },
    Table { table_as: AliasAs },
    Order,
    Set,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExprCtx {
    Cond,
    Item,
}

/// 자동 별칭 이름: 0 → `A` … 25 → `Z` · 26 → `AA` · 27 → `AB` ….
fn alias_name(n: usize) -> String {
    let mut n = n;
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    s
}

/// ★ 방언 치환(스킬 §22 · T-255): 포맷은 그대로, **이름만** — `ISNULL/NVL/COALESCE` · `SUBSTRING/SUBSTR` · `LEN/LENGTH/CHAR_LENGTH` ·
/// `GETDATE()/SYSDATE/CURRENT_TIMESTAMP` · `EXCEPT/MINUS`. 인자 순서가 다른 것(`CHARINDEX/INSTR`)·구조가 다른 것(`TOP`·`#TEMP`)은 안 건드린다.
pub fn substitute_dialect(toks: Vec<Token>, target: DialectTarget) -> Vec<Token> {
    if target == DialectTarget::None {
        return toks;
    }
    let fn_name = |up: &str| -> Option<&'static str> {
        let group: &[&str] = match up {
            "ISNULL" | "NVL" | "COALESCE" => &["ISNULL", "NVL", "COALESCE"],
            "SUBSTRING" | "SUBSTR" => &["SUBSTRING", "SUBSTR", "SUBSTRING"],
            "LEN" | "LENGTH" | "CHAR_LENGTH" => &["LEN", "LENGTH", "CHAR_LENGTH"],
            _ => return None,
        };
        Some(match target {
            DialectTarget::Tsql => group[0],
            DialectTarget::Oracle => group[1],
            _ => group[2],
        })
    };
    let now_name = match target {
        DialectTarget::Tsql => "GETDATE",
        DialectTarget::Oracle => "SYSDATE",
        _ => "CURRENT_TIMESTAMP",
    };
    let setop = match target {
        DialectTarget::Oracle => "MINUS",
        _ => "EXCEPT",
    };
    let mk = |from: &Token, kind: Kind, text: &str, nl: u32| Token {
        kind,
        text: text.to_string(),
        nl_before: nl,
        span: from.span,
    };
    let mut out: Vec<Token> = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        if t.kind == Kind::Word {
            let up = t.up();
            let next_paren = toks.get(i + 1).is_some_and(|n| n.is_punct("("));
            if next_paren {
                if let Some(new) = fn_name(&up) {
                    out.push(mk(t, Kind::Word, new, t.nl_before));
                    i += 1;
                    continue;
                }
                // `GETDATE()` → 이름만 남거나(`SYSDATE`·`CURRENT_TIMESTAMP`) 그대로.
                if up == "GETDATE" && toks.get(i + 2).is_some_and(|c| c.is_punct(")")) {
                    if now_name == "GETDATE" {
                        out.push(t.clone());
                        i += 1;
                    } else {
                        out.push(mk(t, Kind::Word, now_name, t.nl_before));
                        i += 3;
                    }
                    continue;
                }
            } else if up == "SYSDATE" || up == "CURRENT_TIMESTAMP" {
                if now_name == "GETDATE" {
                    out.push(mk(t, Kind::Word, "GETDATE", t.nl_before));
                    out.push(mk(t, Kind::Punct, "(", 0));
                    out.push(mk(t, Kind::Punct, ")", 0));
                } else {
                    out.push(mk(t, Kind::Word, now_name, t.nl_before));
                }
                i += 1;
                continue;
            } else if up == "EXCEPT" || up == "MINUS" {
                out.push(mk(t, Kind::Word, setop, t.nl_before));
                i += 1;
                continue;
            }
        }
        out.push(t.clone());
        i += 1;
    }
    out
}

fn is_cmp(op: &str) -> bool {
    matches!(op, "=" | "<>" | "!=" | "<" | ">" | "<=" | ">=")
}

/// 절을 시작하는 키워드(항목·조건이 끝나는 경계).
fn is_clause_start(up: &str) -> bool {
    matches!(
        up,
        "SELECT"
            | "FROM"
            | "WHERE"
            | "GROUP"
            | "HAVING"
            | "ORDER"
            | "LIMIT"
            | "OFFSET"
            | "FETCH"
            | "UNION"
            | "INTERSECT"
            | "MINUS"
            | "EXCEPT"
            | "INTO"
            | "VALUES"
            | "SET"
            | "RETURNING"
            | "START"
            | "CONNECT"
            | "QUALIFY"
            | "WINDOW"
            | "FOR"
            | "USING"
            | "OUTPUT"
            | "WITH"
    )
}

/// 항목 토큰의 끝이 암묵 별칭인가(`expr name` · 이름은 키워드가 아님 · 앞 토큰이 `.`/연산자/`(`/`,`가 아님).
fn implicit_alias(toks: &[&Token]) -> Option<String> {
    if toks.len() < 2 {
        return None;
    }
    let last = toks[toks.len() - 1];
    let prev = toks[toks.len() - 2];
    if last.kind != Kind::Word && last.kind != Kind::Quoted {
        return None;
    }
    if last.kind == Kind::Word && (is_keyword(&last.text) || is_unit_word(&last.text)) {
        return None;
    }
    if prev.is("AS")
        || prev.is_punct(".")
        || prev.is_punct("(")
        || prev.is_punct(",")
        || prev.kind == Kind::Op
    {
        return None;
    }
    // `A.X Y`: prev = X(Word) · `COUNT(*) CNT`: prev = `)` · `'a' X`: prev = Str.
    Some(last.text.clone())
}

fn is_unit_word(w: &str) -> bool {
    matches!(
        w.to_ascii_uppercase().as_str(),
        "DAY"
            | "HOUR"
            | "MINUTE"
            | "SECOND"
            | "MONTH"
            | "YEAR"
            | "DAYS"
            | "HOURS"
            | "MINUTES"
            | "SECONDS"
    )
}

/// 단순 열 참조(`col` · `a.col` · `s.t.col`)면 마지막 이름.
fn bare_column(toks: &[&Token]) -> Option<String> {
    if toks.is_empty() || toks.len() > 5 || toks.len().is_multiple_of(2) {
        return None;
    }
    for (n, t) in toks.iter().enumerate() {
        if n % 2 == 0 {
            if !(t.kind == Kind::Word && !is_keyword(&t.text) || t.kind == Kind::Quoted) {
                return None;
            }
        } else if !t.is_punct(".") {
            return None;
        }
    }
    let last = toks[toks.len() - 1];
    if last.kind == Kind::Quoted || last.text == "*" {
        return None;
    }
    Some(last.text.clone())
}

fn need_space(prev: &str, next: &str) -> bool {
    if prev.is_empty() {
        return false;
    }
    let pc = prev.chars().last().unwrap_or(' ');
    let nc = next.chars().next().unwrap_or(' ');
    if pc == '(' || pc == '.' || pc == ' ' || pc == '\t' {
        return false;
    }
    if nc == ')' || nc == ',' || nc == '.' || nc == ';' {
        return false;
    }
    // 함수 호출 `NAME(`: 앞이 단어/닫는 괄호이고 다음이 `(`면 붙인다(키워드 `IN (` · `VALUES (` · `EXISTS (`는 띄운다).
    if nc == '(' {
        let word = prev
            .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$' || c == '#'))
            .next()
            .unwrap_or("");
        if !word.is_empty() && !is_keyword(word) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{format_basic, Case};

    /// 탭 들여쓰기 + 콤마 뒤 **탭** 간격(kiros33 배치 · 옛 기본 `,\t`를 기대하는 시험용).
    fn tab_opts() -> Options {
        Options {
            comma_gap: crate::Gap::Tab,
            ..Options::default()
        }
    }

    fn words(s: &str) -> Vec<String> {
        lex(s)
            .into_iter()
            .filter(|t| !t.is_comment())
            .map(|t| t.text.to_ascii_uppercase())
            .collect()
    }

    /// 토큰 보존: 입력과 출력의 토큰(공백 제외 · 대문자)이 같다 · 멱등: 다시 포맷해도 같다.
    fn assert_tokens_kept(src: &str, opts: &Options) {
        let out = format_basic(src, opts);
        assert_eq!(words(src), words(&out), "\n--- in\n{src}\n--- out\n{out}");
        let again = format_basic(&out, opts);
        assert_eq!(out, again, "idempotent\n--- out\n{out}\n--- again\n{again}");
    }

    #[test]
    fn basic_select_layout() {
        let o = tab_opts();
        let src = "select a.x, b.y as yy, count(*) cnt from t a inner join u b on a.id = b.id and b.k > 1 where a.x = 1 and b.y in ('p', 'q') group by a.x, b.y order by a.x desc;";
        let out = format_basic(src, &o);
        let want = concat!(
            "SELECT\n\ta.x\n,\tb.y AS yy\n,\tcount(*) cnt\nFROM\n\tt a\nINNER JOIN u b\n\tON a.id = b.id\n\tAND b.k > 1\n",
            "WHERE\n\ta.x = 1\n\tAND b.y IN ('p', 'q')\nGROUP BY\n\ta.x\n,\tb.y\nORDER BY\n\ta.x DESC;\n"
        );
        assert_eq!(out, want);
        assert_tokens_kept(src, &o);
    }

    #[test]
    fn where_seed_and_trailing_comma() {
        let o = Options {
            where_seed: true,
            comma: Comma::Trailing,
            indent: crate::Indent::Spaces(2),
            tab_width: 2,
            ..Options::default()
        };
        let out = format_basic("select a, b from t where x = 1 or y = 2", &o);
        assert_eq!(
            out,
            "SELECT\n  a,\n  b\nFROM\n  t\nWHERE 1=1\n  AND x = 1\n  OR y = 2\n"
        );
        // 이미 1=1이 있으면 두 번 넣지 않는다.
        let out2 = format_basic("select a from t where 1=1 and x = 1", &o);
        assert_eq!(out2, "SELECT\n  a\nFROM\n  t\nWHERE 1=1\n  AND x = 1\n");
        // 공백 들여쓰기 + 줄 앞 콤마 = 한 단계 위 열에 `,`.
        let o3 = Options {
            indent: crate::Indent::Spaces(4),
            tab_width: 4,
            ..Options::default()
        };
        assert_eq!(
            format_basic("select a, b from t", &o3),
            "SELECT\n    a\n  , b\nFROM\n    t\n"
        );
    }

    #[test]
    fn subquery_case_and_comments_kept() {
        let o = tab_opts();
        let src = "-- head\nselect a.k, (select max(b.d) from t2 b where b.k = a.k) as last_d, /* c */ case when a.q > 100 then 'H' when a.q > 0 then 'L' else 'N' end grp -- tail\nfrom t1 a where a.k between 1 and 9 and exists (select 1 from t3 c where c.k = a.k)";
        assert_tokens_kept(src, &o);
        let out = format_basic(src, &o);
        assert!(
            out.starts_with("-- head\nSELECT\n\ta.k\n,\t(\n\t\tSELECT\n\t\t\tmax(b.d)\n"),
            "{out}"
        );
        assert!(out.contains("\t) AS last_d\n"), "{out}");
        assert!(
            out.contains(",\t/* c */ CASE WHEN a.q > 100 THEN 'H'"),
            "{out}"
        );
        // 줄 끝 주석 앞 간격도 `comma_gap`(탭 간격 = 탭).
        assert!(out.contains("END grp\t-- tail\n"), "{out}");
        assert!(out.contains("\tAND EXISTS (\n\t\tSELECT\n"), "{out}");
        assert!(out.contains("\ta.k BETWEEN 1 AND 9\n"), "{out}");
    }

    #[test]
    fn long_case_is_multiline() {
        let o = Options {
            case_inline_max: 40,
            ..Options::default()
        };
        let out = format_basic(
            "select case when a.qty > 100 then 'HIGH' when a.qty > 0 then 'LOW' else 'NONE' end as g from t a",
            &o,
        );
        assert!(
            out.contains("\tCASE\n\t\tWHEN a.qty > 100 THEN 'HIGH'\n\t\tWHEN a.qty > 0 THEN 'LOW'\n\t\tELSE 'NONE'\n\tEND AS g\n"),
            "{out}"
        );
    }

    #[test]
    fn dml_and_oneliners() {
        let o = tab_opts();
        let src = "insert into tb_code (cd, nm) values ('1', 'a');\n\nupdate tb_x a set a.q = 1, a.d = sysdate where a.k = 'x';\ndelete from tb_y where 1=1 and k = 2";
        let out = format_basic(src, &o);
        // 짧은 DML 한 줄은 그대로(빈 줄 규칙만).
        assert_eq!(
            out,
            "insert into tb_code (cd, nm) values ('1', 'a');\n\nupdate tb_x a set a.q = 1, a.d = sysdate where a.k = 'x';\n\ndelete from tb_y where 1=1 and k = 2\n"
        );
        let o2 = Options {
            keep_oneliners: false,
            ..tab_opts()
        };
        let out2 = format_basic(
            "update tb_x a set a.q = 1, a.d = sysdate where a.k = 'x' and a.z is null;",
            &o2,
        );
        assert_eq!(
            out2,
            "UPDATE tb_x a SET\n\ta.q = 1\n,\ta.d = sysdate\nWHERE\n\ta.k = 'x'\n\tAND a.z IS NULL;\n"
        );
        let o3 = Options {
            keep_oneliners: false,
            semicolon_newline: true,
            identifier_case: Case::Upper,
            function_case: Case::Upper,
            ..tab_opts()
        };
        let out3 = format_basic("update tb_x a set a.q = nvl(a.q, 0) where a.k = 'x';", &o3);
        assert_eq!(
            out3,
            "UPDATE TB_X A SET\n\tA.Q = NVL(A.Q, 0)\nWHERE\n\tA.K = 'x'\n;\n"
        );
        assert_tokens_kept("insert into t (a, b) select x, y from u where u.z = 1", &o2);
        assert_tokens_kept(
            "merge into t a using (select k, v from s) s on (a.k = s.k) when matched then update set a.v = s.v when not matched then insert (k, v) values (s.k, s.v)",
            &o2,
        );
        // 한 줄 SELECT는 늘 포맷한다(§19는 짧은 DML만).
        assert_eq!(
            format_basic("select 1 from dual", &o),
            "SELECT\n\t1\nFROM\n\tdual\n"
        );
    }

    #[test]
    fn with_and_set_ops() {
        let o = tab_opts();
        let src = "with c1 as (select k from t1), c2 as (select k from t2) select a.k from c1 a union all select b.k from c2 b";
        assert_tokens_kept(src, &o);
        let out = format_basic(src, &o);
        assert!(
            out.starts_with(
                "WITH\n\tc1 AS (\n\t\tSELECT\n\t\t\tk\n\t\tFROM\n\t\t\tt1\n\t)\n,\tc2 AS ("
            ),
            "{out}"
        );
        assert!(out.contains("\nUNION ALL\nSELECT\n"), "{out}");
    }

    #[test]
    fn alias_options() {
        let o = Options {
            column_alias_all: true,
            column_as: AliasAs::Add,
            table_as: AliasAs::Add,
            ..tab_opts()
        };
        let out = format_basic("select a.x, a.y yy, sum(a.q) from t a", &o);
        assert_eq!(
            out,
            "SELECT\n\ta.x AS x\n,\ta.y AS yy\n,\tsum(a.q)\nFROM\n\tt AS a\n"
        );
        let o2 = Options {
            column_as: AliasAs::Remove,
            ..tab_opts()
        };
        let out2 = format_basic("select a.y as yy from t a", &o2);
        assert_eq!(out2, "SELECT\n\ta.y yy\nFROM\n\tt a\n");
    }

    #[test]
    fn plsql_passthrough_and_ddl() {
        let o = Options::default();
        let src =
            "CREATE OR REPLACE PROCEDURE P AS\nBEGIN\n  UPDATE t SET a = 1;\n  COMMIT;\nEND;\n/";
        assert_eq!(format_basic(src, &o), format!("{src}\n"));
        let ddl = "create table t (a int, b varchar2(10))";
        assert_eq!(format_basic(ddl, &o), format!("{ddl}\n"));
    }

    #[test]
    fn sample_round_trips() {
        let o = Options {
            keep_oneliners: false,
            ..Options::default()
        };
        assert_tokens_kept(crate::SAMPLE_SQL, &o);
        let o2 = Options {
            keep_oneliners: false,
            where_seed: true,
            comma: Comma::Trailing,
            logical_newline: LogicalNewline::After,
            list_style: ListStyle::Auto,
            ..Options::default()
        };
        // 시드는 토큰을 더하므로 보존 검사는 시드 없는 쪽만 · 여기서는 멱등만.
        let out = format_basic(crate::SAMPLE_SQL, &o2);
        assert_eq!(out, format_basic(&out, &o2));
        assert!(out.contains("WHERE 1=1"), "{out}");
    }

    #[test]
    fn align_helpers() {
        assert_eq!(display_width("ab\tc", 4), 5);
        assert_eq!(tabs_to(2, 8, 4), "\t\t");
        assert_eq!(tabs_to(4, 8, 4), "\t");
        let o = tab_opts();
        let lines = layout("select a, b from t where x = 1 and y = 2", &o);
        let b = blocks(&lines);
        assert!(b
            .iter()
            .any(|(s, e)| e - s == 2 && lines[*s].role == Role::Item));
        assert!(b
            .iter()
            .any(|(s, e)| e - s == 2 && lines[*s].role == Role::Cond));
        assert_eq!(line_prefix(1, true, &o), ",\t");
        assert_eq!(line_prefix(2, false, &o), "\t\t");
    }

    /// 시드 구분(탭)과 조건 줄 위치(같은 열)(사용자 09-29): `WHERE\t1=1` · `ON\t1=1` · AND/OR가 WHERE 열에 · 멱등.
    #[test]
    fn seed_gap_and_cond_same_column() {
        let o = Options {
            where_seed: true,
            seed_gap: crate::Gap::Tab,
            cond_indent: false,
            ..Options::default()
        };
        let out = format_basic(
            "select a from t inner join u on u.k = t.k where x = 1 or y = 2",
            &o,
        );
        assert_eq!(
            out,
            "SELECT\n\ta\nFROM\n\tt\nINNER JOIN u\n\tON\t1=1\n\tAND u.k = t.k\nWHERE\t1=1\nAND x = 1\nOR y = 2\n"
        );
        assert_eq!(format_basic(&out, &o), out, "idempotent");
        // 기본(공백 · 한 단계 안)은 종전 그대로.
        let d = Options {
            where_seed: true,
            ..Options::default()
        };
        assert_eq!(
            format_basic("select a from t where x = 1", &d),
            "SELECT\n\ta\nFROM\n\tt\nWHERE 1=1\n\tAND x = 1\n"
        );
    }

    /// T-255 괄호 AND/OR 그룹 시드(스킬 §3): `(1=0` + OR 줄 + `)` 한 단계 더 · 원문 시드 유지 · 멱등 · BETWEEN AND는 그룹이 아님.
    #[test]
    fn paren_group_seed() {
        let o = Options {
            where_seed: true,
            paren_seed: true,
            ..Options::default()
        };
        let out = format_basic("select a from t where x = 1 and (b > 0 or c > 0)", &o);
        assert_eq!(
            out,
            "SELECT\n\ta\nFROM\n\tt\nWHERE 1=1\n\tAND x = 1\n\tAND (1=0\n\t\tOR b > 0\n\t\tOR c > 0\n\t\t)\n"
        );
        assert_eq!(format_basic(&out, &o), out, "idempotent");
        // AND 그룹 = (1=1 · 원문에 1=1이 있으면 그대로.
        let out2 = format_basic("select a from t where (1=1 and b = 1 and c = 2)", &o);
        assert!(
            out2.contains("\tAND (1=1\n\t\tAND b = 1\n\t\tAND c = 2\n\t\t)"),
            "{out2}"
        );
        // BETWEEN … AND 만 있는 괄호는 인라인 · 서브쿼리 괄호는 블록 그대로.
        let out3 = format_basic(
            "select a from t where (x between 1 and 9) and exists (select 1 from u)",
            &o,
        );
        assert!(out3.contains("AND (x BETWEEN 1 AND 9)"), "{out3}");
        assert!(!out3.contains("(1=1\n\t\tAND x BETWEEN"), "{out3}");
        // 옵션이 꺼져 있으면 종전대로 인라인.
        let off = Options {
            where_seed: true,
            ..Options::default()
        };
        assert!(format_basic("select a from t where (b > 0 or c > 0)", &off)
            .contains("AND (b > 0 OR c > 0)"));
    }

    /// T-255 방언 치환(스킬 §22): 이름만 바뀌고 배치는 같다.
    #[test]
    fn dialect_substitution() {
        let o = Options {
            dialect_target: DialectTarget::Oracle,
            ..Options::default()
        };
        let out = format_basic("select isnull(a, 0), len(b), getdate() from t except select 1, 2, current_timestamp from u", &o);
        assert!(
            out.contains("NVL(a, 0)") && out.contains("LENGTH(b)") && out.contains("SYSDATE"),
            "{out}"
        );
        assert!(
            out.contains("MINUS") && !out.contains("EXCEPT") && !out.contains("CURRENT_TIMESTAMP"),
            "{out}"
        );
        let t = Options {
            dialect_target: DialectTarget::Tsql,
            ..Options::default()
        };
        let out2 = format_basic(
            "select nvl(a, 0), substr(b, 1, 2), sysdate from t minus select 1, 2, 3 from u",
            &t,
        );
        assert!(
            out2.contains("ISNULL(a, 0)")
                && out2.contains("SUBSTRING(b, 1, 2)")
                && out2.contains("GETDATE()"),
            "{out2}"
        );
        assert!(out2.contains("EXCEPT"), "{out2}");
        // ANSI.
        let a = Options {
            dialect_target: DialectTarget::Ansi,
            ..Options::default()
        };
        let out3 = format_basic("select isnull(a, 0), len(b), sysdate from t", &a);
        assert!(
            out3.contains("COALESCE(a, 0)")
                && out3.contains("CHAR_LENGTH(b)")
                && out3.contains("CURRENT_TIMESTAMP"),
            "{out3}"
        );
        // None = 그대로.
        assert_tokens_kept(
            "select isnull(a, 0), getdate() from t except select 1, 2 from u",
            &Options::default(),
        );
    }

    /// T-255 별칭 자동 부여(스킬 2-1): FROM/JOIN의 별칭 없는 테이블·인라인뷰 · 이미 있는 별칭은 유지 · 문장 안 단어와 안 겹침 · UPDATE 대상은 안 건드림.
    #[test]
    fn auto_alias_for_tables() {
        let o = Options {
            auto_alias: true,
            ..Options::default()
        };
        let out = format_basic(
            "select x from tb_order inner join tb_item i on i.k = tb_order.k where a = 1",
            &o,
        );
        // `A`는 WHERE의 단어 a와 겹쳐 건너뛴다 · 기존 별칭 i는 유지.
        assert!(out.contains("\ttb_order B\n"), "{out}");
        assert!(out.contains("INNER JOIN tb_item i\n"), "{out}");
        assert!(!out.contains("tb_order A"), "{out}");
        // 인라인뷰도 별칭(문장 전체에서 유일 — 안쪽 dual이 C를 쓰면 뷰는 D).
        let out2 = format_basic(
            "select x from tb_order, (select 1 from dual) where a = 1",
            &o,
        );
        assert!(out2.contains("\ttb_order B\n"), "{out2}");
        assert!(out2.contains("dual C\n"), "{out2}");
        assert!(out2.contains(") D\n"), "{out2}");
        assert_eq!(format_basic(&out2, &o), out2, "idempotent");
        assert_eq!(alias_name(0), "A");
        assert_eq!(alias_name(25), "Z");
        assert_eq!(alias_name(26), "AA");
        assert_eq!(alias_name(27), "AB");
        let up = format_basic("update tb_order set qty = 0 where k = 1", &o);
        assert!(
            !up.contains("tb_order A") && !up.contains("tb_order B"),
            "{up}"
        );
        // 꺼져 있으면 그대로(토큰 보존).
        assert_tokens_kept(
            "select x from tb_order, tb_item where a = 1",
            &Options::default(),
        );
    }

    /// `AS` 앞뒤 간격 = `as_gap`(기본 공백 · 탭이면 `a\tAS\tx` · 테이블 별칭도 · 멱등).
    #[test]
    fn as_gap_tab_or_space() {
        let o = Options::default();
        let out = format_basic("select a.x as x from t as a", &o);
        assert!(out.contains("a.x AS x") && out.contains("t AS a"), "{out}");
        let o = Options {
            as_gap: crate::Gap::Tab,
            ..Options::default()
        };
        let out = format_basic("select a.x as x from t as a", &o);
        assert!(
            out.contains("a.x\tAS\tx") && out.contains("t\tAS\ta"),
            "{out}"
        );
        assert_eq!(format_basic(&out, &o), out, "idempotent");
    }

    /// AND/OR 뒤 간격 = `logical_gap`(줄 앞 배치 · 탭이면 `AND\t조건` · 시드 AND도 · 멱등).
    #[test]
    fn logical_gap_tab() {
        let o = Options {
            logical_gap: crate::Gap::Tab,
            where_seed: true,
            ..Options::default()
        };
        let out = format_basic("select 1 from t where a = 1 and b = 2 or c = 3", &o);
        assert!(out.contains("\tAND\ta = 1\n"), "{out}");
        assert!(out.contains("\tOR\tc = 3\n"), "{out}");
        assert_eq!(format_basic(&out, &o), out, "idempotent");
        let o2 = Options {
            logical_gap: crate::Gap::Tab,
            logical_newline: LogicalNewline::After,
            ..Options::default()
        };
        let out = format_basic("select 1 from t where a = 1 and b = 2", &o2);
        assert!(out.contains("a = 1 AND\n"), "{out}");
    }

    /// 연산자 양쪽 간격 = `operator_gap`(공백/탭 · 꺼져 있으면 붙여 쓴다).
    #[test]
    fn operator_gap_tab_or_space() {
        let o = Options {
            operator_gap: crate::Gap::Tab,
            ..Options::default()
        };
        let out = format_basic(
            "select 1 from t where a = 1 and b > 2 and c in (1, 2) and d like 'x%' and e is not null and f not in (3)",
            &o,
        );
        assert!(out.contains("a\t=\t1"), "{out}");
        assert!(out.contains("b\t>\t2"), "{out}");
        // 단어 연산자도 조각 · 4자 이상(LIKE · IS NOT · NOT IN)은 왼쪽 = 설정(탭) · 오른쪽 = 공백 1개(`operator_long_space` 기본 켬).
        assert!(out.contains("c\tIN\t(1, 2)"), "{out}");
        assert!(out.contains("d\tLIKE 'x%'"), "{out}");
        assert!(out.contains("e\tIS NOT NULL"), "{out}");
        assert!(out.contains("f\tNOT IN (3)"), "{out}");
        let o3 = Options {
            operator_gap: crate::Gap::Tab,
            operator_long_space: false,
            ..Options::default()
        };
        let out = format_basic("select 1 from t where d like 'x%'", &o3);
        assert!(out.contains("d\tLIKE\t'x%'"), "{out}");
        // 멱등.
        let o4 = Options::default();
        let src = "select 1 from t where c in (1, 2) and d not like 'x%' and e is null";
        let once = format_basic(src, &o4);
        assert_eq!(format_basic(&once, &o4), once, "{once}");
        assert!(
            once.contains("d NOT LIKE 'x%'") && once.contains("e IS NULL"),
            "{once}"
        );
        let o = Options {
            operator_spaces: false,
            ..Options::default()
        };
        let out = format_basic("select 1 from t where a = 1", &o);
        assert!(out.contains("a=1"), "{out}");
    }

    /// 줄 앞 콤마 + 탭 들여쓰기 + 간격 "공백"(사용자 09-29): 콤마 앞은 탭만(공백 채움 없음) · 뒤는 간격 설정.
    #[test]
    fn leading_comma_gap_follows_setting_in_tab_mode() {
        let o = Options {
            indent: Indent::Tab,
            tab_width: 4,
            comma_gap: crate::Gap::Space,
            ..Options::default()
        };
        assert_eq!(line_prefix(1, true, &o), ", ");
        assert_eq!(line_prefix(2, true, &o), "\t, ");
        let o = Options {
            indent: Indent::Tab,
            comma_gap: crate::Gap::Tab,
            ..Options::default()
        };
        assert_eq!(line_prefix(1, true, &o), ",\t");
        let o = Options {
            indent: Indent::Spaces(2),
            comma_gap: crate::Gap::Tab,
            ..Options::default()
        };
        assert_eq!(line_prefix(1, true, &o), ",\t");
    }

    /// T-255 테이블 설명 주석(스킬 2-2): 호스트가 준 설명이 있는 물리 테이블 줄 끝에 `--\t설명` · 없는 것·서브쿼리는 없음.
    #[test]
    fn table_description_comments() {
        let o = Options {
            comma_gap: crate::Gap::Tab,
            table_comments: vec![
                ("BISCM.TB_ORDER".into(), "주문".into()),
                ("TB_ITEM".into(), "품목".into()),
            ],
            ..Options::default()
        };
        let out = format_basic("select 1 from biscm.tb_order a inner join tb_item b on b.k = a.k left join tb_none c on c.k = a.k, (select 1 from dual) d", &o);
        assert!(out.contains("biscm.tb_order a\t--\t주문\n"), "{out}");
        assert!(out.contains("INNER JOIN tb_item b\t--\t품목\n"), "{out}");
        assert!(out.contains("LEFT JOIN tb_none c\n"), "{out}");
        assert!(!out.contains("d\t--"), "{out}");
        // 다시 포맷해도 주석이 두 번 붙지 않는다(원문 주석은 인라인 조각으로 유지 · 같은 글).
        let again = format_basic(&out, &o);
        assert_eq!(again.matches("--\t주문").count(), 1, "{again}");
    }
}
