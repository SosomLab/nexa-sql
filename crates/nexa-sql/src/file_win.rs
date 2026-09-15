//! 파일 열기/저장 창(T-74 · nexa-ui docs/20 F-6) — **자체 대화상자**(네이티브 0 · 3-OS 동일).
//!
//! 본체는 nexa-dlg [`FilePicker`](복합 컨트롤). 이 모듈은 창(winit + softbuffer)과 이벤트 번역만 맡는다 —
//! 접속 창처럼 **모달**(열려 있는 동안 메인·보조 창 입력 차단 · `winfocus::set_enabled`). Esc = 취소 · Enter = 확정.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::Rect;
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nexa_dlg::{FileFilter, FilePicker, PickerAction, PickerLabels, PickerMode};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 창 → 호스트.
pub(crate) enum FileWinAction {
    None,
    Paint,
    /// 확정 경로(모드 · 경로 · 인코딩 값 `auto|utf8|utf8bom|utf16le|utf16be`).
    Confirm(PickerMode, PathBuf, String),
    /// 취소/닫힘.
    Cancel,
    /// 클립보드에 쓸 텍스트(경로/이름 복사).
    CopyText(String),
}

pub(crate) struct FileWin {
    window: Option<Rc<Window>>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    picker: Option<FilePicker>,
    mode: PickerMode,
}

/// 앱 문자열 → 선택기 라벨(i18n 규칙: 리터럴은 여기 없다).
pub(crate) fn labels() -> PickerLabels {
    PickerLabels {
        file_name: t(Msg::LblFileName).into(),
        file_type: t(Msg::LblFileType).into(),
        ok_open: t(Msg::BtnOpen).into(),
        ok_save: t(Msg::BtnSave).into(),
        cancel: t(Msg::BtnCancel).into(),
        new_folder: t(Msg::BtnNewFolder).into(),
        new_folder_name: t(Msg::LblNewFolderName).into(),
        show_hidden: t(Msg::LblShowHidden).into(),
        col_name: t(Msg::ColFileName).into(),
        col_modified: t(Msg::ColModified).into(),
        col_size: t(Msg::ColSize).into(),
        col_kind: t(Msg::ColKind).into(),
        kind_folder: t(Msg::KindFolder).into(),
        kind_file: t(Msg::KindFile).into(),
        place_home: t(Msg::PlaceHome).into(),
        place_desktop: t(Msg::PlaceDesktop).into(),
        place_documents: t(Msg::PlaceDocuments).into(),
        place_downloads: t(Msg::PlaceDownloads).into(),
        place_drives: t(Msg::PlaceDrives).into(),
        kind_drive: t(Msg::KindDrive).into(),
        place_recent: t(Msg::PlaceRecent).into(),
        path_hint: t(Msg::PhPath).into(),
        err_not_found: t(Msg::ErrFileNotFound).into(),
        err_exists: t(Msg::ErrFileExists).into(),
        err_bad_name: t(Msg::ErrBadFileName).into(),
        err_list: t(Msg::ErrListDir).into(),
        err_mkdir: t(Msg::ErrMkdir).into(),
        menu_open: t(Msg::MnOpenItem).into(),
        menu_copy_path: t(Msg::MnCopyPath).into(),
        menu_copy_name: t(Msg::ExpCopyName).into(),
        menu_refresh: t(Msg::ExpRefresh).into(),
    }
}

/// SQL 편집기용 필터(SQL · 텍스트 · 전체).
pub(crate) fn sql_filters() -> Vec<FileFilter> {
    vec![
        FileFilter::new(
            t(Msg::FilterSql),
            &["sql", "pls", "plb", "pks", "pkb", "prc", "fnc", "trg"],
        ),
        FileFilter::new(t(Msg::FilterText), &["txt", "md", "json", "csv"]),
        FileFilter::new(t(Msg::FilterAll), &[]),
    ]
}

impl FileWin {
    pub(crate) fn new() -> Self {
        FileWin {
            window: None,
            ctx: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            picker: None,
            mode: PickerMode::Open,
        }
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn is_open(&self) -> bool {
        self.window.is_some()
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        self.window.is_some() && self.picker.as_mut().is_some_and(|p| p.tick(now_ms))
    }

    pub(crate) fn animating(&self) -> bool {
        self.window.is_some() && self.picker.as_ref().is_some_and(FilePicker::animating)
    }

    /// 현재 폴더(닫을 때 `file.last_dir`에 기억) · 숨김 표시 상태.
    pub(crate) fn current_dir(&self) -> Option<PathBuf> {
        self.picker.as_ref().map(|p| p.current_dir().to_path_buf())
    }

    pub(crate) fn show_hidden(&self) -> Option<bool> {
        self.picker.as_ref().map(FilePicker::show_hidden)
    }

    /// 창 열기 — `start` 폴더 · 기본 파일명(저장) · 최근 폴더.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        over: Option<(i32, i32, u32, u32)>,
        owner: Option<&Window>,
        mode: PickerMode,
        start: Option<&Path>,
        default_name: &str,
        recent: Vec<PathBuf>,
        show_hidden: bool,
        encoding: &str,
    ) {
        if let Some(w) = &self.window {
            w.focus_window();
            return;
        }
        self.mode = mode;
        let mut picker = FilePicker::new(mode, start, sql_filters(), labels());
        picker.set_default_name(default_name);
        picker.set_recent(recent);
        picker.set_show_hidden(show_hidden);
        // 하단 인코딩 콤보(Golden/DBeaver 하단 줄 · 세 OS 동일 · 사용자 09-15) — 열기 = 자동 감지 기본 · 저장 = 탭 인코딩.
        let items: Vec<(&str, String)> = match mode {
            PickerMode::Open => vec![
                ("auto", t(Msg::EncAuto).to_string()),
                ("utf8", t(Msg::EncUtf8).to_string()),
                ("utf8bom", t(Msg::EncUtf8Bom).to_string()),
                ("utf16le", t(Msg::EncUtf16Le).to_string()),
                ("utf16be", t(Msg::EncUtf16Be).to_string()),
            ],
            PickerMode::Save => vec![
                ("utf8", t(Msg::EncUtf8).to_string()),
                ("utf8bom", t(Msg::EncUtf8Bom).to_string()),
                ("utf16le", t(Msg::EncUtf16Le).to_string()),
                ("utf16be", t(Msg::EncUtf16Be).to_string()),
            ],
        };
        let refs: Vec<(&str, &str)> = items.iter().map(|(v, l)| (*v, l.as_str())).collect();
        let sel = refs.iter().position(|(v, _)| *v == encoding).unwrap_or(0);
        picker.set_extra(t(Msg::LblEncoding), &refs, sel);
        self.picker = Some(picker);
        let (lw, lh) = (900.0, 580.0);
        let title = match mode {
            PickerMode::Open => t(Msg::WinOpenFile),
            PickerMode::Save => t(Msg::WinSaveFile),
        };
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {title}"))
            .with_theme(theme)
            .with_resizable(true)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh))
            .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 420.0));
        if let Some((x, y, w, h)) = over {
            let cx = x + (w as i32 - lw as i32) / 2;
            let cy = y + (h as i32 - lh as i32) / 2;
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(cx.max(0), cy.max(0)));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(attrs) else {
            self.picker = None;
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        if let Ok(ctx) = softbuffer::Context::new(win.clone()) {
            if let Ok(s) = softbuffer::Surface::new(&ctx, win.clone()) {
                self.surface = Some(s);
            }
            self.ctx = Some(ctx);
        }
        win.set_ime_allowed(true);
        self.window = Some(win);
        self.layout();
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.ctx = None;
        self.window = None;
        self.picker = None;
    }

    fn layout(&mut self) {
        let Some(w) = &self.window else { return };
        let sz = w.inner_size();
        let r = Rect::new(0, 0, sz.width as i32, sz.height as i32);
        if let Some(p) = &mut self.picker {
            p.set_scale(self.scale);
            let mut inv = Invalidations::default();
            p.set_bounds(r, &mut inv);
        }
    }

    fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        self.picker.as_mut().and_then(FilePicker::focused_textbox)
    }

    fn to_input(&self, ev: &WindowEvent) -> Option<InputEvent> {
        let (x, y) = self.cursor;
        let key = |k: CtlKey| InputEvent::Key {
            key: k,
            shift: self.shift,
            primary: false,
        };
        Some(match ev {
            WindowEvent::CursorMoved { position, .. } => InputEvent::MouseMove {
                x: position.x as i32,
                y: position.y as i32,
            },
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => InputEvent::MouseDown {
                    x,
                    y,
                    shift: self.shift,
                    primary: false,
                },
                (ElementState::Released, MouseButton::Left) => InputEvent::MouseUp { x, y },
                (ElementState::Pressed, MouseButton::Right) => InputEvent::RightDown { x, y },
                _ => return None,
            },
            WindowEvent::MouseWheel { delta, .. } => crate::input::wheel_event(delta, self.shift),
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Enter) => key(CtlKey::Enter),
                    Key::Named(NamedKey::ArrowLeft) => key(CtlKey::Left),
                    Key::Named(NamedKey::ArrowRight) => key(CtlKey::Right),
                    Key::Named(NamedKey::ArrowUp) => key(CtlKey::Up),
                    Key::Named(NamedKey::ArrowDown) => key(CtlKey::Down),
                    Key::Named(NamedKey::Home) => key(CtlKey::Home),
                    Key::Named(NamedKey::End) => key(CtlKey::End),
                    Key::Named(NamedKey::Delete) => key(CtlKey::Delete),
                    Key::Named(NamedKey::Backspace) => InputEvent::Char {
                        c: '\u{8}',
                        now_ms: 0,
                    },
                    Key::Named(NamedKey::Space) => InputEvent::Char { c: ' ', now_ms: 0 },
                    Key::Character(t) => {
                        let c = t.chars().next()?;
                        if c.is_control() {
                            return None;
                        }
                        InputEvent::Char { c, now_ms: 0 }
                    }
                    _ => return None,
                }
            }
            _ => return None,
        })
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> FileWinAction {
        match ev {
            WindowEvent::CloseRequested => {
                self.close();
                return FileWinAction::Cancel;
            }
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
                return FileWinAction::None;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
                self.redraw();
                return FileWinAction::None;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                return FileWinAction::None;
            }
            WindowEvent::Ime(ime) => {
                let mut inv = Invalidations::default();
                if let Some(tb) = self.focused_textbox() {
                    match ime {
                        winit::event::Ime::Preedit(t, _) => tb.set_preedit(t, &mut inv),
                        winit::event::Ime::Commit(t) => {
                            tb.set_preedit("", &mut inv);
                            for c in t.chars().filter(|c| !c.is_control()) {
                                tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                            }
                        }
                        _ => {}
                    }
                    self.redraw();
                }
                return FileWinAction::None;
            }
            WindowEvent::KeyboardInput { event: kev, .. }
                if kev.state == ElementState::Pressed
                    && matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) =>
            {
                if self.picker.as_ref().is_some_and(FilePicker::popup_open) {
                    // 열린 콤보가 Esc를 받는다(컨트롤 몫).
                } else {
                    self.close();
                    return FileWinAction::Cancel;
                }
            }
            WindowEvent::RedrawRequested => return FileWinAction::Paint,
            _ => {}
        }
        if let WindowEvent::CursorMoved { position, .. } = ev {
            self.cursor = (position.x as i32, position.y as i32);
            // 헤더 경계 위 = ↔ 커서(컬럼 폭 조절).
            let over = self
                .picker
                .as_ref()
                .is_some_and(|p| p.header_edge_hover(self.cursor.0, self.cursor.1));
            if let Some(w) = &self.window {
                w.set_cursor(if over {
                    winit::window::CursorIcon::ColResize
                } else {
                    winit::window::CursorIcon::Default
                });
            }
        }
        let Some(ie) = self.to_input(ev) else {
            return FileWinAction::None;
        };
        let mut inv = Invalidations::default();
        let mode = self.mode;
        let Some(p) = &mut self.picker else {
            return FileWinAction::None;
        };
        p.on_event(&ie, &mut inv);
        let a = p.take_action();
        let enc = p.extra_value().unwrap_or_else(|| "auto".into());
        self.redraw();
        match a {
            PickerAction::Confirm(path) => {
                self.close();
                FileWinAction::Confirm(mode, path, enc)
            }
            PickerAction::Cancel => {
                self.close();
                FileWinAction::Cancel
            }
            PickerAction::CopyText(t) => FileWinAction::CopyText(t),
            PickerAction::None => FileWinAction::None,
        }
    }

    pub(crate) fn paint(&mut self, ui: &Font, th: &Theme, font_px: f32) {
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
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs {
                base: SlotFont {
                    size: font_px,
                    bold: false,
                    italic: false,
                },
                ..FontPrefs::default()
            };
            let mut dc = RasterCtx::new(&mut gfx, ui, self.scale).with_fonts(prefs);
            dc.fill_rect(
                Rect::new(0, 0, size.width as i32, size.height as i32),
                th.window_bg,
            );
            dc.select_font(FontSlot::Base, false);
            if let Some(p) = &self.picker {
                p.paint(&mut dc, th);
            }
        }
        let _ = buf.present();
    }
}
