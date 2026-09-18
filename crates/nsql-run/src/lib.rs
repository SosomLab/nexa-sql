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

pub mod meta;
pub mod txlog;

use nsql_core::{
    Column, CursorHandle, DbError, Dialect, ExecRequest, ExecResult, ResultSet, Session, Stage,
    Timeline, Value,
};
use nsql_i18n::{t, tf, Msg};
use nsql_script::{
    parse_spool, split_script, Action, ConnectSpec, Engine, Item, ItemKind, PrepareMode, Prepared,
    SpoolCmd, SpoolMode, SqlKind,
};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// `@`/`@@` 중첩 상한(자기 자신을 부르는 스크립트 보호).
const MAX_SCRIPT_DEPTH: usize = 32;

/// 호스트로 흘러가는 이벤트.
#[derive(Debug)]
pub enum RunEvent {
    /// 항목 실행 시작(줄 · 종류 요약).
    Begin {
        index: usize,
        line: usize,
        summary: String,
    },
    /// 조회 결과. `more` = 페치 상한(`Runner::max_rows`)에서 잘렸다(서버에 행이 더 있다).
    ResultSet {
        index: usize,
        rs: ResultSet,
        elapsed: Duration,
        more: bool,
    },
    /// DML/DDL 완료.
    Done {
        index: usize,
        rows_affected: Option<u64>,
        elapsed: Duration,
    },
    /// `PRINT`·`VARIABLE` 목록 등.
    Print {
        pairs: Vec<(String, Value)>,
    },
    /// 서버 메시지(DBMS_OUTPUT · T-SQL PRINT) · 엔진 정보.
    Message(String),
    Connected {
        description: String,
        dialect: Dialect,
    },
    Disconnected,
    Error {
        index: usize,
        line: usize,
        error: DbError,
    },
    /// 항목 하나의 단계별 소요(docs/26) — 결과 이벤트 뒤에 온다. CLI `--timing` · GUI 상태줄.
    Timing {
        index: usize,
        timeline: Timeline,
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
            ..
        } => vec![LogEntry::new(LogKind::Done, t(Msg::LogDone))
            .rows(*rows_affected)
            .elapsed(*elapsed)],
        RunEvent::Print { pairs } => vec![LogEntry::new(
            LogKind::Info,
            tf(
                Msg::LogPrint,
                &[&pairs
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")],
            ),
        )],
        RunEvent::Message(m) => vec![LogEntry::new(LogKind::Info, m.clone())],
        RunEvent::Connected {
            description,
            dialect,
        } => vec![LogEntry::new(
            LogKind::Connect,
            format!("{description} ({dialect})"),
        )],
        RunEvent::Disconnected => vec![LogEntry::new(LogKind::Disconnect, "")],
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
    pub engine: Engine,
    pub session: Option<Box<dyn Session>>,
    opener: Opener,
    /// `CONNECT <프로필>`(사용자명만 있고 나머지가 빈 스펙)을 저장소에서 푼다. 없으면 그대로 접속.
    pub resolver: Option<Resolver>,
    /// 마지막 접속 설명(상태줄).
    pub connection: Option<String>,
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
}

/// 러너가 아는 열린 커서의 상태(핸들 · 원문 · 지금까지 넘긴 행 수).
#[derive(Debug)]
struct OpenCursor {
    handle: CursorHandle,
    /// 잘린 결과를 만든 문장 원문(호스트의 "더 가져오기"가 같은 문장인지 확인).
    sql: String,
    /// 호스트에 넘긴 행 수(= 다음 페치의 OFFSET) — `fetch_page`가 커서/OFFSET 중 어느 쪽을 쓸지 판정.
    served: usize,
    /// 자동 커밋을 커서가 닫힐 때까지 미뤘는가(PG WITHOUT HOLD 커서 보호 · 닫을 때 커밋).
    deferred_commit: bool,
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
            max_rows: 0,
            message_sink: None,
            spool: None,
            strict: false,
            script_dirs: Vec::new(),
            cursor: None,
            keep_cursor: true,
            fetch_size: 0,
            cursor_idle_secs: 0,
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
        self
    }

    /// 실시간 메시지 싱크 장착(체이닝).
    #[must_use]
    pub fn with_message_sink(mut self, sink: nsql_core::MessageSink) -> Self {
        self.message_sink = Some(sink);
        self
    }

    /// 기본 자동 커밋(설정 `session.autocommit` · 스크립트 `SET AUTOCOMMIT`이 우선).
    #[must_use]
    pub fn with_autocommit(mut self, on: bool) -> Self {
        self.engine.settings.autocommit = on;
        self
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
        let prev = self.max_rows;
        self.set_max_rows(0);
        let r = match self.session.as_mut() {
            Some(s) => {
                let t = Instant::now();
                s.execute(&ExecRequest {
                    sql: sql.to_string(),
                    params: Vec::new(),
                })
                .map(|res| {
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
                deferred_commit: false,
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

    /// 열린 커서를 닫는다(없으면 무시). 미뤄 둔 자동 커밋이 있으면 여기서 커밋한다.
    pub fn close_cursor(&mut self) {
        if let Some(c) = self.cursor.take() {
            if let Some(s) = self.session.as_mut() {
                let _ = s.close_cursor(c.handle);
                if c.deferred_commit {
                    let _ = s.commit();
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
                        if c.deferred_commit {
                            let _ = session.commit();
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
        let batch = self.fetch_size.max(2000);
        let mut acc = ResultSet::default();
        let mut bytes: u64 = 0;
        let mut timeline = Timeline::new();
        let t0 = Instant::now();
        let mut stopped = false;
        loop {
            let (mut rs, more, _) = self.fetch_next(h, batch)?;
            bytes += rs.approx_bytes();
            if acc.columns.is_empty() {
                acc.columns = std::mem::take(&mut rs.columns);
            }
            acc.rows.append(&mut rs.rows);
            if !more {
                break;
            }
            let over_budget = budget_bytes > 0 && bytes >= budget_bytes;
            if over_budget || !progress(acc.rows.len() as u64, bytes) {
                stopped = true;
                break;
            }
        }
        let span = timeline.push(Stage::Navigate, t0.elapsed());
        span.rows = Some(acc.rows.len() as u64);
        span.bytes = Some(bytes);
        span.note = Some(
            if stopped {
                "fetch all (stopped)"
            } else {
                "fetch all"
            }
            .into(),
        );
        Ok((acc, stopped, timeline))
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
        let (q, max) = if limit == 0 {
            (nsql_io::paging::page_sql(d, sql, offset, 0), 0)
        } else {
            (nsql_io::paging::page_sql(d, sql, offset, limit + 1), limit)
        };
        let t0 = Instant::now();
        let (rs, more, _) = self.query_once(&q, max)?;
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
                    return Ok((rs, more, tl, true));
                }
            }
        }
        self.fetch_offset(sql, offset, limit)
            .map(|(rs, more, tl)| (rs, more, tl, false))
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
        self.engine.dialect = session.dialect();
        self.session = Some(session);
        self.connection = Some(description.into());
        self.push_max_rows();
        self
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
        match (self.opener)(spec) {
            Ok(mut session) => {
                let dialect = session.dialect();
                self.engine.dialect = dialect;
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
                self.push_max_rows();
                emit(RunEvent::Connected {
                    description: spec.redacted(),
                    dialect,
                });
                true
            }
            Err(e) => {
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
        let items = split_script(src);
        let mut errors = 0;
        for (i, item) in items.iter().enumerate() {
            if !self.run_item(i, item, prompt, emit) {
                errors += 1;
                if self.engine.settings.exit_on_error {
                    break;
                }
            }
        }
        if pushed.is_some() {
            self.script_dirs.pop();
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
        emit(RunEvent::Begin {
            index,
            line: item.line,
            summary: summary(item),
        });
        let mut guard = 0;
        loop {
            let actions = self.engine.plan(item);
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
                ..
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
                self.execute(index, item, prepared, expect_out, emit)
            }
            Action::LocalAssign { name, value } => {
                emit(RunEvent::Message(format!(":{name} = {}", value.display())));
                true
            }
            Action::Print(pairs) => {
                // 커서 값은 1회 fetch해 결과 집합으로.
                let mut plain = Vec::new();
                for (n, v) in pairs {
                    if let Value::Cursor(c) = v {
                        match self.session.as_mut().map(|s| s.fetch_cursor(c)) {
                            Some(Ok(rs)) => {
                                emit(RunEvent::Message(format!("PRINT {n} (refcursor)")));
                                let (rs, more) = self.trim_rows(rs);
                                emit(RunEvent::ResultSet {
                                    index,
                                    rs,
                                    elapsed: Duration::ZERO,
                                    more,
                                });
                                self.engine.vars.assign(&n, Value::Null);
                            }
                            Some(Err(e)) => {
                                emit(RunEvent::Error {
                                    index,
                                    line: item.line,
                                    error: e,
                                });
                                return false;
                            }
                            None => {
                                emit(RunEvent::Error {
                                    index,
                                    line: item.line,
                                    error: msg_err(t(Msg::NoSession)),
                                });
                                return false;
                            }
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
                emit(RunEvent::Disconnected);
                true
            }
            Action::Describe(o) => self.describe(index, item, &o, emit),
            Action::Show(w) => {
                let w_up = w.trim().to_ascii_uppercase();
                // `SHOW TABLES|VIEWS|<kind>` — 카탈로그 목록(T-52 · psql `\dt` · sqlite `.tables` 별칭의 종착).
                if !matches!(w_up.as_str(), "USER" | "VARIABLES" | "VAR") {
                    if let Some(kind) = nsql_catalog::ObjectKind::parse(&w_up.to_ascii_lowercase())
                    {
                        return self.show_objects(index, item, kind, emit);
                    }
                }
                let text = match w_up.as_str() {
                    "USER" => format!("USER = {}", self.connection.clone().unwrap_or_default()),
                    "VARIABLES" | "VAR" => self
                        .engine
                        .vars
                        .iter()
                        .map(|(n, v)| format!("{n} = {}", v.value.display()))
                        .collect::<Vec<_>>()
                        .join("\n"),
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
            Action::SetOption { name, value } => {
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
        prepared: Prepared,
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
        let names: Vec<String> = prepared.params.iter().map(|p| p.name.clone()).collect();
        let mode = prepared.mode;
        let line_offset = prepared.line_offset;
        let started = Instant::now();
        let request = prepared.into_request();
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
                if expect_out && result.out_params.is_empty() {
                    absorb_from_result_set(&mut result, &names);
                }
                for m in result.messages.drain(..) {
                    emit(RunEvent::Message(m));
                }
                let n_sets = result.result_sets.len();
                let max_rows = self.max_rows;
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
                            deferred_commit: false,
                            last_used: Instant::now(),
                        });
                    } else if let Some(s) = self.session.as_mut() {
                        let _ = s.close_cursor(h);
                    }
                }
                for (i, rs) in result.result_sets.drain(..).enumerate() {
                    let (rs, trimmed) = trim_rows(rs, max_rows);
                    let more = trimmed || (i + 1 == n_sets && pending.is_some());
                    emit(RunEvent::ResultSet {
                        index,
                        rs,
                        elapsed,
                        more,
                    });
                }
                if n_sets == 0 || result.rows_affected.is_some() {
                    emit(RunEvent::Done {
                        index,
                        rows_affected: result.rows_affected,
                        elapsed,
                    });
                }
                if let Some(pairs) = self.engine.absorb(&result) {
                    emit(RunEvent::Print { pairs });
                }
                if self.engine.settings.autocommit {
                    match kept.as_mut() {
                        // 커서가 살아 있는 동안은 커밋을 미룬다(PG WITHOUT HOLD 커서 보호 · 닫힐 때 커밋).
                        Some(c) => c.deferred_commit = true,
                        None => {
                            if let Some(s) = self.session.as_mut() {
                                let t = Instant::now();
                                let _ = s.commit();
                                timeline.push(Stage::Commit, t.elapsed());
                            }
                        }
                    }
                }
                self.cursor = kept;
                emit(RunEvent::Timing { index, timeline });
                // ★ 컴파일 결과(사용자 09-15 "객체 생성·수정"): Oracle은 CREATE가 성공해도 INVALID일 수 있다 —
                // SQL*Plus의 "Warning: … created with compilation errors" + SHOW ERRORS를 한 번에.
                self.report_compile_errors(index, item, emit)
            }
            Err(mut e) => {
                if mode == PrepareMode::DeclarePrepend {
                    e.message
                        .push_str(&format!(" (프리펜드 {line_offset}줄 보정 필요)"));
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
    fn trim_rows(&self, rs: ResultSet) -> (ResultSet, bool) {
        trim_rows(rs, self.max_rows)
    }

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
            if fetch_size > 0 {
                let _ = s.set_option("fetch_size", &fetch_size.to_string());
            }
            s.set_message_sink(sink);
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
        if self.engine.dialect != Dialect::Oracle {
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
        match nsql_catalog::compile_errors(session.as_mut(), &owner, &name_up) {
            Ok(errs) if !errs.is_empty() => {
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
        let dialect = session.dialect();
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
        let name = match dialect {
            Dialect::Oracle => {
                if object.contains('"') {
                    name
                } else {
                    name.to_ascii_uppercase()
                }
            }
            Dialect::Postgres => {
                if object.contains('"') {
                    name
                } else {
                    name.to_ascii_lowercase()
                }
            }
            _ => name,
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

#[cfg(test)]
mod tests {
    use super::*;
    use nsql_driver_sqlite::SqliteSession;

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

    fn collect(r: &mut Runner, src: &str) -> (usize, Vec<RunEvent>) {
        let mut ev = Vec::new();
        let mut no_prompt = |_: &str| None;
        let errs = r.run_script(src, &mut no_prompt, &mut |e| ev.push(e));
        (errs, ev)
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
        // 느슨한 기본은 그대로 프롬프트.
        let mut loose = runner();
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

    #[test]
    fn connect_via_opener_and_substitution_prompt() {
        let mut r = runner();
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
}
