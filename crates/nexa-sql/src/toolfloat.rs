//! **플로팅 툴바 창**(사용자 09-17 · Golden `ScriptRunToolbar` 창) — 도크에서 떼어 낸 그룹 하나를 담는 작은 OS 창.
//!
//! - 툴바 컨트롤은 [`nexa_ctl::ToolDock`]이 계속 **소유**한다. 이 창은 표면·이벤트 변환만 맡고, 그릴 때·이벤트 때마다
//!   호스트가 `ToolDock::bar_mut(group)`로 빌려 `set_bounds(클라이언트)` → `paint`/`on_event` 한다(상태는 도크 한 곳).
//! - 이동 = OS 타이틀바(`Moved` → 좌표를 도크 배치에 저장) · 크기 고정 · 닫기(×) = 도크로 복귀.
//! - ★ 왼쪽 **손잡이**(점 2열 · T-134 ⓐ · 사용자 11-21/22)를 잡고 끌면 창이 따라오고, 주 창의 툴바 위에서 놓으면 그 자리에
//!   붙는다 — OS 제목줄 이동은 "놓은 순간"이 앱에 오지 않아 직접 끈다(창 좌표를 못 얻는 Wayland에서는 동작하지 않는다).
//! - 메인 창의 소유 창(`winfocus::owned_by`) — 작업표시줄 항목 없음 · 항상 메인 위 · 함께 최소화.

use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::Rect;
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Toolbar, Widget};
use nexa_gfx::{Font, Surface};
use std::rc::Rc;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

/// 창이 호스트에 넘기는 일.
pub(crate) enum FloatAction {
    None,
    /// 다시 그려 달라(호스트가 도크의 툴바를 빌려 [`ToolFloatWin::paint`]).
    Paint,
    /// 툴바에 줄 입력(클라이언트 좌표).
    Input(InputEvent),
    /// 창이 움직였다(화면 물리 px) — 배치 저장.
    Moved(i32, i32),
    /// × — 도크로 복귀.
    Close,
    /// 손잡이로 끄는 중(커서의 화면 물리 px · 좌표를 못 얻으면 `None`).
    Drag(Option<(i32, i32)>),
    /// 손잡이를 놓았다(커서의 화면 물리 px).
    Drop(Option<(i32, i32)>),
}

/// 손잡이 폭(논리 px).
pub(crate) const GRIP_W: f32 = 12.0;

pub(crate) struct ToolFloatWin {
    /// 담고 있는 그룹 id.
    pub group: String,
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    /// 손잡이를 잡은 자리(클라이언트 px) — 끄는 동안만.
    grab: Option<(i32, i32)>,
}

impl ToolFloatWin {
    /// 그룹 창 열기 — `pos` = 화면 물리 px(없으면 OS 기본) · `size` = 논리 px(툴바 권장 폭·높이).
    pub(crate) fn open(
        el: &ActiveEventLoop,
        group: &str,
        title: &str,
        theme: Option<winit::window::Theme>,
        pos: Option<(i32, i32)>,
        size: (f32, f32),
        owner: Option<&Window>,
    ) -> Option<Self> {
        let mut attrs = Window::default_attributes()
            .with_title(title)
            .with_theme(theme)
            .with_resizable(false)
            // 닫기만(Golden 플로팅 툴바처럼 최소화/최대화 없음).
            .with_enabled_buttons(winit::window::WindowButtons::CLOSE)
            .with_inner_size(winit::dpi::LogicalSize::new(
                size.0.max(40.0) + GRIP_W,
                size.1.max(20.0),
            ));
        if let Some((x, y)) = pos {
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(x, y));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let win = el
            .create_window(crate::winfocus::attrs_headless(attrs))
            .ok()?;
        let win = Rc::new(win);
        let scale = win.scale_factor() as f32;
        let mut me = ToolFloatWin {
            group: group.to_string(),
            window: None,
            surface: None,
            scale,
            cursor: (-1, -1),
            grab: None,
        };
        me.surface = crate::present::Presenter::new(win.clone()).ok();
        me.window = Some(win);
        me.redraw();
        Some(me)
    }

    pub(crate) fn is(&self, id: WindowId) -> bool {
        self.window.as_ref().is_some_and(|w| w.id() == id)
    }

    pub(crate) fn scale(&self) -> f32 {
        self.scale
    }

    /// 클라이언트 영역(툴바 bounds).
    pub(crate) fn client(&self) -> Rect {
        let (w, h) = self
            .window
            .as_ref()
            .map(|w| (w.inner_size().width as i32, w.inner_size().height as i32))
            .unwrap_or((0, 0));
        let g = self.grip_w();
        Rect::new(g, 0, (w - g).max(0), h)
    }

    fn grip_w(&self) -> i32 {
        (GRIP_W * self.scale).round() as i32
    }

    /// 커서의 화면 좌표(클라이언트 원점 + 커서).
    fn cursor_screen(&self) -> Option<(i32, i32)> {
        let o = self.window.as_ref()?.inner_position().ok()?;
        Some((o.x + self.cursor.0, o.y + self.cursor.1))
    }

    /// 현재 창 좌표(화면 물리 px).
    pub(crate) fn position(&self) -> Option<(i32, i32)> {
        let p = self.window.as_ref()?.outer_position().ok()?;
        Some((p.x, p.y))
    }

    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    pub(crate) fn close(&mut self) {
        self.surface = None;
        self.window = None;
    }

    /// winit 사건 → 호스트 동작. 마우스는 좌클릭·이동만(우클릭 메뉴는 도크 쪽 규약과 같이 호스트가 열 수 있게 Input으로).
    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> FloatAction {
        match ev {
            WindowEvent::CloseRequested => FloatAction::Close,
            WindowEvent::RedrawRequested | WindowEvent::Resized(_) => FloatAction::Paint,
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                FloatAction::Paint
            }
            WindowEvent::Moved(p) => FloatAction::Moved(p.x, p.y),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                if let Some((gx, gy)) = self.grab {
                    // 잡은 점이 커서 아래에 머물도록 창을 옮긴다(옮긴 뒤 커서의 클라이언트 좌표는 다시 잡은 점 근처).
                    if let Some(w) = &self.window {
                        if let Ok(o) = w.outer_position() {
                            w.set_outer_position(winit::dpi::PhysicalPosition::new(
                                o.x + self.cursor.0 - gx,
                                o.y + self.cursor.1 - gy,
                            ));
                        }
                    }
                    return FloatAction::Drag(self.cursor_screen());
                }
                let (x, y) = self.cursor;
                FloatAction::Input(InputEvent::MouseMove { x, y })
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = (-1, -1);
                FloatAction::Input(InputEvent::MouseMove { x: -1, y: -1 })
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (x, y) = self.cursor;
                match (button, state) {
                    (MouseButton::Left, ElementState::Pressed) if x >= 0 && x < self.grip_w() => {
                        self.grab = Some((x, y));
                        FloatAction::None
                    }
                    (MouseButton::Left, ElementState::Released) if self.grab.is_some() => {
                        self.grab = None;
                        FloatAction::Drop(self.cursor_screen())
                    }
                    (MouseButton::Left, ElementState::Pressed) => {
                        FloatAction::Input(InputEvent::MouseDown {
                            x,
                            y,
                            shift: false,
                            primary: false,
                        })
                    }
                    (MouseButton::Left, ElementState::Released) => {
                        FloatAction::Input(InputEvent::MouseUp { x, y })
                    }
                    (MouseButton::Right, ElementState::Pressed) => {
                        FloatAction::Input(InputEvent::RightDown { x, y })
                    }
                    _ => FloatAction::None,
                }
            }
            _ => FloatAction::None,
        }
    }

    /// 도크의 툴바를 빌려 이 창에 그린다(bounds = 클라이언트 · 배율 = 이 창).
    pub(crate) fn paint(&mut self, bar: &mut Toolbar, font: &Font, th: &Theme) {
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
        let (wi, hi) = (size.width as i32, size.height as i32);
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let mut dc = RasterCtx::new(&mut gfx, font, self.scale);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.chrome_bg);
            let mut inv = Invalidations::default();
            bar.set_scale(self.scale);
            let g = self.grip_w();
            // 손잡이 = 도크 그립과 같은 점 2열.
            let dot = ((2.0 * self.scale).round() as i32).max(1);
            let span = 4 * dot * 2 - dot;
            let (x0, y0) = ((g - dot * 3) / 2, (hi - span) / 2);
            for r in 0..4 {
                for c in 0..2 {
                    dc.fill_rect(
                        Rect::new(x0 + c * dot * 2, y0 + r * dot * 2, dot, dot),
                        th.text_dim,
                    );
                }
            }
            bar.set_bounds(Rect::new(g, 0, (wi - g).max(0), hi), &mut inv);
            bar.paint(&mut dc, th);
            bar.paint_tooltip(&mut dc, th);
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
