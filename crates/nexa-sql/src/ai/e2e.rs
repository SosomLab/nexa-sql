//! ★ 실기 E2E(수동 · `#[ignore]` · 11-12 사용자 "claude, codex의 cli 모드가 동작하는지 확인을 자동화"): 앱과 **같은 부품**([`Host`] ·
//! `mcp::start` · `config_json` · `resolve_cli` · `claude_cli::args`)으로 진짜 CLI를 띄워 두 턴을 보내고 시간을 잰다.
//! 로그인된 `claude`/`codex`가 있어야 하므로 CI에서는 돌지 않는다 —
//! `cargo test -p nexa-sql --bin nexa-sql ai::e2e -- --ignored --nocapture --test-threads=1`
//! API 모드(11-13 사용자 "AI는 API 모드로 동작 — AI 관련 시험도 함께") = `claude_api_tool_round_trip`: 저장된 프로필·볼트 키를
//! **읽기만** 해서 진짜 요청 세 턴(글 · 도구 왕복 · 대화 이어 가기) — 키는 요청 머리글로만 가고 어디에도 찍지 않는다 ·
//! `NSQL_E2E_API_PROFILE`(기본 = 현재 프로필 → 키가 있는 첫 Claude 프로필) · `NSQL_E2E_API_MODEL`(기본 = 프로필 모델).
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
    wait_turn_tools(host, limit, &[])
}

/// [`wait_turn`] + 도구 이름별 정해 둔 답(`canned` = (도구 이름, 결과 JSON) · 없는 도구는 `{}`).
fn wait_turn_tools(host: &Host, limit: Duration, canned: &[(&str, &str)]) -> Turn {
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
                let output = canned
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map_or("{}", |(_, o)| *o);
                let _ = reply.send(ToolReply {
                    output: output.into(),
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

/// ★ API 모드 실기 왕복(11-13): 앱과 같은 부품([`Host::start_api`] · `claude_api::run_turn` · 진짜 도구 정의 · 시스템 프롬프트)으로
/// ① 글 한 턴 ② 도구 호출 → 정해 둔 결과 → 그 값을 답에 쓰는지 ③ 앞 턴을 기억하는지(대화 누적)를 본다.
#[test]
#[ignore]
fn claude_api_tool_round_trip() {
    use super::{ProfileStore, Provider};
    let dir = ProfileStore::dir().expect("config dir");
    let store = ProfileStore::load(&dir);
    let vault = nsql_vault::Vault::open_default().expect("vault");
    let key_of = |name: &str| {
        vault
            .get_secret(&format!("ai.{name}"))
            .and_then(|b| String::from_utf8(b).ok())
    };
    let want = std::env::var("NSQL_E2E_API_PROFILE").unwrap_or_else(|_| store.current.clone());
    let usable = |p: &&super::Profile| {
        p.provider == Provider::Claude && p.wif.is_none() && key_of(&p.name).is_some()
    };
    let p = store
        .profiles
        .iter()
        .filter(usable)
        .find(|p| p.name == want)
        .or_else(|| store.profiles.iter().find(usable))
        .expect("no Claude API-key profile with a stored key")
        .clone();
    // ★ 키는 요청 머리글로만 — 출력·단언 글에 싣지 않는다.
    let key = key_of(&p.name).expect("key");
    let model = std::env::var("NSQL_E2E_API_MODEL")
        .ok()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| {
            if p.model.is_empty() {
                "opus".into()
            } else {
                p.model.clone()
            }
        });
    println!("api model={model} effort={:?}", p.effort);
    let system = super::tools::system_prompt("Oracle", "e2e", "assistant");
    let launch = || super::host::ApiLaunch {
        base_url: p.base_url.clone(),
        auth: super::claude_api::ApiAuth::Key(key.clone()),
        wif: None,
        workspace: p.workspace.clone(),
        model: model.clone(),
        effort: p.effort.clone(),
        system: system.clone(),
        timeout_secs: 120,
        max_tokens: 4096,
        prices: String::new(),
    };
    let wake: Wake = Arc::new(|| {});
    let mut host = Host::new(wake);

    host.start_api(launch(), "Reply with the single word OK.", &[])
        .unwrap();
    let a = wait_turn(&host, Duration::from_secs(120));
    show("api turn 1", &a);
    host.busy = false;
    assert!(
        !a.notes.iter().any(|n| n.starts_with("ERROR")),
        "turn 1 error"
    );
    assert!(a.done.is_some() && a.text.contains("OK"), "turn 1");

    let canned = [(
        "list_vars",
        r#"{"vars":[{"name":"E2E_TOKEN","type":"VARCHAR2","layer":"tab","value":"ZEBRA-7391"}]}"#,
    )];
    host.start_api(
        launch(),
        "Use the list_vars tool, then reply with only the value of the variable E2E_TOKEN.",
        &[],
    )
    .unwrap();
    let b = wait_turn_tools(&host, Duration::from_secs(180), &canned);
    show("api turn 2", &b);
    host.busy = false;
    assert!(
        b.notes.iter().any(|n| n == "tool list_vars"),
        "turn 2: list_vars was not called"
    );
    assert!(
        b.done.is_some() && b.text.contains("ZEBRA-7391"),
        "turn 2: tool result not used"
    );

    host.start_api(
        launch(),
        "Without calling any tool: which variable name did I ask about? Reply with the name only.",
        &[],
    )
    .unwrap();
    let c = wait_turn(&host, Duration::from_secs(120));
    show("api turn 3", &c);
    host.stop();
    assert!(
        c.done.is_some() && c.text.contains("E2E_TOKEN"),
        "turn 3: conversation history lost"
    );
    // 디버그 출력에 키가 실리지 않는다(보안 불변식).
    let dbg = format!("{:?}", super::claude_api::ApiAuth::Key(key.clone()));
    assert!(!dbg.contains(&key), "key leaked into Debug output");
}
