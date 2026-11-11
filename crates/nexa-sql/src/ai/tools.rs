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
        desc: "Run a command palette command by id (e.g. view.word_wrap, edit.format, view.explorer).",
        write: true,
        props: &[("id", "string", "Command id from list_commands", true)],
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
        desc: "Read the active editor: full text, the selection, and the statement at the caret.",
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
        name: "format_sql",
        desc: "Format SQL text with the app formatter and return the result (editor unchanged).",
        write: false,
        props: &[("sql", "string", "SQL text", true)],
    },
    Tool {
        name: "search_objects",
        desc: "Search database objects known to the object explorer (tables, views, procedures ...) by name substring.",
        write: false,
        props: &[
            ("query", "string", "Name substring (case-insensitive)", true),
            ("kind", "string", "Optional kind filter: table, view, procedure, function, ...", false),
            ("limit", "integer", "Max results (default 50)", false),
        ],
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
        desc: "Connect with a saved profile: if its password is stored, connects directly; otherwise opens the connection window where the user must type the password and confirm.",
        write: true,
        props: &[("profile", "string", "Profile name", true)],
    },
    Tool {
        name: "run_query",
        desc: "Run SQL in the current session and return up to max_rows rows as text. SELECT runs after read-only check; any other statement asks the user first.",
        write: true,
        props: &[
            ("sql", "string", "SQL statement", true),
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
    let short = name.strip_prefix("mcp__nexa__").unwrap_or(name);
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
        let required: Vec<Json> = self
            .props
            .iter()
            .filter(|(_, _, _, req)| *req)
            .map(|(n, _, _, _)| Json::Str((*n).into()))
            .collect();
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
pub(crate) fn system_prompt(dialect: &str, profile: &str) -> String {
    format!(
        "You are the assistant inside Nexa SQL, a desktop SQL client. The user works in a SQL editor with a result grid. \
Current session: profile '{profile}', dialect {dialect}. Use the provided tools to read the editor, inspect database objects, \
run read-only queries, format SQL, change settings and run palette commands. Never ask for or echo passwords or API keys. \
Prefer SELECT statements; any statement that modifies data or schema must be explicit and will require the user's approval. \
Answer concisely; put SQL in fenced code blocks.\n\n\
Skills (application-layer playbooks). Nexa SQL itself holds the connection state and cached metadata: when a request matches a skill, \
first call the `skill` tool with that name and follow it; prefer app tools (session_info, list_profiles, search_objects, describe_object, \
get_result) over querying the DBMS, and query the database only when the skill says so or the user asks for server-side data.\n{skills}",
        skills = super::skills::index_text()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_and_lookup() {
        let t = find("mcp__nexa__run_query").expect("tool");
        assert!(t.write);
        let s = nsql_settings::json::dump(&t.schema());
        assert!(s.contains("\"required\":[\"sql\"]"), "{s}");
        assert!(find("nope").is_none());
        let m = nsql_settings::json::dump(&mcp_tools());
        assert!(m.contains("\"approve\"") && m.contains("inputSchema"));
        let a = nsql_settings::json::dump(&api_tools());
        assert!(a.contains("input_schema") && !a.contains("approve"));
        assert!(TOOLS.iter().filter(|t| !t.write).count() >= 8);
    }
}
