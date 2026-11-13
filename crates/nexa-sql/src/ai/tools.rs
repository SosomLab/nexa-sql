//! ★ 앱 기능 도구 표(docs/110 §6 · 사용자 11-10 "command palette 기능을 전부 API로"): API 모드는 Messages API `tools`로 ·
//! CLI 모드는 MCP 서버 `nexa`의 도구로 **같은 표**를 내보낸다. 실행은 UI 스레드(`App::ai_run_tool`) 한 곳.
//!
//! 안전 경계(110 §7): `write` 도구는 사용자 확인 카드(Auto 모드면 자동 승인 · 단 `run_query`의 비조회 문장은 늘 확인) ·
//! 접속·비밀번호는 호스트가 들고 AI에는 이름만 · 행 데이터는 세션당 한 번 허용(`rows_allowed`).

use nsql_settings::json::Json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tool {
    pub name: &'static str,
    pub desc: &'static str,
    /// 쓰기(상태를 바꾼다) — 확인 카드 대상.
    pub write: bool,
    /// 입력 속성 (이름 · 종류 · 설명 · 필수).
    pub props: &'static [(&'static str, &'static str, &'static str, bool)],
}

pub(crate) const TOOLS: &[Tool] = &[
    Tool {
        name: "list_commands",
        desc: "List command palette commands (id and label). Use run_command to execute one.",
        write: false,
        props: &[("filter", "string", "Optional substring to filter by id or label", false)],
    },
    Tool {
        name: "run_command",
        desc: "Run a command palette command by id (e.g. view.word_wrap, edit.format, view.explorer). Commands that show/hide a window or switch an option are toggles: the app checks the current state first - with state=on (default) an already open window is only brought to the front, never closed.",
        write: true,
        props: &[
            ("id", "string", "Command id from list_commands", true),
            ("state", "string", "For toggle commands: on (default - make sure it is shown/enabled) | off | toggle", false),
        ],
    },
    Tool {
        name: "get_setting",
        desc: "Read a setting value by key (e.g. editor.word_wrap, grid.null_text). Empty key lists all keys.",
        write: false,
        props: &[("key", "string", "Setting key", false)],
    },
    Tool {
        name: "set_setting",
        desc: "Change a setting (persisted). Bool values are on/off.",
        write: true,
        props: &[
            ("key", "string", "Setting key", true),
            ("value", "string", "New value", true),
        ],
    },
    Tool {
        name: "get_editor_text",
        desc: "Read the active editor: caret position (line, column), full text, the selection, and the statement at the caret with its line range. Use this when the user says 'the query at the cursor' / 'this query'.",
        write: false,
        props: &[],
    },
    Tool {
        name: "set_editor_text",
        desc: "Replace the active editor text (undoable), or insert at the caret when mode=insert.",
        write: true,
        props: &[
            ("text", "string", "SQL text", true),
            ("mode", "string", "replace (default) | insert | new_tab", false),
        ],
    },
    Tool {
        name: "list_vars",
        desc: "List the app's bind variables of the active editor tab (:NAME) and substitution variables (&NAME): name, type, layer (tab/shared/global/profile), current value (secrets masked), formula and whether it is stale. Use this - not a query - when the user asks about variables.",
        write: false,
        props: &[],
    },
    Tool {
        name: "get_var",
        desc: "Show one variable: whether it is a plain value or a formula, the formula text, the variables the formula depends on with their current values, and the current value. Answer the user directly from this.",
        write: false,
        props: &[("name", "string", "Variable name (without ':'; prefix '&' for a substitution variable)", true)],
    },
    Tool {
        name: "set_var",
        desc: "Set a variable's current value directly in the app (same as editing it in the Variables window; nothing is sent to the database). `value` is the literal text; null=true sets NULL. Creates the variable when it does not exist. Give `formula` (not value) when the user wants an expression or a variable that depends on other variables. Use this when the user asks to set or change a variable - do not edit the script, do not run EXEC yourself and do not tell the user to do it by hand.",
        write: true,
        props: &[
            ("name", "string", "Variable name (without ':'; prefix '&' for a substitution variable)", true),
            ("value", "string", "New value as text (a literal)", false),
            ("formula", "string", "Instead of value: an expression such as ':VAR2 + 2' - the app runs EXEC :NAME := <formula>, so the variable becomes a formula that follows the variables it depends on", false),
            ("null", "boolean", "Set NULL instead of a value", false),
        ],
    },
    Tool {
        name: "delete_var",
        desc: "Delete a variable from the app (same as deleting its row in the Variables window; nothing is sent to the database). Removes it from the tab, shared and global layers; profile variables are read-only and stay. The result lists formula variables that referenced it. Use this when the user asks to delete or remove a variable - set_var null=true only clears the value and keeps the variable.",
        write: true,
        props: &[("name", "string", "Variable name (without ':'; prefix '&' for a substitution variable)", true)],
    },
    Tool {
        name: "edit_editor",
        desc: "Edit a part of the active editor (undoable). op = replace (put `text` in place of the range) | sort_asc | sort_desc | unique (drop duplicate lines, keep order) | reverse | upper | lower | trim (trailing spaces) | select (only select the range). Range = from_line..to_line (1-based, inclusive); without it the current selection's lines, else the statement at the caret. For other editing commands (comment, indent, duplicate/join/split/move lines, format) use list_commands filter=edit. and run_command.",
        write: true,
        props: &[
            ("op", "string", "replace | sort_asc | sort_desc | unique | reverse | upper | lower | trim | select", true),
            ("from_line", "integer", "First line (1-based)", false),
            ("to_line", "integer", "Last line (inclusive)", false),
            ("text", "string", "New text for op=replace", false),
        ],
    },
    Tool {
        name: "format_sql",
        desc: "Format SQL text with the app formatter and return the result (editor unchanged).",
        write: false,
        props: &[("sql", "string", "SQL text", true)],
    },
    Tool {
        name: "search_objects",
        desc: "Find objects in the metadata the app already loaded (no database query). Ranked: exact > prefix > word start > substring > fuzzy; current schema first. Query words are ANDed; `SCHEMA.NAME` narrows the schema. in_columns=true also matches loaded column names (find a table by a column). First line is a summary (hits, loaded objects, schemas, current schema).",
        write: false,
        props: &[
            ("query", "string", "Name words (case-insensitive) or SCHEMA.NAME", true),
            ("kind", "string", "Optional kind filter: table, view, procedure, function, ...", false),
            ("schema", "string", "Optional schema filter", false),
            ("in_columns", "boolean", "Also search loaded column names (default false)", false),
            ("limit", "integer", "Max results (default 50)", false),
        ],
    },
    Tool {
        name: "list_tabs",
        desc: "Editor tabs: id, title, file path, dirty/clean, active. Use before switch_tab or when the user refers to 'the other tab'.",
        write: false,
        props: &[],
    },
    Tool {
        name: "switch_tab",
        desc: "Make an editor tab active (by id from list_tabs or by exact title).",
        write: false,
        props: &[("tab", "string", "Tab id or title", true)],
    },
    Tool {
        name: "list_files",
        desc: "Find files under the current project/folder (bounded walk, hidden/target folders skipped). Words are ANDed against the relative path.",
        write: false,
        props: &[
            ("query", "string", "Path words (case-insensitive); empty = top entries", false),
            ("limit", "integer", "Max results (default 50)", false),
        ],
    },
    Tool {
        name: "open_file",
        desc: "Open a file in an editor tab (path relative to the project/folder root or absolute).",
        write: false,
        props: &[("path", "string", "File path", true)],
    },
    Tool {
        name: "list_extensions",
        desc: "Installed/built-in extensions with id, name, version, kind and enabled state.",
        write: false,
        props: &[],
    },
    Tool {
        name: "set_extension_enabled",
        desc: "Enable or disable an installed extension by id (persisted; asks the user).",
        write: true,
        props: &[
            ("id", "string", "Extension id (from list_extensions)", true),
            ("enabled", "boolean", "true = enable, false = disable", true),
        ],
    },
    Tool {
        name: "meta_stats",
        desc: "What the app has loaded per schema (object counts by kind, how many have columns loaded) and the current schema. Call before deciding whether a dictionary query is needed.",
        write: false,
        props: &[],
    },
    Tool {
        name: "describe_object",
        desc: "Describe a table/view: columns loaded in the explorer (name, type, nullable, key), status and extra info.",
        write: false,
        props: &[
            ("name", "string", "Object name", true),
            ("schema", "string", "Schema (optional, current schema when empty)", false),
        ],
    },
    Tool {
        name: "object_source",
        desc: "Open the DDL/source of an object in a tab and return its text (procedures, views, triggers ...).",
        write: false,
        props: &[
            ("name", "string", "Object name", true),
            ("schema", "string", "Schema (optional)", false),
        ],
    },
    Tool {
        name: "list_profiles",
        desc: "List saved connection profile names (never passwords).",
        write: false,
        props: &[],
    },
    Tool {
        name: "session_info",
        desc: "Current session: profile, dialect, connected, current schema, busy.",
        write: false,
        props: &[],
    },
    Tool {
        name: "connect",
        desc: "Connect with a saved profile. scope=shared (default): app-wide shared connection — connects directly when the password is stored, else opens the connection window for the user. scope=private: this editor tab's own session — runs `CONNECT <profile>;` in the active editor (same as typing it), needs a stored password.",
        write: true,
        props: &[
            ("profile", "string", "Profile name", true),
            (
                "scope",
                "string",
                "shared (default) or private (per-editor-tab session via CONNECT statement)",
                false,
            ),
        ],
    },
    Tool {
        name: "run_query",
        desc: "Run SQL in the current session and return up to max_rows rows as text to you only (the user's result grid is not touched; use it freely for exploration). SELECT runs after read-only check; any other statement asks the user first. To show a result to the user, end your answer with the final query in one ```sql block: the app runs the last read-only block into the result grid.",
        write: true,
        props: &[
            ("sql", "string", "SQL statement", true),
            ("purpose", "string", "One short sentence in the user's language: what this statement does and why it is needed now (shown to the user on the approval card)", true),
            ("max_rows", "integer", "Rows to return (default = ai.max_rows)", false),
        ],
    },
    Tool {
        name: "get_result",
        desc: "Return the active result grid (columns and up to max_rows rows) as text.",
        write: false,
        props: &[("max_rows", "integer", "Rows to return (default = ai.max_rows)", false)],
    },
    Tool {
        name: "get_output",
        desc: "Return the tail of the Output tab of the active editor (server messages, DBMS_OUTPUT/PRINT, statement errors with line numbers).",
        write: false,
        props: &[("max_lines", "integer", "Lines from the end (default 100)", false)],
    },
    Tool {
        name: "get_log",
        desc: "Return the tail of the app's execution log (time|kind|message: send, execute, fetch, done, ERROR with the server error text). Use errors_only to see just the errors. Call this when the user says a query failed.",
        write: false,
        props: &[
            ("max_lines", "integer", "Lines from the end (default 60)", false),
            ("errors_only", "boolean", "Only ERROR lines", false),
        ],
    },
    Tool {
        name: "explain",
        desc: "Show the execution plan of a statement (runs EXPLAIN in the current session) and return it as text.",
        write: true,
        props: &[("sql", "string", "SQL statement", true)],
    },
    // ★ 스킬 플레이북(docs/110 §15-12): 색인은 시스템 프롬프트 · 본문은 이 도구로.
    Tool {
        name: "skill",
        desc: "Read a Nexa SQL skill playbook (connections, metadata, editor, results, app) that says which app tools to use for a kind of request and when, if ever, to query the DBMS.",
        write: false,
        props: &[("name", "string", "skill name from the Skills index", true)],
    },
];

pub(crate) fn find(name: &str) -> Option<&'static Tool> {
    let short = super::bridge::tool_short_name(name);
    TOOLS.iter().find(|t| t.name == short)
}

impl Tool {
    /// JSON Schema(입력).
    pub(crate) fn schema(&self) -> Json {
        let props: Vec<(String, Json)> = self
            .props
            .iter()
            .map(|(n, ty, d, _)| {
                (
                    (*n).to_string(),
                    Json::Obj(vec![
                        ("type".into(), Json::Str((*ty).into())),
                        ("description".into(), Json::Str((*d).into())),
                    ]),
                )
            })
            .collect();
        let mut props = props;
        let mut required: Vec<Json> = self
            .props
            .iter()
            .filter(|(_, _, _, req)| *req)
            .map(|(n, _, _, _)| Json::Str((*n).into()))
            .collect();
        // ★ 쓰기 도구는 모두 `purpose`(승인 카드의 "목적" 줄 · 사용자 11-12 "승인 대상과 목표를 요약해서") — 표에 없으면 붙인다.
        if self.write && !self.props.iter().any(|(n, _, _, _)| *n == "purpose") {
            props.push((
                "purpose".to_string(),
                Json::Obj(vec![
                    ("type".into(), Json::Str("string".into())),
                    (
                        "description".into(),
                        Json::Str(
                            "One short sentence in the user's language: what this does and why it is needed now (shown to the user on the approval card)"
                                .into(),
                        ),
                    ),
                ]),
            ));
            required.push(Json::Str("purpose".into()));
        }
        Json::Obj(vec![
            ("type".into(), Json::Str("object".into())),
            ("properties".into(), Json::Obj(props)),
            ("required".into(), Json::Arr(required)),
        ])
    }

    /// Messages API `tools[]` 항목.
    pub(crate) fn api_json(&self) -> Json {
        Json::Obj(vec![
            ("name".into(), Json::Str(self.name.into())),
            ("description".into(), Json::Str(self.desc.into())),
            ("input_schema".into(), self.schema()),
        ])
    }

    /// MCP `tools/list` 항목.
    pub(crate) fn mcp_json(&self) -> Json {
        Json::Obj(vec![
            ("name".into(), Json::Str(self.name.into())),
            ("description".into(), Json::Str(self.desc.into())),
            ("inputSchema".into(), self.schema()),
        ])
    }
}

/// API `tools` 배열 전체.
pub(crate) fn api_tools() -> Json {
    Json::Arr(TOOLS.iter().map(Tool::api_json).collect())
}

/// MCP 도구 목록(앱 도구 + `approve` = CLI 권한 프롬프트 도구).
pub(crate) fn mcp_tools() -> Json {
    let mut v: Vec<Json> = TOOLS.iter().map(Tool::mcp_json).collect();
    v.push(Json::Obj(vec![
        ("name".into(), Json::Str("approve".into())),
        (
            "description".into(),
            Json::Str("Permission prompt: asks the Nexa SQL user to allow a tool call".into()),
        ),
        (
            "inputSchema".into(),
            Json::Obj(vec![
                ("type".into(), Json::Str("object".into())),
                (
                    "properties".into(),
                    Json::Obj(vec![
                        (
                            "tool_name".into(),
                            Json::Obj(vec![("type".into(), Json::Str("string".into()))]),
                        ),
                        (
                            "input".into(),
                            Json::Obj(vec![("type".into(), Json::Str("object".into()))]),
                        ),
                        (
                            "tool_use_id".into(),
                            Json::Obj(vec![("type".into(), Json::Str("string".into()))]),
                        ),
                    ]),
                ),
                (
                    "required".into(),
                    Json::Arr(vec![
                        Json::Str("tool_name".into()),
                        Json::Str("input".into()),
                    ]),
                ),
            ]),
        ),
    ]));
    Json::Arr(v)
}

/// 시스템 프롬프트(두 모드 공통 · 110 §6-2).
pub(crate) fn system_prompt(dialect: &str, profile: &str, agent: &str) -> String {
    format!(
        "You are the assistant inside Nexa SQL, a desktop SQL client. The user works in a SQL editor with a result grid. \
Current session: profile '{profile}', dialect {dialect}. Use the provided tools to read the editor, inspect database objects, \
run read-only queries, format SQL, manage tabs/files/extensions, change settings and run palette commands. Never ask for or echo passwords or API keys. \
Prefer SELECT statements; any statement that modifies data or schema must be explicit and will require the user's approval.\n\n\
Style (the user reads this in a narrow pane): answer first, no filler, do not restate the question; short sentences; prefer a short bullet list over prose; \
at most one blank line between paragraphs; SQL goes in one fenced ```sql block (the app formats it and offers Copy/Insert/Run buttons), \
followed by at most two short lines; no closing summaries or offers. \
Data goes in a markdown table by default (objects, columns, variables, settings, comparisons, findings with more than one attribute): \
a header row, at most 6 columns and about 20 rows, short cell text; the app renders the table with aligned columns. \
Use a bullet list only for items that have a single attribute.\n\n\
Skills (application-layer playbooks). Nexa SQL itself holds the connection state and cached metadata: when a request matches a skill, \
first call the `skill` tool with that name and follow it; prefer app tools (session_info, list_profiles, meta_stats, search_objects, describe_object, \
get_result, list_tabs, list_files, list_extensions) over querying the DBMS, and query the database only when the skill says so or the user asks for server-side data.\n{skills}{agent_text}",
        skills = super::skills::index_text(),
        agent_text = super::agents::prompt_text(agent)
    )
}

/// ★ 줄 단위 편집 연산(도구 `edit_editor` · 순수 · 사용자 11-12 "정렬, Permute 등 편집 기능"): 줄 묶음 → 바뀐 줄 묶음.
/// 모르는 연산이면 `None`.
pub(crate) fn lines_op(lines: &[&str], op: &str) -> Option<Vec<String>> {
    let own = || lines.iter().map(|l| (*l).to_string()).collect::<Vec<_>>();
    Some(match op {
        "sort_asc" => {
            let mut v = own();
            v.sort_by_key(|l| l.to_lowercase());
            v
        }
        "sort_desc" => {
            let mut v = own();
            v.sort_by_key(|l| std::cmp::Reverse(l.to_lowercase()));
            v
        }
        "unique" => {
            let mut seen = std::collections::HashSet::new();
            own()
                .into_iter()
                .filter(|l| seen.insert(l.clone()))
                .collect()
        }
        "reverse" => {
            let mut v = own();
            v.reverse();
            v
        }
        "upper" => lines.iter().map(|l| l.to_uppercase()).collect(),
        "lower" => lines.iter().map(|l| l.to_lowercase()).collect(),
        "trim" => lines.iter().map(|l| l.trim_end().to_string()).collect(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn lines_op_sort_unique_reverse_case_trim() {
        let l = ["b", "A", "c", "b"];
        assert_eq!(
            super::lines_op(&l, "sort_asc").expect("op"),
            ["A", "b", "b", "c"]
        );
        assert_eq!(
            super::lines_op(&l, "sort_desc").expect("op"),
            ["c", "b", "b", "A"]
        );
        assert_eq!(super::lines_op(&l, "unique").expect("op"), ["b", "A", "c"]);
        assert_eq!(
            super::lines_op(&l, "reverse").expect("op"),
            ["b", "c", "A", "b"]
        );
        assert_eq!(super::lines_op(&["x  "], "trim").expect("op"), ["x"]);
        assert_eq!(super::lines_op(&["aB"], "upper").expect("op"), ["AB"]);
        assert!(super::lines_op(&l, "nope").is_none());
    }

    use super::*;

    #[test]
    fn schema_and_lookup() {
        let t = find("mcp__nexa-sql__run_query").expect("tool");
        assert!(t.write);
        let s = nsql_settings::json::dump(&t.schema());
        assert!(s.contains("\"required\":[\"sql\",\"purpose\"]"), "{s}");
        assert!(find("nope").is_none());
        let m = nsql_settings::json::dump(&mcp_tools());
        assert!(m.contains("\"approve\"") && m.contains("inputSchema"));
        let a = nsql_settings::json::dump(&api_tools());
        assert!(a.contains("input_schema") && !a.contains("approve"));
        assert!(TOOLS.iter().filter(|t| !t.write).count() >= 8);
    }
}
