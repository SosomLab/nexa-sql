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

use nsql_core::{Column, DbError, Dialect, ExecResult, ResultSet, Session, Stage, Timeline, Value};
use nsql_script::{
    split_script, Action, ConnectSpec, Engine, Item, ItemKind, PrepareMode, Prepared, SqlKind,
};
use std::time::{Duration, Instant};

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
                format!("line {line}: {summary}"),
            )]
        }
        RunEvent::ResultSet { rs, elapsed, .. } => vec![LogEntry::new(LogKind::Done, "result set")
            .rows(rs.rows.len() as u64)
            .elapsed(*elapsed)],
        RunEvent::Done {
            rows_affected,
            elapsed,
            ..
        } => vec![LogEntry::new(LogKind::Done, "done")
            .rows(*rows_affected)
            .elapsed(*elapsed)],
        RunEvent::Print { pairs } => vec![LogEntry::new(
            LogKind::Info,
            format!(
                "PRINT {}",
                pairs
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
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
            format!("line {line}: {}", error.message),
        )],
        RunEvent::Timing { timeline, .. } => timeline
            .spans
            .iter()
            .map(|s| {
                let (kind, msg) = match s.stage {
                    Stage::Send => (LogKind::Send, "sent"),
                    Stage::Execute => (LogKind::Execute, "first response"),
                    Stage::Fetch => (LogKind::Fetch, "fetched"),
                    Stage::OutputFlush => (LogKind::Output, "server output"),
                    Stage::Commit => (LogKind::Commit, "commit"),
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
        }
    }

    /// 실시간 메시지 싱크 장착(체이닝).
    #[must_use]
    pub fn with_message_sink(mut self, sink: nsql_core::MessageSink) -> Self {
        self.message_sink = Some(sink);
        self
    }

    /// 페치 상한 장착(체이닝 · 0 = 무제한).
    #[must_use]
    pub fn with_max_rows(mut self, n: usize) -> Self {
        self.max_rows = n;
        self
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

    /// 스크립트 전체 실행. 오류 시 `WHENEVER SQLERROR EXIT`면 중단. 반환 = 오류 수.
    pub fn run_script(
        &mut self,
        src: &str,
        prompt: Prompter<'_>,
        emit: &mut dyn FnMut(RunEvent),
    ) -> usize {
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
        errors
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
                match prompt(name) {
                    Some(v) => {
                        self.engine.define(name, &v);
                        guard += 1;
                        if guard > 64 {
                            emit(RunEvent::Error {
                                index,
                                line: item.line,
                                error: msg_err("치환 변수 루프"),
                            });
                            return false;
                        }
                        continue;
                    }
                    None => {
                        emit(RunEvent::Error {
                            index,
                            line: item.line,
                            error: msg_err(format!("치환 변수 &{name} 미정의")),
                        });
                        return false;
                    }
                }
            }
            let mut ok = true;
            for a in actions {
                if !self.perform(index, item, a, emit) {
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
        emit: &mut dyn FnMut(RunEvent),
    ) -> bool {
        match action {
            Action::Execute {
                prepared,
                expect_out,
                ..
            } => self.execute(index, item, prepared, expect_out, emit),
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
                                    error: msg_err("접속이 없습니다"),
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
                let text = match w_up.as_str() {
                    "USER" => format!("USER = {}", self.connection.clone().unwrap_or_default()),
                    "VARIABLES" | "VAR" => self
                        .engine
                        .vars
                        .iter()
                        .map(|(n, v)| format!("{n} = {}", v.value.display()))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => format!("SHOW {w}: 미지원(설정 = {:?})", self.engine.settings.other),
                };
                emit(RunEvent::Message(text));
                true
            }
            Action::Spool(t) => {
                emit(RunEvent::Message(format!("SPOOL {t}: 호스트 미구현(T-9)")));
                true
            }
            Action::Prompt(t) => {
                emit(RunEvent::Message(t));
                true
            }
            Action::RunScript { path, .. } => match std::fs::read_to_string(&path) {
                Ok(src) => {
                    let mut no_prompt = |_: &str| None;
                    let errs = self.run_script(&src, &mut no_prompt, emit);
                    errs == 0
                }
                Err(e) => {
                    emit(RunEvent::Error {
                        index,
                        line: item.line,
                        error: msg_err(format!("{path}: {e}")),
                    });
                    false
                }
            },
            Action::NeedInput { name } => {
                emit(RunEvent::Error {
                    index,
                    line: item.line,
                    error: msg_err(format!("치환 변수 &{name} 미정의")),
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

    fn execute(
        &mut self,
        index: usize,
        item: &Item,
        prepared: Prepared,
        expect_out: bool,
        emit: &mut dyn FnMut(RunEvent),
    ) -> bool {
        let Some(session) = self.session.as_mut() else {
            emit(RunEvent::Error {
                index,
                line: item.line,
                error: msg_err("접속이 없습니다 — CONNECT 먼저"),
            });
            return false;
        };
        let names: Vec<String> = prepared.params.iter().map(|p| p.name.clone()).collect();
        let mode = prepared.mode;
        let line_offset = prepared.line_offset;
        let started = Instant::now();
        match session.execute(&prepared.into_request()) {
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
                for rs in result.result_sets.drain(..) {
                    let (rs, more) = trim_rows(rs, max_rows);
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
                    if let Some(s) = self.session.as_mut() {
                        let t = Instant::now();
                        let _ = s.commit();
                        timeline.push(Stage::Commit, t.elapsed());
                    }
                }
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

    /// 드라이버에 페치 상한(+1 · 초과 여부 판정용)을 알린다 — 접속 직후 · 상한 변경 시.
    fn push_max_rows(&mut self) {
        let n = self.max_rows;
        let sink = self.message_sink.clone();
        if let Some(s) = self.session.as_mut() {
            let v = if n == 0 { 0 } else { n + 1 };
            let _ = s.set_option("max_rows", &v.to_string());
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
                error: msg_err("접속이 없습니다 — CONNECT 먼저"),
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
}
