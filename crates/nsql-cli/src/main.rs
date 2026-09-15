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
mod plan;
mod term;

use nsql_core::{DbError, Dialect, Session};
use nsql_io::Format;
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
    positional: Vec<String>,
}

fn usage() -> ! {
    eprintln!(
        "nsql — Nexa SQL 명령줄\n\n  nsql plan   [-d dialect] <script|-> [args]\n  nsql run    -c <target> [-d dialect] [-f grid|csv|tsv|json|jsonl] [--no-prompt] [--timing] [--log] <script|-> [args]\n  nsql shell  -c <target> [-d dialect]\n  nsql export -c <target> (-q <sql> | -t <table>) [-f fmt] [-o file]\n  nsql conn   list | add <name> [<target>] [--host h --port n --db d --user u -d dialect -p pw] | show <name> | rm <name> | test [<name>] | path\n  nsql cat    -c <target> [-s schema] [-f fmt] schemas | kinds | <kind> | columns <object> | source <kind> <name> | errors <name>\n              kind: tables views mviews procs funcs packages bodies sequences triggers indexes synonyms types\n\n  target: 프로필 이름(nsql conn) · sqlite::memory: · sqlite:file.db · oracle://u:p@h:1521/svc · mssql://u:p@h:1433/db · postgres://u:p@h:5432/db · u/p@h:1521/svc\n  이 빌드의 드라이버: {}",
        nsql_drivers::available().iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ")
    );
    std::process::exit(2);
}

fn parse_opts() -> Opts {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else { usage() };
    if cmd == "-h" || cmd == "--help" {
        usage();
    }
    if cmd == "--version" || cmd == "-V" {
        println!("nsql {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
    let mut o = Opts {
        cmd,
        target: None,
        dialect: Dialect::Oracle,
        format: Format::Grid,
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
            RunEvent::ResultSet { rs, elapsed, .. } => {
                let _ = nsql_io::write_result_set(&mut out, &rs, &self.format, self.dialect);
                if self.feedback && self.format == Format::Grid {
                    let _ = writeln!(
                        out,
                        "\n{} rows ({:.3}s)\n",
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

fn connect_or_exit(runner: &mut Runner, target: &str, dialect: Dialect, printer: &mut Printer) {
    let spec = resolve_target(target, dialect).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
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
        dialect: o.dialect,
        errors: 0,
        feedback: true,
        timing: o.timing,
        log: log_hub(o),
    };
    let mut runner = Runner::new(o.dialect, opener(o.dialect)).with_resolver(resolver());
    connect_or_exit(&mut runner, target, o.dialect, &mut printer);
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
        dialect: o.dialect,
        errors: 0,
        feedback: true,
        timing: o.timing,
        log: log_hub(o),
    };
    let mut runner = Runner::new(o.dialect, opener(o.dialect)).with_resolver(resolver());
    connect_or_exit(&mut runner, target, o.dialect, &mut printer);
    eprintln!("nsql shell — `;`·단독 `/`·명령 줄로 실행 · exit 종료 · 변수는 세션 동안 유지");
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
        dialect: o.dialect,
        errors: 0,
        feedback: false,
        timing: o.timing,
        log: log_hub(o),
    };
    let mut runner = Runner::new(o.dialect, opener(o.dialect)).with_resolver(resolver());
    connect_or_exit(&mut runner, target, o.dialect, &mut printer);
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
