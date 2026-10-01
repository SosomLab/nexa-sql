//! App — 설정 반영·테마·언어(설정 키 → 화면 · docs/24).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// settings.json으로 편집(사용자 09-15) — 내보내고 외부 프로그램(.json 연결)으로 연 뒤 저장을 감시한다.
    /// `settings.json_editor`: external = OS 연결 프로그램 · builtin = 편집기 탭(T-76 1차 · 09-16). 둘 다 저장 감시로 반영.
    pub(crate) fn edit_settings_json(&mut self) {
        let path = match self.settings.export_json() {
            Ok(p) => p,
            Err(e) => {
                self.sess.status = tf(Msg::StJsonError, &[&e.to_string()]);
                return;
            }
        };
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        self.json_watch = Some((path.clone(), mtime));
        self.json_next = Instant::now() + Duration::from_millis(1000);
        let builtin = self.settings.get("settings.json_editor") == Some("builtin");
        if builtin {
            // T-76 1차(09-16): 편집기 탭으로 연다 — Ctrl+S 저장은 파일 쓰기(T-74) → 위의 1초 감시가 바뀐 키를 반영한다
            // (외부 편집기와 같은 경로 · 별도 훅 없음).
            self.open_file_enc(&path, "utf8");
            self.sess.status = tf(Msg::StJsonOpened, &[&path.display().to_string()]);
            self.redraw();
            return;
        }
        match open_external(&path) {
            Ok(()) => self.sess.status = tf(Msg::StJsonOpened, &[&path.display().to_string()]),
            Err(e) => self.sess.status = tf(Msg::StJsonError, &[&e]),
        }
        self.redraw();
    }

    /// settings.json 저장 감시 — 바뀌었으면 다시 읽어 바뀐 키만 반영·저장(1초 폴링 · 열어 둔 뒤에만).
    pub(crate) fn json_tick(&mut self, now: Instant) -> Option<Instant> {
        let (path, last) = self.json_watch.clone()?;
        if now < self.json_next {
            return Some(self.json_next);
        }
        self.json_next = now + Duration::from_millis(1000);
        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        if mtime.is_some() && mtime != last {
            self.json_watch = Some((path.clone(), mtime));
            match std::fs::read_to_string(&path) {
                Ok(text) => match self.settings.import_json(&text) {
                    Ok(r) => {
                        self.persist_settings();
                        let mut restart = false;
                        for k in &r.changed {
                            if !self.apply_setting(k) {
                                restart = true;
                            }
                        }
                        let mut note = String::new();
                        if !r.unknown.is_empty() {
                            note.push_str(&tf(Msg::StUnknownKeys, &[&r.unknown.join(",")]));
                        }
                        if !r.invalid.is_empty() {
                            note.push_str(&format!(
                                " · invalid {}",
                                r.invalid
                                    .iter()
                                    .map(|(k, _)| k.as_str())
                                    .collect::<Vec<_>>()
                                    .join(",")
                            ));
                        }
                        if restart {
                            note.push_str(" · ");
                            note.push_str(t(Msg::StNeedsRestart));
                        }
                        self.sess.status =
                            tf(Msg::StJsonReloaded, &[&r.changed.len().to_string(), &note]);
                        self.prefs_win.refresh(&self.settings);
                        self.prefs_win.redraw();
                        self.log_win
                            .push(LogEntry::new(LogKind::Info, self.sess.status.clone()));
                    }
                    Err(e) => {
                        self.sess.status = tf(Msg::StJsonError, &[&e]);
                        self.log_win
                            .push(LogEntry::new(LogKind::Error, self.sess.status.clone()));
                    }
                },
                Err(e) => self.sess.status = tf(Msg::StJsonError, &[&e.to_string()]),
            }
            self.redraw();
        }
        Some(self.json_next)
    }

    /// 코드가 설정을 바꾼 뒤(확장 켜기/끄기 · 저장소 추가 · 관리자 활성화) 열려 있는 설정 창의 스냅샷을 다시 읽는다
    /// (사용자 09-17 "팔레트에서 켜도 설정 창은 꺼진 채").
    pub(crate) fn prefs_sync(&mut self) {
        self.prefs_win.refresh(&self.settings);
        self.prefs_win.redraw();
    }

    /// 파일 탭 강조색(`editor.tab_accent` · 비면 테마 accent = 결과 탭과 같음).
    pub(crate) fn apply_tab_accent(&mut self) {
        let c = self
            .settings
            .get("editor.tab_accent_color")
            .and_then(nexa_ctl::theme::color_from_hex);
        self.editors.set_tab_accent(c);
    }

    pub(crate) fn apply_ruler_style(&mut self) {
        let show = self.settings.flag("editor.rulers_show");
        let (color, hex_alpha) = color_alpha_setting(&self.settings, "editor.ruler_color");
        // `#RRGGBBAA`의 AA가 있으면 별도 불투명도 설정보다 우선(색 창이 알파를 함께 저장 · 09-17).
        let alpha = hex_alpha
            .unwrap_or(self.settings.int("editor.ruler_alpha").clamp(5, 100) as f32 / 100.0);
        let occ = self.settings.flag("editor.highlight_selection");
        self.editors.set_ruler_style(show, color, alpha, occ);
    }

    /// 동일 출현 상자 스타일(`editor.occurrence_*` · 사용자 09-17): 모양 · 선 색+알파 · 두께 · 배경 색+알파.
    pub(crate) fn apply_occurrence_style(&mut self) {
        let (line, la) = color_alpha_setting(&self.settings, "editor.occurrence_line_color");
        let (fill, fa) = color_alpha_setting(&self.settings, "editor.occurrence_fill_color");
        let st = nexa_ctl::OccurrenceStyle {
            round: self.settings.get("editor.occurrence_shape") == Some("round"),
            line,
            line_alpha: la.unwrap_or(0.7),
            width: self
                .settings
                .int("editor.occurrence_line_width")
                .clamp(0, 4) as i32,
            fill,
            fill_alpha: fa.unwrap_or(if fill.is_some() { 1.0 } else { 0.0 }),
        };
        self.editors.set_occurrence_style(st);
    }

    /// 설정 → nexa-gfx 텍스트 렌더(대비 감마 · 정수 스냅) — 전 창 공통(글리프 캐시 키에 감마가 들어 있어 비울 필요 없음).
    pub(crate) fn apply_text_render(&self) {
        let pct = self.settings.int("ui.text_contrast").clamp(100, 250) as f32;
        nexa_gfx::text::set_text_contrast(pct / 100.0);
        nexa_gfx::text::set_text_snap(self.settings.flag("ui.text_snap"));
        nexa_gfx::text::set_text_hint(self.settings.flag("ui.text_hint"));
        nexa_gfx::text::set_text_gdi(self.settings.flag("ui.text_gdi"));
        nexa_gfx::text::set_text_weight(
            self.settings.int("ui.text_weight").clamp(0, 60) as f32 / 100.0,
        );
    }

    /// 설정 → 미니맵(T-97).
    pub(crate) fn apply_minimap(&mut self) {
        // 전체 선택 뒤 화면(09-30 · 기동 때 미니맵과 함께 적용).
        self.editors
            .set_select_all_keep(self.settings.get("editor.select_all_view") != Some("end"));
        let on = self.settings.flag("editor.minimap");
        let w = self.settings.int("editor.minimap_width").clamp(20, 400) as i32;
        self.editors.set_minimap(on, w);
        self.editors
            .set_split_max(self.settings.int("editor.split_max"));
        let (color, alpha) = color_alpha_setting(&self.settings, "editor.minimap_box_color");
        self.editors
            .set_minimap_box(color, alpha, self.settings.flag("editor.minimap_border"));
        self.editors.set_minimap_opts(
            self.settings.get("editor.minimap_viewport") == Some("hover"),
            self.settings.get("editor.minimap_click") == Some("text"),
            self.settings.flag("editor.minimap_find"),
        );
        self.redraw();
    }

    /// 설정 → nexa-gfx 탭 폭 + 편집기 들여쓰기.
    pub(crate) fn apply_indent(&mut self) {
        let ts = self.settings.int("editor.tab_size").clamp(1, 8);
        nexa_gfx::text::set_tab_cols(ts as u32);
        let stops = self.settings.get("editor.tab_stops") != Some("fixed");
        nexa_gfx::text::set_tab_stops(stops);
        self.editors.set_tab_stops(stops);
        self.editors
            .set_indent(ts as u8, self.settings.flag("editor.indent_spaces"));
        // Auto indent(docs/49): 변수 4 + 규칙 세트.
        let cfg = nexa_ctl::AutoIndent {
            enabled: self.settings.flag("editor.auto_indent"),
            smart: self.settings.flag("editor.smart_indent"),
            to_bracket: self.settings.flag("editor.indent_to_bracket"),
            trim: self.settings.flag("editor.trim_auto_whitespace"),
        };
        let rules = match self.settings.get("editor.indent_rules").unwrap_or("sql") {
            "brackets" => nexa_ctl::IndentRules::brackets(),
            "none" => nexa_ctl::IndentRules::none(),
            _ => nexa_ctl::IndentRules::sql(),
        };
        self.editors.set_auto_indent(cfg, rules);
        self.redraw();
    }

    /// 파일 싱크 허브(설정 `log.file` · 형식 `log.file_format`(same = 창과 같게) · 회전 `log.file_max_kb`) — 설정이 바뀌면 새로.
    pub(crate) fn rebuild_log_hub(&mut self) {
        self.log_hub = None;
        // 경로 설정도 내장 변수를 받는다(`${nsqlHome}/log.txt` · `${workspaceFolder}/…` · 사용자 09-23).
        let file = nsql_script::intrinsic::expand(
            self.settings.get("log.file").unwrap_or("").trim(),
            &self.run_intrinsic(),
        );
        if file.is_empty() {
            return;
        }
        let name = self.settings.get("log.format").unwrap_or("raw").to_string();
        let ff = self
            .settings
            .get("log.file_format")
            .unwrap_or("same")
            .to_string();
        let ff = if ff == "same" { name } else { ff };
        let tpl = self.settings.get("log.template").unwrap_or("").to_string();
        let cols = nsql_log::Columns::parse(self.settings.get("log.columns").unwrap_or(""));
        let max = self.settings.int("log.file_max_kb").max(64) as u64 * 1024;
        match nsql_log::FileSink::open(&file, nsql_log::formatter_with(&ff, &tpl, cols), max) {
            Ok(sink) => self.log_hub = Some(nsql_log::LogHub::spawn(vec![Box::new(sink)], 4096)),
            Err(e) => {
                self.sess.status = tf(Msg::ErrLogFile, &[&e.to_string()]);
                self.log_win.push(LogEntry::new(
                    LogKind::Error,
                    tf(Msg::ErrLogFile, &[&e.to_string()]),
                ));
            }
        }
    }

    /// 환경 설정 창에서 바뀐 값을 **즉시** 반영(가능한 것만 · 나머지는 다음 시작). 키를 도메인 조각 다섯으로 나눠 차례로 묻는다
    /// (T-248 · 09-29 · 순서 = 종전 match 그대로라 행동 보존) — 어느 조각도 모르면 `false`.
    pub(crate) fn apply_setting(&mut self, key: &str) -> bool {
        let hit = self.apply_setting_ui(key)
            || self.apply_setting_conn(key)
            || self.apply_setting_explorer(key)
            || self.apply_setting_grid(key)
            || self.apply_setting_editor(key)
            || self.apply_setting_misc(key);
        if !hit {
            return false;
        }
        self.layout();
        self.redraw();
        self.conn_win.redraw();
        true
    }

    /// 설정 즉시 반영 — 테마·언어·글꼴 래스터·애니메이션·입력·포맷·객체 링크(`ui.` · `input.` · `format.` · `objlink.`). 맞는 키가 없으면 `false`.
    fn apply_setting_ui(&mut self, key: &str) -> bool {
        match key {
            "ui.theme" => self.apply_theme(),
            // ★ 포맷 옵션(공통 `format.*` · 확장 `ext.sqlfmt_kiros33.*`) = 열려 있는 미리보기 탭을 다시 그린다(docs/95).
            k if k.starts_with("format.") || k.starts_with("ext.sqlfmt_kiros33.") => {
                self.format_preview_refresh();
            }
            // ★ Ctrl 객체 링크(T-256): 밑줄 스타일을 다시 적용하고 켜고 끔·표시 방식·상한을 다시 판정.
            k if k.starts_with("objlink.") => {
                self.apply_objlink_style();
                self.objlink_sync();
            }
            // 끄는 순간 들고 있던 비밀번호 봉투를 전부 버린다(세션 자격 금고 · 켜는 것은 다음 입력부터).
            "connect.remember_session_password" => {
                if !self.settings.flag("connect.remember_session_password") {
                    nsql_vault::session::clear();
                }
            }
            "ui.lang" => {
                nsql_i18n::set_lang(self.settings.lang());
                self.relabel();
            }
            "ui.hover_color" | "ui.pressed_color" => {
                for tg in ColorTarget::ALL {
                    if tg.key() == key {
                        apply_color(tg, color_setting(&self.settings, key).as_deref());
                    }
                }
            }
            "ui.click_guard_ms" => {
                nexa_ctl::set_default_click_guard_ms(self.settings.int(key).clamp(0, 2000) as u64)
            }
            "ui.fade_fast_ms" => nexa_ctl::tokens::set_fade_ms(
                nexa_ctl::tokens::FadeSpeed::Fast,
                fade_ms(&self.settings, key, 5000),
            ),
            "ui.fade_slow_ms" => nexa_ctl::tokens::set_fade_ms(
                nexa_ctl::tokens::FadeSpeed::Slow,
                fade_ms(&self.settings, key, 5000),
            ),
            // 애니메이션 마스터(auto/on/off · 향상 모드 off) = 페이드·슬라이드 전부 0으로/복귀.
            "ui.animations" => {
                for k in [
                    "ui.fade_fast_ms",
                    "ui.fade_slow_ms",
                    "ui.fade_out_ms",
                    "ui.slide_ms",
                ] {
                    self.apply_setting(k);
                }
            }
            // 프레임 상한·캐럿 깜빡임은 about_to_wait가 매번 설정을 읽는다(즉시 반영).
            "ui.max_fps" | "editor.caret_blink" => self.redraw(),
            "ui.slide_ms" | "ui.tooltip_delay_ms" | "ui.dblclick_ms" => {
                self.conn_win.set_tuning(conn_tuning(&self.settings));
                self.sync_project_panel_opts();
            }
            // 복사 버튼 체크 표시 시간 — 접속 창(튜닝) + 실행 카드 둘 다.
            "ui.copy_feedback_ms" => {
                self.conn_win.set_tuning(conn_tuning(&self.settings));
                self.apply_run_toast();
            }
            "ui.menu_max_width" => self.rebuild_menus(),
            "input.ime_hint_watch" => {
                #[cfg(all(unix, not(target_os = "macos")))]
                imewatch::set_enabled(self.settings.flag("input.ime_hint_watch"));
            }
            "input.ime_hint" => {
                let on = self.settings.flag("input.ime_hint");
                self.conn_win.set_ime_hint(on);
                self.input_win.set_ime_hint(on);
            }
            _ => return false,
        }
        true
    }

    /// 설정 즉시 반영 — 접속·서버 상태·파일·프로젝트 아이콘·토스트·실행 카드·DBMS 클라이언트(`probe.` · `net.` · `mssql.` · `oracle.` · `gfx.`). 맞는 키가 없으면 `false`.
    fn apply_setting_conn(&mut self, key: &str) -> bool {
        let i = |s: &Settings, k: &str| s.int(k);
        match key {
            "probe.interval"
            | "probe.max_retries"
            | "probe.max_inflight"
            | "probe.icmp"
            | "probe.timeout_ms"
            | "probe.retry_delay_ms"
            | "probe.enabled"
            | "probe.dns_cache_secs" => {
                let n = self.settings.int("probe.max_inflight").clamp(1, 64) as usize;
                self.conn_win.set_policy(probe_policy(&self.settings), n);
            }
            "file.os_icons" => nexa_fs::shell::set_os_icons(self.settings.flag(key)),
            "project.icons" => self.project_panel.set_icons(self.settings.flag(key)),
            "editor.tab_line_unsaved"
            | "editor.tab_line_file"
            | "editor.tab_line_preview"
            | "editor.tab_unsaved_text"
            | "editor.tab_unsaved_color"
            | "editor.tab_close_show" => self.apply_tab_line_colors(),
            "file.probe_chevrons" => nexa_dlg::set_probe_chevrons(self.settings.flag(key)),
            "ui.toast_ms" | "ui.toast_alpha" => {
                self.toasts.configure(
                    self.settings.int("ui.toast_ms"),
                    self.settings.int("ui.toast_alpha"),
                );
                self.apply_run_toast();
            }
            "ui.toast_progress" | "ui.toast_fade_to" | "ui.toast_bar_spent" => {
                self.toasts.configure_progress(
                    self.settings.flag("ui.toast_progress"),
                    self.settings.int("ui.toast_fade_to"),
                    self.settings.int("ui.toast_bar_spent"),
                );
                self.apply_run_toast();
            }
            "output.max_lines" | "output.timestamps" | "output.show" => {
                self.output_apply_settings();
            }
            "run.toast" | "run.toast_hide_ms" | "run.toast_tick_ms" | "run.toast_follow"
            | "run.toast_max" => self.apply_run_toast(),
            "net.keepalive_secs" | "session.call_timeout_secs" => self.apply_net_options(),
            "mssql.encrypt" => {
                nsql_drivers::set_mssql_encryption(self.settings.get(key) == Some("login"))
            }
            // Oracle 클라이언트(자동/직접 지정) — 드라이버에 넘기고 설정 창의 읽기 전용 탐지 결과를 다시 계산한다.
            "oracle.client_mode" | "oracle.client_dir" | "oracle.tns_admin" => {
                apply_oracle_client(&self.settings);
                self.prefs_win.set_info(dbms_info_values());
                self.prefs_win.refresh(&self.settings);
            }
            "mssql.cancel" => {
                nsql_drivers::set_mssql_cancel_socket(self.settings.get(key) == Some("socket"))
            }
            "ui.hover_intent_ms" => {
                nexa_ctl::tokens::set_intent_ms(i(&self.settings, key).clamp(0, 500) as u64)
            }
            "ui.fade_out_ms" => {
                nexa_ctl::tokens::set_fade_out_ms(fade_ms(&self.settings, key, 2000))
            }
            "input.scroll_natural" => {
                input::set_natural_scroll(self.settings.flag(key));
            }
            "clipboard.x11_native" => {
                set_clip_native(self.settings.flag(key));
            }
            "input.hangul_compose" => {
                self.hangul_app = None;
                self.sync_hangul_mode();
            }
            // 새로 여는 창부터(메인 창은 재시작 뒤) — 열려 있는 창의 뒷단은 바꾸지 않는다.
            "gfx.mac_present" => {
                present::set_mode(self.settings.get(key).unwrap_or("softbuffer"));
            }
            _ => return false,
        }
        true
    }

    /// 설정 즉시 반영 — 탐색기·메타·외부 파일 감시·글자 래스터·툴바·상태줄(`explorer.` · `gen.` · `meta.` · `file.external_*` · `toolbar.` · `statusbar.`). 맞는 키가 없으면 `false`.
    fn apply_setting_explorer(&mut self, key: &str) -> bool {
        match key {
            "explorer.visible" => {
                self.explorer.set_visible(self.settings.flag(key));
                self.layout();
            }
            "explorer.icons" => self.explorer.set_icons(self.settings.flag(key)),
            "explorer.sizes" => self.explorer.set_sizes(self.settings.flag(key)),
            "explorer.tooltip" => self.explorer.set_tooltip(self.settings.flag(key)),
            "explorer.timeout" => self
                .explorer
                .set_load_timeout(self.settings.int(key).clamp(0, 600) as u64),
            "explorer.disconnect_pick" => self
                .explorer
                .set_disconnect_pick(self.settings.get(key).unwrap_or("auto")),
            "explorer.keep_offline" => self.explorer.set_keep_offline(self.settings.flag(key)),
            "explorer.filter_scope" => self
                .explorer
                .set_filter_scope(self.settings.get(key).unwrap_or("all")),
            "explorer.share_catalog" => self.explorer.set_share_catalog(self.settings.flag(key)),
            "explorer.details" | "explorer.details_h" | "explorer.details_collapsed" => {
                self.objdetail
                    .set_collapsed(self.settings.flag("explorer.details_collapsed"));
                self.layout();
            }
            "explorer.show_system_schemas" | "explorer.hide_empty_schemas" => {
                self.explorer
                    .set_schema_opts(schema_opts_from(&self.settings));
            }
            "explorer.mssql_tree" | "explorer.mssql_system_dbs" => {
                self.explorer.set_mssql_tree(
                    self.settings.get("explorer.mssql_tree") != Some("schema"),
                    self.settings.flag("explorer.mssql_system_dbs"),
                );
            }
            "explorer.search_index"
            | "explorer.index_max"
            | "explorer.index_hits_max"
            | "explorer.index_prefetch"
            | "explorer.index_idle_ms"
            | "meta.warm_columns_max"
            | "meta.warm_idle_ms"
            | "meta.detail_max"
            | "meta.detail_ttl_secs"
            | "meta.cols_ttl_secs"
            | "meta.disk_cache"
            | "meta.warm_comments" => {
                self.explorer.set_index_cfg(index_cfg_from(&self.settings));
            }
            "explorer.source_schema" => self
                .explorer
                .set_source_qualify(self.settings.flag("explorer.source_schema")),
            "editor.select_all_view" => self
                .editors
                .set_select_all_keep(self.settings.get("editor.select_all_view") != Some("end")),
            "gen.qualified" | "gen.compact" | "gen.full_ddl" | "gen.separate_fk"
            | "gen.bind_note" => {
                let o = gen_opts_from(&self.settings);
                self.explorer.set_gen_opts(o);
            }
            "meta.refresh_highlight_ms" => self
                .explorer
                .set_highlight_ms(self.settings.int(key).max(0) as u64),
            "meta.refresh_secs" => self.meta_refresh_next = None,
            "file.external_poll_ms" | "file.external_check" | "file.external_change" => {
                self.ext_poll_next = None;
            }
            // 안정 대기·크기 상한은 감시 스레드를 만들 때 넣는 값 → 다음 확인 때 새로 만든다.
            "file.external_settle_ms" | "file.external_merge_max_kb" => self.ext_watch = None,
            "explorer.typeahead"
            | "explorer.typeahead_timeout_ms"
            | "explorer.typeahead_space"
            | "explorer.typeahead_special"
            | "explorer.typeahead_pos" => {
                let cfg = typeahead_cfg(&self.settings);
                self.explorer.set_typeahead(cfg);
                self.project_panel.set_typeahead(cfg);
                self.bm_panel.set_typeahead(cfg);
                self.outline_panel.set_typeahead(cfg);
            }
            "ui.text_contrast" | "ui.text_snap" | "ui.text_hint" | "ui.text_weight" => {
                self.apply_text_render();
                self.log_win.redraw();
            }
            "toolbar.hidden" => {
                self.apply_toolbar_visibility();
                self.layout();
            }
            "toolbar.layout" => {
                // 설정 창에서 직접 바꿨을 때(비우면 초기 배치). 앱이 저장한 값과 같으면 아무 일도 없다.
                if self.settings.get(key).unwrap_or("") != self.tool_dock.layout().serialize() {
                    self.apply_tool_layout_setting();
                }
            }
            "statusbar.git" | "statusbar.git_secs" => {
                self.git
                    .set_interval(self.settings.int("statusbar.git_secs").max(2) as u64);
                self.git.refresh(true);
            }
            _ => return false,
        }
        true
    }

    /// 설정 즉시 반영 — 결과 그리드·편집기 클릭·북마크(`grid.` · `editor.dblclick*` · `bookmark.`). 맞는 키가 없으면 `false`.
    fn apply_setting_grid(&mut self, key: &str) -> bool {
        match key {
            "grid.font_face" => {
                let pref = self.settings.get("editor.font_face").map(str::to_string);
                self.grid_font =
                    load_grid_font(self.settings.get(key).unwrap_or(""), pref.as_deref());
            }
            "editor.font_face" => {
                // 고정폭 얼굴 교체(편집기 · 행번호 · 그리드 'mono') — 못 찾으면 사슬 fail-over라 항상 Some.
                let pref = self.settings.get(key).map(str::to_string);
                if let Some(l) = nexa_font::mono_font(pref.as_deref()) {
                    self.mono_font = l.font;
                }
                let gf = self
                    .settings
                    .get("grid.font_face")
                    .unwrap_or("")
                    .to_string();
                self.grid_font = load_grid_font(&gf, pref.as_deref());
                self.layout();
            }
            "grid.col_min_width" | "grid.col_max_mode" | "grid.col_max_chars" => {
                let (lo, chars) = (
                    self.settings.int("grid.col_min_width") as i32,
                    self.settings.grid_col_max_chars() as i32,
                );
                self.all_grids().for_each(|g| g.set_col_limits(lo, chars));
                self.redraw();
            }
            "grid.row_numbers" => {
                let on = self.settings.flag(key);
                self.all_grids().for_each(|g| g.set_row_numbers(on));
            }
            "grid.row_height_pct" => {
                let pct = self.settings.int(key).clamp(110, 300) as i32;
                self.all_grids().for_each(|g| g.set_row_pct(pct));
            }
            "grid.null_text" => {
                let text = self.settings.get(key).unwrap_or("NULL").to_string();
                self.all_grids().for_each(|g| g.set_null_text(&text));
            }
            "grid.row_focus" | "grid.row_focus_color" => self.apply_grid_row_focus(),
            "grid.edit"
            | "grid.edit_empty"
            | "grid.edit_strict"
            | "grid.edit_refresh"
            | "grid.paste_max_rows"
            | "grid.filter_list_max" => self.apply_grid_edit_cfg(),
            "editor.dblclick" | "editor.triple_click" | "editor.dblclick_underscore" => {
                self.apply_click_policy();
            }
            k if k.starts_with("bookmark.") => {
                self.bookmarks.apply_settings(&self.settings);
                let on = self.bookmarks.enabled;
                self.act_bar.set_item_visible("view.bookmarks", on);
                if !on && self.bm_panel.is_visible() {
                    self.bm_panel.set_visible(false);
                    self.layout();
                }
                self.bm_sync_ui();
            }
            _ => return false,
        }
        true
    }

    /// 설정 즉시 반영 — 창 최상위·로그·메모리·스크롤·줄 번호·인텔리센스(`window.` · `log.` · `mem.` · `editor.scroll` · `intel.`). 맞는 키가 없으면 `false`.
    fn apply_setting_editor(&mut self, key: &str) -> bool {
        match key {
            "window.always_on_top" => self.apply_on_top(),
            "log.always_on_top" => self.log_win.set_on_top(self.settings.flag(key)),
            "mem.always_on_top" => self.mem_win.set_on_top(self.settings.flag(key)),
            "mem.statusbar" | "mem.status_refresh_ms" => {
                self.mem_status = (0, None);
                self.redraw();
            }
            "grid.scroll" => {
                let on = self.settings.get(key) == Some("row");
                self.grid.set_row_snap(on);
                self.log_win.set_row_snap(on);
            }
            k if k.starts_with("scroll.fast") => self.apply_fast_scroll(),
            "editor.scroll" => self
                .editors
                .set_scroll_snap(self.settings.get(key) == Some("row")),
            "editor.line_numbers" => self.editors.set_line_numbers(self.settings.flag(key)),
            k if k.starts_with("intel.") => {
                self.intel.set_cfg(self.intel_cfg());
                self.explorer
                    .set_preload(self.settings.flag("intel.preload"));
                self.explorer
                    .set_routines(self.settings.flag("intel.from_routines"));
            }
            _ => return false,
        }
        true
    }

    /// 설정 즉시 반영 — 편집기 표시·짝·큰 파일·되돌리기·프로젝트·검색·배치(`editor.` 나머지 · `file.large_*` · `project.` · `search.` · `layout.`). 맞는 키가 없으면 `false`.
    /// ★ 고속 스크롤 설정 → nexa-ctl 전역 [`nexa_ctl::set_fast_scroll`](8영역의 `ScrollBars`·가속기·HUD가 그때그때 읽는다) +
    ///   결과 그리드 override(`scroll.fast_grid_extra` = 한 번 먼저 오르고 상한 두 배). 시작 때와 `scroll.*` 변경 때.
    pub(crate) fn apply_fast_scroll(&mut self) {
        let (step, max) = nsql_settings::scroll_speed_params(
            self.settings.get("scroll.fast_speed").unwrap_or("fast"),
        );
        let cfg = nexa_ctl::FastScroll {
            enabled: self.settings.flag("scroll.fast"),
            step,
            max,
            window_ms: self.settings.int("scroll.fast_window_ms").clamp(20, 2000) as u64,
            hud: self.settings.flag("scroll.fast_hud"),
            hud_pos: nexa_ctl::HudPos::parse(
                self.settings
                    .get("scroll.fast_hud_pos")
                    .unwrap_or("top_right"),
            ),
            hud_hold_ms: self
                .settings
                .int("scroll.fast_hud_hold_ms")
                .clamp(0, 10_000) as u64,
            hud_fade_ms: self
                .settings
                .int("scroll.fast_hud_fade_ms")
                .clamp(50, 10_000) as u64,
        };
        nexa_ctl::set_fast_scroll(cfg);
        let grid = self
            .settings
            .flag("scroll.fast_grid_extra")
            .then(|| nexa_ctl::FastScroll {
                step: step.saturating_sub(1).max(1),
                max: max.saturating_mul(2),
                ..cfg
            });
        self.grid.set_fast_override(grid);
        self.redraw();
    }

    fn apply_setting_misc(&mut self, key: &str) -> bool {
        match key {
            "editor.diff_marks" => self.editors.set_diff_marks(self.settings.flag(key)),
            // 자동 닫기(코어 설정) = 편집기 옵션 한 벌을 다시 계산해 적용(키 접두가 확장 것이 아니라 None으로).
            "editor.auto_close_pairs"
            | "editor.pair_kinds"
            | "editor.pair_in_strings"
            | "editor.pair_match" => self.apply_extensions(None),
            "file.large_l1_mb"
            | "file.large_l1_lines"
            | "file.large_l2_mb"
            | "file.large_l2_lines" => {
                self.editors.set_large_cfg(large_cfg(&self.settings));
            }
            "file.large_ext_level" | "file.large_syntax_level" => {
                let (ext, syn) = large_feature_levels(&self.settings);
                self.editors.set_large_feature_levels(ext, syn);
                self.redraw();
            }
            "editor.undo_group_ms" | "editor.undo_giant_mb" => {
                let (ms, giant) = undo_rules(&self.settings);
                self.editors.set_undo_rules(ms, giant);
            }
            "editor.max_occurrences" => {
                let cap = self.occurrence_cap();
                self.editors.set_max_regions(cap);
            }
            "editor.undo_budget_mb" => self
                .editors
                .set_undo_budget(self.settings.int(key).max(1) as usize * 1024 * 1024),
            // 확장 관리자 켬/끔(설정 창에서 바꿔도) = 확장 효과 전체 재적용 + 활동 막대 아이콘.
            "project.preview_tab"
            | "project.scan_max"
            | "project.scan_threads"
            | "file.show_hidden"
            | "file.show_dot" => self.sync_project_panel_opts(),
            "search.history_max" => self
                .search_history
                .borrow_mut()
                .set_max(self.settings.int(key).max(0) as usize),
            "search.history_view" => {
                self.search_history
                    .borrow_mut()
                    .set_view(search_history::HistoryView::parse(
                        self.settings.get(key).unwrap_or("dropdown"),
                    ))
            }
            "search.history_rows" => self
                .search_history
                .borrow_mut()
                .set_rows(self.settings.int(key).max(1) as usize),
            "extensions.enabled" => {
                self.apply_extensions(None);
                self.layout();
            }
            "editor.minimap"
            | "editor.split_max"
            | "editor.minimap_width"
            | "editor.minimap_box_color"
            | "editor.minimap_border"
            | "editor.minimap_viewport"
            | "editor.minimap_click"
            | "editor.minimap_find"
            | "editor.minimap_errors" => self.apply_minimap(),
            // 자원 거버너(T-90a/d · docs/39): 상한 세터 4종 · 모드가 바뀌면 원장 키 전부 재적용(실효 값이 바뀌므로).
            "log.max_lines" => self
                .log_win
                .set_max_lines(self.settings.int(key).max(100) as usize),
            "editor.undo_max" => self
                .editors
                .set_undo_max(self.settings.int(key).max(1) as usize),
            "ui.glyph_cache" => {
                nexa_gfx::text::set_glyph_cache_max(self.settings.int(key).max(256) as usize);
            }
            "file.icon_cache" => {
                nexa_fs::shell::set_icon_cache_max(self.settings.int(key).max(16) as usize);
            }
            "txlog.max_entries" => {
                let n = self.settings.int("txlog.max_entries").max(16) as usize;
                self.txlog.set_cap(n);
            }
            "perf.boost" => {
                // 향상 모드 켬/끔 = 강제 대상 키를 전부 다시 적용(실효 값이 바뀐다 · 저장값은 그대로).
                let keys: Vec<&'static str> =
                    nsql_settings::perf::BOOST.iter().map(|(k, _)| *k).collect();
                for k in keys {
                    self.apply_setting(k);
                }
                let on = self.settings.flag(key);
                self.sess.status = t(if on {
                    Msg::StPerfBoostOn
                } else {
                    Msg::StPerfBoostOff
                })
                .into();
                self.layout();
            }
            "ui.menu_icons" => nexa_ctl::controls::set_menu_icons(self.settings.flag(key)),
            "ui.clipboard_probe" => {}
            "perf.mode" => {
                let keys: Vec<&'static str> = nsql_settings::PERF.iter().map(|(k, _)| *k).collect();
                for k in keys {
                    self.apply_setting(k);
                }
                self.sess.status = tf(
                    Msg::StPerfMode,
                    &[t(self.settings.perf_mode_display().label())],
                );
            }
            "editor.tab_size"
            | "editor.indent_spaces"
            | "editor.tab_stops"
            | "editor.auto_indent"
            | "editor.smart_indent"
            | "editor.indent_to_bracket"
            | "editor.trim_auto_whitespace"
            | "editor.indent_rules" => self.apply_indent(),
            "file.eol_new" => self
                .editors
                .set_default_eol(eol::default_eol(self.settings.get(key).unwrap_or("auto"))),
            "editor.rulers" => self
                .editors
                .set_rulers(parse_rulers(self.settings.get(key).unwrap_or("80"))),
            "editor.rulers_show"
            | "editor.ruler_color"
            | "editor.ruler_alpha"
            | "editor.highlight_selection" => self.apply_ruler_style(),
            k if k.starts_with("editor.occurrence_") => self.apply_occurrence_style(),
            "editor.tab_accent_color" => self.apply_tab_accent(),
            "ui.font_face" => {
                let pref = self.settings.get(key).map(str::to_string);
                if let Some(l) =
                    nexa_font::ui_font(pref.as_deref()).or_else(|| nexa_font::ui_font(None))
                {
                    self.ui_font = l.font;
                }
                self.layout();
            }
            "extensions.disabled" => self.apply_extensions(None),
            k if k.starts_with("ext.rainbow_pairs.") => self.apply_extensions(Some(k)),
            "editor.text_pad_left" => self
                .editors
                .set_text_inset(self.settings.int(key).clamp(0, 32) as i32),
            "tabs.tooltip" => self.editors.set_tooltip(self.settings.flag(key)),
            "log.template" => self
                .log_win
                .set_template(self.settings.get(key).unwrap_or("")),
            "log.columns" => self
                .log_win
                .set_columns(self.settings.get(key).unwrap_or("")),
            "log.kinds" => self.log_win.set_kinds(self.settings.get(key).unwrap_or("")),
            "log.file" | "log.file_format" | "log.file_max_kb" => self.rebuild_log_hub(),
            "log.switch_scale" => self.log_win.set_switch_scale(self.settings.int(key)),
            "grid.max_rows" => {
                let n = self.settings.int(key).max(0) as usize;
                self.all_grids().for_each(|g| g.set_default_page_rows(n));
            }
            "grid.auto_fetch" => {
                let on = self.settings.flag(key);
                self.all_grids().for_each(|g| g.set_auto_fetch(on));
            }
            "grid.result_tabs" | "grid.result_tabbar_single" | "grid.result_tab_title" => {
                self.apply_result_tab_opts();
            }
            "tabs.rows" => {
                let multi = self.settings.get(key) != Some("single");
                self.editors.set_multiline_tabs(multi);
                self.layout();
            }
            "log.wrap" => self.log_win.set_wrap(self.settings.flag(key)),
            "log.dev_mode" | "log.dev_layers" => self.apply_detail_mask(),
            "log.newest_first" => self.log_win.set_newest_first(self.settings.flag(key)),
            "log.autoscroll" => self.log_win.set_autoscroll(self.settings.flag(key)),
            "log.format" => self
                .log_win
                .set_format(self.settings.get(key).unwrap_or("raw")),
            k if k.starts_with("key.") => {
                self.keymap = Keymap::from_settings(&self.settings);
                self.apply_menu_decor();
                self.keys_win.refresh(&self.keymap);
            }
            k if k.starts_with("editor.whitespace") || k.starts_with("editor.show_") => {
                let ws = whitespace_style(&self.settings);
                self.editors.set_whitespace(ws);
                // 설정 창에서 바꾼 직후 편집기에 바로 보여야 한다(사용자 09-17) · 로그 창에 적용 사실을 남긴다(진단).
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    tf(
                        Msg::StWhitespaceApplied,
                        &[
                            self.settings
                                .get("editor.whitespace")
                                .unwrap_or("selection"),
                            self.settings.get("editor.whitespace_chars").unwrap_or(""),
                        ],
                    ),
                ));
                self.redraw();
            }
            "ui.font_size"
            | "ui.menu_font_size"
            | "editor.font_size"
            | "grid.font_size"
            | "explorer.width"
            | "explorer.font_size"
            | "layout.editor_split_pct" => {
                self.layout();
            }
            _ => return false,
        }
        true
    }

    /// 편집기 탭 유형별 활성 줄 색(사용자 09-22): 설정 `editor.tab_line_{scratch,file,preview}`(`#RRGGBB` · 빈 값 = 기본) —
    /// 기본 = 스크립트 warn(미저장 주의) · 파일 accent · 미리보기 text_dim(임시). 테마·설정이 바뀔 때 다시 계산.
    pub(crate) fn apply_tab_line_colors(&mut self) {
        let pick = |s: &Settings, key: &str, dflt: Option<nexa_ctl::Color>| {
            color_alpha_setting(s, key).0.or(dflt)
        };
        let c = [
            pick(
                &self.settings,
                "editor.tab_line_unsaved",
                Some(self.theme.warn),
            ),
            // ★ 파일 탭 색은 **명시**한다 — `None`은 탭 바에서 "바 공통 accent"(= 활성 탭의 유형 색)로 떨어져 미저장 탭이 활성인
            //   채 파일 탭을 묶으면 파일 탭 줄까지 주황이 됐다(사용자 09-23 "각 탭의 색을 유지"). 기본 = **진한 회색**(사용자 09-28
            //   "일반 파일은 진한 회색으로 구분선·닫음/수정 표시" · 다크 테마는 배경에 묻히지 않게 밝은 회색).
            pick(
                &self.settings,
                "editor.tab_line_file",
                Some(if self.theme.is_dark {
                    nexa_ctl::Color(0x009A_9A9A)
                } else {
                    nexa_ctl::Color(0x005A_5A5A)
                }),
            ),
            pick(
                &self.settings,
                "editor.tab_line_preview",
                // 미리보기 = 보라 계열(사용자 09-28 "흐린 색은 식별이 안 된다" → 추천색 #9B6BD6 · 라이트/다크 둘 다 보임).
                Some(nexa_ctl::Color(0x009B_6BD6)),
            ),
        ];
        self.editors.set_tab_line_colors(c);
        // 닫기 상자 표시(09-28): always(기본) / hover — 편집기 탭 + 지금·잠든 결과 탭 바 전부.
        let close_always = self.settings.get("editor.tab_close_show") != Some("hover");
        self.editors.set_close_always(close_always);
        self.panel.set_close_always(close_always);
        for p in self.panels.values_mut() {
            p.set_close_always(close_always);
        }
        // ★ 미저장 탭 이름 색(사용자 09-28): 켬/끔 + 색(빈 값 = 미저장 줄 색).
        self.editors.set_tab_unsaved(
            self.settings.flag("editor.tab_unsaved_text"),
            color_alpha_setting(&self.settings, "editor.tab_unsaved_color").0,
        );
    }

    /// 개발자 모드 마스크(`log.dev_mode` × `log.dev_layers` · 향상 모드는 dev_mode를 끈다) → nsql-log 전역 게이트.
    pub(crate) fn apply_detail_mask(&mut self) {
        let on = self.settings.flag("log.dev_mode");
        let mask = if on {
            nsql_log::parse_detail_layers(self.settings.get("log.dev_layers").unwrap_or(""))
        } else {
            0
        };
        nsql_log::set_detail_mask(mask);
        self.log_win.set_dev(on);
        self.log_win.set_dev_mask(nsql_log::parse_detail_layers(
            self.settings.get("log.dev_layers").unwrap_or(""),
        ));
    }

    /// 실행 상태 카드 설정(`run.toast` · `run.toast_hide_ms` · 불투명도는 토스트와 공용 `ui.toast_alpha`).
    pub(crate) fn apply_run_toast(&mut self) {
        let (on, hide, alpha) = (
            self.settings.flag("run.toast"),
            self.settings.int("run.toast_hide_ms"),
            self.settings.int("ui.toast_alpha"),
        );
        let (prog, fade_to, spent) = (
            self.settings.flag("ui.toast_progress"),
            self.settings.int("ui.toast_fade_to"),
            self.settings.int("ui.toast_bar_spent"),
        );
        let tick = self.settings.int("run.toast_tick_ms");
        self.run_toast.configure(on, hide, alpha);
        self.run_toast
            .configure_copy(self.settings.int("ui.copy_feedback_ms"));
        self.run_toast.configure_progress(prog, fade_to, spent);
        self.run_toast.configure_tick(tick);
        self.run_toast.configure_stack(
            self.settings.flag("run.toast_follow"),
            self.settings.int("run.toast_max"),
        );
    }

    /// `ui.theme` + OS 판정으로 팔레트를 다시 고르고 전체를 다시 그린다.
    pub(crate) fn apply_theme(&mut self) {
        let wt = self.window.as_ref().and_then(|w| w.theme());
        self.theme = theme::resolve(self.settings.theme_mode(), wt);
        self.apply_tab_line_colors();
        self.log_win.redraw();
        if let Some(w) = &self.window {
            w.set_theme(theme::window_theme(self.settings.theme_mode()));
        }
        self.redraw();
    }

    /// Ctrl/⌘+⇧T — System → Light → Dark 순환 · 저장.
    pub(crate) fn cycle_theme(&mut self) {
        let next = self.settings.theme_mode().next();
        let _ = self.settings.set("ui.theme", next.as_str());
        self.persist_settings();
        self.apply_theme();
        self.sess.status = tf(Msg::StThemeChanged, &[t(next.label())]);
    }

    /// Ctrl/⌘+⇧L — 언어 전환 · 저장 · 라벨 다시 만들기.
    pub(crate) fn toggle_lang(&mut self) {
        let next = current_lang().next();
        let _ = self.settings.set("ui.lang", next.code());
        self.persist_settings();
        nsql_i18n::set_lang(next);
        self.relabel();
        self.sess.status = tf(Msg::StLangChanged, &[next.endonym()]);
        self.redraw();
    }

    pub(crate) fn persist_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.sess.status = tf(Msg::CfgSaveFailed, &[&e.to_string()]);
        }
    }

    /// 언어가 바뀌면 컨트롤 문자열을 다시 만든다. TextBox는 placeholder 교체 API가 없어 본문을 보존해 재생성.
    pub(crate) fn relabel(&mut self) {
        let tabs = self.editors.tab_list_dirty();
        self.menubar.set_menus(App::build_menus_with(
            &self.recent_files(),
            &tabs,
            self.demo_ready,
            self.gate_shown.unwrap_or(false),
            self.project_menu_entries(),
        ));
        let layout = self.tool_dock.layout();
        self.tool_dock = App::build_tool_dock();
        self.tool_dock.apply_layout(&layout);
        let _ = self.tool_dock.take_actions();
        self.apply_toolbar_visibility();
        self.conn_win.relabel();
        self.editors.rebuild_boxes();
        self.layout();
        self.set_focus(self.focus);
    }

    /// 다중 선택 구간 수 상한(`editor.max_occurrences` · 최소 100 · docs/72 §2).
    pub(crate) fn occurrence_cap(&self) -> usize {
        self.settings.int("editor.max_occurrences").max(100) as usize
    }
}
