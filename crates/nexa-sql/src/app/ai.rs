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
    },
    /// 행 데이터 동의만(허용 = `rows_allowed` 뒤 도구 실행).
    Rows {
        name: String,
        input: String,
        reply: mpsc::Sender<ToolReply>,
    },
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
}

#[derive(Default)]
pub(crate) struct AiState {
    pub host: Option<Host>,
    pub sessions: Vec<Session>,
    pub cur: Option<usize>,
    pub store: ProfileStore,
    pub mcp: Option<ai::mcp::McpServer>,
    pub approval: Option<(String, String, String, ApprovalReply)>,
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
        matches!(self.editors.active_view(), Some("ai.chat" | "ai.codex"))
    }

    /// 편집기에서 다른 제공자의 채팅 탭이 앞으로 오면 패널 탭도 따라간다(그리기 직전에 부른다).
    pub(crate) fn ai_follow_view(&mut self) {
        let want = match self.editors.active_view() {
            Some("ai.codex") => Provider::Codex,
            Some("ai.chat") => Provider::Claude,
            _ => return,
        };
        if want != self.ai.tab {
            self.ai_switch_tab(want);
            self.ai_sync_panel();
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

    /// 채팅 탭이 보던 세션으로 되돌린다(있을 때 · 턴 중엔 안 바꿈) — 분할 창이 `cur`를 편집기 탭 세션으로 바꿔 둔 뒤.
    fn ai_chat_restore(&mut self) {
        if self.ai.host.as_ref().is_some_and(|h| h.busy) {
            return;
        }
        let p = self.ai.tab;
        let want = self.ai_chat_slot(p).clone();
        let found = want.and_then(|id| self.ai.sessions.iter().position(|s| s.id == id));
        if let Some(i) = found {
            if self.ai.cur != Some(i) {
                self.ai.cur = Some(i);
                self.ai.approval = None;
                self.ai.pending = None;
                self.ai_view.reset();
                self.ai_sync_panel();
            }
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

    /// 채팅 탭을 그리기 직전: 분할 창(또는 다른 제공자 탭)에서 넘어왔으면 이 제공자 채팅 탭이 보던 세션으로.
    pub(crate) fn ai_chat_prepare(&mut self) {
        self.ai_init();
        self.ai_view.set_side_mode(false);
        if self.ai.on_chat != Some(self.ai.tab) {
            self.ai_chat_restore();
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
                        self.ai.cur = Some(i);
                        self.ai.approval = None;
                        self.ai.pending = None;
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
            // 활동 막대에서 열 때는 채팅 탭이 보던 세션으로(분할 창 세션을 끌고 오지 않게).
            self.ai_chat_restore();
            self.ai_open_chat();
        } else if self.focus == Focus::Ai {
            self.set_focus(Focus::Editor);
        }
        self.layout();
        self.redraw();
    }

    /// 채팅 뷰 탭 열기/앞으로.
    pub(crate) fn ai_open_chat(&mut self) {
        self.ai_init();
        if self.ai.cur.is_none() {
            self.ai_new_session();
        }
        let tab = self.ai.tab;
        self.ai_chat_mark();
        self.editors.open_view_tab(tab.view_key(), tab.title());
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
    fn ai_cur_mut(&mut self) -> Option<&mut Session> {
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
        self.ai.sessions.insert(0, s);
        self.ai.cur = Some(0);
        self.ai.approval = None;
        self.ai.pending = None;
        self.ai.status.clear();
        self.ai_view.reset();
        self.ai_sync_panel();
    }

    /// 이전 세션 열기(이어하기: CLI = `--resume` · API = 대화 재구성).
    pub(crate) fn ai_open_session(&mut self, id: &str) {
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
        self.ai.cur = Some(i);
        self.ai.approval = None;
        self.ai.pending = None;
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
        if self.ai.side_rect.w > 0 && !self.ai_view_active() {
            // 분할 창에서 고른 세션 = 이 편집기 탭에 연결.
            if let (Some(tid), Some(s)) = (self.ai.side_tab, self.ai_cur()) {
                self.ai.tab_sessions.insert(tid, s.id.clone());
            }
            self.redraw();
        } else {
            self.ai_open_chat();
        }
    }

    pub(crate) fn ai_delete_session(&mut self, id: &str) {
        let Some(i) = self.ai.sessions.iter().position(|s| s.id == id) else {
            return;
        };
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
        self.ai_view
            .set_max_lines(self.settings.int("ai.composer_max_lines") as i32);
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
        ViewInfo {
            busy: self.ai.host.as_ref().is_some_and(|h| h.busy),
            approval: self
                .ai
                .approval
                .as_ref()
                .map(|(id, tool, sum, _)| (id.clone(), tool.clone(), sum.clone())),
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
                PanelAction::OpenSession(id) => self.ai_open_session(&id),
                PanelAction::RenameSession(id, title) => {
                    if let Some(s) = self.ai.sessions.iter_mut().find(|s| s.id == id) {
                        s.title = title;
                        ai::history_save(s);
                    }
                }
                PanelAction::DeleteSession(id) => self.ai_delete_session(&id),
                PanelAction::ShowChat => self.ai_open_chat(),
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
                ViewAction::OpenSession(id) => self.ai_open_session(&id),
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
        });
        self.ai.run_block = Some((msg, block));
        self.ai.capture = None;
        self.ai.status = t(Msg::AiRunning).into();
        self.run_text_whole(sql.to_string());
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
        if !self.sess.connected || !self.gate_open() || self.ai.pending.is_some() {
            self.ai.status = t(Msg::AiQueryBusy).into();
            self.redraw();
            return;
        }
        self.ai.capture = None;
        self.run_text_whole(sql.to_string());
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
        if !self.ai.host.as_ref().is_some_and(|h| h.busy) {
            self.ai.last_tick = None;
            return None;
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
        Some(self.ai.last_tick.unwrap_or(now) + std::time::Duration::from_secs(1))
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

    fn ai_insert_editor(&mut self, s: &str) {
        if self.ai_view_active() {
            // 채팅 탭 자신에게는 넣을 수 없다 — 새 탭.
            self.editors.new_tab(None);
        }
        let mut inv = Invalidations::default();
        self.ed_mut().paste(s, &mut inv);
        self.set_focus(Focus::Editor);
    }

    // ───────────────────────── 보내기 · 멈춤 ─────────────────────────

    pub(crate) fn ai_send(&mut self, text: &str) {
        self.ai_init();
        if self.ai.cur.is_none() {
            self.ai_new_session();
        }
        if self.ai.host.as_ref().is_some_and(|h| h.busy) {
            self.ai.status = t(Msg::AiBusyNote).into();
            return;
        }
        if ai::contains_secret(text) {
            self.ai_push(ChatMsg::new(Kind::Error, t(Msg::AiSecretBlocked)));
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
        let mut um = ChatMsg::new(Kind::User, text);
        um.attachments = attachments.iter().map(|a| a.label().to_string()).collect();
        self.ai_push(um);
        self.ai.turn_started = Some(Instant::now());
        self.ai.turn_wall = {
            let st = nsql_log::now_local().stamp();
            st.get(11..19).unwrap_or(&st).to_string()
        };
        self.ai.cli_turn_usage = None;
        let system =
            ai::tools::system_prompt(&format!("{:?}", self.sess.dialect), &self.sess.profile);
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
                if !self.ai_host().is_cli_alive() {
                    if self.ai.mcp.is_none() {
                        let port = self.settings.int("ai.mcp_port").clamp(0, 65535) as u16;
                        let tx = self.ai_host().tx.clone();
                        let wake = self.ai_wake();
                        match ai::mcp::start(tx, wake, port) {
                            Ok(s) => self.ai.mcp = Some(s),
                            Err(e) => {
                                self.ai_push(ChatMsg::new(Kind::Error, format!("mcp: {e}")));
                                return;
                            }
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
                        if let Some(k) = self.ai_key_for(&profile) {
                            env.push(("ANTHROPIC_API_KEY".into(), k));
                        }
                    }
                    // ★ `--bare`는 Claude Code의 자체 로그인(구독)도 건너뛴다 → 키를 넘길 때만(사용자 11-11 "터미널은 로그인돼 있는데").
                    let bare = self.settings.flag("ai.bare") && !env.is_empty();
                    let args = ai::claude_cli::args(
                        &model,
                        &effort,
                        self.settings.get("ai.permission_mode").unwrap_or("default"),
                        cfg.as_deref(),
                        cli_session.as_deref(),
                        bare,
                        &system,
                    );
                    let launch = CliLaunch {
                        program,
                        prefix,
                        args,
                        env,
                    };
                    if let Err(e) = self.ai_host().start_cli(launch) {
                        self.ai_push(ChatMsg::new(Kind::Error, tf(Msg::AiCliStartFailed, &[&e])));
                        return;
                    }
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
        if !self.ai_host().is_codex_alive() {
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
            if let Err(e) = self.ai_host().start_codex(launch) {
                self.ai_push(ChatMsg::new(Kind::Error, tf(Msg::AiCliStartFailed, &[&e])));
                return;
            }
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

    fn ai_after_send(&mut self) {
        self.ai_active_status();
        if let Some(s) = self.ai_cur() {
            ai::history_save(s);
        }
        self.ai_view.reset_scroll_to_bottom();
        self.ai_sync_panel();
        self.redraw();
    }

    pub(crate) fn ai_stop(&mut self) {
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
                ApprovalReply::Api { reply, .. } | ApprovalReply::Rows { reply, .. } => {
                    let _ = reply.send(ToolReply {
                        output: "cancelled".into(),
                        ok: false,
                    });
                }
            }
        }
        self.ai.pending = None;
        self.ai.status = t(Msg::AiStopped).into();
        self.ai_push(ChatMsg::new(Kind::Note, t(Msg::AiStopped)));
        self.ai.run = None;
        self.ai_sync_panel();
        self.redraw();
    }

    fn ai_push(&mut self, m: ChatMsg) {
        if let Some(s) = self.ai_run_mut() {
            s.messages.push(m);
        }
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
                let short = tool
                    .strip_prefix("mcp__nexa__")
                    .unwrap_or(&tool)
                    .to_string();
                let needs_user = short == "run_query" && !self.ai_sql_is_readonly(&summary);
                if auto && !needs_user {
                    let _ = reply.send(true);
                    return;
                }
                let mut m = ChatMsg::new(
                    Kind::Approval {
                        tool: short.clone(),
                    },
                    redact(&summary),
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
        let Some((_, _, _, r)) = self.ai.approval.take() else {
            return;
        };
        match r {
            ApprovalReply::Cli(tx) => {
                let _ = tx.send(ok);
            }
            ApprovalReply::Api { name, input, reply } => {
                if ok {
                    self.ai_exec_tool(&name, &input, reply);
                } else {
                    let _ = reply.send(ToolReply {
                        output: "denied by the user".into(),
                        ok: false,
                    });
                }
            }
            ApprovalReply::Rows { name, input, reply } => {
                if ok {
                    if let Some(s) = self.ai_run_mut() {
                        s.rows_allowed = true;
                    }
                    self.ai_exec_tool(&name, &input, reply);
                } else {
                    let _ = reply.send(ToolReply {
                        output: "the user declined to share row data".into(),
                        ok: false,
                    });
                }
            }
        }
        self.ai_push(ChatMsg::new(
            Kind::Note,
            t(if ok { Msg::AiAllowed } else { Msg::AiDenied }),
        ));
        self.redraw();
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
        let is_rows = matches!(tool.name, "run_query" | "get_result" | "explain");
        let readonly_q = tool.name != "run_query" || {
            let sql = json_parse(&input)
                .ok()
                .map(|v| ai::jstr(&v, "sql"))
                .unwrap_or_default();
            sql_is_readonly(&sql)
        };
        let short = tool.name.to_string();
        if is_rows && !rows_allowed {
            let mut m = ChatMsg::new(
                Kind::Approval {
                    tool: short.clone(),
                },
                t(Msg::AiRowsConsent),
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
        if tool.write && (!auto || !readonly_q) {
            let mut m = ChatMsg::new(
                Kind::Approval {
                    tool: short.clone(),
                },
                redact(&input),
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
                },
            ));
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
                    self.menu_action(&id);
                    ok(format!("ran {id}"))
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
                    let sel = self.editors.cur().copy_selection().unwrap_or_default();
                    let byte_pos = full
                        .char_indices()
                        .nth(self.editors.cur().caret())
                        .map_or(full.len(), |(b, _)| b);
                    let stmt =
                        nsql_script::statement_at_in(&full, byte_pos, Some(self.sess.dialect))
                            .map(|it| full[it.span].to_string())
                            .unwrap_or_default();
                    ok(format!(
                        "<text>\n{full}\n</text>\n<selection>\n{sel}\n</selection>\n<statement_at_caret>\n{stmt}\n</statement_at_caret>"
                    ))
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
                let rows = self
                    .explorer
                    .ai_search_objects(&g("query"), &g("kind"), limit);
                if rows.is_empty() {
                    ok("no objects (is the explorer connected and loaded?)".into())
                } else {
                    ok(rows.join("\n"))
                }
            }
            "describe_object" => {
                let schema = g("schema");
                let s = (!schema.is_empty()).then_some(schema.as_str());
                match self.explorer.ai_describe(s, &g("name")) {
                    Some(d) => ok(d),
                    None => err("object not found in explorer; try search_objects".into()),
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
                            "profile '{p}' has no stored password: the connection window is open with it preselected. Tell the user they must type the password and confirm the login in that window themselves (you cannot do it)."
                        ))
                    }
                    Ok(None) => err(format!("unknown profile '{p}' (use list_profiles)")),
                    Err(e) => err(e.to_string()),
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
                });
                self.run_text_whole(src);
                return;
            }
            "object_source" => {
                let schema = g("schema");
                let s = (!schema.is_empty()).then_some(schema.as_str());
                match self.explorer.ai_object_info(s, &g("name")) {
                    Some(o) => {
                        let before = self.editors.active_id();
                        self.explorer.open_source_of(&o);
                        self.ai.pending = Some(PendingTool {
                            reply: Some(reply),
                            attach_label: None,
                            since: Instant::now(),
                            max_rows,
                            wait_editor: Some(before),
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
        let done_text: Option<(String, bool)> = if let Some(before) = p.wait_editor {
            let cur = self.editors.active_id();
            let text = self.editors.cur().text();
            if cur != before && !text.trim().is_empty() {
                Some((text, true))
            } else if timeout {
                Some(("timed out opening the source".into(), false))
            } else {
                None
            }
        } else if !self.sess.busy && p.since.elapsed() > std::time::Duration::from_millis(250) {
            if self.sess.run_had_err {
                Some((redact(&self.sess.status), false))
            } else {
                Some((
                    self.ai
                        .capture
                        .take()
                        .unwrap_or_else(|| redact(&self.sess.status)),
                    true,
                ))
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
            self.ai_send(text);
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
        if pointer && (in_view || popup) {
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
            return true;
        }
        false
    }
}

fn t_(m: Msg) -> String {
    t(m).to_string()
}

/// 읽기 전용 문장인가(SELECT/WITH/SHOW/EXPLAIN/DESC/VALUES · 한 문장).
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

#[cfg(test)]
mod tests {
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
