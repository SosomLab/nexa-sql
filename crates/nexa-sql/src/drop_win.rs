//! ★ 객체 삭제 확인 창(10-01 · 사용자 "탐색기 우클릭으로 객체 삭제 · 팝업 확인 · 모달 · 정보 표시 · 타임아웃 재확인 버튼").
//!
//! 모달(열려 있는 동안 모든 창 잠김 · `App::modal_open`) · 객체 정보 줄 + DROP 문 + 백업 상태 · **삭제 버튼 = 2단 타임아웃**(한 번 누르면
//! `N초 안에 다시` 상태로 무장 · 그 안에 다시 누르면 실행 · 지나면 해제 · 로그인 창 Delete와 같은 부품 `TimeoutButton`).
//! 흐름(호스트 `app/drop.rs`): Confirm → (백업) → Done | BackupFailed(백업 없이 삭제 = 다시 타임아웃 재확인) | Blocked(운영 접속 백업 실패 = 삭제 안 함).

use nexa_ctl::controls::ButtonTone;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, TimeoutButton, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use std::rc::Rc;
use std::time::Instant;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DropAction {
    None,
    Paint,
    /// 닫기(취소 · Esc · 끝난 뒤 닫기).
    Close,
    /// 삭제 재확인 발화(Confirm 단계).
    Proceed,
    /// 백업 실패 뒤 "백업 없이 삭제" 재확인 발화.
    ProceedNoBackup,
}

/// 창의 단계 — 버튼 라벨·활성과 안내 줄이 달라진다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DropPhase {
    /// 정보 + 삭제 버튼(백업 켬/끔 안내).
    Confirm,
    /// 백업 DDL을 가져와 쓰는 중(버튼 잠김).
    Backing,
    /// 백업 실패 — 다시 한 번 타임아웃 재확인 뒤 백업 없이 삭제.
    BackupFailed(String),
    /// 삭제하지 않음(운영 접속 백업 실패 · 세션 없음 등) — 닫기만.
    Blocked(String),
    /// 삭제 문장을 보냈다 — 닫기만(결과는 Output).
    Done(String),
}

pub(crate) struct DropWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    lines: Vec<(String, String)>,
    phase: DropPhase,
    btn_delete: Button,
    btn_cancel: Button,
    /// 무장된 삭제 버튼(타임아웃) · 시작 시각.
    arm: Option<(TimeoutButton, Instant)>,
    timeout_ms: u64,
    fit: bool,
}

impl DropWin {
    pub(crate) fn new() -> Self {
        let mut btn_delete = Button::new(t(Msg::DropBtnDelete));
        btn_delete.set_tone(ButtonTone::Danger);
        DropWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            lines: Vec::new(),
            phase: DropPhase::Confirm,
            btn_delete,
            btn_cancel: Button::new(t(Msg::DropBtnCancel)),
            arm: None,
            timeout_ms: 5000,
            fit: true,
        }
    }

    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        owner: Option<&Window>,
        lines: Vec<(String, String)>,
        timeout_ms: u64,
    ) {
        self.lines = lines;
        self.timeout_ms = timeout_ms.max(1000);
        self.phase = DropPhase::Confirm;
        self.arm = None;
        if let Some(w) = &self.window {
            crate::winfocus::focus(w);
            self.fit = true;
            self.redraw();
            return;
        }
        let size = winit::dpi::LogicalSize::new(600.0, 360.0);
        let attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::DropWinTitle)))
            .with_theme(theme)
            .with_resizable(false)
            .with_inner_size(size);
        let mut attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        attrs = crate::wingeom::centered_over(attrs, owner, size.width, 3);
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        crate::winfocus::focus(&win);
        self.window = Some(win);
        self.fit = true;
        self.redraw();
    }

    pub(crate) fn set_phase(&mut self, phase: DropPhase) {
        self.phase = phase;
        self.arm = None;
        self.btn_delete.clear_transient();
        self.fit = true;
        self.redraw();
    }

    /// 정보 줄을 덧붙인다(백업 경로 등).
    pub(crate) fn push_line(&mut self, k: impl Into<String>, v: impl Into<String>) {
        self.lines.push((k.into(), v.into()));
        self.fit = true;
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
        self.arm = None;
        self.btn_delete.clear_transient();
        self.btn_cancel.clear_transient();
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

    /// 자체 시험 덤프(`drop.dump:`): 단계 · 무장 여부 · 정보 줄.
    pub(crate) fn dump(&self) -> String {
        let mut s = format!(
            "open={} phase={:?} armed={}\n",
            self.window.is_some(),
            self.phase,
            self.arm.is_some()
        );
        for (k, v) in &self.lines {
            s.push_str(&format!("{k}|{v}\n"));
        }
        s
    }

    /// 무장 타임아웃 진행(호스트 틱) — 다시 그려야 하면 true · 만료 = 해제.
    pub(crate) fn tick(&mut self) -> bool {
        let Some((tb, epoch)) = self.arm.as_mut() else {
            return false;
        };
        let changed = tb.tick(epoch.elapsed().as_millis() as u64);
        if tb.fired_by_timeout() {
            let _ = tb.take_fired();
            self.arm = None;
            self.btn_delete.clear_transient();
            return true;
        }
        changed
    }

    pub(crate) fn armed(&self) -> bool {
        self.arm.is_some()
    }

    fn delete_enabled(&self) -> bool {
        matches!(self.phase, DropPhase::Confirm | DropPhase::BackupFailed(_))
    }

    fn arm_delete(&mut self) {
        let label = match self.phase {
            DropPhase::BackupFailed(_) => t(Msg::DropBtnNoBackup),
            _ => t(Msg::DropBtnDelete),
        };
        let mut tb = TimeoutButton::new(label, self.timeout_ms)
            .with_warn(true)
            .with_suffix(t(Msg::UnitSecShort))
            .with_show_remaining(true);
        let mut inv = Invalidations::default();
        tb.set_bounds(self.btn_delete.bounds(), &mut inv);
        tb.set_scale(self.scale);
        tb.start(0);
        tb.set_focused(true);
        self.btn_delete.clear_transient();
        self.arm = Some((tb, Instant::now()));
        self.redraw();
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> DropAction {
        let mut inv = Invalidations::default();
        match ev {
            WindowEvent::CloseRequested => return DropAction::Close,
            WindowEvent::RedrawRequested => return DropAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                self.btn_delete.clear_transient();
                self.btn_cancel.clear_transient();
                self.redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                match self.arm.as_mut() {
                    Some((tb, _)) => tb.on_event(&mv, &mut inv),
                    None => self.btn_delete.on_event(&mv, &mut inv),
                }
                self.btn_cancel.on_event(&mv, &mut inv);
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
                        shift: false,
                        primary: false,
                    },
                    ElementState::Released => InputEvent::MouseUp { x, y },
                };
                let up = matches!(e, InputEvent::MouseUp { .. });
                // 마우스 라우팅 규칙: 커서 아래 컨트롤에만(놓기는 전부 · 포커스 링 ≤ 1).
                if self.btn_cancel.bounds().contains(p) || up {
                    self.btn_cancel.on_event(&e, &mut inv);
                }
                if self.delete_enabled() && (self.btn_delete.bounds().contains(p) || up) {
                    match self.arm.as_mut() {
                        Some((tb, _)) => tb.on_event(&e, &mut inv),
                        None => self.btn_delete.on_event(&e, &mut inv),
                    }
                }
                self.btn_cancel.set_focused(false);
                self.redraw();
                if self.btn_cancel.take_clicked() {
                    return DropAction::Close;
                }
                if let Some((tb, _)) = self.arm.as_mut() {
                    if let Some(nexa_ctl::FiredBy::Click) = tb.take_fired() {
                        self.arm = None;
                        return match self.phase {
                            DropPhase::BackupFailed(_) => DropAction::ProceedNoBackup,
                            _ => DropAction::Proceed,
                        };
                    }
                } else if self.btn_delete.take_clicked() && self.delete_enabled() {
                    // 1단: 무장(타임아웃 안에 다시 누르면 실행).
                    self.arm_delete();
                }
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                if matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) {
                    return DropAction::Close;
                }
            }
            _ => {}
        }
        DropAction::None
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
        let inv = &mut Invalidations::default();
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let prefs = FontPrefs::with_base_status(ui_px);
            let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.panel_bg);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            let pad = px(16.0);
            let clip = Rect::new(0, 0, wi, hi);
            let mut y = pad;
            // 제목 줄(위험색) + 단계 안내.
            dc.text(pad, y, clip, t(Msg::DropWinTitle), th.danger);
            y += th_txt + px(8.0);
            let (note, color) = match &self.phase {
                DropPhase::Confirm => (t(Msg::DropNoteConfirm).to_string(), th.text_dim),
                DropPhase::Backing => (t(Msg::DropNoteBacking).to_string(), th.accent),
                DropPhase::BackupFailed(e) => {
                    (format!("{} — {e}", t(Msg::DropNoteBackupFailed)), th.danger)
                }
                DropPhase::Blocked(e) => (e.clone(), th.danger),
                DropPhase::Done(m) => (m.clone(), th.ok),
            };
            for line in note.split('\n') {
                let v = nexa_ctl::draw::ellipsize_middle(&mut dc, line, wi - pad * 2);
                dc.text(pad, y, clip, &v, color);
                y += th_txt + px(4.0);
            }
            y += px(6.0);
            let label_w = self
                .lines
                .iter()
                .filter(|(k, _)| !k.is_empty())
                .map(|(k, _)| dc.text_width(k))
                .max()
                .unwrap_or(0)
                + px(14.0);
            let row_h = th_txt + px(5.0);
            for (k, v) in &self.lines {
                if k.is_empty() {
                    let v = nexa_ctl::draw::ellipsize_middle(&mut dc, v, wi - pad * 2);
                    dc.text(pad, y, clip, &v, th.text_dim);
                } else {
                    dc.text(pad, y, clip, k, th.text_dim);
                    let vr = Rect::new(pad + label_w, y, wi - pad * 2 - label_w, row_h);
                    let v = nexa_ctl::draw::ellipsize_middle(&mut dc, v, vr.w);
                    dc.text(vr.x, y, vr, &v, th.text);
                }
                y += row_h;
            }
            let btn_h = th_txt + px(14.0);
            let btn_y = hi - pad - btn_h;
            if self.fit {
                self.fit = false;
                let need = y + px(12.0) + btn_h + pad;
                if need != hi {
                    let _ = win.request_inner_size(winit::dpi::PhysicalSize::new(
                        size.width,
                        need.max(px(200.0)) as u32,
                    ));
                }
            }
            // 버튼: [삭제(타임아웃)] [취소/닫기] — 왼쪽부터.
            let del_label = match self.phase {
                DropPhase::BackupFailed(_) => t(Msg::DropBtnNoBackup),
                _ => t(Msg::DropBtnDelete),
            };
            let cancel_label = if matches!(
                self.phase,
                DropPhase::Confirm | DropPhase::Backing | DropPhase::BackupFailed(_)
            ) {
                t(Msg::DropBtnCancel)
            } else {
                t(Msg::LicBtnClose)
            };
            let mut bx = pad;
            let del_w = (dc.text_width(del_label) + px(60.0)).max(px(150.0));
            let del_rect = Rect::new(bx, btn_y, del_w, btn_h);
            if self.delete_enabled() {
                match self.arm.as_mut() {
                    Some((tb, _)) => {
                        tb.set_scale(s);
                        tb.set_bounds(del_rect, inv);
                        tb.paint(&mut dc, th);
                    }
                    None => {
                        self.btn_delete.set_label(del_label);
                        self.btn_delete.set_scale(s);
                        self.btn_delete.set_bounds(del_rect, inv);
                        self.btn_delete.paint(&mut dc, th);
                    }
                }
                bx += del_w + px(8.0);
            }
            self.btn_cancel.set_label(cancel_label);
            self.btn_cancel.set_scale(s);
            let cw = (dc.text_width(cancel_label) + px(28.0)).max(px(84.0));
            self.btn_cancel
                .set_bounds(Rect::new(bx, btn_y, cw, btn_h), inv);
            self.btn_cancel.paint(&mut dc, th);
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
