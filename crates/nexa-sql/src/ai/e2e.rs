//! ★ 실기 E2E(수동 · `#[ignore]` · 11-12 사용자 "claude, codex의 cli 모드가 동작하는지 확인을 자동화"): 앱과 **같은 부품**([`Host`] ·
//! `mcp::start` · `config_json` · `resolve_cli` · `claude_cli::args`)으로 진짜 CLI를 띄워 두 턴을 보내고 시간을 잰다.
//! 로그인된 `claude`/`codex`가 있어야 하므로 CI에서는 돌지 않는다 —
//! `cargo test -p nexa-sql --bin nexa-sql ai::e2e -- --ignored --nocapture --test-threads=1`
//! 환경: `NSQL_E2E_MODEL`(기본 opus) · `NSQL_E2E_CODEX_MODEL`(기본 = 설정 기본값) · `NSQL_E2E_PREWARM_MS`(띄운 뒤 첫 보내기까지 쉼 —
//! 미리 띄우기의 효과 측정).
#![allow(clippy::unwrap_used)]

use super::host::{CliLaunch, CodexLaunch, Host, Wake};
use super::{AiEvent, ToolReply};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Turn {
    init: Option<f32>,
    first: Option<f32>,
    done: Option<f32>,
    text: String,
    notes: Vec<String>,
}

/// 한 턴을 끝까지 받는다(도구 요청·승인은 UI 대신 바로 답한다).
fn wait_turn(host: &Host, limit: Duration) -> Turn {
    let t0 = Instant::now();
    let mut t = Turn {
        init: None,
        first: None,
        done: None,
        text: String::new(),
        notes: Vec::new(),
    };
    while t0.elapsed() < limit {
        let Ok(ev) = host.rx.recv_timeout(Duration::from_millis(200)) else {
            continue;
        };
        let at = t0.elapsed().as_secs_f32();
        match ev {
            AiEvent::Started { note, .. } => {
                t.init = Some(at);
                t.notes.push(format!("init {note}"));
            }
            AiEvent::TextDelta(s) => {
                t.first.get_or_insert(at);
                t.text.push_str(&s);
            }
            AiEvent::ToolRequest { name, reply, .. } => {
                t.notes.push(format!("tool {name}"));
                let _ = reply.send(ToolReply {
                    output: "{}".into(),
                    ok: true,
                });
            }
            AiEvent::Approval { tool, reply, .. } => {
                t.notes.push(format!("approve {tool}"));
                let _ = reply.send(true);
            }
            AiEvent::Error(e) => t.notes.push(format!("ERROR {e}")),
            AiEvent::Note(n) => t.notes.push(format!("note {n}")),
            AiEvent::Done { stop, .. } => {
                t.done = Some(at);
                t.notes.push(format!("done {stop}"));
                break;
            }
            _ => {}
        }
    }
    t
}

fn show(label: &str, t: &Turn) {
    println!(
        "[{label}] init={:?} first_text={:?} done={:?} text={:?}",
        t.init,
        t.first,
        t.done,
        t.text.trim()
    );
    for n in &t.notes {
        println!("    {n}");
    }
}

fn prewarm() {
    let ms: u64 = std::env::var("NSQL_E2E_PREWARM_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if ms > 0 {
        println!("prewarm {ms} ms");
        std::thread::sleep(Duration::from_millis(ms));
    }
}

#[test]
#[ignore]
fn claude_cli_two_turns() {
    let wake: Wake = Arc::new(|| {});
    let mut host = Host::new(wake.clone());
    let srv = super::mcp::start(host.tx.clone(), wake, 0).unwrap();
    let cfg = super::mcp::config_json(srv.port, &srv.token);
    let (program, prefix) = super::host::resolve_cli("claude");
    let model = std::env::var("NSQL_E2E_MODEL").unwrap_or_else(|_| "opus".into());
    let args = super::claude_cli::args(&model, "", "default", Some(&cfg), None, false, "");
    println!("program={program} prefix={prefix:?} port={}", srv.port);
    let t0 = Instant::now();
    host.start_cli(CliLaunch {
        program,
        prefix,
        args,
        env: vec![
            ("MCP_TIMEOUT".into(), "120000".into()),
            ("MCP_TOOL_TIMEOUT".into(), "120000".into()),
        ],
    })
    .unwrap();
    prewarm();
    println!("spawned +{:.1}s", t0.elapsed().as_secs_f32());
    host.send_cli("Reply with the single word OK.", &[])
        .unwrap();
    let a = wait_turn(&host, Duration::from_secs(180));
    show("claude turn 1", &a);
    host.send_cli("Reply with the single word AGAIN.", &[])
        .unwrap();
    let b = wait_turn(&host, Duration::from_secs(120));
    show("claude turn 2", &b);
    host.stop();
    assert!(
        a.notes.iter().any(|n| n.contains("nexa-sql:connected")),
        "MCP server did not connect"
    );
    assert!(a.done.is_some() && a.text.contains("OK"), "turn 1");
    assert!(b.done.is_some() && b.text.contains("AGAIN"), "turn 2");
}

#[test]
#[ignore]
fn codex_cli_two_turns() {
    let wake: Wake = Arc::new(|| {});
    let mut host = Host::new(wake);
    let (program, prefix) = super::host::resolve_cli("codex");
    let model = std::env::var("NSQL_E2E_CODEX_MODEL").unwrap_or_else(|_| {
        nsql_settings::REGISTRY
            .iter()
            .find(|k| k.key == "ai.codex_model")
            .map(|k| k.default.to_string())
            .unwrap_or_default()
    });
    // 앱과 같은 보정(CLI 로그인 계정이 못 쓰는 모델이면 계정 목록의 첫 모델).
    let t_list = Instant::now();
    let models = super::host::codex_list_models(&program, &prefix, "").unwrap_or_default();
    println!(
        "model/list {:.1}s {models:?}",
        t_list.elapsed().as_secs_f32()
    );
    let model = if !models.is_empty() && !models.contains(&model) {
        models[0].clone()
    } else {
        model
    };
    let cwd = std::env::temp_dir().join("nsql-e2e-codex");
    let _ = std::fs::create_dir_all(&cwd);
    println!("program={program} prefix={prefix:?} model={model}");
    let t0 = Instant::now();
    host.start_codex(CodexLaunch {
        program,
        prefix,
        api_key: String::new(),
        model,
        effort: String::new(),
        cwd: cwd.to_string_lossy().into_owned(),
        system: String::new(),
        resume: None,
    })
    .unwrap();
    prewarm();
    println!("spawned +{:.1}s", t0.elapsed().as_secs_f32());
    host.send_codex("Reply with the single word OK.", &[])
        .unwrap();
    let a = wait_turn(&host, Duration::from_secs(180));
    show("codex turn 1", &a);
    host.send_codex("Reply with the single word AGAIN.", &[])
        .unwrap();
    let b = wait_turn(&host, Duration::from_secs(120));
    show("codex turn 2", &b);
    host.stop();
    let _ = std::fs::remove_dir_all(&cwd);
    assert!(a.done.is_some() && a.text.contains("OK"), "turn 1");
    assert!(b.done.is_some() && b.text.contains("AGAIN"), "turn 2");
}
