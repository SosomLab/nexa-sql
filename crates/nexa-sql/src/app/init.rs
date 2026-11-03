//! App — 기동 초기화(221차 T-248 ③ · 11-01 · docs/93 §8): `main()`이 `App`을 만든 **뒤** 하던 일 두 묶음을 옮김(동작 불변 · `app.` → `self.`).
//!   ① `init_apply_settings` = 설정값을 컨트롤·전역 토큰·드라이버 허브에 한 번 적용(기동 1회 · 바뀐 설정은 `apply_setting`이 따로 처리)
//!   ② `init_startup_target` = 실행 인자 접속(프로필 이름 또는 접속 문자열 · 사용자 09-17 · T-104).
//!   순서는 종전 그대로(① 뒤 ②) · `main()`은 인자 해석 · 창 백엔드 · App 조립 · 기동 계측만 남긴다.

use crate::*;

impl App {
    /// 설정 → 컨트롤·전역 토큰·드라이버 허브 1회 적용(종전 `main()` App 리터럴 뒤 단락 · 221차 이동).
    /// `row_snap`·`rulers`·`ws_style` = `main`이 App 조립에도 쓰는 값이라 그대로 받는다 · `net_proxy` = L0 네트워크 신호용(`net.watch` 꺼져 있으면 버림).
    pub(crate) fn init_apply_settings(
        &mut self,
        probe_hub: probe::ProbeHub,
        max_rows: usize,
        net_proxy: EventLoopProxy<Wake>,
        row_snap: bool,
        rulers: Vec<usize>,
        ws_style: nexa_ctl::WhitespaceStyle,
    ) {
        self.grid.set_row_snap(row_snap);
        self.apply_fast_scroll();
        self.apply_result_tab_opts();
        // 검색어 이력(전역 한 벌)을 상자를 가진 곳마다 건넨다(찾기/바꾸기 · 파일 검색 · 프로젝트/북마크/아웃라인/확장 필터 · 설정 검색 · 사용자 09-23).
        {
            let h = self.search_history.clone();
            self.find.set_history(h.clone());
            self.search.set_history(h.clone());
            self.project_panel.set_history(h.clone());
            self.explorer.set_history(h.clone());
            // 글로벌 변수 층을 첫 세션에(docs/63 §11).
            self.sess
                .control(worker::Cmd::GlobalVars(self.global_vars.clone()));
            self.bm_panel.set_history(h.clone());
            self.outline_panel.set_history(h.clone());
            self.ext_panel.set_history(h.clone());
            self.prefs_win.set_history(h);
        }
        {
            let pct = self.settings.int("grid.row_height_pct").clamp(110, 300) as i32;
            self.grid.set_row_pct(pct);
        }
        self.editors
            .set_scroll_snap(self.settings.get("editor.scroll") == Some("row"));
        self.apply_minimap();
        self.apply_line_height();
        // 자원 거버너 상한(T-90d · 부팅 1회) — 실효 값(`perf.mode` 반영).
        self.log_win
            .set_max_lines(self.settings.int("log.max_lines").max(100) as usize);
        self.editors
            .set_undo_max(self.settings.int("editor.undo_max").max(1) as usize);
        self.editors.set_large_cfg(large_cfg(&self.settings));
        {
            let (ext, syn) = large_feature_levels(&self.settings);
            self.editors.set_large_feature_levels(ext, syn);
        }
        self.editors.set_undo_budget(
            self.settings.int("editor.undo_budget_mb").max(1) as usize * 1024 * 1024,
        );
        let (ms, giant) = undo_rules(&self.settings);
        self.editors.set_undo_rules(ms, giant);
        let cap = self.occurrence_cap();
        self.editors.set_max_regions(cap);
        nexa_gfx::text::set_glyph_cache_max(self.settings.int("ui.glyph_cache").max(256) as usize);
        nexa_fs::shell::set_icon_cache_max(self.settings.int("file.icon_cache").max(16) as usize);
        // D-58: full 모드인데 배터리/원격 세션이면 1회 안내.
        if let Some(m) = self.settings.perf_hint() {
            self.sess.status = t(m).into();
        }
        let null_text = self
            .settings
            .get("grid.null_text")
            .unwrap_or("NULL")
            .to_string();
        self.grid.set_null_text(&null_text);
        self.apply_grid_row_focus();
        self.apply_grid_edit_cfg();
        self.sync_menu_shortcuts();
        self.apply_click_policy();
        self.bookmarks.apply_settings(&self.settings);
        let bm_on = self.bookmarks.enabled;
        self.act_bar.set_item_visible("view.bookmarks", bm_on);
        nexa_ctl::controls::set_menu_icons(self.settings.flag("ui.menu_icons"));
        // ★ 모든 우클릭 메뉴에 아이콘 칸을 늘 예약(사용자 10-13) — 그리기는 `ui.menu_icons`(향상 모드 = 끔 · 칸만 남음).
        nexa_ctl::controls::ctxmenu::set_icon_col_always(true);
        self.log_win
            .set_on_top(self.settings.flag("log.always_on_top"));
        self.editors.set_rulers(rulers);
        self.conn_win
            .set_probe(probe_hub, probe_policy(&self.settings));
        // ★ L0 네트워크 신호(docs/107 §3-2 · T-313 ⑤ · 39 §3 부하원 = 콜백뿐): 깃발 + 깨우기 → `about_to_wait`에서 `net_changed`.
        if self.settings.flag("net.watch") {
            let net_proxy = std::sync::Mutex::new(net_proxy);
            self.netwatch = nexa_sys::netwatch::NetWatch::start(Box::new(move || {
                if let Ok(p) = net_proxy.lock() {
                    let _ = p.send_event(Wake);
                }
            }));
        }
        // ★ T-352: Finder 더블클릭·Dock 드롭·`open -a`로 넘어오는 파일(macOS Apple Event) — 이벤트 루프(= 앱 델리게이트)가 생긴 뒤 · `run` 전에 설치해야
        //   기동 때 넘어온 파일도 받는다. 큐는 `resumed` 끝과 `about_to_wait`에서 비운다(`pump_os_open_files`).
        self.os_open_files = nexa_sys::open_files::install();
        self.editors.set_whitespace(ws_style);
        self.log_win.set_row_snap(row_snap);
        self.toasts.configure(
            self.settings.int("ui.toast_ms"),
            self.settings.int("ui.toast_alpha"),
        );
        self.toasts.configure_progress(
            self.settings.flag("ui.toast_progress"),
            self.settings.int("ui.toast_fade_to"),
            self.settings.int("ui.toast_bar_spent"),
        );
        {
            let on = self.settings.flag("input.ime_hint");
            self.conn_win.set_ime_hint(on);
            self.input_win.set_ime_hint(on);
            #[cfg(all(unix, not(target_os = "macos")))]
            imewatch::set_enabled(self.settings.flag("input.ime_hint_watch"));
        }
        self.apply_text_render();
        self.apply_toolbar_visibility();
        self.git
            .set_interval(self.settings.int("statusbar.git_secs").max(2) as u64);
        self.apply_ruler_style();
        self.apply_occurrence_style();
        self.apply_objlink_style();
        // 설정 창 Format 분류의 미리보기 상자(SQL 구문 강조 · 읽기 전용 · 사용자 09-29).
        {
            let syntax = self.editors.syntax_for_title("preview.sql");
            let tb = self.editors.preview_box("", &syntax);
            self.prefs_win.set_preview_box(tb);
        }
        self.apply_run_toast();
        self.apply_detail_mask();
        self.editors
            .set_diff_marks(self.settings.flag("editor.diff_marks"));
        self.sync_run_stmt_button();
        nsql_drivers::set_mssql_encryption(self.settings.get("mssql.encrypt") == Some("login"));
        nsql_drivers::set_mysql_tls(
            self.settings.get("mysql.tls").unwrap_or("preferred"),
            self.settings.get("mysql.tls_ca").unwrap_or(""),
        );
        // PostgreSQL TLS(T-70 · 155차 · `pg.tls` · `pg.tls_ca`).
        nsql_drivers::set_pg_tls(
            self.settings.get("pg.tls").unwrap_or("preferred"),
            self.settings.get("pg.tls_ca").unwrap_or(""),
        );
        // SSH 터널(T-3 · 159차 · `ssh.command` · `ssh.connect_timeout_secs`).
        nsql_drivers::set_ssh(
            self.settings.get("ssh.command").unwrap_or("ssh"),
            self.settings.int("ssh.connect_timeout_secs").max(1) as u64,
        );
        // ★ 162차: 터널 PID 원장 = <설정 폴더>/run · 지난 인스턴스가 남긴 ssh를 기동 때 정리(소유자 죽음 + 이름 ssh/포트 열림일 때만).
        nsql_drivers::set_ssh_state_dir(nsql_settings::config_dir().map(|d| d.join("run")));
        let stale = nsql_drivers::cleanup_stale_tunnels();
        if stale > 0 {
            eprintln!("[ssh] cleaned {stale} stale tunnel process(es) from a previous run");
        }
        // ★ 확장 드라이버 폴더(DR-29 · T-27 ③) = <설정 폴더>/drivers(`NSQL_DRIVER_DIR`가 있으면 그쪽) — 기동 때 읽지 않고 첫 접속 때 지연 로드.
        nsql_drivers::ext::set_driver_dir(
            nsql_settings::config_dir()
                .map(|d| d.join("drivers"))
                .as_deref(),
        );
        // 지난번 삭제하지 못한(세션이 쓰던) 확장 폴더 정리 — 표식 파일이 없으면 stat 하나(사용자 10-23).
        let _ = nsql_drivers::ext::sweep_pending();
        // ★ D-350 접속 시 드라이버 자동 설치 정책(설정 `driver.auto_install` · `driver.repo` · `driver.http_timeout_secs`).
        nsql_drivers::set_auto_install(self.settings.flag("driver.auto_install").then(|| {
            nsql_drivers::ext::install::Source::from_settings(
                self.settings.get("driver.repo"),
                self.settings.int("driver.http_timeout_secs"),
            )
        }));
        nsql_drivers::set_auto_upgrade(self.settings.flag("driver.auto_upgrade"));
        apply_oracle_client(&self.settings);
        self.apply_net_options();
        // ★ D-350 하루 1회 드라이버 갱신 확인(`driver.check_updates` · 조용히 · 자동 교체 없음).
        self.drivers_check_updates_at_start();
        // 첫 화면부터 탭 표식(미연결 사선)이 보이게 — 세션 상태 → 화면 3층 동기화 1회.
        self.sync_sess_ui();
        nsql_drivers::set_mssql_cancel_socket(self.settings.get("mssql.cancel") == Some("socket"));
        nsql_drivers::set_odbc_cancel(self.settings.get("odbc.cancel").unwrap_or("auto"));
        self.apply_tab_accent();
        // ★ 포맷 프리셋(D-431): 기동 때 한 번 — settings.json을 밖에서 고쳤어도 표시 이름이 실제 값과 맞게.
        self.format_preset_sync();
        self.apply_extensions(None);
        self.apply_window_sizes();
        self.editors
            .set_text_inset(self.settings.int("editor.text_pad_left").clamp(0, 32) as i32);
        self.grid.set_default_page_rows(max_rows);
        self.grid.set_col_limits(
            self.settings.int("grid.col_min_width") as i32,
            self.settings.grid_col_max_chars() as i32,
        );
        self.grid
            .set_auto_fetch(self.settings.flag("grid.auto_fetch"));
        self.log_win.set_wrap(self.settings.flag("log.wrap"));
        self.log_win
            .set_switch_scale(self.settings.int("log.switch_scale"));
        self.log_win
            .set_template(self.settings.get("log.template").unwrap_or(""));
        self.log_win
            .set_columns(self.settings.get("log.columns").unwrap_or(""));
        self.log_win
            .set_kinds(self.settings.get("log.kinds").unwrap_or(""));
        self.log_win
            .set_session_filter(self.settings.get("log.session_filter").unwrap_or(""));
        self.rebuild_log_hub();
        self.log_win
            .set_newest_first(self.settings.flag("log.newest_first"));
        self.log_win
            .set_autoscroll(self.settings.flag("log.autoscroll"));
        self.grid
            .set_row_numbers(self.settings.flag("grid.row_numbers"));
        // 호버 행 페이드 진입 시간(ms) — 전역이라 버튼·콤보·그리드·목록에 함께 적용(사용자 09-14).
        // 스크롤 방향(맥식 자연스러운 스크롤) — 창 세 개 공통.
        input::set_natural_scroll(self.settings.flag("input.scroll_natural"));
        set_clip_native(self.settings.flag("clipboard.x11_native"));
        // 화면 내보내기 방식(T-147) — 첫 창이 만들어지기 전에.
        present::set_mode(self.settings.get("gfx.mac_present").unwrap_or("softbuffer"));
        // 객체 상세 패널(docs/86): 편집기 탭과 같은 상자(글꼴 지표·줄 간격 일치 · 09-25 캡처의 줄 겹침·하단 미표시) + 축소 상태(설정 기억).
        let plain = self.editors.syntax_for_title("detail.txt");
        self.objdetail.set_box(self.editors.preview_box("", &plain));
        self.objdetail
            .set_collapsed(self.settings.flag("explorer.details_collapsed"));
        // 접속 창 조정값(비노출 설정 · 사용자 09-14 "구현 값은 설정으로").
        self.conn_win.set_tuning(conn_tuning(&self.settings));
        nexa_ctl::tokens::set_intent_ms(
            self.settings.int("ui.hover_intent_ms").clamp(0, 500) as u64
        );
        nexa_ctl::set_default_click_guard_ms(
            self.settings.int("ui.click_guard_ms").clamp(0, 2000) as u64
        );
        nexa_ctl::tokens::set_fade_out_ms(fade_ms(&self.settings, "ui.fade_out_ms", 2000));
        nexa_fs::shell::set_os_icons(self.settings.flag("file.os_icons"));
        nexa_dlg::set_probe_chevrons(self.settings.flag("file.probe_chevrons"));
        // Linux 시스템 테마(10-24 · 138차): gsettings 조회 캐시 TTL(종전엔 기동 때 적용되지 않아 설정 파일 값이 첫 변경까지 무시됐다) +
        //   dconf 폴더 감시 켬/끔(`ui.theme_watch`) — 다른 OS no-op.
        theme::set_cache_ms(self.settings.int("ui.theme_cache_ms").clamp(0, 60_000) as u64);
        theme::set_watch(self.settings.flag("ui.theme_watch"));
        // hover / 눌림 색(설정 · `#RRGGBBAA` · 비우면 테마 기본).
        for tg in ColorTarget::ALL {
            apply_color(tg, color_setting(&self.settings, tg.key()).as_deref());
        }
        // 페이드 속도 속성 두 단(Fast/Slow)의 실제 ms — 컨트롤은 속도 이름만 알고 여기서 값이 연계된다(사용자 09-14).
        nexa_ctl::tokens::set_fade_ms(
            nexa_ctl::tokens::FadeSpeed::Fast,
            fade_ms(&self.settings, "ui.fade_fast_ms", 5000),
        );
        nexa_ctl::tokens::set_fade_ms(
            nexa_ctl::tokens::FadeSpeed::Slow,
            fade_ms(&self.settings, "ui.fade_slow_ms", 5000),
        );
    }

    /// 실행 인자 접속(사용자 09-17 · T-104): 프로필 이름이든 접속 문자열이든 스펙으로 풀어 Connect 버튼과 같은 길로(종전 `main()` 끝 단락 · 221차 이동).
    pub(crate) fn init_startup_target(
        &mut self,
        initial_target: Option<String>,
        arg_fill_only: bool,
    ) {
        // ★ 실행 인자 접속(사용자 09-17): 프로필 이름이든 접속 문자열이든 스펙으로 풀어 **Connect 버튼과 같은 경로**(`last_spec` →
        //   접속 뒤 탐색기도 붙는다 · T-104 해결). 스펙으로 못 풀면 종전 `Cmd::Connect(문자열)`.
        if let Some(target) = initial_target.filter(|_| !arg_fill_only) {
            let spec = if nsql_vault::is_profile_name(&target) {
                match Vault::open_default().and_then(|v| v.get(&target)) {
                    Ok(Some(spec)) => Some(spec),
                    _ => {
                        self.sess.status = tf(Msg::StArgProfileMissing, &[&target]);
                        None
                    }
                }
            } else {
                nsql_drivers::parse_target(&target, DEFAULT_DIALECT).ok()
            };
            match spec {
                Some(spec) => {
                    self.sess.busy = true;
                    self.sess.status = tf(Msg::StConnecting, &[&spec.redacted()]);
                    self.sess.last_spec = Some(spec.clone());
                    // 스펙을 미리 둔다 — `RunEvent::Connected`의 탐색기 붙이기가 `spec`을 본다(`connect_quietly`와 같게).
                    self.sess.spec = Some(spec.clone());
                    if nsql_vault::is_profile_name(&target) {
                        self.conn_win.select_by_name(&target);
                        // 접속 창 Connect와 같게 프로필 이름을 세션에 — 탐색기 루트·공유 연결 목록이 `host:port`가 아니라
                        // 프로필 이름으로 보인다(86차 mac 점검 · T-148).
                        self.sess.profile = target.clone();
                    }
                    // 기동 인자 접속 — 방금 만든 세션이라 앞선 작업이 없다(`busy`는 위에서 화면용으로 올렸다).
                    let pass = self.sess.pass_queued("startup-arg");
                    self.sess.submit(
                        pass,
                        worker::Cmd::ConnectSpec {
                            spec,
                            reconnect_same: false,
                        },
                    );
                }
                None if !nsql_vault::is_profile_name(&target) => {
                    self.sess.busy = true;
                    self.sess.status = tf(Msg::StConnecting, &[&target]);
                    let pass = self.sess.pass_queued("startup-arg");
                    self.sess.submit(pass, worker::Cmd::Connect(target));
                }
                None => {}
            }
        }
    }
}
