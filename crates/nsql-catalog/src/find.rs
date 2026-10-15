//! ★ 객체 찾기(docs/113 §4 · 10-15): 컬럼·소스 본문을 **서버 질의 1회**로 찾는다 — 방언별 사전 뷰 LIKE(대소문자 · 이스케이프) ·
//! 결과는 (스키마, 객체, 종류, 컬럼/줄) 평면 목록 · 상한. 줄 번호 = Oracle `ALL_SOURCE`는 줄 단위(공짜) · 다른 DBMS는 정의 한 덩어리라
//! [`match_lines`]가 클라이언트에서 가른다. 낱말 경계는 서버 LIKE가 모르는 조건이라 클라이언트 후처리.
use super::{cell_str, col, lit, query, ObjectKind, Session};
use nsql_core::{DbError, Dialect, Value};

/// 찾기 조건(검색어 · 대소문자 구분 · 낱말 전체).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindPat {
    pub text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

/// 컬럼 일치 한 건.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColHit {
    pub schema: String,
    pub table: String,
    pub table_kind: ObjectKind,
    pub column: String,
    pub data_type: String,
}

/// 소스 일치 한 줄.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceHit {
    pub schema: String,
    pub name: String,
    pub kind: ObjectKind,
    /// 1부터 · 모르면 0.
    pub line: u32,
    pub text: String,
}

/// LIKE 패턴(`%` `_` `\`를 `\`로 이스케이프 · 양쪽 `%`).
pub fn like_pattern(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('%');
    for ch in text.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('%');
    out
}

/// 방언별 `LIKE` 술어(열 식 · 패턴 · 대소문자) — `ESCAPE '\'` 포함.
fn like_pred(d: Dialect, expr: &str, pat: &FindPat) -> String {
    let p = lit(&like_pattern(&pat.text));
    match (d, pat.case_sensitive) {
        (Dialect::Oracle, false) => format!("UPPER({expr}) LIKE UPPER({p}) ESCAPE '\\'"),
        (Dialect::Oracle, true) => format!("{expr} LIKE {p} ESCAPE '\\'"),
        (Dialect::Postgres, false) => format!("{expr} ILIKE {p} ESCAPE '\\'"),
        (Dialect::Postgres, true) => format!("{expr} LIKE {p} ESCAPE '\\'"),
        (Dialect::Mssql, false) => {
            format!("{expr} COLLATE Latin1_General_CI_AS LIKE {p} ESCAPE '\\'")
        }
        (Dialect::Mssql, true) => {
            format!("{expr} COLLATE Latin1_General_CS_AS LIKE {p} ESCAPE '\\'")
        }
        (Dialect::Sqlite, true) => format!("instr({expr}, {}) > 0", lit(&pat.text)),
        (Dialect::Mysql, true) => format!("{expr} LIKE BINARY {p} ESCAPE '\\'"),
        _ => format!("{expr} LIKE {p} ESCAPE '\\'"),
    }
}

/// `스키마 IN (...)` 조각(비면 없음).
fn schema_in(expr: &str, schemas: &[String]) -> String {
    if schemas.is_empty() {
        return String::new();
    }
    let list: Vec<String> = schemas.iter().map(|s| lit(s)).collect();
    format!(" AND {expr} IN ({})", list.join(", "))
}

/// 상한을 방언대로 감싼다(0 = 없음).
fn cap(d: Dialect, sql: String, max: usize) -> String {
    if max == 0 {
        return sql;
    }
    match d {
        Dialect::Oracle => format!("SELECT * FROM ({sql}) WHERE ROWNUM <= {max}"),
        Dialect::Mssql => sql.replacen("SELECT ", &format!("SELECT TOP ({max}) "), 1),
        _ => format!("{sql} LIMIT {max}"),
    }
}

/// 컬럼 찾기 SQL(순수 · 시험) — 열 = 스키마 · 표 · 표 종류 코드 · 컬럼 · 자료형.
pub fn columns_sql(d: Dialect, schemas: &[String], pat: &FindPat, max: usize) -> Option<String> {
    let sql = match d {
        Dialect::Oracle => format!(
            "SELECT c.owner, c.table_name, o.object_type, c.column_name, c.data_type FROM all_tab_columns c JOIN all_objects o ON o.owner = c.owner AND o.object_name = c.table_name AND o.object_type IN ('TABLE', 'VIEW', 'MATERIALIZED VIEW') WHERE {}{} ORDER BY c.owner, c.table_name, c.column_id",
            like_pred(d, "c.column_name", pat),
            schema_in("c.owner", schemas)
        ),
        Dialect::Postgres => format!(
            "SELECT n.nspname, c.relname, c.relkind::text, a.attname, format_type(a.atttypid, a.atttypmod) FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace WHERE a.attnum > 0 AND NOT a.attisdropped AND c.relkind IN ('r', 'v', 'm', 'p', 'f') AND {}{} ORDER BY n.nspname, c.relname, a.attnum",
            like_pred(d, "a.attname", pat),
            schema_in("n.nspname", schemas)
        ),
        Dialect::Mssql => format!(
            "SELECT s.name, o.name, o.type_desc, c.name, t.name FROM sys.columns c JOIN sys.objects o ON o.object_id = c.object_id JOIN sys.schemas s ON s.schema_id = o.schema_id JOIN sys.types t ON t.user_type_id = c.user_type_id WHERE o.type IN ('U', 'V') AND {}{} ORDER BY s.name, o.name, c.column_id",
            like_pred(d, "c.name", pat),
            schema_in("s.name", schemas)
        ),
        Dialect::Sqlite => format!(
            "SELECT 'main', m.name, m.type, p.name, p.type FROM sqlite_master m JOIN pragma_table_info(m.name) p WHERE m.type IN ('table', 'view') AND m.name NOT LIKE 'sqlite_%' AND {} ORDER BY m.name, p.cid",
            like_pred(d, "p.name", pat)
        ),
        Dialect::Mysql => format!(
            "SELECT table_schema, table_name, 'TABLE', column_name, column_type FROM information_schema.columns WHERE {}{} ORDER BY table_schema, table_name, ordinal_position",
            like_pred(d, "column_name", pat),
            schema_in("table_schema", schemas)
        ),
        Dialect::Odbc => return None,
    };
    Some(cap(d, sql, max))
}

/// 소스 찾기 SQL 목록(순수 · 시험) — 각 열 = 스키마 · 이름 · 종류 코드 · 줄(0 = 덩어리) · 글. 방언마다 질의 1~2개.
/// `kinds`가 비면 소스 있는 종류 전부.
pub fn source_sqls(
    d: Dialect,
    schemas: &[String],
    kinds: &[ObjectKind],
    pat: &FindPat,
    max: usize,
) -> Vec<String> {
    let want = |k: ObjectKind| kinds.is_empty() || kinds.contains(&k);
    let mut out = Vec::new();
    match d {
        Dialect::Oracle => {
            let mut types: Vec<&str> = Vec::new();
            if want(ObjectKind::Procedure) {
                types.push("'PROCEDURE'");
            }
            if want(ObjectKind::Function) {
                types.push("'FUNCTION'");
            }
            if want(ObjectKind::Package) {
                types.push("'PACKAGE'");
                types.push("'PACKAGE BODY'");
            }
            if want(ObjectKind::Trigger) {
                types.push("'TRIGGER'");
            }
            if want(ObjectKind::Type) {
                types.push("'TYPE'");
                types.push("'TYPE BODY'");
            }
            if !types.is_empty() {
                out.push(cap(
                    d,
                    format!(
                        "SELECT owner, name, type, line, text FROM all_source WHERE type IN ({}) AND {}{} ORDER BY owner, name, type, line",
                        types.join(", "),
                        like_pred(d, "text", pat),
                        schema_in("owner", schemas)
                    ),
                    max,
                ));
            }
            if want(ObjectKind::View) {
                // 12c+ `TEXT_VC`(VARCHAR2 4000) — LONG `TEXT`는 LIKE 불가(113 §1 시사점).
                out.push(cap(
                    d,
                    format!(
                        "SELECT owner, view_name, 'VIEW', 0, text_vc FROM all_views WHERE {}{} ORDER BY owner, view_name",
                        like_pred(d, "text_vc", pat),
                        schema_in("owner", schemas)
                    ),
                    max,
                ));
            }
        }
        Dialect::Postgres => {
            if want(ObjectKind::Function) || want(ObjectKind::Procedure) {
                out.push(cap(
                    d,
                    format!(
                        "SELECT n.nspname, p.proname, CASE p.prokind WHEN 'p' THEN 'PROCEDURE' ELSE 'FUNCTION' END, 0, p.prosrc FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE n.nspname NOT IN ('pg_catalog', 'information_schema') AND {}{} ORDER BY n.nspname, p.proname",
                        like_pred(d, "p.prosrc", pat),
                        schema_in("n.nspname", schemas)
                    ),
                    max,
                ));
            }
            if want(ObjectKind::View) {
                out.push(cap(
                    d,
                    format!(
                        "SELECT schemaname, viewname, 'VIEW', 0, definition FROM pg_views WHERE schemaname NOT IN ('pg_catalog', 'information_schema') AND {}{} ORDER BY schemaname, viewname",
                        like_pred(d, "definition", pat),
                        schema_in("schemaname", schemas)
                    ),
                    max,
                ));
            }
            if want(ObjectKind::MaterializedView) {
                out.push(cap(
                    d,
                    format!(
                        "SELECT schemaname, matviewname, 'MATERIALIZED VIEW', 0, definition FROM pg_matviews WHERE {}{} ORDER BY schemaname, matviewname",
                        like_pred(d, "definition", pat),
                        schema_in("schemaname", schemas)
                    ),
                    max,
                ));
            }
            if want(ObjectKind::Trigger) {
                out.push(cap(
                    d,
                    format!(
                        "SELECT n.nspname, t.tgname, 'TRIGGER', 0, pg_get_triggerdef(t.oid) FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid JOIN pg_namespace n ON n.oid = c.relnamespace WHERE NOT t.tgisinternal AND {}{} ORDER BY n.nspname, t.tgname",
                        like_pred(d, "pg_get_triggerdef(t.oid)", pat),
                        schema_in("n.nspname", schemas)
                    ),
                    max,
                ));
            }
        }
        Dialect::Mssql => {
            let mut types: Vec<&str> = Vec::new();
            if want(ObjectKind::Procedure) {
                types.push("'P'");
            }
            if want(ObjectKind::Function) {
                types.push("'FN'");
                types.push("'IF'");
                types.push("'TF'");
            }
            if want(ObjectKind::View) {
                types.push("'V'");
            }
            if want(ObjectKind::Trigger) {
                types.push("'TR'");
            }
            if !types.is_empty() {
                out.push(cap(
                    d,
                    format!(
                        "SELECT s.name, o.name, o.type_desc, 0, m.definition FROM sys.sql_modules m JOIN sys.objects o ON o.object_id = m.object_id JOIN sys.schemas s ON s.schema_id = o.schema_id WHERE o.type IN ({}) AND {}{} ORDER BY s.name, o.name",
                        types.join(", "),
                        like_pred(d, "m.definition", pat),
                        schema_in("s.name", schemas)
                    ),
                    max,
                ));
            }
        }
        Dialect::Sqlite => {
            let mut types: Vec<&str> = Vec::new();
            if want(ObjectKind::View) {
                types.push("'view'");
            }
            if want(ObjectKind::Trigger) {
                types.push("'trigger'");
            }
            if want(ObjectKind::Table) {
                types.push("'table'");
            }
            if !types.is_empty() {
                out.push(cap(
                    d,
                    format!(
                        "SELECT 'main', name, type, 0, sql FROM sqlite_master WHERE type IN ({}) AND sql IS NOT NULL AND name NOT LIKE 'sqlite_%' AND {} ORDER BY name",
                        types.join(", "),
                        like_pred(d, "sql", pat)
                    ),
                    max,
                ));
            }
        }
        Dialect::Mysql => {
            if want(ObjectKind::Procedure) || want(ObjectKind::Function) {
                out.push(cap(
                    d,
                    format!(
                        "SELECT routine_schema, routine_name, routine_type, 0, routine_definition FROM information_schema.routines WHERE {}{} ORDER BY routine_schema, routine_name",
                        like_pred(d, "routine_definition", pat),
                        schema_in("routine_schema", schemas)
                    ),
                    max,
                ));
            }
            if want(ObjectKind::View) {
                out.push(cap(
                    d,
                    format!(
                        "SELECT table_schema, table_name, 'VIEW', 0, view_definition FROM information_schema.views WHERE {}{} ORDER BY table_schema, table_name",
                        like_pred(d, "view_definition", pat),
                        schema_in("table_schema", schemas)
                    ),
                    max,
                ));
            }
            if want(ObjectKind::Trigger) {
                out.push(cap(
                    d,
                    format!(
                        "SELECT trigger_schema, trigger_name, 'TRIGGER', 0, action_statement FROM information_schema.triggers WHERE {}{} ORDER BY trigger_schema, trigger_name",
                        like_pred(d, "action_statement", pat),
                        schema_in("trigger_schema", schemas)
                    ),
                    max,
                ));
            }
        }
        Dialect::Odbc => {}
    }
    out
}

/// 종류 코드(사전 뷰 글) → `ObjectKind`.
pub fn kind_of(code: &str) -> ObjectKind {
    match code.trim().to_ascii_uppercase().as_str() {
        "TABLE" | "U" | "R" | "P" | "F" | "USER_TABLE" | "BASE TABLE" => ObjectKind::Table,
        "VIEW" | "V" => ObjectKind::View,
        "MATERIALIZED VIEW" | "M" => ObjectKind::MaterializedView,
        "PROCEDURE" | "SQL_STORED_PROCEDURE" => ObjectKind::Procedure,
        "FUNCTION"
        | "SQL_SCALAR_FUNCTION"
        | "SQL_TABLE_VALUED_FUNCTION"
        | "SQL_INLINE_TABLE_VALUED_FUNCTION" => ObjectKind::Function,
        "PACKAGE" => ObjectKind::Package,
        "PACKAGE BODY" => ObjectKind::PackageBody,
        "TRIGGER" | "SQL_TRIGGER" => ObjectKind::Trigger,
        "TYPE" | "TYPE BODY" => ObjectKind::Type,
        _ => ObjectKind::Table,
    }
}

/// 한 줄이 조건에 맞는가(대소문자 · 낱말 경계) — 서버 LIKE가 통과시킨 뒤의 클라이언트 판정.
pub fn line_matches(line: &str, pat: &FindPat) -> bool {
    let (hay, needle) = if pat.case_sensitive {
        (line.to_string(), pat.text.clone())
    } else {
        (line.to_lowercase(), pat.text.to_lowercase())
    };
    if needle.is_empty() {
        return false;
    }
    let mut from = 0usize;
    while let Some(i) = hay[from..].find(&needle) {
        let start = from + i;
        let end = start + needle.len();
        if !pat.whole_word {
            return true;
        }
        let before = hay[..start].chars().next_back();
        let after = hay[end..].chars().next();
        let is_w = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        if !is_w(before) && !is_w(after) {
            return true;
        }
        from = end;
        if from >= hay.len() {
            break;
        }
    }
    false
}

/// 정의 한 덩어리를 줄로 갈라 일치 줄만(1부터 · `per_obj` 상한 · 0 = 무제한).
pub fn match_lines(text: &str, pat: &FindPat, per_obj: usize) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for (i, l) in text.lines().enumerate() {
        if line_matches(l, pat) {
            out.push((i as u32 + 1, l.trim_end().to_string()));
            if per_obj > 0 && out.len() >= per_obj {
                break;
            }
        }
    }
    out
}

/// 컬럼 찾기(서버 질의 1회). 반환 = (일치, 잘렸는가).
pub fn columns(
    s: &mut dyn Session,
    schemas: &[String],
    pat: &FindPat,
    max: usize,
) -> Result<(Vec<ColHit>, bool), DbError> {
    let d = s.dialect();
    let Some(sql) = columns_sql(d, schemas, pat, max) else {
        return Ok((Vec::new(), false));
    };
    let rs = query(s, &sql)?;
    let mut out = Vec::new();
    for r in &rs.rows {
        let column = col(r, 3);
        if !line_matches(&column, pat) {
            continue;
        }
        out.push(ColHit {
            schema: col(r, 0),
            table: col(r, 1),
            table_kind: kind_of(&col(r, 2)),
            column,
            data_type: col(r, 4),
        });
    }
    let truncated = max > 0 && rs.rows.len() >= max;
    Ok((out, truncated))
}

/// 소스 찾기(서버 질의 1~2회). 반환 = (일치 줄, 잘렸는가). 덩어리 정의는 `match_lines`로 줄을 가른다.
pub fn source(
    s: &mut dyn Session,
    schemas: &[String],
    kinds: &[ObjectKind],
    pat: &FindPat,
    max: usize,
    per_obj: usize,
) -> Result<(Vec<SourceHit>, bool), DbError> {
    let d = s.dialect();
    let mut out = Vec::new();
    let mut truncated = false;
    let mut last_err: Option<DbError> = None;
    for sql in source_sqls(d, schemas, kinds, pat, max) {
        let rs = match query(s, &sql) {
            Ok(rs) => rs,
            Err(e) => {
                // 한 질의가 실패해도(권한 · 옛 버전 `TEXT_VC` 없음) 나머지는 계속 · 마지막 오류만 보고.
                last_err = Some(e);
                continue;
            }
        };
        truncated |= max > 0 && rs.rows.len() >= max;
        let mut per: std::collections::HashMap<(String, String, ObjectKind), usize> =
            std::collections::HashMap::new();
        for r in &rs.rows {
            let schema = col(r, 0);
            let name = col(r, 1);
            let kind = kind_of(&col(r, 2));
            let line = r
                .get(3)
                .filter(|v| !matches!(v, Value::Null))
                .map(cell_str)
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(0);
            let text = col(r, 4);
            let key = (schema.clone(), name.clone(), kind);
            if line > 0 {
                // 줄 단위(Oracle ALL_SOURCE) = 서버 LIKE가 걸렀지만 낱말 경계는 여기서.
                if !line_matches(&text, pat) {
                    continue;
                }
                let n = per.entry(key).or_insert(0);
                if per_obj > 0 && *n >= per_obj {
                    continue;
                }
                *n += 1;
                out.push(SourceHit {
                    schema,
                    name,
                    kind,
                    line,
                    text: text.trim_end().to_string(),
                });
            } else {
                for (ln, t) in match_lines(&text, pat, per_obj) {
                    out.push(SourceHit {
                        schema: schema.clone(),
                        name: name.clone(),
                        kind,
                        line: ln,
                        text: t,
                    });
                }
            }
        }
    }
    if out.is_empty() {
        if let Some(e) = last_err {
            return Err(e);
        }
    }
    Ok((out, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pat(text: &str, cs: bool, ww: bool) -> FindPat {
        FindPat {
            text: text.into(),
            case_sensitive: cs,
            whole_word: ww,
        }
    }

    #[test]
    fn like_pattern_escapes_wildcards() {
        assert_eq!(like_pattern("a%b_c\\d"), "%a\\%b\\_c\\\\d%");
        assert_eq!(like_pattern("cust"), "%cust%");
    }

    #[test]
    fn columns_sql_per_dialect() {
        let p = pat("cust", false, false);
        let ora = columns_sql(Dialect::Oracle, &["HR".into()], &p, 100).unwrap();
        assert!(ora.contains("UPPER(c.column_name) LIKE UPPER('%cust%') ESCAPE '\\'"));
        assert!(ora.contains("c.owner IN ('HR')") && ora.contains("ROWNUM <= 100"));
        let pg = columns_sql(Dialect::Postgres, &[], &p, 0).unwrap();
        assert!(pg.contains("a.attname ILIKE '%cust%'") && !pg.contains("LIMIT"));
        let ms = columns_sql(Dialect::Mssql, &[], &pat("Cust", true, false), 50).unwrap();
        assert!(
            ms.starts_with("SELECT TOP (50) ") && ms.contains("COLLATE Latin1_General_CS_AS LIKE")
        );
        let lite = columns_sql(Dialect::Sqlite, &[], &pat("x", true, false), 10).unwrap();
        assert!(lite.contains("instr(p.name, 'x') > 0") && lite.ends_with("LIMIT 10"));
        assert!(columns_sql(Dialect::Odbc, &[], &p, 10).is_none());
    }

    #[test]
    fn source_sqls_follow_kinds() {
        let p = pat("customer", false, false);
        let all = source_sqls(Dialect::Oracle, &[], &[], &p, 0);
        assert_eq!(all.len(), 2, "ALL_SOURCE + ALL_VIEWS");
        assert!(all[0].contains("'PACKAGE BODY'") && all[1].contains("text_vc"));
        let only_views = source_sqls(Dialect::Oracle, &[], &[ObjectKind::View], &p, 0);
        assert_eq!(only_views.len(), 1);
        let pg = source_sqls(
            Dialect::Postgres,
            &["public".into()],
            &[ObjectKind::Trigger],
            &p,
            5,
        );
        assert_eq!(pg.len(), 1);
        assert!(pg[0].contains("pg_get_triggerdef") && pg[0].ends_with("LIMIT 5"));
        let ms = source_sqls(
            Dialect::Mssql,
            &[],
            &[ObjectKind::Procedure, ObjectKind::View],
            &p,
            0,
        );
        assert!(ms[0].contains("o.type IN ('P', 'V')"));
        assert!(source_sqls(Dialect::Mssql, &[], &[ObjectKind::Sequence], &p, 0).is_empty());
        let lite = source_sqls(Dialect::Sqlite, &[], &[], &p, 0);
        assert!(lite[0].contains("sqlite_master") && lite[0].contains("'table'"));
    }

    #[test]
    fn kind_codes() {
        assert_eq!(kind_of("SQL_STORED_PROCEDURE"), ObjectKind::Procedure);
        assert_eq!(kind_of("PACKAGE BODY"), ObjectKind::PackageBody);
        assert_eq!(kind_of("v"), ObjectKind::View);
        assert_eq!(kind_of("m"), ObjectKind::MaterializedView);
        assert_eq!(kind_of("trigger"), ObjectKind::Trigger);
    }

    #[test]
    fn line_matching_case_and_word() {
        assert!(line_matches(
            "SELECT Customer_id FROM t",
            &pat("customer", false, false)
        ));
        assert!(!line_matches(
            "SELECT Customer_id FROM t",
            &pat("customer", true, false)
        ));
        assert!(
            !line_matches("SELECT customer_id FROM t", &pat("customer", false, true)),
            "customer_id = 낱말 아님"
        );
        assert!(line_matches(
            "SELECT customer FROM t",
            &pat("customer", false, true)
        ));
        assert!(
            line_matches("x.customer.y", &pat("customer", false, true)),
            "점은 경계"
        );
        assert!(!line_matches("abc", &pat("", false, false)));
    }

    #[test]
    fn match_lines_numbers_and_cap() {
        let text =
            "CREATE VIEW v AS\n  SELECT customer\n  FROM customer_base\n  WHERE Customer > 0";
        let p = pat("customer", false, false);
        let all = match_lines(text, &p, 0);
        assert_eq!(
            all.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(match_lines(text, &p, 2).len(), 2);
        let ww = match_lines(text, &pat("customer", false, true), 0);
        assert_eq!(ww.iter().map(|(n, _)| *n).collect::<Vec<_>>(), vec![2, 4]);
    }
}
