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
    /// 설치(id · 버전 = 색인 최신 · 갱신도 같은 길 = 옆에 설치).
    Install(String),
    /// 설치본 전부 삭제(id).
    Remove(String),
    /// 실제로 올려 함수표 확인(id).
    Check(String),
    /// ★ 파일에서 설치(패키지 zip 또는 오프라인 번들 · 호스트가 파일 대화상자를 연다 · D-350).
    InstallFile,
}

const COLS: [(Msg, i32); 7] = [
    (Msg::DrvColId, 110),
    (Msg::DrvColName, 0),
    (Msg::DrvColDialect, 80),
    (Msg::DrvColBuiltin, 70),
    (Msg::DrvColInstalled, 80),
    (Msg::DrvColLatest, 80),
    (Msg::DrvColState, 190),
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
    selected: Option<String>,
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
            selected: None,
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
            w.focus_window();
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
                default_size: (720.0, 380.0),
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

    fn selected_row(&self) -> Option<&DriverRow> {
        let id = self.selected.as_deref()?;
        self.rows.iter().find(|r| r.id == id)
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
                    self.selected = self
                        .hover
                        .and_then(|i| self.rows.get(i))
                        .map(|r| r.id.clone());
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
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => self.close(),
                    Key::Named(NamedKey::F5) => return DriverWinAction::Refresh,
                    Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp)
                        if !self.rows.is_empty() =>
                    {
                        let n = self.rows.len();
                        let cur = self
                            .selected
                            .as_deref()
                            .and_then(|s| self.rows.iter().position(|r| r.id == s));
                        let down =
                            matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::ArrowDown));
                        let next = match (cur, down) {
                            (None, true) => 0,
                            (None, false) => n - 1,
                            (Some(i), true) => (i + 1) % n,
                            (Some(i), false) => (i + n - 1) % n,
                        };
                        self.selected = self.rows.get(next).map(|r| r.id.clone());
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
        let sel = self.selected_row().cloned();
        if self.install_btn.take_clicked() {
            if let Some(r) = &sel {
                if matches!(
                    r.state,
                    DriverState::NotInstalled | DriverState::UpdateAvailable
                ) {
                    return DriverWinAction::Install(r.id.clone());
                }
            }
        }
        if self.remove_btn.take_clicked() {
            if let Some(r) = sel.as_ref().filter(|r| r.installed.is_some()) {
                return DriverWinAction::Remove(r.id.clone());
            }
        }
        if self.check_btn.take_clicked() {
            if let Some(r) = sel.as_ref().filter(|r| r.installed.is_some()) {
                return DriverWinAction::Check(r.id.clone());
            }
        }
        if self.file_btn.take_clicked() {
            return DriverWinAction::InstallFile;
        }
        DriverWinAction::None
    }

    /// 버튼 활성 = 고른 줄의 상태(순수에 가깝다 · 시험용 분리).
    fn button_states(sel: Option<&DriverRow>, busy: bool) -> (bool, bool, bool, bool) {
        let installed = sel.is_some_and(|r| r.installed.is_some());
        let installable = sel.is_some_and(|r| {
            matches!(
                r.state,
                DriverState::NotInstalled | DriverState::UpdateAvailable
            )
        });
        (
            !busy,
            !busy && installable,
            !busy && installed,
            !busy && installed,
        )
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
            self.rows = rows_in.to_vec();
            if self
                .selected
                .as_deref()
                .is_some_and(|s| !self.rows.iter().any(|r| r.id == s))
            {
                self.selected = None;
            }
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
                let selected = self.selected.as_deref() == Some(r.id.as_str());
                if selected {
                    dc.fill_rect_alpha(clip, th.accent, 0.16);
                } else if self.hover == Some(ri) {
                    dc.fill_rect_alpha(clip, th.text, 0.06);
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
            let sel = self.selected_row().cloned();
            let (b_refresh, b_install, b_remove, b_check) =
                Self::button_states(sel.as_ref(), self.busy);
            self.install_btn.set_label(
                if sel
                    .as_ref()
                    .is_some_and(|r| r.state == DriverState::UpdateAvailable)
                {
                    t(Msg::BtnDrvUpdate)
                } else {
                    t(Msg::BtnDrvInstall)
                },
            );
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
        assert_eq!(rows[0].state_text(), "built-in 9.9.9 != extension 1.0.0");
        assert_eq!(rows[2].state_text(), "built-in = extension 1.0.0");
        assert_eq!(rows[3].state_text(), "built-in 9.9.9");
        assert_eq!(rows[1].state_text(), "no build for this OS");
        assert_eq!(rows[4].builtin, None);
        assert_eq!(rows[4].state_text(), "installed");
        let mut d_latest = rows[3].clone();
        d_latest.latest = Some("0.1.1".into());
        assert_eq!(
            d_latest.state_text(),
            "built-in 9.9.9 - extension 0.1.1 available"
        );
        // 색인 없음 = 설치본만.
        let only = build_rows(None, &[ext("q", "3")], "x", |_| None);
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].state, DriverState::Installed);
        // 버튼 활성: 미설치 = 설치만 · 설치됨 = 삭제·확인 · 갱신 = 셋 다 · 작업 중 = 전부 끔.
        assert_eq!(
            DriverWin::button_states(Some(&rows[3]), false),
            (true, true, false, false)
        );
        assert_eq!(
            DriverWin::button_states(Some(&rows[2]), false),
            (true, false, true, true)
        );
        assert_eq!(
            DriverWin::button_states(Some(&rows[0]), false),
            (true, true, true, true)
        );
        assert_eq!(
            DriverWin::button_states(Some(&rows[0]), true),
            (false, false, false, false)
        );
        assert_eq!(
            DriverWin::button_states(None, false),
            (true, false, false, false)
        );
    }
}
