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
        // 자체 시험(10-11): 설정 창 "변경" 보기 켜기/끄기 · 카드 키 덤프(`prefs.filter:changed|all` · `prefs.dump:<파일>`).
        if let Some(mode) = id.strip_prefix("prefs.filter:") {
            self.prefs_win.set_changed_only(mode == "changed");
            self.prefs_win.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("prefs.dump:") {
            let _ = std::fs::write(path, self.prefs_win.dump_text());
            return;
        }
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
                        | "down"
                        | "up"
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
                // `ui.down:x/y` · `ui.up:x/y` = 누름/놓음 분리(드래그 선택 = down → move ×n → up · 조건 바 캡처 시험 · 10-06).
                "down" => self.route(InputEvent::MouseDown {
                    x,
                    y,
                    shift: false,
                    primary: false,
                }),
                "up" => self.route(InputEvent::MouseUp { x, y }),
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
        // 설명 포함 토글(10-15 · E2E): `explorer.desc:on|off` = 설정 `explorer.search_desc`를 그 값으로(필터 틀 토글과 같은 길).
        if let Some(v) = id.strip_prefix("explorer.desc:") {
            let on = matches!(v.trim(), "on" | "true" | "1");
            let _ = self
                .settings
                .set("explorer.search_desc", if on { "on" } else { "off" });
            let _ = self.apply_setting("explorer.search_desc");
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("explorer.dump:") {
            let _ = std::fs::write(path, self.explorer.dump_rows());
            return;
        }
        // 서버 헤더 덤프(`explorer.header:<파일>` · `라벨|연결 N개 · 전용 M` 한 줄씩 · 10-13 전용 세션 E2E).
        if let Some(path) = id.strip_prefix("explorer.header:") {
            let _ = std::fs::write(path, self.explorer.dump_headers());
            return;
        }
        if let Some(rest) = id.strip_prefix("explorer.select") {
            // 행 번호 또는 라벨 경로(`explorer.expand`와 같은 풀이 · 10-11: 콜론 꼴 `explorer.select:3`이 조용히 무시되던 결함).
            let arg = rest.trim_start_matches(':');
            let ok = match arg.parse::<usize>() {
                Ok(row) => self.explorer.capture_select(row),
                Err(_) => self.explorer.capture_select_label(arg),
            };
            if std::env::var_os("NSQL_TRACE_META").is_some() {
                eprintln!(
                    "[detail] explorer.select {arg} ok={ok} target={:?}",
                    self.explorer.selected_target().map(|t| t.key())
                );
            }
            if ok {
                self.explorer_actions();
                self.redraw();
            }
            self.sess.status = format!("explorer.select row={arg} ok={ok}");
            return;
        }
        if let Some(path) = id.strip_prefix("details.dump:") {
            let _ = std::fs::write(path, self.objdetail.text());
            return;
        }
        // 자체 시험(Goto Anything 10-07): 팔레트 입력란 글 넣기 · 목록 덤프(`mode query rows sel` + 보이는 행 id\t라벨).
        if let Some(q) = id.strip_prefix("palette.query:") {
            self.palette.set_query(q);
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("palette.dump:") {
            let _ = std::fs::write(path, self.palette.dump());
            return;
        }
        // 활성 탭을 그 경로로 저장(자체 시험 · T-299 ① 색인 즉시 반영 확인 · 10-07).
        if let Some(p) = id.strip_prefix("file.save_as:") {
            self.save_to(std::path::Path::new(p));
            return;
        }
        // 탭 이동 기록 덤프(10-07 · 오래된 것 → 최근 · `id\t제목` 줄).
        if let Some(path) = id.strip_prefix("editor.tabhist:") {
            let out: String = self
                .editors
                .tab_history()
                .iter()
                .map(|(id, t)| format!("{id}\t{t}\n"))
                .collect();
            let _ = std::fs::write(path, out);
            return;
        }
        // 결과 n번째 행을 Enter처럼 고른다(협업 V1 "Ctrl+P 결과를 여는 기동 명령" · 키 주입 0).
        if let Some(n) = id.strip_prefix("palette.pick:") {
            let i = n.trim().parse::<usize>().unwrap_or(0);
            if let Some(pid) = self.palette.pick_row(i) {
                self.palette.close();
                self.ime_refresh();
                self.menu_action(&pid);
                self.redraw();
            }
            return;
        }
        // 자체 시험(T-283): 마지막 비교 결과 `same=… hunks=… name=…`.
        if let Some(path) = id.strip_prefix("compare.dump:") {
            self.compare_dump(path);
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
        // ★ 144차 자체 시험(10-27 · 클립보드를 덮지 않는다): `grid.copy:<메뉴 id>:<파일>` = 우클릭 메뉴 항목(`copy_h_tab` · `copy_hdr_comma` …)을
        //   고른 것과 같은 길로 복사 글을 만들어 **파일에** 쓴다(OS 클립보드 ✗ · 61 §2-4).
        if let Some(rest) = id.strip_prefix("grid.copy:") {
            if let Some((mid, path)) = rest.split_once(':') {
                self.grid.menu_pick(mid);
                let (text, n) = self.grid.take_copy().unwrap_or_default();
                let _ = std::fs::write(
                    path,
                    format!(
                        "id={mid}
cells={n}
{text}"
                    ),
                );
                self.sess.status = format!("grid.copy {mid} cells={n}");
            }
            self.redraw();
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
        // ★ 값 상태 3택(설계 B · 10-12): `cellview.null:<null|empty|value>` — 버튼과 같은 길 · `cellview.apply` = [셀에 반영].
        if let Some(k) = id.strip_prefix("cellview.null:") {
            let k = match k {
                "null" => sqlprev_win::Nullness::Null,
                "empty" => sqlprev_win::Nullness::Empty,
                _ => sqlprev_win::Nullness::Value,
            };
            self.sqlprev_win.set_nullness(k);
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
        // 자체 시험(T-181 필터 줄): `grid.addfilter:<열 번호>;<연산 코드>;<값>` = 키·마우스 없이 술어 추가 ·
        // `grid.rmfilter:<열 번호>` = 칩의 ×와 같은 길 · `grid.chips:<파일>` = 필터 줄 칩 글 덤프(줄이 없으면 빈 파일).
        if let Some(rest) = id.strip_prefix("grid.addfilter:") {
            let mut it = rest.splitn(3, ';');
            if let (Some(c), Some(op)) = (it.next(), it.next()) {
                if let (Ok(c), Some(op)) =
                    (c.trim().parse::<usize>(), grid::FilterOp::parse(op.trim()))
                {
                    self.grid
                        .add_filter(c, op, it.next().unwrap_or_default().to_string());
                    // 칩 ×·메뉴 길과 같게 요청(채움 페치 · 승격)을 바로 걷는다.
                    self.after_grid_event();
                    self.redraw();
                }
            }
            return;
        }
        if let Some(c) = id
            .strip_prefix("grid.rmfilter:")
            .and_then(|c| c.trim().parse::<usize>().ok())
        {
            self.grid.remove_filter(c);
            self.after_grid_event();
            self.redraw();
            return;
        }
        // 자체 시험(T-181 후속 · 값 목록 팝업): `grid.funnel:<열 번호>` = 깔때기 클릭 · `grid.vpick:<글>` = 검색 상자 글 ·
        // `grid.vpick.toggle:<n>` = 보이는 n번째 값 토글 · `grid.vpick.apply` = [적용] · `grid.vpick.dump:<파일>` = 팝업 상태 덤프.
        // `grid.fetch_all` = 도구줄 ⇊(나머지 이어 받기) · `grid.sort:<열>` = 열 머리 클릭 정렬 토글 — 자체 시험(증분 표시 V1 · 10-09).
        if id == "grid.fetch_all" {
            self.grid.request_fetch_all();
            self.redraw();
            return;
        }
        if let Some(c) = id
            .strip_prefix("grid.sort:")
            .and_then(|c| c.trim().parse::<usize>().ok())
        {
            self.grid.sort_by_col_cmd(c);
            self.redraw();
            return;
        }
        // `grid.page:<n>` = 활성 결과 탭의 세그먼트 크기(푸터 입력란과 같은 탭별 값 · 0 = 전체) — 자체 시험(10-09 "행 수 0 → 중지 → 재조회 200행").
        if let Some(n) = id
            .strip_prefix("grid.page:")
            .and_then(|c| c.trim().parse::<usize>().ok())
        {
            // ★ 첫 그리기 전에 들어온 훅은 옛 탭 그리드에 쓰인다(127차 V3 · Debug 첫 그리기 1.57 s > 1500 ms) → run_text_in과
            //   같은 규칙으로 활성 탭의 그리드를 먼저 맞춘다(10-15).
            self.sync_grid_tab();
            self.grid.set_page_rows(n);
            self.redraw();
            return;
        }
        if let Some(c) = id
            .strip_prefix("grid.funnel:")
            .and_then(|c| c.trim().parse::<usize>().ok())
        {
            self.set_focus(Focus::Grid);
            self.grid.open_value_pick(c);
            self.ime_refresh();
            self.redraw();
            return;
        }
        if let Some(text) = id.strip_prefix("grid.vpick:") {
            self.grid.value_pick_mut().set_search(text);
            self.redraw();
            return;
        }
        if let Some(n) = id
            .strip_prefix("grid.vpick.toggle:")
            .and_then(|n| n.trim().parse::<usize>().ok())
        {
            self.grid.value_pick_mut().toggle(n);
            self.redraw();
            return;
        }
        // 자체 시험(인라인 조건 입력란): `grid.cond:<글>` = 상자 글 · `grid.cond.run` = Enter · `grid.cond.dump:<파일>`.
        if let Some(text) = id.strip_prefix("grid.cond:") {
            self.grid.cond_set_text(text);
            self.redraw();
            return;
        }
        if id == "grid.cond.run" {
            self.grid.cond_run_now();
            self.after_grid_event();
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("grid.cond.dump:") {
            let _ = std::fs::write(path, self.grid.cond_dump());
            return;
        }
        if id == "grid.vpick.all" {
            self.grid.value_pick_mut().toggle_all_visible();
            self.redraw();
            return;
        }
        if id == "grid.vpick.add" {
            self.grid.value_pick_mut().toggle_add_to_filter();
            self.redraw();
            return;
        }
        if id == "grid.vpick.apply" {
            self.grid.value_pick_apply();
            self.after_grid_event();
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("grid.vpick.dump:") {
            let _ = std::fs::write(path, self.grid.value_pick_mut().dump());
            return;
        }
        // 자체 시험(T-180 ⑤): `grid.reveal:<열 이름>` = 열 머리 메뉴 "객체 탐색기에서 보기"와 같은 길(출처 테이블 + 열).
        if let Some(col) = id.strip_prefix("grid.reveal:") {
            if let Some(table) = self.grid.reveal_table() {
                self.reveal_table_member(&table, Some(col.trim().to_string()));
            }
            return;
        }
        // 자체 시험(T-178 시그니처 카드): `intel.sigcard:<파일>` = 카드 글(없으면 빈 파일) · `intel.sighelp` = 지금 캐럿에서 판정.
        if id == "intel.sighelp" {
            self.intel_signature_help();
            return;
        }
        if let Some(path) = id.strip_prefix("intel.sigcard:") {
            let _ = std::fs::write(
                path,
                self.sig_card_tip().map(|(t, _)| t).unwrap_or_default(),
            );
            return;
        }
        // 자체 시험(T-181): `grid.filteror:on|off` = 필터 결합 AND/OR.
        if let Some(v) = id.strip_prefix("grid.filteror:") {
            self.grid
                .set_filter_or(!matches!(v.trim(), "off" | "0" | "false"));
            self.redraw();
            return;
        }
        // 자체 시험(T-181): `grid.requery` = 필터 메뉴 "필터로 서버 재조회"와 같은 길.
        if id == "grid.requery" {
            if let Some(sql) = self.grid.filter_query() {
                self.run_in_fresh_tab(sql);
            }
            return;
        }
        // 자체 시험(T-180 ⑥): `grid.fksync` = 외래 키 열 맞춤 · `grid.follow:<원본 행>;<열>` = 참조 행 보기.
        if id == "grid.fksync" {
            self.grid_fk_sync();
            return;
        }
        if let Some(rest) = id.strip_prefix("grid.follow:") {
            if let Some((r, c)) = rest.split_once(';') {
                if let Ok(r) = r.trim().parse::<usize>() {
                    self.grid_fk_sync();
                    self.follow_fk(r, c.trim());
                }
            }
            return;
        }
        if let Some(path) = id.strip_prefix("grid.chips:") {
            let _ = std::fs::write(path, self.grid.dump_chips());
            return;
        }
        // 자체 시험(10-11 페치 일관성 E2E): 원본 세트의 한 열 값 전부 → 파일(`grid.col:<파일>|<열 이름>`).
        if let Some(rest) = id.strip_prefix("grid.col:") {
            if let Some((path, col)) = rest.split_once('|') {
                let _ = std::fs::write(path, self.grid.dump_column(col).unwrap_or_default());
            }
            return;
        }
        // 자체 시험(10-12 스크롤 점검): 썸 중심 좌표 `grid.thumbs:<파일>`(`v=x,y` · `h=x,y` · 없으면 `-`) · 휠 사건 주입
        //   `ui.wheel:x/y/delta` · `ui.hwheel:x/y/delta`(창 좌표 · 커서를 그 자리로 옮긴 뒤 휠 = 커서 아래 영역 라우팅 그대로).
        if let Some(path) = id.strip_prefix("grid.thumbs:") {
            let scale = self.scale;
            let v = self.grid.test_vthumb_center(scale);
            let h = self.grid.test_hthumb_center(scale);
            let f = |p: Option<(i32, i32)>| p.map_or("-".to_string(), |(x, y)| format!("{x},{y}"));
            let b = self.grid.outer_bounds();
            let _ = std::fs::write(
                path,
                format!(
                    "v={}\nh={}\nbounds={},{},{},{}\nscroll_x={}\n",
                    f(v),
                    f(h),
                    b.x,
                    b.y,
                    b.w,
                    b.h,
                    self.grid.scroll_x_for_test()
                ),
            );
            return;
        }
        if let Some((kind, rest)) = id
            .strip_prefix("ui.")
            .and_then(|r| r.split_once(':'))
            .filter(|(k, _)| matches!(*k, "wheel" | "hwheel"))
        {
            let mut it = rest.split('/');
            let x: i32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let y: i32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            let d: i32 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            self.route(InputEvent::MouseMove { x, y });
            let ev = if kind == "wheel" {
                InputEvent::Wheel { delta: d }
            } else {
                InputEvent::HWheel { delta: d }
            };
            self.route(ev);
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("grid.dump:") {
            let _ = std::fs::write(path, self.grid.dump_edit());
            return;
        }
        // ★ 자체 시험(10-10 118차 mac 크래시 회귀 · 기능 점검 S95): `grid.vdrag:<dy>` = 활성 그리드 세로 스크롤바 **썸을 잡고** dy(장치 px ·
        //   음수 = 위)만큼 끌어 놓는다 — 좌표는 그리드가 셈하므로 OS·배율·창 크기와 무관 · 사건은 `ui.*`와 같은 `route` 경로(OS 입력 주입 아님).
        //   내용 높이가 수천만 px(수십만 행)이면 종전 nexa-ctl `scroll.rs`의 i32 곱이 넘쳐 Debug = abort · Release = 감김이었다 →
        //   뒤이은 `grid.dump`의 `scroll_end=true`로 판정한다.
        if let Some(dy) = id
            .strip_prefix("grid.vdrag:")
            .and_then(|v| v.trim().parse::<i32>().ok())
        {
            let scale = self.scale;
            if let Some((x, y)) = self.grid.test_vthumb_center(scale) {
                self.cursor = (x, y);
                self.route(InputEvent::MouseMove { x, y });
                self.route(InputEvent::MouseDown {
                    x,
                    y,
                    shift: false,
                    primary: false,
                });
                self.route(InputEvent::MouseMove { x, y: y + dy });
                self.route(InputEvent::MouseUp { x, y: y + dy });
                self.redraw();
            } else {
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    "[test] grid.vdrag: no result grid or no vertical scrollbar".to_string(),
                ));
            }
            return;
        }
        // 자체 시험(10-01 ㉘): 활성 패널의 결과 탭 상태 — 첫 줄 `active=<제목>|output=<Output 탭인가>|rows=<활성 그리드 행 수>` · 둘째 줄부터 탭 제목.
        // 자체 시험(10-10 · D-272 E2E `scripts/define-e2e.sh`): 변수 입력 창 상태 — 열렸는가 · 묻는 이름들.
        if let Some(path) = id.strip_prefix("input.dump:") {
            let _ = std::fs::write(path, self.input_win.dump_text());
            return;
        }
        if let Some(path) = id.strip_prefix("result.dump:") {
            let mut out = format!(
                "active={}|output={}|rows={}\n",
                self.panel
                    .tabs
                    .get(self.panel.active)
                    .map(|t| t.title.as_str())
                    .unwrap_or(""),
                self.panel.output_active(),
                self.grid.row_count()
            );
            for t in &self.panel.tabs {
                out.push_str(&format!("{}|{}\n", t.title, t.is_output));
            }
            let _ = std::fs::write(path, out);
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
            // 머리 = 제목|종류|읽기전용|구문|caret=줄:열(1부터 · 10-16 미리보기 줄 이동 판정 · 열 = 131차 T-371 오류 위치 판정 ·
            //   종전 `caret=N` 접두 매치와 호환)(10-07 · 모르는 확장자 = Plain Text 판정용).
            let (caret_line, caret_col, sel) = {
                let tb = self.editors.cur();
                let c = tb.caret();
                let l = tb.buf().line_of(c);
                // 선택 = `sel=시작-끝`(글자 인덱스 · 없으면 `-`) — 찾기 이동 뒤 일치가 선택돼 있는지 판정(131차 ⑪).
                let sel = tb
                    .selection()
                    .filter(|(a, b)| b > a)
                    .map_or("-".to_string(), |(a, b)| format!("{a}-{b}"));
                (l + 1, c - tb.buf().line_start(l) + 1, sel)
            };
            let head = format!(
                "{}|{:?}|{}|{}|caret={}:{}|sel={}
",
                self.editors.title_of(i),
                self.editors.tab_kind(i),
                self.editors.active_read_only(),
                self.editors.syntax_name(),
                caret_line,
                caret_col,
                sel
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
        // 툴바 우클릭 메뉴(사용자 10-04 · 자체 시험): `toolbar.menu:<버튼 id>` = 그 버튼 가운데에서 메뉴 열기 ·
        //   `toolbar.menudump:<경로>` = 열린 메뉴의 항목 id(구분선 = 빈 줄).
        if let Some(item) = id.strip_prefix("toolbar.menu:") {
            if let Some(r) = self.tool_dock.item_rect(item) {
                self.open_toolbar_menu(r.x + r.w / 2, r.y + r.h / 2);
                self.redraw();
            }
            return;
        }
        // 상태바 우클릭 메뉴(자체 시험): `statusbar.menu:<칸 id>` = 그 칸 가운데에서(없는 id = 띠 왼쪽 빈 자리) · 덤프는 `toolbar.menudump`.
        if let Some(seg) = id.strip_prefix("statusbar.menu:") {
            let b = self.status_bar_rect;
            let (x, y) = self
                .status_seg_rects
                .iter()
                .find(|(i, _)| *i == seg)
                .map_or((b.x + 4, b.y + b.h / 2), |(_, r)| {
                    (r.x + r.w / 2, r.y + r.h / 2)
                });
            self.open_statusbar_menu(x, y);
            self.redraw();
            return;
        }
        // 열린 툴바/상태바 메뉴의 항목 확정(사용자 경로와 같은 함수 · 자체 시험) — `toolbar.pick:<항목 id>`.
        if let Some(item) = id.strip_prefix("toolbar.pick:") {
            self.status_menu.close();
            self.indent_pick(item);
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("toolbar.menudump:") {
            let _ = std::fs::write(path, self.status_menu.item_ids().join("\n") + "\n");
            return;
        }
        // 순서/표시 편집 창(사용자 10-04 · 자체 시험): 열기 · 선택 · 이동 · 표시 전환 · 기본값 · 덤프.
        if let Some(key) = id.strip_prefix("order.open:") {
            self.open_order_editor(key);
            return;
        }
        if let Some(arg) = id.strip_prefix("order.cmd:") {
            let a = if let Some(n) = arg.strip_prefix("select=") {
                self.order_win.select(n.parse().unwrap_or(0));
                crate::order_win::OrderWinAction::None
            } else if let Some(name) = arg.strip_prefix("add=") {
                // 자유 항목 모드(제외 폴더): 이름 추가 · `del` = 선택 행 삭제.
                self.order_win.add_item(name)
            } else {
                match arg {
                    "del" => self.order_win.remove_sel(),
                    "up" => self.order_win.move_sel(false),
                    "down" => self.order_win.move_sel(true),
                    "toggle" => {
                        let i = self.order_win.selected();
                        self.order_win.toggle(i)
                    }
                    "reset" => self.order_win.reset_all(),
                    _ => crate::order_win::OrderWinAction::None,
                }
            };
            if let crate::order_win::OrderWinAction::Changed { key, value } = a {
                self.order_changed(key, &value);
            }
            return;
        }
        if let Some(path) = id.strip_prefix("order.dump:") {
            // `setting` 줄 = 지금 편집 중인 키의 값(툴바는 가상 키라 지금 상태에서 만든다).
            let cur = match self.order_win.key() {
                Some(k) if k == super::toolbar::TOOLBAR_ORDER_KEY => self.toolbar_order_value(),
                Some(k) => self.settings.get(k).unwrap_or("").to_string(),
                None => String::new(),
            };
            let out = format!("{}setting {cur}\n", self.order_win.dump());
            let _ = std::fs::write(path, out);
            return;
        }
        // ★ 자체 시험 훅(10-07 · 전체 시험 시나리오 S80~): 외부 변경·삭제 흉내 · 힙 정리 · 탭 상태 덤프 · 확장 뷰 탭 · 외부 변경 띠 버튼.
        if let Some(rest) = id.strip_prefix("fs.write:") {
            // `fs.write:<경로>|<글>` — `\n`은 줄 바꿈(기동 명령은 한 줄).
            if let Some((path, text)) = rest.split_once('|') {
                let _ = std::fs::write(path, text.replace("\\n", "\n"));
            }
            return;
        }
        if let Some(path) = id.strip_prefix("fs.delete:") {
            let _ = std::fs::remove_file(path);
            return;
        }
        if id == "mem.trim" {
            self.mem_trim_now();
            return;
        }
        // 찾기 틀 Esc(취소 = 열 때 자리 복귀)·×(닫기) 길(131차 · E2E `win-find-origin-e2e.sh` · 키 주입 없이).
        if id == "find.cancel" {
            self.find_action(crate::findbar::FindAction::Cancel);
            return;
        }
        if id == "find.close" {
            self.find_action(crate::findbar::FindAction::Close);
            return;
        }
        if let Some(path) = id.strip_prefix("tabs.dump:") {
            // 한 줄 = 제목|종류|미저장|삭제됨|뷰 탭|읽기 전용 · 활성 탭은 머리에 `*`.
            let mut out = String::new();
            let active = self.editors.active();
            for i in 0..self.editors.tab_count() {
                let (unsaved, deleted, view, ro) = self.editors.tab_flags(i);
                out.push_str(&format!(
                    "{}{}|{:?}|unsaved={unsaved}|deleted={deleted}|view={view}|ro={ro}\n",
                    if i == active { "*" } else { "" },
                    self.editors.title_of(i),
                    self.editors.tab_kind(i)
                ));
            }
            let _ = std::fs::write(path, out);
            return;
        }
        if let Some(ext_id) = id.strip_prefix("ext.view:") {
            if !self.ext_reopen_view(ext_id) {
                self.sess.status = format!("ext.view: {ext_id} not installed");
            }
            self.redraw();
            return;
        }
        if let Some(which) = id.strip_prefix("ext.banner:") {
            let hit = match which.trim() {
                "diff" => extfile::BannerHit::Diff,
                "reload" => extfile::BannerHit::Reload,
                "keep" => extfile::BannerHit::Keep,
                "follow" => extfile::BannerHit::Follow,
                "dismiss" => extfile::BannerHit::Dismiss,
                "overwrite" => extfile::BannerHit::Overwrite,
                _ => extfile::BannerHit::None,
            };
            self.ext_banner_pick(hit);
            self.redraw();
            return;
        }
        // 자체 시험(10-07): 프로젝트 탐색기 플래시 상태 + 향상 모드 판정(`perf.boost` 원값) — 협업 bin64 ⓒ 진단.
        if let Some(path) = id.strip_prefix("flash.dump:") {
            let out = format!(
                "{} boost_on={} perf.boost={:?}\n",
                self.project_panel.flash_dump(),
                self.settings.boost_on(),
                self.settings.get("perf.boost")
            );
            let _ = std::fs::write(path, out);
            return;
        }
        if let Some(path) = id.strip_prefix("mem.dump:") {
            let smp = self.mem_sample();
            let mut out = format!(
                "footprint={} resident={} anon={} private_ws={} sum={} other={}\n",
                smp.sys.footprint,
                smp.sys.resident,
                smp.sys.anon,
                smp.sys.private_ws,
                smp.data.sum(),
                smp.other()
            );
            for g in memstat::Group::ALL {
                // 런타임 묶음 = 기타(총량 − 집계 합) — 화면 소계와 같은 값(협업 bin63 관찰).
                let sum = if g == memstat::Group::Runtime {
                    smp.other()
                } else {
                    smp.data.group_sum(g)
                };
                out.push_str(&format!("group {g:?}={sum}\n"));
            }
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
        // ★ 객체 찾기 창 자체 시험(113 §7): `findobj.open` · `findobj.query:[cwr:]<글>` · `findobj.dump:<파일>` · `findobj.pick:<n>` · `findobj.close`.
        if id == "findobj.open" {
            self.request_findobj(None);
            return;
        }
        if id == "findobj.close" {
            self.findobj_action(crate::findobj_win::FoAction::Close);
            return;
        }
        // 닫기 **요청**(X·[닫기]와 같은 길 · 검색 중 = 확인 메뉴 · 10-16) · 상태줄 `confirm=<열림>`.
        if id == "findobj.closereq" {
            let a = self.findobj_win.close_requested();
            self.findobj_action(a);
            let open = self.findobj_win.close_confirm_open();
            self.sess.status = format!(
                "findobj.closereq confirm={open} open={}",
                self.findobj_win.is_open()
            );
            return;
        }
        if let Some(q) = id.strip_prefix("findobj.query:") {
            if !self.findobj_win.is_open() {
                self.sess.status = "findobj.query: window not open".into();
                return;
            }
            let act = self.findobj_win.test_query(q);
            self.findobj_action(act);
            return;
        }
        if let Some(path) = id.strip_prefix("findobj.dump:") {
            // 끝 줄 = 메모리·스레드(10-16): `mem=<바이트>|threads=<엔진 스레드 수>|open=<창>`.
            let text = format!(
                "{}mem={}|threads={}|open={}\n",
                self.findobj_win.dump(),
                self.findobj_win.approx_bytes(),
                self.findobj_engine.threads_alive(),
                self.findobj_win.is_open()
            );
            let _ = std::fs::write(path, text);
            return;
        }
        // ★ 미리보기 반응 계측(10-16 사용자 "컨트롤이 받는 사건만 재면 맹점"): 클릭은 **OS 사건과 같은 길**(`findobj_window_event` =
        //   CursorMoved + Pressed + Released)로 합성해 창 안 모든 분기(스위치·팝업·서버 목록·스플리터·머리·목록)를 지나고(`consumed=`로 누가
        //   먹었는지 판정) · 키는 ↑/↓ 분기 함수(KeyEvent는 밖에서 못 만든다) · 선택만 `findobj.sel:<n>` · 소스 있는 첫 행 `findobj.selkind:<종류>`.
        //   좌표 구분자는 `/`(쉼표 = 명령 구분자). 계측 로그 = `[preview] …`(`log.dump`).
        if let Some(n) = id
            .strip_prefix("findobj.sel:")
            .and_then(|n| n.trim().parse::<usize>().ok())
        {
            let ok = self.findobj_win.test_select(n);
            self.sess.status = format!("findobj.sel {n} ok={ok}");
            return;
        }
        if let Some(k) = id.strip_prefix("findobj.selkind:") {
            // `종류[:최소 소스 줄 수]` — ↓ 연타 판정에는 일치 줄이 넉넉한 행이 필요(협업 16차: 일치 2줄 뷰 = 두 번째 ↓가 못 움직임).
            let (kind, min_src) = match k.split_once(':') {
                Some((a, n)) => (a.trim(), n.trim().parse::<usize>().unwrap_or(1)),
                None => (k.trim(), 1),
            };
            let r = self.findobj_win.test_select_kind(kind, min_src);
            self.sess.status = format!("findobj.selkind {k} row={r:?}");
            return;
        }
        if let Some(k) = id.strip_prefix("findobj.clickpre:") {
            match self.findobj_win.test_pre_line_point(k) {
                Some((x, y, no)) => {
                    self.findobj_synth_click(x, y);
                    self.sess.status = format!("findobj.clickpre {k} at {x}/{y} line={no}");
                }
                None => self.sess.status = format!("findobj.clickpre {k} = no such line"),
            }
            return;
        }
        if let Some(n) = id
            .strip_prefix("findobj.clickrow:")
            .and_then(|n| n.trim().parse::<usize>().ok())
        {
            match self.findobj_win.test_row_point(n) {
                Some((x, y)) => {
                    self.findobj_synth_click(x, y);
                    self.sess.status = format!("findobj.clickrow {n} at {x}/{y}");
                }
                None => self.sess.status = format!("findobj.clickrow {n} = no such row"),
            }
            return;
        }
        if let Some((x, y)) = id
            .strip_prefix("findobj.click:")
            .and_then(|xy| xy.split_once('/'))
            .and_then(|(x, y)| Some((x.trim().parse::<i32>().ok()?, y.trim().parse::<i32>().ok()?)))
        {
            self.findobj_synth_click(x, y);
            self.sess.status = format!("findobj.click {x}/{y}");
            return;
        }
        if let Some(d) = id.strip_prefix("findobj.key:") {
            let down = d.trim().eq_ignore_ascii_case("down");
            let no = self.findobj_synth_key(down);
            self.sess.status = format!("findobj.key {d} line={no:?}");
            return;
        }
        // 자체 시험(10-15): 상태줄 글 덤프(`status.dump:<파일>` · 기동 명령들이 상태줄에만 남기는 판정 결과를 파일로).
        if let Some(path) = id.strip_prefix("status.dump:") {
            let _ = std::fs::write(path, &self.sess.status);
            return;
        }
        if let Some(path) = id.strip_prefix("tx.dump:") {
            let _ = std::fs::write(path, self.tx_dump());
            return;
        }
        // 자체 시험(10-24): 설정 내보내기/가져오기 = 파일 창 없이 같은 길(`settings.export:<파일>` · `settings.import:<파일>`).
        if let Some(path) = id.strip_prefix("settings.export:") {
            self.settings_export_to(Path::new(path));
            return;
        }
        if let Some(path) = id.strip_prefix("settings.import:") {
            self.settings_import_from(Path::new(path));
            return;
        }
        if let Some(path) = id.strip_prefix("objbk.dump:") {
            let _ = std::fs::write(path, self.objbk_dump());
            return;
        }
        if let Some(f) = id.strip_prefix("findobj.filter:") {
            self.findobj_win.test_filter(f);
            return;
        }
        if let Some(f) = id.strip_prefix("findobj.servers:") {
            self.findobj_win.test_servers(f);
            return;
        }
        if let Some(n) = id
            .strip_prefix("findobj.pick:")
            .and_then(|n| n.trim().parse::<usize>().ok())
        {
            let act = self.findobj_win.test_pick(n);
            self.findobj_action(act);
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
        // ★ 자체 시험(㉗-k · 메뉴 경로): `objlink.menu:<이름>`(우클릭처럼 열기) · `objlink.resync`(열린 채 재분석 강제) · `objlink.pick:<id>`(항목 확정).
        // 자체 시험(10-12 ④ 재질의 확인): 토스트 덤프 · 버튼 누름(키 주입 없이 실제 동작 id 경로).
        if let Some(path) = id.strip_prefix("toast.dump:") {
            let _ = std::fs::write(path, self.toasts.dump_text());
            return;
        }
        // ★ 자체 시험(10-27 드라이버 갱신 알림): 기동 갱신 확인을 나이·설치 여부와 무관하게 지금(네트워크 1 GET · 격리 홈에서만 쓴다).
        if id == "drivers.check_updates" {
            self.drivers_check_updates_now();
            return;
        }
        // ★ 자체 시험(docs/111 · T-339): Ctrl 홀드 흉내(`ui.key:ctrl_down`/`ctrl_up` = 수식키 사건과 같은 길) · 줄 끝 라벨 덤프.
        if let Some(what) = id.strip_prefix("ui.key:") {
            match what {
                "ctrl_down" | "ctrl_up" => {
                    self.primary = what == "ctrl_down";
                    self.objlink_sync();
                    self.inline_key.clear();
                    self.inline_sync();
                    self.redraw();
                }
                // ★ 키 주입(10-13 · 메뉴 열린 채 키 우선권 V1): `ui.key:down|up|left|right|enter|escape|tab|home|end|pageup|pagedown|
                //   backspace|delete|space` · `ui.key:char:<글자>` · 접두 `shift+`(예 `shift+enter`) — OS 입력이 아니라 앱 안 `route`.
                _ => {
                    let (shift, name) = what
                        .strip_prefix("shift+")
                        .map_or((false, what), |n| (true, n));
                    let now_ms = self.started.elapsed().as_millis() as u64;
                    let key = |k: CtlKey| InputEvent::Key {
                        key: k,
                        shift,
                        primary: false,
                    };
                    let ev = match name {
                        "down" => Some(key(CtlKey::Down)),
                        "up" => Some(key(CtlKey::Up)),
                        "left" => Some(key(CtlKey::Left)),
                        "right" => Some(key(CtlKey::Right)),
                        "enter" => Some(key(CtlKey::Enter)),
                        "escape" | "esc" => Some(key(CtlKey::Escape)),
                        "home" => Some(key(CtlKey::Home)),
                        "end" => Some(key(CtlKey::End)),
                        "pageup" => Some(key(CtlKey::PageUp)),
                        "pagedown" => Some(key(CtlKey::PageDown)),
                        "delete" => Some(key(CtlKey::Delete)),
                        "space" => Some(key(CtlKey::Space)),
                        "tab" => Some(InputEvent::Char { c: '\t', now_ms }),
                        "backspace" => Some(InputEvent::Char { c: '\u{8}', now_ms }),
                        other => other
                            .strip_prefix("char:")
                            .and_then(|c| c.chars().next())
                            .map(|c| InputEvent::Char { c, now_ms }),
                    };
                    if let Some(ev) = ev {
                        self.route(ev);
                        self.redraw();
                    }
                }
            }
            return;
        }
        if let Some(path) = id.strip_prefix("editor.inline_dump:") {
            let _ = std::fs::write(path, self.inline_dump_text());
            return;
        }
        // ★ 자체 시험(10-13 · Thin 실시간 전환 V1): `config.set:<키>=<값>` = 설정 창에서 값을 바꾼 것과 같은 길(저장 · `apply_setting` ·
        //   설정 창 갱신) — 설정 창은 별도 창이라 `ui.*` 좌표 명령이 못 닿는다. 빈 값 = 기본값으로.
        if let Some(kv) = id.strip_prefix("config.set:") {
            if let Some((k, v)) = kv.split_once('=') {
                let r = if v.is_empty() {
                    self.settings.reset(k).map(|_| ())
                } else {
                    self.settings.set(k, v).map(|_| ())
                };
                match r {
                    Ok(()) => {
                        self.persist_settings();
                        let _ = self.apply_setting(k);
                        self.prefs_win.refresh(&self.settings);
                        self.log_win.push(LogEntry::new(
                            LogKind::Info,
                            format!("[test] config.set {k} = {v}"),
                        ));
                    }
                    Err(e) => self.log_win.push(LogEntry::new(
                        LogKind::Info,
                        format!("[test] config.set {k} failed: {e}"),
                    )),
                }
            }
            self.redraw();
            return;
        }
        if id == "toast.action" {
            if let Some(a) = self.toasts.test_press_button() {
                self.menu_action(&a);
            }
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("log.dump:") {
            let _ = std::fs::write(path, self.log_win.dump_text());
            return;
        }
        // 시각 붙은 로그 덤프(중지 지연 측정 · 10-09).
        if let Some(path) = id.strip_prefix("log.dump_t:") {
            let _ = std::fs::write(path, self.log_win.dump_text_timed());
            return;
        }
        if let Some(name) = id.strip_prefix("objlink.menu:") {
            let ok = self.objlink_menu_named(name);
            self.sess.status = format!("objlink.menu {name} ok={ok}");
            self.redraw();
            return;
        }
        if id == "objlink.resync" {
            self.objlink_resync_forced();
            self.redraw();
            return;
        }
        if let Some(item) = id.strip_prefix("objlink.pick:") {
            if self.objlink_menu.is_open() {
                self.objlink_menu.close();
                self.objlink_menu_pick(item);
            }
            self.redraw();
            return;
        }
        if let Some(path) = id.strip_prefix("objlink.dump:") {
            let text = self.objlink_dump_text();
            let _ = std::fs::write(path, text);
            return;
        }
        // ★ 자체 시험(docs/107 ② · T-313): 끝점 끊김/복귀 주입 · 레지스트리 덤프(키 주입·네트워크 조작 없이 전파를 본다).
        if let Some(ep) = id.strip_prefix("net.break:") {
            self.health_event(ep, app::health::HealthEvent::Down("test".into()));
            return;
        }
        // 시험 훅(Debug 전용): 다음 ■ 한 번은 드라이버 취소를 보내지 않는다(강제 중지 경로 · 협업 10-09).
        if id == "run.stop_mute" {
            if cfg!(debug_assertions) {
                self.sess.cancel_mute = self.sess.cancel_mute.saturating_add(1);
            }
            return;
        }
        // `sess.kill` = 활성 탭 세션의 워커 접속을 실제로 떨어뜨림(Debug · 자동 재접속 경로 시험 · 10-09).
        if id == "sess.kill" {
            if cfg!(debug_assertions) {
                self.sess.control(crate::worker::Cmd::KillSession);
            }
            return;
        }
        if id == "net.changed" {
            // L0 신호 모의 = 실제 처리 함수 그대로(끝점 전부 Suspect + 메타 NetChanged + 다음 실행 preflight).
            self.net_last = None;
            self.net_changed();
            return;
        }
        if let Some(ep) = id.strip_prefix("net.suspect:") {
            // L0 OS 신호 모의(⑤ 전까지는 시험 훅만) — Alive → Suspect(알림 없음 · 다음 동작이 판정).
            self.health_event(ep, app::health::HealthEvent::NetChanged);
            return;
        }
        if let Some(ep) = id.strip_prefix("net.heal:") {
            self.health_event(ep, app::health::HealthEvent::Up);
            return;
        }
        if let Some(path) = id.strip_prefix("health.dump:") {
            let _ = std::fs::write(path, self.health_dump_text());
            return;
        }
        if let Some(path) = id.strip_prefix("explorer.selpath:") {
            let _ = std::fs::write(path, self.explorer.selected_path());
            return;
        }
        // ★ 174차 자체 시험(T-14 · 키 주입 0): `keymap.dump:<경로>` = 키맵 파일 요약 · context 행 · context 키의 지금 값 ·
        //   `keymap.try:<조합[;둘째]>@<경로>` = 그 조합이 지금 어느 명령에 걸리는지(실행 안 함 · 덧붙여 씀).
        if let Some(path) = id.strip_prefix("keymap.dump:") {
            let _ = std::fs::write(path, self.keymap_dump());
            return;
        }
        if let Some((chords, path)) = id
            .strip_prefix("keymap.try:")
            .and_then(|r| r.split_once('@'))
        {
            let line = self.keymap_try(chords);
            let prev = std::fs::read_to_string(path).unwrap_or_default();
            let _ = std::fs::write(path, prev + &line);
            return;
        }
        // ★ 자체 시험(10-01 ㉗-j): 활성 탭에 파일 내용을 붙여 넣는다(사용자 재현 순서 "새 편집창 → 쿼리 붙여넣기" · 키 주입 0).
        if let Some(path) = id.strip_prefix("editor.paste:") {
            if let Ok(text) = std::fs::read_to_string(path) {
                let mut inv = nexa_ctl::Invalidations::default();
                self.editors.cur_mut().paste(&text, &mut inv);
                self.redraw();
            }
            return;
        }
        // ★ 자체 시험(10-01 ㉗-h): 활성 탭을 이미 열린 공유 연결로 묶는다(탭 표식 메뉴에서 연결을 고른 것과 같은 길 `sess.use:<세션>`).
        if let Some(name) = id.strip_prefix("session.bind:") {
            let spec = Vault::open_default()
                .ok()
                .and_then(|v| v.get(name).ok().flatten());
            let sid = spec.as_ref().and_then(|sp| {
                self.all_sess()
                    .find(|s| {
                        !s.closing
                            && !s.is_private()
                            && s.spec
                                .as_ref()
                                .is_some_and(|h| crate::worker::same_server(h, sp))
                    })
                    .map(|s| s.id)
            });
            match sid {
                Some(sid) => {
                    let tab = self.editors.active_id();
                    self.badge_pick(tab, &format!("sess.use:{sid}"));
                    self.sess.status = format!("session.bind {name} → #{sid}");
                }
                None => self.sess.status = format!("session.bind {name}: no such shared session"),
            }
            self.redraw();
            return;
        }
        // ★ 자체 시험(10-01 ㉕): DB 용량을 우클릭 없이 청한다(= 메뉴 "용량 확인"과 같은 길).
        if id == "explorer.dbsizes" {
            self.explorer.request_db_sizes();
            self.redraw();
            return;
        }
        if let Some(rest) = id.strip_prefix("explorer.menu") {
            // 행 번호 또는 라벨 경로(`explorer.expand/select`와 같은 풀이 · 10-11 119차 성능 s11이 라벨을 쓴다).
            let arg = rest.trim_start_matches(':');
            let row = match arg.parse::<usize>() {
                Ok(r) => Some(r),
                Err(_) => self.explorer.visible_row_by_label(arg),
            };
            let ok = row.is_some_and(|r| self.explorer.capture_menu(r));
            self.sess.status = format!("explorer.menu row={arg} opened={ok}");
            self.redraw();
            return;
        }
        // 자체 시험(10-15 노드 DnD): `explorer.rowpt:<라벨|행>:<파일>` = 그 줄의 창 좌표 `x y`(`ui.down/move/up`에 넣는다).
        if let Some(rest) = id.strip_prefix("explorer.rowpt:") {
            // 첫 `:`에서 가른다(경로에 `D:`가 있다).
            let (arg, path) = rest.split_once(':').unwrap_or((rest, ""));
            let row = match arg.parse::<usize>() {
                Ok(r) => Some(r),
                Err(_) => self.explorer.visible_row_by_label(arg),
            };
            let pt = row.and_then(|r| self.explorer.row_point(r));
            let text = pt.map_or("none".to_string(), |p| format!("{} {}", p.x, p.y));
            if path.is_empty() {
                self.sess.status = format!("explorer.rowpt {arg} = {text}");
            } else {
                let _ = std::fs::write(path, text);
            }
            return;
        }
        if let Some(pick) = id.strip_prefix("explorer.pick:") {
            let ok = self.explorer.capture_pick(pick);
            if ok {
                // 메뉴 항목이 만든 동작(삭제 창 요청 · Generate SQL …)을 바로 거둔다 — 실제 클릭 길과 같게(10-15 · 삭제 창 E2E).
                self.explorer_actions();
                self.sess.status = format!("explorer.pick {pick}: ok");
            } else {
                self.sess.status = format!("explorer.pick {pick}: no menu and no selected row");
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
        // `conn.quick:<접속 문자열>` = 간략 접속 칸에 넣고 Enter와 같은 길(사용자 10-22 · 프로필 자동 저장 + 접속 · 자체 시험).
        if let Some(s) = id.strip_prefix("conn.quick:") {
            self.conn_win.panel.set_quick_text(s);
            if let Some(a) = self.conn_win.panel.quick_action() {
                self.handle_panel_action(a);
            }
            self.conn_win.redraw();
            return;
        }
        // T-153 자체 시험: 입력 창 i행 타입 콤보 고르기(`inputs.type:<행>:<타입>`) → 덤프의 `type=`·`preview=`로 확인(키 주입 없이).
        if let Some(rest) = id.strip_prefix("inputs.type:") {
            if let Some((i, ty)) = rest.split_once(':') {
                if let Ok(i) = i.parse::<usize>() {
                    if self.input_win.pick_type_for_test(i, ty) {
                        self.input_win.redraw();
                    }
                }
            }
            return;
        }
        // ★ Insert Sequence 자체 시험(docs/124 §3 ⑦ · 키 주입 0): `editor.regions:<a[-b];…>` = 다중 캐럿/구간 지정(구분 `;` · NSQL_STARTUP_CMD가 `,`로 쪼개므로 · ui.click:x/y 선례) · `seq.open` ·
        //   `seq.set:<식>`(입력란 글 = 식 · 미리보기 갱신) · `seq.commit` · `seq.cancel` · `seq.dump:<파일>`(질의 · 오류 · 값 · 본문).
        if let Some(spec) = id.strip_prefix("editor.regions:") {
            let regions: Vec<(usize, usize)> = spec
                .split(';')
                .filter_map(|r| {
                    let (a, b) = r.split_once('-').unwrap_or((r, r));
                    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
                })
                .collect();
            if !regions.is_empty() {
                self.ed_mut().set_regions_pub(&regions);
            }
            self.redraw();
            return;
        }
        if id == "seq.open" {
            self.prompt_open(crate::extensions::insert_sequence::CMD);
            return;
        }
        // `seq.open:<프롬프트 id>` = 확장(wasm)의 프롬프트(148차).
        if let Some(p) = id.strip_prefix("seq.open:") {
            self.prompt_open(p);
            return;
        }
        if let Some(expr) = id.strip_prefix("seq.set:") {
            if self.insseq_active() {
                self.palette.set_query(expr);
                self.insseq_update(expr);
            }
            return;
        }
        if id == "seq.commit" {
            let q = self.palette.query();
            let pick = self.palette.prompt_pick();
            self.palette.close();
            self.insseq_commit(&q, pick);
            return;
        }
        // `seq.pick:<id>` = 라이브 프롬프트의 고를 수 있는 행을 id로 고른다(T-447 · 자체 시험).
        if let Some(p) = id.strip_prefix("seq.pick:") {
            if !self.palette.set_prompt_pick(p) {
                self.sess.status = format!("seq.pick: no such row {p}");
            }
            self.redraw();
            return;
        }
        if id == "seq.cancel" {
            self.palette.close();
            self.insseq_cancel();
            return;
        }
        if let Some(path) = id.strip_prefix("seq.dump:") {
            self.insseq_dump(path);
            return;
        }
        if let Some(name) = id.strip_prefix("conn.edit:") {
            if let Ok(Some(spec)) = Vault::open_default().and_then(|v| v.get(name)) {
                self.conn_win.capture_open_detail(name);
                let vars = crate::connect::profile_vars(name);
                self.conn_win.panel.fill(name, &spec, vars);
                self.conn_win.panel.capture_touch();
                self.conn_win.redraw();
            }
            return;
        }
        // ★ T-153 후속 자체 시험(10-27): `conn.var:<프로필>:<이름>=<값>` = 프로필을 폼에 불러와 변수를 넣고(빈 값 = 삭제) [저장]과 같은 길 —
        //   결과는 CLI `nsql conn var <프로필>` 또는 프로필 파일의 `var.` 줄로 확인. `conn.vars:<프로필>` = 불러오기 + 프로필 변수 창 열기(캡처).
        if let Some(rest) = id.strip_prefix("conn.var:") {
            if let Some((name, kv)) = rest.split_once(':') {
                if let Ok(Some(spec)) = Vault::open_default().and_then(|v| v.get(name)) {
                    self.conn_win.capture_open_detail(name);
                    let vars = crate::connect::profile_vars(name);
                    self.conn_win.panel.fill(name, &spec, vars);
                    let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
                    if !self.conn_win.panel.set_var(k, v) {
                        self.sess.status = crate::profvars_win::bad_name_note(k);
                    } else if let Some(a) = self.conn_win.panel.save_action() {
                        self.handle_panel_action(a);
                    }
                    self.conn_win.redraw();
                }
            }
            return;
        }
        if let Some(name) = id.strip_prefix("conn.vars:") {
            if let Ok(Some(spec)) = Vault::open_default().and_then(|v| v.get(name)) {
                self.conn_win.capture_open_detail(name);
                let vars = crate::connect::profile_vars(name);
                self.conn_win.panel.fill(name, &spec, vars);
                self.open_profvars = true;
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
        // ★ T-352 자체 시험 훅: OS 파일 열기 사건 흉내(모든 OS · `about_to_wait`에서 `pump_os_open_files`가 연다 = 실제 경로와 같은 길).
        if let Some(path) = id.strip_prefix("os.open:") {
            nexa_sys::open_files::simulate([std::path::PathBuf::from(path)]);
            self.redraw();
            return;
        }
        match id.strip_prefix("open:") {
            Some(path) => self.open_file(Path::new(path)),
            None => self.menu_action(id),
        }
    }
}
