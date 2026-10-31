//! 파일 열기/저장 창(T-74 · nexa-ui docs/20 F-6) — **자체 대화상자**(네이티브 0 · 3-OS 동일).
//!
//! 본체는 nexa-dlg [`FilePicker`](복합 컨트롤). 이 모듈은 창(winit + softbuffer)과 이벤트 번역만 맡는다 —
//! 접속 창처럼 **모달**(열려 있는 동안 메인·보조 창 입력 차단 · `winfocus::set_enabled`). Esc = 취소 · Enter = 확정.

use crate::FilePurpose;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::Rect;
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, Theme};
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nexa_dlg::{FileFilter, FilePicker, PickerAction, PickerLabels, PickerMode};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
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
    /// ★ 열기 모드 다중 확정(고른 순서 · 파일만 · 인코딩 · 사용자 09-22).
    ConfirmMany(Vec<PathBuf>, String),
    /// 취소/닫힘.
    Cancel,
    /// 클립보드에 쓸 텍스트(경로/이름 복사).
    CopyText(String),
}

pub(crate) struct FileWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    /// 주 수식키(Windows·Linux = Ctrl · macOS = ⌘) — 다중 선택 토글·Ctrl+A(사용자 09-22).
    primary: bool,
    picker: Option<FilePicker>,
    mode: PickerMode,
    /// 덮어쓰기 무장 시간(ms · 호스트가 설정에서 넣는다).
    overwrite_confirm_ms: u64,
    /// 다음 `open` 한 번에만 쓰는 필터(용도별 · 없으면 SQL 필터).
    next_filters: Option<Vec<FileFilter>>,
}

/// 앱 문자열 → 선택기 라벨(i18n 규칙: 리터럴은 여기 없다).
pub(crate) fn labels() -> PickerLabels {
    PickerLabels {
        file_name: t(Msg::LblFileName).into(),
        file_type: t(Msg::LblFileType).into(),
        ok_open: t(Msg::BtnOpen).into(),
        ok_save: t(Msg::BtnSave).into(),
        ok_folder: t(Msg::BtnSelectFolder).into(),
        folder_name: t(Msg::LblFolderName).into(),
        cancel: t(Msg::BtnCancel).into(),
        new_folder: t(Msg::BtnNewFolder).into(),
        new_folder_name: t(Msg::LblNewFolderName).into(),
        show_hidden: t(Msg::LblShowHidden).into(),
        show_dot: t(Msg::LblShowDot).into(),
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
        overwrite_ask: t(Msg::AskOverwrite).into(),
        overwrite_yes: t(Msg::BtnOverwrite).into(),
        err_bad_name: t(Msg::ErrBadFileName).into(),
        err_list: t(Msg::ErrListDir).into(),
        err_mkdir: t(Msg::ErrMkdir).into(),
        menu_open: t(Msg::MnOpenItem).into(),
        menu_copy_path: t(Msg::MnCopyPath).into(),
        menu_copy_name: t(Msg::ExpCopyName).into(),
        menu_refresh: t(Msg::ExpRefresh).into(),
        menu_rename: t(Msg::MnRenameItem).into(),
        err_rename: t(Msg::ErrRename).into(),
        menu_delete: t(Msg::MnDeleteItem).into(),
        err_delete: t(Msg::ErrDelete).into(),
        perm_delete_ask: t(Msg::PermDeleteAsk).into(),
        menu_undo: t(Msg::MnUndo).into(),
        menu_redo: t(Msg::MnRedo).into(),
        err_undo: t(Msg::ErrUndoFileOp).into(),
        multi_selected: t(Msg::MultiOpenHeader).into(),
    }
}

/// 라이선스 파일 필터(사용자 09-27 "Open 버튼을 누르면 sql이 기본이라 라이선스가 안 보임") — `.license` 기본 · 전체.
pub(crate) fn license_filters() -> Vec<FileFilter> {
    vec![
        FileFilter::new(t(Msg::FilterLicense), &["license"]),
        FileFilter::new(t(Msg::FilterAll), &[]),
    ]
}

const SQL_EXTS: [&str; 8] = ["sql", "pls", "plb", "pks", "pkb", "prc", "fnc", "trg"];

/// SQL 편집기용 필터(SQL · 텍스트 · 전체).
pub(crate) fn sql_filters() -> Vec<FileFilter> {
    vec![
        FileFilter::new(t(Msg::FilterSql), &SQL_EXTS),
        FileFilter::new(t(Msg::FilterText), &["txt", "md", "json", "csv"]),
        FileFilter::new(t(Msg::FilterAll), &[]),
    ]
}

/// ★ 용도별 필터(사용자 10-23 "대상 확장자를 기본으로 · 전체와 다른 유형도 함께"): 첫 줄 = 그 용도의 대상 확장자(기본 선택) ·
///   이어서 다른 유형(쿼리 · 텍스트 · 프로젝트 · 데이터) · 마지막 = 전체. 폴더 고르기는 호출 쪽이 "폴더" 한 줄로 바꾼다.
///   `log_ext` = 로그 내보내기의 현재 형식 확장자(`log.format` · 그 밖 용도는 무시).
pub(crate) fn filters_for(purpose: FilePurpose, log_ext: &str) -> Vec<FileFilter> {
    let sql = || FileFilter::new(t(Msg::FilterSql), &SQL_EXTS);
    let text = || FileFilter::new(t(Msg::FilterText), &["txt", "md", "json", "csv"]);
    let project = || FileFilter::new(t(Msg::FilterProject), &["nsql-project"]);
    let data = || FileFilter::new(t(Msg::FilterData), &["csv", "tsv", "txt", "jsonl", "json"]);
    let all = || FileFilter::new(t(Msg::FilterAll), &[]);
    match purpose {
        FilePurpose::Editor | FilePurpose::RunFile | FilePurpose::SqlPreview => {
            vec![sql(), text(), project(), all()]
        }
        FilePurpose::Project => vec![project(), sql(), all()],
        FilePurpose::LogExport => vec![
            FileFilter::new(tf(Msg::FilterLog, &[log_ext]), &[log_ext]),
            text(),
            all(),
        ],
        // 셀 값 = 어떤 파일이든(이진 포함) → 전체가 기본 · 텍스트·SQL은 좁혀 보기용.
        FilePurpose::CellValue => vec![all(), text(), sql()],
        FilePurpose::Import => vec![data(), text(), sql(), all()],
        // ★ T-8 GUI 2차: 결과 내보내기 = 데이터 형식(확장자 = 형식 · sql = INSERT 문).
        FilePurpose::ResultExport => vec![data(), sql(), all()],
        FilePurpose::License => license_filters(),
        FilePurpose::DriverPackage => vec![
            FileFilter::new(t(Msg::FilterDriverPackage), &["zip"]),
            all(),
        ],
        FilePurpose::SettingFolder(_) => vec![all()],
        FilePurpose::SettingsFile => vec![FileFilter::new(t(Msg::FilterJson), &["json"]), all()],
    }
}

impl FileWin {
    pub(crate) fn new() -> Self {
        FileWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            picker: None,
            mode: PickerMode::Open,
            overwrite_confirm_ms: 5000,
            next_filters: None,
        }
    }

    /// 덮어쓰기 무장 시간(설정 `file.overwrite_confirm_ms` · 다음에 여는 창부터).
    pub(crate) fn set_overwrite_confirm_ms(&mut self, ms: u64) {
        self.overwrite_confirm_ms = ms;
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

    /// 다음 열기 한 번의 필터(용도별 · 예: 라이선스 = `.license` 기본).
    pub(crate) fn set_next_filters(&mut self, f: Option<Vec<FileFilter>>) {
        self.next_filters = f;
    }

    pub(crate) fn show_dot(&self) -> Option<bool> {
        self.picker.as_ref().map(FilePicker::show_dot)
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
        pinned: Vec<(String, PathBuf)>,
        show_hidden: bool,
        show_dot: bool,
        encoding: &str,
        single: bool,
    ) {
        if let Some(w) = &self.window {
            crate::winfocus::focus(w);
            return;
        }
        self.mode = mode;
        // 폴더 고르기 = 파일 필터가 뜻이 없다(목록에 폴더만) → "폴더" 한 줄.
        let filters = if mode == PickerMode::Folder {
            vec![nexa_dlg::FileFilter::new(t(Msg::FilterFolders), &[])]
        } else if let Some(f) = self.next_filters.take() {
            f
        } else {
            sql_filters()
        };
        let mut picker = FilePicker::new(mode, start, filters, labels());
        // 용도가 파일 하나만 뜻하면(프로젝트 파일 · 실행 파일 · 로그 내보내기) 다중 선택 없음(사용자 09-22).
        picker.set_multi(!single);
        // 덮어쓰기 = 타임아웃 버튼(두 번 눌러야 저장 · 사용자 09-19) — 시간은 설정 `file.overwrite_confirm_ms`(0 = 한 번에).
        picker.set_overwrite_confirm_ms(self.overwrite_confirm_ms);
        picker.set_default_name(default_name);
        picker.set_recent(recent);
        picker.set_pinned(pinned);
        picker.set_show_hidden(show_hidden);
        picker.set_show_dot(show_dot);
        // 하단 인코딩 콤보(Golden/DBeaver 하단 줄 · 세 OS 동일 · 사용자 09-15) — 열기 = 자동 감지 기본 · 저장 = 탭 인코딩.
        // 목록 = `enc::LIST`(상태줄 팝업과 같은 한 곳 · 09-16) · 열기는 자동 감지가 먼저.
        let mut items: Vec<(&str, String)> = Vec::new();
        if mode == PickerMode::Open {
            items.push(("auto", t(Msg::EncAuto).to_string()));
        }
        items.extend(
            crate::enc::LIST
                .iter()
                .map(|e| (e.id, t(e.label).to_string())),
        );
        let refs: Vec<(&str, &str)> = items.iter().map(|(v, l)| (*v, l.as_str())).collect();
        let sel = refs.iter().position(|(v, _)| *v == encoding).unwrap_or(0);
        // 인코딩 콤보는 파일을 열고 저장할 때만(폴더 고르기에는 없다).
        if mode != PickerMode::Folder {
            picker.set_extra(t(Msg::LblEncoding), &refs, sel);
        }
        self.picker = Some(picker);
        let (lw, lh) = (900.0, 580.0);
        let title = match mode {
            PickerMode::Open => t(Msg::WinOpenFile),
            PickerMode::Save => t(Msg::WinSaveFile),
            PickerMode::Folder => t(Msg::WinSelectFolder),
        };
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL - {title}"))
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
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        // 앱 조합 모드(T-139)면 IME를 붙이지 않는다 — raw 자모를 받아 상자가 직접 조합한다.
        win.set_ime_allowed(crate::input::system_ime());
        self.window = Some(win);
        self.layout();
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
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
            primary: self.primary,
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
                    primary: self.primary,
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
                    Key::Named(NamedKey::PageUp) => key(CtlKey::PageUp),
                    Key::Named(NamedKey::PageDown) => key(CtlKey::PageDown),
                    // Space: 주 수식키와 함께 = 선택 토글 키(다중 선택 · dir2 규약) · 그냥 = 이름 상자 글자.
                    Key::Named(NamedKey::Space) if self.primary => key(CtlKey::Space),
                    Key::Named(NamedKey::Space) => InputEvent::Char { c: ' ', now_ms: 0 },
                    Key::Character(t) => {
                        let c = t.chars().next()?;
                        if c.is_control() {
                            return None;
                        }
                        // 주 수식키 + 글자 = 단축키 — 글자로 넣지 않는다: Ctrl+A 전체 선택 · ★ Ctrl+Z / Ctrl+Y(또는
                        // Ctrl+Shift+Z) = 파일 작업 되돌리기/다시 실행(사용자 10-22 · nexa-dlg 스택).
                        if self.primary {
                            return match crate::input::shortcut_letter(kev) {
                                Some('a') => Some(InputEvent::SelectAll),
                                Some('z') if self.shift => Some(InputEvent::Redo),
                                Some('z') => Some(InputEvent::Undo),
                                Some('y') => Some(InputEvent::Redo),
                                _ => None,
                            };
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
                if nexa_ctl::draw::set_show_full(m.state().alt_key()) {
                    self.redraw();
                }
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
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
            PickerAction::ConfirmMany(paths) => {
                self.close();
                FileWinAction::ConfirmMany(paths, enc)
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
            let prefs = FontPrefs::with_base(font_px);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ 용도별 필터(사용자 10-23): 첫 줄 = 대상 확장자 · 전체 포함 · 다른 유형도 함께.
    #[test]
    fn filters_lead_with_target_and_include_all() {
        let f = filters_for(FilePurpose::DriverPackage, "log");
        assert_eq!(f[0].exts, ["zip"]);
        assert!(f.last().is_some_and(|x| x.exts.is_empty()));
        let f = filters_for(FilePurpose::Project, "log");
        assert_eq!(f[0].exts, ["nsql-project"]);
        assert!(f.iter().any(|x| x.exts.contains(&"sql".to_string())));
        let f = filters_for(FilePurpose::LogExport, "jsonl");
        assert_eq!(f[0].exts, ["jsonl"]);
        assert!(f[0].label.contains("jsonl"));
        let f = filters_for(FilePurpose::Editor, "log");
        assert_eq!(f[0].exts[0], "sql");
        assert!(f
            .iter()
            .any(|x| x.exts.contains(&"nsql-project".to_string())));
        assert!(filters_for(FilePurpose::CellValue, "log")[0]
            .exts
            .is_empty());
        assert_eq!(filters_for(FilePurpose::Import, "log")[0].exts[0], "csv");
        assert_eq!(
            filters_for(FilePurpose::License, "log")[0].exts,
            ["license"]
        );
        for p in [
            FilePurpose::Editor,
            FilePurpose::Import,
            FilePurpose::SqlPreview,
            FilePurpose::RunFile,
        ] {
            assert!(
                filters_for(p, "log").iter().any(|x| x.exts.is_empty()),
                "{p:?} 전체 없음"
            );
        }
    }
}
