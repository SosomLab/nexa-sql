//! 카탈로그(docs/28 · 사용자 09-15 "객체 탐색기 · 객체 생성/수정") — **`Session` 포트 위에서** 방언별 메타 SQL을 실행해
//! 스키마 · 오브젝트 목록 · 컬럼 · 소스(DDL) · 컴파일 오류를 돌려준다. GUI 탐색기와 CLI `nsql cat`이 같은 함수를 부른다.
//!
//! - 드라이버는 바뀌지 않는다(메타 조회 = 일반 SQL). 어댑터가 카탈로그를 직접 제공하는 확장(RPC `catalog.*`)은 후속.
//! - 한 호출 = 한 단계만(직계 자식 · docs/28 §0). 무거운 것(행 수 · DDL)은 요청 시.
//! - 문자열 리터럴은 `'` 이스케이프 · 식별자는 방언 인용으로 감싼다(`"NAME"` · `[name]`).

#![cfg_attr(test, allow(clippy::unwrap_used))]

use nsql_core::{DbError, Dialect, ExecRequest, ResultSet, Session, Value};

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

// ───────────────────────────────────────────── 오브젝트 목록

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
            let sql = format!(
                "SELECT object_name, status, TO_CHAR(last_ddl_time, 'YYYY-MM-DD HH24:MI:SS'), '' FROM all_objects WHERE owner = {} AND object_type = {} AND (object_name NOT LIKE 'BIN$%') ORDER BY object_name",
                lit(schema),
                lit(oracle_type(kind))
            );
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
                    "SELECT p.proname, '', '', pg_get_function_identity_arguments(p.oid) FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace WHERE n.nspname = {} AND p.prokind = '{}' ORDER BY p.proname",
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
            "SELECT c.name, t.name, CASE WHEN t.name IN ('nchar','nvarchar') AND c.max_length > 0 THEN c.max_length / 2 ELSE c.max_length END, c.precision, c.scale, CASE WHEN c.is_nullable = 1 THEN 'Y' ELSE 'N' END, c.column_id, ISNULL(d.definition, '') FROM sys.columns c JOIN sys.types t ON t.user_type_id = c.user_type_id LEFT JOIN sys.default_constraints d ON d.object_id = c.default_object_id WHERE c.object_id = OBJECT_ID({}) ORDER BY c.column_id",
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
