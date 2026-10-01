//! App — 기동 명령(`NSQL_STARTUP_CMD` · 자체 시험·자동 점검 경로 · docs/61 §4).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 자체 캡처용 기동 명령 하나 — `open:<경로>` = 파일을 탭으로 · 나머지 = 명령 id.
    pub(crate) fn startup_cmd(&mut self, id: &str) {
        // `conn.edit:<프로필>` = 로그인 창의 상세 폼을 그 프로필로 열고 두 칸을 바꾼 상태로(필수·바뀜 표식 캡처).
        // 자체 캡처용: 이름 바꾸기 입력 상자를 키 없이 연다(`tab.rename` = 편집기 탭 · `result.rename` = 결과 탭).
        // 자체 캡처용: 설정의 "찾아보기…"를 키·마우스 없이 누른다(`prefs.browse:<폴더 설정 키>`).
        if let Some(key) = id.strip_prefix("prefs.browse:") {
            if let Some(k) = nsql_settings::entry(key).map(|e| e.key) {
                self.file_purpose = FilePurpose::SettingFolder(k);
                self.folder_start = self
                    .settings
                    .get(k)
                    .map(PathBuf::from)
                    .filter(|p| p.is_dir());
                self.open_file_dlg = Some(PickerMode::Folder);
            }
            return;
        }
        // 자체 캡처용: 탐색기 우클릭 메뉴(`explorer.menu[:n]` = n번째 보이는 줄) · 항목 고르기(`explorer.pick:<id>`).
        // 자체 시험용: **앱 안에서** 마우스 사건을 만든다(`ui.move:x/y` · `ui.click:x/y` · `ui.rclick:x/y` — 창 좌표 · 장치 픽셀 · 쉼표는 명령 구분자라 못 쓴다).
        //   OS 입력 주입이 아니다 — 사용자의 커서·포커스·전경 창을 건드리지 않고, 실제 입력과 같은 `route` 경로를 그대로 탄다.
        if let Some((kind, xy)) = id
            .strip_prefix("ui.")
            .and_then(|r| r.split_once(':'))
            .filter(|(k, _)| {
                matches!(
                    *k,
                    "move"
                        | "click"
                        | "dclick"
                        | "sclick"
                        | "cclick"
                        | "rclick"
                        | "wheel"
                        | "hwheel"
                )
            })
        {
            let mut it = xy
                .split(['/', 'x'])
                .map(|v| v.trim().parse::<i32>().unwrap_or(0));
            let (x, y) = (it.next().unwrap_or(0), it.next().unwrap_or(0));
            // `ui.wheel:x/y/delta` · `ui.hwheel:x/y/delta` = 커서 아래로 휠 사건(가로 스크롤 결함 재현 · 09-22).
            let delta = it.next().unwrap_or(120);
            self.cursor = (x, y);
            self.route(InputEvent::MouseMove { x, y });
            match kind {
                "wheel" => self.route(InputEvent::Wheel { delta }),
                "hwheel" => self.route(InputEvent::HWheel { delta }),
                "click" | "dclick" | "sclick" | "cclick" => {
                    // `sclick` = Shift+클릭 · `cclick` = Ctrl/⌘+클릭(동시 편집 탭 선택 캡처 · 09-22) ·
                    // `dclick` = 더블클릭(같은 자리 두 번 · 프로젝트 탐색기의 정식 탭 열기 캡처).
                    let times = if kind == "dclick" { 2 } else { 1 };
                    for _ in 0..times {
                        self.route(InputEvent::MouseDown {
                            x,
                            y,
                            shift: kind == "sclick",
                            primary: kind == "cclick",
                        });
                        self.route(InputEvent::MouseUp { x, y });
                    }
                }
                "rclick" => self.route(InputEvent::RightDown { x, y }),
                _ => {}
            }
            self.redraw();
            return;
        }
        // 자체 시험용(83 · 09-25): 줄 펼치기 · 트리 덤프 · SQL Preview 덤프(키 주입 없이 구조·생성 결과를 파일로 확인).
        if let Some(rest) = id.strip_prefix("explorer.expand") {
            let arg = rest.trim_start_matches(':');
            // 행 번호 또는 라벨 경로(`DB/Tables` · `Tables` · 101 E2E) — 보이는 행 기준.
            let ok = match arg.parse::<usize>() {
                Ok(row) => self.explorer.capture_expand(row),
                Err(_) => self.explorer.capture_expand_label(arg),
            };
            let row = arg;
            self.sess.status = format!("explorer.expand row={row} ok={ok}");
            self.redraw();
            return;
        }
        if let Some(q) = id.strip_prefix("explorer.filter:") {
            self.explorer.set_filter_text(q);
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("explorer.dump:") {
            let _ = std::fs::write(path, self.explorer.dump_rows());
            return;
        }
        if let Some(rest) = id.strip_prefix("explorer.select") {
            if let Ok(row) = rest.parse::<usize>() {
                self.explorer.capture_select(row);
                self.explorer_actions();
                self.redraw();
            }
            return;
        }
        if let Some(path) = id.strip_prefix("details.dump:") {
            let _ = std::fs::write(path, self.objdetail.text());
            return;
        }
        if let Some(path) = id.strip_prefix("explorer.stat:") {
            let _ = std::fs::write(path, self.explorer.stat_text());
            return;
        }
        // ★ 자체 시험(그리드 편집 · docs/87 §9 E-3 · 키 주입 0): `grid.edit.set:<행>,<열>,<글>` · `grid.edit.cmd:<id>` · `grid.dump:<파일>`.
        if let Some(rest) = id.strip_prefix("grid.edit.set:") {
            // 기동 명령 목록이 쉼표로 나뉘므로 여기 구분자는 `;` — `grid.edit.set:<행>;<열>;<글>`.
            let mut it = rest.splitn(3, ';');
            let r = it.next().and_then(|v| v.trim().parse::<usize>().ok());
            let c = it.next().and_then(|v| v.trim().parse::<usize>().ok());
            let text = it.next().unwrap_or("");
            if let (Some(r), Some(c)) = (r, c) {
                let ok = self.grid.set_cell_for_test(r, c, text);
                self.sess.status = format!("grid.edit.set {r},{c} ok={ok}");
                self.after_grid_event();
                self.redraw();
            }
            return;
        }
        if let Some(cmd) = id.strip_prefix("grid.edit.cmd:") {
            let ok = self.grid.edit_command(cmd);
            self.sess.status = format!("grid.edit.cmd {cmd} ok={ok}");
            self.after_grid_event();
            self.redraw();
            return;
        }
        // 자체 시험(87 §5): 파일에서 셀에 넣기 `grid.edit.load:<행>;<열>;<파일>` · 값 창 `cellview.open:<행>;<열>` ·
        //   `cellview.mode:<text|hex|image>` · `cellview.dump:<파일>` · `cellview.save:<파일>`.
        if let Some(rest) = id.strip_prefix("grid.edit.load:") {
            let parts: Vec<&str> = rest.splitn(3, ';').collect();
            if let (Some(r), Some(c), Some(f)) = (
                parts.first().and_then(|s| s.parse::<usize>().ok()),
                parts.get(1).and_then(|s| s.parse::<usize>().ok()),
                parts.get(2),
            ) {
                self.grid.select_cell_for_test(r, c);
                self.cell_load_file_at(r, c, std::path::Path::new(f));
                self.after_grid_event();
                self.redraw();
            }
            return;
        }
        if let Some(rest) = id.strip_prefix("cellview.open:") {
            let mut it = rest.split(';').filter_map(|s| s.parse::<usize>().ok());
            if let (Some(r), Some(c)) = (it.next(), it.next()) {
                self.grid.select_cell_for_test(r, c);
                self.grid.edit_command("grid.edit.view_value");
                self.after_grid_event();
                self.redraw();
            }
            return;
        }
        if let Some(m) = id.strip_prefix("cellview.mode:") {
            let mode = match m {
                "hex" => sqlprev_win::ValueMode::Hex,
                "image" => sqlprev_win::ValueMode::Image,
                _ => sqlprev_win::ValueMode::Text,
            };
            self.sqlprev_win.set_mode(mode);
            return;
        }
        if let Some(path) = id.strip_prefix("cellview.dump:") {
            let _ = std::fs::write(path, self.sqlprev_win.dump_value());
            return;
        }
        if let Some(path) = id.strip_prefix("cellview.save:") {
            let _ = std::fs::write(path, self.sqlprev_win.value_bytes());
            return;
        }
        if let Some(rest) = id.strip_prefix("grid.select:") {
            if let Some((r, c)) = rest.split_once(';') {
                if let (Ok(r), Ok(c)) = (r.trim().parse::<usize>(), c.trim().parse::<usize>()) {
                    self.grid.select_cell_for_test(r, c);
                    self.redraw();
                }
            }
            return;
        }
        if let Some(path) = id.strip_prefix("grid.dump:") {
            let _ = std::fs::write(path, self.grid.dump_edit());
            return;
        }
        // 자체 시험(10-01): 삭제 창 덤프(`drop.dump:<파일>` · 단계·무장·정보 줄·DROP 문·백업 경로).
        // 자체 시험(10-01): 삭제 창의 2단 확인을 거친 것과 같은 길(키·마우스 주입 없이 흐름 검증) — `drop.confirm` · `drop.confirm_nobackup`.
        if id == "drop.confirm" {
            self.drop_action(crate::drop_win::DropAction::Proceed);
            return;
        }
        if id == "drop.confirm_nobackup" {
            self.drop_action(crate::drop_win::DropAction::ProceedNoBackup);
            return;
        }
        if let Some(path) = id.strip_prefix("drop.dump:") {
            let _ = std::fs::write(path, self.drop_dump());
            return;
        }
        // 자체 시험(09-30): 활성 편집기 탭의 Output 본문 덤프(`output.dump:<파일>` · 첫 줄 `보임|줄수`).
        if let Some(path) = id.strip_prefix("output.dump:") {
            let _ = std::fs::write(path, self.output_dump());
            return;
        }
        // 자체 시험(09-30): 활성 편집 탭 덤프 — 첫 줄 `제목|탭 종류|읽기 전용` · 둘째 줄부터 본문(소스 열기·DDL 열기 결과를 파일로 대조).
        // 자체 시험(10-01 ⑭ · 키 주입 0): 캐럿 이동(`editor.caret:<글자 인덱스|end>`) · 완성 호출(`intel.probe`) · 후보 덤프(`intel.dump:<파일>`).
        if let Some(n) = id.strip_prefix("editor.caret:") {
            let mut inv = Invalidations::default();
            let ed = self.editors.cur_mut();
            let idx = if n == "end" {
                ed.text().chars().count()
            } else {
                n.parse().unwrap_or(0)
            };
            ed.select_range(idx, idx, &mut inv);
            self.redraw();
            return;
        }
        if id == "intel.probe" {
            self.focus = Focus::Editor;
            self.intel_request(true);
            return;
        }
        if let Some(path) = id.strip_prefix("intel.dump:") {
            let _ = std::fs::write(path, self.intel.dump_cands());
            return;
        }
        if let Some(path) = id.strip_prefix("editor.dump:") {
            let i = self.editors.active();
            let head = format!(
                "{}|{:?}|{}
",
                self.editors.title_of(i),
                self.editors.tab_kind(i),
                self.editors.active_read_only()
            );
            let _ = std::fs::write(path, head + &self.editors.cur().text());
            return;
        }
        // 자체 시험(09-26): 트랜잭션 로그 덤프 — 전 세션 · 전 목적 · 닫힌 것 포함 · 한 줄 = `목적|본문|행|결과|tx` + 열린 트랜잭션 갱신 수.
        if let Some(path) = id.strip_prefix("txlog.dump:") {
            let f = nsql_run::txlog::TxFilter {
                all_purposes: true,
                previous: true,
                ..Default::default()
            };
            let mut out = String::new();
            for e in self.txlog.entries(&f) {
                out.push_str(&format!(
                    "{:?}|{}|{}|{:?}|{:?}\n",
                    e.purpose,
                    nsql_run::txlog::one_line(&e.text, 80),
                    e.rows.map_or("-".to_string(), |r| r.to_string()),
                    e.result,
                    self.txlog.outcome_of(e)
                ));
            }
            out.push_str(&format!(
                "open_updates={}\n",
                self.txlog.open_record().map_or(0, |r| r.updates)
            ));
            let _ = std::fs::write(path, out);
            return;
        }
        // 자체 시험(09-26 성능 전수): 메모리 계측 표본을 파일로 — 총량·anon·부품 원장(L1 Meta · L2 MetaCols · L3 MetaDetail …).
        // 자체 시험(T-34): 라이선스 창 상태 덤프 · 파일 설치(파일 창과 같은 길).
        // 자체 시험(09-27 · 세션 자격 금고 재현): 열려 있는 비밀번호 창에 **저장된 프로필의 비밀번호**로 답한다(키 주입 없이 프롬프트 경로를
        //   그대로 지난다 · 값은 로그에 남지 않는다). `pw.answer_from_profile:<프로필>` · 창이 없으면 아무것도 안 함.
        if let Some(name) = id.strip_prefix("pw.answer_from_profile:") {
            if self.input_win.is_password() {
                let pw = Vault::open_default()
                    .ok()
                    .and_then(|v| v.resolve(name).ok().flatten())
                    .and_then(|spec| spec.password);
                match pw {
                    Some(p) => {
                        self.password_reply(worker::PwReply::Value(nsql_core::Secret::new(p)))
                    }
                    None => {
                        self.sess.status = format!("pw.answer_from_profile: no password in {name}")
                    }
                }
            }
            return;
        }
        // 자체 시험(09-27 · Linux IME 안내): `ime.fake:native|latin` = 감시가 준 것처럼 상태를 넣고 즉시 재판정 · `ime.dump:<파일>` = 안내 보임 여부.
        if let Some(mode) = id.strip_prefix("ime.fake:") {
            imestate::set_from_watch(mode == "native");
            self.input_win.ime_repoll();
            return;
        }
        if let Some(path) = id.strip_prefix("ime.dump:") {
            self.input_win.ime_repoll();
            let _ = std::fs::write(
                path,
                format!(
                    "visible={} state={:?}\n",
                    self.input_win.ime_hint_visible(),
                    imestate::current(None)
                ),
            );
            return;
        }
        if let Some(path) = id.strip_prefix("about.dump:") {
            let _ = std::fs::write(path, self.about_win.dump());
            return;
        }
        if let Some(path) = id.strip_prefix("license.dump:") {
            let view = self.license_view();
            let mut out = format!("badge={} warn={}\n", view.state, view.warn);
            for (k, v) in &view.rows {
                out.push_str(&format!("{k}={v}\n"));
            }
            out.push_str(&self.license_win.dump());
            let _ = std::fs::write(path, out);
            return;
        }
        if let Some(path) = id.strip_prefix("license.install:") {
            self.license_install(Path::new(path));
            return;
        }
        if let Some(path) = id.strip_prefix("mem.dump:") {
            let smp = self.mem_sample();
            let mut out = format!(
                "footprint={} resident={} anon={} sum={} other={}\n",
                smp.sys.footprint,
                smp.sys.resident,
                smp.sys.anon,
                smp.data.sum(),
                smp.other()
            );
            for c in memstat::Cat::ALL {
                out.push_str(&format!("{c:?}={}\n", smp.data.get(c)));
            }
            let _ = std::fs::write(path, out);
            return;
        }
        // 자체 시험(09-25): 서버 하나 더 접속(접속 창 Connect와 같은 길 = 공유 연결 추가) — `connect:<프로필|접속 문자열>`.
        if let Some(target) = id.strip_prefix("connect:") {
            let spec = if nsql_vault::is_profile_name(target) {
                Vault::open_default()
                    .ok()
                    .and_then(|v| v.get(target).ok().flatten())
            } else {
                nsql_drivers::parse_target(target, DEFAULT_DIALECT).ok()
            };
            if let Some(spec) = spec {
                self.attempt_queue.push_back(Attempt::Connect {
                    name: target.to_string(),
                    spec,
                    reconnect_same: false,
                });
                self.dispatch_attempts();
            }
            return;
        }
        // 진단(09-25 "북마크 니모닉 표시가 안 된다"): 탭마다 문서 열쇠 · 저장소 표식/라벨 수 · 텍스트박스에 걸린 수 · 설정.
        if let Some(path) = id.strip_prefix("bm.stat:") {
            let mut out = format!(
                "enabled={} gutter={} tabs={} store_docs={}\n",
                self.bookmarks.enabled,
                self.settings.flag("bookmark.gutter"),
                self.editors.len(),
                self.bookmarks.store.doc_keys_debug()
            );
            let (c, d) = (self.theme.accent, self.theme.text_dim);
            for i in 0..self.editors.len() {
                let key = bookmarks::Bookmarks::doc_key(&self.editors, i);
                let marks = self.bookmarks.marks_for(&self.editors, i, c, d).len();
                let labels = self.bookmarks.labels_for(&self.editors, i).len();
                let tb = self
                    .editors
                    .tab_box(i)
                    .map(|t| t.gutter_stat())
                    .unwrap_or((0, 0, false));
                out.push_str(&format!(
                    "{i}: title={:?} key={key:?} marks={marks} labels={labels} tb={tb:?}\n",
                    self.editors.tab_titles().get(i)
                ));
            }
            let _ = std::fs::write(path, out);
            return;
        }
        // 자체 시험(89 §3-3): Import 창 `import.open:<표>;<파일>` · `import.start` · `import.cancel` · `import.dump:<파일>`.
        if let Some(rest) = id.strip_prefix("import.open:") {
            if let Some((table, file)) = rest.split_once(';') {
                self.import_ctx = Some((table.to_string(), PathBuf::from(file), None));
                self.import_pending = true;
            }
            return;
        }
        if id == "import.start" {
            let spec = self.import_win.start_spec();
            self.import_start(spec);
            return;
        }
        if id == "import.cancel" {
            self.import_cancel();
            return;
        }
        if let Some(path) = id.strip_prefix("import.dump:") {
            let text = if self.import_win.is_open() {
                self.import_win.dump()
            } else {
                "closed\n".to_string()
            };
            let _ = std::fs::write(path, text);
            return;
        }
        if let Some(path) = id.strip_prefix("sqlprev.dump:") {
            let text = if self.sqlprev_win.is_open() {
                format!(
                    "{}\n---\n{}",
                    self.sqlprev_win.note_text(),
                    self.sqlprev_win.text()
                )
            } else {
                "closed".to_string()
            };
            let _ = std::fs::write(path, text);
            return;
        }
        // ★ 자체 시험(10-01 ㉗): 링크 이름으로 "객체 탐색기에서 보기" · 선택 경로 덤프(`explorer.select`와 접두가 겹치지 않게 `selpath`).
        if let Some(name) = id.strip_prefix("objlink.reveal:") {
            let ok = self.objlink_reveal_named(name);
            self.sess.status = format!("objlink.reveal {name} ok={ok}");
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("explorer.selpath:") {
            let _ = std::fs::write(path, self.explorer.selected_path());
            return;
        }
        // ★ 자체 시험(10-01 ㉕): DB 용량을 우클릭 없이 청한다(= 메뉴 "용량 확인"과 같은 길).
        if id == "explorer.dbsizes" {
            self.explorer.request_db_sizes();
            self.redraw();
            return;
        }
        if let Some(rest) = id.strip_prefix("explorer.menu") {
            let row = rest.trim_start_matches(':').parse().unwrap_or(0);
            let ok = self.explorer.capture_menu(row);
            self.sess.status = format!("explorer.menu row={row} opened={ok}");
            self.redraw();
            return;
        }
        if let Some(pick) = id.strip_prefix("explorer.pick:") {
            let ok = self.explorer.capture_pick(pick);
            if !ok {
                self.sess.status = format!("explorer.pick {pick}: menu not open");
            }
            self.redraw();
            return;
        }
        // 자체 시험용: 파일 검색 패널에 검색어를 넣고 실행(`search.run:<글>` · 제외 로그 캡처 09-23).
        if let Some(q) = id.strip_prefix("search.run:") {
            // `검색어|범위` — 범위 상자 글까지(쉼표는 명령 구분자라 범위 안에서는 `;`로 적고 여기서 콤마로 바꾼다).
            let (q, w) = match q.split_once('|') {
                Some((q, w)) => (q, Some(w.replace(';', ","))),
                None => (q, None),
            };
            self.search.run_query(q, w.as_deref());
            self.redraw();
            return;
        }
        // 자체 시험용: 활성 탭 이름을 바로 바꾼다(`tab.rename_to:<이름>` — 팔레트 입력 없이 · 북마크 패널 문서 이름 추종 캡처 09-23).
        if let Some(name) = id.strip_prefix("tab.rename_to:") {
            let i = self.editors.active();
            self.editors.rename_tab(i, name);
            self.redraw();
            return;
        }
        if id == "tab.drag_demo" {
            self.editors.capture_drag_demo();
            self.redraw();
            return;
        }
        if id == "tab.rename" {
            self.tab_menu_request(editors::TabMenuReq::Rename(self.editors.active()));
            self.redraw();
            return;
        }
        if id == "result.rename" {
            self.panel_action(ResultAction::Rename(self.panel.active));
            return;
        }
        if id == "result.menu" {
            self.panel.open_menu_for_capture();
            self.redraw();
            return;
        }
        if let Some(q) = id.strip_prefix("edit.prefs:") {
            self.prefs_query = Some(q.to_string());
            self.open_prefs = true;
            return;
        }
        // 자체 캡처용: 로그인 목록의 우클릭 메뉴를 `row`번째 줄에서 연다(`conn.menu[:n]` · 창 밖으로 안 잘리는지 모서리 캡처 · 61 §2-2 ④).
        if let Some(xy) = id.strip_prefix("conn.ime_hint:") {
            let mut it = xy.split('/').map(|v| v.trim().parse::<i32>().unwrap_or(0));
            let at = (it.next().unwrap_or(0), it.next().unwrap_or(0));
            self.conn_win.capture_ime_hint(at);
            return;
        }
        // (프로젝트 명령 `project.*`는 `menu_action` 앞머리에서 `project_cmd`로 — 메뉴바·팔레트와 같은 길.)
        // 자체 시험: 상태 팝업의 선택(`multi.open`/`multi.cancel` · `tx.*` · `disc.*` · `close.*`)은 팝업 픽 경로로(메뉴 동작이 아니다 ·
        //   09-22 기능 점검 S03 — `multi.open`이 `menu_action`으로 떨어져 아무 일도 안 했다).
        if ["multi.", "tx.", "disc.", "close."]
            .iter()
            .any(|p| id.starts_with(p))
        {
            self.indent_pick(id);
            self.redraw();
            return;
        }
        // 자체 캡처: 파일 대화상자 없이 다중 열기 확인 팝업(`file.open_many:<a>;<b>;…` · 쉼표는 기동 명령 구분자라 `;`).
        if let Some(rest) = id.strip_prefix("file.open_many:") {
            let paths: Vec<PathBuf> = rest
                .split(';')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .collect();
            self.multi_open_ask(paths, "auto".into());
            self.redraw();
            return;
        }
        if let Some(rest) = id.strip_prefix("conn.menu") {
            let row = rest.trim_start_matches(':').parse().unwrap_or(0);
            let ok = self.conn_win.capture_menu(row);
            self.sess.status = format!("conn.menu row={row} opened={ok}");
            self.conn_win.redraw();
            return;
        }
        if let Some(name) = id.strip_prefix("conn.edit:") {
            if let Ok(Some(spec)) = Vault::open_default().and_then(|v| v.get(name)) {
                self.conn_win.capture_open_detail(name);
                self.conn_win.panel.fill(name, &spec);
                self.conn_win.panel.capture_touch();
                self.conn_win.redraw();
            }
            return;
        }
        // `tx.manual`/`tx.auto` = 상태줄 Auto-commit 팝업의 선택과 같은 길(수동 커밋 실기를 입력 주입 없이 · `tx.log`는 창이라 아래로).
        if let Some(rest) = id
            .strip_prefix("tx.")
            .filter(|r| matches!(*r, "manual" | "auto"))
        {
            self.tx_pick(rest);
            self.redraw();
            return;
        }
        match id.strip_prefix("open:") {
            Some(path) => self.open_file(Path::new(path)),
            None => self.menu_action(id),
        }
    }
}
