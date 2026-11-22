//! App — 프로젝트·작업 환경·다중 열기(docs/67·70).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

/// 프로젝트 동기화 결과(`App::project_sync_external` · 67 §9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjSync {
    /// 디스크가 기준본 그대로(또는 동기화 끔).
    Same,
    /// 다른 인스턴스의 문서를 합쳤다(바뀐 것이 없었어도 기준본은 옮겨졌다).
    Merged,
    /// 실행 중인 탭의 본문을 건드려야 해서 미뤘다 — 지금 저장하면 저쪽 변경을 덮는다.
    Busy,
}

/// 문서 바이트의 지문(같은 프로세스 안 비교용 · 저장하지 않는다).
fn doc_mark(doc: &[u8]) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    h.write(doc);
    h.finish()
}

/// 새 문서 id — 겹치지 않는 40~48비트 값(JSON 숫자 = f64의 정수 범위 안 · 옛 파일의 작은 탭 id와도 안 겹친다).
fn new_doc_id() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u32(std::process::id());
    (h.finish() & ((1 << 48) - 1)) | (1 << 40)
}

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
            danger: false,
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
        // ★ T-172 (C): 활성 자리 탭을 닫는다 / 원본을 이 파일로 지정한다(띠 버튼과 같은 길 · 자동 시험).
        if id == "project.missing_close" {
            self.project_missing_pick(false);
            return;
        }
        if let Some(p) = id.strip_prefix("project.missing_found:") {
            if let Some(mid) = self.missing_id_at(self.editors.active()) {
                self.project_missing_found(mid, Path::new(p));
            }
            return;
        }
        // ★ T-172 (B): `project.relocate_folder:<순번>` = 폴더 고르기 창 · `…:<순번>:<경로>` = 바로(자동 시험).
        // 경로 변수의 이 PC 자리(시험·자동화용 기동 명령 · `project.var_set:이름=폴더`).
        if let Some(rest) = id.strip_prefix("project.var_set:") {
            if let Some((name, dir)) = rest.split_once('=') {
                self.project_set_var_pc(name, Path::new(dir.trim()));
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("project.relocate_folder:") {
            let (i, dir) = match rest.split_once(':') {
                Some((a, b)) => (a.parse::<usize>().ok(), Some(PathBuf::from(b))),
                None => (rest.parse::<usize>().ok(), None),
            };
            if let Some(i) = i.filter(|&i| i < self.project.folders.len()) {
                match dir {
                    Some(d) => self.project_relocate_folder(i, &d),
                    None => {
                        self.relocate_idx = Some(i);
                        self.file_purpose = FilePurpose::Project;
                        // 시작 = 옛 자리에서 위로 올라가며 처음 있는 폴더.
                        self.folder_start = self.project.folders[i]
                            .ancestors()
                            .find(|a| a.is_dir())
                            .map(Path::to_path_buf);
                        self.open_file_dlg = Some(PickerMode::Folder);
                    }
                }
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
        // 자체 시험: 프로젝트 패널을 **정해진 상태로**(`project.panel:on|off`) — `view.project`는 토글이라 앞서 읽은
        // 프로젝트 파일의 `panel` 상태에 따라 닫히기도 한다(T-177 · 기능 점검 S06·S46).
        if let Some(want) = id.strip_prefix("project.panel:") {
            let on = !matches!(want.trim(), "off" | "0" | "false");
            if self.project_panel.is_visible() != on {
                self.menu_action("view.project");
            }
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
                // T-507: 닫으면 탭이 통째로 사라진다 → 결과 탭 미적용 편집을 먼저 묻는다.
                if self.project.is_open() && self.rdirty_ask("project.close") {
                    return;
                }
                if self.project.is_open() {
                    let _ = self.project_save();
                    self.project_set(project::Project::default(), false);
                    // ★ 닫은 뒤 = 처음 실행 상태(사용자 09-23): 전체 상태는 위 저장으로 프로젝트 파일·스냅숏에 담겼다.
                    self.reset_workspace_to_initial();
                    self.sess.status = t(Msg::StProjectClosed).into();
                }
            }
            "project.add_folder" => {
                self.relocate_idx = None;
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
        // T-507: 다른 프로젝트로 바꾸면 지금 탭들이 사라진다 → 결과 탭 미적용 편집을 먼저 묻는다(같은 파일 다시 열기는 해당 없음).
        if self.project.path.as_deref() != Some(path)
            && self.rdirty_ask(&format!("project.load:{}", path.to_string_lossy()))
        {
            return;
        }
        let pc = self
            .settings
            .get("project.pc_paths")
            .unwrap_or("")
            .to_string();
        match project::Project::load_pc(path, &pc) {
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
        // ★ 저장 보호(67 §9): 지금 프로젝트를 **같은 파일**에 쓰는 명시 저장(`project.save`)도 다른 인스턴스가 쓴 것을 먼저 합친다 —
        //   자동 저장·닫기 길(`project_save`)만 합치고 이 길은 바로 덮어쓰던 구멍(협업 11-19 관찰). 새 프로젝트·다른 이름은 해당 없음.
        if !self.project_new_fresh && self.project.path.as_deref() == Some(path.as_path()) {
            let _ = self.project_sync_external(true);
        }
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
                self.project_warn_absolute(true);
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

    /// ★ T-172 (A)(B)(사용자 11-22 "이 PC 대응표에만"): 못 찾은 프로젝트 폴더를 이 PC의 다른 자리로 다시 지정한다 — 프로젝트 파일의
    ///   폴더 정의는 그대로 두고, 설정 `project.pc_paths`에 `고유 번호 + 폴더 열쇠 → 이 PC의 경로`를 적는다. 그 폴더 아래라서
    ///   못 찾았던 탭은 다시 연다.
    pub(crate) fn project_relocate_folder(&mut self, i: usize, dir: &Path) {
        let taken = self
            .project
            .folders
            .iter()
            .enumerate()
            .any(|(j, f)| j != i && f == dir);
        if i >= self.project.folders.len() || !dir.is_dir() || taken {
            return;
        }
        let key = self.project.folder_key(i);
        let Some(id) = self.project.ensure_id() else {
            return;
        };
        let Some((old, new)) = self.project.relocate_folder(i, dir) else {
            return;
        };
        let raw = project::pc_paths_set(
            self.settings.get("project.pc_paths").unwrap_or(""),
            &id,
            &key,
            &new,
        );
        let _ = self.settings.set("project.pc_paths", &raw);
        self.persist_settings();
        self.missing_repoint(&old, &new);
        self.sess.status = tf(
            Msg::StProjectFolderRelocated,
            &[&nexa_fs::path::display(&old), &nexa_fs::path::display(&new)],
        );
        self.project_changed(true);
    }

    /// 못 찾은 탭들 가운데 `old` 아래를 가리키던 것을 `new` 아래로 옮기고, 그래서 찾게 된 탭은 다시 연다(폴더 다시 지정 ·
    /// 경로 변수의 이 PC 자리 공용).
    fn missing_repoint(&mut self, old: &Path, new: &Path) {
        for m in &mut self.proj_missing {
            if let Some(p) = m.path.as_mut() {
                if let Ok(r) = p.strip_prefix(old) {
                    *p = new.join(r);
                }
            }
        }
        let (found, still): (Vec<_>, Vec<_>) = std::mem::take(&mut self.proj_missing)
            .into_iter()
            .partition(|m| m.path.as_ref().is_some_and(|p| p.is_file()));
        self.proj_missing = still;
        let opts = nsql_bookmarks::RelocateOpts::default();
        let (mut dirty, mut remap) = (0usize, Vec::new());
        for t in &found {
            let _ = self.project_restore_tab(t, &opts, &mut dirty, &mut remap);
            // 자리 탭은 진짜 탭을 연 뒤에 닫는다(마지막 탭을 닫아 빈 탭이 생기는 일을 피한다).
            self.missing_tab_close(t.id);
        }
    }

    /// ★ T-172 P3 2단계(D-498 · 73 §4-2 "이 PC만의 값"): 경로 변수 `name`의 이 PC 자리를 `dir`로 — 프로젝트 파일의 `vars`는 그대로
    ///   두고 설정 `project.pc_paths`에 `고유 번호 + var:이름 → 폴더`를 적는다. 그 변수 아래라서 못 찾았던 탭은 다시 연다.
    ///   진입점 = 자리 탭 [찾아보기…]에서 고른 파일로 추론(`project_missing_found`) · 기동 명령 `project.var_set:이름=폴더`.
    pub(crate) fn project_set_var_pc(&mut self, name: &str, dir: &Path) -> bool {
        if !self.project.is_open() || !dir.is_dir() {
            return false;
        }
        let Some(id) = self.project.ensure_id() else {
            return false;
        };
        let Some((old, new)) = self.project.set_var_pc(name, dir) else {
            return false;
        };
        let raw = project::pc_paths_set(
            self.settings.get("project.pc_paths").unwrap_or(""),
            &id,
            &format!("{}{}", project::PC_VAR_KEY, name.trim()),
            &new,
        );
        let _ = self.settings.set("project.pc_paths", &raw);
        self.persist_settings();
        self.missing_repoint(&old, &new);
        let msg = tf(
            Msg::StProjectVarSet,
            &[name.trim(), &nexa_fs::path::display(&new)],
        );
        self.log_win
            .push(LogEntry::new(LogKind::Info, format!("project: {msg}")));
        self.sess.status = msg;
        self.project_changed(true);
        true
    }

    // ───────── ★ T-172 (C)(D-189 · D-496): 못 찾은 탭의 자리 탭 ─────────
    // 원본을 못 찾은 파일 탭은 탭 줄의 **그 자리에** 빈 읽기 전용 뷰 탭(열쇠 `missing:<문서 id>`)으로 보이고, 띠가 사정과
    // [찾아보기…] [탭 닫기]를 낸다. 저장할 때 자리 탭이 살아 있으면 원래 기록(경로 · 캐럿 · 미저장 본문)을 그 자리에 되적고,
    // 닫혔으면 기록에서도 뺀다(미저장 본문이 있으면 닫기 전에 한 번 더 묻는다).

    /// 탭 `i`가 자리 탭이면 그 못 찾은 탭의 문서 id.
    pub(crate) fn missing_id_at(&self, i: usize) -> Option<u64> {
        self.editors
            .view_key_of(i)?
            .strip_prefix("missing:")?
            .parse()
            .ok()
    }

    fn missing_tab_close(&mut self, mid: u64) {
        let hit = (0..self.editors.tab_count()).find(|&i| self.missing_id_at(i) == Some(mid));
        if let Some(i) = hit {
            self.editors.close_tab_forced(i);
        }
    }

    /// 활성 탭이 자리 탭이면 그 띠.
    pub(crate) fn project_missing_banner(&self, tab_id: u64) -> Option<extfile::BannerView> {
        let i = self.editors.index_of_id(tab_id)?;
        let mid = self.missing_id_at(i)?;
        let m = self.proj_missing.iter().find(|m| m.id == mid)?;
        let path = m
            .path
            .as_ref()
            .map(|p| nexa_fs::path::display(p))
            .unwrap_or_default();
        let msg = if self.missing_close_armed == Some(mid) {
            Msg::ProjBanMissingDiscard
        } else if m.text.is_some() {
            Msg::ProjBanMissingSnap
        } else {
            Msg::ProjBanMissing
        };
        Some(extfile::BannerView {
            text: tf(msg, &[&path]),
            buttons: vec![
                (
                    extfile::BannerHit::MissingBrowse,
                    t(Msg::BtnProjMissingBrowse).to_string(),
                ),
                (
                    extfile::BannerHit::MissingClose,
                    t(Msg::BtnProjMissingClose).to_string(),
                ),
            ],
            danger: true,
        })
    }

    /// 띠 버튼(활성 자리 탭) — 찾아보기 = 파일 열기 창(고른 파일로 그 탭을 되살린다) · 닫기 = 탭 닫기 길 그대로.
    pub(crate) fn project_missing_pick(&mut self, browse: bool) {
        let i = self.editors.active();
        let Some(mid) = self.missing_id_at(i) else {
            return;
        };
        if browse {
            self.missing_browse = Some(mid);
            self.file_purpose = FilePurpose::Editor;
            self.open_file_dlg = Some(PickerMode::Open);
        } else {
            self.close_tab_guarded(i);
        }
    }

    /// 자리 탭을 닫으려 한다 — 프로젝트 파일에 미저장 본문이 들어 있으면 **한 번 더** 눌러야 닫힌다(D-496 · 닫으면 그 본문이 사라진다).
    ///   막았으면 true(상태줄·띠로 알림).
    pub(crate) fn project_missing_close_blocked(&mut self, i: usize) -> bool {
        let Some(mid) = self.missing_id_at(i) else {
            return false;
        };
        let has_body = self
            .proj_missing
            .iter()
            .any(|m| m.id == mid && m.text.is_some());
        if !has_body {
            return false;
        }
        if self.missing_close_armed == Some(mid) {
            self.missing_close_armed = None;
            return false;
        }
        self.missing_close_armed = Some(mid);
        let id = self.editors.tab_id(i);
        self.editors.switch_to_id(id);
        self.sync_grid_tab();
        self.sess.status = tf(Msg::ProjBanMissingDiscard, &[""]).trim_end().to_string();
        self.ext_banner_sync();
        true
    }

    /// 못 찾은 탭의 원본을 사용자가 찾아 주었다 → 그 파일로 탭을 되살리고(캐럿 · 미저장 본문 그대로) 자리 탭을 닫는다.
    pub(crate) fn project_missing_found(&mut self, mid: u64, path: &Path) {
        let Some(k) = self.proj_missing.iter().position(|m| m.id == mid) else {
            return;
        };
        if !path.is_file() {
            return;
        }
        // ★ T-172 P3 2단계: 못 찾은 경로가 경로 변수 아래(`${var:이름}/꼬리`)이고 고른 파일이 같은 꼬리로 끝나면, 그 변수의
        //   **이 PC 자리**를 추론해 적는다 — 같은 변수 아래의 다른 못 찾은 탭도 함께 열린다(이 탭도 그 길에서 열렸으면 끝).
        let hint = self.proj_missing[k]
            .path
            .as_ref()
            .and_then(|old| self.project.var_of_path(old))
            .filter(|(_, tail)| path.ends_with(tail));
        if let Some((name, tail)) = hint {
            let mut dir = path.to_path_buf();
            for _ in tail.components() {
                dir.pop();
            }
            if self.project_set_var_pc(&name, &dir) {
                let Some(k2) = self.proj_missing.iter().position(|m| m.id == mid) else {
                    return;
                };
                return self.project_missing_found_at(k2, mid, path);
            }
        }
        self.project_missing_found_at(k, mid, path);
    }

    fn project_missing_found_at(&mut self, k: usize, mid: u64, path: &Path) {
        let mut tab = self.proj_missing.remove(k);
        tab.path = Some(path.to_path_buf());
        let opts = nsql_bookmarks::RelocateOpts::default();
        let (mut dirty, mut remap) = (0usize, Vec::new());
        let _ = self.project_restore_tab(&tab, &opts, &mut dirty, &mut remap);
        self.missing_tab_close(mid);
        self.project_changed(true);
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
        // 미저장 탭의 툴팁 "저장(프로젝트 파일)" 줄 = 이 프로젝트 파일(사용자 10-07).
        self.editors.set_tip_saved_fallback(p.path.clone());
        // 못 찾은 탭 목록은 **이 프로젝트 문서에 아직 적혀 있는 것만** 남긴다 — 저장 뒤 다시 들어오는 길(같은 문서)에서는
        //   그대로 두어야 다음 저장에서도 보존된다(11-22 V1: 여기서 비워 두 번째 저장 때 사라졌다) · 다른 프로젝트로 바뀌면
        //   그 문서에 없으니 걷힌다(복원하는 길은 `project_restore`가 새로 채운다).
        self.proj_missing
            .retain(|m| p.tabs.iter().any(|t| t.path == m.path));
        let switched = self.project.path != p.path;
        self.project = p;
        self.apply_project_settings();
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
        // ★ T-172: 다른 프로젝트를 열었는데 등록 폴더를 못 찾았다 → 알린다(로그 + 알림 · 탐색기 우클릭 "폴더 다시 지정…").
        let gone = self.project.folders.iter().filter(|f| !f.is_dir()).count();
        if switched {
            // 다른 프로젝트 = 절대 경로 알림 기준을 처음으로(T-172 · D-188).
            self.project_abs_warned = 0;
        }
        if switched && gone > 0 {
            let msg = tf(Msg::StProjectFoldersMissing, &[&gone.to_string()]);
            self.log_win.push(LogEntry::new(LogKind::Info, msg.clone()));
            self.toasts
                .push(toast::ToastKind::Info, t(Msg::MnProject).to_string(), msg);
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
                self.bookmarks.load_doc_json(&js, &self.tab_doc_ids);
            }
            self.bm_sync_ui();
            self.project_note_disk(self.project.to_document());
            self.project_mark_saved_gen();
        }
    }

    /// ★ T-39b(10-30 · 67 §8-3 · D-406·D-407): 프로젝트 `settings` 절 → 설정 **덮어쓰기 층**(소유자 `project` · 파일에 저장되지 않음 ·
    ///   프로젝트를 닫거나 바꾸면 통째로 걷힘) + `[문법]` 묶음의 들여쓰기 → 편집기 문법 층. 허용 키(`project_allowed`)만 · 나머지는 무시 +
    ///   로그 한 줄(받은 파일로 위험한 동작이 켜지지 않게 · 67 §8-2). 걷힌 키·넣은 키는 `apply_setting`으로 즉시 반영.
    pub(crate) fn apply_project_settings(&mut self) {
        let mut touched = self.settings.clear_overlay("project");
        let mut by_syntax: std::collections::BTreeMap<String, (Option<u8>, Option<bool>)> =
            std::collections::BTreeMap::new();
        let mut ignored: Vec<String> = Vec::new();
        for (group, key, raw) in self.project.settings.clone() {
            if !nsql_settings::project_allowed(&key) {
                ignored.push(key);
                continue;
            }
            match group {
                None => match self.settings.set_overlay(&key, &raw, "project") {
                    Ok(_) => {
                        if !touched.contains(&key) {
                            touched.push(key);
                        }
                    }
                    Err(_) => ignored.push(key),
                },
                Some(g) => {
                    let slot = by_syntax.entry(g.clone()).or_default();
                    match key.as_str() {
                        "editor.tab_size" => {
                            match raw.parse::<u8>().ok().filter(|v| (1..=16).contains(v)) {
                                Some(v) => slot.0 = Some(v),
                                None => ignored.push(format!("[{g}] {key}")),
                            }
                        }
                        "editor.indent_spaces" => match raw.as_str() {
                            "on" | "true" => slot.1 = Some(true),
                            "off" | "false" => slot.1 = Some(false),
                            _ => ignored.push(format!("[{g}] {key}")),
                        },
                        _ => ignored.push(format!("[{g}] {key}")),
                    }
                }
            }
        }
        self.editors.set_syntax_indent(by_syntax);
        for k in &touched {
            self.apply_setting(k);
        }
        if !ignored.is_empty() {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                tf(Msg::LogProjectSettingsIgnored, &[&ignored.join(", ")]),
            ));
        }
        self.prefs_sync_project_scope();
    }

    /// ★ T-39b 범위 탭(67 §8-4): 설정 창에 프로젝트 절의 평키 값(= 덮어쓰기 층 `project`의 실제 적용값)과 "프로젝트에서 덮어씀" 덧말을 넣는다
    ///   — 프로젝트 없음 = `None`(스위치 숨김).
    pub(crate) fn prefs_sync_project_scope(&mut self) {
        let vals = self
            .project
            .is_open()
            .then(|| self.settings.overlay_of("project"));
        self.prefs_win.set_project_scope(vals);
    }

    /// ★ T-39b 범위 탭: 설정 창 "프로젝트"에서 바꾼 값(`Some`) / 제거(`None`) → 헤더 `settings` 절(평키) → 즉시 적용 → 자동 저장 감시 ·
    ///   허용 키·정규화 통과 값만(아니면 카드에 오류 표시 · 절은 그대로).
    pub(crate) fn project_setting_set(&mut self, key: &str, value: Option<&str>) {
        if !self.project.is_open() || !nsql_settings::project_allowed(key) {
            return;
        }
        let normalized = match value.map(str::trim).filter(|v| !v.is_empty()) {
            None => None,
            Some(v) => {
                match nsql_settings::entry(key).and_then(|e| nsql_settings::normalize(e.kind, v)) {
                    Some(n) => Some(n),
                    None => {
                        let allowed = nsql_settings::entry(key)
                            .map_or_else(String::new, |e| nsql_settings::allowed(e.kind));
                        self.prefs_win.set_error(key, allowed);
                        self.prefs_win.redraw();
                        return;
                    }
                }
            }
        };
        self.project
            .settings
            .retain(|(g, k, _)| !(g.is_none() && k == key));
        if let Some(n) = normalized {
            self.project.settings.push((None, key.to_string(), n));
        }
        self.apply_project_settings();
        self.project_changed(false);
        self.prefs_win.refresh(&self.settings);
        self.prefs_win.redraw();
    }

    /// ★ T-39b(67 §8-4): 상태줄 팝업 "프로젝트에 저장" — 지금 탭의 들여쓰기를 프로젝트 `settings`의 `[문법]` 묶음에(그 문법 두 키만 갈아 끼움)
    ///   → 즉시 적용 → 자동 저장 감시.
    pub(crate) fn indent_save_project(&mut self, ts: u8, spaces: bool) {
        if !self.project.is_open() {
            return;
        }
        let g = self.editors.syntax_name().to_lowercase();
        self.project.settings.retain(|(gr, k, _)| {
            !(gr.as_deref() == Some(g.as_str())
                && (k == "editor.tab_size" || k == "editor.indent_spaces"))
        });
        self.project
            .settings
            .push((Some(g.clone()), "editor.tab_size".into(), ts.to_string()));
        self.project.settings.push((
            Some(g.clone()),
            "editor.indent_spaces".into(),
            if spaces { "on" } else { "off" }.into(),
        ));
        self.apply_project_settings();
        self.project_changed(false);
        self.sess.status = tf(Msg::StIndentSavedProject, &[&g]);
        self.redraw();
    }

    /// ★ T-69(67 §8-3 문법 층): 상태줄 팝업 "이 문법의 기본으로 저장" — `<설정 폴더>/Packages/User/<문법>.nexa-settings`(JSON 객체 · 있으면
    ///   두 키만 갱신 · 나머지 보존) → 레지스트리를 다시 훑어 편집기 문법 층 교체(탭 재정의 없는 탭에 즉시).
    pub(crate) fn indent_save_syntax(&mut self, ts: u8, spaces: bool) {
        use nsql_settings::json::{dump, parse, Json};
        let name = self.editors.syntax_name();
        let Some(dir) = crate::syntax::SyntaxRegistry::packages_dir() else {
            self.sess.status = tf(Msg::StIndentSaveSyntaxFail, &["no config folder"]);
            return;
        };
        let user = dir.join("User");
        let path = user.join(format!("{name}.nexa-settings"));
        let mut obj: Vec<(String, Json)> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| parse(&t).ok())
            .and_then(|j| match j {
                Json::Obj(items) => Some(items),
                _ => None,
            })
            .unwrap_or_default();
        for (k, v) in [
            ("editor.tab_size", Json::Num(f64::from(ts))),
            ("editor.indent_spaces", Json::Bool(spaces)),
        ] {
            match obj.iter_mut().find(|(key, _)| key == k) {
                Some(slot) => slot.1 = v,
                None => obj.push((k.to_string(), v)),
            }
        }
        let r = std::fs::create_dir_all(&user)
            .and_then(|()| std::fs::write(&path, format!("{}\n", dump(&Json::Obj(obj)))));
        match r {
            Ok(()) => {
                let defaults = crate::syntax::SyntaxRegistry::load().indent_defaults();
                self.editors.set_syntax_defaults(defaults);
                self.sess.status = tf(
                    Msg::StIndentSavedSyntax,
                    &[&name, &nexa_fs::path::display(&path)],
                );
            }
            Err(e) => self.sess.status = tf(Msg::StIndentSaveSyntaxFail, &[&e.to_string()]),
        }
        self.redraw();
    }

    /// 방금 읽거나 쓴 프로젝트 문서를 기준본으로 삼고 그 순간의 파일 서명을 기억한다(다음 동기화 = 서명이 달라졌을 때만 읽는다).
    fn project_note_disk(&mut self, doc: Vec<u8>) {
        self.project_ours_mark = doc_mark(&doc);
        self.project_last_json = doc;
        self.project_sig = self
            .project
            .path
            .as_deref()
            .and_then(nexa_fs::watch::file_sig);
    }

    /// 병합 열쇠 → 지금 열려 있는 탭 id.
    fn project_tab_of(&self, k: &projsync::TabKey) -> Option<u64> {
        (0..self.editors.tab_count())
            .find(|&i| {
                let path = self.editors.path_of(i);
                match k {
                    projsync::TabKey::File(p) => path.as_deref() == Some(p.as_path()),
                    projsync::TabKey::Id(d) => {
                        path.is_none() && self.tab_doc_ids.get(&self.editors.tab_id(i)) == Some(d)
                    }
                }
            })
            .map(|i| self.editors.tab_id(i))
    }

    /// ★ 다중 인스턴스 동기화(67 §9 · 사용자 11-19 "A에서 저장한 것을 B가 포커스 받을 때 갱신"): 프로젝트 파일이 **다른 인스턴스**
    ///   (또는 git pull)로 바뀌었으면 항목 단위 3-way로 우리 상태에 합친다 — 기준본 = 마지막으로 읽거나 쓴 문서(`project_last_json`).
    ///   - 탭 본문: 저쪽만 바꿈 = 조용히 반영(되돌리기 한 단계) · 둘 다 바꿈 = 줄 단위 병합(병합 전 백업) · 겹침 = 그 탭에 띠.
    ///   - 탭 열기·닫기 · 폴더 · `settings` 절 · 북마크 = 항목 단위(`projsync::plan` · `Store::merge3`) · 화면 상태는 합치지 않는다.
    ///
    ///   부르는 때 = 메인 창이 포커스를 받을 때 + **저장 직전**(자동 저장 포함 = 덮어쓰기 방지). `force` = 실행 중인 탭이 있어도 진행
    ///   (닫기·종료 길). 설정 `project.external_change` = `auto`(기본) / `ask`(본문은 늘 띠로) / `off`(종전 = 마지막에 쓴 쪽이 이김).
    pub(crate) fn project_sync_external(&mut self, force: bool) -> ProjSync {
        let Some(path) = self.project.path.clone() else {
            return ProjSync::Same;
        };
        let mode = self
            .settings
            .get("project.external_change")
            .unwrap_or("auto")
            .to_string();
        if self.project_last_json.is_empty() || mode == "off" {
            return ProjSync::Same;
        }
        // 값싼 1차 판정 = 서명(크기 · 수정 시각 · 파일 식별자). 파일이 없으면 다음 저장이 되살린다.
        let sig = nexa_fs::watch::file_sig(&path);
        if sig.is_none() || sig == self.project_sig {
            return ProjSync::Same;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            return ProjSync::Same;
        };
        if bytes == self.project_last_json {
            self.project_sig = sig;
            return ProjSync::Same;
        }
        // 못 읽는 문서(깨졌거나 쓰는 중)면 합칠 수 없다 — 종전처럼 우리 상태가 다음 저장에 쓰인다.
        // 이 PC의 폴더 대응표를 세 문서에 똑같이 입힌다(T-172 (A)) — 탭·폴더 경로가 지금 열린 것과 같은 자리로 풀려야 맞춰진다.
        let pc = self
            .settings
            .get("project.pc_paths")
            .unwrap_or("")
            .to_string();
        let (Ok(theirs), Ok(base)) = (
            project::Project::from_bytes_pc(&path, &bytes, &pc),
            project::Project::from_bytes_pc(&path, &self.project_last_json, &pc),
        ) else {
            return ProjSync::Same;
        };
        self.project_capture_state();
        // 우리 것도 같은 길(문서 → 모델)로 풀어 비교한다(경로 표기·본문 바이트가 같은 규칙을 거치게).
        let ours_doc = self.project.to_document();
        // 마지막으로 쓰거나 맞춘 뒤 우리가 바꾼 것이 없는가 — 없으면 합친 뒤에도 "쓸 것 없음"으로 둔다(아래).
        let was_clean = doc_mark(&ours_doc) == self.project_ours_mark;
        let Ok(ours) = project::Project::from_bytes_pc(&path, &ours_doc, &pc) else {
            return ProjSync::Same;
        };
        let plan = projsync::plan(&base, &ours, &theirs);
        // 실행 중인 탭의 본문은 바꾸지 않는다(실행 범위·오류 줄이 어긋난다) → 통째로 미룬다(기준본을 옮기지 않으므로 다음에 다시 온다).
        let touches_running = plan.tabs.iter().any(|op| match op {
            projsync::TabOp::Body { key, .. }
            | projsync::TabOp::Revert { key, .. }
            | projsync::TabOp::Close(key) => self
                .project_tab_of(key)
                .is_some_and(|id| self.editors.is_running(id)),
            projsync::TabOp::Open(_) => false,
        });
        if touches_running && !force {
            return ProjSync::Busy;
        }
        let keep_active = self.editors.active_id();
        let opts = nsql_bookmarks::RelocateOpts::default();
        let (mut applied, mut conflicts) = (0usize, 0usize);
        for op in plan.tabs {
            match op {
                projsync::TabOp::Open(t) => {
                    let (mut dirty, mut remap) = (0usize, Vec::new());
                    if self
                        .project_restore_tab(&t, &opts, &mut dirty, &mut remap)
                        .is_some()
                    {
                        applied += 1;
                    }
                }
                projsync::TabOp::Close(key) => {
                    let Some(i) = self
                        .project_tab_of(&key)
                        .and_then(|id| self.editors.index_of_id(id))
                    else {
                        continue;
                    };
                    // 본문을 담지 못한 탭(상한 초과)이 고쳐져 있으면 닫지 않는다 — 계획은 "담긴 본문이 조상과 같다"만 보았다.
                    let captured = ours
                        .tabs
                        .iter()
                        .find(|t| projsync::key_of(t).as_ref() == Some(&key))
                        .is_some_and(|t| t.text.is_some());
                    if captured || !self.editors.is_dirty(i) {
                        self.editors.close_tab_forced(i);
                        applied += 1;
                    }
                }
                projsync::TabOp::Body { key, base, theirs } => {
                    let Some(id) = self.project_tab_of(&key) else {
                        continue;
                    };
                    match self.project_sync_body(id, base, &theirs, mode == "ask") {
                        Some(true) => applied += 1,
                        Some(false) => conflicts += 1,
                        None => {}
                    }
                }
                projsync::TabOp::Revert { key, base } => {
                    let projsync::TabKey::File(p) = &key else {
                        continue;
                    };
                    let Some(id) = self.project_tab_of(&key) else {
                        continue;
                    };
                    let Some(i) = self.editors.index_of_id(id) else {
                        continue;
                    };
                    // 우리 버퍼가 조상 본문 그대로일 때만 디스크로 — 더 고쳤으면 파일 쪽 외부 변경 확인(`ext_check`)이 맡는다.
                    let same = self
                        .editors
                        .tab_box(i)
                        .is_some_and(|tb| tb.buf().eq_str(&base));
                    if mode == "ask" || !same {
                        continue;
                    }
                    if let (Ok(raw), Some(fsig)) = (std::fs::read(p), nexa_fs::watch::file_sig(p)) {
                        let enc = self.editors.snapshot(i).map(|s| s.2.to_string());
                        let (text, _, used) =
                            Self::decode_bytes(&raw, enc.as_deref().unwrap_or("auto"));
                        let (disk_eol, disk) = eol::detect(&text);
                        self.editors.apply_external(
                            i,
                            Some(&disk),
                            &disk,
                            Some(disk_eol),
                            Some(used),
                        );
                        let info = self.ext_files.entry(id).or_default();
                        info.sig = Some(fsig);
                        info.pending = None;
                        info.ignored = None;
                        info.diverged = false;
                        applied += 1;
                    }
                }
            }
        }
        if self.editors.index_of_id(keep_active).is_some()
            && self.editors.active_id() != keep_active
        {
            self.editors.switch_to_id(keep_active);
            self.sync_grid_tab();
        }
        // 저쪽이 먼저 만든 고유 번호는 받는다(우리 것이 있으면 우리 것 — 나중에 쓰는 쪽).
        if self.project.id.is_none() && theirs.id.is_some() {
            self.project.id = theirs.id.clone();
        }
        if plan.folders != self.project.folders {
            self.project.folder_src = plan
                .folders
                .iter()
                .map(|f| ours.folder_src_of(f).or_else(|| theirs.folder_src_of(f)))
                .collect();
            self.project.folders = plan.folders;
            // 적어 둔 폴더 이름(`folders[].name`)은 정의 경로가 열쇠라 합치기만 한다(우리 것 먼저 · 저쪽에만 있는 것 추가).
            for (d, n) in &theirs.folder_names {
                if !self.project.folder_names.iter().any(|(x, _)| x == d) {
                    self.project.folder_names.push((d.clone(), n.clone()));
                }
            }
            applied += 1;
            self.project_changed(false);
        }
        // 경로 변수(T-172 P3)도 합치기만 한다(우리 것 먼저 · 저쪽에만 있는 이름 추가 — 지금은 파일을 직접 고칠 때만 생긴다).
        for (n, v) in &theirs.path_vars {
            if !self.project.path_vars.iter().any(|(x, _)| x == n) {
                self.project.path_vars.push((n.clone(), v.clone()));
            }
        }
        if plan.settings != self.project.settings {
            self.project.settings = plan.settings;
            self.apply_project_settings();
            applied += 1;
        }
        if theirs.bookmarks != base.bookmarks {
            if let Some(tj) = theirs.bookmarks.as_deref() {
                let n =
                    self.bookmarks
                        .merge_external(base.bookmarks.as_deref(), tj, &self.tab_doc_ids);
                if n > 0 {
                    applied += n;
                    self.bm_sync_ui();
                }
            }
        }
        // 기준본 = 방금 합친 디스크 문서.
        self.project_last_json = bytes;
        self.project_sig = sig;
        // ★ 우리가 바꾼 것이 없었으면 합친 결과를 "맞춘 상태"로 삼는다 — 저쪽 문서와 화면 상태만 다르다는 이유로 다시 쓰지 않는다
        //   (두 유휴 인스턴스가 번갈아 덮어쓰는 것을 막는다). 바꾼 것이 있었으면 표식을 그대로 두어 다음 자동 저장이 합친 문서를 쓴다.
        if was_clean {
            self.project_capture_state();
            self.project_ours_mark = doc_mark(&self.project.to_document());
            self.project_mark_saved_gen();
        }
        if applied > 0 || conflicts > 0 {
            let msg = if conflicts > 0 {
                tf(
                    Msg::StProjectSyncConflict,
                    &[&applied.to_string(), &conflicts.to_string()],
                )
            } else {
                tf(Msg::StProjectSynced, &[&applied.to_string()])
            };
            self.log_win.push(LogEntry::new(LogKind::Info, msg.clone()));
            self.sess.status = msg;
            self.project_touch();
            self.ext_banner_sync();
            self.sync_open_files();
            self.layout();
            self.redraw();
        }
        ProjSync::Merged
    }

    /// 저쪽에서 바뀐 탭 본문 하나를 우리 버퍼에 합친다(판정 = 파일의 외부 변경과 같은 `extfile::decide`) —
    /// `Some(true)` = 반영 · `Some(false)` = 겹쳐서 띠로 물음 · `None` = 할 일 없음(이미 같음).
    fn project_sync_body(
        &mut self,
        id: u64,
        base: Option<String>,
        theirs: &str,
        ask: bool,
    ) -> Option<bool> {
        let i = self.editors.index_of_id(id)?;
        let (buf, saved, _, _) = self.editors.snapshot(i)?;
        let saved = saved.to_string();
        let large = self.editors.is_large(i);
        // 조상에 본문이 없던 파일 탭 = 그때는 디스크 그대로였다 → 기준 = 저장 기준 본문(큰 파일은 사본이 없어 버퍼를 기준으로).
        let base = match base {
            Some(b) => b,
            None if self.editors.path_of(i).is_none() => String::new(),
            None if large && !self.editors.is_dirty(i) => buf.clone(),
            None => saved,
        };
        let max = (self.settings.int("file.external_merge_max_kb").max(16) as usize) * 1024;
        let decision = extfile::decide(&extfile::DecideIn {
            base: &base,
            buf: &buf,
            disk: theirs,
            ask_always: ask,
            merge_on: self.settings.flag("file.external_merge") && !large,
            size_ok: theirs.len() <= max && buf.len() <= max,
            format_changed: false,
        });
        match decision {
            extfile::Decision::Ignore | extfile::Decision::AdoptBase => {
                self.project_pending.remove(&id);
                None
            }
            extfile::Decision::Reload => {
                self.project_pending.remove(&id);
                self.editors.apply_synced(i, theirs);
                Some(true)
            }
            extfile::Decision::Merge(m) => {
                self.project_pending.remove(&id);
                let name = self.editors.title_of(i);
                self.ext_backup(&name, &buf);
                self.editors.apply_synced(i, &m.text);
                Some(true)
            }
            extfile::Decision::Ask(n) => {
                self.project_pending.insert(id, (theirs.to_string(), n));
                Some(false)
            }
        }
    }

    /// 프로젝트 파일 저장 — 탐색기의 마지막 선택 위치를 담아서(닫기·전환·폴더 변경·종료 = 전부 이 길).
    pub(crate) fn project_save(&mut self) -> Result<(), String> {
        // ★ 저장 보호(67 §9): 다른 인스턴스가 그 사이 썼으면 먼저 합친다 — 무조건 덮어쓰지 않는다(닫기·전환·종료 길 = 미루지 않음).
        let _ = self.project_sync_external(true);
        self.project_capture_state();
        let r = self.project.save();
        if r.is_ok() {
            self.project_note_disk(self.project.to_document());
            self.project_mark_saved_gen();
            self.project_warn_absolute(false);
        }
        r
    }

    /// ★ T-172 (D-188 · 73 §4-4 ④): 방금 쓴 프로젝트 파일에 절대 경로로 남은 것이 있으면 로그 창에 한 줄(`project.warn_absolute` ·
    /// 저장은 막지 않는다). 명시 저장(`explicit`)은 매번 · 자동 저장·닫기 길은 그 수가 달라졌을 때만(같은 줄이 주기마다 쌓이지 않게).
    fn project_warn_absolute(&mut self, explicit: bool) {
        let n = if self.settings.flag("project.warn_absolute") {
            self.project.absolute_path_count()
        } else {
            0
        };
        let changed = n != self.project_abs_warned;
        self.project_abs_warned = n;
        if n > 0 && (explicit || changed) {
            let msg = tf(Msg::StProjectAbsolutePaths, &[&n.to_string()]);
            self.log_win.push(LogEntry::new(LogKind::Info, msg));
        }
    }

    /// 작업 환경을 프로젝트에 담는다(사용자 09-23): 탐색기 선택 · 탭 순서(파일 = 경로만 · 스크립트 = 본문 ≤ `project.unsaved_max_mb`) ·
    /// 캐럿 + fuzzy 앵커(북마크 `make_anchor` · 10만 줄 넘는 탭은 줄 번호만) · 활성 탭 · 북마크(JSON).
    fn project_capture_state(&mut self) {
        if let Some(p) = self.project_panel.selected_path() {
            self.project.last_selected = Some(p);
        }
        let opts = nsql_bookmarks::RelocateOpts::default();
        // 미저장 본문 상한 = 탭 하나 기준(`project.unsaved_max_mb` · 0 = 무제한 · 사용자 09-30).
        let max_bytes = (self.settings.int("project.unsaved_max_mb").max(0) as usize) << 20;
        let mut over: Vec<(String, usize)> = Vec::new();
        // ★ T-172 (C)(D-496): 자리 탭이 닫힌 못 찾은 탭은 기록에서도 뺀다 · 살아 있는 것은 탭 줄의 그 자리에 되적는다.
        let alive: Vec<u64> = (0..self.editors.tab_count())
            .filter_map(|i| self.missing_id_at(i))
            .collect();
        self.proj_missing.retain(|m| alive.contains(&m.id));
        let mut tabs = Vec::new();
        for i in 0..self.editors.tab_count() {
            if let Some(mid) = self.missing_id_at(i) {
                if let Some(m) = self.proj_missing.iter().find(|m| m.id == mid) {
                    tabs.push(m.clone());
                }
                continue;
            }
            let Some(tb) = self.editors.tab_box(i) else {
                continue;
            };
            let read_only = self.editors.is_read_only(i);
            let tab_id_now = self.editors.tab_id(i);
            // ★ 이진 안내 탭 = 경로가 없으니 **원본 파일 경로**로 담는다(복원 = 다시 열기 → 이진 판정 → 안내 · 읽기 전용 = 같은 기준 ·
            //   사용자 10-13). 그 밖의 경로 없는 읽기 전용 탭("(앞부분)" · 디스크 보기)은 담지 않는다(복원하면 일반 스크래치가 됐다).
            //   뷰 탭(`ext:`·정보·비교 = 경로 없음 + 읽기 전용)은 `view` 열쇠로 복원되므로 이 거름에서 **제외**(124차 리뷰 · 09-30 복원 회귀).
            let path = match self.editors.path_of(i) {
                Some(p) => Some(p),
                None if read_only && self.editors.view_key_of(i).is_none() => {
                    match self.binary_notice_live(tab_id_now) {
                        Some(p) => Some(p),
                        None => continue,
                    }
                }
                None => None,
            };
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
                // ★ 탭의 **문서 id**(67 §9 · 11-19): 탭 id는 인스턴스 안에서만 유일하므로(둘 다 1부터 센다) 같은 프로젝트를 연 두
                //   인스턴스가 탭을 맞추려면 인스턴스와 무관한 값이 필요하다 — 복원한 탭은 파일에 적혀 있던 값을 잇고
                //   (`project_restore_tab`), 새 탭은 처음 담을 때 겹치지 않는 값을 받는다.
                id: *self
                    .tab_doc_ids
                    .entry(tab_id_now)
                    .or_insert_with(new_doc_id),
                indent: self.editors.indent_of(i),
                view: self.editors.view_key_of(i),
                read_only,
                // ★ T-121 (e)(10-30): 이 탭이 묶인 공유 세션의 프로필(접속 중일 때만).
                profile: self.tab_profile_for_capture(self.editors.tab_id(i)),
                ..project::TabState::default()
            };
            // ★ 캡처 비용 = 바뀐 탭만(사용자 09-30 · 72 §3): 본문 세대(epoch · change_seq)·캐럿이 지난 캡처와 같으면 앵커를 다시
            //   만들지 않는다 · 만들 때도 전체 줄 사본 없이 버퍼 줄 조회로(`make_anchor_by`).
            let tab_id = self.editors.tab_id(i);
            let body_key = (buf.epoch(), buf.change_seq());
            let mark = self.capture_cache.entry(tab_id).or_default();
            if buf.line_count() <= 100_000 {
                if mark.anchor_key != Some((body_key.0, body_key.1, caret)) {
                    let n = buf.line_count();
                    let a = nsql_bookmarks::make_anchor_by(
                        n,
                        &|l| buf.line_text(l),
                        line.min(n.saturating_sub(1)),
                        col as u32,
                        &opts,
                    );
                    mark.anchor = (a.text, a.before, a.after);
                    mark.anchor_key = Some((body_key.0, body_key.1, caret));
                }
                t.anchor_text = mark.anchor.0.clone();
                t.before = mark.anchor.1.clone();
                t.after = mark.anchor.2.clone();
            }
            // 본문을 담는 탭 = 이름 없는 스크립트 **+ 미저장 편집이 있는 파일 탭**(사용자 09-23 "탭별로 프로젝트 파일에 · 로드 때 자동 복구 ·
            // 백업은 백업용으로만") — 파일 탭은 그때의 디스크 해시도 같이(로드 때 밖에서 바뀌었는지 알리려고).
            // ★ 상한 = **탭 하나 기준** `project.unsaved_max_mb`(사용자 09-30 "개별 편집 탭 기준의 용량만 문제없다면 정상 동작") · 판정은
            //   복사 **전에** 버퍼 바이트 수로 · 넘는 탭은 조용히 버리지 않고 한 번 알린다(유실 위험 = 파일로 저장 안내).
            let dirty_file = path.is_some() && self.editors.is_dirty(i);
            let wants_body = t.view.is_none() && (path.is_none() || dirty_file);
            let fits = max_bytes == 0 || buf.len_bytes() <= max_bytes;
            if wants_body && fits {
                t.text = Some(tb.text());
                if let Some(p) = &path {
                    t.disk_hash = mark.disk_hash(p);
                }
            }
            if wants_body && !fits && !buf.is_empty() {
                if !mark.over_noticed {
                    mark.over_noticed = true;
                    over.push((self.editors.title_of(i), buf.len_bytes()));
                }
            } else {
                mark.over_noticed = false;
            }
            // ★ T-153 ③(사용자 10-26): 무제 탭의 변수 표는 프로젝트 파일에(파일 탭 = 경로 해시 파일) — 파일/폴더 모드는 프로젝트가 없어 종전대로 버려진다.
            if path.is_none() {
                if let Some(list) = self.tab_vars.get(&tab_id).filter(|l| !l.is_empty()) {
                    t.vars = Some(nsql_script::vars_to_script(list));
                }
            }
            tabs.push(t);
        }
        // 닫힌 탭의 캐시는 버린다.
        let live: std::collections::HashSet<u64> = (0..self.editors.tab_count())
            .map(|i| self.editors.tab_id(i))
            .collect();
        self.capture_cache.retain(|id, _| live.contains(id));
        self.tab_doc_ids.retain(|id, _| live.contains(id));
        self.project_pending.retain(|id, _| live.contains(id));
        // 파일 탭의 미저장 본문 = 스냅숏(docs/70 §5 · 원본은 안 만진다) — ★ T-165 P3 ①(10-31): 쓰기는 여기(동기)가 아니라
        //   `backup_tick`의 저장 큐 + `backup-writer` 스레드가 맡는다(틱마다 바뀐 탭만 큐에 · 만료분만 본문을 떠서 보냄 · clean = 삭제).
        for (title, bytes) in over {
            let msg = tf(
                Msg::StAutosaveTabTooLarge,
                &[
                    &title,
                    &nsql_core::fmt_bytes(bytes as u64),
                    &self.settings.int("project.unsaved_max_mb").to_string(),
                ],
            );
            self.log_win.push(LogEntry::new(LogKind::Info, msg.clone()));
            self.sess.status = msg;
        }
        // ★ T-172 P1: 못 찾았던 탭을 끝에 되돌려 적는다 — 그사이 같은 경로를 연 탭이 있으면 그쪽이 이긴다 · 파일이 돌아왔어도
        //   열지 않았으면 그대로 둔다(다음에 열 때 복원된다).
        for m in &self.proj_missing {
            if !tabs.iter().any(|t: &project::TabState| t.path == m.path) {
                tabs.push(m.clone());
            }
        }
        self.project.tabs = tabs;
        self.project.active = self.editors.active();
        // ★ 67 §9: 이름 없는 탭의 북마크 열쇠도 문서 id로 적는다(탭 id는 인스턴스마다 다르다).
        self.project.bookmarks = Some(self.bookmarks.doc_store(&self.tab_doc_ids).to_json());
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
        } else if self.ai_panel.is_visible() {
            Some("ai".into())
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
        self.proj_missing.clear();
        for t in &tabs {
            let mut id =
                self.project_restore_tab(t, &opts, &mut restored_dirty, &mut scratch_remap);
            // ★ T-172 P1: 원본을 못 찾은 파일 탭 = 상태를 들고 있다가 저장 때 되돌려 적는다 + 로그로 알린다(조용히 잃지 않는다).
            if let (None, Some(p)) = (id, &t.path) {
                if !p.is_file() {
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        tf(Msg::StProjectTabMissing, &[&p.to_string_lossy()]),
                    ));
                    // ★ T-172 (C): 탭 줄의 그 자리에 자리 탭(빈 읽기 전용 뷰 탭)을 둔다 — 띠가 사정과 [찾아보기…] [탭 닫기]를 낸다.
                    let mut m = t.clone();
                    if m.id == 0 || self.proj_missing.iter().any(|x| x.id == m.id) {
                        m.id = new_doc_id();
                    }
                    let title = if m.title.is_empty() {
                        editors::file_title(p)
                    } else {
                        m.title.clone()
                    };
                    self.editors
                        .open_view_tab(&format!("missing:{}", m.id), &title);
                    // 보관된 미저장 본문은 읽기 전용으로 보여 준다(D-189).
                    if let Some(txt) = &m.text {
                        let at = self.editors.active();
                        self.editors.set_view_text(at, txt);
                    }
                    id = Some(self.editors.active_id());
                    self.proj_missing.push(m);
                }
            }
            ids.push(id);
        }
        // ★ T-172 P3(73 §4-3 "모아서 한 번"): 못 찾은 탭 가운데 **경로 변수 아래**인 것을 변수 이름별로 모아 로그 한 줄 —
        //   자리 탭 하나에서 [찾아보기…]로 파일을 고르면 그 변수의 이 PC 자리가 정해져 나머지도 함께 열린다는 것까지 알린다(창 없음).
        let mut need: Vec<(String, usize)> = Vec::new();
        for m in &self.proj_missing {
            if let Some((name, _)) = m.path.as_ref().and_then(|p| self.project.var_of_path(p)) {
                match need.iter_mut().find(|(n, _)| *n == name) {
                    Some(hit) => hit.1 += 1,
                    None => need.push((name, 1)),
                }
            }
        }
        if !need.is_empty() {
            let names = need
                .iter()
                .map(|(n, c)| format!("{n} ({c})"))
                .collect::<Vec<_>>()
                .join(", ");
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                tf(Msg::StProjectVarsMissing, &[&names]),
            ));
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
        self.project_note_disk(self.project.to_document());
        self.project_mark_saved_gen();
        self.sync_open_files();
        self.layout();
        self.redraw();
    }

    /// 저장된 탭 하나를 연다(`project_restore`의 한 바퀴 · 다중 인스턴스 동기화의 "저쪽이 새로 연 탭"도 이 길 · 67 §9) — 연 탭의 id.
    ///   `restored_dirty` = 미저장 본문을 올린 파일 탭 수 · `scratch_remap` = 무제 탭의 (문서 id → 새 탭 id · 북마크 재매핑용).
    fn project_restore_tab(
        &mut self,
        t: &project::TabState,
        opts: &nsql_bookmarks::RelocateOpts,
        restored_dirty: &mut usize,
        scratch_remap: &mut Vec<(u64, u64)>,
    ) -> Option<u64> {
        let id = match &t.path {
            Some(p) => {
                if !p.is_file() {
                    None
                } else {
                    if t.read_only {
                        // 읽기 전용으로 열렸던 탭 = 같은 기준으로(사용자 10-13 · 이진이면 안내 탭 · 글이면 읽기 전용 열기 ·
                        //   미리보기였으면 미리보기 안내로).
                        if t.preview && fileload::sniff_binary(p) {
                            let name = editors::file_title(p);
                            self.open_binary_preview(p, &name);
                        } else {
                            self.load_file(p, "auto", LoadMode::ReadOnly);
                        }
                    } else if t.preview {
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
                            *restored_dirty += 1;
                            if t.disk_hash != 0 && backups::disk_hash(p) != t.disk_hash {
                                self.log_win.push(LogEntry::new(
                                    LogKind::Info,
                                    tf(Msg::StBackupExternal, &[&p.to_string_lossy()]),
                                ));
                                // ★ T-165 P3 D-180(10-30 · 70 §9): 복원한 미저장 본문 ≠ 지금 디스크 → 외부 변경과 같은 길로
                                //   **탭 띠**(비교 / 다시 읽기 / 유지 · `extfile` 58 정책) — 종전 = 로그만.
                                if let (Ok(bytes), Some(sig)) =
                                    (std::fs::read(p), nexa_fs::watch::file_sig(p))
                                {
                                    let id = self.editors.tab_id(i);
                                    self.ext_on_changed(id, p, sig, &bytes);
                                }
                            }
                        }
                        self.restore_caret(i, t, opts);
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
            // ★ 뷰 탭(확장 상세) = 열쇠로 같은 뷰를 다시 연다 · 확장이 없어졌으면 탭도 없음(사용자 09-30).
            None if t.view.is_some() => {
                let key = t.view.clone().unwrap_or_default();
                match key.strip_prefix("ext:") {
                    Some(id) if self.ext_reopen_view(id) => Some(self.editors.active_id()),
                    _ => None,
                }
            }
            None => {
                self.editors.new_tab(Some(t.title.clone()));
                let i = self.editors.active();
                if let Some(tb) = self.editors.tab_box_mut(i) {
                    if let Some(txt) = &t.text {
                        tb.set_text(txt);
                    }
                    // ★ 미리보기였던 경로 없는 탭(객체 찾기 미리보기 · 10-16 사용자 1안) = 미리보기로 복원 · 저장 지점을 찍어
                    //   곧장 승격되지 않게(편집해야 정식). 정식 탭으로 보이던 종전 결함.
                    if t.preview {
                        tb.mark_saved();
                    }
                }
                self.restore_caret(i, t, opts);
                if t.indent.is_some() {
                    self.editors.set_indent_of(i, t.indent);
                }
                let new_id = self.editors.tab_id(i);
                if t.preview {
                    self.editors.set_preview_id(Some(new_id));
                }
                // ★ T-121 (e)(10-30): 저장 당시 프로필의 공유 세션에 다시 묶기(없으면 접속될 때까지 보관).
                if let Some(p) = &t.profile {
                    self.restore_tab_bind(new_id, p);
                }
                // 무제 탭의 변수 표 복원(T-153 ③ · 새 탭 id로).
                if t.path.is_none() {
                    if let Some(v) = &t.vars {
                        let states = nsql_script::vars_from_script(v);
                        if !states.is_empty() {
                            self.tab_vars.insert(new_id, states);
                        }
                    }
                }
                // 이름 없는 탭의 북마크(`Scratch { tab }`)는 옛 id → 새 id로 재매핑(사용자 09-23 검토 · 본문이 그대로라 줄이 맞는다).
                if t.id != 0 {
                    scratch_remap.push((t.id, new_id));
                }
                Some(new_id)
            }
        };
        // ★ 문서 id 등록(67 §9): 이 탭을 다음에 담을 때 같은 id로 적는다 — 다른 인스턴스와 탭을 맞추는 열쇠.
        if let (Some(id), true) = (id, t.id != 0) {
            self.tab_doc_ids.insert(id, t.id);
        }
        id
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
            Some("ai") => Some(("view.ai", self.ai_panel.is_visible())),
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
        let (periodic_wait, quick) = self.project_autosave_waits();
        let periodic = self.project_autosave_at + periodic_wait;
        let changed = self.project_touch_at.map(|t| t + quick);
        let mut next = changed.map_or(periodic, |t| t.min(periodic));
        // ★ T-165 P3 ①: 스냅숏 큐의 다음 만료도 깨움 시각에(잠자기 친화 · 70 §3).
        if let Some(q) = self.backup_queue.next_due() {
            next = next.min(q);
        }
        Some(next.max(now))
    }

    /// ★ T-165 P3 ①(70 §3~4 · 10-31): 파일 탭 미저장 스냅숏 = **저장 큐 + 쓰기 스레드**. 틱마다 ① dirty 파일 탭(상한 안)의 본문 세대가
    ///   지난 push 뒤 바뀌었으면 `push`(같은 탭 = 대기 재설정 · `project.autosave_change_ms` 뒤 · 계속 쳐도 `project.autosave_secs`마다
    ///   한 번은) ② clean이 됐거나 닫힌 탭 = 큐 취소(+ 스냅숏 삭제) ③ 만료된 것만 최대 `project.backup_batch`개 본문을 떠서
    ///   `backup-writer`에 보낸다(UI 스레드 디스크 쓰기 0 · 나머지는 다음 틱) ④ 스레드의 실패는 로그 한 줄(경로당 1회).
    fn backup_tick(&mut self, now: Instant) {
        let (max_wait, delay) = self.project_autosave_waits();
        let max_bytes = (self.settings.int("project.unsaved_max_mb").max(0) as usize) << 20;
        let mut remove: Vec<PathBuf> = Vec::new();
        let mut live: std::collections::HashSet<u64> = std::collections::HashSet::new();
        for i in 0..self.editors.tab_count() {
            let tab = self.editors.tab_id(i);
            live.insert(tab);
            let Some(p) = self.editors.path_of(i) else {
                continue;
            };
            let dirty = self.editors.is_dirty(i);
            let Some((fits, rev)) = self.editors.tab_box(i).map(|tb| {
                (
                    max_bytes == 0 || tb.buf().len_bytes() <= max_bytes,
                    tb.rev(),
                )
            }) else {
                continue;
            };
            if dirty && fits {
                if self.backup_rev.get(&tab) != Some(&rev) {
                    self.backup_rev.insert(tab, rev);
                    self.backup_queue.push(tab, p, now, delay, max_wait);
                }
            } else if self.backup_rev.remove(&tab).is_some() {
                self.backup_queue.cancel(&tab);
                if let Some(m) = self.capture_cache.get_mut(&tab) {
                    m.backup_key = None;
                }
                remove.push(p);
            }
        }
        // 닫힌 탭 = 큐에서 뺀다(스냅숏 삭제는 닫기 경로 `backup_forget` + `backups::remove`가 이미).
        let gone: Vec<u64> = self
            .backup_rev
            .keys()
            .copied()
            .filter(|t| !live.contains(t))
            .collect();
        for t in gone {
            self.backup_rev.remove(&t);
            self.backup_queue.cancel(&t);
        }
        let batch = self.settings.int("project.backup_batch").clamp(1, 64) as usize;
        let due = self.backup_queue.take_due_limit(now, batch);
        let mut writes: Vec<(PathBuf, String)> = Vec::new();
        for (tab, p) in due {
            let Some(i) = self
                .editors
                .index_of_id(tab)
                .filter(|&i| self.editors.is_dirty(i))
            else {
                continue;
            };
            let Some((key, text)) = self
                .editors
                .tab_box(i)
                .map(|tb| ((tb.buf().epoch(), tb.buf().change_seq()), tb.text()))
            else {
                continue;
            };
            self.capture_cache.entry(tab).or_default().backup_key = Some(key);
            writes.push((p, text));
        }
        if !remove.is_empty() || !writes.is_empty() {
            let w = self
                .backup_writer
                .get_or_insert_with(backups::Writer::spawn);
            for p in remove {
                w.remove(&p);
            }
            for (p, text) in writes {
                w.write(&p, text);
            }
        }
        let errs = self
            .backup_writer
            .as_ref()
            .map(backups::Writer::take_errors)
            .unwrap_or_default();
        for e in errs {
            self.log_win.push(LogEntry::new(LogKind::Error, e));
        }
    }

    /// ★ T-165 P3 ④(70 §4 · 10-31): 미저장 본문이 `project.unsaved_max_mb`를 넘는 탭(프로젝트 보존·스냅숏·hot exit 전부 담지 않는다) —
    ///   집합이 바뀌면 편집기 위 띠(`ext_banner_sync` · "자동 백업되지 않습니다 — 저장하세요" [저장][닫기])를 다시 맞춘다. 0 = 무제한.
    ///   프로젝트 유무와 무관(파일 모드 hot exit도 같은 상한).
    pub(crate) fn backup_over_tick(&mut self) {
        let max_bytes = (self.settings.int("project.unsaved_max_mb").max(0) as usize) << 20;
        let mut now: std::collections::HashSet<u64> = std::collections::HashSet::new();
        if max_bytes > 0 {
            for i in 0..self.editors.tab_count() {
                if !self.editors.is_dirty(i) {
                    continue;
                }
                if self
                    .editors
                    .tab_box(i)
                    .is_some_and(|tb| tb.buf().len_bytes() > max_bytes)
                {
                    now.insert(self.editors.tab_id(i));
                }
            }
        }
        if now != self.backup_over {
            // 상한 안으로 돌아온 탭은 닫기 기억도 푼다(다시 넘으면 다시 띄움).
            self.backup_over_dismissed.retain(|t| now.contains(t));
            self.backup_over = now;
            self.ext_banner_sync();
        }
    }

    /// 탭을 버리거나 저장했다 — 스냅숏 큐에서 뺀다(파일 삭제는 호출자가 `backups::remove`).
    pub(crate) fn backup_forget(&mut self, tab: u64) {
        self.backup_queue.cancel(&tab);
        self.backup_rev.remove(&tab);
    }

    /// (주기 점검 `project.autosave_secs` ≥ 5초, 변경 뒤 저장 `project.autosave_change_ms` ≥ 100 ms · 주기보다 크지 않게 · 09-30 ms 단위).
    fn project_autosave_waits(&self) -> (Duration, Duration) {
        let periodic =
            Duration::from_secs(self.settings.int("project.autosave_secs").max(5) as u64);
        let quick =
            Duration::from_millis(self.settings.int("project.autosave_change_ms").max(100) as u64)
                .min(periodic);
        (periodic, quick)
    }

    /// ★ 종료 직전 마지막 저장(사용자 09-23 "프로그램 종료 시 꼭 저장"): 프로젝트(자동 저장이 켜져 있을 때 · 꺼져 있으면
    /// `request_exit`의 물음이 이미 처리) + **북마크 워크스페이스**(디바운스를 기다리지 않고 지금). 창 닫기·File ▸ Exit 두 길 모두.
    pub(crate) fn flush_on_exit(&mut self) {
        if self.project.is_open() && self.project_autosave_on() {
            let _ = self.project_save();
        }
        self.bookmarks.save_now();
        // ★ T-165 P3 ①: 큐에 남은 스냅숏은 지금 동기로 쓰고(디바운스를 기다리지 않음) · 쓰기 스레드는 보낸 것을 끝까지 쓴 뒤 합류.
        for (tab, p) in self.backup_queue.flush() {
            if let Some(i) = self
                .editors
                .index_of_id(tab)
                .filter(|&i| self.editors.is_dirty(i))
            {
                if let Some(tb) = self.editors.tab_box(i) {
                    let _ = backups::write(&p, &tb.text());
                }
            }
        }
        if let Some(w) = self.backup_writer.take() {
            w.shutdown();
        }
    }

    /// ★ T-165 P3 ② hot exit(D-181 · 70 §9): 파일 모드 종료 문서의 자리 = 설정 폴더(`workspaces/hot-exit.nsql-project` · D-183 프로젝트 폴더에
    ///   쓰지 않음 · D-407 사용자 프로젝트 파일과 무관).
    fn hot_exit_path() -> Option<PathBuf> {
        nsql_settings::config_dir().map(|d| d.join("workspaces").join("hot-exit.nsql-project"))
    }

    /// hot exit이 켜져 있고(기본 `always`) 프로젝트가 없으며, 미저장 탭을 **전부** 담을 수 있는가(`project.unsaved_max_mb` 넘는 탭이
    /// 있으면 거짓 = 그 탭은 묻는다 · 70 §9 ④).
    pub(crate) fn hot_exit_applies(&self) -> bool {
        if self.project.is_open() || self.settings.get("project.hot_exit") == Some("off") {
            return false;
        }
        self.hot_exit_tabs_fit()
    }

    /// ★ 프로젝트 모드의 hot exit(70 §6 ⑥ · D-181 · 10-31 28차 협업 관찰): 프로젝트가 열려 있고 자동 저장이 켜져 있으면 미저장 본문은
    ///   `request_exit`의 `project_save`가 프로젝트 문서(blob)에 담으므로 **파일 탭도 묻지 않는다** — `hot_exit=off`이거나 상한을 넘는 탭이
    ///   있으면 종전처럼 묻는다(종전 = 프로젝트 모드의 파일 탭은 늘 물었다 · 09-23 규칙이 D-181 이전).
    pub(crate) fn hot_exit_project(&self) -> bool {
        self.project.is_open()
            && self.project_autosave_on()
            && self.settings.get("project.hot_exit") != Some("off")
            && self.hot_exit_tabs_fit()
    }

    /// 미저장 탭이 전부 `project.unsaved_max_mb` 안인가(0 = 무제한).
    fn hot_exit_tabs_fit(&self) -> bool {
        let max_bytes = (self.settings.int("project.unsaved_max_mb").max(0) as usize) << 20;
        (0..self.editors.tab_count()).all(|i| {
            !self.editors.is_dirty(i)
                || max_bytes == 0
                || self
                    .editors
                    .tab_box(i)
                    .is_some_and(|tb| tb.buf().len_bytes() <= max_bytes)
        })
    }

    /// 종료 직전: 파일 모드의 작업 환경(탭 · 미저장 본문 · 캐럿)을 hot exit 문서로 쓴다 — 미저장이 하나도 없으면 문서를 지운다(다음 기동에
    ///   복원할 것이 없다). 프로젝트 모드에는 해당 없음.
    pub(crate) fn hot_exit_save(&mut self) {
        let Some(path) = Self::hot_exit_path() else {
            return;
        };
        if self.project.is_open() {
            return;
        }
        let dirty = (0..self.editors.tab_count())
            .filter(|&i| self.editors.is_dirty(i))
            .count();
        if dirty == 0 {
            let _ = std::fs::remove_file(&path);
            return;
        }
        // 임시로 경로를 꽂아 캡처(상대 경로 기준 = 설정 폴더) → 저장 → 파일 모드로 되돌림.
        let saved = self.project.clone();
        self.project.path = Some(path.clone());
        self.project_capture_state();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let r = self.project.save();
        self.project = saved;
        match r {
            Ok(()) => {
                let msg = tf(Msg::StHotExitSaved, &[&dirty.to_string()]);
                self.log_win.push(LogEntry::new(LogKind::Info, msg));
            }
            Err(e) => self
                .log_win
                .push(LogEntry::new(LogKind::Info, tf(Msg::ErrProjectFile, &[&e]))),
        }
    }

    /// 기동(파일 모드 · 첫 인스턴스 · 파일 인자 없음): hot exit 문서가 있으면 탭을 복원하고 문서를 지운다(한 번만).
    pub(crate) fn hot_exit_restore(&mut self) {
        if self.project.is_open()
            || !self.arg_files.is_empty()
            || !self.first_instance
            || self.settings.get("project.hot_exit") == Some("off")
        {
            return;
        }
        let Some(path) = Self::hot_exit_path().filter(|p| p.is_file()) else {
            return;
        };
        let Ok(doc) = project::Project::load(&path) else {
            let _ = std::fs::remove_file(&path);
            return;
        };
        let n = doc.tabs.len();
        let unsaved = doc.tabs.iter().filter(|t| t.text.is_some()).count();
        // 탭만 빌려 쓴다(폴더·설정 절은 없음) — `project_restore`는 `self.project.tabs`를 읽으므로 잠시 꽂았다가 원래(빈) 프로젝트로.
        let saved = self.project.clone();
        self.project.tabs = doc.tabs;
        self.project.active = doc.active;
        self.project_restore();
        self.project = saved;
        self.project_last_json.clear();
        let _ = std::fs::remove_file(&path);
        let msg = tf(
            Msg::StHotExitRestored,
            &[&n.to_string(), &unsaved.to_string()],
        );
        self.log_win.push(LogEntry::new(LogKind::Info, msg.clone()));
        self.sess.status = msg;
    }

    /// ★ 자동 저장 틱(사용자 09-28 정의 · [70 §7](../../../docs/70-autosave-and-restore.md)):
    ///   ① **감시된 변경**(변경 세대 = 탭 집합·순서·본문 세대·활성 탭 · `project_touch` = 폴더·탐색기 선택)은 **최초 변경 시점 +
    ///      `project.autosave_change_ms`(10초)** 에 1회 저장 — 그 안의 추가 변경은 타이머를 옮기지 않는다(최초 트리거 기준).
    ///   ② **감시되지 않는 변경**(캐럿 · 북마크 · 탐색기 펼침 · 패널 표시 · 검색어 · 프로필 표식)은 **마지막 저장·점검 +
    ///      `project.autosave_secs`(30초)** 주기 점검으로 — 문서를 만들어 이전 저장본과 같으면 쓰지 않는다.
    ///   ③ 어느 길이든 저장·점검하면 두 타이머를 모두 초기화한다(저장 횟수 최소화) · 변경이 없으면 쓰지 않는다.
    pub(crate) fn project_autosave_tick(&mut self) {
        // ★ T-165 P3 ④: 상한 초과 탭 띠는 프로젝트 유무와 무관하게.
        self.backup_over_tick();
        if !self.project.is_open() || !self.project_autosave_on() {
            return;
        }
        // ★ T-165 P3 ①: 파일 탭 스냅숏(큐 → 스레드)은 프로젝트 문서 저장과 따로, 매 틱.
        self.backup_tick(Instant::now());
        let (periodic_wait, quick) = self.project_autosave_waits();
        // 최초 변경 시각(감시된 변경) — 이미 잡혀 있으면 옮기지 않는다.
        if self.project_touch_at.is_none() && self.project_autosave_dirty() {
            self.project_touch_at = Some(Instant::now());
        }
        let changed_due = self.project_touch_at.is_some_and(|t| t.elapsed() >= quick);
        let periodic_due = self.project_autosave_at.elapsed() >= periodic_wait;
        if !changed_due && !periodic_due {
            return;
        }
        self.project_touch_at = None;
        self.project_autosave_at = Instant::now();
        // ★ 저장 보호(67 §9): 다른 인스턴스가 쓴 것을 먼저 합친다 · 실행 중인 탭 때문에 지금 못 합치면 이번 저장은 건너뛴다
        //   (덮어쓰면 저쪽 변경이 사라진다 · 다음 주기에 다시).
        if self.project_sync_external(false) == ProjSync::Busy {
            return;
        }
        self.project_capture_state();
        let js = self.project.to_document();
        // 판정 = 우리가 마지막으로 쓰거나 맞춘 문서와 같은가(디스크 문서가 아니라 · 67 §9).
        if doc_mark(&js) == self.project_ours_mark {
            // 🔧 쓸 것이 없어도 "저장됨"이다 — 세대만 다르고 문서가 같으면 `*`가 영원히 남던 결함(사용자 09-28).
            self.project_mark_saved_gen();
        } else if self.project.save().is_ok() {
            self.project_note_disk(js);
            self.project_mark_saved_gen();
            self.project_warn_absolute(false);
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
            // ★ 목록이 바뀌면 **바로** 다시 그린다(사용자 10-12 "Release · 프로젝트 없음 · 미저장 탭 3개 · 탭 전환 때 열린 파일 목록이
            //   0.5 s 늦다"): 프로젝트가 열려 있으면 `project_sync_active`가 뒤에서 `redraw()`를 불러 가려졌지만, 프로젝트가 없으면
            //   그 함수가 이른 반환이라 다음 그리기 = 캐럿 깜빡임 타이머(500 ms)였다. Debug가 괜찮았던 것은 실제 설정의
            //   `project.restore_last=on`으로 프로젝트가 열려 있었기 때문(빌드 차이 아님).
            self.redraw();
        }
    }

    /// 작업 환경이 바뀌었다(탭 · 폴더) → 2초 뒤 저장(자동 저장 켬 · 프로젝트 열림).
    pub(crate) fn project_touch(&mut self) {
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
        // 아웃라인 표시 설정(T-422) — 기동 때 한 벌(변경은 `apply_setting_editor`).
        self.outline_panel
            .set_cfg(crate::outline_panel::outline_cfg(&self.settings));
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
        let pc = self
            .settings
            .get("project.pc_paths")
            .unwrap_or("")
            .to_string();
        match project::Project::load_pc(&path, &pc) {
            Ok(proj) => {
                if from_arg {
                    // 복원은 기동 마지막(`project_restore` 호출부)에서 한 번 — 여기서 하면 두 번 열린다.
                    self.project_set(proj, false);
                } else {
                    self.project = proj;
                    self.apply_project_settings();
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
            } else if let Some(rest) = id.strip_prefix("ext.run:") {
                // ★ 확장 메뉴(프로젝트 탐색기 행 · 146차): 행의 경로를 상황(doc_path)으로 실행.
                if let Some((cmd, path)) = rest.split_once(':') {
                    let (cmd, path) = (cmd.to_string(), path.to_string());
                    self.run_extension_cmd_for(&cmd, &path);
                }
            } else {
                // 열린 파일 행 클릭(`editor.switch:`) = 패널에서 비롯된 전환 → 동기는 조용히(선택만 · 사용자 10-07).
                if id.starts_with("editor.switch:") {
                    self.project_sync_quiet = true;
                }
                self.project_cmd(&id);
            }
            self.redraw();
        }
        if let Some(req) = self.project_panel.take_open() {
            // 트리에서 열기 = 패널에서 비롯된 전환 → 동기는 조용히(클릭한 행에 포커스 유지 · 사용자 10-07).
            self.project_sync_quiet = true;
            self.project_open_req(req);
        }
    }

    /// 탐색기 클릭 = 미리보기 탭(설정 `project.preview_tab`) · 더블클릭/Enter = 정식 탭.
    /// 큰 파일(`file.async_load_mb` 이상)은 미리보기 없이 보통 열기(자리 탭 + 스레드 · 큰 파일 확인).
    pub(crate) fn project_open_req(&mut self, req: project_panel::OpenReq) {
        let preview = self.settings.flag("project.preview_tab") && !req.permanent;
        if !preview {
            // ★ 이진 안내 탭은 경로가 없어 `promote_path`가 못 찾는다 → 안내 표로 찾아 **승격/전환**(협업 V1 10-13 "더블클릭 = 두 탭").
            if let Some(i) = self.binary_notice_tab(&req.path) {
                let _ = self.editors.promote_tab(i);
                self.editors.switch(i);
                self.set_focus(Focus::Editor);
                self.layout();
                self.redraw();
                return;
            }
            // 한 번 클릭인데 정식 탭 = 이유와 경로를 로그에(사용자 10-13 "직접 열리는 이유와 파일 경로를 출력").
            if !req.permanent {
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    tf(
                        Msg::LogPreviewBypass,
                        &[&req.path.display().to_string(), t(Msg::LogPreviewOff)],
                    ),
                ));
            }
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
        // 같은 파일의 이진 안내 탭이 이미 있으면(미리보기든 정식이든) 그 탭으로(중복 탭 없음 · 10-13).
        if let Some(i) = self.binary_notice_tab(&req.path) {
            self.editors.switch(i);
            self.layout();
            self.redraw();
            return;
        }
        let size = std::fs::metadata(&req.path).map(|m| m.len()).unwrap_or(0);
        let async_mb = self.settings.int("file.async_load_mb").max(1);
        let async_at = (async_mb as u64) << 20;
        if size >= async_at {
            // 큰 글 파일 = 스레드 적재 길(정식 탭) — 이유·경로 로그(10-13).
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                tf(
                    Msg::LogPreviewBypass,
                    &[
                        &req.path.display().to_string(),
                        &tf(
                            Msg::LogPreviewTooBig,
                            &[&nsql_core::fmt_bytes(size), &async_mb.to_string()],
                        ),
                    ],
                ),
            ));
            self.open_file(&req.path);
            return;
        }
        // 크기 게이트 아래(= 미리보기 대상)에서만 이진 판정(앞 8 KiB) → 미리보기 안내. ≥ `file.async_load_mb`는 이진이든 글이든
        // **정식 탭**(위 게이트 · 이진 = 정식 안내 탭 + 우회 로그 · 사용자 10-13 "8 MB 기준이 이미지에도 동작해야").
        if fileload::sniff_binary(&req.path) {
            let name = editors::file_title(&req.path);
            self.open_binary_preview(&req.path, &name);
            self.layout();
            self.redraw();
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
            Err(e) if e == fileload::BINARY => {
                // 이진 파일 = 안내를 **미리보기 탭**에(한 번 클릭 · 더블클릭은 위 `open_file` 길 = 정식 탭 · 10-13).
                let name = editors::file_title(&req.path);
                self.open_binary_preview(&req.path, &name);
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

    /// ★ 프로젝트 탐색기 필터 = Ctrl+P와 **같은 파일 색인**(사용자 10-07 "유니버설 데이터 · 속도"): 필터 글이 있으면 색인 열거를 깨우고
    /// (60초 캐시 · 제외 폴더 · 이중 버퍼 = `goto_refresh_files`) 글 세대·파일 수·읽는 중이 바뀔 때마다 패널에 민다 — 패널은 일치 파일의
    /// 조상만 노드로 만든다(워커 열거 0). 틱마다 호출 · 비용 = 열쇠 비교 하나.
    pub(crate) fn project_index_pump(&mut self) {
        let Some(rev) = self.project_panel.index_wanted() else {
            self.project_index_key = (0, 0, false, None);
            return;
        };
        self.goto_refresh_files();
        let scanning = self.goto_walk.is_some();
        let len = self.goto_files.len();
        let at = self.goto_files_at;
        let requested = self.project_panel.take_index_request();
        let (prev_rev, prev_len, prev_scan, prev_at) = self.project_index_key;
        // 전체 다시 = 필터 글이 바뀜 · 목록이 줄었다 · **열거 세대가 바뀜**(재열거 완료로 목록 교체 · 길이가 같아도 · 사용자 10-07) · 패널이 요청
        //   (트리 재구성 뒤) · 처음. 그 밖(목록이 자람 · 읽는 중 끝) = **증분**(새 파일만).
        let full = requested || rev != prev_rev || len < prev_len || at != prev_at;
        if !full && len == prev_len && scanning == prev_scan {
            return;
        }
        // 열거가 자라는 동안의 증분 밀기는 250 ms마다(틱마다 다시 거르고 행을 짜던 CPU 1코어 · 협업 bin45) — 끝·글 변경은 즉시.
        if !full && scanning && self.project_index_last.elapsed() < Duration::from_millis(250) {
            return;
        }
        let from = if full { 0 } else { prev_len };
        self.project_index_key = (rev, len, scanning, at);
        self.project_index_last = Instant::now();
        let excluded =
            !super::goto::exclude_names(self.settings.get(super::goto::EXCLUDE_KEY).unwrap_or(""))
                .is_empty();
        let files = std::mem::take(&mut self.goto_files);
        let lower = std::mem::take(&mut self.goto_lower);
        self.project_panel
            .apply_index(&files, &lower, from, scanning, excluded);
        self.goto_files = files;
        self.goto_lower = lower;
        self.redraw();
    }

    /// 활성 탭이 바뀌면(사용자 10-07 규칙) ① 트리에 있는 파일이면 트리 행에 **선택 표시만**(스크롤·포커스 이동 없음 · `project.auto_reveal`이면
    /// 조상도 펼침 = `reveal_quiet` · 아니면 `mark_path`) ② **열린 파일(OPEN FILES)** 행도 선택(활성) + 그 행이 보이게 스크롤 = 기본 포커스.
    /// 트리로 스크롤하는 것은 사용자가 "프로젝트 탐색기에서 보기"를 눌렀을 때(`reveal_in_project`)뿐.
    pub(crate) fn project_sync_active(&mut self) {
        // 🔧 1회성 깃발은 **활성 탭이 안 바뀌었어도** 매 틱 소비한다(사용자 10-07 ① "Ctrl+P로 연 파일이 열린 파일 행으로 안 감" =
        //   패널에서 같은 탭을 다시 고른 뒤 깃발이 남아 다음 전환(Ctrl+P 열기)에 잘못 적용되던 결함).
        // 매트릭스(사용자 10-07): 편집기 탭 클릭 등 = 열린 파일 행으로 스크롤(기본 포커스) · 패널(열린 파일·트리)에서 비롯된 전환 = 선택만.
        let mut quiet = std::mem::take(&mut self.project_sync_quiet);
        // 탭 닫힘 뒤 자동 선택된 탭(사용자 10-07 ②): 미리보기/뷰 탭 닫힘 = `tabs.close_preview_focus`(기본 keep = 선택만 · 프로젝트 폴더
        //   탐색을 이어 가려는 뜻) · 편집기 탭 닫힘 = `tabs.close_editor_focus`(기본 move = 탭을 클릭한 것처럼).
        if let Some(kind) = self.editors.take_closed_kind() {
            quiet |= close_keeps_focus(
                kind,
                self.settings
                    .get("tabs.close_preview_focus")
                    .unwrap_or("keep"),
                self.settings
                    .get("tabs.close_editor_focus")
                    .unwrap_or("move"),
            );
        }
        let id = self.editors.active_id();
        if id == self.last_synced_tab {
            return;
        }
        self.last_synced_tab = id;
        if !self.project.is_open() {
            return;
        }
        // 열린 파일 목록을 먼저 지금 탭에 맞춘다(활성 행이 바로 있어야 그쪽으로 스크롤할 수 있다).
        self.sync_open_files();
        match self.editors.active_path() {
            Some(p) if self.editors.in_project(&p) => {
                if self.settings.flag("project.auto_reveal") {
                    self.project_panel.reveal_quiet(&p);
                } else {
                    self.project_panel.mark_path(&p);
                }
            }
            // 트리 밖 파일·스크립트 탭 = 트리 선택 없음(닫힌 파일 표시가 남지 않게 · 협업 bin42 관찰).
            _ => self.project_panel.clear_mark(),
        }
        if !quiet {
            self.project_panel.focus_open_active();
        }
        self.redraw();
    }

    /// ★ 상태줄 자동 저장 메뉴의 답(사용자 09-28): `project` = 프로젝트 파일을 선택한 채 그 폴더를 OS 탐색기로 ·
    ///   `backups` = 일반 파일 자동 저장 위치(`backups/files`)를 이 탭의 최신 스냅숏을 선택한 채(없으면 폴더만) · `settings` = 설정 창.
    pub(crate) fn autosave_pick(&mut self, what: &str) {
        match what {
            // 풀다운 Project ▸ 저장과 동일(사용자 09-30 · 상태줄 메뉴 하단).
            "save_project" => self.menu_action("project.save"),
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
            "objbackup" => self.objbk_open_dir(),
            "objbackup.settings" => {
                self.prefs_query = Some("backup.object".into());
                self.open_prefs = true;
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
        doc_mark(&self.project.to_document()) != self.project_ours_mark
    }

    /// 프로젝트 자동 저장·복원은 Pro(25 §13-3) — 설정이 켜져 있어도 게이트가 켜진 무료면 꺼진 것으로(기본은 게이트 끔 · D-145).
    pub(crate) fn project_autosave_on(&self) -> bool {
        self.settings.flag("project.autosave")
            && self.entitled(nsql_license::Feature::ProjectRestore)
    }
}

/// 탭 닫힘 뒤 자동 선택된 탭의 탐색기 동기를 **조용히(선택만)** 할 것인가 — 닫힌 탭 종류별 설정값(`keep` = 유지 · `move` = 이동).
pub(crate) fn close_keeps_focus(
    kind: crate::editors::ClosedKind,
    preview_focus: &str,
    editor_focus: &str,
) -> bool {
    match kind {
        crate::editors::ClosedKind::PreviewOrView => preview_focus == "keep",
        crate::editors::ClosedKind::Editor => editor_focus == "keep",
    }
}

#[cfg(test)]
mod close_focus_tests {
    use super::close_keeps_focus;
    use crate::editors::ClosedKind;

    /// 기본값 = 미리보기/뷰 유지(keep) · 편집기 이동(move) · 각 키는 자기 종류에만 영향(MC/DC 쌍).
    #[test]
    fn close_keeps_focus_matrix() {
        assert!(close_keeps_focus(ClosedKind::PreviewOrView, "keep", "move"));
        assert!(!close_keeps_focus(ClosedKind::Editor, "keep", "move"));
        assert!(!close_keeps_focus(
            ClosedKind::PreviewOrView,
            "move",
            "move"
        ));
        assert!(close_keeps_focus(ClosedKind::Editor, "keep", "keep"));
        assert!(!close_keeps_focus(ClosedKind::Editor, "move", "move"));
    }
}
