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

use nsql_core::{DbError, Dialect, ExecResult, ResultSet, Session, Value};
use nsql_script::{
    split_script, Action, ConnectSpec, Engine, Item, ItemKind, PrepareMode, Prepared,
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
    /// 조회 결과.
    ResultSet {
        index: usize,
        rs: ResultSet,
        elapsed: Duration,
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
}

/// 접속 열기 — 호스트가 드라이버 레지스트리로 주입한다.
pub type Opener = Box<dyn FnMut(&ConnectSpec) -> Result<Box<dyn Session>, DbError>>;
/// 치환 변수 프롬프트 — `None`이면 실행 중단.
pub type Prompter<'a> = &'a mut dyn FnMut(&str) -> Option<String>;

#[allow(missing_debug_implementations)]
pub struct Runner {
    pub engine: Engine,
    pub session: Option<Box<dyn Session>>,
    opener: Opener,
    /// 마지막 접속 설명(상태줄).
    pub connection: Option<String>,
}

impl Runner {
    pub fn new(dialect: Dialect, opener: Opener) -> Self {
        Runner {
            engine: Engine::new(dialect),
            session: None,
            opener,
            connection: None,
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
        self
    }

    pub fn connect(&mut self, spec: &ConnectSpec, emit: &mut dyn FnMut(RunEvent)) -> bool {
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
                                emit(RunEvent::ResultSet {
                                    index,
                                    rs,
                                    elapsed: Duration::ZERO,
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
            Action::Describe(o) => {
                emit(RunEvent::Message(format!(
                    "DESCRIBE {o}: 카탈로그 포트는 M3(T-20)"
                )));
                true
            }
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
                if expect_out && result.out_params.is_empty() {
                    absorb_from_result_set(&mut result, &names);
                }
                for m in result.messages.drain(..) {
                    emit(RunEvent::Message(m));
                }
                let n_sets = result.result_sets.len();
                for rs in result.result_sets.drain(..) {
                    emit(RunEvent::ResultSet { index, rs, elapsed });
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
                        let _ = s.commit();
                    }
                }
                true
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
