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

/// `offset`행을 건너뛰고 `limit`행만 — 방언별 페이징 문법으로 감싼 질의.
#[must_use]
pub fn page_sql(dialect: Dialect, sql: &str, offset: usize, limit: usize) -> String {
    let q = core(sql);
    match dialect {
        Dialect::Oracle => {
            format!("SELECT * FROM (\n{q}\n) OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY")
        }
        Dialect::Mssql => format!(
            "SELECT * FROM (\n{q}\n) x ORDER BY (SELECT NULL) OFFSET {offset} ROWS FETCH NEXT {limit} ROWS ONLY"
        ),
        Dialect::Postgres | Dialect::Mysql | Dialect::Sqlite | Dialect::Odbc => {
            format!("SELECT * FROM (\n{q}\n) x LIMIT {limit} OFFSET {offset}")
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
