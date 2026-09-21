//! **변수 값 입력 창**(D-137 · docs/63 V3) — 실행 전에 값이 필요한 바인드(`:X`)와 치환 변수(`&v`)를 **실행당 한 번** 한 격자에서 받는다.
//! 워커는 답을 기다리며 멈춰 있고(세션 = 바쁨), 이 창이 닫히는 길은 셋: **Run**(값을 넣고 실행) · **Skip**(값 없이 그대로 = 종전
//! 동작) · **Cancel**/Esc/창 닫기(실행하지 않음). 값 해석은 러너 몫(빈 칸·`NULL` = NULL · 수 · `'글'` · 그 밖 = 글 그대로).
//!
//! 창 골격은 트랜잭션 로그 창과 같다(winit 창 + `Presenter` · 포커스 ≤ 1 · 마우스는 커서 아래 컨트롤에만 · Esc = 닫기).

use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::raster::RasterCtx;
use nexa_ctl::theme::{FontPrefs, SlotFont, Theme};
use nexa_ctl::{Button, Control, InputEvent, Invalidations, Key as CtlKey, TextBox, Widget};
use nexa_gfx::{Font, Surface};
use nsql_i18n::{t, Msg};
use nsql_script::{InputKind, InputNeed};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::event::{ElementState, Ime, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub(crate) enum InputWinAction {
    None,
    Paint,
    /// 값을 넣고 실행((종류, 이름, 친 글)).
    Run(Vec<(InputKind, String, String)>),
    /// 비밀번호 입력 창의 답 — 값은 [`nsql_core::Secret`](버려질 때 0으로 덮어쓴다) · 입력란은 이미 지웠다.
    Password(nsql_core::Secret),
    /// 값 없이 그대로 실행.
    Skip,
    /// 실행하지 않는다.
    Cancel,
}

struct Row {
    need: InputNeed,
    tb: TextBox,
}

pub(crate) struct InputWin {
    window: Option<Rc<Window>>,
    surface: Option<crate::present::Presenter>,
    scale: f32,
    cursor: (i32, i32),
    shift: bool,
    primary: bool,
    rows: Vec<Row>,
    focus: usize,
    run_btn: Button,
    skip_btn: Button,
    cancel_btn: Button,
    /// 답을 받을 세션(호스트가 기억해 둔다 · 다중 세션에서 섞이지 않게).
    pub sess: u64,
    /// **비밀번호 모드**(사용자 09-21 — 비밀번호 자리가 없는 접속 문자열): 물을 대상의 표시(가린 접속 문자열). 입력란 하나 ·
    /// 가림 · 복사 불가 · "건너뛰기" 없음 · 답은 `Password(Secret)` · 닫을 때 입력란을 0으로 덮어쓴다.
    password_for: Option<String>,
    /// 들고 있던 비밀번호가 거부돼 다시 묻는 것이다(안내 글이 달라진다).
    password_rejected: bool,
}

impl InputWin {
    pub(crate) fn new() -> Self {
        InputWin {
            window: None,
            surface: None,
            scale: 1.0,
            cursor: (0, 0),
            shift: false,
            primary: false,
            rows: Vec::new(),
            focus: 0,
            run_btn: Button::new(t(Msg::BtnRunInputs)),
            skip_btn: Button::new(t(Msg::BtnSkipInputs)),
            cancel_btn: Button::new(t(Msg::BtnCancel)),
            sess: 0,
            password_for: None,
            password_rejected: false,
        }
    }

    /// **비밀번호 한 번 묻기** — `target` = 가린 접속 문자열(누구의 비밀번호인지). 같은 창·같은 배선을 쓴다.
    pub(crate) fn open_password(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        owner: Option<&Window>,
        target: String,
        rejected: bool,
        sess: u64,
    ) {
        let need = InputNeed {
            kind: InputKind::Macro,
            name: "password".into(),
            line: 0,
            prompt: None,
            default: None,
            hide: true,
        };
        self.open(el, theme, owner, vec![need], sess);
        self.password_for = Some(target);
        self.password_rejected = rejected;
        self.run_btn.set_label(t(Msg::BtnConnect));
        if let Some(w) = &self.window {
            w.set_title(&format!("Nexa SQL — {}", t(Msg::WinPassword)));
        }
        self.redraw();
    }

    pub(crate) fn is_password(&self) -> bool {
        self.password_for.is_some()
    }

    /// ★ 모달인가(사용자 mac 09-21 "비밀번호 창은 최상위 모달 — Enter/버튼 전에는 다른 기능을 쓸 수 없게"): 비밀번호 모드일 때.
    /// 접속 창·파일 창과 같은 길(`App::sync_modal` + 사건 가드 + 맥 자식 창). 변수 입력 창까지 넓히려면 이 판정만 바꾼다.
    pub(crate) fn is_modal(&self) -> bool {
        self.is_open() && self.password_for.is_some()
    }

    /// 입력 격자를 연다(이미 열려 있으면 내용을 바꾼다).
    pub(crate) fn open(
        &mut self,
        el: &ActiveEventLoop,
        theme: Option<winit::window::Theme>,
        owner: Option<&Window>,
        needs: Vec<InputNeed>,
        sess: u64,
    ) {
        self.wipe_if_password();
        self.run_btn.set_label(t(Msg::BtnRunInputs));
        self.sess = sess;
        self.rows = needs
            .into_iter()
            .map(|need| {
                let mut tb = TextBox::new(match need.kind {
                    InputKind::Bind => "NULL",
                    InputKind::Macro => "",
                });
                // `ACCEPT`(T-153): DEFAULT = 미리 채운다 · HIDE = 가린다.
                if let Some(d) = &need.default {
                    tb.set_text(d);
                }
                tb.set_masked(need.hide);
                Row { tb, need }
            })
            .collect();
        self.focus = 0;
        self.sync_focus();
        if let Some(w) = &self.window {
            w.set_title(&format!("Nexa SQL — {}", t(Msg::WinInputs)));
            crate::winfocus::focus(w);
            self.redraw();
            return;
        }
        let n = self.rows.len().clamp(1, 12) as f64;
        let attrs = Window::default_attributes()
            .with_title(format!("Nexa SQL — {}", t(Msg::WinInputs)))
            .with_theme(theme)
            .with_resizable(true)
            .with_inner_size(winit::dpi::LogicalSize::new(520.0, 120.0 + n * 34.0));
        let mut attrs = crate::winfocus::owned_by(crate::icon::with_icon(attrs), owner);
        // 메인 창 가운데쯤(실행한 자리에서 눈이 멀리 가지 않게).
        if let Some(o) = owner {
            if let Ok(p) = o.outer_position() {
                let s = o.outer_size();
                attrs = attrs.with_position(winit::dpi::PhysicalPosition::new(
                    p.x + s.width as i32 / 2 - 300,
                    p.y + s.height as i32 / 3,
                ));
            }
        }
        let Ok(win) = el.create_window(attrs) else {
            return;
        };
        let win = Rc::new(win);
        self.scale = win.scale_factor() as f32;
        self.surface = crate::present::Presenter::new(win.clone()).ok();
        // 앱 조합 모드(T-139)면 IME를 붙이지 않는다 — raw 자모를 받아 상자가 직접 조합한다.
        win.set_ime_allowed(crate::input::system_ime());
        crate::winfocus::focus(&win);
        self.window = Some(win);
        self.redraw();
    }

    /// 비밀번호 모드였으면 입력란(본문·되돌리기 기록·조합 글)을 0으로 덮어쓰고 모드를 끝낸다.
    fn wipe_if_password(&mut self) {
        if self.password_for.take().is_some() {
            for r in &mut self.rows {
                r.tb.wipe();
            }
        }
    }

    /// Enter·실행 버튼 — 보통은 값 목록 · 비밀번호 모드는 `Secret`(입력란은 꺼내는 즉시 지운다).
    fn submit(&mut self) -> InputWinAction {
        if self.password_for.is_some() {
            let s = self
                .rows
                .first_mut()
                .map(|r| r.tb.take_secret_text())
                .unwrap_or_default();
            return InputWinAction::Password(nsql_core::Secret::new(s));
        }
        InputWinAction::Run(self.values())
    }

    pub(crate) fn close(&mut self) {
        self.wipe_if_password();
        self.surface = None;
        self.window = None;
        for r in &mut self.rows {
            r.tb.set_focused(false);
        }
        self.rows.clear();
        for b in [&mut self.run_btn, &mut self.skip_btn, &mut self.cancel_btn] {
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

    /// 포커스는 입력란 하나에만(포커스 규칙: 한 창에 링 ≤ 1).
    fn sync_focus(&mut self) {
        let f = self.focus;
        for (i, r) in self.rows.iter_mut().enumerate() {
            r.tb.set_focused(i == f);
        }
    }

    fn values(&self) -> Vec<(InputKind, String, String)> {
        self.rows
            .iter()
            .map(|r| (r.need.kind, r.need.name.clone(), r.tb.text()))
            .collect()
    }

    fn key_event(&self, kev: &winit::event::KeyEvent) -> Option<InputEvent> {
        let key = |k: CtlKey| InputEvent::Key {
            key: k,
            shift: self.shift,
            primary: self.primary,
        };
        Some(match kev.logical_key.as_ref() {
            Key::Named(NamedKey::ArrowLeft) => key(CtlKey::Left),
            Key::Named(NamedKey::ArrowRight) => key(CtlKey::Right),
            Key::Named(NamedKey::Home) => key(CtlKey::Home),
            Key::Named(NamedKey::End) => key(CtlKey::End),
            Key::Named(NamedKey::Delete) => key(CtlKey::Delete),
            Key::Named(NamedKey::Backspace) => InputEvent::Char {
                c: '\u{8}',
                now_ms: 0,
            },
            Key::Named(NamedKey::Space) => InputEvent::Char { c: ' ', now_ms: 0 },
            Key::Character(t) => {
                if self.primary {
                    return None;
                }
                let c = t.chars().next()?;
                if c.is_control() {
                    return None;
                }
                InputEvent::Char { c, now_ms: 0 }
            }
            _ => return None,
        })
    }

    pub(crate) fn handle(&mut self, ev: &WindowEvent) -> InputWinAction {
        let mut inv = Invalidations::default();
        match ev {
            WindowEvent::CloseRequested => return InputWinAction::Cancel,
            WindowEvent::RedrawRequested => return InputWinAction::Paint,
            WindowEvent::Resized(_) => self.redraw(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = *scale_factor as f32;
                self.redraw();
            }
            WindowEvent::Focused(false) => {
                for b in [&mut self.run_btn, &mut self.skip_btn, &mut self.cancel_btn] {
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
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as i32, position.y as i32);
                let (x, y) = self.cursor;
                let mv = InputEvent::MouseMove { x, y };
                for r in &mut self.rows {
                    r.tb.on_event(&mv, &mut inv);
                }
                for b in [&mut self.run_btn, &mut self.skip_btn, &mut self.cancel_btn] {
                    b.on_event(&mv, &mut inv);
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
                if !up {
                    if let Some(i) = self.rows.iter().position(|r| r.tb.bounds().contains(p)) {
                        self.focus = i;
                        self.sync_focus();
                    }
                }
                // 마우스 라우팅 규칙: 누름은 커서 아래 컨트롤에만 · 뗌은 전부(눌림 상태 해제).
                for r in &mut self.rows {
                    if up || r.tb.bounds().contains(p) {
                        r.tb.on_event(&e, &mut inv);
                    }
                }
                for b in [&mut self.run_btn, &mut self.skip_btn, &mut self.cancel_btn] {
                    if up || b.bounds().contains(p) {
                        b.on_event(&e, &mut inv);
                    }
                    // 버튼은 누르면 스스로 포커스를 켠다 — 이 창의 포커스는 입력란 것이다.
                    b.set_focused(false);
                }
                self.redraw();
                if self.run_btn.take_clicked() {
                    return self.submit();
                }
                if self.skip_btn.take_clicked() {
                    return InputWinAction::Skip;
                }
                if self.cancel_btn.take_clicked() {
                    return InputWinAction::Cancel;
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if let Some(r) = self.rows.get_mut(self.focus) {
                    r.tb.set_preedit("", &mut inv);
                    for c in text.chars().filter(|c| !c.is_control()) {
                        r.tb.on_event(&InputEvent::Char { c, now_ms: 0 }, &mut inv);
                    }
                }
                self.redraw();
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                if let Some(r) = self.rows.get_mut(self.focus) {
                    r.tb.set_preedit(text, &mut inv);
                }
                self.redraw();
            }
            WindowEvent::KeyboardInput { event: kev, .. } if kev.state == ElementState::Pressed => {
                match kev.logical_key.as_ref() {
                    Key::Named(NamedKey::Escape) => return InputWinAction::Cancel,
                    Key::Named(NamedKey::Enter) => return self.submit(),
                    Key::Named(NamedKey::Tab) | Key::Named(NamedKey::ArrowDown)
                        if !self.rows.is_empty() =>
                    {
                        let n = self.rows.len();
                        self.focus = if self.shift
                            && !matches!(kev.logical_key.as_ref(), Key::Named(NamedKey::ArrowDown))
                        {
                            (self.focus + n - 1) % n
                        } else {
                            (self.focus + 1) % n
                        };
                        self.sync_focus();
                        self.redraw();
                        return InputWinAction::None;
                    }
                    Key::Named(NamedKey::ArrowUp) if !self.rows.is_empty() => {
                        let n = self.rows.len();
                        self.focus = (self.focus + n - 1) % n;
                        self.sync_focus();
                        self.redraw();
                        return InputWinAction::None;
                    }
                    Key::Character(c) if self.primary && c.eq_ignore_ascii_case("v") => {
                        let secret = self.password_for.is_some();
                        if let (Some(mut text), Some(r)) =
                            (crate::clipboard::read_text(), self.rows.get_mut(self.focus))
                        {
                            // 한 줄 입력란 — 첫 줄만.
                            let mut line = text.lines().next().unwrap_or("").to_string();
                            r.tb.paste(&line, &mut inv);
                            // 비밀번호를 붙여 넣었으면 우리가 뜬 사본은 바로 지운다(클립보드 자체는 사용자의 것 — 건드리지 않는다).
                            if secret {
                                nsql_core::secret::wipe_string(&mut line);
                                nsql_core::secret::wipe_string(&mut text);
                            }
                        }
                        self.redraw();
                        return InputWinAction::None;
                    }
                    _ => {}
                }
                if let Some(e) = self.key_event(kev) {
                    if let Some(r) = self.rows.get_mut(self.focus) {
                        r.tb.on_event(&e, &mut inv);
                    }
                    self.redraw();
                }
            }
            _ => {}
        }
        InputWinAction::None
    }

    pub(crate) fn paint(&mut self, font: &Font, th: &Theme, ui_px: f32) {
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
            let pad = px(12.0);
            // 안내 한 줄(비밀번호 모드 = 누구의 비밀번호인지 + 저장하지 않는다는 약속).
            let hint = match &self.password_for {
                Some(target) if self.password_rejected => {
                    nsql_i18n::tf(Msg::PasswordHintRejected, &[target])
                }
                Some(target) => nsql_i18n::tf(Msg::PasswordHint, &[target]),
                None => t(Msg::InputsHint).to_string(),
            };
            dc.text(pad, pad, Rect::new(0, 0, wi, hi), &hint, th.text_dim);
            let row_h = th_txt + px(12.0);
            let gap = px(6.0);
            // 이름 열 폭 = 가장 긴 "접두 + 이름" · 종류 표시는 이름 뒤 흐린 글.
            // `ACCEPT … PROMPT 글`이면 그 글을(끝의 `:`·공백은 떼고) 이름 뒤에 붙인다.
            let pw_mode = self.password_for.is_some();
            let label = |r: &Row| {
                if pw_mode {
                    return t(Msg::LblPassword).to_string();
                }
                let base = match r.need.kind {
                    InputKind::Bind => format!(":{}", r.need.name),
                    InputKind::Macro => format!("&{}", r.need.name),
                };
                match r
                    .need
                    .prompt
                    .as_deref()
                    .map(|p| p.trim().trim_end_matches(':').trim())
                {
                    Some(p) if !p.is_empty() => format!("{base}  {p}"),
                    _ => base,
                }
            };
            let name_w = self
                .rows
                .iter()
                .map(|r| dc.text_width(&label(r)))
                .max()
                .unwrap_or(0)
                .clamp(px(80.0), wi / 2);
            let btn_h = th_txt + px(14.0);
            let btn_y = hi - pad - btn_h;
            let mut y = pad + th_txt + px(10.0);
            let inv = &mut Invalidations::default();
            for r in &mut self.rows {
                if y + row_h > btn_y - gap {
                    break; // 창보다 많은 줄은 그리지 않는다(창을 키우면 보인다 · 12줄까지는 열 때 맞춘다).
                }
                let text = label(r);
                dc.text(
                    pad,
                    y + (row_h - th_txt) / 2,
                    Rect::new(0, 0, wi, hi),
                    &text,
                    th.text,
                );
                let x = pad + name_w + px(10.0);
                r.tb.set_scale(s);
                r.tb.set_bounds(Rect::new(x, y, wi - x - pad, row_h), inv);
                r.tb.paint(&mut dc, th);
                y += row_h + gap;
            }
            // 버튼(오른쪽 정렬: Cancel · Skip · Run).
            let bw = px(96.0);
            let mut bx = wi - pad - bw;
            for (k, b) in [&mut self.run_btn, &mut self.skip_btn, &mut self.cancel_btn]
                .into_iter()
                .enumerate()
            {
                b.set_scale(s);
                // 비밀번호에는 "건너뛰기"가 없다(빈 비밀번호는 접속 문자열의 `user:@host`로 명시한다).
                if pw_mode && k == 1 {
                    b.set_bounds(Rect::default(), inv);
                    continue;
                }
                b.set_bounds(Rect::new(bx, btn_y, bw, btn_h), inv);
                b.paint(&mut dc, th);
                bx -= bw + px(8.0);
            }
        }
        let _ = buf.present();
        self.surface = Some(surface);
    }
}
