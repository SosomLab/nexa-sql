//! App — 세션 컨텍스트 · 접속/해제 · 실행 통제 문지기(docs/52·54 · DR-34).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    pub(crate) fn session_mode(&self) -> SessionMode {
        SessionMode::parse(self.settings.get("session.mode"))
    }

    /// 접속 창(로그인)이 붙이는 세션 — 개별 모드 = 활성 탭의 세션 · 공유 모드 = `login_plan`
    /// (같은 서버 = 중복 없이 그 세션 · 놀고 있는 세션 재활용 · 아니면 **추가** · 상한). None = 지금은 못 한다(안내는 여기서).
    pub(crate) fn login_place(&mut self, spec: &ConnectSpec) -> Option<u64> {
        if self.session_mode() == SessionMode::PerEditor {
            return Some(self.sess_id_for_tab(self.editors.active_id()));
        }
        let views: Vec<sessions::SharedView> = self
            .all_sess()
            .filter(|s| !s.is_private() && !s.closing)
            .map(|s| sessions::SharedView {
                id: s.id,
                same_server: s
                    .spec
                    .as_ref()
                    .or(s.last_spec.as_ref())
                    .is_some_and(|have| worker::same_server(have, spec)),
                connected: s.connected,
                blocked: s.blocked(),
                idle_closed: s.idle_closed,
                bound_tabs: self.bound_tabs(s.id),
            })
            .collect();
        let max = self.cap(
            nsql_license::Feature::MultiConnection,
            self.settings.int("session.max_shared").max(1) as usize,
            2,
        );
        match sessions::login_plan(&views, max) {
            sessions::LoginPlan::Use(id) | sessions::LoginPlan::Recycle(id) => Some(id),
            sessions::LoginPlan::New => Some(self.new_shared()),
            sessions::LoginPlan::Busy(_) => {
                self.sess.status = t(Msg::StRunning).into();
                self.fail_login_attempt(t(Msg::StRunning));
                None
            }
            sessions::LoginPlan::Limit => {
                let m = tf(Msg::StSessSharedLimit, &[&max.to_string()]);
                self.log_win.push(LogEntry::new(LogKind::Error, m.clone()));
                self.fail_login_attempt(&m);
                self.sess.status = m;
                None
            }
        }
    }

    /// 접속 시도를 워커에 보내지 못했다 — 접속 창의 "접속 중" 표시를 실패로 되돌린다.
    fn fail_login_attempt(&mut self, why: &str) {
        let name = self.panel_op_name();
        self.conn_win.set_connect_mark(&name, None);
        self.set_panel_result(&name, ConnState::Failed(why.to_string()));
        self.conn_win.redraw();
    }

    /// 모든 세션(지금 것 + 잠든 것).
    pub(crate) fn all_sess(&self) -> impl Iterator<Item = &Sess> {
        std::iter::once(&self.sess).chain(self.parked.iter())
    }

    pub(crate) fn sess_by_id(&self, id: u64) -> Option<&Sess> {
        self.all_sess().find(|s| s.id == id)
    }

    /// 탭이 쓰는 세션 id — ① 그 탭의 전용 세션(닫는 중이 아닌) ② 탭이 고른 공유 세션 ③ 기본 공유 세션.
    pub(crate) fn sess_id_for_tab(&self, tab: u64) -> u64 {
        let private = self
            .all_sess()
            .find(|s| s.owner == Some(tab) && !s.closing)
            .map(|s| s.id);
        let bound = self.tab_bind.get(&tab).copied();
        let alive = bound.is_some_and(|id| {
            self.all_sess()
                .any(|s| s.id == id && !s.is_private() && !s.closing)
        });
        sessions::route_tab(private, bound, alive, self.default_shared)
    }

    /// ★ 지금 활성 탭을 공유 세션 `id`에 묶는다(전용 세션이 있는 탭은 그대로) — 접속 창으로 새 연결을 붙였을 때·"이 연결 사용"
    ///   (사용자 09-28 "새 서버에 연결하면 현재 탭에 자동 적용" · 다른 탭은 건드리지 않는다). 바뀌면 그 탭의 결과 탭을 얼린다.
    pub(crate) fn bind_active_tab_to(&mut self, id: u64) {
        let tab = self.editors.active_id();
        if self.all_sess().any(|s| s.owner == Some(tab) && !s.closing) {
            return;
        }
        if self.tab_bind.get(&tab) != Some(&id) {
            self.tab_bind.insert(tab, id);
            self.freeze_results_of(tab);
            self.sess_ui_dirty = true;
        }
    }

    /// 공유 세션을 **실제로 쓰는** 탭 수(해제 버튼 배지·툴팁 · 해제 물음 · 세션 창).
    /// 🔧 09-28(사용자 "편집기 탭은 3개보다 많은데 배지가 3"): 묶임 표(`tab_bind`)는 탭이 **활성화될 때** 늦게 채워져 프로젝트
    ///   복원 직후 아직 열어 보지 않은 탭을 빼먹었다 → 탭마다 라우팅(`sess_id_for_tab` = 전용 → 묶임 → 기본 공유)으로 센다.
    pub(crate) fn bound_tabs(&self, id: u64) -> usize {
        self.editors
            .tab_ids()
            .into_iter()
            .filter(|&t| self.sess_id_for_tab(t) == id)
            .count()
    }

    /// 공유 세션 하나 추가(워커 하나) — 기존 연결은 그대로 둔다(docs/52 §2-1).
    fn new_shared(&mut self) -> u64 {
        let (w, ev) = self.spawn_worker();
        let id = self.next_sess_id;
        self.next_sess_id += 1;
        let s = Sess::new(id, None, w, ev, DEFAULT_DIALECT);
        s.control(worker::Cmd::GlobalVars(self.global_vars.clone()));
        self.parked.push(s);
        id
    }

    /// 공유 연결 활성화 — **지금 탭**과 새 탭·탐색기·접속 창 표시가 이 연결을 따른다(다른 탭은 묶인 대로 · 09-28).
    pub(crate) fn activate_shared(&mut self, id: u64) {
        self.bind_active_tab_to(id);
        let Some((spec, profile, desc, connected)) =
            self.sess_by_id(id).filter(|s| !s.is_private()).map(|s| {
                (
                    s.spec.clone(),
                    s.profile.clone(),
                    s.desc.clone(),
                    s.connected,
                )
            })
        else {
            return;
        };
        self.default_shared = id;
        self.primary_sess = id;
        self.default_spec = spec.clone();
        self.editors
            .set_conn_desc(if connected { desc } else { String::new() });
        if connected {
            if let Some(spec) = spec {
                self.conn_win.mark_connected(&profile);
                self.conn_win.clear_connect_marks();
                self.conn_win
                    .set_connect_mark(&profile, Some(ConnectMark::Connected));
                self.explorer.connect(&spec, &profile, true, false);
            }
        }
        self.sync_sess();
        self.sync_sess_ui();
        self.redraw();
    }

    /// 공유 세션 해제(툴바 드롭다운 · 메뉴) — 탐색기가 이 연결을 따르고 있었으면 함께 닫고, 묶인 탭은 끊김 표식으로 남는다.
    fn disconnect_shared(&mut self, id: u64) {
        if self.sess.id == id {
            // 세션 창·탐색기에서 고른 해제 = 이미 연결 단위로 고른 것이라 "다른 탭도 씀" 확인을 다시 묻지 않는다.
            self.disconnect_current();
            return;
        }
        self.with_sess(id, |a| {
            if !a.sess.tx_pending.is_empty() {
                // 다른 탭의 세션 — 확인 팝업은 그 탭을 앞에 두고 답해야 한다.
                a.sess.status = t(Msg::StSessTxPending).into();
                a.log_win
                    .push(LogEntry::new(LogKind::Error, a.sess.status.clone()));
                return;
            }
            a.disconnect_force();
        });
        self.sess.status = t(Msg::StDisconnected).into();
        self.reap_sessions();
        self.sync_sess_ui();
        self.redraw();
    }

    /// 모든 연결 해제(툴바 Disconnect 본체): 지금 세션에 미커밋이 있으면 묻고(DR-30) · 다른 세션은 미커밋이 있으면 남긴다(로그) ·
    /// 나머지는 전부 끊는다 → 전 탭이 미연결(공유 탭은 끊긴 공유 연결에 묶인 채 · 전용 탭은 세션 규칙대로).
    pub(crate) fn disconnect_all(&mut self) {
        if !self.sess.tx_pending.is_empty() {
            self.tx_after = Some(TxAfter::Disconnect);
            self.open_tx_guard(Msg::MnTxCommitDisconnect, Msg::MnTxRollbackDisconnect);
            self.redraw();
            return;
        }
        let ids: Vec<u64> = self
            .all_sess()
            .filter(|s| !s.closing && (s.connected || s.busy || s.idle_closed))
            .map(|s| s.id)
            .collect();
        let mut skipped = 0usize;
        for sid in ids {
            if self
                .sess_by_id(sid)
                .is_some_and(|s| !s.tx_pending.is_empty())
            {
                skipped += 1;
                continue;
            }
            self.disconnect_session(sid);
        }
        let note = if skipped > 0 {
            tf(Msg::StSessSkippedPending, &[&skipped.to_string()])
        } else {
            String::new()
        };
        self.sess.status = tf(Msg::StSessAllDropped, &[&note]);
        self.log_win
            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
        self.sync_sess_ui();
        self.redraw();
    }

    /// 세션 하나 해제 — 전용/개별 탭 세션이면 그 탭의 규칙(공유 복귀 · 개별 모드 = 끊김)으로, 공유 연결이면 공유 해제로.
    pub(crate) fn disconnect_session(&mut self, id: u64) {
        match self.sess_by_id(id).map(|s| s.owner) {
            Some(Some(tab)) => self.disconnect_private(tab),
            Some(None) => self.disconnect_shared(id),
            None => {}
        }
    }

    pub(crate) fn disconnect_pick(&mut self, id: &str) {
        if id == "conn.drop_all" {
            let ids: Vec<u64> = self.all_sess().map(|s| s.id).collect();
            for sid in ids {
                self.mark_disc(sid, sessions::DiscPath::SessionsWin);
            }
            self.disconnect_all();
        } else if let Some(sid) = id.strip_prefix("conn.use:").and_then(|v| v.parse().ok()) {
            self.activate_shared(sid);
        } else if let Some(sid) = id.strip_prefix("conn.again:").and_then(|v| v.parse().ok()) {
            // 세션 관리자의 "다시 접속" = 명시적 요청(설정 `connect.reconnect_same`).
            let again = self.settings.flag("connect.reconnect_same");
            let fallback = self.default_spec.clone();
            self.with_sess(sid, |a| {
                if let Some(spec) = a.sess.spec.clone().or(fallback) {
                    a.connect_quietly(spec, again);
                }
            });
            self.sync_sess_ui();
        } else if let Some(sid) = id.strip_prefix("conn.drop:").and_then(|v| v.parse().ok()) {
            self.mark_disc(sid, sessions::DiscPath::SessionsWin);
            self.disconnect_session(sid);
        }
    }

    /// `id` 세션을 잠시 `self.sess` 자리에 놓고 `f`를 돈다(끝나면 되돌린다). 기존 코드는 늘 `self.sess`만 보므로
    /// 잠든 세션의 이벤트 처리·접속 창의 공유 세션 조작을 같은 코드로 한다.
    pub(crate) fn with_sess<R>(&mut self, id: u64, f: impl FnOnce(&mut Self) -> R) -> Option<R> {
        if self.sess.id == id {
            return Some(f(self));
        }
        let i = self.parked.iter().position(|s| s.id == id)?;
        let home = self.sess.id;
        std::mem::swap(&mut self.sess, &mut self.parked[i]);
        let r = f(self);
        if let Some(j) = self.parked.iter().position(|s| s.id == home) {
            std::mem::swap(&mut self.sess, &mut self.parked[j]);
        }
        Some(r)
    }

    /// 활성 편집기 탭의 세션을 `self.sess`로 — 탭 전환·세션 생성/해제 뒤에 부른다(이벤트 뒤 · 페인트 전 · 실행 직전).
    /// 개별 모드면 처음 활성화된 탭에 전용 세션을 만들고 기본 접속 정보로 붙인다(열기만 하고 안 본 탭은 접속하지 않는다 = 부하 0).
    pub(crate) fn sync_sess(&mut self) {
        let tab = self.editors.active_id();
        if tab == 0 {
            return;
        }
        // 상한에 닿았으면 조용히 공유 세션을 쓴다(표식 없음 = 공유) — 페인트마다 불리므로 여기서 경고를 내지 않는다.
        let room = self
            .all_sess()
            .filter(|s| s.is_private() && !s.closing)
            .count()
            < self.cap(
                nsql_license::Feature::MultiConnection,
                self.settings.int("session.max_private").max(0) as usize,
                2,
            );
        if room
            && self.session_mode() == SessionMode::PerEditor
            && !self.all_sess().any(|s| s.owner == Some(tab) && !s.closing)
        {
            let spec = self.default_spec.clone();
            if let Some(id) = self.new_private(tab) {
                if let Some(spec) = spec {
                    self.with_sess(id, |a| a.connect_quietly(spec, false));
                }
            }
        }
        // ★ 공유 모드의 탭은 **만들어질 때(복원 포함) 그때의 활성 공유 연결**에 바로 묶인다(사용자 09-18) — 뒤에 활성 연결을 바꿔도
        //   이 탭은 그대로. 🔧 09-28(사용자 실기 "탭 하나의 연결만 바꿨는데 모든 탭이 바뀜"): 종전엔 **활성 탭만** 묶어, 아직 열어
        //   보지 않은 탭들이 떠다니다가 새 접속(활성 공유 연결 교체)에 한꺼번에 옮겨 갔다 → 전용 세션이 없는 탭 전부를 지금 묶는다.
        let default = self.default_shared;
        let mut bound_any = false;
        for t in self.editors.tab_ids() {
            if !self.tab_bind.contains_key(&t)
                && !self.all_sess().any(|s| s.owner == Some(t) && !s.closing)
            {
                self.tab_bind.insert(t, default);
                bound_any = true;
            }
        }
        if bound_any {
            // 함께 쓰는 탭 수가 바뀌었다 → 해제 버튼 배지·툴팁을 바로(사용자 09-19 "탭이 추가돼도 숫자가 안 는다").
            self.sess_ui_dirty = true;
        }
        let want = self.sess_id_for_tab(tab);
        if self.sess.id != want {
            if let Some(i) = self.parked.iter().position(|s| s.id == want) {
                std::mem::swap(&mut self.sess, &mut self.parked[i]);
                self.sync_sess_ui();
                self.redraw();
            }
        }
    }

    /// 탭을 **미연결**로(사용자 09-18 · 표식 메뉴 "미연결"): 전용 세션이 있으면 그 세션을 끊어 미연결로 남기고 ·
    /// 공유 탭이면 접속 없는 전용 자리(세션 객체만)를 만들어 어떤 연결에도 묶이지 않게 한다.
    pub(crate) fn make_unconnected(&mut self, tab: u64) {
        let private = self
            .all_sess()
            .find(|s| s.owner == Some(tab) && !s.closing)
            .map(|s| s.id);
        if let Some(id) = private {
            if self.sess_by_id(id).is_some_and(|s| s.disc_path.is_none()) {
                self.mark_disc(id, sessions::DiscPath::Badge);
            }
            self.disconnect_private_as(tab, true);
            return;
        }
        if let Some(id) = self.new_private(tab) {
            self.with_sess(id, |a| {
                a.sess.user_disconnected = true;
                a.sess.status = t(Msg::StSessUnconnected).into();
            });
            self.freeze_results_of(tab);
            self.sync_sess();
            self.sync_sess_ui();
            self.redraw();
        }
    }

    /// 새 탭 규칙(사용자 09-18): 공유 모드 = 활성 공유 연결(`sync_sess`가 묶는다) · 개별 모드 = **미연결 자리 + 접속 창을 바로**
    /// (접속하지 않고 닫으면 미연결 그대로).
    pub(crate) fn on_new_tab(&mut self) {
        if self.session_mode() != SessionMode::PerEditor {
            return;
        }
        let tab = self.editors.active_id();
        if let Some(id) = self.new_private(tab) {
            self.with_sess(id, |a| a.sess.user_disconnected = true);
            self.sync_sess();
            self.sync_sess_ui();
            self.open_conn = true;
        }
    }

    /// 탭 전용 세션 하나(워커 스레드 하나) — 상한 `session.max_private`(기본 사상: 1 인스턴스 · 1 서버 · 1 계정이라 예외는 아껴 쓴다).
    pub(crate) fn new_private(&mut self, tab: u64) -> Option<u64> {
        let max = self.cap(
            nsql_license::Feature::MultiConnection,
            self.settings.int("session.max_private").max(0) as usize,
            2,
        );
        let n = self
            .all_sess()
            .filter(|s| s.is_private() && !s.closing)
            .count();
        if n >= max {
            self.sess.status = tf(Msg::StSessLimit, &[&max.to_string()]);
            self.log_win
                .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
            return None;
        }
        let (w, ev) = self.spawn_worker();
        let id = self.next_sess_id;
        self.next_sess_id += 1;
        let s = Sess::new(id, Some(tab), w, ev, DEFAULT_DIALECT);
        s.control(worker::Cmd::GlobalVars(self.global_vars.clone()));
        self.parked.push(s);
        Some(id)
    }

    /// 접속 창을 거치지 않는 접속(개별 모드의 탭 자동 접속 · 유휴 해제 뒤 재접속) — 시도 큐·접속 창 표시는 건드리지 않는다.
    /// `reconnect_same` = 같은 서버에 이미 붙어 있어도 끊고 다시(사용자의 **명시적** 재접속 = 설정 `connect.reconnect_same`).
    pub(crate) fn connect_quietly(&mut self, spec: ConnectSpec, reconnect_same: bool) {
        self.sess.busy = true;
        self.sess.user_disconnected = false;
        self.sess.idle_closed = false;
        self.sess.status = tf(Msg::StConnecting, &[&spec.redacted()]);
        self.sess.spec = Some(spec.clone());
        self.sess.touch();
        // 접속은 줄 세우기가 뜻이다 — 실행 경로의 조용한 재접속(`wake_if_idle`)은 뒤따르는 실행보다 먼저 가야 한다.
        let pass = self.sess.pass_queued("connect_quietly");
        self.sess.submit(
            pass,
            worker::Cmd::ConnectSpec {
                spec,
                reconnect_same,
            },
        );
    }

    /// 모든 세션의 워커 응답을 처리한다 — 잠든 세션은 잠시 앞으로 꺼내 같은 코드로.
    pub(crate) fn drain_all(&mut self) {
        self.drain_events();
        let ids: Vec<u64> = self.parked.iter().map(|s| s.id).collect();
        for id in ids {
            self.with_sess(id, |a| a.drain_events());
        }
        self.reap_sessions();
        self.sync_sess();
        self.sync_sess_ui();
    }

    /// 닫는 중인 세션 · 주인 탭이 닫힌 전용 세션을 거둔다(워커에 Quit = 커밋 없이 닫지 않는다: 미커밋은 닫기 전에 이미 물었다).
    pub(crate) fn reap_sessions(&mut self) {
        let alive = self.editors.tab_ids();
        for s in &mut self.parked {
            if s.owner.is_some_and(|t| !alive.contains(&t)) {
                s.closing = true;
            }
        }
        // 지금 세션이 닫는 중이면 먼저 자리를 비킨다(공유 세션으로).
        if self.sess.closing || self.sess.owner.is_some_and(|t| !alive.contains(&t)) {
            self.sess.closing = true;
            let to = self.default_shared;
            if let Some(i) = self.parked.iter().position(|s| s.id == to) {
                std::mem::swap(&mut self.sess, &mut self.parked[i]);
            }
        }
        let before = self.parked.len();
        // 거두는 세션의 해제 로그(탭 닫기 · 다른 연결로 전환 · 스크립트 DISCONNECT 뒤 거둠) — 아직 접속돼 있던 것만.
        let closing: Vec<u64> = self
            .parked
            .iter()
            .filter(|s| s.closing && s.id != SHARED && (s.connected || s.busy))
            .map(|s| s.id)
            .collect();
        for id in closing {
            let fallback =
                if alive.contains(&self.sess_by_id(id).and_then(|s| s.owner).unwrap_or(0)) {
                    sessions::DiscPath::Switch
                } else {
                    sessions::DiscPath::TabClose
                };
            self.with_sess(id, |a| a.log_disconnect(fallback));
        }
        self.parked.retain(|s| {
            if s.closing && s.id != SHARED {
                // 실행 중이면 먼저 취소를 보낸다(닫힌 탭의 질의가 서버에 남아 돌지 않게).
                if s.blocked() {
                    let _ = s.worker.cancel_run();
                }
                s.control(worker::Cmd::Disconnect);
                s.control(worker::Cmd::Quit);
                false
            } else {
                true
            }
        });
        self.tab_bind.retain(|t, _| alive.contains(t));
        // 끊긴 채 아무도 안 쓰는 공유 세션 객체(추가 접속 실패 · 해제 뒤)는 거둔다 — 활성 연결·묶인 탭이 있는 것은 남긴다.
        let default = self.default_shared;
        let binds: Vec<u64> = self.tab_bind.values().copied().collect();
        self.parked.retain(|s| {
            let n = binds.iter().filter(|b| **b == s.id).count();
            if !s.is_private()
                && sessions::reap_shared(
                    s.connected,
                    s.blocked(),
                    s.idle_closed,
                    s.id == default,
                    n,
                )
            {
                s.control(worker::Cmd::Quit);
                false
            } else {
                true
            }
        });
        if self.parked.len() != before {
            self.sync_sess_ui();
        }
    }

    /// 지금 세션이 붙은 서버의 탐색기를 확보한다 — 스펙이 프로필 이름뿐이면 저장소에서 완성해 둔다(메타 세션도 같은 자격으로
    /// 붙는다 · 같은 서버 판정·재접속에도 같은 스펙을 쓴다). 인라인 스펙은 이미 자격을 갖고 있다(비밀번호 필수 · 09-19).
    pub(crate) fn explorer_attach(&mut self) {
        let Some(spec) = self.sess.spec.clone() else {
            return;
        };
        let (full, name) = match sessions::bare_profile_name(&spec) {
            Some(n) => match Vault::open_default().and_then(|v| v.resolve(n)) {
                Ok(Some(f)) => (f, n.to_string()),
                _ => return,
            },
            None => (spec, self.sess.profile.clone()),
        };
        self.sess.spec = Some(full.clone());
        self.sess.env_temp = false;
        let show = self.sess_id_for_tab(self.editors.active_id()) == self.sess.id;
        // ★ 일회성 비밀번호(입력 창으로 받은 것): 세션 스펙에는 **넣지 않는다**. 탐색기 메타 세션이 같은 자격으로 붙도록 이 호출에만
        //   빌려주고 바로 지운다(메타 스레드도 접속 뒤 지운다 · 유휴 회수 없음).
        let once = match self.pw_once.take() {
            Some((sid, secret, _)) if sid == self.sess.id && full.password.is_none() => {
                Some(secret)
            }
            other => {
                self.pw_once = other;
                None
            }
        };
        match once {
            Some(secret) => {
                let mut lend = full.clone();
                lend.password = Some(secret.expose().to_string());
                drop(secret);
                self.explorer.connect(&lend, &name, show, true);
                nsql_core::secret::wipe_opt(&mut lend.password);
            }
            None => self.explorer.connect(&full, &name, show, false),
        }
    }

    /// 세션 상태가 바뀌었을 때 화면의 세 층을 한 번에 맞춘다: 탭 표식·설명 · 통제(툴바) · 트랜잭션 · 해제 버튼.
    pub(crate) fn sync_sess_ui(&mut self) {
        // 설정 창이 열려 있으면 활성 탭 덧말·미리보기도 따라온다(탭 전환 · 사용자 09-29).
        self.prefs_format_preview_refresh();
        // ★ 활성 탭의 연결이 바뀌었다(접속 추가·표식 메뉴·탭 전환) → 탐색기를 그 서버 칸의 현재 스키마로 한 번(10-01 ㉗-j · 세션마다 한 번).
        if self.sess.connected {
            let key = (self.editors.active_id(), self.sess.id);
            let spec = self.sess.spec.clone();
            // ★ 그 서버 칸이 이미 보이면 옮기지 않는다(사용자 10-08: 탐색기에서 소스를 연 새 탭에도 ㉗-j가 돌아 선택이 현재 스키마 행으로
            //   튀었다) — 다른 서버 칸일 때만 그 칸 + 현재 스키마로.
            if self.explorer_focus_key != Some(key)
                && !self.explorer.is_shown(spec.as_ref())
                && self.explorer.focus_server(spec.as_ref())
            {
                self.explorer_focus_key = Some(key);
            }
        }
        let mut info: HashMap<u64, (nexa_ctl::TabBadge, String)> = HashMap::new();
        // 공유 연결이 둘 이상이면 공유 탭에도 표식(어느 서버인지 · 표식 메뉴로 고른다).
        let multi = self
            .all_sess()
            .filter(|s| !s.is_private() && !s.closing)
            .count()
            > 1;
        for tab in self.editors.tab_ids() {
            let Some(s) = self.sess_by_id(self.sess_id_for_tab(tab)) else {
                continue;
            };
            let badge = match sessions::badge_kind(s.is_private(), multi, s.connected && !s.broken)
            {
                sessions::BadgeKind::Private => nexa_ctl::TabBadge::Link,
                sessions::BadgeKind::Shared => nexa_ctl::TabBadge::Shared,
                sessions::BadgeKind::Off => nexa_ctl::TabBadge::LinkOff,
            };
            info.insert(tab, (badge, s.desc.clone()));
        }
        self.editors.set_sess_info(info);
        // 탐색기 참조 수 = 지금 붙어 있는 세션들의 서버(0이 된 서버는 메타 접속만 닫고 트리는 남긴다) · 활성 탭의 서버를 앞으로.
        let live: Vec<ConnectSpec> = self
            .all_sess()
            .filter(|s| (s.connected || s.idle_closed) && !s.closing)
            .filter_map(|s| s.spec.clone())
            .collect();
        self.explorer.sync_refs(&live);
        let cur = self.sess.spec.clone();
        self.explorer.show_for(cur.as_ref());
        self.sync_gate();
        self.sync_tx_ui();
        self.sessions_win.redraw();
        // Disconnect = **지금 탭의 연결**이 있을 때만(종전 규칙).
        let cur = self.sess.connected || self.sess.busy;
        self.sync_disconnect_btn(cur);
        self.sync_tab_conn_item(cur);
    }

    /// ★ 툴바 "탭 연결정보"(사용자 09-28): 지금 탭의 서버 = 프로필 이름 > 계정@호스트 · 미연결 = "연결 없음" · 색 = 연결됨 초록 · 끊김 빨강.
    fn sync_tab_conn_item(&mut self, connected: bool) {
        let mut inv = Invalidations::default();
        let (label, tip, tone) = if connected {
            (
                sessions::tab_conn_label(
                    &self.sess.profile,
                    self.sess.spec.as_ref(),
                    &self.sess.desc,
                ),
                self.sess.desc.clone(),
                if self.sess.broken {
                    nexa_ctl::ToolTone::Danger
                } else {
                    nexa_ctl::ToolTone::Ok
                },
            )
        } else {
            (
                t(Msg::MnSessNoConnection).to_string(),
                t(Msg::TipTabConn).to_string(),
                nexa_ctl::ToolTone::Default,
            )
        };
        self.tool_dock.set_item_label("sess.tab", &label, &mut inv);
        self.tool_dock.set_item_tone("sess.tab", tone, &mut inv);
        self.tool_dock.set_item_tip("sess.tab", &tip);
        // ★ 작업 단위(10-01 ⑫): 값만 · 바꿀 수 있는 방언(SQL Server · MySQL)에서만 활성.
        let (db_label, editable) = if connected {
            let spec = self.sess.spec.clone();
            sessions::db_unit(
                Some(self.sess.dialect),
                spec.as_ref(),
                self.sess_unit_value(spec.as_ref()).as_deref(),
                self.explorer.current_schema(spec.as_ref()).as_deref(),
            )
        } else {
            ("-".to_string(), false)
        };
        self.tool_dock
            .set_item_label("sess.db", &db_label, &mut inv);
        self.tool_dock
            .set_item_enabled("sess.db", connected && editable, &mut inv);
        self.tool_dock.set_item_tip("sess.db", t(Msg::TipTabDb));
        if !inv.is_empty() {
            // 글 폭이 바뀌면 그룹 배치도 다시(다음 그리기에서 실측).
            self.tool_layout_dirty = true;
        }
    }

    /// 이 세션이 전환한 작업 단위(세션 사실 · ⑳): SQL Server/MySQL = 현재 DB(없으면 탐색기 서버 정보) · Oracle/PG = 현재 스키마(`cur_schema`).
    fn sess_unit_value(&self, spec: Option<&ConnectSpec>) -> Option<String> {
        match self.sess.dialect {
            nsql_core::Dialect::Mssql | nsql_core::Dialect::Mysql => self
                .sess
                .current_db
                .clone()
                .or_else(|| self.explorer.current_db(spec)),
            _ => self.sess.cur_schema.clone(),
        }
    }

    /// ★ 탭 전환 때 세션을 **그 탭의 작업 단위**로 조용히 맞춘다(10-01 ⑯ · 사용자 "새 탭 = 연결 기본값 · 직접 바꾼 값은 탭이 기억"):
    /// 탭 값 → 없으면 연결 기본값 → 지금 값과 다르면 `Cmd::SetUnit`(Output 없음 · 성공 = `DbChanged`). 바꿀 수 없는 방언·미연결·바쁨 = 건너뜀.
    pub(crate) fn apply_tab_unit(&mut self) {
        if !self.sess.connected || self.sess.busy || self.sess.blocked() {
            return;
        }
        let d = self.sess.dialect;
        let spec = self.sess.spec.clone();
        let (_, editable) = sessions::db_unit(Some(d), spec.as_ref(), None, None);
        if !editable {
            return;
        }
        let have = match d {
            nsql_core::Dialect::Mssql | nsql_core::Dialect::Mysql => self
                .sess
                .current_db
                .clone()
                .or_else(|| self.explorer.current_db(spec.as_ref())),
            _ => self.sess.cur_schema.clone(),
        };
        let tab = self.editors.active_id();
        let Some(want) = sessions::unit_to_apply(
            self.tab_unit.get(&(tab, self.sess.id)).map(String::as_str),
            self.sess.default_unit.as_deref(),
            have.as_deref(),
        ) else {
            return;
        };
        let Some(pass) = self.sess.pass() else {
            return;
        };
        self.sess.submit(pass, worker::Cmd::SetUnit(want));
    }

    /// ★ 툴바 "작업 단위" 클릭(10-01 ⑫): SQL Server = 데이터베이스 목록 · MySQL = 스키마(DB) 목록 · 현재 ✓ · 고르면 `USE`(편집기 명령과 같은 길).
    pub(crate) fn open_tab_db_menu(&mut self) {
        if !self.sess.connected {
            return;
        }
        let spec = self.sess.spec.clone();
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let list = match self.sess.dialect {
            nsql_core::Dialect::Mssql => self.explorer.databases(spec.as_ref()),
            // Oracle(⑮) = 스키마 목록(`ALTER SESSION SET CURRENT_SCHEMA`) · MySQL = DB(= 스키마) 목록.
            nsql_core::Dialect::Mysql | nsql_core::Dialect::Oracle => {
                self.explorer.schemas(spec.as_ref())
            }
            _ => return,
        };
        let (cur, _) = sessions::db_unit(
            Some(self.sess.dialect),
            spec.as_ref(),
            self.sess_unit_value(spec.as_ref()).as_deref(),
            self.explorer.current_schema(spec.as_ref()).as_deref(),
        );
        let items: Vec<CtxItem> = if list.is_empty() {
            // 목록이 아직 없다 = 채움을 청하고 안내(⑰ · 다음 클릭에 보인다).
            self.explorer.request_objects(spec.as_ref(), "");
            vec![CtxItem::maybe("sess.db:", t(Msg::MnSessUseNoList), false)]
        } else {
            list.iter()
                .map(|d| {
                    CtxItem::item(format!("sess.db:{d}"), d.clone())
                        .with_mark(d.eq_ignore_ascii_case(&cur))
                })
                .collect()
        };
        let i = self.editors.active();
        let anchor = self.tool_dock.item_rect("sess.db");
        self.badge_menu_tab = Some(self.editors.tab_id(i));
        match anchor {
            Some(r) => self.editors.open_badge_menu_at(i, items, r),
            None => self.editors.open_badge_menu(i, items),
        }
        self.redraw();
    }

    /// 툴바 "탭 연결정보" 클릭 = 탭 표식 메뉴와 같은 목록을 그 항목 아래에(사용자 09-28).
    pub(crate) fn open_tab_conn_menu(&mut self) {
        let i = self.editors.active();
        let Some(items) = self.badge_menu_items(i) else {
            return;
        };
        let anchor = self.tool_dock.item_rect("sess.tab");
        self.badge_menu_tab = Some(self.editors.tab_id(i));
        match anchor {
            Some(r) => self.editors.open_badge_menu_at(i, items, r),
            None => self.editors.open_badge_menu(i, items),
        }
        self.redraw();
    }

    /// ★ 통제의 단일 출구(§3): 지금 세션이 막혔으면 실행 계열 진입점을 한꺼번에 끄고, 풀리면 한꺼번에 켠다.
    /// 툴바 · 결과 도구줄(추가 페치·전체 조회·건수)이 같은 판정을 본다 · 값이 바뀔 때만 쓴다.
    pub(crate) fn sync_gate(&mut self) {
        let blocked = !self.gate().run_other;
        // 활성 그리드는 탭 전환으로 바뀌므로 매번 알린다(그리드가 바뀔 때만 도구줄을 다시 맞춘다).
        // 연결 전에는 서버로 나가는 결과 도구줄 버튼(새로고침·전체 조회·건수)을 전부 끈다(사용자 09-19) — 유휴 닫힘은 조용히 재접속하므로 연결로.
        self.grid
            .set_session_connected(self.sess.connected || self.sess.idle_closed);
        self.grid.set_session_blocked(blocked);
        if self.gate_shown == Some(blocked) {
            return;
        }
        self.gate_shown = Some(blocked);
        let mut inv = Invalidations::default();
        for id in ["run.all", "run.explain"] {
            self.tool_dock.set_item_enabled(id, !blocked, &mut inv);
        }
        // 메뉴바 Run 메뉴도 같은 판정(비활성 항목 · 09-19 검토).
        self.rebuild_menus();
        // 문장 실행(단일 커서 조건과 겹침) · Commit/Rollback(대기 문장 조건과 겹침)은 각자의 동기화가 통제 상태를 함께 본다.
        self.sync_run_stmt_button();
        self.sync_tx_ui();
        self.redraw();
    }

    /// 지금 세션의 통제 상태가 화면에 내는 값(순수 판정 `sessions::gate_view` · MC/DC 표 D6).
    pub(crate) fn gate(&self) -> sessions::GateView {
        sessions::gate_view(
            self.sess.busy,
            self.sess.aux,
            self.editors.cur().has_multi(),
            !self.sess.tx_pending.is_empty(),
        )
    }

    pub(crate) fn gate_open(&mut self) -> bool {
        self.gate_pass().is_some()
    }

    /// ★ 문지기(§3 · T-122): 지금 세션에 새 DB 작업을 보내도 되면 **증표**를 준다 — 막혔으면 상태줄 안내 + `None`.
    /// DB로 가는 진입점은 이 증표를 받아 `Sess::submit`에 넘긴다(증표 없이는 보낼 길이 없다).
    pub(crate) fn gate_pass(&mut self) -> Option<sessions::GatePass> {
        self.sync_sess();
        let pass = self.sess.pass();
        if pass.is_none() {
            self.sess.status = t(Msg::StRunning).into();
            self.redraw();
        }
        pass
    }

    /// 유휴 세션 점검(§6 · 30초 간격) — 닫아도 안전한 세션만 닫고 스펙은 남긴다(다음 실행 때 조용히 재접속).
    pub(crate) fn idle_tick(&mut self, now: Instant) {
        if now < self.idle_next {
            return;
        }
        self.idle_next = now + Duration::from_secs(30);
        let limit = self.settings.int("session.idle_secs").max(0) as u64;
        if limit == 0 {
            return;
        }
        // 탐색기 메타 세션도 같은 한도로 유휴 회수(트리는 그대로 · 다음 펼침 때 메타 스레드가 다시 연다).
        self.explorer.idle_tick(limit);
        let include_shared = self.settings.flag("session.idle_shared");
        let manual = !self.settings.flag("session.autocommit");
        let due: Vec<u64> = self
            .all_sess()
            .filter(|s| {
                sessions::idle_action(sessions::IdleInput {
                    dialect: s.dialect,
                    private: s.is_private(),
                    connected: s.connected && s.spec.is_some(),
                    blocked: s.blocked(),
                    // 수동 커밋이면 조회만 했어도 트랜잭션이 열려 있을 수 있다 → 닫지 않는다.
                    tx_open: !s.tx_pending.is_empty() || s.tx_dirty || (manual && s.tx_read),
                    stateful: s.stateful,
                    idle: now.saturating_duration_since(s.last_used),
                    limit_secs: limit,
                    include_shared,
                }) == sessions::IdleAction::Close
            })
            .map(|s| s.id)
            .collect();
        for id in due {
            self.with_sess(id, |a| {
                // 닫기(commit + logoff)가 죽은 소켓에 갇혀도 세션의 큐가 막히지 않게 — 옛 워커에 Disconnect를 남기고 새 워커로(docs/53 §4-8).
                a.sess.disc_path = Some(sessions::DiscPath::Idle);
                a.abandon_worker();
                let m = tf(Msg::StSessIdleClosed, &[&a.sess.desc]);
                a.log_win.push(LogEntry::new(LogKind::Info, m.clone()));
                a.sess.status = m;
            });
        }
        self.sync_sess_ui();
    }

    /// 실행 직전: 유휴로 닫힌 세션이면 같은 스펙으로 먼저 다시 붙는다(워커는 순차라 뒤따르는 실행은 접속 뒤에 돈다).
    pub(crate) fn wake_if_idle(&mut self) {
        if self.sess.idle_closed {
            if let Some(spec) = self.sess.spec.clone() {
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    tf(Msg::StReconnecting, &[&spec.redacted()]),
                ));
                self.connect_quietly(spec, false);
                // 이 접속의 완료 신호는 뒤따르는 작업의 busy를 풀면 안 된다.
                self.sess.skip_done += 1;
            }
        }
    }

    /// 탭의 전용 세션 해제 — 미커밋이 있으면 먼저 묻는다. 공유 모드 = 세션을 거두고 공유 세션으로 복귀 ·
    /// 개별 모드 = 세션은 남기되 끊긴 상태(그 탭은 다시 접속할 때까지 실행 불가).
    pub(crate) fn disconnect_private(&mut self, tab: u64) {
        let keep = self.session_mode() == SessionMode::PerEditor;
        self.disconnect_private_as(tab, keep);
    }

    /// `keep` = 세션 객체를 **미연결**로 남긴다(개별 모드 · 표식 메뉴 "미연결") · 아니면 거두고 공유 세션으로 복귀(공유 모드).
    fn disconnect_private_as(&mut self, tab: u64, keep: bool) {
        let Some(id) = self
            .all_sess()
            .find(|s| s.owner == Some(tab) && !s.closing)
            .map(|s| s.id)
        else {
            return;
        };
        let per_editor = keep;
        // 미커밋이 있으면 먼저 묻는다(잃는 순간만 모달 · DR-30) — 답한 뒤 `disconnect_force`가 다시 여기로 온다.
        let pending = self
            .sess_by_id(id)
            .is_some_and(|s| !s.tx_pending.is_empty());
        let step = sessions::private_disconnect_step(self.sess.id == id, pending);
        if step == sessions::PrivateDisc::Ask {
            self.tx_after = Some(TxAfter::Disconnect);
            self.open_tx_guard(Msg::MnTxCommitDisconnect, Msg::MnTxRollbackDisconnect);
            self.redraw();
            return;
        }
        self.with_sess(id, |a| {
            if step == sessions::PrivateDisc::Refuse {
                a.sess.status = t(Msg::StSessTxPending).into();
                a.log_win
                    .push(LogEntry::new(LogKind::Error, a.sess.status.clone()));
                return;
            }
            let stuck = a.sess.busy;
            a.log_disconnect(sessions::DiscPath::Badge);
            a.sess.control(worker::Cmd::Disconnect);
            a.editors.set_running(a.sess.run_editor, false);
            if per_editor {
                // 갇힌 워커는 버리고 새 워커로(즉시 해제 규약 · 09-16) — 세션 객체는 남는다.
                if stuck {
                    let (w, ev) = a.spawn_worker();
                    a.sess.worker = w;
                    a.sess.events = ev;
                }
                a.sess.busy = false;
                a.sess.aux = 0;
                a.sess.connected = false;
                a.sess.user_disconnected = true;
                a.sess.idle_closed = false;
                a.sess.status = t(Msg::StSessDisconnected).into();
            } else {
                a.sess.closing = true;
            }
            a.tx_close(TxOutcome::Lost);
            let m = tf(Msg::StSessClosed, &[&a.sess.desc]);
            a.log_win.push(LogEntry::new(LogKind::Info, m));
        });
        if !per_editor {
            self.freeze_tab_results();
        }
        self.reap_sessions();
        self.sync_sess();
        if !per_editor {
            self.sess.status = t(Msg::StSessBackToShared).into();
        }
        self.sync_sess_ui();
        self.redraw();
    }

    /// 탭 표식 메뉴(§7): 세션 설명 · 해제(공유 복귀) · 다시 접속 · 공유 연결 고르기(공유 연결이 둘 이상일 때).
    /// 탭 표식 메뉴(사용자 09-18 확정 — 꼭 필요한 것만): `No connection` · 연결할 수 있는 공유 연결 목록 · (전용 연결이 있을 때만)
    /// 구분자 + 전용 연결 한 줄. **지금 이 탭이 쓰는 것 앞에만 ✓**(1탭 1연결 · 배타) · 체크 유무와 무관하게 글자는 같은 열.
    /// `No connection` = 이 탭은 어떤 서버에도 연결되지 않은 상태(전용 연결이 있으면 그 연결을 해제).
    pub(crate) fn open_badge_menu(&mut self, i: usize) {
        let Some(items) = self.badge_menu_items(i) else {
            return;
        };
        self.badge_menu_tab = Some(self.editors.tab_id(i));
        self.editors.open_badge_menu(i, items);
        self.redraw();
    }

    /// 표식 메뉴 항목(탭 표식 · 툴바 탭 연결정보가 같이 쓴다).
    fn badge_menu_items(&self, i: usize) -> Option<Vec<nexa_ctl::controls::ctxmenu::CtxItem>> {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let tab = self.editors.tab_id(i);
        let s = self.sess_by_id(self.sess_id_for_tab(tab))?;
        let cur = s.id;
        let private = s.is_private();
        // 이 탭의 세션이 접속돼 있지 않으면(시작 직후 · 해제 뒤 · 미연결 자리) `No connection`에 ✓ — 목록의 다른 줄은 접속된 것만이라 배타.
        let unconnected = !s.connected && !s.busy && !s.idle_closed;
        let mut items =
            vec![CtxItem::item("sess.none", t(Msg::MnSessNoConnection)).with_mark(unconnected)];
        for sh in self
            .all_sess()
            .filter(|s| !s.is_private() && !s.closing && (s.connected || s.busy))
        {
            let label = if sh.broken {
                format!("{} · {}", sh.desc, t(Msg::StSessBrokenTag))
            } else {
                sh.desc.clone()
            };
            items.push(
                CtxItem::item(format!("sess.use:{}", sh.id), label)
                    .with_mark(!private && sh.id == cur),
            );
        }
        if private && !unconnected {
            items.push(CtxItem::Separator);
            let label = if s.broken {
                format!("{} · {}", s.desc, t(Msg::StSessBrokenTag))
            } else if !s.connected {
                format!("{} · {}", s.desc, t(Msg::ExpOffline))
            } else {
                s.desc.clone()
            };
            items.push(CtxItem::item("sess.private", label).with_mark(true));
        }
        // ★ 서버 유형(없음/개발/테스트/운영) 지정 + 접속 정보 → Output(10-01 · CONNECT 세션도 유형을 가질 수 있게) ·
        //   여기서 바꾸면 **이 세션만 임시**(머리글로 알린다 · 영속 = 프로필 편집).
        if s.connected {
            items.push(CtxItem::Separator);
            items.push(CtxItem::maybe(
                "sess.env.hdr",
                t(Msg::MnEnvSessionHdr),
                false,
            ));
            let cur_env = s.spec.as_ref().and_then(|x| x.env);
            for (id, label, env) in [
                ("sess.env:none", Msg::MnEnvNone, None),
                (
                    "sess.env:dev",
                    Msg::MnEnvDev,
                    Some(nsql_script::ConnEnv::Dev),
                ),
                (
                    "sess.env:test",
                    Msg::MnEnvTest,
                    Some(nsql_script::ConnEnv::Test),
                ),
                (
                    "sess.env:prod",
                    Msg::MnEnvProd,
                    Some(nsql_script::ConnEnv::Prod),
                ),
            ] {
                items.push(CtxItem::item(id, t(label)).with_mark(cur_env == env));
            }
            items.push(CtxItem::Separator);
            items.push(CtxItem::item("sess.info", t(Msg::MnSessInfo)));
        }
        Some(items)
    }

    pub(crate) fn badge_pick(&mut self, tab: u64, id: &str) {
        match id {
            "sess.disconnect" => self.disconnect_private(tab),
            "sess.none" => self.make_unconnected(tab),
            "sess.info" => self.output_conn_info(),
            // ★ 작업 단위에서 고른 DB(10-01 ⑫) = 편집기 `USE`와 같은 길(세션 · 러너 `DbChanged` → 탐색기·툴바).
            //   접두는 `sess.db:` — `sess.use:<세션 번호>`(아래 · 공유 연결 고르기)와 달라야 한다(㉔ · 둘이 같아
            //   연결 고르기가 `CURRENT_SCHEMA = "0"`으로 나갔다).
            x if x.starts_with("sess.db:") => {
                let db = &x["sess.db:".len()..];
                if !db.is_empty() {
                    if let Some(sql) = sessions::use_sql(self.sess.dialect, db) {
                        self.run_text_whole(sql);
                    }
                }
            }
            x if x.starts_with("sess.env:") => {
                self.set_session_env_guarded(nsql_script::ConnEnv::from_name(
                    &x["sess.env:".len()..],
                ));
            }
            // 전용 연결 줄 = 이미 이 탭의 것 — 끊겨 있으면 다시 접속, 아니면 아무것도 안 함.
            "sess.private" => {
                if self
                    .sess_by_id(self.sess_id_for_tab(tab))
                    .is_some_and(|s| !s.connected)
                {
                    self.badge_pick(tab, "sess.reconnect");
                }
            }
            "sess.reconnect" => {
                let Some(sid) = self
                    .all_sess()
                    .find(|s| s.owner == Some(tab) && !s.closing)
                    .map(|s| s.id)
                else {
                    return;
                };
                let fallback = self.default_spec.clone();
                // 표식 메뉴의 "다시 접속" = 사용자의 명시적 요청 → 설정 `connect.reconnect_same`이 켜져 있으면 끊고 다시.
                let again = self.settings.flag("connect.reconnect_same");
                self.with_sess(sid, |a| {
                    if let Some(spec) = a.sess.spec.clone().or(fallback) {
                        a.connect_quietly(spec, again);
                    }
                });
                self.sync_sess_ui();
            }
            id if id.starts_with("sess.use:") => {
                if let Ok(sid) = id["sess.use:".len()..].parse::<u64>() {
                    // 전용 탭이면 먼저 그 세션을 닫는다(미커밋이 있으면 확인 팝업이 뜨고 여기서는 묶지 않는다).
                    if self.all_sess().any(|s| s.owner == Some(tab) && !s.closing) {
                        self.disconnect_private(tab);
                    }
                    if !self.all_sess().any(|s| s.owner == Some(tab) && !s.closing) {
                        self.tab_bind.insert(tab, sid);
                        // 다른 서버의 결과를 이어 받지 않게.
                        self.freeze_results_of(tab);
                        self.sync_sess();
                        // 새 세션에서의 이 탭 단위(기억 없음 = 그 연결의 기본값)로 조용히 맞춘다(㉔).
                        self.apply_tab_unit();
                        self.sync_sess_ui();
                    }
                }
            }
            _ => {}
        }
        self.redraw();
    }

    /// DB 워커 하나 시작(설정 현재값 · 시작 때와 같은 인자).
    fn spawn_worker(&self) -> (worker::Handle, mpsc::Receiver<RunEvent>) {
        let proxy = self.wake_proxy.clone();
        worker::spawn(
            DEFAULT_DIALECT,
            self.settings.int("grid.max_rows").max(0) as usize,
            self.settings.flag("session.autocommit"),
            Box::new(move || {
                let _ = proxy.send_event(Wake);
            }),
        )
    }

    /// ★ 접속 해제 — **서버 상태와 무관하게 즉시**(사용자 09-16: VPN 끊긴 채 조회가 "Loading…"에 멈추면 Disconnect가
    /// 무반응이었다). 워커는 순차라 앞 명령(실행 · 세션 commit/close)이 응답 없는 서버에서 TCP 타임아웃까지 갇힌다 →
    /// 기다리지 않는다: 옛 워커에는 Disconnect를 남기고 손잡이를 버린다(갇힌 호출이 풀리면 세션을 닫고 스스로 끝난다 ·
    /// 늦게 오는 이벤트는 버려진 채널로 사라진다) · 새 워커를 만들어 다음 접속을 받는다 · UI는 지금 해제 상태로.
    /// 탐색기 메타 스레드도 같은 방식(`Explorer::disconnect`).
    pub(crate) fn disconnect_now(&mut self) {
        // ★ 툴바 Disconnect의 뜻은 지금 탭이 쥔 연결로 정한다(docs/54 · `sessions::disconnect_plan` D16):
        //   전용 = 이 탭만 · 공유 혼자 = 바로 · 공유를 다른 탭도 쓰면 "모두 해제 / 이 탭만 떼기 / 취소".
        let bound = self.bound_tabs(self.sess.id);
        if sessions::disconnect_plan(self.sess.is_private(), bound)
            == sessions::DisconnectPlan::SharedAsk
        {
            self.open_disc_guard(bound);
            return;
        }
        self.disconnect_current();
    }

    /// 접속 해제 로그 한 줄(사용자 09-19 "어느 경로로 어떤 서버를") — 지금 세션 기준: 종류(공유/전용[탭]) · 대상 · 경로.
    /// 경로는 `Sess.disc_path`(진입점이 미리 표시) · 없으면 `fallback`. 워커를 바꾸는 해제는 `Disconnected` 이벤트가 오지
    /// 않으므로 여기서 바로 남기고, 이벤트로 오는 해제(스크립트·서버)는 `drain_events`가 같은 문장으로 남긴다.
    pub(crate) fn log_disconnect(&mut self, fallback: sessions::DiscPath) {
        let path = self.sess.disc_path.unwrap_or(fallback);
        self.sess.disc_path = Some(path);
        let kind = match self.sess.owner {
            Some(tab) => {
                let title = self
                    .editors
                    .tab_list()
                    .into_iter()
                    .find(|(id, _, _)| *id == tab)
                    .map(|(_, t, _)| t)
                    .unwrap_or_default();
                tf(Msg::LogSessPrivate, &[&title])
            }
            None => t(Msg::LogSessShared).to_string(),
        };
        let target = self
            .sess
            .spec
            .as_ref()
            .map(nsql_script::ConnectSpec::redacted)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.sess.desc.clone());
        let line = tf(Msg::LogDisconnected, &[&kind, &target, t(path.msg())]);
        let e = LogEntry::new(LogKind::Disconnect, line);
        if let Some(h) = &self.log_hub {
            h.push(e.clone());
        }
        self.log_win.push(e);
    }

    /// 세션 `id`에 해제 경로를 표시(진입점이 부른다 · 그 뒤의 해제가 로그에 경로를 남긴다).
    fn mark_disc(&mut self, id: u64, path: sessions::DiscPath) {
        self.with_sess(id, |a| a.sess.disc_path = Some(path));
    }

    /// 공유 연결을 다른 탭도 쓸 때의 확인 팝업(잃는 순간 규칙과 같은 부품).
    fn open_disc_guard(&mut self, bound: usize) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let others = bound.saturating_sub(1).to_string();
        self.sess.status = tf(Msg::StDiscGuard, &[&others]);
        let items = vec![
            CtxItem::item("disc.all", tf(Msg::MnDiscAll, &[&bound.to_string()])),
            CtxItem::item("disc.detach", t(Msg::MnDiscDetach)),
            CtxItem::Separator,
            CtxItem::item("disc.cancel", t(Msg::MnTxCancel)),
        ];
        let r = self
            .tool_dock
            .item_rect("conn.disconnect")
            .unwrap_or(self.status_tx_rect);
        self.open_status_popup(Rect::new(r.x, r.bottom(), 0, 0), items);
        self.redraw();
    }

    pub(crate) fn disc_pick(&mut self, id: &str) {
        match id {
            "all" => self.disconnect_current(),
            // 이 탭만 떼기 = 미연결 자리(다른 탭들은 그대로 공유 연결을 쓴다).
            "detach" => self.make_unconnected(self.editors.active_id()),
            _ => {}
        }
    }

    /// 이 서버에 붙은 세션 전부 해제(탐색기 루트 메뉴 · docs/54): 공유 세션 = 해제(묶인 탭은 미연결로 보임) ·
    /// 전용 세션 = 그 탭을 미연결로 · 메타 세션은 참조 수 0이 되면 `sync_sess_ui`가 닫는다(트리는 오프라인으로 남는다).
    pub(crate) fn disconnect_server(&mut self, spec: &ConnectSpec) {
        let ids: Vec<(u64, Option<u64>)> = self
            .all_sess()
            .filter(|s| {
                !s.closing
                    && s.spec
                        .as_ref()
                        .is_some_and(|have| worker::same_server(have, spec))
            })
            .map(|s| (s.id, s.owner))
            .collect();
        for (id, owner) in ids {
            self.mark_disc(id, sessions::DiscPath::Explorer);
            match owner {
                Some(tab) => self.disconnect_private_as(tab, true),
                None => self.disconnect_shared(id),
            }
        }
        self.sync_sess();
        self.sync_sess_ui();
        self.redraw();
    }

    /// 오프라인 루트를 다시 연결(탐색기 루트 메뉴): 이 서버의 공유 세션이 남아 있으면 그 세션으로 · 없으면 공유 세션을 새로.
    pub(crate) fn connect_server(&mut self, spec: ConnectSpec) {
        let existing = self
            .all_sess()
            .find(|s| {
                !s.closing
                    && !s.is_private()
                    && s.spec
                        .as_ref()
                        .is_some_and(|have| worker::same_server(have, &spec))
            })
            .map(|s| s.id);
        let id = match existing {
            Some(id) => id,
            None => self.new_shared(),
        };
        let again = self.settings.flag("connect.reconnect_same");
        self.with_sess(id, |a| a.connect_quietly(spec, again));
        self.activate_shared(id);
        self.sync_sess_ui();
        self.redraw();
    }

    /// 이 연결로 새 탭(탐색기 루트 메뉴): 새 탭을 만들고 이 서버의 공유 세션에 묶는다(없으면 그냥 새 탭).
    pub(crate) fn new_tab_on(&mut self, spec: &ConnectSpec) {
        if !self.tab_room() {
            return;
        }
        self.editors.new_tab(None);
        self.set_focus(Focus::Editor);
        let tab = self.editors.active_id();
        self.bind_tab_to_server(tab, spec);
        self.sync_sess();
        self.sync_sess_ui();
        self.redraw();
    }

    /// ★ 탭을 이 서버의 세션에 묶는다(공유 연결 → 전용 세션만 있으면 같은 스펙으로 전용 하나 · 둘 다 없으면 그대로 · 09-30 부품화).
    pub(crate) fn bind_tab_to_server(&mut self, tab: u64, spec: &ConnectSpec) {
        let shared = self
            .all_sess()
            .find(|s| {
                !s.closing
                    && !s.is_private()
                    && s.spec
                        .as_ref()
                        .is_some_and(|have| worker::same_server(have, spec))
            })
            .map(|s| s.id);
        // ★ 이 서버에 공유 연결이 없고 **전용 세션만** 있다(편집기 `CONNECT`로 붙은 서버 · 사용자 09-21): 새 탭도 같은 스펙으로
        //   전용 세션을 하나 연다 — 종전에는 묶을 공유 연결이 없어 새 탭이 "연결 없음"으로 남았다. 스펙에 비밀번호가 없으면
        //   워커가 세션 자격 금고에서 꺼내 쓰고, 금고에도 없으면 한 번 묻는다.
        let private_spec = if shared.is_none() {
            self.all_sess()
                .filter(|s| !s.closing && s.is_private())
                .filter_map(|s| s.spec.clone())
                .find(|have| worker::same_server(have, spec))
        } else {
            None
        };
        if let Some(sid) = shared {
            self.tab_bind.insert(tab, sid);
        } else if let Some(pspec) = private_spec {
            if let Some(id) = self.new_private(tab) {
                self.with_sess(id, |a| a.connect_quietly(pspec, false));
            }
        }
    }

    /// 지금 탭의 연결 해제(전용 = 그 세션 · 공유 = 그 공유 연결 전체).
    fn disconnect_current(&mut self) {
        // 미커밋 문장이 있으면 먼저 묻는다(잃는 순간만 모달 · DR-30).
        if !self.sess.tx_pending.is_empty() {
            self.tx_after = Some(TxAfter::Disconnect);
            self.open_tx_guard(Msg::MnTxCommitDisconnect, Msg::MnTxRollbackDisconnect);
            self.redraw();
            return;
        }
        self.disconnect_force();
    }

    pub(crate) fn disconnect_force(&mut self) {
        // 이 세션이 비밀번호를 묻는 중이면 그 물음부터 거둔다(입력 창을 닫고 워커에 취소 — 늦은 답이 새 워커로 가지 않게).
        if self.input_win.is_open()
            && self.input_win.is_password()
            && self.input_win.sess == self.sess.id
        {
            self.password_reply(worker::PwReply::Cancel);
        }
        // 전용 세션 탭에서의 해제 = 그 탭의 세션만(공유 모드면 공유 세션으로 복귀 · docs/52 §4).
        if let Some(tab) = self.sess.owner {
            self.disconnect_private(tab);
            return;
        }
        let stuck = self.sess.busy;
        self.log_disconnect(sessions::DiscPath::Toolbar);
        self.sess.control(worker::Cmd::Disconnect);
        let (w, ev) = self.spawn_worker();
        self.sess.worker = w;
        self.sess.events = ev;
        self.sess.busy = false;
        self.sess.aux = 0;
        self.sess.user_disconnected = true;
        self.sess.idle_closed = false;
        self.editors.set_running(self.sess.run_editor, false);
        self.sess.status = t(if stuck {
            Msg::StDisconnectedAbandon
        } else {
            Msg::StDisconnected
        })
        .into();
        if stuck {
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
        }
        self.tx_close(TxOutcome::Lost);
        self.on_conn_disconnected();
        self.sync_sess_ui();
        self.redraw();
    }

    /// 드라이버 네트워크 옵션(docs/53): keepalive · Oracle 호출 상한 — 다음 접속부터.
    pub(crate) fn apply_net_options(&self) {
        nsql_drivers::set_net_options(
            self.settings.int("net.keepalive_secs").max(0) as u64,
            self.settings.int("session.call_timeout_secs").max(0) as u64,
        );
    }

    /// 접속 계열 결과 `Disconnected`의 UI 반영(워커 이벤트 · 즉시 해제 공용).
    pub(crate) fn on_conn_disconnected(&mut self) {
        self.sess.live_sid = None;
        self.sess.connected = false;
        self.sess.key_cache.clear();
        self.sess.sql_wait = None;
        // 접속 창·공유 접속 설명은 **접속 창으로 붙은 세션**이 끊겼을 때만 되돌린다(전용 세션의 해제는 그 탭의 일 · docs/52).
        if self.sess.id != self.primary_sess {
            return;
        }
        // 해제됐으니 "접속됨" 결과는 더 이상 사실이 아니다(테스트 결과는 유지).
        self.panel_results
            .retain(|_, st| !matches!(st, ConnState::Connected(_)));
        self.sess.key_cache.clear();
        self.sess.sql_wait = None;
        self.editors.set_conn_desc("");
        self.conn_win.clear_connect_marks();
        self.conn_win.clear_active();
        self.panel_op = None;
        self.conn_win.panel.set_state(ConnState::Idle);
    }

    /// 툴바 접속 해제 버튼 = 접속돼 있을 때만 활성(사용자 09-15).
    /// 툴바 두 버튼(docs/52 §7-2): Disconnect = 끊을 연결이 하나라도 있으면 활성 · Connect(플러그) 색 = **지금 탭의 연결** —
    /// 초록(연결됨) · 빨강(끊김 확인) · 기본(미연결). 탭마다 연결이 다를 수 있으므로 "어딘가 연결됨"이 아니라 이 탭의 사실을 보인다.
    fn sync_disconnect_btn(&mut self, cur_connected: bool) {
        let mut inv = Invalidations::default();
        self.tool_dock
            .set_item_enabled("conn.disconnect", cur_connected, &mut inv);
        // 툴팁·배지 = 누르면 무슨 일이 나는지(docs/54): 전용 / 공유 / 공유 + 함께 쓰는 탭 수(배지).
        let bound = self.bound_tabs(self.sess.id);
        let desc = self.sess.desc.clone();
        let (tip, badge) = match sessions::disconnect_plan(self.sess.is_private(), bound) {
            sessions::DisconnectPlan::Private => (t(Msg::TipDisconnectPrivate).to_string(), None),
            sessions::DisconnectPlan::SharedAlone if cur_connected => {
                (tf(Msg::TipDisconnectShared, &[&desc]), None)
            }
            sessions::DisconnectPlan::SharedAlone => (t(Msg::TipDisconnect).to_string(), None),
            sessions::DisconnectPlan::SharedAsk => (
                tf(Msg::TipDisconnectSharedN, &[&desc, &bound.to_string()]),
                Some(bound.to_string()),
            ),
        };
        self.tool_dock.set_item_tip("conn.disconnect", &tip);
        self.tool_dock
            .set_item_badge("conn.disconnect", badge.as_deref(), &mut inv);
        let tone = if self.sess.connected && self.sess.broken {
            ToolTone::Danger
        } else if self.sess.connected {
            ToolTone::Ok
        } else {
            ToolTone::Default
        };
        self.tool_dock.set_item_tone("conn.toggle", tone, &mut inv);
    }

    /// 세션 창의 표 줄 — 세션 상태의 단일 원천(`Sess`)에서 그릴 때마다 만든다(복사는 줄 수만큼 · 세션은 몇 개뿐).
    pub(crate) fn session_rows(&self) -> Vec<SessRow> {
        let titles: HashMap<u64, String> = self
            .editors
            .tab_list()
            .into_iter()
            .map(|(id, title, _)| (id, title))
            .collect();
        let now = Instant::now();
        let mut rows: Vec<SessRow> = self
            .all_sess()
            .filter(|s| !s.closing && (s.connected || s.busy || s.idle_closed || s.is_private()))
            .map(|s| SessRow {
                id: s.id,
                server: if s.desc.is_empty() {
                    t(Msg::MnSessNoneConnected).to_string()
                } else {
                    s.desc.clone()
                },
                shared: !s.is_private(),
                active: s.id == self.default_shared && !s.is_private(),
                tab: s
                    .owner
                    .map(|tab| (tab, titles.get(&tab).cloned().unwrap_or_default())),
                connected: s.connected,
                broken: s.broken,
                busy: s.blocked(),
                idle_secs: now.saturating_duration_since(s.last_used).as_secs(),
                pending: s.tx_pending.len(),
            })
            .collect();
        // 접속 순(세션 id 순) · 같은 서버끼리는 창이 모은다.
        rows.sort_by_key(|r| r.id);
        rows
    }

    /// 중지(카드 ■ = 툴바 ■ · T-108): 실행 중 문장은 드라이버 취소 핸들로 서버에 취소 · 전체 조회는 다음 배치 경계에서.
    /// 지금 세션의 워커를 버리고 새 워커로(즉시 해제 규약 09-16 — 죽은 소켓에 갇힌 호출을 기다리지 않는다).
    /// 세션은 **끊김(Broken)으로 남기고 스펙을 지킨다** → 다음 동작 때 판정 뒤 조용히 재접속(`wake_if_idle`).
    fn abandon_worker(&mut self) {
        let (w, ev) = self.spawn_worker();
        self.log_disconnect(sessions::DiscPath::Stop);
        self.sess.control(worker::Cmd::Disconnect);
        self.sess.worker = w;
        self.sess.events = ev;
        self.sess.busy = false;
        self.sess.aux = 0;
        self.sess.skip_done = 0;
        self.sess.connected = false;
        self.sess.idle_closed = true;
        self.sess.run_cancel_requested = false;
        self.editors.set_running(self.sess.run_editor, false);
        self.run_toast
            .finish(self.sess.run_card, runtoast::Phase::Stopped { rows: 0 });
        self.grid.fetch_failed();
        self.tx_close(TxOutcome::Lost);
    }

    pub(crate) fn stop_run(&mut self) {
        if !self.sess.busy && !self.grid.fetch_all_active() {
            return;
        }
        // 입력 창이 답을 기다리는 중이면 ■ = 입력 취소(워커는 입력을 기다리며 멈춰 있다 — DB로 간 것이 없다).
        if self.input_win.is_open() && self.input_win.sess == self.sess.id {
            if self.input_win.is_password() {
                self.password_reply(worker::PwReply::Cancel);
            } else {
                self.input_reply(worker::InputReply::Cancel);
            }
            return;
        }
        // ★ 끊긴 서버(docs/53 §3): 취소(OCIBreak)도 같은 소켓으로 가 함께 막힌다 → 워커를 버리는 것이 유일한 즉시 중지.
        if self.sess.broken {
            self.abandon_worker();
            self.sess.status = tf(Msg::StSessBrokenStopped, &[&self.sess.desc]);
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            self.sync_sess_ui();
            self.redraw();
            return;
        }
        // ★ 두 번째 ■(사용자 10-09): 취소를 보냈는데 `run.force_stop_secs`가 지나도 서버가 안 멈추면 접속을 끊어 강제 중지 — 종전에는
        //   ■를 몇 번 눌러도 같은 취소만 다시 보냈다(OOB 차단망 · 원격 DB 링크 · 긴 서버 호출에서 수 초~분 기다림). 끊은 뒤 = 열린 트랜잭션
        //   Lost(서버 롤백) · 다음 실행 때 자동 재접속 · 로그 1줄.
        if self.sess.busy && self.sess.run_cancel_requested {
            let wait = Duration::from_secs(self.settings.int("run.force_stop_secs").max(0) as u64);
            if wait.as_secs() > 0 && self.sess.run_cancel_at.is_some_and(|t| t.elapsed() >= wait) {
                let desc = self.sess.desc.clone();
                self.abandon_worker();
                self.sess.run_cancel_at = None;
                self.sess.status = tf(Msg::StRunForceStopped, &[&desc]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                self.tx_close(TxOutcome::Lost);
                self.sync_sess_ui();
                self.redraw();
                return;
            }
        }
        self.sess.run_cancel_requested = self.sess.busy;
        // 첫 요청 시각은 재전송에도 유지(연타해도 강제 중지 창이 뒤로 밀리지 않게).
        self.sess.run_cancel_at = if self.sess.busy {
            self.sess.run_cancel_at.or_else(|| Some(Instant::now()))
        } else {
            None
        };
        if self.sess.dialect == Dialect::Mssql
            && self.settings.get("mssql.cancel") != Some("socket")
            && self.settings.get("mssql.encrypt") != Some("login")
        {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                t(Msg::StMssqlAttentionFallback),
            ));
        }
        // 시험 훅: 취소를 보내지 않고 "요청됨" 상태만(강제 중지 경로 · Debug 전용).
        let (sent, drops) = if cfg!(debug_assertions) && std::mem::take(&mut self.sess.cancel_mute)
        {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                "[test] run.stop_mute: cancel not sent".to_string(),
            ));
            (true, false)
        } else {
            self.sess.worker.cancel_run()
        };
        self.sess.run_cancel_drops = drops;
        self.sess.status = t(if sent && drops {
            Msg::StRunCancellingDrop
        } else if sent {
            Msg::StRunCancelling
        } else {
            Msg::StFetchCancelling
        })
        .into();
        self.log_win
            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
        self.redraw();
    }
}
