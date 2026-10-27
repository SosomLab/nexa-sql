//! ★ **드라이버 관리자 창**(docs/22 §5-2 · T-29 2차 · 사용자 10-23 "GUI에서는 드라이버 어떻게 확인") — DBeaver Driver Manager 대응.
//! 설치된 확장 드라이버와 색인(`drivers/index.json`)의 드라이버를 한 표에 보여 주고 [새로 고침][설치/갱신][삭제][확인]을 제공한다.
//!
//! - 표: id · 이름 · 방언 · **내장**(이 빌드에 컴파일된 버전 · 사용자 10-23) · 설치본 · 최신 · 상태(설치됨 · 갱신 있음 · 미설치 · 이 OS 빌드 없음 ·
//!   내장이면 "내장 x ≠ 확장 y"처럼 내장/확장 버전 차이). 줄을 고르면 버튼이 맞춰 켜진다(버튼은 확장 설치 상태 기준 · 내장은 접속에 먼저 쓰인다).
//! - 데이터는 호스트가 갖고([`crate::app::drivers`] · 색인 + `ext::list_extensions_in`) 창은 그릴 때 받은 줄만 본다(변수 창과 같은 꼴).
//! - 네트워크는 **버튼을 눌렀을 때만**(26 §8): 창을 열 때는 캐시된 색인만 읽고, [새로 고침]이 조건부 GET · [설치]가 자산 내려받기.
//!   오래 걸리는 일은 워커 스레드(`DriverTask`)가 하고 진행은 상태줄에 — 창·메인 창 입력을 막지 않는다.
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, Widget};
use nexa_gfx::{Font, Surface};
use nsql_drivers::ext::install::Index;
use nsql_drivers::ext::ExtInfo;
use nsql_i18n::{t, tf, Msg};
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 줄의 상태(순수 판정).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DriverState {
    Installed,
    UpdateAvailable,
    NotInstalled,
    /// 색인에 있지만 이 OS·아키텍처 빌드가 없다.
    NoBuild,
}

impl DriverState {
    fn label(self) -> Msg {
        match self {
            DriverState::Installed => Msg::DrvStateInstalled,
            DriverState::UpdateAvailable => Msg::DrvStateUpdate,
            DriverState::NotInstalled => Msg::DrvStateNotInstalled,
            DriverState::NoBuild => Msg::DrvStateNoBuild,
        }
    }
}

impl DriverRow {
    /// 상태 칸 글(순수): 내장이면 내장/확장 버전 차이(사용자 10-23 "내장 여부와 차이값") · 아니면 확장 상태.
    pub(crate) fn state_text(&self) -> String {
        match (&self.builtin, &self.installed, &self.latest) {
            (Some(b), Some(i), _) if b == i => tf(Msg::DrvStateBuiltinExtSame, &[b]),
            (Some(b), Some(i), _) => tf(Msg::DrvStateBuiltinExtDiff, &[b, i]),
            (Some(b), None, Some(l)) if self.state != DriverState::NoBuild => {
                tf(Msg::DrvStateBuiltinLatest, &[b, l])
            }
            (Some(b), None, _) => tf(Msg::DrvStateBuiltin, &[b]),
            (None, _, _) => t(self.state.label()).to_string(),
        }
    }
}

/// 표의 한 줄(호스트가 만든다 · [`build_rows`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DriverRow {
    pub id: String,
    pub name: String,
    pub dialect: String,
    /// 이 빌드에 내장된 버전(`nsql_drivers::builtin_version`) — 있으면 접속은 이것을 먼저 쓴다.
    pub builtin: Option<String>,
    pub installed: Option<String>,
    pub latest: Option<String>,
    pub state: DriverState,
    /// 설치본 라이브러리 경로(상태줄 안내용).
    pub path: Option<String>,
}

/// 색인 + 설치본 → 줄(순수): 색인 순서대로 · 색인에 없는 설치본은 뒤에 · 상태 = 설치본 vs 최신(`version_cmp`).
pub(crate) fn build_rows(
    index: Option<&Index>,
    installed: &[ExtInfo],
    plat: &str,
    builtin: impl Fn(&str) -> Option<&'static str>,
) -> Vec<DriverRow> {
    let mut rows: Vec<DriverRow> = Vec::new();
    if let Some(idx) = index {
        for d in &idx.drivers {
            let inst = installed.iter().find(|e| e.id == d.id);
            let plat_ok = d.platforms.is_empty() || d.platforms.iter().any(|p| p == plat);
            let latest = d.latest.clone().filter(|l| !l.is_empty());
            let state = match (inst, &latest) {
                (Some(e), Some(l))
                    if nsql_drivers::ext::version_cmp(l, &e.version)
                        == std::cmp::Ordering::Greater =>
                {
                    DriverState::UpdateAvailable
                }
                (Some(_), _) => DriverState::Installed,
                (None, _) if !plat_ok => DriverState::NoBuild,
                (None, _) => DriverState::NotInstalled,
            };
            rows.push(DriverRow {
                id: d.id.clone(),
                name: d.name.clone(),
                dialect: d.dialect.clone(),
                builtin: builtin(&d.id).map(str::to_string),
                installed: inst.map(|e| e.version.clone()),
                latest,
                state,
                path: inst.map(|e| e.path.display().to_string()),
            });
        }
    }
    for e in installed {
        if rows.iter().any(|r| r.id == e.id) {
            continue;
        }
        rows.push(DriverRow {
            id: e.id.clone(),
            name: e.id.clone(),
            dialect: e.dialect.clone().unwrap_or_else(|| e.id.clone()),
            builtin: builtin(&e.id).map(str::to_string),
            installed: Some(e.version.clone()),
            latest: None,
            state: DriverState::Installed,
            path: Some(e.path.display().to_string()),
        });
    }
    rows
}

pub(crate) enum DriverWinAction {
    None,
    Paint,
    /// 색인 다시 받기(조건부 GET).
    Refresh,
    /// 설치(id들 · 버전 = 색인 최신 · 갱신도 같은 길 = 옆에 설치 · 여러 개 = 호스트가 차례로 · 사용자 10-27 다중 선택).
    Install(Vec<String>),
    /// 설치본 전부 삭제(id들).
    Remove(Vec<String>),
    /// 실제로 올려 함수표 확인(id들).
    Check(Vec<String>),
    /// ★ 파일에서 설치(패키지 zip 또는 오프라인 번들 · 호스트가 파일 대화상자를 연다 · D-350).
    InstallFile,
}

/// 키보드 이동(파일 그리드 규약 · [`Sel::key_move`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelMove {
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

/// ★ 표의 다중 선택(순수 · 사용자 10-27 "파일 그리드처럼" = nexa-dlg 파일 선택기와 같은 규약): 클릭 = 단일 · Ctrl+클릭 = 토글 ·
/// Shift+클릭 = 기준 행~행 범위 · 방향키/Home/End/PageUp/PageDown = 캐럿 이동(단일) · Shift+이동 = 범위 · Ctrl+이동 = 캐럿만(선택 유지) ·
/// Space = 캐럿 행 토글 · Ctrl+A = 전부. `marks`는 고른 순서의 행 번호(id 대응은 창이 한다).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Sel {
    pub marks: Vec<usize>,
    /// 범위 선택의 기준 행.
    pub anchor: Option<usize>,
    /// 키보드 캐럿(포커스 행).
    pub cursor: Option<usize>,
}

impl Sel {
    pub(crate) fn contains(&self, row: usize) -> bool {
        self.marks.contains(&row)
    }

    fn single(&mut self, row: usize) {
        self.marks = vec![row];
        self.anchor = Some(row);
        self.cursor = Some(row);
    }

    fn toggle(&mut self, row: usize) {
        if let Some(i) = self.marks.iter().position(|m| *m == row) {
            self.marks.remove(i);
        } else {
            self.marks.push(row);
        }
        self.anchor = Some(row);
        self.cursor = Some(row);
    }

    fn range_to(&mut self, row: usize) {
        let a = self.anchor.unwrap_or(row);
        let (lo, hi) = (a.min(row), a.max(row));
        self.marks = (lo..=hi).collect();
        if self.anchor.is_none() {
            self.anchor = Some(row);
        }
        self.cursor = Some(row);
    }

    /// 마우스 클릭(수식키별 · 조건 두 개 = 독립 영향 쌍 시험).
    pub(crate) fn click(&mut self, row: usize, shift: bool, primary: bool) {
        if shift {
            self.range_to(row);
        } else if primary {
            self.toggle(row);
        } else {
            self.single(row);
        }
    }

    /// 키보드 이동 → 새 캐럿 행(순수). `page` = 한 화면 행 수(≥ 1).
    pub(crate) fn target(&self, mv: SelMove, n: usize, page: usize) -> Option<usize> {
        if n == 0 {
            return None;
        }
        let last = n - 1;
        let page = page.max(1);
        Some(match (self.cursor, mv) {
            (_, SelMove::Home) => 0,
            (_, SelMove::End) => last,
            (None, SelMove::Up | SelMove::PageUp) => last,
            (None, SelMove::Down | SelMove::PageDown) => 0,
            (Some(c), SelMove::Up) => c.saturating_sub(1),
            (Some(c), SelMove::Down) => (c + 1).min(last),
            (Some(c), SelMove::PageUp) => c.saturating_sub(page),
            (Some(c), SelMove::PageDown) => (c + page).min(last),
        })
    }

    /// 키보드 이동 적용: 기본 = 단일 선택 · Shift = 범위 · Ctrl = 캐럿만(선택 유지 · 탐색기 규약). 캐럿이 옮겨졌으면 그 행.
    pub(crate) fn key_move(
        &mut self,
        mv: SelMove,
        n: usize,
        page: usize,
        shift: bool,
        primary: bool,
    ) -> Option<usize> {
        let row = self.target(mv, n, page)?;
        if shift {
            self.range_to(row);
        } else if primary {
            self.cursor = Some(row);
        } else {
            self.single(row);
        }
        Some(row)
    }

    /// Space = 캐럿 행 토글(캐럿이 없으면 무시).
    pub(crate) fn toggle_cursor(&mut self) {
        if let Some(c) = self.cursor {
            self.toggle(c);
        }
    }

    pub(crate) fn all(&mut self, n: usize) {
        self.marks = (0..n).collect();
        if self.cursor.is_none() && n > 0 {
            self.cursor = Some(0);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.marks.clear();
        self.anchor = None;
        self.cursor = None;
    }
}

const COLS: [(Msg, i32); 7] = [
    (Msg::DrvColId, 96),
    (Msg::DrvColName, 0),
    (Msg::DrvColDialect, 76),
    (Msg::DrvColBuiltin, 64),
    (Msg::DrvColInstalled, 72),
    (Msg::DrvColLatest, 64),
    (Msg::DrvColState, 200),
];

pub(crate) struct DriverWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    refresh_btn: Button,
    install_btn: Button,
    remove_btn: Button,
    check_btn: Button,
    file_btn: Button,
    rows: Vec<DriverRow>,
    /// 다중 선택(행 번호 · 줄이 바뀌면 id로 다시 맞춘다 · [`Sel`]).
    sel: Sel,
    hover: Option<usize>,
    scroll: i32,
    row_h: i32,
    table: Rect,
    /// 상태줄(호스트가 넣는다 · 진행 · 결과 · 오류).
    status: String,
    /// 작업 중(버튼 잠금).
    busy: bool,
}

impl DriverWin {
    pub(crate) fn new() -> Self {
        DriverWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            refresh_btn: Button::new(t(Msg::BtnDrvRefresh)),
            install_btn: Button::new(t(Msg::BtnDrvInstall)),
            remove_btn: Button::new(t(Msg::BtnDrvRemove)),
            check_btn: Button::new(t(Msg::BtnDrvCheck)),
            file_btn: Button::new(t(Msg::BtnDrvInstallFile)),
            rows: Vec::new(),
            sel: Sel::default(),
            hover: None,
            scroll: 0,
            row_h: 0,
            table: Rect::default(),
            status: String::new(),
            busy: false,
        }
    }

    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        near: Option<(i32, i32, u32)>,
        owner: Option<&Window>,
    ) {
        if let Some(w) = &self.window {
            crate::winfocus::focus(w);
            self.redraw();
            return;
        }
        let Some(o) = crate::winhost::open_window(
            el,
            crate::winhost::OpenSpec {
                title: format!("Nexa SQL - {}", t(Msg::MnDrivers)),
                theme,
                near,
                dy: 160,
                owner,
                memo: None,
                default_size: (860.0, 400.0), // 이름 열이 "Oracle (OCI, Instant Client)"까지 보이게(10-23)
                ime: false,
            },
        ) else {
            return;
        };
        self.scale = o.scale;
        self.surface = o.surface;
        self.window = Some(o.window);
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
        for b in self.buttons() {
            b.clear_transient();
        }
    }

    pub(crate) fn window(&self) -> Option<&Window> {
        self.window.as_deref()
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// 상태줄 글 + 작업 중 여부(호스트).
    pub(crate) fn set_status(&mut self, text: &str, busy: bool) {
        if self.status != text || self.busy != busy {
            self.status = text.to_string();
            self.busy = busy;
            self.redraw();
        }
    }

    fn buttons(&mut self) -> [&mut Button; 5] {
        [
            &mut self.refresh_btn,
            &mut self.install_btn,
            &mut self.remove_btn,
            &mut self.check_btn,
            &mut self.file_btn,
        ]
    }

    /// 고른 줄들(고른 순서).
    fn marked_rows(&self) -> Vec<&DriverRow> {
        self.sel
            .marks
            .iter()
            .filter_map(|&i| self.rows.get(i))
            .collect()
    }

    /// 한 화면의 행 수(PageUp/Down 단위).
    fn page_rows(&self) -> usize {
        if self.row_h > 0 {
            ((self.table.h / self.row_h) as usize).max(1)
        } else {
            1
        }
    }

    /// 캐럿 행이 보이게 스크롤(키보드 이동 뒤).
    fn scroll_to_row(&mut self, row: usize) {
        if self.row_h <= 0 || self.table.h <= 0 {
            return;
        }
        let top = row as i32 * self.row_h;
        let bottom = top + self.row_h;
        if top < self.scroll {
            self.scroll = top;
        } else if bottom > self.scroll + self.table.h {
            self.scroll = bottom - self.table.h;
        }
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> DriverWinAction {
        let mut inv = Invalidations::default();
        match ev {
            WindowEvent::CloseRequested => self.close(),
            WindowEvent::RedrawRequested => return DriverWinAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                for b in self.buttons() {
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
                for b in self.buttons() {
                    b.on_event(&mv, &mut inv);
                }
                let over = if self.table.contains(Point { x, y }) && self.row_h > 0 {
                    let r = ((y - self.table.y + self.scroll) / self.row_h) as usize;
                    (r < self.rows.len()).then_some(r)
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
                if !up && self.table.contains(p) {
                    // ★ 파일 그리드 규약(사용자 10-27): 클릭 = 단일 · Ctrl = 토글 · Shift = 범위 · 빈 곳 = 선택 해제.
                    match self.hover.filter(|i| *i < self.rows.len()) {
                        Some(i) => self.sel.click(i, self.shift, self.primary),
                        None => self.sel.clear(),
                    }
                }
                for b in self.buttons() {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                }
                self.redraw();
                return self.button_action();
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                // ★ 키보드 선택 = 파일 그리드와 같게(사용자 10-27): 이동 = 단일 · Shift+이동 = 범위 · Ctrl+이동 = 캐럿만 · Space = 토글 · Ctrl+A = 전부.
                let mv = match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::ArrowDown) => Some(SelMove::Down),
                    Key::Named(NamedKey::ArrowUp) => Some(SelMove::Up),
                    Key::Named(NamedKey::Home) => Some(SelMove::Home),
                    Key::Named(NamedKey::End) => Some(SelMove::End),
                    Key::Named(NamedKey::PageUp) => Some(SelMove::PageUp),
                    Key::Named(NamedKey::PageDown) => Some(SelMove::PageDown),
                    _ => None,
                };
                if let Some(mv) = mv {
                    let (n, page) = (self.rows.len(), self.page_rows());
                    if let Some(row) = self.sel.key_move(mv, n, page, self.shift, self.primary) {
                        self.scroll_to_row(row);
                    }
                    self.redraw();
                    return DriverWinAction::None;
                }
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => self.close(),
                    Key::Named(NamedKey::F5) => return DriverWinAction::Refresh,
                    Key::Named(NamedKey::Space) => {
                        self.sel.toggle_cursor();
                        self.redraw();
                    }
                    Key::Character(c) if self.primary && c.eq_ignore_ascii_case("a") => {
                        self.sel.all(self.rows.len());
                        self.redraw();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        DriverWinAction::None
    }

    fn button_action(&mut self) -> DriverWinAction {
        if self.refresh_btn.take_clicked() {
            return DriverWinAction::Refresh;
        }
        let marked: Vec<DriverRow> = self.marked_rows().into_iter().cloned().collect();
        if self.install_btn.take_clicked() {
            let ids = Self::installable_ids(&marked);
            if !ids.is_empty() {
                return DriverWinAction::Install(ids);
            }
        }
        if self.remove_btn.take_clicked() {
            let ids = Self::installed_ids(&marked);
            if !ids.is_empty() {
                return DriverWinAction::Remove(ids);
            }
        }
        if self.check_btn.take_clicked() {
            let ids = Self::installed_ids(&marked);
            if !ids.is_empty() {
                return DriverWinAction::Check(ids);
            }
        }
        if self.file_btn.take_clicked() {
            return DriverWinAction::InstallFile;
        }
        DriverWinAction::None
    }

    /// 고른 줄 중 설치/갱신할 수 있는 것(순수 · 고른 순서).
    fn installable_ids(marked: &[DriverRow]) -> Vec<String> {
        marked
            .iter()
            .filter(|r| {
                matches!(
                    r.state,
                    DriverState::NotInstalled | DriverState::UpdateAvailable
                )
            })
            .map(|r| r.id.clone())
            .collect()
    }

    /// 고른 줄 중 설치본이 있는 것(삭제·확인 대상 · 순수).
    fn installed_ids(marked: &[DriverRow]) -> Vec<String> {
        marked
            .iter()
            .filter(|r| r.installed.is_some())
            .map(|r| r.id.clone())
            .collect()
    }

    /// 버튼 활성 = 고른 줄들의 상태(순수 · 하나라도 해당하면 켠다 · 시험용 분리).
    fn button_states(marked: &[DriverRow], busy: bool) -> (bool, bool, bool, bool) {
        let installed = !Self::installed_ids(marked).is_empty();
        let installable = !Self::installable_ids(marked).is_empty();
        (
            !busy,
            !busy && installable,
            !busy && installed,
            !busy && installed,
        )
    }

    /// 설치 버튼 글(순수): 설치/갱신 대상이 전부 "갱신 있음"이면 [갱신] · 아니면 [설치] · 둘 이상이면 "(n)".
    fn install_label(marked: &[DriverRow]) -> String {
        let ids = Self::installable_ids(marked);
        let all_update = !ids.is_empty()
            && marked
                .iter()
                .filter(|r| ids.contains(&r.id))
                .all(|r| r.state == DriverState::UpdateAvailable);
        let base = t(if all_update {
            Msg::BtnDrvUpdate
        } else {
            Msg::BtnDrvInstall
        });
        Self::count_label(base, ids.len())
    }

    /// 버튼 글에 개수 꼬리(둘 이상일 때만 · 순수).
    fn count_label(base: &str, n: usize) -> String {
        if n > 1 {
            let n = n.to_string();
            tf(Msg::BtnCount, &[base, n.as_str()])
        } else {
            base.to_string()
        }
    }

    pub(crate) fn paint(&mut self, rows_in: &[DriverRow], font: &Font, th: &Theme, ui_px: f32) {
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
        if self.rows != rows_in {
            // 줄이 바뀌면(새로 고침·설치 뒤) 고른 id를 새 번호로 다시 맞춘다(없어진 것은 버림 · 기준·캐럿도 같이).
            let remap = |i: usize| -> Option<usize> {
                let id = &self.rows.get(i)?.id;
                rows_in.iter().position(|r| &r.id == id)
            };
            let marks: Vec<usize> = self.sel.marks.iter().filter_map(|&i| remap(i)).collect();
            let anchor = self.sel.anchor.and_then(remap);
            let cursor = self.sel.cursor.and_then(remap);
            self.sel = Sel {
                marks,
                anchor,
                cursor,
            };
            self.rows = rows_in.to_vec();
        }
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
            let status_h = th_txt + px(6.0);
            let btn_h = th_txt + px(12.0);
            let foot = status_h + pad + btn_h;
            let table = Rect::new(pad, pad, wi - pad * 2, (hi - pad * 3 - foot).max(0));
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
            let fixed: i32 = COLS.iter().map(|(_, w)| px(*w as f32)).sum();
            let flex_w = (table.w - fixed - px(2.0)).max(px(100.0));
            let widths: Vec<i32> = COLS
                .iter()
                .map(|(_, w)| if *w == 0 { flex_w } else { px(*w as f32) })
                .collect();
            let hdr = Rect::new(table.x + 1, table.y + 1, table.w - 2, row_h);
            dc.fill_rect(hdr, th.chrome_bg);
            let mut cx = hdr.x;
            for ((m, _), w) in COLS.iter().zip(widths.iter()) {
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
            let body = Rect::new(
                table.x + 1,
                hdr.bottom() + 1,
                table.w - 2,
                (table.bottom() - 1 - hdr.bottom() - 1).max(0),
            );
            self.table = body;
            let max_scroll = (self.rows.len() as i32 * row_h - body.h).max(0);
            self.scroll = self.scroll.clamp(0, max_scroll);
            let first = (self.scroll / row_h.max(1)) as usize;
            let mut y = body.y - (self.scroll % row_h.max(1));
            if self.rows.is_empty() {
                dc.text(
                    body.x + px(8.0),
                    body.y + px(8.0),
                    body,
                    t(Msg::DrvEmpty),
                    th.text_dim,
                );
            }
            for (ri, r) in self.rows.iter().enumerate().skip(first) {
                if y >= body.bottom() {
                    break;
                }
                let top = y.max(body.y);
                let clip = Rect::new(body.x, top, body.w, (y + row_h).min(body.bottom()) - top);
                let selected = self.sel.contains(ri);
                if selected {
                    dc.fill_rect_alpha(clip, th.accent, 0.16);
                } else if self.hover == Some(ri) {
                    dc.fill_rect_alpha(clip, th.text, 0.06);
                }
                // 키보드 캐럿 행 = 가는 테두리(창의 포커스 링은 이것 하나 · claude-ui-rules).
                if self.sel.cursor == Some(ri) {
                    dc.fill_rect(Rect::new(clip.x, clip.y, clip.w, 1), th.accent);
                    dc.fill_rect(Rect::new(clip.x, clip.bottom() - 1, clip.w, 1), th.accent);
                }
                if r.state == DriverState::UpdateAvailable {
                    dc.fill_rect(Rect::new(clip.x, clip.y, px(3.0), clip.h), th.accent);
                }
                let ty = y + (row_h - th_txt) / 2;
                let cells = [
                    r.id.clone(),
                    r.name.clone(),
                    r.dialect.clone(),
                    r.builtin.clone().unwrap_or_else(|| "-".into()),
                    r.installed.clone().unwrap_or_else(|| "-".into()),
                    r.latest.clone().unwrap_or_else(|| "-".into()),
                    r.state_text(),
                ];
                let mut cx = body.x;
                for (i, w) in widths.iter().enumerate() {
                    let cell = Rect::new(cx, clip.y, *w - px(4.0), clip.h);
                    let color = if r.state == DriverState::NoBuild || !(i == 0 || i == 1) {
                        th.text_dim
                    } else {
                        th.text
                    };
                    dc.text(cx + px(6.0), ty, cell, &cells[i], color);
                    cx += w;
                }
                y += row_h;
            }
            // 상태줄 + 버튼.
            let inv = &mut Invalidations::default();
            let sy = table.bottom() + pad;
            let status =
                nexa_ctl::draw::ellipsize_middle(&mut dc, &self.status, wi - pad * 2 - px(4.0));
            dc.text(
                pad + px(2.0),
                sy + px(3.0),
                Rect::new(pad, sy, wi - pad * 2, status_h),
                &status,
                if self.busy { th.accent } else { th.text_dim },
            );
            let marked: Vec<DriverRow> = self.marked_rows().into_iter().cloned().collect();
            let (b_refresh, b_install, b_remove, b_check) = Self::button_states(&marked, self.busy);
            self.install_btn.set_label(Self::install_label(&marked));
            let n_inst = Self::installed_ids(&marked).len();
            self.remove_btn
                .set_label(Self::count_label(t(Msg::BtnDrvRemove), n_inst));
            self.check_btn
                .set_label(Self::count_label(t(Msg::BtnDrvCheck), n_inst));
            self.refresh_btn.set_enabled(b_refresh);
            self.install_btn.set_enabled(b_install);
            self.remove_btn.set_enabled(b_remove);
            self.check_btn.set_enabled(b_check);
            self.file_btn.set_enabled(!self.busy);
            let by = sy + status_h + pad;
            let bw = ((wi - pad * 2 - px(8.0) * 4) / 5).max(px(60.0));
            let mut bx = pad;
            for b in self.buttons() {
                b.set_scale(s);
                b.set_bounds(Rect::new(bx, by, bw, btn_h), inv);
                b.paint(&mut dc, th);
                bx += bw + px(8.0);
            }
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ext(id: &str, ver: &str) -> ExtInfo {
        ExtInfo {
            id: id.into(),
            version: ver.into(),
            path: PathBuf::from(format!("{id}.dll")),
            dialect: None,
            abi: Some(1),
            manifest: true,
        }
    }

    #[test]
    fn rows_merge_index_and_installed_with_states() {
        let idx = nsql_drivers::ext::install::parse_index(
            r#"{"drivers":[{"id":"a","name":"A","dialect":"a","latest":"2.0.0"},
                {"id":"b","latest":"1.0.0","platforms":["nowhere-arch"]},{"id":"c","latest":"1.0.0"},{"id":"d"}]}"#,
        )
        .expect("index");
        let rows = build_rows(
            Some(&idx),
            &[ext("a", "1.0.0"), ext("c", "1.0.0"), ext("zz", "0.1")],
            "windows-x86_64",
            |id| match id {
                "a" | "d" => Some("9.9.9"),
                "c" => Some("1.0.0"),
                _ => None,
            },
        );
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c", "d", "zz"]);
        assert_eq!(rows[0].state, DriverState::UpdateAvailable);
        assert_eq!(rows[1].state, DriverState::NoBuild);
        assert_eq!(rows[2].state, DriverState::Installed);
        assert_eq!(rows[3].state, DriverState::NotInstalled);
        assert_eq!(rows[4].state, DriverState::Installed);
        assert_eq!(rows[4].latest, None);
        assert_eq!(rows[4].name, "zz");
        // ★ 내장 열·상태 문구(사용자 10-23): 내장 ≠ 확장 · 내장 = 확장 · 내장 + 확장 최신 · 내장만 · 비내장은 확장 상태.
        assert_eq!(rows[0].builtin.as_deref(), Some("9.9.9"));
        assert_eq!(rows[0].state_text(), "built-in 9.9.9 ≠ extension 1.0.0");
        assert_eq!(rows[2].state_text(), "built-in = extension 1.0.0");
        assert_eq!(rows[3].state_text(), "built-in 9.9.9");
        assert_eq!(rows[1].state_text(), "no build for this OS");
        assert_eq!(rows[4].builtin, None);
        assert_eq!(rows[4].state_text(), "installed");
        let mut d_latest = rows[3].clone();
        d_latest.latest = Some("0.1.1".into());
        assert_eq!(
            d_latest.state_text(),
            "built-in 9.9.9 · extension 0.1.1 available"
        );
        // 색인 없음 = 설치본만.
        let only = build_rows(None, &[ext("q", "3")], "x", |_| None);
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].state, DriverState::Installed);
        // 버튼 활성: 미설치 = 설치만 · 설치됨 = 삭제·확인 · 갱신 = 셋 다 · 작업 중 = 전부 끔 · 여러 줄 = 하나라도 해당하면.
        let one = |i: usize| vec![rows[i].clone()];
        assert_eq!(
            DriverWin::button_states(&one(3), false),
            (true, true, false, false)
        );
        assert_eq!(
            DriverWin::button_states(&one(2), false),
            (true, false, true, true)
        );
        assert_eq!(
            DriverWin::button_states(&one(0), false),
            (true, true, true, true)
        );
        assert_eq!(
            DriverWin::button_states(&one(0), true),
            (false, false, false, false)
        );
        assert_eq!(
            DriverWin::button_states(&[], false),
            (true, false, false, false)
        );
        // ★ 다중 선택(사용자 10-27): 설치·삭제 대상은 고른 순서대로 해당 줄만 · 버튼 글 = 전부 갱신이면 [갱신] · 섞이면 [설치] · 둘 이상 "(n)".
        let many = vec![
            rows[2].clone(),
            rows[0].clone(),
            rows[3].clone(),
            rows[1].clone(),
        ];
        assert_eq!(DriverWin::installable_ids(&many), ["a", "d"]);
        assert_eq!(DriverWin::installed_ids(&many), ["c", "a"]);
        assert_eq!(
            DriverWin::button_states(&many, false),
            (true, true, true, true)
        );
        assert_eq!(DriverWin::install_label(&many), "Install (2)");
        assert_eq!(DriverWin::install_label(&one(0)), "Update");
        assert_eq!(DriverWin::install_label(&one(2)), "Install");
        assert_eq!(DriverWin::count_label("Remove", 1), "Remove");
        assert_eq!(DriverWin::count_label("Remove", 3), "Remove (3)");
    }

    /// ★ 다중 선택 모델(사용자 10-27 "파일 그리드처럼") — 클릭 수식키 · 키보드 이동 수식키의 독립 영향 쌍.
    #[test]
    fn sel_click_and_keys_follow_file_grid_rules() {
        let mut s = Sel::default();
        s.click(2, false, false);
        assert_eq!(
            (s.marks.clone(), s.anchor, s.cursor),
            (vec![2], Some(2), Some(2))
        );
        s.click(4, false, true); // Ctrl = 토글 추가 · 기준 이동
        assert_eq!(s.marks, [2, 4]);
        assert_eq!(s.anchor, Some(4));
        s.click(4, false, true); // 다시 Ctrl = 해제
        assert_eq!(s.marks, [2]);
        s.click(1, true, false); // Shift = 기준(4)~1 범위
        assert_eq!(s.marks, [1, 2, 3, 4]);
        assert_eq!(s.anchor, Some(4), "범위 선택은 기준을 옮기지 않는다");
        s.click(1, true, true); // Shift가 Ctrl보다 우선
        assert_eq!(s.marks, [1, 2, 3, 4]);
        s.click(0, false, false); // 보통 클릭 = 단일
        assert_eq!(s.marks, [0]);
        // 키보드: 이동 = 단일 · Shift+이동 = 범위 · Ctrl+이동 = 캐럿만.
        assert_eq!(s.key_move(SelMove::Down, 6, 2, false, false), Some(1));
        assert_eq!(s.marks, [1]);
        assert_eq!(s.key_move(SelMove::Down, 6, 2, true, false), Some(2));
        assert_eq!(s.marks, [1, 2]);
        assert_eq!(s.key_move(SelMove::PageDown, 6, 2, false, true), Some(4));
        assert_eq!(s.marks, [1, 2], "Ctrl+이동 = 선택 유지");
        assert_eq!(s.cursor, Some(4));
        s.toggle_cursor(); // Space
        assert_eq!(s.marks, [1, 2, 4]);
        assert_eq!(s.key_move(SelMove::End, 6, 2, false, false), Some(5));
        assert_eq!(
            s.key_move(SelMove::Down, 6, 2, false, false),
            Some(5),
            "끝에서 멈춘다(순환 없음)"
        );
        assert_eq!(s.key_move(SelMove::Home, 6, 2, false, false), Some(0));
        assert_eq!(s.key_move(SelMove::Up, 6, 2, false, false), Some(0));
        assert_eq!(s.key_move(SelMove::PageUp, 6, 9, false, false), Some(0));
        s.all(6);
        assert_eq!(s.marks, [0, 1, 2, 3, 4, 5]);
        // 빈 표 · 캐럿 없음.
        let mut e = Sel::default();
        assert_eq!(e.key_move(SelMove::Down, 0, 1, false, false), None);
        assert_eq!(
            e.key_move(SelMove::Up, 3, 1, false, false),
            Some(2),
            "캐럿 없을 때 ↑ = 끝"
        );
        e.clear();
        assert_eq!(
            e.key_move(SelMove::Down, 3, 1, false, false),
            Some(0),
            "캐럿 없을 때 ↓ = 처음"
        );
        e.toggle_cursor();
        assert!(e.marks.is_empty(), "Space = 캐럿 행 토글(0 → 해제)");
    }
}
