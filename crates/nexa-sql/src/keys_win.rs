//! 단축키 설정 창(사용자 09-15 · T-53) — 명령 목록(라벨 · 현재 단축키 · 충돌 배지) + **캡처**(지정 → 다음 키 조합).
//!
//! 메인 창의 소유 창. 값은 호스트가 [`KeysAction::Changed`]를 받아 설정 `key.<id>`에 쓰고 키맵을 다시 만든다(이 창은 고르기만).
//! - [지정] = 캡처 모드(행이 "키를 누르세요…" · Esc = 취소) · [비우기] = 단축키 없음(`none`) · [초기화] = 전부 플랫폼 기본.
//! - 충돌 = 같은 조합을 쓰는 다른 명령 이름을 행 끝에 경고색으로.
//! - ★ 174차(T-14): 사용자 키맵 파일(`Packages/User/*.sublime-keymap`)의 바인딩은 단축키 뒤 "(파일)" 꼬리 · context 바인딩은
//!   명령 행 뒤에 읽기 전용 행(조건 요약 `when …`) — 이 창에서는 바꿀 수 없다(파일을 고친다 · [지정]/[비우기] 비활성).

use crate::keymap::{self, Chord, Keymap, COMMANDS};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, TextBox, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 창이 호스트에 요청하는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeysAction {
    None,
    Paint,
    /// 명령 `id`의 단축키를 `code`로(설정 문법 · `none` = 없음).
    Changed {
        id: String,
        code: String,
    },
    /// 전부 플랫폼 기본으로.
    ResetAll,
}

const PAD: f32 = 12.0;
const ROW_H: f32 = 26.0;
const HEAD_H: f32 = 24.0;
const BTN_H: f32 = 28.0;
const LABEL_W: f32 = 230.0;
const CHORD_W: f32 = 170.0;
/// ★ T-53 잔여(10-30): 검색 칸 높이(안내 아래 · 머리글 위).
const SEARCH_H: f32 = 28.0;

/// 표시 행(호스트가 키맵으로 채운다).
#[derive(Clone, Debug)]
struct RowView {
    id: &'static str,
    label: String,
    chord: String,
    conflict: Option<String>,
    /// ★ 174차: 사용자 키맵 파일의 바인딩이 섞여 있다 → 단축키 뒤 "(파일)" 꼬리(바꾸려면 파일을 고친다).
    file: bool,
    /// ★ 174차: context 바인딩 행(조건 요약 · 읽기 전용 = [지정]/[비우기] 비활성 · 명령 행들 뒤에 붙는다).
    when: Option<String>,
    /// ★ 180차(결정 ② a′): 파일이 이 명령에 묶은 조합의 표시 글 — [비우기] 때 "파일이 … 지정(파일 우선)" 안내에 쓴다.
    file_disp: Option<String>,
    /// ★ T-53 잔여(10-30): 그룹 머리 행(명령 id 접두 `file.`·`edit.` … 의 라벨 · 고를 수 없음 · 검색 때 일치 행이 있는 그룹만).
    header: bool,
}

pub(crate) struct KeysWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    alt: bool,
    /// macOS Control 눌림(⌘와 별개 · 키맵 `ctrl+cmd+…`).
    ctrl_mac: bool,
    rows: Vec<RowView>,
    selected: Option<usize>,
    /// 캡처 중(선택 행에 다음 키 조합을 넣는다).
    capturing: bool,
    /// ★ 180차(결정 ② a′ · D-397): 파일의 context 없는 바인딩 복사본(열 때) — [지정]한 조합을 파일이 쓰면 안내(B).
    file_chords: Vec<(Vec<Chord>, &'static str)>,
    /// ★ 180차: 안내 한 줄(명령 id · 글) — 그 행의 오른쪽 열에 경고색. 동작은 파일 > 설정 그대로 · 혼란만 줄인다(사용자 10-30 a′).
    notice: Option<(&'static str, String)>,
    /// ★ T-53 잔여(10-30): 거르기 전 행 전부(키맵 순서) — `rows` = 그룹 머리를 끼우고 검색으로 거른 표시 뷰.
    all: Vec<RowView>,
    /// 검색 칸(한 줄 · 라벨·단축키·id·조건에 부분 일치 · 대소문자 무시) — 캡처 중이 아닐 때 글자 키는 전부 이리로.
    search: TextBox,
    /// 마지막으로 거른 질의(바뀌었을 때만 다시 만든다).
    query: String,
    scroll: i32,
    list: Rect,
    assign_btn: Button,
    clear_btn: Button,
    reset_btn: Button,
    close_btn: Button,
}

impl KeysWin {
    pub(crate) fn new() -> Self {
        KeysWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            alt: false,
            ctrl_mac: false,
            rows: Vec::new(),
            selected: None,
            capturing: false,
            file_chords: Vec::new(),
            notice: None,
            all: Vec::new(),
            search: TextBox::new(t(Msg::PhFilter)),
            query: String::new(),
            scroll: 0,
            list: Rect::default(),
            assign_btn: Button::new(t(Msg::BtnAssign)),
            clear_btn: Button::new(t(Msg::BtnClear)),
            reset_btn: Button::new(t(Msg::BtnReset)),
            close_btn: Button::new(t(Msg::BtnClose)),
        }
    }

    /// 키맵으로 표시 행을 다시 만든다(열 때 · 값이 바뀔 때).
    pub(crate) fn refresh(&mut self, km: &Keymap) {
        self.all = COMMANDS
            .iter()
            .map(|c| {
                let code = km.code_of(c.id);
                let conflict = code
                    .split('|')
                    .filter_map(Chord::parse)
                    .find_map(|ch| km.conflict(&ch, c.id))
                    .map(keymap::label_of);
                RowView {
                    id: c.id,
                    label: t(c.label).to_string(),
                    chord: km.display_of(c.id),
                    conflict,
                    file: km.file_bound(c.id),
                    when: None,
                    file_disp: km.file_display_of(c.id),
                    header: false,
                }
            })
            .collect();
        self.file_chords = km.file_chords().to_vec();
        // ★ 174차: 사용자 키맵 파일의 context 바인딩은 명령 행 뒤에 읽기 전용 행으로(조건 요약 열) — 그룹은 그 명령의 접두를 따른다.
        self.all
            .extend(km.ctx_rows().into_iter().map(|(chord, when, id)| RowView {
                id,
                label: keymap::label_of(id),
                chord,
                conflict: None,
                file: true,
                when: Some(when),
                file_disp: None,
                header: false,
            }));
        self.rebuild_view();
    }

    /// ★ T-53 잔여(10-30): `all` → `rows` = 그룹 머리(id 접두 · 첫 등장 순) + 그 그룹의 행(검색 일치만 · 빈 그룹은 머리도 없음).
    ///   선택은 같은 id의 행으로 따라가고 없으면 풀린다.
    fn rebuild_view(&mut self) {
        let q = self.query.to_lowercase();
        let keep_id = self
            .selected
            .and_then(|i| self.rows.get(i))
            .map(|r| (r.id, r.when.clone()));
        let mut groups: Vec<&str> = Vec::new();
        for r in &self.all {
            let g = group_key(r.id);
            if !groups.contains(&g) {
                groups.push(g);
            }
        }
        let mut rows: Vec<RowView> = Vec::new();
        for g in groups {
            let members: Vec<&RowView> = self
                .all
                .iter()
                .filter(|r| group_key(r.id) == g && row_matches(r, &q))
                .collect();
            if members.is_empty() {
                continue;
            }
            rows.push(RowView {
                id: "",
                label: group_label(g),
                chord: String::new(),
                conflict: None,
                file: false,
                when: None,
                file_disp: None,
                header: true,
            });
            rows.extend(members.into_iter().cloned());
        }
        self.rows = rows;
        self.selected = keep_id.and_then(|(id, when)| {
            self.rows
                .iter()
                .position(|r| !r.header && r.id == id && r.when == when)
        });
        self.clamp_scroll();
        self.sync_enabled();
    }

    /// 검색 칸의 글이 바뀌었으면 다시 거른다(참 = 바뀜).
    fn sync_query(&mut self) -> bool {
        let now = self.search.text();
        if now == self.query {
            return false;
        }
        self.query = now;
        self.scroll = 0;
        self.rebuild_view();
        true
    }

    /// 선택 행을 이 창에서 바꿀 수 있는가(context 행 = 파일에서만 · 그룹 머리 = 아님).
    fn editable(&self) -> bool {
        self.selected.is_some_and(|i| {
            self.rows
                .get(i)
                .is_some_and(|r| r.when.is_none() && !r.header)
        })
    }

    fn sync_enabled(&mut self) {
        let on = self.editable() && !self.capturing;
        self.assign_btn.set_enabled(on);
        self.clear_btn.set_enabled(on);
    }

    /// ★ 180차(결정 ② a′): [지정]으로 받은 조합을 키맵 파일이 **다른** 명령에 묶어 두었으면 안내 글(B 경우 — 설정은 저장되지만
    ///   키맵을 다시 만들 때 파일이 가로채므로 이 행에서는 빠져 보인다). 같은 명령이면 가로채는 것이 없으니 None.
    fn assign_notice(&self, ch: &Chord, id: &str) -> Option<String> {
        keymap::file_owner_in(&self.file_chords, std::slice::from_ref(ch))
            .filter(|owner| *owner != id)
            .map(|owner| {
                tf(
                    Msg::KeysFileHasKey,
                    &[&ch.display(), &keymap::label_of(owner)],
                )
            })
    }

    /// ★ 180차(결정 ② a′): [비우기]한 명령을 키맵 파일이 지정하고 있으면 안내 글(C 경우 — `none`을 저장해도 파일이 다시 붙인다).
    fn clear_notice(&self, i: usize) -> Option<String> {
        self.rows
            .get(i)
            .and_then(|r| r.file_disp.as_deref())
            .map(|d| tf(Msg::KeysFileHasCmd, &[d]))
    }

    /// 캡처 시작(안내는 새 동작이 시작되면 걷는다).
    fn start_capture(&mut self) {
        self.capturing = true;
        self.notice = None;
        self.sync_enabled();
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
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

    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if self.window.is_none() {
            return false;
        }
        let mut any = false;
        for b in self.buttons() {
            any |= b.tick(now_ms);
        }
        any
    }

    pub(crate) fn animating(&self) -> bool {
        self.window.is_some()
            && [
                &self.assign_btn,
                &self.clear_btn,
                &self.reset_btn,
                &self.close_btn,
            ]
            .iter()
            .any(|b| b.is_animating())
    }

    fn buttons(&mut self) -> [&mut Button; 4] {
        [
            &mut self.assign_btn,
            &mut self.clear_btn,
            &mut self.reset_btn,
            &mut self.close_btn,
        ]
    }

    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        over: Option<(i32, i32, u32, u32)>,
        owner: Option<&Window>,
    ) {
        if let Some(w) = &self.window {
            crate::winfocus::focus(w);
            return;
        }
        let (lw, lh) = (560.0, 620.0);
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL - {}", t(Msg::WinKeys)))
            .with_theme(theme)
            .with_resizable(true)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
        if let Some((x, y, w, h)) = over {
            let cx = x + (w as i32 - lw as i32) / 2;
            let cy = y + (h as i32 - lh as i32) / 2;
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(cx.max(0), cy.max(0)));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(crate::winfocus::attrs_headless(attrs)) else {
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        self.window = Some(win);
        self.capturing = false;
        self.sync_enabled();
        self.layout();
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
        self.capturing = false;
        self.notice = None;
    }

    fn layout(&mut self) {
        let Some(win) = &self.window else { return };
        let size = win.inner_size();
        let (w, h) = (size.width as i32, size.height as i32);
        let s = self.scale;
        let pad = self.s(PAD);
        let mut inv = Invalidations::default();
        let hint_h = self.s(40.0);
        // ★ T-53: 안내 아래 검색 칸 한 줄 → 머리글 → 목록.
        let sh = self.s(SEARCH_H);
        self.search.set_scale(s);
        self.search
            .set_bounds(Rect::new(pad, pad + hint_h, w - pad * 2, sh), &mut inv);
        let top = pad + hint_h + sh + self.s(6.0) + self.s(HEAD_H);
        let by = h - pad - self.s(BTN_H);
        self.list = Rect::new(pad, top, w - pad * 2, (by - pad) - top);
        let bw = self.s(96.0);
        let gap = self.s(8.0);
        let bh = self.s(BTN_H);
        let mut x = pad;
        for b in [&mut self.assign_btn, &mut self.clear_btn] {
            b.set_scale(s);
            b.set_bounds(Rect::new(x, by, bw, bh), &mut inv);
            x += bw + gap;
        }
        self.close_btn.set_scale(s);
        self.close_btn
            .set_bounds(Rect::new(w - pad - bw, by, bw, bh), &mut inv);
        self.reset_btn.set_scale(s);
        self.reset_btn
            .set_bounds(Rect::new(w - pad - bw * 2 - gap, by, bw, bh), &mut inv);
        self.clamp_scroll();
    }

    fn row_h(&self) -> i32 {
        self.s(ROW_H)
    }

    fn content_h(&self) -> i32 {
        self.rows.len() as i32 * self.row_h()
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h() - self.list.h).max(0);
        self.scroll = self.scroll.clamp(0, max);
    }

    fn row_at(&self, p: Point) -> Option<usize> {
        if !self.list.contains(p) {
            return None;
        }
        let i = (p.y - self.list.y + self.scroll) / self.row_h();
        (i >= 0 && (i as usize) < self.rows.len()).then_some(i as usize)
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> KeysAction {
        match ev {
            WindowEvent::CloseRequested => {
                self.close();
                return KeysAction::None;
            }
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
                return KeysAction::None;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
                self.redraw();
                return KeysAction::None;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
                // macOS Control(⌘와 별개 · Sublime `ctrl+cmd+g`) — 다른 OS에선 늘 false.
                self.ctrl_mac = cfg!(target_os = "macos") && m.state().control_key();
                self.alt = m.state().alt_key();
                return KeysAction::None;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                let mut inv = Invalidations::default();
                for b in self.buttons() {
                    b.on_event(&mv, &mut inv);
                }
                self.redraw();
                return KeysAction::None;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y * ROW_H * 3.0 * self.scale,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32,
                };
                self.scroll -= dy.round() as i32;
                self.clamp_scroll();
                self.redraw();
                return KeysAction::None;
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                let is_esc = matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape));
                if self.capturing {
                    if is_esc {
                        self.capturing = false;
                        self.sync_enabled();
                        self.redraw();
                        return KeysAction::None;
                    }
                    // 조합키만 누른 경우는 None → 계속 기다린다.
                    let Some(ch) = Chord::from_winit(
                        &kev.logical_key,
                        &kev.physical_key,
                        self.primary,
                        self.shift,
                        self.alt,
                        self.ctrl_mac,
                    ) else {
                        return KeysAction::None;
                    };
                    self.capturing = false;
                    self.sync_enabled();
                    if let Some(i) = self.selected {
                        let id = self.rows[i].id;
                        self.notice = self.assign_notice(&ch, id).map(|n| (id, n));
                        return KeysAction::Changed {
                            id: id.to_string(),
                            code: ch.code(),
                        };
                    }
                    return KeysAction::None;
                }
                if is_esc {
                    // ★ T-53: 검색 글이 있으면 Esc는 먼저 검색을 비운다 · 빈 상태의 Esc = 창 닫기.
                    if !self.query.is_empty() {
                        self.search.set_text("");
                        self.sync_query();
                        self.redraw();
                        return KeysAction::None;
                    }
                    self.close();
                    return KeysAction::None;
                }
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::ArrowDown) => self.move_sel(1),
                    Key::Named(NamedKey::ArrowUp) => self.move_sel(-1),
                    Key::Named(NamedKey::Enter) if self.editable() => self.start_capture(),
                    Key::Named(NamedKey::Enter) => {}
                    _ => {
                        // ★ T-53: 그 밖의 키 = 검색 칸(글자 · Backspace · ←/→ · Home/End) — 목록에는 글 입력이 없으므로 늘 이리로.
                        if let Some(e) = crate::input::text_key_event(
                            kev,
                            self.shift,
                            self.primary,
                            crate::input::TextKeys::Line,
                        ) {
                            let mut inv = Invalidations::default();
                            self.search.set_focused(true);
                            for b in self.buttons() {
                                b.set_focused(false);
                            }
                            self.search.on_event(&e, &mut inv);
                            self.sync_query();
                        }
                    }
                }
                self.redraw();
                return KeysAction::None;
            }
            WindowEvent::RedrawRequested => return KeysAction::Paint,
            _ => {}
        }
        let (x, y) = self.cursor;
        let ie = match ev {
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => InputEvent::MouseDown {
                    x,
                    y,
                    shift: self.shift,
                    primary: false,
                },
                (ElementState::Released, MouseButton::Left) => InputEvent::MouseUp { x, y },
                _ => return KeysAction::None,
            },
            _ => return KeysAction::None,
        };
        let mut inv = Invalidations::default();
        if let InputEvent::MouseDown { x, y, .. } = ie {
            let p = Point { x, y };
            if let Some(i) = self.row_at(p) {
                // ★ T-53: 그룹 머리 행은 고르지 않는다(클릭 무시).
                if self.rows.get(i).is_some_and(|r| r.header) {
                    return KeysAction::None;
                }
                self.capturing = false;
                self.selected = Some(i);
                self.sync_enabled();
                for b in self.buttons() {
                    b.set_focused(false);
                }
                self.redraw();
                return KeysAction::None;
            }
        }
        // ★ T-53: 검색 칸(캐럿 놓기·드래그 선택) — 눌렀으면 포커스 링은 검색 칸에만.
        if self.search.bounds().contains(Point { x, y }) || self.search.is_focused() {
            self.search.on_event(&ie, &mut inv);
            if matches!(ie, InputEvent::MouseDown { .. })
                && self.search.bounds().contains(Point { x, y })
            {
                self.search.set_focused(true);
                for b in self.buttons() {
                    b.set_focused(false);
                }
                self.redraw();
                return KeysAction::None;
            }
        }
        for b in self.buttons() {
            b.on_event(&ie, &mut inv);
        }
        // 포커스 규칙(CLAUDE.md §3): 누른 버튼 하나만 링(버튼을 눌렀으면 검색 칸 링은 내린다).
        if let InputEvent::MouseDown { x, y, .. } = ie {
            let p = Point { x, y };
            let mut any = false;
            for b in self.buttons() {
                let hit = b.bounds().contains(p);
                any |= hit;
                b.set_focused(hit);
            }
            if any {
                self.search.set_focused(false);
            }
        }
        if self.assign_btn.take_clicked() {
            if self.editable() {
                self.start_capture();
            }
            self.redraw();
            return KeysAction::None;
        }
        if self.clear_btn.take_clicked() {
            self.redraw();
            if let Some(i) = self.selected.filter(|_| self.editable()) {
                let id = self.rows[i].id;
                self.notice = self.clear_notice(i).map(|n| (id, n));
                return KeysAction::Changed {
                    id: id.to_string(),
                    code: "none".into(),
                };
            }
            return KeysAction::None;
        }
        if self.reset_btn.take_clicked() {
            self.capturing = false;
            self.notice = None;
            self.sync_enabled();
            self.redraw();
            return KeysAction::ResetAll;
        }
        if self.close_btn.take_clicked() {
            self.close();
            return KeysAction::None;
        }
        self.redraw();
        KeysAction::None
    }

    fn move_sel(&mut self, d: i32) {
        if self.rows.is_empty() {
            return;
        }
        let n = self.rows.len() as i32;
        let cur = self.selected.map_or(-1, |i| i as i32);
        let mut next = (cur + d).clamp(0, n - 1);
        // ★ T-53: 그룹 머리 행은 건너뛴다(끝에 머리만 남으면 제자리).
        while self.rows[next as usize].header {
            let step = if d < 0 { -1 } else { 1 };
            let cand = next + step;
            if cand < 0 || cand >= n {
                if cur >= 0 {
                    return;
                }
                break;
            }
            next = cand;
        }
        if self.rows[next as usize].header {
            return;
        }
        self.selected = Some(next as usize);
        self.sync_enabled();
        // 보이게 스크롤.
        let top = next * self.row_h();
        if top < self.scroll {
            self.scroll = top;
        } else if top + self.row_h() > self.scroll + self.list.h {
            self.scroll = top + self.row_h() - self.list.h;
        }
        self.clamp_scroll();
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
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let pad = (PAD * s).round() as i32;
        let list = self.list;
        let row_h = (ROW_H * s).round() as i32;
        let label_w = (LABEL_W * s).round() as i32;
        let chord_w = (CHORD_W * s).round() as i32;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs::with_base(font_px);
            let mut dc = RasterCtx::new(&mut gfx, ui, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.window_bg);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            // 안내 2줄.
            for (i, line) in t(Msg::KeysHint).split('\n').enumerate() {
                dc.text(
                    pad,
                    pad + i as i32 * th_txt,
                    Rect::new(0, 0, wi, hi),
                    line,
                    th.text_dim,
                );
            }
            // ★ T-53: 검색 칸.
            self.search.paint(&mut dc, th);
            // 머리글.
            let hy = list.y - (HEAD_H * s).round() as i32;
            let head = Rect::new(list.x, hy, list.w, (HEAD_H * s).round() as i32);
            dc.fill_rect(head, th.panel_bg);
            let ty = hy + (head.h - th_txt) / 2;
            dc.text(
                list.x + pad / 2,
                ty,
                head,
                t(Msg::LblKeyCommand),
                th.text_dim,
            );
            dc.text(
                list.x + label_w,
                ty,
                head,
                t(Msg::LblKeyShortcut),
                th.text_dim,
            );
            dc.stroke_round_rect(
                Rect::new(list.x, hy, list.w, list.h + head.h),
                0,
                th.border,
                1.0,
            );
            // 행.
            let first = (self.scroll / row_h).max(0) as usize;
            for (i, r) in self.rows.iter().enumerate().skip(first) {
                let y = list.y + i as i32 * row_h - self.scroll;
                if y >= list.bottom() {
                    break;
                }
                let rr = Rect::new(list.x, y, list.w, row_h);
                let clip = rr.intersection(&list);
                if clip.h <= 0 {
                    continue;
                }
                let sel = self.selected == Some(i);
                if sel {
                    dc.fill_rect(clip, th.sel_bg);
                } else if i % 2 == 1 {
                    dc.fill_rect(clip, th.panel_bg);
                }
                let ty = y + (row_h - th_txt) / 2;
                if r.header {
                    // ★ T-53: 그룹 머리 = 띠 배경 + 흐린 라벨(단축키·조건 열 없음).
                    dc.fill_rect(clip, th.panel_bg);
                    dc.text(list.x + pad / 2, ty, clip, &r.label, th.text_dim);
                    continue;
                }
                dc.text(list.x + pad / 2, ty, clip, &r.label, th.text);
                let (chord, color) = if sel && self.capturing {
                    (t(Msg::KeysCapturing).to_string(), th.accent)
                } else if r.file {
                    (
                        format!("{} {}", r.chord, t(Msg::KeysFromFile)),
                        if r.when.is_some() {
                            th.text_dim
                        } else {
                            th.text
                        },
                    )
                } else {
                    (r.chord.clone(), th.text)
                };
                dc.text(list.x + label_w, ty, clip, &chord, color);
                if let Some((_, n)) = self.notice.as_ref().filter(|(nid, _)| *nid == r.id) {
                    // ★ 180차(결정 ② a′): 파일이 이 키/명령을 씀(파일 우선) — 충돌 열 자리에 경고색.
                    dc.text(list.x + label_w + chord_w, ty, clip, n, th.warn);
                } else if let Some(c) = &r.conflict {
                    dc.text(
                        list.x + label_w + chord_w,
                        ty,
                        clip,
                        &tf(Msg::KeysConflict, &[c]),
                        th.warn,
                    );
                } else if let Some(w) = &r.when {
                    dc.text(
                        list.x + label_w + chord_w,
                        ty,
                        clip,
                        &tf(Msg::KeysWhen, &[w]),
                        th.text_dim,
                    );
                }
            }
            for b in [
                &mut self.assign_btn,
                &mut self.clear_btn,
                &mut self.reset_btn,
                &mut self.close_btn,
            ] {
                b.paint(&mut dc, th);
            }
        }
        let _ = buf.present();
    }
}

/// ★ T-53 잔여(10-30): 명령 id의 그룹 열쇠 = 첫 `.` 앞 접두(`file.save` → `file` · 없으면 전체).
pub(crate) fn group_key(id: &str) -> &str {
    id.split_once('.').map_or(id, |(g, _)| g)
}

/// 그룹 라벨 — 메뉴 이름을 그대로 쓴다(`file`·`edit`·`view`·`run`·`find`·`bookmark`·`tab`·`conn`·`vars`·`mem`·`ext`·`goto` ·
/// 결과·객체·인텔리센스는 전용 글 · 그 밖 = "Other").
pub(crate) fn group_label(g: &str) -> String {
    let m = match g {
        "file" => Msg::MnFile,
        "edit" => Msg::MnEdit,
        "view" => Msg::MnView,
        "run" => Msg::MnRun,
        "find" => Msg::MnFind,
        "bookmark" => Msg::MnBookmarks,
        "tab" => Msg::MnTabs,
        "conn" => Msg::MnConnect,
        "vars" => Msg::MnVariables,
        "mem" => Msg::MnMemoryWindow,
        "ext" => Msg::MnExtensions,
        "goto" => Msg::MnGotoSymbol,
        "result" => Msg::KeysGroupResult,
        "obj" => Msg::KeysGroupObject,
        "intel" => Msg::KeysGroupIntel,
        _ => Msg::KeysGroupOther,
    };
    t(m).to_string()
}

/// 검색 일치(소문자 질의 · 빈 질의 = 전부): 라벨 · 단축키 표시 · 명령 id · 조건 요약 어디든 부분 일치.
fn row_matches(r: &RowView, q: &str) -> bool {
    q.is_empty()
        || r.label.to_lowercase().contains(q)
        || r.chord.to_lowercase().contains(q)
        || r.id.to_lowercase().contains(q)
        || r.when
            .as_deref()
            .is_some_and(|w| w.to_lowercase().contains(q))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// ★ T-53 잔여(10-30): 그룹 머리(접두 · 첫 등장 순 · 고를 수 없음 · ↑↓가 건너뜀) + 검색(라벨·단축키·id·조건 · 대소문자 무시 ·
    ///   빈 그룹은 머리도 없음 · 선택은 같은 행으로 따라감 · Esc = 검색 비우기 먼저).
    #[test]
    fn groups_and_search_filter_rows() {
        assert_eq!(group_key("file.save_as"), "file");
        assert_eq!(group_key("nodot"), "nodot");
        assert_eq!(group_label("file"), t(Msg::MnFile));
        assert_eq!(group_label("zzz"), t(Msg::KeysGroupOther));
        let mut w = KeysWin::new();
        w.refresh(&Keymap::default());
        let headers = w.rows.iter().filter(|r| r.header).count();
        assert!(headers >= 5, "그룹 머리 여러 개: {headers}");
        assert_eq!(
            w.rows.iter().filter(|r| !r.header).count(),
            COMMANDS.len(),
            "명령 행은 전부"
        );
        assert!(w.rows[0].header, "첫 행은 머리");
        assert_eq!(w.rows[0].label, group_label(group_key(COMMANDS[0].id)));
        // ↓ 첫 이동 = 머리를 건너뛰어 첫 명령 행.
        w.move_sel(1);
        assert_eq!(w.selected, Some(1));
        assert!(w.editable());
        // 검색 = "save" → file 그룹의 save 명령만 + 그 머리.
        w.search.set_text("SAVE");
        assert!(w.sync_query());
        assert!(!w.sync_query(), "같은 글 = 다시 안 거른다");
        assert!(w
            .rows
            .iter()
            .filter(|r| !r.header)
            .all(|r| r.id.contains("save")
                || r.label.to_lowercase().contains("save")
                || r.chord.to_lowercase().contains("save")));
        assert!(w.rows.iter().filter(|r| !r.header).count() >= 1);
        assert_eq!(
            w.selected, None,
            "선택했던 첫 명령(file.new)은 걸러져 선택 해제"
        );
        // 일치 없음 = 행 0(머리도 없음).
        w.search.set_text("no-such-command-xyz");
        w.sync_query();
        assert!(w.rows.is_empty());
        // 비우면 전부 복귀 · 선택은 id로 따라간다.
        w.search.set_text("");
        w.sync_query();
        assert_eq!(w.rows.iter().filter(|r| !r.header).count(), COMMANDS.len());
        let i = w.rows.iter().position(|r| r.id == "file.save").unwrap();
        w.selected = Some(i);
        w.search.set_text("file.save");
        w.sync_query();
        assert!(w.selected.is_some_and(|j| w.rows[j].id == "file.save"));
    }

    /// ★ 174차(T-14): 파일 바인딩 행 = "(파일)" 표시 · context 바인딩 = 명령 행 뒤 읽기 전용 행(조건 요약) · 그 행을 고르면 [지정]/[비우기] 꺼짐.
    #[test]
    fn file_and_context_rows_are_marked_and_context_rows_are_read_only() {
        let b = crate::keymap_file::parse(
            r#"[
                { "keys": ["ctrl+alt+9"], "command": "file.save_as" },
                { "keys": ["ctrl+k", "ctrl+9"], "command": "edit.upper_case",
                  "context": [{ "key": "num_selections", "operator": "equal", "operand": 2 }] }
            ]"#,
        )
        .unwrap();
        let mut km = Keymap::default();
        assert_eq!(km.apply_bindings(&b), (1, 1, 0));
        let mut w = KeysWin::new();
        w.refresh(&km);
        let cmd_rows = |w: &KeysWin| w.rows.iter().filter(|r| !r.header).count();
        assert_eq!(
            cmd_rows(&w),
            COMMANDS.len() + 1,
            "context 행 하나가 더 있다(그룹 머리는 세지 않음 · T-53)"
        );
        // ★ 표시 문자열은 OS마다 다르다(맥 = `⌘⌥9` · 그 밖 = `Ctrl+Alt+9` · ci 40903924326 macos ✗) → 같은 표시 함수로 기대값을 만든다.
        let disp = |code: &str| crate::keymap::Chord::parse(code).unwrap().display();
        let save_as = w.rows.iter().find(|r| r.id == "file.save_as").unwrap();
        assert!(save_as.file && save_as.when.is_none());
        assert_eq!(save_as.chord, disp("ctrl+alt+9"));
        // ★ T-53: context 행은 그 명령의 그룹(edit) 안 · 그 그룹의 마지막.
        let ctx_i = w.rows.iter().position(|r| r.when.is_some()).unwrap();
        let last = &w.rows[ctx_i];
        assert_eq!(last.id, "edit.upper_case");
        assert!(last.file);
        assert_eq!(
            last.chord,
            format!("{}, {}", disp("ctrl+k"), disp("ctrl+9"))
        );
        assert_eq!(last.when.as_deref(), Some("num_selections == 2"));
        assert!(
            w.rows.get(ctx_i + 1).is_none_or(|r| r.header),
            "그룹의 끝(다음은 머리 또는 없음)"
        );
        let plain = w.rows.iter().find(|r| r.id == "file.new").unwrap();
        assert!(!plain.file && plain.when.is_none());
        // 명령 행 선택 = 바꿀 수 있음 · context 행 선택 = 읽기 전용 · 머리 행 = 읽기 전용.
        let first_cmd = w.rows.iter().position(|r| !r.header).unwrap();
        w.selected = Some(first_cmd);
        w.sync_enabled();
        assert!(w.editable() && w.assign_btn.is_enabled() && w.clear_btn.is_enabled());
        w.selected = Some(ctx_i);
        w.sync_enabled();
        assert!(!w.editable() && !w.assign_btn.is_enabled() && !w.clear_btn.is_enabled());
        w.selected = Some(0);
        w.sync_enabled();
        assert!(!w.editable(), "머리 행");
        // 행이 사라진 키맵으로 다시 채우면 그 선택은 풀린다(context 행 선택 → 파일 없음).
        w.selected = Some(ctx_i);
        w.refresh(&Keymap::default());
        assert_eq!(cmd_rows(&w), COMMANDS.len());
        assert_eq!(w.selected, None);
    }

    /// ★ 180차(결정 ② a′ · D-397): 동작은 파일 > 설정 그대로 · 안내만 — B = [지정]한 조합을 파일이 다른 명령에 씀 → 안내(같은 명령이면 없음 ·
    ///   파일에 없는 조합이면 없음) · C = [비우기]한 명령을 파일이 지정 → 안내(파일 바인딩 없는 명령이면 없음).
    #[test]
    fn file_wins_notices_for_assign_and_clear() {
        let b = crate::keymap_file::parse(
            r#"[
                { "keys": ["ctrl+alt+9"], "command": "file.save_as" },
                { "keys": ["ctrl+k", "ctrl+9"], "command": "edit.upper_case",
                  "context": [{ "key": "num_selections", "operator": "equal", "operand": 2 }] }
            ]"#,
        )
        .unwrap();
        let mut km = Keymap::default();
        km.apply_bindings(&b);
        let mut w = KeysWin::new();
        w.refresh(&km);
        let ch = Chord::parse("ctrl+alt+9").unwrap();
        // B: 다른 명령에 지정 → 안내에 조합 표시와 파일 쪽 명령 라벨이 들어간다.
        let n = w.assign_notice(&ch, "edit.upper_case").expect("B 안내");
        assert!(
            n.contains(&ch.display()) && n.contains(&keymap::label_of("file.save_as")),
            "{n}"
        );
        // 같은 명령이면 가로채는 것이 없다 · 파일에 없는 조합도 없다.
        assert_eq!(w.assign_notice(&ch, "file.save_as"), None);
        assert_eq!(
            w.assign_notice(&Chord::parse("ctrl+alt+8").unwrap(), "edit.upper_case"),
            None
        );
        // C: 파일이 지정한 명령을 비우면 안내 · 아니면 없음 · context 행은 [비우기] 자체가 꺼지지만 file_disp도 없다.
        let i_save = w.rows.iter().position(|r| r.id == "file.save_as").unwrap();
        let i_new = w.rows.iter().position(|r| r.id == "file.new").unwrap();
        assert!(w
            .clear_notice(i_save)
            .is_some_and(|n| n.contains(&ch.display())));
        assert_eq!(w.clear_notice(i_new), None);
        assert_eq!(w.clear_notice(w.rows.len() - 1), None);
        // 안내는 새 캡처를 시작하면 걷힌다.
        w.notice = Some(("file.save_as", "x".into()));
        w.selected = Some(i_save);
        w.start_capture();
        assert!(w.capturing && w.notice.is_none());
        // 파일 없는 키맵 = 안내 없음.
        w.refresh(&Keymap::default());
        assert_eq!(w.assign_notice(&ch, "edit.upper_case"), None);
    }
}
