//! `nsql config` — 앱 설정(T-37 · GUI와 같은 `settings.conf`). 레지스트리는 `nsql-settings` 단일 원천.
//!
//! ```text
//! nsql config list                # 전체 키 · 현재 값 · (기본값) 표시
//! nsql config list perf           # 부하원 키만 도메인별(docs/39 §4-5) · 실효 값 · 출처(개별/모드/기본) · full/balanced/low
//! nsql config get <key>
//! nsql config set <key> <value>   # 검증 후 원자적 저장 — 예: ui.lang ko · ui.theme dark
//! nsql config reset <key>
//! nsql config export-json [file|-]   # 객체 계층 JSON으로(기본 settings.json 옆 파일) · import-json <file>
//! nsql config export [file|-] [portable] · import <file>   # 같은 코덱의 짧은 이름(10-24) · portable = 창 상태·PC 정보·최근 목록 제외
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

/// `nsql config list perf` 본문 — 머리글(모드 · 실효 모드 · 키 수) + 도메인별 표(키 · 실효 값 · 출처 · full/balanced/low).
/// 출처: `user` = 개별 값(모드보다 우선 · 표시 모드 custom) · `mode` = 프리셋 · `default` = 기본값과 같음.
pub(crate) fn perf_table(s: &Settings) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let rows = s.perf_rows();
    let _ = writeln!(
        out,
        "{}",
        tf(
            Msg::CfgPerfHeader,
            &[
                s.perf_mode_display().as_str(),
                s.perf_mode_resolved().as_str(),
                &rows.len().to_string()
            ]
        )
    );
    let _ = writeln!(out, "{}", t(Msg::CfgPerfColumns));
    let mut last: Option<nsql_settings::Domain> = None;
    for r in rows {
        if last != Some(r.binding.domain) {
            last = Some(r.binding.domain);
            let _ = writeln!(
                out,
                "\n[{} · {}]",
                r.binding.domain.as_str(),
                t(r.binding.domain.label())
            );
        }
        let src = match r.source {
            nsql_settings::PerfSource::Mode(m) => format!("{}:{}", t(r.source.label()), m.as_str()),
            other => t(other.label()).to_string(),
        };
        let _ = writeln!(
            out,
            "{:<26} {:<10} {:<14} {} / {} / {}",
            r.entry.key, r.value, src, r.binding.full, r.binding.balanced, r.binding.low
        );
    }
    out
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
            // 부하원 원장(docs/39 §4-5) — `perf::PERF`에 등재된 키만 도메인별로.
            if key.is_some_and(|k| k == "perf") {
                print!("{}", perf_table(&s));
                return 0;
            }
            // 비노출 설정은 `list all`에서만.
            let show_all = key.is_some_and(|k| k == "all" || k == "--all");
            let mut rows = if show_all { s.list() } else { s.list_visible() };
            // 읽기 전용 정보 키(`*.info_*`)는 GUI가 그때그때 계산하는 값이라 CLI엔 빈 칸뿐 → 목록에서 뺀다(사용자 10-23 15번).
            rows.retain(|(e, _, _)| !nsql_settings::is_info(e.key));
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
        "export-json" | "json" | "export" => {
            let s = match open() {
                Ok(s) => s,
                Err(c) => return c,
            };
            // `portable`(세 번째 인자 · 10-24) = 창 상태·이 PC 정보·최근 목록 제외(설정 창 [내보내기…]와 같은 범위).
            let scope = if o.positional.get(2).is_some_and(|w| w == "portable") {
                nsql_settings::ExportScope::Portable
            } else {
                nsql_settings::ExportScope::Full
            };
            match key {
                Some(p) if p != "-" => {
                    match std::fs::write(p, nsql_settings::to_json_scoped(&s, scope)) {
                        Ok(()) => {
                            println!("{p}");
                            0
                        }
                        Err(e) => {
                            eprintln!("{p}: {e}");
                            1
                        }
                    }
                }
                Some(_) => {
                    print!("{}", nsql_settings::to_json_scoped(&s, scope));
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
        "import-json" | "import" => {
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
                        eprintln!(
                            "{}",
                            nsql_i18n::tf(nsql_i18n::Msg::CfgImportUnknownIgnored, &[k])
                        );
                    }
                    for (k, v) in &r.invalid {
                        eprintln!(
                            "{}",
                            nsql_i18n::tf(nsql_i18n::Msg::CfgImportInvalidIgnored, &[k, v])
                        );
                    }
                    println!(
                        "{}",
                        nsql_i18n::tf(
                            nsql_i18n::Msg::CfgImportChanged,
                            &[&r.changed.len().to_string()]
                        )
                    );
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
            // 단위가 바뀐 옛 키(`ui.toast_secs`)는 옛 단위로 답한다(`get_as` · docs/94 §6-5).
            match s.get_as(key) {
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
            // ★ 포맷 프리셋(D-431 · 협업 35차 관찰): `format.preset`은 GUI와 같은 뜻 = 그 프리셋의 스타일 키를 **복사**(이름만 적지 않는다) ·
            //   `custom`은 고를 것이 없다 · 스타일 키를 바꾸면 일치 이름을 `format.preset`에 맞춘다(표시 불일치 방지).
            if key == "format.preset" {
                let Some(pairs) = nsql_settings::format_preset_pairs(value) else {
                    eprintln!("{}", tf(Msg::CliErrPreset, &[value]));
                    return 2;
                };
                // 함께 바뀌는 스타일 키 수(GUI 토스트와 같은 안내 · 협업 36차 P3).
                let changed = pairs
                    .iter()
                    .filter(|(k, v)| {
                        let def = nsql_settings::REGISTRY
                            .iter()
                            .find(|e| e.key == *k)
                            .map_or("", |e| e.default);
                        s.get(k).unwrap_or(def) != *v
                    })
                    .count();
                for (k, v) in pairs {
                    if let Err(e) = s.set(k, v) {
                        eprintln!("{e}");
                        return 2;
                    }
                }
                println!(
                    "{}",
                    tf(Msg::StFormatPresetApplied, &[value, &changed.to_string()])
                );
            }
            match s.set(key, value) {
                Ok(n) => {
                    if nsql_settings::is_format_style_key(key) {
                        let m = nsql_settings::format_preset_match(&|k| s.get(k));
                        let _ = s.set("format.preset", m);
                    }
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// `nsql config list perf`(docs/39 §4-5): 머리글 · 도메인 6묶음 · 출처 열(default → mode:low → user).
    #[test]
    fn perf_table_lists_ledger_by_domain_with_sources() {
        let dir = std::env::temp_dir().join(format!("nsql-cli-perf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Settings::open(dir.join(nsql_settings::FILE_NAME));
        let full = perf_table(&s);
        assert!(
            full.starts_with("perf.mode = full · effective = full"),
            "{full}"
        );
        // UI 도메인(§3-5)은 모드가 건드리는 키가 없어 묶음이 없다 — 원장에 있는 도메인만 머리글이 나온다.
        for (_, b) in nsql_settings::PERF {
            assert!(
                full.contains(&format!("[{} · ", b.domain.as_str())),
                "{full}"
            );
        }
        assert!(!full.contains("[ui · "), "{full}");
        assert!(
            full.contains("grid.max_rows              200        default"),
            "{full}"
        );
        s.set("perf.mode", "low").unwrap();
        let low = perf_table(&s);
        assert!(
            low.contains("grid.max_rows              100        mode:low"),
            "{low}"
        );
        assert!(
            low.contains("probe.enabled              off        mode:low"),
            "{low}"
        );
        s.set("grid.max_rows", "150").unwrap();
        let custom = perf_table(&s);
        assert!(
            custom.starts_with("perf.mode = custom · effective = low"),
            "{custom}"
        );
        assert!(
            custom.contains("grid.max_rows              150        user"),
            "{custom}"
        );
        assert_eq!(
            custom.matches('\n').count(),
            full.matches('\n').count(),
            "행 수는 원장 크기"
        );
    }
}
