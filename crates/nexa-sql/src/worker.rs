//! 워커 스레드 — Runner(엔진 + 세션)를 소유한다. UI는 명령을 보내고 이벤트를 받는다.
//! 세션 객체는 이 스레드 안에서만 만들어지고 산다(`Send` 불요).
//!
//! 연결 프로필(T-16b): `Connect("prod")`처럼 **이름**이 오면 `nsql-vault`에서 푼다. 저장소는 호출마다
//! 연다 — 다른 창·CLI 인스턴스가 방금 저장한 프로필도 그대로 보인다(메모리 캐시 없음).
//!
//! 영향도 분리(사용자 09-14):
//! - **패닉 격리** — 명령 하나가 드라이버 안에서 패닉해도 워커 스레드는 살아남는다(`catch_unwind`). 세션은 버리고(상태 불명)
//!   오류 이벤트 + `Disconnected`를 내보내 UI가 `busy`에 갇히지 않는다.
//! - **실행 전 빠른 판정**(`Cmd::Run.preflight`) — UI가 신호등이 초록이 아니라고 알리거나 직전 실행이 접속성 오류였으면,
//!   쿼리를 드라이버에 넘기기 전에 호스트:포트 TCP 연결을 `probe.timeout` 안에 먼저 본다. 실패면 드라이버의 긴 타임아웃을
//!   기다리지 않고 바로 오류를 낸다(쿼리는 보내지 않음).

use crate::probe;
use nsql_core::{DbError, Dialect, Session};
use nsql_i18n::{tf, Msg};
use nsql_run::{Opener, RunEvent, Runner};
use nsql_script::ConnectSpec;
use nsql_vault::Vault;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::time::Duration;

pub(crate) enum Cmd {
    /// 접속 문자열 **또는 프로필 이름**(실행 인자 경로).
    Connect(String),
    /// 접속 패널이 조립한 스펙으로 접속(세션 유지). 지금 세션이 **같은 서버**(방언·호스트·포트·DB·사용자·역할)면
    /// 그대로 두고 성공을 알린다 — `reconnect_same`이면 닫고 다시 접속(설정 `connect.reconnect_same` · 사용자 09-14).
    /// 다른 서버면 기존 세션을 닫고 새로 접속한다.
    ConnectSpec {
        spec: ConnectSpec,
        reconnect_same: bool,
    },
    /// 접속만 해 보고 끊는다 — CLI `nsql conn test`와 같은 `nsql_run::test_connection`.
    Test(ConnectSpec),
    Disconnect,
    /// 스펙을 프로필로 저장(접속 패널 · 비밀번호 저장 여부는 스펙의 password 유무).
    SaveSpec {
        name: String,
        spec: ConnectSpec,
    },
    Run {
        src: String,
        /// `Some(timeout)` = 실행 전 호스트:포트 빠른 판정(신호등이 초록이 아닐 때 UI가 켠다).
        preflight: Option<Duration>,
    },
    Quit,
}

/// 접속 계열 명령의 결과 — 접속 패널 상태줄의 원천(실행 이벤트와 분리).
#[derive(Debug)]
pub(crate) enum ConnOutcome {
    Connected(String),
    ConnectFailed(String),
    TestOk {
        description: String,
        elapsed_s: String,
    },
    TestFailed(String),
    Disconnected,
    Saved(String),
    SaveFailed(String),
}

fn err(message: String) -> RunEvent {
    RunEvent::Error {
        index: 0,
        line: 0,
        error: DbError {
            code: None,
            message,
            position: None,
        },
    }
}

/// 같은 서버인가 — 비밀번호만 빼고 비교(같으면 세션을 다시 열 이유가 없다).
fn same_server(a: &ConnectSpec, b: &ConnectSpec) -> bool {
    a.dialect == b.dialect
        && a.host == b.host
        && a.port == b.port
        && a.database == b.database
        && a.user == b.user
        && a.role == b.role
}

/// 이름이면 저장소에서, 아니면 접속 문자열 파싱.
fn resolve_target(target: &str, default_dialect: Dialect) -> Result<ConnectSpec, String> {
    if nsql_vault::is_profile_name(target) {
        let v = Vault::open_default().map_err(|e| e.to_string())?;
        if let Some(spec) = v.resolve(target).map_err(|e| e.to_string())? {
            return Ok(spec);
        }
    }
    nsql_drivers::parse_target(target, default_dialect)
}

/// 저장된 프로필 이름(상태줄 안내용 · 실패하면 빈 목록).
pub(crate) fn profile_names() -> Vec<String> {
    Vault::open_default()
        .and_then(|v| v.list())
        .map(|l| l.into_iter().map(|p| p.name).collect())
        .unwrap_or_default()
}

pub(crate) struct Handle {
    tx: mpsc::Sender<Cmd>,
    /// 명령 완료 신호(상태 메시지 옵션).
    pub done: mpsc::Receiver<Option<String>>,
    /// 접속 계열 결과.
    pub conn: mpsc::Receiver<ConnOutcome>,
}

impl Handle {
    pub(crate) fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
}

pub(crate) fn spawn(
    default_dialect: Dialect,
    wake: Box<dyn Fn() + Send>,
) -> (Handle, mpsc::Receiver<RunEvent>) {
    let (tx, rx) = mpsc::channel::<Cmd>();
    let (etx, erx) = mpsc::channel::<RunEvent>();
    let (dtx, drx) = mpsc::channel::<Option<String>>();
    let (ctx_tx, ctx_rx) = mpsc::channel::<ConnOutcome>();
    std::thread::Builder::new()
        .name("nsql-worker".into())
        .spawn(move || {
            let opener: Opener = Box::new(
                move |spec: &ConnectSpec| -> Result<Box<dyn Session>, DbError> {
                    nsql_drivers::open(spec, default_dialect)
                },
            );
            let mut runner =
                Runner::new(default_dialect, opener).with_resolver(Box::new(|name: &str| {
                    let v = Vault::open_default().map_err(|e| e.to_string())?;
                    v.resolve(name).map_err(|e| e.to_string())
                }));
            let mut test_opener: Opener = Box::new(
                move |spec: &ConnectSpec| -> Result<Box<dyn Session>, DbError> {
                    nsql_drivers::open(spec, default_dialect)
                },
            );
            let mut emit = |e: RunEvent| {
                let _ = etx.send(e);
                wake();
            };
            // 활성 세션의 스펙(같은 서버 판정) · 호스트:포트(빠른 판정용) · 직전 실행이 접속성 오류였는가(다음 실행은 무조건 빠른 판정).
            let mut active_spec: Option<ConnectSpec> = None;
            let mut active_ep: Option<(String, u16)> = None;
            let mut suspect = false;
            let endpoint = |spec: &ConnectSpec| spec.host.clone().zip(spec.port);
            while let Ok(cmd) = rx.recv() {
                // 패닉 격리 — 한 명령의 패닉이 워커(=앱 전체)를 죽이지 않는다.
                let r = catch_unwind(AssertUnwindSafe(|| match cmd {
                    Cmd::ConnectSpec {
                        spec,
                        reconnect_same,
                    } => {
                        // 같은 서버에 이미 붙어 있으면 세션을 유지한다(설정으로 재접속 강제 가능).
                        let same = runner.session.is_some()
                            && active_spec.as_ref().is_some_and(|a| same_server(a, &spec));
                        if same && !reconnect_same {
                            let desc = runner.connection.clone().unwrap_or_else(|| spec.redacted());
                            let _ = ctx_tx.send(ConnOutcome::Connected(desc));
                            let _ = dtx.send(None);
                            wake();
                            return true;
                        }
                        let mut last_err: Option<String> = None;
                        let ok = runner.connect(&spec, &mut |e: RunEvent| {
                            if let RunEvent::Error { error, .. } = &e {
                                last_err = Some(error.message.clone());
                            }
                            emit(e);
                        });
                        if ok {
                            active_ep = endpoint(&spec);
                            active_spec = Some(spec.clone());
                            suspect = false;
                        }
                        let _ = ctx_tx.send(if ok {
                            ConnOutcome::Connected(spec.redacted())
                        } else {
                            ConnOutcome::ConnectFailed(last_err.unwrap_or_default())
                        });
                        let _ = dtx.send(None);
                        wake();
                        true
                    }
                    Cmd::Test(spec) => {
                        let r = nsql_run::test_connection(&spec, &mut test_opener);
                        let _ = ctx_tx.send(match r {
                            Ok(rep) => ConnOutcome::TestOk {
                                description: format!("{} · {}", rep.description, rep.session),
                                elapsed_s: format!("{:.3}", rep.elapsed.as_secs_f64()),
                            },
                            Err(e) => ConnOutcome::TestFailed(e.message),
                        });
                        let _ = dtx.send(None);
                        wake();
                        true
                    }
                    Cmd::Disconnect => {
                        if let Some(mut s) = runner.session.take() {
                            let _ = s.commit();
                        }
                        active_ep = None;
                        active_spec = None;
                        suspect = false;
                        emit(RunEvent::Disconnected);
                        let _ = ctx_tx.send(ConnOutcome::Disconnected);
                        let _ = dtx.send(None);
                        wake();
                        true
                    }
                    Cmd::SaveSpec { name, spec } => {
                        let r = Vault::open_default()
                            .and_then(|v| v.save(&name, &spec))
                            .map_err(|e| e.to_string());
                        let _ = ctx_tx.send(match r {
                            Ok(()) => ConnOutcome::Saved(name),
                            Err(e) => ConnOutcome::SaveFailed(e),
                        });
                        let _ = dtx.send(None);
                        wake();
                        true
                    }
                    Cmd::Connect(target) => {
                        match resolve_target(&target, default_dialect) {
                            Ok(spec) => {
                                if runner.connect(&spec, &mut emit) {
                                    active_ep = endpoint(&spec);
                                    active_spec = Some(spec);
                                    suspect = false;
                                }
                            }
                            Err(e) => emit(err(e)),
                        }
                        let _ = dtx.send(None);
                        wake();
                        true
                    }
                    Cmd::Run { src, preflight } => {
                        // 실행 전 빠른 판정 — 신호등이 초록이 아니거나(UI) 직전 실행이 접속성 오류였으면(워커) 포트를 먼저 본다.
                        let want = preflight.or_else(|| suspect.then(|| Duration::from_secs(2)));
                        if let (Some(timeout), Some((host, port))) = (want, active_ep.as_ref()) {
                            let t = std::time::Instant::now();
                            if probe::probe_once(host, *port, timeout) != probe::Outcome::Up {
                                let ep = format!("{host}:{port}");
                                let ms = t.elapsed().as_millis().to_string();
                                emit(err(tf(Msg::ErrServerUnreachable, &[&ep, &ms])));
                                let _ = dtx.send(Some(tf(Msg::WkErrors, &["1"])));
                                wake();
                                return true;
                            }
                        }
                        // 치환 변수 프롬프트는 최소 GUI에서 빈 값(T-16c에서 대화상자).
                        let mut prompt = |_: &str| Some(String::new());
                        let mut conn_err = false;
                        let errs = runner.run_script(&src, &mut prompt, &mut |e: RunEvent| {
                            if let RunEvent::Error { error, .. } = &e {
                                conn_err |= probe::is_connection_error(error.code, &error.message);
                            }
                            emit(e);
                        });
                        suspect = conn_err;
                        let _ = dtx.send(if errs > 0 {
                            Some(tf(Msg::WkErrors, &[&errs.to_string()]))
                        } else {
                            None
                        });
                        wake();
                        true
                    }
                    Cmd::Quit => {
                        if let Some(s) = runner.session.as_mut() {
                            let _ = s.commit();
                        }
                        false
                    }
                }));
                match r {
                    Ok(true) => {}
                    Ok(false) => break,
                    Err(payload) => {
                        // 드라이버/엔진 패닉 — 세션은 상태 불명이라 버린다. UI는 오류 + 끊김을 받아 busy를 푼다.
                        let what = payload
                            .downcast_ref::<&str>()
                            .map(|s| s.to_string())
                            .or_else(|| payload.downcast_ref::<String>().cloned())
                            .unwrap_or_default();
                        runner.session = None;
                        runner.connection = None;
                        active_ep = None;
                        active_spec = None;
                        suspect = false;
                        emit(err(tf(Msg::WkPanic, &[&what])));
                        emit(RunEvent::Disconnected);
                        let _ = ctx_tx.send(ConnOutcome::Disconnected);
                        let _ = dtx.send(Some(tf(Msg::WkErrors, &["1"])));
                        wake();
                    }
                }
            }
        })
        .expect("worker thread");
    (
        Handle {
            tx,
            done: drx,
            conn: ctx_rx,
        },
        erx,
    )
}
