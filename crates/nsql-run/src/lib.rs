//! # nsql-run — 호스트 오케스트레이션 (docs/01 §3)
//!
//! `Engine::plan` → `Action` → `Session::execute` → `Engine::absorb`를 잇고, 결과를 **이벤트**로 흘린다.
//! GUI는 이벤트를 채널로 받아 그리고, CLI는 그 자리에서 찍는다. 이 크레이트는 스레드를 만들지 않는다 —
//! 호스트가 워커에서 돌린다(UI 스레드는 기다리지 않는다).
//!
//! OUT 회수 규약: Oracle·MSSQL 어댑터는 `ExecResult::out_params`를 직접 채운다. OUT 파라미터가 없는 방언
//! (SQLite·PG·MySQL·ODBC)은 **마지막 결과 집합이 1행이고 컬럼 이름이 변수 이름과 같으면** 그 값을 흡수한다
//! (`EXEC :V := expr` → `SELECT expr AS "V"` 경로).

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod bulk;
pub mod meta;
pub mod txlog;

use nsql_core::{
    Column, CursorHandle, DbError, Dialect, ExecRequest, ExecResult, ResultSet, Session, Stage,
    Timeline, Value,
};
use nsql_i18n::{t, tf, Msg};
use nsql_script::{
    parse_spool, Action, ConnectSpec, Engine, Item, ItemKind, PrepareMode, Prepared, SpoolCmd,
    SpoolMode, SqlKind,
};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// `@`/`@@` 중첩 상한(자기 자신을 부르는 스크립트 보호).
const MAX_SCRIPT_DEPTH: usize = 32;

/// 호스트로 흘러가는 이벤트.
/// [`Runner::apply_changes`] 입력 한 문장 — 실행문 + (UPDATE/DELETE) 사전 검사문 + 사람이 읽는 라벨(보고용).
#[derive(Clone, Debug, PartialEq)]
pub struct ApplyStmt {
    pub req: ExecRequest,
    /// 같은 WHERE의 `SELECT COUNT(*)`(대상 행 수 = 1 검사) — INSERT는 None.
    pub guard: Option<ExecRequest>,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyPhase {
    Begin,
    /// 쓰기 전 대상 행 수 검사 — 아무것도 바뀌지 않았다.
    PreCheck,
    Execute,
    Commit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyError {
    pub index: usize,
    pub phase: ApplyPhase,
    pub message: String,
    pub label: String,
}

/// [`Runner::apply_changes`] 결과.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub total: usize,
    /// 성공한 문장 수(롤백 뒤에는 0).
    pub done: usize,
    /// 문장마다 영향 행 수(실패 문장 전까지).
    pub affected: Vec<u64>,
    pub error: Option<ApplyError>,
    pub committed: bool,
    /// 자동 = 전체 롤백 · 수동 = 세이브포인트로 우리 부분만 되돌림.
    pub rolled_back: bool,
    /// ★ 되돌리지 못했다 — 사용자가 ROLLBACK 해야 한다(수동 모드 · 세이브포인트 실패 등).
    pub rollback_needed: bool,
    /// 수동 모드 — 트랜잭션이 열린 채 남았다(커밋/롤백은 사용자).
    pub tx_left_open: bool,
    /// ★ 실행한 문장마다 한 줄(사전 검사 포함 · 순서대로) — 호스트가 트랜잭션 로그(44)에 싣는다(사용자 09-26 "로그 창에 안 보임").
    pub log: Vec<ApplyLogItem>,
}

/// 적용 중 실행한 문장 하나의 기록(트랜잭션 로그용).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ApplyLogItem {
    pub sql: String,
    /// 이 문장에 매핑된 바인드(로그 3계층 · docs/44 §9).
    pub binds: Vec<txlog::TxBind>,
    /// 사전 검사문(`SELECT COUNT(*)`)인가 — 로그 창에서는 Util(전체 보기에서만).
    pub guard: bool,
    pub elapsed: Duration,
    /// 영향 행 수(사전 검사 = 대상 행 수).
    pub rows: Option<u64>,
    pub error: Option<String>,
}

/// 방언별 세이브포인트 문장 (설정 · 되돌리기 · 해제).
fn savepoint_sql(
    dialect: Dialect,
) -> (
    Option<&'static str>,
    Option<&'static str>,
    Option<&'static str>,
) {
    // T-149 ④(10-30): 문장은 방언 표(`Syntax::savepoint` · None = 미지원 = DuckDB 1.x · Access · ODBC 일반 · ClickHouse).
    match nsql_core::Caps::of(dialect).syntax.savepoint {
        Some((set, back, release)) => (Some(set), Some(back), release),
        None => (None, None, None),
    }
}

/// `SELECT COUNT(*)` 결과의 수.
fn count_of(r: &ExecResult) -> Option<u64> {
    let v = r.result_sets.first()?.rows.first()?.first()?;
    match v {
        nsql_core::Value::Int(i) => u64::try_from(*i).ok(),
        nsql_core::Value::Float(f) => Some(*f as u64),
        nsql_core::Value::Decimal(d) | nsql_core::Value::Str(d) => d.trim().parse().ok(),
        _ => None,
    }
}

/// ★ 운영 보류 커밋 정책(docs/114 §5 · D-295~D-299 · 10-15): 호스트가 운영 접속 + 설정으로 **이번 실행**에 넘긴다 · `None` = 보통 실행.
/// 보류 모드 = 자동 커밋 모드여도 변경 문장을 명시 트랜잭션 안에서 돌리고(수동 모드의 시작문 규칙 재사용) 끝에 커밋하지 않는다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HoldPolicy {
    /// 실행 합계 영향 행 수 기준(0 = 변경이 있으면 늘 보류 · D-295 · D-296).
    pub rows: u64,
    /// PL/SQL 블록·EXEC·CALL = 행 수와 무관하게 보류(D-297).
    pub blocks: bool,
    /// WHERE 없는 UPDATE/DELETE = 행 수와 무관하게 보류.
    pub no_where: bool,
    /// 스크립트의 **첫** `COMMIT` 문은 이미 확인됐다(보류 카드/토스트 [커밋] 뒤 이어 실행 · 한 번만 · D-299).
    pub commit_ok: bool,
}

/// 보류된 문장 하나(카드 · 트랜잭션 로그 · 상태줄).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldStmt {
    pub index: usize,
    pub summary: String,
    pub rows: Option<u64>,
    /// PL/SQL 블록·EXEC·CALL(행 수 불명 · D-297).
    pub block: bool,
    /// WHERE 없는 UPDATE/DELETE.
    pub no_where: bool,
}

/// 보류 판정(순수 · D-295~D-297): 블록·WHERE 없음은 그 스위치가 켜져 있으면 무조건 · 아니면 합계 ≥ 기준(기준 0 = 늘).
#[must_use]
pub fn hold_decision(p: &HoldPolicy, total_rows: u64, block: bool, no_where: bool) -> bool {
    (p.blocks && block) || (p.no_where && no_where) || total_rows >= p.rows
}

/// 사용자 `COMMIT` 문인가(`END`·`COMMIT WORK` 포함 · `ROLLBACK`/`ABORT` 제외 · 순수).
#[must_use]
pub fn is_commit_sql(sql: &str) -> bool {
    if nsql_core::TxControl::of_sql(sql) != nsql_core::TxControl::End {
        return false;
    }
    !matches!(nsql_core::first_keyword(sql).as_str(), "ROLLBACK" | "ABORT")
}

/// `UPDATE`/`DELETE`에 **깊이 0의 `WHERE`가 없다**(순수 · 주석·문자열·따옴표 식별자 안은 무시 · 서브쿼리 안의 WHERE는 세지 않음 ·
/// 다른 문장 = false). 운영 보류의 "WHERE 없는 변경" 판정(114 §3-3 · G2).
#[must_use]
pub fn dml_without_where(sql: &str) -> bool {
    let cs: Vec<char> = sql.chars().collect();
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut words: Vec<(String, i32)> = Vec::new();
    while i < cs.len() {
        let c = cs[i];
        if c == '-' && cs.get(i + 1) == Some(&'-') {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && cs.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < cs.len() && !(cs[i] == '*' && cs[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            let q = c;
            i += 1;
            while i < cs.len() {
                if cs[i] == q {
                    if q == '\'' && cs.get(i + 1) == Some(&'\'') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if c == '[' {
            while i < cs.len() && cs[i] != ']' {
                i += 1;
            }
            i += 1;
            continue;
        }
        if c == '(' {
            depth += 1;
            i += 1;
            continue;
        }
        if c == ')' {
            depth -= 1;
            i += 1;
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            let st = i;
            while i < cs.len() && (cs[i].is_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            let w: String = cs[st..i].iter().collect::<String>().to_ascii_uppercase();
            words.push((w, depth));
            continue;
        }
        i += 1;
    }
    let Some((first, _)) = words.first() else {
        return false;
    };
    if first != "UPDATE" && first != "DELETE" {
        return false;
    }
    !words.iter().any(|(w, d)| *d == 0 && w == "WHERE")
}

#[derive(Debug)]
pub enum RunEvent {
    /// 항목 실행 시작(줄 · 종류 요약). `server` = 이 항목이 서버로 가는가([`goes_to_server`]) — 클라이언트 명령(`PRINT` ·
    /// `VARIABLE` · `DEFINE` · `SET` …)은 네트워크를 만들지 않으므로 호스트는 전송 로그를 내지 않는다(mac 09-21).
    Begin {
        index: usize,
        line: usize,
        summary: String,
        server: bool,
    },
    /// 조회 결과. `more` = 페치 상한(`Runner::max_rows`)에서 잘렸다(서버에 행이 더 있다).
    /// `label` = 이 결과의 이름(REF CURSOR 변수 이름 등 · 호스트가 결과 탭 제목·머리줄로 쓴다 · 보통 조회는 `None`).
    ResultSet {
        index: usize,
        rs: ResultSet,
        elapsed: Duration,
        more: bool,
        label: Option<String>,
    },
    /// **다시 접속하려다 기존 접속을 잃었다**(같은 서버·계정에 다른 자격으로 `CONNECT` → 기존 접속을 먼저 닫음 → 새 접속 실패 ·
    /// 바로 뒤에 그 `Error`가 온다). `Disconnected`(스크립트 `DISCONNECT`)와 구별한다 — 호스트는 `Disconnected`를 보면 전용 세션을
    /// 거두고 탭을 공유 연결로 돌려보내는데, 접속 실패에 그렇게 하면 오류 표시가 세션과 함께 사라지고 탭이 엉뚱한 서버에 붙는다.
    ConnectionClosed,
    /// ★ 실행 전에 값이 필요한 입력(D-137 · docs/63 V3) — 호스트가 **한 번** 묻고 [`Runner::apply_inputs`]로 돌려준다.
    /// 러너가 내지 않는다(호스트가 [`Runner::missing_inputs`]로 찾아 자기 채널에 싣는 용도).
    InputNeeded {
        needs: Vec<nsql_script::InputNeed>,
    },
    /// ★ 변수 표가 바뀌었다(실행 하나가 끝날 때 **한 번** · 바뀐 것이 있을 때만 · docs/63 §3). `local` = 탭 층 전체 ·
    /// `shared` = 연결 공유 층 전체 · `changed` = 이번 실행에서 바뀐 이름(보여 줄 표기). 호스트는 이것으로 탭의 표를 갱신하고
    /// 패널·로그를 그린다(매 프레임 0).
    Vars {
        local: Vec<nsql_script::VarState>,
        shared: Vec<nsql_script::VarState>,
        /// 앱 전역 층(docs/63 §11 · 스크립트 `VAR x GLOBAL`로 올린 것을 호스트가 받아 저장·전파).
        global: Vec<nsql_script::VarState>,
        changed: Vec<String>,
        /// 치환 변수 **탭 층**(`DEFINE` · 이름 · 원문 · 표시값 = 사용 시 모드면 `원문 → 값` · 09-23) — 호스트가 탭마다 들고 실행마다 되돌려 준다.
        defines: Vec<(String, String, String)>,
        /// ★ 치환 변수 **연결 공유 층**(`DEFINE x SHARE` · T-153 후속 10-26) — 원본은 이 러너의 `MacroStore`(세션 수명) · 호스트는 보여 주기만.
        defines_shared: Vec<(String, String, String)>,
        /// ★ 치환 변수 **앱 전역 층**(`DEFINE x GLOBAL`) — 호스트가 받아 다른 세션 러너에 `set_layer(Global)`로 전파한다.
        defines_global: Vec<(String, String, String)>,
    },
    /// 수동 커밋 모드에서 **변경 없는 트랜잭션을 러너가 끝냈다**(docs/56 L1 · `deferred` = 열린 커서가 닫힐 때 끝난다).
    ReadTxEnded {
        index: usize,
        deferred: bool,
    },
    /// DML/DDL 완료.
    Done {
        index: usize,
        rows_affected: Option<u64>,
        elapsed: Duration,
        /// ★ T-416 전달 모드 꼬리표(`bind N` · `declare-prepend` · `inline N` · 빈 글 = 매개변수 없음 · `nsql_script::delivery_tag`).
        delivery: String,
    },
    /// `PRINT` — 이름·값.
    Print {
        pairs: Vec<(String, Value)>,
    },
    /// 인자 없는 `VARIABLE` — 선언된 변수의 이름·타입(SQL*Plus 관용).
    VarList {
        vars: Vec<(String, nsql_core::VarType)>,
    },
    /// 서버 메시지(DBMS_OUTPUT · T-SQL PRINT) · 엔진 정보.
    Message(String),
    /// ★ 서버 유형 지정(10-01 · `CONNTYPE prod`): 호스트(GUI)가 세션 유형을 **임시로** 바꾼다(프로필에 저장 안 함) ·
    /// 값 = 정규화된 저장 값 `none`|`dev`|`test`|`prod`(입력은 정식·3자리 약어·별칭).
    ConnType(String),
    /// ★ `SHOW CONN` — 호스트가 접속 정보를 쓴다(GUI = Output · CLI = 한 줄).
    ShowConn,
    /// ★ `USE db`가 성공했다(10-01 · 101 §3): 이 세션의 현재 DB — 호스트는 탐색기 현재 DB 표시·메타 추종·상태줄에 반영(왕복 0).
    DbChanged(String),
    /// ★ 눈에 띄어야 하는 안내(T-202 · 사용자 09-24 "강제 전체 조회를 눈에 띄게") — GUI 상태줄 + 로그 · CLI stderr. 텍스트에 ⚠.
    Warning(String),
    /// ★ 드라이버 제약 안내(docs/110 §6 · 사용자 10-13 "Warning 토스트로 안내"): `at_connect` = 접속 직후 한 번(`Session::limits`) ·
    /// 아니면 이번 실행이 제약에 닿았다(`ExecResult::limits_hit`). 호스트 = Warning 토스트 + Output + 로그 · CLI = stderr.
    DriverLimits {
        driver: String,
        limits: Vec<nsql_core::DriverLimit>,
        at_connect: bool,
    },
    Connected {
        description: String,
        dialect: Dialect,
        /// ★ 서버가 답한 **현재 스키마**(`nsql_catalog::current_schema` · 접속 직후 1회 · 모르면 빈 글) — 사용자 이름 ≠ 기본 스키마인
        /// 계정(로그온 트리거·`?schema=`)에서 이름 풀이의 기준(사용자 09-30).
        schema: String,
        /// ★ 접속 직후의 **현재 DB**(SQL Server `DB_NAME()` · MySQL `DATABASE()` · 그 밖 빈 글 · 10-01 ⑯) = 탭 작업 단위의 연결 기본값.
        database: String,
        /// ★ 이 러너의 자동 커밋 상태(사용자 10-29 "처음 접속 시 접속 정보에 autocommit 표시") — CLI 접속 줄 꼬리.
        autocommit: bool,
    },
    Disconnected,
    Error {
        index: usize,
        line: usize,
        error: DbError,
    },
    /// ★ 운영 보류(114 §5 · D-295 · 10-15): 변경이 **커밋되지 않고 열려 있다** — 호스트는 카드·배지·상단 Commit 확인으로 받는다.
    ///   `at_commit` = 스크립트의 사용자 `COMMIT` 직전에 멈췄다(D-299 · `remaining` = 그 뒤 남은 원문) · 아니면 실행 끝.
    Held {
        stmts: Vec<HeldStmt>,
        total_rows: u64,
        at_commit: bool,
        remaining: Option<String>,
    },
    /// 항목 하나의 단계별 소요(docs/26) — 결과 이벤트 뒤에 온다. CLI `--timing` · GUI 상태줄.
    Timing {
        index: usize,
        timeline: Timeline,
    },
    /// ★ 실행 **직전**의 바인드 매핑(이름·타입·값 · 리터럴) — 트랜잭션 로그의 3계층(원문 · 매핑 · 값 치환 쿼리 · docs/44 §9 · 09-28).
    /// 바인드가 없는 문장은 내지 않는다.
    Binds {
        index: usize,
        binds: Vec<txlog::TxBind>,
    },
}

/// 이벤트 → 로그 엔트리(GUI 로그 창 · CLI `--log` 공용 매핑 · docs/26 §3). 타임스탬프는 엔트리 생성 시각.
pub fn log_entries(ev: &RunEvent) -> Vec<nsql_log::LogEntry> {
    use nsql_log::{LogEntry, LogKind};
    match ev {
        RunEvent::Begin { line, summary, .. } => {
            vec![LogEntry::new(
                LogKind::Send,
                tf(Msg::LogLineSummary, &[&line.to_string(), summary]),
            )]
        }
        RunEvent::ResultSet { rs, elapsed, .. } => {
            vec![LogEntry::new(LogKind::Done, t(Msg::LogResultSet))
                .rows(rs.rows.len() as u64)
                .elapsed(*elapsed)]
        }
        RunEvent::Done {
            rows_affected,
            elapsed,
            delivery,
            ..
        } => {
            // 전달 모드 꼬리표(T-416) — 매개변수가 있을 때만 붙는다.
            let text = if delivery.is_empty() {
                t(Msg::LogDone).to_string()
            } else {
                format!("{} · {delivery}", t(Msg::LogDone))
            };
            vec![LogEntry::new(LogKind::Done, &text)
                .rows(*rows_affected)
                .elapsed(*elapsed)]
        }
        // 값까지(mac 09-21 — 종전에는 이름만 이어 붙여 GUI에서는 `PRINT`로 값을 볼 수 없었다) · 비밀 이름은 가림(D-140).
        RunEvent::Print { pairs } => vec![LogEntry::new(
            LogKind::Info,
            tf(Msg::LogPrint, &[&print_pairs_text(pairs)]),
        )],
        RunEvent::VarList { vars } => vec![LogEntry::new(
            LogKind::Info,
            tf(Msg::LogVarList, &[&var_list_text(vars)]),
        )],
        RunEvent::Message(m) => vec![LogEntry::new(LogKind::Info, m.clone())],
        RunEvent::ConnType(v) => vec![LogEntry::new(LogKind::Info, format!("conntype → {v}"))],
        RunEvent::ShowConn => Vec::new(),
        RunEvent::DbChanged(db) => vec![LogEntry::new(
            LogKind::Info,
            format!("current database → {db}"),
        )],
        RunEvent::Warning(m) => vec![LogEntry::new(LogKind::Info, m.clone())],
        RunEvent::Held {
            stmts,
            total_rows,
            at_commit,
            ..
        } => vec![LogEntry::new(
            LogKind::Info,
            tf(
                Msg::LogProdHeld,
                &[
                    &stmts.len().to_string(),
                    &total_rows.to_string(),
                    if *at_commit { "COMMIT" } else { "end" },
                ],
            ),
        )],
        RunEvent::DriverLimits { driver, limits, .. } => vec![LogEntry::new(
            LogKind::Info,
            format!(
                "{driver}: driver limitations: {}",
                limits
                    .iter()
                    .map(|l| l.code())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )],
        RunEvent::Connected {
            description,
            dialect,
            schema,
            ..
        } => vec![LogEntry::new(
            LogKind::Connect,
            if schema.is_empty() {
                format!("{description} ({dialect})")
            } else {
                format!("{description} ({dialect}) · schema {schema}")
            },
        )],
        RunEvent::Disconnected | RunEvent::ConnectionClosed => {
            vec![LogEntry::new(LogKind::Disconnect, "")]
        }
        // 읽기 트랜잭션 자동 종료는 호스트가 개발자 층(tx)으로만 남긴다(평소 로그를 어지럽히지 않는다).
        RunEvent::ReadTxEnded { .. }
        | RunEvent::Vars { .. }
        | RunEvent::InputNeeded { .. }
        | RunEvent::Binds { .. } => Vec::new(),
        RunEvent::Error { line, error, .. } => vec![LogEntry::new(
            LogKind::Error,
            tf(Msg::LogLineSummary, &[&line.to_string(), &error.message]),
        )],
        RunEvent::Timing { timeline, .. } => timeline
            .spans
            .iter()
            .map(|s| {
                let (kind, msg) = match s.stage {
                    Stage::Send => (LogKind::Send, t(Msg::LogSent)),
                    Stage::Execute => (LogKind::Execute, t(Msg::LogFirstResponse)),
                    Stage::Fetch => (LogKind::Fetch, t(Msg::LogFetched)),
                    Stage::OutputFlush => (LogKind::Output, t(Msg::LogServerOutput)),
                    Stage::Commit => (LogKind::Commit, t(Msg::LogCommit)),
                    other => (LogKind::Info, other.label()),
                };
                let mut m = msg.to_string();
                if let Some(n) = &s.note {
                    m.push_str(" · ");
                    m.push_str(n);
                }
                LogEntry::new(kind, m).rows(s.rows).elapsed(s.dur)
            })
            .collect(),
    }
}

/// 접속 열기 — 호스트가 드라이버 레지스트리로 주입한다.
pub type Opener = Box<dyn FnMut(&ConnectSpec) -> Result<Box<dyn Session>, DbError>>;
/// 치환 변수 프롬프트 — `None`이면 실행 중단.
pub type Prompter<'a> = &'a mut dyn FnMut(&str) -> Option<String>;
/// 프로필 이름 → 스펙(연결 프로필 저장소 · T-16b). `Ok(None)` = 이름이 아님(접속 문자열로 취급),
/// `Err` = 이름 꼴인데 프로필이 없거나 봉투를 열 수 없음.
pub type Resolver = Box<dyn FnMut(&str) -> Result<Option<ConnectSpec>, String>>;

/// `SPOOL` 파일 상태(T-9 · SQL*Plus 의미) — 호스트가 **찍는 모든 출력**을 [`Spool::write`]로 복사한다. Runner는 `SPOOL` 명령으로
/// 열고 닫기만 하고 무엇을 쓰는지는 호스트 Printer가 정한다(렌더링은 호스트 소관 · docs/30 포트). 장착하지 않은 호스트(GUI)는
/// [`Msg::SpoolNotSupported`] 메시지. 실시간 서버 메시지 싱크(다른 스레드)도 같은 핸들에 쓰므로 `Arc<Mutex>`.
#[derive(Debug, Default)]
pub struct Spool {
    path: Option<PathBuf>,
    file: Option<BufWriter<File>>,
    /// 쓰기 실패 — 스풀을 닫고 한 번만 보고한다([`Spool::take_error`]).
    error: Option<String>,
}

/// 호스트와 Runner가 공유하는 스풀 핸들.
pub type SpoolHandle = Arc<Mutex<Spool>>;

impl Spool {
    #[must_use]
    pub fn new_handle() -> SpoolHandle {
        Arc::new(Mutex::new(Spool::default()))
    }

    #[must_use]
    pub fn is_on(&self) -> bool {
        self.file.is_some()
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// 명령 적용 → 사용자에게 보일 메시지(없으면 `None` · 시작은 SQL*Plus처럼 조용하다). 실패 = 메시지.
    /// 이미 스풀 중에 새 `SPOOL file`이면 앞 파일을 닫고 새 파일로.
    pub fn apply(&mut self, cmd: &SpoolCmd) -> Result<Option<String>, String> {
        match cmd {
            SpoolCmd::Status => Ok(Some(match &self.path {
                Some(p) => tf(Msg::SpoolStatusOn, &[&p.display().to_string()]),
                None => t(Msg::SpoolStatusOff).to_string(),
            })),
            SpoolCmd::Off => Ok(Some(match self.close() {
                Some(p) => tf(Msg::SpoolStopped, &[&p.display().to_string()]),
                None => t(Msg::SpoolStatusOff).to_string(),
            })),
            SpoolCmd::Start { path, mode } => {
                let mut o = std::fs::OpenOptions::new();
                match mode {
                    SpoolMode::Replace => o.write(true).create(true).truncate(true),
                    SpoolMode::Append => o.append(true).create(true),
                    SpoolMode::Create => o.write(true).create_new(true),
                };
                match o.open(path) {
                    Ok(f) => {
                        self.close();
                        self.path = Some(PathBuf::from(path));
                        self.file = Some(BufWriter::new(f));
                        self.error = None;
                        Ok(None)
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                        Err(tf(Msg::SpoolExists, &[path]))
                    }
                    Err(e) => Err(tf(Msg::SpoolOpenFailed, &[path, &e.to_string()])),
                }
            }
        }
    }

    /// 출력 복사 — 켜져 있을 때만. 쓰기 실패면 닫고 오류를 보관한다.
    pub fn write(&mut self, bytes: &[u8]) {
        if let Some(f) = self.file.as_mut() {
            if let Err(e) = f.write_all(bytes) {
                self.error = Some(e.to_string());
                self.close();
            }
        }
    }

    /// 마지막 쓰기 오류(한 번만).
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }

    /// 닫는다(플러시) — 닫힌 파일 경로.
    pub fn close(&mut self) -> Option<PathBuf> {
        if let Some(mut f) = self.file.take() {
            let _ = f.flush();
        }
        self.path.take()
    }
}

/// 접속 테스트 결과(`nsql conn test` · GUI "Test Connection" 공용).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestReport {
    /// 비밀번호 가린 접속 설명.
    pub description: String,
    /// 세션이 보고한 방언.
    pub dialect: Dialect,
    /// 세션 자기 설명(`Session::describe` — 서버·버전 등 어댑터가 아는 만큼).
    pub session: String,
    /// 열기까지 걸린 시간.
    pub elapsed: Duration,
}

/// 접속만 해 보고 끊는다 — 세션은 반환하지 않는다(호출측이 열린 세션을 원하면 [`Runner::connect`]).
/// GUI 워커·CLI가 **같은 함수**를 부른다(핵심 기능 모듈화 · 사용자 09-14).
pub fn test_connection(spec: &ConnectSpec, opener: &mut Opener) -> Result<TestReport, DbError> {
    let started = Instant::now();
    let session = opener(spec)?;
    Ok(TestReport {
        description: spec.redacted(),
        dialect: session.dialect(),
        session: session.describe(),
        elapsed: started.elapsed(),
    })
}

#[allow(missing_debug_implementations)]
pub struct Runner {
    /// 마지막으로 만든 저장 단위(종류 · 소유자 · 이름) — `SHOW ERRORS`가 다시 읽는다(SQL*Plus 관용 · 09-20).
    last_unit: Option<(String, String, String)>,
    pub engine: Engine,
    pub session: Option<Box<dyn Session>>,
    opener: Opener,
    /// `CONNECT <프로필>`(사용자명만 있고 나머지가 빈 스펙)을 저장소에서 푼다. 없으면 그대로 접속.
    pub resolver: Option<Resolver>,
    /// 마지막 접속 설명(상태줄).
    pub connection: Option<String>,
    /// 지금 붙어 있는 대상(**비밀번호 없음** · 21 §7) — 같은 서버·계정에 다시 `CONNECT`하는지 보고, 호스트 워커의 자동 재접속
    /// 원천([`Self::connected_to`] · 비밀번호는 세션 자격 금고에서 인출 · 사용자 10-09)이 된다.
    connected_to: Option<ConnectSpec>,
    /// ★ 결과 셋 페치 상한(0 = 무제한 · DBeaver "ResultSet fetch size" 차용 · 사용자 09-15). 드라이버에는
    /// 세션 옵션 `max_rows`로 `상한+1`을 알려 조기 중단(Oracle·SQLite)하고, 호스트는 상한 초과분을 잘라 `more`를 표시한다.
    pub max_rows: usize,
    /// 서버 메시지 실시간 싱크(접속마다 세션에 심는다 · GUI = 이벤트 채널 · CLI = stdout).
    pub message_sink: Option<nsql_core::MessageSink>,
    /// `SPOOL` 포트(T-9) — 호스트가 장착하면 파일을 열고 닫는다. 없으면 "미지원" 메시지.
    pub spool: Option<SpoolHandle>,
    /// 엄격 모드(설정 `script.strict` · T-9): 미정의 `&var`는 프롬프트 대신 오류 · 선언 없는 `:bind`는 암묵 선언 대신 오류.
    pub strict: bool,
    /// 실행 중인 스크립트 폴더 스택 — `@@`·`:r`의 기준(맨 위 = 현재 스크립트).
    script_dirs: Vec<PathBuf>,
    /// ★ 서버 커서(T-48a · docs/43 D-70) — 마지막 실행이 상한에서 잘렸고 드라이버가 커서를 열어 둔 경우. 세션당 1개.
    /// 스크립트 문장 실행([`Runner::execute`]) · 접속/해제 · 호스트의 [`Runner::close_cursor`]가 닫는다.
    cursor: Option<OpenCursor>,
    /// 커서 유지 여부(설정 `grid.fetch_mode` = cursor). `false`면 실행 뒤 열린 커서를 곧바로 닫는다(OFFSET 재질의 폴백 · 종전 동작).
    pub keep_cursor: bool,
    /// 왕복당 행수(설정 `db.fetch_size` · Oracle ARRAYSIZE 격 · 0 = 드라이버 기본) — 접속 시 세션 옵션 `fetch_size`로.
    pub fetch_size: usize,
    /// 유휴 커서 상한(설정 `db.cursor_idle_secs` · 0 = 없음) — 마지막 페치 뒤 이 시간이 지나면 닫고 OFFSET 폴백.
    pub cursor_idle_secs: u64,
    /// ★ T-116 겹침 검사(설정 `grid.overlap_check` · 기본 켬 · docs/43 §9-3): OFFSET 재질의로 다음 페이지를 받을 때 `offset-1`행을 함께
    /// 받아 **직전 페이지의 마지막 행**과 비교한다 — 다르면 서버 결과가 바뀐 것(삽입·삭제·정렬 동점) → 오류로 알리고 이어 붙이지 않는다.
    pub overlap_check: bool,
    /// 직전 페이지의 꼬리(문장 · 넘긴 행 수 = 다음 OFFSET · 마지막 행) — 겹침 검사의 기준. 실행·페치마다 갱신.
    page_tail: Option<(String, usize, Vec<Value>)>,
    /// 읽기 트랜잭션 자동 종료(설정 `tx.read_end` · docs/56 L1).
    pub read_end: ReadEnd,
    /// 열린 트랜잭션에 변경이 있었는가(수동 모드 · 커밋/롤백/암묵 커밋/접속에서 초기화).
    /// ★ 운영 보류 커밋(114 §5 · 10-15): 이번 실행의 정책(`None` = 보통) + 누적(보류 문장 · 합계 행 · 블록 · WHERE 없음) · COMMIT 직전 멈춤 번호.
    hold: Option<HoldPolicy>,
    hold_stmts: Vec<HeldStmt>,
    hold_total: u64,
    hold_block: bool,
    /// ★ 열린 트랜잭션 안에서 실행된 **변경 문장 수**(수동 모드 · `nsql shell` 프롬프트 `*n` · T-77 잔여 10-30) — 트랜잭션이 끝나면 0.
    tx_stmts: usize,
    hold_no_where: bool,
    hold_stop: Option<usize>,
    tx_changed: bool,
    /// 사용자가 직접 트랜잭션을 열었거나 잠금을 잡았는가(자동 종료 제외 근거).
    tx_user: bool,
    /// 루틴 서명 캐시(호출 이름 대문자 → 인자 목록) — 선언 없는 바인드의 타입 추론용 · DDL 실행·접속에서 비운다.
    sig_cache: std::collections::HashMap<String, Vec<nsql_catalog::RoutineArg>>,
    /// 실행 뒤 돌아온 REF CURSOR를 바로 결과 집합으로 보여 주는가(설정 `run.cursor_autoshow` · 기본 켬).
    pub auto_cursor: bool,
    /// 호출 서명 조회(T-151 · 설정 `vars.signature_lookup` · 기본 켬) — 끄면 카탈로그 조회 0(= 종전: 타입 추론·`OUTPUT` 보충·
    /// PG OUT 받기 없음). 부하원 원장 39 §3.
    pub signature_lookup: bool,
    /// PostgreSQL 커서 이름 풀기(T-150 · 설정 `pg.refcursor_expand` · 기본 켬) — 드라이버 옵션으로 내려간다.
    pub refcursor_expand: bool,
    /// `run_script_in` 중첩 깊이(`@스크립트`) — 변수 사건은 가장 바깥에서만.
    run_depth: u32,
    /// 러너가 수동 모드를 위해 연 트랜잭션이 살아 있는가([`manual_begin_sql`] · 커밋/롤백/접속에서 내림).
    tx_open: bool,
}

/// 읽기 트랜잭션 자동 종료(docs/56 L1 · 설정 `tx.read_end`): 수동 커밋 모드에서 **변경이 없던 트랜잭션**을 결과를 다 받은 뒤
/// 조용히 끝낸다 — PG `idle in transaction` · MySQL 메타데이터 잠금 · SQLite 읽기 잠금이 남지 않게.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ReadEnd {
    /// 끝내지 않는다(종전 동작 — 반복 읽기를 습관적으로 쓰는 환경).
    Off,
    /// 문장 분류로 판정(기본).
    #[default]
    Auto,
    /// 분류 판정 뒤 **서버에 한 번 더 묻는다**(함수 호출 SELECT의 부작용까지 · 지원 방언만 · 실패 = 끝내지 않음).
    Strict,
}

impl ReadEnd {
    /// 설정 문자열(`auto` · `strict` · `off`).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "off" => Self::Off,
            "strict" => Self::Strict,
            _ => Self::Auto,
        }
    }
}

/// 트랜잭션을 끝내는 방법(미뤄 둔 종료 포함).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TxEnd {
    Commit,
    Rollback,
}

/// 미뤄 둔 종료를 실행한다. 돌려주는 값 = 트랜잭션을 끝냈는가(러너가 연 트랜잭션 표시를 내린다).
fn finish_tx(s: &mut dyn Session, end: Option<TxEnd>) -> bool {
    match end {
        Some(TxEnd::Commit) => {
            let _ = s.commit();
            true
        }
        Some(TxEnd::Rollback) => {
            let _ = s.rollback();
            true
        }
        None => false,
    }
}

/// 입력 창에 친 글 → 바인드 값: 빈 글·`NULL` = NULL · 수 = 수 · `'…'` = 그 안의 글 · 그 밖 = 글 그대로.
#[must_use]
pub fn input_value(text: &str) -> Value {
    let t = text.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("NULL") {
        return Value::Null;
    }
    match nsql_script::command::parse_literal(t) {
        Ok(v) => v,
        Err(_) => Value::Str(text.to_string()),
    }
}

/// 프로필 변수 `(이름, 글)` → 프로필 층 상태(값 = 리터럴 추정 · 비밀 이름 규칙 적용 · 수식 없음).
#[must_use]
pub fn profile_var_states(vars: Vec<(String, String)>) -> Vec<nsql_script::VarState> {
    vars.into_iter()
        .map(|(name, text)| {
            let value = input_value(&text);
            nsql_script::VarState {
                ty: nsql_core::VarType::infer(&value),
                value,
                declared: false,
                secret: nsql_script::looks_secret(&name),
                layer: nsql_script::Layer::Fixed,
                formula: None,
                stale: false,
                name,
            }
        })
        .collect()
}

/// 변수 타입의 표시 글자(`SHOW VARIABLES` · 패널).
fn var_type_label(ty: &nsql_core::VarType) -> String {
    use nsql_core::VarType as T;
    match ty {
        T::Number => "NUMBER".into(),
        T::Varchar2(n) => format!("VARCHAR2({n})"),
        T::Char(n) => format!("CHAR({n})"),
        T::Clob => "CLOB".into(),
        T::RefCursor => "REFCURSOR".into(),
        T::BinaryFloat => "BINARY_FLOAT".into(),
        T::BinaryDouble => "BINARY_DOUBLE".into(),
        T::Date => "DATE".into(),
        T::Timestamp => "TIMESTAMP".into(),
        T::Boolean => "BOOLEAN".into(),
        T::Blob => "BLOB".into(),
        T::Auto => "auto".into(),
    }
}

/// 서버 타입 글자 → 변수 타입(서명 추론용). 문자열류는 `None` = 그대로 `Auto`(종전 동작을 바꾸지 않는다).
fn var_type_of(data_type: &str) -> Option<nsql_core::VarType> {
    use nsql_core::VarType;
    match data_type {
        "REF CURSOR" => Some(VarType::RefCursor),
        "NUMBER" | "INTEGER" | "FLOAT" | "BINARY_INTEGER" | "PLS_INTEGER" => Some(VarType::Number),
        "CLOB" => Some(VarType::Clob),
        // T-151: 날짜·불리언은 글자로 바인드하면 시각이 잘리거나(NLS 형식) PLS-00306이다 → 진짜 타입으로.
        "DATE" => Some(VarType::Date),
        "PL/SQL BOOLEAN" | "BOOLEAN" | "BIT" => Some(VarType::Boolean),
        // SQL Server(`TYPE_NAME` 대문자).
        "INT" | "BIGINT" | "SMALLINT" | "TINYINT" | "DECIMAL" | "NUMERIC" | "MONEY"
        | "SMALLMONEY" | "REAL" => Some(VarType::Number),
        "DATETIME" | "DATETIME2" | "SMALLDATETIME" => Some(VarType::Timestamp),
        t if t.starts_with("TIMESTAMP") => Some(VarType::Timestamp),
        _ => None,
    }
}

/// ★ 수동 커밋 모드에서 문장 앞에 열어야 하는 트랜잭션 문장(T-146 · 09-21). Oracle은 첫 DML이 곧 트랜잭션이지만
/// **PostgreSQL·SQLite·SQL Server·MySQL은 서버가 문장마다 자동 커밋**한다 — 종전에는 수동 모드에서도 아무도 트랜잭션을
/// 열지 않아 `INSERT`가 "미커밋"으로 보이면서 실제로는 이미 커밋됐다(CI `pg_read_only_transaction_ends_in_manual_mode`).
///
/// 조건(전부 참일 때만 연다): 수동 모드 · 암묵 트랜잭션이 없는 방언 · 러너가 연 트랜잭션이 없다 · 사용자가 직접 연
/// 트랜잭션이 없다 · 이 문장이 트랜잭션 통제 문장(`BEGIN`/`COMMIT`/…)이 아니다.
fn manual_begin_sql(
    caps: &nsql_core::Caps,
    autocommit: bool,
    tx_open: bool,
    tx_user: bool,
    control: nsql_core::TxControl,
) -> Option<&'static str> {
    if autocommit || tx_open || tx_user || control != nsql_core::TxControl::None {
        return None;
    }
    // 시작문은 능력표가 안다(T-152) — Oracle = 암묵 트랜잭션 · ODBC = 뒤의 DBMS를 모른다(드라이버의 자동 커밋 속성 몫) = 없음.
    caps.tx_begin
}

/// 한 문장이 열린 트랜잭션에 남기는 흔적(docs/56 L1 판정의 입력).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TxEffect {
    /// 조회·빈 문장·영향 0행 DML — 변경 없음.
    Clean,
    /// 변경(DML 영향 행 · 트랜잭션 DDL · 프로시저 호출 등 알 수 없는 문장).
    Change,
    /// 사용자가 직접 트랜잭션을 열었거나 잠금을 잡았다(`BEGIN` · `SET TRANSACTION` · `SAVEPOINT` · `LOCK` · `FOR UPDATE`).
    UserHold,
    /// 트랜잭션이 끝났다(`COMMIT`/`ROLLBACK` 문장 · 암묵 커밋 DDL).
    Ended,
}

fn tx_effect(sql: &str, rows_affected: Option<u64>, dialect: Dialect) -> TxEffect {
    use nsql_core::{TxClass, TxControl};
    match TxControl::of_sql(sql) {
        TxControl::Begin => return TxEffect::UserHold,
        TxControl::End => return TxEffect::Ended,
        TxControl::None => {}
    }
    let class = TxClass::of_sql(sql);
    match class {
        TxClass::None => TxEffect::Clean,
        TxClass::Read => {
            if nsql_core::is_locking_read(sql) {
                TxEffect::UserHold
            } else {
                TxEffect::Clean
            }
        }
        TxClass::Insert | TxClass::Update | TxClass::Delete => {
            if rows_affected == Some(0) {
                TxEffect::Clean
            } else {
                TxEffect::Change
            }
        }
        _ if class.is_ddl() && !dialect.ddl_transactional() => TxEffect::Ended,
        _ => TxEffect::Change,
    }
}

/// 러너가 아는 열린 커서의 상태(핸들 · 원문 · 지금까지 넘긴 행 수).
#[derive(Debug)]
struct OpenCursor {
    handle: CursorHandle,
    /// 잘린 결과를 만든 문장 원문(호스트의 "더 가져오기"가 같은 문장인지 확인).
    sql: String,
    /// 호스트에 넘긴 행 수(= 다음 페치의 OFFSET) — `fetch_page`가 커서/OFFSET 중 어느 쪽을 쓸지 판정.
    served: usize,
    /// 트랜잭션 종료를 커서가 닫힐 때까지 미뤘는가(PG WITHOUT HOLD 커서 보호) — 자동 커밋 = Commit · 수동 모드의
    /// 읽기 트랜잭션 자동 종료(docs/56 L1) = Rollback.
    deferred_end: Option<TxEnd>,
    last_used: Instant,
}

impl Runner {
    pub fn new(dialect: Dialect, opener: Opener) -> Self {
        Runner {
            engine: Engine::new(dialect),
            session: None,
            opener,
            resolver: None,
            connection: None,
            connected_to: None,
            max_rows: 0,
            message_sink: None,
            spool: None,
            strict: false,
            script_dirs: Vec::new(),
            cursor: None,
            keep_cursor: true,
            fetch_size: 0,
            cursor_idle_secs: 0,
            overlap_check: true,
            page_tail: None,
            last_unit: None,
            read_end: ReadEnd::Auto,
            hold: None,
            hold_stmts: Vec::new(),
            hold_total: 0,
            hold_block: false,
            tx_stmts: 0,
            hold_no_where: false,
            hold_stop: None,
            tx_changed: false,
            tx_user: false,
            auto_cursor: true,
            signature_lookup: true,
            refcursor_expand: true,
            sig_cache: std::collections::HashMap::new(),
            run_depth: 0,
            tx_open: false,
        }
    }

    /// 왕복당 행수(체이닝 · 설정 `db.fetch_size`).
    #[must_use]
    pub fn with_fetch_size(mut self, n: usize) -> Self {
        self.fetch_size = n;
        self
    }

    /// 커서 유지 여부(체이닝 · 설정 `grid.fetch_mode`).
    #[must_use]
    pub fn with_keep_cursor(mut self, on: bool) -> Self {
        self.keep_cursor = on;
        self
    }

    /// `SPOOL` 포트 장착(체이닝) — CLI Printer가 같은 핸들에 출력을 복사한다.
    #[must_use]
    pub fn with_spool(mut self, spool: SpoolHandle) -> Self {
        self.spool = Some(spool);
        self
    }

    /// 엄격 모드(체이닝 · 설정 `script.strict`).
    #[must_use]
    pub fn with_strict(mut self, on: bool) -> Self {
        self.strict = on;
        // 엄격 모드 = 미정의 `&var`는 오류(T-9) → 너그러운 방언 기본(D-272)보다 우선: 전 방언 SQL*Plus식으로 고정.
        if on {
            self.engine.set_define_mode("on");
        }
        self
    }

    /// 치환 접두 모드(체이닝 · 설정 `script.define` = auto|on|off · D-272).
    #[must_use]
    pub fn with_define_mode(mut self, mode: &str) -> Self {
        self.engine.set_define_mode(mode);
        self
    }

    /// 문자열 리터럴 안 `&이름` 치환 여부(설정 `script.define_in_strings` · 기본 켬 · T-149 잔여).
    #[must_use]
    pub fn with_define_in_strings(mut self, on: bool) -> Self {
        self.engine.settings.define_in_strings = on;
        self
    }

    /// 실시간 메시지 싱크 장착(체이닝).
    #[must_use]
    pub fn with_message_sink(mut self, sink: nsql_core::MessageSink) -> Self {
        self.message_sink = Some(sink);
        self
    }

    /// `SELECT … INTO` 여러 행 = 첫 행(설정 `vars.into_policy = first`).
    #[must_use]
    /// 설정 `vars.brace_subst` · `vars.max_value_kb`(T-153).
    pub fn with_var_limits(mut self, brace_subst: bool, max_value_kb: usize) -> Self {
        self.engine.settings.brace_subst = brace_subst;
        self.engine.settings.max_value_bytes = max_value_kb.saturating_mul(1024);
        self
    }

    pub fn with_into_first(mut self, on: bool) -> Self {
        self.engine.settings.into_first = on;
        self
    }

    /// REF CURSOR 자동 표시(설정 `run.cursor_autoshow`).
    #[must_use]
    pub fn with_auto_cursor(mut self, on: bool) -> Self {
        self.auto_cursor = on;
        self
    }

    /// 기본 자동 커밋(설정 `session.autocommit` · 스크립트 `SET AUTOCOMMIT`이 우선).
    #[must_use]
    pub fn with_autocommit(mut self, on: bool) -> Self {
        self.engine.settings.autocommit = on;
        self
    }

    /// ★ 자동 커밋 전환(호스트 설정 토글 · **모든 세션의 러너에** · nexa-sql 09-28): 종전에는 `SET AUTOCOMMIT` 스크립트가 토글
    ///   시점의 활성 세션 한 곳에만 갔고, 다른 연결·탭 전용 세션의 러너는 수동으로 남아 그리드 적용이 "트랜잭션 열림"을
    ///   보고했다(자동 커밋인데 커밋/롤백 버튼 활성). 열려 있던 트랜잭션은 그대로 — 자동 모드의 다음 문장 뒤 규칙
    ///   (`autocommit_needs_commit`)이 끝낸다.
    pub fn set_autocommit(&mut self, on: bool) {
        self.engine.settings.autocommit = on;
    }

    /// 페치 상한 장착(체이닝 · 0 = 무제한) — 세션이 이미 있으면 바로 알린다.
    #[must_use]
    pub fn with_max_rows(mut self, n: usize) -> Self {
        self.max_rows = n;
        self.push_max_rows();
        self
    }

    /// 왕복당 행수 변경(전체 조회는 큰 배열 · docs/43) — 세션에 즉시 알린다(0 = 드라이버 기본).
    pub fn set_fetch_size(&mut self, n: usize) {
        self.fetch_size = n;
        self.push_max_rows();
    }

    /// 페치 상한 변경(실행마다 · 결과 탭의 세그먼트 크기 · docs/43) — 세션에도 즉시 알린다.
    /// 현재 세션의 실행 취소 핸들(T-108) — 호스트 워커가 실행 직전에 꺼내 UI와 공유한다.
    pub fn cancel_handle(&self) -> Option<std::sync::Arc<dyn nsql_core::CancelHandle>> {
        self.session.as_ref().and_then(|s| s.cancel_handle())
    }

    pub fn set_max_rows(&mut self, n: usize) {
        if self.max_rows != n {
            self.max_rows = n;
            self.push_max_rows();
        }
    }

    /// 접속된 세션의 방언(없으면 None).
    #[must_use]
    pub fn dialect(&self) -> Option<Dialect> {
        self.session.as_ref().map(|s| s.dialect())
    }

    /// 단문 1회 — 이벤트 없이 결과만(OFFSET 재질의 · 전체 조회 · COUNT · docs/43 §3). `max_rows`(0 = 무제한)는 이 호출에만.
    /// **곁가지 실행**: 드라이버 상한 0으로 실행해 서버 커서를 열지 않고(열린 커서를 건드리지 않음) 초과분은 여기서 자른다.
    /// 세션이 없으면 `Err`(호출자가 먼저 확인한다). 반환 = (첫 결과 집합 · 더 있음 · 소요).
    pub fn query_once(
        &mut self,
        sql: &str,
        max_rows: usize,
    ) -> Result<(ResultSet, bool, Duration), DbError> {
        self.query_req_once(
            &ExecRequest {
                sql: sql.to_string(),
                params: Vec::new(),
            },
            max_rows,
        )
    }

    /// ★ 실행 계획 검토(docs/122 · 사용자 10-23): 방언별 EXPLAIN 스크립트를 돌려 결과 집합을 [`nsql_script::plan::PlanReport`]로.
    /// `analyze` = 문장을 실제로 실행한다(DML이면 바뀐다 · 호스트가 경고). 결과 집합 선택 = SQL Server는 `StmtText` 열이 있는 것 ·
    /// 그 밖은 마지막 것(Oracle 실제 계획은 문장 결과 뒤에 DISPLAY_CURSOR가 온다). 첫 오류가 있으면 그 오류.
    pub fn explain(
        &mut self,
        stmt: &str,
        opts: nsql_script::plan::PlanOptions,
    ) -> Result<nsql_script::plan::PlanReport, DbError> {
        let dialect = self.engine.dialect;
        let src = nsql_script::plan::explain_script_opts(dialect, stmt, opts);
        let mut sets: Vec<ResultSet> = Vec::new();
        let mut first_err: Option<DbError> = None;
        let mut prompt = |_: &str| Some(String::new());
        let prev = self.max_rows;
        self.set_max_rows(0);
        // ★ 읽기 트랜잭션 자동 종료(docs/56 L1)는 **스크립트가 끝난 뒤 한 번**(CI db2.yml 40219925165 · 10-26): 문장마다 돌면
        //   Db2 `EXPLAIN`(EXPLAIN 표에 쓰지만 분류 = 읽기)이 남긴 행을 바로 ROLLBACK이 지워 트리 질의가 0행이 된다(Oracle
        //   PLAN_TABLE도 같은 꼴). 끝에서 되돌리면 EXPLAIN 표 정리까지 겸한다(수동 모드 · 러너가 이 스크립트로 연 깨끗한 트랜잭션만).
        let read_end = std::mem::replace(&mut self.read_end, ReadEnd::Off);
        let was_open = self.tx_open || self.tx_changed || self.tx_user;
        let _ = self.run_script(&src, &mut prompt, &mut |e| match e {
            RunEvent::ResultSet { rs, .. } => sets.push(rs),
            RunEvent::Error { error, .. } if first_err.is_none() => first_err = Some(error),
            _ => {}
        });
        self.read_end = read_end;
        self.set_max_rows(prev);
        if !was_open
            && read_end != ReadEnd::Off
            && self.tx_open
            && !self.tx_changed
            && !self.tx_user
        {
            if let Some(s) = self.session.as_mut() {
                let _ = s.rollback();
            }
            self.note_tx_ended();
        }
        if let Some(e) = first_err {
            return Err(e);
        }
        // T-149 ④(10-30): 계획 결과 집합을 고르는 열 이름은 방언 표(`Syntax::plan_text_column` · SQL Server `StmtText`).
        let pick = match nsql_core::Caps::of(dialect).syntax.plan_text_column {
            Some(col) => sets
                .iter()
                .find(|rs| rs.columns.iter().any(|c| c.name.eq_ignore_ascii_case(col)))
                .or(sets.last()),
            None => sets.last(),
        };
        let (columns, rows): (Vec<String>, Vec<Vec<String>>) = match pick {
            Some(rs) => (
                rs.columns.iter().map(|c| c.name.clone()).collect(),
                rs.rows
                    .iter()
                    .map(|r| r.iter().map(Value::display).collect())
                    .collect(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        // T-418: 설명한 문장의 바인드 수(고유 이름) → 탭 머리·검토의 "바인드 기준/리터럴 기준".
        let binds = nsql_script::bind::unique_names(&nsql_script::bind::extract_binds(stmt)).len();
        Ok(nsql_script::plan::build_report(
            dialect,
            opts.analyze,
            binds,
            &columns,
            &rows,
        ))
    }

    /// [`Self::query_once`]의 바인드 문장판(그리드 편집 적용 뒤 행 단위 재조회 · 87 §12-4).
    pub fn query_req_once(
        &mut self,
        req: &ExecRequest,
        max_rows: usize,
    ) -> Result<(ResultSet, bool, Duration), DbError> {
        let prev = self.max_rows;
        self.set_max_rows(0);
        let r = match self.session.as_mut() {
            Some(s) => {
                let t = Instant::now();
                s.execute(req).map(|res| {
                    if let Some(h) = res.pending {
                        let _ = s.close_cursor(h);
                    }
                    let rs = res.result_sets.into_iter().next().unwrap_or_default();
                    let (rs, more) = trim_rows(rs, max_rows);
                    (rs, more, t.elapsed())
                })
            }
            None => Err(DbError {
                code: None,
                message: String::new(),
                position: None,
            }),
        };
        self.set_max_rows(prev);
        r
    }

    /// 활성 세션의 드라이버가 서버 커서를 지원하는가(Oracle·SQLite·PG = true · MSSQL = false).
    #[must_use]
    pub fn cursor_supported(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.cursor_supported())
    }

    /// 문장을 **커서를 열어 둔 채** 실행 — 첫 `first`행과 커서 핸들(더 없으면 None)을 돌려준다(T-48b 전체 조회 스트리밍).
    /// 열려 있던 커서는 먼저 닫는다. 이어지는 행은 [`Runner::fetch_all`]/[`Runner::fetch_next`]로.
    pub fn query_stream(
        &mut self,
        sql: &str,
        first: usize,
    ) -> Result<(ResultSet, Option<CursorHandle>, Duration), DbError> {
        self.close_cursor();
        let prev = self.max_rows;
        self.set_max_rows(first.max(1));
        let r = match self.session.as_mut() {
            Some(s) => {
                let t = Instant::now();
                s.execute(&ExecRequest {
                    sql: sql.to_string(),
                    params: Vec::new(),
                })
                .map(|res| {
                    let rs = res.result_sets.into_iter().next().unwrap_or_default();
                    (rs, res.pending, t.elapsed())
                })
            }
            None => Err(msg_err(t(Msg::NoSession))),
        };
        self.set_max_rows(prev);
        let (rs, pending, d) = r?;
        if let Some(h) = pending {
            self.cursor = Some(OpenCursor {
                handle: h,
                sql: sql.to_string(),
                served: rs.rows.len(),
                deferred_end: None,
                last_used: Instant::now(),
            });
        }
        Ok((rs, pending, d))
    }

    // ────────────────────────────────────────────── 추가 페치(T-48a · docs/43 §3-1)

    /// 열린 커서의 (핸들, 지금까지 넘긴 행 수).
    #[must_use]
    pub fn cursor(&self) -> Option<(CursorHandle, usize)> {
        self.cursor.as_ref().map(|c| (c.handle, c.served))
    }

    /// 호스트의 "더 가져오기"가 열린 커서로 이어질 수 있는가 — 같은 문장이고 `offset`이 커서 위치와 같을 때.
    /// 유휴 상한(`cursor_idle_secs`)을 넘겼으면 닫고 `false`(호스트는 OFFSET 폴백).
    pub fn cursor_matches(&mut self, sql: &str, offset: usize) -> bool {
        let Some(c) = self.cursor.as_ref() else {
            return false;
        };
        if self.cursor_idle_secs > 0 && c.last_used.elapsed().as_secs() >= self.cursor_idle_secs {
            self.close_cursor();
            return false;
        }
        c.served == offset && same_sql(&c.sql, sql)
    }

    /// 트랜잭션이 호스트 명령으로 끝났다(Commit/Rollback 버튼 · 접속/해제) — L1 판정 상태를 비운다.
    /// 이 스크립트를 실행하기 전에 물어야 할 입력(D-137) — 표에도 없고 앞에서 대입되지도 않은 채 **읽히는** 바인드와
    /// 정의되지 않은 치환 변수. 빈 목록 = 바로 실행해도 된다.
    #[must_use]
    pub fn missing_inputs(&self, src: &str) -> Vec<nsql_script::InputNeed> {
        // D-272: 너그러운 방언(Oracle 밖 기본)은 미정의 `&x`를 묻지 않는다 → 훑기에 치환 글자를 넘기지 않는다(정의된 것은 어차피 빠진 입력이 아니다).
        let define_char = if self.engine.settings.define_lenient {
            None
        } else {
            self.engine.settings.define_char
        };
        // ★ 10-30: 문자열 안 `&이름`은 설정 `script.define_in_strings`를 따른다(엔진 substitute와 같은 규칙 · 끄면 'R&D'를 묻지 않는다).
        nsql_script::missing_inputs_with(
            src,
            self.dialect(),
            &self.engine.vars,
            &self.engine.defines,
            define_char,
            self.engine.settings.define_in_strings,
        )
    }

    /// 호스트가 받은 입력을 넣는다 — 바인드 = 타입 있는 값(`NULL` · 수 · `'글'` · 그 밖 = 글 그대로 · 빈 글 = NULL) ·
    /// 치환 변수 = 글자 그대로.
    /// ★ 프로필 층 변수(T-153 · docs/63 §11 · 읽기 전용 `Layer::Fixed`): 접속 프로필의 `var.<이름>=값`을 통째로(값 = 리터럴 추정 · 타입 Auto).
    pub fn set_profile_vars(&mut self, vars: Vec<(String, String)>) {
        self.engine.vars.set_fixed(profile_var_states(vars));
    }

    /// 입력 창의 답 — `(종류 · 이름 · 친 글 · 고른 타입)`. 타입이 `Auto`가 아니면 그 타입으로 **선언 + 대입**(T-153 · 사용자 10-26 타입 콤보 ·
    /// 선언된 변수는 타입 고정 → 뒤 대입에서 값을 따라 바뀌지 않는다) · `Auto`/없음 = 종전(리터럴 추정 대입).
    pub fn apply_inputs(
        &mut self,
        values: Vec<(
            nsql_script::InputKind,
            String,
            String,
            Option<nsql_core::VarType>,
        )>,
    ) {
        // 새 실행의 답 — 앞 실행에서 쓰이지 않고 남은 `ACCEPT` 표시는 버린다.
        self.engine.accepted.clear();
        for (kind, name, text, ty) in values {
            match kind {
                nsql_script::InputKind::Macro => {
                    self.engine.define(&name, &text);
                    self.engine.accepted.insert(name.to_ascii_uppercase());
                }
                nsql_script::InputKind::Bind => match ty.filter(|t| *t != nsql_core::VarType::Auto)
                {
                    Some(ty) => {
                        let v = nsql_script::typed_input(&text, &ty);
                        self.engine.vars.declare(&name, ty, Some(v));
                    }
                    None => {
                        let v = input_value(&text);
                        self.engine.vars.assign(&name, v);
                    }
                },
            }
        }
    }

    /// ★ 선언 없이 쓴 바인드의 타입을 **루틴 서명에서** 정한다(`EXEC proc(:PC_A, :PC_B)` — Golden 관용 · 변수 관리 09-21).
    /// REF CURSOR OUT 인자를 문자열로 바인드하면 PLS-00306이므로, 타입이 아직 없는 바인드가 있을 때만 `ALL_ARGUMENTS`를
    /// 한 번 읽는다(루틴당 1회 · 캐시 · 실패는 조용히 = 종전 동작). 방언별 방식 = `Caps::call_signature`(Oracle `BindTypes` ·
    /// PG·MySQL `Captures` · SQL Server `OutputMarks` · SQLite 등 `None` = 아무것도 안 함 · T-149 ③ 10-30 주석 정정).
    fn infer_call_bind_types(&mut self, item: &Item) {
        self.engine.call_captures = None;
        self.engine.call_outputs.clear();
        let use_sig = self.engine.caps.call_signature;
        if use_sig == nsql_core::CallSignature::None || !self.signature_lookup {
            return;
        }
        let ItemKind::Command(nsql_script::Command::Exec { body }) = &item.kind else {
            return;
        };
        // T-SQL은 괄호 없는 호출(`EXEC p 1, :X`)이다 — 모양 읽기만 다르고 뒤는 같다.
        let marks = use_sig == nsql_core::CallSignature::OutputMarks;
        let shape = if marks {
            nsql_script::tsql_call_shape(body)
        } else {
            nsql_script::call_shape(body)
        };
        let Some(shape) = shape else {
            return;
        };
        if use_sig == nsql_core::CallSignature::Captures {
            self.infer_call_captures(&shape);
            return;
        }
        // (자리(1부터 · 0 = 반환값), 이름 표기, 바인드) 가운데 타입이 필요한 것.
        let mut need: Vec<(i64, Option<&str>, &str)> = Vec::new();
        if let Some(r) = shape.ret.as_deref() {
            if self.engine.vars.needs_type(r) {
                need.push((0, None, r));
            }
        }
        for (i, a) in shape.args.iter().enumerate() {
            if let Some(b) = a.bind.as_deref() {
                if self.engine.vars.needs_type(b) {
                    need.push((i as i64 + 1, a.named.as_deref(), b));
                }
            }
        }
        // SQL Server: `OUTPUT`이 빠진 바인드 인자가 있으면 타입이 필요 없어도 서명을 본다(빠뜨리면 값이 조용히 버려진다).
        let unmarked = marks && shape.args.iter().any(|a| a.bind.is_some() && !a.output);
        if need.is_empty() && !unmarked {
            return;
        }
        let key = shape.name.to_ascii_uppercase();
        if !self.sig_cache.contains_key(&key) {
            let Some(session) = self.session.as_mut() else {
                return;
            };
            let args =
                nsql_catalog::routine_args(session.as_mut(), &shape.name).unwrap_or_default();
            self.sig_cache.insert(key.clone(), args);
        }
        let Some(all) = self.sig_cache.get(&key) else {
            return;
        };
        // 오버로드 고르기: 이름 표기가 전부 있고 넘긴 인자 수가 형식 인자 수 이하인 첫 묶음.
        let passed = shape.args.len();
        let mut overloads: Vec<&str> = all.iter().map(|a| a.overload.as_str()).collect();
        overloads.dedup();
        let pick = overloads.into_iter().find(|ov| {
            let params: Vec<&nsql_catalog::RoutineArg> = all
                .iter()
                .filter(|a| a.overload == *ov && a.position >= 1)
                .collect();
            passed <= params.len()
                && shape.args.iter().all(|a| {
                    a.named
                        .as_deref()
                        .is_none_or(|n| params.iter().any(|p| p.name == n))
                })
        });
        let Some(ov) = pick else {
            return;
        };
        let hints: Vec<(String, nsql_core::VarType, String)> = need
            .iter()
            .filter_map(|(pos, named, bind)| {
                let arg = all.iter().find(|a| {
                    a.overload == ov
                        && match named {
                            Some(n) => a.name == *n,
                            None => a.position == *pos,
                        }
                })?;
                Some((
                    (*bind).to_string(),
                    var_type_of(&arg.data_type)?,
                    arg.in_out.clone(),
                ))
            })
            .collect();
        let outputs: Vec<String> = if marks {
            output_binds(&shape, all, ov)
        } else {
            Vec::new()
        };
        for (bind, ty, in_out) in hints {
            // ★ 자동 변수는 OUT 자리에서 서명 타입으로 다시 맞춘다(사용자 10-08 · `VarStore::hint_type`).
            self.engine.vars.hint_type(&bind, ty, &in_out);
        }
        self.engine.call_outputs = outputs;
    }

    /// 엔진의 계획·흡수 부산물을 실행 메시지로(선언 없는 바인드 · 상한에서 잘린 값) — 목록은 여기서 비워진다.
    fn flush_diagnostics(&mut self, emit: &mut dyn FnMut(RunEvent)) {
        if self.engine.diagnostics.is_empty() {
            return;
        }
        let diags = std::mem::take(&mut self.engine.diagnostics);
        for d in &diags {
            match d {
                nsql_script::Diagnostic::Truncated(name, bytes) => {
                    emit(RunEvent::Message(tf(
                        Msg::WarnVarTruncated,
                        &[name, &bytes.to_string()],
                    )));
                }
                // 엔진 안내(T-419 DeclarePrepend 등) = 메시지 그대로(엔진이 두 언어로 만든다).
                nsql_script::Diagnostic::Info(text) => emit(RunEvent::Message(text.clone())),
                nsql_script::Diagnostic::ImplicitVariable(_) => {}
            }
        }
        let implicit = implicit_names(diags);
        if !implicit.is_empty() {
            emit(RunEvent::Message(tf(
                Msg::WarnImplicitVar,
                &[&implicit.join(", ")],
            )));
        }
    }

    /// 시스템 변수(T-153): 접속에 딸린 것(`_USER` · `_CONNECT_IDENTIFIER` · `_DIALECT`).
    fn set_connection_sysvars(&mut self, user: Option<&str>, database: Option<&str>) {
        let sv = &mut self.engine.sysvars;
        sv.insert("_USER".into(), user.unwrap_or_default().to_string());
        sv.insert(
            "_CONNECT_IDENTIFIER".into(),
            database.unwrap_or_default().to_string(),
        );
        sv.insert("_DIALECT".into(), self.engine.dialect.to_string());
    }

    /// 시스템 변수: 문장 하나가 끝난 뒤의 것(`_ROW_COUNT` · `_SQLCODE` · `_ELAPSED_MS`).
    fn set_statement_sysvars(&mut self, rows: Option<u64>, code: i64, elapsed: Duration) {
        let sv = &mut self.engine.sysvars;
        sv.insert("_ROW_COUNT".into(), rows.unwrap_or(0).to_string());
        sv.insert("_SQLCODE".into(), code.to_string());
        sv.insert("_ELAPSED_MS".into(), elapsed.as_millis().to_string());
    }

    /// 서명을 읽는다(루틴당 1회 · 캐시 · 실패 = 빈 목록).
    fn signature(&mut self, name: &str) -> Vec<nsql_catalog::RoutineArg> {
        let key = name.to_ascii_uppercase();
        if !self.sig_cache.contains_key(&key) {
            let Some(session) = self.session.as_mut() else {
                return Vec::new();
            };
            let args = nsql_catalog::routine_args(session.as_mut(), name).unwrap_or_default();
            self.sig_cache.insert(key.clone(), args);
        }
        self.sig_cache.get(&key).cloned().unwrap_or_default()
    }

    /// ★ PostgreSQL `EXEC proc(3, :X, :Y)`(T-151): `CALL`은 OUT/INOUT 인자를 **1행 결과**로 돌려준다(열 = 형식 인자 이름 · 순서 =
    /// OUT 인자 순서). 어느 자리가 OUT인지는 서명(`pg_proc`)만 안다 — 그 자리에 바인드를 줬으면 그 바인드가 값을 받는다.
    /// 종전에는 열 이름과 바인드 이름이 같을 때만 받아졌고, 다르면 **조용히 버려졌다**. 바인드가 하나도 없으면 아무것도 하지 않는다.
    fn infer_call_captures(&mut self, shape: &nsql_script::CallShape) {
        if !shape.args.iter().any(|a| a.bind.is_some()) {
            return;
        }
        let all = self.signature(&shape.name);
        if let Some(c) = call_captures(shape, &all) {
            self.engine.call_captures = Some(c);
        }
    }

    /// 끝낼 때의 커밋(CLI — SQL*Plus처럼 종료 시 커밋): 열린 트랜잭션이 있을 때만 보낸다(T-155 — 서버가 문장마다 커밋하는
    /// 방언에서 빈 `COMMIT`은 왕복 낭비 + PostgreSQL 경고 "there is no transaction in progress").
    pub fn commit_at_exit(&mut self) {
        if !self
            .engine
            .caps
            .autocommit_needs_commit(self.tx_open, self.tx_user)
        {
            return;
        }
        if let Some(s) = self.session.as_mut() {
            let _ = s.commit();
        }
        self.note_tx_ended();
    }

    pub fn note_tx_ended(&mut self) {
        self.tx_changed = false;
        self.tx_user = false;
        self.tx_open = false;
        self.tx_stmts = 0;
        self.hold_stmts.clear();
        self.hold_total = 0;
        self.hold_block = false;
        self.hold_no_where = false;
    }

    /// ★ 이번 실행의 운영 보류 정책(워커가 실행마다 넘긴다 · 끝나면 `None`) — 누적은 비운다.
    pub fn set_hold(&mut self, p: Option<HoldPolicy>) {
        self.hold = p;
        self.hold_stmts.clear();
        self.hold_total = 0;
        self.hold_block = false;
        self.hold_no_where = false;
        self.hold_stop = None;
    }

    /// 보류 모드의 문장 결과 누적(변경이면 기록 · 영향 0행 = 변경 아님 · 114 §9-2 B).
    fn hold_note(&mut self, index: usize, item: &Item, rows: Option<u64>) {
        if self.hold.is_none() {
            return;
        }
        if tx_effect(&item.text, rows, self.engine.dialect) != TxEffect::Change {
            return;
        }
        let class = nsql_core::TxClass::of_sql(&item.text);
        let block = matches!(class, nsql_core::TxClass::Other);
        let no_where = dml_without_where(&item.text);
        self.hold_total += rows.unwrap_or(0);
        self.hold_block |= block;
        self.hold_no_where |= no_where;
        self.hold_stmts.push(HeldStmt {
            index,
            summary: summary(item),
            rows,
            block,
            no_where,
        });
    }

    /// 가장 바깥 실행이 끝났다(또는 사용자 `COMMIT` 직전에 멈췄다) — 보류냐 커밋이냐(D-295~D-297) · 보류 = 사건 + 트랜잭션은 그대로 ·
    /// 기준 미만(기준을 올린 경우만) = 지금 커밋(A9).
    fn hold_finish(
        &mut self,
        at_commit: bool,
        remaining: Option<String>,
        emit: &mut dyn FnMut(RunEvent),
    ) {
        let Some(p) = self.hold else {
            return;
        };
        if !self.tx_changed {
            return;
        }
        // ★ 이번 실행에 새 변경이 없는데 사용자 `COMMIT`을 가로챘다(앞 실행의 보류만 열려 있음 · 10-16 사용자) — 빈 목록으로 알려
        //   호스트가 "COMMIT 문으로는 처리할 수 없음" 경고를 띄운다(GUI). CLI는 보류 정책을 쓰지 않아 여기 오지 않는다.
        if self.hold_stmts.is_empty() {
            if at_commit {
                emit(RunEvent::Held {
                    stmts: Vec::new(),
                    total_rows: 0,
                    at_commit,
                    remaining,
                });
            }
            return;
        }
        if at_commit || hold_decision(&p, self.hold_total, self.hold_block, self.hold_no_where) {
            let stmts = std::mem::take(&mut self.hold_stmts);
            emit(RunEvent::Held {
                stmts,
                total_rows: self.hold_total,
                at_commit,
                remaining,
            });
            return;
        }
        if let Some(sess) = self.session.as_mut() {
            let _ = sess.commit();
        }
        let total = self.hold_total;
        self.note_tx_ended();
        emit(RunEvent::Message(tf(
            Msg::RunHoldAutoCommitted,
            &[&total.to_string()],
        )));
    }

    /// 기본 자동 커밋 모드인가(설정 · 스크립트 `SET AUTOCOMMIT` 반영).
    #[must_use]
    pub fn autocommit(&self) -> bool {
        self.engine.settings.autocommit
    }

    /// 러너가 연 트랜잭션이 살아 있는가(수동 모드).
    #[must_use]
    pub fn tx_open(&self) -> bool {
        self.tx_open
    }

    /// ★ 열린 트랜잭션 안의 변경 문장 수(`nsql shell` 프롬프트 `nsql*n>` · T-77 잔여 10-30) — 트랜잭션이 없으면 0.
    #[must_use]
    pub fn tx_pending(&self) -> usize {
        if self.tx_open {
            self.tx_stmts
        } else {
            0
        }
    }

    /// ★ 그리드 편집 적용(nexa-sql 87 §7·§14 · T-182) — **데이터 보호 불변식**(사용자 09-26): 셀 편집이 만든 UPDATE/DELETE는
    /// 정확히 **1행**만 바꿔야 한다. ① 사전 검사 = 쓰기 전에 `guard`(같은 WHERE의 `COUNT(*)`)로 대상 행 수가 1인지 전부 확인(하나라도 아니면
    /// 아무것도 바꾸지 않는다) ② 실행 = 문장마다 영향 행 수 = 1(아니면 그 자리에서 멈춘다) ③ 실패 = 자동 커밋 모드 → 전체 롤백 ·
    /// 수동 모드 → **세이브포인트**로 우리 부분만 되돌린다(사용자의 앞선 작업은 열린 채) · 되돌리기까지 실패하면 "ROLLBACK 필요"를 보고.
    /// ④ 성공 = 자동 커밋 모드만 커밋 · 수동은 열어 둔다. 원인은 `ApplyReport.error`(문장 index · 단계 · 상세)로.
    pub fn apply_changes(&mut self, stmts: &[ApplyStmt]) -> ApplyReport {
        let mut rep = ApplyReport {
            total: stmts.len(),
            ..ApplyReport::default()
        };
        let auto = self.engine.settings.autocommit;
        let begin = self.engine.caps.tx_begin;
        let dialect = self.engine.dialect;
        let Some(sess) = self.session.as_mut() else {
            rep.error = Some(ApplyError {
                index: 0,
                phase: ApplyPhase::PreCheck,
                message: "no session".into(),
                label: String::new(),
            });
            return rep;
        };
        let run = |sess: &mut Box<dyn Session>, sql: &str| -> Result<(), String> {
            sess.execute(&ExecRequest {
                sql: sql.to_string(),
                params: Vec::new(),
            })
            .map(|_| ())
            .map_err(|e| e.message)
        };
        // 시작문(자동 = 원자성 · 수동 = 열린 트랜잭션이 없을 때만) + 수동 모드 세이브포인트.
        let mut began = false;
        if let Some(b) = begin {
            if auto || !self.tx_open {
                began = true;
                if let Err(m) = run(sess, b) {
                    rep.error = Some(ApplyError {
                        index: 0,
                        phase: ApplyPhase::Begin,
                        message: m,
                        label: String::new(),
                    });
                    return rep;
                }
            }
        }
        let (sp_set, sp_back, sp_release) = savepoint_sql(dialect);
        let mut savepoint = false;
        if !auto {
            if let Some(sql) = sp_set {
                savepoint = run(sess, sql).is_ok();
            }
        }
        // ① 사전 검사 — 쓰기 전에 대상 행 수 전부 확인.
        for (i, st) in stmts.iter().enumerate() {
            let Some(g) = st.guard.as_ref() else { continue };
            let t0 = Instant::now();
            let n = match sess.execute(g) {
                Ok(r) => count_of(&r),
                Err(e) => {
                    rep.log.push(ApplyLogItem {
                        sql: g.sql.clone(),
                        binds: txlog::TxBind::from_params(&g.params, dialect),
                        guard: true,
                        elapsed: t0.elapsed(),
                        rows: None,
                        error: Some(e.message.clone()),
                    });
                    rep.error = Some(ApplyError {
                        index: i,
                        phase: ApplyPhase::PreCheck,
                        message: e.message,
                        label: st.label.clone(),
                    });
                    break;
                }
            };
            rep.log.push(ApplyLogItem {
                sql: g.sql.clone(),
                binds: txlog::TxBind::from_params(&g.params, dialect),
                guard: true,
                elapsed: t0.elapsed(),
                rows: n,
                error: None,
            });
            if n != Some(1) {
                rep.error = Some(ApplyError {
                    index: i,
                    phase: ApplyPhase::PreCheck,
                    message: format!(
                        "target rows = {} (must be exactly 1)",
                        n.map_or_else(|| "?".to_string(), |v| v.to_string())
                    ),
                    label: st.label.clone(),
                });
                break;
            }
        }
        // ② 실행 — 문장마다 영향 행 수 1.
        if rep.error.is_none() {
            for (i, st) in stmts.iter().enumerate() {
                let t0 = Instant::now();
                match sess.execute(&st.req) {
                    Ok(r) => {
                        let n = r.rows_affected.unwrap_or(0);
                        rep.affected.push(n);
                        rep.log.push(ApplyLogItem {
                            sql: st.req.sql.clone(),
                            binds: txlog::TxBind::from_params(&st.req.params, dialect),
                            guard: false,
                            elapsed: t0.elapsed(),
                            rows: Some(n),
                            error: (n != 1)
                                .then(|| format!("affected {n} rows (must be exactly 1)")),
                        });
                        if n != 1 {
                            rep.error = Some(ApplyError {
                                index: i,
                                phase: ApplyPhase::Execute,
                                message: format!("affected {n} rows (must be exactly 1)"),
                                label: st.label.clone(),
                            });
                            break;
                        }
                        rep.done += 1;
                    }
                    Err(e) => {
                        rep.log.push(ApplyLogItem {
                            sql: st.req.sql.clone(),
                            binds: txlog::TxBind::from_params(&st.req.params, dialect),
                            guard: false,
                            elapsed: t0.elapsed(),
                            rows: None,
                            error: Some(e.message.clone()),
                        });
                        rep.error = Some(ApplyError {
                            index: i,
                            phase: ApplyPhase::Execute,
                            message: e.message,
                            label: st.label.clone(),
                        });
                        break;
                    }
                }
            }
        }
        // ③ 마무리.
        if auto {
            if rep.error.is_none() {
                match sess.commit() {
                    Ok(()) => rep.committed = true,
                    Err(e) => {
                        rep.error = Some(ApplyError {
                            index: rep.done,
                            phase: ApplyPhase::Commit,
                            message: e.message,
                            label: String::new(),
                        });
                    }
                }
            }
            if rep.error.is_some() {
                rep.rolled_back = sess.rollback().is_ok();
                rep.rollback_needed = !rep.rolled_back;
                rep.done = 0;
            }
            self.note_tx_ended();
        } else {
            if rep.error.is_some() {
                if savepoint {
                    if let Some(sql) = sp_back {
                        rep.rolled_back = run(sess, sql).is_ok();
                    }
                }
                rep.rollback_needed = !rep.rolled_back;
                if rep.rolled_back {
                    rep.done = 0;
                }
            } else if savepoint {
                if let Some(sql) = sp_release {
                    let _ = run(sess, sql);
                }
            }
            // 수동 모드 = 우리가 열었든 이미 열려 있었든 트랜잭션은 열린 채(커밋/롤백은 사용자). ★ 시작문이 없는 방언(Oracle ·
            //   암묵 트랜잭션)에서는 `began`이 늘 false라 첫 적용 뒤 "열림"을 알리지 못했다 → 커밋/롤백 버튼·대기 표식이 안 켜지고
            //   서버에는 미커밋 변경이 남는 위험(사용자 09-26 실서버) — 실제로 쓴 문장이 있으면(또는 되돌리지 못했으면) 열린 것이다.
            if began || self.tx_open || rep.done > 0 || rep.rollback_needed {
                self.tx_open = true;
                rep.tx_left_open = true;
            }
            if rep.done > 0 || rep.rollback_needed {
                self.tx_changed = true;
                self.tx_stmts += rep.done;
            }
        }
        rep
    }

    /// 이 문장 뒤 열린 트랜잭션을 끝내도 되는가(수동 모드 전용 · docs/56 L1). 문장의 흔적을 상태에 반영하고, 변경·사용자 통제가
    /// 없을 때만 참 — `Strict`면 서버에 "쓰기를 했는가"를 한 번 더 묻는다(쓰기가 보이면 변경으로 기록 · 질의 실패 = 끝내지 않음).
    fn read_end_due(&mut self, sql: &str, rows_affected: Option<u64>) -> bool {
        match tx_effect(sql, rows_affected, self.engine.dialect) {
            TxEffect::Ended => {
                self.note_tx_ended();
                return false;
            }
            TxEffect::UserHold => {
                self.tx_user = true;
                return false;
            }
            TxEffect::Change => {
                self.tx_changed = true;
                self.tx_stmts += 1;
                return false;
            }
            TxEffect::Clean => {}
        }
        if self.read_end == ReadEnd::Off || self.tx_changed || self.tx_user {
            return false;
        }
        if self.read_end == ReadEnd::Strict {
            if let (Some(probe), Some(s)) = (self.engine.caps.strict_probe, self.session.as_mut()) {
                let wrote = s
                    .execute(&ExecRequest {
                        sql: probe.to_string(),
                        params: Vec::new(),
                    })
                    .ok()
                    .and_then(|r| {
                        r.result_sets
                            .first()
                            .and_then(|rs| rs.rows.first())
                            .and_then(|row| row.first())
                            .map(|v| v.display())
                    });
                match wrote.as_deref() {
                    Some("0") => {}
                    Some(_) => {
                        self.tx_changed = true;
                        return false;
                    }
                    None => return false,
                }
            }
        }
        true
    }

    /// 열린 커서를 닫는다(없으면 무시). 미뤄 둔 자동 커밋이 있으면 여기서 커밋한다.
    pub fn close_cursor(&mut self) {
        if let Some(c) = self.cursor.take() {
            if let Some(s) = self.session.as_mut() {
                let _ = s.close_cursor(c.handle);
                if finish_tx(s.as_mut(), c.deferred_end) {
                    self.tx_open = false;
                    self.tx_stmts = 0;
                }
            }
        }
    }

    /// 커서에서 다음 `n`행(0 = 끝까지) — `(결과, 더 있음, Navigate 스팬)`. 옛 핸들·닫힌 커서면 `Err`(호스트는 OFFSET 폴백).
    /// 끝에 닿으면 커서를 닫는다(미룬 자동 커밋 포함).
    pub fn fetch_next(
        &mut self,
        h: CursorHandle,
        n: usize,
    ) -> Result<(ResultSet, bool, Timeline), DbError> {
        if !self.cursor.as_ref().is_some_and(|c| c.handle == h) {
            return Err(msg_err(format!("cursor #{} is not open", h.0)));
        }
        let Some(session) = self.session.as_mut() else {
            self.cursor = None;
            return Err(msg_err(t(Msg::NoSession)));
        };
        let t0 = Instant::now();
        let r = session.fetch_next(h, n);
        let mut timeline = Timeline::new();
        match r {
            Ok((rs, more)) => {
                let span = timeline.push(Stage::Navigate, t0.elapsed());
                span.rows = Some(rs.rows.len() as u64);
                span.bytes = Some(rs.approx_bytes());
                span.note = Some(format!("cursor #{}", h.0));
                if let Some(c) = self.cursor.as_mut() {
                    c.served += rs.rows.len();
                    c.last_used = Instant::now();
                }
                if !more {
                    // 드라이버가 이미 닫았다 — 러너 상태·미룬 커밋만 정리.
                    if let Some(c) = self.cursor.take() {
                        if finish_tx(session.as_mut(), c.deferred_end) {
                            self.tx_open = false;
                            self.tx_stmts = 0;
                        }
                    }
                }
                Ok((rs, more, timeline))
            }
            Err(e) => {
                self.close_cursor();
                Err(e)
            }
        }
    }

    /// 커서에서 끝까지 — 배치(`fetch_size`·최소 2000)로 읽어 이어 붙인다. `budget_bytes`(0 = 무제한)에 닿거나 `progress`
    /// (행 수, 바이트 → 계속?)가 `false`를 돌려주면 멈춘다. 반환 = (모은 행, 아직 남았는가(예산/취소로 멈춤), Navigate 스팬).
    pub fn fetch_all(
        &mut self,
        h: CursorHandle,
        budget_bytes: u64,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<(ResultSet, bool, Timeline), DbError> {
        let mut acc = ResultSet::default();
        let (stopped, timeline) =
            self.fetch_all_with(h, budget_bytes, &mut |mut rs, rows, bytes| {
                if acc.columns.is_empty() {
                    acc.columns = std::mem::take(&mut rs.columns);
                }
                acc.rows.append(&mut rs.rows);
                progress(rows, bytes)
            })?;
        Ok((acc, stopped, timeline))
    }

    /// ★ 배치 **싱크** 판(사용자 10-09 "결과가 조회 가능한 상태가 되는 시점에 바로 화면에 append"): 배치마다 `on_batch(배치, 누적 행 수,
    /// 누적 바이트)`에 **넘겨 준다**(러너가 모으지 않는다 · 메모리 = 배치 하나) · 거짓을 돌려주면 중지. 반환 = (중지됨, Timeline).
    /// 예산(`budget_bytes`)·마지막 배치 뒤에는 콜백의 반환과 무관하게 끝난다. [`Self::fetch_all`]은 이것을 모아 주는 꼴.
    pub fn fetch_all_with(
        &mut self,
        h: CursorHandle,
        budget_bytes: u64,
        on_batch: &mut dyn FnMut(ResultSet, u64, u64) -> bool,
    ) -> Result<(bool, Timeline), DbError> {
        let batch = self.fetch_size.max(2000);
        let mut rows: u64 = 0;
        let mut bytes: u64 = 0;
        let mut timeline = Timeline::new();
        let t0 = Instant::now();
        let mut stopped = false;
        loop {
            let (rs, more, _) = self.fetch_next(h, batch)?;
            bytes += rs.approx_bytes();
            rows += rs.rows.len() as u64;
            let go = on_batch(rs, rows, bytes);
            if !more {
                break;
            }
            let over_budget = budget_bytes > 0 && bytes >= budget_bytes;
            if over_budget || !go {
                stopped = true;
                break;
            }
        }
        let span = timeline.push(Stage::Navigate, t0.elapsed());
        span.rows = Some(rows);
        span.bytes = Some(bytes);
        span.note = Some(
            if stopped {
                "fetch all (stopped)"
            } else {
                "fetch all"
            }
            .into(),
        );
        Ok((stopped, timeline))
    }

    /// ★ T-8 스트리밍 내보내기(10-30): 질의 결과를 **배치마다** `on_batch`로 넘긴다(메모리에 모으지 않음) — 커서를 여는 드라이버는
    /// `query_stream` + `fetch_all_with`(배치 = `fetch_size`·2000 중 큰 쪽) · 못 여는 드라이버(SQL Server·MySQL·ODBC …)는 OFFSET 페이지
    /// 루프 폴백(`fetch_offset` · 정렬 없는 질의는 순서가 흔들릴 수 있어 호출자가 안내). `on_batch`가 거짓이면 멈춘다(취소). 반환 = 넘긴 행 수.
    pub fn export_stream(
        &mut self,
        sql: &str,
        on_batch: &mut dyn FnMut(&ResultSet) -> bool,
    ) -> Result<u64, DbError> {
        let batch = self.fetch_size.max(2000);
        if self.cursor_supported() {
            let (first, h, _) = self.query_stream(sql, batch)?;
            let mut total = first.rows.len() as u64;
            let go = on_batch(&first);
            match h {
                Some(h) if go => {
                    let (_stopped, _) = self.fetch_all_with(h, 0, &mut |rs, _, _| {
                        total += rs.rows.len() as u64;
                        on_batch(&rs)
                    })?;
                    self.close_cursor();
                }
                _ => self.close_cursor(),
            }
            return Ok(total);
        }
        let mut offset = 0usize;
        let mut total = 0u64;
        loop {
            let (rs, more, _) = self.fetch_offset(sql, offset, batch)?;
            let n = rs.rows.len();
            total += n as u64;
            let go = on_batch(&rs);
            if !more || n == 0 || !go {
                break;
            }
            offset += n;
        }
        Ok(total)
    }

    /// `SELECT COUNT(*) FROM (질의) x` — 같은 세션에서 곁가지로(열린 커서는 그대로). 반환 = (건수, Navigate 스팬).
    pub fn count(&mut self, sql: &str) -> Result<(u64, Timeline), DbError> {
        let Some(d) = self.dialect() else {
            return Err(msg_err(t(Msg::NoSession)));
        };
        let t0 = Instant::now();
        let (rs, _, _) = self.query_once(&nsql_io::paging::count_sql(d, sql), 0)?;
        let n = rs
            .rows
            .first()
            .and_then(|r| r.first())
            .and_then(|v| v.display().trim().parse::<f64>().ok())
            .map(|n| n.max(0.0) as u64)
            .ok_or_else(|| msg_err(t(Msg::ErrCountParse)))?;
        let mut timeline = Timeline::new();
        let span = timeline.push(Stage::Navigate, t0.elapsed());
        span.rows = Some(n);
        span.note = Some("count".into());
        Ok((n, timeline))
    }

    /// OFFSET 재질의(docs/43 §3-4 · 커서가 없을 때) — `offset`행을 건너뛰고 `limit`행(0 = 남은 전부). 반환 = (결과, 더 있음, Navigate 스팬).
    pub fn fetch_offset(
        &mut self,
        sql: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(ResultSet, bool, Timeline), DbError> {
        let Some(d) = self.dialect() else {
            return Err(msg_err(t(Msg::NoSession)));
        };
        // 래핑은 limit+1행을 달라고 하고 상한은 limit — 한 행이 더 오면 `more`.
        // ★ 157차: 서버가 `OFFSET … FETCH`를 못 받으면(Db2 ≤ 10.5 · `Caps::offset_fetch`) `ROW_NUMBER()` 폴백 + 번호 열 떼기.
        let offset_fetch = self.session.as_ref().is_none_or(|s| s.caps().offset_fetch);
        // ★ T-116 겹침 검사: 직전 페이지 꼬리가 이 문장·이 위치면 `offset-1`행부터 받아 첫 행을 꼬리와 대조한다(한 행 비용).
        let check = self.overlap_check
            && offset > 0
            && self
                .page_tail
                .as_ref()
                .is_some_and(|(s, served, _)| *served == offset && same_sql(s, sql));
        let (start, extra) = if check { (offset - 1, 1) } else { (offset, 0) };
        let (q, max) = if limit == 0 {
            (
                nsql_io::paging::page_sql_opts(d, sql, start, 0, offset_fetch),
                0,
            )
        } else {
            (
                nsql_io::paging::page_sql_opts(d, sql, start, limit + 1 + extra, offset_fetch),
                limit + extra,
            )
        };
        let t0 = Instant::now();
        let (mut rs, more, _) = self.query_once(&q, max)?;
        nsql_io::paging::strip_rn(&mut rs);
        if check {
            let expected = self.page_tail.as_ref().map(|t| &t.2);
            if rs.rows.first() != expected {
                // 꼬리가 사라졌거나 다른 행이 그 자리에 왔다 = 서버 결과가 바뀜 → 이어 붙이면 누락·중복. 호스트는 처음부터 다시 받는다.
                self.page_tail = None;
                return Err(msg_err(tf(Msg::ErrPageOverlap, &[&offset.to_string()])));
            }
            rs.rows.remove(0);
        }
        let mut timeline = Timeline::new();
        let span = timeline.push(Stage::Navigate, t0.elapsed());
        span.rows = Some(rs.rows.len() as u64);
        span.bytes = Some(rs.approx_bytes());
        span.note = Some(format!("offset {offset}"));
        Ok((rs, more, timeline))
    }

    /// 다음 세그먼트 — 같은 문장의 커서가 `offset` 위치에 열려 있으면 커서로, 아니면 OFFSET 재질의로(호스트 공용 · GUI 워커 · CLI `\more`).
    /// 커서 경로가 실패하면(옛 핸들 · 유휴 초과) 조용히 OFFSET으로 간다. 반환 = (결과, 더 있음, Navigate 스팬, 커서를 썼는가).
    pub fn fetch_page(
        &mut self,
        sql: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(ResultSet, bool, Timeline, bool), DbError> {
        if limit > 0 && self.cursor_matches(sql, offset) {
            if let Some((h, _)) = self.cursor() {
                if let Ok((rs, more, tl)) = self.fetch_next(h, limit) {
                    self.remember_tail(sql, offset, &rs);
                    return Ok((rs, more, tl, true));
                }
            }
        }
        let (rs, more, tl) = self.fetch_offset(sql, offset, limit)?;
        self.remember_tail(sql, offset, &rs);
        Ok((rs, more, tl, false))
    }

    /// 겹침 검사 기준 갱신(T-116): 이 페이지의 마지막 행 = 다음 OFFSET 재질의가 대조할 꼬리.
    fn remember_tail(&mut self, sql: &str, offset: usize, rs: &ResultSet) {
        self.page_tail = rs
            .rows
            .last()
            .map(|row| (sql.to_string(), offset + rs.rows.len(), row.clone()));
    }

    /// 이 문장을 OFFSET 재질의·재실행으로 이어 받을 수 있는가 — **조회(SELECT·WITH)만**. 프로시저 호출·`PRINT`가 낸 커서 결과
    /// (T-202)는 커서가 닫히면 이어 받을 길이 없다(프로시저를 다시 돌리지 않는다 · 호스트는 "커서 닫힘"으로 끝낸다).
    #[must_use]
    pub fn requery_ok(sql: &str) -> bool {
        matches!(nsql_script::classify_sql(sql), nsql_script::SqlKind::Query)
    }

    /// 프로필 해석기 장착(체이닝).
    #[must_use]
    pub fn with_resolver(mut self, r: Resolver) -> Self {
        self.resolver = Some(r);
        self
    }

    /// `CONNECT prod`처럼 **사용자명 자리에만 값이 있는** 스펙 = 프로필 이름 후보.
    fn bare_name(spec: &ConnectSpec) -> Option<&str> {
        match spec {
            ConnectSpec {
                user: Some(u),
                password: None,
                host: None,
                port: None,
                database: None,
                role: None,
                dialect: None,
                schema: None,
                env: _,
                tls: _,
                tls_ca: _,
                ssh: _,
                ssh_key: _,
                driver: _,
                autocommit: _,
            } => Some(u.as_str()),
            _ => None,
        }
    }

    /// 이미 열린 세션으로 시작(GUI 연결 대화상자 · 테스트).
    pub fn with_session(
        mut self,
        session: Box<dyn Session>,
        description: impl Into<String>,
    ) -> Self {
        self.engine.set_caps(session.dialect(), session.caps());
        self.set_connection_sysvars(None, None);
        self.session = Some(session);
        self.connection = Some(description.into());
        // 새 세션 = 트랜잭션 없음(러너가 연 트랜잭션 표시도 내린다 · T-146) · 서명 캐시는 서버마다 다르다.
        self.sig_cache.clear();
        self.note_tx_ended();
        // 세션에 묶인 값(REF CURSOR 핸들)은 새 세션에서 죽은 핸들이다.
        self.engine.vars.invalidate_cursors();
        self.push_max_rows();
        self
    }

    /// 지금 붙어 있는 대상(**비밀번호 없음** · 프로필 이름은 푼 뒤) — 호스트의 자동 재접속 원천. 비밀번호는 세션 자격 금고
    /// (nsql-vault `session` · 접속마다 opener가 인출)에서 온다(21 §7 · 사용자 10-09 "저장된 비밀번호가 아니면 암호화해서 메모리에 ·
    /// 인출 API로"). 접속한 적이 없으면 None.
    #[must_use]
    pub fn connected_to(&self) -> Option<&ConnectSpec> {
        self.connected_to.as_ref()
    }

    pub fn connect(&mut self, spec: &ConnectSpec, emit: &mut dyn FnMut(RunEvent)) -> bool {
        // 프로필 이름이면 저장소에서 완전한 스펙으로 바꾼다(비밀번호 포함).
        let resolved;
        let spec = match (Self::bare_name(spec), self.resolver.as_mut()) {
            (Some(name), Some(r)) => match r(name) {
                Ok(Some(s)) => {
                    resolved = s;
                    &resolved
                }
                Ok(None) => spec,
                Err(message) => {
                    emit(RunEvent::Error {
                        index: 0,
                        line: 0,
                        error: DbError {
                            code: None,
                            message,
                            position: None,
                        },
                    });
                    return false;
                }
            },
            _ => spec,
        };
        self.close_cursor();
        if let Some(s) = self.session.as_mut() {
            let _ = s.commit();
        }
        // ★ **같은 서버·계정**에 다시 `CONNECT` = 자격을 바꿔 다시 붙겠다는 뜻(사용자 09-21 · SQL*Plus와 같다): 기존 접속을
        //   **먼저 닫는다** → 새 자격이 거부되면 옛 자격의 세션이 살아남지 않는다("연결 없음"으로 끝난다). 다른 서버로의
        //   `CONNECT`가 실패한 경우는 종전대로 옛 접속을 유지한다.
        let same_target = self.connected_to.as_ref().is_some_and(|c| {
            c.dialect == spec.dialect
                && c.host == spec.host
                && c.port == spec.port
                && c.database == spec.database
                && c.user == spec.user
                && c.role == spec.role
        });
        //   `Disconnected`는 **실패했을 때만** 알린다 — 성공하면 `Connected` 하나로 충분하고, 호스트는 `Disconnected`를 보면
        //   전용 세션을 거두기 때문이다(곧 이어질 접속이 갈 곳을 잃는다).
        let closed_first = same_target && self.session.take().is_some();
        if closed_first {
            self.connection = None;
            self.connected_to = None;
            self.note_tx_ended();
        }
        match (self.opener)(spec) {
            Ok(mut session) => {
                let dialect = session.dialect();
                self.engine.set_caps(dialect, session.caps());
                // ★ 접속별 자동 커밋(`?autocommit=` · 프로필 `autocommit=` · T-77 잔여 10-30) — 접속 직후 이 러너의 모드로(설정보다 우선 ·
                //   이후 `SET AUTOCOMMIT`·호스트 토글은 종전대로 덮는다). 없으면 그대로.
                if let Some(ac) = spec.autocommit {
                    self.engine.settings.autocommit = ac;
                }
                self.set_connection_sysvars(spec.user.as_deref(), spec.database.as_deref());
                if self.engine.settings.serveroutput {
                    let _ = session.set_option("serveroutput", "on");
                }
                // 기본 스키마(`?schema=` · 프로필 설정) — 접속 직후 세션 설정(방언별 · 실패해도 접속은 유지 · 오류는 이벤트로).
                if let Some(sc) = &spec.schema {
                    if let Err(e) = session.set_option("schema", sc) {
                        emit(RunEvent::Error {
                            index: 0,
                            line: 0,
                            error: e,
                        });
                    }
                }
                self.session = Some(session);
                self.connection = Some(spec.redacted());
                self.connected_to = Some(ConnectSpec {
                    password: None,
                    ..spec.clone()
                });

                // 새 세션 = 트랜잭션 없음(러너가 연 트랜잭션 표시도 내린다 · T-146) · 서명 캐시는 서버마다 다르다.
                self.sig_cache.clear();
                self.note_tx_ended();
                // 세션에 묶인 값(REF CURSOR 핸들)은 새 세션에서 죽은 핸들이다.
                self.engine.vars.invalidate_cursors();
                self.push_max_rows();
                // 현재 스키마 1회(실패 = 빈 글 · 링크 판정은 사용자 이름으로 폴백).
                let schema = self
                    .session
                    .as_mut()
                    .and_then(|s| nsql_catalog::current_schema(s.as_mut()).ok())
                    .unwrap_or_default();
                // 현재 DB 1회(DB 전환이 있는 방언만 · 탭 작업 단위의 기본값 · ⑯).
                let database = if self.engine.caps.syntax.database_switch {
                    self.session
                        .as_mut()
                        .and_then(|s| nsql_catalog::server_info(s.as_mut()).ok())
                        .map(|i| i.current_db)
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                emit(RunEvent::Connected {
                    description: spec.redacted(),
                    dialect,
                    schema,
                    database,
                    autocommit: self.engine.settings.autocommit,
                });
                // ★ 드라이버 제약 안내(docs/110 §6) — 접속 직후 한 번(제약이 없는 드라이버는 조용하다).
                if let Some(s) = self.session.as_ref() {
                    let limits = s.limits();
                    if !limits.is_empty() {
                        emit(RunEvent::DriverLimits {
                            driver: s.driver_name(),
                            limits: limits.to_vec(),
                            at_connect: true,
                        });
                    }
                }
                true
            }
            Err(e) => {
                if closed_first {
                    emit(RunEvent::ConnectionClosed);
                }
                // 접속 거부가 드라이버 제약(OS 인증 · 11g · NNE)이면 제약 안내도(세션이 없으므로 드라이버 이름 = 메시지 머리).
                if let Some(l) = nsql_core::DriverLimit::find_in(&e.message) {
                    emit(RunEvent::DriverLimits {
                        driver: e.message.split(':').next().unwrap_or("").trim().to_string(),
                        limits: vec![l],
                        at_connect: false,
                    });
                }
                emit(RunEvent::Error {
                    index: 0,
                    line: 0,
                    error: e,
                });
                false
            }
        }
    }

    /// 스크립트 전체 실행(출처 파일 없음 — 편집기 버퍼·stdin · `@@`는 cwd 기준). 오류 시 `WHENEVER SQLERROR EXIT`면 중단. 반환 = 오류 수.
    pub fn run_script(
        &mut self,
        src: &str,
        prompt: Prompter<'_>,
        emit: &mut dyn FnMut(RunEvent),
    ) -> usize {
        self.run_script_in(src, None, prompt, emit)
    }

    /// ★ D-355(10-24 · 사용자 "변수 창이 조회 시점 값"): 사용 시 모드의 **stale 수식을 지금 한 묶음으로** 재계산 — 변수 창이 열려 있을 때
    /// 실행 끝(`Cmd::Run.refresh_vars`) 또는 창의 ⟳. `EXEC :이름 := 수식`들을 스크립트 하나로 돌려 `RunEvent::Vars` 한 번 ·
    /// 연쇄 의존(재계산이 또 다른 수식을 stale로)은 최대 4회 · 오류가 나면 멈춤. 돌려준 값 = 재계산한 수식 수(0 = 왕복 없음).
    pub fn refresh_stale_vars(&mut self, emit: &mut dyn FnMut(RunEvent)) -> usize {
        let mut total = 0;
        for _ in 0..4 {
            let stale = self.engine.vars.take_all_stale_formulas();
            if stale.is_empty() {
                break;
            }
            let src: String = stale
                .iter()
                .map(|(n, f)| format!("EXEC :{n} := {f}\n"))
                .collect();
            total += stale.len();
            let mut no_prompt = |_: &str| None;
            if self.run_script(&src, &mut no_prompt, emit) > 0 {
                break;
            }
        }
        total
    }

    /// ★ **소스 전체를 한 단위로**(09-30 · 사용자 "객체 소스 탭 F5 = CREATE 한 번"): 객체 소스 탭(소스 열기 · DDL 열기)의
    /// F5는 본문을 문장으로 나누지 않고 **한 항목**으로 보낸다 — 분할기가 방언·머리 줄 모양 때문에 잘못 나눠 "줄 단위 실행"이
    /// 되는 일이 없다. 분할이 정확히 1항목이면 그대로(오늘과 같은 길) · 아니면 [`whole_item`]로 합친다(끝의 `/`·`GO`·`;` 제거).
    /// 컴파일 검사·`RunEvent::Message`(compiled)는 보통 길과 같다.
    pub fn run_whole(
        &mut self,
        src: &str,
        prompt: Prompter<'_>,
        emit: &mut dyn FnMut(RunEvent),
    ) -> usize {
        let items = nsql_script::split_script_in(src, self.dialect());
        let item = match items.as_slice() {
            [one] => one.clone(),
            _ => whole_item(src),
        };
        self.run_depth += 1;
        let ok = self.run_item(0, &item, prompt, emit);
        self.run_depth -= 1;
        if self.run_depth == 0 {
            self.hold_finish(false, None, emit);
        }
        usize::from(!ok)
    }

    /// 스크립트 전체 실행 — `script_path`가 있으면 그 폴더가 안쪽 `@@path`·`:r path`의 기준이 된다(`nsql run file.sql`).
    pub fn run_script_in(
        &mut self,
        src: &str,
        script_path: Option<&Path>,
        prompt: Prompter<'_>,
        emit: &mut dyn FnMut(RunEvent),
    ) -> usize {
        let pushed = script_path.map(|p| {
            let dir = p.parent().filter(|d| !d.as_os_str().is_empty());
            self.script_dirs
                .push(dir.map_or_else(|| PathBuf::from("."), Path::to_path_buf));
        });
        // 방언을 아는 분할(SQLite 트리거 `END;` · `BEGIN;` 트랜잭션 — nsql-script `split_script_in`).
        let items = nsql_script::split_script_in(src, self.dialect());
        let mut errors = 0;
        self.run_depth += 1;
        for (i, item) in items.iter().enumerate() {
            let ok = self.run_item(i, item, prompt, emit);
            // 진단(10-24 · ODBC GUI 커밋 멈춤 추적에 쓴 것 · `NSQL_TRACE_RUN=1` · 환경은 1회만 읽는다).
            static TRACE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            if *TRACE.get_or_init(|| std::env::var_os("NSQL_TRACE_RUN").is_some()) {
                eprintln!(
                    "[run] item {i} ok={ok} exit_on_error={} hold_stop={:?} tx_open={} tx_user={}",
                    self.engine.settings.exit_on_error, self.hold_stop, self.tx_open, self.tx_user
                );
            }
            if !ok {
                errors += 1;
                if self.engine.settings.exit_on_error {
                    break;
                }
            }
            // ★ 운영 보류: 사용자 `COMMIT` 직전에 멈췄다 → 남은 원문(그 뒤 첫 문장부터)을 호스트에 넘기고 끝(D-299).
            if self.hold_stop.take().is_some() {
                let remaining = items
                    .get(i + 1)
                    .map(|it| src[it.span.start..].trim().to_string())
                    .filter(|r| !r.is_empty());
                self.hold_finish(true, remaining, emit);
                break;
            }
        }
        self.run_depth -= 1;
        if self.run_depth == 0 {
            self.hold_finish(false, None, emit);
        }
        if pushed.is_some() {
            self.script_dirs.pop();
        }
        // 가장 바깥 실행이 끝났다 = 변수 표의 바뀐 것을 한 번에 알린다(`@스크립트` 안쪽에서는 알리지 않는다).
        if self.run_depth == 0 {
            let changed = self.engine.vars.take_dirty();
            let defines_dirty = std::mem::take(&mut self.engine.defines_dirty);
            if !changed.is_empty() || defines_dirty {
                // 층별로 따로(T-153 후속 10-26): 탭 층은 호스트가 다음 실행에 되돌려 주고 · 공유 층은 러너가 들고 · 글로벌 층은 호스트가 전파.
                let mut layered =
                    |layer: nsql_script::MacroLayer| -> Vec<(String, String, String)> {
                        let pairs: Vec<(String, String)> = self
                            .engine
                            .defines
                            .layer(layer)
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        pairs
                            .into_iter()
                            .map(|(k, v)| {
                                let shown = self.engine.define_display(&k, &v);
                                (k, v, shown)
                            })
                            .collect()
                    };
                let defines = layered(nsql_script::MacroLayer::Local);
                let defines_shared = layered(nsql_script::MacroLayer::Shared);
                let defines_global = layered(nsql_script::MacroLayer::Global);
                emit(RunEvent::Vars {
                    local: self.engine.vars.local_states(),
                    shared: self.engine.vars.shared_states(),
                    global: self.engine.vars.global_states(),
                    changed,
                    defines,
                    defines_shared,
                    defines_global,
                });
            }
        }
        errors
    }

    /// `@path`(cwd 기준) · `@@path`·`:r path`(호출 스크립트 폴더 기준 · 없으면 cwd) → 실제 경로.
    /// 그 경로가 없고 확장자도 없으면 `.sql`을 붙여 본다(SQL*Plus).
    fn resolve_script(&self, path: &str, relative_to_caller: bool) -> PathBuf {
        let p = Path::new(path);
        let mut full = match self.script_dirs.last() {
            Some(dir) if relative_to_caller && p.is_relative() => dir.join(p),
            _ => p.to_path_buf(),
        };
        if !full.exists() && full.extension().is_none() {
            let alt = full.with_extension("sql");
            if alt.exists() {
                full = alt;
            }
        }
        full
    }

    /// `@`/`@@`/`:r` 포함 실행(T-9). 인자가 있으면 `&1..&n`을 정의하고 **돌아올 때 호출자의 값을 복원**한다(중첩 안전 ·
    /// 인자가 없으면 호출자의 `&1..`을 그대로 물려받는다). 프롬프트는 호출자의 것을 그대로 쓴다.
    fn include(
        &mut self,
        index: usize,
        item: &Item,
        full: &Path,
        args: &[String],
        prompt: Prompter<'_>,
        emit: &mut dyn FnMut(RunEvent),
    ) -> bool {
        if self.script_dirs.len() >= MAX_SCRIPT_DEPTH {
            emit(RunEvent::Error {
                index,
                line: item.line,
                error: msg_err(tf(
                    Msg::ScriptNestTooDeep,
                    &[&full.display().to_string(), &MAX_SCRIPT_DEPTH.to_string()],
                )),
            });
            return false;
        }
        let src = match std::fs::read_to_string(full) {
            Ok(s) => s,
            Err(e) => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: msg_err(tf(
                        Msg::ScriptReadFailed,
                        &[&full.display().to_string(), &e.to_string()],
                    )),
                });
                return false;
            }
        };
        let saved: Vec<(String, Option<String>)> = (1..=args.len())
            .map(|i| {
                let k = i.to_string();
                let v = self.engine.defines.get(&k).cloned();
                (k, v)
            })
            .collect();
        if !args.is_empty() {
            self.engine.set_args(args);
        }
        let errs = self.run_script_in(&src, Some(full), prompt, emit);
        for (k, v) in saved {
            match v {
                Some(v) => {
                    self.engine.defines.insert(k, v);
                }
                None => {
                    self.engine.defines.remove(&k);
                }
            }
        }
        errs == 0
    }

    /// 항목 하나. 반환 = 성공 여부.
    pub fn run_item(
        &mut self,
        index: usize,
        item: &Item,
        prompt: Prompter<'_>,
        emit: &mut dyn FnMut(RunEvent),
    ) -> bool {
        // ★ 운영 보류(D-299): 스크립트의 사용자 `COMMIT`은 변경이 열려 있으면 **보내지 않고 멈춘다**(호스트 확인 뒤 재진입 = `commit_ok`).
        if let Some(p) = self.hold.as_mut() {
            if is_commit_sql(&item.text) {
                if p.commit_ok {
                    p.commit_ok = false;
                } else if self.tx_changed {
                    self.hold_stop = Some(index);
                    return true;
                }
            }
        }
        emit(RunEvent::Begin {
            index,
            line: item.line,
            summary: summary(item),
            server: goes_to_server(item),
        });
        self.infer_call_bind_types(item);
        if item.text.contains("_DATE") || item.text.contains("_TIMESTAMP") {
            let now = nsql_log::now_local().stamp();
            let sv = &mut self.engine.sysvars;
            sv.insert("_DATE".into(), now.get(..10).unwrap_or(&now).to_string());
            sv.insert(
                "_TIMESTAMP".into(),
                now.get(..19).unwrap_or(&now).to_string(),
            );
        }
        let mut guard = 0;
        loop {
            let actions = self.engine.plan(item);
            // ★ 계획 부산물의 소비자(T-153): 선언 없이 쓰여 **NULL로 만들어진 바인드**를 알린다 — 종전에는 아무도 읽지 않아
            // 오타가 조용히 NULL이 됐고, 목록은 비워지지 않은 채 세션 내내 쌓였다(09-21 Windows 점검).
            self.flush_diagnostics(emit);
            if let [Action::NeedInput { name }] = actions.as_slice() {
                // 엄격 모드(T-9): 묻지 않고 오류 — 배치에서 조용히 빈 값이 들어가는 사고 방지.
                if self.strict {
                    emit(RunEvent::Error {
                        index,
                        line: item.line,
                        error: msg_err(tf(Msg::StrictUndefined, &[name])),
                    });
                    return false;
                }
                match prompt(name) {
                    Some(v) => {
                        self.engine.define(name, &v);
                        guard += 1;
                        if guard > 64 {
                            emit(RunEvent::Error {
                                index,
                                line: item.line,
                                error: msg_err(t(Msg::SubstLoop)),
                            });
                            return false;
                        }
                        continue;
                    }
                    None => {
                        emit(RunEvent::Error {
                            index,
                            line: item.line,
                            error: msg_err(tf(Msg::SubstUndefined, &[name])),
                        });
                        return false;
                    }
                }
            }
            // ★ 재계획(D-184): 앞의 재계산 행동을 먼저 실행(OUT 흡수 = 값 갱신)하고 같은 항목을 다시 계획한다.
            if matches!(actions.last(), Some(Action::Replan)) {
                let n = actions.len() - 1;
                for a in actions.into_iter().take(n) {
                    if !self.perform(index, item, a, prompt, emit) {
                        return false;
                    }
                }
                guard += 1;
                if guard > 64 {
                    emit(RunEvent::Error {
                        index,
                        line: item.line,
                        error: msg_err(t(Msg::SubstLoop)),
                    });
                    return false;
                }
                continue;
            }
            let mut ok = true;
            for a in actions {
                if !self.perform(index, item, a, prompt, emit) {
                    ok = false;
                    break;
                }
            }
            return ok;
        }
    }

    fn perform(
        &mut self,
        index: usize,
        item: &Item,
        action: Action,
        prompt: Prompter<'_>,
        emit: &mut dyn FnMut(RunEvent),
    ) -> bool {
        match action {
            Action::Execute {
                prepared,
                expect_out,
                kind,
            } => {
                // 엄격 모드(T-9): 선언 없는 `:bind`는 암묵 선언 대신 오류(SQL*Plus "bind variable not declared").
                if self.strict && !prepared.implicit.is_empty() {
                    for n in &prepared.implicit {
                        self.engine.vars.remove(n);
                    }
                    emit(RunEvent::Error {
                        index,
                        line: item.line,
                        error: msg_err(tf(
                            Msg::StrictImplicitBind,
                            &[&prepared.implicit.join(", ")],
                        )),
                    });
                    return false;
                }
                // DDL은 루틴 서명을 바꿀 수 있다 → 서명 캐시를 비운다(다음 EXEC에서 다시 읽는다).
                if matches!(kind, SqlKind::Ddl)
                    || item
                        .text
                        .trim_start()
                        .get(..6)
                        .is_some_and(|w| w.eq_ignore_ascii_case("CREATE"))
                {
                    self.sig_cache.clear();
                }
                self.execute(index, item, prepared, expect_out, emit)
            }
            Action::LocalAssign { name, value } => {
                // 대입 메아리 — 비밀 이름(D-140)은 값을 찍지 않는다(T-162 ③: CLI 출력·GUI 로그 창·로그 파일로 새는 길이었다).
                let shown = if nsql_script::looks_secret(&name) {
                    "******".to_string()
                } else {
                    value.display()
                };
                emit(RunEvent::Message(format!(":{name} = {shown}")));
                true
            }
            Action::ListVars(vars) => {
                emit(RunEvent::VarList { vars });
                true
            }
            Action::Print(pairs) => {
                // 커서 값은 1회 fetch해 결과 집합으로.
                let mut plain = Vec::new();
                for (n, v) in pairs {
                    if let Value::Cursor(c) = v {
                        let page = self.keep_cursor;
                        match self.emit_cursor(index, item, &n, c, page, emit) {
                            Err(()) => return false,
                            Ok(Some(k)) => {
                                // 상한을 넘는 커서 = 열린 커서로 유지(T-202 · 앞 커서 닫기 = 세션당 1개).
                                self.close_cursor();
                                self.cursor = Some(k);
                            }
                            Ok(None) => {}
                        }
                    } else {
                        plain.push((n, v));
                    }
                }
                if !plain.is_empty() {
                    emit(RunEvent::Print { pairs: plain });
                }
                true
            }
            Action::Connect(spec) => self.connect(&spec, emit),
            Action::Disconnect => {
                self.close_cursor();
                if let Some(mut s) = self.session.take() {
                    let _ = s.commit();
                }
                self.connection = None;
                self.connected_to = None;
                emit(RunEvent::Disconnected);
                true
            }
            Action::Describe(o) => self.describe(index, item, &o, emit),
            Action::Show(w) => {
                let w_up = w.trim().trim_end_matches(';').trim().to_ascii_uppercase();
                // ★ `SHOW CONN|CONNECTION|CONNINFO`(10-01) — 접속 정보는 호스트가 안다(프로필·유형·세션).
                if matches!(w_up.as_str(), "CONN" | "CONNECTION" | "CONNINFO") {
                    emit(RunEvent::ShowConn);
                    return true;
                }
                // `SHOW TABLES|VIEWS|<kind>` — 카탈로그 목록(T-52 · psql `\dt` · sqlite `.tables` 별칭의 종착).
                if !matches!(w_up.as_str(), "USER" | "VARIABLES" | "VAR" | "VARS") {
                    if let Some(kind) = nsql_catalog::ObjectKind::parse(&w_up.to_ascii_lowercase())
                    {
                        return self.show_objects(index, item, kind, emit);
                    }
                }
                // `SHOW ERR[ORS] [종류 이름]` — 마지막으로 만든 저장 단위(또는 지정한 것)의 컴파일 오류(Oracle `ALL_ERRORS`).
                //   만들 때 이미 오류로 보고하지만(`report_compile_errors`) SQL*Plus 스크립트의 관용 줄을 "미지원"으로 남기지 않는다.
                if w_up == "ERR"
                    || w_up == "ERRORS"
                    || w_up.starts_with("ERR ")
                    || w_up.starts_with("ERRORS ")
                {
                    let text = self.show_errors(&w_up);
                    emit(RunEvent::Message(text));
                    return true;
                }
                // `SHOW VARIABLES` = 변수 표를 **결과 표**로(이름 · 타입 · 값 · 층 · 선언 — 비밀 값은 가린다 · docs/63 V2).
                //   GUI에서는 결과 탭 하나로, CLI에서는 표로 보인다(종전 = 글자 한 덩어리).
                if matches!(w_up.as_str(), "VARIABLES" | "VAR" | "VARS") {
                    // ★ 모든 층(사용자 10-07): 같은 이름의 글로벌·탭 줄을 함께 — `Active` = `*`(지금 쓰이는 값) · 빈칸 = 앞 층에 가려짐.
                    let rows: Vec<Vec<String>> = self
                        .engine
                        .vars
                        .iter_all()
                        .into_iter()
                        .map(|(_, v, layer, shadowed)| {
                            vec![
                                v.label.clone(),
                                var_type_label(&v.ty),
                                if v.secret {
                                    "******".to_string()
                                } else {
                                    v.value.display()
                                },
                                layer.word().to_string(),
                                if v.declared { "VAR" } else { "auto" }.to_string(),
                                if shadowed { "" } else { "*" }.to_string(),
                            ]
                        })
                        .collect();
                    emit(RunEvent::ResultSet {
                        index,
                        rs: text_result_set(
                            &["Name", "Type", "Value", "Layer", "Declared", "Active"],
                            rows,
                        ),
                        elapsed: Duration::ZERO,
                        more: false,
                        label: Some("Variables".to_string()),
                    });
                    return true;
                }
                // ★ `SHOW AUTOCOMMIT|AUTO`(사용자 10-29): 이 러너의 플래그(서버 상태를 묻지 않음 · 44 §3).
                if matches!(w_up.as_str(), "AUTOCOMMIT" | "AUTO") {
                    let on = if self.engine.settings.autocommit {
                        "ON"
                    } else {
                        "OFF"
                    };
                    emit(RunEvent::Message(tf(Msg::ShowAutocommit, &[on])));
                    return true;
                }
                // ★ `SHOW VERSION|VER|SERVER`(사용자 10-29): 클라이언트 판 + 서버 판(`nsql_catalog::server_info` · 1 왕복 · 접속 없으면 안내).
                if matches!(w_up.as_str(), "VERSION" | "VER" | "SERVER") {
                    let variant = build_variant();
                    let tag = if variant.is_empty() {
                        String::new()
                    } else {
                        format!(" ({variant})")
                    };
                    let client = tf(Msg::ShowVersionClient, &[env!("CARGO_PKG_VERSION"), &tag]);
                    let server = match self.session.as_mut() {
                        None => t(Msg::NoSession).to_string(),
                        Some(s) => {
                            let d = s.dialect().to_string();
                            match nsql_catalog::server_info(s.as_mut()) {
                                Ok(i) if !i.version.is_empty() => {
                                    let mut detail = format!("{d} {}", i.version);
                                    if !i.edition.is_empty() {
                                        detail.push_str(&format!(" ({})", i.edition));
                                    }
                                    if !i.login.is_empty() {
                                        detail.push_str(&format!(" · user={}", i.login));
                                    }
                                    if !i.current_db.is_empty() {
                                        detail.push_str(&format!(" · db={}", i.current_db));
                                    }
                                    tf(Msg::ShowVersionServer, &[&detail])
                                }
                                Ok(_) => tf(Msg::ShowVersionUnknown, &[&d]),
                                Err(e) => {
                                    tf(Msg::ShowVersionUnknown, &[&format!("{d}: {}", e.message)])
                                }
                            }
                        }
                    };
                    emit(RunEvent::Message(format!("{client}\n{server}")));
                    return true;
                }
                let text = match w_up.as_str() {
                    "USER" => format!("USER = {}", self.connection.clone().unwrap_or_default()),
                    _ => tf(Msg::ShowUnsupported, &[w.trim()]),
                };
                emit(RunEvent::Message(text));
                true
            }
            Action::Spool(target) => self.spool_cmd(index, item, &target, emit),
            Action::Prompt(t) => {
                emit(RunEvent::Message(t));
                true
            }
            Action::RunScript {
                path,
                args,
                relative_to_caller,
            } => {
                let full = self.resolve_script(&path, relative_to_caller);
                self.include(index, item, &full, &args, prompt, emit)
            }
            Action::NeedInput { name } => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: msg_err(tf(Msg::SubstUndefined, &[&name])),
                });
                false
            }
            // `ACCEPT`(T-153): 묻는다 — 빈 답·답 없음 = DEFAULT · 엄격 모드(묻지 않음)에서 DEFAULT도 없으면 오류.
            // 안내 글이 있으면 그것을, 없으면 이름을 호스트의 입력 함수에 넘긴다(CLI = "Enter value for …").
            Action::Accept {
                name,
                prompt: label,
                default,
                hide: _,
            } => {
                let asked = if self.strict {
                    None
                } else {
                    let label = label.filter(|l| !l.trim().is_empty());
                    prompt(label.as_deref().unwrap_or(&name))
                };
                match accept_value(asked, default) {
                    Some(v) => {
                        self.engine.define(&name, &v);
                        true
                    }
                    None => {
                        emit(RunEvent::Error {
                            index,
                            line: item.line,
                            error: msg_err(tf(Msg::SubstUndefined, &[&name])),
                        });
                        false
                    }
                }
            }
            Action::SetOption { name, value } => {
                // ★ 접속 유형(10-01): 서버 옵션이 아니라 호스트 세션 속성 — 값 검증 뒤 이벤트.
                if name == "conntype" {
                    // 정식 · 3자리 약어 · 별칭 → 저장 값(`none`·`dev`·`test`·`prod`)으로 정규화해 넘긴다(사용자 10-01).
                    match nsql_script::ConnEnv::parse(&value) {
                        Ok(env) => {
                            emit(RunEvent::ConnType(
                                env.map_or("none", nsql_script::ConnEnv::as_str).to_string(),
                            ));
                            return true;
                        }
                        Err(_) => {
                            emit(RunEvent::Error {
                                index,
                                line: item.line,
                                error: msg_err(format!(
                                    "CONNTYPE {value}: {}",
                                    nsql_script::ENV_CHOICES
                                )),
                            });
                            return false;
                        }
                    }
                }
                if let Some(s) = self.session.as_mut() {
                    if let Err(e) = s.set_option(&name, &value) {
                        emit(RunEvent::Error {
                            index,
                            line: item.line,
                            error: e,
                        });
                        return false;
                    }
                }
                true
            }
            // 재계획 표식은 계획 루프가 소비한다(여기 오면 홀로 온 것 = 아무것도 안 함).
            Action::Replan => true,
            Action::Nothing(m) => {
                if self.engine.settings.echo {
                    emit(RunEvent::Message(m));
                }
                true
            }
            Action::Error(e) => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: msg_err(e),
                });
                false
            }
        }
    }

    /// `SPOOL …`(T-9) — 해석은 [`parse_spool`], 파일은 호스트가 장착한 [`Spool`]. 시작은 조용히 · OFF/맨몸은 메시지.
    fn spool_cmd(
        &mut self,
        index: usize,
        item: &Item,
        target: &str,
        emit: &mut dyn FnMut(RunEvent),
    ) -> bool {
        let cmd = match parse_spool(target) {
            Ok(c) => c,
            Err(opt) => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: msg_err(tf(Msg::SpoolBadOption, &[&opt])),
                });
                return false;
            }
        };
        let Some(handle) = self.spool.as_ref() else {
            emit(RunEvent::Message(t(Msg::SpoolNotSupported).to_string()));
            return true;
        };
        let r = handle
            .lock()
            .map_err(|e| e.to_string())
            .and_then(|mut s| s.apply(&cmd));
        match r {
            Ok(Some(m)) => {
                emit(RunEvent::Message(m));
                true
            }
            Ok(None) => true,
            Err(m) => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: msg_err(m),
                });
                false
            }
        }
    }

    /// `SHOW TABLES|VIEWS|…` — 현재 스키마의 오브젝트 목록을 결과 집합으로(`nsql cat <kind>`와 같은 컬럼).
    fn show_objects(
        &mut self,
        index: usize,
        item: &Item,
        kind: nsql_catalog::ObjectKind,
        emit: &mut dyn FnMut(RunEvent),
    ) -> bool {
        let Some(session) = self.session.as_mut() else {
            emit(RunEvent::Error {
                index,
                line: item.line,
                error: msg_err(t(Msg::NoSession)),
            });
            return false;
        };
        let schema = nsql_catalog::current_schema(session.as_mut()).unwrap_or_default();
        let started = Instant::now();
        match nsql_catalog::objects(session.as_mut(), &schema, kind) {
            Ok(list) => {
                let rows = list
                    .into_iter()
                    .map(|i| vec![i.schema, i.name, i.status, i.modified, i.extra])
                    .collect();
                emit(RunEvent::ResultSet {
                    index,
                    rs: text_result_set(&["Schema", "Name", "Status", "Modified", "Extra"], rows),
                    elapsed: started.elapsed(),
                    more: false,
                    label: None,
                });
                true
            }
            Err(e) => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: e,
                });
                false
            }
        }
    }

    fn execute(
        &mut self,
        index: usize,
        item: &Item,
        mut prepared: Prepared,
        expect_out: bool,
        emit: &mut dyn FnMut(RunEvent),
    ) -> bool {
        // 새 실행 = 앞 커서 닫기(docs/43 D-70 · 세션당 1개).
        self.close_cursor();
        let keep_cursor = self.keep_cursor;
        let Some(session) = self.session.as_mut() else {
            emit(RunEvent::Error {
                index,
                line: item.line,
                error: msg_err(t(Msg::NoSession)),
            });
            return false;
        };
        // ★ 수동 커밋 모드: 서버가 문장마다 자동 커밋하는 방언이면 트랜잭션을 먼저 연다(T-146).
        // ★ 운영 보류(114 §5 A6): 자동 커밋 모드여도 명시 트랜잭션으로(수동 모드의 시작문 규칙 그대로).
        let autocommit_eff = self.engine.settings.autocommit && self.hold.is_none();
        if let Some(begin) = manual_begin_sql(
            &self.engine.caps,
            autocommit_eff,
            self.tx_open,
            self.tx_user,
            nsql_core::TxControl::of_sql(&item.text),
        ) {
            if let Err(e) = session.execute(&ExecRequest {
                sql: begin.to_string(),
                params: Vec::new(),
            }) {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: e,
                });
                return false;
            }
            self.tx_open = true;
        }
        let names: Vec<String> = prepared.params.iter().map(|p| p.name.clone()).collect();
        let captures = std::mem::take(&mut prepared.captures);
        let mode = prepared.mode;
        let dialect_now = session.dialect();
        let line_offset = prepared.line_offset;
        let started = Instant::now();
        let request = prepared.into_request();
        if !request.params.is_empty() {
            emit(RunEvent::Binds {
                index,
                binds: txlog::TxBind::from_params(&request.params, session.dialect()),
            });
        }
        match session.execute(&request) {
            Ok(mut result) => {
                let elapsed = started.elapsed();
                // 드라이버가 단계를 나누지 않았으면 전체를 Execute로(실행+페치 합산이라 note로 밝힌다).
                let mut timeline = std::mem::take(&mut result.timing);
                if timeline.is_empty() {
                    let rows: u64 = result.result_sets.iter().map(|r| r.rows.len() as u64).sum();
                    let span = timeline.push(Stage::Execute, elapsed);
                    span.rows = (rows > 0).then_some(rows);
                    span.note = Some("execute+fetch".into());
                }
                // OUT 바인드가 없는 방언(PG·MySQL·SQLite·ODBC)만 "1행 결과 = 돌아온 값"으로 읽는다. Oracle·SQL Server는 진짜 OUT이
                // 있으므로 결과 집합은 결과 집합이다 — 종전에는 프로시저의 암묵 결과(1행)가 변수로 빨려 들어가 그리드에서 사라졌다(09-21 실기).
                if !captures.is_empty() {
                    // ★ 요청이 받는 쪽을 명시했다(`EXEC :V := 식` · `EXEC SELECT … INTO :A, :B`) — 한 행을 **자리 순서대로** 받는다.
                    //   0행·여러 행 = Oracle식 오류(D-139 · `vars.into_policy` = first면 첫 행 · 0행은 그대로 두고 알림).
                    match capture_row(&mut result, &captures, self.engine.settings.into_first) {
                        Ok(None) => {}
                        Ok(Some(note)) => emit(RunEvent::Message(note)),
                        Err(msg) => {
                            emit(RunEvent::Error {
                                index,
                                line: item.line,
                                error: msg_err(msg),
                            });
                            self.cursor = None;
                            return false;
                        }
                    }
                } else if expect_out
                    && result.out_params.is_empty()
                    && self.engine.caps.captures_rows()
                {
                    // 호출(`CALL p(…)`)의 OUT = 1행 결과(PG · MySQL) — 열 이름이 곧 변수 이름.
                    absorb_from_result_set(&mut result, &names);
                }
                for m in result.messages.drain(..) {
                    emit(RunEvent::Message(m));
                }
                // ★ 이번 실행이 드라이버 제약에 닿았다(docs/110 §6 · `ExecResult::limits_hit`) → 호스트가 Warning 토스트.
                if !result.limits_hit.is_empty() {
                    let driver = self
                        .session
                        .as_ref()
                        .map_or_else(String::new, |s| s.driver_name());
                    emit(RunEvent::DriverLimits {
                        driver,
                        limits: std::mem::take(&mut result.limits_hit),
                        at_connect: false,
                    });
                }
                let n_sets = result.result_sets.len();
                let max_rows = self.max_rows;
                // ★ T-116 겹침 검사의 첫 꼬리: 마지막 결과 집합에서 상한 안 마지막 행(호스트가 들고 있는 마지막 행과 같다).
                // 결과 집합이 없는 실행(DML·DDL)은 꼬리를 건드리지 않는다 — 페이지 사이에 다른 문장을 돌려도 그 조회의 꼬리는 남는다.
                if n_sets > 0 {
                    self.page_tail = result.result_sets.last().and_then(|r| {
                        let served = if max_rows > 0 {
                            r.rows.len().min(max_rows)
                        } else {
                            r.rows.len()
                        };
                        r.rows
                            .get(served.checked_sub(1)?)
                            .map(|row| (item.text.clone(), served, row.clone()))
                    });
                }
                // 드라이버가 커서를 열어 뒀으면(마지막 결과 집합이 잘림) 유지 모드일 때만 러너가 맡는다 · 아니면 바로 닫는다.
                let pending = result.pending.take();
                let mut kept: Option<OpenCursor> = None;
                if let Some(h) = pending {
                    if keep_cursor && n_sets > 0 {
                        let served = result.result_sets.last().map_or(0, |r| {
                            if max_rows > 0 {
                                r.rows.len().min(max_rows)
                            } else {
                                r.rows.len()
                            }
                        });
                        kept = Some(OpenCursor {
                            handle: h,
                            sql: item.text.clone(),
                            served,
                            deferred_end: None,
                            last_used: Instant::now(),
                        });
                    } else if let Some(s) = self.session.as_mut() {
                        let _ = s.close_cursor(h);
                    }
                }
                let mut labels = std::mem::take(&mut result.result_labels).into_iter();
                let row_count = result.rows_affected.or_else(|| {
                    result.result_sets.last().map(|r| {
                        r.rows
                            .len()
                            .min(if max_rows > 0 { max_rows } else { usize::MAX })
                            as u64
                    })
                });
                self.set_statement_sysvars(row_count, 0, elapsed);
                for (i, rs) in result.result_sets.drain(..).enumerate() {
                    let (rs, trimmed) = trim_rows(rs, max_rows);
                    // `COLUMN … NEW_VALUE`: 보여 주는 결과의 마지막 행(등록이 없으면 비용 0).
                    self.engine.note_result(&rs.columns, rs.rows.last());
                    let more = trimmed || (i + 1 == n_sets && pending.is_some());
                    emit(RunEvent::ResultSet {
                        index,
                        rs,
                        elapsed,
                        more,
                        // 드라이버가 풀어 낸 커서의 이름(PostgreSQL refcursor · T-150) — 결과 탭 제목.
                        label: labels.next().flatten(),
                    });
                }
                if n_sets == 0 || result.rows_affected.is_some() {
                    emit(RunEvent::Done {
                        index,
                        rows_affected: result.rows_affected,
                        elapsed,
                        delivery: nsql_script::delivery_tag(mode, names.len(), Some(dialect_now)),
                    });
                }
                self.hold_note(index, item, result.rows_affected);
                // ★ `USE db` 성공(10-01 · 101 §3): 세션의 현재 DB가 바뀌었다 — 호스트에 알린다(파싱만 · 왕복 0).
                if let Some(d) = self.dialect() {
                    if let Some(db) = nsql_catalog::context_switch_name(d, &item.text) {
                        emit(RunEvent::DbChanged(db));
                    }
                }
                let printed = self.engine.absorb(&result);
                self.flush_diagnostics(emit);
                // ★ 돌아온 REF CURSOR는 바로 결과로 보여 준다(`VAR rc REFCURSOR` + `EXEC proc(:rc)` → 그리드 · 커서가 둘 이상이면
                //   각각 · 설정 `run.cursor_autoshow` · SQL*Plus의 `SET AUTOPRINT ON`과 같은 효과를 커서에만). 끄면 `PRINT rc`로.
                let cursors: Vec<(String, nsql_core::CursorId)> = result
                    .out_params
                    .iter()
                    .filter_map(|(n, v)| match v {
                        Value::Cursor(c) => Some((n.clone(), *c)),
                        _ => None,
                    })
                    .collect();
                let mut cursor_ok = true;
                if self.auto_cursor {
                    // 세션당 열린 커서 1개 → **마지막** 커서만 페이지(상한 + 이어 받기)로 남기고 앞 커서는 전부 읽는다(T-202).
                    let last = cursors.len().saturating_sub(1);
                    for (i, (n, c)) in cursors.iter().enumerate() {
                        match self.emit_cursor(index, item, n, *c, i == last && keep_cursor, emit) {
                            Err(()) => cursor_ok = false,
                            Ok(Some(k)) => {
                                if let (Some(old), Some(s)) = (kept.take(), self.session.as_mut()) {
                                    let _ = s.close_cursor(old.handle);
                                }
                                kept = Some(k);
                            }
                            Ok(None) => {}
                        }
                    }
                }
                if let Some(pairs) = printed {
                    // 자동 표시한 커서는 값 목록에서 뺀다(`<refcursor #n>` 글자 대신 그리드로 이미 나갔다).
                    let shown = self.auto_cursor;
                    let pairs: Vec<(String, Value)> = pairs
                        .into_iter()
                        .filter(|(_, v)| !(shown && matches!(v, Value::Cursor(_))))
                        .collect();
                    if !pairs.is_empty() {
                        emit(RunEvent::Print { pairs });
                    }
                }
                if !cursor_ok {
                    self.cursor = kept;
                    return false;
                }
                if autocommit_eff {
                    let needed = self
                        .engine
                        .caps
                        .autocommit_needs_commit(self.tx_open, self.tx_user);
                    match kept.as_mut() {
                        // 서버가 이미 커밋했다(열린 트랜잭션 없음) — 보낼 것이 없다.
                        _ if !needed => {}
                        // 커서가 살아 있는 동안은 커밋을 미룬다(PG WITHOUT HOLD 커서 보호 · 닫힐 때 커밋).
                        Some(c) => c.deferred_end = Some(TxEnd::Commit),
                        None => {
                            if let Some(s) = self.session.as_mut() {
                                let t = Instant::now();
                                let _ = s.commit();
                                timeline.push(Stage::Commit, t.elapsed());
                            }
                            self.tx_open = false;
                            self.tx_stmts = 0;
                        }
                    }
                } else if self.read_end_due(&item.text, result.rows_affected) {
                    // ★ docs/56 L1: 수동 모드라도 변경이 없던 트랜잭션은 결과를 다 받은 뒤 끝낸다(ROLLBACK = 부작용 0).
                    let deferred = kept.is_some();
                    match kept.as_mut() {
                        Some(c) => c.deferred_end = Some(TxEnd::Rollback),
                        None => {
                            if let Some(s) = self.session.as_mut() {
                                let t = Instant::now();
                                let _ = s.rollback();
                                let span = timeline.push(Stage::Commit, t.elapsed());
                                span.note = Some("read tx end (rollback)".into());
                            }
                            self.tx_open = false;
                            self.tx_stmts = 0;
                        }
                    }
                    emit(RunEvent::ReadTxEnded { index, deferred });
                }
                self.cursor = kept;
                emit(RunEvent::Timing { index, timeline });
                // ★ 컴파일 결과(사용자 09-15 "객체 생성·수정"): Oracle은 CREATE가 성공해도 INVALID일 수 있다 —
                // SQL*Plus의 "Warning: … created with compilation errors" + SHOW ERRORS를 한 번에.
                self.report_compile_errors(index, item, emit)
            }
            Err(mut e) => {
                let code = e.code.unwrap_or(-1);
                self.set_statement_sysvars(None, code, started.elapsed());
                // ★ T-371: 위치는 **보낸 문장** 기준 → 원문 문장(`item.text`) 기준으로. 바인드 재작성(`:x` → `$1`/`@x`)·프리펜드·
                //   치환으로 그 자리 앞이 달라졌으면 맞출 수 없으니 버린다(호스트 = 문장 시작 줄로 폴백).
                e.position = e
                    .position
                    .and_then(|p| map_error_position(&request.sql, &item.text, p));
                if mode == PrepareMode::DeclarePrepend {
                    e.message
                        .push_str(&format!(" (프리펜드 {line_offset}줄 보정 필요)"));
                }
                // ★ 실패한 실행이 드라이버 제약 때문이면(메시지 꼬리표 `driver limitation: …` · docs/110 §6) 오류와 함께 제약 안내도.
                if let Some(l) = nsql_core::DriverLimit::find_in(&e.message) {
                    emit(RunEvent::DriverLimits {
                        driver: self
                            .session
                            .as_ref()
                            .map_or_else(String::new, |s| s.driver_name()),
                        limits: vec![l],
                        at_connect: false,
                    });
                }
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: e,
                });
                false
            }
        }
    }
}

/// 페치 상한 적용 — 상한 초과분을 잘라 `(결과, 더 있음)`.
fn trim_rows(mut rs: ResultSet, max_rows: usize) -> (ResultSet, bool) {
    if max_rows > 0 && rs.rows.len() > max_rows {
        rs.rows.truncate(max_rows);
        (rs, true)
    } else {
        (rs, false)
    }
}

impl Runner {
    /// 드라이버에 페치 상한을 알린다 — 접속 직후 · 상한 변경 시. 커서를 못 여는 드라이버에는 **+1**(초과 여부 판정용 · 러너가 자른다),
    /// 커서 지원 드라이버에는 상한 그대로(드라이버가 한 행을 엿봐 `pending`으로 알린다). 왕복당 행수(`fetch_size`)도 함께.
    fn push_max_rows(&mut self) {
        let n = self.max_rows;
        let fetch_size = self.fetch_size;
        let sink = self.message_sink.clone();
        if let Some(s) = self.session.as_mut() {
            let v = if n == 0 || s.cursor_supported() {
                n
            } else {
                n + 1
            };
            let _ = s.set_option("max_rows", &v.to_string());
            let _ = s.set_option(
                "refcursor_expand",
                if self.refcursor_expand { "on" } else { "off" },
            );
            if fetch_size > 0 {
                let _ = s.set_option("fetch_size", &fetch_size.to_string());
            }
            s.set_message_sink(sink);
        }
    }

    /// REF CURSOR 변수 하나를 **한 번** 받아 결과 집합으로 낸다(라벨 = 변수 이름) · 변수는 비운다(커서는 1회용).
    /// `PRINT rc`와 실행 뒤 자동 표시([`Runner::auto_cursor`])가 같이 쓴다. 실패 = 오류 이벤트 + `false`.
    fn emit_cursor(
        &mut self,
        index: usize,
        item: &Item,
        name: &str,
        cursor: nsql_core::CursorId,
        page: bool,
        emit: &mut dyn FnMut(RunEvent),
    ) -> Result<Option<OpenCursor>, ()> {
        let started = Instant::now();
        // 페이지 = 상한(`max_rows`)까지만 읽고 커서를 열어 둔다(T-202 · 이어 받기 = 호스트의 `fetch_page`/`fetch_all` ·
        // 커서 결과는 OFFSET 재질의가 없으므로 커서가 닫히면 끝) · 아니면 전부 읽는다(종전 · 앞 커서 · `keep_cursor` 끔).
        // ★ 커서를 유지할 수 없으면(드라이버 미지원 · `keep_cursor` 끔 · 앞 커서 · 드라이버가 전부 읽음) 줄 수 제한을 **무시하고
        //   전부** 읽는다 — 잘라 놓고 이어 받을 길이 없는 것보다 낫다(사용자 09-24) · 상한을 넘었으면 눈에 띄는 경고(`Warning`).
        let limit = self.max_rows;
        let max = if page && limit > 0 && self.cursor_supported() {
            limit
        } else {
            0
        };
        match self
            .session
            .as_mut()
            .map(|s| s.fetch_cursor_page(cursor, max))
        {
            Some(Ok((rs, handle))) => {
                emit(RunEvent::Message(format!("PRINT {name} (refcursor)")));
                let served = rs.rows.len();
                if limit > 0 && handle.is_none() && served > limit {
                    emit(RunEvent::Warning(tf(
                        Msg::RunCursorFullFetch,
                        &[name, &served.to_string(), &limit.to_string()],
                    )));
                }
                let kept = handle.map(|h| OpenCursor {
                    handle: h,
                    sql: item.text.clone(),
                    served,
                    deferred_end: None,
                    last_used: Instant::now(),
                });
                emit(RunEvent::ResultSet {
                    index,
                    rs,
                    elapsed: started.elapsed(),
                    more: kept.is_some(),
                    label: Some(name.to_string()),
                });
                self.engine.vars.assign(name, Value::Null);
                Ok(kept)
            }
            Some(Err(e)) => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: e,
                });
                Err(())
            }
            None => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: msg_err(t(Msg::NoSession)),
                });
                Err(())
            }
        }
    }

    /// `SHOW ERRORS [종류 이름]`의 본문 — 대상이 없거나 Oracle이 아니면 "오류 없음".
    fn show_errors(&mut self, w_up: &str) -> String {
        let mut words = w_up.split_whitespace().skip(1);
        let named = match (words.next(), words.next(), words.next()) {
            // `PACKAGE BODY name` 같은 두 낱말 종류.
            (Some(k1), Some(k2), Some(n)) => Some((format!("{k1} {k2}"), n.to_string())),
            (Some(k), Some(n), None) => Some((k.to_string(), n.to_string())),
            _ => None,
        };
        if !self.engine.caps.compile_errors {
            return t(Msg::ShowNoErrors).to_string();
        }
        let Some(session) = self.session.as_mut() else {
            return t(Msg::NoSession).to_string();
        };
        let (kind, owner, name) = match named {
            Some((k, n)) => {
                let (owner, name) = match n.split_once('.') {
                    Some((o, nm)) => (o.to_string(), nm.to_string()),
                    None => (
                        nsql_catalog::current_schema(session.as_mut()).unwrap_or_default(),
                        n,
                    ),
                };
                (k, owner, name)
            }
            None => match self.last_unit.clone() {
                Some(u) => u,
                None => return t(Msg::ShowNoErrors).to_string(),
            },
        };
        match nsql_catalog::compile_errors(session.as_mut(), &owner, &name) {
            Ok(errs) if !errs.is_empty() => {
                let mut out = tf(Msg::ShowErrorsFor, &[&format!("{kind} {owner}.{name}")]);
                for e in &errs {
                    out.push('\n');
                    out.push_str(&e.to_string());
                }
                out
            }
            Ok(_) => t(Msg::ShowNoErrors).to_string(),
            Err(e) => e.to_string(),
        }
    }

    /// `CREATE [OR REPLACE] PROCEDURE|FUNCTION|PACKAGE|TRIGGER|TYPE|VIEW …`가 성공한 뒤 Oracle `ALL_ERRORS`를 읽어
    /// 오류가 있으면 `RunEvent::Error`(줄/열/메시지 목록) — 없으면 true.
    fn report_compile_errors(
        &mut self,
        index: usize,
        item: &Item,
        emit: &mut dyn FnMut(RunEvent),
    ) -> bool {
        if !self.engine.caps.compile_errors {
            return true;
        }
        if !matches!(
            item.kind,
            ItemKind::Sql(SqlKind::Block) | ItemKind::Sql(SqlKind::Ddl)
        ) {
            return true;
        }
        let Some((kind, schema, name)) = nsql_catalog::parse_create_header(&item.text) else {
            return true;
        };
        if !kind.has_source() {
            return true;
        }
        let Some(session) = self.session.as_mut() else {
            return true;
        };
        let owner = match schema {
            Some(s) => s.to_ascii_uppercase(),
            None => nsql_catalog::current_schema(session.as_mut()).unwrap_or_default(),
        };
        let quoted = item.text.contains(&format!("\"{name}\""));
        let name_up = if quoted {
            name
        } else {
            name.to_ascii_uppercase()
        };
        self.last_unit = Some((
            kind.code().to_ascii_uppercase(),
            owner.clone(),
            name_up.clone(),
        ));
        match nsql_catalog::compile_errors(session.as_mut(), &owner, &name_up) {
            // ★ 오류 없음 = "compiled" 한 줄(Output 탭 · 09-30 · SQL*Plus "Procedure created."에 해당).
            Ok(errs) if errs.is_empty() => {
                emit(RunEvent::Message(tf(
                    Msg::OutCompiledOk,
                    &[
                        &kind.code().to_ascii_uppercase(),
                        &format!("{owner}.{name_up}"),
                    ],
                )));
                true
            }
            Ok(errs) => {
                let mut msg = format!(
                    "Warning: {} {owner}.{name_up} created with compilation errors",
                    kind.code().to_ascii_uppercase()
                );
                for e in &errs {
                    msg.push('\n');
                    msg.push_str(&e.to_string());
                }
                let first = errs.first().map(|e| e.line as usize);
                emit(RunEvent::Error {
                    index,
                    line: item.line + first.unwrap_or(1).saturating_sub(1),
                    error: DbError {
                        code: None,
                        message: msg,
                        position: None,
                    },
                });
                false
            }
            _ => true,
        }
    }

    /// `DESC[RIBE] [schema.]object` — 카탈로그 컬럼 목록을 결과 집합으로.
    fn describe(
        &mut self,
        index: usize,
        item: &Item,
        object: &str,
        emit: &mut dyn FnMut(RunEvent),
    ) -> bool {
        let Some(session) = self.session.as_mut() else {
            emit(RunEvent::Error {
                index,
                line: item.line,
                error: msg_err(t(Msg::NoSession)),
            });
            return false;
        };
        let clean = |s: &str| {
            s.trim_matches(|c| c == '"' || c == '[' || c == ']' || c == '`')
                .to_string()
        };
        let (schema, name) = match object.trim().split_once('.') {
            Some((s, n)) => (clean(s), clean(n)),
            None => {
                let cur = nsql_catalog::current_schema(session.as_mut()).unwrap_or_default();
                (cur, clean(object.trim()))
            }
        };
        // Oracle·MSSQL은 대소문자 무관 이름을 저장 규칙대로(Oracle 대문자 · PG 소문자).
        let name = if object.contains('"') {
            name
        } else {
            session.caps().fold_ident(&name)
        };
        let started = Instant::now();
        match nsql_catalog::columns(session.as_mut(), &schema, &name) {
            Ok(cols) if !cols.is_empty() => {
                let rs = ResultSet {
                    columns: ["Name", "Type", "Nullable", "Default"]
                        .iter()
                        .map(|n| Column {
                            name: (*n).to_string(),
                            type_name: String::new(),
                        })
                        .collect(),
                    rows: cols
                        .iter()
                        .map(|c| {
                            vec![
                                Value::Str(c.name.clone()),
                                Value::Str(c.data_type.clone()),
                                Value::Str(if c.nullable { "Y".into() } else { "N".into() }),
                                Value::Str(c.default.clone()),
                            ]
                        })
                        .collect(),
                };
                emit(RunEvent::ResultSet {
                    index,
                    rs,
                    elapsed: started.elapsed(),
                    more: false,
                    label: None,
                });
                true
            }
            Ok(_) => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: msg_err(format!(
                        "DESCRIBE {object}: object not found ({schema}.{name})"
                    )),
                });
                false
            }
            Err(e) => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: e,
                });
                false
            }
        }
    }
}

/// OUT 파라미터가 없는 방언(SQLite·PG·MySQL·ODBC): `expect_out`인 실행의 **마지막 결과 집합이 1행**이면
/// 그 컬럼(별칭)이 곧 변수다 — `EXEC :V := expr` → `SELECT expr AS "V"` 경로. 결과 집합은 그리드에 보이지 않게 뺀다
/// (SQL*Plus에서 EXEC는 결과 집합을 내지 않고 `PRINT`가 값을 보여 준다).
fn absorb_from_result_set(result: &mut ExecResult, _names: &[String]) {
    let Some(rs) = result.result_sets.last() else {
        return;
    };
    if rs.rows.len() != 1 || rs.columns.is_empty() {
        return;
    }
    let outs: Vec<(String, Value)> = rs
        .columns
        .iter()
        .zip(rs.rows[0].iter())
        .map(|(c, v)| (c.name.to_ascii_uppercase(), v.clone()))
        .collect();
    result.out_params = outs;
    result.result_sets.pop();
    if result.rows_affected.is_none() {
        result.rows_affected = Some(0);
    }
}

/// 진단에서 암묵 변수 이름만(`:이름` 표기 · 나온 순서 · 중복 없이).
fn implicit_names(diags: Vec<nsql_script::Diagnostic>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in diags {
        if let nsql_script::Diagnostic::ImplicitVariable(n) = d {
            let n = format!(":{n}");
            if !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

/// SQL Server 호출에서 **`OUTPUT`을 보충할 바인드**(T-151 · 순수): 서명이 OUTPUT이라 한 인자 자리에 바인드를 주고도 `OUTPUT`을
/// 쓰지 않은 것. 이름 표기(`@b = :X`)는 이름으로 · 아니면 자리로 맞춘다.
fn output_binds(
    shape: &nsql_script::CallShape,
    all: &[nsql_catalog::RoutineArg],
    overload: &str,
) -> Vec<String> {
    shape
        .args
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.output)
        .filter_map(|(i, a)| {
            let bind = a.bind.as_deref()?;
            let arg = all.iter().find(|p| {
                p.overload == overload
                    && match a.named.as_deref() {
                        Some(n) => p.name == n,
                        None => p.position == i as i64 + 1,
                    }
            })?;
            arg.in_out.contains("OUT").then(|| bind.to_string())
        })
        .collect()
}

/// `ACCEPT`의 값(순수): 답이 있고 비어 있지 않으면 그 답 · 아니면 DEFAULT · DEFAULT도 없으면 빈 답은 빈 글 · 답 없음은 `None`(오류).
fn accept_value(asked: Option<String>, default: Option<String>) -> Option<String> {
    match (asked, default) {
        (Some(a), _) if !a.is_empty() => Some(a),
        (_, Some(d)) => Some(d),
        (Some(a), None) => Some(a),
        (None, None) => None,
    }
}

/// 호출의 **받는 바인드 목록**(T-151 · 순수): 서명에서 넘긴 인자 수가 맞는 첫 오버로드를 골라, 그 OUT/INOUT 인자마다
/// "그 자리에 넘긴 바인드"(이름 표기 `이름 => :X`면 이름으로 · 아니면 자리로)를 적는다 — 바인드가 아닌 자리는 빈 글(건너뛴다).
/// 받을 바인드가 하나도 없거나 서명을 모르면 `None`(= 종전 동작).
fn call_captures(
    shape: &nsql_script::CallShape,
    all: &[nsql_catalog::RoutineArg],
) -> Option<Vec<String>> {
    let passed = shape.args.len();
    let mut overloads: Vec<&str> = all.iter().map(|a| a.overload.as_str()).collect();
    overloads.dedup();
    let ov = overloads.into_iter().find(|ov| {
        let params: Vec<&nsql_catalog::RoutineArg> = all
            .iter()
            .filter(|a| a.overload == *ov && a.position >= 1)
            .collect();
        passed <= params.len()
            && shape.args.iter().all(|a| {
                a.named
                    .as_deref()
                    .is_none_or(|n| params.iter().any(|p| p.name == n))
            })
    })?;
    let mut out: Vec<String> = Vec::new();
    for p in all.iter().filter(|a| a.overload == ov && a.position >= 1) {
        if !p.in_out.contains("OUT") {
            continue;
        }
        let arg = shape
            .args
            .iter()
            .enumerate()
            .find(|(i, a)| match a.named.as_deref() {
                Some(n) => n == p.name,
                None => *i as i64 + 1 == p.position,
            })
            .map(|(_, a)| a);
        out.push(arg.and_then(|a| a.bind.clone()).unwrap_or_default());
    }
    out.iter().any(|b| !b.is_empty()).then_some(out)
}

/// 마지막 결과 집합의 한 행을 `captures` 이름들로 받는다(자리 순서) — 받은 집합은 결과에서 뺀다(EXEC는 그리드를 내지 않는다).
/// `Ok(Some(안내))` = 값을 바꾸지 않았다(first 정책의 0행) · `Err` = Oracle식 오류(0행 = ORA-01403 · 여러 행 = ORA-01422에 해당).
fn capture_row(
    result: &mut ExecResult,
    captures: &[String],
    first: bool,
) -> Result<Option<String>, String> {
    let Some(rs) = result.result_sets.pop() else {
        return Err(t(Msg::ErrIntoNoData).to_string());
    };
    if rs.rows.is_empty() {
        return if first {
            Ok(Some(t(Msg::ErrIntoNoData).to_string()))
        } else {
            Err(t(Msg::ErrIntoNoData).to_string())
        };
    }
    if rs.rows.len() > 1 && !first {
        return Err(tf(Msg::ErrIntoTooMany, &[&rs.rows.len().to_string()]));
    }
    let row = &rs.rows[0];
    if row.len() < captures.len() {
        return Err(tf(
            Msg::ErrIntoColumns,
            &[&captures.len().to_string(), &row.len().to_string()],
        ));
    }
    // 빈 이름 = 받지 않는 자리(호출의 OUT 인자에 바인드가 아닌 값을 넘긴 곳 · T-151).
    result.out_params = captures
        .iter()
        .cloned()
        .zip(row.iter().cloned())
        .filter(|(n, _)| !n.is_empty())
        .collect();
    if result.rows_affected.is_none() {
        result.rows_affected = Some(0);
    }
    Ok(None)
}

/// 문자열만 담는 결과 집합(카탈로그 목록용).
fn text_result_set(cols: &[&str], rows: Vec<Vec<String>>) -> ResultSet {
    ResultSet {
        columns: cols
            .iter()
            .map(|n| Column {
                name: (*n).to_string(),
                type_name: String::new(),
            })
            .collect(),
        rows: rows
            .into_iter()
            .map(|r| r.into_iter().map(Value::Str).collect())
            .collect(),
    }
}

/// 같은 문장인가 — 앞뒤 공백·끝 `;`만 무시(호스트가 보관한 원문과 러너의 `Item.text` 비교).
fn same_sql(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().trim_end_matches(';').trim_end().to_string();
    norm(a) == norm(b)
}

fn msg_err(m: impl Into<String>) -> DbError {
    DbError {
        code: None,
        message: m.into(),
        position: None,
    }
}

/// ★ 본문 전체를 **한 항목**으로(09-30 · [`Runner::run_whole`]): 끝의 종결 줄(`/` · `GO`)과 마지막 `;`(PL/SQL 블록은 `END;`의
/// `;`를 남긴다 · 블록이 아니면 뗀다)을 정리하고 종류는 첫 단어로 판정한다. 순수 함수 — 시험 대상.
#[must_use]
pub fn whole_item(src: &str) -> Item {
    let mut text = src.trim_end().to_string();
    // 종결 줄(`/` · `GO`)은 여러 개라도 전부(빈 줄 사이에 있어도).
    loop {
        let t = text.trim_end();
        let Some((head, last)) = t.rsplit_once('\n') else {
            break;
        };
        let l = last.trim();
        if l == "/" || l.eq_ignore_ascii_case("GO") {
            text = head.to_string();
        } else {
            break;
        }
    }
    let mut text = text.trim().to_string();
    let kind = nsql_script::classify_sql(&text);
    if !matches!(kind, SqlKind::Block) {
        if let Some(t) = text.strip_suffix(';') {
            text = t.trim_end().to_string();
        }
    }
    Item {
        kind: ItemKind::Sql(kind),
        text,
        span: 0..src.len(),
        line: 1,
    }
}

/// `PRINT` 한 줄 문구 `A = 1 · B = 'x'` — 비밀 이름(D-140)은 `******`. GUI 로그·CLI 공용.
pub fn print_pairs_text(pairs: &[(String, Value)]) -> String {
    pairs
        .iter()
        .map(|(n, v)| {
            let bare = n.trim_start_matches([':', '&', '@']);
            if nsql_script::looks_secret(bare) {
                format!("{n} = ******")
            } else {
                format!("{n} = {}", v.display())
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// ★ T-266 서버 메시지 등급(10-30 · 사용자 결정): 드라이버가 글자로만 넘기는 메시지(`RunEvent::Message`)의 등급 — 호스트(Output 탭 · CLI)가
/// 색·표식을 고른다. 판정 = 등급 접두(PostgreSQL `RAISE`: `WARNING:` · `NOTICE:` · `INFO:` · `DEBUG:` · `LOG:` · `EXCEPTION`/`ERROR`/`FATAL`/`PANIC`) ·
/// 그 밖 = Info(DBMS_OUTPUT · T-SQL PRINT · RAISERROR 심각도 ≤ 10은 드라이버가 등급을 주지 않아 Info = 한계 · docs/100 §3-3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageLevel {
    Info,
    Warning,
    Error,
}

/// 메시지 글의 등급(접두 · 대소문자 무시 · 접두 뒤는 `:` 또는 공백).
#[must_use]
pub fn message_level(text: &str) -> MessageLevel {
    let head: String = text
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    let rest = text.trim_start()[head.len()..].chars().next();
    if !matches!(rest, Some(':') | Some(' ')) {
        return MessageLevel::Info;
    }
    match head.as_str() {
        "WARNING" | "WARN" => MessageLevel::Warning,
        "EXCEPTION" | "ERROR" | "FATAL" | "PANIC" => MessageLevel::Error,
        _ => MessageLevel::Info,
    }
}

#[cfg(test)]
mod msg_level_tests {
    use super::{message_level, MessageLevel};

    /// PG RAISE 접두 · 대소문자 · 접두 뒤 구분자 없으면 Info · 보통 글 = Info.
    #[test]
    fn levels_from_prefix() {
        assert_eq!(message_level("WARNING: low disk"), MessageLevel::Warning);
        assert_eq!(message_level("warning  x"), MessageLevel::Warning);
        assert_eq!(message_level("NOTICE: hello"), MessageLevel::Info);
        assert_eq!(message_level("EXCEPTION: boom"), MessageLevel::Error);
        assert_eq!(message_level("ERROR: x"), MessageLevel::Error);
        assert_eq!(
            message_level("WARNINGS are fine"),
            MessageLevel::Info,
            "접두 뒤 구분자 없음"
        );
        assert_eq!(message_level("Hello world"), MessageLevel::Info);
        assert_eq!(message_level(""), MessageLevel::Info);
    }
}

/// 인자 없는 `VARIABLE`의 한 줄 문구 `A NUMBER · B VARCHAR2(40)`.
pub fn var_list_text(vars: &[(String, nsql_core::VarType)]) -> String {
    vars.iter()
        .map(|(n, t)| format!("{n} {}", t.sql_name()))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// 이 항목이 **서버로 가는가** — SQL · `EXEC` · `CONNECT`/`DISCONNECT` · `DESCRIBE` · `SHOW`(`SHOW VARIABLES`는 제외)는 간다.
/// 나머지 스크립트 명령(`PRINT` · `VARIABLE` · `DEFINE` · `SET` · `ACCEPT` · `PROMPT` · `@파일` …)은 클라이언트 안에서 끝난다.
/// (`EXEC :V := 리터럴`은 실제로는 왕복 0이지만 계획 전에는 알 수 없어 `EXEC`를 서버행으로 본다.)
pub fn goes_to_server(item: &Item) -> bool {
    match &item.kind {
        ItemKind::Sql(_) => true,
        ItemKind::Invalid(_) => false,
        ItemKind::Command(c) => match c {
            // ★ `EXEC :V := 리터럴`은 클라이언트 로컬 대입(DB 왕복 0 · `nsql_script::local_assign` · 139차 win) — 전송 로그 ·
            //   트랜잭션 로그(Running 잔류) · 세션 상태 표식(docs/52 §6-4 오탐)의 대상이 아니다. 우변이 식이면 서버행 그대로.
            nsql_script::Command::Exec { body } => nsql_script::local_assign(body).is_none(),
            nsql_script::Command::Connect(_)
            | nsql_script::Command::Disconnect
            | nsql_script::Command::Describe { .. } => true,
            nsql_script::Command::Show { what } => !what.trim().eq_ignore_ascii_case("VARIABLES"),
            _ => false,
        },
    }
}

fn summary(item: &Item) -> String {
    match &item.kind {
        ItemKind::Command(c) => format!("{c:?}")
            .split(|c: char| !c.is_alphanumeric())
            .next()
            .unwrap_or("Command")
            .to_string(),
        ItemKind::Sql(k) => format!("{k:?}"),
        ItemKind::Invalid(_) => "Invalid".into(),
    }
}

/// ★ 빌드 변형 이름(`SHOW VERSION` · 도움말 · 10-29): 호스트가 기동 때 한 번 알려 준다(`full` = 내장 드라이버 전부 · `lite` =
///   Oracle OCI만 · D-350·D-352) — 기능 플래그는 호스트 크레이트에 있으므로 여기서는 저장만. 모르면 빈 글.
static BUILD_VARIANT: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

pub fn set_build_variant(v: &'static str) {
    let _ = BUILD_VARIANT.set(v);
}

pub fn build_variant() -> &'static str {
    BUILD_VARIANT.get().copied().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-153: 입력 창이 고른 타입은 선언 + 대입(타입 고정) · `auto`/없음 = 종전 대입.
    #[test]
    fn apply_inputs_declares_with_chosen_type() {
        use nsql_core::{Value, VarType};
        let mut r = runner();
        r.apply_inputs(vec![
            (
                nsql_script::InputKind::Bind,
                "A".into(),
                "42".into(),
                Some(VarType::Varchar2(4000)),
            ),
            (
                nsql_script::InputKind::Bind,
                "B".into(),
                "42".into(),
                Some(VarType::Auto),
            ),
            (
                nsql_script::InputKind::Bind,
                "C".into(),
                "yes".into(),
                Some(VarType::Boolean),
            ),
        ]);
        let a = r.engine.vars.get("A").expect("A");
        assert_eq!(
            (a.ty.clone(), a.value.clone(), a.auto_ty),
            (VarType::Varchar2(4000), Value::Str("42".into()), false)
        );
        let b = r.engine.vars.get("B").expect("B");
        assert_eq!((b.value.clone(), b.auto_ty), (Value::Int(42), true));
        assert_eq!(
            r.engine.vars.get("C").map(|v| v.value.clone()),
            Some(Value::Bool(true))
        );
    }
    use nsql_driver_sqlite::SqliteSession;

    /// ★ 객체 단위 실행(09-30): 종결 줄 제거 · 블록은 `END;` 유지 · DDL/뷰는 끝 `;` 제거 · `GO`(T-SQL) · 여러 종결 줄.
    #[test]
    fn whole_item_strips_terminators_and_keeps_block() {
        let src = "CREATE OR REPLACE PROCEDURE p AS\nBEGIN\n  NULL;\nEND p;\n/\n";
        let it = whole_item(src);
        assert!(matches!(it.kind, ItemKind::Sql(SqlKind::Block)));
        assert_eq!(
            it.text,
            "CREATE OR REPLACE PROCEDURE p AS\nBEGIN\n  NULL;\nEND p;"
        );
        assert_eq!(it.line, 1);
        let v = whole_item("CREATE OR REPLACE VIEW v AS\nSELECT 1 FROM dual;\n");
        assert!(matches!(v.kind, ItemKind::Sql(SqlKind::Ddl)));
        assert_eq!(v.text, "CREATE OR REPLACE VIEW v AS\nSELECT 1 FROM dual");
        let g =
            whole_item("CREATE OR ALTER PROCEDURE dbo.p AS\nBEGIN\n  SELECT 1;\nEND\nGO\n\nGO\n");
        assert!(matches!(g.kind, ItemKind::Sql(SqlKind::Block)));
        assert!(g.text.ends_with("END"), "{}", g.text);
        // 패키지 스펙+본문처럼 `/`가 가운데 있으면 그대로(합치지 않는 게 아니라 호출자가 분할 1항목일 때만 이 길을 탄다).
        let two =
            "CREATE OR REPLACE PACKAGE a AS END;\n/\nCREATE OR REPLACE PACKAGE BODY a AS END;\n/\n";
        assert!(whole_item(two).text.contains("\n/\n"));
    }

    /// ★ `CONNTYPE`·`SHOW CONN`(10-01): 서버로 가지 않고 이벤트로 호스트에.
    #[test]
    fn conntype_and_show_conn_emit_events() {
        let mut r = runner();
        let mut evs = Vec::new();
        let mut prompt = |_: &str| None;
        r.run_script(
            "CONNTYPE Production\nSHOW CONN\nSET CONNTYPE non\nCONNTYPE stg",
            &mut prompt,
            &mut |e| evs.push(e),
        );
        assert!(
            evs.iter()
                .any(|e| matches!(e, RunEvent::ConnType(v) if v == "prod")),
            "{evs:?}"
        );
        assert!(evs
            .iter()
            .any(|e| matches!(e, RunEvent::ConnType(v) if v == "none")));
        assert!(evs
            .iter()
            .any(|e| matches!(e, RunEvent::ConnType(v) if v == "test")));
        assert!(evs.iter().any(|e| matches!(e, RunEvent::ShowConn)));
        let mut evs2 = Vec::new();
        r.run_script("CONNTYPE bogus", &mut prompt, &mut |e| evs2.push(e));
        assert!(
            evs2.iter().any(|e| matches!(e, RunEvent::Error { .. })),
            "잘못된 값 = 오류"
        );
    }

    fn runner() -> Runner {
        let opener: Opener = Box::new(|spec: &ConnectSpec| {
            let s = SqliteSession::open(spec.database.as_deref().unwrap_or(":memory:"))?;
            Ok(Box::new(s) as Box<dyn Session>)
        });
        Runner::new(Dialect::Sqlite, opener).with_session(
            Box::new(SqliteSession::open(":memory:").unwrap()),
            "sqlite :memory:",
        )
    }

    /// 같은 서버·계정에 다시 `CONNECT`(자격을 바꿔 붙기 · 사용자 09-21): 기존 접속을 **먼저 닫는다** → 새 접속이 실패하면
    /// 옛 세션이 살아남지 않고 `Disconnected`를 알린다 · 성공하면 `Connected`만. 다른 서버로의 실패한 `CONNECT`는 옛 접속 유지.
    #[test]
    fn reconnect_to_same_target_closes_the_old_session_first() {
        use std::cell::Cell;
        use std::rc::Rc;
        let fail = Rc::new(Cell::new(false));
        let f = fail.clone();
        let opener: Opener = Box::new(move |_spec: &ConnectSpec| {
            if f.get() {
                return Err(DbError {
                    code: Some(1005),
                    message: "ORA-01005: null password given".into(),
                    position: None,
                });
            }
            Ok(Box::new(SqliteSession::open(":memory:")?) as Box<dyn Session>)
        });
        let mut r = Runner::new(Dialect::Oracle, opener);
        let a = ConnectSpec::parse("oracle://u@h:1521/svc").unwrap();
        let mut ev = Vec::new();
        assert!(r.connect(&a, &mut |e| ev.push(e)));
        // 같은 대상 · 다른 자격 · 성공 = 끊김 알림 없이 다시 붙는다.
        let b = ConnectSpec::parse("oracle://u:pw@h:1521/svc?schema=HR").unwrap();
        // ★ 재접속 원천(10-09): 붙어 있는 대상은 **비밀번호 없이** 남는다(비밀번호는 금고 몫 · 21 §7).
        assert!(r
            .connected_to()
            .is_some_and(|s| s.password.is_none() && s.user.as_deref() == Some("u")));
        ev.clear();
        assert!(r.connect(&b, &mut |e| ev.push(e)));
        assert!(!ev
            .iter()
            .any(|e| matches!(e, RunEvent::Disconnected | RunEvent::ConnectionClosed)));
        assert!(ev.iter().any(|e| matches!(e, RunEvent::Connected { .. })));
        // 같은 대상 · 거부 = 옛 세션은 이미 닫혔다 → `Disconnected` + 오류.
        fail.set(true);
        ev.clear();
        let c = ConnectSpec::parse("oracle://u:@h:1521/svc").unwrap();
        assert!(!r.connect(&c, &mut |e| ev.push(e)));
        assert!(r.session.is_none() && r.connection.is_none());
        assert!(matches!(ev.first(), Some(RunEvent::ConnectionClosed)));
        assert!(matches!(ev.get(1), Some(RunEvent::Error { .. })));
        // 다른 서버로의 실패 = 옛 접속 유지(종전 동작).
        fail.set(false);
        assert!(r.connect(&a, &mut |_| {}));
        fail.set(true);
        ev.clear();
        let other = ConnectSpec::parse("oracle://u:pw@other:1521/svc").unwrap();
        assert!(!r.connect(&other, &mut |e| ev.push(e)));
        assert!(r.session.is_some());
        assert!(!ev.iter().any(|e| matches!(e, RunEvent::Disconnected)));
    }

    /// docs/56 L1 — 수동 모드의 읽기 트랜잭션 자동 종료: 조회만이면 끝난다(이벤트) · 변경 뒤 조회는 안 끝낸다 ·
    /// 0행 DML은 변경 아님 · FOR UPDATE/사용자 BEGIN은 유지 · COMMIT 문장 뒤엔 다시 끝낸다 · off면 안 끝낸다.
    #[test]
    fn read_only_transactions_end_in_manual_mode() {
        let ended = |ev: &[RunEvent]| {
            ev.iter()
                .filter(|e| matches!(e, RunEvent::ReadTxEnded { .. }))
                .count()
        };
        let mut r = runner().with_autocommit(false);
        let (_, ev) = collect(
            &mut r,
            "CREATE TABLE t (a INT);
",
        );
        assert_eq!(ended(&ev), 0, "DDL(SQLite = 트랜잭션 DDL) = 변경");
        // 호스트의 Commit 버튼과 같게: 세션을 실제로 커밋한 뒤 상태를 비운다(러너가 연 트랜잭션이 남아 있으면 안 된다 · T-146).
        if let Some(s) = r.session.as_mut() {
            let _ = s.commit();
        }
        r.note_tx_ended();
        let (_, ev) = collect(
            &mut r,
            "SELECT 1;
",
        );
        assert_eq!(ended(&ev), 1, "조회만 = 끝낸다");
        let (_, ev) = collect(
            &mut r,
            "UPDATE t SET a = 1 WHERE a = 99;
SELECT 2;
",
        );
        assert_eq!(
            ended(&ev),
            2,
            "0행 DML은 변경이 아니다 → 두 문장 모두 뒤에 끝낸다"
        );
        let (_, ev) = collect(
            &mut r,
            "INSERT INTO t VALUES (1);
SELECT * FROM t;
",
        );
        assert_eq!(ended(&ev), 0, "변경이 대기 중이면 조회 뒤에도 유지");
        // 호스트의 Commit/Rollback 버튼 = `note_tx_ended`(SQLite 드라이버는 명시 BEGIN이 없으면 `COMMIT` 문장이 오류라 버튼 경로로 본다).
        // 호스트의 Commit 버튼과 같게: 세션을 실제로 커밋한 뒤 상태를 비운다(러너가 연 트랜잭션이 남아 있으면 안 된다 · T-146).
        if let Some(s) = r.session.as_mut() {
            let _ = s.commit();
        }
        r.note_tx_ended();
        let (_, ev) = collect(&mut r, "SELECT * FROM t;");
        assert_eq!(ended(&ev), 1, "커밋 뒤 = 다시 깨끗");
        r.read_end = ReadEnd::Off;
        let (_, ev) = collect(
            &mut r,
            "SELECT 3;
",
        );
        assert_eq!(ended(&ev), 0, "off = 종전대로 유지");
        // 자동 커밋 모드는 대상이 아니다(문장마다 커밋).
        let mut a = runner().with_autocommit(true);
        let (_, ev) = collect(&mut a, "SELECT 1;");
        assert_eq!(ended(&ev), 0);
    }

    #[test]
    fn tx_effect_rules() {
        let e = |sql: &str, rows: Option<u64>, d: Dialect| tx_effect(sql, rows, d);
        assert_eq!(e("select 1", None, Dialect::Postgres), TxEffect::Clean);
        assert_eq!(
            e("select * from t for update", None, Dialect::Oracle),
            TxEffect::UserHold
        );
        assert_eq!(e("BEGIN", None, Dialect::Postgres), TxEffect::UserHold);
        assert_eq!(
            e("update t set a=1", Some(0), Dialect::Oracle),
            TxEffect::Clean
        );
        assert_eq!(
            e("update t set a=1", Some(3), Dialect::Oracle),
            TxEffect::Change
        );
        assert_eq!(
            e("delete from t", None, Dialect::Oracle),
            TxEffect::Change,
            "행 수를 모르면 변경"
        );
        assert_eq!(
            e("create table x (a int)", None, Dialect::Oracle),
            TxEffect::Ended,
            "암묵 커밋 DDL"
        );
        assert_eq!(
            e("create table x (a int)", None, Dialect::Postgres),
            TxEffect::Change,
            "트랜잭션 DDL"
        );
        assert_eq!(
            e("call p()", None, Dialect::Oracle),
            TxEffect::Change,
            "알 수 없는 문장 = 변경"
        );
        assert_eq!(e("rollback", None, Dialect::Mssql), TxEffect::Ended);
        assert_eq!(ReadEnd::parse("strict"), ReadEnd::Strict);
        assert_eq!(ReadEnd::parse("bogus"), ReadEnd::Auto);
        assert!(nsql_core::Caps::of(Dialect::Sqlite).strict_probe.is_none());
    }

    fn collect(r: &mut Runner, src: &str) -> (usize, Vec<RunEvent>) {
        let mut ev = Vec::new();
        let mut no_prompt = |_: &str| None;
        let errs = r.run_script(src, &mut no_prompt, &mut |e| ev.push(e));
        (errs, ev)
    }

    /// mac 09-21 로그 점검: `PRINT` 로그에 **값**(비밀 이름은 `******`) · 인자 없는 `VARIABLE` = 이름·타입 목록(`VarList` ·
    /// 로그 "VARIABLE …" — 종전엔 `PRINT`로 찍혔다) · `Begin.server` = 클라이언트 명령 false · SQL true.
    #[test]
    fn print_and_variable_logs_and_server_flag() {
        let mut r = runner();
        let (errs, ev) = collect(
            &mut r,
            "VAR V_N NUMBER = 3\nEXEC :V_TOKEN := 'abc'\nPRINT V_N V_TOKEN\nVARIABLE\nSELECT 1;\n",
        );
        assert_eq!(errs, 0);
        let print = ev
            .iter()
            .find(|e| matches!(e, RunEvent::Print { .. }))
            .expect("PRINT");
        let line = log_entries(print)[0].message.clone();
        assert_eq!(line, "PRINT V_N = 3 · V_TOKEN = ******", "{line}");
        let list = ev
            .iter()
            .find(|e| matches!(e, RunEvent::VarList { .. }))
            .expect("VARIABLE 목록");
        let line = log_entries(list)[0].message.clone();
        assert!(line.starts_with("VARIABLE "), "{line}");
        assert!(
            line.contains("V_N NUMBER") && line.contains("V_TOKEN VARCHAR2(3)"),
            "{line}"
        );
        assert!(!line.contains("abc"), "목록에는 값이 없다: {line}");
        // 서버행 표식: 항목 순서 = VAR · EXEC · PRINT · VARIABLE · SELECT — `EXEC :V := 'abc'`는 로컬 리터럴 대입이라
        // 서버행이 아니다(139차 win · `nsql_script::local_assign` · 종전 참은 전송 로그·세션 상태 표식 오탐).
        let flags: Vec<bool> = ev
            .iter()
            .filter_map(|e| match e {
                RunEvent::Begin { server, .. } => Some(*server),
                _ => None,
            })
            .collect();
        assert_eq!(flags, vec![false, false, false, false, true]);
    }

    /// `goes_to_server` 표 — 명령별 독립 확인.
    #[test]
    fn goes_to_server_table() {
        use nsql_script::split_script;
        let kinds =
            |src: &str| -> Vec<bool> { split_script(src).iter().map(goes_to_server).collect() };
        assert_eq!(
            kinds("PRINT a\nVARIABLE\nDEFINE x = 1\nSET SERVEROUTPUT ON\nPROMPT hi\n"),
            vec![false; 5]
        );
        assert_eq!(
            kinds("SELECT 1;\nEXEC p(:a)\nCONNECT x\nDISCONNECT\nDESC t\nSHOW ERRORS\n"),
            vec![true; 6]
        );
        assert_eq!(kinds("SHOW VARIABLES\nSHOW ERRORS\n"), vec![false, true]);
        // 로컬 리터럴 대입은 서버행이 아니다(139차 win · 트랜잭션 로그 Running 잔류 · 세션 상태 표식 오탐) · 식 우변 · 호출은 서버행.
        assert_eq!(
            kinds("EXEC :a := 1\nEXEC :b := 'x'\nEXEC :c := :a + 1\nEXEC p(:a)\n"),
            vec![false, false, true, true]
        );
    }

    /// ★ T-153 후속(10-26): `DEFINE x SHARE|GLOBAL`은 층을 옮기고 `Vars`가 층별로 나뉘어 온다 · 다음 실행의 탭 층 교체(`set_layer(Local)`)는
    /// 공유·글로벌을 지우지 않는다 · 없는 이름 = 오류 1.
    #[test]
    fn define_layer_moves_macro_and_vars_event_is_layered() {
        let mut r = runner();
        let (errs, ev) = collect(
            &mut r,
            "DEFINE A = 1\nDEFINE B = 2\nDEFINE B SHARE\nDEFINE C = 3\nDEFINE C GLOBAL\n",
        );
        assert_eq!(errs, 0);
        let vars = ev
            .iter()
            .rev()
            .find(|e| matches!(e, RunEvent::Vars { .. }))
            .expect("Vars");
        let RunEvent::Vars {
            defines,
            defines_shared,
            defines_global,
            ..
        } = vars
        else {
            unreachable!()
        };
        let names =
            |v: &Vec<(String, String, String)>| v.iter().map(|d| d.0.clone()).collect::<Vec<_>>();
        assert_eq!(names(defines), vec!["A"]);
        assert_eq!(names(defines_shared), vec!["B"]);
        assert_eq!(names(defines_global), vec!["C"]);
        r.engine.defines.set_layer(
            nsql_script::MacroLayer::Local,
            std::iter::once(("A".to_string(), "9".to_string())).collect(),
        );
        assert_eq!(r.engine.defines.get("B").map(String::as_str), Some("2"));
        assert_eq!(r.engine.defines.get("C").map(String::as_str), Some("3"));
        let (errs, _) = collect(&mut r, "DEFINE NOPE SHARE\n");
        assert_eq!(errs, 1);
    }

    /// `CONNECT dev` — 사용자명만 있는 스펙은 프로필 해석기를 거친다(T-16b). 해석기가 없으면 그대로 접속.
    #[test]
    fn connect_bare_name_goes_through_resolver() {
        let mut r = runner().with_resolver(Box::new(|name: &str| match name {
            "dev" => Ok(Some(ConnectSpec {
                dialect: Some(Dialect::Sqlite),
                database: Some(":memory:".into()),
                ..Default::default()
            })),
            "typo" => Err("프로필 'typo'이(가) 없습니다".into()),
            _ => Ok(None),
        }));
        let (errs, ev) = collect(&mut r, "CONNECT dev\nSELECT 1 AS one;\n");
        assert_eq!(errs, 0, "{ev:?}");
        assert!(ev.iter().any(|e| matches!(e, RunEvent::Connected { .. })));
        let (errs, ev) = collect(&mut r, "CONNECT typo\n");
        assert_eq!(errs, 1);
        assert!(ev
            .iter()
            .any(|e| matches!(e, RunEvent::Error { error, .. } if error.message.contains("typo"))));
        // 완전한 접속 문자열은 해석기를 거치지 않는다.
        let (errs, _) = collect(&mut r, "CONNECT x/y@localhost/:memory:\n");
        assert_eq!(errs, 0);
    }

    #[test]
    fn session_variables_survive_across_statements_on_sqlite() {
        let mut r = runner();
        let src = "CREATE TABLE emp(id INTEGER, name TEXT);\nINSERT INTO emp VALUES (7, '홍길동');\nEXEC :V_ID := 7\nEXEC :V_NAME := (SELECT name FROM emp WHERE id = :V_ID)\nSELECT name, :V_NAME AS bound FROM emp WHERE id = :V_ID;\nPRINT V_NAME\n";
        let (errs, ev) = collect(&mut r, src);
        assert_eq!(errs, 0, "{ev:?}");
        assert_eq!(
            r.engine.vars.get("V_NAME").unwrap().value,
            Value::Str("홍길동".into())
        );
        let rs = ev
            .iter()
            .find_map(|e| {
                if let RunEvent::ResultSet { rs, .. } = e {
                    Some(rs)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            rs.rows[0],
            vec![Value::Str("홍길동".into()), Value::Str("홍길동".into())]
        );
        assert!(ev.iter().any(|e| matches!(e, RunEvent::Print { pairs } if pairs[0] == ("V_NAME".to_string(), Value::Str("홍길동".into())))));
    }

    #[test]
    fn errors_and_whenever_exit() {
        let mut r = runner();
        let (errs, ev) = collect(&mut r, "SELECT * FROM nope;\nSELECT 1;\n");
        assert_eq!(errs, 1);
        assert!(
            ev.iter().any(|e| matches!(e, RunEvent::ResultSet { .. })),
            "두 번째 문장은 계속 실행"
        );
        let (errs, ev) = collect(
            &mut r,
            "WHENEVER SQLERROR EXIT\nSELECT * FROM nope;\nSELECT 1;\n",
        );
        assert_eq!(errs, 1);
        assert!(
            !ev.iter().any(|e| matches!(e, RunEvent::ResultSet { .. })),
            "중단"
        );
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nsql-run-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn result_sets(ev: &[RunEvent]) -> Vec<&ResultSet> {
        ev.iter()
            .filter_map(|e| match e {
                RunEvent::ResultSet { rs, .. } => Some(rs),
                _ => None,
            })
            .collect()
    }

    /// T-9(a) SPOOL: 시작은 조용히 · 맨몸은 상태 · OFF는 닫고 보고 · CREATE는 기존 파일 거부 · 모르는 옵션은 오류 ·
    /// 포트가 없는 호스트는 "미지원" 메시지. 파일에는 호스트가 `write`한 것만 들어간다.
    #[test]
    fn spool_port_opens_and_closes_files() {
        let dir = temp_dir("spool");
        let target = dir.join("out");
        let handle = Spool::new_handle();
        let mut r = runner().with_spool(handle.clone());
        let (errs, ev) = collect(&mut r, "SPOOL\n");
        assert_eq!(errs, 0);
        assert!(ev
            .iter()
            .any(|e| matches!(e, RunEvent::Message(m) if m == t(Msg::SpoolStatusOff))));
        let (errs, ev) = collect(&mut r, &format!("SPOOL {}\n", target.display()));
        assert_eq!(errs, 0, "{ev:?}");
        assert!(
            !ev.iter().any(|e| matches!(e, RunEvent::Message(_))),
            "시작은 조용히"
        );
        {
            let mut sp = handle.lock().unwrap();
            assert!(sp.is_on());
            assert_eq!(
                sp.path().unwrap().extension().unwrap(),
                "lst",
                ".lst 기본 확장자"
            );
            sp.write(b"line 1\n");
        }
        let (_, ev) = collect(&mut r, "SPOOL\n");
        assert!(ev.iter().any(
            |e| matches!(e, RunEvent::Message(m) if m.contains("out.lst") && m != t(Msg::SpoolStatusOff))
        ));
        let (errs, ev) = collect(&mut r, "SPOOL OFF\n");
        assert_eq!(errs, 0);
        assert!(!handle.lock().unwrap().is_on());
        assert!(ev
            .iter()
            .any(|e| matches!(e, RunEvent::Message(m) if m.contains("out.lst"))));
        let lst = dir.join("out.lst");
        assert_eq!(std::fs::read_to_string(&lst).unwrap(), "line 1\n");
        // APPEND는 이어쓰기 · 기본(REPLACE)은 덮어쓰기.
        collect(&mut r, &format!("SPOOL {} APPEND\n", lst.display()));
        handle.lock().unwrap().write(b"line 2\n");
        collect(&mut r, "SPOOL OUT\n");
        assert_eq!(std::fs::read_to_string(&lst).unwrap(), "line 1\nline 2\n");
        collect(&mut r, &format!("SPOOL {}\n", lst.display()));
        handle.lock().unwrap().write(b"fresh\n");
        collect(&mut r, "SPOOL OFF\n");
        assert_eq!(std::fs::read_to_string(&lst).unwrap(), "fresh\n");
        // CREATE = 이미 있으면 오류 · 모르는 옵션 = 오류.
        let (errs, _) = collect(&mut r, &format!("SPOOL {} CREATE\n", lst.display()));
        assert_eq!(errs, 1);
        let (errs, _) = collect(&mut r, "SPOOL x.lst BOGUS\n");
        assert_eq!(errs, 1);
        // 포트 없는 호스트.
        let mut plain = runner();
        let (errs, ev) = collect(&mut plain, "SPOOL a.lst\n");
        assert_eq!(errs, 0);
        assert!(ev
            .iter()
            .any(|e| matches!(e, RunEvent::Message(m) if m == t(Msg::SpoolNotSupported))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-9(b) `@`는 cwd 기준 · `@@`는 호출 스크립트 폴더 기준(중첩도 자기 폴더) · 인자 `&1..&n` 전달 · 돌아오면 호출자의 `&1` 복원 ·
    /// 확장자 없으면 `.sql` 보완 · 없는 파일은 오류.
    #[test]
    fn include_resolves_relative_to_caller_and_passes_args() {
        let dir = temp_dir("include");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(
            dir.join("main.sql"),
            "DEFINE tag = outer\nSELECT '&1' AS main_arg;\n@@sub/child 7 x\nSELECT '&1' AS restored;\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("sub/child.sql"),
            "SELECT &1 AS child_arg, '&2' AS second, '&tag' AS inherited;\n@@leaf.sql\n",
        )
        .unwrap();
        std::fs::write(dir.join("sub/leaf.sql"), "SELECT '&1' AS leaf_sees;\n").unwrap();
        let mut r = runner();
        r.engine.set_args(&["top".to_string()]);
        let src = std::fs::read_to_string(dir.join("main.sql")).unwrap();
        let mut ev = Vec::new();
        let mut no_prompt = |_: &str| None;
        let errs = r.run_script_in(
            &src,
            Some(&dir.join("main.sql")),
            &mut no_prompt,
            &mut |e| ev.push(e),
        );
        assert_eq!(errs, 0, "{ev:?}");
        let sets = result_sets(&ev);
        assert_eq!(sets.len(), 4, "{ev:?}");
        assert_eq!(sets[0].rows[0][0], Value::Str("top".into()));
        assert_eq!(
            sets[1].rows[0],
            vec![
                Value::Int(7),
                Value::Str("x".into()),
                Value::Str("outer".into())
            ]
        );
        assert_eq!(
            sets[2].rows[0][0],
            Value::Str("7".into()),
            "인자 없는 중첩은 물려받는다"
        );
        assert_eq!(
            sets[3].rows[0][0],
            Value::Str("top".into()),
            "돌아오면 복원"
        );
        assert!(
            !r.engine.defines.contains_key("2"),
            "호출자에 없던 &2는 지운다"
        );
        // `@`는 cwd 기준 — 절대 경로면 어디서든.
        let (errs, ev) = collect(
            &mut r,
            &format!("@{} 1 2\n", dir.join("sub/child.sql").display()),
        );
        assert_eq!(errs, 0, "{ev:?}");
        // 없는 파일.
        let (errs, ev) = collect(&mut r, "@@nope.sql\n");
        assert_eq!(errs, 1);
        assert!(ev.iter().any(
            |e| matches!(e, RunEvent::Error { error, .. } if error.message.contains("nope.sql"))
        ));
        // 자기 자신을 부르는 스크립트는 상한에서 멈춘다.
        std::fs::write(dir.join("loop.sql"), "@@loop.sql\n").unwrap();
        let (errs, ev) = collect(&mut r, &format!("@{}\n", dir.join("loop.sql").display()));
        assert!(errs >= 1);
        assert!(ev.iter().any(|e| matches!(e, RunEvent::Error { error, .. } if error.message.contains(&MAX_SCRIPT_DEPTH.to_string()))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// T-9(d) 엄격 모드: 미정의 `&var`는 프롬프트가 있어도 오류 · 선언 없는 `:bind`는 오류(그리고 암묵 선언을 남기지 않는다) ·
    /// DEFINE/VARIABLE로 선언하면 통과.
    #[test]
    fn strict_mode_rejects_undefined_substitution_and_implicit_bind() {
        let mut r = runner().with_strict(true);
        let mut ev = Vec::new();
        let mut prompt = |_: &str| Some("5".to_string());
        let errs = r.run_script("SELECT &N AS n;\n", &mut prompt, &mut |e| ev.push(e));
        assert_eq!(errs, 1);
        assert!(ev
            .iter()
            .any(|e| matches!(e, RunEvent::Error { error, .. } if error.message.contains("&N"))));
        let (errs, ev) = collect(&mut r, "SELECT :UNDECLARED AS x;\n");
        assert_eq!(errs, 1, "{ev:?}");
        assert!(ev.iter().any(
            |e| matches!(e, RunEvent::Error { error, .. } if error.message.contains(":UNDECLARED"))
        ));
        assert!(
            r.engine.vars.get("UNDECLARED").is_none(),
            "암묵 선언을 남기지 않는다"
        );
        let (errs, ev) = collect(
            &mut r,
            "DEFINE n = 3\nVARIABLE v NUMBER = 4\nSELECT :v + &n AS x;\n",
        );
        assert_eq!(errs, 0, "{ev:?}");
        // 엄격이 아니면 프롬프트(D-272 뒤 SQLite 기본은 미정의 `&`를 묻지 않으므로 `on`으로 고정해 프롬프트 기법만 본다).
        let mut loose = runner().with_define_mode("on");
        let mut ev = Vec::new();
        let errs = loose.run_script("SELECT &N AS n;\n", &mut prompt, &mut |e| ev.push(e));
        assert_eq!(errs, 0);
    }

    /// T-52 `SHOW TABLES`(psql `\dt` · sqlite `.tables`의 종착) — 카탈로그 목록을 결과 집합으로.
    #[test]
    fn show_tables_lists_catalog_objects() {
        let mut r = runner();
        let (errs, ev) = collect(
            &mut r,
            "CREATE TABLE emp(id INTEGER);\nCREATE TABLE dept(id INTEGER);\nSHOW TABLES\nSHOW USER\n",
        );
        assert_eq!(errs, 0, "{ev:?}");
        let sets = result_sets(&ev);
        assert_eq!(sets.len(), 1);
        let names: Vec<String> = sets[0].rows.iter().map(|row| row[1].display()).collect();
        assert!(
            names.contains(&"emp".to_string()) && names.contains(&"dept".to_string()),
            "{names:?}"
        );
        assert!(ev
            .iter()
            .any(|e| matches!(e, RunEvent::Message(m) if m.starts_with("USER = "))));
    }

    /// D-135 계층: 탭 층은 호스트가 실행마다 갈아 끼운다 → 다른 탭의 값은 안 보이고, `VAR x SHARE`로 올린 값만 같이 본다.
    /// 변수 사건은 실행당 한 번 · 바뀐 것이 있을 때만.
    #[test]
    fn tab_local_vars_and_shared_layer() {
        let mut r = runner();
        let vars_of = |ev: &[RunEvent]| {
            ev.iter().find_map(|e| match e {
                RunEvent::Vars {
                    local,
                    shared,
                    changed,
                    ..
                } => Some((local.clone(), shared.clone(), changed.clone())),
                _ => None,
            })
        };
        // 탭 A.
        let (errs, ev) = collect(&mut r, "EXEC :X := 1\nEXEC :Y := 'keep'\nVAR Y SHARE\n");
        assert_eq!(errs, 0, "{ev:?}");
        let (local_a, shared, changed) = vars_of(&ev).expect("변수 사건");
        assert_eq!(local_a.len(), 1);
        assert_eq!(shared.len(), 1);
        assert_eq!(changed.len(), 2);
        // 탭 B(빈 탭 층): X는 없고 공유한 Y는 보인다.
        r.engine.vars.set_local(Vec::new());
        let (errs, ev) = collect(&mut r, "PRINT Y\n");
        assert_eq!(errs, 0, "{ev:?}");
        assert!(vars_of(&ev).is_none(), "바뀐 것이 없으면 사건도 없다");
        let (errs, _) = collect(&mut r, "PRINT X\n");
        assert_eq!(errs, 1, "다른 탭의 변수는 보이지 않는다");
        // 탭 A로 돌아오면 X가 있다.
        r.engine.vars.set_local(local_a);
        let (errs, _) = collect(&mut r, "PRINT X Y\n");
        assert_eq!(errs, 0);
    }

    /// D-139: OUT 바인드가 없는 방언의 `EXEC SELECT … INTO` — 자리 순서로 받고, 0행·여러 행은 Oracle식 오류 · first 정책은 첫 행.
    #[test]
    fn select_into_row_policy() {
        let mut r = runner();
        let (errs, ev) = collect(
            &mut r,
            "CREATE TABLE t (a INT, b TEXT);\nINSERT INTO t VALUES (1, 'x');\nINSERT INTO t VALUES (2, 'y');\n\
             EXEC SELECT a, b INTO :A, :B FROM t WHERE a = 2\n",
        );
        assert_eq!(errs, 0, "{ev:?}");
        assert_eq!(
            r.engine.vars.get("A").map(|v| v.value.clone()),
            Some(Value::Int(2))
        );
        assert_eq!(
            r.engine.vars.get("B").map(|v| v.value.clone()),
            Some(Value::Str("y".into()))
        );
        assert!(result_sets(&ev).is_empty(), "EXEC는 그리드를 내지 않는다");
        // 0행 · 여러 행 = 오류, 값은 그대로.
        let (errs, _) = collect(&mut r, "EXEC SELECT a INTO :A FROM t WHERE a = 99\n");
        assert_eq!(errs, 1);
        let (errs, _) = collect(&mut r, "EXEC SELECT a INTO :A FROM t\n");
        assert_eq!(errs, 1);
        assert_eq!(
            r.engine.vars.get("A").map(|v| v.value.clone()),
            Some(Value::Int(2))
        );
        // first 정책: 여러 행 = 첫 행 · 0행 = 그대로 두고 알림.
        r.engine.settings.into_first = true;
        let (errs, _) = collect(&mut r, "EXEC SELECT a INTO :A FROM t ORDER BY a\n");
        assert_eq!(errs, 0);
        assert_eq!(
            r.engine.vars.get("A").map(|v| v.value.clone()),
            Some(Value::Int(1))
        );
        let (errs, _) = collect(&mut r, "EXEC SELECT a INTO :A FROM t WHERE a = 99\n");
        assert_eq!(errs, 0);
        assert_eq!(
            r.engine.vars.get("A").map(|v| v.value.clone()),
            Some(Value::Int(1))
        );
        // 식 대입도 같은 길(자리 순서 — 열 이름에 기대지 않는다).
        let (errs, _) = collect(&mut r, "EXEC :N := (SELECT COUNT(*) FROM t)\n");
        assert_eq!(errs, 0);
        assert_eq!(
            r.engine.vars.get("N").map(|v| v.value.clone()),
            Some(Value::Int(2))
        );
    }

    /// D-137: 빠진 입력 찾기 → 넣기 → 실행(바인드 = 타입 있는 값 · 치환 = 글자) · 다시 물을 것이 없다.
    #[test]
    fn missing_inputs_then_apply() {
        // D-272: SQLite 기본은 미정의 `&`를 묻지 않는다 → 입력 창 기법 자체를 보는 이 시험은 `on`으로 고정.
        let mut r = runner().with_define_mode("on");
        let src = "SELECT :N + 1 AS n, '&WHO' AS who;\n";
        let needs = r.missing_inputs(src);
        assert_eq!(needs.len(), 2, "{needs:?}");
        r.apply_inputs(vec![
            (nsql_script::InputKind::Bind, "N".into(), "41".into(), None),
            (
                nsql_script::InputKind::Macro,
                "WHO".into(),
                "kim".into(),
                None,
            ),
        ]);
        assert!(r.missing_inputs(src).is_empty());
        let (errs, ev) = collect(&mut r, src);
        assert_eq!(errs, 0, "{ev:?}");
        let sets = result_sets(&ev);
        assert_eq!(sets[0].rows[0][0].display(), "42");
        assert_eq!(sets[0].rows[0][1].display(), "kim");
        // 빈 칸·NULL = NULL · 따옴표 = 그 안의 글 · 그 밖 = 글 그대로.
        assert_eq!(super::input_value(" "), Value::Null);
        assert_eq!(super::input_value("null"), Value::Null);
        assert_eq!(super::input_value("'a''b'"), Value::Str("a'b".into()));
        assert_eq!(super::input_value("ACME"), Value::Str("ACME".into()));
    }

    /// T-151 — SQL Server `OUTPUT` 보충: 서명이 OUTPUT인 자리의 바인드만 · 이미 `OUTPUT`을 쓴 것·IN 자리·상수는 빼고 · 이름 표기.
    #[test]
    fn output_binds_follow_the_signature() {
        use nsql_catalog::RoutineArg;
        let arg = |pos: i64, name: &str, io: &str| RoutineArg {
            overload: "1".into(),
            position: pos,
            name: name.into(),
            data_type: "INT".into(),
            in_out: io.into(),
        };
        let sig = vec![
            arg(1, "A", "IN"),
            arg(2, "B", "IN/OUT"),
            arg(3, "MSG", "IN/OUT"),
        ];
        let f = |s: &str| {
            super::output_binds(&nsql_script::tsql_call_shape(s).expect("call"), &sig, "1")
        };
        assert_eq!(f("p 3, :X, :Y"), vec!["X".to_string(), "Y".to_string()]);
        assert_eq!(f("p :IN_ONLY, :X OUTPUT, :Y"), vec!["Y".to_string()]);
        assert_eq!(f("p @msg = :M, @a = :A, @b = 5"), vec!["M".to_string()]);
        assert!(f("p 1, 2, 3").is_empty());
        assert_eq!(
            super::var_type_of("DATETIME2"),
            Some(nsql_core::VarType::Timestamp)
        );
        assert_eq!(
            super::var_type_of("TIMESTAMP(6)"),
            Some(nsql_core::VarType::Timestamp)
        );
        assert_eq!(
            super::var_type_of("PL/SQL BOOLEAN"),
            Some(nsql_core::VarType::Boolean)
        );
        assert_eq!(super::var_type_of("NVARCHAR"), None);
    }

    /// T-153 — 러너 왕복: `COLUMN … NEW_VALUE` → 다음 문장의 `&변수` · 시스템 변수(`_ROW_COUNT` · `_SQLCODE` · `_DATE` ·
    /// `_DIALECT`) · `${이름:q}`.
    #[test]
    fn new_value_and_system_vars_round_trip() {
        let mut r = runner();
        let last = |ev: &[RunEvent]| -> Vec<String> {
            ev.iter()
                .rev()
                .find_map(|e| match e {
                    RunEvent::ResultSet { rs, .. } => {
                        Some(rs.rows[0].iter().map(Value::display).collect())
                    }
                    _ => None,
                })
                .unwrap_or_default()
        };
        let (errs, ev) = collect(
            &mut r,
            "CREATE TABLE t (a INT);
INSERT INTO t VALUES (1),(5),(3);
COLUMN mx NEW_VALUE v_mx
SELECT MAX(a) AS mx FROM t;
SELECT &v_mx + 1 AS nxt, '&_DIALECT' AS d, length('&_DATE') AS dl;
",
        );
        assert_eq!(errs, 0, "{ev:?}");
        assert_eq!(last(&ev), vec!["6", "sqlite", "10"]);
        let (_, ev) = collect(
            &mut r,
            "DELETE FROM t WHERE a > 1;\nSELECT &_ROW_COUNT AS rc, &_SQLCODE AS sc;\n",
        );
        assert_eq!(last(&ev), vec!["2", "0"]);
        let (errs, _) = collect(&mut r, "SELECT * FROM no_such_table;\n");
        assert_eq!(errs, 1);
        let (_, ev) = collect(
            &mut r,
            "SELECT CASE WHEN &_SQLCODE <> 0 THEN 'failed' ELSE 'ok' END AS s;\n",
        );
        assert_eq!(last(&ev), vec!["failed"]);
        r.engine.define("who", "O'Neil");
        let (_, ev) = collect(&mut r, "SELECT ${who:q} AS w;\n");
        assert_eq!(last(&ev), vec!["O'Neil"]);
    }

    /// T-153 — 선언 없이 쓴 바인드는 **한 번** 알린다(그 뒤에는 표에 있다) · 진단 목록은 실행마다 비워진다(쌓이지 않는다).
    #[test]
    fn implicit_binds_are_reported_once() {
        let mut r = runner();
        let msgs = |ev: &[RunEvent]| -> Vec<String> {
            ev.iter()
                .filter_map(|e| match e {
                    RunEvent::Message(m) if m.contains(":TYPO") => Some(m.clone()),
                    _ => None,
                })
                .collect()
        };
        let (_, ev) = collect(&mut r, "SELECT :TYPO AS a, :TYPO AS b;\n");
        assert_eq!(msgs(&ev).len(), 1, "{ev:?}");
        assert!(r.engine.diagnostics.is_empty());
        let (_, ev) = collect(&mut r, "SELECT :TYPO AS a;\n");
        assert!(msgs(&ev).is_empty(), "두 번째부터는 표에 있는 변수다");
        // 대입된 변수는 알리지 않는다.
        let (_, ev) = collect(&mut r, "EXEC :OK := 1\nSELECT :OK AS a;\n");
        let ok: Vec<&RunEvent> = ev
            .iter()
            .filter(
                |e| matches!(e, RunEvent::Message(m) if *m == tf(Msg::WarnImplicitVar, &[":OK"])),
            )
            .collect();
        assert!(ok.is_empty(), "{ok:?}");
    }

    /// T-153 — `ACCEPT` 값 판정(조건마다 혼자 결과를 바꾼다) + 러너 왕복: 물어서 넣는다 · 빈 답 = DEFAULT · 입력 창이 이미
    /// 답했으면 묻지 않는다 · 엄격 모드 = DEFAULT 아니면 오류.
    #[test]
    fn accept_asks_defaults_and_skips() {
        use super::accept_value as f;
        let s = |x: &str| Some(x.to_string());
        assert_eq!(f(s("7"), s("10")), s("7"));
        assert_eq!(f(s(""), s("10")), s("10"), "빈 답 = DEFAULT");
        assert_eq!(f(None, s("10")), s("10"), "답 없음 = DEFAULT");
        assert_eq!(f(s(""), None), s(""));
        assert_eq!(f(None, None), None);

        let mut r = runner();
        let src = "ACCEPT n NUMBER DEFAULT 10 PROMPT 'N? '\nSELECT &n + 1 AS v;\n";
        let mut asked: Vec<String> = Vec::new();
        let value =
            |script: &str, r: &mut Runner, answer: Option<&str>, asked: &mut Vec<String>| {
                let mut out = String::new();
                let mut prompt = |l: &str| {
                    asked.push(l.to_string());
                    answer.map(str::to_string)
                };
                r.run_script(script, &mut prompt, &mut |e| {
                    if let RunEvent::ResultSet { rs, .. } = e {
                        out = rs.rows[0][0].display();
                    }
                });
                out
            };
        assert_eq!(value(src, &mut r, Some("41"), &mut asked), "42");
        assert_eq!(asked, vec!["N? ".to_string()], "안내 글로 묻는다");
        assert_eq!(
            value(src, &mut r, Some(""), &mut asked),
            "11",
            "빈 답 = DEFAULT(앞 실행의 41이 아니다)"
        );
        // 입력 창이 이미 답했다 → 묻지 않는다.
        asked.clear();
        r.apply_inputs(vec![(
            nsql_script::InputKind::Macro,
            "N".into(),
            "5".into(),
            None,
        )]);
        assert_eq!(value(src, &mut r, Some("999"), &mut asked), "6");
        assert!(asked.is_empty(), "{asked:?}");
        // 다음 실행은 다시 묻는다.
        assert_eq!(value(src, &mut r, Some("1"), &mut asked), "2");
    }

    /// T-151 — 호출의 받는 바인드: OUT/INOUT 자리의 바인드만(자리 · 이름 표기) · 바인드가 아닌 OUT 자리는 빈 글 · IN 자리의 바인드는
    /// 받지 않는다 · 인자 수가 맞는 오버로드 · 서명을 모르거나 받을 것이 없으면 None.
    #[test]
    fn call_captures_follow_the_signature() {
        use nsql_catalog::RoutineArg;
        let arg = |ov: &str, pos: i64, name: &str, io: &str| RoutineArg {
            overload: ov.into(),
            position: pos,
            name: name.into(),
            data_type: "INTEGER".into(),
            in_out: io.into(),
        };
        let sig = vec![
            arg("1", 1, "A", "IN"),
            arg("2", 1, "A", "IN"),
            arg("2", 2, "B", "IN/OUT"),
            arg("2", 3, "MSG", "OUT"),
        ];
        let shape = |s: &str| nsql_script::call_shape(s).expect("call");
        let f = |s: &str| super::call_captures(&shape(s), &sig);
        assert_eq!(
            f("p(3, :X, :Y)"),
            Some(vec!["X".to_string(), "Y".to_string()])
        );
        assert_eq!(
            f("p(:IN_ONLY, 5, :Y)"),
            Some(vec![String::new(), "Y".to_string()]),
            "IN 자리의 바인드는 받지 않는다 · 값이 온 OUT 자리는 건너뛴다"
        );
        assert_eq!(
            f("p(a => 1, msg => :M, b => :N)"),
            Some(vec!["N".to_string(), "M".to_string()]),
            "이름 표기 = 이름으로 · 순서는 서명의 OUT 순서"
        );
        assert_eq!(f("p(:ONLY)"), None, "첫 오버로드(IN 하나) = 받을 것이 없다");
        assert_eq!(f("p(1, 2, 3)"), None);
        assert_eq!(f("p(1, 2, 3, 4)"), None, "인자 수가 맞는 오버로드가 없다");
        assert_eq!(
            super::call_captures(&shape("p(1, :X)"), &[]),
            None,
            "서명 모름"
        );
    }

    /// T-146 판정(MC/DC) — 다섯 조건이 각각 혼자 결과를 바꾼다.
    #[test]
    fn manual_begin_mcdc() {
        use nsql_core::TxControl as C;
        let f = |d: Dialect, a: bool, o: bool, u: bool, c: C| {
            super::manual_begin_sql(&nsql_core::Caps::of(d), a, o, u, c)
        };
        // 기준: 수동 · PG · 열린 것 없음 · 사용자 트랜잭션 없음 · 보통 문장 → 연다.
        assert_eq!(
            f(Dialect::Postgres, false, false, false, C::None),
            Some("BEGIN")
        );
        assert_eq!(
            f(Dialect::Postgres, true, false, false, C::None),
            None,
            "자동 커밋"
        );
        assert_eq!(
            f(Dialect::Postgres, false, true, false, C::None),
            None,
            "이미 열림"
        );
        assert_eq!(
            f(Dialect::Postgres, false, false, true, C::None),
            None,
            "사용자 BEGIN"
        );
        assert_eq!(
            f(Dialect::Postgres, false, false, false, C::Begin),
            None,
            "통제 문장"
        );
        assert_eq!(
            f(Dialect::Postgres, false, false, false, C::End),
            None,
            "통제 문장"
        );
        assert_eq!(
            f(Dialect::Oracle, false, false, false, C::None),
            None,
            "암묵 트랜잭션 방언"
        );
        assert_eq!(
            f(Dialect::Mssql, false, false, false, C::None),
            Some("BEGIN TRANSACTION")
        );
        assert_eq!(
            f(Dialect::Mysql, false, false, false, C::None),
            Some("START TRANSACTION")
        );
        assert_eq!(
            f(Dialect::Sqlite, false, false, false, C::None),
            Some("BEGIN")
        );
    }

    /// T-146 전 경로(SQLite 파일 · 두 접속): 수동 모드의 INSERT는 **ROLLBACK으로 되돌아가야** 한다(종전 = 이미 커밋돼 남았다).
    #[test]
    fn manual_mode_insert_is_really_uncommitted() {
        let path = std::env::temp_dir().join(format!("nsql_t146_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let target = format!("CONNECT sqlite://{}\n", path.display());
        let mut r = runner().with_autocommit(false);
        let (errs, ev) = collect(
            &mut r,
            &format!("{target}CREATE TABLE t (a INT);\nCOMMIT;\nINSERT INTO t VALUES (1);\nROLLBACK;\nSELECT COUNT(*) FROM t;\n"),
        );
        assert_eq!(errs, 0, "{ev:?}");
        let sets = result_sets(&ev);
        assert_eq!(
            sets.last().map(|s| s.rows[0][0].display()),
            Some("0".to_string()),
            "{ev:?}"
        );
        // 커밋하면 남는다.
        let (errs, ev) = collect(
            &mut r,
            "INSERT INTO t VALUES (2);\nCOMMIT;\nSELECT COUNT(*) FROM t;\n",
        );
        assert_eq!(errs, 0, "{ev:?}");
        assert_eq!(
            result_sets(&ev).last().map(|s| s.rows[0][0].display()),
            Some("1".to_string())
        );
        drop(r);
        let _ = std::fs::remove_file(&path);
    }

    /// `SHOW ERRORS`(SQL*Plus 관용 줄 · 09-20) — "미지원"이 아니라 "오류 없음"(Oracle 밖에서는 컴파일 오류 개념이 없다).
    #[test]
    fn show_errors_is_supported() {
        let mut r = runner();
        let (errs, ev) = collect(
            &mut r,
            "CREATE VIEW v1 AS SELECT 1 AS n;\nSHOW ERRORS\nSHOW ERR;\nSHOW ERRORS VIEW v1\n",
        );
        assert_eq!(errs, 0, "{ev:?}");
        let n = ev
            .iter()
            .filter(|e| matches!(e, RunEvent::Message(m) if m == nsql_i18n::t(Msg::ShowNoErrors)))
            .count();
        assert_eq!(n, 3, "{ev:?}");
    }

    #[test]
    fn connect_via_opener_and_substitution_prompt() {
        let mut r = runner().with_define_mode("on");
        let mut ev = Vec::new();
        let mut prompt = |name: &str| {
            Some(if name == "N" {
                "5".to_string()
            } else {
                String::new()
            })
        };
        let errs = r.run_script(
            "CONNECT sqlite://x/:memory:\nSELECT &N AS n;\n",
            &mut prompt,
            &mut |e| ev.push(e),
        );
        assert_eq!(errs, 0, "{ev:?}");
        assert!(ev.iter().any(|e| matches!(e, RunEvent::Connected { .. })));
        let rs = ev
            .iter()
            .find_map(|e| {
                if let RunEvent::ResultSet { rs, .. } = e {
                    Some(rs)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(rs.rows[0][0], Value::Int(5));
    }

    /// ★ T-77 잔여(10-30): ① 접속 스펙 `?autocommit=off`가 러너 모드를 정한다(설정보다 우선 · `Connected.autocommit`에 실린다) ·
    /// ② 수동 모드의 변경 문장 수 `tx_pending`(셸 프롬프트 `*n`) = DML마다 +1 · 읽기만 = 열려도 0 · COMMIT/ROLLBACK = 0.
    #[test]
    fn profile_autocommit_and_tx_pending_count() {
        let mut r = runner().with_autocommit(true);
        let (errs, ev) = collect(&mut r, "CONNECT sqlite://x/:memory:?autocommit=off\n");
        assert_eq!(errs, 0, "{ev:?}");
        assert!(ev.iter().any(|e| matches!(
            e,
            RunEvent::Connected {
                autocommit: false,
                ..
            }
        )));
        assert!(!r.autocommit(), "스펙이 끈다");
        assert_eq!(r.tx_pending(), 0);
        let (errs, _) = collect(&mut r, "CREATE TABLE t(id INTEGER);\n");
        assert_eq!(errs, 0);
        let (errs, _) = collect(
            &mut r,
            "INSERT INTO t VALUES (1);\nINSERT INTO t VALUES (2);\n",
        );
        assert_eq!(errs, 0);
        assert!(r.tx_open());
        assert_eq!(
            r.tx_pending(),
            3,
            "CREATE(SQLite DDL = 트랜잭션 안 변경) + INSERT 2"
        );
        let (errs, _) = collect(&mut r, "SELECT COUNT(*) FROM t;\n");
        assert_eq!(errs, 0);
        assert_eq!(r.tx_pending(), 3, "읽기는 세지 않는다");
        let (errs, _) = collect(&mut r, "COMMIT;\n");
        assert_eq!(errs, 0);
        assert!(!r.tx_open());
        assert_eq!(r.tx_pending(), 0, "커밋 뒤 0");
        // 모르는 값·없음 = 러너 모드 그대로(여기서는 끔인 채).
        let (errs, ev) = collect(&mut r, "CONNECT sqlite://x/:memory:\n");
        assert_eq!(errs, 0, "{ev:?}");
        assert!(!r.autocommit());
        let (errs, _) = collect(&mut r, "CONNECT sqlite://x/:memory:?autocommit=on\n");
        assert_eq!(errs, 0);
        assert!(r.autocommit(), "스펙이 켠다");
    }

    /// ★ T-8 스트리밍 내보내기(10-30): 5000행을 배치로 전부 넘긴다(커서 경로 · 배치 ≥ 2) · 멈춤 = 첫 배치 뒤 중단 · 끝나면 커서 닫힘.
    #[test]
    fn export_stream_batches_and_stop() {
        let mut r = seeded(5000);
        let mut batches = 0usize;
        let mut rows = 0usize;
        let total = r
            .export_stream("SELECT id FROM t ORDER BY id", &mut |rs| {
                batches += 1;
                rows += rs.rows.len();
                true
            })
            .unwrap();
        assert_eq!(total, 5000);
        assert_eq!(rows, 5000);
        assert!(batches >= 2, "배치 {batches}");
        assert!(r.cursor().is_none(), "끝나면 커서 닫힘");
        let mut seen = 0usize;
        let total = r
            .export_stream("SELECT id FROM t ORDER BY id", &mut |rs| {
                seen += rs.rows.len();
                false
            })
            .unwrap();
        assert!(
            total < 5000 && total == seen as u64,
            "첫 배치 뒤 멈춤: {total}"
        );
        assert!(r.cursor().is_none());
    }

    fn seeded(n: usize) -> Runner {
        let mut r = runner();
        let (errs, _) = collect(
            &mut r,
            &format!(
                "CREATE TABLE t(id INTEGER);\nWITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c WHERE x < {n}) INSERT INTO t SELECT x FROM c;\n"
            ),
        );
        assert_eq!(errs, 0);
        r
    }

    fn first_rs(ev: &[RunEvent]) -> (&ResultSet, bool) {
        ev.iter()
            .find_map(|e| match e {
                RunEvent::ResultSet { rs, more, .. } => Some((rs, *more)),
                _ => None,
            })
            .unwrap()
    }

    /// T-48a: 상한에서 잘린 조회는 커서를 남기고(`more` = true · 이벤트 모양은 종전 그대로) `fetch_next`가 이어 준다 ·
    /// `fetch_page`는 위치·문장이 맞으면 커서, 아니면 OFFSET · 끝에서 커서가 닫힌다 · Navigate 스팬.
    /// T-48b: `query_stream`은 커서를 열어 둔 채 첫 배치를 주고, `fetch_all`이 배치마다 진행률을 부르며 취소하면 멈춘다.
    #[test]
    fn query_stream_then_fetch_all_with_progress_and_cancel() {
        let mut r = seeded(25).with_keep_cursor(true);
        assert!(r.cursor_supported());
        let (rs, h, _) = r.query_stream("SELECT id FROM t ORDER BY id", 10).unwrap();
        assert_eq!(rs.rows.len(), 10);
        let h = h.expect("더 있으면 커서");
        assert_eq!(r.cursor().map(|c| c.1), Some(10));
        // fetch_size 5 → 배치 5(최소 2000이 적용되므로 한 번에 끝남) · 진행률은 끝까지 한 번 이상.
        let mut calls = 0;
        let (rest, stopped, _) = r
            .fetch_all(h, 0, &mut |rows, _| {
                calls += 1;
                rows < 1000
            })
            .unwrap();
        assert_eq!(rest.rows.len(), 15);
        assert!(!stopped);
        assert!(r.cursor().is_none(), "끝에 닿으면 닫힌다");
        // 취소(배치 최소 2000 → 4,500행 표): 첫 배치 뒤 false → stopped · 받은 행은 남는다 · 커서는 호스트가 닫는다.
        let mut big = seeded(4500).with_keep_cursor(true);
        let (rs, h, _) = big
            .query_stream("SELECT id FROM t ORDER BY id", 10)
            .unwrap();
        assert_eq!(rs.rows.len(), 10);
        let h = h.unwrap();
        let mut seen = Vec::new();
        let (rest, stopped, _) = big
            .fetch_all(h, 0, &mut |rows, _| {
                seen.push(rows);
                false
            })
            .unwrap();
        assert!(stopped);
        assert_eq!(rest.rows.len(), 2000);
        assert_eq!(seen, vec![2000]);
        assert!(
            big.cursor().is_some(),
            "멈춘 커서는 열려 있다(호스트가 닫음)"
        );
        big.close_cursor();
        assert!(big.cursor().is_none());
        // 예산: 남은 예산 1바이트 → 첫 배치 뒤 멈춤.
        let (_, h, _) = big
            .query_stream("SELECT id FROM t ORDER BY id", 10)
            .unwrap();
        let (_, stopped, _) = big.fetch_all(h.unwrap(), 1, &mut |_, _| true).unwrap();
        assert!(stopped);
        big.close_cursor();
        // 다 담기면 커서 없음.
        let (rs, h, _) = r.query_stream("SELECT id FROM t ORDER BY id", 100).unwrap();
        assert_eq!(rs.rows.len(), 25);
        assert!(h.is_none());
        let _ = calls;
    }

    /// ★ T-116 겹침 검사(10-29): 커서 없이(OFFSET 재질의) 다음 페이지를 받을 때 `offset-1`행 = 직전 꼬리면 그대로 이어 붙이고(꼬리 행은
    /// 결과에서 뺌 · 행 수·more 그대로), 서버가 바뀌어(삭제) 그 자리에 다른 행이 오면 오류 · 끄면 검사 없음 · 꼬리와 다른 위치면 검사 없음.
    #[test]
    fn overlap_check_detects_server_change() {
        let mut r = seeded(25).with_max_rows(10);
        r.keep_cursor = false;
        let (errs, ev) = collect(&mut r, "SELECT id FROM t ORDER BY id;\n");
        assert_eq!(errs, 0, "{ev:?}");
        let (rs, more) = first_rs(&ev);
        assert_eq!((rs.rows.len(), more), (10, true));
        assert!(r.cursor().is_none(), "커서 유지 끔");
        // 일치: 11..15 · 꼬리 행(10)은 결과에 없다 · more.
        let (rs, more, _, used) = r.fetch_page("SELECT id FROM t ORDER BY id", 10, 5).unwrap();
        assert!(!used);
        assert_eq!(rs.rows.len(), 5, "{:?}", rs.rows);
        assert_eq!(rs.rows[0][0], Value::Int(11));
        assert!(more);
        // 서버 변경: 앞쪽 행 삭제 → offset 14 자리의 행이 15가 아니라 16 → 오류(이어 붙이지 않음).
        let (errs, _) = collect(&mut r, "DELETE FROM t WHERE id = 3;\n");
        assert_eq!(errs, 0);
        let e = r
            .fetch_page("SELECT id FROM t ORDER BY id", 15, 5)
            .expect_err("겹침 불일치");
        assert!(e.message.contains("15"), "{}", e.message);
        // 꼬리와 다른 위치(20)는 검사 없이 그냥 받는다.
        let (rs, _, _, _) = r.fetch_page("SELECT id FROM t ORDER BY id", 20, 5).unwrap();
        assert_eq!(rs.rows.len(), 4);
        // 끄면 같은 불일치도 조용히 이어 붙인다(종전 동작).
        let (errs, _) = collect(&mut r, "DELETE FROM t WHERE id = 4;\n");
        assert_eq!(errs, 0);
        r.overlap_check = false;
        assert!(r.fetch_page("SELECT id FROM t ORDER BY id", 24, 5).is_ok());
    }

    #[test]
    fn cursor_fetch_next_and_fetch_page() {
        let mut r = seeded(25).with_max_rows(10);
        let (errs, ev) = collect(&mut r, "SELECT id FROM t ORDER BY id;\n");
        assert_eq!(errs, 0, "{ev:?}");
        let (rs, more) = first_rs(&ev);
        assert_eq!(rs.rows.len(), 10);
        assert!(more);
        let (h, served) = r.cursor().expect("커서 유지");
        assert_eq!(served, 10);
        // 위치가 다르면 OFFSET 재질의(커서는 그대로).
        let (rs, more, tl, used) = r
            .fetch_page("SELECT id FROM t ORDER BY id", 20, 10)
            .unwrap();
        assert!(!used);
        assert_eq!(rs.rows.len(), 5);
        assert!(!more);
        assert_eq!(tl.spans[0].stage, Stage::Navigate);
        assert!(tl.spans[0].note.as_deref().unwrap().starts_with("offset"));
        assert_eq!(r.cursor().map(|c| c.0), Some(h));
        // 위치가 맞으면 커서.
        let (rs, more, tl, used) = r
            .fetch_page("SELECT id FROM t ORDER BY id;", 10, 10)
            .unwrap();
        assert!(used);
        assert_eq!(rs.rows[0][0], Value::Int(11));
        assert!(more);
        assert_eq!(tl.spans[0].note.as_deref(), Some("cursor #1"));
        assert_eq!(r.cursor().map(|c| c.1), Some(20));
        let (rs, more, _) = r.fetch_next(h, 10).unwrap();
        assert_eq!(rs.rows.len(), 5);
        assert!(!more);
        assert!(r.cursor().is_none(), "끝 = 닫힘");
        // 옛 핸들은 오류 → fetch_page는 OFFSET으로.
        assert!(r.fetch_next(h, 10).is_err());
        let (rs, _, _, used) = r.fetch_page("SELECT id FROM t ORDER BY id", 0, 3).unwrap();
        assert!(!used);
        assert_eq!(rs.rows.len(), 3);
    }

    /// 정확히 상한만큼이면 더 없음(커서 0) · 상한 0이면 무제한 · `keep_cursor` 끄면 커서를 바로 닫는다(OFFSET 폴백만).
    #[test]
    fn exact_limit_and_fetch_mode_off() {
        let mut r = seeded(10).with_max_rows(10);
        let (_, ev) = collect(&mut r, "SELECT id FROM t;\n");
        let (rs, more) = first_rs(&ev);
        assert_eq!(rs.rows.len(), 10);
        assert!(!more);
        assert!(r.cursor().is_none());

        let mut r = seeded(30).with_max_rows(10).with_keep_cursor(false);
        let (_, ev) = collect(&mut r, "SELECT id FROM t;\n");
        let (rs, more) = first_rs(&ev);
        assert_eq!(rs.rows.len(), 10);
        assert!(more, "더 있음은 여전히 알린다");
        assert!(r.cursor().is_none(), "유지 모드 꺼짐 = 즉시 닫힘");
        let (rs, more, _, used) = r.fetch_page("SELECT id FROM t", 10, 10).unwrap();
        assert!(!used);
        assert_eq!(rs.rows.len(), 10);
        assert!(more);
    }

    /// 새 실행·COUNT: 스크립트 문장 실행은 앞 커서를 닫고, `count`는 곁가지라 커서를 살려 둔다 · `fetch_all`은 끝까지 모은다.
    #[test]
    fn new_statement_closes_cursor_but_count_keeps_it() {
        let mut r = seeded(45).with_max_rows(10);
        collect(&mut r, "SELECT id FROM t ORDER BY id;\n");
        let (h, _) = r.cursor().unwrap();
        let (n, tl) = r.count("SELECT id FROM t ORDER BY id").unwrap();
        assert_eq!(n, 45);
        assert_eq!(tl.spans[0].note.as_deref(), Some("count"));
        assert_eq!(
            r.cursor().map(|c| c.0),
            Some(h),
            "COUNT는 커서를 건드리지 않는다"
        );
        let mut calls = 0;
        let (rs, stopped, tl) = r
            .fetch_all(h, 0, &mut |_, _| {
                calls += 1;
                true
            })
            .unwrap();
        assert_eq!(rs.rows.len(), 35);
        assert_eq!(rs.rows[0][0], Value::Int(11));
        assert!(!stopped);
        assert_eq!(tl.spans[0].rows, Some(35));
        assert!(r.cursor().is_none());
        // 새 커서 → 다음 문장 실행이 닫는다.
        collect(&mut r, "SELECT id FROM t ORDER BY id;\n");
        assert!(r.cursor().is_some());
        collect(&mut r, "SELECT 1;\n");
        assert!(r.cursor().is_none());
    }

    /// `fetch_all` 예산: 바이트 예산에 닿으면 멈추고 `stopped` = true · 커서는 살아 있다.
    #[test]
    fn fetch_all_stops_at_budget() {
        let mut r = seeded(5000).with_max_rows(10);
        r.fetch_size = 100;
        collect(&mut r, "SELECT id FROM t ORDER BY id;\n");
        let (h, _) = r.cursor().unwrap();
        let (rs, stopped, _) = r.fetch_all(h, 1, &mut |_, _| true).unwrap();
        assert!(stopped);
        assert_eq!(rs.rows.len(), 2000, "첫 배치(최소 2000) 뒤 예산 판정");
        assert!(r.cursor().is_some());
        r.close_cursor();
        assert!(r.cursor().is_none());
    }

    #[test]
    fn same_sql_ignores_terminator_and_whitespace() {
        assert!(same_sql("select 1", "  select 1;\n"));
        assert!(!same_sql("select 1", "select 2"));
    }
    /// T-202 커서 결과 이어 받기 — 모의 세션(Oracle 방언 · REF CURSOR 한 개 · 행 N).
    struct RefCur {
        n: usize,
        pos: usize,
        pageable: bool,
        commits: usize,
    }

    impl RefCur {
        fn rows(&self, from: usize, to: usize) -> ResultSet {
            ResultSet {
                columns: vec![nsql_core::Column {
                    name: "ID".into(),
                    type_name: "NUMBER".into(),
                }],
                rows: (from..to).map(|i| vec![Value::Int(i as i64)]).collect(),
            }
        }
    }

    impl Session for RefCur {
        fn dialect(&self) -> Dialect {
            Dialect::Oracle
        }
        fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
            let mut r = ExecResult {
                rows_affected: Some(0),
                ..ExecResult::default()
            };
            if req.sql.contains("FIND") {
                r.out_params
                    .push(("RC".into(), Value::Cursor(nsql_core::CursorId(1))));
            }
            Ok(r)
        }
        fn fetch_cursor(&mut self, _c: nsql_core::CursorId) -> Result<ResultSet, DbError> {
            Ok(self.rows(0, self.n))
        }
        fn fetch_cursor_page(
            &mut self,
            _c: nsql_core::CursorId,
            max: usize,
        ) -> Result<(ResultSet, Option<CursorHandle>), DbError> {
            if self.pageable && max > 0 && self.n > max {
                self.pos = max;
                Ok((self.rows(0, max), Some(CursorHandle(9))))
            } else {
                Ok((self.rows(0, self.n), None))
            }
        }
        fn cursor_supported(&self) -> bool {
            self.pageable
        }
        fn fetch_next(
            &mut self,
            h: CursorHandle,
            max: usize,
        ) -> Result<(ResultSet, bool), DbError> {
            assert_eq!(h, CursorHandle(9));
            let to = if max == 0 {
                self.n
            } else {
                (self.pos + max).min(self.n)
            };
            let rs = self.rows(self.pos, to);
            self.pos = to;
            Ok((rs, to < self.n))
        }
        fn commit(&mut self) -> Result<(), DbError> {
            self.commits += 1;
            Ok(())
        }
        fn rollback(&mut self) -> Result<(), DbError> {
            Ok(())
        }
    }

    fn refcur_runner(n: usize, pageable: bool) -> Runner {
        let opener: Opener = Box::new(|_: &ConnectSpec| {
            Err(DbError {
                code: None,
                message: "no".into(),
                position: None,
            })
        });
        Runner::new(Dialect::Oracle, opener)
            .with_session(
                Box::new(RefCur {
                    n,
                    pos: 0,
                    pageable,
                    commits: 0,
                }),
                "fake oracle",
            )
            .with_max_rows(5)
    }

    /// 상한을 넘는 REF CURSOR = 첫 페이지(5행 · `more`) + 커서 유지 → 같은 문장·위치의 `fetch_page`는 커서로 이어 받는다 ·
    /// 자동 커밋은 커서가 닫힐 때(재질의 아님 · `requery_ok` = 거짓).
    #[test]
    fn refcursor_result_is_paged_through_kept_cursor() {
        let mut r = refcur_runner(12, true);
        let (errs, ev) = collect(&mut r, "VARIABLE rc REFCURSOR\nEXEC FIND(:rc)\n");
        assert_eq!(errs, 0, "{ev:?}");
        let rs = ev
            .iter()
            .find_map(|e| match e {
                RunEvent::ResultSet {
                    rs, more, label, ..
                } => Some((rs.rows.len(), *more, label.clone())),
                _ => None,
            })
            .expect("cursor result");
        assert_eq!(rs, (5, true, Some("RC".into())));
        assert!(!ev.iter().any(|e| matches!(e, RunEvent::Warning(_))));
        assert!(!Runner::requery_ok("EXEC FIND(:rc)"));
        assert!(r.cursor_matches("EXEC FIND(:rc)", 5));
        let (page, more, _, via) = r.fetch_page("EXEC FIND(:rc)", 5, 5).unwrap();
        assert!(via && more);
        assert_eq!(page.rows.len(), 5);
        assert_eq!(page.rows[0][0], Value::Int(5));
        let (last, more, _, via) = r.fetch_page("EXEC FIND(:rc)", 10, 5).unwrap();
        assert!(via && !more);
        assert_eq!(last.rows.len(), 2);
        // 다른 위치는 커서와 어긋남 → 커서 없음(호스트는 requery_ok 거짓이라 "커서 닫힘"으로 끝낸다).
        assert!(!r.cursor_matches("EXEC FIND(:rc)", 3));
    }

    /// 커서를 유지할 수 없는 드라이버 = 줄 수 제한을 무시하고 전부(12행 · `more` 없음) + ⚠ 경고 한 줄.
    #[test]
    fn refcursor_without_cursor_support_fetches_all_with_warning() {
        let mut r = refcur_runner(12, false);
        let (errs, ev) = collect(&mut r, "VARIABLE rc REFCURSOR\nEXEC FIND(:rc)\n");
        assert_eq!(errs, 0, "{ev:?}");
        assert!(ev.iter().any(|e| matches!(
            e,
            RunEvent::ResultSet { rs, more: false, .. } if rs.rows.len() == 12
        )));
        assert!(ev
            .iter()
            .any(|e| matches!(e, RunEvent::Warning(m) if m.contains("12") && m.contains('5'))));
        assert!(!r.cursor_matches("EXEC FIND(:rc)", 12));
    }

    /// 상한 안이면 커서 없음 · 경고 없음.
    #[test]
    fn refcursor_within_limit_has_no_cursor_and_no_warning() {
        let mut r = refcur_runner(3, true);
        let (errs, ev) = collect(&mut r, "VARIABLE rc REFCURSOR\nEXEC FIND(:rc)\n");
        assert_eq!(errs, 0);
        assert!(ev.iter().any(
            |e| matches!(e, RunEvent::ResultSet { rs, more: false, .. } if rs.rows.len() == 3)
        ));
        assert!(!ev.iter().any(|e| matches!(e, RunEvent::Warning(_))));
    }

    #[test]
    fn requery_ok_is_query_only() {
        assert!(Runner::requery_ok("SELECT 1 FROM DUAL"));
        assert!(Runner::requery_ok("  with c as (select 1) select * from c"));
        assert!(!Runner::requery_ok("EXEC P(:rc)"));
        assert!(!Runner::requery_ok("BEGIN p(:rc); END;"));
        assert!(!Runner::requery_ok("PRINT rc"));
    }
}

#[cfg(test)]
mod apply_changes_tests {
    //! ★ 데이터 보호 불변식(nexa-sql 87 §14 · 사용자 09-26): 사전 검사 ≠ 1 = 쓰기 0 · 실행 영향 ≠ 1 = 되돌림(자동 롤백 / 수동 세이브포인트).
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Guard {
        log: Arc<Mutex<Vec<String>>>,
        count: u64,
        affected: Vec<u64>,
        pos: usize,
        dialect: Dialect,
    }

    impl Session for Guard {
        fn dialect(&self) -> Dialect {
            self.dialect
        }
        fn execute(&mut self, req: &ExecRequest) -> Result<ExecResult, DbError> {
            self.log.lock().expect("log").push(req.sql.clone());
            let mut r = ExecResult::default();
            if req.sql.starts_with("SELECT COUNT") {
                r.result_sets.push(ResultSet {
                    columns: vec![Column {
                        name: "N".into(),
                        type_name: "int8".into(),
                    }],
                    rows: vec![vec![Value::Int(self.count as i64)]],
                });
            } else if req.sql.starts_with("UPDATE")
                || req.sql.starts_with("DELETE")
                || req.sql.starts_with("INSERT")
            {
                let n = self.affected.get(self.pos).copied().unwrap_or(1);
                self.pos += 1;
                r.rows_affected = Some(n);
            }
            Ok(r)
        }
        fn fetch_cursor(&mut self, _c: nsql_core::CursorId) -> Result<ResultSet, DbError> {
            Ok(ResultSet::default())
        }
        fn fetch_cursor_page(
            &mut self,
            _c: nsql_core::CursorId,
            _max: usize,
        ) -> Result<(ResultSet, Option<CursorHandle>), DbError> {
            Ok((ResultSet::default(), None))
        }
        fn commit(&mut self) -> Result<(), DbError> {
            self.log.lock().expect("log").push("COMMIT".into());
            Ok(())
        }
        fn rollback(&mut self) -> Result<(), DbError> {
            self.log.lock().expect("log").push("ROLLBACK".into());
            Ok(())
        }
    }

    fn runner(
        count: u64,
        affected: Vec<u64>,
        autocommit: bool,
    ) -> (Runner, Arc<Mutex<Vec<String>>>) {
        runner_of(Dialect::Postgres, count, affected, autocommit)
    }

    fn runner_of(
        dialect: Dialect,
        count: u64,
        affected: Vec<u64>,
        autocommit: bool,
    ) -> (Runner, Arc<Mutex<Vec<String>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let opener: Opener = Box::new(|_: &ConnectSpec| {
            Err(DbError {
                code: None,
                message: "no".into(),
                position: None,
            })
        });
        let r = Runner::new(dialect, opener)
            .with_session(
                Box::new(Guard {
                    log: Arc::clone(&log),
                    count,
                    affected,
                    pos: 0,
                    dialect,
                }),
                "fake",
            )
            .with_autocommit(autocommit);
        (r, log)
    }

    /// ★ 수동 모드의 `explain`(CI db2.yml 40219925165 · 10-26): 읽기 트랜잭션 자동 종료(56 L1)가 EXPLAIN 문장과 트리 질의 **사이**에
    /// 끼면 Db2 EXPLAIN 표 행이 지워져 0행 — 스크립트가 끝난 뒤 ROLLBACK 한 번(= EXPLAIN 표 정리)이어야 한다. 이미 열린 트랜잭션은 건드리지 않는다.
    #[test]
    fn explain_defers_read_end_rollback_to_script_end() {
        let (mut r, log) = runner_of(Dialect::Db2, 0, vec![], false);
        let _ = r.explain(
            "SELECT e.ID FROM NSQLT e",
            nsql_script::plan::PlanOptions { analyze: false },
        );
        let got = log.lock().expect("log").clone();
        let explain_at = got
            .iter()
            .position(|s| s.starts_with("EXPLAIN PLAN SELECTION"))
            .expect("EXPLAIN 문장");
        let tree_at = got
            .iter()
            .position(|s| s.starts_with("WITH k AS"))
            .expect("트리 질의");
        assert!(explain_at < tree_at, "{got:#?}");
        assert!(
            !got[explain_at..tree_at].iter().any(|s| s == "ROLLBACK"),
            "EXPLAIN과 트리 질의 사이에 ROLLBACK: {got:#?}"
        );
        assert_eq!(got.last().map(String::as_str), Some("ROLLBACK"), "{got:#?}");
        assert_eq!(
            got.iter().filter(|s| *s == "ROLLBACK").count(),
            1,
            "{got:#?}"
        );
        assert!(!r.tx_open(), "러너가 연 읽기 트랜잭션은 끝나 있어야 한다");
        // 사용자가 이미 변경을 쥔 트랜잭션은 explain이 끝내지 않는다.
        let (mut r, log) = runner_of(Dialect::Db2, 0, vec![1], false);
        let _ = r.apply_changes(&[stmt("UPDATE t SET a=1", true)]);
        let before = log.lock().expect("log").len();
        let _ = r.explain(
            "SELECT 1 FROM T",
            nsql_script::plan::PlanOptions { analyze: false },
        );
        let got = log.lock().expect("log").clone();
        assert!(
            !got[before..]
                .iter()
                .any(|s| s == "ROLLBACK" || s == "COMMIT"),
            "{got:#?}"
        );
        assert!(r.tx_open());
    }

    /// ★ 시작문이 없는 방언(Oracle)의 수동 모드: 적용이 성공하면 트랜잭션이 열린 것(`tx_left_open`) · 문장 기록은 사전 검사+실행문 순서.
    #[test]
    fn manual_oracle_marks_tx_open_and_logs_statements() {
        let (mut r, _log) = runner_of(Dialect::Oracle, 1, vec![1, 1], false);
        assert!(r.engine.caps.tx_begin.is_none(), "Oracle = 암묵 트랜잭션");
        assert!(!r.tx_open());
        let rep = r.apply_changes(&[
            stmt("UPDATE t SET a=1", true),
            stmt("UPDATE t SET a=2", true),
        ]);
        assert!(rep.error.is_none() && rep.done == 2);
        assert!(rep.tx_left_open, "수동 모드 · 쓴 문장 있음 = 열림");
        assert!(r.tx_open());
        let kinds: Vec<(bool, Option<u64>)> = rep.log.iter().map(|l| (l.guard, l.rows)).collect();
        assert_eq!(
            kinds,
            vec![
                (true, Some(1)),
                (true, Some(1)),
                (false, Some(1)),
                (false, Some(1))
            ]
        );
        assert!(rep.log.iter().all(|l| l.error.is_none()));
        // 사전 검사 실패 = 쓴 문장 0 · 열린 것으로 표시하지 않는다.
        let (mut r2, _) = runner_of(Dialect::Oracle, 2, vec![1], false);
        let rep2 = r2.apply_changes(&[stmt("UPDATE t SET a=1", true)]);
        assert!(rep2.error.is_some() && rep2.done == 0);
        assert!(!rep2.tx_left_open && !r2.tx_open());
        assert_eq!(rep2.log.len(), 1);
        assert!(rep2.log[0].guard && rep2.log[0].rows == Some(2));
    }

    fn stmt(sql: &str, guard: bool) -> ApplyStmt {
        ApplyStmt {
            req: ExecRequest {
                sql: sql.into(),
                params: Vec::new(),
            },
            guard: guard.then(|| ExecRequest {
                sql: format!("SELECT COUNT(*) FROM t WHERE ({sql})"),
                params: Vec::new(),
            }),
            label: format!("L:{sql}"),
        }
    }

    #[test]
    fn precheck_mismatch_writes_nothing_and_rolls_back() {
        let (mut r, log) = runner(2, vec![1, 1], true);
        let rep = r.apply_changes(&[stmt("UPDATE t SET a=1", true), stmt("DELETE FROM t", true)]);
        let e = rep.error.expect("error");
        assert_eq!(e.phase, ApplyPhase::PreCheck);
        assert_eq!(e.index, 0);
        assert_eq!(e.label, "L:UPDATE t SET a=1");
        assert!(e.message.contains("target rows = 2"));
        let l = log.lock().expect("log").clone();
        assert!(
            l.iter()
                .all(|s| !s.starts_with("UPDATE") && !s.starts_with("DELETE")),
            "쓰기 0: {l:?}"
        );
        assert!(l.contains(&"ROLLBACK".to_string()));
        assert!(rep.rolled_back && !rep.committed && rep.done == 0 && !rep.rollback_needed);
    }

    #[test]
    fn success_auto_commit_runs_guards_first() {
        let (mut r, log) = runner(1, vec![1, 1, 1], true);
        let rep = r.apply_changes(&[
            stmt("DELETE FROM t", true),
            stmt("UPDATE t SET a=1", true),
            stmt("INSERT INTO t VALUES (1)", false),
        ]);
        assert!(rep.error.is_none(), "{rep:?}");
        assert!(rep.committed && rep.done == 3);
        let l = log.lock().expect("log").clone();
        let first_write = l
            .iter()
            .position(|s| s.starts_with("DELETE"))
            .expect("delete");
        let last_guard = l
            .iter()
            .rposition(|s| s.starts_with("SELECT COUNT"))
            .expect("guard");
        assert!(last_guard < first_write, "사전 검사가 전부 먼저: {l:?}");
        assert_eq!(l.first().map(String::as_str), Some("BEGIN"));
        assert_eq!(l.last().map(String::as_str), Some("COMMIT"));
    }

    #[test]
    fn execute_mismatch_in_manual_mode_rolls_back_to_savepoint() {
        let (mut r, log) = runner(1, vec![1, 2], false);
        let rep = r.apply_changes(&[
            stmt("UPDATE t SET a=1", true),
            stmt("UPDATE t SET b=2", true),
        ]);
        let e = rep.error.expect("error");
        assert_eq!((e.phase, e.index), (ApplyPhase::Execute, 1));
        assert!(e.message.contains("affected 2 rows"));
        let l = log.lock().expect("log").clone();
        assert!(l.contains(&"SAVEPOINT nsql_edit".to_string()), "{l:?}");
        assert!(
            l.contains(&"ROLLBACK TO SAVEPOINT nsql_edit".to_string()),
            "{l:?}"
        );
        assert!(!l.contains(&"COMMIT".to_string()) && !l.contains(&"ROLLBACK".to_string()));
        assert!(rep.rolled_back && !rep.rollback_needed && rep.tx_left_open && rep.done == 0);
        assert!(r.tx_open());
    }

    #[test]
    fn success_manual_mode_releases_savepoint_and_keeps_tx_open() {
        let (mut r, log) = runner(1, vec![1], false);
        let rep = r.apply_changes(&[stmt("UPDATE t SET a=1", true)]);
        assert!(rep.error.is_none() && rep.tx_left_open && !rep.committed && rep.done == 1);
        let l = log.lock().expect("log").clone();
        assert!(
            l.contains(&"RELEASE SAVEPOINT nsql_edit".to_string()),
            "{l:?}"
        );
        assert!(!l.contains(&"COMMIT".to_string()));
    }
}

#[cfg(test)]
mod hold_tests {
    use super::*;

    #[test]
    fn hold_decision_mcdc() {
        let p = HoldPolicy {
            rows: 0,
            blocks: true,
            no_where: true,
            commit_ok: false,
        };
        // 기준 0 = 변경이 있으면 늘(행 0이어도 — 호출자는 변경 문장이 있을 때만 묻는다).
        assert!(hold_decision(&p, 0, false, false));
        let p5 = HoldPolicy { rows: 5, ..p };
        assert!(!hold_decision(&p5, 4, false, false));
        assert!(hold_decision(&p5, 5, false, false));
        // 블록·WHERE 없음은 각자 스위치로만.
        assert!(hold_decision(&p5, 1, true, false));
        assert!(hold_decision(&p5, 1, false, true));
        let off = HoldPolicy {
            blocks: false,
            no_where: false,
            ..p5
        };
        assert!(!hold_decision(&off, 1, true, true));
    }

    #[test]
    fn commit_sql_and_where_detection() {
        assert!(is_commit_sql("COMMIT"));
        assert!(is_commit_sql("commit work;"));
        assert!(is_commit_sql("END"));
        assert!(!is_commit_sql("ROLLBACK"));
        assert!(!is_commit_sql("ROLLBACK TO sp1"));
        assert!(!is_commit_sql("BEGIN NULL; END;"));
        assert!(dml_without_where("UPDATE t SET a = 1"));
        assert!(dml_without_where("DELETE FROM hr.emp"));
        assert!(!dml_without_where("UPDATE t SET a = 1 WHERE id = 3"));
        assert!(!dml_without_where(
            "DELETE FROM t WHERE id IN (SELECT id FROM x)"
        ));
        // 서브쿼리 안의 WHERE는 깊이 1 = 세지 않음 · 문자열·주석 안의 WHERE도 무시.
        assert!(dml_without_where(
            "UPDATE t SET a = (SELECT MAX(v) FROM x WHERE x.k = 1)"
        ));
        assert!(dml_without_where("UPDATE t SET a = 'where' -- where\n"));
        assert!(!dml_without_where("INSERT INTO t VALUES (1)"));
        assert!(!dml_without_where("SELECT 1"));
        assert!(!dml_without_where(
            "MERGE INTO t USING s ON (1=1) WHEN MATCHED THEN UPDATE SET a = 1"
        ));
    }
}

/// T-371: 보낸 문장(`sent`)의 글자 위치 `pos`를 원문 문장(`orig`) 위치로 — 두 문장이 같거나 **`pos` 앞까지 같으면** 그대로 ·
/// 아니면 `None`(바인드 재작성·프리펜드·치환이 그 앞을 바꿨다). 순수 함수.
#[must_use]
pub fn map_error_position(sent: &str, orig: &str, pos: usize) -> Option<usize> {
    if sent == orig {
        return Some(pos);
    }
    let mut a = sent.chars();
    let mut b = orig.chars();
    for _ in 0..pos {
        match (a.next(), b.next()) {
            (Some(x), Some(y)) if x == y => {}
            _ => return None,
        }
    }
    Some(pos)
}

#[cfg(test)]
mod errpos_tests {
    use super::map_error_position;

    #[test]
    fn position_maps_when_prefix_unchanged() {
        assert_eq!(
            map_error_position("SELECT x FROM t", "SELECT x FROM t", 9),
            Some(9),
            "같은 문장"
        );
        assert_eq!(
            map_error_position(
                "SELECT x FROM t WHERE a = $1",
                "SELECT x FROM t WHERE a = :v",
                9
            ),
            Some(9),
            "재작성이 위치 뒤 = 그대로"
        );
        assert_eq!(
            map_error_position("DECLARE @v INT;\nSELECT x FROM t", "SELECT x FROM t", 20),
            None,
            "프리펜드 = 앞이 다르다 → 버림"
        );
        assert_eq!(
            map_error_position("SELECT 가 FROM t", "SELECT 가 FROM t", 8),
            Some(8)
        );
        assert_eq!(
            map_error_position("abc", "abd", 3),
            None,
            "위치 직전 글자가 다르다"
        );
        assert_eq!(
            map_error_position("abc", "abd", 2),
            Some(2),
            "위치 앞까지는 같다"
        );
        assert_eq!(
            map_error_position("ab", "abcd", 3),
            None,
            "보낸 문장이 더 짧다"
        );
    }
}
