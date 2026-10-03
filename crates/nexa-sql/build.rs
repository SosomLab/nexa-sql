//! build.rs — GUI `nexa-sql.exe`의 아이콘·버전 리소스(Windows). 본체 = `packaging/windows/winres.rs`(CLI와 공용 · docs/93 §4).
//! + DBMS 아이콘 표(`assets/dbms/<테마>/<크기>/<id>.png` → `OUT_DIR/dbms_icons_table.rs` · `dbms_icons.rs`가 include).
include!("../../packaging/windows/winres.rs");

use std::fmt::Write as _;

/// 표의 열 순서(`dbms_icons.rs`의 `slot`과 같아야 한다): light/64 · dark/64 · light/20 · dark/20.
const DBMS_SLOTS: [&str; 4] = ["light/64", "dark/64", "light/20", "dark/20"];

fn gen_dbms_icons() {
    let root =
        Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir")).join("assets/dbms");
    println!("cargo:rerun-if-changed={}", root.display());
    for s in DBMS_SLOTS {
        println!("cargo:rerun-if-changed={}", root.join(s).display());
    }
    // 기준 = 첫 칸(light/64)의 파일 목록 — 나머지 칸에 같은 id가 없으면 빌드 실패(빠진 그림을 조용히 넘기지 않는다).
    let mut ids: Vec<String> = std::fs::read_dir(root.join(DBMS_SLOTS[0]))
        .expect("assets/dbms/light/64")
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            (p.extension().is_some_and(|x| x == "png"))
                .then(|| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
                .flatten()
        })
        .collect();
    ids.sort();
    let mut out = String::from("pub(crate) static TABLE: &[(&str, [&[u8]; 4])] = &[\n");
    for id in &ids {
        let _ = write!(out, "    ({id:?}, [");
        for s in DBMS_SLOTS {
            let p = root.join(s).join(format!("{id}.png"));
            assert!(p.is_file(), "DBMS 아이콘 빠짐: {}", p.display());
            let _ = write!(out, "include_bytes!({:?}), ", p.display().to_string());
        }
        out.push_str("]),\n");
    }
    out.push_str("];\n");
    let dst = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("dbms_icons_table.rs");
    std::fs::write(dst, out).expect("dbms_icons_table.rs");
}

fn main() {
    embed_windows_resources("nexa-sql.rc", "nexa-sql");
    gen_dbms_icons();
}
