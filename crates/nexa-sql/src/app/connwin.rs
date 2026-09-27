//! App — 접속 창(로그인 목록·시험·프로필 관리 · docs/22).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 접속 패널의 요청 → 워커/저장소.
    pub(crate) fn handle_conn_win_action(&mut self, a: ConnWinAction) {
        match a {
            ConnWinAction::Paint => {
                let ui_px = self.settings.font_px("ui.font_size");
                self.conn_win.paint(&self.ui_font, &self.theme, ui_px);
            }
            ConnWinAction::Panel(a) => self.handle_panel_action(a),
            ConnWinAction::Login(name) => self.login_profile(&name),
            ConnWinAction::TestProfile(name) => self.test_profile(&name),
            ConnWinAction::Delete(name) => self.delete_profile(&name),
            ConnWinAction::Duplicate(name) => self.duplicate_profile(&name),
            ConnWinAction::SetEnv(name, env) => self.set_profile_env(&name, env),
            ConnWinAction::CopyText(text) => {
                if !clipboard::write_text(&text) {
                    self.sess.status = t(Msg::ErrClipboard).into();
                }
            }
        }
    }

    pub(crate) fn open_conn_window(&mut self, el: &ActiveEventLoop) {
        let over = self.window.as_ref().and_then(|w| {
            let p = w.outer_position().ok()?;
            let sz = w.outer_size();
            Some((p.x, p.y, sz.width, sz.height))
        });
        let owner = self.window.clone();
        self.conn_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            over,
            owner.as_deref(),
        );
        if let (Some(o), Some(c)) = (owner.as_deref(), self.conn_win.window()) {
            winfocus::attach_child(o, c);
        }
    }

    /// 로그인 목록 더블클릭/Enter — 저장소에서 읽어 폼에 채우고 바로 접속.
    /// 행 테스트 버튼 — 폼을 건드리지 않고 저장소 스펙으로 접속만 해 본다. 결과는 행 버튼 표시로.
    /// ★ 접속 테스트 시작 — 요청당 스레드(순차 워커·`busy`와 무관 · 실패 서버 타임아웃이 다른 테스트를 막지 않는다 · 사용자 09-14).
    /// 같은 프로필이 이미 테스트 중이면 무시.
    pub(crate) fn start_test(&mut self, name: &str, spec: ConnectSpec) {
        if self.conn_win.test_mark(name) == Some(TestMark::Testing) {
            return;
        }
        self.sess.status = t(Msg::StTesting).into();
        self.conn_win.set_test_mark(name, TestMark::Testing);
        self.panel_op = Some((name.to_string(), ConnState::Testing));
        if self.conn_win.panel.profile_name() == name {
            self.conn_win.panel.set_state(ConnState::Testing);
        }
        // 동시 상한·큐를 거친다(초과분은 대기 · 표시는 진행 중과 같은 노란 고리).
        self.attempt_queue.push_back(Attempt::Test {
            name: name.to_string(),
            spec,
        });
        self.dispatch_attempts();
        self.conn_win.redraw();
    }

    /// 큐에서 시도를 꺼내 상한까지 시작한다 — 결과가 올 때마다 다시 부른다.
    pub(crate) fn dispatch_attempts(&mut self) {
        while self.attempts_inflight < self.attempts_max {
            let Some(a) = self.attempt_queue.pop_front() else {
                break;
            };
            self.attempts_inflight += 1;
            // 시도 자체를 신호등 대상으로 기록(성공 여부 무관 · 사용자 09-16).
            match &a {
                Attempt::Test { name, .. } | Attempt::Connect { name, .. } => {
                    self.conn_win.mark_attempted(name);
                }
            }
            match a {
                Attempt::Test { name, spec } => {
                    let proxy = self.wake_proxy.clone();
                    worker::spawn_test(
                        name,
                        spec,
                        DEFAULT_DIALECT,
                        self.tests_tx.clone(),
                        Box::new(move || {
                            let _ = proxy.send_event(Wake);
                        }),
                    );
                }
                Attempt::Connect {
                    name: a_name,
                    spec,
                    reconnect_same,
                } => {
                    // 접속 창의 대상 세션(docs/52 §2): 공유 모드 = 기본 공유 세션(활성 탭이 전용이어도) · 개별 모드 = 활성 탭의 세션.
                    let Some(target) = self.login_place(&spec) else {
                        self.attempts_inflight = self.attempts_inflight.saturating_sub(1);
                        // 자리를 못 잡았다(세션 바쁨·상한) — 막을 걷는다(사유는 login_place가 상태줄에).
                        self.conn_win.veil_end(&a_name);
                        continue;
                    };
                    self.conn_win.veil_phase(&a_name, t(Msg::VeilConnecting));
                    let profile = a_name.clone();
                    self.with_sess(target, |a| {
                        a.sess.profile = profile;
                        a.sess.busy = true;
                        a.sess.attempt_inflight = true;
                        a.sess.user_disconnected = false;
                        a.sess.idle_closed = false;
                        a.sess.status = tf(Msg::StConnecting, &[&spec.redacted()]);
                        a.sess.last_spec = Some(spec.clone());
                        a.sess.spec = Some(spec.clone());
                        a.sess.touch();
                        a.sess.worker.send(worker::Cmd::ConnectSpec {
                            spec,
                            reconnect_same,
                        });
                    });
                }
            }
        }
    }

    /// 시도 하나가 끝났다(테스트 결과 · 접속 성공/실패) — 슬롯을 비우고 큐를 이어 간다.
    pub(crate) fn attempt_done(&mut self) {
        self.attempts_inflight = self.attempts_inflight.saturating_sub(1);
        self.dispatch_attempts();
    }

    fn test_profile(&mut self, name: &str) {
        match Vault::open_default().and_then(|v| v.get(name)) {
            Ok(Some(spec)) => {
                let spec = self.conn_win.with_session_pw(name, spec);
                // 행 Test = 행 선택 + (폼이 펼쳐져 있으면) 폼에 채움 + 폼 Test와 동일 경로(사용자 09-14 "두 행위 동일").
                self.conn_win.select_by_name(name);
                if self.conn_win.is_detail_open() && self.conn_win.panel.profile_name() != name {
                    self.handle_panel_action(PanelAction::LoadProfile(name.to_string()));
                }
                self.start_test(name, spec);
            }
            Ok(None) => self.sess.status = tf(Msg::StTestFailed, &[name]),
            Err(e) => self.sess.status = tf(Msg::StTestFailed, &[&e.to_string()]),
        }
        self.conn_win.redraw();
    }

    fn login_profile(&mut self, name: &str) {
        // (바쁜 세션 판정은 시도를 보낼 때 `login_place`가 한다 — 공유 연결이 여럿이라 대상은 스펙을 봐야 정해진다.)
        if self.conn_win.is_testing(name) {
            self.sess.status = t(Msg::StTesting).into();
            return;
        }
        match Vault::open_default().and_then(|v| v.get(name)) {
            Ok(Some(spec)) => {
                let spec = self.conn_win.with_session_pw(name, spec);
                self.conn_win.panel.fill(name, &spec);
                self.conn_win.panel.set_state(ConnState::Idle);
                if let Some(a) = self.conn_win.panel.connect_action() {
                    self.handle_panel_action(a);
                }
            }
            Ok(None) => self
                .conn_win
                .panel
                .set_state(ConnState::Failed(name.to_string())),
            Err(e) => self
                .conn_win
                .panel
                .set_state(ConnState::Failed(e.to_string())),
        }
        self.conn_win.redraw();
    }

    /// 우클릭 Duplicate — `<이름>_Copied`(있으면 `_Copied2`…)로 저장(비밀번호 봉투 포함).
    /// 목록 우클릭 메뉴의 접속 유형 — 저장소의 그 프로필만 고친다(비밀번호 봉투 포함 그대로 다시 쓴다). 지금 붙어 있는 세션이
    /// 그 프로필이면 세션의 표식도 바로 바꾼다(다시 접속할 필요 없음).
    fn set_profile_env(&mut self, name: &str, env: Option<nsql_script::ConnEnv>) {
        let r = Vault::open_default().and_then(|v| {
            let Some(mut spec) = v.get(name)? else {
                return Ok(false);
            };
            spec.env = env;
            v.save(name, &spec)?;
            Ok(true)
        });
        match r {
            Ok(true) => {
                let label = match env {
                    Some(nsql_script::ConnEnv::Prod) => t(Msg::MnEnvProd),
                    Some(nsql_script::ConnEnv::Test) => t(Msg::MnEnvTest),
                    Some(nsql_script::ConnEnv::Dev) => t(Msg::MnEnvDev),
                    None => t(Msg::MnEnvNone),
                };
                self.sess.status = tf(Msg::StEnvSet, &[name, label]);
                self.conn_win.refresh_profiles(Some(name));
                let ids: Vec<u64> = self
                    .all_sess()
                    .filter(|s| s.profile == name)
                    .map(|s| s.id)
                    .collect();
                for id in ids {
                    self.with_sess(id, |a| {
                        if let Some(sp) = a.sess.spec.as_mut() {
                            sp.env = env;
                        }
                    });
                }
            }
            Ok(false) => {}
            Err(e) => self.sess.status = e.to_string(),
        }
        self.conn_win.redraw();
        self.redraw();
    }

    fn duplicate_profile(&mut self, name: &str) {
        let r = Vault::open_default().and_then(|v| {
            let Some(spec) = v.get(name)? else {
                return Ok(None);
            };
            let names: Vec<String> = v.list()?.into_iter().map(|p| p.name).collect();
            let mut new = format!("{name}_Copied");
            let mut n = 2;
            while names.contains(&new) {
                new = format!("{name}_Copied{n}");
                n += 1;
            }
            if !nsql_vault::is_profile_name(&new) {
                return Ok(None);
            }
            v.save(&new, &spec)?;
            Ok(Some(new))
        });
        match r {
            Ok(Some(new)) => {
                self.sess.status = tf(Msg::WkProfileSaved, &[&new, ""]);
                self.conn_win.refresh_profiles(Some(&new));
            }
            Ok(None) => self.sess.status = t(Msg::ErrProfileName).into(),
            Err(e) => self.sess.status = e.to_string(),
        }
        self.conn_win.redraw();
    }

    fn delete_profile(&mut self, name: &str) {
        match Vault::open_default().and_then(|v| v.remove(name)) {
            Ok(_) => {
                self.sess.status = tf(Msg::StProfileDeleted, &[name]);
                self.panel_results.remove(name.trim());
            }
            Err(e) => self.sess.status = e.to_string(),
        }
        // 인접 항목 자동 선택 · 폼이 펼쳐져 있으면 그 항목으로 갱신(비면 New 상태).
        if let Some(next) = self.conn_win.after_delete() {
            self.handle_panel_action(PanelAction::LoadProfile(next));
        }
        self.redraw();
    }
}
