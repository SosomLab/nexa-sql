//! `nsql cat` — 카탈로그(docs/28 · 사용자 09-15) · GUI 탐색기와 같은 `nsql-catalog` 함수를 부른다.
//!
//! ```text
//! nsql cat -c <target> [-s schema] schemas            스키마 목록
//! nsql cat -c <target> kinds                          이 방언의 오브젝트 종류
//! nsql cat -c <target> [-s schema] tables|views|procs|funcs|packages|bodies|sequences|triggers|indexes|synonyms|types
//! nsql cat -c <target> [-s schema] columns <object>   컬럼(DESC)
//! nsql cat -c <target> [-s schema] source <kind> <name>   소스/DDL(편집기에 열어 고친 뒤 `nsql run`으로 재컴파일)
//! nsql cat -c <target> [-s schema] errors <name>      컴파일 오류(Oracle ALL_ERRORS)
//! ```
//! 출력은 `-f grid|csv|tsv|json|jsonl`(결과 집합과 같은 작성기) · `source`는 본문 그대로.

use crate::{opener, resolve_target, Opts};
use nsql_catalog::ObjectKind;
use nsql_core::{Column, Dialect, ResultSet, Session, Value};
use std::io::{self, Write};

fn rs(cols: &[&str], rows: Vec<Vec<String>>) -> ResultSet {
    ResultSet {
        columns: cols
            .iter()
            .map(|c| Column {
                name: (*c).to_string(),
                type_name: String::new(),
            })
            .collect(),
        rows: rows
            .into_iter()
            .map(|r| r.into_iter().map(Value::Str).collect())
            .collect(),
    }
}

fn print_rs(o: &Opts, dialect: Dialect, set: &ResultSet) {
    let out = io::stdout();
    let mut out = out.lock();
    let _ = nsql_io::write_result_set(&mut out, set, &o.format, dialect);
    let _ = out.flush();
}

fn open(o: &Opts) -> Result<Box<dyn Session>, String> {
    let Some(target) = &o.target else {
        return Err("-c <target>가 필요합니다".into());
    };
    let spec = resolve_target(target, o.dialect)?;
    let mut op = opener(o.dialect);
    op(&spec).map_err(|e| e.to_string())
}

fn usage() -> i32 {
    eprint!("{}", crate::help::text(Some("cat")));
    2
}

pub(crate) fn cmd_cat(o: &Opts) -> i32 {
    let Some(sub) = o.positional.first().map(String::as_str) else {
        return usage();
    };
    let mut session = match open(o) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let s = session.as_mut();
    let dialect = s.dialect();
    let schema = match &o.schema {
        Some(sc) => sc.clone(),
        None => nsql_catalog::current_schema(s).unwrap_or_default(),
    };
    let res: Result<i32, nsql_core::DbError> = (|| {
        match sub {
            "schemas" => {
                let list = nsql_catalog::schemas(s)?;
                print_rs(
                    o,
                    dialect,
                    &rs(&["Schema"], list.into_iter().map(|n| vec![n]).collect()),
                );
            }
            "kinds" => {
                let rows = nsql_catalog::kinds_for(dialect)
                    .iter()
                    .map(|k| vec![k.code().to_string(), k.folder().to_string()])
                    .collect();
                print_rs(o, dialect, &rs(&["Kind", "Folder"], rows));
            }
            "columns" | "desc" | "describe" => {
                let Some(obj) = o.positional.get(1) else {
                    return Ok(usage());
                };
                let (sc, name) = split_name(obj, &schema);
                let cols = nsql_catalog::columns(s, &sc, &name)?;
                let rows = cols
                    .into_iter()
                    .map(|c| {
                        vec![
                            c.position.to_string(),
                            c.name,
                            c.data_type,
                            if c.nullable { "Y".into() } else { "N".into() },
                            c.default,
                        ]
                    })
                    .collect();
                print_rs(
                    o,
                    dialect,
                    &rs(&["#", "Name", "Type", "Nullable", "Default"], rows),
                );
            }
            "source" | "ddl" => {
                let (Some(kind), Some(name)) = (o.positional.get(1), o.positional.get(2)) else {
                    return Ok(usage());
                };
                let Some(kind) = ObjectKind::parse(kind) else {
                    eprintln!("알 수 없는 종류: {kind}");
                    return Ok(2);
                };
                let (sc, name) = split_name(name, &schema);
                let text = nsql_catalog::source(s, &sc, kind, &name)?;
                print!("{text}");
            }
            "errors" | "err" => {
                let Some(name) = o.positional.get(1) else {
                    return Ok(usage());
                };
                let (sc, name) = split_name(name, &schema);
                let errs = nsql_catalog::compile_errors(s, &sc, &name)?;
                if errs.is_empty() {
                    println!("No errors.");
                } else {
                    let rows = errs
                        .into_iter()
                        .map(|e| vec![e.line.to_string(), e.col.to_string(), e.severity, e.text])
                        .collect();
                    print_rs(o, dialect, &rs(&["Line", "Col", "Severity", "Text"], rows));
                    return Ok(1);
                }
            }
            other => {
                let Some(kind) = ObjectKind::parse(other) else {
                    return Ok(usage());
                };
                let list = nsql_catalog::objects(s, &schema, kind)?;
                let rows = list
                    .into_iter()
                    .map(|i| vec![i.schema, i.name, i.status, i.modified, i.extra])
                    .collect();
                print_rs(
                    o,
                    dialect,
                    &rs(&["Schema", "Name", "Status", "Modified", "Extra"], rows),
                );
            }
        }
        Ok(0)
    })();
    match res {
        Ok(code) => code,
        Err(e) => {
            eprintln!("ERROR: {e}");
            1
        }
    }
}

/// `schema.name` 또는 `name`(기본 스키마).
fn split_name(obj: &str, default_schema: &str) -> (String, String) {
    let clean = |s: &str| {
        s.trim_matches(|c| c == '"' || c == '[' || c == ']' || c == '`')
            .to_string()
    };
    match obj.split_once('.') {
        Some((sc, n)) => (clean(sc), clean(n)),
        None => (default_schema.to_string(), clean(obj)),
    }
}
