//! ★ 런타임 libduckdb 위치(D-350 · 64 §5): cdylib가 같은 폴더의 `libduckdb.so`/`.dylib`를 찾도록 rpath(`$ORIGIN` · `@loader_path`) ·
//! Windows는 로더(`nsql-driver-dyn`)가 `LOAD_WITH_ALTERED_SEARCH_PATH`로 cdylib 폴더를 먼저 본다.
//! 개발·시험 편의: `download-lib`가 받은 공유 라이브러리를 `target/<profile>/`(+`deps/`)로 복사해 시험 바이너리가 찾게 한다.
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-rpath,$ORIGIN");
    } else if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-arg-cdylib=-Wl,-rpath,@loader_path");
    }
    // OUT_DIR = target/<profile>/build/nsql-driver-duckdb-<hash>/out(cargo ≤ 1.99) 또는 …/build/nsql-driver-duckdb/<hash>/out
    // (cargo 1.100부터 · 11-13 CI "libduckdb.so: cannot open shared object file") → 단계 수를 세지 않고 `build` 폴더의 부모를 찾는다.
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    let Some(profile_dir) = out
        .ancestors()
        .find(|a| a.file_name().is_some_and(|f| f == "build"))
        .and_then(Path::parent)
        .map(Path::to_path_buf)
    else {
        return;
    };
    let names: &[&str] = if cfg!(windows) {
        &["duckdb.dll"]
    } else if cfg!(target_os = "macos") {
        &["libduckdb.dylib"]
    } else {
        &["libduckdb.so"]
    };
    let Ok(build_dir) = std::fs::read_dir(profile_dir.join("build")) else {
        return;
    };
    for entry in build_dir.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // 옛 배치 = `libduckdb-sys-<hash>/out/` · 새 배치 = `libduckdb-sys/<hash>/out/`.
        if name != "libduckdb-sys" && !name.starts_with("libduckdb-sys-") {
            continue;
        }
        for n in names {
            // download-lib는 out/ 아래(하위 폴더 포함)에 푼다.
            for candidate in find_in(&entry.path(), n, 5) {
                if candidate.is_file() {
                    for dest_dir in [profile_dir.clone(), profile_dir.join("deps")] {
                        let dest = dest_dir.join(n);
                        if !dest.exists() {
                            let _ = std::fs::copy(&candidate, &dest);
                        }
                    }
                    println!(
                        "cargo:warning=libduckdb runtime copied: {}",
                        candidate.display()
                    );
                    return;
                }
            }
        }
    }
}

fn find_in(dir: &Path, name: &str, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if depth == 0 {
        return out;
    }
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(find_in(&p, name, depth - 1));
            } else if p.file_name().is_some_and(|f| f == name) {
                out.push(p);
            }
        }
    }
    out
}
