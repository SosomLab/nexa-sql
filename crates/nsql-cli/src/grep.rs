//! `nsql grep <패턴> [경로…] [-i] [-w] [-e] [--no-ignore] [-j N] [--max-file-kb N]` — 파일 검색(`nsql-search` · GUI 파일 검색 패널과 같은 엔진 · T-81b).
//!
//! 출력 = `path:line:col: text`(색 없음 · 스트리밍) · 종료 코드 0 = 일치 있음 · 1 = 없음 · 2 = 인자/정규식 오류.
//! 읽기 실패는 stderr에 `path: 메시지`로 찍고 계속한다.

use nsql_i18n::{tf, Msg};
use nsql_search::{Batch, Search, SearchOpts};
use std::io::Write;
use std::path::PathBuf;

/// `nsql grep`의 인자(전역 파서가 모르는 것은 `positional`로 넘어온다).
struct GrepArgs {
    opts: SearchOpts,
}

fn parse(positional: &[String]) -> Result<GrepArgs, String> {
    // 기본 = 대소문자 구분(grep 규약) · `-i`만 무시.
    let mut opts = SearchOpts {
        case: true,
        ..SearchOpts::default()
    };
    let mut pattern: Option<String> = None;
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut it = positional.iter();
    let mut only_paths = false;
    while let Some(a) = it.next() {
        if only_paths || !a.starts_with('-') || a == "-" {
            if pattern.is_none() {
                pattern = Some(a.clone());
            } else {
                roots.push(PathBuf::from(a));
            }
            continue;
        }
        match a.as_str() {
            "--" => only_paths = true,
            "-i" | "--ignore-case" => opts.case = false,
            "-w" | "--word" => opts.word = true,
            "-e" | "--regex" => opts.regex = true,
            "--no-ignore" => opts.gitignore = false,
            "-j" | "--threads" => opts.threads = number(a, it.next())?,
            "--max-file-kb" => opts.max_file_kb = number(a, it.next())?,
            other => return Err(tf(Msg::CliGrepUnknownOption, &[other])),
        }
    }
    let Some(query) = pattern else {
        return Err(tf(Msg::CliGrepPatternRequired, &[]));
    };
    if roots.is_empty() {
        roots.push(PathBuf::from("."));
    }
    opts.query = query;
    opts.roots = roots;
    Ok(GrepArgs { opts })
}

fn number(flag: &str, v: Option<&String>) -> Result<usize, String> {
    v.and_then(|s| s.parse().ok())
        .ok_or_else(|| tf(Msg::CliGrepBadNumber, &[flag]))
}

/// 표시용 경로 — `./` 접두는 뗀다(ripgrep 규약).
fn display(p: &std::path::Path) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix("./")
        .map_or_else(|| s.to_string(), str::to_string)
}

pub(crate) fn cmd_grep(positional: &[String]) -> i32 {
    let args = match parse(positional) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let (rx, _cancel) = match Search::spawn(args.opts) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{}", tf(Msg::CliGrepBadPattern, &[&e.to_string()]));
            return 2;
        }
    };
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    let mut found = false;
    for b in rx {
        match b {
            Batch::Matches(ms) => {
                found = true;
                for m in &ms {
                    if writeln!(
                        out,
                        "{}:{}:{}: {}",
                        display(&m.path),
                        m.line_no,
                        m.col,
                        m.line
                    )
                    .is_err()
                    {
                        return if found { 0 } else { 1 }; // 파이프 닫힘(head 등)
                    }
                }
            }
            Batch::Error { path, message } => {
                let _ = out.flush();
                eprintln!("{}: {message}", display(&path));
            }
            Batch::Done(_) => break,
        }
    }
    let _ = out.flush();
    if found {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parses_flags_pattern_and_paths() {
        let a = parse(&args(&[
            "SELECT",
            "src",
            "docs",
            "-i",
            "-w",
            "-e",
            "--no-ignore",
            "-j",
            "3",
            "--max-file-kb",
            "64",
        ]))
        .expect("parse");
        assert_eq!(a.opts.query, "SELECT");
        assert_eq!(
            a.opts.roots,
            vec![PathBuf::from("src"), PathBuf::from("docs")]
        );
        assert!(!a.opts.case && a.opts.word && a.opts.regex && !a.opts.gitignore);
        assert_eq!((a.opts.threads, a.opts.max_file_kb), (3, 64));
        // 기본 = 대소문자 구분 · 현재 폴더 · .gitignore 적용.
        let a = parse(&args(&["x"])).expect("parse");
        assert!(a.opts.case && a.opts.gitignore);
        assert_eq!(a.opts.roots, vec![PathBuf::from(".")]);
        // `--` 뒤는 전부 경로 · `-`로 시작하는 패턴도 가능.
        let a = parse(&args(&["--", "-x", "-y"])).expect("parse");
        assert_eq!((a.opts.query.as_str(), a.opts.roots.len()), ("-x", 1));
        assert!(parse(&args(&["-i"])).is_err(), "패턴 없음");
        assert!(parse(&args(&["x", "-j", "many"])).is_err());
        assert!(parse(&args(&["x", "--bogus"])).is_err());
    }

    #[test]
    fn display_strips_dot_slash() {
        assert_eq!(display(std::path::Path::new("./a/b.sql")), "a/b.sql");
        assert_eq!(display(std::path::Path::new("src/a.rs")), "src/a.rs");
    }
}
