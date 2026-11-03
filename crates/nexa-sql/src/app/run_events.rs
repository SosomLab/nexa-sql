//! App — `drain_events`의 큰 `RunEvent::*` 가지 본문(220차 T-248 · 11-01 · docs/93 §8) — 사건 하나 = 메서드 하나(`on_ev_*`) ·
//!   본문은 `events.rs`에서 **그대로 옮긴 것**(동작 불변 · 들여쓰기만) · 루프 제어가 필요한 가지(Error)는 bool을 돌려 호출부가 `continue`.
//!   새 사건 처리는 여기(또는 그 기능의 app/<기능>.rs)에 · `drain_events`는 분기와 공통 전후 처리만 남긴다.

use crate::*;

impl App {
    /// `drain_events`의 `RunEvent::Begin` 가지(220차 T-248 분해 · 동작 불변 · 11-01).
    pub(crate) fn on_ev_begin(&mut self, index: usize, server: bool, summary: &str) {
        // 접속 명령(`CONNECT`·`DISCONNECT`)은 문장이 아니다 — 트랜잭션 로그에 넣으면 `Done`이 없어 "실행 중"으로 남는다(139차 win).
        let conn_cmd = matches!(summary, "Connect" | "Disconnect");
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
        // ★ T-123(219차): Oracle GTT에 넣는 평범한 INSERT/MERGE도 세션 상태(적재된 메타의 TEMPORARY · 왕복 0).
        if server
            && !self.sess.stateful
            && (sessions::alters_session_state(&stmt) || self.inserts_into_temp(&stmt))
        {
            self.sess.stateful = true;
            let head: String = stmt.lines().next().unwrap_or("").chars().take(80).collect();
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
        // 트랜잭션 로그 = **서버로 가는 문장만**(139차 win V1): 클라이언트 명령(`DEFINE`·`VARIABLE`·로컬 `EXEC :A := 5` …)과
        // 접속 명령은 `Done`이 오지 않아 "실행 중"으로 남았다.
        if server && !conn_cmd {
            self.txlog.select_session(self.sess.id).begin(
                nsql_log::now_local().stamp(),
                self.sess.run_editor,
                purpose,
                index,
                &stmt,
            );
            self.txlog_win.redraw();
        }
    }

    /// `drain_events`의 `RunEvent::ResultSet` 가지(220차 T-248 분해 · 동작 불변 · 11-01).
    pub(crate) fn on_ev_result_set(
        &mut self,
        index: usize,
        rs: nsql_core::ResultSet,
        elapsed: std::time::Duration,
        more: bool,
        label: Option<String>,
    ) {
        self.txlog
            .select_session(self.sess.id)
            .result(index, rs.rows.len() as u64, elapsed);
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
        // 방언을 아는 분할(D-434): DuckDB·ClickHouse `FROM t` · PG `TABLE t`도 조회 = 건수·재질의 가능.
        let dialect = self.sess.dialect;
        let is_query = stmt.as_deref().is_some_and(|s| {
            nsql_script::split_script_in(s, Some(dialect))
                .first()
                .is_some_and(|it| {
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
        // ★ 운영 여부도 같은 이유로 **그 결과를 만든 세션의 것**(10-15 사용자 실기: 전용 NON 세션을 쓰다 공유 PRD로
        //   돌아온 탭의 그리드가 옛 `prod=false`를 쥔 채 재실행돼 편집이 열렸다 · 서버 유형 게이트 `announce_env_change`의 짝).
        let prod =
            self.sess.spec.as_ref().and_then(|sp| sp.env) == Some(nsql_script::ConnEnv::Prod);
        // ★ 증분 표시(사용자 10-09): 탭의 행 수가 0(전체)이고 더 있으면 첫 세그먼트를 보인 채 **나머지를 자동으로 이어 받는다**
        //   (⇊와 같은 길 · 배치마다 `Batch` · ■/진행률/예산은 배치 경계) — `whole`이면 종전대로(드라이버가 전부 읽은 뒤 표시).
        let live = self.settings.get("grid.fetch_display") != Some("whole");
        if let Some(g) = self.grid_for(k) {
            g.set_result(rs);
            g.set_dialect(dial);
            g.set_prod(prod);
            g.set_more(more);
            if let Some(s) = stmt.as_deref() {
                g.set_result_origin(s, is_query);
            }
            if live && more && g.page_rows() == 0 && is_query {
                // 세션이 아직 busy(Done 전)라 여기서 청하면 `session_blocked`에 막힌다 → 예약 · Done 뒤 거둔다(협업 bin119).
                g.arm_auto_fetch_all();
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

    /// `drain_events`의 `RunEvent::DriverLimits` 가지(220차 T-248 분해 · 동작 불변 · 11-01).
    pub(crate) fn on_ev_driver_limits(
        &mut self,
        driver: String,
        limits: Vec<nsql_core::DriverLimit>,
        at_connect: bool,
    ) {
        let title = tf(
            if at_connect {
                Msg::ToastDriverLimitsTitle
            } else {
                Msg::ToastDriverLimitsHit
            },
            &[&driver],
        );
        // ★ 토스트 = 핵심 낱말만 한 줄 + "자세한 내용은 로그 창"(사용자 10-13 "경고가 너무 길다") · 상세(• 줄 + 전환 안내)는 로그 창에.
        let short = limits
            .iter()
            .map(|l| t(crate::app::output::limit_short(*l)))
            .collect::<Vec<_>>()
            .join(" · ");
        // ★ 접속 직후 안내는 "다시 보지 않기"(사용자 10-21): `notice.oracle_thin`이 꺼져 있으면 토스트만 생략(로그 창 상세는 그대로) ·
        //   켜져 있으면 [다시 보지 않기] 버튼 = `notice.hide:oracle_thin` → 설정 ▸ 숨겨진 메시지에서 다시 켠다. 제약 기능을 실제로
        //   요청했을 때의 경고(`at_connect == false`)는 숨기지 않는다.
        if !at_connect {
            self.toasts.push(
                toast::ToastKind::Warn,
                title.clone(),
                tf(Msg::ToastLimitsShort, &[&short]),
            );
        } else if self.settings.flag("notice.oracle_thin") {
            self.toasts.push_button(
                toast::ToastKind::Warn,
                title.clone(),
                tf(Msg::ToastLimitsShort, &[&short]),
                t(Msg::BtnDontShowAgain),
                "notice.hide:oracle_thin",
            );
        }
        self.log_win.push(LogEntry::new(LogKind::Info, title));
        for l in &limits {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!("  • {}", t(crate::app::output::limit_msg(*l))),
            ));
        }
        self.log_win.push(LogEntry::new(
            LogKind::Info,
            format!("  {}", t(Msg::LimitHintOracle)),
        ));
    }

    /// `drain_events`의 `RunEvent::Connected` 가지(220차 T-248 분해 · 동작 불변 · 11-01).
    pub(crate) fn on_ev_connected(
        &mut self,
        description: String,
        dialect: nsql_core::Dialect,
        schema: String,
        database: String,
        autocommit: bool,
    ) {
        // ★ 접속별 자동 커밋(T-77 잔여 · 10-30): 스펙/프로필 `autocommit=`이 러너 모드를 바꿨으면(설정과 다름) 로그 + 토스트로
        //   알린다 — 커밋/롤백 버튼·대기 표식은 여전히 전역 설정을 보므로 사용자가 차이를 알아야 한다.
        if autocommit != self.settings.flag("session.autocommit") {
            let m = tf(
                Msg::StProfileAutocommit,
                &[if autocommit { "on" } else { "off" }],
            );
            self.log_win.push(LogEntry::new(LogKind::Info, m.clone()));
            self.toasts
                .push(toast::ToastKind::Info, description.clone(), m);
        }
        // ★ 연결 기본 작업 단위(⑯): DB 전환 방언 = 접속 직후 DB · 그 밖 = 현재 스키마.
        self.sess.default_unit = if matches!(
            dialect,
            nsql_core::Dialect::Mssql | nsql_core::Dialect::Mysql
        ) {
            Some(database).filter(|d| !d.is_empty())
        } else {
            Some(schema.clone()).filter(|s| !s.is_empty())
        };
        self.sess.current_db = None;
        // 새 접속 = 이 세션에 대한 탭 기억은 전부 버린다(재접속 뒤 옛 스키마를 되밀지 않게 · ㉔).
        let sid = self.sess.id;
        self.tab_unit.retain(|(_, s), _| *s != sid);
        self.sess.disc_path = None;
        self.sess.disc_logged = false;
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
        // ★ T-325 2차(11-04): 방언이 확정되면 SQL 구문 탭의 강조 키워드도 그 방언으로(예약어 표 · 108 §9).
        self.editors.apply_sql_dialect(dialect);
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
        // 접속 성공 = 끝점이 살아 있다(107 §9-2) — 끊겼던 끝점이면 복귀 전파 · 131차: 레지스트리가 멀쩡해도 흘린다
        // (세션 0 → `health_gc`가 기록을 지운 뒤 다시 접속 = 탐색기 헤더 "끊김"만 남던 결함 · 사용자 10-19).
        if let Some(ep) = app::health::ep_text(self.sess.spec.as_ref()) {
            self.health_event(&ep, app::health::HealthEvent::Up);
        }
        self.sess.user_disconnected = false;
        self.sess.idle_closed = false;
        self.sess.desc = description.clone();
        self.sess.key_cache.clear();
        self.sess.touch();
        self.txlog.set_session_label(self.sess.id, &description);
        self.log_win.set_session(&description);
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

    /// `drain_events`의 `RunEvent::Timing` 가지(220차 T-248 분해 · 동작 불변 · 11-01).
    pub(crate) fn on_ev_timing(&mut self, index: usize, timeline: nsql_core::Timeline) {
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

    /// `drain_events`의 `RunEvent::Error` 가지(220차 T-248 분해 · 동작 불변 · 11-01). 돌려주는 값 = 바깥 루프 `continue`(이 사건의 뒤 처리를 건너뜀).
    pub(crate) fn on_ev_error(
        &mut self,
        index: usize,
        line: usize,
        error: nsql_core::DbError,
    ) -> bool {
        // 주입 재조회가 실패했으면 그리드를 원문으로 되돌리고 주입 없이 다시 판정(87 §13 · WITHOUT ROWID 표 등).
        // 🔧 09-28: **재조회가 나가 있을 때만** — 사용자의 다른 문장(예: 자동 커밋 모드의 COMMIT/ROLLBACK 오류)이
        //   같은 탭에서 실패하면 성공해 둔 주입 계획을 "실패"로 지우고 버튼을 영영 껐다.
        if let Some(g) = self.run_grid() {
            if g.inject_pending() {
                g.requery_failed();
            }
        }
        self.txlog
            .select_session(self.sess.id)
            .error(index, error.code, &error.message);
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
            return true;
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
        let (cls, summary) = toast::summarize(err_dialect, error.code, &error.message, &stmt);
        self.sess.status = tf(Msg::StErrorLine, &[&line.to_string(), &summary]);
        // "테이블/뷰 없음"인데 탐색기에는 그 이름이 있다 = 트리가 낡았다 → 그 폴더만 다시(docs/57 T4).
        if cls.class == nsql_core::ErrorClass::NoTable
            && self.settings.flag("meta.refresh_on_missing")
        {
            let name = explorer::missing_name(&error.message);
            let schema = self.meta_default_schema();
            self.explorer
                .note_missing(self.sess.spec.as_ref(), name.as_deref(), schema.as_deref());
        }
        if self.settings.flag("editor.minimap_errors") && line > 0 {
            let ed_line = self.sess.run_line_base + line - 1;
            self.editors
                .set_error_line(self.sess.run_editor, Some(ed_line));
        }
        // ★ T-371(사용자 10-19): 오류 위치로 캐럿 — 드라이버 위치(문장 안 글자)가 있으면 그 자리 · 없으면 문장 시작 줄.
        //   실행한 편집기 탭이 지금 활성일 때만(다른 탭·Output 보는 중엔 옮기지 않는다) · 선택은 만들지 않는다.
        if self.settings.flag("editor.error_goto")
            && self.editors.active_id() == self.sess.run_editor
        {
            let start = self.sess.last_run_starts.get(index).copied().flatten();
            let stmt_len = stmt.chars().count();
            let by_pos =
                app::run::error_caret(self.sess.run_src_base, start, error.position, stmt_len);
            // ★ T-372(사용자 10-20): 구문 오류면 서버 위치 **앞 구간**에서 키워드 오타 후보를 찾아 **첫 번째**로 캐럿 ·
            //   Output에 후보 + 서버 위치 한 줄. 별도 부품(`app/errsuspect.rs` → nsql-script `errsuspect`) · 향상 모드 = 설정 강제 끔.
            let by_suspect = if app::errsuspect::analyze_wanted(
                self.settings.flag("editor.error_analyze"),
                cls.class,
                start,
            ) {
                let t0 = std::time::Instant::now();
                let suspects =
                    self.error_suspects(err_dialect, &stmt, error.position, &error.message);
                if std::env::var_os("NSQL_TRACE_ERRPOS").is_some() {
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        format!(
                            "[errpos] suspects={} {:.2} ms pos={:?} {:?}",
                            suspects.len(),
                            t0.elapsed().as_secs_f64() * 1000.0,
                            error.position,
                            suspects.iter().map(|s| &s.text).collect::<Vec<_>>()
                        ),
                    ));
                }
                if let Some(text) = self.error_suspects_text(
                    &suspects,
                    self.sess.run_src_base,
                    start,
                    error.position,
                    stmt_len,
                ) {
                    let ed = self.sess.run_editor;
                    self.output_push(ed, crate::output::OutKind::Warn, &text);
                }
                suspects.first().and_then(|s| {
                    app::run::error_caret(self.sess.run_src_base, start, Some(s.start), stmt_len)
                })
            } else {
                None
            };
            let target = by_suspect.or(by_pos).or_else(|| {
                (line > 0).then(|| {
                    let buf = self.editors.cur().buf();
                    let l = (self.sess.run_line_base + line - 1)
                        .min(buf.line_count().saturating_sub(1));
                    buf.line_start(l)
                })
            });
            if let Some(ch) = target {
                let mut inv = Invalidations::default();
                self.ed_mut().select_range(ch, ch, &mut inv);
            }
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
            let body = cls
                .object
                .unwrap_or_else(|| error.message.lines().next().unwrap_or("").to_string());
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
        false
    }
}
