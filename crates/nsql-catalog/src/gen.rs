//! ★ **Generate SQL**(docs/83 §3 · 09-25 · 사용자 "테이블 = DML 유형별 · 프로시저 = CALL · 공통 = DDL — 테이블·패키지·프로시저·뷰·
//! 인덱스·제약 등등"). 탐색기 우클릭 ▸ Generate SQL ▸ 항목 → 메타 세션에서 [`generate`] → SQL Preview 모달(호스트).
//!
//! - 바인드 표기 = `:컬럼`(D-203 · 엔진이 방언으로 고쳐 쓴다 DR-8) · 식별자는 방언 인용([`super::qualified`]).
//! - 키 = PK → UNIQUE → 없음([41](../../../docs/41-sql-copy-key-rules.md) 규칙) — 키가 없으면 `WHERE 1 = 0` + 주석(실수로 전부 바꾸지 않게).
//! - DDL 원천 = Oracle `DBMS_METADATA.GET_DDL`(종류 → 메타데이터 타입) · PG `pg_get_*` + sys 구성 · SQL Server `OBJECT_DEFINITION` +
//!   sys 구성 · SQLite `sqlite_master.sql` · 그 밖은 [`super::source`].
//! - CLI = `nsql cat gen <what> <object> [kind] [sub-kind sub-name]`(자동 점검 · GUI와 같은 함수).

use super::tree::{attributes, index_columns};
use super::{
    col, columns, lit, package_members, query, quote_ident, routine_args, source, table_ddl,
    table_detail, Dialect, KeyDef, ObjectInfo, ObjectKind, Session, SubKind,
};
use nsql_core::DbError;

/// 생성할 문장 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GenWhat {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
    Call,
    Ddl,
}

impl GenWhat {
    pub const ALL: [GenWhat; 7] = [
        GenWhat::Select,
        GenWhat::Insert,
        GenWhat::Update,
        GenWhat::Delete,
        GenWhat::Merge,
        GenWhat::Call,
        GenWhat::Ddl,
    ];

    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            GenWhat::Select => "select",
            GenWhat::Insert => "insert",
            GenWhat::Update => "update",
            GenWhat::Delete => "delete",
            GenWhat::Merge => "merge",
            GenWhat::Call => "call",
            GenWhat::Ddl => "ddl",
        }
    }

    /// 메뉴 라벨(SQL 낱말 = 번역 없음).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            GenWhat::Select => "SELECT",
            GenWhat::Insert => "INSERT",
            GenWhat::Update => "UPDATE",
            GenWhat::Delete => "DELETE",
            GenWhat::Merge => "MERGE",
            GenWhat::Call => "CALL",
            GenWhat::Ddl => "DDL",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<GenWhat> {
        let s = s.trim().to_ascii_lowercase();
        GenWhat::ALL
            .iter()
            .copied()
            .find(|w| w.code() == s || (s == "exec" && *w == GenWhat::Call))
    }
}

/// 무엇을 만들 것인가 — 주인 객체 + 종류 + (하위 항목이면) 그 폴더·이름.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenSpec {
    pub owner: ObjectInfo,
    pub what: GenWhat,
    /// 하위 항목(제약·인덱스·트리거)의 DDL이면 `(폴더, 이름)`.
    pub sub: Option<(SubKind, String)>,
}

impl GenSpec {
    /// 미리보기 제목·파일 이름의 밑감(`OBJ_select` · `OBJ_PK_ddl`).
    #[must_use]
    pub fn title(&self) -> String {
        match &self.sub {
            Some((_, n)) => format!("{}.{}_{}", self.owner.name, n, self.what.code()),
            None => format!("{}_{}", self.owner.name, self.what.code()),
        }
    }
}

/// 메뉴에 낼 항목(83 §3 표) — 하위 항목은 DDL만(제약·인덱스·트리거) · 관계 = SELECT(+테이블은 DML 넷 + MERGE) + DDL ·
/// 루틴·패키지 = CALL + DDL · 시퀀스 = SELECT(NEXTVAL) + DDL · 그 밖 = DDL.
#[must_use]
pub fn gen_whats(dialect: Dialect, kind: ObjectKind, sub: Option<SubKind>) -> Vec<GenWhat> {
    use GenWhat::*;
    if let Some(sk) = sub {
        return match sk {
            SubKind::Constraints
            | SubKind::UniqueKeys
            | SubKind::CheckConstraints
            | SubKind::ForeignKeys
            | SubKind::Indexes
            | SubKind::Triggers => vec![Ddl],
            _ => Vec::new(),
        };
    }
    if kind == ObjectKind::Table {
        let mut v = vec![Select, Insert, Update, Delete];
        if dialect != Dialect::Odbc {
            v.push(Merge);
        }
        v.push(Ddl);
        return v;
    }
    if kind.is_relation() {
        return vec![Select, Ddl];
    }
    if kind.is_routine() || kind == ObjectKind::Package {
        return vec![Call, Ddl];
    }
    if kind == ObjectKind::Sequence {
        return vec![Select, Ddl];
    }
    vec![Ddl]
}

/// 바인드 이름(`:이름`) — 식별자에서 글자·숫자·`_`만.
fn bind(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!(":{clean}")
}

/// 키 컬럼(PK → UNIQUE → 없음).
fn key_cols(keys: &[KeyDef]) -> Vec<String> {
    keys.iter()
        .find(|k| k.kind == 'P')
        .or_else(|| keys.iter().find(|k| k.kind == 'U'))
        .map(|k| k.cols.clone())
        .unwrap_or_default()
}

fn is_key(name: &str, keys: &[String]) -> bool {
    keys.iter().any(|k| k.eq_ignore_ascii_case(name))
}

/// 문장 하나를 만든다(메타 세션 · 읽기 전용 사전 질의만).
pub fn generate(s: &mut dyn Session, spec: &GenSpec) -> Result<String, DbError> {
    let d = s.dialect();
    let o = &spec.owner;
    if let Some((sk, name)) = &spec.sub {
        return sub_ddl(s, d, o, *sk, name);
    }
    match spec.what {
        GenWhat::Select => select_sql(s, d, o),
        GenWhat::Insert | GenWhat::Update | GenWhat::Delete | GenWhat::Merge => {
            dml_sql(s, d, o, spec.what)
        }
        GenWhat::Call => call_sql(s, d, o),
        GenWhat::Ddl => object_ddl(s, d, o),
    }
}

fn q(d: Dialect, o: &ObjectInfo) -> String {
    qual(d, &o.schema, &o.name)
}

/// 식별자 — **단순 이름**(글자·숫자·`_`·`$`·`#` · 방언 기본 대소문자 = Oracle 대문자 · PG 소문자)은 인용하지 않고, 그 밖은 방언 인용
/// (읽기 좋은 문장 · 우리 엔진의 `EXEC` 해석도 맨 이름이 편하다).
fn ident(d: Dialect, name: &str) -> String {
    let simple = !name.is_empty()
        && name.chars().enumerate().all(|(i, c)| {
            c == '_'
                || c.is_ascii_alphabetic()
                || (i > 0 && (c.is_ascii_digit() || c == '$' || c == '#'))
        });
    let case_ok = match d {
        Dialect::Oracle => name.chars().all(|c| !c.is_ascii_lowercase()),
        Dialect::Postgres => name.chars().all(|c| !c.is_ascii_uppercase()),
        _ => true,
    };
    if simple && case_ok {
        name.to_string()
    } else {
        quote_ident(d, name)
    }
}

fn qual(d: Dialect, schema: &str, name: &str) -> String {
    if schema.is_empty() {
        ident(d, name)
    } else {
        format!("{}.{}", ident(d, schema), ident(d, name))
    }
}

fn select_sql(s: &mut dyn Session, d: Dialect, o: &ObjectInfo) -> Result<String, DbError> {
    if o.kind == ObjectKind::Sequence {
        let qn = q(d, o);
        return Ok(match d {
            Dialect::Oracle => format!("SELECT {qn}.NEXTVAL FROM DUAL;\n"),
            Dialect::Mssql => format!("SELECT NEXT VALUE FOR {qn};\n"),
            Dialect::Postgres => format!("SELECT nextval('{}');\n", q(d, o).replace('\'', "''")),
            _ => format!("SELECT {qn}.NEXTVAL;\n"),
        });
    }
    let cols = columns(s, &o.schema, &o.name)?;
    let keys = if o.kind == ObjectKind::Table {
        key_cols(&table_detail(s, &o.schema, &o.name)?.keys)
    } else {
        Vec::new()
    };
    let mut out = String::from("SELECT\n");
    let n = cols.len();
    for (i, c) in cols.iter().enumerate() {
        out.push_str("    t.");
        out.push_str(&ident(d, &c.name));
        if i + 1 < n {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(&format!("FROM {} t\n", q(d, o)));
    for (i, k) in keys.iter().enumerate() {
        out.push_str(if i == 0 { "WHERE " } else { "  AND " });
        out.push_str(&format!("t.{} = {}\n", ident(d, k), bind(k)));
    }
    let trimmed = out.trim_end().to_string();
    Ok(format!("{trimmed};\n"))
}

fn dml_sql(
    s: &mut dyn Session,
    d: Dialect,
    o: &ObjectInfo,
    what: GenWhat,
) -> Result<String, DbError> {
    let cols = columns(s, &o.schema, &o.name)?;
    if cols.is_empty() {
        return Err(DbError {
            code: None,
            message: format!("{}.{}: no columns", o.schema, o.name),
            position: None,
        });
    }
    let keys = key_cols(&table_detail(s, &o.schema, &o.name)?.keys);
    let qn = q(d, o);
    let names: Vec<String> = cols.iter().map(|c| ident(d, &c.name)).collect();
    let binds: Vec<String> = cols.iter().map(|c| bind(&c.name)).collect();
    let non_key: Vec<usize> = (0..cols.len())
        .filter(|&i| !is_key(&cols[i].name, &keys))
        .collect();
    let key_idx: Vec<usize> = (0..cols.len())
        .filter(|&i| is_key(&cols[i].name, &keys))
        .collect();
    let no_key_note = "-- no primary/unique key: WHERE 1 = 0 keeps this from touching every row — set your own condition\n";
    let where_keys = |indent: &str| -> String {
        if key_idx.is_empty() {
            return format!("{indent}WHERE 1 = 0\n");
        }
        let mut w = String::new();
        for (j, &i) in key_idx.iter().enumerate() {
            w.push_str(indent);
            w.push_str(if j == 0 { "WHERE " } else { "  AND " });
            w.push_str(&format!("{} = {}\n", names[i], binds[i]));
        }
        w
    };
    let out = match what {
        GenWhat::Insert => {
            let mut out = format!("INSERT INTO {qn} (\n");
            out.push_str(&list(&names, "    "));
            out.push_str(") VALUES (\n");
            out.push_str(&list(&binds, "    "));
            out.push_str(");\n");
            out
        }
        GenWhat::Update => {
            let mut out = String::new();
            if key_idx.is_empty() {
                out.push_str(no_key_note);
            }
            out.push_str(&format!("UPDATE {qn}\n"));
            let set_idx: &[usize] = if non_key.is_empty() {
                &key_idx
            } else {
                &non_key
            };
            for (j, &i) in set_idx.iter().enumerate() {
                out.push_str(if j == 0 { "   SET " } else { "       " });
                out.push_str(&format!("{} = {}", names[i], binds[i]));
                out.push_str(if j + 1 < set_idx.len() { ",\n" } else { "\n" });
            }
            out.push_str(&where_keys(" "));
            format!("{};\n", out.trim_end())
        }
        GenWhat::Delete => {
            let mut out = String::new();
            if key_idx.is_empty() {
                out.push_str(no_key_note);
            }
            out.push_str(&format!("DELETE FROM {qn}\n"));
            out.push_str(&where_keys(" "));
            format!("{};\n", out.trim_end())
        }
        GenWhat::Merge => merge_sql(d, &qn, &names, &binds, &key_idx, &non_key),
        _ => unreachable!("dml_sql only handles DML"),
    };
    Ok(out)
}

fn list(items: &[String], indent: &str) -> String {
    let mut out = String::new();
    let n = items.len();
    for (i, it) in items.iter().enumerate() {
        out.push_str(indent);
        out.push_str(it);
        if i + 1 < n {
            out.push(',');
        }
        out.push('\n');
    }
    out
}

fn merge_sql(
    d: Dialect,
    qn: &str,
    names: &[String],
    binds: &[String],
    key_idx: &[usize],
    non_key: &[usize],
) -> String {
    if key_idx.is_empty() {
        let mut out = String::from("-- MERGE needs a primary/unique key and this table has none — INSERT is generated instead\n");
        out.push_str(&format!("INSERT INTO {qn} (\n"));
        out.push_str(&list(names, "    "));
        out.push_str(") VALUES (\n");
        out.push_str(&list(binds, "    "));
        out.push_str(");\n");
        return out;
    }
    let set_idx: &[usize] = if non_key.is_empty() { key_idx } else { non_key };
    match d {
        Dialect::Sqlite | Dialect::Mysql => {
            let mut out = format!("INSERT INTO {qn} (\n");
            out.push_str(&list(names, "    "));
            out.push_str(") VALUES (\n");
            out.push_str(&list(binds, "    "));
            out.push(')');
            if d == Dialect::Sqlite {
                let ks: Vec<String> = key_idx.iter().map(|&i| names[i].clone()).collect();
                out.push_str(&format!(
                    "\nON CONFLICT ({}) DO UPDATE SET\n",
                    ks.join(", ")
                ));
                let sets: Vec<String> = set_idx
                    .iter()
                    .map(|&i| format!("{} = excluded.{}", names[i], names[i]))
                    .collect();
                out.push_str(&list(&sets, "    "));
            } else {
                out.push_str("\nON DUPLICATE KEY UPDATE\n");
                let sets: Vec<String> = set_idx
                    .iter()
                    .map(|&i| format!("{} = VALUES({})", names[i], names[i]))
                    .collect();
                out.push_str(&list(&sets, "    "));
            }
            format!("{};\n", out.trim_end())
        }
        _ => {
            let alias_as = if d == Dialect::Postgres { " AS" } else { "" };
            let mut out = format!("MERGE INTO {qn}{alias_as} t\nUSING (\n    SELECT\n");
            let src: Vec<String> = (0..names.len())
                .map(|i| format!("{} AS {}", binds[i], names[i]))
                .collect();
            out.push_str(&list(&src, "        "));
            if d == Dialect::Oracle {
                out.push_str("    FROM DUAL\n");
            }
            out.push_str(&format!("){alias_as} s\n"));
            let on: Vec<String> = key_idx
                .iter()
                .map(|&i| format!("t.{} = s.{}", names[i], names[i]))
                .collect();
            if d == Dialect::Postgres {
                out.push_str(&format!("ON {}\n", on.join(" AND ")));
            } else {
                out.push_str(&format!("ON ({})\n", on.join(" AND ")));
            }
            out.push_str("WHEN MATCHED THEN UPDATE SET\n");
            let sets: Vec<String> = set_idx
                .iter()
                .map(|&i| {
                    if d == Dialect::Postgres {
                        format!("{} = s.{}", names[i], names[i])
                    } else {
                        format!("t.{} = s.{}", names[i], names[i])
                    }
                })
                .collect();
            out.push_str(&list(&sets, "    "));
            out.push_str("WHEN NOT MATCHED THEN INSERT (\n");
            out.push_str(&list(names, "    "));
            out.push_str(") VALUES (\n");
            let vals: Vec<String> = names.iter().map(|n| format!("s.{n}")).collect();
            out.push_str(&list(&vals, "    "));
            out.push(')');
            format!("{out};\n")
        }
    }
}

/// Oracle SQL*Plus `VARIABLE` 타입(OUT 인자 받기).
fn oracle_var_type(data_type: &str) -> &'static str {
    let up = data_type.to_ascii_uppercase();
    if up.contains("CURSOR") {
        "REFCURSOR"
    } else if up.contains("NUMBER")
        || up.contains("INTEGER")
        || up.contains("FLOAT")
        || up.contains("BINARY")
    {
        "NUMBER"
    } else if up.contains("CLOB") {
        "CLOB"
    } else {
        "VARCHAR2(4000)"
    }
}

/// 루틴 하나의 CALL 문(한 오버로드 = 첫 것 · 인자 = `routine_args`).
fn call_one(
    s: &mut dyn Session,
    d: Dialect,
    call_name: &str,
    display: &str,
    is_function: bool,
    extra: &str,
) -> Result<String, DbError> {
    let all = routine_args(s, call_name)?;
    let first_ov = all
        .iter()
        .map(|a| a.overload.clone())
        .min()
        .unwrap_or_default();
    let args: Vec<_> = all.into_iter().filter(|a| a.overload == first_ov).collect();
    let ret = args
        .iter()
        .find(|a| a.position == 0 || a.name.is_empty())
        .cloned();
    let params: Vec<_> = args
        .iter()
        .filter(|a| a.position != 0 && !a.name.is_empty())
        .cloned()
        .collect();
    let mut out = String::new();
    for p in &params {
        out.push_str(&format!(
            "-- {:<6} {:<30} {}\n",
            p.in_out.replace("IN/OUT", "IN OUT"),
            p.name.trim_start_matches('@'),
            p.data_type
        ));
    }
    if let Some(r) = &ret {
        if is_function {
            out.push_str(&format!("-- {:<6} {:<30} {}\n", "RETURN", "", r.data_type));
        }
    }
    match d {
        Dialect::Oracle => {
            for p in params.iter().filter(|p| p.in_out.contains("OUT")) {
                out.push_str(&format!(
                    "VARIABLE {} {}\n",
                    p.name.to_ascii_lowercase(),
                    oracle_var_type(&p.data_type)
                ));
            }
            let named: Vec<String> = params
                .iter()
                .map(|p| format!("{} => {}", p.name, bind(&p.name.to_ascii_lowercase())))
                .collect();
            if is_function {
                let ret_ty = ret.as_ref().map(|r| r.data_type.as_str()).unwrap_or("");
                if ret_ty.to_ascii_uppercase().contains("CURSOR") {
                    out.push_str("VARIABLE rc REFCURSOR\n");
                    out.push_str(&format!("EXEC :rc := {display}({})\n", named.join(", ")));
                } else {
                    out.push_str(&format!(
                        "SELECT {display}({}) FROM DUAL;\n",
                        named.join(", ")
                    ));
                }
            } else {
                out.push_str(&format!("EXEC {display}({})\n", named.join(", ")));
            }
            for p in params.iter().filter(|p| p.in_out.contains("OUT")) {
                out.push_str(&format!("PRINT {}\n", p.name.to_ascii_lowercase()));
            }
        }
        Dialect::Mssql => {
            let parts: Vec<String> = params
                .iter()
                .map(|p| {
                    let bare = p.name.trim_start_matches('@');
                    let mut t = format!("@{bare} = {}", bind(bare));
                    if p.in_out.contains("OUT") {
                        t.push_str(" OUTPUT");
                    }
                    t
                })
                .collect();
            if is_function {
                let table_fn = matches!(extra, "TF" | "IF" | "FT");
                let inner: Vec<String> = params
                    .iter()
                    .map(|p| bind(p.name.trim_start_matches('@')))
                    .collect();
                if table_fn {
                    out.push_str(&format!("SELECT * FROM {display}({});\n", inner.join(", ")));
                } else {
                    out.push_str(&format!("SELECT {display}({});\n", inner.join(", ")));
                }
            } else if parts.is_empty() {
                out.push_str(&format!("EXEC {display};\n"));
            } else {
                out.push_str(&format!("EXEC {display}\n    {};\n", parts.join(",\n    ")));
            }
        }
        Dialect::Postgres => {
            let inner: Vec<String> = params.iter().map(|p| bind(&p.name)).collect();
            if is_function {
                out.push_str(&format!("SELECT * FROM {display}({});\n", inner.join(", ")));
            } else {
                out.push_str(&format!("CALL {display}({});\n", inner.join(", ")));
            }
        }
        _ => {
            if params.is_empty() {
                out.push_str("-- argument list unknown for this DBMS — fill in the values\n");
            }
            let inner: Vec<String> = params.iter().map(|p| bind(&p.name)).collect();
            if is_function {
                out.push_str(&format!("SELECT {display}({});\n", inner.join(", ")));
            } else {
                out.push_str(&format!("CALL {display}({});\n", inner.join(", ")));
            }
        }
    }
    Ok(out)
}

fn call_sql(s: &mut dyn Session, d: Dialect, o: &ObjectInfo) -> Result<String, DbError> {
    let display = q(d, o);
    let call_name = format!("{}.{}", o.schema, o.name);
    if o.kind == ObjectKind::Package {
        // 패키지 = 멤버마다 한 블록(프로시저 · 함수) — 이름 = SCHEMA.PKG.MEMBER.
        let members = package_members(s, &o.schema, &o.name)?;
        if members.is_empty() {
            return Ok(format!("-- {display}: no public members\n"));
        }
        let mut out = String::new();
        for m in members {
            let is_fn = !m.data_type.eq_ignore_ascii_case("PROCEDURE");
            out.push_str(&format!("-- ── {}.{}\n", display, m.name));
            let block = call_one(
                s,
                d,
                &format!("{call_name}.{}", m.name),
                &format!("{display}.{}", ident(d, &m.name)),
                is_fn,
                "",
            )?;
            out.push_str(&block);
            out.push('\n');
        }
        return Ok(out.trim_end().to_string() + "\n");
    }
    let is_fn = matches!(o.kind, ObjectKind::Function | ObjectKind::Aggregate);
    let call = if d == Dialect::Postgres && !o.extra.is_empty() {
        format!("{call_name}({})", o.extra.trim_start_matches("SETOF "))
    } else {
        call_name
    };
    call_one(s, d, &call, &display, is_fn, &o.extra)
}

/// Oracle `DBMS_METADATA` 객체 타입.
fn oracle_meta_type(kind: ObjectKind) -> Option<&'static str> {
    Some(match kind {
        ObjectKind::Table | ObjectKind::ExternalTable => "TABLE",
        ObjectKind::View => "VIEW",
        ObjectKind::MaterializedView => "MATERIALIZED_VIEW",
        ObjectKind::Procedure => "PROCEDURE",
        ObjectKind::Function => "FUNCTION",
        ObjectKind::Package => "PACKAGE",
        ObjectKind::PackageBody => "PACKAGE_BODY",
        ObjectKind::Sequence => "SEQUENCE",
        ObjectKind::Trigger | ObjectKind::SchemaTrigger => "TRIGGER",
        ObjectKind::Index => "INDEX",
        ObjectKind::Synonym => "SYNONYM",
        ObjectKind::Type => "TYPE",
        ObjectKind::Queue => "AQ_QUEUE",
        ObjectKind::DbLink => "DB_LINK",
        ObjectKind::JavaClass => "JAVA_CLASS",
        ObjectKind::Job | ObjectKind::SchedulerJob | ObjectKind::SchedulerProgram => "PROCOBJ",
        _ => return None,
    })
}

fn oracle_get_ddl(
    s: &mut dyn Session,
    ty: &str,
    name: &str,
    owner: &str,
) -> Result<String, DbError> {
    let sql = format!(
        "SELECT DBMS_METADATA.GET_DDL({}, {}, {}) FROM dual",
        lit(ty),
        lit(name),
        lit(owner)
    );
    let rs = query(s, &sql)?;
    let body = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
    let body = body.trim();
    if body.is_empty() {
        return Err(DbError {
            code: None,
            message: format!("{owner}.{name}: DDL not found ({ty})"),
            position: None,
        });
    }
    let mut out = body.to_string();
    if !out.ends_with(';') && !out.ends_with('/') {
        out.push(';');
    }
    out.push('\n');
    Ok(out)
}

fn object_ddl(s: &mut dyn Session, d: Dialect, o: &ObjectInfo) -> Result<String, DbError> {
    match d {
        Dialect::Oracle => match oracle_meta_type(o.kind) {
            Some(ty) => oracle_get_ddl(s, ty, &o.name, &o.schema),
            None => source(s, &o.schema, o.kind, &o.name),
        },
        Dialect::Postgres => match o.kind {
            ObjectKind::Table | ObjectKind::ForeignTable => table_with_keys(s, d, o),
            ObjectKind::Index => {
                let sql = format!("SELECT pg_get_indexdef({}::regclass)", lit(&q(d, o)));
                let rs = query(s, &sql)?;
                Ok(format!(
                    "{};\n",
                    rs.rows.first().map(|r| col(r, 0)).unwrap_or_default()
                ))
            }
            ObjectKind::Sequence => {
                let sql = format!(
                    "SELECT 'CREATE SEQUENCE ' || quote_ident(schemaname) || '.' || quote_ident(sequencename) || E'\\n    AS ' || data_type || E'\\n    START WITH ' || start_value || E'\\n    INCREMENT BY ' || increment_by || E'\\n    MINVALUE ' || min_value || E'\\n    MAXVALUE ' || max_value || E'\\n    CACHE ' || cache_size || CASE WHEN cycle THEN E'\\n    CYCLE' ELSE '' END || ';' FROM pg_sequences WHERE schemaname = {} AND sequencename = {}",
                    lit(&o.schema),
                    lit(&o.name)
                );
                let rs = query(s, &sql)?;
                Ok(format!(
                    "{}\n",
                    rs.rows.first().map(|r| col(r, 0)).unwrap_or_default()
                ))
            }
            ObjectKind::Type => {
                let attrs = attributes(s, d, &o.schema, &o.name)?;
                if attrs.is_empty() {
                    let sql = format!(
                        "SELECT string_agg(quote_literal(e.enumlabel), ', ' ORDER BY e.enumsortorder) FROM pg_enum e JOIN pg_type t ON t.oid = e.enumtypid JOIN pg_namespace n ON n.oid = t.typnamespace WHERE n.nspname = {} AND t.typname = {}",
                        lit(&o.schema),
                        lit(&o.name)
                    );
                    let rs = query(s, &sql)?;
                    let labels = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
                    if labels.is_empty() {
                        return Err(no_ddl(o));
                    }
                    return Ok(format!("CREATE TYPE {} AS ENUM ({labels});\n", q(d, o)));
                }
                let mut out = format!("CREATE TYPE {} AS (\n", q(d, o));
                let items: Vec<String> = attrs
                    .iter()
                    .map(|a| format!("{} {}", ident(d, &a.name), a.detail))
                    .collect();
                out.push_str(&list(&items, "    "));
                out.push_str(");\n");
                Ok(out)
            }
            ObjectKind::Extension => Ok(format!(
                "CREATE EXTENSION IF NOT EXISTS {};\n",
                ident(d, &o.name)
            )),
            _ => source(s, &o.schema, o.kind, &o.name),
        },
        Dialect::Mssql => match o.kind {
            ObjectKind::Table | ObjectKind::ExternalTable => table_with_keys(s, d, o),
            ObjectKind::Index => {
                // 부가 = "테이블 · UNIQUE · PK" — 테이블 이름은 첫 조각.
                let table = o.extra.split(" · ").next().unwrap_or("").trim().to_string();
                if table.is_empty() {
                    return Err(no_ddl(o));
                }
                mssql_index_ddl(s, &o.schema, &table, &o.name)
            }
            ObjectKind::Sequence => {
                let sql = format!(
                    "SELECT 'CREATE SEQUENCE ' + QUOTENAME(s.name) + '.' + QUOTENAME(q.name) + ' AS ' + t.name + CHAR(10) + '    START WITH ' + CONVERT(varchar(40), q.start_value) + CHAR(10) + '    INCREMENT BY ' + CONVERT(varchar(40), q.increment) + CHAR(10) + '    MINVALUE ' + CONVERT(varchar(40), q.minimum_value) + CHAR(10) + '    MAXVALUE ' + CONVERT(varchar(40), q.maximum_value) + CASE WHEN q.is_cycling = 1 THEN CHAR(10) + '    CYCLE' ELSE '' END + ';' FROM sys.sequences q JOIN sys.schemas s ON s.schema_id = q.schema_id JOIN sys.types t ON t.user_type_id = q.user_type_id WHERE s.name = {} AND q.name = {}",
                    lit(&o.schema),
                    lit(&o.name)
                );
                let rs = query(s, &sql)?;
                Ok(format!(
                    "{}\n",
                    rs.rows.first().map(|r| col(r, 0)).unwrap_or_default()
                ))
            }
            _ => source(s, &o.schema, o.kind, &o.name),
        },
        Dialect::Sqlite => {
            let sql = format!(
                "SELECT sql FROM sqlite_master WHERE name = {} AND sql IS NOT NULL",
                lit(&o.name)
            );
            let rs = query(s, &sql)?;
            let body = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
            if body.is_empty() {
                return Err(no_ddl(o));
            }
            let mut out = body.trim().to_string();
            if !out.ends_with(';') {
                out.push(';');
            }
            out.push('\n');
            if o.kind == ObjectKind::Table {
                // 인덱스·트리거도 함께(테이블 DDL 한 벌).
                let sql = format!(
                    "SELECT sql FROM sqlite_master WHERE tbl_name = {} AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type, name",
                    lit(&o.name)
                );
                for r in query(s, &sql)?.rows {
                    out.push_str(&format!("{};\n", col(&r, 0).trim()));
                }
            }
            Ok(out)
        }
        _ => source(s, &o.schema, o.kind, &o.name),
    }
}

fn no_ddl(o: &ObjectInfo) -> DbError {
    DbError {
        code: None,
        message: format!(
            "{}.{}: DDL not available for {:?}",
            o.schema, o.name, o.kind
        ),
        position: None,
    }
}

/// 테이블 DDL 한 벌(PG · SQL Server): CREATE TABLE + 제약 ALTER + 인덱스 CREATE.
fn table_with_keys(s: &mut dyn Session, d: Dialect, o: &ObjectInfo) -> Result<String, DbError> {
    let mut out = table_ddl(s, &o.schema, &o.name)?;
    let det = table_detail(s, &o.schema, &o.name)?;
    let qn = q(d, o);
    for k in &det.keys {
        if let Some(line) = constraint_line(s, d, o, k)? {
            out.push_str(&format!(
                "ALTER TABLE {qn} ADD CONSTRAINT {} {line};\n",
                ident(d, &k.name)
            ));
        }
    }
    let key_names: Vec<&str> = det.keys.iter().map(|k| k.name.as_str()).collect();
    for i in &det.indexes {
        if key_names.iter().any(|k| k.eq_ignore_ascii_case(&i.name)) {
            continue; // 키가 만든 인덱스는 제약 줄이 대신한다.
        }
        let cols: Vec<String> = i.cols.iter().map(|c| ident(d, c)).collect();
        out.push_str(&format!(
            "CREATE {}INDEX {} ON {qn} ({});\n",
            if i.unique { "UNIQUE " } else { "" },
            ident(d, &i.name),
            cols.join(", ")
        ));
    }
    Ok(out)
}

/// 제약 본문(`PRIMARY KEY (a, b)` · `UNIQUE (…)` · `FOREIGN KEY (…) REFERENCES t (…)` · `CHECK (…)`) — 방언 사전에서 정확한 것을 우선.
fn constraint_line(
    s: &mut dyn Session,
    d: Dialect,
    o: &ObjectInfo,
    k: &KeyDef,
) -> Result<Option<String>, DbError> {
    if d == Dialect::Postgres {
        let sql = format!(
            "SELECT pg_get_constraintdef(oid, true) FROM pg_constraint WHERE conname = {} AND conrelid = {}::regclass",
            lit(&k.name),
            lit(&q(d, o))
        );
        let rs = query(s, &sql)?;
        return Ok(rs.rows.first().map(|r| col(r, 0)).filter(|t| !t.is_empty()));
    }
    let cols: Vec<String> = k.cols.iter().map(|c| ident(d, c)).collect();
    Ok(Some(match k.kind {
        'P' => format!("PRIMARY KEY ({})", cols.join(", ")),
        'U' => format!("UNIQUE ({})", cols.join(", ")),
        'R' => {
            let target = k.ref_table.clone().unwrap_or_default();
            let (rcols, rt) = if d == Dialect::Mssql {
                let sql = format!(
                    "SELECT SCHEMA_NAME(ro.schema_id) + '.' + ro.name, STUFF((SELECT ', ' + QUOTENAME(rc.name) FROM sys.foreign_key_columns fkc JOIN sys.columns rc ON rc.object_id = fkc.referenced_object_id AND rc.column_id = fkc.referenced_column_id WHERE fkc.constraint_object_id = fk.object_id ORDER BY fkc.constraint_column_id FOR XML PATH('')), 1, 2, '') FROM sys.foreign_keys fk JOIN sys.objects ro ON ro.object_id = fk.referenced_object_id WHERE fk.name = {} AND fk.parent_object_id = OBJECT_ID({})",
                    lit(&k.name),
                    lit(&q(d, o))
                );
                let rs = query(s, &sql)?;
                match rs.rows.first() {
                    Some(r) => (col(r, 1), col(r, 0)),
                    None => (String::new(), target),
                }
            } else {
                (String::new(), target)
            };
            if rcols.is_empty() {
                format!("FOREIGN KEY ({}) REFERENCES {rt}", cols.join(", "))
            } else {
                format!(
                    "FOREIGN KEY ({}) REFERENCES {rt} ({rcols})",
                    cols.join(", ")
                )
            }
        }
        _ => {
            if d == Dialect::Mssql {
                let sql = format!(
                    "SELECT definition FROM sys.check_constraints WHERE name = {} AND parent_object_id = OBJECT_ID({})",
                    lit(&k.name),
                    lit(&q(d, o))
                );
                let rs = query(s, &sql)?;
                let def = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
                if def.is_empty() {
                    return Ok(None);
                }
                format!("CHECK {def}")
            } else {
                return Ok(None);
            }
        }
    }))
}

fn mssql_index_ddl(
    s: &mut dyn Session,
    schema: &str,
    table: &str,
    index: &str,
) -> Result<String, DbError> {
    let d = Dialect::Mssql;
    let sql = format!(
        "SELECT i.is_unique, i.type_desc FROM sys.indexes i JOIN sys.objects o ON o.object_id = i.object_id JOIN sys.schemas s ON s.schema_id = o.schema_id WHERE s.name = {} AND o.name = {} AND i.name = {}",
        lit(schema),
        lit(table),
        lit(index)
    );
    let rs = query(s, &sql)?;
    let Some(r) = rs.rows.first() else {
        return Err(DbError {
            code: None,
            message: format!("{schema}.{table}.{index}: index not found"),
            position: None,
        });
    };
    let unique = matches!(col(r, 0).as_str(), "1" | "true" | "True");
    let ty = col(r, 1);
    let cols = index_columns(s, d, schema, index)?;
    let keys: Vec<String> = cols
        .iter()
        .filter(|c| !c.detail.contains("INCLUDE"))
        .map(|c| {
            let mut t = ident(d, &c.name);
            if c.detail.contains("DESC") {
                t.push_str(" DESC");
            }
            t
        })
        .collect();
    let incl: Vec<String> = cols
        .iter()
        .filter(|c| c.detail.contains("INCLUDE"))
        .map(|c| ident(d, &c.name))
        .collect();
    let mut out = format!(
        "CREATE {}{} INDEX {} ON {} ({})",
        if unique { "UNIQUE " } else { "" },
        ty.to_ascii_uppercase(),
        ident(d, index),
        qual(d, schema, table),
        keys.join(", ")
    );
    if !incl.is_empty() {
        out.push_str(&format!(" INCLUDE ({})", incl.join(", ")));
    }
    out.push_str(";\n");
    Ok(out)
}

/// 하위 항목(제약 · 인덱스 · 트리거)의 DDL.
fn sub_ddl(
    s: &mut dyn Session,
    d: Dialect,
    o: &ObjectInfo,
    sk: SubKind,
    name: &str,
) -> Result<String, DbError> {
    let is_index = sk == SubKind::Indexes;
    let is_trigger = sk == SubKind::Triggers;
    match d {
        Dialect::Oracle => {
            if is_index {
                return oracle_get_ddl(s, "INDEX", name, &o.schema);
            }
            if is_trigger {
                return oracle_get_ddl(s, "TRIGGER", name, &o.schema);
            }
            // 제약: 일반 → 외래 키 순으로 시도(종류를 모르므로).
            oracle_get_ddl(s, "CONSTRAINT", name, &o.schema)
                .or_else(|_| oracle_get_ddl(s, "REF_CONSTRAINT", name, &o.schema))
        }
        Dialect::Postgres => {
            if is_index {
                let sql = format!(
                    "SELECT pg_get_indexdef({}::regclass)",
                    lit(&qual(d, &o.schema, name))
                );
                let rs = query(s, &sql)?;
                return Ok(format!(
                    "{};\n",
                    rs.rows.first().map(|r| col(r, 0)).unwrap_or_default()
                ));
            }
            if is_trigger {
                return source(s, &o.schema, ObjectKind::Trigger, name);
            }
            let sql = format!(
                "SELECT pg_get_constraintdef(oid, true) FROM pg_constraint WHERE conname = {} AND conrelid = {}::regclass",
                lit(name),
                lit(&q(d, o))
            );
            let rs = query(s, &sql)?;
            let def = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
            if def.is_empty() {
                return Err(no_ddl(o));
            }
            Ok(format!(
                "ALTER TABLE {} ADD CONSTRAINT {} {def};\n",
                q(d, o),
                ident(d, name)
            ))
        }
        Dialect::Mssql => {
            if is_index {
                return mssql_index_ddl(s, &o.schema, &o.name, name);
            }
            if is_trigger {
                return source(s, &o.schema, ObjectKind::Trigger, name);
            }
            let det = table_detail(s, &o.schema, &o.name)?;
            let Some(k) = det.keys.iter().find(|k| k.name.eq_ignore_ascii_case(name)) else {
                return Err(no_ddl(o));
            };
            match constraint_line(s, d, o, k)? {
                Some(line) => Ok(format!(
                    "ALTER TABLE {} ADD CONSTRAINT {} {line};\n",
                    q(d, o),
                    ident(d, name)
                )),
                None => Err(no_ddl(o)),
            }
        }
        Dialect::Sqlite => {
            if is_index || is_trigger {
                let sql = format!(
                    "SELECT sql FROM sqlite_master WHERE name = {} AND sql IS NOT NULL",
                    lit(name)
                );
                let rs = query(s, &sql)?;
                let body = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
                if body.is_empty() {
                    return Err(no_ddl(o));
                }
                return Ok(format!("{};\n", body.trim()));
            }
            // SQLite 제약은 테이블 정의 안에 있다 — 테이블 DDL을 낸다.
            let mut out =
                format!("-- SQLite keeps constraints inside the table definition ({name})\n");
            out.push_str(&object_ddl(s, d, o)?);
            Ok(out)
        }
        _ => {
            // 일반: 키·인덱스는 사전에서 만든 줄로.
            let det = table_detail(s, &o.schema, &o.name)?;
            if is_index {
                if let Some(i) = det
                    .indexes
                    .iter()
                    .find(|i| i.name.eq_ignore_ascii_case(name))
                {
                    let cols: Vec<String> = i.cols.iter().map(|c| ident(d, c)).collect();
                    return Ok(format!(
                        "CREATE {}INDEX {} ON {} ({});\n",
                        if i.unique { "UNIQUE " } else { "" },
                        ident(d, name),
                        q(d, o),
                        cols.join(", ")
                    ));
                }
                return Err(no_ddl(o));
            }
            if let Some(k) = det.keys.iter().find(|k| k.name.eq_ignore_ascii_case(name)) {
                if let Some(line) = constraint_line(s, d, o, k)? {
                    return Ok(format!(
                        "ALTER TABLE {} ADD CONSTRAINT {} {line};\n",
                        q(d, o),
                        ident(d, name)
                    ));
                }
            }
            Err(no_ddl(o))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(kind: ObjectKind) -> ObjectInfo {
        ObjectInfo {
            schema: "HR".into(),
            name: "EMP".into(),
            kind,
            status: String::new(),
            modified: String::new(),
            extra: String::new(),
        }
    }

    /// 83 §3 표: 테이블 = DML 넷 + MERGE + DDL · 뷰 = SELECT + DDL · 루틴·패키지 = CALL + DDL · 시퀀스 = SELECT + DDL ·
    /// 하위 항목 = 제약·인덱스·트리거만 DDL · 그 밖 = DDL.
    #[test]
    fn menu_items_follow_the_table() {
        use GenWhat::*;
        assert_eq!(
            gen_whats(Dialect::Oracle, ObjectKind::Table, None),
            vec![Select, Insert, Update, Delete, Merge, Ddl]
        );
        assert_eq!(
            gen_whats(Dialect::Odbc, ObjectKind::Table, None),
            vec![Select, Insert, Update, Delete, Ddl]
        );
        assert_eq!(
            gen_whats(Dialect::Mssql, ObjectKind::View, None),
            vec![Select, Ddl]
        );
        assert_eq!(
            gen_whats(Dialect::Oracle, ObjectKind::Package, None),
            vec![Call, Ddl]
        );
        assert_eq!(
            gen_whats(Dialect::Postgres, ObjectKind::Sequence, None),
            vec![Select, Ddl]
        );
        assert_eq!(
            gen_whats(Dialect::Oracle, ObjectKind::Synonym, None),
            vec![Ddl]
        );
        assert_eq!(
            gen_whats(Dialect::Oracle, ObjectKind::Table, Some(SubKind::Indexes)),
            vec![Ddl]
        );
        assert!(gen_whats(Dialect::Oracle, ObjectKind::Table, Some(SubKind::Columns)).is_empty());
        for w in GenWhat::ALL {
            assert_eq!(GenWhat::parse(w.code()), Some(w));
        }
        assert_eq!(GenWhat::parse("EXEC"), Some(Call));
    }

    /// MERGE 문 모양(방언별) — 키가 없으면 INSERT로 대신하고 주석.
    #[test]
    fn merge_shapes_per_dialect() {
        let names = vec!["ID".to_string(), "NAME".to_string()];
        let binds = vec![":ID".to_string(), ":NAME".to_string()];
        let ora = merge_sql(Dialect::Oracle, "HR.EMP", &names, &binds, &[0], &[1]);
        assert!(
            ora.contains("MERGE INTO HR.EMP t")
                && ora.contains("FROM DUAL")
                && ora.contains("ON (t.ID = s.ID)")
        );
        assert!(ora.contains("t.NAME = s.NAME") && ora.trim_end().ends_with(';'));
        let pg = merge_sql(Dialect::Postgres, "hr.emp", &names, &binds, &[0], &[1]);
        assert!(
            pg.contains("MERGE INTO hr.emp AS t")
                && pg.contains("ON t.ID = s.ID")
                && pg.contains("    NAME = s.NAME")
        );
        let lite = merge_sql(Dialect::Sqlite, "emp", &names, &binds, &[0], &[1]);
        assert!(
            lite.contains("ON CONFLICT (ID) DO UPDATE SET")
                && lite.contains("NAME = excluded.NAME")
        );
        let my = merge_sql(Dialect::Mysql, "emp", &names, &binds, &[0], &[1]);
        assert!(my.contains("ON DUPLICATE KEY UPDATE") && my.contains("NAME = VALUES(NAME)"));
        let none = merge_sql(Dialect::Oracle, "HR.EMP", &names, &binds, &[], &[0, 1]);
        assert!(none.starts_with("-- MERGE needs") && none.contains("INSERT INTO HR.EMP"));
    }

    #[test]
    fn spec_title_and_bind_names() {
        let sp = GenSpec {
            owner: info(ObjectKind::Table),
            what: GenWhat::Ddl,
            sub: Some((SubKind::Indexes, "EMP_PK".into())),
        };
        assert_eq!(sp.title(), "EMP.EMP_PK_ddl");
        assert_eq!(
            GenSpec {
                owner: info(ObjectKind::Table),
                what: GenWhat::Select,
                sub: None
            }
            .title(),
            "EMP_select"
        );
        assert_eq!(bind("Order Date"), ":Order_Date");
        assert_eq!(ident(Dialect::Oracle, "EMP"), "EMP");
        assert_eq!(ident(Dialect::Oracle, "Emp"), "\"Emp\"");
        assert_eq!(ident(Dialect::Postgres, "emp_v2"), "emp_v2");
        assert_eq!(ident(Dialect::Postgres, "Emp"), "\"Emp\"");
        assert_eq!(ident(Dialect::Mssql, "Order Date"), "[Order Date]");
        assert_eq!(qual(Dialect::Oracle, "HR", "EMP"), "HR.EMP");
        assert_eq!(oracle_var_type("REF CURSOR"), "REFCURSOR");
        assert_eq!(oracle_var_type("NUMBER"), "NUMBER");
        assert_eq!(oracle_var_type("DATE"), "VARCHAR2(4000)");
    }
}
