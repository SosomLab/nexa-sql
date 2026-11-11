//! ★ 연계 추상(docs/110 §16 · 사용자 11-11 "용어는 Claude식이지만 Codex 등 다른 AI도 같은 기능을 추상화 + 구현부로"): 하네스의
//! 네 개념(**도구 · 스킬 · 훅 · 에이전트**)은 공급자와 무관한 앱 쪽 자료(`tools.rs` · `skills.rs` · `hooks.rs` · `agents.rs`)이고,
//! 공급자(Claude API · Claude CLI · Codex app-server)는 **전달 방식**만 다르다. 이 표가 그 대응이며, 새 공급자는 여기에 한 줄과
//! 전달 구현(host.rs 런처 + 이벤트 파서)을 보태면 된다 — 도구·스킬·훅·에이전트는 손대지 않는다.
//!
//! | 개념 | 앱 쪽 원천 | Claude API | Claude CLI(`claude -p`) | Codex(app-server) |
//! |---|---|---|---|---|
//! | 도구 | `tools::TOOLS` | 요청 `tools`(`api_tools`) · 호출 = `tool_use` 블록 | MCP 서버 `nexa-sql`(`mcp_tools` · `nsql mcp-bridge`) · 이름 `mcp__nexa-sql__<도구>` | `thread/start.dynamicTools` · 호출 = `item/tool/call` |
//! | 승인 | `ai_tool_request`(쓰기 도구 · 행 데이터) | 앱이 직접(카드) | `--permission-prompt-tool mcp__nexa-sql__approve` | `item/tool/call` 응답 전 카드 |
//! | 스킬 | `skills::SKILLS`(색인 + `skill` 도구) | 시스템 프롬프트 + 도구 | `--append-system-prompt` + MCP 도구 | 첫 턴 `<app_context>` + dynamicTools |
//! | 훅 | `hooks::post_turn`(턴 끝 · 결정적) | 공통(호스트에서) | 공통 | 공통 |
//! | 에이전트 | `agents::AGENTS`(지시 + 허용 도구) | 시스템 프롬프트 + 호스트 강제 | 같음 | 같음 |
//!
//! 사용자 용어(Claude Code식)와 Codex 용어의 대응: MCP 서버 ↔ dynamicTools · 시스템 프롬프트 ↔ `<app_context>` · 승인 도구 ↔ tool/call 응답.

use super::{Mode, Provider};

/// MCP 서버 이름(Claude CLI) — 도구 이름 접두 `mcp__<서버>__`.
pub(crate) const MCP_SERVER: &str = "nexa-sql";

/// 도구 이름 접두(Claude CLI의 MCP 호출 이름) — `MCP_SERVER`와 짝(시험이 지킨다).
pub(crate) const MCP_TOOL_PREFIX: &str = "mcp__nexa-sql__";

/// 도구 이름 접두(Claude CLI의 MCP 호출 이름) → 앱 도구 이름.
pub(crate) fn tool_short_name(name: &str) -> &str {
    name.strip_prefix(MCP_TOOL_PREFIX).unwrap_or(name)
}

/// 승인 도구의 전체 이름(Claude CLI `--permission-prompt-tool`).
pub(crate) fn approve_tool_name() -> String {
    format!("mcp__{MCP_SERVER}__approve")
}

/// 전달 방식(공급자 × 모드) — 설명·덤프·문서용.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// 요청 본문 `tools` + `system`.
    ClaudeApi,
    /// MCP 서버 + `--append-system-prompt`.
    ClaudeCli,
    /// app-server `dynamicTools` + 첫 턴 `<app_context>`.
    Codex,
}

pub(crate) fn delivery(provider: Provider, mode: Mode) -> Delivery {
    match (provider, mode) {
        (Provider::Claude, Mode::Api) => Delivery::ClaudeApi,
        (Provider::Claude, Mode::Cli) => Delivery::ClaudeCli,
        (Provider::Codex, _) => Delivery::Codex,
    }
}

impl Delivery {
    /// 한 줄 설명(`/status` · 문서).
    pub(crate) fn describe(self) -> &'static str {
        match self {
            Delivery::ClaudeApi => "tools in request · system prompt · approval = app card",
            Delivery::ClaudeCli => {
                "MCP server nexa-sql · --append-system-prompt · approval = mcp__nexa-sql__approve"
            }
            Delivery::Codex => {
                "dynamicTools · <app_context> on first turn · approval = tool/call reply"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_delivery() {
        assert_eq!(tool_short_name("mcp__nexa-sql__run_query"), "run_query");
        assert_eq!(tool_short_name("run_query"), "run_query");
        assert_eq!(approve_tool_name(), "mcp__nexa-sql__approve");
        assert_eq!(MCP_TOOL_PREFIX, format!("mcp__{MCP_SERVER}__"));
        assert_eq!(delivery(Provider::Claude, Mode::Cli), Delivery::ClaudeCli);
        assert_eq!(delivery(Provider::Codex, Mode::Api), Delivery::Codex);
        assert!(Delivery::Codex.describe().contains("dynamicTools"));
    }
}
