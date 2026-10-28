//! 실서버 통합 테스트 — 환경변수가 있을 때만 실행(없으면 통과·건너뜀 메시지).
//!   NSQL_ORACLE_URL=oracle://nexa:nexa@oracle:1521/FREEPDB1
//!   NSQL_MSSQL_URL=mssql://sa:Nexa%40Sql2026@mssql:1433/master   (비밀번호의 @는 %40)
//!   NSQL_PG_URL=postgres://nexa:nexa@postgres:5432/nexa
//!   NSQL_MYSQL_URL=mysql://nexa:nexa@mysql:3306/nexa   (135차 · `NSQL_MYSQL_PROFILE=<프로필>`도 된다)
//!   (로컬: `NSQL_PG_PROFILE=<프로필>` — 저장된 프로필로 · 실제 서버이므로 임시 객체만 쓰는 테스트를 이름으로 골라 돌릴 것)
//! Codespaces devcontainer(.devcontainer) · GitHub Actions(integration.yml)에서 각 DBMS 컨테이너를 띄워 돌린다.
#![allow(clippy::unwrap_used)]

use nsql_core::{DbError, Dialect, Session, Value};
use nsql_run::{Opener, RunEvent, Runner};
use nsql_script::ConnectSpec;

/// ★ D-350 B단계 적합성: `NSQL_TEST_DYN=1`이면 모든 시험이 **확장 경로**(target의 cdylib를 임시 드라이버 폴더에 깔고 ABI v2로 로드)로
/// 세션을 연다 — 내장 경로와 같은 시험이 같은 결과를 내야 한다. 없으면 내장(`nsql_drivers::open`).
fn open_session(spec: &ConnectSpec, dialect: Dialect) -> Result<Box<dyn Session>, DbError> {
    if std::env::var_os("NSQL_TEST_DYN").is_none() {
        return nsql_drivers::open(spec, dialect);
    }
    let root = dyn_root();
    {}
    let real = spec.dialect.unwrap_or(dialect);
    for id in nsql_drivers::ext_ids(real) {
        if let Some(s) = nsql_drivers::ext::open_ext(id, spec)? {
            return Ok(s);
        }
    }
    panic!(
        "[dyn] {real} 확장 cdylib 없음({} · cargo build -p nsql-driver-* 먼저)",
        root.display()
    );
}

/// cdylib가 있는 target/<profile>.
fn cdylib_target_dir() -> std::path::PathBuf {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(profile)
}

/// target의 cdylib 하나를 임시 드라이버 폴더에 `<id>/0.0.0-test/`로 깐다(매니페스트 · 동봉 런타임 포함) — 없으면 false.
fn stage_cdylib(target: &std::path::Path, root: &std::path::Path, d: Dialect, id: &str) -> bool {
    let lib = nsql_driver_dyn::library_file_name(&nsql_drivers::ext_crate_name(id));
    let src = target.join(&lib);
    if !src.is_file() {
        return false;
    }
    let dir = root.join(id).join("0.0.0-test");
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::copy(&src, dir.join(&lib)).expect("copy cdylib");
    copy_bundled_runtime(target, id, &dir);
    std::fs::write(
        dir.join(nsql_drivers::ext::MANIFEST),
        format!(
            r#"{{"id":"{id}","version":"0.0.0-test","dialect":"{d}","protocol":{},"lib":"{lib}"}}"#,
            nsql_abi::ABI_VERSION
        ),
    )
    .expect("manifest");
    true
}

/// 확장 경로 시험의 드라이버 폴더(프로세스당 하나 · 처음 부를 때 cdylib 전부를 깔고 `set_driver_dir`).
fn dyn_root() -> &'static std::path::Path {
    static ROOT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let target = cdylib_target_dir();
        let root = std::env::temp_dir().join(format!("nsql-dyn-it-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (d, ids) in nsql_drivers::DRIVER_IDS {
            for id in *ids {
                stage_cdylib(&target, &root, *d, id);
            }
        }
        nsql_drivers::ext::set_driver_dir(Some(&root));
        eprintln!("[dyn] 확장 경로 시험 · 드라이버 폴더 {}", root.display());
        root
    })
}

/// ★ 올라간 확장 드라이버 삭제(사용자 10-23 "로딩된 대상은 삭제가 불가능?"): 세션이 쥐고 있어도 삭제가 **즉시 성공**한다 — 지울 수 없는
/// 파일(Windows 매핑 DLL)은 `.trash/`로 옮기고 표식에 적어 다음 시작 때 지운다 · 목록·탐색에서는 바로 빠진다 · 올라간 판은 계속 쓰인다. `NSQL_TEST_DYN=1`일 때만.
#[test]
fn dyn_remove_loaded_extension_moves_to_trash() {
    if std::env::var_os("NSQL_TEST_DYN").is_none() {
        eprintln!("[dyn] NSQL_TEST_DYN 없음 - 건너뜀");
        return;
    }
    let root = dyn_root();
    let spec = ConnectSpec::parse("sqlite::memory:").unwrap();
    let mut held = nsql_drivers::ext::open_ext("sqlite", &spec)
        .expect("open")
        .expect("sqlite cdylib 설치됨");
    assert!(nsql_drivers::ext::is_loaded("sqlite"));
    let removed =
        nsql_drivers::ext::remove_extension("sqlite", None).expect("올라가 있어도 삭제 성공");
    assert_eq!(removed, [root.join("sqlite")]);
    assert!(
        nsql_drivers::ext::find_extension("sqlite").is_none(),
        "목록·탐색에서 즉시 빠진다"
    );
    assert!(nsql_drivers::ext::list_extensions_in(root)
        .iter()
        .all(|e| e.id != "sqlite"));
    let pending = nsql_drivers::ext::pending_in(root);
    if cfg!(windows) {
        assert!(!root.join("sqlite").exists() || pending.iter().any(|p| p.starts_with("sqlite")));
        assert!(
            pending
                .iter()
                .any(|p| p.starts_with(nsql_drivers::ext::TRASH_DIR)),
            "매핑 DLL은 휴지통으로: {pending:?}"
        );
    } else {
        assert!(!root.join("sqlite").exists(), "unix는 바로 지워진다");
    }
    // 올라간 세션은 계속 동작한다.
    let r = held
        .execute(&nsql_core::ExecRequest {
            sql: "SELECT 1".to_string(),
            params: Vec::new(),
        })
        .expect("query after remove");
    assert_eq!(r.result_sets.len(), 1);
    drop(held);
    // 다른 시험을 위해 다시 깐다(올라간 판이 캐시돼 있어 바로 열린다).
    assert!(stage_cdylib(
        &cdylib_target_dir(),
        root,
        Dialect::Sqlite,
        "sqlite"
    ));
    assert!(nsql_drivers::ext::open_ext("sqlite", &spec)
        .expect("open")
        .is_some());
}

/// ★ 실행 계획 검토(docs/122 · 사용자 10-23 "각 DBMS별 저장 프로필로 자동 시험"): 임시 표 + 조인 조회로 `Runner::explain`(추정 · 가능하면 실제)
/// → 트리 노드 ≥ 1 · 검토 ≥ 1 · 전체 스캔 지적(작은 표라 Warn)을 확인한다. 서버가 없으면 건너뜀(`runner_for`).
fn explain_check(r: &mut Runner, dialect: Dialect, setup: &str, stmt: &str, analyze: bool) {
    use nsql_script::plan::{PlanOptions, Severity};
    if !setup.is_empty() {
        run(r, setup);
    }
    let rep = r
        .explain(stmt, PlanOptions { analyze: false })
        .unwrap_or_else(|e| panic!("[plan] {dialect} 추정 계획 실패: {}", e.message));
    assert!(!rep.raw.is_empty(), "[plan] {dialect} 원문 없음");
    assert!(
        !rep.nodes.is_empty(),
        "[plan] {dialect} 트리 없음: {:#?}",
        rep.raw
    );
    assert!(!rep.findings.is_empty());
    assert!(
        rep.findings
            .iter()
            .any(|f| f.severity >= Severity::Warn && f.node.is_some()),
        "[plan] {dialect} 전체 스캔 지적 기대: {:#?}",
        rep.findings
    );
    let text = nsql_script::plan::render_text(&rep);
    eprintln!("[plan] {dialect} estimated\n{text}");
    if analyze {
        let rep = r
            .explain(stmt, PlanOptions { analyze: true })
            .unwrap_or_else(|e| panic!("[plan] {dialect} 실제 계획 실패: {}", e.message));
        assert!(
            rep.analyzed && !rep.nodes.is_empty(),
            "[plan] {dialect} 실제 계획 트리 없음: {:#?}",
            rep.raw
        );
        assert!(
            rep.nodes.iter().any(|n| n.actual_rows.is_some()),
            "[plan] {dialect} 실측 행 없음: {:#?}",
            rep.nodes
        );
        eprintln!(
            "[plan] {dialect} actual\n{}",
            nsql_script::plan::render_text(&rep)
        );
    }
}

#[test]
fn explain_review_sqlite() {
    let spec = ConnectSpec::parse("sqlite::memory:").unwrap();
    let opener: Opener = Box::new(move |s: &ConnectSpec| open_session(s, Dialect::Sqlite));
    let mut r = Runner::new(Dialect::Sqlite, opener);
    run(&mut r, "CONNECT sqlite::memory:\nCREATE TABLE emp (id INTEGER PRIMARY KEY, dept INTEGER, name TEXT);\nCREATE TABLE dept (id INTEGER PRIMARY KEY, name TEXT);\nINSERT INTO emp VALUES (1, 10, 'a'), (2, 20, 'b');\nINSERT INTO dept VALUES (10, 'x'), (20, 'y');\n");
    let _ = spec;
    explain_check(
        &mut r,
        Dialect::Sqlite,
        "",
        "SELECT e.name, d.name FROM emp e JOIN dept d ON d.id = e.dept WHERE e.name = 'a'",
        false,
    );
}

#[test]
fn explain_review_pg() {
    let Some(mut r) = runner_for("NSQL_PG_URL", Dialect::Postgres) else {
        return;
    };
    explain_check(&mut r, Dialect::Postgres,
        "DROP TABLE IF EXISTS nsqlt_plan_e; DROP TABLE IF EXISTS nsqlt_plan_d;\nCREATE TABLE nsqlt_plan_d (id INT PRIMARY KEY, name TEXT);\nCREATE TABLE nsqlt_plan_e (id INT PRIMARY KEY, dept INT, name TEXT);\nINSERT INTO nsqlt_plan_d VALUES (10,'x'),(20,'y');\nINSERT INTO nsqlt_plan_e VALUES (1,10,'a'),(2,20,'b');\n",
        "SELECT e.name, d.name FROM nsqlt_plan_e e JOIN nsqlt_plan_d d ON d.id = e.dept WHERE e.name = 'a'", true);
    run(
        &mut r,
        "DROP TABLE nsqlt_plan_e; DROP TABLE nsqlt_plan_d;\n",
    );
}

#[test]
fn explain_review_mssql() {
    let Some(mut r) = runner_for("NSQL_MSSQL_URL", Dialect::Mssql) else {
        return;
    };
    explain_check(&mut r, Dialect::Mssql,
        "IF OBJECT_ID('tempdb..#nsqlt_plan_e') IS NOT NULL DROP TABLE #nsqlt_plan_e\nGO\nIF OBJECT_ID('tempdb..#nsqlt_plan_d') IS NOT NULL DROP TABLE #nsqlt_plan_d\nGO\nCREATE TABLE #nsqlt_plan_d (id INT PRIMARY KEY, name NVARCHAR(20))\nGO\nCREATE TABLE #nsqlt_plan_e (id INT PRIMARY KEY, dept INT, name NVARCHAR(20))\nGO\nINSERT INTO #nsqlt_plan_d VALUES (10,N'x'),(20,N'y');\nINSERT INTO #nsqlt_plan_e VALUES (1,10,N'a'),(2,20,N'b');\n",
        "SELECT e.name, d.name FROM #nsqlt_plan_e e JOIN #nsqlt_plan_d d ON d.id = e.dept WHERE e.name = N'a'", true);
}

#[test]
fn explain_review_mysql() {
    let Some(mut r) = runner_for("NSQL_MYSQL_URL", Dialect::Mysql) else {
        return;
    };
    // MariaDB는 EXPLAIN ANALYZE 문법이 달라 추정 계획만(docs/122).
    explain_check(&mut r, Dialect::Mysql,
        "DROP TABLE IF EXISTS nsqlt_plan_e; DROP TABLE IF EXISTS nsqlt_plan_d;\nCREATE TABLE nsqlt_plan_d (id INT PRIMARY KEY, name VARCHAR(20));\nCREATE TABLE nsqlt_plan_e (id INT PRIMARY KEY, dept INT, name VARCHAR(20));\nINSERT INTO nsqlt_plan_d VALUES (10,'x'),(20,'y');\nINSERT INTO nsqlt_plan_e VALUES (1,10,'a'),(2,20,'b');\n",
        "SELECT e.name, d.name FROM nsqlt_plan_e e JOIN nsqlt_plan_d d ON d.id = e.dept WHERE e.name = 'a'", false);
    run(
        &mut r,
        "DROP TABLE nsqlt_plan_e; DROP TABLE nsqlt_plan_d;\n",
    );
}

#[test]
fn explain_review_oracle() {
    for d in oracle_drivers() {
        nsql_drivers::set_oracle_driver(d);
        let Some(mut r) = runner_for("NSQL_ORACLE_URL", Dialect::Oracle) else {
            return;
        };
        // 실제 계획(DISPLAY_CURSOR)은 V$SQL_PLAN 권한이 있어야 한다 → 추정만 필수 · 실제는 되면 좋고(실패해도 시험은 통과).
        explain_check(&mut r, Dialect::Oracle,
            "BEGIN EXECUTE IMMEDIATE 'DROP TABLE nsqlt_plan_e'; EXCEPTION WHEN OTHERS THEN NULL; END;\n/\nBEGIN EXECUTE IMMEDIATE 'DROP TABLE nsqlt_plan_d'; EXCEPTION WHEN OTHERS THEN NULL; END;\n/\nCREATE TABLE nsqlt_plan_d (id NUMBER PRIMARY KEY, name VARCHAR2(20));\nCREATE TABLE nsqlt_plan_e (id NUMBER PRIMARY KEY, dept NUMBER, name VARCHAR2(20));\nINSERT INTO nsqlt_plan_d VALUES (10,'x');\nINSERT INTO nsqlt_plan_d VALUES (20,'y');\nINSERT INTO nsqlt_plan_e VALUES (1,10,'a');\nINSERT INTO nsqlt_plan_e VALUES (2,20,'b');\nCOMMIT;\n",
            "SELECT e.name, d.name FROM nsqlt_plan_e e JOIN nsqlt_plan_d d ON d.id = e.dept WHERE e.name = 'a'", false);
        match r.explain("SELECT e.name, d.name FROM nsqlt_plan_e e JOIN nsqlt_plan_d d ON d.id = e.dept WHERE e.name = 'a'", nsql_script::plan::PlanOptions { analyze: true }) {
            Ok(rep) => eprintln!("[plan] oracle actual({})\n{}", d.as_str(), nsql_script::plan::render_text(&rep)),
            Err(e) => eprintln!("[plan] oracle actual({}) 생략: {}", d.as_str(), e.message),
        }
        run(
            &mut r,
            "DROP TABLE nsqlt_plan_e;\nDROP TABLE nsqlt_plan_d;\n",
        );
    }
}

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
        move |s: &ConnectSpec| -> Result<Box<dyn Session>, DbError> { open_session(s, dialect) },
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

/// ★ Oracle 시험은 **두 드라이버로**(docs/110 · T-336 · 사용자 10-13 "두 실행의 기능/결과 동일"): `NSQL_ORACLE_DRIVERS=oci,thin`
/// (기본 = 이 빌드에 든 것 전부). 드라이버 선택은 레지스트리 전역이라 접속 전에 바꾼다(ODPI-C는 OCI를 처음 쓸 때만 초기화).
fn oracle_drivers() -> Vec<nsql_drivers::OracleDriver> {
    use nsql_drivers::OracleDriver as D;
    match std::env::var("NSQL_ORACLE_DRIVERS") {
        Ok(v) => v
            .split(',')
            .map(|x| D::parse(x.trim()))
            .filter(|d| d.included())
            .collect(),
        Err(_) => [D::Oci, D::Thin]
            .into_iter()
            .filter(|d| d.included())
            .collect(),
    }
}

/// 같은 시험 본문을 Oracle 드라이버마다 한 번씩(건너뛰면 한 번만 알린다).
/// Oracle 드라이버 선택은 **프로세스 전역**(`set_oracle_driver`) — 병렬 시험이 겹치면 OCI 반복이 Thin으로 붙을 수 있어 직렬화(124차 리뷰).
static ORACLE_DRIVER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn for_each_oracle(body: impl Fn(Runner)) {
    let _guard = ORACLE_DRIVER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for d in oracle_drivers() {
        nsql_drivers::set_oracle_driver(d);
        eprintln!("[oracle driver] {}", d.as_str());
        let Some(r) = runner_for("NSQL_ORACLE_URL", Dialect::Oracle) else {
            return;
        };
        body(r);
    }
    nsql_drivers::set_oracle_driver(nsql_drivers::OracleDriver::Thin);
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
    for_each_oracle(|mut r| {
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
    });
}

#[test]
fn oracle_plsql_block_out_binds_and_dml() {
    for_each_oracle(|mut r| {
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
    });
}

#[test]
fn mssql_session_variables_and_select_into() {
    let Some(mut r) = runner_for("NSQL_MSSQL_URL", Dialect::Mssql) else {
        return;
    };
    let ev = run(
        &mut r,
        "EXEC :V_NAME := 'nexa'\nEXEC SELECT COUNT(*) INTO :V_CNT FROM sys.objects WHERE object_id > 0\nPRINT V_CNT\nSELECT :V_NAME AS n, :V_CNT AS c;\nEXEC SELECT TOP 1 name INTO :V_OBJ FROM sys.objects ORDER BY name\nPRINT V_OBJ\n",
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
    for_each_oracle(|mut r| {
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
        // OCI = DATE(Typ=12) · Thin = TIMESTAMP(Typ=18 · oracledb beta.4는 DATE IN 바인드 타입이 없다 · 값은 같다 · docs/110 §6).
        let typ = rs.rows[0][1].display();
        if nsql_drivers::oracle_driver() == nsql_drivers::OracleDriver::Thin {
            assert!(typ == "Typ=12" || typ == "Typ=18", "{typ}");
        } else {
            assert_eq!(typ, "Typ=12", "글자가 아니라 DATE로 바인드된다");
        }
        run(&mut r, "EXEC :d := :d + 1\n");
        assert_eq!(get(&r, "D").as_deref(), Some("2026-09-22 13:45:10"));
        run(&mut r, "BEGIN :b := (1 = 1); END;\n/\n");
        assert_eq!(get(&r, "B").as_deref(), Some("true"));
        run(&mut r, "BEGIN :b := NOT :b; END;\n/\n");
        assert_eq!(get(&r, "B").as_deref(), Some("false"));
        // 서명 추론: SYS 패키지의 OUT 인자(VARCHAR2 · INTEGER) — 선언 없는 바인드로 받는다.
        run(&mut r, "EXEC DBMS_OUTPUT.GET_LINE(:LINE, :STATUS)\n");
        assert_eq!(get(&r, "STATUS").as_deref(), Some("1"));
    });
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
    let opener: Opener = Box::new(|s: &ConnectSpec| open_session(s, Dialect::Sqlite));
    let mut r = Runner::new(Dialect::Sqlite, opener);
    let spec = nsql_drivers::parse_target("sqlite::memory:", Dialect::Sqlite).unwrap();
    let mut ev = Vec::new();
    assert!(r.connect(&spec, &mut |e| ev.push(e)));
    let ev = run(&mut r, "EXEC :A := 41\nSELECT :A + 1 AS b;\n");
    assert_eq!(first_rs(&ev).unwrap().rows[0][0], Value::Int(42));
}

/// ★ OCI ↔ Thin 동일성(docs/110 §3-3 · T-336): 같은 조회의 **열 이름·타입 글·값**이 두 드라이버에서 같다(그리드·복사·편집 판정이 읽는 것).
/// 읽기 전용(dual) — 실서버 프로필로도 돈다. 두 드라이버가 다 빌드에 있을 때만.
#[test]
fn oracle_driver_parity_columns_and_values() {
    use nsql_core::ExecRequest;
    use nsql_drivers::OracleDriver as D;
    if !(D::Oci.included() && D::Thin.included()) {
        eprintln!("[skip] 두 드라이버가 다 들어 있지 않음");
        return;
    }
    let profile = std::env::var("NSQL_ORACLE_PROFILE").ok();
    let spec = match (std::env::var("NSQL_ORACLE_URL"), profile) {
        (Ok(url), _) => nsql_drivers::parse_target(&url, Dialect::Oracle).expect("접속 문자열"),
        (Err(_), Some(name)) => nsql_vault::Vault::open_default()
            .and_then(|v| v.get(&name))
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("프로필 없음: {name}")),
        _ => {
            eprintln!("[skip] NSQL_ORACLE_URL 없음");
            return;
        }
    };
    const SQLS: &[&str] = &[
        "SELECT 1 AS n, 1.5 AS d, -0.5 AS neg, 1E20 AS big, 123456789012345678901234567890 AS huge, CAST(12345.678 AS NUMBER(10,2)) AS fixed FROM dual",
        "SELECT CAST(1.25 AS BINARY_DOUBLE) AS bd, CAST(1.5 AS BINARY_FLOAT) AS bf, CAST(3 AS FLOAT) AS fl, CAST(7 AS NUMBER(5)) AS n5 FROM dual",
        // `VARCHAR2(10 CHAR)`는 뺀다 = 알려진 타입 글 차이(OCI 바이트 40 · Thin 선언 10 · docs/110 §6).
        "SELECT 'abc' AS s, N'한글' AS ns, CAST('x' AS CHAR(3)) AS c3, CAST('가' AS VARCHAR2(10)) AS v10, CAST(NULL AS VARCHAR2(10)) AS nul FROM dual",
        "SELECT DATE '2026-09-21' AS dt, TIMESTAMP '2026-09-21 13:45:10.123456' AS ts, CAST(TIMESTAMP '2026-09-21 13:45:10.5' AS TIMESTAMP(3)) AS ts3, TIMESTAMP '2026-09-21 13:45:10.5 +09:00' AS tstz FROM dual",
        "SELECT INTERVAL '1 02:03:04.5' DAY TO SECOND AS ids, INTERVAL '1-2' YEAR TO MONTH AS iym, HEXTORAW('DEADBEEF') AS rw, ROWID AS rid, TO_CLOB('clob 한글') AS cl FROM dual",
        "SELECT * FROM (SELECT LEVEL AS lv FROM dual CONNECT BY LEVEL <= 1200) ORDER BY lv",
    ];
    // 두 드라이버가 다 있을 때만(`NSQL_ORACLE_DRIVERS` · Instant Client 없는 PC = Thin만 → 건너뜀).
    let drivers = oracle_drivers();
    if drivers.len() < 2 {
        eprintln!("[skip] 패리티는 OCI+Thin 둘 다 필요({drivers:?})");
        return;
    }
    let mut per_driver: Vec<(D, Vec<nsql_core::ResultSet>)> = Vec::new();
    for d in drivers {
        nsql_drivers::set_oracle_driver(d);
        let mut s = open_session(&spec, Dialect::Oracle)
            .unwrap_or_else(|e| panic!("{} 접속 실패: {}", d.as_str(), e.message));
        assert_eq!(s.dialect(), Dialect::Oracle);
        let mut sets = Vec::new();
        for sql in SQLS {
            let r = s
                .execute(&ExecRequest {
                    sql: (*sql).to_string(),
                    params: Vec::new(),
                })
                .unwrap_or_else(|e| panic!("{} 실행 실패: {} — {}", d.as_str(), e.message, sql));
            sets.push(r.result_sets.into_iter().next().expect("결과"));
        }
        per_driver.push((d, sets));
    }
    nsql_drivers::set_oracle_driver(D::Thin);
    let (oci, thin) = (&per_driver[0].1, &per_driver[1].1);
    for (i, sql) in SQLS.iter().enumerate() {
        assert_eq!(
            oci[i].columns, thin[i].columns,
            "열(이름·타입 글)이 다르다: {sql}"
        );
        assert_eq!(oci[i].rows, thin[i].rows, "값이 다르다: {sql}");
    }
}

// ───────────────────────────────────────────── MySQL/MariaDB(135차 · nsql-driver-mysql)

/// 세션 변수 · `SELECT … INTO`(텍스트 프로토콜 = 열 타입으로 값 복원) · 바인드 조회(준비문 = 이진 프로토콜) · 타입 표본.
#[test]
fn mysql_session_variables_select_into_and_types() {
    let Some(mut r) = runner_for("NSQL_MYSQL_URL", Dialect::Mysql) else {
        return;
    };
    let ev = run(
        &mut r,
        "EXEC :V_NAME := 'nexa'\nEXEC SELECT COUNT(*) INTO :V_CNT FROM (SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3) t\nSELECT :V_NAME AS n, :V_CNT AS c;\nSELECT 1234.5 AS d, DATE '2026-07-23' AS dt, TIMESTAMP '2026-07-23 01:02:03' AS ts, 0x414243 AS b, CAST(1 AS UNSIGNED) AS u, NULL AS nul;\n",
    );
    assert_eq!(
        r.engine.vars.get("V_CNT").unwrap().value,
        Value::Int(3),
        "{ev:#?}"
    );
    let sets: Vec<&nsql_core::ResultSet> = ev
        .iter()
        .filter_map(|e| match e {
            RunEvent::ResultSet { rs, .. } => Some(rs),
            _ => None,
        })
        .collect();
    assert_eq!(sets.len(), 2, "{ev:#?}");
    // 바인드 조회 = 준비문(이진) — 글자·정수가 타입 그대로.
    assert_eq!(
        sets[0].rows[0],
        vec![Value::Str("nexa".into()), Value::Int(3)]
    );
    assert_eq!(sets[0].columns[0].name, "n");
    // 텍스트 프로토콜 = 컬럼 타입으로 복원.
    let row = &sets[1].rows[0];
    assert_eq!(row[0], Value::Decimal("1234.5".into()), "{row:?}");
    assert_eq!(row[1], Value::Str("2026-07-23".into()));
    assert_eq!(row[2], Value::Str("2026-07-23 01:02:03".into()));
    assert_eq!(
        row[3],
        Value::Bytes(b"ABC".to_vec()),
        "0x… = VARBINARY → 바이트"
    );
    assert_eq!(row[4], Value::Int(1));
    assert_eq!(row[5], Value::Null);
    assert_eq!(sets[1].columns[0].type_name, "DECIMAL");
    // 정수 상수 표현식의 타입 이름 = MySQL 8 `BIGINT UNSIGNED` · MariaDB 11.4 `INT UNSIGNED`(서버 차이 · 10-22 실측).
    let t4 = sets[1].columns[4].type_name.as_str();
    assert!(matches!(t4, "BIGINT UNSIGNED" | "INT UNSIGNED"), "{t4}");
    // 여러 행 · 0행 정책(D-139).
    let (errs, ev) = run_raw(
        &mut r,
        "EXEC SELECT n INTO :A FROM (SELECT 1 AS n UNION ALL SELECT 2) t\n",
    );
    assert_eq!(errs, 1, "여러 행 = 오류: {ev:#?}");
    let (errs, ev) = run_raw(
        &mut r,
        "EXEC SELECT n INTO :A FROM (SELECT 1 AS n) t WHERE n > 9\n",
    );
    assert_eq!(errs, 1, "0행 = 오류: {ev:#?}");
}

/// `EXEC p(:A, :B)` = `CALL p(?, ?)` 준비문 — INOUT/OUT 값은 **마지막 결과 집합 한 행**으로 돌아오고, 받을 자리는 서명
/// (`information_schema.parameters` · `CallSignature::Captures`)이 정한다. 프로시저 본문의 `;`는 `BEGIN … END` 짝으로 넘긴다.
#[test]
fn mysql_procedure_out_binds_follow_signature() {
    let Some(mut r) = runner_for("NSQL_MYSQL_URL", Dialect::Mysql) else {
        return;
    };
    run(
        &mut r,
        "DROP PROCEDURE IF EXISTS nsql_it_io;
CREATE PROCEDURE nsql_it_io(IN a INT, INOUT b INT, INOUT msg VARCHAR(100))
BEGIN
  IF a > 0 THEN
    SET b = a * b;
  END IF;
  SET msg = CONCAT(msg, ' -> ', b);
END;
EXEC :X := 21
EXEC :Y := 'in'
EXEC nsql_it_io(3, :X, :Y)
",
    );
    let get = |r: &Runner, n: &str| r.engine.vars.get(n).map(|v| v.value.display());
    assert_eq!(get(&r, "X").as_deref(), Some("63"));
    assert_eq!(get(&r, "Y").as_deref(), Some("in -> 63"));
    run(&mut r, "EXEC :M := 'lit'\nEXEC nsql_it_io(10, 4, :M)\n");
    assert_eq!(get(&r, "M").as_deref(), Some("lit -> 40"));
    assert_eq!(
        get(&r, "X").as_deref(),
        Some("63"),
        "상수를 준 INOUT 자리는 변수를 건드리지 않는다"
    );
    // 프로시저 안의 SELECT = 결과 집합 · 뒤의 OUT 행은 결과로 남지 않는다(흡수).
    let ev = run(
        &mut r,
        "DROP PROCEDURE IF EXISTS nsql_it_sel;
CREATE PROCEDURE nsql_it_sel(OUT n INT)
BEGIN
  SELECT 'row' AS r;
  SET n = 7;
END;
EXEC nsql_it_sel(:N)
DROP PROCEDURE nsql_it_sel;
DROP PROCEDURE nsql_it_io;
",
    );
    assert_eq!(get(&r, "N").as_deref(), Some("7"), "{ev:#?}");
    let sets: Vec<&nsql_core::ResultSet> = ev
        .iter()
        .filter_map(|e| match e {
            RunEvent::ResultSet { rs, .. } => Some(rs),
            _ => None,
        })
        .collect();
    assert_eq!(sets.len(), 1, "{ev:#?}");
    assert_eq!(sets[0].rows[0][0], Value::Str("row".into()));
}

/// 수동 커밋(러너가 `START TRANSACTION`을 보낸다 · `Caps.tx_begin`) — ROLLBACK은 되돌리고 COMMIT은 남긴다 · 임시 표라 서버에 남지 않는다.
#[test]
fn mysql_manual_commit_and_rollback() {
    let Some(mut r) = runner_for("NSQL_MYSQL_URL", Dialect::Mysql) else {
        return;
    };
    run(
        &mut r,
        "CREATE TEMPORARY TABLE nsql_it_t (id INT) ENGINE=InnoDB;\nSET AUTOCOMMIT OFF\nINSERT INTO nsql_it_t VALUES (1);\nROLLBACK;\nEXEC SELECT COUNT(*) INTO :C1 FROM nsql_it_t\nINSERT INTO nsql_it_t VALUES (2);\nCOMMIT;\nEXEC SELECT COUNT(*) INTO :C2 FROM nsql_it_t\nSET AUTOCOMMIT ON\n",
    );
    let get = |r: &Runner, n: &str| r.engine.vars.get(n).map(|v| v.value.display());
    assert_eq!(get(&r, "C1").as_deref(), Some("0"));
    assert_eq!(get(&r, "C2").as_deref(), Some("1"));
}

/// 드라이버 직접: 경고 = `SHOW WARNINGS` 메시지 · 문법 오류 `at line N` = 그 줄 머리의 글자 위치(T-371) · 취소 핸들 있음 ·
/// `max_rows` 상한 · 백틱·`#` 주석이 든 문장.
/// ★ T-388 TLS 실측(`NSQL_MYSQL_URL` · `NSQL_MARIADB_URL` 둘 다 docker 기본 = 자체 서명 인증서로 TLS 켬): `?tls=required` =
/// Ssl_cipher 있음 · `?tls=disabled` = 없음 · `?tls=preferred` = 서버가 되면 암호화 · `?tls=verify_identity`(CA 없음) = 실패
/// (검증 실패 뒤 멈춤은 드라이버의 20 s 상한이 오류로 바꾼다 · MySQL 쪽 한 번만 잰다). `SHOW SESSION STATUS`는 MySQL 방언에서
/// 서버 문장으로 가는 패스스루(split `command_or_sql`)까지 함께 확인한다.
#[test]
fn mysql_tls_modes_follow_url_param() {
    use nsql_core::ExecRequest;
    fn cipher(url: &str, tls: &str) -> Result<String, String> {
        let sep = if url.contains('?') { '&' } else { '?' };
        let spec = nsql_drivers::parse_target(&format!("{url}{sep}tls={tls}"), Dialect::Mysql)
            .expect("접속 문자열");
        let mut s = open_session(&spec, Dialect::Mysql).map_err(|e| e.message)?;
        let r = s
            .execute(&ExecRequest {
                sql: "SHOW SESSION STATUS LIKE 'Ssl_cipher'".to_string(),
                params: Vec::new(),
            })
            .map_err(|e| e.message)?;
        assert_eq!(r.result_sets.len(), 1, "SHOW … = 결과 집합 하나");
        Ok(r.result_sets
            .first()
            .and_then(|rs| rs.rows.first())
            .and_then(|row| row.get(1))
            .map(nsql_core::Value::display)
            .unwrap_or_default())
    }
    let on = |v: &str| !v.is_empty() && v != "NULL";
    let mut any = false;
    if let Ok(url) = std::env::var("NSQL_MYSQL_URL") {
        any = true;
        let req = cipher(&url, "required").expect("mysql required");
        assert!(on(&req), "required → Ssl_cipher: {req:?}");
        let dis = cipher(&url, "disabled").expect("mysql disabled");
        assert!(!on(&dis), "disabled → 평문: {dis:?}");
        let pre = cipher(&url, "preferred").expect("mysql preferred");
        assert!(on(&pre), "preferred → 서버가 되면 암호화: {pre:?}");
        let started = std::time::Instant::now();
        let vid = cipher(&url, "verify_identity");
        assert!(
            vid.is_err(),
            "verify_identity(CA 없음 · 자체 서명) = 실패: {vid:?}"
        );
        assert!(
            started.elapsed().as_secs() <= 40,
            "검증 실패 멈춤은 상한 안에 오류로 끝난다"
        );
    }
    if let Ok(url) = std::env::var("NSQL_MARIADB_URL") {
        any = true;
        let req = cipher(&url, "required").expect("mariadb required");
        assert!(
            on(&req),
            "MariaDB(docker 11.4 = TLS 켬) required → Ssl_cipher: {req:?}"
        );
        let dis = cipher(&url, "disabled").expect("mariadb disabled");
        assert!(!on(&dis), "disabled → 평문: {dis:?}");
        let pre = cipher(&url, "preferred").expect("mariadb preferred");
        assert!(on(&pre), "preferred → 암호화: {pre:?}");
    }
    if !any {
        eprintln!("[skip] NSQL_MYSQL_URL/NSQL_MARIADB_URL 없음");
    }
}

/// ★ T-70 후속(158차 · 사용자 10-28 결정 ⓐ): CI의 **자체 서명 TLS PG**(`NSQL_PG_TLS_URL` · CA 파일 = `NSQL_PG_TLS_CA`)로 모드 5종 ·
/// 인증서 오류 원문(사슬) · `tls_ca=` 검증 · preferred의 "(TLS on)" 꼬리 · **TLS 세션의 취소**(같은 TLS로 cancel_query · 57014).
/// 내장(rlib)·확장(`NSQL_TEST_DYN=1` · cdylib) 두 경로 모두 같은 단언. env가 없으면 생략.
#[test]
fn pg_tls_modes_follow_url_param() {
    use nsql_core::ExecRequest;
    let Ok(url) = std::env::var("NSQL_PG_TLS_URL") else {
        eprintln!("[skip] NSQL_PG_TLS_URL 없음");
        return;
    };
    let ca = std::env::var("NSQL_PG_TLS_CA").ok();
    let probe = "SELECT ssl FROM pg_stat_ssl WHERE pid = pg_backend_pid()";
    // (세션, pg_stat_ssl.ssl 글자, 설명) — 접속·조회 실패는 오류 문구로.
    let open =
        |tls: &str, ca: Option<&str>| -> Result<(Box<dyn Session>, String, String), String> {
            let sep = if url.contains('?') { '&' } else { '?' };
            let mut u = format!("{url}{sep}tls={tls}");
            if let Some(ca) = ca {
                u.push_str(&format!("&tls_ca={ca}"));
            }
            let spec = nsql_drivers::parse_target(&u, Dialect::Postgres)?;
            let mut s = open_session(&spec, Dialect::Postgres).map_err(|e| e.message)?;
            let r = s
                .execute(&ExecRequest {
                    sql: probe.to_string(),
                    params: Vec::new(),
                })
                .map_err(|e| e.message)?;
            let on = r
                .result_sets
                .first()
                .and_then(|rs| rs.rows.first())
                .and_then(|row| row.first())
                .map(nsql_core::Value::display)
                .unwrap_or_default();
            let desc = s.describe();
            Ok((s, on, desc))
        };
    let on = |v: &str| v == "t" || v.eq_ignore_ascii_case("true");
    // 1. required · preferred = 암호화 · disabled = 평문 · preferred는 설명 꼬리 "(TLS on)".
    let (_, v, _) = open("required", None).expect("required");
    assert!(on(&v), "required → ssl: {v:?}");
    let (_, v, d) = open("preferred", None).expect("preferred");
    assert!(on(&v), "preferred → 서버가 되면 암호화: {v:?}");
    assert!(d.ends_with("(TLS on)"), "preferred 설명 꼬리: {d:?}");
    let (_, v, d) = open("disabled", None).expect("disabled");
    assert!(!on(&v), "disabled → 평문: {v:?}");
    assert!(!d.contains("(TLS"), "disabled = 꼬리 없음: {d:?}");
    // 2. 검증 모드 = CA 없으면(자체 서명) 인증서 오류 원문이 사슬로 · 상한 안에 끝난다.
    let started = std::time::Instant::now();
    let e = open("verify_ca", None)
        .err()
        .expect("verify_ca(CA 없음) = 실패");
    assert!(
        started.elapsed().as_secs() <= 40,
        "검증 실패는 상한 안에 오류로 끝난다({e})"
    );
    assert!(
        e.contains("certificate") || e.contains("Unknown") || e.contains("TLS"),
        "인증서 오류 원문이 보인다: {e}"
    );
    let e = open("verify_identity", None)
        .err()
        .expect("verify_identity(CA 없음) = 실패");
    assert!(e.contains("TLS") || e.contains("certificate"), "{e}");
    // 3. CA 파일을 주면 verify_ca · verify_identity(SAN = localhost) 둘 다 암호화 접속.
    if let Some(ca) = ca.as_deref() {
        let (_, v, _) = open("verify_ca", Some(ca)).expect("verify_ca + CA");
        assert!(on(&v), "verify_ca + CA → ssl: {v:?}");
        let (_, v, _) = open("verify_identity", Some(ca)).expect("verify_identity + CA");
        assert!(on(&v), "verify_identity + CA → ssl: {v:?}");
        // libpq 이름도 같은 뜻(`sslmode=verify-full` · `sslrootcert=`).
        let sep = if url.contains('?') { '&' } else { '?' };
        let spec = nsql_drivers::parse_target(
            &format!("{url}{sep}sslmode=verify-full&sslrootcert={ca}"),
            Dialect::Postgres,
        )
        .expect("libpq 이름");
        // 파서는 libpq 이름을 `_`꼴로만 정규화(verify_full) · 드라이버 TlsMode::parse가 둘 다 받는다.
        assert!(
            matches!(spec.tls.as_deref(), Some("verify_identity" | "verify_full")),
            "{:?}",
            spec.tls
        );
        assert_eq!(spec.tls_ca.as_deref(), Some(ca));
    } else {
        eprintln!("[skip] NSQL_PG_TLS_CA 없음 → CA 검증 생략");
    }
    // 4. TLS 세션의 취소 = 취소 접속도 같은 TLS(T-70 · `PgCancel`) → pg_sleep이 57014로 끝난다.
    let (mut s, _, _) = open("required", None).expect("required for cancel");
    let h = s.cancel_handle().expect("PG = 취소 핸들 있음");
    let t = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        h.cancel()
    });
    let started = std::time::Instant::now();
    let r = s.execute(&ExecRequest {
        sql: "SELECT pg_sleep(30)".to_string(),
        params: Vec::new(),
    });
    let cancel = t.join().expect("cancel thread");
    assert!(cancel.is_ok(), "TLS 취소 접속 성공: {cancel:?}");
    let e = r.err().expect("취소된 문장 = 오류");
    assert!(
        e.message.contains("57014") || e.message.contains("cancel"),
        "취소 오류 원문: {}",
        e.message
    );
    assert!(
        started.elapsed().as_secs() < 20,
        "취소가 먹혀 30 s 전에 끝난다({:?})",
        started.elapsed()
    );
}

/// ★ 대량 적재(89 §1-5 · T-387 후속): `LOAD DATA LOCAL INFILE` 싱크 왕복 — 임시 표에 3행(탭·줄바꿈·역슬래시·한글·NULL·바이트) →
/// `finish` = 3 → 재조회 값 동일. 서버 `local_infile=OFF`면 `bulk_begin` 거절(러너 폴백 경로) = 건너뜀으로 기록.
/// `NSQL_MARIADB_URL`이 있으면 MariaDB로도 같은 표를 돈다.
#[test]
fn mysql_bulk_load_local_infile_round_trip() {
    use nsql_core::ExecRequest;
    fn run(url: &str, tag: &str) {
        let spec = nsql_drivers::parse_target(url, Dialect::Mysql).expect("접속 문자열");
        let mut s = open_session(&spec, Dialect::Mysql)
            .unwrap_or_else(|e| panic!("{tag} 접속 실패: {}", e.message));
        let exec = |s: &mut Box<dyn Session>, sql: &str| {
            s.execute(&ExecRequest {
                sql: sql.to_string(),
                params: Vec::new(),
            })
            .unwrap_or_else(|e| panic!("{tag} {sql}: {}", e.message))
        };
        exec(
            &mut s,
            "CREATE TEMPORARY TABLE nsql_it_bulk (id INT, name VARCHAR(50), memo TEXT NULL, bin VARBINARY(10))",
        );
        let cols: Vec<String> = ["id", "name", "memo", "bin"]
            .iter()
            .map(|c| c.to_string())
            .collect();
        let types = vec![String::new(); 4];
        let rows = vec![
            vec![
                Value::Int(1),
                Value::Str("a\tb".into()),
                Value::Null,
                Value::Bytes(vec![0, 1, 255]),
            ],
            vec![
                Value::Int(2),
                Value::Str("line\nbreak".into()),
                Value::Str("x\\y".into()),
                Value::Bytes(Vec::new()),
            ],
            vec![
                Value::Int(3),
                Value::Str("한글".into()),
                Value::Str("ok".into()),
                Value::Null,
            ],
        ];
        let total = {
            let mut sink = match s.bulk_begin(
                "nsql_it_bulk",
                &cols,
                &types,
                &nsql_core::BulkOpts::default(),
            ) {
                Ok(k) => k,
                Err(e) => {
                    eprintln!(
                        "[skip] {tag} LOAD DATA LOCAL 거절(서버 local_infile OFF?): {}",
                        e.message
                    );
                    return;
                }
            };
            for r in &rows {
                sink.push(r).expect("push");
            }
            assert_eq!(sink.flush().expect("flush"), 3, "{tag} flush = 3행");
            assert!(sink.commit().expect("commit"));
            sink.finish().expect("finish")
        };
        assert_eq!(total, 3, "{tag} finish = 3행");
        let r = exec(
            &mut s,
            "SELECT id, name, memo, bin FROM nsql_it_bulk ORDER BY id",
        );
        let got = &r.result_sets[0].rows;
        assert_eq!(got.len(), 3, "{tag} {got:?}");
        assert_eq!(got[0][1], Value::Str("a\tb".into()), "{tag} 탭 보존");
        assert_eq!(got[0][2], Value::Null, "{tag} NULL");
        assert_eq!(got[0][3], Value::Bytes(vec![0, 1, 255]), "{tag} 바이트");
        assert_eq!(
            got[1][1],
            Value::Str("line\nbreak".into()),
            "{tag} 줄바꿈 보존"
        );
        assert_eq!(got[1][2], Value::Str("x\\y".into()), "{tag} 역슬래시 보존");
        assert_eq!(
            got[1][3],
            Value::Bytes(Vec::new()),
            "{tag} 빈 바이트 ≠ NULL"
        );
        assert_eq!(got[2][1], Value::Str("한글".into()), "{tag} utf8mb4");
        exec(&mut s, "DROP TEMPORARY TABLE nsql_it_bulk");
    }
    let mut any = false;
    if let Ok(url) = std::env::var("NSQL_MYSQL_URL") {
        any = true;
        run(&url, "mysql");
    }
    if let Ok(url) = std::env::var("NSQL_MARIADB_URL") {
        any = true;
        run(&url, "mariadb");
    }
    if !any {
        eprintln!("[skip] NSQL_MYSQL_URL/NSQL_MARIADB_URL 없음");
    }
}

/// ★ ODBC 대상(T-383): `NSQL_ODBC_URL`(CI = `odbc:Driver={PostgreSQL Unicode};…`) · 없으면 Windows 로컬에서 저장된 SQL Server
/// 프로필(`NSQL_MSSQL_PROFILE`)을 인박스 "SQL Server" ODBC 드라이버로 바꿔 쓴다(비밀번호는 금고에서 · 환경 변수에 적지 않는다).
fn odbc_target() -> Option<String> {
    if let Ok(u) = std::env::var("NSQL_ODBC_URL") {
        return Some(u);
    }
    // ★ 10-24(138차 Linux · 사용자 "ODBC 접속 테스트"): 저장 프로필 → ODBC 접속 문자열(3-OS · 비밀번호는 금고에서만 · 환경 변수·로그에 안 남긴다).
    //   `NSQL_ODBC_FROM_PROFILE=<이름>` (+ 선택 `NSQL_ODBC_DRIVER=<odbcinst 등록 이름>` · 기본 = 방언별: PG "PostgreSQL Unicode" ·
    //   SQL Server = unix "ODBC Driver 18 for SQL Server"(+TrustServerCertificate=yes) / Windows 인박스 "SQL Server").
    if let Ok(name) = std::env::var("NSQL_ODBC_FROM_PROFILE") {
        return odbc_url_from_profile(&name);
    }
    if cfg!(windows) {
        if let Ok(name) = std::env::var("NSQL_MSSQL_PROFILE") {
            let spec = nsql_vault::Vault::open_default()
                .and_then(|v| v.get(&name))
                .ok()
                .flatten()?;
            return Some(format!(
                "odbc:Driver={{SQL Server}};Server={},{};Database={};Uid={};Pwd={}",
                spec.host.as_deref()?,
                spec.port.unwrap_or(1433),
                spec.database.as_deref().unwrap_or("master"),
                spec.user.as_deref()?,
                spec.password.as_deref()?
            ));
        }
    }
    None
}

/// 저장 프로필(기본 금고) → ODBC 접속 문자열(10-24 · 비밀번호는 반환값 안에만 · 호출자가 로그에 찍지 않는다).
fn odbc_url_from_profile(name: &str) -> Option<String> {
    let spec = nsql_vault::Vault::open_default()
        .and_then(|v| v.get(name))
        .ok()
        .flatten()?;
    let (host, user, pw) = (
        spec.host.as_deref()?,
        spec.user.as_deref()?,
        spec.password.as_deref()?,
    );
    let drv = std::env::var("NSQL_ODBC_DRIVER").ok();
    Some(match spec.dialect? {
        Dialect::Postgres => format!(
            "odbc:Driver={{{}}};Server={host};Port={};Database={};Uid={user};Pwd={pw}",
            drv.as_deref().unwrap_or("PostgreSQL Unicode"),
            spec.port.unwrap_or(5432),
            spec.database.as_deref().unwrap_or("postgres"),
        ),
        Dialect::Mssql => format!(
            "odbc:Driver={{{}}};Server={host},{};Database={};Uid={user};Pwd={pw};TrustServerCertificate=yes",
            drv.as_deref().unwrap_or(if cfg!(windows) {
                "SQL Server"
            } else {
                "ODBC Driver 18 for SQL Server"
            }),
            spec.port.unwrap_or(1433),
            spec.database.as_deref().unwrap_or("master"),
        ),
        other => {
            eprintln!("[skip] NSQL_ODBC_FROM_PROFILE: {other:?} 방언은 ODBC 문자열 규칙 없음");
            return None;
        }
    })
}

/// ★ GUI ODBC 실접속 E2E 준비(10-24 · ignored · `scripts/linux-odbc-e2e.sh` ⑦): 저장 프로필의 자격 증명으로 **격리 홈**(`NSQL_ODBC_TARGET_HOME`)
/// 금고에 ODBC 프로필(`NSQL_ODBC_PROFILE_NAME` · 기본 `odbc-e2e`)을 쓴다 — 비밀번호는 프로세스 안에서만 · 실제 홈은 읽기만.
#[test]
#[ignore = "격리 홈에 프로필을 쓴다 — E2E 스크립트가 부른다"]
fn odbc_write_profile_into_isolated_home() {
    let Ok(src) = std::env::var("NSQL_ODBC_FROM_PROFILE") else {
        eprintln!("[skip] NSQL_ODBC_FROM_PROFILE 없음");
        return;
    };
    let Ok(home) = std::env::var("NSQL_ODBC_TARGET_HOME") else {
        eprintln!("[skip] NSQL_ODBC_TARGET_HOME 없음");
        return;
    };
    let name = std::env::var("NSQL_ODBC_PROFILE_NAME").unwrap_or_else(|_| "odbc-e2e".into());
    let url = odbc_url_from_profile(&src).expect("원본 프로필(호스트·사용자·비밀번호·방언)");
    let spec = nsql_drivers::parse_target(&url, Dialect::Odbc).expect("접속 문자열");
    assert_eq!(spec.dialect, Some(Dialect::Odbc));
    assert!(spec.password.is_some(), "Pwd= → 스펙 비밀번호");
    std::fs::create_dir_all(&home).expect("격리 홈");
    let v = nsql_vault::Vault::open(&home).expect("격리 금고");
    v.save(&name, &spec).expect("프로필 저장");
    let back = v.get(&name).expect("읽기").expect("있음");
    assert_eq!(
        back.password, spec.password,
        "격리 금고 왕복 = 같은 비밀번호"
    );
    eprintln!("[ok] wrote odbc profile {name} into {home}");
}

/// ★ ODBC 1차(T-383): 접속 · 타입 복원(정수·실수·문자열·NULL) · `?` 바인드(문자열·정수) · DML 영향 행 수 · 커밋 · 오류 진단 코드.
/// 이식성 = PostgreSQL ODBC(CI)와 SQL Server ODBC(Windows 로컬) 양쪽에서 같은 SQL이 돌게 표준 SQL만 쓴다 · 임시 객체 `nsqlt_odbc`는 끝에 DROP.
#[test]
fn odbc_select_bind_dml_and_errors() {
    use nsql_core::{BindParam, Direction, ExecRequest, VarType};
    if !nsql_drivers::available().contains(&Dialect::Odbc) {
        eprintln!("[skip] 이 빌드에 ODBC 드라이버 없음(D-345 = Windows만)");
        return;
    }
    let Some(url) = odbc_target() else {
        eprintln!("[skip] NSQL_ODBC_URL 없음(Windows = NSQL_MSSQL_PROFILE로 대체)");
        return;
    };
    let spec = nsql_drivers::parse_target(&url, Dialect::Odbc).expect("접속 문자열");
    assert_eq!(spec.dialect, Some(Dialect::Odbc), "{spec:?}");
    let mut s = open_session(&spec, Dialect::Odbc)
        .unwrap_or_else(|e| panic!("ODBC 접속 실패: {}", e.message));
    assert_eq!(s.dialect(), Dialect::Odbc);
    assert!(s.is_alive());
    assert!(s.driver_name().contains("odbc-api"), "{}", s.driver_name());
    let exec = |s: &mut Box<dyn Session>, sql: &str, params: Vec<BindParam>| {
        s.execute(&ExecRequest {
            sql: sql.to_string(),
            params,
        })
    };
    let r = exec(
        &mut s,
        "SELECT CAST(1 AS INTEGER) AS i, CAST(2.5 AS FLOAT) AS f, 'ab' AS t, CAST(NULL AS INTEGER) AS n",
        Vec::new(),
    )
    .expect("select");
    let rs = &r.result_sets[0];
    assert_eq!(
        rs.columns
            .iter()
            .map(|c| c.name.to_ascii_lowercase())
            .collect::<Vec<_>>(),
        ["i", "f", "t", "n"]
    );
    assert_eq!(rs.rows[0][0], Value::Int(1), "{:?}", rs.rows[0]);
    assert_eq!(rs.rows[0][1], Value::Float(2.5));
    assert_eq!(rs.rows[0][2], Value::Str("ab".into()));
    assert_eq!(rs.rows[0][3], Value::Null);
    // ? 바인드(문자열 · 정수) — 위치 순.
    let r = exec(
        &mut s,
        "SELECT ? AS p1, CAST(? AS INTEGER) + 1 AS p2",
        vec![
            BindParam {
                name: "1".into(),
                value: Value::Str("xy".into()),
                ty: VarType::Varchar2(0),
                direction: Direction::In,
            },
            BindParam {
                name: "2".into(),
                value: Value::Int(41),
                ty: VarType::Number,
                direction: Direction::In,
            },
        ],
    )
    .expect("bind");
    assert_eq!(r.result_sets[0].rows[0][0], Value::Str("xy".into()));
    assert_eq!(r.result_sets[0].rows[0][1], Value::Int(42));
    // DML + 영향 행 수 + 커밋(자동 커밋 끔 = ODBC API로).
    let _ = exec(&mut s, "DROP TABLE nsqlt_odbc", Vec::new());
    exec(
        &mut s,
        "CREATE TABLE nsqlt_odbc (id INTEGER, name VARCHAR(20))",
        Vec::new(),
    )
    .expect("create");
    let r = exec(
        &mut s,
        "INSERT INTO nsqlt_odbc (id, name) VALUES (?, ?)",
        vec![
            BindParam {
                name: "1".into(),
                value: Value::Int(7),
                ty: VarType::Number,
                direction: Direction::In,
            },
            BindParam {
                name: "2".into(),
                value: Value::Str("한글".into()),
                ty: VarType::Varchar2(0),
                direction: Direction::In,
            },
        ],
    )
    .expect("insert");
    assert_eq!(r.rows_affected, Some(1), "{r:?}");
    s.commit().expect("commit");
    let r = exec(&mut s, "SELECT id, name FROM nsqlt_odbc", Vec::new()).expect("select back");
    assert_eq!(
        r.result_sets[0].rows[0],
        vec![Value::Int(7), Value::Str("한글".into())]
    );
    // ★ 2차: 드라이버 카탈로그(SQLTables/SQLColumns) — 방금 만든 표가 보이고 컬럼 2개가 위치 순.
    let tables = s
        .catalog_tables(None)
        .expect("SQLTables")
        .expect("ODBC = Some");
    let mine = tables
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case("nsqlt_odbc"))
        .unwrap_or_else(|| panic!("nsqlt_odbc not in SQLTables: {} rows", tables.len()));
    assert!(!mine.kind.is_empty(), "{mine:?}");
    let cols = s
        .catalog_columns(Some(&mine.schema), "nsqlt_odbc")
        .expect("SQLColumns")
        .expect("ODBC = Some");
    assert_eq!(
        cols.iter()
            .map(|c| c.name.to_ascii_lowercase())
            .collect::<Vec<_>>(),
        ["id", "name"],
        "{cols:?}"
    );
    assert_eq!(cols[1].size, Some(20), "{:?}", cols[1]);
    exec(&mut s, "DROP TABLE nsqlt_odbc", Vec::new()).expect("drop");
    s.commit().expect("commit 2");
    // ★ 2차: 긴 LOB = 행 단위 스트리밍(32K 상한 없이) · 취소 = SQLCancel(SQL Server 전용 구문 · 그 밖은 건너뜀).
    if s.driver_name().contains("SQL Server") {
        let r = exec(
            &mut s,
            "SELECT REPLICATE(CAST('x' AS VARCHAR(MAX)), 100000) AS big, 7 AS n",
            Vec::new(),
        )
        .expect("lob");
        match &r.result_sets[0].rows[0][0] {
            Value::Str(t) => assert_eq!(t.len(), 100_000, "LOB 길이"),
            other => panic!("LOB = Str 기대: {other:?}"),
        }
        assert_eq!(r.result_sets[0].rows[0][1], Value::Int(7));
        // ★ 10-24: unix MS 드라이버는 auto에서 취소 핸들 없음(SQLCancel 경합 SIGSEGV 회피 · `odbc.cancel`) → 그 절은 건너뛴다.
        if let Some(cancel) = s.cancel_handle() {
            let t0 = std::time::Instant::now();
            let canceller = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(400));
                cancel.cancel()
            });
            let r = exec(&mut s, "WAITFOR DELAY '00:00:20'", Vec::new());
            let took = t0.elapsed();
            canceller.join().expect("join").expect("SQLCancel");
            assert!(r.is_err(), "취소된 실행 = 오류로 끝남: {r:?}");
            assert!(took.as_secs() < 10, "취소까지 {took:?}");
            assert!(s.is_alive(), "취소 뒤 세션 유지");
            let r = exec(&mut s, "SELECT 1 AS ok", Vec::new()).expect("after cancel");
            assert_eq!(r.result_sets[0].rows[0][0], Value::Int(1));
        } else {
            eprintln!("[skip] ODBC cancel handle = None (odbc.cancel auto · unix MS driver = SQLCancel 경합 SIGSEGV 회피) — SQLCancel 절 건너뜀");
        }
    }
    // 오류 = 진단 레코드(SQLSTATE + 드라이버 메시지 · 네이티브 코드).
    let e =
        exec(&mut s, "SELECT * FROM nsqlt_odbc_missing", Vec::new()).expect_err("missing table");
    assert!(!e.message.is_empty(), "{e:?}");
    assert!(s.is_alive(), "오류 뒤에도 세션 유지");
}

#[test]
fn mysql_driver_warnings_error_position_and_limits() {
    use nsql_core::ExecRequest;
    let profile = std::env::var("NSQL_MYSQL_PROFILE").ok();
    let spec = match (std::env::var("NSQL_MYSQL_URL"), profile) {
        (Ok(url), _) => nsql_drivers::parse_target(&url, Dialect::Mysql).expect("접속 문자열"),
        (Err(_), Some(name)) => nsql_vault::Vault::open_default()
            .and_then(|v| v.get(&name))
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("프로필 없음: {name}")),
        _ => {
            eprintln!("[skip] NSQL_MYSQL_URL 없음");
            return;
        }
    };
    let mut s =
        open_session(&spec, Dialect::Mysql).unwrap_or_else(|e| panic!("접속 실패: {}", e.message));
    assert_eq!(s.dialect(), Dialect::Mysql);
    assert!(s.cancel_handle().is_some());
    assert!(s.is_alive());
    let exec = |s: &mut Box<dyn Session>, sql: &str| {
        s.execute(&ExecRequest {
            sql: sql.to_string(),
            params: Vec::new(),
        })
    };
    // 경고(1365 Division by 0) → messages.
    let r = exec(&mut s, "SELECT 1/0 AS z").unwrap();
    assert!(
        r.messages.iter().any(|m| m.contains("1365")),
        "{:?}",
        r.messages
    );
    // 문법 오류 위치 = 2번째 줄 머리(글자 인덱스 9).
    let e = exec(&mut s, "SELECT 1\nFROM nope_tbl WHERE").unwrap_err();
    assert_eq!(e.code, Some(1064), "{}", e.message);
    assert_eq!(e.position, Some(9), "{}", e.message);
    // 없는 표 = 위치 없음.
    let e = exec(&mut s, "SELECT * FROM nsql_no_such_table").unwrap_err();
    assert_eq!(e.code, Some(1146));
    assert_eq!(e.position, None);
    // 백틱 식별자 · `#` 주석 · 백슬래시 이스케이프.
    let r = exec(&mut s, "SELECT 'it\\'s' AS `a b` # tail\n").unwrap();
    assert_eq!(r.result_sets[0].columns[0].name, "a b");
    assert_eq!(r.result_sets[0].rows[0][0], Value::Str("it's".into()));
    // 상한: 5행 중 2행만.
    s.set_option("max_rows", "2").unwrap();
    let r = exec(
        &mut s,
        "SELECT n FROM (SELECT 1 AS n UNION ALL SELECT 2 UNION ALL SELECT 3 UNION ALL SELECT 4 UNION ALL SELECT 5) t",
    )
    .unwrap();
    assert_eq!(
        r.result_sets[0].rows.len(),
        2,
        "상한까지만(나머지는 드라이버가 비운다)"
    );
    s.set_option("max_rows", "0").unwrap();
    // 세션은 그대로 쓸 수 있다(결과를 끝까지 비웠다).
    let r = exec(&mut s, "SELECT DATABASE() AS d, VERSION() AS v").unwrap();
    assert_eq!(r.result_sets[0].rows.len(), 1);
    assert!(s.is_alive());
}

/// ★ ODBC 자동화(서버 없음 · 사용자 10-23 "ODBC 연결도 자동화"): Windows 내장 **Text 드라이버**로 임시 폴더의 CSV를 열어 조회·카탈로그·
/// 자동 커밋 전용 표시를 본다. 드라이버가 없으면(IM002) 건너뜀. 확장 경로(`NSQL_TEST_DYN=1`)로도 같은 결과.
#[test]
fn odbc_text_driver_csv_without_server() {
    use nsql_core::ExecRequest;
    if !cfg!(windows) || !nsql_drivers::available().contains(&Dialect::Odbc) {
        eprintln!("[skip] ODBC = Windows만");
        return;
    }
    let dir = std::env::temp_dir().join(format!("nsql-odbc-text-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(
        dir.join("t.csv"),
        "id,name,amount\r\n1,alpha,1.5\r\n2,beta,\r\n3,감마,3\r\n",
    )
    .expect("csv");
    std::fs::write(
        dir.join("schema.ini"),
        "[t.csv]\r\nFormat=CSVDelimited\r\nColNameHeader=True\r\nCharacterSet=65001\r\nCol1=id Integer\r\nCol2=name Text Width 50\r\nCol3=amount Double\r\n",
    )
    .expect("schema.ini");
    let url = format!(
        "odbc:Driver={{Microsoft Text Driver (*.txt; *.csv)}};Dbq={};Extensions=csv",
        dir.display()
    );
    let spec = nsql_drivers::parse_target(&url, Dialect::Odbc).expect("접속 문자열");
    let mut s = match open_session(&spec, Dialect::Odbc) {
        Ok(s) => s,
        Err(e) if e.message.contains("IM002") => {
            eprintln!("[skip] Text ODBC 드라이버 없음: {}", e.message);
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        Err(e) => panic!("Text ODBC 접속 실패: {e:?}"),
    };
    assert!(
        s.driver_name().contains("autocommit only"),
        "Text 드라이버 = 트랜잭션 없음 표시: {}",
        s.driver_name()
    );
    let r = s
        .execute(&ExecRequest {
            sql: "SELECT id, name, amount FROM [t.csv] ORDER BY id".into(),
            params: Vec::new(),
        })
        .expect("select");
    let rs = &r.result_sets[0];
    assert_eq!(rs.columns.len(), 3);
    assert_eq!(rs.rows.len(), 3, "{rs:?}");
    assert_eq!(rs.rows[0][0], Value::Int(1));
    assert_eq!(rs.rows[0][1], Value::Str("alpha".into()));
    assert!(
        matches!(rs.rows[0][2], Value::Float(f) if (f - 1.5).abs() < 1e-9),
        "{:?}",
        rs.rows[0][2]
    );
    assert_eq!(rs.rows[1][2], Value::Null, "빈 칸 = NULL");
    assert_eq!(
        rs.rows[2][1],
        Value::Str("감마".into()),
        "UTF-8(CharacterSet=65001)"
    );
    // 바인드(`?`) · 집계.
    let r = s
        .execute(&ExecRequest {
            sql: "SELECT COUNT(*) AS n FROM [t.csv] WHERE id > ?".into(),
            params: vec![nsql_core::BindParam {
                name: "1".into(),
                value: Value::Int(1),
                ty: nsql_core::VarType::Number,
                direction: nsql_core::Direction::In,
            }],
        })
        .expect("bind");
    assert_eq!(r.result_sets[0].rows[0][0], Value::Int(2));
    // 카탈로그 훅(SQLTables · SQLColumns).
    let tables = s
        .catalog_tables(None)
        .expect("tables")
        .expect("odbc catalog");
    assert!(
        tables.iter().any(|t| t.name.eq_ignore_ascii_case("t.csv")),
        "{tables:?}"
    );
    let cols = s
        .catalog_columns(None, "t.csv")
        .expect("columns")
        .expect("odbc catalog");
    let names: Vec<String> = cols.iter().map(|c| c.name.to_ascii_lowercase()).collect();
    assert_eq!(names, ["id", "name", "amount"]);
    // 자동 커밋 전용: commit = no-op · rollback = 안내 오류 · 세션 살아 있음.
    assert!(s.commit().is_ok());
    assert!(s.rollback().is_err());
    assert!(s.is_alive());
    // 오류 = 진단 코드가 메시지에.
    let e = s
        .execute(&ExecRequest {
            sql: "SELECT nope FROM [missing.csv]".into(),
            params: Vec::new(),
        })
        .expect_err("missing file");
    assert!(!e.message.is_empty());
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 동봉 런타임(DuckDB의 duckdb.dll/libduckdb.so/.dylib)을 cdylib 옆으로 — download-lib가 `build/libduckdb-sys-*/out/`에 푼 것을 찾는다.
fn copy_bundled_runtime(target: &std::path::Path, id: &str, dest_dir: &std::path::Path) {
    if id != "duckdb" {
        return;
    }
    let name = if cfg!(windows) {
        "duckdb.dll"
    } else if cfg!(target_os = "macos") {
        "libduckdb.dylib"
    } else {
        "libduckdb.so"
    };
    let Ok(rd) = std::fs::read_dir(target.join("build")) else {
        return;
    };
    for e in rd.flatten() {
        if !e
            .file_name()
            .to_string_lossy()
            .starts_with("libduckdb-sys-")
        {
            continue;
        }
        let p = e.path().join("out").join(name);
        if p.is_file() {
            let _ = std::fs::copy(&p, dest_dir.join(name));
            return;
        }
    }
}

// ───────────────────────────────────────────── ClickHouse(138차 · nsql-driver-clickhouse · D-354 · HTTP 인터페이스)

/// ClickHouse = 확장 전용 → 내장 경로(`NSQL_TEST_DYN` 없음)에서는 크레이트를 직접 연다 · 확장 경로에서는 cdylib(ABI v2 · [`open_session`]).
fn clickhouse_spec() -> Option<ConnectSpec> {
    match std::env::var("NSQL_CLICKHOUSE_URL") {
        Ok(url) => {
            Some(nsql_drivers::parse_target(&url, Dialect::Clickhouse).expect("접속 문자열"))
        }
        Err(_) => {
            eprintln!("[skip] NSQL_CLICKHOUSE_URL 없음");
            None
        }
    }
}

fn clickhouse_open(spec: &ConnectSpec) -> Result<Box<dyn Session>, DbError> {
    if std::env::var_os("NSQL_TEST_DYN").is_some() {
        return open_session(spec, Dialect::Clickhouse);
    }
    Ok(Box::new(nsql_driver_clickhouse::ChSession::open(spec)?))
}

/// 스크립트 경로(러너 · 세션 변수): DDL(Memory 엔진) · 다중 행 INSERT · `EXEC :V := 리터럴`(로컬) · `?` 자리 = 드라이버 리터럴 치환 · 한글 ·
/// 타입 복원(UInt64 · String · Nullable(Float64) · Decimal · Date · Array) · 트랜잭션 없음(COMMIT = no-op) · 끝에 DROP.
#[test]
fn clickhouse_script_ddl_dml_binds_and_types() {
    let Some(spec) = clickhouse_spec() else {
        return;
    };
    let opener: Opener = Box::new(|s: &ConnectSpec| clickhouse_open(s));
    let mut r = Runner::new(Dialect::Clickhouse, opener);
    let mut ev = Vec::new();
    assert!(r.connect(&spec, &mut |e| ev.push(e)), "접속 실패: {ev:?}");
    let _ = run_raw(&mut r, "DROP TABLE IF EXISTS nsqlt_ch;");
    let ev = run(
        &mut r,
        "CREATE TABLE nsqlt_ch (id UInt64, name String, score Nullable(Float64), amt Decimal(10, 2), d Date, tags Array(String)) ENGINE = Memory;\n\
         INSERT INTO nsqlt_ch VALUES (1, '한글', 1.5, 12.34, '2024-01-02', ['a','b']), (2, 'it''s', NULL, 0.01, '2024-01-03', []);\n\
         EXEC :V_ID := 2\n\
         EXEC :V_NAME := 'it''s'\n\
         SELECT id, name, score, amt, d, tags FROM nsqlt_ch WHERE id <= :V_ID ORDER BY id;\n\
         SELECT count() AS c FROM nsqlt_ch WHERE name = :V_NAME;\n\
         COMMIT;\n\
         DROP TABLE nsqlt_ch;",
    );
    let sets: Vec<&nsql_core::ResultSet> = ev
        .iter()
        .filter_map(|e| match e {
            RunEvent::ResultSet { rs, .. } => Some(rs),
            _ => None,
        })
        .collect();
    assert_eq!(sets.len(), 2, "{ev:#?}");
    let rs = sets[0];
    assert_eq!(
        rs.columns
            .iter()
            .map(|c| c.type_name.as_str())
            .collect::<Vec<_>>(),
        [
            "UInt64",
            "String",
            "Nullable(Float64)",
            "Decimal(10, 2)",
            "Date",
            "Array(String)"
        ]
    );
    assert_eq!(
        rs.rows[0],
        vec![
            Value::Int(1),
            Value::Str("한글".into()),
            Value::Float(1.5),
            Value::Decimal("12.34".into()),
            Value::Str("2024-01-02".into()),
            Value::Str("[\"a\",\"b\"]".into()),
        ]
    );
    assert_eq!(rs.rows[1][1], Value::Str("it's".into()));
    assert_eq!(rs.rows[1][2], Value::Null);
    assert_eq!(
        sets[1].rows[0][0],
        Value::Int(1),
        "따옴표 이스케이프된 바인드 리터럴"
    );
}

/// 직접 세션 경로: 영향 행 수(`X-ClickHouse-Summary`) · `USE`(클라이언트 DB) · 오류(UNKNOWN_TABLE 60 · 구문 62 + 위치) · 카탈로그 훅
/// (`system.tables`/`system.columns` · Nullable · Decimal 자릿수) · 취소 핸들(실행 밖 = Ok) · is_alive · 임시 표 DROP.
#[test]
fn clickhouse_session_rows_affected_use_errors_catalog_and_cancel() {
    let Some(spec) = clickhouse_spec() else {
        return;
    };
    let db = spec.database.clone().unwrap_or_else(|| "default".into());
    let mut s = clickhouse_open(&spec).expect("open");
    assert_eq!(s.dialect(), Dialect::Clickhouse);
    assert!(s.describe().starts_with("ClickHouse "), "{}", s.describe());
    let exec = |s: &mut Box<dyn Session>, sql: &str| {
        s.execute(&nsql_core::ExecRequest {
            sql: sql.into(),
            params: Vec::new(),
        })
    };
    let _ = exec(&mut s, "DROP TABLE IF EXISTS nsqlt_ch2");
    exec(&mut s, "CREATE TABLE nsqlt_ch2 (id UInt32, name Nullable(FixedString(8)), amt Decimal(12, 3)) ENGINE = Memory").expect("create");
    let r = exec(
        &mut s,
        "INSERT INTO nsqlt_ch2 VALUES (1, 'a', 1.5), (2, NULL, 2.25), (3, 'c', 3)",
    )
    .expect("insert");
    assert_eq!(r.rows_affected, Some(3), "{r:?}");
    // USE = 서버 왕복 없이 현재 DB 교체 → 같은 이름으로 되돌리면 표가 그대로 보인다.
    exec(&mut s, &format!("USE {db}")).expect("use");
    let r = exec(&mut s, "SELECT count() FROM nsqlt_ch2").expect("count");
    assert_eq!(r.result_sets[0].rows[0][0], Value::Int(3));
    let e = exec(&mut s, "SELECT * FROM nsqlt_nope_zz").expect_err("unknown table");
    assert_eq!(e.code, Some(60), "{e:?}");
    let e = exec(&mut s, "SELECT 1 FRM nsqlt_ch2").expect_err("syntax");
    assert_eq!(e.code, Some(62), "{e:?}");
    assert!(e.position.is_some(), "구문 오류 위치: {e:?}");
    let ts = s.catalog_tables(Some(&db)).expect("tables").expect("hook");
    let t = ts
        .iter()
        .find(|t| t.name == "nsqlt_ch2")
        .expect("nsqlt_ch2 in system.tables");
    assert_eq!((t.schema.as_str(), t.kind.as_str()), (db.as_str(), "TABLE"));
    let cs = s
        .catalog_columns(Some(&db), "nsqlt_ch2")
        .expect("columns")
        .expect("hook");
    assert_eq!(
        cs.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["id", "name", "amt"]
    );
    assert_eq!((cs[1].nullable, cs[1].size), (Some(true), Some(8)));
    assert_eq!((cs[2].size, cs[2].decimal_digits), (Some(12), Some(3)));
    assert_eq!(cs[2].position, 3);
    let c = s.cancel_handle().expect("cancel handle");
    c.cancel().expect("실행 중이 아니면 Ok");
    assert!(s.is_alive());
    assert!(
        s.commit().is_ok() && s.rollback().is_ok(),
        "트랜잭션 없음 = no-op"
    );
    exec(&mut s, "DROP TABLE nsqlt_ch2").expect("drop");
}

// ── IBM Db2(138차 · D-359 · 내장 ODBC 어댑터 위 `Dialect::Db2` · CI = `db2.yml` 별도 잡 · 로컬 = `NSQL_DB2_URL` 없으면 skip).

fn db2_spec() -> Option<ConnectSpec> {
    match std::env::var("NSQL_DB2_URL") {
        Ok(url) => Some(nsql_drivers::parse_target(&url, Dialect::Db2).expect("접속 문자열")),
        Err(_) => {
            eprintln!("[skip] NSQL_DB2_URL 없음");
            None
        }
    }
}

/// ★ 네이티브 DRDA(D-361): 내장 경로 = 크레이트 직접(`Db2Session::open`) · 확장 경로(NSQL_TEST_DYN=1) = cdylib `db2`.
fn db2_open(spec: &ConnectSpec) -> Result<Box<dyn Session>, DbError> {
    if std::env::var_os("NSQL_TEST_DYN").is_some() {
        return open_session(spec, Dialect::Db2);
    }
    Ok(Box::new(nsql_driver_db2::Db2Session::open(spec)?))
}

/// ★ 157차 서버 커서(이어 받기 · 한 세션 한 커서 · 자동 커밋 보류) + 10.5 페이징 폴백 SQL(`ROW_NUMBER()` · 12.1에서도 같은 문법)을 실서버로.
/// 3,000행 × ~110 B = 한 블록(64 KB)을 넘겨야 첫 응답이 열린 채 끝난다(작은 결과는 첫 블록에 ENDQRYRM이 같이 와 커서 없이 끝 = 종전).
#[test]
fn db2_server_cursor_and_rownum_paging() {
    let Some(spec) = db2_spec() else {
        return;
    };
    let mut s = db2_open(&spec).expect("open");
    assert!(s.cursor_supported());
    let req = |q: &str| nsql_core::ExecRequest {
        sql: q.to_string(),
        params: Vec::new(),
    };
    let _ = s.execute(&req("DROP TABLE NSQLT_DB2C"));
    s.execute(&req(
        "CREATE TABLE NSQLT_DB2C (ID INTEGER NOT NULL PRIMARY KEY, PAD VARCHAR(100))",
    ))
    .expect("create");
    s.execute(&req(
        "INSERT INTO NSQLT_DB2C WITH T(N) AS (VALUES 1 UNION ALL SELECT N + 1 FROM T WHERE N < 3000) SELECT N, REPEAT('x', 100) FROM T",
    ))
    .expect("insert 3000");
    s.set_option("max_rows", "2").expect("max_rows");
    let r = s
        .execute(&req("SELECT ID, PAD FROM NSQLT_DB2C ORDER BY ID"))
        .expect("select");
    assert_eq!(
        r.result_sets[0].rows.len(),
        2,
        "{:?}",
        r.result_sets[0].rows.len()
    );
    assert_eq!(r.result_sets[0].rows[1][0], Value::Int(2));
    let h = r.pending.expect("상한에 닿은 조회 = 커서 열림");
    let (p2, more) = s.fetch_next(h, 2).expect("fetch_next 1");
    assert_eq!((p2.rows.len(), more), (2, true));
    assert_eq!(p2.rows[0][0], Value::Int(3));
    assert_eq!(p2.columns[0].name, "ID");
    let (rest, more) = s.fetch_next(h, 0).expect("fetch_next rest");
    assert_eq!(
        (rest.rows.len(), more),
        (2996, false),
        "남은 전부 = CNTQRY 블록 여러 개"
    );
    assert_eq!(rest.rows[0][0], Value::Int(5));
    assert_eq!(rest.rows[2995][0], Value::Int(3000));
    assert!(s.fetch_next(h, 2).is_err(), "끝난 커서 = 닫힘");
    // ★ 2단계(섹션 풀 · WITH HOLD): 새 실행·커밋은 열린 커서를 **닫지 않는다** · 롤백은 닫는다 · close_cursor는 멱등.
    // (ID만은 3,000행이 ~21 KB라 첫 블록에 ENDQRYRM까지 와 커서가 안 열린다 = 작은 결과의 정상 동작 · CI 40642186831 → PAD 포함)
    let h2 = s
        .execute(&req("SELECT ID, PAD FROM NSQLT_DB2C ORDER BY ID"))
        .expect("select 2")
        .pending
        .expect("pending 2");
    let c = s
        .execute(&req("SELECT COUNT(*) FROM NSQLT_DB2C"))
        .expect("count");
    assert_eq!(c.result_sets[0].rows[0][0], Value::Int(3000));
    let (n2, more) = s.fetch_next(h2, 2).expect("새 실행 뒤에도 커서 유지");
    assert_eq!((n2.rows[0][0].clone(), more), (Value::Int(3), true));
    s.commit().expect("commit");
    let (n3, _) = s.fetch_next(h2, 2).expect("커밋 뒤에도 유지(WITH HOLD)");
    assert_eq!(n3.rows[0][0], Value::Int(5));
    s.rollback().expect("rollback");
    assert!(s.fetch_next(h2, 2).is_err(), "롤백 = 서버가 커서를 닫는다");
    s.close_cursor(h2).expect("close_cursor 멱등");
    // 동시 커서 둘(오름·내림) = 섹션이 달라 서로 간섭 없음.
    let ha = s
        .execute(&req("SELECT ID, PAD FROM NSQLT_DB2C ORDER BY ID"))
        .expect("asc")
        .pending
        .expect("pending asc");
    let hd = s
        .execute(&req("SELECT ID, PAD FROM NSQLT_DB2C ORDER BY ID DESC"))
        .expect("desc")
        .pending
        .expect("pending desc");
    let (a1, _) = s.fetch_next(ha, 1).expect("asc 1");
    let (d1, _) = s.fetch_next(hd, 1).expect("desc 1");
    assert_eq!(
        (a1.rows[0][0].clone(), d1.rows[0][0].clone()),
        (Value::Int(3), Value::Int(2998))
    );
    s.close_cursor(ha).expect("close asc");
    let (drest, more) = s.fetch_next(hd, 0).expect("desc rest");
    assert_eq!((drest.rows.len(), more), (2997, false));
    assert_eq!(drest.rows[2996][0], Value::Int(1));
    // 풀 소진: 65번째 커서가 가장 오래된 것을 밀어낸다(섹션 1..=64).
    let mut hs = Vec::new();
    for _ in 0..65 {
        hs.push(
            s.execute(&req("SELECT ID, PAD FROM NSQLT_DB2C ORDER BY ID"))
                .expect("pool")
                .pending
                .expect("pending pool"),
        );
    }
    assert!(s.fetch_next(hs[0], 1).is_err(), "가장 오래된 커서 = 반납됨");
    assert!(s.fetch_next(hs[1], 1).is_ok(), "둘째 = 아직 열림");
    assert!(s.fetch_next(hs[64], 1).is_ok(), "65째 = 열림");
    for h in hs {
        s.close_cursor(h).expect("close pool");
    }
    // 상한 없음 = 커서 없이 전부(종전).
    s.set_option("max_rows", "0").expect("max_rows 0");
    let all = s
        .execute(&req("SELECT ID, PAD FROM NSQLT_DB2C ORDER BY ID"))
        .expect("all");
    assert_eq!(all.result_sets[0].rows.len(), 3000);
    assert!(all.pending.is_none());
    // 상한 2라도 한 블록에 다 오는 작은 결과(ID만 · ~21 KB)는 커서 없이 끝난다 — 호스트의 상한 자르기는 그대로.
    s.set_option("max_rows", "2").expect("max_rows 2");
    let small = s
        .execute(&req("SELECT ID FROM NSQLT_DB2C ORDER BY ID"))
        .expect("small");
    assert!(small.pending.is_none(), "한 블록 = 커서 없음");
    s.set_option("max_rows", "0").expect("max_rows 0");
    // 10.5 폴백 페이징 SQL(nsql-io `page_sql_opts(Db2, …, false)`와 같은 꼴) = 번호 열이 끝에 · 호스트가 뗀다.
    let q = "SELECT * FROM (SELECT x.*, ROW_NUMBER() OVER () AS NSQL_RN__ FROM (\nSELECT ID FROM NSQLT_DB2C ORDER BY ID\n) x) y WHERE NSQL_RN__ > 1 AND NSQL_RN__ <= 3";
    let p = s.execute(&req(q)).expect("rownum paging");
    let rs = &p.result_sets[0];
    assert_eq!(
        rs.columns.last().map(|c| c.name.as_str()),
        Some("NSQL_RN__")
    );
    assert_eq!(
        rs.rows.iter().map(|r| r[0].clone()).collect::<Vec<_>>(),
        vec![Value::Int(2), Value::Int(3)]
    );
    s.execute(&req("DROP TABLE NSQLT_DB2C")).expect("drop");
}

/// ★ T-426 2차 ⑤ EUSRIDPWD(SECMEC 9) 실서버 검증 — db2.yml이 기존 시험 뒤 서버 인증을 `SERVER_ENCRYPT`로 바꾸고 `NSQL_DB2_EXPECT_SECMEC9=1`로
/// 이 시험만 돌린다(이름이 `db2_`로 시작하지 않아 앞 단계 필터 밖). env가 없으면 생략 · 있으면 **평문 3 폴백은 실패**로 판정(설명 꼬리로 본다).
#[test]
fn db2enc_secmec9_encrypted_login() {
    if std::env::var_os("NSQL_DB2_EXPECT_SECMEC9").is_none() {
        eprintln!("[skip] NSQL_DB2_EXPECT_SECMEC9 없음");
        return;
    }
    let Some(spec) = db2_spec() else {
        return;
    };
    let mut s = db2_open(&spec).unwrap_or_else(|e| panic!("Db2 암호화 접속 실패: {}", e.message));
    let d = s.describe();
    assert!(
        d.contains("EUSRIDPWD"),
        "SECMEC 9(EUSRIDPWD)로 붙어야 한다 — 설명 = {d}"
    );
    let r = s
        .execute(&nsql_core::ExecRequest {
            sql: "VALUES 1".into(),
            params: Vec::new(),
        })
        .expect("VALUES 1 over encrypted login");
    assert_eq!(r.result_sets[0].rows[0][0], Value::Int(1));
}

/// ★ 서버 정책 확인(사용자 10-25 소작업): `SERVER_ENCRYPT` 서버에 **평문 3만** 제안(`NSQL_DB2_AUTH=plain`)하면 거절돼야 한다 — 정보성(db2.yml은
/// continue-on-error · 받아 주면 그 사실을 기록). `NSQL_DB2_EXPECT_SECMEC9`가 없으면 생략.
#[test]
fn db2enc_plain_login_rejected_under_server_encrypt() {
    if std::env::var_os("NSQL_DB2_EXPECT_SECMEC9").is_none() {
        eprintln!("[skip] NSQL_DB2_EXPECT_SECMEC9 없음");
        return;
    }
    let Some(spec) = db2_spec() else {
        return;
    };
    std::env::set_var("NSQL_DB2_AUTH", "plain");
    let r = db2_open(&spec);
    std::env::remove_var("NSQL_DB2_AUTH");
    match r {
        Err(e) => eprintln!(
            "[ok] plain login rejected under SERVER_ENCRYPT: {}",
            e.message
        ),
        Ok(s) => panic!(
            "SERVER_ENCRYPT server accepted plain USRIDPWD (3) — describe = {}",
            s.describe()
        ),
    }
}

/// 스크립트 경로(러너 · 세션 변수): DDL(PK) · 다중 행 INSERT · `EXEC :V := 리터럴`(로컬) · `?` 바인드 · 스칼라 식 = `FROM SYSIBM.SYSDUMMY1`(`EXEC :B := :A + 10`) ·
/// 세이브포인트(`ON ROLLBACK RETAIN CURSORS`) · `FETCH FIRST` · 한글 · COMMIT · 끝에 DROP.
#[test]
fn db2_script_ddl_dml_binds_savepoint_and_types() {
    let Some(spec) = db2_spec() else {
        return;
    };
    let opener: Opener = Box::new(|s: &ConnectSpec| db2_open(s));
    let mut r = Runner::new(Dialect::Db2, opener);
    let mut ev = Vec::new();
    assert!(r.connect(&spec, &mut |e| ev.push(e)), "접속 실패: {ev:?}");
    let _ = run_raw(&mut r, "DROP TABLE NSQLT_DB2;");
    let ev = run(
        &mut r,
        "CREATE TABLE NSQLT_DB2 (ID INTEGER NOT NULL PRIMARY KEY, NAME VARCHAR(50), AMT DECIMAL(10, 2), D DATE);\n\
         INSERT INTO NSQLT_DB2 VALUES (1, '한글', 12.34, '2024-01-02'), (2, 'it''s', 0.01, '2024-01-03'), (3, NULL, NULL, NULL);\n\
         COMMIT;\n\
         EXEC :V_ID := 2\n\
         EXEC :V_SUM := :V_ID + 10\n\
         SELECT ID, NAME, AMT, D FROM NSQLT_DB2 WHERE ID <= :V_ID ORDER BY ID;\n\
         SELECT COUNT(*) AS C FROM NSQLT_DB2 WHERE ID <= :V_SUM;\n\
         SELECT ID FROM NSQLT_DB2 ORDER BY ID OFFSET 1 ROWS FETCH FIRST 1 ROWS ONLY;\n\
         DROP TABLE NSQLT_DB2;\n\
         COMMIT;",
    );
    let sets: Vec<&nsql_core::ResultSet> = ev
        .iter()
        .filter_map(|e| match e {
            RunEvent::ResultSet { rs, .. } => Some(rs),
            _ => None,
        })
        .collect();
    assert_eq!(sets.len(), 3, "{ev:#?}");
    let rs = sets[0];
    assert_eq!(rs.rows.len(), 2);
    assert_eq!(rs.rows[0][0], Value::Int(1), "{:?}", rs.rows[0]);
    assert_eq!(rs.rows[0][1], Value::Str("한글".into()));
    assert_eq!(rs.rows[0][2], Value::Decimal("12.34".into()));
    assert_eq!(rs.rows[1][1], Value::Str("it's".into()));
    assert_eq!(
        sets[1].rows[0][0],
        Value::Int(3),
        "스칼라 식(:V_ID + 10 = 12)은 SYSDUMMY1로 계산"
    );
    assert_eq!(sets[2].rows[0][0], Value::Int(2), "OFFSET/FETCH FIRST");
}

/// ★ T-426 2차 ① LOB: CLOB(한글 · 2 KB) · BLOB(5 KB · 바인드 = BLOB(X'…') 재작성) · NULL LOB · 조회 = EXTDTA 붙이기 · 끝에 DROP.
#[test]
fn db2_lob_clob_blob_round_trip() {
    use nsql_core::{BindParam, Direction, ExecRequest, VarType};
    let Some(spec) = db2_spec() else {
        return;
    };
    let mut s = db2_open(&spec).unwrap_or_else(|e| panic!("Db2 접속 실패: {}", e.message));
    let exec = |s: &mut Box<dyn Session>, sql: &str, params: Vec<BindParam>| {
        s.execute(&ExecRequest {
            sql: sql.to_string(),
            params,
        })
    };
    let _ = exec(&mut s, "DROP TABLE NSQLT_DB2L", Vec::new());
    exec(
        &mut s,
        "CREATE TABLE NSQLT_DB2L (ID INTEGER NOT NULL PRIMARY KEY, C CLOB(1M), B BLOB(1M))",
        Vec::new(),
    )
    .expect("create");
    let big_text: String = "한글 clob ".repeat(200);
    let big_bytes: Vec<u8> = (0..5000u32).map(|i| (i % 251) as u8).collect();
    let p = |name: &str, v: Value, ty: VarType| BindParam {
        name: name.into(),
        value: v,
        ty,
        direction: Direction::In,
    };
    exec(
        &mut s,
        "INSERT INTO NSQLT_DB2L (ID, C, B) VALUES (?, ?, ?)",
        vec![
            p("ID", Value::Int(1), VarType::Number),
            p("C", Value::Str(big_text.clone()), VarType::Clob),
            p("B", Value::Bytes(big_bytes.clone()), VarType::Blob),
        ],
    )
    .expect("insert lob");
    exec(&mut s, "INSERT INTO NSQLT_DB2L (ID) VALUES (2)", Vec::new()).expect("insert null lob");
    // ★ 2차 ⑥: 32 767 B 초과 = EXTDTA 입력 + DSS 연속(CLOB ≈ 110 000 B UTF-8 · BLOB 200 000 B → DDM 확장 길이).
    let huge_text: String = "큰 clob 입력 ".repeat(7_000); // 16 B × 7 000 = 112 000 B(> 32 767 → DSS 연속 4조각)
    let huge_bytes: Vec<u8> = (0..200_000u32).map(|i| (i % 253) as u8).collect();
    assert!(huge_text.len() > 100_000);
    exec(
        &mut s,
        "INSERT INTO NSQLT_DB2L (ID, C, B) VALUES (?, ?, ?)",
        vec![
            p("ID", Value::Int(3), VarType::Number),
            p("C", Value::Str(huge_text.clone()), VarType::Clob),
            p("B", Value::Bytes(huge_bytes.clone()), VarType::Blob),
        ],
    )
    .expect("insert huge lob");
    s.commit().expect("commit");
    let r = exec(
        &mut s,
        "SELECT ID, C, B FROM NSQLT_DB2L ORDER BY ID",
        Vec::new(),
    )
    .expect("select lob");
    let rs = &r.result_sets[0];
    assert_eq!(rs.rows.len(), 3, "rows={}", rs.rows.len());
    assert_eq!(rs.rows[0][0], Value::Int(1));
    assert_eq!(rs.rows[0][1], Value::Str(big_text), "CLOB");
    assert_eq!(rs.rows[0][2], Value::Bytes(big_bytes), "BLOB");
    assert_eq!(
        (&rs.rows[1][1], &rs.rows[1][2]),
        (&Value::Null, &Value::Null),
        "NULL LOB"
    );
    assert_eq!(rs.rows[2][0], Value::Int(3));
    assert!(
        rs.rows[2][1] == Value::Str(huge_text.clone()),
        "huge CLOB round trip (len {} ↔ {:?})",
        huge_text.len(),
        match &rs.rows[2][1] {
            Value::Str(t) => t.len(),
            _ => 0,
        }
    );
    assert!(
        rs.rows[2][2] == Value::Bytes(huge_bytes.clone()),
        "huge BLOB round trip (len {})",
        huge_bytes.len()
    );
    exec(&mut s, "DROP TABLE NSQLT_DB2L", Vec::new()).expect("drop");
    s.commit().expect("commit");
}

/// ★ 10-26 Db2 후속: EXPLAIN_STREAM 트리(`Runner::explain` · 노드 ≥ 2 · 대상에 표 이름 · depth > 0 노드 있음) + 막힘 질의(`DB2_BLOCKERS_SQL` ·
/// 실행 가능 = 빈 결과 또는 권한 −551 · 문법 −104/−204는 실패). EXPLAIN 표는 `SYSINSTALLOBJECTS('EXPLAIN','C',…,CURRENT USER)`로 준비(이미 있으면 무시).
#[test]
fn db2_explain_tree_and_blockers_query() {
    use nsql_script::plan::PlanOptions;
    let Some(spec) = db2_spec() else {
        return;
    };
    let opener: Opener =
        Box::new(|s: &ConnectSpec| -> Result<Box<dyn Session>, DbError> { db2_open(s) });
    let mut r = Runner::new(Dialect::Db2, opener);
    let mut ev = Vec::new();
    assert!(
        r.connect(&spec, &mut |e| ev.push(e)),
        "Db2 접속 실패: {ev:?}"
    );
    // 준비 둘은 실패해도 계속(EXPLAIN 표가 이미 있으면 SYSINSTALLOBJECTS가 오류 · 첫 회차엔 DROP 대상이 없어 −204) → `run`(오류 0 단언)이 아니라 `run_raw`.
    let _ = run_raw(
        &mut r,
        "CALL SYSPROC.SYSINSTALLOBJECTS('EXPLAIN', 'C', CAST(NULL AS VARCHAR(128)), CURRENT USER)",
    );
    let _ = run_raw(&mut r, "DROP TABLE NSQLT_DB2E");
    run(
        &mut r,
        "CREATE TABLE NSQLT_DB2E (ID INTEGER NOT NULL PRIMARY KEY, NAME VARCHAR(40))",
    );
    run(&mut r, "INSERT INTO NSQLT_DB2E VALUES (1, 'a'), (2, 'b')");
    run(&mut r, "COMMIT");
    let rep = r
        .explain(
            "SELECT e.ID, e.NAME FROM NSQLT_DB2E e WHERE e.NAME = 'a'",
            PlanOptions { analyze: false },
        )
        .unwrap_or_else(|e| panic!("[plan] db2 explain 실패: {}", e.message));
    if rep.nodes.len() < 2 {
        // ★ 진단(CI 40219428298 = 트리 질의 0행): EXPLAIN 표가 어느 스키마에 쓰였는지 · 최신 EXPLAIN_STATEMENT 값 · 직접 EXPLAIN 뒤 건수.
        let mut diag = String::new();
        let mut probe = |r: &mut Runner, sql: &str| {
            let (errs, ev) = run_raw(r, sql);
            let rows = first_rs(&ev)
                .map(|rs| {
                    rs.rows
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(Value::display)
                                .collect::<Vec<_>>()
                                .join(" | ")
                        })
                        .collect::<Vec<_>>()
                        .join(" ; ")
                })
                .unwrap_or_default();
            let err = ev
                .iter()
                .find_map(|e| match e {
                    RunEvent::Error { error, .. } => Some(error.message.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            diag.push_str(&format!("\n  {sql}\n    => errs {errs} · [{rows}] {err}"));
        };
        probe(
            &mut r,
            "VALUES (CURRENT USER, CURRENT SCHEMA, CURRENT SERVER)",
        );
        for schema in ["", "SYSTOOLS."] {
            probe(
                &mut r,
                &format!("SELECT COUNT(*) FROM {schema}EXPLAIN_INSTANCE"),
            );
            probe(&mut r, &format!("SELECT explain_requester, querytag, explain_level, explain_time, source_name FROM {schema}EXPLAIN_STATEMENT ORDER BY explain_time DESC FETCH FIRST 5 ROWS ONLY"));
            probe(
                &mut r,
                &format!("SELECT COUNT(*), MIN(operator_type) FROM {schema}EXPLAIN_OPERATOR"),
            );
        }
        probe(&mut r, "EXPLAIN PLAN SELECTION SET QUERYTAG = 'NSQLD' FOR SELECT e.ID FROM NSQLT_DB2E e WHERE e.ID = 1");
        probe(
            &mut r,
            "SELECT COUNT(*) FROM EXPLAIN_STATEMENT WHERE querytag = 'NSQLD'",
        );
        probe(
            &mut r,
            "SELECT COUNT(*) FROM SYSTOOLS.EXPLAIN_STATEMENT WHERE querytag = 'NSQLD'",
        );
        panic!("[plan] db2 트리 0행 · raw {:#?} · 진단:{diag}", rep.raw);
    }
    assert!(
        rep.nodes.iter().any(|n| n.depth > 0),
        "depth: {:#?}",
        rep.nodes
    );
    assert!(
        rep.nodes
            .iter()
            .any(|n| n.object.to_ascii_uppercase().contains("NSQLT_DB2E")),
        "object: {:#?}",
        rep.nodes
    );
    assert!(rep.nodes.iter().any(|n| n.cost.is_some()));
    // 막힘 질의 = 내 핸들로 실행(결과 0행 또는 −551).
    let handle = r
        .session
        .as_mut()
        .expect("session")
        .execute(&nsql_core::ExecRequest {
            sql: "VALUES MON_GET_APPLICATION_HANDLE()".into(),
            params: Vec::new(),
        })
        .expect("handle")
        .result_sets[0]
        .rows[0][0]
        .display();
    let sql = nsql_catalog::DB2_BLOCKERS_SQL.replace("{sid}", handle.trim());
    match r
        .session
        .as_mut()
        .expect("session")
        .execute(&nsql_core::ExecRequest {
            sql,
            params: Vec::new(),
        }) {
        Ok(res) => assert!(
            res.result_sets[0].rows.is_empty(),
            "막힌 세션이 없어야 한다"
        ),
        Err(e) => assert_eq!(
            e.code,
            Some(-551),
            "권한 밖 오류: {} {:?}",
            e.message,
            e.code
        ),
    }
    let _ = run_raw(&mut r, "DROP TABLE NSQLT_DB2E");
    run(&mut r, "COMMIT");
}

/// ★ 미비사항(10-25): DBCLOB **입력**(CLOB과 같은 0xCF · UTF-8 · 서버 변환) 왕복 + 읽기(0xCD EXTDTA = UTF-16BE 복원).
#[test]
fn db2_lob_dbclob_round_trip() {
    use nsql_core::{BindParam, Direction, ExecRequest, VarType};
    let Some(spec) = db2_spec() else {
        return;
    };
    let mut s = db2_open(&spec).unwrap_or_else(|e| panic!("Db2 접속 실패: {}", e.message));
    let exec = |s: &mut Box<dyn Session>, sql: &str, params: Vec<BindParam>| {
        s.execute(&ExecRequest {
            sql: sql.to_string(),
            params,
        })
    };
    let _ = exec(&mut s, "DROP TABLE NSQLT_DB2D", Vec::new());
    exec(
        &mut s,
        "CREATE TABLE NSQLT_DB2D (ID INTEGER NOT NULL PRIMARY KEY, D DBCLOB(1M))",
        Vec::new(),
    )
    .expect("create");
    let dtext: String = "한글 dbclob ".repeat(300);
    exec(
        &mut s,
        "INSERT INTO NSQLT_DB2D (ID, D) VALUES (?, ?)",
        vec![
            BindParam {
                name: "ID".into(),
                value: Value::Int(1),
                ty: VarType::Number,
                direction: Direction::In,
            },
            BindParam {
                name: "D".into(),
                value: Value::Str(dtext.clone()),
                ty: VarType::Clob,
                direction: Direction::In,
            },
        ],
    )
    .expect("insert dbclob");
    s.commit().expect("commit");
    let r = exec(&mut s, "SELECT ID, D FROM NSQLT_DB2D", Vec::new()).expect("select dbclob");
    let rs = &r.result_sets[0];
    assert_eq!(rs.rows.len(), 1);
    assert_eq!(rs.rows[0][1], Value::Str(dtext), "DBCLOB round trip");
    exec(&mut s, "DROP TABLE NSQLT_DB2D", Vec::new()).expect("drop");
    s.commit().expect("commit");
}

/// ★ 미비사항(10-25): XML **입력** = `?`를 `XMLPARSE(DOCUMENT CAST(? AS CLOB(2G)))`로 재작성(CLOB 경로) → XMLSERIALIZE로 읽어 조각 비교.
#[test]
fn db2_lob_xml_round_trip() {
    use nsql_core::{BindParam, Direction, ExecRequest, VarType};
    let Some(spec) = db2_spec() else {
        return;
    };
    let mut s = db2_open(&spec).unwrap_or_else(|e| panic!("Db2 접속 실패: {}", e.message));
    let exec = |s: &mut Box<dyn Session>, sql: &str, params: Vec<BindParam>| {
        s.execute(&ExecRequest {
            sql: sql.to_string(),
            params,
        })
    };
    let _ = exec(&mut s, "DROP TABLE NSQLT_DB2X", Vec::new());
    exec(
        &mut s,
        "CREATE TABLE NSQLT_DB2X (ID INTEGER NOT NULL PRIMARY KEY, X XML)",
        Vec::new(),
    )
    .expect("create");
    let xml = "<doc><item n=\"1\">한글</item><item n=\"2\">two</item></doc>";
    exec(
        &mut s,
        "INSERT INTO NSQLT_DB2X (ID, X) VALUES (?, ?)",
        vec![
            BindParam {
                name: "ID".into(),
                value: Value::Int(2),
                ty: VarType::Number,
                direction: Direction::In,
            },
            BindParam {
                name: "X".into(),
                value: Value::Str(xml.to_string()),
                ty: VarType::Clob,
                direction: Direction::In,
            },
        ],
    )
    .expect("insert xml");
    s.commit().expect("commit");
    let r = exec(
        &mut s,
        "SELECT ID, XMLSERIALIZE(X AS CLOB(1M)) FROM NSQLT_DB2X",
        Vec::new(),
    )
    .expect("select xml");
    let rs = &r.result_sets[0];
    assert_eq!(rs.rows.len(), 1);
    let got = match &rs.rows[0][1] {
        Value::Str(t) => t.clone(),
        other => panic!("XML serialized as {other:?}"),
    };
    assert!(
        got.contains("<item n=\"1\">한글</item>") && got.contains("<item n=\"2\">two</item>"),
        "XML round trip: {got}"
    );
    exec(&mut s, "DROP TABLE NSQLT_DB2X", Vec::new()).expect("drop");
    s.commit().expect("commit");
}

/// ★ T-426 2차 ③ CALL: SQL PL 프로시저(DYNAMIC RESULT SETS 2 · IN · OUT) → 결과 집합 2(열 이름 · 행) + OUT 매개변수 값 · 끝에 DROP.
#[test]
fn db2_call_result_sets_and_out_param() {
    use nsql_core::{BindParam, Direction, ExecRequest, VarType};
    let Some(spec) = db2_spec() else {
        return;
    };
    let mut s = db2_open(&spec).unwrap_or_else(|e| panic!("Db2 접속 실패: {}", e.message));
    let exec = |s: &mut Box<dyn Session>, sql: &str, params: Vec<BindParam>| {
        s.execute(&ExecRequest {
            sql: sql.to_string(),
            params,
        })
    };
    let _ = exec(&mut s, "DROP PROCEDURE NSQLP_RS", Vec::new());
    let _ = exec(&mut s, "DROP TABLE NSQLT_DB2P", Vec::new());
    exec(
        &mut s,
        "CREATE TABLE NSQLT_DB2P (ID INTEGER NOT NULL PRIMARY KEY, NAME VARCHAR(20))",
        Vec::new(),
    )
    .expect("create");
    exec(
        &mut s,
        "INSERT INTO NSQLT_DB2P VALUES (1, 'a'), (2, 'b'), (3, 'c')",
        Vec::new(),
    )
    .expect("insert");
    exec(
        &mut s,
        "CREATE PROCEDURE NSQLP_RS (IN P_MIN INTEGER, OUT P_CNT INTEGER) DYNAMIC RESULT SETS 2 LANGUAGE SQL BEGIN DECLARE C1 CURSOR WITH RETURN FOR SELECT ID, NAME FROM NSQLT_DB2P WHERE ID >= P_MIN ORDER BY ID; DECLARE C2 CURSOR WITH RETURN FOR SELECT COUNT(*) AS N FROM NSQLT_DB2P; SELECT COUNT(*) INTO P_CNT FROM NSQLT_DB2P WHERE ID >= P_MIN; OPEN C1; OPEN C2; END",
        Vec::new(),
    )
    .expect("create procedure");
    s.commit().expect("commit");
    let r = exec(
        &mut s,
        "CALL NSQLP_RS(?, ?)",
        vec![
            BindParam {
                name: "P_MIN".into(),
                value: Value::Int(2),
                ty: VarType::Number,
                direction: Direction::In,
            },
            BindParam {
                name: "P_CNT".into(),
                value: Value::Null,
                ty: VarType::Number,
                direction: Direction::Out,
            },
        ],
    )
    .expect("call");
    assert_eq!(r.result_sets.len(), 2, "{r:?}");
    let rs1 = &r.result_sets[0];
    assert_eq!(
        rs1.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["ID", "NAME"],
        "{rs1:?}"
    );
    assert_eq!(
        rs1.rows,
        vec![
            vec![Value::Int(2), Value::Str("b".into())],
            vec![Value::Int(3), Value::Str("c".into())]
        ]
    );
    assert_eq!(r.result_sets[1].rows, vec![vec![Value::Int(3)]]);
    assert_eq!(
        r.out_params,
        vec![("P_CNT".to_string(), Value::Int(2))],
        "{r:?}"
    );
    exec(&mut s, "DROP PROCEDURE NSQLP_RS", Vec::new()).expect("drop proc");
    exec(&mut s, "DROP TABLE NSQLT_DB2P", Vec::new()).expect("drop table");
    s.commit().expect("commit");
}

/// ★ T-426 2차 ② 취소 = 소켓 끊기(`drops_session`): 긴 질의(SYSCAT 교차곱) 중 다른 스레드에서 취소 → 실행은 전송 오류로 끝나고 `is_alive` = 거짓 →
/// 새로 열면 정상. 유휴 상태의 취소는 아무 일도 없다.
#[test]
fn db2_cancel_drops_session_and_reopen() {
    use nsql_core::ExecRequest;
    let Some(spec) = db2_spec() else {
        return;
    };
    let mut s = db2_open(&spec).unwrap_or_else(|e| panic!("Db2 접속 실패: {}", e.message));
    let h = s.cancel_handle().expect("취소 핸들");
    assert!(h.drops_session());
    h.cancel().expect("유휴 취소 = no-op");
    assert!(s.is_alive());
    let h2 = s.cancel_handle().expect("취소 핸들");
    let t = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(400));
        h2.cancel()
    });
    let r = s.execute(&ExecRequest {
        sql: "SELECT COUNT(*) FROM SYSCAT.COLUMNS A, SYSCAT.COLUMNS B, SYSCAT.TABLES C".into(),
        params: Vec::new(),
    });
    t.join().expect("join").expect("cancel");
    let e = r.expect_err("취소된 실행은 오류");
    assert!(
        e.message.starts_with("DRDA recv") || e.message.starts_with("DRDA send"),
        "{e:?}"
    );
    assert!(!s.is_alive(), "취소 = 세션 끊김");
    drop(s);
    let mut s2 = db2_open(&spec).unwrap_or_else(|e| panic!("재접속 실패: {}", e.message));
    let r = s2
        .execute(&ExecRequest {
            sql: "VALUES 1".into(),
            params: Vec::new(),
        })
        .expect("재접속 뒤 실행");
    assert_eq!(r.result_sets[0].rows[0][0], Value::Int(1));
}

/// 직접 세션 경로: 방언/Caps(Db2 · 대문자 접힘 · SYSDUMMY1) · 영향 행 수 · 카탈로그(현재 스키마 · 스키마 목록 · 컬럼 · PK) ·
/// `set_option("schema")` = `SET SCHEMA` · 오류 분류(SQL0204N → NoObject · SQL0104N → Syntax) · 임시 표 DROP.
#[test]
fn db2_session_catalog_schema_and_errors() {
    use nsql_core::{dberr, ErrorClass, ExecRequest};
    let Some(spec) = db2_spec() else {
        return;
    };
    let mut s = db2_open(&spec).unwrap_or_else(|e| panic!("Db2 접속 실패: {}", e.message));
    assert_eq!(s.dialect(), Dialect::Db2);
    assert_eq!(s.caps().scalar_from, Some("SYSIBM.SYSDUMMY1"));
    assert!(s.is_alive());
    let exec = |s: &mut Box<dyn Session>, sql: &str| {
        s.execute(&ExecRequest {
            sql: sql.to_string(),
            params: Vec::new(),
        })
    };
    let _ = exec(&mut s, "DROP TABLE NSQLT_DB2S");
    exec(
        &mut s,
        "CREATE TABLE NSQLT_DB2S (ID INTEGER NOT NULL PRIMARY KEY, NAME VARCHAR(20) DEFAULT 'x')",
    )
    .expect("create");
    let r = exec(&mut s, "INSERT INTO NSQLT_DB2S (ID) VALUES (1), (2)").expect("insert");
    assert_eq!(r.rows_affected, Some(2), "{r:?}");
    s.commit().expect("commit");
    let cur = nsql_catalog::current_schema(&mut *s).expect("current schema");
    assert!(!cur.is_empty());
    let schemas = nsql_catalog::schemas(&mut *s).expect("schemas");
    assert!(schemas.iter().any(|x| x == &cur), "{schemas:?}");
    let cols = nsql_catalog::columns(&mut *s, &cur, "NSQLT_DB2S").expect("columns");
    assert_eq!(
        cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        ["ID", "NAME"],
        "{cols:?}"
    );
    assert!(!cols[0].nullable && cols[1].nullable);
    assert!(cols[1].default.contains('x'), "{:?}", cols[1].default);
    assert_eq!(
        nsql_catalog::keys(&mut *s, &cur, "NSQLT_DB2S")
            .expect("keys")
            .pk,
        vec!["ID".to_string()]
    );
    // 스키마 전환 = SET SCHEMA(존재하는 스키마 SYSCAT) → 비한정 이름이 그쪽으로 풀린다 · 되돌림.
    s.set_option("schema", "SYSCAT").expect("set schema");
    assert_eq!(
        nsql_catalog::current_schema(&mut *s).expect("cur"),
        "SYSCAT"
    );
    s.set_option("schema", &cur).expect("set schema back");
    // 오류 분류(SQLCODE = ODBC 네이티브 코드).
    let e = exec(&mut s, "SELECT * FROM NSQLT_NO_SUCH_TABLE").expect_err("no table");
    let c = dberr::classify(
        Dialect::Db2,
        e.code,
        &e.message,
        "SELECT * FROM NSQLT_NO_SUCH_TABLE",
    );
    assert_eq!(c.class, ErrorClass::NoObject, "{e:?}");
    let e = exec(&mut s, "SELEC 1 FROM SYSIBM.SYSDUMMY1").expect_err("syntax");
    let c = dberr::classify(Dialect::Db2, e.code, &e.message, "SELEC 1");
    assert_eq!(c.class, ErrorClass::Syntax, "{e:?}");
    exec(&mut s, "DROP TABLE NSQLT_DB2S").expect("drop");
    s.commit().expect("commit");
}
