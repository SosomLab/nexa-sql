//! 실서버 통합 테스트 — 환경변수가 있을 때만 실행(없으면 통과·건너뜀 메시지).
//!   NSQL_ORACLE_URL=oracle://nexa:nexa@oracle:1521/FREEPDB1
//!   NSQL_MSSQL_URL=mssql://sa:Nexa%40Sql2026@mssql:1433/master   (비밀번호의 @는 %40)
//!   NSQL_PG_URL=postgres://nexa:nexa@postgres:5432/nexa
//!   (로컬: `NSQL_PG_PROFILE=<프로필>` — 저장된 프로필로 · 실제 서버이므로 임시 객체만 쓰는 테스트를 이름으로 골라 돌릴 것)
//! Codespaces devcontainer(.devcontainer) · GitHub Actions(integration.yml)에서 각 DBMS 컨테이너를 띄워 돌린다.
#![allow(clippy::unwrap_used)]

use nsql_core::{DbError, Dialect, Session, Value};
use nsql_run::{Opener, RunEvent, Runner};
use nsql_script::ConnectSpec;

fn runner_for(env: &str, dialect: Dialect) -> Option<Runner> {
    // `<…>_URL` 대신 `<…>_PROFILE=<저장된 프로필 이름>`도 받는다 — 로컬에서 비밀번호를 환경 변수에 적지 않고 돌린다.
    // ⚠ 프로필은 **실제 서버**다: 이름으로 골라(`cargo test … pg_refcursor`) 임시 객체만 쓰는 테스트만 돌린다.
    let profile = std::env::var(env.replace("_URL", "_PROFILE")).ok();
    let spec = match (std::env::var(env), profile) {
        (Ok(url), _) => nsql_drivers::parse_target(&url, dialect).expect("접속 문자열"),
        (Err(_), Some(name)) => nsql_vault::Vault::open_default()
            .and_then(|v| v.get(&name))
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("프로필 없음: {name}")),
        (Err(_), None) => {
            eprintln!("[skip] {env} 없음");
            return None;
        }
    };
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

/// 오류가 나도 되는 실행 — (오류 수, 사건). 정책 시험(0행·여러 행 = 오류)이 쓴다.
fn run_raw(r: &mut Runner, src: &str) -> (usize, Vec<RunEvent>) {
    let mut ev = Vec::new();
    let mut no_prompt = |_: &str| None;
    let errs = r.run_script(src, &mut no_prompt, &mut |e| ev.push(e));
    (errs, ev)
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

/// D-139: SQL Server의 `SELECT @v = col`은 여러 행이면 마지막 행을 말없이 쓴다 → 서버 쪽 `@@ROWCOUNT` 검사로 Oracle과 같은 오류.
#[test]
fn mssql_select_into_row_guard() {
    let Some(mut r) = runner_for("NSQL_MSSQL_URL", Dialect::Mssql) else {
        return;
    };
    let (errs, ev) = run_raw(
        &mut r,
        "EXEC SELECT TOP 1 object_id INTO :V_ID FROM sys.objects ORDER BY object_id\n",
    );
    assert_eq!(errs, 0, "{ev:#?}");
    let kept = r.engine.vars.get("V_ID").unwrap().value.clone();
    let (errs, ev) = run_raw(
        &mut r,
        "EXEC SELECT object_id INTO :V_ID FROM sys.objects WHERE 1 = 0\n",
    );
    assert_eq!(errs, 1, "0행 = 오류: {ev:#?}");
    let (errs, ev) = run_raw(
        &mut r,
        "EXEC SELECT object_id INTO :V_ID FROM sys.objects\n",
    );
    assert_eq!(errs, 1, "여러 행 = 오류: {ev:#?}");
    assert_eq!(
        r.engine.vars.get("V_ID").unwrap().value,
        kept,
        "오류면 변수는 그대로"
    );
}

/// D-139(OUT 바인드 없는 방언): PostgreSQL `EXEC SELECT … INTO` = 자리 순서로 받고 0행·여러 행은 오류.
#[test]
fn pg_select_into_row_policy() {
    let Some(mut r) = runner_for("NSQL_PG_URL", Dialect::Postgres) else {
        return;
    };
    let (errs, ev) = run_raw(
        &mut r,
        "EXEC SELECT 1, 'x' INTO :A, :B FROM generate_series(1, 1)\nEXEC :N := (SELECT count(*) FROM generate_series(1, 5))\n",
    );
    assert_eq!(errs, 0, "{ev:#?}");
    assert_eq!(
        r.engine.vars.get("B").unwrap().value,
        Value::Str("x".into())
    );
    let (errs, ev) = run_raw(
        &mut r,
        "EXEC SELECT g INTO :A FROM generate_series(1, 3) g\n",
    );
    assert_eq!(errs, 1, "여러 행 = 오류: {ev:#?}");
    let (errs, ev) = run_raw(
        &mut r,
        "EXEC SELECT g INTO :A FROM generate_series(1, 3) g WHERE g > 9\n",
    );
    assert_eq!(errs, 1, "0행 = 오류: {ev:#?}");
    // 배열 조각의 `:`는 바인드가 아니다.
    let (errs, ev) = run_raw(&mut r, "SELECT (ARRAY[1,2,3,4])[2:3] AS s;\n");
    assert_eq!(errs, 0, "{ev:#?}");
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
fn pg_session_variables_procedure_out_and_notice() {
    let Some(mut r) = runner_for("NSQL_PG_URL", Dialect::Postgres) else {
        return;
    };
    let ev = run(
        &mut r,
        "EXEC :V_NAME := 'nexa'\nEXEC SELECT COUNT(*) INTO :V_CNT FROM (SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3) t\nSELECT :V_NAME AS n, :V_CNT AS c;\nCREATE OR REPLACE PROCEDURE nsql_it_p(IN a INT, OUT b INT) LANGUAGE plpgsql AS $$ BEGIN b := a * 2; END $$;\nCALL nsql_it_p(21, NULL);\nSELECT 1234.5::numeric AS d, DATE '2026-07-23' AS dt, TIMESTAMP '2026-07-23 01:02:03' AS ts;\n",
    );
    // 단순 질의 프로토콜은 셀이 텍스트로 온다(`Str("3")`) — 표시값으로 비교.
    assert_eq!(
        r.engine.vars.get("V_CNT").unwrap().value.display(),
        "3",
        "{ev:#?}"
    );
    let sets: Vec<&nsql_core::ResultSet> = ev
        .iter()
        .filter_map(|e| match e {
            RunEvent::ResultSet { rs, .. } => Some(rs),
            _ => None,
        })
        .collect();
    assert_eq!(sets[0].rows[0][0], Value::Str("nexa".into()));
    // CALL의 OUT은 1행 결과로 온다.
    assert_eq!(sets[1].rows[0][0], Value::Str("42".into()), "{ev:#?}");
    assert_eq!(sets[2].rows[0][0], Value::Str("1234.5".into()));
}

/// docs/56 L1 — 수동 커밋 모드에서 조회만 하면 트랜잭션이 남지 않는다(`idle in transaction` 아님) · 변경 뒤에는 남는다.
#[test]
fn pg_read_only_transaction_ends_in_manual_mode() {
    let Some(mut r) = runner_for("NSQL_PG_URL", Dialect::Postgres) else {
        return;
    };
    let Some(mut watcher) = runner_for("NSQL_PG_URL", Dialect::Postgres) else {
        return;
    };
    // 자동 커밋(기본값이 무엇이든)으로 준비 → 수동으로 전환.
    run(
        &mut r,
        "SET AUTOCOMMIT ON
DROP TABLE IF EXISTS nsql_it_l1;
CREATE TABLE nsql_it_l1 (a INT);
SET AUTOCOMMIT OFF
",
    );
    let ev = run(
        &mut r,
        "SELECT pg_backend_pid();
",
    );
    let pid = first_rs(&ev).unwrap().rows[0][0].display();
    assert!(
        ev.iter().any(|e| matches!(e, RunEvent::ReadTxEnded { .. })),
        "조회만 = 읽기 트랜잭션 종료 이벤트: {ev:#?}"
    );
    let state = |w: &mut Runner| {
        let ev = run(
            w,
            &format!(
                "SET AUTOCOMMIT ON
SELECT state FROM pg_stat_activity WHERE pid = {pid};
"
            ),
        );
        first_rs(&ev).unwrap().rows[0][0].display()
    };
    assert_eq!(
        state(&mut watcher),
        "idle",
        "조회 뒤 트랜잭션이 남으면 안 된다"
    );
    // 변경이 있으면 남는다(사용자가 확정해야 한다) — 그 뒤의 조회도 끝내지 않는다.
    let ev = run(
        &mut r,
        "INSERT INTO nsql_it_l1 VALUES (1);
SELECT * FROM nsql_it_l1;
",
    );
    assert!(!ev.iter().any(|e| matches!(e, RunEvent::ReadTxEnded { .. })));
    assert_eq!(state(&mut watcher), "idle in transaction");
    // 정리.
    run(
        &mut r,
        "ROLLBACK;
SET AUTOCOMMIT ON
DROP TABLE IF EXISTS nsql_it_l1;
",
    );
}

/// T-151 — SQL Server: 호출 쪽에 `OUTPUT`을 빠뜨려도 **서명(`sys.parameters`)이 OUTPUT이라 한 자리의 바인드는 값을 받는다**
/// (종전 = 조용히 버려짐) · 선언 없이 생긴 글자 변수가 돌아오는 값을 자르지 않는다(종전 = `NVARCHAR(2)`로 선언돼 `'in'` 그대로) ·
/// 이름 표기 · 정수는 `63.0000000000`이 아니라 `63` · DATETIME2 왕복. 세션 임시 프로시저(`#…`)라 서버에 남지 않는다.
#[test]
fn mssql_exec_output_marks_follow_signature() {
    let Some(mut r) = runner_for("NSQL_MSSQL_URL", Dialect::Mssql) else {
        return;
    };
    run(
        &mut r,
        "SET AUTOCOMMIT ON
CREATE PROCEDURE #nsql_it_io @a INT, @b INT OUTPUT, @msg NVARCHAR(100) OUTPUT, @d DATETIME2 = NULL OUTPUT AS BEGIN SET @b = @a * @b; SET @msg = @msg + N' -> ' + CAST(@b AS NVARCHAR(20)); SET @d = DATEADD(DAY, 1, @d); END
GO
EXEC :X := 21
EXEC :Y := 'in'
EXEC #nsql_it_io 3, :X, :Y
",
    );
    let get = |r: &Runner, n: &str| r.engine.vars.get(n).map(|v| v.value.display());
    assert_eq!(get(&r, "X").as_deref(), Some("63"));
    assert_eq!(get(&r, "Y").as_deref(), Some("in -> 63"));
    run(&mut r, "EXEC #nsql_it_io @msg = :Y, @a = 2, @b = :X\n");
    assert_eq!(get(&r, "X").as_deref(), Some("126"));
    assert_eq!(get(&r, "Y").as_deref(), Some("in -> 63 -> 126"));
    // 이미 OUTPUT을 쓴 호출은 그대로(두 번 붙이지 않는다) · 시각 왕복.
    run(
        &mut r,
        "EXEC :D := '2026-09-21 13:45:10'\nEXEC #nsql_it_io 1, :X OUTPUT, :Y output, :D\n",
    );
    assert_eq!(get(&r, "X").as_deref(), Some("126"));
    assert_eq!(get(&r, "D").as_deref(), Some("2026-09-22 13:45:10"));
}

/// T-151 — Oracle 날짜·불리언의 **진짜 바인드**: 종전에는 전부 VARCHAR2(4000)이라 DATE가 세션 NLS 형식 글(`26/09/21`)로 바뀌어
/// 시각이 잘리고 다시 쓰면 ORA-01722였다 · PL/SQL BOOLEAN은 글자로 바인드할 수 없다. 서버에 객체를 만들지 않는다.
#[test]
fn oracle_date_timestamp_boolean_binds_are_native() {
    let Some(mut r) = runner_for("NSQL_ORACLE_URL", Dialect::Oracle) else {
        return;
    };
    run(
        &mut r,
        "VAR d DATE
VAR ts TIMESTAMP
VAR b BOOLEAN
EXEC :d := TO_DATE('2026-09-21 13:45:10','YYYY-MM-DD HH24:MI:SS')
EXEC :ts := TO_TIMESTAMP('2026-09-21 13:45:10.123456','YYYY-MM-DD HH24:MI:SS.FF')
",
    );
    let get = |r: &Runner, n: &str| r.engine.vars.get(n).map(|v| v.value.display());
    assert_eq!(
        get(&r, "D").as_deref(),
        Some("2026-09-21 13:45:10"),
        "시각이 잘리지 않는다"
    );
    assert_eq!(get(&r, "TS").as_deref(), Some("2026-09-21 13:45:10.123456"));
    let ev = run(
        &mut r,
        "SELECT TO_CHAR(:d,'YYYY-MM-DD HH24:MI:SS') AS a, SUBSTR(DUMP(:d), 1, 6) AS t FROM dual;\n",
    );
    let rs = first_rs(&ev).expect("rs");
    assert_eq!(rs.rows[0][0].display(), "2026-09-21 13:45:10");
    assert_eq!(
        rs.rows[0][1].display(),
        "Typ=12",
        "글자가 아니라 DATE로 바인드된다"
    );
    run(&mut r, "EXEC :d := :d + 1\n");
    assert_eq!(get(&r, "D").as_deref(), Some("2026-09-22 13:45:10"));
    run(&mut r, "BEGIN :b := (1 = 1); END;\n/\n");
    assert_eq!(get(&r, "B").as_deref(), Some("true"));
    run(&mut r, "BEGIN :b := NOT :b; END;\n/\n");
    assert_eq!(get(&r, "B").as_deref(), Some("false"));
    // 서명 추론: SYS 패키지의 OUT 인자(VARCHAR2 · INTEGER) — 선언 없는 바인드로 받는다.
    run(&mut r, "EXEC DBMS_OUTPUT.GET_LINE(:LINE, :STATUS)\n");
    assert_eq!(get(&r, "STATUS").as_deref(), Some("1"));
}

/// T-151 — PostgreSQL `EXEC proc(…)`의 OUT/INOUT 값은 **서명이 말한 자리의 바인드**가 받는다: 바인드 이름이 형식 인자 이름과
/// 달라도(종전 = 조용히 버려짐) · 이름 표기 · OUT 자리에 상수를 준 경우 · 이름이 같은 종전 경우. 임시 프로시저라 서버에 남지 않는다.
#[test]
fn pg_exec_out_binds_follow_signature() {
    let Some(mut r) = runner_for("NSQL_PG_URL", Dialect::Postgres) else {
        return;
    };
    run(
        &mut r,
        "SET AUTOCOMMIT ON
CREATE PROCEDURE pg_temp.nsql_it_io(IN a int, INOUT b int, INOUT msg text) LANGUAGE plpgsql AS $$ BEGIN b := a * b; msg := msg || ' -> ' || b; END $$;
EXEC :X := 21
EXEC :Y := 'in'
EXEC pg_temp.nsql_it_io(3, :X, :Y)
",
    );
    let get = |r: &Runner, n: &str| r.engine.vars.get(n).map(|v| v.value.display());
    assert_eq!(get(&r, "X").as_deref(), Some("63"));
    assert_eq!(get(&r, "Y").as_deref(), Some("in -> 63"));
    run(
        &mut r,
        "EXEC pg_temp.nsql_it_io(msg => :Y, a => 2, b => :X)\n",
    );
    assert_eq!(get(&r, "X").as_deref(), Some("126"));
    assert_eq!(get(&r, "Y").as_deref(), Some("in -> 63 -> 126"));
    run(
        &mut r,
        "EXEC :M := 'lit'\nEXEC pg_temp.nsql_it_io(10, 4, :M)\n",
    );
    assert_eq!(get(&r, "M").as_deref(), Some("lit -> 40"));
    assert_eq!(
        get(&r, "X").as_deref(),
        Some("126"),
        "상수를 준 OUT 자리는 아무 변수도 건드리지 않는다"
    );
    // 이름이 같은 종전 경우.
    run(
        &mut r,
        "EXEC :B := 5\nEXEC :MSG := 'same'\nEXEC pg_temp.nsql_it_io(2, :B, :MSG)\n",
    );
    assert_eq!(get(&r, "B").as_deref(), Some("10"));
    assert_eq!(get(&r, "MSG").as_deref(), Some("same -> 10"));
}

/// T-150 — PostgreSQL `refcursor`: 함수가 돌려준 커서 이름이 아니라 **커서의 내용**이 결과로 온다(이름 = 결과의 label).
/// 자동 커밋(드라이버가 호출과 FETCH를 한 트랜잭션으로 묶는다) · 수동 커밋(러너가 연 트랜잭션 안) · 페치 상한이 있는 커서 경로와
/// 없는 단순 경로 · `SETOF refcursor`(이름 있는 커서 둘) · 커서가 아닌 글자 결과는 그대로. 임시 함수(`pg_temp`)라 서버에 남지 않는다.
#[test]
fn pg_refcursor_results_are_expanded() {
    let Some(mut r) = runner_for("NSQL_PG_URL", Dialect::Postgres) else {
        return;
    };
    run(
        &mut r,
        "SET AUTOCOMMIT ON
CREATE FUNCTION pg_temp.nsql_it_rc() RETURNS refcursor LANGUAGE plpgsql AS $$
DECLARE c refcursor;
BEGIN
  OPEN c FOR SELECT g AS n, 'row ' || g AS label FROM generate_series(1, 3) g;
  RETURN c;
END $$;
CREATE FUNCTION pg_temp.nsql_it_rc2() RETURNS SETOF refcursor LANGUAGE plpgsql AS $$
DECLARE a refcursor := 'nsql_it_cur_a'; b refcursor := 'nsql_it_cur_b';
BEGIN
  OPEN a FOR SELECT 1 AS one;
  RETURN NEXT a;
  OPEN b FOR SELECT g AS n FROM generate_series(10, 12) g;
  RETURN NEXT b;
END $$;
",
    );
    let sets = |ev: &[RunEvent]| -> Vec<(Option<String>, Vec<String>, usize)> {
        ev.iter()
            .filter_map(|e| match e {
                RunEvent::ResultSet { rs, label, .. } => Some((
                    label.clone(),
                    rs.columns.iter().map(|c| c.name.clone()).collect(),
                    rs.rows.len(),
                )),
                _ => None,
            })
            .collect()
    };
    for mode in ["SET AUTOCOMMIT ON", "SET AUTOCOMMIT OFF"] {
        run(&mut r, &format!("{mode}\n"));
        let one = sets(&run(&mut r, "SELECT pg_temp.nsql_it_rc();\n"));
        assert_eq!(
            one.len(),
            1,
            "{mode}: 이름 표 대신 커서의 내용 하나 · {one:?}"
        );
        assert_eq!(one[0].1, vec!["n", "label"], "{mode}");
        assert_eq!(one[0].2, 3, "{mode}");
        assert!(one[0].0.is_some(), "{mode}: label = 커서 이름");
        let two = sets(&run(&mut r, "SELECT pg_temp.nsql_it_rc2();\n"));
        assert_eq!(
            two.len(),
            2,
            "{mode}: SETOF refcursor = 커서마다 결과 · {two:?}"
        );
        assert_eq!(two[0].0.as_deref(), Some("nsql_it_cur_a"));
        assert_eq!((two[0].1.clone(), two[0].2), (vec!["one".to_string()], 1));
        assert_eq!(two[1].0.as_deref(), Some("nsql_it_cur_b"));
        assert_eq!(two[1].2, 3);
        // 커서가 아닌 글자는 그대로(이름이 같은 열린 커서가 없다).
        let plain = sets(&run(&mut r, "SELECT upper('nsql_it_cur_a') AS f;\n"));
        assert_eq!(plain.len(), 1);
        assert_eq!((plain[0].0.clone(), plain[0].2), (None, 1));
        // 풀어 낸 커서는 닫혀 있다.
        let ev = run(
            &mut r,
            "SELECT count(*) FROM pg_cursors WHERE name LIKE 'nsql_it_cur%';\n",
        );
        assert_eq!(first_rs(&ev).unwrap().rows[0][0].display(), "0", "{mode}");
        run(&mut r, "ROLLBACK;\n");
    }
    run(&mut r, "SET AUTOCOMMIT ON\n");
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
