//! 방언별 재작성 — 같은 `:NAME` 스크립트가 각 DBMS에서 동작하게(docs/08 §4 · docs/05 §7).
//!
//! | 방언 | 플레이스홀더 | 모드 |
//! |---|---|---|
//! | Oracle | `:NAME` 그대로 | 드라이버 이름 바인드 · `EXEC` → `BEGIN … END;` |
//! | SQL Server | `@NAME` | 1차 `sp_executesql` 파라미터(OUTPUT) · 2차 `DECLARE` 프리펜드(배치 첫 문장 DDL·USE·SET) |
//! | PostgreSQL | `$n` | 위치 바인드 |
//! | MySQL·SQLite·ODBC | `?` | 위치 바인드 |
//!
//! Oracle 관용 `SELECT … INTO :A, :B FROM …`은 T-SQL `SELECT @A = …, @B = … FROM …`으로 옮긴다.

use crate::bind::{extract_binds, unique_names, BindRef};
use crate::lexer::{classify, find_word_ci};
use crate::vars::VarStore;
use nsql_core::{BindParam, Dialect, Direction, ExecRequest, Value, VarType};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrepareMode {
    /// 드라이버가 파라미터를 바인딩한다(Oracle 이름 · PG/MySQL 위치 · MSSQL RPC).
    Bind,
    /// 값을 리터럴로 배치 앞에 선언(`DECLARE @v … = …;`) — 바인드가 불가능한 T-SQL 배치.
    DeclarePrepend,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Prepared {
    pub sql: String,
    pub params: Vec<BindParam>,
    pub mode: PrepareMode,
    /// 원문에 없던 변수를 암묵 생성했을 때 이름들(호스트가 경고·프롬프트).
    pub implicit: Vec<String>,
    /// 프리펜드로 늘어난 줄 수 — 오류 줄번호 보정용.
    pub line_offset: usize,
    /// ★ **잡을 행**(docs/63 §3 `captures`): OUT 바인드가 없는 방언에서 `EXEC :V := 식` · `EXEC SELECT … INTO :A, :B`를
    /// 보통 조회로 바꿔 보냈을 때, 돌아온 **한 행을 자리 순서대로** 받을 변수 이름. 비어 있으면 잡지 않는다(결과는 결과다).
    /// 0행·여러 행 정책은 러너 몫(D-139 · 설정 `vars.into_policy`).
    pub captures: Vec<String>,
}

impl Prepared {
    /// 바인드를 찾지 않고 **원문 그대로** — 저장 코드를 만드는 DDL(`CREATE … PROCEDURE|FUNCTION|PACKAGE|TRIGGER|TYPE`)용.
    /// 본문의 `:NEW`/`:OLD`(트리거 의사 레코드)나 `:x`는 클라이언트 변수가 아니다 — 종전에는 이것들이 암묵 변수로 만들어져
    /// 바인드됐다(트리거 DDL = ORA-01027 위험 · 변수 표 오염 · 09-21 점검).
    #[must_use]
    pub fn verbatim(sql: &str) -> Prepared {
        Prepared {
            sql: sql.to_string(),
            params: Vec::new(),
            mode: PrepareMode::Bind,
            implicit: Vec::new(),
            line_offset: 0,
            captures: Vec::new(),
        }
    }

    pub fn into_request(self) -> ExecRequest {
        ExecRequest {
            sql: self.sql,
            params: self.params,
        }
    }
}

/// `sql`의 `:NAME`을 방언에 맞춰 재작성하고 저장소 값을 붙인다.
/// `inout`이 참이면(PL/SQL 블록·EXEC) 모든 바인드가 InOut — 실행 후 값이 돌아온다.
pub fn prepare(dialect: Dialect, sql: &str, vars: &mut VarStore, inout: bool) -> Prepared {
    let refs = extract_binds(sql);
    let names = unique_names(&refs);
    let mut implicit = Vec::new();
    for n in &names {
        if !vars.contains(n) {
            vars.assign(n, Value::Null);
            implicit.push(n.clone());
        }
    }
    let dir = if inout {
        Direction::InOut
    } else {
        Direction::In
    };
    let param_for = |n: &str| -> BindParam {
        let v = vars.get(n).expect("just ensured");
        BindParam {
            name: n.to_string(),
            value: v.value.clone(),
            ty: v.ty.clone(),
            direction: dir,
        }
    };
    match dialect {
        Dialect::Oracle => Prepared {
            sql: sql.to_string(),
            params: names.iter().map(|n| param_for(n)).collect(),
            mode: PrepareMode::Bind,
            implicit,
            line_offset: 0,
            captures: Vec::new(),
        },
        Dialect::Mssql => {
            let rewritten = rewrite_select_into_tsql(sql);
            let rewritten = replace_refs(&rewritten, &extract_binds(&rewritten), |r| {
                format!("@{}", r.name)
            });
            let params: Vec<BindParam> = names.iter().map(|n| param_for(n)).collect();
            if needs_declare_prepend(&rewritten) {
                let mut head = String::new();
                for p in &params {
                    head.push_str(&format!(
                        "DECLARE @{} {} = {};\n",
                        p.name,
                        p.ty.tsql_type(),
                        p.value.to_sql_literal(Dialect::Mssql)
                    ));
                }
                let line_offset = params.len();
                Prepared {
                    sql: format!("{head}{rewritten}"),
                    params: Vec::new(),
                    mode: PrepareMode::DeclarePrepend,
                    implicit,
                    line_offset,
                    captures: Vec::new(),
                }
            } else {
                Prepared {
                    sql: rewritten,
                    params,
                    mode: PrepareMode::Bind,
                    implicit,
                    line_offset: 0,
                    captures: Vec::new(),
                }
            }
        }
        Dialect::Postgres => {
            let sql2 = replace_refs(sql, &refs, |r| {
                let idx = names.iter().position(|n| n == &r.name).map_or(0, |i| i + 1);
                format!("${idx}")
            });
            Prepared {
                sql: sql2,
                params: names.iter().map(|n| param_for(n)).collect(),
                mode: PrepareMode::Bind,
                implicit,
                line_offset: 0,
                captures: Vec::new(),
            }
        }
        Dialect::Mysql | Dialect::Sqlite | Dialect::Odbc => {
            // `?`는 등장 순서대로 — 같은 변수가 두 번 나오면 두 번 보낸다.
            let sql2 = replace_refs(sql, &refs, |_| "?".to_string());
            let params = refs.iter().map(|r| param_for(&r.name)).collect();
            Prepared {
                sql: sql2,
                params,
                mode: PrepareMode::Bind,
                implicit,
                line_offset: 0,
                captures: Vec::new(),
            }
        }
    }
}

fn replace_refs(sql: &str, refs: &[BindRef], f: impl Fn(&BindRef) -> String) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut last = 0;
    for r in refs {
        out.push_str(&sql[last..r.start]);
        out.push_str(&f(r));
        last = r.end;
    }
    out.push_str(&sql[last..]);
    out
}

/// `sp_executesql`로 감쌀 수 없는 배치(docs/05 §7 (a) 단점) — 첫 문장 제약 DDL · `USE` · `SET` 지속.
/// ★ `SET @V = 식`은 세션 옵션이 아니라 **변수 대입**(`EXEC :V := 식`의 재작성)이다 — 앞붙임 길로 보내면 바인드가 비어
/// 값이 돌아오지 않는다(09-21 실서버: `EXEC :V := DB_NAME()`이 오류 없이 빈 값).
pub fn needs_declare_prepend(tsql: &str) -> bool {
    let up = tsql.trim_start().to_ascii_uppercase();
    let w: Vec<&str> = up.split_whitespace().take(3).collect();
    match w.first().copied() {
        Some("SET") => !w.get(1).is_some_and(|t| t.starts_with('@')),
        Some("USE") => true,
        Some("CREATE") | Some("ALTER") => {
            // CREATE [OR ALTER] <target> — 배치 첫 문장이어야 하는 대상만.
            let w4: Vec<&str> = up.split_whitespace().take(4).collect();
            let target = if w4.get(1).copied() == Some("OR") {
                w4.get(3).copied()
            } else {
                w4.get(1).copied()
            };
            matches!(
                target,
                Some("PROCEDURE")
                    | Some("PROC")
                    | Some("FUNCTION")
                    | Some("VIEW")
                    | Some("TRIGGER")
                    | Some("SCHEMA")
                    | Some("DEFAULT")
                    | Some("RULE")
            )
        }
        _ => false,
    }
}

/// OUT 바인드가 없는 방언(PG · MySQL · SQLite · ODBC): `SELECT a, b INTO :X, :Y FROM …` → `SELECT a AS "X", b AS "Y" FROM …`.
/// 호스트가 1행 결과의 컬럼 이름을 변수로 흡수한다(`EXEC :V := expr`와 같은 규약). INTO가 없거나 개수가 어긋나면 원문.
pub fn rewrite_select_into_alias(sql: &str) -> String {
    let classes = classify(sql);
    if !sql.trim_start().to_ascii_uppercase().starts_with("SELECT") {
        return sql.to_string();
    }
    let Some(into) = find_word_ci(sql, &classes, "INTO") else {
        return sql.to_string();
    };
    let Some(from_rel) = find_word_ci(&sql[into..], &classify(&sql[into..]), "FROM") else {
        return sql.to_string();
    };
    let from = into + from_rel;
    let select_kw = sql.to_ascii_uppercase().find("SELECT").unwrap_or(0);
    let (prefix, select_list) = split_select_prefix(&sql[select_kw + 6..into]);
    let cols = split_top_level_commas(select_list);
    let targets: Vec<&str> = sql[into + 4..from]
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if cols.len() != targets.len() || targets.iter().any(|t| !t.starts_with(':')) {
        return sql.to_string();
    }
    let aliased: Vec<String> = cols
        .iter()
        .zip(targets.iter())
        .map(|(c, t)| {
            format!(
                "{} AS \"{}\"",
                c.trim(),
                t.trim_start_matches(':').to_ascii_uppercase()
            )
        })
        .collect();
    let prefix = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix} ")
    };
    format!(
        "SELECT {prefix}{} {}",
        aliased.join(", "),
        sql[from..].trim()
    )
}

/// Oracle `SELECT a, b INTO :X, :Y FROM …` → T-SQL 대입 조회 `SELECT :X = a, :Y = b FROM …`.
/// 바인드 표기(`:X`)는 남겨 둔다 — [`prepare`]가 `@X`로 바꾸며 파라미터를 붙인다(순서 중요).
/// INTO가 없거나(일반 조회) 목록 개수가 어긋나면 원문 그대로.
pub fn rewrite_select_into_tsql(sql: &str) -> String {
    let classes = classify(sql);
    let up_starts = sql.trim_start().to_ascii_uppercase().starts_with("SELECT");
    if !up_starts {
        return sql.to_string();
    }
    let Some(into) = find_word_ci(sql, &classes, "INTO") else {
        return sql.to_string();
    };
    let Some(from_rel) = find_word_ci(&sql[into..], &classify(&sql[into..]), "FROM") else {
        return sql.to_string();
    };
    let from = into + from_rel;
    let select_kw = sql.to_ascii_uppercase().find("SELECT").unwrap_or(0);
    let select_list = &sql[select_kw + 6..into];
    // `TOP n` · `TOP (n)` · `DISTINCT` 접두는 대입 앞에 남긴다(T-SQL: SELECT TOP 1 @v = col …).
    let (prefix, select_list) = split_select_prefix(select_list);
    let into_list = &sql[into + 4..from];
    let cols = split_top_level_commas(select_list);
    let targets: Vec<&str> = into_list
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if cols.len() != targets.len() || targets.iter().any(|t| !t.starts_with(':')) {
        return sql.to_string();
    }
    let assigns: Vec<String> = cols
        .iter()
        .zip(targets.iter())
        .map(|(c, t)| {
            format!(
                ":{} = {}",
                t.trim_start_matches(':').to_ascii_uppercase(),
                c.trim()
            )
        })
        .collect();
    let prefix = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix} ")
    };
    format!(
        "{}SELECT {prefix}{} {}",
        &sql[..select_kw],
        assigns.join(", "),
        sql[from..].trim_start()
    )
}

/// 선택 목록 앞의 `DISTINCT` · `TOP n` · `TOP (n) [PERCENT] [WITH TIES]`를 떼어낸다.
fn split_select_prefix(list: &str) -> (String, &str) {
    let mut rest = list.trim_start();
    let mut prefix: Vec<String> = Vec::new();
    loop {
        let up = rest.to_ascii_uppercase();
        if up.starts_with("DISTINCT") && rest[8..].starts_with(|c: char| c.is_whitespace()) {
            prefix.push("DISTINCT".into());
            rest = rest[8..].trim_start();
            continue;
        }
        if up.starts_with("ALL ") {
            prefix.push("ALL".into());
            rest = rest[3..].trim_start();
            continue;
        }
        if up.starts_with("TOP") && rest[3..].starts_with(|c: char| c.is_whitespace() || c == '(') {
            let after = rest[3..].trim_start();
            let n = if let Some(inner) = after.strip_prefix('(') {
                let close = inner.find(')').map_or(inner.len(), |i| i + 1);
                after[..close + 1].to_string()
            } else {
                after
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
            };
            if n.is_empty() {
                break;
            }
            let mut tail = after[n.len()..].trim_start();
            let mut extra = String::new();
            for kw in ["PERCENT", "WITH TIES"] {
                if tail.to_ascii_uppercase().starts_with(kw) {
                    extra.push(' ');
                    extra.push_str(kw);
                    tail = tail[kw.len()..].trim_start();
                }
            }
            prefix.push(format!("TOP {n}{extra}"));
            rest = tail;
            continue;
        }
        break;
    }
    (prefix.join(" "), rest)
}

/// 괄호 깊이 0의 `,`로 나눈다(문자열·주석은 [`classify`]로 보호).
fn split_top_level_commas(s: &str) -> Vec<String> {
    let classes = classify(s);
    let b = s.as_bytes();
    let mut depth = 0i32;
    let mut parts = Vec::new();
    let mut last = 0;
    for i in 0..b.len() {
        if classes[i] != crate::lexer::Class::Code {
            continue;
        }
        match b[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(s[last..i].trim().to_string());
                last = i + 1;
            }
            _ => {}
        }
    }
    let tail = s[last..].trim();
    if !tail.is_empty() {
        parts.push(tail.to_string());
    }
    parts
}

/// 식에 `(SELECT`/`(WITH` 서브쿼리가 있는가(대소문자·공백 무시).
fn contains_subquery(expr: &str) -> bool {
    let up = expr.to_ascii_uppercase();
    let mut i = 0;
    let b = up.as_bytes();
    while let Some(off) = up[i..].find('(') {
        let rest = up[i + off + 1..].trim_start();
        if rest.starts_with("SELECT") || rest.starts_with("WITH") {
            let kw_len = if rest.starts_with("SELECT") { 6 } else { 4 };
            // 키워드 뒤가 식별자 문자가 아니어야 한다(SELECTED 같은 이름 제외).
            if rest
                .as_bytes()
                .get(kw_len)
                .is_none_or(|c| !crate::lexer::is_ident_char(*c))
            {
                return true;
            }
        }
        i += off + 1;
        if i >= b.len() {
            break;
        }
    }
    false
}

/// `EXEC 본문`을 방언별 실행 문장으로.
pub fn wrap_exec(dialect: Dialect, body: &str) -> String {
    let body = body.trim().trim_end_matches(';').trim();
    match dialect {
        Dialect::Oracle => {
            // ★ `:V := (SELECT …)` — PL/SQL은 식 안의 서브쿼리를 허용하지 않는다(PLS-00103 · 19c 실서버 09-13).
            // `SELECT (…) INTO :V FROM DUAL`로 바꾸면 스칼라 서브쿼리·SYSDATE·함수 호출이 모두 SQL 식으로 평가된다.
            if let Some((lhs, rhs)) = body.split_once(":=") {
                let (lhs, rhs) = (lhs.trim(), rhs.trim());
                if let Some(name) = lhs.strip_prefix(':') {
                    if name.bytes().all(crate::lexer::is_ident_char) && contains_subquery(rhs) {
                        return format!(
                            "BEGIN SELECT {rhs} INTO :{} FROM DUAL; END;",
                            name.to_ascii_uppercase()
                        );
                    }
                }
            }
            format!("BEGIN {body}; END;")
        }
        Dialect::Mssql => {
            // `:V := expr` → `SET @V = expr` · 그 외는 T-SQL EXEC 그대로.
            if let Some((lhs, rhs)) = body.split_once(":=") {
                let lhs = lhs.trim();
                if let Some(name) = lhs.strip_prefix(':') {
                    if name.bytes().all(crate::lexer::is_ident_char) {
                        return format!("SET :{} = {}", name.to_ascii_uppercase(), rhs.trim());
                    }
                }
            }
            let up = body.to_ascii_uppercase();
            if up.starts_with("SELECT") {
                rewrite_select_into_tsql(body)
            } else {
                format!("EXEC {body}")
            }
        }
        Dialect::Postgres => {
            if let Some((lhs, rhs)) = body.split_once(":=") {
                if let Some(name) = lhs.trim().strip_prefix(':') {
                    return format!("SELECT {} AS \"{}\"", rhs.trim(), name.to_ascii_uppercase());
                }
            }
            if body.to_ascii_uppercase().starts_with("SELECT") {
                rewrite_select_into_alias(body)
            } else {
                format!("CALL {body}")
            }
        }
        Dialect::Mysql | Dialect::Sqlite | Dialect::Odbc => {
            if let Some((lhs, rhs)) = body.split_once(":=") {
                if let Some(name) = lhs.trim().strip_prefix(':') {
                    return format!("SELECT {} AS \"{}\"", rhs.trim(), name.to_ascii_uppercase());
                }
            }
            if body.to_ascii_uppercase().starts_with("SELECT") {
                rewrite_select_into_alias(body)
            } else {
                format!("CALL {body}")
            }
        }
    }
}

/// 실행 후 이름별 OUT 값을 저장소 키로 정규화해 돌려준다(위치 바인드 방언 대비).
pub fn out_type_hint(ty: &VarType) -> &'static str {
    match ty {
        VarType::RefCursor => "cursor",
        VarType::Number | VarType::BinaryFloat | VarType::BinaryDouble => "number",
        _ => "text",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> VarStore {
        let mut v = VarStore::new();
        v.assign("V_PROJECT_CD", Value::Str("SEBANG".into()));
        v.assign("V_MP_VRSN_SEQ", Value::Int(3));
        v
    }

    #[test]
    fn oracle_passes_named_binds_through() {
        let mut v = store();
        let p = prepare(
            Dialect::Oracle,
            "SELECT :V_PROJECT_CD, :V_MP_VRSN_SEQ, :V_NEW FROM DUAL",
            &mut v,
            false,
        );
        assert_eq!(
            p.sql,
            "SELECT :V_PROJECT_CD, :V_MP_VRSN_SEQ, :V_NEW FROM DUAL"
        );
        assert_eq!(p.params.len(), 3);
        assert_eq!(p.params[0].value, Value::Str("SEBANG".into()));
        assert_eq!(p.implicit, vec!["V_NEW"]);
        assert_eq!(p.mode, PrepareMode::Bind);
    }

    #[test]
    fn mssql_uses_at_params_and_output_for_blocks() {
        let mut v = store();
        let p = prepare(
            Dialect::Mssql,
            "SELECT :V_PROJECT_CD AS cd, :V_MP_VRSN_SEQ AS seq",
            &mut v,
            false,
        );
        assert_eq!(p.sql, "SELECT @V_PROJECT_CD AS cd, @V_MP_VRSN_SEQ AS seq");
        assert_eq!(p.params[0].direction, Direction::In);
        assert_eq!(p.params[1].ty.tsql_type(), "DECIMAL(38,10)");
        let p = prepare(Dialect::Mssql, "SELECT @x = 1", &mut v, true);
        assert!(p.params.is_empty());
    }

    #[test]
    fn mssql_declare_prepend_for_batch_first_ddl() {
        let mut v = store();
        let p = prepare(
            Dialect::Mssql,
            "CREATE PROCEDURE p AS SELECT :V_PROJECT_CD",
            &mut v,
            false,
        );
        assert_eq!(p.mode, PrepareMode::DeclarePrepend);
        assert!(
            p.sql
                .starts_with("DECLARE @V_PROJECT_CD NVARCHAR(6) = N'SEBANG';\nCREATE PROCEDURE"),
            "{}",
            p.sql
        );
        assert_eq!(p.line_offset, 1);
        assert!(p.params.is_empty());
    }

    /// `SET @V = 식`(= `EXEC :V := 식`)은 바인드 길 — 세션 옵션 `SET …` · `USE`만 앞붙임 길(09-21 실서버 결함).
    #[test]
    fn mssql_set_variable_is_bound_not_prepended() {
        assert!(!needs_declare_prepend("SET @V_DB = DB_NAME()"));
        assert!(!needs_declare_prepend("  set @n = @n + 1"));
        assert!(needs_declare_prepend("SET NOCOUNT ON"));
        assert!(needs_declare_prepend("SET SHOWPLAN_TEXT ON"));
        assert!(needs_declare_prepend("USE tempdb"));
        let mut v = store();
        let p = prepare(
            Dialect::Mssql,
            &wrap_exec(Dialect::Mssql, ":V_NEW := DB_NAME()"),
            &mut v,
            true,
        );
        assert_eq!(p.mode, PrepareMode::Bind);
        assert_eq!(p.sql, "SET @V_NEW = DB_NAME()");
        assert_eq!(p.params.len(), 1);
        assert_eq!(p.params[0].direction, Direction::InOut);
    }

    #[test]
    fn select_into_rewrites_to_assignment_select() {
        let sql = "SELECT\n\tA.PROJECT_CD\n,\tNVL(A.MP_VRSN_SEQ, 0)\nINTO\n\t:V_PROJECT_CD\n,\t:V_MP_VRSN_SEQ\nFROM\n\tM4S_O301010 A\nWHERE A.PROJECT_CD = 'SEBANG'";
        let t = rewrite_select_into_tsql(sql);
        assert_eq!(t, "SELECT :V_PROJECT_CD = A.PROJECT_CD, :V_MP_VRSN_SEQ = NVL(A.MP_VRSN_SEQ, 0) FROM\n\tM4S_O301010 A\nWHERE A.PROJECT_CD = 'SEBANG'");
        assert_eq!(
            rewrite_select_into_tsql("SELECT 1 FROM t"),
            "SELECT 1 FROM t"
        );
        assert_eq!(
            rewrite_select_into_tsql("INSERT INTO t SELECT 1"),
            "INSERT INTO t SELECT 1"
        );
    }

    #[test]
    fn select_into_alias_for_out_less_dialects() {
        assert_eq!(
            rewrite_select_into_alias("SELECT a, COUNT(*) INTO :X, :Y FROM t WHERE k = 1"),
            "SELECT a AS \"X\", COUNT(*) AS \"Y\" FROM t WHERE k = 1"
        );
        assert_eq!(
            rewrite_select_into_alias("SELECT 1 FROM t"),
            "SELECT 1 FROM t"
        );
        assert_eq!(
            wrap_exec(
                Dialect::Postgres,
                "SELECT COUNT(*) INTO :V_CNT FROM (SELECT 1) t"
            ),
            "SELECT COUNT(*) AS \"V_CNT\" FROM (SELECT 1) t"
        );
        assert_eq!(
            wrap_exec(Dialect::Postgres, "my_proc(1, NULL)"),
            "CALL my_proc(1, NULL)"
        );
    }

    #[test]
    fn postgres_dollar_and_question_dialects() {
        let mut v = store();
        let p = prepare(
            Dialect::Postgres,
            "SELECT :V_PROJECT_CD, :V_MP_VRSN_SEQ, :V_PROJECT_CD",
            &mut v,
            false,
        );
        assert_eq!(p.sql, "SELECT $1, $2, $1");
        assert_eq!(p.params.len(), 2);
        let p = prepare(
            Dialect::Mysql,
            "SELECT :V_PROJECT_CD, :V_MP_VRSN_SEQ, :V_PROJECT_CD",
            &mut v,
            false,
        );
        assert_eq!(p.sql, "SELECT ?, ?, ?");
        assert_eq!(p.params.len(), 3);
        assert_eq!(p.params[2].name, "V_PROJECT_CD");
    }

    #[test]
    fn exec_wrapping_per_dialect() {
        assert_eq!(
            wrap_exec(Dialect::Oracle, "sp_x(:a, 'b');"),
            "BEGIN sp_x(:a, 'b'); END;"
        );
        // 서브쿼리 대입은 SELECT … INTO … FROM DUAL(19c 실서버 PLS-00103 · 09-13).
        assert_eq!(
            wrap_exec(
                Dialect::Oracle,
                ":V_CNT := (SELECT COUNT(*) FROM user_tables)"
            ),
            "BEGIN SELECT (SELECT COUNT(*) FROM user_tables) INTO :V_CNT FROM DUAL; END;"
        );
        assert_eq!(
            wrap_exec(Dialect::Oracle, ":v := ( with x as (select 1 n from dual) select n from x )"),
            "BEGIN SELECT ( with x as (select 1 n from dual) select n from x ) INTO :V FROM DUAL; END;"
        );
        // 서브쿼리가 없는 대입·함수 호출은 PL/SQL 대입 그대로.
        assert_eq!(
            wrap_exec(Dialect::Oracle, ":V_USER := USER"),
            "BEGIN :V_USER := USER; END;"
        );
        assert_eq!(
            wrap_exec(Dialect::Oracle, ":V := f(SELECTED_COL)"),
            "BEGIN :V := f(SELECTED_COL); END;"
        );
        assert_eq!(
            wrap_exec(Dialect::Mssql, ":V := UPPER('x')"),
            "SET :V = UPPER('x')"
        );
        assert_eq!(
            wrap_exec(Dialect::Mssql, "sp_x :a, 'b'"),
            "EXEC sp_x :a, 'b'"
        );
        assert_eq!(
            wrap_exec(Dialect::Mssql, "SELECT a INTO :X FROM t"),
            "SELECT :X = a FROM t"
        );
        assert_eq!(
            wrap_exec(Dialect::Postgres, ":V := now()"),
            "SELECT now() AS \"V\""
        );
        assert_eq!(wrap_exec(Dialect::Mysql, "sp_x(1)"), "CALL sp_x(1)");
        assert_eq!(
            wrap_exec(Dialect::Sqlite, ":m := (SELECT 1)"),
            "SELECT (SELECT 1) AS \"M\""
        );
    }
}
