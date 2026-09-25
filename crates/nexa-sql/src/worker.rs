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
    /// 연결 공유 층의 변수를 통째로 바꾼다(변수 창이 고쳤다 · D-135) — DB로 가는 것은 없다.
    SharedVars(Vec<nsql_script::VarState>),
    /// 앱 전역 층(docs/63 §11)을 통째로 바꾼다(시작 · 변수 창 · 다른 세션의 `VAR x GLOBAL`).
    GlobalVars(Vec<nsql_script::VarState>),
    /// 수동 커밋 모드의 Commit/Rollback(메뉴 · 단축키 · 사용자 09-15).
    Commit,
    Rollback,
    Run {
        src: String,
        /// `Some(timeout)` = 실행 전 호스트:포트 빠른 판정(신호등이 초록이 아닐 때 UI가 켠다).
        preflight: Option<Duration>,
        /// 이번 실행의 페치 상한(결과 탭의 세그먼트 크기 · 0 = 전체 · docs/43).
        max_rows: usize,
        /// ★ 실행하는 탭의 변수 표(탭 층 · D-135) — 호스트가 탭마다 들고 실행마다 넘긴다. 끝나면 `RunEvent::Vars`로 돌아간다.
        ///   연결 공유 층은 러너(= 이 세션)에 남는다. `None` = 탭 층을 건드리지 않는다(결과 새로고침 등 편집기와 무관한 실행).
        vars: Option<Vec<nsql_script::VarState>>,
        /// 탭의 치환 변수(`DEFINE` · 이름 · 원문 · 09-23) — `None` = 엔진 것 그대로.
        defines: Option<Vec<(String, String)>>,
        /// 내장 변수 층(`${workspaceFolder}` … · 호스트가 실행마다 스냅숏 · 사용자 09-23) — `None` = 엔진 것 그대로.
        intrinsic: Option<std::sync::Arc<std::collections::BTreeMap<String, String>>>,
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
    /// 접속 문자열에 비밀번호 자리가 없다 — 입력 창으로 **한 번** 묻는다(`target` = 가린 표시 · 답 = [`PwReply`]).
    PasswordNeeded {
        target: String,
        /// 들고 있던(세션 자격 금고) 비밀번호를 서버가 거부해 폐기하고 다시 묻는 것이다 — 입력 창이 그 사실을 알린다.
        rejected: bool,
    },
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
        /// 커서에서 이어 읽었다(새 SQL 없음 · docs/43 §11) — 아니면 OFFSET 재질의/재실행이었다.
        via_cursor: bool,
    },
    /// ★ 재질의 알림(docs/43 §11 · 사용자 09-22): 커서가 없어 **새 SQL을 서버에 보내기 직전** — 호스트가 실행 상태 카드를 켠다.
    Requery {
        key: u64,
        offset: usize,
        /// 0 = 전체(나머지 끝까지).
        limit: usize,
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
/// 같은 **서버**(방언·호스트·포트) — 탐색기 묶음 트리의 헤더 단위(docs/54 §9 · 09-25): DB·계정이 달라도 한 서버 아래 연결로.
pub(crate) fn same_host(a: &ConnectSpec, b: &ConnectSpec) -> bool {
    a.dialect == b.dialect && a.host == b.host && a.port == b.port
}

/// 같은 **카탈로그**(방언·호스트·포트·DB/서비스 — 계정 무관) — 탐색기 칸·메타 저장소의 단위(docs/54 §10 · 09-25 사용자
/// "인텔리센스는 서버 단위로 하나"): 같은 카탈로그의 연결들은 트리·메타를 공유하고 자격만 빌려 준다.
pub(crate) fn same_catalog(a: &ConnectSpec, b: &ConnectSpec) -> bool {
    a.dialect == b.dialect && a.host == b.host && a.port == b.port && a.database == b.database
}

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

/// ★ **비밀번호를 물어야 하는가**(docs/52 §4 · T-132 · 사용자 09-21): 접속 문자열에 비밀번호 자리가 **없을 때만**
/// (`user@host` → `None`). `user:@host`는 빈 비밀번호를 **명시**한 것(`Some("")`)이라 묻지 않고 그대로 접속한다. SQLite는 자격이
/// 없고, 호스트가 없는 대상은 드라이버가 판단한다. 자격 빌리기(같은 서버·계정 프로필의 비밀번호를 몰래 쓰기)는 하지 않는다 —
/// "비밀번호 없이 접속됨"처럼 보여 오해·오접속을 낳는다. 물어서 받은 값은 **그 접속에만** 쓰고 버린다([`PwReply`]).
pub(crate) fn password_required(spec: &ConnectSpec, default_dialect: Dialect) -> bool {
    spec.dialect.unwrap_or(default_dialect) != Dialect::Sqlite
        && spec.host.is_some()
        && spec.password.is_none()
}

/// **들고 있던 비밀번호가 무효인가** — 서버가 "사용자/비밀번호가 틀렸다"고 답한 경우만(계정 잠김 · 만료 · DB 없음 · 네트워크는 아니다:
/// 그때 다시 물어 봐야 같은 실패를 한 번 더 할 뿐이고, 잠긴 계정에는 시도를 보태지 않는다). Oracle ORA-01017 · SQL Server 18456 ·
/// MySQL 1045 · PostgreSQL "password authentication failed"(28P01).
pub(crate) fn stale_password(dialect: Dialect, e: &DbError) -> bool {
    match dialect {
        // ORA-01005 = 빈 비밀번호를 줬다("null password given") — `user:@host`가 거부된 모양.
        Dialect::Oracle => {
            matches!(e.code, Some(1017 | 1005))
                || e.message.contains("ORA-01017")
                || e.message.contains("ORA-01005")
        }
        Dialect::Mssql => e.code == Some(18456),
        Dialect::Mysql => e.code == Some(1045),
        Dialect::Postgres => {
            let low = e.message.to_ascii_lowercase();
            low.contains("password authentication failed") || low.contains("28p01")
        }
        _ => false,
    }
}

/// **자격 자리의 이름** — 비밀번호만 뺀 "같은 서버·계정"(= [`same_server`]가 보는 것과 같은 항목). 접속 문자열의 `?schema=`·`?env=`
/// 같은 덧붙임은 자격과 무관하므로 넣지 않는다. 세션 자격 금고(nsql-vault `session`)의 열쇠이자 봉투의 도메인이다.
pub(crate) fn cred_id(spec: &ConnectSpec, default_dialect: Dialect) -> String {
    format!(
        "{}://{}@{}:{}/{}#{}",
        spec.dialect.unwrap_or(default_dialect),
        spec.user.as_deref().unwrap_or(""),
        spec.host.as_deref().unwrap_or("").to_ascii_lowercase(),
        spec.port.map_or(String::new(), |p| p.to_string()),
        spec.database.as_deref().unwrap_or(""),
        spec.role.as_deref().unwrap_or(""),
    )
}

/// 같은 서버에 붙어 있는 탭에서 `CONNECT`가 들고 온 비밀번호가 **지금 자격과 다른가** — 다르면 "그대로 유지"가 아니라 다시 접속한다.
/// 비밀번호 자리가 없는 `CONNECT`(`user@host` · 프로필 이름)와 금고를 끈 경우는 "같다"로 본다(종전 동작).
pub(crate) fn credential_changed(spec: &ConnectSpec, default_dialect: Dialect) -> bool {
    match spec.password.as_deref() {
        Some(p) if remember_session_password() => {
            !nsql_vault::session::matches(&cred_id(spec, default_dialect), p)
        }
        _ => false,
    }
}

/// 설정 `connect.remember_session_password`(기본 켬) — 입력한 비밀번호를 이번 실행 동안 메모리 봉투로 재사용하는가.
pub(crate) fn remember_session_password() -> bool {
    nsql_settings::Settings::open_default()
        .map_or(true, |s| s.flag("connect.remember_session_password"))
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
    runner.engine.settings.into_first = s.get("vars.into_policy") == Some("first");
    // 부하원 스위치(39 §3): 호출 서명 조회 · PG 커서 이름 풀기.
    runner.signature_lookup = s.get("vars.signature_lookup").is_none_or(|v| v == "on");
    runner.refcursor_expand = s.get("pg.refcursor_expand").is_none_or(|v| v == "on");
    // T-153: `${이름:형식}` 치환 · 돌아온 값의 크기 상한.
    runner.engine.settings.brace_subst = s.get("vars.brace_subst").is_none_or(|v| v == "on");
    runner.engine.settings.env_subst = s.get("vars.env_subst").is_none_or(|v| v == "on");
    runner.engine.settings.expand_at_use = s.get("vars.expand_at") == Some("use");
    runner.engine.settings.max_value_bytes =
        (s.int("vars.max_value_kb").max(0) as usize).saturating_mul(1024);
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
    /// 커서 결과(REF CURSOR · T-202)의 커서가 이미 닫혔다(유휴·다른 실행·커밋) — 재질의가 없으므로 "더 없음"으로 끝낸다.
    CursorGone,
}

/// 입력 창의 답(D-137) — 워커는 `RunEvent::InputNeeded`를 낸 뒤 이것을 기다린다(그동안 세션은 "바쁨").
pub(crate) enum InputReply {
    /// (종류, 이름, 친 글) — 빈 목록 = "이번엔 건너뛰기"(값 없이 그대로 실행 = 종전 동작).
    Values(Vec<(nsql_script::InputKind, String, String)>),
    /// 실행하지 않는다.
    Cancel,
}

/// 비밀번호 입력 창의 답 — 워커는 `ConnOutcome::PasswordNeeded`를 낸 뒤 이것을 기다린다(그동안 세션은 "바쁨").
/// 값은 [`nsql_core::Secret`]에 담겨 온다: 접속에 한 번 쓰고 **0으로 덮어써** 버린다 · 스펙·프로필·로그·설정 어디에도 남기지 않는다.
pub(crate) enum PwReply {
    Value(nsql_core::Secret),
    Cancel,
}

pub(crate) struct Handle {
    tx: mpsc::Sender<Cmd>,
    /// 비밀번호 입력 창의 답을 보내는 길(접속을 여는 자리에서 기다린다).
    pw_tx: mpsc::Sender<PwReply>,
    /// 입력 창의 답을 보내는 길(명령 큐와 따로 — 워커가 실행 명령 안에서 기다린다).
    input_tx: mpsc::Sender<InputReply>,
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

    /// 비밀번호 입력 창의 답.
    pub(crate) fn password(&self, r: PwReply) {
        let _ = self.pw_tx.send(r);
    }

    /// 입력 창의 답.
    pub(crate) fn input(&self, r: InputReply) {
        let _ = self.input_tx.send(r);
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
    let (input_tx, input_rx) = mpsc::channel::<InputReply>();
    let (pw_tx, pw_rx) = mpsc::channel::<PwReply>();
    let cancel_fetch = Arc::new(AtomicBool::new(false));
    let cancel_flag = cancel_fetch.clone();
    let cancel_run: Arc<Mutex<Option<Arc<dyn nsql_core::CancelHandle>>>> =
        Arc::new(Mutex::new(None));
    let cancel_slot = cancel_run.clone();
    std::thread::Builder::new()
        .name("nsql-worker".into())
        .spawn(move || {
            let wake_shared = std::sync::Arc::new(std::sync::Mutex::new(wake));
            // ★ 접속을 여는 **한 자리** — 비밀번호 자리가 없는 접속 문자열이면 여기서 물어본다(편집기 `CONNECT` · 시작 인자 ·
            //   탐색기 "연결" · 끊긴 뒤 재접속이 모두 이 길을 지난다). 받은 값은 이 접속에만 쓰고 덮어써 지운다.
            let opener: Opener = {
                let ask_tx = ctx_tx.clone();
                let ask_wake = wake_shared.clone();
                Box::new(
                    move |spec: &ConnectSpec| -> Result<Box<dyn Session>, DbError> {
                        let dialect = spec.dialect.unwrap_or(default_dialect);
                        // ★ **명시한 비밀번호**(`user:pw@host` · 빈 값 `user:@host` 포함 · 저장된 프로필): 그 서버·계정의 자격 자리를
                        //   이 값으로 **바꿔 든다**(자리는 하나) → 거부되면 자리를 비운다 = 앞서 입력해 둔 값도 다시 쓰이지 않고,
                        //   다음 `user@host` 접속은 다시 묻는다(사용자 09-21).
                        if let (Some(p), true) = (
                            spec.password.as_ref(),
                            dialect != Dialect::Sqlite && spec.host.is_some(),
                        ) {
                            let slot = cred_id(spec, default_dialect);
                            if remember_session_password() {
                                nsql_vault::session::remember(
                                    &slot,
                                    &nsql_core::Secret::new(p.clone()),
                                );
                            }
                            let r = nsql_drivers::open(spec, default_dialect);
                            if let Err(e) = &r {
                                if stale_password(dialect, e) {
                                    nsql_vault::session::forget(&slot);
                                }
                            }
                            return r;
                        }
                        if !password_required(spec, default_dialect) {
                            return nsql_drivers::open(spec, default_dialect);
                        }
                        // ★ 세션 자격 금고(설정 `connect.remember_session_password` · nsql-vault `session`): 이번 실행에서 같은
                        //   서버·계정에 이미 입력한 비밀번호가 있으면 **묻지 않고** 그것으로 연다. 실패하면 바로 잊는다(다음에는 묻는다 —
                        //   틀린 비밀번호로 되풀이 시도해 계정을 잠그지 않는다).
                        let vault_id = cred_id(spec, default_dialect);
                        let remember = remember_session_password();
                        let mut rejected = false;
                        if !remember {
                            nsql_vault::session::forget(&vault_id);
                        } else if let Some(secret) = nsql_vault::session::recall(&vault_id) {
                            let mut once = spec.clone();
                            once.password = Some(secret.expose().to_string());
                            drop(secret);
                            let r = nsql_drivers::open(&once, default_dialect);
                            nsql_core::secret::wipe_opt(&mut once.password);
                            match r {
                                // ★ 들고 있던 비밀번호가 **무효**(서버에서 바뀌었다 · 사용자 09-21): 폐기하고 **이 접속 안에서 바로 다시
                                //   묻는다**. 그 밖의 실패(네트워크 · 리스너 · 서비스 이름)는 비밀번호 탓이 아니므로 그대로 두고 오류만 낸다.
                                Err(e)
                                    if stale_password(
                                        spec.dialect.unwrap_or(default_dialect),
                                        &e,
                                    ) =>
                                {
                                    nsql_vault::session::forget(&vault_id);
                                    rejected = true;
                                }
                                other => return other,
                            }
                        }
                        // 밀린 답(앞선 물음의 늦은 답)은 버린다 — `Secret`이라 버려지면서 지워진다.
                        while pw_rx.try_recv().is_ok() {}
                        let _ = ask_tx.send(ConnOutcome::PasswordNeeded {
                            target: spec.redacted(),
                            rejected,
                        });
                        if let Ok(f) = ask_wake.lock() {
                            f();
                        }
                        let secret = match pw_rx.recv() {
                            Ok(PwReply::Value(s)) => s,
                            Ok(PwReply::Cancel) | Err(_) => {
                                return Err(DbError {
                                    code: None,
                                    message: t(Msg::ErrPasswordCancelled).into(),
                                    position: None,
                                });
                            }
                        };
                        // 이 접속에만 쓰는 스펙 사본 — 드라이버가 돌아오면 비밀번호를 0으로 덮어쓴다(성공·실패 공통).
                        let mut once = spec.clone();
                        once.password = Some(secret.expose().to_string());
                        let r = nsql_drivers::open(&once, default_dialect);
                        nsql_core::secret::wipe_opt(&mut once.password);
                        // 접속에 **성공한** 값만 금고에 든다(봉투 · 평문 아님).
                        if r.is_ok() && remember {
                            nsql_vault::session::remember(&vault_id, &secret);
                        }
                        drop(secret);
                        r
                    },
                )
            };
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
                        let mut via_cursor = false;
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
                                if !runner.cursor_matches(&sql, offset)
                                    && !nsql_run::Runner::requery_ok(&sql)
                                {
                                    // ★ T-202: 커서 결과인데 커서가 없다 — 프로시저를 다시 돌리지 않고 빈 페이지 + 더 없음.
                                    stop = Some(FetchStop::CursorGone);
                                    Ok((nsql_core::ResultSet::default(), false, Duration::ZERO))
                                } else if limit == 0 {
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
                                    via_cursor = cursor_h.is_some() && !strict_replace;
                                    if !via_cursor {
                                        let _ = ctx_tx.send(ConnOutcome::Requery {
                                            key,
                                            offset: if strict_replace { 0 } else { offset },
                                            limit: 0,
                                        });
                                        wake_now();
                                    }
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
                                    let _ = ctx_tx.send(ConnOutcome::Requery {
                                        key,
                                        offset: 0,
                                        limit: need,
                                    });
                                    wake_now();
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
                                    if !(runner.cursor_matches(&sql, offset)
                                        && runner.cursor().is_some())
                                    {
                                        let _ = ctx_tx.send(ConnOutcome::Requery {
                                            key,
                                            offset,
                                            limit,
                                        });
                                        wake_now();
                                    }
                                    runner
                                        .fetch_page(&sql, offset, limit)
                                        .map(|(rs, more, tl, via)| {
                                            via_cursor = via;
                                            (rs, more, tl.total())
                                        })
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
                            via_cursor,
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
                        vars,
                        defines,
                        intrinsic,
                    } => {
                        if let Some(v) = vars {
                            runner.engine.vars.set_local(v);
                        }
                        if let Some(d) = defines {
                            runner.engine.defines = d.into_iter().collect();
                            runner.engine.defines_dirty = true;
                        }
                        if let Some(m) = intrinsic {
                            runner.engine.settings.intrinsic = m;
                        }
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
                        // ★ 실행 전에 빠진 입력을 **한 번** 묻는다(D-137 · 설정 `vars.undeclared` = prompt|auto|error · docs/63 V3).
                        //   prompt = 입력 창(UI가 답할 때까지 기다린다) · auto = 종전(NULL/빈 글) · error = 실행하지 않는다.
                        let policy = nsql_settings::Settings::open_default()
                            .ok()
                            .and_then(|s| s.get("vars.undeclared").map(str::to_string))
                            .unwrap_or_else(|| "prompt".into());
                        if policy != "auto" {
                            let needs = runner.missing_inputs(&src);
                            if !needs.is_empty() {
                                if policy == "error" {
                                    let names: Vec<String> =
                                        needs.iter().map(|n| n.name.clone()).collect();
                                    emit(err(tf(Msg::ErrInputsMissing, &[&names.join(", ")])));
                                    let _ = dtx.send(Some(tf(Msg::WkErrors, &["1"])));
                                    wake_now();
                                    return true;
                                }
                                // 밀린 답을 비우고(앞 실행의 늦은 답) 묻는다.
                                while input_rx.try_recv().is_ok() {}
                                emit(RunEvent::InputNeeded { needs });
                                wake_now();
                                match input_rx.recv() {
                                    Ok(InputReply::Values(v)) => runner.apply_inputs(v),
                                    Ok(InputReply::Cancel) | Err(_) => {
                                        let _ =
                                            dtx.send(Some(t(Msg::StInputCancelled).to_string()));
                                        wake_now();
                                        return true;
                                    }
                                }
                            }
                        }
                        // 실행 중에 새로 드러난 치환 변수(앞의 훑기가 못 본 것)는 빈 값(종전).
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
                    Cmd::SharedVars(v) => {
                        runner.engine.vars.set_shared(v);
                        true
                    }
                    Cmd::GlobalVars(v) => {
                        runner.engine.vars.set_global(v);
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
            pw_tx,
            input_tx,
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
        // 빈 비밀번호를 **명시**(`user:@host`) = 묻지 않는다(사용자 09-21) · 자리가 아예 없으면 묻는다.
        assert!(!password_required(
            &p("oracle://scott:@db.local/orcl"),
            Dialect::Oracle
        ));
        let mut none = p("oracle://scott/x@db.local/orcl");
        none.password = None;
        assert!(password_required(&none, Dialect::Oracle));
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

    /// 일회성 비밀번호(사용자 09-21): 비밀번호 자리가 없는 접속 문자열 = 서버에 가기 **전에** 묻는다 → 취소하면 접속 실패
    /// (아무것도 보내지 않음) · `user:@host`(빈 비밀번호 명시)는 묻지 않는다. 로컬 리스너 = 빠른 판정만 통과시키는 더미(네트워크 0).
    #[test]
    fn missing_password_is_asked_once_and_cancel_aborts() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        // 받은 연결은 바로 닫는다(드라이버가 응답을 기다리며 매달리지 않게) — 시험이 끝나면 깃발로 멈춘다.
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let acceptor = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let Ok((c, _)) = listener.accept() {
                        drop(c);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            })
        };
        let run = |target: String, reply: Option<PwReply>| -> (bool, Option<String>) {
            let (w, _events) = spawn(Dialect::Oracle, 10, true, Box::new(|| {}));
            w.send(Cmd::ConnectSpec {
                spec: ConnectSpec::parse(&target).expect("spec"),
                reconnect_same: false,
            });
            // 물음에 답할 때(①)는 넉넉히 · 묻지 않아야 하는 경우(②③)는 물음이 올 시간만(빠른 판정 뒤 곧바로 온다).
            let wait = if reply.is_some() { 20 } else { 6 };
            let mut reply = reply;
            let (mut asked, mut failed) = (false, None);
            let deadline = std::time::Instant::now() + Duration::from_secs(wait);
            while std::time::Instant::now() < deadline && failed.is_none() {
                while let Ok(o) = w.conn.try_recv() {
                    match o {
                        ConnOutcome::PasswordNeeded { target, rejected } => {
                            assert!(!target.contains("secret"), "{target}");
                            assert!(!rejected, "거부 표시는 인증 실패 뒤에만");
                            asked = true;
                            if let Some(r) = reply.take() {
                                w.password(r);
                            }
                        }
                        ConnOutcome::ConnectFailed(e) => failed = Some(e),
                        _ => {}
                    }
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            (asked, failed)
        };
        // ① 자리가 없다 → 묻는다 → 취소 = "취소" 오류(드라이버에 가지 않았다).
        let (asked, failed) = run(
            format!("oracle://scott@127.0.0.1:{port}/orcl"),
            Some(PwReply::Cancel),
        );
        assert!(asked, "PasswordNeeded");
        assert_eq!(failed.as_deref(), Some(t(Msg::ErrPasswordCancelled)));
        // ③ 세션 자격 금고: 이번 실행에서 이 서버·계정의 비밀번호를 이미 받았다 → **묻지 않고** 그것으로 연다. 더미 리스너의 실패는
        //   인증 실패가 아니므로(네트워크) 비밀번호를 **버리지 않는다** — 버리고 다시 묻는 것은 서버가 비밀번호를 거부했을 때뿐.
        if remember_session_password() {
            let target = format!("oracle://scott@127.0.0.1:{port}/orcl");
            let id = cred_id(&ConnectSpec::parse(&target).expect("spec"), Dialect::Oracle);
            nsql_vault::session::remember(&id, &nsql_core::Secret::new("secret".into()));
            let (asked, failed) = run(target, None);
            assert!(!asked, "금고에 있으면 묻지 않는다");
            // 실패 글은 환경에 따라 늦게 올 수 있다(CI Windows = Oracle 클라이언트 없음) — 오면 비밀번호가 없는지만 본다.
            assert!(failed.is_none_or(|e| !e.contains("secret")));
            assert!(
                nsql_vault::session::recall(&id).is_some(),
                "비밀번호 탓이 아닌 실패로는 잊지 않는다"
            );
            nsql_vault::session::forget(&id);
        }
        // ② 빈 비밀번호를 명시 → 묻지 않는다(더미 리스너라 드라이버 오류로 끝난다 — 그 글은 보지 않는다).
        let (asked, failed) = run(format!("oracle://scott:@127.0.0.1:{port}/orcl"), None);
        assert!(!asked, "명시한 빈 비밀번호는 묻지 않는다");
        let _ = failed; // 드라이버의 실패 시점은 환경마다 다르다 — 이 시험의 관심은 "묻지 않는다"뿐.
        stop.store(true, Ordering::Relaxed);
        let _ = acceptor.join();
    }

    /// 사용자 09-21 시나리오: ① 입력해 둔 값이 자리에 있다 → ② 같은 서버에 `user:@host`(빈 비밀번호 명시)로 접속 = 자리가 빈 값으로
    /// **바뀐다**(다른 자격이므로 "그대로 유지"가 아니다) → ③ 서버가 거부 = 자리가 **빈다** → ④ 다음 `user@host`는 다시 묻는다.
    /// (②의 실제 거부는 실서버가 있어야 하므로 여기서는 자리의 변화를 단계별로 본다 · 더미 리스너 = 네트워크 0.)
    #[test]
    fn explicit_password_replaces_the_slot_and_rejection_empties_it() {
        if !remember_session_password() {
            return;
        }
        let none = ConnectSpec::parse("oracle://slot_user@slot-host:1521/svc").expect("spec");
        let empty =
            ConnectSpec::parse("oracle://slot_user:@slot-host:1521/svc?schema=HR").expect("spec");
        let id = cred_id(&none, Dialect::Oracle);
        assert_eq!(
            id,
            cred_id(&empty, Dialect::Oracle),
            "비밀번호·덧붙임만 다른 것은 같은 자리"
        );
        nsql_vault::session::remember(&id, &nsql_core::Secret::new("typed".into()));
        // 비밀번호 자리가 없는 CONNECT = 자격이 바뀐 것이 아니다 · 같은 값 = 아니다 · 다른 값(빈 값 포함) = 바뀌었다.
        assert!(!credential_changed(&none, Dialect::Oracle));
        let mut same = none.clone();
        same.password = Some("typed".into());
        assert!(!credential_changed(&same, Dialect::Oracle));
        assert!(credential_changed(&empty, Dialect::Oracle));
        // ② 명시한 값이 자리를 바꾼다 → ③ 거부되면 비운다(오프너가 하는 두 걸음).
        nsql_vault::session::remember(&id, &nsql_core::Secret::new(String::new()));
        assert!(nsql_vault::session::matches(&id, ""));
        assert!(stale_password(
            Dialect::Oracle,
            &DbError {
                code: Some(1005),
                message: "ORA-01005: null password given; logon denied".into(),
                position: None,
            }
        ));
        nsql_vault::session::forget(&id);
        // ④ 자리가 비었다 → `user@host`는 다시 묻는다(`password_required` + 금고에 없음).
        assert!(nsql_vault::session::recall(&id).is_none());
        assert!(password_required(&none, Dialect::Oracle));
    }

    /// 무효 판정 = "사용자/비밀번호가 틀렸다"만. 잠김·만료·DB 없음·네트워크는 다시 묻지 않는다(방언마다 하나씩 뒤집어 본다).
    #[test]
    fn stale_password_is_only_a_rejected_credential() {
        let e = |code: Option<i64>, m: &str| DbError {
            code,
            message: m.into(),
            position: None,
        };
        assert!(stale_password(Dialect::Oracle, &e(Some(1017), "ORA-01017")));
        assert!(stale_password(
            Dialect::Oracle,
            &e(None, "OCI Error: ORA-01017: invalid")
        ));
        assert!(stale_password(
            Dialect::Oracle,
            &e(Some(1005), "ORA-01005: null password given")
        ));
        assert!(!stale_password(
            Dialect::Oracle,
            &e(Some(28000), "ORA-28000: locked")
        ));
        assert!(!stale_password(
            Dialect::Oracle,
            &e(Some(12541), "ORA-12541: no listener")
        ));
        assert!(stale_password(
            Dialect::Mssql,
            &e(Some(18456), "Login failed")
        ));
        assert!(!stale_password(
            Dialect::Mssql,
            &e(Some(4060), "Cannot open database")
        ));
        assert!(stale_password(
            Dialect::Postgres,
            &e(None, "FATAL: password authentication failed for user \"u\"")
        ));
        assert!(!stale_password(
            Dialect::Postgres,
            &e(None, "connection refused")
        ));
        assert!(!stale_password(Dialect::Sqlite, &e(Some(1017), "x")));
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
