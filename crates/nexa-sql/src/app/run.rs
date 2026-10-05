//! App — SQL 실행·설명 계획·입력 응답(docs/43).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 실행할 스크립트를 보고 세션 배치를 정한다(§4). 반환 false = 실행하지 않는다(이미 처리했거나 거부).
    ///  - `CONNECT 대상`이 먼저 나오면: 이 탭의 전용 세션으로(없으면 만든다) — 스크립트 전체가 그 세션에서 돈다.
    ///  - `DISCONNECT`가 먼저 나오고 이 탭이 전용 세션이면: 그 세션을 닫는다(공유 모드 = 공유 세션 복귀 · 개별 모드 = 실행 불가 상태).
    ///    공유 세션 탭의 `DISCONNECT`는 종전대로 워커가 처리한다(공유 세션 해제).
    fn place_run(&mut self, src: &mut String) -> bool {
        let tab = self.editors.active_id();
        let intent = sessions::connect_intent(src);
        let plan = sessions::placement(
            intent.as_ref(),
            self.sess.is_private(),
            self.settings.flag("session.private_connect"),
            intent.is_some() && sessions::statements_before_connect(src),
        );
        match plan {
            sessions::Placement::Refuse => {
                // D-99: 새 전용 세션에는 아직 접속이 없다 — 앞 문장이 조용히 "not connected"로 실패하게 두지 않는다.
                self.sess.status = t(Msg::StSessConnectFirst).into();
                self.log_win
                    .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                self.toasts.push(
                    toast::ToastKind::Error,
                    "CONNECT".to_string(),
                    self.sess.status.clone(),
                );
                self.redraw();
                false
            }
            sessions::Placement::NewPrivate | sessions::Placement::Retarget => {
                if plan == sessions::Placement::NewPrivate {
                    let Some(id) = self.new_private(tab) else {
                        self.redraw();
                        return false;
                    };
                    // 공유 세션의 결과는 새 세션에서 이어 받을 수 없다 → 이 탭의 옛 결과는 "더 있음"을 내린다.
                    self.freeze_tab_results();
                    self.sync_sess();
                    if self.sess.id != id {
                        return false;
                    }
                }
                if let Some(ConnectIntent::Connect(spec)) = intent {
                    // 전용 탭의 CONNECT가 **같은 서버·계정**이면(동일성 = `same_server`) 설정 `connect.reconnect_same`에 따라:
                    //   끔 = 기존 접속 유지(CONNECT 명령만 지우고 나머지 실행) · 켬 = 명시적 재접속(끊고 다시).
                    // ★ 프로필 이름(`CONNECT M4PLAN`)은 저장소에서 **완성한 스펙으로 비교**한다 — 이름뿐인 스펙은 어떤 접속과도
                    //   같지 않아서 되풀이할 때마다 다시 접속했다(사용자 09-21). 세션의 스펙은 접속 뒤 이미 완성본이다(`explorer_attach`).
                    let target = sessions::bare_profile_name(&spec)
                        .and_then(|n| {
                            Vault::open_default()
                                .and_then(|v| v.resolve(n))
                                .ok()
                                .flatten()
                        })
                        .unwrap_or_else(|| spec.clone());
                    let same = plan == sessions::Placement::Retarget
                        && self.sess.connected
                        && !self.sess.broken
                        && self
                            .sess
                            .spec
                            .as_ref()
                            .is_some_and(|have| worker::same_server(have, &target));
                    //   ★ 단, **자격이 바뀐** CONNECT(`user:@host` · 다른 비밀번호)는 유지가 아니라 다시 접속이다(사용자 09-21).
                    let reconnect_setting = self.settings.flag("connect.reconnect_same");
                    let cred_changed = same && worker::credential_changed(&target, DEFAULT_DIALECT);
                    let keep = sessions::keep_same_session(same, reconnect_setting, cred_changed);
                    if keep {
                        *src = sessions::strip_first_connect(src);
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::StSessSameKept, &[&self.sess.desc]),
                        ));
                    } else {
                        // ★ 왜 다시 접속하는지 한 줄(사용자 09-25 "이미 접속됐는데 다시 접속" 진단): 계정·서버 다름 / 자격 변경 /
                        //   설정 / 접속 안 됨.
                        let why = if !self.sess.connected || self.sess.broken {
                            Msg::RsnNotConnected
                        } else if !same {
                            Msg::RsnAccountDiffers
                        } else if cred_changed {
                            Msg::RsnCredChanged
                        } else {
                            Msg::RsnReconnectSetting
                        };
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::StSessRetarget, &[&target.redacted(), t(why)]),
                        ));
                        self.sess.spec = Some(spec);
                    }
                }
                self.sess.user_disconnected = false;
                self.sess.idle_closed = false;
                true
            }
            sessions::Placement::ClosePrivate => {
                self.disconnect_private(tab);
                false
            }
            sessions::Placement::Run => {
                // 공유 연결이 여럿일 수 있다 → 탭은 **처음 실행한 연결에 묶인다**(활성 연결을 바꿔도 이 탭은 엉뚱한 서버로 가지 않는다).
                if !self.sess.is_private() && !self.tab_bind.contains_key(&tab) {
                    self.tab_bind.insert(tab, self.sess.id);
                    self.sess_ui_dirty = true;
                }
                // `session.private_connect = off`의 CONNECT = 이 세션이 대상을 바꾼다 → 탐색기·재접속이 새 대상을 알게.
                if let Some(ConnectIntent::Connect(spec)) = intent {
                    self.sess.spec = Some(spec);
                    self.sess.profile.clear();
                    self.sess.env_temp = false;
                }
                true
            }
        }
    }

    pub(crate) fn run_sql(&mut self, all: bool) {
        if !self.gate_open() {
            return;
        }
        // 다중 커서/선택이면 "캐럿 문장"이 하나가 아니다 → 문장 실행은 막고 전체 실행(F5)만(사용자 09-17).
        if !all && self.editors.cur().has_multi() {
            self.sess.status = t(Msg::StMultiCaretRun).into();
            self.redraw();
            return;
        }
        // ★ 객체 소스 탭의 F5(09-30 · 사용자 "소스 열기 후 F5가 줄 단위로 돈다 → 객체 단위로"): 본문 전체를 **한 항목**으로
        //   (분할 없음 · `Runner::run_whole`) — 프로시저·함수·패키지·트리거·타입은 컴파일 한 번 · 뷰는 CREATE 한 번. 결과·컴파일
        //   메시지는 Output 탭으로.
        if all {
            let tab = self.editors.active_id();
            if let Some(o) = self.object_tabs.get(&tab).cloned() {
                let src = self.ed_mut().text();
                let (kind, name) = o.label();
                self.sess.run_had_rs = false;
                self.output_push(
                    tab,
                    crate::output::OutKind::Info,
                    &tf(Msg::OutRunObject, &[&kind, &name]),
                );
                // ★ 객체 스키마 ≠ 세션 현재 스키마(10-01 · T-267): 한정이 켜져 있으면 안내 · 꺼져 있으면 강한 경고(현재 스키마에 만들어진다).
                if let Some(cs) = self.sess.cur_schema.clone() {
                    if !cs.eq_ignore_ascii_case(&o.schema) {
                        let qualified = self.settings.flag("explorer.source_schema");
                        let msg = if qualified {
                            tf(Msg::OutSchemaMismatch, &[&o.schema, &cs])
                        } else {
                            tf(Msg::OutSchemaMismatchUnqualified, &[&o.schema, &cs])
                        };
                        self.output_push(tab, crate::output::OutKind::Warn, &msg);
                        self.toasts.push(
                            toast::ToastKind::Warn,
                            t(Msg::OutSchemaMismatchTitle).to_string(),
                            msg,
                        );
                    }
                }
                self.run_text_whole(src);
                if self.sess.busy {
                    self.sess.status = tf(Msg::StRunObject, &[&kind, &name]);
                }
                return;
            }
        }
        // Ctrl/⌘+Enter = 선택 영역 → 없으면 **캐럿 위치의 한 문장**(`;` 종결 · 사용자 09-14) → F5 = 전체.
        let mut line_base = 0usize;
        let text = if all {
            None
        } else {
            let sel = self
                .ed_mut()
                .copy_selection()
                .filter(|s| !s.trim().is_empty());
            if sel.is_some() {
                // 선택 실행: 선택 시작 줄.
                if let Some((a, _)) = self.ed_mut().selection() {
                    let t = self.ed_mut().text();
                    line_base = t.chars().take(a).filter(|c| *c == '\n').count();
                }
                sel
            } else {
                let full = self.ed_mut().text();
                let byte_pos = full
                    .char_indices()
                    .nth(self.ed_mut().caret())
                    .map_or(full.len(), |(b, _)| b);
                // ★ 원문 조각(`span`)을 넘긴다 — `it.text`는 정규화된 실행 텍스트라(홀로 선 `EXEC` 줄 + 본문 = `EXEC 본문`)
                //   다시 스크립트로 분할하면 다른 항목이 된다(mac 09-21: `EXEC` 블록 + `SELECT … INTO :V`가 현재 문 실행에서만
                //   Msg 102 · 선택 실행과 같은 길 = 원문 그대로).
                nsql_script::statement_at_in(&full, byte_pos, Some(self.sess.dialect)).map(|it| {
                    line_base = it.line.saturating_sub(1);
                    full[it.span].to_string()
                })
            }
        };
        let src = text.unwrap_or_else(|| self.ed_mut().text());
        self.run_text(src, line_base, all);
    }

    /// 본문 실행의 공통 경로 — 편집기 실행(`run_sql`)과 **디스크에서 바로 실행**(docs/59 §4 3단계 · 편집기에 싣지 않는다)이 같이 쓴다.
    pub(crate) fn run_text(&mut self, src: String, line_base: usize, all: bool) {
        self.run_text_in(src, line_base, all, false);
    }

    /// ★ 본문 전체를 한 항목으로(객체 소스 탭 F5 · 09-30).
    pub(crate) fn run_text_whole(&mut self, src: String) {
        self.run_text_in(src, 0, true, true);
    }

    fn run_text_in(&mut self, mut src: String, line_base: usize, all: bool, whole: bool) {
        // ★ 세션 배치(docs/52 §4): `CONNECT`면 이 탭의 전용 세션으로 · 전용 탭의 `DISCONNECT`면 해제하고 끝.
        if !self.place_run(&mut src) {
            return;
        }
        // 문지기는 배치(전용 세션으로 옮김) **뒤** · 유휴 재접속(`busy`를 올린다) **앞**에서 — 실행은 그 접속 뒤에 줄 선다.
        let Some(pass) = self.gate_pass() else {
            return;
        };
        self.wake_if_idle();
        self.sess.touch();
        self.sess.run_line_base = line_base;
        let target = self.run_target_tab();
        if let Some(g) = self.grid_for(target) {
            g.set_source_sql(&src);
        }
        self.sess.run_tab = target;
        self.sess.run_fresh_prev = None;
        self.sess.run_set_stmt = None;
        self.sess.run_children = 0;
        self.sess.run_tracking = true;
        self.sess.run_editor = self.editors.active_id();
        self.sess.run_had_rs = false;
        if src.trim().is_empty() {
            self.sess.status = t(Msg::ErrNoSql).into();
            return;
        }
        // ★ 실행 필요 판단이 운영 판단보다 먼저(사용자 10-01): 지금 유형과 같은 `CONNTYPE`처럼 바꿀 것이 없는 항목만이면
        //   실행하지 않고 "변경 없음"만 알린다(운영 2단 확인도 뜨지 않는다).
        let cur_env = self.sess.spec.as_ref().and_then(|sp| sp.env);
        let needed = sessions::run_needed_items(&split_items(&src, self.sess.dialect), cur_env);
        if needed.is_empty() {
            let line = tf(
                Msg::OutSessEnvSame,
                &[&self.sess.desc, &super::drop::env_label(cur_env)],
            );
            self.sess.status = line.clone();
            self.output_push(
                self.editors.active_id(),
                crate::output::OutKind::Info,
                &line,
            );
            self.redraw();
            return;
        }
        // 운영 접속 + 변경 문장(서버로 가는 것) = 2단 실행(같은 본문을 3초 안에 다시 실행하면 진행 · 앱의 2단 확인 관례 · docs/56 §4).
        let prod = cur_env == Some(nsql_script::ConnEnv::Prod);
        if sessions::prod_confirm_needed(prod, self.settings.flag("run.prod_confirm"), &needed) {
            let key = nexa_fs::watch::content_hash(src.as_bytes());
            let armed = self
                .sess
                .prod_armed
                .is_some_and(|(k, at)| k == key && at.elapsed() <= Duration::from_secs(3));
            if !armed {
                self.sess.prod_armed = Some((key, Instant::now()));
                self.sess.status = t(Msg::StProdConfirm).into();
                self.toasts.push(
                    toast::ToastKind::Error,
                    t(Msg::StProdConfirmTitle),
                    t(Msg::StProdConfirm).to_string(),
                );
                self.redraw();
                return;
            }
            self.sess.prod_armed = None;
        }
        self.sess.busy = true;
        self.sess.status = t(Msg::StRunning).into();
        self.run_toast_start(&src);
        // 신호등이 초록이 아닌 서버(빨강·파랑·확인 중·모름)에는 실행 전 빠른 포트 판정을 건다(사용자 09-14).
        let pol = *self.conn_win.policy();
        let light = self.conn_win.status_of(self.conn_win.active_name());
        let preflight =
            (pol.enabled && light != Some(probe::ProbeStatus::Up)).then_some(pol.timeout);
        self.sess.last_run_items = if whole {
            vec![src.clone()]
        } else {
            split_items(&src, self.sess.dialect)
        };
        // 세션 상태 표식(`stateful`)은 여기서 일괄로 켜지 않는다 — 문장이 **서버로 나가는 순간**(`RunEvent::Begin`) 그때의 세션에 켠다.
        // (실행 전 일괄이면 `CONNECT` 뒤 문장의 상태가 **앞** 세션에 붙어 재접속 직후 헛경고가 났다 · 사용자 09-30.)
        let max_rows = self.grid.page_rows();
        self.sess.single_run = !all;
        self.sess.submit(
            pass,
            worker::Cmd::Run {
                src,
                preflight,
                max_rows,
                vars: self.run_vars(),
                defines: self.run_defines(),
                intrinsic: Some(self.run_intrinsic()),
                whole,
            },
        );
        self.live_start();
        self.redraw();
    }

    /// 실행 계획(사용자 09-15 기본 기능) — 캐럿 문장(또는 선택)을 방언별 EXPLAIN 관용으로 감싸 실행.
    pub(crate) fn run_explain(&mut self) {
        let Some(pass) = self.gate_pass() else {
            return;
        };
        let text = self
            .ed_mut()
            .copy_selection()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                let full = self.ed_mut().text();
                let byte_pos = full
                    .char_indices()
                    .nth(self.ed_mut().caret())
                    .map_or(full.len(), |(b, _)| b);
                // 원문 조각(`span`) — 위 `run_sql`과 같은 이유(정규화된 `text`는 재분할하면 달라진다).
                nsql_script::statement_at_in(&full, byte_pos, Some(self.sess.dialect))
                    .map(|it| full[it.span].to_string())
            });
        let Some(stmt) = text.filter(|s| !s.trim().is_empty()) else {
            self.sess.status = t(Msg::ErrNoSql).into();
            return;
        };
        let src = nsql_script::explain_script(self.sess.dialect, &stmt);
        self.wake_if_idle();
        self.sess.touch();
        self.sess.run_tab = self.run_target_tab();
        self.sess.run_set_stmt = None;
        self.sess.run_children = 0;
        self.sess.run_tracking = true;
        self.sess.run_editor = self.editors.active_id();
        self.sess.run_had_rs = false;
        self.sess.busy = true;
        self.sess.status = t(Msg::StRunning).into();
        self.run_toast_start(&src);
        self.sess.last_run_items = split_items(&src, self.sess.dialect);
        self.sess.submit(
            pass,
            worker::Cmd::Run {
                src,
                preflight: None,
                max_rows: self.grid.page_rows(),
                vars: self.run_vars(),
                defines: self.run_defines(),
                intrinsic: Some(self.run_intrinsic()),
                whole: false,
            },
        );
        self.live_start();
        self.redraw();
    }

    /// 마지막 실행 대상 결과 탭의 그리드(활성이면 `grid` · 아니면 잠든 것 · 닫혔으면 `None`).
    /// 입력 창의 답을 그 세션의 워커에 보내고 창을 닫는다(다중 세션: 물은 세션에게).
    /// 비밀번호 입력 창의 답을 **물은 세션의 워커**에 보내고 창을 닫는다. 값이면 둘째 사본을 잠깐 쥔다 — 같은 서버의 탐색기
    /// 메타 세션이 같은 자격으로 붙어야 트리가 보인다(`explorer_attach`가 소비 · 못 쓰면 20초 뒤 폐기).
    pub(crate) fn password_reply(&mut self, reply: worker::PwReply) {
        let sid = self.input_win.sess;
        self.input_win.close();
        self.key_guard = Some(Instant::now());
        self.pw_pending = None;
        // 세션 자격 금고가 켜져 있으면 탐색기 메타 세션은 금고에서 빌린다 → 둘째 사본을 쥘 필요가 없다.
        let lend = !worker::remember_session_password();
        self.pw_once = match &reply {
            worker::PwReply::Value(s) if lend => Some((
                sid,
                nsql_core::Secret::new(s.expose().to_string()),
                Instant::now(),
            )),
            _ => None,
        };
        if sid == self.sess.id {
            self.sess.worker.password(reply);
        } else {
            let mut reply = Some(reply);
            self.with_sess(sid, |a| {
                if let Some(r) = reply.take() {
                    a.sess.worker.password(r);
                }
            });
        }
        if let Some(w) = &self.window {
            w.focus_window();
        }
        self.redraw();
    }

    pub(crate) fn input_reply(&mut self, reply: worker::InputReply) {
        let sid = self.input_win.sess;
        self.input_win.close();
        self.key_guard = Some(Instant::now());
        self.input_pending = None;
        if sid == self.sess.id {
            self.sess.worker.input(reply);
        } else {
            let mut reply = Some(reply);
            self.with_sess(sid, |a| {
                if let Some(r) = reply.take() {
                    a.sess.worker.input(r);
                }
            });
        }
        if let Some(w) = &self.window {
            w.focus_window();
        }
        self.redraw();
    }

    /// 문장 실행 버튼 = 단일 커서일 때만(전체 실행은 늘 활성 · 사용자 09-17). 값이 바뀔 때만 툴바에 쓴다.
    pub(crate) fn sync_run_stmt_button(&mut self) {
        // ■ = 막힌 상태를 푸는 유일한 버튼 — 실행 중이거나 보조 요청(전체 조회 등)이 진행 중일 때 켠다.
        let gate = self.gate();
        let stop = gate.stop;
        if stop != self.run_stop_enabled {
            self.run_stop_enabled = stop;
            let mut inv = Invalidations::default();
            self.tool_dock.set_item_enabled("run.stop", stop, &mut inv);
            self.redraw();
        }
        let on = gate.run_statement;
        if on != self.run_stmt_enabled {
            self.run_stmt_enabled = on;
            let mut inv = Invalidations::default();
            self.tool_dock
                .set_item_enabled("run.statement", on, &mut inv);
            self.tool_dock.set_item_tip(
                "run.statement",
                t(if on || !self.editors.cur().has_multi() {
                    Msg::TipRunStatement
                } else {
                    Msg::TipRunStatementMulti
                }),
            );
            self.redraw();
        }
    }
}
