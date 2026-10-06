//! App — 파일 열기/저장·인코딩·탭 닫기·종료 흐름(T-74 · docs/59).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 저장하지 않은 탭을 닫으려 한다(X · Ctrl+W · 탭 메뉴 · 모두 닫기) — 설정 `editor.close_unsaved`: `ask` = 탭 옆 메뉴로 묻는다 ·
    /// `twice` = 종전의 2단 닫기.
    pub(crate) fn ask_save_close(&mut self, i: usize) {
        use nexa_ctl::controls::ctxmenu::CtxItem;
        if self.settings.get("editor.close_unsaved") == Some("twice") {
            self.editors.close_tab_two_step(i);
            return;
        }
        // 저장은 **활성 탭**에 하는 동작이다 → 닫으려는 탭을 앞으로.
        self.editors.switch(i);
        self.sync_grid_tab();
        let title = self
            .editors
            .tab_list()
            .into_iter()
            .nth(i)
            .map(|(_, t, _)| t)
            .unwrap_or_default();
        self.sess.status = tf(Msg::StUnsavedAsk, &[&title]);
        let mut items = vec![
            CtxItem::item("close.save", t(Msg::MnCloseSave)),
            CtxItem::item("close.discard", t(Msg::MnCloseDiscard)),
        ];
        // ★ 미저장 탭이 둘 이상이면 일괄 처리(사용자 09-26): 모두 저장 · 모두 취소(변경 버리고 닫기) · 닫기 = 그대로 진행 취소.
        let n_dirty = self.dirty_tabs().len();
        if n_dirty >= 2 {
            items.push(CtxItem::Separator);
            items.push(CtxItem::item(
                "close.save_all",
                tf(Msg::MnCloseSaveAll, &[&n_dirty.to_string()]),
            ));
            items.push(CtxItem::item(
                "close.discard_all",
                tf(Msg::MnCloseDiscardAll, &[&n_dirty.to_string()]),
            ));
        }
        items.push(CtxItem::Separator);
        items.push(CtxItem::item("close.cancel", t(Msg::MnCloseCancel)));
        let r = self
            .editors
            .tab_rect(i)
            .map_or(self.status_tx_rect, |r| Rect::new(r.x, r.bottom(), 0, 0));
        self.open_status_popup(r, items);
        self.redraw();
    }

    /// 닫기 확인 메뉴의 답.
    pub(crate) fn close_pick(&mut self, id: &str) {
        let tab = self.editors.active_id();
        match id {
            "close.discard" => {
                if let Some(i) = self.editors.index_of_id(tab) {
                    if let Some(p) = self.editors.path_of(i) {
                        backups::remove(&p);
                    }
                    self.editors.close_tab_forced(i);
                    self.sync_grid_tab();
                }
                if self.exit_pending {
                    self.request_exit();
                }
            }
            "close.cancel" => {
                self.exit_pending = false;
                self.exit_project_asked = false;
                self.save_all_pending = false;
            }
            "close.save_all" => {
                self.save_all_pending = true;
                self.save_all_step();
            }
            "close.discard_all" => {
                for i in self.dirty_tabs().into_iter().rev() {
                    if let Some(p) = self.editors.path_of(i) {
                        backups::remove(&p);
                    }
                    self.editors.close_tab_forced(i);
                }
                self.sync_grid_tab();
                if self.exit_pending {
                    self.request_exit();
                }
            }
            "close.save" => {
                self.close_after_save = Some(tab);
                match self.editors.active_path() {
                    Some(p) => {
                        self.save_to(&p);
                        self.finish_close_after_save();
                    }
                    // 이름 없는 스크립트 = 저장 창(이미 있는 이름은 그 창이 타임아웃 버튼으로 다시 확인한다).
                    None => self.open_file_dlg = Some(PickerMode::Save),
                }
            }
            _ => {}
        }
        self.redraw();
    }

    /// "저장하고 닫기"의 뒷부분 — 저장이 실제로 끝나 그 탭에 바뀐 것이 없을 때만 닫는다(저장 실패 · 외부 변경 확인 대기 = 닫지 않는다).
    pub(crate) fn finish_close_after_save(&mut self) {
        let Some(tab) = self.close_after_save.take() else {
            return;
        };
        if self.save_all_pending {
            // 모두 저장: 저장이 끝난 탭은 그대로 두고(종료 중이면 종료 흐름이 닫는다) 다음 미저장 탭으로.
            self.save_all_step();
            return;
        }
        if let Some(i) = self.editors.index_of_id(tab) {
            if !self.editors.is_dirty(i) {
                self.editors.close_tab_forced(i);
                self.sync_grid_tab();
            }
        }
        if self.exit_pending {
            self.request_exit();
        }
    }

    pub(crate) fn close_tab_guarded(&mut self, i: usize) {
        let id = self.editors.tab_id(i);
        // 실행 중인 탭은 닫지 않는다(결과가 갈 곳이 사라진다) — ■로 중지한 뒤(09-19 검토).
        if self.editors.is_running(id) {
            self.sess.status = t(Msg::StTabRunningClose).into();
            self.redraw();
            return;
        }
        // 그 탭이 쓰는 세션의 대기 문장(전용 세션이면 그 세션 전부 = 탭을 닫으면 세션도 닫힌다 · docs/52 §8).
        let sid = self.sess_id_for_tab(id);
        let n = self.sess_by_id(sid).map_or(0, |s| {
            if s.is_private() {
                s.tx_pending.len()
            } else {
                s.tx_pending.iter().filter(|t| t.editor == id).count()
            }
        });
        if n == 0 {
            self.editors.close_tab_confirmed(i);
            return;
        }
        match self.settings.get("tx.close_action").unwrap_or("ask") {
            act @ ("commit" | "rollback") => {
                let commit = act == "commit";
                // 그 세션이 실행 중이면 커밋/롤백을 줄 세우지 않는다(T-122) — 탭을 닫지 않고 상태줄에 알린다.
                let sent = self.with_sess(sid, |a| {
                    let Some(pass) = a.gate_pass() else {
                        return false;
                    };
                    a.sess.submit(
                        pass,
                        if commit {
                            worker::Cmd::Commit
                        } else {
                            worker::Cmd::Rollback
                        },
                    );
                    a.tx_close(if commit {
                        TxOutcome::Committed
                    } else {
                        TxOutcome::RolledBack
                    });
                    true
                });
                if sent != Some(false) {
                    self.editors.close_tab_confirmed(i);
                }
            }
            _ => {
                // 묻는 팝업은 지금 세션에 답을 보낸다 → 그 탭을 먼저 앞으로.
                if self.sess.id != sid {
                    self.editors.switch(i);
                    self.sync_grid_tab();
                }
                self.tx_after = Some(TxAfter::CloseTab(i));
                self.open_tx_guard(Msg::MnTxCommitClose, Msg::MnTxRollbackClose);
            }
        }
    }

    /// 종료 — 미커밋이 있으면 묻는다.
    /// 종료 마무리(저장 · 창 크기 · 워커 종료 · 루프 종료) — 창 이벤트 끝과 유휴 틱 **둘 다**에서 부른다(09-24: 기동 명령·타이머로
    /// 요청한 종료가 창 이벤트가 없으면 영영 처리되지 않았다 — `scripts/func-block-comment.sh`가 잡음).
    pub(crate) fn finish_exit(&mut self, el: &ActiveEventLoop) {
        self.flush_on_exit();
        self.persist_window_sizes(true);
        for s in self.all_sess() {
            s.control(worker::Cmd::Quit);
        }
        el.exit();
    }

    pub(crate) fn request_exit(&mut self) {
        // ★ 종료 흐름(사용자 09-23): ① 프로젝트 — 자동 저장이면 저장 · 아니면 묻기 ② 미저장 **파일** 탭마다 묻기(스크립트 탭은
        //   프로젝트에 본문이 보존되므로 프로젝트가 있으면 묻지 않는다 · 없으면 전부 묻는다) ③ 트랜잭션 확인 → 종료.
        if self.project.is_open() {
            if self.project_autosave_on() {
                let _ = self.project_save();
            } else if !self.exit_project_asked && self.project_unsaved() {
                // 🔧 저장할 것이 있을 때만 묻는다(사용자 09-28 "방금 저장했어도 창이 계속 뜬다") — 자동 저장 틱과 같은 비교(문서 = 마지막 저장본).
                self.exit_pending = true;
                self.ask_project_exit();
                return;
            }
        }
        let keep_scratch = self.project.is_open();
        let dirty = (0..self.editors.tab_count()).find(|&i| {
            self.editors.is_dirty(i) && !(keep_scratch && self.editors.path_of(i).is_none())
        });
        if let Some(i) = dirty {
            self.exit_pending = true;
            self.ask_save_close(i);
            return;
        }
        self.exit_pending = false;
        self.exit_project_asked = false;
        if self.sess.tx_pending.is_empty() {
            // 잠든 세션에 미커밋이 있으면 그 탭을 앞으로 꺼내 거기서 묻는다(세션마다 한 번씩 · docs/52 §8).
            let tab = self
                .parked
                .iter()
                .find(|s| !s.tx_pending.is_empty())
                .map(|s| s.owner.unwrap_or(s.tx_pending[0].editor));
            if let Some(tab) = tab {
                self.editors.switch_to_id(tab);
                self.sync_grid_tab();
            }
            if self.sess.tx_pending.is_empty() {
                self.exit_requested = true;
                return;
            }
        }
        self.tx_after = Some(TxAfter::Exit);
        self.open_tx_guard(Msg::MnTxCommitExit, Msg::MnTxRollbackExit);
        self.redraw();
    }

    /// 물어야 할 미저장 탭(index · 종료 중이고 프로젝트가 열려 있으면 스크립트 탭은 프로젝트가 보존하므로 제외).
    fn dirty_tabs(&self) -> Vec<usize> {
        let keep_scratch = self.exit_pending && self.project.is_open();
        (0..self.editors.tab_count())
            .filter(|&i| {
                self.editors.is_dirty(i) && !(keep_scratch && self.editors.path_of(i).is_none())
            })
            .collect()
    }

    /// ★ 모두 저장(사용자 09-26): 미저장 탭을 앞에서부터 하나씩 — 경로가 있으면 바로 저장 · 이름 없는 탭은 저장 창(끝나면
    /// `finish_close_after_save`가 여기로 돌아온다) · 전부 저장되면 종료 중이면 종료 흐름을 잇는다 · 저장 실패/취소 = 멈춤.
    fn save_all_step(&mut self) {
        loop {
            let Some(i) = self.dirty_tabs().first().copied() else {
                self.save_all_pending = false;
                self.sess.status = t(Msg::StSavedAll).into();
                if self.exit_pending {
                    self.request_exit();
                }
                self.redraw();
                return;
            };
            self.editors.switch(i);
            self.sync_grid_tab();
            let tab = self.editors.active_id();
            match self.editors.active_path() {
                Some(p) => {
                    self.save_to(&p);
                    if self.editors.is_dirty(i) {
                        // 저장 실패(오류·외부 변경 확인 대기) — 멈추고 사용자에게(상태줄은 save_to가 썼다).
                        self.save_all_pending = false;
                        self.exit_pending = false;
                        self.redraw();
                        return;
                    }
                }
                None => {
                    self.close_after_save = Some(tab);
                    self.open_file_dlg = Some(PickerMode::Save);
                    self.redraw();
                    return;
                }
            }
        }
    }

    /// 최근 파일(설정 `file.recent` · `|` 구분 · 최신 먼저 · 존재하는 것만).
    pub(crate) fn recent_files(&self) -> Vec<PathBuf> {
        self.settings
            .get("file.recent")
            .unwrap_or("")
            .split('|')
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .take(8)
            .collect()
    }

    pub(crate) fn push_recent(&mut self, path: &Path) {
        let mut v = self.recent_files();
        v.retain(|p| p != path);
        v.insert(0, path.to_path_buf());
        v.truncate(8);
        let joined = v
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("|");
        let _ = self.settings.set("file.recent", &joined);
        self.persist_settings();
        let tabs = self.editors.tab_list_dirty();
        self.menubar.set_menus(App::build_menus_with(
            &v,
            &tabs,
            self.demo_ready,
            self.gate_shown.unwrap_or(false),
            self.project_menu_entries(),
            (self.theme_menu_entries(), self.lang_menu_entries()),
        ));
    }

    /// 대화상자를 닫을 때 마지막 폴더·숨김 표시를 기억한다.
    pub(crate) fn remember_file_dialog(&mut self, dir: Option<&Path>) {
        let dir = dir
            .map(Path::to_path_buf)
            .or_else(|| self.file_win.current_dir());
        if let Some(d) = dir {
            let _ = self.settings.set("file.last_dir", &d.to_string_lossy());
        }
        if let Some(h) = self.file_win.show_hidden() {
            let _ = self
                .settings
                .set("file.show_hidden", if h { "on" } else { "off" });
        }
        if let Some(d) = self.file_win.show_dot() {
            let _ = self
                .settings
                .set("file.show_dot", if d { "on" } else { "off" });
        }
        self.persist_settings();
    }

    pub(crate) fn open_file_window(&mut self, el: &ActiveEventLoop, mode: PickerMode) {
        let over = self.window.as_ref().and_then(|w| {
            let p = w.outer_position().ok()?;
            let sz = w.outer_size();
            Some((p.x, p.y, sz.width, sz.height))
        });
        let owner = self.window.clone();
        // 시작 폴더 = 활성 탭 파일의 폴더 → 마지막 폴더 → 홈.
        let start = self
            .editors
            .active_path()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| {
                self.settings
                    .get("file.last_dir")
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
            });
        let default_name = match (mode, self.file_purpose) {
            (PickerMode::Save, FilePurpose::LogExport) => {
                let ext = match self.settings.get("log.format").unwrap_or("raw") {
                    "markdown" => "md",
                    "jsonl" => "jsonl",
                    "csv" => "csv",
                    "tsv" => "tsv",
                    _ => "log",
                };
                format!("nexa-sql.{ext}")
            }
            (
                PickerMode::Save,
                FilePurpose::Editor | FilePurpose::RunFile | FilePurpose::SettingFolder(_),
            ) => {
                let t = self.editors.active_title();
                if t.contains('.') {
                    t
                } else {
                    format!("{t}.sql")
                }
            }
            (PickerMode::Save, FilePurpose::Project) => format!(
                "{}.{}",
                self.project.name().unwrap_or_else(|| "untitled".into()),
                project::EXT
            ),
            (PickerMode::Save, FilePurpose::SqlPreview | FilePurpose::CellValue) => {
                self.sqlprev_win.file_name()
            }
            (PickerMode::Open | PickerMode::Folder, _)
            | (PickerMode::Save, FilePurpose::Import | FilePurpose::License) => String::new(),
        };
        // ★ 파일 창의 호스트 창(그 위에 뜨고 · 닫히면 **그 창으로** 포커스가 돌아간다): 설정 폴더 = 설정 창 · 라이선스 = 라이선스 창.
        //   메인으로 돌아가면 z-order가 바뀌어 호스트 창이 메인 뒤로 숨는다(사용자 09-27 "라이선스 파일을 고르면 창이 뒤로").
        let host: Option<Rc<Window>> = match (mode, self.file_purpose) {
            (PickerMode::Folder, _) => self.prefs_win.window_rc(),
            (_, FilePurpose::License) => self.license_win.window_rc(),
            _ => None,
        };
        let over2 = host.as_ref().and_then(|w| {
            let p = w.outer_position().ok()?;
            let sz = w.outer_size();
            Some((p.x, p.y, sz.width, sz.height))
        });
        self.picker_return = host.as_ref().map(|w| w.id());
        // 폴더 고르기: 시작 = 그 설정의 지금 값.
        let start = if mode == PickerMode::Folder {
            self.folder_start.take().or(start)
        } else {
            start
        };
        let owner = host.or(owner);
        let over = over2.or(over);
        let recent_dirs: Vec<PathBuf> = self
            .recent_files()
            .iter()
            .filter_map(|p| p.parent().map(Path::to_path_buf))
            .fold(Vec::new(), |mut acc, d| {
                if !acc.contains(&d) {
                    acc.push(d);
                }
                acc
            });
        let show_hidden = self.settings.flag("file.show_hidden");
        let show_dot = self.settings.flag("file.show_dot");
        let encoding = match mode {
            PickerMode::Open | PickerMode::Folder => "auto".to_string(),
            PickerMode::Save => self.editors.active_encoding(),
        };
        self.file_win.set_next_filters(
            matches!(self.file_purpose, FilePurpose::License).then(file_win::license_filters),
        );
        self.file_win
            .set_overwrite_confirm_ms(self.settings.int("file.overwrite_confirm_ms").max(0) as u64);
        self.file_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            over,
            owner.as_deref(),
            mode,
            start.as_deref(),
            &default_name,
            recent_dirs,
            show_hidden,
            show_dot,
            &encoding,
            // 편집기 파일 열기만 다중 · 프로젝트/실행 파일/내보내기/설정 폴더는 하나(사용자 09-22).
            !matches!(self.file_purpose, FilePurpose::Editor),
        );
    }

    /// 파일 → 탭(자동 감지: BOM으로 UTF-8/UTF-16 판별 · 없으면 UTF-8).
    pub(crate) fn open_file(&mut self, path: &Path) {
        self.open_file_enc(path, "auto");
        self.folder_title_apply(path);
    }

    /// 바이트 → 문자열(인코딩 지정 · `auto` = BOM 감지). 돌려주는 값 = (본문, 대체 문자 발생, 실제 인코딩).
    pub(crate) fn decode_bytes(bytes: &[u8], enc: &str) -> (String, bool, &'static str) {
        enc::decode(bytes, enc)
    }

    fn encode_text(text: &str, enc: &str) -> Vec<u8> {
        enc::encode(text, enc)
    }

    /// 파일 → 탭(인코딩 지정). 깨진 바이트는 대체 문자 + 안내 · `\r\n`은 `\n`으로(저장 때 되돌린다).
    pub(crate) fn open_file_enc(&mut self, path: &Path, enc: &str) {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        // ★ 무료판 편집 용량(D-46 · 25 §13-3): 2 MB 이상은 **읽기 전용**으로 연다(실행·조회는 그대로 · 큰 파일 모드 L2와 같은 길).
        if !self.entitled(nsql_license::Feature::LargeEdit) && size >= (2u64 << 20) {
            self.license_limit_note(nsql_license::Feature::LargeEdit);
            self.load_file(path, enc, LoadMode::ReadOnly);
            return;
        }
        let ask = (self.settings.int("file.large_ask_mb").max(0) as u64) << 20;
        // ★ 큰 파일(docs/59 §4 3단계): 기준을 넘으면 **먼저 묻는다** — 열기 / 읽기 전용 / 앞부분만 / 열지 않고 실행.
        if ask > 0 && size >= ask {
            self.big_pending = Some((path.to_path_buf(), enc.to_string(), size));
            let mb = format!("{:.0}", size as f64 / 1048576.0);
            let head = self.settings.int("file.large_head_mb").max(1).to_string();
            self.palette.set_commands(vec![
                ("bigfile.open".into(), tf(Msg::BigOpen, &[&mb])),
                ("bigfile.readonly".into(), t(Msg::BigReadOnly).into()),
                ("bigfile.head".into(), tf(Msg::BigHead, &[&head])),
                ("bigfile.run".into(), t(Msg::BigRun).into()),
            ]);
            self.palette.open("");
            self.ime_refresh();
            self.redraw();
            return;
        }
        self.load_file(path, enc, LoadMode::Open);
    }

    /// 파일을 읽어 `mode`대로 쓴다. 큰 파일(`file.async_load_mb` 이상)은 **자리 탭**을 먼저 만들고 작업 스레드가 읽기·풀기·
    /// 편집기 본문 준비까지 끝낸다 — 그동안 진행 막은 그 탭 안에만 보이고 다른 탭은 그대로 쓴다(`fileload.rs`).
    pub(crate) fn load_file(&mut self, path: &Path, enc: &str, mode: LoadMode) {
        let to_tab = mode != LoadMode::Run;
        if matches!(mode, LoadMode::Open | LoadMode::ReadOnly) {
            // 이미 열려 있으면 읽지 않고 그 탭으로 · 같은 파일을 읽는 중이면 그 자리 탭으로.
            if let Some(i) = self.editors.path_tab(path) {
                self.editors.switch(i);
                self.set_focus(Focus::Editor);
                self.layout();
                self.redraw();
                return;
            }
            let dup = self
                .file_loads
                .iter()
                .find(|j| j.path == path)
                .and_then(|j| j.tab);
            if let Some(i) = dup.and_then(|id| self.editors.index_of_id(id)) {
                self.editors.switch(i);
                self.redraw();
                return;
            }
        }
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let limit = match mode {
            LoadMode::Head(n) => Some(n),
            _ => None,
        };
        let origin = self.editors.active_id();
        let async_at = (self.settings.int("file.async_load_mb").max(1) as u64) << 20;
        if size < async_at {
            let result = fileload::load(path, enc, limit, to_tab, None);
            self.file_loaded(FileLoaded {
                path: path.to_path_buf(),
                mode,
                tab: None,
                origin,
                result,
            });
            return;
        }
        let name = editors::file_title(path);
        let (tx, rx) = std::sync::mpsc::channel();
        let (p, e) = (path.to_path_buf(), enc.to_string());
        let proxy = std::sync::Mutex::new(self.wake_proxy.clone());
        let prog = std::sync::Arc::new(fileload::Progress::default());
        let prog_t = prog.clone();
        let spawned = std::thread::Builder::new()
            .name("file-load".into())
            .spawn(move || {
                let result = fileload::load(&p, &e, limit, to_tab, Some(&prog_t));
                let _ = tx.send(result);
                if let Ok(px) = proxy.lock() {
                    let _ = px.send_event(Wake);
                }
            });
        if spawned.is_err() {
            let result = fileload::load(path, enc, limit, to_tab, None);
            self.file_loaded(FileLoaded {
                path: path.to_path_buf(),
                mode,
                tab: None,
                origin,
                result,
            });
            return;
        }
        // 자리 탭(편집기에 실을 때만) — 비어 있고 읽기 전용 · 경로 없음. "앞부분만"은 읽을거리라 No connection.
        let tab = to_tab.then(|| {
            let id = self.editors.begin_load_tab(&name);
            if matches!(mode, LoadMode::Head(_)) {
                self.make_unconnected(id);
            }
            id
        });
        self.file_loads.push(LoadJob {
            rx,
            path: path.to_path_buf(),
            mode,
            tab,
            origin,
            name,
            total: limit.map_or(size, |n| n.min(size)),
            started: Instant::now(),
            prog,
            ready: None,
            shown_full: false,
        });
        self.sess.status = tf(
            Msg::StFileLoading,
            &[&nexa_fs::path::display(path), &nsql_core::fmt_bytes(size)],
        );
        if to_tab {
            self.set_focus(Focus::Editor);
        }
        self.layout();
        self.redraw();
    }

    /// 적재 스레드의 결과 수거(틱). 자리 탭이 닫혔으면 그 적재를 취소한다. 진행 막이 보이는 중(자리 탭이 활성 + 지연 지남)이면
    /// **100% 프레임을 한 번 그린 뒤에** 옮겨 넣는다 — 사용자가 본 마지막 값이 100%가 되게.
    pub(crate) fn file_loads_poll(&mut self) {
        if self.file_loads.is_empty() {
            return;
        }
        let delay = Duration::from_millis(self.settings.int("file.load_progress_ms").max(0) as u64);
        let active = self.editors.active_id();
        let mut done = Vec::new();
        let mut k = 0;
        while k < self.file_loads.len() {
            // 자리 탭을 닫았다 = 그 적재 취소(스레드는 다음 덩어리에서 멈춘다).
            let closed = self.file_loads[k]
                .tab
                .is_some_and(|id| self.editors.index_of_id(id).is_none());
            if closed {
                let job = self.file_loads.remove(k);
                job.prog.cancel();
                self.sess.status = tf(Msg::StLoadCancelled, &[&job.name]);
                self.mem_released();
                continue;
            }
            let job = &mut self.file_loads[k];
            if job.ready.is_none() {
                match job.rx.try_recv() {
                    Ok(r) => job.ready = Some(r),
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        job.ready = Some(Err("file-load thread ended".into()));
                    }
                }
            }
            let visible =
                job.tab == Some(active) && fileload::overlay_due(job.started.elapsed(), delay);
            if job.ready.is_some() && (!visible || job.shown_full) {
                let job = self.file_loads.remove(k);
                if let Some(result) = job.ready {
                    done.push(FileLoaded {
                        path: job.path,
                        mode: job.mode,
                        tab: job.tab,
                        origin: job.origin,
                        result,
                    });
                }
                continue;
            }
            k += 1;
        }
        for l in done {
            self.file_loaded(l);
        }
        // 막이 보이는 동안만 계속 다시 그린다(다른 탭에서 일하는 중에는 프레임을 만들지 않는다).
        if self.file_loads.iter().any(|j| j.tab == Some(active)) {
            self.redraw();
        }
    }

    /// 활성 탭의 적재 취소(Esc · `file.load_cancel`): 스레드에 알리고 자리 탭을 닫는다. 돌려주는 값 = 취소했는가.
    pub(crate) fn file_load_cancel_active(&mut self) -> bool {
        let active = self.editors.active_id();
        let Some(k) = self.file_loads.iter().position(|j| j.tab == Some(active)) else {
            return false;
        };
        let job = self.file_loads.remove(k);
        job.prog.cancel();
        if let Some(i) = self.editors.index_of_id(active) {
            self.editors.close_tab_confirmed(i);
            self.editors.reap_views();
        }
        self.sess.status = tf(Msg::StLoadCancelled, &[&job.name]);
        self.mem_released();
        self.layout();
        self.redraw();
        true
    }

    /// 진행 막 그리기(편집 영역 가운데) — **활성 탭이 적재 중인 탭일 때만** · 지연이 지난 뒤. 100%를 그렸으면 표시해 둔다
    /// (수거가 그걸 보고 옮겨 넣는다). 그리는 동안 `surface`가 `self`를 잡고 있어 필드만 받는 연관 함수로 둔다.
    pub(crate) fn paint_file_load(
        jobs: &mut [LoadJob],
        active: u64,
        delay: Duration,
        area: Rect,
        dc: &mut dyn nexa_ctl::draw::DrawCtx,
        th: &nexa_ctl::theme::Theme,
        s: f32,
    ) {
        let more = jobs.len().saturating_sub(1);
        let Some(job) = jobs.iter_mut().find(|j| j.tab == Some(active)) else {
            return;
        };
        if !fileload::overlay_due(job.started.elapsed(), delay) {
            return;
        }
        let stage = if job.ready.is_some() {
            fileload::Stage::Opening
        } else {
            // 스레드는 끝났어도 아직 받지 않았으면 그 앞 단계에 둔다 — 100%는 "받았다"는 뜻으로만 쓴다.
            match job.prog.stage() {
                fileload::Stage::Opening => fileload::Stage::Preparing,
                st => st,
            }
        };
        fileload::paint(
            dc,
            th,
            area,
            s,
            &fileload::View {
                name: &job.name,
                stage,
                read: job.prog.read(),
                total: job.total,
                started: job.started,
                more,
            },
        );
        if stage == fileload::Stage::Opening {
            job.shown_full = true;
        }
    }

    /// 읽은 결과를 쓴다 — 편집기에 실을 것은 자리 탭(없으면 지금 만든다 = 작은 파일의 동기 경로)에 **옮겨 넣고**,
    /// 실행만 할 것은 돌린다. 활성 탭은 바꾸지 않는다(사용자가 다른 탭에서 일하고 있을 수 있다).
    pub(crate) fn file_loaded(&mut self, l: FileLoaded) {
        let path = l.path;
        let name = editors::file_title(&path);
        let loaded = match l.result {
            Ok(v) => v,
            Err(e) => {
                // 자리 탭은 걷는다(빈 탭이 남지 않게).
                if let Some(i) = l.tab.and_then(|id| self.editors.index_of_id(id)) {
                    self.editors.close_tab_confirmed(i);
                    self.editors.reap_views();
                }
                self.sess.status = if e == fileload::CANCELLED {
                    tf(Msg::StLoadCancelled, &[&name])
                } else {
                    tf(Msg::StFileReadError, &[&path.display().to_string(), &e])
                };
                self.layout();
                self.redraw();
                return;
            }
        };
        let (eol, used, lossy, truncated) =
            (loaded.eol, loaded.used, loaded.lossy, loaded.truncated);
        match (l.mode, loaded.body) {
            (LoadMode::Run, fileload::Body::Text(text)) => {
                // ★ 읽는 사이 활성 탭이 바뀌었으면 실행하지 않는다 — 실행은 활성 탭의 세션으로 가므로, 사용자가 고른 것과
                //   **다른 접속**(운영일 수도 있다)에서 큰 스크립트가 돌 뻔한 경우다.
                if self.editors.active_id() != l.origin {
                    let msg = tf(Msg::StRunFileTabChanged, &[&name]);
                    self.log_win
                        .push(LogEntry::new(LogKind::Error, msg.clone()));
                    self.sess.status = msg;
                } else if self.gate_open() {
                    let n = nsql_script::split_script(&text).len();
                    self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        tf(Msg::StRunFile, &[&name, &n.to_string()]),
                    ));
                    self.run_text(text, 0, true);
                }
            }
            (mode, fileload::Body::Prepared(prep)) => {
                let head = matches!(mode, LoadMode::Head(_));
                let tab = match l.tab {
                    Some(id) => id,
                    None => {
                        let id = self.editors.begin_load_tab(&name);
                        if head {
                            self.make_unconnected(id);
                        }
                        id
                    }
                };
                // "앞부분만" = 경로 없는 읽기 전용 읽을거리(저장해도 원본을 덮지 않는다) · 원래 파일의 구문.
                let title = if head && truncated {
                    tf(Msg::BigHeadTitle, &[&name])
                } else {
                    name.clone()
                };
                let keep_path = (!head).then_some(path.as_path());
                let t_fill = Instant::now();
                let filled = self
                    .editors
                    .fill_loaded(tab, keep_path, &title, &name, prep, eol);
                if self.frame_trace.is_some() {
                    // 계측(`NSQL_TRACE_FRAMES`): UI 스레드가 옮겨 넣느라 멎은 시간 — 다른 탭에서 타이핑 중이면 이만큼 끊긴다.
                    eprintln!(
                        "[load] fill {name}: {:.1} ms on the UI thread",
                        t_fill.elapsed().as_secs_f64() * 1000.0
                    );
                }
                let Some(i) = filled else {
                    self.mem_released();
                    return; // 자리 탭이 그사이 닫혔다 — 버린다.
                };
                self.editors.set_encoding(i, used);
                if head || mode == LoadMode::ReadOnly {
                    self.editors.set_read_only(i, true);
                }
                let mut restored = 0;
                if !head {
                    self.ext_track(tab);
                    self.push_recent(&path);
                    if mode == LoadMode::Open {
                        restored = self.undo_persist_load(i, &path);
                        self.vars_persist_load(tab, &path);
                        self.bookmarks.on_opened(&self.editors, i);
                        self.bm_refresh_tab(i);
                    }
                }
                if self.editors.active_id() == tab {
                    self.set_focus(Focus::Editor);
                }
                let level = self.editors.large_level(i);
                self.sess.status = if lossy {
                    tf(Msg::StFileDecodedLossy, &[&name])
                } else if level > 0 {
                    tf(Msg::StLargeMode, &[&name, &level.to_string()])
                } else if restored > 0 {
                    tf(Msg::StUndoRestored, &[&title, &restored.to_string()])
                } else {
                    tf(Msg::StFileOpened, &[&title])
                };
            }
            // 짝이 맞지 않는 조합은 만들지 않는다(실행 = 문자열 · 그 밖 = 준비본).
            (_, fileload::Body::Text(_)) => {}
        }
        // 읽느라 쓴 임시 버퍼를 놓았다 → 힙 정리 예약.
        self.mem_released();
        self.layout();
        self.redraw();
    }

    /// 변수 표 보존(D-136 · `varsfile`) — 실행이 그 탭의 변수를 바꿨을 때 · 파일이 있는 탭만.
    pub(crate) fn vars_persist_save(&mut self, tab: u64, vars: &[nsql_script::VarState]) {
        if !self.settings.flag("vars.persist") {
            return;
        }
        let Some(path) = self
            .editors
            .index_of_id(tab)
            .and_then(|i| self.editors.path_of(i))
        else {
            return;
        };
        let Some(dir) = varsfile::dir() else { return };
        if !self.vars_pruned {
            self.vars_pruned = true;
            varsfile::prune_in(&dir, self.settings.int("vars.persist_days").max(0) as u64);
        }
        varsfile::store_in(&dir, &path, vars);
    }

    /// 방금 연 파일의 변수 표를 되살린다(그 탭에 아직 표가 없을 때만).
    fn vars_persist_load(&mut self, tab: u64, path: &Path) {
        if !self.settings.flag("vars.persist") || self.tab_vars.contains_key(&tab) {
            return;
        }
        let Some(dir) = varsfile::dir() else { return };
        let vars = varsfile::load_in(&dir, path);
        if !vars.is_empty() {
            self.tab_vars.insert(tab, vars);
        }
    }

    /// 되돌리기 기록 파일 쓰기(docs/60 D-129 · 저장 직후 · 활성 탭) — 기록이 없으면 옛 파일을 지운다. 큰 파일 탭은 건너뛴다.
    fn undo_persist_save(&mut self, path: &Path) {
        if !self.settings.flag("editor.undo_persist") {
            return;
        }
        let Some(dir) = undofile::dir() else { return };
        if !self.undo_pruned {
            self.undo_pruned = true;
            undofile::prune_in(
                &dir,
                self.settings.int("editor.undo_persist_days").max(0) as u64,
            );
        }
        let i = self.editors.active();
        if self.editors.large_level(i) > 0 {
            undofile::store_in(&dir, path, None);
            return;
        }
        let cap = (self.settings.int("editor.undo_persist_mb").max(1) as usize) << 20;
        let bytes = self.editors.cur().export_history(cap);
        undofile::store_in(&dir, path, bytes.as_deref());
    }

    /// 방금 읽은 탭에 되돌리기 기록을 되살린다(본문이 기록의 것과 같을 때만) — 되살린 단계 수.
    fn undo_persist_load(&mut self, i: usize, path: &Path) -> usize {
        if !self.settings.flag("editor.undo_persist") || self.editors.large_level(i) > 0 {
            return 0;
        }
        let cap = (self.settings.int("editor.undo_persist_mb").max(1) as u64) << 20;
        let Some(bytes) = undofile::dir().and_then(|d| undofile::load_in(&d, path, cap)) else {
            return 0;
        };
        self.editors.import_history(i, &bytes)
    }

    /// 활성 탭 → 파일(UTF-8 · BOM 없음 · 원래 줄끝 유지).
    pub(crate) fn save_to(&mut self, path: &Path) {
        // 저장 직전 확인(docs/58 — 어떤 감지도 놓칠 수 있다 · 마지막 안전망): 디스크가 읽어 온 것과 다르면 2단 저장.
        if !self.ext_save_guard(path) {
            return;
        }
        // 저장 줄끝 = 설정 `file.eol_save`(keep = 탭 줄끝) · docs/38.
        let eol = eol::save_eol(
            self.settings.get("file.eol_save").unwrap_or("keep"),
            self.editors.active_eol(),
        );
        let text = eol::apply(&self.editors.cur().text(), eol);
        let enc = self.editors.active_encoding();
        let text = Self::encode_text(&text, &enc);
        let tmp = path.with_extension(format!(
            "{}.nsql-tmp",
            path.extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        let res = std::fs::write(&tmp, &text).and_then(|()| std::fs::rename(&tmp, path));
        match res {
            Ok(()) => {
                self.editors.mark_saved(path);
                backups::remove(path);
                self.undo_persist_save(path);
                self.ext_track_active();
                self.git.refresh(true);
                self.push_recent(path);
                // 새 파일이 생겼을 수 있다 — 프로젝트 탐색기의 펼친 폴더를 다시 열거.
                if self.project_panel.is_visible() {
                    self.project_panel.refresh();
                }
                let name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.sess.status = tf(Msg::StFileSaved, &[&name]);
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                self.sess.status = tf(
                    Msg::StFileWriteError,
                    &[&path.display().to_string(), &e.to_string()],
                );
            }
        }
        self.redraw();
    }
}
