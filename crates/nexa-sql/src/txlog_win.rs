//! **트랜잭션 로그 창**(docs/44 §2-4 · T-107 · 사용자 09-17) — DBeaver `Transaction log` 창처럼 **모덜리스** 별도 창.
//! 열 = 시간 · 종류 · 문장 · 소요 · 행 · 결과 · **Tx**(대기/커밋됨/롤백됨/암묵 커밋/자동/끊김) · 위 검색 상자 · 아래 스위치 3
//! (모든 질의 · 이전 트랜잭션 · 이 탭만). 행 색 = 상태의 보조(대기 accent · 롤백 회색+취소선 · 오류 danger · 중지 호박).
//!
//! 데이터는 호스트의 [`nsql_run::txlog::TxLog`]가 갖고, 창은 그릴 때 빌려 읽는다(복사 0 · 로그 창과 같은 창 골격).

use nexa_ctl::controls::ctxmenu::{ContextMenu, CtxItem};
use nexa_ctl::controls::{LabelSide, Switch};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{Color, FontPrefs, SlotFont, Theme};
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use nsql_run::txlog::{one_line, ExecOutcome, Purpose, TxFilter, TxLog, TxOutcome};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub(crate) enum TxLogAction {
    None,
    Paint,
    /// 우클릭 메뉴: 문장 복사(entry id).
    CopySql(u64),
    /// 우클릭 메뉴: 새 편집기 탭으로(entry id).
    OpenSql(u64),
}

pub(crate) struct TxLogWin {
    window: Option<Rc<Window>>,
    /// 창 기하 기억(`wingeom::Memo`): 기록 위치·크기(같은 모니터일 때만 씀) · 마지막 닫힌 (위치, 크기).
    memo: crate::wingeom::Memo,
    last: Option<((i32, i32), (f64, f64))>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    search: TextBox,
    sw_all: Switch,
    sw_prev: Switch,
    sw_tab: Switch,
    filter: TxFilter,
    /// 활성 편집기 탭(호스트가 그릴 때 준다 · "이 탭만" 필터).
    active_editor: u64,
    scroll: i32,
    row_h: i32,
    rows_seen: usize,
    hover: Option<usize>,
    /// 마지막 페인트의 표 영역·열 경계(히트 테스트).
    table: Rect,
    /// 접속·탭 설명(제목).
    title_ctx: String,
    /// 우클릭 메뉴 + 마지막 페인트의 행 → entry id.
    menu: ContextMenu,
    row_ids: Vec<u64>,
    menu_row: Option<u64>,
    /// "차단 중" 띠의 줄(호스트가 준다 · 비면 띠 없음).
    blocking: Vec<String>,
}

/// 열 폭(논리 px · 문장 열은 나머지 전부).
/// 마지막 "세션" 열은 기록에 세션이 둘 이상일 때만 보인다(docs/52 D-105 · 전 세션 합본).
const COLS: [(Msg, i32); 8] = [
    (Msg::TxColTime, 78),
    (Msg::TxColType, 78),
    (Msg::TxColText, 0),
    (Msg::TxColDuration, 66),
    (Msg::TxColRows, 56),
    (Msg::TxColResult, 120),
    (Msg::TxColTx, 118),
    (Msg::TxColSession, 170),
];

impl TxLogWin {
    pub(crate) fn new() -> Self {
        TxLogWin {
            window: None,
            memo: crate::wingeom::Memo::default(),

            last: None,
            surface: None,
            scale: 1.0,
            cursor: (-1, -1),
            shift: false,
            primary: false,
            search: TextBox::new(t(Msg::PhTxSearch)),
            sw_all: Switch::new(t(Msg::TxSwAll), false).with_label_side(LabelSide::Right),
            sw_prev: Switch::new(t(Msg::TxSwPrev), true).with_label_side(LabelSide::Right),
            sw_tab: Switch::new(t(Msg::TxSwThisTab), false).with_label_side(LabelSide::Right),
            filter: TxFilter {
                previous: true,
                ..TxFilter::default()
            },
            active_editor: 0,
            scroll: 0,
            row_h: 22,
            rows_seen: 0,
            hover: None,
            table: Rect::new(0, 0, 0, 0),
            title_ctx: String::new(),
            menu: ContextMenu::new(),
            row_ids: Vec::new(),
            menu_row: None,
            blocking: Vec::new(),
        }
    }

    /// 열기(모덜리스 · 메인 소유 창) — `near` = 메인 창 좌표·폭(오른쪽 아래에).
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        near: Option<(i32, i32, u32)>,
        owner: Option<&Window>,
        title_ctx: &str,
    ) {
        self.title_ctx = title_ctx.to_string();
        if let Some(w) = &self.window {
            w.set_title(&self.title());
            w.focus_window();
            self.redraw();
            return;
        }
        // 창 규칙(사용자 09-17): 같은 모니터면 기록 위치·크기 · 아니면 기본 크기로 메인 창 근처.
        let same = self.memo.on_same_monitor(owner);
        let (lw, lh) = same.and_then(|(_, s)| s).unwrap_or((960.0, 420.0));
        let mut attrs = Window::default_attributes()
            .with_title(self.title())
            .with_theme(theme)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
        if let Some(((x, y), _)) = same {
            attrs = attrs.with_position(crate::wingeom::logical(x, y));
        } else if let Some((x, y, w)) = near {
            attrs =
                attrs.with_position(winit::dpi::PhysicalPosition::new(x + w as i32 + 8, y + 360));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        // 기억한 위치는 프레임 기준으로 다시 놓는다(macOS 제목 표시줄 드리프트 방지 · wingeom::place_outer).
        if let Some(((x, y), _)) = same {
            crate::wingeom::place_outer(&win, Some((x, y)));
        }
        // 화면 밖으로 나가지 않게(메인 창 오른쪽 기본 위치 · 해상도가 바뀐 뒤의 기억 위치).
        crate::wingeom::keep_on_screen(&win, owner);
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        // ★ IME 허용 — 이 창에도 한글 입력란(검색·값)이 있다. winit 창은 기본으로 IME가 붙지 않아(Windows) 한글 조합이
        //   안 됐다(사용자 09-19 "설정 검색에 한글 입력이 안 된다" · 접속 창·파일 창은 이미 켜 둔 것과 같은 규칙).
        // 앱 조합 모드(T-139)면 IME를 붙이지 않는다 — raw 자모를 받아 상자가 직접 조합한다.
        win.set_ime_allowed(crate::input::system_ime());
        self.window = Some(win);
        self.search.set_focused(true);
        self.redraw();
    }

    fn title(&self) -> String {
        if self.title_ctx.is_empty() {
            format!("Nexa SQL — {}", t(Msg::WinTxLog))
        } else {
            format!("Nexa SQL — {} [{}]", t(Msg::WinTxLog), self.title_ctx)
        }
    }

    pub(crate) fn close(&mut self) {
        if let Some(w) = &self.window {
            if let Some(p) = crate::wingeom::outer_pos(w) {
                self.last = Some((p, crate::wingeom::logical_size(w)));
            }
        }
        self.surface = None;
        self.window = None;
        self.search.set_focused(false);
    }

    /// 기억된 기하(설정 `window.<name>_pos`/`_size`) — 열 때 같은 모니터면 그대로 쓴다.
    pub(crate) fn set_memo(&mut self, m: crate::wingeom::Memo) {
        self.memo = m;
    }

    /// 마지막으로 닫힌 (위치, 크기)(1회성 · 호스트가 설정에 저장).
    pub(crate) fn take_last(&mut self) -> Option<((i32, i32), (f64, f64))> {
        self.last.take()
    }

    /// **"차단 중" 띠**(docs/56 L3): 내 미커밋 때문에 기다리는 세션들 — 줄마다 "연결 — 기다리는 세션 설명". 비면 띠 없음.
    /// 바뀌었으면 true(호스트가 다시 그린다).
    pub(crate) fn set_blocking(&mut self, lines: Vec<String>) -> bool {
        if self.blocking == lines {
            return false;
        }
        self.blocking = lines;
        true
    }

    /// 창(호스트가 IME 허용을 바꿀 때 · T-139).
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

    /// 활성 편집기 탭(호스트가 탭 전환·실행 때 갱신).
    pub(crate) fn set_active_editor(&mut self, id: u64) {
        if self.active_editor != id {
            self.active_editor = id;
            self.redraw();
        }
    }

    fn sync_filter(&mut self) {
        self.filter.text = self.search.text();
        self.filter.all_purposes = self.sw_all.is_on();
        self.filter.previous = self.sw_prev.is_on();
        self.filter.editor = self.sw_tab.is_on().then_some(self.active_editor);
    }

    fn key_event(&self, kev: &winit::event::KeyEvent) -> Option<InputEvent> {
        let key = |k: CtlKey| InputEvent::Key {
            key: k,
            shift: self.shift,
            primary: self.primary,
        };
        Some(match kev.logical_key.as_ref() {
            Key::Named(NamedKey::ArrowLeft) => key(CtlKey::Left),
            Key::Named(NamedKey::ArrowRight) => key(CtlKey::Right),
            Key::Named(NamedKey::Home) => key(CtlKey::Home),
            Key::Named(NamedKey::End) => key(CtlKey::End),
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

    /// winit 사건 → 호스트 동작(Paint만 · 나머지는 창이 스스로).
    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> TxLogAction {
        let mut inv = Invalidations::default();
        // 우클릭 메뉴가 열려 있으면 모달(팝업 규칙: 항목 선택·Esc = 닫기 · 바깥 클릭 = 닫고 통과).
        if self.menu.is_open() {
            let (x, y) = self.cursor;
            let me = match ev {
                WindowEvent::CursorMoved { position, .. } => {
                    self.cursor = (position.x as i32, position.y as i32);
                    Some(InputEvent::MouseMove {
                        x: position.x as i32,
                        y: position.y as i32,
                    })
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => Some(InputEvent::MouseDown {
                    x,
                    y,
                    shift: false,
                    primary: false,
                }),
                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => Some(InputEvent::MouseUp { x, y }),
                WindowEvent::KeyboardInput { event: kev, .. }
                    if kev.state == ElementState::Pressed
                        && matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) =>
                {
                    Some(InputEvent::Key {
                        key: CtlKey::Escape,
                        shift: false,
                        primary: false,
                    })
                }
                WindowEvent::RedrawRequested => return TxLogAction::Paint,
                _ => None,
            };
            if let Some(e) = me {
                let consumed = self.menu.on_event(&e);
                self.redraw();
                if let Some(id) = self.menu.take_picked() {
                    let row = self.menu_row.take();
                    return match (id.as_str(), row) {
                        ("copy", Some(r)) => TxLogAction::CopySql(r),
                        ("open", Some(r)) => TxLogAction::OpenSql(r),
                        _ => TxLogAction::None,
                    };
                }
                if consumed {
                    return TxLogAction::None;
                }
            }
        }
        match ev {
            WindowEvent::CloseRequested => self.close(),
            WindowEvent::RedrawRequested => return TxLogAction::Paint,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                // 행 위 우클릭 = 문장 메뉴(복사 · 편집기로).
                if let Some(r) = self.hover {
                    if let Some(&id) = self.row_ids.get(r) {
                        self.menu_row = Some(id);
                        let (x, y) = self.cursor;
                        let host = self.table;
                        self.menu.set_scale(self.scale);
                        self.menu.open_at(
                            x,
                            y,
                            vec![
                                CtxItem::item("copy", t(Msg::MnTxCopySql)),
                                CtxItem::item("open", t(Msg::MnTxOpenSql)),
                            ],
                            host,
                            (220.0 * self.scale) as i32,
                        );
                        self.redraw();
                    }
                }
            }
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
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
            WindowEvent::MouseWheel { delta, .. } => {
                let e = crate::input::wheel_event(delta, self.shift);
                if let InputEvent::Wheel { delta: px } = e {
                    self.scroll = (self.scroll - px / 3).max(0);
                    self.redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                self.search.on_event(&mv, &mut inv);
                for sw in [&mut self.sw_all, &mut self.sw_prev, &mut self.sw_tab] {
                    sw.on_event(&mv, &mut inv);
                }
                let over = if self.table.contains(Point { x, y }) && self.row_h > 0 {
                    let r = ((y - self.table.y + self.scroll) / self.row_h) as usize;
                    (r < self.rows_seen).then_some(r)
                } else {
                    None
                };
                if over != self.hover {
                    self.hover = over;
                    self.redraw();
                }
                if !inv.is_empty() {
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
                if matches!(e, InputEvent::MouseDown { .. }) {
                    // 포커스 ≤ 1: 검색 상자는 클릭했을 때만.
                    self.search.set_focused(self.search.bounds().contains(p));
                }
                if up || self.search.bounds().contains(p) {
                    self.search.on_event(&e, &mut inv);
                }
                for sw in [&mut self.sw_all, &mut self.sw_prev, &mut self.sw_tab] {
                    if up || sw.bounds().contains(p) {
                        sw.on_event(&e, &mut inv);
                    }
                }
                let toggled = self.sw_all.take_toggled().is_some()
                    | self.sw_prev.take_toggled().is_some()
                    | self.sw_tab.take_toggled().is_some();
                if toggled {
                    self.scroll = 0;
                }
                self.sync_filter();
                self.redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.search.set_preedit("", &mut inv);
                for c in text.chars().filter(|c| !c.is_control()) {
                    self.search
                        .on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                }
                self.sync_filter();
                self.redraw();
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                self.search.set_preedit(text, &mut inv);
                self.redraw();
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                if matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) {
                    self.close();
                    return TxLogAction::None;
                }
                if let Some(e) = self.key_event(kev) {
                    self.search.on_event(&e, &mut inv);
                    self.scroll = 0;
                    self.sync_filter();
                    self.redraw();
                }
            }
            _ => {}
        }
        TxLogAction::None
    }

    /// 그리기 — 로그 데이터는 호스트 것을 빌린다.
    pub(crate) fn paint(&mut self, log: &TxLog, font: &Font, th: &Theme, ui_px: f32) {
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
            let pad = px(8.0);
            // 검색 상자(위)
            let sh = th_txt + px(10.0);
            self.search.set_scale(s);
            self.search.set_bounds(
                Rect::new(pad, pad, wi - pad * 2, sh),
                &mut Invalidations::default(),
            );
            self.search.paint(&mut dc, th);
            // 스위치(아래)
            let sw_h = th_txt + px(8.0);
            let sw_y = hi - pad - sw_h;
            let mut sx = pad;
            for sw in [&mut self.sw_all, &mut self.sw_prev, &mut self.sw_tab] {
                sw.set_scale(s);
                let w = px(230.0);
                sw.set_bounds(Rect::new(sx, sw_y, w, sw_h), &mut Invalidations::default());
                sw.paint(&mut dc, th);
                sx += w + pad;
            }
            // "차단 중" 띠(검색 상자와 표 사이 · 최대 3줄 + 나머지 개수) — 위험색 왼쪽 막대 + 옅은 바탕.
            let mut top = pad + sh + pad;
            if !self.blocking.is_empty() {
                let shown = self.blocking.len().min(3);
                let extra = self.blocking.len() - shown;
                let lines = shown + 1 + usize::from(extra > 0);
                let band = Rect::new(pad, top, wi - pad * 2, th_txt * lines as i32 + px(10.0));
                dc.fill_rect_alpha(band, th.danger, 0.12);
                dc.fill_rect(Rect::new(band.x, band.y, px(4.0), band.h), th.danger);
                let tx = band.x + px(12.0);
                let mut ty = band.y + px(5.0);
                dc.select_font(FontSlot::Base, true);
                dc.text(
                    tx,
                    ty,
                    band,
                    &tf(Msg::TxBandBlocking, &[&self.blocking.len().to_string()]),
                    th.danger,
                );
                dc.select_font(FontSlot::Base, false);
                ty += th_txt;
                for line in self.blocking.iter().take(shown) {
                    dc.text(tx, ty, band, line, th.text);
                    ty += th_txt;
                }
                if extra > 0 {
                    dc.text(
                        tx,
                        ty,
                        band,
                        &tf(Msg::TxBandMore, &[&extra.to_string()]),
                        th.text_dim,
                    );
                }
                top = band.bottom() + pad;
            }
            let table = Rect::new(pad, top, wi - pad * 2, (sw_y - pad - top).max(0));
            dc.fill_rect(table, th.field_bg);
            dc.fill_rect(Rect::new(table.x, table.y, table.w, 1), th.border);
            dc.fill_rect(
                Rect::new(table.x, table.bottom() - 1, table.w, 1),
                th.border,
            );
            dc.fill_rect(Rect::new(table.x, table.y, 1, table.h), th.border);
            dc.fill_rect(Rect::new(table.right() - 1, table.y, 1, table.h), th.border);
            self.row_h = th_txt + px(6.0);
            let row_h = self.row_h;
            // 열 폭(문장 열 = 나머지)
            let ncols = if log.multi_session() {
                COLS.len()
            } else {
                COLS.len() - 1
            };
            let cols = &COLS[..ncols];
            let fixed: i32 = cols.iter().map(|(_, w)| px(*w as f32)).sum();
            let text_w = (table.w - fixed - px(2.0)).max(px(120.0));
            let widths: Vec<i32> = cols
                .iter()
                .map(|(_, w)| if *w == 0 { text_w } else { px(*w as f32) })
                .collect();
            // 헤더
            let hdr = Rect::new(table.x + 1, table.y + 1, table.w - 2, row_h);
            dc.fill_rect(hdr, th.chrome_bg);
            let mut cx = hdr.x;
            for ((m, _), w) in cols.iter().zip(widths.iter()) {
                let cell = Rect::new(cx, hdr.y, *w, row_h);
                dc.text(
                    cx + px(6.0),
                    hdr.y + (row_h - th_txt) / 2,
                    cell,
                    t(*m),
                    th.text,
                );
                dc.fill_rect(Rect::new(cx + w - 1, hdr.y, 1, row_h), th.border);
                cx += w;
            }
            // 행
            let body = Rect::new(
                table.x + 1,
                hdr.bottom() + 1,
                table.w - 2,
                (table.bottom() - 1 - hdr.bottom() - 1).max(0),
            );
            self.table = body;
            let rows: Vec<&nsql_run::txlog::TxEntry> = log.entries(&self.filter).collect();
            self.row_ids = rows.iter().map(|e| e.id).collect();
            self.rows_seen = rows.len();
            let max_scroll = (rows.len() as i32 * row_h - body.h).max(0);
            self.scroll = self.scroll.clamp(0, max_scroll);
            let first = (self.scroll / row_h.max(1)) as usize;
            let mut y = body.y - (self.scroll % row_h.max(1));
            for (ri, e) in rows.iter().enumerate().skip(first) {
                if y >= body.bottom() {
                    break;
                }
                let r = Rect::new(body.x, y, body.w, row_h);
                let outcome = log.outcome_of(e);
                let (tint, alpha) = match (&e.result, &outcome) {
                    (ExecOutcome::Err { .. }, _) => (Some(th.danger), 0.15),
                    (ExecOutcome::Stopped { .. }, _) => (Some(Color(0x00E0_A020)), 0.14),
                    (_, TxOutcome::Pending) => (Some(th.accent), 0.12),
                    (
                        _,
                        TxOutcome::RolledBack
                        | TxOutcome::Lost
                        | TxOutcome::AutoRolledBack(_)
                        | TxOutcome::ReadEnded,
                    ) => (Some(th.text_dim), 0.10),
                    _ => (None, 0.0),
                };
                if ri % 2 == 1 {
                    dc.fill_rect(r, th.panel_bg_alt);
                }
                if let Some(c) = tint {
                    dc.fill_rect_alpha(r, c, alpha);
                }
                if self.hover == Some(ri) {
                    dc.fill_rect_alpha(r, th.text, 0.06);
                }
                let dim = matches!(
                    outcome,
                    TxOutcome::RolledBack | TxOutcome::Lost | TxOutcome::AutoRolledBack(_)
                );
                let fg = if dim { th.text_dim } else { th.text };
                let cells: [String; 8] = [
                    e.at.get(11..19).unwrap_or(&e.at).to_string(),
                    format!(
                        "SQL / {}",
                        t(match e.purpose {
                            Purpose::User => Msg::TxPurposeUser,
                            Purpose::Meta => Msg::TxPurposeMeta,
                            Purpose::Util => Msg::TxPurposeUtil,
                        })
                    ),
                    one_line(&e.text, 200),
                    e.duration
                        .map(|d| format!("{:.3}s", d.as_secs_f64()))
                        .unwrap_or_default(),
                    e.rows.map(|n| n.to_string()).unwrap_or_default(),
                    match &e.result {
                        ExecOutcome::Running => t(Msg::TxResRunning).to_string(),
                        ExecOutcome::Ok => t(Msg::TxResOk).to_string(),
                        ExecOutcome::Err { code, message } => match code {
                            Some(c) => format!("[{c}] {message}"),
                            None => message.clone(),
                        },
                        ExecOutcome::Stopped { rows } => {
                            tf(Msg::TxResStopped, &[&rows.to_string()])
                        }
                    },
                    match &outcome {
                        TxOutcome::Pending => t(Msg::TxOutPending).to_string(),
                        TxOutcome::Auto => t(Msg::TxOutAuto).to_string(),
                        TxOutcome::Committed => tf(Msg::TxOutCommitted, &[&ended(log, e)]),
                        TxOutcome::RolledBack => tf(Msg::TxOutRolledBack, &[&ended(log, e)]),
                        TxOutcome::ImplicitCommit(by) => tf(Msg::TxOutImplicit, &[by]),
                        TxOutcome::Switched => t(Msg::TxOutSwitched).to_string(),
                        TxOutcome::Lost => t(Msg::TxOutLost).to_string(),
                        TxOutcome::ReadEnded => t(Msg::TxOutReadEnded).to_string(),
                        TxOutcome::AutoRolledBack(m) => {
                            tf(Msg::TxOutAutoRolledBack, &[&m.to_string()])
                        }
                        TxOutcome::AutoCommitted(m) => {
                            tf(Msg::TxOutAutoCommitted, &[&m.to_string()])
                        }
                    },
                    log.session_label(e.session).to_string(),
                ];
                let mut cx = body.x;
                for (i, w) in widths.iter().enumerate() {
                    let cell = Rect::new(cx, y, *w, row_h);
                    let clip = Rect::new(
                        cell.x,
                        cell.y.max(body.y),
                        cell.w,
                        cell.h.min(body.bottom() - cell.y.max(body.y)),
                    );
                    let color = if i == 5 && matches!(e.result, ExecOutcome::Err { .. }) {
                        th.danger
                    } else if i == 6 && matches!(outcome, TxOutcome::Pending) {
                        th.accent
                    } else {
                        fg
                    };
                    let ty = y + (row_h - th_txt) / 2;
                    dc.text(cx + px(6.0), ty, clip, &cells[i], color);
                    if dim && i == 2 {
                        // 롤백된 문장 = 취소선.
                        let tw = dc.text_width(&cells[i]).min(w - px(12.0));
                        dc.fill_rect(Rect::new(cx + px(6.0), ty + th_txt / 2, tw, 1), th.text_dim);
                    }
                    cx += w;
                }
                y += row_h;
            }
            if rows.is_empty() {
                dc.text(
                    body.x + px(10.0),
                    body.y + px(10.0),
                    body,
                    t(Msg::TxEmpty),
                    th.text_dim,
                );
            }
            // 푸터 오른쪽: 건수 · 열린 트랜잭션
            let open = log.open_record();
            let info = match open {
                Some(r) => tf(
                    Msg::TxFooterOpen,
                    &[
                        &rows.len().to_string(),
                        &r.updates.to_string(),
                        r.started.get(11..16).unwrap_or(""),
                    ],
                ),
                None => tf(Msg::TxFooter, &[&rows.len().to_string()]),
            };
            let iw = dc.text_width(&info);
            dc.text(
                wi - pad - iw,
                sw_y + (sw_h - th_txt) / 2,
                Rect::new(0, sw_y, wi, sw_h),
                &info,
                th.text_dim,
            );
            self.menu.paint(&mut dc, th);
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}

fn ended(log: &TxLog, e: &nsql_run::txlog::TxEntry) -> String {
    e.tx.and_then(|id| log.record(id))
        .and_then(|r| r.ended.as_ref())
        .and_then(|s| s.get(11..16))
        .unwrap_or("")
        .to_string()
}
