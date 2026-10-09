//! App — 백그라운드 사건 소화(워커·접속·가져오기·실행 카드).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    pub(crate) fn handle_panel_action(&mut self, a: PanelAction) {
        match a {
            PanelAction::Connect(spec) => {
                // ★ 접속 = [저장] 선수행(사용자 09-30 "접속은 되었지만 미저장 상태로 남는 버그"): 폼이 저장본과 다르면 저장 버튼과
                //   같은 길(필수 항목 · 이름 규칙 · 이름 변경 · 비밀번호 저장 여부)을 먼저 지나고, 저장이 실패하면 접속하지 않는다.
                //   저장할 이름이 없는 폼(필수 미충족)은 저장을 건너뛰고 종전처럼 접속만 한다.
                if self.conn_win.panel.is_dirty() {
                    match self.conn_win.panel.save_action() {
                        Some(save) => {
                            self.handle_panel_action(save);
                            if matches!(self.conn_win.panel.state(), ConnState::Failed(_)) {
                                // 저장 실패(파일 쓰기 등) = 접속하지 않고 안내(사용자 09-30 "저장을 먼저 하고 접속하라는 경고").
                                self.sess.status = t(Msg::ConnSaveFirst).into();
                                self.toasts.push(
                                    toast::ToastKind::Warn,
                                    t(Msg::ConnSaveFirstTitle).to_string(),
                                    t(Msg::ConnSaveFirst).to_string(),
                                );
                                return;
                            }
                        }
                        None => {
                            // 저장 조건 미충족(필수 항목 · 이름 규칙) = 폼에 이미 표시됨 + 안내.
                            self.sess.status = t(Msg::ConnSaveFirst).into();
                            self.conn_win
                                .panel
                                .set_state(ConnState::Failed(t(Msg::ConnSaveFirst).into()));
                            return;
                        }
                    }
                }
                let name = self.conn_win.panel.profile_name();
                // 테스트 중인 프로필은 끝날 때까지 접속도 막는다(사용자 09-14).
                if self.conn_win.is_testing(&name) {
                    self.sess.status = t(Msg::StTesting).into();
                    self.conn_win.panel.set_state(ConnState::Testing);
                    return;
                }
                if let Some(pw) = spec.password.as_deref() {
                    self.conn_win.remember_pw(&name, pw);
                }
                self.conn_win
                    .set_connect_mark(&name, Some(ConnectMark::Connecting));
                self.panel_op = Some((name.clone(), ConnState::Connecting));
                // ★ 접속 중 막(사용자 09-19): 창 전체를 덮고 대상·단계·경과를 보인다 — 큐 대기면 그 단계부터.
                let phase = if self.attempts_inflight >= self.attempts_max {
                    Msg::VeilQueued
                } else {
                    Msg::VeilConnecting
                };
                self.conn_win
                    .veil_begin(&name, &spec.connection_string(), t(phase));
                // 같은 서버면 기존 세션 유지 · 설정 `connect.reconnect_same`이면 닫고 다시 접속(사용자 09-14).
                let reconnect_same = self.settings.flag("connect.reconnect_same");
                // 동시 상한·큐를 거친다(초과분은 앞의 시도가 끝나면 시작).
                self.attempt_queue.push_back(Attempt::Connect {
                    name,
                    spec,
                    reconnect_same,
                });
                self.dispatch_attempts();
            }
            PanelAction::Test(spec) => {
                let name = self.conn_win.panel.profile_name();
                if let Some(pw) = spec.password.as_deref() {
                    self.conn_win.remember_pw(&name, pw);
                }
                self.start_test(&name, spec);
            }
            PanelAction::Disconnect => {
                self.sess.disc_path = Some(sessions::DiscPath::ConnWin);
                self.disconnect_now();
            }
            PanelAction::Save {
                name,
                spec,
                rename_from,
            } => {
                // 저장하지 않더라도 입력된 비밀번호는 세션에 보관(사용자 09-14).
                let typed = self.conn_win.panel.password_text();
                self.conn_win.remember_pw(&name, &typed);
                // ★ 저장은 파일 쓰기뿐 — 워커(순차 · Test/Connect 뒤에 줄 섬)를 거치지 않고 즉시(사용자 09-14
                //   "Save에서 접속 테스트를 하지 않도록": 실제로는 앞선 Test의 20초 타임아웃을 기다리던 것). 접속 검증 없음 · 포트가 틀려도 저장.
                // 이름 변경(사용자 09-16): 새 이름으로 저장이 성공한 뒤에만 옛 항목을 지운다(실패 시 옛 프로필 보존).
                let res = Vault::open_default().and_then(|v| {
                    v.save(&name, &spec)?;
                    if let Some(old) = &rename_from {
                        v.remove(old)?;
                    }
                    Ok(())
                });
                match res {
                    Ok(()) => {
                        self.sess.status = tf(Msg::WkProfileSaved, &[&name, ""]);
                        // 저장본 = 지금 값(바뀜 표시 전부 꺼짐 · T-131).
                        self.conn_win.panel.mark_saved();
                        // 이름이 바뀌었으면 옛 이름의 결과는 버린다(저장 내용이 달라졌을 수 있으니 옮기지 않는다).
                        if let Some(old) = &rename_from {
                            self.panel_results.remove(old.trim());
                        }
                        // 폼은 이제 새 이름의 프로필을 "불러온" 상태 — 또 바꿔 저장하면 다시 이름 변경.
                        self.conn_win.panel.fill(&name, &spec);
                        self.conn_win.refresh_profiles(Some(&name));
                        // 지킴이가 미뤄 둔 동작(다른 프로필 불러오기 · New · 닫기)을 이어간다.
                        for a in self.conn_win.after_save(true) {
                            self.handle_conn_win_action(a);
                        }
                    }
                    Err(e) => {
                        let e = e.to_string();
                        self.sess.status = tf(Msg::WkProfileSaveFailed, &[&e]);
                        self.conn_win.panel.set_state(ConnState::Failed(e));
                        self.conn_win.after_save(false);
                    }
                }
            }
            PanelAction::Edit(_) => {}     // 접속 창이 자체 처리(클립보드)
            PanelAction::CopyFile(_) => {} // 접속 창이 자체 처리(`copy_profile_file` → CopyText)
            PanelAction::LoadProfile(name) => {
                match Vault::open_default().and_then(|v| v.get(&name)) {
                    Ok(Some(spec)) => {
                        let spec = self.conn_win.with_session_pw(&name, spec);
                        self.conn_win.panel.fill(&name, &spec);
                        // 상태는 한 번에 하나(사용자 09-14): 진행/결과가 이 프로필 것이면 복원, 아니면 프로필별 마지막 결과(09-16).
                        let st = self.panel_state_for(&name);
                        self.conn_win.panel.set_state(st);
                    }
                    Ok(None) => {}
                    Err(e) => self
                        .conn_win
                        .panel
                        .set_state(ConnState::Failed(e.to_string())),
                }
            }
        }
        self.redraw();
    }

    /// 진행 중/마지막 패널 작업의 프로필 이름(없으면 지금 패널의 이름).
    pub(crate) fn panel_op_name(&self) -> String {
        self.panel_op
            .as_ref()
            .map_or_else(|| self.conn_win.panel.profile_name(), |(n, _)| n.clone())
    }

    /// 패널 작업 결과를 기록하고, 지금 패널에 그 프로필이 떠 있을 때만 상태줄에 보인다(한 번에 상태 하나 · 사용자 09-14).
    pub(crate) fn set_panel_result(&mut self, name: &str, st: ConnState) {
        if self.conn_win.panel.profile_name().trim() == name.trim() {
            self.conn_win.panel.set_state(st.clone());
        }
        // 상세 패널이 닫혀 있어도 목록 오른쪽 아래에 같은 안내(사용자 09-16).
        self.conn_win.set_note(name, st.clone());
        // 완료 결과는 프로필별로도 남긴다(나중에 Details로 열 때 복원 · 사용자 09-16).
        if !name.trim().is_empty()
            && !matches!(
                st,
                ConnState::Idle | ConnState::Testing | ConnState::Connecting
            )
        {
            self.panel_results
                .insert(name.trim().to_string(), st.clone());
        }
        self.panel_op = Some((name.to_string(), st));
    }

    /// 프로필을 폼에 불러올 때 보일 상태 — 진행 중/직전 작업이 이 프로필 것이면 그것, 아니면 프로필별 마지막 결과, 없으면 Idle.
    fn panel_state_for(&self, name: &str) -> ConnState {
        // ★ 진행 중은 **행의 표식이 원천**(사용자 09-19: 테스트 도중 Details를 펼치면 가끔 "Testing"이 안 보였다 — `panel_op`가 다른
        //   프로필의 결과·프로브에 덮여 있었다). 행 Test/Connect와 폼 Test/Connect는 같은 표식을 본다.
        if self.conn_win.is_testing(name) {
            return ConnState::Testing;
        }
        if self.conn_win.connect_mark(name) == Some(ConnectMark::Connecting) {
            return ConnState::Connecting;
        }
        resolve_panel_state(self.panel_op.as_ref(), &self.panel_results, name)
    }

    fn drain_conn(&mut self) -> bool {
        let mut changed = false;
        // 접속 테스트 결과(스레드별) — 이름이 함께 오므로 여러 테스트가 섞여도 각자 자리에.
        while let Ok(r) = self.tests_rx.try_recv() {
            changed = true;
            self.attempt_done();
            let name = r.name;
            match r.outcome {
                Ok((description, elapsed_s)) => {
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        tf(Msg::LogTestOk, &[&description, &elapsed_s.to_string()]),
                    ));
                    // 상태줄·패널은 프로필 이름으로 간략하게(사용자 09-16) · 접속 문자열 상세는 위 로그 창에.
                    let label = if name.is_empty() { &description } else { &name };
                    let msg = tf(Msg::StTestOk, &[label, &elapsed_s]);
                    self.sess.status = msg.clone();
                    self.conn_win.set_test_mark(&name, TestMark::Ok);
                    self.set_panel_result(&name, ConnState::TestOk(msg));
                }
                Err(e) => {
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("test: {e}")));
                    self.sess.status = tf(Msg::StTestFailed, &[&e]);
                    self.conn_win.set_test_mark(&name, TestMark::Failed);
                    self.set_panel_result(&name, ConnState::Failed(e));
                    self.conn_win.note_failure(&name);
                }
            }
        }
        while let Ok(o) = self.sess.worker.conn.try_recv() {
            changed = true;
            match o {
                ConnOutcome::Connected(d) if !std::mem::take(&mut self.sess.attempt_inflight) => {
                    // 접속 창을 거치지 않은 접속(개별 모드 자동 접속 · 유휴 뒤 재접속 · [다시 연결]) — 시도 큐·접속 창 표시는 그대로.
                    self.sess.key_cache.clear();
                    self.sess.connected = true;
                    self.sess.desc = d;
                    self.sess.broken = false;
                    if let Some(ep) = app::health::ep_text(self.sess.spec.as_ref()) {
                        if self.health.state(&ep) != app::health::HealthState::Alive {
                            self.health_event(&ep, app::health::HealthEvent::Up);
                        }
                    }
                    self.sync_sess_ui();
                }
                ConnOutcome::ConnectFailed(e)
                    if !std::mem::take(&mut self.sess.attempt_inflight) =>
                {
                    self.sess.connected = false;
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("connect: {e}")));
                    self.sess.status = tf(Msg::StConnectFailed, &[&e]);
                }
                ConnOutcome::Connected(d) => {
                    self.attempt_done();
                    self.sess.key_cache.clear();
                    self.sess.connected = true;
                    self.sess.broken = false;
                    self.sess.desc = d.clone();
                    // 접속 창으로 붙은 세션 = 탐색기·접속 창 표시가 따라가는 세션 · 개별 모드의 새 탭이 쓸 기본 접속 정보.
                    //   공유 모드면 이 연결이 **활성 공유 연결**이 된다(묶이지 않은 탭이 따른다 · 기존 연결은 그대로 유지).
                    let relinked = self.primary_sess != self.sess.id;
                    self.primary_sess = self.sess.id;
                    if !self.sess.is_private() {
                        self.default_shared = self.sess.id;
                        // ★ 접속 창으로 붙인 연결은 **지금 탭**에 바로(사용자 09-28) — 다른 탭은 묶인 대로(모든 탭은 만들어질 때 묶인다).
                        let sid = self.sess.id;
                        self.bind_active_tab_to(sid);
                    }
                    self.default_spec = self.sess.last_spec.clone();
                    self.sess.spec = self.sess.last_spec.clone();
                    // 이미 붙어 있던 연결을 다시 고른 경우(워커가 세션을 유지 = Connected 이벤트 없음) 탐색기를 이쪽으로 돌린다.
                    // ★ 또한 `RunEvent::Connected`가 이 결과보다 **먼저** 처리되면(두 채널의 경주 · 시작 인자
                    //   접속에서 재현 · 사용자 09-19 "sqlite는 접속이 안 되었다" = 탐색기에 Demo 루트가 없음) 그때는 `spec`이
                    //   비어 있어 탐색기를 못 붙였다 → 지금 spec이 채워졌으니 이 서버의 탐색기가 없으면 여기서 붙인다.
                    let missing = !self.explorer.has_server(self.sess.spec.as_ref());
                    if relinked || missing {
                        self.explorer_attach();
                    }
                    let name = self.panel_op_name();
                    self.conn_win.mark_connected(&name);
                    // 접속 버튼 초록 = 지금 접속된 프로필 하나만 → 잠시 보여 준 뒤 창 닫힘(사용자 09-14).
                    self.conn_win.clear_connect_marks();
                    self.conn_win
                        .set_connect_mark(&name, Some(ConnectMark::Connected));
                    self.conn_win.veil_phase(&name, t(Msg::VeilConnected));
                    self.conn_win.close_after_connect();
                    // 활성 탭에 접속 정보 적용 — 탭이 없으면 새 탭(사용자 09-14).
                    self.editors.ensure_tab();
                    self.editors.set_conn_desc(d.clone());
                    self.set_panel_result(&name, ConnState::Connected(d));
                }
                ConnOutcome::ConnectFailed(e) => {
                    self.attempt_done();
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, format!("connect: {e}")));
                    self.sess.status = tf(Msg::StConnectFailed, &[&e]);
                    let name = self.panel_op_name();
                    self.conn_win.set_connect_mark(&name, None);
                    self.conn_win.veil_end(&name);
                    self.set_panel_result(&name, ConnState::Failed(e));
                    // 접속 실패 확인 → 그 서버 신호등 즉시 갱신(사용자 09-14).
                    self.conn_win.note_failure(&name);
                }
                // 접속 문자열에 비밀번호 자리가 없다 → 한 번 묻는다(입력 창은 `about_to_wait`에서 · 워커는 답을 기다린다).
                ConnOutcome::PasswordNeeded { target, rejected } => {
                    self.sess.status = t(Msg::StPasswordPrompt).into();
                    if rejected {
                        // 들고 있던 비밀번호가 거부돼 폐기했다 — 로그에 한 줄(값은 없다 · 대상은 가린 표시).
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::PasswordHintRejected, &[&target]),
                        ));
                    }
                    self.pw_pending = Some((self.sess.id, target, rejected));
                }
                ConnOutcome::SessionId(sid) => {
                    self.sess.live_sid = Some(sid);
                }
                ConnOutcome::Keys(table, info) => {
                    self.sess.aux_done();
                    self.sess.key_cache.insert(table, info.clone());
                    if let Some(tab) = self.sess.edit_wait.take() {
                        if let Some(g) = self.grid_for(tab) {
                            g.set_keys(info.as_ref());
                        }
                        self.redraw();
                    }
                    if let Some(kind) = self.sess.sql_wait.take() {
                        self.finish_sql_copy(kind, info.as_ref());
                    }
                    if let Some(kind) = self.sess.view_wait.take() {
                        self.finish_view_sql(kind, info.as_ref());
                    }
                }
                ConnOutcome::ImportProgress { key, rows, secs } => {
                    if key == self.import_key {
                        self.import_win.set_progress(rows, secs);
                    }
                }
                ConnOutcome::ImportDone { key, result } => {
                    self.sess.aux_done();
                    if key == self.import_key {
                        self.import_cancel = None;
                        self.import_done(result);
                    }
                }
                ConnOutcome::Applied {
                    key,
                    rep,
                    refetched,
                } => self.on_conn_applied(key, rep, refetched),
                ConnOutcome::Requery { key, offset, limit } => {
                    // 커서가 없어 서버에 새 SQL(OFFSET 재질의/재실행)이 간다 — 직접 실행처럼 카드(이미 켜져 있으면 로그만).
                    let line = tf(Msg::StRequery, &[&offset.to_string(), &limit.to_string()]);
                    self.log_win
                        .push(LogEntry::new(LogKind::Info, line.clone()));
                    if !self.sess.fetch_card.is_some_and(|(k, _)| k == key) {
                        let sql = self
                            .grid_for(key)
                            .map(|g| g.source_sql().to_string())
                            .unwrap_or_default();
                        self.fetch_card_start(key, Msg::CardRequery, &sql);
                    }
                    self.sess.status = line;
                    self.redraw();
                }
                // ★ 증분 표시(사용자 10-09): 배치를 바로 이어 붙인다(진행·■ 상태는 유지 · 열 너비는 첫 세그먼트 기준 · 정렬·필터는 재투영).
                ConnOutcome::Batch {
                    key,
                    rs,
                    rows,
                    bytes,
                } => {
                    let n = rs.rows.len();
                    if let Some(g) = self.grid_for(key) {
                        g.append_live(rs);
                        g.set_fetch_progress(rows, bytes);
                    }
                    if n > 0 {
                        self.run_toast.progress(self.sess.run_card, rows, bytes);
                        self.sess.status = tf(
                            Msg::StFetchLive,
                            &[&rows.to_string(), &nsql_core::fmt_bytes(bytes)],
                        );
                    }
                    self.redraw();
                }
                ConnOutcome::FetchProgress { key, rows, bytes } => {
                    if let Some(g) = self.grid_for(key) {
                        g.set_fetch_progress(rows, bytes);
                    }
                    self.run_toast.progress(self.sess.run_card, rows, bytes);
                    dlog!(self, LogLayer::Fetch, LogLevel::Progress, {
                        LogEntry::new(
                            LogKind::Fetch,
                            tf(Msg::LogDetProgress, &[&nsql_core::fmt_bytes(bytes)]),
                        )
                        .rows(rows)
                    });
                    self.sess.status = tf(
                        Msg::StFetchingProgress,
                        &[&rows.to_string(), &nsql_core::fmt_bytes(bytes)],
                    );
                    self.redraw();
                }
                ConnOutcome::Page {
                    key,
                    offset,
                    all,
                    result,
                    stop,
                    replace,
                    via_cursor,
                } => self.on_conn_page(key, offset, all, result, stop, replace, via_cursor),
                ConnOutcome::Count { key, result } => {
                    self.sess.aux_done();
                    if let Some((k, t0)) = self.sess.fetch_card {
                        if k == key {
                            self.sess.fetch_card = None;
                            let phase = match &result {
                                Ok(n) => runtoast::Phase::Done {
                                    rows: Some(*n),
                                    secs: t0.elapsed().as_secs_f64(),
                                    stages: String::new(),
                                },
                                Err(e) => runtoast::Phase::Error(e.clone()),
                            };
                            self.run_toast.finish(self.sess.run_card, phase);
                        }
                    }
                    match result {
                        Ok(n) => {
                            if let Some(g) = self.grid_for(key) {
                                g.set_total(n);
                            }
                            self.sess.status = tf(Msg::StCountResult, &[&n.to_string()]);
                            self.log_win
                                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                        }
                        Err(e) => {
                            if let Some(g) = self.grid_for(key) {
                                g.fetch_failed();
                            }
                            self.sess.status = tf(Msg::StFetchFailed, &[&e]);
                        }
                    }
                    self.redraw();
                }
                ConnOutcome::Disconnected => self.on_conn_disconnected(),
                // ★ 끊김 확인(docs/53 §3): 세션은 남기고 상태만 — 표식·플러그·목록이 "끊김"을 보인다.
                ConnOutcome::Broken {
                    reason: m,
                    endpoint,
                } => {
                    // ★ 끝점 층(107 §9-2): SYN 실패는 그 서버의 모든 세션·탐색기에 — 레지스트리 한 자리로(토스트·투영·헤더).
                    if endpoint {
                        if let Some(ep) = app::health::ep_text(self.sess.spec.as_ref()) {
                            self.health_event(&ep, app::health::HealthEvent::Down(m.clone()));
                        }
                    }
                    if !self.sess.broken {
                        self.sess.broken = true;
                        let line = if m.is_empty() {
                            tf(Msg::StSessBroken, &[&self.sess.desc])
                        } else {
                            format!("{} - {m}", tf(Msg::StSessBroken, &[&self.sess.desc]))
                        };
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, line.clone()));
                        self.sess.status = line;
                        self.sync_sess_ui();
                    }
                }
                ConnOutcome::Alive => {
                    if self.sess.broken {
                        self.sess.broken = false;
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::StSessAlive, &[&self.sess.desc]),
                        ));
                        self.sync_sess_ui();
                    }
                    // 한 세션이 닿았으면 끝점도 산 것(107 §9-2) — 끊겼던 끝점이면 복귀 전파 · 의심(L0)이었으면 조용히 해소.
                    if let Some(ep) = app::health::ep_text(self.sess.spec.as_ref()) {
                        if self.health.state(&ep) != app::health::HealthState::Alive {
                            self.health_event(&ep, app::health::HealthEvent::Up);
                        }
                    }
                }
            }
        }
        changed
    }

    /// `Cmd::Apply` 결과(그리드 편집 적용 · 87 §12·§14): 행 단위 재조회/제자리 교체 · 상태·로그·토스트 · 트랜잭션 로그 한 줄씩 ·
    /// 수동 모드 표식 · 필요하면 전체 재조회 — `drain_conn`에서 분리(T-248 · 09-29 · 행동 보존).
    fn on_conn_applied(
        &mut self,
        key: u64,
        rep: nsql_run::ApplyReport,
        refetched: Vec<Result<nsql_core::ResultSet, String>>,
    ) {
        self.sess.aux_done();
        self.sess.edit_apply = None;
        let mode = self
            .settings
            .get("grid.edit_refresh")
            .unwrap_or("rows")
            .to_string();
        // ★ 행 단위 재조회(87 §12-4 · T-230): 성공 + 결과가 행마다 1행이면 제자리 교체 → 전체 재조회 없음.
        let mut patched = false;
        if rep.error.is_none() && rep.done > 0 {
            if let Some(g) = self.grid_for(key) {
                patched = match mode.as_str() {
                    "rows" => g.apply_done_rows(refetched),
                    "local" => g.apply_done_local(),
                    _ => false,
                };
            }
        }
        let requery = mode != "local" && !patched;
        // ★ 87 §14 데이터 보호 불변식: 실패 원인을 단계·문장·키·행 수까지 상세히 + 되돌림 상태(자동 롤백 / 세이브포인트 / ROLLBACK 필요).
        let msg = match &rep.error {
            None if rep.tx_left_open => tf(Msg::StGeAppliedTx, &[&rep.done.to_string()]),
            None => tf(Msg::StGeApplied, &[&rep.done.to_string()]),
            Some(e) => {
                let n = (e.index + 1).to_string();
                let mut s = match e.phase {
                    nsql_run::ApplyPhase::PreCheck => {
                        tf(Msg::StGePreCheck, &[&n, &e.label, &e.message])
                    }
                    _ => tf(Msg::StGeExecFail, &[&n, &e.label, &e.message]),
                };
                s.push(' ');
                if rep.rollback_needed {
                    s.push_str(t(Msg::StGeRollbackNeeded));
                } else if rep.rolled_back && rep.tx_left_open {
                    s.push_str(t(Msg::StGeSavepointBack));
                } else if rep.rolled_back {
                    s.push_str(t(Msg::StGeAutoRolledBack));
                } else if e.phase == nsql_run::ApplyPhase::PreCheck {
                    // 사전 검사 단계 = 아직 아무것도 쓰지 않았다(문구에 포함).
                }
                s
            }
        };
        self.log_win.push(LogEntry::new(
            if rep.error.is_some() {
                LogKind::Error
            } else {
                LogKind::Info
            },
            msg.clone(),
        ));
        if rep.error.is_some() {
            self.toasts
                .push(toast::ToastKind::Error, t(Msg::MnGeApply), msg.clone());
        }
        self.sess.status = msg;
        // ★ 트랜잭션 로그(44 · 사용자 09-26 "적용해도 로그 창에 안 보임 · 3번이 1줄"): 실행한 문장마다 한 줄 —
        //   사전 검사 = Util(전체 보기에서만) · 실행문 = User · 수동 모드에서 실제로 남은 것만 열린 트랜잭션에 붙인다(pending 수).
        {
            let attach = rep.tx_left_open && (rep.error.is_none() || rep.rollback_needed);
            let editor = self.sess.run_editor;
            let log = self.txlog.select_session(self.sess.id);
            log.begin_batch();
            for (i, it) in rep.log.iter().enumerate() {
                let stamp = nsql_log::now_local().stamp();
                let purpose = if it.guard {
                    TxPurpose::Util
                } else {
                    TxPurpose::User
                };
                log.begin(stamp.clone(), editor, purpose, i, &it.sql);
                log.set_binds(i, it.binds.clone());
                match &it.error {
                    Some(m) => log.error(i, None, m),
                    None => {
                        log.done(i, it.rows, it.elapsed);
                        if attach && !it.guard {
                            log.attach_tx(i, stamp, true);
                        }
                    }
                }
            }
            self.txlog_win.redraw();
        }
        if patched {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                t(Msg::StGeRowsPatched).to_string(),
            ));
        } else if let Some(g) = self.grid_for(key) {
            g.apply_done(
                rep.done,
                rep.error.as_ref().map(|e| (e.index, e.message.clone())),
            );
        }
        if rep.tx_left_open {
            // 수동 모드: 커밋/롤백 표식(34 UX) — 트랜잭션 로그의 한 줄로.
            let stamp = nsql_log::now_local().stamp();
            self.sess.tx_pending.push(TxItem {
                editor: self.sess.run_editor,
                at: Instant::now(),
                when: stamp.get(11..16).unwrap_or("").to_string(),
                summary: t(Msg::MnGeApply).to_string(),
                class: nsql_core::TxClass::Update,
            });
            self.sess.tx_dirty = true;
            self.sess.tx_stale_logged = false;
            self.sync_tx_ui();
        }
        if rep.error.is_none() && rep.done > 0 && requery && key == self.grid_tab {
            self.refresh_result();
        }
        self.redraw();
    }

    /// `Cmd::FetchPage` 결과(docs/43): `offset`부터 이어 붙임(0 = 교체) · 전체 조회의 멈춤 사유 · 커서/OFFSET 구분 —
    /// `drain_conn`에서 분리(T-248 · 09-29 · 행동 보존).
    #[allow(clippy::too_many_arguments)] // `ConnOutcome::Page`의 페이로드 그대로(묶음 구조체는 워커 ABI 변경 = T-246 몫).
    fn on_conn_page(
        &mut self,
        key: u64,
        offset: usize,
        all: bool,
        result: Result<(nsql_core::ResultSet, bool, Duration), String>,
        stop: Option<crate::worker::FetchStop>,
        replace: bool,
        via_cursor: bool,
    ) {
        self.sess.aux_done();
        // 실행 Facade(docs/43 §11): 이 페치의 카드가 켜져 있으면 결과로 끝낸다(실패 = Error).
        let card = self.sess.fetch_card.is_some_and(|(k, _)| k == key);
        if card {
            if let Err(e) = &result {
                self.run_toast
                    .finish(self.sess.run_card, runtoast::Phase::Error(e.clone()));
                self.sess.fetch_card = None;
            }
        }
        match result {
            Ok((rs, more, elapsed)) => {
                let n = rs.rows.len().to_string();
                let secs = format!("{:.3}", elapsed.as_secs_f64());
                if replace {
                    // 엄격 일관성(docs/43 §9): 처음부터 다시 받아 교체했다 — 로그 1줄.
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        tf(Msg::StRefetchReplaced, &[&n]),
                    ));
                }
                let dial = self.sess.dialect; // 이 페치를 돌려준 세션의 방언(결과의 속성 · 09-26)
                let total = match self.grid_for(key) {
                    Some(g) => {
                        if replace {
                            g.replace_rows(rs, more);
                        } else if all {
                            // 전체 조회 = 나머지 이어 붙이기(위치·정렬·텍스트 스크롤 유지 · 09-17).
                            g.append_all(rs, more);
                        } else if offset == 0 {
                            g.set_result(rs);
                            g.set_dialect(dial);
                            g.set_more(more);
                        } else {
                            g.append_page(rs, more);
                        }
                        g.row_count()
                    }
                    // (전체 조회가 예산에서 잘렸으면 아래에서 안내)
                    None => 0,
                };
                self.sess.status = if !sessions::fetch_card_policy(all, false, via_cursor) {
                    tf(Msg::StFetchedCursor, &[&n, &secs, &total.to_string()])
                } else {
                    tf(Msg::StFetched, &[&n, &secs, &total.to_string()])
                };
                if !sessions::fetch_card_policy(all, false, via_cursor) {
                    // 커서 이어 읽기 = 카드 없이 로그 한 줄(서버에 새 SQL 없음).
                    self.log_win
                        .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                }
                if all || card {
                    self.sess.fetch_card = None;
                    let phase = match stop {
                        Some(worker::FetchStop::Cancelled) => {
                            runtoast::Phase::Stopped { rows: total as u64 }
                        }
                        _ => runtoast::Phase::Done {
                            rows: Some(total as u64),
                            secs: elapsed.as_secs_f64(),
                            stages: String::new(),
                        },
                    };
                    self.run_toast.finish(self.sess.run_card, phase);
                    dlog!(self, LogLayer::Fetch, LogLevel::Timing, {
                        let b = self.grid_for(key).map_or(0, |g| g.approx_bytes());
                        LogEntry::new(
                            LogKind::Fetch,
                            tf(
                                Msg::LogDetFetchAll,
                                &[&nsql_core::fmt_bytes(b), &speed_of(b, elapsed)],
                            ),
                        )
                        .rows(total as u64)
                        .elapsed(elapsed)
                    });
                }
                match stop {
                    Some(worker::FetchStop::CursorGone) => {
                        // 커서 결과의 커서가 닫혔다(T-202) — 그리드는 빈 페이지로 `more=false`가 됐다 · 안내 한 줄.
                        self.sess.status = t(Msg::StCursorGone).into();
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                    }
                    Some(worker::FetchStop::Budget) => {
                        // 전체 조회가 메모리 예산(D-72)에서 멈췄다.
                        self.sess.status = tf(
                            Msg::StBudgetExceeded,
                            &[&self.settings.int("grid.memory_budget_mb").to_string()],
                        );
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                    }
                    Some(worker::FetchStop::Cancelled) => {
                        self.sess.status = tf(Msg::StFetchCancelled, &[&total.to_string()]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                    }
                    None => {}
                }
            }
            Err(e) => {
                if let Some(g) = self.grid_for(key) {
                    g.fetch_failed();
                }
                self.sess.status = tf(Msg::StFetchFailed, &[&e]);
                self.log_win
                    .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
            }
        }
        self.redraw();
    }

    /// 실행 계열 진입점의 공통 문지기 — 막혔으면 상태줄에 알리고 false.
    /// ★ Import 창 "시작"(89 §3-3): 탐색기 칸의 서버 = 활성 세션의 서버여야 한다 · gate · `Cmd::Import`(aux · 끝 = `ImportDone`).
    pub(crate) fn import_start(&mut self, mut spec: nsql_run::bulk::ImportSpec) {
        let Some((table, path, server)) = self.import_ctx.clone() else {
            return;
        };
        if self.import_win.is_running() {
            return;
        }
        if let Some(sv) = &server {
            self.sync_sess();
            // ★ 끊긴 끝점(docs/107 §4 · T-313 ④): 시작 전에 막는다 — 적재 중 끊김은 배치 경계 중단(89)이지만 시작은 애초에 안 한다.
            if let Some(ep) = app::health::ep_text(Some(sv)) {
                if self.health.is_broken(&ep) {
                    let msg = tf(Msg::StEpBrokenBlocked, &[&ep]);
                    self.sess.status = msg.clone();
                    self.toasts.push(
                        toast::ToastKind::Warn,
                        t(Msg::ExpHealthDownTitle).to_string(),
                        msg,
                    );
                    self.redraw();
                    return;
                }
            }
            let same = self
                .sess
                .spec
                .as_ref()
                .is_some_and(|have| worker::same_server(have, sv));
            if !same {
                let msg = tf(
                    Msg::StImportOtherServer,
                    &[sv.host.as_deref().unwrap_or_default()],
                );
                self.sess.status = msg.clone();
                self.import_win.set_result(msg, true, "other-server".into());
                return;
            }
        }
        let Some(pass) = self.gate_pass() else {
            self.import_win
                .set_result(t(Msg::StRunning).to_string(), true, "blocked".into());
            return;
        };
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        spec.cancel = Some(cancel.clone());
        spec.table = table.clone();
        self.import_cancel = Some(cancel);
        self.import_key += 1;
        self.sess.aux += 1;
        self.sess.status = tf(Msg::StImportStarted, &[&path.to_string_lossy(), &table]);
        self.log_win
            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
        self.import_win.set_running();
        self.sess.submit(
            pass,
            worker::Cmd::Import {
                key: self.import_key,
                path,
                spec,
            },
        );
        self.redraw();
    }

    /// Import 취소 = 깃발만(워커가 다음 배치 경계에서 멈추고 `ImportDone`으로 답한다).
    pub(crate) fn import_cancel(&mut self) {
        if let Some(c) = &self.import_cancel {
            c.store(true, std::sync::atomic::Ordering::Relaxed);
            self.sess.status = t(Msg::ImpBtnCancel).into();
        }
    }

    /// `ImportDone` → 창 결과 줄 · 상태줄 · 로그(실패 = 행·줄 지목 · 취소 = 커밋 행).
    fn import_done(&mut self, result: Result<nsql_run::bulk::BulkReport, String>) {
        let (text, bad, report) = match result {
            Err(m) => (m.clone(), true, format!("err {m}")),
            Ok(rep) => {
                let rows = rep.rows.to_string();
                match &rep.failure {
                    None => {
                        let secs = rep.elapsed.as_secs_f64();
                        let rate = rep.rows as f64 / secs.max(1e-6);
                        (
                            tf(
                                Msg::ImpDone,
                                &[
                                    &rows,
                                    &format!("{secs:.2}"),
                                    &format!("{rate:.0}"),
                                    &rep.path,
                                    &rep.batches.to_string(),
                                ],
                            ),
                            false,
                            format!(
                                "ok rows={} path={} batches={}",
                                rep.rows, rep.path, rep.batches
                            ),
                        )
                    }
                    Some(f) if f.message == nsql_run::bulk::CANCELLED => (
                        tf(Msg::ImpCancelled, &[&rows]),
                        true,
                        format!("cancelled rows={}", rep.rows),
                    ),
                    Some(f) => (
                        tf(
                            Msg::ImpFailed,
                            &[&f.row.to_string(), &f.line.to_string(), &f.message, &rows],
                        ),
                        true,
                        format!(
                            "failed row={} line={} rows={} msg={}",
                            f.row, f.line, rep.rows, f.message
                        ),
                    ),
                }
            }
        };
        self.sess.status = text.clone();
        self.log_win.push(LogEntry::new(
            if bad { LogKind::Error } else { LogKind::Info },
            text.clone(),
        ));
        self.import_win.set_result(text, bad, report);
        self.redraw();
    }

    /// 실행 시작 → 카드(문장 · 시작 시각 · 문장 수).
    pub(crate) fn run_toast_start(&mut self, src: &str) {
        // 이 세션의 문장 실행 = 활동(docs/56 L2) — 유휴 시계·경고·카운트다운을 되돌린다.
        self.tx_guard_reset();
        self.sess.run_cancel_requested = false;
        self.sess.run_cancel_at = None;
        self.sync_run_stmt_button();
        self.editors.set_running(self.sess.run_editor, true);
        self.editors.set_error_line(self.sess.run_editor, None);
        let n = split_items(src, self.sess.dialect).len().max(1);
        self.sess.run_card = self.run_toast.start(src, nsql_log::now_local().stamp(), n);
    }

    pub(crate) fn drain_events(&mut self) {
        let mut changed = self.drain_conn();
        self.conn_win.drain_probes();
        while let Ok(ev) = self.sess.events.try_recv() {
            changed = true;
            // 다시 접속하려다 기존 접속을 잃었다(`ConnectionClosed`) — 해제는 같지만 **세션은 거두지 않는다**(아래).
            let lost_by_connect = matches!(ev, RunEvent::ConnectionClosed);
            if matches!(ev, RunEvent::Disconnected | RunEvent::ConnectionClosed) {
                // 호스트가 시작한 해제는 이미 경로와 함께 남겼다(disc_path) · 스크립트/서버 쪽 해제만 여기서 남긴다.
                if self.sess.disc_path.is_none() {
                    self.log_disconnect(if lost_by_connect {
                        sessions::DiscPath::Reconnect
                    } else {
                        sessions::DiscPath::Script
                    });
                }
            } else {
                for e in nsql_run::log_entries(&ev) {
                    if let Some(h) = &self.log_hub {
                        h.push(e.clone());
                    }
                    self.log_win.push(e);
                }
            }
            // ★ Output 탭(09-30): 사람이 읽는 메시지(PRINT · 서버 출력 · 컴파일 · 경고 · 오류 · 문장 완료)는 실행한 탭의 Output으로.
            self.output_on_event(&ev);
            match ev {
                RunEvent::Begin { index, server, .. } => {
                    if index == 0 {
                        self.txlog.select_session(self.sess.id).begin_batch();
                    }
                    self.run_toast.set_phase(
                        self.sess.run_card,
                        runtoast::Phase::Running {
                            index,
                            total: self.sess.last_run_items.len(),
                        },
                    );
                    let stmt = self
                        .sess
                        .last_run_items
                        .get(index)
                        .cloned()
                        .unwrap_or_default();
                    // 세션 상태를 바꾸는 문장이 서버로 나가면 이 세션은 유휴로 닫지 않고, 재접속 때 한 번 알린다(docs/52 §6-4 · D-103).
                    // 표식은 문장이 실제로 도는 세션에 붙는다(`CONNECT` 뒤 문장 = 새 세션). 켜지는 순간 원인 문장을 로그에 남긴다.
                    if server && !self.sess.stateful && sessions::alters_session_state(&stmt) {
                        self.sess.stateful = true;
                        let head: String =
                            stmt.lines().next().unwrap_or("").chars().take(80).collect();
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::StSessStateful, &[head.trim()]),
                        ));
                    }
                    // 전송 로그는 **서버로 가는 항목에만**(클라이언트 명령은 네트워크 0 · mac 09-21 `PRINT`/`VARIABLE`에 찍히던 것).
                    if server {
                        dlog!(self, LogLayer::Net, LogLevel::Timing, {
                            let now = nsql_log::now_local().stamp();
                            LogEntry::new(
                                LogKind::Send,
                                tf(Msg::LogDetSent, &[&now, &stmt.len().to_string()]),
                            )
                        });
                    }
                    let purpose = if stmt
                        .trim_start()
                        .to_ascii_uppercase()
                        .starts_with("SET AUTOCOMMIT")
                    {
                        TxPurpose::Util
                    } else {
                        TxPurpose::User
                    };
                    self.txlog.select_session(self.sess.id).begin(
                        nsql_log::now_local().stamp(),
                        self.sess.run_editor,
                        purpose,
                        index,
                        &stmt,
                    );
                    self.txlog_win.redraw();
                }
                RunEvent::ResultSet {
                    index,
                    rs,
                    elapsed,
                    more,
                    label,
                } => {
                    self.txlog.select_session(self.sess.id).result(
                        index,
                        rs.rows.len() as u64,
                        elapsed,
                    );
                    self.run_toast.first_page(
                        self.sess.run_card,
                        rs.rows.len() as u64,
                        rs.approx_bytes(),
                        elapsed,
                    );
                    dlog!(self, LogLayer::Net, LogLevel::Timing, {
                        let b = rs.approx_bytes();
                        LogEntry::new(
                            LogKind::Fetch,
                            tf(
                                Msg::LogDetFirstSeg,
                                &[&nsql_core::fmt_bytes(b), &speed_of(b, elapsed)],
                            ),
                        )
                        .rows(rs.rows.len() as u64)
                        .elapsed(elapsed)
                    });
                    self.run_toast.set_phase(
                        self.sess.run_card,
                        runtoast::Phase::Done {
                            rows: Some(rs.rows.len() as u64),
                            secs: elapsed.as_secs_f64(),
                            stages: String::new(),
                        },
                    );
                    self.sess.last_rows = Some(rs.rows.len());
                    self.sess.last_secs = Some(elapsed.as_secs_f64());
                    let n = rs.rows.len().to_string();
                    let secs = format!("{:.3}", elapsed.as_secs_f64());
                    // 페치 상한에서 잘렸으면 "더 있음"을 알린다(DBeaver식 · 사용자 09-15).
                    self.sess.status = if more {
                        tf(Msg::StRowsMore, &[&n, &secs])
                    } else {
                        tf(Msg::StRows, &[&n, &secs])
                    };
                    // 결과는 실행을 시작한 결과 탭의 그리드로(탭이 이미 닫혔으면 버림) · 탭 제목 갱신(T-93).
                    // 이 결과를 만든 문장 하나를 그리드에 알린다 — 건수(Σ)·OFFSET 재질의·새로고침·SQL 복사가 스크립트 전체가
                    // 아니라 **그 문장**을 쓴다 · 조회 문장이 아니면(EXEC/PRINT의 REF CURSOR 등) 건수 불가(09-19 규정).
                    let stmt = self.sess.last_run_items.get(index).cloned();
                    let is_query = stmt.as_deref().is_some_and(|s| {
                        nsql_script::split_script(s).first().is_some_and(|it| {
                            matches!(
                                it.kind,
                                nsql_script::ItemKind::Sql(nsql_script::SqlKind::Query)
                            )
                        })
                    });
                    // ★ 같은 문장이 결과를 둘 이상 냈으면(REF CURSOR 여러 개 · 암묵 결과 · 다중 결과 집합) 두 번째부터는
                    //   딸린 결과 탭으로 — 종전에는 같은 그리드를 덮어써 마지막 것만 남았다(09-21).
                    let same_stmt = self.sess.run_set_stmt == Some(index);
                    let slot = sessions::extra_result_slot(
                        self.sess.run_set_stmt,
                        index,
                        self.sess.run_children,
                        self.settings.flag("grid.result_per_statement"),
                    );
                    self.sess.run_set_stmt = Some(index);
                    let k = match slot {
                        Some(ord) => {
                            self.sess.run_children = ord + 1;
                            self.child_result_tab(self.sess.run_tab, ord)
                        }
                        None => self.sess.run_tab,
                    };
                    // 이름 있는 결과(커서 변수)나 같은 문장의 추가 결과는 조회 문장 하나로 다시 만들 수 없다 → 건수·재질의 불가.
                    let is_query = is_query && !(same_stmt && slot.is_some()) && label.is_none();
                    // ★ 방언은 **그 결과를 만든 세션의 것**으로 같이 싣는다(사용자 09-26 Linux — 종전에는 접속 성공 시점에만
                    //   일괄로 넣어, 뒤이어 다른 방언으로 `CONNECT`하면 옛 결과 그리드의 방언까지 덮어써졌다 → Oracle 결과에
                    //   SQL Server 문법(`[열]`)으로 UPDATE를 만들어 ORA-00936. 결과의 방언은 결과의 속성이다 · DR-33).
                    let dial = self.sess.dialect;
                    // ★ 증분 표시(사용자 10-09): 탭의 행 수가 0(전체)이고 더 있으면 첫 세그먼트를 보인 채 **나머지를 자동으로 이어 받는다**
                    //   (⇊와 같은 길 · 배치마다 `Batch` · ■/진행률/예산은 배치 경계) — `whole`이면 종전대로(드라이버가 전부 읽은 뒤 표시).
                    let live = self.settings.get("grid.fetch_display") != Some("whole");
                    if let Some(g) = self.grid_for(k) {
                        g.set_result(rs);
                        g.set_dialect(dial);
                        g.set_more(more);
                        if let Some(s) = stmt.as_deref() {
                            g.set_result_origin(s, is_query);
                        }
                        if live && more && g.page_rows() == 0 && is_query {
                            g.request_fetch_all();
                        }
                    }
                    // 외래 키 열 표시(T-180 ⑥) — 활성 그리드 기준(다른 탭의 결과면 탭 전환 때 drain이 다시 맞춘다).
                    self.grid_fk_sync();
                    // ★ Output 탭을 보던 중이라도 결과 셋이 오면 **그 결과 탭으로**(사용자 10-01 ㉘ · `output.activate = always`만 예외 ·
                    //   포커스는 그대로 — 편집 중인 캐럿을 뺏지 않는다). 다른 편집기의 패널이면 그 패널의 활성만 바꾼다.
                    let policy = self
                        .settings
                        .get("output.activate")
                        .unwrap_or("no_results")
                        .to_string();
                    let ed = self.sess.run_editor;
                    if ed == self.panel_editor {
                        if crate::results::switch_to_result(self.panel.output_active(), &policy) {
                            if let Some(i) = self.panel.index_of(k) {
                                let f = self.focus;
                                self.activate_result(i);
                                self.set_focus(f);
                            }
                        }
                    } else if let Some(p) = self.panels.get_mut(&ed) {
                        if crate::results::switch_to_result(p.output_active(), &policy) {
                            if let Some(i) = p.index_of(k) {
                                p.active = i;
                                p.sync_bar();
                            }
                        }
                    }
                    // ★ 편집 준비(키 조회·열 명세)는 결과가 온 직후에(그리드 사건을 기다리지 않는다 · docs/87).
                    if k == self.grid_tab {
                        self.after_grid_event();
                    }
                    self.tx_on_read(index);
                    self.retitle_result(k);
                    if let Some(name) = label {
                        self.title_result_as(k, &name);
                    } else if let (Some(ord), true) = (slot, same_stmt) {
                        // 같은 문장의 이름 없는 추가 결과(암묵 결과 · 다중 결과 집합) = "제목 (n)" · 다른 문장의 결과는 제 SQL에서 제목을 얻었다.
                        let base = self.result_title(self.sess.run_tab);
                        self.title_result_as(k, &format!("{base} ({})", ord + 2));
                    }
                }
                RunEvent::Done {
                    index,
                    rows_affected,
                    elapsed,
                } => {
                    let secs = format!("{:.3}", elapsed.as_secs_f64());
                    self.sess.status = match rows_affected {
                        Some(n) => tf(Msg::StRowsAffected, &[&n.to_string(), &secs]),
                        None => tf(Msg::StOk, &[&secs]),
                    };
                    let stmt = self
                        .sess
                        .last_run_items
                        .get(index)
                        .cloned()
                        .unwrap_or_default();
                    self.txlog
                        .select_session(self.sess.id)
                        .done(index, rows_affected, elapsed);
                    self.run_toast.set_phase(
                        self.sess.run_card,
                        runtoast::Phase::Done {
                            rows: rows_affected,
                            secs: elapsed.as_secs_f64(),
                            stages: String::new(),
                        },
                    );
                    self.tx_on_done(index, &stmt, rows_affected);
                    self.meta_on_done(&stmt);
                }
                // PRINT · VARIABLE 목록 · 서버 메시지는 로그 창으로만(`log_entries`) — 결과 영역은 조회 결과만(사용자 09-17).
                RunEvent::Print { .. } | RunEvent::VarList { .. } => {}
                // 실행 전에 값이 필요하다(D-137) — 입력 창은 이벤트 루프에서 연다(`about_to_wait`). 워커는 답을 기다린다.
                RunEvent::InputNeeded { needs } => {
                    self.sess.status = t(Msg::WinInputs).into();
                    self.input_pending = Some((self.sess.id, needs));
                }
                // ★ 변수 표가 바뀌었다(실행당 한 번 · D-135): 탭 층 = 실행한 탭의 표 · 공유 층 = 이 세션의 표 · 로그에 바뀐 값(비밀은 가림).
                RunEvent::Vars {
                    local,
                    shared,
                    global,
                    changed,
                    defines,
                } => {
                    self.tab_defines.insert(self.sess.run_editor, defines);
                    let line = vars_log_line(&changed, &local, &shared, &global);
                    // 스크립트가 글로벌 층을 바꿨으면(`VAR x GLOBAL` · 대입) 단일 원천 갱신 + 저장 + 다른 세션에 전파.
                    if global != self.global_vars {
                        self.global_vars = global;
                        self.global_vars_changed();
                    }
                    self.vars_changed = (
                        self.sess.run_editor,
                        changed.iter().map(|n| n.to_ascii_uppercase()).collect(),
                    );
                    self.vars_win.redraw();
                    self.vars_persist_save(self.sess.run_editor, &local);
                    self.tab_vars.insert(self.sess.run_editor, local);
                    self.sess.shared_vars = shared;
                    if !line.is_empty() {
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::LogVarsChanged, &[&line]),
                        ));
                    }
                }
                RunEvent::Binds { index, binds } => {
                    // 실행 직전의 바인드 매핑 → 그 문장 행(3계층 · docs/44 §9).
                    self.txlog
                        .select_session(self.sess.id)
                        .set_binds(index, binds);
                    self.txlog_win.redraw();
                }
                RunEvent::Warning(m) => {
                    // 눈에 띄게(T-202): 상태줄 + 로그(`log_entries`가 이미 넣었다).
                    self.sess.status = m;
                }
                // ★ 편집기 스크립트 명령(10-01): `CONNTYPE x` → 이 탭 세션의 유형 · `SHOW CONN` → 접속 정보 Output.
                RunEvent::ConnType(v) => {
                    let env = if v == "none" {
                        None
                    } else {
                        nsql_script::ConnEnv::from_name(&v)
                    };
                    self.set_session_env(env);
                }
                RunEvent::ShowConn => self.output_conn_info(),
                // ★ `USE db` 성공(101 §3): 세션의 현재 DB → 탐색기(현재 DB 표시 · 메타 추종) · 상태줄.
                RunEvent::DbChanged(db) => {
                    self.sess.current_db = Some(db.clone());
                    // ★ 탭이 기억(⑯): 이 탭에서 바꾼 값 = 돌아올 때 다시 맞춘다.
                    self.tab_unit
                        .insert((self.editors.active_id(), self.sess.id), db.clone());
                    // Oracle `ALTER SESSION SET CURRENT_SCHEMA` · PG `SET search_path` = 현재 스키마가 바뀐 것(⑮).
                    if matches!(
                        self.sess.dialect,
                        nsql_core::Dialect::Oracle | nsql_core::Dialect::Postgres
                    ) {
                        self.sess.cur_schema = Some(db.clone());
                    }
                    if let Some(spec) = self.sess.spec.clone() {
                        self.explorer.set_current_db(&spec, &db);
                    }
                    self.sess_ui_dirty = true;
                }
                RunEvent::Message(m) => {
                    if m == t(Msg::StCommitted) {
                        self.tx_close(TxOutcome::Committed);
                        self.sess.status = m.clone();
                    } else if m == t(Msg::StRolledBack) {
                        self.tx_close(TxOutcome::RolledBack);
                        self.sess.status = m.clone();
                    }
                }
                RunEvent::Connected {
                    description,
                    dialect,
                    schema,
                    database,
                } => {
                    // ★ 연결 기본 작업 단위(⑯): DB 전환 방언 = 접속 직후 DB · 그 밖 = 현재 스키마.
                    self.sess.default_unit = if matches!(
                        dialect,
                        nsql_core::Dialect::Mssql | nsql_core::Dialect::Mysql
                    ) {
                        Some(database.clone()).filter(|d| !d.is_empty())
                    } else {
                        Some(schema.clone()).filter(|s| !s.is_empty())
                    };
                    self.sess.current_db = None;
                    // 새 접속 = 이 세션에 대한 탭 기억은 전부 버린다(재접속 뒤 옛 스키마를 되밀지 않게 · ㉔).
                    let sid = self.sess.id;
                    self.tab_unit.retain(|(_, s), _| *s != sid);
                    self.sess.disc_path = None;
                    self.sess.status = tf(
                        Msg::StConnected,
                        &[
                            sessions::status_conn_desc(dialect.is_file_based(), &description),
                            &dialect.to_string(),
                        ],
                    );
                    self.startup_connected = true;
                    if self.sess.skip_done == 0 {
                        self.sess.busy = false;
                    }
                    self.sess.dialect = dialect;
                    self.sess.connected = true;
                    self.sess.cur_schema = Some(schema).filter(|s| !s.is_empty());
                    let was_broken = std::mem::take(&mut self.sess.broken);
                    // ★ 전용 탭 복귀 안내(107 §9-4 · D-270): 전용 세션은 거의 늘 상태가 있다(CONNECT 자체 · ALTER SESSION) → 끊겼다 붙으면 알린다.
                    if was_broken && self.sess.is_private() {
                        let m = t(Msg::StPrivateSessReset).to_string();
                        self.log_win.push(LogEntry::new(LogKind::Info, m.clone()));
                        self.toasts
                            .push(toast::ToastKind::Warn, description.clone(), m);
                    }
                    // 접속 성공 = 끝점이 살아 있다(107 §9-2) — 끊겼던 끝점이면 복귀 전파.
                    if let Some(ep) = app::health::ep_text(self.sess.spec.as_ref()) {
                        if self.health.is_broken(&ep) {
                            self.health_event(&ep, app::health::HealthEvent::Up);
                        }
                    }
                    self.sess.user_disconnected = false;
                    self.sess.idle_closed = false;
                    self.sess.desc = description.clone();
                    self.sess.key_cache.clear();
                    self.sess.touch();
                    self.txlog.set_session_label(self.sess.id, &description);
                    // 새 접속 = 서버의 세션 상태는 처음부터 — 앞 세션에서 세션 설정·임시 데이터를 만들었으면 한 번 알린다.
                    if std::mem::take(&mut self.sess.stateful) {
                        let m = t(Msg::StSessStateLost).to_string();
                        self.log_win.push(LogEntry::new(LogKind::Info, m.clone()));
                        // 안내지 오류가 아니다(사용자 09-30 "에러처럼 뜨는 토스트") — 경고 종류로.
                        self.toasts
                            .push(toast::ToastKind::Warn, description.clone(), m);
                    }
                    // 방언은 **이 세션의 결과 그리드**에만(Copy SQL 방언): 공유 세션 = 전용 탭을 뺀 전부 · 전용 세션 = 주인 탭의 것.
                    self.set_dialect_for_sess_grids(dialect);
                    self.tx_close(TxOutcome::Lost);
                    if self.sess.is_private() {
                        self.log_win.push(LogEntry::new(
                            LogKind::Connect,
                            tf(Msg::StSessPrivateOpened, &[&description]),
                        ));
                    }
                    if self.sess.attempt_inflight {
                        self.primary_sess = self.sess.id;
                    }
                    // ★ 탐색기 = 서버별(docs/52 §2-2): **어떤 세션이든** 붙은 서버의 탐색기를 확보한다(이미 있으면 그대로 · 메타가
                    //   인텔리센스·툴팁의 단일 원천이라 그 서버에 붙은 세션이 하나라도 있는 동안 유지된다). 활성 탭의 세션이면 앞으로.
                    self.explorer_attach();
                }
                RunEvent::Disconnected | RunEvent::ConnectionClosed => {
                    // 접속이 끊겼다: 결과는 기본으로 **보기용으로 남긴다**(09-18 결정) — `mem.release_results_on_disconnect`를
                    //   켜면 그 세션으로 받은 결과 그리드를 비워 메모리를 바로 돌려준다.
                    if self.settings.flag("mem.release_results_on_disconnect") {
                        self.grid.clear_result();
                        for g in self.sleeping_grids_mut() {
                            g.clear_result();
                        }
                    }
                    self.mem_released();
                    self.sess.status = t(Msg::StDisconnected).into();
                    self.sess.connected = false;
                    // (탐색기는 `sync_sess_ui`의 참조 수 맞춤이 처리한다 — 이 서버에 붙은 세션이 남아 있으면 유지.)
                    self.tx_close(TxOutcome::Lost);
                    // 공유 모드의 전용 세션이 스크립트 안의 DISCONNECT로 끊겼다 → 세션을 거두고 공유 세션으로 복귀(유휴 닫기는 제외).
                    // ★ 사용자가 배지 메뉴 "No connection"으로 **명시적으로** 끊은 세션(`user_disconnected`)은 거두지 않는다 —
                    //   공유로 되돌리면 사용자의 선택을 무시하는 것(09-19). 툴바 Disconnect(keep=false)는 종전대로 공유 복귀.
                    // ★ `CONNECT`가 실패해 접속을 잃은 경우(`lost_by_connect`)는 **거두지 않는다**(사용자 09-21 "첫 번째에는 오류
                    //   토스트가 안 뜬다"): 거두면 ① 이 세션의 실행 상태 카드·상태줄에 실릴 접속 오류가 세션과 함께 사라지고
                    //   ② 탭이 공유 연결(다른 서버일 수 있다)로 조용히 돌아간다. 탭은 "연결 없음"인 전용 세션으로 남는다 —
                    //   처음부터 접속에 실패한 전용 세션과 같은 모습이다.
                    if sessions::reap_on_disconnect(
                        self.sess.is_private(),
                        self.sess.idle_closed,
                        self.sess.user_disconnected,
                        self.session_mode() == SessionMode::Shared,
                        lost_by_connect,
                    ) {
                        self.sess.closing = true;
                    }
                }
                RunEvent::ReadTxEnded { index, deferred } => self.tx_on_read_ended(index, deferred),
                RunEvent::Timing { index, timeline } => {
                    // 상태줄 = 결과 요약 + 단계별 소요(docs/26). 렌더 시간은 그리드 푸터가 자체 표시.
                    self.txlog
                        .select_session(self.sess.id)
                        .timing(index, timeline.total());
                    self.run_toast.timing(
                        self.sess.run_card,
                        timeline.total().as_secs_f64(),
                        timeline.summary(),
                    );
                    for sp in &timeline.spans {
                        let (layer, msg) = match sp.stage {
                            nsql_core::Stage::Send => (LogLayer::Net, Msg::LogDetStageSend),
                            nsql_core::Stage::Execute => (LogLayer::Exec, Msg::LogDetStageExec),
                            nsql_core::Stage::Fetch | nsql_core::Stage::Receive => {
                                (LogLayer::Fetch, Msg::LogDetStageFetch)
                            }
                            nsql_core::Stage::Load => (LogLayer::Load, Msg::LogDetStageLoad),
                            _ => (LogLayer::App, Msg::LogDetStageOther),
                        };
                        if layer == LogLayer::App {
                            continue;
                        }
                        dlog!(self, layer, LogLevel::Timing, {
                            let b = sp.bytes.unwrap_or(0);
                            LogEntry::new(
                                LogKind::Info,
                                tf(
                                    msg,
                                    &[
                                        sp.stage.label(),
                                        &nsql_core::fmt_dur(sp.dur),
                                        &nsql_core::fmt_bytes(b),
                                        &speed_of(b, sp.dur),
                                        sp.note.as_deref().unwrap_or(""),
                                    ],
                                ),
                            )
                            .rows(sp.rows)
                            .elapsed(sp.dur)
                        });
                    }
                    self.sess.status = format!("{} · ⏱ {}", self.sess.status, timeline.summary());
                }
                RunEvent::Error { index, line, error } => {
                    // 주입 재조회가 실패했으면 그리드를 원문으로 되돌리고 주입 없이 다시 판정(87 §13 · WITHOUT ROWID 표 등).
                    // 🔧 09-28: **재조회가 나가 있을 때만** — 사용자의 다른 문장(예: 자동 커밋 모드의 COMMIT/ROLLBACK 오류)이
                    //   같은 탭에서 실패하면 성공해 둔 주입 계획을 "실패"로 지우고 버튼을 영영 껐다.
                    if let Some(g) = self.run_grid() {
                        if g.inject_pending() {
                            g.requery_failed();
                        }
                    }
                    self.txlog.select_session(self.sess.id).error(
                        index,
                        error.code,
                        &error.message,
                    );
                    self.txlog_win.redraw();
                    if std::mem::take(&mut self.sess.run_cancel_requested) {
                        // 사용자가 ■를 눌러 드라이버가 끊은 실행 — 오류 토스트 대신 "중지됨"(T-108).
                        if std::mem::take(&mut self.sess.run_cancel_drops) {
                            // 소켓을 끊은 취소(SQL Server): 열린 트랜잭션은 서버가 롤백 · 다음 실행 때 자동 재접속.
                            self.tx_close(TxOutcome::Lost);
                            self.sess.status = t(Msg::StRunCancelledDrop).into();
                            self.log_win
                                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                        } else {
                            self.sess.status = t(Msg::StRunCancelled).into();
                        }
                        self.run_toast
                            .set_phase(self.sess.run_card, runtoast::Phase::Stopped { rows: 0 });
                        if let Some(g) = self.run_grid() {
                            g.clear_result();
                        }
                        self.sess.busy = false;
                        self.sync_run_stmt_button();
                        self.editors.set_running(self.sess.run_editor, false);
                        self.redraw();
                        continue;
                    }
                    // 공통 분류 + 코드 부각(docs/42): 상태줄 · 결과 메시지 · 로그 창 · 토스트(분류된 오류만).
                    let stmt = self
                        .sess
                        .last_run_items
                        .get(index)
                        .cloned()
                        .unwrap_or_default();
                    let err_dialect = sessions::error_dialect(
                        self.sess.connected,
                        self.sess.dialect,
                        self.sess.last_spec.as_ref().and_then(|s| s.dialect),
                    );
                    let (cls, summary) =
                        toast::summarize(err_dialect, error.code, &error.message, &stmt);
                    self.sess.status = tf(Msg::StErrorLine, &[&line.to_string(), &summary]);
                    // "테이블/뷰 없음"인데 탐색기에는 그 이름이 있다 = 트리가 낡았다 → 그 폴더만 다시(docs/57 T4).
                    if cls.class == nsql_core::ErrorClass::NoTable
                        && self.settings.flag("meta.refresh_on_missing")
                    {
                        let name = explorer::missing_name(&error.message);
                        let schema = self.meta_default_schema();
                        self.explorer.note_missing(
                            self.sess.spec.as_ref(),
                            name.as_deref(),
                            schema.as_deref(),
                        );
                    }
                    if self.settings.flag("editor.minimap_errors") && line > 0 {
                        let ed_line = self.sess.run_line_base + line - 1;
                        self.editors
                            .set_error_line(self.sess.run_editor, Some(ed_line));
                    }
                    self.run_toast
                        .set_phase(self.sess.run_card, runtoast::Phase::Error(summary.clone()));
                    // 오류가 나도 결과 영역은 기본 형태(빈 그리드)로 — 본문은 로그 창·상태줄·토스트(사용자 09-17).
                    if let Some(g) = self.run_grid() {
                        // 조건 바 실행(T-181 후속)의 오류면 결과를 비우지 않고 조건 상자 테두리를 빨갛게(토스트·카드와 함께 ·
                        //   사용자 10-06 "오류가 나면 조건 바에 경고 표시").
                        if g.cond_run_failed() {
                            g.cond_server_error(&summary);
                        } else {
                            g.clear_result();
                        }
                    }
                    // ★ 같은 오류를 두 번 보이지 않는다(사용자 09-19 "오류가 왜 2번 출력되나"): 실행 상태 카드(`run.toast`)가
                    //   이미 분류된 오류 요약을 빨간 카드로 보여 주므로, 카드가 켜져 있으면 오류 토스트는 띄우지 않는다
                    //   (카드를 끈 사용자에게만 토스트 · 로그 창에는 종전대로 한 줄).
                    let card_shows = self.settings.flag("run.toast");
                    if let Some(label) = toast::class_label(cls.class).filter(|_| !card_shows) {
                        let title = match &cls.code {
                            Some(c) => format!("{c} · {label}"),
                            None => label.to_string(),
                        };
                        let body = cls.object.clone().unwrap_or_else(|| {
                            error.message.lines().next().unwrap_or("").to_string()
                        });
                        self.toasts.push(toast::ToastKind::Error, title, body);
                        self.redraw();
                    }
                    self.sess.busy = false;
                    // 실행 중 접속성 오류 확인 → 활성 서버 신호등 즉시 갱신(사용자 09-14).
                    if probe::is_connection_error(error.code, &error.message) {
                        if self.sess.id == self.primary_sess {
                            let name = self.conn_win.active_name().to_string();
                            self.conn_win.note_failure(&name);
                        }
                        if !self.sess.broken {
                            self.sess.broken = true;
                            self.sync_sess_ui();
                        }
                    }
                }
            }
        }
        while let Ok(done) = self.sess.worker.done.try_recv() {
            changed = true;
            self.sess.touch();
            if self.sess.skip_done > 0 {
                // 실행 앞에 끼운 재접속의 완료 — 실패했으면 알리기만 하고 뒤따르는 작업의 완료를 기다린다.
                self.sess.skip_done -= 1;
                if let Some(m) = done {
                    self.sess.status = m;
                }
                continue;
            }
            self.sess.busy = false;
            self.sync_run_stmt_button();
            self.editors.set_running(self.sess.run_editor, false);
            // 실행이 끝났다 = 이번 실행에서 쓰이지 않은 딸린 결과 탭(앞선 실행의 커서 탭)을 걷는다.
            if std::mem::take(&mut self.sess.run_tracking) {
                self.prune_child_results(self.sess.run_tab, self.sess.run_children);
            }
            // 실행이 끝났다 = 이번 실행의 DDL을 폴더별로 한 번만 탐색기에 반영(스크립트 디바운스 · docs/57 T1).
            let ddl = std::mem::take(&mut self.sess.ddl_now);
            self.meta_flush(ddl);
            // 실행이 끝났다 = 앞선 결과(그리드·텍스트 보기·페치 버퍼)를 놓았다 → 1초 뒤 힙을 한 번 정리.
            self.mem_released();
            let failed = done.is_some();
            // 새 결과 탭으로 시작한 실행이 **결과 하나 없이 오류**로 끝났으면 그 탭을 거두고 앞 탭으로(09-28).
            if let Some(prev) = self.sess.run_fresh_prev.take() {
                // ★ 결과 셋 없이 끝난 실행(컴파일 · DDL · PRINT)도 Output 탭이 있으면 빈 새 결과 탭을 거둔다(10-01 · T-266).
                let only_output = !self.sess.run_had_rs
                    && self.sess.run_set_stmt.is_none()
                    && self.panel.output_index().is_some();
                if (failed && self.sess.run_set_stmt.is_none()) || only_output {
                    let fresh = Some(self.sess.run_tab);
                    self.drop_fresh_result_tab(fresh, prev);
                }
            }
            // 뒤에서 끝난 실행(D-104): 그 탭 제목 앞에 ✓/✗ — 탭을 보면 지워진다.
            if self.editors.active_id() != self.sess.run_editor {
                self.editors
                    .set_done_mark(self.sess.run_editor, Some(!failed));
            }
            if std::mem::take(&mut self.sess.run_cancel_requested) && !failed {
                // Attention 취소(SQL Server · 세션 유지): 오류 없이 부분 결과로 끝난다 → "중지됨".
                self.sess.run_cancel_drops = false;
                // ★ 실행 중지 = 받은 행은 **보기만 유지**(사용자 09-17 결정): 이번 실행 스트림의 앞부분이라 보는 용도로는 정확하지만
                //   OFFSET 재실행은 정렬이 없으면 순서가 달라질 수 있어 이어 받기(⇊)·자동 페치는 막는다(`more=false`) · 전체는 재실행.
                //   (⇊ 나머지 이어 받기의 중지는 T-48b대로 받은 행 + 더 있음 유지 — 늘 연속된 앞부분 · 사용자 "이전 세그먼트 방식 유지".)
                let rows = self.sess.last_rows.unwrap_or(0) as u64;
                self.sess.status = tf(Msg::StRunCancelledPartial, &[&rows.to_string()]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                self.run_toast
                    .finish(self.sess.run_card, runtoast::Phase::Stopped { rows });
                if let Some(g) = self.run_grid() {
                    g.set_more(false);
                }
            } else {
                self.run_toast.finish_keep(self.sess.run_card, done.clone());
            }
            if let Some(m) = done {
                self.sess.status = m;
            }
            // 현재 문장 실행 뒤 캐럿(설정 `run.after_statement` · 사용자 09-16): stay / next_ok / next_always.
            if std::mem::take(&mut self.sess.single_run)
                && self.editors.active_id() == self.sess.run_editor
            {
                let mode = self.settings.get("run.after_statement").unwrap_or("stay");
                if mode == "next_always" || (mode == "next_ok" && !failed) {
                    self.goto_statement(true);
                }
            }
        }
        if self.explorer.drain() {
            changed = true;
            // "불러오는 중"인 완성 팝업은 메타가 도착한 세대에 다시 조립한다(09-23).
            if self.intel.is_loading() {
                self.intel_request(false);
            }
            // 테이블 제약이 도착했으면 결과 그리드의 외래 키 열도 맞춘다(T-180 ⑥).
            self.grid_fk_sync();
        }
        if self.live_drain() {
            changed = true;
        }
        if self.tx_block_drain() {
            changed = true;
        }
        if self.explorer_actions() {
            changed = true;
        }
        if changed {
            self.redraw();
        }
    }
}
