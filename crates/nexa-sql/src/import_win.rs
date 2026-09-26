//! ★ **Import Data 창**(docs/89 §3-3 · T-236 B-3 · 09-26): 탐색기 표 우클릭 ▸ Import Data… → 파일 창 → 이 창.
//! 파일 미리보기(앞 6줄) · 옵션(헤더 · 열 매핑 `src=dst,…` · 배치 · 커밋 간격 · 경로) · 시작 → 세션 워커가 `Runner::import_file`을
//! 돌리고 진행(행 수 · 초)을 보내며 · 취소 = 배치 경계에서 멈춤(현재 배치 롤백) · 끝 = 요약/오류(행·줄 지목).
//!
//! 창 골격은 SQL Preview 창과 같다(winit 창 + `Presenter` · 포커스 ≤ 1 · 마우스는 커서 아래 컨트롤에만 · Esc = 닫기/취소).

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{
    Button, Checkbox, Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget,
};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use nsql_run::bulk::{BulkMode, ImportSpec};
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::rc::Rc;
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub(crate) enum ImportAction {
    None,
    Paint,
    /// 시작(사양은 창의 옵션에서).
    Start(ImportSpec),
    /// 진행 중 취소.
    Cancel,
    Close,
}

const MODES: [BulkMode; 4] = [
    BulkMode::Auto,
    BulkMode::Driver,
    BulkMode::MultiRow,
    BulkMode::Single,
];

pub(crate) struct ImportWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    /// 대상 표(사용자 표기 · 창 제목) · 파일.
    table: String,
    path: PathBuf,
    preview: TextBox,
    cb_header: Checkbox,
    tb_map: TextBox,
    tb_batch: TextBox,
    tb_commit: TextBox,
    btn_mode: Button,
    mode_label: String,
    mode_idx: usize,
    btn_start: Button,
    btn_cancel: Button,
    btn_close: Button,
    /// 진행 중(시작 뒤 끝 보고 전).
    running: bool,
    /// (커밋 행, 경과 초) · 마지막 결과 줄(오류 = 빨강).
    progress: Option<(u64, f64)>,
    result: Option<(String, bool)>,
    /// 자체 시험 덤프용 마지막 보고.
    last_report: String,
}

impl ImportWin {
    pub(crate) fn new() -> Self {
        let mut preview = TextBox::new("").with_multiline();
        preview.set_read_only(true);
        preview.set_minimap(false);
        preview.set_line_numbers(true);
        preview.set_popup_deferred(true);
        ImportWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            table: String::new(),
            path: PathBuf::new(),
            preview,
            cb_header: Checkbox::new(t(Msg::ImpHeader), true),
            tb_map: TextBox::new(""),
            tb_batch: TextBox::new(""),
            tb_commit: TextBox::new(""),
            btn_mode: Button::new(""),
            mode_label: String::new(),
            mode_idx: 0,
            btn_start: Button::new(t(Msg::ImpBtnStart)),
            btn_cancel: Button::new(t(Msg::ImpBtnCancel)),
            btn_close: Button::new(t(Msg::ImpBtnClose)),
            running: false,
            progress: None,
            result: None,
            last_report: String::new(),
        }
    }

    /// 열기 — 표 · 파일 · 미리보기 글(앞 줄들) · 설정 기본값(배치 · 커밋 · 경로).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        owner: Option<&Window>,
        table: String,
        path: PathBuf,
        preview_text: String,
        defaults: (usize, usize, BulkMode),
    ) {
        self.table = table;
        self.path = path;
        self.preview.set_text(&preview_text);
        self.preview.goto_line(1);
        self.tb_batch.set_text(&defaults.0.to_string());
        self.tb_commit.set_text(&defaults.1.to_string());
        self.mode_idx = MODES.iter().position(|m| *m == defaults.2).unwrap_or(0);
        self.sync_mode_label();
        self.running = false;
        self.progress = None;
        self.result = None;
        self.last_report.clear();
        self.sync_buttons();
        if let Some(w) = &self.window {
            w.set_title(&self.title());
            crate::winfocus::focus(w);
            self.redraw();
            return;
        }
        let size = winit::dpi::LogicalSize::new(720.0, 520.0);
        let attrs = Window::default_attributes()
            .with_title(self.title())
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
        self.tb_map.set_focused(true);
        self.redraw();
    }

    fn title(&self) -> String {
        format!("Nexa SQL — {} — {}", t(Msg::WinImport), self.table)
    }

    fn sync_mode_label(&mut self) {
        let m = MODES[self.mode_idx];
        let label = match m {
            BulkMode::Auto => t(Msg::OptBulkAuto),
            BulkMode::Driver => t(Msg::OptBulkDriver),
            BulkMode::MultiRow => t(Msg::OptBulkMultiRow),
            BulkMode::Single => t(Msg::OptBulkSingle),
        };
        self.mode_label = format!("{}: {label}", t(Msg::ImpMode));
        self.btn_mode.set_label(self.mode_label.clone());
    }

    fn sync_buttons(&mut self) {
        self.btn_start.set_enabled(!self.running);
        self.btn_cancel.set_enabled(self.running);
        self.btn_close.set_enabled(!self.running);
    }

    /// 지금 옵션으로 사양을 만든다.
    fn spec(&self) -> ImportSpec {
        let map: Vec<(String, String)> = self
            .tb_map
            .text()
            .split(',')
            .filter_map(|kv| {
                kv.split_once('=')
                    .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
            })
            .collect();
        ImportSpec {
            table: self.table.clone(),
            header: self.cb_header.is_checked(),
            cols: None,
            map,
            batch_rows: self.tb_batch.text().trim().parse().unwrap_or(1000),
            commit_every: self.tb_commit.text().trim().parse().unwrap_or(10_000),
            mode: MODES[self.mode_idx],
            empty_null: true,
            opts: nsql_core::BulkOpts::default(),
            format: nsql_run::bulk::ImportFormat::Auto,
            cancel: None,
        }
    }

    /// 호스트: 시작됨(워커로 보냈다).
    pub(crate) fn set_running(&mut self) {
        self.running = true;
        self.progress = Some((0, 0.0));
        self.result = None;
        self.sync_buttons();
        self.redraw();
    }

    pub(crate) fn set_progress(&mut self, rows: u64, secs: f64) {
        self.progress = Some((rows, secs));
        self.redraw();
    }

    /// 호스트: 끝(요약 또는 오류) — `report` = 덤프용 한 줄.
    pub(crate) fn set_result(&mut self, text: String, bad: bool, report: String) {
        self.running = false;
        self.result = Some((text, bad));
        self.last_report = report;
        self.sync_buttons();
        self.redraw();
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running
    }

    /// 자체 시험 덤프(`import.dump:`): 상태 · 진행 · 결과.
    pub(crate) fn dump(&self) -> String {
        format!(
            "table={} file={} running={} progress={:?} result={:?} report={}\n",
            self.table,
            self.path.display(),
            self.running,
            self.progress,
            self.result
                .as_ref()
                .map(|(s, b)| format!("{}:{s}", if *b { "err" } else { "ok" })),
            self.last_report
        )
    }

    /// 자체 시험: 시작 요청(버튼과 같은 길).
    pub(crate) fn start_spec(&self) -> ImportSpec {
        self.spec()
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
        self.running = false;
        for b in [
            &mut self.btn_start,
            &mut self.btn_cancel,
            &mut self.btn_close,
            &mut self.btn_mode,
        ] {
            b.clear_transient();
        }
        self.tb_map.set_focused(false);
        self.tb_batch.set_focused(false);
        self.tb_commit.set_focused(false);
        self.preview.set_focused(false);
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

    /// 포커스 링은 하나: 텍스트박스 셋 + 미리보기 중 하나만.
    fn focused_box(&mut self) -> Option<&mut TextBox> {
        if self.tb_map.is_focused() {
            Some(&mut self.tb_map)
        } else if self.tb_batch.is_focused() {
            Some(&mut self.tb_batch)
        } else if self.tb_commit.is_focused() {
            Some(&mut self.tb_commit)
        } else if self.preview.is_focused() {
            Some(&mut self.preview)
        } else {
            None
        }
    }

    fn own_focus(&mut self, p: Point) {
        let boxes = [
            self.tb_map.bounds(),
            self.tb_batch.bounds(),
            self.tb_commit.bounds(),
            self.preview.bounds(),
        ];
        let hit = boxes.iter().position(|b| b.contains(p));
        self.tb_map.set_focused(hit == Some(0));
        self.tb_batch.set_focused(hit == Some(1));
        self.tb_commit.set_focused(hit == Some(2));
        self.preview.set_focused(hit == Some(3));
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> ImportAction {
        let mut inv = Invalidations::default();
        match ev {
            WindowEvent::CloseRequested => {
                return if self.running {
                    ImportAction::Cancel
                } else {
                    ImportAction::Close
                };
            }
            WindowEvent::RedrawRequested => return ImportAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                for b in [
                    &mut self.btn_start,
                    &mut self.btn_cancel,
                    &mut self.btn_close,
                    &mut self.btn_mode,
                ] {
                    b.clear_transient();
                }
                self.redraw();
            }
            WindowEvent::ModifiersChanged(m) => {
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
                for b in [
                    &mut self.btn_start,
                    &mut self.btn_cancel,
                    &mut self.btn_close,
                    &mut self.btn_mode,
                ] {
                    b.on_event(&mv, &mut inv);
                }
                self.cb_header.on_event(&mv, &mut inv);
                for tb in [
                    &mut self.tb_map,
                    &mut self.tb_batch,
                    &mut self.tb_commit,
                    &mut self.preview,
                ] {
                    tb.on_event(&mv, &mut inv);
                }
                if !inv.is_empty() {
                    self.redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (x, y) = self.cursor;
                if self.preview.bounds().contains(Point { x, y }) {
                    let e = crate::input::wheel_event(delta, self.shift);
                    self.preview.on_event(&e, &mut inv);
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
                if !up {
                    self.own_focus(p);
                }
                // 마우스 라우팅 규칙: 누름은 커서 아래 컨트롤에만 · 뗌은 전부.
                for tb in [
                    &mut self.tb_map,
                    &mut self.tb_batch,
                    &mut self.tb_commit,
                    &mut self.preview,
                ] {
                    if up || tb.bounds().contains(p) {
                        tb.on_event(&e, &mut inv);
                    }
                }
                if up || self.cb_header.bounds().contains(p) {
                    self.cb_header.on_event(&e, &mut inv);
                    self.cb_header.set_focused(false);
                    let _ = self.cb_header.take_toggled();
                }
                for b in [
                    &mut self.btn_start,
                    &mut self.btn_cancel,
                    &mut self.btn_close,
                    &mut self.btn_mode,
                ] {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                    b.set_focused(false);
                }
                self.redraw();
                if self.btn_mode.take_clicked() {
                    self.mode_idx = (self.mode_idx + 1) % MODES.len();
                    self.sync_mode_label();
                    return ImportAction::None;
                }
                if self.btn_start.take_clicked() && !self.running {
                    return ImportAction::Start(self.spec());
                }
                if self.btn_cancel.take_clicked() && self.running {
                    return ImportAction::Cancel;
                }
                if self.btn_close.take_clicked() && !self.running {
                    return ImportAction::Close;
                }
            }
            WindowEvent::MouseInput { state, button, .. }
                if *button == MouseButton::Right && *state == ElementState::Pressed =>
            {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                for tb in [
                    &mut self.tb_map,
                    &mut self.tb_batch,
                    &mut self.tb_commit,
                    &mut self.preview,
                ] {
                    if tb.bounds().contains(p) {
                        tb.on_event(&InputEvent::RightDown { x, y }, &mut inv);
                    }
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if let Some(tb) = self.focused_box() {
                    tb.set_preedit("", &mut inv);
                    for c in text.chars().filter(|c| !c.is_control()) {
                        tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                    }
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                if let Some(tb) = self.focused_box() {
                    tb.set_preedit(text, &mut inv);
                }
                self.redraw();
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                if matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) {
                    return if self.running {
                        ImportAction::Cancel
                    } else {
                        ImportAction::Close
                    };
                }
                if let Key::Character(_) = kev.logical_key.as_ref() {
                    if self.primary && crate::input::shortcut_letter(kev) == Some('a') {
                        if let Some(tb) = self.focused_box() {
                            tb.on_event(&InputEvent::SelectAll, &mut inv);
                        }
                        self.redraw();
                        return ImportAction::None;
                    }
                }
                if let Some(e) = self.key_event(kev) {
                    if let Some(tb) = self.focused_box() {
                        tb.on_event(&e, &mut inv);
                    }
                    self.redraw();
                }
            }
            _ => {}
        }
        ImportAction::None
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
        let inv = &mut Invalidations::default();
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
            // 1행: 대상 · 파일.
            // 머리줄: 대상 · 파일(긴 경로 = 가운데 축약 · Alt = 전체).
            let head = format!(
                "{} · {}",
                tf(Msg::ImpTarget, &[&self.table]),
                self.path.display()
            );
            let head = nexa_ctl::draw::ellipsize_middle(&mut dc, &head, wi - pad * 2);
            dc.text(pad, pad, clip, &head, th.text_dim);
            let row_h = th_txt + px(14.0);
            let mut y = pad + th_txt + px(10.0);
            // 2행: 헤더 체크 · 매핑 라벨+상자.
            self.cb_header.set_scale(s);
            let cbw = px(24.0) + dc.text_width(t(Msg::ImpHeader)) + px(12.0);
            self.cb_header
                .set_bounds(Rect::new(pad, y, cbw, row_h), inv);
            self.cb_header.paint(&mut dc, th);
            let lx = pad + cbw + px(16.0);
            let map_label = t(Msg::ImpMap);
            dc.text(lx, y + (row_h - th_txt) / 2, clip, map_label, th.text);
            let mx = lx + dc.text_width(map_label) + px(8.0);
            self.tb_map.set_scale(s);
            self.tb_map
                .set_bounds(Rect::new(mx, y, wi - pad - mx, row_h), inv);
            self.tb_map.paint(&mut dc, th);
            y += row_h + px(8.0);
            // 3행: 배치 · 커밋 · 경로 버튼.
            let mut x = pad;
            for (label, tb) in [
                (t(Msg::ImpBatch), &mut self.tb_batch),
                (t(Msg::ImpCommit), &mut self.tb_commit),
            ] {
                dc.text(x, y + (row_h - th_txt) / 2, clip, label, th.text);
                x += dc.text_width(label) + px(8.0);
                tb.set_scale(s);
                tb.set_bounds(Rect::new(x, y, px(90.0), row_h), inv);
                tb.paint(&mut dc, th);
                x += px(90.0) + px(16.0);
            }
            self.btn_mode.set_scale(s);
            let mw = dc.text_width(&self.mode_label) + px(28.0);
            self.btn_mode.set_bounds(Rect::new(x, y, mw, row_h), inv);
            self.btn_mode.paint(&mut dc, th);
            y += row_h + px(8.0);
            // 버튼 행(아래) · 진행/결과 줄(그 위).
            let btn_h = th_txt + px(14.0);
            let btn_y = hi - pad - btn_h;
            let status_y = btn_y - px(8.0) - th_txt;
            let mut bx = pad;
            for (b, label) in [
                (&mut self.btn_start, Msg::ImpBtnStart),
                (&mut self.btn_cancel, Msg::ImpBtnCancel),
                (&mut self.btn_close, Msg::ImpBtnClose),
            ] {
                b.set_scale(s);
                let bw = (dc.text_width(t(label)) + px(28.0)).max(px(84.0));
                b.set_bounds(Rect::new(bx, btn_y, bw, btn_h), inv);
                b.paint(&mut dc, th);
                bx += bw + px(8.0);
            }
            let line = match (&self.result, &self.progress) {
                (Some((text, bad)), _) => (text.clone(), if *bad { th.danger } else { th.ok }),
                (None, Some((rows, secs))) => (
                    tf(Msg::ImpRunning, &[&rows.to_string(), &format!("{secs:.1}")]),
                    th.accent,
                ),
                _ => (t(Msg::ImpHint).to_string(), th.text_dim),
            };
            dc.text(pad, status_y, clip, &line.0, line.1);
            // 미리보기 = 나머지.
            let pv = Rect::new(
                pad,
                y,
                wi - pad * 2,
                (status_y - px(10.0) - y).max(px(40.0)),
            );
            self.preview.set_scale(s);
            self.preview.set_bounds(pv, inv);
        }
        {
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
            self.preview.paint(&mut dc, th);
        }
        for tb in [&self.tb_map, &self.tb_batch, &self.tb_commit, &self.preview] {
            if tb.popup_open() {
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
                tb.paint_popup(&mut dc, th);
            }
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
