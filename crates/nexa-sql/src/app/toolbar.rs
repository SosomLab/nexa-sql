//! App — 툴바 그룹 도크·플로팅·표시 항목(docs/30 ToolDock).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 커서 아래의 툴바 그룹(도크에 붙은 것) — 우클릭 메뉴의 "영역".
    fn toolbar_group_at(&self, p: Point) -> Option<String> {
        self.tool_dock
            .groups()
            .into_iter()
            .filter(|(_, _, floating)| !floating)
            .map(|(id, _, _)| id)
            .find(|id| {
                self.tool_dock
                    .bar(id)
                    .is_some_and(|b| b.bounds().contains(p))
            })
    }

    /// 커서 아래의 툴바 버튼(숨길 수 있는 것만 · `TOOLBAR_ITEMS`).
    fn toolbar_item_at(&self, p: Point) -> Option<&'static str> {
        TOOLBAR_ITEMS
            .iter()
            .map(|(id, _)| *id)
            .find(|id| self.tool_dock.item_rect(id).is_some_and(|r| r.contains(p)))
    }

    /// ★ 툴바 우클릭(사용자 10-04): **영역(커서 아래 그룹)의 추가 메뉴 → 구분선 → 공통 메뉴**.
    ///   영역 = 그 그룹과 관련된 명령(버튼에 없는 것) + 누른 버튼 숨기기 + 그룹 띄우기/붙이기 ·
    ///   공통 = 툴바 설정…(보기·순서 편집 창으로 바로) · 툴바 배치 초기화. 버튼별 표시 목록은 편집 창으로 옮겼다.
    pub(crate) fn open_toolbar_menu(&mut self, x: i32, y: i32) {
        let items = self.toolbar_menu_items(Point { x, y });
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(Rect::new(x, y, 0, 0));
        self.status_menu.set_scale(self.scale);
        // 대상을 가리지 않게(61 §2-2-b): 툴바 띠 바로 아래에.
        let avoid = self.tool_dock.bounds();
        self.status_menu
            .open_beside(x, y, avoid, items, host, px(220.0, self.scale));
    }

    /// 우클릭 메뉴 항목(순수에 가깝게 — 위치만 받는다 · 시험·덤프용으로 분리).
    pub(crate) fn toolbar_menu_items(&self, p: Point) -> Vec<nexa_ctl::controls::ctxmenu::CtxItem> {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let mut items: Vec<CtxItem> = Vec::new();
        if let Some(gid) = self.toolbar_group_at(p) {
            for (cmd, m) in toolbar_area_commands(&gid) {
                items.push(CtxItem::item(format!("tbcmd:{cmd}"), t(*m)));
            }
            // 누른 버튼의 표시 여부(체크 = 보임 · 끄면 숨김 — 다시 켜기는 툴바 설정에서).
            if let Some(id) = self.toolbar_item_at(p) {
                let label = TOOLBAR_ITEMS
                    .iter()
                    .find(|(i, _)| *i == id)
                    .map_or(id, |(_, m)| crate::order_win::without_shortcut(t(*m)));
                items.push(CtxItem::item(format!("tb:{id}"), label).with_checked(true));
            }
            let floating = self.tool_dock.is_floating(&gid);
            let title = self.tool_dock.title(&gid).unwrap_or(&gid).to_string();
            let verb = t(if floating {
                Msg::MnDockGroup
            } else {
                Msg::MnFloatGroup
            });
            items.push(CtxItem::item(
                format!("tbg:{gid}"),
                format!("{title} — {verb}"),
            ));
            items.push(CtxItem::Separator);
        }
        items.push(CtxItem::item("tb.settings", t(Msg::MnToolbarSettings)));
        items.push(CtxItem::item("tb.reset", t(Msg::MnResetToolbar)));
        items
    }

    /// ★ 상태바 우클릭(사용자 10-04 · 툴바와 같은 구조): **항목(커서 아래 칸)의 메뉴 → 구분선 → 공통 메뉴**.
    ///   항목 = 그 칸과 관련된 명령 + "숨기기"(잠긴 칸 제외 · 다시 켜기는 상태바 설정에서) · 공통 = 상태바 설정… · 항목 기본값.
    pub(crate) fn open_statusbar_menu(&mut self, x: i32, y: i32) {
        let items = self.statusbar_menu_items(Point { x, y });
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(Rect::new(x, y, 0, 0));
        self.status_menu.set_scale(self.scale);
        // 대상을 가리지 않게(61 §2-2-b): 상태바는 창 맨 아래 — 아래에 자리가 없으니 띠 바로 **위**로 열린다.
        let avoid = self.status_bar_rect;
        self.status_menu
            .open_beside(x, y, avoid, items, host, px(220.0, self.scale));
    }

    pub(crate) fn statusbar_menu_items(
        &self,
        p: Point,
    ) -> Vec<nexa_ctl::controls::ctxmenu::CtxItem> {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let mut items: Vec<CtxItem> = Vec::new();
        let seg = self
            .status_seg_rects
            .iter()
            .find(|(_, r)| r.contains(p))
            .map(|(id, _)| *id);
        if let Some(id) = seg {
            for (cmd, m) in crate::statusbar::area_commands(id) {
                items.push(CtxItem::item(format!("tbcmd:{cmd}"), t(*m)));
            }
            if !crate::statusbar::LOCKED.contains(&id) {
                let name = t(crate::statusbar::label(id, None));
                items.push(CtxItem::item(
                    format!("sb.hide:{id}"),
                    tf(Msg::MnStatusHide, &[name]),
                ));
            }
            if !items.is_empty() {
                items.push(CtxItem::Separator);
            }
        }
        items.push(CtxItem::item("sb.settings", t(Msg::MnStatusSettings)));
        items.push(CtxItem::item("sb.reset", t(Msg::MnStatusReset)));
        items
    }

    /// 지금 툴바 상태(도크 순서 + 숨긴 버튼) → 편집 창이 읽는 값(`order` 문법).
    pub(crate) fn toolbar_order_value(&self) -> String {
        let hidden = self.hidden_toolbar_ids();
        let order = self.tool_dock.layout().order;
        let blocks: Vec<nexa_ctl::order::OrderBlock> = order
            .iter()
            .filter_map(|gid| {
                let (_, items) = TOOLBAR_BLOCKS.iter().find(|(b, _)| b == gid)?;
                Some((
                    gid.clone(),
                    true,
                    items
                        .iter()
                        .map(|i| (i.to_string(), !hidden.contains(&i.to_string())))
                        .collect(),
                ))
            })
            .collect();
        nexa_ctl::order::serialize(&blocks)
    }

    /// 편집 창의 값 → 툴바에 적용: 그룹 순서 = 도크 순서(행·플로팅은 그대로) · 자식 체크 = `toolbar.hidden`. 저장까지.
    pub(crate) fn apply_toolbar_order(&mut self, value: &str) {
        let blocks = toolbar_order_blocks(value);
        let hidden: Vec<&str> = blocks
            .iter()
            .flat_map(|(_, _, items)| items.iter())
            .filter(|(_, vis)| !*vis)
            .map(|(id, _)| id.as_str())
            .collect();
        if hidden.is_empty() {
            let _ = self.settings.reset("toolbar.hidden");
        } else {
            let _ = self.settings.set("toolbar.hidden", &hidden.join(","));
        }
        // 순서만 바꾼다 — 그룹마다 지금의 행과 플로팅 좌표는 지킨다.
        let old = self.tool_dock.layout();
        let row_of = |id: &str| {
            old.order
                .iter()
                .position(|o| o == id)
                .and_then(|i| old.rows.get(i).copied())
                .unwrap_or(0)
        };
        let order: Vec<String> = blocks.iter().map(|(id, _, _)| id.clone()).collect();
        let rows = order.iter().map(|id| row_of(id)).collect();
        self.tool_dock.apply_layout(&DockLayout {
            order,
            rows,
            floating: old.floating,
        });
        let _ = self.tool_dock.take_actions();
        self.apply_toolbar_visibility();
        // 기본 배치(정의 순 · 한 행 · 플로팅 없음)면 줄을 남기지 않는다(상태바 편집과 같은 규칙 — 기본값 = 빈 값).
        if self.tool_dock.layout().serialize() == TOOLBAR_GROUP_IDS.join(",") {
            let _ = self.settings.reset("toolbar.layout");
            self.tool_layout_dirty = false;
        } else {
            self.save_tool_layout();
        }
        self.persist_settings();
        self.layout();
        self.redraw();
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
        self.sync_toolbar_tips();
        for f in &self.tool_floats {
            f.redraw();
        }
    }

    /// 툴바 버튼 툴팁 = 이름 + **지금 키맵의 단축키**(OS 표기 · 사용자가 바꾼 값 · 10-04) — 문구에 박힌 `(Ctrl+…)`는 떼고
    /// 다시 붙인다(맥에서 `Ctrl`로 보이던 것 · 단축키를 바꿔도 옛 글이 남던 것). 상태에 따라 바뀌는 툴팁(해제 · 트랜잭션 로그 등)은
    /// 각자의 동기화가 뒤에 덮어쓴다.
    pub(crate) fn sync_toolbar_tips(&mut self) {
        for (id, m) in TOOLBAR_ITEMS {
            let name = crate::order_win::without_shortcut(t(*m));
            let sc = self.keymap.display_of(id);
            let tip = if sc.is_empty() {
                name.to_string()
            } else {
                format!("{name} ({sc})")
            };
            self.tool_dock.set_item_tip(id, &tip);
        }
    }

    /// 설정 `toolbar.layout` → 도크 배치 + 플로팅 창(시작 시 · 설정 창에서 값을 바꿨을 때).
    pub(crate) fn apply_tool_layout_setting(&mut self) {
        let text = self
            .settings
            .get("toolbar.layout")
            .unwrap_or("")
            .to_string();
        let mut layout = DockLayout::parse(&text);
        // 새 그룹 "edit"(10-06)이 저장된 배치에 없으면 "file" 바로 뒤에(도크는 모르는 그룹을 맨 뒤에 붙인다).
        if !layout.order.iter().any(|g| g == "edit") {
            let at = layout
                .order
                .iter()
                .position(|g| g == "file")
                .map_or(0, |i| i + 1);
            layout.order.insert(at, "edit".to_string());
            // `rows`는 `order`와 나란하다 — 비어 있지 않으면 같은 자리에 "file"의 행을 넣는다.
            if !layout.rows.is_empty() {
                let row = layout.rows.get(at.saturating_sub(1)).copied().unwrap_or(0);
                layout.rows.insert(at.min(layout.rows.len()), row);
            }
        }
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
        self.order_win.close();
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
        // ★ 편집 그룹(파일 옆 · 사용자 10-06): 실행 취소 · 다시 실행(편집기 명령 그대로).
        let edit = ToolGroup::new(
            "edit",
            t(Msg::MnEdit),
            vec![
                ToolItem::new("edit.undo", toolicons::undo()).tip(t(Msg::MnUndo)),
                ToolItem::new("edit.redo", toolicons::redo()).tip(t(Msg::MnRedo)),
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
        let mut dock = ToolDock::new(vec![file, edit, run, conn, tabconn, view]);
        dock.set_icon_size(18);
        dock
    }
}

/// 툴바 편집 창의 가상 키(설정 키가 아니다 — `toolbar.layout`(그룹 순서) + `toolbar.hidden`(버튼 표시)을 한 값으로 묶어 오간다).
pub(crate) const TOOLBAR_ORDER_KEY: &str = "toolbar.order";

/// 툴바 그룹과 그 안의 숨길 수 있는 버튼(정의 순 = `build_tool_dock`의 순서 · 글자 항목만 있는 그룹은 자식 없음).
pub(crate) const TOOLBAR_BLOCKS: nexa_ctl::order::OrderDefs = &[
    (
        "file",
        &["file.new", "file.open", "file.save", "file.save_as"],
    ),
    ("edit", &["edit.undo", "edit.redo"]),
    (
        "run",
        &[
            "run.statement",
            "run.all",
            "run.stop",
            "run.commit",
            "run.rollback",
            "tx.log",
        ],
    ),
    ("conn", &["conn.toggle", "conn.disconnect", "conn.sessions"]),
    ("tabconn", &[]),
    ("view", &["view.log"]),
];

/// 그룹은 숨기지 않는다(버튼 단위로만) — 편집 창에서 그룹 체크 잠금.
pub(crate) const TOOLBAR_GROUP_IDS: &[&str] = &["file", "edit", "run", "conn", "tabconn", "view"];

pub(crate) fn toolbar_order_blocks(value: &str) -> Vec<nexa_ctl::order::OrderBlock> {
    nexa_ctl::order::parse(TOOLBAR_BLOCKS, &[], value)
        .into_iter()
        .map(|(id, _, items)| (id, true, items))
        .collect()
}

pub(crate) fn toolbar_order_setting(blocks: &[nexa_ctl::order::OrderBlock]) -> String {
    nexa_ctl::order::normalize(TOOLBAR_BLOCKS, &[], &nexa_ctl::order::serialize(blocks))
}

pub(crate) fn toolbar_order_label(block: &str, item: Option<&str>) -> Msg {
    match item {
        Some(i) => TOOLBAR_ITEMS
            .iter()
            .find(|(id, _)| *id == i)
            .map_or(Msg::MnView, |(_, m)| *m),
        None => match block {
            "file" => Msg::MnFile,
            "edit" => Msg::MnEdit,
            "run" => Msg::MnRun,
            "conn" => Msg::TbGroupConn,
            "tabconn" => Msg::TbGroupTabConn,
            _ => Msg::MnView,
        },
    }
}

/// 영역(그룹)별 추가 메뉴 — 그 영역과 관련 있지만 버튼에는 없는 명령(명령 id · 라벨).
pub(crate) fn toolbar_area_commands(gid: &str) -> &'static [(&'static str, Msg)] {
    match gid {
        "file" => &[
            ("file.run_file", Msg::MnRunFile),
            ("file.close_tab", Msg::MnCloseTab),
        ],
        "edit" => &[
            ("edit.soft_undo", Msg::MnSoftUndo),
            ("edit.soft_redo", Msg::MnSoftRedo),
        ],
        "run" => &[
            ("run.statement_new_tab", Msg::MnRunStatementNewTab),
            ("run.explain", Msg::MnExplain),
        ],
        "conn" | "tabconn" => &[
            ("session.info", Msg::MnSessInfo),
            ("view.variables", Msg::MnVariables),
        ],
        "view" => &[
            ("view.txlog", Msg::MnTxLogWindow),
            ("view.sessions", Msg::MnSessManager),
            ("view.memory", Msg::MnMemoryWindow),
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 편집 창 정의 = 실제 툴바와 같아야 한다: 숨길 수 있는 버튼 전부가 정확히 한 그룹에 · 그룹 id = 잠금 목록.
    #[test]
    fn toolbar_blocks_cover_every_hideable_item_once() {
        let in_blocks: Vec<&str> = TOOLBAR_BLOCKS
            .iter()
            .flat_map(|(_, items)| items.iter().copied())
            .collect();
        for (id, _) in TOOLBAR_ITEMS {
            assert_eq!(in_blocks.iter().filter(|i| *i == id).count(), 1, "{id}");
        }
        assert_eq!(in_blocks.len(), TOOLBAR_ITEMS.len());
        let groups: Vec<&str> = TOOLBAR_BLOCKS.iter().map(|(b, _)| *b).collect();
        assert_eq!(groups, TOOLBAR_GROUP_IDS);
        // 실제 도크의 그룹 순서·소속과 같다.
        let dock = App::build_tool_dock();
        let dock_groups: Vec<String> = dock.groups().into_iter().map(|(id, _, _)| id).collect();
        assert_eq!(dock_groups, groups);
        for (b, items) in TOOLBAR_BLOCKS {
            for i in *items {
                assert_eq!(dock.group_of(i), Some(*b), "{i}");
                assert_ne!(
                    toolbar_order_label(b, Some(i)),
                    Msg::MnView,
                    "{i}: 라벨 없음"
                );
            }
        }
    }

    #[test]
    fn toolbar_order_value_round_trip_and_groups_never_hidden() {
        assert_eq!(
            toolbar_order_setting(&toolbar_order_blocks("")),
            "",
            "기본 = 빈 값"
        );
        // 그룹 순서 바꿈 + 버튼 하나 숨김 + 그룹 숨김 시도(무시).
        let v = "view:0[view.log:1]|file:1[file.new:1,file.open:0,file.save:1,file.save_as:1]";
        let b = toolbar_order_blocks(v);
        assert_eq!(b[0].0, "view");
        assert!(b.iter().all(|(_, vis, _)| *vis), "그룹은 늘 표시");
        let hidden: Vec<&str> = b
            .iter()
            .flat_map(|(_, _, it)| it.iter())
            .filter(|(_, v)| !*v)
            .map(|(i, _)| i.as_str())
            .collect();
        assert_eq!(hidden, ["file.open"]);
        let s = toolbar_order_setting(&b);
        assert_eq!(toolbar_order_setting(&toolbar_order_blocks(&s)), s);
        // 영역 메뉴: 아는 그룹마다 하나 이상.
        for g in TOOLBAR_GROUP_IDS {
            assert!(!toolbar_area_commands(g).is_empty(), "{g}");
        }
    }
}
