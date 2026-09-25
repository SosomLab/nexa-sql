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
                // `schemas all` = 시스템 스키마까지 · `nonempty` = 빈 스키마 숨김(GUI 설정과 같은 규칙).
                let opts = nsql_catalog::SchemaOpts {
                    show_system: o.positional.iter().any(|t| t == "all"),
                    hide_empty: o.positional.iter().any(|t| t == "nonempty"),
                };
                let list = nsql_catalog::schemas_opt(s, opts)?;
                print_rs(
                    o,
                    dialect,
                    &rs(&["Schema"], list.into_iter().map(|n| vec![n]).collect()),
                );
            }
            // ★ 객체 상세(86 · T-223): `detail <object> [kind]` — 탐색기 아래 객체 상세 패널과 같은 함수.
            "detail" | "info" => {
                let Some(obj) = o.positional.get(1) else {
                    return Ok(usage());
                };
                let (sc, name) = split_name(obj, &schema);
                let kind = o
                    .positional
                    .get(2)
                    .and_then(|k| {
                        nsql_catalog::ObjectKind::ALL
                            .iter()
                            .copied()
                            .find(|x| x.code() == k.to_lowercase())
                    })
                    .unwrap_or(nsql_catalog::ObjectKind::Table);
                let owner = nsql_catalog::ObjectInfo {
                    schema: sc,
                    name,
                    kind,
                    status: String::new(),
                    modified: String::new(),
                    extra: String::new(),
                };
                let secs =
                    nsql_catalog::object_details(s, &owner, nsql_catalog::GenOpts::default())?;
                for sec in secs {
                    println!("== {:?} ==", sec.id);
                    match sec.text {
                        Some(t) => println!("{t}"),
                        None => {
                            let heads: Vec<String> =
                                sec.headers.iter().map(|h| format!("{h:?}")).collect();
                            print!("{}", nsql_catalog::render_table(&heads, &sec.rows));
                        }
                    }
                    println!();
                }
            }
            // ★ 코멘트 그대로(86 §4): `comments <table>` — 테이블·컬럼 코멘트(NULL/공백 구분 · 객체 상세 패널과 같은 함수).
            "comments" => {
                let Some(obj) = o.positional.get(1) else {
                    // 객체 없이 = 스키마 전체(워머와 같은 함수) 요약.
                    let t0 = std::time::Instant::now();
                    let (tables, cols) = nsql_catalog::schema_comments_raw(s, &schema);
                    let tn = tables.iter().filter(|(_, c)| c.is_some()).count();
                    let cn = cols.iter().filter(|(_, _, c)| c.is_some()).count();
                    println!(
                        "schema {schema}: tables {} (with comment {tn}) · columns {} (with comment {cn}) · {} ms",
                        tables.len(),
                        cols.len(),
                        t0.elapsed().as_millis()
                    );
                    return Ok(0);
                };
                let (sc, name) = split_name(obj, &schema);
                let (tc, cols) = nsql_catalog::comments_raw(s, &sc, &name);
                println!(
                    "table: {}",
                    tc.map_or("NULL".to_string(), |c| format!("{c:?}"))
                );
                let rows = cols
                    .into_iter()
                    .map(|(n, c)| vec![n, c.map_or("NULL".to_string(), |c| format!("{c:?}"))])
                    .collect();
                print_rs(o, dialect, &rs(&["Column", "Comment"], rows));
            }
            // ★ 이름 인덱스(84 §2): `index [max]` — 탐색기 검색이 쓰는 서버 전체 (스키마, 종류, 이름) 한 번에.
            "index" => {
                let max = o
                    .positional
                    .get(1)
                    .and_then(|t| t.parse::<usize>().ok())
                    .unwrap_or(0);
                // 범위 = `-s 스키마` 하나 · 없으면 GUI 탐색기와 같은 보이는 스키마 목록(시스템 스키마 제외) — 전체(ALL_OBJECTS 무제한)는 느리다.
                let scope: Vec<String> = match &o.schema {
                    Some(sc) => vec![sc.clone()],
                    None => nsql_catalog::schemas_opt(s, nsql_catalog::SchemaOpts::default())?,
                };
                let t0 = std::time::Instant::now();
                let (list, truncated) = nsql_catalog::name_index(s, &scope, max)?;
                let n = list.len();
                let rows = list
                    .into_iter()
                    .map(|e| vec![e.schema, e.kind.code().to_string(), e.name])
                    .collect();
                print_rs(o, dialect, &rs(&["Schema", "Kind", "Name"], rows));
                eprintln!(
                    "-- index: {n} entries · {} ms{}",
                    t0.elapsed().as_millis(),
                    if truncated { " · truncated" } else { "" }
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
            // ★ 하위 항목(83 §1): `sub <object> <kind>` — 제약·외래 키·참조·인덱스·트리거·파티션·의존·규칙·정책·확장 속성·인자·속성·메서드·멤버.
            "sub" | "subs" => {
                let (Some(obj), Some(sub)) = (o.positional.get(1), o.positional.get(2)) else {
                    return Ok(usage());
                };
                let Some(sub) = nsql_catalog::SubKind::parse(sub) else {
                    eprintln!("알 수 없는 하위 종류: {sub}");
                    return Ok(2);
                };
                let (sc, name) = split_name(obj, &schema);
                // 주인 종류 = 3번째 인자(기본 table) — 인덱스 컬럼·타입 속성·패키지 멤버는 종류가 달라야 맞는 질의를 탄다.
                let kind = o
                    .positional
                    .get(3)
                    .and_then(|k| ObjectKind::parse(k))
                    .unwrap_or(ObjectKind::Table);
                let owner = nsql_catalog::ObjectInfo {
                    schema: sc,
                    name,
                    kind,
                    status: String::new(),
                    modified: String::new(),
                    extra: String::new(),
                };
                let list = nsql_catalog::sub_items(s, &owner, sub)?;
                let rows = list
                    .into_iter()
                    .map(|i| vec![i.name, i.detail, i.status, format!("{:?}", i.icon)])
                    .collect();
                print_rs(o, dialect, &rs(&["Name", "Detail", "Status", "Icon"], rows));
            }
            // ★ Generate SQL(83 §3): `gen <what> <object> [kind] [sub-kind sub-name]` — GUI 미리보기와 같은 함수.
            "gen" | "generate" => {
                let (Some(what), Some(obj)) = (o.positional.get(1), o.positional.get(2)) else {
                    return Ok(usage());
                };
                let Some(what) = nsql_catalog::GenWhat::parse(what) else {
                    eprintln!("알 수 없는 생성 종류: {what}");
                    return Ok(2);
                };
                let (sc, name) = split_name(obj, &schema);
                // `k=v` 토큰 = 옵션(qualified · compact · full · fk) · 나머지 = 종류 · 하위 종류 · 하위 이름.
                let (opt_tokens, rest): (Vec<String>, Vec<String>) = o
                    .positional
                    .iter()
                    .skip(3)
                    .cloned()
                    .partition(|t| t.contains('='));
                let opts = nsql_catalog::GenOpts::default().with_tokens(&opt_tokens);
                let kind = rest
                    .first()
                    .and_then(|k| ObjectKind::parse(k))
                    .unwrap_or(ObjectKind::Table);
                let sub = match (rest.get(1), rest.get(2)) {
                    (Some(sk), Some(sn)) => match nsql_catalog::SubKind::parse(sk) {
                        Some(sk) => Some((sk, sn.clone())),
                        None => {
                            eprintln!("알 수 없는 하위 종류: {sk}");
                            return Ok(2);
                        }
                    },
                    _ => None,
                };
                // 서명·테이블 같은 부가(`extra`)는 목록에서 찾아 채운다(PG 오버로드 · SQL Server 함수 종류·인덱스 테이블).
                let extra = if kind.is_routine() || kind == ObjectKind::Index {
                    nsql_catalog::objects(s, &sc, kind)?
                        .into_iter()
                        .find(|i| i.name.eq_ignore_ascii_case(&name))
                        .map(|i| i.extra)
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let spec = nsql_catalog::GenSpec {
                    owner: nsql_catalog::ObjectInfo {
                        schema: sc,
                        name,
                        kind,
                        status: String::new(),
                        modified: String::new(),
                        extra,
                    },
                    what,
                    sub,
                    opts,
                };
                let text = nsql_catalog::generate(s, &spec)?;
                print!("{text}");
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
