//! **DDL 문장 → 바뀌는 객체**(docs/57 §2-3 · T-138): 실행이 성공한 문장에서 (동작 · 종류 · 스키마 · 이름)을 뽑아
//! 객체 탐색기가 **그 폴더 하나만** 다시 읽게 한다(DataGrip "smart refresh"와 같은 생각).
//!
//! 머리말만 읽는 얕은 파서다 — 본문(컬럼 정의 · PL/SQL)은 보지 않는다. 읽지 못한 DDL(동적 SQL · `EXECUTE IMMEDIATE` ·
//! `sp_rename`)은 `None`이고, 그 몫은 유휴 워터마크(T2)와 "못 찾음" 신호(T4)가 맡는다 — 추측으로 전체를 다시 읽지 않는다.

use crate::Dialect;

/// 무엇을 했는가.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdlVerb {
    Create,
    Drop,
    /// 구조 변경(목록은 그대로 · 테이블이면 컬럼이 바뀔 수 있다).
    Alter,
    /// 이름 바꾸기(`RENAME a TO b` · `ALTER … RENAME TO …`) — 목록이 바뀐다.
    Rename,
    /// `COMMENT ON` — 목록은 그대로(설명만).
    Comment,
}

impl DdlVerb {
    /// 그 종류 폴더의 **목록**이 바뀌는가.
    #[must_use]
    pub fn changes_list(self) -> bool {
        matches!(self, DdlVerb::Create | DdlVerb::Drop | DdlVerb::Rename)
    }
}

/// 객체 종류(탐색기 폴더에 대응 · `nsql-catalog::ObjectKind`로의 사상은 UI 쪽 — 이 크레이트는 의존 0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DdlKind {
    Table,
    View,
    MaterializedView,
    Index,
    Sequence,
    Procedure,
    Function,
    Package,
    PackageBody,
    Trigger,
    Synonym,
    Type,
    /// 스키마·사용자 — 탐색기 루트의 스키마 목록이 바뀐다.
    Schema,
}

/// 문장 하나가 건드린 객체.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DdlTarget {
    pub verb: DdlVerb,
    pub kind: DdlKind,
    /// 적힌 스키마(없으면 그 세션의 현재 스키마 — 호출자가 채운다).
    pub schema: Option<String>,
    /// 객체 이름(따옴표를 벗긴 것 · 대소문자는 적힌 그대로 — 비교는 호출자가 대소문자 무시로).
    pub name: String,
}

/// 머리말 토큰: 낱말(대문자로 접은 것 + 원문) 또는 점으로 이은 이름.
#[derive(Debug)]
struct Tok {
    /// 점으로 나눈 부분들(따옴표 벗김). 낱말 하나면 길이 1.
    parts: Vec<String>,
    /// 따옴표 없는 단일 낱말의 대문자형(키워드 비교용 · 따옴표가 있었으면 빈 문자열).
    upper: String,
}

/// 머리말에서 토큰을 최대 `limit`개 읽는다 — 주석·공백을 건너뛰고 `(`·`;`·`,`·`=`에서 멈춘다.
fn head_tokens(sql: &str, limit: usize) -> Vec<Tok> {
    let ch: Vec<char> = sql.chars().collect();
    let mut i = 0usize;
    let mut out: Vec<Tok> = Vec::new();
    while i < ch.len() && out.len() < limit {
        let c = ch[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '-' && ch.get(i + 1) == Some(&'-') {
            while i < ch.len() && ch[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && ch.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < ch.len() && !(ch[i] == '*' && ch[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(ch.len());
            continue;
        }
        // 이름(따옴표 있는 부분 포함 · 점으로 이어짐).
        let mut parts: Vec<String> = Vec::new();
        let mut quoted = false;
        while let Some(&c) = ch.get(i) {
            let close = match c {
                '"' => Some('"'),
                '`' => Some('`'),
                '[' => Some(']'),
                _ => None,
            };
            let mut part = String::new();
            if let Some(q) = close {
                quoted = true;
                i += 1;
                while i < ch.len() && ch[i] != q {
                    part.push(ch[i]);
                    i += 1;
                }
                i = (i + 1).min(ch.len());
            } else {
                while i < ch.len()
                    && (ch[i].is_alphanumeric() || matches!(ch[i], '_' | '$' | '#' | '@'))
                {
                    part.push(ch[i]);
                    i += 1;
                }
            }
            if part.is_empty() {
                break;
            }
            parts.push(part);
            if ch.get(i) == Some(&'.') {
                i += 1;
                continue;
            }
            break;
        }
        if parts.is_empty() {
            // 이름이 아닌 글자(괄호·세미콜론 …) = 머리말 끝.
            break;
        }
        let upper = if parts.len() == 1 && !quoted {
            parts[0].to_uppercase()
        } else {
            String::new()
        };
        out.push(Tok { parts, upper });
    }
    out
}

fn kind_of(word: &str) -> Option<DdlKind> {
    Some(match word {
        "TABLE" => DdlKind::Table,
        "VIEW" => DdlKind::View,
        "INDEX" => DdlKind::Index,
        "SEQUENCE" => DdlKind::Sequence,
        "PROCEDURE" | "PROC" => DdlKind::Procedure,
        "FUNCTION" => DdlKind::Function,
        "PACKAGE" => DdlKind::Package,
        "TRIGGER" => DdlKind::Trigger,
        "SYNONYM" => DdlKind::Synonym,
        "TYPE" => DdlKind::Type,
        "SCHEMA" | "USER" => DdlKind::Schema,
        _ => return None,
    })
}

/// 종류 앞에 올 수 있는 꾸밈말(건너뛴다).
fn is_modifier(word: &str) -> bool {
    matches!(
        word,
        "OR" | "REPLACE"
            | "ALTER"
            | "GLOBAL"
            | "LOCAL"
            | "PRIVATE"
            | "TEMPORARY"
            | "TEMP"
            | "UNLOGGED"
            | "UNIQUE"
            | "BITMAP"
            | "CLUSTERED"
            | "NONCLUSTERED"
            | "FULLTEXT"
            | "SPATIAL"
            | "EDITIONABLE"
            | "NONEDITIONABLE"
            | "FORCE"
            | "NO"
            | "PUBLIC"
            | "RECURSIVE"
            | "VIRTUAL"
            | "EXTERNAL"
            | "FOREIGN"
            | "SHARDED"
            | "DUPLICATED"
            | "IMMUTABLE"
            | "BLOCKCHAIN"
            | "DEFINER"
            | "ALGORITHM"
            | "SQL"
            | "SECURITY"
    )
}

/// 이름 앞에 올 수 있는 말(`IF [NOT] EXISTS` · `CONCURRENTLY` · `ONLY`).
fn is_name_prefix(word: &str) -> bool {
    matches!(word, "IF" | "NOT" | "EXISTS" | "CONCURRENTLY" | "ONLY")
}

fn split_name(parts: &[String], drop_last: bool) -> Option<(Option<String>, String)> {
    let parts = if drop_last {
        parts.get(..parts.len().checked_sub(1)?)?
    } else {
        parts
    };
    let name = parts.last()?.clone();
    // a.b.c(SQL Server db.schema.name) = 뒤의 둘만.
    let schema = (parts.len() >= 2).then(|| parts[parts.len() - 2].clone());
    Some((schema, name))
}

/// 실행한 문장 하나가 바꾸는 객체(모르면 `None`). `dialect`는 방언별 표기 차이에만 쓴다(지금은 예약 — 머리말 문법이 거의 같다).
#[must_use]
pub fn ddl_target(sql: &str, _dialect: Dialect) -> Option<DdlTarget> {
    let toks = head_tokens(sql, 16);
    let first = toks.first()?.upper.as_str();
    let verb = match first {
        "CREATE" => DdlVerb::Create,
        "DROP" => DdlVerb::Drop,
        "ALTER" => DdlVerb::Alter,
        "RENAME" => {
            // Oracle `RENAME old TO new` — 테이블·뷰·시퀀스·동의어 어느 것일 수도 있지만 대부분 테이블.
            let (schema, name) = split_name(&toks.get(1)?.parts, false)?;
            return Some(DdlTarget {
                verb: DdlVerb::Rename,
                kind: DdlKind::Table,
                schema,
                name,
            });
        }
        "COMMENT" => {
            if toks.get(1)?.upper != "ON" {
                return None;
            }
            let what = toks.get(2)?.upper.as_str();
            let column = what == "COLUMN";
            let (kind, at) = if column {
                (DdlKind::Table, 3)
            } else if what == "MATERIALIZED" && toks.get(3).is_some_and(|t| t.upper == "VIEW") {
                (DdlKind::MaterializedView, 4)
            } else {
                (kind_of(what)?, 3)
            };
            let (schema, name) = split_name(&toks.get(at)?.parts, column)?;
            return Some(DdlTarget {
                verb: DdlVerb::Comment,
                kind,
                schema,
                name,
            });
        }
        _ => return None,
    };
    // 꾸밈말을 건너 종류를 찾는다.
    let mut i = 1;
    let mut materialized = false;
    let kind = loop {
        let w = toks.get(i)?.upper.as_str();
        i += 1;
        if w == "MATERIALIZED" {
            materialized = true;
            continue;
        }
        if let Some(k) = kind_of(w) {
            break k;
        }
        if !is_modifier(w) {
            return None;
        }
    };
    let mut kind = match (kind, materialized) {
        (DdlKind::View, true) => DdlKind::MaterializedView,
        (k, _) => k,
    };
    // `PACKAGE BODY` · `TYPE BODY`.
    if toks.get(i).is_some_and(|t| t.upper == "BODY") {
        if kind == DdlKind::Package {
            kind = DdlKind::PackageBody;
        }
        i += 1;
    }
    while toks.get(i).is_some_and(|t| is_name_prefix(&t.upper)) {
        i += 1;
    }
    let name_tok = toks.get(i)?;
    // `CREATE INDEX ON t (…)`(PostgreSQL 이름 없는 인덱스) = 이름을 모른다 → 폴더만 갱신하도록 빈 이름.
    let (schema, name) = if kind == DdlKind::Index && name_tok.upper == "ON" {
        let (s, _) = split_name(&toks.get(i + 1)?.parts, false)?;
        (s, String::new())
    } else {
        split_name(&name_tok.parts, false)?
    };
    // `ALTER … RENAME TO …` = 목록이 바뀐다.
    let verb = if verb == DdlVerb::Alter && toks[i..].iter().any(|t| t.upper == "RENAME") {
        DdlVerb::Rename
    } else {
        verb
    };
    Some(DdlTarget {
        verb,
        kind,
        schema,
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(sql: &str) -> Option<(DdlVerb, DdlKind, Option<&'static str>, String)> {
        ddl_target(sql, Dialect::Oracle).map(|d| {
            let schema: Option<&'static str> = d.schema.map(|s| &*Box::leak(s.into_boxed_str()));
            (d.verb, d.kind, schema, d.name)
        })
    }

    #[test]
    fn create_variants_across_dialects() {
        use DdlKind as K;
        use DdlVerb as V;
        assert_eq!(
            t("create table t1 (a int)"),
            Some((V::Create, K::Table, None, "t1".into()))
        );
        assert_eq!(
            t("CREATE GLOBAL TEMPORARY TABLE hr.tmp_x (a NUMBER) ON COMMIT DELETE ROWS"),
            Some((V::Create, K::Table, Some("hr"), "tmp_x".into()))
        );
        assert_eq!(
            t("-- c\n/* c2 */ CREATE OR REPLACE FORCE EDITIONABLE VIEW \"Hr\".\"V One\" AS SELECT 1 FROM dual"),
            Some((V::Create, K::View, Some("Hr"), "V One".into()))
        );
        assert_eq!(
            t("CREATE MATERIALIZED VIEW IF NOT EXISTS mv AS SELECT 1"),
            Some((V::Create, K::MaterializedView, None, "mv".into()))
        );
        assert_eq!(
            t("CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS ix_a ON public.t (a)"),
            Some((V::Create, K::Index, None, "ix_a".into()))
        );
        assert_eq!(
            t("CREATE INDEX ON sales.t (a)"),
            Some((V::Create, K::Index, Some("sales"), String::new()))
        );
        assert_eq!(
            t("CREATE OR ALTER PROC [dbo].[usp_x] @a int AS SELECT 1"),
            Some((V::Create, K::Procedure, Some("dbo"), "usp_x".into()))
        );
        assert_eq!(
            t("create or replace package body pkg_a as end;"),
            Some((V::Create, K::PackageBody, None, "pkg_a".into()))
        );
        assert_eq!(
            t("CREATE PUBLIC SYNONYM s1 FOR hr.t"),
            Some((V::Create, K::Synonym, None, "s1".into()))
        );
        assert_eq!(
            t("CREATE SCHEMA IF NOT EXISTS app"),
            Some((V::Create, K::Schema, None, "app".into()))
        );
        // SQL Server 3부분 이름 = 뒤의 둘.
        assert_eq!(
            t("CREATE TABLE db1.dbo.orders (id int)"),
            Some((V::Create, K::Table, Some("dbo"), "orders".into()))
        );
    }

    #[test]
    fn drop_alter_rename_comment() {
        use DdlKind as K;
        use DdlVerb as V;
        assert_eq!(
            t("DROP TABLE IF EXISTS hr.t1 CASCADE"),
            Some((V::Drop, K::Table, Some("hr"), "t1".into()))
        );
        assert_eq!(
            t("drop materialized view mv"),
            Some((V::Drop, K::MaterializedView, None, "mv".into()))
        );
        assert_eq!(
            t("ALTER TABLE ONLY public.t ADD COLUMN b int"),
            Some((V::Alter, K::Table, Some("public"), "t".into()))
        );
        assert_eq!(
            t("ALTER TABLE t RENAME TO t2"),
            Some((V::Rename, K::Table, None, "t".into()))
        );
        assert_eq!(
            t("RENAME old_t TO new_t"),
            Some((V::Rename, K::Table, None, "old_t".into()))
        );
        assert_eq!(
            t("COMMENT ON COLUMN hr.emp.name IS 'x'"),
            Some((V::Comment, K::Table, Some("hr"), "emp".into()))
        );
        assert_eq!(
            t("COMMENT ON MATERIALIZED VIEW mv IS 'x'"),
            Some((V::Comment, K::MaterializedView, None, "mv".into()))
        );
        assert_eq!(
            t("COMMENT ON TABLE emp IS 'x'"),
            Some((V::Comment, K::Table, None, "emp".into()))
        );
        assert!(V::Create.changes_list() && V::Rename.changes_list() && !V::Alter.changes_list());
    }

    #[test]
    fn unknown_or_non_ddl_is_none() {
        for sql in [
            "SELECT * FROM t",
            "TRUNCATE TABLE t",
            "BEGIN EXECUTE IMMEDIATE 'create table x(a int)'; END;",
            "EXEC sp_rename 'a', 'b'",
            "CREATE DATABASE d",
            "GRANT SELECT ON t TO u",
            "CREATE",
            "",
        ] {
            assert_eq!(t(sql), None, "{sql}");
        }
    }
}
