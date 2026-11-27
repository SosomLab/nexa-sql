//! 방언별 문장 렌더(순수). 규칙 표 = docs/119 §3-3.

use crate::{
    Blocked, Change, Column, Dialect, Index, Key, KeyKind, Plan, Reason, Risk, Stmt, Table,
};

/// 인용 없이 쓰면 안 되는 흔한 예약어(전 방언 공통의 작은 집합 — 호스트의 방언 표가 더 넓다 · 여기는 안전망).
const RESERVED: &[&str] = &[
    "ALL",
    "ALTER",
    "AND",
    "AS",
    "ASC",
    "BETWEEN",
    "BY",
    "CASE",
    "CHECK",
    "COLUMN",
    "COMMENT",
    "CONSTRAINT",
    "CREATE",
    "DATE",
    "DEFAULT",
    "DELETE",
    "DESC",
    "DISTINCT",
    "DROP",
    "ELSE",
    "END",
    "EXISTS",
    "FOREIGN",
    "FROM",
    "GRANT",
    "GROUP",
    "HAVING",
    "IN",
    "INDEX",
    "INSERT",
    "INTO",
    "IS",
    "JOIN",
    "KEY",
    "LEVEL",
    "LIKE",
    "NOT",
    "NULL",
    "NUMBER",
    "ON",
    "OR",
    "ORDER",
    "PRIMARY",
    "REFERENCES",
    "ROW",
    "ROWS",
    "SELECT",
    "SET",
    "SIZE",
    "TABLE",
    "THEN",
    "TO",
    "UNION",
    "UNIQUE",
    "UPDATE",
    "USER",
    "VALUES",
    "VIEW",
    "WHEN",
    "WHERE",
    "WITH",
];

/// 방언 식별자(필요할 때만 인용): 글자·숫자·`_`·`$`만이고 그 방언의 접힘 대소문자와 같으며 예약어가 아니면 그대로.
#[must_use]
pub fn ident(d: Dialect, name: &str) -> String {
    let mut chars = name.chars();
    let head_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    let rest_ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    let case_ok = match d {
        Dialect::Oracle => !name.chars().any(|c| c.is_ascii_lowercase()),
        Dialect::Postgres => !name.chars().any(|c| c.is_ascii_uppercase()),
        Dialect::Mssql | Dialect::Mysql | Dialect::Sqlite => true,
    };
    let reserved = RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name));
    if head_ok && rest_ok && case_ok && !reserved {
        return name.to_string();
    }
    match d {
        Dialect::Mssql => format!("[{}]", name.replace(']', "]]")),
        Dialect::Mysql => format!("`{}`", name.replace('`', "``")),
        _ => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

/// 사용자가 새로 친 이름을 그 방언의 기본 접힘으로(Oracle 대문자 · PostgreSQL 소문자) — `"…"`로 감쌌으면 벗기고 그대로.
#[must_use]
pub fn fold_new_name(d: Dialect, typed: &str) -> String {
    let t = typed.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return t[1..t.len() - 1].replace("\"\"", "\"");
    }
    match d {
        Dialect::Oracle => t.to_uppercase(),
        Dialect::Postgres => t.to_lowercase(),
        _ => t.to_string(),
    }
}

/// `스키마.이름`(스키마가 비면 이름만).
#[must_use]
pub fn qualified(d: Dialect, schema: &str, name: &str) -> String {
    if schema.is_empty() {
        ident(d, name)
    } else {
        format!("{}.{}", ident(d, schema), ident(d, name))
    }
}

fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// SQL Server N'…' 리터럴.
fn nlit(s: &str) -> String {
    format!("N'{}'", s.replace('\'', "''"))
}

fn cols(d: Dialect, names: &[String]) -> String {
    names
        .iter()
        .map(|c| ident(d, c))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 외래 키 대상(`스키마.이름` 또는 `이름`).
fn ref_name(d: Dialect, s: &str) -> String {
    match s.split_once('.') {
        Some((sc, n)) => qualified(d, sc, n),
        None => ident(d, s),
    }
}

fn key_body(d: Dialect, k: &Key) -> String {
    match k.kind {
        KeyKind::Primary => format!("PRIMARY KEY ({})", cols(d, &k.cols)),
        KeyKind::Unique => format!("UNIQUE ({})", cols(d, &k.cols)),
        KeyKind::Check => format!("CHECK ({})", k.check.trim()),
        KeyKind::Foreign => {
            let mut s = format!(
                "FOREIGN KEY ({}) REFERENCES {}",
                cols(d, &k.cols),
                ref_name(d, &k.ref_table)
            );
            if !k.ref_cols.is_empty() {
                s.push_str(&format!(" ({})", cols(d, &k.ref_cols)));
            }
            let od = k.on_delete.trim();
            if !od.is_empty() && !od.eq_ignore_ascii_case("NO ACTION") {
                s.push_str(&format!(" ON DELETE {}", od.to_uppercase()));
            }
            s
        }
    }
}

fn key_clause(d: Dialect, k: &Key) -> String {
    if k.name.is_empty() {
        key_body(d, k)
    } else {
        format!("CONSTRAINT {} {}", ident(d, &k.name), key_body(d, k))
    }
}

/// 컬럼 정의(이름 타입 기본값 NULL) — `mysql_full` = MySQL `MODIFY`용 전체 정의(코멘트 포함).
fn col_def(d: Dialect, c: &Column, table: &str) -> String {
    let mut s = format!("{} {}", ident(d, &c.name), c.data_type.trim());
    let def = c.default.trim();
    match d {
        Dialect::Mssql => {
            if !def.is_empty() {
                s.push_str(&format!(
                    " CONSTRAINT {} DEFAULT {def}",
                    ident(d, &df_name(table, &c.name))
                ));
            }
            s.push_str(if c.nullable { " NULL" } else { " NOT NULL" });
        }
        Dialect::Mysql => {
            s.push_str(if c.nullable { " NULL" } else { " NOT NULL" });
            if !def.is_empty() {
                s.push_str(&format!(" DEFAULT {def}"));
            }
            if !c.comment.is_empty() {
                s.push_str(&format!(" COMMENT {}", lit(&c.comment)));
            }
        }
        Dialect::Oracle | Dialect::Postgres | Dialect::Sqlite => {
            if !def.is_empty() {
                s.push_str(&format!(" DEFAULT {def}"));
            }
            if !c.nullable {
                s.push_str(" NOT NULL");
            }
        }
    }
    s
}

/// SQL Server 기본값 제약 이름 관례.
fn df_name(table: &str, col: &str) -> String {
    format!("DF_{table}_{col}")
}

/// SQL Server: 그 컬럼의 기본값 제약(이름을 모른다)을 찾아 지우는 배치 한 덩어리.
fn mssql_drop_default(schema: &str, table: &str, col: &str) -> String {
    let obj = format!(
        "{}.{}",
        brack(if schema.is_empty() { "dbo" } else { schema }),
        brack(table)
    );
    // `EXEC( )` 안에는 리터럴·변수만 올 수 있다(함수 호출 불가 — 11-27 실서버 "'QUOTENAME' 근처의 구문") → 문장을 변수에 먼저 만든다.
    // 제약이 없으면 `+`가 NULL을 내어 실행하지 않는다.
    format!(
        "DECLARE @nsql_df sysname = (SELECT dc.name FROM sys.default_constraints dc JOIN sys.columns c ON c.object_id = dc.parent_object_id AND c.column_id = dc.parent_column_id WHERE dc.parent_object_id = OBJECT_ID({}) AND c.name = {}); DECLARE @nsql_sql nvarchar(max) = N'ALTER TABLE {} DROP CONSTRAINT ' + QUOTENAME(@nsql_df); IF @nsql_sql IS NOT NULL EXEC(@nsql_sql)",
        nlit(&obj),
        nlit(col),
        obj.replace('\'', "''")
    )
}

fn brack(s: &str) -> String {
    format!("[{}]", s.replace(']', "]]"))
}

/// SQL Server 코멘트(MS_Description) — 원래 있었는지에 따라 추가/수정/삭제.
fn mssql_comment(
    schema: &str,
    table: &str,
    col: Option<&str>,
    from: &str,
    to: &str,
) -> Option<String> {
    let proc_ = match (from.is_empty(), to.is_empty()) {
        (true, true) => return None,
        (true, false) => "sp_addextendedproperty",
        (false, false) => "sp_updateextendedproperty",
        (false, true) => "sp_dropextendedproperty",
    };
    let mut s = format!("EXEC sys.{proc_} @name = N'MS_Description'");
    if !to.is_empty() {
        s.push_str(&format!(", @value = {}", nlit(to)));
    }
    s.push_str(&format!(
        ", @level0type = N'SCHEMA', @level0name = {}, @level1type = N'TABLE', @level1name = {}",
        nlit(if schema.is_empty() { "dbo" } else { schema }),
        nlit(table)
    ));
    if let Some(c) = col {
        s.push_str(&format!(
            ", @level2type = N'COLUMN', @level2name = {}",
            nlit(c)
        ));
    }
    Some(s)
}

fn index_create(d: Dialect, t: &Table, tq: &str, i: &Index) -> String {
    let name = match d {
        Dialect::Oracle | Dialect::Sqlite => qualified(d, &t.schema, &i.name),
        _ => ident(d, &i.name),
    };
    let on = if d == Dialect::Sqlite {
        ident(d, &t.name)
    } else {
        tq.to_string()
    };
    format!(
        "CREATE {}INDEX {name} ON {on} ({})",
        if i.unique { "UNIQUE " } else { "" },
        cols(d, &i.cols)
    )
}

fn index_drop(d: Dialect, t: &Table, tq: &str, name: &str) -> String {
    match d {
        Dialect::Mssql | Dialect::Mysql => format!("DROP INDEX {} ON {tq}", ident(d, name)),
        _ => format!("DROP INDEX {}", qualified(d, &t.schema, name)),
    }
}

struct Out {
    plan: Plan,
}

impl Out {
    fn push(&mut self, sql: String, risk: Risk) {
        self.plan.stmts.push(Stmt { sql, risk });
    }
    fn block(&mut self, what: &str, reason: Reason) {
        let b = Blocked {
            what: what.to_string(),
            reason,
        };
        if !self.plan.blocked.contains(&b) {
            self.plan.blocked.push(b);
        }
    }
}

fn drop_key(d: Dialect, tq: &str, k: &Key, o: &mut Out) {
    let sql = match (d, k.kind) {
        (Dialect::Sqlite, _) => return o.block(&k.name, Reason::NeedsRebuild),
        (Dialect::Mysql, KeyKind::Primary) => format!("ALTER TABLE {tq} DROP PRIMARY KEY"),
        (Dialect::Mysql, KeyKind::Unique) => {
            format!("ALTER TABLE {tq} DROP INDEX {}", ident(d, &k.name))
        }
        (Dialect::Mysql, KeyKind::Foreign) => {
            format!("ALTER TABLE {tq} DROP FOREIGN KEY {}", ident(d, &k.name))
        }
        (Dialect::Mysql, KeyKind::Check) => {
            format!("ALTER TABLE {tq} DROP CHECK {}", ident(d, &k.name))
        }
        _ => format!("ALTER TABLE {tq} DROP CONSTRAINT {}", ident(d, &k.name)),
    };
    o.push(sql, Risk::Warn);
}

fn add_key(d: Dialect, tq: &str, k: &Key, o: &mut Out) {
    if d == Dialect::Sqlite {
        return o.block(&k.name, Reason::NeedsRebuild);
    }
    o.push(
        format!("ALTER TABLE {tq} ADD {}", key_clause(d, k)),
        Risk::Warn,
    );
}

pub(crate) fn plan(d: Dialect, t: &Table, changes: &[Change]) -> Plan {
    let tq = qualified(d, &t.schema, &t.name);
    let mut o = Out {
        plan: Plan::default(),
    };

    // 1) 제약·인덱스 삭제(외래 키 먼저).
    for fk_pass in [true, false] {
        for c in changes {
            if let Change::DropKey(k) = c {
                if (k.kind == KeyKind::Foreign) == fk_pass {
                    drop_key(d, &tq, k, &mut o);
                }
            }
        }
    }
    for c in changes {
        if let Change::DropIndex(i) = c {
            o.push(index_drop(d, t, &tq, &i.name), Risk::Safe);
        }
    }
    // 2) 컬럼 이름.
    for c in changes {
        if let Change::RenameColumn { from, to } = c {
            let sql = match d {
                Dialect::Mssql => format!(
                    "EXEC sp_rename {}, {}, N'COLUMN'",
                    nlit(&format!(
                        "{}.{}.{}",
                        brack(if t.schema.is_empty() {
                            "dbo"
                        } else {
                            &t.schema
                        }),
                        brack(&t.name),
                        brack(from)
                    )),
                    nlit(to)
                ),
                _ => format!(
                    "ALTER TABLE {tq} RENAME COLUMN {} TO {}",
                    ident(d, from),
                    ident(d, to)
                ),
            };
            o.push(sql, Risk::Safe);
        }
    }
    // 3) 컬럼 추가.
    for c in changes {
        if let Change::AddColumn { col, after, at_end } = c {
            let def = col_def(d, col, &t.name);
            let risk = if !col.nullable && col.default.trim().is_empty() {
                Risk::Warn
            } else {
                Risk::Safe
            };
            let sql = match d {
                Dialect::Oracle => format!("ALTER TABLE {tq} ADD ({def})"),
                Dialect::Mssql => format!("ALTER TABLE {tq} ADD {def}"),
                Dialect::Mysql => {
                    let pos = match (at_end, after) {
                        (true, _) => String::new(),
                        (false, Some(a)) => format!(" AFTER {}", ident(d, a)),
                        (false, None) => " FIRST".to_string(),
                    };
                    format!("ALTER TABLE {tq} ADD COLUMN {def}{pos}")
                }
                Dialect::Postgres | Dialect::Sqlite => format!("ALTER TABLE {tq} ADD COLUMN {def}"),
            };
            o.push(sql, risk);
            // 가운데에 넣기는 MySQL만 — 다른 방언은 끝에 더해지고 그 사실을 알린다.
            if !at_end && d != Dialect::Mysql {
                o.block(&col.name, Reason::NeedsRebuild);
            }
        }
    }
    // 4) 타입 · NULL · 기본값.
    alter_columns(d, t, &tq, changes, &mut o);
    // 5) 컬럼 삭제.
    for c in changes {
        if let Change::DropColumn { name, had_default } = c {
            if d == Dialect::Mssql && *had_default {
                o.push(mssql_drop_default(&t.schema, &t.name, name), Risk::Danger);
            }
            o.push(
                format!("ALTER TABLE {tq} DROP COLUMN {}", ident(d, name)),
                Risk::Danger,
            );
        }
    }
    // 6) 제약 이름 · 추가(외래 키 마지막).
    for c in changes {
        if let Change::RenameKey { key, from } = c {
            match d {
                Dialect::Oracle | Dialect::Postgres => o.push(
                    format!(
                        "ALTER TABLE {tq} RENAME CONSTRAINT {} TO {}",
                        ident(d, from),
                        ident(d, &key.name)
                    ),
                    Risk::Safe,
                ),
                Dialect::Mssql => o.push(
                    format!(
                        "EXEC sp_rename {}, {}, N'OBJECT'",
                        nlit(&format!(
                            "{}.{}",
                            brack(if t.schema.is_empty() {
                                "dbo"
                            } else {
                                &t.schema
                            }),
                            brack(from)
                        )),
                        nlit(&key.name)
                    ),
                    Risk::Safe,
                ),
                Dialect::Mysql => {
                    let mut old = key.clone();
                    old.name.clone_from(from);
                    drop_key(d, &tq, &old, &mut o);
                    add_key(d, &tq, key, &mut o);
                }
                Dialect::Sqlite => o.block(&key.name, Reason::NeedsRebuild),
            }
        }
    }
    for fk_pass in [false, true] {
        for c in changes {
            if let Change::AddKey(k) = c {
                if (k.kind == KeyKind::Foreign) == fk_pass {
                    add_key(d, &tq, k, &mut o);
                }
            }
        }
    }
    // 7) 인덱스.
    for c in changes {
        match c {
            Change::RenameIndex { index, from } => match d {
                Dialect::Oracle | Dialect::Postgres => o.push(
                    format!(
                        "ALTER INDEX {} RENAME TO {}",
                        qualified(d, &t.schema, from),
                        ident(d, &index.name)
                    ),
                    Risk::Safe,
                ),
                Dialect::Mssql => o.push(
                    format!(
                        "EXEC sp_rename {}, {}, N'INDEX'",
                        nlit(&format!(
                            "{}.{}.{}",
                            brack(if t.schema.is_empty() {
                                "dbo"
                            } else {
                                &t.schema
                            }),
                            brack(&t.name),
                            brack(from)
                        )),
                        nlit(&index.name)
                    ),
                    Risk::Safe,
                ),
                Dialect::Mysql => o.push(
                    format!(
                        "ALTER TABLE {tq} RENAME INDEX {} TO {}",
                        ident(d, from),
                        ident(d, &index.name)
                    ),
                    Risk::Safe,
                ),
                Dialect::Sqlite => {
                    o.push(index_drop(d, t, &tq, from), Risk::Safe);
                    o.push(index_create(d, t, &tq, index), Risk::Safe);
                }
            },
            Change::AddIndex(i) => o.push(
                index_create(d, t, &tq, i),
                if i.unique { Risk::Warn } else { Risk::Safe },
            ),
            _ => {}
        }
    }
    // 8) 코멘트(MySQL 컬럼 코멘트는 4)의 MODIFY에 들어갔다).
    for c in changes {
        match c {
            Change::ColumnComment { col, from } => comment_col(d, t, &tq, col, from, &mut o),
            Change::AddColumn { col, .. } if !col.comment.is_empty() => {
                comment_col(d, t, &tq, col, "", &mut o)
            }
            Change::TableComment { to, from } => match d {
                Dialect::Oracle => {
                    o.push(format!("COMMENT ON TABLE {tq} IS {}", lit(to)), Risk::Safe)
                }
                Dialect::Postgres => o.push(
                    format!(
                        "COMMENT ON TABLE {tq} IS {}",
                        if to.is_empty() {
                            "NULL".to_string()
                        } else {
                            lit(to)
                        }
                    ),
                    Risk::Safe,
                ),
                Dialect::Mssql => {
                    if let Some(s) = mssql_comment(&t.schema, &t.name, None, from, to) {
                        o.push(s, Risk::Safe);
                    }
                }
                Dialect::Mysql => o.push(
                    format!("ALTER TABLE {tq} COMMENT = {}", lit(to)),
                    Risk::Safe,
                ),
                Dialect::Sqlite => o.block(&t.name, Reason::NotSupported),
            },
            _ => {}
        }
    }
    // 9) 테이블 이름(맨 끝 — 앞의 문장은 모두 옛 이름을 쓴다).
    for c in changes {
        if let Change::RenameTable { to } = c {
            let sql = match d {
                Dialect::Mssql => format!(
                    "EXEC sp_rename {}, {}",
                    nlit(&format!(
                        "{}.{}",
                        brack(if t.schema.is_empty() {
                            "dbo"
                        } else {
                            &t.schema
                        }),
                        brack(&t.name)
                    )),
                    nlit(to)
                ),
                Dialect::Mysql => format!("RENAME TABLE {tq} TO {}", qualified(d, &t.schema, to)),
                _ => format!("ALTER TABLE {tq} RENAME TO {}", ident(d, to)),
            };
            o.push(sql, Risk::Safe);
        }
    }
    o.plan
}

fn comment_col(d: Dialect, t: &Table, tq: &str, col: &Column, from: &str, o: &mut Out) {
    match d {
        Dialect::Oracle => o.push(
            format!(
                "COMMENT ON COLUMN {tq}.{} IS {}",
                ident(d, &col.name),
                lit(&col.comment)
            ),
            Risk::Safe,
        ),
        Dialect::Postgres => o.push(
            format!(
                "COMMENT ON COLUMN {tq}.{} IS {}",
                ident(d, &col.name),
                if col.comment.is_empty() {
                    "NULL".to_string()
                } else {
                    lit(&col.comment)
                }
            ),
            Risk::Safe,
        ),
        Dialect::Mssql => {
            if let Some(s) = mssql_comment(&t.schema, &t.name, Some(&col.name), from, &col.comment)
            {
                o.push(s, Risk::Safe);
            }
        }
        Dialect::Mysql => {} // MODIFY/ADD에 포함
        Dialect::Sqlite => o.block(&col.name, Reason::NotSupported),
    }
}

/// 4단계: 타입 · NULL · 기본값(+ MySQL은 코멘트·순서까지 한 `MODIFY`) · 다른 방언의 순서 변경 = 막힘.
fn alter_columns(d: Dialect, t: &Table, tq: &str, changes: &[Change], o: &mut Out) {
    // 컬럼별로 모은다(등장 순서 유지).
    struct Acc<'a> {
        col: &'a Column,
        ty: Option<&'a str>,
        null: bool,
        default: Option<&'a str>,
        comment: bool,
        mv: Option<&'a Option<String>>,
    }
    let mut accs: Vec<Acc> = Vec::new();
    fn slot<'a, 'b>(accs: &'b mut Vec<Acc<'a>>, col: &'a Column) -> &'b mut Acc<'a> {
        let i = match accs.iter().position(|a| a.col.name == col.name) {
            Some(i) => i,
            None => {
                accs.push(Acc {
                    col,
                    ty: None,
                    null: false,
                    default: None,
                    comment: false,
                    mv: None,
                });
                accs.len() - 1
            }
        };
        &mut accs[i]
    }
    for c in changes {
        match c {
            Change::AlterType { col, from, .. } => slot(&mut accs, col).ty = Some(from),
            Change::AlterNull { col, .. } => slot(&mut accs, col).null = true,
            Change::AlterDefault { col, from, .. } => slot(&mut accs, col).default = Some(from),
            Change::ColumnComment { col, .. } if d == Dialect::Mysql => {
                slot(&mut accs, col).comment = true
            }
            Change::MoveColumn { col, after } => slot(&mut accs, col).mv = Some(after),
            _ => {}
        }
    }
    for a in &accs {
        let c = a.col;
        let name = ident(d, &c.name);
        let ty_risk = a.ty.map(|from| crate::type_change_risk(from, &c.data_type));
        let null_risk = (a.null && !c.nullable).then_some(Risk::Warn);
        if a.mv.is_some() && d != Dialect::Mysql {
            o.block(&c.name, Reason::NeedsRebuild);
        }
        match d {
            Dialect::Sqlite => {
                if a.ty.is_some() || a.null || a.default.is_some() {
                    o.block(&c.name, Reason::NeedsRebuild);
                }
            }
            Dialect::Mysql => {
                let pos = match a.mv {
                    None => String::new(),
                    Some(Some(x)) => format!(" AFTER {}", ident(d, x)),
                    Some(None) => " FIRST".to_string(),
                };
                let risk = ty_risk
                    .into_iter()
                    .chain(null_risk)
                    .max()
                    .unwrap_or(Risk::Safe);
                o.push(
                    format!(
                        "ALTER TABLE {tq} MODIFY COLUMN {}{pos}",
                        col_def(d, c, &t.name)
                    ),
                    risk,
                );
            }
            Dialect::Mssql => {
                if a.ty.is_some() || a.null {
                    let risk = ty_risk
                        .into_iter()
                        .chain(null_risk)
                        .max()
                        .unwrap_or(Risk::Safe);
                    o.push(
                        format!(
                            "ALTER TABLE {tq} ALTER COLUMN {name} {} {}",
                            c.data_type.trim(),
                            if c.nullable { "NULL" } else { "NOT NULL" }
                        ),
                        risk,
                    );
                }
                if let Some(from) = a.default {
                    if !from.trim().is_empty() {
                        o.push(mssql_drop_default(&t.schema, &t.name, &c.name), Risk::Safe);
                    }
                    if !c.default.trim().is_empty() {
                        o.push(
                            format!(
                                "ALTER TABLE {tq} ADD CONSTRAINT {} DEFAULT {} FOR {name}",
                                ident(d, &df_name(&t.name, &c.name)),
                                c.default.trim()
                            ),
                            Risk::Safe,
                        );
                    }
                }
            }
            Dialect::Oracle => {
                if a.ty.is_some() {
                    o.push(
                        format!("ALTER TABLE {tq} MODIFY ({name} {})", c.data_type.trim()),
                        ty_risk.unwrap_or(Risk::Warn),
                    );
                }
                if a.default.is_some() {
                    let v = if c.default.trim().is_empty() {
                        "NULL"
                    } else {
                        c.default.trim()
                    };
                    o.push(
                        format!("ALTER TABLE {tq} MODIFY ({name} DEFAULT {v})"),
                        Risk::Safe,
                    );
                }
                if a.null {
                    o.push(
                        format!(
                            "ALTER TABLE {tq} MODIFY ({name} {})",
                            if c.nullable { "NULL" } else { "NOT NULL" }
                        ),
                        null_risk.unwrap_or(Risk::Safe),
                    );
                }
            }
            Dialect::Postgres => {
                if a.ty.is_some() {
                    o.push(
                        format!(
                            "ALTER TABLE {tq} ALTER COLUMN {name} TYPE {}",
                            c.data_type.trim()
                        ),
                        ty_risk.unwrap_or(Risk::Warn),
                    );
                }
                if a.default.is_some() {
                    let sql = if c.default.trim().is_empty() {
                        format!("ALTER TABLE {tq} ALTER COLUMN {name} DROP DEFAULT")
                    } else {
                        format!(
                            "ALTER TABLE {tq} ALTER COLUMN {name} SET DEFAULT {}",
                            c.default.trim()
                        )
                    };
                    o.push(sql, Risk::Safe);
                }
                if a.null {
                    o.push(
                        format!(
                            "ALTER TABLE {tq} ALTER COLUMN {name} {} NOT NULL",
                            if c.nullable { "DROP" } else { "SET" }
                        ),
                        null_risk.unwrap_or(Risk::Safe),
                    );
                }
            }
        }
    }
}

/// 새 테이블: `CREATE TABLE` 한 문장 + 인덱스 + 코멘트.
#[must_use]
pub fn create_table(d: Dialect, t: &Table) -> Vec<Stmt> {
    let tq = qualified(d, &t.schema, &t.name);
    let mut lines: Vec<String> = t
        .columns
        .iter()
        .map(|c| format!("    {}", col_def(d, c, &t.name)))
        .collect();
    // 외래 키는 뒤에.
    for fk_pass in [false, true] {
        for k in t
            .keys
            .iter()
            .filter(|k| (k.kind == KeyKind::Foreign) == fk_pass)
        {
            lines.push(format!("    {}", key_clause(d, k)));
        }
    }
    let mut sql = format!("CREATE TABLE {tq} (\n{}\n)", lines.join(",\n"));
    if d == Dialect::Mysql && !t.comment.is_empty() {
        sql.push_str(&format!(" COMMENT = {}", lit(&t.comment)));
    }
    let mut o = Out {
        plan: Plan::default(),
    };
    o.push(sql, Risk::Safe);
    for i in &t.indexes {
        o.push(index_create(d, t, &tq, i), Risk::Safe);
    }
    if !t.comment.is_empty() {
        match d {
            Dialect::Oracle | Dialect::Postgres => o.push(
                format!("COMMENT ON TABLE {tq} IS {}", lit(&t.comment)),
                Risk::Safe,
            ),
            Dialect::Mssql => {
                if let Some(s) = mssql_comment(&t.schema, &t.name, None, "", &t.comment) {
                    o.push(s, Risk::Safe);
                }
            }
            Dialect::Mysql | Dialect::Sqlite => {}
        }
    }
    for c in t.columns.iter().filter(|c| !c.comment.is_empty()) {
        if matches!(d, Dialect::Oracle | Dialect::Postgres | Dialect::Mssql) {
            comment_col(d, t, &tq, c, "", &mut o);
        }
    }
    o.plan.stmts
}

/// 문장 목록 → 스크립트 글(SQL Server = `GO` 배치 · 그 밖 = `;`).
#[must_use]
pub fn script(d: Dialect, stmts: &[Stmt]) -> String {
    let mut s = String::new();
    for st in stmts {
        s.push_str(&st.sql);
        s.push_str(if d == Dialect::Mssql { "\nGO\n" } else { ";\n" });
    }
    s
}

/// 타입 완성 목록(자주 쓰는 순).
#[must_use]
pub fn type_names(d: Dialect) -> &'static [&'static str] {
    match d {
        Dialect::Oracle => &[
            "VARCHAR2(100)",
            "NUMBER",
            "NUMBER(10)",
            "NUMBER(18,2)",
            "DATE",
            "TIMESTAMP",
            "CHAR(1)",
            "NVARCHAR2(100)",
            "CLOB",
            "BLOB",
            "INTEGER",
            "FLOAT",
            "TIMESTAMP WITH TIME ZONE",
            "RAW(16)",
        ],
        Dialect::Postgres => &[
            "varchar(100)",
            "text",
            "integer",
            "bigint",
            "numeric(18,2)",
            "boolean",
            "date",
            "timestamp",
            "timestamptz",
            "smallint",
            "real",
            "double precision",
            "uuid",
            "jsonb",
            "bytea",
            "char(1)",
            "serial",
            "bigserial",
        ],
        Dialect::Mssql => &[
            "nvarchar(100)",
            "varchar(100)",
            "int",
            "bigint",
            "decimal(18,2)",
            "bit",
            "date",
            "datetime2",
            "datetime",
            "smallint",
            "tinyint",
            "float",
            "uniqueidentifier",
            "nvarchar(max)",
            "varbinary(max)",
            "char(1)",
            "money",
        ],
        Dialect::Mysql => &[
            "varchar(100)",
            "int",
            "bigint",
            "decimal(18,2)",
            "tinyint(1)",
            "date",
            "datetime",
            "timestamp",
            "text",
            "char(1)",
            "double",
            "json",
            "blob",
            "longtext",
            "smallint",
        ],
        Dialect::Sqlite => &["TEXT", "INTEGER", "REAL", "NUMERIC", "BLOB"],
    }
}
