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
    ObjectInfo, ObjectKind, RoutineArg, Session,
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
    /// 연결된 서버의 카탈로그(T-272 · SQL Server `sp_catalogs`).
    Catalogs,
    /// 연결된 서버 카탈로그의 테이블·뷰(T-272 · `sp_tables_ex` · 잎의 부가 = 4부 이름).
    LinkedTables,
}

impl SubKind {
    pub const ALL: [SubKind; 20] = [
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
        SubKind::Catalogs,
        SubKind::LinkedTables,
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
            SubKind::Catalogs => "Catalogs",
            SubKind::LinkedTables => "Tables",
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
            SubKind::Catalogs => "catalogs",
            SubKind::LinkedTables => "linked_tables",
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
        (
            Dialect::Odbc | Dialect::Duckdb | Dialect::Clickhouse | Dialect::Access,
            ObjectKind::Table,
        ) => &[Columns],
        // Db2(10-24 · D-359): SYSCAT로 제약·인덱스·참조·트리거까지.
        (Dialect::Db2, ObjectKind::Table) => &[
            Columns,
            Constraints,
            ForeignKeys,
            Indexes,
            References,
            Triggers,
        ],
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
        (
            Dialect::Odbc | Dialect::Duckdb | Dialect::Clickhouse | Dialect::Access,
            ObjectKind::Index,
        ) => &[],
        (_, ObjectKind::Index) => &[Columns],
        // ★ 연결된 서버(T-272 · 11-21 · 101 §5): 펼침 = 카탈로그(`sp_catalogs`) → 카탈로그 잎 = 테이블(`sp_tables_ex`).
        (Dialect::Mssql, ObjectKind::LinkedServer) => &[Catalogs],
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

/// ★ 패키지 멤버 잎(Procedures/Functions 폴더의 항목)이 가진 하위 폴더(사용자 09-29 "패키지의 프로시저도 개별 프로시저처럼").
/// Oracle 패키지 멤버 = Arguments(의존은 패키지 단위라 뺀다) · 그 밖의 잎 = 없음.
#[must_use]
pub fn member_sub_kinds(dialect: Dialect, sub: SubKind) -> &'static [SubKind] {
    match (dialect, sub) {
        (Dialect::Oracle, SubKind::Procedures | SubKind::Functions) => &[SubKind::Arguments],
        // 연결된 서버의 카탈로그 잎 = 그 카탈로그의 테이블(T-272).
        (Dialect::Mssql, SubKind::Catalogs) => &[SubKind::LinkedTables],
        _ => &[],
    }
}

/// 패키지 멤버 잎을 하위 폴더의 주인 객체로(이름 `패키지.멤버` · `routine_args`가 3부 이름을 패키지로 푼다).
#[must_use]
pub fn member_object(owner: &ObjectInfo, sub: SubKind, item: &SubItem) -> Option<ObjectInfo> {
    // ★ 연결된 서버의 카탈로그 잎(T-272): 주인 = 그 서버 · **`schema` 자리에 카탈로그 이름**을 싣는다([`linked_tables`]가 읽는다).
    if sub == SubKind::Catalogs {
        return (owner.kind == ObjectKind::LinkedServer).then(|| ObjectInfo {
            db: String::new(),
            schema: item.name.clone(),
            name: owner.name.clone(),
            kind: ObjectKind::LinkedServer,
            status: String::new(),
            modified: String::new(),
            extra: String::new(),
        });
    }
    let kind = match sub {
        SubKind::Procedures => ObjectKind::Procedure,
        SubKind::Functions => ObjectKind::Function,
        _ => return None,
    };
    if owner.kind != ObjectKind::Package {
        return None;
    }
    Some(ObjectInfo {
        db: String::new(),
        schema: owner.schema.clone(),
        name: format!("{}.{}", owner.name, item.name),
        kind,
        status: item.status.clone(),
        modified: String::new(),
        extra: item.detail.clone(),
    })
}

/// `sp_catalogs` 호출문(순수 · 서버 이름은 리터럴로 — 이름에 든 `'`는 겹쳐 쓴다).
#[must_use]
pub fn linked_catalogs_sql(server: &str) -> String {
    format!("EXEC sp_catalogs @server_name = N{}", super::lit(server))
}

/// `sp_tables_ex` 호출문(순수) — 그 카탈로그의 테이블·뷰 전부(유형은 받은 뒤 거른다 · 공급자마다 유형 글이 달라서).
#[must_use]
pub fn linked_tables_sql(server: &str, catalog: &str) -> String {
    format!(
        "EXEC sp_tables_ex @table_server = N{}, @table_catalog = N{}",
        super::lit(server),
        super::lit(catalog)
    )
}

/// 4부 이름 `[서버].[카탈로그].[스키마].[테이블]`(순수 · 빈 스키마는 빈 칸 그대로 — `[s].[c]..[t]`).
#[must_use]
pub fn four_part_name(server: &str, catalog: &str, schema: &str, table: &str) -> String {
    let q = |x: &str| super::quote_ident(Dialect::Mssql, x);
    let sch = if schema.is_empty() {
        String::new()
    } else {
        q(schema)
    };
    format!("{}.{}.{}.{}", q(server), q(catalog), sch, q(table))
}

/// ★ 연결된 서버의 카탈로그 목록(T-272 · 101 §5 · `sp_catalogs` = CATALOG_NAME · DESCRIPTION). 실패(권한 · 공급자가 카탈로그를
/// 주지 않음 · 원격 접속 실패)는 **오류 그대로** 올린다 → 탐색기가 폴더에 `⚠ 메시지`로 보인다(빈 폴더 + ⚠).
fn linked_catalogs(s: &mut dyn Session, d: Dialect, server: &str) -> Result<Vec<SubItem>, DbError> {
    if d != Dialect::Mssql {
        return Ok(Vec::new());
    }
    Ok(super::query(s, &linked_catalogs_sql(server))?
        .rows
        .iter()
        .map(|r| item(super::col(r, 0), super::col(r, 1), SubIcon::Property))
        .filter(|it| !it.name.is_empty())
        .collect())
}

/// ★ 연결된 서버 카탈로그의 테이블·뷰(T-272 · `sp_tables_ex` = TABLE_CAT · TABLE_SCHEM · TABLE_NAME · TABLE_TYPE · REMARKS).
/// 잎 = `스키마.이름` · 부가 = **4부 이름**(그대로 질의에 쓸 수 있다) · 시스템 테이블은 뺀다.
fn linked_tables(
    s: &mut dyn Session,
    d: Dialect,
    server: &str,
    catalog: &str,
) -> Result<Vec<SubItem>, DbError> {
    if d != Dialect::Mssql {
        return Ok(Vec::new());
    }
    Ok(super::query(s, &linked_tables_sql(server, catalog))?
        .rows
        .iter()
        .filter_map(|r| {
            linked_table_item(
                server,
                catalog,
                &super::col(r, 1),
                &super::col(r, 2),
                &super::col(r, 3),
            )
        })
        .collect())
}

/// `sp_tables_ex` 한 행 → 잎(순수 · 시험): 이름 없는 행·시스템 유형은 `None` · 뷰는 상태 칸에 `VIEW`.
fn linked_table_item(
    server: &str,
    catalog: &str,
    schema: &str,
    table: &str,
    ty: &str,
) -> Option<SubItem> {
    let ty = ty.trim().to_ascii_uppercase();
    if table.is_empty() || ty.starts_with("SYSTEM") {
        return None;
    }
    let name = if schema.is_empty() {
        table.to_string()
    } else {
        format!("{schema}.{table}")
    };
    let mut it = item(
        name,
        four_part_name(server, catalog, schema, table),
        SubIcon::Reference,
    );
    if ty == "VIEW" {
        it.status = ty;
    }
    Some(it)
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
                        if !k.ref_cols.is_empty() {
                            detail.push_str(&format!(" ({})", k.ref_cols.join(", ")));
                        }
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
        SubKind::Catalogs => linked_catalogs(s, d, name),
        SubKind::LinkedTables => linked_tables(s, d, name, schema),
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
            let args = routine_args(s, &call)?;
            // ★ Oracle 패키지 멤버는 **그 멤버의 오버로드 번호**(`member_detail`이 흐린 글에 적은 `#n` → `member_overload`)의 인자만 —
            //   종전엔 이름이 같은 두 멤버(`PRC_RUN_ALL` ×2)가 두 오버로드의 인자를 합쳐 똑같이 보였다(사용자 10-10).
            let want_ov = if d == Dialect::Oracle {
                member_overload(&owner.extra).map(str::to_string)
            } else {
                None
            };
            let args: Vec<RoutineArg> = args
                .into_iter()
                .filter(|a| want_ov.as_deref().is_none_or(|w| a.overload == w))
                .collect();
            // 오버로드 꼬리 `#n`은 **오버로드가 둘 이상일 때만**(Oracle 번호) — PG `overload` = oid · SQL Server = object_id라 늘 붙던 꼬리
            //   `#718165`/`#1760061356`(E2E 10-08).
            let overloads: std::collections::HashSet<&str> =
                args.iter().map(|a| a.overload.as_str()).collect();
            let multi = overloads.len() > 1;
            Ok(args
                .into_iter()
                // 거르는 것은 **함수 반환값 행**(Oracle `position 0` · 이름 없음)뿐 — 종전 "이름 없으면 버림"은 PG의 무명 인자
                //   (`CREATE FUNCTION f(int)` · `abs(bigint)`)까지 지워 Arguments (0)으로 보였다(협업 V1 10-10 #15-b).
                .filter(|a| !(a.position == 0 && a.name.is_empty()))
                .map(|a| {
                    let ov = if multi && !matches!(a.overload.as_str(), "" | "0" | "1") {
                        format!(" #{}", a.overload)
                    } else {
                        String::new()
                    };
                    // 무명 인자 = `$n`(PG 관례 · 자리 번호).
                    let name = if a.name.is_empty() {
                        format!("${}", a.position)
                    } else {
                        a.name
                    };
                    item(
                        name,
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
            let icon = if want_proc {
                SubIcon::Procedure
            } else {
                SubIcon::Function
            };
            let public = package_members(s, schema, name)?;
            // ★ 본문 전용(비공개) 멤버(사용자 11-25 "구현이 Body에 반영되어 목록에 보이지 않는 버그"): `ALL_PROCEDURES`는 스펙에
            //   선언된 것만 준다 → 본문 소스(`ALL_SOURCE`)를 읽어 아웃라인으로 뽑아 덧붙인다. 본문이 없거나 읽을 권한이 없거나
            //   감싸진(wrapped) 본문이면 공개 멤버만(실패를 목록 오류로 올리지 않는다). 폴더를 펼칠 때만 읽는다(지연 로딩).
            let body = super::source_with(
                s,
                schema,
                super::ObjectKind::PackageBody,
                name,
                super::SourceOpts { qualify: false },
            )
            .unwrap_or_default();
            let public_names: Vec<&str> = public.iter().map(|m| m.name.as_str()).collect();
            let private = private_members(&body, &public_names);
            let mut out: Vec<SubItem> = public
                .iter()
                .filter(|m| m.data_type.eq_ignore_ascii_case("PROCEDURE") == want_proc)
                .map(|m| {
                    item(
                        m.name.clone(),
                        member_detail(want_proc, &m.data_type, &m.overload),
                        icon,
                    )
                })
                .collect();
            out.extend(
                private
                    .into_iter()
                    .filter(|(_, is_proc)| *is_proc == want_proc)
                    .map(|(n, _)| SubItem {
                        status: PRIVATE_MEMBER.to_string(),
                        ..item(n, "", icon)
                    }),
            );
            Ok(out)
        }
    }
}

/// 본문 전용(비공개) 패키지 멤버 잎의 `status` 값 — 밖에서 부를 수 없고 `ALL_ARGUMENTS`에 인자가 없다(탐색기는 이 잎을 펼치지 않는다).
pub const PRIVATE_MEMBER: &str = "private";

/// 패키지 **본문 소스**에서 본문 전용 멤버를 뽑는다(순수 · 11-25): 패키지 바로 아래의 `PROCEDURE`/`FUNCTION`(전방 선언 · 구현 모두 ·
/// 서브프로그램 안의 로컬 것은 제외) 중 공개 이름(`public` · 대소문자 무시)에 없는 것 — 이름은 대문자로, 같은 이름은 한 번만(나온 순서).
/// 반환 = (이름, 프로시저인가).
#[must_use]
pub fn private_members(body_src: &str, public: &[&str]) -> Vec<(String, bool)> {
    use nsql_script::outline::{outline, SymKind};
    let o = outline(body_src, Some(nsql_core::Dialect::Oracle));
    let Some(pkg) = o
        .symbols
        .iter()
        .position(|s| s.kind == SymKind::PackageBody)
    else {
        return Vec::new();
    };
    let mut out: Vec<(String, bool)> = Vec::new();
    for s in &o.symbols {
        let is_proc = match s.kind {
            SymKind::Procedure => true,
            SymKind::Function => false,
            _ => continue,
        };
        if s.parent != Some(pkg) {
            continue;
        }
        let n = s.name.trim_matches('"').to_uppercase();
        if public.iter().any(|p| p.eq_ignore_ascii_case(&n)) || out.iter().any(|(x, _)| *x == n) {
            continue;
        }
        out.push((n, is_proc));
    }
    out
}

/// 패키지 멤버 잎의 흐린 글(10-10): 프로시저 = 빈 글 또는 `#n` · 함수 = `FUNCTION → T`[` #n`]. `#n`은 오버로드일 때만(ALL_PROCEDURES
/// `overload`). [`member_overload`]가 되읽는다 — 둘은 한 쌍(시험 `member_detail_overload_round_trip`).
pub(super) fn member_detail(is_proc: bool, data_type: &str, overload: &str) -> String {
    let base = if is_proc { "" } else { data_type };
    match (base.is_empty(), overload.is_empty()) {
        (true, true) => String::new(),
        (true, false) => format!("#{overload}"),
        (false, true) => base.to_string(),
        (false, false) => format!("{base} #{overload}"),
    }
}

/// [`member_detail`]이 적은 오버로드 번호(`#n` 끝 조각 · 숫자만) — 없으면 `None`.
pub(super) fn member_overload(detail: &str) -> Option<&str> {
    let tail = detail.rsplit(' ').next().unwrap_or(detail);
    let n = tail.strip_prefix('#')?;
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then_some(n)
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
        Dialect::Db2 => format!(
            "SELECT TRIM(colname), CASE colorder WHEN 'D' THEN 'DESC' ELSE '' END FROM SYSCAT.INDEXCOLUSE WHERE indschema = {} AND indname = {} AND colorder <> 'I' ORDER BY colseq",
            lit(schema), lit(name)
        ),
        Dialect::Mysql => format!(
            "SELECT column_name, IFNULL(collation, '') FROM information_schema.statistics WHERE table_schema = {} AND index_name = {} ORDER BY seq_in_index",
            lit(schema), lit(name)
        ),
        Dialect::Odbc | Dialect::Duckdb | Dialect::Clickhouse | Dialect::Access => return Ok(Vec::new()),
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
        Dialect::Db2 => format!(
            "SELECT TRIM(constname), TRIM(tabschema) || '.' || TRIM(tabname) FROM SYSCAT.REFERENCES WHERE reftabschema = {} AND reftabname = {} ORDER BY tabname, constname",
            lit(schema), lit(name)
        ),
        Dialect::Mysql => format!(
            "SELECT constraint_name, constraint_schema || '.' || table_name FROM information_schema.referential_constraints WHERE unique_constraint_schema = {} AND referenced_table_name = {} ORDER BY table_name, constraint_name",
            lit(schema), lit(name)
        ),
        Dialect::Sqlite | Dialect::Odbc | Dialect::Duckdb | Dialect::Clickhouse | Dialect::Access => return Ok(Vec::new()),
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
        Dialect::Db2 => format!(
            "SELECT TRIM(trigname), CASE trigtime WHEN 'A' THEN 'AFTER' WHEN 'B' THEN 'BEFORE' ELSE 'INSTEAD OF' END || ' ' || TRIM(CASE eventinsert WHEN 'Y' THEN 'INSERT ' ELSE '' END || CASE eventupdate WHEN 'Y' THEN 'UPDATE ' ELSE '' END || CASE eventdelete WHEN 'Y' THEN 'DELETE' ELSE '' END), CASE valid WHEN 'N' THEN 'INVALID' WHEN 'X' THEN 'INOPERATIVE' ELSE '' END FROM SYSCAT.TRIGGERS WHERE tabschema = {} AND tabname = {} ORDER BY trigname",
            lit(schema), lit(name)
        ),
        Dialect::Mysql => format!(
            "SELECT trigger_name, CONCAT(action_timing, ' ', event_manipulation), '' FROM information_schema.triggers WHERE event_object_schema = {} AND event_object_table = {} ORDER BY trigger_name",
            lit(schema), lit(name)
        ),
        Dialect::Sqlite => format!(
            "SELECT name, '', '' FROM sqlite_master WHERE type = 'trigger' AND tbl_name = {} ORDER BY name",
            lit(name)
        ),
        Dialect::Odbc | Dialect::Duckdb | Dialect::Clickhouse | Dialect::Access => return Ok(Vec::new()),
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

    /// `member_detail` ↔ `member_overload` 왕복(프로시저·함수 · 오버로드 유무).
    #[test]
    fn private_members_from_body_source() {
        // 전방 선언 + 구현(같은 이름 한 번) · 공개 이름 제외 · 로컬 서브프로그램 제외 · 함수/프로시저 구분.
        let src = "CREATE OR REPLACE PACKAGE BODY pkg_x IS
            PROCEDURE cr_item(p_a VARCHAR2, p_b VARCHAR2);
            FUNCTION calc(p_a NUMBER) RETURN NUMBER;
            PROCEDURE run(p_a VARCHAR2) IS
              PROCEDURE local_helper IS BEGIN NULL; END local_helper;
            BEGIN
  cr_item(p_a, 'x');
END run;
            PROCEDURE cr_item(p_a VARCHAR2, p_b VARCHAR2) IS
BEGIN
  NULL;
END cr_item;
            FUNCTION calc(p_a NUMBER) RETURN NUMBER IS
BEGIN
  RETURN p_a;
END calc;
            END pkg_x;
/
";
        assert_eq!(
            private_members(src, &["RUN"]),
            vec![("CR_ITEM".to_string(), true), ("CALC".to_string(), false)]
        );
        // 본문이 없거나(빈 글) 패키지 본문이 아니면 빈 목록.
        assert!(private_members("", &[]).is_empty());
        assert!(private_members(
            "CREATE OR REPLACE PROCEDURE p IS BEGIN NULL; END;
/
",
            &[]
        )
        .is_empty());
    }

    #[test]
    fn member_detail_overload_round_trip() {
        assert_eq!(member_detail(true, "PROCEDURE", ""), "");
        assert_eq!(member_detail(true, "PROCEDURE", "2"), "#2");
        assert_eq!(
            member_detail(false, "FUNCTION → NUMBER", ""),
            "FUNCTION → NUMBER"
        );
        assert_eq!(
            member_detail(false, "FUNCTION → NUMBER", "3"),
            "FUNCTION → NUMBER #3"
        );
        assert_eq!(member_overload(""), None);
        assert_eq!(member_overload("#2"), Some("2"));
        assert_eq!(member_overload("FUNCTION → NUMBER #3"), Some("3"));
        assert_eq!(
            member_overload("FUNCTION → NUMBER"),
            None,
            "타입 글자는 번호가 아니다"
        );
        assert_eq!(member_overload("#"), None);
        assert_eq!(member_overload("#x1"), None);
    }

    /// ★ T-272: 연결된 서버 ▸ 카탈로그 ▸ 테이블 — 하위 종류 사슬 · 카탈로그 잎의 주인(`schema` 자리에 카탈로그) · 호출문 ·
    /// 4부 이름 · `sp_tables_ex` 행 거르기(시스템 유형 · 이름 없는 행) · 뷰 표시.
    #[test]
    fn linked_server_catalogs_and_tables() {
        assert_eq!(
            sub_kinds(Dialect::Mssql, ObjectKind::LinkedServer),
            &[SubKind::Catalogs]
        );
        assert!(sub_kinds(Dialect::Oracle, ObjectKind::LinkedServer).is_empty());
        assert_eq!(
            member_sub_kinds(Dialect::Mssql, SubKind::Catalogs),
            &[SubKind::LinkedTables]
        );
        assert!(member_sub_kinds(Dialect::Mssql, SubKind::LinkedTables).is_empty());
        let owner = |kind| ObjectInfo {
            db: String::new(),
            schema: String::new(),
            name: "LNK".into(),
            kind,
            status: String::new(),
            modified: String::new(),
            extra: String::new(),
        };
        let cat = item("sales", "", SubIcon::Property);
        let m = member_object(&owner(ObjectKind::LinkedServer), SubKind::Catalogs, &cat)
            .expect("카탈로그 주인");
        assert_eq!(
            (m.kind, m.name.as_str(), m.schema.as_str()),
            (ObjectKind::LinkedServer, "LNK", "sales")
        );
        assert!(member_object(&owner(ObjectKind::Table), SubKind::Catalogs, &cat).is_none());
        assert_eq!(
            linked_catalogs_sql("L'K"),
            "EXEC sp_catalogs @server_name = N'L''K'"
        );
        assert_eq!(
            linked_tables_sql("LNK", "sales"),
            "EXEC sp_tables_ex @table_server = N'LNK', @table_catalog = N'sales'"
        );
        assert_eq!(
            four_part_name("LNK", "sales", "dbo", "T]1"),
            "[LNK].[sales].[dbo].[T]]1]"
        );
        assert_eq!(four_part_name("LNK", "c", "", "t"), "[LNK].[c]..[t]");
        let t = linked_table_item("LNK", "sales", "dbo", "orders", "TABLE").expect("테이블");
        assert_eq!(
            (t.name.as_str(), t.detail.as_str(), t.status.as_str()),
            ("dbo.orders", "[LNK].[sales].[dbo].[orders]", "")
        );
        let v = linked_table_item("LNK", "sales", "", "v1", "view").expect("뷰");
        assert_eq!((v.name.as_str(), v.status.as_str()), ("v1", "VIEW"));
        assert!(linked_table_item("LNK", "sales", "sys", "objects", "SYSTEM TABLE").is_none());
        assert!(linked_table_item("LNK", "sales", "dbo", "", "TABLE").is_none());
    }

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
        // 패키지 멤버 잎 = Arguments 폴더(Oracle만) · 주인은 `패키지.멤버`.
        assert_eq!(
            member_sub_kinds(Dialect::Oracle, SubKind::Procedures),
            &[SubKind::Arguments]
        );
        assert!(member_sub_kinds(Dialect::Oracle, SubKind::Columns).is_empty());
        assert!(member_sub_kinds(Dialect::Postgres, SubKind::Functions).is_empty());
        let pkg = ObjectInfo {
            db: String::new(),
            schema: "S".into(),
            name: "PKG".into(),
            kind: ObjectKind::Package,
            status: String::new(),
            modified: String::new(),
            extra: String::new(),
        };
        let m = member_object(
            &pkg,
            SubKind::Functions,
            &item("F", "FUNCTION", SubIcon::Function),
        )
        .expect("member");
        assert_eq!((m.name.as_str(), m.kind), ("PKG.F", ObjectKind::Function));
        assert!(member_object(&pkg, SubKind::Columns, &item("X", "", SubIcon::Column)).is_none());
        // 오버로드 번호는 멤버의 흐린 글로 실려 `extra`가 되고, Arguments 잎이 되읽어 그 오버로드의 인자만 고른다(10-10).
        let m2 = member_object(
            &pkg,
            SubKind::Procedures,
            &item(
                "PRC_RUN_ALL",
                member_detail(true, "PROCEDURE", "2"),
                SubIcon::Procedure,
            ),
        )
        .expect("member");
        assert_eq!(member_overload(&m2.extra), Some("2"));
        for k in SubKind::ALL {
            assert_eq!(SubKind::parse(k.code()), Some(k));
            assert_eq!(SubKind::parse(k.label()), Some(k));
        }
    }
}
