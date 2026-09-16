//! 결과 행 → SQL 문 생성(SELECT · INSERT · UPDATE · DELETE · MERGE) — GUI 그리드 "Copy SQL"과 CLI `-f sql:*`가 같은 코드를 쓴다
//! ([docs/41](../../../docs/41-sql-copy-key-rules.md) · 사용자 09-16).
//!
//! **키 규칙**(`KeyMode::Pk` 기본): PK → 첫 유니크 제약/인덱스 → **앞 3개 컬럼**(경고 1회). `KeyMode::All` = 전체 컬럼이 키.
//! 키 컬럼은 결과에 **모두 있어야** 채택된다(없으면 다음 후보).

use nsql_core::{Dialect, KeyInfo, Value};

/// 생성할 문장 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlKind {
    Select,
    Insert,
    Update,
    Delete,
    Merge,
}

impl SqlKind {
    /// `select` · `insert` · `update` · `delete` · `merge`(`upsert`).
    pub fn parse(s: &str) -> Option<SqlKind> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "select" | "sel" => SqlKind::Select,
            "insert" | "ins" => SqlKind::Insert,
            "update" | "upd" => SqlKind::Update,
            "delete" | "del" => SqlKind::Delete,
            "merge" | "upsert" => SqlKind::Merge,
            _ => return None,
        })
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            SqlKind::Select => "select",
            SqlKind::Insert => "insert",
            SqlKind::Update => "update",
            SqlKind::Delete => "delete",
            SqlKind::Merge => "merge",
        }
    }
}

/// 유일성 기준 설정(`sql.key_mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyMode {
    /// PK 또는 첫 유니크 인덱스(없으면 앞 3개 컬럼 + 경고) — 기본.
    #[default]
    Pk,
    /// 전체 컬럼.
    All,
}

impl KeyMode {
    pub fn parse(s: &str) -> Option<KeyMode> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "pk" | "key" | "unique" => KeyMode::Pk,
            "all" | "all_columns" => KeyMode::All,
            _ => return None,
        })
    }
}

/// 채택된 키의 출처(경고·표시용).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeySource {
    /// 기본 키.
    Pk,
    /// 유니크 제약/인덱스(이름).
    Unique(String),
    /// 카탈로그에 키가 없어 앞 n개 컬럼(경고 대상).
    FirstN(usize),
    /// 전체 컬럼(설정 `all`).
    All,
}

/// 문장의 WHERE/ON 기준 컬럼.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeySpec {
    pub cols: Vec<String>,
    pub source: KeySource,
}

impl KeySpec {
    /// 경고가 필요한가(카탈로그 키 없이 앞 컬럼으로 대체).
    #[must_use]
    pub fn needs_warning(&self) -> bool {
        matches!(self.source, KeySource::FirstN(_))
    }
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn all_present(cols: &[String], names: &[String]) -> bool {
    !cols.is_empty() && cols.iter().all(|k| names.iter().any(|n| eq_ci(n, k)))
}

/// 키 선택 — `names` = 결과(선택) 컬럼 이름 · `info` = 카탈로그 키(없으면 `None`).
#[must_use]
pub fn choose_key(mode: KeyMode, info: Option<&KeyInfo>, names: &[String]) -> KeySpec {
    if mode == KeyMode::All {
        return KeySpec {
            cols: names.to_vec(),
            source: KeySource::All,
        };
    }
    if let Some(i) = info {
        if all_present(&i.pk, names) {
            return KeySpec {
                cols: i.pk.clone(),
                source: KeySource::Pk,
            };
        }
        for (uname, cols) in &i.unique {
            if all_present(cols, names) {
                return KeySpec {
                    cols: cols.clone(),
                    source: KeySource::Unique(uname.clone()),
                };
            }
        }
    }
    let n = names.len().min(3);
    KeySpec {
        cols: names[..n].to_vec(),
        source: KeySource::FirstN(n),
    }
}

/// 실행한 SQL에서 대상 테이블을 추정한다 — `FROM t` · `INSERT INTO t` · `UPDATE t` · `MERGE INTO t`(서브쿼리 `FROM (`는 건너뜀).
/// 별칭·스키마 접두(`s.t`)는 그대로 돌려준다(카탈로그 조회는 [`split_table`] 참고).
#[must_use]
pub fn guess_table(sql: &str) -> Option<String> {
    let toks: Vec<&str> = sql
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(' || c == ')')
        .filter(|t| !t.is_empty())
        .collect();
    let mut i = 0;
    while i < toks.len() {
        let up = toks[i].to_ascii_uppercase();
        let next = toks.get(i + 1).copied();
        let cand = match up.as_str() {
            "FROM" | "UPDATE" => next,
            "INTO"
                if i > 0
                    && matches!(
                        toks[i - 1].to_ascii_uppercase().as_str(),
                        "INSERT" | "MERGE"
                    ) =>
            {
                next
            }
            _ => None,
        };
        if let Some(t) = cand {
            let tu = t.to_ascii_uppercase();
            let is_kw = matches!(tu.as_str(), "SELECT" | "WHERE" | "DUAL" | "VALUES");
            let is_ident = t.chars().all(|c| {
                c.is_alphanumeric()
                    || c == '_'
                    || c == '.'
                    || c == '$'
                    || c == '#'
                    || c == '"'
                    || c == '['
                    || c == ']'
                    || c == '`'
            });
            if !is_kw && is_ident {
                return Some(t.to_string());
            }
        }
        i += 1;
    }
    None
}

/// `schema.table`/`"Q"."T"`/`[s].[t]` → (스키마, 테이블) — 따옴표를 벗기고 방언의 기본 대소문자로(Oracle 대문자 · PostgreSQL 소문자 ·
/// 따옴표로 감싼 이름은 그대로).
#[must_use]
pub fn split_table(dialect: Dialect, name: &str) -> (Option<String>, String) {
    fn unq(dialect: Dialect, part: &str) -> String {
        let quoted = (part.starts_with('"') && part.ends_with('"'))
            || (part.starts_with('[') && part.ends_with(']'))
            || (part.starts_with('`') && part.ends_with('`'));
        if quoted && part.len() >= 2 {
            return part[1..part.len() - 1].to_string();
        }
        match dialect {
            Dialect::Oracle => part.to_ascii_uppercase(),
            Dialect::Postgres => part.to_ascii_lowercase(),
            _ => part.to_string(),
        }
    }
    let parts: Vec<&str> = name.split('.').collect();
    match parts.as_slice() {
        [t] => (None, unq(dialect, t)),
        [s, t] => (Some(unq(dialect, s)), unq(dialect, t)),
        [.., s, t] => (Some(unq(dialect, s)), unq(dialect, t)),
        [] => (None, String::new()),
    }
}

/// 행마다 한 문장 — `names` = 컬럼 이름(출력 순서) · `rows` = 그 순서의 값 · `table` = 대상(없으면 `T`).
#[must_use]
pub fn generate(
    dialect: Dialect,
    table: &str,
    names: &[String],
    rows: &[Vec<Value>],
    kind: SqlKind,
    key: &KeySpec,
) -> String {
    let table = if table.is_empty() { "T" } else { table };
    let key_idx: Vec<usize> = names
        .iter()
        .enumerate()
        .filter(|(_, n)| key.cols.iter().any(|k| eq_ci(k, n)))
        .map(|(i, _)| i)
        .collect();
    let key_idx: Vec<usize> = if key_idx.is_empty() && !names.is_empty() {
        vec![0]
    } else {
        key_idx
    };
    let col_list = names.join(", ");
    let mut out = String::new();
    for row in rows {
        let lits: Vec<String> = (0..names.len())
            .map(|i| {
                row.get(i)
                    .map(|v| v.to_sql_literal(dialect))
                    .unwrap_or_else(|| "NULL".into())
            })
            .collect();
        let pred: Vec<String> = key_idx
            .iter()
            .map(|&i| {
                if lits[i] == "NULL" {
                    format!("{} IS NULL", names[i])
                } else {
                    format!("{} = {}", names[i], lits[i])
                }
            })
            .collect();
        let key_pred = pred.join(" AND ");
        let non_key: Vec<usize> = (0..names.len()).filter(|i| !key_idx.contains(i)).collect();
        let sets: Vec<String> = non_key
            .iter()
            .map(|&i| format!("{} = {}", names[i], lits[i]))
            .collect();
        let stmt = match kind {
            SqlKind::Select => format!("SELECT {col_list} FROM {table} WHERE {key_pred};"),
            SqlKind::Insert => format!(
                "INSERT INTO {table} ({col_list}) VALUES ({});",
                lits.join(", ")
            ),
            SqlKind::Update => {
                if sets.is_empty() {
                    // 전 컬럼이 키 — 같은 값으로 SET(문장 형태 유지).
                    let all: Vec<String> = (0..names.len())
                        .map(|i| format!("{} = {}", names[i], lits[i]))
                        .collect();
                    format!("UPDATE {table} SET {} WHERE {key_pred};", all.join(", "))
                } else {
                    format!("UPDATE {table} SET {} WHERE {key_pred};", sets.join(", "))
                }
            }
            SqlKind::Delete => format!("DELETE FROM {table} WHERE {key_pred};"),
            SqlKind::Merge => {
                let src: Vec<String> = names
                    .iter()
                    .zip(lits.iter())
                    .map(|(n, l)| format!("{l} AS {n}"))
                    .collect();
                let on: Vec<String> = key_idx
                    .iter()
                    .map(|&i| format!("t.{0} = s.{0}", names[i]))
                    .collect();
                let upd: Vec<String> = non_key
                    .iter()
                    .map(|&i| format!("t.{0} = s.{0}", names[i]))
                    .collect();
                let ins_vals: Vec<String> = names.iter().map(|n| format!("s.{n}")).collect();
                let src_sel = match dialect {
                    Dialect::Oracle => format!("SELECT {} FROM DUAL", src.join(", ")),
                    _ => format!("SELECT {}", src.join(", ")),
                };
                let upd_clause = if upd.is_empty() {
                    String::new()
                } else {
                    format!(" WHEN MATCHED THEN UPDATE SET {}", upd.join(", "))
                };
                match dialect {
                    Dialect::Mssql => format!(
                        "MERGE {table} AS t USING ({src_sel}) AS s ON {}{upd_clause} WHEN NOT MATCHED THEN INSERT ({col_list}) VALUES ({});",
                        on.join(" AND "),
                        ins_vals.join(", ")
                    ),
                    _ => format!(
                        "MERGE INTO {table} t USING ({src_sel}) s ON ({}){upd_clause} WHEN NOT MATCHED THEN INSERT ({col_list}) VALUES ({});",
                        on.join(" AND "),
                        ins_vals.join(", ")
                    ),
                }
            }
        };
        out.push_str(&stmt);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["ID", "CODE", "NAME", "QTY"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
    fn rows() -> Vec<Vec<Value>> {
        vec![vec![
            Value::Int(1),
            Value::Str("A".into()),
            Value::Str("x'y".into()),
            Value::Null,
        ]]
    }

    #[test]
    fn key_choice_pk_unique_firstn_all() {
        let n = names();
        let info = KeyInfo {
            pk: vec!["ID".into()],
            unique: vec![("UX".into(), vec!["CODE".into(), "NAME".into()])],
        };
        assert_eq!(
            choose_key(KeyMode::Pk, Some(&info), &n).source,
            KeySource::Pk
        );
        // PK 컬럼이 결과에 없으면 유니크로.
        let no_id: Vec<String> = n[1..].to_vec();
        let k = choose_key(KeyMode::Pk, Some(&info), &no_id);
        assert_eq!(k.source, KeySource::Unique("UX".into()));
        assert_eq!(k.cols, vec!["CODE", "NAME"]);
        // 카탈로그 없음 = 앞 3개 + 경고.
        let k = choose_key(KeyMode::Pk, None, &n);
        assert_eq!(k.source, KeySource::FirstN(3));
        assert!(k.needs_warning());
        assert_eq!(k.cols, vec!["ID", "CODE", "NAME"]);
        // 전체 컬럼.
        let k = choose_key(KeyMode::All, Some(&info), &n);
        assert_eq!(k.source, KeySource::All);
        assert_eq!(k.cols.len(), 4);
    }

    #[test]
    fn all_five_kinds_with_composite_key() {
        let key = KeySpec {
            cols: vec!["ID".into(), "CODE".into()],
            source: KeySource::Pk,
        };
        let g = |k| generate(Dialect::Oracle, "EMP", &names(), &rows(), k, &key);
        assert_eq!(
            g(SqlKind::Select),
            "SELECT ID, CODE, NAME, QTY FROM EMP WHERE ID = 1 AND CODE = 'A';\n"
        );
        assert_eq!(
            g(SqlKind::Insert),
            "INSERT INTO EMP (ID, CODE, NAME, QTY) VALUES (1, 'A', 'x''y', NULL);\n"
        );
        assert_eq!(
            g(SqlKind::Update),
            "UPDATE EMP SET NAME = 'x''y', QTY = NULL WHERE ID = 1 AND CODE = 'A';\n"
        );
        assert_eq!(
            g(SqlKind::Delete),
            "DELETE FROM EMP WHERE ID = 1 AND CODE = 'A';\n"
        );
        let m = g(SqlKind::Merge);
        assert!(m.starts_with("MERGE INTO EMP t USING (SELECT 1 AS ID, 'A' AS CODE, 'x''y' AS NAME, NULL AS QTY FROM DUAL) s ON (t.ID = s.ID AND t.CODE = s.CODE) WHEN MATCHED THEN UPDATE SET t.NAME = s.NAME, t.QTY = s.QTY WHEN NOT MATCHED THEN INSERT"), "{m}");
        let ms = generate(
            Dialect::Mssql,
            "dbo.Emp",
            &names(),
            &rows(),
            SqlKind::Merge,
            &key,
        );
        assert!(
            ms.starts_with("MERGE dbo.Emp AS t USING (SELECT 1 AS ID"),
            "{ms}"
        );
    }

    #[test]
    fn null_key_uses_is_null_and_all_mode_updates_all() {
        let key = KeySpec {
            cols: vec!["QTY".into()],
            source: KeySource::Pk,
        };
        let d = generate(
            Dialect::Postgres,
            "t",
            &names(),
            &rows(),
            SqlKind::Delete,
            &key,
        );
        assert_eq!(d, "DELETE FROM t WHERE QTY IS NULL;\n");
        let all = KeySpec {
            cols: names(),
            source: KeySource::All,
        };
        let u = generate(
            Dialect::Postgres,
            "t",
            &names(),
            &rows(),
            SqlKind::Update,
            &all,
        );
        assert!(u.starts_with("UPDATE t SET ID = 1, CODE = 'A'"));
        assert!(u.contains("WHERE ID = 1 AND CODE = 'A' AND NAME = 'x''y' AND QTY IS NULL;"));
    }

    #[test]
    fn table_guess_and_split() {
        assert_eq!(
            guess_table("SELECT * FROM HR.EMP e WHERE 1=1").as_deref(),
            Some("HR.EMP")
        );
        assert_eq!(
            guess_table("select a from (select 1 from dual) x").as_deref(),
            None
        );
        assert_eq!(
            split_table(Dialect::Oracle, "hr.emp"),
            (Some("HR".into()), "EMP".into())
        );
        assert_eq!(
            split_table(Dialect::Postgres, "\"Emp\""),
            (None, "Emp".into())
        );
        assert_eq!(
            split_table(Dialect::Mssql, "[dbo].[Emp]"),
            (Some("dbo".into()), "Emp".into())
        );
    }
}
