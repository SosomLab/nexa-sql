//! 색 설정 창(사용자 09-14) — 버튼 hover 색 · 눌림 색을 nexa-ctl [`ColorPanel`](투명도·프리셋·최근 색)로 고른다.
//!
//! 메인 창의 소유 창(작업표시줄 항목 1개). 왼쪽 = 대상 2개(Hover · Pressed · 현재 색 칩) · 오른쪽 = 색 패널 ·
//! 아래 = 미리보기 버튼(고른 색이 실시간으로 hover/눌림에 반영된다) · 초기화 · 닫기.
//! 값은 `#RRGGBBAA`로 설정(`ui.hover_color` · `ui.pressed_color`)에 저장되고, 최근 색은 `ui.color_recent`.
//! 이 창은 색을 **고르기만** 한다 — 적용(nexa-ctl 토큰)·저장은 호스트가 [`ColorsAction::Changed`]를 받아서.

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, Theme};
use nexa_ctl::{
    rgba_from_hex, Button, ColorPanel, Control, InputEvent, Invalidations, Key as CtlKey, Widget,
};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 고르는 대상.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColorTarget {
    Hover,
    Pressed,
    /// 설정 레지스트리의 임의 색 키(`*_color` · 설정 창 "고르기" · 09-17) — 값은 `#RRGGBBAA`로 저장.
    Key(&'static str),
}

impl ColorTarget {
    pub(crate) const ALL: [ColorTarget; 2] = [ColorTarget::Hover, ColorTarget::Pressed];

    pub(crate) fn key(self) -> &'static str {
        match self {
            ColorTarget::Hover => "ui.hover_color",
            ColorTarget::Pressed => "ui.pressed_color",
            ColorTarget::Key(k) => k,
        }
    }

    fn label(self) -> Msg {
        match self {
            ColorTarget::Hover => Msg::LblColorHover,
            ColorTarget::Pressed => Msg::LblColorPressed,
            ColorTarget::Key(k) => nsql_settings::entry(k).map_or(Msg::WinColors, |e| e.label),
        }
    }

    fn idx(self) -> usize {
        match self {
            ColorTarget::Hover | ColorTarget::Key(_) => 0,
            ColorTarget::Pressed => 1,
        }
    }
}

/// ★ T-119(11-22): 값이 **색 목록**(`#RRGGBB,#RRGGBB,…`)인 설정 키 — 설정 창 [선택…]이 이 창을 목록 모드로 연다.
pub(crate) fn is_color_list_key(k: &str) -> bool {
    k == "ext.rainbow_pairs.colors"
}

/// 목록에 둘 수 있는 색의 최대 수(창 높이에 맞춘 값 · 넘는 항목은 읽을 때 버린다).
const LIST_MAX: usize = 8;

/// `#RRGGBB`/`#RRGGBBAA` → `#RRGGBB`(대문자 · 알파 버림) — 형식 오류 = None.
fn norm_hex(s: &str) -> Option<String> {
    rgba_from_hex(s).map(|rgba| format!("#{:06X}", rgba >> 8))
}

/// 색 목록 편집 모델(순수 · T-119): 항목 = `#RRGGBB` · 선택 하나 · 최소 1 · 최대 [`LIST_MAX`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColorList {
    colors: Vec<String>,
    sel: usize,
}

impl ColorList {
    /// 설정 글(쉼표 구분) → 목록. 잘못된 항목은 건너뛰고(확장의 `parse_colors`와 같은 규칙), 하나도 없으면 `default`.
    pub(crate) fn parse(spec: &str, default: &[String]) -> Self {
        let mut colors: Vec<String> = spec.split(',').filter_map(norm_hex).collect();
        if colors.is_empty() {
            colors = default.iter().filter_map(|c| norm_hex(c)).collect();
        }
        if colors.is_empty() {
            colors.push("#808080".into());
        }
        colors.truncate(LIST_MAX);
        ColorList { colors, sel: 0 }
    }

    /// 설정에 쓰는 글(`#RRGGBB,#RRGGBB,…`).
    pub(crate) fn join(&self) -> String {
        self.colors.join(",")
    }

    fn selected(&self) -> &str {
        &self.colors[self.sel]
    }

    fn select(&mut self, i: usize) -> bool {
        let ok = i < self.colors.len() && i != self.sel;
        if ok {
            self.sel = i;
        }
        ok
    }

    /// 선택한 색을 그 아래에 하나 더(고른 뒤 바꾸면 된다) — 꽉 찼으면 false.
    fn add(&mut self) -> bool {
        if self.colors.len() >= LIST_MAX {
            return false;
        }
        let c = self.selected().to_string();
        self.sel += 1;
        self.colors.insert(self.sel, c);
        true
    }

    /// 선택한 색을 뺀다 — 하나 남았으면 false(빈 목록은 "테마 기본"이라 [초기화]가 맡는다).
    fn del(&mut self) -> bool {
        if self.colors.len() <= 1 {
            return false;
        }
        self.colors.remove(self.sel);
        self.sel = self.sel.min(self.colors.len() - 1);
        true
    }

    fn move_by(&mut self, up: bool) -> bool {
        let to = if up {
            self.sel.checked_sub(1)
        } else {
            (self.sel + 1 < self.colors.len()).then_some(self.sel + 1)
        };
        let Some(to) = to else { return false };
        self.colors.swap(self.sel, to);
        self.sel = to;
        true
    }

    /// 선택한 항목의 색을 바꾼다(형식 오류·같은 값 = false).
    fn set_selected(&mut self, hex: &str) -> bool {
        match norm_hex(hex) {
            Some(h) if h != self.colors[self.sel] => {
                self.colors[self.sel] = h;
                true
            }
            _ => false,
        }
    }
}

/// 창이 호스트에 요청하는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ColorsAction {
    None,
    Paint,
    /// 값이 바뀌었다(드래그 중에도 — 실시간 미리보기) · `hex` = `#RRGGBBAA`.
    Changed {
        target: ColorTarget,
        hex: String,
    },
    /// 두 색 모두 테마 기본으로(키 모드 = 그 키만).
    Reset,
}

const PAD: f32 = 12.0;
const LIST_W: f32 = 150.0;
const ROW_H: f32 = 32.0;
const BTN_H: f32 = 28.0;

pub(crate) struct ColorsWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    panel: ColorPanel,
    target: ColorTarget,
    /// 대상별 값(`#RRGGBBAA` · None = 테마 기본).
    values: [Option<String>; 2],
    /// 키 모드(설정 창에서 연 임의 색 키) — 목록은 이 키 하나 · 값 None = 기본(빈 문자열).
    key_mode: Option<(&'static str, Option<String>)>,
    /// ★ T-119 목록 모드(값이 색 목록인 키 · [`is_color_list_key`]) — 왼쪽 = 색 행들 + [+][−][▲][▼] · 오른쪽 패널 = 선택한 행의 색.
    list: Option<(&'static str, ColorList)>,
    preview: Button,
    reset_btn: Button,
    close_btn: Button,
    /// 목록 모드의 추가 · 삭제 · 위로 · 아래로(다른 모드에서는 빈 사각형이라 눌리지 않는다).
    list_btns: [Button; 4],
    list_rows: Vec<Rect>,
}

impl ColorsWin {
    pub(crate) fn new(hover: Option<String>, pressed: Option<String>, recent: &[u32]) -> Self {
        let mut panel = ColorPanel::new(hover.as_deref().unwrap_or("#D8E8FFFF"));
        panel.set_recent(recent);
        ColorsWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            panel,
            target: ColorTarget::Hover,
            values: [hover, pressed],
            key_mode: None,
            list: None,
            preview: Button::new(t(Msg::BtnPreview)),
            reset_btn: Button::new(t(Msg::BtnReset)),
            close_btn: Button::new(t(Msg::BtnClose)),
            list_btns: [
                Button::new("+"),
                Button::new("\u{2212}"),
                Button::new("\u{25B2}"),
                Button::new("\u{25BC}"),
            ],
            list_rows: Vec::new(),
        }
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

    /// 버튼 hover 페이드 틱 — 다시 그려야 하면 true.
    pub(crate) fn tick(&mut self, now_ms: u64) -> bool {
        if self.window.is_none() {
            return false;
        }
        let a = self.preview.tick(now_ms);
        let b = self.reset_btn.tick(now_ms);
        let c = self.close_btn.tick(now_ms);
        let d = self.panel.tick(now_ms);
        let mut e = false;
        for bt in &mut self.list_btns {
            e |= bt.tick(now_ms);
        }
        a || b || c || d || e
    }

    pub(crate) fn animating(&self) -> bool {
        self.window.is_some()
            && (self.preview.is_animating()
                || self.reset_btn.is_animating()
                || self.close_btn.is_animating()
                || self.panel.is_animating()
                || self.list_btns.iter().any(Button::is_animating))
    }

    /// 대상의 현재 hex(없으면 테마 기본 = `sel_bg` 불투명 · 키 모드 = 강조색 70%).
    fn value_of(&self, th: &Theme, tg: ColorTarget) -> String {
        if let Some((_, l)) = &self.list {
            return format!("{}FF", l.selected());
        }
        if let ColorTarget::Key(_) = tg {
            return self
                .key_mode
                .as_ref()
                .and_then(|(_, v)| v.clone())
                .unwrap_or_else(|| format!("#{:06X}B3", th.accent.0 & 0x00FF_FFFF));
        }
        self.values[tg.idx()]
            .clone()
            .unwrap_or_else(|| format!("#{:06X}FF", th.sel_bg.0 & 0x00FF_FFFF))
    }

    /// 지금 보이는 대상 목록(키 모드 = 하나 · 기본 = hover·pressed).
    fn targets(&self) -> Vec<ColorTarget> {
        if self.list.is_some() {
            return Vec::new(); // 목록 모드의 행은 색 항목(대상이 아니다)
        }
        match self.key_mode {
            Some((k, _)) => vec![ColorTarget::Key(k)],
            None => ColorTarget::ALL.to_vec(),
        }
    }

    /// 설정 창의 색 키 "고르기" — 이 키 하나만 고르는 모드로 전환(열려 있어도 대상만 바꾼다).
    pub(crate) fn set_key_mode(&mut self, key: &'static str, value: Option<String>, th: &Theme) {
        self.list = None;
        self.key_mode = Some((key, value));
        self.target = ColorTarget::Key(key);
        let v = self.value_of(th, self.target);
        self.panel.set_value(&v);
        self.panel.set_focused(false);
        self.redraw();
    }

    /// 기본 모드(hover·pressed).
    /// ★ T-119 목록 모드로 전환: `spec` = 설정 값(쉼표 구분 · 비면 테마의 괄호 색 6개) — 선택 = 첫 색.
    pub(crate) fn set_list_mode(&mut self, key: &'static str, spec: &str, th: &Theme) {
        let default: Vec<String> = th
            .rainbow
            .iter()
            .map(|c| format!("#{:06X}", c.0 & 0x00FF_FFFF))
            .collect();
        self.key_mode = None;
        self.list = Some((key, ColorList::parse(spec, &default)));
        self.target = ColorTarget::Key(key);
        self.sync_list_panel(th);
        self.layout();
        self.redraw();
    }

    /// 목록 모드: 패널 값 = 선택한 행의 색 · 버튼 활성(삭제 = 2개 이상 · 추가 = 최대 미만 · 이동 = 끝이 아닐 때).
    fn sync_list_panel(&mut self, th: &Theme) {
        let v = self.value_of(th, self.target);
        self.panel.set_value(&v);
        self.panel.set_focused(false);
        if let Some((_, l)) = &self.list {
            let (n, sel) = (l.colors.len(), l.sel);
            let on = [n < LIST_MAX, n > 1, sel > 0, sel + 1 < n];
            for (b, e) in self.list_btns.iter_mut().zip(on) {
                b.set_enabled(e);
            }
        }
    }

    /// 목록 모드의 조작 하나(버튼 · 자체 시험 `colors.op:`) — `add` · `del` · `up` · `down` · `sel:<n>` · `set:<hex>`.
    /// 값이 바뀌면 `Changed`(hex = 쉼표로 이은 목록) · 선택만 바뀌면 `None`.
    pub(crate) fn list_op(&mut self, op: &str, th: &Theme) -> ColorsAction {
        let Some((key, l)) = &mut self.list else {
            return ColorsAction::None;
        };
        let key = *key;
        let changed = match op.split_once(':') {
            Some(("sel", n)) => {
                if let Ok(i) = n.trim().parse::<usize>() {
                    l.select(i);
                }
                false
            }
            Some(("set", hex)) => l.set_selected(hex),
            _ => match op {
                "add" => l.add(),
                "del" => l.del(),
                "up" => l.move_by(true),
                "down" => l.move_by(false),
                _ => false,
            },
        };
        let hex = l.join();
        self.sync_list_panel(th);
        self.layout();
        self.redraw();
        if changed {
            ColorsAction::Changed {
                target: ColorTarget::Key(key),
                hex,
            }
        } else {
            ColorsAction::None
        }
    }

    /// 자체 시험 `colors.dump:` — 모드 · 키 · 선택 · 값 한 줄.
    pub(crate) fn dump(&self) -> String {
        match (&self.list, &self.key_mode) {
            (Some((k, l)), _) => format!(
                "mode=list key={k} sel={} n={} colors={}\n",
                l.sel,
                l.colors.len(),
                l.join()
            ),
            (None, Some((k, v))) => {
                format!("mode=key key={k} value={}\n", v.as_deref().unwrap_or("-"))
            }
            (None, None) => "mode=default\n".to_string(),
        }
    }

    /// 기본 모드(hover·pressed).
    pub(crate) fn set_default_mode(&mut self, th: &Theme) {
        if self.list.take().is_some() {
            self.target = ColorTarget::Hover;
            let v = self.value_of(th, self.target);
            self.panel.set_value(&v);
            self.layout();
            self.redraw();
        }
        if self.key_mode.take().is_some() {
            self.target = ColorTarget::Hover;
            let v = self.value_of(th, self.target);
            self.panel.set_value(&v);
            self.redraw();
        }
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
        let (lw, lh) = (500.0, 340.0);
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL - {}", t(Msg::WinColors)))
            .with_theme(theme)
            .with_resizable(false)
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
        self.layout();
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
    }

    fn layout(&mut self) {
        let Some(win) = &self.window else { return };
        let size = win.inner_size();
        let (w, h) = (size.width as i32, size.height as i32);
        let s = self.scale;
        let pad = self.s(PAD);
        let mut inv = Invalidations::default();
        // 왼쪽 대상 목록.
        let list_x = pad;
        let mut y = pad + self.s(20.0);
        // 목록 모드 = 색 수만큼 낮은 행 + 그 아래 [+][−][▲][▼] 한 줄 · 그 밖 = 대상 2행(버튼 4는 빈 사각형 = 눌리지 않는다).
        let n_list = self.list.as_ref().map(|(_, l)| l.colors.len());
        let (rows, row_h, gap_y) = match n_list {
            Some(n) => (n, self.s(24.0), self.s(2.0)),
            None => (2, self.s(ROW_H), self.s(4.0)),
        };
        self.list_rows.clear();
        for _ in 0..rows {
            self.list_rows
                .push(Rect::new(list_x, y, self.s(LIST_W), row_h));
            y += row_h + gap_y;
        }
        let (sbw, sbh, sgap) = (self.s(34.0), self.s(24.0), self.s(4.0));
        for (i, b) in self.list_btns.iter_mut().enumerate() {
            b.set_scale(s);
            let r = if n_list.is_some() {
                Rect::new(list_x + i as i32 * (sbw + sgap), y + sgap, sbw, sbh)
            } else {
                Rect::default()
            };
            b.set_bounds(r, &mut inv);
        }
        // 오른쪽 색 패널.
        self.panel.set_scale(s);
        let px = pad + self.s(LIST_W) + pad;
        let (pw, ph) = self.panel.preferred_size();
        self.panel
            .set_bounds(Rect::new(px, pad, pw.min(w - px - pad), ph), &mut inv);
        // 아래 버튼 줄.
        let by = h - pad - self.s(BTN_H);
        let bw = self.s(96.0);
        let gap = self.s(8.0);
        for b in [&mut self.preview, &mut self.reset_btn, &mut self.close_btn] {
            b.set_scale(s);
        }
        self.preview
            .set_bounds(Rect::new(pad, by, bw, self.s(BTN_H)), &mut inv);
        self.close_btn
            .set_bounds(Rect::new(w - pad - bw, by, bw, self.s(BTN_H)), &mut inv);
        self.reset_btn.set_bounds(
            Rect::new(w - pad - bw * 2 - gap, by, bw, self.s(BTN_H)),
            &mut inv,
        );
    }

    fn to_input(&self, ev: &WindowEvent) -> Option<InputEvent> {
        let (x, y) = self.cursor;
        let key = |k: CtlKey| InputEvent::Key {
            key: k,
            shift: self.shift,
            primary: false,
        };
        Some(match ev {
            WindowEvent::MouseInput { state, button, .. } => match (state, button) {
                (ElementState::Pressed, MouseButton::Left) => InputEvent::MouseDown {
                    x,
                    y,
                    shift: self.shift,
                    primary: false,
                },
                (ElementState::Released, MouseButton::Left) => InputEvent::MouseUp { x, y },
                _ => return None,
            },
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Enter) => key(CtlKey::Enter),
                    Key::Named(NamedKey::ArrowLeft) => key(CtlKey::Left),
                    Key::Named(NamedKey::ArrowRight) => key(CtlKey::Right),
                    Key::Named(NamedKey::Home) => key(CtlKey::Home),
                    Key::Named(NamedKey::End) => key(CtlKey::End),
                    Key::Named(NamedKey::Delete) => key(CtlKey::Delete),
                    Key::Named(NamedKey::Backspace) => InputEvent::Char {
                        c: '\u{8}',
                        now_ms: 0,
                    },
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

    pub(crate) fn handle(&mut self, ev: &WindowEvent, th: &Theme) -> ColorsAction {
        match ev {
            WindowEvent::CloseRequested => {
                self.close();
                return ColorsAction::None;
            }
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
                return ColorsAction::None;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
                self.redraw();
                return ColorsAction::None;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                return ColorsAction::None;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                let mut inv = Invalidations::default();
                self.panel.on_event(&mv, &mut inv);
                for b in [&mut self.preview, &mut self.reset_btn, &mut self.close_btn] {
                    b.on_event(&mv, &mut inv);
                }
                for b in &mut self.list_btns {
                    b.on_event(&mv, &mut inv);
                }
                self.redraw();
                return self.after_panel();
            }
            WindowEvent::KeyboardInput { event: kev, .. }
                if kev.state == ElementState::Pressed
                    && matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) =>
            {
                self.close();
                return ColorsAction::None;
            }
            WindowEvent::RedrawRequested => return ColorsAction::Paint,
            _ => {}
        }
        let Some(ie) = self.to_input(ev) else {
            return ColorsAction::None;
        };
        let mut inv = Invalidations::default();
        if let InputEvent::MouseDown { x, y, .. } = ie {
            let p = Point { x, y };
            // ★ T-119 목록 모드: 색 행 클릭 = 그 행 선택(패널이 그 색을 보인다).
            if self.list.is_some() {
                if let Some(i) = self.list_rows.iter().position(|r| r.contains(p)) {
                    let _ = self.list_op(&format!("sel:{i}"), th);
                    return ColorsAction::None;
                }
            }
            let targets = self.targets();
            for (i, r) in self.list_rows.iter().enumerate().take(targets.len()) {
                if r.contains(p) {
                    let tg = targets[i];
                    if tg != self.target {
                        self.target = tg;
                        let v = self.value_of(th, tg);
                        self.panel.set_value(&v);
                    }
                    self.panel.set_focused(false);
                    self.redraw();
                    return ColorsAction::None;
                }
            }
        }
        // 색 패널 + 버튼.
        self.panel.on_event(&ie, &mut inv);
        for b in [&mut self.preview, &mut self.reset_btn, &mut self.close_btn] {
            b.on_event(&ie, &mut inv);
        }
        // 목록 버튼: 누름은 커서 아래 버튼에만(마우스 라우팅 규칙) · 뗌은 넓게.
        let down_at = match ie {
            InputEvent::MouseDown { x, y, .. } => Some(Point { x, y }),
            _ => None,
        };
        for b in &mut self.list_btns {
            if down_at.is_none_or(|p| b.bounds().contains(p)) {
                b.on_event(&ie, &mut inv);
            }
        }
        // 포커스 규칙(CLAUDE.md §3): 마지막으로 누른 곳만 링 — 패널 밖 클릭이면 패널 링 해제 · 버튼은 누른 하나만.
        if let InputEvent::MouseDown { x, y, .. } = ie {
            let p = Point { x, y };
            let hit = [
                self.preview.bounds().contains(p),
                self.reset_btn.bounds().contains(p),
                self.close_btn.bounds().contains(p),
            ];
            self.preview.set_focused(hit[0]);
            self.reset_btn.set_focused(hit[1]);
            self.close_btn.set_focused(hit[2]);
            let mut list_hit = false;
            for b in &mut self.list_btns {
                let h = b.bounds().contains(p);
                b.set_focused(h);
                list_hit |= h;
            }
            if list_hit || hit.iter().any(|&h| h) {
                self.panel.set_focused(false);
            }
        }
        // ★ T-119: 목록 버튼 클릭 = 조작 하나(값이 바뀌면 호스트에 목록 전체를 보고).
        for (i, op) in ["add", "del", "up", "down"].into_iter().enumerate() {
            if self.list_btns[i].take_clicked() {
                return self.list_op(op, th);
            }
        }
        let _ = self.preview.take_clicked(); // 미리보기 = 눌러 보는 용도(동작 없음)
        if self.reset_btn.take_clicked() {
            if let Some((k, _)) = self.list {
                // 목록 모드의 초기화 = 설정을 비운다(= 테마의 괄호 색) → 창도 그 목록으로.
                self.set_list_mode(k, "", th);
                return ColorsAction::Reset;
            }
            if let Some((_, v)) = &mut self.key_mode {
                *v = None;
            } else {
                self.values = [None, None];
            }
            let v = self.value_of(th, self.target);
            self.panel.set_value(&v);
            self.redraw();
            return ColorsAction::Reset;
        }
        if self.close_btn.take_clicked() {
            self.close();
            return ColorsAction::None;
        }
        self.redraw();
        self.after_panel()
    }

    /// 패널 변경 수거 → 호스트에 보고(실시간 적용).
    fn after_panel(&mut self) -> ColorsAction {
        if let Some(hex) = self.panel.take_changed() {
            if let Some((key, l)) = &mut self.list {
                // 목록 모드: 패널 색 = 선택한 행의 색 → 값은 목록 전체(쉼표로 이은 글).
                if !l.set_selected(&hex) {
                    return ColorsAction::None;
                }
                return ColorsAction::Changed {
                    target: ColorTarget::Key(key),
                    hex: l.join(),
                };
            }
            if let Some((_, v)) = &mut self.key_mode {
                *v = Some(hex.clone());
            } else {
                self.values[self.target.idx()] = Some(hex.clone());
            }
            return ColorsAction::Changed {
                target: self.target,
                hex,
            };
        }
        ColorsAction::None
    }

    /// 키 모드의 키(기본 모드 = None).
    pub(crate) fn key_mode_key(&self) -> Option<&'static str> {
        self.key_mode
            .as_ref()
            .map(|(k, _)| *k)
            .or(self.list.as_ref().map(|(k, _)| *k))
    }

    /// 최근 색(호스트 영속화용).
    pub(crate) fn recent(&self) -> Vec<u32> {
        self.panel.recent().to_vec()
    }

    pub(crate) fn paint(&mut self, ui: &Font, th: &Theme, font_px: f32) {
        // surface 가변 대여 전에 값 문자열을 만들어 둔다.
        let targets = self.targets();
        let hexes: Vec<String> = targets.iter().map(|&tg| self.value_of(th, tg)).collect();
        let list_view: Option<(Vec<String>, usize)> =
            self.list.as_ref().map(|(_, l)| (l.colors.clone(), l.sel));
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
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs::with_base(font_px);
            let mut dc = RasterCtx::new(&mut gfx, ui, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.window_bg);
            dc.select_font(FontSlot::Base, false);
            // 왼쪽 목록 제목 + 대상 행(칩 + 라벨).
            dc.text(
                pad,
                pad,
                Rect::new(0, 0, wi, hi),
                t(Msg::LblColorTargets),
                th.text_dim,
            );
            let th_txt = dc.text_height();
            for (i, tg) in targets.iter().enumerate() {
                let r = self.list_rows[i];
                if *tg == self.target {
                    dc.fill_round_rect(r, (5.0 * s).round() as i32, th.sel_bg);
                }
                let chip = (18.0 * s).round() as i32;
                let cr = Rect::new(
                    r.x + (6.0 * s).round() as i32,
                    r.y + (r.h - chip) / 2,
                    chip,
                    chip,
                );
                if let Some(rgba) = rgba_from_hex(&hexes[i]) {
                    dc.fill_round_rect(cr, 3, th.panel_bg);
                    dc.fill_round_rect_alpha(
                        cr,
                        3,
                        nexa_ctl::theme::Color(rgba >> 8),
                        f32::from(rgba as u8) / 255.0,
                    );
                }
                dc.stroke_round_rect(cr, 3, th.border, 1.0);
                dc.text(
                    cr.right() + (8.0 * s).round() as i32,
                    r.y + (r.h - th_txt) / 2,
                    r,
                    t(tg.label()),
                    th.text,
                );
            }
            // ★ T-119 목록 모드: 색 행(칩 + `#RRGGBB`) · 선택 행 강조 · 그 아래 [+][−][▲][▼].
            if let Some((colors, sel)) = &list_view {
                for (i, hex) in colors.iter().enumerate() {
                    let Some(r) = self.list_rows.get(i).copied() else {
                        break;
                    };
                    if i == *sel {
                        dc.fill_round_rect(r, (4.0 * s).round() as i32, th.sel_bg);
                    }
                    let chip = (14.0 * s).round() as i32;
                    let cr = Rect::new(
                        r.x + (6.0 * s).round() as i32,
                        r.y + (r.h - chip) / 2,
                        chip,
                        chip,
                    );
                    if let Some(rgba) = rgba_from_hex(hex) {
                        dc.fill_round_rect(cr, 3, nexa_ctl::theme::Color(rgba >> 8));
                    }
                    dc.stroke_round_rect(cr, 3, th.border, 1.0);
                    dc.text(
                        cr.right() + (8.0 * s).round() as i32,
                        r.y + (r.h - th_txt) / 2,
                        r,
                        hex,
                        th.text,
                    );
                }
                for b in &self.list_btns {
                    b.paint(&mut dc, th);
                }
            }
            // 안내(대상 모드만 — 목록 모드는 그 자리에 버튼 줄이 있다).
            if let Some(last) = targets
                .len()
                .checked_sub(1)
                .and_then(|i| self.list_rows.get(i))
            {
                let hint_y = last.bottom() + (10.0 * s).round() as i32;
                let hint_clip = Rect::new(pad, hint_y, (LIST_W * s).round() as i32, hi - hint_y);
                for (i, line) in t(Msg::ColorsHint).split('\n').enumerate() {
                    dc.text(
                        pad,
                        hint_y + i as i32 * th_txt,
                        hint_clip,
                        line,
                        th.text_dim,
                    );
                }
            }
            // 색 패널 + 버튼.
            self.panel.paint(&mut dc, th);
            self.preview.paint(&mut dc, th);
            self.reset_btn.paint(&mut dc, th);
            self.close_btn.paint(&mut dc, th);
        }
        let _ = buf.present();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ★ T-119: 색 목록 모델 — 읽기(형식 오류 건너뜀 · 8자리 = 알파 버림 · 비면 기본 · 최대 수) · 추가/삭제/이동/바꾸기 · 왕복.
    #[test]
    fn color_list_model_round_trips_and_edits() {
        let default = vec!["#111111".to_string(), "#222222".to_string()];
        let l = ColorList::parse("", &default);
        assert_eq!(l.join(), "#111111,#222222");
        let mut l = ColorList::parse(" ff0000 , nope, #00ff0080,#12345", &default);
        assert_eq!(
            l.join(),
            "#FF0000,#00FF00",
            "형식 오류는 건너뛰고 알파는 버린다"
        );
        assert_eq!(ColorList::parse(&l.join(), &default), l, "왕복");
        // 추가 = 선택한 색을 그 아래에 · 선택이 새 행으로.
        assert!(l.add());
        assert_eq!((l.join().as_str(), l.sel), ("#FF0000,#FF0000,#00FF00", 1));
        assert!(l.set_selected("#0000ffcc") && !l.set_selected("#0000FF") && !l.set_selected("zz"));
        assert_eq!(l.join(), "#FF0000,#0000FF,#00FF00");
        // 이동: 위·아래 · 끝에서는 그대로.
        assert!(l.move_by(true) && !l.move_by(true));
        assert_eq!((l.join().as_str(), l.sel), ("#0000FF,#FF0000,#00FF00", 0));
        assert!(l.move_by(false) && l.move_by(false) && !l.move_by(false));
        assert_eq!((l.join().as_str(), l.sel), ("#FF0000,#00FF00,#0000FF", 2));
        // 삭제: 선택이 범위 안으로 · 하나 남으면 더 못 뺀다.
        assert!(l.del() && l.sel == 1 && l.del() && !l.del());
        assert_eq!(l.join(), "#FF0000");
        // 선택: 범위 밖·같은 행은 변화 없음.
        assert!(!l.select(0) && !l.select(9));
        // 최대 수: 읽을 때 자르고 · 꽉 차면 추가 불가.
        let many = ["#010101"; 12].join(",");
        let mut m = ColorList::parse(&many, &default);
        assert_eq!(m.colors.len(), LIST_MAX);
        assert!(!m.add());
        assert!(
            is_color_list_key("ext.rainbow_pairs.colors") && !is_color_list_key("ui.hover_color")
        );
    }

    /// 창 없이도 목록 조작이 호스트 보고(값 = 쉼표로 이은 목록)를 낸다 · 선택만 바꾸면 보고 없음 · 초기화 = 테마 기본.
    #[test]
    fn list_mode_ops_report_joined_value() {
        let th = Theme::light();
        let mut w = ColorsWin::new(None, None, &[]);
        w.set_list_mode("ext.rainbow_pairs.colors", "#FF0000,#00FF00", &th);
        assert!(w
            .dump()
            .starts_with("mode=list key=ext.rainbow_pairs.colors sel=0 n=2"));
        assert_eq!(w.key_mode_key(), Some("ext.rainbow_pairs.colors"));
        assert_eq!(w.list_op("sel:1", &th), ColorsAction::None);
        match w.list_op("set:#0000FF", &th) {
            ColorsAction::Changed { target, hex } => {
                assert_eq!(target, ColorTarget::Key("ext.rainbow_pairs.colors"));
                assert_eq!(hex, "#FF0000,#0000FF");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            w.list_op("add", &th),
            ColorsAction::Changed { .. }
        ));
        assert_eq!(w.list_op("nope", &th), ColorsAction::None);
        // 빈 값 = 테마의 괄호 색 6개.
        w.set_list_mode("ext.rainbow_pairs.colors", "", &th);
        assert!(w.dump().contains(" n=6 "), "{}", w.dump());
        // 다른 모드로 가면 목록 모드는 끝난다.
        w.set_default_mode(&th);
        assert_eq!(w.dump(), "mode=default\n");
        assert_eq!(w.list_op("add", &th), ColorsAction::None);
    }
}
