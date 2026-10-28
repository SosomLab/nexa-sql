//! 추가 페치·건수용 질의 래핑(docs/43 §3-4 · D-70 OFFSET 폴백).
//!
//! 서버 커서를 유지하지 못할 때 **같은 세션에서 재질의**해 다음 세그먼트를 받는다. 정렬(ORDER BY) 없는 질의는
//! 페이지가 겹치거나 빠질 수 있다 — 호출자가 경고한다(설정 `grid.offset_warn`).

use nsql_core::Dialect;

/// 뒤의 `;`·공백을 뗀 본문(서브쿼리로 감싸기 위해).
fn core(sql: &str) -> &str {
    let mut s = sql.trim();
    while let Some(rest) = s.strip_suffix(';') {
        s = rest.trim_end();
    }
    s
}

/// ★ 10.5 폴백의 번호 열 이름(157차 · Db2 ≤ 10.5 = `ROW_NUMBER()` 감싸기 · 결과 끝에 붙어 오므로 호출자가 [`strip_rn`]으로 뗀다).
pub const RN_COL: &str = "NSQL_RN__";

/// `offset`행을 건너뛰고 `limit`행만(`limit` 0 = 남은 전부) — 방언별 페이징 문법으로 감싼 질의(`OFFSET … FETCH`를 받는 서버 전제).
#[must_use]
pub fn page_sql(dialect: Dialect, sql: &str, offset: usize, limit: usize) -> String {
    page_sql_opts(dialect, sql, offset, limit, true)
}

/// [`page_sql`] + `offset_fetch`(= [`nsql_core::Caps::offset_fetch`] · 세션이 서버 버전으로 안다) — 거짓이면(Db2 ≤ 10.5 · 157차)
/// `ROW_NUMBER() OVER ()`로 감싸 [`RN_COL`] 조건으로 자른다(번호 열이 결과 끝에 온다 → [`strip_rn`]).
#[must_use]
pub fn page_sql_opts(
    dialect: Dialect,
    sql: &str,
    offset: usize,
    limit: usize,
    offset_fetch: bool,
) -> String {
    let q = core(sql);
    if dialect == Dialect::Db2 && !offset_fetch {
        let hi = if limit > 0 {
            format!(" AND {RN_COL} <= {}", offset + limit)
        } else {
            String::new()
        };
        return format!(
            "SELECT * FROM (SELECT x.*, ROW_NUMBER() OVER () AS {RN_COL} FROM (\n{q}\n) x) y WHERE {RN_COL} > {offset}{hi}"
        );
    }
    match dialect {
        Dialect::Oracle => {
            let fetch = if limit > 0 {
                format!(" FETCH NEXT {limit} ROWS ONLY")
            } else {
                String::new()
            };
            format!("SELECT * FROM (\n{q}\n) OFFSET {offset} ROWS{fetch}")
        }
        // Db2(10-24 · D-359): `OFFSET … ROWS FETCH FIRST … ROWS ONLY`(11.1+ · fullselect에 바로).
        Dialect::Db2 => {
            let fetch = if limit > 0 {
                format!(" FETCH FIRST {limit} ROWS ONLY")
            } else {
                String::new()
            };
            format!("SELECT * FROM (\n{q}\n) x OFFSET {offset} ROWS{fetch}")
        }
        // Access(Jet) = OFFSET 없음 · `TOP n`만 → 첫 페이지만 자를 수 있다(뒷 페이지 = 호스트의 재질의 폴백).
        Dialect::Access => {
            if offset == 0 && limit > 0 {
                format!("SELECT TOP {limit} * FROM (\n{q}\n) x")
            } else {
                q.to_string()
            }
        }
        Dialect::Mssql => {
            let fetch = if limit > 0 {
                format!(" FETCH NEXT {limit} ROWS ONLY")
            } else {
                String::new()
            };
            format!("SELECT * FROM (\n{q}\n) x ORDER BY (SELECT NULL) OFFSET {offset} ROWS{fetch}")
        }
        Dialect::Postgres
        | Dialect::Mysql
        | Dialect::Sqlite
        | Dialect::Odbc
        | Dialect::Duckdb
        | Dialect::Clickhouse => {
            let lim = if limit > 0 {
                format!("LIMIT {limit} ")
            } else if dialect == Dialect::Sqlite || dialect == Dialect::Mysql {
                // SQLite·MySQL은 OFFSET만 못 쓴다 → LIMIT -1 / 큰 수.
                if dialect == Dialect::Sqlite {
                    "LIMIT -1 ".into()
                } else {
                    "LIMIT 18446744073709551615 ".into()
                }
            } else {
                String::new()
            };
            format!("SELECT * FROM (\n{q}\n) x {lim}OFFSET {offset}")
        }
    }
}

/// 결과의 끝 열이 [`RN_COL`]이면(10.5 폴백) 열과 값을 뗀다 — 그 밖은 그대로.
pub fn strip_rn(rs: &mut nsql_core::ResultSet) {
    if rs
        .columns
        .last()
        .is_some_and(|c| c.name.eq_ignore_ascii_case(RN_COL))
    {
        rs.columns.pop();
        for r in &mut rs.rows {
            r.pop();
        }
    }
}

/// `SELECT COUNT(*) FROM (질의) x`.
#[must_use]
pub fn count_sql(dialect: Dialect, sql: &str) -> String {
    let q = core(sql);
    match dialect {
        Dialect::Oracle => format!("SELECT COUNT(*) FROM (\n{q}\n)"),
        _ => format!("SELECT COUNT(*) FROM (\n{q}\n) x"),
    }
}

/// 질의에 최상위 `ORDER BY`가 있는가(대충 — 마지막 `ORDER BY` 토큰 · 서브쿼리 안이면 오판 가능 · 경고용).
#[must_use]
pub fn has_order_by(sql: &str) -> bool {
    let up = core(sql).to_ascii_uppercase();
    let Some(i) = up.rfind("ORDER BY") else {
        return false;
    };
    // 마지막 ORDER BY 뒤에 닫는 괄호가 없으면 최상위로 본다.
    !up[i..].contains(')')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_semicolon_and_wraps() {
        let s = page_sql(Dialect::Postgres, "select * from t;  ", 200, 200);
        assert_eq!(
            s,
            "SELECT * FROM (\nselect * from t\n) x LIMIT 200 OFFSET 200"
        );
        let o = page_sql(Dialect::Oracle, "select * from t", 0, 10);
        assert!(o.ends_with(") OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY"));
        let m = page_sql(Dialect::Mssql, "select * from t", 5, 5);
        assert!(m.contains("ORDER BY (SELECT NULL) OFFSET 5 ROWS FETCH NEXT 5 ROWS ONLY"));
    }

    /// `limit` 0 = 남은 전부(CLI `\all`의 OFFSET 폴백).
    #[test]
    fn zero_limit_means_rest() {
        assert!(
            page_sql(Dialect::Oracle, "select 1 from dual", 200, 0).ends_with(") OFFSET 200 ROWS")
        );
        assert!(page_sql(Dialect::Mssql, "select 1", 200, 0).ends_with("OFFSET 200 ROWS"));
        assert!(page_sql(Dialect::Postgres, "select 1", 200, 0).ends_with(") x OFFSET 200"));
        assert!(page_sql(Dialect::Sqlite, "select 1", 200, 0).ends_with(") x LIMIT -1 OFFSET 200"));
        assert!(page_sql(Dialect::Mysql, "select 1", 200, 0)
            .contains("LIMIT 18446744073709551615 OFFSET 200"));
    }

    /// 157차: Db2 ≤ 10.5 = `ROW_NUMBER()` 폴백(번호 열 끝) · strip은 그 열만 뗀다.
    #[test]
    fn db2_rownum_fallback_and_strip() {
        let q = page_sql_opts(
            Dialect::Db2,
            "select id from t order by id;",
            200,
            100,
            false,
        );
        assert_eq!(
            q,
            "SELECT * FROM (SELECT x.*, ROW_NUMBER() OVER () AS NSQL_RN__ FROM (\nselect id from t order by id\n) x) y WHERE NSQL_RN__ > 200 AND NSQL_RN__ <= 300"
        );
        assert!(page_sql_opts(Dialect::Db2, "select 1", 200, 0, false)
            .ends_with("WHERE NSQL_RN__ > 200"));
        assert_eq!(
            page_sql_opts(Dialect::Db2, "select 1", 200, 100, true),
            page_sql(Dialect::Db2, "select 1", 200, 100)
        );
        // 다른 방언은 offset_fetch 거짓이어도 그대로.
        assert_eq!(
            page_sql_opts(Dialect::Postgres, "select 1", 1, 2, false),
            page_sql(Dialect::Postgres, "select 1", 1, 2)
        );
        let col = |n: &str| nsql_core::Column {
            name: n.into(),
            type_name: String::new(),
        };
        let mut rs = nsql_core::ResultSet {
            columns: vec![col("ID"), col("NSQL_RN__")],
            rows: vec![
                vec![nsql_core::Value::Int(1), nsql_core::Value::Int(201)],
                vec![nsql_core::Value::Int(2), nsql_core::Value::Int(202)],
            ],
        };
        strip_rn(&mut rs);
        assert_eq!(rs.columns.len(), 1);
        assert_eq!(
            rs.rows,
            vec![
                vec![nsql_core::Value::Int(1)],
                vec![nsql_core::Value::Int(2)]
            ]
        );
        strip_rn(&mut rs);
        assert_eq!(rs.columns.len(), 1, "번호 열이 없으면 그대로");
    }

    #[test]
    fn count_and_order_detection() {
        assert_eq!(
            count_sql(Dialect::Oracle, "select 1 from dual;"),
            "SELECT COUNT(*) FROM (\nselect 1 from dual\n)"
        );
        assert!(count_sql(Dialect::Mssql, "select 1").ends_with(") x"));
        assert!(has_order_by("select * from t order by a"));
        assert!(!has_order_by(
            "select * from (select * from t order by a) x"
        ));
        assert!(!has_order_by("select * from t"));
    }
}
