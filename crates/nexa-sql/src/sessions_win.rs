//! **세션 창**(사용자 09-18 "트랜잭션 로그 창처럼 세션 창도 별도로") — 모덜리스 별도 창에 전체 세션을 **서버별로 모아** 표로 보인다.
//! 열 = 서버 · 세션(● 활성 공유 / 공유 / 🔌 탭 제목) · 상태(연결됨 · 작업 중 · 끊김 · 오프라인) · 유휴 · 대기 문장 · 탭.
//! 행 우클릭 = 활성 연결로 / 다시 접속 / 접속 해제 / 탭으로 · 더블클릭 = 탭으로(전용) 또는 활성 연결로(공유) · 빈 곳 우클릭 = 모두 해제.
//!
//! 데이터는 호스트가 그릴 때 [`SessRow`] 목록으로 만들어 준다(세션 상태의 단일 원천 = `App`의 `Sess` · 창은 보기만).

use nexa_ctl::controls::ctxmenu::{ContextMenu, CtxItem};
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::InputEvent;
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, tf, Msg};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// 표의 한 줄(호스트가 만든다).
#[derive(Clone, Debug)]
pub(crate) struct SessRow {
    pub id: u64,
    /// 서버 묶음 이름(접속 설명 · 같은 서버 = 같은 글).
    pub server: String,
    pub shared: bool,
    pub active: bool,
    /// 전용/개별 세션의 주인 탭(id, 제목).
    pub tab: Option<(u64, String)>,
    pub connected: bool,
    pub broken: bool,
    pub busy: bool,
    pub idle_secs: u64,
    pub pending: usize,
}

pub(crate) enum SessWinAction {
    None,
    Paint,
    Activate(u64),
    Reconnect(u64),
    Disconnect(u64),
    DisconnectAll,
    GoTab(u64),
}

pub(crate) struct SessionsWin {
    window: Option<Rc<Window>>,
    memo: crate::wingeom::Memo,
    last: Option<((i32, i32), (f64, f64))>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    scroll: i32,
    row_h: i32,
    hover: Option<usize>,
    table: Rect,
    menu: ContextMenu,
    /// 마지막 페인트의 표시 줄(서버 머리줄 = None · 세션 줄 = Some(id, shared, tab)).
    rows: Vec<Option<(u64, bool, Option<u64>)>>,
    menu_row: Option<(u64, bool, Option<u64>)>,
    last_click: Option<(usize, Instant)>,
}

/// 열 폭(논리 px · 서버 열 = 나머지).
const COLS: [(Msg, i32); 6] = [
    (Msg::SessColServer, 0),
    (Msg::SessColSession, 170),
    (Msg::SessColState, 110),
    (Msg::SessColIdle, 80),
    (Msg::SessColPending, 70),
    (Msg::SessColTab, 160),
];

impl SessionsWin {
    pub(crate) fn new() -> Self {
        SessionsWin {
            window: None,
            memo: crate::wingeom::Memo::default(),
            last: None,
            surface: None,
            scale: 1.0,
            cursor: (-1, -1),
            scroll: 0,
            row_h: 22,
            hover: None,
            table: Rect::default(),
            menu: ContextMenu::new(),
            rows: Vec::new(),
            menu_row: None,
            last_click: None,
        }
    }

    /// 열기(모덜리스 · 메인 소유 창) — 트랜잭션 로그 창과 같은 규칙.
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
        let same = self.memo.on_same_monitor(owner);
        let (lw, lh) = same.and_then(|(_, s)| s).unwrap_or((860.0, 360.0));
        let mut attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinSessions)))
            .with_theme(theme)
            .with_inner_size(winit::dpi::LogicalSize::new(lw, lh));
        if let Some(((x, y), _)) = same {
            attrs = attrs.with_position(crate::wingeom::logical(x, y));
        } else if let Some((x, y, w)) = near {
            attrs =
                attrs.with_position(winit::dpi::PhysicalPosition::new(x + w as i32 + 8, y + 40));
        }
        let attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        // 기억한 위치는 프레임 기준으로 다시 놓는다(macOS 제목 표시줄 드리프트 방지 · wingeom::place_outer).
        if let Some(((x, y), _)) = same {
            crate::wingeom::place_outer(&win, Some((x, y)));
        }
        // 화면 밖으로 나가지 않게(메인 창 오른쪽 기본 위치 · 해상도가 바뀐 뒤의 기억 위치).
        crate::wingeom::keep_on_screen(&win, owner);
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        self.window = Some(win);
        self.redraw();
    }

    pub(crate) fn close(&mut self) {
        if let Some(w) = &self.window {
            if let Some(p) = crate::wingeom::outer_pos(w) {
                self.last = Some((p, crate::wingeom::logical_size(w)));
            }
        }
        self.surface = None;
        self.window = None;
    }

    pub(crate) fn set_memo(&mut self, m: crate::wingeom::Memo) {
        self.memo = m;
    }

    pub(crate) fn take_last(&mut self) -> Option<((i32, i32), (f64, f64))> {
        self.last.take()
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

    fn row_at(&self, p: Point) -> Option<usize> {
        if !self.table.contains(p) || self.row_h <= 0 {
            return None;
        }
        let r = ((p.y - self.table.y + self.scroll) / self.row_h) as usize;
        (r < self.rows.len()).then_some(r)
    }

    fn open_menu(&mut self, row: Option<(u64, bool, Option<u64>)>) {
        let (x, y) = self.cursor;
        let mut items = Vec::new();
        if let Some((_, shared, tab)) = row {
            if shared {
                items.push(CtxItem::item("use", t(Msg::MnSessActivate)));
            }
            if tab.is_some() {
                items.push(CtxItem::item("tab", t(Msg::MnSessGoTab)));
            }
            items.push(CtxItem::item("again", t(Msg::MnSessReconnect)));
            items.push(CtxItem::item("drop", t(Msg::MnDisconnect)));
            items.push(CtxItem::Separator);
        }
        items.push(CtxItem::item("drop_all", t(Msg::MnSessDisconnectAll)));
        self.menu_row = row;
        let host = self.table;
        self.menu.set_scale(self.scale);
        self.menu
            .open_at(x, y, items, host, (240.0 * self.scale) as i32);
        self.redraw();
    }

    /// winit 사건 → 호스트 동작.
    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> SessWinAction {
        if self.menu.is_open() {
            let (x, y) = self.cursor;
            let me = match ev {
                WindowEvent::CursorMoved { position, .. } => {
                    self.cursor = (position.x as i32, position.y as i32);
                    Some(InputEvent::MouseMove {
                        x: position.x as i32,
                        y: position.y as i32,
                    })
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => Some(InputEvent::MouseDown {
                    x,
                    y,
                    shift: false,
                    primary: false,
                }),
                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => Some(InputEvent::MouseUp { x, y }),
                WindowEvent::KeyboardInput { event: kev, .. }
                    if kev.state == ElementState::Pressed
                        && matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) =>
                {
                    Some(InputEvent::Key {
                        key: nexa_ctl::Key::Escape,
                        shift: false,
                        primary: false,
                    })
                }
                WindowEvent::RedrawRequested => return SessWinAction::Paint,
                _ => None,
            };
            if let Some(e) = me {
                let consumed = self.menu.on_event(&e);
                self.redraw();
                if let Some(id) = self.menu.take_picked() {
                    let row = self.menu_row.take();
                    return match (id.as_str(), row) {
                        ("use", Some((sid, _, _))) => SessWinAction::Activate(sid),
                        ("again", Some((sid, _, _))) => SessWinAction::Reconnect(sid),
                        ("drop", Some((sid, _, _))) => SessWinAction::Disconnect(sid),
                        ("tab", Some((_, _, Some(tab)))) => SessWinAction::GoTab(tab),
                        ("drop_all", _) => SessWinAction::DisconnectAll,
                        _ => SessWinAction::None,
                    };
                }
                if consumed {
                    return SessWinAction::None;
                }
            }
        }
        match ev {
            WindowEvent::CloseRequested => self.close(),
            WindowEvent::RedrawRequested => return SessWinAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let e = crate::input::wheel_event(delta, false);
                if let InputEvent::Wheel { delta: px } = e {
                    self.scroll = (self.scroll - px / 3).max(0);
                    self.redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let over = self.row_at(Point { x, y });
                if over != self.hover {
                    self.hover = over;
                    self.redraw();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                let row = self.hover.and_then(|r| self.rows.get(r).copied().flatten());
                self.open_menu(row);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                // 더블클릭(400ms) = 전용이면 그 탭으로 · 공유면 활성 연결로.
                if let Some(r) = self.hover {
                    let now = Instant::now();
                    let dbl = self.last_click.is_some_and(|(lr, at)| {
                        lr == r && now.duration_since(at).as_millis() < 400
                    });
                    self.last_click = Some((r, now));
                    if dbl {
                        self.last_click = None;
                        if let Some(Some((sid, shared, tab))) = self.rows.get(r).copied() {
                            return match tab {
                                Some(t) => SessWinAction::GoTab(t),
                                None if shared => SessWinAction::Activate(sid),
                                None => SessWinAction::None,
                            };
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                if matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::Escape)) {
                    self.close();
                }
            }
            _ => {}
        }
        SessWinAction::None
    }

    /// 그리기 — 세션 줄은 호스트가 만든 것(서버별로 모아 머리줄 + 세션 줄).
    pub(crate) fn paint(&mut self, rows_in: &[SessRow], font: &Font, th: &Theme, ui_px: f32) {
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
        let s = self.scale;
        let (wi, hi) = (size.width as i32, size.height as i32);
        let px = |v: f32| (v * s).round() as i32;
        {
            let mut gfx = Surface::new(&mut buf, size.width as usize, size.height as usize);
            let slot = |size: f32| SlotFont {
                size,
                bold: false,
                italic: false,
            };
            let prefs = FontPrefs {
                base: slot(ui_px),
                status: slot(ui_px),
                ..FontPrefs::default()
            };
            let mut dc = RasterCtx::new(&mut gfx, font, s).with_fonts(prefs);
            dc.fill_rect(Rect::new(0, 0, wi, hi), th.panel_bg);
            dc.select_font(FontSlot::Base, false);
            let th_txt = dc.text_height();
            let pad = px(8.0);
            let foot_h = th_txt + px(8.0);
            let table = Rect::new(pad, pad, wi - pad * 2, (hi - pad * 3 - foot_h).max(0));
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
            let server_w = (table.w - fixed - px(2.0)).max(px(160.0));
            let widths: Vec<i32> = COLS
                .iter()
                .map(|(_, w)| if *w == 0 { server_w } else { px(*w as f32) })
                .collect();
            // 헤더
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
            // 서버별 묶기(입력 순서 유지).
            let mut groups: Vec<(String, Vec<&SessRow>)> = Vec::new();
            for r in rows_in {
                match groups.iter_mut().find(|(g, _)| *g == r.server) {
                    Some((_, v)) => v.push(r),
                    None => groups.push((r.server.clone(), vec![r])),
                }
            }
            // 표시 줄: 서버 머리줄 + 세션 줄.
            let mut lines: Vec<(Option<&SessRow>, String)> = Vec::new();
            for (server, members) in &groups {
                lines.push((None, server.clone()));
                for r in members {
                    lines.push((Some(r), String::new()));
                }
            }
            self.rows = lines
                .iter()
                .map(|(r, _)| r.map(|r| (r.id, r.shared, r.tab.as_ref().map(|(id, _)| *id))))
                .collect();
            let body = Rect::new(
                table.x + 1,
                hdr.bottom() + 1,
                table.w - 2,
                (table.bottom() - 1 - hdr.bottom() - 1).max(0),
            );
            self.table = body;
            let max_scroll = (lines.len() as i32 * row_h - body.h).max(0);
            self.scroll = self.scroll.clamp(0, max_scroll);
            let first = (self.scroll / row_h.max(1)) as usize;
            let mut y = body.y - (self.scroll % row_h.max(1));
            for (ri, (row, server)) in lines.iter().enumerate().skip(first) {
                if y >= body.bottom() {
                    break;
                }
                let r = Rect::new(body.x, y, body.w, row_h);
                let clip = Rect::new(
                    r.x,
                    r.y.max(body.y),
                    r.w,
                    r.h.min(body.bottom() - r.y.max(body.y)),
                );
                let ty = y + (row_h - th_txt) / 2;
                match row {
                    None => {
                        // 서버 머리줄(흐린 배경 · 굵은 느낌은 색으로).
                        dc.fill_rect_alpha(clip, th.text, 0.06);
                        dc.text(body.x + px(6.0), ty, clip, server, th.text);
                    }
                    Some(sr) => {
                        if self.hover == Some(ri) {
                            dc.fill_rect_alpha(clip, th.text, 0.06);
                        }
                        if sr.broken {
                            dc.fill_rect_alpha(clip, th.danger, 0.10);
                        } else if sr.active {
                            dc.fill_rect_alpha(clip, th.accent, 0.08);
                        }
                        let session = match &sr.tab {
                            Some((_, title)) => format!("🔌 [{title}]"),
                            None if sr.active => format!("●  {}", t(Msg::MnSessSharedOne)),
                            None => format!("    {}", t(Msg::MnSessSharedOne)),
                        };
                        let (state, color) = if sr.broken {
                            (t(Msg::StSessBrokenTag).to_string(), th.danger)
                        } else if sr.busy {
                            (t(Msg::SessStateBusy).to_string(), th.accent)
                        } else if sr.connected {
                            (t(Msg::SessStateConnected).to_string(), th.text)
                        } else {
                            (t(Msg::ExpOffline).to_string(), th.text_dim)
                        };
                        let idle = if sr.connected {
                            let m = sr.idle_secs / 60;
                            if m >= 60 {
                                format!("{}h {}m", m / 60, m % 60)
                            } else {
                                format!("{m}m {}s", sr.idle_secs % 60)
                            }
                        } else {
                            String::new()
                        };
                        let cells: [String; 6] = [
                            String::new(),
                            session,
                            state,
                            idle,
                            if sr.pending > 0 {
                                sr.pending.to_string()
                            } else {
                                String::new()
                            },
                            sr.tab.as_ref().map_or(String::new(), |(_, t)| t.clone()),
                        ];
                        let mut cx = body.x;
                        for (i, w) in widths.iter().enumerate() {
                            let cell = Rect::new(cx, y, *w, row_h).intersection(&clip);
                            let c = if i == 2 { color } else { th.text };
                            dc.text(cx + px(6.0), ty, cell, &cells[i], c);
                            cx += w;
                        }
                    }
                }
                y += row_h;
            }
            if lines.is_empty() {
                dc.text(
                    body.x + px(10.0),
                    body.y + px(10.0),
                    body,
                    t(Msg::MnSessNoneConnected),
                    th.text_dim,
                );
            }
            // 푸터: 세션 수 · 서버 수 · 안내.
            let info = tf(
                Msg::SessFooter,
                &[&rows_in.len().to_string(), &groups.len().to_string()],
            );
            let fy = hi - pad - foot_h;
            dc.text(
                pad,
                fy + (foot_h - th_txt) / 2,
                Rect::new(0, fy, wi, foot_h),
                &info,
                th.text_dim,
            );
            let hint = t(Msg::SessHint);
            let hw = dc.text_width(hint);
            dc.text(
                wi - pad - hw,
                fy + (foot_h - th_txt) / 2,
                Rect::new(0, fy, wi, foot_h),
                hint,
                th.text_dim,
            );
            self.menu.paint(&mut dc, th);
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
