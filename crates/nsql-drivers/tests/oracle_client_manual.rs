//! Oracle 클라이언트 **직접 지정**(설정 `oracle.client_mode = manual` · 사용자 09-21) — 실서버 + 실제 Instant Client가 있어야 돈다.
//!
//! ODPI-C는 프로세스에서 한 번만 초기화되므로 **별도 테스트 바이너리**(= 별도 프로세스)로 둔다. 환경 변수가 없으면 건너뛴다:
//! `NSQL_ORACLE_PROFILE`(볼트 프로필 — 비밀번호를 환경 변수에 적지 않는다) 또는 `NSQL_ORACLE_URL` + `NSQL_ORACLE_CLIENT_DIR_TEST`
//! (Instant Client 폴더). 서버에는 아무것도 만들지 않는다(`SELECT … FROM dual` 한 번).
#![cfg(feature = "oracle")]

use nsql_core::{Dialect, ExecRequest};

#[test]
fn manual_client_folder_is_the_only_one_used() {
    let Ok(dir) = std::env::var("NSQL_ORACLE_CLIENT_DIR_TEST") else {
        eprintln!("[skip] NSQL_ORACLE_CLIENT_DIR_TEST 없음");
        return;
    };
    let spec = match (
        std::env::var("NSQL_ORACLE_URL"),
        std::env::var("NSQL_ORACLE_PROFILE"),
    ) {
        (Ok(url), _) => nsql_drivers::parse_target(&url, Dialect::Oracle).expect("접속 문자열"),
        (Err(_), Ok(name)) => nsql_vault::Vault::open_default()
            .and_then(|v| v.get(&name))
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("프로필 없음: {name}")),
        _ => {
            eprintln!("[skip] NSQL_ORACLE_URL/NSQL_ORACLE_PROFILE 없음");
            return;
        }
    };
    // ① 라이브러리가 없는 폴더를 지정 = 접속 실패 — PATH에 다른 클라이언트가 있어도 조용히 쓰지 않는다.
    let empty = std::env::temp_dir().join(format!("nsql-ora-empty-{}", std::process::id()));
    std::fs::create_dir_all(&empty).expect("임시 폴더");
    nsql_drivers::set_oracle_client(true, &empty.display().to_string(), "");
    let info = nsql_drivers::oracle_client_info();
    assert_eq!(
        (info.source, info.library.is_some()),
        ("not_found", false),
        "{info:?}"
    );
    assert!(
        nsql_drivers::open(&spec, Dialect::Oracle).is_err(),
        "라이브러리가 없는 폴더를 지정했는데 접속됐다"
    );
    let _ = std::fs::remove_dir_all(&empty);
    // ② 진짜 폴더 = 접속된다 · 출처 = 설정 · 로드된 버전을 안다.
    nsql_drivers::set_oracle_client(true, &dir, "");
    let info = nsql_drivers::oracle_client_info();
    assert_eq!(info.source, "setting", "{info:?}");
    assert!(info.library.is_some(), "{info:?}");
    let mut s = nsql_drivers::open(&spec, Dialect::Oracle).expect("직접 지정한 클라이언트로 접속");
    let r = s
        .execute(&ExecRequest {
            sql: "SELECT 1 FROM dual".into(),
            params: Vec::new(),
        })
        .expect("조회");
    assert_eq!(r.result_sets[0].rows.len(), 1);
    let v = nsql_drivers::oracle_client_info()
        .version
        .expect("로드된 뒤에는 버전을 안다");
    assert!(v.chars().next().is_some_and(|c| c.is_ascii_digit()), "{v}");
}
