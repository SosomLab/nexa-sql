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
    k.starts_with("run.")
        || k.starts_with("tx.")
        || k.starts_with("hold.")
        || k.starts_with("conn.")
        || k.starts_with("sess.")
        || k.starts_with("grid.apply")
        || k.starts_with("import.")
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
        assert_eq!(c("run_command", true, "run.all"), Risk::Protection);
        assert_eq!(c("run_command", true, "view.log"), Risk::AppWrite);
        assert_eq!(c("set_editor_text", true, ""), Risk::AppWrite);
        assert_eq!(c("explain", true, "delete from t"), Risk::Read);
        assert_eq!(c("search_objects", false, ""), Risk::Read);
        assert_eq!(c("set_var", true, ""), Risk::Read);
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
    fn bind_assign_is_recognized() {
        assert!(is_bind_assign(":V := 'a'"));
        assert!(is_bind_assign(":VAR3 := :VAR2 + 2;"));
        assert!(!is_bind_assign("pkg.do_it(:V)"));
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
