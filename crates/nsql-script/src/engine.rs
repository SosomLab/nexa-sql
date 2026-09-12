//! 엔진 — 항목을 **행동**으로 계획한다. DB를 모른다(호스트가 실행하고 결과를 되돌려 준다).
//!
//! 세션 변수는 SQL*Plus처럼 클라이언트에만 산다(docs/04 §8.2). 대입 우변이 리터럴이면
//! DB 왕복 없이 로컬 대입([`Action::LocalAssign`]) — 그래서 `EXEC :V := 'x'`는 **모든 방언에서**
//! 똑같이 동작하고, MSSQL에서도 이후 문장이 같은 변수를 본다.

use crate::command::{parse_literal, Command, SetOption};
use crate::connect::ConnectSpec;
use crate::dialect::{prepare, wrap_exec, Prepared};
use crate::split::{Item, ItemKind, SqlKind};
use crate::vars::VarStore;
use nsql_core::{Dialect, ExecResult, Value};
use std::collections::BTreeMap;

/// 호스트가 수행할 행동.
#[derive(Clone, PartialEq, Debug)]
pub enum Action {
    /// 드라이버에 실행 요청. `expect_out`이면 결과의 OUT 값을 [`Engine::absorb`]로 돌려줘야 한다.
    Execute {
        prepared: Prepared,
        expect_out: bool,
        kind: SqlKind,
    },
    /// DB 왕복 없이 끝난 대입.
    LocalAssign {
        name: String,
        value: Value,
    },
    /// `PRINT` — 표시할 이름·값. 커서면 호스트가 `fetch_cursor`.
    Print(Vec<(String, Value)>),
    Connect(ConnectSpec),
    Disconnect,
    Describe(String),
    Show(String),
    Spool(String),
    Prompt(String),
    RunScript {
        path: String,
        args: Vec<String>,
        relative_to_caller: bool,
    },
    /// 치환 변수(`&name`)가 미정의 — 호스트가 값을 받아 [`Engine::define`] 후 같은 항목을 재계획.
    NeedInput {
        name: String,
    },
    /// 세션에 전달할 옵션(`SET SERVEROUTPUT ON` → `("serveroutput","on")` · `SET ARRAYSIZE` → `("fetch_size","n")`).
    SetOption {
        name: String,
        value: String,
    },
    /// 아무것도 안 함(REM · SET 처리 완료 · VARIABLE 선언 등). 메시지는 상태줄용.
    Nothing(String),
    Error(String),
}

/// 계획 부산물 — 경고·정보.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Diagnostic {
    ImplicitVariable(String),
    Info(String),
}

/// 엔진 설정 — `SET`이 바꾼다.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Settings {
    pub serveroutput: bool,
    pub timing: bool,
    pub autocommit: bool,
    pub autoprint: bool,
    pub feedback: bool,
    /// 치환 변수 접두 문자. `None` = `SET DEFINE OFF`.
    pub define_char: Option<char>,
    pub verify: bool,
    pub echo: bool,
    pub fetch_size: u32,
    pub sqlformat: String,
    pub exit_on_error: bool,
    pub other: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            serveroutput: false,
            timing: false,
            autocommit: false,
            autoprint: false,
            feedback: true,
            define_char: Some('&'),
            verify: true,
            echo: false,
            fetch_size: 500,
            sqlformat: "grid".into(),
            exit_on_error: false,
            other: BTreeMap::new(),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Engine {
    pub dialect: Dialect,
    pub vars: VarStore,
    /// 치환 변수(`DEFINE` · `:setvar` · `&1..&n`).
    pub defines: BTreeMap<String, String>,
    pub settings: Settings,
    pub diagnostics: Vec<Diagnostic>,
}

impl Engine {
    pub fn new(dialect: Dialect) -> Self {
        Engine {
            dialect,
            vars: VarStore::new(),
            defines: BTreeMap::new(),
            settings: Settings::default(),
            diagnostics: Vec::new(),
        }
    }

    pub fn define(&mut self, name: &str, value: &str) {
        self.defines
            .insert(name.to_ascii_uppercase(), value.to_string());
    }

    /// 스크립트 인자 `&1..&n`.
    pub fn set_args(&mut self, args: &[String]) {
        for (i, a) in args.iter().enumerate() {
            self.defines.insert((i + 1).to_string(), a.clone());
        }
    }

    /// 항목 하나를 계획한다. 여러 행동이 나올 수 있다(예: 자동 PRINT).
    pub fn plan(&mut self, item: &Item) -> Vec<Action> {
        let text = match self.substitute(&item.text) {
            Ok(t) => t,
            Err(name) => return vec![Action::NeedInput { name }],
        };
        match &item.kind {
            ItemKind::Invalid(msg) => vec![Action::Error(msg.clone())],
            ItemKind::Sql(kind) => {
                let inout = matches!(kind, SqlKind::Block);
                let prepared = prepare(self.dialect, &text, &mut self.vars, inout);
                self.note_implicit(&prepared);
                vec![Action::Execute {
                    prepared,
                    expect_out: inout,
                    kind: *kind,
                }]
            }
            ItemKind::Command(cmd) => self.plan_command(cmd, &text),
        }
    }

    fn note_implicit(&mut self, p: &Prepared) {
        for n in &p.implicit {
            self.diagnostics
                .push(Diagnostic::ImplicitVariable(n.clone()));
        }
    }

    fn plan_command(&mut self, cmd: &Command, text: &str) -> Vec<Action> {
        match cmd {
            Command::Variable { name: None, .. } => {
                let list: Vec<(String, Value)> = self
                    .vars
                    .iter()
                    .map(|(n, v)| (n.clone(), v.value.clone()))
                    .collect();
                vec![Action::Print(list)]
            }
            Command::Variable {
                name: Some(n),
                ty: None,
                ..
            } => match self.vars.get(n) {
                Some(v) => vec![Action::Nothing(format!("variable {n} {:?}", v.ty))],
                None => vec![Action::Error(format!("변수 {n}가 없습니다"))],
            },
            Command::Variable {
                name: Some(n),
                ty: Some(t),
                init,
            } => {
                self.vars.declare(n, t.clone(), init.clone());
                vec![Action::Nothing(format!("variable {n} 선언"))]
            }
            Command::Print { names } => {
                if names.is_empty() {
                    let list = self
                        .vars
                        .iter()
                        .map(|(n, v)| (n.clone(), v.value.clone()))
                        .collect();
                    return vec![Action::Print(list)];
                }
                let mut out = Vec::new();
                for n in names {
                    match self.vars.get(n) {
                        Some(v) => out.push((n.clone(), v.value.clone())),
                        None => return vec![Action::Error(format!("변수 {n}가 없습니다"))],
                    }
                }
                vec![Action::Print(out)]
            }
            Command::Exec { .. } => self.plan_exec(text),
            Command::Connect(spec) => vec![Action::Connect(spec.clone())],
            Command::Disconnect => vec![Action::Disconnect],
            Command::Set(opt) => {
                self.apply_set(opt);
                match opt {
                    SetOption::ServerOutput { on } => {
                        vec![Action::SetOption {
                            name: "serveroutput".into(),
                            value: if *on { "on" } else { "off" }.into(),
                        }]
                    }
                    SetOption::FetchSize(n) => vec![Action::SetOption {
                        name: "fetch_size".into(),
                        value: n.to_string(),
                    }],
                    SetOption::AutoCommit(b) => vec![Action::SetOption {
                        name: "autocommit".into(),
                        value: b.to_string(),
                    }],
                    _ => vec![Action::Nothing(format!("{opt:?}"))],
                }
            }
            Command::Define { name: None, .. } => {
                let list = self
                    .defines
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::Str(v.clone())))
                    .collect();
                vec![Action::Print(list)]
            }
            Command::Define {
                name: Some(n),
                value: Some(v),
            } => {
                self.define(n, v);
                vec![Action::Nothing(format!("define {n}"))]
            }
            Command::Define {
                name: Some(n),
                value: None,
            } => match self.defines.get(n) {
                Some(v) => vec![Action::Print(vec![(n.clone(), Value::Str(v.clone()))])],
                None => vec![Action::Error(format!("치환 변수 {n}가 없습니다"))],
            },
            Command::Undefine { names } => {
                for n in names {
                    self.defines.remove(n);
                }
                vec![Action::Nothing("undefine".into())]
            }
            Command::SetVar { name, value } => {
                match value {
                    Some(v) => self.define(name, v),
                    None => {
                        self.defines.remove(&name.to_ascii_uppercase());
                    }
                }
                vec![Action::Nothing(format!("setvar {name}"))]
            }
            Command::Describe { object } => vec![Action::Describe(object.clone())],
            Command::Show { what } => vec![Action::Show(what.clone())],
            Command::Spool { target } => vec![Action::Spool(target.clone())],
            Command::Prompt { text } => vec![Action::Prompt(text.clone())],
            Command::Run {
                path,
                args,
                relative_to_caller,
            } => {
                vec![Action::RunScript {
                    path: path.clone(),
                    args: args.clone(),
                    relative_to_caller: *relative_to_caller,
                }]
            }
            Command::Include { path } => vec![Action::RunScript {
                path: path.clone(),
                args: Vec::new(),
                relative_to_caller: true,
            }],
            Command::Remark => vec![Action::Nothing("rem".into())],
            Command::Go { .. } => vec![Action::Nothing("go".into())],
            Command::Whenever { on_error_exit } => {
                self.settings.exit_on_error = *on_error_exit;
                vec![Action::Nothing("whenever".into())]
            }
        }
    }

    /// `EXEC 본문`: 리터럴 대입이면 로컬, 아니면 방언별 래핑 후 InOut 실행.
    fn plan_exec(&mut self, text: &str) -> Vec<Action> {
        // text = "EXEC <body>" (치환 후). 본문만 떼어낸다.
        let body = text.trim_start();
        let body = body
            .get(4..)
            .map(|b| b.trim_start_matches(|c: char| c.is_ascii_alphabetic()))
            .unwrap_or("")
            .trim();
        let body = body.trim_end_matches(';').trim();
        if let Some((lhs, rhs)) = body.split_once(":=") {
            let lhs = lhs.trim();
            if lhs.starts_with(':')
                && lhs.len() > 1
                && lhs[1..].bytes().all(crate::lexer::is_ident_char)
            {
                if let Ok(v) = parse_literal(rhs) {
                    let name = lhs[1..].to_ascii_uppercase();
                    self.vars.assign(&name, v.clone());
                    let mut acts = vec![Action::LocalAssign {
                        name: name.clone(),
                        value: v.clone(),
                    }];
                    if self.settings.autoprint {
                        acts.push(Action::Print(vec![(name, v)]));
                    }
                    return acts;
                }
            }
        }
        let wrapped = wrap_exec(self.dialect, body);
        let prepared = prepare(self.dialect, &wrapped, &mut self.vars, true);
        self.note_implicit(&prepared);
        vec![Action::Execute {
            prepared,
            expect_out: true,
            kind: SqlKind::Block,
        }]
    }

    fn apply_set(&mut self, opt: &SetOption) {
        let s = &mut self.settings;
        match opt {
            SetOption::ServerOutput { on } => s.serveroutput = *on,
            SetOption::Timing(b) => s.timing = *b,
            SetOption::AutoCommit(b) => s.autocommit = *b,
            SetOption::AutoPrint(b) => s.autoprint = *b,
            SetOption::Feedback(b) => s.feedback = *b,
            SetOption::Define(c) => s.define_char = *c,
            SetOption::Verify(b) => s.verify = *b,
            SetOption::Echo(b) => s.echo = *b,
            SetOption::FetchSize(n) => s.fetch_size = (*n).clamp(1, 100_000),
            SetOption::SqlFormat(f) => s.sqlformat = f.clone(),
            SetOption::Other { name, value } => {
                s.other.insert(name.clone(), value.clone());
            }
        }
    }

    /// 실행 결과의 OUT 값을 저장소로. `autoprint`면 표시할 목록을 돌려준다.
    pub fn absorb(&mut self, result: &ExecResult) -> Option<Vec<(String, Value)>> {
        self.vars.absorb(&result.out_params);
        if self.settings.autoprint && !result.out_params.is_empty() {
            Some(result.out_params.clone())
        } else {
            None
        }
    }

    /// 치환 변수 전처리(docs/04 §8.2). `&&NAME`은 정의를 남기고, `&NAME`은 한 번만.
    /// 미정의면 `Err(NAME)` — 호스트가 프롬프트. `SET DEFINE OFF`면 원문 그대로.
    pub fn substitute(&mut self, text: &str) -> Result<String, String> {
        let Some(ch) = self.settings.define_char else {
            return Ok(text.to_string());
        };
        let classes = crate::lexer::classify(text);
        let b = text.as_bytes();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < b.len() {
            // 주석 안은 치환하지 않는다(SQL*Plus는 하지만 사고를 부른다). 문자열 안은 SQL*Plus처럼 치환.
            if b[i] == ch as u8
                && !matches!(
                    classes[i],
                    crate::lexer::Class::LineComment | crate::lexer::Class::BlockComment
                )
            {
                let double = i + 1 < b.len() && b[i + 1] == ch as u8;
                let start = i + if double { 2 } else { 1 };
                let mut j = start;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if j > start {
                    let name = text[start..j].to_ascii_uppercase();
                    match self.defines.get(&name) {
                        Some(v) => {
                            out.push_str(v);
                            // 뒤따르는 `.`은 이름 종결자(SET CONCAT) — 소비한다.
                            if j < b.len() && b[j] == b'.' {
                                j += 1;
                            }
                            i = j;
                            continue;
                        }
                        None => return Err(name),
                    }
                }
            }
            let c = text[i..].chars().next().unwrap_or('\0');
            out.push(c);
            i += c.len_utf8();
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::split::split_script;
    use nsql_core::{BindParam, Direction, VarType};

    fn plan_all(engine: &mut Engine, src: &str) -> Vec<Action> {
        split_script(src)
            .iter()
            .flat_map(|it| engine.plan(it))
            .collect()
    }

    #[test]
    fn literal_assignment_is_local_and_visible_to_later_statements() {
        let mut e = Engine::new(Dialect::Mssql);
        let acts = plan_all(
            &mut e,
            "EXEC :V_PRG_NM := 'SP_M4P';\nSELECT :V_PRG_NM AS n;\n",
        );
        assert_eq!(
            acts[0],
            Action::LocalAssign {
                name: "V_PRG_NM".into(),
                value: Value::Str("SP_M4P".into())
            }
        );
        match &acts[1] {
            Action::Execute {
                prepared,
                expect_out: false,
                ..
            } => {
                assert_eq!(prepared.sql, "SELECT @V_PRG_NM AS n");
                assert_eq!(
                    prepared.params,
                    vec![BindParam {
                        name: "V_PRG_NM".into(),
                        value: Value::Str("SP_M4P".into()),
                        ty: VarType::Varchar2(6),
                        direction: Direction::In
                    }]
                );
            }
            a => panic!("{a:?}"),
        }
    }

    #[test]
    fn oracle_exec_select_into_round_trip() {
        let mut e = Engine::new(Dialect::Oracle);
        let src = "EXEC\nSELECT A.PROJECT_CD, A.MP_VRSN_SEQ INTO :V_PROJECT_CD, :V_MP_VRSN_SEQ FROM M4S_O301010 A WHERE A.PROJECT_CD = 'SEBANG'\n\nPRINT V_PROJECT_CD\n";
        let items = split_script(src);
        let acts = e.plan(&items[0]);
        let Action::Execute {
            prepared,
            expect_out: true,
            ..
        } = &acts[0]
        else {
            panic!("{acts:?}")
        };
        assert!(
            prepared.sql.starts_with("BEGIN SELECT A.PROJECT_CD"),
            "{}",
            prepared.sql
        );
        assert!(prepared.sql.ends_with("'SEBANG'; END;"));
        assert_eq!(prepared.params.len(), 2);
        assert!(prepared
            .params
            .iter()
            .all(|p| p.direction == Direction::InOut));
        assert_eq!(
            e.diagnostics,
            vec![
                Diagnostic::ImplicitVariable("V_PROJECT_CD".into()),
                Diagnostic::ImplicitVariable("V_MP_VRSN_SEQ".into())
            ]
        );
        // 드라이버가 돌려준 OUT 값 흡수
        let res = ExecResult {
            out_params: vec![
                ("V_PROJECT_CD".into(), Value::Str("SEBANG".into())),
                ("V_MP_VRSN_SEQ".into(), Value::Int(7)),
            ],
            ..Default::default()
        };
        assert!(e.absorb(&res).is_none());
        let acts = e.plan(&items[1]);
        assert_eq!(
            acts,
            vec![Action::Print(vec![(
                "V_PROJECT_CD".into(),
                Value::Str("SEBANG".into())
            )])]
        );
        assert_eq!(e.vars.get("v_mp_vrsn_seq").unwrap().ty, VarType::Number);
    }

    #[test]
    fn mssql_exec_select_into_becomes_assignment_select_with_output() {
        let mut e = Engine::new(Dialect::Mssql);
        let src = "EXEC\nSELECT A.PROJECT_CD, A.SEQ INTO :V_CD, :V_SEQ FROM T A\n";
        let acts = plan_all(&mut e, src);
        let Action::Execute {
            prepared,
            expect_out: true,
            ..
        } = &acts[0]
        else {
            panic!("{acts:?}")
        };
        assert_eq!(
            prepared.sql,
            "SELECT @V_CD = A.PROJECT_CD, @V_SEQ = A.SEQ FROM T A"
        );
        assert!(prepared
            .params
            .iter()
            .all(|p| p.direction == Direction::InOut));
    }

    #[test]
    fn variable_declare_print_and_refcursor_type() {
        let mut e = Engine::new(Dialect::Oracle);
        let acts = plan_all(&mut e, "VARIABLE rc REFCURSOR\nVAR n NUMBER = 5\nEXEC OPEN :rc FOR SELECT * FROM dual\nPRINT rc n\n");
        assert!(matches!(acts[0], Action::Nothing(_)));
        let Action::Execute { prepared, .. } = &acts[2] else {
            panic!()
        };
        assert_eq!(prepared.sql, "BEGIN OPEN :rc FOR SELECT * FROM dual; END;");
        assert_eq!(prepared.params[0].ty, VarType::RefCursor);
        assert_eq!(
            acts[3],
            Action::Print(vec![
                ("RC".into(), Value::Null),
                ("N".into(), Value::Int(5))
            ])
        );
    }

    #[test]
    fn substitution_variables_prompt_then_replace() {
        let mut e = Engine::new(Dialect::Oracle);
        let items = split_script("SELECT * FROM emp WHERE dept = '&dept' AND x = &&lim -- &no\n");
        assert_eq!(
            e.plan(&items[0]),
            vec![Action::NeedInput {
                name: "DEPT".into()
            }]
        );
        e.define("dept", "SALES");
        assert_eq!(
            e.plan(&items[0]),
            vec![Action::NeedInput { name: "LIM".into() }]
        );
        e.define("lim", "10");
        let Action::Execute { prepared, .. } = &e.plan(&items[0])[0] else {
            panic!()
        };
        assert_eq!(
            prepared.sql,
            "SELECT * FROM emp WHERE dept = 'SALES' AND x = 10 -- &no"
        );
        // SET DEFINE OFF → 원문 그대로
        let items = split_script("SET DEFINE OFF\nSELECT '&raw' FROM dual;\n");
        e.plan(&items[0]);
        let Action::Execute { prepared, .. } = &e.plan(&items[1])[0] else {
            panic!()
        };
        assert_eq!(prepared.sql, "SELECT '&raw' FROM dual");
    }

    #[test]
    fn connect_and_settings() {
        let mut e = Engine::new(Dialect::Oracle);
        let acts = plan_all(&mut e, "connect user/pass@192.168.1.1:1521/db;\nSET SERVEROUTPUT ON\nSET AUTOPRINT ON\nEXEC :x := 1\n");
        assert!(
            matches!(&acts[0], Action::Connect(c) if c.host.as_deref() == Some("192.168.1.1") && c.port == Some(1521))
        );
        assert!(e.settings.serveroutput);
        assert_eq!(
            acts[3],
            Action::LocalAssign {
                name: "X".into(),
                value: Value::Int(1)
            }
        );
        assert_eq!(acts[4], Action::Print(vec![("X".into(), Value::Int(1))]));
    }

    #[test]
    fn sqlcmd_setvar_feeds_substitution() {
        let mut e = Engine::new(Dialect::Mssql);
        let acts = plan_all(&mut e, ":setvar Env prod\nSELECT '&Env' AS env\nGO\n");
        let Action::Execute { prepared, .. } = &acts[1] else {
            panic!("{acts:?}")
        };
        assert_eq!(prepared.sql, "SELECT 'prod' AS env");
    }
}
