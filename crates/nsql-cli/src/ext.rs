//! `nsql ext` — 확장 패키지 개발 도구(T-166 E8 · docs/75 §9-3 · 11-05). 코어 = `nsql-ext`(GUI 매니저와 같은 파서·sha256·서명 검증).
//!
//! ```text
//! nsql ext check <dir>                 # 패키지 폴더(extension.json) 또는 저장소 루트(index.json) 검사 · 문제 있으면 종료 1
//! nsql ext pack <dir> [--index <root>] # files[].sha256 재계산 · 루트 색인 행 갱신/추가(원문 서식 보존)
//! nsql ext verify <dir|url> [--pub <a.pub>]...   # 저장소 검증: 색인 서명(index.sig · --pub 있을 때) + 패키지 전부 check
//! nsql ext list [<dir|url>]            # 색인의 패키지 목록(기본 = 설정 extensions.default_repository · 없으면 공식 URL)
//! ```
//! 서명 **생성**은 여기 없다(`nexa-license-tool sign-file index.json --key <봉투>` → `index.sig` · 사용자 11-04 "서명 = 도구만").
//! 종료 코드: 0 · 1(문제 있음·실패) · 2(사용법).

use nsql_ext::check::{self, IssueCode, Report};
use nsql_ext::net::Source;
use nsql_ext::{pack, sig};
use nsql_i18n::{t, tf, Msg};
use std::path::{Path, PathBuf};

use crate::Opts;

fn usage() -> i32 {
    eprint!("{}", crate::help::text(Some("ext")));
    2
}

/// 하위 명령 뒤 인자에서 `--index <root>` · `--pub <file>`(여러 번)을 걷어낸 나머지.
struct Args {
    rest: Vec<String>,
    index: Option<String>,
    pubs: Vec<String>,
}

fn parse(positional: &[String]) -> Args {
    let mut a = Args {
        rest: Vec::new(),
        index: None,
        pubs: Vec::new(),
    };
    let mut it = positional.iter();
    while let Some(p) = it.next() {
        match p.as_str() {
            "--index" => a.index = it.next().cloned(),
            "--pub" => {
                if let Some(f) = it.next() {
                    a.pubs.push(f.clone());
                }
            }
            _ => a.rest.push(p.clone()),
        }
    }
    a
}

pub(crate) fn cmd_ext(o: &Opts) -> i32 {
    let Some(sub) = o.positional.first() else {
        return usage();
    };
    let a = parse(&o.positional[1..]);
    match sub.as_str() {
        "check" => match a.rest.first() {
            Some(d) => cmd_check(Path::new(d)),
            None => usage(),
        },
        "pack" => match a.rest.first() {
            Some(d) => cmd_pack(Path::new(d), a.index.as_deref().map(Path::new)),
            None => usage(),
        },
        "verify" => match a.rest.first() {
            Some(s) => cmd_verify(&Source::parse(s), &a.pubs),
            None => usage(),
        },
        "list" | "ls" => cmd_list(a.rest.first().map(|s| Source::parse(s))),
        _ => usage(),
    }
}

/// 문제 한 줄 → 사용자 문구.
fn issue_text(i: &check::Issue) -> String {
    let d = i.detail.as_str();
    match i.code {
        IssueCode::MetaParse => tf(Msg::ExtIssueMetaParse, &[d]),
        IssueCode::FileMissing => tf(Msg::ExtIssueFileMissing, &[d]),
        IssueCode::Sha256Mismatch => tf(Msg::ExtIssueSha256, &[d]),
        IssueCode::WasmMagic => t(Msg::ExtIssueWasmMagic).to_string(),
        IssueCode::BadId => tf(Msg::ExtIssueBadId, &[d]),
        IssueCode::BadVersion => tf(Msg::ExtIssueBadVersion, &[d]),
        IssueCode::BadPlatform => tf(Msg::ExtIssueBadPlatform, &[d]),
        IssueCode::BadMinApp => tf(Msg::ExtIssueBadMinApp, &[d]),
        IssueCode::IndexMissing => tf(Msg::ExtIssueIndexMissing, &[d]),
        IssueCode::IndexMismatch => tf(Msg::ExtIssueIndexMismatch, &[d]),
        IssueCode::NoFiles => t(Msg::ExtIssueNoFiles).to_string(),
    }
}

/// 보고서 출력 → 문제 수.
fn print_report(r: &Report) -> usize {
    let kind = r.kind.map_or("?", |k| k.as_str());
    if r.ok() {
        println!(
            "{}",
            tf(
                Msg::ExtCheckOk,
                &[&r.dir_name, &r.id, &r.version, kind, &r.files.to_string()]
            )
        );
    } else {
        println!(
            "{}",
            tf(
                Msg::ExtCheckIssues,
                &[&r.dir_name, &r.issues.len().to_string()]
            )
        );
        for i in &r.issues {
            println!("  - {}", issue_text(i));
        }
    }
    r.issues.len()
}

fn cmd_check(p: &Path) -> i32 {
    let reports = if check::is_root(p) {
        match check::check_root(p) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{}", tf(Msg::ExtIndexError, &[&e]));
                return 1;
            }
        }
    } else {
        // 패키지 폴더 — 부모에 index.json이 있으면 색인 행까지 본다.
        let parent_index = p
            .parent()
            .filter(|d| check::is_root(d))
            .and_then(|d| std::fs::read_to_string(d.join("index.json")).ok())
            .and_then(|t| nsql_ext::parse_index(&t).ok());
        vec![check::check_dir(p, parent_index.as_ref())]
    };
    let bad: usize = reports.iter().map(print_report).sum();
    println!(
        "{}",
        tf(
            Msg::ExtCheckSummary,
            &[&reports.len().to_string(), &bad.to_string()]
        )
    );
    i32::from(bad > 0)
}

fn cmd_pack(dir: &Path, index: Option<&Path>) -> i32 {
    // `--index`가 없어도 부모가 루트면 그 색인을 쓴다.
    let root: Option<PathBuf> = index.map(Path::to_path_buf).or_else(|| {
        dir.parent()
            .filter(|d| check::is_root(d))
            .map(Path::to_path_buf)
    });
    match pack::pack_dir(dir, root.as_deref()) {
        Ok(r) => {
            for (f, sha) in &r.updated {
                println!("{}", tf(Msg::ExtPackSha, &[f, sha]));
            }
            let index_note = if r.index_added {
                t(Msg::ExtPackIndexAdded).to_string()
            } else if r.index_updated {
                t(Msg::ExtPackIndexUpdated).to_string()
            } else {
                String::new()
            };
            println!(
                "{}",
                tf(
                    Msg::ExtPackDone,
                    &[&r.id, &r.version, &r.updated.len().to_string(), &index_note]
                )
            );
            // 다듬은 뒤 바로 검사(색인 포함) — 문제가 남으면 종료 1.
            let rep = check::check_dir(
                dir,
                root.as_deref()
                    .and_then(|d| std::fs::read_to_string(d.join("index.json")).ok())
                    .and_then(|t| nsql_ext::parse_index(&t).ok())
                    .as_ref(),
            );
            i32::from(print_report(&rep) > 0)
        }
        Err(e) => {
            eprintln!("{}", tf(Msg::ExtPackFailed, &[&e]));
            1
        }
    }
}

fn load_pubs(files: &[String]) -> Result<Vec<(String, [u8; 32])>, i32> {
    let mut keys = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(f) else {
            eprintln!("{}", tf(Msg::ExtPubUnreadable, &[f]));
            return Err(1);
        };
        match sig::parse_public_key_file(&text) {
            Some(k) => keys.push(k),
            None => {
                eprintln!("{}", tf(Msg::ExtPubNotKey, &[f]));
                return Err(1);
            }
        }
    }
    Ok(keys)
}

fn cmd_verify(src: &Source, pubs: &[String]) -> i32 {
    let keys = match load_pubs(pubs) {
        Ok(k) => k,
        Err(c) => return c,
    };
    let index_bytes = match src.read("index.json") {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{}", tf(Msg::ExtIndexError, &[&e]));
            return 1;
        }
    };
    let mut bad = 0usize;
    // ① 색인 서명(D-373): 공개키를 줬을 때만 판정 · 사이드카 없음 = 실패.
    if keys.is_empty() {
        println!("{}", t(Msg::ExtVerifyNoKey));
    } else {
        let sig_text = src.read_text(sig::SIG_FILE).unwrap_or_default();
        match sig::verify_index(&index_bytes, &sig_text, &keys) {
            Ok(id) => println!("{}", tf(Msg::ExtVerifySigOk, &[&id])),
            Err(e) => {
                bad += 1;
                let why = match e {
                    sig::SigError::Missing => t(Msg::ExtVerifySigMissing).to_string(),
                    sig::SigError::Malformed => t(Msg::ExtVerifySigMalformed).to_string(),
                    sig::SigError::UnknownKey(k) => tf(Msg::ExtVerifySigUnknownKey, &[&k]),
                    sig::SigError::Invalid(k) => tf(Msg::ExtVerifySigInvalid, &[&k]),
                };
                println!("{}", tf(Msg::ExtVerifySigBad, &[&why]));
            }
        }
    }
    // ② 패키지 전부(폴더 = check_root · URL = 메타·파일을 내려받아 같은 판정).
    let index_text = String::from_utf8_lossy(&index_bytes).into_owned();
    let idx = match nsql_ext::parse_index(&index_text) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{}", tf(Msg::ExtIndexError, &[&e]));
            return 1;
        }
    };
    let reports: Vec<Report> = match src {
        Source::Dir(root) => idx
            .packages
            .iter()
            .map(|p| check::check_dir(&root.join(&p.dir), Some(&idx)))
            .collect(),
        Source::Url(_) => idx
            .packages
            .iter()
            .map(|p| check_remote(src, p, &idx))
            .collect(),
    };
    bad += reports.iter().map(print_report).sum::<usize>();
    println!(
        "{}",
        tf(
            Msg::ExtCheckSummary,
            &[&reports.len().to_string(), &bad.to_string()]
        )
    );
    i32::from(bad > 0)
}

/// 원격 패키지 검사: `extension.json`과 `files[]`를 받아 폴더 검사와 같은 판정(임시 폴더에 받아 `check_dir` 재사용).
fn check_remote(src: &Source, p: &nsql_ext::Summary, idx: &nsql_ext::Index) -> Report {
    let tmp = std::env::temp_dir().join(format!("nsql-ext-verify-{}", std::process::id()));
    let dir = tmp.join(&p.dir);
    let _ = std::fs::create_dir_all(&dir);
    let fail = |detail: String| Report {
        dir_name: p.dir.clone(),
        id: p.id.clone(),
        name: p.name.clone(),
        version: p.version.clone(),
        kind: Some(p.kind),
        files: 0,
        issues: vec![check::Issue {
            code: IssueCode::MetaParse,
            detail,
        }],
    };
    let meta_text = match src.read_text(&format!("{}/extension.json", p.dir)) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    if let Err(e) = std::fs::write(dir.join("extension.json"), &meta_text) {
        return fail(e.to_string());
    }
    if let Ok(m) = nsql_ext::parse_meta(&meta_text) {
        for f in m.files.iter().filter(|f| f.url.is_empty()) {
            if let Ok(b) = src.read(&format!("{}/{}", p.dir, f.path)) {
                let _ = std::fs::write(dir.join(&f.path), b);
            }
        }
    }
    let r = check::check_dir(&dir, Some(idx));
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

fn cmd_list(src: Option<Source>) -> i32 {
    let src = src.unwrap_or_else(|| {
        let conf = crate::settings_cached()
            .ok()
            .and_then(|s| s.get("extensions.default_repository").map(str::to_string))
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| {
                "https://raw.githubusercontent.com/SosomLab/nexa-sql/main/extensions".to_string()
            });
        Source::parse(&conf)
    });
    match src
        .read_text("index.json")
        .and_then(|t| nsql_ext::parse_index(&t))
    {
        Ok(idx) => {
            println!(
                "{}",
                tf(
                    Msg::ExtListHead,
                    &[&idx.name, &idx.packages.len().to_string()]
                )
            );
            for p in &idx.packages {
                println!(
                    "{:<24} {:<10} {:<8} {}",
                    p.id,
                    p.version,
                    p.kind.as_str(),
                    p.summary
                );
            }
            0
        }
        Err(e) => {
            eprintln!("{}", tf(Msg::ExtIndexError, &[&e]));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_split_flags() {
        let a = parse(&[
            "x".into(),
            "--index".into(),
            "r".into(),
            "--pub".into(),
            "a.pub".into(),
            "--pub".into(),
            "b.pub".into(),
        ]);
        assert_eq!(a.rest, vec!["x".to_string()]);
        assert_eq!(a.index.as_deref(), Some("r"));
        assert_eq!(a.pubs, vec!["a.pub".to_string(), "b.pub".to_string()]);
    }
}
