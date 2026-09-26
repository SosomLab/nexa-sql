//! `nsql license` — 라이선스(docs/23 §1 · §4-2 · T-33). 판정·파일 자리는 `nsql-license`(GUI와 같은 폴더 `<설정>/license/`).
//!
//! ```text
//! nsql license status                      # 상태 · 파일 · 내용(기한 · features) · 빌드일 · 기기 코드
//! nsql license request [<name> [<email>]]  # 요청 코드 한 줄(이메일 본문에 붙인다 · 기기 ID는 해시 · 원문 없음)
//! nsql license install <file>              # 검증 통과(Licensed)만 복사 · 원본 보존 · 무효면 쓰지 않음(종료 1)
//! nsql license remove                      # 사용자 폴더 파일 삭제(기기 공용 파일은 남는다)
//! nsql license path                        # 설치 자리
//! nsql license export <file>               # 설치된 라이선스를 파일로 복사(백업 · docs/25 §11-3 · T-46) · import = install
//! ```
//! 종료 코드: 0 · 1(실패·거부) · 2(사용법). 기능 게이트의 4는 각 명령 입구가 낸다(23 §4-2 · D-41 뒤).

use std::path::Path;

use nsql_license::{features_of, InstallError, LicenseState, Licensing, RequestMeta, PRODUCT};

use crate::Opts;

fn usage() -> i32 {
    eprint!("{}", crate::help::text(Some("license")));
    2
}

pub(crate) fn cmd_license(o: &Opts) -> i32 {
    let Some(sub) = o.positional.first() else {
        return usage();
    };
    match sub.as_str() {
        "status" | "st" => status(),
        "request" | "req" => request(
            o.positional.get(1).map(String::as_str),
            o.positional.get(2).map(String::as_str),
        ),
        "install" | "add" => match o.positional.get(1) {
            Some(f) => install(Path::new(f)),
            None => usage(),
        },
        "remove" | "rm" => remove(),
        "export" | "backup" => match o.positional.get(1) {
            Some(f) => export(Path::new(f)),
            None => usage(),
        },
        "import" | "restore" => match o.positional.get(1) {
            Some(f) => install(Path::new(f)),
            None => usage(),
        },
        "path" => {
            let l = Licensing::open_default();
            match l.primary_path() {
                Some(p) => {
                    println!("{}", p.display());
                    0
                }
                None => {
                    eprintln!("사용자 설정 폴더를 알 수 없습니다");
                    1
                }
            }
        }
        _ => usage(),
    }
}

fn state_label(s: &LicenseState) -> String {
    match s {
        LicenseState::Invalid(i) => format!("invalid({i:?})").to_ascii_lowercase(),
        other => other.name().to_string(),
    }
}

fn print_license(l: &nsql_license::License) {
    let feats: Vec<&str> = if l.features.contains("*") {
        vec!["*"]
    } else {
        features_of(l)
            .into_iter()
            .map(|f| f.as_str())
            .collect::<Vec<_>>()
    };
    println!("id             {}", l.id);
    println!("licensee       {}", l.licensee);
    println!("kind           {}", l.kind);
    println!("tier           {}", l.tier);
    println!("features       {}", feats.join(","));
    if !l.machines.is_empty() {
        println!("machines       {}", l.machines.len());
    }
    println!("issued         {}", l.issued);
    println!("updates_until  {}", l.updates_until);
    println!(
        "expires        {}",
        if l.expires.is_empty() {
            "-"
        } else {
            &l.expires
        }
    );
}

fn status() -> i32 {
    let l = Licensing::open_default();
    println!("state          {}", state_label(l.state()));
    println!(
        "file           {}",
        l.path()
            .map_or("-".to_string(), |p| p.display().to_string())
    );
    if let Some(lic) = l.state().license() {
        print_license(lic);
    }
    println!("build_date     {}", PRODUCT.build_date);
    println!(
        "machine        {}",
        Licensing::machine_code().unwrap_or_else(|| "-".to_string())
    );
    println!(
        "install_to     {}",
        l.primary_path()
            .map_or("-".to_string(), |p| p.display().to_string())
    );
    0
}

fn request(name: Option<&str>, email: Option<&str>) -> i32 {
    let meta = RequestMeta {
        name: name.unwrap_or("").to_string(),
        email: email.unwrap_or("").to_string(),
    };
    match Licensing::request_code(&meta) {
        Some(code) => {
            println!("{code}");
            0
        }
        None => {
            eprintln!("이 PC의 기기 ID를 얻을 수 없습니다(OS 기기 식별자 없음)");
            1
        }
    }
}

fn install(file: &Path) -> i32 {
    let mut l = Licensing::open_default();
    match l.install(file) {
        Ok(lic) => {
            println!(
                "installed      {}",
                l.path()
                    .map_or("-".to_string(), |p| p.display().to_string())
            );
            print_license(&lic);
            0
        }
        Err(InstallError::Rejected(s)) => {
            eprintln!(
                "라이선스를 설치하지 않았습니다: {} (파일은 그대로 · 기기 코드 = {})",
                state_label(s.as_ref()),
                Licensing::machine_code().unwrap_or_else(|| "-".to_string())
            );
            1
        }
        Err(InstallError::Io(e)) => {
            eprintln!("라이선스를 설치할 수 없습니다: {e}");
            1
        }
    }
}

fn remove() -> i32 {
    let mut l = Licensing::open_default();
    match l.remove() {
        Ok(true) => {
            println!("removed · state {}", state_label(l.state()));
            0
        }
        Ok(false) => {
            println!("nothing to remove · state {}", state_label(l.state()));
            0
        }
        Err(e) => {
            eprintln!("삭제할 수 없습니다: {e}");
            1
        }
    }
}

/// 백업(T-46 · 25 §11-3): 판정에 쓴 파일을 그대로 복사(서명 파일이라 복사 = 백업 · 다른 PC에서는 기기 ID로 거부).
fn export(dest: &Path) -> i32 {
    let l = Licensing::open_default();
    let Some(src) = l.path() else {
        eprintln!("설치된 라이선스가 없습니다");
        return 1;
    };
    match std::fs::copy(src, dest) {
        Ok(n) => {
            println!(
                "exported       {} ({n} bytes) · state {}",
                dest.display(),
                state_label(l.state())
            );
            0
        }
        Err(e) => {
            eprintln!("복사할 수 없습니다: {e}");
            1
        }
    }
}
