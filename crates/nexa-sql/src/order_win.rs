//! **순서/표시 편집 창**(사용자 10-04 "상태바 위치와 사용 여부를 설정 화면에서 · nexa-dir3 툴바 조정 화면 참고"): 설정 창의
//! [편집…]에서 여는 보조 창. 행 = 체크(표시) + 이름 · 선택한 행을 ▲▼ / Ctrl(⌘)+↑↓ / 끌기로 옮긴다 · Space = 표시 전환 ·
//! ★ **그룹 단위 이동**(사용자 10-04): 그룹(블록) 행 = 통째 이동 · 자식 행 = 그 그룹 안에서만 · 그룹 체크 = 통째 숨김(자식 체크 보존) ·
//! 잠긴 항목은 체크를 못 끈다 · Esc = 끌기 취소 → 닫기. 바꿀 때마다 [`OrderWinAction::Changed`]로 호스트에 알려 **즉시 적용·저장**
//! (확인/취소 없음 — nexa-dir3 DLG-073과 같은 규약). 무엇을 고치는지는 [`OrderSpec`] 어댑터가 정한다(지금 = 상태바 하나).

use nexa_ctl::controls::Button;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::order::OrderBlock;
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, Theme};
use nexa_ctl::{InputEvent, Invalidations, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 편집 대상 어댑터 — 설정 키마다 하나.
#[derive(Clone, Copy)]
pub(crate) struct OrderSpec {
    /// 설정 키(통지에 같이 돌려준다).
    pub key: &'static str,
    pub title: Msg,
    /// 설정값 → 블록(그룹) 목록.
    pub blocks: fn(&str) -> Vec<OrderBlock>,
    /// 목록 → 저장할 설정값(기본과 같으면 빈 문자열).
    pub to_setting: fn(&[OrderBlock]) -> String,
    /// (블록, 자식) → 라벨 · 자식 `None` = 그룹 행.
    pub label: fn(&str, Option<&str>) -> Msg,
    /// 체크를 끌 수 없는 블록.
    pub locked: &'static [&'static str],
    /// 자식의 순서를 바꿀 수 있는가(툴바 = 그룹 안 버튼 순서는 고정 → `false`).
    pub child_move: bool,
}

/// 툴팁 문구를 라벨로 쓸 때 끝의 단축키 괄호(`Run statement (Ctrl+Enter)`)를 뗀다 — 이름만 남긴다(단축키 표기는 OS마다 다르다).
pub(crate) fn without_shortcut(label: &str) -> &str {
    match label.rfind(" (") {
        Some(i) if label.ends_with(')') => &label[..i],
        _ => label,
    }
}

/// 화면 행 — (블록 index, 자식 index · `None` = 그룹/단일 블록 행).
type Row = (usize, Option<usize>);

/// 블록 목록 → 화면 행(블록 행 + 그 자식 행들).
pub(crate) fn rows_of(blocks: &[OrderBlock]) -> Vec<Row> {
    let mut out = Vec::new();
    for (bi, (_, _, items)) in blocks.iter().enumerate() {
        out.push((bi, None));
        out.extend((0..items.len()).map(|ci| (bi, Some(ci))));
    }
    out
}

pub(crate) enum OrderWinAction {
    Paint,
    /// 값이 바뀌었다(정규화한 설정값) — 호스트가 저장·즉시 반영.
    Changed {
        key: &'static str,
        value: String,
    },
    Close,
    None,
}

const PAD: f32 = 12.0;
const ROW_H: f32 = 26.0;
const BTN_H: f32 = 28.0;
const SIDE_W: f32 = 84.0;
const CHECK: f32 = 16.0;
const INDENT: f32 = 22.0;
const DRAG_THRESHOLD: i32 = 5;

/// 끌기 — 누른 자리에서 [`DRAG_THRESHOLD`]를 넘으면 시작 · 시작할 때의 목록을 들고 있다가 Esc면 되돌린다.
struct Drag {
    press_y: i32,
    active: bool,
    before: Vec<OrderBlock>,
}

pub(crate) struct OrderWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    primary: bool,
    spec: Option<OrderSpec>,
    blocks: Vec<OrderBlock>,
    /// 선택 행([`rows_of`]의 index).
    sel: usize,
    drag: Option<Drag>,
    /// 마지막 페인트의 목록 영역(히트 테스트).
    list: Rect,
    up: Button,
    down: Button,
    reset: Button,
}

/// 항목 하나를 `from` → `to`로 옮긴다(순수 · 범위 밖이면 그대로). 돌려주는 값 = 실제로 옮겼는가.
pub(crate) fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from == to || from >= items.len() || to >= items.len() {
        return false;
    }
    let it = items.remove(from);
    items.insert(to, it);
    true
}

impl OrderWin {
    pub(crate) fn new() -> Self {
        OrderWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (-1, -1),
            primary: false,
            spec: None,
            blocks: Vec::new(),
            sel: 0,
            drag: None,
            list: Rect::default(),
            up: Button::new("▲"),
            down: Button::new("▼"),
            reset: Button::new(t(Msg::OrdReset)),
        }
    }

    /// 편집 대상과 지금 값을 넣는다(열기 전 · 열려 있으면 내용만 바뀐다).
    pub(crate) fn set(&mut self, spec: OrderSpec, value: &str) {
        self.blocks = (spec.blocks)(value);
        self.spec = Some(spec);
        self.sel = self.sel.min(self.rows().len().saturating_sub(1));
        self.drag = None;
        self.reset.set_label(t(Msg::OrdReset));
        self.redraw();
    }

    /// 열기(모델리스 · 소유자 = 설정 창 또는 메인 창 — 그 위에 뜬다).
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
        let Some(spec) = self.spec else {
            return;
        };
        let rows = self.rows().len().max(1) as f64;
        let h = f64::from(PAD) * 3.0 + 24.0 + rows * f64::from(ROW_H) + 2.0;
        let Some(o) = crate::winhost::open_window(
            el,
            crate::winhost::OpenSpec {
                title: format!("Nexa SQL - {}", t(spec.title)),
                theme,
                near,
                dy: 60,
                owner,
                memo: None,
                default_size: (380.0, h),
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
        self.drag = None;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.window.is_some()
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

    fn rows(&self) -> Vec<Row> {
        rows_of(&self.blocks)
    }

    /// 지금 내용(진단·시험 덤프) — `key` + 행마다 `[x] id`(자식은 들여쓰기) · 선택 행 앞에 `>`.
    pub(crate) fn dump(&self) -> String {
        let mut s = format!("key {}\n", self.spec.map_or("-", |sp| sp.key));
        for (i, (bi, ci)) in self.rows().into_iter().enumerate() {
            let (id, vis) = match ci {
                None => (self.blocks[bi].0.as_str(), self.blocks[bi].1),
                Some(c) => (self.blocks[bi].2[c].0.as_str(), self.blocks[bi].2[c].1),
            };
            s.push_str(&format!(
                "{}{}[{}] {id}\n",
                if i == self.sel { ">" } else { " " },
                if ci.is_some() { "  " } else { "" },
                if vis { "x" } else { " " }
            ));
        }
        s
    }

    fn changed(&self) -> OrderWinAction {
        self.redraw();
        match self.spec {
            Some(sp) => OrderWinAction::Changed {
                key: sp.key,
                value: (sp.to_setting)(&self.blocks),
            },
            None => OrderWinAction::None,
        }
    }

    fn block_locked(&self, bi: usize) -> bool {
        match (self.spec, self.blocks.get(bi)) {
            (Some(sp), Some((id, _, _))) => sp.locked.contains(&id.as_str()),
            _ => true,
        }
    }

    /// 표시 전환(행 index) — 그룹 행 = 통째(잠긴 블록 = 거부) · 자식 행 = 그 칸만(그룹이 숨겨져 있으면 거부).
    pub(crate) fn toggle(&mut self, row: usize) -> OrderWinAction {
        let Some(&(bi, ci)) = self.rows().get(row) else {
            return OrderWinAction::None;
        };
        match ci {
            None => {
                if self.block_locked(bi) {
                    return OrderWinAction::None;
                }
                self.blocks[bi].1 = !self.blocks[bi].1;
            }
            Some(c) => {
                if !self.blocks[bi].1 {
                    return OrderWinAction::None;
                }
                let it = &mut self.blocks[bi].2[c];
                it.1 = !it.1;
            }
        }
        self.changed()
    }

    /// 선택 행을 `to` 행의 자리로(끌기) — 그룹 행 = 그 행이 속한 그룹의 자리로 통째 · 자식 행 = **같은 그룹 안**의 자리로만.
    fn move_sel_to_row(&mut self, to_row: usize) -> bool {
        let rows = self.rows();
        let (Some(&(bi, ci)), Some(&(tb, tc))) = (rows.get(self.sel), rows.get(to_row)) else {
            return false;
        };
        match ci {
            None => {
                if !move_item(&mut self.blocks, bi, tb) {
                    return false;
                }
                self.select_block(tb);
            }
            Some(_) if !self.spec.is_some_and(|sp| sp.child_move) => return false,
            Some(c) => {
                // 다른 그룹 위 = 자기 그룹의 가까운 끝으로.
                let n = self.blocks[bi].2.len();
                let to = match (tb.cmp(&bi), tc) {
                    (std::cmp::Ordering::Equal, Some(t)) => t,
                    (std::cmp::Ordering::Equal, None) | (std::cmp::Ordering::Less, _) => 0,
                    (std::cmp::Ordering::Greater, _) => n.saturating_sub(1),
                };
                if !move_item(&mut self.blocks[bi].2, c, to) {
                    return false;
                }
                self.select_child(bi, to);
            }
        }
        true
    }

    fn select_block(&mut self, bi: usize) {
        if let Some(r) = self.rows().iter().position(|r| *r == (bi, None)) {
            self.sel = r;
        }
    }

    fn select_child(&mut self, bi: usize, ci: usize) {
        if let Some(r) = self.rows().iter().position(|r| *r == (bi, Some(ci))) {
            self.sel = r;
        }
    }

    /// 선택 행을 위/아래로 한 칸 — 그룹 행 = 이웃 그룹과 통째 자리 바꿈 · 자식 행 = 그룹 안에서.
    pub(crate) fn move_sel(&mut self, down: bool) -> OrderWinAction {
        let Some(&(bi, ci)) = self.rows().get(self.sel) else {
            return OrderWinAction::None;
        };
        let step = |i: usize| if down { Some(i + 1) } else { i.checked_sub(1) };
        let moved = match ci {
            None => step(bi).is_some_and(|to| {
                let ok = move_item(&mut self.blocks, bi, to);
                if ok {
                    self.select_block(to);
                }
                ok
            }),
            Some(_) if !self.spec.is_some_and(|sp| sp.child_move) => false,
            Some(c) => step(c).is_some_and(|to| {
                let ok = move_item(&mut self.blocks[bi].2, c, to);
                if ok {
                    self.select_child(bi, to);
                }
                ok
            }),
        };
        if moved {
            self.changed()
        } else {
            OrderWinAction::None
        }
    }

    /// 지금 편집 중인 키(없으면 `None`).
    pub(crate) fn key(&self) -> Option<&'static str> {
        self.spec.map(|sp| sp.key)
    }

    pub(crate) fn selected(&self) -> usize {
        self.sel
    }

    pub(crate) fn select(&mut self, i: usize) {
        if i < self.rows().len() {
            self.sel = i;
            self.redraw();
        }
    }

    /// 기본값으로(빈 설정값).
    pub(crate) fn reset_all(&mut self) -> OrderWinAction {
        let Some(sp) = self.spec else {
            return OrderWinAction::None;
        };
        self.blocks = (sp.blocks)("");
        self.sel = self.sel.min(self.rows().len().saturating_sub(1));
        self.changed()
    }

    fn row_h(&self) -> i32 {
        (ROW_H * self.scale).round() as i32
    }

    /// 커서 y → 행(목록 밖 = `None`).
    fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.list.contains(Point { x, y }) {
            return None;
        }
        let i = ((y - self.list.y) / self.row_h().max(1)) as usize;
        (i < self.rows().len()).then_some(i)
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> OrderWinAction {
        match ev {
            WindowEvent::RedrawRequested => OrderWinAction::Paint,
            WindowEvent::CloseRequested => OrderWinAction::Close,
            WindowEvent::Resized(_) => {
                self.redraw();
                OrderWinAction::None
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
                OrderWinAction::None
            }
            WindowEvent::ModifiersChanged(m) => {
                self.primary = if cfg!(target_os = "macos") {
                    m.state().super_key()
                } else {
                    m.state().control_key()
                };
                OrderWinAction::None
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                // 끌기: 문턱을 넘으면 시작 · 커서가 있는 행으로 선택 행을 따라 옮긴다(바로 반영).
                if let Some(d) = &mut self.drag {
                    if !d.active && (y - d.press_y).abs() > DRAG_THRESHOLD {
                        d.active = true;
                    }
                    if d.active {
                        let rh = self.row_h().max(1);
                        let last = self.rows().len().saturating_sub(1) as i32;
                        let to = ((y - self.list.y) / rh).clamp(0, last) as usize;
                        if self.move_sel_to_row(to) {
                            return self.changed();
                        }
                        return OrderWinAction::None;
                    }
                }
                let mut inv = Invalidations::default();
                let mv = InputEvent::MouseMove { x, y };
                self.up.on_event(&mv, &mut inv);
                self.down.on_event(&mv, &mut inv);
                self.reset.on_event(&mv, &mut inv);
                if !inv.is_empty() {
                    self.redraw();
                }
                OrderWinAction::None
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.cursor;
                let p = Point { x, y };
                let up = *state == ElementState::Released;
                if up {
                    self.drag = None;
                } else if let Some(i) = self.row_at(x, y) {
                    self.sel = i;
                    self.redraw();
                    // 체크 칸(행 왼쪽 · 자식은 들여쓴 자리) = 표시 전환 · 그 밖 = 선택 + 끌기 준비.
                    let indent = if self.rows()[i].1.is_some() {
                        INDENT
                    } else {
                        0.0
                    };
                    let check_right =
                        self.list.x + ((PAD + indent + CHECK + 8.0) * self.scale).round() as i32;
                    if x < check_right {
                        return self.toggle(i);
                    }
                    self.drag = Some(Drag {
                        press_y: y,
                        active: false,
                        before: self.blocks.clone(),
                    });
                    return OrderWinAction::None;
                }
                let ev = if up {
                    InputEvent::MouseUp { x, y }
                } else {
                    InputEvent::MouseDown {
                        x,
                        y,
                        shift: false,
                        primary: false,
                    }
                };
                let mut inv = Invalidations::default();
                // 마우스 라우팅 규칙: 눌림은 커서 아래 컨트롤에만 · 뗌은 늘(눌린 상태를 풀게).
                for b in [&mut self.up, &mut self.down, &mut self.reset] {
                    if up || b.bounds().contains(p) {
                        b.on_event(&ev, &mut inv);
                    }
                }
                if self.up.take_clicked() {
                    return self.move_sel(false);
                }
                if self.down.take_clicked() {
                    return self.move_sel(true);
                }
                if self.reset.take_clicked() {
                    return self.reset_all();
                }
                if !inv.is_empty() {
                    self.redraw();
                }
                OrderWinAction::None
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => {
                        // 끌기 중 = 끌기 전으로 되돌림 · 아니면 닫기.
                        if let Some(d) = self.drag.take() {
                            if d.active && d.before != self.blocks {
                                self.blocks = d.before;
                                return self.changed();
                            }
                            return OrderWinAction::None;
                        }
                        OrderWinAction::Close
                    }
                    Key::Named(NamedKey::ArrowUp) if self.primary => self.move_sel(false),
                    Key::Named(NamedKey::ArrowDown) if self.primary => self.move_sel(true),
                    Key::Named(NamedKey::ArrowUp) => {
                        self.select(self.sel.saturating_sub(1));
                        OrderWinAction::None
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        self.select(self.sel + 1);
                        OrderWinAction::None
                    }
                    Key::Named(NamedKey::Space) => self.toggle(self.sel),
                    _ => OrderWinAction::None,
                }
            }
            _ => OrderWinAction::None,
        }
    }

    pub(crate) fn paint(&mut self, font: &Font, th: &Theme, ui_px: f32) {
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
            let pad = px(PAD);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            // 안내 한 줄(조작법).
            let hint_h = px(24.0);
            let ty = dc.text_center_y(pad, hint_h);
            // 맥의 이동 단축키는 ⌘+↑↓(`primary`) — 안내 글의 수정키 이름도 맞춘다.
            let hint = if cfg!(target_os = "macos") {
                t(Msg::OrdHint).replace("Ctrl", "⌘")
            } else {
                t(Msg::OrdHint).to_string()
            };
            dc.text(
                pad,
                ty,
                Rect::new(pad, pad, wi - pad * 2, hint_h),
                &hint,
                th.text_dim,
            );
            // 목록(왼쪽) + 버튼 열(오른쪽).
            let side_w = px(SIDE_W);
            let top = pad + hint_h + px(4.0);
            let row_h = self.row_h();
            let list = Rect::new(pad, top, wi - pad * 3 - side_w, hi - top - pad);
            self.list = Rect::new(
                list.x,
                list.y,
                list.w,
                (row_h * self.rows().len() as i32).min(list.h),
            );
            dc.fill_rect(list, th.field_bg);
            dc.stroke_round_rect(list, 0, th.border, 1.0);
            let check = px(CHECK);
            let rows = self.rows();
            for (i, (bi, ci)) in rows.iter().copied().enumerate() {
                let r = Rect::new(list.x, list.y + row_h * i as i32, list.w, row_h);
                if r.bottom() > list.bottom() {
                    break;
                }
                if i == self.sel {
                    dc.fill_rect(r, th.sel_bg);
                }
                let block = &self.blocks[bi];
                let (item, vis) = match ci {
                    None => (None, block.1),
                    Some(c) => (Some(block.2[c].0.as_str()), block.2[c].1),
                };
                // 체크를 못 바꾸는 행 = 비활성 모양: 잠긴 블록 · 숨긴 그룹의 자식.
                let fixed = match ci {
                    None => self.block_locked(bi),
                    Some(_) => !block.1,
                };
                let indent = if ci.is_some() { px(INDENT) } else { 0 };
                let cb = Rect::new(r.x + pad + indent, r.y + (row_h - check) / 2, check, check);
                nexa_ctl::controls::draw_checkbox_glyph(&mut dc, th, cb, vis, !fixed);
                let label = match self.spec {
                    Some(sp) => without_shortcut(t((sp.label)(&block.0, item))),
                    None => item.unwrap_or(block.0.as_str()),
                };
                // 그룹 행(자식이 있는 블록) = 굵게.
                dc.select_font(FontSlot::Base, ci.is_none() && !block.2.is_empty());
                let lx = cb.right() + px(8.0);
                let ty = dc.text_center_y(r.y, row_h);
                // 흐리게 = 실제로 안 보이는 칸(자기 체크가 꺼졌거나 그룹이 숨겨짐).
                let shown = vis && block.1;
                dc.text(
                    lx,
                    ty,
                    Rect::new(lx, r.y, r.right() - lx - px(4.0), row_h),
                    label,
                    if shown { th.text } else { th.text_dim },
                );
            }
            dc.select_font(FontSlot::Base, false);
            let _ = th_txt;
            // 버튼 열: ▲ ▼ 위 · 기본값 아래.
            let bx = wi - pad - side_w;
            let bh = px(BTN_H);
            let mut inv = Invalidations::default();
            self.up.set_bounds(Rect::new(bx, top, side_w, bh), &mut inv);
            self.down
                .set_bounds(Rect::new(bx, top + bh + px(6.0), side_w, bh), &mut inv);
            self.reset
                .set_bounds(Rect::new(bx, hi - pad - bh, side_w, bh), &mut inv);
            // 연타가 뜻인 버튼(한 칸씩 여러 번).
            self.up.set_rapid(true);
            self.down.set_rapid(true);
            self.up.paint(&mut dc, th);
            self.down.paint(&mut dc, th);
            self.reset.paint(&mut dc, th);
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> OrderSpec {
        OrderSpec {
            key: crate::statusbar::KEY,
            title: Msg::WinStatusLayout,
            blocks: crate::statusbar::blocks,
            to_setting: crate::statusbar::to_setting,
            label: crate::statusbar::label,
            locked: crate::statusbar::LOCKED,
            child_move: true,
        }
    }

    fn value(a: OrderWinAction) -> Option<String> {
        match a {
            OrderWinAction::Changed { value, .. } => Some(value),
            _ => None,
        }
    }

    fn row_of(w: &OrderWin, id: &str) -> usize {
        w.dump()
            .lines()
            .skip(1)
            .position(|l| {
                l.trim_start_matches('>')
                    .trim()
                    .ends_with(&format!("] {id}"))
            })
            .expect("row")
    }

    #[test]
    fn without_shortcut_strips_only_trailing_parens() {
        assert_eq!(
            without_shortcut("Run statement (Ctrl+Enter)"),
            "Run statement"
        );
        assert_eq!(without_shortcut("Save as"), "Save as");
        assert_eq!(
            without_shortcut("Position / selection"),
            "Position / selection"
        );
        assert_eq!(without_shortcut("a (b) c"), "a (b) c");
    }

    #[test]
    fn move_item_bounds() {
        let mut v = vec![1, 2, 3];
        assert!(move_item(&mut v, 0, 2));
        assert_eq!(v, [2, 3, 1]);
        assert!(!move_item(&mut v, 1, 1));
        assert!(!move_item(&mut v, 0, 3));
        assert!(!move_item(&mut v, 3, 0));
        assert_eq!(v, [2, 3, 1]);
    }

    /// 창 없이 모델 조작: 그룹 통째 이동 · 자식은 그룹 안에서만 · 표시 전환 · 잠금 · 기본값.
    #[test]
    fn groups_move_whole_children_stay_inside() {
        let mut w = OrderWin::new();
        w.set(spec(), "");
        assert!(w
            .dump()
            .starts_with("key statusbar.layout\n>[x] tx\n [x] state\n   [x] large\n"));
        // 맨 위 블록을 위로 = 변화 없음.
        assert!(value(w.move_sel(false)).is_none());
        // 그룹 `format`을 위로 한 칸 = `project` 그룹과 통째 자리 바꿈(자식 4개가 같이 간다).
        w.select(row_of(&w, "format"));
        let v = value(w.move_sel(false)).expect("changed");
        assert!(
            v.contains("format:1[enc:1,eol:1,indent:1,syntax:1]|project:1[autosave:1,git:1]"),
            "{v}"
        );
        assert_eq!(w.selected(), row_of(&w, "format"), "선택이 그룹을 따라간다");
        // 자식 `syntax`를 위로 두 번 = 그룹 안에서만 · 첫째 자식에서 더 위로는 못 간다(그룹 밖으로 안 나감).
        w.select(row_of(&w, "syntax"));
        for _ in 0..3 {
            let _ = w.move_sel(false);
        }
        assert!(
            value(w.move_sel(false)).is_none(),
            "그룹의 첫 자리에서 멈춘다"
        );
        let v = (spec().to_setting)(&w.blocks);
        assert!(v.contains("format:1[syntax:1,enc:1,eol:1,indent:1]"), "{v}");
        // 자식 표시 끄기 → 그룹 숨김 → 숨긴 그룹의 자식은 못 바꾼다(체크는 보존).
        let v = value(w.toggle(row_of(&w, "eol"))).expect("changed");
        assert!(v.contains("eol:0"), "{v}");
        let v = value(w.toggle(row_of(&w, "format"))).expect("changed");
        assert!(v.contains("format:0[syntax:1,enc:1,eol:0,indent:1]"), "{v}");
        assert!(value(w.toggle(row_of(&w, "enc"))).is_none());
        // 잠긴 블록(라이선스)은 못 끈다.
        assert!(value(w.toggle(row_of(&w, "license"))).is_none());
        // 끌기 규칙: 자식을 다른 그룹 위로 끌면 자기 그룹의 가까운 끝에서 멈춘다.
        w.select(row_of(&w, "syntax"));
        assert!(w.move_sel_to_row(row_of(&w, "license")));
        let v = (spec().to_setting)(&w.blocks);
        assert!(v.contains("[enc:1,eol:0,indent:1,syntax:1]"), "{v}");
        // 끌기: 그룹 행을 다른 그룹의 자식 위에 놓으면 그 그룹 자리로 통째.
        w.select(row_of(&w, "tx"));
        assert!(w.move_sel_to_row(row_of(&w, "rows")));
        let v = (spec().to_setting)(&w.blocks);
        assert!(
            v.starts_with("state:1[") && v.contains("|result:1[rows:1,time:1]|tx:1|"),
            "{v}"
        );
        // 기본값 = 빈 설정값.
        assert_eq!(value(w.reset_all()).as_deref(), Some(""));
    }
}
