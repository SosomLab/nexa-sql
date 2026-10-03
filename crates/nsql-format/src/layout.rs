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

/// 연산자 부류(사용자 09-30): 단항 후위 · 이항 · BETWEEN(특수).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpKind {
    /// `IS NULL` · `IS NOT NULL` — 왼쪽 피연산자만.
    Unary,
    /// `=` `<>` `<` `>` `<=` `>=` `IN` `IS` `LIKE` `NOT IN` `NOT LIKE` `IS NOT`.
    Binary,
    /// `BETWEEN` · `NOT BETWEEN` — 오른쪽에 `x AND y`.
    Between,
}

/// 연산자 글 → 부류(대소문자 무시 · 순수).
#[must_use]
pub fn op_kind(op: &str) -> OpKind {
    let up: String = op
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase();
    match up.as_str() {
        "IS NULL" | "IS NOT NULL" | "IS TRUE" | "IS NOT TRUE" | "IS FALSE" | "IS NOT FALSE"
        | "IS UNKNOWN" | "IS NOT UNKNOWN" => OpKind::Unary,
        "BETWEEN" | "NOT BETWEEN" => OpKind::Between,
        _ => OpKind::Binary,
    }
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
                } else if tight {
                    // 붙는 토큰(`;` `)` `,` `.`) 앞의 연산자 뒤 간격은 지운다(`IS NULL ;` 결함 · 09-30).
                    while s.ends_with(' ') || s.ends_with('\t') {
                        s.pop();
                    }
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
                // 아웃라이어(채움 = 공백 하나)여도 AS 뒤는 `as_gap`(탭 1개 · 사용자 09-30 정정).
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
                // ★ 연산자 부류(사용자 09-30): 단항 후위(`IS NULL` · `IS NOT NULL`) = 왼쪽 간격·정렬만, 오른쪽 없음 ·
                //   이항(`=` `<>` `IN` `LIKE` …) = 좌우 간격(4자 이상은 오른쪽 공백 1개 · `operator_long_space`) ·
                //   BETWEEN = 이항과 같되 오른쪽은 늘 공백 1개(`a BETWEEN x AND y` · 안의 AND는 공백 하나).
                let kind = op_kind(op);
                let fill = pad.map(|f| f(i, &s)).unwrap_or_default();
                let long = opts.operator_long_space && op.chars().count() >= 4;
                if !fill.is_empty() {
                    // 정렬된 연산자(확장의 탭 채움) · 채움이 탭 없는 공백뿐이면(아웃라이어) 뒤도 공백 하나.
                    let plain = !fill.contains('\t');
                    s.push_str(&fill);
                    s.push_str(op);
                    match kind {
                        OpKind::Unary => {}
                        OpKind::Between => s.push(' '),
                        OpKind::Binary => {
                            if opts.operator_gap == crate::Gap::Tab && !long && !plain {
                                s.push('\t');
                            } else {
                                s.push(' ');
                            }
                        }
                    }
                } else if opts.operator_spaces {
                    let g = match opts.operator_gap {
                        crate::Gap::Tab => '\t',
                        crate::Gap::Space => ' ',
                    };
                    if need_space {
                        s.push(g);
                    }
                    s.push_str(op);
                    match kind {
                        OpKind::Unary => {}
                        OpKind::Between => s.push(' '),
                        OpKind::Binary => s.push(if long { ' ' } else { g }),
                    }
                } else {
                    // 연산자 공백 끔 = 기호 연산자만 붙인다(`a=1`) · 낱말 연산자(`IS NULL` `IN` `BETWEEN`)는 붙일 수 없어 한 칸.
                    let word = op.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
                    if word && need_space {
                        s.push(' ');
                    }
                    s.push_str(op);
                    // 단항 뒤에는 피연산자가 없다 · BETWEEN 뒤는 늘 한 칸.
                    match kind {
                        OpKind::Unary => {}
                        OpKind::Between => s.push(' '),
                        OpKind::Binary if word => s.push(' '),
                        OpKind::Binary => tight_next = true,
                    }
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
                // 인라인 주석 앞 간격 = `comment_space`/`comment_gap`(사용자 09-30 · 종전엔 콤마 간격을 빌려 썼다).
                if !s.is_empty() && opts.comment_space {
                    if opts.comment_gap == crate::Gap::Tab {
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
    // 앞 문장이 스크립트 명령(`EXEC` · `PRINT` · `SET` …)이었나 — 명령 줄 앞뒤에는 빈 줄을 강제로 넣지 않는다(원문 그대로).
    let mut prev_cmd = false;
    while i < toks.len() {
        // 문장 앞 독립 주석.
        let stmt_start = i;
        // 문장 끝 = 최상위 `;`(괄호 밖) — 통과 대상(PL/SQL·CREATE 루틴)은 끝까지.
        if starts_block_passthrough(&toks[i..]) {
            // ★ 블록의 끝까지만(10-03 · 종전 = 문서 끝까지 → 블록 하나 뒤의 문장이 전부 포맷되지 않았다).
            let j = block_end(&toks, i);
            let text = src[toks[i].span.0..toks[j - 1].span.1].trim_end();
            let blank = if first_stmt {
                0
            } else if prev_cmd {
                (toks[i].nl_before.saturating_sub(1) as usize).min(opts.max_blank_lines)
            } else {
                blank_of(&toks[i], first_stmt, opts)
            };
            push_raw(&mut out, text, blank);
            first_stmt = false;
            prev_cmd = false;
            i = j;
            continue;
        }
        // ★ 스크립트 명령(사용자 10-03): 끝은 `;`가 아니라 실행기의 규칙으로 정한다 — `EXEC` = 줄 또는 블록 · 그 밖의 명령 = 그 줄.
        //   (종전에는 `;` 없는 명령 줄이 다음 `;`까지 뒤 문장을 삼켜 통째로 원문 통과였다.)
        let head = (i..toks.len()).find(|&k| !toks[k].is_comment());
        let is_cmd = head.is_some_and(|h| is_command_head(&toks, h));
        let blank = if first_stmt {
            0
        } else if is_cmd || prev_cmd {
            (toks[i].nl_before.saturating_sub(1) as usize).min(opts.max_blank_lines)
        } else {
            blank_of(&toks[i], first_stmt, opts)
        };
        if is_cmd {
            let h = head.unwrap_or(i);
            let j = match exec_statement(&mut out, &toks, i, src, opts, blank) {
                Some(j) => j,
                None => {
                    // 한 줄 명령: 그 줄의 `;`까지 또는 줄 끝까지 — 원문 그대로.
                    let mut j = h + 1;
                    while j < toks.len() && toks[j].nl_before == 0 {
                        j += 1;
                        if toks[j - 1].is_punct(";") {
                            break;
                        }
                    }
                    format_statement(&mut out, &toks[i..j], src, opts, blank);
                    j
                }
            };
            first_stmt = false;
            prev_cmd = true;
            i = j;
            continue;
        }
        let mut depth = 0i32;
        let mut j = i;
        while j < toks.len() {
            let t = &toks[j];
            // 단독 `/` · `GO` 줄 = 문장 끝(실행기 `collect_sql`과 같게 — `;` 없는 T-SQL 배치가 뒤 문장을 삼키지 않는다).
            if j > i && t.nl_before > 0 && is_terminator_line(&toks, j) {
                break;
            }
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
        format_statement(&mut out, stmt, src, opts, blank);
        first_stmt = false;
        prev_cmd = false;
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

/// `BEGIN`이 블록이 아니라 **트랜잭션 문장**인가(`BEGIN;` · `BEGIN TRANSACTION` · `BEGIN WORK` …) — nsql-script `is_begin_transaction`과 같게.
fn begin_is_transaction(toks: &[Token], at: usize) -> bool {
    let Some(n) = toks[at + 1..].iter().find(|t| !t.is_comment()) else {
        return false;
    };
    n.is_punct(";")
        || (n.kind == Kind::Word
            && matches!(
                n.up().as_str(),
                "TRANSACTION"
                    | "TRAN"
                    | "WORK"
                    | "DEFERRED"
                    | "IMMEDIATE"
                    | "EXCLUSIVE"
                    | "ISOLATION"
                    | "READ"
                    | "DISTRIBUTED"
            ))
}

/// 통과 블록(`toks[start]`부터)의 끝(배타) — 실행기 `collect_sql`과 같은 생각: **단독 `/`·`GO` 줄 앞**에서, 그것이 없으면
/// `BEGIN`/`CASE` … `END` 짝이 0으로 돌아온 뒤(또는 짝 없는 `END` = 패키지·타입 명세의 끝) **다음 최상위 `;`** 뒤에서.
/// `END IF`·`END LOOP`·`END WHILE`·`END REPEAT`·`END FOR`는 짝을 닫지 않는다(`IF`·`LOOP`는 열지 않으므로). 둘 다 없으면 끝까지.
fn block_end(toks: &[Token], start: usize) -> usize {
    let mut depth = 0i32;
    let mut paren = 0i32;
    let mut closed = false;
    let mut j = start;
    while j < toks.len() {
        let t = &toks[j];
        if j > start && t.nl_before > 0 && is_terminator_line(toks, j) {
            return j;
        }
        if t.is_punct("(") {
            paren += 1;
        } else if t.is_punct(")") {
            paren -= 1;
        } else if t.kind == Kind::Word {
            let up = t.up();
            if (up == "BEGIN" && !begin_is_transaction(toks, j)) || up == "CASE" {
                depth += 1;
            } else if up == "END" {
                let next = toks[j + 1..].iter().find(|n| !n.is_comment());
                let opener_less = next.is_some_and(|n| {
                    n.kind == Kind::Word
                        && matches!(n.up().as_str(), "IF" | "LOOP" | "WHILE" | "REPEAT" | "FOR")
                });
                if !opener_less {
                    depth -= 1;
                    if depth <= 0 {
                        depth = 0;
                        closed = true;
                    }
                }
            }
        } else if t.is_punct(";") && paren <= 0 && closed {
            return j + 1;
        }
        j += 1;
    }
    toks.len()
}

/// 문장 첫 토큰이 PL/SQL 블록·루틴 생성이면 블록 끝([`block_end`])까지 통과(안의 `;`로 쪼개면 안 된다).
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
        "DECLARE" => true,
        "BEGIN" => {
            let at = toks.iter().position(|t| !t.is_comment()).unwrap_or(0);
            !begin_is_transaction(toks, at)
        }
        "CREATE" => words.iter().any(|w| {
            matches!(
                w.as_str(),
                "PROCEDURE" | "FUNCTION" | "PACKAGE" | "TRIGGER" | "TYPE" | "BODY"
            )
        }),
        _ => false,
    }
}

/// 스크립트 명령을 시작하는 단어(대문자) — `EXEC` 블록 본문이 여기서 끝난다. nsql-script `is_command_start`와 같은 목록
/// (이 크레이트는 의존 0이라 사본을 둔다 · 그쪽을 바꾸면 여기도).
const COMMAND_WORDS: &[&str] = &[
    "VAR",
    "VARIABLE",
    "PRINT",
    "EXEC",
    "EXECUTE",
    "CONN",
    "CONNECT",
    "DISC",
    "DISCONNECT",
    "SET",
    "DEF",
    "DEFINE",
    "ACC",
    "ACCEPT",
    "COL",
    "COLUMN",
    "UNDEF",
    "UNDEFINE",
    "DESC",
    "DESCRIBE",
    "SHO",
    "SHOW",
    "CONNTYPE",
    "SPO",
    "SPOOL",
    "PROMPT",
    "REM",
    "REMARK",
    "GO",
    "WHENEVER",
];

/// `toks[j]`(줄 첫 토큰)가 단독 `/` 또는 `GO` 줄인가 — 문장 종결 줄.
fn is_terminator_line(toks: &[Token], j: usize) -> bool {
    let t = &toks[j];
    toks.get(j + 1).is_none_or(|n| n.nl_before > 0)
        && ((t.kind == Kind::Op && t.text == "/") || t.is("GO"))
}

/// `toks[j]`가 스크립트 명령의 첫 토큰인가(`CONNECT BY`는 질의의 일부 · `@a = 1`은 SQL Server 변수 대입 · 단독 `/` 줄 포함).
fn is_command_head(toks: &[Token], j: usize) -> bool {
    let t = &toks[j];
    match t.kind {
        Kind::Op => is_terminator_line(toks, j),
        Kind::Bind => {
            (t.text.starts_with('@')
                && !toks
                    .get(j + 1)
                    .is_some_and(|n| n.kind == Kind::Op && n.text == "="))
                || matches!(
                    t.text.as_str(),
                    ":setvar" | ":connect" | ":disconnect" | ":r"
                )
        }
        Kind::Word => {
            let up = t.up();
            if matches!(up.as_str(), "CONNECT" | "CONN")
                && toks.get(j + 1).is_some_and(|n| n.is("BY"))
            {
                return false;
            }
            COMMAND_WORDS.contains(&up.as_str())
        }
        _ => false,
    }
}

/// 줄 첫 토큰 `toks[j]`가 `EXEC` 블록 본문을 끝내는가 — 단독 `/` · `@`로 시작하는 줄 · 스크립트 명령 줄.
fn exec_block_boundary(toks: &[Token], j: usize) -> bool {
    let t = &toks[j];
    (t.kind == Kind::Bind && t.text.starts_with('@')) || is_command_head(toks, j)
}

/// `EXEC` 본문이 프로시저·패키지 호출인가 — `이름[.이름…]` 뒤가 끝 · `(` · 인자(연산자가 아님). PL/SQL 문장 머리말(`OPEN` …)과
/// 대입(`x := …`)은 호출이 아니다.
fn exec_is_call(core: &[Token]) -> bool {
    const NOT_CALL: &[&str] = &[
        "OPEN",
        "CLOSE",
        "RAISE",
        "IMMEDIATE",
        "LOCK",
        "SAVEPOINT",
        "GOTO",
        "EXIT",
        "CONTINUE",
        "FORALL",
        "NULL",
    ];
    let name = |t: &Token| {
        (t.kind == Kind::Word && !is_keyword(&t.text) && !NOT_CALL.contains(&t.up().as_str()))
            || t.kind == Kind::Quoted
    };
    let mut k = 0;
    if !core.first().is_some_and(name) {
        return false;
    }
    k += 1;
    while core.get(k).is_some_and(|t| t.is_punct("."))
        && core
            .get(k + 1)
            .is_some_and(|t| matches!(t.kind, Kind::Word | Kind::Quoted))
    {
        k += 2;
    }
    core.get(k).is_none_or(|t| t.kind != Kind::Op)
}

/// 한 줄 `EXEC …`가 다음 줄(`toks[j]`가 그 첫 토큰)로 이어지는가 — 괄호가 열려 있거나 · 콤마로 이어지거나 · `;`뿐이거나 ·
/// SQL Server 이름 지정 인자(`@a = 1`)일 때. 실행기는 한 줄 `EXEC`를 줄 끝에서 끊으므로 이런 꼴은 한 줄로 합쳐야 실행된다.
fn exec_line_continues(toks: &[Token], j: usize, depth: i32) -> bool {
    let t = &toks[j];
    if t.is_comment() {
        return depth > 0;
    }
    depth > 0
        || t.is_punct(",")
        || t.is_punct(";")
        || toks[j - 1].is_punct(",")
        || (t.kind == Kind::Bind
            && t.text.starts_with('@')
            && toks
                .get(j + 1)
                .is_some_and(|n| n.kind == Kind::Op && n.text == "="))
}

/// ★ `EXEC`/`EXECUTE`로 시작하는 문장(사용자 10-03 "EXEC가 앞에 있으면 포맷 대상이 아니다"). 처리했으면 다음 문장의 토큰 위치.
///
/// 문장의 끝은 실행기(nsql-script `split`)와 같게 본다 — **홀로 선 `EXEC`** = 다음 줄부터 `;` · 빈 줄 · 단독 `/` · 다음 명령 줄까지
/// (블록) · **한 줄 `EXEC …`** = 그 줄(이어지는 줄은 [`exec_line_continues`]).
/// - 본문이 `SELECT`/`WITH`(= `SELECT … INTO`) → `EXEC` 한 줄 + 본문을 평소대로 포맷(블록 꼴 · 한 줄 꼴이었으면 블록 꼴로 바꾼다 ·
///   `;` 없이 바로 다음 줄에 SQL이 이어지면 블록이 그 줄을 삼키지 않게 `;`를 붙인다).
/// - 본문이 프로시저·패키지 호출(이름으로 시작) → **한 줄**(줄 바꿈만 없앤다 · 줄 안의 간격은 원문 그대로).
/// - 그 밖(대입 `:v := …` · `OPEN … FOR` · 줄 주석이 낀 호출 …) → 원문 그대로.
fn exec_statement(
    out: &mut Vec<Line>,
    toks: &[Token],
    start: usize,
    src: &str,
    opts: &Options,
    blank: usize,
) -> Option<usize> {
    let mut h = start;
    while h < toks.len() && toks[h].is_comment() {
        h += 1;
    }
    let head = toks.get(h)?;
    if !(head.is("EXEC") || head.is("EXECUTE")) {
        return None;
    }
    let bare = toks.get(h + 1).is_none_or(|t| t.nl_before > 0);
    // 문장의 끝.
    let mut depth = 0i32;
    let mut j = h + 1;
    while j < toks.len() {
        let t = &toks[j];
        if t.nl_before >= 2 {
            break;
        }
        if t.nl_before == 1 && j > h + 1 {
            let stop = if bare {
                exec_block_boundary(toks, j)
            } else {
                !exec_line_continues(toks, j, depth)
            };
            if stop {
                break;
            }
        }
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
    let body = &toks[h + 1..j];
    let has_semi = body.last().is_some_and(|t| t.is_punct(";"));
    let core = &body[..body.len() - usize::from(has_semi)];
    let first = core.iter().find(|t| !t.is_comment());
    let is_query = first.is_some_and(|t| t.is("SELECT") || t.is("WITH"));
    let is_call = exec_is_call(core) && depth == 0 && !core.iter().any(Token::is_comment);
    if !is_query && !is_call {
        // 원문 그대로(앞머리 주석은 `format_statement`가 제 줄로).
        format_statement(out, &toks[start..j], src, opts, blank);
        return Some(j);
    }
    // 앞머리 주석은 독립 줄.
    let mut blank = blank;
    for t in &toks[start..h] {
        let mut l = Line::new(0, Role::Comment);
        l.parts.push(Part::Text(t.text.clone()));
        l.blank_before = blank;
        blank = 0;
        out.push(l);
    }
    let exec = opts.keyword_case.apply(&head.text);
    let mut line = Line::new(0, Role::Raw);
    line.blank_before = blank;
    if is_call {
        // 한 줄: 줄 바꿈만 없앤다. 이미 한 줄이면 간격은 원문 그대로 · 여러 줄이었으면 줄 안의 간격(정렬용 공백·탭)은 한 칸으로,
        // 줄이 바뀐 자리는 띄어쓰기 규칙으로.
        let multi = body.iter().any(|t| t.nl_before > 0);
        let mut text = exec;
        let mut prev = head;
        for t in body {
            let gap = (t.nl_before == 0)
                .then(|| src.get(prev.span.1..t.span.0))
                .flatten()
                .filter(|g| g.chars().all(|c| c == ' ' || c == '\t'));
            match gap {
                Some(g) if !multi => text.push_str(g),
                Some("") => {}
                Some(_) => text.push(' '),
                None if std::ptr::eq(prev, head) || need_space(&prev.text, &t.text) => {
                    text.push(' ');
                }
                None => {}
            }
            text.push_str(&t.text);
            prev = t;
        }
        line.parts.push(Part::Text(text));
        out.push(line);
        return Some(j);
    }
    // 블록 꼴: `EXEC` 한 줄 + 평소대로 포맷한 질의.
    line.parts.push(Part::Text(exec));
    out.push(line);
    format_statement(out, body, src, opts, 0);
    let swallows_next = !bare
        && !has_semi
        && toks
            .get(j)
            .is_some_and(|n| n.nl_before == 1 && !exec_block_boundary(toks, j));
    if swallows_next {
        match out.last_mut() {
            Some(l)
                if !opts.semicolon_newline
                    && l.role != Role::Raw
                    && !matches!(l.parts.last(), Some(Part::Comment(_))) =>
            {
                match l.parts.last_mut() {
                    Some(Part::Text(t)) => t.push(';'),
                    _ => l.parts.push(Part::Text(";".to_string())),
                }
            }
            _ => {
                let mut l = Line::new(0, Role::Other);
                l.parts.push(Part::Text(";".to_string()));
                out.push(l);
            }
        }
    }
    Some(j)
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
    // 이 문장의 첫 줄 자리(빈 줄 수를 넣을 줄).
    let start = out.len();
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
    // 빈 줄 수는 이 문장이 만든 첫 줄에(종전에는 첫 단어로 시작하는 줄을 거꾸로 찾아, 앞 문장에 빈 줄이 없으면 그 앞 문장의
    // 줄에 넣었다 · 10-03).
    if let Some(l) = out.get_mut(start) {
        l.blank_before = blank;
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
    /// 지금 자리의 주석 토큰들을 전부 처리(같은 줄 = 인라인 · 아니면 독립 줄) — 키워드를 엿보기 전에 부른다.
    fn drain_comments(&mut self, indent: usize) {
        while self.peek().is_some_and(|t| t.is_comment()) {
            let t = self.next().expect("tok");
            self.comment(t, indent);
        }
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
            // ★ 독립 주석(집합 연산자 앞뒤 · 구분행)은 먼저 제 줄로 — `peek_at`은 주석을 건너뛰지만 `next`는 아니라서 주석 토큰이
            //   SELECT 자리로 소비되어 목록이 한 줄로 무너졌다(kiros33 구분행 재포맷 · 09-30).
            self.drain_comments(indent);
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

    /// 지금 자리(열 목록의 시작)의 SELECT가 `INTO :변수`(바인드 대상)를 갖는가 — 같은 깊이에서 FROM·`;`·닫는 괄호 전까지 본다.
    fn select_into_vars(&self) -> bool {
        let mut depth = 0i32;
        let mut j = self.i;
        while let Some(t) = self.toks.get(j) {
            j += 1;
            if t.is_punct("(") {
                depth += 1;
            } else if t.is_punct(")") {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            } else if depth == 0 {
                if t.is_punct(";") || t.is("FROM") {
                    return false;
                }
                if t.is("INTO") {
                    return self.toks[j..]
                        .iter()
                        .find(|n| !n.is_comment())
                        .is_some_and(|n| n.kind == Kind::Bind);
                }
            }
        }
        false
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
        // ★ `SELECT … INTO :변수`(= `EXEC` 본문 · 10-03)에는 별칭을 만들어 붙이지 않는다 — 실행기가 방언별로 `@V = 식` ·
        //   `식 AS "V"`로 다시 쓰므로 붙인 별칭이 문장을 깨뜨린다(이미 있는 별칭은 그대로).
        let alias_all = self.opts.column_alias_all && !self.select_into_vars();
        let col_as = self.opts.column_as;
        self.list_items(indent + 1, Role::Item, move |w, ind| {
            w.expr_item(ind, ItemKind::Select { alias_all, col_as });
        });
        // 절.
        loop {
            // 절 사이 독립 주석(같은 결함 · 위 `query` 참조).
            self.drain_comments(indent);
            let Some(t) = self.peek_at(0) else { return };
            let up = t.up();
            if t.is_punct(")") || t.is_punct(";") {
                return;
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
                // BETWEEN도 연산자 조각(사용자 09-30 "BETWEEN 앞 공백 · 4자 이상 = 왼쪽 간격 유지 · 오른쪽 공백 1개") —
                //   `BETWEEN a AND b`의 AND는 `between` 계수로 조건 연산어와 구분한다.
                // 표 `WORD_OPS`(긴 구절 우선 · 사용자 09-30): 단항 후위 · 이항 · BETWEEN · 방언 확장(ILIKE · REGEXP · RLIKE · SIMILAR TO ·
                //   IS [NOT] DISTINCT FROM). 함수 호출(`REGEXP_LIKE(...)`)·EXISTS·불리언 컬럼은 연산자가 없어 조각도 없다(정렬·간격 대상 아님).
                let _ = &up;
                let (words, n) = WORD_OPS
                    .iter()
                    .find(|(_, ws)| self.peek_phrase(ws))
                    .map_or(("", 0), |(w, ws)| (*w, ws.len()));
                if n > 0 {
                    for _ in 0..n {
                        self.next();
                    }
                    if words.ends_with("BETWEEN") {
                        *between += 1;
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
        // ★ 윈도우 함수 `OVER (…)`(사용자 09-30 · 스킬 §8): 길면 여러 줄(`window_break`) · 짧으면 한 줄(아래 일반 괄호).
        if t.is("OVER") && self.peek().is_some_and(|n| n.is_punct("(")) {
            self.emit(t);
            if self.window_is_long() {
                self.next();
                self.window_clause(indent);
                return;
            }
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
    /// `OVER` 다음 `(`가 peek 자리 — 짝 `)`까지의 원문 글자 수가 `window_break`를 넘는가(0 = 늘 한 줄).
    fn window_is_long(&self) -> bool {
        let limit = self.opts.window_break;
        if limit == 0 {
            return false;
        }
        let Some(open) = self.toks.get(self.i) else {
            return false;
        };
        let mut depth = 0i32;
        let mut j = self.i;
        while let Some(t) = self.toks.get(j) {
            if t.is_punct("(") {
                depth += 1;
            } else if t.is_punct(")") {
                depth -= 1;
                if depth == 0 {
                    return t.span.0.saturating_sub(open.span.1) > limit;
                }
            }
            j += 1;
        }
        false
    }

    /// `OVER (`를 소비한 뒤: PARTITION BY · ORDER BY · ROWS/RANGE/GROUPS 프레임을 각 줄(한 단계 안)에 · `)`는 항목 들여쓰기 줄에
    /// (뒤에 AS 별칭이 이어진다 · 스킬 §8).
    fn window_clause(&mut self, indent: usize) {
        self.word("(");
        let inner = indent + 1;
        let mut opened = false;
        loop {
            let Some(t) = self.peek() else { return };
            if t.is_punct(")") {
                self.next();
                self.open(indent, Role::Item);
                self.word(")");
                return;
            }
            let up = t.up();
            let starts_clause = t.kind == Kind::Word
                && (matches!(up.as_str(), "ROWS" | "RANGE" | "GROUPS")
                    || (up == "PARTITION" && self.peek_phrase(&["PARTITION", "BY"]))
                    || (up == "ORDER" && self.peek_phrase(&["ORDER", "BY"])));
            if starts_clause {
                self.open(inner, Role::Other);
                opened = true;
            } else if !opened {
                // 절 키워드 없이 시작하는 내용(드묾) = 첫 줄로.
                self.open(inner, Role::Other);
                opened = true;
            }
            let t = self.next().expect("tok");
            if t.is_comment() {
                self.comment(t, inner);
            } else if t.is_punct("(") {
                self.paren_open_at(inner);
            } else if t.is_punct(",") {
                self.word(",");
            } else {
                self.emit(t);
            }
        }
    }

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
    // `<=>`(MySQL null-safe) · `^=`/`~=`(Oracle 부등)도 이항 비교(사용자 09-30 특수 연산자 정리).
    matches!(
        op,
        "=" | "<>" | "!=" | "<" | ">" | "<=" | ">=" | "<=>" | "^=" | "~="
    )
}

/// ★ 단어 비교 연산자 표(사용자 09-30 · 긴 구절이 먼저 — `IS NOT NULL`이 `IS NOT`보다 앞). (조각 글, 토큰 열).
///   단항 후위 = `IS [NOT] NULL|TRUE|FALSE|UNKNOWN` · BETWEEN = `[NOT] BETWEEN` · 나머지 = 이항(`IS [NOT] DISTINCT FROM` · `[NOT] IN/LIKE/ILIKE/
///   REGEXP/RLIKE/SIMILAR TO` · `IS [NOT]` · `IN` · `IS`). 함수 조건·EXISTS·불리언 컬럼은 여기 없다(연산자 없음).
const WORD_OPS: &[(&str, &[&str])] = &[
    ("IS NOT DISTINCT FROM", &["IS", "NOT", "DISTINCT", "FROM"]),
    ("IS DISTINCT FROM", &["IS", "DISTINCT", "FROM"]),
    ("IS NOT NULL", &["IS", "NOT", "NULL"]),
    ("IS NOT TRUE", &["IS", "NOT", "TRUE"]),
    ("IS NOT FALSE", &["IS", "NOT", "FALSE"]),
    ("IS NOT UNKNOWN", &["IS", "NOT", "UNKNOWN"]),
    ("IS NULL", &["IS", "NULL"]),
    ("IS TRUE", &["IS", "TRUE"]),
    ("IS FALSE", &["IS", "FALSE"]),
    ("IS UNKNOWN", &["IS", "UNKNOWN"]),
    ("NOT SIMILAR TO", &["NOT", "SIMILAR", "TO"]),
    ("SIMILAR TO", &["SIMILAR", "TO"]),
    ("NOT BETWEEN", &["NOT", "BETWEEN"]),
    ("NOT IN", &["NOT", "IN"]),
    ("NOT LIKE", &["NOT", "LIKE"]),
    ("NOT ILIKE", &["NOT", "ILIKE"]),
    ("NOT REGEXP", &["NOT", "REGEXP"]),
    ("NOT RLIKE", &["NOT", "RLIKE"]),
    ("IS NOT", &["IS", "NOT"]),
    ("BETWEEN", &["BETWEEN"]),
    ("IN", &["IN"]),
    ("IS", &["IS"]),
    ("LIKE", &["LIKE"]),
    ("ILIKE", &["ILIKE"]),
    ("REGEXP", &["REGEXP"]),
    ("RLIKE", &["RLIKE"]),
];

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

/// `prev`가 단항 `-`/`+`로 끝나는가 — 부호 앞이 비었거나(조각 첫머리 = 앞 조각이 비교 연산자) · `(` `,` · 연산자 글자 · 키워드.
fn unary_sign_at_end(prev: &str) -> bool {
    let p = prev.trim_end();
    if !(p.ends_with('-') || p.ends_with('+')) {
        return false;
    }
    let before = p[..p.len() - 1].trim_end();
    if before.is_empty() {
        return true;
    }
    let last = before.chars().last().unwrap_or(' ');
    if matches!(
        last,
        '(' | ',' | '=' | '<' | '>' | '+' | '-' | '*' | '/' | '|' | '!' | '^' | '~'
    ) {
        return true;
    }
    let word = before
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
        .next()
        .unwrap_or("");
    !word.is_empty() && is_keyword(word)
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
    // PostgreSQL 캐스트 `a::text`(10-03 · 종전 `a :: text`).
    if next.starts_with("::") || prev.ends_with("::") {
        return false;
    }
    // 단항 부호(10-03 · 종전 `- 1`): 조각 첫머리 · `(`·`,`·연산자·키워드 뒤의 `-`/`+`는 다음 토큰에 붙인다(`a - 1`은 그대로).
    if unary_sign_at_end(prev) {
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
            comment_gap: crate::Gap::Tab,
            ..Options::default()
        }
    }

    /// 인라인 주석 앞 간격(사용자 09-30): 켬 = 공백 두 칸/탭 · 끔 = 붙여 씀.
    #[test]
    fn inline_comment_gap_options() {
        let src = "select a from t -- note\nwhere a = 1";
        let o = Options::default();
        assert!(
            format_basic(src, &o).contains("\tt  -- note\n"),
            "{}",
            format_basic(src, &o)
        );
        let o = Options {
            comment_gap: crate::Gap::Tab,
            ..Options::default()
        };
        assert!(
            format_basic(src, &o).contains("\tt\t-- note\n"),
            "{}",
            format_basic(src, &o)
        );
        let o = Options {
            comment_space: false,
            ..Options::default()
        };
        assert!(
            format_basic(src, &o).contains("\tt-- note\n"),
            "{}",
            format_basic(src, &o)
        );
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
        assert!(
            out.contains("e\tIS NOT NULL\n"),
            "IS NOT NULL = 한 연산자: {out}"
        );
        let out2 = format_basic("select 1 from t where a.del_yn is null and b = 1", &o);
        assert!(
            out2.contains("a.del_yn\tIS NULL\n"),
            "IS NULL = 한 연산자(오른쪽 공백): {out2}"
        );
        assert_eq!(format_basic(&out2, &o), out2, "idempotent");
        assert!(out.contains("f\tNOT IN (3)"), "{out}");
        // BETWEEN = 연산자 조각: 왼쪽 탭 · 오른쪽 공백 1개(4자 이상) · 안의 AND는 조건 연산어가 아니다.
        let ob = Options {
            operator_gap: crate::Gap::Tab,
            ..Options::default()
        };
        let out = format_basic(
            "select 1 from t where a.yymm between '202601' and '202612' and x not between 1 and 2 and y = 3",
            &ob,
        );
        assert!(
            out.contains("a.yymm\tBETWEEN '202601' AND '202612'\n"),
            "{out}"
        );
        assert!(out.contains("x\tNOT BETWEEN 1 AND 2\n"), "{out}");
        assert!(out.contains("y\t=\t3\n"), "{out}");
        assert_eq!(format_basic(&out, &ob), out, "idempotent");
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

    /// 윈도우 함수(사용자 09-30 · 스킬 §8): 짧으면 한 줄 · `window_break`를 넘으면 `OVER (` + PARTITION BY/ORDER BY/프레임 각 줄 + `)` 줄 · 별칭은 `)` 뒤 · 멱등.
    #[test]
    fn window_over_breaks_when_long() {
        let src = "select row_number() over (partition by a.k order by a.d) rn, sum(a.q) over (partition by a.k order by a.d rows between 2 preceding and current row) as mov3 from t a";
        let o = Options::default();
        let out = format_basic(src, &o);
        assert!(
            out.contains("OVER (PARTITION BY a.k ORDER BY a.d) rn\n"),
            "0 = 늘 한 줄: {out}"
        );
        let o = Options {
            window_break: 40,
            ..Options::default()
        };
        let out = format_basic(src, &o);
        assert!(
            out.contains("row_number() OVER (PARTITION BY a.k ORDER BY a.d) rn\n"),
            "짧은 것은 한 줄: {out}"
        );
        assert!(
            out.contains("sum(a.q) OVER (\n\t\tPARTITION BY a.k\n\t\tORDER BY a.d\n\t\tROWS BETWEEN 2 PRECEDING AND CURRENT ROW\n\t) AS mov3\n"),
            "{out}"
        );
        assert_eq!(format_basic(&out, &o), out, "idempotent");
        assert_tokens_kept(src, &o);
    }

    /// 연산자 없는 조건 + 방언 특수 연산자(사용자 09-30): 함수 호출·EXISTS·불리언 컬럼은 조각 없음(정렬·간격 대상 아님) ·
    /// `IS [NOT] DISTINCT FROM`·`ILIKE`·`REGEXP`·`SIMILAR TO`·`IS TRUE`·`<=>` — 표대로 · 멱등.
    #[test]
    fn function_conditions_and_dialect_operators() {
        let tab = Options {
            operator_gap: crate::Gap::Tab,
            ..Options::default()
        };
        let src = "select 1 from t a where regexp_like(a.nm, '^P') and exists (select 1 from d where d.k = a.k and d.x is null) and a.flag and not a.done and a.b is distinct from a.c and a.n ilike 'x%' and a.m similar to 'y' and a.t is true and a.q <=> 1";
        let out = format_basic(src, &tab);
        assert!(
            out.contains("\tregexp_like(a.nm, '^P')\n"),
            "함수 조건 = 연산자 없음: {out}"
        );
        assert!(out.contains("\tAND EXISTS (\n"), "{out}");
        assert!(
            out.contains("d.k\t=\ta.k\n") && out.contains("d.x\tIS NULL\n"),
            "서브쿼리 안도 같은 규칙: {out}"
        );
        assert!(
            out.contains("\tAND a.flag\n") && out.contains("\tAND NOT a.done\n"),
            "불리언 컬럼: {out}"
        );
        assert!(out.contains("a.b\tIS DISTINCT FROM a.c\n"), "{out}");
        assert!(out.contains("a.n\tILIKE 'x%'\n"), "{out}");
        assert!(out.contains("a.m\tSIMILAR TO 'y'\n"), "{out}");
        assert!(out.contains("a.t\tIS TRUE\n"), "단항: {out}");
        assert!(
            out.contains("a.q\t<=>\t1\n"),
            "기호 3자 = 이항 좌우 탭: {out}"
        );
        assert_eq!(format_basic(&out, &tab), out, "idempotent");
        assert_tokens_kept(src, &Options::default());
    }

    /// 연산자 부류(사용자 09-30): 단항 `IS NULL`은 왼쪽 간격만 · 이항은 좌우 · BETWEEN은 오른쪽 공백 1개 + 안의 AND 한 칸 ·
    /// 간격 공백/탭 두 경우 · 연산자 공백 끔 · 멱등.
    #[test]
    fn operator_kinds_unary_binary_between() {
        assert_eq!(op_kind("is null"), OpKind::Unary);
        assert_eq!(op_kind("IS  NOT NULL"), OpKind::Unary);
        assert_eq!(op_kind("not between"), OpKind::Between);
        assert_eq!(op_kind("IS NOT"), OpKind::Binary);
        let src = "select 1 from t where a is null and b is not null and c between 1 and 2 and d = 3 and e not in (4)";
        let tab = Options {
            operator_gap: crate::Gap::Tab,
            ..Options::default()
        };
        let out = format_basic(src, &tab);
        assert!(
            out.contains("\ta\tIS NULL\n"),
            "단항 = 왼쪽 탭 · 오른쪽 없음: {out}"
        );
        assert!(out.contains("b\tIS NOT NULL\n"), "{out}");
        assert!(
            out.contains("c\tBETWEEN 1 AND 2\n"),
            "BETWEEN = 오른쪽 공백 1개 · AND 한 칸: {out}"
        );
        assert!(out.contains("d\t=\t3\n"), "이항 = 좌우 탭: {out}");
        assert!(
            out.contains("e\tNOT IN (4)\n"),
            "4자 이상 이항 = 오른쪽 공백 1개: {out}"
        );
        assert_eq!(format_basic(&out, &tab), out, "idempotent");
        let sp = Options::default();
        let out = format_basic(src, &sp);
        assert!(
            out.contains("a IS NULL\n")
                && out.contains("c BETWEEN 1 AND 2\n")
                && out.contains("d = 3\n"),
            "{out}"
        );
        let off = Options {
            operator_spaces: false,
            ..Options::default()
        };
        let out = format_basic(src, &off);
        assert!(
            out.contains("a IS NULL\n"),
            "단항은 공백 끔과 무관(키워드 앞 한 칸): {out}"
        );
        assert!(out.contains("c BETWEEN 1 AND 2\n"), "{out}");
        assert!(out.contains("d=3\n"), "{out}");
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

    /// 집합 연산자·절 사이의 독립 주석(구분행)이 있어도 다음 SELECT 목록은 여러 줄(09-30 · 종전엔 주석이 SELECT 자리로 소비됨) · 멱등.
    #[test]
    fn standalone_comment_between_set_op_and_select_keeps_layout() {
        let o = Options::default();
        for src in [
            "select a, b from t1\n---------\nunion all\n---------\nselect c, d from t2",
            "select a, b from t1\nunion all\n-- x\nselect c, d from t2",
            "select a, b from t1\n-- before where\nwhere a = 1",
        ] {
            let out = format_basic(src, &o);
            assert!(!out.contains("SELECT c, d"), "{src}\n{out}");
            assert!(
                out.contains("SELECT\n\tc\n, d\n") || !src.contains("select c"),
                "{out}"
            );
            assert_eq!(format_basic(&out, &o), out, "idempotent: {out}");
        }
    }

    /// T-255 테이블 설명 주석(스킬 2-2): 호스트가 준 설명이 있는 물리 테이블 줄 끝에 `--\t설명` · 없는 것·서브쿼리는 없음.
    #[test]
    fn table_description_comments() {
        let o = Options {
            comma_gap: crate::Gap::Tab,
            comment_gap: crate::Gap::Tab,
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

    /// `EXEC` + `SELECT … INTO`(사용자 10-03): `EXEC` 한 줄 + 평소대로 포맷한 질의(블록 꼴) — 홀로 선 `EXEC`도 한 줄 꼴도.
    #[test]
    fn exec_select_into_formats_as_block() {
        let o = Options::default();
        let want = "EXEC\nSELECT\n\ta\n, b\nINTO\n\t:v_a\n, :v_b\nFROM\n\tt\nWHERE\n\tx = 1;\n";
        let block = format_basic("exec\nselect a, b into :v_a, :v_b from t where x = 1;", &o);
        assert_eq!(block, want, "{block}");
        let inline = format_basic("EXEC select a, b into :v_a, :v_b from t where x = 1;", &o);
        assert_eq!(inline, want, "{inline}");
        assert_eq!(format_basic(want, &o), want, "idempotent");
        // 블록의 끝 = 빈 줄 · 다음 명령 줄(`;` 없이) — 뒤 문장을 삼키지 않는다.
        let out = format_basic(
            "EXEC\nselect count(*) into :n from t\nPRINT n\n\nselect a from t;",
            &o,
        );
        assert_eq!(
            out,
            "EXEC\nSELECT\n\tcount(*)\nINTO\n\t:n\nFROM\n\tt\nPRINT n\n\nSELECT\n\ta\nFROM\n\tt;\n",
            "{out}"
        );
        // 한 줄 꼴 + `;` 없음 + 바로 다음 줄이 SQL = 블록이 그 줄을 삼키지 않게 `;`를 붙인다(다음이 명령 줄·빈 줄이면 안 붙인다).
        let out = format_basic("EXEC select 1 into :n from dual\nselect :n from dual;", &o);
        assert!(
            out.starts_with("EXEC\nSELECT\n\t1\nINTO\n\t:n\nFROM\n\tdual;\nSELECT\n"),
            "{out}"
        );
        let out = format_basic("EXEC select 1 into :n from dual\nPRINT n", &o);
        assert!(out.contains("\tdual\nPRINT n"), "{out}");
    }

    /// `SELECT … INTO :변수`에는 "모든 열에 별칭" 옵션이 별칭을 만들지 않는다(실행기 재작성이 깨진다) · 보통 SELECT는 그대로.
    #[test]
    fn select_into_vars_gets_no_auto_alias() {
        let o = Options {
            column_alias_all: true,
            column_as: crate::AliasAs::Add,
            ..Options::default()
        };
        let out = format_basic("EXEC\nselect a.x, count(*) into :v1, :v2 from t a", &o);
        assert!(
            out.starts_with("EXEC\nSELECT\n\ta.x\n, count(*)\nINTO\n"),
            "{out}"
        );
        let out = format_basic("select a.x from t a", &o);
        assert!(out.contains("a.x AS x"), "{out}");
    }

    /// 통과 블록은 **블록 끝**까지만(10-03 · 종전 = 문서 끝까지): `END;` · 단독 `/`·`GO` 줄 · 패키지 명세 `END p;` · PG `$$ … $$;` ·
    /// `END IF`/`END LOOP`는 짝이 아니다 · `BEGIN;`·`BEGIN TRANSACTION`은 블록이 아니다 — 뒤 문장은 포맷된다.
    #[test]
    fn block_passthrough_ends_at_block_end() {
        let o = Options::default();
        let src = "BEGIN :v := 1; END;\n/\nselect 1 from t;\nDECLARE\n  x NUMBER;\nBEGIN\n  IF x THEN NULL; END IF;\n  FOR r IN (SELECT 1 FROM DUAL) LOOP NULL; END LOOP;\nEND;\nselect 2 from t;\nCREATE OR REPLACE PACKAGE p IS\n  PROCEDURE a(x IN NUMBER);\nEND p;\n/\nselect 3 from t;\nBEGIN;\nselect 4 from t;\nCREATE FUNCTION f() RETURNS int AS $$ BEGIN RETURN 1; END $$ LANGUAGE plpgsql;\nselect 5 from t;\nCREATE TRIGGER tr AFTER INSERT ON t BEGIN UPDATE t SET a = CASE WHEN 1 THEN 2 END; END;\nselect 6 from t;";
        let out = format_basic(src, &o);
        for n in 1..=6 {
            assert!(
                out.contains(&format!("SELECT\n\t{n}\nFROM\n\tt;")),
                "select {n} 포맷 안 됨:\n{out}"
            );
        }
        assert!(
            out.contains("  PROCEDURE a(x IN NUMBER);\nEND p;\n/\n"),
            "{out}"
        );
        assert!(out.contains("END $$ LANGUAGE plpgsql;\n"), "{out}");
        assert_eq!(format_basic(&out, &o), out, "idempotent: {out}");
    }

    /// 단항 부호·캐스트·접두 문자열은 붙여 쓴다(10-03 · 종전 `- 1` · `a :: text` · `N 'x'`).
    #[test]
    fn unary_sign_cast_and_prefixed_strings_stay_glued() {
        let o = Options::default();
        let out = format_basic(
            "select -1 a, (-2) b, a - -1 c, b::text d, -a e, case when x then -1 else +1 end f, N'x' g, a-1 h from t where a = -1 and b in (-1, -2) and e = f -1",
            &o,
        );
        for want in [
            "\t-1 a\n",
            "(-2) b",
            "a - -1 c",
            "b::text d",
            ", -a e\n",
            "THEN -1 ELSE +1 END f",
            "N'x' g",
            "a - 1 h",
            "a = -1",
            "IN (-1, -2)",
            "e = f - 1",
        ] {
            assert!(out.contains(want), "{want:?} 없음:\n{out}");
        }
    }

    /// `EXEC` + 프로시저·패키지 호출(사용자 10-03) = 한 줄 — 여러 줄 인자 · 홀로 선 `EXEC` · SQL Server 이름 지정 인자.
    #[test]
    fn exec_call_is_one_line() {
        let o = Options::default();
        let out = format_basic("exec SP_X(:PC_RET,\n    'A',   'B'\n  , 'C'\n);", &o);
        assert_eq!(out, "EXEC SP_X(:PC_RET, 'A', 'B', 'C');\n", "{out}");
        let out = format_basic(
            "EXEC\nPKG_A.PROC_B(\n    P_NAME => 'DUAL',\n    P_RC   => :rc\n)\n;",
            &o,
        );
        assert_eq!(
            out, "EXEC PKG_A.PROC_B(P_NAME => 'DUAL', P_RC => :rc);\n",
            "{out}"
        );
        let out = format_basic("EXEC dbo.usp_x\n    @a = 1,\n    @b = N'x';", &o);
        assert_eq!(out, "EXEC dbo.usp_x @a = 1, @b = N'x';\n", "{out}");
        // 이미 한 줄 = 간격은 원문 그대로(키워드 대소문자만).
        let one = "EXEC DBMS_STATS.GATHER_TABLE_STATS(USER, 'T', CASCADE=>TRUE,  NO_INVALIDATE=>FALSE);\n";
        assert_eq!(format_basic(one, &o), one);
        assert_eq!(
            format_basic("execute pkg.p(1)\nPRINT rc", &o),
            "EXECUTE pkg.p(1)\nPRINT rc\n"
        );
        // 한 줄 호출 뒤의 다음 줄은 다른 문장.
        let out = format_basic("EXEC p(1)\nselect a from t;", &o);
        assert_eq!(out, "EXEC p(1)\nSELECT\n\ta\nFROM\n\tt;\n", "{out}");
    }

    /// `EXEC` + 그 밖(대입 · `OPEN … FOR` · 줄 주석이 낀 호출) = 원문 그대로.
    #[test]
    fn exec_other_stays_raw() {
        let o = Options::default();
        for src in [
            "EXEC\t:V_PRG_NM\t\t\t:=\t'SP_X';\n",
            "EXEC :rc := PKG.F('V$', 500)\nPRINT rc\n",
            "EXEC OPEN :rc FOR SELECT a FROM t WHERE ROWNUM <= 3\n",
            "EXEC p(1, -- 첫째\n  2);\n",
            "exec\n",
        ] {
            assert_eq!(format_basic(src, &o), src, "{src:?}");
        }
    }

    /// 스크립트 명령 줄(`PRINT` · `SET` · `:setvar` · `GO` …)은 그 줄에서 끝난다 — 뒤 문장이 포맷되고 · 명령 줄 사이에 빈 줄을
    /// 넣지 않고 · 치환 변수 `&v`는 한 토큰.
    #[test]
    fn script_command_lines_end_at_line() {
        let o = Options::default();
        let out = format_basic(
            "VARIABLE rc REFCURSOR\nEXEC :a := 1\nPRINT a\nselect a from t;\n:setvar Top 3\nselect top &Top name from t\nGO\n\nselect 1;",
            &o,
        );
        assert_eq!(
            out,
            "VARIABLE rc REFCURSOR\nEXEC :a := 1\nPRINT a\nSELECT\n\ta\nFROM\n\tt;\n:setvar Top 3\nSELECT TOP\n\t&Top name\nFROM\n\tt\nGO\n\nSELECT\n\t1;\n",
            "{out}"
        );
        // `CONNECT BY`로 시작하는 줄은 명령이 아니다(계층 질의의 절).
        let out = format_basic(
            "EXEC\nselect a into :v from t start with a = 1 connect by prior a = b",
            &o,
        );
        assert!(out.contains("\nCONNECT BY\n\tPRIOR a = b"), "{out}");
        assert_eq!(format_basic(&out, &o), out, "idempotent: {out}");
    }
}
