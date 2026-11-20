//! ★ 호스트(docs/110 §5·§14-1): 한 세션의 **상대**를 돌린다 — `Cli`(자식 `claude` 프로세스 · stream-json) · `Api`(Messages API
//! 턴 스레드) · `Mock`(네트워크·CLI 없이 헤드리스 V1용 각본 · `NSQL_AI_MOCK=1`). 셋 다 [`AiEvent`]를 같은 채널로 보내고 깨운다.

use super::{claude_api, claude_cli, AiEvent, Attachment, ToolReply, Usage};
use nsql_ext::procjob::KillJob;
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
        /// 후손까지 끝내는 묶음(Windows 작업 개체 · T-479 · Unix는 빈 값 = 프로세스 그룹이 대신).
        job: KillJob,
    },
    Api {
        stop: Arc<AtomicBool>,
    },
    /// `codex app-server`(stdio JSONL) — 쓰기는 `writer` 공유 · 스레드 id·턴 id는 읽기 스레드가 채운다.
    Codex {
        child: std::process::Child,
        job: KillJob,
        writer: Arc<Mutex<std::process::ChildStdin>>,
        thread: Arc<Mutex<Option<String>>>,
        turn: Arc<Mutex<Option<String>>>,
        /// 스레드가 준비되기 전에 들어온 첫 턴(글 · 로컬 이미지 경로).
        pending: PendingTurn,
        next_id: Arc<std::sync::atomic::AtomicU64>,
        system: String,
    },
    Mock {
        stop: Arc<AtomicBool>,
    },
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
/// 종료. 키가 비면 CLI 자체 로그인으로 조회. 30초 상한 · 콘솔 창 없음.
pub(crate) fn codex_list_model_infos(
    program: &str,
    prefix: &[String],
    api_key: &str,
) -> Result<Vec<super::codex::ModelInfo>, String> {
    use super::codex;
    let mut cmd = nsql_ext::net::quiet_command(program);
    cmd.args(prefix)
        .arg("app-server")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("{program}: {e}"))?;
    // 후손까지(T-479): 이 함수가 어떻게 끝나든(오류 길 포함) 묶음이 닫히며 app-server가 띄운 프로세스도 함께 끝난다.
    let job = KillJob::attach(&child);
    let (Some(mut stdin), Some(out)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        return Err("no stdio".into());
    };
    if let Err(e) = write_line(&mut stdin, &codex::initialize(1)) {
        let _ = child.kill();
        return Err(e);
    }
    let (tx, rx) = mpsc::channel::<Result<Vec<super::codex::ModelInfo>, String>>();
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
                            let _ = tx.send(Ok(codex::model_infos(result.as_ref())));
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
    job.kill();
    let _ = child.kill();
    let _ = child.wait();
    if let Ok(h) = reader {
        let _ = h.join();
    }
    r
}

/// 모델 id만([`codex_list_model_infos`] · E2E — 프로필 검증은 노력도까지 받는다 = T-486).
#[cfg(test)]
pub(crate) fn codex_list_models(
    program: &str,
    prefix: &[String],
    api_key: &str,
) -> Result<Vec<String>, String> {
    codex_list_model_infos(program, prefix, api_key).map(|v| v.into_iter().map(|m| m.id).collect())
}

/// 자식 프로세스 끝내기 — 종료 신호는 바로 보내고 **종료 대기는 다른 스레드에서**(11-14 "세션을 고를 때 잠시 멈춤"):
/// `kill` 뒤 `wait`는 핸들 정리·백신 검사로 수십~수백 ms 걸릴 수 있는데 종전에는 UI 스레드가 기다렸다(세션 전환·새 세션·모드
/// 변경마다). 스레드를 못 만들면 기다리지 않고 놓는다(프로세스는 이미 종료 신호를 받았다).
///
/// ★ 손자까지(11-15 · 가짜 CLI 재현): CLI가 띄운 셸·긴 명령은 CLI만 죽이면 고아로 남았다(세션 전환·■·앱 종료 뒤에도 생존) →
/// Unix는 자식을 자기 프로세스 그룹의 머리로 띄우고([`own_group`]) 여기서 **그룹째** 끝낸다. Windows는 띄운 직후 넣어 둔
/// 작업 개체([`KillJob`] · T-479 · 11-20)를 끝낸다 — node(CLI) · Codex가 띄운 셸 · `nsql mcp-bridge`까지. 묶음은 종료 대기가 끝난
/// 뒤에 닫는다(닫히는 순간에도 남은 것이 끝난다).
fn reap(mut child: std::process::Child, job: KillJob) {
    kill_group(&child);
    job.kill();
    let _ = child.kill();
    let _ = std::thread::Builder::new()
        .name("ai-reap".into())
        .spawn(move || {
            // 묶음은 이 스레드가 쥐고 있다가 종료 대기가 끝난 뒤(스코프 끝)에 닫는다 — `drop(job)`은 `KillJob`이 빈 값인
            // 비Windows에서 clippy `drop_non_drop`에 걸린다(11-21 CI).
            let _job = job;
            let _ = child.wait();
        });
}

/// 자식을 **자기 프로세스 그룹의 머리**로 띄운다(Unix · pgid = 자식 pid) — [`reap`]이 그룹째 끝낼 수 있게. 다른 OS는 그대로.
fn own_group(cmd: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = cmd;
    }
}

/// [`own_group`]으로 띄운 자식의 그룹 전체에 종료 신호(Unix `kill(-pgid, SIGKILL)`). 아직 `wait`하지 않은 자식만 넘긴다
/// (pid가 재사용되지 않는 동안 · 그룹이 이미 없으면 ESRCH로 끝). pid 0·1은 보내지 않는다(0 = 내 그룹 · -1 = 전부).
fn kill_group(child: &std::process::Child) {
    #[cfg(unix)]
    {
        extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        const SIGKILL: i32 = 9;
        if let Ok(pid) = i32::try_from(child.id()) {
            if pid > 1 {
                // SAFETY: 정수 둘을 받는 libc `kill` — 메모리를 건드리지 않는다. 대상은 우리가 그룹 머리로 띄운 자식의 그룹뿐.
                unsafe {
                    kill(-pid, SIGKILL);
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child;
    }
}

/// CLI 출력 한 줄씩 → 사건(읽기 스레드 본문 · 시험할 수 있게 떼어 냄 = T-485). **일부러 내린 프로세스**(`retire_cli`가 세대를
/// 올렸다 = `gen != mine`)의 줄과 종료 알림은 버린다 — 새 프로세스의 턴에 옛 프로세스의 "cli exited"·`Done`이 끼어들지 않게(11-16).
fn pump_cli<R: BufRead>(
    out: R,
    gen: &std::sync::atomic::AtomicU64,
    mine: u64,
    tx: &mpsc::Sender<AiEvent>,
    wake: &Wake,
) {
    for line in out.lines().map_while(Result::ok) {
        if gen.load(Ordering::Relaxed) != mine {
            return;
        }
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
    if gen.load(Ordering::Relaxed) != mine {
        return;
    }
    let _ = tx.send(AiEvent::Note("cli exited".into()));
    let _ = tx.send(AiEvent::Done {
        stop: "exit".into(),
        ms: 0,
    });
    wake();
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
    /// API 대화의 세대(11-15) — 새 턴 시작·대화 교체마다 올린다. 턴 스레드는 끝날 때 **자기가 시작한 세대일 때만** 대화를
    /// 되돌려 넣는다: 종전에는 ■ 뒤 늦게 끝난 스레드가 그 사이 시작한 새 턴(또는 다른 세션으로 바꾼 대화)을 통째로 덮어썼다.
    pub api_gen: Arc<std::sync::atomic::AtomicU64>,
    /// CLI 프로세스의 세대(11-16) — 조건이 바뀌어 **일부러 내리고 다시 띄울 때**([`Host::retire_cli`]) 올린다. 출력 스레드는
    /// 자기 세대일 때만 사건을 보낸다: 내린 프로세스의 늦은 "cli exited"·종료 사건이 새 프로세스의 턴에 끼어들지 않게
    /// (메모 줄이 남고, 종료 사건이 도는 턴의 busy를 풀었다).
    pub cli_gen: Arc<std::sync::atomic::AtomicU64>,
    /// CLI stderr 꼬리(실패 진단).
    pub stderr_tail: Arc<Mutex<Vec<String>>>,
    /// ★ 턴 도중에 상대에게 **이미 넣은** 요청(11-15 · [`Host::steer`]): (식별자 · 세션 id · 글) — `SteerState` 사건으로 풀린다.
    /// 남아 있는 동안 CLI는 지금 턴이 끝나도 곧바로 다음 턴을 돈다(글만 답하던 턴 · 실측) → [`Host::turn_done`]이 busy를 유지.
    pub steer_keys: Vec<(String, String, String)>,
    /// Codex `turn/steer` 요청 id(응답을 읽기 스레드가 `SteerState`로 바꾼다).
    steer_ids: Arc<Mutex<Vec<u64>>>,
    /// 넣는 줄 식별자 일련번호.
    steer_seq: u64,
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
    /// API 키 또는 Bearer(WIF면 자리만 — 스레드에서 `ensure_access`로 채운다).
    pub auth: claude_api::ApiAuth,
    /// WIF 프로필이면 (설정 · 프로필 이름) — 턴마다 토큰 확보·갱신(볼트).
    pub wif: Option<(super::wif::Wif, String)>,
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
            api_gen: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            cli_gen: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            stderr_tail: Arc::new(Mutex::new(Vec::new())),
            steer_keys: Vec::new(),
            steer_ids: Arc::new(Mutex::new(Vec::new())),
            steer_seq: 0,
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
        own_group(&mut cmd);
        cmd.args(&l.prefix)
            .arg("app-server")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", l.program))?;
        // 아래 오류 길에서 일찍 돌아가도 묶음이 닫히며 후손까지 끝난다(T-479).
        let job = KillJob::attach(&child);
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
        let steer_ids = self.steer_ids.clone();
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
                    Some(t) => send_line(codex::thread_resume(3, t, &model)),
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
                            // 턴 도중 넣기(`turn/steer`)의 응답 = 성공이면 같은 턴에 반영됨 · 오류면 턴이 이미 끝남(대기 풀 유지).
                            let steered = steer_ids.lock().is_ok_and(|mut g| {
                                let had = g.contains(&id);
                                g.retain(|x| *x != id);
                                had
                            });
                            if steered {
                                emit(AiEvent::SteerState {
                                    key: format!("codex-{id}"),
                                    started: error.is_none(),
                                });
                                continue;
                            }
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
                                            id, &tid, &text, &imgs, &model, &effort, &system,
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
            job,
            writer,
            thread,
            turn,
            pending,
            next_id,
            system: l.system,
        };
        Ok(())
    }

    /// Codex 턴 — 스레드가 준비됐으면 바로 `turn/start` · 아니면 대기열(악수 뒤 읽기 스레드가 보낸다).
    /// 모델·노력도는 **턴마다 지금 값**(11-15 · 띄울 때 값이 아니다 — 살아 있는 프로세스에서 바꾼 것이 바로 먹게).
    pub(crate) fn send_codex(
        &mut self,
        text: &str,
        local_images: &[String],
        model: &str,
        effort: &str,
    ) -> Result<(), String> {
        use super::codex;
        let Running::Codex {
            writer,
            thread,
            pending,
            next_id,
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
                let line = codex::turn_start(id, &tid, text, local_images, model, effort, system);
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

    /// ★ 진행 중인 턴에 사용자 글을 넣는다(11-15 · 실측: CLI = stdin에 `uuid` 붙인 줄 · Codex = `turn/steer`). 넣었으면 참 —
    /// 그 뒤 `SteerState` 사건이 온다(`steer_keys`에 등재). 상대가 못 받는 상태(모의·API·스레드/턴 id 모름·쓰기 실패)면 거짓 =
    /// 호출한 쪽은 종전대로 대기 풀에 둔다. 글만(첨부가 있는 요청은 넣지 않는다 — 호출한 쪽 판정).
    pub(crate) fn steer(&mut self, sid: &str, text: &str) -> bool {
        use super::codex;
        if !self.busy {
            return false;
        }
        self.steer_seq += 1;
        let key = match &mut self.running {
            Running::Cli {
                stdin: Some(si), ..
            } => {
                // uuid 꼴(8-4-4-4-12 · 16진) — CLI가 그대로 `command_uuid`로 돌려준다.
                let t = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos());
                let key = format!(
                    "{:08x}-{:04x}-4{:03x}-8{:03x}-{:012x}",
                    (t >> 32) as u32,
                    (t >> 16) as u16,
                    (t as u16) & 0x0fff,
                    (self.steer_seq as u16) & 0x0fff,
                    (t as u64) & 0xffff_ffff_ffff
                );
                let line = claude_cli::steer_line(text, &key);
                if si
                    .write_all(line.as_bytes())
                    .and_then(|()| si.write_all(b"\n"))
                    .and_then(|()| si.flush())
                    .is_err()
                {
                    return false;
                }
                key
            }
            Running::Codex {
                writer,
                thread,
                turn,
                next_id,
                ..
            } => {
                let (Some(tid), Some(tu)) = (
                    thread.lock().ok().and_then(|t| t.clone()),
                    turn.lock().ok().and_then(|t| t.clone()),
                ) else {
                    return false;
                };
                let id = next_id.fetch_add(1, Ordering::Relaxed);
                // 응답이 쓰기보다 먼저 읽히지 않게 id부터 등재.
                if let Ok(mut g) = self.steer_ids.lock() {
                    g.push(id);
                }
                let ok = writer.lock().is_ok_and(|mut w| {
                    w.write_all(codex::turn_steer(id, &tid, &tu, text).as_bytes())
                        .and_then(|()| w.write_all(b"\n"))
                        .and_then(|()| w.flush())
                        .is_ok()
                });
                if !ok {
                    if let Ok(mut g) = self.steer_ids.lock() {
                        g.retain(|x| *x != id);
                    }
                    return false;
                }
                format!("codex-{id}")
            }
            _ => return false,
        };
        self.steer_keys
            .push((key, sid.to_string(), text.to_string()));
        true
    }

    /// 턴 끝(`Done`) — busy를 내린다. 단 CLI에 넣어 둔 줄이 아직 읽히지 않았으면 CLI가 곧바로 다음 턴을 돌므로(실측 ⓓ)
    /// busy를 유지한다(대기 풀 보내기·새 요청이 그 턴과 겹치지 않게). `exited` = 프로세스가 끝남 → 넣어 둔 줄도 사라졌다.
    pub(crate) fn turn_done(&mut self, exited: bool) {
        if exited || !matches!(self.running, Running::Cli { .. }) {
            self.steer_keys.clear();
        }
        self.busy = !self.steer_keys.is_empty();
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

    /// 떠 있는 CLI 프로세스를 **다시 띄우려고** 내린다(모델·노력도가 바뀜 · 11-16): 세대를 올려 그 프로세스의 남은 사건을 버린다.
    pub(crate) fn retire_cli(&mut self) {
        self.cli_gen.fetch_add(1, Ordering::Relaxed);
        self.stop();
    }

    /// CLI 프로세스 시작(세션당 한 번 · 이후 턴은 stdin 줄).
    pub(crate) fn start_cli(&mut self, l: CliLaunch) -> Result<(), String> {
        self.stop();
        let mut cmd = nsql_ext::net::quiet_command(&l.program);
        own_group(&mut cmd);
        cmd.args(&l.prefix)
            .args(&l.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        for (k, v) in &l.env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().map_err(|e| format!("{}: {e}", l.program))?;
        // 아래 오류 길에서 일찍 돌아가도 묶음이 닫히며 후손까지 끝난다(T-479).
        let job = KillJob::attach(&child);
        let stdin = child.stdin.take();
        let Some(out) = child.stdout.take() else {
            let _ = child.kill();
            return Err("no stdout".into());
        };
        let tx = self.tx.clone();
        let wake = self.wake.clone();
        let cli_gen = self.cli_gen.clone();
        let mine = cli_gen.load(Ordering::Relaxed);
        let spawned = std::thread::Builder::new()
            .name("ai-cli-out".into())
            .spawn(move || pump_cli(BufReader::new(out), &cli_gen, mine, &tx, &wake));
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
        self.running = Running::Cli { child, stdin, job };
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
        let gen = self.api_gen.clone();
        let my_gen = match self.api_messages.lock() {
            Ok(mut m) => {
                // ★ ■로 끊긴 턴은 꼬리가 user로 남는다(받던 답을 버림 · tool_result까지 넣고 멈춤) — 그대로 붙이면 user가
                //   연달아 두 개인 본문이 된다(11-15 · 대화 재구성 경로와 같은 병합으로 한 메시지로).
                claude_api::push_merged(&mut m, claude_api::user_message(text, attachments));
                gen.fetch_add(1, Ordering::Relaxed) + 1
            }
            Err(_) => gen.fetch_add(1, Ordering::Relaxed) + 1,
        };
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
                        let stop = stop2.clone();
                        move || {
                            for e in wrx {
                                // ★ ■ 뒤의 사건은 버린다(협업 V1 11-15): 멈춘 턴의 늦은 글·도구 호출이 "멈춤" 아래에 붙거나,
                                //   그 사이 시작한 새 턴에 섞였다. UI는 ■ 때 이미 턴을 닫았다(도구 요청은 보낸 쪽이 사라져 취소).
                                if stop.load(Ordering::Relaxed) {
                                    continue;
                                }
                                let _ = tx.send(e);
                                wake();
                            }
                        }
                    });
                let auth = match &l.wif {
                    Some((cfg, profile)) => {
                        match super::wif::ensure_access(cfg, &l.base_url, &l.workspace, profile) {
                            Ok(tok) => claude_api::ApiAuth::Bearer(tok),
                            Err(e) => {
                                let _ = wtx.send(AiEvent::Error(format!("WIF: {e}")));
                                let _ = wtx.send(AiEvent::Done {
                                    stop: "error".into(),
                                    ms: 0,
                                });
                                drop(wtx);
                                if let Ok(h) = forward {
                                    let _ = h.join();
                                }
                                wake();
                                return;
                            }
                        }
                    }
                    None => l.auth.clone(),
                };
                claude_api::run_turn(
                    &l.base_url,
                    &auth,
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
                    // 그 사이 새 턴이 시작됐거나 대화가 바뀌었으면 건드리지 않는다(세대 확인은 잠금 안에서).
                    if gen.load(Ordering::Relaxed) == my_gen {
                        *m = msgs;
                    }
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
    pub(crate) fn start_mock(&mut self, text: &str, prices: String, model: String) {
        // ■(11-15): API 턴과 같은 깃발 — 섰으면 다음 확인 자리에서 `cancelled`로 끝내고 늦은 사건을 내지 않는다.
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
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
                // 멈췄으면 끝 사건 하나만 내고 참(호출한 쪽이 바로 return).
                let cancelled = || {
                    if !stopped.load(Ordering::Relaxed) {
                        return false;
                    }
                    send(AiEvent::Done {
                        stop: "cancelled".into(),
                        ms: 0,
                    });
                    true
                };
                send(AiEvent::Started {
                    cli_session: Some("mock".into()),
                    note: "mock host".into(),
                });
                for w in ["Mock ", "reply ", "to: ", &text] {
                    send(AiEvent::TextDelta(w.to_string()));
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                // ★ 모의 도구 호출 목록(헤드리스 V1 · 협업 제안 11-12): `NSQL_AI_MOCK_TOOL="name:{json};name2:{json}"` — 없으면 종전
                //   `get_setting` 하나. 결과는 " → <앞 200자>" 글로 되돌려 덤프로 판정한다(도구 결과 줄은 basic 레벨에서 숨기므로).
                let calls: Vec<(String, String)> = std::env::var("NSQL_AI_MOCK_TOOL")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .map(|v| {
                        v.split(';')
                            .filter_map(|c| {
                                let (n, a) = c.split_once(':')?;
                                Some((n.trim().to_string(), a.trim().to_string()))
                            })
                            .collect()
                    })
                    .unwrap_or_else(|| {
                        vec![(
                            "get_setting".to_string(),
                            "{\"key\":\"editor.word_wrap\"}".to_string(),
                        )]
                    });
                // 도구 호출 전 지연(V1 · `NSQL_AI_MOCK_TOOL_DELAY_MS`): 턴이 도는 동안 요청을 하나 더 보낼 틈을 만든다.
                if let Some(ms) = std::env::var("NSQL_AI_MOCK_TOOL_DELAY_MS")
                    .ok()
                    .and_then(|v| v.trim().parse::<u64>().ok())
                {
                    // 잘게 자서 ■를 50 ms 안에 본다.
                    let until = std::time::Instant::now()
                        + std::time::Duration::from_millis(ms.min(10_000));
                    while std::time::Instant::now() < until && !stopped.load(Ordering::Relaxed) {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                }
                for (k, (name, input)) in calls.into_iter().enumerate() {
                    if cancelled() {
                        return;
                    }
                    let id = format!("m{}", k + 1);
                    let (rtx, rrx) = mpsc::channel();
                    send(AiEvent::ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });
                    send(AiEvent::ToolRequest {
                        id: id.clone(),
                        name: name.clone(),
                        input,
                        reply: rtx,
                    });
                    let r = rrx
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap_or(ToolReply {
                            output: "timeout".into(),
                            ok: false,
                        });
                    if cancelled() {
                        return;
                    }
                    let head: String = r.output.chars().take(200).collect();
                    send(AiEvent::ToolResult {
                        id,
                        name: name.clone(),
                        output: r.output,
                        ok: r.ok,
                    });
                    if std::env::var_os("NSQL_AI_MOCK_TOOL").is_some() {
                        send(AiEvent::TextDelta(format!("\n→ {name}: {head}")));
                    }
                    // 진행 중 턴 반영(V1 · API 턴 루프와 같은 자리): 도구 결과 뒤에 대기 요청을 묻고, 받았으면 블록 수를 글로.
                    let (stx, srx) = mpsc::channel();
                    send(AiEvent::SteerRequest { reply: stx });
                    if let Ok(blocks) = srx.recv_timeout(std::time::Duration::from_secs(5)) {
                        if !blocks.is_empty() {
                            send(AiEvent::TextDelta(format!(
                                "\n[steered {} block(s)]",
                                blocks.len()
                            )));
                        }
                    }
                }
                if cancelled() {
                    return;
                }
                send(AiEvent::TextDelta(" done.".into()));
                send(AiEvent::Usage(Usage {
                    input: 10,
                    output: 5,
                    cache_read: 0,
                    cache_write: 0,
                    // V1: 비용 표 반영 판정용 — API 스레드와 같은 추정 함수(`ai.prices`).
                    cost_usd: Usage::estimate_cost_in(&prices, &model, 10, 5, 0),
                }));
                send(AiEvent::Done {
                    stop: "end_turn".into(),
                    ms: 42,
                });
            });
        self.running = Running::Mock { stop };
        self.busy = true;
    }

    /// ■ — 진행 중인 턴을 끊는다(CLI는 프로세스 종료 · API는 깃발).
    pub(crate) fn stop(&mut self) {
        match std::mem::replace(&mut self.running, Running::Idle) {
            Running::Cli { child, stdin, job } => {
                drop(stdin);
                reap(child, job);
            }
            Running::Api { stop } => stop.store(true, Ordering::Relaxed),
            Running::Codex {
                child, writer, job, ..
            } => {
                drop(writer);
                reap(child, job);
            }
            Running::Mock { stop } => stop.store(true, Ordering::Relaxed),
            Running::Idle => {}
        }
        self.steer_keys.clear();
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
            self.api_gen.fetch_add(1, Ordering::Relaxed);
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
        h.start_mock(
            "hi",
            nsql_settings::AI_PRICES_DEFAULT.into(),
            "claude-opus-5-5".into(),
        );
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

    /// ★ T-485(11-16 CLI 세대): 스스로 끝난 프로세스는 "cli exited" + `Done{exit}`를 남기고, **일부러 내린**(세대가 지난) 프로세스는
    /// 아무것도 남기지 않는다 — 깨움도 없다. `retire_cli`는 세대를 올린다.
    #[test]
    fn retired_cli_generation_is_silent() {
        use std::sync::atomic::AtomicU64;
        let run = |gen_now: u64, mine: u64| {
            let (tx, rx) = mpsc::channel::<AiEvent>();
            let woke = Arc::new(AtomicBool::new(false));
            let w2 = woke.clone();
            let wake: Wake = Arc::new(move || w2.store(true, Ordering::Relaxed));
            let gen = AtomicU64::new(gen_now);
            pump_cli(std::io::Cursor::new("not json\n\n"), &gen, mine, &tx, &wake);
            drop(tx);
            (rx.iter().collect::<Vec<_>>(), woke.load(Ordering::Relaxed))
        };
        let (evs, woke) = run(3, 3);
        assert!(woke);
        assert!(
            matches!(evs.as_slice(), [.., AiEvent::Note(n), AiEvent::Done { stop, .. }] if n == "cli exited" && stop == "exit"),
            "같은 세대 = 종료 알림"
        );
        let (evs, woke) = run(4, 3);
        assert!(evs.is_empty() && !woke, "지난 세대 = 조용히");
        let mut h = Host::new(Arc::new(|| {}));
        let g0 = h.cli_gen.load(Ordering::Relaxed);
        h.retire_cli();
        assert_eq!(h.cli_gen.load(Ordering::Relaxed), g0 + 1);
    }

    /// 턴 도중 넣기(11-15): 상대가 없으면(한가함·모의) 넣지 않는다 · 턴 끝에서 CLI가 아니면 넣어 둔 표식을 비우고 busy를 내린다.
    #[test]
    fn steer_needs_a_live_peer_and_turn_done_clears() {
        let mut h = Host::new(Arc::new(|| {}));
        assert!(!h.steer("s1", "hi"), "한가한 호스트");
        h.busy = true;
        assert!(!h.steer("s1", "hi"), "상대 없음(Idle)");
        h.steer_keys.push(("k".into(), "s1".into(), "hi".into()));
        h.turn_done(false);
        assert!(
            !h.busy && h.steer_keys.is_empty(),
            "CLI가 아니면 유지하지 않는다"
        );
    }
}
