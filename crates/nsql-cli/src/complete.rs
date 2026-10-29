//! `nsql shell` Tab 완성(T-453 · D-393) — 문맥은 GUI와 같은 `nsql_script::intel::context_at` · 후보 = 셸 명령 · 키워드 · 내장 함수(정적)
//! 그리고 접속 뒤 **현재 스키마의 객체·컬럼**(첫 Tab에서 지연 적재 · 이후 캐시 · 접속/스키마가 바뀌면 [`MetaCache::reset`] · docs/61 §1-8).
//! 반환 = (바꿀 시작 글자 인덱스 · 후보들). 후보 글자의 대소문자 = 친 접두를 따른다(`intel::apply_case` `match`).

use nsql_catalog::ObjectKind;
use nsql_core::{Dialect, Session};
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

/// 후보를 구한다 — `cursor`는 글자 인덱스 · 세션이 없으면 정적 후보만.
pub(crate) fn complete(
    line: &str,
    cursor: usize,
    dialect: Option<Dialect>,
    session: Option<&mut (dyn Session + 'static)>,
    cache: &mut MetaCache,
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
    let cased = |k: &str| intel::apply_case(k, &prefix, "match");
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
