//! App — 북마크(docs/69 · T-167).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 탭 하나의 거터 마크를 저장소에서 다시 만든다.
    pub(crate) fn bm_refresh_tab(&mut self, i: usize) {
        let (c, d) = (self.theme.accent, self.theme.text_dim);
        let marks = if self.bookmarks.enabled && self.settings.flag("bookmark.gutter") {
            self.bookmarks.marks_for(&self.editors, i, c, d)
        } else {
            Vec::new()
        };
        let labels = if self.bookmarks.enabled && self.settings.flag("bookmark.gutter") {
            self.bookmarks.labels_for(&self.editors, i)
        } else {
            Vec::new()
        };
        let on = self.bookmarks.enabled;
        let mm = if on && self.settings.flag("bookmark.minimap") {
            self.bookmarks.minimap_for(&self.editors, i, c)
        } else {
            Vec::new()
        };
        let inline = if on && self.settings.flag("bookmark.inline_label") {
            let cap = self.settings.int("bookmark.inline_label_chars").max(8) as usize;
            self.bookmarks.inline_for(&self.editors, i, cap)
        } else {
            Vec::new()
        };
        if let Some(tb) = self.editors.tab_box_mut(i) {
            tb.set_line_marks(marks);
            tb.set_gutter_labels(labels);
            tb.set_minimap_marks(mm);
            tb.set_inline_labels(inline);
        }
        // ★ 활성 탭의 줄 끝 라벨은 합성기가 최종 결정(docs/111 · 북마크 Ctrl 홀드 · 객체 설명과 합성) — 북마크가 바뀌면 다시.
        if i == self.editors.active() {
            self.inline_key.clear();
            self.inline_stale_mark(); // 방금 TextBox에 북마크 라벨만 썼다 — 합성 결과가 캐시와 같아도 다시 쓴다(124차 리뷰)
            self.inline_sync();
        }
    }

    /// 표시 전부(거터 · 패널 · 상태줄) 갱신.
    pub(crate) fn bm_sync_ui(&mut self) {
        for i in 0..self.editors.len() {
            self.bm_refresh_tab(i);
        }
        self.bm_titles_rev = self.editors.titles_rev();
        self.bm_panel.set_tab_titles(self.editors.tab_titles());
        self.bm_panel.sync(&self.bookmarks.store);
        self.redraw();
    }

    /// 문서 열쇠의 표시 이름(토스트·상태줄) — 이름 없는 탭은 id로 지금 탭 제목(패널과 같은 규칙).
    fn bm_doc_name(&self, doc: &nsql_bookmarks::DocKey) -> String {
        self.bm_panel.doc_name(doc)
    }

    /// 틱: 활성 탭의 줄 변경 기록 소비(L0) · 바뀐 표시 갱신 · 디바운스 저장 · **탭 이름이 바뀌면 패널의 문서 이름**(id → 제목).
    pub(crate) fn bm_tick(&mut self) {
        if !self.bookmarks.enabled {
            return;
        }
        // ★ T-167 B6 하루 1회(기동 뒤 첫 틱 포함): 파일 존재 · 서버 존재 점검 + 무효 정리 — 바뀐 것이 있으면 로그 한 줄씩.
        if self.bookmarks.daily_due() {
            self.bm_daily_check();
        }
        let i = self.editors.active();
        let moved = self.bookmarks.sync_tab(&self.editors, i);
        if self.bookmarks.take_changed() || moved {
            self.bm_sync_ui();
        } else if self.bm_titles_rev != self.editors.titles_rev() {
            // 탭 제목 세대가 바뀐 때만(이름 바꾸기 · 열기/닫기 · 저장) — 매 틱 제목 비교는 하지 않는다.
            self.bm_titles_rev = self.editors.titles_rev();
            if self.bm_panel.set_tab_titles(self.editors.tab_titles()) {
                self.redraw();
            }
        }
        self.bookmarks.tick_save();
    }

    /// ★ T-167 B6: 아는 서버 키 = 저장 프로필(비밀번호 없는 스펙) + 살아 있는 탐색기 칸.
    fn bm_known_server_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .explorer
            .find_servers()
            .into_iter()
            .filter_map(|(s, _)| s)
            .map(|s| server_key(&s))
            .collect();
        if let Ok(list) = nsql_vault::Vault::open_default().and_then(|v| v.list()) {
            for p in list {
                let k = server_key(&p.spec);
                if !keys.contains(&k) {
                    keys.push(k);
                }
            }
        }
        keys
    }

    /// 하루 1회 점검 + 저장소를 열 때(`project_set` 뒤) — 파일 없음/되살림 · 서버 모름/되살림 · 정리.
    pub(crate) fn bm_daily_check(&mut self) {
        let (missing, revived) = self.bookmarks.validate_files();
        if missing + revived > 0 {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                tf(
                    Msg::LogBookmarksValidated,
                    &[&missing.to_string(), &revived.to_string()],
                ),
            ));
        }
        let known = self.bm_known_server_keys();
        let (unknown, back) = self.bookmarks.validate_servers(&known);
        if unknown + back > 0 {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                tf(
                    Msg::LogBookmarksServers,
                    &[&unknown.to_string(), &back.to_string()],
                ),
            ));
        }
        let pruned = self.bookmarks.prune_now();
        if pruned > 0 {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!("bookmarks: pruned {pruned} stale"),
            ));
        }
        if missing + revived + unknown + back + pruned > 0 {
            self.bm_sync_ui();
        }
    }

    /// ★ T-167 B6 `ObjectMissing`(69 §3-3 · 10-31): 성공한 DDL 문장(`meta_on_done`)이 DROP/RENAME이면 그 객체의 소스 북마크를 무효로 ·
    /// CREATE면 되살림 — 탐색기 갱신 설정(`meta.refresh_on_ddl`)과 무관하게 · 접속 세션의 서버 키 · 스키마 없으면 세션 기본 스키마.
    pub(crate) fn bm_on_ddl(&mut self, t: &nsql_core::DdlTarget) {
        use nsql_core::DdlVerb;
        let dropped = match t.verb {
            DdlVerb::Drop | DdlVerb::Rename => true,
            DdlVerb::Create => false,
            _ => return,
        };
        let Some(spec) = self.sess.spec.as_ref() else {
            return;
        };
        let Some(kind) = crate::explorer::folder_kind(t.kind) else {
            return;
        };
        let profile = server_key(spec);
        let schema = t.schema.clone().or_else(|| self.meta_default_schema());
        let n = self
            .bookmarks
            .on_ddl(&profile, schema.as_deref(), kind.code(), &t.name, dropped);
        if n > 0 {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!(
                    "bookmarks: {n} object bookmark(s) {} after {:?} {}",
                    if dropped { "invalidated" } else { "revived" },
                    t.verb,
                    t.name
                ),
            ));
            self.bm_sync_ui();
        }
    }

    /// 패널이 낸 요청 거두기.
    pub(crate) fn bm_pump(&mut self) {
        while let Some(a) = self.bm_panel.take_action() {
            match a {
                bookmarks_panel::BmAction::Goto(id, permanent) => self.bm_goto(id, permanent),
                bookmarks_panel::BmAction::Remove(id) => {
                    if let Some(b) = self.bookmarks.remove(id) {
                        self.bm_removed_toast(1, &b.display());
                    }
                }
                bookmarks_panel::BmAction::Rename(id, text) => {
                    self.bookmarks.set_label(id, Some(text))
                }
                bookmarks_panel::BmAction::Note(id, text) => {
                    self.bookmarks.set_note(id, Some(text))
                }
                bookmarks_panel::BmAction::Mnemonic(id, n) => match n {
                    // ★ 같은 번호 다시 = 니모닉만 해제 · 다른 북마크 것이면 확인 없이 바로 가져오고 [복원하기] 안내
                    // (사용자 10-22 · 거터·지정 키와 같은 뜻).
                    Some(n) => {
                        if let crate::bookmarks::MnSet::Set(Some(r)) =
                            self.bookmarks.toggle_mnemonic(id, n)
                        {
                            self.bm_taken_notice(r);
                        }
                    }
                    None => self.bookmarks.set_mnemonic(id, None),
                },
                bookmarks_panel::BmAction::MoveGroup(id, g) => self.bookmarks.move_group(id, g),
                bookmarks_panel::BmAction::ReorderGroup(g, before) => {
                    self.bookmarks.reorder_group(g, before)
                }
                bookmarks_panel::BmAction::RemoveDoc(doc) => {
                    let n = self.bookmarks.remove_doc(&doc);
                    if n > 0 {
                        let name = self.bm_doc_name(&doc);
                        self.bm_removed_toast(n, &name);
                    }
                }
                bookmarks_panel::BmAction::NewGroup => {
                    if !self.lic_gate(nsql_license::Feature::BookmarkGroups) {
                        continue;
                    }
                    let base = t(Msg::BmNewGroupName).to_string();
                    let gid = self.bookmarks.new_group(&base);
                    self.bm_panel.sync(&self.bookmarks.store);
                    self.sess.status = tf(Msg::StBookmarkGroupNew, &[&gid.to_string()]);
                }
                bookmarks_panel::BmAction::RenameGroup(g, name) => {
                    self.bookmarks.rename_group(g, &name)
                }
                bookmarks_panel::BmAction::ToggleGroup(g) => self.bookmarks.toggle_group(g),
                bookmarks_panel::BmAction::DefaultGroup(g) => self.bookmarks.set_default_group(g),
                bookmarks_panel::BmAction::DeleteGroup(g, keep) => {
                    if !self.bookmarks.delete_group(g, keep) {
                        self.sess.status = t(Msg::StBookmarkGroupDefault).into();
                    } else if !keep {
                        self.bm_removed_toast(0, "");
                    }
                }
                bookmarks_panel::BmAction::RemoveInvalid => {
                    let n = self.bookmarks.remove_invalid();
                    if n > 0 {
                        self.bm_removed_toast(n, "");
                    }
                }
                bookmarks_panel::BmAction::OpenSettings => self.menu_action("edit.prefs"),
                bookmarks_panel::BmAction::SelectAllInDoc(doc) => {
                    if let Some(i) = self.bm_tab_of(&doc) {
                        self.editors.switch(i);
                        self.bm_select_all_in(i);
                        self.set_focus(Focus::Editor);
                    }
                }
            }
        }
        if self.bookmarks.take_changed() {
            self.bm_sync_ui();
        }
    }

    /// 제거 뒤 5초 [되돌리기] 토스트(69 C-28 · Ctrl+Z는 본문만) — 클릭 = `bookmark.undo_remove`.
    fn bm_removed_toast(&mut self, n: usize, what: &str) {
        let body = if n == 1 {
            tf(Msg::StBookmarkRemoved, &[what])
        } else {
            tf(Msg::StBookmarkCleared, &[&n.to_string()])
        };
        self.sess.status = body.clone();
        self.toasts.push_action(
            toast::ToastKind::Info,
            body,
            t(Msg::StBookmarkUndo).to_string(),
            "bookmark.undo_remove",
        );
    }

    fn bm_tab_of(&self, doc: &nsql_bookmarks::DocKey) -> Option<usize> {
        let ci = cfg!(any(windows, target_os = "macos"));
        (0..self.editors.len())
            .find(|&i| bookmarks::Bookmarks::doc_key(&self.editors, i).same(doc, ci))
    }

    /// 북마크로 이동 — 열린 탭이면 전환 · 파일이면 열고 · 그 줄로.
    /// 북마크로 이동 — `permanent` = 정식 탭(더블클릭·Enter·메뉴 Open · 이미 미리보기로 열려 있으면 승격) ·
    /// false = 미리보기 탭(한 번 클릭 · 69 B4b · 프로젝트 탐색기와 같은 규칙 `project.preview_tab` · 편집하면 승격).
    fn bm_goto(&mut self, id: u64, permanent: bool) {
        let Some(b) = self.bookmarks.store.get(id).cloned() else {
            return;
        };
        let mut idx = self.bm_tab_of(&b.doc);
        if let nsql_bookmarks::DocKey::File { path } = &b.doc {
            if idx.is_none() || permanent {
                self.project_open_req(project_panel::OpenReq {
                    path: PathBuf::from(path),
                    permanent,
                });
                idx = self.bm_tab_of(&b.doc);
            }
        }
        // ★ T-167 B5 재개(69 §4-2): 객체 북마크인데 소스 탭이 없으면 — 그 서버에 **살아 있는** 세션 칸이 있을 때만 소스를 조회해 열고
        //   도착하면 이어서 이동(`bm_goto_pending`) · 없으면 안내만(자동 재접속 금지 · 26 §8).
        if idx.is_none() {
            if let nsql_bookmarks::DocKey::Object {
                profile,
                schema,
                kind,
                name,
                ..
            } = &b.doc
            {
                let spec = self
                    .explorer
                    .find_servers()
                    .into_iter()
                    .filter_map(|(s, _)| s)
                    .find(|s| server_key(s) == *profile);
                let Some(spec) = spec else {
                    self.sess.status = tf(Msg::StBookmarkNeedConnect, &[profile]);
                    self.redraw();
                    return;
                };
                let Some(k) = nsql_catalog::ObjectKind::parse(kind) else {
                    self.sess.status = t(Msg::StBookmarkNoDoc).into();
                    return;
                };
                let info = nsql_catalog::ObjectInfo {
                    db: String::new(),
                    schema: schema.clone(),
                    name: name.clone(),
                    kind: k,
                    status: String::new(),
                    modified: String::new(),
                    extra: String::new(),
                };
                if self.explorer.open_source_for(Some(&spec), &info) {
                    self.bm_goto_pending = Some((id, permanent));
                    self.sess.status =
                        tf(Msg::StBookmarkOpeningObject, &[&format!("{schema}.{name}")]);
                } else {
                    self.sess.status = tf(Msg::StBookmarkNeedConnect, &[profile]);
                }
                self.redraw();
                return;
            }
        }
        let Some(i) = idx else {
            self.sess.status = t(Msg::StBookmarkNoDoc).into();
            return;
        };
        self.editors.switch(i);
        self.editors.cur_mut().goto_line(b.anchor.line as usize + 1);
        if let Some(bm) = self.bookmarks.store.get_mut(id) {
            bm.visited = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
        }
        self.set_focus(Focus::Editor);
        self.redraw();
    }

    /// ★ T-167 B5: 객체 소스 탭이 열린 직후 — 재탐색(되살림 포함) + 대기 중인 북마크 이동이 **이 문서** 것이면 이어서 간다.
    pub(crate) fn bm_after_source_open(&mut self) {
        let i = self.editors.active();
        self.bookmarks.relocate_doc(&self.editors, i);
        if let Some((id, permanent)) = self.bm_goto_pending.take() {
            let doc = bookmarks::Bookmarks::doc_key(&self.editors, i);
            let same = self
                .bookmarks
                .store
                .get(id)
                .is_some_and(|b| b.doc.same(&doc, cfg!(any(windows, target_os = "macos"))));
            if same {
                self.bm_goto(id, permanent);
            }
        }
    }

    /// 문서의 북마크 줄 전부에 캐럿(Sublime Select All Bookmarks · 69 §6-1).
    fn bm_select_all_in(&mut self, i: usize) {
        let lines = self.bookmarks.doc_lines(&self.editors, i);
        if lines.is_empty() {
            self.sess.status = t(Msg::StBookmarkNone).into();
            return;
        }
        if let Some(tb) = self.editors.tab_box_mut(i) {
            let regions: Vec<(usize, usize)> = lines
                .iter()
                .map(|&l| {
                    let a = tb
                        .buf()
                        .line_start(l.min(tb.buf().line_count().saturating_sub(1)));
                    (a, a)
                })
                .collect();
            tb.set_regions_pub(&regions);
        }
        let n = lines.len();
        self.sess.status = tf(Msg::StSelections, &[&n.to_string()]);
    }

    /// `bookmark.*` 명령.
    /// 거터 우클릭(사용자 09-23): 활성 편집기의 거터(북마크/니모닉 영역 + 줄번호) 안이면 그 줄로 캐럿을 옮기고 상태에 맞는 메뉴를 연다.
    /// 열었으면 true(사건 소비) · 거터 밖이면 false(본문 우클릭 = 종전 편집 메뉴).
    pub(crate) fn open_bm_gutter_menu(&mut self, p: Point) -> bool {
        if !self.bookmarks.enabled {
            return false;
        }
        // 뷰 탭(비교 diff 뷰어 · 확장 상세)은 자리 탭의 숨은 상자 위에 다른 것이 그려진다 → 그 거터 자리의 우클릭은 뷰의 몫.
        if self.editors.active_view().is_some() {
            return false;
        }
        let i = self.editors.active();
        let Some(tb) = self.editors.tab_box(i) else {
            return false;
        };
        if !tb.in_gutter(p) {
            return false;
        }
        // 마지막 줄 아래 빈 영역 = 줄 단위 항목(토글·니모닉) 없이 "북마크 보기"만(사용자 09-23).
        let on_text = !tb.below_text(p);
        let line = on_text.then(|| tb.line_at_point(p)).flatten();
        let state = line.map(|line| {
            self.editors.cur_mut().goto_line(line + 1);
            let (bm, mn) = self.bookmarks.line_state(&self.editors, i, line);
            (line, bm, mn)
        });
        self.bm_gutter = state.map(|(line, bm, _)| (i, line, bm));
        self.close_context_menus();
        let used = self.bookmarks.used_mnemonics(&self.editors, i);
        let items = bm_gutter_items(state.map(|(_, bm, mn)| (bm.is_some(), mn)), &used);
        self.open_status_popup_w(Rect::new(p.x, p.y, 1, 1), items, 170.0);
        true
    }

    /// ★ 사용 중 니모닉을 가져온 뒤의 안내(사용자 10-22 3차 · 오류처럼 보이지 않게 Info · 상태줄은 보통 지정 문구 그대로):
    /// 토스트 [복원하기] = `bookmark.restore_mn` → [`Self::bm_restore_pending`]. `bookmark.mnemonic_notice` off = 조용히.
    fn bm_taken_notice(&mut self, r: crate::bookmarks::MnRestore) {
        if !self.settings.flag("bookmark.mnemonic_notice") {
            return;
        }
        let from_line = self
            .bookmarks
            .store
            .get(r.from_id)
            .map_or(0, |b| b.anchor.line as usize)
            + 1;
        self.bookmarks.pending_restore = Some(r);
        self.toasts.push_button(
            toast::ToastKind::Info,
            tf(
                Msg::ToastBmMnemonicTaken,
                &[&r.n.to_string(), &from_line.to_string()],
            ),
            tf(Msg::ToastBmMnemonicTakenBody, &[&from_line.to_string()]),
            t(Msg::BtnBmRestoreMnemonic),
            "bookmark.restore_mn",
        );
        self.redraw();
    }

    /// 토스트 [복원하기] — 방금 가져온 니모닉을 원래 북마크로 되돌린다(이번에 만든 북마크는 지우고 · 전 번호는 되돌림).
    pub(crate) fn bm_restore_pending(&mut self) {
        let Some(r) = self.bookmarks.pending_restore.take() else {
            return;
        };
        self.sess.status = match self.bookmarks.restore_mnemonic(r) {
            Some(line) => tf(
                Msg::StBookmarkMnemonicRestored,
                &[&r.n.to_string(), &(line + 1).to_string()],
            ),
            None => tf(Msg::StBookmarkNoMnemonic, &[&r.n.to_string()]),
        };
        self.bm_sync_ui();
    }

    /// 거터 메뉴의 답 — `view` = 좌측 북마크 패널 열기 · `toggle` = 토글(캐럿은 이미 그 줄) · `mn:<n>` = 니모닉 지정(없으면 만들어서) · `mn:clear` = 해제.
    pub(crate) fn bm_gutter_pick(&mut self, rest: &str) {
        if rest == "view" {
            self.bm_gutter = None;
            if !self.bm_panel.is_visible() {
                self.menu_action("view.bookmarks");
            }
            self.redraw();
            return;
        }
        let Some((i, line, bm)) = self.bm_gutter.take() else {
            return;
        };
        if self.editors.active() != i {
            self.editors.switch(i);
        }
        self.editors.cur_mut().goto_line(line + 1);
        match rest {
            "toggle" => self.bookmark_cmd("bookmark.toggle"),
            "mn:clear" => {
                if let Some(id) = bm {
                    self.bookmarks.set_mnemonic(id, None);
                    self.bm_sync_ui();
                }
            }
            other => {
                if let Some(n) = other.strip_prefix("mn:") {
                    self.bookmark_cmd(&format!("bookmark.set_{n}"));
                }
            }
        }
        self.redraw();
    }

    pub(crate) fn bookmark_cmd(&mut self, id: &str) {
        if !self.bookmarks.enabled {
            self.sess.status = t(Msg::StBookmarkOff).into();
            return;
        }
        let i = self.editors.active();
        if let Some(n) = id
            .strip_prefix("bookmark.set_")
            .and_then(|n| n.parse::<u8>().ok())
        {
            // ★ 같은 줄·같은 번호 다시 = 니모닉만 해제(북마크 유지) · 다른 줄의 북마크가 쓰는 번호면 확인 없이 바로
            // 가져오고 [복원하기] 안내(사용자 10-22 · 69 §C-10 · 지정 키·거터 메뉴 같은 길).
            match self.bookmarks.toggle_mnemonic_at_caret(&self.editors, i, n) {
                Ok(crate::bookmarks::MnSet::Set(restore)) => {
                    let line = self.editors.caret_line_col().0;
                    self.sess.status = tf(
                        Msg::StBookmarkMnemonic,
                        &[&n.to_string(), &line.to_string()],
                    );
                    if let Some(r) = restore {
                        self.bm_taken_notice(r);
                    }
                }
                Ok(crate::bookmarks::MnSet::Cleared) => {
                    self.sess.status = tf(Msg::StBookmarkMnemonicCleared, &[&n.to_string()]);
                }
                Err(k) => self.sess.status = tf(Msg::StBookmarkCap, &[&format!("bookmark.{k}")]),
            }
        } else if let Some(n) = id
            .strip_prefix("bookmark.goto_")
            .and_then(|n| n.parse::<u8>().ok())
        {
            match self.bookmarks.mnemonic_line(&self.editors, i, n) {
                Some(l) => {
                    self.editors.cur_mut().goto_line(l + 1);
                    self.set_focus(Focus::Editor);
                }
                None => self.sess.status = tf(Msg::StBookmarkNoMnemonic, &[&n.to_string()]),
            }
        } else {
            match id {
                "bookmark.toggle" => match self.bookmarks.toggle_caret(&self.editors, i) {
                    Ok(true) => {
                        let line = self.editors.caret_line_col().0;
                        self.sess.status = tf(Msg::StBookmarkAdded, &[&line.to_string()]);
                    }
                    Ok(false) => self.sess.status = tf(Msg::StBookmarkRemoved, &[""]),
                    Err(k) => {
                        self.sess.status = tf(Msg::StBookmarkCap, &[&format!("bookmark.{k}")])
                    }
                },
                "bookmark.next" | "bookmark.prev" => {
                    match self
                        .bookmarks
                        .next_line(&self.editors, i, id == "bookmark.next")
                    {
                        Some(l) => {
                            self.editors.cur_mut().goto_line(l + 1);
                            self.set_focus(Focus::Editor);
                        }
                        None => self.sess.status = t(Msg::StBookmarkNone).into(),
                    }
                }
                "bookmark.clear_doc" => {
                    let n = self.bookmarks.clear_doc(&self.editors, i);
                    self.sess.status = tf(Msg::StBookmarkCleared, &[&n.to_string()]);
                }
                "bookmark.select_all" => self.bm_select_all_in(i),
                "bookmark.undo_remove" => {
                    let n = self.bookmarks.undo_remove();
                    self.sess.status = tf(Msg::StBookmarkRestored, &[&n.to_string()]);
                }
                "bookmark.label" => {
                    if !self.bm_panel.is_visible() {
                        self.menu_action("view.bookmarks");
                    }
                    if !self.bm_panel.begin_rename() {
                        self.set_focus(Focus::Bookmarks);
                    }
                }
                _ => {}
            }
        }
        if self.bookmarks.take_changed() {
            self.bm_sync_ui();
        }
        self.redraw();
    }
}

/// ★ T-167 B5(69 §4-2): 서버 키 = `호스트:포트/데이터베이스`(소문자 호스트 · 계정·역할은 뺀다 — 같은 객체를 다른 계정으로 봐도 같은 문서).
pub(crate) fn server_key(s: &nsql_script::ConnectSpec) -> String {
    format!(
        "{}:{}/{}",
        s.host.as_deref().unwrap_or("").to_ascii_lowercase(),
        s.port.map(|p| p.to_string()).unwrap_or_default(),
        s.database.as_deref().unwrap_or("")
    )
}

/// 객체 소스 탭의 출처 → 북마크 문서 열쇠(서버 없으면 `None` = 이름 없는 탭 규칙으로).
pub(crate) fn object_doc_key(o: &crate::explorer::ObjectOrigin) -> Option<nsql_bookmarks::DocKey> {
    let spec = o.server.as_ref()?;
    Some(nsql_bookmarks::DocKey::Object {
        profile: server_key(spec),
        dialect: spec.dialect.map(|d| d.to_string()).unwrap_or_default(),
        schema: o.schema.clone(),
        kind: o.kind.code().to_string(),
        name: o.name.clone(),
        part: "source".into(),
    })
}

#[cfg(test)]
mod object_doc_key_tests {
    use super::*;

    /// 서버 키는 계정·역할·비밀번호와 무관 · 호스트 소문자 · 종류 코드.
    #[test]
    fn server_key_ignores_account_and_folds_host() {
        let mut a =
            nsql_script::ConnectSpec::parse("oracle://u1:pw@MyHost:1521/APPDB").expect("spec");
        let b = nsql_script::ConnectSpec::parse("oracle://u2@myhost:1521/APPDB").expect("spec");
        assert_eq!(server_key(&a), server_key(&b));
        assert_eq!(server_key(&a), "myhost:1521/APPDB");
        a.role = Some("sysdba".into());
        assert_eq!(server_key(&a), server_key(&b));
        let o = crate::explorer::ObjectOrigin {
            schema: "HR".into(),
            name: "PKG_X".into(),
            kind: nsql_catalog::ObjectKind::Procedure,
            server: Some(b),
        };
        let k = object_doc_key(&o).expect("key");
        assert_eq!(
            k,
            nsql_bookmarks::DocKey::Object {
                profile: "myhost:1521/APPDB".into(),
                dialect: "oracle".into(),
                schema: "HR".into(),
                kind: "procedure".into(),
                name: "PKG_X".into(),
                part: "source".into(),
            }
        );
        assert!(object_doc_key(&crate::explorer::ObjectOrigin { server: None, ..o }).is_none());
    }
}
