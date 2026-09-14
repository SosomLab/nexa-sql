//! 접속 패널(기본 UI · [docs/22 §1](../../../docs/22-driver-extensions.md) DBeaver 폼 + Golden 로그인 리스트의 축소판).
//!
//! 사용자 09-14: *"UI는 기본만 · DB 종류·접속 정보 설정 → 연결 버튼 → 접속 상태 확인"*.
//! 핵심 로직은 전부 공용 층 — 스펙 조립 = [`ConnectSpec::from_parts`] · 테스트 = `nsql_run::test_connection` ·
//! 저장 = `nsql_vault` — CLI `nsql conn add/test`와 **같은 함수**를 부른다(패널은 값을 모아 [`PanelAction`]으로 내보낼 뿐).
//!
//! 구성(세로 폼): 프로필 콤보(Golden 로그인 리스트 자리) · DB 종류 · 호스트 · 포트 · 데이터베이스/서비스(파일 기반 방언은 파일/DSN) ·
//! 사용자 · 비밀번호(마스킹) · 비밀번호 저장 · 프로필 이름 · [Test] [Connect/Disconnect] [Save] · 상태줄(● 색 + 문구).

use nexa_ctl::controls::ComboControl;
use nexa_ctl::draw::{DrawCtx, FontSlot};
use nexa_ctl::geom::{Point, Rect};
use nexa_ctl::theme::Theme;
use nexa_ctl::{
    Button, Checkbox, Combo, ComboItem, Control, InputEvent, Invalidations, Key, TextBox, Widget,
};
use nsql_core::Dialect;
use nsql_i18n::{t, tf, Msg};
use nsql_script::ConnectSpec;

/// 패널이 호스트에 요청하는 일(I/O는 전부 호스트/워커 몫).
#[derive(Debug)]
pub(crate) enum PanelAction {
    Connect(ConnectSpec),
    Test(ConnectSpec),
    Disconnect,
    Save {
        name: String,
        spec: ConnectSpec,
    },
    /// 프로필 콤보에서 이름을 골랐다 — 호스트가 저장소에서 스펙을 읽어 [`ConnectPanel::fill`]로 채운다.
    LoadProfile(String),
}

/// 접속 상태(상태줄 색·문구의 단일 원천).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConnState {
    Idle,
    Connecting,
    Connected(String),
    Failed(String),
    Testing,
    TestOk(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Host,
    Port,
    Database,
    User,
    Password,
    Name,
}

const FIELDS: [Field; 6] = [
    Field::Name,
    Field::Host,
    Field::Port,
    Field::Database,
    Field::User,
    Field::Password,
];

pub(crate) struct ConnectPanel {
    bounds: Rect,
    scale: f32,
    focused: bool,
    dialect: Combo,
    host: TextBox,
    port: TextBox,
    database: TextBox,
    user: TextBox,
    password: TextBox,
    name: TextBox,
    save_pw: Checkbox,
    test_btn: Button,
    connect_btn: Button,
    save_btn: Button,
    field_focus: Option<Field>,
    /// 방언 콤보가 바꾼 기본 포트(사용자가 손대지 않은 포트 칸만 따라간다).
    auto_port: Option<u16>,
    pub(crate) state: ConnState,
    connected: bool,
}

fn digits_only(c: char) -> bool {
    c.is_ascii_digit()
}

impl ConnectPanel {
    /// `available` = 이 빌드에 드라이버가 있는 방언. 콤보는 **전 방언**을 보이고 없는 것은 "(no driver)"를 붙인다 —
    /// 저장된 프로필(예: PostgreSQL · 드라이버는 M4)을 그대로 불러와 보이게(09-14).
    pub(crate) fn new(available: Vec<Dialect>) -> Self {
        let first = available.first().copied().unwrap_or(Dialect::Sqlite);
        let mut p = ConnectPanel {
            bounds: Rect::new(0, 0, 0, 0),
            scale: 1.0,
            focused: false,
            dialect: Combo::new(
                Dialect::ALL
                    .iter()
                    .map(|d| {
                        let label = if available.contains(d) {
                            d.display_name().to_string()
                        } else {
                            format!("{} {}", d.display_name(), t(Msg::ValNoDriver))
                        };
                        ComboItem::new(d.to_string(), label)
                    })
                    .collect(),
                Dialect::ALL.iter().position(|d| *d == first).unwrap_or(0),
            ),
            host: TextBox::new(t(Msg::PhHost)),
            port: TextBox::new(""),
            database: TextBox::new(t(Msg::PhDatabase)),
            user: TextBox::new(t(Msg::PhUser)),
            password: TextBox::new(t(Msg::PhPassword)),
            name: TextBox::new(t(Msg::PhProfileName)),
            save_pw: Checkbox::new(t(Msg::LblSavePassword), true),
            test_btn: Button::new(t(Msg::BtnTest)),
            connect_btn: Button::new(t(Msg::BtnConnect)),
            save_btn: Button::new(t(Msg::BtnSave)),
            field_focus: None,
            auto_port: None,
            state: ConnState::Idle,
            connected: false,
        };
        p.port.set_char_filter(Some(digits_only));
        p.port.set_max_chars(5);
        p.password.set_masked(true);
        p.apply_dialect(first);
        p
    }

    /// 언어 전환 — 라벨·placeholder 재생성(TextBox는 본문 보존 재생성).
    pub(crate) fn relabel(&mut self) {
        self.save_pw = Checkbox::new(t(Msg::LblSavePassword), self.save_pw.is_checked());
        self.test_btn.set_label(t(Msg::BtnTest));
        self.save_btn.set_label(t(Msg::BtnSave));
        self.update_connect_label();
        let rebuild = |tb: &mut TextBox, ph: &str, masked: bool| {
            let text = tb.text();
            let mut n = TextBox::new(ph).with_text(&text);
            n.set_masked(masked);
            *tb = n;
        };
        let db_ph = self.database_placeholder();
        rebuild(&mut self.host, t(Msg::PhHost), false);
        rebuild(&mut self.database, db_ph, false);
        rebuild(&mut self.user, t(Msg::PhUser), false);
        rebuild(&mut self.password, t(Msg::PhPassword), true);
        rebuild(&mut self.name, t(Msg::PhProfileName), false);
        self.layout_inner();
    }

    fn selected_dialect(&self) -> Dialect {
        Dialect::from_name(&self.dialect.selected_value()).unwrap_or(Dialect::Sqlite)
    }

    fn database_placeholder(&self) -> &'static str {
        match self.selected_dialect() {
            Dialect::Sqlite => t(Msg::PhSqliteFile),
            Dialect::Odbc => t(Msg::PhDsn),
            Dialect::Oracle => t(Msg::PhService),
            _ => t(Msg::PhDatabase),
        }
    }

    /// 방언 변경 — 기본 포트 반영 · 파일 기반이면 호스트/포트 숨김.
    fn apply_dialect(&mut self, d: Dialect) {
        let cur: Option<u16> = self.port.text().trim().parse().ok();
        if cur.is_none() || cur == self.auto_port {
            self.auto_port = d.default_port();
            self.port
                .set_text(&d.default_port().map(|p| p.to_string()).unwrap_or_default());
        }
        let text = self.database.text();
        self.database = TextBox::new(self.database_placeholder()).with_text(&text);
        self.layout_inner();
    }

    /// 저장소에서 읽은 스펙으로 폼을 채운다.
    pub(crate) fn fill(&mut self, name: &str, spec: &ConnectSpec) {
        if let Some(d) = spec.dialect {
            self.dialect.select_value(&d.to_string());
            self.auto_port = d.default_port();
        }
        self.host.set_text(spec.host.as_deref().unwrap_or(""));
        self.port
            .set_text(&spec.port.map(|p| p.to_string()).unwrap_or_default());
        self.database
            .set_text(spec.database.as_deref().unwrap_or(""));
        self.user.set_text(spec.user.as_deref().unwrap_or(""));
        self.password
            .set_text(spec.password.as_deref().unwrap_or(""));
        self.save_pw.set_checked(spec.password.is_some());
        self.name.set_text(name);
        let d = self.selected_dialect();
        self.apply_dialect(d);
    }

    /// 폼 → 스펙(GUI·CLI 공용 검증). 실패 문구는 상태줄로.
    fn build_spec(&self) -> Result<ConnectSpec, String> {
        let port = self.port.text().trim().parse::<u16>().ok();
        ConnectSpec::from_parts(
            self.selected_dialect(),
            &self.host.text(),
            port,
            &self.database.text(),
            &self.user.text(),
            &self.password.text(),
        )
    }

    /// 새 프로필 — 폼 비우기(DB 종류는 유지).
    pub(crate) fn clear(&mut self) {
        for tb in [
            &mut self.host,
            &mut self.port,
            &mut self.database,
            &mut self.user,
            &mut self.password,
            &mut self.name,
        ] {
            tb.set_text("");
        }
        self.save_pw.set_checked(false);
        let d = self.selected_dialect();
        self.apply_dialect(d);
    }

    pub(crate) fn is_connected(&self) -> bool {
        self.connected
    }

    pub(crate) fn state_ref(&self) -> &ConnState {
        &self.state
    }

    pub(crate) fn set_state(&mut self, s: ConnState) {
        self.connected = matches!(s, ConnState::Connected(_));
        self.state = s;
        self.update_connect_label();
    }

    fn update_connect_label(&mut self) {
        self.connect_btn.set_label(if self.connected {
            t(Msg::BtnDisconnect)
        } else {
            t(Msg::BtnConnect)
        });
    }

    // ── 레이아웃

    pub(crate) fn bounds(&self) -> Rect {
        self.bounds
    }

    fn s(&self, v: f32) -> i32 {
        (v * self.scale).round() as i32
    }

    pub(crate) fn set_bounds(&mut self, b: Rect, scale: f32) {
        self.bounds = b;
        self.scale = scale;
        for c in [
            &mut self.host,
            &mut self.port,
            &mut self.database,
            &mut self.user,
            &mut self.password,
            &mut self.name,
        ] {
            c.set_scale(scale);
        }
        self.dialect.set_scale(scale);
        self.save_pw.set_scale(scale);
        self.test_btn.set_scale(scale);
        self.connect_btn.set_scale(scale);
        self.save_btn.set_scale(scale);
        self.layout_inner();
    }

    fn file_based(&self) -> bool {
        self.selected_dialect().is_file_based()
    }

    /// 라벨 위 · 필드 아래의 세로 폼. 반환: 상태줄 y.
    fn layout_inner(&mut self) {
        let mut inv = Invalidations::default();
        let b = self.bounds;
        let pad = self.s(10.0);
        let label_h = self.s(16.0);
        let field_h = self.s(26.0);
        let gap = self.s(8.0);
        let x = b.x + pad;
        let w = b.w - pad * 2;
        let mut y = b.y + pad;
        // 라벨 한 줄 + 컨트롤 한 줄. `inv`는 인자로 받아 캡처 충돌을 피한다.
        let place = |ctl: &mut dyn Widget, inv: &mut Invalidations, y: &mut i32, h: i32| {
            *y += label_h;
            ctl.set_bounds(Rect::new(x, *y, w, h), inv);
            *y += h + gap;
        };
        // 접속 명칭(프로필 이름)이 맨 위 — 필수(사용자 09-14).
        place(&mut self.name, &mut inv, &mut y, field_h);
        place(&mut self.dialect, &mut inv, &mut y, field_h);
        let off = Rect::new(0, 0, 0, 0);
        if self.file_based() {
            self.host.set_bounds(off, &mut inv);
            self.port.set_bounds(off, &mut inv);
        } else {
            // 호스트 + 포트 한 줄(포트 폭 고정).
            y += label_h;
            let port_w = self.s(72.0);
            self.host
                .set_bounds(Rect::new(x, y, w - port_w - gap, field_h), &mut inv);
            self.port
                .set_bounds(Rect::new(x + w - port_w, y, port_w, field_h), &mut inv);
            y += field_h + gap;
        }
        place(&mut self.database, &mut inv, &mut y, field_h);
        place(&mut self.user, &mut inv, &mut y, field_h);
        place(&mut self.password, &mut inv, &mut y, field_h);
        self.save_pw
            .set_bounds(Rect::new(x, y, w, self.s(22.0)), &mut inv);
        y += self.s(22.0) + gap;
        // 버튼 3개 한 줄
        let bw = (w - gap * 2) / 3;
        let bh = self.s(28.0);
        self.test_btn.set_bounds(Rect::new(x, y, bw, bh), &mut inv);
        self.connect_btn
            .set_bounds(Rect::new(x + bw + gap, y, bw, bh), &mut inv);
        self.save_btn.set_bounds(
            Rect::new(x + (bw + gap) * 2, y, w - (bw + gap) * 2, bh),
            &mut inv,
        );
    }

    /// 상태줄 자리(버튼 아래).
    fn status_rect(&self) -> Rect {
        let b = self.test_btn.bounds();
        let pad = self.s(10.0);
        Rect::new(
            self.bounds.x + pad,
            b.bottom() + self.s(10.0),
            self.bounds.w - pad * 2,
            self.s(48.0),
        )
    }

    // ── 포커스

    pub(crate) fn set_focused(&mut self, on: bool) {
        self.focused = on;
        if on && self.field_focus.is_none() {
            self.field_focus = Some(Field::Name);
        }
        self.sync_focus();
    }

    fn textbox(&mut self, f: Field) -> &mut TextBox {
        match f {
            Field::Host => &mut self.host,
            Field::Port => &mut self.port,
            Field::Database => &mut self.database,
            Field::User => &mut self.user,
            Field::Password => &mut self.password,
            Field::Name => &mut self.name,
        }
    }

    fn sync_focus(&mut self) {
        let cur = if self.focused { self.field_focus } else { None };
        for f in FIELDS {
            let on = cur == Some(f);
            self.textbox(f).set_focused(on);
        }
    }

    /// 호스트의 IME·클립보드 라우팅 지점.
    pub(crate) fn focused_textbox(&mut self) -> Option<&mut TextBox> {
        if !self.focused {
            return None;
        }
        let f = self.field_focus?;
        Some(self.textbox(f))
    }

    pub(crate) fn focus_host(&mut self) {
        self.focused = true;
        self.field_focus = Some(if self.file_based() {
            Field::Database
        } else {
            Field::Host
        });
        self.sync_focus();
    }

    fn visible_fields(&self) -> Vec<Field> {
        FIELDS
            .iter()
            .copied()
            .filter(|f| !(self.file_based() && matches!(f, Field::Host | Field::Port)))
            .collect()
    }

    fn focus_next(&mut self, back: bool) {
        let vis = self.visible_fields();
        let i = self
            .field_focus
            .and_then(|f| vis.iter().position(|x| *x == f))
            .unwrap_or(0);
        let n = vis.len();
        let j = if back { (i + n - 1) % n } else { (i + 1) % n };
        self.field_focus = Some(vis[j]);
        self.sync_focus();
    }

    pub(crate) fn popup_open(&self) -> bool {
        self.dialect.is_open()
    }

    // ── 이벤트

    /// 패널 안의 이벤트. 반환 = 호스트가 할 일.
    pub(crate) fn route(
        &mut self,
        ev: &InputEvent,
        inv: &mut Invalidations,
    ) -> Option<PanelAction> {
        // 열린 콤보가 있으면 그것이 먼저(모달).
        if self.dialect.is_open() {
            self.dialect.on_event(ev, inv);
            return self.after_combo();
        }
        if let InputEvent::MouseDown { x, y, .. } = *ev {
            let p = Point { x, y };
            let mut hit = None;
            for f in self.visible_fields() {
                if self.textbox(f).bounds().contains(p) {
                    hit = Some(f);
                }
            }
            if hit.is_some() {
                self.field_focus = hit;
                self.focused = true;
                self.sync_focus();
            } else if self.dialect.bounds().contains(p) || self.save_pw.bounds().contains(p) {
                self.field_focus = None;
                self.sync_focus();
            }
        }
        // 마우스 사건은 버튼·콤보·체크박스 전부에.
        if matches!(
            ev,
            InputEvent::MouseDown { .. }
                | InputEvent::MouseUp { .. }
                | InputEvent::MouseMove { .. }
        ) {
            self.dialect.on_event(ev, inv);
            self.save_pw.on_event(ev, inv);
            self.test_btn.on_event(ev, inv);
            self.connect_btn.on_event(ev, inv);
            self.save_btn.on_event(ev, inv);
            if let Some(a) = self.after_combo() {
                return Some(a);
            }
            if self.test_btn.take_clicked() {
                return self.act_test();
            }
            if self.connect_btn.take_clicked() {
                return self.act_connect();
            }
            if self.save_btn.take_clicked() {
                return self.act_save();
            }
        }
        // 키 · 문자 → 포커스 텍스트박스
        match ev {
            InputEvent::Key {
                key: Key::Enter, ..
            } if self.field_focus.is_some() => return self.act_connect(),
            InputEvent::Char { c: '\t', .. } => {
                self.focus_next(false);
                inv.push(self.bounds);
                return None;
            }
            _ => {}
        }
        if let Some(f) = self.field_focus {
            let tb = self.textbox(f);
            tb.on_event(ev, inv);
            if f == Field::Port {
                // 사용자가 포트를 직접 고치면 자동 포트 추종을 끈다.
                let cur: Option<u16> = self.port.text().trim().parse().ok();
                if cur != self.auto_port {
                    self.auto_port = None;
                }
            }
        }
        None
    }

    fn after_combo(&mut self) -> Option<PanelAction> {
        if let Some(v) = self.dialect.take_changed() {
            if let Some(d) = Dialect::from_name(&v) {
                self.apply_dialect(d);
            }
        }
        None
    }

    fn act_test(&mut self) -> Option<PanelAction> {
        match self.build_spec() {
            Ok(spec) => {
                self.set_state(ConnState::Testing);
                Some(PanelAction::Test(spec))
            }
            Err(e) => {
                self.set_state(ConnState::Failed(e));
                None
            }
        }
    }

    /// 메뉴·툴바의 "접속/해제" — 버튼 클릭과 같은 경로.
    pub(crate) fn connect_action(&mut self) -> Option<PanelAction> {
        self.act_connect()
    }

    fn act_connect(&mut self) -> Option<PanelAction> {
        if self.connected {
            return Some(PanelAction::Disconnect);
        }
        match self.build_spec() {
            Ok(spec) => {
                self.set_state(ConnState::Connecting);
                Some(PanelAction::Connect(spec))
            }
            Err(e) => {
                self.set_state(ConnState::Failed(e));
                None
            }
        }
    }

    fn act_save(&mut self) -> Option<PanelAction> {
        let name = self.name.text().trim().to_string();
        if !nsql_vault::is_profile_name(&name) {
            self.set_state(ConnState::Failed(t(Msg::ErrProfileName).into()));
            return None;
        }
        match self.build_spec() {
            Ok(mut spec) => {
                if !self.save_pw.is_checked() {
                    spec.password = None;
                }
                Some(PanelAction::Save { name, spec })
            }
            Err(e) => {
                self.set_state(ConnState::Failed(e));
                None
            }
        }
    }

    // ── 그리기

    pub(crate) fn paint(&self, dc: &mut dyn DrawCtx, th: &Theme) {
        let b = self.bounds;
        dc.fill_rect(b, th.panel_bg);
        dc.fill_rect(Rect::new(b.right() - 1, b.y, 1, b.h), th.border);
        dc.select_font(FontSlot::Base, false);
        let label_h = self.s(16.0);
        let label = |dc: &mut dyn DrawCtx, r: Rect, text: &str| {
            if r.w == 0 {
                return;
            }
            dc.text(r.x, r.y - label_h + self.s(1.0), b, text, th.text_dim);
        };
        label(dc, self.name.bounds(), t(Msg::LblProfileName));
        label(dc, self.dialect.bounds(), t(Msg::LblDbType));
        label(dc, self.host.bounds(), t(Msg::LblHost));
        label(dc, self.port.bounds(), t(Msg::LblPort));
        let db_label = match self.selected_dialect() {
            Dialect::Sqlite => t(Msg::LblFile),
            Dialect::Odbc => t(Msg::LblDsn),
            Dialect::Oracle => t(Msg::LblService),
            _ => t(Msg::LblDatabase),
        };
        label(dc, self.database.bounds(), db_label);
        label(dc, self.user.bounds(), t(Msg::LblUser));
        label(dc, self.password.bounds(), t(Msg::LblPassword));
        label(dc, self.name.bounds(), t(Msg::LblProfileName));
        for tb in [
            &self.host,
            &self.port,
            &self.database,
            &self.user,
            &self.password,
            &self.name,
        ] {
            if tb.bounds().w > 0 {
                tb.paint(dc, th);
            }
        }
        self.save_pw.paint(dc, th);
        self.test_btn.paint(dc, th);
        self.connect_btn.paint(dc, th);
        self.save_btn.paint(dc, th);
        // 상태줄: ● + 문구(패널 폭 안에서 잘림)
        let sr = self.status_rect();
        let (color, text) = match &self.state {
            ConnState::Idle => (th.text_dim, t(Msg::StIdle).to_string()),
            ConnState::Connecting => (th.warn, t(Msg::StConnectingShort).to_string()),
            ConnState::Testing => (th.warn, t(Msg::StTesting).to_string()),
            ConnState::Connected(d) => (th.ok, tf(Msg::StConnectedShort, &[d])),
            ConnState::TestOk(d) => (th.ok, d.clone()),
            ConnState::Failed(e) => (th.danger, e.clone()),
        };
        let dot = self.s(8.0);
        dc.fill_ellipse(Rect::new(sr.x, sr.y + self.s(4.0), dot, dot), color);
        dc.text(sr.x + dot + self.s(6.0), sr.y, sr, &text, th.text);
        // 콤보는 팝업을 스스로 그린다 — 열린 것이 최상위가 되게 마지막에.
        self.dialect.paint(dc, th);
        for tb in [
            &self.host,
            &self.database,
            &self.user,
            &self.password,
            &self.name,
        ] {
            tb.paint_popup(dc, th);
        }
    }
}
