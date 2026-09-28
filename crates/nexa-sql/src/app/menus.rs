//! App — 메뉴·팔레트·명령 분배(풀다운·우클릭·키 명령).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 들여쓰기 팝업(Sublime 상태바 클릭과 같은 항목 · docs/31 §2 1차): 공백/탭 · 탭 폭 1~8 · 변환.
    /// ★ 고른 값은 **활성 탭에만** 적용한다(탭마다 다를 수 있다 · 설정은 새 탭의 기본값 · 사용자 09-15).
    pub(crate) fn open_indent_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let (tsz, spaces) = self.editors.indent();
        let ts = i64::from(tsz);
        let mark = |on: bool, s: String| {
            if on {
                format!("✓ {s}")
            } else {
                format!("   {s}")
            }
        };
        let mut items = vec![
            CtxItem::item(
                "indent.spaces",
                mark(spaces, t(Msg::MnIndentSpaces).to_string()),
            ),
            CtxItem::item(
                "indent.tabs",
                mark(!spaces, t(Msg::MnIndentTabs).to_string()),
            ),
        ];
        for n in 1..=8 {
            items.push(CtxItem::item(
                format!("indent.size:{n}"),
                mark(n == ts, tf(Msg::MnTabWidth, &[&n.to_string()])),
            ));
        }
        items.push(CtxItem::item(
            "indent.to_spaces",
            format!("   {}", t(Msg::MnConvertToSpaces)),
        ));
        items.push(CtxItem::item(
            "indent.to_tabs",
            format!("   {}", t(Msg::MnConvertToTabs)),
        ));
        let r = self.status_tab_rect;
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(r);
        // 상태줄 위로 열리도록 세그먼트 상단 기준(팝업 부품이 화면 안에 맞춘다).
        // 배율 전달(빠져 있어 맥 2x에서 항목 간격이 절반이었다 · 09-16).
        self.status_menu.set_scale(self.scale);
        self.status_menu
            .open_at(r.x, r.y, items, host, px(80.0, self.scale));
    }

    /// 상태줄 줄끝 팝업(LF/CRLF · 현재 = ✓) — 고르면 활성 탭 줄끝 변경(저장 때 반영 · docs/38).
    /// ★ 상태줄 자동 저장 항목 클릭 = 메뉴(사용자 09-28): 프로젝트 폴더 열기(프로젝트 파일 선택 · 프로젝트일 때) · 일반 파일 자동 저장
    /// 위치 열기(이 탭의 최신 스냅숏 선택) · 자동 저장 설정(설정 창을 `project.autosave`로).
    pub(crate) fn open_autosave_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let mut items = Vec::new();
        if self.project.is_open() {
            items.push(CtxItem::item("autosave.project", t(Msg::MnAutosaveProject)));
        }
        items.push(CtxItem::item("autosave.backups", t(Msg::MnAutosaveBackups)));
        items.push(CtxItem::Separator);
        items.push(CtxItem::item(
            "autosave.settings",
            t(Msg::MnAutosaveSettings),
        ));
        let r = self.status_autosave_rect;
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(r);
        self.status_menu.set_scale(self.scale);
        // 폭 인자 = **최소 폭**(그릴 때 실측 글 폭이 더 크면 늘어난다) → 작게 주어 언어별 문구 길이에 맞춘다(사용자 09-28).
        self.status_menu
            .open_at(r.x, r.y, items, host, px(80.0, self.scale));
    }

    pub(crate) fn open_eol_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        // Sublime식 3종(사용자 09-16 캡처): Windows CRLF · Unix LF · Mac OS 9 CR · 현재 = ✓ + 강조색.
        let cur = self.editors.active_eol();
        let mark = |on: bool, s: &str| {
            if on {
                format!("✓ {s}")
            } else {
                format!("   {s}")
            }
        };
        let it = |id: &str, m: Msg, e: eol::Eol| {
            CtxItem::item(id, mark(cur == e, t(m))).with_active(cur == e)
        };
        let items = vec![
            it("eol.crlf", Msg::MnEolCrlf, eol::Eol::Crlf),
            it("eol.lf", Msg::MnEolLf, eol::Eol::Lf),
            it("eol.cr", Msg::MnEolCr, eol::Eol::Cr),
        ];
        let r = self.status_eol_rect;
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(r);
        self.status_menu.set_scale(self.scale);
        self.status_menu
            .open_at(r.x, r.y, items, host, px(80.0, self.scale));
    }

    /// 상태줄 인코딩 팝업(사용자 09-16 · Sublime 두 메뉴를 한 팝업에): 위 = "다른 인코딩으로 다시 열기 ▸"(파일 탭일 때만 ·
    /// 파일을 그 인코딩으로 다시 디코드) · 아래 = 저장 인코딩 목록(현재 = ✓ 강조). 유니코드 4종 뒤 구분선.
    pub(crate) fn open_enc_menu(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let cur = self.editors.active_encoding();
        let mark = |on: bool, s: &str| {
            if on {
                format!("✓ {s}")
            } else {
                format!("   {s}")
            }
        };
        let list = |prefix: &str, with_mark: bool| -> Vec<CtxItem> {
            let mut v: Vec<CtxItem> = Vec::new();
            for (i, e) in enc::LIST.iter().enumerate() {
                if i == enc::UNICODE_COUNT {
                    v.push(CtxItem::Separator);
                }
                let label = if with_mark {
                    mark(cur == e.id, t(e.label))
                } else {
                    t(e.label).to_string()
                };
                let item = CtxItem::item(format!("{prefix}{}", e.id), label);
                v.push(if with_mark {
                    item.with_active(cur == e.id)
                } else {
                    item
                });
            }
            v
        };
        let mut items: Vec<CtxItem> = Vec::new();
        if self.editors.active_path().is_some() {
            items.push(CtxItem::submenu(
                "enc.reopen",
                t(Msg::MnReopenEnc),
                list("enc.reopen:", false),
            ));
            items.push(CtxItem::Separator);
        }
        items.extend(list("enc.set:", true));
        let r = self.status_enc_rect;
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(r);
        self.status_menu.set_scale(self.scale);
        self.status_menu
            .open_at(r.x, r.y, items, host, px(80.0, self.scale));
    }

    pub(crate) fn indent_pick(&mut self, id: &str) {
        if id.starts_with("close.") {
            self.close_pick(id);
            return;
        }
        // 상태줄 자동 저장 메뉴의 답(사용자 09-28).
        if let Some(rest) = id.strip_prefix("autosave.") {
            self.autosave_pick(rest);
            return;
        }
        // 거터(북마크/니모닉 영역) 우클릭 메뉴의 답(사용자 09-23).
        if let Some(rest) = id.strip_prefix("bmg.") {
            self.bm_gutter_pick(rest);
            return;
        }
        // 종료 전 프로젝트 저장 물음의 답(사용자 09-23).
        if id == "project.exit_save" || id == "project.exit_skip" {
            if id == "project.exit_save" {
                if let Err(e) = self.project_save() {
                    self.sess.status = tf(Msg::ErrProjectFile, &[&e]);
                }
            }
            self.exit_project_asked = true;
            self.request_exit();
            self.redraw();
            return;
        }
        // 툴바 Disconnect 드롭다운(공유 연결 목록 · docs/52 §7).
        if id.starts_with("conn.drop")
            || id.starts_with("conn.use:")
            || id.starts_with("conn.again:")
        {
            self.disconnect_pick(id);
            return;
        }
        let (ts, spaces) = self.editors.indent();
        if let Some(e) = id.strip_prefix("enc.set:") {
            self.editors.set_active_encoding(e);
            self.sess.status = tf(Msg::StEncSet, &[&enc::label(e)]);
            return;
        }
        if let Some(e) = id.strip_prefix("enc.reopen:") {
            if let Some(path) = self.editors.active_path() {
                let e = e.to_string();
                self.open_file_enc(&path, &e);
            }
            return;
        }
        if id == "demo.create" {
            self.start_demo_create();
            return;
        }
        if id == "demo.later" || id == "demo.ask" {
            return;
        }
        if let Some(gid) = id.strip_prefix("tbg:") {
            if self.tool_dock.is_floating(gid) {
                self.dock_group(gid);
            } else {
                // 커서 자리에 띄운다(창은 이벤트 루프 핸들에서).
                let at = Some(self.cursor);
                self.pending_float.push((gid.to_string(), at));
            }
            return;
        }
        if id == "tb.reset" {
            self.reset_toolbar();
            return;
        }
        if let Some(tid) = id.strip_prefix("tb:") {
            let mut hidden = self.hidden_toolbar_ids();
            if let Some(i) = hidden.iter().position(|h| h == tid) {
                hidden.remove(i);
            } else {
                hidden.push(tid.to_string());
            }
            let _ = self.settings.set("toolbar.hidden", &hidden.join(","));
            self.persist_settings();
            self.apply_toolbar_visibility();
            self.layout();
            return;
        }
        if let Some(rest) = id.strip_prefix("multi.") {
            self.multi_pick(rest);
            self.redraw();
            return;
        }
        if let Some(rest) = id.strip_prefix("tx.") {
            self.tx_pick(rest);
            self.redraw();
            return;
        }
        if let Some(rest) = id.strip_prefix("disc.") {
            self.disc_pick(rest);
            self.redraw();
            return;
        }
        match id {
            "eol.lf" => self.editors.set_active_eol(eol::Eol::Lf),
            "eol.crlf" => self.editors.set_active_eol(eol::Eol::Crlf),
            "eol.cr" => self.editors.set_active_eol(eol::Eol::Cr),
            "indent.spaces" | "indent.tabs" => {
                self.editors.set_tab_indent(ts, id == "indent.spaces");
            }
            "indent.to_spaces" => self.editors.convert_indent(true),
            "indent.to_tabs" => self.editors.convert_indent(false),
            s if s.starts_with("indent.size:") => {
                let n = s["indent.size:".len()..]
                    .parse::<u8>()
                    .unwrap_or(4)
                    .clamp(1, 8);
                self.editors.set_tab_indent(n, spaces);
            }
            _ => return,
        }
        self.redraw();
    }

    /// 우클릭 메뉴 전부 닫기(풀다운과 배타 · 사용자 09-22).
    pub(crate) fn close_context_menus(&mut self) {
        self.editors.close_menus();
        self.status_menu.close();
        self.explorer.close_menu();
        self.grid.close_menu();
        self.panel.close_menu();
    }

    /// 설정 `sql.key_mode`.
    pub(crate) fn key_mode(&self) -> nsql_io::KeyMode {
        self.settings
            .get("sql.key_mode")
            .and_then(nsql_io::KeyMode::parse)
            .unwrap_or(nsql_io::KeyMode::Pk)
    }

    /// 단축키로 온 명령 — 팔레트 토글·로그 창·접속 창처럼 이벤트 루프 핸들이 필요한 것만 여기서, 나머지는 [`Self::menu_action`].
    pub(crate) fn key_command(&mut self, id: &str, el: &ActiveEventLoop) {
        match id {
            "view.palette" => {
                if self.palette.is_open() {
                    self.palette.close();
                    self.redraw();
                } else {
                    self.open_palette("");
                }
            }
            "view.log" => self.toggle_log_window(el),
            "view.txlog" | "tx.log" => self.open_txlog_window(el),
            "view.sessions" => self.open_sessions_window(el),
            "view.memory" => {
                if self.mem_win.is_open() {
                    self.mem_win.close();
                    self.persist_window_sizes(false);
                } else {
                    self.open_mem_window(el);
                }
            }
            "view.variables" => self.open_vars_window(el),
            "view.on_top" => {
                let on = !self.settings.flag("window.always_on_top");
                let _ = self
                    .settings
                    .set("window.always_on_top", if on { "on" } else { "off" });
                let _ = self.settings.save();
                self.apply_on_top();
                self.sess.status = tf(Msg::StOnTop, &[if on { "on" } else { "off" }]);
                self.redraw();
            }
            "conn.toggle" => self.open_conn_window(el),
            _ => self.menu_action(id),
        }
    }

    /// 메뉴·툴바 액션(id = 메뉴 항목 값 · 툴바 항목 id — 같은 어휘).
    pub(crate) fn menu_action(&mut self, id: &str) {
        // ★ 한글 앱 조합 중이면 명령 앞에서 음절을 확정한다(09-27 IME 전수 조사 — 종전에는 ⌘S/⌘R이 조합 중 음절을 빼고
        //   저장·실행했다). 조합 중이 아니면 비용 0 · 시스템 IME(Windows·Linux)는 OS가 같은 일을 한다.
        if let Some(tb) = self.focused_textbox() {
            let mut inv = Invalidations::default();
            if tb.commit_composition(&mut inv) {
                self.redraw();
            }
        }
        // 적재 취소(Esc와 같은 길 · 팔레트/자동화용) — 활성 탭의 적재만.
        if id == "file.load_cancel" {
            self.file_load_cancel_active();
            return;
        }
        // ★ 적재 중인 탭(사용자 09-20): 그 탭의 본문을 쓰는 명령(편집·찾기·실행·저장)만 막는다 — 새 탭·열기·보기·접속은 그대로.
        if self.editors.active_loading() && fileload::blocked_while_loading(id) {
            self.sess.status = t(Msg::StLoadTabBusy).into();
            self.redraw();
            return;
        }
        // ★ 프로젝트 명령(docs/67 §2-2 · `project.*` 전부 — 접미 인자가 붙는 것도) — 메뉴바·팔레트·툴바·패널 링크·기동 명령이
        //   전부 이 한 길로(🔧 09-22 사용자: 풀다운 "새 프로젝트 저장…"이 무반응 — 패널 링크만 `project_cmd`로 갔다).
        if id.starts_with("project.") {
            self.project_cmd(id);
            return;
        }
        match id {
            "file.new" => {
                if self.tab_room() {
                    self.editors.new_tab(None);
                    self.set_focus(Focus::Editor);
                    self.on_new_tab();
                }
            }
            "file.exit" => self.request_exit(),
            // ★ 파일 열기/저장(T-74) — 자체 대화상자(nexa-dlg) · 네이티브 0.
            "file.open" => self.open_file_dlg = Some(PickerMode::Open),
            "file.save" => match self.editors.active_path() {
                Some(p) => self.save_to(&p),
                None => self.open_file_dlg = Some(PickerMode::Save),
            },
            "file.save_as" => self.open_file_dlg = Some(PickerMode::Save),
            "file.follow" => self.ext_toggle_follow(),
            // 큰 파일 열기 선택(팔레트 목록 · docs/59 §4 3단계).
            "bigfile.open" | "bigfile.readonly" | "bigfile.head" | "bigfile.run" => {
                // 팔레트 밖(기동 명령 · 단축키)에서 와도 선택 목록은 닫는다.
                self.palette.close();
                if let Some((path, enc, _)) = self.big_pending.take() {
                    let mode = match id {
                        "bigfile.readonly" => LoadMode::ReadOnly,
                        "bigfile.head" => LoadMode::Head(
                            (self.settings.int("file.large_head_mb").max(1) as u64) << 20,
                        ),
                        "bigfile.run" => LoadMode::Run,
                        _ => LoadMode::Open,
                    };
                    self.load_file(&path, &enc, mode);
                }
            }
            // 디스크에서 바로 실행 — 파일 대화상자로 고른 뒤 편집기에 싣지 않고 돌린다.
            "file.run_file" => {
                self.file_purpose = FilePurpose::RunFile;
                self.open_file_dlg = Some(PickerMode::Open);
            }
            // 큰 파일 모드의 기능 축소를 이 탭에서만 풀거나 다시 건다.
            "file.large_force" => {
                self.sess.status = match self.editors.toggle_large_force() {
                    Some(true) => t(Msg::StLargeForced).into(),
                    Some(false) => t(Msg::StLargeRestored).into(),
                    None => t(Msg::StLargeNotLarge).into(),
                };
                self.layout();
                self.redraw();
            }
            // 메모리 지금 정리(팔레트) — 보이지 않는 탭의 그리기 캐시 + 힙 → OS. 결과·본문은 건드리지 않는다.
            "mem.trim_now" => {
                let (before, _) = memtrim::usage();
                self.mem_trim("manual");
                let (after, _) = memtrim::usage();
                self.sess.status = tf(
                    Msg::StMemTrimmed,
                    &[&nsql_core::fmt_bytes(before), &nsql_core::fmt_bytes(after)],
                );
                self.redraw();
            }
            id if id.starts_with("tab:") => {
                if let Ok(tid) = id["tab:".len()..].parse::<u64>() {
                    self.editors.switch_to_id(tid);
                    self.set_focus(Focus::Editor);
                    self.layout();
                }
            }
            id if id.starts_with("file.recent:") => {
                let i: usize = id["file.recent:".len()..].parse().unwrap_or(usize::MAX);
                if let Some(p) = self.recent_files().get(i).cloned() {
                    self.open_file(&p);
                }
            }
            "edit.cut" => self.clip_action(EditCtxAction::Cut),
            // ★ 객체 상세 패널에 포커스 = 선택 글 복사(docs/86).
            "edit.copy" if self.focus == Focus::Details => {
                if let Some(sel) = self.objdetail.copy_selection() {
                    if !clipboard::write_text(&sel) {
                        self.sess.status = t(Msg::ErrClipboard).into();
                    }
                }
            }
            "edit.copy" => self.clip_action(EditCtxAction::Copy),
            "edit.paste" => self.clip_action(EditCtxAction::Paste),
            "edit.select_all" => {
                if self.focus == Focus::Grid && self.grid.editing_cell() {
                    self.grid.live_select_all();
                    self.redraw();
                } else if self.focus == Focus::Grid {
                    self.grid.select_all();
                } else {
                    self.route(InputEvent::SelectAll);
                }
            }
            // ★ 탐색기에 포커스가 있으면 ⌘F/Ctrl+F = 객체 필터 상자(docs/28 §7 · 사용자 09-25).
            "edit.find" if self.focus == Focus::Explorer => {
                self.explorer.focus_filter();
                self.redraw();
            }
            "edit.find" | "edit.replace" => {
                if id == "edit.replace" && self.find.is_visible() && self.focus == Focus::Find {
                    // 열린 채 Ctrl+H = 바꾸기 줄 펼침/접기(VS Code).
                    self.find.toggle_replace();
                    self.layout();
                    self.redraw();
                } else {
                    let seed = self.editors.cur().copy_selection();
                    self.find.open(id == "edit.replace", seed);
                    self.find
                        .set_tooltip_delay(self.settings.int("ui.tooltip_delay_ms").max(0) as u128);
                    self.layout();
                    self.set_focus(Focus::Find);
                    self.find_step(true, false);
                }
            }
            // ★ Sublime Ctrl+D — 캐럿 밑 단어 → 다음 출현을 추가 선택(다중 커서 · 09-15).
            // 코드 완성 수동 트리거(Ctrl+Space · docs/76) · Goto Symbol(Ctrl+R) · 심볼 이동.
            "edit.complete" => self.intel_request(true),
            "goto.symbol" => self.open_goto_symbol(),
            id if id.starts_with("sym:") => {
                if let Ok(b) = id[4..].parse::<usize>() {
                    self.goto_byte(b);
                }
            }
            "edit.goto_bracket" => {
                if self.editors.cur_mut().goto_bracket(false) {
                    self.redraw();
                }
            }
            "edit.bracket_prev"
            | "edit.bracket_next"
            | "edit.bracket_parent"
            | "edit.bracket_child" => self.run_extension_cmd(id),
            "ext.enable_mgr" | "ext.disable_mgr" | "ext.install" | "ext.remove" | "ext.list"
            | "ext.enable" | "ext.disable" | "ext.repo_add" | "ext.repo_list"
            | "ext.repo_remove" => self.ext_command(id),
            x if x.starts_with("ext.") => self.ext_pick(x),
            "edit.expand_brackets" => {
                if self.editors.cur_mut().expand_to_brackets() {
                    self.redraw();
                }
            }
            "edit.expand_selection" => {
                // ★ 다중 선택 구간 수 상한 `editor.max_occurrences`(docs/72 §2 · 09-22 전까지 키만 있고 미배선).
                let cap = self.occurrence_cap();
                let n0 = self.editors.selection_count();
                if n0 >= cap {
                    self.sess.status = tf(Msg::StOccurrenceCap, &[&n0.to_string()]);
                } else if self.editors.cur_mut().select_next_occurrence() {
                    let n = self.editors.selection_count();
                    if n > 1 {
                        self.sess.status = tf(Msg::StSelections, &[&n.to_string()]);
                    }
                }
                self.set_focus(Focus::Editor);
            }
            // ★ Quick Skip Next(Ctrl+K,Ctrl+D · 사용자 09-22): 마지막 출현을 버리고 다음 출현으로.
            "edit.skip_occurrence" => {
                if self.editors.cur_mut().skip_next_occurrence() {
                    let n = self.editors.selection_count();
                    self.sess.status = tf(Msg::StSelections, &[&n.to_string()]);
                }
                self.set_focus(Focus::Editor);
            }
            // 같은 문자열 전부 선택(Sublime Ctrl+⇧D 계열 · 상한 = 더 못 찾을 때까지).
            "edit.select_all_occurrences" => {
                let cap = self.occurrence_cap();
                let ed = self.editors.cur_mut();
                let mut capped = false;
                if ed.select_next_occurrence() {
                    // 상한(`editor.max_occurrences`)에서 멈춘다 — 구간마다 캐럿·편집이 곱해진다(docs/72 §2).
                    while ed.selection_count() < cap && ed.select_next_occurrence() {}
                    capped = ed.selection_count() >= cap;
                }
                let n = self.editors.selection_count();
                self.sess.status = if capped {
                    tf(Msg::StOccurrenceCap, &[&n.to_string()])
                } else {
                    tf(Msg::StSelections, &[&n.to_string()])
                };
                self.set_focus(Focus::Editor);
            }
            // ★ Sublime 줄·선택 편집(T-98 · 09-16) — 편집기에 포커스일 때만.
            "edit.duplicate_line" => self.editor_cmd(EditCommand::DuplicateLines),
            "edit.delete_line" => self.editor_cmd(EditCommand::DeleteLines),
            "edit.join_lines" => self.editor_cmd(EditCommand::JoinLines),
            "edit.swap_line_up" => self.editor_cmd(EditCommand::SwapLinesUp),
            "edit.swap_line_down" => self.editor_cmd(EditCommand::SwapLinesDown),
            "edit.toggle_comment" => self.editor_cmd(EditCommand::ToggleComment),
            "edit.toggle_block_comment" => self.editor_cmd(EditCommand::ToggleBlockComment),
            // ★ SQL 포맷(docs/95 · 사용자 09-28): 기본 포맷터 / 고르기 / 미리보기 · 팔레트 항목 `format.<동사>:<엔진>`.
            "edit.format" => self.format_sql_cmd(),
            "edit.format_with" => self.format_with_cmd(),
            "edit.format_preview" => self.format_preview_cmd(),
            x if x.starts_with("format.") => self.format_pick(x),
            "intel.refresh" | "intel.refresh_server" | "intel.refresh_all" => {
                self.intel_refresh(id)
            }
            "edit.indent" => self.editor_cmd(EditCommand::Indent),
            "edit.unindent" => self.editor_cmd(EditCommand::Unindent),
            "edit.select_line" => self.editor_cmd(EditCommand::SelectLines),
            "edit.split_lines" => self.editor_cmd(EditCommand::SplitIntoLines),
            "edit.add_caret_up" => self.editor_cmd(EditCommand::AddCaretUp),
            "edit.add_caret_down" => self.editor_cmd(EditCommand::AddCaretDown),
            "edit.upper_case" => self.editor_cmd(EditCommand::UpperCase),
            "edit.lower_case" => self.editor_cmd(EditCommand::LowerCase),
            // Goto Anything(T-96): 탭 · 최근 파일 · `:줄`.
            "view.goto_anything" | "tab.find" => self.open_goto_anything(""),
            "edit.goto_line" => self.open_goto_anything(":"),
            id if id.starts_with("goto.line:") => {
                if let Ok(n) = id["goto.line:".len()..].parse::<usize>() {
                    self.ed_mut().goto_line(n);
                    self.set_focus(Focus::Editor);
                }
            }
            // 줄끝 변환 메뉴(T-89 잔여) — 상태줄 팝업과 같은 경로.
            id if id.starts_with("eol.") => self.indent_pick(id),
            "edit.find_next" => self.find_step(true, true),
            "edit.find_prev" => self.find_step(false, true),
            "edit.next_statement" => self.goto_statement(true),
            "edit.prev_statement" => self.goto_statement(false),
            "edit.undo" => self.route(InputEvent::Undo),
            "edit.redo" => self.route(InputEvent::Redo),
            // ★ 선택 되돌리기(Sublime soft undo · 사용자 09-22) — 편집기에서만.
            "edit.soft_undo" | "edit.soft_redo" => {
                let ed = self.editors.cur_mut();
                let ok = if id == "edit.soft_undo" {
                    ed.soft_undo()
                } else {
                    ed.soft_redo()
                };
                if ok {
                    let n = self.editors.selection_count();
                    if n > 1 {
                        self.sess.status = tf(Msg::StSelections, &[&n.to_string()]);
                    }
                }
                self.set_focus(Focus::Editor);
            }
            "view.log" => self.toggle_log = true,
            "view.toolbar_reset" => self.reset_toolbar(),
            "view.layout_reset" => self.reset_layout(),
            "view.colors" => self.open_colors = true,
            "view.keys" => self.open_keys = true,
            "view.extensions" => {
                if !self.settings.flag("extensions.enabled") {
                    self.sess.status = t(Msg::StExtManagerOff).into();
                    self.redraw();
                    return;
                }
                let on = !self.ext_panel.is_visible();
                if on {
                    self.side_panel_close_others("view.extensions");
                }
                self.ext_panel.set_visible(on);
                if on {
                    self.ext_panel.focus_query();
                    self.set_focus(Focus::Ext);
                    self.ext_panel_sync();
                    self.ext_fetch_start();
                } else if self.focus == Focus::Ext {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
            }
            "view.bookmarks" => {
                let on = !self.bm_panel.is_visible();
                if on {
                    self.side_panel_close_others("view.bookmarks");
                    self.bm_panel.sync(&self.bookmarks.store);
                }
                self.bm_panel.set_visible(on);
                if on {
                    self.bm_panel.focus_filter();
                    self.set_focus(Focus::Bookmarks);
                } else if self.focus == Focus::Bookmarks {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
                self.redraw();
            }
            x if x.starts_with("bookmark.") => self.bookmark_cmd(x),
            // 아웃라인 패널(docs/76): 켜면 활성 탭 심볼 동기 + 필터 포커스 · 다른 좌측 패널은 닫힌다.
            "view.object_details" => {
                let on = !self.settings.flag("explorer.details");
                let _ = self
                    .settings
                    .set("explorer.details", if on { "on" } else { "off" });
                self.persist_settings();
                if on && !self.explorer.is_visible() {
                    self.side_panel_close_others("view.explorer");
                    self.explorer.set_visible(true);
                }
                self.layout();
                self.sync_detail_target(true);
                self.redraw();
            }
            "view.outline" => {
                let on = !self.outline_panel.is_visible();
                if on {
                    self.side_panel_close_others("view.outline");
                }
                self.outline_panel.set_visible(on);
                if on {
                    self.outline_sync();
                    self.outline_panel.focus_filter();
                    self.set_focus(Focus::Outline);
                } else if self.focus == Focus::Outline {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
                self.redraw();
            }
            "view.project" => {
                let on = !self.project_panel.is_visible();
                if on {
                    self.side_panel_close_others("view.project");
                    self.sync_project_panel_opts();
                }
                self.project_panel.set_visible(on);
                if on {
                    self.project_panel.focus_filter();
                    self.set_focus(Focus::Project);
                } else if self.focus == Focus::Project {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
            }
            "view.search" => {
                let on = !self.search.is_visible();
                if on {
                    self.side_panel_close_others("view.search");
                }
                if on && self.explorer.is_visible() {
                    self.explorer.set_visible(false);
                    let _ = self.settings.set("explorer.visible", "off");
                    let _ = self.settings.save();
                }
                self.search.set_visible(on);
                if on {
                    let seed = self.editors.cur().copy_selection();
                    self.search.focus_query(seed);
                    self.search
                        .set_tooltip_delay(self.settings.int("ui.tooltip_delay_ms").max(0) as u128);
                    self.set_focus(Focus::Search);
                } else if self.focus == Focus::Search {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
            }
            "view.explorer" => {
                let on = !self.explorer.is_visible();
                if on {
                    self.side_panel_close_others("view.explorer");
                }
                if on && self.search.is_visible() {
                    self.search.set_visible(false);
                    if self.focus == Focus::Search {
                        self.set_focus(Focus::Editor);
                    }
                }
                self.explorer.set_visible(on);
                let _ = self
                    .settings
                    .set("explorer.visible", if on { "on" } else { "off" });
                let _ = self.settings.save();
                if !on && self.focus == Focus::Explorer {
                    self.set_focus(Focus::Editor);
                }
                self.layout();
            }
            "file.close_tab" => {
                let i = self.editors.active();
                self.close_tab_guarded(i);
                // 마지막 탭은 닫히지 않고 비워진다(탭 수가 그대로라 틱이 못 본다) → 여기서 회수를 예약.
                self.mem_released();
                self.set_focus(Focus::Editor);
            }
            "tab.next" | "tab.prev" => {
                let n = self.editors.len();
                if n > 1 {
                    let i = self.editors.active();
                    let j = if id == "tab.next" {
                        (i + 1) % n
                    } else {
                        (i + n - 1) % n
                    };
                    self.editors.switch(j);
                }
            }
            "view.theme" => self.cycle_theme(),
            "view.lang" => self.toggle_lang(),
            "view.palette" => self.open_palette(""),
            id if id.starts_with("syntax.set:") => {
                let name = &id["syntax.set:".len()..];
                if self.editors.set_syntax(name) {
                    self.sess.status = tf(Msg::StSyntaxSet, &[name]);
                }
            }
            "run.statement" => self.run_sql(false),
            // Ctrl+\ = 새 결과 탭에 실행(T-93 · 끄면 Ctrl+Enter와 같다 · D-73).
            // 이 탭의 변수(탭 층 + 연결 공유 층)를 **실행할 수 있는 스크립트**로 새 편집기 탭에(내보내기 = 저장 · 가져오기 = 실행 · D-136).
            "vars.script" => {
                let mut all = self
                    .tab_vars
                    .get(&self.editors.active_id())
                    .cloned()
                    .unwrap_or_default();
                all.extend(self.sess.shared_vars.iter().cloned());
                all.extend(self.global_vars.iter().cloned());
                let text = nsql_script::vars_to_script(&all);
                if self.tab_room() {
                    self.editors.new_tab(None);
                    self.editors.cur_mut().set_text(&text);
                    self.set_focus(Focus::Editor);
                    self.redraw();
                }
            }
            // 이 탭의 변수 표를 새 결과 탭으로(`SHOW VARIABLES` — 러너가 탭 층 + 공유 층 + 프로필 층을 한 표로 낸다 · docs/63 V2).
            "vars.show" => {
                if !self.sess.busy && self.gate_open() {
                    let prev = self.fresh_result_tab_for_run();
                    self.run_text("SHOW VARIABLES".to_string(), 0, true);
                    self.mark_fresh_run_tab(prev);
                }
            }
            "run.statement_new_tab" => {
                let prev = if self.sess.busy {
                    None
                } else {
                    self.fresh_result_tab_for_run()
                };
                self.run_sql(false);
                self.mark_fresh_run_tab(prev);
            }
            "result.tab.close" => {
                let i = self.panel.active;
                self.panel_action(ResultAction::Close(i));
            }
            "result.tab.next" | "result.tab.prev" => {
                let n = self.panel.tabs.len();
                if n > 1 {
                    let i = self.panel.active;
                    let j = if id == "result.tab.next" {
                        (i + 1) % n
                    } else {
                        (i + n - 1) % n
                    };
                    self.panel_action(ResultAction::Activate(j));
                }
            }
            "run.all" => self.run_sql(true),
            "run.stop" => self.stop_run(),
            "run.explain" => self.run_explain(),
            "run.commit" | "run.rollback" => {
                if self.gate_open() {
                    self.sess.busy = true;
                    self.sess.touch();
                    self.sess.worker.send(if id == "run.commit" {
                        worker::Cmd::Commit
                    } else {
                        worker::Cmd::Rollback
                    });
                    self.sync_gate();
                }
            }
            // 트랜잭션 로그 창(T-107)이 오기 전까지는 상태줄 트랜잭션 팝업(모드 전환 · Commit(n) · Rollback(n) · 대기 목록).
            "tx.log" | "view.txlog" => self.open_txlog = true,
            // 접속 창 열기 — 연결 중이어도 끊지 않고 그냥 연다(사용자 09-14). 끊기는 폼의 Disconnect 버튼.
            "conn.toggle" => self.open_conn = true,
            // 툴바 Disconnect = **지금 탭의 연결 해제**(종전과 같음 · 사용자 09-18 원복) · 세션 목록 버튼/View = 세션 창.
            "conn.disconnect" => {
                self.sess.disc_path = Some(sessions::DiscPath::Toolbar);
                self.disconnect_now();
            }
            // 세션 목록 = 토글(사용자 09-19): 열려 있으면 닫고 · 아니면 연다.
            "view.variables" => {
                if self.vars_win.is_open() {
                    self.vars_win.close();
                } else {
                    self.open_vars = true;
                    // 창은 다음 `about_to_wait`에서 만든다 — 이벤트가 없으면 그 틱이 오지 않으므로 깨운다.
                    self.redraw();
                }
            }
            "view.memory" => self.toggle_mem_window(),
            "help.license" => {
                if self.license_win.is_open() {
                    self.license_win.close();
                    self.sync_modal();
                } else {
                    self.open_license = true;
                    self.redraw();
                }
            }
            "sess.tab" | "sess.tab#drop" => self.open_tab_conn_menu(),
            "conn.sessions" | "view.sessions" => {
                if self.sessions_win.is_open() {
                    self.sessions_win.close();
                    self.persist_window_sizes(false);
                } else {
                    self.open_sessions = true;
                }
            }
            id if id.starts_with("conn.drop")
                || id.starts_with("conn.use:")
                || id.starts_with("conn.again:") =>
            {
                self.disconnect_pick(id);
            }
            "edit.prefs" => self.open_prefs = true,
            "edit.settings_json" => self.edit_settings_json(),
            "help.demo" => self.start_demo_create(),
            "help.about" => {
                // About 창(종전 = 상태줄 한 줄뿐이라 "미동작"으로 보였다 · 사용자 09-27).
                if self.about_win.is_open() {
                    self.about_win.close();
                    self.sync_modal();
                } else {
                    self.open_about = true;
                    self.redraw();
                }
            }
            _ => {}
        }
        self.redraw();
    }

    pub(crate) fn open_status_popup(
        &mut self,
        r: Rect,
        items: Vec<nexa_ctl::controls::ctxmenu::CtxItem>,
    ) {
        self.open_status_popup_w(r, items, 80.0);
    }

    /// 폭을 지정하는 판 — 거터 북마크 메뉴처럼 짧은 항목만 있는 팝업은 좁게(사용자 09-23 "메뉴 폭이 너무 넓어").
    pub(crate) fn open_status_popup_w(
        &mut self,
        r: Rect,
        items: Vec<nexa_ctl::controls::ctxmenu::CtxItem>,
        width: f32,
    ) {
        let host = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or(r);
        self.status_menu.set_scale(self.scale);
        self.status_menu
            .open_at(r.x, r.y, items, host, px(width, self.scale));
    }

    /// 미커밋 탭 닫기(설정 `tx.close_action`: ask / commit / rollback).
    /// 탭 우클릭 메뉴(09-17): 이름 바꾸기(팔레트 프롬프트) · 닫기 계열(높은 index부터 · 미커밋/미저장 가드는 탭마다) · 파일 위치 열기.
    pub(crate) fn tab_menu_request(&mut self, req: editors::TabMenuReq) {
        use editors::TabMenuReq;
        let n = self.editors.tab_count();
        match req {
            TabMenuReq::Rename(i) => {
                let title = self.editors.title_of(i);
                // 이름을 바꾸는 탭 바로 아래에 붙이고 그 탭을 강조한다(사용자 09-21).
                let anchor = self.editors.tab_rect(i);
                self.palette.open_prompt_at(
                    &format!("tab.rename:{i}"),
                    t(Msg::PhTabRename),
                    &title,
                    anchor,
                );
            }
            TabMenuReq::Close(i) => self.close_tab_guarded(i),
            TabMenuReq::CloseLeft(i) => {
                for j in (0..i.min(n)).rev() {
                    self.close_tab_guarded(j);
                }
            }
            TabMenuReq::CloseRight(i) => {
                for j in ((i + 1)..n).rev() {
                    self.close_tab_guarded(j);
                }
            }
            TabMenuReq::CloseAll => {
                for j in (0..n).rev() {
                    self.close_tab_guarded(j);
                }
            }
            TabMenuReq::Reveal(i) => {
                if let Some(path) = self.editors.path_of(i) {
                    if let Err(e) = nexa_fs::shell::reveal_in_file_manager(&path) {
                        self.sess.status = tf(Msg::StRevealFailed, &[&e.to_string()]);
                    }
                }
            }
            TabMenuReq::CopyName(i) | TabMenuReq::CopyPath(i) => {
                if let Some(path) = self.editors.path_of(i) {
                    let full = matches!(req, TabMenuReq::CopyPath(_));
                    self.copy_path_text(&path, full);
                }
            }
            TabMenuReq::KeepOpen(i) => {
                self.editors.promote_tab(i);
            }
            TabMenuReq::RevealProject(i) => {
                if let Some(path) = self.editors.path_of(i) {
                    self.reveal_in_project(&path);
                }
            }
        }
    }

    /// 편집기 편집 명령 — 편집기 포커스일 때만 · 바뀌면 찾기 표시 갱신 + 상태줄 선택 수.
    fn editor_cmd(&mut self, cmd: EditCommand) {
        if self.focus != Focus::Editor {
            return;
        }
        if self.ed_mut().edit_command(cmd) {
            let n = self.editors.selection_count();
            if n > 1 {
                self.sess.status = tf(Msg::StSelections, &[&n.to_string()]);
            }
        }
    }

    pub(crate) fn build_menus() -> Vec<MenuDef> {
        Self::build_menus_with(&[], &[], false, false, Vec::new())
    }

    /// 메뉴 정의 — File 메뉴 아래쪽에 최근 파일(최대 8 · Eclipse/DBeaver 관례).
    /// `blocked` = 지금 탭의 세션이 작업 중(docs/52 §3 통제) — Run 메뉴의 실행 계열은 비활성으로(툴바와 같은 판정 · 09-19).
    pub(crate) fn build_menus_with(
        recent: &[PathBuf],
        tabs: &[(u64, String, bool, bool)],
        demo_ready: bool,
        blocked: bool,
        project: Vec<MenuEntry>,
    ) -> Vec<MenuDef> {
        let item = |id: &str, m: Msg| MenuEntry::Item(ComboItem::new(id, t(m)));
        let gated = |id: &str, m: Msg| {
            if blocked {
                MenuEntry::Disabled(ComboItem::new(id, t(m)))
            } else {
                MenuEntry::Item(ComboItem::new(id, t(m)))
            }
        };
        let mut file = vec![
            item("file.new", Msg::MnNew),
            item("file.open", Msg::MnOpen),
            item("file.save", Msg::MnSave),
            item("file.save_as", Msg::MnSaveAs),
            MenuEntry::Separator,
            item("file.run_file", Msg::MnRunFile),
            MenuEntry::Separator,
            item("file.close_tab", Msg::MnCloseTab),
        ];
        if !recent.is_empty() {
            file.push(MenuEntry::Separator);
            for (i, p) in recent.iter().take(8).enumerate() {
                let name = p
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let label = format!("{}  {}", name, nexa_fs::path::display(p));
                file.push(MenuEntry::Item(ComboItem::new(
                    format!("file.recent:{i}"),
                    label,
                )));
            }
        }
        file.push(MenuEntry::Separator);
        file.push(item("file.exit", Msg::MnExit));
        vec![
            MenuDef::new(t(Msg::MnFile), file),
            MenuDef::new(t(Msg::MnProject), project),
            // ★ Edit 메뉴 = 그룹(하위 메뉴 · 사용자 09-22 "1레벨로 너무 길다 — Sublime/VS Code/IntelliJ 참고"):
            //   Sublime의 Edit(Line ▸ · Comment ▸ · Convert Case ▸) + Selection 메뉴 + Find 메뉴를 한 메뉴 안 그룹으로 접었다 ·
            //   자주 쓰는 Undo/Redo · Cut/Copy/Paste · Select All · Toggle Comment · Go to Line · Preferences는 1레벨 유지
            //   (VS Code Edit 메뉴와 같은 자리) · 설정 = 맨 아래(IntelliJ/Sublime Preferences 자리).
            MenuDef::new(
                t(Msg::MnEdit),
                vec![
                    item("edit.undo", Msg::MnUndo),
                    item("edit.redo", Msg::MnRedo),
                    // Sublime Edit ▸ Undo Selection ▸ Soft Undo / Soft Redo.
                    MenuEntry::sub(
                        t(Msg::MnGrpUndoSelection),
                        vec![
                            item("edit.soft_undo", Msg::MnSoftUndo),
                            item("edit.soft_redo", Msg::MnSoftRedo),
                        ],
                    ),
                    MenuEntry::Separator,
                    item("edit.cut", Msg::MnCut),
                    item("edit.copy", Msg::MnCopy),
                    item("edit.paste", Msg::MnPaste),
                    MenuEntry::Separator,
                    item("edit.select_all", Msg::MnSelectAll),
                    MenuEntry::sub(
                        t(Msg::MnGrpSelection),
                        vec![
                            item("edit.expand_selection", Msg::MnExpandSelection),
                            item("edit.skip_occurrence", Msg::MnSkipOccurrence),
                            item("edit.select_all_occurrences", Msg::MnSelectAllOccurrences),
                            MenuEntry::Separator,
                            item("edit.select_line", Msg::MnSelectLine),
                            item("edit.split_lines", Msg::MnSplitLines),
                            item("edit.add_caret_up", Msg::MnAddCaretUp),
                            item("edit.add_caret_down", Msg::MnAddCaretDown),
                            MenuEntry::Separator,
                            item("edit.expand_brackets", Msg::MnExpandBrackets),
                        ],
                    ),
                    MenuEntry::sub(
                        t(Msg::MnGrpBrackets),
                        vec![
                            item("edit.complete", Msg::MnComplete),
                            item("goto.symbol", Msg::MnGotoSymbol),
                            MenuEntry::Separator,
                            item("edit.goto_bracket", Msg::MnGotoBracket),
                            item("edit.bracket_prev", Msg::MnBracketPrev),
                            item("edit.bracket_next", Msg::MnBracketNext),
                            item("edit.bracket_parent", Msg::MnBracketParent),
                            item("edit.bracket_child", Msg::MnBracketChild),
                        ],
                    ),
                    MenuEntry::sub(
                        t(Msg::MnGrpLine),
                        vec![
                            item("edit.indent", Msg::MnIndent),
                            item("edit.unindent", Msg::MnUnindent),
                            MenuEntry::Separator,
                            item("edit.swap_line_up", Msg::MnSwapLineUp),
                            item("edit.swap_line_down", Msg::MnSwapLineDown),
                            MenuEntry::Separator,
                            item("edit.duplicate_line", Msg::MnDuplicateLine),
                            item("edit.delete_line", Msg::MnDeleteLine),
                            item("edit.join_lines", Msg::MnJoinLines),
                        ],
                    ),
                    MenuEntry::sub(
                        t(Msg::MnGrpConvertCase),
                        vec![
                            item("edit.upper_case", Msg::MnUpperCase),
                            item("edit.lower_case", Msg::MnLowerCase),
                        ],
                    ),
                    item("edit.toggle_comment", Msg::MnToggleComment),
                    item("edit.toggle_block_comment", Msg::MnToggleBlockComment),
                    MenuEntry::Separator,
                    item("edit.format", Msg::MnFormatSql),
                    item("edit.format_with", Msg::MnFormatWith),
                    item("edit.format_preview", Msg::MnFormatPreview),
                    MenuEntry::Separator,
                    // 인텔리센스 캐시 새로 고침(79 §4 · 현재 스키마 · 이 서버 · 전 서버).
                    item("intel.refresh", Msg::MnIntelRefresh),
                    item("intel.refresh_server", Msg::MnIntelRefreshServer),
                    item("intel.refresh_all", Msg::MnIntelRefreshAll),
                    MenuEntry::Separator,
                    // 북마크(docs/69 §6-1).
                    MenuEntry::sub(
                        t(Msg::MnBookmarks),
                        vec![
                            item("bookmark.toggle", Msg::MnBmToggle),
                            item("bookmark.next", Msg::MnBmNext),
                            item("bookmark.prev", Msg::MnBmPrev),
                            MenuEntry::Separator,
                            item("bookmark.select_all", Msg::MnBmSelectAll),
                            item("bookmark.label", Msg::MnBmLabel),
                            item("bookmark.clear_doc", Msg::MnBmClearDoc),
                            MenuEntry::Separator,
                            item("view.bookmarks", Msg::MnBookmarksPanel),
                        ],
                    ),
                    MenuEntry::sub(
                        t(Msg::MnGrpFind),
                        vec![
                            item("edit.find", Msg::MnFind),
                            item("edit.replace", Msg::MnReplace),
                            MenuEntry::Separator,
                            item("edit.find_next", Msg::MnFindNext),
                            item("edit.find_prev", Msg::MnFindPrev),
                        ],
                    ),
                    item("edit.goto_line", Msg::MnGotoLine),
                    MenuEntry::Separator,
                    MenuEntry::sub(
                        t(Msg::MnGrpLineEndings),
                        vec![
                            item("eol.crlf", Msg::MnEolCrlf),
                            item("eol.lf", Msg::MnEolLf),
                            item("eol.cr", Msg::MnEolCr),
                        ],
                    ),
                    MenuEntry::Separator,
                    item("edit.prefs", Msg::MnPreferences),
                ],
            ),
            MenuDef::new(
                t(Msg::MnView),
                vec![
                    item("view.palette", Msg::MnCommandPalette),
                    item("view.goto_anything", Msg::MnGotoAnything),
                    item("view.explorer", Msg::MnExplorer),
                    item("view.object_details", Msg::MnObjectDetails),
                    item("view.search", Msg::MnSearchPanel),
                    item("view.project", Msg::MnProjectPanel),
                    item("view.log", Msg::MnLogWindow),
                    item("view.txlog", Msg::MnTxLogWindow),
                    item("view.sessions", Msg::MnSessManager),
                    item("view.memory", Msg::MnMemoryWindow),
                    item("view.variables", Msg::MnVariables),
                    item("vars.show", Msg::MnShowVariables),
                    item("vars.script", Msg::MnVariablesScript),
                    item("view.on_top", Msg::MnAlwaysOnTop),
                    item("view.toolbar_reset", Msg::MnResetToolbar),
                    item("view.layout_reset", Msg::MnResetLayout),
                    MenuEntry::Separator,
                    item("view.colors", Msg::MnColors),
                    item("view.keys", Msg::MnKeys),
                    item("view.theme", Msg::MnTheme),
                    item("view.lang", Msg::MnLanguage),
                ],
            ),
            MenuDef::new(
                t(Msg::MnRun),
                vec![
                    gated("run.statement", Msg::MnRunStatement),
                    gated("run.statement_new_tab", Msg::MnRunStatementNewTab),
                    gated("run.all", Msg::MnRunAll),
                    gated("run.explain", Msg::MnExplain),
                    MenuEntry::Separator,
                    gated("run.commit", Msg::MnCommit),
                    gated("run.rollback", Msg::MnRollback),
                    MenuEntry::Separator,
                    item("conn.toggle", Msg::MnConnect),
                    item("conn.disconnect", Msg::MnDisconnect),
                ],
            ),
            // ★ 탭 메뉴(Golden Tabs · 사용자 09-16): 열린 탭 순서대로 · 활성 탭은 ✓ · 고르면 전환.
            MenuDef::new(t(Msg::MnTabs), {
                let mut v: Vec<MenuEntry> = tabs
                    .iter()
                    .map(|(id, title, active, dirty)| {
                        let mut ci = ComboItem::new(format!("tab:{id}"), title.clone());
                        if *active {
                            ci.icon = Some("✓".into());
                        }
                        // ★ 미저장 탭 = 강조색(사용자 09-26 "저장되지 않은 탭의 메뉴 색을 더 강조").
                        if *dirty {
                            MenuEntry::Emph(ci)
                        } else {
                            MenuEntry::Item(ci)
                        }
                    })
                    .collect();
                // 탭 찾기(Goto Anything · T-96 추천안 A).
                v.push(MenuEntry::Separator);
                v.push(item("tab.find", Msg::MnFindTab));
                v
            }),
            MenuDef::new(
                t(Msg::MnHelp),
                vec![
                    // 샘플 데이터(사용자 09-17): 이미 있으면 비활성.
                    if demo_ready {
                        MenuEntry::Disabled(ComboItem::new("help.demo", t(Msg::MnDemoCreate)))
                    } else {
                        item("help.demo", Msg::MnDemoCreate)
                    },
                    MenuEntry::Separator,
                    item("help.license", Msg::MnLicense),
                    item("help.about", Msg::MnAbout),
                ],
            ),
        ]
    }

    /// 탭 목록이 바뀌었으면(열기·닫기·이름·활성) 메뉴를 다시 만든다 — 드롭다운이 열려 있는 동안은 미룬다.
    pub(crate) fn sync_tabs_menu(&mut self) {
        if self.menubar.is_open() {
            return;
        }
        let tabs = self.editors.tab_list_dirty();
        let sig = tabs
            .iter()
            .map(|(id, t, a, d)| format!("{id}:{t}:{a}:{d}"))
            .collect::<Vec<_>>()
            .join("|");
        if sig != self.tabs_menu_sig {
            self.tabs_menu_sig = sig;
            let recent = self.recent_files();
            self.menubar.set_menus(App::build_menus_with(
                &recent,
                &tabs,
                self.demo_ready,
                self.gate_shown.unwrap_or(false),
                self.project_menu_entries(),
            ));
        }
    }

    /// 복사·잘라내기·붙여넣기·전체 선택 — 포커스 텍스트박스 ↔ OS 클립보드([`clipboard`]). 실패는 상태줄에.
    pub(crate) fn clip_action(&mut self, act: EditCtxAction) {
        let mut inv = Invalidations::default();
        let mut failed = false;
        match act {
            // ★ 셀 편집 중 = 클립보드 동작은 편집 상자에 한정(사용자 09-26 ⌘X/⌘A 지적).
            EditCtxAction::Copy if self.focus == Focus::Grid && self.grid.editing_cell() => {
                if let Some(t) = self.grid.live_copy() {
                    failed = !clipboard::write_text(&t);
                }
            }
            EditCtxAction::Cut if self.focus == Focus::Grid && self.grid.editing_cell() => {
                // 셀 편집도 같은 순서 — 클립보드가 받아 준 뒤에만 지운다(09-26).
                if let Some(t) = self.grid.live_copy() {
                    if clipboard::write_text(&t) {
                        self.grid.live_cut();
                    } else {
                        failed = true;
                    }
                }
            }
            EditCtxAction::Paste if self.focus == Focus::Grid && self.grid.editing_cell() => {
                match clipboard::read_text() {
                    Some(text) => self.grid.live_paste(&text),
                    None => failed = true,
                }
            }
            EditCtxAction::Copy if self.focus == Focus::Grid => {
                if let Some((text, n)) = self.grid.copy_selection(grid::CopyKind::Tsv) {
                    failed = !clipboard::write_text(&text);
                    if !failed {
                        self.sess.status = tf(Msg::StCopied, &[&n.to_string()]);
                    }
                }
            }
            // ★ 큰 선택 복사/잘라내기 확인(사용자 09-22 · docs/72 §2): 문자열을 만들기 전에 바이트 수로 판정 — 첫 누름은 안내,
            //   3초 안에 같은 동작 = 실행(거대 편집 확인과 같은 꼴).
            EditCtxAction::Copy | EditCtxAction::Cut
                if self.focus == Focus::Editor && self.copy_confirm_pending() => {}
            EditCtxAction::Copy => {
                let sel = self.focused_textbox().and_then(|tb| tb.copy_selection());
                if std::env::var_os("NSQL_TRACE_CLIP").is_some() {
                    eprintln!(
                        "[clip] copy: focus={:?} selection={}",
                        self.focus,
                        sel.as_ref()
                            .map_or("None".into(), |t| format!("{} bytes", t.len()))
                    );
                }
                if let Some(text) = sel {
                    let rich_wanted =
                        self.focus == Focus::Editor && self.settings.flag("editor.copy_rich");
                    let rich = rich_wanted && self.entitled(nsql_license::Feature::RichCopy);
                    if rich_wanted && !rich {
                        // 서식 복사는 Pro — 텍스트로 복사하고 상태줄로 안내(복사 흐름을 창으로 끊지 않는다).
                        self.sess.status =
                            self.license_denied_text(nsql_license::Feature::RichCopy);
                    }
                    let hl = self.editors.cur().highlighter().cloned();
                    failed = match (rich, hl) {
                        (true, Some(h)) => {
                            let px = self.settings.font_px("editor.font_size").round() as i32;
                            let html = nexa_ctl::to_html(
                                &text,
                                h.as_ref(),
                                &self.theme,
                                "Consolas, 'Cascadia Mono', 'D2Coding', Menlo",
                                px,
                            );
                            !clipboard::write_rich(&text, &html)
                        }
                        _ => !clipboard::write_text(&text),
                    };
                }
            }
            EditCtxAction::Cut => {
                // ★ 클립보드에 **올라간 것을 확인한 뒤에만** 지운다(사용자 09-26 Linux — 클립보드 도구가 없는 환경에서
                //   종전에는 글자가 먼저 사라지고 클립보드는 비어 있었다: 어디에도 없는 상태 · 데이터 보호 불변식 61 §1-7).
                if let Some(text) = self.focused_textbox().and_then(|tb| tb.copy_selection()) {
                    if clipboard::write_text(&text) {
                        if let Some(tb) = self.focused_textbox() {
                            tb.cut_selection(&mut inv);
                        }
                    } else {
                        failed = true;
                    }
                }
            }
            // ★ 그리드 붙여넣기(docs/87 §6): 앵커 셀부터 행렬 · 아래로 부족하면 행 자동 추가.
            EditCtxAction::Paste if self.focus == Focus::Grid && !self.grid.editing_cell() => {
                match clipboard::read_text() {
                    Some(text) => {
                        self.grid.paste_text(&text);
                        self.after_grid_event();
                    }
                    None => failed = true,
                }
            }
            EditCtxAction::Paste => match clipboard::read_text() {
                Some(text) => {
                    if let Some(tb) = self.focused_textbox() {
                        tb.paste(&text, &mut inv);
                    }
                }
                None => failed = true,
            },
            // 플러그인 메뉴 기여(우클릭 서브메뉴 항목) → 명령 id 그대로 메뉴 경로로(09-17 · extensions).
            EditCtxAction::Custom(id) => self.menu_action(&id),
        }
        if failed {
            self.sess.status = t(Msg::ErrClipboard).into();
        }
        self.redraw();
    }

    /// 큰 선택 복사/잘라내기 확인이 필요한가 — `editor.copy_confirm_mb`(0 = 안 물음) 이상이면 첫 누름은 안내(3초 무장) · 무장 중 되풀이 = 통과.
    fn copy_confirm_pending(&mut self) -> bool {
        let limit = (self.settings.int("editor.copy_confirm_mb").max(0) as usize) << 20;
        if limit == 0 {
            return false;
        }
        let bytes = self.ed_mut().selected_bytes();
        if bytes < limit {
            return false;
        }
        let now = Instant::now();
        if self.copy_armed_until.is_some_and(|t| now <= t) {
            self.copy_armed_until = None;
            return false;
        }
        self.copy_armed_until = Some(now + Duration::from_secs(3));
        self.sess.status = tf(Msg::StCopyConfirm, &[&nsql_core::fmt_bytes(bytes as u64)]);
        true
    }

    /// 명령 팔레트 열기(prefill = 초기 질의 · 예 "Set Syntax: ").
    pub(crate) fn open_palette(&mut self, prefill: &str) {
        let mut cmds: Vec<(String, String)> = Vec::new();
        let m =
            |id: &str, menu: Msg, item: Msg| (id.to_string(), format!("{}: {}", t(menu), t(item)));
        cmds.push(m("file.new", Msg::MnFile, Msg::MnNew));
        cmds.push(m("file.open", Msg::MnFile, Msg::MnOpen));
        cmds.push(m("file.save", Msg::MnFile, Msg::MnSave));
        cmds.push(m("file.save_as", Msg::MnFile, Msg::MnSaveAs));
        cmds.push(m("file.follow", Msg::MnFile, Msg::MnFileFollow));
        cmds.push(m("file.run_file", Msg::MnFile, Msg::MnRunFile));
        cmds.push(m("file.large_force", Msg::MnFile, Msg::MnLargeForce));
        cmds.push(m("mem.trim_now", Msg::MnView, Msg::MnMemTrimNow));
        cmds.push(m("file.exit", Msg::MnFile, Msg::MnExit));
        cmds.push(m("edit.cut", Msg::MnEdit, Msg::MnCut));
        cmds.push(m("edit.copy", Msg::MnEdit, Msg::MnCopy));
        cmds.push(m("edit.paste", Msg::MnEdit, Msg::MnPaste));
        cmds.push(m("edit.select_all", Msg::MnEdit, Msg::MnSelectAll));
        cmds.push(m(
            "edit.expand_selection",
            Msg::MnEdit,
            Msg::MnExpandSelection,
        ));
        cmds.push(m("edit.goto_bracket", Msg::MnEdit, Msg::MnGotoBracket));
        cmds.push(m("edit.bracket_prev", Msg::MnEdit, Msg::MnBracketPrev));
        cmds.push(m("edit.bracket_next", Msg::MnEdit, Msg::MnBracketNext));
        cmds.push(m("edit.bracket_parent", Msg::MnEdit, Msg::MnBracketParent));
        cmds.push(m("edit.bracket_child", Msg::MnEdit, Msg::MnBracketChild));
        cmds.push(m("edit.complete", Msg::MnEdit, Msg::MnComplete));
        cmds.push(m("goto.symbol", Msg::MnEdit, Msg::MnGotoSymbol));
        // Extension Manager(Sublime "Package Control: …" 표기 · docs/50 §10).
        // 관리자 상태에 맞는 명령만(사용자 09-19): 꺼짐 = "켜기" 하나 · 켜짐 = "끄기" + 나머지(두 번째 "켜기"는 없다).
        if self.settings.flag("extensions.enabled") {
            cmds.push(m(
                "ext.disable_mgr",
                Msg::MnExtensions,
                Msg::MnExtDisableManager,
            ));
            cmds.push(m("view.extensions", Msg::MnView, Msg::MnExtensionsPanel));
            cmds.push(m("ext.install", Msg::MnExtensions, Msg::MnExtInstall));
            cmds.push(m("ext.remove", Msg::MnExtensions, Msg::MnExtRemove));
            cmds.push(m("ext.list", Msg::MnExtensions, Msg::MnExtList));
            cmds.push(m("ext.enable", Msg::MnExtensions, Msg::MnExtEnable));
            cmds.push(m("ext.disable", Msg::MnExtensions, Msg::MnExtDisable));
            cmds.push(m("ext.repo_add", Msg::MnExtensions, Msg::MnExtRepoAdd));
            cmds.push(m("ext.repo_list", Msg::MnExtensions, Msg::MnExtRepoList));
            cmds.push(m(
                "ext.repo_remove",
                Msg::MnExtensions,
                Msg::MnExtRepoRemove,
            ));
        } else {
            cmds.push(m(
                "ext.enable_mgr",
                Msg::MnExtensions,
                Msg::MnExtEnableManager,
            ));
        }
        cmds.push(m(
            "edit.expand_brackets",
            Msg::MnEdit,
            Msg::MnExpandBrackets,
        ));
        cmds.push(m(
            "edit.skip_occurrence",
            Msg::MnEdit,
            Msg::MnSkipOccurrence,
        ));
        for (id, msg) in [
            ("bookmark.toggle", Msg::MnBmToggle),
            ("bookmark.next", Msg::MnBmNext),
            ("bookmark.prev", Msg::MnBmPrev),
            ("bookmark.select_all", Msg::MnBmSelectAll),
            ("bookmark.label", Msg::MnBmLabel),
            ("bookmark.clear_doc", Msg::MnBmClearDoc),
            ("view.bookmarks", Msg::MnBookmarksPanel),
        ] {
            cmds.push(m(id, Msg::MnBookmarks, msg));
        }
        cmds.push(m(
            "edit.select_all_occurrences",
            Msg::MnEdit,
            Msg::MnSelectAllOccurrences,
        ));
        cmds.push(m("edit.undo", Msg::MnEdit, Msg::MnUndo));
        cmds.push(m("edit.find", Msg::MnEdit, Msg::MnFind));
        cmds.push(m("edit.replace", Msg::MnEdit, Msg::MnReplace));
        cmds.push(m("edit.find_next", Msg::MnEdit, Msg::MnFindNext));
        cmds.push(m("edit.find_prev", Msg::MnEdit, Msg::MnFindPrev));
        cmds.push(m("edit.redo", Msg::MnEdit, Msg::MnRedo));
        cmds.push(m("edit.soft_undo", Msg::MnEdit, Msg::MnSoftUndo));
        cmds.push(m("edit.soft_redo", Msg::MnEdit, Msg::MnSoftRedo));
        // ★ 보기 메뉴의 켜고 끄는 항목은 **지금 상태 기준 한 줄**(사용자 09-27 "켜져 있으면 숨기기, 꺼져 있으면 보이기 —
        //   2줄씩 보이지 않게"): 팔레트를 여는 순간 상태를 읽어 라벨만 정한다(평소 비용 0 · 열 때 한 번).
        let tv = |id: &str, item: Msg, on: bool| {
            let label = t(item).to_string();
            let verb = tf(if on { Msg::PalHide } else { Msg::PalShow }, &[&label]);
            (id.to_string(), format!("{}: {}", t(Msg::MnView), verb))
        };
        cmds.push(tv(
            "view.explorer",
            Msg::MnExplorer,
            self.explorer.is_visible(),
        ));
        cmds.push(tv(
            "view.object_details",
            Msg::MnObjectDetails,
            self.settings.flag("explorer.details"),
        ));
        cmds.push(tv(
            "view.search",
            Msg::MnSearchPanel,
            self.search.is_visible(),
        ));
        cmds.push(tv(
            "view.project",
            Msg::MnProjectPanel,
            self.project_panel.is_visible(),
        ));
        cmds.push(tv(
            "view.bookmarks",
            Msg::MnBookmarksPanel,
            self.bm_panel.is_visible(),
        ));
        cmds.push(tv(
            "view.outline",
            Msg::MnOutlinePanel,
            self.outline_panel.is_visible(),
        ));
        cmds.push(tv("view.log", Msg::MnLogWindow, self.log_win.is_open()));
        cmds.push(tv(
            "view.txlog",
            Msg::MnTxLogWindow,
            self.txlog_win.is_open(),
        ));
        cmds.push(tv(
            "view.sessions",
            Msg::MnSessManager,
            self.sessions_win.is_open(),
        ));
        cmds.push(tv(
            "view.memory",
            Msg::MnMemoryWindow,
            self.mem_win.is_open(),
        ));
        cmds.push(tv(
            "view.variables",
            Msg::MnVariables,
            self.vars_win.is_open(),
        ));
        cmds.push(tv(
            "view.on_top",
            Msg::MnAlwaysOnTop,
            self.settings.flag("window.always_on_top"),
        ));
        cmds.push(m("view.colors", Msg::MnView, Msg::MnColors));
        cmds.push(m("view.keys", Msg::MnView, Msg::MnKeys));
        cmds.push(m("view.theme", Msg::MnView, Msg::MnTheme));
        cmds.push(m("view.lang", Msg::MnView, Msg::MnLanguage));
        cmds.push(m("view.toolbar_reset", Msg::MnView, Msg::MnResetToolbar));
        cmds.push(m("view.layout_reset", Msg::MnView, Msg::MnResetLayout));
        cmds.push(m("project.new", Msg::MnProject, Msg::MnProjectNew));
        cmds.push(m("project.open", Msg::MnProject, Msg::MnProjectOpen));
        cmds.push(m("project.switch", Msg::MnProject, Msg::MnProjectSwitch));
        cmds.push(m("project.save", Msg::MnProject, Msg::MnProjectSave));
        cmds.push(m("project.save_as", Msg::MnProject, Msg::MnProjectSaveAs));
        cmds.push(m("project.close", Msg::MnProject, Msg::MnProjectClose));
        cmds.push(m(
            "project.add_folder",
            Msg::MnProject,
            Msg::MnProjectAddFolder,
        ));
        cmds.push(m("view.variables", Msg::MnView, Msg::MnVariables));
        cmds.push(m("vars.script", Msg::MnView, Msg::MnVariablesScript));
        cmds.push(m("vars.show", Msg::MnView, Msg::MnShowVariables));
        cmds.push(m("file.close_tab", Msg::MnFile, Msg::MnCloseTab));
        cmds.push(m("tab.next", Msg::MnView, Msg::MnNextTab));
        cmds.push(m("tab.prev", Msg::MnView, Msg::MnPrevTab));
        cmds.push(m("view.theme", Msg::MnView, Msg::MnTheme));
        cmds.push(m("view.lang", Msg::MnView, Msg::MnLanguage));
        cmds.push(m("run.statement", Msg::MnRun, Msg::MnRunStatement));
        cmds.push(m(
            "run.statement_new_tab",
            Msg::MnRun,
            Msg::MnRunStatementNewTab,
        ));
        cmds.push(m("result.tab.close", Msg::MnRun, Msg::MnResultCloseTab));
        cmds.push(m("result.tab.next", Msg::MnRun, Msg::MnResultNextTab));
        cmds.push(m("result.tab.prev", Msg::MnRun, Msg::MnResultPrevTab));
        cmds.push(m("run.all", Msg::MnRun, Msg::MnRunAll));
        cmds.push(m("run.explain", Msg::MnRun, Msg::MnExplain));
        cmds.push(m("run.commit", Msg::MnRun, Msg::MnCommit));
        cmds.push(m("run.rollback", Msg::MnRun, Msg::MnRollback));
        cmds.push(m("conn.toggle", Msg::MnRun, Msg::MnConnect));
        cmds.push(m("conn.disconnect", Msg::MnRun, Msg::MnDisconnect));
        for (id, msg) in [
            ("edit.duplicate_line", Msg::MnDuplicateLine),
            ("edit.delete_line", Msg::MnDeleteLine),
            ("edit.join_lines", Msg::MnJoinLines),
            ("edit.swap_line_up", Msg::MnSwapLineUp),
            ("edit.swap_line_down", Msg::MnSwapLineDown),
            ("edit.toggle_comment", Msg::MnToggleComment),
            ("edit.toggle_block_comment", Msg::MnToggleBlockComment),
            ("intel.refresh", Msg::MnIntelRefresh),
            ("intel.refresh_server", Msg::MnIntelRefreshServer),
            ("intel.refresh_all", Msg::MnIntelRefreshAll),
            ("edit.indent", Msg::MnIndent),
            ("edit.unindent", Msg::MnUnindent),
            ("edit.select_line", Msg::MnSelectLine),
            ("edit.split_lines", Msg::MnSplitLines),
            ("edit.add_caret_up", Msg::MnAddCaretUp),
            ("edit.add_caret_down", Msg::MnAddCaretDown),
            ("edit.upper_case", Msg::MnUpperCase),
            ("edit.lower_case", Msg::MnLowerCase),
            ("edit.goto_line", Msg::MnGotoLine),
            ("eol.crlf", Msg::MnEolCrlf),
            ("eol.lf", Msg::MnEolLf),
            ("eol.cr", Msg::MnEolCr),
        ] {
            cmds.push(m(id, Msg::MnEdit, msg));
        }
        cmds.push(m("view.goto_anything", Msg::MnView, Msg::MnGotoAnything));
        cmds.push(m("edit.prefs", Msg::MnEdit, Msg::MnPreferences));
        cmds.push(m("edit.settings_json", Msg::MnEdit, Msg::MnSettingsJson));
        cmds.push(m("help.license", Msg::MnHelp, Msg::MnLicense));
        cmds.push(m("help.about", Msg::MnHelp, Msg::MnAbout));
        for name in self.syntax.names() {
            cmds.push((
                format!("syntax.set:{name}"),
                format!("{}: {name}", t(Msg::PalSetSyntax)),
            ));
        }
        self.palette.set_commands(cmds);
        self.palette.open(prefill);
        self.ime_refresh();
        self.redraw();
    }

    /// 접속 창 열기(메인 창 위 가운데) — 이미 열려 있으면 앞으로.
    /// 우클릭 편집 메뉴(nexa-ctl 내장)의 아이콘·단축키 + 그리드 메뉴 단축키 — 부팅·키맵 변경 때(사용자 09-15 "기본 기능에도 이미지").
    pub(crate) fn apply_menu_decor(&mut self) {
        nexa_ctl::controls::set_edit_menu_decor(nexa_ctl::controls::EditMenuDecor {
            icons: [
                Some(toolicons::mi_copy()),
                Some(toolicons::mi_cut()),
                Some(toolicons::mi_paste()),
                Some(toolicons::mi_select_all()),
            ],
            shortcuts: [
                self.keymap.display_of("edit.copy"),
                self.keymap.display_of("edit.cut"),
                self.keymap.display_of("edit.paste"),
                self.keymap.display_of("edit.select_all"),
            ],
        });
        let (sc_copy, sc_all) = (
            self.keymap.display_of("edit.copy"),
            self.keymap.display_of("edit.select_all"),
        );
        self.all_grids()
            .for_each(|g| g.set_shortcuts(sc_copy.clone(), sc_all.clone()));
    }

    /// 메뉴바를 현재 상태(최근 파일 · 탭 · 데모 준비 여부)로 다시 만든다.
    pub(crate) fn rebuild_menus(&mut self) {
        self.menubar
            .set_max_label_width(self.settings.int("ui.menu_max_width") as i32);
        let tabs = self.editors.tab_list_dirty();
        self.menubar.set_menus(App::build_menus_with(
            &self.recent_files(),
            &tabs,
            self.demo_ready,
            self.gate_shown.unwrap_or(false),
            self.project_menu_entries(),
        ));
    }

    /// 열린 팝업 메뉴의 비트 집합(배타 규칙의 입력 · 09-22): 1 풀다운 · 2 편집기 탭 메뉴 · 4 편집기 본문 편집 메뉴 · 8 상태줄/툴바 팝업 ·
    /// 16 오브젝트 탐색기 · 32 결과 그리드 · 64 결과 탭 줄. 팔레트·툴팁·하위 메뉴(같은 메뉴 안)는 메뉴가 아니다.
    pub(crate) fn open_menus(&self) -> u32 {
        let mut m = 0;
        if self.menubar.is_open() {
            m |= 1;
        }
        if self.editors.tab_menu_open() {
            m |= 2;
        }
        if self.editors.edit_menu_open() {
            m |= 4;
        }
        if self.status_menu.is_open() {
            m |= 8;
        }
        if self.explorer.menu_open() {
            m |= 16;
        }
        if self.intel.is_open() {
            m |= 1024;
        }
        if self.grid.menu_open() {
            m |= 32;
        }
        if self.panel.menu_open() {
            m |= 64;
        }
        if self.bm_panel.menu_open() {
            m |= 128;
        }
        if self.project_panel.menu_open() {
            m |= 256;
        }
        if self.objdetail.menu_open() {
            m |= 2048;
        }
        m
    }

    pub(crate) fn close_menu_bits(&mut self, bits: u32) {
        if bits & 1 != 0 {
            self.menubar.dismiss();
        }
        if bits & 2 != 0 {
            self.editors.close_tab_menu();
        }
        if bits & 4 != 0 {
            self.editors.close_edit_menus();
        }
        if bits & 8 != 0 {
            self.status_menu.close();
        }
        if bits & 16 != 0 {
            self.explorer.close_menu();
        }
        if bits & 32 != 0 {
            self.grid.close_menu();
        }
        if bits & 2048 != 0 {
            self.objdetail.close_menu();
        }
        if bits & 128 != 0 {
            self.bm_panel.close_menu();
        }
        if bits & 256 != 0 {
            self.project_panel.close_menu();
        }
        if bits & 64 != 0 {
            self.panel.close_menu();
        }
    }
}
