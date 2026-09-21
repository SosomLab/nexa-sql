//! 스크립트 명령(SQL이 아닌 줄) 해석 — SQL*Plus 부분집합 + sqlcmd 호환(docs/08 §2).
//!
//! 지원: `VAR[IABLE]` · `PRINT` · `EXEC[UTE]` · `CONN[ECT]` · `DISC[ONNECT]` · `SET` ·
//! `DEF[INE]` · `UNDEF[INE]` · `DESC[RIBE]` · `SHO[W]` · `SPO[OL]` · `PROMPT` · `@` `@@` ·
//! `REM[ARK]` · `GO [n]` · `:setvar` `:connect` `:r`(sqlcmd) · `WHENEVER SQLERROR`.

use crate::connect::ConnectSpec;
use nsql_core::{Dialect, Value, VarType};

#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    /// `VARIABLE` — 인자 없음 = 목록 · 이름만 = 조회 · 이름+타입[=값] = 선언.
    Variable {
        name: Option<String>,
        ty: Option<VarType>,
        init: Option<Value>,
    },
    /// `VAR[IABLE] name SHARE|LOCAL` — 변수를 연결 공유 층으로 올리거나 탭 층으로 내린다(D-135 · docs/63).
    VarScope {
        name: String,
        shared: bool,
    },
    Print {
        names: Vec<String>,
    },
    /// `EXEC 본문` — 본문은 PL/SQL 한 문장(끝 `;` 제거됨).
    Exec {
        body: String,
    },
    Connect(ConnectSpec),
    Disconnect,
    Set(SetOption),
    Define {
        name: Option<String>,
        value: Option<String>,
    },
    Undefine {
        names: Vec<String>,
    },
    /// `COL[UMN] 열 NEW_V[ALUE] 변수`(SQL*Plus · T-153) — 그 열의 **마지막 행 값**을 치환 변수에 넣는다(`&변수`로 다음 문장에서 쓴다).
    /// `COLUMN 열 CLEAR` = 해제. 그 밖의 옵션(`FORMAT` · `HEADING` …)은 표시용이라 받아 주고 무시한다(서버로 보내지 않는다).
    Column {
        name: String,
        new_value: Option<String>,
        clear: bool,
    },
    /// `ACC[EPT] name [NUM[BER]|CHAR|DATE] [FOR[MAT] f] [DEF[AULT] v] [PROMPT 글|NOPR[OMPT]] [HIDE]`(SQL*Plus · T-153) —
    /// 값을 **물어서** 치환 변수에 넣는다. 타입·형식 낱말은 받아 주되 검사하지 않는다(값은 글자 그대로 끼워진다).
    Accept {
        name: String,
        default: Option<String>,
        prompt: Option<String>,
        hide: bool,
    },
    Describe {
        object: String,
    },
    Show {
        what: String,
    },
    Spool {
        target: String,
    },
    Prompt {
        text: String,
    },
    /// `@path args` / `@@path args`(호출 스크립트 기준 상대 경로).
    Run {
        path: String,
        args: Vec<String>,
        relative_to_caller: bool,
    },
    Remark,
    /// T-SQL 배치 구분 — `GO [n]`.
    Go {
        repeat: u32,
    },
    /// sqlcmd `:setvar NAME value` — 치환 변수와 같은 저장소(`DEFINE`)로 간다.
    SetVar {
        name: String,
        value: Option<String>,
    },
    /// sqlcmd `:r file`.
    Include {
        path: String,
    },
    /// `WHENEVER SQLERROR EXIT|CONTINUE`.
    Whenever {
        on_error_exit: bool,
    },
}

/// `SET` 옵션 — 엔진이 이해하는 것만 열거하고 나머지는 [`SetOption::Other`]로 보존.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SetOption {
    ServerOutput {
        on: bool,
    },
    Timing(bool),
    AutoCommit(bool),
    AutoPrint(bool),
    Feedback(bool),
    /// `SET DEFINE ON|OFF|<char>`.
    Define(Option<char>),
    Verify(bool),
    Echo(bool),
    /// `SET ARRAYSIZE n` / `SET FETCHROWS n`.
    FetchSize(u32),
    /// `SET SQLFORMAT csv|json|…`(SQLcl) — CLI 출력 형식.
    SqlFormat(String),
    Other {
        name: String,
        value: String,
    },
}

fn on_off(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_uppercase().as_str(),
        "ON" | "TRUE" | "1" | "YES"
    )
}

/// 줄의 첫 단어가 스크립트 명령인가 — 문장 분리기가 SQL과 구분할 때 쓴다.
/// `EXEC`·`SET`·`SHOW`·`DESC` 같은 T-SQL 겹침 단어는 **줄 첫머리에서만** 명령으로 본다.
pub fn is_command_start(line: &str) -> bool {
    let l = line.trim_start();
    if l.starts_with('@')
        || l.starts_with(":setvar")
        || l.starts_with(":connect")
        || l.starts_with(":disconnect")
        || l.starts_with(":r ")
    {
        return true;
    }
    if is_connect_by(l) {
        return false;
    }
    let word: String = l
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase();
    matches!(
        word.as_str(),
        "VAR"
            | "VARIABLE"
            | "PRINT"
            | "EXEC"
            | "EXECUTE"
            | "CONN"
            | "CONNECT"
            | "DISC"
            | "DISCONNECT"
            | "SET"
            | "DEF"
            | "DEFINE"
            | "ACC"
            | "ACCEPT"
            | "COL"
            | "COLUMN"
            | "UNDEF"
            | "UNDEFINE"
            | "DESC"
            | "DESCRIBE"
            | "SHO"
            | "SHOW"
            | "SPO"
            | "SPOOL"
            | "PROMPT"
            | "REM"
            | "REMARK"
            | "GO"
            | "WHENEVER"
    ) && (l.len() == word.len() || !l.as_bytes()[word.len()].is_ascii_alphanumeric())
}

/// Oracle 계층 질의의 `CONNECT BY …` 조각인가(docs/52 §5) — 문장 첫머리에 오면(선택 실행으로 잘린 조각) 접속 명령으로
/// 오인하지 않고 SQL로 서버에 보낸다(서버가 구문 오류를 낸다 · 무해). `CONNECT BY`뿐이면(뒤에 아무것도 없음) 프로필 이름 `BY`.
fn is_connect_by(l: &str) -> bool {
    let mut it = l.split_whitespace();
    let (Some(a), Some(b), Some(_)) = (it.next(), it.next(), it.next()) else {
        return false;
    };
    (a.eq_ignore_ascii_case("CONNECT") || a.eq_ignore_ascii_case("CONN"))
        && b.eq_ignore_ascii_case("BY")
}

/// 한 줄(또는 EXEC 블록 텍스트)을 명령으로 해석한다. SQL이면 `Ok(None)`.
pub fn parse_command(text: &str) -> Result<Option<Command>, String> {
    let t = text.trim().trim_end_matches(';').trim();
    if t.is_empty() {
        return Ok(None);
    }
    if is_connect_by(t) {
        return Ok(None);
    }
    if t == ":disconnect" {
        return Ok(Some(Command::Disconnect));
    }
    if let Some(rest) = t.strip_prefix("@@") {
        return Ok(Some(run_cmd(rest, true)));
    }
    if let Some(rest) = t.strip_prefix('@') {
        return Ok(Some(run_cmd(rest, false)));
    }
    if let Some(rest) = t.strip_prefix(":setvar") {
        let mut it = rest.split_whitespace();
        let name = it.next().ok_or(":setvar: 이름이 필요합니다")?.to_string();
        let value = it.collect::<Vec<_>>().join(" ");
        let value = if value.is_empty() {
            None
        } else {
            Some(value.trim_matches('"').to_string())
        };
        return Ok(Some(Command::SetVar { name, value }));
    }
    if let Some(rest) = t.strip_prefix(":connect") {
        return Ok(Some(Command::Connect(ConnectSpec::parse(rest)?)));
    }
    if let Some(rest) = t.strip_prefix(":r ") {
        return Ok(Some(Command::Include {
            path: rest.trim().to_string(),
        }));
    }
    let (word, rest) = split_first_word(t);
    let up = word.to_ascii_uppercase();
    let rest = rest.trim();
    Ok(Some(match up.as_str() {
        "VAR" | "VARIABLE" => parse_variable(rest)?,
        "PRINT" => {
            // T-SQL `PRINT 'text'` · `PRINT @v` · `PRINT (expr)`는 서버 문장이다 — 바인드 이름 목록일 때만 SQL*Plus PRINT.
            let is_name = |s: &str| {
                let s = s.trim_start_matches(':');
                !s.is_empty()
                    && s.chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && s.bytes().all(crate::lexer::is_ident_char)
            };
            if !rest.split_whitespace().all(is_name) {
                return Ok(None);
            }
            Command::Print {
                names: rest
                    .split_whitespace()
                    .map(|s| s.trim_start_matches(':').to_ascii_uppercase())
                    .collect(),
            }
        }
        "EXEC" | "EXECUTE" => Command::Exec {
            body: rest.trim_end_matches(';').trim().to_string(),
        },
        "CONN" | "CONNECT" => Command::Connect(ConnectSpec::parse(rest)?),
        "DISC" | "DISCONNECT" => Command::Disconnect,
        "SET" => match parse_set(rest)? {
            Some(c) => c,
            None => return Ok(None), // 서버 SET 문(T-SQL NOCOUNT/XACT_ABORT/SHOWPLAN · Oracle TRANSACTION/ROLE · PG search_path …)
        },
        "DEF" | "DEFINE" => {
            if rest.is_empty() {
                Command::Define {
                    name: None,
                    value: None,
                }
            } else if let Some((n, v)) = rest.split_once('=') {
                Command::Define {
                    name: Some(n.trim().to_ascii_uppercase()),
                    value: Some(unquote(v.trim())),
                }
            } else {
                Command::Define {
                    name: Some(rest.to_ascii_uppercase()),
                    value: None,
                }
            }
        }
        "ACC" | "ACCEPT" => parse_accept(rest)?,
        "COL" | "COLUMN" => parse_column(rest)?,
        "UNDEF" | "UNDEFINE" => Command::Undefine {
            names: rest
                .split_whitespace()
                .map(|s| s.to_ascii_uppercase())
                .collect(),
        },
        "DESC" | "DESCRIBE" => Command::Describe {
            object: rest.to_string(),
        },
        "SHO" | "SHOW" => Command::Show {
            what: rest.to_string(),
        },
        "SPO" | "SPOOL" => Command::Spool {
            target: rest.to_string(),
        },
        "PROMPT" => Command::Prompt {
            text: rest.to_string(),
        },
        "REM" | "REMARK" => Command::Remark,
        "GO" => Command::Go {
            repeat: rest.parse().unwrap_or(1),
        },
        "WHENEVER" => {
            let r = rest.to_ascii_uppercase();
            Command::Whenever {
                on_error_exit: r.starts_with("SQLERROR EXIT"),
            }
        }
        _ => return Ok(None),
    }))
}

fn run_cmd(rest: &str, relative: bool) -> Command {
    let mut it = rest.split_whitespace();
    let path = it.next().unwrap_or("").to_string();
    Command::Run {
        path,
        args: it.map(str::to_string).collect(),
        relative_to_caller: relative,
    }
}

fn split_first_word(t: &str) -> (&str, &str) {
    let n = t
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .map(char::len_utf8)
        .sum();
    (&t[..n], &t[n..])
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
        || (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
    {
        v[1..v.len() - 1].replace("''", "'")
    } else {
        v.to_string()
    }
}

/// 낱말로 나눈다 — 따옴표(`'…'` · `"…"`) 안의 공백은 나누지 않고, 따옴표는 벗긴다(`''` = `'`).
fn quoted_words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut had = false;
    let mut it = s.chars().peekable();
    while let Some(ch) = it.next() {
        match quote {
            Some(q) if ch == q => {
                if it.peek() == Some(&q) {
                    cur.push(q);
                    it.next();
                } else {
                    quote = None;
                }
            }
            Some(_) => cur.push(ch),
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                had = true;
            }
            None if ch.is_whitespace() => {
                if had || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    had = false;
                }
            }
            None => cur.push(ch),
        }
    }
    if had || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// `COLUMN 열 [NEW_VALUE 변수] [CLEAR] [그 밖의 표시 옵션…]`.
fn parse_column(rest: &str) -> Result<Command, String> {
    let words = quoted_words(rest);
    let mut it = words.into_iter();
    let name = it
        .next()
        .ok_or_else(|| "COLUMN: 열 이름이 필요합니다".to_string())?
        .to_ascii_uppercase();
    let (mut new_value, mut clear) = (None, false);
    while let Some(w) = it.next() {
        let up = w.to_ascii_uppercase();
        if up.len() >= 5 && "NEW_VALUE".starts_with(up.as_str()) {
            new_value = Some(
                it.next()
                    .filter(|v| v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
                    .ok_or_else(|| "COLUMN … NEW_VALUE: 변수 이름이 필요합니다".to_string())?
                    .to_ascii_uppercase(),
            );
        } else if up == "CLE" || up == "CLEAR" {
            clear = true;
        }
    }
    Ok(Command::Column {
        name,
        new_value,
        clear,
    })
}

/// `ACCEPT name [타입] [FORMAT f] [DEFAULT v] [PROMPT 글|NOPROMPT] [HIDE]`.
fn parse_accept(rest: &str) -> Result<Command, String> {
    let words = quoted_words(rest);
    let mut it = words.into_iter();
    let name = it
        .next()
        .filter(|n| n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        .ok_or_else(|| "ACCEPT: 변수 이름이 필요합니다".to_string())?
        .to_ascii_uppercase();
    let (mut default, mut prompt, mut hide) = (None, None, false);
    while let Some(w) = it.next() {
        match w.to_ascii_uppercase().as_str() {
            "NUM" | "NUMBER" | "CHAR" | "DATE" | "BINARY_FLOAT" | "BINARY_DOUBLE" => {}
            "FOR" | "FORMAT" => {
                it.next();
            }
            "DEF" | "DEFAULT" => default = it.next(),
            "PROMPT" => prompt = it.next(),
            "NOPR" | "NOPROMPT" => prompt = Some(String::new()),
            "HIDE" => hide = true,
            other => return Err(format!("ACCEPT: 알 수 없는 옵션 {other}")),
        }
    }
    Ok(Command::Accept {
        name,
        default,
        prompt,
        hide,
    })
}

/// `VARIABLE [name [type [= value]]]`.
fn parse_variable(rest: &str) -> Result<Command, String> {
    if rest.is_empty() {
        return Ok(Command::Variable {
            name: None,
            ty: None,
            init: None,
        });
    }
    let (name, after) = {
        let n = rest
            .chars()
            .take_while(|c| !c.is_whitespace() && *c != '=')
            .map(char::len_utf8)
            .sum();
        (&rest[..n], rest[n..].trim())
    };
    // 이름은 **친 그대로**(표기 보존 D-142 — 저장소의 열쇠는 `norm`이 대문자로 만든다 · 종전에는 여기서 대문자로 바꿔
    // `VAR v_Name NUMBER`의 표기가 `V_NAME`으로 보였다 · T-151).
    let bare = name.trim_start_matches(':').to_string();
    let name = Some(bare.clone());
    if after.is_empty() {
        return Ok(Command::Variable {
            name,
            ty: None,
            init: None,
        });
    }
    match after.to_ascii_uppercase().as_str() {
        "SHARE" | "SHARED" => {
            return Ok(Command::VarScope {
                name: bare,
                shared: true,
            })
        }
        "LOCAL" | "UNSHARE" => {
            return Ok(Command::VarScope {
                name: bare,
                shared: false,
            })
        }
        _ => {}
    }
    let (ty_text, init_text) = match after.find('=') {
        Some(i) => (after[..i].trim(), Some(after[i + 1..].trim())),
        None => (after, None),
    };
    let ty =
        VarType::parse(ty_text).ok_or_else(|| format!("VARIABLE: 알 수 없는 타입 '{ty_text}'"))?;
    let init = init_text.map(parse_literal).transpose()?;
    Ok(Command::Variable {
        name,
        ty: Some(ty),
        init,
    })
}

/// 대입 우변이 리터럴이면 값으로. 문자열('…' · ''이스케이프) · 정수 · 실수 · NULL.
pub fn parse_literal(s: &str) -> Result<Value, String> {
    let s = s.trim().trim_end_matches(';').trim();
    if s.eq_ignore_ascii_case("null") {
        return Ok(Value::Null);
    }
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        let inner = &s[1..s.len() - 1];
        // 홀수 개의 따옴표가 중간에 있으면 리터럴이 아니다('a' || 'b').
        if inner.replace("''", "").contains('\'') {
            return Err("리터럴이 아님".into());
        }
        return Ok(Value::Str(inner.replace("''", "'")));
    }
    if let Ok(i) = s.parse::<i64>() {
        return Ok(Value::Int(i));
    }
    if s.parse::<f64>().is_ok()
        && s.bytes()
            .all(|c| c.is_ascii_digit() || c == b'.' || c == b'-' || c == b'+')
    {
        return Ok(Value::Decimal(s.to_string()));
    }
    Err("리터럴이 아님".into())
}

/// SQL*Plus 표시·세션 옵션 이름(우리가 먹는 것) — 그 밖의 `SET …`은 서버 문장으로 통과한다(09-15 · T-SQL `SET NOCOUNT ON` 등이
/// 조용히 무시되던 결함 수정).
const SQLPLUS_SET_NAMES: &[&str] = &[
    "PAGES",
    "PAGESIZE",
    "LIN",
    "LINESIZE",
    "HEA",
    "HEADING",
    "TERM",
    "TERMOUT",
    "TRIMS",
    "TRIMSPOOL",
    "TRIM",
    "TRIMOUT",
    "NULL",
    "COLSEP",
    "SQLBL",
    "SQLBLANKLINES",
    "WRAP",
    "LONG",
    "LONGC",
    "LONGCHUNKSIZE",
    "NUMF",
    "NUMFORMAT",
    "NUM",
    "NUMWIDTH",
    "TAB",
    "SPACE",
    "PAUSE",
    "ESC",
    "ESCAPE",
    "SCAN",
    "CONCAT",
    "SUFFIX",
    "SQLPROMPT",
    "SQLP",
    "TIME",
    "EXITC",
    "EXITCOMMIT",
    "ERRORL",
    "ERRORLOGGING",
    "APPI",
    "APPINFO",
    "ROWPREFETCH",
    "LOBPREFETCH",
    "STATEMENTCACHE",
    "MARK",
    "MARKUP",
    "SHOW",
    "SHOWMODE",
    "UND",
    "UNDERLINE",
    "RECSEP",
    "RECSEPCHAR",
    "NEWP",
    "NEWPAGE",
    "EMB",
    "EMBEDDED",
    "FLU",
    "FLUSH",
    "BLO",
    "BLOCKTERMINATOR",
    "CMDS",
    "CMDSEP",
    "COPYC",
    "COPYCOMMIT",
    "COPYTYPECHECK",
    "DESCRIBE",
    "EDITF",
    "EDITFILE",
    "FLAGGER",
    "INSTANCE",
    "LOBOF",
    "LOBOFFSET",
    "LOGSOURCE",
    "SECUREDCOL",
    "SHIFT",
    "SHIFTINOUT",
    "SQLC",
    "SQLCASE",
    "SQLCO",
    "SQLCONTINUE",
    "SQLN",
    "SQLNUMBER",
    "SQLPLUSCOMPAT",
    "SQLPLUSCOMPATIBILITY",
    "SQLPRE",
    "SQLPREFIX",
    "SQLT",
    "SQLTERMINATOR",
    "XQUERY",
    "ENCODING",
    "DDL",
    "HISTORY",
    "HIST",
];

fn parse_set(rest: &str) -> Result<Option<Command>, String> {
    let (name, value) = split_first_word(rest);
    let name_up = name.to_ascii_uppercase();
    let value = value.trim();
    Ok(Some(Command::Set(match name_up.as_str() {
        "SERVEROUT" | "SERVEROUTPUT" => SetOption::ServerOutput {
            on: on_off(value.split_whitespace().next().unwrap_or("")),
        },
        "TIMI" | "TIMING" => SetOption::Timing(on_off(value)),
        "AUTOCOMMIT" => SetOption::AutoCommit(on_off(value)),
        "AUTOP" | "AUTOPRINT" => SetOption::AutoPrint(on_off(value)),
        "FEED" | "FEEDBACK" => SetOption::Feedback(on_off(value) || value.parse::<u32>().is_ok()),
        "DEF" | "DEFINE" => {
            let v = value.trim();
            SetOption::Define(if v.eq_ignore_ascii_case("off") {
                None
            } else if v.eq_ignore_ascii_case("on") || v.is_empty() {
                Some('&')
            } else {
                v.chars().next()
            })
        }
        "VER" | "VERIFY" => SetOption::Verify(on_off(value)),
        "ECHO" => SetOption::Echo(on_off(value)),
        "ARRAY" | "ARRAYSIZE" | "FETCHROWS" => SetOption::FetchSize(
            value
                .parse()
                .map_err(|_| format!("SET {name}: 숫자가 필요합니다"))?,
        ),
        "SQLFORMAT" => SetOption::SqlFormat(value.to_ascii_lowercase()),
        n if SQLPLUS_SET_NAMES.contains(&n) => SetOption::Other {
            name: name_up,
            value: value.to_string(),
        },
        _ => return Ok(None),
    })))
}

/// `SPOOL` 인자 해석 결과(호스트가 파일을 연다 · T-9). SQL*Plus 의미: 확장자가 없으면 `.lst`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SpoolCmd {
    /// 맨몸 `SPOOL` — 현재 대상 보고.
    Status,
    /// `SPOOL OFF` · `SPOOL OUT`(OUT = 인쇄 없이 닫기와 동일).
    Off,
    Start {
        path: String,
        mode: SpoolMode,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpoolMode {
    /// 기본 — 있으면 덮어쓴다(SQL*Plus `REPLACE`).
    Replace,
    Append,
    /// 이미 있으면 오류.
    Create,
}

/// `SPOOL [file [CREATE|REPLACE|APPEND] | OFF | OUT]`. 경로는 따옴표(`"a b.lst"`)로 감쌀 수 있다.
/// 실패 = 알 수 없는 옵션 토큰(호스트가 메시지로).
pub fn parse_spool(target: &str) -> Result<SpoolCmd, String> {
    let t = target.trim();
    if t.is_empty() {
        return Ok(SpoolCmd::Status);
    }
    let (path, rest) = if let Some(q) = t.strip_prefix('"') {
        match q.find('"') {
            Some(i) => (q[..i].to_string(), q[i + 1..].trim()),
            None => (q.to_string(), ""),
        }
    } else {
        let n = t.find(char::is_whitespace).unwrap_or(t.len());
        (t[..n].to_string(), t[n..].trim())
    };
    if rest.is_empty() && matches!(path.to_ascii_uppercase().as_str(), "OFF" | "OUT") {
        return Ok(SpoolCmd::Off);
    }
    let mode = match rest.to_ascii_uppercase().as_str() {
        "" | "REP" | "REPLACE" => SpoolMode::Replace,
        "APP" | "APPEND" => SpoolMode::Append,
        "CRE" | "CREATE" => SpoolMode::Create,
        _ => return Err(rest.to_string()),
    };
    Ok(SpoolCmd::Start {
        path: spool_default_ext(&path),
        mode,
    })
}

/// 마지막 경로 요소에 확장자가 없으면 `.lst`(SQL*Plus).
fn spool_default_ext(path: &str) -> String {
    let base = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if base.contains('.') {
        path.to_string()
    } else {
        format!("{path}.lst")
    }
}

/// 실행 계획 스크립트(GUI Explain · CLI `nsql explain`) — 방언별 관용. `stmt`는 한 문장(끝 `;` 없어도 됨).
#[must_use]
pub fn explain_script(dialect: Dialect, stmt: &str) -> String {
    let s = stmt.trim().trim_end_matches(';').trim();
    match dialect {
        Dialect::Oracle => {
            format!("EXPLAIN PLAN FOR {s};\nSELECT * FROM TABLE(DBMS_XPLAN.DISPLAY());\n")
        }
        // DECLARE로 시작해야 어댑터가 sp_executesql(RPC)이 아닌 배치로 보낸다 — SHOWPLAN 결과 집합은 배치에서만 온다(09-15 실측).
        Dialect::Mssql => format!(
            "SET SHOWPLAN_TEXT ON;
GO
DECLARE @nsql_plan BIT;
{s};
GO
SET SHOWPLAN_TEXT OFF;
GO
"
        ),
        Dialect::Postgres => format!("EXPLAIN (VERBOSE, COSTS) {s};\n"),
        Dialect::Sqlite => format!("EXPLAIN QUERY PLAN {s};\n"),
        Dialect::Mysql | Dialect::Odbc => format!("EXPLAIN {s};\n"),
    }
}

#[cfg(test)]
mod tests {
    /// T-153 — `COLUMN … NEW_VALUE`: 줄임말(`COL` · `NEW_V`) · 표시 옵션은 무시 · CLEAR · 이름 없음 = 오류.
    #[test]
    fn column_new_value_command() {
        let p = |s: &str| super::parse_command(s);
        assert_eq!(
            p("COLUMN max_id NEW_VALUE v_max"),
            Ok(Some(super::Command::Column {
                name: "MAX_ID".into(),
                new_value: Some("V_MAX".into()),
                clear: false,
            }))
        );
        assert_eq!(
            p("col dt format a20 new_v today heading 'Today'"),
            Ok(Some(super::Command::Column {
                name: "DT".into(),
                new_value: Some("TODAY".into()),
                clear: false,
            }))
        );
        assert_eq!(
            p("COLUMN x CLEAR"),
            Ok(Some(super::Command::Column {
                name: "X".into(),
                new_value: None,
                clear: true,
            }))
        );
        assert!(p("COLUMN").is_err());
        assert!(p("COLUMN x NEW_VALUE").is_err());
    }

    /// T-153 — `ACCEPT`: 줄임말 · 타입/형식 낱말은 건너뛴다 · 따옴표 안의 공백 · DEFAULT/PROMPT/NOPROMPT/HIDE · 이름 없음·모르는 옵션 = 오류.
    #[test]
    fn accept_command_options() {
        let p = |s: &str| super::parse_command(s);
        assert_eq!(
            p("ACCEPT dept NUMBER FORMAT '999' DEFAULT 10 PROMPT 'Department no: ' HIDE"),
            Ok(Some(super::Command::Accept {
                name: "DEPT".into(),
                default: Some("10".into()),
                prompt: Some("Department no: ".into()),
                hide: true,
            }))
        );
        assert_eq!(
            p("acc who def 'O''Neil' nopr"),
            Ok(Some(super::Command::Accept {
                name: "WHO".into(),
                default: Some("O'Neil".into()),
                prompt: Some(String::new()),
                hide: false,
            }))
        );
        assert_eq!(
            p("ACCEPT x"),
            Ok(Some(super::Command::Accept {
                name: "X".into(),
                default: None,
                prompt: None,
                hide: false,
            }))
        );
        assert!(p("ACCEPT").is_err());
        assert!(p("ACCEPT x BOGUS").is_err());
        assert!(super::is_command_start("ACCEPT x"));
        assert!(!super::is_command_start("ACCEPTED_ROWS := 1"));
    }

    use super::*;

    #[test]
    fn server_set_statements_pass_through_and_sqlplus_set_is_command() {
        assert!(parse_command("SET NOCOUNT ON").unwrap().is_none());
        assert!(
            parse_command("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
                .unwrap()
                .is_none()
        );
        assert!(parse_command("SET search_path = public").unwrap().is_none());
        assert!(matches!(
            parse_command("SET SERVEROUTPUT ON").unwrap(),
            Some(Command::Set(SetOption::ServerOutput { on: true }))
        ));
        assert!(matches!(
            parse_command("SET PAGESIZE 0").unwrap(),
            Some(Command::Set(SetOption::Other { .. }))
        ));
        assert!(explain_script(Dialect::Oracle, "SELECT 1 FROM dual;")
            .starts_with("EXPLAIN PLAN FOR SELECT 1 FROM dual;"));
        assert!(explain_script(Dialect::Mssql, "SELECT 1").contains("SHOWPLAN_TEXT ON"));
    }

    #[test]
    fn variable_forms() {
        assert_eq!(
            parse_command("VARIABLE").unwrap(),
            Some(Command::Variable {
                name: None,
                ty: None,
                init: None
            })
        );
        assert_eq!(
            parse_command("var v_cnt number").unwrap(),
            Some(Command::Variable {
                name: Some("v_cnt".into()),
                ty: Some(VarType::Number),
                init: None
            })
        );
        assert_eq!(
            parse_command("VARIABLE rc REFCURSOR").unwrap(),
            Some(Command::Variable {
                name: Some("rc".into()),
                ty: Some(VarType::RefCursor),
                init: None
            })
        );
        assert_eq!(
            parse_command("VAR xyz VARCHAR2(10)='te''st'").unwrap(),
            Some(Command::Variable {
                name: Some("xyz".into()),
                ty: Some(VarType::Varchar2(10)),
                init: Some(Value::Str("te'st".into()))
            })
        );
        assert!(parse_command("VAR x BLOB").is_err());
    }

    #[test]
    fn exec_print_connect_set() {
        assert_eq!(
            parse_command("EXEC :V := 'x';").unwrap(),
            Some(Command::Exec {
                body: ":V := 'x'".into()
            })
        );
        assert_eq!(
            parse_command("print v_a :v_b").unwrap(),
            Some(Command::Print {
                names: vec!["V_A".into(), "V_B".into()]
            })
        );
        assert!(matches!(
            parse_command("connect u/p@h:1521/s;").unwrap(),
            Some(Command::Connect(_))
        ));
        assert_eq!(
            parse_command("SET SERVEROUTPUT ON SIZE 1000000").unwrap(),
            Some(Command::Set(SetOption::ServerOutput { on: true }))
        );
        assert_eq!(
            parse_command("set define off").unwrap(),
            Some(Command::Set(SetOption::Define(None)))
        );
        assert_eq!(
            parse_command("SET ARRAYSIZE 500").unwrap(),
            Some(Command::Set(SetOption::FetchSize(500)))
        );
        assert_eq!(
            parse_command("GO 3").unwrap(),
            Some(Command::Go { repeat: 3 })
        );
        assert_eq!(
            parse_command(":setvar Env \"prod\"").unwrap(),
            Some(Command::SetVar {
                name: "Env".into(),
                value: Some("prod".into())
            })
        );
        assert_eq!(
            parse_command("@@sub/x.sql a b").unwrap(),
            Some(Command::Run {
                path: "sub/x.sql".into(),
                args: vec!["a".into(), "b".into()],
                relative_to_caller: true
            })
        );
        assert_eq!(
            parse_command("DEFINE dept = 'SALES'").unwrap(),
            Some(Command::Define {
                name: Some("DEPT".into()),
                value: Some("SALES".into())
            })
        );
        assert_eq!(parse_command("SELECT 1 FROM dual").unwrap(), None);
    }

    #[test]
    fn command_start_detection() {
        assert!(is_command_start(
            "  exec dbms_stats.gather_table_stats(user, 't')"
        ));
        assert!(is_command_start("SET SERVEROUTPUT ON"));
        assert!(is_command_start("@x.sql"));
        assert!(!is_command_start("SETTINGS_TABLE t"));
        assert!(!is_command_start("SELECT 1"));
        assert!(!is_command_start("EXECUTION_LOG"));
        assert!(is_command_start("GO"));
    }

    /// docs/52 §5: `CONNECT BY …` 조각은 접속 명령이 아니다 · `:disconnect`는 `:connect`의 짝.
    #[test]
    fn connect_by_fragment_is_sql_and_colon_disconnect() {
        assert!(!is_command_start("CONNECT BY PRIOR empno = mgr"));
        assert_eq!(parse_command("connect by level < 3;"), Ok(None));
        assert!(is_command_start("CONNECT BY"), "프로필 이름 BY");
        assert!(is_command_start("CONNECT \"oracle:h:1521/x\""));
        assert!(is_command_start(":disconnect"));
        assert_eq!(parse_command(":disconnect"), Ok(Some(Command::Disconnect)));
        assert_eq!(parse_command("DISC"), Ok(Some(Command::Disconnect)));
    }

    #[test]
    fn spool_forms() {
        assert_eq!(parse_spool(""), Ok(SpoolCmd::Status));
        assert_eq!(parse_spool("off"), Ok(SpoolCmd::Off));
        assert_eq!(parse_spool("OUT"), Ok(SpoolCmd::Off));
        assert_eq!(
            parse_spool("out.txt"),
            Ok(SpoolCmd::Start {
                path: "out.txt".into(),
                mode: SpoolMode::Replace
            })
        );
        // 확장자 없음 → .lst · 디렉터리에 점이 있어도 파일 이름만 본다.
        assert_eq!(
            parse_spool("./a.b/report append"),
            Ok(SpoolCmd::Start {
                path: "./a.b/report.lst".into(),
                mode: SpoolMode::Append
            })
        );
        assert_eq!(
            parse_spool("\"my out.log\" CREATE"),
            Ok(SpoolCmd::Start {
                path: "my out.log".into(),
                mode: SpoolMode::Create
            })
        );
        assert_eq!(parse_spool("x.lst bogus"), Err("bogus".into()));
        // 명령 해석기와 이어진다.
        assert_eq!(
            parse_command("spo out.lst"),
            Ok(Some(Command::Spool {
                target: "out.lst".into()
            }))
        );
    }

    /// `@`는 cwd 기준 · `@@`는 호출 스크립트 기준 · 인자는 `&1..&n`(호스트가 정의) · sqlcmd `:r`도 포함.
    #[test]
    fn include_forms() {
        assert_eq!(
            parse_command("@init.sql 2026 Q1").unwrap(),
            Some(Command::Run {
                path: "init.sql".into(),
                args: vec!["2026".into(), "Q1".into()],
                relative_to_caller: false
            })
        );
        assert_eq!(
            parse_command("@@lib/common.sql").unwrap(),
            Some(Command::Run {
                path: "lib/common.sql".into(),
                args: vec![],
                relative_to_caller: true
            })
        );
        assert_eq!(
            parse_command(":r C:\\x\\a.sql").unwrap(),
            Some(Command::Include {
                path: "C:\\x\\a.sql".into()
            })
        );
        assert!(is_command_start("@@a.sql"));
        assert!(is_command_start(":r a.sql"));
    }

    #[test]
    fn literals() {
        assert_eq!(parse_literal("'a''b'").unwrap(), Value::Str("a'b".into()));
        assert_eq!(parse_literal("42").unwrap(), Value::Int(42));
        assert_eq!(
            parse_literal("3.14").unwrap(),
            Value::Decimal("3.14".into())
        );
        assert_eq!(parse_literal("NULL").unwrap(), Value::Null);
        assert!(parse_literal("'a' || 'b'").is_err());
        assert!(parse_literal("SYSDATE").is_err());
    }
}
