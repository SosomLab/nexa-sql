//! ★ T-283 2단계(사용자 11-06 결정 "컬럼 + 인자/반환" · 19 §6-2 "차이 분석 로직" · 11-07): CREATE 문의 **정적 선언**을 경량으로 뽑아
//! 견준다(순수 · 시험). 편집기 문장과 서버 생성 DDL **양쪽을 같은 추출기**로 읽으므로 방언 표기 차이가 상당 부분 상쇄된다.
//! 테이블(·열 목록을 적은 뷰) = 열(이름 · 타입 · NULL 허용) · 프로시저/함수 = 인자(이름 · 방향 · 타입) + 반환 타입. 못 읽으면 `None`
//! (= "선언 비교 불가" · 소스 줄 diff는 그대로). 완전한 파서가 아니라 **머리 선언만** 본다 — 본문(BEGIN … END)은 보지 않는다.

use nsql_catalog::ObjectKind;

/// 열 선언.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColDecl {
    pub name: String,
    /// 정규화한 타입 글(대문자 · 괄호 안 공백 0 · Oracle `BYTE`/`CHAR` 길이 단위 제거) · 뷰 열 목록이면 빈 글.
    pub ty: String,
    pub nullable: bool,
}

/// 인자 선언.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArgDecl {
    /// 이름(대문자 · 인용/`@` 제거 · PG 이름 없는 인자 = 빈 글).
    pub name: String,
    /// `IN` · `OUT` · `INOUT`.
    pub mode: String,
    pub ty: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Decl {
    Table(Vec<ColDecl>),
    Routine {
        args: Vec<ArgDecl>,
        ret: Option<String>,
    },
}

/// 선언 차이(이름 = 표시용 원문 대문자).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DeclDiff {
    /// 편집기에만(추가).
    pub added: Vec<String>,
    /// 서버에만(삭제).
    pub removed: Vec<String>,
    /// 양쪽에 있으나 타입/NULL/방향이 다름 `(이름, 편집기, 서버)`.
    pub changed: Vec<(String, String, String)>,
    /// 공통 항목의 순서가 다름.
    pub reordered: bool,
    /// 반환 타입 `(편집기, 서버)`(다를 때만).
    pub ret: Option<(String, String)>,
}

impl DeclDiff {
    pub(crate) fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
            && !self.reordered
            && self.ret.is_none()
    }
}

// ───────────────────────────────────────────── 토큰

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    /// 낱말(대문자 · 인용 부호를 벗겼으면 `quoted`).
    Word(String, bool),
    /// `(` `)` `,` `;` `=` 등 한 글자.
    P(char),
}

impl Tok {
    fn is(&self, w: &str) -> bool {
        matches!(self, Tok::Word(s, false) if s == w)
    }
    fn word(&self) -> Option<&str> {
        match self {
            Tok::Word(s, _) => Some(s.as_str()),
            Tok::P(_) => None,
        }
    }
}

/// 토큰화: 주석(`--` · `/* */`) 제거 · 문자열 리터럴은 낱말 하나(`'…'`) · 인용 식별자(`"…"` `[…]` `` `…` ``)는 벗겨 `quoted` · 그 밖 낱말은 대문자.
fn tokenize(text: &str) -> Vec<Tok> {
    let cs: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '-' && cs.get(i + 1) == Some(&'-') {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && cs.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < cs.len() && !(cs[i] == '*' && cs[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(cs.len());
            continue;
        }
        if c == '\'' {
            let mut s = String::from("'");
            i += 1;
            while i < cs.len() {
                if cs[i] == '\'' {
                    if cs.get(i + 1) == Some(&'\'') {
                        s.push_str("''");
                        i += 2;
                        continue;
                    }
                    break;
                }
                s.push(cs[i]);
                i += 1;
            }
            s.push('\'');
            i += 1;
            out.push(Tok::Word(s, false));
            continue;
        }
        // PG 배열 접미 `[]`(`int[]` · `text[][]`)는 식별자 인용이 아니라 타입의 일부.
        if c == '[' && cs.get(i + 1) == Some(&']') {
            out.push(Tok::Word("[]".into(), false));
            i += 2;
            continue;
        }
        let close = match c {
            '"' => Some('"'),
            '[' => Some(']'),
            '`' => Some('`'),
            _ => None,
        };
        if let Some(close) = close {
            let mut s = String::new();
            i += 1;
            while i < cs.len() && cs[i] != close {
                s.push(cs[i]);
                i += 1;
            }
            i += 1;
            out.push(Tok::Word(s, true));
            continue;
        }
        if c.is_alphanumeric() || matches!(c, '_' | '$' | '#' | '@' | '.') {
            let mut s = String::new();
            while i < cs.len()
                && (cs[i].is_alphanumeric() || matches!(cs[i], '_' | '$' | '#' | '@' | '.'))
            {
                s.push(cs[i]);
                i += 1;
            }
            out.push(Tok::Word(s.to_ascii_uppercase(), false));
            continue;
        }
        out.push(Tok::P(c));
        i += 1;
    }
    out
}

/// 괄호 묶음(`(` 다음부터 짝 `)` 전까지)을 최상위 `,`로 나눈 항목들 · 반환 = (항목들, `)` 다음 위치). `start` = `(` 위치.
fn paren_items(toks: &[Tok], start: usize) -> Option<(Vec<Vec<Tok>>, usize)> {
    if toks.get(start) != Some(&Tok::P('(')) {
        return None;
    }
    let mut depth = 0usize;
    let mut items: Vec<Vec<Tok>> = vec![Vec::new()];
    let mut i = start;
    while i < toks.len() {
        match &toks[i] {
            Tok::P('(') => {
                depth += 1;
                if depth > 1 {
                    items.last_mut()?.push(toks[i].clone());
                }
            }
            Tok::P(')') => {
                depth -= 1;
                if depth == 0 {
                    if items.last().is_some_and(Vec::is_empty) {
                        items.pop();
                    }
                    return Some((items, i + 1));
                }
                items.last_mut()?.push(toks[i].clone());
            }
            Tok::P(',') if depth == 1 => items.push(Vec::new()),
            t => items.last_mut()?.push(t.clone()),
        }
        i += 1;
    }
    None
}

/// 타입 글 정규화: 낱말은 공백 하나로 · 괄호·쉼표 주변 공백 0 · Oracle 길이 단위 `BYTE`/`CHAR` 제거 · 흔한 동의어 통일.
fn type_text(toks: &[Tok]) -> String {
    let mut s = String::new();
    for t in toks {
        match t {
            Tok::P(c) => s.push(*c),
            Tok::Word(w, _) => {
                if !s.is_empty() && !s.ends_with('(') && !s.ends_with(',') && w != "[]" {
                    s.push(' ');
                }
                s.push_str(w);
            }
        }
    }
    let s = s
        .replace(" (", "(")
        .replace("( ", "(")
        .replace(" )", ")")
        .replace(", ", ",")
        .replace(" ,", ",")
        .replace(" BYTE)", ")")
        .replace(" CHAR)", ")");
    // 흔한 동의어 통일(배열 접미는 떼었다 붙인다).
    let (base, arr) = match s.find("[]") {
        Some(k) => (s[..k].to_string(), s[k..].to_string()),
        None => (s.clone(), String::new()),
    };
    let base = match base.as_str() {
        "INT" | "INT4" => "INTEGER".to_string(),
        "INT8" => "BIGINT".to_string(),
        "INT2" => "SMALLINT".to_string(),
        "BOOL" => "BOOLEAN".to_string(),
        "CHARACTER VARYING" => "VARCHAR".to_string(),
        "DOUBLE PRECISION" => "FLOAT".to_string(),
        _ => base,
    };
    base + &arr
}

const COL_STOP: &[&str] = &[
    "NOT",
    "NULL",
    "DEFAULT",
    "CONSTRAINT",
    "GENERATED",
    "PRIMARY",
    "UNIQUE",
    "REFERENCES",
    "CHECK",
    "COLLATE",
    "IDENTITY",
    "AUTO_INCREMENT",
    "COMMENT",
    "ENABLE",
    "DISABLE",
    "VISIBLE",
    "INVISIBLE",
    "SPARSE",
    "MASKED",
    "ROWGUIDCOL",
    "ENCRYPT",
    "AS",
    "ON",
    "WITH",
    "STORAGE",
    "ALWAYS",
    "CHARACTER",
    "INLINE",
];

const CONSTRAINT_HEAD: &[&str] = &[
    "CONSTRAINT",
    "PRIMARY",
    "UNIQUE",
    "FOREIGN",
    "CHECK",
    "KEY",
    "INDEX",
    "PERIOD",
    "EXCLUDE",
    "LIKE",
    "FULLTEXT",
    "SPATIAL",
    "SUPPLEMENTAL",
];

/// 열 항목 하나 → 선언(제약 항목은 None).
fn col_decl(item: &[Tok]) -> Option<ColDecl> {
    let first = item.first()?;
    let name = match first {
        Tok::Word(w, quoted) => {
            if !quoted && CONSTRAINT_HEAD.contains(&w.as_str()) {
                return None;
            }
            w.to_ascii_uppercase()
        }
        Tok::P(_) => return None,
    };
    let rest = &item[1..];
    let stop = rest
        .iter()
        .position(|t| {
            t.word().is_some_and(|w| COL_STOP.contains(&w)) && !matches!(t, Tok::Word(_, true))
        })
        .unwrap_or(rest.len());
    let ty = type_text(&rest[..stop]);
    let tail = &rest[stop..];
    let mut nullable = true;
    for (i, t) in tail.iter().enumerate() {
        if t.is("NOT") && tail.get(i + 1).is_some_and(|n| n.is("NULL")) {
            nullable = false;
        }
        if t.is("PRIMARY") && tail.get(i + 1).is_some_and(|n| n.is("KEY")) {
            nullable = false;
        }
    }
    Some(ColDecl { name, ty, nullable })
}

/// 인자 항목 하나 → 선언. 방향 낱말(IN · OUT · INOUT · `IN OUT` · OUTPUT · VARIADIC · NOCOPY)은 어디 있든 뽑고, `DEFAULT …`/`= …`/`:= …`는 자른다.
fn arg_decl(item: &[Tok]) -> Option<ArgDecl> {
    // 1) 방향 낱말은 항목 **전체**에서 찾는다 — SQL Server는 기본값 뒤에 OUTPUT이 온다(`@b int = 0 OUTPUT`).
    let mut mode = String::from("IN");
    let mut i = 0;
    while i < item.len() {
        let t = &item[i];
        if t.is("IN") && item.get(i + 1).is_some_and(|n| n.is("OUT")) {
            mode = "INOUT".into();
            i += 2;
            continue;
        }
        if t.is("INOUT") {
            mode = "INOUT".into();
        } else if (t.is("OUT") || t.is("OUTPUT")) && mode != "INOUT" {
            mode = "OUT".into();
        }
        i += 1;
    }
    // 2) 이름·타입 = 방향 낱말을 뺀 본문 · `DEFAULT …`/`= …`/`:= …`에서 끊는다.
    let mut body: Vec<Tok> = Vec::new();
    let mut i = 0;
    while i < item.len() {
        let t = &item[i];
        if matches!(t, Tok::P('=') | Tok::P(':')) || t.is("DEFAULT") {
            break;
        }
        if t.is("IN") && item.get(i + 1).is_some_and(|n| n.is("OUT")) {
            i += 2;
            continue;
        }
        if t.is("IN")
            || t.is("OUT")
            || t.is("OUTPUT")
            || t.is("INOUT")
            || t.is("VARIADIC")
            || t.is("NOCOPY")
            || t.is("READONLY")
        {
            i += 1;
            continue;
        }
        body.push(t.clone());
        i += 1;
    }
    if body.is_empty() {
        return None;
    }
    let (name, ty_toks): (String, &[Tok]) = if body.len() >= 2 {
        match &body[0] {
            Tok::Word(w, _) => (w.trim_start_matches('@').to_ascii_uppercase(), &body[1..]),
            Tok::P(_) => (String::new(), &body[..]),
        }
    } else {
        (String::new(), &body[..])
    };
    Some(ArgDecl {
        name,
        mode,
        ty: type_text(ty_toks),
    })
}

const RET_STOP: &[&str] = &[
    "IS",
    "AS",
    "BEGIN",
    "LANGUAGE",
    "DETERMINISTIC",
    "PIPELINED",
    "PARALLEL_ENABLE",
    "RESULT_CACHE",
    "AUTHID",
    "WITH",
    "NOT",
    "READS",
    "MODIFIES",
    "CONTAINS",
    "SQL",
    "COMMENT",
    "RETURNS",
    "RETURN",
    "DECLARE",
    "CALLED",
    "STRICT",
    "IMMUTABLE",
    "STABLE",
    "VOLATILE",
    "SECURITY",
    "SET",
    "EXTERNAL",
    "NO",
];

/// 반환 타입: `RETURN`/`RETURNS` 뒤 낱말들(바로 이어지는 괄호 묶음 포함 · `SETOF` 유지) · 없으면 None.
fn return_type(toks: &[Tok], from: usize) -> Option<String> {
    let at = (from..toks.len()).find(|&i| toks[i].is("RETURN") || toks[i].is("RETURNS"))?;
    let mut i = at + 1;
    let mut parts: Vec<Tok> = Vec::new();
    while i < toks.len() {
        match &toks[i] {
            Tok::P(';') | Tok::P('$') | Tok::P('{') => break,
            Tok::P('(') => {
                let (items, end) = paren_items(toks, i)?;
                parts.push(Tok::P('('));
                for (k, it) in items.iter().enumerate() {
                    if k > 0 {
                        parts.push(Tok::P(','));
                    }
                    parts.extend(it.iter().cloned());
                }
                parts.push(Tok::P(')'));
                i = end;
                continue;
            }
            t if t.word().is_some_and(|w| RET_STOP.contains(&w))
                && !matches!(t, Tok::Word(_, true)) =>
            {
                break;
            }
            t => parts.push(t.clone()),
        }
        i += 1;
    }
    (!parts.is_empty()).then(|| type_text(&parts))
}

/// CREATE 문에서 선언을 뽑는다 — 종류별로 머리만.
pub(crate) fn extract(kind: ObjectKind, text: &str) -> Option<Decl> {
    let toks = tokenize(text);
    let kind_word = match kind {
        ObjectKind::Table => "TABLE",
        ObjectKind::View | ObjectKind::MaterializedView => "VIEW",
        ObjectKind::Procedure => "PROCEDURE",
        ObjectKind::Function => "FUNCTION",
        _ => return None,
    };
    let create = toks.iter().position(|t| t.is("CREATE"))?;
    let kw = (create..toks.len()).find(|&i| toks[i].is(kind_word))?;
    // 이름 = 종류 낱말 뒤 낱말들(`IF NOT EXISTS` 건너뜀) · `(`/AS/IS/RETURN(S)/`@`로 시작하는 인자 전까지.
    let mut i = kw + 1;
    while i < toks.len() && (toks[i].is("IF") || toks[i].is("NOT") || toks[i].is("EXISTS")) {
        i += 1;
    }
    while i < toks.len() {
        match &toks[i] {
            Tok::P('(') => break,
            Tok::Word(w, false)
                if matches!(
                    w.as_str(),
                    "AS" | "IS" | "RETURN" | "RETURNS" | "WITH" | "BEGIN"
                ) || w.starts_with('@') =>
            {
                break
            }
            _ => i += 1,
        }
    }
    match kind {
        ObjectKind::Table => {
            let (items, _) = paren_items(&toks, i)?;
            let cols: Vec<ColDecl> = items.iter().filter_map(|it| col_decl(it)).collect();
            (!cols.is_empty()).then_some(Decl::Table(cols))
        }
        ObjectKind::View | ObjectKind::MaterializedView => {
            let (items, _) = paren_items(&toks, i)?;
            let cols: Vec<ColDecl> = items
                .iter()
                .filter_map(|it| {
                    it.first().and_then(Tok::word).map(|w| ColDecl {
                        name: w.to_ascii_uppercase(),
                        ty: String::new(),
                        nullable: true,
                    })
                })
                .collect();
            (!cols.is_empty()).then_some(Decl::Table(cols))
        }
        _ => {
            let (args, after) = if toks.get(i) == Some(&Tok::P('(')) {
                let (items, end) = paren_items(&toks, i)?;
                (items.iter().filter_map(|it| arg_decl(it)).collect(), end)
            } else if toks
                .get(i)
                .is_some_and(|t| t.word().is_some_and(|w| w.starts_with('@')))
            {
                // SQL Server `CREATE PROCEDURE p @a int, @b int OUTPUT AS …`(괄호 없음) — AS/WITH/FOR 전까지 최상위 `,` 분리.
                let mut items: Vec<Vec<Tok>> = vec![Vec::new()];
                let mut depth = 0usize;
                let mut j = i;
                while j < toks.len() {
                    match &toks[j] {
                        Tok::P('(') => {
                            depth += 1;
                            items.last_mut()?.push(toks[j].clone());
                        }
                        Tok::P(')') => {
                            depth = depth.saturating_sub(1);
                            items.last_mut()?.push(toks[j].clone());
                        }
                        Tok::P(',') if depth == 0 => items.push(Vec::new()),
                        t if depth == 0 && (t.is("AS") || t.is("WITH") || t.is("FOR")) => break,
                        t => items.last_mut()?.push(t.clone()),
                    }
                    j += 1;
                }
                (items.iter().filter_map(|it| arg_decl(it)).collect(), j)
            } else {
                (Vec::new(), i)
            };
            let ret = if kind == ObjectKind::Function {
                return_type(&toks, after)
            } else {
                None
            };
            Some(Decl::Routine { args, ret })
        }
    }
}

/// 선언 차이(`a` = 편집기 · `b` = 서버). 종류가 다르면 None.
pub(crate) fn diff(a: &Decl, b: &Decl) -> Option<DeclDiff> {
    let mut d = DeclDiff::default();
    match (a, b) {
        (Decl::Table(ca), Decl::Table(cb)) => {
            let find = |cols: &[ColDecl], n: &str| cols.iter().position(|c| c.name == n);
            for c in ca {
                match find(cb, &c.name) {
                    None => d.added.push(c.name.clone()),
                    Some(j) => {
                        let o = &cb[j];
                        // 뷰 열 목록(타입 없음)은 이름만 견준다.
                        if !c.ty.is_empty()
                            && !o.ty.is_empty()
                            && (c.ty != o.ty || c.nullable != o.nullable)
                        {
                            let f = |x: &ColDecl| {
                                format!("{}{}", x.ty, if x.nullable { "" } else { " NOT NULL" })
                            };
                            d.changed.push((c.name.clone(), f(c), f(o)));
                        }
                    }
                }
            }
            for o in cb {
                if find(ca, &o.name).is_none() {
                    d.removed.push(o.name.clone());
                }
            }
            let common_a: Vec<&str> = ca
                .iter()
                .filter(|c| find(cb, &c.name).is_some())
                .map(|c| c.name.as_str())
                .collect();
            let common_b: Vec<&str> = cb
                .iter()
                .filter(|c| find(ca, &c.name).is_some())
                .map(|c| c.name.as_str())
                .collect();
            d.reordered = common_a != common_b;
        }
        (Decl::Routine { args: aa, ret: ra }, Decl::Routine { args: ab, ret: rb }) => {
            let named =
                aa.iter().all(|x| !x.name.is_empty()) && ab.iter().all(|x| !x.name.is_empty());
            let label = |x: &ArgDecl, pos: usize| {
                if x.name.is_empty() {
                    format!("${}", pos + 1)
                } else {
                    x.name.clone()
                }
            };
            let desc = |x: &ArgDecl| format!("{} {}", x.mode, x.ty);
            if named {
                let find = |args: &[ArgDecl], n: &str| args.iter().position(|x| x.name == n);
                for x in aa {
                    match find(ab, &x.name) {
                        None => d.added.push(x.name.clone()),
                        Some(j) => {
                            let o = &ab[j];
                            if x.mode != o.mode || x.ty != o.ty {
                                d.changed.push((x.name.clone(), desc(x), desc(o)));
                            }
                        }
                    }
                }
                for o in ab {
                    if find(aa, &o.name).is_none() {
                        d.removed.push(o.name.clone());
                    }
                }
                let ca: Vec<&str> = aa
                    .iter()
                    .filter(|x| find(ab, &x.name).is_some())
                    .map(|x| x.name.as_str())
                    .collect();
                let cb: Vec<&str> = ab
                    .iter()
                    .filter(|x| find(aa, &x.name).is_some())
                    .map(|x| x.name.as_str())
                    .collect();
                d.reordered = ca != cb;
            } else {
                // 이름 없는 인자(PG `f(integer, text)`) = 자리로 견준다.
                let n = aa.len().min(ab.len());
                for k in 0..n {
                    if aa[k].mode != ab[k].mode || aa[k].ty != ab[k].ty {
                        d.changed
                            .push((label(&aa[k], k), desc(&aa[k]), desc(&ab[k])));
                    }
                }
                for (k, x) in aa.iter().enumerate().skip(n) {
                    d.added.push(label(x, k));
                }
                for (k, x) in ab.iter().enumerate().skip(n) {
                    d.removed.push(label(x, k));
                }
            }
            if ra != rb {
                d.ret = Some((
                    ra.clone().unwrap_or_default(),
                    rb.clone().unwrap_or_default(),
                ));
            }
        }
        _ => return None,
    }
    Some(d)
}

/// 둘을 한 번에: `(선언 종류가 테이블인가, 차이)` · 어느 쪽이든 못 읽으면 None.
pub(crate) fn compare(kind: ObjectKind, editor: &str, server: &str) -> Option<(bool, DeclDiff)> {
    let a = extract(kind, editor)?;
    let b = extract(kind, server)?;
    let table = matches!(a, Decl::Table(_));
    diff(&a, &b).map(|d| (table, d))
}

#[cfg(test)]
mod tests {
    // 시험 = 실패가 곧 판정(history.rs·backups.rs 시험 모듈과 같은 관례).
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn cols(d: &Decl) -> Vec<(&str, &str, bool)> {
        match d {
            Decl::Table(c) => c
                .iter()
                .map(|x| (x.name.as_str(), x.ty.as_str(), x.nullable))
                .collect(),
            Decl::Routine { .. } => panic!("table expected"),
        }
    }

    #[test]
    fn table_columns_oracle_and_pg_forms() {
        let ora = "CREATE TABLE \"HR\".\"EMP\" (\n  \"ID\" NUMBER(10,0) NOT NULL ENABLE,\n  \"NAME\" VARCHAR2(30 BYTE),\n  HIRED DATE DEFAULT SYSDATE,\n  CONSTRAINT PK_EMP PRIMARY KEY (\"ID\")\n) SEGMENT CREATION IMMEDIATE;";
        let d = extract(ObjectKind::Table, ora).expect("table");
        assert_eq!(
            cols(&d),
            vec![
                ("ID", "NUMBER(10,0)", false),
                ("NAME", "VARCHAR2(30)", true),
                ("HIRED", "DATE", true)
            ]
        );
        let pg = "create table hr.emp (id int primary key, name varchar(30), hired date default now(), check (id > 0));";
        let d2 = extract(ObjectKind::Table, pg).expect("table");
        assert_eq!(
            cols(&d2),
            vec![
                ("ID", "INTEGER", false),
                ("NAME", "VARCHAR(30)", true),
                ("HIRED", "DATE", true)
            ]
        );
        // MySQL 백틱 · AUTO_INCREMENT · KEY 항목 제외.
        let my = "CREATE TABLE `emp` (`id` int NOT NULL AUTO_INCREMENT, `name` varchar(30) DEFAULT NULL, PRIMARY KEY (`id`), KEY ix (`name`)) ENGINE=InnoDB";
        assert_eq!(
            cols(&extract(ObjectKind::Table, my).expect("table")),
            vec![("ID", "INTEGER", false), ("NAME", "VARCHAR(30)", true)]
        );
        assert!(extract(ObjectKind::Table, "SELECT 1").is_none());
    }

    #[test]
    fn table_diff_added_removed_changed_reordered() {
        let a = extract(
            ObjectKind::Table,
            "CREATE TABLE t (b int, a varchar(10) not null, c date)",
        )
        .unwrap();
        let b = extract(
            ObjectKind::Table,
            "CREATE TABLE t (a varchar(20), b int, d int)",
        )
        .unwrap();
        let d = diff(&a, &b).unwrap();
        assert_eq!(d.added, vec!["C"]);
        assert_eq!(d.removed, vec!["D"]);
        assert_eq!(
            d.changed,
            vec![(
                "A".to_string(),
                "VARCHAR(10) NOT NULL".to_string(),
                "VARCHAR(20)".to_string()
            )]
        );
        assert!(d.reordered);
        assert!(!d.is_empty());
        assert!(diff(&a, &a).unwrap().is_empty());
        // 뷰 열 목록 = 이름만.
        let v = extract(
            ObjectKind::View,
            "CREATE OR REPLACE VIEW v (x, y) AS SELECT 1, 2",
        )
        .unwrap();
        assert_eq!(cols(&v), vec![("X", "", true), ("Y", "", true)]);
        assert!(
            extract(ObjectKind::View, "CREATE VIEW v AS SELECT 1 x").is_none(),
            "열 목록 없는 뷰 = 비교 불가"
        );
    }

    #[test]
    fn routine_args_four_dialects_and_return() {
        let ora = "CREATE OR REPLACE PROCEDURE hr.p (p_id IN NUMBER, p_name IN OUT VARCHAR2, p_cnt OUT NUMBER DEFAULT 0) IS BEGIN NULL; END;";
        let Some(Decl::Routine { args, ret }) = extract(ObjectKind::Procedure, ora) else {
            panic!()
        };
        assert_eq!(
            args.iter()
                .map(|a| (a.name.as_str(), a.mode.as_str(), a.ty.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("P_ID", "IN", "NUMBER"),
                ("P_NAME", "INOUT", "VARCHAR2"),
                ("P_CNT", "OUT", "NUMBER")
            ]
        );
        assert!(ret.is_none());
        let pg = "CREATE OR REPLACE FUNCTION public.f(a integer, OUT b text, VARIADIC c int[] = '{}') RETURNS SETOF integer LANGUAGE sql AS $$ select 1 $$;";
        let Some(Decl::Routine { args, ret }) = extract(ObjectKind::Function, pg) else {
            panic!()
        };
        assert_eq!(
            args.iter()
                .map(|a| (a.name.as_str(), a.mode.as_str(), a.ty.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("A", "IN", "INTEGER"),
                ("B", "OUT", "TEXT"),
                ("C", "IN", "INTEGER[]")
            ]
        );
        assert_eq!(ret.as_deref(), Some("SETOF INTEGER"));
        let ms =
            "CREATE PROCEDURE dbo.p @a int, @b nvarchar(30) = N'x' OUTPUT AS BEGIN SELECT 1 END";
        let Some(Decl::Routine { args, .. }) = extract(ObjectKind::Procedure, ms) else {
            panic!()
        };
        assert_eq!(
            args.iter()
                .map(|a| (a.name.as_str(), a.mode.as_str(), a.ty.as_str()))
                .collect::<Vec<_>>(),
            vec![("A", "IN", "INTEGER"), ("B", "OUT", "NVARCHAR(30)")]
        );
        let my = "CREATE FUNCTION f(a INT, b VARCHAR(10)) RETURNS INT DETERMINISTIC RETURN a;";
        let Some(Decl::Routine { args, ret }) = extract(ObjectKind::Function, my) else {
            panic!()
        };
        assert_eq!(args.len(), 2);
        assert_eq!(ret.as_deref(), Some("INTEGER"));
        // 이름 없는 PG 인자 = 자리 비교 · 반환 타입 차이.
        let x = extract(
            ObjectKind::Function,
            "CREATE FUNCTION f(integer, text) RETURNS int AS $$ $$",
        )
        .unwrap();
        let y = extract(
            ObjectKind::Function,
            "CREATE FUNCTION f(integer) RETURNS bigint AS $$ $$",
        )
        .unwrap();
        let d = diff(&x, &y).unwrap();
        assert_eq!(d.added, vec!["$2"]);
        assert_eq!(d.ret, Some(("INTEGER".to_string(), "BIGINT".to_string())));
        assert!(compare(ObjectKind::Table, "CREATE TABLE t (a int)", "garbage").is_none());
    }
}
