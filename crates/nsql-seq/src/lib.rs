//! ★ **Insert Sequence 엔진**([docs/124](../../../docs/124-insert-sequence.md) · 사용자 10-27): 다중 캐럿에 넣을 값 n개를 **표현식 한 줄**로 만든다.
//!
//! 문법(§2) = `시작[연산 증가][#반복][~형식] [x 목록 …] ["틀"]`
//! - 시작 = 정수 · `0x`/`0b`/`0o` 정수 · 소수 · 알파벳(`a` `AA` = 엑셀 열) · 글자(유니코드) · 시각(`09:30[:00]`) · 날짜(`2026-10-27`) · 목록(`[a,b,1..5]`)
//! - 연산 = `+ - * ^`(더하기 · 빼기 · 등비 · 제곱) · 증가 = 수(+ 시각·날짜 단위 `s m h d w M y`)
//! - `#k` = 값마다 k번 반복 · `~형식` = `[+][0][폭][.소수][b|o|d|x|X]` 또는 시각·날짜 패턴(`HH:mm:ss` `yyyy-MM-dd` …)
//! - `*`(또는 `x` `×`) = 곱집합(목록끼리만 · `]` 뒤에서만 곱집합 · 그 밖의 `*`는 등비) · 틀 = `"col_{}"`(`{}` 값 · `{1}` `{2}` 곱집합 항 · `{n}` 1부터 번호 · `{i}` 0부터)
//!
//! 순수 함수 · 의존 0 · 소수는 10진 정밀(이진 부동소수 오차 없음) · 오류는 [`SeqErr`](호스트가 i18n 문구로 바꾼다).

use std::fmt;

/// 기본 표현식(Placeholder · 설정 `editor.seq_default`의 기본값).
pub const DEFAULT_EXPR: &str = "1+1";

/// 범위 `a..z`·반복·폭의 상한(실수로 거대한 목록을 만들지 않게).
const MAX_RANGE: usize = 10_000;
const MAX_REPEAT: usize = 1_000;
const MAX_WIDTH: usize = 64;
const MAX_POW: u32 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeqErr {
    /// 읽을 수 없는 조각(원문).
    Parse(String),
    /// `x`의 양쪽은 목록이어야 한다.
    ProductNeedsLists,
    EmptyList,
    /// 진수 형식(b·o·x)은 정수 값에만.
    BaseNeedsInteger,
    Overflow,
    /// 범위가 상한을 넘었다(상한).
    RangeTooLarge(usize),
    /// 단위(s m h d w M y)는 시각·날짜 시작값에만.
    UnitNeedsTimeOrDate(char),
    /// 패턴 형식은 시각·날짜 시작값에만.
    PatternNeedsTimeOrDate,
    /// 반복은 1~상한.
    BadRepeat,
    /// 폭 상한.
    WidthTooLarge,
}

impl fmt::Display for SeqErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeqErr::Parse(s) => write!(f, "cannot read '{s}'"),
            SeqErr::ProductNeedsLists => write!(f, "x needs lists on both sides, e.g. [a,b]x[1,2]"),
            SeqErr::EmptyList => write!(f, "empty list"),
            SeqErr::BaseNeedsInteger => write!(f, "base format (b o x) needs an integer value"),
            SeqErr::Overflow => write!(f, "number too large"),
            SeqErr::RangeTooLarge(n) => write!(f, "range too large (max {n})"),
            SeqErr::UnitNeedsTimeOrDate(c) => write!(f, "unit {c} needs a time or date start"),
            SeqErr::PatternNeedsTimeOrDate => {
                write!(f, "pattern format needs a time or date start")
            }
            SeqErr::BadRepeat => write!(f, "repeat must be 1..{MAX_REPEAT}"),
            SeqErr::WidthTooLarge => write!(f, "width must be at most {MAX_WIDTH}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Add,
    Sub,
    Mul,
    Pow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base {
    Bin,
    Oct,
    Dec,
    Hex { upper: bool },
}

impl Base {
    fn radix(self) -> u32 {
        match self {
            Base::Bin => 2,
            Base::Oct => 8,
            Base::Dec => 10,
            Base::Hex { .. } => 16,
        }
    }
}

/// 10진 고정소수 — 값 = `mant / 10^scale`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Dec {
    mant: i128,
    scale: u32,
}

impl Dec {
    fn int(v: i128) -> Dec {
        Dec { mant: v, scale: 0 }
    }
    fn pow10(n: u32) -> Option<i128> {
        10i128.checked_pow(n)
    }
    /// 같은 자리수로 맞춘 (a, b, scale).
    fn align(a: Dec, b: Dec) -> Option<(i128, i128, u32)> {
        let s = a.scale.max(b.scale);
        let am = a.mant.checked_mul(Self::pow10(s - a.scale)?)?;
        let bm = b.mant.checked_mul(Self::pow10(s - b.scale)?)?;
        Some((am, bm, s))
    }
    fn add(a: Dec, b: Dec) -> Option<Dec> {
        let (am, bm, s) = Self::align(a, b)?;
        Some(Dec {
            mant: am.checked_add(bm)?,
            scale: s,
        })
    }
    fn sub(a: Dec, b: Dec) -> Option<Dec> {
        let (am, bm, s) = Self::align(a, b)?;
        Some(Dec {
            mant: am.checked_sub(bm)?,
            scale: s,
        })
    }
    fn mul(a: Dec, b: Dec) -> Option<Dec> {
        Some(
            Dec {
                mant: a.mant.checked_mul(b.mant)?,
                scale: a.scale.checked_add(b.scale)?,
            }
            .trimmed(),
        )
    }
    fn mul_int(a: Dec, k: i128) -> Option<Dec> {
        Some(Dec {
            mant: a.mant.checked_mul(k)?,
            scale: a.scale,
        })
    }
    /// 끝 0 정리(자리수를 줄인다).
    fn trimmed(self) -> Dec {
        let mut d = self;
        while d.scale > 0 && d.mant % 10 == 0 {
            d.mant /= 10;
            d.scale -= 1;
        }
        d
    }
    fn is_integer(self) -> bool {
        self.trimmed().scale == 0
    }
    /// 정수 부분·소수 부분 문자열(부호 없음) — `prec` = 고정 소수 자리(반올림 · 0 = 정수) · None = 끝 0 정리.
    fn to_parts(self, prec: Option<u32>) -> Option<(bool, String, String)> {
        let d = match prec {
            None => self.trimmed(),
            Some(p) if p >= self.scale => Dec {
                mant: self.mant.checked_mul(Self::pow10(p - self.scale)?)?,
                scale: p,
            },
            Some(p) => {
                // 반올림(절반은 0에서 먼 쪽).
                let div = Self::pow10(self.scale - p)?;
                let q = self.mant / div;
                let r = self.mant % div;
                let half = div / 2;
                let q = if r.abs() >= half && r != 0 {
                    q + self.mant.signum()
                } else {
                    q
                };
                Dec { mant: q, scale: p }
            }
        };
        let neg = d.mant < 0;
        let digits = d.mant.unsigned_abs().to_string();
        let s = d.scale as usize;
        let (ip, fp) = if s == 0 {
            (digits, String::new())
        } else if digits.len() > s {
            let (a, b) = digits.split_at(digits.len() - s);
            (a.to_string(), b.to_string())
        } else {
            (
                "0".to_string(),
                format!("{}{digits}", "0".repeat(s - digits.len())),
            )
        };
        Some((neg, ip, fp))
    }
}

/// 숫자 형식 `~[+][0][폭][.소수][b|o|d|x|X]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct NumFmt {
    sign: bool,
    zero: bool,
    width: usize,
    prec: Option<u32>,
    base: Option<Base>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Fmt {
    None,
    Num(NumFmt),
    /// 시각·날짜 패턴.
    Pattern(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Num {
        start: Dec,
        base: Option<Base>,
    },
    /// 엑셀 열 번호(a=1 … z=26 · aa=27) · 대문자 여부.
    Alpha {
        start: i128,
        upper: bool,
    },
    /// 유니코드 코드 포인트.
    Char {
        start: i128,
    },
    /// 자정부터 초 · 자리 수(2 = HH:mm · 3 = HH:mm:ss).
    Time {
        start: i64,
        parts: u8,
    },
    /// 1970-01-01부터 일.
    Date {
        start: i64,
    },
    /// 이미 펼친 목록.
    List(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Step {
    value: Dec,
    /// 시각·날짜 단위(s m h d w M y) · 없으면 시작값의 마지막 자리.
    unit: Option<char>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Term {
    kind: Kind,
    op: Op,
    step: Step,
    repeat: usize,
    fmt: Fmt,
}

/// 파싱된 표현식 — `values(n)`으로 값 n개.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seq {
    terms: Vec<Term>,
    template: Option<String>,
}

// ───────────────────────── 파싱 ─────────────────────────

/// 표현식 → [`Seq`]. 빈 글은 [`DEFAULT_EXPR`].
pub fn parse(expr: &str) -> Result<Seq, SeqErr> {
    let src = expr.trim();
    let src = if src.is_empty() { DEFAULT_EXPR } else { src };
    let (body, template) = split_template(src)?;
    let chunks = split_product(&body);
    let mut terms = Vec::new();
    for c in &chunks {
        terms.push(parse_term(c.trim())?);
    }
    if terms.len() > 1 && !terms.iter().all(|t| matches!(t.kind, Kind::List(_))) {
        return Err(SeqErr::ProductNeedsLists);
    }
    Ok(Seq { terms, template })
}

/// 끝의 따옴표 틀(`"…"` · `'…'`)을 뗀다 — 대괄호 밖 첫 따옴표부터 끝까지.
fn split_template(src: &str) -> Result<(String, Option<String>), SeqErr> {
    let mut depth = 0i32;
    for (i, c) in src.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            '"' | '\'' if depth <= 0 => {
                let rest = &src[i..];
                let q = c;
                let inner = rest[1..]
                    .strip_suffix(q)
                    .ok_or_else(|| SeqErr::Parse(rest.to_string()))?;
                if inner.contains(q) {
                    return Err(SeqErr::Parse(rest.to_string()));
                }
                return Ok((src[..i].trim().to_string(), Some(inner.to_string())));
            }
            _ => {}
        }
    }
    Ok((src.to_string(), None))
}

/// 곱집합 분리 — 대괄호 밖에서 **`]` 바로 뒤**(공백 허용)의 `*`/`x`/`×`만 구분자(`0x1f`·`~x`·등비 `1*2`와 안 섞인다 — 목록 항은 연산을
/// 갖지 않으므로 `]` 뒤의 `*`는 곱집합뿐 · 149차 사용자 10-28 "x 대신 * 기호").
fn split_product(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut after_close = false;
    for c in body.chars() {
        match c {
            '[' => {
                depth += 1;
                after_close = false;
                cur.push(c);
            }
            ']' => {
                depth -= 1;
                after_close = true;
                cur.push(c);
            }
            ' ' | '\t' => cur.push(c),
            '*' | 'x' | 'X' | '×' if depth <= 0 && after_close => {
                out.push(std::mem::take(&mut cur));
                after_close = false;
            }
            _ => {
                after_close = false;
                cur.push(c);
            }
        }
    }
    out.push(cur);
    out
}

fn parse_term(s: &str) -> Result<Term, SeqErr> {
    if s.is_empty() {
        return Err(SeqErr::Parse(String::new()));
    }
    // 대괄호 밖의 `~`(형식) · `#`(반복)을 뒤에서 떼어 낸다.
    let (head, fmt_s) = split_outside(s, '~');
    let (head, rep_s) = split_outside(&head, '#');
    let repeat = match rep_s {
        None => 1,
        Some(r) => {
            let r = r.trim();
            let n: usize = r.parse().map_err(|_| SeqErr::Parse(format!("#{r}")))?;
            if n == 0 || n > MAX_REPEAT {
                return Err(SeqErr::BadRepeat);
            }
            n
        }
    };
    let head: String = head.chars().filter(|c| !c.is_whitespace()).collect();
    let (kind, rest) = parse_start(&head)?;
    let (op, step) = parse_step(rest, &kind)?;
    let fmt = parse_fmt(fmt_s.as_deref(), &kind)?;
    // 목록은 지금 펼친다(반복 · 형식 포함).
    let kind = match kind {
        Kind::List(items) => {
            let mut v = Vec::new();
            for it in items {
                for _ in 0..repeat {
                    v.push(it.clone());
                }
            }
            if v.is_empty() {
                return Err(SeqErr::EmptyList);
            }
            Kind::List(v)
        }
        k => k,
    };
    Ok(Term {
        kind,
        op,
        step,
        repeat,
        fmt,
    })
}

/// 대괄호 밖 첫 `sep`에서 가른다 — (앞, 뒤).
fn split_outside(s: &str, sep: char) -> (String, Option<String>) {
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            c if c == sep && depth <= 0 => {
                return (s[..i].to_string(), Some(s[i + c.len_utf8()..].to_string()));
            }
            _ => {}
        }
    }
    (s.to_string(), None)
}

/// 시작값 토큰 → (종류, 나머지 글).
fn parse_start(s: &str) -> Result<(Kind, &str), SeqErr> {
    let b = s.as_bytes();
    // 목록.
    if s.starts_with('[') {
        let end = s.find(']').ok_or_else(|| SeqErr::Parse(s.to_string()))?;
        let items = parse_list(&s[1..end])?;
        return Ok((Kind::List(items), &s[end + 1..]));
    }
    // 날짜 yyyy-M-d.
    if let Some((days, n)) = scan_date(s) {
        return Ok((Kind::Date { start: days }, &s[n..]));
    }
    // 시각 H:mm[:ss].
    if let Some((secs, parts, n)) = scan_time(s) {
        return Ok((Kind::Time { start: secs, parts }, &s[n..]));
    }
    // 진수 정수 / 수.
    let sign_len = usize::from(b.first().is_some_and(|c| *c == b'-' || *c == b'+'));
    let rest = &s[sign_len..];
    if rest.len() >= 3
        && rest.as_bytes()[0] == b'0'
        && matches!(rest.as_bytes()[1], b'x' | b'X' | b'b' | b'B' | b'o' | b'O')
    {
        let base = match rest.as_bytes()[1] {
            b'x' => Base::Hex { upper: false },
            b'X' => Base::Hex { upper: true },
            b'b' | b'B' => Base::Bin,
            _ => Base::Oct,
        };
        let digits: usize = rest[2..]
            .chars()
            .take_while(|c| c.is_digit(base.radix()))
            .count();
        if digits > 0 {
            let lit = &rest[2..2 + digits];
            let mut v = i128::from_str_radix(lit, base.radix()).map_err(|_| SeqErr::Overflow)?;
            if b.first() == Some(&b'-') {
                v = -v;
            }
            return Ok((
                Kind::Num {
                    start: Dec::int(v),
                    base: Some(base),
                },
                &s[sign_len + 2 + digits..],
            ));
        }
    }
    if let Some((d, n)) = scan_decimal(s) {
        return Ok((
            Kind::Num {
                start: d,
                base: None,
            },
            &s[n..],
        ));
    }
    // 알파벳(영문자만 이어진 머리).
    let letters: usize = s.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    if letters > 0 {
        let word = &s[..letters];
        let upper = word.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        return Ok((
            Kind::Alpha {
                start: alpha_to_num(word),
                upper,
            },
            &s[letters..],
        ));
    }
    // 글자 하나(유니코드).
    let c = s
        .chars()
        .next()
        .ok_or_else(|| SeqErr::Parse(s.to_string()))?;
    if matches!(c, '+' | '-' | '*' | '^' | '~' | '#' | ']' | ',') {
        return Err(SeqErr::Parse(s.to_string()));
    }
    Ok((
        Kind::Char {
            start: i128::from(u32::from(c)),
        },
        &s[c.len_utf8()..],
    ))
}

/// `[+-]?\d+(\.\d+)?` → 고정소수 · 글자 수.
fn scan_decimal(s: &str) -> Option<(Dec, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    let neg = match b.first() {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == int_start {
        return None;
    }
    let mut scale = 0u32;
    let mut digits = s[int_start..i].to_string();
    if i < b.len() && b[i] == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
        let fs = i + 1;
        let mut j = fs;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        scale = (j - fs) as u32;
        digits.push_str(&s[fs..j]);
        i = j;
    }
    let mut mant: i128 = digits.parse().ok()?;
    if neg {
        mant = -mant;
    }
    Some((Dec { mant, scale }, i))
}

/// `H:mm[:ss]` → (초, 자리 수, 글자 수).
fn scan_time(s: &str) -> Option<(i64, u8, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut parts: Vec<i64> = Vec::new();
    loop {
        let st = i;
        while i < b.len() && b[i].is_ascii_digit() && i - st < 2 {
            i += 1;
        }
        if i == st {
            return None;
        }
        parts.push(s[st..i].parse().ok()?);
        if i < b.len() && b[i] == b':' && parts.len() < 3 {
            i += 1;
            continue;
        }
        break;
    }
    if parts.len() < 2 || parts[1] > 59 || parts[0] > 23 || parts.get(2).is_some_and(|x| *x > 59) {
        return None;
    }
    let secs = parts[0] * 3600 + parts[1] * 60 + parts.get(2).copied().unwrap_or(0);
    Some((secs, parts.len() as u8, i))
}

/// `yyyy-M-d` → (1970-01-01부터 일, 글자 수).
fn scan_date(s: &str) -> Option<(i64, usize)> {
    let b = s.as_bytes();
    if b.len() < 8 || !b[..4].iter().all(u8::is_ascii_digit) || b[4] != b'-' {
        return None;
    }
    let y: i64 = s[..4].parse().ok()?;
    let mut i = 5;
    let ms = i;
    while i < b.len() && b[i].is_ascii_digit() && i - ms < 2 {
        i += 1;
    }
    if i == ms || i >= b.len() || b[i] != b'-' {
        return None;
    }
    let m: i64 = s[ms..i].parse().ok()?;
    i += 1;
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() && i - ds < 2 {
        i += 1;
    }
    if i == ds {
        return None;
    }
    let d: i64 = s[ds..i].parse().ok()?;
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    Some((days_from_civil(y, m, d), i))
}

/// 목록 안 원소들: `a` · `1..5` · `1..10+2` · `a..e` · 글자 범위.
fn parse_list(inner: &str) -> Result<Vec<String>, SeqErr> {
    let mut out = Vec::new();
    for raw in inner.split(',') {
        let it = raw.trim();
        if it.is_empty() {
            continue;
        }
        if let Some((a, b)) = it.split_once("..") {
            let (a, b) = (a.trim(), b.trim());
            // 범위 끝의 증가 `+k`.
            let (b, step) = match b.rfind('+') {
                Some(p) if p > 0 => (
                    b[..p].trim(),
                    b[p + 1..]
                        .trim()
                        .parse::<i128>()
                        .map_err(|_| SeqErr::Parse(it.to_string()))?,
                ),
                _ => (b, 1),
            };
            if step <= 0 {
                return Err(SeqErr::Parse(it.to_string()));
            }
            expand_range(a, b, step, &mut out)?;
        } else {
            out.push(it.to_string());
        }
    }
    Ok(out)
}

fn expand_range(a: &str, b: &str, step: i128, out: &mut Vec<String>) -> Result<(), SeqErr> {
    let push_checked = |out: &mut Vec<String>, s: String| -> Result<(), SeqErr> {
        if out.len() >= MAX_RANGE {
            return Err(SeqErr::RangeTooLarge(MAX_RANGE));
        }
        out.push(s);
        Ok(())
    };
    if let (Some((da, na)), Some((db, nb))) = (scan_decimal(a), scan_decimal(b)) {
        if na == a.len() && nb == b.len() && da.is_integer() && db.is_integer() {
            let (lo, hi) = (da.trimmed().mant, db.trimmed().mant);
            let mut v = lo;
            while if lo <= hi { v <= hi } else { v >= hi } {
                push_checked(out, v.to_string())?;
                v = if lo <= hi { v + step } else { v - step };
            }
            return Ok(());
        }
    }
    let alpha = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic());
    if alpha(a) && alpha(b) {
        let upper = a.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        let (lo, hi) = (alpha_to_num(a), alpha_to_num(b));
        let mut v = lo;
        while if lo <= hi { v <= hi } else { v >= hi } {
            push_checked(out, num_to_alpha(v, upper))?;
            v = if lo <= hi { v + step } else { v - step };
        }
        return Ok(());
    }
    let (ca, cb) = (a.chars().collect::<Vec<_>>(), b.chars().collect::<Vec<_>>());
    if ca.len() == 1 && cb.len() == 1 {
        let (lo, hi) = (i128::from(u32::from(ca[0])), i128::from(u32::from(cb[0])));
        let mut v = lo;
        while if lo <= hi { v <= hi } else { v >= hi } {
            if let Some(c) = u32::try_from(v).ok().and_then(char::from_u32) {
                push_checked(out, c.to_string())?;
            }
            v = if lo <= hi { v + step } else { v - step };
        }
        return Ok(());
    }
    Err(SeqErr::Parse(format!("{a}..{b}")))
}

/// 나머지 글 = `[+-*^]증가[단위]` 또는 빈 글.
fn parse_step(rest: &str, kind: &Kind) -> Result<(Op, Step), SeqErr> {
    if rest.is_empty() {
        return Ok((
            Op::Add,
            Step {
                value: Dec::int(1),
                unit: None,
            },
        ));
    }
    let mut chars = rest.chars();
    let op = match chars.next() {
        Some('+') => Op::Add,
        Some('-') => Op::Sub,
        Some('*') => Op::Mul,
        Some('^') => Op::Pow,
        _ => return Err(SeqErr::Parse(rest.to_string())),
    };
    let num = chars.as_str();
    let (value, n) = scan_decimal(num).ok_or_else(|| SeqErr::Parse(rest.to_string()))?;
    let tail = &num[n..];
    let unit = match tail {
        "" => None,
        "s" | "m" | "h" | "d" | "w" | "M" | "y" => tail.chars().next(),
        _ => return Err(SeqErr::Parse(rest.to_string())),
    };
    if let Some(u) = unit {
        let ok = match kind {
            Kind::Time { .. } => matches!(u, 's' | 'm' | 'h'),
            Kind::Date { .. } => matches!(u, 'd' | 'w' | 'M' | 'y'),
            _ => false,
        };
        if !ok {
            return Err(SeqErr::UnitNeedsTimeOrDate(u));
        }
    }
    if matches!(kind, Kind::List(_)) && op != Op::Add {
        return Err(SeqErr::Parse(rest.to_string()));
    }
    Ok((op, Step { value, unit }))
}

fn parse_fmt(spec: Option<&str>, kind: &Kind) -> Result<Fmt, SeqErr> {
    let Some(spec) = spec else {
        return Ok(Fmt::None);
    };
    let spec = spec.trim();
    if spec.is_empty() {
        return Ok(Fmt::None);
    }
    // 숫자 형식 `[+][0][폭][.소수][b|o|d|x|X]`.
    let mut f = NumFmt::default();
    let mut s = spec;
    let mut numeric = true;
    if let Some(r) = s.strip_prefix('+') {
        f.sign = true;
        s = r;
    }
    if let Some(r) = s.strip_prefix('0') {
        if r.starts_with(|c: char| c.is_ascii_digit()) {
            f.zero = true;
            s = r;
        }
    }
    let w: usize = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if w > 0 {
        f.width = s[..w].parse().map_err(|_| SeqErr::WidthTooLarge)?;
        s = &s[w..];
    }
    if let Some(r) = s.strip_prefix('.') {
        let p: usize = r.chars().take_while(|c| c.is_ascii_digit()).count();
        if p == 0 {
            numeric = false;
        } else {
            f.prec = Some(r[..p].parse().map_err(|_| SeqErr::WidthTooLarge)?);
            s = &r[p..];
        }
    }
    if numeric {
        match s {
            "" => {}
            "b" => f.base = Some(Base::Bin),
            "o" => f.base = Some(Base::Oct),
            "d" => f.base = Some(Base::Dec),
            "x" => f.base = Some(Base::Hex { upper: false }),
            "X" => f.base = Some(Base::Hex { upper: true }),
            _ => numeric = false,
        }
    }
    if numeric {
        if f.width > MAX_WIDTH {
            return Err(SeqErr::WidthTooLarge);
        }
        if f.prec.is_some_and(|p| p as usize > MAX_WIDTH) {
            return Err(SeqErr::WidthTooLarge);
        }
        return Ok(Fmt::Num(f));
    }
    match kind {
        Kind::Time { .. } | Kind::Date { .. } => Ok(Fmt::Pattern(spec.to_string())),
        _ => Err(SeqErr::PatternNeedsTimeOrDate),
    }
}

// ───────────────────────── 생성 ─────────────────────────

impl Seq {
    /// 값 n개(캐럿 순). 오류(넘침 등)는 그 자리에 빈 글이 아니라 `Err`(호스트 = 전부 공백).
    pub fn values(&self, n: usize) -> Result<Vec<String>, SeqErr> {
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            out.push(self.value_at(i)?);
        }
        Ok(out)
    }

    fn value_at(&self, i: usize) -> Result<String, SeqErr> {
        let parts: Vec<String> = if self.terms.len() > 1 {
            // 곱집합(왼쪽이 바깥) · 전체 조합 수로 순환.
            let lens: Vec<usize> = self
                .terms
                .iter()
                .map(|t| match &t.kind {
                    Kind::List(v) => v.len(),
                    _ => 1,
                })
                .collect();
            let total: usize = lens.iter().product::<usize>().max(1);
            let mut k = i % total;
            let mut idx = vec![0usize; lens.len()];
            for j in (0..lens.len()).rev() {
                idx[j] = k % lens[j];
                k /= lens[j];
            }
            self.terms
                .iter()
                .zip(idx)
                .map(|(t, j)| match &t.kind {
                    Kind::List(v) => v[j].clone(),
                    _ => String::new(),
                })
                .collect()
        } else {
            vec![self.terms[0].value_at(i)?]
        };
        let joined = parts.concat();
        Ok(match &self.template {
            None => joined,
            Some(t) => apply_template(t, &joined, &parts, i),
        })
    }
}

/// 틀 치환: `{}` 값 · `{1}`… 곱집합 항 · `{n}` 1부터 · `{i}` 0부터 · `{{` `}}` 글자.
fn apply_template(t: &str, joined: &str, parts: &[String], i: usize) -> String {
    let mut out = String::new();
    let mut rest = t;
    while let Some(p) = rest.find('{') {
        out.push_str(&rest[..p]);
        let after = &rest[p + 1..];
        if let Some(a) = after.strip_prefix('{') {
            out.push('{');
            rest = a;
            continue;
        }
        let Some(e) = after.find('}') else {
            out.push('{');
            rest = after;
            continue;
        };
        let key = &after[..e];
        match key {
            "" => out.push_str(joined),
            "n" => out.push_str(&(i + 1).to_string()),
            "i" => out.push_str(&i.to_string()),
            k => match k.parse::<usize>() {
                Ok(j) if j >= 1 && j <= parts.len() => out.push_str(&parts[j - 1]),
                _ => {
                    out.push('{');
                    out.push_str(key);
                    out.push('}');
                }
            },
        }
        rest = &after[e + 1..];
    }
    out.push_str(&rest.replace("}}", "}"));
    out
}

impl Term {
    fn value_at(&self, i: usize) -> Result<String, SeqErr> {
        let j = (i / self.repeat) as i128;
        match &self.kind {
            Kind::List(v) => Ok(v[i % v.len()].clone()),
            Kind::Num { start, base } => {
                let v = progress(*start, self.op, self.step.value, j)?;
                let mut f = match self.fmt {
                    Fmt::Num(f) => f,
                    _ => NumFmt::default(),
                };
                if f.base.is_none() {
                    f.base = *base;
                }
                format_num(v, f)
            }
            Kind::Alpha { start, upper } => {
                let v = progress(Dec::int(*start), self.op, self.step.value, j)?;
                let n = v.trimmed();
                if n.scale != 0 || n.mant < 1 {
                    return Ok(String::new());
                }
                Ok(pad_text(&num_to_alpha(n.mant, *upper), &self.fmt))
            }
            Kind::Char { start } => {
                let v = progress(Dec::int(*start), self.op, self.step.value, j)?.trimmed();
                let c = u32::try_from(v.mant).ok().and_then(char::from_u32);
                Ok(pad_text(
                    &c.filter(|_| v.scale == 0)
                        .map(String::from)
                        .unwrap_or_default(),
                    &self.fmt,
                ))
            }
            Kind::Time { start, parts } => {
                let unit_secs: i64 = match self.step.unit {
                    Some('h') => 3600,
                    Some('m') => 60,
                    Some('s') => 1,
                    _ => {
                        if *parts >= 3 {
                            1
                        } else {
                            60
                        }
                    }
                };
                let step = self.step.value.trimmed();
                let delta = (step.mant as i64)
                    .checked_mul(unit_secs)
                    .ok_or(SeqErr::Overflow)?
                    / 10i64.pow(step.scale);
                let v = match self.op {
                    Op::Add => start + delta * j as i64,
                    Op::Sub => start - delta * j as i64,
                    _ => return Err(SeqErr::Parse("* ^".into())),
                };
                let secs = v.rem_euclid(86_400);
                let pat = match &self.fmt {
                    Fmt::Pattern(p) => p.clone(),
                    _ => {
                        if *parts >= 3 {
                            "HH:mm:ss".to_string()
                        } else {
                            "HH:mm".to_string()
                        }
                    }
                };
                Ok(format_time(secs, &pat))
            }
            Kind::Date { start } => {
                let step = self.step.value.trimmed();
                if step.scale != 0 {
                    return Err(SeqErr::Parse(format!("{}", step.mant)));
                }
                let k = match self.op {
                    Op::Add => step.mant * j,
                    Op::Sub => -step.mant * j,
                    _ => return Err(SeqErr::Parse("* ^".into())),
                } as i64;
                let days = match self.step.unit {
                    Some('w') => start + k * 7,
                    Some('M') => add_months(*start, k),
                    Some('y') => add_months(*start, k * 12),
                    _ => start + k,
                };
                let pat = match &self.fmt {
                    Fmt::Pattern(p) => p.clone(),
                    _ => "yyyy-MM-dd".to_string(),
                };
                Ok(format_date(days, &pat))
            }
        }
    }
}

/// j번째 값: `+` start + j·step · `-` start − j·step · `*` start·step^j · `^` (start + j)^step.
fn progress(start: Dec, op: Op, step: Dec, j: i128) -> Result<Dec, SeqErr> {
    let r = match op {
        Op::Add => Dec::add(start, Dec::mul_int(step, j).ok_or(SeqErr::Overflow)?),
        Op::Sub => Dec::sub(start, Dec::mul_int(step, j).ok_or(SeqErr::Overflow)?),
        Op::Mul => {
            let mut v = start;
            for _ in 0..j {
                v = Dec::mul(v, step).ok_or(SeqErr::Overflow)?;
            }
            Some(v)
        }
        Op::Pow => {
            let p = step.trimmed();
            if p.scale != 0 || p.mant < 0 || p.mant > i128::from(MAX_POW) {
                return Err(SeqErr::Parse(format!("^{}", p.mant)));
            }
            let b = Dec::add(start, Dec::int(j)).ok_or(SeqErr::Overflow)?;
            let mut v = Dec::int(1);
            for _ in 0..p.mant {
                v = Dec::mul(v, b).ok_or(SeqErr::Overflow)?;
            }
            Some(v)
        }
    };
    r.ok_or(SeqErr::Overflow)
}

fn format_num(v: Dec, f: NumFmt) -> Result<String, SeqErr> {
    let body = match f.base {
        Some(b) if b != Base::Dec => {
            if !v.is_integer() {
                return Err(SeqErr::BaseNeedsInteger);
            }
            let m = v.trimmed().mant;
            let digits = to_radix(m.unsigned_abs(), b.radix());
            let digits = match b {
                Base::Hex { upper: true } => digits.to_ascii_uppercase(),
                _ => digits,
            };
            (m < 0, digits)
        }
        _ => {
            let (neg, ip, fp) = v.to_parts(f.prec).ok_or(SeqErr::Overflow)?;
            (
                neg,
                if fp.is_empty() {
                    ip
                } else {
                    format!("{ip}.{fp}")
                },
            )
        }
    };
    let (neg, digits) = body;
    let sign = if neg {
        "-"
    } else if f.sign {
        "+"
    } else {
        ""
    };
    let len = sign.len() + digits.chars().count();
    if f.width > len {
        let pad = f.width - len;
        if f.zero {
            Ok(format!("{sign}{}{digits}", "0".repeat(pad)))
        } else {
            Ok(format!("{}{sign}{digits}", " ".repeat(pad)))
        }
    } else {
        Ok(format!("{sign}{digits}"))
    }
}

fn to_radix(mut v: u128, radix: u32) -> String {
    if v == 0 {
        return "0".into();
    }
    let mut s = Vec::new();
    while v > 0 {
        let d = (v % u128::from(radix)) as u32;
        s.push(char::from_digit(d, radix).unwrap_or('?'));
        v /= u128::from(radix);
    }
    s.iter().rev().collect()
}

/// 글(알파벳·글자)의 형식 = 폭만(공백 왼쪽 채움).
fn pad_text(s: &str, fmt: &Fmt) -> String {
    match fmt {
        Fmt::Num(f) if f.width > s.chars().count() => {
            format!("{}{s}", " ".repeat(f.width - s.chars().count()))
        }
        _ => s.to_string(),
    }
}

/// 엑셀 열 번호(a=1 … z=26 · aa=27 · 대소문자 무관).
fn alpha_to_num(w: &str) -> i128 {
    let mut n: i128 = 0;
    for c in w.chars() {
        n = n * 26 + i128::from(c.to_ascii_lowercase() as u32 - 'a' as u32 + 1);
    }
    n
}

fn num_to_alpha(mut n: i128, upper: bool) -> String {
    if n < 1 {
        return String::new();
    }
    let mut s = Vec::new();
    while n > 0 {
        n -= 1;
        let c = (b'a' + (n % 26) as u8) as char;
        s.push(if upper { c.to_ascii_uppercase() } else { c });
        n /= 26;
    }
    s.iter().rev().collect()
}

// ───────────────────────── 시각 · 날짜 ─────────────────────────

fn format_time(secs: i64, pat: &str) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    let mut out = String::new();
    let cs: Vec<char> = pat.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let run = |c: char| cs[i..].iter().take_while(|&&x| x == c).count();
        match cs[i] {
            'H' => {
                let n = run('H');
                out.push_str(&pad_num(h, n));
                i += n;
            }
            'm' => {
                let n = run('m');
                out.push_str(&pad_num(m, n));
                i += n;
            }
            's' => {
                let n = run('s');
                out.push_str(&pad_num(s, n));
                i += n;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn format_date(days: i64, pat: &str) -> String {
    let (y, m, d) = civil_from_days(days);
    let mut out = String::new();
    let cs: Vec<char> = pat.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let run = |c: char| cs[i..].iter().take_while(|&&x| x == c).count();
        match cs[i] {
            'y' => {
                let n = run('y');
                if n <= 2 {
                    out.push_str(&pad_num(y.rem_euclid(100), 2));
                } else {
                    out.push_str(&pad_num(y, 4));
                }
                i += n;
            }
            'M' => {
                let n = run('M');
                out.push_str(&pad_num(m, n));
                i += n;
            }
            'd' => {
                let n = run('d');
                out.push_str(&pad_num(d, n));
                i += n;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn pad_num(v: i64, width: usize) -> String {
    let s = v.to_string();
    if s.len() < width {
        format!("{}{s}", "0".repeat(width - s.len()))
    } else {
        s
    }
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// 달 더하기(일은 그 달의 마지막 날로 자른다 · 1-31 + 1M = 2-28).
fn add_months(days: i64, k: i64) -> i64 {
    let (y, m, d) = civil_from_days(days);
    let total = y * 12 + (m - 1) + k;
    let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    days_from_civil(ny, nm, d.min(days_in_month(ny, nm)))
}

/// Howard Hinnant의 civil ↔ days(1970-01-01 = 0).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// ───────────────────────── 도움말 ─────────────────────────

/// 치트 시트(표현식 · 영어 · 한국어) — 팔레트 `?` 행 · docs/124 §2 표와 같은 순서.
pub const HELP: &[(&str, &str, &str)] = &[
    ("1+1", "start + step (default)", "시작+증가(기본)"),
    ("10-2", "down by 2", "2씩 내려가기"),
    ("1*2", "geometric (1 2 4 8)", "등비(1 2 4 8)"),
    ("1^2", "powers (1 4 9 16)", "제곱(1 4 9 16)"),
    ("1+1#2", "repeat each value twice", "값마다 2번 반복"),
    ("1+1~03", "zero-padded width 3", "0 채움 3자리"),
    ("1+1~3", "right-aligned width 3", "공백 채움 3자리"),
    ("-1+1~+", "always show sign", "부호 표시"),
    (
        "250+1~x",
        "hex (~X upper · ~b bin · ~o oct · ~04b)",
        "16진수(~X 대문자 · ~b 2진 · ~o 8진 · ~04b)",
    ),
    (
        "0x1f+1",
        "hex start keeps hex output (~d = decimal)",
        "진수 시작값 = 그 진수로 출력(~d = 10진)",
    ),
    (
        "0.2+0.02",
        "exact decimals (0.28 → 0.3) · ~.2 fixed",
        "정밀 소수(0.28 → 0.3) · ~.2 고정",
    ),
    (
        "a+1",
        "letters a b … z aa (A+1 upper · aa+1)",
        "알파벳 a b … z aa(A+1 대문자 · aa+1)",
    ),
    (
        "가+1",
        "any character by code point (① Ⅰ …)",
        "유니코드 순 글자(① Ⅰ …)",
    ),
    (
        "09:00+15",
        "time, step = last unit (+90s +1h ~mm:ss)",
        "시각 · 증가 = 마지막 자리(+90s +1h ~mm:ss)",
    ),
    (
        "2026-10-27+1",
        "date by day (+1w +1M +1y ~yyyy/MM/dd)",
        "날짜 일 단위(+1w +1M +1y ~yyyy/MM/dd)",
    ),
    ("[월,화,수]", "cycle a list", "목록 순환"),
    (
        "[1..3] [a..c] [0..20+5]",
        "ranges inside a list",
        "목록 안 범위",
    ),
    (
        "[A,B]*[1,2]",
        "product A1 A2 B1 B2 (x also works)",
        "곱집합 A1 A2 B1 B2(x도 됨)",
    ),
    (
        "[A,B]*[1..3] \"{1}-{2}\"",
        "template with product parts",
        "틀 + 곱집합 항",
    ),
    (
        "1+1 \"col_{}\"",
        "template: {} value · {n} 1-based · {i} 0-based",
        "틀: {} 값 · {n} 1부터 · {i} 0부터",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 149차: 곱집합 구분자 `*`(사용자 10-28) — `]` 뒤에서만 곱집합 · 숫자 항의 `*`는 등비 그대로 · `x`·`×`도 계속 · 목록이 아닌 쪽은 오류.
    #[test]
    fn star_is_product_after_list_and_geometric_elsewhere() {
        let v = parse("[A,B]*[1,2]")
            .expect("parse")
            .values(5)
            .expect("values");
        assert_eq!(v, vec!["A1", "A2", "B1", "B2", "A1"]);
        assert_eq!(
            parse("[A,B] * [1..3] \"{1}-{2}\"")
                .expect("parse")
                .values(2)
                .expect("values"),
            vec!["A-1", "A-2"]
        );
        assert_eq!(
            parse("[A,B]x[1,2]")
                .expect("parse")
                .values(2)
                .expect("values"),
            vec!["A1", "A2"]
        );
        assert_eq!(
            parse("1*2").expect("parse").values(4).expect("values"),
            vec!["1", "2", "4", "8"]
        );
        assert_eq!(parse("[1,2]*3").err(), Some(SeqErr::ProductNeedsLists));
    }

    fn v(expr: &str, n: usize) -> Vec<String> {
        parse(expr)
            .unwrap_or_else(|e| panic!("{expr}: {e}"))
            .values(n)
            .unwrap_or_else(|e| panic!("{expr}: {e}"))
    }
    fn j(expr: &str, n: usize) -> String {
        v(expr, n).join(" ")
    }

    /// docs/124 §2 표의 예 — 숫자 · 연산 · 반복 · 형식.
    #[test]
    fn numbers_ops_repeat_formats() {
        assert_eq!(j("", 5), "1 2 3 4 5");
        assert_eq!(j("1+1", 5), "1 2 3 4 5");
        assert_eq!(j("10+5", 5), "10 15 20 25 30");
        assert_eq!(j("10-2", 5), "10 8 6 4 2");
        assert_eq!(j("-3+1", 5), "-3 -2 -1 0 1");
        assert_eq!(j("1*2", 5), "1 2 4 8 16");
        assert_eq!(j("1^2", 5), "1 4 9 16 25");
        assert_eq!(j("2^3", 5), "8 27 64 125 216");
        assert_eq!(j("1+1#2", 5), "1 1 2 2 3");
        assert_eq!(j("1+1~03", 5), "001 002 003 004 005");
        assert_eq!(v("1+1~3", 2), vec!["  1", "  2"]);
        assert_eq!(j("-1+1~+", 5), "-1 +0 +1 +2 +3");
        assert_eq!(j("250+1~x", 3), "fa fb fc");
        assert_eq!(j("250+1~X", 3), "FA FB FC");
        assert_eq!(j("1+1~b", 5), "1 10 11 100 101");
        assert_eq!(j("1+1~o", 9), "1 2 3 4 5 6 7 10 11");
        assert_eq!(j("1+1~04b", 3), "0001 0010 0011");
        assert_eq!(j("0x1f+1", 3), "1f 20 21");
        assert_eq!(j("0x1f+1~d", 3), "31 32 33");
        assert_eq!(j("0b101+1", 2), "101 110");
        assert_eq!(j("1+1~x \"0x{}\"", 3), "0x1 0x2 0x3");
        assert_eq!(j("1 + 1 ~ 03", 2), "001 002", "공백 무시");
        assert_eq!(j("1+1~+03", 2), "+01 +02");
        assert_eq!(j("-5+1~03", 2), "-05 -04");
    }

    /// 소수 = 10진 정밀(0.28 → 0.3 · 오차 없음) · `~.n` 고정 · 반올림.
    #[test]
    fn decimals_are_exact() {
        assert_eq!(j("0.2+0.02", 7), "0.2 0.22 0.24 0.26 0.28 0.3 0.32");
        assert_eq!(j("0.2+0.02~.2", 6), "0.20 0.22 0.24 0.26 0.28 0.30");
        assert_eq!(j("0.1+0.1", 10), "0.1 0.2 0.3 0.4 0.5 0.6 0.7 0.8 0.9 1");
        assert_eq!(j("1+0.5~.0", 4), "1 2 2 3", "반올림(절반은 0에서 먼 쪽)");
        assert_eq!(j("1.5*2", 4), "1.5 3 6 12");
        assert_eq!(j("0.5^2", 3), "0.25 2.25 6.25");
        assert_eq!(
            parse("1.5+1~x").and_then(|s| s.values(2)),
            Err(SeqErr::BaseNeedsInteger)
        );
        assert_eq!(j("1.0+1~x", 2), "1 2", "1.0은 정수");
    }

    /// 알파벳(엑셀 열) · 유니코드 글자 · 폭.
    #[test]
    fn letters_and_chars() {
        assert_eq!(j("a+1", 5), "a b c d e");
        assert_eq!(j("A+1", 3), "A B C");
        assert_eq!(j("y+1", 4), "y z aa ab");
        assert_eq!(j("aa+1", 3), "aa ab ac");
        assert_eq!(j("Z+1", 2), "Z AA");
        assert_eq!(j("c-1", 4), "c b a ", "a 아래는 빈 글");
        assert_eq!(j("가+1", 3), "가 각 갂");
        assert_eq!(j("①+1", 3), "① ② ③");
        assert_eq!(j("Ⅰ+1", 3), "Ⅰ Ⅱ Ⅲ");
        assert_eq!(v("a+1~3", 2), vec!["  a", "  b"]);
        assert_eq!(j("a+2", 3), "a c e");
    }

    /// 시각: 증가 단위 = 마지막 자리 · 명시 단위 · 패턴 · 24시간 넘김.
    #[test]
    fn times() {
        assert_eq!(j("09:00+15", 5), "09:00 09:15 09:30 09:45 10:00");
        assert_eq!(j("09:00:00+90s", 3), "09:00:00 09:01:30 09:03:00");
        assert_eq!(j("09:00+1h", 3), "09:00 10:00 11:00");
        assert_eq!(j("23:30+30", 3), "23:30 00:00 00:30");
        assert_eq!(j("00:00+30s~mm:ss", 3), "00:00 00:30 01:00");
        assert_eq!(j("9:05+5~H:mm", 2), "9:05 9:10");
        assert_eq!(j("10:00-15", 3), "10:00 09:45 09:30");
        assert_eq!(j("09:00:00+1", 2), "09:00:00 09:00:01", "3자리 = 초");
        assert_eq!(j("09:00+1m", 2), "09:00 09:01");
        assert_eq!(parse("09:00+1d"), Err(SeqErr::UnitNeedsTimeOrDate('d')));
        assert_eq!(parse("1+1~HH:mm"), Err(SeqErr::PatternNeedsTimeOrDate));
    }

    /// 날짜: 일 · 주 · 달(말일 자름) · 해 · 패턴.
    #[test]
    fn dates() {
        assert_eq!(j("2026-10-27+1", 3), "2026-10-27 2026-10-28 2026-10-29");
        assert_eq!(j("2026-10-30+1", 3), "2026-10-30 2026-10-31 2026-11-01");
        assert_eq!(j("2026-10-27+1w", 2), "2026-10-27 2026-11-03");
        assert_eq!(j("2026-01-31+1M", 3), "2026-01-31 2026-02-28 2026-03-31");
        assert_eq!(j("2024-02-29+1y", 2), "2024-02-29 2025-02-28");
        assert_eq!(j("2026-10-27+1~yyyy/MM/dd", 2), "2026/10/27 2026/10/28");
        assert_eq!(j("2026-10-27+1~MM-dd", 2), "10-27 10-28");
        assert_eq!(j("2026-10-27+1~yyyyMMdd", 2), "20261027 20261028");
        assert_eq!(j("2026-10-27+1~yy.M.d", 2), "26.10.27 26.10.28");
        assert_eq!(j("2026-01-01-1", 2), "2026-01-01 2025-12-31");
        assert_eq!(j("2026-3-5+1", 1), "2026-03-05");
        assert!(parse("2026-13-01+1").is_err());
    }

    /// 목록 · 범위 · 곱집합 · 틀.
    #[test]
    fn lists_products_templates() {
        assert_eq!(j("[월,화,수]", 5), "월 화 수 월 화");
        assert_eq!(j("[1..3]", 5), "1 2 3 1 2");
        assert_eq!(j("[a..c]", 5), "a b c a b");
        assert_eq!(j("[0..20+5]", 5), "0 5 10 15 20");
        assert_eq!(j("[5..1]", 5), "5 4 3 2 1");
        assert_eq!(j("[A,B]x[1,2]", 5), "A1 A2 B1 B2 A1");
        assert_eq!(j("[A,B]x[1..3] \"{1}-{2}\"", 5), "A-1 A-2 A-3 B-1 B-2");
        assert_eq!(j("[A,B] x [1,2] x [x,y]", 3), "A1x A1y A2x");
        assert_eq!(j("1+1 \"col_{}\"", 3), "col_1 col_2 col_3");
        assert_eq!(j("[x,y] \"{n}. {}\"", 3), "1. x 2. y 3. x");
        assert_eq!(j("1+1 '{i}:{}'", 2), "0:1 1:2");
        assert_eq!(j("1+1 \"{{{}}}\"", 2), "{1} {2}");
        assert_eq!(j("[a,b]#2", 5), "a a b b a");
        assert_eq!(j("[가..다]", 3), "가 각 갂");
        assert!(parse("1+1x[a]").is_err(), "x 앞이 목록이 아니면 x는 글자");
        assert_eq!(parse("[a]x1+1"), Err(SeqErr::ProductNeedsLists));
        assert_eq!(parse("[]"), Err(SeqErr::EmptyList));
        assert_eq!(parse("[1..100000]"), Err(SeqErr::RangeTooLarge(MAX_RANGE)));
    }

    /// 오류 · 상한.
    #[test]
    fn errors_and_limits() {
        assert!(parse("1+").is_err());
        assert!(parse("+").is_err());
        assert!(parse("1&1").is_err());
        assert_eq!(parse("1+1#0"), Err(SeqErr::BadRepeat));
        assert_eq!(parse("1+1~99"), Err(SeqErr::WidthTooLarge));
        assert!(
            parse("1^99").and_then(|s| s.values(1)).is_err(),
            "지수 상한"
        );
        assert_eq!(
            parse("1+1 \"abc").map(|_| ()),
            Err(SeqErr::Parse("\"abc".into()))
        );
        assert_eq!(
            parse("1*10").and_then(|s| s.values(60)),
            Err(SeqErr::Overflow)
        );
    }

    /// 날짜 산술 보조.
    #[test]
    fn civil_roundtrip() {
        for (y, m, d) in [
            (1970, 1, 1),
            (2000, 2, 29),
            (2026, 10, 27),
            (1969, 12, 31),
            (2100, 3, 1),
        ] {
            let days = days_from_civil(y, m, d);
            assert_eq!(civil_from_days(days), (y, m, d));
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(alpha_to_num("aa"), 27);
        assert_eq!(num_to_alpha(27, false), "aa");
        assert_eq!(num_to_alpha(702, true), "ZZ");
        assert_eq!(num_to_alpha(703, false), "aaa");
    }
}
