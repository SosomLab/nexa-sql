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
use nsql_core::{
    BindParam, Caps, Dialect, Direction, ExecForm, ExecRequest, Marker, Value, VarType,
};

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

/// `sql`의 `:NAME`을 방언에 맞춰 재작성하고 저장소 값을 붙인다(내장 방언의 능력표로 — 테스트·`nsql plan`용 얇은 껍질).
/// `inout`이 참이면(PL/SQL 블록·EXEC) 모든 바인드가 InOut — 실행 후 값이 돌아온다.
pub fn prepare(dialect: Dialect, sql: &str, vars: &mut VarStore, inout: bool) -> Prepared {
    prepare_with(&Caps::of(dialect), sql, vars, inout)
}

/// [`prepare`]의 본체 — 바인드 자리 표기는 **능력표**(`Marker`)가 정한다(T-152 · 엔진은 세션의 능력표로 부른다).
pub fn prepare_with(caps: &Caps, sql: &str, vars: &mut VarStore, inout: bool) -> Prepared {
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
            ty: bind_type(&v.ty, v.declared, inout),
            direction: dir,
        }
    };
    match caps.marker {
        Marker::Named => Prepared {
            sql: sql.to_string(),
            params: names.iter().map(|n| param_for(n)).collect(),
            mode: PrepareMode::Bind,
            implicit,
            line_offset: 0,
            captures: Vec::new(),
        },
        Marker::AtName => {
            let rewritten = rewrite_select_into_tsql(sql);
            // ★ 바인드 하나뿐인 SELECT 항목에는 **변수 표기를 열 이름으로** 붙인다(사용자 09-21): Oracle은 `:V`를 그대로 열 이름으로
            //   돌려주지만 SQL Server는 매개변수만 있는 열에 이름을 주지 않아 결과 머리줄이 비었다.
            let refs2 = extract_binds(&rewritten);
            let lone = lone_select_items(&rewritten, &refs2);
            let rewritten = replace_refs_at(&rewritten, &refs2, |k, r| {
                if lone[k] {
                    format!("@{} AS [{}]", r.name, &rewritten[r.start..r.end])
                } else {
                    format!("@{}", r.name)
                }
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
        Marker::DollarN => {
            // PostgreSQL도 매개변수만 있는 열은 `?column?`으로 온다 → 같은 규칙으로 변수 표기를 열 이름으로.
            let lone = lone_select_items(sql, &refs);
            let sql2 = replace_refs_at(sql, &refs, |k, r| {
                let idx = names.iter().position(|n| n == &r.name).map_or(0, |i| i + 1);
                if lone[k] {
                    format!("${idx} AS \"{}\"", &sql[r.start..r.end])
                } else {
                    format!("${idx}")
                }
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
        Marker::Question => {
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

/// 바인드로 보낼 타입. **선언 없이 대입으로 생긴 글자 변수**의 타입은 "지금 값의 길이"일 뿐 그릇의 크기가 아니다 — 그대로
/// 보내면 SQL Server가 `NVARCHAR(2)`로 선언해 **돌아오는 값을 말없이 자른다**(09-21 실서버: `'in'` → `'in -> 63'`이 `'in'`).
/// 선언한 변수(`VAR X VARCHAR2(10)`)의 길이는 사용자의 뜻이므로 그대로 둔다 · 값이 돌아오지 않는 바인드(읽기만)는 넓힐 까닭이 없다.
fn bind_type(ty: &VarType, declared: bool, inout: bool) -> VarType {
    match ty {
        VarType::Varchar2(n) if inout && !declared => VarType::Varchar2((*n).max(4000)),
        other => other.clone(),
    }
}

/// [`replace_refs`]와 같되 몇 번째 참조인지도 준다.
fn replace_refs_at(sql: &str, refs: &[BindRef], f: impl Fn(usize, &BindRef) -> String) -> String {
    let mut out = String::with_capacity(sql.len() + refs.len() * 8);
    let mut last = 0;
    for (k, r) in refs.iter().enumerate() {
        out.push_str(&sql[last..r.start]);
        out.push_str(&f(k, r));
        last = r.end;
    }
    out.push_str(&sql[last..]);
    out
}

/// 참조마다 "**바인드 하나뿐인 SELECT 항목**인가"(별칭도 연산도 없다) — 그런 열은 서버가 이름을 주지 않는다(SQL Server = 빈 이름 ·
/// PostgreSQL = `?column?`). 조건: 괄호 밖 · 가장 가까운 절 머리가 `SELECT` · 앞 = `SELECT`/`DISTINCT`/`ALL`/`,` ·
/// 뒤 = `,`/`;`/끝/`FROM`. 주석·글자 상수 안은 보지 않는다(`lexer::classify`). `ORDER BY :a, :b` · 함수 인자 · `IN (…)` ·
/// `VALUES (…)` · `SET a = :x`는 해당하지 않는다.
fn lone_select_items(sql: &str, refs: &[BindRef]) -> Vec<bool> {
    use crate::lexer::{classify, Class};
    if refs.is_empty() {
        return Vec::new();
    }
    let cls = classify(sql);
    let b = sql.as_bytes();
    let code = |i: usize| cls[i] == Class::Code;
    const CLAUSES: [&str; 12] = [
        "SELECT", "FROM", "WHERE", "GROUP", "ORDER", "HAVING", "SET", "VALUES", "INTO", "ON", "BY",
        "UNION",
    ];
    // ★ **한 번만 훑는다**(문장 길이에 선형): 앞에서부터 괄호 깊이 · 가장 가까운 절 머리 · 바로 앞 낱말/기호를 굴리다가 참조가
    //   시작하는 자리에서 그 상태를 읽는다. 참조마다 처음부터 다시 훑으면 바인드가 많은 긴 문장(`IN (:a, :b, …)` 수천 개)에서
    //   길이 × 참조 수가 된다(89차 성능 점검에서 고침).
    let (mut depth, mut clause_is_select, mut word) = (0i32, false, String::new());
    // 바로 앞의 의미 있는 것: 낱말이면 그 낱말 · 기호면 그 글자.
    let mut prev = String::new();
    let mut out = Vec::with_capacity(refs.len());
    let mut k = 0;
    let mut i = 0;
    while i < b.len() && k < refs.len() {
        if i == refs[k].start {
            if !word.is_empty() {
                if depth == 0 && CLAUSES.contains(&word.as_str()) {
                    clause_is_select = word == "SELECT";
                }
                prev = std::mem::take(&mut word);
            }
            let r = &refs[k];
            let before_ok = depth == 0
                && clause_is_select
                && matches!(prev.as_str(), "SELECT" | "DISTINCT" | "ALL" | ",");
            out.push(before_ok && lone_after(b, &code, r.end));
            // 바인드 자체는 "낱말도 기호도 아닌 값"이다 — 뒤따르는 것의 `prev`가 되지 않게 표시만 남긴다.
            prev = String::from("?");
            i = r.end;
            k += 1;
            continue;
        }
        if code(i) {
            let c = b[i] as char;
            if c.is_ascii_alphanumeric() || c == '_' {
                word.push(c.to_ascii_uppercase());
            } else {
                if !word.is_empty() {
                    if depth == 0 && CLAUSES.contains(&word.as_str()) {
                        clause_is_select = word == "SELECT";
                    }
                    prev = std::mem::take(&mut word);
                }
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                if !c.is_whitespace() {
                    prev.clear();
                    prev.push(c);
                }
            }
        }
        i += 1;
    }
    out.resize(refs.len(), false);
    out
}

/// 바인드 뒤: 첫 의미 있는 것이 `,` · `;` · 끝 · `FROM`이면 그 항목은 바인드 하나로 끝난다.
fn lone_after(b: &[u8], code: &dyn Fn(usize) -> bool, end: usize) -> bool {
    let mut i = end;
    while i < b.len() && (!code(i) || (b[i] as char).is_whitespace()) {
        i += 1;
    }
    if i >= b.len() {
        return true;
    }
    let c = b[i] as char;
    if c == ',' || c == ';' {
        return true;
    }
    let mut w = String::new();
    while i < b.len() && code(i) && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
        w.push((b[i] as char).to_ascii_uppercase());
        i += 1;
    }
    w == "FROM"
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
    // `FROM`이 없는 꼴(`SELECT 식 INTO :V` — T-SQL에는 DUAL이 없다 · T-162 ②): INTO 목록은 문장 끝(`;` 앞)까지이고 꼬리는 없다.
    //   종전에는 고치지 않고 그대로 보내 `SELECT 식 INTO @V`가 구문 오류(Msg 102)였다.
    let from = match find_word_ci(&sql[into..], &classify(&sql[into..]), "FROM") {
        Some(rel) => into + rel,
        None => sql.trim_end().trim_end_matches(';').trim_end().len(),
    };
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
    let tail = sql[from..].trim_start();
    if tail.is_empty() || tail.starts_with(';') {
        return format!(
            "{}SELECT {prefix}{}{tail}",
            &sql[..select_kw],
            assigns.join(", ")
        );
    }
    format!(
        "{}SELECT {prefix}{} {tail}",
        &sql[..select_kw],
        assigns.join(", ")
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

/// `EXEC 본문`을 방언별 실행 문장으로(내장 방언의 능력표로 — 테스트·`nsql plan`용 얇은 껍질).
pub fn wrap_exec(dialect: Dialect, body: &str) -> String {
    wrap_exec_with(&Caps::of(dialect), body)
}

/// `EXEC 본문`을 **능력표의 틀**(`ExecForm`)대로 실행 문장으로(T-152).
pub fn wrap_exec_with(caps: &Caps, body: &str) -> String {
    let body = body.trim().trim_end_matches(';').trim();
    match caps.exec_form {
        ExecForm::PlsqlBlock => {
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
        ExecForm::TsqlBatch => {
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
        ExecForm::Call => {
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

    /// 선언 없이 생긴 글자 변수: 값이 돌아오는 바인드(InOut)만 넓힌다 · 선언한 길이와 읽기 전용은 그대로.
    #[test]
    fn undeclared_text_binds_are_widened_only_when_values_come_back() {
        use super::bind_type;
        let t = VarType::Varchar2(2);
        assert_eq!(bind_type(&t, false, true), VarType::Varchar2(4000));
        assert_eq!(
            bind_type(&t, true, true),
            VarType::Varchar2(2),
            "선언한 길이 = 사용자의 뜻"
        );
        assert_eq!(
            bind_type(&t, false, false),
            VarType::Varchar2(2),
            "읽기만 하는 바인드"
        );
        assert_eq!(
            bind_type(&VarType::Varchar2(9000), false, true),
            VarType::Varchar2(9000)
        );
        assert_eq!(bind_type(&VarType::Number, false, true), VarType::Number);
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
        // 바인드 하나뿐인 SELECT 항목 = 변수 표기가 열 이름(서버는 `?column?`을 준다 · 09-21).
        assert_eq!(
            p.sql,
            "SELECT $1 AS \":V_PROJECT_CD\", $2 AS \":V_MP_VRSN_SEQ\", $1 AS \":V_PROJECT_CD\""
        );
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

    /// 바인드 하나뿐인 SELECT 항목에만 열 이름을 붙인다(사용자 09-21 — SQL Server 결과 머리줄이 비었다). 연산·함수 인자·
    /// `IN (…)`·`WHERE`·`ORDER BY`·별칭이 이미 있는 항목·주석/글자 상수 안은 건드리지 않는다.
    #[test]
    fn lone_bind_select_items_get_the_variable_as_column_name() {
        let t = |sql: &str| {
            let mut v = store();
            prepare(Dialect::Mssql, sql, &mut v, false).sql
        };
        assert_eq!(
            t("SELECT\n\t:V_A\n,\t:V_B\n;"),
            "SELECT\n\t@V_A AS [:V_A]\n,\t@V_B AS [:V_B]\n;"
        );
        assert_eq!(t("SELECT :v_a FROM t"), "SELECT @V_A AS [:v_a] FROM t");
        assert_eq!(
            t("SELECT DISTINCT :V_A, c FROM t"),
            "SELECT DISTINCT @V_A AS [:V_A], c FROM t"
        );
        // 해당 없음.
        assert_eq!(
            t("SELECT :V_A + 1, :V_B AS b"),
            "SELECT @V_A + 1, @V_B AS b"
        );
        assert_eq!(t("SELECT f(:V_A, :V_B), c"), "SELECT f(@V_A, @V_B), c");
        assert_eq!(
            t("SELECT c FROM t WHERE a = :V_A AND b IN (:V_A, :V_B)"),
            "SELECT c FROM t WHERE a = @V_A AND b IN (@V_A, @V_B)"
        );
        assert_eq!(
            t("SELECT c FROM t ORDER BY :V_A, :V_B"),
            "SELECT c FROM t ORDER BY @V_A, @V_B"
        );
        assert_eq!(
            t("SELECT (SELECT :V_A), ':V_B' -- , :V_B\n"),
            "SELECT (SELECT @V_A), ':V_B' -- , :V_B\n"
        );
        assert_eq!(
            t("UPDATE t SET a = :V_A, b = :V_B"),
            "UPDATE t SET a = @V_A, b = @V_B"
        );
    }

    /// `FROM` 없는 `SELECT 식 INTO :V`(T-162 ②) — T-SQL 대입 꼴로 고친다 · `FROM`이 있는 꼴은 종전 그대로.
    #[test]
    fn tsql_select_into_without_from() {
        assert_eq!(
            rewrite_select_into_tsql("SELECT DB_NAME(), 1 + 2 INTO :v_db, :v_n;"),
            "SELECT :V_DB = DB_NAME(), :V_N = 1 + 2;"
        );
        assert_eq!(
            rewrite_select_into_tsql("SELECT GETDATE() INTO :v_now"),
            "SELECT :V_NOW = GETDATE()"
        );
        assert_eq!(
            rewrite_select_into_tsql("SELECT name INTO :v FROM sys.objects WHERE object_id = 1"),
            "SELECT :V = name FROM sys.objects WHERE object_id = 1"
        );
        // 대상이 바인드가 아니면(진짜 `SELECT … INTO 새_테이블`) 건드리지 않는다.
        assert_eq!(
            rewrite_select_into_tsql("SELECT 1 AS a INTO #t"),
            "SELECT 1 AS a INTO #t"
        );
    }

    /// 바인드가 수천 개인 긴 문장도 준비 시간이 길이에 선형이다(열 이름 판정이 참조마다 처음부터 훑던 것을 한 번 훑기로).
    #[test]
    fn many_binds_prepare_in_linear_time() {
        let n = 5000;
        let mut sql = String::from("SELECT :B0, c FROM t WHERE c IN (");
        for k in 1..n {
            sql.push_str(&format!(":B{k}, "));
        }
        sql.push_str(":B0)");
        let mut v = VarStore::new();
        let t0 = std::time::Instant::now();
        let p = prepare(Dialect::Mssql, &sql, &mut v, false);
        assert!(p
            .sql
            .starts_with("SELECT @B0 AS [:B0], c FROM t WHERE c IN (@B1, "));
        assert!(
            !p.sql[30..].contains(" AS ["),
            "IN 목록의 바인드에는 열 이름을 붙이지 않는다"
        );
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(2),
            "{:?}",
            t0.elapsed()
        );
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
