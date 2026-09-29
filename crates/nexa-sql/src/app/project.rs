//! App — 프로젝트·작업 환경·다중 열기(docs/67·70).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

/// 글자열 FNV-1a(변경 세대 섞기용 · T-253).
fn fnv_str(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

impl App {
    /// 확인 팝업 — 개수·합계 크기 · 상한(`file.open_max`)까지의 목록 · 초과분 안내 · 열기/취소.
    pub(crate) fn multi_open_ask(&mut self, paths: Vec<PathBuf>, enc: String) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        let info = |label: String| CtxItem::Item {
            id: String::new(),
            label,
            enabled: false,
            icon: None,
            shortcut: None,
            sub: None,
            children: Vec::new(),
            checked: None,
            active: false,
            mark: None,
            emph: false,
            marks: Vec::new(),
        };
        let max = self.settings.int("file.open_max").max(1) as usize;
        let take: Vec<PathBuf> = paths.iter().take(max).cloned().collect();
        let excluded = paths.len().saturating_sub(take.len());
        if take.len() == 1 {
            // 상한이 1이거나 하나만 남았다 = 바로.
            self.open_file_enc(&take[0], &enc);
            return;
        }
        let sizes: Vec<u64> = take
            .iter()
            .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
            .collect();
        let total: u64 = sizes.iter().sum();
        let mut items = vec![info(tf(
            Msg::MultiOpenHeader,
            &[&take.len().to_string(), &nsql_core::fmt_bytes(total)],
        ))];
        items.push(CtxItem::Separator);
        for (p, sz) in take.iter().zip(&sizes) {
            items.push(info(format!(
                "{}  ({})",
                editors::file_title(p),
                nsql_core::fmt_bytes(*sz)
            )));
        }
        if excluded > 0 {
            items.push(CtxItem::Separator);
            items.push(info(tf(
                Msg::MultiOpenExcluded,
                &[&excluded.to_string(), &max.to_string()],
            )));
        }
        items.push(CtxItem::Separator);
        items.push(CtxItem::item(
            "multi.open",
            tf(Msg::MultiOpenGo, &[&take.len().to_string()]),
        ));
        items.push(CtxItem::item("multi.cancel", t(Msg::BtnCancel)));
        self.multi_pending = Some((take, enc));
        // ★ 화면 **중앙** 모달(사용자 09-22): 한 번 열어 크기를 재고 가운데에 다시 연다 · Enter = 첫 활성 항목(모두 열기).
        let again = items.clone();
        self.open_status_popup(Rect::new(0, 0, 0, 0), items);
        let mb = self.status_menu.bounds();
        let r = self
            .window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(
                    (sz.width as i32 - mb.w) / 2,
                    (sz.height as i32 - mb.h) / 2,
                    0,
                    0,
                )
            })
            .unwrap_or_default();
        let open_idx = again.len().saturating_sub(2); // [정보들 · 구분선 · 열기 · 취소]
        self.open_status_popup(r, again);
        self.status_menu.set_default(open_idx); // Enter = 모두 열기(기본 항목 · 테두리 표시)
    }

    /// 팝업 항목(`multi.*`).
    pub(crate) fn multi_pick(&mut self, id: &str) {
        match id {
            "open" => self.multi_open_start(),
            _ => {
                self.multi_pending = None;
            }
        }
    }

    /// 자리 탭을 전부 만들고 스레드 하나가 순서대로 읽는다. 이미 연 파일은 건너뛴다.
    fn multi_open_start(&mut self) {
        let Some((paths, enc)) = self.multi_pending.take() else {
            return;
        };
        let origin = self.editors.active_id();
        let mut jobs: Vec<(PathBuf, u64)> = Vec::new();
        for p in paths {
            if self.editors.path_tab(&p).is_some() {
                continue;
            }
            let id = self.editors.begin_load_tab(&editors::file_title(&p));
            jobs.push((p, id));
        }
        if jobs.is_empty() {
            self.redraw();
            return;
        }
        // 첫 파일의 탭을 보인다(자리 탭은 마지막 것이 활성이 되므로 되돌린다).
        self.editors.switch_to_id(jobs[0].1);
        let n = jobs.len();
        let (tx, rx) = std::sync::mpsc::channel();
        let proxy = std::sync::Mutex::new(self.wake_proxy.clone());
        let spawned = std::thread::Builder::new()
            .name("multi-open".into())
            .spawn(move || {
                for (p, id) in jobs {
                    let r = fileload::load(&p, &enc, None, true, None);
                    if tx.send((p, id, r)).is_err() {
                        break;
                    }
                    if let Ok(px) = proxy.lock() {
                        let _ = px.send_event(Wake);
                    }
                }
            });
        if spawned.is_err() {
            self.sess.status = tf(Msg::StFileReadError, &["multi-open", "thread"]);
            return;
        }
        self.multi_load = Some(MultiLoad {
            rx,
            remaining: n,
            total: n,
            origin,
            started: Instant::now(),
        });
        self.sess.status = tf(Msg::StMultiOpenStart, &[&n.to_string()]);
        self.set_focus(Focus::Editor);
        self.layout();
        self.redraw();
    }

    /// 순차 적재 결과 수거(틱) — 온 것부터 그 자리 탭에 옮겨 넣는다(활성 탭은 바꾸지 않는다).
    pub(crate) fn multi_load_poll(&mut self) {
        let Some(ml) = self.multi_load.as_mut() else {
            return;
        };
        let mut got = Vec::new();
        while let Ok(item) = ml.rx.try_recv() {
            got.push(item);
        }
        if got.is_empty() {
            return;
        }
        let origin = ml.origin;
        for (path, tab, result) in got {
            self.file_loaded(FileLoaded {
                path,
                mode: LoadMode::Open,
                tab: Some(tab),
                origin,
                result,
            });
            if let Some(ml) = self.multi_load.as_mut() {
                ml.remaining = ml.remaining.saturating_sub(1);
            }
        }
        let done = self.multi_load.as_ref().is_some_and(|m| m.remaining == 0);
        if done {
            let m = self.multi_load.take().unwrap_or_else(|| unreachable!());
            self.sess.status = tf(
                Msg::StMultiOpenDone,
                &[
                    &m.total.to_string(),
                    &format!("{:.1}", m.started.elapsed().as_secs_f32()),
                ],
            );
        }
        self.redraw();
    }

    /// Project 메뉴 항목 — 명령 · 폴더 제거(폴더마다) · 최근 프로젝트 · 프로젝트 없음.
    pub(crate) fn project_menu_entries(&self) -> Vec<MenuEntry> {
        let item = |id: &str, m: Msg| MenuEntry::Item(ComboItem::new(id, t(m)));
        let open = self.project.is_open();
        let gated = |id: &str, m: Msg| {
            if open {
                MenuEntry::Item(ComboItem::new(id, t(m)))
            } else {
                MenuEntry::Disabled(ComboItem::new(id, t(m)))
            }
        };
        let mut v = vec![
            item("project.new", Msg::MnProjectNew),
            item("project.open", Msg::MnProjectOpen),
            item("project.switch", Msg::MnProjectSwitch),
            MenuEntry::Separator,
            gated("project.save", Msg::MnProjectSave),
            gated("project.save_as", Msg::MnProjectSaveAs),
            gated("project.close", Msg::MnProjectClose),
            MenuEntry::Separator,
            gated("project.add_folder", Msg::MnProjectAddFolder),
        ];
        // 폴더 제거는 탐색기 루트 우클릭에서만(풀다운의 폴더별 항목은 뺐다 · 사용자 09-23).
        let recent = project::recent_list(self.settings.get("project.recent").unwrap_or(""));
        if !recent.is_empty() {
            v.push(MenuEntry::Separator);
            for (i, p) in recent.iter().enumerate() {
                let name = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                v.push(MenuEntry::Item(ComboItem::new(
                    format!("project.recent:{i}"),
                    format!("{}  {}", name, nexa_fs::path::display(p)),
                )));
            }
        }
        v
    }

    /// `project.*` 명령 하나로(메뉴 · 팔레트 · 탐색기 링크 · 기동 명령).
    pub(crate) fn project_cmd(&mut self, id: &str) {
        if let Some(rest) = id.strip_prefix("project.recent:") {
            let recent = project::recent_list(self.settings.get("project.recent").unwrap_or(""));
            if let Some(p) = rest
                .parse::<usize>()
                .ok()
                .and_then(|i| recent.get(i).cloned())
            {
                self.project_load_path(&p);
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("project.remove_folder:") {
            if let Some(f) = rest
                .parse::<usize>()
                .ok()
                .and_then(|i| self.project.remove_folder(i))
            {
                self.sess.status = tf(Msg::StProjectFolderRemoved, &[&nexa_fs::path::display(&f)]);
                self.project_changed(true);
            }
            return;
        }
        // 자체 캡처·자동화: 대화상자 없이 바로 연다.
        if let Some(p) = id.strip_prefix("project.load:") {
            self.project_load_path(Path::new(p.trim()));
            return;
        }
        // OPEN FILES × = 그 탭 닫기(미저장 확인은 기존 흐름).
        if let Some(id) = id
            .strip_prefix("editor.close:")
            .and_then(|s| s.trim().parse::<u64>().ok())
        {
            if let Some(i) = self.editors.index_of_id(id) {
                self.close_tab_guarded(i);
            }
            self.redraw();
            return;
        }
        // OPEN FILES 항목 클릭 = 그 탭으로(사용자 09-23).
        if let Some(id) = id
            .strip_prefix("editor.switch:")
            .and_then(|s| s.trim().parse::<u64>().ok())
        {
            self.editors.switch_to_id(id);
            self.sync_grid_tab();
            self.set_focus(Focus::Editor);
            self.redraw();
            return;
        }
        // 자체 시험: 탐색기 필터에 글 넣기(`project.filter:<글>` · 키 주입 없이 필터 결과를 캡처).
        if let Some(q) = id.strip_prefix("project.filter:") {
            // `project.filter:[cwrp]:<글>` = 옵션(Case·Word·Regex·Path)을 먼저 켠다.
            let (flags, text) = match q.split_once(':') {
                Some((f, t)) if !f.is_empty() && f.chars().all(|c| "cwrp".contains(c)) => (f, t),
                _ => ("", q),
            };
            self.project_panel.set_filter_opts(
                flags.contains('c'),
                flags.contains('w'),
                flags.contains('r'),
                flags.contains('p'),
            );
            self.project_panel.set_filter_text(text.trim());
            self.redraw();
            return;
        }
        match id {
            // "새 프로젝트 저장"(파일 모드에서도 늘 활성) = 빈 프로젝트 · "다른 이름으로" = 지금 프로젝트 복사 — 둘 다 저장 뒤 그 프로젝트로 전환.
            "project.new" | "project.save_as" => {
                self.project_new_fresh = id == "project.new";
                self.file_purpose = FilePurpose::Project;
                self.open_file_dlg = Some(PickerMode::Save);
            }
            "project.open" => {
                self.file_purpose = FilePurpose::Project;
                self.open_file_dlg = Some(PickerMode::Open);
            }
            "project.save" => {
                if let Some(p) = self.project.path.clone() {
                    self.project_save_to(&p);
                } else {
                    self.sess.status = t(Msg::StProjectNoProject).into();
                }
            }
            "project.close" | "project.none" => {
                self.palette.close();
                if self.project.is_open() {
                    let _ = self.project_save();
                    self.project_set(project::Project::default(), false);
                    // ★ 닫은 뒤 = 처음 실행 상태(사용자 09-23): 전체 상태는 위 저장으로 프로젝트 파일·스냅숏에 담겼다.
                    self.reset_workspace_to_initial();
                    self.sess.status = t(Msg::StProjectClosed).into();
                }
            }
            "project.add_folder" => {
                if self.project.is_open() {
                    self.file_purpose = FilePurpose::Project;
                    self.folder_start = self
                        .project
                        .path
                        .as_ref()
                        .and_then(|p| p.parent().map(Path::to_path_buf));
                    self.open_file_dlg = Some(PickerMode::Folder);
                } else {
                    self.sess.status = t(Msg::StProjectNoProject).into();
                }
            }
            // 탐색기 필터 옆 토글(사용자 09-22) — 파일 대화상자와 같은 설정 키를 뒤집는다 → 변경 훅이 패널에 되돌려 준다.
            "project.toggle_hidden" | "project.toggle_dot" => {
                let key = if id == "project.toggle_hidden" {
                    "file.show_hidden"
                } else {
                    "file.show_dot"
                };
                let on = !self.settings.flag(key);
                let _ = self.settings.set(key, if on { "on" } else { "off" });
                let _ = self.apply_setting(key);
                self.redraw();
            }
            "project.switch" => {
                // 팔레트로 고른다: (프로젝트 없음) + 최근 목록.
                let recent =
                    project::recent_list(self.settings.get("project.recent").unwrap_or(""));
                let mut cmds: Vec<(String, String)> =
                    vec![("project.none".into(), t(Msg::MnProjectNone).into())];
                for (i, p) in recent.iter().enumerate() {
                    let name = p
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    cmds.push((
                        format!("project.recent:{i}"),
                        format!("{}  {}", name, nexa_fs::path::display(p)),
                    ));
                }
                cmds.push(("project.open".into(), t(Msg::MnProjectOpen).into()));
                self.palette.set_commands(cmds);
                self.palette.open("");
                self.ime_refresh();
            }
            _ => {}
        }
        self.redraw();
    }

    /// ★ 프로젝트를 닫은 뒤 = **처음 실행 상태**(사용자 09-23 "기본 편집 탭 1개만 남기고 모두 닫기 · 초기 상태"): 편집기는
    /// 빈 `Script_1` 하나(전체 상태는 직전 `project_save`가 프로젝트 파일·스냅숏에 담았다) · 좌측은 객체 탐색기만 · 북마크는
    /// 로컬 세트(`bind_project(None)`이 이미 바꿈) · 접속은 그대로(접속은 사용자 몫 · 70 §2).
    fn reset_workspace_to_initial(&mut self) {
        self.editors.reset_to_initial();
        self.side_panel_close_others("view.explorer");
        self.sync_grid_tab();
        self.sync_gate();
        self.apply_tab_line_colors();
        self.sync_open_files();
        self.layout();
        self.redraw();
    }

    /// 프로젝트 파일을 읽어 현재 프로젝트로(현재 것은 먼저 저장).
    pub(crate) fn project_load_path(&mut self, path: &Path) {
        self.palette.close();
        match project::Project::load(path) {
            Ok(p) => {
                let switching = self.project.path.as_deref() != Some(path);
                if self.project.is_open() && switching {
                    let _ = self.project_save();
                }
                // 같은 파일을 다시 열면(전환 목록에서 지금 프로젝트) 복원하지 않는다 — 탭이 겹쳐 늘지 않게.
                self.project_set(p, switching);
            }
            Err(e) => {
                self.sess.status = tf(Msg::ErrProjectFile, &[&e]);
                self.log_win
                    .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
            }
        }
        self.redraw();
    }

    /// 새 프로젝트/다른 이름으로 — 지금 프로젝트(없으면 빈 것)를 그 경로에 쓴다.
    pub(crate) fn project_save_to(&mut self, path: &Path) {
        let path = if path.extension().is_some_and(|e| e == project::EXT) {
            path.to_path_buf()
        } else {
            path.with_extension(project::EXT)
        };
        let fresh = std::mem::take(&mut self.project_new_fresh);
        let mut p = if fresh {
            project::Project::default()
        } else {
            self.project.clone()
        }
        .with_path(&path);
        // 프로젝트 파일이 놓인 폴더 = 기본 폴더(새 프로젝트는 늘 · 복사본은 폴더가 없을 때만 · 사용자 09-22).
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            if fresh || p.folders.is_empty() {
                p.folders.insert(0, dir.to_path_buf());
                p.folders.dedup();
            }
        }
        // ★ 지금 작업 환경(탭 경로 · 캐럿 · 북마크 · 패널 상태)을 담아서 쓴다 — 담지 않고 써서 "저장했는데 탭 경로가 없다"
        //   (사용자 09-23). 새 프로젝트도 지금 열린 탭이 그 프로젝트의 첫 작업 환경이다.
        let prev = std::mem::replace(&mut self.project, p);
        self.project_capture_state();
        match self.project.save() {
            Ok(()) => {
                self.sess.status = tf(Msg::StProjectSaved, &[&nexa_fs::path::display(&path)]);
                // 파일 모드 → 새 프로젝트: 로컬 북마크는 프로젝트로 **이관**(로컬 파일 비움 · 사용자 09-23 "닫으면 북마크도 초기화").
                if !prev.is_open() {
                    self.bookmarks.mark_migrate_local();
                }
                let p = self.project.clone();
                self.project_set(p, false);
            }
            Err(e) => {
                self.project = prev;
                self.sess.status = tf(Msg::ErrProjectFile, &[&e]);
            }
        }
        self.redraw();
    }

    pub(crate) fn project_add_folder(&mut self, dir: &Path) {
        if self.project.add_folder(dir) {
            self.sess.status = tf(Msg::StProjectFolderAdded, &[&nexa_fs::path::display(dir)]);
            self.project_changed(true);
        }
    }

    /// 프로젝트 교체 = 상태 · 설정(`project.last` · 최근) · 탐색기 · 메뉴 · `restore` = 작업 환경 복원까지
    /// (**다른** 프로젝트를 열 때만 true — 지금 작업 환경을 그 파일에 저장하는 길(저장 · 새 프로젝트 · 닫기)은 false.
    ///  저장 때도 복원이 돌아 누를 때마다 스크립트 탭이 하나씩 늘던 결함 · 사용자 09-23).
    fn project_set(&mut self, p: project::Project, restore: bool) {
        self.project = p;
        let last = self
            .project
            .path
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let _ = self.settings.set("project.last", &last);
        if let Some(p) = self.project.path.clone() {
            let raw = project::push_recent(self.settings.get("project.recent").unwrap_or(""), &p);
            let _ = self.settings.set("project.recent", &raw);
            self.sess.status = tf(
                Msg::StProjectOpened,
                &[
                    &self.project.name().unwrap_or_default(),
                    &self.project.folders.len().to_string(),
                ],
            );
        }
        self.persist_settings();
        self.project_changed(false);
        // 프로젝트가 바뀌면 북마크 저장소도(워크스페이스 파일 · 69 C-17).
        self.bookmarks.bind_project(self.project.path.as_deref());
        if restore {
            self.bm_sync_ui();
            self.project_restore();
        } else {
            // 🔧 저장·새 프로젝트·닫기 길: `bind_project`가 저장소를 비우고 워크스페이스 파일(옛 것)을 읽어 **방금 담은 북마크
            //   (니모닉 포함)가 사라지던 결함**(사용자 09-23) — 프로젝트 파일에 담긴 북마크를 그대로 되돌린다(복원 길은 `project_restore`가 한다).
            if let Some(js) = self.project.bookmarks.clone() {
                self.bookmarks.load_json(&js);
            }
            self.bm_sync_ui();
            self.project_last_json = self.project.to_document();
            self.project_mark_saved_gen();
        }
    }

    /// 프로젝트 파일 저장 — 탐색기의 마지막 선택 위치를 담아서(닫기·전환·폴더 변경·종료 = 전부 이 길).
    pub(crate) fn project_save(&mut self) -> Result<(), String> {
        self.project_capture_state();
        let r = self.project.save();
        if r.is_ok() {
            self.project_last_json = self.project.to_document();
            self.project_mark_saved_gen();
        }
        r
    }

    /// 작업 환경을 프로젝트에 담는다(사용자 09-23): 탐색기 선택 · 탭 순서(파일 = 경로만 · 스크립트 = 본문 ≤ 1 MB) ·
    /// 캐럿 + fuzzy 앵커(북마크 `make_anchor` · 10만 줄 넘는 탭은 줄 번호만) · 활성 탭 · 북마크(JSON).
    fn project_capture_state(&mut self) {
        if let Some(p) = self.project_panel.selected_path() {
            self.project.last_selected = Some(p);
        }
        let opts = nsql_bookmarks::RelocateOpts::default();
        let mut tabs = Vec::new();
        for i in 0..self.editors.tab_count() {
            let Some(tb) = self.editors.tab_box(i) else {
                continue;
            };
            let path = self.editors.path_of(i);
            let buf = tb.buf();
            let caret = tb.caret();
            let line = buf.line_of(caret);
            let col = caret.saturating_sub(buf.line_start(line));
            let mut t = project::TabState {
                path: path.clone(),
                title: self.editors.title_of(i),
                line,
                col,
                preview: self.editors.preview_id() == Some(self.editors.tab_id(i)),
                id: self.editors.tab_id(i),
                indent: self.editors.indent_of(i),
                ..project::TabState::default()
            };
            if buf.line_count() <= 100_000 {
                let lines: Vec<String> = (0..buf.line_count())
                    .map(|l| buf.line_text(l).into_owned())
                    .collect();
                let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
                let a = nsql_bookmarks::make_anchor(
                    &refs,
                    line.min(refs.len().saturating_sub(1)),
                    col as u32,
                    &opts,
                );
                t.anchor_text = a.text;
                t.before = a.before;
                t.after = a.after;
            }
            // 본문을 담는 탭 = 이름 없는 스크립트 **+ 미저장 편집이 있는 파일 탭**(사용자 09-23 "탭별로 프로젝트 파일에 · 로드 때 자동 복구 ·
            // 백업은 백업용으로만") — 파일 탭은 그때의 디스크 해시도 같이(로드 때 밖에서 바뀌었는지 알리려고). 1 MB 상한은 같다.
            let dirty_file = path.is_some() && self.editors.is_dirty(i);
            if path.is_none() || dirty_file {
                let text = tb.text();
                if text.len() <= 1 << 20 {
                    t.text = Some(text);
                    if let Some(p) = &path {
                        t.disk_hash = backups::disk_hash(p);
                    }
                }
            }
            tabs.push(t);
        }
        // 파일 탭의 미저장 본문 = 스냅숏(docs/70 §5 · 원본은 안 만진다) · clean이면 스냅숏 삭제.
        for i in 0..self.editors.tab_count() {
            let Some(p) = self.editors.path_of(i) else {
                continue;
            };
            if self.editors.is_dirty(i) {
                if let Some(tb) = self.editors.tab_box(i) {
                    let text = tb.text();
                    if text.len() <= 8 << 20 {
                        backups::write(&p, &text);
                    }
                }
            } else {
                backups::remove(&p);
            }
        }
        self.project.tabs = tabs;
        self.project.active = self.editors.active();
        self.project.bookmarks = Some(self.bookmarks.store.to_json());
        // ★ 좌측 패널 상태(사용자 09-23 "각 좌측 기능별로 복원"): 탐색기 펼침 · 보이던 패널 · 검색어 · 북마크 접힘 · 접속 표식.
        self.project.expanded = self.project_panel.expanded_dirs();
        self.project.panel = if self.project_panel.is_visible() {
            Some("project".into())
        } else if self.bm_panel.is_visible() {
            Some("bookmarks".into())
        } else if self.search.is_visible() {
            Some("search".into())
        } else if self.ext_panel.is_visible() {
            Some("ext".into())
        } else {
            None
        };
        self.project.search = Some(self.search.query_text()).filter(|s| !s.is_empty());
        self.project.bm_collapsed = self.bm_panel.collapsed_groups();
        // 접속은 **표식만**(docs/70 §2 · 26 §8 자동 재접속 금지): 붙어 있는 세션의 프로필 이름(중복 제거 · 접속 순서).
        let mut profiles: Vec<String> = Vec::new();
        for s in std::iter::once(&self.sess).chain(self.parked.iter()) {
            if s.connected && !s.profile.is_empty() && !profiles.contains(&s.profile) {
                profiles.push(s.profile.clone());
            }
        }
        self.project.profiles = profiles;
    }

    /// 프로젝트를 열었을 때 작업 환경 복원(사용자 09-23): 탭 순서대로 파일은 다시 읽고(원본 = 최신) 스크립트는 본문 그대로 ·
    /// 캐럿은 앵커로 fuzzy 재탐색(외부 수정 대응 · 북마크 `relocate`) · 활성 탭 · 북마크.
    pub(crate) fn project_restore(&mut self) {
        let tabs = self.project.tabs.clone();
        // ★ 작업 환경은 **교체**된다(사용자 09-23 "마지막 작업 상태 그대로"): 열려 있던 탭 중 복원 목록에 없는 것은 뒤에서 닫는다
        //   — 이전 프로젝트가 있었으면 그 파일(스크립트 본문)과 스냅숏(파일 탭 미저장분)에 이미 담겼고, 없었으면(파일 모드에서
        //   열기) 미저장 탭은 남긴다. 닫기는 복원 뒤에(마지막 탭을 닫으면 빈 탭이 새로 생기는 일을 피한다).
        let prev_saved = {
            let n = b"\"tabs\"";
            self.project_last_json.windows(n.len()).any(|w| w == n)
        };
        let old_ids: Vec<u64> = (0..self.editors.tab_count())
            .map(|i| self.editors.tab_id(i))
            .collect();
        if tabs.is_empty() {
            self.project_restore_panels();
            return;
        }
        let opts = nsql_bookmarks::RelocateOpts::default();
        let mut ids: Vec<Option<u64>> = Vec::new();
        let mut restored_dirty = 0usize;
        let mut scratch_remap: Vec<(u64, u64)> = Vec::new();
        for t in &tabs {
            let id = match &t.path {
                Some(p) => {
                    if !p.is_file() {
                        None
                    } else {
                        if t.preview {
                            // 미리보기 탭은 미리보기로(북마크·탐색기 한 번 클릭으로 연 것 · 편집하면 승격).
                            self.project_open_req(project_panel::OpenReq {
                                path: p.clone(),
                                permanent: false,
                            });
                        } else {
                            self.open_file(p);
                        }
                        let i = self.editors.active();
                        if self.editors.active_path().as_deref() == Some(p.as_path()) {
                            // ★ 미저장 편집분은 **프로젝트 파일**에서 그대로 올린다(사용자 09-23 "탭별로 저장해 자동 복구" · `backups/`는
                            //   백업용으로만 · 복원에 쓰지 않는다). 디스크가 그 사이 바뀌었으면(해시 다름) 올리되 로그로 알린다.
                            if let Some(body) = &t.text {
                                if let Some(tb) = self.editors.tab_box_mut(i) {
                                    tb.set_text(body);
                                }
                                restored_dirty += 1;
                                if t.disk_hash != 0 && backups::disk_hash(p) != t.disk_hash {
                                    self.log_win.push(LogEntry::new(
                                        LogKind::Info,
                                        tf(Msg::StBackupExternal, &[&p.to_string_lossy()]),
                                    ));
                                }
                            }
                            self.restore_caret(i, t, &opts);
                            if t.indent.is_some() {
                                self.editors.set_indent_of(i, t.indent);
                            }
                            // 사용자가 바꾼 이름(파일 이름과 다르면) = 복원(사용자 09-28 · 프로젝트 모드 보존).
                            let fname = p.file_name().map(|n| n.to_string_lossy().into_owned());
                            if !t.title.is_empty() && fname.as_deref() != Some(t.title.as_str()) {
                                self.editors.rename_tab(i, &t.title);
                            }
                            Some(self.editors.tab_id(i))
                        } else {
                            None
                        }
                    }
                }
                None => {
                    self.editors.new_tab(Some(t.title.clone()));
                    let i = self.editors.active();
                    if let Some(tb) = self.editors.tab_box_mut(i) {
                        if let Some(txt) = &t.text {
                            tb.set_text(txt);
                        }
                    }
                    self.restore_caret(i, t, &opts);
                    if t.indent.is_some() {
                        self.editors.set_indent_of(i, t.indent);
                    }
                    let new_id = self.editors.tab_id(i);
                    // 이름 없는 탭의 북마크(`Scratch { tab }`)는 옛 id → 새 id로 재매핑(사용자 09-23 검토 · 본문이 그대로라 줄이 맞는다).
                    if t.id != 0 {
                        scratch_remap.push((t.id, new_id));
                    }
                    Some(new_id)
                }
            };
            ids.push(id);
        }
        if let Some(Some(id)) = ids.get(self.project.active) {
            self.editors.switch_to_id(*id);
            self.sync_grid_tab();
        }
        if let Some(js) = self.project.bookmarks.clone() {
            if self.bookmarks.load_json(&js) {
                self.bookmarks.remap_scratch(&scratch_remap);
                self.bm_sync_ui();
            }
        }
        // 복원 목록에 없던 옛 탭 닫기(위 주석) — 복원으로 재사용된 탭(같은 파일)은 남는다.
        let restored: Vec<u64> = ids.iter().flatten().copied().collect();
        for id in old_ids.into_iter().rev() {
            if restored.contains(&id) {
                continue;
            }
            let Some(i) = self.editors.index_of_id(id) else {
                continue;
            };
            if !self.editors.is_dirty(i) || prev_saved {
                self.editors.close_tab_forced(i);
            }
        }
        if let Some(Some(id)) = ids.get(self.project.active) {
            self.editors.switch_to_id(*id);
            self.sync_grid_tab();
        }
        self.project_restore_panels();
        let n = restored.len();
        self.sess.status = if restored_dirty > 0 {
            tf(
                Msg::StProjectRestoredDirty,
                &[&n.to_string(), &restored_dirty.to_string()],
            )
        } else {
            tf(Msg::StProjectRestored, &[&n.to_string()])
        };
        if !self.project.profiles.is_empty() {
            // 접속은 표식만(docs/70 §2) — 어디에 붙어 있었는지 알려 주고 접속은 사용자가.
            let msg = tf(
                Msg::StProjectPrevProfiles,
                &[&self.project.profiles.join(", ")],
            );
            self.log_win.push(LogEntry::new(LogKind::Info, msg.clone()));
            self.toasts
                .push(toast::ToastKind::Info, t(Msg::MnProject).to_string(), msg);
        }
        self.project_last_json = self.project.to_document();
        self.project_mark_saved_gen();
        self.sync_open_files();
        self.layout();
        self.redraw();
    }

    /// 좌측 패널 상태 복원(사용자 09-23 "각 좌측 기능별"): 탐색기 펼침(선택은 `set_project`가) · 검색어 · 북마크 접힘 · 보이던 패널.
    fn project_restore_panels(&mut self) {
        let expanded = self.project.expanded.clone();
        if !expanded.is_empty() {
            self.project_panel.expand_dirs(&expanded);
            if let Some(sel) = self.project.last_selected.clone() {
                self.project_panel.reveal(&sel);
            }
        }
        if let Some(q) = self.project.search.clone() {
            self.search.set_query_text(&q);
        }
        let folded = self.project.bm_collapsed.clone();
        if !folded.is_empty() {
            self.bm_panel.set_collapsed_groups(folded);
        }
        let want = match self.project.panel.as_deref() {
            Some("project") => Some(("view.project", self.project_panel.is_visible())),
            Some("bookmarks") => Some(("view.bookmarks", self.bm_panel.is_visible())),
            Some("outline") => Some(("view.outline", self.outline_panel.is_visible())),
            Some("search") => Some(("view.search", self.search.is_visible())),
            Some("ext") => Some(("view.extensions", self.ext_panel.is_visible())),
            _ => None,
        };
        if let Some((cmd, visible)) = want {
            if !visible {
                self.menu_action(cmd);
            }
        }
    }

    /// 저장된 캐럿을 지금 본문에 맞춘다 — 앵커가 있으면 북마크와 같은 fuzzy 재탐색(줄 이동 · 못 찾으면 저장된 줄).
    fn restore_caret(
        &mut self,
        i: usize,
        t: &project::TabState,
        opts: &nsql_bookmarks::RelocateOpts,
    ) {
        let Some(tb) = self.editors.tab_box_mut(i) else {
            return;
        };
        let mut line = t.line;
        if !t.anchor_text.is_empty() {
            let buf = tb.buf();
            let lines: Vec<String> = (0..buf.line_count())
                .map(|l| buf.line_text(l).into_owned())
                .collect();
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            let a = nsql_bookmarks::Anchor {
                line: t.line as u32,
                col: t.col as u32,
                text: t.anchor_text.clone(),
                before: t.before.clone(),
                after: t.after.clone(),
                doc_hash: 0,
                doc_lines: 0,
            };
            if let nsql_bookmarks::Relocated::Moved(l) =
                nsql_bookmarks::relocate(&a, &refs, None, opts)
            {
                line = l;
            }
        }
        tb.goto_line(line + 1);
    }

    /// 자동 저장의 다음 마감 시각(틱 스케줄러 깨움용 · 사용자 09-23): `min(최초 변경 + change_secs, 마지막 저장·점검 + secs)`.
    pub(crate) fn project_autosave_next(&self, now: Instant) -> Option<Instant> {
        if !self.project.is_open() || !self.project_autosave_on() {
            return None;
        }
        let (secs, quick) = self.project_autosave_secs();
        let periodic = self.project_autosave_at + Duration::from_secs(secs);
        let changed = self
            .project_touch_at
            .map(|t| t + Duration::from_secs(quick));
        Some(changed.map_or(periodic, |t| t.min(periodic)).max(now))
    }

    /// (주기 점검 초 `project.autosave_secs` ≥ 5, 변경 뒤 저장 초 `project.autosave_change_secs` ≥ 1 · 주기보다 크지 않게).
    fn project_autosave_secs(&self) -> (u64, u64) {
        let secs = self.settings.int("project.autosave_secs").max(5) as u64;
        let quick = (self.settings.int("project.autosave_change_secs").max(1) as u64).min(secs);
        (secs, quick)
    }

    /// ★ 종료 직전 마지막 저장(사용자 09-23 "프로그램 종료 시 꼭 저장"): 프로젝트(자동 저장이 켜져 있을 때 · 꺼져 있으면
    /// `request_exit`의 물음이 이미 처리) + **북마크 워크스페이스**(디바운스를 기다리지 않고 지금). 창 닫기·File ▸ Exit 두 길 모두.
    pub(crate) fn flush_on_exit(&mut self) {
        if self.project.is_open() && self.project_autosave_on() {
            let _ = self.project_save();
        }
        self.bookmarks.save_now();
    }

    /// ★ 자동 저장 틱(사용자 09-28 정의 · [70 §7](../../../docs/70-autosave-and-restore.md)):
    ///   ① **감시된 변경**(변경 세대 = 탭 집합·순서·본문 세대·활성 탭 · `project_touch` = 폴더·탐색기 선택)은 **최초 변경 시점 +
    ///      `project.autosave_change_secs`(10초)** 에 1회 저장 — 그 안의 추가 변경은 타이머를 옮기지 않는다(최초 트리거 기준).
    ///   ② **감시되지 않는 변경**(캐럿 · 북마크 · 탐색기 펼침 · 패널 표시 · 검색어 · 프로필 표식)은 **마지막 저장·점검 +
    ///      `project.autosave_secs`(30초)** 주기 점검으로 — 문서를 만들어 이전 저장본과 같으면 쓰지 않는다.
    ///   ③ 어느 길이든 저장·점검하면 두 타이머를 모두 초기화한다(저장 횟수 최소화) · 변경이 없으면 쓰지 않는다.
    pub(crate) fn project_autosave_tick(&mut self) {
        if !self.project.is_open() || !self.project_autosave_on() {
            return;
        }
        let (secs, quick) = self.project_autosave_secs();
        // 최초 변경 시각(감시된 변경) — 이미 잡혀 있으면 옮기지 않는다.
        if self.project_touch_at.is_none() && self.project_autosave_dirty() {
            self.project_touch_at = Some(Instant::now());
        }
        let changed_due = self
            .project_touch_at
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(quick));
        let periodic_due = self.project_autosave_at.elapsed().as_secs() >= secs;
        if !changed_due && !periodic_due {
            return;
        }
        self.project_touch_at = None;
        self.project_autosave_at = Instant::now();
        self.project_capture_state();
        let js = self.project.to_document();
        if js == self.project_last_json {
            // 🔧 쓸 것이 없어도 "저장됨"이다 — 세대만 다르고 문서가 같으면 `*`가 영원히 남던 결함(사용자 09-28).
            self.project_mark_saved_gen();
        } else if self.project.save().is_ok() {
            self.project_last_json = js;
            self.project_mark_saved_gen();
        }
    }

    /// OPEN FILES(프로젝트 패널) 동기 — 탭 순서 · 제목 · 미저장 · 활성 · 동시 편집 칸.
    pub(crate) fn sync_open_files(&mut self) {
        if !self.project_panel.is_visible() {
            return;
        }
        let split = self.editors.split_tabs().to_vec();
        let v: Vec<project_panel::OpenFile> = self
            .editors
            .tab_list()
            .into_iter()
            .enumerate()
            .map(|(i, (id, title, active))| project_panel::OpenFile {
                id,
                title,
                // 미저장 = 탭 바의 점과 같은 판정(새 탭은 늘 · 파일은 저장본과 다를 때 · 09-28).
                dirty: self.editors.is_unsaved(i),
                active,
                grouped: split.len() > 1 && split.contains(&i),
                color: self.editors.line_color_of(i),
            })
            .collect();
        if self.project_panel.set_open_files(v) {
            self.project_touch();
        }
    }

    /// 작업 환경이 바뀌었다(탭 · 폴더) → 2초 뒤 저장(자동 저장 켬 · 프로젝트 열림).
    fn project_touch(&mut self) {
        if self.project.is_open() {
            // 최초 변경 시각은 옮기지 않는다(사용자 09-28 "최초 트리거 + N초에 저장").
            if self.project_touch_at.is_none() {
                self.project_touch_at = Some(Instant::now());
            }
            self.project_touch_seq = self.project_touch_seq.wrapping_add(1);
        }
    }

    /// ★ 프로젝트 변경 세대(값싸게 · 그리기마다): 탭마다 (id, 본문 세대) + 활성 탭 + 구조 변경 카운터를 접는다 —
    ///   프로젝트 문서를 다시 만들지 않는다(그건 자동 저장 틱의 몫). 저장하면 `project_gen_saved`에 담고, 다르면 상태줄에 `*`.
    pub(crate) fn project_change_gen(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x0100_0000_01b3);
        };
        mix(self.project_touch_seq);
        mix(self.editors.active() as u64);
        for i in 0..self.editors.tab_count() {
            mix(self.editors.tab_id(i));
            mix(self.editors.tab_box(i).map_or(0, |tb| tb.rev()));
            // ★ T-253(09-29): 캐럿 위치 · 탭별 들여쓰기 재정의도 감시(문서에 담기는 것은 전부 ①로).
            mix(self.editors.tab_box(i).map_or(0, |tb| tb.caret() as u64));
            if let Some((ts, sp)) = self.editors.indent_of(i) {
                mix(u64::from(ts) | (u64::from(sp) << 8) | (1 << 16));
            }
        }
        // 북마크 지문 · 탐색기 펼침 · 보이는 패널 · 검색어 · 북마크 그룹 접힘(T-253 · 70 §7-1).
        mix(self.bookmarks.fingerprint());
        for d in self.project_panel.expanded_dirs() {
            mix(fnv_str(&d.to_string_lossy()));
        }
        mix(u64::from(self.project_panel_code()));
        mix(fnv_str(&self.search.query_text()));
        for g in self.bm_panel.collapsed_groups() {
            mix(u64::from(g) | (1 << 32));
        }
        h
    }

    /// 보이는 좌측 패널 부호(0 없음 · 1 프로젝트 · 2 북마크 · 3 검색 · 4 확장) — 문서의 `panel`과 같은 판정.
    fn project_panel_code(&self) -> u8 {
        if self.project_panel.is_visible() {
            1
        } else if self.bm_panel.is_visible() {
            2
        } else if self.search.is_visible() {
            3
        } else if self.ext_panel.is_visible() {
            4
        } else {
            0
        }
    }

    /// 저장 직후 = 지금 세대를 저장 세대로.
    fn project_mark_saved_gen(&mut self) {
        self.project_gen_saved = self.project_change_gen();
    }

    /// 상태줄 표식: 프로젝트가 열려 있고 마지막 저장 뒤 바뀐 것이 있는가.
    pub(crate) fn project_autosave_dirty(&self) -> bool {
        self.project.is_open() && self.project_change_gen() != self.project_gen_saved
    }

    /// 종료 전 프로젝트 저장 물음(자동 저장이 꺼져 있을 때).
    pub(crate) fn ask_project_exit(&mut self) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        self.sess.status = t(Msg::StProjectAsk).into();
        let items = vec![
            CtxItem::item("project.exit_save", t(Msg::MnProjectExitSave)),
            CtxItem::item("project.exit_skip", t(Msg::MnProjectExitSkip)),
            CtxItem::Separator,
            CtxItem::item("close.cancel", t(Msg::MnCloseCancel)),
        ];
        let r = self.status_tx_rect;
        self.open_status_popup(r, items);
        self.redraw();
    }

    /// 폴더 목록이 바뀌었다 — 탐색기·메뉴 갱신(`save`면 파일에도).
    fn project_changed(&mut self, save: bool) {
        self.project_touch();
        if let Some(p) = self.project_panel.selected_path() {
            self.project.last_selected = Some(p);
        }
        if save {
            // 폴더 추가/제거 때도 작업 환경을 담아 쓴다(탭 목록이 비거나 묵던 결함 · 사용자 09-23).
            if let Err(e) = self.project_save() {
                self.sess.status = tf(Msg::ErrProjectFile, &[&e]);
            }
        }
        self.sync_project_panel_opts();
        let has_recent =
            !project::recent_list(self.settings.get("project.recent").unwrap_or("")).is_empty();
        self.project_panel.set_has_recent(has_recent);
        self.project_panel.set_project(
            self.project.name(),
            &self.project.folders,
            self.project.last_selected.clone().as_deref(),
        );
        self.rebuild_menus();
        self.layout();
        self.redraw();
    }

    pub(crate) fn sync_project_panel_opts(&mut self) {
        self.project_panel.set_list_opts(
            self.settings.flag("file.show_hidden"),
            self.settings.flag("file.show_dot"),
            self.settings.int("project.scan_max").max(0) as usize,
            self.settings.int("project.scan_threads").max(0) as usize,
        );
        self.project_panel
            .set_icons(self.settings.flag("project.icons"));
        self.editors
            .set_project_folders(self.project.folders.clone());
        self.project_panel
            .set_tooltip_delay(self.settings.int("ui.tooltip_delay_ms").max(0) as u128);
        self.project_panel
            .set_dblclick_ms(self.settings.int("ui.dblclick_ms").max(0) as u128);
        self.editors
            .set_dblclick_ms(self.settings.int("ui.dblclick_ms").max(0) as u128);
        self.bm_panel
            .set_dblclick_ms(self.settings.int("ui.dblclick_ms").max(0) as u128);
        self.search
            .set_dblclick_ms(self.settings.int("ui.dblclick_ms").max(0) as u128);
        // 타입어헤드 한 벌(객체 탐색기와 같은 설정 · 09-28) — 기동 때와 설정 변경 때.
        let ta = crate::typeahead_cfg(&self.settings);
        self.project_panel.set_typeahead(ta);
        self.bm_panel.set_typeahead(ta);
        self.outline_panel.set_typeahead(ta);
    }

    /// 기동 시작 모드(사용자 09-22 · [`startup_project_plan`]): 기본 = **파일 모드**(프로젝트 없음) · 인자 `.nsql-project` =
    /// 프로젝트 모드 · `project.restore_last`가 켜져 있고 **첫 인스턴스**이며 파일 인자가 없을 때만 마지막 프로젝트 복원.
    pub(crate) fn project_startup(&mut self) {
        self.sync_project_panel_opts();
        let plan = startup_project_plan(
            self.arg_project.as_deref(),
            !self.arg_files.is_empty(),
            self.first_instance,
            self.settings.flag("project.restore_last")
                && self.entitled(nsql_license::Feature::ProjectRestore),
            self.settings.get("project.last").unwrap_or(""),
        );
        let Some((path, from_arg)) = plan else {
            return;
        };
        match project::Project::load(&path) {
            Ok(proj) => {
                if from_arg {
                    // 복원은 기동 마지막(`project_restore` 호출부)에서 한 번 — 여기서 하면 두 번 열린다.
                    self.project_set(proj, false);
                } else {
                    self.project = proj;
                    self.project_panel.set_project(
                        self.project.name(),
                        &self.project.folders,
                        self.project.last_selected.clone().as_deref(),
                    );
                    // 🔧 편집기에도 폴더 목록을(사용자 09-28 "처음 열면 '프로젝트 탐색기에서 보기'가 비활성 · 패널을 다녀오면
                    //   활성") — 이 길은 `project_changed`를 타지 않아 탭 메뉴의 `in_project` 판정이 빈 목록을 보고 있었다.
                    self.editors
                        .set_project_folders(self.project.folders.clone());
                }
            }
            Err(e) => {
                if from_arg {
                    self.sess.status = tf(Msg::ErrProjectFile, &[&e]);
                } else {
                    let _ = self.settings.set("project.last", "");
                    self.persist_settings();
                }
            }
        }
    }

    /// 탐색기가 낸 요청 거두기(열기 · 링크 명령).
    pub(crate) fn project_pump(&mut self) {
        if let Some(id) = self.project_panel.take_command() {
            // 우클릭 ▸ 이름/경로 복사 · 파일 위치 열기(폴더·파일 공통 · 사용자 09-28).
            if let Some(p) = id.strip_prefix("path.copy_name:") {
                self.copy_path_text(Path::new(p), false);
            } else if let Some(p) = id.strip_prefix("path.copy_full:") {
                self.copy_path_text(Path::new(p), true);
            } else if let Some(p) = id.strip_prefix("path.reveal:") {
                if let Err(e) = nexa_fs::shell::reveal_in_file_manager(Path::new(p)) {
                    self.sess.status = tf(Msg::StRevealFailed, &[&e.to_string()]);
                }
            } else {
                self.project_cmd(&id);
            }
            self.redraw();
        }
        if let Some(req) = self.project_panel.take_open() {
            self.project_open_req(req);
        }
    }

    /// 탐색기 클릭 = 미리보기 탭(설정 `project.preview_tab`) · 더블클릭/Enter = 정식 탭.
    /// 큰 파일(`file.async_load_mb` 이상)은 미리보기 없이 보통 열기(자리 탭 + 스레드 · 큰 파일 확인).
    pub(crate) fn project_open_req(&mut self, req: project_panel::OpenReq) {
        let preview = self.settings.flag("project.preview_tab") && !req.permanent;
        if !preview {
            if !self.editors.promote_path(&req.path) {
                self.open_file(&req.path);
            }
            self.set_focus(Focus::Editor);
            self.layout();
            self.redraw();
            return;
        }
        if let Some(i) = self.editors.path_tab(&req.path) {
            self.editors.switch(i);
            self.layout();
            self.redraw();
            return;
        }
        let size = std::fs::metadata(&req.path).map(|m| m.len()).unwrap_or(0);
        let async_at = (self.settings.int("file.async_load_mb").max(1) as u64) << 20;
        if size >= async_at {
            self.open_file(&req.path);
            return;
        }
        match fileload::load(&req.path, "auto", None, true, None) {
            Ok(l) => {
                if let fileload::Body::Prepared(prep) = l.body {
                    let i = self.editors.open_preview(&req.path, prep, l.eol);
                    self.editors.set_encoding(i, l.used);
                    let id = self.editors.tab_id(i);
                    self.ext_track(id);
                    self.sync_grid_tab();
                    self.sync_gate();
                }
            }
            Err(e) => {
                self.sess.status = tf(Msg::StFileReadError, &[&req.path.display().to_string(), &e]);
            }
        }
        self.layout();
        self.redraw();
    }

    /// 프로젝트 탐색기에서 파일 보기(탭 메뉴 · 사용자 09-22): 패널이 닫혀 있으면 열고 · 자동 확장 설정과 무관하게
    /// 조상을 펼쳐 선택 · 보이게 스크롤 · 포커스를 패널로.
    pub(crate) fn reveal_in_project(&mut self, path: &Path) {
        if !self.project_panel.is_visible() {
            self.menu_action("view.project");
        }
        if self.project_panel.reveal(path) {
            self.set_focus(Focus::Project);
        }
        self.redraw();
    }

    /// 활성 탭이 바뀌면 탐색기의 선택을 그 파일에 맞춘다(프로젝트 폴더 안 파일만) — 기본은 펼치지 않고 표시만(`mark_path`) ·
    /// `project.auto_reveal`이면 조상을 펼치고 스크롤(`reveal` · 포커스는 안 옮긴다).
    pub(crate) fn project_sync_active(&mut self) {
        let id = self.editors.active_id();
        if id == self.last_synced_tab {
            return;
        }
        self.last_synced_tab = id;
        if !self.project.is_open() {
            return;
        }
        let Some(p) = self.editors.active_path() else {
            return;
        };
        if !self.editors.in_project(&p) {
            return;
        }
        if self.settings.flag("project.auto_reveal") {
            self.project_panel.reveal(&p);
        } else {
            self.project_panel.mark_path(&p);
        }
        self.redraw();
    }

    /// ★ 상태줄 자동 저장 메뉴의 답(사용자 09-28): `project` = 프로젝트 파일을 선택한 채 그 폴더를 OS 탐색기로 ·
    ///   `backups` = 일반 파일 자동 저장 위치(`backups/files`)를 이 탭의 최신 스냅숏을 선택한 채(없으면 폴더만) · `settings` = 설정 창.
    pub(crate) fn autosave_pick(&mut self, what: &str) {
        match what {
            "project" => {
                let Some(p) = self.project.path.clone() else {
                    self.sess.status = t(Msg::StAutosaveNone).into();
                    return;
                };
                self.sess.status =
                    match nexa_fs::shell::reveal_in_file_manager(&p).map_err(|e| e.to_string()) {
                        Ok(()) => tf(Msg::StAutosaveOpened, &[&p.display().to_string()]),
                        Err(e) => e,
                    };
            }
            "backups" => {
                let Some(dir) = crate::backups::snapshot_dir() else {
                    self.sess.status = t(Msg::StAutosaveNone).into();
                    return;
                };
                let _ = std::fs::create_dir_all(&dir);
                let snap = self
                    .editors
                    .active_path()
                    .and_then(|p| crate::backups::snapshot_file(&p));
                self.sess.status = match snap {
                    Some(f) => match nexa_fs::shell::reveal_in_file_manager(&f)
                        .map_err(|e| e.to_string())
                    {
                        Ok(()) => tf(Msg::StAutosaveOpened, &[&f.display().to_string()]),
                        Err(e) => e,
                    },
                    None => match crate::open_external(&dir) {
                        Ok(()) => tf(Msg::StAutosaveNoSnapshot, &[&dir.display().to_string()]),
                        Err(e) => e,
                    },
                };
            }
            "settings" => {
                self.prefs_query = Some("project.autosave".into());
                self.open_prefs = true;
            }
            _ => {}
        }
        self.redraw();
    }

    /// 파일 이름(`name.ext`) 또는 전체 경로를 클립보드로 + 상태줄 안내(탭 메뉴 · 프로젝트 탐색기 · 09-28).
    pub(crate) fn copy_path_text(&mut self, path: &Path, full: bool) {
        let text = if full {
            nexa_fs::shell::explorer_path(path)
        } else {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned())
        };
        let text = if cfg!(windows) {
            text
        } else if full {
            path.to_string_lossy().into_owned()
        } else {
            text
        };
        self.sess.status = if clipboard::write_text(&text) {
            tf(Msg::StCopiedText, &[&text])
        } else {
            t(Msg::ErrClipboard).into()
        };
    }

    /// ★ 폴더 모드의 탭 이름 보존 파일(`<폴더>/.nsql/tab-titles.tsv` · 줄 = `경로\t이름` · 사용자 09-28) — 폴더 모드가 아니면 None.
    fn folder_titles_file(&self) -> Option<PathBuf> {
        match project::WorkMode::of(self.project.path.as_deref(), self.arg_folder.as_deref()) {
            project::WorkMode::Folder(_) => project::WorkMode::of(None, self.arg_folder.as_deref())
                .local_dir()
                .map(|d| d.join("tab-titles.tsv")),
            _ => None,
        }
    }

    fn folder_titles_read(&self) -> Vec<(String, String)> {
        let Some(f) = self.folder_titles_file() else {
            return Vec::new();
        };
        std::fs::read_to_string(f)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect()
    }

    /// 폴더 모드: 탭 이름을 바꾸면 그 파일의 이름을 기록(파일 이름으로 되돌리면 지움) — 파일 탭만(이름 없는 탭은 폴더 모드에서 복원되지 않는다).
    pub(crate) fn folder_title_remember(&mut self, i: usize) {
        let (Some(f), Some(path)) = (self.folder_titles_file(), self.editors.path_of(i)) else {
            return;
        };
        let key = path.to_string_lossy().into_owned();
        let mut rows: Vec<(String, String)> = self
            .folder_titles_read()
            .into_iter()
            .filter(|(p, _)| *p != key)
            .collect();
        if self.editors.is_custom_title(i) {
            rows.push((key, self.editors.title_of(i)));
        }
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let body: String = rows.iter().map(|(p, t)| format!("{p}\t{t}\n")).collect();
        let _ = std::fs::write(f, body);
    }

    /// 폴더 모드: 연 파일에 기록된 이름이 있으면 그 이름으로.
    pub(crate) fn folder_title_apply(&mut self, path: &Path) {
        if self.folder_titles_file().is_none() {
            return;
        }
        let key = path.to_string_lossy().into_owned();
        if let Some((_, t)) = self
            .folder_titles_read()
            .into_iter()
            .find(|(p, _)| *p == key)
        {
            if self.editors.active_path().as_deref() == Some(path) {
                let i = self.editors.active();
                self.editors.rename_tab(i, &t);
            }
        }
    }

    /// 지금 작업 환경을 담은 프로젝트 문서가 마지막 저장본과 다른가(종료 물음 · 자동 저장 틱과 같은 비교 · 09-28).
    pub(crate) fn project_unsaved(&mut self) -> bool {
        if !self.project.is_open() {
            return false;
        }
        self.project_capture_state();
        self.project.to_document() != self.project_last_json
    }

    /// 프로젝트 자동 저장·복원은 Pro(25 §13-3) — 설정이 켜져 있어도 게이트가 켜진 무료면 꺼진 것으로(기본은 게이트 끔 · D-145).
    pub(crate) fn project_autosave_on(&self) -> bool {
        self.settings.flag("project.autosave")
            && self.entitled(nsql_license::Feature::ProjectRestore)
    }
}
