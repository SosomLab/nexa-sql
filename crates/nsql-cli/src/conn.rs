//! `nsql conn` — 연결 프로필(docs/11 §1 · T-16b). 저장소 = `nsql-vault`(GUI와 같은 폴더).
//!
//! ```text
//! nsql conn list                                   # 이름 · 방언 · user@host:port/db · 비밀번호 유무
//! nsql conn add <name> <target> [-d dialect] [-p pw] [--no-prompt]   # 비밀번호 없으면 숨김 입력
//! nsql conn add <name> -d oracle --host h --port 1521 --db svc --user u [-p pw]   # 필드로(GUI 폼과 같은 경로)
//! nsql conn test [<name>] [-d … --host … --db … --user … -p …]       # 프로필 또는 필드로 접속해 보고 끊는다 · 경과 시간
//! nsql conn show <name>                            # 비밀번호 가린 스펙 · 파일 경로
//! nsql conn rm <name>
//! nsql conn test <name>                            # 실제 접속 후 끊는다
//! nsql conn path                                   # 저장 폴더
//! ```

use std::io::{self, IsTerminal, Write};

use nsql_core::Dialect;
use nsql_vault::Vault;

use crate::{opener, term, Opts};
use nsql_script::ConnectSpec;

fn usage() -> i32 {
    eprint!("{}", crate::help::text(Some("conn")));
    2
}

fn open_vault() -> Result<Vault, i32> {
    Vault::open_default().map_err(|e| {
        eprintln!(
            "{}",
            nsql_i18n::tf(nsql_i18n::Msg::CliConnVaultOpenFailed, &[&e.to_string()])
        );
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
                eprintln!("{}", nsql_i18n::t(nsql_i18n::Msg::CfgNoConfigDir));
                1
            }
        },
        "add" | "save" => {
            let Some(name) = name else {
                eprintln!("nsql conn add <name> [<target>] [--host …]");
                return 2;
            };
            add(o, name, o.positional.get(2).map(String::as_str))
        }
        "show" => {
            let Some(name) = name else {
                eprintln!("nsql conn show <name>");
                return 2;
            };
            if !nsql_vault::is_profile_name(name) {
                eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliConnBadName, &[name]));
                return 2;
            }
            show(name)
        }
        "rm" | "remove" | "delete" => {
            let Some(name) = name else {
                eprintln!("nsql conn rm <name>");
                return 2;
            };
            if !nsql_vault::is_profile_name(name) {
                eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliConnBadName, &[name]));
                return 2;
            }
            rm(name)
        }
        "test" => {
            if let Some(n) = name {
                if !nsql_vault::is_profile_name(n) {
                    eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliConnBadName, &[n]));
                    return 2;
                }
            }
            test(o, name)
        }
        // ★ 저장 프로필의 서버 유형(10-01 · 영속): `nsql conn env <name>` = 보기 · `… <name> <유형>` = 지정
        //   (정식 · 3자리 약어 · 별칭 모두 받음 · 표시 = 정식 + 3자리 약어).
        "env" | "type" => {
            let Some(name) = name else {
                eprintln!("nsql conn env <name> [{}]", nsql_script::ENV_CHOICES);
                return 2;
            };
            if !nsql_vault::is_profile_name(name) {
                eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliConnBadName, &[name]));
                return 2;
            }
            env(name, o.positional.get(2).map(String::as_str))
        }
        _ => usage(),
    }
}

fn env(name: &str, value: Option<&str>) -> i32 {
    let v = match open_vault() {
        Ok(v) => v,
        Err(c) => return c,
    };
    let Ok(Some(mut spec)) = v.get(name) else {
        eprintln!(
            "{}",
            nsql_i18n::tf(nsql_i18n::Msg::StArgProfileMissing, &[name])
        );
        return 1;
    };
    let Some(value) = value else {
        println!("{name}: {}", env_label(spec.env));
        return 0;
    };
    let env = match nsql_script::ConnEnv::parse(value) {
        Ok(e) => e,
        Err(_) => {
            eprintln!(
                "{}",
                nsql_i18n::tf(
                    nsql_i18n::Msg::CliConnUnknownEnv,
                    &[value, nsql_script::ENV_CHOICES]
                )
            );
            return 2;
        }
    };
    spec.env = env;
    match v.save(name, &spec) {
        Ok(()) => {
            println!("{name}: {}", env_label(env));
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// 서버 유형 표시 = 정식 용어 + 3자리 약어(사용자 10-01) — 예 `Production (PRD)`.
fn env_label(e: Option<nsql_script::ConnEnv>) -> String {
    format!(
        "{} ({})",
        nsql_script::ConnEnv::name_of(e),
        nsql_script::ConnEnv::abbr_of(e)
    )
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
            "{}",
            nsql_i18n::tf(
                nsql_i18n::Msg::CliConnNone,
                &[&v.dir().display().to_string()]
            )
        );
        return 0;
    }
    let w = list.iter().map(|p| p.name.len()).max().unwrap_or(4).max(4);
    let out = io::stdout();
    let mut out = out.lock();
    let _ = writeln!(
        out,
        "{:<w$}  {:<7}  {:<4}  {:<3}  TARGET",
        "NAME", "DIALECT", "TYPE", "PW"
    );
    for p in &list {
        let dialect = p.spec.dialect.map_or("-".to_string(), |d| d.to_string());
        let mut s = p.spec.clone();
        // 유형 열 = 3자리 약어(없음 = `-` · 목록에서 눈에 띄는 것은 지정된 유형뿐).
        s.env = None;
        // ★ 방언을 지운 채 그리지 않고(ODBC가 `xDriver=…`처럼 사용자와 본문이 붙던 결함 · 10-24 맥) 방언 포함 표시에서 접두만 뗀다.
        let full = s.redacted();
        let target = full
            .strip_prefix(&format!("{dialect}://"))
            .or_else(|| full.strip_prefix(&format!("{dialect}:")))
            .unwrap_or(&full);
        let _ = writeln!(
            out,
            "{:<w$}  {:<7}  {:<4}  {:<3}  {}",
            p.name,
            dialect,
            p.spec.env.map_or("-", nsql_script::ConnEnv::abbr),
            if p.has_password { "✓" } else { "-" },
            target
        );
    }
    0
}

/// 필드 옵션(`--host` 등)이 하나라도 있으면 `ConnectSpec::from_parts`(GUI 폼과 같은 검증), 아니면 접속 문자열.
fn spec_from_opts(o: &Opts, target: Option<&str>) -> Result<ConnectSpec, String> {
    let has_parts =
        o.host.is_some() || o.database.is_some() || o.user.is_some() || o.port.is_some();
    match (target, has_parts) {
        (Some(t), _) => nsql_drivers::parse_target(t, o.dialect),
        (None, true) => ConnectSpec::from_parts(
            o.dialect,
            o.host.as_deref().unwrap_or(""),
            o.port,
            o.database.as_deref().unwrap_or(""),
            o.user.as_deref().unwrap_or(""),
            o.password.as_deref().unwrap_or(""),
        ),
        (None, false) => Err(nsql_i18n::t(nsql_i18n::Msg::CliConnNeedTarget).to_string()),
    }
}

fn add(o: &Opts, name: &str, target: Option<&str>) -> i32 {
    if !nsql_vault::is_profile_name(name) {
        eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliConnBadName, &[name]));
        return 2;
    }
    let mut spec = match spec_from_opts(o, target) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if let Some(p) = &o.password {
        spec.password = Some(p.clone());
    }
    let mut needs_pw = spec.password.is_none() && spec.dialect != Some(Dialect::Sqlite);
    // `--password-stdin` · `NSQL_PASSWORD`가 프롬프트보다 먼저다(접속과 같은 순서 · T-132).
    if needs_pw {
        if let Some(p) = term::supplied_password() {
            spec.password = Some(p);
            needs_pw = false;
        }
    }
    if needs_pw && !o.no_prompt && io::stdin().is_terminal() {
        match term::read_password(&nsql_i18n::tf(nsql_i18n::Msg::CliPwPromptSave, &[name])) {
            Some(p) if !p.is_empty() => spec.password = Some(p),
            _ => {}
        }
    }
    let v = match open_vault() {
        Ok(v) => v,
        Err(c) => return c,
    };
    if let Err(e) = v.save(name, &spec) {
        eprintln!(
            "{}",
            nsql_i18n::tf(nsql_i18n::Msg::WkProfileSaveFailed, &[&e.to_string()])
        );
        return 1;
    }
    println!(
        "{}",
        nsql_i18n::tf(
            nsql_i18n::Msg::CliConnSaved,
            &[
                name,
                &spec.redacted(),
                &if spec.password.is_some() {
                    String::new()
                } else {
                    nsql_i18n::t(nsql_i18n::Msg::CliConnNoPassword).to_string()
                },
                &v.dir().display().to_string(),
            ],
        )
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
                "  {}: {}",
                nsql_i18n::t(nsql_i18n::Msg::OutConnEnv),
                env_label(p.spec.env)
            );
            if let Ok(p) = v.path_of(name) {
                println!(
                    "  {}: {}",
                    nsql_i18n::t(nsql_i18n::Msg::LicLblFile),
                    p.display()
                );
            }
            0
        }
        Ok(None) => {
            eprintln!(
                "{}",
                nsql_i18n::tf(nsql_i18n::Msg::StArgProfileMissing, &[name])
            );
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
            println!(
                "{}",
                nsql_i18n::tf(nsql_i18n::Msg::StProfileDeleted, &[name])
            );
            0
        }
        Ok(false) => {
            eprintln!(
                "{}",
                nsql_i18n::tf(nsql_i18n::Msg::StArgProfileMissing, &[name])
            );
            1
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn test(o: &Opts, name: Option<&str>) -> i32 {
    // 프로필 이름이면 저장소에서(비밀번호 포함), 아니면 필드(--host …)로 — GUI "Test Connection"과 같은 함수.
    let mut spec = match name {
        Some(name) => {
            let v = match open_vault() {
                Ok(v) => v,
                Err(c) => return c,
            };
            match v.get(name) {
                Ok(Some(s)) => s,
                Ok(None) => {
                    eprintln!(
                        "{}",
                        nsql_i18n::tf(nsql_i18n::Msg::StArgProfileMissing, &[name])
                    );
                    return 1;
                }
                Err(e) => {
                    eprintln!("{e}");
                    return 1;
                }
            }
        }
        None => match spec_from_opts(o, None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliConnTestUsage, &[&e]));
                return 2;
            }
        },
    };
    // 비밀번호가 없으면 env/프롬프트로(빈 비밀번호로 서버에 가서 ORA-01005를 받던 것 · 사용자 09-16).
    let label = name.map_or_else(|| spec.redacted(), str::to_string);
    term::ensure_password(&mut spec, o.no_prompt, &label);
    // 드라이버 설정·자동 설치 스위치(run과 같은 길 · 🔧 10-23).
    crate::apply_driver_switches();
    let mut op = opener(o.dialect);
    match nsql_run::test_connection(&spec, &mut op) {
        Ok(r) => {
            println!(
                "OK  {}  ({} · {:.3}s)  {}",
                r.description,
                r.dialect,
                r.elapsed.as_secs_f64(),
                r.session
            );
            0
        }
        Err(e) => {
            // 드라이버 없음/자동 설치 실패 = 짧은 Msg(GUI와 같은 문구) · 그 밖 = 접속 실패 + 서버 메시지.
            let dn = spec
                .dialect
                .or(Some(o.dialect))
                .map(|d| d.display_name())
                .unwrap_or("");
            let text = match e.code {
                Some(nsql_drivers::ERR_DRIVER_MISSING) => {
                    nsql_i18n::tf(nsql_i18n::Msg::StDriverMissing, &[dn])
                }
                Some(nsql_drivers::ERR_DRIVER_INSTALL_FAILED) => format!(
                    "{} ({})",
                    nsql_i18n::tf(nsql_i18n::Msg::StDriverInstallFailed, &[dn]),
                    e.message
                ),
                _ => nsql_i18n::tf(nsql_i18n::Msg::StConnectFailed, &[&e.message]),
            };
            eprintln!("{text}");
            // CLI엔 로그 창이 없다 → 드라이버 오류의 영어 상세를 한 줄 더(협업 10-23 지적).
            if matches!(
                e.code,
                Some(nsql_drivers::ERR_DRIVER_MISSING)
                    | Some(nsql_drivers::ERR_DRIVER_INSTALL_FAILED)
            ) {
                eprintln!("  {}", e.message);
            }
            1
        }
    }
}
