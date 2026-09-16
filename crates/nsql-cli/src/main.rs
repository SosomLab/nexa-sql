//! `nsql` — 명령줄 도구(docs/11). GUI와 같은 코어(`nsql-run`·`nsql-drivers`·`nsql-io`)를 쓴다.
//!
//! ```text
//! nsql plan   [-d dialect] <script|-> [args...]                 # 실행 없이 계획 출력(DB 불요)
//! nsql run    -c <target> [-d dialect] [-f grid|csv|tsv|json|jsonl] [--no-prompt] <script|-> [args...]
//! nsql shell  -c <target> [-d dialect]                          # 대화형(줄 단위 · `;`/`/`/명령으로 실행 · exit)
//! nsql export -c <target> (-q <sql> | -t <table>) [-f csv|tsv|json|jsonl|insert[:T]] [-o file]
//! nsql conn   list | add <name> [<target>] [--host h --port n --db d --user u -d dialect -p pw] | show <name> | rm <name> | test [<name>] | path
//! nsql config list | get <key> | set <key> <value> | reset <key> | path     # 앱 설정(ui.lang · ui.theme …) — GUI와 공유
//! target: 프로필 이름 · sqlite::memory: · sqlite:file.db · oracle://u:p@h:1521/svc · mssql://u:p@h:1433/db · postgres://u:p@h:5432/db · u/p@h:1521/svc(-d로 방언)
//! ```

mod cat;
mod config;
mod conn;
mod help;
mod plan;
mod term;

use nsql_core::{DbError, Dialect, Session};
use nsql_io::{Format, GridOpts, Overflow};
use nsql_run::{Opener, RunEvent, Runner};
use nsql_script::{split_script, ConnectSpec};
use nsql_vault::Vault;
use std::io::{self, BufRead, Read, Write};

struct Opts {
    cmd: String,
    target: Option<String>,
    dialect: Dialect,
    format: Format,
    no_prompt: bool,
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
                    eprintln!("알 수 없는 방언: {v}");
                    std::process::exit(2)
                });
            }
            "-f" | "--format" => {
                let v = val("-f");
                o.format = Format::parse(&v).unwrap_or_else(|| {
                    eprintln!("알 수 없는 형식: {v}");
                    std::process::exit(2)
                });
            }
            "-p" | "--password" => o.password = Some(val("-p")),
            "--host" => o.host = Some(val("--host")),
            "--port" => {
                let v = val("--port");
                o.port = Some(v.parse().unwrap_or_else(|_| {
                    eprintln!("--port: 숫자가 아닙니다: {v}");
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
                    eprintln!("--max-rows: 숫자가 아닙니다: {v}");
                    std::process::exit(2)
                });
            }
            "--width" | "--line-width" | "--linesize" => {
                let v = val("--width");
                o.width = Some(v.parse().unwrap_or_else(|_| {
                    eprintln!("--width: 숫자가 아닙니다: {v}");
                    std::process::exit(2)
                }));
            }
            "--max-col-width" | "--colwidth" => {
                let v = val("--max-col-width");
                o.max_col = Some(v.parse().unwrap_or_else(|_| {
                    eprintln!("--max-col-width: 숫자가 아닙니다: {v}");
                    std::process::exit(2)
                }));
            }
            "--overflow" => {
                let v = val("--overflow");
                o.overflow = Some(Overflow::parse(&v).unwrap_or_else(|| {
                    eprintln!("--overflow: wrap | truncate | expanded | none 중 하나: {v}");
                    std::process::exit(2)
                }));
            }
            "-x" | "--expanded" => o.overflow = Some(Overflow::Expanded),
            "--no-prompt" => o.no_prompt = true,
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
            eprintln!("stdin 읽기 실패: {e}");
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

fn opener(default_dialect: Dialect) -> Opener {
    Box::new(
        move |spec: &ConnectSpec| -> Result<Box<dyn Session>, DbError> {
            nsql_drivers::open(spec, default_dialect)
        },
    )
}

/// 스크립트 `CONNECT <프로필>` 해석기 — 저장소는 호출마다 연다(다른 인스턴스가 방금 저장한 프로필도 보인다).
fn resolver() -> nsql_run::Resolver {
    Box::new(|name: &str| {
        let v = Vault::open_default().map_err(|e| e.to_string())?;
        v.resolve(name).map_err(|e| e.to_string())
    })
}

/// 표 폭 옵션 — 설정 `cli.width`/`cli.max_col_width`/`cli.overflow`(`nsql config set …`) 위에 플래그가 덮는다.
/// 줄 폭 0 = 터미널이면 콘솔 폭 · 파이프/파일이면 무제한.
fn grid_opts(o: &Opts) -> GridOpts {
    let mut g = GridOpts {
        max_col: 60,
        line_width: 0,
        overflow: Overflow::None,
    };
    if let Ok(s) = nsql_settings::Settings::open_default() {
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
fn shell_set(line: &str, p: &mut Printer) -> bool {
    let dialect = p.dialect;
    if let Some(rest) = line.trim().to_ascii_lowercase().strip_prefix("copy") {
        let rest = rest.trim().trim_end_matches(';');
        if rest.is_empty() || Format::parse(rest).is_some() {
            let fmt = Format::parse(rest).unwrap_or_else(|| p.format.clone());
            match &p.last {
                None => eprintln!("{}", nsql_i18n::t(nsql_i18n::Msg::CliCopyNoResult)),
                Some(rs) => {
                    let text = nsql_io::render_result_set(rs, &fmt, dialect, &p.grid);
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
    let show = |g: &GridOpts| {
        eprintln!(
            "width {} (0 = 무제한 · set width auto = 터미널 폭) · colwidth {} · overflow {} · format {}",
            g.line_width,
            g.max_col,
            g.overflow.name(),
            fmt_name
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
        eprintln!("set width <n|auto> · set colwidth <n> · set overflow none|wrap|truncate|expanded · set format grid|markdown|csv|tsv|json|jsonl · get <키> · \\x · copy [fmt] · 영구: nsql config set cli.*");
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
        "overflow" | "wrap" => match Overflow::parse(v) {
            Some(x) => g.overflow = x,
            None => eprintln!("set overflow none|wrap|truncate|expanded"),
        },
        "format" | "fmt" => match Format::parse(v) {
            Some(f) => {
                let g = *g;
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
            None => eprintln!("set format grid|markdown|csv|tsv|json|jsonl"),
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

/// 이벤트 → 터미널 출력. 반환 = 오류 수.
struct Printer {
    format: Format,
    /// 표 폭·넘침(설정 `cli.*` → 플래그 → 셸 `set`).
    grid: GridOpts,
    /// 마지막 결과(셸 `copy` — 터미널이 접은 줄이 아니라 원문을 클립보드로 · 09-16).
    last: Option<nsql_core::ResultSet>,
    dialect: Dialect,
    errors: usize,
    feedback: bool,
    /// 단계별 소요 출력(`--timing`).
    timing: bool,
    /// 실행 로그 허브(`--log` · 별도 스레드 stderr 싱크 · 없으면 None).
    log: Option<nsql_log::LogHub>,
}

/// `--log`면 설정 `log.format`으로 stderr 싱크를 **로그 스레드**에 띄운다(메인 경로 비차단 · docs/26 §3).
fn log_hub(o: &Opts) -> Option<nsql_log::LogHub> {
    if !o.log {
        return None;
    }
    let name = nsql_settings::Settings::open_default()
        .map(|s| s.get("log.format").unwrap_or("raw").to_string())
        .unwrap_or_else(|_| "raw".into());
    let fmt: Box<dyn nsql_log::LogFormat + Send> = match name.as_str() {
        "markdown" => Box::new(nsql_log::MarkdownFormat),
        "grid" => Box::new(nsql_log::GridFormat),
        _ => Box::new(nsql_log::RawFormat),
    };
    Some(nsql_log::LogHub::spawn(
        vec![Box::new(nsql_log::StderrSink::new(fmt))],
        4096,
    ))
}

impl Printer {
    fn handle(&mut self, ev: RunEvent) {
        if let Some(hub) = &self.log {
            for e in nsql_run::log_entries(&ev) {
                hub.push(e);
            }
        }
        let out = io::stdout();
        let mut out = out.lock();
        match ev {
            RunEvent::Begin { .. } => {}
            RunEvent::ResultSet {
                rs, elapsed, more, ..
            } => {
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
                if self.feedback && matches!(self.format, Format::Grid | Format::Markdown) {
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
            RunEvent::Print { pairs } => {
                for (n, v) in pairs {
                    let _ = writeln!(out, "{n} = {}", v.display());
                }
            }
            RunEvent::Message(m) => {
                let _ = writeln!(out, "{m}");
            }
            RunEvent::Connected {
                description,
                dialect,
            } => {
                self.dialect = dialect;
                let _ = writeln!(out, "Connected: {description} ({dialect})");
            }
            RunEvent::Disconnected => {
                let _ = writeln!(out, "Disconnected");
            }
            RunEvent::Timing { timeline, .. } => {
                if self.timing {
                    eprintln!("⏱ {}", timeline.summary());
                }
            }
            RunEvent::Error { line, error, .. } => {
                self.errors += 1;
                let _ = out.flush();
                eprintln!("ERROR line {line}: {error}");
            }
        }
    }
}

/// 실행 중 서버 메시지(PRINT · RAISE NOTICE)를 도착 즉시 stdout에(사용자 09-15 "실행 중 로그").
fn stdout_sink() -> nsql_core::MessageSink {
    std::sync::Arc::new(|m: String| {
        let out = io::stdout();
        let mut out = out.lock();
        let _ = writeln!(out, "{m}");
        let _ = out.flush();
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
    let ok = runner.connect(&spec, &mut |e| printer.handle(e));
    if !ok {
        std::process::exit(1);
    }
}

fn prompt_stdin(name: &str) -> Option<String> {
    eprint!("Enter value for {name}: ");
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
    let mut printer = Printer {
        format: o.format.clone(),
        grid: grid_opts(o),
        last: None,
        dialect: o.dialect,
        errors: 0,
        feedback: true,
        timing: o.timing,
        log: log_hub(o),
    };
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_max_rows(o.max_rows)
        .with_message_sink(stdout_sink())
        .with_resolver(resolver());
    connect_or_exit(&mut runner, target, o.dialect, &mut printer, o.no_prompt);
    runner.engine.set_args(&o.positional[1..]);
    let no_prompt = o.no_prompt || path == "-";
    let mut prompt = |name: &str| if no_prompt { None } else { prompt_stdin(name) };
    let errs = runner.run_script(&src, &mut prompt, &mut |e| printer.handle(e));
    if let Some(s) = runner.session.as_mut() {
        let _ = s.commit();
    }
    if errs > 0 {
        1
    } else {
        0
    }
}

fn cmd_shell(o: &Opts) -> i32 {
    let Some(target) = &o.target else {
        eprintln!("-c <target>가 필요합니다");
        return 2;
    };
    let mut printer = Printer {
        format: o.format.clone(),
        grid: grid_opts(o),
        last: None,
        dialect: o.dialect,
        errors: 0,
        feedback: true,
        timing: o.timing,
        log: log_hub(o),
    };
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_max_rows(o.max_rows)
        .with_message_sink(stdout_sink())
        .with_resolver(resolver());
    connect_or_exit(&mut runner, target, o.dialect, &mut printer, o.no_prompt);
    eprintln!("nsql shell — `;`·단독 `/`·명령 줄로 실행 · exit 종료 · 변수는 세션 동안 유지 · 표 폭: set width <n|auto> · set colwidth <n> · set overflow wrap|truncate|expanded|none · \\x · show");
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
        if buf.is_empty() && shell_set(t, &mut printer) {
            continue;
        }
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
        runner.run_script(&src, &mut prompt, &mut |e| printer.handle(e));
    }
    if let Some(s) = runner.session.as_mut() {
        let _ = s.commit();
    }
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
    let mut printer = Printer {
        format: o.format.clone(),
        grid: grid_opts(o),
        last: None,
        dialect: o.dialect,
        errors: 0,
        feedback: false,
        timing: o.timing,
        log: log_hub(o),
    };
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_max_rows(o.max_rows)
        .with_message_sink(stdout_sink())
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
            eprintln!("-q <sql> 또는 -t <table>이 필요합니다");
            return 2;
        }
    };
    let format = match (&o.format, &o.table) {
        (Format::Grid, _) => Format::Csv,
        (Format::Insert { table }, Some(t)) if table == "T" => Format::Insert { table: t.clone() },
        (f, _) => f.clone(),
    };
    let mut printer = Printer {
        format: Format::Grid,
        grid: grid_opts(o),
        last: None,
        dialect: o.dialect,
        errors: 0,
        feedback: false,
        timing: o.timing,
        log: log_hub(o),
    };
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_max_rows(o.max_rows)
        .with_message_sink(stdout_sink())
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
    for (i, item) in items.iter().enumerate() {
        runner.run_item(i, item, &mut no_prompt, &mut |e| match e {
            RunEvent::ResultSet { rs, .. } => {
                rows += rs.rows.len();
                if let Err(e) = nsql_io::write_result_set(&mut out, &rs, &format, dialect) {
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
        "cat" | "catalog" | "obj" => cat::cmd_cat(&o),
        "config" | "settings" => config::cmd_config(&o),
        other => {
            eprintln!("알 수 없는 명령: {other}\n");
            usage()
        }
    };
    std::process::exit(code);
}
