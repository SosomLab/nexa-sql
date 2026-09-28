//! App — 트랜잭션 UX · 수동 커밋 잠금 방지(docs/34·56).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// DML/DDL 완료 → 대기 목록 갱신. 수동 모드의 DML(영향 행 > 0) = 대기 +1 · 방언별 암묵 커밋 DDL = 비움 ·
    /// 자동 모드 + `tx.smart_commit` = 첫 DML 뒤 수동으로 전환.
    pub(crate) fn tx_on_done(&mut self, index: usize, stmt: &str, rows_affected: Option<u64>) {
        let auto = self.settings.flag("session.autocommit");
        // ★ 사용자가 직접 실행한 COMMIT/ROLLBACK(09-27 사용자 실기): 서버는 끝냈는데 툴바·"N pending"·트랜잭션 로그는
        //   툴바 버튼 경로(`tx_close`)만 알아 그대로 남았다 → 같은 경로로 닫는다(러너의 `TxControl::End`와 같은 판정).
        if let Some(commit) = sessions::user_tx_end(stmt) {
            let open = !self.sess.tx_pending.is_empty() || self.sess.tx_dirty || self.sess.tx_read;
            if open {
                self.tx_close(if commit {
                    TxOutcome::Committed
                } else {
                    TxOutcome::RolledBack
                });
                self.sync_tx_ui();
            }
            return;
        }
        if implicit_commit(self.sess.dialect, stmt) {
            if !self.sess.tx_pending.is_empty() {
                let w = first_word(stmt);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, tf(Msg::StTxImplicit, &[&w])));
                self.tx_close(TxOutcome::ImplicitCommit(w));
            }
            return;
        }
        let class = nsql_core::TxClass::of_sql(stmt);
        // 대기 대상: 영향 행 > 0인 DML · 트랜잭션 DDL 방언(PG·SQL Server·SQLite)의 DDL/TRUNCATE(docs/44 §5).
        let ddl_pending = class.is_ddl() && self.sess.dialect.ddl_transactional();
        if !ddl_pending && !rows_affected.is_some_and(|n| n > 0) {
            return;
        }
        if auto {
            if self.settings.flag("tx.smart_commit") {
                self.set_autocommit_now(false);
                self.sess.status = t(Msg::StTxSmartSwitched).into();
            }
            return;
        }
        let stamp = nsql_log::now_local().stamp();
        let when = stamp.get(11..16).unwrap_or("").to_string();
        self.txlog
            .select_session(self.sess.id)
            .attach_tx(index, stamp, true);
        self.txlog_win.redraw();
        self.sess.tx_pending.push(TxItem {
            editor: self.sess.run_editor,
            at: Instant::now(),
            when,
            summary: one_line(stmt, 60),
            class,
        });
        self.sess.tx_dirty = true;
        self.sess.tx_stale_logged = false;
        self.sync_tx_ui();
    }

    /// 대기 목록 비우기(커밋 · 롤백 · 해제 · 암묵 커밋).
    fn tx_clear(&mut self) {
        self.tx_guard_reset();
        self.sess.tx_blockers = 0;
        self.sess.tx_blocker_who.clear();
        self.sess.tx_pending.clear();
        self.sess.tx_read = false;
        self.sess.tx_dirty = false;
        self.sess.tx_stale_logged = false;
        self.sync_tx_ui();
    }

    /// 수동 모드의 조회 — 읽기 트랜잭션 표시(배지 없음 · 초록 · docs/44 §5).
    pub(crate) fn tx_on_read(&mut self, index: usize) {
        if self.settings.flag("session.autocommit") {
            return;
        }
        // 수동 모드의 조회는 열린 트랜잭션에 속한다(롤백/커밋 시점 표시 · docs/44 §3).
        self.txlog.select_session(self.sess.id).attach_tx(
            index,
            nsql_log::now_local().stamp(),
            false,
        );
        self.txlog_win.redraw();
        if self.sess.tx_read {
            return;
        }
        self.sess.tx_read = true;
        self.sync_tx_ui();
    }

    /// docs/56 L3 — 미커밋 변경이 있는 세션마다 주기적으로 "나 때문에 기다리는 세션"을 메타 세션에 묻는다.
    /// 조건: 설정 주기 > 0 · 세션 식별자를 앎 · 그 서버의 탐색기(메타 세션)가 온라인 · 이 서버에서 꺼지지 않음 · 한가함.
    pub(crate) fn tx_block_tick(&mut self, now: Instant) -> Option<Instant> {
        let poll = self.settings.int("tx.block_poll_secs").max(0) as u64;
        if poll == 0 {
            return None;
        }
        let due: Vec<(u64, String, Option<ConnectSpec>)> = self
            .all_sess()
            .filter(|s| {
                !s.tx_pending.is_empty()
                    && !s.closing
                    && !s.tx_block_off
                    && s.connected
                    && !s.blocked()
                    && now >= s.tx_block_next
            })
            .filter_map(|s| s.live_sid.clone().map(|sid| (s.id, sid, s.spec.clone())))
            .collect();
        for (id, sid, spec) in due {
            if self.explorer.blockers_poll(spec.as_ref(), &sid) {
                self.with_sess(id, |a| {
                    a.sess.tx_block_next = now + Duration::from_secs(poll)
                });
            }
        }
        self.all_sess()
            .filter(|s| !s.tx_pending.is_empty() && !s.tx_block_off && s.live_sid.is_some())
            .map(|s| s.tx_block_next)
            .min()
    }

    /// 트랜잭션 로그 창의 "차단 중" 띠를 전 세션의 현재 상태로 맞춘다(세션마다 "연결 — 기다리는 세션").
    fn tx_block_band_sync(&mut self) {
        let lines: Vec<String> = self
            .all_sess()
            .filter(|s| !s.tx_pending.is_empty())
            .flat_map(|s| {
                let name = if s.profile.is_empty() {
                    s.desc.clone()
                } else {
                    s.profile.clone()
                };
                s.tx_blocker_who
                    .iter()
                    .map(move |w| format!("{name} — {w}"))
                    .collect::<Vec<_>>()
            })
            .collect();
        if self.txlog_win.set_blocking(lines) {
            self.txlog_win.redraw();
        }
    }

    /// 막힘 감지 응답 반영 — 0 → n이면 위험 토스트·로그·상태줄 · n → 0이면 해소 로그 · 오류(권한 없음)면 그 세션에서 기능을 끈다.
    pub(crate) fn tx_block_drain(&mut self) -> bool {
        let mut changed = false;
        for (sid, r) in self.explorer.take_blockers() {
            let Some(id) = self
                .all_sess()
                .find(|s| s.live_sid.as_deref() == Some(sid.as_str()))
                .map(|s| s.id)
            else {
                continue;
            };
            match r {
                Ok(list) => {
                    let prev = self.sess_by_id(id).map_or(0, |s| s.tx_blockers);
                    let pending = self
                        .sess_by_id(id)
                        .is_some_and(|s| !s.tx_pending.is_empty());
                    let n = if pending { list.len() } else { 0 };
                    // 띠의 내용(누가 기다리는가)은 개수가 같아도 바뀔 수 있다 → 먼저 저장.
                    let who: Vec<String> = if pending { list.clone() } else { Vec::new() };
                    self.with_sess(id, |a| a.sess.tx_blocker_who = who);
                    self.tx_block_band_sync();
                    if n == prev {
                        continue;
                    }
                    self.with_sess(id, |a| a.sess.tx_blockers = n);
                    changed = true;
                    if n > 0 {
                        let who = list.iter().take(5).cloned().collect::<Vec<_>>().join(" | ");
                        let text = tf(Msg::StTxBlocking, &[&n.to_string(), &who]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, text.clone()));
                        self.toasts.push(
                            toast::ToastKind::Error,
                            t(Msg::TxWarnToastTitle),
                            text.clone(),
                        );
                        self.sess.status = text;
                        if let Some(w) = &self.window {
                            w.request_user_attention(Some(
                                winit::window::UserAttentionType::Informational,
                            ));
                        }
                    } else {
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            t(Msg::StTxBlockingCleared).to_string(),
                        ));
                    }
                }
                Err(e) => {
                    self.with_sess(id, |a| {
                        a.sess.tx_block_off = true;
                        a.sess.tx_blockers = 0;
                    });
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        tf(Msg::StTxBlockPollOff, &[&e]),
                    ));
                    changed = true;
                }
            }
        }
        changed
    }

    /// 활동·정리 시 유휴 미커밋 상태를 되돌린다(이 세션 · 카드가 이 세션 것이면 걷는다).
    pub(crate) fn tx_guard_reset(&mut self) {
        self.sess.last_exec = Instant::now();
        self.sess.tx_warned_at = None;
        self.sess.tx_snooze_until = None;
        self.sess.tx_countdown = None;
        if self.tx_warn.sess_id() == Some(self.sess.id) {
            self.tx_warn.hide();
        }
    }

    /// docs/56 L2 — 모든 세션의 유휴 미커밋을 점검한다(판정 = `sessions::tx_guard_step`). 돌려주는 값 = 다음에 깰 시각.
    pub(crate) fn tx_guard_tick(&mut self, now: Instant) -> Option<Instant> {
        if now < self.tx_guard_next {
            return Some(self.tx_guard_next);
        }
        let ids: Vec<u64> = self
            .all_sess()
            .filter(|s| !s.tx_pending.is_empty() && !s.closing)
            .map(|s| s.id)
            .collect();
        if ids.is_empty() {
            if self.tx_warn.sess_id().is_some() {
                self.tx_warn.hide();
                self.redraw();
            }
            self.tx_guard_next = now + Duration::from_secs(3600);
            return None;
        }
        let stale_min = self.settings.int("tx.stale_min").max(1) as u64;
        let remind_min = self.settings.int("tx.remind_min").max(0) as u64;
        let limit_min = self.settings.int("tx.idle_limit_min").max(1) as u64;
        let countdown =
            Duration::from_secs(self.settings.int("tx.idle_countdown_secs").clamp(5, 600) as u64);
        let action = sessions::TxIdleAction::parse(
            self.settings.get("tx.idle_action").unwrap_or("rollback"),
        );
        for id in ids {
            let Some(s) = self.sess_by_id(id) else {
                continue;
            };
            // 운영 접속 = 더 엄격한 기준(`tx.prod_*` · 전역 값보다 느슨해지지는 않는다 · docs/56 §4).
            let prod = s.spec.as_ref().and_then(|sp| sp.env) == Some(nsql_script::ConnEnv::Prod);
            let (stale_min, limit_min) = sessions::tx_limits_for(
                prod,
                (stale_min, limit_min),
                (
                    self.settings.int("tx.prod_stale_min").max(1) as u64,
                    self.settings.int("tx.prod_idle_limit_min").max(1) as u64,
                ),
            );
            let input = sessions::TxGuardIn {
                pending: true,
                blocked: s.blocked(),
                idle: now.saturating_duration_since(s.last_exec),
                stale_min,
                remind_min,
                action,
                limit_min,
                since_warn: s.tx_warned_at.map(|t| now.saturating_duration_since(t)),
                snoozed: s.tx_snooze_until.is_some_and(|t| now < t),
                counting: s.tx_countdown.map(|t| t.saturating_duration_since(now)),
            };
            match sessions::tx_guard_step(input) {
                sessions::TxGuardStep::None => {}
                sessions::TxGuardStep::Counting => self.redraw(),
                sessions::TxGuardStep::Warn => {
                    self.with_sess(id, |a| a.sess.tx_warned_at = Some(now));
                    // 카운트다운 카드가 떠 있으면 그것이 우선.
                    if !self.tx_warn.counting() {
                        self.tx_warn_show(id, None);
                    }
                }
                sessions::TxGuardStep::StartCountdown => {
                    let deadline = now + countdown;
                    self.with_sess(id, |a| {
                        a.sess.tx_countdown = Some(deadline);
                        a.sess.tx_warned_at = Some(now);
                    });
                    self.tx_warn_show(id, Some((deadline, action)));
                }
                sessions::TxGuardStep::Fire => self.tx_guard_fire(id, action),
            }
        }
        self.tx_guard_next = now
            + if self.tx_warn.counting() {
                Duration::from_secs(1)
            } else {
                Duration::from_secs(5)
            };
        Some(self.tx_guard_next)
    }

    /// 경고/카운트다운 카드를 띄운다(+ 창이 비활성이면 작업 표시줄 깜빡임 · 로그 1줄).
    fn tx_warn_show(&mut self, id: u64, countdown: Option<(Instant, sessions::TxIdleAction)>) {
        let Some(s) = self.sess_by_id(id) else { return };
        let n = s.tx_pending.len();
        let idle_min = s.last_exec.elapsed().as_secs() / 60;
        let title = tf(Msg::TxWarnTitle, &[&n.to_string(), &idle_min.to_string()]);
        let first = s
            .tx_pending
            .first()
            .map(|i| i.summary.clone())
            .unwrap_or_default();
        let detail = if s.desc.is_empty() {
            first
        } else {
            format!("{} · {}", s.desc, first)
        };
        let (countdown, buttons) = match countdown {
            Some((deadline, act)) => {
                let commit = act == sessions::TxIdleAction::Commit;
                let word = t(if commit {
                    Msg::TxWarnCommitWord
                } else {
                    Msg::TxWarnRollbackWord
                })
                .to_string();
                (
                    Some((deadline, word)),
                    vec![
                        (
                            txwarn::TxWarnHit::Rollback,
                            t(Msg::BtnTxRollbackNow).to_string(),
                        ),
                        (txwarn::TxWarnHit::Commit, t(Msg::BtnTxCommit).to_string()),
                        (txwarn::TxWarnHit::Later, t(Msg::BtnTxExtend).to_string()),
                    ],
                )
            }
            None => (
                None,
                vec![
                    (txwarn::TxWarnHit::Commit, t(Msg::BtnTxCommit).to_string()),
                    (
                        txwarn::TxWarnHit::Rollback,
                        t(Msg::BtnTxRollback).to_string(),
                    ),
                    (txwarn::TxWarnHit::Later, t(Msg::BtnTxLater).to_string()),
                ],
            ),
        };
        self.log_win
            .push(LogEntry::new(LogKind::Error, format!("{title} — {detail}")));
        self.tx_warn.show(txwarn::TxWarnView {
            sess_id: id,
            title,
            detail,
            countdown,
            buttons,
        });
        if let Some(w) = &self.window {
            w.request_user_attention(Some(winit::window::UserAttentionType::Informational));
        }
        self.redraw();
    }

    /// 카드 버튼 — 커밋/롤백은 그 세션에(작업 중이면 무시) · 나중에/연장 = 재알림 간격(0이면 경고 간격)만큼 미룸.
    pub(crate) fn tx_warn_pick(&mut self, hit: txwarn::TxWarnHit) {
        let Some(id) = self.tx_warn.sess_id() else {
            return;
        };
        match hit {
            txwarn::TxWarnHit::Commit | txwarn::TxWarnHit::Rollback => {
                let commit = hit == txwarn::TxWarnHit::Commit;
                self.with_sess(id, |a| {
                    if a.sess.blocked() || a.sess.tx_pending.is_empty() {
                        return;
                    }
                    a.sess.worker.send(if commit {
                        worker::Cmd::Commit
                    } else {
                        worker::Cmd::Rollback
                    });
                    a.tx_close(if commit {
                        TxOutcome::Committed
                    } else {
                        TxOutcome::RolledBack
                    });
                });
                self.tx_warn.hide();
            }
            txwarn::TxWarnHit::Later => {
                let mins = match self.settings.int("tx.remind_min") {
                    m if m > 0 => m,
                    _ => self.settings.int("tx.stale_min").max(1),
                } as u64;
                let until = Instant::now() + Duration::from_secs(mins * 60);
                self.with_sess(id, |a| {
                    a.sess.tx_snooze_until = Some(until);
                    a.sess.tx_countdown = None;
                });
                self.tx_warn.hide();
            }
            txwarn::TxWarnHit::Card | txwarn::TxWarnHit::None => {}
        }
        self.tx_guard_next = Instant::now();
    }

    /// 카운트다운 만료 — 자동 처리(롤백/커밋) + 트랜잭션 로그·로그 창·토스트·상태줄 기록.
    fn tx_guard_fire(&mut self, id: u64, action: sessions::TxIdleAction) {
        let commit = action == sessions::TxIdleAction::Commit;
        let done = self.with_sess(id, |a| {
            if a.sess.blocked() || a.sess.tx_pending.is_empty() {
                a.sess.tx_countdown = None;
                return None;
            }
            let n = a.sess.tx_pending.len();
            let idle_min = a.sess.last_exec.elapsed().as_secs() / 60;
            a.sess.worker.send(if commit {
                worker::Cmd::Commit
            } else {
                worker::Cmd::Rollback
            });
            a.tx_close(if commit {
                TxOutcome::AutoCommitted(idle_min)
            } else {
                TxOutcome::AutoRolledBack(idle_min)
            });
            Some((n, idle_min))
        });
        if self.tx_warn.sess_id() == Some(id) {
            self.tx_warn.hide();
        }
        if let Some(Some((n, idle_min))) = done {
            let text = tf(
                if commit {
                    Msg::StTxAutoCommitted
                } else {
                    Msg::StTxAutoRolledBack
                },
                &[&n.to_string(), &idle_min.to_string()],
            );
            self.log_win
                .push(LogEntry::new(LogKind::Error, text.clone()));
            self.toasts.push(
                toast::ToastKind::Warn,
                t(Msg::TxWarnToastTitle),
                text.clone(),
            );
            self.sess.status = text;
        }
        self.redraw();
    }

    /// 러너가 변경 없는 읽기 트랜잭션을 끝냈다(docs/56 L1) — 읽기 표시를 걷고 트랜잭션 로그의 열린 기록을 닫는다.
    /// `deferred` = 열린 커서가 닫힐 때 끝난다(표시는 지금 걷는다 — 잠금은 커서 유휴 상한·다음 실행에서 풀린다).
    pub(crate) fn tx_on_read_ended(&mut self, index: usize, deferred: bool) {
        let _ = index;
        dlog!(self, LogLayer::Tx, LogLevel::Basic, {
            LogEntry::new(
                LogKind::Info,
                tf(
                    Msg::LogTxReadEnded,
                    &[if deferred {
                        " · deferred to cursor close"
                    } else {
                        ""
                    }],
                ),
            )
        });
        if !self.sess.tx_pending.is_empty() {
            return;
        }
        self.sess.tx_read = false;
        self.sess.tx_dirty = false;
        let stamp = nsql_log::now_local().stamp();
        self.txlog
            .select_session(self.sess.id)
            .close_tx(TxOutcome::ReadEnded, stamp);
        self.txlog_win.redraw();
        self.sync_tx_ui();
    }

    /// 트랜잭션 버튼 색·배지·툴팁(docs/44 §5): 자동 커밋 = 기본색·배지 없음 · 수동 = 가장 심각한 문장 종류의 색 + 대기 수.
    fn sync_tx_button(&mut self) {
        use nsql_core::TxClass;
        let auto = self.settings.flag("session.autocommit");
        let mut inv = Invalidations::default();
        let n = self.sess.tx_pending.len();
        let top = self
            .sess
            .tx_pending
            .iter()
            .map(|i| i.class)
            .max_by_key(|c| c.severity())
            .unwrap_or(if self.sess.tx_read {
                TxClass::Read
            } else {
                TxClass::None
            });
        let stale = self.tx_is_stale();
        let tone = if auto || top == TxClass::None {
            ToolTone::Default
        } else if stale {
            ToolTone::Danger
        } else {
            match top {
                TxClass::Read => ToolTone::Ok,
                TxClass::DdlCreate => ToolTone::Custom(nexa_ctl::Color(0x8E5BD6FF)),
                TxClass::Insert | TxClass::Other => ToolTone::Accent,
                TxClass::Update => ToolTone::Custom(nexa_ctl::Color(0xE0A020FF)),
                TxClass::Delete | TxClass::Truncate | TxClass::DdlDrop => ToolTone::Danger,
                TxClass::None => ToolTone::Default,
            }
        };
        let badge = if auto || n == 0 {
            None
        } else if stale {
            Some(format!("!{n}"))
        } else {
            Some(n.to_string())
        };
        let tip = if auto {
            t(Msg::TipTxLogAuto).to_string()
        } else if n == 0 && !self.sess.tx_read {
            t(Msg::TipTxLog).to_string()
        } else {
            let mut kinds: Vec<Msg> = Vec::new();
            let mut seen: Vec<TxClass> = self.sess.tx_pending.iter().map(|i| i.class).collect();
            if self.sess.tx_read && seen.is_empty() {
                seen.push(TxClass::Read);
            }
            seen.sort_by_key(|c| std::cmp::Reverse(c.severity()));
            seen.dedup();
            for c in seen {
                kinds.push(match c {
                    TxClass::Read => Msg::TxClsRead,
                    TxClass::Insert => Msg::TxClsInsert,
                    TxClass::Update => Msg::TxClsUpdate,
                    TxClass::Delete => Msg::TxClsDelete,
                    TxClass::Truncate => Msg::TxClsTruncate,
                    TxClass::DdlCreate => Msg::TxClsDdlCreate,
                    TxClass::DdlDrop => Msg::TxClsDdlDrop,
                    TxClass::Other | TxClass::None => Msg::TxClsOther,
                });
            }
            let list = kinds.iter().map(|m| t(*m)).collect::<Vec<_>>().join(" · ");
            let since = self
                .sess
                .tx_pending
                .first()
                .map_or(String::new(), |f| f.when.clone());
            tf(Msg::TipTxState, &[&n.to_string(), &since, &list])
        };
        self.tool_dock.set_item_tone("tx.log", tone, &mut inv);
        self.tool_dock
            .set_item_badge("tx.log", badge.as_deref(), &mut inv);
        if let Some(gid) = self.tool_dock.group_of("tx.log").map(str::to_string) {
            if let Some(bar) = self.tool_dock.bar_mut(&gid) {
                bar.set_item_tip("tx.log", &tip);
            }
        }
        for f in &self.tool_floats {
            f.redraw();
        }
    }

    /// 탭 배지 · 툴바 Commit/Rollback 활성·색 — 세 층이 같은 사실을 말한다.
    pub(crate) fn sync_tx_ui(&mut self) {
        self.sync_tx_button();
        let stale = self.tx_is_stale();
        // 탭 배지는 **모든 세션**의 대기 문장을 모은다(전용 세션 탭도 자기 배지) · 오래됨은 세션별 첫 문장 기준.
        let min = self.settings.int("tx.stale_min").max(1) as u64;
        let mut map: HashMap<u64, (usize, bool)> = HashMap::new();
        for s in self.all_sess() {
            let st = s
                .tx_pending
                .first()
                .is_some_and(|f| f.at.elapsed().as_secs() >= min * 60);
            for it in &s.tx_pending {
                let e = map.entry(it.editor).or_insert((0, st));
                e.0 += 1;
                e.1 = st;
            }
        }
        let mode = self.settings.get("tx.badge").unwrap_or("count").to_string();
        self.editors.set_tx_badges(map, &mode);
        let has = !self.sess.tx_pending.is_empty();
        // 툴바 Commit/Rollback = 지금 세션에 대기 문장이 있고 **세션이 한가할 때만**(통제 · docs/52 §3) · 색은 대기 여부 그대로.
        let open = self.gate().commit;
        let mut inv = Invalidations::default();
        for id in ["run.commit", "run.rollback"] {
            self.tool_dock.set_item_enabled(id, open, &mut inv);
            self.tool_dock.set_item_tone(
                id,
                if !has {
                    ToolTone::Default
                } else if stale {
                    ToolTone::Danger
                } else {
                    ToolTone::Accent
                },
                &mut inv,
            );
        }
        self.redraw();
    }

    fn tx_is_stale(&self) -> bool {
        let min = self.settings.int("tx.stale_min").max(1) as u64;
        self.sess
            .tx_pending
            .first()
            .is_some_and(|f| f.at.elapsed().as_secs() >= min * 60)
    }

    /// 오래된 미커밋 감시(about_to_wait · 1회 로그 + 배지 ⚠).
    pub(crate) fn tx_tick(&mut self) {
        if self.sess.tx_pending.is_empty() || self.sess.tx_stale_logged || !self.tx_is_stale() {
            return;
        }
        self.sess.tx_stale_logged = true;
        let min = self.settings.int("tx.stale_min").max(1).to_string();
        self.log_win
            .push(LogEntry::new(LogKind::Error, tf(Msg::StTxStale, &[&min])));
        self.sess.status = tf(Msg::StTxStale, &[&min]);
        self.sync_tx_ui();
    }

    /// 상태줄 트랜잭션 팝업: 모드 전환 · Commit(n) · Rollback(n) · 대기 문장 목록.
    pub(crate) fn open_tx_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let auto = self.settings.flag("session.autocommit");
        let n = self.sess.tx_pending.len();
        let mark = |on: bool, s: &str| {
            if on {
                format!("✓ {s}")
            } else {
                format!("   {s}")
            }
        };
        let mut items = vec![
            CtxItem::item("tx.auto", mark(auto, t(Msg::MnTxAuto))).with_active(auto),
            CtxItem::item("tx.manual", mark(!auto, t(Msg::MnTxManual))).with_active(!auto),
            CtxItem::Separator,
            CtxItem::item("tx.commit", tf(Msg::MnTxCommitN, &[&n.to_string()]))
                .with_shortcut(self.keymap.display_of("run.commit")),
            CtxItem::item("tx.rollback", tf(Msg::MnTxRollbackN, &[&n.to_string()]))
                .with_shortcut(self.keymap.display_of("run.rollback")),
        ];
        if n > 0 {
            items.push(CtxItem::Separator);
            for it in self.sess.tx_pending.iter().take(12) {
                items.push(CtxItem::item(
                    "tx.noop",
                    format!("{}  {}", it.when, it.summary),
                ));
            }
        }
        self.open_status_popup(self.status_tx_rect, items);
    }

    /// 잃는 순간의 확인 팝업(Commit / Rollback / Cancel) — 상태줄 트랜잭션 세그먼트 자리(없으면 창 가운데).
    pub(crate) fn open_tx_guard(&mut self, commit: Msg, rollback: Msg) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let n = self.sess.tx_pending.len().to_string();
        self.sess.status = tf(Msg::StTxGuard, &[&n]);
        let items = vec![
            CtxItem::item("tx.commit_then", t(commit)),
            CtxItem::item("tx.rollback_then", t(rollback)),
            CtxItem::Separator,
            CtxItem::item("tx.cancel", t(Msg::MnTxCancel)),
        ];
        let mut r = self.status_tx_rect;
        if r.w == 0 {
            if let Some(w) = &self.window {
                let sz = w.inner_size();
                r = Rect::new(
                    sz.width as i32 / 2 - px(120.0, self.scale),
                    sz.height as i32 / 2,
                    0,
                    0,
                );
            }
        }
        self.open_status_popup(r, items);
    }

    /// 팝업 항목(`tx.*`).
    pub(crate) fn tx_pick(&mut self, id: &str) {
        match id {
            "auto" => self.set_autocommit(true),
            "manual" => self.set_autocommit(false),
            // 상태줄 팝업의 Commit/Rollback도 메뉴·툴바와 같은 문지기를 지난다(docs/52 §3).
            "commit" => self.menu_action("run.commit"),
            "rollback" => self.menu_action("run.rollback"),
            "commit_then" => {
                self.sess.worker.send(worker::Cmd::Commit);
                self.tx_close(TxOutcome::Committed);
                self.run_tx_after();
            }
            "rollback_then" => {
                self.sess.worker.send(worker::Cmd::Rollback);
                self.tx_close(TxOutcome::RolledBack);
                self.run_tx_after();
            }
            "cancel" => self.tx_after = None,
            _ => {}
        }
    }

    fn run_tx_after(&mut self) {
        match self.tx_after.take() {
            Some(TxAfter::CloseTab(i)) => {
                self.editors.close_tab_confirmed(i);
                self.set_focus(Focus::Editor);
            }
            Some(TxAfter::Disconnect) => self.disconnect_force(),
            // 다른 세션에도 미커밋이 남아 있을 수 있다 → 다시 점검(전부 답해야 종료).
            Some(TxAfter::Exit) => self.request_exit(),
            Some(TxAfter::SwitchAuto) => self.set_autocommit_now(true),
            None => {}
        }
    }

    /// 모드 전환 — 수동 → 자동인데 대기 문장이 있으면 먼저 묻는다.
    fn set_autocommit(&mut self, on: bool) {
        if on && !self.sess.tx_pending.is_empty() {
            self.tx_after = Some(TxAfter::SwitchAuto);
            self.open_tx_guard(Msg::MnTxCommitSwitch, Msg::MnTxRollbackSwitch);
            return;
        }
        self.set_autocommit_now(on);
    }

    /// 설정 + **살아 있는 모든 세션**(워커 명령 `Cmd::Autocommit` · 큐 순서 보장 · 09-28). 세션이 작업 중이면 문지기가 거부(설정도
    /// 그대로 — 설정과 세션이 어긋나지 않게 · 09-19 검토).
    /// 🔧 09-28(사용자 실기 "자동 커밋인데 커밋/롤백 버튼 활성"): 종전엔 `SET AUTOCOMMIT` 스크립트를 **활성 세션 하나**에만, 그것도
    ///   한가할 때만 보냈다 → 다른 연결·탭 전용 세션·잠든 세션의 러너는 옛 모드로 남아 그리드 적용이 `tx_left_open`을 보고했다.
    fn set_autocommit_now(&mut self, on: bool) {
        if !self.gate_open() {
            return;
        }
        let _ = self
            .settings
            .set("session.autocommit", if on { "on" } else { "off" });
        self.persist_settings();
        for s in self.all_sess() {
            s.worker.send(worker::Cmd::Autocommit(on));
        }
        self.log_win.push(LogEntry::new(
            LogKind::Info,
            if on {
                "SET AUTOCOMMIT ON".to_string()
            } else {
                "SET AUTOCOMMIT OFF".to_string()
            },
        ));
        if on {
            self.tx_close(TxOutcome::Switched);
        }
        self.sync_tx_ui();
    }

    /// 열린 수동 트랜잭션을 결과와 함께 닫고 대기 목록을 비운다(커밋·롤백·암묵·전환·끊김 — docs/44 §3).
    pub(crate) fn tx_close(&mut self, outcome: TxOutcome) {
        // (세션이 끊기는 경로도 여기를 지난다 — Lost) 큰 결과·메타를 놓았을 수 있다.
        if matches!(outcome, TxOutcome::Lost) {
            self.mem_released();
        }
        // 커밋을 기다리던 DDL(docs/57 D-107): 커밋 계열 = 이제 다른 세션(메타 세션)도 본다 → 반영 · 롤백·소실 = 버림 ·
        // 읽기 트랜잭션 종료는 변경이 없던 것이라 대기열과 무관.
        match &outcome {
            TxOutcome::Committed
            | TxOutcome::ImplicitCommit(_)
            | TxOutcome::Switched
            | TxOutcome::AutoCommitted(_) => {
                let ddl = std::mem::take(&mut self.sess.ddl_wait);
                self.meta_flush(ddl);
            }
            TxOutcome::RolledBack | TxOutcome::Lost | TxOutcome::AutoRolledBack(_) => {
                self.sess.ddl_wait.clear();
            }
            _ => {}
        }
        let stamp = nsql_log::now_local().stamp();
        self.txlog
            .select_session(self.sess.id)
            .close_tx(outcome, stamp);
        self.tx_clear();
        self.txlog_win.redraw();
    }
}
