//! App — ★ 객체 삭제 흐름(10-01 · 사용자 "탐색기 우클릭 삭제 · 모달 확인 · 백업 후 삭제 · 운영은 백업 실패 시 삭제 금지"):
//!
//! 탐색기 ▸ Delete… → `DropReq`(객체 · 서버 · 프로필 · 유형 · DROP 문) → 모달 `DropWin`(정보 + 2단 타임아웃 삭제 버튼)
//!   → Proceed → [`explorer.drop_backup` 켬] DDL 생성 요청(탐색기 메타 세션 · `GenWhat::Ddl` 전체) → 파일 백업(`backups::write_drop`)
//!       → 성공 = DROP 실행 · 실패 = 운영(Prod) 접속이면 Blocked(삭제 안 함) · 아니면 BackupFailed(백업 없이 삭제 = 다시 타임아웃 재확인)
//!   → DROP = 그 서버의 세션으로 `run_text_whole`(Output · 트랜잭션 로그 · DDL 감지 → 탐색기 갱신 = 보통 실행과 같은 길).
//! 결정(권장안 · 10-01): D-240 백업 폴더 `<설정>/backups/drop/` · 보관 = `project.backup_days` · D-241 DROP 문은 옵션 없는 보수형(CASCADE/PURGE
//! 없음 · 종속이 있으면 서버 오류를 그대로) · D-242 삭제 세션 = 그 서버의 공유 세션(없으면 전용) · D-245 타임아웃 = `login.delete_confirm_ms`.
use crate::drop_win::{DropAction, DropPhase};
use crate::*;

/// 삭제 요청(창이 열리기 전 · 흐름 내내 유지).
#[derive(Clone, Debug)]
pub(crate) struct DropReq {
    pub owner: nsql_catalog::ObjectInfo,
    pub server: Option<ConnectSpec>,
    pub profile: String,
    pub env: Option<nsql_script::ConnEnv>,
    pub dialect: Dialect,
    pub sql: String,
    /// 백업 파일(성공했을 때).
    pub backup: Option<PathBuf>,
}

impl App {
    /// 탐색기 메뉴 "Delete…" → 요청을 만들고 창 열기를 예약(창은 `about_to_wait`에서).
    pub(crate) fn drop_request(
        &mut self,
        owner: nsql_catalog::ObjectInfo,
        server: Option<ConnectSpec>,
    ) {
        let dialect = server
            .as_ref()
            .and_then(|s| s.dialect)
            .unwrap_or(self.sess.dialect);
        let Some(sql) = nsql_catalog::drop_sql(dialect, &owner) else {
            self.sess.status = tf(Msg::DropUnsupported, &[owner.kind.code()]);
            self.redraw();
            return;
        };
        // 프로필 이름·유형 = 그 서버에 붙은 세션(공유 우선)에서 · 없으면 접속 창 목록의 같은 서버 프로필.
        let (profile, env) = self.server_profile_env(server.as_ref());
        self.drop_req = Some(DropReq {
            owner,
            server,
            profile,
            env,
            dialect,
            sql,
            backup: None,
        });
        self.open_drop = true;
        self.redraw();
    }

    /// 창 열기(`about_to_wait` · `open_drop` 깃발) — 메인 창의 자식 모달.
    pub(crate) fn open_drop_window(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        let Some(req) = self.drop_req.clone() else {
            return;
        };
        let lines = self.drop_lines(&req);
        let timeout = self
            .settings
            .int("login.delete_confirm_ms")
            .clamp(1000, 60_000) as u64;
        let owner = self.window.clone();
        let was_open = self.drop_win.is_open();
        self.drop_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            owner.as_deref(),
            lines,
            timeout,
        );
        if !was_open {
            if let (Some(o), Some(c)) = (owner.as_deref(), self.drop_win.window()) {
                winfocus::attach_child(o, c);
            }
        }
        self.sync_modal();
    }

    /// 그 서버에 붙은 세션의 프로필 이름 · 접속 유형(없으면 빈 값 · None).
    fn server_profile_env(
        &self,
        server: Option<&ConnectSpec>,
    ) -> (String, Option<nsql_script::ConnEnv>) {
        let Some(spec) = server else {
            return (
                self.sess.profile.clone(),
                self.sess.spec.as_ref().and_then(|s| s.env),
            );
        };
        let found = self
            .all_sess()
            .filter(|s| !s.closing)
            .find(|s| {
                s.spec
                    .as_ref()
                    .is_some_and(|have| worker::same_server(have, spec))
            })
            .map(|s| (s.profile.clone(), s.spec.as_ref().and_then(|x| x.env)));
        found.unwrap_or_else(|| (String::new(), spec.env))
    }

    /// 창에 보일 정보 줄.
    pub(crate) fn drop_lines(&self, r: &DropReq) -> Vec<(String, String)> {
        let o = &r.owner;
        let env = env_label(r.env);
        let server = r
            .server
            .as_ref()
            .map(ConnectSpec::connection_string)
            .unwrap_or_else(|| self.sess.desc.clone());
        let mut v = vec![
            (
                t(Msg::DropRowServer).to_string(),
                format!("{} · {}", r.profile, server),
            ),
            (t(Msg::DropRowEnv).to_string(), env),
            (
                t(Msg::DropRowObject).to_string(),
                format!("{}.{}", o.schema, o.name),
            ),
            (
                t(Msg::DropRowKind).to_string(),
                o.kind.code().to_ascii_uppercase(),
            ),
        ];
        if !o.status.is_empty() {
            v.push((t(Msg::DropRowStatus).to_string(), o.status.clone()));
        }
        if !o.modified.is_empty() {
            v.push((t(Msg::DropRowModified).to_string(), o.modified.clone()));
        }
        if !o.extra.is_empty() {
            v.push((t(Msg::DropRowExtra).to_string(), o.extra.clone()));
        }
        v.push((t(Msg::DropRowStatement).to_string(), r.sql.clone()));
        let backup = if self.settings.flag("explorer.drop_backup") {
            tf(
                Msg::DropRowBackupOn,
                &[&self.settings.int("project.backup_days").to_string()],
            )
        } else {
            t(Msg::DropRowBackupOff).to_string()
        };
        v.push((t(Msg::DropRowBackup).to_string(), backup));
        v
    }

    /// 창 사건 처리(이벤트 루프에서).
    pub(crate) fn drop_action(&mut self, a: DropAction) {
        match a {
            DropAction::Paint => {
                let ui_px = self.settings.font_px("ui.font_size");
                self.drop_win.paint(&self.ui_font, &self.theme, ui_px);
            }
            DropAction::Close => {
                self.drop_win.close();
                self.drop_req = None;
                self.drop_backup_wait = None;
                self.sync_modal();
                self.redraw();
            }
            DropAction::Proceed => self.drop_proceed(),
            DropAction::ProceedNoBackup => {
                self.output_push(
                    self.editors.active_id(),
                    crate::output::OutKind::Warn,
                    t(Msg::DropNoBackupWarn),
                );
                self.drop_execute();
            }
            DropAction::None => {}
        }
    }

    /// 삭제 재확인 발화: 백업 켬 = DDL 요청 · 끔 = 바로 실행.
    fn drop_proceed(&mut self) {
        let Some(req) = self.drop_req.clone() else {
            return;
        };
        if !self.settings.flag("explorer.drop_backup") {
            self.drop_execute();
            return;
        }
        self.drop_win.set_phase(DropPhase::Backing);
        let Some(server) = req.server.clone() else {
            self.drop_backup_failed(t(Msg::DropBackupNoServer).to_string());
            return;
        };
        let mut opts = gen_opts_from(&self.settings);
        opts.full_ddl = true;
        opts.qualified = true;
        self.drop_backup_wait = Some(req.owner.clone());
        if !self
            .explorer
            .gen_object_on(&server, req.owner, nsql_catalog::GenWhat::Ddl, opts)
        {
            self.drop_backup_wait = None;
            self.drop_backup_failed(t(Msg::DropBackupNoServer).to_string());
        }
    }

    /// DDL 생성 결과가 삭제 백업용이면 가로챈다(`ExplorerAction::Preview`) — true = 가로챘다.
    pub(crate) fn drop_take_backup(
        &mut self,
        spec: &nsql_catalog::GenSpec,
        r: &Result<String, String>,
    ) -> bool {
        let Some(wait) = self.drop_backup_wait.as_ref() else {
            return false;
        };
        if spec.what != nsql_catalog::GenWhat::Ddl
            || spec.owner.schema != wait.schema
            || spec.owner.name != wait.name
        {
            return false;
        }
        self.drop_backup_wait = None;
        match r {
            Ok(text) => {
                let Some(req) = self.drop_req.clone() else {
                    return true;
                };
                match backups::write_drop(&req.profile, &req.owner, &req.dialect.to_string(), text)
                {
                    Ok(path) => {
                        self.drop_win
                            .push_line(t(Msg::DropRowBackupFile), path.display().to_string());
                        if let Some(r) = self.drop_req.as_mut() {
                            r.backup = Some(path.clone());
                        }
                        self.output_push(
                            self.editors.active_id(),
                            crate::output::OutKind::Info,
                            &tf(Msg::DropBackupOk, &[&path.display().to_string()]),
                        );
                        backups::prune_drop(self.settings.int("project.backup_days").max(1) as u64);
                        self.drop_execute();
                    }
                    Err(e) => self.drop_backup_failed(e),
                }
            }
            Err(e) => self.drop_backup_failed(e.clone()),
        }
        true
    }

    /// 백업 실패: 운영 접속 = 삭제 금지(Blocked) · 그 밖 = 백업 없이 삭제 재확인(BackupFailed).
    fn drop_backup_failed(&mut self, e: String) {
        let prod = self
            .drop_req
            .as_ref()
            .is_some_and(|r| r.env == Some(nsql_script::ConnEnv::Prod));
        self.output_push(
            self.editors.active_id(),
            crate::output::OutKind::Error,
            &tf(Msg::DropBackupFailed, &[&e]),
        );
        if prod {
            self.drop_win
                .set_phase(DropPhase::Blocked(t(Msg::DropProdBlocked).to_string()));
            self.toasts.push(
                toast::ToastKind::Error,
                t(Msg::DropWinTitle).to_string(),
                t(Msg::DropProdBlocked).to_string(),
            );
        } else {
            self.drop_win.set_phase(DropPhase::BackupFailed(e));
        }
        self.redraw();
    }

    /// DROP 문을 그 서버의 세션으로 보낸다(보통 실행과 같은 길 = Output · 트랜잭션 로그 · DDL 감지).
    fn drop_execute(&mut self) {
        let Some(req) = self.drop_req.clone() else {
            return;
        };
        let sid = match req.server.as_ref() {
            Some(spec) => self
                .all_sess()
                .filter(|s| !s.closing && s.connected)
                .find(|s| {
                    s.spec
                        .as_ref()
                        .is_some_and(|have| worker::same_server(have, spec))
                })
                .map(|s| s.id),
            None => Some(self.sess.id),
        };
        let Some(sid) = sid else {
            self.drop_win
                .set_phase(DropPhase::Blocked(t(Msg::DropNoSession).to_string()));
            return;
        };
        let sql = req.sql.clone();
        let (kind, name) = (
            req.owner.kind.code().to_ascii_uppercase(),
            format!("{}.{}", req.owner.schema, req.owner.name),
        );
        let ed = self.editors.active_id();
        self.output_push(
            ed,
            crate::output::OutKind::Warn,
            &tf(Msg::DropRunning, &[&kind, &name, &sql]),
        );
        let sent = self
            .with_sess(sid, |a| {
                if !a.gate_open() {
                    return false;
                }
                a.run_text_whole(sql.clone());
                true
            })
            .unwrap_or(false);
        if sent {
            self.sess_ui_dirty = true;
            self.drop_win
                .set_phase(DropPhase::Done(tf(Msg::DropSent, &[&kind, &name])));
            self.sess.status = tf(Msg::DropSent, &[&kind, &name]);
        } else {
            self.drop_win
                .set_phase(DropPhase::Blocked(t(Msg::DropSessionBusy).to_string()));
        }
        self.redraw();
    }

    /// 자체 시험 덤프(`drop.dump:`).
    pub(crate) fn drop_dump(&self) -> String {
        let mut s = self.drop_win.dump();
        if let Some(r) = &self.drop_req {
            s.push_str(&format!("sql={}\nbackup={:?}\n", r.sql, r.backup));
        }
        s
    }

    // ── 서버 유형 · 접속 정보(10-01 · 사용자 "CONNECT 한 경우 유형 지정 불가 → 접속 중 유형 지정 명령 · 접속 정보 Output 출력")

    /// 지금 탭 세션의 서버 유형을 **임시로** 바꾼다(사용자 10-01 — 영속 = 프로필 편집뿐 · 세션 연결 뒤 변경과 편집기 탭
    /// `CONNTYPE`은 임시 · D-243 정정). 세션 스펙과 재접속용 `last_spec`만 바꾸고 저장소는 건드리지 않는다.
    pub(crate) fn set_session_env(&mut self, env: Option<nsql_script::ConnEnv>) {
        let Some(spec) = self.sess.spec.as_mut() else {
            self.sess.status = t(Msg::ExpNotConnected).into();
            self.redraw();
            return;
        };
        // 같은 값 = 변경 없음(임시 표식도 건드리지 않는다 · 사용자 10-01).
        if spec.env == env {
            let line = tf(Msg::OutSessEnvSame, &[&self.sess.desc, &env_label(env)]);
            self.sess.status = line.clone();
            self.output_push(
                self.editors.active_id(),
                crate::output::OutKind::Info,
                &line,
            );
            self.redraw();
            return;
        }
        spec.env = env;
        if let Some(ls) = self.sess.last_spec.as_mut() {
            ls.env = env;
        }
        self.sess.env_temp = true;
        let label = env_label(env);
        let line = tf(Msg::OutSessEnvSet, &[&self.sess.desc, &label]);
        self.sess.status = line.clone();
        self.output_push(
            self.editors.active_id(),
            crate::output::OutKind::Info,
            &line,
        );
        self.sess_ui_dirty = true;
        self.redraw();
    }

    /// ★ 팔레트·세션 메뉴에서 유형 변경(10-01 · 사용자): **운영 → 다른 유형**은 2단(3초 안에 같은 항목을 다시 고르면 진행 ·
    /// 편집기 실행의 2단 확인 관례 · 토스트 + 상태줄 + Output 한 줄) · 그 밖은 바로. 판정 = `sessions::env_change_needs_confirm`.
    pub(crate) fn set_session_env_guarded(&mut self, env: Option<nsql_script::ConnEnv>) {
        let cur = self.sess.spec.as_ref().and_then(|sp| sp.env);
        if crate::sessions::env_change_needs_confirm(
            cur,
            env,
            self.settings.flag("run.prod_confirm"),
        ) {
            let key =
                nexa_fs::watch::content_hash(format!("sess.env:{}", env_label(env)).as_bytes());
            let armed = self
                .sess
                .prod_armed
                .is_some_and(|(k, at)| k == key && at.elapsed() <= Duration::from_secs(3));
            if !armed {
                self.sess.prod_armed = Some((key, Instant::now()));
                let line = tf(Msg::StProdEnvConfirm, &[&env_label(cur), &env_label(env)]);
                self.sess.status = line.clone();
                self.toasts.push(
                    crate::toast::ToastKind::Error,
                    t(Msg::StProdConfirmTitle),
                    line.clone(),
                );
                self.output_push(
                    self.editors.active_id(),
                    crate::output::OutKind::Info,
                    &line,
                );
                self.redraw();
                return;
            }
            self.sess.prod_armed = None;
        }
        self.set_session_env(env);
    }

    /// 현재 탭의 접속 정보를 Output에 쓴다.
    pub(crate) fn output_conn_info(&mut self) {
        let ed = self.editors.active_id();
        let s = &self.sess;
        let mut lines = vec![t(Msg::OutConnInfoTitle).to_string()];
        if !s.connected {
            lines.push(format!("  {}", t(Msg::ExpNotConnected)));
        } else {
            let spec = s.spec.as_ref();
            let kind = if s.is_private() {
                t(Msg::OutConnPrivate)
            } else {
                t(Msg::OutConnShared)
            };
            lines.push(format!("  {}: {}", t(Msg::OutConnDesc), s.desc));
            lines.push(format!(
                "  {}: {}",
                t(Msg::OutConnProfile),
                if s.profile.trim().is_empty() {
                    "—"
                } else {
                    s.profile.trim()
                }
            ));
            lines.push(format!("  {}: {:?}", t(Msg::OutConnDialect), s.dialect));
            // 임시로 바꾼 유형이면 프로필의 값을 곁에(영속 값과 지금 값이 다를 수 있다 · 저장소는 여기서만 읽는다).
            let mut env_line = env_label(spec.and_then(|x| x.env));
            if s.env_temp {
                let profile = s.profile.trim();
                let saved = (!profile.is_empty() && nsql_vault::is_profile_name(profile))
                    .then(|| {
                        Vault::open_default()
                            .and_then(|v| v.get(profile))
                            .ok()
                            .flatten()
                    })
                    .flatten();
                let base = saved.map_or_else(|| "—".to_string(), |sp| env_label(sp.env));
                env_line = format!("{env_line} ({})", tf(Msg::OutConnEnvTemp, &[&base]));
            }
            lines.push(format!("  {}: {}", t(Msg::OutConnEnv), env_line));
            lines.push(format!(
                "  {}: {}",
                t(Msg::OutConnSchema),
                s.cur_schema.as_deref().unwrap_or("—")
            ));
            lines.push(format!("  {}: {} #{}", t(Msg::OutConnSession), kind, s.id));
            lines.push(format!(
                "  {}: {}",
                t(Msg::OutConnTx),
                if s.tx_pending.is_empty() {
                    t(Msg::OutConnTxNone).to_string()
                } else {
                    s.tx_pending.len().to_string()
                }
            ));
            if s.broken {
                lines.push(format!("  ⚠ {}", t(Msg::StSessBrokenTag)));
            }
        }
        let text = lines.join("\n");
        self.output_push(ed, crate::output::OutKind::Info, &text);
        self.redraw();
    }
}

/// 서버 유형 표시 = 정식 용어 + 3자리 약어(사용자 10-01 · 예 `운영 (PRD)`) — 메뉴 문구 `서버 유형: 운영 (PRD)`에서 값만.
pub(crate) fn env_label(env: Option<nsql_script::ConnEnv>) -> String {
    let s = t(env_menu_msg(env));
    s.split_once(": ").map_or(s, |(_, v)| v).to_string()
}

/// 유형별 메뉴 문구(목록 우클릭 · 세션 메뉴 · 팔레트 · 프로필 폼 콤보가 같은 문구를 쓴다).
pub(crate) fn env_menu_msg(env: Option<nsql_script::ConnEnv>) -> Msg {
    match env {
        Some(nsql_script::ConnEnv::Prod) => Msg::MnEnvProd,
        Some(nsql_script::ConnEnv::Test) => Msg::MnEnvTest,
        Some(nsql_script::ConnEnv::Dev) => Msg::MnEnvDev,
        None => Msg::MnEnvNone,
    }
}
