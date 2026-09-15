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
use nsql_i18n::{t, tf, Msg};
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
    Disconnect,
    /// 수동 커밋 모드의 Commit/Rollback(메뉴 · 단축키 · 사용자 09-15).
    Commit,
    Rollback,
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
    Disconnected,
    /// 편집기 세션의 Oracle SID(라이브 로그 모니터가 V$SESSION을 볼 때 · T-71).
    SessionId(String),
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
    max_rows: usize,
    autocommit: bool,
    auto_reconnect: bool,
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
            let wake_shared = std::sync::Arc::new(std::sync::Mutex::new(wake));
            let wake_now = {
                let w = wake_shared.clone();
                move || {
                    if let Ok(f) = w.lock() {
                        f();
                    }
                }
            };
            // ★ 실행 중 서버 메시지(PRINT · RAISE NOTICE)를 도착 즉시 UI 로그로(사용자 09-15).
            let sink: nsql_core::MessageSink = {
                let etx = etx.clone();
                let wake = wake_shared.clone();
                std::sync::Arc::new(move |m: String| {
                    let _ = etx.send(RunEvent::Message(m));
                    if let Ok(w) = wake.lock() {
                        w();
                    }
                })
            };
            let mut runner = Runner::new(default_dialect, opener)
                .with_max_rows(max_rows)
                .with_message_sink(sink)
                .with_autocommit(autocommit)
                .with_resolver(Box::new(|name: &str| {
                    let v = Vault::open_default().map_err(|e| e.to_string())?;
                    v.resolve(name).map_err(|e| e.to_string())
                }));
            let mut emit = {
                let etx = etx.clone();
                let wake = wake_shared.clone();
                move |e: RunEvent| {
                    let _ = etx.send(e);
                    if let Ok(w) = wake.lock() {
                        w();
                    }
                }
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
                            wake_now();
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
                        // Oracle이면 세션 SID를 알려 준다(라이브 모니터 V$SESSION 조회용).
                        if ok && runner.engine.dialect == Dialect::Oracle {
                            if let Some(s) = runner.session.as_mut() {
                                let req = nsql_core::ExecRequest {
                                    sql: "SELECT SYS_CONTEXT('USERENV','SID') FROM dual".into(),
                                    params: vec![],
                                };
                                if let Ok(r) = s.execute(&req) {
                                    if let Some(v) = r
                                        .result_sets
                                        .first()
                                        .and_then(|rs| rs.rows.first())
                                        .and_then(|row| row.first())
                                    {
                                        let _ = ctx_tx.send(ConnOutcome::SessionId(v.display()));
                                    }
                                }
                            }
                        }
                        let _ = dtx.send(None);
                        wake_now();
                        true
                    }
                    c @ (Cmd::Commit | Cmd::Rollback) => {
                        let commit = matches!(c, Cmd::Commit);
                        let r = match runner.session.as_mut() {
                            Some(s) => {
                                if commit {
                                    s.commit()
                                } else {
                                    s.rollback()
                                }
                            }
                            None => Ok(()),
                        };
                        match r {
                            Ok(()) => emit(RunEvent::Message(
                                t(if commit {
                                    Msg::StCommitted
                                } else {
                                    Msg::StRolledBack
                                })
                                .to_string(),
                            )),
                            Err(e) => emit(err(e.message)),
                        }
                        let _ = dtx.send(None);
                        wake_now();
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
                        wake_now();
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
                        wake_now();
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
                                wake_now();
                                return true;
                            }
                        }
                        // ★ 자동 재접속(사용자 09-15 기본 기능): 직전 실행이 접속성 오류였고 서버가 살아 있으면 같은 스펙으로 먼저 다시 붙는다.
                        if suspect && auto_reconnect {
                            if let Some(spec) = active_spec.clone() {
                                emit(RunEvent::Message(tf(
                                    Msg::StReconnecting,
                                    &[&spec.redacted()],
                                )));
                                let mut failed = false;
                                let ok = runner.connect(&spec, &mut |e: RunEvent| {
                                    if matches!(e, RunEvent::Error { .. }) {
                                        failed = true;
                                    }
                                    emit(e);
                                });
                                if !ok || failed {
                                    let _ = dtx.send(Some(tf(Msg::WkErrors, &["1"])));
                                    wake_now();
                                    return true;
                                }
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
                        wake_now();
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
                        wake_now();
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

/// 접속 테스트 결과(별도 스레드 · 어느 프로필인지 이름을 함께).
pub(crate) struct TestResult {
    pub name: String,
    /// Ok = (설명, 소요 초 문자열) · Err = 메시지.
    pub outcome: Result<(String, String), String>,
}

/// ★ 접속 테스트는 요청마다 **짧은 스레드 하나**(사용자 09-14) — 순차 워커·`busy`와 무관해서 실패 서버의 긴 타임아웃이
/// 다른 서버 테스트나 실행을 막지 않는다. 세션은 스레드 안에서 열고 바로 닫는다(DB 워커 세션과 공유 없음).
pub(crate) fn spawn_test(
    name: String,
    spec: ConnectSpec,
    default_dialect: Dialect,
    tx: mpsc::Sender<TestResult>,
    wake: Box<dyn Fn() + Send>,
) {
    let _ = std::thread::Builder::new()
        .name(format!("nsql-test:{name}"))
        .spawn(move || {
            let mut opener: Opener = Box::new(
                move |spec: &ConnectSpec| -> Result<Box<dyn Session>, DbError> {
                    nsql_drivers::open(spec, default_dialect)
                },
            );
            let outcome = match nsql_run::test_connection(&spec, &mut opener) {
                // 세션 자기 설명이 접속 설명과 같으면(Oracle 등) 한 번만(사용자 09-16 "동일한 정보가 2개").
                Ok(rep) => Ok((
                    if rep.session.is_empty() || rep.session == rep.description {
                        rep.description
                    } else {
                        format!("{} · {}", rep.description, rep.session)
                    },
                    format!("{:.3}", rep.elapsed.as_secs_f64()),
                )),
                Err(e) => Err(e.message),
            };
            let _ = tx.send(TestResult { name, outcome });
            wake();
        });
}
