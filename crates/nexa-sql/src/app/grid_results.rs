//! App — 결과 그리드·결과 탭·페치·그리드 편집 적용(docs/43·87).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 활성 편집기 탭의 결과 그리드 전부에서 "더 있음"을 내린다(세션이 바뀌어 이어 받기가 뜻을 잃을 때).
    pub(crate) fn freeze_tab_results(&mut self) {
        let tab = self.editors.active_id();
        self.freeze_results_of(tab);
    }

    pub(crate) fn freeze_results_of(&mut self, tab: u64) {
        if tab == self.panel_editor {
            self.grid.set_more(false);
            for t in &mut self.panel.tabs {
                t.grid.set_more(false);
            }
        } else if let Some(p) = self.panels.get_mut(&tab) {
            for t in &mut p.tabs {
                t.grid.set_more(false);
            }
        }
    }

    /// 그리드가 메뉴로 만든 복사 텍스트를 OS 클립보드로.
    pub(crate) fn after_grid_event(&mut self) {
        // 사건 없이 그림만 바뀐 것(hover 모드 깔때기 · 10-06) → 다시 그리기.
        if self.grid.take_dirty() {
            self.redraw();
        }
        // ★ 인라인 조건 입력란 Enter(사용자 10-06): 감싼 SQL을 **같은 탭**에서 다시 실행(실행 오류는 그대로 토스트) · 게이트가
        //   닫혀 있으면 상태줄만.
        // 그리드 안 글 상자(조건 바·값 목록 검색)의 우클릭 메뉴 = 복사·잘라내기·붙여넣기(10-06).
        if let Some(a) = self.grid.take_text_edit_ctx() {
            self.clip_action(a);
        }
        if let Some(sql) = self.grid.take_cond_run() {
            if self.gate_open() {
                self.log_win.push(LogEntry::new(LogKind::Info, sql.clone()));
                self.grid.set_source_sql(&sql);
                self.refresh_result();
            } else {
                self.sess.status = t(Msg::StGeRequeryBusy).into();
                self.redraw();
            }
        }
        if let Some(e) = self.grid.take_error() {
            self.sess.status = tf(Msg::StFilterError, &[&e]);
            self.log_win
                .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
        }
        if let Some((text, n)) = self.grid.take_copy() {
            if clipboard::write_text(&text) {
                self.sess.status = tf(Msg::StCopied, &[&n.to_string()]);
            } else {
                self.sess.status = t(Msg::ErrClipboard).into();
            }
        }
        if let Some(kind) = self.grid.take_pending_sql() {
            self.begin_sql_copy(kind);
        }
        // 필터 메뉴 "필터로 서버 재조회"(T-181): 조회용 Query를 새 결과 탭으로(문지기 · 세션 바쁘면 보내지 않음).
        if let Some(sql) = self.grid.take_requery() {
            self.run_in_fresh_tab(sql);
        }
        // 셀 메뉴 "참조 행 보기"(T-180 ⑥): 외래 키 → 부모 테이블을 기본 키로 조회(새 결과 탭 · 문지기).
        if let Some((row, col)) = self.grid.take_follow() {
            self.follow_fk(row, &col);
        }
        // 열 머리 "객체 탐색기에서 보기"(T-180 ⑤): 출처 테이블 + 그 열 → 탐색기 찾기(편집기 링크·F4와 같은 길).
        if let Some(col) = self.grid.take_reveal() {
            if let Some(table) = self.grid.reveal_table() {
                self.reveal_table_member(&table, Some(col));
            }
        }
        // ★ 그리드 필터 "포함…"(T-181): 팔레트 입력 → `grid.filter:<열>` → `Grid::add_filter`.
        if let Some((col, op, initial)) = self.grid.take_filter_prompt() {
            let anchor = Some(self.grid.bounds);
            let placeholder = match op {
                crate::grid::FilterOp::StartsWith => Msg::PalFilterStarts,
                crate::grid::FilterOp::Gt | crate::grid::FilterOp::Ge => Msg::PalFilterGt,
                crate::grid::FilterOp::Lt | crate::grid::FilterOp::Le => Msg::PalFilterLt,
                crate::grid::FilterOp::Between => Msg::PalFilterBetween,
                crate::grid::FilterOp::Regex => Msg::PalFilterRegex,
                crate::grid::FilterOp::In => Msg::PalFilterIn,
                _ => Msg::PalFilterContains,
            };
            self.palette.open_prompt_at(
                &format!("grid.filter:{col}:{}", op.code()),
                t(placeholder),
                &initial,
                anchor,
            );
            self.ime_refresh();
            self.redraw();
        }
        // 결과 도구줄(docs/43 §4-2): 추가/전체/건수 · 새로고침 · SQL 보기.
        if let Some(req) = self.grid.take_fetch_request() {
            self.send_fetch(req);
        }
        if self.grid.take_cancel_request() {
            self.sess.worker.cancel_fetch();
            self.sess.status = t(Msg::StFetchCancelling).into();
        }
        if self.grid.take_refresh() {
            self.refresh_result();
        }
        if let Some(kind) = self.grid.take_pending_view() {
            self.begin_view_sql(kind);
        }
        // ★ 그리드 편집(docs/87): 키 조회 · 적용 · 미리보기 · 상태.
        for req in self.grid.take_edit_requests() {
            match req {
                grid::EditRequest::Status(s) => {
                    self.sess.status = s;
                    self.redraw();
                }
                grid::EditRequest::Preview { title, text } => {
                    self.sqlprev_plain = Some((title, text));
                }
                grid::EditRequest::ViewCell(v) => {
                    self.sqlprev_value = Some(v);
                }
                grid::EditRequest::ClipboardPaste => {
                    if let Some(text) = clipboard::read_text() {
                        self.grid.live_paste(&text);
                    } else {
                        self.sess.status = t(Msg::ErrClipboard).into();
                    }
                    self.redraw();
                }
                grid::EditRequest::NeedKeys { table } => self.grid_edit_keys(table),
                grid::EditRequest::Requery { sql } => {
                    // ★ 숨은 열 주입 재조회(87 §13 · T-231): 출처 문장을 바꿔 같은 탭에서 다시 실행 — 게이트가 닫혀 있으면 주입 없이 판정.
                    if self.gate_open() {
                        self.log_win.push(LogEntry::new(LogKind::Info, sql.clone()));
                        self.grid.set_source_sql(&sql);
                        self.refresh_result();
                    } else {
                        // 🔧 보내지 못한 것은 실패가 아니다(09-28): 표식 없이 되돌려 버튼이 다시 켜지게.
                        self.grid.requery_aborted();
                    }
                }
                grid::EditRequest::Apply {
                    table,
                    stmts,
                    preview,
                    refetch,
                } => self.grid_edit_apply(&table, stmts, &preview, refetch),
            }
        }
    }

    /// 편집 열 명세 = 메타 저장소(카탈로그 컬럼 · 길이·NOT NULL·기본값 · 79 단일 원천)에서 즉시 — 없으면 결과 타입 이름만으로(그리드가 이미 넣음).
    fn grid_edit_specs(&mut self, table: &str) {
        let dialect = self.sess.dialect;
        let (schema, name) = nsql_io::split_table(dialect, table);
        let specs: Vec<nexa_ctl::gridedit::CellSpec> = {
            let (names, snap) = self.explorer.meta_view(self.sess.spec.as_ref());
            let Some(id) = snap.lookup(names, schema.as_deref(), &name) else {
                return;
            };
            match snap.columns(id) {
                nsql_run::meta::ColState::Loaded { cols, .. } => cols
                    .iter()
                    .map(|c| {
                        let ty = names.get(c.data_type);
                        let mut kind = nexa_ctl::gridedit::CellKind::from_type_name(ty);
                        if dialect == Dialect::Oracle && kind == nexa_ctl::gridedit::CellKind::Date
                        {
                            kind = nexa_ctl::gridedit::CellKind::DateTime; // Oracle DATE = 시각 포함
                        }
                        let mut sp =
                            nexa_ctl::gridedit::CellSpec::new(names.get(c.name).to_string(), kind);
                        sp.max_len = nexa_ctl::gridedit::CellSpec::max_len_from_type_name(ty);
                        sp.nullable = c.nullable.unwrap_or(true);
                        sp.default = c
                            .default
                            .map(|d| names.get(d).to_string())
                            .filter(|d| !d.trim().is_empty());
                        sp
                    })
                    .collect(),
                _ => return,
            }
        };
        self.grid.set_col_specs(&specs);
    }

    /// 편집 키(PK/UK): 키 캐시 → 없으면 `Cmd::Keys`(SQL 복사와 같은 길 · 41 D-66).
    fn grid_edit_keys(&mut self, table: String) {
        self.grid_edit_specs(&table);
        if let Some(info) = self.sess.key_cache.get(&table) {
            let info = info.clone();
            self.grid.set_keys(info.as_ref());
            return;
        }
        let pass = match self.sess.pass() {
            Some(p) if self.sess.edit_wait.is_none() => p,
            _ => {
                // 세션이 바쁘면(실행이 막 끝나는 중) 요청을 되돌려 두고 다음 깨어남에 다시(`about_to_wait`).
                self.grid.requeue_keys(table);
                return;
            }
        };
        let (schema, tname) = nsql_io::split_table(self.sess.dialect, &table);
        self.sess.edit_wait = Some(self.grid_tab);
        self.sess.aux += 1;
        self.sess.submit(
            pass,
            worker::Cmd::Keys {
                key: table,
                schema,
                table: tname,
            },
        );
    }

    /// 변경 적용 → 워커 `Cmd::Apply`(gate · 한 트랜잭션 · 결과는 `ConnOutcome::Applied`).
    fn grid_edit_apply(
        &mut self,
        table: &str,
        stmts: Vec<nsql_run::ApplyStmt>,
        preview: &str,
        refetch: Vec<nsql_core::ExecRequest>,
    ) {
        let Some(pass) = self.gate_pass() else {
            self.grid
                .apply_done(0, Some((0, t(Msg::StRunning).to_string())));
            return;
        };
        self.log_win.push(LogEntry::new(
            LogKind::Info,
            format!("{} {} · {}", t(Msg::MnGeApply), table, stmts.len()),
        ));
        self.log_win
            .push(LogEntry::new(LogKind::Send, preview.to_string()));
        self.sess.edit_apply = Some(self.grid_tab);
        self.sess.aux += 1;
        self.sess.status = t(Msg::StRunning).into();
        // 행 단위 재조회는 설정이 rows일 때만 보낸다(requery/local = 전체 재조회/그대로).
        let refetch = if self.settings.get("grid.edit_refresh") == Some("rows") {
            refetch
        } else {
            Vec::new()
        };
        self.sess.submit(
            pass,
            worker::Cmd::Apply {
                key: self.grid_tab,
                stmts,
                refetch,
            },
        );
        self.redraw();
    }

    /// 값 보기 창의 "파일에서 넣기"(87 §5): 이진 열이거나 UTF-8이 아니면 바이트 · 아니면 글 — 그리드 셀 + 창 둘 다 갱신.
    pub(crate) fn cell_load_file(&mut self, path: &std::path::Path) {
        if !self.lic_gate(nsql_license::Feature::LobImage) {
            return;
        }
        let Some((row, col, binary)) = self.sqlprev_win.value_cell() else {
            return;
        };
        match self.cell_load_into(row, col, binary, path) {
            Ok((bytes, text, n)) => {
                self.sqlprev_win.replace_value(bytes, text);
                self.sess.status = tf(
                    Msg::StCellLoaded,
                    &[&path.to_string_lossy(), &n.to_string()],
                );
                self.sqlprev_win.set_note(self.sess.status.clone());
                self.after_grid_event();
            }
            Err(m) => self.sqlprev_win.set_note(m),
        }
        self.redraw();
    }

    /// 자체 시험 길: 표시 좌표의 셀에 파일을 넣는다(값 창 없이).
    pub(crate) fn cell_load_file_at(&mut self, di: usize, pos: usize, path: &std::path::Path) {
        let Some((row, col, binary)) = self.grid.cell_at_for_test(di, pos) else {
            return;
        };
        match self.cell_load_into(row, col, binary, path) {
            Ok((_, _, n)) => {
                self.sess.status = tf(
                    Msg::StCellLoaded,
                    &[&path.to_string_lossy(), &n.to_string()],
                );
            }
            Err(m) => self.sess.status = m,
        }
    }

    /// 파일 → 셀(공통): (이진, 글, 바이트 수).
    fn cell_load_into(
        &mut self,
        row: nexa_ctl::gridedit::RowRef,
        col: usize,
        binary: bool,
        path: &std::path::Path,
    ) -> Result<CellLoad, String> {
        let data = std::fs::read(path).map_err(|e| tf(Msg::ErrLogFile, &[&e.to_string()]))?;
        let n = data.len();
        let label = path
            .file_name()
            .map_or_else(|| "file".to_string(), |f| f.to_string_lossy().into_owned());
        if !binary {
            if let Ok(text) = String::from_utf8(data.clone()) {
                self.grid.set_cell_value(row, col, Some(text.clone()))?;
                return Ok((None, Some(text), n));
            }
        }
        self.grid.set_cell_bytes(row, col, data.clone(), &label)?;
        Ok((Some(data), None, n))
    }

    /// 연속 클릭 정책 → nexa-ctl 전역(편집기·셀 편집기·패널 상자 공통 · 사용자 09-26).
    pub(crate) fn apply_click_policy(&mut self) {
        let dbl =
            nexa_ctl::ClickAction::parse(self.settings.get("editor.dblclick").unwrap_or("word"))
                .unwrap_or(nexa_ctl::ClickAction::Word);
        let triple = nexa_ctl::ClickAction::parse(
            self.settings.get("editor.triple_click").unwrap_or("line"),
        )
        .unwrap_or(nexa_ctl::ClickAction::Line);
        nexa_ctl::set_click_policy(
            dbl,
            triple,
            self.settings.flag("editor.dblclick_underscore"),
        );
    }

    /// 편집 설정 → 전 그리드(docs/87 §8).
    pub(crate) fn apply_grid_edit_cfg(&mut self) {
        let cfg = grid::EditCfg {
            on: self.settings.flag("grid.edit"),
            empty_as_null: self.settings.get("grid.edit_empty") != Some("empty"),
            paste_max: self.settings.int("grid.paste_max_rows").max(1) as usize,
            hidden_keys: self.settings.flag("grid.edit_hidden_keys"),
            rowid: self.settings.flag("grid.edit_rowid"),
            all_cols: self.settings.flag("grid.edit_all_cols"),
            concurrency: match self.settings.get("grid.edit_concurrency") {
                Some("key_old") => gridedit_sql::Concurrency::KeyOld,
                Some("all_old") => gridedit_sql::Concurrency::AllOld,
                _ => gridedit_sql::Concurrency::Key,
            },
        };
        self.all_grids().for_each(|g| g.set_edit_cfg(cfg.clone()));
        // 정규식 필터 값 목록 상한(사용자 09-30 · `grid.filter_list_max`) — 같은 적용 길에 얹는다(시작 · grid 편집 키 변경).
        let list_max = self.settings.int("grid.filter_list_max").max(1) as usize;
        self.all_grids()
            .for_each(|g| g.set_filter_list_max(list_max));
        let pick_max = self.settings.int("grid.filter_pick_max").clamp(5, 200) as usize;
        self.all_grids()
            .for_each(|g| g.set_filter_pick_max(pick_max));
        let strip = self.settings.flag("grid.filter_strip");
        self.all_grids().for_each(|g| g.set_filter_strip(strip));
        // ★ 결과 필터 사용 여부 · 깔때기 표시 방법 · 값 목록 상한(T-181 후속 · 10-06).
        let enabled = self.settings.flag("grid.filter_enabled");
        self.all_grids().for_each(|g| g.set_filter_enabled(enabled));
        let funnel = self
            .settings
            .get("grid.filter_funnel")
            .unwrap_or("always")
            .to_string();
        self.all_grids().for_each(|g| g.set_filter_funnel(&funnel));
        let values_max = self.settings.int("grid.filter_values_max").clamp(20, 5000) as usize;
        self.all_grids()
            .for_each(|g| g.set_filter_values_max(values_max));
        let popup_rows = self.settings.int("grid.filter_popup_rows").clamp(3, 40) as usize;
        self.all_grids()
            .for_each(|g| g.set_filter_popup_rows(popup_rows));
        let scope = self
            .settings
            .get("grid.filter_values_scope")
            .unwrap_or("others")
            .to_string();
        self.all_grids()
            .for_each(|g| g.set_filter_values_scope(&scope));
        let cond = self.settings.flag("grid.condition_bar");
        self.all_grids().for_each(|g| g.set_condition_bar(cond));
        let tpl = self.settings.flag("grid.cond_drop_template");
        self.all_grids().for_each(|g| g.set_cond_drop_template(tpl));
        let lines = self.settings.int("grid.cond_max_lines").clamp(2, 12) as usize;
        self.all_grids().for_each(|g| g.set_cond_max_lines(lines));
        // 조건 바 완성 = 편집기 인텔리센스와 같은 기준(사용자 10-07).
        let pass = self.settings.flag("intel.key_passthrough");
        let min_chars = self.settings.int("intel.min_chars").clamp(1, 10) as usize;
        self.all_grids()
            .for_each(|g| g.set_cond_intel(pass, min_chars));
    }

    /// 캐럿을 다음/이전 문장(`;` 분리 · [`nsql_script::split_script`]) 시작으로(Alt+↓/↑ · 실행 뒤 자동 이동).
    pub(crate) fn goto_statement(&mut self, forward: bool) {
        let full = self.ed_mut().text();
        let caret = self.ed_mut().caret();
        let byte = full
            .char_indices()
            .nth(caret)
            .map_or(full.len(), |(b, _)| b);
        let items = nsql_script::split_script_in(&full, Some(self.sess.dialect));
        let cur = items
            .iter()
            .position(|it| it.span.start <= byte && byte <= it.span.end);
        let idx = if forward {
            match cur {
                Some(c) => Some(c + 1),
                None => items.iter().position(|it| it.span.start > byte),
            }
        } else {
            match cur {
                Some(c) => c.checked_sub(1),
                None => items.iter().rposition(|it| it.span.end < byte),
            }
        };
        let Some(it) = idx.and_then(|i| items.get(i)) else {
            return;
        };
        let ci = full[..it.span.start.min(full.len())].chars().count();
        let mut inv = Invalidations::default();
        self.ed_mut().select_range(ci, ci, &mut inv);
        self.redraw();
    }

    /// 결과 탭 id로 그리드 찾기 — 활성이면 `grid` · 아니면 잠든 것(활성 패널의 다른 탭 · 잠든 패널의 탭).
    pub(crate) fn grid_for(&mut self, key: u64) -> Option<&mut grid::Grid> {
        if key == self.grid_tab {
            return Some(&mut self.grid);
        }
        if let Some(t) = self.panel.tabs.iter_mut().find(|t| t.id == key) {
            return Some(&mut t.grid);
        }
        self.panels
            .values_mut()
            .flat_map(|p| p.tabs.iter_mut())
            .find(|t| t.id == key)
            .map(|t| &mut t.grid)
    }

    /// 잠든 그리드 전부(활성 패널의 비활성 탭 + 잠든 패널의 탭 · 활성 자리표시자 포함 — 빈 그리드라 무해).
    pub(crate) fn sleeping_grids(&self) -> impl Iterator<Item = &grid::Grid> {
        self.panel.tabs.iter().map(|t| &t.grid).chain(
            self.panels
                .values()
                .flat_map(|p| p.tabs.iter().map(|t| &t.grid)),
        )
    }

    pub(crate) fn sleeping_grids_mut(&mut self) -> impl Iterator<Item = &mut grid::Grid> {
        self.panel.tabs.iter_mut().map(|t| &mut t.grid).chain(
            self.panels
                .values_mut()
                .flat_map(|p| p.tabs.iter_mut().map(|t| &mut t.grid)),
        )
    }

    /// 설정 `grid.result_tabs`/`grid.result_tabbar_single` → 모든 패널. 끄면 활성 탭 외 전부 즉시 해제(D-73 "강제로 메모리 줄이기").
    pub(crate) fn apply_result_tab_opts(&mut self) {
        let enabled = self.settings.flag("grid.result_tabs");
        let always = self.settings.flag("grid.result_tabbar_single");
        let numbered = self.settings.get("grid.result_tab_title") != Some("table");
        self.panel.set_options(enabled, always);
        self.panel.set_numbered(numbered);
        for p in self.panels.values_mut() {
            p.set_options(enabled, always);
            p.set_numbered(numbered);
        }
        if !enabled {
            let keep = self.panel.active_id();
            self.panel.tabs.retain(|t| t.id == keep);
            self.panel.active = 0;
            for p in self.panels.values_mut() {
                let keep = p.active_id();
                p.tabs.retain(|t| t.id == keep);
                p.active = 0;
            }
        }
        self.panel.sync_bar();
        self.layout();
        self.redraw();
    }

    /// 결과 탭 전체의 행 바이트 합이 예산(`grid.memory_budget_mb`)을 넘는가(D-72).
    /// 활성 패널의 결과 탭 `i`를 활성으로(그리드 맞바꾸기 · D-71 "그리기는 활성 탭만").
    pub(crate) fn activate_result(&mut self, i: usize) {
        if i >= self.panel.tabs.len() || i == self.panel.active {
            return;
        }
        let a = self.panel.active;
        std::mem::swap(&mut self.grid, &mut self.panel.tabs[a].grid);
        self.panel.active = i;
        std::mem::swap(&mut self.grid, &mut self.panel.tabs[i].grid);
        self.grid_tab = self.panel.tabs[i].id;
        let b = self.panel.tabs[a].grid.outer_bounds();
        self.grid.set_bounds(b);
        self.panel.sync_bar();
        self.set_focus(Focus::Grid);
        // Output 탭을 보면 미읽음 배지를 거둔다(10-01).
        self.output_sync_badge();
    }

    /// 실행용 새 결과 탭(설정 `grid.result_tabs`일 때만) — 만들었으면 그 전 활성 탭 id.
    /// 조회 하나를 **새 결과 탭**으로 실행(SHOW VARIABLES · 참조 행 보기 · 필터 재조회가 같이 쓴다) — 문지기를 지난다.
    pub(crate) fn run_in_fresh_tab(&mut self, sql: String) {
        if self.sess.busy || !self.gate_open() {
            return;
        }
        let prev = self.fresh_result_tab_for_run();
        self.run_text(sql, 0, true);
        self.mark_fresh_run_tab(prev);
    }

    pub(crate) fn fresh_result_tab_for_run(&mut self) -> Option<u64> {
        if !self.settings.flag("grid.result_tabs") {
            return None;
        }
        let prev = self.panel.tabs.get(self.panel.active).map(|t| t.id);
        self.new_result_tab();
        prev
    }

    /// 실행이 실제로 시작됐으면 "새 탭으로 시작함"을 세션에 적는다(시작 못 했으면 = 막힘 · 곧바로 걷는다).
    pub(crate) fn mark_fresh_run_tab(&mut self, prev: Option<u64>) {
        let Some(prev) = prev else { return };
        if self.sess.run_tracking {
            self.sess.run_fresh_prev = Some(prev);
        } else {
            let fresh = self.panel.tabs.get(self.panel.active).map(|t| t.id);
            self.drop_fresh_result_tab(fresh, prev);
        }
    }

    /// ★ 결과 없이 끝난 실행의 새 결과 탭을 거두고 그 전 탭으로(사용자 09-28 "실행 오류면 결과 탭을 추가할 필요 없다").
    pub(crate) fn drop_fresh_result_tab(&mut self, fresh: Option<u64>, prev: u64) {
        if let Some(i) = fresh.and_then(|id| self.panel.index_of(id)) {
            self.close_result_tab(i);
        }
        if let Some(j) = self.panel.index_of(prev) {
            self.activate_result(j);
        }
    }

    /// ★ 실행 결과가 갈 탭(10-01 ㉘-b · 사용자 "2행 가져옴인데 결과 탭이 비어 있다"): 활성 탭이 Output이면 결과는 **가장 최근 결과 탭**
    ///   (없으면 새 결과 탭)으로 — 종전에는 `grid_tab`(= Output 자리표시 그리드)에 써서 아무 데도 보이지 않았다.
    pub(crate) fn run_target_tab(&mut self) -> u64 {
        if !self.panel.output_active() {
            return self.grid_tab;
        }
        if let Some(t) = self
            .panel
            .tabs
            .iter()
            .filter(|t| !t.is_output)
            .max_by_key(|t| t.seq)
        {
            return t.id;
        }
        self.new_result_tab();
        self.grid_tab
    }

    /// 새 결과 탭(Ctrl+\ · D-71): 현재 설정을 물려받은 빈 그리드 · 상한을 넘으면 가장 오래된 비고정 탭 정리.
    pub(crate) fn new_result_tab(&mut self) {
        let a = self.panel.active;
        // 실제 그리드를 제자리에 돌려놓고 그 설정을 물려받는다.
        std::mem::swap(&mut self.grid, &mut self.panel.tabs[a].grid);
        let fresh = self.panel.tabs[a].grid.fresh_like();
        let id = self.next_result_id;
        self.next_result_id += 1;
        let tab = ResultTab {
            id,
            title: t(Msg::ResultTabDefault).to_string(),
            pinned: false,
            sys_pinned: false,
            named: false,
            sql: String::new(),
            grid: fresh,
            seq: id,
            child_of: None,
            is_output: false,
        };
        self.panel.push(tab);
        let max = self.cap(
            nsql_license::Feature::ResultTabs,
            self.settings.int("grid.result_tabs_max").clamp(1, 64) as usize,
            2,
        );
        if self.panel.tabs.len() > max && self.settings.flag("grid.result_tab_evict") {
            if let Some(v) = self.panel.evict_candidate() {
                let gone = self.panel.remove(v).map(|t| t.title).unwrap_or_default();
                self.sess.status = tf(Msg::StResultTabEvicted, &[&gone]);
                self.log_win
                    .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            }
        }
        let i = self.panel.index_of(id).unwrap_or(0);
        self.panel.active = i;
        std::mem::swap(&mut self.grid, &mut self.panel.tabs[i].grid);
        self.grid_tab = id;
        self.panel.sync_bar();
        if self.panel.take_bar_changed() {
            self.layout();
        }
    }

    /// `parent` 탭에 딸린 `ord`번째 결과 탭의 id — 있으면 재사용, 없으면 같은 패널 끝에 만든다(활성 탭은 바꾸지 않는다 ·
    /// 상한 `grid.result_tabs_max`를 넘으면 가장 오래된 비고정 탭을 걷는다). 패널을 못 찾으면 `parent`(덮어쓰기 = 종전).
    pub(crate) fn child_result_tab(&mut self, parent: u64, ord: u32) -> u64 {
        let max = self.cap(
            nsql_license::Feature::ResultTabs,
            self.settings.int("grid.result_tabs_max").clamp(1, 64) as usize,
            2,
        );
        let evict = self.settings.flag("grid.result_tab_evict");
        let Some(fresh) = self.grid_for(parent).map(|g| g.fresh_like()) else {
            return parent;
        };
        let in_active = self.panel.index_of(parent).is_some();
        let panel = if in_active {
            &mut self.panel
        } else {
            match self
                .panels
                .values_mut()
                .find(|p| p.index_of(parent).is_some())
            {
                Some(p) => p,
                None => return parent,
            }
        };
        if let Some(t) = panel
            .tabs
            .iter()
            .find(|t| t.child_of == Some((parent, ord)))
        {
            return t.id;
        }
        let id = self.next_result_id;
        self.next_result_id += 1;
        panel.push(ResultTab {
            id,
            title: t(Msg::ResultTabDefault).to_string(),
            pinned: false,
            sys_pinned: false,
            named: false,
            sql: String::new(),
            grid: fresh,
            seq: id,
            child_of: Some((parent, ord)),
            is_output: false,
        });
        if panel.tabs.len() > max && evict {
            // 방금 만든 탭·부모·활성 탭은 걷지 않는다.
            let active = panel.active;
            let victim = panel
                .tabs
                .iter()
                .enumerate()
                .filter(|(i, t)| !t.pinned && *i != active && t.id != id && t.id != parent)
                .min_by_key(|(_, t)| t.seq)
                .map(|(i, _)| i);
            if let Some(v) = victim {
                panel.remove(v);
            }
        }
        panel.sync_bar();
        if in_active && self.panel.take_bar_changed() {
            self.layout();
        }
        id
    }

    /// 실행이 끝났다 — `parent`에 딸린 탭 가운데 이번 실행에서 쓰이지 않은 것(`ord >= used`)을 걷는다(고정·활성 탭은 둔다).
    pub(crate) fn prune_child_results(&mut self, parent: u64, used: u32) {
        let in_active = self.panel.index_of(parent).is_some();
        let panel = if in_active {
            &mut self.panel
        } else {
            match self
                .panels
                .values_mut()
                .find(|p| p.index_of(parent).is_some())
            {
                Some(p) => p,
                None => return,
            }
        };
        let active_id = panel.tabs.get(panel.active).map(|t| t.id);
        let gone: Vec<u64> = panel
            .tabs
            .iter()
            .filter(|t| {
                matches!(t.child_of, Some((p, o)) if p == parent && o >= used)
                    && !t.pinned
                    && Some(t.id) != active_id
            })
            .map(|t| t.id)
            .collect();
        if gone.is_empty() {
            return;
        }
        for id in gone {
            if let Some(i) = panel.index_of(id) {
                panel.remove(i);
            }
        }
        panel.sync_bar();
        if in_active && self.panel.take_bar_changed() {
            self.layout();
        }
        self.mem_released();
    }

    /// 결과 탭의 지금 제목.
    pub(crate) fn result_title(&self, key: u64) -> String {
        self.panel
            .tabs
            .iter()
            .chain(self.panels.values().flat_map(|p| p.tabs.iter()))
            .find(|t| t.id == key)
            .map(|t| t.title.clone())
            .unwrap_or_default()
    }

    /// 결과 탭 제목을 정해 준다(커서 변수 이름 · 딸린 결과 번호) — 사용자가 이름 붙였거나 고정한 탭은 그대로.
    pub(crate) fn title_result_as(&mut self, key: u64, title: &str) {
        let panel = if self.panel.index_of(key).is_some() {
            Some(&mut self.panel)
        } else {
            self.panels.values_mut().find(|p| p.index_of(key).is_some())
        };
        if let Some(p) = panel {
            if let Some(i) = p.index_of(key) {
                if !p.tabs[i].named && !p.tabs[i].pinned {
                    p.tabs[i].title = p.unique_title(title, i);
                }
            }
            p.sync_bar();
        }
    }

    /// 결과가 도착한 탭의 제목(사용자가 이름 붙이거나 고정한 탭은 그대로).
    pub(crate) fn retitle_result(&mut self, key: u64) {
        let (base, sql) = match self.grid_for(key) {
            Some(g) => (
                results::title_from_sql(g.source_table().as_deref(), g.source_sql()),
                g.source_sql().to_string(),
            ),
            None => return,
        };
        let cur_editor = self.panel_editor;
        let panel = if self.panel.index_of(key).is_some() {
            Some(&mut self.panel)
        } else {
            self.panels.values_mut().find(|p| p.index_of(key).is_some())
        };
        let _ = cur_editor;
        if let Some(p) = panel {
            if let Some(i) = p.index_of(key) {
                // 이 결과를 만든 실행 쿼리를 탭에 보관한다(우클릭 ▸ 실행 쿼리 복사).
                p.tabs[i].sql.clone_from(&sql);
                if p.numbered() {
                    // 번호 규칙(`결과N` · 기본): 번호가 있으면 그대로 · 없으면 새 번호(이름 붙인·고정한 탭은 그대로).
                    p.ensure_numbered(i);
                } else if !p.tabs[i].named && !p.tabs[i].pinned {
                    let title = p.unique_title(&base, i);
                    p.tabs[i].title = title;
                }
            }
            p.sync_bar();
        }
    }

    /// 결과 탭 패널 동작(탭 바 클릭 · 우클릭 메뉴 · 단축키).
    pub(crate) fn panel_action(&mut self, a: ResultAction) {
        let n = self.panel.tabs.len();
        match a {
            ResultAction::Activate(i) => self.activate_result(i),
            ResultAction::Close(i) => self.close_result_tab(i),
            ResultAction::CloseOthers(i) => {
                self.activate_result(i);
                let keep = self.grid_tab;
                let ids: Vec<u64> = self
                    .panel
                    .tabs
                    .iter()
                    .filter(|t| t.id != keep && !t.pinned)
                    .map(|t| t.id)
                    .collect();
                for id in ids {
                    if let Some(j) = self.panel.index_of(id) {
                        self.close_result_tab(j);
                    }
                }
            }
            ResultAction::CloseRight(i) => {
                let ids: Vec<u64> = self
                    .panel
                    .tabs
                    .iter()
                    .skip(i + 1)
                    .filter(|t| !t.pinned)
                    .map(|t| t.id)
                    .collect();
                for id in ids {
                    if let Some(j) = self.panel.index_of(id) {
                        self.close_result_tab(j);
                    }
                }
            }
            ResultAction::TogglePin(i) => {
                if let Some(t) = self.panel.tabs.get_mut(i) {
                    // 자동 고정(Output) 탭의 첫 토글 = 사용자 고정으로(표식이 생긴다) · 그다음부터 보통 토글.
                    if t.sys_pinned {
                        t.sys_pinned = false;
                        t.pinned = true;
                    } else {
                        t.pinned = !t.pinned;
                    }
                }
            }
            ResultAction::CopySql(i) => {
                // 탭에 보관한 실행 쿼리(없으면 그 탭 그리드의 출처 문장)를 클립보드에.
                let sql = self
                    .panel
                    .tabs
                    .get(i)
                    .map(|t| t.sql.clone())
                    .filter(|s| !s.trim().is_empty())
                    .or_else(|| {
                        let id = self.panel.tabs.get(i)?.id;
                        self.grid_for(id).map(|g| g.source_sql().to_string())
                    })
                    .unwrap_or_default();
                // ★ T-255: `grid.copy_sql_format`이면 기본 포맷터로 정돈해 복사(실패하면 원문).
                let sql = if self.settings.flag("grid.copy_sql_format") && !sql.trim().is_empty() {
                    let engine = self.format_default_engine();
                    self.format_run(&engine, &sql, false).unwrap_or(sql)
                } else {
                    sql
                };
                if sql.trim().is_empty() {
                    self.sess.status = t(Msg::StResultNoSql).to_string();
                } else if clipboard::write_text(&sql) {
                    self.sess.status =
                        tf(Msg::StResultSqlCopied, &[&sql.lines().count().to_string()]);
                }
            }
            ResultAction::Rename(i) => {
                if let Some(tab) = self.panel.tabs.get(i) {
                    let (id, title) = (tab.id, tab.title.clone());
                    let anchor = self.panel.tab_rect(i);
                    self.palette.open_prompt_at(
                        &format!("result.rename:{id}"),
                        t(Msg::PhResultRename),
                        &title,
                        anchor,
                    );
                }
            }
            ResultAction::MoveFirst(i) if i < n => self.move_result_tab(i, 0),
            ResultAction::MoveLast(i) if i < n => self.move_result_tab(i, n - 1),
            ResultAction::Move { from, to } if from < n && to < n => self.move_result_tab(from, to),
            _ => {}
        }
        self.panel.sync_bar();
        if self.panel.take_bar_changed() {
            self.layout();
        }
        self.redraw();
    }

    fn move_result_tab(&mut self, from: usize, to: usize) {
        let active_id = self.grid_tab;
        let t = self.panel.tabs.remove(from);
        self.panel.tabs.insert(to, t);
        self.panel.active = self.panel.index_of(active_id).unwrap_or(0);
    }

    /// 결과 탭 닫기 = rows·커서 즉시 해제(D-72). 활성 탭이면 이웃을 활성으로 · 마지막 하나면 빈 탭으로 교체.
    pub(crate) fn close_result_tab(&mut self, i: usize) {
        if i >= self.panel.tabs.len() {
            return;
        }
        if i == self.panel.active {
            // 실제 그리드를 자리에 돌려놓은 뒤 제거 → 이웃 탭의 그리드를 꺼낸다.
            std::mem::swap(&mut self.grid, &mut self.panel.tabs[i].grid);
            let bounds = self.grid.outer_bounds();
            drop(self.panel.remove(i));
            if self.panel.tabs.is_empty() {
                let id = self.next_result_id;
                self.next_result_id += 1;
                let fresh = self.grid.fresh_like();
                self.panel.push(ResultTab {
                    id,
                    title: t(Msg::ResultTabDefault).to_string(),
                    pinned: false,
                    sys_pinned: false,
                    named: false,
                    sql: String::new(),
                    grid: fresh,
                    seq: id,
                    child_of: None,
                    is_output: false,
                });
                self.panel.active = 0;
            }
            let a = self.panel.active;
            std::mem::swap(&mut self.grid, &mut self.panel.tabs[a].grid);
            self.grid_tab = self.panel.tabs[a].id;
            self.grid.set_bounds(bounds);
        } else {
            drop(self.panel.remove(i));
            self.panel.active = self.panel.index_of(self.grid_tab).unwrap_or(0);
        }
        self.offset_warned.remove(&self.grid_tab);
    }

    /// 추가 페치·전체 조회·건수를 워커에(같은 세션 · docs/43 §3-4 OFFSET 폴백).
    fn send_fetch(&mut self, req: grid::FetchReq) {
        let sql = self.grid.source_sql().to_string();
        if sql.trim().is_empty() {
            self.grid.fetch_failed();
            return;
        }
        // ★ 통제(docs/52 §3): 이 탭의 세션이 다른 작업 중이면 추가 페치·전체 조회·건수도 보내지 않는다(요청 상태만 푼다 —
        //   풀린 뒤 스크롤·버튼으로 다시 요청된다). 워커는 순차라 보내도 안전하지만, 언제 끝날지 모르는 대기를 만들지 않는다.
        let Some(pass) = self.gate_pass() else {
            self.grid.fetch_failed();
            return;
        };
        self.wake_if_idle();
        let key = self.grid_tab;
        // 메모리 예산(D-72 · 09-17 탭별 독립): **이 탭**의 행이 예산을 넘으면 추가 페치만 거부(전체 조회는 교체라 허용).
        //   다른 탭의 크기는 보지 않는다 — 사용자 09-17 "탭은 서로 영향을 미치지 않아야".
        let budget = (self.settings.int("grid.memory_budget_mb").max(1) as u64) * 1024 * 1024;
        if matches!(req, grid::FetchReq::Next { .. }) && self.grid.approx_bytes() > budget {
            self.grid.fetch_failed();
            self.sess.status = tf(
                Msg::StBudgetExceeded,
                &[&self.settings.int("grid.memory_budget_mb").to_string()],
            );
            self.log_win
                .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
            self.redraw();
            return;
        }
        match req {
            grid::FetchReq::Next { offset, limit } => {
                if self.settings.flag("grid.offset_warn")
                    && !nsql_io::paging::has_order_by(&sql)
                    && self.offset_warned.insert(key)
                {
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        t(Msg::StOffsetWarn).to_string(),
                    ));
                }
                self.sess.submit(
                    pass,
                    worker::Cmd::FetchPage {
                        key,
                        sql,
                        offset,
                        limit,
                        budget_bytes: 0,
                        strict: self.settings.get("grid.refetch_mode") != Some("offset"),
                        strict_all: self.settings.get("grid.refetch_mode") == Some("strict_all"),
                    },
                );
            }
            grid::FetchReq::All => {
                // 실행 Facade(docs/43 §11): 전체 조회는 길 수 있어 커서 경로여도 카드.
                self.fetch_card_start(key, Msg::CardFetchAll, &sql);
                // 전체 조회 = **나머지 이어 받기**(09-17 위치 유지): offset = 이미 든 행 수 · 예산 = 이 탭 예산에서 든 만큼을 뺀 나머지
                // (탭별 독립 · 다른 탭을 빼지 않는다 · 이미 넘었으면 1 = 첫 배치 뒤 예산 정지).
                let remain = budget.saturating_sub(self.grid.approx_bytes()).max(1);
                self.sess.submit(
                    pass,
                    worker::Cmd::FetchPage {
                        key,
                        sql,
                        offset: self.grid.row_count(),
                        limit: 0,
                        budget_bytes: remain,
                        strict: self.settings.get("grid.refetch_mode") != Some("offset"),
                        strict_all: self.settings.get("grid.refetch_mode") == Some("strict_all"),
                    },
                );
            }
            grid::FetchReq::Count => {
                // 건수 = 새 SQL(`SELECT COUNT(*) …`) → 카드.
                self.fetch_card_start(key, Msg::CardCount, &sql);
                self.sess.submit(pass, worker::Cmd::Count { key, sql });
            }
        }
        self.sess.aux += 1;
        self.sess.touch();
        self.sync_gate();
        self.sess.status = t(Msg::StFetching).into();
        self.redraw();
    }

    /// ★ 페치/건수의 실행 상태 카드 시작(docs/43 §11 실행 Facade): 직접 실행과 같은 카드 · 결과(`Page`/`Count`)가 오면 끝난다.
    pub(crate) fn fetch_card_start(&mut self, key: u64, kind: Msg, sql: &str) {
        let text = format!("{} — {}", t(kind), nsql_run::txlog::one_line(sql, 200));
        self.sess.fetch_card = Some((key, Instant::now()));
        self.sess.run_card = self
            .run_toast
            .start(&text, nsql_log::now_local().stamp(), 1);
        self.run_toast
            .set_phase(self.sess.run_card, runtoast::Phase::Fetching);
    }

    /// 새로고침 — 같은 문장을 이 탭의 세그먼트 크기로 다시 실행.
    pub(crate) fn refresh_result(&mut self) {
        let src = self.grid.source_sql().to_string();
        if src.trim().is_empty() {
            return;
        }
        let Some(pass) = self.gate_pass() else {
            return;
        };
        self.wake_if_idle();
        self.sess.touch();
        self.sess.run_tab = self.grid_tab;
        self.sess.run_set_stmt = None;
        self.sess.run_children = 0;
        // 결과 새로고침은 **그 탭의 문장 하나**만 다시 돈다 → 딸린 결과 탭을 걷지 않는다(첫 결과 탭을 새로 고쳤다고
        // 다른 문장의 결과 탭이 닫히면 안 된다) · 변수 표는 지금 편집기 탭의 것.
        self.sess.run_tracking = false;
        self.sess.run_editor = self.editors.active_id();
        self.sess.busy = true;
        self.sess.status = t(Msg::StRunning).into();
        self.run_toast_start(&src);
        self.sess.last_run_items = split_items(&src, self.sess.dialect);
        let max_rows = self.grid.page_rows();
        self.sess.submit(
            pass,
            worker::Cmd::Run {
                src,
                preflight: None,
                max_rows,
                vars: self.run_vars(),
                defines: self.run_defines(),
                intrinsic: Some(self.run_intrinsic()),
                whole: false,
            },
        );
        self.live_start();
        self.redraw();
    }

    /// SQL 보기(결과 도구줄) — 키 규칙은 Copy SQL과 같다(docs/41 · 캐시 → 워커 1회).
    fn begin_view_sql(&mut self, kind: nsql_io::SqlKind) {
        if self.key_mode() == nsql_io::KeyMode::All {
            self.finish_view_sql(kind, None);
            return;
        }
        let Some(guess) = self.grid.source_table() else {
            self.finish_view_sql(kind, None);
            return;
        };
        if let Some(info) = self.sess.key_cache.get(&guess) {
            let info = info.clone();
            self.finish_view_sql(kind, info.as_ref());
            return;
        }
        let Some(pass) = self.gate_pass() else {
            return;
        };
        let (schema, table) = nsql_io::split_table(self.sess.dialect, &guess);
        self.sess.view_wait = Some(kind);
        self.sess.aux += 1;
        self.sess.submit(
            pass,
            worker::Cmd::Keys {
                key: guess,
                schema,
                table,
            },
        );
    }

    pub(crate) fn finish_view_sql(
        &mut self,
        kind: nsql_io::SqlKind,
        info: Option<&nsql_core::KeyInfo>,
    ) {
        let names = self.grid.all_col_names();
        let key = nsql_io::choose_key(self.key_mode(), info, &names);
        if key.needs_warning() && kind != nsql_io::SqlKind::Insert {
            let table = self.grid.source_table();
            let w = tf(
                Msg::SqlKeyWarnFirstN,
                &[
                    table.as_deref().unwrap_or("T"),
                    &key.cols.len().to_string(),
                    &key.cols.join(", "),
                ],
            );
            self.sess.status = w.clone();
            self.log_win.push(LogEntry::new(LogKind::Error, w));
        }
        self.grid.finish_view_sql(kind, &key);
        self.redraw();
    }

    /// ★ Copy SQL(docs/41): 키 = 설정(pk: 카탈로그 PK → 유니크 → 앞 3컬럼 + 경고 1회 · all: 전체 컬럼). 카탈로그는 워커에
    ///   1회 묻고(테이블당 캐시) 답이 오면 완성한다 — UI 스레드는 디스크·네트워크를 만지지 않는다.
    fn begin_sql_copy(&mut self, kind: nsql_io::SqlKind) {
        if self.key_mode() == nsql_io::KeyMode::All {
            self.finish_sql_copy(kind, None);
            return;
        }
        let Some(guess) = self.grid.source_table() else {
            self.finish_sql_copy(kind, None);
            return;
        };
        if let Some(info) = self.sess.key_cache.get(&guess) {
            let info = info.clone();
            self.finish_sql_copy(kind, info.as_ref());
            return;
        }
        let Some(pass) = self.gate_pass() else {
            return;
        };
        let (schema, table) = nsql_io::split_table(self.sess.dialect, &guess);
        self.sess.sql_wait = Some(kind);
        self.sess.aux += 1;
        self.sess.submit(
            pass,
            worker::Cmd::Keys {
                key: guess,
                schema,
                table,
            },
        );
    }

    /// 키 정보로 문장을 만들어 클립보드에 · 대체 키/테이블 미추정은 상태줄 + 로그에 1회 경고.
    pub(crate) fn finish_sql_copy(
        &mut self,
        kind: nsql_io::SqlKind,
        info: Option<&nsql_core::KeyInfo>,
    ) {
        let names = self.grid.selected_col_names();
        let key = nsql_io::choose_key(self.key_mode(), info, &names);
        let Some((text, n)) = self.grid.copy_sql(kind, &key) else {
            return;
        };
        if !clipboard::write_text(&text) {
            self.sess.status = t(Msg::ErrClipboard).into();
            return;
        }
        self.sess.status = tf(Msg::StCopied, &[&n.to_string()]);
        let table = self.grid.source_table();
        let mut warns: Vec<String> = Vec::new();
        if table.is_none() {
            warns.push(t(Msg::SqlKeyWarnNoTable).to_string());
        }
        if key.needs_warning() && kind != nsql_io::SqlKind::Insert {
            warns.push(tf(
                Msg::SqlKeyWarnFirstN,
                &[
                    table.as_deref().unwrap_or("T"),
                    &key.cols.len().to_string(),
                    &key.cols.join(", "),
                ],
            ));
        }
        for w in warns {
            self.sess.status = w.clone();
            self.log_win.push(LogEntry::new(LogKind::Error, w));
        }
        self.redraw();
    }

    /// 지금 세션(`self.sess`)의 결과 그리드에 방언을 알린다 — 전용 세션 = 주인 탭의 패널 · 공유 세션 = 전용 탭이 아닌 패널 전부.
    pub(crate) fn set_dialect_for_sess_grids(&mut self, dialect: Dialect) {
        let prod =
            self.sess.spec.as_ref().and_then(|sp| sp.env) == Some(nsql_script::ConnEnv::Prod);
        let owner = self.sess.owner;
        let private_tabs: Vec<u64> = self.all_sess().filter_map(|s| s.owner).collect();
        let mine = |tab: u64| match owner {
            Some(o) => tab == o,
            None => !private_tabs.contains(&tab),
        };
        if mine(self.panel_editor) {
            self.grid.set_dialect(dialect);
            self.grid.set_prod(prod);
            for t in &mut self.panel.tabs {
                t.grid.set_dialect(dialect);
                t.grid.set_prod(prod);
            }
        }
        for (tab, p) in &mut self.panels {
            if mine(*tab) {
                for t in &mut p.tabs {
                    t.grid.set_dialect(dialect);
                    t.grid.set_prod(prod);
                }
            }
        }
    }

    /// 행 포커스 배경 설정 → 전 그리드(사용자 09-22).
    pub(crate) fn apply_grid_row_focus(&mut self) {
        let on = self.settings.flag("grid.row_focus");
        let (c, a) = color_alpha_setting(&self.settings, "grid.row_focus_color");
        self.all_grids().for_each(|g| g.set_row_focus(on, c, a));
    }

    /// 활성 그리드 + 잠든 그리드 전부(설정 전파용).
    pub(crate) fn all_grids(&mut self) -> impl Iterator<Item = &mut grid::Grid> {
        let (grid, panel, panels) = (&mut self.grid, &mut self.panel, &mut self.panels);
        std::iter::once(grid)
            .chain(panel.tabs.iter_mut().map(|t| &mut t.grid))
            .chain(
                panels
                    .values_mut()
                    .flat_map(|p| p.tabs.iter_mut().map(|t| &mut t.grid)),
            )
    }

    pub(crate) fn run_grid(&mut self) -> Option<&mut grid::Grid> {
        let k = self.sess.run_tab;
        self.grid_for(k)
    }

    /// ★ 편집기 탭 ↔ 결과 패널 쌍 동기화(사용자 09-16 · T-93): 활성 편집기 탭이 바뀌었으면 그 탭의 패널을 꺼내 오고
    /// (없으면 설정만 물려받은 빈 탭 하나) 지금 패널은 잠재운다 · 닫힌 편집기 탭의 패널은 통째로 버린다(rows 즉시 해제).
    /// 페인트 직전과 이벤트 뒤에 부른다.
    pub(crate) fn sync_grid_tab(&mut self) {
        self.sync_tabs_menu();
        // 탭을 바꿨다 = 그 파일을 확인하고(오래 안 본 탭) 확인 띠를 그 탭의 것으로.
        let tab = self.editors.active_id();
        if tab != self.ext_last_tab {
            self.ext_last_tab = tab;
            self.vars_win_context();
            self.ext_check(false);
            self.ext_banner_sync();
        }
        // 활성 탭의 세션도 같은 시점에 맞춘다(docs/52) — 표식 클릭·메뉴 선택도 여기서 거둔다.
        if let Some(i) = self.editors.take_badge_request() {
            self.open_badge_menu(i);
        }
        if let Some((tab, id)) = self.editors.take_badge_pick() {
            self.badge_pick(tab, &id);
        }
        // 탭 바 [+]로 만든 새 탭(메뉴 New와 같은 규칙).
        if self.editors.take_new_tab_created() {
            self.on_new_tab();
        }
        // ★ 순서(09-19): 거두기 → 활성 탭 세션 맞추기(새 탭을 공유 연결에 **묶는다**) → 표식·배지. 종전엔 표식을 먼저 맞춰
        //   새 탭이 아직 안 묶인 상태로 배지(함께 쓰는 탭 수)를 계산했고, 다음 탭 전환 때에야 숫자가 늘었다.
        self.reap_sessions();
        self.sync_sess();
        // 탭 목록(새 탭·닫기)이나 묶임이 바뀌었으면 표식·해제 버튼 배지를 바로 맞춘다 — 이벤트를 기다리지 않는다(바뀔 때만 · 페인트마다 아님).
        let ids = self.editors.tab_ids();
        if ids != self.badge_tabs || self.sess_ui_dirty {
            self.badge_tabs = ids;
            self.sess_ui_dirty = false;
            self.sync_sess_ui();
        }
        self.sync_gate();
        let cur = self.editors.active_id();
        if cur != self.panel_editor {
            let b = self.grid.outer_bounds();
            // 실제 그리드를 활성 자리에 돌려놓고 패널을 잠재운다.
            let a = self
                .panel
                .active
                .min(self.panel.tabs.len().saturating_sub(1));
            if let Some(slot) = self.panel.tabs.get_mut(a) {
                std::mem::swap(&mut self.grid, &mut slot.grid);
            }
            let enabled = self.settings.flag("grid.result_tabs");
            let always = self.settings.flag("grid.result_tabbar_single");
            let next = self.panels.remove(&cur).unwrap_or_else(|| {
                let id = self.next_result_id;
                self.next_result_id += 1;
                let fresh = self
                    .panel
                    .tabs
                    .get(a)
                    .map_or_else(grid::Grid::default, |t| t.grid.fresh_like());
                ResultPanel::new(
                    ResultTab {
                        id,
                        title: t(Msg::ResultTabDefault).to_string(),
                        pinned: false,
                        sys_pinned: false,
                        named: false,
                        sql: String::new(),
                        grid: fresh,
                        seq: id,
                        child_of: None,
                        is_output: false,
                    },
                    enabled,
                    always,
                )
            });
            let mut next = next;
            next.set_numbered(self.settings.get("grid.result_tab_title") != Some("table"));
            // 닫기 상자 표시 규칙도 새 결과 탭 바에(설정 `editor.tab_close_show` · 09-28).
            next.set_close_always(self.settings.get("editor.tab_close_show") != Some("hover"));
            let old = std::mem::replace(&mut self.panel, next);
            if self.panel_editor != 0 {
                self.panels.insert(self.panel_editor, old);
            }
            self.panel_editor = cur;
            // ★ 탭이 바뀌었다 → 세션 작업 단위를 이 탭의 것으로(⑯ · 조용히).
            self.apply_tab_unit();
            let a = self
                .panel
                .active
                .min(self.panel.tabs.len().saturating_sub(1));
            self.panel.active = a;
            std::mem::swap(&mut self.grid, &mut self.panel.tabs[a].grid);
            self.grid_tab = self.panel.tabs[a].id;
            self.grid.set_bounds(b);
            self.panel.sync_bar();
            self.layout();
        }
        let alive = self.editors.tab_ids();
        self.panels.retain(|id, _| alive.contains(id));
        self.tab_vars.retain(|id, _| alive.contains(id));
        self.object_tabs.retain(|id, _| alive.contains(id));
        if cur != self.output_last_editor {
            self.output_last_editor = cur;
            self.output_on_tab_switch();
        }
    }
}
