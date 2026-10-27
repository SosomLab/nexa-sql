//! ★ 확장 드라이버 왕복(T-27 ②): SQLite 드라이버를 cdylib로 빌드한 산출물(`cargo build -p nsql-driver-sqlite`)을 로드해
//! 조회·바인드·DML·커밋·오류·취소 핸들·describe를 ABI 너머로 돌린다. 산출물이 없으면 건너뜀(CI는 먼저 빌드한다).
use nsql_core::{BindParam, Direction, ExecRequest, Session, Value, VarType};
use nsql_driver_dyn::{library_file_name, DynDriver};
use std::path::PathBuf;

fn sqlite_cdylib() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("NSQL_DYN_SQLITE") {
        return Some(PathBuf::from(p));
    }
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(profile)
        .join(library_file_name("nsql-driver-sqlite"));
    root.is_file().then_some(root)
}

#[test]
fn sqlite_over_abi_round_trip() {
    let Some(lib) = sqlite_cdylib() else {
        // ★ CI에서는 건너뜀 = 실패(10-27): ci.yml이 cdylib을 먼저 빌드하므로 없으면 작업 순서가 깨진 것 · 로컬은 안내만(통과 시 출력이
        //   삼켜져 로그로는 실행/건너뜀을 가를 수 없었다 = 협업 10-27 지적).
        assert!(
            std::env::var_os("CI").is_none(),
            "nsql_driver_sqlite cdylib 없음 — CI는 `cargo build -p nsql-driver-sqlite` 단계가 선행해야 한다"
        );
        eprintln!(
            "[skip] nsql_driver_sqlite cdylib 없음 — `cargo build -p nsql-driver-sqlite` 뒤 다시"
        );
        return;
    };
    let drv = DynDriver::load(&lib, "sqlite")
        .unwrap_or_else(|e| panic!("load {}: {}", lib.display(), e.message));
    assert_eq!(drv.id(), "sqlite");
    assert_eq!(drv.dialect(), nsql_core::Dialect::Sqlite);
    let spec = nsql_script::ConnectSpec::parse("sqlite::memory:").expect("spec");
    let mut s = drv
        .open(&spec)
        .unwrap_or_else(|e| panic!("open: {}", e.message));
    assert!(s.is_alive());
    assert!(s.driver_name().starts_with("ext:sqlite"));
    assert!(
        s.cancel_handle().is_some(),
        "SQLite 취소 핸들(interrupt)이 ABI 너머로"
    );
    let exec = |s: &mut nsql_driver_dyn::DynSession, sql: &str, params: Vec<BindParam>| {
        s.execute(&ExecRequest {
            sql: sql.to_string(),
            params,
        })
    };
    let r = exec(
        &mut s,
        "SELECT 1 AS n, 'a' AS t, NULL AS z, 2.5 AS f",
        Vec::new(),
    )
    .expect("select");
    let rs = &r.result_sets[0];
    assert_eq!(
        rs.columns
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["n", "t", "z", "f"]
    );
    assert_eq!(
        rs.rows[0],
        vec![
            Value::Int(1),
            Value::Str("a".into()),
            Value::Null,
            Value::Float(2.5)
        ]
    );
    exec(&mut s, "CREATE TABLE t (id INTEGER, name TEXT)", Vec::new()).expect("create");
    let r = exec(
        &mut s,
        "INSERT INTO t VALUES (?, ?)",
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
    assert_eq!(r.rows_affected, Some(1));
    s.commit().expect("commit");
    let r = exec(&mut s, "SELECT id, name FROM t", Vec::new()).expect("select back");
    assert_eq!(
        r.result_sets[0].rows[0],
        vec![Value::Int(7), Value::Str("한글".into())]
    );
    let e = exec(&mut s, "SELECT * FROM no_such_table", Vec::new()).expect_err("error");
    assert!(e.message.contains("no_such_table"), "{e:?}");
    assert!(s.is_alive(), "오류 뒤에도 세션 유지");
    s.set_option("max_rows", "1").expect("set_option");
    let r = exec(&mut s, "SELECT 1 UNION ALL SELECT 2", Vec::new()).expect("max_rows");
    assert_eq!(r.result_sets[0].rows.len(), 1);
    // ★ 10-26: 상한에서 잘린 결과의 **열린 커서 핸들**이 ABI 너머로 와야 한다(종전엔 코덱에서 빠져 확장 드라이버는 늘 None →
    //   GUI 스크롤 끝마다 "커서 없음 → 처음부터 다시 받을까요?" 토스트). 이어 받기 = fetch_next → 2 · 더 없음 → 핸들 닫힘.
    assert!(s.cursor_supported(), "SQLite 확장 = 커서 유지 드라이버");
    let h = r
        .pending
        .expect("상한에서 잘렸으면 pending 커서가 ABI 너머로 온다");
    let (rest, more) = s.fetch_next(h, 10).expect("fetch_next");
    assert_eq!(rest.rows, vec![vec![Value::Int(2)]]);
    assert!(!more, "끝까지 받았다");
    assert!(
        s.fetch_next(h, 10).is_err(),
        "끝에 닿은 커서는 닫힌다(옛 핸들 = 오류 → 호스트 폴백)"
    );
    s.set_option("max_rows", "0").expect("set_option");
    let r = exec(&mut s, "SELECT 1 UNION ALL SELECT 2", Vec::new()).expect("no limit");
    assert_eq!(r.result_sets[0].rows.len(), 2);
    assert!(r.pending.is_none(), "상한 없음 = 커서 없음");
    drop(s);
}
