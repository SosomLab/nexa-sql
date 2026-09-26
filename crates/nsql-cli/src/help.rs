//! `nsql --help` · `nsql <명령> --help` · `nsql help [<명령>]` — 명령·인자·옵션마다 상세 설명(사용자 09-16).
//! 문구는 전부 `nsql-i18n::Msg`(영어 기본 · 한국어) · 표는 데이터(옵션 원장)에서 만든다 — 새 옵션은 [`OPTS`]에 한 줄.

use nsql_i18n::{t, Msg};

/// 옵션 하나 — 플래그 표기 · 값 자리 · 설명 · (있으면) 기본값을 주는 설정 키.
///
/// 기본값 문구는 **설정에서 읽어** 붙인다(하드코딩 금지 · 사용자 09-16: `--overflow` 도움말이 `wrap`이라 적혀 있었는데
/// 실제 기본은 `none`이었다). 사용자가 바꾼 값이면 그 값 + 내장 기본을 함께.
struct Opt {
    flags: &'static str,
    arg: &'static str,
    desc: Msg,
    setting: Option<&'static str>,
}

/// 옵션 원장(표기 순서 = 도움말 순서).
const OPTS: &[Opt] = &[
    Opt {
        flags: "-c, --connect",
        arg: "<target>",
        desc: Msg::HlpOptConnect,
        setting: None,
    },
    Opt {
        flags: "-d, --dialect",
        arg: "<name>",
        desc: Msg::HlpOptDialect,
        setting: None,
    },
    Opt {
        flags: "-f, --format",
        arg: "<fmt>",
        desc: Msg::HlpOptFormat,
        setting: Some("cli.format"),
    },
    Opt {
        flags: "--host",
        arg: "<host>",
        desc: Msg::HlpOptHost,
        setting: None,
    },
    Opt {
        flags: "--port",
        arg: "<n>",
        desc: Msg::HlpOptPort,
        setting: None,
    },
    Opt {
        flags: "--db, --database",
        arg: "<name>",
        desc: Msg::HlpOptDb,
        setting: None,
    },
    Opt {
        flags: "-u, --user",
        arg: "<user>",
        desc: Msg::HlpOptUser,
        setting: None,
    },
    Opt {
        flags: "-p, --password",
        arg: "<pw>",
        desc: Msg::HlpOptPassword,
        setting: None,
    },
    Opt {
        flags: "--password-stdin",
        arg: "",
        desc: Msg::HlpOptPasswordStdin,
        setting: None,
    },
    Opt {
        flags: "--no-prompt",
        arg: "",
        desc: Msg::HlpOptNoPrompt,
        setting: None,
    },
    Opt {
        flags: "-v, --var",
        arg: "<name=value>",
        desc: Msg::HlpOptVar,
        setting: None,
    },
    Opt {
        flags: "--no-header",
        arg: "",
        desc: Msg::HlpOptNoHeader,
        setting: None,
    },
    Opt {
        flags: "--cols",
        arg: "<a,b,…>",
        desc: Msg::HlpOptCols,
        setting: None,
    },
    Opt {
        flags: "--map",
        arg: "<src=dst,…>",
        desc: Msg::HlpOptMap,
        setting: None,
    },
    Opt {
        flags: "--batch",
        arg: "<n>",
        desc: Msg::HlpOptBatch,
        setting: Some("bulk.batch_rows"),
    },
    Opt {
        flags: "--commit-every",
        arg: "<n>",
        desc: Msg::HlpOptCommitEvery,
        setting: Some("bulk.commit_every"),
    },
    Opt {
        flags: "--mode",
        arg: "<auto|driver|multirow|single>",
        desc: Msg::HlpOptMode,
        setting: Some("bulk.mode"),
    },
    Opt {
        flags: "--empty-null",
        arg: "<on|off>",
        desc: Msg::HlpOptEmptyNull,
        setting: Some("bulk.empty_null"),
    },
    Opt {
        flags: "--max-rows",
        arg: "<n>",
        desc: Msg::HlpOptMaxRows,
        setting: None,
    },
    Opt {
        flags: "--width, --linesize",
        arg: "<n>",
        desc: Msg::HlpOptWidth,
        setting: Some("cli.width"),
    },
    Opt {
        flags: "--max-col-width",
        arg: "<n>",
        desc: Msg::HlpOptMaxColWidth,
        setting: Some("cli.max_col_width"),
    },
    Opt {
        flags: "--overflow",
        arg: "<mode>",
        desc: Msg::HlpOptOverflow,
        setting: Some("cli.overflow"),
    },
    Opt {
        flags: "-x, --expanded",
        arg: "",
        desc: Msg::HlpOptExpanded,
        setting: None,
    },
    Opt {
        flags: "--timing",
        arg: "",
        desc: Msg::HlpOptTiming,
        setting: None,
    },
    Opt {
        flags: "--log",
        arg: "",
        desc: Msg::HlpOptLog,
        setting: None,
    },
    Opt {
        flags: "-q, --query",
        arg: "<sql>",
        desc: Msg::HlpOptQuery,
        setting: None,
    },
    Opt {
        flags: "-t, --table",
        arg: "<table>",
        desc: Msg::HlpOptTable,
        setting: None,
    },
    Opt {
        flags: "-o, --out",
        arg: "<file>",
        desc: Msg::HlpOptOut,
        setting: None,
    },
    Opt {
        flags: "-s, --schema",
        arg: "<schema>",
        desc: Msg::HlpOptSchema,
        setting: None,
    },
    Opt {
        flags: "-h, --help",
        arg: "",
        desc: Msg::HlpOptHelp,
        setting: None,
    },
    Opt {
        flags: "-V, --version",
        arg: "",
        desc: Msg::HlpOptVersion,
        setting: None,
    },
];

/// 명령 하나 — 이름 · 사용 줄 · 한 줄 설명 · 상세 · 위치 인자 · 쓰는 옵션(플래그 첫 표기) · 예.
struct Cmd {
    name: &'static str,
    usage: &'static str,
    brief: Msg,
    detail: Msg,
    args: &'static [(&'static str, Msg)],
    opts: &'static [&'static str],
    /// 이 명령에서만 기본값이 다른 옵션(플래그 표기 → 문구) — 예: `export -f`는 csv.
    notes: &'static [(&'static str, Msg)],
    examples: &'static [&'static str],
}

const CONN_OPTS: &[&str] = &[
    "-c",
    "-d",
    "--host",
    "--port",
    "--db",
    "-u",
    "-p",
    "--password-stdin",
    "--no-prompt",
];

const CMDS: &[Cmd] = &[
    Cmd {
        name: "run",
        usage: "nsql run -c <target> [options] <script|-> [args...]",
        brief: Msg::HlpCmdRun,
        detail: Msg::HlpCmdRunDetail,
        args: &[("<script|->", Msg::HlpArgScript), ("[args...]", Msg::HlpArgScriptArgs)],
        opts: &["-c", "-d", "--host", "--port", "--db", "-u", "-p", "--password-stdin", "--no-prompt", "-v", "-f", "--max-rows", "--width", "--max-col-width", "--overflow", "-x", "--timing", "--log"],
        notes: &[],
        examples: &[
            "nsql run -c prod report.sql",
            "nsql run -c oracle://scott:tiger@db:1521/orcl -f csv query.sql > out.csv",
            "echo \"SELECT 1 FROM dual;\" | nsql run -c prod -",
            "nsql run -c prod --width 120 --overflow truncate wide.sql",
            "nsql run -c prod -f markdown query.sql > result.md",
        ],
    },
    Cmd {
        name: "shell",
        usage: "nsql shell -c <target> [options]",
        brief: Msg::HlpCmdShell,
        detail: Msg::HlpCmdShellDetail,
        args: &[],
        opts: &["-c", "-d", "--host", "--port", "--db", "-u", "-p", "--password-stdin", "--no-prompt", "-v", "-f", "--max-rows", "--width", "--max-col-width", "--overflow", "-x", "--timing", "--log"],
        notes: &[],
        examples: &["nsql shell -c prod", "nsql shell -c prod --width 160", "nsql shell -c sqlite:app.db -d sqlite"],
    },
    Cmd {
        name: "export",
        usage: "nsql export -c <target> (-q <sql> | -t <table>) [-f fmt] [-o file]",
        brief: Msg::HlpCmdExport,
        detail: Msg::HlpCmdExportDetail,
        args: &[],
        opts: &["-c", "-d", "--host", "--port", "--db", "-u", "-p", "--password-stdin", "--no-prompt", "-q", "-t", "-f", "-o", "--max-rows"],
        notes: &[("-f, --format", Msg::HlpExportFmtDefault)],
        examples: &["nsql export -c prod -t EMP -f csv -o emp.csv", "nsql export -c prod -q \"SELECT * FROM emp WHERE deptno=10\" -f insert:EMP"],
    },
    Cmd {
        name: "import",
        usage: "nsql import -c <target> -t <table> [-f csv|tsv] [--no-header] [--cols a,b] [--map src=dst,…] [--batch N] [--commit-every N] [--mode auto|driver|multirow|single] [--empty-null on|off] <file|->",
        brief: Msg::HlpCmdImport,
        detail: Msg::HlpCmdImportDetail,
        args: &[],
        opts: &["-c", "-d", "--host", "--port", "--db", "-u", "-p", "--password-stdin", "--no-prompt", "-t", "-f", "--no-header", "--cols", "--map", "--batch", "--commit-every", "--mode", "--empty-null", "--timing"],
        notes: &[],
        examples: &["nsql import -c prod -t EMP emp.csv", "nsql import -c prod -t EMP -f tsv --map emp_no=EMPNO,name=ENAME --commit-every 5000 emp.tsv"],
    },
    Cmd {
        name: "explain",
        usage: "nsql explain -c <target> (-q <sql> | <file>)",
        brief: Msg::HlpCmdExplain,
        detail: Msg::HlpCmdExplainDetail,
        args: &[("<file>", Msg::HlpArgExplainFile)],
        opts: &["-c", "-d", "--host", "--port", "--db", "-u", "-p", "--password-stdin", "--no-prompt", "-q"],
        notes: &[],
        examples: &["nsql explain -c prod -q \"SELECT * FROM emp\"", "nsql explain -c prod slow.sql"],
    },
    Cmd {
        name: "plan",
        usage: "nsql plan [-d dialect] <script|-> [args...]",
        brief: Msg::HlpCmdPlan,
        detail: Msg::HlpCmdPlanDetail,
        args: &[("<script|->", Msg::HlpArgScript), ("[args...]", Msg::HlpArgScriptArgs)],
        opts: &["-d"],
        notes: &[],
        examples: &["nsql plan -d mssql session-vars.sql 2026 Q1"],
    },
    Cmd {
        name: "conn",
        usage: "nsql conn list | add <name> [<target>] [conn options] | show <name> | rm <name> | test [<name>] [conn options] | path",
        brief: Msg::HlpCmdConn,
        detail: Msg::HlpCmdConnDetail,
        args: &[
            ("list", Msg::HlpArgConnList),
            ("add <name> [<target>]", Msg::HlpArgConnAdd),
            ("show <name>", Msg::HlpArgConnShow),
            ("rm <name>", Msg::HlpArgConnRm),
            ("test [<name>]", Msg::HlpArgConnTest),
            ("path", Msg::HlpArgConnPath),
        ],
        opts: CONN_OPTS,
        notes: &[],
        examples: &[
            "nsql conn add prod oracle://scott@db:1521/orcl        # asks for the password",
            "nsql conn add prod -d oracle --host db --port 1521 --db orcl --user scott",
            "nsql conn test prod",
            "nsql conn test -d oracle --host db --port 1521 --db orcl --user scott",
        ],
    },
    Cmd {
        name: "cat",
        usage: "nsql cat -c <target> [-s schema] [-f fmt] schemas | kinds | <kind> | columns <object> | sub <object> <sub-kind> [kind] | detail <object> [kind] | gen <what> <object> [kind] [sub-kind sub-name] | source <kind> <name> | errors <name>",
        brief: Msg::HlpCmdCat,
        detail: Msg::HlpCmdCatDetail,
        args: &[
            ("schemas", Msg::HlpArgCatSchemas),
            ("kinds", Msg::HlpArgCatKinds),
            ("<kind>", Msg::HlpArgCatKind),
            ("columns <object>", Msg::HlpArgCatColumns),
            ("sub <object> <sub-kind> [kind]", Msg::HlpArgCatSub),
            ("gen <what> <object> [kind] [sub-kind sub-name]", Msg::HlpArgCatGen),
            ("detail <object> [kind]", Msg::HelpCatDetail),
        ("source <kind> <name>", Msg::HlpArgCatSource),
            ("errors <name>", Msg::HlpArgCatErrors),
        ],
        opts: &["-c", "-d", "-s", "-f", "--width", "--max-col-width", "--overflow", "-x"],
        notes: &[],
        examples: &["nsql cat -c prod tables", "nsql cat -c prod -s HR columns EMPLOYEES", "nsql cat -c prod sub EMPLOYEES foreign_keys", "nsql cat -c prod gen merge EMPLOYEES", "nsql cat -c prod gen call PKG_ORDER package", "nsql cat -c prod source packages PKG_ORDER"],
    },
    Cmd {
        name: "config",
        usage: "nsql config list [all|perf] | get <key> | set <key> <value> | reset <key> | export-json | path",
        brief: Msg::HlpCmdConfig,
        detail: Msg::HlpCmdConfigDetail,
        args: &[
            ("list [all]", Msg::HlpArgConfigList),
            ("list perf", Msg::HlpArgConfigListPerf),
            ("get <key>", Msg::HlpArgConfigGet),
            ("set <key> <value>", Msg::HlpArgConfigSet),
            ("reset <key>", Msg::HlpArgConfigReset),
            ("path", Msg::HlpArgConfigPath),
        ],
        opts: &[],
        notes: &[],
        examples: &["nsql config list", "nsql config list perf", "nsql config set perf.mode low", "nsql config set cli.width 160", "nsql config set ui.lang ko", "nsql config get grid.max_rows"],
    },
    Cmd {
        name: "grep",
        usage: "nsql grep <pattern> [<path>…] [-i] [-w] [-e] [--no-ignore] [-j <n>] [--max-file-kb <n>]",
        brief: Msg::HlpCmdGrep,
        detail: Msg::HlpCmdGrepDetail,
        args: &[
            ("<pattern>", Msg::HlpArgGrepPattern),
            ("<path>…", Msg::HlpArgGrepPaths),
            ("-i, --ignore-case", Msg::HlpArgGrepIgnoreCase),
            ("-w, --word", Msg::HlpArgGrepWord),
            ("-e, --regex", Msg::HlpArgGrepRegex),
            ("--no-ignore", Msg::HlpArgGrepNoIgnore),
            ("-j, --threads <n>", Msg::HlpArgGrepThreads),
            ("--max-file-kb <n>", Msg::HlpArgGrepMaxFileKb),
        ],
        opts: &[],
        notes: &[],
        examples: &["nsql grep SELECT", "nsql grep -i -w emp scripts/ ../Oracle", "nsql grep -e 'sp_\\w+_create' --no-ignore ."],
    },
    Cmd {
        name: "bookmark",
        usage: "nsql bookmark list [--project <file>] [--doc <path>] [--md] | add <file> <line> [--label <text>] | rm <id> | prune [--days N]",
        brief: Msg::HlpCmdBookmark,
        detail: Msg::HlpCmdBookmarkDetail,
        args: &[
            ("list [--doc <path>] [--md]", Msg::HlpArgBmList),
            ("add <file> <line> [--label <text>]", Msg::HlpArgBmAdd),
            ("rm <id>", Msg::HlpArgBmRm),
            ("prune [--days N]", Msg::HlpArgBmPrune),
            ("--project <file>", Msg::HlpArgBmProject),
        ],
        opts: &[],
        notes: &[],
        examples: &[
            "nsql bookmark list",
            "nsql bookmark add scripts/report.sql 42 --label \"monthly totals\"",
            "nsql bookmark list --project D:/work/erp.nsql-project --md",
            "nsql bookmark prune --days 7",
        ],
    },
];

fn opt_by_flag(flag: &str) -> Option<&'static Opt> {
    OPTS.iter().find(|o| o.flags.split(", ").any(|f| f == flag))
}

/// 왼쪽 열 폭에 맞춰 `left  desc` 줄(설명이 여러 줄이면 들여쓰기 유지).
fn row(out: &mut String, left: &str, desc: &str, width: usize) {
    let mut first = true;
    for line in desc.split('\n') {
        if first {
            out.push_str(&format!("  {left:<width$}  {line}\n"));
            first = false;
        } else {
            out.push_str(&format!("  {:<width$}  {line}\n", ""));
        }
    }
}

fn opts_table(out: &mut String, flags: &[&str], notes: &[(&str, Msg)]) {
    let rows: Vec<(String, String)> = flags
        .iter()
        .filter_map(|f| opt_by_flag(f))
        .map(|o| {
            let left = if o.arg.is_empty() {
                o.flags.to_string()
            } else {
                format!("{} {}", o.flags, o.arg)
            };
            let mut desc = t(o.desc).to_string();
            if let Some((_, m)) = notes.iter().find(|(f, _)| *f == o.flags) {
                desc.push_str(&format!(". {}", t(*m)));
            } else if let Some(key) = o.setting {
                desc.push_str(&default_note(key));
            }
            (left, desc)
        })
        .collect();
    let w = rows
        .iter()
        .map(|(l, _)| l.chars().count())
        .max()
        .unwrap_or(0);
    for (l, d) in rows {
        row(out, &l, &d, w);
    }
}

/// ` Default: <key> = <값>` — 값은 **지금 설정 파일에서 get**(없으면 레지스트리 기본). 사용자가 바꿔 두었으면
/// `(built-in <내장 기본>)`을 덧붙여 어느 쪽이 어긋났는지 보이게.
fn default_note(key: &str) -> String {
    let Some(e) = nsql_settings::entry(key) else {
        return String::new();
    };
    let cur = nsql_settings::Settings::open_default()
        .ok()
        .and_then(|st| st.get(key).map(|v| v.to_string()))
        .unwrap_or_else(|| e.default.to_string());
    let mut s = format!(". {}", nsql_i18n::tf(Msg::HlpDefaultOf, &[key, &cur]));
    if cur != e.default {
        s.push_str(&format!(
            " {}",
            nsql_i18n::tf(Msg::HlpBuiltIn, &[e.default])
        ));
    }
    s
}

/// 전체 개요(명령 목록 + 접속 대상 표기 + 드라이버).
fn overview(drivers: &str) -> String {
    let mut o = String::new();
    o.push_str(&format!("{}\n\n", t(Msg::HlpTitle)));
    o.push_str(&format!("{}\n", t(Msg::HlpCommands)));
    let w = CMDS.iter().map(|c| c.name.len()).max().unwrap_or(0);
    for c in CMDS {
        row(&mut o, c.name, t(c.brief), w);
    }
    o.push_str(&format!("\n{}\n", t(Msg::HlpTargets)));
    for line in t(Msg::HlpTargetForms).split('\n') {
        o.push_str(&format!("  {line}\n"));
    }
    o.push_str(&format!("\n{}\n", t(Msg::HlpCommonOptions)));
    opts_table(
        &mut o,
        &["-c", "-d", "-f", "--width", "-x", "-h", "-V"],
        &[],
    );
    o.push_str(&format!("\n  {}: {drivers}\n", t(Msg::HlpDrivers)));
    o.push_str(&format!("\n{}\n", t(Msg::HlpMoreHelp)));
    o
}

/// 명령 하나의 상세.
fn detail(c: &Cmd) -> String {
    let mut o = String::new();
    o.push_str(&format!("{}\n\n  {}\n\n", t(c.brief), c.usage));
    for line in t(c.detail).split('\n') {
        o.push_str(&format!("  {line}\n"));
    }
    if !c.args.is_empty() {
        o.push_str(&format!("\n{}\n", t(Msg::HlpArguments)));
        let w = c
            .args
            .iter()
            .map(|(a, _)| a.chars().count())
            .max()
            .unwrap_or(0);
        for (a, m) in c.args {
            row(&mut o, a, t(*m), w);
        }
    }
    if !c.opts.is_empty() {
        o.push_str(&format!("\n{}\n", t(Msg::HlpOptions)));
        opts_table(&mut o, c.opts, c.notes);
    }
    if c.name == "shell" {
        o.push_str(&format!("\n{}\n", t(Msg::HlpShellCommands)));
        for line in t(Msg::HlpShellCommandList).split('\n') {
            o.push_str(&format!("  {line}\n"));
        }
        // T-48d 추가 페치(\more · \all · \count · \pager) + set max_rows(기본값은 설정 cli.max_rows).
        for line in t(Msg::CliShellSetMaxRows).split('\n') {
            o.push_str(&format!("  {line}\n"));
        }
        for line in t(Msg::HlpShellFetchList).split('\n') {
            o.push_str(&format!("  {line}\n"));
        }
        o.push_str(&format!(
            "  {}\n",
            default_note("cli.max_rows").trim_start_matches(". ")
        ));
        // T-52 다른 CLI 별칭(psql · sqlcmd · sqlite3) + SPOOL(T-9).
        o.push_str(&format!("\n{}\n", t(Msg::HlpShellAliases)));
        for line in t(Msg::HlpShellAliasList).split('\n') {
            o.push_str(&format!("  {line}\n"));
        }
    }
    if !c.examples.is_empty() {
        o.push_str(&format!("\n{}\n", t(Msg::HlpExamples)));
        for e in c.examples {
            o.push_str(&format!("  {e}\n"));
        }
    }
    o
}

fn drivers() -> String {
    nsql_drivers::available()
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// 도움말 본문. `cmd`가 없거나 모르는 명령이면 개요.
pub(crate) fn text(cmd: Option<&str>) -> String {
    match cmd.and_then(|n| {
        CMDS.iter().find(|c| {
            c.name == n
                || (n == "catalog" && c.name == "cat")
                || (n == "settings" && c.name == "config")
        })
    }) {
        Some(c) => detail(c),
        None => overview(&drivers()),
    }
}

/// 도움말을 stdout에(`--help` · 종료 0용).
pub(crate) fn print(cmd: Option<&str>) {
    print!("{}", text(cmd));
}

/// 인자 목록에 `-h`/`--help`가 있는가.
pub(crate) fn wants_help(args: &[String]) -> bool {
    args.iter().any(|a| a == "-h" || a == "--help")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_option_is_in_the_ledger() {
        for c in CMDS {
            for f in c.opts {
                assert!(opt_by_flag(f).is_some(), "{}: {f}", c.name);
            }
        }
        assert!(overview("sqlite").contains("nsql"));
        for c in CMDS {
            let d = detail(c);
            assert!(d.contains(c.usage), "{}", c.name);
        }
    }
}
