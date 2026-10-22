//! `nsql driver` — 확장 드라이버 관리(DR-29 · T-28·T-29 · docs/22 §0·§2-3·§3). 폴더 = `NSQL_DRIVER_DIR` > `<설정 폴더>/drivers`.
//!
//! ```text
//! nsql driver list                          # id · 버전 · 방언 · ABI · 매니페스트 유무 · 경로(id별 최신 하나)
//! nsql driver path                          # 드라이버 폴더
//! nsql driver check <id>                    # 실제로 올려 함수표를 확인(ABI 버전 · 드라이버 id · 방언)
//! nsql driver rm <id>[@<ver>]               # 폴더 삭제(이 프로세스가 올린 것은 거부 · 휴지통 없음)
//! nsql driver search [<text>] [--refresh]   # 색인(nexa-sql 저장소 `drivers/index.json` · 1시간 캐시) + 설치 표시
//! nsql driver install <id>[@<ver>] [--pre] [--force]   # 내려받기 → sha256 → Ed25519 → 원자적 설치
//! nsql driver install --file <zip> [--force]           # 오프라인(옆의 .sha256·.sig)
//! nsql driver update [<id>] [--check]       # 색인 latest보다 낮은 설치본을 옆에 설치(SxS · --check = 목록만)
//! ```
//! 네트워크는 search·install·update에서만(26 §8) · 저장소 = 설정 `driver.repo` · 토큰 = `NSQL_GITHUB_TOKEN` · 타임아웃 `driver.http_timeout_secs`.
use crate::Opts;
use nsql_drivers::ext::{self, install, keys::DRIVER_KEYS};
use std::io::Write;
use std::path::{Path, PathBuf};

fn usage() -> i32 {
    eprint!("{}", crate::help::text(Some("driver")));
    2
}

/// 하위 명령 뒤 인자에서 `--flag`·`--opt <값>`을 걷어낸 나머지.
struct Args {
    rest: Vec<String>,
    file: Option<String>,
    pre: bool,
    force: bool,
    refresh: bool,
    check: bool,
}

fn parse(positional: &[String]) -> Args {
    let mut a = Args {
        rest: Vec::new(),
        file: None,
        pre: false,
        force: false,
        refresh: false,
        check: false,
    };
    let mut it = positional.iter();
    while let Some(p) = it.next() {
        match p.as_str() {
            "--file" => a.file = it.next().cloned(),
            "--pre" => a.pre = true,
            "--force" => a.force = true,
            "--refresh" => a.refresh = true,
            "--check" => a.check = true,
            _ => a.rest.push(p.clone()),
        }
    }
    a
}

fn root() -> Result<PathBuf, i32> {
    ext::driver_dir().ok_or_else(|| {
        eprintln!("드라이버 폴더를 알 수 없습니다(NSQL_DRIVER_DIR 또는 설정 폴더)");
        1
    })
}

fn source() -> install::Source {
    let (repo, timeout) =
        crate::settings_cached()
            .ok()
            .map_or((install::DEFAULT_REPO.to_string(), 30), |s| {
                (
                    s.get("driver.repo")
                        .filter(|r| !r.trim().is_empty())
                        .unwrap_or(install::DEFAULT_REPO)
                        .to_string(),
                    s.int("driver.http_timeout_secs").clamp(5, 600),
                )
            });
    install::Source {
        timeout: std::time::Duration::from_secs(timeout as u64),
        ..install::Source::new(&repo)
    }
}

pub(crate) fn cmd_driver(o: &Opts) -> i32 {
    // ★ 드라이버 폴더 = `NSQL_DRIVER_DIR` > `<설정 폴더>/drivers` — `driver` 명령은 접속 경로(`apply_load_switches`)를 지나지 않으므로
    //   여기서 직접 잡는다(사용자 10-23 실기: `nsql driver search` = "드라이버 폴더를 알 수 없습니다").
    if ext::driver_dir().is_none() {
        ext::set_driver_dir(
            nsql_settings::config_dir()
                .map(|d| d.join("drivers"))
                .as_deref(),
        );
    }
    let Some(sub) = o.positional.first() else {
        return usage();
    };
    let a = parse(&o.positional[1..]);
    let arg = a.rest.first().map(String::as_str);
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
            let (id, ver) = split_spec(spec);
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
        "search" | "index" => search(arg, a.refresh),
        "install" | "add" => install_cmd(arg, &a),
        "update" | "upgrade" => update(arg, &a),
        _ => usage(),
    }
}

fn split_spec(spec: &str) -> (&str, Option<&str>) {
    match spec.split_once('@') {
        Some((i, v)) => (i, Some(v)),
        None => (spec, None),
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

fn search(text: Option<&str>, refresh: bool) -> i32 {
    let Ok(root) = root() else { return 1 };
    let src = source();
    let (idx, cached) = match install::fetch_index(&src, &root, refresh) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let installed = ext::list_extensions_in(&root);
    let plat = install::platform();
    let needle = text.map(str::to_ascii_lowercase);
    let mut n = 0;
    println!(
        "{:<14} {:<10} {:<10} {:<10} {:<8} name",
        "id", "latest", "installed", "dialect", "platform"
    );
    for d in &idx.drivers {
        if let Some(q) = &needle {
            let hay = format!("{} {} {}", d.id, d.name, d.dialect).to_ascii_lowercase();
            if !hay.contains(q.as_str()) {
                continue;
            }
        }
        n += 1;
        let inst = installed
            .iter()
            .find(|e| e.id == d.id)
            .map_or("-".to_string(), |e| e.version.clone());
        let plat_ok = d.platforms.is_empty() || d.platforms.contains(&plat);
        println!(
            "{:<14} {:<10} {:<10} {:<10} {:<8} {}",
            d.id,
            d.latest.as_deref().unwrap_or("?"),
            inst,
            d.dialect,
            if plat_ok { "yes" } else { "no" },
            d.name
        );
    }
    if n == 0 {
        println!("(no match) — {} drivers in index", idx.drivers.len());
    }
    eprintln!(
        "index: {} ({})",
        root.join(install::INDEX_FILE).display(),
        if cached { "cached" } else { "fetched" }
    );
    0
}

fn progress() -> impl FnMut(&str, u64, Option<u64>) {
    let mut last_stage = String::new();
    let mut last_pct = u64::MAX;
    move |stage: &str, n: u64, total: Option<u64>| {
        let mut err = std::io::stderr();
        if stage != last_stage {
            if !last_stage.is_empty() {
                let _ = writeln!(err);
            }
            last_stage = stage.to_string();
            last_pct = u64::MAX;
            let _ = write!(err, "{stage:<9}");
        }
        if stage == "download" {
            let shown = match total {
                Some(t) if t > 0 => (n * 100 / t).min(100),
                _ => n / (1 << 20),
            };
            if shown != last_pct {
                last_pct = shown;
                match total {
                    Some(t) if t > 0 => {
                        let _ = write!(err, "\r{stage:<9}{:>3}% {}", shown, human(t));
                    }
                    _ => {
                        let _ = write!(err, "\r{stage:<9}{}", human(n));
                    }
                }
            }
        }
        let _ = err.flush();
    }
}

fn human(b: u64) -> String {
    if b >= 1 << 20 {
        format!("{:.1} MB", b as f64 / (1u64 << 20) as f64)
    } else {
        format!("{:.0} KB", b as f64 / 1024.0)
    }
}

fn report(r: &install::InstallReport) {
    eprintln!();
    println!(
        "installed {}@{} → {} ({} files · {} · key {})",
        r.id,
        r.version,
        r.dir.display(),
        r.files,
        human(r.bytes),
        r.key_id
    );
}

fn install_cmd(arg: Option<&str>, a: &Args) -> i32 {
    let Ok(root) = root() else { return 1 };
    if let Some(f) = &a.file {
        return match install::install_file(&root, Path::new(f), a.force, DRIVER_KEYS) {
            Ok(r) => {
                report(&r);
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        };
    }
    let Some(spec) = arg else {
        eprintln!("nsql driver install <id>[@<ver>] [--pre] [--force] | --file <zip>");
        return 2;
    };
    let (id, ver) = split_spec(spec);
    let src = source();
    let mut p = progress();
    match install::install(&src, &root, id, ver, a.pre, a.force, DRIVER_KEYS, &mut p) {
        Ok(r) => {
            report(&r);
            0
        }
        Err(e) => {
            eprintln!("\n{e}");
            1
        }
    }
}

fn update(arg: Option<&str>, a: &Args) -> i32 {
    let Ok(root) = root() else { return 1 };
    let src = source();
    let (idx, _) = match install::fetch_index(&src, &root, true) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let installed: Vec<_> = ext::list_extensions_in(&root)
        .into_iter()
        .filter(|e| arg.is_none_or(|id| id == e.id))
        .collect();
    let ups = install::updates(&idx, &installed);
    if ups.is_empty() {
        println!("up to date ({} installed)", installed.len());
        return 0;
    }
    for u in &ups {
        println!("{:<14} {} → {}", u.id, u.installed, u.latest);
    }
    if a.check {
        return 0;
    }
    let mut rc = 0;
    for u in &ups {
        let mut p = progress();
        match install::install(
            &src,
            &root,
            &u.id,
            Some(&u.latest),
            false,
            a.force,
            DRIVER_KEYS,
            &mut p,
        ) {
            Ok(r) => report(&r),
            Err(e) => {
                eprintln!("\n{}: {e}", u.id);
                rc = 1;
            }
        }
    }
    rc
}
