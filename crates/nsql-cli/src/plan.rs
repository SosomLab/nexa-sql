//! `nsql plan` — 실행 없이 엔진 계획을 출력한다(DB 불요 · 회귀 점검용).

use nsql_core::Dialect;
use nsql_script::{Action, Engine, ItemKind};

pub(crate) fn run_plan(
    dialect: Dialect,
    src: &str,
    script_args: &[String],
    define_mode: &str,
) -> i32 {
    let mut engine = Engine::new(dialect);
    // D-272: 실행 미리보기도 설정 `script.define`(auto|on|off)을 따른다(러너를 안 거치는 길 · 협업 V1 10-10).
    engine.set_define_mode(define_mode);
    engine.set_args(script_args);
    let items = nsql_script::split_script_in(src, Some(dialect));
    println!("# dialect={dialect} items={}", items.len());
    let mut errors = 0;
    // 서버가 채우는 변수(잡을 행 `captures`) — 접속 없는 plan에서는 값이 생기지 않는다(T-162 ⑥).
    let mut server_filled: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
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
        // 서버 식 대입 뒤의 `PRINT` — plan은 서버에 가지 않으니 "변수 없음" 오류가 아니라 안내다.
        if let ItemKind::Command(nsql_script::Command::Print { names }) = &item.kind {
            let pending = |n: &String| {
                !engine.vars.contains(n) && server_filled.contains(&n.to_ascii_uppercase())
            };
            if names.iter().any(pending) {
                for n in names {
                    match engine.vars.get(n) {
                        Some(v) => println!("  → PRINT  {n} = {}", v.value.display()),
                        None if pending(n) => {
                            println!(
                                "{}",
                                nsql_i18n::tf(nsql_i18n::Msg::CliPlanServerFilled, &[n])
                            )
                        }
                        None => {
                            errors += 1;
                            println!("{}", nsql_i18n::tf(nsql_i18n::Msg::CliPlanNoVar, &[n]));
                        }
                    }
                }
                continue;
            }
        }
        let mut guard = 0;
        loop {
            let acts = engine.plan(item);
            if let [Action::NeedInput { name }] = acts.as_slice() {
                eprintln!(
                    "{}",
                    nsql_i18n::tf(nsql_i18n::Msg::CliPlanUndefinedSubst, &[name])
                );
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
                        // 전달 모드 꼬리표(T-416 · 실행 로그와 같은 글 · 매개변수 없으면 `no params`).
                        let tag = nsql_script::delivery_tag(
                            prepared.mode,
                            prepared.params.len(),
                            Some(dialect),
                        );
                        println!(
                            "  → EXECUTE ({kind:?} · {} · out={expect_out})",
                            if tag.is_empty() {
                                "no params"
                            } else {
                                tag.as_str()
                            }
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
                        server_filled
                            .extend(prepared.captures.iter().map(|n| n.to_ascii_uppercase()));
                        if !prepared.implicit.is_empty() {
                            println!(
                                "{}",
                                nsql_i18n::tf(
                                    nsql_i18n::Msg::CliPlanImplicitVars,
                                    &[&prepared.implicit.join(", ")]
                                )
                            );
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
