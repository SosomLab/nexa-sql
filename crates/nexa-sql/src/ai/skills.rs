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

Nexa SQL (not the DBMS) owns the connection state. Answer from the app first.

Terms:
- Shared connection: one connection per server/account that any editor tab can use (the app's default). There can be several (different servers), one is the active default.
- Private session: a connection owned by a single editor tab (created by a CONNECT statement in that editor or per-editor mode).
- Profile: a saved login (host, port, user, dialect). Passwords live in the vault and are never shown.

Steps:
1. Call `session_info`. It returns the current editor's session (profile, dialect, connected, schema/current db, busy, pending uncommitted statements, masked description) and the list of ALL app sessions (shared/private, owner tab, connected/broken/busy, idle seconds, pending count).
2. Answer directly from that output. Do NOT run SQL against v$session, pg_stat_activity, sys.dm_exec_sessions or similar unless the user explicitly asks about server-side sessions, other users, locks or waits.
3. Saved profiles: `list_profiles`. To open a connection: `connect` with the profile name (the user confirms the login).
4. 'Which schema/database am I in' = schema / current db from `session_info`; do not guess from SQL text.
",
    },
    Skill {
        name: "metadata",
        when: "the user asks about tables, views, columns, keys, procedures, functions, triggers or DDL of a connected database",
        body: "\
# Skill: metadata (use what the app already loaded)

The object explorer caches metadata per connection. Prefer it over dictionary queries.

Steps:
1. `search_objects` with a name substring (and schema when known) to find the object and its kind.
2. `describe_object` for columns (name, type, nullable, key) and status/extra info.
3. `object_source` to open DDL/source (procedures, views, triggers, packages) in a tab and read it.
4. Only when the explorer has no information for that object (not loaded, unknown schema), run a read-only dictionary query with `run_query` and say that it came from the database. Examples: Oracle ALL_TAB_COLUMNS / ALL_CONSTRAINTS, PostgreSQL information_schema.columns, SQL Server sys.columns, MySQL information_schema.COLUMNS.
5. Scope names with the current schema from `session_info` when the user does not qualify them. Never modify objects from this skill.
",
    },
    Skill {
        name: "editor",
        when: "the user refers to the SQL in the editor: review, fix, format, explain, rewrite, or asks to put SQL into the editor",
        body: "\
# Skill: editor

1. `get_editor_text` returns the whole text, the selection and the statement at the caret. Work on the selection when there is one, else the statement at the caret, else the whole text.
2. `format_sql` formats with the app formatter (editor unchanged); `set_editor_text` replaces the text (undoable) or inserts at the caret with mode=insert. Replacing is a write: tell the user what changes.
3. `explain` shows the execution plan of one statement in the current session.
4. Put SQL you propose in a fenced ```sql block; the user can copy it, insert it, or run it from the block.
",
    },
    Skill {
        name: "results",
        when: "the user asks about query results, the result grid, row counts, or wants data fetched",
        body: "\
# Skill: results and queries

1. `get_result` returns the active result grid (columns + rows up to max_rows). Use it when the user asks about 'the result' / 'these rows'.
2. `run_query` runs SQL in the current session and returns rows. Only SELECT/WITH/SHOW/EXPLAIN run after a read-only check; anything else asks the user first. Row data leaves the app only after the user consents once per chat session.
3. Keep max_rows small (the app caps it); summarize instead of dumping large results.
4. Never write DML/DDL unless the user asked for it explicitly; the app enforces a one-row rule for its own edits and asks approval for yours.
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
