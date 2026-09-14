//! `nsql config` — 앱 설정(T-37 · GUI와 같은 `settings.conf`). 레지스트리는 `nsql-settings` 단일 원천.
//!
//! ```text
//! nsql config list                # 전체 키 · 현재 값 · (기본값) 표시
//! nsql config get <key>
//! nsql config set <key> <value>   # 검증 후 원자적 저장 — 예: ui.lang ko · ui.theme dark
//! nsql config reset <key>
//! nsql config path
//! ```

use nsql_i18n::{t, tf, Msg};
use nsql_settings::{allowed, Settings};

use crate::Opts;

fn usage() -> i32 {
    eprintln!("{}", t(Msg::CfgUsage));
    2
}

fn open() -> Result<Settings, i32> {
    Settings::open_default().map_err(|e| {
        eprintln!("{}", tf(Msg::CfgOpenFailed, &[&e.to_string()]));
        1
    })
}

fn save(s: &Settings) -> i32 {
    match s.save() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{}", tf(Msg::CfgSaveFailed, &[&e.to_string()]));
            1
        }
    }
}

pub(crate) fn cmd_config(o: &Opts) -> i32 {
    let Some(sub) = o.positional.first() else {
        return usage();
    };
    let key = o.positional.get(1).map(String::as_str);
    match sub.as_str() {
        "list" | "ls" => {
            let s = match open() {
                Ok(s) => s,
                Err(c) => return c,
            };
            for (e, v, modified) in s.list() {
                let mark = if modified { "" } else { t(Msg::CfgDefaultMark) };
                println!(
                    "{:<18} {:<8} {}  [{}]  {}",
                    e.key,
                    v,
                    mark,
                    allowed(e.kind),
                    t(e.desc)
                );
            }
            0
        }
        "path" => match nsql_settings::config_dir() {
            Some(d) => {
                println!("{}", d.join(nsql_settings::FILE_NAME).display());
                0
            }
            None => {
                eprintln!("{}", t(Msg::CfgNoConfigDir));
                1
            }
        },
        "get" => {
            let Some(key) = key else { return usage() };
            let s = match open() {
                Ok(s) => s,
                Err(c) => return c,
            };
            match s.get(key) {
                Some(v) => {
                    println!("{v}");
                    0
                }
                None => {
                    eprintln!("{}", tf(Msg::CfgUnknownKey, &[key]));
                    2
                }
            }
        }
        "set" => {
            let (Some(key), Some(value)) = (key, o.positional.get(2)) else {
                return usage();
            };
            let mut s = match open() {
                Ok(s) => s,
                Err(c) => return c,
            };
            match s.set(key, value) {
                Ok(n) => {
                    let code = save(&s);
                    if code == 0 {
                        println!("{}", tf(Msg::CfgSet, &[key, &n]));
                    }
                    code
                }
                Err(e) => {
                    eprintln!("{e}");
                    2
                }
            }
        }
        "reset" => {
            let Some(key) = key else { return usage() };
            let mut s = match open() {
                Ok(s) => s,
                Err(c) => return c,
            };
            match s.reset(key) {
                Ok(d) => {
                    let code = save(&s);
                    if code == 0 {
                        println!("{}", tf(Msg::CfgReset, &[key, d]));
                    }
                    code
                }
                Err(e) => {
                    eprintln!("{e}");
                    2
                }
            }
        }
        _ => usage(),
    }
}
