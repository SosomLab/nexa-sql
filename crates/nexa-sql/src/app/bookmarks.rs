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
                bookmarks_panel::BmAction::Mnemonic(id, n) => self.bookmarks.set_mnemonic(id, n),
                bookmarks_panel::BmAction::MoveGroup(id, g) => self.bookmarks.move_group(id, g),
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
        let items = bm_gutter_items(state.map(|(_, bm, mn)| (bm.is_some(), mn)));
        self.open_status_popup_w(Rect::new(p.x, p.y, 1, 1), items, 170.0);
        true
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
            match self.bookmarks.set_mnemonic_at_caret(&self.editors, i, n) {
                Ok(_) => {
                    let line = self.editors.caret_line_col().0;
                    self.sess.status = tf(
                        Msg::StBookmarkMnemonic,
                        &[&n.to_string(), &line.to_string()],
                    );
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
