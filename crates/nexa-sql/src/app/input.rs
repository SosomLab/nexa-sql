//! App — 입력 경로(키·마우스 라우팅 · 포커스 · IME · docs/61 §2-2).
//!
//! main.rs의 `impl App`에서 기능별로 옮긴 조각(docs/93 §4). 상태는 `App` 한 곳 · 여기는 동작만.

use crate::*;

impl App {
    /// 스플리터 두 개에 마우스 사건을 준다. 소비(드래그 시작·중·끝)했으면 `true` — hover만 바뀐 경우는 다시 그리되 통과.
    fn route_splitters(&mut self, ev: &InputEvent) -> bool {
        let s = self.scale;
        match self.split_v.on_event(ev) {
            SplitEvent::None => {}
            SplitEvent::Hover => self.redraw(),
            SplitEvent::Start => return true,
            SplitEvent::Drag(x) => {
                // 띠 시작 x → 탐색기 폭(논리 px · 설정 범위로 클램프) → `explorer.width`(자동 기억).
                let grip = px(SPLIT_GRIP, s);
                let act_w = px(activity::BAR_W, s);
                let w = ((x + grip / 2 - act_w) as f32 / s).round() as i64;
                let _ = self
                    .settings
                    .set("explorer.width", &w.clamp(160, 800).to_string());
                self.layout();
                self.redraw();
                return true;
            }
            SplitEvent::End => {
                self.persist_settings();
                self.redraw();
                return true;
            }
        }
        // ★ 객체 상세 스플리터(docs/86): 띠 y → 패널 높이(논리 px) → `explorer.details_h`.
        match self.split_d.on_event(ev) {
            SplitEvent::None => {}
            SplitEvent::Hover => self.redraw(),
            SplitEvent::Start => return true,
            SplitEvent::Drag(y) => {
                let grip = px(SPLIT_GRIP, s);
                let b = self.explorer.bounds();
                let bottom = b.y + b.h + grip + self.objdetail.bounds().h;
                let min_h = (self.objdetail.min_h() as f32 / s).round() as i64;
                let h = ((bottom - (y + grip)) as f32 / s).round() as i64;
                let _ = self.settings.set(
                    "explorer.details_h",
                    &h.clamp(min_h.max(80), 1200).to_string(),
                );
                self.layout();
                self.redraw();
                return true;
            }
            SplitEvent::End => {
                self.persist_settings();
                self.redraw();
                return true;
            }
        }
        match self.split_h.on_event(ev) {
            SplitEvent::None => false,
            SplitEvent::Hover => {
                self.redraw();
                false
            }
            SplitEvent::Start => true,
            SplitEvent::Drag(y) => {
                // 띠 시작 y = body_top + editor_h − pad → 편집기 비율(%) → `layout.editor_split_pct`.
                let pad = px(8.0, s);
                let body = self.act_bar.bounds();
                if body.h > 0 {
                    let editor_h = y + pad - body.y;
                    let pct = (editor_h as f32 / body.h as f32 * 100.0).round() as i64;
                    let _ = self
                        .settings
                        .set("layout.editor_split_pct", &pct.clamp(10, 90).to_string());
                    self.layout();
                    self.redraw();
                }
                true
            }
            SplitEvent::End => {
                self.persist_settings();
                self.redraw();
                true
            }
        }
    }

    pub(crate) fn set_focus(&mut self, f: Focus) {
        self.focus = f;
        self.ed_mut().set_focused(f == Focus::Editor);
        self.explorer.set_focused(f == Focus::Explorer);
        self.objdetail.set_focused(f == Focus::Details);
        self.find.set_focused(f == Focus::Find);
        self.search.set_focused(f == Focus::Search);
        self.project_panel.set_focused(f == Focus::Project);
        self.bm_panel.set_focused(f == Focus::Bookmarks);
        self.outline_panel.set_focused(f == Focus::Outline);
        self.ext_panel.set_focused(f == Focus::Ext);
        self.ime_refresh();
    }

    /// ★ 지금 키 입력이 **타입어헤드**로 가는 상태인가(사용자 09-28 "객체 탐색기 방식을 프로젝트·북마크·아웃라인에도"):
    /// 객체 탐색기 포커스 · 또는 프로젝트/북마크/아웃라인 패널 포커스인데 그 패널의 글 입력 상자(필터 · 이름 편집)가 포커스가 아닐 때.
    /// 이 상태에서는 IME를 끊고(조합 글자가 새지 않게) Windows 한/영 모드는 앱이 자판→자모로 바꾼다.
    pub(crate) fn typeahead_target(&self) -> bool {
        match self.focus {
            Focus::Explorer => true,
            Focus::Project => !self.project_panel.wants_ime(),
            Focus::Bookmarks => !self.bm_panel.wants_ime(),
            Focus::Outline => !self.outline_panel.wants_ime(),
            _ => false,
        }
    }

    /// 메인 창 IME 허용 = 글 입력 포커스이거나 **팔레트가 열려 있을 때**(사용자 09-22 "팔레트에 한글 입력이 안 됨" — 포커스가
    /// 그리드/탐색기인 채 팔레트를 열면 IME가 꺼져 한글이 새고 있었다). 앱 조합 모드(T-139)면 늘 끔.
    /// 패널(프로젝트·북마크·아웃라인)은 **글 입력 상자가 포커스일 때만** 허용 — 목록 포커스 = 타입어헤드(09-28). 값이 바뀔 때만 창에 쓴다.
    pub(crate) fn ime_refresh(&mut self) {
        let f = self.focus;
        let palette = self.palette.is_open();
        let allow = input::system_ime()
            && (palette
                || f == Focus::Editor
                || f == Focus::Find
                || f == Focus::Search
                || f == Focus::Ext
                || (matches!(f, Focus::Project | Focus::Bookmarks | Focus::Outline)
                    && !self.typeahead_target()));
        if self.ime_last == Some(allow) {
            return;
        }
        if let Some(w) = &self.window {
            w.set_ime_allowed(allow);
            self.ime_last = Some(allow);
        }
    }

    /// 한글 조합 방식 맞춤(T-139): 설정 × OS × 입력 소스 → nexa-ctl 앱 조합 스위치 + 모든 창의 IME 허용.
    /// 부르는 때 = 기동 · 창 활성화 · 입력 소스 바뀜 알림 · 설정 변경(값이 바뀔 때만 창에 쓴다).
    pub(crate) fn sync_hangul_mode(&mut self) {
        let setting = self
            .settings
            .get("input.hangul_compose")
            .unwrap_or("auto")
            .to_string();
        let korean = if setting == "auto" && cfg!(target_os = "macos") {
            nexa_sys::input_source::is_korean()
        } else {
            None
        };
        let app = input::hangul_app_mode(&setting, cfg!(target_os = "macos"), korean);
        if self.hangul_app == Some(app) {
            return;
        }
        self.hangul_app = Some(app);
        nexa_ctl::controls::set_hangul_app_compose(app);
        input::set_system_ime(!app);
        let f = self.focus;
        self.set_focus(f);
        for w in [
            self.conn_win.window(),
            self.file_win.window(),
            self.prefs_win.window(),
            self.txlog_win.window(),
            self.input_win.window(),
            self.sqlprev_win.window(),
            self.import_win.window(),
            self.vars_win.window(),
        ]
        .into_iter()
        .flatten()
        {
            w.set_ime_allowed(!app);
        }
    }

    /// 포커스 텍스트 박스(IME·편집 컨텍스트 라우팅).
    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        match self.focus {
            // ★ 뷰 탭(확장 상세)이 활성이면 클립보드 동작(⌘/Ctrl+C·A)의 대상은 그 본문 상자(사용자 09-30).
            Focus::Editor if self.editors.active_view().is_some() => {
                Some(self.ext_view.textbox_mut())
            }
            Focus::Editor => Some(self.editors.cur_mut()),
            Focus::Find => self.find.focused_textbox(),
            Focus::Search => self.search.focused_textbox(),
            Focus::Ext => self.ext_panel.focused_textbox(),
            Focus::Project => self.project_panel.focused_textbox(),
            Focus::Bookmarks => self.bm_panel.focused_textbox(),
            Focus::Outline => self.outline_panel.focused_textbox(),
            Focus::Explorer => self.explorer.focused_textbox(),
            Focus::Grid | Focus::Details => None,
        }
    }

    pub(crate) fn ctl_event(&mut self, event: &WindowEvent) -> Option<InputEvent> {
        let (x, y) = self.cursor;
        let key = |k: CtlKey, shift: bool, primary: bool| InputEvent::Key {
            key: k,
            shift,
            primary,
        };
        Some(match event {
            WindowEvent::CursorMoved { position, .. } => InputEvent::MouseMove {
                x: position.x as i32,
                y: position.y as i32,
            },
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, winit::event::MouseButton::Left) => InputEvent::MouseDown {
                    x,
                    y,
                    shift: self.shift,
                    primary: self.primary,
                },
                (ElementState::Released, winit::event::MouseButton::Left) => {
                    InputEvent::MouseUp { x, y }
                }
                (ElementState::Pressed, winit::event::MouseButton::Right) => {
                    // Linux(Sublime) 규칙: Shift+우클릭 = 열 선택 시작 — 좌드래그로 바꿔 보내고 메뉴는 열지 않는다.
                    if self.column_rule() == "shift_right"
                        && self.shift
                        && self.editors.editor_bounds().contains(Point { x, y })
                    {
                        self.col_right_drag = true;
                        self.editors.set_column_mode(true);
                        InputEvent::MouseDown {
                            x,
                            y,
                            shift: false,
                            primary: false,
                        }
                    } else {
                        InputEvent::RightDown { x, y }
                    }
                }
                (ElementState::Released, winit::event::MouseButton::Right)
                    if self.col_right_drag =>
                {
                    self.col_right_drag = false;
                    self.editors.set_column_mode(false);
                    InputEvent::MouseUp { x, y }
                }
                _ => return None,
            },
            // 휠: 가로 성분(틸트 휠·트랙패드)이 있으면 HWheel · Shift+세로 휠 = 가로(관례) · 아니면 세로.
            // 휠 변환은 한 곳(`input::wheel_event` · 방향 반전 설정 포함).
            WindowEvent::MouseWheel { delta, .. } => crate::input::wheel_event(delta, self.shift),
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Enter) => key(CtlKey::Enter, self.shift, self.primary),
                    Key::Named(NamedKey::Escape) => key(CtlKey::Escape, false, false),
                    Key::Named(NamedKey::ArrowUp) => key(CtlKey::Up, self.shift, self.primary),
                    Key::Named(NamedKey::ArrowDown) => key(CtlKey::Down, self.shift, self.primary),
                    // ← → 수식키 번역(Sublime 기본 키맵 · 사용자 09-17): Win/Linux Ctrl = 단어 · Alt = 서브워드 ·
                    // mac ⌥ = 단어 · ⌃ = 서브워드 · ⌘ = 줄 처음/끝(primary 그대로).
                    Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowRight) => {
                        let right =
                            matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::ArrowRight));
                        let (word, subword) = if cfg!(target_os = "macos") {
                            (self.alt, self.ctrl_raw)
                        } else {
                            (self.primary && !self.alt, self.alt && !self.primary)
                        };
                        let k = match (word, subword, right) {
                            (true, _, false) => CtlKey::WordLeft,
                            (true, _, true) => CtlKey::WordRight,
                            (false, true, false) => CtlKey::SubwordLeft,
                            (false, true, true) => CtlKey::SubwordRight,
                            (false, false, false) => CtlKey::Left,
                            (false, false, true) => CtlKey::Right,
                        };
                        let primary = self.primary && !word && !subword;
                        key(k, self.shift, primary)
                    }
                    Key::Named(NamedKey::Home) => key(CtlKey::Home, self.shift, self.primary),
                    Key::Named(NamedKey::End) => key(CtlKey::End, self.shift, self.primary),
                    Key::Named(NamedKey::PageUp) => key(CtlKey::PageUp, self.shift, self.primary),
                    Key::Named(NamedKey::PageDown) => {
                        key(CtlKey::PageDown, self.shift, self.primary)
                    }
                    Key::Named(NamedKey::Delete) => key(CtlKey::Delete, false, false),
                    Key::Named(NamedKey::Backspace) => InputEvent::Char {
                        c: '\u{8}',
                        now_ms: 0,
                    },
                    Key::Named(NamedKey::Tab) => InputEvent::Char { c: '\t', now_ms: 0 },
                    // 수식키(⌘/Ctrl · Control)와 함께 누른 Space는 단축키 후보지 글자가 아니다 — 키맵에 없으면 버린다
                    //   (사용자 09-23 mac: ⌃Space가 표에 없어 공백이 들어갔다). Shift+Space는 글자 그대로.
                    Key::Named(NamedKey::Space) if self.primary || self.ctrl_raw => return None,
                    Key::Named(NamedKey::Space) => InputEvent::Char { c: ' ', now_ms: 0 },
                    Key::Character(t) if !self.primary => {
                        let c = t.chars().next()?;
                        if c.is_control() {
                            return None;
                        }
                        // Windows 탐색기 한글 모드: IME가 없어 라틴이 온다 → 두벌식 자모로(대문자 = 시프트 · 숫자·기호는 그대로).
                        // 프로젝트·북마크·아웃라인 목록 포커스도 같은 길(09-28 · `typeahead_target`).
                        let c = if cfg!(windows) && self.typeahead_target() && self.hangul_mode {
                            nexa_ctl::hangul::jamo_from_qwerty(c, c.is_ascii_uppercase())
                                .unwrap_or(c)
                        } else {
                            c
                        };
                        InputEvent::Char { c, now_ms: 0 }
                    }
                    _ => return None,
                }
            }
            _ => return None,
        })
    }

    /// ★ 팝업 메뉴 배타 규칙(사용자 09-22 "다른 메뉴들도 배타적 배치가 기본"): 사건 전후로 열린 메뉴 집합을 비교해
    /// **새로 열린 메뉴가 있으면 나머지를 전부 닫는다**(마지막에 연 것이 이긴다). 같은 메뉴의 하위 메뉴·툴팁·팔레트는 대상이 아니다.
    pub(crate) fn route(&mut self, ev: InputEvent) {
        let before = self.open_menus();
        self.route_dispatch(ev);
        let after = self.open_menus();
        let fresh = after & !before;
        if fresh != 0 && after != fresh {
            self.close_menu_bits(after & !fresh);
            self.redraw();
        }
    }

    fn route_dispatch(&mut self, ev: InputEvent) {
        if self.frame_trace.is_some() {
            if let InputEvent::Key { .. } = ev {
                self.tmark("route");
            }
        }
        if let InputEvent::MouseMove { x, y } = ev {
            self.pointer = Some(Point { x, y });
            // ★ Ctrl 객체 링크 hover(T-256 · 켜져 있을 때만 셈).
            self.objlink_hover(Point { x, y });
            // 클릭되는 상태줄 항목 위 = hover 선택색(들어오고 나갈 때만 다시 그림 · 사용자 09-28 "버튼처럼").
            let over = self.status_clickable_at(Point { x, y });
            if over != self.status_hover {
                self.status_hover = over;
                self.redraw();
            }
        }
        if matches!(ev, InputEvent::MouseUp { .. }) && (self.mem_pressed || self.autosave_pressed) {
            self.mem_pressed = false;
            self.autosave_pressed = false;
            self.redraw();
        }
        self.route_inner(ev, Invalidations::default());
        self.sync_run_stmt_button();
        self.giant_notice();
        self.regions_cap_notice();
    }

    /// 막힌 거대 편집(docs/60 D-130)을 알린다 — 편집기가 막았다는 표시를 꺼내 상태줄 + 토스트로.
    /// 다중 선택 구간 상한에 걸렸으면 상태줄 안내(`editor.max_occurrences` · 줄 나누기·열 선택·Ctrl+클릭 포함 · docs/72).
    pub(crate) fn regions_cap_notice(&mut self) {
        if self.ed_mut().take_regions_capped() {
            let n = self.editors.selection_count();
            self.sess.status = tf(Msg::StOccurrenceCap, &[&n.to_string()]);
        }
    }

    pub(crate) fn giant_notice(&mut self) {
        let Some((bytes, repeat)) = self.ed_mut().take_giant_blocked() else {
            return;
        };
        let size = nsql_core::fmt_bytes(bytes as u64);
        let msg = tf(
            if repeat {
                Msg::StGiantEditRepeat
            } else {
                Msg::StGiantEditTyping
            },
            &[&size],
        );
        self.toasts.push(
            toast::ToastKind::Error,
            t(Msg::StGiantEditTitle),
            msg.clone(),
        );
        self.sess.status = msg;
        self.redraw();
    }

    pub(crate) fn route_inner(&mut self, ev: InputEvent, mut inv: Invalidations) {
        if let InputEvent::MouseDown { x, y, .. } = ev {
            if self.toasts.click(Point { x, y }) {
                if let Some(a) = self.toasts.take_action() {
                    self.menu_action(&a);
                }
                self.redraw();
                return;
            }
            match self.ext_banner.click(Point { x, y }) {
                extfile::BannerHit::None => {}
                hit => {
                    self.ext_banner_pick(hit);
                    self.redraw();
                    return;
                }
            }
            match self.tx_warn.click(Point { x, y }) {
                txwarn::TxWarnHit::None => {}
                hit => {
                    self.tx_warn_pick(hit);
                    self.redraw();
                    return;
                }
            }
            match self.run_toast.click(Point { x, y }) {
                runtoast::RunToastHit::Stop => {
                    self.stop_run();
                    return;
                }
                runtoast::RunToastHit::Card => {
                    self.redraw();
                    return;
                }
                runtoast::RunToastHit::Copy(sql) => {
                    // 실행 카드의 복사 버튼(사용자 09-23) — 결과 탭 "SQL 복사"와 같은 상태줄 문구.
                    if clipboard::write_text(&sql) {
                        self.sess.status =
                            tf(Msg::StResultSqlCopied, &[&sql.lines().count().to_string()]);
                    } else {
                        self.sess.status = t(Msg::ErrClipboard).into();
                    }
                    self.redraw();
                    return;
                }
                runtoast::RunToastHit::None => {}
            }
        }
        if let InputEvent::Wheel { delta } = ev {
            let p = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            if self.run_toast.wheel(p, delta) {
                self.redraw();
                return;
            }
        }
        if let InputEvent::MouseMove { x, y } = ev {
            if self.run_toast.hover(Point { x, y }) {
                self.redraw();
            }
            if self.tx_warn.hover(Point { x, y }) {
                self.redraw();
            }
            if self.ext_banner.hover(Point { x, y }) {
                self.redraw();
            }
        }
        // 마우스 다운은 포커스를 옮긴다.
        // 우클릭 메뉴가 열리기 전에 "붙여넣기 가능" 여부를 넣어 준다.
        if matches!(ev, InputEvent::RightDown { .. }) {
            // ★ 편집기 우클릭 메뉴의 Format 그룹은 선택 유무·기본 포맷터에 따라 달라진다 → 열리기 직전에 다시 만든다(사용자 09-29).
            self.refresh_menu_extras();
            // 설정 `ui.clipboard_probe`(향상 모드는 끔): 끄면 클립보드를 읽지 않고 붙여넣기를 항상 활성으로.
            let has = !self.settings.flag("ui.clipboard_probe")
                || clipboard::read_text().is_some_and(|s| !s.is_empty());
            if let Some(tb) = self.focused_textbox() {
                tb.set_clipboard_has_text(has);
            }
        }
        let is_mouse = matches!(
            ev,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
        );
        // ★ 우클릭도 **포인터 사건**이다(사용자 09-21 "탐색기 우클릭 새로 고침이 안 된다"): `is_mouse`에는 우클릭이 없어서
        //   탐색기·툴바의 우클릭 분기가 처음부터 닿지 않는 코드였다(메뉴 코드는 있었지만 실제 입력으로는 열리지 않았다).
        //   우클릭 메뉴가 있는 영역은 `is_ptr`로 판정한다.
        let is_ptr = is_mouse || matches!(ev, InputEvent::RightDown { .. });
        // ★ 포인터 캡처 부품(사용자 09-30 "글자 선택 드래그가 있는 컨트롤은 영역 밖에서 놓아도 기본 처리" · 61 §2-2-c): 누른
        //   영역을 기억하고, 커서가 그 영역 밖으로 나간 **이동·놓임**은 커서 아래 컨트롤이 아니라 **누른 영역**에 준다(그 영역이
        //   포커스일 때만 — 팝업·메뉴가 누름을 먹은 경우는 제외). 놓임으로 캡처가 끝난다. 개별 컨트롤마다 고치지 않는다.
        if let InputEvent::MouseMove { x, y } = ev {
            // ★ hover 이탈 통지: 영역이 바뀌면 이전 영역에 "밖" 이동을 한 번(툴팁·hover 정리). 지금은 hover 상태를 스스로 못 걷는
            //   객체 탐색기·객체 상세만(다른 영역은 자기 MouseMove에서 걷는다) — 새 영역이 툴팁을 갖게 되면 여기 한 줄.
            let now_area = self.area_at(Point { x, y });
            if now_area != self.hover_area {
                let out = InputEvent::MouseMove { x: -1, y: -1 };
                let left = match self.hover_area {
                    Some(Focus::Explorer) => self.explorer.on_event(&out),
                    Some(Focus::Details) => self.objdetail.on_event(&out, Instant::now()),
                    _ => false,
                };
                if left {
                    self.redraw();
                }
                self.hover_area = now_area;
            }
        }
        if let InputEvent::MouseDown { x, y, .. } = ev {
            self.press_capture = self.area_at(Point { x, y });
        } else if is_mouse {
            if let Some(area) = self.press_capture {
                let up = matches!(ev, InputEvent::MouseUp { .. });
                let cur = Point {
                    x: self.cursor.0,
                    y: self.cursor.1,
                };
                let outside = !self.area_bounds(area).contains(cur);
                if up {
                    self.press_capture = None;
                }
                if outside && self.focus == area {
                    self.dispatch_captured(area, ev, &mut inv);
                    self.redraw();
                    return;
                }
            }
        }
        // ★ 탐색기 우클릭 메뉴는 **창 위에** 뜬다(탐색기 폭에 갇히지 않는다 · 09-21) → 열려 있는 동안은 다른 영역(분할선·탭 바·
        //   편집기)보다 **먼저** 사건을 받는다. 메뉴 안 = 메뉴가 먹는다 · 키·휠 = 메뉴가 먹는다 · 바깥 클릭 = 메뉴를 닫고 그 클릭은
        //   그대로 아래로 흘려 보낸다(CLAUDE.md §3 팝업 규칙 — 다음 동작이 한 번의 입력으로 이어지게).
        if self.explorer.is_visible() && self.explorer.menu_open() {
            let at = match ev {
                InputEvent::MouseDown { x, y, .. }
                | InputEvent::RightDown { x, y }
                | InputEvent::MouseUp { x, y }
                | InputEvent::MouseMove { x, y } => Some(Point { x, y }),
                _ => None,
            };
            let inside = at.is_none_or(|p| self.explorer.menu_bounds().contains(p));
            if self.explorer.on_event(&ev) {
                self.redraw();
            }
            if self.explorer_actions() {
                self.redraw();
            }
            // ★ 바깥 **클릭**만 아래로 흘린다(메뉴를 닫고 그 클릭을 그대로 진행). 마우스 **이동**은 메뉴 밖이어도 여기서 끝낸다 —
            //   아래로 흘리면 다른 영역(열려 있던 탭 표식 메뉴 · 탭 툴팁)이 그 이동을 받아 포커스를 편집기로 옮기고, 탐색기는
            //   포커스를 잃으면서 메뉴를 닫는다(사용자 09-21 "항목으로 가는 사이 메뉴가 사라져 누를 수 없다").
            let outside_click = !inside
                && matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                );
            if !outside_click {
                return;
            }
        }
        // ★ MouseUp은 커서가 어디에 있든 **편집기에도** 전달한다 — 탭 바·탐색기 위에서 놓으면 편집기가 드래그 끝을
        //   못 받아 다음 MouseMove가 선택을 바꾸던 결함(사용자 09-15). 중복 전달은 무해(dragging=false 멱등).
        if matches!(ev, InputEvent::MouseUp { .. }) {
            self.ed_mut().on_event(&ev, &mut inv);
            // ★ 결과 그리드도 같다(사용자 09-29): 그리드 안에서 누르고 밖(메뉴·상태줄)에서 놓으면 드래그가 남아 이동마다
            //   가로 스크롤이 일어났다 → 드래그 중이면 어디서 놓든 그리드에 MouseUp.
            if self.grid.dragging() {
                self.grid.on_event(&ev, self.scale);
            }
        }
        // ★ 결과 그리드 컬럼 이동 중(09-19): Esc = 취소(포커스와 무관하게).
        if self.grid.col_dragging()
            && matches!(
                ev,
                InputEvent::Key {
                    key: CtlKey::Escape,
                    ..
                }
            )
        {
            self.grid.cancel_col_drag();
            self.redraw();
            return;
        }
        // ★ 툴바 그룹 드래그 중(09-19): 마우스는 도크가 잡고 있고(고스트가 도크 밖까지 따라간다) · Esc = 취소.
        if self.tool_dock.is_dragging() {
            if matches!(
                ev,
                InputEvent::Key {
                    key: CtlKey::Escape,
                    ..
                }
            ) {
                self.tool_dock.cancel_drag(&mut inv);
                self.drain_dock_actions();
                self.redraw();
                return;
            }
            if is_mouse {
                self.tool_dock.on_event(&ev, &mut inv);
                self.drain_dock_actions();
                self.redraw();
                return;
            }
        }
        // 열린 명령 팔레트는 모달.
        if self.route_palette(ev, &mut inv) {
            return;
        }
        // ★ 자동 완성 팝업(docs/76): ↑↓/Enter/Tab/Esc/휠/클릭은 팝업이 먼저 · 글자·Backspace는 편집기로 가서 다시 거른다 ·
        //   바깥 클릭은 닫고 통과 · 다른 탭이면 닫는다.
        if self.route_completion(ev, is_mouse) {
            return;
        }
        // 상태줄 들여쓰기 팝업(열려 있으면 모달 · 바깥 클릭은 닫고 통과).
        if self.route_status_menu(ev) {
            return;
        }
        // ★ Ctrl 객체 링크(T-256): 우클릭 메뉴가 열려 있으면 모달(바깥 클릭은 닫고 통과) · Ctrl 동안 링크 위 좌/우클릭은
        //   편집기로 가지 않는다(좌 = 설명 복사 · 우 = 메뉴).
        if self.route_objlink_menu(&ev) {
            return;
        }
        if self.objlinks.active {
            let hit = match ev {
                InputEvent::MouseDown { x, y, .. } => self.objlink_click(Point { x, y }, false),
                InputEvent::RightDown { x, y } => self.objlink_click(Point { x, y }, true),
                _ => false,
            };
            if hit {
                return;
            }
        }
        // ★ 스플리터(탐색기|편집기 · 편집기|결과) — 마우스만 · 드래그 중이면 다른 컨트롤보다 먼저(사용자 09-16).
        if is_mouse && self.route_splitters(&ev) {
            return;
        }
        // 상태줄 구문 이름 클릭 → 팔레트(Set Syntax) · 들여쓰기 세그먼트 클릭 → 팝업.
        if let InputEvent::MouseDown { x, y, .. } = ev {
            if self.status_syntax_rect.contains(Point { x, y }) {
                let prefill = format!("{}: ", t(Msg::PalSetSyntax));
                self.open_palette(&prefill);
                return;
            }
            if self.status_tab_rect.contains(Point { x, y }) {
                self.open_indent_menu();
                self.redraw();
                return;
            }
            if self.status_eol_rect.contains(Point { x, y }) {
                self.open_eol_menu();
                self.redraw();
                return;
            }
            if self.status_mem_rect.contains(Point { x, y }) {
                self.mem_pressed = true;
                self.toggle_mem_window();
                self.redraw();
                return;
            }
            if self.status_lic_rect.contains(Point { x, y }) {
                self.open_license = true;
                self.redraw();
                return;
            }
            if self.status_autosave_rect.contains(Point { x, y }) {
                self.autosave_pressed = true;
                self.open_autosave_menu();
                self.redraw();
                return;
            }
            if self.status_tx_rect.contains(Point { x, y }) {
                self.open_tx_menu();
                self.redraw();
                return;
            }
            if self.status_enc_rect.contains(Point { x, y }) {
                self.open_enc_menu();
                self.redraw();
                return;
            }
        }
        // 열린 메뉴는 모달 — 어디를 눌러도 메뉴바가 먼저 받는다. 단 **우클릭**은 풀다운을 닫고 그대로 진행(그 자리의 우클릭 메뉴가 열린다 · 배타).
        if self.menubar.is_open() && !matches!(ev, InputEvent::RightDown { .. }) {
            self.menubar.on_event(&ev, &mut inv);
            if let Some(id) = self.menubar.take_picked() {
                self.menu_action(&id);
            }
            self.redraw();
            return;
        }
        if matches!(ev, InputEvent::RightDown { .. }) && self.menubar.is_open() {
            // 우클릭 메뉴가 열리는 길 = 풀다운은 닫는다(배타).
            self.menubar.dismiss();
        }
        if let InputEvent::RightDown { x, y } = ev {
            if self.tool_dock.bounds().contains(Point { x, y }) {
                self.open_toolbar_menu(x, y);
                self.redraw();
                return;
            }
            // ★ 편집기 거터(북마크/니모닉 영역 + 줄번호) 우클릭 = 북마크 메뉴(사용자 09-23): 그 줄에 캐럿을 두고 상태에 맞춰
            //   "추가"(없을 때) / "제거"·"니모닉 해제"(있을 때) · "니모닉 지정 ▸ 1~9"(늘). 본문 우클릭(편집 메뉴)은 그대로.
            if self.open_bm_gutter_menu(Point { x, y }) {
                self.redraw();
                return;
            }
        }
        if is_mouse {
            self.menubar.on_event(&ev, &mut inv);
            if let Some(id) = self.menubar.take_picked() {
                self.menu_action(&id);
            }
            self.tool_dock.on_event(&ev, &mut inv);
            if let Some(id) = self.tool_dock.take_clicked() {
                self.menu_action(&id);
            }
            self.drain_dock_actions();
            if self.menubar.is_open() {
                // ★ 풀다운이 열리면 다른 팝업(탭·편집·상태줄·탐색기·그리드·결과 메뉴)은 닫는다 — 동시 표시 금지(사용자 09-22).
                self.close_context_menus();
                self.redraw();
                return;
            }
        }
        // 찾기 바 — 마우스는 바 안일 때 · 키/문자는 포커스일 때.
        if self.route_findbar(ev, is_ptr) {
            return;
        }
        // 결과 그리드 우클릭 메뉴가 열려 있으면 그리드가 먼저(바깥 클릭 = 닫고 통과).
        if self.route_grid_menu(ev) {
            return;
        }
        // 활동 막대 — 마우스는 커서 아래일 때만(클릭 = 패널 토글/동작).
        if is_mouse {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            if self.act_bar.bounds().contains(cur) {
                if self.act_bar.on_event(&ev) {
                    self.redraw();
                }
                if let Some(id) = self.act_bar.take_picked() {
                    self.menu_action(id);
                }
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return;
                }
            } else if self.act_bar.clear_hover() {
                self.redraw();
            }
        }
        // 확장 패널 — 마우스는 커서 아래 · 키는 포커스일 때(마우스 라우팅 규칙).
        if self.route_ext_panel(ev, is_mouse) {
            return;
        }
        // 북마크 패널의 우클릭 메뉴는 창 위에 뜬다 → 열린 동안 먼저 받는다(탐색기 메뉴와 같은 규칙 · 바깥 클릭은 닫고 통과).
        if self.route_bm_menu(ev) {
            return;
        }
        // 북마크 패널(docs/69 §6) — 프로젝트 탐색기와 같은 규칙.
        if self.route_bm_panel(ev, is_ptr) {
            return;
        }
        // 아웃라인 패널(docs/76) — 북마크 패널과 같은 규칙(포인터는 안일 때 · 키는 포커스일 때 · 열기 요청은 pump).
        if self.route_outline_panel(ev, is_ptr) {
            return;
        }
        // 프로젝트 탐색기의 우클릭 메뉴는 창 위에 뜬다 → 열린 동안 먼저 받는다(북마크 패널과 같은 규칙 · 바깥 클릭은 닫고 통과).
        if self.route_project_menu(ev) {
            return;
        }
        // 프로젝트 탐색기(docs/67 §4) — 마우스는 커서 아래 · 키는 포커스일 때.
        if self.route_project_panel(ev, is_ptr) {
            return;
        }
        // 파일 검색 패널(T-81a) — 마우스는 커서 아래 · 키는 포커스일 때.
        if self.route_search_panel(ev, is_mouse) {
            return;
        }
        // ★ 객체 상세 패널(docs/86): 마우스 = 패널 안 · 키 = 포커스일 때 — 탐색기보다 먼저(독립 영역).
        if self.route_objdetail(ev, is_ptr) {
            return;
        }
        // ★ 오브젝트 탐색기 — 열린 메뉴는 먼저 · 마우스는 커서 아래 · 키는 포커스일 때.
        if self.route_explorer(ev, is_ptr) {
            return;
        }
        // 편집기 탭 바(클릭·드래그·휠 · 툴팁 호버).
        if self.editors.route_tabs(&ev, &mut inv) {
            if let Some(i) = self.editors.take_tx_close_request() {
                self.close_tab_guarded(i);
            }
            if let Some(req) = self.editors.take_tab_menu_request() {
                self.tab_menu_request(req);
            }
            // 포커스는 **누를 때만** 옮긴다 — 이동(hover·툴팁)으로 옮기면 다른 영역의 포커스에 딸린 것(탐색기 메뉴 · 타입어헤드)이 꺼진다.
            if !matches!(ev, InputEvent::MouseMove { .. }) {
                self.set_focus(Focus::Editor);
            }
            self.redraw();
            return;
        }
        // 결과 탭 바·우클릭 메뉴(T-93) — 커서 아래일 때만(마우스 라우팅 규칙).
        {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            if let Some(act) = self.panel.route(&ev, cur) {
                if let Some(a) = act {
                    self.panel_action(a);
                }
                self.redraw();
                return;
            }
        }
        // ★ 뷰 탭(확장 상세): 편집기 자리의 마우스·휠은 뷰가 받고, 편집기 포커스의 글자·키 입력은 버린다(편집 대상이 아니다).
        if self.route_view_tab(ev, is_mouse) {
            return;
        }
        // ★ 좌클릭·우클릭 모두 커서 아래 컨트롤에 포커스(마우스 라우팅 규칙 · CLAUDE.md §3) — 우클릭이 빠져 있어
        //   편집기에 포커스가 있으면 그리드 우클릭이 편집기로 가서 메뉴가 안 떴다(사용자 09-16 · 좌클릭 뒤에야 동작).
        // ★ 편집기 우클릭 메뉴가 열려 있으면 마우스 사건은 **그 편집기에만**(항목 클릭 = 기능만 · 포커스/칸 활성/캐럿 이동으로
        //   새지 않는다 · 사용자 09-26 "우클릭 후 클릭은 모두 같은 동작"). 바깥 클릭은 상자가 스스로 닫고 통과시킨다.
        //   팝업은 편집기 영역 밖(그리드·탐색기 위)까지 펼쳐지므로 **팝업 사각형 안**의 사건은 커서 아래 영역이 아니라 편집기가 받는다.
        {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let popup_at = |b: Rect| !b.is_empty() && b.contains(cur);
            let ed_popup =
                self.editors.cur().popup_open() && popup_at(self.editors.cur().popup_bounds());
            let grid_popup = self.grid.editing_cell() && popup_at(self.grid.live_popup_bounds());
            if (ed_popup || grid_popup) && (is_mouse || matches!(ev, InputEvent::RightDown { .. }))
            {
                if ed_popup {
                    self.ed_mut().on_event(&ev, &mut inv);
                    let pending = self.ed_mut().take_edit_ctx();
                    if let Some(act) = pending {
                        self.clip_action(act);
                    }
                } else {
                    self.grid.set_shift(self.shift);
                    self.grid.on_event(&ev, self.scale);
                    self.after_grid_event();
                }
                self.redraw();
                return;
            }
        }
        if let InputEvent::MouseDown { x, y, .. } | InputEvent::RightDown { x, y } = ev {
            let p = Point { x, y };
            if self.editors.editor_bounds().contains(p) {
                self.set_focus(Focus::Editor);
                // 동시 편집: 누른 칸의 탭이 활성(키·실행 대상)이 된다.
                if self.editors.activate_pane_at(p) {
                    self.sync_gate();
                }
            } else if self.grid.bounds.contains(p) {
                self.set_focus(Focus::Grid);
            }
        }
        // 스크롤바 호버·드래그: 포커스와 무관하게 **커서 아래** 편집기/그리드가 마우스 사건을 받는다.
        if is_mouse {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            if self.grid.bounds.contains(cur) && self.focus != Focus::Grid {
                self.grid.set_shift(self.shift);
                self.grid.on_event(&ev, self.scale);
                // 포커스가 없어도 스크롤바 드래그가 끝에 닿으면 자동 페치 요청이 생긴다(09-16).
                self.after_grid_event();
                if self.grid.bars_visible() {
                    inv.push(self.grid.bounds);
                }
            }
            if self.editors.editor_bounds().contains(cur)
                && self.focus != Focus::Editor
                && matches!(ev, InputEvent::MouseMove { .. })
            {
                self.editors.box_at_or_cur_mut(cur).on_event(&ev, &mut inv);
            }
        }
        // 휠은 포커스가 아니라 **커서 아래 영역**으로 간다(편집기·그리드·패널).
        let is_wheel = matches!(ev, InputEvent::Wheel { .. } | InputEvent::HWheel { .. });
        let cur = Point {
            x: self.cursor.0,
            y: self.cursor.1,
        };
        if is_wheel && self.grid.bounds.contains(cur) {
            self.grid.set_shift(self.shift);
            self.grid.on_event(&ev, self.scale);
            // ★ 휠은 포커스와 무관하게 오므로 여기서도 요청(스크롤 끝 자동 페치 · 09-16: 휠로는 안 됐다).
            self.after_grid_event();
            inv.push(self.grid.bounds);
        } else if is_wheel && self.editors.editor_bounds().contains(cur) {
            self.editors.box_at_or_cur_mut(cur).on_event(&ev, &mut inv);
        } else {
            let enter = matches!(
                ev,
                InputEvent::Key {
                    key: CtlKey::Enter,
                    ..
                }
            );
            let _ = enter;
            match self.focus {
                Focus::Editor => {
                    self.ed_mut().on_event(&ev, &mut inv);
                    self.tmark("editor on_event");
                    self.intel_after_event(&ev);
                }
                Focus::Grid => {
                    self.grid.set_shift(self.shift);
                    self.grid.on_event(&ev, self.scale);
                    self.after_grid_event();
                    inv.push(self.grid.bounds);
                }
                Focus::Explorer
                | Focus::Find
                | Focus::Search
                | Focus::Ext
                | Focus::Project
                | Focus::Bookmarks
                | Focus::Outline
                | Focus::Details => {}
            }
            // 편집 컨텍스트 요청(우클릭 메뉴 복사·붙여넣기) — 호스트가 OS 클립보드를 잇는다.
            let pending = self.ed_mut().take_edit_ctx();
            if let Some(act) = pending {
                self.clip_action(act);
            }
        }
        if !inv.is_empty() {
            self.redraw();
        }
    }
}

/// `route_inner`의 고리들(docs/93 §4 — 책임 연쇄: 먼저 받는 쪽이 가져간다 · 순서는 `route_inner`가 정한다).
impl App {
    /// 명령 팔레트가 열려 있으면 모든 입력은 팔레트가 먼저. 가져갔으면 `true`(연쇄 끝).
    fn route_palette(&mut self, ev: InputEvent, inv: &mut Invalidations) -> bool {
        if self.palette.is_open() {
            match self.palette.on_event(&ev, inv) {
                PaletteAction::None => {}
                PaletteAction::Close => {
                    self.palette.close();
                    self.ime_refresh();
                }
                PaletteAction::Pick(id) => {
                    self.palette.close();
                    self.ime_refresh();
                    self.menu_action(&id);
                }
                PaletteAction::Prompt { id, text } => {
                    self.palette.close();
                    if let Some(i) = id.strip_prefix("tab.rename:").and_then(|n| n.parse().ok()) {
                        self.editors.rename_tab(i, &text);
                        self.folder_title_remember(i);
                    } else if let Some(rid) = id
                        .strip_prefix("result.rename:")
                        .and_then(|n| n.parse::<u64>().ok())
                    {
                        // 그 사이 편집기 탭을 바꿨어도 결과 탭 id로 찾는다(지금 패널 → 잠든 패널).
                        if !self.panel.rename(rid, &text) {
                            for p in self.panels.values_mut() {
                                if p.rename(rid, &text) {
                                    break;
                                }
                            }
                        }
                    } else if let Some((c, op)) = id.strip_prefix("grid.filter:").and_then(|r| {
                        let (n, o) = r.split_once(':')?;
                        Some((n.parse::<usize>().ok()?, crate::grid::FilterOp::parse(o)?))
                    }) {
                        // 그리드 필터 값 입력(T-181 · 타입별) — 빈 글은 무시.
                        if !text.trim().is_empty() {
                            self.grid.add_filter(c, op, text.trim().to_string());
                        }
                    } else if id == "ext.repo_add" {
                        self.ext_repo_add(&text);
                    }
                }
            }
            self.redraw();
            return true;
        }
        false
    }

    /// 자동 완성 팝업(docs/76): ↑↓/Enter/Tab/Esc/휠/클릭은 팝업이 먼저. 가져갔으면 `true`(연쇄 끝).
    fn route_completion(&mut self, ev: InputEvent, is_mouse: bool) -> bool {
        if self.intel.is_open() {
            let cur_tab = self.editors.tab_id(self.editors.active());
            if self.intel.tab() != Some(cur_tab) || self.focus != Focus::Editor {
                self.intel.close();
            } else {
                let outside = self.intel.menu.is_outside_click(&ev);
                // Tab은 `Char('\t')`로 온다 — 팝업이 열려 있으면 Enter와 같다(확정).
                let is_tab = matches!(ev, InputEvent::Char { c: '\t', .. });
                // ←/→는 라벨이 넘쳐 가로 스크롤이 있을 때만 팝업이(아니면 편집기로 · 09-24).
                let lr = self.intel.menu.label_overflow() > 0
                    && matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Left | CtlKey::Right,
                            ..
                        }
                    );
                let nav = is_tab
                    || lr
                    || matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Up
                                | CtlKey::Down
                                | CtlKey::Enter
                                | CtlKey::Escape
                                | CtlKey::PageUp
                                | CtlKey::PageDown
                                | CtlKey::Home
                                | CtlKey::End,
                            ..
                        }
                    );
                // ★ 키 관통(`intel.key_passthrough` · 사용자 09-24 "Enter를 눌러도 창만 사라진다"): 고른 항목이 없으면 Enter/Tab은
                //   팝업만 닫고 **그 키는 편집기로 그대로** 간다(줄 바꿈·탭 입력) · 끄면 종전처럼 팝업이 삼킨다(두 번 입력).
                let accept_key = is_tab
                    || matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Enter,
                            ..
                        }
                    );
                if nav
                    && accept_key
                    && self.intel.menu.hovered().is_none()
                    && self.intel.cfg().key_passthrough
                {
                    self.intel.close();
                } else if nav {
                    let ev2 = if is_tab {
                        InputEvent::Key {
                            key: CtlKey::Enter,
                            shift: false,
                            primary: false,
                        }
                    } else {
                        ev
                    };
                    // End = 남은 페이지 전부 붙인 뒤 마지막으로(76 §13 D-206).
                    if matches!(
                        ev2,
                        InputEvent::Key {
                            key: CtlKey::End,
                            ..
                        }
                    ) {
                        self.intel.extend_all();
                    }
                    self.intel.menu.on_event(&ev2);
                    // 끝에 닿았으면 다음 페이지(T-196).
                    if self.intel.menu.take_reached_end() {
                        self.intel.extend();
                    }
                    if let Some(id) = self.intel.menu.take_picked() {
                        // Alt를 누른 채 확정 = 한정자 규칙 반대(`A.컬럼` ↔ 컬럼만 · 09-24).
                        self.intel.pick_with(&id, self.alt);
                        self.intel_apply();
                    } else if let Some(full) = self.intel.hovered_full() {
                        // 강조 행의 전체 이름을 상태줄에(폭 상한으로 가운데 …가 된 긴 이름 · 09-23).
                        self.sess.status = full;
                        self.intel_card_settle();
                    }
                    if matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Escape,
                            ..
                        }
                    ) {
                        self.intel.close();
                    }
                    self.redraw();
                    return true;
                }
                // ★ 팝업 밖 휠(사용자 09-24 "영역 밖 스크롤이 안으로 전달") = 팝업을 닫고 편집기로 흘린다(팝업이 본문과 어긋난 채 남지 않게).
                if is_wheel_ev(&ev)
                    && self
                        .pointer
                        .is_some_and(|p| !self.intel.menu.bounds().contains(p))
                {
                    self.intel.close();
                    self.redraw();
                } else if is_mouse || is_wheel_ev(&ev) {
                    let consumed = self.intel.menu.on_event(&ev);
                    if self.intel.menu.take_reached_end() {
                        self.intel.extend();
                    }
                    if let Some(id) = self.intel.menu.take_picked() {
                        // Alt를 누른 채 확정 = 한정자 규칙 반대(`A.컬럼` ↔ 컬럼만 · 09-24).
                        self.intel.pick_with(&id, self.alt);
                        self.intel_apply();
                        self.redraw();
                        return true;
                    }
                    if let Some(full) = self.intel.hovered_full() {
                        self.sess.status = full;
                        self.intel_card_settle();
                    }
                    if outside {
                        self.intel.close();
                    } else if consumed || !matches!(ev, InputEvent::MouseMove { .. }) {
                        self.redraw();
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 상태줄 팝업 메뉴. 가져갔으면 `true`(연쇄 끝).
    /// 클릭되는 상태줄 항목(구문 · 들여쓰기 · 줄끝 · 인코딩 · 트랜잭션 · 라이선스 · 메모리 · 자동 저장) 위인가.
    pub(crate) fn status_clickable_at(&self, p: Point) -> bool {
        [
            self.status_syntax_rect,
            self.status_tab_rect,
            self.status_eol_rect,
            self.status_enc_rect,
            self.status_tx_rect,
            self.status_lic_rect,
            self.status_mem_rect,
            self.status_autosave_rect,
        ]
        .iter()
        .any(|r| r.w > 0 && r.contains(p))
    }

    fn route_status_menu(&mut self, ev: InputEvent) -> bool {
        if self.status_menu.is_open() {
            let consumed = self.status_menu.on_event(&ev);
            if let Some(id) = self.status_menu.take_picked() {
                self.indent_pick(&id);
                self.redraw();
                return true;
            }
            if consumed || !matches!(ev, InputEvent::MouseDown { .. }) {
                self.redraw();
                return true;
            }
        }
        false
    }

    /// 찾기 막대. 가져갔으면 `true`(연쇄 끝).
    fn route_findbar(&mut self, ev: InputEvent, is_ptr: bool) -> bool {
        if self.find.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let in_bar = self.find.bounds().contains(cur);
            // ★ 상자의 우클릭 메뉴가 열려 있으면 바 밖(메뉴가 펼쳐진 곳)의 마우스·키도 찾기 바로(사용자 09-17).
            let popup = self.find.popup_open();
            if popup || (is_ptr && in_bar) || (self.focus == Focus::Find && !is_ptr) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) && in_bar
                {
                    self.set_focus(Focus::Find);
                }
                let a = self.find.on_event(&ev);
                self.find_action(a);
                // 편집 메뉴의 Copy/Cut/Paste는 호스트가 OS 클립보드로 잇는다(편집기와 같은 경로).
                if let Some(act) = self.find.take_edit_ctx() {
                    self.clip_action(act);
                }
                self.redraw();
                if popup || !matches!(ev, InputEvent::MouseMove { .. }) {
                    return true;
                }
            }
        }
        false
    }

    /// 결과 그리드 우클릭 메뉴. 가져갔으면 `true`(연쇄 끝).
    fn route_grid_menu(&mut self, ev: InputEvent) -> bool {
        if self.grid.menu_open() {
            self.grid.set_shift(self.shift);
            self.grid.on_event(&ev, self.scale);
            self.after_grid_event();
            // 항목을 골랐거나 메뉴 안을 눌렀으면 그 클릭은 끝(아래 셀 선택으로 전파 금지 · 사용자 09-15) · 바깥 **좌/우** 클릭만 통과
            //   (우클릭도 — 09-22: 그리드 메뉴가 열린 채 편집기/탭을 우클릭하면 닫히기만 했다).
            if self.grid.menu_open()
                || self.grid.take_menu_click()
                || !matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                )
            {
                self.redraw();
                return true;
            }
        }
        false
    }

    /// 확장 패널. 가져갔으면 `true`(연쇄 끝).
    fn route_ext_panel(&mut self, ev: InputEvent, is_mouse: bool) -> bool {
        if self.ext_panel.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let inside = self.ext_panel.bounds().contains(cur);
            if (is_mouse && inside) || (is_wheel_ev(&ev) && inside) {
                if matches!(ev, InputEvent::MouseDown { .. }) {
                    self.set_focus(Focus::Ext);
                }
                if self.ext_panel.on_event(&ev) {
                    self.redraw();
                }
                self.ext_panel_actions();
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return true;
                }
            } else if self.focus == Focus::Ext
                && matches!(
                    ev,
                    InputEvent::Key { .. }
                        | InputEvent::Char { .. }
                        | InputEvent::SelectAll
                        | InputEvent::Undo
                        | InputEvent::Redo
                )
            {
                if matches!(
                    ev,
                    InputEvent::Key {
                        key: CtlKey::Escape,
                        ..
                    }
                ) && !self.ext_panel.has_query()
                {
                    self.set_focus(Focus::Editor);
                    self.redraw();
                    return true;
                }
                if self.ext_panel.on_event(&ev) {
                    self.redraw();
                }
                return true;
            }
        }
        false
    }

    /// 북마크 패널 우클릭 메뉴. 가져갔으면 `true`(연쇄 끝).
    fn route_bm_menu(&mut self, ev: InputEvent) -> bool {
        if self.bm_panel.is_visible() && self.bm_panel.menu_open() {
            let outside_click = matches!(
                ev,
                InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
            ) && !self.bm_panel.bounds().contains(Point {
                x: self.cursor.0,
                y: self.cursor.1,
            });
            if self.bm_panel.on_event(&ev) {
                self.redraw();
            }
            self.bm_pump();
            if (!outside_click || self.bm_panel.menu_open())
                && !matches!(ev, InputEvent::MouseMove { .. })
            {
                return true;
            }
        }
        false
    }

    /// 북마크 패널. 가져갔으면 `true`(연쇄 끝).
    fn route_bm_panel(&mut self, ev: InputEvent, is_ptr: bool) -> bool {
        if self.bm_panel.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let inside = self.bm_panel.bounds().contains(cur);
            // 우클릭(메뉴)도 포인터 사건 — `is_ptr`(탐색기와 같은 판정 · 09-22 S35: `is_mouse`만 보면 RightDown이 안 닿는다).
            if (is_ptr && inside) || (is_wheel_ev(&ev) && inside) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) {
                    self.set_focus(Focus::Bookmarks);
                }
                if self.bm_panel.on_event(&ev) {
                    self.redraw();
                }
                self.ime_refresh();
                self.bm_pump();
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return true;
                }
            } else if self.focus == Focus::Bookmarks
                && matches!(
                    ev,
                    InputEvent::Key { .. }
                        | InputEvent::Char { .. }
                        | InputEvent::SelectAll
                        | InputEvent::Undo
                        | InputEvent::Redo
                )
            {
                let handled = self.bm_panel.on_event(&ev);
                self.ime_refresh();
                if !handled
                    && matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Escape,
                            ..
                        }
                    )
                {
                    self.set_focus(Focus::Editor);
                    self.redraw();
                    return true;
                }
                if handled {
                    self.redraw();
                }
                self.bm_pump();
                return true;
            }
        }
        false
    }

    /// 아웃라인 패널. 가져갔으면 `true`(연쇄 끝).
    fn route_outline_panel(&mut self, ev: InputEvent, is_ptr: bool) -> bool {
        if self.outline_panel.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let inside = self.outline_panel.bounds().contains(cur);
            if (is_ptr && inside) || (is_wheel_ev(&ev) && inside) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) {
                    self.set_focus(Focus::Outline);
                }
                if self.outline_panel.on_event(&ev) {
                    self.redraw();
                }
                self.ime_refresh();
                self.outline_pump();
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return true;
                }
            } else if self.focus == Focus::Outline
                && matches!(
                    ev,
                    InputEvent::Key { .. }
                        | InputEvent::Char { .. }
                        | InputEvent::SelectAll
                        | InputEvent::Undo
                        | InputEvent::Redo
                )
            {
                let handled = self.outline_panel.on_event(&ev);
                self.ime_refresh();
                if !handled
                    && matches!(
                        ev,
                        InputEvent::Key {
                            key: CtlKey::Escape,
                            ..
                        }
                    )
                {
                    self.set_focus(Focus::Editor);
                    self.redraw();
                    return true;
                }
                if handled {
                    self.redraw();
                }
                self.outline_pump();
                return true;
            }
        }
        false
    }

    /// 프로젝트 패널 우클릭 메뉴. 가져갔으면 `true`(연쇄 끝).
    fn route_project_menu(&mut self, ev: InputEvent) -> bool {
        if self.project_panel.is_visible() && self.project_panel.menu_open() {
            let outside_click = matches!(
                ev,
                InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
            ) && !self.project_panel.bounds().contains(Point {
                x: self.cursor.0,
                y: self.cursor.1,
            });
            if self.project_panel.on_event(&ev) {
                self.redraw();
            }
            self.project_pump();
            if (!outside_click || self.project_panel.menu_open())
                && !matches!(ev, InputEvent::MouseMove { .. })
            {
                return true;
            }
        }
        false
    }

    /// 프로젝트 탐색기 패널. 가져갔으면 `true`(연쇄 끝).
    fn route_project_panel(&mut self, ev: InputEvent, is_ptr: bool) -> bool {
        if self.project_panel.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let inside = self.project_panel.bounds().contains(cur);
            if (is_ptr && inside) || (is_wheel_ev(&ev) && inside) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) {
                    self.set_focus(Focus::Project);
                }
                if self.project_panel.on_event(&ev) {
                    self.redraw();
                }
                self.ime_refresh();
                self.project_pump();
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return true;
                }
            } else if self.focus == Focus::Project
                && matches!(
                    ev,
                    InputEvent::Key { .. }
                        | InputEvent::Char { .. }
                        | InputEvent::SelectAll
                        | InputEvent::Undo
                        | InputEvent::Redo
                )
            {
                if matches!(
                    ev,
                    InputEvent::Key {
                        key: CtlKey::Escape,
                        ..
                    }
                ) {
                    self.set_focus(Focus::Editor);
                    self.redraw();
                    return true;
                }
                if self.project_panel.on_event(&ev) {
                    self.redraw();
                }
                self.ime_refresh();
                self.project_pump();
                return true;
            }
        }
        false
    }

    /// 파일 검색 패널. 가져갔으면 `true`(연쇄 끝).
    fn route_search_panel(&mut self, ev: InputEvent, is_mouse: bool) -> bool {
        if self.search.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let inside = self.search.bounds().contains(cur);
            if (is_mouse && inside) || (is_wheel_ev(&ev) && inside) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) {
                    self.set_focus(Focus::Search);
                }
                if self.search.on_event(&ev) {
                    self.redraw();
                }
                if self.search.take_request() {
                    self.start_search();
                }
                if let Some(req) = self.search.take_open() {
                    self.open_search_result(req);
                }
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return true;
                }
            } else if self.focus == Focus::Search
                && matches!(
                    ev,
                    InputEvent::Key { .. }
                        | InputEvent::Char { .. }
                        | InputEvent::SelectAll
                        | InputEvent::Undo
                        | InputEvent::Redo
                )
            {
                if matches!(
                    ev,
                    InputEvent::Key {
                        key: CtlKey::Escape,
                        ..
                    }
                ) && !self.search.searching()
                {
                    self.set_focus(Focus::Editor);
                    self.redraw();
                    return true;
                }
                if self.search.on_event(&ev) {
                    self.redraw();
                }
                if self.search.take_request() {
                    self.start_search();
                }
                if let Some(req) = self.search.take_open() {
                    self.open_search_result(req);
                }
                return true;
            }
        }
        false
    }

    /// 객체 상세 패널(docs/86). 가져갔으면 `true`(연쇄 끝).
    fn route_objdetail(&mut self, ev: InputEvent, is_ptr: bool) -> bool {
        if self.objdetail.is_visible() {
            // 열린 편집 메뉴 = 먼저(팝업 규칙): 메뉴 안·키·휠은 메뉴가 · 바깥 클릭은 닫고 그 클릭은 아래로(09-26).
            if self.objdetail.menu_open() {
                let at = match ev {
                    InputEvent::MouseDown { x, y, .. } | InputEvent::RightDown { x, y } => {
                        Some(Point { x, y })
                    }
                    _ => None,
                };
                let outside = at.is_some_and(|p| !self.objdetail.menu_bounds().contains(p));
                if outside {
                    self.objdetail.close_menu();
                    self.redraw();
                } else {
                    if self.objdetail.on_event(&ev, Instant::now()) {
                        self.redraw();
                    }
                    self.detail_actions();
                    return true;
                }
            }
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let in_det = self.objdetail.bounds().contains(cur);
            let key_ev = matches!(
                ev,
                InputEvent::Key { .. } | InputEvent::Char { .. } | InputEvent::SelectAll
            );
            if (is_ptr && in_det)
                || (is_wheel_ev(&ev) && in_det)
                || (key_ev && self.focus == Focus::Details)
            {
                if matches!(ev, InputEvent::MouseDown { .. }) {
                    self.set_focus(Focus::Details);
                }
                if self.objdetail.on_event(&ev, Instant::now()) {
                    self.redraw();
                }
                self.detail_actions();
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return true;
                }
            } else if matches!(ev, InputEvent::MouseMove { .. })
                && self.objdetail.on_event(&ev, Instant::now())
            {
                self.redraw();
            }
        }
        false
    }

    /// 오브젝트 탐색기 — 열린 메뉴 먼저 · 마우스 = 커서 아래 · 키 = 포커스. 가져갔으면 `true`(연쇄 끝).
    fn route_explorer(&mut self, ev: InputEvent, is_ptr: bool) -> bool {
        if self.explorer.is_visible() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let in_exp = self.explorer.bounds().contains(cur);
            if self.explorer.menu_open() || (is_ptr && in_exp) || (is_wheel_ev(&ev) && in_exp) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) && !self.explorer.menu_open()
                {
                    // 한 창에 열린 메뉴는 하나 — 탭 표식 메뉴를 열어 둔 채 탐색기에서 우클릭하면 두 메뉴가 함께 떠 있었다.
                    if self.editors.tab_menu_open() {
                        self.editors.close_tab_menu();
                        self.redraw();
                    }
                    self.set_focus(Focus::Explorer);
                }
                if self.explorer.on_event(&ev) {
                    self.redraw();
                }
                if self.explorer_actions() {
                    self.redraw();
                }
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    return true;
                }
            } else if self.focus == Focus::Explorer
                && matches!(ev, InputEvent::Key { .. } | InputEvent::Char { .. })
            {
                if self.explorer.on_event(&ev) {
                    self.redraw();
                }
                if self.explorer_actions() {
                    self.redraw();
                }
                return true;
            }
        }
        false
    }

    /// ★ 포인터가 창을 떠났다 / 창이 비활성이 됐다(사용자 09-30 "툴팁이 남는다 · 비활성이면 마우스 상태 정리"): hover·툴팁·캡처처럼
    ///   마우스가 있어야 뜻이 있는 상태를 전부 걷는다 — 영역마다 "밖" 이동을 한 번 주고 캡처·hover 영역·링크 hover를 비운다.
    pub(crate) fn pointer_gone(&mut self) {
        let out = InputEvent::MouseMove { x: -1, y: -1 };
        let mut changed = false;
        if self.explorer.is_visible() {
            changed |= self.explorer.on_event(&out);
        }
        if self.objdetail.is_visible() {
            changed |= self.objdetail.on_event(&out, Instant::now());
        }
        if self.editors.active_view().is_some() {
            changed |= self.ext_view.on_event(&out);
        }
        self.grid.on_event(&out, self.scale);
        self.hover_area = None;
        self.press_capture = None;
        self.cursor = (-1, -1);
        if self.objlinks.hot.take().is_some() {
            // 편집기의 링크 hover 밑줄도 걷는다.
            let _ = self.editors.cur_mut().set_link_hot(None);
            changed = true;
        }
        if changed {
            self.redraw();
        }
    }

    /// 포인터 캡처: 점 아래의 **영역**(보이는 것만 · 팝업·바·스플리터는 영역이 아니다).
    fn area_at(&self, p: Point) -> Option<Focus> {
        if self.editors.editor_bounds().contains(p) {
            return Some(Focus::Editor);
        }
        if self.grid.area().contains(p) {
            return Some(Focus::Grid);
        }
        let panels: [(Focus, bool, Rect); 8] = [
            (
                Focus::Explorer,
                self.explorer.is_visible(),
                self.explorer.hit_bounds(),
            ),
            (
                Focus::Project,
                self.project_panel.is_visible(),
                self.project_panel.bounds(),
            ),
            (
                Focus::Bookmarks,
                self.bm_panel.is_visible(),
                self.bm_panel.bounds(),
            ),
            (
                Focus::Outline,
                self.outline_panel.is_visible(),
                self.outline_panel.bounds(),
            ),
            (
                Focus::Ext,
                self.ext_panel.is_visible(),
                self.ext_panel.bounds(),
            ),
            (
                Focus::Search,
                self.search.is_visible(),
                self.search.bounds(),
            ),
            (Focus::Find, self.find.is_visible(), self.find.bounds()),
            (
                Focus::Details,
                self.objdetail.is_visible(),
                self.objdetail.bounds(),
            ),
        ];
        panels
            .into_iter()
            .find(|(_, vis, b)| *vis && b.contains(p))
            .map(|(f, _, _)| f)
    }

    fn area_bounds(&self, area: Focus) -> Rect {
        match area {
            Focus::Editor => self.editors.editor_bounds(),
            Focus::Grid => self.grid.area(),
            Focus::Explorer => self.explorer.hit_bounds(),
            Focus::Project => self.project_panel.bounds(),
            Focus::Bookmarks => self.bm_panel.bounds(),
            Focus::Outline => self.outline_panel.bounds(),
            Focus::Ext => self.ext_panel.bounds(),
            Focus::Search => self.search.bounds(),
            Focus::Find => self.find.bounds(),
            Focus::Details => self.objdetail.bounds(),
        }
    }

    /// 캡처된 영역에 이동·놓임을 전달(그 영역의 평소 경로와 같은 후속 처리).
    fn dispatch_captured(&mut self, area: Focus, ev: InputEvent, inv: &mut Invalidations) {
        match area {
            Focus::Editor => {
                if self.editors.active_view().is_some() {
                    self.ext_view.on_event(&ev);
                    self.ext_view_actions();
                } else {
                    self.ed_mut().on_event(&ev, inv);
                    if let Some(act) = self.ed_mut().take_edit_ctx() {
                        self.clip_action(act);
                    }
                }
            }
            Focus::Grid => {
                self.grid.set_shift(self.shift);
                self.grid.on_event(&ev, self.scale);
                self.after_grid_event();
            }
            Focus::Explorer => {
                self.explorer.on_event(&ev);
                self.explorer_actions();
            }
            Focus::Project => {
                self.project_panel.on_event(&ev);
                self.project_pump();
            }
            Focus::Bookmarks => {
                self.bm_panel.on_event(&ev);
                self.bm_pump();
            }
            Focus::Outline => {
                self.outline_panel.on_event(&ev);
                self.outline_pump();
            }
            Focus::Ext => {
                self.ext_panel.on_event(&ev);
                self.ext_panel_actions();
            }
            Focus::Search => {
                self.search.on_event(&ev);
                if self.search.take_request() {
                    self.start_search();
                }
            }
            Focus::Find => {
                let a = self.find.on_event(&ev);
                self.find_action(a);
                if let Some(act) = self.find.take_edit_ctx() {
                    self.clip_action(act);
                }
            }
            Focus::Details => {
                self.objdetail.on_event(&ev, Instant::now());
                self.detail_actions();
            }
        }
    }

    /// 뷰 탭(확장 상세). 가져갔으면 `true`(연쇄 끝).
    fn route_view_tab(&mut self, ev: InputEvent, is_mouse: bool) -> bool {
        if self.editors.active_view().is_some() {
            let cur = Point {
                x: self.cursor.0,
                y: self.cursor.1,
            };
            let in_view = self.editors.editor_bounds().contains(cur);
            let pointer =
                is_mouse || matches!(ev, InputEvent::Wheel { .. } | InputEvent::HWheel { .. });
            // 우클릭 메뉴가 열려 있으면 포인터 사건은 전부 뷰로(항목 선택 · 바깥 클릭 = 닫기).
            let popup = self.ext_view.popup_open();
            if pointer && (in_view || popup) {
                if matches!(
                    ev,
                    InputEvent::MouseDown { .. } | InputEvent::RightDown { .. }
                ) {
                    self.set_focus(Focus::Editor);
                    self.ext_view.set_focused(true);
                }
                if self.ext_view.on_event(&ev) {
                    self.redraw();
                }
                self.ext_view_actions();
                return true;
            }
            if !pointer && self.focus == Focus::Editor {
                // ★ 키·전체 선택 = 읽기 전용 본문 상자(캐럿·선택·스크롤 · 사용자 09-30) — 글은 바뀌지 않는다.
                if self.ext_view.on_event(&ev) {
                    self.redraw();
                }
                return true;
            }
        }
        false
    }
}
