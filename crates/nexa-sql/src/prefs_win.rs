//! 환경 설정 창(T-39 · 사용자 09-15) — VS Code/DBeaver식: **검색** · 왼쪽 **트리**(그룹 ▸ 카테고리 = `CATEGORY_TREE`) ·
//! 오른쪽 **설정 카드 목록**(스크롤) — 종류별 컨트롤: Bool = 스위치 · Choice/Lang = 콤보 · Int/Text = 입력란(즉시 검증) ·
//! 색(`ui.*_color`) = 입력란 + [선택…](색 설정 창) · 단축키(`key.*`) = 입력란 + [캡처…](단축키 창).
//! 기본값과 다른 카드는 왼쪽 강조 막대 + [초기화]. 값은 바꾸는 즉시 호스트가 저장·반영(`PrefsAction::Changed`).
//! 이 창은 레지스트리 스냅샷(`refresh`)만 가진다 — 설정 파일·적용은 호스트 몫.

use nexa_ctl::controls::Switch;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::tokens::{hover_alpha, FadeSpeed, IntentFade};
use nexa_ctl::{
    Button, Combo, ComboControl, ComboItem, Control, InputEvent, Invalidations, Key as CtlKey,
    LabelSide, ScrollBars, TextBox, TreeControl, TreeModel, TreeNode, TreeView, Widget,
};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Lang, Msg};
use nsql_settings::{Entry, SettingKind, Settings, CATEGORY_TREE};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 창이 호스트에 요청하는 것.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PrefsAction {
    None,
    Paint,
    /// 값 변경(호스트가 정규화·저장·반영).
    Changed {
        key: String,
        value: String,
    },
    /// 기본값으로.
    Reset(String),
    OpenColors,
    OpenKeys,
    /// settings.json으로 편집(호스트가 내보내고 열고 감시한다).
    EditJson,
}

const PAD: f32 = 12.0;
const TREE_W: f32 = 230.0;
/// 스플리터 손잡이 폭(논리 px) — 왼쪽(검색+트리)과 카드 사이 · 드래그로 폭 조절 · hover 시 서서히 진해짐(clip과 같은 부품 `IntentFade`).
const SPLIT_W: f32 = 6.0;
const LEFT_MIN: f32 = 160.0;
const RIGHT_MIN: f32 = 320.0;
const SEARCH_H: f32 = 30.0;
const CTL_H: f32 = 28.0;
const CARD_GAP: f32 = 10.0;

enum CardCtl {
    Bool(Switch),
    Choice(Box<Combo>),
    Text(Box<TextBox>),
}

struct Card {
    entry: &'static Entry,
    value: String,
    modified: bool,
    ctl: CardCtl,
    reset: Button,
    /// 색·단축키 보조 버튼(선택…/캡처…).
    aux: Option<Button>,
    /// 마지막 페인트에서 정해진 카드 사각형(히트 테스트).
    rect: Rect,
    /// 검색 모드에서 카테고리 표시.
    show_cat: bool,
    error: Option<String>,
}

/// 스냅샷 한 줄.
#[derive(Clone)]
struct Snap {
    entry: &'static Entry,
    value: String,
    modified: bool,
}

pub(crate) struct PrefsWin {
    window: Option<Rc<Window>>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    search: TextBox,
    tree: TreeView,
    advanced: Switch,
    json_btn: Button,
    close_btn: Button,
    cards: Vec<Card>,
    /// 카드 영역·스크롤.
    list: Rect,
    scroll: i32,
    content_h: i32,
    bars: ScrollBars,
    /// 스냅샷(전체 · 비노출 포함) — `refresh`로 갱신.
    snap: Vec<Snap>,
    /// 현재 선택(그룹 index, 카테고리 index) · 검색어.
    sel: (usize, Option<usize>),
    last_tree_row: usize,
    query: String,
    /// 왼쪽 열 폭(논리 px) · 드래그 중(시작 x, 시작 폭) · 손잡이 hover 페이드 · 손잡이 사각형.
    left_w: f32,
    split_drag: Option<(i32, f32)>,
    split_fade: IntentFade,
    split_rect: Rect,
}

fn is_color_key(k: &str) -> bool {
    k.ends_with("_color")
}

fn is_key_key(k: &str) -> bool {
    k.starts_with("key.")
}

impl PrefsWin {
    pub(crate) fn new() -> Self {
        let mut roots: Vec<TreeNode> = Vec::new();
        for (g, cats) in CATEGORY_TREE {
            let kids: Vec<TreeNode> = cats.iter().map(|c| TreeNode::leaf(t(*c))).collect();
            roots.push(TreeNode::branch(t(*g), kids));
        }
        let mut model = TreeModel::new(roots);
        for gi in 0..CATEGORY_TREE.len() {
            model.set_expanded(&[gi], true);
        }
        PrefsWin {
            window: None,
            ctx: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            search: TextBox::new(t(Msg::PhSearchSettings)),
            tree: TreeView::new(model),
            advanced: Switch::new(t(Msg::LblAdvanced), false).with_label_side(LabelSide::Left),
            json_btn: Button::new(t(Msg::BtnEditJson)),
            close_btn: Button::new(t(Msg::BtnClose)),
            cards: Vec::new(),
            list: Rect::default(),
            scroll: 0,
            content_h: 0,
            bars: ScrollBars::new(),
            snap: Vec::new(),
            sel: (0, Some(0)),
            last_tree_row: 1,
            query: String::new(),
            left_w: TREE_W,
            split_drag: None,
            split_fade: IntentFade::with_speed(FadeSpeed::Fast),
            split_rect: Rect::default(),
        }
    }

    /// 레지스트리 스냅샷 갱신(열 때 · 값이 바뀔 때). 카드는 값만 갱신(입력 중인 상자는 건드리지 않음).
    pub(crate) fn refresh(&mut self, s: &Settings) {
        self.snap = s
            .list()
            .into_iter()
            .map(|(e, v, m)| Snap {
                entry: e,
                value: v.to_string(),
                modified: m,
            })
            .collect();
        if self.cards.is_empty() {
            self.rebuild_cards();
            return;
        }
        for c in &mut self.cards {
            let Some(sn) = self.snap.iter().find(|x| x.entry.key == c.entry.key) else {
                continue;
            };
            c.modified = sn.modified;
            if c.value != sn.value {
                c.value = sn.value.clone();
                c.error = None;
                match &mut c.ctl {
                    CardCtl::Bool(sw) => sw.set_on(sn.value == "on"),
                    CardCtl::Choice(cb) => cb.select_value(&sn.value),
                    CardCtl::Text(tb) => {
                        if !tb.is_focused() {
                            tb.set_text(&sn.value);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn set_error(&mut self, key: &str, e: String) {
        if let Some(c) = self.cards.iter_mut().find(|c| c.entry.key == key) {
            c.error = Some(e);
        }
    }

    /// 지금 선택/검색에 맞는 카드 목록을 만든다.
    fn rebuild_cards(&mut self) {
        let adv = self.advanced.is_on();
        let q = self.query.trim().to_lowercase();
        let mut chosen: Vec<Snap> = Vec::new();
        if !q.is_empty() {
            for sn in &self.snap {
                if !adv && nsql_settings::is_hidden(sn.entry.key) {
                    continue;
                }
                let hay = format!(
                    "{} {} {} {}",
                    sn.entry.key,
                    t(sn.entry.label),
                    t(sn.entry.desc),
                    t(sn.entry.cat)
                )
                .to_lowercase();
                if hay.contains(&q) {
                    chosen.push(sn.clone());
                }
            }
            chosen.sort_by_key(|s| nsql_settings::tree_order(s.entry.cat));
        } else {
            let (gi, ci) = self.sel;
            let cats: Vec<Msg> = match CATEGORY_TREE.get(gi) {
                Some((_, cats)) => match ci {
                    Some(c) => cats.get(c).copied().into_iter().collect(),
                    None => cats.to_vec(),
                },
                None => Vec::new(),
            };
            for sn in &self.snap {
                if cats.contains(&sn.entry.cat) && (adv || !nsql_settings::is_hidden(sn.entry.key))
                {
                    chosen.push(sn.clone());
                }
            }
        }
        let show_cat = !q.is_empty() || self.sel.1.is_none();
        self.cards = chosen
            .into_iter()
            .map(|sn| {
                let ctl = match sn.entry.kind {
                    SettingKind::Bool => CardCtl::Bool(
                        Switch::new("", sn.value == "on").with_label_side(LabelSide::None),
                    ),
                    SettingKind::Choice(opts) => {
                        let items: Vec<ComboItem> = opts
                            .iter()
                            .map(|(v, m)| ComboItem::new(*v, t(*m)))
                            .collect();
                        let idx = opts.iter().position(|(v, _)| *v == sn.value).unwrap_or(0);
                        CardCtl::Choice(Box::new(Combo::new(items, idx)))
                    }
                    SettingKind::Lang => {
                        let items: Vec<ComboItem> = Lang::ALL
                            .iter()
                            .map(|l| ComboItem::new(l.code(), l.endonym()))
                            .collect();
                        let idx = Lang::ALL
                            .iter()
                            .position(|l| l.code() == sn.value)
                            .unwrap_or(0);
                        CardCtl::Choice(Box::new(Combo::new(items, idx)))
                    }
                    SettingKind::Int { .. } | SettingKind::Text => {
                        CardCtl::Text(Box::new(TextBox::new("").with_text(&sn.value)))
                    }
                };
                let aux = if is_color_key(sn.entry.key) {
                    Some(Button::new(t(Msg::BtnPick)))
                } else if is_key_key(sn.entry.key) {
                    Some(Button::new(t(Msg::BtnCapture)))
                } else {
                    None
                };
                Card {
                    entry: sn.entry,
                    value: sn.value,
                    modified: sn.modified,
                    ctl,
                    reset: Button::new(t(Msg::BtnReset)),
                    aux,
                    rect: Rect::default(),
                    show_cat,
                    error: None,
                }
            })
            .collect();
        self.scroll = 0;
        self.content_h = 0;
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
        let mut any = self.bars.tick(now_ms)
            | self.split_fade.tick(now_ms)
            | self.tree.tick(now_ms)
            | self.search.tick(now_ms)
            | self.close_btn.tick(now_ms)
            | self.json_btn.tick(now_ms);
        for c in &mut self.cards {
            any |= c.reset.tick(now_ms);
            if let Some(b) = &mut c.aux {
                any |= b.tick(now_ms);
            }
            match &mut c.ctl {
                CardCtl::Bool(_) => {}
                CardCtl::Choice(cb) => any |= cb.tick_hover(now_ms),
                CardCtl::Text(tb) => any |= tb.tick(now_ms),
            }
        }
        any
    }

    pub(crate) fn animating(&self) -> bool {
        self.window.is_some()
            && (self.bars.is_visible()
                || self.split_fade.is_animating()
                || self.search.is_animating()
                || self.close_btn.is_animating()
                || self.json_btn.is_animating()
                || self.cards.iter().any(|c| {
                    c.reset.is_animating()
                        || c.aux.as_ref().is_some_and(|b| b.is_animating())
                        || match &c.ctl {
                            CardCtl::Bool(_) => false,
                            CardCtl::Choice(cb) => cb.hover_animating(),
                            CardCtl::Text(tb) => tb.is_animating(),
                        }
                }))
    }

    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        over: Option<(i32, i32, u32, u32)>,
        owner: Option<&Window>,
    ) {
        if let Some(w) = &self.window {
            w.focus_window();
            return;
        }
        let (lw, lh) = (920.0, 640.0);
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinPreferences)))
            .with_theme(theme)
            .with_resizable(true)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
        if let Some((x, y, w, h)) = over {
            let cx = x + (w as i32 - lw as i32) / 2;
            let cy = y + (h as i32 - lh as i32) / 2;
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(cx.max(0), cy.max(0)));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(attrs) else {
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
        self.window = Some(win);
        self.rebuild_cards();
        self.layout();
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.ctx = None;
        self.window = None;
        self.cards.clear();
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    fn layout(&mut self) {
        let Some(win) = &self.window else { return };
        let size = win.inner_size();
        let (w, h) = (size.width as i32, size.height as i32);
        let s = self.scale;
        let pad = self.s(PAD);
        let mut inv = Invalidations::default();
        let bottom_h = self.s(CTL_H);
        let by = h - pad - bottom_h;
        // 세로 우선 분할(사용자 09-15): 왼쪽 열 = 검색(위) + 트리(아래) · 스플리터 · 오른쪽 = 카드(맨 위부터).
        let max_left = ((w - pad * 2 - self.s(SPLIT_W)) as f32 / s - RIGHT_MIN).max(LEFT_MIN);
        self.left_w = self.left_w.clamp(LEFT_MIN, max_left.max(LEFT_MIN));
        let lw = self.s(self.left_w);
        let top = pad;
        self.search.set_scale(s);
        self.search
            .set_bounds(Rect::new(pad, top, lw, self.s(SEARCH_H)), &mut inv);
        let tree_top = top + self.s(SEARCH_H) + self.s(8.0);
        self.tree.set_scale(s);
        self.tree
            .set_bounds(Rect::new(pad, tree_top, lw, by - pad - tree_top), &mut inv);
        self.split_rect = Rect::new(pad + lw, top, self.s(SPLIT_W), by - pad - top);
        let lx = self.split_rect.right() + pad / 2;
        self.list = Rect::new(lx, top, w - lx - pad, by - pad - top);
        // 아래 줄: 고급 스위치 · 닫기
        self.advanced.set_scale(s);
        self.advanced
            .set_bounds(Rect::new(pad, by, self.s(160.0), bottom_h), &mut inv);
        self.json_btn.set_scale(s);
        self.json_btn.set_bounds(
            Rect::new(
                w - pad - self.s(96.0) * 2 - self.s(8.0),
                by,
                self.s(96.0),
                bottom_h,
            ),
            &mut inv,
        );
        self.close_btn.set_scale(s);
        self.close_btn.set_bounds(
            Rect::new(w - pad - self.s(96.0), by, self.s(96.0), bottom_h),
            &mut inv,
        );
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let max = (self.content_h - self.list.h).max(0);
        self.scroll = self.scroll.clamp(0, max);
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

    /// 포커스 텍스트박스(IME).
    fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if self.search.is_focused() {
            return Some(&mut self.search);
        }
        for c in &mut self.cards {
            if let CardCtl::Text(tb) = &mut c.ctl {
                if tb.is_focused() {
                    return Some(&mut **tb);
                }
            }
        }
        None
    }

    fn any_combo_open(&self) -> bool {
        self.cards.iter().any(|c| match &c.ctl {
            CardCtl::Choice(cb) => cb.is_open(),
            _ => false,
        })
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> PrefsAction {
        match ev {
            WindowEvent::CloseRequested => {
                self.close();
                return PrefsAction::None;
            }
            WindowEvent::Resized(_) => {
                self.layout();
                self.redraw();
                return PrefsAction::None;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.layout();
                self.redraw();
                return PrefsAction::None;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().shift_key();
                return PrefsAction::None;
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
                return self.after_edit();
            }
            WindowEvent::KeyboardInput { event: kev, .. }
                if kev.state == ElementState::Pressed
                    && matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) =>
            {
                if self.any_combo_open() {
                    // 콤보가 열려 있으면 Esc는 콤보가 받는다.
                } else {
                    self.close();
                    return PrefsAction::None;
                }
            }
            WindowEvent::RedrawRequested => return PrefsAction::Paint,
            _ => {}
        }
        if let WindowEvent::CursorMoved { position, .. } = ev {
            self.cursor = (position.x as i32, position.y as i32);
        }
        let Some(ie) = self.to_input(ev) else {
            return PrefsAction::None;
        };
        let mut inv = Invalidations::default();
        // 스플리터: hover = 서서히 진해짐(IntentFade · 마지막 위치만) · 드래그 = 왼쪽 열 폭.
        match ie {
            InputEvent::MouseMove { x, y } => {
                if let Some((x0, w0)) = self.split_drag {
                    self.left_w = w0 + (x - x0) as f32 / self.scale;
                    self.layout();
                    self.redraw();
                    return PrefsAction::None;
                }
                let over = self.split_rect.contains(Point { x, y }) && !self.any_combo_open();
                self.split_fade.set(over.then_some(0));
                if let Some(w) = &self.window {
                    w.set_cursor(if over {
                        winit::window::CursorIcon::ColResize
                    } else {
                        winit::window::CursorIcon::Default
                    });
                }
            }
            InputEvent::MouseDown { x, y, .. }
                if self.split_rect.contains(Point { x, y }) && !self.any_combo_open() =>
            {
                self.split_drag = Some((x, self.left_w));
                self.split_fade.jump(Some(0));
                return PrefsAction::None;
            }
            InputEvent::MouseUp { .. } if self.split_drag.is_some() => {
                self.split_drag = None;
                self.redraw();
                return PrefsAction::None;
            }
            _ => {}
        }
        // 열린 콤보 = 모달(바깥 클릭은 닫고 통과 — 콤보 자체가 처리).
        if self.any_combo_open() {
            for c in &mut self.cards {
                if let CardCtl::Choice(cb) = &mut c.ctl {
                    if cb.is_open() {
                        cb.on_event(&ie, &mut inv);
                    }
                }
            }
            let a = self.collect_changes();
            self.redraw();
            if !matches!(ie, InputEvent::MouseDown { .. }) || self.any_combo_open() {
                return a;
            }
        }
        let p = Point {
            x: self.cursor.0,
            y: self.cursor.1,
        };
        let is_mouse = matches!(
            ie,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
        );
        // 포커스 규칙: MouseDown마다 누른 곳 하나만.
        if let InputEvent::MouseDown { .. } = ie {
            let in_search = self.search.bounds().contains(p);
            self.search.set_focused(in_search);
            self.tree.set_focused(self.tree.bounds().contains(p));
            self.close_btn
                .set_focused(self.close_btn.bounds().contains(p));
            self.json_btn
                .set_focused(self.json_btn.bounds().contains(p));
            for c in &mut self.cards {
                match &mut c.ctl {
                    CardCtl::Text(tb) => tb.set_focused(tb.bounds().contains(p)),
                    CardCtl::Choice(cb) => cb.set_focused(cb.bounds().contains(p)),
                    CardCtl::Bool(sw) => sw.set_focused(false),
                }
                c.reset.set_focused(c.reset.bounds().contains(p));
                if let Some(b) = &mut c.aux {
                    b.set_focused(b.bounds().contains(p));
                }
            }
        }
        // 카드 영역 스크롤(휠 · 바)
        let is_wheel = matches!(ie, InputEvent::Wheel { .. } | InputEvent::HWheel { .. });
        if self.list.contains(p) || (!is_mouse && !is_wheel) {
            let (_, ny, consumed) = self.bars.on_event(
                &ie,
                self.list,
                self.list.w,
                self.content_h.max(self.list.h),
                0,
                self.scroll,
                self.scale,
            );
            if ny != self.scroll {
                self.scroll = ny;
                self.clamp_scroll();
            }
            if consumed || is_wheel {
                self.redraw();
                return PrefsAction::None;
            }
        }
        // 검색·트리·하단
        // ★ 마우스 사건은 커서 아래 컨트롤에만(CLAUDE.md §3 라우팅 규칙) · 키는 포커스 컨트롤에만.
        let route = |r: Rect, focused: bool| -> bool {
            if is_mouse || is_wheel {
                r.contains(p)
            } else {
                focused
            }
        };
        if route(self.search.bounds(), self.search.is_focused()) {
            self.search.on_event(&ie, &mut inv);
        }
        if route(self.tree.bounds(), self.tree.is_focused()) {
            self.tree.on_event(&ie, &mut inv);
        }
        if route(self.advanced.bounds(), false) {
            self.advanced.on_event(&ie, &mut inv);
        }
        if route(self.close_btn.bounds(), self.close_btn.is_focused())
            || matches!(ie, InputEvent::MouseMove { .. })
        {
            self.close_btn.on_event(&ie, &mut inv);
        }
        if route(self.json_btn.bounds(), self.json_btn.is_focused())
            || matches!(ie, InputEvent::MouseMove { .. })
        {
            self.json_btn.on_event(&ie, &mut inv);
        }
        if let Some(q) = self.search.take_changed() {
            self.query = q;
            self.rebuild_cards();
            self.redraw();
            return PrefsAction::None;
        }
        if self.advanced.take_toggled().is_some() {
            self.rebuild_cards();
            self.redraw();
            return PrefsAction::None;
        }
        if self.close_btn.take_clicked() {
            self.close();
            return PrefsAction::None;
        }
        if self.json_btn.take_clicked() {
            return PrefsAction::EditJson;
        }
        // 트리 선택 변화 → 카테고리 전환(검색 중이면 검색어를 비운다).
        let row = self.tree.selected_row();
        if row != self.last_tree_row {
            self.last_tree_row = row;
            let rows = self.tree.model().flatten();
            if let Some(r) = rows.get(row) {
                let gi = r.path.first().copied().unwrap_or(0);
                let ci = r.path.get(1).copied();
                self.sel = (gi, ci);
                if !self.query.is_empty() {
                    self.query.clear();
                    self.search.set_text("");
                }
                self.rebuild_cards();
                self.redraw();
                return PrefsAction::None;
            }
        }
        // 카드 컨트롤(보이는 것만)
        if self.list.contains(p) || !is_mouse {
            for c in &mut self.cards {
                if c.rect.h == 0 {
                    continue;
                }
                match &mut c.ctl {
                    CardCtl::Bool(sw) => sw.on_event(&ie, &mut inv),
                    CardCtl::Choice(cb) => cb.on_event(&ie, &mut inv),
                    CardCtl::Text(tb) => tb.on_event(&ie, &mut inv),
                }
                c.reset.on_event(&ie, &mut inv);
                if let Some(b) = &mut c.aux {
                    b.on_event(&ie, &mut inv);
                }
            }
        }
        let a = self.collect_changes();
        self.redraw();
        a
    }

    fn after_edit(&mut self) -> PrefsAction {
        self.collect_changes()
    }

    /// 컨트롤 변화 수거 → 첫 변경만 보고(한 이벤트에 하나).
    fn collect_changes(&mut self) -> PrefsAction {
        for c in &mut self.cards {
            let key = c.entry.key.to_string();
            if c.reset.take_clicked() {
                return PrefsAction::Reset(key);
            }
            if let Some(b) = &mut c.aux {
                if b.take_clicked() {
                    return if is_color_key(&key) {
                        PrefsAction::OpenColors
                    } else {
                        PrefsAction::OpenKeys
                    };
                }
            }
            match &mut c.ctl {
                CardCtl::Bool(sw) => {
                    if let Some(on) = sw.take_toggled() {
                        return PrefsAction::Changed {
                            key,
                            value: if on { "on".into() } else { "off".into() },
                        };
                    }
                }
                CardCtl::Choice(cb) => {
                    if let Some(v) = cb.take_changed() {
                        return PrefsAction::Changed { key, value: v };
                    }
                }
                CardCtl::Text(tb) => {
                    if let Some(v) = tb.take_changed() {
                        // 즉시 검증 — 틀리면 카드에 표시만(저장 안 함).
                        if nsql_settings::normalize(c.entry.kind, &v).is_some()
                            || (v.trim().is_empty() && c.entry.default.is_empty())
                        {
                            c.error = None;
                            return PrefsAction::Changed { key, value: v };
                        }
                        c.error = Some(nsql_settings::allowed(c.entry.kind));
                    }
                }
            }
        }
        PrefsAction::None
    }

    /// 단어 단위 줄바꿈(설명).
    fn wrap(dc: &mut dyn DrawCtx, text: &str, w: i32) -> Vec<String> {
        let mut lines = Vec::new();
        for para in text.split('\n') {
            let mut cur = String::new();
            for word in para.split_whitespace() {
                let cand = if cur.is_empty() {
                    word.to_string()
                } else {
                    format!("{cur} {word}")
                };
                if dc.text_width(&cand) <= w || cur.is_empty() {
                    cur = cand;
                } else {
                    lines.push(std::mem::replace(&mut cur, word.to_string()));
                }
            }
            lines.push(cur);
        }
        lines
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
            let mut dc = RasterCtx::new(&mut gfx, ui, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.window_bg);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            // ── 카드 레이아웃(텍스트 폭 실측 · 스크롤 반영) → 컨트롤 bounds
            let card_w = list.w - (18.0 * s).round() as i32;
            let inner_pad = (10.0 * s).round() as i32;
            let ctl_h = (CTL_H * s).round() as i32;
            let gap = (CARD_GAP * s).round() as i32;
            let text_w = card_w - inner_pad * 2 - (120.0 * s).round() as i32;
            let mut y = list.y + gap - self.scroll;
            let mut inv = Invalidations::default();
            let mut desc_lines: Vec<Vec<String>> = Vec::with_capacity(self.cards.len());
            for c in &mut self.cards {
                let lines = Self::wrap(&mut dc, t(c.entry.desc), text_w);
                let extra = if c.error.is_some() { th_txt } else { 0 };
                let ch = inner_pad * 2
                    + th_txt
                    + (4.0 * s).round() as i32
                    + th_txt * lines.len() as i32
                    + (8.0 * s).round() as i32
                    + ctl_h
                    + extra;
                c.rect = Rect::new(list.x, y, card_w, ch);
                // 컨트롤 줄(카드 아래쪽): [컨트롤][보조][초기화]
                let cy = y + ch - inner_pad - ctl_h;
                let mut cx = list.x + inner_pad;
                let ctl_w = match &c.ctl {
                    CardCtl::Bool(_) => (56.0 * s).round() as i32,
                    CardCtl::Choice(_) => (260.0 * s).round() as i32,
                    CardCtl::Text(_) => (320.0 * s).round() as i32,
                };
                let visible = y + ch > list.y && y < list.bottom();
                let hidden = Rect::default();
                let r = if visible {
                    Rect::new(cx, cy, ctl_w, ctl_h)
                } else {
                    hidden
                };
                match &mut c.ctl {
                    CardCtl::Bool(sw) => {
                        sw.set_scale(s);
                        sw.set_bounds(r, &mut inv);
                    }
                    CardCtl::Choice(cb) => {
                        cb.set_scale(s);
                        cb.set_bounds(r, &mut inv);
                    }
                    CardCtl::Text(tb) => {
                        tb.set_scale(s);
                        tb.set_bounds(r, &mut inv);
                    }
                }
                cx += ctl_w + (8.0 * s).round() as i32;
                if let Some(b) = &mut c.aux {
                    b.set_scale(s);
                    let bw = (90.0 * s).round() as i32;
                    b.set_bounds(
                        if visible {
                            Rect::new(cx, cy, bw, ctl_h)
                        } else {
                            hidden
                        },
                        &mut inv,
                    );
                    cx += bw + (8.0 * s).round() as i32;
                }
                c.reset.set_scale(s);
                let rw = (90.0 * s).round() as i32;
                c.reset.set_bounds(
                    if visible && c.modified {
                        Rect::new(cx, cy, rw, ctl_h)
                    } else {
                        hidden
                    },
                    &mut inv,
                );
                if !visible {
                    c.rect.h = 0;
                }
                desc_lines.push(lines);
                y += ch + gap;
            }
            self.content_h = y + self.scroll - list.y;
            // ── 그리기: 검색 · 트리 · 카드(클립) · 하단
            self.search.paint(&mut dc, th);
            self.tree.paint(&mut dc, th);
            // 스플리터 — 가는 선 + hover/드래그 시 진해지는 손잡이.
            let sr = self.split_rect;
            let cx = sr.x + sr.w / 2;
            dc.fill_rect(Rect::new(cx, sr.y, 1, sr.h), th.border);
            let a = hover_alpha(self.split_drag.is_some(), self.split_fade.value(0));
            if a > 0.0 {
                dc.fill_rect_alpha(Rect::new(cx - 1, sr.y, 3, sr.h), th.accent, a.max(0.15));
            }
            dc.fill_rect(list, th.window_bg);
            for (c, lines) in self.cards.iter().zip(desc_lines.iter()) {
                if c.rect.h == 0 {
                    continue;
                }
                let r = c.rect;
                let clip = r.intersection(&list);
                if clip.h <= 0 {
                    continue;
                }
                dc.fill_round_rect(clip, (6.0 * s).round() as i32, th.panel_bg);
                if c.modified {
                    dc.fill_rect(
                        Rect::new(r.x, r.y, (3.0 * s).round() as i32, r.h).intersection(&list),
                        th.accent,
                    );
                }
                let tx = r.x + inner_pad;
                let mut ty = r.y + inner_pad;
                dc.select_font(FontSlot::Base, true);
                let label = if c.show_cat {
                    format!("{} › {}", t(c.entry.cat), t(c.entry.label))
                } else {
                    t(c.entry.label).to_string()
                };
                dc.text(tx, ty, clip, &label, th.text);
                dc.select_font(FontSlot::Base, false);
                let key_w = dc.text_width(c.entry.key);
                dc.text(
                    r.right() - inner_pad - key_w,
                    ty,
                    clip,
                    c.entry.key,
                    th.text_dim,
                );
                ty += th_txt + (4.0 * s).round() as i32;
                for l in lines {
                    dc.text(tx, ty, clip, l, th.text_dim);
                    ty += th_txt;
                }
                if let Some(e) = &c.error {
                    dc.text(tx, ty, clip, e, th.danger);
                }
                // 기본값 표시(컨트롤 오른쪽 끝)
                let dv = tf(Msg::LblDefaultValue, &[c.entry.default]);
                let dw = dc.text_width(&dv);
                let cy = r.bottom() - inner_pad - ctl_h;
                dc.text(
                    r.right() - inner_pad - dw,
                    cy + (ctl_h - th_txt) / 2,
                    clip,
                    &dv,
                    th.text_dim,
                );
                match &c.ctl {
                    CardCtl::Bool(sw) => sw.paint(&mut dc, th),
                    CardCtl::Choice(_) => {}
                    CardCtl::Text(tb) => tb.paint(&mut dc, th),
                }
                if let Some(b) = &c.aux {
                    b.paint(&mut dc, th);
                }
                if c.modified {
                    c.reset.paint(&mut dc, th);
                }
            }
            // 콤보는 드롭다운이 아래 카드를 덮어야 하므로 맨 뒤에.
            for c in &self.cards {
                if c.rect.h == 0 {
                    continue;
                }
                if let CardCtl::Choice(cb) = &c.ctl {
                    cb.paint(&mut dc, th);
                }
            }
            self.bars.paint(
                &mut dc,
                th,
                list,
                list.w,
                self.content_h.max(list.h),
                0,
                self.scroll,
                s,
            );
            // 하단
            self.advanced.paint(&mut dc, th);
            self.close_btn.paint(&mut dc, th);
            self.json_btn.paint(&mut dc, th);
            let _ = pad;
            self.search.paint_popup(&mut dc, th);
        }
        let _ = buf.present();
    }
}
