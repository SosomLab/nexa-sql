//! `nsql plan` — 실행 없이 엔진 계획을 출력한다(DB 불요 · 회귀 점검용).

use nsql_core::Dialect;
use nsql_script::{Action, Engine, ItemKind};

pub(crate) fn run_plan(dialect: Dialect, src: &str, script_args: &[String]) -> i32 {
    let mut engine = Engine::new(dialect);
    engine.set_args(script_args);
    let items = nsql_script::split_script_in(src, Some(dialect));
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
                    Action::ListVars(list) => {
                        for (n, t) in list {
                            println!("  → VARIABLE  {n} {}", t.sql_name());
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
                    Action::Accept { name, default, .. } => println!(
                        "  → ACCEPT &{name}{}",
                        default
                            .as_deref()
                            .map(|d| format!(" (default {d})"))
                            .unwrap_or_default()
                    ),
                    Action::SetOption { name, value } => println!("  → SET {name}={value}"),
                    Action::Nothing(msg) => println!("  · {msg}"),
                    Action::Replan => println!("  · replan (formula re-evaluated first)"),
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
