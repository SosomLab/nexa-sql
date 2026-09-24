//! ★ 객체 탐색기 **트리 표 + 하위 항목 조회**(docs/83 §1 · 09-25 · DBeaver 항해자 대조).
//!
//! - `sub_kinds(dialect, kind)` = 객체 아래 하위 폴더(Columns · Constraints · Foreign Keys · References · Indexes · Triggers · Partitions ·
//!   Dependencies · Rules · Policies · Extended Properties · Arguments · Attributes · Methods · Procedures · Functions) — 탐색기는 이 표를
//!   읽어 노드를 만들 뿐 방언 분기를 갖지 않는다.
//! - `sub_items(s, owner, sub)` = 그 폴더의 잎 목록(`SubItem` = 이름 · 부가 · 아이콘 힌트) — 방언별 사전 뷰 한 질의 · 없는 조합 = 빈 목록.
//!   컬럼은 [`super::columns`](메타 저장소와 공용) · 제약/인덱스는 [`super::table_detail`] · 인자는 [`super::routine_args`] ·
//!   패키지 멤버는 [`super::package_members`]를 그대로 쓴다(단일 원천 · 새 질의는 References·Triggers·Partitions·Dependencies·Rules·
//!   Policies·Extended Properties·인덱스 컬럼·타입 속성/메서드만).

use super::{
    col, lit, package_members, qualified, query, quote_ident, routine_args, table_detail, Dialect,
    ObjectInfo, ObjectKind, Session,
};
use nsql_core::DbError;

/// 객체 아래 하위 폴더 종류.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubKind {
    Columns,
    /// 제약 전부(P·U·R·C) — Oracle·PG·일반.
    Constraints,
    /// SQL Server: PK + UNIQUE.
    UniqueKeys,
    /// SQL Server: CHECK.
    CheckConstraints,
    ForeignKeys,
    /// 이 테이블을 가리키는 외래 키(다른 테이블의 FK).
    References,
    Indexes,
    Triggers,
    Partitions,
    Dependencies,
    /// PG rules.
    Rules,
    /// PG row-level security policies.
    Policies,
    /// SQL Server extended properties.
    ExtendedProperties,
    /// 루틴 인자.
    Arguments,
    /// 타입 속성.
    Attributes,
    /// 타입 메서드(Oracle).
    Methods,
    /// 패키지 멤버 프로시저.
    Procedures,
    /// 패키지 멤버 함수.
    Functions,
}

impl SubKind {
    pub const ALL: [SubKind; 18] = [
        SubKind::Columns,
        SubKind::Constraints,
        SubKind::UniqueKeys,
        SubKind::CheckConstraints,
        SubKind::ForeignKeys,
        SubKind::References,
        SubKind::Indexes,
        SubKind::Triggers,
        SubKind::Partitions,
        SubKind::Dependencies,
        SubKind::Rules,
        SubKind::Policies,
        SubKind::ExtendedProperties,
        SubKind::Arguments,
        SubKind::Attributes,
        SubKind::Methods,
        SubKind::Procedures,
        SubKind::Functions,
    ];

    /// 폴더 라벨(영어 고정 — DBMS 용어는 번역하지 않는다 · 사용자 09-19).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SubKind::Columns => "Columns",
            SubKind::Constraints => "Constraints",
            SubKind::UniqueKeys => "Unique Keys",
            SubKind::CheckConstraints => "Check Constraints",
            SubKind::ForeignKeys => "Foreign Keys",
            SubKind::References => "References",
            SubKind::Indexes => "Indexes",
            SubKind::Triggers => "Triggers",
            SubKind::Partitions => "Partitions",
            SubKind::Dependencies => "Dependencies",
            SubKind::Rules => "Rules",
            SubKind::Policies => "Policies",
            SubKind::ExtendedProperties => "Extended Properties",
            SubKind::Arguments => "Arguments",
            SubKind::Attributes => "Attributes",
            SubKind::Methods => "Methods",
            SubKind::Procedures => "Procedures",
            SubKind::Functions => "Functions",
        }
    }

    /// 코드(CLI `nsql cat sub <종류>` · 기동 명령).
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            SubKind::Columns => "columns",
            SubKind::Constraints => "constraints",
            SubKind::UniqueKeys => "unique_keys",
            SubKind::CheckConstraints => "check_constraints",
            SubKind::ForeignKeys => "foreign_keys",
            SubKind::References => "references",
            SubKind::Indexes => "indexes",
            SubKind::Triggers => "triggers",
            SubKind::Partitions => "partitions",
            SubKind::Dependencies => "dependencies",
            SubKind::Rules => "rules",
            SubKind::Policies => "policies",
            SubKind::ExtendedProperties => "extended_properties",
            SubKind::Arguments => "arguments",
            SubKind::Attributes => "attributes",
            SubKind::Methods => "methods",
            SubKind::Procedures => "procedures",
            SubKind::Functions => "functions",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<SubKind> {
        let s = s.trim().to_ascii_lowercase().replace([' ', '-'], "_");
        SubKind::ALL
            .iter()
            .copied()
            .find(|k| k.code() == s || k.label().to_ascii_lowercase().replace(' ', "_") == s)
    }
}

/// 잎 항목의 아이콘 힌트(호스트가 아이콘 종류로 맞춘다).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubIcon {
    Column,
    /// PK·UNIQUE.
    Key,
    Check,
    ForeignKey,
    Reference,
    Index,
    Trigger,
    Partition,
    Dependency,
    Rule,
    Policy,
    Property,
    Argument,
    Attribute,
    Method,
    Procedure,
    Function,
}

/// 하위 폴더의 잎 한 줄.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubItem {
    pub name: String,
    /// 흐린 부가(컬럼 목록 · 타입 · 대상 테이블 …).
    pub detail: String,
    pub icon: SubIcon,
    /// 상태(`DISABLED` · `INVALID` … · 없으면 빈).
    pub status: String,
}

fn item(name: impl Into<String>, detail: impl Into<String>, icon: SubIcon) -> SubItem {
    SubItem {
        name: name.into(),
        detail: detail.into(),
        icon,
        status: String::new(),
    }
}

/// 객체 아래 하위 폴더(83 §1 표 · 빈 목록 = 펼칠 수 없음).
#[must_use]
pub fn sub_kinds(dialect: Dialect, kind: ObjectKind) -> &'static [SubKind] {
    use SubKind::*;
    match (dialect, kind) {
        (Dialect::Oracle, ObjectKind::Table) => &[
            Columns,
            Constraints,
            ForeignKeys,
            References,
            Triggers,
            Indexes,
            Partitions,
            Dependencies,
        ],
        (Dialect::Mssql, ObjectKind::Table) => &[
            Columns,
            UniqueKeys,
            CheckConstraints,
            ForeignKeys,
            Indexes,
            References,
            Triggers,
            ExtendedProperties,
        ],
        (Dialect::Postgres, ObjectKind::Table) => &[
            Columns,
            Constraints,
            ForeignKeys,
            Indexes,
            Dependencies,
            References,
            Partitions,
            Triggers,
            Rules,
            Policies,
        ],
        (Dialect::Odbc, ObjectKind::Table) => &[Columns],
        (_, ObjectKind::Table) => &[Columns, Constraints, ForeignKeys, Indexes, Triggers],
        (Dialect::Mssql, ObjectKind::ExternalTable) => &[Columns],
        (Dialect::Postgres, ObjectKind::ForeignTable) => &[Columns, Constraints],
        (Dialect::Oracle, ObjectKind::View) => &[Columns, Constraints, Triggers, Dependencies],
        (Dialect::Mssql, ObjectKind::View) => &[Columns, Triggers, ExtendedProperties],
        (Dialect::Postgres, ObjectKind::View) => &[Columns, Dependencies, Triggers, Rules],
        (_, ObjectKind::View) => &[Columns],
        (Dialect::Oracle, ObjectKind::MaterializedView) => {
            &[Columns, Constraints, Indexes, Dependencies]
        }
        (Dialect::Postgres, ObjectKind::MaterializedView) => &[Columns, Indexes, Dependencies],
        (_, ObjectKind::MaterializedView) => &[Columns],
        (Dialect::Odbc, ObjectKind::Index) => &[],
        (_, ObjectKind::Index) => &[Columns],
        (Dialect::Oracle, ObjectKind::Type) => &[Attributes, Methods],
        (Dialect::Postgres, ObjectKind::Type) => &[Attributes],
        (Dialect::Oracle, ObjectKind::Package) => &[Procedures, Functions, Dependencies],
        (Dialect::Oracle, ObjectKind::Procedure | ObjectKind::Function) => {
            &[Arguments, Dependencies]
        }
        (
            Dialect::Mssql | Dialect::Postgres | Dialect::Mysql,
            ObjectKind::Procedure | ObjectKind::Function,
        ) => &[Arguments],
        (Dialect::Postgres, ObjectKind::Aggregate) => &[Arguments],
        _ => &[],
    }
}

fn kind_char_label(k: char) -> &'static str {
    match k {
        'P' => "PRIMARY KEY",
        'U' => "UNIQUE",
        'R' => "FOREIGN KEY",
        _ => "CHECK",
    }
}

fn key_icon(k: char) -> SubIcon {
    match k {
        'P' | 'U' => SubIcon::Key,
        'R' => SubIcon::ForeignKey,
        _ => SubIcon::Check,
    }
}

/// 하위 폴더의 잎 목록 — 방언별 사전 뷰 · 없는 조합은 빈 목록(오류 아님).
pub fn sub_items(
    s: &mut dyn Session,
    owner: &ObjectInfo,
    sub: SubKind,
) -> Result<Vec<SubItem>, DbError> {
    let d = s.dialect();
    let (schema, name) = (owner.schema.as_str(), owner.name.as_str());
    match sub {
        SubKind::Columns if owner.kind == ObjectKind::Index => index_columns(s, d, schema, name),
        SubKind::Columns => Ok(super::columns(s, schema, name)?
            .into_iter()
            .map(|c| {
                item(
                    c.name,
                    format!(
                        "{}{}",
                        c.data_type,
                        if c.nullable { "" } else { " NOT NULL" }
                    ),
                    SubIcon::Column,
                )
            })
            .collect()),
        SubKind::Constraints
        | SubKind::UniqueKeys
        | SubKind::CheckConstraints
        | SubKind::ForeignKeys => {
            let keep: &[char] = match sub {
                SubKind::Constraints => &['P', 'U', 'R', 'C'],
                SubKind::UniqueKeys => &['P', 'U'],
                SubKind::CheckConstraints => &['C'],
                _ => &['R'],
            };
            let det = table_detail(s, schema, name)?;
            Ok(det
                .keys
                .into_iter()
                .filter(|k| keep.contains(&k.kind))
                .map(|k| {
                    let mut detail = format!("{} ({})", kind_char_label(k.kind), k.cols.join(", "));
                    if let Some(r) = &k.ref_table {
                        detail.push_str(" → ");
                        detail.push_str(r);
                    }
                    item(k.name, detail, key_icon(k.kind))
                })
                .collect())
        }
        SubKind::Indexes => {
            let det = table_detail(s, schema, name)?;
            Ok(det
                .indexes
                .into_iter()
                .map(|i| {
                    item(
                        i.name,
                        format!(
                            "{}({})",
                            if i.unique { "UNIQUE " } else { "" },
                            i.cols.join(", ")
                        ),
                        SubIcon::Index,
                    )
                })
                .collect())
        }
        SubKind::References => references(s, d, schema, name),
        SubKind::Triggers => triggers(s, d, schema, name),
        SubKind::Partitions => partitions(s, d, schema, name),
        SubKind::Dependencies => dependencies(s, d, schema, name),
        SubKind::Rules => rules(s, d, schema, name),
        SubKind::Policies => policies(s, d, schema, name),
        SubKind::ExtendedProperties => extended_properties(s, d, schema, name),
        SubKind::Arguments => {
            let call = if d == Dialect::Postgres && !owner.extra.is_empty() {
                format!(
                    "{schema}.{name}({})",
                    owner.extra.trim_start_matches("SETOF ")
                )
            } else {
                format!("{schema}.{name}")
            };
            Ok(routine_args(s, &call)?
                .into_iter()
                .filter(|a| !a.name.is_empty())
                .map(|a| {
                    let ov = if matches!(a.overload.as_str(), "" | "0" | "1") {
                        String::new()
                    } else {
                        format!(" #{}", a.overload)
                    };
                    item(
                        a.name,
                        format!("{} {}{ov}", a.in_out, a.data_type),
                        SubIcon::Argument,
                    )
                })
                .collect())
        }
        SubKind::Attributes => attributes(s, d, schema, name),
        SubKind::Methods => methods(s, d, schema, name),
        SubKind::Procedures | SubKind::Functions => {
            let want_proc = sub == SubKind::Procedures;
            Ok(package_members(s, schema, name)?
                .into_iter()
                .filter(|m| {
                    let is_proc = m.data_type.eq_ignore_ascii_case("PROCEDURE");
                    is_proc == want_proc
                })
                .map(|m| {
                    let icon = if want_proc {
                        SubIcon::Procedure
                    } else {
                        SubIcon::Function
                    };
                    item(
                        m.name,
                        if want_proc {
                            String::new()
                        } else {
                            m.data_type
                        },
                        icon,
                    )
                })
                .collect())
        }
    }
}

fn rows_to_items(s: &mut dyn Session, sql: &str, icon: SubIcon) -> Result<Vec<SubItem>, DbError> {
    let rs = query(s, sql)?;
    Ok(rs
        .rows
        .iter()
        .map(|r| {
            let mut it = item(col(r, 0), col(r, 1), icon);
            if r.len() > 2 {
                it.status = col(r, 2);
            }
            it
        })
        .collect())
}

pub(super) fn index_columns(
    s: &mut dyn Session,
    d: Dialect,
    schema: &str,
    name: &str,
) -> Result<Vec<SubItem>, DbError> {
    let sql = match d {
        Dialect::Oracle => format!(
            "SELECT column_name, DECODE(descend, 'DESC', 'DESC', '') FROM all_ind_columns WHERE index_owner = {} AND index_name = {} ORDER BY column_position",
            lit(schema), lit(name)
        ),
        Dialect::Mssql => format!(
            "SELECT c.name, CASE WHEN ic.is_descending_key = 1 THEN 'DESC' ELSE '' END + CASE WHEN ic.is_included_column = 1 THEN 'INCLUDE' ELSE '' END FROM sys.indexes i JOIN sys.objects o ON o.object_id = i.object_id JOIN sys.schemas sc ON sc.schema_id = o.schema_id JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id WHERE sc.name = {} AND i.name = {} ORDER BY ic.is_included_column, ic.key_ordinal",
            lit(schema), lit(name)
        ),
        Dialect::Postgres => format!(
            "SELECT pg_get_indexdef(i.indexrelid, k.ord::int, true), '' FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid JOIN pg_namespace n ON n.oid = c.relnamespace CROSS JOIN LATERAL generate_series(1, i.indnatts) AS k(ord) WHERE n.nspname = {} AND c.relname = {} ORDER BY k.ord",
            lit(schema), lit(name)
        ),
        Dialect::Sqlite => {
            let rs = query(s, &format!("PRAGMA index_info({})", quote_ident(d, name)))?;
            return Ok(rs.rows.iter().map(|r| item(col(r, 2), "", SubIcon::Column)).collect());
        }
        Dialect::Mysql => format!(
            "SELECT column_name, IFNULL(collation, '') FROM information_schema.statistics WHERE table_schema = {} AND index_name = {} ORDER BY seq_in_index",
            lit(schema), lit(name)
        ),
        Dialect::Odbc => return Ok(Vec::new()),
    };
    rows_to_items(s, &sql, SubIcon::Column)
}

fn references(
    s: &mut dyn Session,
    d: Dialect,
    schema: &str,
    name: &str,
) -> Result<Vec<SubItem>, DbError> {
    let sql = match d {
        Dialect::Oracle => format!(
            "SELECT c.constraint_name, c.owner || '.' || c.table_name FROM all_constraints c WHERE c.constraint_type = 'R' AND (c.r_owner, c.r_constraint_name) IN (SELECT owner, constraint_name FROM all_constraints WHERE owner = {} AND table_name = {} AND constraint_type IN ('P','U')) ORDER BY c.owner, c.table_name, c.constraint_name",
            lit(schema), lit(name)
        ),
        Dialect::Mssql => format!(
            "SELECT fk.name, SCHEMA_NAME(o.schema_id) + '.' + o.name FROM sys.foreign_keys fk JOIN sys.objects o ON o.object_id = fk.parent_object_id WHERE fk.referenced_object_id = OBJECT_ID({}) ORDER BY o.name, fk.name",
            lit(&qualified(d, schema, name))
        ),
        Dialect::Postgres => format!(
            "SELECT con.conname, n.nspname || '.' || c.relname FROM pg_constraint con JOIN pg_class c ON c.oid = con.conrelid JOIN pg_namespace n ON n.oid = c.relnamespace WHERE con.contype = 'f' AND con.confrelid = {}::regclass ORDER BY n.nspname, c.relname, con.conname",
            lit(&qualified(d, schema, name))
        ),
        Dialect::Mysql => format!(
            "SELECT constraint_name, constraint_schema || '.' || table_name FROM information_schema.referential_constraints WHERE unique_constraint_schema = {} AND referenced_table_name = {} ORDER BY table_name, constraint_name",
            lit(schema), lit(name)
        ),
        Dialect::Sqlite | Dialect::Odbc => return Ok(Vec::new()),
    };
    rows_to_items(s, &sql, SubIcon::Reference)
}

fn triggers(
    s: &mut dyn Session,
    d: Dialect,
    schema: &str,
    name: &str,
) -> Result<Vec<SubItem>, DbError> {
    let sql = match d {
        Dialect::Oracle => format!(
            "SELECT trigger_name, trigger_type || ' ' || triggering_event, DECODE(status, 'DISABLED', 'DISABLED', '') FROM all_triggers WHERE table_owner = {} AND table_name = {} ORDER BY trigger_name",
            lit(schema), lit(name)
        ),
        Dialect::Mssql => format!(
            "SELECT t.name, CASE WHEN t.is_instead_of_trigger = 1 THEN 'INSTEAD OF ' ELSE 'AFTER ' END + STUFF((SELECT ', ' + te.type_desc FROM sys.trigger_events te WHERE te.object_id = t.object_id FOR XML PATH('')), 1, 2, ''), CASE WHEN t.is_disabled = 1 THEN 'DISABLED' ELSE '' END FROM sys.triggers t WHERE t.parent_id = OBJECT_ID({}) ORDER BY t.name",
            lit(&qualified(d, schema, name))
        ),
        Dialect::Postgres => format!(
            "SELECT t.tgname, regexp_replace(pg_get_triggerdef(t.oid, true), '^CREATE (CONSTRAINT )?TRIGGER \\S+ ', ''), CASE WHEN t.tgenabled = 'D' THEN 'DISABLED' ELSE '' END FROM pg_trigger t WHERE t.tgrelid = {}::regclass AND NOT t.tgisinternal ORDER BY t.tgname",
            lit(&qualified(d, schema, name))
        ),
        Dialect::Mysql => format!(
            "SELECT trigger_name, CONCAT(action_timing, ' ', event_manipulation), '' FROM information_schema.triggers WHERE event_object_schema = {} AND event_object_table = {} ORDER BY trigger_name",
            lit(schema), lit(name)
        ),
        Dialect::Sqlite => format!(
            "SELECT name, '', '' FROM sqlite_master WHERE type = 'trigger' AND tbl_name = {} ORDER BY name",
            lit(name)
        ),
        Dialect::Odbc => return Ok(Vec::new()),
    };
    rows_to_items(s, &sql, SubIcon::Trigger)
}

fn partitions(
    s: &mut dyn Session,
    d: Dialect,
    schema: &str,
    name: &str,
) -> Result<Vec<SubItem>, DbError> {
    let sql = match d {
        Dialect::Oracle => format!(
            // `HIGH_VALUE`는 LONG(문자 함수 불가 · ORA-00932) → 테이블스페이스·행 수만.
            "SELECT partition_name, NVL(tablespace_name, '') || NVL2(num_rows, ' · ' || num_rows || ' rows', '') FROM all_tab_partitions WHERE table_owner = {} AND table_name = {} ORDER BY partition_position",
            lit(schema), lit(name)
        ),
        Dialect::Postgres => format!(
            "SELECT c.relname, COALESCE(pg_get_expr(c.relpartbound, c.oid, true), '') FROM pg_inherits i JOIN pg_class c ON c.oid = i.inhrelid WHERE i.inhparent = {}::regclass ORDER BY c.relname",
            lit(&qualified(d, schema, name))
        ),
        _ => return Ok(Vec::new()),
    };
    rows_to_items(s, &sql, SubIcon::Partition)
}

fn dependencies(
    s: &mut dyn Session,
    d: Dialect,
    schema: &str,
    name: &str,
) -> Result<Vec<SubItem>, DbError> {
    let sql = match d {
        // 이 객체가 **참조하는** 것(DBeaver Dependencies 폴더 = referenced) — 참조되는 쪽(References)은 테이블만 FK로.
        Dialect::Oracle => format!(
            "SELECT DISTINCT referenced_owner || '.' || referenced_name, referenced_type FROM all_dependencies WHERE owner = {} AND name = {} AND referenced_owner NOT IN ('SYS','PUBLIC') AND NOT (referenced_owner = owner AND referenced_name = name) ORDER BY 2, 1",
            lit(schema), lit(name)
        ),
        // PG: 뷰/규칙이 참조하는 관계(pg_rewrite 경유 · 자기 자신 제외) — 2차에서 pg_depend 전체로(T-206).
        Dialect::Postgres => format!(
            "SELECT DISTINCT n2.nspname || '.' || c2.relname, c2.relkind::text FROM pg_depend dep JOIN pg_rewrite rw ON rw.oid = dep.objid AND dep.classid = 'pg_rewrite'::regclass JOIN pg_class c2 ON c2.oid = dep.refobjid JOIN pg_namespace n2 ON n2.oid = c2.relnamespace WHERE rw.ev_class = {}::regclass AND c2.oid <> rw.ev_class ORDER BY 1",
            lit(&qualified(d, schema, name))
        ),
        _ => return Ok(Vec::new()),
    };
    rows_to_items(s, &sql, SubIcon::Dependency)
}

fn rules(
    s: &mut dyn Session,
    d: Dialect,
    schema: &str,
    name: &str,
) -> Result<Vec<SubItem>, DbError> {
    if d != Dialect::Postgres {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT rulename, regexp_replace(definition, '\\s+', ' ', 'g') FROM pg_rules WHERE schemaname = {} AND tablename = {} ORDER BY rulename",
        lit(schema),
        lit(name)
    );
    rows_to_items(s, &sql, SubIcon::Rule)
}

fn policies(
    s: &mut dyn Session,
    d: Dialect,
    schema: &str,
    name: &str,
) -> Result<Vec<SubItem>, DbError> {
    if d != Dialect::Postgres {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT policyname, cmd || ' · ' || array_to_string(roles, ', ') || CASE WHEN permissive = 'PERMISSIVE' THEN '' ELSE ' · RESTRICTIVE' END FROM pg_policies WHERE schemaname = {} AND tablename = {} ORDER BY policyname",
        lit(schema),
        lit(name)
    );
    rows_to_items(s, &sql, SubIcon::Policy)
}

fn extended_properties(
    s: &mut dyn Session,
    d: Dialect,
    schema: &str,
    name: &str,
) -> Result<Vec<SubItem>, DbError> {
    if d != Dialect::Mssql {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT ep.name, CONVERT(nvarchar(200), ep.value) FROM sys.extended_properties ep WHERE ep.major_id = OBJECT_ID({}) AND ep.minor_id = 0 AND ep.class = 1 ORDER BY ep.name",
        lit(&qualified(d, schema, name))
    );
    rows_to_items(s, &sql, SubIcon::Property)
}

pub(super) fn attributes(
    s: &mut dyn Session,
    d: Dialect,
    schema: &str,
    name: &str,
) -> Result<Vec<SubItem>, DbError> {
    let sql = match d {
        Dialect::Oracle => format!(
            "SELECT attr_name, attr_type_name || CASE WHEN length IS NOT NULL THEN '(' || length || ')' WHEN precision IS NOT NULL THEN '(' || precision || NVL2(scale, ',' || scale, '') || ')' ELSE '' END FROM all_type_attrs WHERE owner = {} AND type_name = {} ORDER BY attr_no",
            lit(schema), lit(name)
        ),
        Dialect::Postgres => format!(
            "SELECT a.attname, format_type(a.atttypid, a.atttypmod) FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace JOIN pg_attribute a ON a.attrelid = t.typrelid WHERE n.nspname = {} AND t.typname = {} AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum",
            lit(schema), lit(name)
        ),
        _ => return Ok(Vec::new()),
    };
    rows_to_items(s, &sql, SubIcon::Attribute)
}

fn methods(
    s: &mut dyn Session,
    d: Dialect,
    schema: &str,
    name: &str,
) -> Result<Vec<SubItem>, DbError> {
    if d != Dialect::Oracle {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT method_name, method_type || DECODE(final, 'YES', ' FINAL', '') FROM all_type_methods WHERE owner = {} AND type_name = {} ORDER BY method_no",
        lit(schema),
        lit(name)
    );
    rows_to_items(s, &sql, SubIcon::Method)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 83 §1 표 — 방언별 테이블 하위 폴더 · 없는 조합은 빈 목록 · 모든 하위 종류는 코드로 왕복.
    #[test]
    fn sub_kinds_follow_the_dbeaver_table() {
        assert_eq!(sub_kinds(Dialect::Oracle, ObjectKind::Table).len(), 8);
        assert_eq!(
            sub_kinds(Dialect::Mssql, ObjectKind::Table)[1],
            SubKind::UniqueKeys
        );
        assert!(sub_kinds(Dialect::Postgres, ObjectKind::Table).contains(&SubKind::Policies));
        assert_eq!(
            sub_kinds(Dialect::Sqlite, ObjectKind::Table),
            &[
                SubKind::Columns,
                SubKind::Constraints,
                SubKind::ForeignKeys,
                SubKind::Indexes,
                SubKind::Triggers
            ]
        );
        assert!(sub_kinds(Dialect::Mssql, ObjectKind::Sequence).is_empty());
        assert!(sub_kinds(Dialect::Oracle, ObjectKind::Package).contains(&SubKind::Functions));
        for k in SubKind::ALL {
            assert_eq!(SubKind::parse(k.code()), Some(k));
            assert_eq!(SubKind::parse(k.label()), Some(k));
        }
    }
}
