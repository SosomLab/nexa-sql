//! App — 그리기(프레임 합성).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    pub(crate) fn paint(&mut self) {
        // 상태줄 메모리 글은 그리기 빌림 전에(5 s에 한 번 OS 조회 · docs/80).
        let mem_txt = self.mem_status_text();
        // 찾기가 열린 동안 본문이 바뀌면 일치 표시도 따라간다(전체 스캔 · 열려 있을 때만 · T-73).
        if self.find.is_visible() {
            self.sync_find_marks();
        }
        let t_frame = Instant::now();
        self.tmark("paint begin");
        let mut marks: [u32; 6] = [0; 6];
        let mut mark_i = 0usize;
        let mut mark = |t: &mut Instant, marks: &mut [u32; 6]| {
            if mark_i < marks.len() {
                marks[mark_i] = t.elapsed().as_micros() as u32;
                mark_i += 1;
            }
            *t = Instant::now();
        };
        let mut t_sec = Instant::now();
        self.sync_grid_tab();
        let (Some(win), Some(surface)) = (self.window.clone(), self.surface.as_mut()) else {
            return;
        };
        let size = win.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            return;
        };
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let caret_on = !self.settings.flag("editor.caret_blink")
            || !self.main_active
            || !nexa_sys::layer_present::app_active().unwrap_or(true)
            || (self.blink_origin.elapsed().as_millis() / 500).is_multiple_of(2);
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let th = self.theme;
            let ui_px = self.settings.font_px("ui.font_size");
            let mono_px = self.settings.font_px("editor.font_size");
            let grid_px = self.settings.font_px("grid.font_size");
            // ── UI 층(한글 UI 본)
            {
                let prefs = FontPrefs::with_base(ui_px);
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s)
                    .with_fonts(prefs)
                    .with_caret_on(caret_on);
                dc.fill_rect(Rect::new(0, 0, wi, hi), th.window_bg);
                self.editors.paint_tabs(&mut dc, &th);
                self.panel.paint_bar(&mut dc, &th);
                // 메뉴바·툴바(창 전폭) — 메뉴 드롭다운은 최상위라 맨 뒤에.
                dc.fill_rect(self.tool_dock.bounds(), th.chrome_bg);
                self.tool_dock.paint(&mut dc, &th);
                // 툴바 툴팁은 탐색기·편집기가 덮지 못하게 최상위 층(메뉴바 직전)에서 그린다(09-16 사용자 캡처: 툴바 아래 검은 띠).
                dc.fill_rect(self.menubar.bounds(), th.chrome_bg);
                dc.fill_rect(
                    Rect::new(0, self.tool_dock.bounds().bottom() - 1, wi, 1),
                    th.border,
                );
                // 메뉴바(드롭다운 포함)는 편집기·그리드·탐색기 뒤인 최상위 패스에서 그린다(09-15 사용자 캡처: 풀다운이 뒤로 가림).
                // 상태줄
                let sy = hi - px(24.0, s);
                dc.fill_rect(Rect::new(0, sy, wi, px(24.0, s)), th.chrome_bg);
                dc.fill_rect(Rect::new(0, sy, wi, 1), th.border);
                dc.select_font(FontSlot::Base, false);
                let busy = if self.sess.blocked() { "⏳ " } else { "" };
                let ty = dc.text_center_y(sy, px(24.0, s));
                // 전용 세션 탭 = 상태줄 앞에 "전용: 접속 설명"(Golden의 Private Session 줄 · docs/52 §7).
                let broken = if self.sess.broken {
                    format!("[{}] ", t(Msg::StSessBrokenTag))
                } else {
                    String::new()
                };
                let left_text = if self.sess.is_private() {
                    format!(
                        "{busy}{broken}[{}: {}] {}",
                        t(Msg::StSessPrivate),
                        if self.sess.desc.is_empty() {
                            "—"
                        } else {
                            self.sess.desc.as_str()
                        },
                        self.sess.status
                    )
                } else {
                    format!("{busy}{broken}{}", self.sess.status)
                };
                // 오른쪽 세그먼트(Sublime/DBeaver/Golden 참고 · docs/29 §4): 접속 · Ln,Col · rows · time · 구문(클릭 = Set Syntax)
                let (ln, col) = self.editors.caret_line_col();
                let mut segs: Vec<(String, bool)> = Vec::new();
                // 트랜잭션 모드(자동/수동 · 수동에 미커밋 변경이 있으면 ●).
                // 트랜잭션 세그먼트(DR-30): Auto / Manual / "Manual ● n pending · since hh:mm" · 클릭 = 팝업.
                let tx = if self.settings.flag("session.autocommit") {
                    t(Msg::StTxAuto).to_string()
                } else if let Some(first) = self.sess.tx_pending.first() {
                    let base = tf(
                        Msg::StTxPending,
                        &[&self.sess.tx_pending.len().to_string(), &first.when],
                    );
                    // 내 미커밋이 남을 막고 있으면 상태줄에도(docs/56 L3).
                    if self.sess.tx_blockers > 0 {
                        format!(
                            "{base} · {}",
                            tf(Msg::StatusTxBlocking, &[&self.sess.tx_blockers.to_string()])
                        )
                    } else {
                        base
                    }
                } else {
                    t(Msg::StTxManual).to_string()
                };
                let tx_idx = segs.len();
                segs.push((tx, false));
                // 메모리 총량 세그먼트(docs/80 · `mem.statusbar` · 클릭 = 메모리 맵 창) — 조회는 5 s에 한 번, 그릴 때만.
                let mem_idx = mem_txt.map(|txt| {
                    segs.push((txt, false));
                    segs.len() - 1
                });
                // 접속 세그먼트 = 프로필 이름만(URL은 툴팁 카드·접속 창에 · 사용자 09-15).
                let conn = match self.conn_win.panel.state_ref() {
                    ConnState::Connected(d) => {
                        let name = self.conn_win.active_name();
                        if name.is_empty() {
                            d.clone()
                        } else {
                            name.to_string()
                        }
                    }
                    _ => "—".to_string(),
                };
                segs.push((conn, false));
                // 큰 파일 모드 · 읽기 전용 표식(docs/59) — 이 탭에서 일부 기능이 꺼져 있음을 늘 보이게.
                let (large_level, large_forced) = self.editors.active_large();
                if large_level > 0 && !large_forced {
                    segs.push((tf(Msg::StLargeSeg, &[&large_level.to_string()]), false));
                }
                if self.editors.active_read_only() {
                    segs.push((t(Msg::StReadOnlySeg).to_string(), false));
                }
                // 북마크 `이 문서/전체`(docs/69 §6-2 · `bookmark.statusbar`).
                if self.bookmarks.enabled && self.settings.flag("bookmark.statusbar") {
                    let (here, all) = self
                        .bookmarks
                        .counts_for(&self.editors, self.editors.active());
                    if all > 0 {
                        segs.push((
                            tf(Msg::StBookmarkSeg, &[&here.to_string(), &all.to_string()]),
                            false,
                        ));
                    }
                }
                let nsel = self.editors.selection_count();
                if self.focus == Focus::Grid
                    && self
                        .grid
                        .selection_summary()
                        .is_some_and(|(r, c, _)| r * c > 1)
                {
                    let (r, c, _) = self.grid.selection_summary().unwrap_or((0, 0, 0));
                    segs.push((tf(Msg::StGridSel, &[&r.to_string(), &c.to_string()]), false));
                } else if nsel > 1 {
                    // 열 모드·다중 커서 = 선택 영역 수(Sublime "5 selection regions").
                    segs.push((tf(Msg::StSelections, &[&nsel.to_string()]), false));
                } else if let Some((l, c)) = self.editors.selection_summary() {
                    // 일반 선택 = 줄 수·문자 수(Sublime "6 lines, 90 characters selected").
                    segs.push((
                        tf(Msg::StSelected, &[&l.to_string(), &c.to_string()]),
                        false,
                    ));
                } else {
                    segs.push((tf(Msg::StPos, &[&ln.to_string(), &col.to_string()]), false));
                }
                if let Some(n) = self.sess.last_rows {
                    segs.push((tf(Msg::StRowsShort, &[&n.to_string()]), false));
                }
                if let Some(secs) = self.sess.last_secs {
                    segs.push((format!("{secs:.3}s"), false));
                }
                // 들여쓰기 세그먼트(Sublime "Tab Size: 4"/"Spaces: 4" · 구문 왼쪽 · 클릭 = 팝업 · 사용자 09-15).
                let (tsz, ispaces) = self.editors.indent();
                let ts = tsz.to_string();
                let indent_seg = if ispaces {
                    tf(Msg::StSpaces, &[&ts])
                } else {
                    tf(Msg::StTabSize, &[&ts])
                };
                // ★ 라이선스 배지(docs/23 §4-4 · D-44: 무료 = "non-commercial use only" 상시) — 클릭 = 라이선스 창.
                let lic_idx = segs.len();
                segs.push((Self::license_badge_of(&self.licensing), false));
                // git 세그먼트(Sublime `main ⑥` · 활성 파일 폴더 · 설정 `statusbar.git` · 배경 조회).
                if self.settings.flag("statusbar.git") {
                    let dir = self
                        .editors
                        .active_path()
                        .and_then(|p| p.parent().map(Path::to_path_buf));
                    self.git.set_dir(dir);
                    self.git.refresh(false);
                    if let Some(info) = self.git.info() {
                        segs.push((format!("{} ({})", info.branch, info.changed), false));
                    }
                }
                // 인코딩 세그먼트(클릭 = 저장 인코딩 / 다시 열기 팝업 · 사용자 09-16).
                segs.push((enc::short(&self.editors.active_encoding()), true));
                // 줄끝 세그먼트(VS Code/Sublime식 · 클릭 = LF/CRLF 팝업 · docs/38 · 사용자 09-16).
                segs.push((self.editors.active_eol().label().to_string(), true));
                segs.push((indent_seg, true));
                segs.push((self.editors.syntax_name(), true));
                let gap = px(12.0, s);
                let mut xr = wi - px(8.0, s);
                self.status_syntax_rect = Rect::new(0, 0, 0, 0);
                self.status_tab_rect = Rect::new(0, 0, 0, 0);
                self.status_eol_rect = Rect::new(0, 0, 0, 0);
                self.status_enc_rect = Rect::new(0, 0, 0, 0);
                self.status_tx_rect = Rect::new(0, 0, 0, 0);
                self.status_mem_rect = Rect::new(0, 0, 0, 0);
                self.status_lic_rect = Rect::new(0, 0, 0, 0);
                let last = segs.len() - 1;
                for (idx, (text, is_syntax)) in segs.iter().enumerate().rev() {
                    let tw = dc.text_width(text);
                    xr -= tw;
                    let r = Rect::new(xr - gap / 2, sy, tw + gap, px(24.0, s));
                    if idx == tx_idx {
                        self.status_tx_rect = r;
                    }
                    if idx == lic_idx {
                        self.status_lic_rect = r;
                    }
                    if Some(idx) == mem_idx {
                        self.status_mem_rect = r;
                        // 클릭 효과 = 상태 레이어(hover · pressed · 버튼과 같은 부품).
                        let st = if self.mem_pressed {
                            nexa_ctl::tokens::State::Pressed
                        } else if self.pointer.is_some_and(|p| r.contains(p)) {
                            nexa_ctl::tokens::State::Hover
                        } else {
                            nexa_ctl::tokens::State::Rest
                        };
                        dc.state_layer(r, th.text, st);
                    }
                    let ty = dc.text_center_y(sy, px(24.0, s));
                    dc.text(
                        xr,
                        ty,
                        r,
                        text,
                        if *is_syntax { th.text } else { th.text_dim },
                    );
                    // 오른쪽 끝부터: 구문 · 들여쓰기 · 줄끝 · 인코딩(클릭 가능한 4개 · 그 앞은 표시만).
                    if *is_syntax {
                        match last - idx {
                            0 => self.status_syntax_rect = r,
                            1 => self.status_tab_rect = r,
                            2 => self.status_eol_rect = r,
                            3 => self.status_enc_rect = r,
                            _ => {}
                        }
                    }
                    xr -= gap;
                    dc.fill_rect(
                        Rect::new(xr + gap / 2, sy + px(5.0, s), 1, px(14.0, s)),
                        th.border,
                    );
                }
                // 왼쪽 상태 문구는 세그먼트 앞에서 잘라 겹치지 않게(09-16 캡처: 긴 타이밍 문구가 세그먼트 위로 지나갔다).
                dc.select_font(FontSlot::Base, false);
                let left_w = (xr - px(8.0, s) - px(4.0, s)).max(0);
                // 접속 유형 표식(운영 = 위험색 · 시험 = 경고색 칩 · 상태줄 맨 앞) — 지금 탭의 세션이 붙어 있을 때만.
                let env_chip = self
                    .sess
                    .connected
                    .then(|| self.sess.spec.as_ref().and_then(|sp| sp.env))
                    .flatten()
                    .and_then(|e| match e {
                        nsql_script::ConnEnv::Prod => Some(("PROD", th.danger)),
                        nsql_script::ConnEnv::Test => Some(("TEST", th.warn)),
                        nsql_script::ConnEnv::Dev => None,
                    });
                let mut lx = px(8.0, s);
                if let Some((label, color)) = env_chip {
                    let cw = dc.text_width(label) + px(12.0, s);
                    let chip = Rect::new(lx, sy + px(4.0, s), cw, px(16.0, s));
                    dc.fill_round_rect(chip, px(3.0, s), color);
                    let cy = dc.text_center_y(chip.y, chip.h);
                    dc.text(chip.x + px(6.0, s), cy, chip, label, th.panel_bg);
                    lx += cw + px(6.0, s);
                }
                dc.text(
                    lx,
                    ty,
                    Rect::new(0, sy, px(8.0, s) + left_w, px(24.0, s)),
                    &left_text,
                    th.text_dim,
                );
            }
            mark(&mut t_sec, &mut marks); // 0 = 크롬(탭·툴바·상태줄)
                                          // ── 고정폭 층(편집기·그리드)
            {
                let prefs = FontPrefs::with_base(mono_px);
                let view_key = self.editors.active_view().map(str::to_string);
                match view_key
                    .as_ref()
                    .and_then(|k| self.ext_details.get(k).cloned())
                {
                    // ★ 뷰 탭(확장 상세) = 편집기 자리에 전용 페이지를 UI 글꼴로 그린다(글 편집기는 그리지 않는다).
                    Some(detail) => {
                        let vprefs = FontPrefs {
                            base: SlotFont {
                                size: ui_px,
                                bold: false,
                                italic: false,
                            },
                            message: SlotFont {
                                size: ui_px * 1.7,
                                bold: true,
                                italic: false,
                            },
                            ..FontPrefs::default()
                        };
                        let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s).with_fonts(vprefs);
                        if view_key.as_deref() != Some(self.ext_view_key.as_str()) {
                            self.ext_view_key = view_key.unwrap_or_default();
                            self.ext_view.reset();
                        }
                        self.ext_view.set_bounds(self.editors.editor_bounds(), s);
                        self.ext_view.paint(&mut dc, &th, &detail);
                    }
                    None => {
                        let mut dc = RasterCtx::new(&mut gfx, &self.mono_font, s)
                            .with_fonts(prefs)
                            .with_caret_on(caret_on);
                        self.editors.paint_bodies(&mut dc, &th);
                    }
                }
            }
            // ★ 객체 상세 본문(고정폭 · 읽기 전용 텍스트박스 · docs/86).
            if self.objdetail.is_visible() && !self.objdetail.is_collapsed() {
                let prefs = FontPrefs::with_base(mono_px);
                let mut dc = RasterCtx::new(&mut gfx, &self.mono_font, s).with_fonts(prefs);
                self.objdetail.paint_body(&mut dc, &th);
            }
            mark(&mut t_sec, &mut marks); // 1 = 편집기
                                          // ── 결과 그리드(고정폭 · 자체 글꼴 크기 `grid.font_size`)
            {
                let prefs = FontPrefs::with_base(grid_px);
                let gf: &Font = self.grid_font.as_ref().unwrap_or(&self.ui_font);
                // 행번호는 고정폭 슬롯(편집기 고정폭 얼굴 · 사용자 09-17) — 자릿수 폭이 흔들리지 않게.
                let fonts = FontSet {
                    mono: Some(&self.mono_font),
                    ..FontSet::single(gf)
                };
                let mut dc = RasterCtx::with_font_set(&mut gfx, fonts, s).with_fonts(prefs);
                // 결과 탭 줄이 바로 위면 그리드의 위 경계선을 끈다(탭 줄 아래선 1px만 · 사용자 09-22).
                let bar = self.panel.bar_visible();
                self.grid.set_top_border(!bar);
                self.grid.set_focused(self.focus == Focus::Grid);
                self.grid.paint(&mut dc, &th, s);
            }
            mark(&mut t_sec, &mut marks); // 2 = 그리드
            if let Some((lines, dur)) = self.grid.take_text_report() {
                dlog!(self, LogLayer::Load, LogLevel::Timing, {
                    LogEntry::new(
                        LogKind::Info,
                        tf(Msg::LogDetTextView, &[&lines.to_string()]),
                    )
                    .elapsed(dur)
                });
            }
            if let Some((render, load, bytes, at)) = self.grid.take_perf_report() {
                self.log_win.push(LogEntry::new(
                    LogKind::Info,
                    tf(
                        Msg::LogRenderPerf,
                        &[
                            &nsql_core::fmt_dur(render),
                            &nsql_core::fmt_dur(load),
                            &nsql_core::fmt_bytes(bytes),
                        ],
                    ),
                ));
                dlog!(self, LogLayer::Load, LogLevel::Timing, {
                    LogEntry::new(
                        LogKind::Info,
                        tf(
                            Msg::LogDetLoad,
                            &[&nsql_core::fmt_dur(load), &nsql_core::fmt_bytes(bytes)],
                        ),
                    )
                    .elapsed(load)
                });
                dlog!(self, LogLayer::Render, LogLevel::Timing, {
                    let (a, b) = at.unwrap_or_default();
                    LogEntry::new(
                        LogKind::Info,
                        tf(Msg::LogDetRender, &[&a, &b, &nsql_core::fmt_dur(render)]),
                    )
                    .elapsed(render)
                });
            }
            // ── 최상위 카드(탭 툴팁 · UI 글꼴)
            {
                let prefs = FontPrefs::with_base(ui_px);
                // In selection 토글 = 편집기에 선택이 있거나 이미 범위가 잡혀 있을 때만(사용자 09-17).
                let sel_ok =
                    self.find_scope.is_some() || self.editors.selection_summary().is_some();
                self.find.set_selection_available(sel_ok);
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s).with_fonts(prefs);
                self.find.paint(&mut dc, &th);
                // 결과 도구줄 상태 글자 = 상태줄과 같은 UI 글꼴·크기(사용자 09-16).
                self.grid.paint_footer_text(&mut dc, &th);
                self.editors.paint_tooltip(&mut dc, &th, wi);
            }
            // ── 오브젝트 탐색기(자체 글꼴 크기 `explorer.font_size` · 기본 = 메뉴 글꼴 · 사용자 09-15)
            {
                let exp_px = match self.settings.font_px("explorer.font_size") {
                    e if e <= 0.0 => self.settings.font_px("ui.menu_font_size"),
                    e => e,
                };
                let prefs = FontPrefs::with_base(exp_px);
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s).with_fonts(prefs);
                self.act_bar.paint(&mut dc, &th);
                self.explorer.set_font_px(exp_px);
                self.explorer.paint(&mut dc, &th);
                if self.objdetail.is_visible() {
                    if !self.objdetail.is_collapsed() {
                        self.split_d.paint(&mut dc, &th);
                    }
                    self.objdetail.paint_header(&mut dc, &th, Instant::now());
                }
                self.search.paint(&mut dc, &th);
                self.project_panel.paint(&mut dc, &th);
                self.bm_panel.paint(&mut dc, &th);
                self.outline_panel.paint(&mut dc, &th);
                self.ext_panel.paint(&mut dc, &th);
            }
            mark(&mut t_sec, &mut marks); // 3 = 탐색기(+카드)
                                          // ── 스플리터(탐색기|편집기 · 편집기|결과) — 본문 위 · hover 시 1초에 걸쳐 진해지는 손잡이(사용자 09-16)
            {
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s);
                self.split_v.paint(&mut dc, &th);
                self.split_h.paint(&mut dc, &th);
            }
            // ── 툴바 툴팁(UI 글꼴) — 툴바 패스에서 그리면 그 뒤에 칠하는 탐색기·편집기가 덮어 툴바 아래 2~3px 띠만
            //    남았다(09-16 Windows 캡처). nexa-ctl `Toolbar::paint_tooltip` 규약대로 팝업 층에서.
            {
                let prefs = FontPrefs::with_base(ui_px);
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s).with_fonts(prefs);
                self.tool_dock.paint_tooltip(&mut dc, &th);
                // 툴바 그룹 드래그 고스트 — 편집기 위까지 나가므로 팝업 층에.
                self.tool_dock.paint_drag_overlay(&mut dc, &th);
                self.find.paint_tooltip(&mut dc, &th);
                self.search.paint_tooltip(&mut dc, &th);
                self.project_panel.paint_tooltip(&mut dc, &th);
                self.project_panel.paint_popup(&mut dc, &th);
                self.bm_panel.paint_popup(&mut dc, &th);
                self.outline_panel.paint_popup(&mut dc, &th);
                self.ext_panel.paint_popup(&mut dc, &th);
                self.objdetail.paint_popup(&mut dc, &th);
                // ★ 팝업(상태줄 메뉴 · 결과 도구줄 툴팁/메뉴 · 팔레트)은 스플리터 **뒤**에 — 앞 층에서 그리면 편집기|결과
                //   구분선이 팝업 위로 지나갔다(09-16 캡처 · 팝업 = 맨 마지막 층 규칙).
                self.status_menu.paint(&mut dc, &th);
                // 자동 완성 팝업(캐럿 아래 · 팝업 층 · docs/76) + 상세 카드(옆 · 같은 높이 · 09-24).
                self.intel.menu.paint(&mut dc, &th);
                if self.intel.is_open() && self.intel.cfg().detail_card {
                    if let Some(target) = self.intel.card_target() {
                        let spec = self.sess.spec.clone();
                        let (names, snap) = self.explorer.meta_view(spec.as_ref());
                        if let Some(card) =
                            intel_card::build(&target, names, &snap, Some(self.sess.dialect))
                        {
                            intel_card::paint(
                                &mut dc,
                                &th,
                                self.intel.menu.bounds(),
                                &card,
                                self.intel.cfg(),
                                s,
                                (wi, hi),
                            );
                        }
                    }
                }
                // ★ 토스트·실행 카드는 팝업(우클릭 메뉴·팔레트) **아래 층**(팝업 = 맨 마지막 규칙 · 사용자 09-22 "우클릭 메뉴가 뒤로 숨음").
                // 토스트 = 편집기 영역의 우하단(결과 그리드를 가리지 않게 · 사용자 09-16 · docs/42).
                let eb = self.editors.editor_bounds();
                let (top, tx, ty) = if eb.w > 0 && eb.h > 0 {
                    (eb.y, eb.right(), eb.bottom())
                } else {
                    (0, wi, hi - px(24.0, s))
                };
                // 실행 카드 누적(아래→위 · 편집기 영역을 넘으면 휠 스크롤 · 사용자 09-22).
                let ty = self.run_toast.paint(&mut dc, &th, top, tx, ty, s);
                let ty = self.tx_warn.paint(&mut dc, &th, tx, ty, s);
                self.ext_banner.set_bounds(self.editors.banner_rect());
                self.ext_banner.paint(&mut dc, &th, s);
                self.toasts.paint(&mut dc, &th, tx, ty, s);
                // 토스트·카드가 바꾼 글꼴 슬롯(Status·굵게)을 되돌린다 — 팝업은 호출자의 글꼴을 쓴다(09-22 메뉴 글자 커짐).
                dc.select_font(FontSlot::Base, false);
                self.grid.paint_overlays(&mut dc, &th);
                self.panel.paint_popups(&mut dc, &th);
                self.editors.paint_popups(&mut dc, &th);
                self.explorer.paint_popups(&mut dc, &th);
                self.palette.paint(&mut dc, &th);
                let eb = self.editors.editor_bounds();
                // ★ 파일 적재 진행 막 — 편집 영역 가운데 · 맨 위 층(적재 중에는 입력도 받지 않는다).
                let delay =
                    Duration::from_millis(self.settings.int("file.load_progress_ms").max(0) as u64);
                let active_tab = self.editors.active_id();
                Self::paint_file_load(&mut self.file_loads, active_tab, delay, eb, &mut dc, &th, s);
            }
            // ── 메뉴바 + 열린 드롭다운(별도 글꼴 크기 `ui.menu_font_size` · 팝업 규칙대로 맨 마지막 층 · 사용자 09-15)
            {
                let prefs = FontPrefs {
                    base: SlotFont {
                        size: self.settings.font_px("ui.menu_font_size"),
                        bold: false,
                        italic: false,
                    },
                    ..FontPrefs::default()
                };
                let mut dc = RasterCtx::new(&mut gfx, &self.ui_font, s).with_fonts(prefs);
                self.menubar.paint(&mut dc, &th);
            }
        }
        mark(&mut t_sec, &mut marks); // 4 = 스플리터·툴팁·메뉴바
        let _ = buf.present();
        mark(&mut t_sec, &mut marks); // 5 = present
        if let Some(tr) = &mut self.frame_trace {
            tr.add(t_frame.elapsed().as_micros() as u32, &marks);
            if let Some(t0) = self.input_at.take() {
                let path: Vec<String> = self
                    .trace_marks
                    .borrow_mut()
                    .drain(..)
                    .map(|(w, t)| format!("{w} +{:.2}", (t - t0).as_secs_f64() * 1000.0))
                    .collect();
                eprintln!(
                    "[frames] input→present {:.2}ms (paint {:.2}ms) · {}",
                    t0.elapsed().as_secs_f64() * 1000.0,
                    t_frame.elapsed().as_secs_f64() * 1000.0,
                    path.join(" · ")
                );
            }
        }
    }

    /// 열(블록) 선택 마우스 규칙(설정 `editor.column_select` · auto = OS별 · 사용자 09-17).
    pub(crate) fn column_rule(&self) -> &'static str {
        match self.settings.get("editor.column_select").unwrap_or("auto") {
            "auto" => {
                if cfg!(target_os = "windows") {
                    "alt_shift"
                } else if cfg!(target_os = "macos") {
                    "alt"
                } else {
                    "shift_right"
                }
            }
            "alt" => "alt",
            "shift_right" => "shift_right",
            _ => "alt_shift",
        }
    }
}
