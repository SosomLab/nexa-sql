//! ★ agents(docs/110 §16 · 사용자 11-11): 세션의 **역할 프리셋** — 시스템 프롬프트에 붙는 지시 + 쓸 수 있는 도구 목록. 별도 프로세스·
//! 서브에이전트가 아니라 "같은 세션을 역할로 좁히는 것"(`/agent <이름>` · 기본 `assistant` = 전부). 도구 허용은 호스트가
//! `ai_tool_request`에서 강제한다(API·CLI·Codex 공통 · 모델이 목록을 받아도 호출은 막힌다).

pub(crate) struct Agent {
    pub name: &'static str,
    pub desc: &'static str,
    pub instructions: &'static str,
    /// None = 모든 도구.
    pub tools: Option<&'static [&'static str]>,
}

pub(crate) const AGENTS: &[Agent] = &[
    Agent {
        name: "assistant",
        desc: "general assistant — all tools",
        instructions: "",
        tools: None,
    },
    Agent {
        name: "schema",
        desc: "schema investigator — read-only, metadata tools first, answers with tables/columns/keys",
        instructions: "Role: schema investigator. Work only from loaded metadata (meta_stats, search_objects, describe_object, object_source) and read-only dictionary queries when metadata is missing. Never modify anything. Answer with a compact list of objects, columns and keys, then at most two sentences.",
        tools: Some(&[
            "skill", "session_info", "list_profiles", "meta_stats", "search_objects", "describe_object",
            "object_source", "run_query", "explain", "get_editor_text", "format_sql",
        ]),
    },
    Agent {
        name: "reviewer",
        desc: "SQL reviewer — explains plans, formats, suggests rewrites; never runs data changes",
        instructions: "Role: SQL reviewer. Read the editor text, check objects with the metadata tools, use explain for plans and format_sql for layout. Point out correctness risks, missing predicates, implicit conversions and index use. Propose a rewritten statement in one ```sql block. Do not run anything except read-only checks.",
        tools: Some(&[
            "skill", "session_info", "meta_stats", "search_objects", "describe_object", "object_source",
            "get_editor_text", "set_editor_text", "format_sql", "explain", "run_query", "get_result",
        ]),
    },
    Agent {
        name: "dba",
        desc: "DBA helper — sessions, locks, waits via read-only dictionary queries; app sessions via session_info",
        instructions: "Role: DBA helper. For app-level questions use session_info; for server-side sessions, locks, waits and parameters run read-only dictionary queries appropriate to the dialect and say which view you used. Never change data or settings.",
        tools: Some(&[
            "skill", "session_info", "list_profiles", "connect", "meta_stats", "search_objects",
            "describe_object", "run_query", "explain", "get_result",
        ]),
    },
];

pub(crate) fn get(name: &str) -> Option<&'static Agent> {
    let n = name.trim().to_ascii_lowercase();
    AGENTS.iter().find(|a| a.name == n)
}

pub(crate) fn names() -> Vec<&'static str> {
    AGENTS.iter().map(|a| a.name).collect()
}

/// 이 에이전트가 그 도구를 쓸 수 있나(모르는 에이전트 = 전부 허용).
pub(crate) fn allows(agent: &str, tool: &str) -> bool {
    match get(agent).and_then(|a| a.tools) {
        Some(list) => list.contains(&tool),
        None => true,
    }
}

/// 시스템 프롬프트에 붙는 글(기본 에이전트 = 빈 글).
pub(crate) fn prompt_text(agent: &str) -> String {
    match get(agent) {
        Some(a) if !a.instructions.is_empty() => {
            let tools = a
                .tools
                .map(|t| t.join(", "))
                .unwrap_or_else(|| "all".into());
            format!(
                "\n\nAgent `{}`: {}\nTools available to this agent: {tools}.",
                a.name, a.instructions
            )
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_and_allow_list() {
        assert!(allows("assistant", "set_setting"));
        assert!(allows("schema", "describe_object"));
        assert!(!allows("schema", "set_setting"));
        assert!(!allows("reviewer", "connect"));
        assert!(allows("nope", "anything"));
        assert!(prompt_text("assistant").is_empty());
        assert!(prompt_text("dba").contains("Agent `dba`"));
        assert_eq!(names().len(), AGENTS.len());
    }
}
