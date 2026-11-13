//! **트랜잭션 로그 창**(docs/44 §2-4 · T-107 · 사용자 09-17) — DBeaver `Transaction log` 창처럼 **모덜리스** 별도 창.
//! 열 = 시간 · 종류 · 문장 · 소요 · 행 · 결과 · **Tx**(대기/커밋됨/롤백됨/암묵 커밋/자동/끊김) · 위 검색 상자 · 아래 스위치 3
//! (모든 질의 · 이전 트랜잭션 · 이 탭만). 행 색 = 상태의 보조(대기 accent · 롤백 회색+취소선 · 오류 danger · 중지 호박).
//!
//! 데이터는 호스트의 [`nsql_run::txlog::TxLog`]가 갖고, 창은 그릴 때 빌려 읽는다(복사 0 · 로그 창과 같은 창 골격).

use crate::listhead::{ListHead, Routed, SortVal};
use nexa_ctl::controls::ctxmenu::{ContextMenu, CtxItem};
use nexa_ctl::controls::{LabelSide, Switch};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{Color, FontPrefs, Theme};
use nexa_ctl::{Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use nsql_run::txlog::{one_line, ExecOutcome, Purpose, TxFilter, TxLog, TxOutcome};
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
    /// ★ 값 치환 쿼리(3계층 ③ · docs/44 §9) 복사 / 새 탭으로.
    CopyBoundSql(u64),
    OpenBoundSql(u64),
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
    /// ★ 상세 글 줄바꿈(맨 앞 스위치 · 사용자 11-13 "Wrap 옵션 · 하단 텍스트 Word-wrap" · 로그 창 푸터와 같은 모양).
    sw_wrap: Switch,
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
    /// ★ 선택한 문장(entry id) → 표 아래 **3계층 상세**(① 바인드 원문 ② 이름/타입/값 매핑 ③ 값 치환 쿼리 · 사용자 09-28 · docs/44 §9).
    sel: Option<u64>,
    /// 마지막 페인트의 상세 영역(사건 라우팅).
    detail: Rect,
    /// ★ 상세 = 읽기 전용 글상자(사용자 11-13 "드래그·더블클릭으로 선택 · 복사 · 우클릭"): 선택·낱말/줄 선택·우클릭 편집 메뉴는
    /// 상자가 스스로 한다 · 글은 선택 행(또는 그 글)이 바뀌었을 때만 갈아 끼운다(`detail_for` = id · 글 해시 — 매번 넣으면 선택이 날아간다).
    detail_tb: TextBox,
    detail_for: Option<(u64, u64)>,
    /// ★ 머리 줄 정렬·값 필터(공용 부품 · 11-13).
    head: ListHead,
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
    /// 언어 전환 뒤 창 제목 재라벨(열려 있을 때만 · 10-13).
    pub(crate) fn relabel(&mut self) {
        if let Some(w) = &self.window {
            w.set_title(&self.title());
        }
    }

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
            search: TextBox::new(t(Msg::PhTxSearch)).with_clearable(),
            sw_wrap: Switch::new(t(Msg::LblLogSwWrap), false).with_label_side(LabelSide::Right),
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
            sel: None,
            detail: Rect::new(0, 0, 0, 0),
            detail_tb: {
                let mut tb = TextBox::new("").with_multiline();
                tb.set_read_only(true);
                tb.set_wrap(false);
                tb.set_line_numbers(false);
                tb.set_gutter_marks(false);
                tb.set_minimap(false);
                tb.set_focus_ring(false);
                // 우클릭 편집 메뉴는 팝업 층에서(`paint_popup` · 맨 마지막에 그린다).
                tb.set_popup_deferred(true);
                tb
            },
            detail_for: None,
            head: ListHead::default(),
        }
    }

    /// 창 전체(팝업 배치의 host — 팝업은 창 밖으로 나가지 않는다).
    fn win_rect(&self) -> Rect {
        self.window
            .as_ref()
            .map(|w| {
                let sz = w.inner_size();
                Rect::new(0, 0, sz.width as i32, sz.height as i32)
            })
            .unwrap_or_default()
    }

    /// 상세 글상자의 선택(없으면 전체)을 클립보드로.
    fn copy_detail(&mut self) {
        let text = self
            .detail_tb
            .copy_selection()
            .unwrap_or_else(|| self.detail_tb.text());
        let _ = crate::clipboard::write_text(&text);
    }

    /// 상세 글상자의 우클릭 편집 메뉴가 남긴 요청(읽기 전용 = 복사만 · 전체 선택은 상자가 스스로).
    fn apply_detail_ctx(&mut self) {
        if let Some(nexa_ctl::EditCtxAction::Copy) = self.detail_tb.take_edit_ctx() {
            self.copy_detail();
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
            crate::winfocus::focus(w);
            self.redraw();
            return;
        }
        // 창 열기 꼬리 = 공통 호스트(T-247 · winhost · 검색·값 입력란이 있어 IME 허용 · 사용자 09-19).
        let Some(o) = crate::winhost::open_window(
            el,
            crate::winhost::OpenSpec {
                title: self.title(),
                theme,
                near,
                dy: 360,
                owner,
                memo: Some(&self.memo),
                default_size: (960.0, 420.0),
                ime: true,
            },
        ) else {
            return;
        };
        self.scale = o.scale;
        self.surface = o.surface;
        self.window = Some(o.window);
        self.search.set_focused(true);
        self.redraw();
    }

    fn title(&self) -> String {
        if self.title_ctx.is_empty() {
            format!("Nexa SQL - {}", t(Msg::WinTxLog))
        } else {
            format!("Nexa SQL - {} [{}]", t(Msg::WinTxLog), self.title_ctx)
        }
    }

    pub(crate) fn close(&mut self) {
        self.last = self.window.as_deref().and_then(crate::winhost::last_geom);
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

    /// winit 사건 → 호스트 동작(Paint만 · 나머지는 창이 스스로).
    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> TxLogAction {
        let mut inv = Invalidations::default();
        // ★ 머리 줄 값 목록 팝업이 먼저(11-13 · 바깥 클릭 = 닫고 통과).
        if self.head.is_open() && !matches!(ev, WindowEvent::RedrawRequested) {
            let was = self.head.is_open();
            let r = self
                .head
                .route(ev, &mut self.cursor, self.shift, self.primary);
            if was {
                self.scroll = 0;
                self.redraw();
            }
            if r == Routed::Consumed {
                return TxLogAction::None;
            }
        }
        // ★ 상세 글상자의 편집 메뉴가 열려 있으면 마우스·Esc는 그 메뉴에만(읽기 전용 글상자 관례 · sqlprev_win).
        if self.detail_tb.popup_open() {
            let (x, y) = self.cursor;
            let me = match ev {
                WindowEvent::CursorMoved { position, .. } => {
                    self.cursor = (position.x as i32, position.y as i32);
                    Some(InputEvent::MouseMove {
                        x: self.cursor.0,
                        y: self.cursor.1,
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
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Right,
                    ..
                } => Some(InputEvent::RightDown { x, y }),
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
                _ => None,
            };
            if let Some(e) = me {
                self.detail_tb.on_event(&e, &mut inv);
                self.apply_detail_ctx();
                self.redraw();
                return TxLogAction::None;
            }
        }
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
                // 바깥 클릭은 메뉴를 닫고 **그 클릭을 그대로 진행**한다(팝업 규칙 · 종전에는 삼켰다).
                let outside = self.menu.is_outside_click(&e);
                let consumed = self.menu.on_event(&e);
                self.redraw();
                if let Some(id) = self.menu.take_picked() {
                    let row = self.menu_row.take();
                    return match (id.as_str(), row) {
                        ("copy", Some(r)) => TxLogAction::CopySql(r),
                        ("open", Some(r)) => TxLogAction::OpenSql(r),
                        ("copy_bound", Some(r)) => TxLogAction::CopyBoundSql(r),
                        ("open_bound", Some(r)) => TxLogAction::OpenBoundSql(r),
                        _ => TxLogAction::None,
                    };
                }
                if consumed && !outside {
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
                // ★ 상세 영역 우클릭 = 글상자의 편집 메뉴(복사 · 전체 선택 · 11-13).
                let (x, y) = self.cursor;
                if self.detail.contains(Point { x, y }) {
                    self.search.set_focused(false);
                    self.detail_tb.set_focused(true);
                    self.detail_tb
                        .on_event(&InputEvent::RightDown { x, y }, &mut inv);
                    self.redraw();
                    return TxLogAction::None;
                }
                // 행 위 우클릭 = 문장 메뉴(복사 · 편집기로).
                if let Some(r) = self.hover {
                    if let Some(&id) = self.row_ids.get(r) {
                        self.menu_row = Some(id);
                        // 팝업 host = 창 전체(아래쪽 행에서도 메뉴가 창 안에서 뒤집힌다 · 종전 = 표 본문이라 잘렸다).
                        let host = self.win_rect();
                        self.menu.set_scale(self.scale);
                        crate::menu_icons::open_at(
                            &mut self.menu,
                            x,
                            y,
                            vec![
                                CtxItem::item("copy", t(Msg::MnTxCopySql)),
                                CtxItem::item("open", t(Msg::MnTxOpenSql)),
                                CtxItem::item("copy_bound", t(Msg::MnTxCopyBoundSql)),
                                CtxItem::item("open_bound", t(Msg::MnTxOpenBoundSql)),
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
                let (x, y) = self.cursor;
                if self.detail.contains(Point { x, y }) {
                    // 상세 글상자 안 스크롤(세로·가로).
                    self.detail_tb.on_event(&e, &mut inv);
                    self.redraw();
                } else if let InputEvent::Wheel { delta: px } = e {
                    self.scroll = (self.scroll - px / 3).max(0);
                    self.redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                self.search.on_event(&mv, &mut inv);
                // 상세 글상자 = 끌어서 선택(누른 채 상자 밖으로 나가도 이어지게 늘 준다).
                self.detail_tb.on_event(&mv, &mut inv);
                for sw in [
                    &mut self.sw_wrap,
                    &mut self.sw_all,
                    &mut self.sw_prev,
                    &mut self.sw_tab,
                ] {
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
                if matches!(e, InputEvent::MouseDown { .. }) && !self.search.popup_open() {
                    // 포커스 ≤ 1: 검색 상자·상세 글상자 가운데 클릭한 하나만(편집 메뉴 열림 중은 유지 · 10-13).
                    self.search.set_focused(self.search.bounds().contains(p));
                    self.detail_tb.set_focused(self.detail.contains(p));
                    // ★ 머리 줄 = 정렬(클릭 · Shift = 결합) · 깔때기 = 값 필터(11-13).
                    if self.head.click(p, self.shift) {
                        self.scroll = 0;
                        self.redraw();
                        return TxLogAction::None;
                    }
                    // 행 클릭 = 선택(3계층 상세) · 같은 행 다시 = 해제 · 빈 곳 = 해제.
                    if self.table.contains(p) {
                        let picked = self.hover.and_then(|r| self.row_ids.get(r).copied());
                        self.sel = if picked == self.sel { None } else { picked };
                    }
                }
                // 상세 글상자: 누름은 상자 안에서만 · 뗌은 늘(끌어 선택의 끝).
                if up || self.detail.contains(p) {
                    self.detail_tb.on_event(&e, &mut inv);
                    self.apply_detail_ctx();
                }
                // 편집 메뉴가 떠 있으면(상자 밖) 항목 클릭도 상자로(사용자 10-13 "입력창 우클릭 메뉴 선택이 안 됨").
                if up || self.search.bounds().contains(p) || self.search.popup_open() {
                    self.search.on_event(&e, &mut inv);
                }
                for sw in [
                    &mut self.sw_wrap,
                    &mut self.sw_all,
                    &mut self.sw_prev,
                    &mut self.sw_tab,
                ] {
                    if up || sw.bounds().contains(p) {
                        sw.on_event(&e, &mut inv);
                    }
                }
                // 줄바꿈 스위치 = 상세 글상자만 바꾼다(목록·필터와 무관).
                if let Some(on) = self.sw_wrap.take_toggled() {
                    self.detail_tb.set_wrap(on);
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
                // ★ 상세 글상자가 포커스면 Ctrl+C = 선택(없으면 전체) 복사 · Ctrl+A = 전체 선택 · 이동 키 = 상자(11-13).
                if self.detail_tb.is_focused() {
                    if self.primary {
                        match crate::input::shortcut_letter(kev) {
                            Some('c') => self.copy_detail(),
                            Some('a') => {
                                self.detail_tb.on_event(&InputEvent::SelectAll, &mut inv);
                            }
                            _ => {}
                        }
                    } else if let Some(e) = crate::input::text_key_event(
                        kev,
                        self.shift,
                        self.primary,
                        crate::input::TextKeys::Multi,
                    ) {
                        self.detail_tb.on_event(&e, &mut inv);
                    }
                    self.redraw();
                    return TxLogAction::None;
                }
                if let Some(e) = crate::input::text_key_event(
                    kev,
                    self.shift,
                    self.primary,
                    crate::input::TextKeys::Line,
                ) {
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
        let Some(mut buf) = surface.frame(size) else {
            self.surface = Some(surface);
            return;
        };
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let px = |v: f32| (v * s).round() as i32;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs::with_base_status(ui_px);
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
            // 스위치 = 글 폭에 맞춘 너비 + 사이 구분선(로그 창 푸터와 같은 모양 · 사용자 09-28 "좌우 여백 최소화").
            let mut sx = pad / 2;
            let track_w = px(34.0);
            for (i, (sw, msg)) in [
                (&mut self.sw_wrap, Msg::LblLogSwWrap),
                (&mut self.sw_all, Msg::TxSwAll),
                (&mut self.sw_prev, Msg::TxSwPrev),
                (&mut self.sw_tab, Msg::TxSwThisTab),
            ]
            .into_iter()
            .enumerate()
            {
                if i > 0 {
                    dc.fill_rect(
                        Rect::new(sx + pad / 2, sw_y + px(3.0), 1, sw_h - px(6.0)),
                        th.border,
                    );
                    sx += pad;
                }
                sw.set_scale(s);
                let w = track_w + dc.text_width(t(msg)) + px(6.0);
                sw.set_bounds(Rect::new(sx, sw_y, w, sw_h), &mut Invalidations::default());
                sw.paint(&mut dc, th);
                sx += w;
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
            // ★ 3계층 상세(선택 행이 있고 기록에 남아 있을 때): 남은 높이의 2/5(120~360 px) — 표는 그만큼 줄어든다.
            let sel_entry = self.sel.and_then(|id| log.entry(id).cloned());
            let detail_h = if sel_entry.is_some() {
                ((sw_y - pad - top) * 2 / 5).clamp(px(120.0), px(360.0))
            } else {
                0
            };
            let table = Rect::new(
                pad,
                top,
                wi - pad * 2,
                (sw_y - pad - top - if detail_h > 0 { detail_h + pad } else { 0 }).max(0),
            );
            self.detail = if detail_h > 0 {
                Rect::new(pad, table.bottom() + pad, wi - pad * 2, detail_h)
            } else {
                Rect::new(0, 0, 0, 0)
            };
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
            self.head.begin_paint();
            for (ci, ((m, _), w)) in cols.iter().zip(widths.iter()).enumerate() {
                let cell = Rect::new(cx, hdr.y, *w, row_h);
                // 정렬 삼각형 · 깔때기(오른쪽) — 머리 글은 그만큼 잘라 그린다.
                let used = self.head.paint_cell(&mut dc, th, ci, cell, s);
                dc.text(
                    cx + px(6.0),
                    hdr.y + (row_h - th_txt) / 2,
                    Rect::new(cx, hdr.y, (*w - used).max(0), row_h),
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
            let mut rows: Vec<&nsql_run::txlog::TxEntry> = log.entries(&self.filter).collect();
            // ★ 머리 줄 값 필터 · 정렬(11-13): 걸려 있을 때만 전체 행의 열 글을 만든다(평소 = 보이는 행만).
            let win_host = Rect::new(0, 0, wi, hi);
            if self.head.wants_values() {
                let all = &rows;
                self.head.open_pending(
                    |col| {
                        all.iter()
                            .map(|e| row_cells(log, e, &log.outcome_of(e))[col].clone())
                            .collect()
                    },
                    win_host,
                    s,
                    row_h,
                );
            }
            if self.head.has_filters() {
                rows.retain(|e| {
                    let c = row_cells(log, e, &log.outcome_of(e));
                    self.head.passes(|i| c[i].clone())
                });
            }
            if self.head.has_sort() {
                rows = self.head.sort_rows(rows, |e, col| match col {
                    // 시간·소요·행 수는 원값으로(글자로 비교하면 틀어진다).
                    0 => SortVal::text(&e.at),
                    3 => e
                        .duration
                        .map_or(SortVal::Empty, |d| SortVal::Num(d.as_secs_f64())),
                    4 => e.rows.map_or(SortVal::Empty, |n| SortVal::Num(n as f64)),
                    _ => SortVal::text(&row_cells(log, e, &log.outcome_of(e))[col]),
                });
            }
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
                if self.sel == Some(e.id) {
                    dc.fill_rect_alpha(r, th.sel_bg, 0.55);
                }
                if self.hover == Some(ri) {
                    dc.fill_rect_alpha(r, th.text, 0.06);
                }
                let dim = matches!(
                    outcome,
                    TxOutcome::RolledBack | TxOutcome::Lost | TxOutcome::AutoRolledBack(_)
                );
                let fg = if dim { th.text_dim } else { th.text };
                let cells = row_cells(log, e, &outcome);
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
            // ── 3계층 상세: ① 바인드 원문 ② 이름 · 타입 · 값 ③ 값 치환 쿼리 — 읽기 전용 글상자(선택·복사·우클릭 · 11-13).
            if let Some(e) = sel_entry.as_ref().filter(|_| detail_h > 0) {
                let d = self.detail;
                dc.fill_rect(d, th.field_bg);
                dc.fill_rect(Rect::new(d.x, d.y, d.w, 1), th.border);
                dc.fill_rect(Rect::new(d.x, d.bottom() - 1, d.w, 1), th.border);
                dc.fill_rect(Rect::new(d.x, d.y, 1, d.h), th.border);
                dc.fill_rect(Rect::new(d.right() - 1, d.y, 1, d.h), th.border);
                let text = detail_text(e);
                let key = {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    text.hash(&mut h);
                    (e.id, h.finish())
                };
                let inv = &mut Invalidations::default();
                if self.detail_for != Some(key) {
                    // 글이 바뀌었을 때만 갈아 끼운다(매번 넣으면 선택이 날아간다) · 캐럿 = 맨 위.
                    self.detail_tb.set_text(&text);
                    self.detail_tb.select_range(0, 0, inv);
                    self.detail_for = Some(key);
                }
                self.detail_tb.set_scale(s);
                self.detail_tb.set_bounds(
                    Rect::new(d.x + 1, d.y + 1, (d.w - 2).max(0), (d.h - 2).max(0)),
                    inv,
                );
                // 편집 메뉴는 창 전체 안에서 배치(상세가 창 아래쪽이라 위로 뒤집혀야 한다).
                self.detail_tb.set_menu_host(Some(win_host));
                self.detail_tb.paint(&mut dc, th);
            } else {
                self.detail_for = None;
                self.detail_tb.set_focused(false);
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
            // 팝업 = 맨 마지막 층(상세 편집 메뉴 · 머리 줄 값 목록 · 행 우클릭 메뉴).
            if self.detail_tb.popup_open() {
                self.detail_tb.paint_popup(&mut dc, th);
            }
            self.head.paint_popup(&mut dc, th);
            self.menu.paint(&mut dc, th);
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}

/// 목록 한 줄의 열 글(그리기 · 값 필터 · 글자 정렬이 같은 글을 쓴다).
fn row_cells(log: &TxLog, e: &nsql_run::txlog::TxEntry, outcome: &TxOutcome) -> [String; 8] {
    [
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
            ExecOutcome::Ok if !e.delivery.is_empty() => {
                format!("{} · {}", t(Msg::TxResOk), e.delivery)
            }
            ExecOutcome::Ok => t(Msg::TxResOk).to_string(),
            ExecOutcome::Err { code, message } => match code {
                Some(c) => format!("[{c}] {message}"),
                None => message.clone(),
            },
            ExecOutcome::Stopped { rows } => tf(Msg::TxResStopped, &[&rows.to_string()]),
        },
        match outcome {
            TxOutcome::Pending => t(Msg::TxOutPending).to_string(),
            TxOutcome::Auto => t(Msg::TxOutAuto).to_string(),
            TxOutcome::Committed => tf(Msg::TxOutCommitted, &[&ended(log, e)]),
            TxOutcome::RolledBack => tf(Msg::TxOutRolledBack, &[&ended(log, e)]),
            TxOutcome::ImplicitCommit(by) => tf(Msg::TxOutImplicit, &[by]),
            TxOutcome::Switched => t(Msg::TxOutSwitched).to_string(),
            TxOutcome::Lost => t(Msg::TxOutLost).to_string(),
            TxOutcome::ReadEnded => t(Msg::TxOutReadEnded).to_string(),
            TxOutcome::AutoRolledBack(m) => tf(Msg::TxOutAutoRolledBack, &[&m.to_string()]),
            TxOutcome::AutoCommitted(m) => tf(Msg::TxOutAutoCommitted, &[&m.to_string()]),
        },
        log.session_label(e.session).to_string(),
    ]
}

/// 3계층 상세 글(① 바인드 원문 ② 이름 · 타입 · 값 ③ 값 치환 쿼리 — 클라이언트 표시용 · 서버가 받은 문장이 아님).
fn detail_text(e: &nsql_run::txlog::TxEntry) -> String {
    let mut out: Vec<String> = Vec::new();
    out.push(format!("① {}", t(Msg::TxDetailSql)));
    out.extend(e.text.lines().map(str::to_string));
    out.push(format!(
        "② {}",
        tf(Msg::TxDetailBinds, &[&e.binds.len().to_string()])
    ));
    if e.binds.is_empty() {
        out.push(t(Msg::TxDetailNoBinds).to_string());
    } else {
        let nw = e
            .binds
            .iter()
            .map(|b| b.name.chars().count())
            .max()
            .unwrap_or(0)
            + 1;
        let tw = e
            .binds
            .iter()
            .map(|b| b.ty.chars().count())
            .max()
            .unwrap_or(0);
        for b in &e.binds {
            let dir = if b.out { "  (OUT)" } else { "" };
            out.push(format!(":{:<nw$} {:<tw$}  {}{dir}", b.name, b.ty, b.value));
        }
    }
    out.push(format!("③ {}", t(Msg::TxDetailBound)));
    out.push(t(Msg::TxDetailBoundNote).to_string());
    out.extend(e.bound_sql().lines().map(str::to_string));
    out.join("\n")
}

fn ended(log: &TxLog, e: &nsql_run::txlog::TxEntry) -> String {
    e.tx.and_then(|id| log.record(id))
        .and_then(|r| r.ended.as_ref())
        .and_then(|s| s.get(11..16))
        .unwrap_or("")
        .to_string()
}
