//! 엔진 — 항목을 **행동**으로 계획한다. DB를 모른다(호스트가 실행하고 결과를 되돌려 준다).
//!
//! 세션 변수는 SQL*Plus처럼 클라이언트에만 산다(docs/04 §8.2). 대입 우변이 리터럴이면
//! DB 왕복 없이 로컬 대입([`Action::LocalAssign`]) — 그래서 `EXEC :V := 'x'`는 **모든 방언에서**
//! 똑같이 동작하고, MSSQL에서도 이후 문장이 같은 변수를 본다.

use crate::command::{parse_literal, Command, SetOption};
use crate::connect::ConnectSpec;
use crate::dialect::{prepare_with, wrap_exec_with, Prepared};
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
    /// 인자 없는 `VARIABLE` — 선언된 변수 목록(이름 · 타입 · SQL*Plus 관용 · 값은 `PRINT`). mac 09-21: 종전에는 `Print`와
    /// 같은 동작이라 로그에 `PRINT`로 찍혔다.
    ListVars(Vec<(String, nsql_core::VarType)>),
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
    /// `ACCEPT`(T-153) — 호스트가 값을 물어 [`Engine::define`]한다(빈 답·답 없음 = `default`). 실행 전 입력 창이 이미 답했으면
    /// 이 행동은 나오지 않는다([`Engine::accepted`]).
    Accept {
        name: String,
        prompt: Option<String>,
        default: Option<String>,
        hide: bool,
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
    /// 돌아온 값이 상한(`vars.max_value_kb`)을 넘어 잘렸다 — (변수 이름, 원래 바이트 수).
    Truncated(String, usize),
    Info(String),
}

/// 엔진 설정 — `SET`이 바꾼다.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Settings {
    /// `SELECT … INTO`가 여러 행이면 첫 행을 쓴다(설정 `vars.into_policy = first` · 기본 = Oracle식 오류 · D-139).
    pub into_first: bool,
    /// `${이름[:형식]}` 치환(설정 `vars.brace_subst` · 기본 켬) — 정의된 이름만 바꾸고 모르는 이름은 글자 그대로 둔다.
    pub brace_subst: bool,
    /// `${env:이름[:형식]}` = **OS 환경 변수**(설정 `vars.env_subst` · 기본 켬 · 사용자 09-21) — 없는 변수는 글자 그대로 둔다.
    /// `brace_subst`가 꺼져 있으면 이것도 돌지 않는다.
    pub env_subst: bool,
    /// ★ 변수 안의 변수 **확장 시점**(설정 `vars.expand_at` · docs/63 §9 · 사용자 09-22): false = **대입 시**(`DEFINE a = &b`의
    /// `&b`를 정의할 때 바꾼다 · SQL*Plus·psql·sqlcmd) · true = **사용 시**(원문을 보관하고 `&a`를 읽을 때 재귀 치환 · 깊이 16 ·
    /// 순환 = 오류 · SQL Workbench/J). `DEFINE` 목록은 사용 시 모드에서 `원문  →  현재 값`으로 보여 준다.
    pub expand_at_use: bool,
    /// 변수 하나가 담는 값의 상한(바이트 · 설정 `vars.max_value_kb` · 0 = 무제한) — 돌아온 CLOB·긴 글이 변수 표·보존 파일·
    /// 변수 창을 부풀리지 않게.
    pub max_value_bytes: usize,
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
            into_first: false,
            brace_subst: true,
            env_subst: true,
            expand_at_use: false,
            max_value_bytes: 1024 * 1024,
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
    /// ★ 드라이버 능력표(T-152) — 엔진의 분기는 방언이 아니라 이것을 묻는다. `new`/`set_dialect`는 내장 방언의 표를,
    /// 러너는 접속한 세션이 말한 표(`Session::caps`)를 넣는다(`set_caps`).
    pub caps: nsql_core::Caps,
    /// **이번 `EXEC 호출`이 돌려줄 값을 받을 바인드들**(OUT/INOUT 인자의 순서 · 바인드가 아닌 자리는 빈 글) — OUT 바인드가 없는
    /// 방언(PostgreSQL)에서 러너가 루틴 서명으로 정해 넣는다(T-151). 다음 `EXEC` 한 번에 쓰이고 비워진다.
    pub call_captures: Option<Vec<String>>,
    /// **이번 `EXEC 호출`에서 `OUTPUT`을 보충할 바인드들**(대문자) — T-SQL은 호출 쪽에 `OUTPUT`이 없으면 값을 돌려주지 않는다.
    /// 러너가 서명(`sys.parameters`)으로 정해 넣는다(T-151). 다음 `EXEC` 한 번에 쓰이고 비워진다.
    pub call_outputs: Vec<String>,
    /// **이번 실행의 입력 창이 이미 답한 `ACCEPT` 이름**(대문자) — 그 `ACCEPT`는 다시 묻지 않고 지나간다(한 번 쓰이고 빠진다).
    pub accepted: std::collections::BTreeSet<String>,
    /// **시스템 변수**(`&_USER` · `&_DATE` … — [`SYSTEM_VARS`]) — 호스트(러너)가 채운다. `DEFINE`으로 같은 이름을 정의하면 그쪽이 이긴다
    /// (SQL*Plus도 `_DATE`를 다시 정의할 수 있다). 사용자 치환 변수(`defines`)와 **섞지 않는다**(목록·보존·`UNDEFINE`에 끼지 않는다).
    pub sysvars: BTreeMap<String, String>,
    /// `COLUMN 열 NEW_VALUE 변수` — 열 이름(대문자) → 치환 변수 이름.
    pub column_new: BTreeMap<String, String>,
    pub vars: VarStore,
    /// 치환 변수(`DEFINE` · `:setvar` · `&1..&n`).
    pub defines: BTreeMap<String, String>,
    pub settings: Settings,
    pub diagnostics: Vec<Diagnostic>,
    /// 사용 시 확장의 재귀 스택(순환·깊이 검출 · 치환 중에만 비어 있지 않다).
    subst_stack: Vec<String>,
}

impl Engine {
    pub fn new(dialect: Dialect) -> Self {
        Engine {
            dialect,
            caps: nsql_core::Caps::of(dialect),
            call_captures: None,
            call_outputs: Vec::new(),
            accepted: std::collections::BTreeSet::new(),
            sysvars: BTreeMap::new(),
            column_new: BTreeMap::new(),
            vars: VarStore::new(),
            defines: BTreeMap::new(),
            settings: Settings::default(),
            diagnostics: Vec::new(),
            subst_stack: Vec::new(),
        }
    }

    /// 방언을 바꾼다 — 능력표도 그 방언의 내장 표로.
    pub fn set_dialect(&mut self, dialect: Dialect) {
        self.dialect = dialect;
        self.caps = nsql_core::Caps::of(dialect);
    }

    /// 접속한 세션의 방언 + **그 세션이 말한 능력표**(확장 드라이버는 내장 표와 다를 수 있다).
    pub fn set_caps(&mut self, dialect: Dialect, caps: nsql_core::Caps) {
        self.dialect = dialect;
        self.caps = caps;
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
        // ★ `DEFINE 이름 = 값`은 치환 **전에** 가로챈다(docs/63 §9): 대입 시 모드 = 값을 지금 치환해 저장(SQL*Plus) ·
        //   사용 시 모드 = 원문 그대로 저장(읽을 때 재귀 확장). 종전엔 원문을 저장하고 쓸 때 한 번만 바꿔 중첩 참조가 남았다.
        if let ItemKind::Command(Command::Define {
            name: Some(n),
            value: Some(v),
        }) = &item.kind
        {
            let stored = if self.settings.expand_at_use {
                v.clone()
            } else {
                match self.substitute(v) {
                    Ok(t) => t,
                    Err(e) => return vec![Self::subst_error(e)],
                }
            };
            self.define(n, &stored);
            return vec![Action::Nothing(format!("define {n}"))];
        }
        let text = match self.substitute(&item.text) {
            Ok(t) => t,
            Err(e) => return vec![Self::subst_error(e)],
        };
        match &item.kind {
            ItemKind::Invalid(msg) => vec![Action::Error(msg.clone())],
            ItemKind::Sql(kind) => {
                // 저장 코드를 만드는 DDL은 바인드 대상이 아니다(본문의 `:NEW`·`:OLD`·`:x`는 서버가 해석한다).
                if matches!(kind, SqlKind::Block) && is_stored_code_ddl(&text) {
                    return vec![Action::Execute {
                        prepared: Prepared::verbatim(&text),
                        expect_out: false,
                        kind: *kind,
                    }];
                }
                let inout = matches!(kind, SqlKind::Block);
                let prepared = prepare_with(&self.caps, &text, &mut self.vars, inout);
                // 경고는 **읽기만 하는 보통 SQL**에서만 — 블록의 바인드는 받는 쪽(OUT)일 수 있다(09-21 실서버: 대입 대상에 오경고).
                if !inout {
                    self.note_implicit(&prepared);
                }
                vec![Action::Execute {
                    prepared,
                    expect_out: inout,
                    kind: *kind,
                }]
            }
            ItemKind::Command(cmd) => self.plan_command(cmd, &text),
        }
    }

    /// [`Engine::substitute`]의 `Err` → 행동: 미정의 이름 = 입력 요청 · `\0`으로 시작 = 순환/깊이 오류.
    fn subst_error(e: String) -> Action {
        match e.strip_prefix('\0') {
            Some(msg) => Action::Error(msg.to_string()),
            None => Action::NeedInput { name: e },
        }
    }

    /// 사용 시 확장: 저장된 원문에 치환 글자가 남아 있으면 재귀로 편다(깊이 16 · 순환 = `\0` 오류).
    fn expand_nested(&mut self, name: &str, v: String) -> Result<String, String> {
        let ch = self.settings.define_char;
        let nested =
            ch.is_some_and(|c| v.contains(c)) || (self.settings.brace_subst && v.contains("${"));
        if !self.settings.expand_at_use || !nested {
            return Ok(v);
        }
        if self.subst_stack.iter().any(|n| n == name) {
            let chain: Vec<&str> = self
                .subst_stack
                .iter()
                .map(String::as_str)
                .chain([name])
                .collect();
            return Err(format!("\0circular substitution: {}", chain.join(" -> ")));
        }
        if self.subst_stack.len() >= 16 {
            return Err(format!("\0substitution too deep (16): {name}"));
        }
        self.subst_stack.push(name.to_string());
        let r = self.substitute(&v);
        self.subst_stack.pop();
        r
    }

    /// `DEFINE` 표시 글 — 사용 시 모드에서 원문에 참조가 남아 있으면 `원문  →  현재 값`(순환이면 원문만).
    fn define_display(&mut self, name: &str, raw: &str) -> String {
        if !self.settings.expand_at_use {
            return raw.to_string();
        }
        match self.expand_nested(name, raw.to_string()) {
            Ok(v) if v != raw => format!("{raw}  \u{2192}  {v}"),
            _ => raw.to_string(),
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
                let list: Vec<(String, nsql_core::VarType)> = self
                    .vars
                    .iter()
                    .map(|(n, v)| (n.clone(), v.ty.clone()))
                    .collect();
                vec![Action::ListVars(list)]
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
            Command::VarScope { name, shared } => {
                let ok = if *shared {
                    self.vars.share(name)
                } else {
                    self.vars.unshare(name)
                };
                if ok {
                    let layer = if *shared { "shared" } else { "local" };
                    vec![Action::Nothing(format!("variable {name} {layer}"))]
                } else {
                    vec![Action::Error(format!("변수 {name}가 없습니다"))]
                }
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
                let pairs: Vec<(String, String)> = self
                    .defines
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                let list = pairs
                    .into_iter()
                    .map(|(k, v)| {
                        let shown = self.define_display(&k, &v);
                        (k, Value::Str(shown))
                    })
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
            } => match self.defines.get(n).cloned() {
                Some(v) => {
                    let shown = self.define_display(n, &v);
                    vec![Action::Print(vec![(n.clone(), Value::Str(shown))])]
                }
                None => vec![Action::Error(format!("치환 변수 {n}가 없습니다"))],
            },
            Command::Column {
                name,
                new_value,
                clear,
            } => {
                if *clear {
                    self.column_new.remove(name);
                }
                if let Some(v) = new_value {
                    self.column_new.insert(name.clone(), v.clone());
                }
                vec![Action::Nothing(format!("column {name}"))]
            }
            Command::Accept {
                name,
                default,
                prompt,
                hide,
            } => {
                if self.accepted.remove(name) {
                    return vec![Action::Nothing(format!("accept {name}"))];
                }
                vec![Action::Accept {
                    name: name.clone(),
                    prompt: prompt.clone(),
                    default: default.clone(),
                    hide: *hide,
                }]
            }
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
            // 치환 뒤 본문(`PROMPT hello &1` — SQL*Plus처럼 &var가 풀린다 · T-9 09-16). 명령어 뒤 첫 공백 하나만 뗀다.
            Command::Prompt { .. } => vec![Action::Prompt(after_command_word(text))],
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
                    // 표에는 처음 쓴 표기로(D-142) — 찾기는 대문자 키라 같다.
                    self.vars.assign(&lhs[1..], v.clone());
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
        let mut wrapped = wrap_exec_with(&self.caps, body);
        let outputs = std::mem::take(&mut self.call_outputs);
        if !outputs.is_empty() {
            wrapped = crate::call::mark_output(&wrapped, &outputs);
        }
        // SQL Server의 `SELECT @A = col …`은 **여러 행이면 마지막 행을 말없이** 쓰고 0행이면 옛 값을 남긴다 → 서버에서 바로
        // 행 수를 검사해 Oracle과 같은 오류로(D-139 · `@@ROWCOUNT`는 바로 다음 문장에서만 유효 · first 정책이면 검사하지 않는다).
        if self.caps.into_rowcount_guard
            && !self.settings.into_first
            && wrapped != body
            && body
                .trim_start()
                .get(..6)
                .is_some_and(|w| w.eq_ignore_ascii_case("SELECT"))
        {
            wrapped.push_str(TSQL_INTO_GUARD);
        }
        let mut prepared = prepare_with(&self.caps, &wrapped, &mut self.vars, true);
        // OUT 바인드가 없는 DBMS: 받는 쪽을 **요청에 명시**한다(추측해서 1행 결과를 빨아들이지 않는다).
        if self.caps.captures_rows() {
            prepared.captures = exec_targets(body);
            // 호출(`EXEC proc(3, :X, :Y)`): 받는 쪽은 서명이 정한다 — 결과 열 이름(= 형식 인자 이름)이 바인드 이름과 다르면
            // 종전에는 값이 조용히 버려졌다(T-151).
            if let Some(c) = self
                .call_captures
                .take()
                .filter(|_| prepared.captures.is_empty())
            {
                prepared.captures = c;
            }
        }
        // `EXEC`의 바인드는 받는 쪽(OUT 인자 · 대입 대상)일 수 있다 → 미정의 경고는 내지 않는다(보통 SQL에서만).
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
        let limit = self.settings.max_value_bytes;
        let too_big = |v: &Value| matches!(v, Value::Str(s) if limit > 0 && s.len() > limit);
        if result.out_params.iter().any(|(_, v)| too_big(v)) {
            // 드문 길: 상한을 넘는 값만 잘라 사본을 만든다(보통 길은 복사 0).
            let clipped: Vec<(String, Value)> = result
                .out_params
                .iter()
                .map(|(n, v)| match v {
                    Value::Str(s) if too_big(v) => {
                        self.diagnostics
                            .push(Diagnostic::Truncated(n.clone(), s.len()));
                        (n.clone(), Value::Str(clip_utf8(s, limit).to_string()))
                    }
                    _ => (n.clone(), v.clone()),
                })
                .collect();
            self.vars.absorb(&clipped);
        } else {
            self.vars.absorb(&result.out_params);
        }
        if self.settings.autoprint && !result.out_params.is_empty() {
            Some(result.out_params.clone())
        } else {
            None
        }
    }

    /// `COLUMN … NEW_VALUE`(T-153): 방금 나온 결과의 **마지막 행**에서 등록된 열의 값을 치환 변수에 넣는다(NULL = 빈 글).
    /// 등록이 없으면 아무것도 하지 않는다(보통 길 = 맵이 비었는지 한 번 본다).
    pub fn note_result(&mut self, columns: &[nsql_core::Column], last_row: Option<&Vec<Value>>) {
        if self.column_new.is_empty() {
            return;
        }
        let Some(row) = last_row else {
            return;
        };
        for (c, v) in columns.iter().zip(row.iter()) {
            if let Some(var) = self.column_new.get(&c.name.to_ascii_uppercase()) {
                let text = match v {
                    Value::Null => String::new(),
                    other => other.display(),
                };
                self.defines.insert(var.clone(), text);
            }
        }
    }

    /// 치환 값 찾기: 사용자 정의 → 시스템 변수(이름이 [`SYSTEM_VARS`]에 있으면 값이 아직 없어도 빈 글 — 묻지 않는다).
    fn macro_value(&self, name: &str) -> Option<&str> {
        if let Some(v) = self.defines.get(name) {
            return Some(v.as_str());
        }
        if let Some(v) = self.sysvars.get(name) {
            return Some(v.as_str());
        }
        SYSTEM_VARS.contains(&name).then_some("")
    }

    /// 치환 변수 전처리(docs/04 §8.2). `&&NAME`은 정의를 남기고, `&NAME`은 한 번만.
    /// 미정의면 `Err(NAME)` — 호스트가 프롬프트. `SET DEFINE OFF`면 원문 그대로.
    pub fn substitute(&mut self, text: &str) -> Result<String, String> {
        let Some(ch) = self.settings.define_char else {
            return Ok(text.to_string());
        };
        // 빠른 길: 치환 글자도 `${`도 없으면 토큰화하지 않는다.
        let brace = self.settings.brace_subst && text.contains("${");
        if !brace && !text.contains(ch) {
            return Ok(text.to_string());
        }
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
                    match self.macro_value(&name).map(str::to_string) {
                        Some(v) => {
                            let v = self.expand_nested(&name, v)?;
                            out.push_str(&v);
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
            // `${이름[:형식]}` — 정의된 이름 + 아는 형식일 때만 바꾼다(모르면 글자 그대로 · 묻지 않는다 · 주석 안 제외).
            if brace
                && b[i] == b'$'
                && b.get(i + 1) == Some(&b'{')
                && !matches!(
                    classes[i],
                    crate::lexer::Class::LineComment | crate::lexer::Class::BlockComment
                )
            {
                if let Some(close) = text[i + 2..].find('}') {
                    let inner = &text[i + 2..i + 2 + close];
                    // `${env:이름[:형식]}` — OS 환경 변수(이름은 OS 규칙대로: Windows = 대소문자 무관 · 그 밖 = 구분).
                    if let Some(rest) = inner
                        .get(..4)
                        .filter(|p| p.eq_ignore_ascii_case("env:"))
                        .map(|_| &inner[4..])
                    {
                        let (var, fmt) = rest.split_once(':').unwrap_or((rest, ""));
                        let value = (self.settings.env_subst && !var.trim().is_empty())
                            .then(|| std::env::var(var.trim()).ok())
                            .flatten();
                        if let Some(s) =
                            value.and_then(|v| format_macro(&v, fmt.trim(), self.dialect))
                        {
                            out.push_str(&s);
                            i += 2 + close + 1;
                            continue;
                        }
                        // 없는 변수 · 꺼진 설정 · 모르는 형식 = 글자 그대로.
                        out.push_str(&text[i..i + 2 + close + 1]);
                        i += 2 + close + 1;
                        continue;
                    }
                    let (name, fmt) = inner.split_once(':').unwrap_or((inner, ""));
                    let key = name.trim().to_ascii_uppercase();
                    let known = !key.is_empty()
                        && key.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_');
                    if known {
                        if let Some(v) = self.macro_value(&key).map(str::to_string) {
                            let v = self.expand_nested(&key, v)?;
                            if let Some(s) = format_macro(&v, fmt.trim(), self.dialect) {
                                out.push_str(&s);
                                i += 2 + close + 1;
                                continue;
                            }
                        }
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

/// 시스템 변수 이름(대문자) — 값은 호스트가 [`Engine::sysvars`]에 넣는다. 실행 전 훑기는 이 이름들을 묻지 않는다.
/// `_USER` · `_CONNECT_IDENTIFIER` · `_DATE` = SQL*Plus의 미리 정의된 변수 · 나머지는 psql(`ROW_COUNT` · `SQLSTATE`)과 같은 쓰임.
pub const SYSTEM_VARS: &[&str] = &[
    "_USER",
    "_CONNECT_IDENTIFIER",
    "_DIALECT",
    "_DATE",
    "_TIMESTAMP",
    "_ROW_COUNT",
    "_SQLCODE",
    "_ELAPSED_MS",
    "_FILE",
];

/// 글을 `limit` 바이트 이하의 글자 경계에서 자른다.
fn clip_utf8(s: &str, limit: usize) -> &str {
    if s.len() <= limit {
        return s;
    }
    let mut end = limit;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// 값 → **SQL 글자 상수**(`:q` 형식 · 순수). 규칙: ① 값 전체를 작은따옴표로 감싼다 ② 값 안의 작은따옴표는 **두 번** 쓴다
/// (`O'Neil` → `'O''Neil'` — SQL 표준 · 전 DBMS 공통 · 값이 글자 상수 밖으로 빠져나가지 못한다 = 주입 방지) ③ SQL Server는
/// **`N'…'`**(유니코드 상수 — `N`이 없으면 한글 등이 DB 코드페이지로 바뀌며 깨질 수 있다) ④ MySQL은 역슬래시가 이스케이프 문자라
/// **역슬래시도 두 번 쓴다**(Windows 경로 `C:\Users` 같은 값이 망가지지 않게 · `NO_BACKSLASH_ESCAPES` 모드가 아닌 기본 동작 기준)
/// ⑤ 그 밖(Oracle · PostgreSQL `standard_conforming_strings = on` · SQLite · ODBC)은 역슬래시가 보통 글자다. 줄바꿈 등 다른 글자는
/// 그대로 둔다(글자 상수 안에서 유효하다). OS와 무관한 순수 글자 처리 — Windows · macOS · Linux에서 같은 결과.
fn sql_text_literal(v: &str, dialect: Dialect) -> String {
    let body = v.replace('\'', "''");
    match dialect {
        Dialect::Mssql => format!("N'{body}'"),
        Dialect::Mysql => format!("'{}'", body.replace('\\', "\\\\")),
        _ => format!("'{body}'"),
    }
}

/// `${이름:형식}`의 형식(순수) — 없음·`raw` = 그대로 · `q` = SQL 글자 상수(psql `:'v'`) · `id` = 인용한 이름(psql `:"v"` — 방언의
/// 인용 부호) · `upper`/`lower` · `n` = 수일 때만(아니면 바꾸지 않는다 — 주입 방지용). 모르는 형식 = `None`(글자 그대로 둔다).
fn format_macro(v: &str, fmt: &str, dialect: Dialect) -> Option<String> {
    Some(match fmt.to_ascii_lowercase().as_str() {
        "" | "raw" => v.to_string(),
        "q" | "quote" | "sql" => sql_text_literal(v, dialect),
        "id" | "ident" => match dialect {
            Dialect::Mssql => format!("[{}]", v.replace(']', "]]")),
            Dialect::Mysql => format!("`{}`", v.replace('`', "``")),
            _ => format!("\"{}\"", v.replace('"', "\"\"")),
        },
        "upper" => v.to_uppercase(),
        "lower" => v.to_lowercase(),
        "n" | "num" => {
            let t = v.trim();
            t.parse::<f64>().ok().filter(|f| f.is_finite())?;
            t.to_string()
        }
        _ => return None,
    })
}

/// 명령어(첫 단어) 뒤의 본문 — `PROMPT  a b` → `a b`(앞 공백은 하나만 뗀다 · SQL*Plus).
/// SQL Server `SELECT @v = …` 바로 뒤에 붙는 행 수 검사(0행 = 51403 · 여러 행 = 51422 — ORA-01403/01422에 맞춘 번호).
const TSQL_INTO_GUARD: &str = ";\nDECLARE @nsql_into_rc INT = @@ROWCOUNT;\n\
IF @nsql_into_rc = 0 THROW 51403, N'INTO: no data found (the query returned no rows)', 1;\n\
IF @nsql_into_rc > 1 THROW 51422, N'INTO: exact fetch returns more than one row', 1";

/// `EXEC` 본문이 값을 받는 변수(자리 순서) — `:V := 식` = [V] · `SELECT … INTO :A, :B …` = [A, B] · 호출 = 없음.
fn exec_targets(body: &str) -> Vec<String> {
    if let Some((lhs, _)) = body.split_once(":=") {
        let l = lhs.trim();
        if l.starts_with(':') && l.len() > 1 && l[1..].bytes().all(crate::lexer::is_ident_char) {
            return vec![l[1..].to_ascii_uppercase()];
        }
    }
    if body
        .trim_start()
        .get(..6)
        .is_some_and(|w| w.eq_ignore_ascii_case("SELECT"))
    {
        return crate::inputs::into_targets(body);
    }
    Vec::new()
}

/// `CREATE [OR REPLACE|OR ALTER] [[NON]EDITIONABLE] PROCEDURE|FUNCTION|PACKAGE|TRIGGER|TYPE|LIBRARY …`인가 —
/// 분할기가 `Block`으로 본 문장 중 **저장 코드 DDL**(익명 블록 `BEGIN`/`DECLARE`가 아닌 것).
fn is_stored_code_ddl(text: &str) -> bool {
    let first = text
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    first == "CREATE" || first == "ALTER"
}

fn after_command_word(text: &str) -> String {
    let t = text.trim_start();
    let n = t
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .map(char::len_utf8)
        .sum::<usize>();
    let rest = &t[n..];
    rest.strip_prefix(' ')
        .unwrap_or(rest)
        .trim_end_matches(';')
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    /// `${env:이름}` = OS 환경 변수(사용자 09-21): 있는 변수 = 값 · 형식(`q`) · 없는 변수·모르는 형식 = 글자 그대로 · 주석 안 제외 ·
    /// `vars.env_subst`/`vars.brace_subst` 끔 = 그대로 · 사용자 정의 `env`라는 치환 변수와 섞이지 않는다.
    #[test]
    fn environment_variables_substitute() {
        use nsql_core::Dialect;
        // 모든 OS에 있는 변수(Windows는 이름의 대소문자를 가리지 않는다).
        let path = std::env::var("PATH").expect("PATH");
        let mut e = super::Engine::new(Dialect::Postgres);
        assert_eq!(
            e.substitute("x ${env:PATH} y").ok(),
            Some(format!("x {path} y"))
        );
        assert_eq!(
            e.substitute("${ENV:PATH:q}").ok(),
            Some(format!("'{}'", path.replace('\'', "''")))
        );
        assert_eq!(
            e.substitute("${env:NSQL_NO_SUCH_VARIABLE_42} ${env:} ${env:PATH:zzz} -- ${env:PATH}")
                .as_deref(),
            Ok("${env:NSQL_NO_SUCH_VARIABLE_42} ${env:} ${env:PATH:zzz} -- ${env:PATH}")
        );
        e.define("env", "mine");
        assert_eq!(e.substitute("${env}").as_deref(), Ok("mine"));
        e.settings.env_subst = false;
        assert_eq!(e.substitute("${env:PATH}").as_deref(), Ok("${env:PATH}"));
        e.settings.env_subst = true;
        e.settings.brace_subst = false;
        assert_eq!(e.substitute("${env:PATH}").as_deref(), Ok("${env:PATH}"));
    }

    /// T-153 — 시스템 변수(정의가 이긴다 · 값이 없어도 묻지 않는다) · `${이름:형식}`(정의된 이름 + 아는 형식만 · 주석 제외 ·
    /// 방언별 이름 인용 · `n`은 수일 때만) · `COLUMN … NEW_VALUE`(마지막 행 · NULL = 빈 글 · CLEAR) · 값 상한.
    #[test]
    fn system_vars_brace_formats_column_new_value_and_value_cap() {
        use nsql_core::{Column, Dialect, ExecResult, Value};
        let mut e = super::Engine::new(Dialect::Oracle);
        e.sysvars.insert("_USER".into(), "SCOTT".into());
        assert_eq!(
            e.substitute("select '&_USER' u, '&_ROW_COUNT' r from dual")
                .as_deref(),
            Ok("select 'SCOTT' u, '' r from dual")
        );
        e.define("_USER", "ME");
        assert_eq!(e.substitute("&_USER").as_deref(), Ok("ME"));
        assert_eq!(e.substitute("&nope"), Err("NOPE".into()));

        e.define("who", "O'Neil");
        e.define("n", "42");
        assert_eq!(
            e.substitute("a=${who:q} b=${who:id} c=${WHO:upper} d=${n:n} e=${who:n} f=${who:zzz} g=${undefined} -- ${who}").as_deref(),
            Ok("a='O''Neil' b=\"O'Neil\" c=O'NEIL d=42 e=${who:n} f=${who:zzz} g=${undefined} -- ${who}")
        );
        // `:q` = SQL 글자 상수: 작은따옴표 두 번 · SQL Server = N'…' · MySQL = 역슬래시도 두 번 · 그 밖 = 역슬래시는 보통 글자.
        let win_path = "C:\\Users\\O'Neil";
        assert_eq!(
            super::sql_text_literal(win_path, Dialect::Oracle),
            "'C:\\Users\\O''Neil'"
        );
        assert_eq!(
            super::sql_text_literal(win_path, Dialect::Postgres),
            "'C:\\Users\\O''Neil'"
        );
        assert_eq!(
            super::sql_text_literal(win_path, Dialect::Mssql),
            "N'C:\\Users\\O''Neil'"
        );
        assert_eq!(
            super::sql_text_literal(win_path, Dialect::Mysql),
            "'C:\\\\Users\\\\O''Neil'"
        );
        assert_eq!(super::sql_text_literal("", Dialect::Sqlite), "''");
        assert_eq!(
            super::sql_text_literal("한글 ' 값", Dialect::Mssql),
            "N'한글 '' 값'"
        );
        let mut m = super::Engine::new(Dialect::Mssql);
        m.define("t", "my]tab");
        assert_eq!(
            m.substitute("select * from ${t:id}").as_deref(),
            Ok("select * from [my]]tab]")
        );
        m.settings.brace_subst = false;
        assert_eq!(m.substitute("${t}").as_deref(), Ok("${t}"));

        let cols = vec![
            Column {
                name: "max_id".into(),
                type_name: String::new(),
            },
            Column {
                name: "other".into(),
                type_name: String::new(),
            },
        ];
        let item = |s: &str| crate::split::split_script(s).remove(0);
        e.plan(&item("COLUMN max_id NEW_VALUE v_max"));
        e.note_result(&cols, Some(&vec![Value::Int(7), Value::Null]));
        assert_eq!(e.substitute("&v_max").as_deref(), Ok("7"));
        e.plan(&item("COLUMN other NEW_VALUE v_o"));
        e.note_result(&cols, Some(&vec![Value::Int(8), Value::Null]));
        assert_eq!(e.substitute("[&v_max][&v_o]").as_deref(), Ok("[8][]"));
        e.plan(&item("COLUMN max_id CLEAR"));
        e.note_result(&cols, Some(&vec![Value::Int(9), Value::Null]));
        assert_eq!(e.substitute("&v_max").as_deref(), Ok("8"));
        e.note_result(&cols, None);

        e.settings.max_value_bytes = 5;
        e.diagnostics.clear();
        let r = ExecResult {
            out_params: vec![
                ("BIG".into(), Value::Str("가나다라".into())),
                ("OK".into(), Value::Str("abc".into())),
            ],
            ..Default::default()
        };
        e.absorb(&r);
        assert_eq!(
            e.vars.get("BIG").map(|v| v.value.display()).as_deref(),
            Some("가"),
            "글자 경계에서 자른다"
        );
        assert_eq!(
            e.vars.get("OK").map(|v| v.value.display()).as_deref(),
            Some("abc")
        );
        assert_eq!(
            e.diagnostics,
            vec![super::Diagnostic::Truncated("BIG".into(), 12)]
        );
    }

    /// 트리거 DDL의 `:NEW`/`:OLD`는 클라이언트 바인드가 아니다 — 변수 표를 더럽히지 않고 원문 그대로 간다(09-21).
    #[test]
    fn stored_code_ddl_is_not_bound() {
        use crate::split::split_script;
        let src = "CREATE OR REPLACE TRIGGER trg BEFORE INSERT ON t FOR EACH ROW\nBEGIN\n  :NEW.a := :OLD.a + 1;\nEND;\n/\n";
        let items = split_script(src);
        let mut e = super::Engine::new(nsql_core::Dialect::Oracle);
        let acts = e.plan(&items[0]);
        match &acts[0] {
            super::Action::Execute {
                prepared,
                expect_out,
                ..
            } => {
                assert!(prepared.params.is_empty(), "{:?}", prepared.params);
                assert!(prepared.sql.contains(":NEW.a"));
                assert!(!expect_out);
            }
            other => panic!("{other:?}"),
        }
        assert!(e.vars.is_empty(), "변수 표가 더러워졌다");
        // 익명 블록은 종전대로 바인드한다.
        let items = split_script("BEGIN :V := 1; END;\n/\n");
        let acts = e.plan(&items[0]);
        assert!(
            matches!(&acts[0], super::Action::Execute { prepared, .. } if prepared.params.len() == 1)
        );
    }

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
        // `EXEC … INTO :A, :B`의 바인드는 **받는 쪽**이다 — 미정의 경고를 내지 않는다(09-21 실서버에서 본 오경고).
        assert!(e.diagnostics.is_empty(), "{:?}", e.diagnostics);
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
        // 대입 SELECT + 바로 뒤의 행 수 검사(D-139 — T-SQL은 여러 행이면 마지막 행을 말없이 쓴다).
        assert!(
            prepared
                .sql
                .starts_with("SELECT @V_CD = A.PROJECT_CD, @V_SEQ = A.SEQ FROM T A;\nDECLARE @nsql_into_rc INT = @@ROWCOUNT;"),
            "{}",
            prepared.sql
        );
        assert!(prepared.sql.contains("THROW 51403") && prepared.sql.contains("THROW 51422"));
        assert!(prepared
            .params
            .iter()
            .all(|p| p.direction == Direction::InOut));
        // first 정책이면 검사를 붙이지 않는다(종전 SQL 그대로).
        let mut e = Engine::new(Dialect::Mssql);
        e.settings.into_first = true;
        let acts = plan_all(&mut e, src);
        let Action::Execute { prepared, .. } = &acts[0] else {
            panic!("{acts:?}")
        };
        assert_eq!(
            prepared.sql,
            "SELECT @V_CD = A.PROJECT_CD, @V_SEQ = A.SEQ FROM T A"
        );
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

    /// docs/63 §9 — 변수 안의 변수: 대입 시(기본) vs 사용 시(설정 `vars.expand_at = use`) · 순환 · 표시.
    #[test]
    fn expand_at_assign_vs_use() {
        let item = |s: &str| crate::split::split_script(s).remove(0);
        let run = |e: &mut Engine| {
            for s in ["DEFINE v1 = 2", "DEFINE v2 = &v1 + 5", "DEFINE v1 = 5"] {
                assert!(matches!(e.plan(&item(s))[0], Action::Nothing(_)));
            }
            e.substitute("SELECT &v2 FROM dual")
        };
        let mut e = Engine::new(Dialect::Oracle);
        assert_eq!(
            run(&mut e).as_deref(),
            Ok("SELECT 2 + 5 FROM dual"),
            "대입 시"
        );
        let mut e = Engine::new(Dialect::Oracle);
        e.settings.expand_at_use = true;
        assert_eq!(
            run(&mut e).as_deref(),
            Ok("SELECT 5 + 5 FROM dual"),
            "사용 시"
        );
        // `${v}` 형식도 재귀.
        assert_eq!(e.substitute("${v2}").as_deref(), Ok("5 + 5"));
        // 표시 = 원문 → 현재 값.
        let Action::Print(p) = &e.plan(&item("DEFINE v2"))[0] else {
            panic!()
        };
        assert_eq!(p[0].1, Value::Str("&v1 + 5  \u{2192}  5 + 5".into()));
        // 순환 = 오류 행동(입력 요청이 아니다).
        e.plan(&item("DEFINE a = &b"));
        e.plan(&item("DEFINE b = &a"));
        assert!(
            matches!(&e.plan(&item("SELECT &a FROM dual"))[0], Action::Error(m) if m.contains("circular"))
        );
        // 미정의 참조는 사용 시 모드에서도 입력 요청.
        e.plan(&item("DEFINE c = &nope"));
        assert_eq!(
            e.plan(&item("SELECT &c FROM dual"))[0],
            Action::NeedInput {
                name: "NOPE".into()
            }
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

    /// `PROMPT`도 치환 변수를 푼다(SQL*Plus) · 미정의면 프롬프트 요청.
    #[test]
    fn prompt_substitutes_variables() {
        let mut e = Engine::new(Dialect::Oracle);
        e.set_args(&["2026".to_string()]);
        let acts = plan_all(&mut e, "PROMPT Year &1 · &&x\n");
        assert_eq!(acts, vec![Action::NeedInput { name: "X".into() }]);
        e.define("x", "Q1");
        let acts = plan_all(&mut e, "PROMPT Year &1 · &x\nPROMPT\n");
        assert_eq!(
            acts,
            vec![
                Action::Prompt("Year 2026 · Q1".into()),
                Action::Prompt(String::new())
            ]
        );
        assert_eq!(after_command_word("prompt  two spaces"), " two spaces");
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
