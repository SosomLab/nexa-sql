//! ★ **SQL Preview 모달 창**(docs/83 §4 · 09-25 · 사용자 "선택된 내용을 모달 방식으로 별도 팝업 · 미니맵 없는 편집 모드 · 하단에
//! 새로고침 · 파일로 저장 · 편집기에서 열기 · 복사 · 닫기"). 탐색기 ▸ Generate SQL의 결과를 보여 준다.
//!
//! 창 골격은 변수 입력 창(`input_win.rs`)과 같다(winit 창 + `Presenter` · 포커스 ≤ 1 · 마우스는 커서 아래 컨트롤에만 · Esc = 닫기).
//! 본문 = nexa-ctl `TextBox`(다중 줄 · SQL 하이라이트 · 줄 번호 · **미니맵 끔** · 편집 가능). "새로고침"은 호스트가 같은 `GenSpec`을
//! 메타 세션에 다시 보내 본문을 바꾼다 · 파일로 저장은 파일 창(`FilePurpose::SqlPreview`) · 편집기에서 열기 = 새 탭 + 이 창 닫기.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{
    Button, Checkbox, Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget,
};
use nexa_gfx::{Font, Surface};
use nsql_catalog::{GenOpts, GenSpec};
use nsql_i18n::{t, tf, Msg};
use nsql_script::ConnectSpec;
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub(crate) enum SqlPrevAction {
    None,
    Paint,
    /// 같은 spec으로 다시 생성(호스트 → 탐색기 → 메타 세션).
    Refresh,
    /// 파일 창(저장)을 연다.
    Save,
    /// 본문을 새 편집기 탭으로(제목 = 파일 이름) · 창은 닫는다.
    OpenEditor,
    /// 선택(없으면 전부)을 클립보드로.
    Copy,
    Close,
}

const BTN_N: usize = 5;
const CB_MSGS: [Msg; 4] = [
    Msg::GenOptQualified,
    Msg::GenOptCompact,
    Msg::GenOptFull,
    Msg::GenOptSepFk,
];
const BTN_MSGS: [Msg; BTN_N] = [
    Msg::SpBtnRefresh,
    Msg::SpBtnSave,
    Msg::SpBtnOpen,
    Msg::SpBtnCopy,
    Msg::SpBtnClose,
];

pub(crate) struct SqlPrevWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    tb: TextBox,
    /// 새로고침 · 파일로 저장 · 편집기에서 열기 · 복사 · 닫기(사용자 순서).
    btns: Vec<Button>,
    labels: Vec<String>,
    /// ★ 생성 옵션 체크박스(DBeaver "Settings" · 사용자 09-25): 정규화 이름 · 간결 · 전체 DDL · FK 분리 — 바꾸면 다시 생성.
    cbs: Vec<Checkbox>,
    cb_labels: Vec<String>,
    /// 지금 보이는 문장의 생성 사양(새로고침용) · 어느 서버 칸에서 왔는가.
    pub(crate) spec: Option<GenSpec>,
    pub(crate) server: Option<ConnectSpec>,
    title: String,
    /// 본문 위 한 줄 안내(오류 · 생성 중 …).
    note: (String, bool),
}

impl SqlPrevWin {
    pub(crate) fn new() -> Self {
        let mut tb = TextBox::new("").with_multiline();
        tb.set_line_numbers(true);
        tb.set_minimap(false);
        SqlPrevWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            tb,
            btns: BTN_MSGS.iter().map(|m| Button::new(t(*m))).collect(),
            labels: BTN_MSGS.iter().map(|m| t(*m).to_string()).collect(),
            cbs: CB_MSGS
                .iter()
                .map(|m| Checkbox::new(t(*m), false))
                .collect(),
            cb_labels: CB_MSGS.iter().map(|m| t(*m).to_string()).collect(),
            spec: None,
            server: None,
            title: String::new(),
            note: (String::new(), false),
        }
    }

    fn desired_size(&self) -> winit::dpi::LogicalSize<f64> {
        winit::dpi::LogicalSize::new(820.0, 560.0)
    }

    /// 체크박스 ↔ 옵션.
    fn cbs_from_opts(&mut self, o: GenOpts) {
        let vals = [o.qualified, o.compact, o.full_ddl, o.separate_fk];
        for (cb, v) in self.cbs.iter_mut().zip(vals) {
            cb.set_checked(v);
        }
    }

    pub(crate) fn opts(&self) -> GenOpts {
        self.spec.as_ref().map_or_else(GenOpts::default, |s| s.opts)
    }

    /// 열기(이미 열려 있으면 본문·제목만 바꾼다).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        owner: Option<&Window>,
        spec: GenSpec,
        server: Option<ConnectSpec>,
        text: Result<String, String>,
        tb: TextBox,
    ) {
        self.title = format!("{} — {}", t(Msg::WinSqlPreview), spec.title());
        self.cbs_from_opts(spec.opts);
        self.spec = Some(spec);
        self.server = server;
        // ★ 편집기 탭과 같은 상자(`Editors::preview_box` · 미니맵·거터 표식만 끔 · 사용자 09-25).
        self.tb = tb;
        // 미리보기 = 읽기 전용(사용자 09-26): 선택·복사·스크롤만 · 잘라내기·붙여넣기·실행 취소는 막는다(메뉴도 비활성 = nexa-ctl `EditMenuCaps.read_only`).
        self.tb.set_read_only(true);
        self.tb.set_popup_deferred(true);
        self.tb.goto_line(1);
        self.tb.set_minimap(false);
        self.set_result(text);
        self.tb.set_focused(true);
        if let Some(w) = &self.window {
            w.set_title(&format!("Nexa SQL — {}", self.title));
            crate::winfocus::focus(w);
            self.redraw();
            return;
        }
        let size = self.desired_size();
        let attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", self.title))
            .with_theme(theme)
            .with_resizable(true)
            .with_inner_size(size);
        let mut attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        if let Some(o) = owner {
            if let Ok(p) = o.outer_position() {
                let s = o.outer_size();
                attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(
                    p.x + s.width as i32 / 2
                        - (size.width * f64::from(o.scale_factor() as f32) / 2.0) as i32,
                    p.y + s.height as i32 / 4,
                ));
            }
        }
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        win.set_ime_allowed(crate::input::system_ime());
        crate::winfocus::focus(&win);
        self.window = Some(win);
        self.redraw();
    }

    /// 생성 결과 반영(새로고침 응답도 여기로) — 오류면 본문은 두고 안내 줄에 빨갛게.
    pub(crate) fn set_result(&mut self, r: Result<String, String>) {
        match r {
            Ok(text) => {
                self.tb.set_text(&text);
                self.note = (String::new(), false);
            }
            Err(e) => self.note = (e, true),
        }
        self.redraw();
    }

    pub(crate) fn set_note(&mut self, note: String) {
        self.note = (note, false);
        self.redraw();
    }

    pub(crate) fn text(&self) -> String {
        self.tb.text()
    }

    /// 안내 줄(자체 시험 덤프용).
    pub(crate) fn note_text(&self) -> &str {
        &self.note.0
    }

    /// 저장·탭 기본 이름(`OBJ_select.sql`).
    pub(crate) fn file_name(&self) -> String {
        match &self.spec {
            Some(sp) => format!("{}.sql", sp.title()),
            None => "preview.sql".into(),
        }
    }

    /// 복사할 글 — 선택이 있으면 그것, 없으면 전부.
    pub(crate) fn copy_text(&self) -> String {
        let all = self.tb.text();
        match self.tb.selection() {
            Some((a, b)) if a != b => {
                let (a, b) = (a.min(b), a.max(b));
                all.chars().skip(a).take(b - a).collect()
            }
            _ => all,
        }
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
        self.tb.set_focused(false);
        for b in &mut self.btns {
            b.clear_transient();
        }
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.window.is_some()
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn key_event(&self, kev: &winit::event::KeyEvent) -> Option<InputEvent> {
        let key = |k: CtlKey| InputEvent::Key {
            key: k,
            shift: self.shift,
            primary: self.primary,
        };
        Some(match kev.logical_key.as_ref() {
            Key::Named(NamedKey::Enter) => key(CtlKey::Enter),
            Key::Named(NamedKey::ArrowUp) => key(CtlKey::Up),
            Key::Named(NamedKey::ArrowDown) => key(CtlKey::Down),
            Key::Named(NamedKey::ArrowLeft) => key(CtlKey::Left),
            Key::Named(NamedKey::ArrowRight) => key(CtlKey::Right),
            Key::Named(NamedKey::Home) => key(CtlKey::Home),
            Key::Named(NamedKey::End) => key(CtlKey::End),
            Key::Named(NamedKey::PageUp) => key(CtlKey::PageUp),
            Key::Named(NamedKey::PageDown) => key(CtlKey::PageDown),
            Key::Named(NamedKey::Delete) => key(CtlKey::Delete),
            Key::Named(NamedKey::Tab) => InputEvent::Char { c: '\t', now_ms: 0 },
            Key::Named(NamedKey::Backspace) => InputEvent::Char {
                c: '\u{8}',
                now_ms: 0,
            },
            Key::Named(NamedKey::Space) => InputEvent::Char { c: ' ', now_ms: 0 },
            Key::Character(t) => {
                if self.primary {
                    return None;
                }
                let c = t.chars().next()?;
                if c.is_control() {
                    return None;
                }
                InputEvent::Char { c, now_ms: 0 }
            }
            _ => return None,
        })
    }

    /// 선택 글(없으면 전체)을 클립보드로 + 안내 줄.
    fn copy_selection_or_all(&mut self) {
        let text = self.tb.copy_selection().unwrap_or_else(|| self.copy_text());
        let n = text.chars().count();
        if crate::clipboard::write_text(&text) {
            self.set_note(tf(Msg::SpCopied, &[&n.to_string()]));
        } else {
            self.set_note(t(Msg::ErrClipboard).to_string());
        }
    }

    /// 우클릭 편집 메뉴가 남긴 요청 처리 — 복사만(읽기 전용 · 잘라내기·붙여넣기는 메뉴에 없다).
    fn apply_edit_ctx(&mut self) {
        if let Some(a) = self.tb.take_edit_ctx() {
            if matches!(a, nexa_ctl::EditCtxAction::Copy) {
                self.copy_selection_or_all();
            }
        }
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> SqlPrevAction {
        let mut inv = Invalidations::default();
        match ev {
            WindowEvent::CloseRequested => return SqlPrevAction::Close,
            WindowEvent::RedrawRequested => return SqlPrevAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                for b in &mut self.btns {
                    b.clear_transient();
                }
                self.redraw();
            }
            WindowEvent::ModifiersChanged(m) => {
                if nexa_ctl::draw::set_show_full(m.state().alt_key()) {
                    self.redraw();
                }
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                self.tb.on_event(&mv, &mut inv);
                if self.tb.popup_open() {
                    // 편집 메뉴가 열려 있으면 이동도 메뉴만(버튼 hover 안 바뀜).
                    self.redraw();
                    return SqlPrevAction::None;
                }
                for b in &mut self.btns {
                    b.on_event(&mv, &mut inv);
                }
                for cb in &mut self.cbs {
                    cb.on_event(&mv, &mut inv);
                }
                if !inv.is_empty() {
                    self.redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = self.cursor;
                if self.tb.bounds().contains(Point { x, y }) {
                    let e = crate::input::wheel_event(delta, self.shift);
                    self.tb.on_event(&e, &mut inv);
                    self.redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } if *button == MouseButton::Left => {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                let e = match state {
                    ElementState::Pressed => InputEvent::MouseDown {
                        x,
                        y,
                        shift: self.shift,
                        primary: self.primary,
                    },
                    ElementState::Released => InputEvent::MouseUp { x, y },
                };
                let up = matches!(e, InputEvent::MouseUp { .. });
                // ★ 편집 메뉴가 열려 있으면 그 클릭은 **메뉴(텍스트박스)에만**(사용자 09-26 "메뉴 클릭이 미리보기 창으로도 전달") ·
                //   고른 항목은 클립보드로(복사 · 전체 선택은 상자가 스스로 · 잘라내기·붙여넣기는 읽기 전용이라 없음).
                if self.tb.popup_open() {
                    self.tb.on_event(&e, &mut inv);
                    self.apply_edit_ctx();
                    self.redraw();
                    return SqlPrevAction::None;
                }
                // 마우스 라우팅 규칙: 누름은 커서 아래 컨트롤에만 · 뗌은 전부.
                if up || self.tb.bounds().contains(p) {
                    self.tb.on_event(&e, &mut inv);
                    self.apply_edit_ctx();
                }
                for b in &mut self.btns {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                    // 포커스 링은 본문 하나(포커스 규칙).
                    b.set_focused(false);
                }
                let mut toggled = false;
                for cb in &mut self.cbs {
                    if up || cb.bounds().contains(p) {
                        cb.on_event(&e, &mut inv);
                    }
                    cb.set_focused(false);
                    if cb.take_toggled().is_some() {
                        toggled = true;
                    }
                }
                self.tb.set_focused(true);
                self.redraw();
                if toggled {
                    // 옵션 → 사양에 반영하고 다시 생성.
                    let vals: Vec<bool> = self.cbs.iter().map(|c| c.is_checked()).collect();
                    if let Some(sp) = self.spec.as_mut() {
                        sp.opts = GenOpts {
                            qualified: vals[0],
                            compact: vals[1],
                            full_ddl: vals[2],
                            separate_fk: vals[3],
                        };
                    }
                    return SqlPrevAction::Refresh;
                }
                let clicked = (0..BTN_N).find(|&i| self.btns[i].take_clicked());
                match clicked {
                    Some(0) => return SqlPrevAction::Refresh,
                    Some(1) => return SqlPrevAction::Save,
                    Some(2) => return SqlPrevAction::OpenEditor,
                    Some(3) => return SqlPrevAction::Copy,
                    Some(4) => return SqlPrevAction::Close,
                    _ => {}
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if *button == MouseButton::Right && *state == ElementState::Pressed =>
            {
                let (x, y) = self.cursor;
                if self.tb.bounds().contains(Point { x, y }) {
                    self.tb.on_event(&InputEvent::RightDown { x, y }, &mut inv);
                    self.redraw();
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.tb.set_preedit("", &mut inv);
                for c in text.chars().filter(|c| !c.is_control()) {
                    self.tb
                        .on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                self.tb.set_preedit(text, &mut inv);
                self.redraw();
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => return SqlPrevAction::Close,
                    Key::Character(c) if self.primary => {
                        let lower = c.to_ascii_lowercase();
                        match lower.as_str() {
                            // ⌘/Ctrl+C = 선택 복사 · 선택이 없으면 전체(사용자 09-26).
                            "c" => {
                                self.copy_selection_or_all();
                                self.redraw();
                                return SqlPrevAction::None;
                            }
                            "a" => {
                                self.tb.on_event(&InputEvent::SelectAll, &mut inv);
                                self.redraw();
                                return SqlPrevAction::None;
                            }
                            // 읽기 전용: 잘라내기·붙여넣기·실행 취소는 없음.
                            "x" | "v" | "z" | "y" => return SqlPrevAction::None,
                            _ => {}
                        }
                    }
                    _ => {}
                }
                if let Some(e) = self.key_event(kev) {
                    self.tb.on_event(&e, &mut inv);
                    self.redraw();
                }
            }
            _ => {}
        }
        SqlPrevAction::None
    }

    pub(crate) fn paint(
        &mut self,
        font: &Font,
        mono_font: &Font,
        th: &Theme,
        ui_px: f32,
        mono_px: f32,
    ) {
        let Some(win) = self.window.clone() else {
            return;
        };
        let Some(mut surface) = self.surface.take() else {
            return;
        };
        let size = win.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            self.surface = Some(surface);
            return;
        };
        if surface.resize(w, h).is_err() {
            self.surface = Some(surface);
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
            self.surface = Some(surface);
            return;
        };
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let px = |v: f32| (v * s).round() as i32;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let slot = |size: f32| SlotFont {
                size,
                bold: false,
                italic: false,
            };
            let prefs = FontPrefs {
                base: slot(ui_px),
                status: slot(ui_px),
                ..FontPrefs::default()
            };
            let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.panel_bg);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            let pad = px(12.0);
            let clip = Rect::new(0, 0, wi, hi);
            // 안내 줄(제목 · 오류는 빨갛게).
            let (note, bad) = &self.note;
            let head = if note.is_empty() {
                self.title.clone()
            } else {
                note.clone()
            };
            dc.text(
                pad,
                pad,
                clip,
                &head,
                if *bad { th.danger } else { th.text_dim },
            );
            let btn_h = th_txt + px(14.0);
            let btn_y = hi - pad - btn_h;
            let cb_h = th_txt + px(8.0);
            let cb_y = btn_y - px(8.0) - cb_h;
            let top = pad + th_txt + px(8.0);
            let inv = &mut Invalidations::default();
            let tb_rect = Rect::new(
                pad,
                top,
                wi - pad * 2,
                (cb_y - px(10.0) - top).max(px(40.0)),
            );
            self.tb.set_scale(s);
            self.tb.set_bounds(tb_rect, inv);
            // 체크박스 행(왼쪽 정렬 · DBeaver "Settings").
            let mut cx = pad;
            for (cb, label) in self.cbs.iter_mut().zip(self.cb_labels.iter()) {
                cb.set_scale(s);
                let w = px(24.0) + dc.text_width(label) + px(12.0);
                cb.set_bounds(Rect::new(cx, cb_y, w, cb_h), inv);
                cb.paint(&mut dc, th);
                cx += w + px(10.0);
            }
            // 버튼(왼쪽 정렬 · 사용자 순서 = 새로고침 · 파일로 저장 · 편집기에서 열기 · 복사 · 닫기).
            let mut bx = pad;
            for (b, label) in self.btns.iter_mut().zip(self.labels.iter()) {
                b.set_scale(s);
                let bw = (dc.text_width(label) + px(28.0)).max(px(84.0));
                b.set_bounds(Rect::new(bx, btn_y, bw, btn_h), inv);
                b.paint(&mut dc, th);
                bx += bw + px(8.0);
            }
        }
        {
            // ★ 본문 = 편집기와 같은 글꼴·크기(`mono_font` · `editor.font_size`)로 따로 그린다(사용자 09-25 "편집기 탭과 동일하게").
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs {
                base: SlotFont {
                    size: mono_px,
                    bold: false,
                    italic: false,
                },
                ..FontPrefs::default()
            };
            let mut dc = RasterCtx::new(&mut gfx, mono_font, s).with_fonts(prefs);
            self.tb.paint(&mut dc, th);
        }
        if self.tb.popup_open() {
            // 우클릭 편집 메뉴 = 맨 마지막 층 · UI 글꼴(09-26).
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs {
                base: SlotFont {
                    size: ui_px,
                    bold: false,
                    italic: false,
                },
                ..FontPrefs::default()
            };
            let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
            self.tb.paint_popup(&mut dc, th);
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
