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
        name: "editor_carets",
        desc: "Column mode / multiple carets in the active editor tab (changes only the selection, not the text): puts one caret on every line from from_line to to_line - mode=line_end (default), line_start, column (at `column`; shorter lines clamp to their end) or select_lines (each line selected). mode=clear returns to a single caret. Follow with insert_sequence, or with edit commands that act on every caret.",
        write: false,
        props: &[
            ("from_line", "integer", "First line (1-based)", false),
            ("to_line", "integer", "Last line (default = from_line)", false),
            ("mode", "string", "line_end (default) | line_start | column | select_lines | clear", false),
            ("column", "integer", "1-based column for mode=column", false),
        ],
    },
    Tool {
        name: "insert_sequence",
        desc: "Insert a number/text sequence with the Insert Sequence extension - one value per caret (or replacing each selection), one undo step. Give from_line/to_line (+ at = line_end | line_start | column) to place the carets and insert in one call, or omit them to use the current carets. prefix / suffix are added around each value (also together with expr). Values: either `expr` in the extension's syntax (start[+step][~format] e.g. `1+1~03` = 001, 002 ...; `10+10`; `a` for letters; a quoted template like `1+1~03 \"-{}\"`), or start / step / width (zero-padded digits) / prefix / suffix.",
        write: true,
        props: &[
            ("expr", "string", "Sequence expression, e.g. 1+1~03", false),
            ("start", "integer", "First value (default 1) when expr is empty", false),
            ("step", "integer", "Increment (default 1)", false),
            ("width", "integer", "Zero-padded width, e.g. 3 = 001", false),
            ("prefix", "string", "Text before each value", false),
            ("suffix", "string", "Text after each value", false),
            ("from_line", "integer", "First line to put carets on (optional)", false),
            ("to_line", "integer", "Last line", false),
            ("at", "string", "line_end (default) | line_start | column", false),
            ("column", "integer", "1-based column when at=column", false),
        ],
    },
    Tool {
        name: "set_syntax",
        desc: "Set the syntax (language highlighting, comment marks, indentation) of the active editor tab, e.g. SQL, JSON, Plain Text or any installed syntax package. The name is matched case-insensitively; if it is not installed the answer lists the available names.",
        write: false,
        props: &[("name", "string", "Syntax name", true)],
    },
    Tool {
        name: "edit_editor",
        desc: "Edit a part of the active editor (undoable). op = replace (put `text` in place of the range) | sort_asc | sort_desc | unique (drop duplicate lines, keep order) | reverse | upper | lower | trim (trailing spaces) | select (only select the range). Range = from_line..to_line (1-based, inclusive); without it the current selection's lines, else the statement at the caret. Sorting, case conversion and permute (reverse / shuffle / unique) must use these built-in ops first; use op=replace with text you computed from the whole range only when no built-in op can express the request. For other editing commands (comment, indent, duplicate/join/split/move lines, format) use list_commands filter=edit. and run_command.",
        write: true,
        props: &[
            ("op", "string", "replace | sort_asc | sort_desc | sort_case (case-sensitive) | sort_num_asc | sort_num_desc | shuffle | unique (= dedupe, removes duplicate lines) | reverse | upper | lower | title (Capitalize Each Word) | swapcase | snake | kebab | upper_camel | lower_camel | trim | select", true),
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
        desc: "Find objects in the metadata the app already loaded (no database query). Matches object names AND table/column comments (descriptions, often Korean - search the user's own words first, e.g. 자재, before English guesses like MATERIAL). Ranked: exact > prefix > word start > substring > fuzzy; current schema first. Query words are ANDed; `SCHEMA.NAME` narrows the schema. in_columns=true also matches loaded column names (find a table by a column). First line is a summary (hits, loaded objects, schemas, current schema, schemas whose comments are loaded). Use this before any dictionary query (ALL_TAB_COMMENTS, ALL_COL_COMMENTS, ALL_TABLES, information_schema, sys.*).",
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
        name: "bookmarks",
        desc: "List line bookmarks (action=list, default) or jump to one (action=goto with id, or mnemonic = its 0-9 number). List columns: id, document, line, text/label, note, number, state. filter = words in the label, document name or note; scope=current limits to the active editor tab.",
        write: false,
        props: &[
            ("action", "string", "list (default) | goto", false),
            ("filter", "string", "Substring filter for list", false),
            ("scope", "string", "all (default) | current", false),
            ("id", "integer", "Bookmark id for goto", false),
            ("mnemonic", "integer", "0-9 number for goto instead of id", false),
        ],
    },
    Tool {
        name: "edit_bookmark",
        desc: "Change line bookmarks: action=add (at `line` of the active editor tab, default = caret line; optional label / note / number), rename (id + label), note (id + note), mnemonic (id + number 0-9 sets or changes the quick-jump number - a number already used by another bookmark moves to this one; -1 removes it), delete (id). Get ids from `bookmarks`. To bookmark a line of another file: open_file (with line) first.",
        write: true,
        props: &[
            ("action", "string", "add | rename | note | mnemonic | delete", true),
            ("id", "integer", "Bookmark id (all actions except add)", false),
            ("line", "integer", "1-based line for add (default: caret line)", false),
            ("label", "string", "Name for add / rename", false),
            ("note", "string", "Note text for add / note", false),
            ("number", "integer", "0-9 for mnemonic, -1 clears it", false),
        ],
    },
    Tool {
        name: "search_files",
        desc: "Search file CONTENTS (the app's Find in Files): sets the query and options, runs the search and returns `path:line:column<TAB>line text` rows (first line = summary). `where` uses the panel syntax: comma-separated folders/files, `*.sql` or `+pattern` to include, `-pattern` to exclude, `<open files>`, `<current file>`; empty = open tabs + the project/folder root. Then use open_file with line to go to a hit. (list_files only matches path names.)",
        write: false,
        props: &[
            ("query", "string", "Text or regular expression to find", true),
            ("case", "boolean", "Match case (default false)", false),
            ("word", "boolean", "Whole word (default false)", false),
            ("regex", "boolean", "Query is a regular expression (default false)", false),
            ("where", "string", "Scope / include / exclude patterns (panel syntax)", false),
            ("where_case", "boolean", "Case-sensitive path patterns (default false)", false),
            ("limit", "integer", "Max result rows (default 100)", false),
        ],
    },
    Tool {
        name: "reveal_object",
        desc: "Go to an object in the object explorer: shows the explorer panel for this tab's connection, expands schema > folder > object (loading branches as needed) and selects it - the same as 'Reveal in Object Explorer' on the editor hover card. `member` selects a column / package member under it. Find the exact name with search_objects first. Returns the selected path.",
        write: false,
        props: &[
            ("name", "string", "Object name", true),
            ("schema", "string", "Schema (optional, current schema when empty)", false),
            ("member", "string", "Column or member name to select under the object", false),
        ],
    },
    Tool {
        name: "filter_project",
        desc: "Show the Project explorer panel and type into its 'Filter files' box so the tree lists only matching files (the same as the user typing there). query = text to filter by (empty clears the filter); case / word / regex / path (match the whole relative path instead of the file name) are the box's toggles. Use this when the user says 'in the project explorer, search / filter ...'. To get the matching names as text use list_files; for file contents use search_files.",
        write: false,
        props: &[
            ("query", "string", "Filter text (empty = clear)", true),
            ("case", "boolean", "Match case (default false)", false),
            ("word", "boolean", "Whole word (default false)", false),
            ("regex", "boolean", "Regular expression (default false)", false),
            ("path", "boolean", "Match against the relative path (default false = file name)", false),
        ],
    },
    Tool {
        name: "open_file",
        desc: "Open a file in an editor tab (path relative to the project/folder root or absolute). With line (and column) the caret goes there - use it to open a search_files hit.",
        write: false,
        props: &[
            ("path", "string", "File path", true),
            ("line", "integer", "1-based line to go to (optional)", false),
            ("column", "integer", "1-based column (optional)", false),
        ],
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
        desc: "Describe a table/view from the app's cached metadata: table comment, columns (name, type, nullable, key and the column comment after `--`), status and extra info. Columns that are not loaded yet are loaded on demand for that one object (no `WHERE 1=0` probe needed). Use this instead of ALL_TAB_COLUMNS / ALL_COL_COMMENTS / information_schema.columns queries.",
        write: false,
        props: &[
            ("name", "string", "Object name", true),
            ("schema", "string", "Schema (optional, current schema when empty)", false),
        ],
    },
    Tool {
        name: "object_source",
        desc: "Read the DDL/source of an object and return its text (procedures, functions, packages, views, triggers ...). By default NOTHING is opened in the editor - use this freely to analyse, verify or explain code. Set open=true ONLY when the user explicitly asked to open / show the object in an editor tab (or the task is to edit it there); never open tabs just to read. For a package the specification is returned; pass part=body for the package body.",
        write: false,
        props: &[
            ("name", "string", "Object name", true),
            ("schema", "string", "Schema (optional)", false),
            ("open", "boolean", "Open the source in an editor tab as well (default false; only on the user's explicit request)", false),
            ("part", "string", "Package only: spec (default) or body", false),
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
        name: "disconnect",
        desc: "Close database connections: scope=current (default - the connection of this tab) or all (every open connection, shared and private). A connection that is busy or has uncommitted changes is skipped and reported (the user must commit or roll back first). Tabs that used a closed connection become unconnected. Use this - not run_command conn.disconnect / conn.toggle and never a DISCONNECT statement via run_query.",
        write: true,
        props: &[("scope", "string", "current (default) | all", false)],
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
first call the `skill` tool with that name and follow it; table and column comments (descriptions) are in the cached metadata too - find them with search_objects / describe_object before any data-dictionary SQL; prefer app tools (session_info, list_profiles, meta_stats, search_objects, describe_object, \
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
        // ★ 11-14(사용자 "Sort Lines (Case Sensitive)" · "Permute Lines: Shuffle"): 대소문자 구분 정렬(코드 순) · 무작위 섞기.
        // 표기 변환(줄마다 · `convert_case`): snake_case · kebab-case · UpperCamelCase · lowerCamelCase.
        "snake" | "kebab" | "upper_camel" | "lower_camel" => lines
            .iter()
            .map(|l| convert_case(l, op).unwrap_or_else(|| (*l).to_string()))
            .collect(),
        "sort_case" => {
            let mut v = own();
            v.sort();
            v
        }
        "shuffle" => {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E37_79B9_7F4A_7C15);
            shuffled(own(), seed)
        }
        "unique" | "dedupe" => {
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
        // ★ 11-14(사용자 "대소문자 변환 · Capital 방식 등"): 낱말 첫 글자만 대문자 · 대소문자 뒤집기 · 숫자 정렬.
        "title" => lines.iter().map(|l| title_case(l)).collect(),
        "swapcase" => lines
            .iter()
            .map(|l| {
                l.chars()
                    .map(|c| {
                        if c.is_uppercase() {
                            c.to_lowercase().collect::<String>()
                        } else {
                            c.to_uppercase().collect::<String>()
                        }
                    })
                    .collect()
            })
            .collect(),
        "sort_num_asc" | "sort_num_desc" => {
            let mut v = own();
            v.sort_by(|a, b| {
                let (ka, kb) = (first_number(a), first_number(b));
                ka.partial_cmp(&kb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.to_lowercase().cmp(&b.to_lowercase()))
            });
            if op == "sort_num_desc" {
                v.reverse();
            }
            v
        }
        "trim" => lines.iter().map(|l| l.trim_end().to_string()).collect(),
        _ => return None,
    })
}

/// ★ 대소문자·표기 변환(사용자 11-14 "Convert Case" — 팔레트 8종 · AI `edit_editor`의 op): `kind` =
/// upper | lower | swapcase | title | snake | kebab | upper_camel | lower_camel. 앞의 넷은 글자마다 · 뒤의 넷은 글을 낱말로
/// 나눠(글자·숫자가 아닌 것 · 소문자→대문자 경계 · 약어 끝 `HTTPServer` = HTTP + Server) 그 표기로 잇는다(앞뒤 공백은 그대로).
/// 모르는 kind = None.
pub(crate) fn convert_case(text: &str, kind: &str) -> Option<String> {
    Some(match kind {
        "upper" => text.to_uppercase(),
        "lower" => text.to_lowercase(),
        "swapcase" => text
            .chars()
            .map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().collect::<String>()
                } else {
                    c.to_uppercase().collect::<String>()
                }
            })
            .collect(),
        "title" => title_case(text),
        "snake" | "kebab" | "upper_camel" | "lower_camel" => {
            let lead: String = text.chars().take_while(|c| c.is_whitespace()).collect();
            let core = text.trim();
            let tail = &text[lead.len() + core.len()..];
            let words = split_words(core);
            let cap = |w: &str| {
                let mut cs = w.chars();
                match cs.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + &cs.as_str().to_lowercase(),
                    None => String::new(),
                }
            };
            let joined = match kind {
                "snake" => words
                    .iter()
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
                    .join("_"),
                "kebab" => words
                    .iter()
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
                    .join("-"),
                "upper_camel" => words.iter().map(|w| cap(w)).collect(),
                _ => words
                    .iter()
                    .enumerate()
                    .map(|(i, w)| if i == 0 { w.to_lowercase() } else { cap(w) })
                    .collect(),
            };
            format!("{lead}{joined}{tail}")
        }
        _ => return None,
    })
}

/// 글을 낱말로: 글자·숫자가 아닌 것은 구분자 · 소문자/숫자 뒤의 대문자 = 새 낱말 · 대문자 연속 뒤 소문자가 오면 마지막 대문자부터 새 낱말.
fn split_words(s: &str) -> Vec<String> {
    let ch: Vec<char> = s.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for i in 0..ch.len() {
        let c = ch[i];
        if !c.is_alphanumeric() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let prev = i.checked_sub(1).map(|j| ch[j]);
        let next = ch.get(i + 1).copied();
        let boundary = prev.is_some_and(|p| {
            p.is_alphanumeric()
                && c.is_uppercase()
                && (p.is_lowercase()
                    || p.is_numeric()
                    || (p.is_uppercase() && next.is_some_and(char::is_lowercase)))
        });
        if boundary && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// 줄 섞기(Fisher-Yates · xorshift64 — 외부 crate 없이 · 같은 씨앗이면 같은 순서).
pub(crate) fn shuffled(mut v: Vec<String>, seed: u64) -> Vec<String> {
    let mut x = seed | 1;
    for i in (1..v.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.swap(i, (x % (i as u64 + 1)) as usize);
    }
    v
}

/// 낱말 첫 글자만 대문자(나머지 소문자) — 낱말 경계 = 글자·숫자가 아닌 것(`_` · 공백 · 구두점).
fn title_case(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut start = true;
    for c in line.chars() {
        if c.is_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = true;
        }
    }
    out
}

/// 줄의 첫 숫자(부호·소수 포함) — 없으면 맨 뒤로 가게 무한대.
fn first_number(line: &str) -> f64 {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() && !b[i].is_ascii_digit() {
        i += 1;
    }
    if i >= b.len() {
        return f64::INFINITY;
    }
    let neg = i > 0 && b[i - 1] == b'-';
    let s = i;
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
        i += 1;
    }
    let n: f64 = line[s..i]
        .trim_end_matches('.')
        .parse()
        .unwrap_or(f64::INFINITY);
    if neg {
        -n
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn convert_case_kinds() {
        let c = |t: &str, k: &str| super::convert_case(t, k).expect("kind");
        assert_eq!(c("Project Name-code", "snake"), "project_name_code");
        assert_eq!(c("projectNameCode", "kebab"), "project-name-code");
        assert_eq!(c("project_name_code", "upper_camel"), "ProjectNameCode");
        assert_eq!(c("PROJECT_NAME_CODE", "lower_camel"), "projectNameCode");
        assert_eq!(c("HTTPServer2Go", "snake"), "http_server2_go");
        assert_eq!(
            c("  item cd  ", "upper_camel"),
            "  ItemCd  ",
            "앞뒤 공백은 그대로"
        );
        assert_eq!(c("aBc", "swapcase"), "AbC");
        assert_eq!(c("hello WORLD", "title"), "Hello World");
        assert_eq!(c("Abc", "upper"), "ABC");
        assert_eq!(c("Abc", "lower"), "abc");
        assert!(super::convert_case("x", "nope").is_none());
        assert_eq!(
            super::lines_op(&["item cd", "use yn"], "snake").expect("op"),
            ["item_cd", "use_yn"]
        );
    }

    #[test]
    fn lines_op_case_sensitive_sort_and_shuffle() {
        let l = ["b", "A", "a", "B"];
        assert_eq!(
            super::lines_op(&l, "sort_case").expect("op"),
            ["A", "B", "a", "b"]
        );
        // 섞기 = 같은 줄들 · 같은 씨앗이면 같은 순서.
        let v: Vec<String> = (0..20).map(|i| i.to_string()).collect();
        let a = super::shuffled(v.clone(), 7);
        assert_eq!(a, super::shuffled(v.clone(), 7));
        assert_ne!(a, v, "20줄이 그대로일 확률은 없다시피");
        let mut sorted = a;
        sorted.sort_by_key(|s| s.parse::<u32>().unwrap_or(0));
        assert_eq!(sorted, v);
        assert_eq!(super::lines_op(&l, "shuffle").expect("op").len(), 4);
    }

    #[test]
    fn lines_op_title_swapcase_and_numeric_sort() {
        let l = ["item_cd  SOME text", "hello-world 2nd"];
        assert_eq!(
            super::lines_op(&l, "title").expect("op"),
            ["Item_Cd  Some Text", "Hello-World 2nd"]
        );
        assert_eq!(
            super::lines_op(&["aBc 1"], "swapcase").expect("op"),
            ["AbC 1"]
        );
        let n = ["row 10", "row 2", "x", "row -1", "row 2.5"];
        assert_eq!(
            super::lines_op(&n, "sort_num_asc").expect("op"),
            ["row -1", "row 2", "row 2.5", "row 10", "x"]
        );
        assert_eq!(
            super::lines_op(&n, "sort_num_desc")
                .expect("op")
                .first()
                .map(String::as_str),
            Some("x")
        );
        assert_eq!(
            super::lines_op(&["a", "b", "a"], "dedupe").expect("op"),
            ["a", "b"]
        );
    }

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
