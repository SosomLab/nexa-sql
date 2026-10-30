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
mod complete;
mod config;
mod conn;
mod driver;
mod grep;
mod help;
mod license;
mod lineedit;
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
    /// `explain --analyze` = 실제 실행 계획(문장을 실행) · `--raw` = DBMS 원문 그대로(docs/122).
    analyze: bool,
    raw: bool,
    /// 비밀번호를 표준 입력의 첫 줄에서 읽는다(T-132 ③).
    password_stdin: bool,
    /// `-v 이름=값`(여러 번) — 스크립트가 돌기 전에 치환 변수(`&이름`)를 정의한다(T-153 · `DEFINE`과 같은 저장소).
    defines: Vec<(String, String)>,
    /// `--timing` — 항목마다 단계별 소요(docs/26)를 stderr에.
    /// `import`: 헤더 없음 · 열 매핑(`src=dst,…`) · 열 이름(헤더 없을 때) · 배치/커밋 간격/경로/빈 값(설정 `bulk.*` 위에 덮음).
    no_header: bool,
    map: Option<String>,
    cols: Option<String>,
    batch: Option<usize>,
    commit_every: Option<usize>,
    mode: Option<String>,
    empty_null: Option<bool>,
    /// `export --fast`: 서버 형식화 추출(PG `COPY TO`) — 안 되면 일반 경로.
    fast: bool,
    /// `nsql format --in-place`(T-255).
    in_place: bool,
    /// `--timing` — 항목마다 단계별 소요(docs/26)를 stderr에.
    timing: bool,
    /// `--timing=json` — 같은 내용을 줄마다 JSON 객체로(T-47 · 스크립트·CI가 파싱 · 10-29).
    timing_json: bool,
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
    let default_format = settings_cached()
        .ok()
        .and_then(|s| s.get("cli.format").and_then(Format::parse))
        .unwrap_or(Format::Grid);
    let mut o = Opts {
        cmd,
        target: None,
        dialect: Dialect::Oracle,
        format: default_format,
        no_prompt: false,
        analyze: false,
        raw: false,
        password_stdin: false,
        defines: Vec::new(),
        no_header: false,
        map: None,
        cols: None,
        batch: None,
        commit_every: None,
        mode: None,
        empty_null: None,
        fast: false,
        in_place: false,
        timing: false,
        timing_json: false,
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
                    eprintln!("{}", tf(Msg::CliErrMissingValue, &[name]));
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
            "--no-header" => o.no_header = true,
            "--map" => o.map = Some(val("--map")),
            "--cols" => o.cols = Some(val("--cols")),
            "--batch" => o.batch = val("--batch").parse().ok(),
            "--commit-every" => o.commit_every = val("--commit-every").parse().ok(),
            "--mode" => o.mode = Some(val("--mode")),
            "--empty-null" => o.empty_null = Some(val("--empty-null") != "off"),
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
            "--analyze" => o.analyze = true,
            "--raw" => o.raw = true,
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
            "--timing=json" => {
                o.timing = true;
                o.timing_json = true;
            }
            "--fast" => o.fast = true,
            "--in-place" => o.in_place = true,
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
    // ★ 접속을 여는 모든 명령(run·shell·explain·import·export·cat·conn test)이 같은 드라이버 설정·자동 설치 정책을 탄다 —
    //   🔧 10-23 사용자 실기: explain/import/export/cat은 `apply_load_switches`를 안 거쳐 자동 설치가 꺼진 것처럼 -1350.
    apply_driver_switches();
    Box::new(
        move |spec: &ConnectSpec| -> Result<Box<dyn Session>, DbError> {
            if !password_missing(spec, default_dialect) {
                return nsql_drivers::open_auto(spec, default_dialect, &mut driver_progress);
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
            let r = nsql_drivers::open_auto(&filled, default_dialect, &mut driver_progress);
            nsql_core::secret::wipe_opt(&mut filled.password);
            r
        },
    )
}

/// ★ D-350 접속 중 드라이버 자동 설치 진행 → stderr 한 줄(단계가 바뀔 때 · 내려받기는 10%마다).
fn driver_progress(id: &str, stage: &str, done: u64, total: Option<u64>) {
    use std::io::Write;
    static LAST: std::sync::Mutex<(String, u64)> = std::sync::Mutex::new((String::new(), u64::MAX));
    let pct = match total {
        Some(t) if t > 0 => (done * 100 / t).min(100),
        _ => 0,
    };
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.0 == stage && (stage != "download" || pct / 10 == last.1 / 10) {
        return;
    }
    *last = (stage.to_string(), pct);
    let detail = if stage == "download" {
        format!("{pct}%")
    } else {
        String::new()
    };
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "{}",
        nsql_i18n::tf(
            Msg::CliDriverInstall,
            &[id, &nsql_i18n::driver_stage_text(stage), &detail],
        )
    );
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
        C::Driver => Some(M::ErrClsDriver),
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
    settings_cached()
        .ok()
        .and_then(|s| s.get("grid.key_mode").and_then(KeyMode::parse))
        .unwrap_or(KeyMode::Pk)
}

/// 설정 `vars.signature_lookup` · `pg.refcursor_expand`(부하원 스위치 · 39 §3)를 러너에 넣는다.
/// ★ 드라이버 쪽 설정 스위치(Oracle 클라이언트·드라이버 선택 · MySQL TLS · 확장 폴더 · 자동 설치) — **접속을 여는 모든 길**(run · conn test ·
/// cat …)이 먼저 부른다. 🔧 10-23: `conn test`가 이걸 안 거쳐 자동 설치가 안 되고 `oracle.driver` 설정도 무시됐다(협업 lite V1 지적).
pub(crate) fn apply_driver_switches() {
    if let Ok(s) = settings_cached() {
        // Oracle 클라이언트(설정 ▸ DBMS ▸ Oracle) — GUI와 같은 값을 CLI도 쓴다(첫 접속 전).
        nsql_drivers::set_oracle_client(
            s.get("oracle.client_mode") == Some("manual"),
            s.get("oracle.client_dir").unwrap_or(""),
            s.get("oracle.tns_admin").unwrap_or(""),
        );
        // Oracle 드라이버 선택(docs/110 · `oracle.driver` · 접속마다 읽는다).
        nsql_drivers::set_oracle_driver(nsql_drivers::OracleDriver::parse(
            s.get("oracle.driver").unwrap_or("thin"),
        ));
        // ODBC 취소 방식(10-24 · `odbc.cancel`) — GUI와 같은 값(CLI는 ■이 없지만 세션 능력표를 같게).
        nsql_drivers::set_odbc_cancel(s.get("odbc.cancel").unwrap_or("auto"));
        // MySQL/MariaDB TLS(T-388 · `mysql.tls` · `mysql.tls_ca`) — GUI와 같은 값.
        nsql_drivers::set_mysql_tls(
            s.get("mysql.tls").unwrap_or("preferred"),
            s.get("mysql.tls_ca").unwrap_or(""),
        );
        // PostgreSQL TLS(T-70 · 155차) — GUI와 같은 값.
        nsql_drivers::set_pg_tls(
            s.get("pg.tls").unwrap_or("preferred"),
            s.get("pg.tls_ca").unwrap_or(""),
        );
        // SSH 터널(T-3 · 159차) — `ssh.command` · `ssh.connect_timeout_secs`.
        nsql_drivers::set_ssh(
            s.get("ssh.command").unwrap_or("ssh"),
            s.get("ssh.connect_timeout_secs")
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(15),
        );
        // 162차: 터널 PID 원장(<설정 폴더>/run) + 지난 실행이 남긴 ssh 정리 — GUI와 같은 자리.
        nsql_drivers::set_ssh_state_dir(nsql_settings::config_dir().map(|d| d.join("run")));
        // ★ T-128(176차 · 협업 V1 발견): keepalive · 서버 호출 상한 — GUI `apply_net_options`와 같은 값(종전 CLI는 안 넘겼다 = 상한 무효).
        nsql_drivers::set_net_options(
            s.int("net.keepalive_secs").max(0) as u64,
            s.int("session.call_timeout_secs").max(0) as u64,
        );
        nsql_drivers::cleanup_stale_tunnels();
        // 확장 드라이버 폴더(T-27 ③) = <설정 폴더>/drivers — GUI와 같은 자리.
        nsql_drivers::ext::set_driver_dir(
            nsql_settings::config_dir()
                .map(|d| d.join("drivers"))
                .as_deref(),
        );
        let _ = nsql_drivers::ext::sweep_pending();
        // ★ D-350 접속 시 드라이버 자동 설치 정책(GUI와 같은 설정).
        nsql_drivers::set_auto_install(s.flag("driver.auto_install").then(|| {
            nsql_drivers::ext::install::Source::from_settings(
                s.get("driver.repo"),
                s.int("driver.http_timeout_secs"),
            )
        }));
    }
}

fn apply_load_switches(runner: &mut Runner) {
    apply_driver_switches();
    if let Ok(s) = settings_cached() {
        runner.signature_lookup = s.get("vars.signature_lookup").is_none_or(|v| v == "on");
        runner.refcursor_expand = s.get("pg.refcursor_expand").is_none_or(|v| v == "on");
        runner.engine.settings.env_subst = s.get("vars.env_subst").is_none_or(|v| v == "on");
        runner.engine.settings.expand_at_use = s.get("vars.expand_at") == Some("use");
    }
}

/// 설정 `vars.brace_subst` · `vars.max_value_kb`(T-153) — (켬, KB).
fn var_limits_setting() -> (bool, usize) {
    settings_cached().ok().map_or((true, 1024), |s| {
        (
            s.get("vars.brace_subst").is_none_or(|v| v == "on"),
            s.int("vars.max_value_kb").max(0) as usize,
        )
    })
}

/// 설정 `vars.into_policy`(D-139) — `first`면 `SELECT … INTO`의 여러 행에서 첫 행을 쓴다.
fn into_first_setting() -> bool {
    settings_cached()
        .ok()
        .is_some_and(|s| s.get("vars.into_policy") == Some("first"))
}

/// 설정 `run.cursor_autoshow` — 실행 뒤 돌아온 REF CURSOR를 바로 결과로(기본 켬 · 끄면 `PRINT rc`).
fn cursor_autoshow_setting() -> bool {
    settings_cached()
        .ok()
        .and_then(|s| s.get("run.cursor_autoshow").map(|v| v == "on"))
        .unwrap_or(true)
}

/// 설정 `script.strict`(HIDDEN · T-9) — 미정의 `&var`·선언 없는 `:bind`를 오류로.
fn strict_setting() -> bool {
    settings_cached()
        .ok()
        .and_then(|s| s.get("script.strict").map(|v| v == "on"))
        .unwrap_or(false)
}

/// 설정 `script.define`(D-272) — `&` 치환 접두 = auto(Oracle만) | on | off.
fn define_setting() -> String {
    settings_cached()
        .ok()
        .and_then(|s| s.get("script.define").map(str::to_string))
        .unwrap_or_else(|| "auto".to_string())
}

/// 설정 `script.define_in_strings`(기본 켬) — 문자열 리터럴 안 `&이름` 치환.
fn define_in_strings_setting() -> bool {
    settings_cached()
        .ok()
        .is_none_or(|s| s.flag("script.define_in_strings"))
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
    let s = settings_cached().ok();
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
        // CLI = 늘 파일이 있다 · 없을 때도 글자 그대로(종전).
        no_file_text: None,
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
    match settings_cached() {
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
    // ★ 머리에 프로그램 판(사용자 10-29 "도움말에 프로그램 버전도").
    eprintln!(
        "{}",
        tf(
            Msg::HlpShellVersion,
            &[env!("CARGO_PKG_VERSION"), nsql_run::build_variant()]
        )
    );
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
    eprintln!("\n{}", t(Msg::CliShellSetFooter));
}

/// 표 폭 옵션 — 설정 `cli.width`/`cli.max_col_width`/`cli.overflow`(`nsql config set …`) 위에 플래그가 덮는다.
/// 줄 폭 0 = 터미널이면 콘솔 폭 · 파이프/파일이면 무제한.
fn grid_opts(o: &Opts) -> GridOpts {
    let mut g = GridOpts::default();
    if let Ok(s) = settings_cached() {
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
/// 셸 설정 항목의 표준 이름(별칭 → 하나 · `show`/`get`/`set` 공용 · 10-30 ⑨).
fn shell_opt_key(k: &str) -> Option<&'static str> {
    Some(match k {
        "width" | "linesize" | "line_width" => "width",
        "colwidth" | "max_col_width" | "col" => "colwidth",
        "overflow" | "wrap" => "overflow",
        "format" | "fmt" => "format",
        "max_rows" | "maxrows" | "rows" => "max_rows",
        "pager" => "pager",
        "autocommit" | "auto" => "autocommit",
        _ => return None,
    })
}

/// ★ 셸 설정 표(사용자 10-30 ⑨): 항목 · 현재값 · 설정 가능한 값 · 설명 — `only` = 그 항목 줄만(머리 포함) · 전체면 꼬리 안내 한 줄.
///   열 폭은 표시 폭(한글 2칸)으로 맞춘다.
fn print_shell_settings(
    g: &GridOpts,
    fmt_name: &str,
    max_rows: usize,
    pager: bool,
    autocommit: bool,
    only: Option<&str>,
) {
    let rows: [(&str, String, Msg, Msg); 7] = [
        (
            "width",
            g.line_width.to_string(),
            Msg::CliOptWidthAllowed,
            Msg::CliOptWidthDesc,
        ),
        (
            "colwidth",
            g.max_col.to_string(),
            Msg::CliOptColwidthAllowed,
            Msg::CliOptColwidthDesc,
        ),
        (
            "overflow",
            g.overflow.name().to_string(),
            Msg::CliOptOverflowAllowed,
            Msg::CliOptOverflowDesc,
        ),
        (
            "format",
            fmt_name.to_string(),
            Msg::CliOptFormatAllowed,
            Msg::CliOptFormatDesc,
        ),
        (
            "max_rows",
            max_rows.to_string(),
            Msg::CliOptMaxRowsAllowed,
            Msg::CliOptMaxRowsDesc,
        ),
        (
            "pager",
            if pager { "on" } else { "off" }.to_string(),
            Msg::CliOptPagerAllowed,
            Msg::CliOptPagerDesc,
        ),
        (
            "autocommit",
            if autocommit { "ON" } else { "OFF" }.to_string(),
            Msg::CliOptAutocommitAllowed,
            Msg::CliOptAutocommitDesc,
        ),
    ];
    let width = |s: &str| s.chars().map(lineedit::char_width).sum::<usize>();
    let pad = |s: &str, w: usize| format!("{s}{}", " ".repeat(w.saturating_sub(width(s))));
    let mut lines: Vec<[String; 4]> = vec![[
        t(Msg::CliShellColItem).to_string(),
        t(Msg::CliShellColValue).to_string(),
        t(Msg::CliShellColAllowed).to_string(),
        t(Msg::CliShellColDesc).to_string(),
    ]];
    for (k, v, a, d) in rows {
        if only.is_none_or(|o| o == k) {
            lines.push([k.to_string(), v, t(a).to_string(), t(d).to_string()]);
        }
    }
    let col = |i: usize| lines.iter().map(|l| width(&l[i])).max().unwrap_or(0);
    let (w0, w1, w2) = (col(0), col(1), col(2));
    for l in &lines {
        eprintln!(
            "{}  {}  {}  {}",
            pad(&l[0], w0),
            pad(&l[1], w1),
            pad(&l[2], w2),
            l[3]
        );
    }
    if only.is_none() {
        eprintln!("{}", t(Msg::CliShellSetFooter));
    }
}

fn shell_set(
    line: &str,
    p: &mut Printer,
    session: Option<&mut (dyn Session + 'static)>,
    autocommit: bool,
) -> bool {
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
    // 표 전체(`show`) · 한 줄(`get <항목>` · `set` 뒤 = 바뀐 항목만).
    let show = |g: &GridOpts| print_shell_settings(g, &fmt_name, max_rows, pager, autocommit, None);
    let show_one = |g: &GridOpts, fmt: &str, rows: usize, pg: bool, k: &str| {
        print_shell_settings(g, fmt, rows, pg, autocommit, shell_opt_key(k))
    };
    if low == "\\x" {
        g.overflow = if g.overflow == Overflow::Expanded {
            Overflow::Wrap
        } else {
            Overflow::Expanded
        };
        show_one(g, &fmt_name, max_rows, pager, "overflow");
        return true;
    }
    if low == "show" || low == "set" || low == "get" || low == "\\pset" || low == "help set" {
        show(g);
        return true;
    }
    // get/show <항목> — 그 줄만(머리 포함).
    if let Some(k) = low
        .strip_prefix("get ")
        .or_else(|| low.strip_prefix("show "))
    {
        let k = k.trim().trim_end_matches(';');
        if matches!(k, "all" | "") {
            show(g);
            return true;
        }
        if shell_opt_key(k).is_none() {
            return false;
        }
        show_one(g, &fmt_name, max_rows, pager, k);
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
                show_one(&g, &fmt_name, n, pager, "max_rows");
                return true;
            }
            None => eprintln!("set max_rows <n>"),
        },
        "pager" => match v {
            "on" | "off" => {
                p.pager = v == "on";
                let g = g.clone();
                show_one(&g, &fmt_name, max_rows, p.pager, "pager");
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
                show_one(&g, &p.format.name(), max_rows, pager, "format");
                return true;
            }
            None => eprintln!(
                "set format grid|markdown|csv|tsv|json|jsonl|sql:select|insert|update|delete|merge"
            ),
        },
        _ => return false,
    }
    show_one(g, &fmt_name, max_rows, pager, k);
    true
}

/// 필수 항목 이름(오류 글 · 접속 문자열의 자리 이름 그대로 · T-133).
fn login_field_name(f: nsql_core::LoginField) -> &'static str {
    use nsql_core::LoginField as F;
    match f {
        F::Host => "host",
        F::Port => "port",
        F::Database => "database",
        F::Schema => "schema",
        F::User => "user",
        F::Password => "password",
    }
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
    /// `--timing=json` = 줄마다 JSON 객체.
    timing_json: bool,
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
            timing_json: o.timing_json,
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
            let line = timing_line(self.timing_json, "statement", tl);
            self.err(&line);
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
                    "\n{}{note}\n",
                    tf(
                        Msg::CliRows,
                        &[
                            &rs.rows.len().to_string(),
                            &format!("{:.3}", elapsed.as_secs_f64())
                        ],
                    )
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
    let st = settings_cached().ok();
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
                        "\n{}{note}",
                        tf(
                            Msg::CliRows,
                            &[
                                &rs.rows.len().to_string(),
                                &format!("{:.3}", elapsed.as_secs_f64())
                            ],
                        )
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
                            let _ = writeln!(
                                out,
                                "{}",
                                tf(
                                    Msg::CliRowsAffected,
                                    &[&n.to_string(), &format!("{:.3}", elapsed.as_secs_f64())],
                                )
                            );
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
            // 드라이버 제약(docs/110 §6) — CLI는 stderr 한 줄(영어 꼬리표).
            RunEvent::DriverLimits {
                driver,
                limits,
                at_connect,
            } => {
                let what = limits
                    .iter()
                    .map(|l| l.code())
                    .collect::<Vec<_>>()
                    .join(", ");
                // GUI 토스트와 같은 문구(ToastDriverLimits* · {0} = 드라이버 이름).
                let head = if *std::borrow::Borrow::<bool>::borrow(&at_connect) {
                    tf(Msg::ToastDriverLimitsTitle, &[&driver])
                } else {
                    tf(Msg::ToastDriverLimitsHit, &[&driver])
                };
                eprintln!("{head}: {what}");
            }
            // 서버 유형은 GUI 세션의 임시 속성 — CLI는 알려만 준다(저장 프로필 = 영속 = `nsql conn env`).
            RunEvent::ConnType(v) => {
                let e = nsql_script::ConnEnv::from_name(&v);
                let _ = writeln!(
                    out,
                    "{}",
                    tf(
                        Msg::CliConnTypeNote,
                        &[
                            nsql_script::ConnEnv::name_of(e),
                            nsql_script::ConnEnv::abbr_of(e),
                            &nsql_script::ConnEnv::abbr_of(e).to_ascii_lowercase(),
                        ],
                    )
                );
            }
            RunEvent::DbChanged(db) => {
                let _ = writeln!(out, "{}", tf(Msg::CliCurrentDb, &[&db]));
            }
            RunEvent::ShowConn => {
                let _ = writeln!(
                    out,
                    "{}",
                    tf(Msg::CliShowConn, &[&self.dialect.to_string()])
                );
            }
            RunEvent::Connected {
                description,
                dialect,
                autocommit,
                ..
            } => {
                self.dialect = dialect;
                // ★ 접속 줄 꼬리에 자동 커밋 상태(사용자 10-29 · CLI = 기본 수동 + 종료 때 커밋이라 겉으로 안 보이던 것).
                let _ = writeln!(
                    out,
                    "{} · {}",
                    tf(Msg::StConnected, &[&description, &dialect.to_string()]),
                    tf(
                        Msg::CliAutocommitTag,
                        &[if autocommit { "on" } else { "off" }]
                    )
                );
            }
            RunEvent::Disconnected | RunEvent::ConnectionClosed => {
                let _ = writeln!(out, "{}", t(Msg::StDisconnected));
            }
            // 변수 표 사건은 GUI용(패널·탭 표) — CLI는 `PRINT`/`SHOW VARIABLES`로 본다.
            RunEvent::ReadTxEnded { .. }
            | RunEvent::Held { .. }
            | RunEvent::Vars { .. }
            | RunEvent::InputNeeded { .. }
            | RunEvent::Binds { .. } => {}
            RunEvent::Timing { timeline, .. } => {
                if self.timing {
                    let line = timing_line(self.timing_json, "statement", &timeline);
                    self.err(&line);
                }
            }
            RunEvent::Error { index, line, error } => {
                self.errors += 1;
                // 공통 분류 + 코드 부각(docs/42) — `ERROR line n: [ORA-00942 · Table not found: X] 원문`.
                let stmt = self.stmts.get(index).map(String::as_str).unwrap_or("");
                // 드라이버 없음/자동 설치 실패 = GUI·conn test와 같은 짧은 Msg + 영어 상세 한 줄(10-23).
                if let Some(m) = match error.code {
                    Some(nsql_drivers::ERR_DRIVER_MISSING) => Some(Msg::StDriverMissing),
                    Some(nsql_drivers::ERR_DRIVER_INSTALL_FAILED) => {
                        Some(Msg::StDriverInstallFailed)
                    }
                    _ => None,
                } {
                    let dn = self.dialect.display_name();
                    self.err(&format!("ERROR line {line}: {}", tf(m, &[dn])));
                    self.err(&format!("  {}", error.message));
                    return;
                }
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
    // ★ T-133 잔여(10-30): 필수 로그인 항목 검사 = 접속 폼과 같은 표(`Dialect::login_required` · 비밀번호는 아래 env/프롬프트 몫) —
    //   빠졌으면 서버에 아무것도 보내지 않고 종료 코드 2(종전에는 드라이버 오류가 늦게 · 방언마다 다른 글로 나왔다).
    let d = spec.dialect.unwrap_or(dialect);
    let missing = spec.missing_login(d, false);
    if !missing.is_empty() {
        let names: Vec<&str> = missing.iter().map(|f| login_field_name(*f)).collect();
        eprintln!(
            "{}",
            tf(
                Msg::CliErrMissingLogin,
                &[&names.join(", "), d.display_name()]
            )
        );
        std::process::exit(2);
    }
    // 비밀번호 없는 접속 문자열/프로필 = env/프롬프트(sqlplus/psql 관례 · 사용자 09-16).
    term::ensure_password(&mut spec, no_prompt, target);
    // 접속 실패는 `Connected`보다 먼저 온다 → 오류 코드 표기는 **대상의 방언**으로(SQLite 실패가 `[ORA-00014]`로 보이던 결함 · T-148).
    printer.dialect = spec.dialect.unwrap_or(dialect);
    // `--timing`이면 접속 단계도(09-24 T-190: 실행 110 ms인데 전체 2 s → 접속이 어디서 걸리는지).
    let t0 = std::time::Instant::now();
    let ok = runner.connect(&spec, &mut |e| printer.handle(e));
    if printer.timing {
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let line = if printer.timing_json {
            format!("{{\"timing\":\"connect\",\"total_ms\":{ms:.1}}}")
        } else {
            format!("⏱ connect {ms:.1}ms")
        };
        printer.err(&line);
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
        eprintln!("{}", t(Msg::CliErrNeedTarget));
        return 2;
    };
    let Some(path) = o.positional.first() else {
        eprintln!("{}", t(Msg::CliErrNeedScript));
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
        .with_define_mode(&define_setting())
        .with_define_in_strings(define_in_strings_setting())
        .with_auto_cursor(cursor_autoshow_setting())
        .with_into_first(into_first_setting())
        .with_var_limits(var_limits_setting().0, var_limits_setting().1);
    apply_load_switches(&mut runner);
    // 내장 변수 층(`${workspaceFolder}`는 프로젝트 없음 → 없음 · `${file}` = 스크립트 · `${cwd}` · `${nsqlHome}` · `${config:키}` · 사용자 09-23).
    runner.engine.settings.intrinsic = std::sync::Arc::new(intrinsic_vars(path, o.dialect));
    let t_runner = std::time::Instant::now();
    connect_or_exit(&mut runner, target, o.dialect, &mut printer, o.no_prompt);
    if o.timing {
        let mut marks = STARTUP_MARKS.get().cloned().unwrap_or_default();
        marks.push(("runner", t_runner));
        marks.push(("connect", std::time::Instant::now()));
        printer.err(&startup_line(&marks));
    }
    runner.engine.set_args(&o.positional[1..]);
    for (n, v) in &o.defines {
        runner.engine.define(n, v);
    }
    let no_prompt = o.no_prompt || path == "-";
    let mut prompt = |name: &str| if no_prompt { None } else { prompt_stdin(name) };
    printer.set_source(&src);
    // ★ T-307(사용자 10-07): 설정 `vars.cli_global`(기본 끔)이 켜져 있으면 GUI와 같은 `vars/global.sql`을 **실행 시작 때 읽고 끝날 때 쓴다**
    //   — 설정은 실행마다 새로 읽히므로 바꾼 뒤 다음 실행부터 적용.
    let cli_global = settings_cached()
        .ok()
        .is_some_and(|s| s.flag("vars.cli_global"));
    let vars_dir = nsql_settings::config_dir().map(|d| d.join("vars"));
    if cli_global {
        if let Some(d) = &vars_dir {
            runner
                .engine
                .vars
                .set_global(nsql_script::varsfile::load_global(d));
            // ★ 치환 변수 글로벌 층도 같은 파일(`DEFINE` 줄 · T-153 후속 10-27) — GUI와 같은 층을 읽고 끝에 쓴다.
            runner.engine.defines.set_layer(
                nsql_script::MacroLayer::Global,
                nsql_script::varsfile::load_global_macros(d)
                    .into_iter()
                    .collect(),
            );
        }
    }
    // 파일이면 그 폴더가 `@@`의 기준(T-9) · stdin이면 cwd.
    let script_path = (path != "-").then(|| Path::new(path.as_str()));
    let errs = runner.run_script_in(&src, script_path, &mut prompt, &mut |e| printer.handle(e));
    printer.flush_sql(runner.session.as_deref_mut());
    runner.commit_at_exit();
    if cli_global {
        if let Some(d) = &vars_dir {
            let macros: Vec<(String, String)> = runner
                .engine
                .defines
                .layer(nsql_script::MacroLayer::Global)
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            nsql_script::varsfile::store_global_full(
                d,
                &runner.engine.vars.global_states(),
                &macros,
            );
        }
    }
    close_spool(&printer.spool);
    if errs > 0 {
        1
    } else {
        0
    }
}

/// ★ `nsql import -c <target> -t <table> [-f csv|tsv] [--no-header] [--cols a,b] [--map src=dst,…] [--batch N] [--commit-every N]
/// [--mode auto|driver|multirow|single] [--empty-null on|off] <file|->` — 대량 적재(docs/89 §3-3 · T-236 · INSERT 전용 D-220).
fn cmd_import(o: &Opts) -> i32 {
    use nsql_i18n::{t, tf, Msg};
    let Some(target) = &o.target else {
        eprintln!("{}", t(Msg::CliErrNeedTarget));
        return 2;
    };
    let Some(table) = &o.table else {
        eprintln!("{}", t(Msg::CliImportNeedTable));
        return 2;
    };
    let Some(path) = o.positional.first() else {
        eprintln!("{}", t(Msg::CliImportNeedFile));
        return 2;
    };
    let format = match &o.format {
        Format::JsonLines => nsql_run::bulk::ImportFormat::Jsonl,
        Format::Tsv => nsql_run::bulk::ImportFormat::Tsv,
        Format::Csv => nsql_run::bulk::ImportFormat::Csv,
        _ => nsql_run::bulk::ImportFormat::Auto,
    };
    // 설정 기본값(`bulk.*`) 위에 플래그.
    let st = settings_cached().ok();
    let int = |k: &str, d: usize| st.map_or(d, |s| s.int(k).max(0) as usize);
    let batch_rows = o
        .batch
        .unwrap_or_else(|| int("bulk.batch_rows", 1000))
        .max(1);
    let commit_every = o
        .commit_every
        .unwrap_or_else(|| int("bulk.commit_every", 10_000));
    let mode = o
        .mode
        .as_deref()
        .or_else(|| st.and_then(|s| s.get("bulk.mode")))
        .and_then(nsql_run::bulk::BulkMode::parse)
        .unwrap_or_default();
    let empty_null = o
        .empty_null
        .unwrap_or_else(|| st.is_none_or(|s| s.flag("bulk.empty_null")));
    let opts = nsql_core::BulkOpts {
        batch_rows,
        check_constraints: st.is_none_or(|s| s.flag("bulk.check_constraints")),
        fire_triggers: st.is_some_and(|s| s.flag("bulk.fire_triggers")),
        append_hint: st.is_some_and(|s| s.flag("bulk.append_hint")),
    };
    let map: Vec<(String, String)> = o
        .map
        .as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|kv| {
            kv.split_once('=')
                .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        })
        .collect();
    let spec = nsql_run::bulk::ImportSpec {
        table: table.clone(),
        format,
        header: !o.no_header,
        cols: o
            .cols
            .as_ref()
            .map(|c| c.split(',').map(|s| s.trim().to_string()).collect()),
        map,
        batch_rows,
        commit_every,
        mode,
        empty_null,
        opts,
        cancel: None,
    };
    let mut printer = Printer::new(o, Format::Grid, false);
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_define_mode(&define_setting())
        .with_define_in_strings(define_in_strings_setting())
        .with_max_rows(1)
        .with_keep_cursor(false)
        .with_message_sink(stdout_sink(printer.spool.clone()))
        .with_resolver(resolver());
    connect_or_exit(&mut runner, target, o.dialect, &mut printer, o.no_prompt);
    let mut last_print = std::time::Instant::now();
    let mut progress = |rows: u64, el: std::time::Duration| {
        if last_print.elapsed().as_millis() >= 500 {
            last_print = std::time::Instant::now();
            let rate = rows as f64 / el.as_secs_f64().max(1e-6);
            eprintln!(
                "{}",
                tf(
                    Msg::CliImportProgress,
                    &[&rows.to_string(), &format!("{rate:.0}")]
                )
            );
        }
    };
    // 준비 오류(파일 · 빈 원료 · 없는 열)는 러너가 안내문으로 돌려준다(GUI Import 창과 같은 길).
    let rep = match runner.import_file(path, &spec, &mut progress) {
        Ok(r) => r,
        Err(m) => {
            eprintln!("{m}");
            close_spool(&printer.spool);
            return 2;
        }
    };
    let rate = rep.rows as f64 / rep.elapsed.as_secs_f64().max(1e-6);
    match &rep.failure {
        None => {
            println!(
                "{}",
                tf(
                    Msg::CliImportDone,
                    &[
                        &rep.rows.to_string(),
                        table,
                        &format!("{:.3}", rep.elapsed.as_secs_f64()),
                        &format!("{rate:.0}"),
                        &rep.path,
                        &rep.batches.to_string(),
                    ],
                )
            );
        }
        Some(f) => {
            eprintln!(
                "{}",
                tf(
                    Msg::CliImportFailed,
                    &[
                        &f.row.to_string(),
                        &f.line.to_string(),
                        &f.message,
                        &rep.rows.to_string()
                    ],
                )
            );
        }
    }
    if o.timing {
        eprintln!("⏱ {}", rep.timeline.summary());
    }
    close_spool(&printer.spool);
    if rep.failure.is_some() {
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
        eprintln!("{}", t(Msg::CliErrNeedTarget));
        return 2;
    };
    let mut printer = Printer::new(o, o.format.clone(), true);
    // 셸만 상한(D-68): 설정 `cli.max_rows`(200) · `--max-rows`가 있으면 그 값 · 세션 중 `set max_rows`.
    printer.shell = true;
    let st = settings_cached().ok();
    printer.max_rows = if o.max_rows > 0 {
        o.max_rows
    } else {
        st.as_ref()
            .map_or(200, |s| s.int("cli.max_rows").max(0) as usize)
    };
    printer.auto_more = st.as_ref().is_some_and(|s| s.flag("cli.auto_more"));
    let (fetch_size, keep_cursor, idle) = fetch_settings();
    // ★ 셸만 GUI 설정 `session.autocommit`(기본 on)을 따른다(사용자 결정 ⑤ b · 10-29) — `nsql run` 등 나머지 러너는 종전대로
    //   수동 + 종료 때 커밋(`commit_at_exit` · SQL*Plus 관례 · 스크립트의 명시 COMMIT 전제). 셸 안 `SET AUTOCOMMIT`은 여전히 우선.
    let autocommit = st.as_ref().is_none_or(|s| s.flag("session.autocommit"));
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_autocommit(autocommit)
        .with_max_rows(printer.shell_limit())
        .with_fetch_size(fetch_size)
        .with_keep_cursor(keep_cursor)
        .with_message_sink(stdout_sink(printer.spool.clone()))
        .with_resolver(resolver())
        .with_spool(printer.spool.clone())
        .with_strict(strict_setting())
        .with_define_mode(&define_setting())
        .with_define_in_strings(define_in_strings_setting())
        .with_auto_cursor(cursor_autoshow_setting())
        .with_into_first(into_first_setting())
        .with_var_limits(var_limits_setting().0, var_limits_setting().1);
    apply_load_switches(&mut runner);
    runner.cursor_idle_secs = idle;
    // T-116 겹침 검사(`grid.overlap_check` · 기본 켬) — GUI 워커와 같은 규칙.
    runner.overlap_check = settings_cached()
        .ok()
        .and_then(|s| s.get("grid.overlap_check").map(|v| v == "on"))
        .unwrap_or(true);
    connect_or_exit(&mut runner, target, o.dialect, &mut printer, o.no_prompt);
    for (n, v) in &o.defines {
        runner.engine.define(n, v);
    }
    eprintln!("{}", t(Msg::CliShellBanner));
    // ★ T-453(D-393): 터미널이면 자체 줄 편집기(좌우·기록·Tab 완성 · 외부 crate 0) · 아니면(배치·파이프) 종전 `read_line`.
    let interactive = io::stdin().is_terminal();
    let hist_max = st
        .as_ref()
        .map_or(500, |s| s.int("cli.history_max").max(0) as usize);
    let mut ed = lineedit::LineEditor::new(hist_max);
    let hist_path = nsql_settings::config_dir().map(|d| d.join("shell_history"));
    if interactive {
        if let Some(p) = &hist_path {
            ed.load_history(p);
        }
    }
    let mut meta = complete::MetaCache::default();
    // ★ GUI 자동완성 설정을 CLI Tab에도(사용자 10-29 ⑥⑦ · 별칭 3키 · 삽입 대소문자).
    let ccfg = complete::CompleteCfg::from_settings(st);
    let stdin = io::stdin();
    let mut buf = String::new();
    loop {
        let line = if interactive {
            // ★ 열린 트랜잭션 표시(T-77 잔여 · 10-30 · psql `*` 관례): `nsql*3> ` = 수동 모드에서 변경 문장 3개 대기 · `nsql*> ` =
            //   열렸지만 변경 없음(읽기 트랜잭션) · 자동 커밋/닫힘 = `nsql> `. 이어지는 줄은 그대로 `   -> `.
            let prompt = if !buf.is_empty() {
                "   -> ".to_string()
            } else if runner.tx_open() {
                match runner.tx_pending() {
                    0 => "nsql*> ".to_string(),
                    n => format!("nsql*{n}> "),
                }
            } else {
                "nsql> ".to_string()
            };
            let prompt = prompt.as_str();
            let dialect = runner.dialect();
            let mut cands = |l: &str, c: usize| {
                complete::complete_with(
                    l,
                    c,
                    dialect,
                    runner.session.as_deref_mut(),
                    &mut meta,
                    &ccfg,
                )
            };
            let mut deco =
                |l: &str, c: usize, chosen: &str| complete::decorate(l, c, dialect, chosen, &ccfg);
            match lineedit::read_interactive(prompt, &mut ed, &mut cands, &mut deco) {
                lineedit::Outcome::Line(l) => l,
                lineedit::Outcome::Cancel => {
                    buf.clear();
                    continue;
                }
                lineedit::Outcome::Eof => break,
            }
        } else {
            // ★ 비TTY(파이프·배치) = 프롬프트 없음(psql 관례 · 사용자 결정 10-30 ③ · 종전에는 끝에 `nsql> `가 남았다).
            let mut line = String::new();
            match stdin.lock().read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            line
        };
        if interactive && hist_max > 0 {
            // 기록 한 항목 = 문장 하나(T-453 잔여 · 10-30): 이어지는 줄은 마지막 항목에 붙인다(↑로 여러 줄 문장이 통째로 돌아온다).
            if buf.is_empty() {
                ed.push_history(&line);
            } else {
                ed.amend_last_history(&line);
            }
            if let Some(p) = &hist_path {
                ed.save_history(p);
            }
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
        let autocommit = runner.autocommit();
        if buf.is_empty() && shell_set(t, &mut printer, runner.session.as_deref_mut(), autocommit) {
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
        // 접속·스키마가 바뀌는 문장 · 객체를 만들고 지우는 문장(DDL · 스크립트) 뒤에는 Tab 완성 캐시를 비운다(다음 Tab에서 다시 읽음).
        let reset_meta = first.to_ascii_uppercase().starts_with("CONN")
            || first.to_ascii_uppercase().starts_with("DISC")
            || complete::invalidates_meta(first)
            || runner
                .dialect()
                .is_some_and(|d| nsql_catalog::context_switch_name(d, &buf).is_some());
        let src = std::mem::take(&mut buf);
        if reset_meta {
            meta.reset();
        }
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
        eprintln!("{}", t(Msg::CliErrNeedTarget));
        return 2;
    };
    let sql = match (&o.query, o.positional.first()) {
        (Some(q), _) => q.clone(),
        (None, Some(p)) => read_source(p),
        (None, None) => {
            eprintln!("{}", t(Msg::CliErrNeedSqlOrFile));
            return 2;
        }
    };
    let mut printer = Printer::new(o, o.format.clone(), false);
    let mut runner = Runner::new(o.dialect, opener(o.dialect))
        .with_define_mode(&define_setting())
        .with_define_in_strings(define_in_strings_setting())
        .with_max_rows(o.max_rows)
        .with_keep_cursor(false)
        .with_message_sink(stdout_sink(printer.spool.clone()))
        .with_resolver(resolver());
    connect_or_exit(&mut runner, target, o.dialect, &mut printer, o.no_prompt);
    let opts = nsql_script::plan::PlanOptions { analyze: o.analyze };
    if !o.raw {
        // ★ 트리 + 검토(docs/122). 원문은 `--raw`.
        if o.analyze {
            eprintln!("{}", t(Msg::StExplainActualWarn));
        }
        return match runner.explain(&sql, opts) {
            Ok(rep) => {
                print!("{}", nsql_script::plan::render_text(&rep));
                0
            }
            Err(e) => {
                eprintln!("ERROR: {}", e.message);
                1
            }
        };
    }
    let src = nsql_script::plan::explain_script_opts(runner.engine.dialect, &sql, opts);
    let mut prompt = |_: &str| Some(String::new());
    let errs = runner.run_script(&src, &mut prompt, &mut |e| printer.handle(e));
    if errs > 0 {
        1
    } else {
        0
    }
}

/// `nsql format [<file|->] [-o <file>] [--in-place]` — 내장 Basic 포맷터(설정 `format.*` · 앱과 같은 키)로 포맷(T-255 · 09-29).
fn cmd_format(o: &Opts) -> i32 {
    let path = o.positional.first().map_or("-", String::as_str);
    let src = read_source(path);
    let s = match settings_cached() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let pairs: Vec<(String, String)> = nsql_format::OPTION_KEYS
        .iter()
        .map(|k| {
            let key = format!("format.{k}");
            let v = s.get(&key).unwrap_or("").to_string();
            (key, v)
        })
        .collect();
    let opts =
        nsql_format::Options::from_pairs(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let out = nsql_format::format_basic(&src, &opts);
    let target = if o.in_place && path != "-" {
        Some(path.to_string())
    } else {
        o.out.clone()
    };
    match target {
        Some(p) => {
            if let Err(e) = std::fs::write(&p, out.as_bytes()) {
                eprintln!("{p}: {e}");
                return 1;
            }
            0
        }
        None => {
            print!("{out}");
            0
        }
    }
}

fn cmd_export(o: &Opts) -> i32 {
    let Some(target) = &o.target else {
        eprintln!("{}", t(Msg::CliErrNeedTarget));
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
        .with_define_mode(&define_setting())
        .with_define_in_strings(define_in_strings_setting())
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
    // ★ `--fast`(docs/89 B-3): csv/tsv면 서버가 형식화해 흘리는 `COPY TO`(PG) — 지원하지 않는 세션이면 일반 경로로.
    if o.fast && matches!(format, Format::Csv | Format::Tsv) {
        if let Some(s) = runner.session.as_deref_mut() {
            match s.copy_out(&sql, matches!(format, Format::Csv), &mut out) {
                Ok(bytes) => {
                    let _ = out.flush();
                    eprintln!(
                        "{}",
                        nsql_i18n::tf(
                            nsql_i18n::Msg::CliExportFast,
                            &[&bytes.to_string(), o.out.as_deref().unwrap_or("-")]
                        )
                    );
                    return 0;
                }
                Err(e) if e.message.contains("not supported") => {
                    eprintln!("{}", nsql_i18n::t(nsql_i18n::Msg::CliExportFastFallback));
                }
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    return 1;
                }
            }
        }
    }
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
    // ★ T-8 스트리밍 내보내기(10-30 · 사용자 결정): 질의 한 문장 + 배치 가능한 형식(csv/tsv/json/jsonl/insert)이면 결과를 모으지 않고
    //   배치마다 바로 쓴다(수백만 행도 메모리 일정 · 커서 없는 드라이버는 OFFSET 페이지 폴백). 그 밖(여러 문장 · 프로시저 · grid/markdown ·
    //   sql:* = 키 조회 필요)은 종전 일괄 경로.
    if let [item] = items.as_slice() {
        if sql_kind.is_none()
            && nsql_io::StreamWriter::supports(&format)
            && matches!(
                item.kind,
                nsql_script::ItemKind::Sql(nsql_script::SqlKind::Query)
            )
        {
            let mut writer = nsql_io::StreamWriter::new(format.clone(), dialect, "");
            let mut write_err: Option<String> = None;
            let streamed = runner.export_stream(&item.text, &mut |rs| match writer
                .write_batch(&mut out, rs)
            {
                Ok(()) => true,
                Err(e) => {
                    write_err = Some(e.to_string());
                    false
                }
            });
            if let Err(e) = writer.finish(&mut out) {
                write_err.get_or_insert(e.to_string());
            }
            let _ = out.flush();
            match (streamed, write_err) {
                (Err(e), _) => {
                    eprintln!("ERROR: {e}");
                    return 1;
                }
                (Ok(_), Some(e)) => {
                    eprintln!("{}", tf(Msg::CliWriteFailed, &[&e]));
                    return 1;
                }
                (Ok(n), None) => {
                    if o.out.is_some() {
                        eprintln!(
                            "{}",
                            tf(
                                Msg::CliExportDone,
                                &[&n.to_string(), o.out.as_deref().unwrap_or("-")]
                            )
                        );
                    }
                    return 0;
                }
            }
        }
    }
    for (i, item) in items.iter().enumerate() {
        runner.run_item(i, item, &mut no_prompt, &mut |e| match e {
            RunEvent::ResultSet { rs, .. } => {
                rows += rs.rows.len();
                if sql_kind.is_some() {
                    deferred.push(rs);
                } else if let Err(e) = nsql_io::write_result_set(&mut out, &rs, &format, dialect) {
                    eprintln!("{}", tf(Msg::CliWriteFailed, &[&e.to_string()]));
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
                eprintln!("{}", tf(Msg::CliWriteFailed, &[&e.to_string()]));
                errors += 1;
            }
            for w in warn {
                let _ = writeln!(out, "-- {w}");
            }
        }
    }
    let _ = out.flush();
    if o.out.is_some() {
        eprintln!(
            "{}",
            tf(
                Msg::CliExportDone,
                &[&rows.to_string(), o.out.as_deref().unwrap_or("-")],
            )
        );
    }
    if errors > 0 {
        1
    } else {
        0
    }
}

/// 프로세스 시작 시각(`--timing` 기동 구간 · T-228).
static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
/// main이 남긴 앞 구간 표식(설정 · 인자).
static STARTUP_MARKS: std::sync::OnceLock<Vec<(&'static str, std::time::Instant)>> =
    std::sync::OnceLock::new();

/// ★ 설정을 프로세스에서 **한 번만** 읽는다(T-228 · 09-26): 기동 중 13곳이 저마다 `Settings::open_default()`로 파일을 열고
/// 레지스트리(475 키)를 풀어 200행 조회의 wall이 +50 ms였다. 읽기 전용 자리만 이 길을 쓴다(`config set`은 제 손으로 열어 저장).
pub(crate) fn settings_cached() -> io::Result<&'static nsql_settings::Settings> {
    static CELL: std::sync::OnceLock<io::Result<nsql_settings::Settings>> =
        std::sync::OnceLock::new();
    CELL.get_or_init(nsql_settings::Settings::open_default)
        .as_ref()
        .map_err(|e| io::Error::new(e.kind(), e.to_string()))
}

/// `--timing` 기동 구간 한 줄(process 시작 → 설정 → 인자 → 러너 준비 → 접속).
fn startup_line(marks: &[(&str, std::time::Instant)]) -> String {
    let start = *START.get_or_init(std::time::Instant::now);
    let mut prev = start;
    let mut parts = Vec::new();
    for (name, at) in marks {
        parts.push(format!(
            "{name} {:.1}ms",
            at.duration_since(prev).as_secs_f64() * 1000.0
        ));
        prev = *at;
    }
    format!(
        "⏱ startup {} · total {:.1}ms",
        parts.join(" · "),
        prev.duration_since(start).as_secs_f64() * 1000.0
    )
}

fn main() {
    START.get_or_init(std::time::Instant::now);
    // ★ 빌드 변형(`SHOW VERSION` · 도움말 · 10-29): 내장 드라이버 전부 = full · Oracle OCI만 = lite(D-350).
    nsql_run::set_build_variant(if cfg!(feature = "drivers-builtin") {
        "full"
    } else {
        "lite"
    });
    // 언어는 설정 파일(ui.lang · 기본 영어)에서 — 폴더를 모르면 기본값(T-37).
    if let Ok(s) = settings_cached() {
        nsql_i18n::set_lang(s.lang());
        nsql_script::set_lang(nsql_script::Lang::from_code(s.lang().code()));
    }
    let t_settings = std::time::Instant::now();
    let o = parse_opts();
    let t_opts = std::time::Instant::now();
    if o.timing {
        STARTUP_MARKS
            .set(vec![("settings", t_settings), ("opts", t_opts)])
            .ok();
    }
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
                eprintln!("{}", t(Msg::CliErrNeedScript));
                std::process::exit(2)
            };
            let src = read_source(path);
            plan::run_plan(o.dialect, &src, &o.positional[1..], &define_setting())
        }
        "run" => cmd_run(&o),
        "shell" => cmd_shell(&o),
        "export" => cmd_export(&o),
        "import" => cmd_import(&o),
        "explain" => cmd_explain(&o),
        "format" | "fmt" => cmd_format(&o),
        "conn" => conn::cmd_conn(&o),
        "driver" | "drivers" => driver::cmd_driver(&o),
        "bookmark" | "bm" => bookmark::cmd_bookmark(&o),
        "cat" | "catalog" | "obj" => cat::cmd_cat(&o),
        "config" | "settings" => config::cmd_config(&o),
        "grep" => grep::cmd_grep(&o.positional),
        "license" | "lic" => license::cmd_license(&o),
        other => {
            eprintln!("{}\n", tf(Msg::CliErrCommand, &[other]));
            usage()
        }
    };
    std::process::exit(code);
}

/// `--timing` 한 줄 — 글(`⏱ …` = `Timeline::summary`) 또는 JSON 객체(`--timing=json` · T-47):
/// `{"timing":"statement","total_ms":12.3,"stages":[{"stage":"execute","ms":1.2,"rows":10,"bytes":null,"note":null},…]}`.
fn timing_line(json: bool, label: &str, tl: &nsql_core::Timeline) -> String {
    if !json {
        return format!("⏱ {}", tl.summary());
    }
    fn esc(s: &str) -> String {
        let mut o = String::with_capacity(s.len() + 2);
        o.push('"');
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                '\n' => o.push_str("\\n"),
                '\r' => o.push_str("\\r"),
                '\t' => o.push_str("\\t"),
                c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                c => o.push(c),
            }
        }
        o.push('"');
        o
    }
    let opt = |v: Option<u64>| v.map_or("null".to_string(), |n| n.to_string());
    let stages: Vec<String> = tl
        .spans
        .iter()
        .map(|sp| {
            format!(
                "{{\"stage\":{},\"ms\":{:.1},\"rows\":{},\"bytes\":{},\"note\":{}}}",
                esc(sp.stage.label()),
                sp.dur.as_secs_f64() * 1000.0,
                opt(sp.rows),
                opt(sp.bytes),
                sp.note.as_deref().map_or("null".to_string(), esc),
            )
        })
        .collect();
    format!(
        "{{\"timing\":{},\"total_ms\":{:.1},\"stages\":[{}]}}",
        esc(label),
        tl.total().as_secs_f64() * 1000.0,
        stages.join(",")
    )
}

#[cfg(test)]
mod tests {
    /// ★ T-47(10-29): `--timing=json` 한 줄 = 파싱 가능한 JSON 객체(단계 배열 · null · 이스케이프) · 글 모드는 종전 요약.
    #[test]
    fn timing_line_json_and_text() {
        use nsql_core::{Stage, Timeline};
        use std::time::Duration;
        let mut tl = Timeline::new();
        tl.push(Stage::Execute, Duration::from_micros(1234)).note = Some("q\"x\"".into());
        let s = tl.push(Stage::Fetch, Duration::from_millis(5));
        s.rows = Some(10);
        s.bytes = Some(2048);
        let j = super::timing_line(true, "statement", &tl);
        assert_eq!(
            j,
            "{\"timing\":\"statement\",\"total_ms\":6.2,\"stages\":[{\"stage\":\"execute\",\"ms\":1.2,\"rows\":null,\"bytes\":null,\"note\":\"q\\\"x\\\"\"},{\"stage\":\"fetch\",\"ms\":5.0,\"rows\":10,\"bytes\":2048,\"note\":null}]}"
        );
        assert!(nsql_json::parse(&j).is_ok(), "{j}");
        let t = super::timing_line(false, "statement", &tl);
        assert!(t.starts_with("⏱ execute") && t.contains("fetch"), "{t}");
    }

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
