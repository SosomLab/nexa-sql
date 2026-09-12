//! 실서버 통합 테스트 — 환경변수가 있을 때만 실행(없으면 통과·건너뜀 메시지).
//!   NSQL_ORACLE_URL=oracle://nexa:nexa@oracle:1521/FREEPDB1
//!   NSQL_MSSQL_URL=mssql://sa:Nexa%40Sql2026@mssql:1433/master   (비밀번호의 @는 %40)
//! Codespaces devcontainer(.devcontainer) · GitHub Actions(integration.yml)에서 각 DBMS 컨테이너를 띄워 돌린다.
#![allow(clippy::unwrap_used)]

use nsql_core::{DbError, Dialect, Session, Value};
use nsql_run::{Opener, RunEvent, Runner};
use nsql_script::ConnectSpec;

fn runner_for(env: &str, dialect: Dialect) -> Option<Runner> {
    let Ok(url) = std::env::var(env) else {
        eprintln!("[skip] {env} 없음");
        return None;
    };
    let spec = nsql_drivers::parse_target(&url, dialect).expect("접속 문자열");
    let opener: Opener = Box::new(
        move |s: &ConnectSpec| -> Result<Box<dyn Session>, DbError> {
            nsql_drivers::open(s, dialect)
        },
    );
    let mut r = Runner::new(dialect, opener);
    let mut ev = Vec::new();
    assert!(r.connect(&spec, &mut |e| ev.push(e)), "접속 실패: {ev:?}");
    Some(r)
}

fn run(r: &mut Runner, src: &str) -> Vec<RunEvent> {
    let mut ev = Vec::new();
    let mut no_prompt = |_: &str| None;
    let errs = r.run_script(src, &mut no_prompt, &mut |e| ev.push(e));
    assert_eq!(errs, 0, "오류: {ev:#?}");
    ev
}

fn first_rs(ev: &[RunEvent]) -> Option<&nsql_core::ResultSet> {
    ev.iter().find_map(|e| {
        if let RunEvent::ResultSet { rs, .. } = e {
            Some(rs)
        } else {
            None
        }
    })
}

#[test]
fn oracle_session_variables_refcursor_and_dbms_output() {
    let Some(mut r) = runner_for("NSQL_ORACLE_URL", Dialect::Oracle) else {
        return;
    };
    let ev = run(
        &mut r,
        "VARIABLE rc REFCURSOR\nEXEC :V_NAME := 'nexa'\nEXEC SELECT COUNT(*) INTO :V_CNT FROM ALL_OBJECTS WHERE ROWNUM <= 3\nPRINT V_CNT\nSELECT :V_NAME AS n, :V_CNT AS c FROM DUAL;\nEXEC OPEN :rc FOR SELECT :V_NAME AS n FROM DUAL\nPRINT rc\nSET SERVEROUTPUT ON\nEXEC DBMS_OUTPUT.PUT_LINE('hello ' || :V_NAME)\n",
    );
    assert_eq!(
        r.engine.vars.get("V_CNT").unwrap().value,
        Value::Int(3),
        "{ev:#?}"
    );
    let rs = first_rs(&ev).unwrap();
    assert_eq!(rs.rows[0], vec![Value::Str("nexa".into()), Value::Int(3)]);
    let cursor_rs = ev
        .iter()
        .filter_map(|e| {
            if let RunEvent::ResultSet { rs, .. } = e {
                Some(rs)
            } else {
                None
            }
        })
        .nth(1)
        .expect("PRINT rc 결과");
    assert_eq!(cursor_rs.rows[0][0], Value::Str("nexa".into()));
    assert!(
        ev.iter()
            .any(|e| matches!(e, RunEvent::Message(m) if m == "hello nexa")),
        "{ev:#?}"
    );
}

#[test]
fn oracle_plsql_block_out_binds_and_dml() {
    let Some(mut r) = runner_for("NSQL_ORACLE_URL", Dialect::Oracle) else {
        return;
    };
    run(&mut r, "BEGIN EXECUTE IMMEDIATE 'DROP TABLE nexa_it'; EXCEPTION WHEN OTHERS THEN NULL; END;\n/\nCREATE TABLE nexa_it (id NUMBER, name VARCHAR2(30));\nINSERT INTO nexa_it VALUES (1, '홍길동');\nBEGIN\n  SELECT name INTO :V_NM FROM nexa_it WHERE id = 1;\n  :V_TWICE := :V_NM || :V_NM;\nEND;\n/\nCOMMIT;\n");
    assert_eq!(
        r.engine.vars.get("V_NM").unwrap().value,
        Value::Str("홍길동".into())
    );
    assert_eq!(
        r.engine.vars.get("V_TWICE").unwrap().value,
        Value::Str("홍길동홍길동".into())
    );
    let ev = run(
        &mut r,
        "SELECT id, name FROM nexa_it WHERE name = :V_NM;\nDROP TABLE nexa_it;\n",
    );
    let rs = first_rs(&ev).unwrap();
    assert_eq!(rs.rows[0], vec![Value::Int(1), Value::Str("홍길동".into())]);
}

#[test]
fn mssql_session_variables_and_select_into() {
    let Some(mut r) = runner_for("NSQL_MSSQL_URL", Dialect::Mssql) else {
        return;
    };
    let ev = run(
        &mut r,
        "EXEC :V_NAME := 'nexa'\nEXEC SELECT COUNT(*) INTO :V_CNT FROM sys.objects WHERE object_id < 10\nPRINT V_CNT\nSELECT :V_NAME AS n, :V_CNT AS c;\nEXEC SELECT TOP 1 name INTO :V_OBJ FROM sys.objects ORDER BY name\nPRINT V_OBJ\n",
    );
    let cnt = &r.engine.vars.get("V_CNT").unwrap().value;
    assert!(
        matches!(cnt, Value::Str(s) if s.parse::<i64>().unwrap() > 0)
            || matches!(cnt, Value::Int(n) if *n > 0),
        "{cnt:?} {ev:#?}"
    );
    let rs = first_rs(&ev).unwrap();
    assert_eq!(rs.rows[0][0], Value::Str("nexa".into()));
    assert!(matches!(&r.engine.vars.get("V_OBJ").unwrap().value, Value::Str(s) if !s.is_empty()));
}

#[test]
fn mssql_dml_batches_and_go() {
    let Some(mut r) = runner_for("NSQL_MSSQL_URL", Dialect::Mssql) else {
        return;
    };
    let ev = run(&mut r, "IF OBJECT_ID('tempdb..#nexa_it') IS NOT NULL DROP TABLE #nexa_it\nGO\nCREATE TABLE #nexa_it (id INT, name NVARCHAR(30))\nGO\nINSERT INTO #nexa_it VALUES (1, N'홍길동'), (2, N'김철수');\nEXEC :V_ID := 2\nSELECT id, name FROM #nexa_it WHERE id = :V_ID;\n");
    let rs = first_rs(&ev).unwrap();
    assert_eq!(rs.rows[0], vec![Value::Int(2), Value::Str("김철수".into())]);
    assert!(
        ev.iter().any(|e| matches!(
            e,
            RunEvent::Done {
                rows_affected: Some(2),
                ..
            }
        )),
        "{ev:#?}"
    );
}

#[test]
fn sqlite_always_runs() {
    let opener: Opener = Box::new(|s: &ConnectSpec| nsql_drivers::open(s, Dialect::Sqlite));
    let mut r = Runner::new(Dialect::Sqlite, opener);
    let spec = nsql_drivers::parse_target("sqlite::memory:", Dialect::Sqlite).unwrap();
    let mut ev = Vec::new();
    assert!(r.connect(&spec, &mut |e| ev.push(e)));
    let ev = run(&mut r, "EXEC :A := 41\nSELECT :A + 1 AS b;\n");
    assert_eq!(first_rs(&ev).unwrap().rows[0][0], Value::Int(42));
}
