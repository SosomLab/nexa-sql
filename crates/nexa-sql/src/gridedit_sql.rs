//! ★ 그리드 편집의 nexa-sql 어댑터 — 편집 가능 판정 · 변경 집합 → 바인드 문장(`SqlGen`) · 미리보기 리터럴(docs/87 §7 · T-182).
//!
//! 부품(`nexa_ctl::gridedit`)은 값을 `Option<String>`으로만 안다. 여기서 ① 출처 문장을 분석해 단일 테이블·컬럼 목록을 뽑고
//! ② 셀 명세(`CellSpec`)에 따라 문자열을 `Value`+`VarType` 바인드로 바꾸며 ③ 방언 능력표(`Caps.marker`)로 자리 표시를 쓴다.
//! 서버로 가는 문장은 **여기 한 곳**에서만 만든다(77 §3 2번).

use nexa_ctl::gridedit::{CellKind, CellSpec, ChangeSet, RowRef};
use nsql_core::{BindParam, Caps, Dialect, Direction, ExecRequest, Marker, Value, VarType};
use nsql_i18n::{t, tf, Msg};

/// 읽기 전용인 이유(상태줄 안내 · 87 §7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReadOnly {
    /// 결과가 조회 문장 하나에서 오지 않았다(스크립트·DML·커서 …).
    NotQuery,
    NoTable,
    MultiTable,
    Aggregate,
    Distinct,
    SetOp,
    Subquery,
    /// 선택 항목이 열 이름이 아니다(식 · 별칭).
    Expression,
    /// 설정 `grid.edit` 꺼짐.
    Disabled,
    /// 운영(PROD) 접속 — `tx.prod_*` 규칙.
    Prod,
    /// 행을 식별할 길이 없다(키 없음 · 물리 식별자 없음 · 비교 가능한 열 없음 · 87 §13).
    NoKey,
}

impl ReadOnly {
    pub(crate) fn text(&self) -> String {
        let m = match self {
            ReadOnly::NotQuery => Msg::GeRoNotQuery,
            ReadOnly::NoTable => Msg::GeRoNoTable,
            ReadOnly::MultiTable => Msg::GeRoMultiTable,
            ReadOnly::Aggregate => Msg::GeRoAggregate,
            ReadOnly::Distinct => Msg::GeRoDistinct,
            ReadOnly::SetOp => Msg::GeRoSetOp,
            ReadOnly::Subquery => Msg::GeRoSubquery,
            ReadOnly::Expression => Msg::GeRoExpression,
            ReadOnly::Disabled => Msg::GeRoDisabled,
            ReadOnly::Prod => Msg::GeRoProd,
            ReadOnly::NoKey => Msg::GeRoNoKey,
        };
        t(m).to_string()
    }
}

/// 편집 대상 — 출처 문장의 테이블 표기(사용자가 쓴 그대로) · `SELECT *`인가 · 아니면 선택한 열 이름(쓴 그대로).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditTarget {
    pub table: String,
    pub star: bool,
    pub cols: Vec<String>,
    /// FROM 절의 테이블 별칭(대문자 · 인용 별칭은 None) — 숨은 열 주입의 한정자(87 §13 · T-231).
    pub alias: Option<String>,
}

/// 주석·문자열을 공백으로 지운 대문자 사본(구조 판정용 · 길이 유지 안 함).
fn skeleton(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let b: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '-' && b.get(i + 1) == Some(&'-') {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            out.push(' ');
            continue;
        }
        if c == '/' && b.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            out.push(' ');
            continue;
        }
        if c == '\'' {
            i += 1;
            while i < b.len() {
                if b[i] == '\'' {
                    if b.get(i + 1) == Some(&'\'') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            i += 1;
            out.push_str("'S'");
            continue;
        }
        out.push(c.to_ascii_uppercase());
        i += 1;
    }
    out
}

fn is_ident(t: &str) -> bool {
    !t.is_empty()
        && t.chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '"' | '[' | '`'))
        && t.chars().all(|c| {
            c.is_alphanumeric() || matches!(c, '_' | '.' | '$' | '#' | '"' | '[' | ']' | '`')
        })
        && !t.starts_with('.')
}

/// 출처 문장이 **단일 테이블 SELECT**(JOIN·GROUP·DISTINCT·집합·서브쿼리 FROM 없음 · 선택 항목 = `*` 또는 열 이름)인가.
pub(crate) fn analyze(sql: &str) -> Result<EditTarget, ReadOnly> {
    let sk = skeleton(sql);
    let toks: Vec<&str> = sk
        .split(|c: char| c.is_whitespace() || c == ';')
        .filter(|t| !t.is_empty())
        .collect();
    if toks.first().copied() != Some("SELECT") {
        return Err(ReadOnly::NotQuery);
    }
    // 금지 낱말(FROM 뒤든 앞이든).
    for w in toks.iter().skip(1) {
        match *w {
            "DISTINCT" => return Err(ReadOnly::Distinct),
            "UNION" | "INTERSECT" | "EXCEPT" | "MINUS" => return Err(ReadOnly::SetOp),
            "JOIN" | "HAVING" => return Err(ReadOnly::MultiTable),
            _ => {}
        }
    }
    if sk.contains("GROUP BY") {
        return Err(ReadOnly::Aggregate);
    }
    let from_at = toks
        .iter()
        .position(|t| *t == "FROM")
        .ok_or(ReadOnly::NoTable)?;
    // 선택 목록(SELECT ~ FROM).
    let sel = toks[1..from_at].join(" ");
    let star = sel == "*" || sel.ends_with(".*") && !sel.contains(',');
    let mut cols = Vec::new();
    if !star {
        if sel.contains('(') {
            return Err(ReadOnly::Expression);
        }
        for item in sel.split(',') {
            let parts: Vec<&str> = item.split_whitespace().collect();
            if parts.len() != 1 || !is_ident(parts[0]) || parts[0] == "*" {
                return Err(ReadOnly::Expression);
            }
            // `A.COL` → `COL`(단일 테이블이라 한정자는 버린다).
            let name = parts[0].rsplit('.').next().unwrap_or(parts[0]).to_string();
            cols.push(name);
        }
    }
    // FROM 뒤: 테이블 [별칭] 그리고 WHERE/ORDER/FETCH/LIMIT/끝 — `(`·`,` 는 거부.
    let after = &toks[from_at + 1..];
    let Some(table) = after.first() else {
        return Err(ReadOnly::NoTable);
    };
    if table.starts_with('(') {
        return Err(ReadOnly::Subquery);
    }
    if !is_ident(table.trim_end_matches(',')) || *table == "DUAL" {
        return Err(ReadOnly::NoTable);
    }
    if table.ends_with(',') {
        return Err(ReadOnly::MultiTable);
    }
    let mut j = 1;
    let mut alias: Option<String> = None;
    // 별칭(키워드가 아니면).
    if let Some(a) = after.get(j) {
        if !matches!(
            *a,
            "WHERE" | "ORDER" | "FETCH" | "LIMIT" | "OFFSET" | "FOR" | "CONNECT" | "START" | "WITH"
        ) {
            if a.starts_with(',') || a.ends_with(',') {
                return Err(ReadOnly::MultiTable);
            }
            if !is_ident(a) {
                return Err(ReadOnly::Subquery);
            }
            // `AS 별칭`이면 다음 토큰이 별칭.
            let mut a = *a;
            if a == "AS" {
                j += 1;
                match after.get(j) {
                    Some(n) if is_ident(n) && !n.contains(',') => a = n,
                    _ => return Err(ReadOnly::Subquery),
                }
            }
            if !a.contains(['"', '[', '`']) {
                alias = Some(a.to_string());
            }
            j += 1;
        }
    }
    if let Some(a) = after.get(j) {
        if a.starts_with(',') {
            return Err(ReadOnly::MultiTable);
        }
        if *a == "WITH" {
            // `WITH (NOLOCK)`은 허용 · `WITH CHECK`… 는 아님 — 간단히 허용.
        }
    }
    // FROM 절 뒤에 서브쿼리가 있어도(WHERE IN (SELECT …)) 편집에는 무관.
    // 원문의 테이블 표기(대소문자·인용 그대로) = 같은 위치의 원문 토큰.
    let raw_table = nsql_io::guess_table(sql).ok_or(ReadOnly::NoTable)?;
    Ok(EditTarget {
        table: raw_table,
        star,
        cols,
        alias,
    })
}

/// 원문에서 최상위(괄호 깊이 0 · 주석·문자열·인용 밖) `FROM` 키워드의 바이트 위치.
fn find_top_from(sql: &str) -> Option<usize> {
    let b = sql.as_bytes();
    let mut i = 0;
    let mut depth = 0i32;
    while i < b.len() {
        let c = b[i];
        match c {
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
                continue;
            }
            b'\'' | b'"' | b'`' => {
                i += 1;
                while i < b.len() && b[i] != c {
                    i += 1;
                }
                i += 1;
                continue;
            }
            b'[' => {
                while i < b.len() && b[i] != b']' {
                    i += 1;
                }
                i += 1;
                continue;
            }
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0
            && (c == b'F' || c == b'f')
            && sql[i..].len() >= 4
            && sql[i..i + 4].eq_ignore_ascii_case("from")
            && (i == 0 || !is_word_byte(b[i - 1]))
            && b.get(i + 4).is_none_or(|n| !is_word_byte(*n))
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn is_word_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c == b'#'
}

/// 원문 첫 `SELECT` 키워드의 끝 위치(앞 공백·주석은 건너뜀).
fn select_end(sql: &str) -> Option<usize> {
    let b = sql.as_bytes();
    let mut i = 0;
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if b.get(i) == Some(&b'-') && b.get(i + 1) == Some(&b'-') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b.get(i) == Some(&b'/') && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        break;
    }
    if sql.len() >= i + 6 && sql[i..i + 6].eq_ignore_ascii_case("select") {
        Some(i + 6)
    } else {
        None
    }
}

/// 결과 출처 문장과 우리가 보낸 재조회 문장이 같은가(러너가 끝 `;`·공백을 다듬는다).
pub(crate) fn same_stmt(a: &str, b: &str) -> bool {
    let n = |s: &str| s.trim().trim_end_matches(';').trim().to_string();
    n(a) == n(b)
}

/// 숨은 열(재조회 주입 · 87 §13-1) — 키 열은 카탈로그 이름(인용) · 물리 식별자는 방언 표현 그대로.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HiddenCol {
    Key(String),
    /// `expr` = 문장에 넣는 식(`ROWID` · `ctid`) · `name` = 결과 열 이름 · `cast` = PG처럼 바인드를 문자열로 받아 캐스트해야 할 타입.
    Physical {
        expr: &'static str,
        name: &'static str,
        cast: Option<&'static str>,
    },
}

/// 방언별 물리 행 식별자(87 §13-3 · D-217 PG = ctid+xmin · D-218 SQL Server 없음).
pub(crate) fn physical_cols(d: Dialect) -> Vec<HiddenCol> {
    match d {
        Dialect::Oracle => vec![HiddenCol::Physical {
            expr: "ROWID",
            name: "ROWID",
            cast: None,
        }],
        Dialect::Sqlite => vec![HiddenCol::Physical {
            expr: "rowid",
            name: "rowid",
            cast: None,
        }],
        Dialect::Postgres => vec![
            HiddenCol::Physical {
                expr: "ctid",
                name: "ctid",
                cast: Some("tid"),
            },
            HiddenCol::Physical {
                expr: "xmin",
                name: "xmin",
                cast: Some("xid"),
            },
        ],
        _ => Vec::new(),
    }
}

/// 단일 테이블 조회 문장 끝에 숨은 열을 덧붙인 재조회 문장(원본 열 index는 그대로 · 숨은 열은 뒤에).
/// `SELECT *`는 Oracle이 `*` 뒤에 열을 허용하지 않아 `<별칭|테이블>.*`로 바꾼다. 인용 별칭·구조를 못 찾으면 None(→ 3급/읽기 전용).
pub(crate) fn inject(
    sql: &str,
    target: &EditTarget,
    d: Dialect,
    hidden: &[HiddenCol],
) -> Option<String> {
    if hidden.is_empty() {
        return None;
    }
    let se = select_end(sql)?;
    let from = find_top_from(sql)?;
    if from <= se {
        return None;
    }
    let list = sql[se..from].trim();
    if list.is_empty() {
        return None;
    }
    let qual = target.alias.clone().unwrap_or_else(|| target.table.clone());
    let list = if list == "*" {
        format!("{qual}.*")
    } else {
        list.to_string()
    };
    let exprs: Vec<String> = hidden
        .iter()
        .map(|h| match h {
            HiddenCol::Key(n) => q(d, n),
            HiddenCol::Physical { expr, .. } => match &target.alias {
                Some(a) => format!("{a}.{expr}"),
                None => expr.to_string(),
            },
        })
        .collect();
    Some(format!(
        "{} {}, {} {}",
        &sql[..se],
        list,
        exprs.join(", "),
        &sql[from..]
    ))
}

/// 열 하나의 편집 메타(결과 열 이름 + 명세).
#[derive(Clone, Debug)]
pub(crate) struct ColMeta {
    pub name: String,
    pub spec: CellSpec,
    /// 드라이버 타입 이름(3급 비교 가능 판정 · `editable::comparable`).
    pub type_name: String,
    /// 숨은 열(재조회로 주입 · 화면·복사·SET·INSERT 제외 · WHERE에만).
    pub hidden: bool,
    /// WHERE에 쓸 식(물리 식별자 · 인용하지 않음) — None이면 열 이름을 인용.
    pub where_expr: Option<String>,
    /// 바인드 캐스트(PG `($1::text)::tid`).
    pub bind_cast: Option<&'static str>,
}

impl ColMeta {
    pub(crate) fn new(
        name: impl Into<String>,
        spec: CellSpec,
        type_name: impl Into<String>,
    ) -> Self {
        ColMeta {
            name: name.into(),
            spec,
            type_name: type_name.into(),
            hidden: false,
            where_expr: None,
            bind_cast: None,
        }
    }

    /// 주입된 숨은 열로 표시(읽기 전용 · 물리 식별자면 WHERE 식·캐스트).
    pub(crate) fn mark_hidden(&mut self, h: &HiddenCol) {
        self.hidden = true;
        self.spec.read_only = true;
        if let HiddenCol::Physical { expr, cast, .. } = h {
            self.where_expr = Some((*expr).to_string());
            self.bind_cast = *cast;
        }
    }
}

/// 낙관적 동시성(87 §13-5 · `grid.edit_concurrency`): WHERE에 키 외에 무엇을 더 비교하나.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Concurrency {
    /// 키만.
    #[default]
    Key,
    /// 수정한 열의 옛 값도(같은 셀을 다른 세션이 바꿨으면 0행).
    KeyOld,
    /// 비교 가능한 모든 열의 옛 값.
    AllOld,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyKind {
    /// DBMS 물리 행 식별자(Oracle ROWID · SQLite rowid · PG ctid+xmin · 숨은 열 · 87 §13-3).
    Physical,
    /// PK 또는 유니크 제약(모든 키 열이 결과에 있다).
    Constraint,
    /// 키가 없어 **전체 열 = 값**(D-198 · 엄격 1행 검사).
    AllColumns,
}

pub(crate) struct GenInput<'a> {
    pub dialect: Dialect,
    pub table: &'a str,
    pub cols: &'a [ColMeta],
    /// 키 열(열 index).
    pub key_cols: &'a [usize],
    /// 낙관적 동시성(키 외 비교 열).
    pub concurrency: Concurrency,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StmtKind {
    Delete,
    Update,
    Insert,
}

#[derive(Clone, Debug)]
pub(crate) struct EditStmt {
    pub kind: StmtKind,
    pub row: RowRef,
    pub req: ExecRequest,
    /// 사람이 읽는 미리보기(바인드 → 리터럴).
    pub preview: String,
    /// ★ 사전 검사문(UPDATE/DELETE = 같은 WHERE의 `SELECT COUNT(*)` · 87 §14 데이터 보호 불변식) — INSERT는 None.
    pub guard: Option<ExecRequest>,
    /// 보고용 라벨(`UPDATE #3 (ID=5)`).
    pub label: String,
}

/// 문자열 셀 → 바인드 값. `Expr` = 바인드 대신 SQL 식(`now`/`today` 토큰).
enum Bound {
    Val(Value, VarType),
    Expr(String),
}

fn bound_of(v: &Option<String>, kind: CellKind, dialect: Dialect) -> Bound {
    let Some(s) = v else {
        let ty = match kind {
            CellKind::Date => VarType::Date,
            CellKind::DateTime => VarType::Timestamp,
            _ => VarType::Auto,
        };
        return Bound::Val(Value::Null, ty);
    };
    match kind {
        CellKind::Number => {
            if s.is_empty() {
                return Bound::Val(Value::Null, VarType::Auto);
            }
            if let Ok(i) = s.parse::<i64>() {
                Bound::Val(Value::Int(i), VarType::Auto)
            } else if s.contains(['e', 'E']) {
                s.parse::<f64>()
                    .map_or(Bound::Val(Value::Decimal(s.clone()), VarType::Auto), |f| {
                        Bound::Val(Value::Float(f), VarType::Auto)
                    })
            } else {
                Bound::Val(Value::Decimal(s.clone()), VarType::Auto)
            }
        }
        CellKind::Bool => Bound::Val(Value::Bool(s == "true"), VarType::Boolean),
        CellKind::Date | CellKind::DateTime | CellKind::Time => match s.as_str() {
            "now" => Bound::Expr(now_expr(dialect, kind == CellKind::Date)),
            "today" => Bound::Expr(now_expr(dialect, true)),
            "" => Bound::Val(Value::Null, VarType::Auto),
            _ => {
                let ty = match kind {
                    CellKind::Date => VarType::Date,
                    CellKind::DateTime => {
                        if s.contains('.') {
                            VarType::Timestamp
                        } else {
                            VarType::Date
                        }
                    }
                    _ => VarType::Auto,
                };
                Bound::Val(Value::Str(s.clone()), ty)
            }
        },
        _ => Bound::Val(Value::Str(s.clone()), VarType::Auto),
    }
}

/// 방언별 "지금"/"오늘" 식.
fn now_expr(dialect: Dialect, date_only: bool) -> String {
    match (dialect, date_only) {
        (Dialect::Oracle, false) => "SYSDATE".into(),
        (Dialect::Oracle, true) => "TRUNC(SYSDATE)".into(),
        (Dialect::Mssql, false) => "GETDATE()".into(),
        (Dialect::Mssql, true) => "CAST(GETDATE() AS date)".into(),
        (Dialect::Postgres, false) => "now()".into(),
        (Dialect::Postgres, true) => "CURRENT_DATE".into(),
        (Dialect::Mysql, false) => "NOW()".into(),
        (Dialect::Mysql, true) => "CURDATE()".into(),
        (Dialect::Sqlite, false) => "datetime('now','localtime')".into(),
        (Dialect::Sqlite, true) => "date('now','localtime')".into(),
        (Dialect::Odbc, false) => "CURRENT_TIMESTAMP".into(),
        (Dialect::Odbc, true) => "CURRENT_DATE".into(),
    }
}

/// 미리보기 리터럴 — 날짜는 방언의 확실한 표기(문자열 암묵 변환에 기대지 않는다).
fn literal(v: &Value, ty: VarType, dialect: Dialect) -> String {
    match (v, &ty) {
        (Value::Str(s), VarType::Date | VarType::Timestamp) => match dialect {
            Dialect::Oracle => {
                if s.contains('.') {
                    format!("TO_TIMESTAMP('{s}','YYYY-MM-DD HH24:MI:SS.FF')")
                } else if s.len() <= 10 {
                    format!("TO_DATE('{s}','YYYY-MM-DD')")
                } else {
                    format!("TO_DATE('{s}','YYYY-MM-DD HH24:MI:SS')")
                }
            }
            Dialect::Mssql => format!("'{}'", s.replacen(' ', "T", 1)),
            Dialect::Postgres => {
                if s.len() <= 10 {
                    format!("DATE '{s}'")
                } else {
                    format!("TIMESTAMP '{s}'")
                }
            }
            _ => format!("'{s}'"),
        },
        _ => v.to_sql_literal(dialect),
    }
}

/// 자리 표시 + 파라미터 모음(방언 능력표 · 이름은 `p1`, `p2` …).
struct Sink {
    marker: Marker,
    params: Vec<BindParam>,
    dialect: Dialect,
    preview: String,
    sql: String,
}

impl Sink {
    fn new(dialect: Dialect) -> Self {
        Sink {
            marker: Caps::of(dialect).marker,
            params: Vec::new(),
            dialect,
            preview: String::new(),
            sql: String::new(),
        }
    }
    fn text(&mut self, s: &str) {
        self.sql.push_str(s);
        self.preview.push_str(s);
    }
    fn value(&mut self, b: Bound) {
        self.value_cast(b, None);
    }
    /// 값 자리 표시(+ 캐스트: PG 물리 식별자는 문자열로 보내 `($1::text)::tid`).
    fn value_cast(&mut self, b: Bound, cast: Option<&str>) {
        match b {
            Bound::Expr(e) => self.text(&e),
            Bound::Val(v, ty) => {
                let n = self.params.len() + 1;
                // ★ 이름은 대문자(`P1`): Oracle 드라이버가 SQL의 자리 표시 이름을 대문자로 모아 `p.name`과 비교한다 — 소문자면
                //   "없는 이름"으로 건너뛰어 바인드 0개 → ORA-01008(사용자 09-26 실서버 로그).
                let name = format!("P{n}");
                let ph = match self.marker {
                    Marker::Named => format!(":{name}"),
                    Marker::AtName => format!("@{name}"),
                    Marker::DollarN => format!("${n}"),
                    Marker::Question => "?".into(),
                };
                let lit = literal(&v, ty.clone(), self.dialect);
                match cast {
                    Some(c) => {
                        self.sql.push_str(&format!("({ph}::text)::{c}"));
                        self.preview.push_str(&format!("({lit}::text)::{c}"));
                    }
                    None => {
                        self.sql.push_str(&ph);
                        self.preview.push_str(&lit);
                    }
                }
                self.params.push(BindParam {
                    name,
                    value: v,
                    ty,
                    direction: Direction::In,
                });
            }
        }
    }
    /// `col = 값` 또는 `col IS NULL`(키 비교).
    fn key_eq(&mut self, col: &str, b: Bound, cast: Option<&str>) {
        match b {
            Bound::Val(Value::Null, _) => self.text(&format!("{col} IS NULL")),
            other => {
                self.text(&format!("{col} = "));
                self.value_cast(other, cast);
            }
        }
    }
    fn finish(
        self,
        kind: StmtKind,
        row: RowRef,
        guard: Option<ExecRequest>,
        label: String,
    ) -> EditStmt {
        EditStmt {
            kind,
            row,
            req: ExecRequest {
                sql: self.sql,
                params: self.params,
            },
            preview: self.preview,
            guard,
            label,
        }
    }
}

fn q(dialect: Dialect, name: &str) -> String {
    nsql_catalog::quote_ident(dialect, name)
}

/// 원본 `Value` → 부품 문자열(편집 상자·키 비교용 · 그리드 표시 문자열과 같은 규칙 · NULL = None).
pub(crate) fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::Bytes(b) => Some(format!("<{} bytes>", b.len())),
        other => Some(other.display()),
    }
}

/// 변경 집합 → 문장 목록(DELETE → UPDATE → INSERT). `original(row, col)` = 원본 셀(키 비교 · 원본 값).
pub(crate) fn generate(
    inp: &GenInput<'_>,
    cs: &ChangeSet,
    original: &dyn Fn(usize, usize) -> Value,
) -> Result<Vec<EditStmt>, String> {
    let d = inp.dialect;
    let ncols = inp.cols.len();
    if inp.key_cols.is_empty() {
        return Err(t(Msg::GeNoKey).to_string());
    }
    for &k in inp.key_cols {
        if inp
            .cols
            .get(k)
            .is_some_and(|c| c.spec.kind == CellKind::Binary)
        {
            return Err(t(Msg::GeBinaryKey).to_string());
        }
    }
    // 동시성 비교 열(키 밖 · 비교 가능한 열만 · 87 §13-5): KeyOld = 그 행에서 수정한 열 · AllOld = 전부.
    let extra_cols = |row: usize| -> Vec<usize> {
        match inp.concurrency {
            Concurrency::Key => Vec::new(),
            Concurrency::KeyOld => {
                let mut v: Vec<usize> = cs
                    .edits()
                    .filter(|((r, c), _)| *r == row && *c < ncols)
                    .map(|((_, c), _)| *c)
                    .filter(|c| {
                        !inp.key_cols.contains(c) && crate::editable::comparable(d, &inp.cols[*c])
                    })
                    .collect();
                v.sort_unstable();
                v.dedup();
                v
            }
            Concurrency::AllOld => (0..ncols)
                .filter(|c| {
                    !inp.key_cols.contains(c) && crate::editable::comparable(d, &inp.cols[*c])
                })
                .collect(),
        }
    };
    let key_where = |sink: &mut Sink, row: usize, extra: &[usize]| {
        sink.text(" WHERE ");
        for (i, &k) in inp.key_cols.iter().chain(extra.iter()).enumerate() {
            if i > 0 {
                sink.text(" AND ");
            }
            let c = &inp.cols[k];
            let v = original(row, k);
            let b = match &v {
                Value::Null => Bound::Val(Value::Null, VarType::Auto),
                // 키 비교는 원본 타입 그대로 바인드(문자열이면 문자열 · 숫자면 숫자) — 날짜 열만 날짜 타입.
                v => {
                    let ty = match c.spec.kind {
                        CellKind::Date => VarType::Date,
                        CellKind::DateTime => VarType::Timestamp,
                        _ => VarType::Auto,
                    };
                    Bound::Val(v.clone(), ty)
                }
            };
            let lhs = c.where_expr.clone().unwrap_or_else(|| q(d, &c.name));
            sink.key_eq(&lhs, b, c.bind_cast);
        }
    };
    // 보고용 라벨: `UPDATE #행 (키=값, …)`.
    let key_text = |row: usize| -> String {
        inp.key_cols
            .iter()
            .filter_map(|&k| inp.cols.get(k).map(|c| (c, k)))
            .map(|(c, k)| format!("{}={}", c.name, original(row, k).display()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    // ★ 사전 검사문 = 같은 WHERE의 COUNT(*)(대상 행 수 = 1 · 87 §14).
    let guard_of = |row: usize, extra: &[usize]| -> ExecRequest {
        let mut g = Sink::new(d);
        g.text(&format!("SELECT COUNT(*) FROM {}", inp.table));
        key_where(&mut g, row, extra);
        ExecRequest {
            sql: g.sql,
            params: g.params,
        }
    };
    // ★ 유일성 사전 검사(D-215): 적용 대기 변경 안에서 키 튜플이 겹치면 거부(수정된 키 값 · 추가 행 · 그대로인 행).
    if !inp.key_cols.is_empty() {
        let key_tuple = |vals: &[Option<String>]| -> String {
            vals.iter()
                .map(|v| v.clone().unwrap_or_else(|| "\u{0}NULL".into()))
                .collect::<Vec<_>>()
                .join("\u{1}")
        };
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut check = |tuple: String, who: String| -> Result<(), String> {
            if tuple.contains("\u{0}NULL") {
                return Ok(()); // NULL 키는 유일성 비교 대상이 아니다(DBMS도 NULL ≠ NULL).
            }
            if let Some(prev) = seen.insert(tuple, who.clone()) {
                return Err(tf(Msg::StGeDupKey, &[&format!("{prev} ↔ {who}")]));
            }
            Ok(())
        };
        // 기존 행 = 수정된 키 값 반영(삭제 행 제외).
        let mut touched: Vec<usize> = cs.edits().map(|((r, _), _)| *r).collect();
        touched.dedup();
        let mut any_key_edit = false;
        for row in touched {
            if cs.is_deleted(RowRef::Existing(row)) {
                continue;
            }
            let vals: Vec<Option<String>> = inp
                .key_cols
                .iter()
                .map(|&k| match cs.cell(RowRef::Existing(row), k) {
                    Some(v) => {
                        any_key_edit = true;
                        v.clone()
                    }
                    None => value_text(&original(row, k)),
                })
                .collect();
            check(key_tuple(&vals), format!("#{}", row + 1))?;
        }
        let inserted: Vec<(usize, Vec<Option<String>>)> = cs
            .inserted_rows()
            .map(|(k, ir)| {
                (
                    k,
                    inp.key_cols
                        .iter()
                        .map(|&c| ir.cells.get(c).cloned().unwrap_or(None))
                        .collect(),
                )
            })
            .collect();
        for (k, vals) in &inserted {
            check(key_tuple(vals), format!("+{}", k + 1))?;
        }
        let _ = any_key_edit;
    }
    let mut out = Vec::new();
    // DELETE
    for row in cs.deleted_rows() {
        let mut s = Sink::new(d);
        let extra = if inp.concurrency == Concurrency::AllOld {
            extra_cols(row)
        } else {
            Vec::new()
        };
        s.text(&format!("DELETE FROM {}", inp.table));
        key_where(&mut s, row, &extra);
        let label = tf(
            Msg::GeGuardLabel,
            &["DELETE", &(row + 1).to_string(), &key_text(row)],
        );
        out.push(s.finish(
            StmtKind::Delete,
            RowRef::Existing(row),
            Some(guard_of(row, &extra)),
            label,
        ));
    }
    // UPDATE(행별 · 수정된 열만) — ★ 키 열을 바꾸는 행을 먼저(D-215 ② · 옮겨 간 자리에 새 값이 들어오는 경우를 통과).
    let mut rows: Vec<usize> = cs.edits().map(|((r, _), _)| *r).collect();
    rows.dedup();
    let key_edit = |row: usize| -> bool {
        cs.edits()
            .any(|((r, c), _)| *r == row && inp.key_cols.contains(c))
    };
    rows.sort_by_key(|&r| !key_edit(r));
    for row in rows {
        if cs.is_deleted(RowRef::Existing(row)) {
            continue;
        }
        let mut s = Sink::new(d);
        s.text(&format!("UPDATE {} SET ", inp.table));
        let mut first = true;
        for ((r, c), v) in cs.edits() {
            if *r != row || *c >= ncols {
                continue;
            }
            let col = &inp.cols[*c];
            if col.spec.read_only {
                continue;
            }
            if !first {
                s.text(", ");
            }
            first = false;
            s.text(&format!("{} = ", q(d, &col.name)));
            s.value(bound_of(v, col.spec.kind, d));
        }
        if first {
            continue;
        }
        let extra = extra_cols(row);
        key_where(&mut s, row, &extra);
        let label = tf(
            Msg::GeGuardLabel,
            &["UPDATE", &(row + 1).to_string(), &key_text(row)],
        );
        out.push(s.finish(
            StmtKind::Update,
            RowRef::Existing(row),
            Some(guard_of(row, &extra)),
            label,
        ));
    }
    // INSERT(살아 있는 추가 행 · 값 있는 열 + 기본값 없는 NULL 열).
    for (k, ir) in cs.inserted_rows() {
        let mut s = Sink::new(d);
        let mut names = Vec::new();
        let mut vals: Vec<Bound> = Vec::new();
        for (c, col) in inp.cols.iter().enumerate() {
            if col.spec.read_only {
                continue;
            }
            let cell = ir.cells.get(c).cloned().unwrap_or(None);
            if cell.is_none() && col.spec.default.is_some() {
                continue; // 서버 기본값
            }
            names.push(q(d, &col.name));
            vals.push(bound_of(&cell, col.spec.kind, d));
        }
        if names.is_empty() {
            continue;
        }
        s.text(&format!(
            "INSERT INTO {} ({}) VALUES (",
            inp.table,
            names.join(", ")
        ));
        for (i, b) in vals.into_iter().enumerate() {
            if i > 0 {
                s.text(", ");
            }
            s.value(b);
        }
        s.text(")");
        let label = tf(Msg::GeGuardLabel, &["INSERT", &format!("+{}", k + 1), ""]);
        out.push(s.finish(StmtKind::Insert, RowRef::Inserted(k), None, label));
    }
    Ok(out)
}

/// 미리보기 본문(문장마다 `;` 줄 · 머리 주석).
pub(crate) fn preview_text(stmts: &[EditStmt], table: &str, key_kind: KeyKind) -> String {
    let n = |k: StmtKind| stmts.iter().filter(|s| s.kind == k).count();
    let mut out = format!(
        "-- {} {} · {} {} (DELETE {} · UPDATE {} · INSERT {})\n",
        t(Msg::GePreviewTable),
        table,
        stmts.len(),
        t(Msg::GePreviewStmts),
        n(StmtKind::Delete),
        n(StmtKind::Update),
        n(StmtKind::Insert)
    );
    if key_kind == KeyKind::AllColumns {
        out.push_str(&format!("-- {}\n", t(Msg::GePreviewAllKey)));
    } else if key_kind == KeyKind::Physical {
        out.push_str(&format!("-- {}\n", t(Msg::GePreviewRowid)));
    }
    for s in stmts {
        out.push_str(&s.preview);
        out.push_str(";\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_single_table_only() {
        let ok =
            analyze("select * from EMP e where e.deptno = 10 order by 1").expect("single table");
        assert_eq!(ok.table, "EMP");
        assert!(ok.star);
        let ok = analyze("SELECT a.id, a.name FROM scott.emp a").expect("cols");
        assert_eq!(ok.cols, vec!["ID", "NAME"]);
        assert_eq!(ok.table, "scott.emp");
        assert_eq!(analyze("select * from a, b"), Err(ReadOnly::MultiTable));
        assert_eq!(
            analyze("select * from a join b on a.x=b.x"),
            Err(ReadOnly::MultiTable)
        );
        assert_eq!(analyze("select distinct x from a"), Err(ReadOnly::Distinct));
        assert_eq!(
            analyze("select x from a union select y from b"),
            Err(ReadOnly::SetOp)
        );
        assert_eq!(analyze("select count(*) from a"), Err(ReadOnly::Expression));
        assert_eq!(analyze("select x as y from a"), Err(ReadOnly::Expression));
        assert_eq!(
            analyze("select x from (select 1 x from dual)"),
            Err(ReadOnly::Subquery)
        );
        assert_eq!(
            analyze("select x from a group by x"),
            Err(ReadOnly::Aggregate)
        );
        assert_eq!(analyze("update a set x = 1"), Err(ReadOnly::NotQuery));
        assert_eq!(analyze("select 1 from dual"), Err(ReadOnly::Expression));
        // 주석·문자열 안의 낱말은 무시.
        let ok =
            analyze("select * from t -- join nothing\n where s = 'a,b union'").expect("comment");
        assert_eq!(ok.table, "t");
        // WHERE 안 서브쿼리는 허용.
        assert!(analyze("select * from t where id in (select id from u)").is_ok());
    }

    fn cols() -> Vec<ColMeta> {
        vec![
            ColMeta::new(
                "ID",
                CellSpec::new("ID", CellKind::Number).not_null(),
                "NUMBER",
            ),
            ColMeta::new("NAME", CellSpec::text("NAME").max_len(10), "VARCHAR2(10)"),
            ColMeta::new("DT", CellSpec::new("DT", CellKind::DateTime), "DATE"),
            ColMeta::new(
                "MEMO",
                CellSpec::text("MEMO").default_expr("'x'"),
                "VARCHAR2(100)",
            ),
        ]
    }

    fn orig(row: usize, col: usize) -> Value {
        match col {
            0 => Value::Int(row as i64 + 1),
            1 => Value::Str(format!("n{row}")),
            2 => Value::Str("2026-01-01 00:00:00".into()),
            _ => Value::Null,
        }
    }

    #[test]
    fn generate_oracle_update_delete_insert() {
        let cols = cols();
        let mut cs = ChangeSet::new(4);
        cs.set_cell(
            RowRef::Existing(0),
            1,
            Some("kim".into()),
            Some(&Some("n0".into())),
        );
        cs.set_cell(
            RowRef::Existing(0),
            2,
            Some("2026-09-26 14:05:07".into()),
            None,
        );
        cs.toggle_delete(RowRef::Existing(2));
        let k = cs.insert_row(
            None,
            vec![
                Some("9".into()),
                Some("new".into()),
                Some("now".into()),
                None,
            ],
        );
        let inp = GenInput {
            dialect: Dialect::Oracle,
            table: "EMP",
            cols: &cols,
            key_cols: &[0],
            concurrency: Concurrency::Key,
        };
        let st = generate(&inp, &cs, &orig).expect("generate");
        assert_eq!(st.len(), 3);
        assert_eq!(st[0].kind, StmtKind::Delete);
        assert_eq!(st[0].req.sql, "DELETE FROM EMP WHERE \"ID\" = :P1");
        assert_eq!(st[0].preview, "DELETE FROM EMP WHERE \"ID\" = 3");
        assert_eq!(
            st[0].guard.as_ref().map(|g| g.sql.as_str()),
            Some("SELECT COUNT(*) FROM EMP WHERE \"ID\" = :P1"),
            "사전 검사문 = 같은 WHERE의 COUNT"
        );
        assert_eq!(st[0].guard.as_ref().map(|g| g.params.len()), Some(1));
        assert!(st[0].label.starts_with("DELETE #3"));
        assert_eq!(st[1].kind, StmtKind::Update);
        assert_eq!(
            st[1].req.sql,
            "UPDATE EMP SET \"NAME\" = :P1, \"DT\" = :P2 WHERE \"ID\" = :P3"
        );
        assert_eq!(st[1].req.params.len(), 3);
        assert_eq!(st[1].req.params[1].ty, VarType::Date);
        assert_eq!(
            st[1].preview,
            "UPDATE EMP SET \"NAME\" = 'kim', \"DT\" = TO_DATE('2026-09-26 14:05:07','YYYY-MM-DD HH24:MI:SS') WHERE \"ID\" = 1"
        );
        assert_eq!(st[2].kind, StmtKind::Insert);
        assert_eq!(st[2].row, RowRef::Inserted(k));
        // MEMO = NULL + 기본값 있음 → 열 생략 · DT = now → SYSDATE 식(바인드 아님).
        assert_eq!(
            st[2].req.sql,
            "INSERT INTO EMP (\"ID\", \"NAME\", \"DT\") VALUES (:P1, :P2, SYSDATE)"
        );
        assert_eq!(st[2].req.params.len(), 2);
        assert_eq!(st[2].req.params[0].value, Value::Int(9));
        assert!(st[2].guard.is_none(), "INSERT는 사전 검사 없음");
    }

    /// 유일성 사전 검사(D-215): 키 값을 이미 있는 다른 행의 키로 바꾸면 적용 전에 거부 · NULL 키는 비교 제외.
    #[test]
    fn duplicate_key_among_pending_is_rejected() {
        let cols = cols();
        let inp = GenInput {
            dialect: Dialect::Sqlite,
            table: "t",
            cols: &cols,
            key_cols: &[0],
            concurrency: Concurrency::Key,
        };
        let mut cs = ChangeSet::new(4);
        // 행 0(ID=1)의 키를 2로 → 행 1(ID=2)과 충돌(행 1도 손댔을 때 비교 대상에 든다).
        cs.set_cell(
            RowRef::Existing(0),
            0,
            Some("2".into()),
            Some(&Some("1".into())),
        );
        cs.set_cell(RowRef::Existing(1), 1, Some("zz".into()), None);
        let err = generate(&inp, &cs, &orig).expect_err("dup");
        assert!(err.contains("#1") && err.contains("#2"), "{err}");
        // 추가 행이 기존 행 키와 같아도 거부.
        let mut cs2 = ChangeSet::new(4);
        cs2.set_cell(RowRef::Existing(2), 1, Some("q".into()), None);
        cs2.insert_row(None, vec![Some("3".into()), Some("n".into()), None, None]);
        assert!(generate(&inp, &cs2, &orig).is_err());
        // NULL 키는 비교 제외.
        let mut cs3 = ChangeSet::new(4);
        cs3.insert_row(None, vec![None, Some("a".into()), None, None]);
        cs3.insert_row(None, vec![None, Some("b".into()), None, None]);
        assert!(generate(&inp, &cs3, &orig).is_ok());
    }

    /// 별칭 포착(주입 한정자) · 인용 별칭은 None.
    #[test]
    fn analyze_alias() {
        assert_eq!(
            analyze("select a, b from t x where x.a = 1")
                .expect("ok")
                .alias,
            Some("X".into())
        );
        assert_eq!(analyze("select * from t").expect("ok").alias, None);
        assert_eq!(
            analyze("select * from t as q").expect("ok").alias,
            Some("Q".into())
        );
        assert_eq!(analyze("select * from t \"Q\"").expect("ok").alias, None);
    }

    /// 숨은 열 주입(87 §13 · T-231): 원본 열 뒤에 덧붙임 · `*`는 Oracle 규칙으로 `<별칭|테이블>.*` · 주석·문자열 안 FROM 무시 · PG = ctid+xmin.
    #[test]
    fn inject_hidden_columns() {
        let rid = physical_cols(Dialect::Oracle);
        let tg = analyze("select a, b from t").expect("ok");
        assert_eq!(
            inject(
                "select a, b from t",
                &tg,
                Dialect::Oracle,
                &[HiddenCol::Key("ID".into())]
            )
            .as_deref(),
            Some("select a, b, \"ID\" from t")
        );
        let tg = analyze("SELECT * FROM emp e WHERE x = 1").expect("ok");
        assert_eq!(
            inject(
                "SELECT * FROM emp e WHERE x = 1",
                &tg,
                Dialect::Oracle,
                &rid
            )
            .as_deref(),
            Some("SELECT E.*, E.ROWID FROM emp e WHERE x = 1")
        );
        let tg = analyze("select * from s.t").expect("ok");
        assert_eq!(
            inject("select * from s.t", &tg, Dialect::Oracle, &rid).as_deref(),
            Some("select s.t.*, ROWID from s.t")
        );
        let src =
            "/* c */ select a from t -- from x\n where n = 'x from y' and b in (select 1 from u)";
        let tg = analyze(src).expect("ok");
        assert_eq!(
            inject(src, &tg, Dialect::Oracle, &rid).as_deref(),
            Some("/* c */ select a, ROWID from t -- from x\n where n = 'x from y' and b in (select 1 from u)")
        );
        let tg = analyze("select a from t").expect("ok");
        assert_eq!(
            inject(
                "select a from t",
                &tg,
                Dialect::Postgres,
                &physical_cols(Dialect::Postgres)
            )
            .as_deref(),
            Some("select a, ctid, xmin from t")
        );
        assert_eq!(
            inject(
                "select a from t",
                &tg,
                Dialect::Sqlite,
                &physical_cols(Dialect::Sqlite)
            )
            .as_deref(),
            Some("select a, rowid from t")
        );
        assert!(physical_cols(Dialect::Mssql).is_empty(), "D-218");
        assert!(inject("select a from t", &tg, Dialect::Oracle, &[]).is_none());
        assert!(same_stmt(
            "select a, rowid from t;\n",
            " select a, rowid from t"
        ));
    }

    /// 물리 식별자 WHERE = 인용 없는 식 · PG는 문자열 바인드 + 캐스트 · 숨은 열은 SET/INSERT에 없다 · 사전 검사도 같은 WHERE.
    #[test]
    fn generate_physical_where_and_cast() {
        let mut cols = cols();
        let mut rid = ColMeta::new("ROWID", CellSpec::text("ROWID"), "ROWID");
        rid.mark_hidden(&physical_cols(Dialect::Oracle)[0]);
        cols.push(rid);
        let orig5 = |r: usize, c: usize| -> Value {
            if c == 4 {
                Value::Str("AAAB".into())
            } else {
                orig(r, c)
            }
        };
        let mut cs = ChangeSet::new(5);
        cs.set_cell(RowRef::Existing(0), 1, Some("kim".into()), None);
        cs.insert_row(
            None,
            vec![
                Some("9".into()),
                Some("n".into()),
                None,
                None,
                Some("ZZ".into()),
            ],
        );
        let inp = GenInput {
            dialect: Dialect::Oracle,
            table: "EMP",
            cols: &cols,
            key_cols: &[4],
            concurrency: Concurrency::Key,
        };
        let st = generate(&inp, &cs, &orig5).expect("generate");
        assert_eq!(
            st[0].req.sql,
            "UPDATE EMP SET \"NAME\" = :P1 WHERE ROWID = :P2"
        );
        assert_eq!(st[0].req.params[1].value, Value::Str("AAAB".into()));
        assert_eq!(
            st[0].guard.as_ref().map(|g| g.sql.as_str()),
            Some("SELECT COUNT(*) FROM EMP WHERE ROWID = :P1")
        );
        assert!(st[0].label.contains("ROWID=AAAB"));
        assert!(
            !st[1].req.sql.contains("ROWID"),
            "숨은 열은 INSERT에 없다: {}",
            st[1].req.sql
        );
        // PG: ctid + xmin · 캐스트.
        let mut pcols = self::tests::cols();
        for h in physical_cols(Dialect::Postgres) {
            let HiddenCol::Physical { name, .. } = &h else {
                unreachable!()
            };
            let mut c = ColMeta::new(*name, CellSpec::text(*name), *name);
            c.mark_hidden(&h);
            pcols.push(c);
        }
        let porig = |r: usize, c: usize| -> Value {
            match c {
                4 => Value::Str("(0,1)".into()),
                5 => Value::Int(777),
                _ => orig(r, c),
            }
        };
        let mut cs = ChangeSet::new(6);
        cs.set_cell(RowRef::Existing(0), 1, Some("kim".into()), None);
        let inp = GenInput {
            dialect: Dialect::Postgres,
            table: "emp",
            cols: &pcols,
            key_cols: &[4, 5],
            concurrency: Concurrency::Key,
        };
        let st = generate(&inp, &cs, &porig).expect("generate");
        assert_eq!(
            st[0].req.sql,
            "UPDATE emp SET \"NAME\" = $1 WHERE ctid = ($2::text)::tid AND xmin = ($3::text)::xid"
        );
        assert_eq!(
            st[0].preview,
            "UPDATE emp SET \"NAME\" = 'kim' WHERE ctid = ('(0,1)'::text)::tid AND xmin = (777::text)::xid"
        );
        let pv = preview_text(&st, "emp", KeyKind::Physical);
        assert!(pv.contains(&t(Msg::GePreviewRowid).to_string()));
    }

    /// D-215 ②: 키 열을 바꾸는 UPDATE가 먼저 · 동시성 = 수정 열/전 열의 옛 값을 WHERE에.
    #[test]
    fn key_edit_first_and_concurrency() {
        let cols = cols();
        let mut cs = ChangeSet::new(4);
        cs.set_cell(RowRef::Existing(0), 1, Some("a".into()), None); // 행 0 = 이름만
        cs.set_cell(RowRef::Existing(1), 0, Some("7".into()), None); // 행 1 = 키 변경
        let inp = GenInput {
            dialect: Dialect::Oracle,
            table: "EMP",
            cols: &cols,
            key_cols: &[0],
            concurrency: Concurrency::Key,
        };
        let st = generate(&inp, &cs, &orig).expect("generate");
        assert_eq!(st[0].row, RowRef::Existing(1), "키 변경 행 먼저");
        assert_eq!(st[1].row, RowRef::Existing(0));
        // KeyOld: 수정한 열(NAME)의 옛 값 추가 · 키 열 자체는 중복하지 않음.
        let inp = GenInput {
            concurrency: Concurrency::KeyOld,
            ..inp
        };
        let st = generate(&inp, &cs, &orig).expect("generate");
        assert_eq!(
            st[1].req.sql,
            "UPDATE EMP SET \"NAME\" = :P1 WHERE \"ID\" = :P2 AND \"NAME\" = :P3"
        );
        assert_eq!(st[1].req.params[2].value, Value::Str("n0".into()));
        assert_eq!(
            st[0].req.sql,
            "UPDATE EMP SET \"ID\" = :P1 WHERE \"ID\" = :P2"
        );
        assert_eq!(
            st[1].guard.as_ref().map(|g| g.sql.as_str()),
            Some("SELECT COUNT(*) FROM EMP WHERE \"ID\" = :P1 AND \"NAME\" = :P2")
        );
        // AllOld: 비교 가능한 모든 열(NULL = IS NULL) · DELETE에도.
        let inp = GenInput {
            concurrency: Concurrency::AllOld,
            ..inp
        };
        let mut cs2 = ChangeSet::new(4);
        cs2.toggle_delete(RowRef::Existing(0));
        let st = generate(&inp, &cs2, &orig).expect("generate");
        assert_eq!(
            st[0].req.sql,
            "DELETE FROM EMP WHERE \"ID\" = :P1 AND \"NAME\" = :P2 AND \"DT\" = :P3 AND \"MEMO\" IS NULL"
        );
    }

    #[test]
    fn generate_markers_and_null_key() {
        let cols = cols();
        let mut cs = ChangeSet::new(4);
        cs.set_cell(RowRef::Existing(1), 3, Some("m".into()), None);
        let key_all = [0usize, 1, 2, 3];
        for (d, want) in [
            (Dialect::Postgres, "UPDATE t SET \"MEMO\" = $1 WHERE \"ID\" = $2 AND \"NAME\" = $3 AND \"DT\" = $4 AND \"MEMO\" IS NULL"),
            (Dialect::Mssql, "UPDATE t SET [MEMO] = @P1 WHERE [ID] = @P2 AND [NAME] = @P3 AND [DT] = @P4 AND [MEMO] IS NULL"),
            (Dialect::Sqlite, "UPDATE t SET \"MEMO\" = ? WHERE \"ID\" = ? AND \"NAME\" = ? AND \"DT\" = ? AND \"MEMO\" IS NULL"),
        ] {
            let inp = GenInput {
                dialect: d,
                table: "t",
                cols: &cols,
                key_cols: &key_all,
                concurrency: Concurrency::Key,
            };
            let st = generate(&inp, &cs, &orig).expect("generate");
            assert_eq!(st[0].req.sql, want, "{d:?}");
            assert_eq!(st[0].req.params.len(), 4);
        }
        let pv = preview_text(&[], "t", KeyKind::AllColumns);
        assert!(pv.lines().count() >= 2);
    }

    #[test]
    fn value_text_and_bound() {
        assert_eq!(value_text(&Value::Null), None);
        assert_eq!(value_text(&Value::Int(3)), Some("3".into()));
        assert!(matches!(
            bound_of(&Some("1.5".into()), CellKind::Number, Dialect::Oracle),
            Bound::Val(Value::Decimal(_), _)
        ));
        assert!(matches!(
            bound_of(&Some("2e3".into()), CellKind::Number, Dialect::Oracle),
            Bound::Val(Value::Float(_), _)
        ));
        assert!(
            matches!(bound_of(&Some("today".into()), CellKind::Date, Dialect::Postgres), Bound::Expr(e) if e == "CURRENT_DATE")
        );
        assert_eq!(
            literal(
                &Value::Str("2026-09-26".into()),
                VarType::Date,
                Dialect::Postgres
            ),
            "DATE '2026-09-26'"
        );
        assert_eq!(
            literal(
                &Value::Str("2026-09-26 01:02:03".into()),
                VarType::Date,
                Dialect::Mssql
            ),
            "'2026-09-26T01:02:03'"
        );
    }
}
