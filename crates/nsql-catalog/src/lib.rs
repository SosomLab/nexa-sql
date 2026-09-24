//! 카탈로그(docs/28 · 사용자 09-15 "객체 탐색기 · 객체 생성/수정") — **`Session` 포트 위에서** 방언별 메타 SQL을 실행해
//! 스키마 · 오브젝트 목록 · 컬럼 · 소스(DDL) · 컴파일 오류를 돌려준다. GUI 탐색기와 CLI `nsql cat`이 같은 함수를 부른다.
//!
//! - 드라이버는 바뀌지 않는다(메타 조회 = 일반 SQL). 어댑터가 카탈로그를 직접 제공하는 확장(RPC `catalog.*`)은 후속.
//! - 한 호출 = 한 단계만(직계 자식 · docs/28 §0). 무거운 것(행 수 · DDL)은 요청 시.
//! - 문자열 리터럴은 `'` 이스케이프 · 식별자는 방언 인용으로 감싼다(`"NAME"` · `[name]`).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use nsql_core::{DbError, Dialect, ExecRequest, KeyInfo, ResultSet, Session, Value};

/// 오브젝트 종류(트리 폴더 = 종류).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    Table,
    View,
    MaterializedView,
    Procedure,
    Function,
    Package,
    PackageBody,
    Sequence,
    Trigger,
    Index,
    Synonym,
    Type,
}

impl ObjectKind {
    pub const ALL: [ObjectKind; 12] = [
        ObjectKind::Table,
        ObjectKind::View,
        ObjectKind::MaterializedView,
        ObjectKind::Procedure,
        ObjectKind::Function,
        ObjectKind::Package,
        ObjectKind::PackageBody,
        ObjectKind::Sequence,
        ObjectKind::Trigger,
        ObjectKind::Index,
        ObjectKind::Synonym,
        ObjectKind::Type,
    ];

    /// CLI 인자 · 설정용 코드.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            ObjectKind::Table => "table",
            ObjectKind::View => "view",
            ObjectKind::MaterializedView => "mview",
            ObjectKind::Procedure => "procedure",
            ObjectKind::Function => "function",
            ObjectKind::Package => "package",
            ObjectKind::PackageBody => "package_body",
            ObjectKind::Sequence => "sequence",
            ObjectKind::Trigger => "trigger",
            ObjectKind::Index => "index",
            ObjectKind::Synonym => "synonym",
            ObjectKind::Type => "type",
        }
    }

    /// 폴더 라벨(영어 · i18n은 호스트가 `Msg`로 덮는다).
    #[must_use]
    pub fn folder(self) -> &'static str {
        match self {
            ObjectKind::Table => "Tables",
            ObjectKind::View => "Views",
            ObjectKind::MaterializedView => "Materialized Views",
            ObjectKind::Procedure => "Procedures",
            ObjectKind::Function => "Functions",
            ObjectKind::Package => "Packages",
            ObjectKind::PackageBody => "Package Bodies",
            ObjectKind::Sequence => "Sequences",
            ObjectKind::Trigger => "Triggers",
            ObjectKind::Index => "Indexes",
            ObjectKind::Synonym => "Synonyms",
            ObjectKind::Type => "Types",
        }
    }

    /// 코드·별칭(`tables` · `procs` · `funcs` …) → 종류.
    #[must_use]
    pub fn parse(s: &str) -> Option<ObjectKind> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "table" | "tables" | "tab" => ObjectKind::Table,
            "view" | "views" => ObjectKind::View,
            "mview" | "mviews" | "materialized" | "matview" => ObjectKind::MaterializedView,
            "procedure" | "procedures" | "proc" | "procs" | "sp" => ObjectKind::Procedure,
            "function" | "functions" | "func" | "funcs" | "fn" => ObjectKind::Function,
            "package" | "packages" | "pkg" | "pkgs" => ObjectKind::Package,
            "package_body" | "package-body" | "body" | "bodies" | "pkgbody" => {
                ObjectKind::PackageBody
            }
            "sequence" | "sequences" | "seq" => ObjectKind::Sequence,
            "trigger" | "triggers" | "trg" => ObjectKind::Trigger,
            "index" | "indexes" | "idx" => ObjectKind::Index,
            "synonym" | "synonyms" | "syn" => ObjectKind::Synonym,
            "type" | "types" => ObjectKind::Type,
            _ => return None,
        })
    }

    /// 소스(본문)를 가진 종류 — 편집기에서 열어 `CREATE OR REPLACE`로 다시 컴파일할 수 있는 것.
    #[must_use]
    pub fn has_source(self) -> bool {
        matches!(
            self,
            ObjectKind::View
                | ObjectKind::MaterializedView
                | ObjectKind::Procedure
                | ObjectKind::Function
                | ObjectKind::Package
                | ObjectKind::PackageBody
                | ObjectKind::Trigger
                | ObjectKind::Type
        )
    }

    /// 행 데이터를 가진 종류(`SELECT *` 템플릿).
    #[must_use]
    pub fn is_relation(self) -> bool {
        matches!(
            self,
            ObjectKind::Table | ObjectKind::View | ObjectKind::MaterializedView
        )
    }
}

/// 오브젝트 한 줄.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectInfo {
    pub schema: String,
    pub name: String,
    pub kind: ObjectKind,
    /// Oracle `VALID`/`INVALID` · 그 외 빈 문자열.
    pub status: String,
    /// 마지막 변경 시각(방언 표기 그대로 · 없으면 빈 문자열).
    pub modified: String,
    /// 부가(PG 함수의 인자 서명 · 시퀀스 현재값 …).
    pub extra: String,
}

/// 컬럼 한 줄.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnInfo {
    pub name: String,
    /// 표시용 타입(`VARCHAR2(20)` · `numeric(10,2)` · `int`).
    pub data_type: String,
    pub nullable: bool,
    pub position: i64,
    pub default: String,
}

/// 컴파일 오류(Oracle `ALL_ERRORS`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    pub line: i64,
    pub col: i64,
    pub text: String,
    /// `ERROR` · `WARNING`.
    pub severity: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{} {}: {}",
            self.line, self.col, self.severity, self.text
        )
    }
}

/// 방언이 가진 종류(트리 폴더 순서).
#[must_use]
pub fn kinds_for(dialect: Dialect) -> &'static [ObjectKind] {
    use ObjectKind::*;
    match dialect {
        Dialect::Oracle => &[
            Table,
            View,
            MaterializedView,
            Procedure,
            Function,
            Package,
            PackageBody,
            Sequence,
            Trigger,
            Index,
            Synonym,
            Type,
        ],
        Dialect::Mssql => &[
            Table, View, Procedure, Function, Sequence, Trigger, Synonym, Type,
        ],
        Dialect::Postgres => &[
            Table,
            View,
            MaterializedView,
            Procedure,
            Function,
            Sequence,
            Trigger,
            Index,
            Type,
        ],
        Dialect::Mysql => &[Table, View, Procedure, Function, Trigger],
        Dialect::Sqlite => &[Table, View, Index, Trigger],
        Dialect::Odbc => &[Table, View],
    }
}

// ───────────────────────────────────────────── 공통 도우미

fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// 방언 식별자 인용.
#[must_use]
pub fn quote_ident(dialect: Dialect, name: &str) -> String {
    match dialect {
        Dialect::Mssql => format!("[{}]", name.replace(']', "]]")),
        Dialect::Mysql => format!("`{}`", name.replace('`', "``")),
        _ => format!("\"{}\"", name.replace('"', "\"\"")),
    }
}

/// `schema.name`(둘 다 인용).
#[must_use]
pub fn qualified(dialect: Dialect, schema: &str, name: &str) -> String {
    if schema.is_empty() {
        quote_ident(dialect, name)
    } else {
        format!(
            "{}.{}",
            quote_ident(dialect, schema),
            quote_ident(dialect, name)
        )
    }
}

fn query(s: &mut dyn Session, sql: &str) -> Result<ResultSet, DbError> {
    let r = s.execute(&ExecRequest {
        sql: sql.to_string(),
        params: vec![],
    })?;
    // 메타 질의는 추가 페치를 안 한다 — 드라이버가 상한에서 커서를 열어 뒀으면 바로 닫는다(핸들 누수 방지 · T-48a).
    if let Some(h) = r.pending {
        let _ = s.close_cursor(h);
    }
    Ok(r.result_sets.into_iter().next().unwrap_or_default())
}

fn cell_str(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Str(s) | Value::Decimal(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Cursor(_) => String::new(),
    }
}

fn cell_i64(v: &Value) -> i64 {
    match v {
        Value::Int(i) => *i,
        Value::Float(f) => *f as i64,
        Value::Str(s) | Value::Decimal(s) => s.trim().parse().unwrap_or(0),
        Value::Bool(b) => i64::from(*b),
        _ => 0,
    }
}

fn col(row: &[Value], i: usize) -> String {
    row.get(i).map(cell_str).unwrap_or_default()
}

fn truthy(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_uppercase().as_str(),
        "Y" | "YES" | "TRUE" | "T" | "1"
    )
}

// ───────────────────────────────────────────── 스키마

/// 스키마(소유자) 목록.
pub fn schemas(s: &mut dyn Session) -> Result<Vec<String>, DbError> {
    let sql = match s.dialect() {
        Dialect::Oracle => "SELECT username FROM all_users ORDER BY username".to_string(),
        Dialect::Mssql => "SELECT name FROM sys.schemas WHERE schema_id < 16384 AND name NOT IN ('sys','INFORMATION_SCHEMA','guest') ORDER BY name".to_string(),
        Dialect::Postgres => "SELECT nspname FROM pg_namespace WHERE nspname NOT LIKE 'pg\\_%' AND nspname <> 'information_schema' ORDER BY nspname".to_string(),
        Dialect::Mysql => "SELECT schema_name FROM information_schema.schemata ORDER BY schema_name".to_string(),
        Dialect::Sqlite => return Ok(vec!["main".into()]),
        Dialect::Odbc => "SELECT DISTINCT table_schema FROM information_schema.tables ORDER BY 1".to_string(),
    };
    let rs = query(s, &sql)?;
    Ok(rs.rows.iter().map(|r| col(r, 0)).collect())
}

/// 현재 스키마(접속 사용자의 기본).
pub fn current_schema(s: &mut dyn Session) -> Result<String, DbError> {
    let sql = match s.dialect() {
        Dialect::Oracle => "SELECT SYS_CONTEXT('USERENV','CURRENT_SCHEMA') FROM dual",
        Dialect::Mssql => "SELECT SCHEMA_NAME()",
        Dialect::Postgres => "SELECT current_schema()",
        Dialect::Mysql => "SELECT DATABASE()",
        Dialect::Sqlite => return Ok("main".into()),
        Dialect::Odbc => return Ok(String::new()),
    };
    let rs = query(s, sql)?;
    Ok(rs.rows.first().map(|r| col(r, 0)).unwrap_or_default())
}

/// 사전 뷰 버킷의 가짜 스키마 이름(메타 저장소 열쇠 · 자동 완성 전용 · docs/76 §8).
pub const DICT_SCHEMA: &str = "$dict";

/// ★ 접속 계정이 **지금 권한으로 읽을 수 있는** 사전 뷰(사용자 09-23 "권한에 맞춰 ALL_/DBA_ 등 · 다른 DBMS도 같은 개념").
/// 정적 표(nsql-script `builtins::system_objects`)는 접속 전 폴백이고, 접속 뒤에는 이 결과가 후보의 원천이다.
/// - Oracle: `ALL_VIEWS`(접근 가능한 뷰만 — `DBA_*`는 SELECT ANY DICTIONARY·SELECT_CATALOG_ROLE이 있을 때만 보인다)
///   + PUBLIC 시노님 `V$`/`GV$`(`ALL_SYNONYMS`도 접근 가능한 것만).
/// - PostgreSQL: `pg_catalog`·`information_schema`에서 `has_table_privilege(SELECT)`인 것.
/// - SQL Server: `sys`·`INFORMATION_SCHEMA`의 뷰·테이블·테이블 값 함수(메타데이터 가시성 규칙이 거른다).
/// - MySQL: `information_schema.tables`의 시스템 스키마(권한 있는 것만 보인다).
/// - SQLite: 내장 표(권한 개념 없음). ODBC: 없음.
///
/// 이름은 Oracle만 맨이름(`ALL_TABLES`) · 그 밖은 `스키마.이름`(FROM 뒤에 그대로 쓰는 꼴).
pub fn dictionary(s: &mut dyn Session) -> Result<Vec<ObjectInfo>, DbError> {
    let mk = |name: String| ObjectInfo {
        schema: DICT_SCHEMA.into(),
        name,
        kind: ObjectKind::View,
        status: String::new(),
        modified: String::new(),
        extra: String::new(),
    };
    // Oracle은 질의 둘로(09-23 실측: `ALL_SYNONYMS`가 수 초 — 메타 워커 큐를 막아 컬럼 로딩이 늦어졌다): ① `ALL_VIEWS`(접근 가능한
    //   사전 뷰만) ② `V$FIXED_TABLE`(V$/GV$ 이름 · SELECT ANY DICTIONARY·SELECT_CATALOG_ROLE이 없으면 실패 = 그 계정은 V$도 못 읽는다 → 건너뜀).
    if s.dialect() == Dialect::Oracle {
        // ★ 09-24 실측(BISCM 19c): `ALL_VIEWS` + LIKE 4개 = **121 s**(앱 로그 `[meta] dictionary`) ↔ `ALL_OBJECTS`(owner SYS · VIEW · LIKE)
        //   3,078행 ≈ 0.1 s · `V$FIXED_TABLE` 1,518행 ≈ 0.1 s → 그것으로. 둘 다 접근 가능한 것만 보인다(권한 반영 유지).
        let mut out: Vec<ObjectInfo> = query(s, "SELECT object_name FROM all_objects WHERE owner = 'SYS' AND object_type = 'VIEW' AND (object_name LIKE 'ALL\\_%' ESCAPE '\\' OR object_name LIKE 'DBA\\_%' ESCAPE '\\' OR object_name LIKE 'USER\\_%' ESCAPE '\\' OR object_name LIKE 'CDB\\_%' ESCAPE '\\') ORDER BY object_name")?
            .rows
            .iter()
            .map(|r| col(r, 0))
            .filter(|n| !n.is_empty())
            .map(mk)
            .collect();
        if let Ok(rs) = query(
            s,
            "SELECT name FROM v$fixed_table WHERE name LIKE 'V$%' OR name LIKE 'GV$%' ORDER BY name",
        ) {
            out.extend(rs.rows.iter().map(|r| col(r, 0)).filter(|n| !n.is_empty()).map(mk));
        }
        return Ok(out);
    }
    let sql = match s.dialect() {
        Dialect::Oracle => unreachable!(),
        Dialect::Mssql => "SELECT s.name + '.' + o.name FROM sys.all_objects o JOIN sys.schemas s ON s.schema_id = o.schema_id WHERE s.name IN ('sys','INFORMATION_SCHEMA') AND o.type IN ('V','U','S','IF','TF') ORDER BY 1",
        Dialect::Postgres => "SELECT n.nspname || '.' || c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname IN ('pg_catalog','information_schema') AND c.relkind IN ('r','v') AND has_table_privilege(c.oid, 'SELECT') ORDER BY 1",
        Dialect::Mysql => "SELECT CONCAT(table_schema, '.', table_name) FROM information_schema.tables WHERE table_schema IN ('information_schema','performance_schema','mysql','sys') ORDER BY 1",
        Dialect::Sqlite => {
            return Ok([
                "sqlite_master",
                "sqlite_schema",
                "sqlite_temp_master",
                "sqlite_temp_schema",
                "sqlite_sequence",
                "sqlite_stat1",
                "sqlite_stat4",
            ]
            .iter()
            .map(|n| mk((*n).to_string()))
            .collect());
        }
        Dialect::Odbc => return Ok(Vec::new()),
    };
    let rs = query(s, sql)?;
    Ok(rs
        .rows
        .iter()
        .map(|r| col(r, 0))
        .filter(|n| !n.is_empty())
        .map(mk)
        .collect())
}

// ───────────────────────────────────────────── 키(PK · 유니크)

/// (종류 'P'/'U', 이름, 컬럼) 행을 [`KeyInfo`]로 — 같은 이름은 한 묶음 · PK가 앞.
fn fold_keys(rows: &[Vec<Value>]) -> KeyInfo {
    let mut info = KeyInfo::default();
    for r in rows {
        let (kind, name, column) = (col(r, 0), col(r, 1), col(r, 2));
        if column.is_empty() {
            continue;
        }
        if kind == "P" {
            info.pk.push(column);
        } else if let Some((_, cols)) = info.unique.iter_mut().find(|(n, _)| *n == name) {
            cols.push(column);
        } else {
            info.unique.push((name, vec![column]));
        }
    }
    info
}

/// 테이블의 기본 키·유니크 제약/인덱스(순서 유지 · docs/41 키 규칙의 원천). 없으면 빈 [`KeyInfo`].
pub fn keys(s: &mut dyn Session, schema: &str, table: &str) -> Result<KeyInfo, DbError> {
    let dialect = s.dialect();
    match dialect {
        Dialect::Oracle => {
            let cons = query(s, &format!(
                "SELECT c.constraint_type, c.constraint_name, cc.column_name FROM all_constraints c JOIN all_cons_columns cc ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name WHERE c.owner = {} AND c.table_name = {} AND c.constraint_type IN ('P','U') AND c.status = 'ENABLED' ORDER BY DECODE(c.constraint_type, 'P', 0, 1), c.constraint_name, cc.position",
                lit(schema), lit(table)
            ))?;
            let idx = query(s, &format!(
                "SELECT 'U', i.index_name, ic.column_name FROM all_indexes i JOIN all_ind_columns ic ON ic.index_owner = i.owner AND ic.index_name = i.index_name WHERE i.table_owner = {} AND i.table_name = {} AND i.uniqueness = 'UNIQUE' ORDER BY i.index_name, ic.column_position",
                lit(schema), lit(table)
            ))?;
            let mut rows = cons.rows;
            rows.extend(idx.rows);
            Ok(fold_keys(&rows))
        }
        Dialect::Mssql => {
            let rs = query(s, &format!(
                "SELECT CASE WHEN i.is_primary_key = 1 THEN 'P' ELSE 'U' END, i.name, c.name FROM sys.indexes i JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id WHERE i.object_id = OBJECT_ID({}) AND (i.is_primary_key = 1 OR i.is_unique = 1) AND ic.is_included_column = 0 ORDER BY CASE WHEN i.is_primary_key = 1 THEN 0 ELSE 1 END, i.index_id, ic.key_ordinal",
                lit(&format!("{}.{}", quote_ident(dialect, schema), quote_ident(dialect, table)))
            ))?;
            Ok(fold_keys(&rs.rows))
        }
        Dialect::Postgres => {
            let rs = query(s, &format!(
                "SELECT CASE WHEN i.indisprimary THEN 'P' ELSE 'U' END, ic.relname, a.attname FROM pg_index i JOIN pg_class c ON c.oid = i.indrelid JOIN pg_namespace n ON n.oid = c.relnamespace JOIN pg_class ic ON ic.oid = i.indexrelid JOIN unnest(i.indkey) WITH ORDINALITY k(attnum, ord) ON true JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = k.attnum WHERE n.nspname = {} AND c.relname = {} AND (i.indisprimary OR i.indisunique) ORDER BY CASE WHEN i.indisprimary THEN 0 ELSE 1 END, ic.relname, k.ord",
                lit(schema), lit(table)
            ))?;
            Ok(fold_keys(&rs.rows))
        }
        Dialect::Mysql | Dialect::Odbc => {
            let rs = query(s, &format!(
                "SELECT CASE WHEN tc.constraint_type = 'PRIMARY KEY' THEN 'P' ELSE 'U' END, tc.constraint_name, kcu.column_name FROM information_schema.table_constraints tc JOIN information_schema.key_column_usage kcu ON kcu.constraint_schema = tc.constraint_schema AND kcu.constraint_name = tc.constraint_name AND kcu.table_name = tc.table_name WHERE tc.table_schema = {} AND tc.table_name = {} AND tc.constraint_type IN ('PRIMARY KEY','UNIQUE') ORDER BY CASE WHEN tc.constraint_type = 'PRIMARY KEY' THEN 0 ELSE 1 END, tc.constraint_name, kcu.ordinal_position",
                lit(schema), lit(table)
            ))?;
            Ok(fold_keys(&rs.rows))
        }
        Dialect::Sqlite => {
            let mut info = KeyInfo::default();
            // PRAGMA table_info: cid, name, type, notnull, dflt_value, pk(복합이면 1,2,…)
            let ti = query(
                s,
                &format!("PRAGMA table_info({})", quote_ident(dialect, table)),
            )?;
            let mut pk: Vec<(i64, String)> = ti
                .rows
                .iter()
                .filter(|r| cell_i64(&r[5]) > 0)
                .map(|r| (cell_i64(&r[5]), col(r, 1)))
                .collect();
            pk.sort();
            info.pk = pk.into_iter().map(|(_, n)| n).collect();
            // PRAGMA index_list: seq, name, unique, origin, partial
            let il = query(
                s,
                &format!("PRAGMA index_list({})", quote_ident(dialect, table)),
            )?;
            for r in &il.rows {
                if cell_i64(&r[2]) != 1 || col(r, 3) == "pk" {
                    continue;
                }
                let name = col(r, 1);
                let ii = query(
                    s,
                    &format!("PRAGMA index_info({})", quote_ident(dialect, &name)),
                )?;
                let mut cols: Vec<(i64, String)> = ii
                    .rows
                    .iter()
                    .map(|r| (cell_i64(&r[0]), col(r, 2)))
                    .collect();
                cols.sort();
                info.unique
                    .push((name, cols.into_iter().map(|(_, n)| n).collect()));
            }
            Ok(info)
        }
    }
}

// ───────────────────────────────────────────── 테이블 상세(키·인덱스·제약 — 완성 상세 카드 · T-179 hover 카드 · 09-24)

/// 제약 하나(종류 = `P` 기본 키 · `U` 유니크 · `R` 외래 키 · `C` 체크).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyDef {
    pub name: String,
    pub kind: char,
    pub cols: Vec<String>,
    /// 외래 키가 가리키는 테이블(`R`만).
    pub ref_table: Option<String>,
}

/// 인덱스 하나.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexDef {
    pub name: String,
    pub unique: bool,
    pub cols: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableDetail {
    pub keys: Vec<KeyDef>,
    pub indexes: Vec<IndexDef>,
    /// 테이블 코멘트(Description · 없으면 None · 09-24).
    pub comment: Option<String>,
    /// 컬럼 코멘트(이름, 코멘트) — 있는 것만.
    pub col_comments: Vec<(String, String)>,
}

/// 테이블·컬럼 코멘트(Description · 사용자 09-24) — 실패는 없음으로(코멘트 기능이 없는 DB · 권한).
fn comments(
    s: &mut dyn Session,
    schema: &str,
    table: &str,
) -> (Option<String>, Vec<(String, String)>) {
    let dialect = s.dialect();
    let (tsql, csql): (Option<String>, Option<String>) = match dialect {
        Dialect::Oracle => (
            Some(format!("SELECT comments FROM all_tab_comments WHERE owner = {} AND table_name = {}", lit(schema), lit(table))),
            Some(format!("SELECT column_name, comments FROM all_col_comments WHERE owner = {} AND table_name = {} AND comments IS NOT NULL", lit(schema), lit(table))),
        ),
        Dialect::Mssql => {
            let obj = lit(&format!("{}.{}", quote_ident(dialect, schema), quote_ident(dialect, table)));
            (
                Some(format!("SELECT CAST(value AS NVARCHAR(4000)) FROM sys.extended_properties WHERE major_id = OBJECT_ID({obj}) AND minor_id = 0 AND name = 'MS_Description'")),
                Some(format!("SELECT c.name, CAST(ep.value AS NVARCHAR(4000)) FROM sys.extended_properties ep JOIN sys.columns c ON c.object_id = ep.major_id AND c.column_id = ep.minor_id WHERE ep.major_id = OBJECT_ID({obj}) AND ep.name = 'MS_Description'")),
            )
        }
        Dialect::Postgres => (
            Some(format!("SELECT obj_description(c.oid, 'pg_class') FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {} AND c.relname = {}", lit(schema), lit(table))),
            Some(format!("SELECT a.attname, col_description(a.attrelid, a.attnum) FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {} AND c.relname = {} AND a.attnum > 0 AND NOT a.attisdropped AND col_description(a.attrelid, a.attnum) IS NOT NULL", lit(schema), lit(table))),
        ),
        Dialect::Mysql | Dialect::Odbc => (
            Some(format!("SELECT table_comment FROM information_schema.tables WHERE table_schema = {} AND table_name = {}", lit(schema), lit(table))),
            Some(format!("SELECT column_name, column_comment FROM information_schema.columns WHERE table_schema = {} AND table_name = {} AND column_comment <> ''", lit(schema), lit(table))),
        ),
        Dialect::Sqlite => (None, None),
    };
    let tc = tsql
        .and_then(|q| query(s, &q).ok())
        .and_then(|rs| rs.rows.first().map(|r| col(r, 0)))
        .filter(|c| !c.trim().is_empty());
    let cc = csql
        .and_then(|q| query(s, &q).ok())
        .map(|rs| {
            rs.rows
                .iter()
                .map(|r| (col(r, 0), col(r, 1)))
                .filter(|(n, c)| !n.is_empty() && !c.trim().is_empty())
                .collect()
        })
        .unwrap_or_default();
    (tc, cc)
}

/// (종류, 이름, 컬럼, 참조) 행을 접는다 — 같은 이름은 한 묶음 · 순서 유지.
fn fold_defs(rows: &[Vec<Value>]) -> Vec<KeyDef> {
    let mut out: Vec<KeyDef> = Vec::new();
    for r in rows {
        let (kind, name, column, rf) = (col(r, 0), col(r, 1), col(r, 2), col(r, 3));
        let kind = kind.chars().next().unwrap_or('C');
        if let Some(k) = out.iter_mut().find(|k| k.name == name) {
            if !column.is_empty() {
                k.cols.push(column);
            }
        } else {
            out.push(KeyDef {
                name,
                kind,
                cols: if column.is_empty() {
                    Vec::new()
                } else {
                    vec![column]
                },
                ref_table: (!rf.is_empty()).then_some(rf),
            });
        }
    }
    out
}

fn fold_indexes(rows: &[Vec<Value>]) -> Vec<IndexDef> {
    let mut out: Vec<IndexDef> = Vec::new();
    for r in rows {
        let (name, uniq, column) = (col(r, 0), col(r, 1), col(r, 2));
        if let Some(i) = out.iter_mut().find(|i| i.name == name) {
            if !column.is_empty() {
                i.cols.push(column);
            }
        } else {
            out.push(IndexDef {
                name,
                unique: truthy(&uniq) || uniq.eq_ignore_ascii_case("UNIQUE"),
                cols: if column.is_empty() {
                    Vec::new()
                } else {
                    vec![column]
                },
            });
        }
    }
    out
}

/// ★ 테이블 하나의 제약(PK·UK·FK·CHECK)과 인덱스 전부(완성 상세 카드 "이 컬럼이 든 객체" · 객체 정보 · 09-24).
/// 질의 둘(제약 · 인덱스) · 테이블 단위라 가볍다(`keys()`는 PK/UK만이라 남겨 둔다).
pub fn table_detail(
    s: &mut dyn Session,
    schema: &str,
    table: &str,
) -> Result<TableDetail, DbError> {
    let dialect = s.dialect();
    let (cons_sql, idx_sql): (String, String) = match dialect {
        Dialect::Oracle => {
            // ★ 09-24 실측(BISCM · 접속 제외): 제약 + FK 대상 테이블 **자기 조인** ≈ 1,155 ms ↔ 조인 없이 ≈ 315 ms · 인덱스 ≈ 91 ms →
            //   조인을 빼고, FK가 있을 때만 대상 테이블 이름을 한 번 더 묻는다(보통 0~1회).
            let cons = query(s, &format!(
                "SELECT c.constraint_type, c.constraint_name, cc.column_name, '', c.r_owner, c.r_constraint_name FROM all_constraints c JOIN all_cons_columns cc ON cc.owner = c.owner AND cc.constraint_name = c.constraint_name WHERE c.owner = {} AND c.table_name = {} AND c.constraint_type IN ('P','U','R','C') AND c.generated = 'USER NAME' ORDER BY DECODE(c.constraint_type, 'P', 0, 'U', 1, 'R', 2, 3), c.constraint_name, cc.position",
                lit(schema), lit(table)
            ))?;
            let mut keys = fold_defs(&cons.rows);
            // FK → (r_owner, r_constraint_name) → 대상 테이블(제약 이름별 1행).
            let mut refs: Vec<(String, String, String)> = Vec::new();
            for r in &cons.rows {
                if col(r, 0) == "R" {
                    let (name, ro, rc) = (col(r, 1), col(r, 4), col(r, 5));
                    if !rc.is_empty() && !refs.iter().any(|(n, _, _)| *n == name) {
                        refs.push((name, ro, rc));
                    }
                }
            }
            if !refs.is_empty() {
                let list = refs
                    .iter()
                    .map(|(_, ro, rc)| format!("({}, {})", lit(ro), lit(rc)))
                    .collect::<Vec<_>>()
                    .join(",");
                if let Ok(rs) = query(s, &format!(
                    "SELECT owner, constraint_name, table_name FROM all_constraints WHERE (owner, constraint_name) IN ({list})"
                )) {
                    for r in &rs.rows {
                        let (ro, rc, tn) = (col(r, 0), col(r, 1), col(r, 2));
                        for (name, o, c) in &refs {
                            if *o == ro && *c == rc {
                                if let Some(k) = keys.iter_mut().find(|k| k.name == *name) {
                                    k.ref_table = Some(tn.clone());
                                }
                            }
                        }
                    }
                }
            }
            let idx = query(s, &format!(
                "SELECT i.index_name, i.uniqueness, ic.column_name FROM all_indexes i JOIN all_ind_columns ic ON ic.index_owner = i.owner AND ic.index_name = i.index_name WHERE i.table_owner = {} AND i.table_name = {} ORDER BY i.index_name, ic.column_position",
                lit(schema), lit(table)
            ))?;
            let (comment, col_comments) = comments(s, schema, table);
            return Ok(TableDetail {
                keys,
                indexes: fold_indexes(&idx.rows),
                comment,
                col_comments,
            });
        }
        Dialect::Mssql => {
            let obj = lit(&format!("{}.{}", quote_ident(dialect, schema), quote_ident(dialect, table)));
            (
                format!(
                    "SELECT x.k, x.n, x.c, x.r FROM (SELECT CASE WHEN kc.type = 'PK' THEN 'P' ELSE 'U' END AS k, kc.name AS n, c.name AS c, '' AS r, ic.key_ordinal AS o FROM sys.key_constraints kc JOIN sys.index_columns ic ON ic.object_id = kc.parent_object_id AND ic.index_id = kc.unique_index_id JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id WHERE kc.parent_object_id = OBJECT_ID({obj}) UNION ALL SELECT 'R', fk.name, c.name, OBJECT_NAME(fk.referenced_object_id), fkc.constraint_column_id FROM sys.foreign_keys fk JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id JOIN sys.columns c ON c.object_id = fkc.parent_object_id AND c.column_id = fkc.parent_column_id WHERE fk.parent_object_id = OBJECT_ID({obj}) UNION ALL SELECT 'C', ck.name, ISNULL(c.name, ''), '', 1 FROM sys.check_constraints ck LEFT JOIN sys.columns c ON c.object_id = ck.parent_object_id AND c.column_id = ck.parent_column_id WHERE ck.parent_object_id = OBJECT_ID({obj})) x ORDER BY CASE x.k WHEN 'P' THEN 0 WHEN 'U' THEN 1 WHEN 'R' THEN 2 ELSE 3 END, x.n, x.o"
                ),
                format!(
                    "SELECT i.name, CASE WHEN i.is_unique = 1 THEN 'Y' ELSE 'N' END, c.name FROM sys.indexes i JOIN sys.index_columns ic ON ic.object_id = i.object_id AND ic.index_id = i.index_id AND ic.is_included_column = 0 JOIN sys.columns c ON c.object_id = ic.object_id AND c.column_id = ic.column_id WHERE i.object_id = OBJECT_ID({obj}) AND i.name IS NOT NULL ORDER BY i.index_id, ic.key_ordinal"
                ),
            )
        }
        Dialect::Postgres => (
            format!(
                "SELECT UPPER(con.contype::text), con.conname, a.attname, COALESCE(rt.relname, '') FROM pg_constraint con JOIN pg_class t ON t.oid = con.conrelid JOIN pg_namespace n ON n.oid = t.relnamespace LEFT JOIN pg_class rt ON rt.oid = con.confrelid CROSS JOIN LATERAL unnest(con.conkey) WITH ORDINALITY k(attnum, ord) JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum WHERE n.nspname = {} AND t.relname = {} AND con.contype IN ('p','u','f','c') ORDER BY CASE con.contype WHEN 'p' THEN 0 WHEN 'u' THEN 1 WHEN 'f' THEN 2 ELSE 3 END, con.conname, k.ord",
                lit(schema), lit(table)
            ),
            format!(
                "SELECT ic.relname, CASE WHEN ix.indisunique THEN 'Y' ELSE 'N' END, a.attname FROM pg_index ix JOIN pg_class t ON t.oid = ix.indrelid JOIN pg_namespace n ON n.oid = t.relnamespace JOIN pg_class ic ON ic.oid = ix.indexrelid CROSS JOIN LATERAL unnest(ix.indkey) WITH ORDINALITY k(attnum, ord) JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = k.attnum WHERE n.nspname = {} AND t.relname = {} ORDER BY ic.relname, k.ord",
                lit(schema), lit(table)
            ),
        ),
        Dialect::Mysql | Dialect::Odbc => (
            format!(
                "SELECT CASE tc.constraint_type WHEN 'PRIMARY KEY' THEN 'P' WHEN 'UNIQUE' THEN 'U' WHEN 'FOREIGN KEY' THEN 'R' ELSE 'C' END, tc.constraint_name, kcu.column_name, IFNULL(kcu.referenced_table_name, '') FROM information_schema.table_constraints tc JOIN information_schema.key_column_usage kcu ON kcu.constraint_schema = tc.constraint_schema AND kcu.constraint_name = tc.constraint_name AND kcu.table_name = tc.table_name WHERE tc.table_schema = {} AND tc.table_name = {} ORDER BY 1, 2, kcu.ordinal_position",
                lit(schema), lit(table)
            ),
            format!(
                "SELECT index_name, CASE WHEN non_unique = 0 THEN 'Y' ELSE 'N' END, column_name FROM information_schema.statistics WHERE table_schema = {} AND table_name = {} ORDER BY index_name, seq_in_index",
                lit(schema), lit(table)
            ),
        ),
        Dialect::Sqlite => {
            let mut d = TableDetail::default();
            let k = keys(s, schema, table)?;
            if !k.pk.is_empty() {
                d.keys.push(KeyDef {
                    name: "PRIMARY KEY".into(),
                    kind: 'P',
                    cols: k.pk,
                    ref_table: None,
                });
            }
            let fk = query(
                s,
                &format!("PRAGMA foreign_key_list({})", quote_ident(dialect, table)),
            )?;
            // id, seq, table, from, to, …
            for r in &fk.rows {
                let name = format!("FK_{}", cell_i64(&r[0]));
                let from = col(r, 3);
                if let Some(e) = d.keys.iter_mut().find(|e| e.name == name) {
                    e.cols.push(from);
                } else {
                    d.keys.push(KeyDef {
                        name,
                        kind: 'R',
                        cols: vec![from],
                        ref_table: Some(col(r, 2)),
                    });
                }
            }
            let il = query(
                s,
                &format!("PRAGMA index_list({})", quote_ident(dialect, table)),
            )?;
            for r in &il.rows {
                let name = col(r, 1);
                let ii = query(
                    s,
                    &format!("PRAGMA index_info({})", quote_ident(dialect, &name)),
                )?;
                let mut cols: Vec<(i64, String)> = ii
                    .rows
                    .iter()
                    .map(|r| (cell_i64(&r[0]), col(r, 2)))
                    .collect();
                cols.sort();
                d.indexes.push(IndexDef {
                    name,
                    unique: cell_i64(&r[2]) == 1,
                    cols: cols.into_iter().map(|(_, n)| n).collect(),
                });
            }
            return Ok(d);
        }
    };
    let cons = query(s, &cons_sql)?;
    let idx = query(s, &idx_sql)?;
    let (comment, col_comments) = comments(s, schema, table);
    Ok(TableDetail {
        keys: fold_defs(&cons.rows),
        indexes: fold_indexes(&idx.rows),
        comment,
        col_comments,
    })
}

// ───────────────────────────────────────────── 오브젝트 목록

/// ★ 테이블처럼 FROM 뒤에 쓸 수 있는 함수인가(`objects()`의 `extra`로 판정 · 09-24): Oracle = `PIPELINED` · SQL Server = 테이블 반환
/// 함수 `IF`/`TF`/`FT` · PostgreSQL = 집합 반환(`SETOF …`). 그 밖·프로시저 = 거짓.
#[must_use]
pub fn table_function(dialect: Dialect, kind: ObjectKind, extra: &str) -> bool {
    if kind != ObjectKind::Function {
        return false;
    }
    match dialect {
        Dialect::Oracle => extra == "PIPELINED",
        Dialect::Mssql => matches!(extra, "IF" | "TF" | "FT"),
        Dialect::Postgres => extra.starts_with("SETOF"),
        _ => false,
    }
}

fn oracle_type(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Table => "TABLE",
        ObjectKind::View => "VIEW",
        ObjectKind::MaterializedView => "MATERIALIZED VIEW",
        ObjectKind::Procedure => "PROCEDURE",
        ObjectKind::Function => "FUNCTION",
        ObjectKind::Package => "PACKAGE",
        ObjectKind::PackageBody => "PACKAGE BODY",
        ObjectKind::Sequence => "SEQUENCE",
        ObjectKind::Trigger => "TRIGGER",
        ObjectKind::Index => "INDEX",
        ObjectKind::Synonym => "SYNONYM",
        ObjectKind::Type => "TYPE",
    }
}

/// 스키마의 한 종류 오브젝트 목록(이름 순).
pub fn objects(
    s: &mut dyn Session,
    schema: &str,
    kind: ObjectKind,
) -> Result<Vec<ObjectInfo>, DbError> {
    let dialect = s.dialect();
    let rows: Vec<(String, String, String, String)> = match dialect {
        Dialect::Oracle => {
            // ★ 함수는 `ALL_PROCEDURES.PIPELINED`를 부가에(FROM 자리의 테이블 함수 판정 · 09-24 · 독립 함수 = procedure_name NULL).
            let sql = if kind == ObjectKind::Function {
                format!(
                    "SELECT o.object_name, o.status, TO_CHAR(o.last_ddl_time, 'YYYY-MM-DD HH24:MI:SS'), CASE WHEN p.pipelined = 'YES' THEN 'PIPELINED' ELSE '' END FROM all_objects o LEFT JOIN all_procedures p ON p.owner = o.owner AND p.object_name = o.object_name AND p.object_type = 'FUNCTION' AND p.procedure_name IS NULL WHERE o.owner = {} AND o.object_type = 'FUNCTION' AND (o.object_name NOT LIKE 'BIN$%') ORDER BY o.object_name",
                    lit(schema)
                )
            } else {
                format!(
                    "SELECT object_name, status, TO_CHAR(last_ddl_time, 'YYYY-MM-DD HH24:MI:SS'), '' FROM all_objects WHERE owner = {} AND object_type = {} AND (object_name NOT LIKE 'BIN$%') ORDER BY object_name",
                    lit(schema),
                    lit(oracle_type(kind))
                )
            };
            query(s, &sql)?
                .rows
                .iter()
                .map(|r| (col(r, 0), col(r, 1), col(r, 2), col(r, 3)))
                .collect()
        }
        Dialect::Mssql => {
            let types: &str = match kind {
                ObjectKind::Table => "'U'",
                ObjectKind::View => "'V'",
                ObjectKind::Procedure => "'P','PC'",
                ObjectKind::Function => "'FN','IF','TF','AF','FS','FT'",
                ObjectKind::Trigger => "'TR','TA'",
                ObjectKind::Sequence => "'SO'",
                ObjectKind::Synonym => "'SN'",
                ObjectKind::Type => {
                    let sql = format!(
                        "SELECT t.name, '', '', '' FROM sys.types t JOIN sys.schemas s ON s.schema_id = t.schema_id WHERE t.is_user_defined = 1 AND s.name = {} ORDER BY t.name",
                        lit(schema)
                    );
                    return Ok(query(s, &sql)?
                        .rows
                        .iter()
                        .map(|r| {
                            info(
                                schema,
                                kind,
                                col(r, 0),
                                String::new(),
                                String::new(),
                                String::new(),
                            )
                        })
                        .collect());
                }
                _ => return Ok(Vec::new()),
            };
            let sql = format!(
                "SELECT o.name, '', CONVERT(varchar(19), o.modify_date, 120), o.type FROM sys.objects o JOIN sys.schemas s ON s.schema_id = o.schema_id WHERE s.name = {} AND o.type IN ({types}) ORDER BY o.name",
                lit(schema)
            );
            query(s, &sql)?
                .rows
                .iter()
                .map(|r| {
                    (
                        col(r, 0),
                        col(r, 1),
                        col(r, 2),
                        col(r, 3).trim().to_string(),
                    )
                })
                .collect()
        }
        Dialect::Postgres => {
            let sql = match kind {
                ObjectKind::Table => format!("SELECT c.relname, '', '', c.relkind FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {} AND c.relkind IN ('r','p','f') ORDER BY c.relname", lit(schema)),
                ObjectKind::View => format!("SELECT c.relname, '', '', '' FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {} AND c.relkind = 'v' ORDER BY c.relname", lit(schema)),
                ObjectKind::MaterializedView => format!("SELECT c.relname, '', '', '' FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {} AND c.relkind = 'm' ORDER BY c.relname", lit(schema)),
                ObjectKind::Sequence => format!("SELECT c.relname, '', '', '' FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {} AND c.relkind = 'S' ORDER BY c.relname", lit(schema)),
                ObjectKind::Index => format!("SELECT c.relname, '', '', t.relname FROM pg_index i JOIN pg_class c ON c.oid = i.indexrelid JOIN pg_class t ON t.oid = i.indrelid JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {} ORDER BY c.relname", lit(schema)),
                ObjectKind::Procedure | ObjectKind::Function => format!(
                    "SELECT p.proname, '', '', CASE WHEN p.proretset THEN 'SETOF ' ELSE '' END || pg_get_function_identity_arguments(p.oid) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE n.nspname = {} AND p.prokind = '{}' ORDER BY p.proname",
                    lit(schema),
                    if kind == ObjectKind::Procedure { 'p' } else { 'f' }
                ),
                ObjectKind::Trigger => format!("SELECT t.tgname, '', '', c.relname FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {} AND NOT t.tgisinternal ORDER BY t.tgname", lit(schema)),
                ObjectKind::Type => format!("SELECT t.typname, '', '', t.typtype FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace WHERE n.nspname = {} AND t.typtype IN ('c','e','d','r') AND NOT EXISTS (SELECT 1 FROM pg_class c WHERE c.reltype = t.oid AND c.relkind <> 'c') ORDER BY t.typname", lit(schema)),
                _ => return Ok(Vec::new()),
            };
            query(s, &sql)?
                .rows
                .iter()
                .map(|r| (col(r, 0), col(r, 1), col(r, 2), col(r, 3)))
                .collect()
        }
        Dialect::Mysql => {
            let sql = match kind {
                ObjectKind::Table => format!("SELECT table_name, '', DATE_FORMAT(update_time, '%Y-%m-%d %H:%i:%s'), '' FROM information_schema.tables WHERE table_schema = {} AND table_type = 'BASE TABLE' ORDER BY table_name", lit(schema)),
                ObjectKind::View => format!("SELECT table_name, '', '', '' FROM information_schema.views WHERE table_schema = {} ORDER BY table_name", lit(schema)),
                ObjectKind::Procedure | ObjectKind::Function => format!("SELECT routine_name, '', DATE_FORMAT(last_altered, '%Y-%m-%d %H:%i:%s'), '' FROM information_schema.routines WHERE routine_schema = {} AND routine_type = '{}' ORDER BY routine_name", lit(schema), if kind == ObjectKind::Procedure { "PROCEDURE" } else { "FUNCTION" }),
                ObjectKind::Trigger => format!("SELECT trigger_name, '', '', event_object_table FROM information_schema.triggers WHERE trigger_schema = {} ORDER BY trigger_name", lit(schema)),
                _ => return Ok(Vec::new()),
            };
            query(s, &sql)?
                .rows
                .iter()
                .map(|r| (col(r, 0), col(r, 1), col(r, 2), col(r, 3)))
                .collect()
        }
        Dialect::Sqlite => {
            let ty = match kind {
                ObjectKind::Table => "table",
                ObjectKind::View => "view",
                ObjectKind::Index => "index",
                ObjectKind::Trigger => "trigger",
                _ => return Ok(Vec::new()),
            };
            let sql = format!(
                "SELECT name, '', '', tbl_name FROM sqlite_master WHERE type = '{ty}' AND name NOT LIKE 'sqlite_%' ORDER BY name"
            );
            query(s, &sql)?
                .rows
                .iter()
                .map(|r| (col(r, 0), col(r, 1), col(r, 2), col(r, 3)))
                .collect()
        }
        Dialect::Odbc => {
            let tt = match kind {
                ObjectKind::Table => "BASE TABLE",
                ObjectKind::View => "VIEW",
                _ => return Ok(Vec::new()),
            };
            let sql = format!(
                "SELECT table_name, '', '', '' FROM information_schema.tables WHERE table_schema = {} AND table_type = '{tt}' ORDER BY table_name",
                lit(schema)
            );
            query(s, &sql)?
                .rows
                .iter()
                .map(|r| (col(r, 0), col(r, 1), col(r, 2), col(r, 3)))
                .collect()
        }
    };
    Ok(rows
        .into_iter()
        .map(|(name, status, modified, extra)| info(schema, kind, name, status, modified, extra))
        .collect())
}

fn info(
    schema: &str,
    kind: ObjectKind,
    name: String,
    status: String,
    modified: String,
    extra: String,
) -> ObjectInfo {
    ObjectInfo {
        schema: schema.to_string(),
        name,
        kind,
        status,
        modified,
        extra,
    }
}

// ───────────────────────────────────────────── 컬럼

fn fmt_type(base: &str, len: &str, prec: &str, scale: &str) -> String {
    let up = base.to_ascii_uppercase();
    let (len, prec, scale) = (len.trim(), prec.trim(), scale.trim());
    if !prec.is_empty() && prec != "0" && (up.contains("NUM") || up.contains("DEC")) {
        if !scale.is_empty() && scale != "0" {
            format!("{base}({prec},{scale})")
        } else {
            format!("{base}({prec})")
        }
    } else if !len.is_empty()
        && len != "0"
        && len != "-1"
        && (up.contains("CHAR") || up.contains("BINARY") || up.contains("RAW"))
    {
        format!("{base}({len})")
    } else {
        base.to_string()
    }
}

/// 테이블/뷰의 컬럼(위치 순).
pub fn columns(s: &mut dyn Session, schema: &str, table: &str) -> Result<Vec<ColumnInfo>, DbError> {
    let dialect = s.dialect();
    let sql = match dialect {
        Dialect::Oracle => format!(
            "SELECT column_name, data_type, CASE WHEN char_used = 'C' THEN char_length ELSE data_length END, data_precision, data_scale, nullable, column_id, '' FROM all_tab_columns WHERE owner = {} AND table_name = {} ORDER BY column_id",
            lit(schema),
            lit(table)
        ),
        Dialect::Mssql => format!(
            "SELECT c.name, t.name, CASE WHEN t.name IN ('nchar','nvarchar') AND c.max_length > 0 THEN c.max_length / 2 ELSE c.max_length END, c.precision, c.scale, CASE WHEN c.is_nullable = 1 THEN 'Y' ELSE 'N' END, c.column_id, ISNULL(d.definition, '') FROM sys.all_columns c JOIN sys.types t ON t.user_type_id = c.user_type_id LEFT JOIN sys.default_constraints d ON d.object_id = c.default_object_id WHERE c.object_id = OBJECT_ID({}) ORDER BY c.column_id",
            lit(&format!("{}.{}", quote_ident(dialect, schema), quote_ident(dialect, table)))
        ),
        Dialect::Postgres | Dialect::Mysql | Dialect::Odbc => format!(
            "SELECT column_name, data_type, character_maximum_length, numeric_precision, numeric_scale, is_nullable, ordinal_position, column_default FROM information_schema.columns WHERE table_schema = {} AND table_name = {} ORDER BY ordinal_position",
            lit(schema),
            lit(table)
        ),
        Dialect::Sqlite => {
            let rs = query(s, &format!("PRAGMA table_info({})", quote_ident(dialect, table)))?;
            // cid, name, type, notnull, dflt_value, pk
            return Ok(rs
                .rows
                .iter()
                .map(|r| ColumnInfo {
                    name: col(r, 1),
                    data_type: col(r, 2),
                    nullable: col(r, 3) == "0",
                    position: cell_i64(&r[0]) + 1,
                    default: col(r, 4),
                })
                .collect());
        }
    };
    let rs = query(s, &sql)?;
    Ok(rs
        .rows
        .iter()
        .map(|r| ColumnInfo {
            name: col(r, 0),
            data_type: fmt_type(&col(r, 1), &col(r, 2), &col(r, 3), &col(r, 4)),
            nullable: truthy(&col(r, 5)),
            position: r.get(6).map(cell_i64).unwrap_or(0),
            default: col(r, 7),
        })
        .collect())
}

// ───────────────────────────────────────────── 소스(DDL)

/// 오브젝트 소스 — 프로시저/함수/패키지/트리거/타입은 `CREATE OR REPLACE …` 전문, 뷰는 `CREATE OR REPLACE VIEW … AS …`,
/// 테이블은 DDL(Oracle `DBMS_METADATA` · 그 외 컬럼으로 생성). 편집기에 열어 고친 뒤 실행하면 곧 재컴파일.
pub fn source(
    s: &mut dyn Session,
    schema: &str,
    kind: ObjectKind,
    name: &str,
) -> Result<String, DbError> {
    let dialect = s.dialect();
    match dialect {
        Dialect::Oracle => match kind {
            ObjectKind::Procedure
            | ObjectKind::Function
            | ObjectKind::Package
            | ObjectKind::PackageBody
            | ObjectKind::Trigger
            | ObjectKind::Type => {
                let sql = format!(
                    "SELECT text FROM all_source WHERE owner = {} AND name = {} AND type = {} ORDER BY line",
                    lit(schema),
                    lit(name),
                    lit(oracle_type(kind))
                );
                let rs = query(s, &sql)?;
                let body: String = rs.rows.iter().map(|r| col(r, 0)).collect();
                if body.is_empty() {
                    return Err(not_found(schema, name));
                }
                let body = body.trim_end().to_string();
                Ok(format!("CREATE OR REPLACE {body}\n/\n"))
            }
            ObjectKind::View => {
                let sql = format!(
                    "SELECT text FROM all_views WHERE owner = {} AND view_name = {}",
                    lit(schema),
                    lit(name)
                );
                let rs = query(s, &sql)?;
                let body = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
                if body.is_empty() {
                    return Err(not_found(schema, name));
                }
                Ok(format!(
                    "CREATE OR REPLACE VIEW {} AS\n{}\n",
                    qualified(dialect, schema, name),
                    body.trim_end()
                ))
            }
            _ => {
                let sql = format!(
                    "SELECT DBMS_METADATA.GET_DDL({}, {}, {}) FROM dual",
                    lit(oracle_type(kind)),
                    lit(name),
                    lit(schema)
                );
                let rs = query(s, &sql)?;
                let body = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
                Ok(format!("{}\n", body.trim()))
            }
        },
        Dialect::Mssql => match kind {
            ObjectKind::Table => table_ddl(s, schema, name),
            _ => {
                let sql = format!(
                    "SELECT OBJECT_DEFINITION(OBJECT_ID({}))",
                    lit(&format!(
                        "{}.{}",
                        quote_ident(dialect, schema),
                        quote_ident(dialect, name)
                    ))
                );
                let rs = query(s, &sql)?;
                let body = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
                if body.trim().is_empty() {
                    return Err(not_found(schema, name));
                }
                // `CREATE PROC` → `CREATE OR ALTER PROC`(2016 SP1+) — 다시 실행하면 곧 수정.
                Ok(format!("{}\nGO\n", to_create_or_alter(body.trim_end())))
            }
        },
        Dialect::Postgres => match kind {
            ObjectKind::Table => table_ddl(s, schema, name),
            ObjectKind::View | ObjectKind::MaterializedView => {
                let sql = format!(
                    "SELECT pg_get_viewdef({}::regclass, true)",
                    lit(&qualified(dialect, schema, name))
                );
                let rs = query(s, &sql)?;
                let body = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
                let kw = if kind == ObjectKind::View {
                    "CREATE OR REPLACE VIEW"
                } else {
                    "CREATE MATERIALIZED VIEW"
                };
                Ok(format!(
                    "{kw} {} AS\n{}\n",
                    qualified(dialect, schema, name),
                    body.trim_end()
                ))
            }
            ObjectKind::Procedure | ObjectKind::Function => {
                // `name` 또는 `name(args)` — 서명이 없으면 첫 오버로드.
                let sql = if name.contains('(') {
                    format!(
                        "SELECT pg_get_functiondef({}::regprocedure)",
                        lit(&format!("{}.{}", quote_ident(dialect, schema), name))
                    )
                } else {
                    format!(
                        "SELECT pg_get_functiondef(p.oid) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE n.nspname = {} AND p.proname = {} ORDER BY p.oid LIMIT 1",
                        lit(schema),
                        lit(name)
                    )
                };
                let rs = query(s, &sql)?;
                let body = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
                if body.trim().is_empty() {
                    return Err(not_found(schema, name));
                }
                Ok(format!("{};\n", body.trim_end().trim_end_matches(';')))
            }
            ObjectKind::Trigger => {
                let sql = format!(
                    "SELECT pg_get_triggerdef(t.oid, true) FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid JOIN pg_namespace n ON n.oid = c.relnamespace WHERE n.nspname = {} AND t.tgname = {} LIMIT 1",
                    lit(schema),
                    lit(name)
                );
                let rs = query(s, &sql)?;
                let body = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
                Ok(format!("{};\n", body.trim_end()))
            }
            _ => Err(no_source(kind)),
        },
        Dialect::Mysql => {
            let what = match kind {
                ObjectKind::Table => "TABLE",
                ObjectKind::View => "VIEW",
                ObjectKind::Procedure => "PROCEDURE",
                ObjectKind::Function => "FUNCTION",
                ObjectKind::Trigger => "TRIGGER",
                _ => return Err(no_source(kind)),
            };
            let rs = query(
                s,
                &format!("SHOW CREATE {what} {}", qualified(dialect, schema, name)),
            )?;
            let idx = if kind == ObjectKind::Procedure || kind == ObjectKind::Function {
                2
            } else {
                1
            };
            let body = rs.rows.first().map(|r| col(r, idx)).unwrap_or_default();
            Ok(format!("{};\n", body.trim_end()))
        }
        Dialect::Sqlite => {
            let rs = query(
                s,
                &format!(
                    "SELECT sql FROM sqlite_master WHERE name = {} AND sql IS NOT NULL",
                    lit(name)
                ),
            )?;
            let body = rs.rows.first().map(|r| col(r, 0)).unwrap_or_default();
            if body.is_empty() {
                return Err(not_found(schema, name));
            }
            Ok(format!("{};\n", body.trim_end()))
        }
        Dialect::Odbc => Err(no_source(kind)),
    }
}

fn not_found(schema: &str, name: &str) -> DbError {
    DbError {
        code: None,
        message: format!("object not found: {schema}.{name}"),
        position: None,
    }
}

fn no_source(kind: ObjectKind) -> DbError {
    DbError {
        code: None,
        message: format!("no source for {}", kind.code()),
        position: None,
    }
}

/// T-SQL `CREATE PROC|PROCEDURE|FUNCTION|VIEW|TRIGGER` → `CREATE OR ALTER …`(이미 그러면 그대로).
fn to_create_or_alter(body: &str) -> String {
    let trimmed = body.trim_start();
    let up = trimmed.to_ascii_uppercase();
    if up.starts_with("CREATE OR ALTER") || !up.starts_with("CREATE") {
        return body.to_string();
    }
    let rest = &trimmed["CREATE".len()..];
    format!("CREATE OR ALTER{rest}")
}

/// 컬럼 목록으로 만든 `CREATE TABLE`(MSSQL·PG·MySQL — 제약·인덱스는 제외 · 골격용).
fn table_ddl(s: &mut dyn Session, schema: &str, name: &str) -> Result<String, DbError> {
    let dialect = s.dialect();
    let cols = columns(s, schema, name)?;
    if cols.is_empty() {
        return Err(not_found(schema, name));
    }
    let mut out = format!("CREATE TABLE {} (\n", qualified(dialect, schema, name));
    let n = cols.len();
    for (i, c) in cols.iter().enumerate() {
        out.push_str("    ");
        out.push_str(&quote_ident(dialect, &c.name));
        out.push(' ');
        out.push_str(&c.data_type);
        if !c.default.is_empty() {
            out.push_str(" DEFAULT ");
            out.push_str(&c.default);
        }
        if !c.nullable {
            out.push_str(" NOT NULL");
        }
        if i + 1 < n {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(");\n");
    Ok(out)
}

// ───────────────────────────────────────────── 컴파일 오류

/// 저장 루틴의 인자 하나(`position` 0 = 함수 반환값 · `name`은 대문자 · 반환값이면 빈 글).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutineArg {
    pub overload: String,
    pub position: i64,
    pub name: String,
    /// 서버가 말한 타입 글자(`REF CURSOR` · `NUMBER` · `VARCHAR2` …).
    pub data_type: String,
    /// `IN` | `OUT` | `IN/OUT`.
    pub in_out: String,
}

/// 호출 이름(`proc` · `pkg.proc` · `schema.proc` · `schema.pkg.proc`)의 인자 목록 — **선언 없이 쓴 바인드의 타입을 서명에서
/// 정하려고**(REF CURSOR OUT을 문자열로 바인드하면 PLS-00306 · 변수 관리 09-21). Oracle(`ALL_ARGUMENTS`) · **PostgreSQL(`pg_proc`
/// — T-151: 어느 자리가 OUT/INOUT인지 알아야 돌아온 값을 그 자리의 바인드로 받는다)** · 그 외 = 빈 목록.
/// 이름이 여러 해석에 맞으면 전부 돌려준다(호출자가 인자 수·이름으로 오버로드를 고른다). 동의어는 풀지 않는다.
pub fn routine_args(s: &mut dyn Session, call_name: &str) -> Result<Vec<RoutineArg>, DbError> {
    if s.dialect() == Dialect::Postgres {
        return pg_routine_args(s, call_name);
    }
    if s.dialect() == Dialect::Mssql {
        return mssql_routine_args(s, call_name);
    }
    if s.dialect() != Dialect::Oracle {
        return Ok(Vec::new());
    }
    let parts: Vec<String> = call_name
        .split('.')
        .map(|p| p.trim().trim_matches('"').to_ascii_uppercase())
        .filter(|p| !p.is_empty())
        .collect();
    let schema = current_schema(s).unwrap_or_default();
    // (소유자, 패키지 또는 없음, 객체) 후보.
    let mut cands: Vec<(String, Option<String>, String)> = Vec::new();
    match parts.as_slice() {
        [n] => cands.push((schema, None, n.clone())),
        [a, b] => {
            cands.push((schema, Some(a.clone()), b.clone()));
            cands.push((a.clone(), None, b.clone()));
        }
        [a, b, c] => cands.push((a.clone(), Some(b.clone()), c.clone())),
        _ => return Ok(Vec::new()),
    }
    let cond: Vec<String> = cands
        .iter()
        .map(|(o, p, n)| {
            let pkg = match p {
                Some(p) => format!("package_name = {}", lit(p)),
                None => "package_name IS NULL".to_string(),
            };
            format!(
                "(owner = {} AND {pkg} AND object_name = {})",
                lit(o),
                lit(n)
            )
        })
        .collect();
    let sql = format!(
        "SELECT NVL(overload, '0'), position, NVL(argument_name, ' '), data_type, in_out \
         FROM all_arguments WHERE data_level = 0 AND ({}) ORDER BY owner, package_name, overload, position",
        cond.join(" OR ")
    );
    let rs = query(s, &sql)?;
    Ok(rs
        .rows
        .iter()
        // 인자 없는 루틴은 `data_type`이 빈 자리표시 행 하나로 온다.
        .filter(|r| !col(r, 3).trim().is_empty())
        .map(|r| RoutineArg {
            overload: col(r, 0),
            position: r.get(1).map(cell_i64).unwrap_or(0),
            name: col(r, 2).trim().to_ascii_uppercase(),
            data_type: col(r, 3).trim().to_ascii_uppercase(),
            in_out: col(r, 4).trim().to_ascii_uppercase(),
        })
        .collect())
}

/// SQL Server 서명 조회문(T-151) — `sys.parameters`: 자리 · 이름(`@` 뗌) · 타입 · OUTPUT 여부. 임시 프로시저(`#이름`)는
/// `tempdb`에 산다 · `db.schema.proc`은 그 DB의 카탈로그를 본다 · 이름은 글자 상수로 인용한다(주입 방지). 반환값(자리 0)은 뺀다.
fn mssql_routine_args_sql(call_name: &str) -> Option<String> {
    let name = call_name.trim();
    if name.is_empty() {
        return None;
    }
    let bare = |p: &str| {
        p.trim()
            .trim_matches(|c| c == '[' || c == ']' || c == '"')
            .to_string()
    };
    let parts: Vec<String> = name.split('.').map(bare).collect();
    let last = parts.last()?;
    let (catalog, object) = if last.starts_with('#') {
        ("tempdb.".to_string(), format!("tempdb..{last}"))
    } else if parts.len() == 3 && !parts[0].is_empty() {
        (
            format!("[{}].", parts[0].replace(']', "]]")),
            name.to_string(),
        )
    } else if parts.len() <= 2 {
        (String::new(), name.to_string())
    } else {
        return None;
    };
    Some(format!(
        "SELECT CAST(p.object_id AS VARCHAR(20)), p.parameter_id, p.name, TYPE_NAME(p.user_type_id), \
         CASE WHEN p.is_output = 1 THEN 'IN/OUT' ELSE 'IN' END \
         FROM {catalog}sys.parameters p WHERE p.object_id = OBJECT_ID(N{}) AND p.parameter_id >= 1 \
         ORDER BY p.parameter_id",
        lit(&object)
    ))
}

fn mssql_routine_args(s: &mut dyn Session, call_name: &str) -> Result<Vec<RoutineArg>, DbError> {
    let Some(sql) = mssql_routine_args_sql(call_name) else {
        return Ok(Vec::new());
    };
    let rs = query(s, &sql)?;
    Ok(rs
        .rows
        .iter()
        .map(|r| RoutineArg {
            overload: col(r, 0),
            position: r.get(1).map(cell_i64).unwrap_or(0),
            name: col(r, 2)
                .trim()
                .trim_start_matches('@')
                .to_ascii_uppercase(),
            data_type: col(r, 3).trim().to_ascii_uppercase(),
            in_out: col(r, 4).trim().to_ascii_uppercase(),
        })
        .collect())
}

/// PostgreSQL 서명 조회문 — `[스키마.]이름`(인용 부호는 벗기고 소문자로 · 스키마가 없으면 검색 경로에 보이는 것 ·
/// `pg_temp` = 이 세션의 임시 스키마). 인자마다 한 줄: (오버로드 = 함수 oid · 자리 1부터 · 이름 · 타입 · 방향).
/// `proargmodes`가 NULL이면 전부 IN이다(`unnest`가 모자란 배열을 NULL로 채운다). `t`(TABLE 열) = OUT · `v`(VARIADIC) = IN.
fn pg_routine_args_sql(call_name: &str) -> Option<String> {
    let parts: Vec<String> = call_name
        .split('.')
        .map(|p| p.trim().trim_matches('"').to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    let (schema, name) = match parts.as_slice() {
        [n] => (None, n.clone()),
        [s, n] => (Some(s.clone()), n.clone()),
        _ => return None,
    };
    let scope = match schema.as_deref() {
        None => "pg_function_is_visible(p.oid)".to_string(),
        Some("pg_temp") => "p.pronamespace = pg_my_temp_schema()".to_string(),
        Some(s) => format!("n.nspname = {}", lit(s)),
    };
    Some(format!(
        "SELECT p.oid::text, a.ord, COALESCE(a.name, ''), format_type(a.typ, NULL), \
         CASE a.mode WHEN 'o' THEN 'OUT' WHEN 't' THEN 'OUT' WHEN 'b' THEN 'IN/OUT' ELSE 'IN' END \
         FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace, \
         LATERAL unnest(COALESCE(p.proallargtypes, p.proargtypes::oid[]), p.proargmodes, p.proargnames) \
         WITH ORDINALITY AS a(typ, mode, name, ord) \
         WHERE p.proname = {} AND {scope} ORDER BY p.oid, a.ord",
        lit(&name)
    ))
}

fn pg_routine_args(s: &mut dyn Session, call_name: &str) -> Result<Vec<RoutineArg>, DbError> {
    let Some(sql) = pg_routine_args_sql(call_name) else {
        return Ok(Vec::new());
    };
    let rs = query(s, &sql)?;
    Ok(rs
        .rows
        .iter()
        .map(|r| RoutineArg {
            overload: col(r, 0),
            position: r.get(1).map(cell_i64).unwrap_or(0),
            name: col(r, 2).trim().to_ascii_uppercase(),
            data_type: col(r, 3).trim().to_ascii_uppercase(),
            in_out: col(r, 4).trim().to_ascii_uppercase(),
        })
        .collect())
}

/// 컴파일 오류(Oracle `ALL_ERRORS` — 그 외 방언은 실행 오류가 곧 컴파일 오류라 빈 목록).
pub fn compile_errors(
    s: &mut dyn Session,
    schema: &str,
    name: &str,
) -> Result<Vec<CompileError>, DbError> {
    if s.dialect() != Dialect::Oracle {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT line, position, text, attribute FROM all_errors WHERE owner = {} AND name = {} ORDER BY sequence",
        lit(schema),
        lit(name)
    );
    let rs = query(s, &sql)?;
    Ok(rs
        .rows
        .iter()
        .map(|r| CompileError {
            line: r.first().map(cell_i64).unwrap_or(0),
            col: r.get(1).map(cell_i64).unwrap_or(0),
            text: col(r, 2).trim_end().to_string(),
            severity: col(r, 3),
        })
        .collect())
}

/// `CREATE [OR REPLACE|OR ALTER] <종류> [schema.]name …` 머리에서 (종류, 스키마, 이름)을 읽는다 — 컴파일 뒤 오류 조회용.
#[must_use]
pub fn parse_create_header(sql: &str) -> Option<(ObjectKind, Option<String>, String)> {
    let mut toks = sql
        .split(|c: char| c.is_whitespace() || c == '(')
        .filter(|t| !t.is_empty());
    if !toks.next()?.eq_ignore_ascii_case("CREATE") {
        return None;
    }
    let mut t = toks.next()?;
    if t.eq_ignore_ascii_case("OR") {
        let _ = toks.next()?; // REPLACE | ALTER
        t = toks.next()?;
    }
    while matches!(
        t.to_ascii_uppercase().as_str(),
        "EDITIONABLE" | "NONEDITIONABLE" | "FORCE" | "NO" | "DEFINER" | "TEMPORARY" | "TEMP"
    ) {
        t = toks.next()?;
    }
    let kind = match t.to_ascii_uppercase().as_str() {
        "PROCEDURE" | "PROC" => ObjectKind::Procedure,
        "FUNCTION" => ObjectKind::Function,
        "PACKAGE" => {
            // PACKAGE BODY?
            let next = toks.next()?;
            if next.eq_ignore_ascii_case("BODY") {
                return name_from(toks.next()?, ObjectKind::PackageBody);
            }
            return name_from(next, ObjectKind::Package);
        }
        "TRIGGER" => ObjectKind::Trigger,
        "VIEW" => ObjectKind::View,
        "TYPE" => {
            let next = toks.next()?;
            if next.eq_ignore_ascii_case("BODY") {
                return name_from(toks.next()?, ObjectKind::Type);
            }
            return name_from(next, ObjectKind::Type);
        }
        "MATERIALIZED" => {
            let _ = toks.next()?; // VIEW
            ObjectKind::MaterializedView
        }
        "TABLE" => ObjectKind::Table,
        "SEQUENCE" => ObjectKind::Sequence,
        "INDEX" => ObjectKind::Index,
        "SYNONYM" => ObjectKind::Synonym,
        _ => return None,
    };
    name_from(toks.next()?, kind)
}

fn name_from(tok: &str, kind: ObjectKind) -> Option<(ObjectKind, Option<String>, String)> {
    let clean = |s: &str| {
        s.trim_matches(|c| c == '"' || c == '[' || c == ']' || c == '`' || c == ';')
            .to_string()
    };
    let tok = tok.trim_end_matches(';');
    if tok.is_empty() {
        return None;
    }
    match tok.split_once('.') {
        Some((sch, nm)) => Some((kind, Some(clean(sch)), clean(nm))),
        None => Some((kind, None, clean(tok))),
    }
}

/// `SELECT *` 템플릿(탐색기 더블클릭 · CLI).
#[must_use]
pub fn select_template(dialect: Dialect, schema: &str, name: &str) -> String {
    let q = qualified(dialect, schema, name);
    match dialect {
        Dialect::Mssql => format!("SELECT TOP 200 * FROM {q};"),
        Dialect::Oracle => format!("SELECT * FROM {q} WHERE ROWNUM <= 200;"),
        _ => format!("SELECT * FROM {q} LIMIT 200;"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn table_function_flag_per_dialect() {
        use super::{table_function, Dialect, ObjectKind};
        assert!(table_function(
            Dialect::Oracle,
            ObjectKind::Function,
            "PIPELINED"
        ));
        assert!(!table_function(Dialect::Oracle, ObjectKind::Function, ""));
        assert!(table_function(Dialect::Mssql, ObjectKind::Function, "TF"));
        assert!(!table_function(Dialect::Mssql, ObjectKind::Function, "FN"));
        assert!(table_function(
            Dialect::Postgres,
            ObjectKind::Function,
            "SETOF a integer"
        ));
        assert!(!table_function(
            Dialect::Postgres,
            ObjectKind::Procedure,
            "SETOF"
        ));
        assert!(!table_function(Dialect::Sqlite, ObjectKind::Function, "TF"));
    }

    /// SQL Server 서명 조회문: 보통 이름 = 지금 DB · `#임시` = tempdb · 세 마디 = 그 DB의 카탈로그 · 대괄호 · 인용.
    #[test]
    fn mssql_routine_args_sql_scopes() {
        let q = super::mssql_routine_args_sql("dbo.sp_x").expect("sql");
        assert!(
            q.contains("FROM sys.parameters") && q.contains("OBJECT_ID(N'dbo.sp_x')"),
            "{q}"
        );
        let q = super::mssql_routine_args_sql("#tmp_p").expect("sql");
        assert!(
            q.contains("FROM tempdb.sys.parameters") && q.contains("N'tempdb..#tmp_p'"),
            "{q}"
        );
        let q = super::mssql_routine_args_sql("[Other].[dbo].[p]").expect("sql");
        assert!(q.contains("FROM [Other].sys.parameters"), "{q}");
        let q = super::mssql_routine_args_sql("x'y").expect("sql");
        assert!(q.contains("N'x''y'"), "{q}");
        assert!(super::mssql_routine_args_sql("a.b.c.d").is_none());
    }

    /// PostgreSQL 서명 조회문: 이름만 = 검색 경로 · `스키마.이름` = 그 스키마 · `pg_temp` = 이 세션의 임시 스키마 · 인용·대소문자 ·
    /// 점이 셋 이상이면 조회하지 않는다 · 값은 글자 상수로 인용된다(주입 방지).
    #[test]
    fn pg_routine_args_sql_scopes() {
        let q = super::pg_routine_args_sql("My_Proc").expect("sql");
        assert!(q.contains("p.proname = 'my_proc'") && q.contains("pg_function_is_visible(p.oid)"));
        let q = super::pg_routine_args_sql("Sales.\"Do_It\"").expect("sql");
        assert!(q.contains("p.proname = 'do_it'") && q.contains("n.nspname = 'sales'"));
        let q = super::pg_routine_args_sql("pg_temp.f").expect("sql");
        assert!(q.contains("pg_my_temp_schema()"));
        assert!(super::pg_routine_args_sql("a.b.c").is_none());
        let q = super::pg_routine_args_sql("x'y").expect("sql");
        assert!(q.contains("'x''y'"), "{q}");
    }

    use super::*;

    #[test]
    fn kinds_parse_and_labels() {
        assert_eq!(ObjectKind::parse("procs"), Some(ObjectKind::Procedure));
        assert_eq!(ObjectKind::parse("Tables"), Some(ObjectKind::Table));
        assert_eq!(ObjectKind::parse("x"), None);
        assert_eq!(ObjectKind::PackageBody.folder(), "Package Bodies");
        assert!(kinds_for(Dialect::Sqlite).contains(&ObjectKind::Trigger));
    }

    #[test]
    fn create_header_parses_kind_schema_name() {
        assert_eq!(
            parse_create_header(
                "CREATE OR REPLACE PROCEDURE BISCM.SP_X(p IN NUMBER) AS BEGIN NULL; END;"
            ),
            Some((ObjectKind::Procedure, Some("BISCM".into()), "SP_X".into()))
        );
        assert_eq!(
            parse_create_header("create or replace package body pkg_a is end;"),
            Some((ObjectKind::PackageBody, None, "pkg_a".into()))
        );
        assert_eq!(
            parse_create_header("CREATE OR ALTER PROC [dbo].[P1] AS SELECT 1"),
            Some((ObjectKind::Procedure, Some("dbo".into()), "P1".into()))
        );
        assert_eq!(
            parse_create_header(
                "CREATE FUNCTION f(a int) RETURNS int AS $$ SELECT 1 $$ LANGUAGE sql"
            ),
            Some((ObjectKind::Function, None, "f".into()))
        );
        assert_eq!(parse_create_header("SELECT 1"), None);
    }

    #[test]
    fn quoting_and_templates() {
        assert_eq!(qualified(Dialect::Mssql, "dbo", "T"), "[dbo].[T]");
        assert_eq!(qualified(Dialect::Oracle, "BISCM", "T"), "\"BISCM\".\"T\"");
        assert_eq!(fmt_type("VARCHAR2", "20", "", ""), "VARCHAR2(20)");
        assert_eq!(fmt_type("NUMBER", "22", "10", "2"), "NUMBER(10,2)");
        assert_eq!(fmt_type("DATE", "7", "", ""), "DATE");
        assert_eq!(
            to_create_or_alter("CREATE PROC p AS SELECT 1"),
            "CREATE OR ALTER PROC p AS SELECT 1"
        );
        assert!(select_template(Dialect::Postgres, "public", "t").ends_with("LIMIT 200;"));
    }
}
