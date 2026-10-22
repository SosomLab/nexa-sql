//! `nsql driver` — 확장 드라이버 관리(DR-29 · T-28 · docs/22 §0·§3). 폴더 = `NSQL_DRIVER_DIR` > `<설정 폴더>/drivers`.
//!
//! ```text
//! nsql driver list                  # id · 버전 · 방언 · ABI · 매니페스트 유무 · 경로(id별 최신 하나)
//! nsql driver path                  # 드라이버 폴더
//! nsql driver check <id>            # 실제로 올려 함수표를 확인(ABI 버전 · 드라이버 id · 방언)
//! nsql driver rm <id>[@<ver>]       # 폴더 삭제(이 프로세스가 올린 것은 거부 · 휴지통 없음 · T-29 뒤 install과 짝)
//! ```
use crate::Opts;
use nsql_drivers::ext;

fn usage() -> i32 {
    eprint!("{}", crate::help::text(Some("driver")));
    2
}

pub(crate) fn cmd_driver(o: &Opts) -> i32 {
    let Some(sub) = o.positional.first() else {
        return usage();
    };
    let arg = o.positional.get(1).map(String::as_str);
    match sub.as_str() {
        "list" | "ls" => list(),
        "path" => match ext::driver_dir() {
            Some(d) => {
                println!("{}", d.display());
                0
            }
            None => {
                eprintln!("드라이버 폴더를 알 수 없습니다(NSQL_DRIVER_DIR 또는 설정 폴더)");
                1
            }
        },
        "check" => {
            let Some(id) = arg else {
                eprintln!("nsql driver check <id>");
                return 2;
            };
            check(id)
        }
        "rm" | "remove" => {
            let Some(spec) = arg else {
                eprintln!("nsql driver rm <id>[@<ver>]");
                return 2;
            };
            let (id, ver) = match spec.split_once('@') {
                Some((i, v)) => (i, Some(v)),
                None => (spec, None),
            };
            match ext::remove_extension(id, ver) {
                Ok(removed) if removed.is_empty() => {
                    eprintln!("없음: {spec}");
                    1
                }
                Ok(removed) => {
                    for p in removed {
                        println!("removed {}", p.display());
                    }
                    0
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        _ => usage(),
    }
}

fn list() -> i32 {
    let dir = ext::driver_dir();
    let items = ext::list_extensions();
    if items.is_empty() {
        println!(
            "(none) — {}",
            dir.map_or("드라이버 폴더 없음".to_string(), |d| d
                .display()
                .to_string())
        );
        return 0;
    }
    println!(
        "{:<14} {:<12} {:<10} {:<5} {:<9} path",
        "id", "version", "dialect", "abi", "manifest"
    );
    for e in items {
        println!(
            "{:<14} {:<12} {:<10} {:<5} {:<9} {}",
            e.id,
            e.version,
            e.dialect.as_deref().unwrap_or("-"),
            e.abi.map_or("-".to_string(), |a| a.to_string()),
            if e.manifest { "yes" } else { "no" },
            e.path.display()
        );
    }
    0
}

fn check(id: &str) -> i32 {
    match ext::check_extension(id) {
        Ok(c) => {
            println!("id       = {}", c.driver_id);
            println!("version  = {}", c.version);
            println!("dialect  = {}", c.dialect);
            println!("abi      = {}", c.abi);
            println!("path     = {}", c.path.display());
            0
        }
        Err(e) => {
            eprintln!("{}", e.message);
            1
        }
    }
}
