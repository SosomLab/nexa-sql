//! build.rs — Windows 실행 파일 아이콘·버전 리소스(T-62 · docs/33). **외부 crate 0**(winres/embed-resource 없이 직접).
//!
//! - 대상 OS가 windows일 때만 일한다. 다른 OS 산출물은 불변.
//! - `packaging/windows/nexa-sql.rc`(아이콘 = `packaging/branding/nexa-sql.ico` SSOT)를 OUT_DIR의 래퍼 .rc
//!   (`#define NSQL_VER_*` ← `CARGO_PKG_VERSION_*` → `#include` 원본)로 감싸 컴파일한다 — 버전의 단일 원천 = Cargo.toml.
//! - 컴파일러: MSVC = `rc.exe`(PATH · VS 개발자 프롬프트 `WindowsSdkDir` · Windows Kits 최신 판) 또는 `llvm-rc` /
//!   GNU = `windres`(`x86_64-w64-mingw32-windres` 등). **없으면 조용히 건너뛴다**(아이콘 없는 exe · 기능 동일 · 경고 0 —
//!   크로스 clippy(check-3os.sh)가 매번 시끄럽지 않게).
//! - 링크는 `cargo:rustc-link-arg-bins=<.res|.o>` — 이 크레이트의 bin에만.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // 런타임 env(빌드 스크립트 규약) — 단독 컴파일·테스트도 가능(env!는 컴파일 시점 고정).
    let Ok(manifest) = env::var("CARGO_MANIFEST_DIR") else {
        return;
    };
    let manifest = PathBuf::from(manifest);
    let rc = manifest.join("../../packaging/windows/nexa-sql.rc");
    let ico = manifest.join("../../packaging/branding/nexa-sql.ico");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", rc.display());
    println!("cargo:rerun-if-changed={}", ico.display());
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    println!("cargo:rerun-if-env-changed=WindowsSDKVersion");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // `cargo package`처럼 저장소 밖에서 빌드하면 packaging/이 없다 — 그때도 조용히.
    if !rc.is_file() || !ico.is_file() {
        return;
    }
    let (Ok(out_dir), Ok(target_env)) = (env::var("OUT_DIR"), env::var("CARGO_CFG_TARGET_ENV"))
    else {
        return;
    };
    let out_dir = PathBuf::from(out_dir);
    let Some(wrapper) = write_wrapper(&out_dir, &rc) else {
        return;
    };
    // 리소스 파일 안의 상대 경로("../branding/nexa-sql.ico")는 원본 .rc 폴더 기준 — cwd를 거기로.
    let Some(cwd) = rc.parent() else {
        return;
    };
    let linked = if target_env == "msvc" {
        compile_msvc(&out_dir, cwd, &wrapper)
    } else {
        compile_gnu(&out_dir, cwd, &wrapper)
    };
    if let Some(obj) = linked {
        println!("cargo:rustc-link-arg-bins={}", obj.display());
    }
}

/// OUT_DIR에 버전 define + `#include` 래퍼를 쓴다. 경로는 슬래시(rc.exe·windres 둘 다 받는다).
fn write_wrapper(out_dir: &Path, rc: &Path) -> Option<PathBuf> {
    let ver = env::var("CARGO_PKG_VERSION").ok()?;
    let mut nums = ver
        .split('-')
        .next()?
        .split('.')
        .map(|s| s.parse::<u16>().unwrap_or(0));
    let (maj, min, pat) = (
        nums.next().unwrap_or(0),
        nums.next().unwrap_or(0),
        nums.next().unwrap_or(0),
    );
    let inc = rc.canonicalize().ok()?;
    let inc = inc.to_string_lossy().replace('\\', "/");
    // Windows canonicalize()는 `\\?\` 접두를 붙인다 — rc.exe가 못 읽으니 뗀다.
    let inc = inc.strip_prefix("//?/").unwrap_or(&inc).to_string();
    let text = format!(
        "// 생성됨 — crates/nexa-sql/build.rs (버전 단일 원천 = Cargo.toml)\n\
         #define NSQL_VER_MAJOR {maj}\n#define NSQL_VER_MINOR {min}\n#define NSQL_VER_PATCH {pat}\n\
         #define NSQL_VER_STR \"{ver}\"\n#include \"{inc}\"\n"
    );
    let path = out_dir.join("nexa-sql-versioned.rc");
    fs::write(&path, text).ok()?;
    Some(path)
}

/// MSVC: rc.exe(또는 llvm-rc) → .res (link.exe·lld-link가 그대로 받는다).
fn compile_msvc(out_dir: &Path, cwd: &Path, wrapper: &Path) -> Option<PathBuf> {
    let res = out_dir.join("nexa-sql.res");
    let tool = find_rc_exe()?;
    let status = Command::new(&tool)
        .current_dir(cwd)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res)
        .arg(wrapper)
        .status()
        .ok()?;
    if status.success() && res.is_file() {
        Some(res)
    } else {
        None
    }
}

/// GNU(mingw): windres → COFF .o.
fn compile_gnu(out_dir: &Path, cwd: &Path, wrapper: &Path) -> Option<PathBuf> {
    let obj = out_dir.join("nexa-sql-res.o");
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let prefixed = format!("{arch}-w64-mingw32-windres");
    for tool in ["windres", prefixed.as_str(), "x86_64-w64-mingw32-windres"] {
        let ok = Command::new(tool)
            .current_dir(cwd)
            .args(["-O", "coff", "-o"])
            .arg(&obj)
            .arg(wrapper)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok && obj.is_file() {
            return Some(obj);
        }
    }
    None
}

/// rc.exe 찾기: PATH → `WindowsSdkDir`+`WindowsSDKVersion`(VS 개발자 프롬프트) → Windows Kits 10 최신 bin → llvm-rc(PATH).
fn find_rc_exe() -> Option<PathBuf> {
    let host_sub = match env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => "x86",
    };
    if let Some(p) = on_path("rc.exe") {
        return Some(p);
    }
    if let (Ok(dir), Ok(ver)) = (env::var("WindowsSdkDir"), env::var("WindowsSDKVersion")) {
        let p = Path::new(&dir)
            .join("bin")
            .join(ver.trim_end_matches('\\'))
            .join(host_sub)
            .join("rc.exe");
        if p.is_file() {
            return Some(p);
        }
    }
    let kits = env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".into());
    let bin = Path::new(&kits).join("Windows Kits").join("10").join("bin");
    if let Ok(rd) = fs::read_dir(&bin) {
        let mut vers: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("10."))
            })
            .collect();
        vers.sort();
        if let Some(p) = vers
            .iter()
            .rev()
            .map(|v| v.join(host_sub).join("rc.exe"))
            .find(|p| p.is_file())
        {
            return Some(p);
        }
    }
    on_path("llvm-rc.exe").or_else(|| on_path("llvm-rc"))
}

fn on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}
