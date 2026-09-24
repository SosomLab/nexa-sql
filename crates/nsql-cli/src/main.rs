//! `nsql` — 명령줄 도구(docs/11). GUI와 같은 코어(`nsql-run`·`nsql-drivers`·`nsql-io`)를 쓴다.
//!
//! ```text
//! nsql plan   [-d dialect] <script|-> [args...]                 # 실행 없이 계획 출력(DB 불요)
//! nsql run    -c <target> [-d dialect] [-f grid|csv|tsv|json|jsonl] [--no-prompt] <script|-> [args...]
//! nsql shell  -c <target> [-d dialect]                          # 대화형(줄 단위 · `;`/`/`/명령으로 실행 · exit)
//! nsql export -c <target> (-q <sql> | -t <table>) [-f csv|tsv|json|jsonl|insert[:T]] [-o file]
//! nsql bookmark list [--project <file>] [--doc <path>] [--md] | add <file> <line> [--label <text>] | rm <id> | prune [--days N]
//! nsql conn   list | add <name> [<target>] [--host h --port n --db d --user u -d dialect -p pw] | show <name> | rm <name> | test [<name>] | path
//! nsql config list | get <key> | set <key> <value> | reset <key> | path     # 앱 설정(ui.lang · ui.theme …) — GUI와 공유
//! target: 프로필 이름 · sqlite::memory: · sqlite:file.db · oracle://u:p@h:1521/svc · mssql://u:p@h:1433/db · postgres://u:p@h:5432/db · u/p@h:1521/svc(-d로 방언)
//! ```

mod bookmark;
mod cat;
mod config;
mod conn;
mod grep;
mod help;
mod plan;
mod term;

use nsql_core::{DbError, Dialect, Session};
use nsql_i18n::{t, tf, Msg};
use nsql_io::{
    choose_key, generate, guess_table, split_table, Format, GridOpts, KeyMode, Overflow, SqlKind,
};
use nsql_run::{Opener, RunEvent, Runner, Spool, SpoolHandle};
use nsql_script::{split_script, ConnectSpec};
use nsql_vault::Vault;
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::path::Path;

struct Opts {
    cmd: String,
    target: Option<String>,
    dialect: Dialect,
    format: Format,
    no_prompt: bool,
    /// 비밀번호를 표준 입력의 첫 줄에서 읽는다(T-132 ③).
    password_stdin: bool,
    /// `-v 이름=값`(여러 번) — 스크립트가 돌기 전에 치환 변수(`&이름`)를 정의한다(T-153 · `DEFINE`과 같은 저장소).
    defines: Vec<(String, String)>,
    /// `--timing` — 항목마다 단계별 소요(docs/26)를 stderr에.
    timing: bool,
    /// `--log` — 실행 로그(타임스탬프 첫 컬럼 · 설정 `log.format`)를 stderr에.
    log: bool,
    /// `-p` — `conn add`의 비밀번호(접속 문자열에 넣기 싫을 때).
    password: Option<String>,
    /// `--host` · `--port` · `--db` · `--user` — 접속 문자열 대신 필드로(GUI 접속 폼과 같은 `ConnectSpec::from_parts` 경로).
    host: Option<String>,
    port: Option<u16>,
    database: Option<String>,
    user: Option<String>,
    query: Option<String>,
    table: Option<String>,
    out: Option<String>,
    /// `-s` — `cat`의 스키마(없으면 접속 사용자의 현재 스키마).
    schema: Option<String>,
    /// `--max-rows N` — 결과 셋 페치 상한(기본 0 = 무제한 · GUI는 설정 `grid.max_rows`).
    max_rows: usize,
    /// `--width N`(줄 폭 · 0 = 터미널 폭) · `--max-col-width N` · `--overflow wrap|truncate|expanded|none` · `-x`(expanded) — 없으면 설정 `cli.*`.
    width: Option<usize>,
    max_col: Option<usize>,
    overflow: Option<Overflow>,
    positional: Vec<String>,
}

fn usage() -> ! {
    eprint!("{}", help::text(None));
    std::process::exit(2);
}

fn parse_opts() -> Opts {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else { usage() };
    if cmd == "-h" || cmd == "--help" {
        help::print(None);
        std::process::exit(0);
    }
    // `nsql help [<명령>]` · `nsql <명령> --help` = 그 명령의 인자·옵션·예(사용자 09-16).
    let rest: Vec<String> = args.collect();
    if cmd == "help" {
        help::print(rest.first().map(String::as_str));
        std::process::exit(0);
    }
    if help::wants_help(&rest) {
        help::print(Some(&cmd));
        std::process::exit(0);
    }
    let args = rest.into_iter();
    if cmd == "--version" || cmd == "-V" {
        println!("nsql {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
    let default_format = nsql_settings::Settings::open_default()
        .ok()
        .and_then(|s| s.get("cli.format").and_then(Format::parse))
        .unwrap_or(Format::Grid);
    let mut o = Opts {
        cmd,
        target: None,
        dialect: Dialect::Oracle,
        format: default_format,
        no_prompt: false,
        password_stdin: false,
        defines: Vec::new(),
        timing: false,
        log: false,
        password: None,
        host: None,
        port: None,
        database: None,
        user: None,
        query: None,
        table: None,
        out: None,
        schema: None,
        max_rows: 0,
        width: None,
        max_col: None,
        overflow: None,
        positional: Vec::new(),
    };
    let mut it = args.peekable();
    while let Some(a) = it.next() {
        let mut val = |name: &str| -> String {
            match it.next() {
                Some(v) => v,
                None => {
                    eprintln!("{name} 값이 없습니다");
                    std::process::exit(2);
                }
            }
        };
        match a.as_str() {
            "-c" | "--connect" => o.target = Some(val("-c")),
            "-d" | "--dialect" => {
                let v = val("-d");
                o.dialect = Dialect::from_name(&v).unwrap_or_else(|| {
                    eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliErrDialect, &[&v]));
                    std::process::exit(2)
                });
            }
            "-f" | "--format" => {
                let v = val("-f");
                o.format = Format::parse(&v).unwrap_or_else(|| {
                    eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliErrFormat, &[&v]));
                    std::process::exit(2)
                });
            }
            "-p" | "--password" => o.password = Some(val("-p")),
            "--host" => o.host = Some(val("--host")),
            "--port" => {
                let v = val("--port");
                o.port = Some(v.parse().unwrap_or_else(|_| {
                    eprintln!(
                        "{}",
                        nsql_i18n::tf(nsql_i18n::Msg::CliErrNotNumber, &["--port", &v])
                    );
                    std::process::exit(2)
                }));
            }
            "--db" | "--database" => o.database = Some(val("--db")),
            "--user" | "-u" => o.user = Some(val("--user")),
            "-q" | "--query" => o.query = Some(val("-q")),
            "-t" | "--table" => o.table = Some(val("-t")),
            "-o" | "--out" => o.out = Some(val("-o")),
            "-s" | "--schema" => o.schema = Some(val("-s")),
            "--max-rows" => {
                let v = val("--max-rows");
                o.max_rows = v.parse().unwrap_or_else(|_| {
                    eprintln!(
                        "{}",
                        nsql_i18n::tf(nsql_i18n::Msg::CliErrNotNumber, &["--max-rows", &v])
                    );
                    std::process::exit(2)
                });
            }
            "--width" | "--line-width" | "--linesize" => {
                let v = val("--width");
                o.width = Some(v.parse().unwrap_or_else(|_| {
                    eprintln!(
                        "{}",
                        nsql_i18n::tf(nsql_i18n::Msg::CliErrNotNumber, &["--width", &v])
                    );
                    std::process::exit(2)
                }));
            }
            "--max-col-width" | "--colwidth" => {
                let v = val("--max-col-width");
                o.max_col = Some(v.parse().unwrap_or_else(|_| {
                    eprintln!(
                        "{}",
                        nsql_i18n::tf(nsql_i18n::Msg::CliErrNotNumber, &["--max-col-width", &v])
                    );
                    std::process::exit(2)
                }));
            }
            "--overflow" => {
                let v = val("--overflow");
                o.overflow = Some(Overflow::parse(&v).unwrap_or_else(|| {
                    eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliErrOverflow, &[&v]));
                    std::process::exit(2)
                }));
            }
            "-x" | "--expanded" => o.overflow = Some(Overflow::Expanded),
            "--no-prompt" => o.no_prompt = true,
            "--password-stdin" => o.password_stdin = true,
            "-v" | "--var" | "--define" => {
                let v = val("-v");
                match parse_define(&v) {
                    Some(d) => o.defines.push(d),
                    None => {
                        eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliErrDefine, &[&v]));
                        std::process::exit(2)
                    }
                }
            }
            "--timing" => o.timing = true,
            "--log" => o.log = true,
            _ => o.positional.push(a),
        }
    }
    o
}

fn read_source(path: &str) -> String {
    if path == "-" {
        let mut s = String::new();
        if let Err(e) = io::stdin().read_to_string(&mut s) {
            eprintln!(
                "{}",
                nsql_i18n::tf(nsql_i18n::Msg::CliErrStdin, &[&e.to_string()])
            );
            std::process::exit(1);
        }
        s
    } else {
        std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("{path}: {e}");
            std::process::exit(1)
        })
    }
}

/// 접속을 여는 한 자리(시작 `-c` · 스크립트 안의 `CONNECT` · `conn test`). ★ 비밀번호 자리가 없는 대상(`user@host`)은
/// 환경 변수 → 터미널 프롬프트로 채우고, **그래도 없으면 서버에 가지 않고 거절한다**(09-21: 터미널이 아닌 실행에서 스크립트의
/// `CONNECT user@host`가 빈 비밀번호로 로그인을 시도해 서버에 실패 기록을 남겼다 — 되풀이되면 계정이 잠긴다 · GUI와 같은 규칙).
/// 빈 비밀번호로 붙으려면 `user:@host`로 명시한다.
fn opener(default_dialect: Dialect) -> Opener {
    Box::new(
        move |spec: &ConnectSpec| -> Result<Box<dyn Session>, DbError> {
            if !password_missing(spec, default_dialect) {
                return nsql_drivers::open(spec, default_dialect);
            }
            let mut filled = spec.clone();
            let label = spec.redacted();
            term::ensure_password(&mut filled, false, &label);
            if password_missing(&filled, default_dialect) {
                return Err(DbError {
                    code: None,
                    message: nsql_i18n::tf(Msg::ErrPasswordRequiredCli, &[&label]),
                    position: None,
                });
            }
            let r = nsql_drivers::open(&filled, default_dialect);
            nsql_core::secret::wipe_opt(&mut filled.password);
            r
        },
    )
}

/// 비밀번호를 채워야 하는 대상인가 — SQLite가 아니고 · 호스트가 있고 · 비밀번호 자리가 없다(`user:@host`의 빈 값은 "있음").
fn password_missing(spec: &ConnectSpec, default_dialect: Dialect) -> bool {
    spec.dialect.unwrap_or(default_dialect) != Dialect::Sqlite
        && spec.host.is_some()
        && spec.password.is_none()
}

/// 스크립트 `CONNECT <프로필>` 해석기 — 저장소는 호출마다 연다(다른 인스턴스가 방금 저장한 프로필도 보인다).
fn resolver() -> nsql_run::Resolver {
    Box::new(|name: &str| {
        let v = Vault::open_default().map_err(|e| e.to_string())?;
        v.resolve(name).map_err(|e| e.to_string())
    })
}

/// 오류 머리 — `코드 · 분류: 대상`(분류 라벨 i18n · 분류 안 되면 코드만 · 둘 다 없으면 빈 문자열).
fn err_head(c: &nsql_core::Classified) -> String {
    use nsql_core::ErrorClass as C;
    use nsql_i18n::Msg as M;
    let label = match c.class {
        C::NoTable => Some(M::ErrClsNoTable),
        C::NoColumn => Some(M::ErrClsNoColumn),
        C::NoObject => Some(M::ErrClsNoObject),
        C::Syntax => Some(M::ErrClsSyntax),
        C::Permission => Some(M::ErrClsPermission),
        C::Login => Some(M::ErrClsLogin),
        C::Connection => Some(M::ErrClsConnection),
        C::Unique => Some(M::ErrClsUnique),
        C::ForeignKey => Some(M::ErrClsForeignKey),
        C::NotNull => Some(M::ErrClsNotNull),
        C::Check => Some(M::ErrClsCheck),
        C::Lock => Some(M::ErrClsLock),
        C::Deadlock => Some(M::ErrClsDeadlock),
        C::DataType => Some(M::ErrClsDataType),
        C::Resource => Some(M::ErrClsResource),
        C::Unknown => None,
    };
    let mut head = c.code.clone().unwrap_or_default();
    if let Some(l) = label {
        if !head.is_empty() {
            head.push_str(" · ");
        }
        head.push_str(nsql_i18n::t(l));
        if let Some(o) = &c.object {
            head.push_str(": ");
            head.push_str(o);
        }
    }
    head
}

/// 설정 `sql.key_mode`(pk 기본 · all).
fn key_mode_setting() -> KeyMode {
    nsql_settings::Settings::open_default()
        .ok()
        .and_then(|s| s.get("sql.key_mode").and_then(KeyMode::parse))
        .unwrap_or(KeyMode::Pk)
}

/// 설정 `vars.signature_lookup` · `pg.refcursor_expand`(부하원 스위치 · 39 §3)를 러너에 넣는다.
fn apply_load_switches(runner: &mut Runner) {
    if let Ok(s) = nsql_settings::Settings::open_default() {
        // Oracle 클라이언트(설정 ▸ DBMS ▸ Oracle) — GUI와 같은 값을 CLI도 쓴다(첫 접속 전).
        nsql_drivers::set_oracle_client(
            s.get("oracle.client_mode") == Some("manual"),
            s.get("oracle.client_dir").unwrap_or(""),
            s.get("oracle.tns_admin").unwrap_or(""),
        );
        runner.signature_lookup = s.get("vars.signature_lookup").is_none_or(|v| v == "on");
        runner.refcursor_expand = s.get("pg.refcursor_expand").is_none_or(|v| v == "on");
        runner.engine.settings.env_subst = s.get("vars.env_subst").is_none_or(|v| v == "on");
        runner.engine.settings.expand_at_use = s.get("vars.expand_at") == Some("use");
    }
}

/// 설정 `vars.brace_subst` · `vars.max_value_kb`(T-153) — (켬, KB).
fn var_limits_setting() -> (bool, usize) {
    nsql_settings::Settings::open_default()
        .ok()
        .map_or((true, 1024), |s| {
            (
                s.get("vars.brace_subst").is_none_or(|v| v == "on"),
                s.int("vars.max_value_kb").max(0) as usize,
            )
        })
}

/// 설정 `vars.into_policy`(D-139) — `first`면 `SELECT … INTO`의 여러 행에서 첫 행을 쓴다.
fn into_first_setting() -> bool {
    nsql_settings::Settings::open_default()
        .ok()
        .is_some_and(|s| s.get("vars.into_policy") == Some("first"))
}

/// 설정 `run.cursor_autoshow` — 실행 뒤 돌아온 REF CURSOR를 바로 결과로(기본 켬 · 끄면 `PRINT rc`).
fn cursor_autoshow_setting() -> bool {
    nsql_settings::Settings::open_default()
        .ok()
        .and_then(|s| s.get("run.cursor_autoshow").map(|v| v == "on"))
        .unwrap_or(true)
}

/// 설정 `script.strict`(HIDDEN · T-9) — 미정의 `&var`·선언 없는 `:bind`를 오류로.
fn strict_setting() -> bool {
    nsql_settings::Settings::open_default()
        .ok()
        .and_then(|s| s.get("script.strict").map(|v| v == "on"))
        .unwrap_or(false)
}

/// 셸 추가 페치 명령(T-48d · docs/43 §5) — `\more [n]`/`more [n]` · `\all` · `\count` · `\pager on|off`.
#[derive(Debug, PartialEq, Eq)]
enum FetchCmd {
    More(Option<usize>),
    All,
    Count,
    Pager(Option<bool>),
}

fn fetch_cmd(line: &str) -> Option<FetchCmd> {
    let t = line.trim().trim_end_matches(';').trim();
    let (head, rest) = match t.find(char::is_whitespace) {
        Some(i) => (&t[..i], t[i..].trim()),
        None => (t, ""),
    };
    let low = head.to_ascii_lowercase();
    Some(match low.as_str() {
        "\\more" | "more" => {
            if rest.is_empty() {
                FetchCmd::More(None)
            } else {
                FetchCmd::More(Some(rest.parse().ok()?))
            }
        }
        "\\all" if rest.is_empty() => FetchCmd::All,
        "\\count" if rest.is_empty() => FetchCmd::Count,
        "\\pager" => match rest.to_ascii_lowercase().as_str() {
            "" => FetchCmd::Pager(None),
            "on" => FetchCmd::Pager(Some(true)),
            "off" => FetchCmd::Pager(Some(false)),
            _ => return None,
        },
        _ => return None,
    })
}

/// 셸 페치 명령 실행 — 마지막 문장의 다음 세그먼트(커서 · 폴백 OFFSET) · 전체 · 건수 · 페이저.
fn run_fetch_cmd(cmd: FetchCmd, p: &mut Printer, runner: &mut Runner) {
    if let FetchCmd::Pager(v) = cmd {
        p.pager = v.unwrap_or(!p.pager);
        eprintln!(
            "{}",
            tf(
                Msg::CliPagerStatus,
                &[if p.pager { "on" } else { "off" }, &pager_name()]
            )
        );
        return;
    }
    let Some(sql) = p.last_sql.clone() else {
        eprintln!("{}", t(Msg::CliNoMoreRows));
        return;
    };
    match cmd {
        FetchCmd::Count => match runner.count(&sql) {
            Ok((n, tl)) => {
                let line = tf(
                    Msg::CliCountResult,
                    &[&n.to_string(), &format!("{:.3}", tl.total().as_secs_f64())],
                );
                p.out(format!("{line}\n").as_bytes());
                if p.timing {
                    p.err(&format!("⏱ {}", tl.summary()));
                }
            }
            Err(e) => p.err(&format!("ERROR: {e}")),
        },
        FetchCmd::More(n) => {
            if !p.more {
                eprintln!("{}", t(Msg::CliNoMoreRows));
                return;
            }
            let n = n.unwrap_or(p.max_rows);
            let offset = p.served;
            match runner.fetch_page(&sql, offset, n) {
                Ok((rs, more, tl, cursor)) => p.print_page(&rs, more, &tl, cursor),
                Err(e) => p.err(&format!("ERROR: {e}")),
            }
        }
        FetchCmd::All => {
            if !p.more {
                eprintln!("{}", t(Msg::CliNoMoreRows));
                return;
            }
            // 커서가 살아 있으면 배치로 스트리밍(메모리 상한 불필요) · 아니면 OFFSET 재질의 한 번(남은 전부).
            let batch = runner.fetch_size.max(2000);
            if runner.cursor_matches(&sql, p.served) {
                while let Some((h, _)) = runner.cursor() {
                    match runner.fetch_next(h, batch) {
                        Ok((rs, more, tl)) => {
                            p.print_page(&rs, more, &tl, true);
                            if !more {
                                break;
                            }
                        }
                        Err(e) => {
                            p.err(&format!("ERROR: {e}"));
                            break;
                        }
                    }
                }
            } else {
                match runner.fetch_offset(&sql, p.served, 0) {
                    Ok((rs, more, tl)) => p.print_page(&rs, more, &tl, false),
                    Err(e) => p.err(&format!("ERROR: {e}")),
                }
            }
        }
        FetchCmd::Pager(_) => {}
    }
}

/// 페치 관련 설정(docs/43 §4-3) — `db.fetch_size` · `grid.fetch_mode`(cursor만 유지) · `db.cursor_idle_secs`.
/// 내장 변수 표(GUI `run_intrinsic`와 같은 규칙 · 설정 `vars.intrinsic` 끔 = 빈 표): 스크립트 경로(절대) · cwd · 홈 · 설정 폴더 ·
/// 실행 파일 · 방언 · 설정 값 전부(`config:키`). 프로젝트는 CLI에 없으므로 `${workspaceFolder}`는 만들지 않는다.
fn intrinsic_vars(path: &str, dialect: Dialect) -> std::collections::BTreeMap<String, String> {
    let s = nsql_settings::Settings::open_default().ok();
    if s.as_ref().is_some_and(|s| !s.flag("vars.intrinsic")) {
        return std::collections::BTreeMap::new();
    }
    let file = (path != "-")
        .then(|| std::path::absolute(path).ok())
        .flatten();
    let ctx = nsql_script::intrinsic::Context {
        project_file: None,
        workspace_dir: None,
        folders: Vec::new(),
        file,
        line: None,
        column: None,
        user_home: std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(std::path::PathBuf::from),
        app_home: nsql_settings::config_dir(),
        exec: std::env::current_exe().ok(),
        cwd: std::env::current_dir().ok(),
        profile: None,
        dialect: Some(dialect.to_string()),
        config: s
            .as_ref()
            .map(|s| {
                nsql_settings::REGISTRY
                    .iter()
                    .filter_map(|e| s.get(e.key).map(|v| (e.key.to_string(), v.to_string())))
                    .collect()
            })
            .unwrap_or_default(),
    };
    nsql_script::intrinsic::build(&ctx)
}

fn fetch_settings() -> (usize, bool, u64) {
    match nsql_settings::Settings::open_default() {
        Ok(s) => (
            s.int("db.fetch_size").max(0) as usize,
            s.get("grid.fetch_mode").unwrap_or("cursor") == "cursor",
            s.int("db.cursor_idle_secs").max(0) as u64,
        ),
        Err(_) => (0, true, 0),
    }
}

/// 셸 별칭(T-52 · docs/27 §1-1 ⑥ · psql `\d` · sqlite3 `.tables` · sqlcmd `:r`) → SQL*Plus식 명령 한 줄.
/// 순수 함수(테스트) — 바꾼 줄은 그대로 스크립트 엔진으로 간다. `:r`은 엔진이 이미 안다(`Command::Include`).
#[derive(Debug, PartialEq, Eq)]
enum ShellAlias {
    Rewrite(String),
    Help,
}

fn shell_alias(line: &str) -> Option<ShellAlias> {
    let t = line.trim().trim_end_matches(';').trim();
    let (head, rest) = match t.find(char::is_whitespace) {
        Some(i) => (&t[..i], t[i..].trim()),
        None => (t, ""),
    };
    let low = head.to_ascii_lowercase();
    Some(match (low.as_str(), rest.is_empty()) {
        ("\\?" | "\\h" | "help", true) => ShellAlias::Help,
        ("\\d" | "\\dt" | ".tables" | ".table", true) => ShellAlias::Rewrite("SHOW TABLES".into()),
        ("\\dv" | ".views", true) => ShellAlias::Rewrite("SHOW VIEWS".into()),
        ("\\d" | ".schema", false) => ShellAlias::Rewrite(format!("DESCRIBE {rest}")),
        ("\\i" | ".read", false) => ShellAlias::Rewrite(format!("@{rest}")),
        ("\\c" | "\\connect", false) => ShellAlias::Rewrite(format!("CONNECT {rest}")),
        _ => return None,
    })
}

/// 셸 `\?`/`help` — 명령 표 + 별칭 표(stderr · `help set`과 같은 자리).
fn print_shell_help() {
    eprintln!("{}", t(Msg::HlpShellCommands));
    for line in t(Msg::HlpShellCommandList).split('\n') {
        eprintln!("  {line}");
    }
    for line in t(Msg::HlpShellFetchList).split('\n') {
        eprintln!("  {line}");
    }
    eprintln!("\n{}", t(Msg::HlpShellAliases));
    for line in t(Msg::HlpShellAliasList).split('\n') {
        eprintln!("  {line}");
    }
    eprintln!("\n{}", t(Msg::CliShellSetHelp));
}

/// 표 폭 옵션 — 설정 `cli.width`/`cli.max_col_width`/`cli.overflow`(`nsql config set …`) 위에 플래그가 덮는다.
/// 줄 폭 0 = 터미널이면 콘솔 폭 · 파이프/파일이면 무제한.
fn grid_opts(o: &Opts) -> GridOpts {
    let mut g = GridOpts::default();
    if let Ok(s) = nsql_settings::Settings::open_default() {
        // NULL 글자(`cli.null_text` · 기본 빈 칸) — 표·Markdown·CSV/TSV가 한 값을 쓴다(GUI는 `grid.null_text`).
        g.null = s.get("cli.null_text").unwrap_or("").to_string();
        g.line_width = s.int("cli.width").max(0) as usize;
        g.max_col = s.int("cli.max_col_width").max(0) as usize;
        g.overflow = s
            .get("cli.overflow")
            .and_then(Overflow::parse)
            .unwrap_or(Overflow::None);
    }
    if let Some(w) = o.width {
        g.line_width = w;
    }
    if let Some(c) = o.max_col {
        g.max_col = c;
    }
    if let Some(v) = o.overflow {
        g.overflow = v;
    }
    if g.line_width == 0 {
        g.line_width = term::columns().unwrap_or(0);
    }
    g
}

/// 셸 `set`/`show`/`\x` — 세션 동안만 바뀐다(영구는 `nsql config set cli.*`). 처리했으면 true.
fn shell_set(line: &str, p: &mut Printer, session: Option<&mut (dyn Session + 'static)>) -> bool {
    let dialect = p.dialect;
    if let Some(rest) = line.trim().to_ascii_lowercase().strip_prefix("copy") {
        let rest = rest.trim().trim_end_matches(';');
        if rest.is_empty() || Format::parse(rest).is_some() {
            let fmt = Format::parse(rest).unwrap_or_else(|| p.format.clone());
            match &p.last {
                None => eprintln!("{}", nsql_i18n::t(nsql_i18n::Msg::CliCopyNoResult)),
                Some(rs) => {
                    let text = if let Format::Sql(kind) = &fmt {
                        let (t, warn) = sql_statements(
                            session,
                            dialect,
                            p.key_mode,
                            p.last_sql.as_deref().unwrap_or(""),
                            rs,
                            *kind,
                        );
                        let mut t = t;
                        for w in warn {
                            t.push_str("-- ");
                            t.push_str(&w);
                            t.push('\n');
                        }
                        t
                    } else {
                        nsql_io::render_result_set(rs, &fmt, dialect, &p.grid)
                    };
                    match term::clipboard_write(&text) {
                        Ok(()) => eprintln!(
                            "{}",
                            nsql_i18n::tf(
                                nsql_i18n::Msg::CliCopied,
                                &[&rs.rows.len().to_string(), &fmt.name()]
                            )
                        ),
                        Err(e) => {
                            eprintln!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliCopyFailed, &[&e]))
                        }
                    }
                }
            }
            return true;
        }
    }
    let g = &mut p.grid;
    let t = line.trim();
    let low = t.to_ascii_lowercase();
    let fmt_name = p.format.name();
    let max_rows = p.max_rows;
    let pager = p.pager;
    let show = |g: &GridOpts| {
        eprintln!(
            "width {} (0 = 무제한 · set width auto = 터미널 폭) · colwidth {} · overflow {} · format {} · max_rows {} · pager {}",
            g.line_width,
            g.max_col,
            g.overflow.name(),
            fmt_name,
            max_rows,
            if pager { "on" } else { "off" }
        );
    };
    if low == "\\x" {
        g.overflow = if g.overflow == Overflow::Expanded {
            Overflow::Wrap
        } else {
            Overflow::Expanded
        };
        show(g);
        return true;
    }
    if low == "show" || low == "set" || low == "get" || low == "\\pset" || low == "help set" {
        show(g);
        eprintln!("{}", nsql_i18n::t(nsql_i18n::Msg::CliShellSetHelp));
        eprintln!("{}", nsql_i18n::t(nsql_i18n::Msg::CliShellSetMaxRows));
        return true;
    }
    // get/show <키> — 값 하나만.
    if let Some(k) = low
        .strip_prefix("get ")
        .or_else(|| low.strip_prefix("show "))
    {
        let k = k.trim().trim_end_matches(';');
        let v = match k {
            "width" | "linesize" | "line_width" => g.line_width.to_string(),
            "colwidth" | "max_col_width" | "col" => g.max_col.to_string(),
            "overflow" | "wrap" => g.overflow.name().to_string(),
            "format" | "fmt" => fmt_name.clone(),
            "max_rows" | "maxrows" | "rows" => max_rows.to_string(),
            "pager" => if pager { "on" } else { "off" }.to_string(),
            "all" | "" => {
                show(g);
                return true;
            }
            _ => return false,
        };
        eprintln!("{k} = {v}");
        return true;
    }
    let Some(rest) = low.strip_prefix("set ") else {
        return false;
    };
    let rest = rest.trim_end_matches(';');
    let mut it = rest.split_whitespace();
    let (Some(k), Some(v)) = (it.next(), it.next()) else {
        show(g);
        return true;
    };
    let num = |v: &str| v.parse::<usize>().ok();
    match k {
        "width" | "linesize" | "line_width" => {
            if v == "auto" {
                g.line_width = term::columns().unwrap_or(0);
            } else if let Some(n) = num(v) {
                g.line_width = n;
            } else {
                eprintln!("set width <n|auto>");
            }
        }
        "colwidth" | "max_col_width" | "col" => match num(v) {
            Some(n) => g.max_col = n,
            None => eprintln!("set colwidth <n>"),
        },
        // 셸 조회 상한(T-48d · 세션 동안만 · 영구는 `nsql config set cli.max_rows`).
        "max_rows" | "maxrows" | "rows" => match num(v) {
            Some(n) => {
                p.max_rows = n;
                let g = g.clone();
                show(&g);
                return true;
            }
            None => eprintln!("set max_rows <n>"),
        },
        "pager" => match v {
            "on" | "off" => {
                p.pager = v == "on";
                let g = g.clone();
                show(&g);
                return true;
            }
            _ => eprintln!("set pager on|off"),
        },
        "overflow" | "wrap" => match Overflow::parse(v) {
            Some(x) => g.overflow = x,
            None => eprintln!("set overflow none|wrap|truncate|expanded"),
        },
        "format" | "fmt" => match Format::parse(v) {
            Some(f) => {
                let g = g.clone();
                p.format = f;
                let fmt_name = p.format.name();
                eprintln!(
                    "width {} · colwidth {} · overflow {} · format {}",
                    g.line_width,
                    g.max_col,
                    g.overflow.name(),
                    fmt_name
                );
                return true;
            }
            None => eprintln!(
                "set format grid|markdown|csv|tsv|json|jsonl|sql:select|insert|update|delete|merge"
            ),
        },
        _ => return false,
    }
    show(g);
    true
}

/// `-c <target>` — 프로필 이름이면 저장소에서, 아니면 접속 문자열 파싱.
fn resolve_target(target: &str, dialect: Dialect) -> Result<ConnectSpec, String> {
    if nsql_vault::is_profile_name(target) {
        let v = Vault::open_default().map_err(|e| e.to_string())?;
        if let Some(spec) = v.resolve(target).map_err(|e| e.to_string())? {
            return Ok(spec);
        }
    }
    nsql_drivers::parse_target(target, dialect)
}

/// `$PAGER`(없으면 Unix `less -FRSX` · Windows `more`)에 bytes를 흘려 보내고 끝날 때까지 기다린다(psql `\pset pager`).
fn page_through(bytes: &[u8]) -> Result<(), String> {
    let spec = std::env::var("PAGER").ok().filter(|p| !p.trim().is_empty());
    let (prog, args): (String, Vec<String>) = match spec {
        Some(p) => {
            let mut it = p.split_whitespace().map(str::to_string);
            (it.next().unwrap_or_default(), it.collect())
        }
        None if cfg!(windows) => ("more".into(), vec![]),
        None => ("less".into(), vec!["-FRSX".into()]),
    };
    let mut child = std::process::Command::new(&prog)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("{prog}: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(bytes);
    }
    child.wait().map(|_| ()).map_err(|e| e.to_string())
}

/// 페이저 이름(상태 표시용).
fn pager_name() -> String {
    std::env::var("PAGER")
        .ok()
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "more".into()
            } else {
                "less -FRSX".into()
            }
        })
}

/// 스풀에 복사(켜져 있을 때만) — 쓰기 실패는 한 번만 stderr에(스풀은 닫힌다).
fn spool_write(spool: &SpoolHandle, bytes: &[u8]) {
    if let Ok(mut sp) = spool.lock() {
        sp.write(bytes);
        if let Some(e) = sp.take_error() {
            eprintln!("{}", tf(Msg::SpoolWriteFailed, &[&e]));
        }
    }
}

/// 이벤트 → 터미널 출력. 반환 = 오류 수.
struct Printer {
    format: Format,
    /// 표 폭·넘침(설정 `cli.*` → 플래그 → 셸 `set`).
    grid: GridOpts,
    /// 마지막 결과(셸 `copy` — 터미널이 접은 줄이 아니라 원문을 클립보드로 · 09-16).
    last: Option<nsql_core::ResultSet>,
    /// 마지막 결과를 만든 문장(테이블 추정용).
    last_sql: Option<String>,
    /// 스크립트의 문장 목록(index → 본문 · `Begin.index`로 찾는다).
    stmts: Vec<String>,
    cur_stmt: usize,
    /// `-f sql:*` 결과는 실행이 끝난 뒤(세션이 자유로울 때) 키를 조회해 찍는다 — (결과, more, 소요, 문장 index, 종류).
    deferred: Vec<(
        nsql_core::ResultSet,
        bool,
        std::time::Duration,
        usize,
        SqlKind,
    )>,
    /// 설정 `sql.key_mode`.
    key_mode: KeyMode,
    dialect: Dialect,
    errors: usize,
    feedback: bool,
    /// 단계별 소요 출력(`--timing`).
    timing: bool,
    /// 실행 로그 허브(`--log` · 별도 스레드 stderr 싱크 · 없으면 None).
    log: Option<nsql_log::LogHub>,
    /// `SPOOL`(T-9) — 이 Printer가 찍는 모든 것(stdout·stderr 오류·타이밍)과 실시간 서버 메시지를 파일에도. Runner와 공유.
    spool: SpoolHandle,
    /// `nsql shell`인가(T-48d · docs/43 §5) — 상한·`\more` 안내·페이저는 셸에서만(D-68: run/export/파이프는 무제한).
    shell: bool,
    /// 셸 조회 상한(설정 `cli.max_rows` · `set max_rows` · 0 = 무제한). grid/markdown일 때만 적용([`Printer::shell_limit`]).
    max_rows: usize,
    /// 마지막 결과가 상한에서 잘렸는가(`\more`·`\all`·auto_more의 조건).
    more: bool,
    /// 마지막 문장에서 지금까지 보여 준 행 수(= 다음 페치의 OFFSET).
    served: usize,
    /// `\pager on` — 결과 표를 `$PAGER`(기본 less)로.
    pager: bool,
    /// 설정 `cli.auto_more` — 잘린 결과 뒤 묻지 않고 다음 세그먼트를 이어 출력.
    auto_more: bool,
}

impl Printer {
    fn new(o: &Opts, format: Format, feedback: bool) -> Printer {
        Printer {
            format,
            grid: grid_opts(o),
            last: None,
            last_sql: None,
            stmts: Vec::new(),
            cur_stmt: 0,
            deferred: Vec::new(),
            key_mode: key_mode_setting(),
            dialect: o.dialect,
            errors: 0,
            feedback,
            timing: o.timing,
            log: log_hub(o),
            spool: Spool::new_handle(),
            shell: false,
            max_rows: 0,
            more: false,
            served: 0,
            pager: false,
            auto_more: false,
        }
    }

    /// 셸에서 러너에 줄 상한 — 표 형식(grid/markdown)일 때만 `max_rows`, csv/tsv/json 등 복사·저장용 출력은 0(D-68).
    fn shell_limit(&self) -> usize {
        if self.shell && matches!(self.format, Format::Grid | Format::Markdown) {
            self.max_rows
        } else {
            0
        }
    }

    /// 결과 표 출력 — `\pager on`이고 터미널이면 `$PAGER`(기본 `less -FRSX`)로, 아니면 stdout. 스풀에는 늘 복사.
    fn out_table(&mut self, bytes: &[u8]) {
        if !(self.pager && self.shell && io::stdout().is_terminal()) {
            self.out(bytes);
            return;
        }
        let _ = io::stdout().flush();
        match page_through(bytes) {
            Ok(()) => spool_write(&self.spool, bytes),
            Err(e) => {
                eprintln!("{}", tf(Msg::CliPagerFailed, &[&e]));
                self.out(bytes);
            }
        }
    }

    /// 추가 페치 결과 한 묶음(`\more` · `\all` · auto_more) — 표 + 피드백 한 줄 + (더 있으면) 안내.
    fn print_page(
        &mut self,
        rs: &nsql_core::ResultSet,
        more: bool,
        tl: &nsql_core::Timeline,
        cursor: bool,
    ) {
        let mut out: Vec<u8> = Vec::new();
        let _ =
            nsql_io::write_result_set_opts(&mut out, rs, &self.format, self.dialect, &self.grid);
        self.served += rs.rows.len();
        self.more = more;
        self.last = Some(rs.clone());
        if matches!(self.format, Format::Grid | Format::Markdown) {
            let _ = writeln!(
                out,
                "\n{}",
                tf(
                    Msg::CliFetchedRows,
                    &[
                        &rs.rows.len().to_string(),
                        &format!("{:.3}", tl.total().as_secs_f64()),
                        t(if cursor {
                            Msg::CliMoreSource
                        } else {
                            Msg::CliMoreSourceOffset
                        }),
                        &self.served.to_string(),
                    ]
                )
            );
            if more {
                let _ = writeln!(
                    out,
                    "{}",
                    tf(Msg::CliShellMoreHint, &[&self.served.to_string()])
                );
            }
            let _ = writeln!(out);
        }
        self.out_table(&out);
        if self.timing {
            self.err(&format!("⏱ {}", tl.summary()));
        }
    }

    /// stdout + 스풀.
    fn out(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let o = io::stdout();
        let mut o = o.lock();
        let _ = o.write_all(bytes);
        drop(o);
        spool_write(&self.spool, bytes);
    }

    /// stderr + 스풀(SQL*Plus처럼 오류도 스풀에 남는다). stdout을 먼저 비워 순서를 지킨다.
    fn err(&mut self, text: &str) {
        let _ = io::stdout().flush();
        eprintln!("{text}");
        spool_write(&self.spool, format!("{text}\n").as_bytes());
    }

    /// 스크립트 문장 목록(테이블 추정용 · 실행 전에).
    fn set_source(&mut self, src: &str) {
        self.stmts = split_script(src).into_iter().map(|i| i.text).collect();
        self.cur_stmt = 0;
    }

    /// `-f sql:*`로 모아 둔 결과를 찍는다 — 세션으로 키(PK/유니크)를 조회하고 결과마다 경고는 1회(문장 끝에 SQL 주석).
    fn flush_sql(&mut self, mut session: Option<&mut (dyn Session + 'static)>) {
        if self.deferred.is_empty() {
            return;
        }
        let items = std::mem::take(&mut self.deferred);
        let mut out: Vec<u8> = Vec::new();
        for (rs, more, elapsed, idx, kind) in items {
            let sql = self.stmts.get(idx).map(String::as_str).unwrap_or("");
            let (text, warn) = sql_statements(
                session.as_deref_mut(),
                self.dialect,
                self.key_mode,
                sql,
                &rs,
                kind,
            );
            let _ = out.write_all(text.as_bytes());
            for w in warn {
                let _ = writeln!(out, "-- {w}");
            }
            if self.feedback {
                let note = if more {
                    format!(" {}", nsql_i18n::t(nsql_i18n::Msg::CliRowsMore))
                } else {
                    String::new()
                };
                let _ = writeln!(
                    out,
                    "\n{} rows ({:.3}s){note}\n",
                    rs.rows.len(),
                    elapsed.as_secs_f64()
                );
            }
        }
        self.out(&out);
    }
}

/// 결과 → SQL 문(docs/41 키 규칙): 테이블 = 실행문에서 추정 · 키 = 설정(pk: 카탈로그 PK → 유니크 → 앞 3컬럼 · all: 전체) ·
/// 경고(테이블 미추정 · 앞 컬럼 대체)는 결과당 1회로 돌려준다(호출자가 맨 아래에 `-- ` 주석으로).
fn sql_statements(
    session: Option<&mut (dyn Session + 'static)>,
    dialect: Dialect,
    mode: KeyMode,
    sql: &str,
    rs: &nsql_core::ResultSet,
    kind: SqlKind,
) -> (String, Vec<String>) {
    let names: Vec<String> = rs.columns.iter().map(|c| c.name.clone()).collect();
    let guess = guess_table(sql);
    let table = guess.clone().unwrap_or_else(|| "T".into());
    let mut warn = Vec::new();
    let key = if mode == KeyMode::All {
        choose_key(KeyMode::All, None, &names)
    } else {
        let info = match (&guess, session) {
            (Some(g), Some(s)) => {
                let (schema, t) = split_table(dialect, g);
                let schema = schema.or_else(|| nsql_catalog::current_schema(s).ok());
                schema.and_then(|sc| nsql_catalog::keys(s, &sc, &t).ok())
            }
            _ => None,
        };
        choose_key(KeyMode::Pk, info.as_ref(), &names)
    };
    if guess.is_none() {
        warn.push(nsql_i18n::t(nsql_i18n::Msg::SqlKeyWarnNoTable).to_string());
    }
    if key.needs_warning() && kind != SqlKind::Insert {
        warn.push(nsql_i18n::tf(
            nsql_i18n::Msg::SqlKeyWarnFirstN,
            &[&table, &key.cols.len().to_string(), &key.cols.join(", ")],
        ));
    }
    (
        generate(dialect, &table, &names, &rs.rows, kind, &key),
        warn,
    )
}

/// `--log`면 설정 `log.format`으로 stderr 싱크를 **로그 스레드**에 띄운다(메인 경로 비차단 · docs/26 §3).
fn log_hub(o: &Opts) -> Option<nsql_log::LogHub> {
    if !o.log {
        return None;
    }
    // 형식 어댑터(설정 `log.format` · 템플릿 · 컬럼) + 파일 싱크(`log.file` · 형식 `log.file_format` · 회전 `log.file_max_kb`).
    let st = nsql_settings::Settings::open_default().ok();
    let get = |k: &str, d: &str| st.as_ref().and_then(|s| s.get(k)).unwrap_or(d).to_string();
    let name = get("log.format", "raw");
    let tpl = get("log.template", nsql_log::DEFAULT_TEMPLATE);
    let cols = nsql_log::Columns::parse(&get("log.columns", ""));
    let mut sinks: Vec<Box<dyn nsql_log::LogSink>> = vec![Box::new(nsql_log::StderrSink::new(
        nsql_log::formatter_with(&name, &tpl, cols),
    ))];
    let file = get("log.file", "");
    if !file.trim().is_empty() {
        let ff = get("log.file_format", "same");
        let ff = if ff == "same" { name } else { ff };
        let max_kb: u64 = get("log.file_max_kb", "5120").parse().unwrap_or(5120);
        match nsql_log::FileSink::open(
            file.trim(),
            nsql_log::formatter_with(&ff, &tpl, cols),
            max_kb * 1024,
        ) {
            Ok(s) => sinks.push(Box::new(s)),
            Err(e) => eprintln!(
                "{}",
                nsql_i18n::tf(nsql_i18n::Msg::ErrLogFile, &[&e.to_string()])
            ),
        }
    }
    Some(nsql_log::LogHub::spawn(sinks, 4096))
}

impl Printer {
    fn handle(&mut self, ev: RunEvent) {
        if let Some(hub) = &self.log {
            for e in nsql_run::log_entries(&ev) {
                hub.push(e);
            }
        }
        // stdout으로 갈 것은 버퍼에 모아 한 번에(스풀 tee) · 오류·타이밍은 `err`(stderr + 스풀).
        let mut out: Vec<u8> = Vec::new();
        match ev {
            RunEvent::Begin { index, .. } => self.cur_stmt = index,
            RunEvent::ResultSet {
                rs,
                elapsed,
                more,
                label,
                ..
            } => {
                self.last_sql = self.stmts.get(self.cur_stmt).cloned();
                // 이름 있는 결과(REF CURSOR 변수)는 표 위에 이름 한 줄 — 커서가 여럿일 때 어느 것인지 보이게(표 형식에서만).
                if let Some(name) = &label {
                    if matches!(self.format, Format::Grid | Format::Markdown) {
                        let _ = writeln!(out, "{name}:");
                    }
                }
                if let Format::Sql(kind) = &self.format {
                    // 키 조회에 세션이 필요 — 실행 중엔 Runner가 쥐고 있으므로 끝난 뒤 `flush_sql`.
                    self.deferred
                        .push((rs.clone(), more, elapsed, self.cur_stmt, *kind));
                    self.last = Some(rs);
                    return;
                }
                let _ = nsql_io::write_result_set_opts(
                    &mut out,
                    &rs,
                    &self.format,
                    self.dialect,
                    &self.grid,
                );
                if self.feedback {
                    self.last = Some(rs.clone());
                }
                // 셸의 추가 페치 상태(T-48d): 이 결과가 잘렸는가 · 지금까지 보여 준 행 수.
                self.more = more;
                self.served = rs.rows.len();
                if self.feedback && matches!(self.format, Format::Grid | Format::Markdown) {
                    let note = if more && !self.shell {
                        format!(" {}", t(Msg::CliRowsMore))
                    } else {
                        String::new()
                    };
                    let _ = writeln!(
                        out,
                        "\n{} rows ({:.3}s){note}",
                        rs.rows.len(),
                        elapsed.as_secs_f64()
                    );
                    // 셸에서 잘리면 표 뒤 한 줄 안내(grid/markdown일 때만 · docs/43 §5).
                    if more && self.shell {
                        let _ = writeln!(
                            out,
                            "{}",
                            tf(Msg::CliShellMoreHint, &[&rs.rows.len().to_string()])
                        );
                    }
                    let _ = writeln!(out);
                }
                self.out_table(&out);
                return;
            }
            RunEvent::Done {
                rows_affected,
                elapsed,
                ..
            } => {
                if self.feedback {
                    match rows_affected {
                        Some(n) => {
                            let _ =
                                writeln!(out, "{n} rows affected ({:.3}s)", elapsed.as_secs_f64());
                        }
                        None => {
                            let _ = writeln!(out, "OK ({:.3}s)", elapsed.as_secs_f64());
                        }
                    }
                }
            }
            RunEvent::VarList { vars } => {
                // SQL*Plus `VARIABLE`(인자 없음) = 선언된 변수의 이름·타입.
                for (n, t) in vars {
                    let _ = writeln!(out, "variable {n} {}", t.sql_name());
                }
            }
            RunEvent::Print { pairs } => {
                for (n, v) in pairs {
                    // 비밀 이름 규칙(D-140 · `*PASS*` `*PWD*` `*SECRET*` `*TOKEN*`)은 여기서도 지킨다 — `SHOW VARIABLES`는
                    // 가리는데 대입 메아리·`PRINT`는 값을 그대로 찍고 있었다(T-162 ③ · 터미널 기록·CI 로그로 새는 길).
                    let bare = n.trim_start_matches([':', '&', '@']);
                    if nsql_script::looks_secret(bare) {
                        let _ = writeln!(out, "{n} = ******");
                    } else {
                        let _ = writeln!(out, "{n} = {}", v.display());
                    }
                }
            }
            RunEvent::Message(m) => {
                let _ = writeln!(out, "{m}");
            }
            RunEvent::Warning(m) => {
                let _ = writeln!(out, "{m}");
            }
            RunEvent::Connected {
                description,
                dialect,
            } => {
                self.dialect = dialect;
                let _ = writeln!(out, "Connected: {description} ({dialect})");
            }
            RunEvent::Disconnected | RunEvent::ConnectionClosed => {
                let _ = writeln!(out, "Disconnected");
            }
            // 변수 표 사건은 GUI용(패널·탭 표) — CLI는 `PRINT`/`SHOW VARIABLES`로 본다.
            RunEvent::ReadTxEnded { .. } | RunEvent::Vars { .. } | RunEvent::InputNeeded { .. } => {
            }
            RunEvent::Timing { timeline, .. } => {
                if self.timing {
                    self.err(&format!("⏱ {}", timeline.summary()));
                }
            }
            RunEvent::Error { index, line, error } => {
                self.errors += 1;
                // 공통 분류 + 코드 부각(docs/42) — `ERROR line n: [ORA-00942 · Table not found: X] 원문`.
                let stmt = self.stmts.get(index).map(String::as_str).unwrap_or("");
                let c = nsql_core::classify(self.dialect, error.code, &error.message, stmt);
                let head = err_head(&c);
                if head.is_empty() {
                    self.err(&format!("ERROR line {line}: {error}"));
                } else {
                    self.err(&format!("ERROR line {line}: [{head}] {}", error.message));
                }
            }
        }
        self.out(&out);
    }
}

/// 실행 중 서버 메시지(PRINT · RAISE NOTICE)를 도착 즉시 stdout에(사용자 09-15 "실행 중 로그") — 스풀에도(드라이버 스레드에서 불린다).
fn stdout_sink(spool: SpoolHandle) -> nsql_core::MessageSink {
    std::sync::Arc::new(move |m: String| {
        let line = format!("{m}\n");
        let out = io::stdout();
        let mut out = out.lock();
        let _ = out.write_all(line.as_bytes());
        let _ = out.flush();
        drop(out);
        spool_write(&spool, line.as_bytes());
    })
}

fn connect_or_exit(
    runner: &mut Runner,
    target: &str,
    dialect: Dialect,
    printer: &mut Printer,
    no_prompt: bool,
) {
    let mut spec = resolve_target(target, dialect).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    // 비밀번호 없는 접속 문자열/프로필 = env/프롬프트(sqlplus/psql 관례 · 사용자 09-16).
    term::ensure_password(&mut spec, no_prompt, target);
    // 접속 실패는 `Connected`보다 먼저 온다 → 오류 코드 표기는 **대상의 방언**으로(SQLite 실패가 `[ORA-00014]`로 보이던 결함 · T-148).
    printer.dialect = spec.dialect.unwrap_or(dialect);
    // `--timing`이면 접속 단계도(09-24 T-190: 실행 110 ms인데 전체 2 s → 접속이 어디서 걸리는지).
    let t0 = std::time::Instant::now();
    let ok = runner.connect(&spec, &mut |e| printer.handle(e));
    if printer.timing {
        printer.err(&format!(
            "⏱ connect {:.1}ms",
            t0.elapsed().as_secs_f64() * 1000.0
        ));
    }
    if !ok {
        std::process::exit(1);
    }
}

/// `-v 이름=값` → (이름, 값). 이름 = 영숫자·`_`(비면 안 된다) · 값은 `=` 뒤 전부(빈 값 · `=`가 든 값 허용).
fn parse_define(arg: &str) -> Option<(String, String)> {
    let (n, v) = arg.split_once('=')?;
    let n = n.trim();
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        .then(|| (n.to_string(), v.to_string()))
}

fn prompt_stdin(name: &str) -> Option<String> {
    // 이름(영숫자·`_`)이면 틀에 넣고, `ACCEPT … PROMPT 글`처럼 글이 오면 그 글 그대로 묻는다(T-153).
    if name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        eprint!("Enter value for {name}: ");
    } else {
        eprint!("{name}");
    }
    let _ = io::stderr().flush();
    let mut s = String::new();
    io::stdin().lock().read_line(&mut s).ok()?;
    Some(s.trim_end_matches(['\r', '\n']).to_string())
}

fn cmd_run(o: &Opts) -> i32 {
    let Some(target) = &o.target else {
        eprintln!("-c <target>가 필요합니다");
        return 2;
    };
    let Some(path) = o.positional.first() else {
        eprintln!("스크립트 경로가 필요합니다(- = stdin)");
        return 2;
    };
    let src = read_source(path);
    let mut printer = Printer::new(o, o.format.clone(), true);
    // run = 무제한(D-68 · `--max-rows`면 그 값) · 커서 유지 없음(배치는 이어 받을 일이 없다).
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_max_rows(o.max_rows)
        .with_fetch_size(fetch_settings().0)
        .with_keep_cursor(false)
        .with_message_sink(stdout_sink(printer.spool.clone()))
        .with_resolver(resolver())
        .with_spool(printer.spool.clone())
        .with_strict(strict_setting())
        .with_auto_cursor(cursor_autoshow_setting())
        .with_into_first(into_first_setting())
        .with_var_limits(var_limits_setting().0, var_limits_setting().1);
    apply_load_switches(&mut runner);
    // 내장 변수 층(`${workspaceFolder}`는 프로젝트 없음 → 없음 · `${file}` = 스크립트 · `${cwd}` · `${nsqlHome}` · `${config:키}` · 사용자 09-23).
    runner.engine.settings.intrinsic = std::sync::Arc::new(intrinsic_vars(path, o.dialect));
    connect_or_exit(&mut runner, target, o.dialect, &mut printer, o.no_prompt);
    runner.engine.set_args(&o.positional[1..]);
    for (n, v) in &o.defines {
        runner.engine.define(n, v);
    }
    let no_prompt = o.no_prompt || path == "-";
    let mut prompt = |name: &str| if no_prompt { None } else { prompt_stdin(name) };
    printer.set_source(&src);
    // 파일이면 그 폴더가 `@@`의 기준(T-9) · stdin이면 cwd.
    let script_path = (path != "-").then(|| Path::new(path.as_str()));
    let errs = runner.run_script_in(&src, script_path, &mut prompt, &mut |e| printer.handle(e));
    printer.flush_sql(runner.session.as_deref_mut());
    runner.commit_at_exit();
    close_spool(&printer.spool);
    if errs > 0 {
        1
    } else {
        0
    }
}

/// 실행 끝 — 열린 스풀을 닫는다(플러시 · SQL*Plus EXIT과 동일).
fn close_spool(spool: &SpoolHandle) {
    if let Ok(mut sp) = spool.lock() {
        sp.close();
    }
}

fn cmd_shell(o: &Opts) -> i32 {
    let Some(target) = &o.target else {
        eprintln!("-c <target>가 필요합니다");
        return 2;
    };
    let mut printer = Printer::new(o, o.format.clone(), true);
    // 셸만 상한(D-68): 설정 `cli.max_rows`(200) · `--max-rows`가 있으면 그 값 · 세션 중 `set max_rows`.
    printer.shell = true;
    let st = nsql_settings::Settings::open_default().ok();
    printer.max_rows = if o.max_rows > 0 {
        o.max_rows
    } else {
        st.as_ref()
            .map_or(200, |s| s.int("cli.max_rows").max(0) as usize)
    };
    printer.auto_more = st.as_ref().is_some_and(|s| s.flag("cli.auto_more"));
    drop(st);
    let (fetch_size, keep_cursor, idle) = fetch_settings();
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_max_rows(printer.shell_limit())
        .with_fetch_size(fetch_size)
        .with_keep_cursor(keep_cursor)
        .with_message_sink(stdout_sink(printer.spool.clone()))
        .with_resolver(resolver())
        .with_spool(printer.spool.clone())
        .with_strict(strict_setting())
        .with_auto_cursor(cursor_autoshow_setting())
        .with_into_first(into_first_setting())
        .with_var_limits(var_limits_setting().0, var_limits_setting().1);
    apply_load_switches(&mut runner);
    runner.cursor_idle_secs = idle;
    connect_or_exit(&mut runner, target, o.dialect, &mut printer, o.no_prompt);
    for (n, v) in &o.defines {
        runner.engine.define(n, v);
    }
    eprintln!("{}", t(Msg::CliShellBanner));
    let stdin = io::stdin();
    let mut buf = String::new();
    loop {
        eprint!("{}", if buf.is_empty() { "nsql> " } else { "   -> " });
        let _ = io::stderr().flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let t = line.trim_end_matches(['\r', '\n']);
        if buf.is_empty()
            && matches!(
                t.trim().to_ascii_lowercase().as_str(),
                "exit" | "quit" | "\\q"
            )
        {
            break;
        }
        if buf.is_empty() {
            if let Some(cmd) = fetch_cmd(t) {
                run_fetch_cmd(cmd, &mut printer, &mut runner);
                continue;
            }
        }
        if buf.is_empty() && shell_set(t, &mut printer, runner.session.as_deref_mut()) {
            continue;
        }
        // 다른 CLI 별칭(T-52): `\d` `.tables` `\i` … → SQL*Plus식 한 줄로 바꿔 엔진에.
        let rewritten;
        let t = if buf.is_empty() {
            match shell_alias(t) {
                Some(ShellAlias::Help) => {
                    print_shell_help();
                    continue;
                }
                Some(ShellAlias::Rewrite(r)) => {
                    rewritten = r;
                    rewritten.as_str()
                }
                None => t,
            }
        } else {
            t
        };
        buf.push_str(t);
        buf.push('\n');
        let first = buf.lines().next().unwrap_or("").trim();
        let complete = t.trim() == "/"
            || t.trim().eq_ignore_ascii_case("GO")
            || t.trim_end().ends_with(';')
            || (nsql_script::command::is_command_start(first)
                && !first.to_ascii_uppercase().starts_with("EXEC")
                || buf.lines().count() == 1
                    && nsql_script::command::is_command_start(first)
                    && !t.trim_end().is_empty()
                    && first != "EXEC");
        if !complete {
            continue;
        }
        let src = std::mem::take(&mut buf);
        let mut prompt = prompt_stdin;
        printer.set_source(&src);
        runner.set_max_rows(printer.shell_limit());
        runner.run_script(&src, &mut prompt, &mut |e| printer.handle(e));
        printer.flush_sql(runner.session.as_deref_mut());
        // `cli.auto_more`: psql FETCH_COUNT처럼 묻지 않고 세그먼트를 이어 출력(스트리밍).
        while printer.auto_more && printer.more {
            run_fetch_cmd(FetchCmd::More(None), &mut printer, &mut runner);
        }
    }
    runner.close_cursor();
    runner.commit_at_exit();
    close_spool(&printer.spool);
    0
}

/// `nsql explain -c <target> -q <sql>` — 방언별 실행 계획(GUI Explain과 같은 `explain_script`).
fn cmd_explain(o: &Opts) -> i32 {
    let Some(target) = &o.target else {
        eprintln!("-c <target>가 필요합니다");
        return 2;
    };
    let sql = match (&o.query, o.positional.first()) {
        (Some(q), _) => q.clone(),
        (None, Some(p)) => read_source(p),
        (None, None) => {
            eprintln!("-q <sql> 또는 <file>이 필요합니다");
            return 2;
        }
    };
    let mut printer = Printer::new(o, o.format.clone(), false);
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_max_rows(o.max_rows)
        .with_keep_cursor(false)
        .with_message_sink(stdout_sink(printer.spool.clone()))
        .with_resolver(resolver());
    connect_or_exit(&mut runner, target, o.dialect, &mut printer, o.no_prompt);
    let src = nsql_script::explain_script(runner.engine.dialect, &sql);
    let mut prompt = |_: &str| Some(String::new());
    let errs = runner.run_script(&src, &mut prompt, &mut |e| printer.handle(e));
    if errs > 0 {
        1
    } else {
        0
    }
}

fn cmd_export(o: &Opts) -> i32 {
    let Some(target) = &o.target else {
        eprintln!("-c <target>가 필요합니다");
        return 2;
    };
    let sql = match (&o.query, &o.table) {
        (Some(q), _) => q.clone(),
        (None, Some(t)) => format!("SELECT * FROM {t}"),
        (None, None) => {
            eprintln!("{}", nsql_i18n::t(nsql_i18n::Msg::CliErrNeedQuery));
            return 2;
        }
    };
    let format = match (&o.format, &o.table) {
        (Format::Grid, _) => Format::Csv,
        (Format::Insert { table }, Some(t)) if table == "T" => Format::Insert { table: t.clone() },
        (f, _) => f.clone(),
    };
    let mut printer = Printer::new(o, Format::Grid, false);
    // export = 무제한(D-68) · 커서 유지 없음 · 왕복당 행수는 설정(`db.fetch_size`).
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_max_rows(o.max_rows)
        .with_fetch_size(fetch_settings().0)
        .with_keep_cursor(false)
        .with_message_sink(stdout_sink(printer.spool.clone()))
        .with_resolver(resolver());
    connect_or_exit(&mut runner, target, o.dialect, &mut printer, o.no_prompt);
    let dialect = runner.engine.dialect;
    let mut out: Box<dyn Write> = match &o.out {
        Some(p) => Box::new(std::fs::File::create(p).unwrap_or_else(|e| {
            eprintln!("{p}: {e}");
            std::process::exit(1)
        })),
        None => Box::new(io::stdout()),
    };
    let items = split_script(&format!("{sql};"));
    let mut errors = 0;
    let mut rows = 0usize;
    let mut no_prompt = |_: &str| None;
    // `-f sql:*`는 실행 뒤 세션으로 키(PK/유니크)를 조회해 쓴다(run과 같은 규칙 · docs/41).
    let mut deferred: Vec<nsql_core::ResultSet> = Vec::new();
    let sql_kind = match &format {
        Format::Sql(k) => Some(*k),
        _ => None,
    };
    for (i, item) in items.iter().enumerate() {
        runner.run_item(i, item, &mut no_prompt, &mut |e| match e {
            RunEvent::ResultSet { rs, .. } => {
                rows += rs.rows.len();
                if sql_kind.is_some() {
                    deferred.push(rs);
                } else if let Err(e) = nsql_io::write_result_set(&mut out, &rs, &format, dialect) {
                    eprintln!("쓰기 실패: {e}");
                    errors += 1;
                }
            }
            RunEvent::Error { error, .. } => {
                eprintln!("ERROR: {error}");
                errors += 1;
            }
            _ => {}
        });
    }
    if let Some(kind) = sql_kind {
        let key_mode = key_mode_setting();
        for rs in deferred {
            let (text, warn) = sql_statements(
                runner.session.as_deref_mut(),
                dialect,
                key_mode,
                &sql,
                &rs,
                kind,
            );
            if let Err(e) = out.write_all(text.as_bytes()) {
                eprintln!("쓰기 실패: {e}");
                errors += 1;
            }
            for w in warn {
                let _ = writeln!(out, "-- {w}");
            }
        }
    }
    let _ = out.flush();
    if o.out.is_some() {
        eprintln!("{rows} rows → {}", o.out.as_deref().unwrap_or("-"));
    }
    if errors > 0 {
        1
    } else {
        0
    }
}

fn main() {
    // 언어는 설정 파일(ui.lang · 기본 영어)에서 — 폴더를 모르면 기본값(T-37).
    if let Ok(s) = nsql_settings::Settings::open_default() {
        nsql_i18n::set_lang(s.lang());
    }
    let o = parse_opts();
    // `--password-stdin`(T-132 ③): 표준 입력의 첫 줄 = 비밀번호. 스크립트를 표준 입력(`-`)에서 읽는 실행과는 함께 쓸 수 없다.
    if o.password_stdin
        && (o.positional.first().is_some_and(|p| p == "-") || !term::read_password_from_stdin())
    {
        eprintln!("{}", nsql_i18n::t(nsql_i18n::Msg::CliErrPasswordStdin));
        std::process::exit(2);
    }
    let code = match o.cmd.as_str() {
        "plan" => {
            let Some(path) = o.positional.first() else {
                eprintln!("스크립트 경로가 필요합니다(- = stdin)");
                std::process::exit(2)
            };
            let src = read_source(path);
            plan::run_plan(o.dialect, &src, &o.positional[1..])
        }
        "run" => cmd_run(&o),
        "shell" => cmd_shell(&o),
        "export" => cmd_export(&o),
        "explain" => cmd_explain(&o),
        "conn" => conn::cmd_conn(&o),
        "bookmark" | "bm" => bookmark::cmd_bookmark(&o),
        "cat" | "catalog" | "obj" => cat::cmd_cat(&o),
        "config" | "settings" => config::cmd_config(&o),
        "grep" => grep::cmd_grep(&o.positional),
        other => {
            eprintln!("알 수 없는 명령: {other}\n");
            usage()
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    /// T-153 — `-v 이름=값`: 값의 `=`·빈 값 허용 · 이름이 비었거나 글자가 이상하면 거부 · `=`가 없으면 거부.
    #[test]
    fn define_option_parses() {
        let f = super::parse_define;
        assert_eq!(f("dept=10"), Some(("dept".into(), "10".into())));
        assert_eq!(f("w=a=b c"), Some(("w".into(), "a=b c".into())));
        assert_eq!(f("e="), Some(("e".into(), String::new())));
        assert_eq!(f("=x"), None);
        assert_eq!(f("novalue"), None);
        assert_eq!(f("bad name=1"), None);
    }

    use super::*;

    /// T-52 별칭 표 — psql · sqlite3 · sqlcmd 어휘가 SQL*Plus식 한 줄로.
    /// T-48d: 셸 페치 명령 파싱 — `\more [n]`·`more`·`\all`·`\count`·`\pager` · SQL과 헷갈리지 않는다.
    #[test]
    fn fetch_commands_parse() {
        assert_eq!(fetch_cmd("\\more"), Some(FetchCmd::More(None)));
        assert_eq!(fetch_cmd("more 50;"), Some(FetchCmd::More(Some(50))));
        assert_eq!(fetch_cmd("\\more x"), None);
        assert_eq!(fetch_cmd("\\all"), Some(FetchCmd::All));
        assert_eq!(fetch_cmd("\\count"), Some(FetchCmd::Count));
        assert_eq!(fetch_cmd("\\pager"), Some(FetchCmd::Pager(None)));
        assert_eq!(fetch_cmd("\\pager ON"), Some(FetchCmd::Pager(Some(true))));
        assert_eq!(fetch_cmd("\\pager maybe"), None);
        assert_eq!(fetch_cmd("\\all x"), None);
        assert_eq!(fetch_cmd("SELECT more FROM t;"), None);
        assert_eq!(fetch_cmd("COUNT"), None);
    }

    #[test]
    fn shell_aliases_map_onto_sqlplus_commands() {
        let rw = |l: &str| match shell_alias(l) {
            Some(ShellAlias::Rewrite(s)) => s,
            other => panic!("{l}: {other:?}"),
        };
        assert_eq!(rw("\\d"), "SHOW TABLES");
        assert_eq!(rw("\\dt;"), "SHOW TABLES");
        assert_eq!(rw(".tables"), "SHOW TABLES");
        assert_eq!(rw("\\dv"), "SHOW VIEWS");
        assert_eq!(rw("\\d emp"), "DESCRIBE emp");
        assert_eq!(rw(".schema hr.emp"), "DESCRIBE hr.emp");
        assert_eq!(rw("\\i init.sql"), "@init.sql");
        assert_eq!(rw(".read sub/x.sql 1 2"), "@sub/x.sql 1 2");
        assert_eq!(rw("\\c prod"), "CONNECT prod");
        assert_eq!(shell_alias("\\?"), Some(ShellAlias::Help));
        assert_eq!(shell_alias("help"), Some(ShellAlias::Help));
        // 엔진이 아는 것·SQL은 건드리지 않는다.
        assert_eq!(shell_alias("help set"), None);
        assert_eq!(shell_alias("SHOW TABLES"), None);
        assert_eq!(shell_alias("DESC emp"), None);
        assert_eq!(shell_alias(":r a.sql"), None);
        assert_eq!(shell_alias("SELECT 1;"), None);
        assert_eq!(shell_alias("\\i"), None, "파일 없는 \\i는 별칭이 아니다");
        // 바뀐 줄은 전부 명령 시작이라 셸이 즉시 실행한다.
        for l in ["\\d", "\\d emp", "\\i x.sql", "\\c prod"] {
            let ShellAlias::Rewrite(r) = shell_alias(l).expect("alias") else {
                panic!()
            };
            assert!(nsql_script::command::is_command_start(&r), "{r}");
        }
    }
}
