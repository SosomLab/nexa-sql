//! `nsql conn` — 연결 프로필(docs/11 §1 · T-16b). 저장소 = `nsql-vault`(GUI와 같은 폴더).
//!
//! ```text
//! nsql conn list                                   # 이름 · 방언 · user@host:port/db · 비밀번호 유무
//! nsql conn add <name> <target> [-d dialect] [-p pw] [--no-prompt]   # 비밀번호 없으면 숨김 입력
//! nsql conn show <name>                            # 비밀번호 가린 스펙 · 파일 경로
//! nsql conn rm <name>
//! nsql conn test <name>                            # 실제 접속 후 끊는다
//! nsql conn path                                   # 저장 폴더
//! ```

use std::io::{self, IsTerminal, Write};

use nsql_core::Dialect;
use nsql_run::Runner;
use nsql_vault::Vault;

use crate::{opener, term, Opts, Printer};

fn usage() -> i32 {
    eprintln!(
        "nsql conn — 연결 프로필(사용자 폴더에 비밀번호 암호화 저장 · GUI와 공유)\n\n  nsql conn list\n  nsql conn add <name> <target> [-d dialect] [-p password] [--no-prompt]\n  nsql conn show <name>\n  nsql conn rm <name>\n  nsql conn test <name>\n  nsql conn path\n\n  이후 `-c <name>`(run/shell/export)과 스크립트의 `CONNECT <name>`이 프로필을 쓴다."
    );
    2
}

fn open_vault() -> Result<Vault, i32> {
    Vault::open_default().map_err(|e| {
        eprintln!("프로필 저장소를 열 수 없습니다: {e}");
        1
    })
}

pub(crate) fn cmd_conn(o: &Opts) -> i32 {
    let Some(sub) = o.positional.first() else {
        return usage();
    };
    let name = o.positional.get(1).map(String::as_str);
    match sub.as_str() {
        "list" | "ls" => list(),
        "path" => match Vault::default_dir() {
            Some(d) => {
                println!("{}", d.display());
                0
            }
            None => {
                eprintln!("사용자 설정 폴더를 알 수 없습니다");
                1
            }
        },
        "add" | "save" => {
            let (Some(name), Some(target)) = (name, o.positional.get(2)) else {
                eprintln!("nsql conn add <name> <target>");
                return 2;
            };
            add(o, name, target)
        }
        "show" => {
            let Some(name) = name else {
                eprintln!("nsql conn show <name>");
                return 2;
            };
            show(name)
        }
        "rm" | "remove" | "delete" => {
            let Some(name) = name else {
                eprintln!("nsql conn rm <name>");
                return 2;
            };
            rm(name)
        }
        "test" => {
            let Some(name) = name else {
                eprintln!("nsql conn test <name>");
                return 2;
            };
            test(o, name)
        }
        _ => usage(),
    }
}

fn list() -> i32 {
    let v = match open_vault() {
        Ok(v) => v,
        Err(c) => return c,
    };
    let list = match v.list() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    if list.is_empty() {
        eprintln!(
            "저장된 프로필이 없습니다 — nsql conn add <name> <target>  ({})",
            v.dir().display()
        );
        return 0;
    }
    let w = list.iter().map(|p| p.name.len()).max().unwrap_or(4).max(4);
    let out = io::stdout();
    let mut out = out.lock();
    let _ = writeln!(out, "{:<w$}  {:<7}  {:<3}  TARGET", "NAME", "DIALECT", "PW");
    for p in &list {
        let dialect = p.spec.dialect.map_or("-".to_string(), |d| d.to_string());
        let mut s = p.spec.clone();
        s.dialect = None;
        let _ = writeln!(
            out,
            "{:<w$}  {:<7}  {:<3}  {}",
            p.name,
            dialect,
            if p.has_password { "✓" } else { "-" },
            s.redacted()
        );
    }
    0
}

fn add(o: &Opts, name: &str, target: &str) -> i32 {
    if !nsql_vault::is_profile_name(name) {
        eprintln!("프로필 이름 '{name}': 영문·숫자·`_ - .`만, 64자 이내");
        return 2;
    }
    let mut spec = match nsql_drivers::parse_target(target, o.dialect) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if let Some(p) = &o.password {
        spec.password = Some(p.clone());
    }
    let needs_pw = spec.password.is_none() && spec.dialect != Some(Dialect::Sqlite);
    if needs_pw && !o.no_prompt && io::stdin().is_terminal() {
        match term::read_password(&format!("Password for {name} (빈 값 = 저장 안 함): ")) {
            Some(p) if !p.is_empty() => spec.password = Some(p),
            _ => {}
        }
    }
    let v = match open_vault() {
        Ok(v) => v,
        Err(c) => return c,
    };
    if let Err(e) = v.save(name, &spec) {
        eprintln!("저장 실패: {e}");
        return 1;
    }
    println!(
        "저장: {name} = {}{}  ({})",
        spec.redacted(),
        if spec.password.is_some() {
            ""
        } else {
            "  (비밀번호 없음 — 접속 시 방언이 요구하면 실패)"
        },
        v.dir().display()
    );
    0
}

fn show(name: &str) -> i32 {
    let v = match open_vault() {
        Ok(v) => v,
        Err(c) => return c,
    };
    match v.peek(name) {
        Ok(Some(p)) => {
            println!("{name}: {}", p.describe());
            println!(
                "  파일: {}",
                v.dir()
                    .join("profiles")
                    .join(format!("{name}.conf"))
                    .display()
            );
            0
        }
        Ok(None) => {
            eprintln!("프로필 '{name}'이(가) 없습니다");
            1
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn rm(name: &str) -> i32 {
    let v = match open_vault() {
        Ok(v) => v,
        Err(c) => return c,
    };
    match v.remove(name) {
        Ok(true) => {
            println!("삭제: {name}");
            0
        }
        Ok(false) => {
            eprintln!("프로필 '{name}'이(가) 없습니다");
            1
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn test(o: &Opts, name: &str) -> i32 {
    let v = match open_vault() {
        Ok(v) => v,
        Err(c) => return c,
    };
    let spec = match v.get(name) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("프로필 '{name}'이(가) 없습니다");
            return 1;
        }
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let mut printer = Printer {
        format: nsql_io::Format::Grid,
        dialect: o.dialect,
        errors: 0,
        feedback: true,
    };
    let started = std::time::Instant::now();
    let mut runner = Runner::new(o.dialect, opener(o.dialect));
    if !runner.connect(&spec, &mut |e| printer.handle(e)) {
        return 1;
    }
    if let Some(s) = runner.session.as_mut() {
        let _ = s.commit();
    }
    println!("OK ({:.3}s)", started.elapsed().as_secs_f64());
    0
}
