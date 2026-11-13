//! ★ 데이터 보호 하네스(docs/110 §17 · 사용자 11-12 "요청과 실행 사이의 제어"): AI가 요청한 도구 호출을 **분류 → 문맥 → 정책**으로
//! 판정하는 순수 계층. 호스트(`App::ai_tool_request`)는 사실(운영 여부 · Auto · 설정)만 넘기고 결과([`Decision`])대로 게이트를 세운다.
//!
//! 계층: ① [`classify_sql`]/[`classify_tool`] = 무엇을 하려는가([`Risk`]) → ② [`Ctx`] = 어디서(운영) · 어떤 편의 설정으로 →
//! ③ [`decide`] = 몇 단계 검증이 필요한가([`Decision`]) → ④ 게이트 1(승인 카드) · 게이트 2(영향 확인 카드 · [`impact_count_sql`]) →
//! ⑤ 실행(사람 경로와 같은 `run_text_in` · 보류 커밋·DDL 가드·백업 그대로) → ⑥ 커밋(운영 = 사람만).
//! 원칙: 운영 세션은 보호가 먼저다 — 자동 승인 설정은 운영에 닿지 않고, 대량·파괴적 변경은 늘 2단계, 커밋·보호 설정 변경은 AI 불가.

use nsql_core::{Dialect, TxClass, TxControl};

/// 요청의 위험 등급(낮은 것 → 높은 것 · 여러 문장이면 가장 높은 것).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Risk {
    /// 조회(데이터를 바꾸지 않는다).
    Read,
    /// 앱 안의 쓰기(편집기 글 바꾸기 · 일반 설정 · 접속 · 확장 켜기) — DB 데이터와 무관.
    AppWrite,
    /// 일반 변경: INSERT · 조건(WHERE)이 있는 UPDATE/DELETE · CREATE(새 객체).
    Write,
    /// 대량 변경: WHERE 없는 UPDATE/DELETE · MERGE · 블록/프로시저 호출(범위를 알 수 없음) · 영향 행 수가 기준 이상.
    Bulk,
    /// 파괴적 변경: DROP · TRUNCATE · ALTER · RENAME · CREATE OR REPLACE · GRANT/REVOKE.
    Destructive,
    /// 트랜잭션 끝(COMMIT/ROLLBACK) — 운영에서는 사람만.
    TxEnd,
    /// 보호 장치 자체를 바꾸는 요청(운영 확인 · 보류 커밋 · 백업 · 자동 승인 설정 · 실행 메뉴 명령).
    Protection,
}

/// 판정 문맥(호스트가 주는 사실).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ctx {
    /// 운영 접속인가(`ConnEnv::Prod`).
    pub prod: bool,
    /// 세션 Auto 토글(조회·앱 쓰기 자동 승인).
    pub auto: bool,
    /// 설정 `ai.guard_auto_write` = dml(비운영의 일반 변경 자동 승인).
    pub auto_write: bool,
}

/// 필요한 검증.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    /// 바로 실행.
    Allow,
    /// 승인 카드 한 번.
    Card,
    /// 승인 카드 + 영향 확인 카드(2단계).
    TwoStage,
    /// 거부(사람이 직접 해야 한다).
    Deny,
}

/// ★ 정책 표(순수 · MC/DC 시험) — 운영은 보호 우선: 자동 승인이 닿지 않고, 대량·파괴적 변경은 늘 2단계, 트랜잭션 끝·보호 변경은 거부.
pub(crate) fn decide(risk: Risk, ctx: Ctx) -> Decision {
    match risk {
        Risk::Read => Decision::Allow,
        Risk::AppWrite => {
            if ctx.auto && !ctx.prod {
                Decision::Allow
            } else {
                Decision::Card
            }
        }
        Risk::Write => {
            if ctx.auto_write && !ctx.prod {
                Decision::Allow
            } else {
                Decision::Card
            }
        }
        Risk::Bulk | Risk::Destructive => Decision::TwoStage,
        Risk::TxEnd | Risk::Protection => {
            if ctx.prod {
                Decision::Deny
            } else {
                Decision::Card
            }
        }
    }
}

/// 문장 하나의 등급.
fn classify_stmt(stmt: &str, dialect: Option<Dialect>) -> Risk {
    let body = stmt.trim();
    if body.is_empty() {
        return Risk::Read;
    }
    if TxControl::of_sql(body) == TxControl::End {
        return Risk::TxEnd;
    }
    let up = head_words(body, 3);
    match TxClass::of_sql_in(body, dialect) {
        TxClass::None | TxClass::Read => {
            // SELECT … INTO(테이블 생성) · FOR UPDATE(잠금)는 조회가 아니다.
            let low = body.to_ascii_lowercase();
            if low.contains(" into ") || low.contains("for update") {
                Risk::Write
            } else {
                Risk::Read
            }
        }
        TxClass::Insert => Risk::Write,
        TxClass::Update | TxClass::Delete => {
            if up.first().is_some_and(|w| w == "MERGE" || w == "UPSERT")
                || nsql_run::dml_without_where(body)
            {
                Risk::Bulk
            } else {
                Risk::Write
            }
        }
        TxClass::Truncate | TxClass::DdlDrop => Risk::Destructive,
        TxClass::DdlCreate => {
            let first = up.first().map(String::as_str).unwrap_or("");
            let replace = up.get(1).is_some_and(|w| w == "OR");
            if (first == "CREATE" && !replace) || first == "COMMENT" {
                Risk::Write
            } else {
                Risk::Destructive
            }
        }
        // 블록 · EXEC/CALL · 그 밖 = 범위를 알 수 없다 → 보수적으로 대량.
        TxClass::Other => Risk::Bulk,
    }
}

/// 앞쪽 낱말 n개(대문자 · 주석·괄호 건너뜀).
fn head_words(sql: &str, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = sql.trim_start();
    loop {
        if let Some(r) = rest.strip_prefix("--") {
            rest = r.split_once('\n').map(|x| x.1).unwrap_or("").trim_start();
            continue;
        }
        if let Some(r) = rest.strip_prefix("/*") {
            rest = r.split_once("*/").map(|x| x.1).unwrap_or("").trim_start();
            continue;
        }
        break;
    }
    for w in rest
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .take(n)
    {
        out.push(w.to_ascii_uppercase());
    }
    out
}

/// SQL 본문의 등급(여러 문장이면 가장 높은 것 · 클라이언트 전용 명령은 건너뜀).
pub(crate) fn classify_sql(sql: &str, dialect: Option<Dialect>) -> Risk {
    let mut worst = Risk::Read;
    for item in nsql_script::split_script_in(sql, dialect) {
        let r = match item.kind {
            nsql_script::ItemKind::Sql(_) => classify_stmt(&item.text, dialect),
            // 스크립트 명령(CONNECT · SET · @file …) = 범위를 알 수 없다.
            nsql_script::ItemKind::Command(_) => Risk::Bulk,
            nsql_script::ItemKind::Invalid(_) => Risk::Bulk,
        };
        worst = worst.max(r);
    }
    worst
}

/// 보호 장치에 속한 설정 키인가(AI가 바꾸면 게이트가 무력해진다).
pub(crate) fn is_protection_setting(key: &str) -> bool {
    let k = key.trim().to_ascii_lowercase();
    k.starts_with("run.prod_")
        || k.starts_with("tx.")
        || k.starts_with("backup.")
        || k.starts_with("ai.guard_")
        || matches!(
            k.as_str(),
            "ai.auto" | "ai.codex_auto" | "ai.permission_mode" | "ai.enabled" | "ai.max_rows"
        )
}

/// 실행·트랜잭션·접속을 건드리는 메뉴 명령인가(AI가 `run_command`로 부르면 SQL이 카드에 안 보인다).
pub(crate) fn is_protection_command(id: &str) -> bool {
    let k = id.trim().to_ascii_lowercase();
    // ★ 접속 해제는 예외(사용자 11-14 "다른 데서 사용 중인 게 아니라면 종료해도 괜찮다"): 데이터를 바꾸지 않는다 — 승인 카드 등급
    //   (`AppWrite`)으로 두고, 실행 중·미커밋 변경이 있는 접속은 호스트가 따로 거절한다(`ai_tool_request`).
    if is_disconnect_command(&k) {
        return false;
    }
    k.starts_with("run.")
        || k.starts_with("tx.")
        || k.starts_with("hold.")
        || k.starts_with("conn.")
        || k.starts_with("sess.")
        || k.starts_with("grid.apply")
        || k.starts_with("import.")
}

/// 지금 탭의 접속을 끊는 메뉴 명령인가.
pub(crate) fn is_disconnect_command(id: &str) -> bool {
    id.trim().eq_ignore_ascii_case("conn.disconnect")
}

/// 도구 호출의 등급. `write` = 도구 표의 쓰기 여부 · `arg` = 핵심 인자(run_query = SQL · set_setting = 키 · run_command = id).
pub(crate) fn classify_tool(name: &str, write: bool, arg: &str, dialect: Option<Dialect>) -> Risk {
    match name {
        "run_query" => classify_sql(arg, dialect),
        // explain은 실행 계획만(실행하지 않음 · analyze 고정 끔).
        "explain" => Risk::Read,
        // ★ 변수 설정(값·수식 · 사용자 11-12): 진입점이 분명하고 트랜잭션이 생기지 않는다 → 조회 수준(운영에서도 카드 없음).
        "set_var" => Risk::Read,
        "set_setting" => {
            if is_protection_setting(arg) {
                Risk::Protection
            } else {
                Risk::AppWrite
            }
        }
        "run_command" => {
            if is_protection_command(arg) {
                Risk::Protection
            } else {
                Risk::AppWrite
            }
        }
        _ if write => Risk::AppWrite,
        _ => Risk::Read,
    }
}

/// ★ 영향 행 수를 세는 조회문(게이트 2 · 실행 전): 단일 테이블 `DELETE FROM t [a] [WHERE …]` · `UPDATE t [a] SET … [WHERE …]` ·
/// `TRUNCATE [TABLE] t` · `DROP TABLE t`만 — 그 밖(여러 문장 · JOIN/FROM 딸린 UPDATE · MERGE · 블록)은 `None` = "알 수 없음".
/// 만든 문장은 원문의 테이블 표기·WHERE 본문을 **그대로** 옮긴 `SELECT COUNT(*) FROM …`이다(조회 전용).
pub(crate) fn impact_count_sql(sql: &str, dialect: Option<Dialect>) -> Option<String> {
    let items: Vec<_> = nsql_script::split_script_in(sql, dialect)
        .into_iter()
        .filter(|i| !i.text.trim().is_empty())
        .collect();
    if items.len() != 1 {
        return None;
    }
    let body = items[0].text.trim().trim_end_matches(';').trim();
    let toks = top_tokens(body)?;
    let up: Vec<String> = toks.iter().map(|t| t.0.to_ascii_uppercase()).collect();
    let word = |i: usize| up.get(i).map(String::as_str).unwrap_or("");
    // 최상위 WHERE 위치(원문 바이트).
    let where_at = toks
        .iter()
        .zip(&up)
        .find(|(_, u)| *u == "WHERE")
        .map(|(t, _)| t.1);
    let tail = |from: usize| body[from..].trim();
    match word(0) {
        "DELETE" => {
            let start = if word(1) == "FROM" { 2 } else { 1 };
            let (tbl, next) = table_ref(&toks, &up, start)?;
            // 테이블 뒤에 WHERE 말고 다른 절(USING · JOIN · FROM · RETURNING …)이 오면 포기.
            let after = word(next);
            if !(after.is_empty() || after == "WHERE") {
                return None;
            }
            Some(match where_at {
                Some(w) => format!("SELECT COUNT(*) FROM {tbl} {}", tail(w)),
                None => format!("SELECT COUNT(*) FROM {tbl}"),
            })
        }
        "UPDATE" => {
            let (tbl, next) = table_ref(&toks, &up, 1)?;
            if word(next) != "SET" {
                return None;
            }
            // SET 뒤 최상위에 FROM/JOIN이 있으면(다중 테이블 UPDATE) 포기.
            if up[next..].iter().any(|u| u == "FROM" || u == "JOIN") {
                return None;
            }
            Some(match where_at {
                Some(w) => format!("SELECT COUNT(*) FROM {tbl} {}", tail(w)),
                None => format!("SELECT COUNT(*) FROM {tbl}"),
            })
        }
        "TRUNCATE" => {
            let start = if word(1) == "TABLE" { 2 } else { 1 };
            let (tbl, next) = table_ref(&toks, &up, start)?;
            let _ = next;
            Some(format!("SELECT COUNT(*) FROM {}", tbl.split(' ').next()?))
        }
        "DROP" if word(1) == "TABLE" => {
            let mut start = 2;
            if word(2) == "IF" && word(3) == "EXISTS" {
                start = 4;
            }
            let (tbl, _) = table_ref(&toks, &up, start)?;
            Some(format!("SELECT COUNT(*) FROM {}", tbl.split(' ').next()?))
        }
        _ => None,
    }
}

/// 최상위(괄호 깊이 0) 토큰과 원문 바이트 위치 — 문자열·인용 식별자·주석은 건너뛴다. 괄호가 안 맞으면 `None`.
fn top_tokens(s: &str) -> Option<Vec<(String, usize)>> {
    let b: Vec<(usize, char)> = s.char_indices().collect();
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut i = 0;
    let mut depth = 0i32;
    while i < b.len() {
        let (pos, c) = b[i];
        if c == '-' && b.get(i + 1).is_some_and(|x| x.1 == '-') {
            while i < b.len() && b[i].1 != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && b.get(i + 1).is_some_and(|x| x.1 == '*') {
            i += 2;
            while i + 1 < b.len() && !(b[i].1 == '*' && b[i + 1].1 == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == '\'' {
            i += 1;
            while i < b.len() {
                if b[i].1 == '\'' {
                    if b.get(i + 1).is_some_and(|x| x.1 == '\'') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if c == '(' {
            depth += 1;
            i += 1;
            continue;
        }
        if c == ')' {
            depth -= 1;
            if depth < 0 {
                return None;
            }
            i += 1;
            continue;
        }
        if c == '"' || c == '`' || c == '[' {
            let close = if c == '[' { ']' } else { c };
            let start = i;
            i += 1;
            while i < b.len() && b[i].1 != close {
                i += 1;
            }
            i += 1;
            if depth == 0 {
                let end = b.get(i).map(|x| x.0).unwrap_or(s.len());
                push_tok(&mut out, &s[b[start].0..end], pos);
            }
            continue;
        }
        if c.is_alphanumeric() || c == '_' || c == '$' || c == '#' || c == '@' {
            let start = i;
            while i < b.len()
                && (b[i].1.is_alphanumeric() || matches!(b[i].1, '_' | '$' | '#' | '@'))
            {
                i += 1;
            }
            if depth == 0 {
                let end = b.get(i).map(|x| x.0).unwrap_or(s.len());
                push_tok(&mut out, &s[b[start].0..end], pos);
            }
            continue;
        }
        if depth == 0 && (c == '.' || c == ',' || c == '=' || c == ';') {
            out.push((c.to_string(), pos));
        }
        i += 1;
    }
    (depth == 0).then_some(out)
}

fn push_tok(out: &mut Vec<(String, usize)>, text: &str, pos: usize) {
    out.push((text.to_string(), pos));
}

/// `start`부터 테이블 참조 `[s.]t [별칭]`을 읽는다 → (원문 표기 "s.t alias", 다음 토큰 번호).
fn table_ref(toks: &[(String, usize)], up: &[String], start: usize) -> Option<(String, usize)> {
    let mut i = start;
    let mut name = String::new();
    loop {
        let t = toks.get(i)?;
        if !is_ident(&t.0) || is_clause_word(&up[i]) {
            return None;
        }
        name.push_str(&t.0);
        i += 1;
        if toks.get(i).is_some_and(|t| t.0 == ".") {
            name.push('.');
            i += 1;
            continue;
        }
        break;
    }
    // 별칭(AS 포함).
    if up.get(i).is_some_and(|u| u == "AS") {
        i += 1;
    }
    if let Some(t) = toks.get(i) {
        if is_ident(&t.0) && !is_clause_word(&up[i]) {
            name.push(' ');
            name.push_str(&t.0);
            i += 1;
        }
    }
    Some((name, i))
}

fn is_ident(t: &str) -> bool {
    t.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '"' | '`' | '[' | '#'))
}

fn is_clause_word(up: &str) -> bool {
    matches!(
        up,
        "WHERE"
            | "SET"
            | "FROM"
            | "USING"
            | "JOIN"
            | "INNER"
            | "LEFT"
            | "RIGHT"
            | "RETURNING"
            | "OUTPUT"
            | "CASCADE"
            | "PURGE"
            | "ORDER"
            | "LIMIT"
    )
}

/// 승인 카드용 요약(순수): 문장 종류(첫 낱말들 · 문장 수)와 대상 객체(FROM/JOIN/INTO/UPDATE/TABLE 뒤의 이름 · 최대 6).
pub(crate) fn summarize(sql: &str, dialect: Option<Dialect>) -> (String, Vec<String>) {
    let items: Vec<_> = nsql_script::split_script_in(sql, dialect)
        .into_iter()
        .filter(|i| !i.text.trim().is_empty())
        .collect();
    let mut kinds: Vec<String> = Vec::new();
    let mut targets: Vec<String> = Vec::new();
    for it in &items {
        if let Some(k) = head_words(&it.text, 1).into_iter().next() {
            if !kinds.contains(&k) {
                kinds.push(k);
            }
        }
        let Some(toks) = top_tokens(it.text.trim().trim_end_matches(';')) else {
            continue;
        };
        let up: Vec<String> = toks.iter().map(|t| t.0.to_ascii_uppercase()).collect();
        for i in 0..toks.len() {
            let lead = matches!(
                up[i].as_str(),
                "FROM" | "JOIN" | "INTO" | "UPDATE" | "TABLE" | "VIEW" | "TRUNCATE"
            );
            if !lead {
                continue;
            }
            let mut j = i + 1;
            while up
                .get(j)
                .is_some_and(|u| matches!(u.as_str(), "TABLE" | "IF" | "EXISTS" | "ONLY"))
            {
                j += 1;
            }
            let mut name = String::new();
            while let Some(t) = toks.get(j) {
                if !is_ident(&t.0) || (name.is_empty() && is_clause_word(&up[j])) {
                    break;
                }
                name.push_str(&t.0);
                j += 1;
                if toks.get(j).is_some_and(|t| t.0 == ".") {
                    name.push('.');
                    j += 1;
                } else {
                    break;
                }
            }
            let key = name.to_ascii_uppercase();
            if !name.is_empty()
                && !matches!(key.as_str(), "SELECT" | "DUAL" | "SET" | "VALUES")
                && !targets.iter().any(|t| t.eq_ignore_ascii_case(&name))
                && targets.len() < 6
            {
                targets.push(name);
            }
        }
    }
    let kind = if items.len() > 1 {
        format!("{} ×{}", kinds.join("+"), items.len())
    } else {
        kinds.join("+")
    };
    (kind, targets)
}

// ───────────────────────── 카탈로그(메타) 조회 판정 ─────────────────────────

#[derive(Debug, PartialEq)]
enum Tok {
    Word(String),
    /// 인용 식별자("…" · `…` · […]) — 카탈로그 이름으로 치지 않는다(보수적).
    Quoted,
    Sym(char),
}

/// 문자열·주석을 뺀 토큰(낱말은 대문자 · `$`·`#`·`_` 포함).
fn cat_tokens(sql: &str) -> Vec<Tok> {
    let ch: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < ch.len() {
        let c = ch[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '\'' {
            i += 1;
            while i < ch.len() {
                if ch[i] == '\'' {
                    if ch.get(i + 1) == Some(&'\'') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            i += 1;
            out.push(Tok::Sym('\''));
        } else if c == '"' || c == '`' || c == '[' {
            let close = if c == '[' { ']' } else { c };
            i += 1;
            while i < ch.len() && ch[i] != close {
                i += 1;
            }
            i += 1;
            out.push(Tok::Quoted);
        } else if c == '-' && ch.get(i + 1) == Some(&'-') {
            while i < ch.len() && ch[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && ch.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < ch.len() && !(ch[i] == '*' && ch[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else if c.is_alphanumeric() || c == '_' {
            let s = i;
            while i < ch.len() && (ch[i].is_alphanumeric() || matches!(ch[i], '_' | '$' | '#')) {
                i += 1;
            }
            out.push(Tok::Word(
                ch[s..i].iter().collect::<String>().to_ascii_uppercase(),
            ));
        } else {
            out.push(Tok::Sym(c));
            i += 1;
        }
    }
    out
}

/// 이름(점으로 나뉜 조각 · 대문자)이 **데이터 사전(카탈로그) 객체**인가 — 방언별 규칙. 한정 없는 접두 규칙(`ALL_`·`USER_`·`PG_`)은
/// 그 방언일 때만(다른 DBMS에서는 사용자 표 이름일 수 있다).
fn is_catalog_name(parts: &[String], dialect: Option<Dialect>) -> bool {
    let Some(name) = parts.last().map(String::as_str) else {
        return false;
    };
    let schema = (parts.len() >= 2).then(|| parts[parts.len() - 2].as_str());
    if parts.len() > 3 {
        return false;
    }
    if schema.is_none() && name == "DUAL" {
        return true;
    }
    let ora_dict = |n: &str| {
        ["ALL_", "USER_", "DBA_", "CDB_", "V$", "GV$", "NLS_"]
            .iter()
            .any(|p| n.starts_with(p))
            || matches!(
                n,
                "DUAL"
                    | "DICT"
                    | "DICTIONARY"
                    | "TAB"
                    | "TABS"
                    | "CAT"
                    | "COLS"
                    | "OBJ"
                    | "IND"
                    | "SEQ"
                    | "SYN"
            )
    };
    match schema {
        Some("INFORMATION_SCHEMA" | "PG_CATALOG" | "SYSCAT" | "SYSIBM" | "SYSSTAT") => true,
        // SQL Server의 `sys.*` = 카탈로그 뷰 · Oracle의 `SYS.`은 사전 뷰 이름일 때만(`SYS.USER$` 같은 기반 표는 아니다).
        Some("SYS") => match dialect {
            Some(Dialect::Mssql) => true,
            Some(Dialect::Oracle) => ora_dict(name),
            _ => false,
        },
        Some("PUBLIC") => dialect == Some(Dialect::Oracle) && ora_dict(name),
        Some(_) => false,
        None => match dialect {
            Some(Dialect::Oracle) => ora_dict(name),
            Some(Dialect::Postgres) => name.starts_with("PG_"),
            Some(Dialect::Sqlite) => {
                matches!(
                    name,
                    "SQLITE_MASTER" | "SQLITE_SCHEMA" | "SQLITE_TEMP_MASTER"
                )
            }
            _ => false,
        },
    }
}

/// `FROM` 다음 토큰부터 그 절이 끝날 때까지 — 표 참조가 **전부 카탈로그 이름**인가(파생 표·표 함수·DB 링크·인용 이름은 거짓).
fn from_refs_are_catalog(t: &[Tok], mut i: usize, dialect: Option<Dialect>) -> bool {
    const END: &[&str] = &[
        "WHERE",
        "GROUP",
        "ORDER",
        "HAVING",
        "UNION",
        "INTERSECT",
        "MINUS",
        "EXCEPT",
        "CONNECT",
        "START",
        "FETCH",
        "LIMIT",
        "OFFSET",
        "FOR",
        "WINDOW",
        "QUALIFY",
    ];
    const JOINW: &[&str] = &[
        "INNER", "LEFT", "RIGHT", "FULL", "OUTER", "CROSS", "NATURAL",
    ];
    let mut expect_ref = true;
    let mut in_cond = false;
    let mut depth = 0i32;
    while i < t.len() {
        if expect_ref {
            // 이름(점으로 이은 낱말들)
            let mut parts: Vec<String> = Vec::new();
            loop {
                match t.get(i) {
                    Some(Tok::Word(w)) => parts.push(w.clone()),
                    _ => return false,
                }
                i += 1;
                if t.get(i) == Some(&Tok::Sym('.')) {
                    i += 1;
                } else {
                    break;
                }
            }
            // 표 함수 `f(` · DB 링크 `@` = 판정 불가.
            if matches!(t.get(i), Some(Tok::Sym('(' | '@'))) || !is_catalog_name(&parts, dialect) {
                return false;
            }
            // 별칭(`AS x` · `x`) 건너뛰기.
            if t.get(i) == Some(&Tok::Word("AS".into())) {
                i += 1;
            }
            if let Some(Tok::Word(w)) = t.get(i) {
                let kw = END.contains(&w.as_str())
                    || JOINW.contains(&w.as_str())
                    || matches!(w.as_str(), "JOIN" | "ON" | "USING");
                if !kw {
                    i += 1;
                }
            }
            expect_ref = false;
            in_cond = false;
            continue;
        }
        match &t[i] {
            Tok::Sym('(') => depth += 1,
            Tok::Sym(')') => {
                depth -= 1;
                if depth < 0 {
                    return true; // 이 FROM을 감싼 괄호가 닫혔다 = 절 끝.
                }
            }
            _ if depth > 0 => {}
            Tok::Sym(',') => expect_ref = true,
            Tok::Sym(';') => return true,
            Tok::Word(w) if END.contains(&w.as_str()) => return true,
            Tok::Word(w) if w == "JOIN" => expect_ref = true,
            Tok::Word(w) if JOINW.contains(&w.as_str()) => {}
            Tok::Word(w) if matches!(w.as_str(), "ON" | "USING") => in_cond = true,
            _ if in_cond => {}
            // 참조 뒤에 뜻 모를 토큰(PIVOT · SAMPLE · 파티션 절 …) = 판정 불가.
            _ => return false,
        }
        i += 1;
    }
    !expect_ref
}

/// ★ **행을 돌려주지 않는 컬럼 확인 조회**인가(사용자 11-14 "컬럼 정보를 읽는 경우 확인 없이"): `SELECT … FROM t WHERE 1=0`처럼
/// 맨 바깥 조건이 늘 거짓이라 **열 머리만** 돌아오는 조회 — 어시스턴트가 테이블의 컬럼을 알아내려고 보낸다(행 데이터 없음).
/// 판정(보수적 · 토큰 기준 = 주석·문자열에 속지 않는다): 문장 하나 · `SELECT`로 시작 · 집합 연산(UNION 등) 없음 · 사용자 루틴 호출
/// 없음 · 문장의 **마지막**이 괄호 밖의 `WHERE 1=0`(또는 `1=2` · `0=1`)일 때만. 그 밖(조건이 더 붙음 · `OR` · 다른 절)은 거짓.
pub(crate) fn is_empty_probe(sql: &str) -> bool {
    let body = sql.trim().trim_end_matches(';').trim();
    if body.is_empty() || body.contains(';') || !nsql_script::bind::expr_is_plain(body) {
        return false;
    }
    let t = cat_tokens(body);
    if !matches!(t.first(), Some(Tok::Word(w)) if w == "SELECT") {
        return false;
    }
    if t.iter().any(|x| {
        matches!(x, Tok::Word(w) if matches!(w.as_str(), "UNION" | "INTERSECT" | "MINUS" | "EXCEPT"))
    }) {
        return false;
    }
    let n = t.len();
    if n < 5 {
        return false;
    }
    let never = matches!(
        (&t[n - 3], &t[n - 2], &t[n - 1]),
        (Tok::Word(a), Tok::Sym('='), Tok::Word(b))
            if matches!((a.as_str(), b.as_str()), ("1", "0") | ("1", "2") | ("0", "1"))
    );
    if !never || !matches!(&t[n - 4], Tok::Word(w) if w == "WHERE") {
        return false;
    }
    // 그 WHERE가 괄호 밖(맨 바깥 조회의 조건)이어야 한다.
    let depth = t[..n - 4].iter().fold(0i32, |d, x| match x {
        Tok::Sym('(') => d + 1,
        Tok::Sym(')') => d - 1,
        _ => d,
    });
    depth == 0
}

/// ★ **카탈로그(메타) 조회**인가(사용자 11-14 "테이블·컬럼 정보를 확인하기 위한 명령은 Allow 질문 없이"): 문장 하나 · 조회문 ·
/// 모든 `FROM`/`JOIN`의 표가 데이터 사전 객체([`is_catalog_name`]) · 사용자 루틴 호출 없음([`nsql_script::bind::expr_is_plain`] —
/// 함수가 표 데이터를 돌려줄 수 있다) · 파생 표·표 함수·DB 링크·인용 이름·CTE 없음. 하나라도 판정하지 못하면 거짓(= 종전대로
/// 행 데이터 동의를 묻는다). 순수 · 조회 여부(읽기 전용)는 호출자가 따로 본다.
pub(crate) fn is_catalog_query(sql: &str, dialect: Option<Dialect>) -> bool {
    let body = sql.trim().trim_end_matches(';').trim();
    if body.is_empty() || body.contains(';') || !nsql_script::bind::expr_is_plain(body) {
        return false;
    }
    let t = cat_tokens(body);
    if !matches!(t.first(), Some(Tok::Word(w)) if w == "SELECT") {
        return false;
    }
    let mut seen = false;
    for i in 0..t.len() {
        if matches!(&t[i], Tok::Word(w) if w == "FROM") {
            if !from_refs_are_catalog(&t, i + 1, dialect) {
                return false;
            }
            seen = true;
        }
    }
    seen
}

/// `EXEC` 본문이 **바인드 변수 대입**(`:이름 := 식`)인가 — 리터럴이든 식이든(식은 DB가 계산만 한다 · 트랜잭션 없음).
/// 프로시저 호출(`pkg.proc(...)` · `:=` 없음) · 여러 문장(`;`) · 대입 대상이 바인드가 아닌 것은 아니다. 순수 판정.
pub(crate) fn is_bind_assign(body: &str) -> bool {
    let body = body.trim().trim_end_matches(';').trim();
    if body.contains(';') {
        return false;
    }
    let Some((lhs, rhs)) = body.split_once(":=") else {
        return false;
    };
    let lhs = lhs.trim();
    lhs.starts_with(':')
        && lhs.len() > 1
        && lhs[1..]
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '#'))
        && !rhs.trim().is_empty()
        // ★ 식 안의 사용자 루틴 호출은 DML을 할 수 있다 → "계산만"이 아니다(11-13 · `nsql_script::bind::expr_is_plain`).
        && nsql_script::bind::expr_is_plain(rhs)
}

/// 사전 조회 결과 글(열 머리 줄 + 값 줄)에서 행 수를 꺼낸다.
pub(crate) fn parse_count(text: &str) -> Option<u64> {
    text.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .and_then(|l| {
            let digits: String = l.chars().filter(char::is_ascii_digit).collect();
            let clean = l
                .chars()
                .all(|c| c.is_ascii_digit() || c == ',' || c == ' ');
            if clean && !digits.is_empty() {
                digits.parse().ok()
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn ctx(prod: bool, auto: bool, auto_write: bool) -> Ctx {
        Ctx {
            prod,
            auto,
            auto_write,
        }
    }

    #[test]
    fn classify_sql_grades_statements() {
        let c = |s: &str| classify_sql(s, None);
        assert_eq!(c("select * from t"), Risk::Read);
        assert_eq!(c("with x as (select 1) select * from x"), Risk::Read);
        assert_eq!(c("select * from t for update"), Risk::Write);
        assert_eq!(c("insert into t values (1)"), Risk::Write);
        assert_eq!(c("update t set a = 1 where id = 3"), Risk::Write);
        assert_eq!(c("delete from t where id = 3"), Risk::Write);
        assert_eq!(c("update t set a = 1"), Risk::Bulk);
        assert_eq!(c("delete from t"), Risk::Bulk);
        assert_eq!(
            c("merge into t using s on (t.k = s.k) when matched then update set a = 1"),
            Risk::Bulk
        );
        assert_eq!(c("begin p; end;"), Risk::Bulk);
        assert_eq!(c("create table t (a int)"), Risk::Write);
        assert_eq!(c("create or replace view v as select 1"), Risk::Destructive);
        assert_eq!(c("alter table t drop column a"), Risk::Destructive);
        assert_eq!(c("drop table t"), Risk::Destructive);
        assert_eq!(c("truncate table t"), Risk::Destructive);
        assert_eq!(c("grant select on t to u"), Risk::Destructive);
        assert_eq!(c("commit"), Risk::TxEnd);
        assert_eq!(c("rollback"), Risk::TxEnd);
        // 여러 문장 = 가장 높은 것.
        assert_eq!(c("select 1; drop table t"), Risk::Destructive);
        assert_eq!(c("update t set a = 1 where id = 1; commit"), Risk::TxEnd);
    }

    #[test]
    fn classify_tool_marks_protection_requests() {
        let c = |n: &str, w: bool, a: &str| classify_tool(n, w, a, None);
        assert_eq!(c("set_setting", true, "editor.word_wrap"), Risk::AppWrite);
        assert_eq!(c("set_setting", true, "run.prod_confirm"), Risk::Protection);
        assert_eq!(c("set_setting", true, "tx.prod_hold"), Risk::Protection);
        assert_eq!(c("set_setting", true, "ai.auto"), Risk::Protection);
        assert_eq!(
            c("set_setting", true, "ai.guard_auto_write"),
            Risk::Protection
        );
        assert_eq!(
            c("set_setting", true, "backup.object_drop"),
            Risk::Protection
        );
        assert_eq!(c("run_command", true, "run.commit"), Risk::Protection);
        // 접속 해제 = 승인 카드 등급(운영에서도 거절하지 않는다 · 11-14) · 그 밖의 접속 명령은 여전히 보호.
        assert_eq!(c("run_command", true, "conn.disconnect"), Risk::AppWrite);
        assert_eq!(c("run_command", true, "conn.connect"), Risk::Protection);
        assert_eq!(c("run_command", true, "run.all"), Risk::Protection);
        assert_eq!(c("run_command", true, "view.log"), Risk::AppWrite);
        assert_eq!(c("set_editor_text", true, ""), Risk::AppWrite);
        assert_eq!(c("explain", true, "delete from t"), Risk::Read);
        assert_eq!(c("search_objects", false, ""), Risk::Read);
        assert_eq!(c("set_var", true, ""), Risk::Read);
        // 변수 삭제는 되돌릴 수 없는 앱 상태 변경 = 승인 카드(11-13).
        assert_eq!(c("delete_var", true, ""), Risk::AppWrite);
    }

    /// 정책 표 전수(등급 7 × 운영 2 × Auto 2 × 자동 쓰기 2 = 56) + 불변식.
    #[test]
    fn decide_policy_table() {
        let risks = [
            Risk::Read,
            Risk::AppWrite,
            Risk::Write,
            Risk::Bulk,
            Risk::Destructive,
            Risk::TxEnd,
            Risk::Protection,
        ];
        for r in risks {
            for prod in [false, true] {
                for auto in [false, true] {
                    for aw in [false, true] {
                        let d = decide(r, ctx(prod, auto, aw));
                        // 불변식 1: 운영에서 데이터를 바꾸는 요청은 절대 바로 실행되지 않는다.
                        if prod && r != Risk::Read {
                            assert_ne!(d, Decision::Allow, "{r:?} prod auto={auto} aw={aw}");
                        }
                        // 불변식 2: 대량·파괴적 변경은 어디서나 2단계.
                        if matches!(r, Risk::Bulk | Risk::Destructive) {
                            assert_eq!(d, Decision::TwoStage, "{r:?}");
                        }
                        // 불변식 3: 운영의 트랜잭션 끝·보호 변경은 거부.
                        if prod && matches!(r, Risk::TxEnd | Risk::Protection) {
                            assert_eq!(d, Decision::Deny, "{r:?}");
                        }
                        // 불변식 4: 보호 변경·트랜잭션 끝은 어떤 자동 승인으로도 풀리지 않는다.
                        if matches!(r, Risk::TxEnd | Risk::Protection) {
                            assert_ne!(d, Decision::Allow, "{r:?}");
                        }
                    }
                }
            }
        }
        // 조건마다 결과가 갈리는 쌍(MC/DC).
        assert_eq!(
            decide(Risk::Write, ctx(false, false, true)),
            Decision::Allow
        );
        assert_eq!(
            decide(Risk::Write, ctx(false, false, false)),
            Decision::Card
        );
        assert_eq!(decide(Risk::Write, ctx(true, false, true)), Decision::Card);
        assert_eq!(
            decide(Risk::AppWrite, ctx(false, true, false)),
            Decision::Allow
        );
        assert_eq!(
            decide(Risk::AppWrite, ctx(false, false, false)),
            Decision::Card
        );
        assert_eq!(
            decide(Risk::AppWrite, ctx(true, true, false)),
            Decision::Card
        );
        assert_eq!(decide(Risk::TxEnd, ctx(false, true, true)), Decision::Card);
        assert_eq!(decide(Risk::Read, ctx(true, false, false)), Decision::Allow);
    }

    #[test]
    fn impact_count_sql_for_single_table_dml() {
        let f = |s: &str| impact_count_sql(s, None);
        assert_eq!(
            f("DELETE FROM appdb.t WHERE id = 3 AND nm = 'a)b';").as_deref(),
            Some("SELECT COUNT(*) FROM appdb.t WHERE id = 3 AND nm = 'a)b'")
        );
        assert_eq!(
            f("delete from t").as_deref(),
            Some("SELECT COUNT(*) FROM t")
        );
        assert_eq!(
            f("update t x set a = (select max(b) from u where u.k = x.k) where x.id in (1, 2)")
                .as_deref(),
            Some("SELECT COUNT(*) FROM t x where x.id in (1, 2)")
        );
        assert_eq!(
            f("update t set a = 1").as_deref(),
            Some("SELECT COUNT(*) FROM t")
        );
        assert_eq!(
            f("truncate table s.t").as_deref(),
            Some("SELECT COUNT(*) FROM s.t")
        );
        assert_eq!(
            f("drop table if exists t").as_deref(),
            Some("SELECT COUNT(*) FROM t")
        );
        // 범위를 못 정하는 것 = None.
        assert_eq!(f("update t set a = 1 from u where u.k = t.k"), None);
        assert_eq!(f("delete from t using u where u.k = t.k"), None);
        assert_eq!(
            f("merge into t using s on (1 = 1) when matched then update set a = 1"),
            None
        );
        assert_eq!(f("delete from t; delete from u"), None);
        assert_eq!(f("drop view v"), None);
        assert_eq!(f("begin null; end;"), None);
    }

    #[test]
    fn summarize_names_kind_and_targets() {
        let (k, t) = summarize(
            "update appdb.t x set a = 1 where x.id in (select id from u)",
            None,
        );
        assert_eq!(k, "UPDATE");
        assert_eq!(t, vec!["appdb.t".to_string()]);
        let (k, t) = summarize("select a from t1 join s.t2 on 1 = 1; delete from t3", None);
        assert_eq!(k, "SELECT+DELETE ×2");
        assert_eq!(
            t,
            vec!["t1".to_string(), "s.t2".to_string(), "t3".to_string()]
        );
        let (k, t) = summarize("drop table if exists t9", None);
        assert_eq!(k, "DROP");
        assert_eq!(t, vec!["t9".to_string()]);
    }

    #[test]
    fn empty_probe_is_only_a_select_that_can_return_no_rows() {
        let p = is_empty_probe;
        assert!(p("SELECT * FROM BISCM.M4S_I301090 WHERE 1=0"));
        assert!(p("select a, b from t where 1 = 0;"));
        assert!(p("SELECT * FROM t x JOIN u y ON y.id = x.id WHERE 1=2"));
        // 행이 돌아올 수 있는 모양은 아니다.
        assert!(!p("SELECT * FROM t"));
        assert!(!p("SELECT * FROM t WHERE 1=1"));
        assert!(!p("SELECT * FROM t WHERE 1=0 OR 1=1"));
        assert!(!p("SELECT * FROM t WHERE a = 1 AND 1=0 ORDER BY a"));
        assert!(!p("SELECT * FROM t UNION ALL SELECT * FROM u WHERE 1=0"));
        assert!(!p("SELECT * FROM t WHERE a IN (SELECT b FROM u WHERE 1=0)"));
        assert!(!p("SELECT * FROM t -- WHERE 1=0"), "주석에 속지 않는다");
        assert!(
            !p("SELECT * FROM t WHERE c = ' WHERE 1=0'"),
            "문자열에 속지 않는다"
        );
        assert!(!p("SELECT pkg.f(1) FROM t WHERE 1=0"), "사용자 루틴 호출");
        assert!(!p("SELECT * FROM t WHERE 1=0; SELECT * FROM u"));
        assert!(!p("DELETE FROM t WHERE 1=0"));
        assert!(!p("WITH x AS (SELECT * FROM t) SELECT * FROM x WHERE 1=0"));
    }

    #[test]
    fn catalog_queries_are_recognized_conservatively() {
        let o = |s: &str| is_catalog_query(s, Some(Dialect::Oracle));
        let p = |s: &str| is_catalog_query(s, Some(Dialect::Postgres));
        let m = |s: &str| is_catalog_query(s, Some(Dialect::Mssql));
        // 데이터 사전만 읽는 조회.
        assert!(o("SELECT owner, table_name, comments FROM all_tab_comments WHERE comments LIKE '%자재%' ORDER BY CASE WHEN comments LIKE '%마스터%' THEN 0 ELSE 1 END, owner"));
        assert!(o("SELECT c.column_name, m.comments FROM all_tab_columns c JOIN all_col_comments m ON m.owner = c.owner AND m.table_name = c.table_name WHERE c.table_name = 'T1';"));
        assert!(o(
            "SELECT COUNT(*) FROM user_tables t, user_indexes i WHERE i.table_name = t.table_name"
        ));
        assert!(o("SELECT * FROM sys.dba_objects WHERE object_name IN (SELECT table_name FROM all_tables)"));
        assert!(o("SELECT sysdate FROM dual"));
        assert!(p(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'"
        ));
        assert!(p("SELECT relname FROM pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace"));
        assert!(m("SELECT name FROM sys.tables"));
        // 사용자 표가 하나라도 끼면 아니다(쉼표 조인 · JOIN · ON 뒤 쉼표 · 하위 질의 · 파생 표).
        assert!(!o("SELECT * FROM biscm.m4s_i002040"));
        assert!(!o("SELECT * FROM all_tables t, emp e"));
        assert!(!o(
            "SELECT * FROM all_tables t JOIN emp e ON e.x = t.table_name"
        ));
        assert!(!o(
            "SELECT * FROM all_tables t JOIN all_indexes i ON i.table_name = t.table_name, emp"
        ));
        assert!(!o(
            "SELECT * FROM all_tables WHERE table_name IN (SELECT name FROM emp)"
        ));
        assert!(!o("SELECT * FROM (SELECT * FROM emp) x"));
        assert!(!o("SELECT (SELECT MAX(sal) FROM emp) FROM dual"));
        // 판정 못 하는 모양 = 아니다: 사용자 함수 · DB 링크 · 인용 이름 · 여러 문장 · CTE · 조회가 아닌 문장.
        assert!(!o("SELECT pkg.get_all(1) FROM dual"));
        assert!(!o(
            "SELECT dbms_xmlgen.getxml('select * from emp') FROM dual"
        ));
        assert!(!o("SELECT * FROM all_tables@remote"));
        assert!(!o(r#"SELECT * FROM "ALL_TABLES""#));
        assert!(!o("SELECT * FROM all_tables; SELECT * FROM emp"));
        assert!(!o("WITH x AS (SELECT * FROM emp) SELECT * FROM x"));
        assert!(!o("DELETE FROM all_tables"));
        // 접두 규칙은 그 방언에서만(다른 DBMS에서는 사용자 표일 수 있다) · Oracle의 SYS 기반 표는 아니다.
        assert!(!p("SELECT * FROM user_accounts"));
        assert!(!m("SELECT * FROM all_orders"));
        assert!(!o("SELECT * FROM sys.user$"));
        assert!(!is_catalog_query("SELECT * FROM all_tables", None));
    }

    #[test]
    fn bind_assign_is_recognized() {
        assert!(is_bind_assign(":V := 'a'"));
        assert!(is_bind_assign(":VAR3 := :VAR2 + 2;"));
        assert!(!is_bind_assign("pkg.do_it(:V)"));
        // 식 안의 사용자 루틴 호출 = 변수 대입 등급이 아니다(카드를 거친다 · 11-13).
        assert!(!is_bind_assign(":V := pkg.audit_and_get(:A)"));
        assert!(!is_bind_assign(":V := my_func(1) + 2"));
        assert!(is_bind_assign(
            ":V := UPPER(:A) || TO_CHAR(SYSDATE, 'YYYY')"
        ));
        assert!(!is_bind_assign(":V := 1; delete from t"));
        assert!(!is_bind_assign("x := 1"));
        assert!(!is_bind_assign(":V :="));
    }

    #[test]
    fn parse_count_reads_the_last_number_line() {
        assert_eq!(parse_count("COUNT(*)\n337\n"), Some(337));
        assert_eq!(parse_count("cnt\n1,234"), Some(1234));
        assert_eq!(parse_count("ORA-00942: table or view does not exist"), None);
        assert_eq!(parse_count(""), None);
    }
}
