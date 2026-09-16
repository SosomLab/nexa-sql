//! `nsql config` — 앱 설정(T-37 · GUI와 같은 `settings.conf`). 레지스트리는 `nsql-settings` 단일 원천.
//!
//! ```text
//! nsql config list                # 전체 키 · 현재 값 · (기본값) 표시
//! nsql config get <key>
//! nsql config set <key> <value>   # 검증 후 원자적 저장 — 예: ui.lang ko · ui.theme dark
//! nsql config reset <key>
//! nsql config export-json [file|-]   # 객체 계층 JSON으로(기본 settings.json 옆 파일) · import-json <file>
//! nsql config path
//! ```

use nsql_i18n::{t, tf, Msg};
use nsql_settings::{allowed, Settings};

use crate::Opts;

fn usage() -> i32 {
    eprint!("{}", crate::help::text(Some("config")));
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
            // 비노출 설정은 `list all`에서만.
            let show_all = key.is_some_and(|k| k == "all" || k == "--all");
            let mut rows = if show_all { s.list() } else { s.list_visible() };
            // DBeaver식 트리 순서(그룹 ▸ 카테고리)로 묶어 보여 준다.
            rows.sort_by_key(|(e, _, _)| nsql_settings::tree_order(e.cat));
            let mut last_cat: Option<Msg> = None;
            for (e, v, modified) in rows {
                if last_cat != Some(e.cat) {
                    last_cat = Some(e.cat);
                    match nsql_settings::group_of(e.cat) {
                        Some(g) => println!("\n[{} ▸ {}]", t(g), t(e.cat)),
                        None => println!("\n[{}]", t(e.cat)),
                    }
                }
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
        "export-json" | "json" => {
            let s = match open() {
                Ok(s) => s,
                Err(c) => return c,
            };
            match key {
                Some(p) if p != "-" => match std::fs::write(p, nsql_settings::to_json(&s)) {
                    Ok(()) => {
                        println!("{p}");
                        0
                    }
                    Err(e) => {
                        eprintln!("{p}: {e}");
                        1
                    }
                },
                Some(_) => {
                    print!("{}", nsql_settings::to_json(&s));
                    0
                }
                None => match s.export_json() {
                    Ok(p) => {
                        println!("{}", p.display());
                        0
                    }
                    Err(e) => {
                        eprintln!("{e}");
                        1
                    }
                },
            }
        }
        "import-json" => {
            let Some(p) = key else {
                eprintln!("nsql config import-json <file>");
                return 2;
            };
            let text = match std::fs::read_to_string(p) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("{p}: {e}");
                    return 1;
                }
            };
            let mut s = match open() {
                Ok(s) => s,
                Err(c) => return c,
            };
            match s.import_json(&text) {
                Ok(r) => {
                    if let Err(e) = s.save() {
                        eprintln!("{e}");
                        return 1;
                    }
                    for k in &r.changed {
                        println!("{k} = {}", s.get(k).unwrap_or(""));
                    }
                    for k in &r.unknown {
                        eprintln!("unknown key ignored: {k}");
                    }
                    for (k, v) in &r.invalid {
                        eprintln!("invalid value ignored: {k} = {v}");
                    }
                    println!("{} changed", r.changed.len());
                    0
                }
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
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
