//! ★ 확장 레지스트리 왕복(T-27 ③): SQLite cdylib 산출물을 `drivers/sqlite/<ver>/`에 두 버전으로 복사 → 최신 선택 →
//! `open_ext` → 조회. 산출물이 없으면 건너뜀(`cargo build -p nsql-driver-sqlite` 뒤).
use nsql_core::{ExecRequest, Value};
use nsql_driver_dyn::library_file_name;
use std::path::PathBuf;

fn sqlite_cdylib() -> Option<PathBuf> {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(profile)
        .join(library_file_name("nsql-driver-sqlite"));
    p.is_file().then_some(p)
}

#[test]
fn registry_picks_latest_version_and_opens() {
    let Some(lib) = sqlite_cdylib() else {
        eprintln!("[skip] nsql_driver_sqlite cdylib 없음");
        return;
    };
    let root = std::env::temp_dir().join(format!("nsql-ext-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let name = library_file_name("nsql-driver-sqlite");
    for ver in ["1.2.0", "1.10.0", "0.9.9"] {
        let d = root.join("sqlite").join(ver);
        std::fs::create_dir_all(&d).expect("mkdir");
        std::fs::copy(&lib, d.join(&name)).expect("copy");
    }
    // 라이브러리 파일이 없는 버전 폴더는 무시.
    std::fs::create_dir_all(root.join("sqlite").join("9.9.9")).expect("mkdir");
    // ★ 매니페스트(T-28): protocol이 다른 판은 목록에서 빠진다(2.0.0 = ABI 99) · lib 이름을 바꾼 판은 그 이름으로 찾는다(1.5.0).
    let bad = root.join("sqlite").join("2.0.0");
    std::fs::create_dir_all(&bad).expect("mkdir");
    std::fs::copy(&lib, bad.join(&name)).expect("copy");
    std::fs::write(
        bad.join(nsql_drivers::ext::MANIFEST),
        r#"{"id":"sqlite","version":"2.0.0","dialect":"sqlite","protocol":99}"#,
    )
    .expect("write");
    let renamed = root.join("sqlite").join("1.5.0");
    std::fs::create_dir_all(&renamed).expect("mkdir");
    std::fs::copy(&lib, renamed.join("custom.bin")).expect("copy");
    std::fs::write(
        renamed.join(nsql_drivers::ext::MANIFEST),
        format!(
            r#"{{"id":"sqlite","version":"1.5.0","dialect":"sqlite","protocol":{},"lib":"custom.bin"}}"#,
            nsql_abi::ABI_VERSION
        ),
    )
    .expect("write");
    let found = nsql_drivers::ext::find_extension_in(&root, "sqlite").expect("found");
    assert_eq!(found.version, "1.10.0", "{found:?}");
    assert!(!found.manifest);
    let inspected = nsql_drivers::ext::list_extensions_in(&root);
    assert!(
        inspected
            .iter()
            .any(|e| e.version == "1.5.0" && e.manifest && e.path.ends_with("custom.bin")),
        "{inspected:?}"
    );
    assert!(
        inspected.iter().all(|e| e.version != "2.0.0"),
        "ABI 99는 제외: {inspected:?}"
    );
    assert!(nsql_drivers::ext::find_extension_in(&root, "nope").is_none());
    // 호스트 값으로 폴더를 지정(환경 변수가 있으면 그쪽이 이기므로 그때는 열기만 건너뜀).
    nsql_drivers::ext::set_driver_dir(Some(&root));
    if std::env::var_os("NSQL_DRIVER_DIR").is_none() {
        let listed = nsql_drivers::ext::list_extensions();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "sqlite");
        let spec = nsql_script::ConnectSpec::parse("sqlite::memory:").expect("spec");
        let mut s = nsql_drivers::ext::open_ext("sqlite", &spec)
            .expect("open_ext")
            .expect("installed");
        let r = s
            .execute(&ExecRequest {
                sql: "SELECT 41 + 1 AS n".into(),
                params: Vec::new(),
            })
            .expect("select");
        assert_eq!(r.result_sets[0].rows[0][0], Value::Int(42));
        assert!(s.driver_name().starts_with("ext:sqlite"));
        assert!(nsql_drivers::ext::open_ext("nope", &spec)
            .expect("no error")
            .is_none());
    }
    nsql_drivers::ext::set_driver_dir(None);
    let _ = std::fs::remove_dir_all(&root);
}

/// ★ D-350 B단계: `cargo build -p nsql-driver-<x>`로 나온 cdylib마다 함수표(ABI v2 · id · 방언)를 확인한다(없는 것은 건너뜀 ·
/// 서버 없이 로드만 · Oracle OCI는 Instant Client 없이도 로드는 된다).
#[test]
fn built_driver_cdylibs_export_abi_v2_vtables() {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(profile);
    let mut seen = 0;
    for (dialect, ids) in nsql_drivers::DRIVER_IDS {
        for id in *ids {
            if *id == "odbc" && !cfg!(windows) {
                continue;
            }
            let p = target.join(library_file_name(&nsql_drivers::ext_crate_name(id)));
            if !p.is_file() {
                eprintln!("[skip] {} 없음", p.display());
                continue;
            }
            let d = nsql_driver_dyn::DynDriver::load(&p, id).expect("load");
            assert_eq!(d.id(), *id, "{}", p.display());
            assert_eq!(d.dialect(), *dialect, "{}", p.display());
            assert_eq!(nsql_drivers::dialect_of_ext(id), Some(*dialect));
            seen += 1;
        }
    }
    eprintln!("cdylib {seen}개 확인");
    assert_eq!(nsql_drivers::ext_ids(nsql_core::Dialect::Postgres), ["pg"]);
    assert_eq!(nsql_drivers::dialect_of_ext("nope"), None);
}
