//! **문서 아웃라인**(사용자 09-23 "현재 문서 기반 Outline을 캐싱하고 변수·함수 등을 빠르게 사용") — docs/76.
//!
//! 문서 하나를 문장 단위([`crate::split_script_in`])로 나누고, 각 문장에서 **이름 있는 것**을 뽑는다:
//! 문장 머리(DataGrip Structure의 DDL/DML/SELECT 문장) · `DEFINE`/`VARIABLE` · CTE(`WITH x AS (`) · `CREATE …` 대상 ·
//! PL/SQL `PROCEDURE`/`FUNCTION`/`PACKAGE [BODY]`/`TRIGGER`/`TYPE`/`CURSOR`/`<<라벨>>` · 선언부 변수(DECLARE~BEGIN 사이 · `IS`/`AS`~`BEGIN`) ·
//! T-SQL `DECLARE @v`. 문자열·주석 안은 보지 않는다([`crate::lexer::classify`]).
//!
//! 소비자: 아웃라인 패널(클릭 = 그 줄) · Goto Symbol 팔레트(Ctrl+R) · 자동 완성의 문서 심볼 후보(`intel`). 캐시 열쇠 = 본문 세대.

use crate::lexer::{classify, is_ident_char, Class};
use crate::split::{split_script_in, Item, ItemKind, SqlKind};
use nsql_core::Dialect;

/// 심볼 종류(아이콘·정렬·필터 기준).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymKind {
    /// 문장 머리(SELECT/INSERT/… · 아웃라인의 뼈대).
    Statement,
    /// `DEFINE name = …`(치환 변수).
    Define,
    /// `VARIABLE name TYPE`(바인드 변수).
    Variable,
    /// `WITH name AS (…)`.
    Cte,
    /// `CREATE TABLE/VIEW/INDEX/SEQUENCE …` 대상(종류는 `detail`).
    Object,
    Procedure,
    Function,
    Package,
    PackageBody,
    Trigger,
    Type,
    Cursor,
    /// `<<label>>`.
    Label,
    /// 선언부 변수(PL/SQL `v_x NUMBER;` · T-SQL `DECLARE @v`).
    Declared,
}

impl SymKind {
    /// 표시 라벨(영어 · UI는 이 값을 i18n 키로 바꾸거나 그대로 쓴다).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SymKind::Statement => "statement",
            SymKind::Define => "define",
            SymKind::Variable => "variable",
            SymKind::Cte => "cte",
            SymKind::Object => "object",
            SymKind::Procedure => "procedure",
            SymKind::Function => "function",
            SymKind::Package => "package",
            SymKind::PackageBody => "package body",
            SymKind::Trigger => "trigger",
            SymKind::Type => "type",
            SymKind::Cursor => "cursor",
            SymKind::Label => "label",
            SymKind::Declared => "declared",
        }
    }
}

/// 심볼 하나.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub kind: SymKind,
    pub name: String,
    /// 부가(문장 머리 요약 · 타입 · CREATE 종류).
    pub detail: String,
    /// 1 기준 줄.
    pub line: usize,
    /// 바이트 오프셋(이름 시작).
    pub byte: usize,
    /// 트리 깊이(뿌리 = 0 · 그 안의 것 = 1 · 패키지 안 서브프로그램 = 2). `parent`가 있으면 그 깊이 + 1.
    pub depth: u8,
    /// ★ 부모 심볼의 색인(`Outline::symbols` 안 · T-422 · D-357) — 문장/CREATE 대상/패키지가 뿌리 · 평면 Vec에 색인만 더해
    ///   복사 없이 트리를 이룬다(패널 접기 · 필터 때 부모 함께 보이기 · Goto Symbol 들여쓰기).
    pub parent: Option<usize>,
}

/// 문서 아웃라인.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outline {
    pub symbols: Vec<Symbol>,
}

impl Outline {
    /// 이름으로 찾기(대소문자 무시 · 문장 머리 제외).
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&Symbol> {
        let l = name.to_lowercase();
        self.symbols
            .iter()
            .find(|s| s.kind != SymKind::Statement && s.name.to_lowercase() == l)
    }

    /// 자동 완성 후보로 쓸 이름들(문장 머리 제외 · 중복 제거 · 나온 순서).
    #[must_use]
    pub fn names(&self) -> Vec<(String, SymKind)> {
        let mut out: Vec<(String, SymKind)> = Vec::new();
        for s in &self.symbols {
            if s.kind == SymKind::Statement {
                continue;
            }
            if !out.iter().any(|(n, _)| n.eq_ignore_ascii_case(&s.name)) {
                out.push((s.name.clone(), s.kind));
            }
        }
        out
    }
}

/// 코드 낱말(식별자·숫자·기호 하나) — 문자열·주석은 건너뛴다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Word<'a> {
    pub text: &'a str,
    /// 바이트 구간.
    pub start: usize,
    pub end: usize,
    /// 인용 식별자(`"x"` · `[x]`)였는가.
    pub quoted: bool,
}

/// 코드 구간의 낱말 목록 — 식별자(`@`·`:`·`&`·`$`·`#` 접두 포함) · 숫자 · 기호 1자 · 인용 식별자(따옴표 벗김).
#[must_use]
pub fn words<'a>(text: &'a str, classes: &[Class]) -> Vec<Word<'a>> {
    let b = text.as_bytes();
    let n = b.len().min(classes.len());
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        match classes[i] {
            Class::Str | Class::LineComment | Class::BlockComment => {
                let mut j = i;
                while j < n && classes[j] == classes[i] {
                    j += 1;
                }
                i = j;
            }
            Class::Ident => {
                let mut j = i;
                while j < n && classes[j] == Class::Ident {
                    j += 1;
                }
                // 따옴표/대괄호 벗김 — 닫는 글자가 ASCII 인용부호/대괄호일 때만(끝에서 잘린 인용 식별자의 마지막 바이트가
                //   다국어 글자 한가운데일 수 있다 · 이진 파일 손실 변환 본문에서 패닉하던 자리 · 사용자 09-23).
                let inner_s = i + 1;
                let closes = j > i + 1 && matches!(b[j - 1], b'"' | b'\'' | b'`' | b']');
                let inner_e = if closes { j - 1 } else { j };
                if inner_e > inner_s
                    && text.is_char_boundary(inner_s)
                    && text.is_char_boundary(inner_e)
                {
                    out.push(Word {
                        text: &text[inner_s..inner_e],
                        start: i,
                        end: j,
                        quoted: true,
                    });
                }
                i = j;
            }
            Class::Code => {
                let c = b[i];
                if c.is_ascii_whitespace() {
                    i += 1;
                    continue;
                }
                let starts_ident = is_ident_char(c)
                    || ((c == b':' || c == b'@' || c == b'&' || c == b'#' || c == b'$')
                        && i + 1 < n
                        && is_ident_char(b[i + 1]));
                if starts_ident {
                    let mut j = i + 1;
                    while j < n
                        && classes[j] == Class::Code
                        && (is_ident_char(b[j]) || b[j] == b'$' || b[j] == b'#')
                    {
                        j += 1;
                    }
                    out.push(Word {
                        text: &text[i..j],
                        start: i,
                        end: j,
                        quoted: false,
                    });
                    i = j;
                } else if c >= 0x80 {
                    // 식별자 밖의 비ASCII 글자(U+FFFD 등) = 낱말이 아니다 — 글자 경계까지 통째로 건너뛴다(바이트 단위로 자르면 패닉).
                    let mut j = i + 1;
                    while j < n && (b[j] & 0xC0) == 0x80 {
                        j += 1;
                    }
                    i = j;
                } else {
                    // 기호 1자(`<<`·`>>`는 둘씩).
                    let j = if (c == b'<' || c == b'>') && i + 1 < n && b[i + 1] == c {
                        i + 2
                    } else {
                        i + 1
                    };
                    out.push(Word {
                        text: &text[i..j],
                        start: i,
                        end: j,
                        quoted: false,
                    });
                    i = j;
                }
            }
        }
    }
    out
}

fn eq(w: &Word<'_>, kw: &str) -> bool {
    !w.quoted && w.text.eq_ignore_ascii_case(kw)
}

fn is_name(w: &Word<'_>) -> bool {
    w.quoted
        || w.text
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_' || *c == b'@')
}

/// 스키마 접두를 포함한 이름(`a.b` → `a.b`) — `words[i]`부터.
fn dotted_name(words: &[Word<'_>], i: usize) -> (String, usize) {
    let mut name = String::new();
    let mut k = i;
    while k < words.len() && is_name(&words[k]) {
        name.push_str(words[k].text);
        if k + 2 < words.len() && words[k + 1].text == "." && is_name(&words[k + 2]) {
            name.push('.');
            k += 2;
        } else {
            k += 1;
            break;
        }
    }
    (name, k)
}

const CREATE_SKIP: &[&str] = &[
    "OR",
    "REPLACE",
    "EDITIONABLE",
    "NONEDITIONABLE",
    "GLOBAL",
    "TEMPORARY",
    "TEMP",
    "UNLOGGED",
    "MATERIALIZED",
    "UNIQUE",
    "BITMAP",
    "PUBLIC",
    "FORCE",
    "NO",
    "IF",
    "NOT",
    "EXISTS",
    "CLUSTERED",
    "NONCLUSTERED",
    "PRIVATE",
];

fn create_kind(w: &str) -> Option<(SymKind, &'static str)> {
    Some(match w.to_ascii_uppercase().as_str() {
        "TABLE" => (SymKind::Object, "table"),
        "VIEW" => (SymKind::Object, "view"),
        "INDEX" => (SymKind::Object, "index"),
        "SEQUENCE" => (SymKind::Object, "sequence"),
        "SYNONYM" => (SymKind::Object, "synonym"),
        "SCHEMA" => (SymKind::Object, "schema"),
        "DATABASE" => (SymKind::Object, "database"),
        "PROCEDURE" | "PROC" => (SymKind::Procedure, "procedure"),
        "FUNCTION" => (SymKind::Function, "function"),
        "PACKAGE" => (SymKind::Package, "package"),
        "TRIGGER" => (SymKind::Trigger, "trigger"),
        "TYPE" => (SymKind::Type, "type"),
        _ => return None,
    })
}

const CLAUSE_STOP: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "GROUP",
    "ORDER",
    "HAVING",
    "INSERT",
    "UPDATE",
    "DELETE",
    "VALUES",
    "SET",
    "JOIN",
    "INNER",
    "LEFT",
    "RIGHT",
    "FULL",
    "CROSS",
    "ON",
    "UNION",
    "INTERSECT",
    "MINUS",
    "EXCEPT",
    "BEGIN",
    "END",
    "DECLARE",
    "IS",
    "AS",
    "RETURN",
    "EXCEPTION",
    "WHEN",
    "THEN",
    "LOOP",
    "IF",
    "ELSE",
    "ELSIF",
    "FOR",
    "WHILE",
    "INTO",
    "MERGE",
    "USING",
    "WITH",
    "LIMIT",
    "FETCH",
    "OFFSET",
];

fn line_of(src: &str, byte: usize) -> usize {
    src.as_bytes()[..byte.min(src.len())]
        .iter()
        .filter(|&&c| c == b'\n')
        .count()
        + 1
}

/// 문장 머리 요약(첫 60자 · 공백 접기).
/// `DEFINE 이름` 뒤가 층 낱말 하나(`SHARE`·`SHARED`·`LOCAL`·`TAB`·`UNSHARE`·`GLOBAL`)로 끝나는가 — command.rs의 층 이동 판정과 같은 낱말(T-434).
fn is_layer_word(rest: &str) -> bool {
    let w = rest
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_ascii_uppercase();
    matches!(
        w.as_str(),
        "SHARE" | "SHARED" | "LOCAL" | "TAB" | "UNSHARE" | "GLOBAL"
    )
}

fn head(text: &str) -> String {
    let mut out = String::new();
    let mut last_ws = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !last_ws && !out.is_empty() {
                out.push(' ');
            }
            last_ws = true;
        } else {
            out.push(c);
            last_ws = false;
        }
        if out.chars().count() >= 60 {
            out.push('…');
            break;
        }
    }
    out
}

/// 문서 → 아웃라인.
#[must_use]
pub fn outline(src: &str, dialect: Option<Dialect>) -> Outline {
    let mut out = Outline::default();
    for item in split_script_in(src, dialect) {
        scan_item(src, &item, &mut out);
    }
    out
}

fn scan_item(src: &str, item: &Item, out: &mut Outline) {
    let base = item.span.start;
    let text = &src[item.span.clone()];
    let classes = classify(text);
    let ws = words(text, &classes);
    let Some(first) = ws.first() else {
        return;
    };
    let push = |out: &mut Outline, kind: SymKind, name: &str, detail: &str, byte: usize| {
        out.symbols.push(Symbol {
            kind,
            name: name.to_string(),
            detail: detail.to_string(),
            line: line_of(src, base + byte),
            byte: base + byte,
            depth: 0,
            parent: None,
        });
    };
    match &item.kind {
        ItemKind::Command(_) => {
            // DEFINE name [= …] · VARIABLE name [TYPE] · 그 밖 명령은 머리만.
            if eq(first, "DEFINE") || eq(first, "DEF") {
                if let Some(w) = ws.get(1).filter(|w| is_name(w)) {
                    let rest = text[w.end..].trim_start();
                    // ★ T-434(10-27): `DEFINE 이름 SHARE|LOCAL|GLOBAL …`은 층 이동이지 선언이 아니다 → 아웃라인에서 제외(원 선언만 남는다).
                    if is_layer_word(rest) {
                        return;
                    }
                    let val = rest
                        .strip_prefix('=')
                        .map(|v| head(v.trim()))
                        .unwrap_or_default();
                    push(out, SymKind::Define, w.text, &val, w.start);
                    return;
                }
            } else if eq(first, "VARIABLE") || eq(first, "VAR") {
                if let Some(w) = ws.get(1).filter(|w| is_name(w)) {
                    let ty = ws.get(2).map(|t| t.text.to_string()).unwrap_or_default();
                    push(out, SymKind::Variable, w.text, &ty, w.start);
                    return;
                }
            }
            push(
                out,
                SymKind::Statement,
                &head(&item.text),
                "command",
                first.start,
            );
        }
        ItemKind::Invalid(_) => {}
        ItemKind::Sql(kind) => {
            let detail = match kind {
                SqlKind::Query => "select",
                SqlKind::Dml => "dml",
                SqlKind::Ddl => "ddl",
                SqlKind::Block => "block",
                SqlKind::Other => "sql",
            };
            // ★ `CREATE … KIND name`으로 시작하는 문장은 문장 행을 만들지 않는다(T-422 · D-357 결정 2) — 대상 행 하나가 뿌리.
            //   그 밖 문장은 머리 행이 뿌리이고 그 안의 CTE·선언·서브프로그램이 자식.
            let root = if create_head(&ws).is_some() {
                None
            } else {
                push(
                    out,
                    SymKind::Statement,
                    &head(&item.text),
                    detail,
                    first.start,
                );
                Some(out.symbols.len() - 1)
            };
            scan_sql(text, &ws, *kind, base, src, out, root);
        }
    }
}

/// `CREATE [수식어…] KIND [BODY] [schema.]name` — 낱말 목록의 머리가 CREATE 대상 선언이면 `(KIND 낱말 색인, 이름 낱말 색인)`.
fn create_head(ws: &[Word<'_>]) -> Option<(usize, usize)> {
    if !ws.first().is_some_and(|w| eq(w, "CREATE")) {
        return None;
    }
    let mut k = 1;
    while k < ws.len() && CREATE_SKIP.iter().any(|s| eq(&ws[k], s)) {
        k += 1;
    }
    let (sk, _) = ws.get(k).and_then(|w| create_kind(w.text))?;
    let mut m = k + 1;
    if matches!(sk, SymKind::Package | SymKind::Type) && ws.get(m).is_some_and(|w| eq(w, "BODY")) {
        m += 1;
    }
    ws.get(m).filter(|w| is_name(w)).map(|_| (k, m))
}

/// 문장 본문 걷기 — `root` = 이 문장의 머리 행(없으면 CREATE 대상이 뿌리가 된다).
fn scan_sql(
    text: &str,
    ws: &[Word<'_>],
    kind: SqlKind,
    base: usize,
    src: &str,
    out: &mut Outline,
    root: Option<usize>,
) {
    let has_stmt_row = root.is_some();
    // 지금의 뿌리(문장 머리 또는 CREATE 대상) · 패키지(안의 서브프로그램·선언의 부모) · ★ 열린 서브프로그램 스택(V1 10-25 결함 ①:
    //   프로시저 안 지역 변수가 패키지의 자식으로 보이던 것) — 본문 있는 PROCEDURE/FUNCTION만 올리고 `END 이름`으로 내린다.
    let mut root = root;
    let mut pkg: Option<usize> = None;
    let mut subs: Vec<usize> = Vec::new();
    let push = |out: &mut Outline,
                kind: SymKind,
                name: &str,
                detail: &str,
                byte: usize,
                parent: Option<usize>|
     -> usize {
        let depth = parent.map_or(0, |p| out.symbols[p].depth.saturating_add(1));
        out.symbols.push(Symbol {
            kind,
            name: name.to_string(),
            detail: detail.to_string(),
            line: line_of(src, base + byte),
            byte: base + byte,
            depth,
            parent,
        });
        out.symbols.len() - 1
    };
    let mut i = 0;
    // 선언부 상태: DECLARE / IS / AS 뒤 ~ BEGIN 앞 = 변수 선언 줄.
    let mut in_decl = false;
    // ★ 괄호 깊이(T-422 결함 수정 · 사용자 10-24 조사): 서브프로그램 매개변수 `(p_name IN VARCHAR2, …)` 줄은 선언이 아니다.
    let mut paren: u32 = 0;
    while i < ws.len() {
        let w = &ws[i];
        if w.quoted {
            i += 1;
            continue;
        }
        if w.text == "(" {
            paren += 1;
            i += 1;
            continue;
        }
        if w.text == ")" {
            paren = paren.saturating_sub(1);
            i += 1;
            continue;
        }
        let inner_parent = subs.last().copied().or(pkg).or(root);
        let up = w.text.to_ascii_uppercase();
        match up.as_str() {
            // `CREATE [OR REPLACE] [수식어…] KIND [BODY] [schema.]name` — 문장 머리뿐 아니라 T-SQL 배치(GO 사이)처럼 한 문장 안의
            //   여러 CREATE도 잡는다. 문장 행이 없으면(CREATE 머리 문장) 대상이 새 뿌리 · 있으면 문장 행의 자식.
            "CREATE" if paren == 0 => {
                let mut k = i + 1;
                while k < ws.len() && CREATE_SKIP.iter().any(|s| eq(&ws[k], s)) {
                    k += 1;
                }
                if let Some((sk0, label0)) = ws.get(k).and_then(|w| create_kind(w.text)) {
                    let mut sk = sk0;
                    let mut label = label0;
                    let mut m = k + 1;
                    if sk == SymKind::Package && ws.get(m).is_some_and(|w| eq(w, "BODY")) {
                        sk = SymKind::PackageBody;
                        label = "package body";
                        m += 1;
                    }
                    if sk == SymKind::Type && ws.get(m).is_some_and(|w| eq(w, "BODY")) {
                        label = "type body";
                        m += 1;
                    }
                    if ws.get(m).is_some_and(is_name) {
                        let (name, _) = dotted_name(ws, m);
                        let parent = if has_stmt_row { root } else { None };
                        let idx = push(out, sk, &name, label, ws[m].start, parent);
                        if !has_stmt_row {
                            root = Some(idx);
                        }
                        pkg = matches!(sk, SymKind::Package | SymKind::PackageBody).then_some(idx);
                        subs.clear();
                        in_decl = false;
                    }
                    i = m + 1;
                    continue;
                }
            }
            "WITH" => {
                // WITH [RECURSIVE] a AS (…), b AS (…)
                let mut k = i + 1;
                if k < ws.len() && eq(&ws[k], "RECURSIVE") {
                    k += 1;
                }
                while let Some(nw) = ws.get(k).filter(|w| is_name(w)) {
                    // `name AS (` 또는 `name (cols) AS (`
                    let mut m = k + 1;
                    if ws.get(m).is_some_and(|w| w.text == "(") {
                        let mut d = 0i32;
                        while m < ws.len() {
                            if ws[m].text == "(" {
                                d += 1;
                            } else if ws[m].text == ")" {
                                d -= 1;
                                if d == 0 {
                                    m += 1;
                                    break;
                                }
                            }
                            m += 1;
                        }
                    }
                    if !(ws.get(m).is_some_and(|w| eq(w, "AS"))
                        && ws.get(m + 1).is_some_and(|w| w.text == "("))
                    {
                        break;
                    }
                    push(out, SymKind::Cte, nw.text, "cte", nw.start, inner_parent);
                    // 괄호 짝까지 건너뛴다.
                    let mut d = 0i32;
                    let mut p = m + 1;
                    while p < ws.len() {
                        if ws[p].text == "(" {
                            d += 1;
                        } else if ws[p].text == ")" {
                            d -= 1;
                            if d == 0 {
                                p += 1;
                                break;
                            }
                        }
                        p += 1;
                    }
                    if ws.get(p).is_some_and(|w| w.text == ",") {
                        k = p + 1;
                        continue;
                    }
                    i = p;
                    break;
                }
                if i <= k {
                    i = k.max(i + 1);
                }
                continue;
            }
            // 블록 안 · 패키지 안 · 또는 문장 첫 낱말(SQLite 등 `;` 분리 방언에서 패키지 본문이 문장으로 쪼개질 때).
            "PROCEDURE" | "FUNCTION"
                if paren == 0 && (kind == SqlKind::Block || pkg.is_some() || i == 0) =>
            {
                if let Some(nw) = ws.get(i + 1).filter(|w| is_name(w)) {
                    let sk = if up == "PROCEDURE" {
                        SymKind::Procedure
                    } else {
                        SymKind::Function
                    };
                    // 본문이 있는 선언(`… IS/AS` 가 `;` 앞에 온다)만 스택에 — 명세의 `PROCEDURE a(…);`는 형제로 남는다.
                    //   선언부 밖에서 새 서브프로그램이 시작되면 앞 것(`END;`만으로 닫힌 것)은 끝난 것으로 본다.
                    let body = subprogram_has_body(ws, i + 2);
                    if body && !in_decl {
                        subs.pop();
                    }
                    let parent = subs.last().copied().or(pkg).or(root);
                    let idx = push(out, sk, nw.text, sk.label(), nw.start, parent);
                    if body {
                        subs.push(idx);
                    }
                    i += 2;
                    continue;
                }
            }
            // ★ 커서는 선언부·패키지 안에서만(T-422 · 종전 = 어디서나 → `FOR … IN c` 같은 본문 낱말 오탐 여지).
            "CURSOR" if paren == 0 && (in_decl || pkg.is_some()) => {
                if let Some(nw) = ws.get(i + 1).filter(|w| is_name(w)) {
                    push(
                        out,
                        SymKind::Cursor,
                        nw.text,
                        "cursor",
                        nw.start,
                        inner_parent,
                    );
                    i += 2;
                    continue;
                }
            }
            "TYPE" if paren == 0 && (kind == SqlKind::Block || pkg.is_some()) => {
                if let Some(nw) = ws.get(i + 1).filter(|w| is_name(w)) {
                    if ws.get(i + 2).is_some_and(|w| eq(w, "IS")) {
                        push(out, SymKind::Type, nw.text, "type", nw.start, inner_parent);
                        i += 2;
                        continue;
                    }
                }
            }
            "<<" => {
                if let Some(nw) = ws.get(i + 1).filter(|w| is_name(w)) {
                    if ws.get(i + 2).is_some_and(|w| w.text == ">>") {
                        push(
                            out,
                            SymKind::Label,
                            nw.text,
                            "label",
                            nw.start,
                            inner_parent,
                        );
                        i += 3;
                        continue;
                    }
                }
            }
            "DECLARE" if paren == 0 => {
                // T-SQL `DECLARE @v TYPE` · PL/SQL `DECLARE` 선언부 시작.
                if let Some(nw) = ws.get(i + 1).filter(|w| w.text.starts_with('@')) {
                    let ty = ws.get(i + 2).map(|t| t.text).unwrap_or("");
                    push(out, SymKind::Declared, nw.text, ty, nw.start, inner_parent);
                    i += 2;
                    continue;
                }
                in_decl = kind == SqlKind::Block;
            }
            "IS" | "AS" if paren == 0 && kind == SqlKind::Block => {
                // 서브프로그램/패키지 머리 뒤 = 선언부(단, `TYPE x IS`·`CURSOR c IS`는 위에서 처리).
                in_decl = true;
            }
            "BEGIN" if paren == 0 => in_decl = false,
            "END" if paren == 0 => {
                in_decl = false;
                // `END 이름;` = 그 서브프로그램 닫힘(스택 맨 위와 같은 이름일 때만).
                if let Some(nw) = ws.get(i + 1) {
                    if subs
                        .last()
                        .is_some_and(|&t| out.symbols[t].name.eq_ignore_ascii_case(nw.text))
                    {
                        subs.pop();
                    }
                }
            }
            _ => {}
        }
        // 선언부 변수: 줄 첫 낱말이 이름이고 다음 낱말이 타입(예약어·기호 아님)이며 `;` 로 끝나는 줄 — 괄호 안(매개변수)은 제외.
        if in_decl
            && paren == 0
            && is_name(w)
            && !CLAUSE_STOP.iter().any(|s| eq(w, s))
            && at_line_start(text, w.start)
        {
            if let Some(ty) = ws.get(i + 1) {
                let ty_up = ty.text.to_ascii_uppercase();
                let is_kw = matches!(
                    ty_up.as_str(),
                    "IS" | "AS"
                        | "BEGIN"
                        | "END"
                        | "PROCEDURE"
                        | "FUNCTION"
                        | "CURSOR"
                        | "TYPE"
                        | "PRAGMA"
                        | "EXCEPTION"
                );
                if is_name(ty) && !is_kw && ty.text != "(" {
                    // 타입 = 다음 `;`까지(요약).
                    let end = text[ty.start..]
                        .find(';')
                        .map(|p| ty.start + p)
                        .unwrap_or(text.len());
                    let ty_text = head(text[ty.start..end].trim());
                    push(
                        out,
                        SymKind::Declared,
                        w.text,
                        &ty_text,
                        w.start,
                        inner_parent,
                    );
                }
            }
        }
        i += 1;
    }
}

/// 서브프로그램 머리 뒤(`from` = 이름 다음 낱말)에 본문이 있는가 — 괄호 밖 첫 `;` 전에 `IS`/`AS`가 오면 본문(`PROCEDURE a IS …`) ·
/// `;`가 먼저면 명세/전방 선언(`PROCEDURE a(x IN NUMBER);`).
fn subprogram_has_body(ws: &[Word<'_>], from: usize) -> bool {
    let mut d: u32 = 0;
    for w in ws.iter().skip(from) {
        match w.text {
            "(" => d += 1,
            ")" => d = d.saturating_sub(1),
            ";" if d == 0 => return false,
            _ if d == 0 && (eq(w, "IS") || eq(w, "AS")) => return true,
            _ => {}
        }
    }
    false
}

fn at_line_start(text: &str, byte: usize) -> bool {
    text[..byte]
        .rsplit('\n')
        .next()
        .is_some_and(|l| l.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(o: &Outline, k: SymKind) -> Vec<String> {
        o.symbols
            .iter()
            .filter(|s| s.kind == k)
            .map(|s| s.name.clone())
            .collect()
    }

    /// 이진 파일을 손실 변환한 본문(U+FFFD · 제어 문자 · 잘린 인용부호 · 긴 줄)에서도 패닉하지 않는다(사용자 09-23 `.o` 파일 아웃라인 = 앱 종료).
    #[test]
    fn binary_like_input_never_panics() {
        let mut bytes: Vec<u8> = Vec::new();
        let mut x: u32 = 0x1234_5678;
        for _ in 0..20_000 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            bytes.push((x & 0xff) as u8);
        }
        let s = String::from_utf8_lossy(&bytes).into_owned();
        for d in [
            None,
            Some(Dialect::Oracle),
            Some(Dialect::Mssql),
            Some(Dialect::Postgres),
            Some(Dialect::Sqlite),
        ] {
            let o = outline(&s, d);
            let _ = o.names();
        }
        // 짝 안 맞는 인용부호 · 주석 · 세미콜론 없음 · 키워드 조각 · 다국어 인용 식별자가 끝에서 잘림.
        for s2 in [
            "CREATE '\u{fffd}PROCEDURE \"x\u{0}y DECLARE /* \u{fffd}\u{fffd} BEGIN END; DEFINE \u{fffd}= '",
            "SELECT \"가나다",
            "SELECT [가나다",
            "CREATE PROCEDURE \u{fffd}",
            "DECLARE v \u{fffd}",
        ] {
            let _ = outline(s2, Some(Dialect::Oracle));
            let _ = outline(s2, Some(Dialect::Mssql));
        }
    }

    #[test]
    fn statements_defines_ctes_and_create_targets() {
        // ★ T-434: `DEFINE v_user SHARE`(층 이동)·`DEFINE v_user GLOBAL;`은 선언이 아니라 아웃라인에 두 번째 줄로 나오지 않는다.
        let src = "DEFINE v_user = 'scott'\nDEFINE v_user SHARE\nDEFINE v_user global;\nVARIABLE rc REFCURSOR\nWITH RECURSIVE n(i) AS (SELECT 1), m AS (SELECT 2)\nSELECT * FROM n, m;\nCREATE OR REPLACE VIEW app.v_x AS SELECT 1 FROM dual;\nCREATE UNIQUE INDEX ix_a ON t(a);\n-- WITH not_a_cte AS ( in comment\nSELECT 'WITH q AS (' FROM dual;";
        let o = outline(src, Some(Dialect::Oracle));
        assert_eq!(names(&o, SymKind::Define), vec!["v_user"]);
        assert_eq!(names(&o, SymKind::Variable), vec!["rc"]);
        assert_eq!(names(&o, SymKind::Cte), vec!["n", "m"]);
        assert_eq!(names(&o, SymKind::Object), vec!["app.v_x", "ix_a"]);
        let stmts: Vec<&Symbol> = o
            .symbols
            .iter()
            .filter(|s| s.kind == SymKind::Statement)
            .collect();
        // ★ CREATE 머리 문장(VIEW · INDEX)은 문장 행이 없다(T-422) — WITH…SELECT · SELECT 둘만.
        assert_eq!(stmts.len(), 2, "{stmts:?}");
        let vx = o.find("app.v_x").expect("view");
        assert_eq!((vx.depth, vx.parent), (0, None));
        // CTE의 부모 = 그 문장 머리 행.
        let n = o.find("n").expect("cte");
        assert_eq!(
            o.symbols[n.parent.expect("parent")].kind,
            SymKind::Statement
        );
        assert_eq!(n.depth, 1);
        assert_eq!(o.find("v_user").map(|s| s.line), Some(1));
        assert_eq!(o.find("m").map(|s| s.line), Some(5));
        let d = o
            .symbols
            .iter()
            .find(|s| s.kind == SymKind::Define)
            .expect("define");
        assert_eq!(d.detail, "'scott'");
    }

    #[test]
    fn plsql_package_body_subprograms_declares_cursor_label() {
        let src = "CREATE OR REPLACE PACKAGE BODY pkg_x AS\n  g_count NUMBER := 0;\n  CURSOR c_emp IS SELECT 1 FROM dual;\n  TYPE t_list IS TABLE OF NUMBER;\n  PROCEDURE do_it(p IN NUMBER) IS\n    v_tmp VARCHAR2(10);\n  BEGIN\n    <<retry>>\n    NULL;\n  END do_it;\n  FUNCTION calc RETURN NUMBER IS\n  BEGIN\n    RETURN 1;\n  END calc;\nEND pkg_x;\n/\n";
        let o = outline(src, Some(Dialect::Oracle));
        assert_eq!(names(&o, SymKind::PackageBody), vec!["pkg_x"]);
        assert_eq!(names(&o, SymKind::Procedure), vec!["do_it"]);
        assert_eq!(names(&o, SymKind::Function), vec!["calc"]);
        assert_eq!(names(&o, SymKind::Cursor), vec!["c_emp"]);
        assert_eq!(names(&o, SymKind::Type), vec!["t_list"]);
        assert_eq!(names(&o, SymKind::Label), vec!["retry"]);
        assert_eq!(names(&o, SymKind::Declared), vec!["g_count", "v_tmp"]);
        let p = o.find("do_it").expect("proc");
        // ★ 패키지 본문이 뿌리(깊이 0 · 문장 행 없음) → 서브프로그램 = 깊이 1 · 부모 = 패키지 본문.
        let body = o.find("pkg_x").expect("body");
        assert_eq!((body.depth, body.parent), (0, None));
        assert_eq!((p.line, p.depth), (5, 1));
        assert_eq!(p.parent.map(|i| o.symbols[i].name.as_str()), Some("pkg_x"));
        // ★ 서브프로그램 안 지역 변수·라벨 = 그 서브프로그램의 자식(깊이 2) · 패키지 선언 g_count = 패키지의 자식(깊이 1).
        let v = o.find("v_tmp").expect("v_tmp");
        assert_eq!(
            (v.depth, v.parent.map(|i| o.symbols[i].name.as_str())),
            (2, Some("do_it"))
        );
        let l = o.find("retry").expect("label");
        assert_eq!(l.parent.map(|i| o.symbols[i].name.as_str()), Some("do_it"));
        let g = o.find("g_count").expect("g_count");
        assert_eq!(
            (g.depth, g.parent.map(|i| o.symbols[i].name.as_str())),
            (1, Some("pkg_x"))
        );
        // `END do_it;` 뒤의 calc는 패키지의 자식(형제).
        let c = o.find("calc").expect("calc");
        assert_eq!(
            (c.depth, c.parent.map(|i| o.symbols[i].name.as_str())),
            (1, Some("pkg_x"))
        );
        assert!(
            !o.symbols.iter().any(|s| s.kind == SymKind::Statement),
            "CREATE 머리 문장에 문장 행이 생겼다: {:?}",
            o.symbols
        );
        let n = o.names();
        assert!(n
            .iter()
            .any(|(s, k)| s == "v_tmp" && *k == SymKind::Declared));
    }

    #[test]
    fn tsql_declare_and_quoted_names() {
        let src = "DECLARE @cnt INT = 0;\nSELECT [Order Id] FROM \"dbo\".\"Orders\" o;\nCREATE PROC dbo.usp_x AS SELECT 1;";
        let o = outline(src, Some(Dialect::Mssql));
        assert_eq!(names(&o, SymKind::Declared), vec!["@cnt"]);
        assert_eq!(names(&o, SymKind::Procedure), vec!["dbo.usp_x"]);
        let cls = classify(src);
        let w = words(src, &cls);
        assert!(w.iter().any(|w| w.quoted && w.text == "Order Id"));
        assert!(w.iter().any(|w| w.text == "@cnt"));
    }

    /// ★ T-422 결함 수정 — 여러 줄 매개변수 목록은 선언 변수가 아니다 · 진짜 지역 변수만 `Declared` · 매개변수 괄호 뒤 `IS`가 선언부를 연다.
    #[test]
    fn parameters_on_their_own_lines_are_not_declared_vars() {
        let src = "CREATE OR REPLACE PACKAGE BODY pkg_rc AS\n  PROCEDURE open_emp(\n    p_name IN VARCHAR2,\n    p_rc OUT SYS_REFCURSOR\n  ) IS\n    v_pat VARCHAR2(100);\n  BEGIN\n    OPEN p_rc FOR SELECT 1 FROM dual;\n  END open_emp;\nEND pkg_rc;\n/\n";
        let o = outline(src, Some(Dialect::Oracle));
        assert_eq!(names(&o, SymKind::Declared), vec!["v_pat"]);
        assert_eq!(names(&o, SymKind::Procedure), vec!["open_emp"]);
        // 본문의 `FOR`/`OPEN`은 커서가 아니다 · 선언부 밖 CURSOR 낱말도 심볼이 아니다.
        assert!(names(&o, SymKind::Cursor).is_empty());
        let v = o.find("v_pat").expect("var");
        assert_eq!(
            v.parent.map(|i| o.symbols[i].name.as_str()),
            Some("open_emp")
        );
        assert_eq!(v.depth, 2);
    }

    /// ★ V1 10-25 결함 ① — 명세 선언(본문 없음)은 형제 · `END;`(이름 없음)로 닫힌 뒤 시작하는 다음 서브프로그램도 형제 ·
    /// 지역(중첩) 서브프로그램은 바깥 것의 자식.
    #[test]
    fn spec_declarations_unnamed_end_and_nested_subprograms() {
        let spec = "CREATE OR REPLACE PACKAGE p AS\n  PROCEDURE a(x IN NUMBER);\n  FUNCTION b RETURN NUMBER;\n  g NUMBER;\nEND p;\n/\n";
        let o = outline(spec, Some(Dialect::Oracle));
        for n in ["a", "b", "g"] {
            let s = o.find(n).expect(n);
            assert_eq!(
                (s.depth, s.parent.map(|i| o.symbols[i].name.as_str())),
                (1, Some("p")),
                "{n}"
            );
        }
        let body = "CREATE OR REPLACE PACKAGE BODY p AS\n  PROCEDURE a IS\n    va NUMBER;\n    PROCEDURE inner_a IS\n      vi NUMBER;\n    BEGIN\n      NULL;\n    END inner_a;\n  BEGIN\n    NULL;\n  END;\n  FUNCTION b RETURN NUMBER IS\n    vb NUMBER;\n  BEGIN\n    RETURN 1;\n  END;\nEND p;\n/\n";
        let o = outline(body, Some(Dialect::Oracle));
        let par = |n: &str| {
            let s = o.find(n).expect(n);
            (s.depth, s.parent.map(|i| o.symbols[i].name.to_string()))
        };
        assert_eq!(par("a"), (1, Some("p".into())));
        assert_eq!(par("va"), (2, Some("a".into())));
        assert_eq!(par("inner_a"), (2, Some("a".into())));
        assert_eq!(par("vi"), (3, Some("inner_a".into())));
        assert_eq!(par("b"), (1, Some("p".into())), "`END;` 뒤의 b는 a의 형제");
        assert_eq!(par("vb"), (2, Some("b".into())));
    }

    /// T-SQL 배치 하나에 CREATE 둘 = 뿌리 둘(문장 행 0) · 일반 문장 안 `CREATE`(동적 SQL 아닌 실제 배치)는 문장 행의 자식.
    #[test]
    fn create_targets_are_roots_and_plain_statements_keep_their_row() {
        let src =
            "CREATE TABLE dbo.t1 (a INT);\nGO\nCREATE PROC dbo.p1 AS SELECT 1;\nGO\nSELECT * FROM dbo.t1;";
        let o = outline(src, Some(Dialect::Mssql));
        let roots: Vec<(&str, SymKind)> = o
            .symbols
            .iter()
            .filter(|s| s.parent.is_none())
            .map(|s| (s.name.as_str(), s.kind))
            .collect();
        assert_eq!(
            roots,
            vec![
                ("dbo.t1", SymKind::Object),
                ("dbo.p1", SymKind::Procedure),
                ("SELECT * FROM dbo.t1", SymKind::Statement),
            ]
        );
        assert!(o.symbols.iter().all(|s| s.depth == 0));
        // 같은 항목 안의 두 번째 CREATE(GO 없는 배치)도 뿌리.
        let o2 = outline(
            "CREATE TABLE a (x INT)\nCREATE TABLE b (y INT)\nGO\n",
            Some(Dialect::Mssql),
        );
        let n: Vec<&str> = o2
            .symbols
            .iter()
            .filter(|s| s.kind == SymKind::Object)
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(n, vec!["a", "b"]);
        assert!(o2.symbols.iter().all(|s| s.parent.is_none()));
    }
}
