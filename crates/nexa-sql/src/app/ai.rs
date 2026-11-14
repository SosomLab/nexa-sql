//! App — ★ AI 어시스턴트(Claude 탭 · docs/110 · 사용자 11-10/11-11): 패널·뷰·호스트를 잇는 동작 — 세션 관리(새 대화 · 이전 기록 열기/
//! 이어하기 · 이름 · 삭제) · 보내기(API/CLI/각본) · 이벤트 수거 · **앱 기능 도구 실행**(UI 스레드 한 곳) · 승인 카드 · 첨부 ·
//! `/` 명령 · 프로필(볼트 키) · 헤드리스 명령(`ai.dump:` · `ai.send:` · `ai.open`).

use crate::ai::claude_api::ApiAuth;
use crate::ai::host::{ApiLaunch, CliLaunch, CodexLaunch, Host};
use crate::ai::panel::{PanelAction, PanelState, ProfileRow, SessionRow};
use crate::ai::view::{Page, ViewAction, ViewInfo};
use crate::ai::{
    self, redact, AiEvent, Attachment, ChatMsg, Kind, Mode, Profile, ProfileStore, Provider,
    Session, ToolReply, Usage,
};
use crate::*;
use nsql_settings::json::{parse as json_parse, Json};
use std::sync::mpsc;
use std::time::Instant;

/// 승인 대기 한 건.
pub(crate) enum ApprovalReply {
    /// CLI 권한 프롬프트(MCP `approve`).
    Cli(mpsc::Sender<bool>),
    /// API 도구 호출(허용 = 지금 실행 · 거부 = 오류 결과).
    Api {
        name: String,
        input: String,
        reply: mpsc::Sender<ToolReply>,
        /// 2단계 검증 대상(대량·파괴적 변경 · guard.rs) — 허용 뒤 영향 확인 카드가 한 번 더.
        two: bool,
        /// ★ 이 카드가 행 데이터 동의도 겸한다(11-13 사용자 "중복된 내용을 굳이 2번 표시하는건 낭비"): 쓰기 문장은 동의 카드와
        /// 쓰기 확인 카드를 따로 띄우지 않고 한 장으로 — 허용 = `rows_allowed` + 실행.
        rows: bool,
        /// ★ 두 번 눌러 허용(운영 접속의 DB 쓰기 · 11-13 사용자 "Allow 버튼이 2중 확인 버튼"): 첫 누름 = 장전(`AiState::armed`) ·
        /// `run.prod_confirm_ms` 안에 다시 누르면 실행 · 지나면 무효.
        twice: bool,
    },
    /// ★ 게이트 2(영향 확인 카드 · guard.rs): 허용 = 실행 · 거부 = 오류 결과.
    Stage2 {
        name: String,
        input: String,
        reply: mpsc::Sender<ToolReply>,
    },
    /// 행 데이터 동의만(허용 = `rows_allowed` 뒤 도구 실행).
    Rows {
        name: String,
        input: String,
        reply: mpsc::Sender<ToolReply>,
    },
}

/// 도구가 기다리는 화면 쪽 비동기 일.
pub(crate) enum AiWait {
    /// 파일 내용 검색이 끝나기를(결과 줄 상한).
    Search { limit: usize },
    /// 객체 탐색기 이동이 끝나기를(찾던 이름 — 선택 경로에 있는지로 성공 판정).
    Reveal { name: String },
    /// 객체 소스를 **탭 없이** 읽는 중(`object_source` · 응답은 meta.rs `OpenSql`이 `ai_source_take`로 가로챈다).
    Source,
}

/// 비동기 도구(실행 결과가 나중에 온다).
pub(crate) struct PendingTool {
    pub reply: Option<mpsc::Sender<ToolReply>>,
    /// 결과를 첨부로(= `/query`).
    pub attach_label: Option<String>,
    pub since: Instant,
    pub max_rows: usize,
    /// 객체 소스 열기 대기(열기 전 활성 편집기 id).
    pub wait_editor: Option<u64>,
    /// ★ 메타 적재 대기(`describe_object` · 11-14): (스키마 · 이름) — 그 객체의 컬럼이 읽히면(또는 상한이 지나면) 설명을 답한다.
    pub wait_meta: Option<(String, String)>,
    /// ★ 화면 쪽 비동기 일을 기다린다(파일 검색 · 객체 탐색기 이동 · 11-14).
    pub wait_ui: Option<AiWait>,
    /// ★ 숨김 실행(11-12 사용자 "과정에 필요한 쿼리는 자체적으로 수행"): AI 도구 `run_query`/`explain`의 결과는 글로만 잡고
    /// 결과 그리드·결과 탭에는 싣지 않는다(`on_ev_result_set`이 본다).
    pub silent: bool,
    /// 끝나면 이 변수의 상태(`get_var`)를 답으로(도구 `set_var`의 수식 지정 · 11-12).
    pub var_after: Option<String>,
}

#[derive(Default)]
pub(crate) struct AiState {
    pub host: Option<Host>,
    pub sessions: Vec<Session>,
    pub cur: Option<usize>,
    pub store: ProfileStore,
    pub mcp: Option<ai::mcp::McpServer>,
    pub approval: Option<(String, String, String, ApprovalReply)>,
    /// ★ 방금 답한 승인 카드(카드 id · 허용 여부 · 누른 시각 · 11-13): 일이 끝날 때까지 카드 버튼을 비활성 + 진행 막대로 남긴다 —
    /// 비우는 곳 = `ai_tick_due`(최소 표시 시간이 지나고 도구 실행·세션 작업이 끝났을 때).
    pub approving: Option<(String, bool, Instant)>,
    pub pending: Option<PendingTool>,
    pub capture: Option<String>,
    pub note: String,
    pub status: String,
    pub turn_started: Option<Instant>,
    /// CLI 모드 턴 사용량(result 줄이 합계 · Done에 더한다).
    pub cli_turn_usage: Option<Usage>,
    pub inited: bool,
    /// ★ 활동 막대 탭(Claude / Codex · 11-11) — 패널·채팅 탭·세션 목록은 이 제공자 것만.
    pub tab: Provider,
    /// 탭을 떠날 때 보던 세션 id(돌아오면 복원).
    pub last_claude: Option<String>,
    pub last_codex: Option<String>,
    /// 지금 턴이 도는 세션 id — 호스트 이벤트는 여기로(탭·세션을 바꿔도 다른 대화에 섞이지 않게).
    pub run: Option<String>,
    /// `codex login status` 첫 줄(Codex CLI 모드 · 패널 바닥 줄 · None = 아직 안 물음 · Some("") = 조회 중).
    pub codex_login: Option<String>,
    /// ★ CLI 로그인 계정이 쓸 수 있는 Codex 모델(`model/list` · 로그인 상태 조회와 함께 · 사용자 11-11 "Free에서 안 되는 모델이 기본").
    pub codex_cli_models: Vec<String>,
    /// 코드 블록 [실행] 대기(메시지 번호 · 블록 번호) — 결과는 그 메시지의 `block_results`로.
    pub run_block: Option<(usize, usize)>,
    /// 진행 중 상태 줄 갱신 마지막 시각(1초 간격) · 턴 시작 시각(HH:MM:SS).
    pub last_tick: Option<Instant>,
    pub turn_wall: String,
    /// ★ 편집기 옆 분할 창(사용자 11-11): 켬 · 자리(레이아웃이 잡는다) · 키 포커스(안을 눌렀을 때).
    pub side: bool,
    /// ★ 분할 창을 켠 편집기 탭들(사용자 11-11 "탭마다 직접 켜야 보이게") — `side`는 마지막 토글 값(설정 `ai.side`에 저장 · 시작 때 활성 탭에만 적용).
    pub side_tabs: std::collections::HashSet<u64>,
    pub side_rect: Rect,
    pub side_focus: bool,
    /// ★ 탭별 세션(사용자 11-11 "각 탭마다 AI 보기 · 쓰던 세션 연결 · 없으면 새로"): 분할 창이 보는 편집기 탭 · 탭 id → 세션 id.
    pub side_tab: Option<u64>,
    pub tab_sessions: std::collections::HashMap<u64, String>,
    /// ★ 채팅 탭이 보는 세션(제공자별 · 사용자 11-11 결정 "분할 창과 채팅 탭은 서로 다른 세션") — 분할 창은 편집기 탭별
    ///   `tab_sessions`, 채팅 탭은 이것. `on_chat` = 마지막으로 그린 면이 그 제공자의 채팅 탭(None = 분할 창/없음).
    pub chat_claude: Option<String>,
    pub chat_codex: Option<String>,
    pub on_chat: Option<Provider>,
    /// ★ 세션별 작성 상자 초안(세션 id → 글 · 11-12).
    pub drafts: std::collections::HashMap<String, String>,
    /// ★ 미리 띄운 CLI 프로세스의 띄우기 서명(아직 턴을 안 보냄 · 11-12) · 미리 띄우기를 시도한 세션 id(세션당 한 번).
    pub warm: Option<String>,
    pub warm_sid: Option<String>,
    /// ★ 세션이 바쁠 때 온 조회 도구 호출의 대기열(11-12 "session busy" 오류 대신 차례로 실행).
    pub tool_queue: std::collections::VecDeque<(String, String, mpsc::Sender<ToolReply>)>,
    /// ★ 턴이 끝난 뒤 결과 그리드에 자동으로 보일 최종 조회문(`ai.show_result=auto`).
    pub auto_show: Option<String>,
    /// ★ 자동 표시 검증(11-12 "실행된다고 답했지만 실제 실행하면 오류"): 자동 표시 실행을 지켜보는 중(시작 시각) · 훅이 포맷으로 바꾼
    /// 마지막 답의 원문(세션 id · 메시지 번호 · 원문) — 포맷한 문장이 오류면 원문으로 되돌리고 원문을 다시 실행한다.
    pub auto_run: Option<Instant>,
    /// ★ 게이트 2 준비(영향 행 수 사전 조회가 도는 중 · 도구 · 입력 · 답 통로 · 2단계 강제) · 영향 확인 카드를 띄운 시각(운영 = 대기 뒤에만 승인).
    pub impact_wait: Option<(String, String, mpsc::Sender<ToolReply>, bool)>,
    /// ★ 방금 허용한 쓰기 카드의 id(11-13 "모두 동일한 개념으로"): 영향 행 수가 나오면 **새 카드를 띄우지 않고** 이 카드에
    /// 영향 줄만 덧붙여 장전된 [허용]으로 되돌린다(`ai_impact_done`).
    pub last_card: Option<String>,
    /// ★ 메타 우선 되돌림(11-14)을 한 턴(= 마지막 사용자 메시지 자리)에서 이미 했다 — 같은 턴의 다음 사전 조회는 보낸다.
    pub meta_nudged_at: Option<usize>,
    /// ★ 도구 `run_query`의 실행이 **객체 백업 뒤로 미뤄졌다**(운영 DROP 등 · 11-13): 대기 중인 도구 호출(`pending`)을 버리지 않고
    /// 백업 → 재진입 실행의 결과를 같은 통로로 보낸다. 종전에는 "not executed"를 먼저 답했는데 백업이 끝나면 문장이 **실제로
    /// 실행**돼 어시스턴트가 아는 것과 DB가 어긋났다(재시도하면 이중 실행).
    pub objbk_wait: bool,
    /// ★ 답하는 중에 보낸 요청의 대기 줄(세션 id · 글 · **그때의 첨부** · 11-14) — 턴이 끝나면 그 세션이 보이는 동안 차례로 보낸다.
    /// 첨부는 줄 세울 때 세션에서 떼어 요청에 묶는다(사용자 11-14 "글만 풀에 들어가고 이미지는 입력란 위에 남았다").
    pub send_queue: std::collections::VecDeque<(String, String, Vec<ai::Attachment>)>,
    /// ★ 탭 없이 읽는 객체 소스(사용자 11-14 "단순 분석 요청이면 탭을 열지 말 것"): 기다리는 출처(스키마 · 이름 · 종류 · 요청 시각)
    /// — 탐색기의 소스 응답(`OpenSql`)이 이것과 같으면 탭을 열지 않고 `source_got`에 담는다(서버 히스토리 저장과 같은 가로채기).
    pub source_wait: Option<(String, String, nsql_catalog::ObjectKind, Instant)>,
    /// 가로챈 소스 글(또는 실패 글) — `ai_tick_pending`이 꺼내 도구 답으로 보낸다.
    pub source_got: Option<(String, bool)>,
    /// ★ 두 번 눌러 허용하는 카드의 첫 누름 시각(`ApprovalReply::Api::twice` · 시간이 지나면 `ai_tick_due`가 비운다).
    pub armed: Option<Instant>,
    pub fmt_orig: Option<(String, usize, String)>,
    /// ★ 마지막 실행 오류(문장 · 메시지 · 편집기 줄) — "AI에게 수정 요청"이 쓴다 · 알림 버튼을 마지막으로 띄운 시각(연달아 안 띄움).
    pub last_error: Option<(String, String, usize)>,
    /// ★ 이중 안전장치(11-12 P0 뒤): 사용자가 승인 카드로 **허용한 비조회 호출**(도구 · 입력) — `ai_exec_tool`의 run_query는 비조회 SQL이면
    ///   이 기록이 같을 때만 실행하고 바로 비운다. 어떤 경로가 확인을 건너뛰어도 실행은 막힌다.
    pub write_ok: Option<(String, String)>,
}

impl App {
    pub(crate) fn ai_enabled(&self) -> bool {
        self.settings.flag("ai.enabled")
    }

    /// 활성 편집기 탭에 분할 창이 켜져 있나(탭별 · 11-11).
    pub(crate) fn ai_side_on(&self) -> bool {
        self.ai.side_tabs.contains(&self.editors.active_id())
    }

    pub(crate) fn ai_view_active(&self) -> bool {
        self.editors
            .active_view()
            .is_some_and(|k| k.starts_with("ai.chat") || k.starts_with("ai.codex"))
    }

    /// 채팅 뷰 탭 열쇠 = `<제공자 키>:<세션 id>`(세션마다 탭 · 11-12).
    fn ai_view_key(prov: Provider, sid: &str) -> String {
        format!("{}:{sid}", prov.view_key())
    }

    /// ★ 표시 레벨 토글(팔레트 "AI: 상세 보기" · 설정 `ai.verbosity` · 사용자 11-12): basic = 도구 호출/성공 결과 줄 숨김.
    pub(crate) fn ai_toggle_detail(&mut self) {
        let on = self.settings.get("ai.verbosity") == Some("detail");
        let _ = self
            .settings
            .set("ai.verbosity", if on { "basic" } else { "detail" });
        self.persist_settings();
        self.ai_sync_panel();
        self.redraw();
    }

    /// 진행 중 활동 줄(사용자 11-12): "진행: <마지막 도구> · 도구 N회 · N s" — 턴 중에만(없으면 빈 글).
    fn ai_activity_text(&self) -> String {
        if !self.ai.host.as_ref().is_some_and(|h| h.busy) {
            return String::new();
        }
        let Some(s) = self.ai_cur() else {
            return String::new();
        };
        let Some(lu) = s.messages.iter().rposition(|m| m.kind == Kind::User) else {
            return String::new();
        };
        let now = ai::now_ms();
        let calls: Vec<&ChatMsg> = s.messages[lu + 1..]
            .iter()
            .filter(|m| matches!(m.kind, Kind::ToolCall { .. }))
            .collect();
        let (last, since) = match calls.last() {
            Some(m) => {
                let name = match &m.kind {
                    Kind::ToolCall { name, .. } => name.clone(),
                    _ => String::new(),
                };
                (name, now.saturating_sub(m.at) as f64 / 1000.0)
            }
            None => (
                t(Msg::AiWaitingFirst).to_string(),
                now.saturating_sub(s.messages[lu].at) as f64 / 1000.0,
            ),
        };
        tf(
            Msg::AiActivityLine,
            &[&last, &calls.len().to_string(), &format!("{since:.0}")],
        )
    }

    /// 진행 단계(사용자 11-12 "①②③ 중 현재 단계 깜빡임"): 0 = 턴 없음 · 1 = 요청 보냄(첫 응답 전) · 2 = 응답 수신 중 · 3 = 완료.
    fn ai_stage(&self) -> u8 {
        let busy = self.ai.host.as_ref().is_some_and(|h| h.busy);
        let Some(s) = self.ai_cur() else { return 0 };
        let last_user = s.messages.iter().rposition(|m| m.kind == Kind::User);
        let Some(lu) = last_user else { return 0 };
        let responded = s.messages.iter().skip(lu + 1).any(|m| {
            matches!(m.kind, Kind::ToolCall { .. } | Kind::ToolResult { .. })
                || (m.kind == Kind::Assistant && !m.text.is_empty())
        });
        if busy {
            if responded {
                2
            } else {
                1
            }
        } else if s.messages.len() > lu + 1 {
            3
        } else {
            0
        }
    }

    /// ★ 세션을 바꿀 때 작성 상자 초안을 세션별로 보존(11-12): 떠나는 세션의 글을 저장하고 새 세션의 초안을 꺼낸다(빈 글 = 지움).
    fn ai_switch_cur(&mut self, i: usize) {
        let txt = self.ai_view.composer_text();
        if let Some(old) = self.ai_cur().map(|s| s.id.clone()) {
            if txt.trim().is_empty() {
                self.ai.drafts.remove(&old);
            } else {
                self.ai.drafts.insert(old, txt);
            }
        }
        self.ai.cur = Some(i);
        let next = self
            .ai_cur()
            .and_then(|s| self.ai.drafts.get(&s.id).cloned())
            .unwrap_or_default();
        self.ai_view.set_composer_text(&next);
    }

    /// 설정 `ai.provider`가 바뀌면 패널 탭도 그 제공자로(사용자 11-12 "기본을 Claude로 바꿔도 Codex로 열림").
    pub(crate) fn ai_apply_provider_default(&mut self) {
        self.ai_init();
        let p = self.ai_provider_default();
        self.ai_switch_tab(p);
        self.ai_view.set_provider(p);
        self.ai_sync_panel();
        self.redraw();
    }

    /// 편집기에서 채팅 탭이 앞으로 오면 패널 탭·현재 세션도 그 탭의 것으로(열쇠 = 제공자:세션 · 그리기 직전에 부른다 · 11-12).
    pub(crate) fn ai_follow_view(&mut self) {
        let Some(key) = self.editors.active_view().map(str::to_string) else {
            return;
        };
        let (want, sid) = if let Some(r) = key.strip_prefix("ai.codex") {
            (Provider::Codex, r)
        } else if let Some(r) = key.strip_prefix("ai.chat") {
            (Provider::Claude, r)
        } else {
            return;
        };
        let sid = sid.trim_start_matches(':').to_string();
        if want != self.ai.tab {
            self.ai_switch_tab(want);
            self.ai_sync_panel();
        }
        let busy = self.ai.host.as_ref().is_some_and(|h| h.busy);
        if !sid.is_empty() && !busy {
            if let Some(i) = self.ai.sessions.iter().position(|s| s.id == sid) {
                if self.ai.cur != Some(i) {
                    self.ai_switch_cur(i);
                    self.ai.approval = None;
                    self.ai.pending = None;
                    self.ai.objbk_wait = false;
                    self.ai_view.reset();
                    self.ai_sync_panel();
                }
            }
        }
    }

    /// 탭 전환 — 보던 세션을 기억하고 그 제공자의 마지막 세션으로(없으면 비움 · 채팅 열 때 새로).
    fn ai_switch_tab(&mut self, p: Provider) {
        if self.ai.tab == p {
            return;
        }
        let cur_id = self.ai_cur().map(|s| s.id.clone());
        match self.ai.tab {
            Provider::Claude => self.ai.last_claude = cur_id,
            Provider::Codex => self.ai.last_codex = cur_id,
        }
        self.ai.tab = p;
        self.ai_view.set_provider(p);
        let want = match p {
            Provider::Claude => self.ai.last_claude.clone(),
            Provider::Codex => self.ai.last_codex.clone(),
        };
        self.ai.cur = want
            .and_then(|id| self.ai.sessions.iter().position(|s| s.id == id))
            .or_else(|| self.ai.sessions.iter().position(|s| s.provider == p));
        self.ai.status.clear();
        self.ai_view.reset();
    }

    /// 기동 · 켤 때: 프로필 저장소 · 이력 목록 · 패널 상태.
    pub(crate) fn ai_init(&mut self) {
        if self.ai.inited {
            return;
        }
        self.ai.inited = true;
        if let Some(dir) = ProfileStore::dir() {
            self.ai.store = ProfileStore::load(&dir);
        }
        ai::history_prune(self.settings.int("ai.history_keep_days").max(0) as u64);
        self.ai.sessions = ai::history_list(200);
        self.ai.cur = None;
        self.ai.tab = self.ai_provider_default();
        self.ai.side = self.settings.flag("ai.side");
        if self.ai.side {
            let tid = self.editors.active_id();
            self.ai.side_tabs.insert(tid);
        }
        self.ai_sync_panel();
    }

    fn ai_chat_slot(&mut self, p: Provider) -> &mut Option<String> {
        match p {
            Provider::Claude => &mut self.ai.chat_claude,
            Provider::Codex => &mut self.ai.chat_codex,
        }
    }

    /// 지금 세션 = 채팅 탭의 세션으로 기록(채팅 탭을 열 때 · 채팅 탭을 그릴 때).
    fn ai_chat_mark(&mut self) {
        let p = self.ai.tab;
        if let Some(id) = self.ai_cur().map(|s| s.id.clone()) {
            *self.ai_chat_slot(p) = Some(id);
        }
        self.ai.on_chat = Some(p);
    }

    /// 채팅 탭을 그리기 직전(세션은 `ai_follow_view`가 탭 열쇠로 맞춘다): 탭 제목을 세션 제목에 맞추고 표식.
    pub(crate) fn ai_chat_prepare(&mut self) {
        self.ai_init();
        self.ai_view.set_side_mode(false);
        if let Some((id, prov, title)) = self
            .ai_cur()
            .map(|s| (s.id.clone(), s.provider, s.title_or_default()))
        {
            self.editors
                .set_view_title(&Self::ai_view_key(prov, &id), &title);
        }
        if !self.ai.host.as_ref().is_some_and(|h| h.busy) {
            self.ai_chat_mark();
        }
    }

    /// 분할 창을 그리기 직전: 초기화·세션이 없으면 새 세션(설정 `ai.side=on`으로 시작했을 때 칩이 비던 관찰 · 11-11).
    pub(crate) fn ai_side_prepare(&mut self) {
        self.ai_init();
        self.ai_view.set_side_mode(true);
        // 탭이 바뀌면(또는 채팅 탭에서 넘어왔으면) 그 탭이 쓰던 세션으로(없으면 새 세션) — 턴이 도는 동안은 바꾸지 않는다.
        let tid = self.editors.active_id();
        let busy = self.ai.host.as_ref().is_some_and(|h| h.busy);
        let from_chat = self.ai.on_chat.is_some();
        if !busy {
            self.ai.on_chat = None;
        }
        if (self.ai.side_tab != Some(tid) || from_chat) && !busy {
            // 떠나는 탭의 세션 기록은 분할 창끼리 옮길 때만(채팅 탭 세션을 엉뚱한 탭에 묶지 않게).
            if let (false, Some(old), Some(sid)) = (
                from_chat,
                self.ai.side_tab,
                self.ai_cur().map(|s| s.id.clone()),
            ) {
                if old != tid {
                    self.ai.tab_sessions.insert(old, sid);
                }
            }
            self.ai.side_tab = Some(tid);
            let found = self
                .ai
                .tab_sessions
                .get(&tid)
                .and_then(|id| self.ai.sessions.iter().position(|s| s.id == *id));
            match found {
                Some(i) => {
                    let prov = self.ai.sessions[i].provider;
                    if prov != self.ai.tab {
                        self.ai_switch_tab(prov);
                    }
                    if self.ai.cur != Some(i) {
                        self.ai_switch_cur(i);
                        self.ai.approval = None;
                        self.ai.pending = None;
                        self.ai.objbk_wait = false;
                        self.ai_view.reset();
                    }
                }
                None => self.ai_new_session(),
            }
            if let Some(s) = self.ai_cur() {
                self.ai.tab_sessions.insert(tid, s.id.clone());
            }
            self.ai_view.set_provider(self.ai.tab);
            self.ai_sync_panel();
        }
        if self.ai.cur.is_none() {
            self.ai_new_session();
            self.ai_view.set_provider(self.ai.tab);
        }
        // 분할 창에서 새 세션·세션 선택으로 바뀐 것도 이 탭에 묶는다(종전엔 탭을 바꿀 때만 기록해 [+] 새 세션이 빠졌다).
        if !busy {
            if let Some(s) = self.ai_cur() {
                self.ai.tab_sessions.insert(tid, s.id.clone());
            }
        }
    }

    /// 편집기 옆 분할 창 토글(툴바 `ai.side` · 팔레트) — 켜면 현재 탭의 세션(없으면 새로)을 그 창에.
    pub(crate) fn ai_toggle_side(&mut self) {
        if !self.ai_enabled() {
            let _ = self.settings.set("ai.enabled", "on");
            self.persist_settings();
            self.ai_apply_enabled();
        }
        self.ai_init();
        // 이 편집기 탭만 토글(다른 탭은 각자 켠다).
        let tid = self.editors.active_id();
        self.ai.side = !self.ai.side_tabs.contains(&tid);
        if self.ai.side {
            self.ai.side_tabs.insert(tid);
        } else {
            self.ai.side_tabs.remove(&tid);
        }
        let _ = self
            .settings
            .set("ai.side", if self.ai.side { "on" } else { "off" });
        self.persist_settings();
        if self.ai.side {
            if self.ai.cur.is_none() {
                self.ai_new_session();
            }
            self.ai_view.set_provider(self.ai.tab);
            self.ai_view.show_page(Page::Chat);
            self.ai.side_focus = true;
            self.set_focus(Focus::Editor);
            self.ai_view.set_focused(true);
        } else {
            self.ai.side_focus = false;
            self.ai_view.set_focused(false);
            self.set_focus(Focus::Editor);
        }
        self.ai_sync_panel();
        self.layout();
        self.redraw();
    }

    /// 설정 `ai.enabled` 적용(활동 막대 항목 · 끄면 패널·호스트 닫기).
    pub(crate) fn ai_apply_enabled(&mut self) {
        let on = self.ai_enabled();
        self.act_bar.set_item_visible("view.ai", on);
        self.act_bar.set_item_visible("view.ai_codex", on);
        if on {
            self.ai_init();
        } else {
            if self.ai_panel.is_visible() {
                self.ai_panel.set_visible(false);
                if self.focus == Focus::Ai {
                    self.set_focus(Focus::Editor);
                }
            }
            if let Some(h) = &mut self.ai.host {
                h.stop();
            }
            self.ai.mcp = None;
        }
        self.layout();
        self.redraw();
    }

    /// 메뉴 `view.ai`(Claude) · `view.ai_codex`(Codex) — 그 탭의 패널 토글(열면 채팅 탭도 앞으로 · 다른 탭이 열려 있으면 전환).
    pub(crate) fn ai_toggle_panel(&mut self, p: Provider) {
        if !self.ai_enabled() {
            let _ = self.settings.set("ai.enabled", "on");
            self.persist_settings();
            self.ai_apply_enabled();
        }
        self.ai_init();
        let on = !(self.ai_panel.is_visible() && self.ai.tab == p);
        if on {
            self.ai_switch_tab(p);
            self.side_panel_close_others(p.act_id());
        }
        self.ai_panel.set_visible(on);
        if on {
            self.ai_codex_login_probe();
            self.ai_sync_panel();
            self.set_focus(Focus::Ai);
            // ★ 활동 막대는 패널만 연다(사용자 11-12 "탭이 열리며 쓰던 편집창이 바뀌어 불편") — 탭은 [⊕ 새 대화]·세션 목록에서.
        } else if self.focus == Focus::Ai {
            self.set_focus(Focus::Editor);
        }
        self.layout();
        self.redraw();
    }

    /// 현재 세션의 채팅 탭을 **고정 탭**으로 열기/앞으로(새 대화 · 팔레트 · 기동 명령).
    pub(crate) fn ai_open_chat(&mut self) {
        self.ai_init();
        if self.ai.cur.is_none() {
            self.ai_new_session();
        }
        let Some((id, prov, title)) = self
            .ai_cur()
            .map(|s| (s.id.clone(), s.provider, s.title_or_default()))
        else {
            return;
        };
        self.ai_open_chat_tab(&id, prov, &title, true);
    }

    /// 세션 하나를 뷰 탭으로(미리보기 = 1클릭 · 고정 = 더블클릭/새 대화 · 편집기 탭 규칙 · 11-12).
    fn ai_open_chat_tab(&mut self, id: &str, prov: Provider, title: &str, pinned: bool) {
        self.ai_chat_mark();
        self.editors
            .open_view_tab_preview(&Self::ai_view_key(prov, id), title, pinned);
        self.ai_view.show_page(Page::Chat);
        self.set_focus(Focus::Editor);
        self.ai_view.set_focused(true);
        self.layout();
        self.redraw();
    }

    fn ai_wake(&self) -> ai::host::Wake {
        let proxy = std::sync::Mutex::new(self.wake_proxy.clone());
        std::sync::Arc::new(move || {
            if let Ok(p) = proxy.lock() {
                let _ = p.send_event(Wake);
            }
        })
    }

    fn ai_host(&mut self) -> &mut Host {
        if self.ai.host.is_none() {
            let wake = self.ai_wake();
            self.ai.host = Some(Host::new(wake));
        }
        self.ai.host.as_mut().expect("host")
    }

    /// ★ 제공자별 설정 키(사용자 11-11 "Claude · Codex 설정을 따로"): Claude = `ai.<이름>` · Codex = `ai.codex_<이름>`.
    fn ai_pref_key(&self, name: &str) -> String {
        match self.ai.tab {
            Provider::Claude => format!("ai.{name}"),
            Provider::Codex => format!("ai.codex_{name}"),
        }
    }

    fn ai_mode_default(&self) -> Mode {
        let k = self.ai_pref_key("mode");
        Mode::parse(self.settings.get(&k).unwrap_or("api"))
    }

    fn ai_effort_default(&self) -> String {
        let k = self.ai_pref_key("effort");
        self.settings
            .get(&k)
            .unwrap_or(ai::DEFAULT_EFFORT)
            .to_string()
    }

    fn ai_auto_default(&self) -> bool {
        let k = self.ai_pref_key("auto");
        self.settings.flag(&k)
    }

    fn ai_provider_default(&self) -> Provider {
        Provider::parse(self.settings.get("ai.provider").unwrap_or("claude"))
    }

    /// 제공자별 기본 모델 설정 키.
    fn ai_model_default(&self, p: Provider) -> String {
        let key = match p {
            Provider::Claude => "ai.model",
            Provider::Codex => "ai.codex_model",
        };
        self.settings
            .get(key)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(p.default_model())
            .to_string()
    }

    /// 지금 탭의 제공자.
    fn ai_provider(&self) -> Provider {
        self.ai.tab
    }

    /// 제공자의 현재 프로필(저장소 `current`가 다른 제공자면 그 제공자의 첫 프로필).
    fn ai_profile_for(&self, p: Provider) -> Option<Profile> {
        self.ai
            .store
            .current()
            .filter(|q| q.provider == p)
            .or_else(|| self.ai.store.profiles.iter().find(|q| q.provider == p))
            .cloned()
    }

    pub(crate) fn ai_cur(&self) -> Option<&Session> {
        self.ai.cur.and_then(|i| self.ai.sessions.get(i))
    }
    pub(crate) fn ai_cur_mut(&mut self) -> Option<&mut Session> {
        let i = self.ai.cur?;
        self.ai.sessions.get_mut(i)
    }
    /// 턴이 도는 세션(없으면 보이는 세션) — 호스트 이벤트·도구 실행은 여기에.
    fn ai_run_idx(&self) -> Option<usize> {
        self.ai
            .run
            .as_ref()
            .and_then(|id| self.ai.sessions.iter().position(|s| s.id == *id))
            .or(self.ai.cur)
    }
    fn ai_run(&self) -> Option<&Session> {
        self.ai_run_idx().and_then(|i| self.ai.sessions.get(i))
    }
    fn ai_run_mut(&mut self) -> Option<&mut Session> {
        let i = self.ai_run_idx()?;
        self.ai.sessions.get_mut(i)
    }

    /// 새 세션(설정의 모드·모델·노력도 · 현재 프로필) — 맨 앞에 끼운다.
    pub(crate) fn ai_new_session(&mut self) {
        if let Some(h) = &mut self.ai.host {
            h.stop();
            h.reset_api();
        }
        // ★ 빈 세션 재사용(사용자 11-12 "창은 1개인데 세션이 2개 · 삭제도 안 됨"): 대화가 없는 세션이 이미 있으면 새로 만들지 않고
        //   그것으로 간다 — 편집기 탭마다 분할 창이 빈 "(new)"를 하나씩 만들던 것 · 지금 세션을 지우면 곧바로 새 빈 세션이 생겨
        //   "삭제가 안 되는" 것처럼 보이던 것을 함께 없앤다.
        let prov = self.ai.tab;
        if let Some(i) = self
            .ai
            .sessions
            .iter()
            .position(|s| s.provider == prov && s.messages.is_empty())
        {
            if self.ai.cur != Some(i) {
                self.ai_switch_cur(i);
            }
            self.ai.approval = None;
            self.ai.pending = None;
            self.ai.objbk_wait = false;
            self.ai.status.clear();
            self.ai_view.reset();
            self.ai_sync_panel();
            return;
        }
        let mode = self.ai_mode_default();
        let provider = self.ai.tab;
        let prof = self.ai_profile_for(provider);
        let profile = prof.as_ref().map(|p| p.name.clone()).unwrap_or_default();
        let model = prof
            .as_ref()
            .map(|p| p.model.clone())
            .filter(|_| mode == Mode::Api || provider == Provider::Codex)
            .unwrap_or_else(|| self.ai_model_default(provider));
        let effort = self.ai_effort_default();
        let mut s = Session::new(mode, &profile, &model, &effort);
        s.provider = provider;
        s.auto = self.ai_auto_default();
        // 떠나는 세션의 초안 저장(새 세션은 빈 작성 상자로).
        let txt = self.ai_view.composer_text();
        if let Some(old) = self.ai_cur().map(|s| s.id.clone()) {
            if !txt.trim().is_empty() {
                self.ai.drafts.insert(old, txt);
            }
        }
        self.ai.sessions.insert(0, s);
        self.ai.cur = Some(0);
        self.ai_view.set_composer_text("");
        self.ai.approval = None;
        self.ai.pending = None;
        self.ai.objbk_wait = false;
        self.ai.status.clear();
        self.ai_view.reset();
        self.ai_sync_panel();
    }

    /// 이전 세션 열기(이어하기: CLI = `--resume` · API = 대화 재구성) · `pinned` = 고정 탭(아니면 미리보기 탭).
    /// `from_side` = 편집기 옆 분할 창의 [≡] 세션 선택에서 왔다(그 탭의 분할 창 세션을 바꾼다). 활동 막대 패널에서 온 것은
    /// **늘 채팅 탭으로** 연다(1클릭 미리보기 · 더블클릭 고정) — 분할 창의 세션은 건드리지 않는다(사용자 11-12).
    pub(crate) fn ai_open_session(&mut self, id: &str, pinned: bool, from_side: bool) {
        let Some(i) = self.ai.sessions.iter().position(|s| s.id == id) else {
            return;
        };
        let prov = self.ai.sessions[i].provider;
        if prov != self.ai.tab {
            self.ai_switch_tab(prov);
        }
        if let Some(h) = &mut self.ai.host {
            h.stop();
            h.reset_api();
        }
        self.ai.run = None;
        self.ai_switch_cur(i);
        self.ai.approval = None;
        self.ai.pending = None;
        self.ai.objbk_wait = false;
        // API 대화 재구성(글만 · 도구 왕복은 뺀다).
        let msgs: Vec<Json> = self.ai.sessions[i]
            .messages
            .iter()
            .filter_map(|m| match m.kind {
                Kind::User => Some(ai::claude_api::user_message(&m.text, &[])),
                Kind::Assistant if !m.text.is_empty() => Some(Json::Obj(vec![
                    ("role".into(), Json::Str("assistant".into())),
                    (
                        "content".into(),
                        Json::Arr(vec![Json::Obj(vec![
                            ("type".into(), Json::Str("text".into())),
                            ("text".into(), Json::Str(m.text.clone())),
                        ])]),
                    ),
                ])),
                _ => None,
            })
            .collect();
        // 마지막이 user면(답이 없었다) 떼어 낸다 — 다음 보내기의 user와 연속이 되지 않게.
        let mut msgs = msgs;
        while msgs.last().is_some_and(|m| ai::jstr(m, "role") == "user") {
            msgs.pop();
        }
        let h = self.ai_host();
        if let Ok(mut m) = h.api_messages.lock() {
            *m = msgs;
        }
        self.ai_view.reset();
        self.ai_sync_panel();
        if from_side && self.ai.side_rect.w > 0 && !self.ai_view_active() {
            // 분할 창에서 고른 세션 = 이 편집기 탭에 연결.
            if let (Some(tid), Some(s)) = (self.ai.side_tab, self.ai_cur()) {
                self.ai.tab_sessions.insert(tid, s.id.clone());
            }
            self.redraw();
        } else {
            let prov = self.ai.sessions[i].provider;
            let title = self.ai.sessions[i].title_or_default();
            self.ai_open_chat_tab(id, prov, &title, pinned);
        }
    }

    pub(crate) fn ai_delete_session(&mut self, id: &str) {
        let Some(i) = self.ai.sessions.iter().position(|s| s.id == id) else {
            return;
        };
        // 그 세션의 뷰 탭도 닫는다(제공자 둘 다 시도).
        self.editors
            .close_view_tabs(&Self::ai_view_key(Provider::Claude, id));
        self.editors
            .close_view_tabs(&Self::ai_view_key(Provider::Codex, id));
        ai::history_delete(id);
        self.ai.sessions.remove(i);
        match self.ai.cur {
            Some(c) if c == i => {
                if let Some(h) = &mut self.ai.host {
                    h.stop();
                    h.reset_api();
                }
                self.ai.cur = None;
                self.ai_view.reset();
            }
            Some(c) if c > i => self.ai.cur = Some(c - 1),
            _ => {}
        }
        self.ai_sync_panel();
        self.redraw();
    }

    pub(crate) fn ai_sync_panel(&mut self) {
        self.ai_panel
            .set_dblclick_ms(self.settings.int("ui.dblclick_ms").max(0) as u128);
        self.ai_view
            .set_detail(self.settings.get("ai.verbosity") == Some("detail"));
        self.ai_view
            .set_line_links(self.settings.flag("ai.line_links"));
        // 실행 오류 카드의 [AI] 버튼(설정이 바뀌면 바로).
        self.run_toast
            .configure_ai(self.ai_enabled() && self.settings.flag("ai.error_offer"));
        // 꺼져 있으면 같은 오류 카드에 [AI 켜기](사용자 11-13 "동일 토스트에서 켤 수 있게").
        let hint = if self.ai_enabled() && !self.settings.flag("ai.error_offer") {
            t(Msg::AiErrOfferCardOn).to_string()
        } else {
            String::new()
        };
        self.run_toast.configure_ai_hint(&hint);
        self.ai_view
            .set_max_lines(self.settings.int("ai.composer_max_lines") as i32);
        self.ai_view
            .set_dblclick_ms(self.settings.int("ui.dblclick_ms").max(0) as u128);
        let prov = self.ai_provider();
        let mode_now = self
            .ai_cur()
            .map(|s| s.mode)
            .unwrap_or_else(|| self.ai_mode_default());
        // CLI 모드 바닥 줄 = 실행 파일 경로(+ Codex는 `codex login status` 첫 줄).
        let cli_note = if mode_now == Mode::Cli {
            let (key, dflt) = match prov {
                Provider::Codex => ("ai.codex_path", "codex"),
                Provider::Claude => ("ai.cli_path", "claude"),
            };
            let (prog, _) = ai::host::resolve_cli(self.settings.get(key).unwrap_or(dflt));
            let mut n = tf(Msg::AiCliPath, &[&prog]);
            if prov == Provider::Codex {
                if let Some(st) = self.ai.codex_login.as_deref().filter(|s| !s.is_empty()) {
                    n = format!(
                        "{n}
{st}"
                    );
                }
            }
            n
        } else {
            String::new()
        };
        let cur = self.ai_cur();
        let provider = self.ai.tab;
        let profile_name = cur
            .map(|s| s.profile.clone())
            .unwrap_or_else(|| self.ai.store.current.clone());
        let model = cur
            .map(|s| s.model.clone())
            .unwrap_or_else(|| self.ai_model_default(provider));
        let models: Vec<(String, String)> = {
            let fetched = self.ai_session_models(provider, mode_now, &profile_name);
            ai::model_items(provider, &fetched, &model)
        };
        let keyed: Vec<String> = nsql_vault::Vault::open_default()
            .map(|v| v.secret_names())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|n| n.strip_prefix("ai.").map(str::to_string))
            .collect();
        let st = PanelState {
            provider,
            mode: cur
                .map(|s| s.mode)
                .unwrap_or_else(|| self.ai_mode_default()),
            profiles: self
                .ai
                .store
                .profiles
                .iter()
                .filter(|p| p.provider == provider)
                .map(|p| ProfileRow {
                    name: p.name.clone(),
                    desc: format!(
                        "{} · {}",
                        p.model,
                        t(if p.wif.is_some() {
                            Msg::AiAuthWif
                        } else if keyed.contains(&p.name) {
                            Msg::AiKeyStored
                        } else {
                            Msg::AiKeyMissing
                        })
                    ),
                })
                .collect(),
            profile: profile_name,
            model,
            models,
            effort: cur
                .map(|s| s.effort.clone())
                .unwrap_or_else(|| self.ai_effort_default()),
            auto: cur
                .map(|s| s.auto)
                .unwrap_or_else(|| self.ai_auto_default()),
            sessions: self
                .ai
                .sessions
                .iter()
                .filter(|s| s.provider == provider)
                .map(|s| SessionRow {
                    id: s.id.clone(),
                    title: s.title_or_default(),
                    mode: s.mode,
                    model: s.model.clone(),
                    turns: s.messages.iter().filter(|m| m.kind == Kind::User).count(),
                })
                .collect(),
            current: cur.map(|s| s.id.clone()).unwrap_or_default(),
            usage: cur.map(|s| s.usage).unwrap_or_default(),
            busy: self.ai.host.as_ref().is_some_and(|h| h.busy),
            cli_note,
        };
        self.ai_panel.set_state(st);
    }

    pub(crate) fn ai_view_info(&self) -> ViewInfo {
        // 볼트는 프로필 페이지에서만 읽는다(그리기마다 파일을 열지 않게).
        let keyed = if self.ai_view.page() != Page::Chat {
            nsql_vault::Vault::open_default()
                .map(|v| v.secret_names())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|n| n.strip_prefix("ai.").map(str::to_string))
                .collect()
        } else {
            Vec::new()
        };
        let cur_sid = self.ai_cur().map(|s| s.id.clone());
        ViewInfo {
            // 비밀 모양은 가려서 보인다(대기 글은 아직 검사만 통과한 원문).
            queued: self
                .ai
                .send_queue
                .iter()
                .filter(|(sid, _, _)| Some(sid) == cur_sid.as_ref())
                .map(|(_, text, atts)| {
                    // 첨부가 묶인 요청은 앞에 개수를 보인다(`[+2] 글 …`).
                    if atts.is_empty() {
                        redact(text)
                    } else {
                        format!("[+{}] {}", atts.len(), redact(text))
                    }
                })
                .collect(),
            busy: self.ai.host.as_ref().is_some_and(|h| h.busy),
            approval: self
                .ai
                .approval
                .as_ref()
                .map(|(id, tool, sum, _)| (id.clone(), tool.clone(), sum.clone())),
            approving: self
                .ai
                .approving
                .as_ref()
                .map(|(id, ok, _)| (id.clone(), *ok)),
            armed: self.ai.armed.and_then(|at| {
                let wait = self.ai_twice_wait();
                let left = wait.checked_sub(at.elapsed())?;
                let id = self.ai.approval.as_ref()?.0.clone();
                Some((id, left.as_millis() as u64, wait.as_millis() as u64))
            }),
            profiles: self.ai.store.profiles.clone(),
            keyed,
            current_profile: self.ai.store.current.clone(),
            model: self.ai_cur().map(|s| s.model.clone()).unwrap_or_default(),
            effort: self.ai_cur().map(|s| s.effort.clone()).unwrap_or_default(),
            auto: self.ai_cur().is_some_and(|s| s.auto),
            note: self.ai.note.clone(),
            status: self.ai.status.clone(),
            title: self
                .ai_cur()
                .map(|s| s.title_or_default())
                .unwrap_or_default(),
            provider: self.ai.tab,
            stage: self.ai_stage(),
            now_ms: ai::now_ms(),
            activity: self.ai_activity_text(),
            sessions: self
                .ai
                .sessions
                .iter()
                .map(|s| (s.id.clone(), s.title_or_default(), s.provider))
                .collect(),
            models: {
                let (mode, model, profile) = self
                    .ai_cur()
                    .map(|s| (s.mode, s.model.clone(), s.profile.clone()))
                    .unwrap_or((Mode::Api, String::new(), String::new()));
                let fetched = self.ai_session_models(self.ai.tab, mode, &profile);
                ai::model_items(self.ai.tab, &fetched, &model)
            },
        }
    }

    // ───────────────────────── 패널 · 뷰 동작 ─────────────────────────

    pub(crate) fn ai_panel_actions(&mut self) {
        for a in self.ai_panel.take_actions() {
            match a {
                PanelAction::SetMode(m) => {
                    let k = self.ai_pref_key("mode");
                    let _ = self.settings.set(&k, m.as_str());
                    self.persist_settings();
                    if let Some(s) = self.ai_cur_mut() {
                        if s.mode != m {
                            s.mode = m;
                            s.cli_session = None;
                            if let Some(h) = &mut self.ai.host {
                                h.stop();
                            }
                        }
                    }
                    self.ai_codex_login_probe();
                }
                PanelAction::SetProfile(p) => {
                    self.ai.store.current = p.clone();
                    self.ai_store_save();
                    let model = self.ai.store.current().map(|q| q.model.clone());
                    let effort = self.ai.store.current().map(|q| q.effort.clone());
                    if let Some(s) = self.ai_cur_mut() {
                        s.profile = p;
                        if let Some(m) = model {
                            s.model = m;
                        }
                        if let Some(e) = effort {
                            s.effort = e;
                        }
                    }
                }
                PanelAction::SetModel(m) => {
                    let k = self.ai_pref_key("model");
                    let _ = self.settings.set(&k, &m);
                    self.persist_settings();
                    if let Some(s) = self.ai_cur_mut() {
                        s.model = m;
                    }
                }
                PanelAction::SetEffort(e) => {
                    let k = self.ai_pref_key("effort");
                    let _ = self.settings.set(&k, &e);
                    self.persist_settings();
                    if let Some(s) = self.ai_cur_mut() {
                        s.effort = e;
                    }
                }
                PanelAction::SetAuto(on) => {
                    let k = self.ai_pref_key("auto");
                    let _ = self.settings.set(&k, if on { "on" } else { "off" });
                    self.persist_settings();
                    if let Some(s) = self.ai_cur_mut() {
                        s.auto = on;
                    }
                }
                PanelAction::NewChat => {
                    self.ai_new_session();
                    self.ai_open_chat();
                }
                PanelAction::ManageProfiles => {
                    // 목록 페이지로(편집은 행·[편집]·[+]에서 · 사용자 11-11 "목록과 편집 분리").
                    self.ai_open_chat();
                    self.ai_view.show_page(Page::Profiles);
                }
                PanelAction::OpenSession(id) => self.ai_open_session(&id, false, false),
                PanelAction::OpenSessionPinned(id) => self.ai_open_session(&id, true, false),
                PanelAction::RenameSession(id, title) => self.ai_rename_session(&id, &title),
                PanelAction::DeleteSession(id) => self.ai_delete_session(&id),
            }
        }
        self.ai_sync_panel();
        self.redraw();
    }

    pub(crate) fn ai_view_actions(&mut self) {
        for a in self.ai_view.take_actions() {
            match a {
                ViewAction::Send(text) => self.ai_send(&text),
                ViewAction::Stop => self.ai_stop(),
                ViewAction::AttachFile => {
                    self.file_purpose = FilePurpose::AiAttach;
                    self.open_file_dlg = Some(nexa_dlg::PickerMode::Open);
                }
                ViewAction::AttachClipboard => self.ai_attach_clipboard(),
                // 대기 풀의 n번째(이 세션 것만 센 순번) 요청을 지운다.
                ViewAction::Unqueue(n) => {
                    let cur = self.ai_cur().map(|s| s.id.clone());
                    let at = self
                        .ai
                        .send_queue
                        .iter()
                        .enumerate()
                        .filter(|(_, (sid, _, _))| Some(sid) == cur.as_ref())
                        .map(|(i, _)| i)
                        .nth(n);
                    if let Some(i) = at {
                        self.ai.send_queue.remove(i);
                    }
                }
                ViewAction::RemoveAttachment(i) => {
                    if let Some(s) = self.ai_cur_mut() {
                        if i < s.attachments.len() {
                            s.attachments.remove(i);
                        }
                    }
                }
                ViewAction::Approve(_, ok) => self.ai_approve(ok),
                ViewAction::Slash(line) => self.ai_slash(&line),
                ViewAction::CopyText(s) => {
                    let _ = crate::clipboard::write_text(&s);
                    self.sess.status = t(Msg::StCopied).into();
                }
                ViewAction::InsertEditor(s) => self.ai_insert_editor(&s),
                ViewAction::ReplaceEditor(s) => self.ai_replace_code(&s),
                ViewAction::GotoLine(n) => self.ai_goto_line(n),
                ViewAction::ProfileUse(arg) => {
                    if let Some(i) = arg.strip_prefix("edit:") {
                        if let Some(p) = i
                            .parse::<usize>()
                            .ok()
                            .and_then(|i| self.ai.store.profiles.get(i))
                            .cloned()
                        {
                            let has_key = self.ai_key_exists(&p.name);
                            self.ai_view.edit_profile(Some(&p), has_key);
                        }
                    } else if let Some(p) = arg
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| self.ai.store.profiles.get(i))
                        .cloned()
                    {
                        self.ai.store.current = p.name.clone();
                        self.ai_store_save();
                        if let Some(s) = self.ai_cur_mut() {
                            s.profile = p.name.clone();
                            s.model = p.model.clone();
                            s.effort = p.effort.clone();
                        }
                        self.ai.note = tf(Msg::AiProfileInUse, &[&p.name]);
                    }
                }
                ViewAction::ProfileDelete(arg) => {
                    if let Some(p) = arg
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| self.ai.store.profiles.get(i))
                        .cloned()
                    {
                        if let Ok(v) = nsql_vault::Vault::open_default() {
                            let _ = v.delete_secret(&p.secret_name());
                        }
                        self.ai.store.remove(&p.name);
                        self.ai_store_save();
                        self.ai.note = tf(Msg::AiProfileDeleted, &[&p.name]);
                    }
                }
                ViewAction::ProfileSave(mut p, key, prev) => {
                    p.provider = self.ai_provider();
                    let name = p.name.clone();
                    // WIF 프로필은 키가 없다(볼트 봉투 = 로그인 토큰 · [로그인…]이 채운다).
                    let key = if p.wif.is_some() { None } else { key };
                    let renamed = prev.filter(|o| *o != name);
                    let exists = self.ai.store.profiles.iter().any(|q| q.name == name);
                    // 새 프로필이거나 이름을 바꿨는데 그 이름이 이미 있으면 덮어쓰지 않는다.
                    if exists && (renamed.is_some() || self.ai_view.editing_name().is_none()) {
                        self.ai.note = tf(Msg::AiNameTaken, &[&name]);
                        continue;
                    }
                    // ★ 이름 변경 = rename(프로필 · 볼트 비밀 · 세션 참조) — 복제가 아니다(사용자 11-11 · 복제는 [복제]).
                    if let Some(old) = renamed {
                        if let Ok(v) = nsql_vault::Vault::open_default() {
                            if let Some(b) = v.get_secret(&format!("ai.{old}")) {
                                if let Err(e) = v.put_secret(&p.secret_name(), &b) {
                                    self.ai.note = tf(Msg::AiKeySaveFailed, &[&e.to_string()]);
                                    continue;
                                }
                                let _ = v.delete_secret(&format!("ai.{old}"));
                            }
                        }
                        self.ai.store.rename(&old, &name);
                        for s in &mut self.ai.sessions {
                            if s.profile == old {
                                s.profile = name.clone();
                                ai::history_save(s);
                            }
                        }
                    }
                    if let Some(k) = key {
                        match nsql_vault::Vault::open_default()
                            .and_then(|v| v.put_secret(&p.secret_name(), k.as_bytes()))
                        {
                            Ok(()) => {}
                            Err(e) => {
                                self.ai.note = tf(Msg::AiKeySaveFailed, &[&e.to_string()]);
                                continue;
                            }
                        }
                    }
                    self.ai.store.upsert(p);
                    if self.ai.store.current.is_empty() {
                        self.ai.store.current = name.clone();
                    }
                    self.ai_store_save();
                    let has_key = self.ai_key_exists(&name);
                    let p2 = self.ai.store.get_mut(&name).cloned();
                    self.ai_view.edit_profile(p2.as_ref(), has_key);
                    self.ai.note = tf(Msg::AiProfileSaved, &[&name]);
                    // ★ [저장 + 검증] 한 번에(사용자 11-11): 키·토큰이 있으면 바로 검증 → 모델 목록 → 워크스페이스 자동 보정.
                    if let Some(p2) = p2 {
                        if has_key {
                            self.ai_validate(p2, None);
                        }
                    }
                }
                ViewAction::ProfileLogin(p) => self.ai_wif_login(p),
                ViewAction::RunCode { msg, block, sql } => self.ai_run_block(msg, block, &sql),
                ViewAction::RunGrid(sql) => self.ai_run_grid(&sql),
                ViewAction::OpenSession(id) => self.ai_open_session(&id, true, true),
                ViewAction::NewSession(p) => {
                    if p != self.ai.tab {
                        self.ai_switch_tab(p);
                    }
                    self.ai_new_session();
                    self.ai_view.set_provider(self.ai.tab);
                    if let (Some(tid), Some(s)) = (self.ai.side_tab, self.ai_cur()) {
                        self.ai.tab_sessions.insert(tid, s.id.clone());
                    }
                }
                ViewAction::ProfileDup(arg) => {
                    if let Some(p) = arg
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| self.ai.store.profiles.get(i))
                        .cloned()
                    {
                        // "<이름>-copy" · 겹치면 -copy2 …(이름 규칙 61자 안).
                        let base: String = p.name.chars().take(54).collect::<String>() + "-copy";
                        let mut name = base.clone();
                        let mut n = 2;
                        while self.ai.store.profiles.iter().any(|x| x.name == name) {
                            name = format!("{base}{n}");
                            n += 1;
                        }
                        let mut q = p.clone();
                        q.name = name.clone();
                        if let Ok(v) = nsql_vault::Vault::open_default() {
                            if let Some(b) = v.get_secret(&p.secret_name()) {
                                let _ = v.put_secret(&q.secret_name(), &b);
                            }
                        }
                        self.ai.store.upsert(q.clone());
                        self.ai_store_save();
                        let has_key = self.ai_key_exists(&name);
                        self.ai_view.edit_profile(Some(&q), has_key);
                        self.ai.note = tf(Msg::AiProfileDuplicated, &[&name]);
                    }
                }
                ViewAction::ShowPage(pg) => {
                    self.ai_view.show_page(pg);
                    if pg == Page::Chat {
                        self.ai.note.clear();
                    }
                }
            }
        }
        self.ai_sync_panel();
        self.redraw();
    }

    fn ai_store_save(&mut self) {
        if let Some(dir) = ProfileStore::dir() {
            if let Err(e) = self.ai.store.save(&dir) {
                self.sess.status = tf(Msg::AiKeySaveFailed, &[&e.to_string()]);
            }
        }
    }

    fn ai_key_exists(&self, profile: &str) -> bool {
        nsql_vault::Vault::open_default()
            .ok()
            .and_then(|v| v.get_secret(&format!("ai.{profile}")))
            .is_some()
    }

    fn ai_key_for(&self, profile: &str) -> Option<String> {
        let v = nsql_vault::Vault::open_default().ok()?;
        let b = v.get_secret(&format!("ai.{profile}"))?;
        String::from_utf8(b).ok()
    }

    /// ★ WIF 로그인(프로필 페이지 [로그인…] · 110 §15-5): 폼 프로필을 먼저 저장하고 스레드에서 브라우저 로그인 → 토큰은 볼트로 ·
    /// 결과는 Note `wif:ok:<프로필>` / `wif:err:<원인>`(토큰 글자는 어디에도 안 흐른다).
    fn ai_wif_login(&mut self, mut p: Profile) {
        let Some(cfg) = p.wif.clone() else {
            return;
        };
        if p.name.trim().is_empty() {
            self.ai.note = t(Msg::AiKeyMissing).into();
            return;
        }
        p.provider = Provider::Claude;
        self.ai.store.upsert(p.clone());
        if self.ai.store.current.is_empty() {
            self.ai.store.current = p.name.clone();
        }
        self.ai_store_save();
        let timeout = self.settings.int("ai.wif_login_timeout_secs").max(30) as u64;
        self.ai.note = t(Msg::AiWifLoginStarted).into();
        let tx = self.ai_host().tx.clone();
        let wake = self.ai_wake();
        let _ = std::thread::Builder::new()
            .name("ai-wif-login".into())
            .spawn(move || {
                let r = ai::wif::login(&cfg, &p.base_url, &p.workspace, &p.name, timeout, &|url| {
                    crate::open_external(std::path::Path::new(url))
                });
                let _ = tx.send(AiEvent::Note(match r {
                    Ok(()) => format!("wif:ok:{}", p.name),
                    Err(e) => format!("wif:err:{e}"),
                }));
                wake();
            });
    }

    /// ★ Codex CLI 로그인 상태(`codex login status` 첫 줄 · 스레드 · 콘솔 창 없음) — Codex 탭이 CLI 모드일 때 한 번만.
    fn ai_codex_login_probe(&mut self) {
        if self.ai.tab != Provider::Codex || self.ai.codex_login.is_some() {
            return;
        }
        let mode_now = self
            .ai_cur()
            .map(|s| s.mode)
            .unwrap_or_else(|| self.ai_mode_default());
        if mode_now != Mode::Cli {
            return;
        }
        self.ai.codex_login = Some(String::new());
        let (prog, prefix) =
            ai::host::resolve_cli(self.settings.get("ai.codex_path").unwrap_or("codex"));
        let tx = self.ai_host().tx.clone();
        let wake = self.ai_wake();
        let _ = std::thread::Builder::new()
            .name("ai-codex-login".into())
            .spawn(move || {
                let mut cmd = nsql_ext::net::quiet_command(&prog);
                cmd.args(&prefix).args(["login", "status"]);
                let line = match cmd.output() {
                    Ok(o) => {
                        let out = String::from_utf8_lossy(&o.stdout).into_owned();
                        let err = String::from_utf8_lossy(&o.stderr).into_owned();
                        out.lines()
                            .chain(err.lines())
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                            .unwrap_or("")
                            .to_string()
                    }
                    Err(e) => e.to_string(),
                };
                let _ = tx.send(AiEvent::Note(format!("codexlogin:{line}")));
                wake();
                // 그 계정이 쓸 수 있는 모델 목록(키 없이 = CLI 로그인) → 모델 팝업·자동 이름의 기준.
                if let Ok(models) = ai::host::codex_list_models(&prog, &prefix, "") {
                    let _ = tx.send(AiEvent::Note(format!("codexmodels:{}", models.join(","))));
                    wake();
                }
            });
    }

    /// ★ 코드 블록 [실행](사용자 11-11): 보이는 SQL을 사용자가 직접 실행하는 것과 같다 — `gate_pass()` → `run_text_whole`(DR-34) ·
    /// 결과는 그 블록 아래에 요약(조회 = 앞 8행 · 그 밖 = 상태 줄). 행 데이터는 화면에만(AI로 보내지 않는다).
    /// ★ 답의 코드 블록 실행(▶ · ▦ · 자동 표시)의 실행 길(협업 V1 11-14): 클라이언트 명령(`EXEC :V := …` · `VAR` · `DEFINE` · `PRINT`)이
    /// 들었거나 문장이 여럿이면 편집기와 같은 문장 단위 실행 · 아니면 통째(한 단위) — 종전에는 늘 통째로 보내 변수 대입이
    /// SQL 그대로 서버에 나가 문법 오류가 났다(도구 `run_query`는 이미 같은 판정을 쓴다).
    fn ai_run_code(&mut self, mut src: String) {
        let mut items = nsql_script::split_script_in(&src, Some(self.sess.dialect));
        // 끝에 `;`만 남은 빈 항목은 떼어 낸다(협업 V1 11-14: 답 정리 훅이 새 줄에 붙인 `;`가 문장 단위 실행에서 빈 문장으로 나가
        //   SQLite "not an error" 오류 줄이 됐다 — `EXEC :P := 5` 다음 줄의 `;`).
        while items.len() > 1
            && items
                .last()
                .is_some_and(|i| i.text.trim().trim_matches(';').trim().is_empty())
        {
            if let Some(last) = items.pop() {
                src.truncate(last.span.start.min(src.len()));
            }
        }
        let scripted = items.len() > 1
            || items
                .iter()
                .any(|i| matches!(i.kind, nsql_script::ItemKind::Command(_)));
        if scripted {
            self.run_text(src, 0, true);
        } else {
            self.run_text_whole(src);
        }
    }

    fn ai_run_block(&mut self, msg: usize, block: usize, sql: &str) {
        if !self.sess.connected || !self.gate_open() || self.ai.pending.is_some() {
            self.ai.status = t(Msg::AiQueryBusy).into();
            self.redraw();
            return;
        }
        self.ai.pending = Some(PendingTool {
            reply: None,
            attach_label: None,
            since: Instant::now(),
            max_rows: 8,
            wait_editor: None,
            wait_meta: None,
            wait_ui: None,
            silent: false,
            var_after: None,
        });
        self.ai.run_block = Some((msg, block));
        self.ai.capture = None;
        self.ai.status = t(Msg::AiRunning).into();
        self.ai_run_code(sql.to_string());
    }

    /// 첨부에서 비밀을 가렸으면 Note + 상태줄로 알린다(0이면 조용).
    fn ai_note_masked(&mut self, hidden: usize) {
        if hidden == 0 || !self.settings.flag("ai.secret_warn") {
            return;
        }
        let m = tf(Msg::AiSecretMasked, &[&hidden.to_string()]);
        self.ai_push(ChatMsg::new(Kind::Note, m.clone()));
        self.ai.status = m;
    }

    /// 코드 블록 [그리드에서 결과 보기](사용자 11-11): 보이는 SQL을 보통 실행과 똑같이(`gate_open` → `run_text_whole` · DR-34) —
    /// 결과는 하단 결과 그리드로 · 대본에는 아무것도 안 붙인다 · AI로 안 보낸다.
    fn ai_run_grid(&mut self, sql: &str) {
        // ★ 독립 채팅 탭에서는 편집기 탭이 필요한 동작을 하지 않는다(사용자 11-14 · 버튼도 비활성 — 명령·단축 경로의 이중 안전).
        if self.ai_view_active() {
            self.ai.status = t(Msg::AiNeedsEditorTab).into();
            self.redraw();
            return;
        }
        if !self.sess.connected || !self.gate_open() || self.ai.pending.is_some() {
            self.ai.status = t(Msg::AiQueryBusy).into();
            self.redraw();
            return;
        }
        self.ai.capture = None;
        // ★ 채팅 **뷰 탭**에서 누르면 결과가 뷰 탭의 보이지 않는 그리드로 갔다(협업 V1 관찰 11-12) → 가장 최근 편집기 탭(뷰 아님)으로
        //   먼저 바꾸고 실행(없으면 새 탭). 분할 창 모드는 활성 탭이 편집기라 그대로.
        if self.ai_view_active() {
            let target = self
                .editors
                .tab_history()
                .into_iter()
                .rev()
                .map(|(id, _)| id)
                .find(|id| {
                    self.editors
                        .index_of_id(*id)
                        .is_some_and(|i| self.editors.view_key_of(i).is_none())
                });
            match target.and_then(|id| self.editors.index_of_id(id)) {
                Some(i) => self.editors.switch(i),
                None => self.editors.new_tab(None),
            }
            self.layout();
            self.sync_grid_tab();
        }
        // ★ 활성 결과 탭이 한 번도 실행된 적 없이 비어 있으면(Result 1) 그 탭에 · 이미 결과가 있으면 새 결과 탭에(사용자 11-12).
        let unused = !self.panel.output_active()
            && self.grid.source_sql().trim().is_empty()
            && self.grid.row_count() == 0;
        let fresh_prev = if self.settings.flag("ai.result_tabs") && !unused {
            self.fresh_result_tab_for_run()
        } else {
            None
        };
        self.ai_run_code(sql.to_string());
        self.mark_fresh_run_tab(fresh_prev);
        self.redraw();
    }

    /// ★ 진행 중 상태 줄(사용자 11-11 "Active 상태를 알 수 있게"): 시작 시각 · 경과 · 받은 글자 수.
    fn ai_active_status(&mut self) {
        let Some(t0) = self.ai.turn_started else {
            return;
        };
        let secs = t0.elapsed().as_secs_f64();
        let chars = self
            .ai_run()
            .and_then(|s| s.messages.last())
            .filter(|m| m.kind == Kind::Assistant)
            .map(|m| m.text.chars().count())
            .unwrap_or(0);
        self.ai.status = tf(
            Msg::AiActive,
            &[
                &self.ai.turn_wall,
                &format!("{secs:.0}"),
                &chars.to_string(),
            ],
        );
    }

    /// 턴 진행 중 1초마다 상태 줄 갱신 — 이벤트 루프 마감 계산에 넣는다(돌아올 시각).
    pub(crate) fn ai_tick_due(&mut self, now: Instant) -> Option<Instant> {
        // ★ 처리 중 승인 카드(11-13): 진행 막대가 흐르게 60 ms마다 다시 그리고, 최소 표시 시간(누름이 눈에 보이게)이 지난 뒤
        //   도구 실행·세션 작업이 끝났으면 카드 버튼을 거둔다(허용 = 접속·실행이 끝날 때까지 · 거부 = 최소 시간만).
        let mut anim: Option<Instant> = None;
        if let Some((_, ok, at)) = &self.ai.approving {
            let min = std::time::Duration::from_millis(if *ok { 400 } else { 250 });
            let working = *ok && (self.ai.pending.is_some() || self.sess.busy);
            if now.duration_since(*at) >= min && !working {
                self.ai.approving = None;
            } else {
                anim = Some(now + std::time::Duration::from_millis(60));
            }
            self.redraw();
        }
        // ★ 장전된 허용 버튼(두 번 눌러 허용 · 11-13): 남은 시간이 줄어드는 것이 보이게 다시 그리고, 시간이 지나면 무효로 되돌린다.
        if let Some(at) = self.ai.armed {
            if self.ai.approval.is_none() || now.duration_since(at) >= self.ai_twice_wait() {
                self.ai.armed = None;
            } else {
                let next = now + std::time::Duration::from_millis(33);
                anim = Some(anim.map_or(next, |a| a.min(next)));
            }
            self.redraw();
        }
        if !self.ai.host.as_ref().is_some_and(|h| h.busy) {
            self.ai.last_tick = None;
            return anim;
        }
        let due = self
            .ai
            .last_tick
            .is_none_or(|t| now.duration_since(t) >= std::time::Duration::from_secs(1));
        if due {
            self.ai.last_tick = Some(now);
            self.ai_active_status();
            self.redraw();
        }
        let next = self.ai.last_tick.unwrap_or(now) + std::time::Duration::from_secs(1);
        Some(anim.map_or(next, |a| a.min(next)))
    }

    /// ★ 이 세션이 쓸 수 있는 모델 목록: API = 프로필이 [검증] 때 받은 목록 · Codex CLI = 로그인 계정의 `model/list` · 그 밖 = 빈 목록(내장 표).
    fn ai_session_models(&self, provider: Provider, mode: Mode, profile: &str) -> Vec<String> {
        if provider == Provider::Codex && mode == Mode::Cli {
            return self.ai.codex_cli_models.clone();
        }
        self.ai
            .store
            .profiles
            .iter()
            .find(|p| p.name == profile)
            .map(|p| p.models.clone())
            .unwrap_or_default()
    }

    /// 비용 표(설정 `ai.prices`).
    fn ai_prices(&self) -> String {
        self.settings
            .get("ai.prices")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(nsql_settings::AI_PRICES_DEFAULT)
            .to_string()
    }

    /// [검증] = 키 확인 + 모델 목록 조회(스레드 · 결과는 Note `validate:ok:<프로필>:<모델,…>`): Claude = `/v1/models` ·
    /// Codex = `codex app-server` `model/list`(키가 비면 CLI 자체 로그인 설정일 때만).
    fn ai_validate(&mut self, p: Profile, key: Option<String>) {
        if let Some(cfg) = p.wif.clone() {
            // WIF = 저장된 토큰으로(만료면 갱신) `/v1/models`.
            self.ai.note = t(Msg::AiValidating).into();
            let tx = self.ai_host().tx.clone();
            let wake = self.ai_wake();
            let _ = std::thread::Builder::new()
                .name("ai-validate".into())
                .spawn(move || {
                    let r = ai::wif::ensure_access(&cfg, &p.base_url, &p.workspace, &p.name)
                        .and_then(|tok| {
                            ai::claude_api::list_models(
                                &p.base_url,
                                &ApiAuth::Bearer(tok),
                                &p.workspace,
                            )
                        });
                    let _ = tx.send(AiEvent::Note(match r {
                        Ok(models) => format!("validate:ok:{}:-:{}", p.name, models.join(",")),
                        Err(e) => format!("validate:err:{e}"),
                    }));
                    wake();
                });
            return;
        }
        let is_codex = self.ai_provider() == Provider::Codex;
        let key = key.or_else(|| self.ai_key_for(&p.name));
        let Some(key) = key else {
            self.ai.note = t(Msg::AiKeyMissing).into();
            return;
        };
        let codex_cli = is_codex
            .then(|| ai::host::resolve_cli(self.settings.get("ai.codex_path").unwrap_or("codex")));
        self.ai.note = t(Msg::AiValidating).into();
        let h = self.ai_host();
        let tx = h.tx.clone();
        let wake = self.ai_wake();
        let _ = std::thread::Builder::new()
            .name("ai-validate".into())
            .spawn(move || {
                let mut ws_auto = false;
                let r = match &codex_cli {
                    Some((prog, prefix)) => ai::host::codex_list_models(prog, prefix, &key),
                    None => {
                        let auth = ApiAuth::Key(key.clone());
                        match ai::claude_api::list_models(&p.base_url, &auth, &p.workspace) {
                            // ★ 조직 범위 키 + 워크스페이스 미지정 → Default 워크스페이스로 다시(자동 보정 · 사용자 11-11).
                            Err(e)
                                if p.workspace.is_empty()
                                    && e.contains("not scoped to a workspace") =>
                            {
                                let r2 = ai::claude_api::list_models(&p.base_url, &auth, "default");
                                ws_auto = r2.is_ok();
                                r2
                            }
                            other => other,
                        }
                    }
                };
                let _ = tx.send(AiEvent::Note(match r {
                    Ok(models) => format!(
                        "validate:ok:{}:{}:{}",
                        p.name,
                        if ws_auto { "d" } else { "-" },
                        models.join(",")
                    ),
                    Err(e) => format!("validate:err:{e}"),
                }));
                wake();
            });
    }

    pub(crate) fn ai_insert_editor(&mut self, s: &str) {
        // ★ 독립 채팅 탭에서는 편집기 탭이 필요한 동작을 하지 않는다(사용자 11-14 · 버튼도 비활성 — 명령·단축 경로의 이중 안전).
        if self.ai_view_active() {
            self.ai.status = t(Msg::AiNeedsEditorTab).into();
            self.redraw();
            return;
        }
        if self.ai_view_active() {
            // 채팅 탭 자신에게는 넣을 수 없다 — 새 탭.
            self.editors.new_tab(None);
        }
        // ★ 먼저 편집기로 키 포커스(분할 창이 쥐고 있으면 편집기 TextBox가 비포커스라 `paste`가 버린다 — 캐럿 수정(11-11) 뒤 회귀 ·
        //   사용자 "Insert into editor를 눌러도 붙지 않음"). 삽입 위치 = 그 탭의 캐럿(선택이 있으면 치환).
        self.ai.side_focus = false;
        self.ai_view.set_focused(false);
        self.set_focus(Focus::Editor);
        let mut inv = Invalidations::default();
        self.ed_mut().paste(s, &mut inv);
        self.redraw();
    }

    // ───────────────────────── 보내기 · 멈춤 ─────────────────────────

    pub(crate) fn ai_send(&mut self, text: &str) {
        self.ai_init();
        if self.ai.cur.is_none() {
            self.ai_new_session();
        }
        if ai::contains_secret(text) {
            self.ai_push(ChatMsg::new(Kind::Error, t(Msg::AiSecretBlocked)));
            return;
        }
        // ★ 답하는 중에 보낸 요청은 **줄을 세운다**(사용자 11-14 "전송 기록에는 남지만 실제 AI에 전달은 되지 않은 것 같다" — 종전에는
        //   상태 글만 바꾸고 버렸는데 입력창은 이미 비워져 글이 사라졌다) → 지금 턴이 끝나면 `ai_drain_send_queue`가 차례로 보낸다.
        //   대본에는 실제로 보낼 때 올린다(턴 중간에 사용자 메시지를 끼우면 API 대화 순서가 깨진다) · ■ = 입력창으로 되돌림.
        if self.ai.host.as_ref().is_some_and(|h| h.busy) {
            // 첨부는 이 요청의 것 — 세션에서 떼어 함께 줄 세운다(입력란 위 칩이 사라지고 풀 줄에 `[+N]`).
            if let Some(s) = self.ai_cur_mut() {
                let (sid, atts) = (s.id.clone(), std::mem::take(&mut s.attachments));
                self.ai.send_queue.push_back((sid, text.to_string(), atts));
            }
            self.ai.status = tf(Msg::AiQueuedNote, &[&self.ai.send_queue.len().to_string()]);
            self.redraw();
            return;
        }
        let (mode, profile) = {
            let s = self.ai_cur().expect("session");
            (s.mode, s.profile.clone())
        };
        // API 모드인데 프로필이 없으면 오류 대신 프로필 폼(키 칸부터)을 연다(사용자 11-11 "직관적으로").
        if mode == Mode::Api && !self.ai.store.profiles.iter().any(|p| p.name == profile) {
            self.ai_view.set_composer_text(text);
            self.ai_view.show_page(Page::Profiles);
            self.ai_view.edit_profile(None, false);
            self.ai.note = t(Msg::AiProfileFirst).into();
            self.ai_sync_panel();
            self.redraw();
            return;
        }
        // ★ 자동 모델 이름(opus · sonnet · sol …)은 여기서 가장 새 id로 푼다 — 기준 = 이 세션이 쓸 수 있는 목록
        //   (API = 프로필이 검증 때 받은 목록 · Codex CLI = 로그인 계정의 model/list · 사용자 11-11).
        let provider = self.ai_provider();
        let fetched: Vec<String> = self.ai_session_models(provider, mode, &profile);
        let (attachments, model, effort, cli_session) = {
            let s = self.ai_cur_mut().expect("session");
            (
                std::mem::take(&mut s.attachments),
                ai::resolve_model(provider, &s.model, &fetched),
                s.effort.clone(),
                s.cli_session.clone(),
            )
        };
        self.ai.run = self.ai_cur().map(|s| s.id.clone());
        if let Some(s) = self.ai_cur().filter(|s| s.messages.is_empty()) {
            let (sid, tid) = (s.id.clone(), self.editors.active_id());
            self.ai.tab_sessions.retain(|t, id| *t == tid || *id != sid);
        }
        let mut um = ChatMsg::new(Kind::User, text);
        um.attachments = attachments.iter().map(|a| a.label().to_string()).collect();
        self.ai_push(um);
        self.ai.turn_started = Some(Instant::now());
        self.ai.turn_wall = {
            let st = nsql_log::now_local().stamp();
            st.get(11..19).unwrap_or(&st).to_string()
        };
        self.ai.cli_turn_usage = None;
        let agent = self
            .ai_cur()
            .map(|s| s.agent.clone())
            .unwrap_or_else(|| "assistant".into());
        let system = ai::tools::system_prompt(
            &format!("{:?}", self.sess.dialect),
            &self.sess.profile,
            &agent,
        );
        if std::env::var_os("NSQL_AI_MOCK").is_some() {
            let prices = self.ai_prices();
            self.ai_host().start_mock(text, prices, model.clone());
            self.ai_after_send();
            return;
        }
        if self.ai_provider() == Provider::Codex {
            self.ai_send_codex(
                mode,
                text,
                &attachments,
                &profile,
                &model,
                &effort,
                cli_session,
                &system,
            );
            self.ai_after_send();
            return;
        }
        match mode {
            Mode::Api => {
                let Some(p) = self
                    .ai
                    .store
                    .profiles
                    .iter()
                    .find(|p| p.name == profile)
                    .cloned()
                else {
                    self.ai_push(ChatMsg::new(Kind::Error, t(Msg::AiNoProfile)));
                    return;
                };
                let (auth, wif) = if let Some(cfg) = p.wif.clone() {
                    if ai::wif::tokens_load(&p.name).is_none() {
                        self.ai_push(ChatMsg::new(
                            Kind::Error,
                            tf(Msg::AiWifNotSignedIn, &[&p.name]),
                        ));
                        return;
                    }
                    (ApiAuth::Bearer(String::new()), Some((cfg, p.name.clone())))
                } else {
                    let Some(key) = self.ai_key_for(&p.name) else {
                        self.ai_push(ChatMsg::new(Kind::Error, tf(Msg::AiNoKeyFor, &[&p.name])));
                        return;
                    };
                    (ApiAuth::Key(key), None)
                };
                let launch = ApiLaunch {
                    base_url: p.base_url.clone(),
                    auth,
                    wif,
                    workspace: p.workspace,
                    model,
                    effort,
                    system,
                    timeout_secs: self.settings.int("ai.timeout_secs").max(10) as u64,
                    max_tokens: self.settings.int("ai.max_tokens").max(256) as u64,
                    prices: self.ai_prices(),
                };
                if let Err(e) = self.ai_host().start_api(launch, text, &attachments) {
                    self.ai_push(ChatMsg::new(Kind::Error, e));
                }
            }
            Mode::Cli => {
                if let Err(e) = self.ai_cli_ensure(
                    &profile,
                    &model,
                    &effort,
                    cli_session.as_deref(),
                    &system,
                    false,
                ) {
                    self.ai_push(ChatMsg::new(Kind::Error, e));
                    return;
                }
                // 글 첨부는 본문에 · 이미지는 블록으로(CLI stream-json).
                let mut body = String::new();
                let mut images: Vec<(String, Vec<u8>)> = Vec::new();
                for a in &attachments {
                    match a {
                        Attachment::Text { label, body: b } => {
                            body.push_str(&format!(
                                "<attachment name=\"{label}\">\n{b}\n</attachment>\n"
                            ));
                        }
                        Attachment::Image { media, bytes, .. } => {
                            images.push((media.clone(), bytes.clone()))
                        }
                        Attachment::Pdf { label, .. } => {
                            body.push_str(&format!(
                                "(attachment {label}: PDF is not supported in CLI mode)\n"
                            ));
                        }
                    }
                }
                body.push_str(text);
                if let Err(e) = self.ai_host().send_cli(&body, &images) {
                    self.ai_push(ChatMsg::new(Kind::Error, e));
                }
            }
        }
        self.ai_after_send();
    }

    /// Codex(`codex app-server` · dynamicTools = 우리 도구 표) — API 모드 = 프로필 API 키 로그인 · CLI 모드 = codex CLI 자체
    /// (ChatGPT) 로그인 그대로(개인 사용 전용 · 110 §13-1 · 사용자 11-11 "Codex도 CLI 기능").
    #[allow(clippy::too_many_arguments)]
    fn ai_send_codex(
        &mut self,
        mode: Mode,
        text: &str,
        attachments: &[Attachment],
        profile: &str,
        model: &str,
        effort: &str,
        resume: Option<String>,
        system: &str,
    ) {
        let key = if mode == Mode::Cli {
            String::new()
        } else {
            self.ai_key_for(profile).unwrap_or_default()
        };
        if mode == Mode::Api && key.is_empty() {
            self.ai_push(ChatMsg::new(Kind::Error, tf(Msg::AiNoKeyFor, &[profile])));
            return;
        }
        // ★ CLI 로그인 계정(Free 등)이 못 쓰는 모델이면 계정 기본 모델로 바꿔 보내고 안내(사용자 11-11).
        let model_owned;
        let model: &str = if mode == Mode::Cli
            && !self.ai.codex_cli_models.is_empty()
            && !self.ai.codex_cli_models.iter().any(|m| m == model)
        {
            model_owned = self.ai.codex_cli_models[0].clone();
            self.ai_push(ChatMsg::new(
                Kind::Note,
                tf(Msg::AiModelSwapped, &[model, &model_owned]),
            ));
            if let Some(s) = self.ai_cur_mut() {
                s.model = model_owned.clone();
            }
            &model_owned
        } else {
            model
        };
        if let Err(e) = self.ai_codex_ensure(key, model, effort, system, resume, false) {
            self.ai_push(ChatMsg::new(Kind::Error, e));
            return;
        }
        // 글 첨부는 본문에 · 이미지는 임시 파일(`localImage` · 접두 nsql- = tmp-clean 대상).
        let mut body = String::new();
        let mut images: Vec<String> = Vec::new();
        for a in attachments {
            match a {
                Attachment::Text { label, body: b } => {
                    body.push_str(&format!(
                        "<attachment name=\"{label}\">\n{b}\n</attachment>\n"
                    ));
                }
                Attachment::Image { bytes, media, .. } => {
                    let ext = if media.contains("png") { "png" } else { "jpg" };
                    let p = std::env::temp_dir().join(format!("nsql-ai-{}.{ext}", ai::stamp_id()));
                    if std::fs::write(&p, bytes).is_ok() {
                        images.push(p.to_string_lossy().into_owned());
                    }
                }
                Attachment::Pdf { label, .. } => {
                    body.push_str(&format!(
                        "(attachment {label}: PDF is not supported with Codex)\n"
                    ));
                }
            }
        }
        body.push_str(text);
        if let Err(e) = self.ai_host().send_codex(&body, &images) {
            self.ai_push(ChatMsg::new(Kind::Error, e));
        }
    }

    /// 띄우기 서명(미리 띄운 프로세스가 지금 보낼 조건과 같은가): 제공자 · 세션 · 모델 · 노력도 · 시스템 프롬프트.
    fn ai_launch_sig(&self, model: &str, effort: &str, system: &str) -> String {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        system.hash(&mut h);
        let sid = self.ai_cur().map(|s| s.id.clone()).unwrap_or_default();
        format!(
            "{:?}|{sid}|{model}|{effort}|{:x}",
            self.ai_provider(),
            h.finish()
        )
    }

    /// ★ Claude CLI 프로세스 보장(보내기 · 미리 띄우기 공용 · 11-12): 살아 있으면 그대로 — 단 **미리 띄운 뒤 조건이 바뀌었으면**
    /// (모델 · 노력도 · 에이전트 · 접속) 내리고 새로 띄운다. `warm` = 미리 띄우기(오류는 돌려주기만 · 대본에 올리지 않는다).
    fn ai_cli_ensure(
        &mut self,
        profile: &str,
        model: &str,
        effort: &str,
        resume: Option<&str>,
        system: &str,
        warm: bool,
    ) -> Result<(), String> {
        let sig = self.ai_launch_sig(model, effort, system);
        if self.ai_host().is_cli_alive() {
            if self.ai.warm.as_ref().is_none_or(|w| *w == sig) {
                return Ok(());
            }
            self.ai_host().stop();
        }
        self.ai.warm = None;
        if self.ai.mcp.is_none() {
            let port = self.settings.int("ai.mcp_port").clamp(0, 65535) as u16;
            let tx = self.ai_host().tx.clone();
            let wake = self.ai_wake();
            match ai::mcp::start(tx, wake, port) {
                Ok(s) => self.ai.mcp = Some(s),
                Err(e) => return Err(format!("mcp: {e}")),
            }
        }
        // ★ 사전 점검(사용자 11-12 "MCP 서버 연결 30초 초과" 오류): 브리지 `nsql` 존재 · 앱 리스너 자기 접속 · 시작 제한 시간은
        //   `MCP_TIMEOUT`(ms · 설정 `ai.mcp_timeout_secs`)으로 CLI에 넘긴다(Claude Code 기본 30초).
        let bridge = ai::mcp::bridge_program();
        if !ai::mcp::bridge_available(&bridge) {
            return Err(tf(Msg::AiMcpBridgeMissing, &[&bridge]));
        }
        if let Some(port) = self.ai.mcp.as_ref().map(|m| m.port) {
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            if std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(2))
                .is_err()
            {
                self.ai.mcp = None;
                return Err(tf(Msg::AiMcpListenerDown, &[&port.to_string()]));
            }
        }
        let cfg = self
            .ai
            .mcp
            .as_ref()
            .map(|m| ai::mcp::config_json(m.port, &m.token));
        let (program, prefix) =
            ai::host::resolve_cli(self.settings.get("ai.cli_path").unwrap_or("claude"));
        // ★ `ai.cli_key=profile`: CLI 모드에서도 현재 프로필의 API 키를 환경 변수로만 넘긴다(CLI 로그인 없이 · 사용자 11-11).
        let mut env: Vec<(String, String)> = Vec::new();
        if self.settings.get("ai.cli_key") == Some("profile") {
            if let Some(k) = self.ai_key_for(profile) {
                env.push(("ANTHROPIC_API_KEY".into(), k));
            }
        }
        // ★ `--bare`는 Claude Code의 자체 로그인(구독)도 건너뛴다 → 키를 넘길 때만(사용자 11-11 "터미널은 로그인돼 있는데").
        let bare = self.settings.flag("ai.bare") && !env.is_empty();
        let mcp_ms = (self.settings.int("ai.mcp_timeout_secs").clamp(10, 600) * 1000).to_string();
        env.push(("MCP_TIMEOUT".into(), mcp_ms.clone()));
        env.push(("MCP_TOOL_TIMEOUT".into(), mcp_ms));
        let args = ai::claude_cli::args(
            model,
            effort,
            self.settings.get("ai.permission_mode").unwrap_or("default"),
            cfg.as_deref(),
            resume,
            bare,
            system,
        );
        let launch = CliLaunch {
            program,
            prefix,
            args,
            env,
        };
        self.ai_host()
            .start_cli(launch)
            .map_err(|e| tf(Msg::AiCliStartFailed, &[&e]))?;
        if warm {
            self.ai.warm = Some(sig);
        }
        Ok(())
    }

    /// ★ Codex app-server 프로세스 보장(보내기 · 미리 띄우기 공용 · 11-12) — 규칙은 [`Self::ai_cli_ensure`]와 같다.
    fn ai_codex_ensure(
        &mut self,
        key: String,
        model: &str,
        effort: &str,
        system: &str,
        resume: Option<String>,
        warm: bool,
    ) -> Result<(), String> {
        let sig = self.ai_launch_sig(model, effort, system);
        if self.ai_host().is_codex_alive() {
            if self.ai.warm.as_ref().is_none_or(|w| *w == sig) {
                return Ok(());
            }
            self.ai_host().stop();
        }
        self.ai.warm = None;
        let (program, prefix) =
            ai::host::resolve_cli(self.settings.get("ai.codex_path").unwrap_or("codex"));
        let cwd = ProfileStore::dir()
            .map(|d| d.join("codex-cwd"))
            .map(|d| {
                let _ = std::fs::create_dir_all(&d);
                d.to_string_lossy().into_owned()
            })
            .unwrap_or_else(|| ".".into());
        let launch = CodexLaunch {
            program,
            prefix,
            api_key: key,
            model: model.to_string(),
            effort: effort.to_string(),
            cwd,
            system: system.to_string(),
            resume,
        };
        self.ai_host()
            .start_codex(launch)
            .map_err(|e| tf(Msg::AiCliStartFailed, &[&e]))?;
        if warm {
            self.ai.warm = Some(sig);
        }
        Ok(())
    }

    /// ★ 미리 띄우기(사용자 11-12 "첫 응답속도를 최소화" · 설정 `ai.prewarm`): CLI 모드 세션에서 작성 상자에 글을 치기 시작하면
    /// CLI 프로세스를 먼저 띄워 둔다(Claude = 도구 서버 연결까지 · Codex = 악수·스레드까지) — 실측 첫 응답 8.3초 → 4.4초.
    /// 세션당 한 번만 시도 · 실패는 조용히(보낼 때 다시 띄우며 오류를 보인다) · 요청은 보내지 않으므로 사용량은 0.
    pub(crate) fn ai_prewarm(&mut self) {
        if !self.settings.flag("ai.prewarm") || std::env::var_os("NSQL_AI_MOCK").is_some() {
            return;
        }
        if self.ai.host.as_ref().is_some_and(|h| h.busy) {
            return;
        }
        let Some((sid, mode, profile)) = self
            .ai_cur()
            .map(|s| (s.id.clone(), s.mode, s.profile.clone()))
        else {
            return;
        };
        if mode != Mode::Cli || self.ai.warm_sid.as_deref() == Some(sid.as_str()) {
            return;
        }
        let draft = self.ai_view.composer_text();
        let draft = draft.trim();
        if draft.is_empty() || draft.starts_with('/') {
            return;
        }
        self.ai.warm_sid = Some(sid.clone());
        let provider = self.ai_provider();
        let alive = if provider == Provider::Codex {
            self.ai_host().is_codex_alive()
        } else {
            self.ai_host().is_cli_alive()
        };
        if alive {
            return;
        }
        let fetched = self.ai_session_models(provider, mode, &profile);
        let (model, effort, resume, agent) = {
            let s = self.ai_cur().expect("session");
            (
                ai::resolve_model(provider, &s.model, &fetched),
                s.effort.clone(),
                s.cli_session.clone(),
                s.agent.clone(),
            )
        };
        let system = ai::tools::system_prompt(
            &format!("{:?}", self.sess.dialect),
            &self.sess.profile,
            &agent,
        );
        let r = if provider == Provider::Codex {
            // 보내기와 같은 모델 보정(계정이 못 쓰는 모델 → 계정 첫 모델 · 안내는 보낼 때).
            let models = &self.ai.codex_cli_models;
            let model = if !models.is_empty() && !models.contains(&model) {
                models[0].clone()
            } else {
                model
            };
            self.ai_codex_ensure(String::new(), &model, &effort, &system, resume, true)
        } else {
            self.ai_cli_ensure(&profile, &model, &effort, resume.as_deref(), &system, true)
        };
        if r.is_ok() {
            // 악수 때 오는 세션 id(Codex 스레드)가 이 세션에 기록되게.
            self.ai.run = Some(sid);
        } else if std::env::var_os("NSQL_TRACE_AI").is_some() {
            eprintln!("[ai] prewarm failed: {:?}", r.err());
        }
    }

    fn ai_after_send(&mut self) {
        self.ai.warm = None;
        self.ai_active_status();
        if let Some(s) = self.ai_cur() {
            ai::history_save(s);
        }
        self.ai_view.reset_scroll_to_bottom();
        self.ai_sync_panel();
        self.redraw();
    }

    /// 대기 줄의 다음 요청을 보낸다(11-14) — 어시스턴트가 한가하고(턴 끝 · 도구·승인·자동 표시 없음) 맨 앞 요청의 세션이 지금
    /// 보이는 세션일 때만. 없어진 세션의 요청은 버린다. 보냈으면 참.
    fn ai_drain_send_queue(&mut self) -> bool {
        if self.ai.send_queue.is_empty()
            || self.ai.host.as_ref().is_some_and(|h| h.busy)
            || self.ai.pending.is_some()
            || !self.ai.tool_queue.is_empty()
            || self.ai.approval.is_some()
            || self.ai.auto_show.is_some()
            || self.ai.auto_run.is_some()
            || self.sess.busy
        {
            return false;
        }
        let sessions: Vec<String> = self.ai.sessions.iter().map(|s| s.id.clone()).collect();
        self.ai
            .send_queue
            .retain(|(sid, _, _)| sessions.contains(sid));
        let cur = self.ai_cur().map(|s| s.id.clone());
        match self.ai.send_queue.front() {
            Some((sid, _, _)) if Some(sid) == cur.as_ref() => {}
            _ => return false,
        }
        let Some((_, text, atts)) = self.ai.send_queue.pop_front() else {
            return false;
        };
        // 이 요청에 묶인 첨부만 보낸다 — 그 사이 입력란에 새로 붙인 첨부(다음 요청 몫)는 잠시 빼 두었다가 되돌린다.
        let held = self
            .ai_cur_mut()
            .map(|s| std::mem::replace(&mut s.attachments, atts))
            .unwrap_or_default();
        self.ai_send(&text);
        if let Some(s) = self.ai_cur_mut() {
            s.attachments.extend(held);
        }
        true
    }

    pub(crate) fn ai_stop(&mut self) {
        // ★ ■: 줄 세운 요청은 보내지 않고 **입력창으로 되돌린다**(쓰던 글이 있으면 그 뒤에 덧붙인다 · 글이 사라지지 않게).
        if !self.ai.send_queue.is_empty() {
            let mut back: Vec<String> = Vec::new();
            let cur = self.ai_view.composer_text();
            if !cur.trim().is_empty() {
                back.push(cur);
            }
            let queued: Vec<_> = self.ai.send_queue.drain(..).collect();
            for (sid, text, atts) in queued {
                back.push(text);
                // 묶여 있던 첨부도 그 세션의 입력란으로 되돌린다.
                if let Some(s) = self.ai.sessions.iter_mut().find(|s| s.id == sid) {
                    s.attachments.extend(atts);
                }
            }
            self.ai_view.set_composer_text(&back.join(
                "
",
            ));
        }
        if let Some(h) = &mut self.ai.host {
            if h.interrupt_codex() {
                h.busy = false;
            } else {
                h.stop();
            }
        }
        if let Some((_, _, _, r)) = self.ai.approval.take() {
            match r {
                ApprovalReply::Cli(tx) => {
                    let _ = tx.send(false);
                }
                ApprovalReply::Api { reply, .. }
                | ApprovalReply::Rows { reply, .. }
                | ApprovalReply::Stage2 { reply, .. } => {
                    let _ = reply.send(ToolReply {
                        output: "cancelled".into(),
                        ok: false,
                    });
                }
            }
        }
        // 게이트 2 준비 중(영향 조회)이던 요청도 취소로 답한다.
        if let Some((_, _, reply, _)) = self.ai.impact_wait.take() {
            let _ = reply.send(ToolReply {
                output: "cancelled".into(),
                ok: false,
            });
        }
        self.ai.last_card = None;
        self.ai.armed = None;
        // ★ ■(중지): 백업 뒤로 미뤄 둔 AI의 실행은 **취소**한다(백업은 버리고 문장을 실행하지 않는다 — 멈춘 뒤에 DROP이 나가면 안 된다).
        let waiting = std::mem::take(&mut self.ai.objbk_wait);
        if waiting {
            self.objbk = None;
        }
        // ★ 대기 중인 도구 호출에는 **멈춘 이유를 답한다**(협업 V1 11-13: 통로를 그냥 닫아 어시스턴트에 "timeout"으로 갔다) —
        //   백업 대기 중이었으면 실행 전에 취소된 것 · 그 밖은 이미 서버로 나갔을 수 있다(되돌리지 않는다 — 재시도 전에 확인하라고 알린다).
        // ★ 어시스턴트가 돌린 문장이 아직 DB에서 도는 중이면 **그 실행도 취소**한다(협업 V1 11-13: ■ 뒤에도 긴 조회가 계속 돌아
        //   세션이 묶였다 · 편집기 ■와 같은 길 `stop_run` — 두 번째 ■ = 강제 중지 규칙도 그대로).
        let cancel_db = self.sess.busy && self.ai.pending.as_ref().is_some_and(|p| p.silent);
        if let Some(mut p) = self.ai.pending.take() {
            if let Some(r) = p.reply.take() {
                let _ = r.send(ToolReply {
                    output: if waiting {
                        "cancelled by the user before it ran - the statement was NOT executed".into()
                    } else {
                        "stopped by the user - the statement may already have been sent to the database; do not retry without checking (get_log) or asking the user".into()
                    },
                    ok: false,
                });
            }
        }
        if cancel_db {
            self.stop_run();
        }
        self.ai.status = t(Msg::AiStopped).into();
        self.ai_push(ChatMsg::new(Kind::Note, t(Msg::AiStopped)));
        self.ai.run = None;
        self.ai_sync_panel();
        self.redraw();
    }

    pub(crate) fn ai_push(&mut self, m: ChatMsg) {
        if let Some(s) = self.ai_run_mut() {
            s.messages.push(m);
        }
    }

    /// ★ 자동 표시한 최종 조회문이 오류로 끝났다(11-12 검증 하네스): 훅이 포맷으로 바꾼 문장이었으면 답을 **원문으로 되돌리고**
    /// 원문을 다시 실행한다(포맷이 문장을 깨뜨린 경우) · 아니면 오류를 대본에 알린다(다음 요청에서 어시스턴트가 `get_log`로 본다).
    fn ai_auto_show_failed(&mut self) {
        let err = redact(&self.sess.status);
        match self.ai.fmt_orig.take() {
            Some((sid, mi, orig)) => {
                let retry = ai::hooks::last_sql_block(&orig).filter(|q| sql_is_readonly(q));
                if let Some(s) = self.ai.sessions.iter_mut().find(|s| s.id == sid) {
                    if let Some(m) = s.messages.get_mut(mi) {
                        m.text = orig;
                    }
                    s.messages
                        .push(ChatMsg::new(Kind::Note, tf(Msg::AiFmtReverted, &[&err])));
                    ai::history_save(s);
                }
                if let Some(sql) = retry {
                    if self.sess.connected && self.gate_open() {
                        self.ai_run_grid(&sql);
                        self.ai.auto_run = Some(Instant::now());
                    }
                }
            }
            None => {
                self.ai_push(ChatMsg::new(
                    Kind::Error,
                    tf(Msg::AiAutoShowFailed, &[&err]),
                ));
            }
        }
        self.redraw();
    }

    // ───────────────────────── 이벤트 수거 ─────────────────────────

    /// 깨어날 때·틱마다 — 호스트 이벤트를 세션에 반영. 바뀌면 true.
    pub(crate) fn ai_poll(&mut self) -> bool {
        let mut changed = false;
        while let Some(ev) = self.ai.host.as_ref().and_then(|h| h.rx.try_recv().ok()) {
            changed = true;
            self.ai_handle(ev);
        }
        if self.ai_tick_pending() {
            changed = true;
        }
        if self.ai_drain_send_queue() {
            changed = true;
        }
        if self.ai.pending.is_none() && !self.sess.busy {
            if let Some((name, input, reply)) = self.ai.tool_queue.pop_front() {
                self.ai_exec_tool(&name, &input, reply);
                changed = true;
            } else if let Some(sql) = self.ai.auto_show.take() {
                // 턴 끝 자동 표시: 접속돼 있고 문지기가 열려 있을 때만(막혔으면 조용히 건너뜀 · ▦로 수동 실행 가능).
                if self.sess.connected && self.gate_open() {
                    self.ai_run_grid(&sql);
                    self.ai.auto_run = Some(Instant::now());
                    changed = true;
                } else {
                    self.ai.fmt_orig = None;
                }
            } else if self
                .ai
                .auto_run
                .is_some_and(|t| t.elapsed() > std::time::Duration::from_millis(300))
            {
                self.ai.auto_run = None;
                if self.sess.run_had_err {
                    self.ai_auto_show_failed();
                } else {
                    self.ai.fmt_orig = None;
                }
                changed = true;
            }
        }
        if changed {
            self.ai_sync_panel();
        }
        changed
    }

    fn ai_handle(&mut self, ev: AiEvent) {
        match ev {
            AiEvent::Started { cli_session, note } => {
                if let Some(s) = self.ai_run_mut() {
                    if cli_session.is_some() {
                        s.cli_session = cli_session;
                    }
                }
                if std::env::var_os("NSQL_TRACE_AI").is_some() {
                    eprintln!("[ai] started {note}");
                }
                if note.contains("connected") {
                    let port = self.ai.mcp.as_ref().map(|m| m.port).unwrap_or(0);
                    self.ai.status = tf(Msg::AiMcpConnected, &[&port.to_string()]);
                }
                // ★ MCP 서버가 못 붙었으면(`mcp=[nexa-sql:failed]`) 진단 Note + CLI 프로세스를 내려 다음 보내기에 다시 띄운다(11-12).
                if note.contains("failed") {
                    let port = self.ai.mcp.as_ref().map(|m| m.port).unwrap_or(0);
                    let bridge = ai::mcp::bridge_program();
                    self.ai_push(ChatMsg::new(
                        Kind::Note,
                        tf(Msg::AiMcpFailed, &[&port.to_string(), &bridge, &note]),
                    ));
                    if let Some(h) = &mut self.ai.host {
                        h.stop();
                    }
                }
            }
            AiEvent::TextDelta(t) => {
                let Some(s) = self.ai_run_mut() else { return };
                match s.messages.last_mut() {
                    Some(m) if m.kind == Kind::Assistant => m.text.push_str(&t),
                    _ => s.messages.push(ChatMsg::new(Kind::Assistant, t)),
                }
                self.ai_active_status();
            }
            AiEvent::ToolCall { id, name, input } => {
                let mut m = ChatMsg::new(
                    Kind::ToolCall {
                        name,
                        input: redact(&input),
                    },
                    "",
                );
                m.id = id;
                self.ai_push(m);
            }
            AiEvent::ToolResult {
                id,
                name,
                output,
                ok,
            } => {
                let mut m = ChatMsg::new(Kind::ToolResult { name, ok }, redact(&output));
                m.id = id;
                self.ai_push(m);
            }
            AiEvent::ToolRequest {
                id,
                name,
                input,
                reply,
            } => {
                let _ = id;
                self.ai_tool_request(name, input, reply);
            }
            AiEvent::Approval {
                id,
                tool,
                summary,
                reply,
            } => {
                let auto = self.ai_run().is_some_and(|s| s.auto);
                let short = ai::bridge::tool_short_name(&tool).to_string();
                // ★ 게이트는 한 곳(`ai_tool_request` · guard.rs): 우리 MCP 도구는 CLI 권한 프롬프트에서 통과시키고 실제 호출에서
                //   분류·정책·카드를 거친다(종전에는 여기와 도구 요청에서 카드가 두 번 떴다). 그 밖 도구는 종전대로.
                let ours = short != tool;
                let needs_user = short == "run_query" && !self.ai_sql_is_readonly(&summary);
                if ours || (auto && !needs_user) {
                    let _ = reply.send(true);
                    return;
                }
                // ★ 전 도구 훅(데이터 보호 · 11-12): UPDATE/DELETE에 WHERE가 없으면 승인 카드 첫 줄에 경고.
                let card = if short == "run_query" && sql_dml_without_where(&summary) {
                    format!("{}\n{}", t(Msg::AiNoWhereWarn), redact(&summary))
                } else {
                    redact(&summary)
                };
                let mut m = ChatMsg::new(
                    Kind::Approval {
                        tool: short.clone(),
                    },
                    card,
                );
                m.id = if id.is_empty() { ai::stamp_id() } else { id };
                let mid = m.id.clone();
                self.ai_push(m);
                self.ai.approval = Some((mid, short, summary, ApprovalReply::Cli(reply)));
            }
            AiEvent::Usage(u) => {
                let is_cli = self.ai_run().is_some_and(|s| s.mode == Mode::Cli);
                if is_cli {
                    self.ai.cli_turn_usage = Some(u);
                } else if let Some(s) = self.ai_run_mut() {
                    s.usage.add(u);
                }
            }
            AiEvent::Done { stop, ms } => {
                if let Some(u) = self.ai.cli_turn_usage.take() {
                    let prices = self.ai_prices();
                    if let Some(s) = self.ai_run_mut() {
                        let mut u = u;
                        if u.cost_usd == 0.0 {
                            let m = ai::resolve_model(s.provider, &s.model, &[]);
                            u.cost_usd = Usage::estimate_cost_in(
                                &prices,
                                &m,
                                u.input,
                                u.output,
                                u.cache_read,
                            );
                        }
                        s.usage.add(u);
                    }
                }
                if let Some(h) = &mut self.ai.host {
                    h.busy = false;
                }
                // ★ PostTurn 훅(hooks.rs · 사용자 11-11 "생성 쿼리를 앱 포맷으로 표시 · 출력 간결"): ```sql 블록 = 앱 포매터 · 빈 줄 정리 —
                //   이번 턴의 **모든** 어시스턴트 메시지(마지막 사용자 메시지 뒤 · 협업 관찰 11-12 "도구 앞 메시지는 안 바뀜").
                self.ai.fmt_orig = None;
                {
                    let fmt_on = self.settings.flag("ai.hook_format_sql");
                    let tidy_on = self.settings.flag("ai.hook_tidy");
                    let now = ai::now_ms();
                    let last_user = self
                        .ai_run()
                        .and_then(|s| s.messages.iter().rposition(|m| m.kind == Kind::User))
                        .unwrap_or(0);
                    let targets: Vec<(usize, String)> = self
                        .ai_run()
                        .map(|s| {
                            s.messages
                                .iter()
                                .enumerate()
                                .skip(last_user)
                                .filter(|(_, m)| m.kind == Kind::Assistant)
                                .map(|(i, m)| (i, m.text.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    let engine = self.format_default_engine();
                    for (i, text) in targets {
                        let new = if fmt_on || tidy_on {
                            ai::hooks::post_turn(&text, fmt_on, tidy_on, &mut |sql| {
                                self.format_run(&engine, sql, false).ok()
                            })
                        } else {
                            text.clone()
                        };
                        if new != text {
                            if let Some(s) = self.ai_run_mut() {
                                let sid = s.id.clone();
                                if let Some(m) = s.messages.get_mut(i) {
                                    m.text = new;
                                }
                                // 뒤 메시지가 덮어쓴다 = 마지막 답의 원문만 남는다.
                                self.ai.fmt_orig = Some((sid, i, text));
                            }
                        }
                    }
                    // ★ 최종 조회 자동 표시(사용자 11-12 "최종 쿼리이므로 자동 조회" · `ai.show_result` auto 기본 / manual): 정상으로 끝난
                    //   턴의 마지막 답에 있는 **마지막 ```sql 블록**이 조회문이면 결과 그리드에 실행한다(쓰기 문장은 절대 자동 실행 안 함 ·
                    //   채팅 뷰 탭에서는 탭을 뺏지 않으려고 건너뜀 = ▦ 수동).
                    self.ai.tool_queue.clear();
                    self.ai.auto_run = None;
                    let bad = ["fail", "error", "exit", "interrupt", "cancel", "abort"];
                    let ok_stop = !bad.iter().any(|b| stop.to_ascii_lowercase().contains(b));
                    self.ai.auto_show = None;
                    if ok_stop
                        && self.settings.get("ai.show_result") != Some("manual")
                        && !self.ai_view_active()
                    {
                        let sql = self
                            .ai_run()
                            .and_then(|s| {
                                s.messages
                                    .iter()
                                    .skip(last_user)
                                    .rev()
                                    .find(|m| m.kind == Kind::Assistant)
                            })
                            .and_then(|m| ai::hooks::last_sql_block(&m.text));
                        if let Some(sql) = sql.filter(|q| sql_is_readonly(q)) {
                            self.ai.auto_show = Some(sql);
                        }
                    }
                    // ⑥ 턴 끝 시각: 이번 턴의 메시지(사용자 요청 포함)에 완료 시각을 적는다(요청 줄 = 최종 경과 · 응답 줄 = 완료).
                    if let Some(s) = self.ai_run_mut() {
                        for m in s.messages.iter_mut().skip(last_user) {
                            if m.done_at == 0 {
                                m.done_at = now;
                            }
                        }
                    }
                }
                let elapsed = self
                    .ai
                    .turn_started
                    .take()
                    .map(|t| t.elapsed().as_millis() as u64)
                    .unwrap_or(ms);
                self.ai.status = tf(Msg::AiTurnDone, &[&stop, &elapsed.to_string()]);
                if let Some(s) = self.ai_run() {
                    ai::history_save(s);
                }
                self.ai.run = None;
            }
            AiEvent::Error(e) => {
                let tail = self
                    .ai
                    .host
                    .as_ref()
                    .map(|h| h.stderr_text())
                    .unwrap_or_default();
                let text = if tail.is_empty() {
                    e
                } else {
                    format!("{e}\n{tail}")
                };
                // CLI가 로그인되지 않았으면 할 일을 안내(사용자 11-11 "이미 API로 연결했는데" — CLI 모드는 키를 쓰지 않는다).
                let not_logged = text.contains("Not logged in") || text.contains("/login");
                self.ai_push(ChatMsg::new(Kind::Error, redact(&text)));
                if not_logged {
                    self.ai_push(ChatMsg::new(Kind::Note, t(Msg::AiCliNotLoggedIn)));
                }
            }
            AiEvent::Note(n) => {
                if let Some(rest) = n.strip_prefix("validate:") {
                    self.ai.note = match rest.split_once(':') {
                        Some(("ok", rest)) => {
                            // "<프로필>:<d|->:<모델,…>" — 목록을 프로필에 저장(모델 팝업 선택지) · d = 워크스페이스 Default 자동 보정.
                            let (name, rest) = rest.split_once(':').unwrap_or((rest, ""));
                            let (ws, csv) = rest.split_once(':').unwrap_or(("-", rest));
                            let models: Vec<String> = csv
                                .split(',')
                                .map(str::trim)
                                .filter(|m| !m.is_empty())
                                .map(String::from)
                                .collect();
                            let n = models.len().to_string();
                            let ws_auto = ws == "d";
                            if let Some(p) = self.ai.store.get_mut(name) {
                                if !models.is_empty() {
                                    p.models = models.clone();
                                }
                                if ws_auto {
                                    p.workspace = "default".into();
                                }
                            }
                            self.ai_store_save();
                            if self.ai_view.editing_name() == Some(name) {
                                self.ai_view.set_models(models);
                                if ws_auto {
                                    self.ai_view.set_workspace("default");
                                }
                            }
                            if ws_auto {
                                tf(Msg::AiWorkspaceAuto, &[&n])
                            } else {
                                tf(Msg::AiValidateOk, &[&n])
                            }
                        }
                        // 조직 범위 키(sk-ant-usr-…)는 workspace id가 필수 — 원문 대신 할 일을 안내하고 그 칸에 포커스.
                        Some((_, e)) if e.contains("anthropic-workspace-id") => {
                            self.ai_view.focus_workspace();
                            t(Msg::AiWorkspaceNeeded).to_string()
                        }
                        Some((_, e)) => tf(Msg::AiValidateFailed, &[e]),
                        None => rest.to_string(),
                    };
                } else if let Some(rest) = n.strip_prefix("codexlogin:") {
                    self.ai.codex_login = Some(rest.to_string());
                } else if let Some(rest) = n.strip_prefix("codexmodels:") {
                    self.ai.codex_cli_models = rest
                        .split(',')
                        .map(str::trim)
                        .filter(|m| !m.is_empty())
                        .map(String::from)
                        .collect();
                } else if let Some(rest) = n.strip_prefix("wif:") {
                    self.ai.note = match rest.split_once(':') {
                        Some(("ok", name)) => {
                            let p2 = self.ai.store.get_mut(name).cloned();
                            self.ai_view.edit_profile(p2.as_ref(), true);
                            tf(Msg::AiWifLoginOk, &[name])
                        }
                        Some((_, e)) => tf(Msg::AiWifLoginFailed, &[e]),
                        None => rest.to_string(),
                    };
                } else if n != "cli exited" || self.ai.host.as_ref().is_some_and(|h| h.busy) {
                    self.ai_push(ChatMsg::new(Kind::Note, n));
                }
            }
        }
    }

    /// 승인 카드 응답.
    pub(crate) fn ai_approve(&mut self, ok: bool) {
        // ★ 두 번 눌러 허용(운영의 DB 쓰기 · 11-13): 첫 누름은 장전만 — 제한 시간 안의 두 번째 누름이 실행한다. 장전 직후의
        //   연타(350 ms 안)는 버린다(습관적 더블 클릭이 두 번으로 세어지지 않게) · 시간이 지난 뒤의 누름은 다시 장전.
        if ok
            && matches!(
                self.ai.approval,
                Some((
                    _,
                    _,
                    _,
                    ApprovalReply::Api { twice: true, .. } | ApprovalReply::Stage2 { .. }
                ))
            )
        {
            match self.ai.armed.map(|at| at.elapsed()) {
                Some(e) if e < std::time::Duration::from_millis(350) => return,
                Some(e) if e <= self.ai_twice_wait() => {}
                _ => {
                    self.ai.armed = Some(Instant::now());
                    self.redraw();
                    return;
                }
            }
        }
        self.ai.armed = None;
        let Some((card_id, _, _, r)) = self.ai.approval.take() else {
            return;
        };
        // ★ 누른 뒤 = 처리 중(11-13): 카드 버튼은 비활성 + 진행 막대로 남고 `ai_tick_due`가 일이 끝나면 거둔다.
        self.ai.approving = Some((card_id.clone(), ok, Instant::now()));
        let mut after: Option<(String, String, mpsc::Sender<ToolReply>, bool)> = None;
        let mut reenter: Option<(String, String, mpsc::Sender<ToolReply>)> = None;
        match r {
            ApprovalReply::Cli(tx) => {
                let _ = tx.send(ok);
            }
            ApprovalReply::Api {
                name,
                input,
                reply,
                two,
                rows,
                twice: _,
            } => {
                if ok {
                    // 동의를 겸한 카드(쓰기 문장 한 장) = 허용이 행 데이터 동의이기도 하다.
                    if rows {
                        if let Some(s) = self.ai_run_mut() {
                            s.rows_allowed = true;
                        }
                    }
                    // 영향 확인이 이어지면 같은 카드에 잇는다(새 카드 없음).
                    self.ai.last_card = Some(card_id);
                    after = Some((name, input, reply, two));
                } else {
                    let _ = reply.send(ToolReply {
                        output: "denied by the user".into(),
                        ok: false,
                    });
                }
            }
            ApprovalReply::Stage2 { name, input, reply } => {
                if ok {
                    self.ai.write_ok = Some((name.clone(), input.clone()));
                    self.ai_exec_tool(&name, &input, reply);
                } else {
                    let _ = reply.send(ToolReply {
                        output: "denied by the user at the impact check".into(),
                        ok: false,
                    });
                }
            }
            ApprovalReply::Rows { name, input, reply } => {
                if ok {
                    if let Some(s) = self.ai_run_mut() {
                        s.rows_allowed = true;
                    }
                    // ★ P0(협업 관찰 11-12 · 데이터 보호): 행 데이터 **동의**는 전송 허락일 뿐 — 쓰기 문장 확인(SQL 본문 · ⚠ WHERE)을
                    //   건너뛰고 바로 실행했다(WHERE 없는 DELETE가 동의 카드 하나로 실행됨). 동의 뒤 도구 요청을 **다시 거친다**:
                    //   읽기 전용(SELECT · get_result · explain)이면 동의만으로 실행 · 쓰기면 확인 카드가 한 번 더(협업 관찰 반영).
                    let sql = json_parse(&input)
                        .ok()
                        .map(|v| ai::jstr(&v, "sql"))
                        .unwrap_or_default();
                    if name != "run_query" || sql_is_readonly(&sql) {
                        self.ai_exec_tool(&name, &input, reply);
                    } else {
                        reenter = Some((name, input, reply));
                    }
                } else {
                    let _ = reply.send(ToolReply {
                        output: "the user declined to share row data".into(),
                        ok: false,
                    });
                }
            }
        }
        // 2단계 대상의 첫 누름은 아직 허용이 끝난 것이 아니다(영향 확인이 같은 카드에서 이어진다) — "허용함" 줄은 실행 때.
        if !(ok && after.as_ref().is_some_and(|a| a.3)) {
            self.ai_push(ChatMsg::new(
                Kind::Note,
                t(if ok { Msg::AiAllowed } else { Msg::AiDenied }),
            ));
        }
        if let Some((name, input, reply)) = reenter {
            self.ai_tool_request(name, input, reply);
        }
        if let Some((name, input, reply, two)) = after {
            self.ai_after_card(name, input, reply, two);
        }
        self.redraw();
    }

    // ───────────────────────── 데이터 보호 하네스(guard.rs · docs/110 §17) ─────────────────────────

    /// 두 번 눌러 허용의 제한 시간 = 사람 경로의 운영 확인과 같은 설정(`run.prod_confirm_ms`).
    fn ai_twice_wait(&self) -> std::time::Duration {
        std::time::Duration::from_millis(
            self.settings.int("run.prod_confirm_ms").clamp(500, 30_000) as u64,
        )
    }

    /// ★ 탐색기의 소스 응답을 AI 도구(`object_source` · 탭 없이 읽기)가 가져간다 — 참 = 소비했다(호출자는 탭을 열지 않는다).
    /// 기다리던 출처(스키마 · 이름 · 종류)와 같은 응답만 · 묵은 대기(시간 초과 뒤)는 비우고 보통 길로 보낸다 · 도구 호출이
    /// 그 사이 멈췄으면(■) 글을 버린다(요청하지 않은 탭이 뒤늦게 열리지 않게).
    pub(crate) fn ai_source_take(
        &mut self,
        origin: Option<&crate::explorer::ObjectOrigin>,
        text: &str,
    ) -> bool {
        let Some((schema, name, kind, since)) = self.ai.source_wait.clone() else {
            return false;
        };
        let limit =
            std::time::Duration::from_secs(self.settings.int("ai.timeout_secs").max(10) as u64 + 5);
        if since.elapsed() > limit {
            self.ai.source_wait = None;
            return false;
        }
        let same = origin.is_some_and(|o| {
            o.kind == kind
                && o.name.eq_ignore_ascii_case(&name)
                && o.schema.eq_ignore_ascii_case(&schema)
        });
        if !same {
            return false;
        }
        self.ai.source_wait = None;
        if self.ai_source_waiting() {
            self.ai.source_got = Some((text.to_string(), true));
        }
        // 요청 때의 "소스 읽는 중…" 상태줄을 지운다(탭이 열리지 않으므로 남는다).
        self.sess.status = String::new();
        true
    }

    /// 탐색기 오류가 소스를 기다리는 중에 왔다 → 그 글로 실패를 답한다(토스트·로그는 보통대로).
    pub(crate) fn ai_source_fail(&mut self, body: &str) {
        if self.ai.source_wait.take().is_some() && self.ai_source_waiting() {
            self.ai.source_got = Some((redact(body), false));
        }
    }

    fn ai_source_waiting(&self) -> bool {
        matches!(
            self.ai.pending.as_ref().and_then(|p| p.wait_ui.as_ref()),
            Some(AiWait::Source)
        )
    }

    /// ★ 객체 백업이 끝났다(11-13): `fail` = 실행하지 못한 이유(백업 실패로 막힘 · 재진입이 보호 장치에 막힘) → 대기 중인 도구
    /// 호출에 **실패로** 답한다 · `None` = 재진입 실행이 시작됐다 → 시간 초과를 지금부터 다시 세고 보통 완료 판정에 맡긴다.
    pub(crate) fn ai_objbk_result(&mut self, fail: Option<String>) {
        if !std::mem::take(&mut self.ai.objbk_wait) {
            return;
        }
        match fail {
            Some(why) => {
                if let Some(mut p) = self.ai.pending.take() {
                    if let Some(r) = p.reply.take() {
                        let _ = r.send(ToolReply {
                            output: format!(
                                "not executed: {} (the statement was NOT run - tell the user)",
                                redact(&why)
                            ),
                            ok: false,
                        });
                    }
                }
            }
            None => {
                if let Some(p) = self.ai.pending.as_mut() {
                    p.since = Instant::now();
                }
            }
        }
    }

    /// 지금 세션이 운영 접속인가.
    fn ai_guard_prod(&self) -> bool {
        self.sess.spec.as_ref().and_then(|sp| sp.env) == Some(nsql_script::ConnEnv::Prod)
    }

    /// ★ 승인 카드 머리 줄(사용자 11-12 "뭘, 왜, 어떻게 하려는지 알 수 없다"): 목적(어시스턴트가 적은 한 문장) · 무엇을(문장 종류 ·
    /// 대상 객체) · 어디서(접속 프로필 · 운영 여부). run_query/explain에만.
    fn ai_card_head(&self, name: &str, input: &str) -> Vec<String> {
        let mut out = Vec::new();
        let v = json_parse(input).unwrap_or(Json::Obj(Vec::new()));
        let g = |k: &str| ai::jstr(&v, k);
        let purpose = g("purpose");
        let purpose = if purpose.trim().is_empty() {
            t(Msg::AiCardNoPurpose).to_string()
        } else {
            redact(purpose.trim())
        };
        out.push(tf(Msg::AiCardPurpose, &[&purpose]));
        let env = t(if self.ai_guard_prod() {
            Msg::AiCardEnvProd
        } else {
            Msg::AiCardEnvOther
        });
        // 무엇을: 도구별로 사람이 읽는 한 줄(대상·바뀌는 것).
        let what = match name {
            "run_query" | "explain" => {
                let dialect = self.sess.connected.then_some(self.sess.dialect);
                let (kind, targets) = ai::guard::summarize(&g("sql"), dialect);
                let tg = if targets.is_empty() {
                    "-".to_string()
                } else {
                    targets.join(", ")
                };
                out.push(tf(Msg::AiCardWhat, &[&kind, &tg]));
                out.push(tf(Msg::AiCardWhere, &[&self.sess.profile, env]));
                return out;
            }
            "set_var" => {
                if !g("formula").trim().is_empty() {
                    tf(
                        Msg::AiDoSetVarFormula,
                        &[&g("name"), &redact(&g("formula"))],
                    )
                } else if matches!(g("null").as_str(), "true" | "1") {
                    tf(Msg::AiDoSetVarNull, &[&g("name")])
                } else {
                    tf(Msg::AiDoSetVar, &[&g("name"), &redact(&g("value"))])
                }
            }
            "delete_var" => tf(Msg::AiDoDeleteVar, &[&g("name")]),
            "edit_editor" => match self.ai_edit_plan(input) {
                Ok(p) => tf(
                    Msg::AiDoEditLines,
                    &[&p.a.to_string(), &p.b.to_string(), &p.op],
                ),
                Err(_) => tf(Msg::AiDoOther, &[name]),
            },
            "set_editor_text" => t(match g("mode").as_str() {
                "insert" => Msg::AiDoEditorInsert,
                "new_tab" => Msg::AiDoEditorNewTab,
                _ => Msg::AiDoEditorWhole,
            })
            .to_string(),
            "run_command" => {
                let id = g("id");
                let label = crate::app::commands::PALETTE
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| format!("{}: {}", t(c.menu), t(c.item)))
                    .unwrap_or_else(|| id.clone());
                let line = tf(Msg::AiDoCommand, &[&label, &id]);
                if ai::guard::is_disconnect_command(&id) {
                    // 이 연결을 쓰는 탭 수를 카드에 보인다(허용 = 그 탭들 모두 미연결이 된다).
                    format!(
                        "{line}
{}",
                        tf(
                            Msg::AiDoDisconnectTabs,
                            &[&self.bound_tabs(self.sess.id).to_string(), &self.sess.desc]
                        )
                    )
                } else {
                    line
                }
            }
            "set_setting" => {
                let key = g("key");
                let now = self.settings.get(&key).unwrap_or("").to_string();
                tf(
                    Msg::AiDoSetting,
                    &[&key, &redact(&now), &redact(&g("value"))],
                )
            }
            "connect" => {
                let scope = g("scope");
                let scope = if scope.is_empty() {
                    "shared".to_string()
                } else {
                    scope
                };
                tf(Msg::AiDoConnect, &[&g("profile"), &scope])
            }
            "set_extension_enabled" => tf(Msg::AiDoExt, &[&g("id"), &g("enabled")]),
            "insert_sequence" => {
                let what = if g("expr").trim().is_empty() {
                    format!(
                        "start {} step {} width {}",
                        g("start"),
                        g("step"),
                        g("width")
                    )
                } else {
                    g("expr")
                };
                let place = if g("from_line").is_empty() {
                    "-".to_string()
                } else {
                    format!("{}-{}", g("from_line"), g("to_line"))
                };
                tf(Msg::AiDoInsertSeq, &[&what, &place])
            }
            "disconnect" => tf(
                Msg::AiDoDisconnect,
                &[if g("scope") == "all" {
                    "all"
                } else {
                    "current"
                }],
            ),
            "edit_bookmark" => {
                let target = [g("id"), g("label"), g("line")]
                    .into_iter()
                    .find(|x| !x.is_empty())
                    .unwrap_or_default();
                tf(Msg::AiDoBookmark, &[&g("action"), &target])
            }
            "switch_tab" | "open_file" => {
                let target = [g("title"), g("path"), g("index"), g("id")]
                    .into_iter()
                    .find(|x| !x.is_empty())
                    .unwrap_or_default();
                tf(Msg::AiDoOpen, &[&target])
            }
            other => tf(Msg::AiDoOther, &[other]),
        };
        out.push(tf(Msg::AiCardDo, &[&what]));
        out.push(tf(Msg::AiCardWhere, &[&self.sess.profile, env]));
        out
    }

    /// 승인 카드 전에 알 수 있는 실패(대상 없음 · 편집기 탭 필요) — 있으면 그 오류 글.
    fn ai_precheck(&self, name: &str, input: &str) -> Option<String> {
        let v = json_parse(input).unwrap_or(Json::Obj(Vec::new()));
        let g = |k: &str| ai::jstr(&v, k);
        match name {
            "delete_var" => {
                let key = g("name").trim().trim_start_matches(':').to_string();
                (!key.is_empty()
                    && !self
                        .vars_rows()
                        .iter()
                        .any(|r| r.name.eq_ignore_ascii_case(&key)))
                .then(|| format!("no variable named {key} in this tab (use list_vars)"))
            }
            "edit_bookmark" => {
                if g("action") == "add" {
                    self.ai_view_active().then(|| {
                        "the chat tab is active; a bookmark needs an editor tab (switch_tab or open_file first)".to_string()
                    })
                } else {
                    let known = g("id")
                        .trim()
                        .parse::<u64>()
                        .ok()
                        .is_some_and(|id| self.bookmarks.store.get(id).is_some());
                    (!known).then(|| {
                        "no such bookmark id (call bookmarks with action=list)".to_string()
                    })
                }
            }
            "insert_sequence" if !self.ai_view_active() => self.ai_seq_precheck(&v),
            "insert_sequence" | "editor_carets" | "set_syntax" => {
                self.ai_view_active().then(|| {
                    "the chat tab is active; this needs an editor tab (switch_tab first)"
                        .to_string()
                })
            }
            _ => None,
        }
    }

    /// 도구 호출의 핵심 인자(등급 판정용): run_query = SQL · set_setting = 키 · run_command = id.
    fn ai_guard_arg(name: &str, input: &str) -> String {
        let v = json_parse(input).unwrap_or(Json::Obj(Vec::new()));
        match name {
            "run_query" | "explain" => ai::jstr(&v, "sql"),
            "set_setting" => ai::jstr(&v, "key"),
            "run_command" => ai::jstr(&v, "id"),
            "edit_editor" => ai::jstr(&v, "op"),
            _ => String::new(),
        }
    }

    /// 게이트 1을 지난 뒤: 조회문이 아닌 `run_query`는 **영향 행 수를 먼저 센다**(단일 테이블 UPDATE/DELETE/TRUNCATE/DROP TABLE) —
    /// 2단계 대상이거나 행 수가 `ai.guard_bulk_rows` 이상이면 게이트 2(영향 확인 카드) · 아니면 실행.
    fn ai_after_card(
        &mut self,
        name: String,
        input: String,
        reply: mpsc::Sender<ToolReply>,
        two: bool,
    ) {
        if name != "run_query" {
            self.ai_exec_tool(&name, &input, reply);
            return;
        }
        let sql = Self::ai_guard_arg(&name, &input);
        let dialect = self.sess.connected.then_some(self.sess.dialect);
        let count = ai::guard::impact_count_sql(&sql, dialect);
        match count {
            Some(q) if self.sess.connected && self.ai.pending.is_none() && self.gate_open() => {
                self.ai.capture = None;
                self.ai.pending = Some(PendingTool {
                    reply: None,
                    attach_label: None,
                    since: Instant::now(),
                    max_rows: 1,
                    wait_editor: None,
                    wait_meta: None,
                    wait_ui: None,
                    silent: true,
                    var_after: None,
                });
                self.ai.impact_wait = Some((name, input, reply, two));
                self.run_text_whole(q);
                if !self.sess.busy {
                    // 사전 조회가 시작되지 못했다(막힘) → 행 수를 모르는 채로 판정.
                    self.ai.pending = None;
                    if let Some((n, i, r, tw)) = self.ai.impact_wait.take() {
                        self.ai_impact_done(n, i, r, tw, None);
                    }
                }
            }
            _ => self.ai_impact_done(name, input, reply, two, None),
        }
    }

    /// 영향 행 수가 나왔다(`None` = 알 수 없음): 2단계 대상·기준 이상이면 게이트 2 · 아니면 실행.
    fn ai_impact_done(
        &mut self,
        name: String,
        input: String,
        reply: mpsc::Sender<ToolReply>,
        two: bool,
        count: Option<u64>,
    ) {
        let limit = self.settings.int("ai.guard_bulk_rows").max(1) as u64;
        if two || count.is_some_and(|n| n >= limit) {
            let prod = self.ai_guard_prod();
            let sql = Self::ai_guard_arg(&name, &input);
            let rows = count
                .map(|n| n.to_string())
                .unwrap_or_else(|| t(Msg::AiGuardUnknown).to_string());
            // ★ 같은 카드 한 장(사용자 11-13 "이중 확인을 보완했는데 여기는 2번 다시 출력 · 모두 동일한 개념으로"): 방금 허용한
            //   카드가 있으면 새 카드를 띄우지 않고 그 카드에 **영향 줄만** 덧붙인다(SQL 본문·운영 표시를 되풀이하지 않는다).
            //   [허용]은 장전된 채(남은 초 + 게이지)로 돌아온다 = 한 번 더 누르면 실행 · 시간이 지나면 처음부터 두 번.
            //   카드가 없는 길(자동 승인된 쓰기가 영향 기준에 걸림)만 새 카드.
            let line = tf(Msg::AiGuardStage2, &[&rows]);
            let reuse = self.ai.last_card.take().filter(|id| {
                self.ai_cur().is_some_and(|s| {
                    s.messages
                        .iter()
                        .any(|m| m.id == *id && matches!(m.kind, Kind::Approval { .. }))
                })
            });
            let mid = match reuse {
                Some(id) => {
                    if let Some(s) = self.ai_run_mut() {
                        if let Some(m) = s.messages.iter_mut().find(|m| m.id == id) {
                            m.text.push('\n');
                            m.text.push_str(&line);
                        }
                    }
                    id
                }
                None => {
                    let mut card = line;
                    if prod {
                        card.push('\n');
                        card.push_str(t(Msg::AiGuardProd));
                    }
                    card.push('\n');
                    card.push_str(&redact(&sql));
                    let mut m = ChatMsg::new(Kind::Approval { tool: name.clone() }, card);
                    m.id = ai::stamp_id();
                    let mid = m.id.clone();
                    self.ai_push(m);
                    mid
                }
            };
            self.ai.approving = None;
            self.ai.armed = Some(Instant::now());
            self.ai.approval = Some((
                mid,
                name.clone(),
                input.clone(),
                ApprovalReply::Stage2 { name, input, reply },
            ));
            self.redraw();
        } else {
            self.ai.last_card = None;
            self.ai.write_ok = Some((name.clone(), input.clone()));
            self.ai_exec_tool(&name, &input, reply);
        }
    }

    // ───────────────────────── 도구 실행(UI 스레드) ─────────────────────────

    fn ai_sql_is_readonly(&self, s: &str) -> bool {
        // 승인 요약은 입력 JSON일 수 있다 — `sql` 값을 꺼내 본다.
        let sql = json_parse(s)
            .ok()
            .map(|v| ai::jstr(&v, "sql"))
            .filter(|q| !q.is_empty())
            .unwrap_or_else(|| s.to_string());
        sql_is_readonly(&sql)
    }

    /// 도구 요청(API · MCP) — 쓰기·행 데이터는 승인 뒤 · 나머지는 바로.
    fn ai_tool_request(&mut self, name: String, input: String, reply: mpsc::Sender<ToolReply>) {
        let Some(tool) = ai::tools::find(&name) else {
            let _ = reply.send(ToolReply {
                output: format!("unknown tool {name}"),
                ok: false,
            });
            return;
        };
        let (auto, rows_allowed) = self
            .ai_cur()
            .map(|s| (s.auto, s.rows_allowed))
            .unwrap_or((false, false));
        // ★ 에이전트 도구 허용(agents.rs · 11-11): 모델이 전체 목록을 받아도 역할 밖 도구는 여기서 막는다(API·CLI·Codex 공통).
        // ★ 사전 검사(11-14 · 협업 V1 관찰 2회): 대상이 없거나 지금 할 수 없는 요청은 승인 카드를 띄우기 전에 거절한다.
        if let Some(why) = self.ai_precheck(tool.name, &input) {
            let _ = reply.send(ToolReply {
                output: why,
                ok: false,
            });
            return;
        }
        let agent = self.ai_cur().map(|s| s.agent.clone()).unwrap_or_default();
        if !ai::agents::allows(&agent, tool.name) {
            let _ = reply.send(ToolReply {
                output: format!(
                    "tool `{}` is not available to agent `{agent}` (use /agent assistant for all tools)",
                    tool.name
                ),
                ok: false,
            });
            return;
        }
        let is_rows = matches!(tool.name, "run_query" | "get_result" | "explain");
        // ★ 데이터 보호 하네스(guard.rs): 분류 → 문맥 → 정책. 조회 판정은 방언 분류와 문자열 판정이 **둘 다** 조회라고 해야 조회.
        let arg = Self::ai_guard_arg(tool.name, &input);
        let dialect = self.sess.connected.then_some(self.sess.dialect);
        let mut risk = ai::guard::classify_tool(tool.name, tool.write, &arg, dialect);
        if tool.name == "run_query" && risk == ai::guard::Risk::Read && !sql_is_readonly(&arg) {
            risk = ai::guard::Risk::Write;
        }
        // ★ 클라이언트 전용 명령만으로 된 본문(`EXEC :V := 리터럴` · `VAR` · `DEFINE` …)은 DB로 가지 않는다 → 대량 변경이 아니라
        //   앱 쓰기(사용자 11-12 "변수 값 하나 바꾸는데 승인 3~4번").
        let client_only = tool.name == "run_query" && {
            let items = split_items(&arg, self.sess.dialect);
            // `EXEC :V := 리터럴`은 클라이언트가 처리하고(DR-8) `EXEC :V := 식`은 DB가 식만 계산한다 — 둘 다 변수 대입이라
            //   트랜잭션이 없다(사용자 11-12 "변수 설정이면 운영에서도 SELECT 수준 점검으로").
            let local_exec = |i: &str| {
                matches!(
                    nsql_script::command::parse_command(i),
                    Ok(Some(nsql_script::command::Command::Exec { ref body }))
                        if ai::guard::is_bind_assign(body)
                )
            };
            !items.is_empty()
                && items
                    .iter()
                    .all(|i| sessions::is_client_only(i) || local_exec(i))
        };
        if client_only {
            // 조회 수준(카드 없음) — 실행부의 이중 안전장치(`write_ok`)는 이 호출을 승인된 것으로 표시해 지난다.
            risk = ai::guard::Risk::Read;
            self.ai.write_ok = Some((tool.name.to_string(), input.clone()));
        }
        // ★ 접속 해제(`run_command conn.disconnect` · 11-14): 운영에서도 승인 카드로 허용하되, **쓰고 있는 접속은 끊지 않는다** —
        //   실행 중이거나 커밋하지 않은 변경(대기·운영 보류)이 있으면 거절한다(끊으면 DBMS에 따라 커밋되거나 사라진다).
        if tool.name == "run_command" && ai::guard::is_disconnect_command(&arg) {
            let why = if self.sess.busy || self.sess.aux > 0 {
                Some("the connection is busy (a statement is running)")
            } else if !self.sess.tx_pending.is_empty() || self.sess.prod_held.is_some() {
                Some("the connection has uncommitted changes - the user must Commit or Rollback first (disconnecting could commit or lose them)")
            } else {
                None
            };
            if let Some(why) = why {
                let _ = reply.send(ToolReply {
                    output: format!("not executed: {why}. Tell the user."),
                    ok: false,
                });
                return;
            }
        }
        let prod = self.ai_guard_prod();
        let decision = ai::guard::decide(
            risk,
            ai::guard::Ctx {
                prod,
                auto,
                auto_write: self.settings.get("ai.guard_auto_write") == Some("dml"),
            },
        );
        if decision == ai::guard::Decision::Deny {
            self.ai_push(ChatMsg::new(Kind::Error, t(Msg::AiGuardDenied)));
            let _ = reply.send(ToolReply {
                output: "refused: on a production connection the assistant cannot commit/rollback, change protection settings or run execution commands - the user must do this by hand. Tell the user what to do instead.".into(),
                ok: false,
            });
            return;
        }
        // ★ 메타 우선 하네스(사용자 11-14 "자체 로딩해서 관리하는 meta 데이터를 먼저 조회하도록" · 설정 `ai.meta_first` 기본 켬):
        //   데이터 사전을 읽는 조회(`is_catalog_query`)인데 이번 턴에 앱 메타 도구(`search_objects` · `describe_object` ·
        //   `meta_stats`)를 한 번도 쓰지 않았으면 **한 번** 되돌린다 — 이름·코멘트·컬럼은 앱이 이미 들고 있다(왕복 0).
        //   같은 턴에 이미 되돌렸거나 메타 도구를 쓴 뒤면 그대로 보낸다(무한 반복 없음).
        if tool.name == "run_query"
            && self.settings.flag("ai.meta_first")
            && ai::guard::is_catalog_query(&arg, dialect)
        {
            let (turn_at, used_meta) = self
                .ai_cur()
                .map(|s| {
                    let lu = s.messages.iter().rposition(|m| m.kind == Kind::User);
                    let used = s.messages[lu.map_or(0, |i| i + 1)..].iter().any(|m| {
                        matches!(&m.kind, Kind::ToolCall { name, .. }
                            if matches!(name.as_str(), "search_objects" | "describe_object" | "meta_stats"))
                    });
                    (lu, used)
                })
                .unwrap_or((None, true));
            if !used_meta && self.ai.meta_nudged_at != turn_at {
                self.ai.meta_nudged_at = turn_at;
                let _ = reply.send(ToolReply {
                    output: "not executed: this reads the data dictionary. Nexa SQL already caches object names, table/column comments and columns - call search_objects first (use the user's own words, e.g. Korean terms: comments are searched too), then describe_object for columns and column comments. Run a dictionary query only if those do not answer. (This reminder is sent once per turn.)".into(),
                    ok: false,
                });
                return;
            }
        }
        let short = tool.name.to_string();
        // ★ 카탈로그(메타) 조회는 행 데이터 동의를 묻지 않는다(사용자 11-14 · 설정 `ai.meta_auto` 기본 켬): 테이블·컬럼·코멘트 같은
        //   데이터 사전만 읽는 조회문(`guard::is_catalog_query` — 사용자 표·함수·파생 표가 하나라도 끼면 아님). 동의 상태
        //   (`rows_allowed`)는 바꾸지 않는다 = 실제 데이터 조회는 여전히 한 번 묻는다.
        //   ★ 행을 돌려주지 않는 컬럼 확인 조회(`… WHERE 1=0` · `guard::is_empty_probe`)도 같다(사용자 11-14 "컬럼 정보를 읽는 경우
        //   확인 없이") — 열 머리만 돌아온다.
        let meta_free = matches!(tool.name, "run_query" | "explain")
            && risk == ai::guard::Risk::Read
            && self.settings.flag("ai.meta_auto")
            && (ai::guard::is_catalog_query(&arg, dialect) || ai::guard::is_empty_probe(&arg));
        let need_rows = is_rows && !rows_allowed && !client_only && !meta_free;
        let carded = matches!(
            decision,
            ai::guard::Decision::Card | ai::guard::Decision::TwoStage
        );
        // ★ 쓰기 문장은 카드 한 장(11-13 사용자 "중복된 내용을 굳이 2번 표시하는건 낭비"): 어차피 SQL 본문이 든 쓰기 확인 카드가
        //   뜨므로 동의 카드를 따로 띄우지 않고 그 카드에 동의 줄을 싣는다. 조회(카드 없음)·자동 승인된 쓰기는 예전대로 동의 카드.
        let merge_rows = need_rows && carded && tool.name == "run_query";
        if need_rows && !merge_rows {
            let mut lines = self.ai_card_head(tool.name, &input);
            lines.push(t(Msg::AiRowsConsent).to_string());
            if !arg.trim().is_empty() {
                lines.push(redact(&arg));
            }
            let mut m = ChatMsg::new(
                Kind::Approval {
                    tool: short.clone(),
                },
                lines.join("\n"),
            );
            m.id = ai::stamp_id();
            let mid = m.id.clone();
            self.ai_push(m);
            self.ai.approval = Some((
                mid,
                short.clone(),
                t(Msg::AiRowsConsent).into(),
                ApprovalReply::Rows {
                    name: short,
                    input,
                    reply,
                },
            ));
            return;
        }
        if matches!(
            decision,
            ai::guard::Decision::Card | ai::guard::Decision::TwoStage
        ) {
            let two = decision == ai::guard::Decision::TwoStage;
            // 카드 본문: run_query면 JSON 대신 **SQL 본문** · 그 밖은 입력 JSON. 머리 줄 = 운영 표시 · 등급 · WHERE 없음 경고.
            let body = if tool.name == "run_query" && !arg.trim().is_empty() {
                redact(&arg)
            } else if let Some(pv) = self.ai_edit_preview(tool.name, &input) {
                // ★ 편집기를 바꾸는 도구 = 전후 줄 미리보기(`ai.diff_context` · 0이면 입력 JSON 그대로).
                pv
            } else {
                redact(&input)
            };
            let mut head: Vec<String> = self.ai_card_head(tool.name, &input);
            if merge_rows {
                head.push(t(Msg::AiRowsConsentIncl).to_string());
            }
            if prod {
                head.push(t(Msg::AiGuardProd).to_string());
            }
            // 두 번 눌러 허용 = 운영 접속의 DB 쓰기(영향 확인 카드가 따로 뜨는 2단계 대상은 그 카드가 두 번째 확인).
            let twice = prod && tool.name == "run_query" && !two;
            self.ai.armed = None;
            let label = match risk {
                ai::guard::Risk::Bulk => Some(Msg::AiRiskBulk),
                ai::guard::Risk::Destructive => Some(Msg::AiRiskDestructive),
                ai::guard::Risk::TxEnd => Some(Msg::AiRiskTxEnd),
                ai::guard::Risk::Protection => Some(Msg::AiRiskProtection),
                _ => None,
            };
            if let Some(l) = label {
                head.push(t(l).to_string());
            }
            if tool.name == "run_query" && sql_dml_without_where(&arg) {
                head.push(t(Msg::AiNoWhereWarn).to_string());
            }
            head.push(body);
            let card = head.join("\n");
            let mut m = ChatMsg::new(
                Kind::Approval {
                    tool: short.clone(),
                },
                card,
            );
            m.id = ai::stamp_id();
            let mid = m.id.clone();
            self.ai_push(m);
            self.ai.approval = Some((
                mid,
                short.clone(),
                input.clone(),
                ApprovalReply::Api {
                    name: short,
                    input,
                    reply,
                    two,
                    rows: merge_rows,
                    twice,
                },
            ));
            return;
        }
        // 바로 실행(조회 · 자동 승인된 일반 변경) — 자동 승인된 변경도 영향 행 수 기준은 거친다.
        if tool.name == "run_query" && risk != ai::guard::Risk::Read {
            self.ai_after_card(short, input, reply, false);
            return;
        }
        self.ai_exec_tool(&short, &input, reply);
    }

    /// 도구 실행 — 즉시 답하거나(동기) 대기 등록(비동기 · 질의·설명·소스).
    fn ai_exec_tool(&mut self, name: &str, input: &str, reply: mpsc::Sender<ToolReply>) {
        let v = json_parse(input).unwrap_or(Json::Obj(Vec::new()));
        let g = |k: &str| ai::jstr(&v, k);
        let gi = |k: &str, d: i64| match ai::jget(&v, k) {
            Some(Json::Num(n)) => *n as i64,
            Some(Json::Str(s)) => s.parse().unwrap_or(d),
            _ => d,
        };
        let max_rows = gi("max_rows", self.settings.int("ai.max_rows").max(1)).max(1) as usize;
        let ok = |s: String| ToolReply {
            output: s,
            ok: true,
        };
        let err = |s: String| ToolReply {
            output: s,
            ok: false,
        };
        let r: ToolReply = match name {
            "list_commands" => {
                let f = g("filter").to_lowercase();
                let mut out = String::new();
                for c in crate::app::commands::PALETTE {
                    let label = format!("{}: {}", t(c.menu), t(c.item));
                    if f.is_empty() || c.id.contains(&f) || label.to_lowercase().contains(&f) {
                        out.push_str(&format!("{}\t{}\n", c.id, label));
                    }
                }
                ok(out)
            }
            "run_command" => {
                let id = g("id");
                if crate::app::commands::PALETTE.iter().any(|c| c.id == id)
                    || id.starts_with("view.")
                {
                    // ★ 토글 명령(창 보이기·옵션 켜기)은 지금 상태를 먼저 본다(사용자 11-12 "이미 열려 있는지 확인") — 기본 state=on:
                    //   이미 켜져/열려 있으면 다시 누르지 않는다(누르면 닫힌다) · 열린 창은 앞으로만.
                    match self.ai_toggle_state(&id) {
                        Some(cur) => {
                            let want = match g("state").as_str() {
                                "off" => false,
                                "toggle" => !cur,
                                _ => true,
                            };
                            if cur == want {
                                if want {
                                    self.ai_raise_window(&id);
                                }
                                ok(format!(
                                    "{id} is already {} - left as it is{}",
                                    if want { "on/open" } else { "off/closed" },
                                    if want { " (brought to the front)" } else { "" }
                                ))
                            } else {
                                self.menu_action(&id);
                                ok(format!(
                                    "{id} is now {}",
                                    if want { "on/open" } else { "off/closed" }
                                ))
                            }
                        }
                        None if ai::guard::is_disconnect_command(&id) => {
                            // 공유 연결의 선택 메뉴를 띄우지 않고 끊는다(카드에 탭 수를 보였다) · 결과는 실제 상태로 답한다.
                            let tabs = self.disconnect_for_ai();
                            if self.sess.connected {
                                err(format!(
                                    "not disconnected yet: {} - tell the user",
                                    redact(&self.sess.status)
                                ))
                            } else {
                                ok(format!(
                                    "disconnected ({tabs} tab(s) were using this connection; they are now unconnected)"
                                ))
                            }
                        }
                        None if id.starts_with("edit.") && self.ai_view_active() => {
                            err("the chat tab is active; edit commands need an editor tab (switch_tab first)".into())
                        }
                        None => {
                            // ★ 편집 명령(`edit.*`)은 편집기에 포커스가 있어야 동작한다 — AI 패널에서 대화 중이면 무시되는데
                            //   "ran"으로 답했다(11-14 조사). 명령 동안만 편집기 포커스로 보고, 명령이 포커스를 옮기지 않았으면 되돌린다.
                            let prev = self.focus;
                            let force = id.starts_with("edit.") && prev != Focus::Editor;
                            if force {
                                self.focus = Focus::Editor;
                            }
                            self.menu_action(&id);
                            if force && self.focus == Focus::Editor {
                                self.focus = prev;
                            }
                            ok(format!("ran {id}"))
                        }
                    }
                } else {
                    err(format!("unknown command {id}"))
                }
            }
            "get_setting" => {
                let key = g("key");
                if key.is_empty() {
                    let mut out = String::new();
                    for e in nsql_settings::REGISTRY {
                        out.push_str(&format!(
                            "{}={}\n",
                            e.key,
                            self.settings.get(e.key).unwrap_or("")
                        ));
                    }
                    ok(out)
                } else {
                    match self.settings.get(&key) {
                        Some(v) => ok(v.to_string()),
                        None => err(format!("unknown key {key}")),
                    }
                }
            }
            "set_setting" => {
                let key = g("key");
                let val = g("value");
                match self.settings.set(&key, &val) {
                    Ok(v) => {
                        self.persist_settings();
                        let applied = self.apply_setting(&key);
                        self.prefs_win.refresh(&self.settings);
                        ok(format!(
                            "{key}={v}{}",
                            if applied { "" } else { " (restart needed)" }
                        ))
                    }
                    Err(e) => err(e.to_string()),
                }
            }
            "get_editor_text" => {
                if self.ai_view_active() {
                    err("the chat tab is active; switch to a SQL editor tab first".into())
                } else {
                    let full = self.editors.cur().text();
                    // ★ 선택·캐럿은 구간에서 직접 읽는다(11-14): `copy_selection`은 편집기에 포커스가 있을 때만 값을 줘, 채팅에
                    //   입력 중이면 선택이 비어 보였다 · 멀티 캐럿(열 모드)도 함께 알린다(줄:열 · 앞 50개).
                    let (sel, carets) = {
                        let tb = self.editors.cur();
                        let buf = tb.buf();
                        let regs = tb.regions();
                        let sel = regs
                            .iter()
                            .filter(|(a, b)| b > a)
                            .map(|&(a, b)| buf.slice_string(a, b))
                            .collect::<Vec<_>>()
                            .join("\n");
                        let list = regs
                            .iter()
                            .take(50)
                            .map(|&(a, b)| {
                                let l = buf.line_of(b);
                                format!(
                                    "L{}:C{}{}",
                                    l + 1,
                                    b - buf.line_start(l) + 1,
                                    if b > a { "*" } else { "" }
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        (
                            sel,
                            format!("<carets count=\"{}\">{list}</carets>\n", regs.len()),
                        )
                    };
                    let byte_pos = full
                        .char_indices()
                        .nth(self.editors.cur().caret())
                        .map_or(full.len(), |(b, _)| b);
                    let item =
                        nsql_script::statement_at_in(&full, byte_pos, Some(self.sess.dialect));
                    // ★ 커서 위치(사용자 11-12 "편집기의 커서 위치를 확인하는 함수"): 줄·열(1부터) · 커서 문장의 줄 범위 · 선택 줄 범위.
                    let (line, col) = self.editors.caret_line_col();
                    let line_of = |b: usize| full[..b.min(full.len())].matches('\n').count() + 1;
                    let (stmt, range) = match &item {
                        Some(it) => (
                            full[it.span.clone()].to_string(),
                            format!(
                                " lines=\"{}-{}\"",
                                line_of(it.span.start),
                                line_of(it.span.end.saturating_sub(1).max(it.span.start))
                            ),
                        ),
                        None => (String::new(), String::new()),
                    };
                    let total = full.matches('\n').count() + 1;
                    ok(format!(
                        "<caret line=\"{line}\" column=\"{col}\" total_lines=\"{total}\"/>\n{carets}<text>\n{full}\n</text>\n<selection>\n{sel}\n</selection>\n<statement_at_caret{range}>\n{stmt}\n</statement_at_caret>"
                    ))
                }
            }
            "list_vars" => ok(redact(&self.ai_vars_list())),
            "get_var" => match self.ai_var_get(&g("name")) {
                Ok(s) => ok(redact(&s)),
                Err(e) => err(e),
            },
            "set_var" if !g("formula").trim().is_empty() => {
                // ★ 수식 지정(사용자 11-12 "VAR3을 VAR2 + 2로 등록"): `EXEC :이름 := 수식`을 엔진으로 실행 — 변수 창에 수식 변수로 남는다.
                let name_ = g("name").trim().trim_start_matches(':').to_string();
                let formula = g("formula").trim().trim_end_matches(';').to_string();
                let valid = !name_.is_empty()
                    && name_.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && !formula.contains(';');
                if !valid {
                    err("bad variable name or formula (one expression, no ';')".into())
                } else if !nsql_script::bind::expr_is_plain(&formula) {
                    // ★ 식 안의 사용자 루틴 호출은 DML을 할 수 있다(11-13 데이터 보호) — 카드 없는 `set_var`로는 보내지 않는다.
                    //   `run_query`의 `EXEC :이름 := 식`은 쓰기 등급으로 분류돼 승인 카드(운영 = 두 번 눌러 허용)를 거친다.
                    err("this formula calls a routine that is not a known built-in function, so it may change data. set_var cannot run it. If the user really wants it, send it with run_query as `EXEC :NAME := <formula>` - that goes through the write approval card.".into())
                } else if !self.sess.connected || self.ai.pending.is_some() || !self.gate_open() {
                    err(format!(
                        "cannot evaluate the formula now: {}",
                        if self.sess.connected {
                            "the session is busy"
                        } else {
                            "not connected (a formula is evaluated by the database)"
                        }
                    ))
                } else {
                    self.ai.capture = None;
                    self.ai.pending = Some(PendingTool {
                        reply: Some(reply),
                        attach_label: None,
                        since: Instant::now(),
                        max_rows: 1,
                        wait_editor: None,
                        wait_meta: None,
                        wait_ui: None,
                        silent: true,
                        var_after: Some(name_.clone()),
                    });
                    // 수식 평가 = 식 하나(사용자가 카드로 승인) → 운영 2단 무장·보류를 건너뛴다.
                    self.prod_confirmed_once = true;
                    self.hold_skip_once = true;
                    self.run_text(format!("EXEC :{name_} := {formula};"), 0, true);
                    self.prod_confirmed_once = false;
                    self.hold_skip_once = false;
                    if !self.sess.busy {
                        if let Some(mut p) = self.ai.pending.take() {
                            if let Some(r) = p.reply.take() {
                                let _ = r.send(err(format!(
                                    "not executed: {}",
                                    redact(&self.sess.status)
                                )));
                            }
                        }
                    }
                    return;
                }
            }
            "set_var" => {
                let null = matches!(g("null").as_str(), "true" | "1");
                match self.ai_var_set(&g("name"), &g("value"), null) {
                    Ok(s) => ok(redact(&s)),
                    Err(e) => err(e),
                }
            }
            "delete_var" => match self.ai_var_delete(&g("name")) {
                Ok(s) => ok(redact(&s)),
                Err(e) => err(e),
            },
            "edit_editor" => {
                if self.ai_view_active() {
                    err("the chat tab is active; switch to a SQL editor tab first".into())
                } else {
                    match self.ai_edit_plan(input) {
                        Err(e) => err(e),
                        Ok(p) => {
                            let mut inv = Invalidations::default();
                            match p.new {
                                None => {
                                    self.ed_mut()
                                        .select_range(p.start, p.start + p.len, &mut inv);
                                    self.redraw();
                                    ok(format!("selected lines {}..{}", p.a, p.b))
                                }
                                Some(t2) => {
                                    self.ed_mut().replace_range(
                                        p.start,
                                        p.start + p.len,
                                        &t2,
                                        &mut inv,
                                    );
                                    self.redraw();
                                    ok(format!(
                                        "lines {}..{} {} done ({} line(s) now; undo with Ctrl+Z)",
                                        p.a,
                                        p.b,
                                        p.op,
                                        t2.split('\n').count()
                                    ))
                                }
                            }
                        }
                    }
                }
            }
            "set_editor_text" => {
                let text = g("text");
                let mode = g("mode");
                if mode == "new_tab" || self.ai_view_active() {
                    self.editors.new_tab(None);
                }
                let mut inv = Invalidations::default();
                if mode == "insert" {
                    self.ed_mut().paste(&text, &mut inv);
                } else {
                    let len = self.ed_mut().text().chars().count();
                    self.ed_mut().replace_range(0, len, &text, &mut inv);
                }
                self.redraw();
                ok("editor updated".into())
            }
            "format_sql" => {
                let engine = self.format_default_engine();
                match self.format_run(&engine, &g("sql"), false) {
                    Ok(s) => ok(s),
                    Err(e) => err(e),
                }
            }
            "search_objects" => {
                let limit = gi("limit", 50).clamp(1, 500) as usize;
                let in_cols = matches!(
                    g("in_columns").to_ascii_lowercase().as_str(),
                    "true" | "1" | "yes" | "on"
                );
                let mut rows = self.explorer.ai_search_objects(
                    &g("query"),
                    &g("kind"),
                    &g("schema"),
                    in_cols,
                    limit,
                );
                // ★ 0건 + 그 스키마의 코멘트가 아직 없음 = 적재를 걸고 알려 준다(11-14 메타 우선: 뜻으로 찾는 검색은 코멘트가
                //   있어야 닿는다 · 스키마 하나만 = 지연 로딩) — 어시스턴트가 사전 SQL로 넘어가기 전에 한 번 더 부르게.
                if rows.first().is_some_and(|l| l.starts_with("0 hits")) {
                    if let Some(sc) = self.explorer.ai_warm_comments(&g("schema")) {
                        rows.push(format!(
                            "hint: table/column comments of schema {sc} were not loaded yet - loading them now. Call search_objects again in a moment (same words) before trying a dictionary query."
                        ));
                    }
                }
                ok(rows.join("\n"))
            }
            "meta_stats" => ok(self.explorer.ai_meta_stats()),
            // ★ 탭 · 파일 · 확장(docs/110 §16 스킬 영역 · 11-11).
            "list_tabs" => {
                let active = self.editors.active_id();
                let rows: Vec<String> = self
                    .editors
                    .tab_titles()
                    .into_iter()
                    .enumerate()
                    .map(|(i, (id, title))| {
                        format!(
                            "{id}\t{title}\t{}\t{}\t{}",
                            self.editors
                                .path_of(i)
                                .map(|p| p.display().to_string())
                                .unwrap_or_default(),
                            if self.editors.is_dirty(i) {
                                "dirty"
                            } else {
                                "clean"
                            },
                            if id == active { "active" } else { "" }
                        )
                    })
                    .collect();
                ok(format!(
                    "id\ttitle\tpath\tstate\tactive\n{}",
                    rows.join("\n")
                ))
            }
            "switch_tab" => {
                let want = g("tab");
                let idx = want
                    .trim()
                    .parse::<u64>()
                    .ok()
                    .and_then(|id| self.editors.index_of_id(id))
                    .or_else(|| {
                        self.editors
                            .tab_titles()
                            .iter()
                            .position(|(_, t)| t.eq_ignore_ascii_case(want.trim()))
                    });
                match idx {
                    Some(i) => {
                        self.editors.switch(i);
                        self.layout();
                        self.redraw();
                        ok(format!("active tab: {}", self.editors.active_title()))
                    }
                    None => err(format!("no tab '{want}' (use list_tabs)")),
                }
            }
            "list_files" => {
                let limit = gi("limit", 50).clamp(1, 500) as usize;
                match self.work_root() {
                    Some(rootdir) => ok(ai_list_files(&rootdir, &g("query"), limit).join("\n")),
                    None => err("no project or folder is open".into()),
                }
            }
            // ★ 북마크 · 파일 내용 검색 · 객체 탐색기 이동(사용자 11-14 "관련해서 필요한 API 및 MCP 구성") — 도구 표 하나가
            //   API · CLI(MCP) · Codex에 함께 나간다.
            // ★ 편집기 기능(사용자 11-14 "AI를 통해서 사용할 수 있도록"): 열 모드(줄마다 캐럿) · 일련번호(Insert Sequence 확장) ·
            //   탭의 구문 바꾸기. 줄 정렬·중복 제거·대소문자(Capital 포함)는 `edit_editor`의 op · 줄바꿈은 `run_command view.word_wrap`.
            "editor_carets" => {
                if self.ai_view_active() {
                    err("the chat tab is active; switch to a SQL editor tab first".into())
                } else if g("mode") == "clear" {
                    let c = self.editors.cur().caret();
                    self.ed_mut().set_regions_pub(&[(c, c)]);
                    self.redraw();
                    ok("back to a single caret".into())
                } else {
                    let mode = g("mode");
                    match self.ai_line_carets(
                        gi("from_line", 0),
                        gi("to_line", 0),
                        &mode,
                        gi("column", 0),
                    ) {
                        Ok(v) => {
                            let n = v.len();
                            self.ed_mut().set_regions_pub(&v);
                            self.redraw();
                            ok(format!(
                                "{n} caret(s) set ({}) - now use insert_sequence or an edit command",
                                if mode.is_empty() { "line_end" } else { mode.as_str() }
                            ))
                        }
                        Err(e) => err(e),
                    }
                }
            }
            "insert_sequence" => {
                if self.ai_view_active() {
                    err("the chat tab is active; switch to a SQL editor tab first".into())
                } else {
                    // 식 = `expr` 그대로 · 없으면 start/step/width/prefix/suffix로 조립(확장 문법: 시작±증가~0자릿수 "틀").
                    let expr = Self::ai_seq_expr(&v);
                    let placed = if gi("from_line", 0) > 0 {
                        self.ai_line_carets(
                            gi("from_line", 0),
                            gi("to_line", 0),
                            &g("at"),
                            gi("column", 0),
                        )
                        .map(|v| self.ed_mut().set_regions_pub(&v))
                    } else {
                        Ok(())
                    };
                    match expr.and_then(|x| placed.and_then(|()| self.ai_insert_sequence(&x))) {
                        Ok(s) => ok(s),
                        Err(e) => err(e),
                    }
                }
            }
            "set_syntax" => {
                if self.ai_view_active() {
                    err("the chat tab is active; switch to an editor tab first".into())
                } else {
                    let want = g("name");
                    let names = self.syntax.names();
                    match names.iter().find(|n| n.eq_ignore_ascii_case(want.trim())) {
                        Some(n) => {
                            let n = n.clone();
                            self.menu_action(&format!("syntax.set:{n}"));
                            self.redraw();
                            if self.editors.syntax_name() == n {
                                ok(format!("the syntax of this tab is now {n}"))
                            } else {
                                err(format!("could not switch to {n}"))
                            }
                        }
                        None => err(format!(
                            "syntax '{}' is not installed. Available: {}. Other languages come from syntax packages (*.nexa-syntax in the Packages folder) - tell the user.",
                            want.trim(),
                            names.join(", ")
                        )),
                    }
                }
            }
            // ★ 프로젝트 탐색기 필터(사용자 11-14 "프로젝트 탐색기 활동 탭으로 이동 → Filter files에 입력해 필터"): 패널을 보이고
            //   필터 상자에 글·옵션을 넣는다(사람이 친 것과 같은 길 `set_filter_text`) · 빈 글 = 필터 해제.
            "filter_project" => {
                if !self.project_panel.is_visible() {
                    self.menu_action("view.project");
                }
                let flag = |k: &str| {
                    matches!(
                        g(k).to_ascii_lowercase().as_str(),
                        "true" | "1" | "yes" | "on"
                    )
                };
                let q = g("query");
                self.project_panel.set_filter_opts(
                    flag("case"),
                    flag("word"),
                    flag("regex"),
                    flag("path"),
                );
                self.project_panel.set_filter_text(q.trim());
                self.redraw();
                ok(if q.trim().is_empty() {
                    "project explorer shown · filter cleared".to_string()
                } else {
                    format!(
                        "project explorer shown · filter = '{}' · {} row(s) listed now (the list may still be updating; use list_files for the file names)",
                        q.trim(),
                        self.project_panel.shown_rows()
                    )
                })
            }
            // ★ 접속 해제 도구(사용자 11-14 "전체 연결을 모두 종료" · "연결된 모든 세션 해제"): scope=current(이 탭의 연결) |
            //   all(열려 있는 연결 전부 · 공유·전용). 실행 중이거나 미커밋 변경이 있는 연결은 건너뛰고 알린다(끊지 않는다).
            "disconnect" => {
                let ids: Vec<u64> = if g("scope") == "all" {
                    self.all_sess()
                        .filter(|s| s.connected && !s.closing)
                        .map(|s| s.id)
                        .collect()
                } else {
                    vec![self.sess.id]
                };
                let mut done: Vec<String> = Vec::new();
                let mut skipped: Vec<String> = Vec::new();
                for id in ids {
                    let r = self.with_sess(id, |a| {
                        if !a.sess.connected {
                            return None;
                        }
                        let desc = a.sess.desc.clone();
                        if a.sess.busy || a.sess.aux > 0 {
                            return Some((desc, Err("busy - a statement is running")));
                        }
                        if !a.sess.tx_pending.is_empty() || a.sess.prod_held.is_some() {
                            return Some((
                                desc,
                                Err("uncommitted changes - the user must Commit or Rollback first"),
                            ));
                        }
                        a.disconnect_for_ai();
                        let left = a.sess.connected;
                        Some((desc, if left { Err("still connected") } else { Ok(()) }))
                    });
                    match r.flatten() {
                        Some((d, Ok(()))) => done.push(d),
                        Some((d, Err(w))) => skipped.push(format!("{d} ({w})")),
                        None => {}
                    }
                }
                self.sync_sess_ui();
                self.redraw();
                let text = format!(
                    "disconnected {}: {}\nnot disconnected {}: {}",
                    done.len(),
                    if done.is_empty() {
                        "-".to_string()
                    } else {
                        done.join(", ")
                    },
                    skipped.len(),
                    if skipped.is_empty() {
                        "-".to_string()
                    } else {
                        skipped.join(", ")
                    }
                );
                if done.is_empty() && skipped.is_empty() {
                    ok("no open connection".into())
                } else if done.is_empty() {
                    err(redact(&text))
                } else {
                    ok(redact(&text))
                }
            }
            "bookmarks" => {
                if g("action") == "goto" {
                    let id = match g("id").trim().parse::<u64>() {
                        Ok(id) => Some(id),
                        Err(_) => g("mnemonic")
                            .trim()
                            .parse::<u8>()
                            .ok()
                            .and_then(|n| self.bookmarks.ai_by_mnemonic(n)),
                    };
                    match id.filter(|id| self.bookmarks.store.get(*id).is_some()) {
                        Some(id) => {
                            self.bm_goto(id, true);
                            self.redraw();
                            ok(format!("went to bookmark {id}"))
                        }
                        None => err("no such bookmark (call bookmarks with action=list)".into()),
                    }
                } else {
                    let cur = (g("scope") == "current" && !self.ai_view_active()).then(|| {
                        crate::bookmarks::Bookmarks::doc_key(&self.editors, self.editors.active())
                    });
                    let rows = self
                        .bookmarks
                        .ai_rows(&g("filter"), cur.as_ref(), &|d| self.bm_panel.doc_name(d));
                    ok(format!(
                        "{} bookmark(s) · columns: id, document, line, text/label, note, number, state\n{}",
                        rows.len(),
                        rows.join("\n")
                    ))
                }
            }
            "edit_bookmark" => {
                let action = g("action");
                let id = g("id").trim().parse::<u64>().ok();
                let known = id.filter(|id| self.bookmarks.store.get(*id).is_some());
                let res: Result<String, String> = match (action.as_str(), known) {
                    ("add", _) => {
                        if self.ai_view_active() {
                            Err("the chat tab is active; a bookmark needs an editor tab (switch_tab or open_file first)".into())
                        } else {
                            let i = self.editors.active();
                            let line = gi("line", 0);
                            let line0 = if line > 0 {
                                (line - 1) as usize
                            } else {
                                self.editors.caret_line_col().0.saturating_sub(1)
                            };
                            match self.bookmarks.add_at(&self.editors, i, line0) {
                                Ok(id) => {
                                    let (label, note) = (g("label"), g("note"));
                                    if !label.trim().is_empty() {
                                        self.bookmarks.set_label(id, Some(label));
                                    }
                                    if !note.trim().is_empty() {
                                        self.bookmarks.set_note(id, Some(note));
                                    }
                                    // 번호(니모닉 0~9)도 추가와 함께 줄 수 있다(사용자 11-14).
                                    let n = gi("number", -1);
                                    if (0..=9).contains(&n) {
                                        self.bookmarks.set_mnemonic(id, Some(n as u8));
                                    }
                                    Ok(format!("added bookmark id={id} at line {}", line0 + 1))
                                }
                                Err("line") => {
                                    Err(format!("line {} is outside the document", line0 + 1))
                                }
                                Err(e) => Err(format!("bookmark limit reached ({e})")),
                            }
                        }
                    }
                    (_, None) => {
                        Err("no such bookmark id (call bookmarks with action=list)".into())
                    }
                    ("rename", Some(id)) => {
                        self.bookmarks.set_label(id, Some(g("label")));
                        Ok(format!("renamed bookmark {id}"))
                    }
                    ("note", Some(id)) => {
                        self.bookmarks.set_note(id, Some(g("note")));
                        Ok(format!("updated the note of bookmark {id}"))
                    }
                    ("mnemonic", Some(id)) => {
                        let n = gi("number", -1);
                        self.bookmarks
                            .set_mnemonic(id, (0..=9).contains(&n).then_some(n as u8));
                        Ok(format!(
                            "bookmark {id}: number set to {}",
                            if (0..=9).contains(&n) {
                                n.to_string()
                            } else {
                                "none".into()
                            }
                        ))
                    }
                    ("delete", Some(id)) => {
                        let _ = self.bookmarks.remove(id);
                        Ok(format!("deleted bookmark {id}"))
                    }
                    _ => Err("unknown action (add | rename | note | mnemonic | delete)".into()),
                };
                self.bm_sync_ui();
                self.redraw();
                match res {
                    Ok(s) => ok(s),
                    Err(e) => err(e),
                }
            }
            "search_files" => {
                let q = g("query");
                if q.trim().is_empty() {
                    err("empty query".into())
                } else if self.ai.pending.is_some() {
                    err("busy - another tool is still waiting".into())
                } else {
                    // 검색 패널을 보이게 하고(값·결과가 화면에 남는다 · 사용자 11-14 "Search에 값을 입력하고 조회까지") 값을 넣어 실행.
                    if !self.search.is_visible() {
                        self.menu_action("view.search");
                    }
                    let flag = |k: &str| {
                        matches!(
                            g(k).to_ascii_lowercase().as_str(),
                            "true" | "1" | "yes" | "on"
                        )
                    };
                    self.search.set_toggles(
                        flag("case"),
                        flag("word"),
                        flag("regex"),
                        flag("where_case"),
                    );
                    self.search.run_query(&q, Some(&g("where")));
                    let _ = self.search.take_request();
                    self.start_search();
                    let limit = gi("limit", 100).clamp(1, 1000) as usize;
                    self.ai.pending = Some(PendingTool {
                        reply: Some(reply),
                        attach_label: None,
                        since: Instant::now(),
                        max_rows: 0,
                        wait_editor: None,
                        wait_meta: None,
                        wait_ui: Some(AiWait::Search { limit }),
                        silent: false,
                        var_after: None,
                    });
                    self.redraw();
                    return;
                }
            }
            "reveal_object" => {
                let (name, schema, member) = (g("name"), g("schema"), g("member"));
                if name.trim().is_empty() {
                    err("empty object name".into())
                } else if self.ai.pending.is_some() {
                    err("busy - another tool is still waiting".into())
                } else if !self.sess.connected {
                    err("not connected - the object explorer shows objects of a connection".into())
                } else {
                    // 편집기 호버 카드의 [객체 탐색기에서 보기]와 같은 길: 이 탭의 연결(전용 세션 포함)의 서버 칸으로 탐색기를 보이고,
                    //   스키마 → 종류 폴더 → 객체(→ 멤버) 순으로 펼쳐 선택한다(안 읽힌 가지는 읽고 이어 간다).
                    let table = if schema.trim().is_empty() {
                        name.trim().to_string()
                    } else {
                        format!("{}.{}", schema.trim(), name.trim())
                    };
                    let member = (!member.trim().is_empty()).then(|| member.trim().to_string());
                    if self.reveal_table_member(&table, member) {
                        self.ai.pending = Some(PendingTool {
                            reply: Some(reply),
                            attach_label: None,
                            since: Instant::now(),
                            max_rows: 0,
                            wait_editor: None,
                            wait_meta: None,
                            wait_ui: Some(AiWait::Reveal {
                                name: name.trim().to_string(),
                            }),
                            silent: false,
                            var_after: None,
                        });
                        self.redraw();
                        return;
                    }
                    err(format!(
                        "could not reveal {table}: {} (find the exact name with search_objects first)",
                        redact(&self.sess.status)
                    ))
                }
            }
            "open_file" => {
                let p = g("path");
                let path = std::path::PathBuf::from(p.trim());
                let path = if path.is_absolute() {
                    path
                } else {
                    match self.work_root() {
                        Some(r) => r.join(&path),
                        None => path,
                    }
                };
                if !path.is_file() {
                    err(format!("not a file: {}", path.display()))
                } else {
                    // ★ 줄(·열)을 주면 열고 그 자리로(11-14 · 검색 결과로 가는 길 — 검색 패널에서 결과를 연 것과 같다).
                    let line = gi("line", 0);
                    if line > 0 {
                        self.open_search_result(crate::search_panel::OpenReq {
                            path: Some(path.clone()),
                            tab: None,
                            line: line as usize,
                            col: (gi("column", 1).max(1) - 1) as usize,
                            len: 0,
                            permanent: true,
                        });
                        ok(format!("opened {} at line {line}", path.display()))
                    } else {
                        self.project_open_req(crate::project_panel::OpenReq {
                            path: path.clone(),
                            permanent: true,
                        });
                        ok(format!("opened {}", path.display()))
                    }
                }
            }
            "list_extensions" => {
                let off = self.ext_disabled();
                let rows: Vec<String> = self
                    .ext_installed_plus_builtin()
                    .into_iter()
                    .map(|r| {
                        format!(
                            "{}\t{}\t{}\t{:?}\t{}",
                            r.id,
                            r.name,
                            r.version,
                            r.kind,
                            if off.contains(&r.id) {
                                "disabled"
                            } else {
                                "enabled"
                            }
                        )
                    })
                    .collect();
                ok(format!(
                    "id\tname\tversion\tkind\tstate\n{}",
                    rows.join("\n")
                ))
            }
            "set_extension_enabled" => {
                let id = g("id");
                let on = matches!(
                    g("enabled").to_ascii_lowercase().as_str(),
                    "true" | "1" | "yes" | "on"
                );
                if !self.ext_installed_plus_builtin().iter().any(|r| r.id == id) {
                    err(format!("unknown extension '{id}' (use list_extensions)"))
                } else {
                    let cur = self
                        .settings
                        .get("extensions.disabled")
                        .unwrap_or("")
                        .to_string();
                    let next = crate::extensions::manager::list_toggle(&cur, &id, !on);
                    let _ = self.settings.set("extensions.disabled", &next);
                    let _ = self.settings.save();
                    self.apply_extensions(None);
                    ok(format!("{id}: {}", if on { "enabled" } else { "disabled" }))
                }
            }
            "describe_object" => {
                let schema = g("schema");
                let name = g("name");
                let s = (!schema.is_empty()).then_some(schema.as_str());
                // ★ 컬럼이 아직 안 읽힌 객체 = 그 객체 하나의 적재를 걸고 기다린다(11-14 · 지연 로딩 — 종전에는 "not loaded"를
                //   답해 어시스턴트가 `SELECT … WHERE 1=0`으로 컬럼을 알아냈다). 답은 `ai_tick_pending`이 보낸다.
                match self.explorer.ai_ensure_columns(s, &name) {
                    None => err("object not found in explorer; try search_objects".into()),
                    Some(false) if self.ai.pending.is_none() => {
                        self.ai.pending = Some(PendingTool {
                            reply: Some(reply),
                            attach_label: None,
                            since: Instant::now(),
                            max_rows: 0,
                            wait_editor: None,
                            wait_meta: Some((schema, name)),
                            wait_ui: None,
                            silent: false,
                            var_after: None,
                        });
                        return;
                    }
                    _ => match self.explorer.ai_describe(s, &name) {
                        Some(d) => ok(d),
                        None => err("object not found in explorer; try search_objects".into()),
                    },
                }
            }
            "list_profiles" => ok(crate::worker::profile_names().join("\n")),
            "skill" => match ai::skills::body(&g("name")) {
                Some(b) => ok(b.to_string()),
                None => err(format!(
                    "unknown skill; available: {}",
                    ai::skills::names().join(", ")
                )),
            },
            // ★ 앱 계층 세션 정보(docs/110 §15-12 · 사용자 11-11): 현재 편집기 세션 + 공유/전용 세션 전부(DBMS 질의 없음).
            "session_info" => {
                let mut out = format!(
                    "current: profile={} dialect={:?} connected={} schema={} db={} busy={} pending_tx={} desc={}\n",
                    self.sess.profile,
                    self.sess.dialect,
                    self.sess.connected,
                    self.sess.cur_schema.clone().unwrap_or_default(),
                    self.sess.current_db.clone().unwrap_or_default(),
                    self.sess.busy,
                    self.sess.tx_pending.len(),
                    self.sess.desc
                );
                out.push_str("sessions (Nexa SQL app level; shared = any tab, private = owned by one editor tab):\n");
                for r in self.session_rows() {
                    out.push_str(&format!(
                        "- #{} {}{} server={} connected={} broken={} busy={} idle={}s pending_tx={}\n",
                        r.id,
                        if r.shared { "shared" } else { "private" },
                        match &r.tab {
                            Some((_, title)) => format!(" (tab \"{title}\")"),
                            None => if r.active { " (default)".to_string() } else { String::new() },
                        },
                        r.server,
                        r.connected,
                        r.broken,
                        r.busy,
                        r.idle_secs,
                        r.pending
                    ));
                }
                ok(out)
            }
            "connect" if g("scope").eq_ignore_ascii_case("private") => {
                // ★ 전용(탭) 세션(사용자 11-11 "CONNECT APP_PRD; 명령만 실행되면 된다"): 편집기에서 친 것과 같은 길(`run_text_whole` →
                //   run.rs의 CONNECT 처리 = 이 탭의 전용 세션) · 새 API 없음 · 비밀번호는 호스트 안.
                let p = g("profile");
                match nsql_vault::Vault::open_default().and_then(|v| v.get(&p)) {
                    Ok(Some(spec)) if spec.password.as_deref().is_some_and(|w| !w.is_empty()) => {
                        if self.ai.pending.is_some() || !self.gate_open() {
                            err(format!("session busy: {}", self.sess.status))
                        } else {
                            self.run_text_whole(format!("CONNECT {p};"));
                            ok(format!(
                                "running `CONNECT {p};` in the active editor tab — it becomes that tab's private session; call session_info in a moment to confirm"
                            ))
                        }
                    }
                    Ok(Some(_)) => err(format!(
                        "profile '{p}' has no stored password (a private session needs one). Use scope=shared (connection window) or tell the user to type `CONNECT {p};` in the editor and log in there."
                    )),
                    Ok(None) => err(format!("unknown profile '{p}' (use list_profiles)")),
                    Err(e) => err(e.to_string()),
                }
            }
            "connect" => {
                let p = g("profile");
                match nsql_vault::Vault::open_default().and_then(|v| v.get(&p)) {
                    // 저장된 비밀번호가 있으면 로그인 목록 행 [Login]과 같은 길로 바로 접속(창 없음 · 비밀번호는 호스트 안).
                    Ok(Some(spec)) if spec.password.as_deref().is_some_and(|w| !w.is_empty()) => {
                        self.login_profile(&p);
                        ok(format!(
                            "connecting to '{p}' with the stored password (no window); call session_info in a moment to confirm connected=true"
                        ))
                    }
                    Ok(Some(_)) => {
                        self.startup_cmd(&format!("conn.edit:{p}"));
                        ok(format!(
                            "profile '{p}' has no stored password; the connection window is open with it preselected. Tell the user they must type the password and confirm the login in that window themselves (you cannot do it)."
                        ))
                    }
                    Ok(None) => err(format!("unknown profile '{p}' (use list_profiles)")),
                    Err(e) => err(e.to_string()),
                }
            }
            "get_output" => {
                let n = gi("max_lines", 100).clamp(1, 2000) as usize;
                match self.panel.output.as_ref().map(|v| v.text()) {
                    Some(text) if !text.trim().is_empty() => {
                        let lines: Vec<&str> = text.lines().collect();
                        let from = lines.len().saturating_sub(n);
                        ok(redact(&lines[from..].join("\n")))
                    }
                    _ => ok("(the Output tab is empty)".into()),
                }
            }
            "get_log" => {
                let n = gi("max_lines", 60).clamp(1, 2000) as usize;
                let only = matches!(g("errors_only").as_str(), "true" | "1");
                let text = self.log_win.dump_text_timed();
                let lines: Vec<&str> = text
                    .lines()
                    .filter(|l| !only || l.contains("|Error|"))
                    .collect();
                if lines.is_empty() {
                    ok(if only {
                        "(no errors in the log)".into()
                    } else {
                        "(the log is empty)".into()
                    })
                } else {
                    let from = lines.len().saturating_sub(n);
                    ok(redact(&lines[from..].join("\n")))
                }
            }
            "get_result" => match self.grid.ai_text(max_rows) {
                Some(s) => ok(s),
                None => err("no result grid".into()),
            },
            "run_query" | "explain" => {
                let sql = g("sql");
                if sql.trim().is_empty() {
                    let _ = reply.send(err("empty sql".into()));
                    return;
                }
                // ★ 세션이 바쁘면(앞선 도구 조회가 도는 중) 오류 대신 대기열에(11-12 "session busy: Running…" 관찰) — 끝나면 차례로.
                if self.sess.connected && (self.ai.pending.is_some() || self.sess.busy) {
                    self.ai
                        .tool_queue
                        .push_back((name.to_string(), input.to_string(), reply));
                    return;
                }
                // ★ 이중 안전장치: 비조회 SQL은 이 호출을 사용자가 카드로 허용한 기록(`write_ok`)이 있어야만 — 없으면 실행 거부 + 오류 줄.
                if name == "run_query" && !sql_is_readonly(&sql) {
                    let ok_rec = self.ai.write_ok.take();
                    if ok_rec.as_ref().is_none_or(|(n, i)| n != name || i != input) {
                        self.ai_push(ChatMsg::new(Kind::Error, t(Msg::AiWriteGuard)));
                        let _ = reply.send(err(
                            "refused: a data-changing statement needs the user's explicit approval card".into(),
                        ));
                        return;
                    }
                }
                if self.ai.pending.is_some() {
                    let _ = reply.send(err("another query is still running".into()));
                    return;
                }
                if !self.sess.connected {
                    let _ = reply.send(err("not connected".into()));
                    return;
                }
                if !self.gate_open() {
                    let _ = reply.send(err(format!("session busy: {}", self.sess.status)));
                    return;
                }
                let src = if name == "explain" {
                    nsql_script::plan::explain_script_opts(
                        self.sess.dialect,
                        &sql,
                        nsql_script::plan::PlanOptions { analyze: false },
                    )
                } else {
                    sql
                };
                self.ai.capture = None;
                self.ai.pending = Some(PendingTool {
                    reply: Some(reply),
                    attach_label: None,
                    since: Instant::now(),
                    max_rows,
                    wait_editor: None,
                    wait_meta: None,
                    wait_ui: None,
                    silent: true,
                    var_after: None,
                });
                // ★ 도구 조회 = 숨김 실행(결과는 AI에게만 · 그리드·결과 탭은 그대로) — 사용자에게 보일 최종 조회는 턴 끝의
                //   자동 표시(`ai.show_result`)나 코드 블록 ▦가 맡는다.
                // ★ 변경 문장은 사용자가 승인 카드(운영 대량·파괴 = 2단계)로 확인했다 → 사람 경로의 "같은 본문 재실행" 무장은 통과로
                //   친다(보류 커밋 · DDL 가드 · 객체 백업 · 읽기 전용 검사는 그대로 탄다).
                let is_write = name == "run_query" && !sql_is_readonly(&src);
                // 이 호출 전에 이미 돌던 객체 백업(사람이 시작)인가 — 아래 "백업 대기" 판정이 남의 백업을 내 것으로 착각하지 않게.
                let bk_before = self.objbk.is_some();
                if name == "explain" {
                    // ★ 실행 계획(사용자 11-12 "운영에서 EXPLAIN이 보호에 막힌다"): 계획만 뽑는다(문장은 실행하지 않음 · analyze 끔).
                    //   Oracle은 `BEGIN EXECUTE IMMEDIATE 'EXPLAIN PLAN FOR …'; END;` 블록이라 운영 2단 무장·보류 커밋에 걸렸다 →
                    //   둘 다 건너뛴다(읽기 전용 검사 · DDL 가드는 그대로). 계획 스크립트는 여러 문장이라 문장 단위로 보낸다.
                    self.prod_confirmed_once = true;
                    self.hold_skip_once = true;
                    self.run_text(src, 0, true);
                    self.hold_skip_once = false;
                } else {
                    if is_write {
                        self.prod_confirmed_once = true;
                    }
                    // ★ 스크립트 명령(`EXEC :V := …` · `VAR` · `DEFINE`)이나 여러 문장이면 문장 단위로 — 통째(whole) 실행은 본문을
                    //   한 항목으로 서버에 보내 변수 대입이 먹지 않았다(사용자 11-12 "아무 변화도 없어").
                    let items = nsql_script::split_script_in(&src, Some(self.sess.dialect));
                    let scripted = items.len() > 1
                        || items
                            .iter()
                            .any(|i| matches!(i.kind, nsql_script::ItemKind::Command(_)));
                    // 변수 대입·클라이언트 명령만이면 운영 보류 커밋(블록 보류)을 건너뛴다 — 트랜잭션이 없다(사용자 11-12).
                    let texts: Vec<String> = items.iter().map(|i| i.text.clone()).collect();
                    let assign_only = sessions::assign_only_items(&texts);
                    if assign_only {
                        self.hold_skip_once = true;
                    }
                    if scripted {
                        self.run_text(src, 0, true);
                    } else {
                        self.run_text_whole(src);
                    }
                    self.hold_skip_once = false;
                }
                self.prod_confirmed_once = false;
                // ★ 실행이 시작되지 못했으면(읽기 전용 · 보류 차단 · DDL 가드 · 백업 대기) 성공이 아니라 **실패로** 알린다 — 종전에는
                //   250 ms 뒤 상태줄 글이 ok=true로 가서 어시스턴트가 "실행됨"으로 읽었다.
                if !self.sess.busy {
                    // ★ `!bk_before`(11-14): 사람이 시작한 백업이 도는 중이면 이 실행은 "백업 중" 안내로 **거절**됐다 — 종전에는
                    //   그것을 내 백업 대기로 읽어, 백업이 끝난 뒤 사람 문장의 결과를 AI에게 "completed"로 답했다(실행 안 됐는데 성공).
                    if !bk_before && self.objbk_just_started() {
                        // ★ 객체 백업 뒤 실행(11-13): 답을 미룬다 — 백업이 끝나 재진입 실행이 끝나면 그 결과를, 백업이 실패하면
                        //   (운영 = 실행 막음) 그 이유를 같은 통로로 보낸다(`ai_objbk_result`). 그동안 다음 도구는 줄을 선다.
                        self.ai.objbk_wait = true;
                    } else if let Some(mut p) = self.ai.pending.take() {
                        if let Some(r) = p.reply.take() {
                            let _ = r.send(err(format!(
                                "not executed: {} (blocked by the app's protection - tell the user)",
                                redact(&self.sess.status)
                            )));
                        }
                    }
                }
                return;
            }
            // ★ 객체 소스(사용자 11-14): 기본은 **탭을 열지 않고** 글만 읽어 답한다 — 분석·검증 요청이 편집기 탭을 늘리지 않게.
            //   탭은 사용자가 열어 달라고 했을 때만(`open` = true · 종전 경로 그대로).
            "object_source" => {
                let schema = g("schema");
                let s = (!schema.is_empty()).then_some(schema.as_str());
                let open = matches!(
                    g("open").to_ascii_lowercase().as_str(),
                    "true" | "1" | "yes" | "on"
                );
                // 미접속(오프라인 칸)은 객체 조회보다 먼저 알린다(메타가 비어 "not found"로 답하던 것 · 협업 V1 11-14).
                if !self.explorer.ai_source_ready() {
                    let _ = reply.send(ToolReply {
                        output: "the object explorer is not connected; connect first".into(),
                        ok: false,
                    });
                    return;
                }
                match self.explorer.ai_object_info(s, &g("name")) {
                    Some(o) if !open => {
                        // 패키지는 스펙이 기본 · `part` = body면 본문(`PackageBody` · 소스 열기의 "본문 열기"와 같은 길).
                        let kind = if o.kind == nsql_catalog::ObjectKind::Package
                            && g("part").eq_ignore_ascii_case("body")
                        {
                            nsql_catalog::ObjectKind::PackageBody
                        } else {
                            o.kind
                        };
                        self.ai.source_got = None;
                        self.ai.source_wait =
                            Some((o.schema.clone(), o.name.clone(), kind, Instant::now()));
                        self.explorer.reload_source_for(None, &o, kind);
                        self.ai.pending = Some(PendingTool {
                            reply: Some(reply),
                            attach_label: None,
                            since: Instant::now(),
                            max_rows,
                            wait_editor: None,
                            wait_meta: None,
                            wait_ui: Some(AiWait::Source),
                            silent: false,
                            var_after: None,
                        });
                        return;
                    }
                    Some(o) => {
                        let before = self.editors.active_id();
                        self.explorer.open_source_of(&o);
                        self.ai.pending = Some(PendingTool {
                            reply: Some(reply),
                            attach_label: None,
                            since: Instant::now(),
                            max_rows,
                            wait_editor: Some(before),
                            wait_meta: None,
                            wait_ui: None,
                            silent: false,
                            var_after: None,
                        });
                        return;
                    }
                    None => err("object not found in explorer".into()),
                }
            }
            other => err(format!("unknown tool {other}")),
        };
        let _ = reply.send(r);
    }

    /// 결과셋이 올 때(run_events) — AI가 기다리면 글로 잡아 둔다.
    pub(crate) fn ai_capture_result(&mut self, rs: &nsql_core::ResultSet) {
        let Some(p) = &self.ai.pending else { return };
        if p.wait_editor.is_some() {
            return;
        }
        let max = p.max_rows;
        let mut out = String::new();
        out.push_str(
            &rs.columns
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>()
                .join("\t"),
        );
        out.push('\n');
        for row in rs.rows.iter().take(max) {
            out.push_str(
                &row.iter()
                    .map(|v| nsql_io::cell_text(v, "NULL"))
                    .collect::<Vec<_>>()
                    .join("\t"),
            );
            out.push('\n');
        }
        if rs.rows.len() > max {
            out.push_str(&format!(
                "… ({} rows total, {} shown)\n",
                rs.rows.len(),
                max
            ));
        }
        match &mut self.ai.capture {
            Some(c) => {
                c.push('\n');
                c.push_str(&out);
            }
            None => self.ai.capture = Some(out),
        }
    }

    /// 대기 도구 완료 판정(틱) — 세션이 한가해졌거나 소스 탭이 열렸다.
    fn ai_tick_pending(&mut self) -> bool {
        let Some(p) = &self.ai.pending else {
            return false;
        };
        let timeout = p.since.elapsed()
            > std::time::Duration::from_secs(self.settings.int("ai.timeout_secs").max(10) as u64);
        let done_text: Option<(String, bool)> = if let Some(w) = &p.wait_ui {
            match w {
                // 파일 검색: 워커가 끝나면(또는 상한 시간) 결과 줄을 답한다.
                AiWait::Search { limit } => (!self.search.searching() || timeout).then(|| {
                    (
                        self.search.ai_results(*limit),
                        self.search.ai_error().is_none(),
                    )
                }),
                // 탭 없이 읽는 객체 소스: 가로챈 글이 오면 답한다 · 안 오면(탐색기 요청이 밀려남 등) 시간 초과로 실패.
                AiWait::Source => self.ai.source_got.take().or_else(|| {
                    timeout.then(|| ("timed out reading the source".to_string(), false))
                }),
                // 객체 탐색기 이동: 단계 기계가 끝나면 선택된 경로를 답한다(이름이 경로에 없으면 실패로).
                AiWait::Reveal { name } => (!self.explorer.reveal_pending()
                    || p.since.elapsed() > std::time::Duration::from_secs(12))
                .then(|| {
                    let path = self
                        .explorer
                        .selected_path()
                        .lines()
                        .next()
                        .unwrap_or("")
                        .to_string();
                    if path.to_lowercase().contains(&name.to_lowercase()) {
                        (format!("selected in the object explorer: {path}"), true)
                    } else {
                        (
                            format!(
                                "could not reveal {name}: {} (selected now: {path})",
                                redact(&self.sess.status)
                            ),
                            false,
                        )
                    }
                }),
            }
        } else if let Some((schema, name)) = &p.wait_meta {
            // 메타 적재 대기(describe_object): 컬럼이 읽혔거나(오류 포함) 8초가 지나면 지금 아는 만큼 답한다.
            let s = (!schema.is_empty()).then_some(schema.as_str());
            if self.explorer.ai_columns_settled(s, name)
                || p.since.elapsed() > std::time::Duration::from_secs(8)
            {
                Some(match self.explorer.ai_describe(s, name) {
                    Some(d) => (d, true),
                    None => (
                        "object not found in explorer; try search_objects".to_string(),
                        false,
                    ),
                })
            } else {
                None
            }
        } else if let Some(before) = p.wait_editor {
            let cur = self.editors.active_id();
            let text = self.editors.cur().text();
            if cur != before && !text.trim().is_empty() {
                Some((text, true))
            } else if timeout {
                Some(("timed out opening the source".into(), false))
            } else {
                None
            }
        } else if self.ai.objbk_wait || self.objbk.is_some() {
            // (`objbk.is_some()` = 코드 블록 ▶·▦ 실행이 백업으로 미뤄진 경우 — 종전에는 250 ms 뒤 백업 대기 상태 글이 블록
            //  결과로 붙었다 · 11-14.)
            // 객체 백업 뒤 실행을 기다리는 중 — 완료·시간 초과 판정을 멈춘다(백업은 대상마다 자체 상한이 있다 · `ai_objbk_result`가 푼다).
            None
        } else if !self.sess.busy && p.since.elapsed() > std::time::Duration::from_millis(250) {
            if self.sess.run_had_err {
                Some((redact(&self.sess.status), false))
            } else {
                let done = self.ai.capture.take().unwrap_or_else(|| {
                    // 결과 집합이 없는 실행(변수 대입 · DDL …) — "실행 중…" 같은 지난 상태 글 대신 완료를 알린다.
                    let st = redact(&self.sess.status);
                    if st.trim_end().ends_with('…') || st.trim().is_empty() {
                        "completed (no result set)".to_string()
                    } else {
                        format!("completed: {st}")
                    }
                });
                match p.var_after.as_deref().map(|n| self.ai_var_get(n)) {
                    Some(Ok(now)) => {
                        let note = if self.settings.get("vars.expand_at") == Some("use") {
                            ""
                        } else {
                            "\nnote: the expression was evaluated once and stored as a plain value - formulas are only remembered when the setting vars.expand_at is 'use' (default 'assign'). Tell the user this and ask whether to turn it on; if yes: set_setting vars.expand_at = use, then call set_var with the formula again."
                        };
                        Some((format!("set.\n{now}{note}"), true))
                    }
                    Some(Err(e)) => Some((e, false)),
                    None => Some((done, true)),
                }
            }
        } else if timeout {
            Some(("query timed out".into(), false))
        } else {
            None
        };
        let Some((text, ok)) = done_text else {
            return false;
        };
        let p = self.ai.pending.take().expect("pending");
        if let Some((mi, block)) = self.ai.run_block.take() {
            if let Some(m) = self.ai_cur_mut().and_then(|s| s.messages.get_mut(mi)) {
                m.block_results.retain(|r| r.0 != block);
                m.block_results.push((block, text, ok));
            }
            self.ai.status = t(if ok { Msg::AiRunDone } else { Msg::AiRunFailed }).into();
            self.redraw();
            return true;
        }
        if let Some((name, input, reply, two)) = self.ai.impact_wait.take() {
            let count = if ok {
                ai::guard::parse_count(&text)
            } else {
                None
            };
            self.ai_impact_done(name, input, reply, two, count);
            return true;
        }
        if let Some(r) = p.reply {
            let _ = r.send(ToolReply { output: text, ok });
        } else if let Some(label) = p.attach_label {
            if ok {
                if let Some(s) = self.ai_run_mut() {
                    s.attachments.push(Attachment::Text { label, body: text });
                }
                self.ai.status = t(Msg::AiAttached).into();
            } else {
                self.ai_push(ChatMsg::new(Kind::Error, text));
            }
        }
        true
    }

    // ───────────────────────── 첨부 · `/` 명령 ─────────────────────────

    /// ★ AI 작성 상자에 붙여넣는 글이 `ai.paste_chip_chars`자 이상이면 첨부 칩으로(0 = 끔 · 110 §14-4). 가로챘으면 `true`.
    pub(crate) fn ai_paste_as_chip(&mut self, text: &str) -> bool {
        if !((self.ai_view_active() || self.ai.side_focus)
            && self.focus == Focus::Editor
            && self.ai_view.page() == Page::Chat)
        {
            return false;
        }
        let limit = self.settings.int("ai.paste_chip_chars").max(0) as usize;
        if limit == 0 || text.chars().count() < limit {
            return false;
        }
        self.ai_init();
        if self.ai.cur.is_none() {
            self.ai_new_session();
        }
        {
            // 비밀은 가리고 첨부(거부하지 않는다 · 가린 곳 수를 안내 · 11-11).
            let (body, hidden) = ai::redact_count(text);
            let n = body.lines().count();
            if let Some(s) = self.ai_cur_mut() {
                s.attachments.push(Attachment::Text {
                    label: format!("pasted ({n} lines)"),
                    body,
                });
            }
            self.ai.status = t(Msg::AiAttached).into();
            self.ai_note_masked(hidden);
        }
        self.ai_sync_panel();
        self.redraw();
        true
    }

    /// ★ AI 작성 상자에서 Ctrl+V — 클립보드에 글이 없고 이미지가 있으면 이미지 칩으로. 가로챘으면 `true`.
    pub(crate) fn ai_paste_image(&mut self) -> bool {
        if !((self.ai_view_active() || self.ai.side_focus)
            && self.focus == Focus::Editor
            && self.ai_view.page() == Page::Chat)
        {
            return false;
        }
        let Some(png) = crate::clipboard::read_image_png() else {
            return false;
        };
        self.ai_init();
        if self.ai.cur.is_none() {
            self.ai_new_session();
        }
        if let Some(s) = self.ai_cur_mut() {
            // 고정 이름 + 형식 확장자(사용자 11-11 "clipboard.png처럼 · 크기는 칩에 · 용량은 미리보기에만").
            s.attachments.push(Attachment::Image {
                label: "clipboard.png".into(),
                media: "image/png".into(),
                bytes: png,
            });
        }
        self.ai.status = t(Msg::AiAttached).into();
        self.ai_sync_panel();
        self.redraw();
        true
    }

    fn ai_attach_clipboard(&mut self) {
        match crate::clipboard::read_text() {
            Some(t) if !t.trim().is_empty() => {
                let (t, hidden) = ai::redact_count(&t);
                self.ai_note_masked(hidden);
                let n = t.lines().count();
                if let Some(s) = self.ai_cur_mut() {
                    s.attachments.push(Attachment::Text {
                        label: format!("clipboard ({n} lines)"),
                        body: t,
                    });
                }
                self.ai.status = nsql_i18n::t(Msg::AiAttached).into();
            }
            _ => match crate::clipboard::read_image_png() {
                Some(png) => {
                    if let Some(s) = self.ai_cur_mut() {
                        s.attachments.push(Attachment::Image {
                            label: "clipboard.png".into(),
                            media: "image/png".into(),
                            bytes: png,
                        });
                    }
                    self.ai.status = nsql_i18n::t(Msg::AiAttached).into();
                }
                None => self.ai.status = nsql_i18n::t(Msg::AiClipboardEmpty).into(),
            },
        }
    }

    /// 파일 창에서 고른 파일 첨부(이미지 · PDF · 글 ≤ 512 KB).
    pub(crate) fn ai_attach_file(&mut self, path: &std::path::Path) {
        let label = path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.ai_push(ChatMsg::new(Kind::Error, e.to_string()));
                return;
            }
        };
        let a = match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" => Attachment::Image {
                label,
                media: match ext.as_str() {
                    "png" => "image/png",
                    "gif" => "image/gif",
                    "webp" => "image/webp",
                    _ => "image/jpeg",
                }
                .into(),
                bytes,
            },
            "pdf" => Attachment::Pdf { label, bytes },
            _ => {
                if bytes.len() > 512 * 1024 {
                    self.ai_push(ChatMsg::new(Kind::Error, t(Msg::AiFileTooBig)));
                    self.ai.status = t(Msg::AiFileTooBig).into();
                    self.redraw();
                    return;
                }
                // 비밀(접속 문자열 비밀번호 · `password=` · API 키)은 가리고 첨부 — 종전엔 거부만 해 "첨부가 안 된다"로 보였다(11-11).
                let (body, hidden) = ai::redact_count(&String::from_utf8_lossy(&bytes));
                self.ai_note_masked(hidden);
                Attachment::Text { label, body }
            }
        };
        let n = match self.ai_cur_mut() {
            Some(s) => {
                s.attachments.push(a);
                s.attachments.len()
            }
            None => 0,
        };
        if std::env::var_os("NSQL_TRACE_AI").is_some() {
            eprintln!("[ai] attach {} -> attachments={n}", path.display());
        }
        self.ai.status = if n == 0 {
            t(Msg::AiNoProfile).into()
        } else {
            t(Msg::AiAttached).into()
        };
        self.redraw();
    }

    /// `/status` 글: 세션 · 제공자/모드 · 모델(자동 이름 → 실제 id) · 노력도 · Auto · 메시지/첨부/행 데이터 · 사용량 · 턴.
    fn ai_status_text(&self) -> String {
        let Some(s) = self.ai_cur() else {
            return t(Msg::AiNoProfile).to_string();
        };
        let fetched = self.ai_session_models(s.provider, s.mode, &s.profile);
        let resolved = ai::resolve_model(s.provider, &s.model, &fetched);
        let prov = format!(
            "{} · {}{}",
            match s.provider {
                Provider::Claude => t(Msg::AiProviderClaude),
                Provider::Codex => t(Msg::AiProviderCodex),
            },
            match (s.mode, s.provider) {
                (Mode::Api, _) => t(Msg::AiModeApi),
                (Mode::Cli, Provider::Claude) => t(Msg::AiModeCliClaude),
                (Mode::Cli, Provider::Codex) => t(Msg::AiModeCliCodex),
            },
            if s.profile.is_empty() {
                String::new()
            } else {
                format!(" · {}", s.profile)
            }
        );
        let busy = self.ai.host.as_ref().is_some_and(|h| h.busy);
        let turn = match self.ai.turn_started {
            Some(t0) if busy => tf(
                Msg::AiTurnRunning,
                &[&self.ai.turn_wall, &t0.elapsed().as_secs().to_string()],
            ),
            _ => t(Msg::AiTurnIdle).to_string(),
        };
        let onoff = |b: bool| if b { "on" } else { "off" };
        let u = &s.usage;
        let turn = format!(
            "{turn}\n{}: {} · agent {}",
            t(Msg::AiDeliveryLabel),
            ai::bridge::delivery(s.provider, s.mode).describe(),
            s.agent
        );
        tf(
            Msg::AiStatusText,
            &[
                &s.title_or_default(),
                &s.id,
                &prov,
                &s.model,
                &resolved,
                &s.effort,
                onoff(s.auto),
                &s.messages.len().to_string(),
                &s.attachments.len().to_string(),
                onoff(s.rows_allowed),
                &u.input.to_string(),
                &u.output.to_string(),
                &u.cache_read.to_string(),
                &u.cache_write.to_string(),
                &format!("{:.4}", u.cost_usd),
                &turn,
            ],
        )
    }

    /// `/account` 글: 현재 API 프로필(인증 · 엔드포인트 · 워크스페이스 · 검증 때 받은 모델) · Claude CLI 키 출처/프로그램 ·
    /// Codex CLI 로그인 줄 + 계정 모델. 비밀은 없다(키는 "저장됨"만).
    fn ai_account_text(&self) -> String {
        let dash = || "-".to_string();
        let (name, auth, endpoint, ws, models) = match self.ai.store.current() {
            Some(p) => (
                p.name.clone(),
                if let Some(w) = &p.wif {
                    format!("WIF · {}", w.issuer)
                } else if self.ai_key_exists(&p.name) {
                    format!("{} (••••)", t(Msg::AiKeyStored))
                } else {
                    t(Msg::AiKeyMissing).to_string()
                },
                p.base_url.clone(),
                if p.workspace.is_empty() {
                    dash()
                } else {
                    p.workspace.clone()
                },
                if p.models.is_empty() {
                    dash()
                } else {
                    p.models.join(", ")
                },
            ),
            None => (
                dash(),
                t(Msg::AiNoProfile).to_string(),
                dash(),
                dash(),
                dash(),
            ),
        };
        let setting = |k: &str, d: &str| {
            let v = self.settings.get(k).unwrap_or("");
            if v.is_empty() {
                d.to_string()
            } else {
                v.to_string()
            }
        };
        let cli_key = setting("ai.cli_key", "login");
        let cli_path = setting("ai.cli_path", "claude");
        let codex = self
            .ai
            .codex_login
            .clone()
            .filter(|l| !l.is_empty())
            .unwrap_or_else(dash);
        let codex_models = if self.ai.codex_cli_models.is_empty() {
            dash()
        } else {
            self.ai.codex_cli_models.join(", ")
        };
        tf(
            Msg::AiAccountText,
            &[
                &name,
                &auth,
                &endpoint,
                &ws,
                &models,
                &cli_key,
                &cli_path,
                &codex,
                &codex_models,
            ],
        )
    }

    fn ai_slash(&mut self, line: &str) {
        let (cmd, arg) = line
            .split_once(' ')
            .map(|(c, a)| (c, a.trim()))
            .unwrap_or((line, ""));
        match cmd {
            "/clear" => {
                self.ai_new_session();
            }
            // ★ 세션 이름 바꾸기(사용자 11-14 "/rename") — 세션 목록에서 이름을 고친 것과 같은 길(`PanelAction::RenameSession`):
            //   기록 저장 · 채팅 탭 제목 · 세션 목록에 바로 반영. 이름을 안 주면 쓰는 법만 알린다.
            "/rename" => {
                let title: String = arg.chars().take(80).collect();
                match self.ai_cur().map(|s| s.id.clone()) {
                    Some(id) if !title.trim().is_empty() => {
                        self.ai_rename_session(&id, title.trim());
                        self.ai_sync_panel();
                        self.ai.status = tf(Msg::AiRenamed, &[title.trim()]);
                    }
                    _ => self.ai.status = t_(Msg::AiRenameUsage),
                }
            }
            "/mode" => {
                let m = Mode::parse(arg);
                self.ai_panel_actions_apply(PanelAction::SetMode(m));
            }
            "/model" if !arg.is_empty() => {
                self.ai_panel_actions_apply(PanelAction::SetModel(arg.into()))
            }
            "/effort" if !arg.is_empty() => {
                self.ai_panel_actions_apply(PanelAction::SetEffort(arg.into()))
            }
            "/auto" => {
                let on = arg.is_empty() || matches!(arg, "on" | "1" | "true");
                self.ai_panel_actions_apply(PanelAction::SetAuto(on));
            }
            "/rows" => {
                let on = arg.is_empty() || matches!(arg, "on" | "1" | "true");
                if let Some(s) = self.ai_cur_mut() {
                    s.rows_allowed = on;
                }
                self.ai_push(ChatMsg::new(
                    Kind::Note,
                    t(if on { Msg::AiRowsOn } else { Msg::AiRowsOff }),
                ));
            }
            "/attach" => {
                self.file_purpose = FilePurpose::AiAttach;
                self.open_file_dlg = Some(nexa_dlg::PickerMode::Open);
            }
            "/clip" => self.ai_attach_clipboard(),
            // ★ 결과 그리드 선택 영역(없으면 보이는 결과 전체 · Markdown 표 · 행 상한 `ai.max_rows`).
            "/grid" if self.ai_view_active() => {
                // 독립 채팅 탭에는 결과 그리드가 없다(11-14).
                self.ai.status = t_(Msg::AiNeedsEditorTab);
            }
            "/grid" => {
                let max = self.settings.int("ai.max_rows").max(1) as usize;
                let text = self
                    .grid
                    .copy_selection(crate::grid::CopyKind::Markdown)
                    .map(|(t, _)| t)
                    .or_else(|| self.grid.ai_text(max));
                match text {
                    Some(t) if !t.trim().is_empty() => {
                        let lines: Vec<&str> = t.lines().take(max + 2).collect();
                        let body = lines.join(
                            "
",
                        );
                        let n = lines.len().saturating_sub(2);
                        if let Some(s) = self.ai_cur_mut() {
                            s.rows_allowed = true;
                            s.attachments.push(Attachment::Text {
                                label: format!("grid ({n} rows)"),
                                body,
                            });
                        }
                        self.ai.status = t_(Msg::AiAttached);
                    }
                    _ => self.ai_push(ChatMsg::new(Kind::Error, t_(Msg::AiGridEmpty))),
                }
            }
            "/profiles" => {
                self.ai_view.show_page(Page::Profiles);
            }
            // ★ `/status` · `/account`(사용자 11-11 "상태·계정 정보도 볼 수 있게"): 앱이 아는 것만(네트워크 0).
            "/status" => {
                let text = self.ai_status_text();
                self.ai_push(ChatMsg::new(Kind::Note, text));
            }
            "/account" => {
                let text = self.ai_account_text();
                self.ai_push(ChatMsg::new(Kind::Note, text));
            }
            // ★ `/agent`(agents.rs · 11-11): 목록 · `/agent <이름>` = 이 세션의 역할 프리셋(지시 + 도구 허용).
            "/agent" => {
                let text = if arg.is_empty() {
                    let cur = self.ai_cur().map(|s| s.agent.clone()).unwrap_or_default();
                    let mut t_ = format!("{} {cur}\n", t(Msg::AiAgentCurrent));
                    for a in ai::agents::AGENTS {
                        t_.push_str(&format!("- {}: {}\n", a.name, a.desc));
                    }
                    t_
                } else if let Some(a) = ai::agents::get(arg) {
                    if let Some(s) = self.ai_cur_mut() {
                        s.agent = a.name.to_string();
                    }
                    tf(Msg::AiAgentSet, &[a.name, a.desc])
                } else {
                    format!(
                        "{} {}",
                        t(Msg::AiAgentUnknown),
                        ai::agents::names().join(", ")
                    )
                };
                self.ai_push(ChatMsg::new(Kind::Note, text));
            }
            // ★ `/skills`(사용자 11-11): 색인(이름 — 언제) · `/skills <이름>` = 플레이북 본문 — 모델이 받는 글 그대로(영어).
            "/skills" => {
                let text = if arg.is_empty() {
                    format!("{}\n\n{}", ai::skills::index_text(), t(Msg::AiSkillsHint))
                } else {
                    match ai::skills::body(arg) {
                        Some(b) => b.to_string(),
                        None => format!(
                            "{} {}",
                            t(Msg::AiSkillUnknown),
                            ai::skills::names().join(", ")
                        ),
                    }
                };
                self.ai_push(ChatMsg::new(Kind::Note, text));
            }
            "/query" => {
                if arg.is_empty() {
                    self.ai_push(ChatMsg::new(Kind::Error, t(Msg::AiQueryUsage)));
                } else if !sql_is_readonly(arg) {
                    self.ai_push(ChatMsg::new(Kind::Error, t(Msg::AiQueryReadOnly)));
                } else if !self.sess.connected || !self.gate_open() || self.ai.pending.is_some() {
                    self.ai_push(ChatMsg::new(Kind::Error, t(Msg::AiQueryBusy)));
                } else {
                    let max_rows = self.settings.int("ai.max_rows").max(1) as usize;
                    self.ai.capture = None;
                    self.ai.pending = Some(PendingTool {
                        reply: None,
                        attach_label: Some(format!(
                            "query: {}",
                            arg.chars().take(60).collect::<String>()
                        )),
                        since: Instant::now(),
                        max_rows,
                        wait_editor: None,
                        wait_meta: None,
                        wait_ui: None,
                        silent: false,
                        var_after: None,
                    });
                    if let Some(s) = self.ai_cur_mut() {
                        s.rows_allowed = true;
                    }
                    self.run_text_whole(arg.to_string());
                    self.ai.status = "…".into();
                }
            }
            _ => {
                let mut text = String::new();
                for (n, has_arg, d) in crate::ai::view::SLASH {
                    text.push_str(&format!(
                        "{n}{}  — {}\n",
                        if *has_arg { " <arg>" } else { "" },
                        t(*d)
                    ));
                }
                self.ai_push(ChatMsg::new(Kind::Note, text));
            }
        }
        self.ai_sync_panel();
        self.redraw();
    }

    /// 세션 이름 바꾸기(세션 목록의 이름 고치기 · `/rename` 공용): 기록 저장 + 채팅 탭 제목.
    fn ai_rename_session(&mut self, id: &str, title: &str) {
        if let Some(s) = self.ai.sessions.iter_mut().find(|s| s.id == id) {
            s.title = title.to_string();
            ai::history_save(s);
            let key = Self::ai_view_key(s.provider, id);
            self.editors.set_view_title(&key, title);
        }
    }

    fn ai_panel_actions_apply(&mut self, a: PanelAction) {
        match a {
            PanelAction::SetMode(m) => {
                let k = self.ai_pref_key("mode");
                let _ = self.settings.set(&k, m.as_str());
                self.persist_settings();
                if let Some(s) = self.ai_cur_mut() {
                    s.mode = m;
                    s.cli_session = None;
                }
                if let Some(h) = &mut self.ai.host {
                    h.stop();
                }
            }
            PanelAction::SetModel(m) => {
                let k = self.ai_pref_key("model");
                let _ = self.settings.set(&k, &m);
                self.persist_settings();
                if let Some(s) = self.ai_cur_mut() {
                    s.model = m;
                }
            }
            PanelAction::SetEffort(e) => {
                let k = self.ai_pref_key("effort");
                let _ = self.settings.set(&k, &e);
                self.persist_settings();
                if let Some(s) = self.ai_cur_mut() {
                    s.effort = e;
                }
            }
            PanelAction::SetAuto(on) => {
                let k = self.ai_pref_key("auto");
                let _ = self.settings.set(&k, if on { "on" } else { "off" });
                self.persist_settings();
                if let Some(s) = self.ai_cur_mut() {
                    s.auto = on;
                }
            }
            _ => {}
        }
    }

    // ───────────────────────── 헤드리스 명령(V1) ─────────────────────────

    /// `ai.open` · `ai.panel` · `ai.send:<글>` · `ai.dump:<파일>` · `ai.approve:<yes|no>`. 처리했으면 true.
    pub(crate) fn ai_startup_cmd(&mut self, id: &str) -> bool {
        if id == "ai.open" {
            if !self.ai_enabled() {
                let _ = self.settings.set("ai.enabled", "on");
                self.ai_apply_enabled();
            }
            if !self.ai_panel.is_visible() {
                let p = self.ai_provider_default();
                self.ai_toggle_panel(p);
            } else {
                self.ai_open_chat();
            }
            return true;
        }
        if id == "ai.panel" {
            let p = self.ai.tab;
            self.ai_toggle_panel(p);
            return true;
        }
        if let Some(p) = id.strip_prefix("ai.tab:") {
            self.ai_toggle_panel(Provider::parse(p));
            return true;
        }
        if let Some(text) = id.strip_prefix("ai.send:") {
            self.ai_open_chat();
            if text.starts_with('/') {
                self.ai_slash(text.trim());
            } else {
                self.ai_send(text);
            }
            return true;
        }
        // 파일 첨부(파일 창 단계를 건너뛰고 확정 뒤 경로만 · 사용자 실기 "첨부 안 됨" 판정용 · 협업 제안 11-11).
        if let Some(path) = id.strip_prefix("ai.attach:") {
            self.ai_init();
            if self.ai.cur.is_none() {
                self.ai_new_session();
            }
            self.ai_attach_file(std::path::Path::new(path));
            return true;
        }
        if let Some(a) = id.strip_prefix("ai.approve:") {
            self.ai_approve(a == "yes");
            return true;
        }
        if let Some(path) = id.strip_prefix("ai.dump:") {
            let mut s = self.ai_panel.dump_text();
            if let Some(sess) = self.ai_cur() {
                s.push_str(&self.ai_view.dump_text(sess, &self.ai_view_info()));
            }
            s.push_str(&format!(
                "pending={} approval={}\n",
                self.ai.pending.is_some(),
                self.ai.approval.is_some()
            ));
            let _ = std::fs::write(path, s);
            return true;
        }
        false
    }
}

impl App {
    // ───────────────────────── 입력 라우팅(input.rs 선례 = 아웃라인 패널) ─────────────────────────

    /// 패널: 포인터는 안일 때 · 키는 포커스일 때. 가져갔으면 true.
    pub(crate) fn route_ai_panel(&mut self, ev: InputEvent, is_ptr: bool) -> bool {
        if !self.ai_panel.is_visible() {
            return false;
        }
        let cur = Point {
            x: self.cursor.0,
            y: self.cursor.1,
        };
        let inside = self.ai_panel.bounds().contains(cur);
        if (is_ptr && inside) || (is_wheel_ev(&ev) && inside) {
            if matches!(
                ev,
                InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
            ) {
                self.set_focus(Focus::Ai);
            }
            if self.ai_panel.on_event(&ev) {
                self.redraw();
            }
            self.ime_refresh();
            self.ai_panel_actions();
            if !matches!(ev, InputEvent::MouseMove { .. }) {
                return true;
            }
        } else if self.focus == Focus::Ai
            && matches!(
                ev,
                InputEvent::Key { .. }
                    | InputEvent::Char { .. }
                    | InputEvent::SelectAll
                    | InputEvent::Undo
                    | InputEvent::Redo
            )
        {
            let handled = self.ai_panel.on_event(&ev);
            self.ime_refresh();
            if !handled
                && matches!(
                    ev,
                    InputEvent::Key {
                        key: CtlKey::Escape,
                        ..
                    }
                )
            {
                self.set_focus(Focus::Editor);
                self.redraw();
                return true;
            }
            if handled {
                self.redraw();
            }
            self.ai_panel_actions();
            return true;
        }
        false
    }

    /// 패널의 드롭 목록·우클릭 메뉴가 열려 있으면 먼저 받는다(바깥 클릭 = 닫고 통과).
    pub(crate) fn route_ai_menu(&mut self, ev: InputEvent) -> bool {
        if self.ai_panel.is_visible() && self.ai_panel.menu_open() {
            let outside_click = matches!(
                ev,
                InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
            ) && !self.ai_panel.menu_bounds().contains(Point {
                x: self.cursor.0,
                y: self.cursor.1,
            });
            if self.ai_panel.on_event(&ev) {
                self.redraw();
            }
            // ★ 우클릭 메뉴 → [이름 바꾸기]로 입력 상자가 생기면 IME를 바로 켠다(종전엔 안 켜져 한글이 자모로 갈라짐 · 사용자 11-11).
            self.ime_refresh();
            self.ai_panel_actions();
            if (!outside_click || self.ai_panel.menu_open())
                && !matches!(ev, InputEvent::MouseMove { .. })
            {
                return true;
            }
        }
        false
    }

    /// 채팅 뷰 탭(확장 뷰와 같은 규칙): 포인터는 편집기 자리 안일 때 · 키는 편집기 포커스일 때.
    pub(crate) fn route_ai_view(&mut self, ev: InputEvent, is_mouse: bool) -> bool {
        let tab = self.ai_view_active();
        let side = !tab && self.ai.side_rect.w > 0;
        if !tab && !side {
            return false;
        }
        let cur = Point {
            x: self.cursor.0,
            y: self.cursor.1,
        };
        let in_view = if side {
            self.ai.side_rect.contains(cur)
        } else {
            self.editors.editor_bounds().contains(cur)
        };
        // 분할 창: 안을 누르면 키 포커스가 창으로 · 밖을 누르면 편집기로(사건은 그대로 통과).
        if side
            && matches!(
                ev,
                InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
            )
        {
            let was = self.ai.side_focus;
            self.ai.side_focus = in_view || self.ai_view.popup_open();
            if was != self.ai.side_focus {
                // 포커스 판정 한 곳: 분할 창이 키를 받으면 작성 상자만 깜빡이고 편집기는 고정 빔(그 반대도 · 11-11).
                self.ai_view.set_focused(self.ai.side_focus);
                let ed_on = self.focus == Focus::Editor && !self.ai.side_focus;
                self.ed_mut().set_focused(ed_on);
                self.redraw();
            }
        }
        let pointer = is_mouse || is_wheel_ev(&ev);
        let popup = self.ai_view.popup_open();
        // ★ 끌기 중(대본 선택 · 스크롤바)이면 영역 밖의 이동·놓기도 뷰가 받는다 — 종전엔 밖에서 놓으면 놓기가 오지 않아
        //   끌기 상태가 남았다(사용자 11-12 "Capture 상태로 남는다").
        let drag_follow = self.ai_view.drag_active()
            && matches!(
                ev,
                InputEvent::MouseMove { .. } | InputEvent::MouseUp { .. }
            );
        if pointer && (in_view || popup || drag_follow) {
            if matches!(
                ev,
                InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
            ) {
                self.set_focus(Focus::Editor);
            }
            if self.ai_view.on_event(&ev) {
                self.redraw();
            }
            self.ime_refresh();
            self.ai_view_actions();
            return true;
        }
        if !pointer && self.focus == Focus::Editor && (tab || self.ai.side_focus) {
            if self.ai_view.on_event(&ev) {
                self.redraw();
            }
            self.ime_refresh();
            self.ai_view_actions();
            self.ai_prewarm();
            return true;
        }
        false
    }
}

fn t_(m: Msg) -> String {
    t(m).to_string()
}

/// 읽기 전용 문장인가(SELECT/WITH/SHOW/EXPLAIN/DESC/VALUES · 한 문장).
/// ★ UPDATE/DELETE 문장에 WHERE가 없는가(승인 카드 경고 · 데이터 보호 불변식 취지 · 11-12): 문장별로 첫 낱말이 UPDATE/DELETE이고
/// `where` 낱말이 없으면 true(문자열 안의 where까지 세는 보수적 판정 = 경고가 덜 뜨는 쪽 · 실행 차단은 아니다).
pub(crate) fn sql_dml_without_where(sql: &str) -> bool {
    for item in nsql_script::split_script_in(sql, None) {
        let body = item.text.trim();
        let mut words = body.split_whitespace();
        let Some(first) = words.next() else { continue };
        let first = first.trim_start_matches('(').to_ascii_lowercase();
        if first != "update" && first != "delete" {
            continue;
        }
        let low = body.to_ascii_lowercase();
        let has_where = low
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|w| w == "where");
        if !has_where {
            return true;
        }
    }
    false
}

pub(crate) fn sql_is_readonly(sql: &str) -> bool {
    let items = nsql_script::split_script_in(sql, None);
    if items.len() > 1 {
        return false;
    }
    let s = sql.trim_start_matches(|c: char| c.is_whitespace() || c == '(');
    // 선행 주석 제거.
    let mut s = s;
    loop {
        if let Some(rest) = s.strip_prefix("--") {
            s = rest
                .split_once('\n')
                .map(|(_, r)| r)
                .unwrap_or("")
                .trim_start();
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest
                .split_once("*/")
                .map(|(_, r)| r)
                .unwrap_or("")
                .trim_start();
        } else {
            break;
        }
    }
    let up = s.to_ascii_uppercase();
    let first = up
        .split(|c: char| !c.is_ascii_alphabetic())
        .next()
        .unwrap_or("");
    matches!(
        first,
        "SELECT" | "WITH" | "SHOW" | "EXPLAIN" | "DESC" | "DESCRIBE" | "VALUES" | "PRAGMA"
    ) && !up.contains(" INTO ")
        && !up.contains("FOR UPDATE")
}

/// ★ `list_files` 도우미(11-11): 작업 루트 아래를 **제한적으로** 걷는다(숨김·`target`·`node_modules` 제외 · 깊이 6 · 5,000개 상한) ·
/// 낱말 AND(상대 경로 소문자) · `상대경로<TAB>바이트`. 질의가 비면 앞 항목들.
fn ai_list_files(root: &std::path::Path, query: &str, limit: usize) -> Vec<String> {
    let words: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let mut out = Vec::new();
    let mut stack: Vec<(std::path::PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let mut scanned = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            scanned += 1;
            if scanned > 5000 || out.len() >= limit {
                return out;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                if depth < 6 {
                    stack.push((p, depth + 1));
                }
                continue;
            }
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let low = rel.to_lowercase();
            if words.iter().all(|w| low.contains(w.as_str())) {
                let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                out.push(format!("{rel}\t{size}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {

    #[test]
    fn dml_without_where_detects_update_delete_only() {
        assert!(super::sql_dml_without_where("DELETE FROM t"));
        assert!(super::sql_dml_without_where("update t set a = 1"));
        assert!(!super::sql_dml_without_where("DELETE FROM t WHERE id = 1"));
        assert!(!super::sql_dml_without_where(
            "UPDATE t SET a = 1 WHERE b = 2"
        ));
        assert!(!super::sql_dml_without_where("SELECT * FROM t"));
        assert!(super::sql_dml_without_where("SELECT 1; DELETE FROM t"));
    }
    use super::sql_is_readonly;

    #[test]
    fn readonly_detection() {
        assert!(sql_is_readonly("select 1 from dual"));
        assert!(sql_is_readonly(
            "  -- note\nWITH x AS (select 1) select * from x"
        ));
        assert!(sql_is_readonly("(select 1)"));
        assert!(!sql_is_readonly("delete from t"));
        assert!(!sql_is_readonly("select * into t2 from t"));
        assert!(!sql_is_readonly("select 1; drop table t"));
        assert!(!sql_is_readonly("select * from t for update"));
    }
}
