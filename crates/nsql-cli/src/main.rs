//! `nsql` — 명령줄 도구(docs/11). M0에서는 **dry-run 플래너**: 스크립트를 읽어 엔진이
//! 무엇을 하려는지(로컬 대입 · 드라이버 요청 · 재작성된 SQL · 파라미터)를 출력한다.
//! 드라이버가 붙으면(M1) `--connect`로 실제 실행한다. GUI(nexa-sql)와 같은 엔진을 쓴다.
//!
//! ```text
//! nsql plan [--dialect oracle|mssql|postgres|mysql|sqlite|odbc] <script.sql> [args...]
//! nsql plan --dialect mssql -            # stdin
//! ```

use nsql_core::Dialect;
use nsql_script::{split_script, Action, Engine, ItemKind};
use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        eprintln!("사용: nsql plan [--dialect <name>] <script.sql | -> [스크립트 인자...]");
        std::process::exit(2);
    }
    if args[0] != "plan" {
        eprintln!(
            "nsql: 아직 'plan'만 지원합니다(드라이버는 M1). 받은 명령: {}",
            args[0]
        );
        std::process::exit(2);
    }
    let mut dialect = Dialect::Oracle;
    let mut rest = args[1..].iter();
    let mut path: Option<String> = None;
    let mut script_args: Vec<String> = Vec::new();
    while let Some(a) = rest.next() {
        if a == "--dialect" || a == "-d" {
            let Some(v) = rest.next() else {
                eprintln!("--dialect 값이 없습니다");
                std::process::exit(2);
            };
            dialect = match Dialect::from_name(v) {
                Some(d) => d,
                None => {
                    eprintln!("알 수 없는 방언: {v}");
                    std::process::exit(2);
                }
            };
        } else if path.is_none() {
            path = Some(a.clone());
        } else {
            script_args.push(a.clone());
        }
    }
    let Some(path) = path else {
        eprintln!("스크립트 경로가 없습니다");
        std::process::exit(2);
    };
    let src = if path == "-" {
        let mut s = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut s) {
            eprintln!("stdin 읽기 실패: {e}");
            std::process::exit(1);
        }
        s
    } else {
        match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{path}: {e}");
                std::process::exit(1);
            }
        }
    };
    let code = run_plan(dialect, &src, &script_args);
    std::process::exit(code);
}

fn run_plan(dialect: Dialect, src: &str, script_args: &[String]) -> i32 {
    let mut engine = Engine::new(dialect);
    engine.set_args(script_args);
    let items = split_script(src);
    println!("# dialect={dialect} items={}", items.len());
    let mut errors = 0;
    for (i, item) in items.iter().enumerate() {
        let label = match &item.kind {
            ItemKind::Command(c) => format!("{c:?}")
                .split(|c: char| !c.is_alphanumeric())
                .next()
                .unwrap_or("Command")
                .to_string(),
            ItemKind::Sql(k) => format!("Sql::{k:?}"),
            ItemKind::Invalid(_) => "Invalid".into(),
        };
        println!("\n[{}] line {} · {label}", i + 1, item.line);
        // 치환 변수 미정의는 여기서 프롬프트 대신 빈 값으로 채운다(비대화형).
        let mut guard = 0;
        loop {
            let acts = engine.plan(item);
            if let [Action::NeedInput { name }] = acts.as_slice() {
                eprintln!("  ! 치환 변수 &{name} 미정의 → 빈 값으로 진행");
                engine.define(name, "");
                guard += 1;
                if guard > 64 {
                    break;
                }
                continue;
            }
            for a in acts {
                match a {
                    Action::Execute {
                        prepared,
                        expect_out,
                        kind,
                    } => {
                        println!(
                            "  → EXECUTE ({kind:?} · {:?} · out={expect_out})",
                            prepared.mode
                        );
                        for l in prepared.sql.lines() {
                            println!("    | {l}");
                        }
                        for p in &prepared.params {
                            println!(
                                "    · {} {:?} {:?} = {}",
                                p.name,
                                p.ty,
                                p.direction,
                                p.value.to_sql_literal(dialect)
                            );
                        }
                        if !prepared.implicit.is_empty() {
                            println!("    ! 암묵 변수: {}", prepared.implicit.join(", "));
                        }
                    }
                    Action::LocalAssign { name, value } => {
                        println!("  → LOCAL  :{name} = {}", value.to_sql_literal(dialect))
                    }
                    Action::Print(list) => {
                        for (n, v) in list {
                            println!("  → PRINT  {n} = {}", v.display());
                        }
                    }
                    Action::Connect(c) => println!("  → CONNECT {}", c.redacted()),
                    Action::Disconnect => println!("  → DISCONNECT"),
                    Action::Describe(o) => println!("  → DESCRIBE {o}"),
                    Action::Show(w) => println!("  → SHOW {w}"),
                    Action::Spool(t) => println!("  → SPOOL {t}"),
                    Action::Prompt(t) => println!("  → PROMPT {t}"),
                    Action::RunScript {
                        path,
                        args,
                        relative_to_caller,
                    } => println!("  → RUN {path} {args:?} (relative={relative_to_caller})"),
                    Action::NeedInput { name } => println!("  → INPUT &{name}"),
                    Action::Nothing(msg) => println!("  · {msg}"),
                    Action::Error(e) => {
                        errors += 1;
                        println!("  ✗ {e}");
                    }
                }
            }
            break;
        }
    }
    println!(
        "\n# variables={} defines={} errors={errors}",
        engine.vars.len(),
        engine.defines.len()
    );
    for (n, v) in engine.vars.iter() {
        println!("  :{n} {:?} = {}", v.ty, v.value.display());
    }
    if errors > 0 {
        1
    } else {
        0
    }
}
