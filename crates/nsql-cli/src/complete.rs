//! `nsql shell` Tab 완성(T-453 · D-393) — 문맥은 GUI와 같은 `nsql_script::intel::context_at` · 후보 = 셸 명령 · 키워드 · 내장 함수(정적)
//! 그리고 접속 뒤 **현재 스키마의 객체·컬럼**(첫 Tab에서 지연 적재 · 이후 캐시 · 접속/스키마가 바뀌면 [`MetaCache::reset`] · docs/61 §1-8).
//! 반환 = (바꿀 시작 글자 인덱스 · 후보들). 후보 글자의 대소문자 = GUI 설정 `intel.insert_case`(기본 "default" · `intel::apply_case`).
//! ★ GUI 자동완성 설정을 함께 따른다(사용자 10-29 ⑥⑦ · [`CompleteCfg`]): 테이블 확정 때 자동 별칭(`intel.insert_alias` ·
//!   `intel.alias_style` · `intel.alias_upper` · 뒤에 별칭이 있으면 안 붙임 = `Context::alias_follows`) = [`decorate`].

use nsql_catalog::ObjectKind;
use nsql_core::{alias, Dialect, Session};
use nsql_script::intel::{self, CtxKind, Want};
use std::collections::HashMap;

/// 셸 명령·별칭(`\?` 도움말 표와 같은 것).
const SHELL_CMDS: &[&str] = &[
    "\\?",
    "\\h",
    "\\d",
    "\\dt",
    "\\dv",
    "\\i",
    "\\c",
    "\\connect",
    "\\q",
    "\\more",
    "\\all",
    "\\count",
    ".tables",
    ".views",
    ".schema",
    ".read",
];

/// GUI와 같은 자동완성 설정(docs/47 · 94): 별칭 3키 + 삽입 대소문자. 시험·설정 없음 = [`Default`](별칭 끔 · "match").
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompleteCfg {
    pub insert_alias: bool,
    pub alias_letters: bool,
    pub alias_upper: bool,
    pub case: String,
}

impl Default for CompleteCfg {
    fn default() -> Self {
        CompleteCfg {
            insert_alias: false,
            alias_letters: false,
            alias_upper: true,
            case: "match".into(),
        }
    }
}

impl CompleteCfg {
    /// 설정 파일(GUI와 공유)에서 — 없으면 기본값.
    pub(crate) fn from_settings(st: Option<&nsql_settings::Settings>) -> Self {
        let Some(s) = st else {
            return Self::default();
        };
        CompleteCfg {
            insert_alias: s.flag("intel.insert_alias"),
            alias_letters: s.get("intel.alias_style").unwrap_or("abbr") == "letters",
            alias_upper: s.flag("intel.alias_upper"),
            case: s.get("intel.insert_case").unwrap_or("default").to_string(),
        }
    }
}

/// ★ 확정한 후보 하나를 줄에 넣을 글로(사용자 10-29 ⑥): 관계 자리(FROM/JOIN) · 문법이 별칭을 허용 · 뒤에 별칭이 없을 때만
/// ` 별칭`을 덧붙인다(GUI `Intel::pick`과 같은 규칙 · 부품 `nsql_core::alias::make` · 같은 문장의 별칭은 피함). 아니면 그대로.
pub(crate) fn decorate(
    line: &str,
    cursor: usize,
    dialect: Option<Dialect>,
    chosen: &str,
    cfg: &CompleteCfg,
) -> String {
    if !cfg.insert_alias {
        return chosen.to_string();
    }
    let byte = line
        .char_indices()
        .nth(cursor)
        .map_or(line.len(), |(i, _)| i);
    let ctx = intel::context_at(line, byte, dialect);
    if ctx.kind != CtxKind::Relation || !ctx.alias_ok || ctx.alias_follows {
        return chosen.to_string();
    }
    let taken: Vec<&str> = ctx
        .aliases
        .iter()
        .filter(|a| {
            !(a.alias.eq_ignore_ascii_case(&ctx.prefix)
                && a.table.eq_ignore_ascii_case(&ctx.prefix))
        })
        .map(|a| a.alias.as_str())
        .collect();
    let a = alias::make(chosen, &taken, cfg.alias_letters, cfg.alias_upper);
    format!("{chosen} {a}")
}

/// 접속 세션의 메타 캐시(현재 스키마 객체 · 스키마 목록 · 테이블별 컬럼).
#[derive(Default)]
pub(crate) struct MetaCache {
    loaded: bool,
    schema: String,
    objects: Vec<(String, ObjectKind)>,
    schemas: Vec<String>,
    /// `SCHEMA.TABLE`(대문자) → 컬럼 이름.
    columns: HashMap<String, Vec<String>>,
}

impl MetaCache {
    /// 접속·스키마가 바뀌었다 — 다음 Tab에서 다시 읽는다.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    fn ensure(&mut self, s: &mut dyn Session) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.schema = nsql_catalog::current_schema(s).unwrap_or_default();
        let scope: Vec<String> = if self.schema.is_empty() {
            Vec::new()
        } else {
            vec![self.schema.clone()]
        };
        if let Ok((list, _)) = nsql_catalog::name_index(s, &scope, 5000) {
            self.objects = list.into_iter().map(|e| (e.name, e.kind)).collect();
        }
        self.schemas =
            nsql_catalog::schemas_opt(s, nsql_catalog::SchemaOpts::default()).unwrap_or_default();
    }

    fn has_object(&self, name: &str) -> bool {
        self.objects
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    fn columns_of(
        &mut self,
        s: &mut dyn Session,
        schema: Option<&str>,
        table: &str,
    ) -> Vec<String> {
        let schema = schema.unwrap_or(&self.schema).to_string();
        let key = format!("{}.{}", schema.to_uppercase(), table.to_uppercase());
        if let Some(c) = self.columns.get(&key) {
            return c.clone();
        }
        let cols: Vec<String> = nsql_catalog::columns(s, &schema, table)
            .map(|v| v.into_iter().map(|c| c.name).collect())
            .unwrap_or_default();
        self.columns.insert(key, cols.clone());
        cols
    }

    fn objects_in(&self, s: &mut dyn Session, schema: &str) -> Vec<String> {
        if schema.eq_ignore_ascii_case(&self.schema) {
            return self.names(|k| matches!(k, ObjectKind::Table | ObjectKind::View));
        }
        let mut out = Vec::new();
        for kind in [ObjectKind::Table, ObjectKind::View] {
            if let Ok(list) = nsql_catalog::objects(s, schema, kind) {
                out.extend(list.into_iter().map(|o| o.name));
            }
        }
        out
    }

    fn names(&self, keep: impl Fn(ObjectKind) -> bool) -> Vec<String> {
        self.objects
            .iter()
            .filter(|(_, k)| keep(*k))
            .map(|(n, _)| n.clone())
            .collect()
    }
}

/// 이 문장의 첫 낱말이 객체 이름·컬럼을 바꿀 수 있는가(DDL · 스크립트 실행) — 그러면 호출자가 [`MetaCache::reset`]
/// (사용자 재현 10-29: 적재 뒤 `CREATE TABLE`을 같은 세션에서 만들면 다음 Tab에 새 표가 안 보였다).
pub(crate) fn invalidates_meta(first: &str) -> bool {
    let u = first.to_ascii_uppercase();
    matches!(u.as_str(), "CREATE" | "DROP" | "ALTER" | "RENAME" | "START")
        || u.starts_with('@')
        || u == "\\I"
        || u == ".READ"
}

/// 접두 걸러내기(대소문자 무시) · 그룹 안 이름순 · 같은 글(대소문자 무시)은 앞 것만.
fn pick(groups: Vec<Vec<String>>, prefix: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for mut g in groups {
        g.retain(|c| {
            c.len() >= prefix.len() && c[..].to_lowercase().starts_with(&prefix.to_lowercase())
        });
        g.sort_by_key(|c| c.to_lowercase());
        for c in g {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(&c)) {
                out.push(c);
            }
        }
    }
    out
}

/// 후보를 구한다 — `cursor`는 글자 인덱스 · 세션이 없으면 정적 후보만(설정 = 기본값 · 시험용).
#[cfg(test)]
pub(crate) fn complete(
    line: &str,
    cursor: usize,
    dialect: Option<Dialect>,
    session: Option<&mut (dyn Session + 'static)>,
    cache: &mut MetaCache,
) -> (usize, Vec<String>) {
    complete_with(
        line,
        cursor,
        dialect,
        session,
        cache,
        &CompleteCfg::default(),
    )
}

/// 후보를 구한다 — `cursor`는 글자 인덱스 · 세션이 없으면 정적 후보만 · 대소문자 = `cfg.case`.
pub(crate) fn complete_with(
    line: &str,
    cursor: usize,
    dialect: Option<Dialect>,
    session: Option<&mut (dyn Session + 'static)>,
    cache: &mut MetaCache,
    cfg: &CompleteCfg,
) -> (usize, Vec<String>) {
    let byte = line
        .char_indices()
        .nth(cursor)
        .map_or(line.len(), |(i, _)| i);
    let head = &line[..byte];
    let trimmed = head.trim_start();
    if !trimmed.is_empty()
        && !trimmed.contains(char::is_whitespace)
        && (trimmed.starts_with('\\') || trimmed.starts_with('.'))
    {
        let start = cursor - trimmed.chars().count();
        let cmds = SHELL_CMDS.iter().map(|s| s.to_string()).collect();
        return (start, pick(vec![cmds], trimmed));
    }
    let ctx = intel::context_at(line, byte, dialect);
    if ctx.star.is_some() {
        return (cursor, Vec::new());
    }
    let prefix = ctx.prefix.clone();
    let start = cursor.saturating_sub(prefix.chars().count());
    let cased = |k: &str| intel::apply_case(k, &prefix, &cfg.case);
    let keywords = || -> Vec<String> { intel::keywords_for(dialect).map(cased).collect() };
    let functions = || -> Vec<String> {
        nsql_script::builtins::functions(dialect)
            .iter()
            .map(|b| cased(b.name))
            .collect()
    };
    let mut groups: Vec<Vec<String>> = Vec::new();
    match &ctx.kind {
        CtxKind::None => return (cursor, Vec::new()),
        CtxKind::Start => groups.push(keywords()),
        CtxKind::Member { qualifier } => {
            if let Some(s) = session {
                cache.ensure(s);
                let target = intel::resolve_alias(&ctx.aliases, qualifier)
                    .filter(|a| !a.local)
                    .map(|a| (a.schema.clone(), a.table.clone()))
                    .or_else(|| {
                        cache
                            .has_object(qualifier)
                            .then(|| (None, qualifier.clone()))
                    });
                if let Some((schema, table)) = target {
                    groups.push(cache.columns_of(s, schema.as_deref(), &table));
                } else if cache
                    .schemas
                    .iter()
                    .any(|x| x.eq_ignore_ascii_case(qualifier))
                {
                    groups.push(cache.objects_in(s, qualifier));
                }
            }
        }
        CtxKind::Relation => {
            if let Some(s) = session {
                cache.ensure(s);
                groups.push(cache.names(|k| {
                    matches!(
                        k,
                        ObjectKind::Table
                            | ObjectKind::View
                            | ObjectKind::MaterializedView
                            | ObjectKind::Synonym
                    )
                }));
                groups.push(cache.schemas.clone());
            }
        }
        CtxKind::Want(w) => {
            // 정적 표(메타 불필요 · GUI와 같은 원천 `builtins`): 자료형 · 정렬 · 시스템 프로시저 파라미터 · `%TYPE`/`%ROWTYPE`.
            match w {
                Want::DataType => groups.push(
                    nsql_script::builtins::types(dialect)
                        .iter()
                        .map(|b| cased(b.name))
                        .collect(),
                ),
                Want::Collation => groups.push(
                    nsql_script::builtins::collations(dialect)
                        .iter()
                        .map(|b| b.name.to_string())
                        .collect(),
                ),
                Want::ProcParam(name) => {
                    if let Some(p) = nsql_script::builtins::proc_params(dialect, name) {
                        groups.push(p.params.iter().map(|(n, _)| n.to_string()).collect());
                    }
                }
                Want::TypeAttr => groups.push(vec![cased("TYPE"), cased("ROWTYPE")]),
                _ => {}
            }
            if let Some(s) = session {
                cache.ensure(s);
                match w {
                    Want::Schema | Want::Database => groups.push(cache.schemas.clone()),
                    Want::Routine => groups.push(cache.names(|k| {
                        matches!(
                            k,
                            ObjectKind::Procedure | ObjectKind::Function | ObjectKind::Package
                        )
                    })),
                    Want::Kind(code) => {
                        if let Some(kind) = ObjectKind::parse(code) {
                            groups.push(cache.names(|k| k == kind));
                        }
                    }
                    // 사용자 정의 타입(`ObjectKind::Type`)도 자료형 자리에.
                    Want::DataType => groups.push(cache.names(|k| k == ObjectKind::Type)),
                    _ => {}
                }
            }
        }
        CtxKind::Expr | CtxKind::ExecArgs { .. } => {
            if let Some(s) = session {
                cache.ensure(s);
                let mut cols = Vec::new();
                for a in ctx.aliases.iter().filter(|a| !a.local) {
                    cols.extend(cache.columns_of(s, a.schema.as_deref(), &a.table));
                }
                groups.push(cols);
            }
            groups.push(functions());
            groups.push(keywords());
        }
    }
    (start, pick(groups, &prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 세션 없이: 문장 시작 = 키워드(접두 대소문자 따름) · 셸 명령 · 식 자리 = 함수+키워드 · 문자열 안 = 없음.
    #[test]
    fn static_candidates_without_session() {
        let mut cache = MetaCache::default();
        let (start, c) = complete("SEL", 3, Some(Dialect::Oracle), None, &mut cache);
        assert_eq!(start, 0);
        assert_eq!(c, ["SELECT"]);
        let (_, c) = complete("sel", 3, Some(Dialect::Oracle), None, &mut cache);
        assert_eq!(c, ["select"]);
        let (start, c) = complete("  \\d", 4, None, None, &mut cache);
        assert_eq!(start, 2);
        assert_eq!(c, ["\\d", "\\dt", "\\dv"]);
        let (start, c) = complete("SELECT NV", 9, Some(Dialect::Oracle), None, &mut cache);
        assert_eq!(start, 7);
        assert!(c.iter().any(|x| x == "NVL"), "{c:?}");
        let (_, c) = complete("SELECT 'sel", 11, Some(Dialect::Oracle), None, &mut cache);
        assert!(c.is_empty(), "문자열 안 = 후보 없음");
        // 커서가 줄 가운데여도 그 앞 조각만.
        let (start, c) = complete("SEL FROM t", 3, Some(Dialect::Oracle), None, &mut cache);
        assert_eq!((start, c), (0, vec!["SELECT".to_string()]));
    }

    /// 세션 있음(SQLite 메모리): `FROM` 자리 = 테이블·뷰 + 스키마 · 접두 걸러내기 · `별칭.` = 컬럼 · 식 자리 = 별칭 테이블 컬럼 먼저.
    #[test]
    #[cfg(feature = "drivers-builtin")]
    fn relation_and_member_candidates_with_sqlite() {
        use nsql_core::ExecRequest;
        let spec = nsql_script::ConnectSpec::parse("sqlite::memory:").expect("spec");
        let mut s = nsql_drivers::open(&spec, Dialect::Sqlite).expect("open");
        for sql in [
            "CREATE TABLE emp (id INTEGER, name TEXT)",
            "CREATE TABLE dept (dno INTEGER)",
            "CREATE VIEW v_emp AS SELECT id FROM emp",
        ] {
            s.execute(&ExecRequest {
                sql: sql.into(),
                params: Vec::new(),
            })
            .expect(sql);
        }
        let d = Some(Dialect::Sqlite);
        let mut cache = MetaCache::default();
        let (start, c) = complete("select * from ", 14, d, Some(s.as_mut()), &mut cache);
        assert_eq!(start, 14);
        assert_eq!(c, ["dept", "emp", "v_emp", "main"], "{c:?}");
        let (start, c) = complete("select * from E", 15, d, Some(s.as_mut()), &mut cache);
        assert_eq!((start, c), (14, vec!["emp".to_string()]));
        let (start, c) = complete("select e. from emp e", 9, d, Some(s.as_mut()), &mut cache);
        assert_eq!((start, c), (9, vec!["id".to_string(), "name".to_string()]));
        let (_, c) = complete("select na from emp", 9, d, Some(s.as_mut()), &mut cache);
        assert_eq!(c.first().map(String::as_str), Some("name"), "{c:?}");
        // 사용자 재현(10-29): 적재 뒤 같은 세션에서 CREATE TABLE → 셸은 [`invalidates_meta`]로 캐시를 비우고 다음 Tab에 새 표.
        s.execute(&ExecRequest {
            sql: "CREATE TABLE tb_1 (c1 int, c2 int)".into(),
            params: Vec::new(),
        })
        .expect("create");
        let (_, c) = complete("select * from tb", 16, d, Some(s.as_mut()), &mut cache);
        assert!(c.is_empty(), "비우기 전 = 캐시 그대로 {c:?}");
        assert!(invalidates_meta("create"));
        cache.reset();
        let (_, c) = complete("select * from tb", 16, d, Some(s.as_mut()), &mut cache);
        assert_eq!(c, ["tb_1"], "{c:?}");
    }

    /// 확정 꾸밈(⑥): 별칭 설정이 켜지면 FROM 자리 테이블에 ` A`(글자 방식) · 뒤에 별칭이 있으면 그대로 · 같은 문장의 별칭은 건너뜀 ·
    /// 식 자리·설정 끔 = 그대로.
    #[test]
    fn decorate_adds_alias_only_in_relation_slot_without_following_alias() {
        let d = Some(Dialect::Oracle);
        let on = CompleteCfg {
            insert_alias: true,
            alias_letters: true,
            alias_upper: true,
            case: "match".into(),
        };
        assert_eq!(decorate("select * from tb", 16, d, "tb_1", &on), "tb_1 A");
        assert_eq!(
            decorate("select * from tb A where 1=1", 16, d, "tb_1", &on),
            "tb_1",
            "뒤에 별칭 = 그대로"
        );
        assert_eq!(
            decorate("select * from emp A join de", 27, d, "dept", &on),
            "dept B",
            "같은 문장의 A는 건너뜀"
        );
        assert_eq!(
            decorate("select na from emp", 9, d, "name", &on),
            "name",
            "식 자리"
        );
        assert_eq!(
            decorate("alter table em", 14, d, "emp", &on),
            "emp",
            "별칭 불허 자리"
        );
        let abbr = CompleteCfg {
            alias_letters: false,
            alias_upper: false,
            ..on
        };
        let got = decorate("select * from sales_cust", 24, d, "sales_customer", &abbr);
        assert!(
            got.starts_with("sales_customer ") && got.len() > 15,
            "{got}"
        );
        assert_eq!(
            decorate("select * from tb", 16, d, "tb_1", &CompleteCfg::default()),
            "tb_1",
            "설정 끔"
        );
    }

    /// 캐시를 비워야 하는 첫 낱말 = DDL · 스크립트 실행 · 그 밖(SELECT·INSERT·SET)은 아니다.
    #[test]
    fn ddl_and_script_commands_invalidate_meta() {
        for w in [
            "create", "DROP", "Alter", "RENAME", "@x.sql", "@@x", "\\i", ".read", "start",
        ] {
            assert!(invalidates_meta(w), "{w}");
        }
        for w in ["select", "insert", "update", "delete", "set", "show", "\\d"] {
            assert!(!invalidates_meta(w), "{w}");
        }
    }

    /// 걸러내기: 대소문자 무시 접두 · 그룹 순서 유지 · 그룹 안 이름순 · 중복 제거.
    #[test]
    fn pick_filters_orders_and_dedupes() {
        let g = vec![
            vec!["emp_b".into(), "EMP_A".into(), "dept".into()],
            vec!["EMP_A".into(), "emp_c".into()],
        ];
        assert_eq!(pick(g, "em"), ["EMP_A", "emp_b", "emp_c"]);
    }
}
