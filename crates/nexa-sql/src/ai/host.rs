//! ★ 호스트(docs/110 §5·§14-1): 한 세션의 **상대**를 돌린다 — `Cli`(자식 `claude` 프로세스 · stream-json) · `Api`(Messages API
//! 턴 스레드) · `Mock`(네트워크·CLI 없이 헤드리스 V1용 각본 · `NSQL_AI_MOCK=1`). 셋 다 [`AiEvent`]를 같은 채널로 보내고 깨운다.

use super::{claude_api, claude_cli, AiEvent, Attachment, ToolReply, Usage};
use nsql_settings::json::Json;
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

pub(crate) type Wake = Arc<dyn Fn() + Send + Sync>;
/// Codex 스레드가 준비되기 전에 들어온 첫 턴(글 · 로컬 이미지 경로).
pub(crate) type PendingTurn = Arc<Mutex<Option<(String, Vec<String>)>>>;

/// CLI 실행 파일 해석(Windows: npm `claude.cmd`는 cmd.exe 없이 `node cli.js`로 · 네이티브 `claude.exe` 우선).
pub(crate) fn resolve_cli(path: &str) -> (String, Vec<String>) {
    let p = path.trim();
    let p = if p.is_empty() { "claude" } else { p };
    #[cfg(windows)]
    {
        use std::path::{Path, PathBuf};
        let has_dir = p.contains('\\') || p.contains('/');
        let mut candidates: Vec<PathBuf> = Vec::new();
        if has_dir {
            candidates.push(PathBuf::from(p));
        } else {
            if let Some(home) = std::env::var_os("USERPROFILE") {
                candidates.push(
                    Path::new(&home)
                        .join(".local")
                        .join("bin")
                        .join(format!("{p}.exe")),
                );
            }
            if let Some(paths) = std::env::var_os("PATH") {
                for dir in std::env::split_paths(&paths) {
                    candidates.push(dir.join(format!("{p}.exe")));
                    candidates.push(dir.join(format!("{p}.cmd")));
                }
            }
        }
        for c in candidates {
            if !c.is_file() {
                continue;
            }
            let s = c.to_string_lossy().into_owned();
            if s.to_ascii_lowercase().ends_with(".cmd") {
                // npm 래퍼: `node "%~dp0\node_modules\@anthropic-ai\claude-code\cli.js"` — 셸 없이 node로 직접.
                let cli_js = c
                    .parent()
                    .map(|d| {
                        d.join("node_modules")
                            .join("@anthropic-ai")
                            .join("claude-code")
                            .join("cli.js")
                    })
                    .filter(|f| f.is_file());
                if let Some(js) = cli_js {
                    return ("node".into(), vec![js.to_string_lossy().into_owned()]);
                }
                continue;
            }
            return (s, Vec::new());
        }
        (p.to_string(), Vec::new())
    }
    #[cfg(not(windows))]
    {
        (p.to_string(), Vec::new())
    }
}

/// 실행 중인 상대.
pub(crate) enum Running {
    Idle,
    Cli {
        child: std::process::Child,
        stdin: Option<std::process::ChildStdin>,
    },
    Api {
        stop: Arc<AtomicBool>,
    },
    /// `codex app-server`(stdio JSONL) — 쓰기는 `writer` 공유 · 스레드 id·턴 id는 읽기 스레드가 채운다.
    Codex {
        child: std::process::Child,
        writer: Arc<Mutex<std::process::ChildStdin>>,
        thread: Arc<Mutex<Option<String>>>,
        turn: Arc<Mutex<Option<String>>>,
        /// 스레드가 준비되기 전에 들어온 첫 턴(글 · 로컬 이미지 경로).
        pending: PendingTurn,
        next_id: Arc<std::sync::atomic::AtomicU64>,
        effort: String,
        system: String,
    },
    Mock,
}

pub(crate) struct CodexLaunch {
    pub program: String,
    pub prefix: Vec<String>,
    /// 프로필 API 키(`account/login/start` apiKey) — 비면 로그인 생략(이미 로그인된 CLI).
    pub api_key: String,
    pub model: String,
    pub effort: String,
    pub cwd: String,
    pub system: String,
    pub resume: Option<String>,
}

fn write_line(w: &mut dyn Write, line: &str) -> Result<(), String> {
    w.write_all(line.as_bytes())
        .and_then(|()| w.write_all(b"\n"))
        .and_then(|()| w.flush())
        .map_err(|e| e.to_string())
}

/// ★ Codex 모델 목록(110 §15-5 · 프로필 [검증]): `codex app-server`를 잠깐 띄워 initialize → (apiKey 로그인) → `model/list` 한 번 →
/// 종료. 키가 비면 CLI 자체 로그인(`ai.codex_cli_login`)으로 조회. 30초 상한 · 콘솔 창 없음.
pub(crate) fn codex_list_models(
    program: &str,
    prefix: &[String],
    api_key: &str,
) -> Result<Vec<String>, String> {
    use super::codex;
    let mut cmd = nsql_ext::net::quiet_command(program);
    cmd.args(prefix)
        .arg("app-server")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("{program}: {e}"))?;
    let (Some(mut stdin), Some(out)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        return Err("no stdio".into());
    };
    if let Err(e) = write_line(&mut stdin, &codex::initialize(1)) {
        let _ = child.kill();
        return Err(e);
    }
    let (tx, rx) = mpsc::channel::<Result<Vec<String>, String>>();
    let key = api_key.to_string();
    let reader = std::thread::Builder::new()
        .name("ai-codex-models".into())
        .spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let codex::Incoming::Response { id, result, error } = codex::parse_line(&line) {
                    if let Some(e) = error {
                        let _ = tx.send(Err(format!("codex {id}: {e}")));
                        return;
                    }
                    let next = match id {
                        1 => {
                            if let Err(e) = write_line(&mut stdin, &codex::initialized()) {
                                let _ = tx.send(Err(e));
                                return;
                            }
                            if key.is_empty() {
                                codex::model_list(3)
                            } else {
                                codex::login_api_key(2, &key)
                            }
                        }
                        2 => codex::model_list(3),
                        3 => {
                            let _ = tx.send(Ok(codex::model_ids(result.as_ref())));
                            return;
                        }
                        _ => continue,
                    };
                    if let Err(e) = write_line(&mut stdin, &next) {
                        let _ = tx.send(Err(e));
                        return;
                    }
                }
            }
            let _ = tx.send(Err("codex app-server closed".into()));
        });
    let r = match rx.recv_timeout(std::time::Duration::from_secs(30)) {
        Ok(r) => r,
        Err(_) => Err("codex app-server: timeout".into()),
    };
    let _ = child.kill();
    let _ = child.wait();
    if let Ok(h) = reader {
        let _ = h.join();
    }
    r
}

/// 세션 하나의 호스트 상태(UI 스레드 소유).
pub(crate) struct Host {
    pub tx: mpsc::Sender<AiEvent>,
    pub rx: mpsc::Receiver<AiEvent>,
    wake: Wake,
    pub running: Running,
    /// 턴 진행 중(보내기 금지 · ■ 활성).
    pub busy: bool,
    /// API 대화(요청 본문 messages · 사고 블록 포함) — 스레드와 공유.
    pub api_messages: Arc<Mutex<Vec<Json>>>,
    /// CLI stderr 꼬리(실패 진단).
    pub stderr_tail: Arc<Mutex<Vec<String>>>,
}

pub(crate) struct CliLaunch {
    pub program: String,
    pub prefix: Vec<String>,
    pub args: Vec<String>,
    /// 자식에만 주는 환경(API 키는 여기로만 · 로그 금지).
    pub env: Vec<(String, String)>,
}

pub(crate) struct ApiLaunch {
    pub base_url: String,
    pub api_key: String,
    pub workspace: String,
    pub model: String,
    pub effort: String,
    pub system: String,
    pub timeout_secs: u64,
    pub max_tokens: u64,
    /// 비용 표(설정 `ai.prices`).
    pub prices: String,
}

impl Host {
    pub(crate) fn new(wake: Wake) -> Host {
        let (tx, rx) = mpsc::channel();
        Host {
            tx,
            rx,
            wake,
            running: Running::Idle,
            busy: false,
            api_messages: Arc::new(Mutex::new(Vec::new())),
            stderr_tail: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(crate) fn is_cli_alive(&mut self) -> bool {
        match &mut self.running {
            Running::Cli { child, .. } => matches!(child.try_wait(), Ok(None)),
            _ => false,
        }
    }

    pub(crate) fn is_codex_alive(&mut self) -> bool {
        match &mut self.running {
            Running::Codex { child, .. } => matches!(child.try_wait(), Ok(None)),
            _ => false,
        }
    }

    /// `codex app-server` 시작 — 악수(initialize → initialized → [login] → thread/start|resume)는 읽기 스레드가 응답을 보며 진행.
    pub(crate) fn start_codex(&mut self, l: CodexLaunch) -> Result<(), String> {
        use super::codex;
        self.stop();
        let mut cmd = nsql_ext::net::quiet_command(&l.program);
        cmd.args(&l.prefix)
            .arg("app-server")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", l.program))?;
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            return Err("no stdin".into());
        };
        let Some(out) = child.stdout.take() else {
            let _ = child.kill();
            return Err("no stdout".into());
        };
        let writer = Arc::new(Mutex::new(stdin));
        let thread: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let turn: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let pending: PendingTurn = Arc::new(Mutex::new(None));
        let next_id = Arc::new(std::sync::atomic::AtomicU64::new(10));
        // 악수 첫 요청.
        {
            let mut w = writer.lock().map_err(|_| "lock".to_string())?;
            w.write_all(codex::initialize(1).as_bytes())
                .and_then(|()| w.write_all(b"\n"))
                .and_then(|()| w.flush())
                .map_err(|e| e.to_string())?;
        }
        let tx = self.tx.clone();
        let wake = self.wake.clone();
        let (w2, th2, tu2, pd2, id2) = (
            writer.clone(),
            thread.clone(),
            turn.clone(),
            pending.clone(),
            next_id.clone(),
        );
        let (key, model, effort, cwd, system, resume) = (
            l.api_key.clone(),
            l.model.clone(),
            l.effort.clone(),
            l.cwd.clone(),
            l.system.clone(),
            l.resume.clone(),
        );
        let spawned = std::thread::Builder::new()
            .name("ai-codex-out".into())
            .spawn(move || {
                let send_line = |line: String| {
                    if let Ok(mut w) = w2.lock() {
                        let _ = w.write_all(line.as_bytes());
                        let _ = w.write_all(b"\n");
                        let _ = w.flush();
                    }
                };
                let emit = |e: AiEvent| {
                    let _ = tx.send(e);
                    wake();
                };
                let start_thread = |send_line: &dyn Fn(String)| match &resume {
                    Some(t) => send_line(codex::thread_resume(3, t)),
                    None => send_line(codex::thread_start(3, &model, &cwd, true)),
                };
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    // 턴 id(중단용).
                    if line.contains("\"turn/started\"") {
                        if let Ok(v) = nsql_settings::json::parse(line.trim()) {
                            let tid = super::jget(&v, "params")
                                .and_then(|p| super::jget(p, "turn"))
                                .map(|t| super::jstr(t, "id"))
                                .unwrap_or_default();
                            if let Ok(mut g) = tu2.lock() {
                                *g = (!tid.is_empty()).then_some(tid);
                            }
                        }
                    }
                    match codex::parse_line(&line) {
                        codex::Incoming::Response { id, result, error } => {
                            if let Some(e) = error {
                                emit(AiEvent::Error(format!("codex {id}: {e}")));
                                if id <= 3 {
                                    emit(AiEvent::Done {
                                        stop: "error".into(),
                                        ms: 0,
                                    });
                                }
                                continue;
                            }
                            match id {
                                1 => {
                                    send_line(codex::initialized());
                                    if key.is_empty() {
                                        start_thread(&send_line);
                                    } else {
                                        send_line(codex::login_api_key(2, &key));
                                    }
                                }
                                2 => start_thread(&send_line),
                                3 => {
                                    let tid = result
                                        .as_ref()
                                        .and_then(|r| super::jget(r, "thread"))
                                        .map(|t| super::jstr(t, "id"))
                                        .unwrap_or_default();
                                    if let Ok(mut g) = th2.lock() {
                                        *g = (!tid.is_empty()).then_some(tid.clone());
                                    }
                                    emit(AiEvent::Started {
                                        cli_session: (!tid.is_empty()).then_some(tid.clone()),
                                        note: "codex thread ready".into(),
                                    });
                                    let first = pd2.lock().ok().and_then(|mut p| p.take());
                                    if let Some((text, imgs)) = first {
                                        let id = id2.fetch_add(1, Ordering::Relaxed);
                                        send_line(codex::turn_start(
                                            id, &tid, &text, &imgs, &effort, &system,
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        codex::Incoming::ServerRequest { id, method, params } => {
                            match method.as_str() {
                                "item/tool/call" => {
                                    let (name, input) = codex::tool_call_args(&params);
                                    let (rtx, rrx) = mpsc::channel::<ToolReply>();
                                    emit(AiEvent::ToolRequest {
                                        id: super::jstr(&params, "itemId"),
                                        name,
                                        input,
                                        reply: rtx,
                                    });
                                    let w3 = w2.clone();
                                    let _ = std::thread::Builder::new()
                                        .name("ai-codex-tool".into())
                                        .spawn(move || {
                                            let r = rrx.recv().unwrap_or(ToolReply {
                                                output: "cancelled".into(),
                                                ok: false,
                                            });
                                            if let Ok(mut w) = w3.lock() {
                                                let _ = w.write_all(
                                                    codex::tool_call_response(&id, &r.output, r.ok)
                                                        .as_bytes(),
                                                );
                                                let _ = w.write_all(b"\n");
                                                let _ = w.flush();
                                            }
                                        });
                                }
                                "item/commandExecution/requestApproval"
                                | "item/fileChange/requestApproval" => {
                                    let (atx, arx) = mpsc::channel::<bool>();
                                    emit(AiEvent::Approval {
                                        id: super::jstr(&params, "itemId"),
                                        tool: method.clone(),
                                        summary: nsql_settings::json::dump(&params),
                                        reply: atx,
                                    });
                                    let w3 = w2.clone();
                                    let _ = std::thread::Builder::new()
                                        .name("ai-codex-approve".into())
                                        .spawn(move || {
                                            let ok = arx.recv().unwrap_or(false);
                                            if let Ok(mut w) = w3.lock() {
                                                let _ = w.write_all(
                                                    codex::approval_response(&id, ok).as_bytes(),
                                                );
                                                let _ = w.write_all(b"\n");
                                                let _ = w.flush();
                                            }
                                        });
                                }
                                "item/permissions/requestApproval" => {
                                    send_line(codex::permissions_response(&id))
                                }
                                _ => send_line(codex::response(
                                    &id,
                                    nsql_settings::json::Json::Obj(Vec::new()),
                                )),
                            }
                        }
                        codex::Incoming::Events(evs) => {
                            for e in evs {
                                let _ = tx.send(e);
                            }
                            wake();
                        }
                        codex::Incoming::Ignore => {}
                    }
                }
                emit(AiEvent::Note("codex exited".into()));
                emit(AiEvent::Done {
                    stop: "exit".into(),
                    ms: 0,
                });
            });
        if spawned.is_err() {
            let _ = child.kill();
            return Err("thread".into());
        }
        if let Some(err) = child.stderr.take() {
            let tail = self.stderr_tail.clone();
            let _ = std::thread::Builder::new()
                .name("ai-codex-err".into())
                .spawn(move || {
                    for line in BufReader::new(err).lines().map_while(Result::ok) {
                        if let Ok(mut t) = tail.lock() {
                            if t.len() >= 20 {
                                t.remove(0);
                            }
                            t.push(line);
                        }
                    }
                });
        }
        self.running = Running::Codex {
            child,
            writer,
            thread,
            turn,
            pending,
            next_id,
            effort: l.effort,
            system: l.system,
        };
        Ok(())
    }

    /// Codex 턴 — 스레드가 준비됐으면 바로 `turn/start` · 아니면 대기열(악수 뒤 읽기 스레드가 보낸다).
    pub(crate) fn send_codex(&mut self, text: &str, local_images: &[String]) -> Result<(), String> {
        use super::codex;
        let Running::Codex {
            writer,
            thread,
            pending,
            next_id,
            effort,
            system,
            ..
        } = &self.running
        else {
            return Err("codex not running".into());
        };
        let tid = thread.lock().ok().and_then(|t| t.clone());
        match tid {
            Some(tid) => {
                let id = next_id.fetch_add(1, Ordering::Relaxed);
                let line = codex::turn_start(id, &tid, text, local_images, effort, system);
                let mut w = writer.lock().map_err(|_| "lock".to_string())?;
                w.write_all(line.as_bytes())
                    .and_then(|()| w.write_all(b"\n"))
                    .and_then(|()| w.flush())
                    .map_err(|e| e.to_string())?;
            }
            None => {
                if let Ok(mut p) = pending.lock() {
                    *p = Some((text.to_string(), local_images.to_vec()));
                }
            }
        }
        self.busy = true;
        Ok(())
    }

    /// Codex 턴 중단(프로세스는 유지) — 턴 id를 모르면 false.
    pub(crate) fn interrupt_codex(&mut self) -> bool {
        use super::codex;
        let Running::Codex {
            writer,
            thread,
            turn,
            next_id,
            ..
        } = &self.running
        else {
            return false;
        };
        let (Some(tid), Some(tu)) = (
            thread.lock().ok().and_then(|t| t.clone()),
            turn.lock().ok().and_then(|t| t.clone()),
        ) else {
            return false;
        };
        let id = next_id.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut w) = writer.lock() {
            let _ = w.write_all(codex::turn_interrupt(id, &tid, &tu).as_bytes());
            let _ = w.write_all(b"\n");
            let _ = w.flush();
        }
        true
    }

    /// CLI 프로세스 시작(세션당 한 번 · 이후 턴은 stdin 줄).
    pub(crate) fn start_cli(&mut self, l: CliLaunch) -> Result<(), String> {
        self.stop();
        let mut cmd = nsql_ext::net::quiet_command(&l.program);
        cmd.args(&l.prefix)
            .args(&l.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for (k, v) in &l.env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", l.program))?;
        let stdin = child.stdin.take();
        let Some(out) = child.stdout.take() else {
            let _ = child.kill();
            return Err("no stdout".into());
        };
        let tx = self.tx.clone();
        let wake = self.wake.clone();
        let spawned = std::thread::Builder::new()
            .name("ai-cli-out".into())
            .spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    let evs = claude_cli::parse_line(&line);
                    let mut any = false;
                    for e in evs {
                        any = true;
                        let _ = tx.send(e);
                    }
                    if any {
                        wake();
                    }
                }
                let _ = tx.send(AiEvent::Note("cli exited".into()));
                let _ = tx.send(AiEvent::Done {
                    stop: "exit".into(),
                    ms: 0,
                });
                wake();
            });
        if spawned.is_err() {
            let _ = child.kill();
            return Err("thread".into());
        }
        if let Some(err) = child.stderr.take() {
            let tail = self.stderr_tail.clone();
            let _ = std::thread::Builder::new()
                .name("ai-cli-err".into())
                .spawn(move || {
                    for line in BufReader::new(err).lines().map_while(Result::ok) {
                        if let Ok(mut t) = tail.lock() {
                            if t.len() >= 20 {
                                t.remove(0);
                            }
                            t.push(line);
                        }
                    }
                });
        }
        self.running = Running::Cli { child, stdin };
        Ok(())
    }

    /// CLI 턴 보내기(stdin 한 줄).
    pub(crate) fn send_cli(
        &mut self,
        text: &str,
        images: &[(String, Vec<u8>)],
    ) -> Result<(), String> {
        let Running::Cli {
            stdin: Some(si), ..
        } = &mut self.running
        else {
            return Err("cli not running".into());
        };
        let line = claude_cli::user_turn_line(text, images);
        si.write_all(line.as_bytes())
            .and_then(|()| si.write_all(b"\n"))
            .and_then(|()| si.flush())
            .map_err(|e| e.to_string())?;
        self.busy = true;
        Ok(())
    }

    /// API 턴(스레드) — 사용자 메시지를 대화에 붙이고 도구 루프를 돈다.
    pub(crate) fn start_api(
        &mut self,
        l: ApiLaunch,
        text: &str,
        attachments: &[Attachment],
    ) -> Result<(), String> {
        if let Running::Api { .. } = self.running {
            if self.busy {
                return Err("busy".into());
            }
        }
        if let Ok(mut m) = self.api_messages.lock() {
            m.push(claude_api::user_message(text, attachments));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let messages = self.api_messages.clone();
        let tx = self.tx.clone();
        let wake = self.wake.clone();
        let stop2 = stop.clone();
        let spawned = std::thread::Builder::new()
            .name("ai-api".into())
            .spawn(move || {
                // 스레드 동안 대화 사본으로 돌고 끝에 되돌려 넣는다(잠금을 길게 쥐지 않게).
                let mut msgs = messages.lock().map(|m| m.clone()).unwrap_or_default();
                let (wtx, wrx) = mpsc::channel::<AiEvent>();
                // 이벤트마다 깨우기 — 전달 스레드 없이 바로 보내고 깨운다.
                let forward = std::thread::Builder::new()
                    .name("ai-api-fwd".into())
                    .spawn({
                        let tx = tx.clone();
                        let wake = wake.clone();
                        move || {
                            for e in wrx {
                                let _ = tx.send(e);
                                wake();
                            }
                        }
                    });
                claude_api::run_turn(
                    &l.base_url,
                    &l.api_key,
                    &l.workspace,
                    &l.model,
                    &l.effort,
                    &l.system,
                    &mut msgs,
                    l.timeout_secs,
                    l.max_tokens,
                    &l.prices,
                    &wtx,
                    &stop2,
                );
                drop(wtx);
                if let Ok(mut m) = messages.lock() {
                    *m = msgs;
                }
                if let Ok(h) = forward {
                    let _ = h.join();
                }
                wake();
            });
        if spawned.is_err() {
            return Err("thread".into());
        }
        self.running = Running::Api { stop };
        self.busy = true;
        Ok(())
    }

    /// 각본 상대(헤드리스 V1 · `NSQL_AI_MOCK=1`): 글 조각 → 도구 요청(get_setting) → 결과 → 끝.
    pub(crate) fn start_mock(&mut self, text: &str) {
        let tx = self.tx.clone();
        let wake = self.wake.clone();
        let text = text.to_string();
        let _ = std::thread::Builder::new()
            .name("ai-mock".into())
            .spawn(move || {
                let send = |e: AiEvent| {
                    let _ = tx.send(e);
                    wake();
                };
                send(AiEvent::Started {
                    cli_session: Some("mock".into()),
                    note: "mock host".into(),
                });
                for w in ["Mock ", "reply ", "to: ", &text] {
                    send(AiEvent::TextDelta(w.to_string()));
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                let (rtx, rrx) = mpsc::channel();
                send(AiEvent::ToolCall {
                    id: "m1".into(),
                    name: "get_setting".into(),
                    input: "{\"key\":\"editor.word_wrap\"}".into(),
                });
                send(AiEvent::ToolRequest {
                    id: "m1".into(),
                    name: "get_setting".into(),
                    input: "{\"key\":\"editor.word_wrap\"}".into(),
                    reply: rtx,
                });
                let r = rrx
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .unwrap_or(ToolReply {
                        output: "timeout".into(),
                        ok: false,
                    });
                send(AiEvent::ToolResult {
                    id: "m1".into(),
                    name: "get_setting".into(),
                    output: r.output,
                    ok: r.ok,
                });
                send(AiEvent::TextDelta(" done.".into()));
                send(AiEvent::Usage(Usage {
                    input: 10,
                    output: 5,
                    cache_read: 0,
                    cache_write: 0,
                    cost_usd: 0.0001,
                }));
                send(AiEvent::Done {
                    stop: "end_turn".into(),
                    ms: 42,
                });
            });
        self.running = Running::Mock;
        self.busy = true;
    }

    /// ■ — 진행 중인 턴을 끊는다(CLI는 프로세스 종료 · API는 깃발).
    pub(crate) fn stop(&mut self) {
        match std::mem::replace(&mut self.running, Running::Idle) {
            Running::Cli { mut child, stdin } => {
                drop(stdin);
                let _ = child.kill();
                let _ = child.wait();
            }
            Running::Api { stop } => stop.store(true, Ordering::Relaxed),
            Running::Codex {
                mut child, writer, ..
            } => {
                drop(writer);
                let _ = child.kill();
                let _ = child.wait();
            }
            Running::Idle | Running::Mock => {}
        }
        self.busy = false;
    }

    pub(crate) fn stderr_text(&self) -> String {
        self.stderr_tail
            .lock()
            .map(|t| t.join("\n"))
            .unwrap_or_default()
    }

    /// API 대화를 비운다(새 세션).
    pub(crate) fn reset_api(&mut self) {
        if let Ok(mut m) = self.api_messages.lock() {
            m.clear();
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_round_trip_drives_tool_request() {
        let woke = Arc::new(AtomicBool::new(false));
        let w2 = woke.clone();
        let mut h = Host::new(Arc::new(move || w2.store(true, Ordering::Relaxed)));
        h.start_mock("hi");
        let mut text = String::new();
        let mut done = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !done && std::time::Instant::now() < deadline {
            match h.rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok(AiEvent::TextDelta(t)) => text.push_str(&t),
                Ok(AiEvent::ToolRequest { reply, .. }) => {
                    let _ = reply.send(ToolReply {
                        output: "off".into(),
                        ok: true,
                    });
                }
                Ok(AiEvent::Done { .. }) => done = true,
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
        }
        assert!(done, "mock finished");
        assert_eq!(text, "Mock reply to: hi done.");
        assert!(woke.load(Ordering::Relaxed));
        let (prog, _) = resolve_cli("");
        assert!(!prog.is_empty());
    }
}
