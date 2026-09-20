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
//!
//! 추가 페치(T-48a · docs/43 §3): `Cmd::FetchPage`는 러너가 **같은 문장의 서버 커서**를 그 위치에 열어 두었으면 `fetch_next`,
//! 아니면 종전 OFFSET 재질의(`nsql_io::paging`)로 간다 — 판정·폴백은 [`Runner::fetch_page`] 한 곳. 설정 `grid.fetch_mode`
//! (cursor|offset|off) · `db.fetch_size` · `db.cursor_idle_secs`는 실행마다 다시 읽어 러너에 넣는다(파일 한 번 · 왕복 대비 무시 가능).

use crate::probe;
use nsql_core::{DbError, Dialect, Session};
use nsql_i18n::{t, tf, Msg};
use nsql_run::{Opener, RunEvent, Runner};
use nsql_script::ConnectSpec;
use nsql_vault::Vault;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
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
    /// 테이블 키(PK·유니크) 조회 — 그리드 Copy SQL의 키 규칙(docs/41). 스키마 없음 = 현재 스키마.
    Keys {
        /// 캐시 키(실행문의 테이블 표기 그대로 · 답에 그대로 실린다).
        key: String,
        schema: Option<String>,
        table: String,
    },
    /// 수동 커밋 모드의 Commit/Rollback(메뉴 · 단축키 · 사용자 09-15).
    Commit,
    Rollback,
    Run {
        src: String,
        /// `Some(timeout)` = 실행 전 호스트:포트 빠른 판정(신호등이 초록이 아닐 때 UI가 켠다).
        preflight: Option<Duration>,
        /// 이번 실행의 페치 상한(결과 탭의 세그먼트 크기 · 0 = 전체 · docs/43).
        max_rows: usize,
    },
    /// 추가 페치(docs/43 §3-4 OFFSET 폴백): `limit` 0 = 전체 조회(래핑 없이 원문 · 상한 0).
    FetchPage {
        /// 결과 탭 키(결과 탭 id).
        key: u64,
        sql: String,
        offset: usize,
        limit: usize,
        /// 전체 조회(limit 0)의 메모리 예산(바이트 · 0 = 무제한 · D-72) — 넘치면 예산까지만 남기고 `more`.
        budget_bytes: u64,
        /// ★ 일관성 엄격(설정 `grid.refetch_mode=strict` · docs/43 §9): 커서가 없고 최상위 ORDER BY도 없으면 이어 붙이지 않고
        ///   **처음부터 다시 받아 교체**한다(한 번의 실행이 결과를 정의 → 행수·값 완전 일치).
        strict: bool,
        /// `strict_all`(43 §9-3): ORDER BY가 있어도 커서를 잃으면 교체(동점 정렬 대비).
        strict_all: bool,
    },
    /// `SELECT COUNT(*) FROM (질의) x`.
    Count {
        key: u64,
        sql: String,
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
    /// `Cmd::Keys` 결과 — (요청한 테이블 표기, 키 정보 · 조회 실패/세션 없음 = None).
    Keys(String, Option<nsql_core::KeyInfo>),
    /// `Cmd::FetchPage` 결과 — `offset`부터 **이어 붙임**(전체 조회도 나머지를 이어 붙인다 · 09-17 위치 유지) ·
    /// `offset` 0은 교체. `all` = 전체 조회 결과(늦은 세그먼트와 구별) · `stop` = 전체 조회가 멈춘 이유(예산·취소).
    Page {
        key: u64,
        offset: usize,
        all: bool,
        result: Result<(nsql_core::ResultSet, bool, Duration), String>,
        stop: Option<FetchStop>,
        /// 결과가 **처음부터 다시 받은 전체**라 그리드가 교체해야 한다(엄격 모드 · 위치는 유지).
        replace: bool,
    },
    /// 전체 조회 진행(배치마다 · T-48b): 지금까지 받은 행·바이트.
    FetchProgress {
        key: u64,
        rows: u64,
        bytes: u64,
    },
    /// `Cmd::Count` 결과.
    Count {
        key: u64,
        result: Result<u64, String>,
    },
    /// ★ 끊김 확인(docs/53 §3): 동작 직전 빠른 판정이 실패했거나 접속성 오류가 났다 — 세션 객체·스펙은 그대로(Broken).
    Broken(String),
    /// 끊김이었던 세션이 다시 살아 있음을 확인했다(판정 성공 · 재접속 성공).
    Alive,
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
pub(crate) fn same_server(a: &ConnectSpec, b: &ConnectSpec) -> bool {
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

/// ★ 인라인 접속 문자열은 **비밀번호가 있어야** 연다(docs/52 §4 · 09-19). 저장소 프로필은 워커에 오기 전에 이미 채워져
/// 오고(`resolve_target`), SQLite는 자격이 없다. 자격 빌리기(같은 서버·계정 프로필의 비밀번호를 몰래 쓰기)는 하지 않는다 —
/// 사용자가 보기에 "비밀번호 없이 접속됨"이 되어 오해·오접속을 낳는다. 비밀번호 변수/모달 입력 = T-132.
pub(crate) fn password_required(spec: &ConnectSpec, default_dialect: Dialect) -> bool {
    spec.dialect.unwrap_or(default_dialect) != Dialect::Sqlite
        && spec.host.is_some()
        && spec.password.as_deref().is_none_or(str::is_empty)
}

/// 생존 판정 설정(docs/53 §3 · 실행마다 읽는다): (빠른 판정 상한, 마지막 성공 뒤 이 시간이 지나면 동작 전에 판정, 자동 재접속).
fn liveness_settings() -> (Duration, Duration, bool) {
    let Ok(s) = nsql_settings::Settings::open_default() else {
        return (Duration::from_secs(2), Duration::from_secs(60), true);
    };
    (
        Duration::from_secs(s.int("probe.timeout").clamp(1, 60) as u64),
        Duration::from_secs(s.int("probe.stale_secs").max(0) as u64),
        s.flag("connect.auto_reconnect"),
    )
}

/// 페치 설정(docs/43 §4-3)을 러너에 반영 — `grid.fetch_mode`(cursor만 커서 유지) · `db.fetch_size` · `db.cursor_idle_secs`.
fn apply_fetch_settings(runner: &mut Runner) {
    let Ok(s) = nsql_settings::Settings::open_default() else {
        return;
    };
    runner.keep_cursor = s.get("grid.fetch_mode").unwrap_or("cursor") == "cursor";
    runner.fetch_size = s.int("db.fetch_size").max(0) as usize;
    runner.cursor_idle_secs = s.int("db.cursor_idle_secs").max(0) as u64;
    // docs/56 L1: 수동 모드의 변경 없는 트랜잭션 자동 종료.
    runner.read_end = nsql_run::ReadEnd::parse(s.get("tx.read_end").unwrap_or("auto"));
    // 실행 뒤 돌아온 REF CURSOR를 바로 결과로(끄면 `PRINT rc`).
    runner.auto_cursor = s.get("run.cursor_autoshow").is_none_or(|v| v == "on");
}

/// docs/56 L4 — 접속 직후 서버 안전망 세션 파라미터(설정 0 = 안 보냄 · 지원 방언만). 돌려주는 값 = 보낼 문장들.
fn server_guard_sql(dialect: Dialect, idle_tx_secs: i64, lock_wait_secs: i64) -> Vec<String> {
    let mut out = Vec::new();
    match dialect {
        Dialect::Postgres => {
            if idle_tx_secs > 0 {
                out.push(format!(
                    "SET idle_in_transaction_session_timeout = '{idle_tx_secs}s'"
                ));
            }
            if lock_wait_secs > 0 {
                out.push(format!("SET lock_timeout = '{lock_wait_secs}s'"));
            }
        }
        Dialect::Mssql => {
            if lock_wait_secs > 0 {
                out.push(format!("SET LOCK_TIMEOUT {}", lock_wait_secs * 1000));
            }
        }
        Dialect::Mysql => {
            if lock_wait_secs > 0 {
                out.push(format!(
                    "SET SESSION innodb_lock_wait_timeout = {lock_wait_secs}"
                ));
                out.push(format!("SET SESSION lock_wait_timeout = {lock_wait_secs}"));
            }
        }
        Dialect::Oracle => {
            if lock_wait_secs > 0 {
                out.push(format!(
                    "ALTER SESSION SET ddl_lock_timeout = {lock_wait_secs}"
                ));
            }
        }
        Dialect::Sqlite | Dialect::Odbc => {}
    }
    out
}

/// 이 세션의 서버 쪽 식별자를 묻는 문장(Oracle SID · PG backend pid · SQL Server SPID · MySQL connection id) —
/// 라이브 모니터(Oracle)와 막힘 감지(docs/56 L3)가 메타 세션에서 "이 세션 때문에 기다리는 세션"을 찾는 열쇠.
fn session_id_sql(dialect: Dialect) -> Option<&'static str> {
    match dialect {
        Dialect::Oracle => Some("SELECT SYS_CONTEXT('USERENV','SID') FROM dual"),
        Dialect::Postgres => Some("SELECT pg_backend_pid()"),
        Dialect::Mssql => Some("SELECT @@SPID"),
        Dialect::Mysql => Some("SELECT CONNECTION_ID()"),
        Dialect::Sqlite | Dialect::Odbc => None,
    }
}

/// 저장된 프로필 이름(상태줄 안내용 · 실패하면 빈 목록).
pub(crate) fn profile_names() -> Vec<String> {
    Vault::open_default()
        .and_then(|v| v.list())
        .map(|l| l.into_iter().map(|p| p.name).collect())
        .unwrap_or_default()
}

/// 전체 조회가 끝까지 가지 못한 이유.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FetchStop {
    /// 메모리 예산(D-72)에 닿아 예산까지만.
    Budget,
    /// 사용자가 중지(도구줄 ■ · Esc).
    Cancelled,
}

pub(crate) struct Handle {
    tx: mpsc::Sender<Cmd>,
    /// 전체 조회 취소 깃발(T-48b) — UI가 올리고 워커가 배치 사이에 본다.
    cancel_fetch: Arc<AtomicBool>,
    /// 실행 중 문장의 취소 핸들(T-108) — 워커가 실행 직전에 넣는다.
    cancel_run: Arc<Mutex<Option<Arc<dyn nsql_core::CancelHandle>>>>,
    /// 명령 완료 신호(상태 메시지 옵션).
    pub done: mpsc::Receiver<Option<String>>,
    /// 접속 계열 결과.
    pub conn: mpsc::Receiver<ConnOutcome>,
}

impl Handle {
    pub(crate) fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }

    /// 진행 중인 전체 조회를 다음 배치 경계에서 멈춘다(지금까지 받은 행은 남는다).
    pub(crate) fn cancel_fetch(&self) {
        self.cancel_fetch.store(true, Ordering::Relaxed);
    }

    /// 실행 중 문장 취소(T-108): 드라이버 핸들이 있으면 서버에 취소를 보내고 true · 없으면(SQL Server) 전체 조회 배치 취소만.
    /// 반환 = (취소를 보냈는가(스레드에 위임), 세션을 끊는 방식인가).
    pub(crate) fn cancel_run(&self) -> (bool, bool) {
        self.cancel_fetch();
        let h = self.cancel_run.lock().ok().and_then(|g| g.clone());
        match h {
            Some(h) => {
                let drops = h.drops_session();
                // ★ 취소는 별도 스레드에서(docs/44 §6): PG `cancel_query`는 새 TCP 접속을 **동기**로 열어 서버가 안 닿으면
                //   OS 접속 타임아웃만큼 막힌다 — UI 스레드에서 부르면 창이 멈춘다. 결과는 로그 창으로만.
                let _ = std::thread::Builder::new()
                    .name("nsql-cancel".into())
                    .spawn(move || {
                        let _ = h.cancel();
                    });
                (true, drops)
            }
            None => (false, false),
        }
    }
}

pub(crate) fn spawn(
    default_dialect: Dialect,
    max_rows: usize,
    autocommit: bool,
    wake: Box<dyn Fn() + Send>,
) -> (Handle, mpsc::Receiver<RunEvent>) {
    let (tx, rx) = mpsc::channel::<Cmd>();
    let (etx, erx) = mpsc::channel::<RunEvent>();
    let (dtx, drx) = mpsc::channel::<Option<String>>();
    let (ctx_tx, ctx_rx) = mpsc::channel::<ConnOutcome>();
    let cancel_fetch = Arc::new(AtomicBool::new(false));
    let cancel_flag = cancel_fetch.clone();
    let cancel_run: Arc<Mutex<Option<Arc<dyn nsql_core::CancelHandle>>>> =
        Arc::new(Mutex::new(None));
    let cancel_slot = cancel_run.clone();
    std::thread::Builder::new()
        .name("nsql-worker".into())
        .spawn(move || {
            let opener: Opener = Box::new(
                move |spec: &ConnectSpec| -> Result<Box<dyn Session>, DbError> {
                    if password_required(spec, default_dialect) {
                        return Err(DbError {
                            code: None,
                            message: t(Msg::ErrPasswordRequired).into(),
                            position: None,
                        });
                    }
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
            apply_fetch_settings(&mut runner);
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
            // 마지막으로 서버와 성공적으로 주고받은 시각(docs/53 §3-1) · 끊김을 UI에 알렸는가(살아나면 Alive 1회).
            let mut last_ok = std::time::Instant::now();
            let mut broken_told = false;
            let endpoint = |spec: &ConnectSpec| spec.host.clone().zip(spec.port);
            // ★ 동작 직전 생존 판정(docs/53 §3): ① UI가 요청했거나(신호등 ≠ 초록) ② 직전 접속성 오류 ③ 드라이버가 끊김을 안다(`is_alive`)
            //   ④ 마지막 성공 뒤 `probe.stale_secs` 지남 → 호스트:포트 TCP 판정(SYN 1 · `probe.timeout`). 죽었으면 Broken + 오류(막힘 0).
            //   살아 있고 ②③이면(설정) 같은 스펙으로 재접속. 반환 = 진행해도 되는가.
            #[allow(clippy::too_many_arguments)]
            fn ensure_alive(
                runner: &mut Runner,
                active_spec: &Option<ConnectSpec>,
                active_ep: &Option<(String, u16)>,
                suspect: &mut bool,
                last_ok: &mut std::time::Instant,
                broken_told: &mut bool,
                preflight: Option<Duration>,
                allow_reconnect: bool,
                ctx_tx: &mpsc::Sender<ConnOutcome>,
                emit: &mut dyn FnMut(RunEvent),
            ) -> Result<(), String> {
                let (timeout, stale, auto) = liveness_settings();
                let dead_hint = runner.session.as_ref().is_some_and(|s| !s.is_alive());
                let stale_now = stale.as_secs() > 0 && last_ok.elapsed() >= stale;
                let plan = crate::sessions::live_plan(
                    preflight.is_some(),
                    *suspect,
                    dead_hint,
                    stale_now,
                    allow_reconnect,
                    auto,
                );
                let timeout = preflight.unwrap_or(timeout);
                if let (true, Some((host, port))) = (plan.probe, active_ep.as_ref()) {
                    let t = std::time::Instant::now();
                    if probe::probe_once(host, *port, timeout, true) != probe::Outcome::Up {
                        let ep = format!("{host}:{port}");
                        let ms = t.elapsed().as_millis().to_string();
                        let m = tf(Msg::ErrServerUnreachable, &[&ep, &ms]);
                        *suspect = true;
                        if !*broken_told {
                            *broken_told = true;
                            let _ = ctx_tx.send(ConnOutcome::Broken(m.clone()));
                        }
                        return Err(m);
                    }
                }
                if plan.reconnect {
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
                            *suspect = true;
                            if !*broken_told {
                                *broken_told = true;
                                let _ = ctx_tx.send(ConnOutcome::Broken(tf(
                                    Msg::StReconnecting,
                                    &[&spec.redacted()],
                                )));
                            }
                            return Err(String::new());
                        }
                        *suspect = false;
                    }
                }
                if *broken_told {
                    *broken_told = false;
                    let _ = ctx_tx.send(ConnOutcome::Alive);
                }
                *last_ok = std::time::Instant::now();
                Ok(())
            }
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
                        // 접속 전 빠른 판정(SYN 1 · docs/53): 끊긴 네트워크에서 드라이버의 긴 접속 타임아웃을 기다리지 않는다.
                        //   (같은 서버 재접속·유휴 뒤 재접속·접속 창 Connect 모두 — 접속 창의 신호등과 별개로 지금 이 순간을 본다.)
                        let (timeout, _, _) = liveness_settings();
                        let reachable = match endpoint(&spec) {
                            Some((host, port)) => {
                                let t = std::time::Instant::now();
                                let up = probe::probe_once(&host, port, timeout, true)
                                    == probe::Outcome::Up;
                                if !up {
                                    let ep = format!("{host}:{port}");
                                    let ms = t.elapsed().as_millis().to_string();
                                    last_err = Some(tf(Msg::ErrServerUnreachable, &[&ep, &ms]));
                                }
                                up
                            }
                            None => true,
                        };
                        let ok = reachable
                            && runner.connect(&spec, &mut |e: RunEvent| {
                                if let RunEvent::Error { error, .. } = &e {
                                    last_err = Some(error.message.clone());
                                }
                                emit(e);
                            });
                        if !reachable {
                            if !broken_told {
                                broken_told = true;
                                let _ = ctx_tx.send(ConnOutcome::Broken(
                                    last_err.clone().unwrap_or_default(),
                                ));
                            }
                            emit(err(last_err.clone().unwrap_or_default()));
                        }
                        if ok {
                            last_ok = std::time::Instant::now();
                            if broken_told {
                                broken_told = false;
                                let _ = ctx_tx.send(ConnOutcome::Alive);
                            }
                            active_ep = endpoint(&spec);
                            active_spec = Some(spec.clone());
                            suspect = false;
                        }
                        let _ = ctx_tx.send(if ok {
                            ConnOutcome::Connected(spec.redacted())
                        } else {
                            ConnOutcome::ConnectFailed(last_err.unwrap_or_default())
                        });
                        // 접속 직후 1회: ① 서버 안전망 세션 파라미터(docs/56 L4 · 설정 0 = 없음) ② 세션 식별자(라이브 모니터 ·
                        //   막힘 감지 L3). PG의 SET은 트랜잭션에 묶이므로 **커밋으로** 끝낸다(롤백하면 SET이 되돌아간다).
                        if ok {
                            runner.note_tx_ended();
                            let dialect = runner.engine.dialect;
                            let (idle_tx, lock_wait) = nsql_settings::Settings::open_default()
                                .map(|s| {
                                    (
                                        s.int("tx.server_idle_timeout_secs"),
                                        s.int("tx.lock_wait_timeout_secs"),
                                    )
                                })
                                .unwrap_or((0, 0));
                            if let Some(s) = runner.session.as_mut() {
                                for sql in server_guard_sql(dialect, idle_tx, lock_wait) {
                                    let req = nsql_core::ExecRequest {
                                        sql: sql.clone(),
                                        params: vec![],
                                    };
                                    if let Err(e) = s.execute(&req) {
                                        emit(RunEvent::Message(format!("{sql} — {}", e.message)));
                                    }
                                }
                                if let Some(sql) = session_id_sql(dialect) {
                                    let req = nsql_core::ExecRequest {
                                        sql: sql.into(),
                                        params: vec![],
                                    };
                                    if let Ok(r) = s.execute(&req) {
                                        if let Some(v) = r
                                            .result_sets
                                            .first()
                                            .and_then(|rs| rs.rows.first())
                                            .and_then(|row| row.first())
                                        {
                                            let _ =
                                                ctx_tx.send(ConnOutcome::SessionId(v.display()));
                                        }
                                    }
                                }
                                let _ = s.commit();
                            }
                        }
                        let _ = dtx.send(None);
                        wake_now();
                        true
                    }
                    Cmd::FetchPage {
                        key,
                        sql,
                        offset,
                        limit,
                        budget_bytes,
                        strict,
                        strict_all,
                    } => {
                        let mut stop: Option<FetchStop> = None;
                        let mut replace = false;
                        let alive = ensure_alive(
                            &mut runner,
                            &active_spec,
                            &active_ep,
                            &mut suspect,
                            &mut last_ok,
                            &mut broken_told,
                            None,
                            true,
                            &ctx_tx,
                            &mut emit,
                        );
                        let result = match (alive, runner.dialect()) {
                            (Err(m), _) => Err(if m.is_empty() {
                                t(Msg::ExpNotConnected).to_string()
                            } else {
                                m
                            }),
                            (Ok(()), None) => Err(t(Msg::ExpNotConnected).to_string()),
                            (Ok(()), Some(_)) => {
                                if limit == 0 {
                                    // ★ 전체 조회 = **나머지 이어 받기**(사용자 09-17 "현재 위치 유지"): 재실행·교체가 아니라 `offset`
                                    //   (= 그리드가 이미 든 행 수)부터 끝까지 받아 이어 붙인다 → 스크롤·정렬·텍스트 보기 위치가 그대로.
                                    //   ① 같은 문장의 커서가 그 위치면 커서에서 fetch_all(재전송 0 · 순서 일관)
                                    //   ② 커서가 없으면(닫힘·다른 문장) 원문을 커서로 다시 실행해 앞 `offset`행은 **버리고** 이어 받기(메모리 0)
                                    //   ③ 커서 없는 드라이버(MSSQL) = OFFSET 재질의로 나머지 전부(진행률·취소 없음 · 예산 절단)
                                    // ★ 왕복당 행수는 `db.fetch_all_size`(기본 5000) — 실측(09-17 Oracle WAN 155k행): 200이면 34.3s · 5000이면 4.6s.
                                    //   배치마다 진행률 · 취소 깃발 · 예산(D-72)은 받으면서 판정(순간 메모리 = 예산).
                                    cancel_flag.store(false, Ordering::Relaxed);
                                    let page_fs = runner.fetch_size;
                                    let all_fs = nsql_settings::Settings::open_default()
                                        .map(|s| s.int("db.fetch_all_size").max(1) as usize)
                                        .unwrap_or(5000);
                                    let batch = page_fs.max(all_fs);
                                    runner.set_fetch_size(batch);
                                    let cursor_h = if runner.cursor_matches(&sql, offset) {
                                        runner.cursor().map(|(h, _)| h)
                                    } else {
                                        None
                                    };
                                    // ★ 엄격 일관성(docs/43 §9): 커서를 잃었고 정렬(ORDER BY)도 없으면 재실행+건너뛰기/OFFSET은
                                    //   순서가 달라질 수 있다 → 처음부터 전부 다시 받아 **교체**(offset 0).
                                    let strict_replace = strict
                                        && cursor_h.is_none()
                                        && (strict_all || !nsql_io::paging::has_order_by(&sql));
                                    replace = strict_replace;
                                    let rows0 = if strict_replace { 0 } else { offset as u64 };
                                    let mut last = std::time::Instant::now();
                                    let mut progress = |rows: u64, bytes: u64| {
                                        if last.elapsed() >= Duration::from_millis(100) {
                                            last = std::time::Instant::now();
                                            let _ = ctx_tx.send(ConnOutcome::FetchProgress {
                                                key,
                                                rows: rows0 + rows,
                                                bytes,
                                            });
                                            wake_now();
                                        }
                                        !cancel_flag.load(Ordering::Relaxed)
                                    };
                                    let r: Result<
                                        (nsql_core::ResultSet, bool, Duration),
                                        nsql_core::DbError,
                                    > = if strict_replace {
                                        // ⓪ 엄격: 처음부터 전부(커서 드라이버는 커서로 스트리밍 · 아니면 OFFSET 0 재질의).
                                        let t0 = std::time::Instant::now();
                                        if runner.cursor_supported() {
                                            runner.query_stream(&sql, batch).and_then(
                                                |(mut rs, h, _)| {
                                                    let Some(h) = h else {
                                                        return Ok((rs, false, t0.elapsed()));
                                                    };
                                                    let (rest, stopped, _) = runner.fetch_all(
                                                        h,
                                                        budget_bytes,
                                                        &mut progress,
                                                    )?;
                                                    rs.rows.extend(rest.rows);
                                                    Ok((rs, stopped, t0.elapsed()))
                                                },
                                            )
                                        } else {
                                            runner.fetch_offset(&sql, 0, 0).map(
                                                |(mut rs, more, tl)| {
                                                    if budget_bytes > 0
                                                        && rs.approx_bytes() > budget_bytes
                                                    {
                                                        let n = rs.rows.len().max(1);
                                                        let per =
                                                            (rs.approx_bytes() / n as u64).max(1);
                                                        let keep = (budget_bytes / per) as usize;
                                                        rs.rows.truncate(keep.max(1));
                                                        (rs, true, tl.total())
                                                    } else {
                                                        (rs, more, tl.total())
                                                    }
                                                },
                                            )
                                        }
                                    } else if let Some(h) = cursor_h {
                                        // ① 커서 이어 받기.
                                        let t0 = std::time::Instant::now();
                                        runner
                                            .fetch_all(h, budget_bytes, &mut progress)
                                            .map(|(rest, stopped, _)| (rest, stopped, t0.elapsed()))
                                    } else if runner.cursor_supported() {
                                        // ② 원문을 커서로 다시 실행 → 앞 offset행은 버리고 이어 받기.
                                        let t0 = std::time::Instant::now();
                                        runner.query_stream(&sql, batch).and_then(
                                            |(mut rs, h, _)| {
                                                let Some(h) = h else {
                                                    // 커서 없이 끝났다(전체가 첫 배치 안) — 앞 offset행만 버린다.
                                                    let skip = offset.min(rs.rows.len());
                                                    rs.rows.drain(..skip);
                                                    return Ok((rs, false, t0.elapsed()));
                                                };
                                                let mut skipped = rs.rows.len();
                                                if skipped > offset {
                                                    rs.rows.drain(..offset);
                                                    // 첫 배치가 offset을 넘었다 — 남은 부분부터 이어 붙일 준비.
                                                } else {
                                                    rs.rows.clear();
                                                    // 나머지 건너뛰기(배치 단위 · 메모리에 남기지 않음).
                                                    while skipped < offset {
                                                        let (chunk, more, _) = runner.fetch_next(
                                                            h,
                                                            (offset - skipped).min(batch),
                                                        )?;
                                                        skipped += chunk.rows.len();
                                                        if !more || chunk.rows.is_empty() {
                                                            return Ok((rs, false, t0.elapsed()));
                                                        }
                                                    }
                                                }
                                                let (rest, stopped, _) = runner.fetch_all(
                                                    h,
                                                    budget_bytes,
                                                    &mut progress,
                                                )?;
                                                rs.rows.extend(rest.rows);
                                                Ok((rs, stopped, t0.elapsed()))
                                            },
                                        )
                                    } else {
                                        // ③ OFFSET 재질의(커서 없는 드라이버).
                                        runner.fetch_offset(&sql, offset, 0).map(
                                            |(mut rs, more, tl)| {
                                                if budget_bytes > 0
                                                    && rs.approx_bytes() > budget_bytes
                                                {
                                                    let n = rs.rows.len().max(1);
                                                    let per = (rs.approx_bytes() / n as u64).max(1);
                                                    let keep = (budget_bytes / per) as usize;
                                                    rs.rows.truncate(keep.max(1));
                                                    (rs, true, tl.total())
                                                } else {
                                                    (rs, more, tl.total())
                                                }
                                            },
                                        )
                                    };
                                    let r = r.map(|(rs, stopped, d)| {
                                        if stopped {
                                            runner.close_cursor();
                                            stop = Some(if cancel_flag.load(Ordering::Relaxed) {
                                                FetchStop::Cancelled
                                            } else {
                                                FetchStop::Budget
                                            });
                                        }
                                        (rs, stopped, d)
                                    });
                                    runner.set_fetch_size(page_fs);
                                    r.map_err(|e| e.message)
                                } else if strict
                                    && !runner.cursor_matches(&sql, offset)
                                    && (strict_all || !nsql_io::paging::has_order_by(&sql))
                                {
                                    // ⓪ 엄격(docs/43 §9): 커서 없음 + 정렬 없음 → 처음부터 `offset+limit`행을 한 실행으로 다시 받아 교체.
                                    //   커서 드라이버는 그 자리에 커서를 남겨 다음 세그먼트부터는 커서로 잇는다.
                                    replace = true;
                                    let need = offset + limit;
                                    let t0 = std::time::Instant::now();
                                    let r: Result<
                                        (nsql_core::ResultSet, bool, Duration),
                                        nsql_core::DbError,
                                    > = if runner.cursor_supported() {
                                        runner.query_stream(&sql, need.max(1)).and_then(
                                            |(mut rs, h, _)| {
                                                let mut more = h.is_some();
                                                if let Some(h) = h {
                                                    while rs.rows.len() < need && more {
                                                        let (chunk, m, _) = runner
                                                            .fetch_next(h, need - rs.rows.len())?;
                                                        more = m;
                                                        if chunk.rows.is_empty() {
                                                            break;
                                                        }
                                                        rs.rows.extend(chunk.rows);
                                                    }
                                                }
                                                Ok((rs, more, t0.elapsed()))
                                            },
                                        )
                                    } else {
                                        runner
                                            .fetch_offset(&sql, 0, need)
                                            .map(|(rs, more, tl)| (rs, more, tl.total()))
                                    };
                                    r.map_err(|e| e.message)
                                } else {
                                    // 같은 문장의 커서가 그 위치에 있으면 fetch_next · 아니면 OFFSET 재질의(limit+1행 · 09-16 more 규칙).
                                    runner
                                        .fetch_page(&sql, offset, limit)
                                        .map(|(rs, more, tl, _)| (rs, more, tl.total()))
                                        .map_err(|e| e.message)
                                }
                            }
                        };
                        let _ = ctx_tx.send(ConnOutcome::Page {
                            key,
                            offset,
                            all: limit == 0,
                            result,
                            stop,
                            replace,
                        });
                        wake_now();
                        true
                    }
                    Cmd::Count { key, sql } => {
                        let alive = ensure_alive(
                            &mut runner,
                            &active_spec,
                            &active_ep,
                            &mut suspect,
                            &mut last_ok,
                            &mut broken_told,
                            None,
                            true,
                            &ctx_tx,
                            &mut emit,
                        );
                        let result = match (alive, runner.dialect()) {
                            (Err(m), _) => Err(if m.is_empty() {
                                t(Msg::ExpNotConnected).to_string()
                            } else {
                                m
                            }),
                            (Ok(()), None) => Err(t(Msg::ExpNotConnected).to_string()),
                            (Ok(()), Some(_)) => {
                                runner.count(&sql).map(|(n, _)| n).map_err(|e| e.message)
                            }
                        };
                        let _ = ctx_tx.send(ConnOutcome::Count { key, result });
                        wake_now();
                        true
                    }
                    Cmd::Keys { key, schema, table } => {
                        let alive = ensure_alive(
                            &mut runner,
                            &active_spec,
                            &active_ep,
                            &mut suspect,
                            &mut last_ok,
                            &mut broken_told,
                            None,
                            true,
                            &ctx_tx,
                            &mut emit,
                        );
                        let info = alive.ok().and(runner.session.as_mut()).and_then(|s| {
                            let schema =
                                schema.or_else(|| nsql_catalog::current_schema(s.as_mut()).ok())?;
                            nsql_catalog::keys(s.as_mut(), &schema, &table).ok()
                        });
                        let _ = ctx_tx.send(ConnOutcome::Keys(key, info));
                        wake_now();
                        true
                    }
                    c @ (Cmd::Commit | Cmd::Rollback) => {
                        let commit = matches!(c, Cmd::Commit);
                        runner.close_cursor(); // 커밋/롤백 = 커서 닫기(docs/43 D-70)
                                               // 커밋/롤백은 재접속하지 않는다(새 세션엔 그 트랜잭션이 없다) — 죽었으면 바로 오류(막힘 0).
                        let alive = ensure_alive(
                            &mut runner,
                            &active_spec,
                            &active_ep,
                            &mut suspect,
                            &mut last_ok,
                            &mut broken_told,
                            None,
                            false,
                            &ctx_tx,
                            &mut emit,
                        );
                        let r = match (alive, runner.session.as_mut()) {
                            (Err(m), _) => Err(DbError {
                                code: None,
                                message: if m.is_empty() {
                                    t(Msg::ExpNotConnected).to_string()
                                } else {
                                    m
                                },
                                position: None,
                            }),
                            (Ok(()), Some(s)) => {
                                if commit {
                                    s.commit()
                                } else {
                                    s.rollback()
                                }
                            }
                            (Ok(()), None) => Ok(()),
                        };
                        runner.note_tx_ended();
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
                        runner.close_cursor();
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
                                // 접속 전 빠른 판정(docs/53) — 실행 인자 접속도 같은 규칙.
                                let (timeout, _, _) = liveness_settings();
                                let reachable = match endpoint(&spec) {
                                    Some((host, port)) => {
                                        let t = std::time::Instant::now();
                                        let up = probe::probe_once(&host, port, timeout, true)
                                            == probe::Outcome::Up;
                                        if !up {
                                            let ep = format!("{host}:{port}");
                                            let ms = t.elapsed().as_millis().to_string();
                                            emit(err(tf(Msg::ErrServerUnreachable, &[&ep, &ms])));
                                        }
                                        up
                                    }
                                    None => true,
                                };
                                if reachable && runner.connect(&spec, &mut emit) {
                                    last_ok = std::time::Instant::now();
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
                    Cmd::Run {
                        src,
                        preflight,
                        max_rows,
                    } => {
                        runner.set_max_rows(max_rows);
                        apply_fetch_settings(&mut runner);
                        if let Err(m) = ensure_alive(
                            &mut runner,
                            &active_spec,
                            &active_ep,
                            &mut suspect,
                            &mut last_ok,
                            &mut broken_told,
                            preflight,
                            true,
                            &ctx_tx,
                            &mut emit,
                        ) {
                            if !m.is_empty() {
                                emit(err(m));
                            }
                            let _ = dtx.send(Some(tf(Msg::WkErrors, &["1"])));
                            wake_now();
                            return true;
                        }
                        // 치환 변수 프롬프트는 최소 GUI에서 빈 값(T-16c에서 대화상자).
                        let mut prompt = |_: &str| Some(String::new());
                        let mut conn_err = false;
                        if let Ok(mut g) = cancel_slot.lock() {
                            *g = runner.cancel_handle();
                        }
                        let errs = runner.run_script(&src, &mut prompt, &mut |e: RunEvent| {
                            if let RunEvent::Error { error, .. } = &e {
                                conn_err |= probe::is_connection_error(error.code, &error.message);
                            }
                            emit(e);
                        });
                        suspect = conn_err;
                        if conn_err {
                            if !broken_told {
                                broken_told = true;
                                let _ = ctx_tx.send(ConnOutcome::Broken(String::new()));
                            }
                        } else {
                            last_ok = std::time::Instant::now();
                        }
                        let _ = dtx.send(if errs > 0 {
                            Some(tf(Msg::WkErrors, &[&errs.to_string()]))
                        } else {
                            None
                        });
                        wake_now();
                        true
                    }
                    Cmd::Quit => {
                        runner.close_cursor();
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
            cancel_fetch,
            cancel_run,
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

#[cfg(test)]
mod tests {
    /// docs/56 L4 — 서버 안전망 세션 파라미터: 0 = 아무것도 안 보냄 · 방언별 문장 · 세션 식별자 질의.
    #[test]
    fn server_guard_and_session_id_sql() {
        use nsql_core::Dialect;
        assert!(super::server_guard_sql(Dialect::Postgres, 0, 0).is_empty());
        let pg = super::server_guard_sql(Dialect::Postgres, 1800, 30);
        assert_eq!(pg.len(), 2);
        assert!(pg[0].contains("idle_in_transaction_session_timeout = '1800s'"));
        assert!(pg[1].contains("lock_timeout = '30s'"));
        assert_eq!(
            super::server_guard_sql(Dialect::Mssql, 1800, 30),
            vec!["SET LOCK_TIMEOUT 30000".to_string()],
            "SQL Server는 유휴 트랜잭션 타임아웃이 없다"
        );
        assert_eq!(super::server_guard_sql(Dialect::Oracle, 0, 15).len(), 1);
        assert_eq!(super::server_guard_sql(Dialect::Mysql, 0, 15).len(), 2);
        assert!(super::server_guard_sql(Dialect::Sqlite, 10, 10).is_empty());
        assert!(
            super::session_id_sql(Dialect::Postgres).is_some_and(|q| q.contains("pg_backend_pid"))
        );
        assert!(super::session_id_sql(Dialect::Mssql).is_some_and(|q| q.contains("@@SPID")));
        assert!(super::session_id_sql(Dialect::Sqlite).is_none());
    }

    use super::*;

    /// MC/DC — 비밀번호 필수 판정: 방언(SQLite 제외) · 호스트 있음 · 비밀번호 없음/빈 문자열.
    #[test]
    fn password_required_mcdc() {
        let p = |t: &str| ConnectSpec::parse(t).expect("spec");
        // 기준: 인라인 · 호스트 있음 · 비밀번호 없음 → 필수.
        assert!(password_required(
            &p("oracle://scott@db.local:1521/orcl"),
            Dialect::Oracle
        ));
        // 비밀번호 있음 → 아니오.
        assert!(!password_required(
            &p("oracle://scott/x@db.local:1521/orcl"),
            Dialect::Oracle
        ));
        // SQLite → 아니오(자격 없음).
        assert!(!password_required(
            &p("sqlite:///tmp/a.db"),
            Dialect::Oracle
        ));
        // 호스트 없음(대상만 · 기본 방언) → 아니오(드라이버가 판단).
        let mut no_host = p("oracle://scott@db.local/orcl");
        no_host.host = None;
        assert!(!password_required(&no_host, Dialect::Oracle));
        // 빈 비밀번호 = 없음.
        let mut empty = p("oracle://scott/x@db.local/orcl");
        empty.password = Some(String::new());
        assert!(password_required(&empty, Dialect::Oracle));
    }

    /// docs/53: 닿지 않는 서버(TEST-NET-1)에 접속 창 경로로 붙이면 드라이버 타임아웃 대신 **빠른 판정**이 `probe.timeout` 안에
    /// 실패를 내고 `Broken`을 알린다(막힘 0).
    #[test]
    fn unreachable_server_fails_fast_and_reports_broken() {
        let (w, events) = spawn(Dialect::Oracle, 10, true, Box::new(|| {}));
        let spec = ConnectSpec::parse("oracle://u/p@192.0.2.1:1521/db").expect("spec");
        let t = std::time::Instant::now();
        w.send(Cmd::ConnectSpec {
            spec,
            reconnect_same: false,
        });
        let mut broken = false;
        let mut failed = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline && !(broken && failed) {
            while let Ok(o) = w.conn.try_recv() {
                match o {
                    ConnOutcome::Broken(_) => broken = true,
                    ConnOutcome::ConnectFailed(_) => failed = true,
                    _ => {}
                }
            }
            if w.done.try_recv().is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = events.try_recv();
        assert!(broken, "Broken 알림");
        assert!(failed, "ConnectFailed");
        // 드라이버(OCI) 접속 타임아웃(수십 초)이 아니라 빠른 판정(기본 2초 · 설정 최대 60초) 안에 끝난다.
        assert!(t.elapsed() < Duration::from_secs(61), "{:?}", t.elapsed());
    }

    /// 즉시 해제 규약(사용자 09-16): 옛 워커가 응답 없는 서버에 갇혀 있어도(여기서는 비라우팅 주소 접속 시도)
    /// 새 워커는 독립적으로 Disconnect에 바로 답한다 — UI가 앞 명령을 기다리지 않아도 된다.
    #[test]
    fn replacement_worker_answers_while_old_one_is_stuck() {
        let (old, _old_events) = spawn(Dialect::Oracle, 10, true, Box::new(|| {}));
        // 비라우팅 주소(TEST-NET-1) — 드라이버가 없거나 즉시 실패해도 상관없다: 새 워커의 독립성만 본다.
        old.send(Cmd::Connect("mssql://u:p@192.0.2.1:1433/db".into()));
        old.send(Cmd::Disconnect);
        let (new, _events) = spawn(Dialect::Oracle, 10, true, Box::new(|| {}));
        let t = std::time::Instant::now();
        new.send(Cmd::Disconnect);
        let got = new.conn.recv_timeout(Duration::from_secs(5));
        assert!(matches!(got, Ok(ConnOutcome::Disconnected)), "{got:?}");
        assert!(t.elapsed() < Duration::from_secs(5));
        drop(old); // 손잡이를 버리면 옛 워커는 갇힌 호출이 풀린 뒤 스스로 끝난다.
    }
}
