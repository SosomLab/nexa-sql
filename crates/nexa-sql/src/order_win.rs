//! **순서/표시 편집 창**(사용자 10-04 "상태바 위치와 사용 여부를 설정 화면에서 · nexa-dir3 툴바 조정 화면 참고"): 설정 창의
//! [편집…]에서 여는 보조 창. 행 = 체크(표시) + 이름 · 선택한 행을 ▲▼ / Ctrl(⌘)+↑↓ / 끌기로 옮긴다 · Space = 표시 전환 ·
//! 잠긴 항목은 체크를 못 끈다 · Esc = 끌기 취소 → 닫기. 바꿀 때마다 [`OrderWinAction::Changed`]로 호스트에 알려 **즉시 적용·저장**
//! (확인/취소 없음 — nexa-dir3 DLG-073과 같은 규약). 무엇을 고치는지는 [`OrderSpec`] 어댑터가 정한다(지금 = 상태바 하나).

use nexa_ctl::controls::Button;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
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
    /// 설정값 → (항목 id, 표시) 목록.
    pub items: fn(&str) -> Vec<(String, bool)>,
    /// 목록 → 저장할 설정값(기본과 같으면 빈 문자열).
    pub to_setting: fn(&[(String, bool)]) -> String,
    pub label: fn(&str) -> Msg,
    /// 체크를 끌 수 없는 항목.
    pub locked: &'static [&'static str],
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
const DRAG_THRESHOLD: i32 = 5;

/// 끌기 — 누른 자리에서 [`DRAG_THRESHOLD`]를 넘으면 시작 · 시작할 때의 목록을 들고 있다가 Esc면 되돌린다.
struct Drag {
    press_y: i32,
    active: bool,
    before: Vec<(String, bool)>,
}

pub(crate) struct OrderWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    primary: bool,
    spec: Option<OrderSpec>,
    items: Vec<(String, bool)>,
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
            items: Vec::new(),
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
        self.items = (spec.items)(value);
        self.spec = Some(spec);
        self.sel = self.sel.min(self.items.len().saturating_sub(1));
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
        let rows = self.items.len().max(1) as f64;
        let h = f64::from(PAD) * 3.0 + 24.0 + rows * f64::from(ROW_H) + 2.0;
        let Some(o) = crate::winhost::open_window(
            el,
            crate::winhost::OpenSpec {
                title: format!("Nexa SQL — {}", t(spec.title)),
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

    /// 지금 내용(진단·시험 덤프) — `key` + 행마다 `[x] id` · 선택 행 앞에 `>`.
    pub(crate) fn dump(&self) -> String {
        let mut s = format!("key {}\n", self.spec.map_or("-", |sp| sp.key));
        for (i, (id, vis)) in self.items.iter().enumerate() {
            s.push_str(&format!(
                "{}[{}] {id}\n",
                if i == self.sel { ">" } else { " " },
                if *vis { "x" } else { " " }
            ));
        }
        s
    }

    fn changed(&self) -> OrderWinAction {
        self.redraw();
        match self.spec {
            Some(sp) => OrderWinAction::Changed {
                key: sp.key,
                value: (sp.to_setting)(&self.items),
            },
            None => OrderWinAction::None,
        }
    }

    fn locked(&self, i: usize) -> bool {
        match (self.spec, self.items.get(i)) {
            (Some(sp), Some((id, _))) => sp.locked.contains(&id.as_str()),
            _ => true,
        }
    }

    /// 표시 전환(잠긴 항목 = 거부).
    pub(crate) fn toggle(&mut self, i: usize) -> OrderWinAction {
        if self.locked(i) {
            return OrderWinAction::None;
        }
        self.items[i].1 = !self.items[i].1;
        self.changed()
    }

    /// 선택 행을 위/아래로 한 칸.
    pub(crate) fn move_sel(&mut self, down: bool) -> OrderWinAction {
        let to = if down {
            self.sel + 1
        } else {
            match self.sel.checked_sub(1) {
                Some(v) => v,
                None => return OrderWinAction::None,
            }
        };
        if move_item(&mut self.items, self.sel, to) {
            self.sel = to;
            self.changed()
        } else {
            OrderWinAction::None
        }
    }

    pub(crate) fn selected(&self) -> usize {
        self.sel
    }

    pub(crate) fn select(&mut self, i: usize) {
        if i < self.items.len() {
            self.sel = i;
            self.redraw();
        }
    }

    /// 기본값으로(빈 설정값).
    pub(crate) fn reset_all(&mut self) -> OrderWinAction {
        let Some(sp) = self.spec else {
            return OrderWinAction::None;
        };
        self.items = (sp.items)("");
        self.sel = self.sel.min(self.items.len().saturating_sub(1));
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
        (i < self.items.len()).then_some(i)
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
                        let last = self.items.len().saturating_sub(1) as i32;
                        let to = ((y - self.list.y) / rh).clamp(0, last) as usize;
                        if move_item(&mut self.items, self.sel, to) {
                            self.sel = to;
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
                    // 체크 칸(행 왼쪽) = 표시 전환 · 그 밖 = 선택 + 끌기 준비.
                    let check_right =
                        self.list.x + ((PAD + CHECK + 8.0) * self.scale).round() as i32;
                    if x < check_right {
                        return self.toggle(i);
                    }
                    self.drag = Some(Drag {
                        press_y: y,
                        active: false,
                        before: self.items.clone(),
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
                            if d.active && d.before != self.items {
                                self.items = d.before;
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
                (row_h * self.items.len() as i32).min(list.h),
            );
            dc.fill_rect(list, th.field_bg);
            dc.stroke_round_rect(list, 0, th.border, 1.0);
            let check = px(CHECK);
            for (i, (id, vis)) in self.items.iter().enumerate() {
                let r = Rect::new(list.x, list.y + row_h * i as i32, list.w, row_h);
                if r.bottom() > list.bottom() {
                    break;
                }
                if i == self.sel {
                    dc.fill_rect(r, th.sel_bg);
                }
                let locked = self.spec.is_some_and(|sp| sp.locked.contains(&id.as_str()));
                let cb = Rect::new(r.x + pad, r.y + (row_h - check) / 2, check, check);
                // 잠긴 항목 = 비활성 모양의 체크(끌 수 없음).
                nexa_ctl::controls::draw_checkbox_glyph(&mut dc, th, cb, *vis, !locked);
                let label = self.spec.map_or(id.as_str(), |sp| t((sp.label)(id)));
                let lx = cb.right() + px(8.0);
                let ty = dc.text_center_y(r.y, row_h);
                dc.text(
                    lx,
                    ty,
                    Rect::new(lx, r.y, r.right() - lx - px(4.0), row_h),
                    label,
                    if *vis { th.text } else { th.text_dim },
                );
            }
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
            items: crate::statusbar::items,
            to_setting: crate::statusbar::to_setting,
            label: crate::statusbar::label,
            locked: crate::statusbar::LOCKED,
        }
    }

    fn value(a: OrderWinAction) -> Option<String> {
        match a {
            OrderWinAction::Changed { value, .. } => Some(value),
            _ => None,
        }
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

    /// 창 없이 모델 조작: 이동 · 표시 전환 · 잠금 · 기본값 — 통지 값이 설정 문법으로 나온다.
    #[test]
    fn edit_model_reports_normalized_values() {
        let mut w = OrderWin::new();
        w.set(spec(), "");
        assert!(w.dump().starts_with("key statusbar.layout\n>[x] tx\n"));
        // 맨 위에서 위로 = 변화 없음.
        assert!(value(w.move_sel(false)).is_none());
        // tx를 한 칸 아래로.
        let v = value(w.move_sel(true)).expect("changed");
        assert!(v.starts_with("mem:1|tx:1|"), "{v}");
        assert_eq!(w.sel, 1);
        // 표시 끄기.
        let v = value(w.toggle(1)).expect("changed");
        assert!(v.contains("tx:0"), "{v}");
        // 잠긴 항목(라이선스)은 못 끈다.
        let lic = w
            .items
            .iter()
            .position(|(id, _)| id == "license")
            .expect("license");
        assert!(value(w.toggle(lic)).is_none());
        assert!(w.items[lic].1);
        // 맨 아래에서 아래로 = 변화 없음.
        w.select(w.items.len() - 1);
        assert!(value(w.move_sel(true)).is_none());
        // 기본값 = 빈 설정값.
        assert_eq!(value(w.reset_all()).as_deref(), Some(""));
        assert!(w.items.iter().all(|(_, v)| *v));
    }
}
