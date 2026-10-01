//! App — 툴바 그룹 도크·플로팅·표시 항목(docs/30 ToolDock).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 툴바 우클릭 = 그룹 띄우기/붙이기 · 버튼 표시 여부 토글(설정 `toolbar.hidden`) · 배치 초기화(사용자 09-16 · 09-17).
    pub(crate) fn open_toolbar_menu(&mut self, x: i32, y: i32) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let hidden = self.hidden_toolbar_ids();
        let mut items: Vec<CtxItem> = self
            .tool_dock
            .groups()
            .into_iter()
            .map(|(id, title, floating)| {
                let verb = t(if floating {
                    Msg::MnDockGroup
                } else {
                    Msg::MnFloatGroup
                });
                CtxItem::item(format!("tbg:{id}"), format!("{title} — {verb}"))
                    .with_checked(floating)
            })
            .collect();
        items.extend(TOOLBAR_ITEMS.iter().map(|(id, m)| {
            CtxItem::item(format!("tb:{id}"), t(*m)).with_checked(!hidden.contains(&id.to_string()))
        }));
        items.push(CtxItem::item("tb.reset", t(Msg::MnResetToolbar)));
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(Rect::new(x, y, 0, 0));
        self.status_menu.set_scale(self.scale);
        self.status_menu
            .open_at(x, y, items, host, px(220.0, self.scale));
    }

    pub(crate) fn hidden_toolbar_ids(&self) -> Vec<String> {
        self.settings
            .get("toolbar.hidden")
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// 설정 `toolbar.hidden` → 툴바 버튼 표시 여부.
    pub(crate) fn apply_toolbar_visibility(&mut self) {
        let hidden = self.hidden_toolbar_ids();
        let mut inv = Invalidations::default();
        for (id, _) in TOOLBAR_ITEMS {
            self.tool_dock
                .set_item_visible(id, !hidden.contains(&id.to_string()), &mut inv);
        }
        for f in &self.tool_floats {
            f.redraw();
        }
    }

    /// 설정 `toolbar.layout` → 도크 배치 + 플로팅 창(시작 시 · 설정 창에서 값을 바꿨을 때).
    pub(crate) fn apply_tool_layout_setting(&mut self) {
        let text = self
            .settings
            .get("toolbar.layout")
            .unwrap_or("")
            .to_string();
        let layout = DockLayout::parse(&text);
        // 열린 플로팅 창은 전부 닫고 배치대로 다시 연다(단순 · 드물다).
        for f in &mut self.tool_floats {
            f.close();
        }
        self.tool_floats.clear();
        self.tool_dock.apply_layout(&layout);
        let _ = self.tool_dock.take_actions();
        for (id, x, y) in layout.floating {
            self.pending_float.push((id, Some((x, y))));
        }
        self.layout();
        self.redraw();
    }

    /// 현재 배치를 설정에 저장(그룹 순서 · 플로팅 좌표 — 포터블 배포에서도 `NSQL_HOME` 아래 같은 파일).
    pub(crate) fn save_tool_layout(&mut self) {
        self.tool_layout_dirty = false;
        let text = self.tool_dock.layout().serialize();
        if self.settings.get("toolbar.layout") != Some(text.as_str()) {
            let _ = self.settings.set("toolbar.layout", &text);
            self.persist_settings();
        }
    }

    /// 도크가 보고한 일(떼어 내기 · 배치 변경)을 거둔다.
    pub(crate) fn drain_dock_actions(&mut self) {
        for a in self.tool_dock.take_actions() {
            match a {
                DockAction::Float { id, x, y } => self.pending_float.push((id, Some((x, y)))),
                DockAction::LayoutChanged => self.tool_layout_dirty = true,
                // 행 수가 바뀌었다(다중 행 도크 · 09-19) — 크롬 높이가 달라지니 창 전체를 다시 배치.
                DockAction::Resized => self.layout(),
            }
        }
    }

    /// 플로팅 창 만들기 — `at`: 도크 클라이언트 좌표(커서) 또는 저장된 화면 좌표(`screen=true`).
    pub(crate) fn open_float(&mut self, el: &ActiveEventLoop, gid: &str, at: Option<(i32, i32)>) {
        if self.tool_floats.iter().any(|f| f.group == gid) {
            return;
        }
        let Some(title) = self.tool_dock.title(gid).map(str::to_string) else {
            return;
        };
        let scale = self.scale.max(0.5);
        let (bw, bh) = {
            let h = self.tool_dock.preferred_height() as f32;
            let w = self
                .tool_dock
                .bar(gid)
                .map_or(120.0, |b| b.preferred_width() as f32 / scale);
            (w, h)
        };
        // 저장된 좌표(이미 플로팅으로 표시)면 그대로, 아니면 커서(클라이언트) → 화면 좌표.
        let pos = if let Some(p) = self.tool_dock.floating_pos(gid) {
            Some(p)
        } else {
            at.and_then(|(cx, cy)| {
                let w = self.window.as_ref()?;
                let o = w.inner_position().ok()?;
                Some((
                    o.x + cx - (bw * scale / 2.0) as i32,
                    o.y + cy - (bh * scale / 2.0) as i32,
                ))
            })
        };
        let owner = self.window.clone();
        let win = ToolFloatWin::open(
            el,
            gid,
            &format!("Nexa SQL — {title}"),
            theme::window_theme(self.settings.theme_mode()),
            pos,
            (bw, bh),
            owner.as_deref(),
        );
        let Some(win) = win else { return };
        let (x, y) = win.position().or(pos).unwrap_or((0, 0));
        self.tool_dock.float(gid, x, y);
        let _ = self.tool_dock.take_actions();
        self.tool_layout_dirty = true;
        self.tool_floats.push(win);
        self.layout();
        self.redraw();
    }

    /// 그룹을 도크로 되돌린다(플로팅 창 닫기 포함).
    pub(crate) fn dock_group(&mut self, gid: &str) {
        if let Some(i) = self.tool_floats.iter().position(|f| f.group == gid) {
            self.tool_floats[i].close();
            self.tool_floats.remove(i);
        }
        self.tool_dock.dock(gid);
        let _ = self.tool_dock.take_actions();
        self.tool_layout_dirty = true;
        self.layout();
        self.redraw();
    }

    /// 툴바 초기화(View 메뉴 · 우클릭 · 사용자 09-17) — 그룹 전부 도크 · 정의 순서 · 숨긴 버튼 복원.
    /// ★ 레이아웃 초기화(사용자 09-27 · 팔레트/보기 메뉴): 보조 창 전부 닫기 · 패널 = 처음 실행 상태(탐색기만) · 툴바 초기화 ·
    /// 항상 위 끔 · 창 크기/위치·탐색기 폭 저장값 제거(다음 기동부터 기본 크기). 설정 자체(테마·언어·글꼴)는 건드리지 않는다.
    pub(crate) fn reset_layout(&mut self) {
        self.reset_toolbar();
        self.log_win.close();
        self.txlog_win.close();
        self.sessions_win.close();
        self.license_win.close();
        self.about_win.close();
        self.mem_win.close();
        self.vars_win.close();
        self.colors_win.close();
        self.keys_win.close();
        self.prefs_win.close();
        for (id, on) in [
            ("view.search", self.search.is_visible()),
            ("view.project", self.project_panel.is_visible()),
            ("view.bookmarks", self.bm_panel.is_visible()),
            ("view.outline", self.outline_panel.is_visible()),
            ("view.extensions", self.ext_panel.is_visible()),
            (
                "view.object_details",
                self.settings.flag("explorer.details"),
            ),
            ("view.on_top", self.settings.flag("window.always_on_top")),
        ] {
            if on {
                self.menu_action(id);
            }
        }
        if !self.explorer.is_visible() {
            self.menu_action("view.explorer");
        }
        for k in [
            "explorer.width",
            "window.main_size",
            "window.main_pos",
            "window.login_size",
            "window.login_pos",
            "window.log_size",
            "window.log_pos",
            "window.sessions_size",
            "window.sessions_pos",
            "window.txlog_size",
            "window.txlog_pos",
            "window.prefs_size",
            "window.prefs_pos",
            "window.monitor",
        ] {
            let _ = self.settings.reset(k);
        }
        self.persist_settings();
        self.layout();
        self.sess.status = t(Msg::StLayoutReset).into();
        self.redraw();
    }

    pub(crate) fn reset_toolbar(&mut self) {
        for f in &mut self.tool_floats {
            f.close();
        }
        self.tool_floats.clear();
        self.pending_float.clear();
        self.tool_dock.reset();
        let _ = self.tool_dock.take_actions();
        let _ = self.settings.reset("toolbar.layout");
        let _ = self.settings.reset("toolbar.hidden");
        self.persist_settings();
        self.apply_toolbar_visibility();
        self.tool_layout_dirty = false;
        self.layout();
        self.redraw();
    }

    /// 툴바 = 목적별 그룹(파일 · 실행 · 접속 · 보기) — 아이콘·구분자 계층(nexa-ctl `ToolGroup`). 순서·플로팅은 도크 배치가 기억.
    pub(crate) fn build_tool_dock() -> ToolDock {
        // 아이콘은 글꼴 글리프가 아니라 코드로 그린 마스크(`toolicons.rs` · 사용자 09-14).
        let file = ToolGroup::new(
            "file",
            t(Msg::MnFile),
            vec![
                ToolItem::new("file.new", toolicons::new_script()).tip(t(Msg::TipNew)),
                ToolItem::new("file.open", toolicons::open_file()).tip(t(Msg::TipOpen)),
                ToolItem::separator(),
                ToolItem::new("file.save", toolicons::save_file()).tip(t(Msg::TipSave)),
                // 다른 이름으로 저장 — 사용자가 Material `save_as` 아이콘을 준 09-16(명령 id는 메뉴와 동일).
                ToolItem::new("file.save_as", toolicons::save_as()).tip(t(Msg::TipSaveAs)),
            ],
        );
        let run = ToolGroup::new(
            "run",
            t(Msg::MnRun),
            vec![
                ToolItem::new("run.statement", toolicons::run_statement())
                    .tip(t(Msg::TipRunStatement)),
                ToolItem::new("run.all", toolicons::run_all()).tip(t(Msg::TipRunAll)),
                ToolItem::new("run.stop", toolicons::fetch_stop()).tip(t(Msg::TipRunStop)),
                ToolItem::separator(),
                // 트랜잭션(DR-30): 대기 문장이 있을 때만 활성 · Material check/undo.
                ToolItem::new("run.commit", toolicons::commit())
                    .tip(t(Msg::TipCommit))
                    .disabled(),
                ToolItem::new("run.rollback", toolicons::rollback())
                    .tip(t(Msg::TipRollback))
                    .disabled(),
                // 트랜잭션 로그(docs/44 §5): 항상 활성 · 수동 모드면 배지 = 대기 수 · 색 = 가장 심각한 문장 종류.
                ToolItem::new("tx.log", toolicons::tx_log()).tip(t(Msg::TipTxLog)),
            ],
        );
        let conn = ToolGroup::new(
            "conn",
            t(Msg::TbGroupConn),
            vec![
                ToolItem::new("conn.toggle", toolicons::connect()).tip(t(Msg::TipConnect)),
                ToolItem::new("conn.disconnect", toolicons::disconnect())
                    .tip(t(Msg::TipDisconnect))
                    .disabled(),
                ToolItem::new("conn.sessions", toolicons::sessions()).tip(t(Msg::TipSessions)),
            ],
        );
        // ★ 탭 연결정보(사용자 09-28): 지금 탭에 묶인 서버를 글로(프로필 이름 · 없으면 계정@호스트) · 클릭 = 탭 표식과 같은 메뉴로 바꾸기.
        let tabconn = ToolGroup::new(
            "tabconn",
            t(Msg::TbGroupTabConn),
            vec![
                ToolItem::text("sess.tab", "—")
                    .with_dropdown()
                    .tip(t(Msg::TipTabConn)),
                // ★ 작업 단위(사용자 10-01 ⑫ · 라벨 없이 값만): Oracle 스키마(고정) · SQL Server/MySQL 현재 DB(드롭다운 = USE) · PG DB(고정).
                ToolItem::text("sess.db", "—")
                    .with_dropdown()
                    .tip(t(Msg::TipTabDb))
                    .disabled(),
            ],
        );
        let view = ToolGroup::new(
            "view",
            t(Msg::MnView),
            vec![ToolItem::new("view.log", toolicons::log()).tip(t(Msg::TipLog))],
        )
        .align_right();
        let mut dock = ToolDock::new(vec![file, run, conn, tabconn, view]);
        dock.set_icon_size(18);
        dock
    }
}
