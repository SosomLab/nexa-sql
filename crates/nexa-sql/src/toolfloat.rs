//! **플로팅 툴바 창**(사용자 09-17 · Golden `ScriptRunToolbar` 창) — 도크에서 떼어 낸 그룹 하나를 담는 작은 OS 창.
//!
//! - 툴바 컨트롤은 [`nexa_ctl::ToolDock`]이 계속 **소유**한다. 이 창은 표면·이벤트 변환만 맡고, 그릴 때·이벤트 때마다
//!   호스트가 `ToolDock::bar_mut(group)`로 빌려 `set_bounds(클라이언트)` → `paint`/`on_event` 한다(상태는 도크 한 곳).
//! - 이동 = OS 타이틀바(`Moved` → 좌표를 도크 배치에 저장) · 크기 고정 · 닫기(×) = 도크로 복귀.
//! - 메인 창의 소유 창(`winfocus::owned_by`) — 작업표시줄 항목 없음 · 항상 메인 위 · 함께 최소화.

use nexa_ctl::draw::DrawCtx;
use nexa_ctl::geom::Rect;
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::Theme;
use nexa_ctl::{Control, InputEvent, Invalidations, Toolbar, Widget};
use nexa_gfx::{Font, Surface};
use std::num::NonZeroU32;
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
}

pub(crate) struct ToolFloatWin {
    /// 담고 있는 그룹 id.
    pub group: String,
    window: Option<Rc<Window>>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    scale: f32,
    cursor: (i32, i32),
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
                size.0.max(40.0),
                size.1.max(20.0),
            ));
        if let Some((x, y)) = pos {
            attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(x, y));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let win = el.create_window(attrs).ok()?;
        let win = Rc::new(win);
        let scale = win.scale_factor() as f32;
        let mut me = ToolFloatWin {
            group: group.to_string(),
            window: None,
            ctx: None,
            surface: None,
            scale,
            cursor: (-1, -1),
        };
        if let Ok(ctx) = softbuffer::Context::new(win.clone()) {
            if let Ok(s) = softbuffer::Surface::new(&ctx, win.clone()) {
                me.surface = Some(s);
            }
            me.ctx = Some(ctx);
        }
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
        Rect::new(0, 0, w, h)
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
        self.ctx = None;
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
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            self.surface = Some(surface);
            return;
        };
        if surface.resize(w, h).is_err() {
            self.surface = Some(surface);
            return;
        }
        let Ok(mut buf) = surface.buffer_mut() else {
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
            bar.set_bounds(Rect::new(0, 0, wi, hi), &mut inv);
            bar.paint(&mut dc, th);
            bar.paint_tooltip(&mut dc, th);
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
