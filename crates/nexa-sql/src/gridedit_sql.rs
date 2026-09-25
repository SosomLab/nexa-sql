//! ★ 그리드 편집의 nexa-sql 어댑터 — 편집 가능 판정 · 변경 집합 → 바인드 문장(`SqlGen`) · 미리보기 리터럴(docs/87 §7 · T-182).
//!
//! 부품(`nexa_ctl::gridedit`)은 값을 `Option<String>`으로만 안다. 여기서 ① 출처 문장을 분석해 단일 테이블·컬럼 목록을 뽑고
//! ② 셀 명세(`CellSpec`)에 따라 문자열을 `Value`+`VarType` 바인드로 바꾸며 ③ 방언 능력표(`Caps.marker`)로 자리 표시를 쓴다.
//! 서버로 가는 문장은 **여기 한 곳**에서만 만든다(77 §3 2번).

use nexa_ctl::gridedit::{CellKind, CellSpec, ChangeSet, RowRef};
use nsql_core::{BindParam, Caps, Dialect, Direction, ExecRequest, Marker, Value, VarType};
use nsql_i18n::{t, Msg};

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
    })
}

/// 열 하나의 편집 메타(결과 열 이름 + 명세).
#[derive(Clone, Debug)]
pub(crate) struct ColMeta {
    pub name: String,
    pub spec: CellSpec,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyKind {
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
        match b {
            Bound::Expr(e) => self.text(&e),
            Bound::Val(v, ty) => {
                let n = self.params.len() + 1;
                let name = format!("p{n}");
                let ph = match self.marker {
                    Marker::Named => format!(":{name}"),
                    Marker::AtName => format!("@{name}"),
                    Marker::DollarN => format!("${n}"),
                    Marker::Question => "?".into(),
                };
                self.sql.push_str(&ph);
                self.preview
                    .push_str(&literal(&v, ty.clone(), self.dialect));
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
    fn key_eq(&mut self, col: &str, b: Bound) {
        match b {
            Bound::Val(Value::Null, _) => self.text(&format!("{col} IS NULL")),
            other => {
                self.text(&format!("{col} = "));
                self.value(other);
            }
        }
    }
    fn finish(self, kind: StmtKind, row: RowRef) -> EditStmt {
        EditStmt {
            kind,
            row,
            req: ExecRequest {
                sql: self.sql,
                params: self.params,
            },
            preview: self.preview,
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
    let key_where = |sink: &mut Sink, row: usize| {
        sink.text(" WHERE ");
        for (i, &k) in inp.key_cols.iter().enumerate() {
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
            sink.key_eq(&q(d, &c.name), b);
        }
    };
    let mut out = Vec::new();
    // DELETE
    for row in cs.deleted_rows() {
        let mut s = Sink::new(d);
        s.text(&format!("DELETE FROM {}", inp.table));
        key_where(&mut s, row);
        out.push(s.finish(StmtKind::Delete, RowRef::Existing(row)));
    }
    // UPDATE(행별 · 수정된 열만).
    let mut rows: Vec<usize> = cs.edits().map(|((r, _), _)| *r).collect();
    rows.dedup();
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
        key_where(&mut s, row);
        out.push(s.finish(StmtKind::Update, RowRef::Existing(row)));
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
        out.push(s.finish(StmtKind::Insert, RowRef::Inserted(k)));
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
            ColMeta {
                name: "ID".into(),
                spec: CellSpec::new("ID", CellKind::Number).not_null(),
            },
            ColMeta {
                name: "NAME".into(),
                spec: CellSpec::text("NAME").max_len(10),
            },
            ColMeta {
                name: "DT".into(),
                spec: CellSpec::new("DT", CellKind::DateTime),
            },
            ColMeta {
                name: "MEMO".into(),
                spec: CellSpec::text("MEMO").default_expr("'x'"),
            },
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
        };
        let st = generate(&inp, &cs, &orig).expect("generate");
        assert_eq!(st.len(), 3);
        assert_eq!(st[0].kind, StmtKind::Delete);
        assert_eq!(st[0].req.sql, "DELETE FROM EMP WHERE \"ID\" = :p1");
        assert_eq!(st[0].preview, "DELETE FROM EMP WHERE \"ID\" = 3");
        assert_eq!(st[1].kind, StmtKind::Update);
        assert_eq!(
            st[1].req.sql,
            "UPDATE EMP SET \"NAME\" = :p1, \"DT\" = :p2 WHERE \"ID\" = :p3"
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
            "INSERT INTO EMP (\"ID\", \"NAME\", \"DT\") VALUES (:p1, :p2, SYSDATE)"
        );
        assert_eq!(st[2].req.params.len(), 2);
        assert_eq!(st[2].req.params[0].value, Value::Int(9));
    }

    #[test]
    fn generate_markers_and_null_key() {
        let cols = cols();
        let mut cs = ChangeSet::new(4);
        cs.set_cell(RowRef::Existing(1), 3, Some("m".into()), None);
        let key_all = [0usize, 1, 2, 3];
        for (d, want) in [
            (Dialect::Postgres, "UPDATE t SET \"MEMO\" = $1 WHERE \"ID\" = $2 AND \"NAME\" = $3 AND \"DT\" = $4 AND \"MEMO\" IS NULL"),
            (Dialect::Mssql, "UPDATE t SET [MEMO] = @p1 WHERE [ID] = @p2 AND [NAME] = @p3 AND [DT] = @p4 AND [MEMO] IS NULL"),
            (Dialect::Sqlite, "UPDATE t SET \"MEMO\" = ? WHERE \"ID\" = ? AND \"NAME\" = ? AND \"DT\" = ? AND \"MEMO\" IS NULL"),
        ] {
            let inp = GenInput {
                dialect: d,
                table: "t",
                cols: &cols,
                key_cols: &key_all,
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
