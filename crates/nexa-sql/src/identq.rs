//! ★ **식별자 인용 정책**(사용자 10-07 "필드 이름을 항상 `"`로 감쌀지 필요한 경우만인지 설정으로 · 두 경로 같은 기준 · DBMS별 표현"):
//! 조건 바에 열 이름을 넣는 모든 길(열 머리 DnD · 셀 우클릭 ▸ 조건 ▸ · 앞으로 생길 것)이 [`quote`] 하나를 지난다.
//! `SELECT *` 템플릿(탐색기 행 조회 · hover 카드 "데이터 200행" · 팔레트 `obj.rows`)의 **스키마·테이블 이름도 같은 정책**([`select_template`] ·
//! 사용자 10-08 "needed면 Database · Owner · Table에도 필요할 때만") — 종전엔 늘 `"BISCM"."T"` · `[dbo].[T]`였다.
//!
//! - 인용 문자 = 방언(nsql-catalog `quote_ident`): Oracle · PostgreSQL · SQLite · ODBC = `"…"` · SQL Server = `[…]` · MySQL = `` `…` ``.
//! - `always` = 늘 감싼다 · 아니면 [`needs_quote`]일 때만: 빈 이름 · 글자/`_`로 시작하지 않음 · 영숫자·`_` 밖의 글자(공백·`-`·`.`·한글 …) ·
//!   예약어(`SELECT` `ORDER` `GROUP` …) · **대소문자 접힘 규칙에 어긋남**(Oracle = 소문자가 있으면 저장된 이름이 대문자가 아니라는 뜻 →
//!   따옴표 없이는 못 찾는다 · PostgreSQL = 반대로 대문자가 있으면) — SQL Server · MySQL · SQLite는 대소문자를 구별하지 않아 규칙 없음.

use nsql_core::Dialect;

/// 이름을 방언·정책에 맞게 인용한다(순수).
pub(crate) fn quote(dialect: Dialect, name: &str, always: bool) -> String {
    if always || needs_quote(dialect, name) {
        nsql_catalog::quote_ident(dialect, name)
    } else {
        name.to_string()
    }
}

/// `schema.name` 한정 글 — 조각마다 [`quote`] 정책(스키마가 비면 이름만).
pub(crate) fn qualified(dialect: Dialect, schema: &str, name: &str, always: bool) -> String {
    if schema.is_empty() {
        quote(dialect, name, always)
    } else {
        format!(
            "{}.{}",
            quote(dialect, schema, always),
            quote(dialect, name, always)
        )
    }
}

/// `SELECT *` 200행 템플릿 — 한정 이름에 인용 정책을 적용한 뒤 nsql-catalog 본문(`select_template_q`)에.
pub(crate) fn select_template(dialect: Dialect, schema: &str, name: &str, always: bool) -> String {
    nsql_catalog::select_template_q(dialect, &qualified(dialect, schema, name, always))
}

/// 따옴표 없이 쓰면 다른 뜻이 되거나 오류가 나는 이름인가(순수 · 모듈 머리말 규칙).
pub(crate) fn needs_quote(dialect: Dialect, name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return true;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return true;
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return true;
    }
    if nsql_format::is_keyword(name) {
        return true;
    }
    match dialect {
        // 따옴표 없는 이름은 대문자로 접힌다 → 소문자가 섞인 이름은 따옴표로 만든 이름.
        Dialect::Oracle => name.chars().any(|c| c.is_ascii_lowercase()),
        // 소문자로 접힌다 → 대문자가 섞인 이름은 따옴표로 만든 이름.
        Dialect::Postgres => name.chars().any(|c| c.is_ascii_uppercase()),
        Dialect::Mssql | Dialect::Mysql | Dialect::Sqlite | Dialect::Odbc => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 방언별 인용 문자 · 필요 판정(특수문자 · 숫자 시작 · 예약어 · 접힘 규칙) · always.
    #[test]
    fn quote_policy_per_dialect() {
        assert_eq!(quote(Dialect::Oracle, "ITEM_CD", false), "ITEM_CD");
        assert_eq!(quote(Dialect::Oracle, "ITEM_CD", true), "\"ITEM_CD\"");
        assert_eq!(
            quote(Dialect::Oracle, "item_cd", false),
            "\"item_cd\"",
            "Oracle = 소문자 섞임"
        );
        assert_eq!(quote(Dialect::Postgres, "item_cd", false), "item_cd");
        assert_eq!(
            quote(Dialect::Postgres, "Item_Cd", false),
            "\"Item_Cd\"",
            "PG = 대문자 섞임"
        );
        assert_eq!(
            quote(Dialect::Mssql, "Item Cd", false),
            "[Item Cd]",
            "공백 · SQL Server 대괄호"
        );
        assert_eq!(quote(Dialect::Mssql, "Order", false), "[Order]", "예약어");
        assert_eq!(
            quote(Dialect::Mssql, "ItemCd", false),
            "ItemCd",
            "대소문자 무관"
        );
        assert_eq!(
            quote(Dialect::Mysql, "1st", false),
            "`1st`",
            "숫자 시작 · MySQL 백틱"
        );
        assert_eq!(
            quote(Dialect::Mysql, "a`b", true),
            "`a``b`",
            "인용 문자 두 번"
        );
        assert_eq!(
            quote(Dialect::Sqlite, "코드", false),
            "\"코드\"",
            "영숫자 밖"
        );
        assert_eq!(quote(Dialect::Oracle, "a\"b", false), "\"a\"\"b\"");
        assert!(needs_quote(Dialect::Oracle, ""));
    }

    /// ★ SELECT 템플릿의 스키마·테이블도 같은 정책(사용자 10-08): needed = 평범한 이름은 맨 글 · 접힘 위반·예약어만 인용 · always = 전부.
    #[test]
    fn select_template_follows_quote_policy() {
        assert_eq!(
            select_template(Dialect::Oracle, "BISCM", "APS_SCHE_IN_BOD_PATTERN", false),
            "SELECT * FROM BISCM.APS_SCHE_IN_BOD_PATTERN WHERE ROWNUM <= 200;"
        );
        assert_eq!(
            select_template(Dialect::Oracle, "BISCM", "APS_SCHE_IN_BOD_PATTERN", true),
            "SELECT * FROM \"BISCM\".\"APS_SCHE_IN_BOD_PATTERN\" WHERE ROWNUM <= 200;"
        );
        assert_eq!(
            select_template(Dialect::Mssql, "dbo", "LOT_BRAND", false),
            "SELECT TOP 200 * FROM dbo.LOT_BRAND;"
        );
        assert_eq!(
            select_template(Dialect::Mssql, "dbo", "Order", false),
            "SELECT TOP 200 * FROM dbo.[Order];",
            "예약어만 인용"
        );
        assert_eq!(
            select_template(Dialect::Oracle, "biscm", "T", false),
            "SELECT * FROM \"biscm\".T WHERE ROWNUM <= 200;",
            "Oracle 소문자 스키마 = 접힘 위반"
        );
        assert_eq!(
            select_template(Dialect::Postgres, "public", "t", false),
            "SELECT * FROM public.t LIMIT 200;"
        );
        assert_eq!(
            qualified(Dialect::Sqlite, "", "main table", false),
            "\"main table\""
        );
    }
}
