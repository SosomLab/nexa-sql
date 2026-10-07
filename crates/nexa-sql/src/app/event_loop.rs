//! App — winit 사건 처리기(`ApplicationHandler` · 창 사건·유휴 틱·재개).
//!
//! main.rs에서 옮김(docs/93 §4).

use crate::*;

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        // macOS Dock 아이콘 — 이벤트 루프 생성 직후에 넣으면 winit의 applicationDidFinishLaunching(활성화 정책 Regular)이
        // Dock 타일을 다시 만들며 덮는다(09-16 실기: 호출은 되나 `exec` 그대로) → 기동이 끝난 첫 resumed에서. 다른 OS no-op.
        icon::set_dock_icon();
        // 메인 창 규칙(사용자 09-17): **마지막 위치에서 종료 시점 크기로** 시작 · `window.monitor`(1-기준 · 왼쪽→오른쪽)가
        //   있으면 그 모니터 왼쪽 위 안쪽. 모니터 목록은 창을 만든 뒤 창 핸들로 얻는다(`el.available_monitors()`는 macOS
        //   `resumed` 시점에 비어 있었다 · 09-17) → 위치는 첫 창 이벤트 때 적용(직후 호출은 캐스케이드 배치가 덮는다).
        // ★ 숨긴 채 만들고 → 위치를 정한 뒤 → 보인다: 보이는 창을 다른 배율 모니터로 옮기면 winit(macOS)이 생성 시점 배율
        //   (2x)을 유지해 1x 모니터에서 반 크기로 그려졌다(09-17 ASCII 캡처). 숨긴 창은 보이는 순간 그 모니터 배율을 받는다.
        let attrs = icon::with_icon(
            Window::default_attributes()
                .with_title(if cfg!(debug_assertions) {
                    "Nexa SQL (Debug)"
                } else {
                    "Nexa SQL"
                })
                .with_visible(false)
                .with_theme(theme::window_theme(self.settings.theme_mode()))
                .with_inner_size({
                    // 마지막으로 닫힌 크기(`window.main_size` · 사용자 09-17) · 없으면 기본.
                    let (w, h) = self
                        .settings
                        .get("window.main_size")
                        .and_then(wingeom::parse_size)
                        .unwrap_or((1375.0, 945.0));
                    winit::dpi::LogicalSize::new(w, h)
                }),
        );
        // ★ 기동 구간 계측 2부(09-24 mac 전수 · 창까지 592 ms 가운데 `App` 뒤 320 ms가 어디인가): resumed 안의 단계별 누적 ms.
        let boot = self.frame_trace.as_ref().map(|f| f.boot);
        let mut rmarks: Vec<(&str, u128)> = Vec::new();
        let rmark = |what: &'static str, v: &mut Vec<(&str, u128)>| {
            if let Some(b) = boot {
                v.push((what, b.elapsed().as_millis()));
            }
        };
        rmark("resumed", &mut rmarks);
        let Ok(win) = el.create_window(attrs) else {
            eprintln!("{}", t(Msg::ErrNoWindow));
            el.exit();
            return;
        };
        let win = Rc::new(win);
        {
            let mut mons: Vec<_> = win.available_monitors().collect();
            mons.sort_by_key(|m| (m.position().x, m.position().y));
            if std::env::var_os("NSQL_TRACE_WINDOW").is_some() {
                for (i, m) in mons.iter().enumerate() {
                    eprintln!(
                        "monitor {}: pos={:?} size={:?} scale={} name={:?}",
                        i + 1,
                        m.position(),
                        m.size(),
                        m.scale_factor(),
                        m.name()
                    );
                }
            }
            let mon = self.settings.int("window.monitor");
            // 논리 좌표: 모니터 위치는 그 모니터 배율로 나눈다(2x 메인 + 1x 보조에서 물리 px는 섞인다 · 09-17).
            let place = if mon > 0 {
                mons.get((mon - 1) as usize).map(|m| {
                    let r = wingeom::monitor_rect(m);
                    (r.0 + 20, r.1 + 30)
                })
            } else {
                self.settings
                    .get("window.main_pos")
                    .and_then(wingeom::parse_pos)
                    .filter(|p| wingeom::on_any_monitor(*p, mons.iter().cloned()))
            };
            if let Some((x, y)) = place {
                win.set_outer_position(wingeom::logical(x, y));
            }
            // 화면 밖으로 나가지 않게(OS가 계단식으로 놓은 기본 위치 · 해상도가 바뀐 뒤의 기억 위치 — 09-21 점검: 1080 높이에서 아래 60px이 잘렸다).
            wingeom::keep_on_screen(&win, None);
            win.set_visible(true);
            rmark("visible", &mut rmarks);
        }
        self.scale = win.scale_factor() as f32;
        // 창이 생기면 OS 판정(winit)이 정확해진다 — System 모드는 여기서 확정.
        self.theme = theme::resolve(self.settings.theme_mode(), win.theme());
        self.apply_tab_line_colors();
        self.apply_tab_history_cfg();
        self.apply_flash_font();
        self.project_panel.set_flash_style(
            self.settings.get("ui.flash_shape").unwrap_or("rounded"),
            self.settings.boost_on(),
        );
        self.editors
            .set_view_tab_badge(self.settings.flag("extensions.tab_badge"));
        // 프로젝트 탐색기 필터 = 파일 색인 모드(Ctrl+P와 같은 원천 · 10-07).
        self.project_panel.set_index_mode(true);
        self.apply_busy_style();
        rmark("window", &mut rmarks);
        match present::Presenter::new(win.clone()) {
            Ok(p) => {
                if self.frame_trace.is_some() {
                    eprintln!("[frames] present backend = {}", p.backend());
                }
                self.surface = Some(p);
            }
            Err(e) => eprintln!("{e}"),
        }
        let near = win
            .outer_position()
            .ok()
            .map(|p| (p.x, p.y, win.outer_size().width));
        rmark("present", &mut rmarks);
        if !rmarks.is_empty() {
            let line: Vec<String> = rmarks.iter().map(|(w, ms)| format!("{w} {ms}")).collect();
            eprintln!("[startup:resumed] {} (ms since main)", line.join(" · "));
        }
        self.window = Some(win);
        self.apply_on_top();
        self.layout();
        self.set_focus(Focus::Editor);
        self.apply_indent();
        self.editors.set_default_eol(eol::default_eol(
            self.settings.get("file.eol_new").unwrap_or("auto"),
        ));
        self.apply_menu_decor();
        // 로그 창은 설정 `log.open_at_start`(기본 off · 사용자 09-15)일 때만 메인 옆에 함께 연다(F10으로 언제든).
        if self.settings.flag("log.open_at_start") {
            let owner = self.window.clone();
            self.log_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                near,
                owner.as_deref(),
            );
        }
        // 한글 조합 방식(T-139): 입력 소스 바뀜 알림을 구독하고 지금 상태로 맞춘다.
        nexa_sys::input_source::watch();
        self.sync_hangul_mode();
        // 툴바 배치 복원(설정 `toolbar.layout` · 플로팅 창은 about_to_wait에서 생성).
        self.apply_tool_layout_setting();
        // 데모(사용자 09-17): 'Demo' 프로필·파일이 있으면 메뉴 비활성 · 없고 아직 안 물었으면 최초 1회 팝업.
        self.demo_ready = Self::demo_exists();
        // 오래된 미저장 스냅숏 정리(docs/70 §5 · `project.backup_days`).
        let pruned = backups::prune(self.settings.int("project.backup_days").max(1) as u64)
            + backups::prune_drop(self.settings.int("project.backup_days").max(1) as u64);
        if pruned > 0 {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                tf(Msg::LogBackupPruned, &[&pruned.to_string()]),
            ));
        }
        self.project_startup();
        // ★ 작업 모드 셋(사용자 09-23 · [project::WorkMode]): 파일 모드 = 전역(`%APPDATA%`) · **폴더 모드**(`nexa-sql .` · `nexa-sql <폴더>`) =
        //   `<폴더>/.nsql/` · 프로젝트 모드 = 프로젝트 파일. 지금 폴더 모드가 나누는 것은 북마크뿐 — 설정·다른 기능도 `WorkMode::local_dir`
        //   한 자리에서 나누도록 설계만(67 §6).
        if let Some(dir) = self.arg_folder.clone() {
            self.bookmarks.set_folder(Some(dir.clone()));
            if self.folder_start.is_none() {
                self.folder_start = Some(dir);
            }
        }
        self.bookmarks.bind_project(self.project.path.as_deref());
        self.bm_sync_ui();
        self.project_restore();
        self.rebuild_menus();
        if !self.demo_ready && !self.settings.flag("demo.prompted") {
            let _ = self.settings.set("demo.prompted", "on");
            self.persist_settings();
            self.pending_demo_prompt = true;
        } // 자체 캡처용 기동 명령(`NSQL_STARTUP_CMD=open:<파일>,view.extensions,…` · 쉼표 구분): 키 주입(SendKeys) 없이 특정 화면을
          //   띄워 PrintWindow로 확인하려는 것(사용자가 쓰는 중에 키를 쏘면 다른 창으로 간다 · 09-19 사고). 평소엔 변수 없음 = 비용 0.
          // 인자로 받은 파일(프로젝트 파일 제외) = 파일 모드로 연다(사용자 09-22).
        for f in std::mem::take(&mut self.arg_files) {
            self.open_file(&f);
        }
        if let Ok(cmds) = std::env::var("NSQL_STARTUP_CMD") {
            for id in cmds.split(',').map(str::trim).filter(|c| !c.is_empty()) {
                // `@connected:<명령>` = 첫 접속이 된 뒤에 실행(시작 인자로 접속하는 프로필 + 실행 시험 · 메모리 측정).
                // `@after:<ms>:<명령>` = 기동 뒤 그 시간이 지나면 실행(닫기 전후 메모리 비교 같은 시차 시험).
                if let Some(rest) = id.strip_prefix("@after:") {
                    if let Some((ms, cmd)) = rest.split_once(':') {
                        let at = Instant::now() + Duration::from_millis(ms.parse().unwrap_or(0));
                        self.startup_timed.push((at, cmd.to_string()));
                    }
                    continue;
                }
                match id.strip_prefix("@connected:") {
                    Some(later) => self.startup_after_connect.push(later.to_string()),
                    None => self.startup_cmd(id),
                }
            }
        }
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, _ev: Wake) {
        self.drain_all();
    }

    /// 이벤트 루프가 끝난다 = 프로그램 종료: 들고 있던 임시 비밀번호(세션 자격 금고)의 봉투와 키를 덮어써 버린다.
    fn exiting(&mut self, _el: &ActiveEventLoop) {
        self.pw_once = None;
        nsql_vault::session::shutdown();
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if self.exit_requested {
            self.finish_exit(el);
            return;
        }
        // 입력 소스가 바뀌었다(한/영 · 다른 입력기 — macOS 분산 알림) → 한글 조합 방식을 다시 맞춘다.
        if nexa_sys::input_source::take_changed() {
            self.sync_hangul_mode();
        }
        self.persist_window_sizes(false);
        self.pump_window_requests(el);
        // 캐럿 깜빡임 — 0.5초 타이머가 **실제로 만료됐을 때만** 다시 그린다.
        // ★ 매 호출마다 request_redraw를 하면 그리기 → about_to_wait → 그리기의 무한 루프가 되어
        //   유휴 CPU 한 코어 100% · 키 입력이 프레임당 하나씩만 처리되는 지연(글자 14개에 3초 ·
        //   옛 결과가 화면에 남음)이 생긴다(09-13 Windows 실기 계측).
        let now = Instant::now();
        if now >= self.next_blink {
            // 캐럿 깜빡임 끔(`editor.caret_blink` · 향상 모드) = 타이머 깨움 없음(캐럿은 켜진 채).
            self.next_blink = if self.settings.flag("editor.caret_blink") {
                now + Duration::from_millis(500)
            } else {
                now + Duration::from_secs(3600)
            };
            // ★ 메인 창이 키 창이 아니면 깜빡이지 않는다(캐럿은 켜진 채 · 09-24 맥 전수: 깜빡임 = 전체 프레임 다시 그리기 ×2/초 ·
            //   맥 softbuffer present 36 ms/프레임이라 뒤에 있어도 유휴 CPU 19 % · footprint +20 MB였다 → 0).
            //   맥은 창이 키 창이어도 **앱이 비활성**(뒤에 있음)이면 깜빡이지 않는다(`NSApplication.isActive`).
            let app_active = nexa_sys::layer_present::app_active().unwrap_or(true);
            if self.focus == Focus::Editor && self.main_active && app_active {
                self.redraw();
            }
        }
        // 오버레이 스크롤바 페이드(편집기·그리드·로그 창) — 보이는 동안만 ≈30ms 타이머.
        let now_ms = self.started.elapsed().as_millis() as u64;
        self.tx_tick();
        self.idle_tick(now);
        // ★ 유휴 인덱스 선적재(docs/84 §7) — 칸마다 간격 판정은 안에서.
        self.explorer.prefetch_tick(now);
        let mut redraw = self.ed_mut().tick(now_ms);
        redraw |= self.grid.tick(now_ms);
        if self.zoom_hud.visible() {
            let st = self.zoom_hud_style();
            redraw |= self.zoom_hud.tick(now, &st);
        }
        // Goto Anything 프로젝트 파일 열거 수거(10-07).
        redraw |= self.goto_walk_tick();
        redraw |= self.git.poll();
        // 잠든 결과 탭의 텍스트 변환도 이어서 거둔다(다른 탭에서 완성 · 09-16) — 그리지는 않는다.
        for g in self.sleeping_grids_mut() {
            let _ = g.tick(now_ms);
        }
        redraw |= self.editors.tick();
        redraw |= self.split_v.tick(now_ms);
        redraw |= self.split_h.tick(now_ms);
        redraw |= self.split_d.tick(now_ms);
        // 더러움 표시(`*`) 갱신 · 닫기 2단 안내.
        redraw |= self.editors.refresh_dirty();
        if let Some(m) = self.editors.take_notice() {
            self.sess.status = t(m).into();
            redraw = true;
        }
        if self.editors.relayout_if_needed() {
            redraw = true;
        }
        if redraw {
            self.redraw();
        }
        if self.toasts.tick(Instant::now()) {
            self.redraw();
        }
        {
            let (rd, next) = self.run_toast.tick(Instant::now());
            self.run_toast_next = next;
            if rd {
                self.redraw();
            }
        }
        if self.log_win.tick(now_ms) {
            self.log_win.redraw();
        }
        if self.colors_win.tick(now_ms) {
            self.colors_win.redraw();
        }
        if self.keys_win.tick(now_ms) {
            self.keys_win.redraw();
        }
        if self.prefs_win.tick(now_ms) {
            self.prefs_win.redraw();
        }
        if self.file_win.tick(now_ms) {
            self.file_win.redraw();
        }
        if self.explorer.tick(now_ms) {
            self.redraw();
        }
        if self.search.tick(now_ms) || self.search.poll() {
            self.redraw();
        }
        if self.project_panel.tick(now_ms) {
            self.redraw();
        }
        // 프로젝트 필터 열거의 실패 폴더 = 로그 창(도착 순 병합 · 사용자 09-23).
        for (path, why) in self.project_panel.take_scan_log() {
            self.log_win.push(LogEntry::new(
                LogKind::Info,
                format!("project filter: skipped {}: {why}", path.display()),
            ));
        }
        if self.bm_panel.tick(now_ms) {
            self.redraw();
        }
        if self.outline_panel.is_visible() && self.outline_panel.tick(now_ms) {
            self.redraw();
        }
        // Ctrl+P 조회 중 표시(혜성 · 완료 깜빡임 · 10-07).
        if self.palette.tick(now_ms) {
            self.redraw();
        }
        self.project_sync_active();
        self.project_index_pump();
        // ★ 활성 뷰 탭의 상세 데이터가 없으면(메모리뿐이라 끊길 수 있다) 다시 채운다 · 확장이 없어졌으면 그 뷰 탭을 닫는다 — 종전엔 빈 일반
        //   편집기로 그려져 사용자가 글을 넣을 수 있었다(사용자 10-07 "확장 탭에 SQL").
        if let Some(k) = self.editors.active_view().map(str::to_string) {
            if !self.ext_details.contains_key(&k) {
                let ok = k
                    .strip_prefix("ext:")
                    .map(|id| id.to_string())
                    .is_some_and(|id| self.ext_reopen_view(&id));
                if !ok {
                    self.editors.close_view_tabs(&k);
                }
                self.redraw();
            }
        }
        // 프로젝트 폴더 변경 감시(T-293 · 10-07).
        if self.dir_watch_pump() {
            self.redraw();
        }
        self.project_autosave_tick();
        self.sync_open_files();
        self.bm_tick();
        self.project_pump();
        if self.editors.poll_preview() {
            self.redraw();
        }
        if self.search.take_request() {
            self.start_search();
        }
        if let Some(req) = self.search.take_open() {
            self.open_search_result(req);
        }
        if self.ext_panel.tick(now_ms) {
            self.redraw();
        }
        self.ext_fetch_poll();
        if self.find.tick(now_ms) {
            self.redraw();
        }
        if self.conn_win.tick_bars(now_ms) {
            self.conn_win.redraw();
        }
        let bars_live = self.ed_mut().scrollbars_visible()
            // 드래그 선택 중 포인터가 편집기 위/아래 밖에 멈춰 있다 → 틱이 자동 스크롤을 이어 간다(T-158 · 그동안만).
            || self.ed_mut().drag_autoscroll_active()
            || self.grid.bars_visible()
            || self.grid.text_pending()
            || self.git.pending()
            || self.sleeping_grids().any(|g| g.text_pending())
            || self.panel.menu_open()
            || self.log_win.bars_visible()
            || self.log_win.tooltip_pending()
            || self.log_win.drag_active()
            || self.conn_win.bars_visible()
            || self.conn_win.tooltip_pending()
            || self.grid.hover_animating()
            || self.conn_win.hover_animating()
            || self.colors_win.animating()
            || self.keys_win.animating()
            || self.prefs_win.animating()
            || self.file_win.animating()
            || self.explorer.bars_visible()
            || self.explorer.background_pending()
            || self.objdetail.is_animating(Instant::now())
            || self.find.animating()
            || self.search.animating()
            || self.project_panel.animating()
            || self.bm_panel.animating()
            || self.ext_fetch_rx.is_some()
            || !self.file_loads.is_empty()
            || self.editors.tooltip_pending()
            || self.zoom_hud.visible()
            || self.goto_walk.is_some()
            || self.palette.animating()
            || self.toasts.animating();
        // 애니메이션 프레임 간격 = 1000 / `ui.max_fps`(60 = 16ms · 30 = 33ms · 15 = 66ms · 향상 모드 30).
        let frame_ms = (1000 / self.settings.int("ui.max_fps").clamp(5, 240)).max(4) as u64;
        let mut next = if bars_live {
            self.next_blink.min(now + Duration::from_millis(frame_ms))
        } else {
            self.next_blink
        };
        // ★ 플래시 페이드 = 프레임 **×3**(사용자 10-07 "사라지는 프레임이 너무 적다") · 향상 모드 = 즉시 모드(프레임 0 · 마감 때 한 번만).
        if self.project_panel.flash_animating() {
            next = next.min(now + Duration::from_millis((frame_ms / 3).max(4)));
        }
        if let Some(t) = self.project_panel.flash_deadline() {
            next = next.min(t);
        }
        // 서버 신호등 재시도 예약(접속 창이 열려 있을 때만).
        if let Some(t) = self.conn_win.tick(now) {
            next = next.min(t);
        }
        // 입력 창의 IME 안내 만료.
        if let Some(t) = self.input_win.tick(now) {
            next = next.min(t);
        }
        // 삭제 창의 타임아웃 버튼(무장 중 100ms 틱 · 10-01).
        if self.drop_win.armed() {
            if self.drop_win.tick() {
                self.drop_win.redraw();
            }
            next = next.min(now + Duration::from_millis(100));
        }
        self.sync_modal();
        // settings.json 감시(열어 둔 뒤 1초 폴링 · 저장 즉시 반영).
        if let Some(t) = self.json_tick(now) {
            next = next.min(t);
        }
        // Oracle 라이브 로그 폴링(실행 중에만 · 끝나면 마지막 1회).
        if let Some(t) = self.live_tick(now) {
            next = next.min(t);
        }
        if let Some(t) = self.run_toast_next {
            next = next.min(t);
        }
        // 막힘 감지(docs/56 L3) — 미커밋 세션이 있을 때만 `tx.block_poll_secs` 간격으로 메타 세션에 한 문장.
        if let Some(t) = self.tx_block_tick(now) {
            next = next.min(t);
        }
        // 자체 캡처용 지연 기동 명령 — 접속이 끝나 세션이 한가해진 뒤 한 번.
        if self.startup_connected && !self.startup_after_connect.is_empty() && !self.sess.blocked()
        {
            for id in std::mem::take(&mut self.startup_after_connect) {
                self.startup_cmd(&id);
            }
        }
        // 쓰이지 못한 일회성 비밀번호는 20초 뒤에 버린다(버려지면서 0으로 덮어쓴다).
        if let Some((_, _, at)) = &self.pw_once {
            let end = *at + Duration::from_secs(20);
            if end <= now {
                self.pw_once = None;
            } else {
                next = next.min(end);
            }
        }
        if !self.startup_timed.is_empty() {
            let due: Vec<String> = self
                .startup_timed
                .iter()
                .filter(|(at, _)| *at <= now)
                .map(|(_, c)| c.clone())
                .collect();
            self.startup_timed.retain(|(at, _)| *at > now);
            let ran = !due.is_empty();
            for id in due {
                self.startup_cmd(&id);
            }
            // ★ 타이머 명령이 부탁한 창(설정·라이선스·Import·세션 …)은 **지금** 연다 — 종전엔 다음 창 사건까지 깃발만 남아
            //   헤드리스 자체 시험(기동 명령만 · 사건 0)에서 안 열렸다(협업 10-07 T-305).
            if ran {
                self.pump_window_requests(el);
            }
            if let Some(t) = self.startup_timed.iter().map(|(at, _)| *at).min() {
                next = next.min(t);
            }
        }
        self.file_loads_poll();
        self.multi_load_poll();
        // ★ 그리드 편집 요청이 세션 바쁨으로 미뤄졌으면 한가해진 뒤 여기서(docs/87 · 키 조회) · 입력 사건 없이 생긴 페치/재조회 요청
        //   (필터 변경 → 로컬 채움 · 승격/복귀 · 페이지 연쇄 · 기동 명령)도 같은 자리에서(협업 bin50/51 a2·b1 · 10-07).
        if (self.grid.has_edit_requests() || self.grid.has_pending_requests())
            && !self.sess.blocked()
        {
            self.after_grid_event();
        }
        // 명령·IME로 온 편집이 거대 편집 확인에 막혔으면 알린다(키 입력은 `route`가 바로 알린다).
        self.giant_notice();
        self.regions_cap_notice();
        // 메모리 회수 — 큰 것을 놓은 직후 1회 + 유휴 주기(`memtrim.rs`).
        if let Some(t) = self.mem_tick(now) {
            next = next.min(t);
        }
        // 외부 파일 변경(docs/58) — 사건 수거 + 활성 창에서 보이는 탭 폴링.
        if let Some(t) = self.ext_tick(now) {
            next = next.min(t);
        }
        // 탐색기 유휴 워터마크(docs/57 T2) — `meta.refresh_secs` 간격(기본 300초 · 0 = 끔 · 유휴일 때만 1행 질의).
        if let Some(t) = self.meta_refresh_tick(now) {
            next = next.min(t);
        }
        // 유휴 미커밋 점검(docs/56 L2) — 미커밋 세션이 있을 때만 5초(카운트다운 중 1초) 간격으로 깬다.
        if let Some(t) = self.tx_guard_tick(now) {
            next = next.min(t);
        }
        // 🔧 북마크 디바운스 저장 · 프로젝트 자동 저장의 **깨움**(사용자 09-23 "값이 바뀌어도 저장 안 됨"): 종전에는 다른 사건이
        //   루프를 깨울 때만 `tick_save`/`project_autosave_tick`이 돌아 앱이 가만히 있으면 저장이 미뤄졌다 → 마감 시각에 스스로 깬다.
        if let Some(t) = self.bookmarks.next_save_at(now) {
            next = next.min(t);
        }
        if let Some(t) = self.project_autosave_next(now) {
            next = next.min(t);
        }
        // 자동 완성 디바운스(docs/76 · `intel.delay_ms`) — 마감이 지나면 요청 · 아니면 그 시각에 깬다.
        if self.intel.due(now) {
            self.intel_request(false);
        }
        // 아웃라인 패널 = 활성 탭·본문 세대가 바뀌었을 때만 다시(캐시 · 유휴 틱 · D-203).
        self.outline_sync();
        // ★ 미사용 판정 즉시 회수(사용자 09-23): `스키마.`로 읽어 온 메타 버킷은 열린 문서 어디에도 그 이름이 없으면 바로 버린다
        //   (문서 낱말 캐시로 판정 · 현재 스키마·사전·트리가 펼친 것은 대상 아님).
        {
            let intel = &self.intel;
            let n = self
                .explorer
                .reclaim_intel_buckets(&|s| intel.doc_mentions(s));
            if n > 0 && self.settings.flag("log.dev_mode") {
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    format!("[intel] reclaimed {n} unused schema bucket(s)"),
                ));
            }
        }
        // 상세 카드 머무름 마감(사용자 09-24): 머문 마지막 대상을 카드에 넘기고 그때만 선조회·다시 그리기.
        if self.intel.card_tick(now) {
            self.intel_card_prefetch();
            self.redraw();
        }
        if let Some(t) = self.intel.next_wake() {
            next = next.min(t);
        }
        // 머무름 툴팁 마감(Ctrl 없는 객체 설명 · `objlink.hover_ms`).
        if let Some(t) = self.objlink_hover_tick(now) {
            next = next.min(t);
        }
        // 결과 열 머리 hover 카드(사용자 10-07).
        if let Some(t) = self.objlink_header_tick(now) {
            next = next.min(t);
        }
        // ★ 메모리 맵 창(docs/80): 열려 있을 때만 `mem.refresh_ms`마다 표본 → 창·상태줄 갱신. 닫혀 있으면 깨우지도 않는다.
        if self.mem_win.is_open() {
            if now >= self.mem_next {
                let every = self.mem_every();
                self.mem_next = now + every;
                let t0 = Instant::now();
                let s = self.mem_sample();
                let sample_us = t0.elapsed().as_micros();
                // ★ T-310 뿌리(협업 재측정 10-08 "그리기 생략 뒤에도 유휴 15~26 ms/초"): 종전엔 표본마다 **메인 창 전체**를 다시 그렸다
                //   (상태줄 총량 글 때문 · 10만 행 그리드·편집기·탐색기 포함) → 상태줄에 찍히는 글(`fmt` 유효숫자 3)이 바뀔 때만.
                //   메모리 창 자체는 `set_sample`이 표시 서명으로 판단한다.
                let shown_before = memstat::fmt(self.mem_status.0);
                self.mem_status = (s.sys.footprint, Some(now));
                self.mem_win.set_sample(s, every.as_millis() as u64);
                let status_changed = memstat::fmt(s.sys.footprint) != shown_before;
                if status_changed {
                    self.redraw();
                }
                // 진단(`NSQL_TRACE_MEMWIN=1`): 표본 비용 µs + 메인 창 다시 그림 여부 — 표본 수집 대 그리기를 가른다.
                if memstat::trace_on() {
                    eprintln!("[memwin] sample {sample_us} us · main redraw {status_changed}");
                }
            }
            next = next.min(self.mem_next);
        }
        el.set_control_flow(ControlFlow::WaitUntil(next));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // 한글 입력 진단(`NSQL_TRACE_IME=1` · T-139 · 평소 비용 = 환경 변수 조회 없음 — 기동 때 한 번 읽어 둔 깃발):
        //   창·키(논리/물리/text)·IME 사건을 온 순서대로 stderr에. 판정 로직은 건드리지 않는다.
        if self.trace_ime {
            match &event {
                WindowEvent::KeyboardInput { event: k, is_synthetic, .. } => eprintln!(
                    "[ime] {:?} key state={:?} logical={:?} physical={:?} text={:?} repeat={} synth={}",
                    id, k.state, k.logical_key, k.physical_key, k.text, k.repeat, is_synthetic
                ),
                WindowEvent::Ime(i) => eprintln!("[ime] {id:?} ime {i:?}"),
                WindowEvent::Focused(f) => eprintln!("[ime] {id:?} focused={f}"),
                WindowEvent::ModifiersChanged(m) => eprintln!("[ime] {id:?} mods={:?}", m.state()),
                _ => {}
            }
        }
        // ★ 닫힌 창에서 누른 키가 새 포커스 창으로 새는 두 길을 막는다(사용자 09-21 — 비밀번호 창 Enter → 편집기 줄바꿈):
        //   ① winit은 창이 포커스를 얻을 때 **이미 눌려 있는 키**를 합성 누름(`is_synthetic`)으로 보낸다 → 어느 창이든 버린다
        //   (사용자가 새로 누른 것이 아니다 · 수식키 상태는 `ModifiersChanged`가 따로 알린다) ② 계속 누르고 있으면 OS 자동 반복이
        //   새 창으로 온다 → 입력 창을 닫은 뒤 그 키가 떼어질 때까지의 반복 누름을 버린다(`key_guard_step`).
        if let WindowEvent::KeyboardInput {
            event: k,
            is_synthetic,
            ..
        } = &event
        {
            let pressed = k.state == ElementState::Pressed;
            if *is_synthetic && pressed {
                return;
            }
            let age = self.key_guard.map(|at| at.elapsed());
            let (drop_it, keep) = key_guard_step(age, pressed, k.repeat);
            if !keep {
                self.key_guard = None;
            }
            if drop_it {
                return;
            }
        }
        // 입력(키 누름·IME·마우스 버튼) = 캐럿 깜빡임 위상을 "켜짐"으로 되돌리고(움직인 캐럿이 최대 0.5초 안 보이던 것 · 09-19)
        //   계측이 켜져 있으면 입력→화면 지연의 시작점을 남긴다.
        let is_input = matches!(
            event,
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    state: ElementState::Pressed,
                    ..
                },
                ..
            } | WindowEvent::Ime(_)
                | WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                }
        );
        if is_input {
            let now = Instant::now();
            self.blink_origin = now;
            self.next_blink = now + Duration::from_millis(500);
            if self.frame_trace.is_some() {
                self.input_at = Some(now);
            }
        }
        if matches!(event, WindowEvent::Focused(true)) {
            self.on_window_focused(id);
            // 라이선스 파일이 밖에서(CLI `nsql license install`) 바뀌었으면 배지·창을 다시(변경 서명만 봄 · 파일은 안 읽음).
            if self.licensing.refresh() {
                self.license_win.redraw();
            }
            // 다른 앱에서 입력 소스를 바꾸고 돌아왔을 수 있다.
            self.sync_hangul_mode();
        }
        // 메인 창 활성/비활성(docs/58 §2-5): 돌아오는 순간 열린 파일을 한 번에 확인 · 뒤에 있을 때는 아무것도 안 한다.
        // ★ 커서가 창 밖으로 나감 = hover·툴팁 정리(빠르게 나가면 마지막 MouseMove가 영역 안이라 툴팁이 남았다 · 사용자 09-30).
        if matches!(event, WindowEvent::CursorLeft { .. })
            && self.window.as_ref().is_some_and(|w| w.id() == id)
        {
            self.pointer_gone();
        }
        if let WindowEvent::Focused(on) = event {
            if self.window.as_ref().is_some_and(|w| w.id() == id) {
                self.main_active = on;
                if !on {
                    // 비활성 = 마우스 상태(hover·툴팁·캡처·링크 hover) 정리(사용자 09-30).
                    self.pointer_gone();
                }
                // 위상 초기화: 돌아오면 켜진 채로 깜빡임 재개 · 나가면 켜진 채 한 번 그리고 멈춘다(09-24).
                self.blink_origin = Instant::now();
                self.next_blink = self.blink_origin + Duration::from_millis(500);
                self.redraw();
                if on {
                    self.ext_check(true);
                }
            }
        }
        // ★ 접속 창 = 모달: 열려 있는 동안 **메인 창과 그 일부인 로그·색·단축키·설정 창**의 입력은 버리고
        //   (OS 수준은 `winfocus::set_enabled`) 접속 창을 앞으로(사용자 09-15 "로그 창도 메인의 일부").
        if let WindowEvent::DroppedFile(p) = &event {
            // OS에서 창으로 끌어다 놓기(3-OS 공통 winit 경로) = 열기.
            let p = p.clone();
            self.open_file(&p);
            return;
        }
        let modal_open = self.modal_open();
        let is_modal_win = self.conn_win.is(id)
            || self.file_win.is(id)
            || self.sqlprev_win.is(id)
            || self.import_win.is(id)
            || self.license_win.is(id)
            || self.about_win.is(id)
            || self.drop_win.is(id)
            || (self.input_win.is_modal() && self.input_win.is(id));
        if modal_open
            && !is_modal_win
            && matches!(
                event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::MouseInput { .. }
                    | WindowEvent::MouseWheel { .. }
                    | WindowEvent::Ime(_)
            )
        {
            if let Some(w) = self.modal_window() {
                w.focus_window();
            }
            return;
        }
        // 보조 창 사건 = 창마다 자기 처리기로(처리했으면 여기서 끝) — `aux_window_event`.
        if self.aux_window_event(el, id, &event) {
            // 보조 창에서 낸 "창 열기" 요청(설정 창의 확장 설정 바로가기 · 사용자 09-30)도 같은 펌프를 지난다.
            self.open_requested_windows(el);
            return;
        }
        match &event {
            WindowEvent::CloseRequested => {
                // 🔧 창 닫기(X)도 **늘** 종료 흐름(`request_exit` = 프로젝트 저장/물음 · 미저장 파일 탭 물음 · 트랜잭션 확인)을
                //   지난다 — 종전에는 미커밋이 없으면 흐름을 건너뛰어 프로젝트·북마크가 저장되지 않았다(사용자 09-23 "종료 시 꼭 저장").
                self.request_exit();
                self.redraw();
                if !self.exit_requested {
                    return;
                }
                self.flush_on_exit();
                self.persist_window_sizes(true);
                for s in self.all_sess() {
                    s.control(worker::Cmd::Quit);
                }
                el.exit();
                return;
            }
            WindowEvent::Moved(_) => {
                // 다른 배율의 모니터로 옮겨진 뒤 ScaleFactorChanged가 안 오는 경우(프로그램 이동 · 09-17) 배율을 다시 읽는다.
                if let Some(w) = &self.window {
                    let s = w.scale_factor() as f32;
                    if (s - self.scale).abs() > 0.01 {
                        self.scale = s;
                        self.layout();
                    }
                }
                self.redraw();
                return;
            }
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
                return;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
                self.redraw();
                return;
            }
            WindowEvent::ThemeChanged(_) => {
                // OS 라이트/다크 전환 — System 모드일 때만 따라간다.
                if self.settings.theme_mode() == ThemeMode::System {
                    self.apply_theme();
                }
                return;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
                // macOS Control(⌘와 별개 · Sublime `ctrl+cmd+g`) — 다른 OS에선 늘 false.
                self.ctrl_mac = cfg!(target_os = "macos") && m.state().control_key();
                self.alt = m.state().alt_key();
                self.ctrl_raw = m.state().control_key();
                // ★ Ctrl(⌘)을 누르는 동안만 객체 링크(T-256) — 떼면 걷는다.
                self.objlink_sync();
                // ★ Alt를 누르는 동안 = 전체 경로 보기(메뉴·팔레트·검색 결과의 가운데 … 축약 해제 · 사용자 09-22).
                if nexa_ctl::draw::set_show_full(self.alt) {
                    self.redraw();
                }
                // ★ 열(블록) 선택 모드(Sublime · 사용자 09-15/09-17 OS별): Windows Alt+Shift · macOS Option · Linux는 우클릭 쪽에서.
                let col = match self.column_rule() {
                    "alt_shift" => self.alt && self.shift,
                    "alt" => self.alt,
                    _ => self.col_right_drag,
                };
                self.editors.set_column_mode(col);
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                // 그리드 헤더 경계 위 = 폭 조절 커서.
                if let Some(w) = &self.window {
                    let cur = Point {
                        x: self.cursor.0,
                        y: self.cursor.1,
                    };
                    let over_edge = self.grid.header_edge_hover(self.cursor.0, self.cursor.1);
                    // 편집기 본문(거터 제외) 위 = **I-빔**(Sublime·VS Code · 사용자 09-19) — 메뉴·팔레트·팝업이 덮으면 화살표.
                    let ed = self.editors.editor_bounds();
                    let gutter = self.editors.cur().gutter_width();
                    let text_area = Rect::new(ed.x + gutter, ed.y, (ed.w - gutter).max(0), ed.h);
                    let covered = self.menubar.is_open()
                        || self.palette.is_open()
                        || self.tool_dock.is_dragging()
                        || self.status_menu.is_open()
                        || self.intel.is_open()
                        || self.editors.cur().popup_open();
                    let over_text = !covered && text_area.contains(cur);
                    // 스플리터 위/드래그 중 = ↔ · ↕ (그리드 헤더 경계보다 우선).
                    w.set_cursor(
                        if self.split_v.is_dragging() || self.split_v.rect().contains(cur) {
                            winit::window::CursorIcon::ColResize
                        } else if self.split_h.is_dragging()
                            || self.split_h.rect().contains(cur)
                            || self.split_d.is_dragging()
                            || (!self.split_d.rect().is_empty()
                                && self.split_d.rect().contains(cur))
                        {
                            winit::window::CursorIcon::RowResize
                        } else if over_edge {
                            winit::window::CursorIcon::ColResize
                        } else if over_text {
                            winit::window::CursorIcon::Text
                        } else {
                            winit::window::CursorIcon::Default
                        },
                    );
                }
            }
            WindowEvent::Ime(ime) => {
                let mut inv = Invalidations::default();
                // ★ 열린 팔레트가 먼저(09-27 사용자 Linux 실기 "팔레트에서 한글이 안 들어간다"): 팔레트는 `Focus` 변형이 아니라
                //   종전에는 조합·확정 글자가 `focused_textbox()`(편집기)로 새고 팔레트에는 영문(KeyboardInput)만 닿았다.
                //   확정 글자는 키 입력과 같은 길(`route_inner` → `palette.on_event`)로 흘려 필터·선택이 같이 돈다.
                if self.palette.is_open() {
                    match ime {
                        Ime::Preedit(t, _) => self.palette.set_preedit(t, &mut inv),
                        Ime::Commit(t) => {
                            self.palette.set_preedit("", &mut inv);
                            for c in t.chars().filter(|c| !c.is_control()) {
                                self.route_inner(
                                    InputEvent::Char { c, now_ms: 0 },
                                    Invalidations::default(),
                                );
                            }
                        }
                        _ => {}
                    }
                    self.redraw();
                    return;
                }
                if let Some(tb) = self.focused_textbox() {
                    match ime {
                        Ime::Preedit(t, _) => tb.set_preedit(t, &mut inv),
                        Ime::Commit(t) => {
                            tb.set_preedit("", &mut inv);
                            for c in t.chars().filter(|c| !c.is_control()) {
                                tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                            }
                        }
                        _ => {}
                    }
                    // ★ 검색에 쓰이는 입력은 전부 조합 중 글자까지 바로 거른다(설정 창 검색과 같은 규칙 · 09-19 확장 패널 →
                    //   09-23 프로젝트 필터·북마크 필터·찾기 막대로 일반화 · 사용자 "자모 완성과 상관없이 한글 검색").
                    match self.focus {
                        Focus::Ext => self.ext_panel.query_changed(),
                        Focus::Project => self.project_panel.query_changed(),
                        Focus::Bookmarks => self.bm_panel.query_changed(),
                        Focus::Outline => self.outline_panel.query_changed(),
                        // 값 목록 팝업 검색 상자(조합 중 글자까지 바로 거른다 · 10-06).
                        Focus::Grid => self.grid.text_input_query_changed(),
                        Focus::Find => self.find_step(true, false),
                        _ => {}
                    }
                    self.redraw();
                }
                return;
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                // 한/영 키(Windows · 탐색기 포커스 전용 — nexa-beep docs/27 §8): 탐색기는 IME를 끊어 OS 전환이 무력하므로
                //   앱이 모드를 토글한다. VK_HANGUL은 키보드 드라이버 수준이라 IME 없이도 온다(논리 HangulMode · 물리 Lang1).
                if cfg!(windows)
                    && self.typeahead_target()
                    && (kev.logical_key == Key::Named(NamedKey::HangulMode)
                        || kev.physical_key
                            == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Lang1))
                {
                    self.hangul_mode = !self.hangul_mode;
                    self.sess.status = t(if self.hangul_mode {
                        Msg::StHangulOn
                    } else {
                        Msg::StHangulOff
                    })
                    .into();
                    self.redraw();
                    return;
                }
                // ★ 적재 중인 탭에서 Esc = 그 적재 취소(자리 탭을 닫는다 · 사용자 09-20). 팔레트·찾기 막대가 열려 있으면 그쪽의 Esc.
                if kev.logical_key == Key::Named(NamedKey::Escape)
                    && self.focus == Focus::Editor
                    && !self.palette.is_open()
                    && self.editors.active_loading()
                    && self.file_load_cancel_active()
                {
                    return;
                }
                // 탐색기 포커스의 F5 = 선택 노드 하위를 조용히 다시 읽기 · Shift+F5 = 캐시를 버리고 새로(docs/57 T3).
                //   편집기 포커스의 F5(전체 실행)와 겹치지 않게 키맵보다 먼저 본다.
                if self.focus == Focus::Explorer && kev.logical_key == Key::Named(NamedKey::F5) {
                    self.explorer.refresh_selected(self.shift);
                    self.redraw();
                    return;
                }
                // ★ 단축키 = 키맵 표 조회(Sublime 기본 · `key.*` 설정 · 사용자 09-15). 조합키 없는 글자는 타이핑이므로
                // 표에 있어도 가로채지 않는다(F-키·Enter 같은 이름 키는 예외).
                if let Some(ch) = Chord::from_winit(
                    &kev.logical_key,
                    &kev.physical_key,
                    self.primary,
                    self.shift,
                    self.alt,
                    self.ctrl_mac,
                ) {
                    // 2단 코드의 둘째 키(`Ctrl+K, Ctrl+U` · 09-16) — 없는 조합이면 안내만.
                    if let Some(first) = self.pending_chord.take() {
                        match self.keymap.lookup_seq(&first, &ch) {
                            // 자동 반복(키를 누르고 있음)은 반복해도 되는 명령만(사용자 09-17 Ctrl+T 80개).
                            Some(id) if kev.repeat && !keymap::repeatable(id) => {}
                            Some(id) => self.key_command(id, el),
                            None => {
                                self.sess.status = tf(
                                    Msg::StChordUnbound,
                                    &[&format!("{}, {}", first.display(), ch.display())],
                                );
                                self.redraw();
                            }
                        }
                        return;
                    }
                    let plain_char =
                        !ch.primary && !ch.alt && !ch.ctrl && ch.key.chars().count() == 1;
                    self.tmark("key");
                    if !plain_char {
                        // `find.*`는 찾기 패널에 포커스일 때만(그 밖에선 가로채지 않는다 — mac Alt+글자 입력 보존).
                        if let Some(id) = self.keymap.lookup(&ch) {
                            if id.starts_with("find.") {
                                if self.focus == Focus::Find && self.find.is_visible() {
                                    let a = self.find.command(id);
                                    self.find_action(a);
                                    self.redraw();
                                    return;
                                }
                                if !cfg!(target_os = "macos") {
                                    return;
                                }
                            }
                        }
                        // 결과 그리드 포커스 = 표 편집 키가 전역 키맵보다 먼저(F2 셀 편집 · Ctrl/⌘+D 행 복제 · T-182).
                        // 값 목록 팝업이 떠 있으면 표 편집 키(F2 · Ctrl+D)는 가로채지 않는다(검색 상자에 쳐야 한다 · 10-06).
                        if self.focus == Focus::Grid
                            && !self.palette.is_open()
                            && !self.grid.text_input_active()
                        {
                            if let Some(cmd) = keymap::grid_focus_command(&ch) {
                                if !kev.repeat {
                                    self.grid.edit_command(cmd);
                                    self.after_grid_event();
                                    self.redraw();
                                }
                                return;
                            }
                        }
                        if self.keymap.is_prefix(&ch) {
                            self.sess.status = tf(Msg::StChordPending, &[&ch.display()]);
                            self.pending_chord = Some(ch);
                            self.redraw();
                            return;
                        }
                        if let Some(id) = self.keymap.lookup(&ch) {
                            // ★ 자동 반복 사건은 편집·이동 명령만 실행(사용자 09-17: Ctrl+T를 누르고 있자 탭 80개 · 릴리스 뒤에도
                            //   밀린 사건이 계속 처리돼 UI가 막혔다). 한 번짜리 명령의 반복은 여기서 즉시 버린다.
                            if kev.repeat && !keymap::repeatable(id) {
                                return;
                            }
                            self.key_command(id, el);
                            return;
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.tmark("RedrawRequested");
                self.paint();
                return;
            }
            _ => {}
        }
        if let Some(ev) = self.ctl_event(&event) {
            self.route(ev);
        }
        // 사건 처리 중에 쌓인 "창 열기/닫기·종료" 요청을 한 번에 — `open_requested_windows`.
        self.open_requested_windows(el);
    }
}

/// `window_event`에서 뗀 두 단계(docs/93 §4 — 1,151줄 함수를 흐름만 남게).
impl App {
    /// 보조 창(파일·접속·설정·단축키·색·툴바 플로팅·변수·가져오기·SQL 미리보기·입력·메모리·About·라이선스·세션·
    /// 트랜잭션 로그·로그)의 사건을 그 창의 처리기로 보낸다. 처리했으면 `true`(메인 창 처리로 가지 않는다).
    fn aux_window_event(
        &mut self,
        el: &ActiveEventLoop,
        id: WindowId,
        event: &WindowEvent,
    ) -> bool {
        if self.file_win.is(id) {
            let ui_px = self.settings.font_px("ui.font_size");
            match self.file_win.handle(event) {
                FileWinAction::Paint => self.file_win.paint(&self.ui_font, &self.theme, ui_px),
                FileWinAction::Confirm(mode, path, enc) => {
                    self.remember_file_dialog(path.parent());
                    match (
                        mode,
                        std::mem::replace(&mut self.file_purpose, FilePurpose::Editor),
                    ) {
                        (PickerMode::Save, FilePurpose::LogExport) => {
                            // 로그 내보내기(현재 형식 · 보이는 줄 · UTF-8).
                            let text = self.log_win.export_text();
                            self.sess.status = match std::fs::write(&path, text) {
                                Ok(()) => tf(
                                    Msg::StLogSaved,
                                    &[
                                        &path.to_string_lossy(),
                                        &self.log_win.visible_len().to_string(),
                                    ],
                                ),
                                Err(e) => tf(Msg::ErrLogFile, &[&e.to_string()]),
                            };
                        }
                        (PickerMode::Folder, FilePurpose::SettingFolder(key)) => {
                            // 고른 폴더 → 설정(직접 입력한 것과 같은 길: 저장 · 반영 · 설정 창 갱신).
                            let value = path.to_string_lossy().into_owned();
                            match self.settings.set(key, &value) {
                                Ok(_) => {
                                    self.persist_settings();
                                    if !self.apply_setting(key) {
                                        self.sess.status = t(Msg::StNeedsRestart).into();
                                    }
                                    self.prefs_win.refresh(&self.settings);
                                }
                                Err(e) => self.prefs_win.set_error(key, e.to_string()),
                            }
                            self.prefs_win.redraw();
                        }
                        (PickerMode::Save, FilePurpose::CellValue) => {
                            let bytes = self.sqlprev_win.value_bytes();
                            let n = bytes.len();
                            self.sess.status = match std::fs::write(&path, bytes) {
                                Ok(()) => {
                                    tf(Msg::StCellSaved, &[&path.to_string_lossy(), &n.to_string()])
                                }
                                Err(e) => tf(Msg::ErrLogFile, &[&e.to_string()]),
                            };
                            self.sqlprev_win.set_note(self.sess.status.clone());
                        }
                        (PickerMode::Open, FilePurpose::CellValue) => {
                            self.cell_load_file(&path);
                        }
                        (PickerMode::Open, FilePurpose::License) => self.license_install(&path),
                        (PickerMode::Open, FilePurpose::Import) => {
                            if let Some(c) = self.import_ctx.as_mut() {
                                c.1 = path.to_path_buf();
                                self.import_pending = true;
                            }
                        }
                        (PickerMode::Save, FilePurpose::SqlPreview) => {
                            let text = self.sqlprev_win.text();
                            self.sess.status = match std::fs::write(&path, text) {
                                Ok(()) => tf(Msg::StSqlPreviewSaved, &[&path.to_string_lossy()]),
                                Err(e) => tf(Msg::ErrLogFile, &[&e.to_string()]),
                            };
                            self.sqlprev_win.set_note(self.sess.status.clone());
                        }
                        (PickerMode::Open, FilePurpose::Project) => self.project_load_path(&path),
                        (PickerMode::Save, FilePurpose::Project) => self.project_save_to(&path),
                        (PickerMode::Folder, FilePurpose::Project) => {
                            self.project_add_folder(&path)
                        }
                        (PickerMode::Folder, _) => {}
                        (PickerMode::Open, FilePurpose::RunFile) => {
                            self.load_file(&path, &enc, LoadMode::Run);
                        }
                        (PickerMode::Open, _) => self.open_file_enc(&path, &enc),
                        (PickerMode::Save, _) => {
                            self.editors.set_active_encoding(&enc);
                            self.save_to(&path);
                            self.finish_close_after_save();
                        }
                    }
                    self.sync_modal();
                }
                FileWinAction::ConfirmMany(paths, enc) => {
                    self.remember_file_dialog(paths.first().and_then(|p| p.parent()));
                    self.file_purpose = FilePurpose::Editor;
                    self.multi_open_ask(paths, enc);
                    self.sync_modal();
                }
                FileWinAction::Cancel => {
                    // 저장 창을 취소했다 = "저장하고 닫기"·"모두 저장"도 없던 일(탭은 그대로 · 종료 흐름 취소).
                    self.close_after_save = None;
                    if self.save_all_pending {
                        self.save_all_pending = false;
                        self.exit_pending = false;
                        self.exit_project_asked = false;
                    }
                    self.file_purpose = FilePurpose::Editor;
                    self.remember_file_dialog(None);
                    self.sync_modal();
                }
                FileWinAction::CopyText(text) => {
                    let _ = clipboard::write_text(&text);
                }
                FileWinAction::None => {
                    if !self.file_win.is_open() {
                        self.sync_modal();
                    }
                }
            }
            return true;
        }
        if self.conn_win.is(id) {
            for a in self.conn_win.handle(event) {
                self.handle_conn_win_action(a);
            }
            return true;
        }
        if self.prefs_win.is(id) {
            let ui_px = self.settings.font_px("ui.font_size");
            match self.prefs_win.handle(event) {
                PrefsAction::Paint => {
                    let mono_px = self.settings.font_px("editor.font_size");
                    self.prefs_win.paint(
                        &self.ui_font,
                        &self.theme,
                        ui_px,
                        &self.mono_font,
                        mono_px,
                    );
                }
                PrefsAction::Changed { key, value } => {
                    let ok = if value.is_empty()
                        && nsql_settings::entry(&key).is_some_and(|e| e.default.is_empty())
                    {
                        self.settings
                            .reset(&key)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    } else {
                        self.settings
                            .set(&key, &value)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    };
                    match ok {
                        Ok(()) => {
                            self.persist_settings();
                            if !self.apply_setting(&key) {
                                self.sess.status = t(Msg::StNeedsRestart).into();
                            }
                            self.prefs_win.refresh(&self.settings);
                        }
                        Err(e) => self.prefs_win.set_error(&key, e),
                    }
                    self.prefs_win.redraw();
                }
                PrefsAction::Reset(key) => {
                    let _ = self.settings.reset(&key);
                    self.persist_settings();
                    if !self.apply_setting(&key) {
                        self.sess.status = t(Msg::StNeedsRestart).into();
                    }
                    self.prefs_win.refresh(&self.settings);
                    self.prefs_win.redraw();
                }
                PrefsAction::OpenColors(key) => {
                    // hover/pressed는 기본 모드 · 그 밖의 `*_color` 키는 그 키 하나를 고르는 모드(09-17).
                    match nsql_settings::entry(&key) {
                        Some(e) if !matches!(e.key, "ui.hover_color" | "ui.pressed_color") => {
                            let v = color_setting(&self.settings, e.key);
                            self.colors_win.set_key_mode(e.key, v, &self.theme);
                        }
                        _ => self.colors_win.set_default_mode(&self.theme),
                    }
                    self.open_colors = true;
                }
                PrefsAction::OpenKeys => self.open_keys = true,
                PrefsAction::OpenOrder(key) => self.open_order_editor(&key),
                // 기본 포맷터 카드의 "설정" 바로가기 = 그 확장의 설정 분류로(사용자 09-30).
                PrefsAction::OpenExtSettings(id) => self.ext_open_settings(&id),
                // 분류 전환 = 미리보기 엔진이 바뀔 수 있다(확장 분류 ↔ Format 분류).
                PrefsAction::Category => {
                    self.prefs_format_preview_refresh();
                    let mono_px = self.settings.font_px("editor.font_size");
                    self.prefs_win.paint(
                        &self.ui_font,
                        &self.theme,
                        ui_px,
                        &self.mono_font,
                        mono_px,
                    );
                }
                // 설정 창이 다시 활성화됐다 → 파일 있음/없음·별칭 수를 다시 본다(파일 시스템만 · 네트워크 0 · 라이브러리 로드 0).
                PrefsAction::RefreshInfo => {
                    self.prefs_win.set_info(dbms_info_values());
                    self.prefs_win.refresh(&self.settings);
                    self.prefs_win.redraw();
                }
                PrefsAction::BrowseFolder { key, current } => {
                    // 폴더 전용 대화상자(파일은 보이지 않는다) — 시작 = 지금 값(있고 폴더면) · 고르면 그 설정에 넣는다.
                    if let Some(k) = nsql_settings::entry(&key).map(|e| e.key) {
                        self.file_purpose = FilePurpose::SettingFolder(k);
                        self.folder_start =
                            Some(PathBuf::from(current.trim())).filter(|p| p.is_dir());
                        self.open_file_dlg = Some(PickerMode::Folder);
                    }
                }
                PrefsAction::EditJson => self.edit_settings_json(),
                PrefsAction::None => {}
            }
            return true;
        }
        if self.keys_win.is(id) {
            let ui_px = self.settings.font_px("ui.font_size");
            match self.keys_win.handle(event) {
                KeysAction::Paint => self.keys_win.paint(&self.ui_font, &self.theme, ui_px),
                KeysAction::Changed { id, code } => {
                    let _ = self.settings.set(&keymap::setting_key(&id), &code);
                    let _ = self.settings.save();
                    self.keymap = Keymap::from_settings(&self.settings);
                    self.sync_menu_shortcuts();
                    self.apply_menu_decor();
                    self.keys_win.refresh(&self.keymap);
                    self.keys_win.redraw();
                }
                KeysAction::ResetAll => {
                    for c in keymap::COMMANDS {
                        let _ = self.settings.reset(&keymap::setting_key(c.id));
                    }
                    let _ = self.settings.save();
                    self.keymap = Keymap::from_settings(&self.settings);
                    self.sync_menu_shortcuts();
                    self.apply_menu_decor();
                    self.keys_win.refresh(&self.keymap);
                    self.keys_win.redraw();
                }
                KeysAction::None => {}
            }
            return true;
        }
        if self.colors_win.is(id) {
            let ui_px = self.settings.font_px("ui.font_size");
            match self.colors_win.handle(event, &self.theme) {
                ColorsAction::Paint => self.colors_win.paint(&self.ui_font, &self.theme, ui_px),
                ColorsAction::Changed { target, hex } => {
                    apply_color(target, Some(&hex));
                    let _ = self.settings.set(target.key(), &hex);
                    if let ColorTarget::Key(k) = target {
                        self.apply_setting(k);
                        self.prefs_win.refresh(&self.settings);
                        self.prefs_win.redraw();
                    }
                    let recent = self
                        .colors_win
                        .recent()
                        .iter()
                        .map(|c| format!("#{c:08X}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    let _ = self.settings.set("ui.color_recent", &recent);
                    let _ = self.settings.save();
                    self.redraw();
                    self.conn_win.redraw();
                }
                ColorsAction::Reset => {
                    if let Some(k) = self.colors_win.key_mode_key() {
                        let _ = self.settings.reset(k);
                        self.apply_setting(k);
                        self.prefs_win.refresh(&self.settings);
                        self.prefs_win.redraw();
                    } else {
                        for tg in ColorTarget::ALL {
                            apply_color(tg, None);
                            let _ = self.settings.reset(tg.key());
                        }
                    }
                    let _ = self.settings.save();
                    self.redraw();
                    self.conn_win.redraw();
                }
                ColorsAction::None => {}
            }
            return true;
        }
        if let Some(fi) = self.tool_floats.iter().position(|f| f.is(id)) {
            let gid = self.tool_floats[fi].group.clone();
            match self.tool_floats[fi].handle(event) {
                FloatAction::Paint => {
                    if let Some(bar) = self.tool_dock.bar_mut(&gid) {
                        self.tool_floats[fi].paint(bar, &self.ui_font, &self.theme);
                    }
                }
                FloatAction::Input(ev) => {
                    let client = self.tool_floats[fi].client();
                    let sc = self.tool_floats[fi].scale();
                    let mut inv = Invalidations::default();
                    let mut clicked = None;
                    if let Some(bar) = self.tool_dock.bar_mut(&gid) {
                        bar.set_scale(sc);
                        bar.set_bounds(client, &mut inv);
                        bar.on_event(&ev, &mut inv);
                        clicked = bar.take_clicked();
                    }
                    if matches!(ev, InputEvent::RightDown { .. }) {
                        // 플로팅 창 우클릭 = 붙이기(한 번의 입력으로 · 팝업 규칙).
                        self.dock_group(&gid);
                        return true;
                    }
                    if !inv.is_empty() {
                        self.tool_floats[fi].redraw();
                    }
                    if let Some(cmd) = clicked {
                        self.menu_action(&cmd);
                        self.redraw();
                    }
                }
                FloatAction::Moved(x, y) => {
                    self.tool_dock.set_floating_pos(&gid, x, y);
                    self.drain_dock_actions();
                }
                FloatAction::Close => self.dock_group(&gid),
                FloatAction::None => {}
            }
            return true;
        }
        if self.vars_win.is(id) {
            match self.vars_win.handle(event) {
                vars_win::VarsWinAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    let rows = self.vars_rows();
                    self.vars_win
                        .paint(&rows, &self.ui_font, &self.theme, ui_px);
                }
                other => self.vars_apply(other),
            }
            return true;
        }
        if self.import_win.is(id) {
            match self.import_win.handle(event) {
                import_win::ImportAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    let mono_px = self.settings.font_px("editor.font_size");
                    self.import_win.paint(
                        &self.ui_font,
                        &self.mono_font,
                        &self.theme,
                        ui_px,
                        mono_px,
                    );
                }
                import_win::ImportAction::Start(spec) => self.import_start(spec),
                import_win::ImportAction::Cancel => self.import_cancel(),
                import_win::ImportAction::Close => {
                    self.import_win.close();
                    self.import_ctx = None;
                    self.sync_modal();
                    self.redraw();
                }
                import_win::ImportAction::None => {}
            }
            return true;
        }
        if self.sqlprev_win.is(id) {
            match self.sqlprev_win.handle(event) {
                sqlprev_win::SqlPrevAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    let mono_px = self.settings.font_px("editor.font_size");
                    self.sqlprev_win.paint(
                        &self.ui_font,
                        &self.mono_font,
                        &self.theme,
                        ui_px,
                        mono_px,
                    );
                }
                sqlprev_win::SqlPrevAction::Refresh => {
                    // 체크박스로 바꾼 옵션은 설정(`gen.*`)에도 남기고 탐색기의 다음 생성에도 쓴다.
                    let o = self.sqlprev_win.opts();
                    for (k, v) in [
                        ("gen.qualified", o.qualified),
                        ("gen.compact", o.compact),
                        ("gen.full_ddl", o.full_ddl),
                        ("gen.separate_fk", o.separate_fk),
                    ] {
                        let cur = self.settings.flag(k);
                        if cur != v {
                            let _ = self.settings.set(k, if v { "on" } else { "off" });
                        }
                    }
                    self.persist_settings();
                    self.explorer.set_gen_opts(o);
                    if let Some(spec) = self.sqlprev_win.spec.clone() {
                        let server = self.sqlprev_win.server.clone();
                        self.sqlprev_win
                            .set_note(tf(Msg::StGenerating, &[&spec.title()]));
                        self.explorer.gen_sql(server.as_ref(), spec);
                    }
                }
                sqlprev_win::SqlPrevAction::Save => {
                    self.file_purpose = FilePurpose::SqlPreview;
                    self.open_file_dlg = Some(PickerMode::Save);
                }
                sqlprev_win::SqlPrevAction::SaveValue => {
                    self.file_purpose = FilePurpose::CellValue;
                    self.open_file_dlg = Some(PickerMode::Save);
                }
                sqlprev_win::SqlPrevAction::LoadFile => {
                    self.file_purpose = FilePurpose::CellValue;
                    self.open_file_dlg = Some(PickerMode::Open);
                }
                sqlprev_win::SqlPrevAction::ApplyCell { row, col, text } => {
                    match self.grid.set_cell_value(row, col, Some(text)) {
                        Ok(()) => {
                            self.sqlprev_win.set_note(t(Msg::StCellApplied).to_string());
                            self.after_grid_event();
                        }
                        Err(m) => self.sqlprev_win.set_note(m),
                    }
                    self.redraw();
                }
                sqlprev_win::SqlPrevAction::Mode(m) => {
                    // 이미지 모드는 Pro(25 §13-3) — 창이 먼저 바꿨으면 Hex로 되돌리고 안내.
                    if m == sqlprev_win::ValueMode::Image
                        && !self.lic_gate(nsql_license::Feature::LobImage)
                    {
                        self.sqlprev_win.set_mode(sqlprev_win::ValueMode::Hex);
                    }
                }
                sqlprev_win::SqlPrevAction::OpenEditor => {
                    let title = self.sqlprev_win.file_name();
                    let text = self.sqlprev_win.text();
                    // ★ DDL 미리보기로 연 탭도 객체 탭(10-01): 종류가 소스 객체(뷰·루틴·트리거·타입)면 출처를 기억해 F5 = 한 단위.
                    let origin = self.sqlprev_win.spec.as_ref().and_then(|sp| {
                        (sp.what == nsql_catalog::GenWhat::Ddl
                            && sp.sub.is_none()
                            && sp.owner.kind.has_source())
                        .then(|| explorer::ObjectOrigin {
                            schema: sp.owner.schema.clone(),
                            name: sp.owner.name.clone(),
                            kind: sp.owner.kind,
                            server: self.sqlprev_win.server.clone(),
                        })
                    });
                    self.sqlprev_win.close();
                    if self.tab_room() {
                        self.editors.new_tab(Some(title));
                        self.editors.cur_mut().set_text(&text);
                        // 캐럿 = 1행 1열(파일 열기와 같이 · 사용자 09-30).
                        self.editors.cur_mut().goto_line(1);
                        self.set_focus(Focus::Editor);
                        if let Some(o) = origin {
                            let tab = self.editors.active_id();
                            if let Some(spec) = o.server.clone() {
                                self.bind_tab_to_server(tab, &spec);
                            }
                            self.object_tabs.insert(tab, o);
                            self.sync_sess();
                            self.sync_sess_ui();
                        }
                    }
                    self.sync_modal();
                }
                sqlprev_win::SqlPrevAction::Copy => {
                    let text = self.sqlprev_win.copy_text();
                    let n = text.chars().count();
                    if clipboard::write_text(&text) {
                        self.sqlprev_win
                            .set_note(tf(Msg::SpCopied, &[&n.to_string()]));
                    } else {
                        self.sqlprev_win.set_note(t(Msg::ErrClipboard).to_string());
                    }
                }
                sqlprev_win::SqlPrevAction::Close => {
                    self.sqlprev_win.close();
                    self.sync_modal();
                }
                sqlprev_win::SqlPrevAction::None => {
                    if self.conn_modal && !self.modal_open() {
                        self.sync_modal();
                    }
                }
            }
            return true;
        }
        if self.input_win.is(id) {
            match self.input_win.handle(event) {
                input_win::InputWinAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    self.input_win.paint(&self.ui_font, &self.theme, ui_px);
                }
                input_win::InputWinAction::Run(values) => {
                    self.input_reply(worker::InputReply::Values(values));
                }
                input_win::InputWinAction::Password(secret) => {
                    self.password_reply(worker::PwReply::Value(secret));
                    self.sync_modal();
                }
                input_win::InputWinAction::Cancel if self.input_win.is_password() => {
                    self.password_reply(worker::PwReply::Cancel);
                    self.sync_modal();
                }
                input_win::InputWinAction::Skip => {
                    self.input_reply(worker::InputReply::Values(Vec::new()));
                }
                input_win::InputWinAction::Cancel => {
                    self.input_reply(worker::InputReply::Cancel);
                }
                input_win::InputWinAction::None => {
                    // 모달이었다가 닫혔으면(창 닫기 등) 메인을 다시 살린다.
                    if self.conn_modal && !self.modal_open() {
                        self.sync_modal();
                    }
                }
            }
            return true;
        }
        if self.order_win.is(id) {
            match self.order_win.handle(event) {
                order_win::OrderWinAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    self.order_win.paint(&self.ui_font, &self.theme, ui_px);
                }
                order_win::OrderWinAction::Changed { key, value } => {
                    self.order_changed(key, &value)
                }
                order_win::OrderWinAction::Close => self.order_win.close(),
                order_win::OrderWinAction::None => {}
            }
            return true;
        }
        if self.mem_win.is(id) {
            match self.mem_win.handle(event) {
                mem_win::MemWinAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    self.mem_win.paint(&self.ui_font, &self.theme, ui_px);
                }
                mem_win::MemWinAction::Toggled(on) => {
                    let _ = self
                        .settings
                        .set("mem.always_on_top", if on { "on" } else { "off" });
                }
                mem_win::MemWinAction::Trim => self.mem_trim_now(),
                mem_win::MemWinAction::Close => {
                    self.mem_win.close();
                    self.persist_window_sizes(false);
                    self.redraw();
                }
                mem_win::MemWinAction::None => {}
            }
            return true;
        }
        if self.drop_win.is(id) {
            let a = self.drop_win.handle(event);
            self.drop_action(a);
            return true;
        }
        if self.about_win.is(id) {
            match self.about_win.handle(event) {
                AboutAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    let lines = self.about_lines();
                    self.about_win
                        .paint(lines, &self.ui_font, &self.theme, ui_px);
                }
                AboutAction::Close => {
                    self.about_win.close();
                    self.sync_modal();
                    self.redraw();
                }
                AboutAction::Copy => {
                    let text = self.about_win.info_text();
                    if clipboard::write_text(&text) {
                        self.about_win.set_note(t(Msg::AboutCopied).to_string());
                    }
                }
                AboutAction::OpenLicense => {
                    self.open_license = true;
                    self.redraw();
                }
                AboutAction::None => {}
            }
            return true;
        }
        if self.license_win.is(id) {
            match self.license_win.handle(event) {
                LicAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    let view = self.license_view();
                    self.license_win
                        .paint(view, &self.ui_font, &self.theme, ui_px);
                }
                LicAction::Close => {
                    self.license_win.close();
                    self.sync_modal();
                    self.redraw();
                }
                LicAction::OpenFile => {
                    self.file_purpose = FilePurpose::License;
                    self.open_file_dlg = Some(PickerMode::Open);
                    self.redraw();
                }
                LicAction::Remove => self.license_remove(),
                LicAction::CopyRequest(name, email) => self.license_copy_request(&name, &email),
                LicAction::CopyText(text) => {
                    let hold = self.settings.int("ui.flash_hold_ms").clamp(0, 30_000) as u64;
                    let fade = self.settings.int("ui.flash_ms").clamp(200, 30_000) as u64;
                    let ok = clipboard::write_text(&text);
                    // 글 복사(이메일 주소 등) = 일반 문구 — 요청 코드 문구(`LicNoteCopied`)는 `CopyRequest`에서만(협업 10-07).
                    let msg = if ok {
                        t(Msg::LicNoteTextCopied).to_string()
                    } else {
                        t(Msg::LicNoteCopyFailed).to_string()
                    };
                    self.license_win.set_flash(msg, !ok, hold, fade);
                }
                LicAction::None => {}
            }
            return true;
        }
        if self.sessions_win.is(id) {
            // ★ 그린 직후에 다시 그리기를 요청하면 끝없이 돈다(세션 창을 열어 두면 유휴 CPU 90% · 09-19 메모리 점검에서 발견).
            let action = self.sessions_win.handle(event);
            let painted = matches!(action, SessWinAction::Paint);
            match action {
                SessWinAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    let rows = self.session_rows();
                    self.sessions_win
                        .paint(&rows, &self.ui_font, &self.theme, ui_px);
                }
                SessWinAction::Activate(sid) => self.activate_shared(sid),
                SessWinAction::Reconnect(sid) => self.disconnect_pick(&format!("conn.again:{sid}")),
                SessWinAction::Disconnect(sid) => self.disconnect_session(sid),
                SessWinAction::DisconnectAll => self.disconnect_all(),
                SessWinAction::GoTab(tab) => {
                    self.editors.switch_to_id(tab);
                    self.sync_grid_tab();
                    if let Some(w) = &self.window {
                        w.focus_window();
                    }
                    self.redraw();
                }
                SessWinAction::None => {}
            }
            if !painted {
                self.sessions_win.redraw();
            }
            return true;
        }
        if self.txlog_win.is(id) {
            let act = self.txlog_win.handle(event);
            match act {
                TxLogAction::Paint => {
                    let ui_px = self.settings.font_px("ui.font_size");
                    self.txlog_win.set_active_editor(self.editors.active_id());
                    self.txlog_win
                        .paint(&self.txlog, &self.ui_font, &self.theme, ui_px);
                }
                TxLogAction::CopySql(eid) | TxLogAction::CopyBoundSql(eid) => {
                    let bound = matches!(act, TxLogAction::CopyBoundSql(_));
                    if let Some(e) = self.txlog.entry(eid) {
                        let text = if bound { e.bound_sql() } else { e.text.clone() };
                        if !clipboard::write_text(&text) {
                            self.sess.status = t(Msg::ErrClipboard).into();
                        }
                    }
                }
                TxLogAction::OpenSql(eid) | TxLogAction::OpenBoundSql(eid) => {
                    let bound = matches!(act, TxLogAction::OpenBoundSql(_));
                    let text = self.txlog.entry(eid).map(|e| {
                        if bound {
                            e.bound_sql()
                        } else {
                            e.text.clone()
                        }
                    });
                    if let Some(text) = text.filter(|_| self.tab_room()) {
                        self.editors.new_tab(None);
                        self.editors.cur_mut().set_text(&text);
                        self.set_focus(Focus::Editor);
                        if let Some(w) = &self.window {
                            w.focus_window();
                        }
                        self.redraw();
                    }
                }
                TxLogAction::None => {}
            }
            return true;
        }
        if self.log_win.is(id) {
            match self.log_win.handle(event) {
                LogWinAction::Paint => {
                    // 시스템 UI 글꼴 · 본문 = 편집기 기본 크기 · 푸터 = 메인 상태줄 크기(사용자 09-16).
                    let body_px = self.settings.font_px("editor.font_size");
                    let footer_px = nexa_ctl::theme::FontPrefs::default().status.size;
                    self.log_win
                        .paint(&self.ui_font, &self.theme, body_px, footer_px);
                }
                LogWinAction::Toggled(key, on) => {
                    // 스위치 = 설정과 같은 값(자동 기억 · 설정 창에도 반영).
                    let _ = self.settings.set(key, if on { "on" } else { "off" });
                    let _ = self.settings.save();
                    if key == "log.dev_mode" {
                        self.apply_detail_mask();
                    }
                }
                LogWinAction::Setting(key, value) => {
                    let _ = self.settings.set(key, &value);
                    let _ = self.settings.save();
                    if key == "log.dev_layers" {
                        self.apply_detail_mask();
                    }
                }
                LogWinAction::SaveAs if !self.lic_gate(nsql_license::Feature::LogExport) => {}
                LogWinAction::SaveAs => {
                    self.file_purpose = FilePurpose::LogExport;
                    self.open_file_window(el, PickerMode::Save);
                    self.sync_modal();
                }
                LogWinAction::CopyText(text) => {
                    let _ = clipboard::write_text(&text);
                }
                LogWinAction::None => {}
            }
            return true;
        }
        false
    }

    /// 사건 처리 중에 켜진 "창 열기·토글·종료" 요청 깃발을 소비한다(깃발 = 처리기 안에서 `el`이 없을 때 미뤄 둔 요청).
    fn open_requested_windows(&mut self, el: &ActiveEventLoop) {
        if std::mem::take(&mut self.toggle_log) {
            self.toggle_log_window(el);
        }
        if std::mem::take(&mut self.open_txlog) {
            self.open_txlog_window(el);
        }
        if std::mem::take(&mut self.open_vars) {
            self.open_vars_window(el);
        }
        if std::mem::take(&mut self.open_sessions) {
            self.open_sessions_window(el);
        }
        if std::mem::take(&mut self.open_license) {
            self.open_license_window(el);
        }
        if std::mem::take(&mut self.open_drop) {
            self.open_drop_window(el);
        }
        if std::mem::take(&mut self.open_about) {
            let owner = self.window.clone();
            let was_open = self.about_win.is_open();
            self.about_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                owner.as_deref(),
            );
            if !was_open {
                if let (Some(o), Some(c)) = (owner.as_deref(), self.about_win.window()) {
                    winfocus::attach_child(o, c);
                }
            }
            self.sync_modal();
        }
        if std::mem::take(&mut self.open_mem) {
            self.open_mem_window(el);
        }
        if std::mem::take(&mut self.open_order) {
            self.open_order_window(el);
        }
        if std::mem::take(&mut self.open_colors) {
            // ★ 설정 창에서 열면 **설정 창을 소유자**로(그 위에 뜬다 · 메인 소유면 설정 창 뒤로 숨어 "안 열린 것처럼" 보이던 결함 · 사용자 09-15).
            let parent: Option<&Window> = self.prefs_win.window().or(self.window.as_deref());
            let over = parent.and_then(|w| {
                let p = w.outer_position().ok()?;
                let sz = w.outer_size();
                Some((p.x, p.y, sz.width, sz.height))
            });
            self.colors_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                over,
                parent,
            );
            if let Some(w) = self.colors_win.window() {
                w.focus_window();
            }
        }
        if std::mem::take(&mut self.open_prefs) {
            self.open_prefs_now(el);
        }
        if std::mem::take(&mut self.open_keys) {
            // 설정 창에서 열면 설정 창을 소유자로(색 창과 같은 이유).
            let parent: Option<&Window> = self.prefs_win.window().or(self.window.as_deref());
            let over = parent.and_then(|w| {
                let p = w.outer_position().ok()?;
                let sz = w.outer_size();
                Some((p.x, p.y, sz.width, sz.height))
            });
            self.keys_win.refresh(&self.keymap);
            self.keys_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                over,
                parent,
            );
            if let Some(w) = self.keys_win.window() {
                w.focus_window();
            }
        }
        if std::mem::take(&mut self.open_conn) {
            self.open_conn_window(el);
            self.sync_modal();
        }
        if let Some(mode) = self.open_file_dlg.take() {
            self.open_file_window(el, mode);
            self.sync_modal();
        }
        if self.exit_requested {
            self.finish_exit(el);
        }
    }
}

impl App {
    /// 설정 창 열기 본체(사건 경로 `open_requested_windows`와 펌프 `pump_window_requests`가 같이 쓴다 · T-305).
    fn open_prefs_now(&mut self, el: &ActiveEventLoop) {
        let over = self.window.as_ref().and_then(|w| {
            let p = w.outer_position().ok()?;
            let sz = w.outer_size();
            Some((p.x, p.y, sz.width, sz.height))
        });
        let owner = self.window.clone();
        self.prefs_win.set_info(dbms_info_values());
        self.prefs_win
            .set_advanced(self.settings.flag("ui.prefs_advanced"));
        self.prefs_win.refresh(&self.settings);
        if let Some(q) = self.prefs_query.take() {
            self.prefs_win.preset_query(&q);
        }
        // 확장 패널 "설정" 버튼 = 그 확장의 분류로(사용자 09-30).
        if let Some(cat) = self.prefs_category.take() {
            self.prefs_win.select_category(cat);
        }
        self.prefs_win.open(
            el,
            theme::window_theme(self.settings.theme_mode()),
            over,
            owner.as_deref(),
        );
        // ★ Format 분류의 미리보기 글(사용자 09-29) — 열 때 한 번 · 이후는 값이 바뀔 때마다.
        self.prefs_format_preview_refresh();
    }

    /// ★ 이벤트 없이도 열어야 하는 창·대화상자 요청(메뉴·기동 명령·워커의 물음 · 깃발 = `open_*`/`*_pending`) — `about_to_wait`
    ///   의 앞 단계(T-248 · 09-29 분리 · 행동 보존): 보조 창 · 파일 대화상자 · SQL 미리보기/값 보기 · Import · 입력/비밀번호 창 ·
    ///   저장 물음 · 데모 · 툴바 플로팅 · 툴바 배치 저장.
    fn pump_window_requests(&mut self, el: &ActiveEventLoop) {
        // 기동 명령·타이머가 부탁한 변수 창(창 이벤트가 없어도 열리게).
        // 창 열기 깃발은 이벤트가 없을 때도 본다(메뉴·기동 명령이 부탁한 창 — `window_event` 끝에서만 보면 늦게 열린다 · docs/61 §6 흠 ⑤).
        if std::mem::take(&mut self.open_txlog) {
            self.open_txlog_window(el);
        }
        if std::mem::take(&mut self.open_sessions) {
            self.open_sessions_window(el);
        }
        if std::mem::take(&mut self.open_license) {
            self.open_license_window(el);
        }
        if std::mem::take(&mut self.open_drop) {
            self.open_drop_window(el);
        }
        if std::mem::take(&mut self.open_about) {
            let owner = self.window.clone();
            let was_open = self.about_win.is_open();
            self.about_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                owner.as_deref(),
            );
            if !was_open {
                if let (Some(o), Some(c)) = (owner.as_deref(), self.about_win.window()) {
                    winfocus::attach_child(o, c);
                }
            }
            self.sync_modal();
        }
        if std::mem::take(&mut self.open_mem) {
            self.open_mem_window(el);
        }
        if std::mem::take(&mut self.open_order) {
            self.open_order_window(el);
        }
        if std::mem::take(&mut self.open_vars) {
            self.open_vars_window(el);
        }
        // 설정 창도 사건 없이(기동 명령 `edit.prefs:<검색어>` · T-305).
        if std::mem::take(&mut self.open_prefs) {
            self.open_prefs_now(el);
        }
        // 파일·폴더 대화상자도 같다(설정 창의 "찾아보기…" · 기동 명령 — 메인 창에 사건이 없으면 열리지 않았다).
        if let Some(mode) = self.open_file_dlg.take() {
            self.open_file_window(el, mode);
            self.sync_modal();
        }
        // ★ 그리드 편집: SQL 미리보기 · 셀 값 보기(읽기 전용 글 창 · docs/87).
        if let Some((title, text)) = self.sqlprev_plain.take() {
            let owner = self.window.clone();
            let syntax = self.editors.syntax_for_title("preview.sql");
            let tb = self.editors.preview_box("", &syntax);
            let was_open = self.sqlprev_win.is_open();
            self.sqlprev_win.open_plain(
                el,
                theme::window_theme(self.settings.theme_mode()),
                owner.as_deref(),
                title,
                text,
                tb,
            );
            if !was_open {
                if let (Some(o), Some(c)) = (owner.as_deref(), self.sqlprev_win.window()) {
                    winfocus::attach_child(o, c);
                }
            }
            self.sync_modal();
        }
        // ★ 값 보기 창(87 §5): 글은 Plain Text 상자(편집 가능 셀이면 편집) · 이진은 16진수/이미지.
        if let Some(v) = self.sqlprev_value.take() {
            let owner = self.window.clone();
            let syntax = self.editors.syntax_for_title("value.txt");
            let tb = self.editors.preview_box("", &syntax);
            let was_open = self.sqlprev_win.is_open();
            let max_mb = self.settings.int("grid.lob_view_max_mb").max(1) as usize;
            let image_on = self.settings.flag("grid.lob_image_preview");
            self.sqlprev_win.open_value(
                el,
                theme::window_theme(self.settings.theme_mode()),
                owner.as_deref(),
                tb,
                v,
                max_mb,
                image_on,
            );
            if !was_open {
                if let (Some(o), Some(c)) = (owner.as_deref(), self.sqlprev_win.window()) {
                    winfocus::attach_child(o, c);
                }
            }
            self.sync_modal();
        }
        // Generate SQL 결과 → SQL Preview 모달(docs/83 §4).
        if let Some((spec, r, server)) = self.sqlprev_pending.take() {
            let owner = self.window.clone();
            let syntax = self.editors.syntax_for_title("preview.sql");
            let tb = self.editors.preview_box("", &syntax);
            let was_open = self.sqlprev_win.is_open();
            self.sqlprev_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                owner.as_deref(),
                spec,
                server,
                r,
                tb,
            );
            // ★ 맥 자식 창(비밀번호 창과 같은 길 · 사용자 09-25 "DDL 팝업이 메인 뒤로 숨는다"): 메인의 자식으로 붙여 늘 위에.
            if !was_open {
                if let (Some(o), Some(c)) = (owner.as_deref(), self.sqlprev_win.window()) {
                    winfocus::attach_child(o, c);
                }
            }
            self.sync_modal();
        }
        // ★ Import 창(89 §3-3): 파일 창 결과(또는 기동 명령 `import.open:`) → 미리보기 6줄 + 설정 `bulk.*` 기본값으로 연다.
        if std::mem::take(&mut self.import_pending) {
            if let Some((table, path, _)) = self.import_ctx.clone() {
                let owner = self.window.clone();
                let was_open = self.import_win.is_open();
                let preview = nsql_run::bulk::preview_head(&path, 6, 64 * 1024);
                let batch = self.settings.int("bulk.batch_rows").max(1) as usize;
                let commit = self.settings.int("bulk.commit_every").max(0) as usize;
                let mode = self
                    .settings
                    .get("bulk.mode")
                    .and_then(nsql_run::bulk::BulkMode::parse)
                    .unwrap_or_default();
                self.import_win.open(
                    el,
                    theme::window_theme(self.settings.theme_mode()),
                    owner.as_deref(),
                    table,
                    path,
                    preview,
                    (batch, commit, mode),
                );
                if !was_open {
                    if let (Some(o), Some(c)) = (owner.as_deref(), self.import_win.window()) {
                        winfocus::attach_child(o, c);
                    }
                }
                self.sync_modal();
            }
        }
        // 워커가 실행 전에 값을 묻는다(D-137) → 입력 창.
        if let Some((sid, needs)) = self.input_pending.take() {
            let owner = self.window.clone();
            let was_open = self.input_win.is_open();
            self.input_win.open(
                el,
                theme::window_theme(self.settings.theme_mode()),
                owner.as_deref(),
                needs,
                sid,
            );
            // ★ 09-25 결함(사용자 "SELECT :Top 실행이 끝나지 않는다"): 비밀번호 길과 달리 자식 창으로 붙이지 않아 맥에서 메인 뒤로
            //   숨었고, 워커는 답을 기다리며 멎었다(중지로만 풀림) → 같은 길(자식 창 + 모달 동기화).
            if !was_open {
                if let (Some(o), Some(c)) = (owner.as_deref(), self.input_win.window()) {
                    winfocus::attach_child(o, c);
                }
            }
            self.sync_modal();
        }
        // 저장하지 않은 탭을 닫으려 했다(X · 단축키 · 탭 메뉴 · 모두 닫기 — 어느 길이든 여기서 걷는다).
        if let Some(i) = self.editors.take_save_close_request() {
            self.ask_save_close(i);
        }
        // 워커가 비밀번호를 묻는다 → 같은 입력 창의 비밀번호 모드(다른 물음이 떠 있으면 그 뒤에).
        if self.pw_pending.is_some() && !self.input_win.is_open() {
            if let Some((sid, target, rejected)) = self.pw_pending.take() {
                let owner = self.window.clone();
                self.input_win.open_password(
                    el,
                    theme::window_theme(self.settings.theme_mode()),
                    owner.as_deref(),
                    input_win::PasswordAsk {
                        target,
                        rejected,
                        remember: self.settings.flag("connect.remember_session_password"),
                    },
                    sid,
                );
                // ★ 최상위 모달(사용자 mac 09-21): 맥은 자식 창(항상 메인 위 · 함께 이동) · 메인·보조 창 입력은 가드가 막고
                //   Windows는 `EnableWindow(FALSE)`(`sync_modal`). 접속 창과 같은 길.
                if let (Some(o), Some(c)) = (owner.as_deref(), self.input_win.window()) {
                    winfocus::attach_child(o, c);
                }
                self.sync_modal();
            }
        }
        if std::mem::take(&mut self.pending_demo_prompt) {
            self.open_demo_prompt();
        }
        if let Some(rx) = self.demo_job.as_ref() {
            if let Ok(r) = rx.try_recv() {
                self.demo_job = None;
                self.finish_demo(r);
            }
        }
        // 툴바 떼어 내기 요청 → 플로팅 창(창 생성은 이벤트 루프 핸들이 있는 여기서) · 바뀐 배치는 한 번에 저장.
        if !self.pending_float.is_empty() {
            let reqs = std::mem::take(&mut self.pending_float);
            for (gid, at) in reqs {
                self.open_float(el, &gid, at);
            }
        }
        if self.tool_layout_dirty {
            self.save_tool_layout();
        }
    }
}
