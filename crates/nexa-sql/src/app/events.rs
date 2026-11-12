//! App — 백그라운드 사건 소화(워커·접속·가져오기·실행 카드).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    pub(crate) fn handle_panel_action(&mut self, a: PanelAction) {
        match a {
            PanelAction::QuickConnect(spec) => {
                // ★ 간략 접속(사용자 10-22): 이름 자동(저장소에 없는 이름) → 폼 채움 → **비밀번호 없이 저장**(문자열의 비밀번호는 세션 보관만
                //   = Save 처리의 `remember_pw`) → 폼을 저장본으로 → 접속 버튼과 같은 길(비밀번호가 없으면 필수 칸 경고로 직접 입력).
                let names: Vec<String> = Vault::open_default()
                    .and_then(|v| v.list())
                    .map(|l| l.into_iter().map(|p| p.name).collect())
                    .unwrap_or_default();
                let name =
                    crate::connect::quick_profile_name(&spec, |n| names.iter().any(|e| e == n));
                let typed = spec.password.clone().unwrap_or_default();
                let mut saved = spec;
                saved.password = None;
                self.conn_win.panel.fill(&name, &saved, Vec::new());
                self.conn_win.panel.set_password_text(&typed);
                self.handle_panel_action(PanelAction::Save {
                    name: name.clone(),
                    spec: saved,
                    rename_from: None,
                    vars: Vec::new(),
                });
                if matches!(self.conn_win.panel.state(), ConnState::Failed(_)) {
                    return;
                }
                // Save 뒤 `fill`이 비밀번호 칸을 비웠다 → 입력값 복원 · 저장본으로 삼아 접속이 다시 저장하지 않게.
                self.conn_win.panel.set_password_text(&typed);
                self.conn_win.panel.mark_saved();
                if let Some(a) = self.conn_win.panel.connect_action() {
                    self.handle_panel_action(a);
                }
            }
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
                vars,
            } => {
                // 저장하지 않더라도 입력된 비밀번호는 세션에 보관(사용자 09-14).
                let typed = self.conn_win.panel.password_text();
                self.conn_win.remember_pw(&name, &typed);
                // ★ 저장은 파일 쓰기뿐 — 워커(순차 · Test/Connect 뒤에 줄 섬)를 거치지 않고 즉시(사용자 09-14
                //   "Save에서 접속 테스트를 하지 않도록": 실제로는 앞선 Test의 20초 타임아웃을 기다리던 것). 접속 검증 없음 · 포트가 틀려도 저장.
                // 이름 변경(사용자 09-16): 새 이름으로 저장이 성공한 뒤에만 옛 항목을 지운다(실패 시 옛 프로필 보존).
                let res = Vault::open_default().and_then(|v| {
                    v.save(&name, &spec)?;
                    // ★ 프로필 변수(T-153 후속 10-27) = 스펙 다음에 같은 파일의 `var.` 줄(스펙 저장이 보존한 옛 줄을 통째로 교체).
                    v.set_vars(&name, &vars)?;
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
                        self.conn_win.panel.fill(&name, &spec, vars);
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
            // ★ [프로필 변수 (n)…] = 창은 다음 `about_to_wait`/사건 끝에서 연다(이벤트 루프 밖 · 다른 보조 창과 같은 깃발 펌프).
            PanelAction::EditVars => {
                self.open_profvars = true;
            }
            PanelAction::Edit(_) => {}     // 접속 창이 자체 처리(클립보드)
            PanelAction::CopyFile(_) => {} // 접속 창이 자체 처리(`copy_profile_file` → CopyText)
            PanelAction::LoadProfile(name) => {
                match Vault::open_default().and_then(|v| v.get(&name)) {
                    Ok(Some(spec)) => {
                        let spec = self.conn_win.with_session_pw(&name, spec);
                        let vars = crate::connect::profile_vars(&name);
                        self.conn_win.panel.fill(&name, &spec, vars);
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
            // 진행 틱(드라이버 설치 단계) = 상태줄·접속 창 베일 글만 바꾸고 테스트는 계속(사용자 10-23).
            if let Some(text) = r.stage {
                self.sess.status = text.clone();
                self.conn_win.veil_phase(&r.name, &text);
                continue;
            }
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
                    // 131차: 레지스트리가 멀쩡해도 늘 흘린다(탐색기 헤더 "끊김"만 남은 경우를 `health_event`가 걷는다).
                    if let Some(ep) = app::health::ep_text(self.sess.spec.as_ref()) {
                        self.health_event(&ep, app::health::HealthEvent::Up);
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
                    // ★ 프로필 층 변수(T-153): 프로필로 붙었으면 `var.*`를 읽어 러너와 변수 창에 준다(인라인 접속 = 비움).
                    self.load_profile_vars();
                    let name = self.panel_op_name();
                    self.conn_win.mark_connected(&name);
                    // 접속 버튼 초록 = 접속 중인 프로필 전부(★ T-121 (d) 10-30 · 종전 09-14 = 방금 것 하나) → 잠시 보여 준 뒤 창 닫힘.
                    self.conn_win_sync_live();
                    self.conn_win
                        .set_connect_mark(&name, Some(ConnectMark::Connected));
                    // ★ T-121 (e): 프로젝트 복원 때 이 프로필을 기다리던 탭들을 묶는다.
                    self.apply_pending_binds(&name);
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
                ConnOutcome::DriverInstall {
                    id,
                    stage,
                    done,
                    total,
                } => {
                    let pct = match total {
                        Some(t) if t > 0 => format!("{}%", (done * 100 / t).min(100)),
                        _ => nsql_core::fmt_bytes(done),
                    };
                    let text = tf(
                        Msg::StDriverInstall,
                        &[&id, &nsql_i18n::driver_stage_text(&stage), &pct],
                    );
                    self.sess.status = text.clone();
                    let name = self.panel_op_name();
                    self.conn_win.veil_phase(&name, &text);
                    if stage == "installed" {
                        self.drivers_info_changed();
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            format!("driver installed: {id}"),
                        ));
                    }
                }
                ConnOutcome::SessionId(sid) => {
                    self.sess.live_sid = Some(sid);
                }
                ConnOutcome::Keys(table, info) => {
                    self.sess.aux_done();
                    // ★ Import 창이 기다리는 기존 표의 키(D-455 · PK/UQ 표시 · 표본 중복 경고).
                    if self
                        .import_ctx
                        .as_ref()
                        .is_some_and(|c| c.existing.as_deref() == Some(table.as_str()))
                    {
                        self.import_win.set_table_keys(info.as_ref());
                    }
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
                // ★ T-8 GUI 2차: 내보내기 진행·끝.
                ConnOutcome::ExportProgress { key, rows, secs } => {
                    if key == self.export_key {
                        self.sess.status = tf(
                            Msg::StExportProgress,
                            &[&rows.to_string(), &format!("{secs:.1}")],
                        );
                        self.redraw();
                    }
                }
                ConnOutcome::ExportDone { key, path, result } => {
                    self.sess.aux_done();
                    if key == self.export_key {
                        self.export_cancel = None;
                        self.result_export_done(&path, result);
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
                // ★ 두 층(107 §9-2 · ⑥): 끝점 층(SYN 실패) = 레지스트리 한 자리 · 세션 층 = 이 세션만 + 세션 토스트 — `app/health.rs conn_broken`.
                ConnOutcome::Broken {
                    reason: m,
                    endpoint,
                } => self.conn_broken(m, endpoint),
                ConnOutcome::Alive => {
                    if self.sess.broken {
                        self.sess.broken = false;
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            tf(Msg::StSessAlive, &[&self.sess.desc]),
                        ));
                        self.sync_sess_ui();
                    }
                    // 한 세션이 닿았으면 끝점도 산 것(107 §9-2) — 끊겼던 끝점이면 복귀 전파 · 의심(L0)이었으면 조용히 해소 ·
                    // 131차: 레지스트리가 멀쩡해도 흘린다(헤더 "끊김"만 남은 경우).
                    if let Some(ep) = app::health::ep_text(self.sess.spec.as_ref()) {
                        self.health_event(&ep, app::health::HealthEvent::Up);
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
                        log.done(i, it.rows, it.elapsed, "");
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
        // ★ 재질의 확인(10-12 사용자 결정 ④): 워커가 엄격 교체 직전에 멈췄다 — 행은 그대로 · 토스트 [계속]으로만 다시 보낸다
        //   (`grid.refetch:<탭>:<offset>:<limit>` → `refetch_confirmed`). 자동 페치는 같은 행 수에서 다시 묻지 않는다(`auto_fetch_at`).
        if matches!(stop, Some(worker::FetchStop::AskRefetch)) {
            let (rows, page) = match self.grid_for(key) {
                Some(g) => {
                    g.fetch_failed();
                    (g.row_count(), g.page_rows())
                }
                None => (0, 0),
            };
            let limit = if all { 0 } else { page };
            if card || all {
                self.sess.fetch_card = None;
                self.run_toast.finish(
                    self.sess.run_card,
                    runtoast::Phase::Stopped { rows: rows as u64 },
                );
            }
            let body = tf(Msg::ToastRefetchBody, &[&rows.to_string()]);
            self.toasts.push_button(
                crate::toast::ToastKind::Warn,
                t(Msg::ToastRefetchTitle),
                body.clone(),
                t(Msg::BtnContinue),
                &format!("grid.refetch:{key}:{offset}:{limit}"),
            );
            self.sess.status = t(Msg::StRefetchAsked).into();
            self.log_win.push(LogEntry::new(LogKind::Info, body));
            self.redraw();
            return;
        }
        match result {
            Ok((rs, more, elapsed)) => {
                let n = rs.rows.len().to_string();
                let secs = format!("{:.3}", elapsed.as_secs_f64());
                // ★ 실행당 1회 경고의 단위(D-463): 첫 페이지(= 새 실행 결과)가 오면 이 탭의 경고 기록을 지운다.
                if !all && !replace && offset == 0 {
                    self.refetch_warned.remove(&key);
                }
                if replace {
                    // ★ D-463(사용자 11-06): 묻지 않고 교체한 뒤 **경고 토스트**(버튼 없음) · 결과 탭의 실행당 1회 · 설정 `grid.refetch_warn`.
                    if self.settings.flag("grid.refetch_warn") && self.refetch_warned.insert(key) {
                        self.toasts.push(
                            crate::toast::ToastKind::Warn,
                            t(Msg::ToastRefetchWarnTitle),
                            tf(Msg::ToastRefetchWarnBody, &[&n]),
                        );
                    }
                    // 엄격 일관성(docs/43 §9): 처음부터 다시 받아 교체했다 — 로그 1줄.
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        tf(Msg::StRefetchReplaced, &[&n]),
                    ));
                }
                // ★ T-433(10-27): ORDER BY 없는 OFFSET 재질의 안내는 **실제로 그 길을 탔을 때만**(커서 이어 받기·교체·전체 조회·첫 페이지 제외) ·
                //   탭마다 1회 — 종전엔 `send_fetch`가 커서 유무를 모른 채 미리 찍어 커서 경로에도 한 줄 나왔다(140차 mac V1 관찰).
                if !all
                    && !replace
                    && !via_cursor
                    && offset > 0
                    && self.settings.flag("grid.offset_warn")
                    && self
                        .grid_for(key)
                        .is_some_and(|g| !nsql_io::paging::has_order_by(g.source_sql()))
                    && self.offset_warned.insert(key)
                {
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        t(Msg::StOffsetWarn).to_string(),
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
                    // 재질의 확인은 위에서 이미 돌려보냈다(여기 오지 않는다).
                    Some(worker::FetchStop::AskRefetch) => {}
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
        let Some(ctx) = self.import_ctx.clone() else {
            return;
        };
        let (path, server) = (ctx.path.clone(), ctx.server);
        let table = spec.table.clone();
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
        if let Some(c) = self.import_ctx.as_mut() {
            c.created = spec.create.is_some();
        }
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
                            {
                                let mut t = tf(
                                    Msg::ImpDone,
                                    &[
                                        &rows,
                                        &format!("{secs:.2}"),
                                        &format!("{rate:.0}"),
                                        &rep.path,
                                        &rep.batches.to_string(),
                                    ],
                                );
                                // ★ D-452 Skip: 건너뛴 행 수를 덧붙인다.
                                if rep.skipped_total > 0 {
                                    t.push_str(" · ");
                                    t.push_str(&tf(
                                        Msg::ImpSkippedN,
                                        &[&rep.skipped_total.to_string()],
                                    ));
                                }
                                t
                            },
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
        // ★ 새 표를 만들었으면(127 §2-2) 그 스키마의 탐색기 메타를 새로 고친다.
        if !bad {
            if let Some(c) = self.import_ctx.clone() {
                if c.created {
                    let _ =
                        self.explorer
                            .refresh_meta(c.server.as_ref(), c.schema.as_deref(), false);
                }
            }
        }
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
        // AI 도구의 숨김 실행은 실행 카드를 띄우지 않는다(협업 V1 관찰 11-12: 과정 쿼리마다 "완료 1행" 카드가 떴다).
        self.sess.run_card = if self.ai.pending.as_ref().is_some_and(|p| p.silent) {
            None
        } else {
            self.run_toast.start(src, nsql_log::now_local().stamp(), n)
        };
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
                RunEvent::Begin {
                    index,
                    server,
                    ref summary,
                    ..
                } => {
                    self.on_ev_begin(index, server, summary);
                }
                RunEvent::ResultSet {
                    index,
                    rs,
                    elapsed,
                    more,
                    label,
                } => {
                    self.on_ev_result_set(index, rs, elapsed, more, label);
                }
                RunEvent::Done {
                    index,
                    rows_affected,
                    elapsed,
                    delivery,
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
                    self.txlog.select_session(self.sess.id).done(
                        index,
                        rows_affected,
                        elapsed,
                        &delivery,
                    );
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
                    // ★ AI 도구의 숨김 실행(11-12 협업 V1): 값 입력 창을 띄우면 사용자는 영문을 모르고 도구는 답 없이 멈춘다 →
                    //   실행을 취소하고 **없는 변수 이름**을 오류로 돌려준다(어시스턴트가 set_var로 먼저 만들거나 사용자에게 묻는다).
                    if self.ai.pending.as_ref().is_some_and(|p| p.silent) {
                        let names: Vec<String> = needs.iter().map(|n| n.name.clone()).collect();
                        self.sess.worker.input(worker::InputReply::Cancel);
                        self.ai.impact_wait = None;
                        if let Some(mut p) = self.ai.pending.take() {
                            if let Some(r) = p.reply.take() {
                                let _ = r.send(crate::ai::ToolReply {
                                    output: format!(
                                        "not executed: these variables have no value yet: {} - set them first with set_var, or ask the user for the values",
                                        names.join(", ")
                                    ),
                                    ok: false,
                                });
                            }
                        }
                    } else {
                        self.sess.status = t(Msg::WinInputs).into();
                        self.input_pending = Some((self.sess.id, needs));
                    }
                }
                // ★ 변수 표가 바뀌었다(실행당 한 번 · D-135): 탭 층 = 실행한 탭의 표 · 공유 층 = 이 세션의 표 · 로그에 바뀐 값(비밀은 가림).
                RunEvent::Vars {
                    local,
                    shared,
                    global,
                    changed,
                    defines,
                    defines_shared,
                    defines_global,
                } => {
                    self.tab_defines.insert(self.sess.run_editor, defines);
                    self.sess.shared_macros = defines_shared;
                    // 스크립트가 치환 변수 글로벌 층을 바꿨으면(`DEFINE x GLOBAL`) 원천 갱신 + 모든 세션 러너에 전파(T-153 후속 10-26).
                    if defines_global != self.global_macros {
                        self.global_macros = defines_global;
                        self.global_macros_changed();
                    }
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
                    // ★ 변수 표가 바뀌었으면 프로젝트 자동 저장 대상(사용자 11-12 "변수가 다음 실행 때 없어진다"): 무제 탭의 변수는
                    //   프로젝트 파일에 담기는데(T-153 ③), 변경 감시(`project_change_gen`)가 변수를 보지 않아 변수만 바꾸면 저장되지
                    //   않았다 — 앱이 정상 종료되지 않으면(강제 종료·재빌드 재시작) 사라졌다.
                    if self.tab_vars.get(&self.sess.run_editor) != Some(&local) {
                        self.project_touch();
                    }
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
                // ★ 드라이버 제약(docs/110 §6 · 사용자 10-13 "Warning 토스트") — 접속 직후 한 번 · 제약 기능이 요청됐을 때마다.
                RunEvent::DriverLimits {
                    driver,
                    limits,
                    at_connect,
                } => {
                    self.on_ev_driver_limits(driver, limits, at_connect);
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
                    autocommit,
                } => {
                    self.on_ev_connected(description, dialect, schema, database, autocommit);
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
                RunEvent::Held {
                    stmts,
                    total_rows,
                    at_commit,
                    remaining,
                } => self.tx_on_held(stmts, total_rows, at_commit, remaining),
                RunEvent::ReadTxEnded { index, deferred } => self.tx_on_read_ended(index, deferred),
                RunEvent::Timing { index, timeline } => {
                    self.on_ev_timing(index, timeline);
                }
                RunEvent::Error { index, line, error } => {
                    self.sess.run_had_err = true; // O-1 `server` 시점 판정(컴파일 오류 포함).
                    if self.on_ev_error(index, line, error) {
                        continue;
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
            // ★ 증분 표시(사용자 10-09): 첫 세그먼트가 "더 있음"으로 왔고 탭 행 수가 0(전체)이면 실행이 끝난 지금 나머지를 자동으로
            //   이어 받는다(⇊과 같은 길 · 배치마다 Batch · 실패한 실행은 아님).
            if done.is_none() {
                let tab = self.sess.run_tab;
                if let Some(g) = self.grid_for(tab) {
                    if g.take_auto_fetch_all() {
                        g.request_fetch_all_force();
                    }
                }
                // ★ O-1 `server` 시점(19 §3 (c) · 11-10): 오브젝트 탭의 객체 F5가 **오류 없이** 끝나면 서버가 실제로 가진 소스를
                //   다시 받아 히스토리에만 넣는다(탭 불변 · 응답은 meta.rs `OpenSql`에서 가로챈다). 오류가 있었으면 요청하지 않는다.
                if std::mem::take(&mut self.sess.run_object) && !self.sess.run_had_err {
                    self.history_server_request(self.sess.run_editor);
                }
            } else {
                self.sess.run_object = false;
            }
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
        // ★ 저장소만 바뀐 응답(L2 컬럼 워머·사전 · 133차 Linux 10-20): 트리는 그대로라 창 전체를 다시 그리지 않는다 — 종전엔 응답마다
        //   `drain() == true` → 재그리기 → `objlink_sync`가 새 스탬프를 보고 본문 링크 전체 재분석(협업 B = 워머 200응답 × ≈12 ms = UI
        //   스레드 CPU 30~60 ms/s가 접속 뒤 1분 넘게). 지금 메타를 보고 있는 소비자(완성 팝업 "불러오는 중" · 밑줄 링크 표시 중)가
        //   있을 때만 다시 그린다 · 나머지는 다음 입력 사건의 재그리기에서 자연히 반영된다.
        if self.explorer.take_meta_changed() {
            let links_shown =
                self.objlinks.active && self.objlink_display() != app::objlink::Display::None;
            if meta_redraw_wanted(self.intel.is_loading(), links_shown) {
                if self.intel.is_loading() {
                    self.intel_request(false);
                }
                changed = true;
            }
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

/// 저장소만 바뀐 메타 응답(L2 컬럼 워머·사전)에 창을 다시 그릴 조건(순수 · MC/DC): 완성 팝업이 메타를 기다리는 중이거나
/// 본문 링크가 지금 표시 중일 때만. 둘 다 아니면 다음 입력 사건의 재그리기에서 반영된다(133차 Linux 10-20).
pub(crate) fn meta_redraw_wanted(intel_loading: bool, links_shown: bool) -> bool {
    intel_loading || links_shown
}

#[cfg(test)]
mod meta_redraw_tests {
    use super::meta_redraw_wanted;

    /// MC/DC — 조건 둘이 각각 결과를 뒤집는 쌍.
    #[test]
    fn meta_redraw_wanted_mc_dc() {
        assert!(!meta_redraw_wanted(false, false), "기준 = 안 그림");
        assert!(meta_redraw_wanted(true, false), "완성 팝업 대기만으로 그림");
        assert!(meta_redraw_wanted(false, true), "링크 표시 중만으로 그림");
        assert!(meta_redraw_wanted(true, true));
    }
}
