//! ★ 스킬(docs/110 §15-12 · 사용자 11-11 "MCP로 줄 정보·기능은 skills로 따로 정리"): 요청 종류별 **플레이북** — 어떤 앱 도구를
//! 어떤 순서로 쓰고, 언제만 DBMS에 질의하는지. 시스템 프롬프트에는 **색인**(이름 · 언제)만 들어가고 본문은 도구 `skill(name)`으로
//! 모델이 필요할 때 받는다(API · CLI · Codex 공통 · 프롬프트는 짧게). 글은 모델에게 가는 영어(사용자 문자열 아님).

pub(crate) struct Skill {
    pub name: &'static str,
    /// 색인 한 줄(언제 쓰나).
    pub when: &'static str,
    pub body: &'static str,
}

pub(crate) const SKILLS: &[Skill] = &[
    Skill {
        name: "connections",
        when: "the user asks about the current connection, session, server, schema, user, shared/private sessions, transactions, or saved profiles",
        body: "\
# Skill: connections (Nexa SQL application layer)

To close connections use the `disconnect` tool (scope=current or all); it reports connections it skipped (busy / uncommitted changes). Do not use `run_command conn.disconnect` or `conn.toggle`, and never send a `DISCONNECT` statement through `run_query`.

Nexa SQL (not the DBMS) owns the connection state. Answer from the app first.

Terms:
- Shared connection: one connection per server/account that any editor tab can use (the app's default). There can be several (different servers), one is the active default.
- Private session: a connection owned by a single editor tab (created by a CONNECT statement in that editor or per-editor mode).
- Profile: a saved login (host, port, user, dialect). Passwords live in the vault and are never shown.

Steps:
1. Call `session_info`. It returns the current editor's session (profile, dialect, connected, schema/current db, busy, pending uncommitted statements, masked description) and the list of ALL app sessions (shared/private, owner tab, connected/broken/busy, idle seconds, pending count).
2. Answer directly from that output. Do NOT run SQL against v$session, pg_stat_activity, sys.dm_exec_sessions or similar unless the user explicitly asks about server-side sessions, other users, locks or waits.
3. Saved profiles: `list_profiles`. To open a connection: `connect` with the profile name. scope=shared (default) = app-wide shared connection: with a stored password it connects directly (then re-check with `session_info`); without one the connection window opens and you must tell the user to type the password and confirm the login there themselves. scope=private = the active editor tab's own session: the app runs `CONNECT <profile>;` in that tab (exactly what the user would type); it needs a stored password. You CAN create private sessions this way — do not say you cannot.
4. 'Which schema/database am I in' = schema / current db from `session_info`; do not guess from SQL text.
",
    },
    Skill {
        name: "text-edit",
        when: "the user asks to transform editor text: sort lines, remove duplicate lines, change case (upper / lower / Capital), column mode or multiple carets, insert numbering / a sequence, word wrap, or change the syntax (language) of the tab",
        body: "\
# Skill: text-edit — editor transformations

0. BUILT-IN FIRST: for sorting, case conversion, permute (reverse / shuffle / unique) and trimming always use the built-in operations below (`edit_editor` op=..., or the palette commands in 3a / 3b). They run inside the app: exact, instant, one undo step, and the text never leaves the editor. Do NOT read the text and retype it for something a built-in op does. Only when no built-in op can express the request (custom order such as 'by the 2nd comma field' or 'by month name', conditional case rules, a mixed rule, a transformation the op list lacks) fall back: read the WHOLE affected range with `get_editor_text`, compute the new text yourself, and write it back in ONE `edit_editor` op=replace over that same range - and say in your answer that it was done by rewriting rather than a built-in command. If a built-in op covers part of the job, do that part with the op and only the remainder by rewriting.
1. Line transforms: `edit_editor` with op = sort_asc | sort_desc | sort_case (case-sensitive) | sort_num_asc | sort_num_desc | shuffle | unique (remove duplicate lines) | reverse | upper | lower | title (Capitalize Each Word) | swapcase | snake | kebab | upper_camel | lower_camel | trim, on from_line..to_line (omit both = the selection or the statement at the caret; whole file = 1..total_lines from `get_editor_text`).
2. Column mode / multiple carets: `editor_carets` (from_line, to_line, mode = line_end | line_start | column | select_lines; clear = one caret). It only moves carets.
3. Numbering: `insert_sequence` uses the Insert Sequence extension - one value per caret. Do 'lines 15-30, append a 3-digit number' in ONE call: from_line=15, to_line=30, at=line_end, start=1, width=3 (or expr `1+1~03`). prefix / suffix add text around each value. One undo step.
3a. The same as palette commands (act on the selected lines, or the whole file without a selection): `run_command` `edit.sort_lines`, `edit.sort_lines_case`, `edit.permute_reverse`, `edit.permute_shuffle`, `edit.permute_unique`.\n3b. Convert Case on the selection (or the word at each caret): `run_command` `edit.case_upper`, `edit.case_lower`, `edit.case_title`, `edit.case_swap`, `edit.case_snake`, `edit.case_kebab`, `edit.case_upper_camel`, `edit.case_lower_camel` - select the text first (`edit_editor` op=select or `editor_carets`).\n4. Word wrap: `run_command` id `view.word_wrap` with state on/off (applies to all tabs).
5. Syntax (language) of the tab: `set_syntax` with the name (SQL, JSON, Plain Text, or an installed package). If the language is not installed the tool lists what is available - say so instead of guessing.
6. Other edit commands (`run_command` `edit.duplicate_line`, `edit.join_lines`, `edit.toggle_comment`, `edit.indent`, `edit.upper_case` ...) act on the current carets / selection - set them first with `edit_editor` op=select or `editor_carets`.
7. `get_editor_text` shows the caret list (`<carets>`), the selection and line numbers - check it before and after.
",
    },
    Skill {
        name: "bookmarks",
        when: "the user asks to add, list, rename, annotate, number, delete or jump to bookmarks (line marks in editor tabs)",
        body: "\
# Skill: bookmarks — line bookmarks in editor tabs

1. `bookmarks` (action=list) shows every bookmark: id, document, line, text/label, note, number (0-9 quick jump), state. Use `filter` / scope=current to narrow.
2. `bookmarks` action=goto with an id (or `mnemonic` = its number) opens the document and moves the caret there.
3. `edit_bookmark` changes them (asks the user unless Auto is on): add (line of the active editor tab, default the caret line; give a label), rename (id + label), note (id + note), mnemonic (id + number 0-9, -1 clears), delete (id).
4. To bookmark a line in another file: `open_file` with that line, then `edit_bookmark` add. Bookmarks need an editor tab - not the chat tab.
5. Never use `run_command bookmark.toggle` to add one: it removes the bookmark when the line already has one.
",
    },
    Skill {
        name: "metadata",
        when: "the user asks about tables, views, columns, keys, procedures, functions, triggers or DDL of a connected database",
        body: "\
# Skill: metadata — how to use Nexa SQL's loaded metadata (object explorer cache)

The object explorer caches metadata per connection. Prefer it over dictionary queries.

Search harness (all in-memory, instant, no database round trip):
1. `meta_stats` — how much the app has loaded per schema and the current schema. If the schema you need shows 0 objects, metadata is not loaded yet: say so and fall back to step 5.
2. `search_objects` — ranked (exact > prefix > word start > substring > fuzzy; current schema first). **Start with the user's own words** (Korean included, e.g. `워크센터`): table names are often codes (`DMS_I301090`) and the meaning lives in the comment, which is matched too (`comment=` column, e.g. `기준정보 - 설비(워크센터)관리`). Try English synonyms only after that. Use several words for AND (`cust order`), `SCHEMA.NAME` to pin a schema, kind=table|view|procedure|... to narrow, in_columns=true to find tables by a column name (results `schema.table.column`). Read the first summary line; `cols=loaded(N)` means `describe_object` will have columns.
3. `describe_object` — table comment, columns (name, type, nullable, PK and the column comment after `--`), status. Columns that were not loaded yet are loaded by the tool itself for that one object (it may take a moment) - never run `SELECT ... WHERE 1=0` probes to get a column list.\n5. Only when the explorer has no information for that object (not loaded, unknown schema), run a read-only dictionary query with `run_query` and say that it came from the database. Examples: Oracle ALL_TAB_COLUMNS / ALL_CONSTRAINTS, PostgreSQL information_schema.columns, SQL Server sys.columns, MySQL information_schema.COLUMNS.
6. Scope names with the current schema from `session_info` when the user does not qualify them. Never modify objects from this skill. Same steps for Claude and Codex (the tools are identical).
",
    },
    Skill {
        name: "editor",
        when: "the user refers to the SQL in the editor: review, fix, format, explain, rewrite, or asks to put SQL into the editor",
        body: "\
# Skill: editor

1. `get_editor_text` returns the caret position (line, column), the whole text, the selection and the statement at the caret with its line range. Work on the selection when there is one, else the statement at the caret, else the whole text.
2. `format_sql` formats with the app formatter (editor unchanged); `set_editor_text` replaces the text (undoable) or inserts at the caret with mode=insert. Replacing is a write: tell the user what changes.
3. `explain` shows the execution plan of one statement in the current session.
3b. Variables - how Nexa SQL works: bind variables (:NAME) and substitution variables (&NAME) are kept by the app per editor tab (layers: tab > shared > global > profile; the nearest layer wins), not by the database. Scripts create them with `VAR name type` and `EXEC :name := value`, but a script line only takes effect when the user runs it - editing the line changes nothing by itself. A variable is either a plain value or a formula (`EXEC :B := :A + 2`); formulas are only remembered when the setting `vars.expand_at` is `use` (default `assign` = the expression is evaluated once and stored as a value): a formula is re-evaluated when the variables it depends on change and is marked stale until then. Interpret requests like this: 'what is X / show X' = `get_var`; 'set / change X to 5' = `set_var` value; 'make X = Y + 2 / X should follow Y / add X as a formula' = `set_var` formula `:Y + 2` (never tell the user to type it into the Variables window); 'delete / remove X' = `delete_var` (do it yourself - never tell the user to delete it in the Variables window; NULL is not deletion); 'list variables' = `list_vars`; 'open the variables window' = `run_command` view.variables (state on). Only change the script text when the user explicitly asks to change the script. After set_var, report the new current value from the tool result. 'Show the value of X' = call `get_var` and answer directly with: value or formula, the formula text and the variables it depends on (with their values), and the current value - do not write a SELECT for it. 'Change X to …' = `set_var` with value (the current value changes at once); 'make X a formula / X = Y + 2' = `set_var` with formula (e.g. `:Y + 2`); only edit the script line when the user asks to change the script. `list_vars` lists everything.
3a. `edit_editor` changes only a part: op=replace with from_line/to_line and text, or line operations sort_asc / sort_desc / unique / reverse / upper / lower / trim, or op=select to highlight lines for the user. Prefer it over replacing the whole text. Other editing commands (toggle comment, indent, duplicate/join/split/move lines, format) run through `list_commands` filter `edit.` and `run_command`.
4. Put SQL you propose in a fenced ```sql block; the user can copy it, insert it, or run it from the block.
",
    },
    Skill {
        name: "results",
        when: "the user asks about query results, the result grid, row counts, or wants data fetched",
        body: "\
# Skill: results and queries

1. `get_result` returns the active result grid (columns + rows up to max_rows). Use it when the user asks about 'the result' / 'these rows'.
2. `run_query` runs SQL in the current session and returns rows. Only SELECT/WITH/SHOW/EXPLAIN run after a read-only check; anything else asks the user first. Row data leaves the app only after the user consents once per chat session. `run_query` results come back to you only; the user's grid does not change. When the user should see rows, finish with exactly one final ```sql block: the app runs the last read-only block into the result grid by itself (do not also paste the rows).
3. Keep max_rows small (the app caps it); summarize instead of dumping large results.
4. Never write DML/DDL unless the user asked for it explicitly; the app enforces a one-row rule for its own edits and asks approval for yours.
5. When the user reports an error or asks to check Output/Log: call `get_log` with errors_only=true (server error text and time) and `get_output` (statement errors with line numbers) first, then `get_editor_text` for the statement that failed. Quote the exact error code.
6. Verify before the final answer: run the exact statement you are about to show with `run_query` (small max_rows) and only then present it. The text in your final ```sql block must be the statement you verified, unchanged.
",
    },
    Skill {
        name: "app",
        when: "the user asks to change a setting, run an app command, open a panel, or asks how Nexa SQL works",
        body: "\
# Skill: app settings and commands

1. `get_setting` with an empty key lists all keys; `set_setting` changes one (persisted; bool = on/off).
2. `list_commands` lists palette commands (id + label); `run_command` runs one (e.g. view.explorer, edit.format).
3. Prefer explaining what a setting does and ask before changing user preferences unless the request is explicit.
",
    },
    Skill {
        name: "tabs",
        when: "the user talks about editor tabs: which tab, switch to a tab, new tab, close tab, the other script, unsaved tabs",
        body: "\
# Skill: editor tabs

1. `list_tabs` — id, title, file path, dirty/clean, active. The chat pane follows the active editor tab (one AI session per tab in the side pane).
2. `switch_tab` with the id or exact title to make a tab active; then `get_editor_text` reads that tab.
3. New tab = `run_command` id `file.new`; next/previous = `tab.next` / `tab.prev`; close = `file.close_tab` (the app asks about unsaved changes — never force).
4. A tab bound to a private session shows that session in `session_info`; `CONNECT <profile>;` in a tab creates one (see connections).
",
    },
    Skill {
        name: "objects",
        when: "the user wants to open, reveal, compare, refresh or get DDL/source of a database object, or row counts/sizes",
        body: "\
# Skill: database objects

1. Locate with the metadata skill (`search_objects` → `describe_object`).
2. Source/DDL: `object_source` returns the definition as text WITHOUT opening anything (procedures, views, triggers, packages - `part=body` for a package body, table DDL where supported). Analysing, verifying, tracing or explaining code must never open editor tabs; pass `open=true` only when the user asked to open / show the object in the editor.
3. To go to an object in the explorer use the `reveal_object` tool (name, optional schema / member) - it works from the chat too and returns the selected path; do NOT move the editor caret and run `obj.reveal` for this. Other explorer actions via `run_command` (they act on the node selected in the explorer, so reveal first): `obj.info`, `obj.rows` (row count), `obj.sizes`, `obj.select_rows` (SELECT * tab), `obj.refresh` / `obj.refresh_meta` (reload a branch / metadata), `obj.compare` / `obj.compare_server` (compare definitions).
4. Never generate or run DDL that changes objects unless the user asked explicitly; it needs approval anyway.
",
    },
    Skill {
        name: "files",
        when: "the user asks to find, open or list files/scripts in the project or folder, recent files, or where a script is",
        body: "\
# Skill: project files

1. `list_files` with path words (ANDed) finds files under the current project/folder root (hidden and build folders skipped, bounded). Empty query lists the first entries.
2. `open_file` opens one in an editor tab (relative to the root or absolute); with `line` the caret goes to that line.\n2b. `filter_project` shows the Project explorer panel and fills its 'Filter files' box (what the user means by 'search / filter in the project explorer').\n2a. `search_files` searches file CONTENTS (query + case / word / regex + `where` scope patterns) and returns `path:line:column` rows - then `open_file` with that line. `list_files` only matches path names.
3. Project commands via `run_command`: `project.open`, `project.switch`, `project.save`, `project.close`; `file.open` opens the file dialog for the user; `view.project` shows the project panel.
4. Do not read file contents through the shell; open the file and use `get_editor_text` if the content matters.
",
    },
    Skill {
        name: "extensions",
        when: "the user asks what extensions are installed, to enable/disable one, formatters, or how to install/update extensions",
        body: "\
# Skill: extensions

1. `list_extensions` — installed and built-in extensions with enabled state (formatters, rainbow pairs, insert sequence, WASM plugins).
2. `set_extension_enabled` (asks the user) to turn one on or off; `apply` happens immediately.
3. Installing/updating from a repository is done by the user in the Extensions panel (`run_command` `view.extensions`); do not attempt network installs from here.
4. Formatting SQL uses the engine chosen in settings (`format.*`); `format_sql` applies it. The app also formats every ```sql block you return (hook).
",
    },
];

/// 시스템 프롬프트용 색인(이름 — 언제).
pub(crate) fn index_text() -> String {
    SKILLS
        .iter()
        .map(|s| format!("- {}: when {}", s.name, s.when))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn body(name: &str) -> Option<&'static str> {
    let n = name.trim().to_ascii_lowercase();
    SKILLS.iter().find(|s| s.name == n).map(|s| s.body)
}

pub(crate) fn names() -> Vec<&'static str> {
    SKILLS.iter().map(|s| s.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_and_lookup() {
        let idx = index_text();
        for s in SKILLS {
            assert!(idx.contains(s.name));
            assert!(s.body.starts_with("# Skill:"), "{}", s.name);
        }
        assert!(body("Connections").is_some_and(|b| b.contains("session_info")));
        assert!(body("nope").is_none());
        assert_eq!(names().len(), SKILLS.len());
    }
}
