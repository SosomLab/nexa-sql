//! 스크립트 명령(SQL이 아닌 줄) 해석 — SQL*Plus 부분집합 + sqlcmd 호환(docs/08 §2).
//!
//! 지원: `VAR[IABLE]` · `PRINT` · `EXEC[UTE]` · `CONN[ECT]` · `DISC[ONNECT]` · `SET` ·
//! `DEF[INE]` · `UNDEF[INE]` · `DESC[RIBE]` · `SHO[W]` · `SPO[OL]` · `PROMPT` · `@` `@@` ·
//! `REM[ARK]` · `GO [n]` · `:setvar` `:connect` `:r`(sqlcmd) · `WHENEVER SQLERROR`.

use crate::connect::ConnectSpec;
use nsql_core::{Value, VarType};

#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    /// `VARIABLE` — 인자 없음 = 목록 · 이름만 = 조회 · 이름+타입[=값] = 선언.
    Variable {
        name: Option<String>,
        ty: Option<VarType>,
        init: Option<Value>,
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
        || l.starts_with(":r ")
    {
        return true;
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

/// 한 줄(또는 EXEC 블록 텍스트)을 명령으로 해석한다. SQL이면 `Ok(None)`.
pub fn parse_command(text: &str) -> Result<Option<Command>, String> {
    let t = text.trim().trim_end_matches(';').trim();
    if t.is_empty() {
        return Ok(None);
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
        "PRINT" => Command::Print {
            names: rest
                .split_whitespace()
                .map(|s| s.trim_start_matches(':').to_ascii_uppercase())
                .collect(),
        },
        "EXEC" | "EXECUTE" => Command::Exec {
            body: rest.trim_end_matches(';').trim().to_string(),
        },
        "CONN" | "CONNECT" => Command::Connect(ConnectSpec::parse(rest)?),
        "DISC" | "DISCONNECT" => Command::Disconnect,
        "SET" => parse_set(rest)?,
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
    let name = Some(name.trim_start_matches(':').to_ascii_uppercase());
    if after.is_empty() {
        return Ok(Command::Variable {
            name,
            ty: None,
            init: None,
        });
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

fn parse_set(rest: &str) -> Result<Command, String> {
    let (name, value) = split_first_word(rest);
    let name_up = name.to_ascii_uppercase();
    let value = value.trim();
    Ok(Command::Set(match name_up.as_str() {
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
        _ => SetOption::Other {
            name: name_up,
            value: value.to_string(),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

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
                name: Some("V_CNT".into()),
                ty: Some(VarType::Number),
                init: None
            })
        );
        assert_eq!(
            parse_command("VARIABLE rc REFCURSOR").unwrap(),
            Some(Command::Variable {
                name: Some("RC".into()),
                ty: Some(VarType::RefCursor),
                init: None
            })
        );
        assert_eq!(
            parse_command("VAR xyz VARCHAR2(10)='te''st'").unwrap(),
            Some(Command::Variable {
                name: Some("XYZ".into()),
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
