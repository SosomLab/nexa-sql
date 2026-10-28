//! ★ T-14 2단계(10-29 · D-393 · 사용자 "레지스트리 먼저 → 파서·context"): **`.sublime-keymap` 파서 + context 평가기**.
//!
//! 파일 형식 = Sublime Text 키 바인딩(JSON 배열 · 주석(`//` `/* */`)·꼬리 쉼표 허용):
//! `[{ "keys": ["ctrl+k", "ctrl+u"], "command": "upper_case", "args": {…}, "context": [{ "key": "selection_empty",
//! "operator": "equal", "operand": false, "match_all": true }] }]`. 파서·평가기는 **순수 함수**(앱 상태 없음) —
//! 적용은 [`crate::keymap::Keymap::apply_bindings`](context 없는 바인딩 = 지금 · context 있는 바인딩 = 보관 · 3단계에서
//! [`ContextSource`]로 평가). 명령 이름 = 이 앱의 id(`file.save`) 또는 Sublime 이름(`save` → [`sublime_alias`]).
//! 파일 위치 = `<설정 폴더>/Packages/User/Default (<OS>).sublime-keymap` · `Default.sublime-keymap`(Sublime 관례 · D-395).

use nsql_json::Json;

/// 바인딩 하나.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Binding {
    /// 코드 열 — 1개 = 단일 조합 · 2개 = 시퀀스(`ctrl+k`, `ctrl+u`). 3개 이상은 보관만(이 앱의 키맵은 2단까지).
    pub keys: Vec<String>,
    pub command: String,
    pub args: Option<Json>,
    pub context: Vec<Context>,
}

/// context 조건 하나(Sublime `{key, operator, operand, match_all}`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Context {
    pub key: String,
    pub op: Op,
    pub operand: Json,
    pub match_all: bool,
}

/// Sublime 연산자 6종.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    Equal,
    NotEqual,
    RegexMatch,
    NotRegexMatch,
    RegexContains,
    NotRegexContains,
}

impl Op {
    fn parse(s: &str) -> Option<Op> {
        Some(match s {
            "equal" => Op::Equal,
            "not_equal" => Op::NotEqual,
            "regex_match" => Op::RegexMatch,
            "not_regex_match" => Op::NotRegexMatch,
            "regex_contains" => Op::RegexContains,
            "not_regex_contains" => Op::NotRegexContains,
            _ => return None,
        })
    }
}

/// 주석(`//` 줄 · `/* … */`)과 꼬리 쉼표(`,]` `,}`)를 걷어 엄격 JSON으로 — 문자열 안은 건드리지 않는다.
pub(crate) fn strip_comments(text: &str) -> String {
    let b: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
                i += 1;
            }
            '/' if i + 1 < b.len() && b[i + 1] == '/' => {
                while i < b.len() && b[i] != '\n' {
                    i += 1;
                }
            }
            '/' if i + 1 < b.len() && b[i + 1] == '*' => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            ',' => {
                // 꼬리 쉼표: 다음 비공백·비주석 글자가 `]`/`}`면 버린다.
                let mut j = i + 1;
                while j < b.len() && b[j].is_whitespace() {
                    j += 1;
                }
                if j < b.len() && (b[j] == ']' || b[j] == '}') {
                    i += 1;
                    continue;
                }
                out.push(c);
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn field<'a>(obj: &'a [(String, Json)], name: &str) -> Option<&'a Json> {
    obj.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

/// 파일 본문 → 바인딩 목록. 오류 = 줄 번호 없는 한 문장(파일 하나가 통째로 거부된다 · Sublime과 같다).
pub(crate) fn parse(text: &str) -> Result<Vec<Binding>, String> {
    let json = nsql_json::parse(&strip_comments(text))?;
    let Json::Arr(items) = json else {
        return Err("keymap: top level must be an array of bindings".to_string());
    };
    let mut out = Vec::with_capacity(items.len());
    for (n, it) in items.iter().enumerate() {
        let Json::Obj(obj) = it else {
            return Err(format!("keymap: binding #{} is not an object", n + 1));
        };
        let keys: Vec<String> = match field(obj, "keys") {
            Some(Json::Arr(ks)) => ks
                .iter()
                .filter_map(|k| match k {
                    Json::Str(s) => Some(s.trim().to_ascii_lowercase()),
                    _ => None,
                })
                .filter(|s| !s.is_empty())
                .collect(),
            Some(Json::Str(s)) => vec![s.trim().to_ascii_lowercase()],
            _ => Vec::new(),
        };
        if keys.is_empty() {
            return Err(format!("keymap: binding #{} has no keys", n + 1));
        }
        let command = match field(obj, "command") {
            Some(Json::Str(s)) if !s.trim().is_empty() => s.trim().to_string(),
            _ => return Err(format!("keymap: binding #{} has no command", n + 1)),
        };
        let args = field(obj, "args").cloned();
        let mut context = Vec::new();
        if let Some(Json::Arr(cs)) = field(obj, "context") {
            for c in cs {
                let Json::Obj(co) = c else {
                    return Err(format!(
                        "keymap: binding #{} context is not an object",
                        n + 1
                    ));
                };
                let key = match field(co, "key") {
                    Some(Json::Str(s)) => s.trim().to_string(),
                    _ => return Err(format!("keymap: binding #{} context has no key", n + 1)),
                };
                let op = match field(co, "operator") {
                    Some(Json::Str(s)) => Op::parse(s).ok_or_else(|| {
                        format!("keymap: binding #{} unknown operator '{s}'", n + 1)
                    })?,
                    None => Op::Equal,
                    _ => return Err(format!("keymap: binding #{} bad operator", n + 1)),
                };
                let operand = field(co, "operand").cloned().unwrap_or(Json::Bool(true));
                let match_all = matches!(field(co, "match_all"), Some(Json::Bool(true)));
                context.push(Context {
                    key,
                    op,
                    operand,
                    match_all,
                });
            }
        }
        out.push(Binding {
            keys,
            command,
            args,
            context,
        });
    }
    Ok(out)
}

/// Sublime 명령 이름 → 이 앱의 명령 id(사용자가 Sublime 키맵을 그대로 가져올 때). 모르는 이름은 `None`(그대로 id로 본다).
pub(crate) fn sublime_alias(cmd: &str) -> Option<&'static str> {
    Some(match cmd {
        "new_file" => "file.new",
        "prompt_open_file" | "prompt_open" => "file.open",
        "save" => "file.save",
        "prompt_save_as" | "save_as" => "file.save_as",
        "close" | "close_file" => "file.close_tab",
        "exit" => "file.exit",
        "undo" => "edit.undo",
        "redo" | "redo_or_repeat" => "edit.redo",
        "soft_undo" => "edit.soft_undo",
        "soft_redo" => "edit.soft_redo",
        "cut" => "edit.cut",
        "copy" => "edit.copy",
        "paste" => "edit.paste",
        "select_all" => "edit.select_all",
        "expand_selection" => "edit.expand_selection",
        "find_under_expand" => "edit.expand_selection",
        "find_under_expand_skip" => "edit.skip_occurrence",
        "find_all_under" => "edit.select_all_occurrences",
        "duplicate_line" => "edit.duplicate_line",
        "join_lines" => "edit.join_lines",
        "toggle_comment" => "edit.toggle_comment",
        "indent" => "edit.indent",
        "unindent" => "edit.unindent",
        "upper_case" => "edit.upper_case",
        "lower_case" => "edit.lower_case",
        "split_selection_into_lines" => "edit.split_lines",
        "auto_complete" => "edit.complete",
        "show_panel" => "edit.find",
        "find_next" => "edit.find_next",
        "find_prev" => "edit.find_prev",
        "toggle_bookmark" => "bookmark.toggle",
        "next_bookmark" => "bookmark.next",
        "prev_bookmark" => "bookmark.prev",
        "select_all_bookmarks" => "bookmark.select_all",
        "clear_bookmarks" => "bookmark.clear_doc",
        "show_overlay" => "view.palette",
        "toggle_side_bar" => "view.explorer",
        "increase_font_size" => "view.zoom_in",
        "decrease_font_size" => "view.zoom_out",
        "reset_font_size" => "view.zoom_reset",
        "next_view" => "tab.next",
        "prev_view" => "tab.prev",
        "goto_line" => "edit.goto_line",
        "goto_symbol_in_project" | "goto_symbol" => "goto.symbol",
        "reindent" | "reformat" => "edit.format",
        "toggle_setting" => return None,
        _ => return None,
    })
}

/// 평가 때 앱이 주는 값(3단계에서 `App`이 구현 · 지금은 시험용 구현만). `None` = 모르는 키 = 그 조건은 거짓.
pub(crate) trait ContextSource {
    fn value(&self, key: &str) -> Option<Json>;
}

fn json_eq(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Num(x), Json::Num(y)) => (x - y).abs() < 1e-9,
        (Json::Str(x), Json::Str(y)) => x == y,
        (Json::Bool(x), Json::Bool(y)) => x == y,
        (Json::Null, Json::Null) => true,
        // 글자 ↔ 숫자/불린은 글자 표현으로 비교(Sublime도 느슨하다).
        (Json::Str(x), other) | (other, Json::Str(x)) => {
            other.scalar_text().is_some_and(|t| t == *x)
        }
        _ => false,
    }
}

fn regex_ok(pattern: &str, text: &str, whole: bool) -> bool {
    let pat = if whole {
        format!("^(?:{pattern})$")
    } else {
        pattern.to_string()
    };
    regex::Regex::new(&pat).is_ok_and(|re| re.is_match(text))
}

/// 조건 하나 평가(값이 배열이면 `match_all` = 전부 · 아니면 하나라도).
fn eval_one(c: &Context, src: &dyn ContextSource) -> bool {
    let Some(v) = src.value(&c.key) else {
        return false;
    };
    let test = |val: &Json| -> bool {
        match c.op {
            Op::Equal => json_eq(val, &c.operand),
            Op::NotEqual => !json_eq(val, &c.operand),
            Op::RegexMatch | Op::NotRegexMatch | Op::RegexContains | Op::NotRegexContains => {
                let (Some(t), Some(p)) = (val.scalar_text(), c.operand.scalar_text()) else {
                    return false;
                };
                let whole = matches!(c.op, Op::RegexMatch | Op::NotRegexMatch);
                let hit = regex_ok(&p, &t, whole);
                if matches!(c.op, Op::NotRegexMatch | Op::NotRegexContains) {
                    !hit
                } else {
                    hit
                }
            }
        }
    };
    match &v {
        Json::Arr(items) if c.match_all => !items.is_empty() && items.iter().all(test),
        Json::Arr(items) => items.iter().any(test),
        other => test(other),
    }
}

/// 바인딩의 context 전부가 참인가(비어 있으면 참).
pub(crate) fn matches(ctx: &[Context], src: &dyn ContextSource) -> bool {
    ctx.iter().all(|c| eval_one(c, src))
}

/// 이 OS의 Sublime 키맵 파일 이름(`Default (Windows)` · `Default (OSX)` · `Default (Linux)`).
pub(crate) fn os_file_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Default (OSX).sublime-keymap"
    } else if cfg!(windows) {
        "Default (Windows).sublime-keymap"
    } else {
        "Default (Linux).sublime-keymap"
    }
}

/// 사용자 키맵 파일 후보(있는 것만) — `Packages/User/Default.sublime-keymap` 다음에 OS별 파일(뒤가 앞을 덮는다 · Sublime 순서).
pub(crate) fn user_files(config_dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let user = config_dir.join("Packages").join("User");
    ["Default.sublime-keymap", os_file_name()]
        .iter()
        .map(|n| user.join(n))
        .filter(|p| p.is_file())
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
    // Sublime 키맵 견본(주석 · 꼬리 쉼표 · context)
    [
        { "keys": ["ctrl+shift+p"], "command": "show_overlay", "args": {"overlay": "command_palette"} },
        { "keys": ["ctrl+k", "ctrl+u"], "command": "upper_case", },
        /* 선택이 없을 때만 */
        { "keys": ["ctrl+d"], "command": "find_under_expand",
          "context": [ { "key": "selection_empty", "operator": "equal", "operand": true, "match_all": true } ] },
        { "keys": ["f9"], "command": "run.statement", "context": [ { "key": "panel" } ] },
    ]
    "#;

    #[test]
    fn strip_keeps_strings_and_drops_comments_and_trailing_commas() {
        let s = strip_comments(r#"[ "a//b", /* x */ 1, ] // tail"#);
        assert_eq!(s.trim(), r#"[ "a//b",  1 ]"#);
        let s = strip_comments(r#"{"k": "v", }"#);
        assert_eq!(s, r#"{"k": "v" }"#);
        assert_eq!(strip_comments(r#""a\"b//c""#), r#""a\"b//c""#);
    }

    #[test]
    fn parses_keys_command_args_context() {
        let b = parse(SAMPLE).expect("parse");
        assert_eq!(b.len(), 4);
        assert_eq!(b[0].keys, vec!["ctrl+shift+p"]);
        assert_eq!(b[0].command, "show_overlay");
        assert!(matches!(&b[0].args, Some(Json::Obj(o)) if o.len() == 1));
        assert_eq!(b[1].keys, vec!["ctrl+k", "ctrl+u"]);
        assert!(b[1].context.is_empty());
        let c = &b[2].context[0];
        assert_eq!(
            (c.key.as_str(), c.op, c.match_all),
            ("selection_empty", Op::Equal, true)
        );
        assert_eq!(c.operand, Json::Bool(true));
        // operator·operand 생략 = equal · true.
        let c = &b[3].context[0];
        assert_eq!(
            (c.op, &c.operand, c.match_all),
            (Op::Equal, &Json::Bool(true), false)
        );
        // 오류 = 한 문장.
        assert!(parse("{}").unwrap_err().contains("array"));
        assert!(parse(r#"[{"keys": [], "command": "x"}]"#)
            .unwrap_err()
            .contains("no keys"));
        assert!(parse(r#"[{"keys": ["a"]}]"#)
            .unwrap_err()
            .contains("no command"));
        assert!(parse(
            r#"[{"keys": ["a"], "command": "x", "context": [{"key": "k", "operator": "weird"}]}]"#
        )
        .unwrap_err()
        .contains("unknown operator"));
    }

    #[test]
    fn aliases_map_common_sublime_names() {
        assert_eq!(sublime_alias("save"), Some("file.save"));
        assert_eq!(
            sublime_alias("find_under_expand"),
            Some("edit.expand_selection")
        );
        assert_eq!(sublime_alias("show_overlay"), Some("view.palette"));
        assert_eq!(sublime_alias("file.save"), None);
        assert_eq!(sublime_alias("toggle_setting"), None);
    }

    struct Src(Vec<(&'static str, Json)>);
    impl ContextSource for Src {
        fn value(&self, key: &str) -> Option<Json> {
            self.0
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
        }
    }

    fn ctx(key: &str, op: Op, operand: Json, all: bool) -> Context {
        Context {
            key: key.into(),
            op,
            operand,
            match_all: all,
        }
    }

    /// 연산자 6종 · 모르는 키 = 거짓 · 배열 값 = match_all(전부)/아니면(하나) · 빈 context = 참.
    #[test]
    fn evaluates_operators_and_match_all() {
        let src = Src(vec![
            ("selection_empty", Json::Bool(true)),
            ("num_selections", Json::Num(2.0)),
            ("preceding_text", Json::Str("SELECT * FROM".into())),
            (
                "sel_texts",
                Json::Arr(vec![Json::Str("a".into()), Json::Str("ab".into())]),
            ),
        ]);
        assert!(matches(&[], &src));
        assert!(matches(
            &[ctx("selection_empty", Op::Equal, Json::Bool(true), false)],
            &src
        ));
        assert!(!matches(
            &[ctx(
                "selection_empty",
                Op::NotEqual,
                Json::Bool(true),
                false
            )],
            &src
        ));
        assert!(matches(
            &[ctx("num_selections", Op::Equal, Json::Num(2.0), false)],
            &src
        ));
        assert!(matches(
            &[ctx(
                "num_selections",
                Op::Equal,
                Json::Str("2".into()),
                false
            )],
            &src
        ));
        assert!(matches(
            &[ctx(
                "preceding_text",
                Op::RegexContains,
                Json::Str("FROM$".into()),
                false
            )],
            &src
        ));
        assert!(!matches(
            &[ctx(
                "preceding_text",
                Op::RegexMatch,
                Json::Str("FROM".into()),
                false
            )],
            &src
        ));
        assert!(matches(
            &[ctx(
                "preceding_text",
                Op::RegexMatch,
                Json::Str("SELECT.*".into()),
                false
            )],
            &src
        ));
        assert!(matches(
            &[ctx(
                "preceding_text",
                Op::NotRegexContains,
                Json::Str("WHERE".into()),
                false
            )],
            &src
        ));
        assert!(
            !matches(
                &[ctx(
                    "preceding_text",
                    Op::RegexContains,
                    Json::Str("(".into()),
                    false
                )],
                &src
            ),
            "잘못된 정규식 = 거짓"
        );
        assert!(matches(
            &[ctx(
                "sel_texts",
                Op::RegexMatch,
                Json::Str("a.*".into()),
                true
            )],
            &src
        ));
        assert!(!matches(
            &[ctx("sel_texts", Op::Equal, Json::Str("a".into()), true)],
            &src
        ));
        assert!(matches(
            &[ctx("sel_texts", Op::Equal, Json::Str("a".into()), false)],
            &src
        ));
        assert!(!matches(
            &[ctx("no_such_key", Op::Equal, Json::Bool(true), false)],
            &src
        ));
        // 둘 다 참이어야.
        assert!(!matches(
            &[
                ctx("selection_empty", Op::Equal, Json::Bool(true), false),
                ctx("num_selections", Op::Equal, Json::Num(1.0), false)
            ],
            &src
        ));
    }

    #[test]
    fn user_files_only_existing() {
        let dir = std::env::temp_dir().join(format!("nsql-keymap-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(user_files(&dir).is_empty());
        let user = dir.join("Packages").join("User");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(user.join(os_file_name()), "[]").unwrap();
        let f = user_files(&dir);
        assert_eq!(f.len(), 1);
        assert!(f[0].ends_with(os_file_name()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
